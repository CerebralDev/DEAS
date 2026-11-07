import test from 'node:test';
import assert from 'node:assert/strict';
import { generationSettings, imageMetadataRows, parseMetadataJson } from '../src/lib/imageMetadata.ts';
import { compileRecipe, type DiagnosticImageIntentV1 } from '../src/lib/execution.ts';

const intent:DiagnosticImageIntentV1={recipeId:'checkpoint-text-to-image-diagnostic',version:1,positivePrompt:' forest  ',negativePrompt:'',checkpoint:'base.safetensors',seed:42,width:512,height:768,steps:2,cfg:7,sampler:'euler',scheduler:'normal',outputPrefix:'raw/test'};
const nodes=['CheckpointLoaderSimple','CLIPTextEncode','EmptyLatentImage','KSampler','VAEDecode','SaveImage','RandomNoise','CFGGuider','KSamplerSelect','ManualSigmas','SamplerCustomAdvanced'];
const allow={nodes,checkpoints:[intent.checkpoint],samplers:['euler'],schedulers:['normal']};
const metadata=(graph:unknown):[string,string][]=>[['prompt',JSON.stringify(graph)]];

test('actual compiled image graphs retain exact prompts and custom sampling without inventing a scheduler',()=>{
 const ordinary=generationSettings(metadata(compileRecipe(intent,allow).graph));
 assert.equal(ordinary?.positive,intent.positivePrompt);assert.equal(ordinary?.negative,'');
 const custom=generationSettings(metadata(compileRecipe({...intent,sigmas:[2,1,0]},allow).graph));
 assert.deepEqual(custom,{positive:intent.positivePrompt,negative:'',checkpoint:intent.checkpoint,seed:42,steps:2,cfg:7,sampler:'euler',sigmas:[2,1,0],width:512,height:768,loras:[]});
});

test('absent and explicitly empty prompts differ in A1111, Swarm and graph metadata',()=>{
 assert.equal(generationSettings([['parameters','cat\nSteps: 2, Seed: 42']])?.negative,undefined);
 assert.equal(generationSettings([['parameters','cat\nNegative prompt: \nSteps: 2, Seed: 42']])?.negative,'');
 assert.equal(generationSettings([['parameters',JSON.stringify({sui_image_params:{prompt:'',negativeprompt:'',seed:42}})]])?.positive,'');
 assert.equal(generationSettings([['parameters',JSON.stringify({sui_image_params:{seed:42}})]])?.positive,undefined);
 assert.equal(generationSettings([['parameters','cat\nNegative prompt: negative_prompt_not_found\nSteps: 2']])?.negative,undefined);
 assert.equal(generationSettings([['parameters','cat\nNegative prompt: real_not_found words\nSteps: 2']])?.negative,'real_not_found words');
});

test('model and LoRAs come from the sampled model chain, never unrelated graph nodes or guessed strengths',()=>{
 const graph:Record<string,unknown>={ '0':{class_type:'UNETLoader',inputs:{unet_name:'wrong.safetensors'}},...compileRecipe(intent,allow).graph};
 graph.unusedLora={class_type:'LoraLoader',inputs:{lora_name:'wrong.safetensors',strength_model:1}};
 assert.equal(generationSettings(metadata(graph))?.checkpoint,intent.checkpoint);
 assert.deepEqual(generationSettings(metadata(graph))?.loras,[]);
});

// Literal emission shapes from jobs_video.rs::ltx/ltx_pass (not an inferred DEAS intent).
test('LTX video API graph recovers linked CFG/sampling and literal conditioning through AV wrappers',()=>{
 const graph={
  '1':{class_type:'UnetLoaderGGUF',inputs:{unet_name:'ltx.gguf'}},
  '2':{class_type:'CLIPTextEncode',inputs:{text:'walk'}},'3':{class_type:'CLIPTextEncode',inputs:{text:''}},
  CN:{class_type:'LTXVConditioning',inputs:{positive:['2',0],negative:['3',0],frame_rate:24}},
  EL:{class_type:'EmptyLTXVLatentVideo',inputs:{width:768,height:512,length:97}},
  CAT:{class_type:'LTXVConcatAVLatent',inputs:{video_latent:['EL',0],audio_latent:['EA',0]}},
  N:{class_type:'RandomNoise',inputs:{noise_seed:9}},Q:{class_type:'KSamplerSelect',inputs:{sampler_name:'euler'}},
  GU:{class_type:'CFGGuider',inputs:{model:['1',0],positive:['CN',0],negative:['CN',1],cfg:3}},
  Z:{class_type:'BasicScheduler',inputs:{model:['1',0],scheduler:'simple',steps:20,denoise:1}},
  '5':{class_type:'SamplerCustomAdvanced',inputs:{noise:['N',0],guider:['GU',0],sampler:['Q',0],sigmas:['Z',0],latent_image:['CAT',0]}},
 };
 assert.deepEqual(generationSettings(metadata(graph)),{media:'video',positive:'walk',negative:'',checkpoint:'ltx.gguf',seed:9,steps:20,cfg:3,sampler:'euler',scheduler:'simple',width:768,height:512,frames:97,frameRate:24,loras:[],skipped:['video.intent']});
});

