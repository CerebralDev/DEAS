import { afterEach, expect, test, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { autoRefine, waitForWorkbenchJob, isImageIntent, stopWorkbench, runWorkbenchJob, readCurrentWorkbench, pollWorkbench, prepareDiagnosticJob, submitDiagnosticJob, readDiagnosticJobStatus, readCurrentDiagnosticJob, loadDiagnosticJobOutput, exportDiagnosticJobOutput } from './jobs';
import { compileRecipe, type DiagnosticImageIntentV1 } from './execution';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn(() => true) }));
afterEach(() => { vi.resetAllMocks(); vi.unstubAllGlobals(); });
const intent: DiagnosticImageIntentV1 = {
  recipeId: 'checkpoint-text-to-image-diagnostic', version: 1, positivePrompt: 'test', negativePrompt: '',
  checkpoint: 'observed.safetensors', seed: 0, width: 512, height: 512, steps: 20, cfg: 7,
  sampler: 'euler', scheduler: 'normal', outputPrefix: 'DEAS_diagnostic',
};
test('auto-refine forwards explicit consent, immutable settings, target order and the admission fence',async()=>{
 vi.mocked(isTauri).mockReturnValue(true);vi.mocked(invoke).mockResolvedValue({job:null,skipped:2});
 const settings={checkpoint:'SAM3/sam3.1.safetensors',targets:['eyes','feet','hands'],denoise:0.3,margin:32,threshold:0.5};
 expect(await autoRefine('http://127.0.0.1:9',intent,settings,7,'source-job')).toEqual({job:null,skipped:2});
 expect(invoke).toHaveBeenCalledWith('native_auto_refine',{baseUrl:'http://127.0.0.1:9',intent,settings,confirmed:true,admissionEpoch:7,sourceJobId:'source-job',masks:[]});
 vi.mocked(invoke).mockResolvedValue({state:'running'});await expect(autoRefine('http://127.0.0.1:9',intent,settings,7)).rejects.toThrow('Invalid native auto-refine result');
});
test('SAM jobs cannot be mistaken for image-generation intents or retried on unknown status',async()=>{
 const sam={recipeId:'sam3-mask',version:1,checkpoint:'sam.safetensors',inputImage:'in.png',width:512,height:512,outputPrefix:'mask',threshold:0.5,prompt:'eyes'} as const;
 expect(isImageIntent(sam)).toBe(false);expect(isImageIntent(intent)).toBe(true);
 const job={id:'sam-job',baseUrl:'http://127.0.0.1:9',promptId:'p',state:'unknown' as const,outputs:[],error:null,generationReady:false as const,intent:sam};
 await expect(waitForWorkbenchJob(job,()=>true)).rejects.toThrow('unknown');expect(invoke).not.toHaveBeenCalled();
});
// Mage-Flow: one encode node carries prompt, negative and latent (no EmptyLatentImage, no separate negative node).
test('Mage-Flow compiles the combined encode node', () => {
  const nodes = ['UNETLoader', 'CLIPLoader', 'VAELoader', 'KSampler', 'VAEDecode', 'SaveImage', 'TextEncodeMageFlowEdit'];
  const allow = { nodes, checkpoints: [intent.checkpoint], diffusionModels: ['mage_flow_int8_convrot.safetensors'], textEncoders: ['qwen3vl_4b_bf16.safetensors'], clipTypes: ['mage'], vaes: ['mage_flow_vae_bf16.safetensors'], samplers: ['euler'], schedulers: ['simple'] };
  const g = compileRecipe({ ...intent, checkpoint: 'mage_flow_int8_convrot.safetensors', steps: 30, cfg: 5, sampler: 'euler', scheduler: 'simple', diffusion: { textEncoder: 'qwen3vl_4b_bf16.safetensors', clipType: 'mage', vae: 'mage_flow_vae_bf16.safetensors', family: 'mage' } }, allow).graph;
  expect(g['2']).toEqual({ class_type: 'TextEncodeMageFlowEdit', inputs: { clip: ['C', 0], prompt: intent.positivePrompt, negative_prompt: intent.negativePrompt, width: 512, height: 512, batch_size: 1 } });
  expect(g['3']).toBeUndefined();
  expect(g['4']).toBeUndefined();
  expect(g['5'].inputs.positive).toEqual(['2', 0]);
  expect(g['5'].inputs.negative).toEqual(['2', 1]);
  expect(g['5'].inputs.latent_image).toEqual(['2', 2]);
});
// Flux.2: the family declares Flux2Scheduler, so the graph is the sigma path, not KSampler.
test('Flux.2 compiles Flux2Scheduler instead of KSampler', () => {
  const nodes = ['UNETLoader', 'CLIPLoader', 'VAELoader', 'CLIPTextEncode', 'VAEDecode', 'SaveImage', 'EmptyFlux2LatentImage', 'RandomNoise', 'CFGGuider', 'KSamplerSelect', 'Flux2Scheduler', 'SamplerCustomAdvanced'];
  const allow = { nodes, checkpoints: [intent.checkpoint], diffusionModels: ['flux-2-klein-base-4b-fp8.safetensors'], textEncoders: ['qwen_3_4b.safetensors'], clipTypes: ['flux2'], vaes: ['flux2-vae.safetensors'], samplers: ['euler'], schedulers: ['simple', 'normal'] };
  const g = compileRecipe({ ...intent, checkpoint: 'flux-2-klein-base-4b-fp8.safetensors', cfg: 5, diffusion: { textEncoder: 'qwen_3_4b.safetensors', clipType: 'flux2', vae: 'flux2-vae.safetensors', family: 'flux2' } }, allow).graph;
  expect(g['4'].class_type).toBe('EmptyFlux2LatentImage');
  expect(g.Z).toEqual({ class_type: 'Flux2Scheduler', inputs: { steps: 20, width: 512, height: 512 } });
  expect(g['5'].class_type).toBe('SamplerCustomAdvanced');
  expect(g['5'].inputs.sigmas).toEqual(['Z', 0]);
  expect(g.G.inputs.cfg).toBe(5);
});
test('Flux.2 masked refinement keeps sampler step count and derives denoise at the cropped canvas',()=>{
 const mask='deas-masks/00000000-0000-0000-0000-000000000001.png';
 const nodes=['UNETLoader','CLIPLoader','VAELoader','CLIPTextEncode','VAEDecode','SaveImage','EmptyFlux2LatentImage','RandomNoise','CFGGuider','KSamplerSelect','Flux2Scheduler','SamplerCustomAdvanced','SplitSigmas','LoadImage','VAEEncode','LoadImageMask','SetLatentNoiseMask','ImageCompositeMasked','ImageCrop','ImageScale','CropMask','MaskToImage','ImageToMask'];
 const allow={nodes,checkpoints:[],diffusionModels:['flux.safetensors'],textEncoders:['qwen.safetensors'],clipTypes:['flux2'],vaes:['ae.safetensors'],samplers:['euler'],schedulers:['normal']};
 const graph=compileRecipe({...intent,checkpoint:'flux.safetensors',diffusion:{textEncoder:'qwen.safetensors',clipType:'flux2',vae:'ae.safetensors',family:'flux2'},inputImage:mask,inputMask:mask,denoise:0.25,inpaintCrop:{x:0,y:0,width:256,height:256,targetWidth:1024,targetHeight:1024}},allow).graph;
 expect(graph.Z.inputs).toEqual({steps:80,width:1024,height:1024});
 expect(graph.ZD).toEqual({class_type:'SplitSigmas',inputs:{sigmas:['Z',0],step:60}});
 expect(graph['5'].inputs.sigmas).toEqual(['ZD',1]);
});
// Lumina2 is a checkpoint family: the top-level family id supplies the SD3 latent and the AuraFlow shift.
test('Lumina2 compiles the checkpoint recipe points by family id', () => {
  const nodes = ['CheckpointLoaderSimple', 'CLIPTextEncode', 'KSampler', 'VAEDecode', 'SaveImage', 'EmptySD3LatentImage', 'ModelSamplingAuraFlow'];
  const allow = { nodes, checkpoints: [intent.checkpoint], samplers: ['res_multistep'], schedulers: ['simple'] };
  const g = compileRecipe({ ...intent, steps: 30, cfg: 4, sampler: 'res_multistep', scheduler: 'simple', family: 'lumina2' }, allow).graph;
  expect(g['1'].class_type).toBe('CheckpointLoaderSimple');
  expect(g['4'].class_type).toBe('EmptySD3LatentImage');
  expect(g.M).toEqual({ class_type: 'ModelSamplingAuraFlow', inputs: { shift: 4, model: ['1', 0] } });
  expect(g['5'].inputs.model).toEqual(['M', 0]);
  expect(g['3'].class_type).toBe('CLIPTextEncode');
});
// HiDream: dual llama3 + t5, Flux latent, no sampling patch, encoded negative.
test('HiDream compiles dual encoders without a sampling patch', () => {
  const nodes = ['UNETLoader', 'DualCLIPLoader', 'VAELoader', 'CLIPTextEncode', 'KSampler', 'VAEDecode', 'SaveImage', 'EmptySD3LatentImage'];
  const allow = { nodes, checkpoints: [intent.checkpoint], diffusionModels: ['hidream_i1_full.safetensors'], textEncoders: ['llama_3_8b.safetensors', 't5xxl_fp16.safetensors'], dualTextEncoders: ['llama_3_8b.safetensors', 't5xxl_fp16.safetensors'], dualClipTypes: ['hidream'], clipTypes: ['hidream'], vaes: ['ae.safetensors'], samplers: ['euler'], schedulers: ['normal'] };
  const g = compileRecipe({ ...intent, checkpoint: 'hidream_i1_full.safetensors', diffusion: { textEncoder: 'llama_3_8b.safetensors', textEncoder2: 't5xxl_fp16.safetensors', clipType: 'hidream', vae: 'ae.safetensors', family: 'hidream' } }, allow).graph;
  expect(g.C.class_type).toBe('DualCLIPLoader');
  expect(g['4'].class_type).toBe('EmptySD3LatentImage');
  expect(g.M).toBeUndefined();
  expect(g['3'].class_type).toBe('CLIPTextEncode');
});
// SD3 and Chroma: the family id disambiguates families that share a CLIP type; SD3 adds ModelSamplingSD3(3).
test('SD3 and Chroma compile their family-declared recipe points', () => {
  const nodes = ['UNETLoader', 'DualCLIPLoader', 'CLIPLoader', 'VAELoader', 'CLIPTextEncode', 'KSampler', 'VAEDecode', 'SaveImage', 'EmptySD3LatentImage', 'EmptyChromaRadianceLatentImage', 'ModelSamplingSD3'];
  const allow = { nodes, checkpoints: [intent.checkpoint], diffusionModels: ['sd3.safetensors', 'chroma.safetensors', 'chroma-radiance.safetensors'], textEncoders: ['clip_l.safetensors', 't5xxl_fp16.safetensors'], dualTextEncoders: ['clip_l.safetensors', 't5xxl_fp16.safetensors'], dualClipTypes: ['sd3'], clipTypes: ['sd3', 'chroma'], vaes: ['ae.safetensors'], samplers: ['dpmpp_2m', 'euler'], schedulers: ['karras', 'simple', 'normal'] };
  const sd3 = compileRecipe({ ...intent, checkpoint: 'sd3.safetensors', steps: 28, cfg: 4.5, sampler: 'dpmpp_2m', scheduler: 'karras', diffusion: { textEncoder: 'clip_l.safetensors', textEncoder2: 't5xxl_fp16.safetensors', clipType: 'sd3', vae: 'ae.safetensors', family: 'sd3' } }, allow).graph;
  expect(sd3['4'].class_type).toBe('EmptySD3LatentImage');
  expect(sd3.M).toEqual({ class_type: 'ModelSamplingSD3', inputs: { shift: 3, model: ['1', 0] } });
  expect(sd3['3'].class_type).toBe('CLIPTextEncode');
  const radiance = compileRecipe({ ...intent, checkpoint: 'chroma-radiance.safetensors', diffusion: { textEncoder: 't5xxl_fp16.safetensors', clipType: 'chroma', vae: 'ae.safetensors', family: 'chroma-radiance' } }, allow).graph;
  expect(radiance['4'].class_type).toBe('EmptyChromaRadianceLatentImage');
  expect(radiance.M).toBeUndefined();
  // Without the family id the shared CLIP type falls back to the first family's latent.
  const shared = compileRecipe({ ...intent, checkpoint: 'chroma.safetensors', diffusion: { textEncoder: 't5xxl_fp16.safetensors', clipType: 'chroma', vae: 'ae.safetensors' } }, allow).graph;
  expect(shared['4'].class_type).toBe('EmptySD3LatentImage');
});
// Flux.1: dual encoders (clip_l + t5xxl), SD3 latent, zeroed negative — the family data plus the dual loader.
test('Flux.1 compiles the dual CLIP loader and family recipe points', () => {
  const nodes = ['UNETLoader', 'DualCLIPLoader', 'VAELoader', 'CLIPTextEncode', 'KSampler', 'VAEDecode', 'SaveImage', 'EmptySD3LatentImage', 'ConditioningZeroOut'];
  const allow = { nodes, checkpoints: [intent.checkpoint], diffusionModels: ['flux1-dev.safetensors'], textEncoders: ['clip_l.safetensors', 't5xxl_fp16.safetensors'], dualTextEncoders: ['clip_l.safetensors', 't5xxl_fp16.safetensors'], dualClipTypes: ['flux'], clipTypes: ['flux'], vaes: ['ae.safetensors'], samplers: ['euler'], schedulers: ['simple'] };
  const flux = { ...intent, checkpoint: 'flux1-dev.safetensors', steps: 20, cfg: 1, sampler: 'euler', scheduler: 'simple', diffusion: { textEncoder: 'clip_l.safetensors', textEncoder2: 't5xxl_fp16.safetensors', clipType: 'flux', vae: 'ae.safetensors' } };
  const g = compileRecipe(flux, allow).graph;
  expect(g.C).toEqual({ class_type: 'DualCLIPLoader', inputs: { clip_name1: 'clip_l.safetensors', clip_name2: 't5xxl_fp16.safetensors', type: 'flux' } });
  expect(g['4'].class_type).toBe('EmptySD3LatentImage');
  expect(g['3']).toEqual({ class_type: 'ConditioningZeroOut', inputs: { conditioning: ['2', 0] } });
  expect(g['2'].inputs.clip).toEqual(['C', 0]);
  expect(() => compileRecipe(flux, { ...allow, dualClipTypes: [] })).toThrow(/textEncoder2/);
  expect(() => compileRecipe({ ...flux, diffusion: { ...flux.diffusion, textEncoder2: 'clip_l.safetensors' } }, allow)).toThrow(/diffusion/);
});
// Z-Image Turbo: the family declares EmptySD3LatentImage, a zeroed negative and ModelSamplingAuraFlow(3) — same graph as jobs.rs.
test('Z-Image compiles the family-declared latent, negative and sampling patch', () => {
  const nodes = ['UNETLoader', 'CLIPLoader', 'VAELoader', 'CLIPTextEncode', 'KSampler', 'VAEDecode', 'SaveImage', 'EmptySD3LatentImage', 'EmptyFlux2LatentImage', 'ModelSamplingAuraFlow', 'ConditioningZeroOut'];
  const allow = { nodes, checkpoints: [intent.checkpoint, 'z_image_turbo_bf16.safetensors'], diffusionModels: ['z_image_turbo_bf16.safetensors'], textEncoders: ['qwen_3_4b.safetensors'], clipTypes: ['lumina2'], vaes: ['ae.safetensors'], samplers: ['res_multistep'], schedulers: ['simple'] };
  const turbo = { ...intent, checkpoint: 'z_image_turbo_bf16.safetensors', steps: 8, cfg: 1, sampler: 'res_multistep', scheduler: 'simple', diffusion: { textEncoder: 'qwen_3_4b.safetensors', clipType: 'lumina2', vae: 'ae.safetensors' } };
  const g = compileRecipe(turbo, allow).graph;
  expect(g['4'].class_type).toBe('EmptySD3LatentImage');
  expect(g['3']).toEqual({ class_type: 'ConditioningZeroOut', inputs: { conditioning: ['2', 0] } });
  expect(g.M).toEqual({ class_type: 'ModelSamplingAuraFlow', inputs: { shift: 3, model: ['1', 0] } });
  expect(g['5'].inputs.model).toEqual(['M', 0]);
  expect(() => compileRecipe(turbo, { ...allow, nodes: nodes.filter(n => n !== 'ModelSamplingAuraFlow') })).toThrow(/ModelSamplingAuraFlow/);
  // MOSS keeps its Flux2 latent through the same family data instead of a compiler branch.
  const moss = compileRecipe({ ...intent, diffusion: { textEncoder: 'moss.safetensors', clipType: 'moss_image', vae: 'flux2.safetensors' } },
    { ...allow, diffusionModels: [intent.checkpoint], textEncoders: ['moss.safetensors'], clipTypes: ['moss_image'], vaes: ['flux2.safetensors'], samplers: ['euler'], schedulers: ['normal'] }).graph;
  expect(moss['4'].class_type).toBe('EmptyFlux2LatentImage');
});
// Qwen Image 2.1 Turbo: the checkpoint's saved 8-step schedule (official ComfyUI template) replaces KSampler — same graph as jobs.rs.
test('fixed sigmas compile to ManualSigmas → SamplerCustomAdvanced in place of KSampler', () => {
  const nodes = ['CheckpointLoaderSimple', 'CLIPTextEncode', 'EmptyLatentImage', 'KSampler', 'VAEDecode', 'SaveImage', 'LoadImage', 'VAEEncode', 'RandomNoise', 'CFGGuider', 'KSamplerSelect', 'ManualSigmas', 'SamplerCustomAdvanced'];
  const allow = { nodes, checkpoints: [intent.checkpoint], images: ['in.png'], samplers: ['euler'], schedulers: ['normal'] };
  const sigmas = [1.0, 0.978453, 0.95418, 0.926626, 0.89508, 0.845148, 0.704534, 0.414568, 0];
  const g = compileRecipe({ ...intent, steps: 8, sigmas }, allow).graph;
  expect(g.Z.inputs.sigmas).toBe('1, 0.978453, 0.95418, 0.926626, 0.89508, 0.845148, 0.704534, 0.414568, 0');
  expect(g['5']).toEqual({ class_type: 'SamplerCustomAdvanced', inputs: { noise: ['N', 0], guider: ['G', 0], sampler: ['Q', 0], sigmas: ['Z', 0], latent_image: ['4', 0] } });
  expect(g.G.inputs).toEqual({ model: ['1', 0], positive: ['2', 0], negative: ['3', 0], cfg: 7 });
  expect(g.Q.inputs).toEqual({ sampler_name: 'euler' });
  const i2i = compileRecipe({ ...intent, steps: 4, sigmas: sigmas.slice(-5), inputImage: 'in.png', denoise: 0.5 }, allow).graph;
  expect(i2i['5'].inputs).toEqual({ noise: ['N', 0], guider: ['G', 0], sampler: ['Q', 0], sigmas: ['Z', 0], latent_image: ['9', 0] });
  expect(() => compileRecipe({ ...intent, steps: 8, sigmas }, { ...allow, nodes: nodes.filter(n => n !== 'ManualSigmas') })).toThrow(/ManualSigmas/);
  for (const bad of [[1, 0.5], [1, 0.5, 0.5, 0.2, 0], [1, 0.8, 0.6, 0.4, 0.2], [1, 0.8, 0.6, '0.4', 0], '1, 0']) {
    expect(() => compileRecipe({ ...intent, steps: 4, sigmas: bad }, allow)).toThrow(/sigmas/);
  }
});
test('native-only commands preserve exact intent, explicit consent and job identity', async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  const response = { id: 'native-id' };
  vi.mocked(invoke).mockResolvedValue(response);
  expect(await prepareDiagnosticJob('http://127.0.0.1:7801/prefix/', intent)).toBe(response);
  expect(invoke).toHaveBeenLastCalledWith('native_job_prepare', { baseUrl: 'http://127.0.0.1:7801/prefix/', intent });
  expect(await submitDiagnosticJob('plan-id')).toBe(response);
  expect(invoke).toHaveBeenLastCalledWith('native_job_submit', { planId: 'plan-id', confirmed: true });
  expect(await readDiagnosticJobStatus('job-id')).toBe(response);
  expect(invoke).toHaveBeenLastCalledWith('native_job_status', { jobId: 'job-id' });
  expect(await readCurrentDiagnosticJob()).toBe(response);
  expect(invoke).toHaveBeenLastCalledWith('native_job_current', {});
  vi.mocked(invoke).mockResolvedValue(null);
  expect(await readCurrentDiagnosticJob()).toBeNull();
});
test('Studio uses independent native collection commands with consent and no diagnostic calls', async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  const snapshot = {jobs: [], preparing: 2, capacity: 20, stopAvailable: true, admissionEpoch:0, stopping:false};
  vi.mocked(invoke).mockResolvedValue(snapshot);
  expect(await readCurrentWorkbench()).toEqual({...snapshot,frontendAdmitting:false});
  expect(invoke).toHaveBeenLastCalledWith('native_workbench_current', {});
  expect(await pollWorkbench()).toEqual({...snapshot,frontendAdmitting:false});
  expect(invoke).toHaveBeenLastCalledWith('native_workbench_poll', {});
  await runWorkbenchJob('http://localhost:7801/prefix/', intent, 0);
  expect(invoke).toHaveBeenLastCalledWith('native_workbench_run', {baseUrl:'http://localhost:7801/prefix/', intent, confirmed:true, admissionEpoch:0});
  expect(invoke).toHaveBeenCalledTimes(3);
});
test.each([readCurrentWorkbench, pollWorkbench])('collection reads bracket pending Run IPC without changing native counts (%s)', async read => {
  vi.mocked(isTauri).mockReturnValue(true);
  const snapshot = {jobs: [], preparing: 0, capacity: 20, stopAvailable: true, admissionEpoch:0, stopping:false};
  let finish!: (value: never) => void;
  let finishRead!: (value: typeof snapshot) => void;
  vi.mocked(invoke).mockImplementation(command => command === 'native_workbench_run'
    ? new Promise(resolve => { finish = resolve; }) : Promise.resolve(snapshot));
  const run = runWorkbenchJob('http://original/', intent, 0);
  expect(await read()).toEqual({...snapshot,frontendAdmitting:true});
  vi.mocked(invoke).mockImplementation(() => new Promise(resolve => { finishRead = resolve; }));
  const staleRead = read();
  finish({id:'admitted'} as never); await run;
  finishRead(snapshot);
  expect(await staleRead).toEqual({...snapshot,frontendAdmitting:true});
  vi.mocked(invoke).mockResolvedValue(snapshot);
  expect(await read()).toEqual({...snapshot,frontendAdmitting:false});

  vi.mocked(invoke).mockImplementation(command => command === 'native_workbench_run'
    ? new Promise(resolve => { finish = resolve; }) : new Promise(resolve => { finishRead = resolve; }));
  const earlierRead = read();
  const laterRun = runWorkbenchJob('http://original/', intent, 0);
  finishRead(snapshot);
  expect(await earlierRead).toEqual({...snapshot,frontendAdmitting:true});
  finish({id:'later'} as never); await laterRun;
});
test('Run registers before invoke and clears only settled calls, including a lost reply', async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  const snapshot = {jobs: [], preparing: 0, capacity: 20, stopAvailable: true, admissionEpoch:0, stopping:false};
  let nestedRead!: ReturnType<typeof readCurrentWorkbench>;
  let finishNested!: (value: typeof snapshot) => void;
  const pending: {resolve:(value:never)=>void;reject:(reason:unknown)=>void}[] = [];
  vi.mocked(invoke).mockImplementation(command => {
    if(command !== 'native_workbench_run') return new Promise(resolve => { finishNested = resolve; });
    nestedRead = readCurrentWorkbench();
    return new Promise((resolve,reject) => { pending.push({resolve,reject}); });
  });
  const immediate = runWorkbenchJob('http://original/', intent, 0);
  pending[0].resolve({id:'immediate'} as never); await immediate;
  finishNested(snapshot);
  expect((await nestedRead).frontendAdmitting).toBe(true);
  vi.mocked(invoke).mockImplementation(command => command === 'native_workbench_run'
    ? new Promise((resolve,reject) => { pending.push({resolve,reject}); }) : Promise.resolve(snapshot));
  const first = runWorkbenchJob('http://original/', intent, 0);
  const second = runWorkbenchJob('http://original/', intent, 0);
  const rejected = expect(second).rejects.toThrow('IPC response lost');
  pending[2].reject(new Error('IPC response lost')); await rejected;
  expect((await readCurrentWorkbench()).frontendAdmitting).toBe(true);
  pending[1].resolve({id:'first'} as never); await first;
  expect(await readCurrentWorkbench()).toEqual({...snapshot,frontendAdmitting:false});
  await expect(runWorkbenchJob(' ', intent, 0)).rejects.toThrow('explicit backend endpoint');
  expect((await readCurrentWorkbench()).frontendAdmitting).toBe(false);
});
test('browser fails closed for all job commands without IPC or browser fetch', async () => {
  vi.mocked(isTauri).mockReturnValue(false);
  const fetch = vi.fn(); vi.stubGlobal('fetch', fetch);
  for (const request of [runWorkbenchJob('http://127.0.0.1:7801', intent, 0), readCurrentWorkbench(), pollWorkbench(), stopWorkbench(), prepareDiagnosticJob('http://127.0.0.1:7801', intent), submitDiagnosticJob('plan'), readDiagnosticJobStatus('job'), readCurrentDiagnosticJob(), loadDiagnosticJobOutput('job', 0), exportDiagnosticJobOutput('job', 0, 'Save')]) {
    await expect(request).rejects.toThrow('desktop app');
  }
  expect(invoke).not.toHaveBeenCalled(); expect(fetch).not.toHaveBeenCalled();
});
test('output transfer and Save As are explicit native commands', async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  const output = { jobId: 'job', index: 0, mimeType: 'image/png', width: 512, height: 512, sha256: 'a'.repeat(64), bytes: [137, 80, 78, 71] };
  vi.mocked(invoke).mockResolvedValue(output);
  expect(await loadDiagnosticJobOutput('job', 0)).toEqual(output);
  expect(invoke).toHaveBeenLastCalledWith('native_job_output', { jobId: 'job', index: 0 });
  vi.mocked(invoke).mockResolvedValue(null);
  expect(await exportDiagnosticJobOutput('job', 0, 'Save PNG')).toBeNull();
  expect(invoke).toHaveBeenLastCalledWith('native_job_export', { jobId: 'job', index: 0, title: 'Save PNG' });
});
test.each([{ mimeType: 'image/jpeg' }, { mimeType: 'audio/flac' }, { mimeType: 'audio/flac', width: 0 }, { jobId: 'foreign' }, { index: 1 }, { bytes: [] }, { bytes: [256] }, { bytes: [-1] }, { bytes: [0.5] }, { width: 8193 }, { width: 8192, height: 8192 }, { height: 0 }, { sha256: 'A'.repeat(64) }])('rejects malformed output before Blob allocation: %s', async invalid => {
  vi.mocked(isTauri).mockReturnValue(true);
  vi.mocked(invoke).mockResolvedValue({ jobId: 'job', index: 0, mimeType: 'image/png', width: 512, height: 512, sha256: 'a'.repeat(64), bytes: [137], ...invalid });
  await expect(loadDiagnosticJobOutput('job', 0)).rejects.toThrow('Invalid native output');
});
test('FLAC outputs carry no dimensions', async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  const output = { jobId: 'job', index: 1, mimeType: 'audio/flac', width: 0, height: 0, sha256: 'a'.repeat(64), bytes: [102, 76, 97, 67] };
  vi.mocked(invoke).mockResolvedValue(output);
  expect(await loadDiagnosticJobOutput('job', 1)).toEqual(output);
});
test('rejects oversized and sparse byte arrays before allocating a typed buffer', async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  for (const bytes of [new Array(144 * 1024 * 1024 + 1), new Array(1)]) {
    vi.mocked(invoke).mockResolvedValue({ jobId: 'job', index: 0, mimeType: 'image/png', width: 1, height: 1, sha256: 'a'.repeat(64), bytes });
    await expect(loadDiagnosticJobOutput('job', 0)).rejects.toThrow('Invalid native output');
  }
});
test('4K PNG metadata passes the same dimension and pixel bounds as native output',async()=>{
 vi.mocked(isTauri).mockReturnValue(true);
 const output={jobId:'job',index:0,mimeType:'image/png',width:3840,height:2160,sha256:'a'.repeat(64),bytes:[137,80,78,71]};
 vi.mocked(invoke).mockResolvedValue(output);
 expect(await loadDiagnosticJobOutput('job',0)).toEqual(output);
});
test('empty endpoints are rejected before IPC, native errors retain code and message', async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  await expect(runWorkbenchJob('  ', intent, 0)).rejects.toThrow('explicit backend endpoint');
  await expect(prepareDiagnosticJob('  ', intent)).rejects.toThrow('explicit backend endpoint');
  expect(invoke).not.toHaveBeenCalled();
  vi.mocked(invoke).mockRejectedValue({ code: 'MODEL_ERROR', message: 'Checkpoint failed to load' });
  await expect(submitDiagnosticJob('plan')).rejects.toThrow('MODEL_ERROR: Checkpoint failed to load');
});
test('Stop coalesces confirmed native calls and closes cross-mount admission until settled', async () => {
 vi.mocked(isTauri).mockReturnValue(true);
 let finish!: (value: never)=>void;
 const snapshot={jobs:[],preparing:0,capacity:20,stopAvailable:true,admissionEpoch:1,stopping:false};
 vi.mocked(invoke).mockImplementation(command=>command==='native_workbench_stop'?new Promise(resolve=>{finish=resolve;}):Promise.resolve(snapshot));
 const first=stopWorkbench();const second=stopWorkbench();
 expect(invoke).toHaveBeenCalledTimes(1);
 expect(invoke).toHaveBeenCalledWith('native_workbench_stop',{confirmed:true});
 expect((await readCurrentWorkbench()).stopping).toBe(true);
 finish({...snapshot,outcome:'empty',discarded:0,issues:[]} as never);
 expect(await first).toEqual(await second);
 expect((await readCurrentWorkbench()).stopping).toBe(false);
});