test('malformed links, conditioning cycles and missing strengths do not fabricate settings or hang',()=>{
 const graph={A:{class_type:'LTXVConditioning',inputs:{positive:['A',0]}},'5':{class_type:'KSampler',inputs:{positive:['A',0],negative:['missing',0],seed:0,steps:2}}};
 assert.deepEqual(generationSettings(metadata(graph)),{seed:0,steps:2});
 const brokenModel={...graph,L:{class_type:'LoraLoader',inputs:{model:['L',0],lora_name:'style.safetensors',strength_model:1}},'5':{class_type:'KSampler',inputs:{model:['L',0],seed:0,steps:2}}};
 assert.deepEqual(generationSettings(metadata(brokenModel)),{seed:0,steps:2});
 const noStrength={...brokenModel,L:{class_type:'LoraLoader',inputs:{lora_name:'style.safetensors'}}};
 assert.equal(generationSettings(metadata(noStrength))?.loras,undefined);
});

test('literal graph values beat truncated display model names and false sampler warnings',()=>{
 const entries=metadata(compileRecipe(intent,allow).graph);
 entries.unshift(['parameters','display prompt\nSteps: 2, Seed: 42, Model: base, Sampler: PLMS']);
 const settings=generationSettings(entries);
 assert.equal(settings?.checkpoint,'base.safetensors');assert.equal(settings?.sampler,'euler');assert.equal(settings?.skipped,undefined);
});

test('Wan stage and H3 BasicGuider graphs stay explicitly partial with no invented CFG or negative prompt',()=>{
 const wan={
  '1':{class_type:'UNETLoader',inputs:{unet_name:'high.safetensors'}},
  '2':{class_type:'CLIPTextEncode',inputs:{text:'walk'}},'3':{class_type:'CLIPTextEncode',inputs:{text:''}},
  W:{class_type:'WanImageToVideo',inputs:{positive:['2',0],negative:['3',0],width:832,height:480,length:81}},
  SH:{class_type:'ModelSamplingSD3',inputs:{model:['1',0],shift:5}},
  KH:{class_type:'KSamplerAdvanced',inputs:{model:['SH',0],positive:['W',0],negative:['W',1],latent_image:['W',2],noise_seed:4,steps:20,cfg:3,sampler_name:'euler',scheduler:'simple'}},
 };
 const w=generationSettings(metadata(wan));
 assert.equal(w?.media,'video');assert.equal(w?.checkpoint,'high.safetensors');assert.equal(w?.positive,'walk');assert.equal(w?.negative,'');assert.equal(w?.frames,81);assert.ok(w?.skipped?.length);
 const h3={
  '1':{class_type:'UNETLoader',inputs:{unet_name:'h3.safetensors'}},
  D:{class_type:'MiniMaxH3Director',inputs:{fl2va_model:['1',0],prompt:'talk',width:1024,height:576,frame_rate:24,duration:4}},
  G:{class_type:'MiniMaxH3DirectorGuide',inputs:{guide:['D',0]}},
  SS:{class_type:'MiniMaxH3SigmaShift',inputs:{model:['D',5]}},
  B:{class_type:'BasicGuider',inputs:{model:['SS',0],conditioning:['G',0]}},
  N:{class_type:'RandomNoise',inputs:{noise_seed:5}},Q:{class_type:'KSamplerSelect',inputs:{sampler_name:'euler'}},
  Z:{class_type:'BasicScheduler',inputs:{steps:20,scheduler:'simple'}},
  '5':{class_type:'SamplerCustomAdvanced',inputs:{noise:['N',0],guider:['B',0],sampler:['Q',0],sigmas:['Z',0],latent_image:['G',1]}},
 };
 const h=generationSettings(metadata(h3));
 assert.equal(h?.media,'video');assert.equal(h?.checkpoint,'h3.safetensors');assert.equal(h?.positive,'talk');assert.equal(h?.negative,undefined);assert.equal(h?.cfg,undefined);assert.equal(h?.frames,undefined);assert.equal(h?.width,1024);assert.equal(h?.frameRate,24);
});

test('actual Qwen encoder outputs preserve empty negative and omit unknown native reference latent size',()=>{
 const diffusion={textEncoder:'qwen.safetensors',clipType:'qwen_image',vae:'vae.safetensors'};
 const graph=compileRecipe({...intent,diffusion,nativeImageReference:{method:'qwen-image-2.1',images:['ref.png']}},{...allow,nodes:['UNETLoader','CLIPLoader','VAELoader','TextEncodeQwenImage21','LoadImage','ImageScale','KSampler','VAEDecode','SaveImage'],diffusionModels:[intent.checkpoint],textEncoders:[diffusion.textEncoder],clipTypes:[diffusion.clipType],vaes:[diffusion.vae],images:['ref.png']}).graph;
 const s=generationSettings(metadata(graph));assert.equal(s?.positive,intent.positivePrompt);assert.equal(s?.negative,'');assert.equal(s?.width,undefined);
});

test('sampler aliases preserve the actual algorithm instead of substituting another sampler',()=>{
 assert.equal(generationSettings([['parameters','cat\nSteps: 2, Sampler: DPM++ 2M SDE Heun']])?.sampler,'dpmpp_2m_sde_heun');
 const restart=generationSettings([['parameters','cat\nSteps: 2, Sampler: Restart']]);
 assert.equal(restart?.sampler,undefined);assert.deepEqual(restart?.skipped,['Sampler: Restart']);
});

const positive='subject_definitions:\nA character says "NaN".\\\n\nsummary:\nFull authored text, not just this summary.\n\ndetailed_description:\nWalk through a forest.';
const graph={
 '1':{class_type:'UNETLoader',inputs:{unet_name:'H3/model.safetensors'}},
 L1:{class_type:'LoraLoaderModelOnly',inputs:{model:['1',0],lora_name:'H3/turbo.safetensors',strength_model:1}},
 D:{class_type:'MiniMaxH3Director',inputs:{prompt:positive,ref2va_model:['L1',0],width:544,height:672,frame_rate:24,duration:4.458333333333333}},
 G:{class_type:'MiniMaxH3DirectorGuide',inputs:{guide:['D',0]}},
 B:{class_type:'BasicGuider',inputs:{model:['D',0],conditioning:['G',0]}},
 N:{class_type:'RandomNoise',inputs:{noise_seed:4041897001}},
 Q:{class_type:'KSamplerSelect',inputs:{sampler_name:'euler'}},
 Z:{class_type:'BasicScheduler',inputs:{steps:6,scheduler:'simple'}},
 '5':{class_type:'SamplerCustomAdvanced',inputs:{guider:['B',0],noise:['N',0],sampler:['Q',0],sigmas:['Z',0],latent_image:['G',1]}},
};
const legacy=JSON.stringify({...graph,cache:{class_type:'LoadImage',inputs:{},is_changed:['CACHE_NAN']}}).replace('"CACHE_NAN"','NaN');

test('legacy NaN cache metadata yields full H3 prompt and actual sampling/model fields, never an invented CFG',()=>{
 const entries:[string,string][]=[['prompt',legacy],['workflow','{"nodes":[],"links":[]}']];
 const settings=generationSettings(entries);
 assert.equal(settings?.positive,positive);assert.equal(settings?.cfg,undefined);
 assert.equal(settings?.checkpoint,'H3/model.safetensors');assert.equal(settings?.steps,6);
 assert.equal(settings?.sampler,'euler');assert.equal(settings?.seed,4041897001);
 assert.deepEqual(settings?.loras,[{name:'H3/turbo.safetensors',strength:1}]);
 assert.equal(settings?.frameRate,24);assert.equal(settings?.frames,undefined);
 assert.deepEqual(settings?.skipped,['video.intent']);
 const rows=imageMetadataRows(entries);
 assert.deepEqual(rows.find(([key])=>key==='prompt'),['prompt',positive]);
 assert.deepEqual(rows.find(([key])=>key==='model'),['model','H3/model.safetensors']);
 assert.equal(rows.some(([key])=>key.includes('#')||key==='cfgscale'||key==='workflow'),false);
});

test('legacy token repair respects strings, escapes, boundaries and malformed/oversized input',()=>{
 assert.deepEqual(parseMetadataJson('{"is_changed":[NaN],"text":"NaN \\\"NaN\\\" \\\\ NaN"}'),{is_changed:[null],text:'NaN "NaN" \\ NaN'});
 for(const invalid of ['{"x":NaNevil}','{"x":undefined}','{"x":Infinity}','{"x":NaN}; globalThis.pwned=true','x'.repeat(2*1024*1024+1)])assert.throws(()=>parseMetadataJson(invalid));
 const broken:[string,string][]=[['prompt','{"broken":NaN + 1}']];
 assert.deepEqual(imageMetadataRows(broken),broken);assert.equal(generationSettings(broken),null);
});