test('a completed Stop fence releases obsolete pending Run ownership without clearing newer calls',async()=>{
 vi.mocked(isTauri).mockReturnValue(true);
 let finish!:(job:never)=>void;
 const snapshot={jobs:[],preparing:0,capacity:20,stopAvailable:true,admissionEpoch:4,stopping:false};
 vi.mocked(invoke).mockImplementation(command=>command==='native_workbench_run'?new Promise(resolve=>{finish=resolve;}):Promise.resolve({...snapshot,outcome:'settled',discarded:1,issues:[]}));
 const old=runWorkbenchJob('http://original/',intent,3);
 await stopWorkbench();
 expect((await readCurrentWorkbench()).frontendAdmitting).toBe(false);
 const finishOld=finish;const newer=runWorkbenchJob('http://original/',intent,4);
 finishOld({id:'obsolete'} as never);await old;
 expect((await readCurrentWorkbench()).frontendAdmitting).toBe(true);
 finish({id:'new'} as never);await newer;
 expect((await readCurrentWorkbench()).frontendAdmitting).toBe(false);
});

test('a failed Stop can be explicitly retried; a pending Stop refuses Run without IPC',async()=>{
 vi.mocked(isTauri).mockReturnValue(true);
 vi.mocked(invoke).mockRejectedValueOnce(new Error('IPC unreachable'));
 await expect(stopWorkbench()).rejects.toThrow('IPC unreachable');
 let finish!:(value:never)=>void;vi.mocked(invoke).mockImplementation(()=>new Promise(resolve=>{finish=resolve;}));
 const stop=stopWorkbench();await expect(runWorkbenchJob('http://original/',intent,0)).rejects.toThrow('Stop is in progress');
 expect(invoke).toHaveBeenCalledTimes(2);
 finish({jobs:[],preparing:0,capacity:20,stopAvailable:true,admissionEpoch:1,stopping:false,outcome:'empty',discarded:0,issues:[]} as never);await stop;
});
