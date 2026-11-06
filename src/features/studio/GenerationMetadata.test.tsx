import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, test, vi } from 'vitest';
import { GenerationMetadata } from './GenerationMetadata';
import { I18nProvider } from '../../i18n/I18nProvider';
import type { JobStatus } from '../../lib/jobs';
import type { DiagnosticImageIntentV1 } from '../../lib/execution';
import { OpenComfyWorkflow } from '../../lib/comfyWorkflow';
import { generationSettings, readImageMetadata } from '../../lib/imageMetadata';
vi.mock('@tauri-apps/api/core',()=>({isTauri:()=>true}));
vi.mock('../../lib/imageMetadata',async importOriginal=>({...await importOriginal<typeof import('../../lib/imageMetadata')>(),readImageMetadata:vi.fn()}));
afterEach(()=>{cleanup();vi.clearAllMocks();});
const job:JobStatus={id:'a',baseUrl:'http://127.0.0.1',promptId:'p',state:'succeeded',outputs:[],error:null,generationReady:false,appVersion:'0.1.14',metadata:{startedAt:1700000000000,generationMs:13310},intent:{recipeId:'checkpoint-text-to-image-diagnostic',version:1,positivePrompt:'<img src=x onerror=alert(1)> forest',negativePrompt:'blur',checkpoint:'Krea2/model.safetensors',width:896,height:1152,seed:940563189,steps:8,cfg:1,sampler:'euler',scheduler:'simple',outputPrefix:'DEAS',diffusion:{textEncoder:'qwen3vl_4b.safetensors',clipType:'krea2',vae:'qwen_vae.safetensors'},loras:[{name:'style.safetensors',strength:0.7}]}};
test.each(['clean','deep','stems'] as const)('audio metadata shows the submitted %s selection, never later preferences',mode=>{
 localStorage.setItem('deas.locale.v1','en');
 localStorage.setItem('deas.pref.studio.audioEnhance.post.mode',JSON.stringify('off'));
 const intent:JobStatus['intent']={recipeId:'music-generation',version:1,family:'yue2',mode:'t2m',planning:'off',checkpoint:'yue2',style:'pop',lyrics:'',seed:1,maxDuration:30,steps:20,cfg:1,sampler:'dpm_2',scheduler:'sgm_uniform',generation:{temperature:1,topP:0.95,topK:100,repetitionPenalty:1.2,cfgScale:1},outputPrefix:'raw/audio/x',audioEnhance:{mode,profile:mode==='deep'?'speech':'mix'}};
 const view=render(<I18nProvider><GenerationMetadata job={{...job,intent}}/></I18nProvider>);
 const row=Array.from(view.container.querySelectorAll('dt')).find(el=>el.textContent==='Audio enhancement');
 expect(row?.nextElementSibling?.textContent).toContain(mode==='clean'?'Clean':mode==='deep'?'Deep':'Stems');
 expect(row?.nextElementSibling?.textContent).toContain(mode==='deep'?'Speech':'Music / mixed audio');
 const {audioEnhance,...off}=intent;void audioEnhance;
 view.rerender(<I18nProvider><GenerationMetadata job={{...job,intent:off}}/></I18nProvider>);
 expect(screen.queryByText('Audio enhancement')).toBeNull();
});

test('renders native generation settings as text and switches with the selected job without stale metadata',()=>{
 localStorage.setItem('deas.locale.v1','en');
 const {container,rerender}=render(<I18nProvider><GenerationMetadata job={job}/></I18nProvider>);
 for(const value of ['<img src=x onerror=alert(1)> forest','blur','Krea2/model.safetensors','896 × 1152','940563189','13.31 sec','0.1.14','qwen3vl_4b.safetensors','qwen_vae.safetensors','style.safetensors (0.7)'])expect(screen.getByText(value)).toBeTruthy();
 expect(container.querySelector('img')).toBeNull();
 rerender(<I18nProvider><GenerationMetadata job={{...job,id:'b',metadata:null,intent:{...(job.intent as DiagnosticImageIntentV1),positivePrompt:'second',checkpoint:'second.safetensors'}}}/></I18nProvider>);
 expect(screen.queryByText('13.31 sec')).toBeNull();expect(screen.queryByText('Krea2/model.safetensors')).toBeNull();expect(screen.getByText('second')).toBeTruthy();
 rerender(<I18nProvider><GenerationMetadata job={null}/></I18nProvider>);
 expect(screen.queryByText('second')).toBeNull();expect(screen.getByText('Select an image or generated job to see its metadata.')).toBeTruthy();
});

test.each(['de','en','fr','ja','tr','zh'])('formats actual image/video duration using selected locale %s',locale=>{
 localStorage.setItem('deas.locale.v1',locale);
 const video:JobStatus['intent']={recipeId:'video-generation',version:1,family:'wan',positivePrompt:'video prompt',negativePrompt:'blur',model:'video.safetensors',textEncoder:'encoder',vae:'vae',seed:42,width:832,height:480,frames:81,frameRate:16,steps:12,cfg:3,sampler:'euler',scheduler:'simple',outputPrefix:'DEAS',output:{codec:'Auto',container:'Auto',bitDepth:'Auto',quality:20,audioCodec:'Auto',audioBitrate:'192k',saveFirstFrame:false,saveLastFrame:false},wan:{lowModel:'low',shiftHigh:5,shiftLow:5,refinerStep:6}};
 const {container,rerender}=render(<I18nProvider><GenerationMetadata job={job}/></I18nProvider>);
 const expected=new Intl.NumberFormat(locale,{style:'unit',unit:'second',unitDisplay:'short',maximumFractionDigits:2}).format(13.31);
 expect(container.textContent).toContain(expected);
 rerender(<I18nProvider><GenerationMetadata job={{...job,intent:video}}/></I18nProvider>);
 expect(container.textContent).toContain(expected);
 expect(screen.getByText('video.safetensors')).toBeTruthy();expect(screen.getByText('42')).toBeTruthy();expect(screen.getByText('12')).toBeTruthy();
 expect(container.textContent).toContain(new Date(job.metadata!.startedAt).toLocaleString(locale));
});

test.each([
 ['unknown',null,'succeeded'],['zero',0,'succeeded'],['failed',13310,'failed'],['running',13310,'running'],['invalid',-1,'succeeded'],['nonfinite',Infinity,'succeeded'],
] as const)('does not invent metadata duration: %s',(_name,generationMs,state)=>{
 localStorage.setItem('deas.locale.v1','en');
 const {container}=render(<I18nProvider><GenerationMetadata job={{...job,state,metadata:{startedAt:1700000000000,generationMs}}}/></I18nProvider>);
 const time=Array.from(container.querySelectorAll('dt')).find(el=>el.textContent==='Generation time');
 if(generationMs===0)expect(time?.nextElementSibling?.textContent).toBe('0 sec');else expect(time).toBeUndefined();
});

test('the metadata restore button hands back the exact immutable job; file rows are display-only in the dock',async()=>{
 localStorage.setItem('deas.locale.v1','en');
 const onRestore=vi.fn();
 render(<I18nProvider><GenerationMetadata job={job} onRestore={onRestore}/></I18nProvider>);
 (await screen.findByRole('button',{name:'Use these settings'})).click();
 expect(onRestore).toHaveBeenCalledExactlyOnceWith(job);
 cleanup();
 const media=(name:string,kind:'image'|'video'='image')=>({name,path:`/images/${name}`,src:`asset:///${name}`,media:kind});
 vi.mocked(readImageMetadata).mockResolvedValueOnce([['parameters','cat\nSteps: 4, Sampler: Euler a, CFG scale: 2, Seed: 9, Size: 64x64, Model: m']]);
 const {rerender}=render(<I18nProvider><GenerationMetadata job={null} media={media('a.png')} onRestore={onRestore}/></I18nProvider>);
 expect(await screen.findByText('cat Steps: 4, Sampler: Euler a, CFG scale: 2, Seed: 9, Size: 64x64, Model: m')).toBeTruthy();
 // The dock no longer carries the file's apply button — the preview's More menu owns that action (see Workbench.test).
 expect(screen.queryByRole('button',{name:'Use these settings'})).toBeNull();
 expect(generationSettings([['parameters','cat\nSteps: 4, Sampler: Euler a, CFG scale: 2, Seed: 9, Size: 64x64, Model: m']])).toMatchObject({positive:'cat',checkpoint:'m',seed:9,steps:4,cfg:2,sampler:'euler_ancestral',width:64,height:64});
 // Videos carry only the ComfyUI prompt/workflow container tags.
 vi.mocked(readImageMetadata).mockResolvedValueOnce([['prompt',JSON.stringify({'5':{class_type:'KSampler',inputs:{seed:5,steps:3,cfg:1,sampler_name:'euler',scheduler:'simple'}}})],['workflow','{}']]);
 rerender(<I18nProvider><GenerationMetadata job={null} media={media('clip.mp4','video')} onRestore={onRestore}/></I18nProvider>);
 expect(await screen.findByText((t:string)=>t.includes('KSampler #5 · seed'))).toBeTruthy();
 expect(screen.queryByRole('button',{name:'Use these settings'})).toBeNull();
 {const g=generationSettings([['prompt',JSON.stringify({'5':{class_type:'KSampler',inputs:{seed:5,steps:3,cfg:1,sampler_name:'euler',scheduler:'simple'}}})]]); expect(g).toBeTruthy(); expect(g).toMatchObject({seed:5,steps:3,cfg:1,sampler:'euler',scheduler:'simple'});}
 vi.mocked(readImageMetadata).mockResolvedValueOnce([['Description','plain']]);
 rerender(<I18nProvider><GenerationMetadata job={null} media={media('b.png')} onRestore={onRestore}/></I18nProvider>);
 expect(await screen.findByText('plain')).toBeTruthy();expect(screen.queryByRole('button',{name:'Use these settings'})).toBeNull();
});

test('offers editor workflows only, disables without the owned backend, and reports import failures',async()=>{
 localStorage.setItem('deas.locale.v1','en');
 const workflow={nodes:[{id:1,type:'CLIPTextEncode'}],links:[],version:0.4};
 const media={name:'clip.mp4',path:'/images/clip.mp4',src:'asset:///clip.mp4',media:'video' as const};
 vi.mocked(readImageMetadata).mockResolvedValue([['workflow',JSON.stringify(workflow)]]);
 const onOpen=vi.fn().mockResolvedValue(undefined);
 const {rerender}=render(<I18nProvider><GenerationMetadata job={null} media={media}/></I18nProvider>);
 expect((await screen.findByRole('button',{name:'Open in Comfy'}) as HTMLButtonElement).disabled).toBe(true);
 rerender(<I18nProvider><OpenComfyWorkflow.Provider value={onOpen}><GenerationMetadata job={null} media={media}/></OpenComfyWorkflow.Provider></I18nProvider>);
 const openButton=await screen.findByRole('button',{name:'Open in Comfy'});
 await act(async()=>openButton.click());
 expect(onOpen).toHaveBeenCalledWith(workflow,'clip.mp4');
 onOpen.mockRejectedValueOnce(new Error('unsupported'));
 await act(async()=>openButton.click());
 expect(await screen.findByRole('alert')).toBeTruthy();
 vi.mocked(readImageMetadata).mockResolvedValueOnce([['workflow','{}']]);
 rerender(<I18nProvider><GenerationMetadata job={null} media={{...media,src:'asset:///bad.mp4',path:'/images/bad.mp4'}}/></I18nProvider>);
 await screen.findByText('{}');expect(screen.queryByRole('button',{name:'Open in Comfy'})).toBeNull();
});

test('reads selected local images, clears stale values, ignores late replies and reports empty/error states',async()=>{
 localStorage.setItem('deas.locale.v1','en');
 let finishOld!:(value:[string,string][])=>void;
 vi.mocked(readImageMetadata).mockImplementationOnce(()=>new Promise(resolve=>{finishOld=resolve;}));
 const media=(name:string)=>({name,path:`/images/${name}`,src:`asset:///${name}`,media:'image' as const});
 const {container,rerender}=render(<I18nProvider><GenerationMetadata job={null} media={media('old.png')}/></I18nProvider>);
 expect(screen.getByText('Reading media metadata…')).toBeTruthy();
 vi.mocked(readImageMetadata).mockResolvedValueOnce([['parameters',JSON.stringify({sui_image_params:{prompt:'<img src=x> imported',model:'old-model',seed:42}})]]);
 rerender(<I18nProvider><GenerationMetadata job={null} media={media('new.png')}/></I18nProvider>);
 expect(await screen.findByText('<img src=x> imported')).toBeTruthy();expect(container.querySelector('img')).toBeNull();
 await act(async()=>finishOld([['parameters','late old image']]));
 expect(screen.queryByText('late old image')).toBeNull();
 vi.mocked(readImageMetadata).mockResolvedValueOnce([]);
 rerender(<I18nProvider><GenerationMetadata job={null} media={media('empty.png')}/></I18nProvider>);
 expect(screen.queryByText('old-model')).toBeNull();expect(await screen.findByText('No embedded metadata found.')).toBeTruthy();
 vi.mocked(readImageMetadata).mockRejectedValueOnce(new Error('bad chunk'));
 rerender(<I18nProvider><GenerationMetadata job={null} media={media('bad.png')}/></I18nProvider>);
 expect(await screen.findByText('Media metadata could not be read.')).toBeTruthy();
 rerender(<I18nProvider><GenerationMetadata job={job}/></I18nProvider>);
 expect(screen.getByText('Krea2/model.safetensors')).toBeTruthy();
});

const aceIntent={recipeId:'music-generation',version:1,family:'ace-step-1-5-xl-sft',mode:'t2m',planning:'off',checkpoint:'ace-xl.safetensors',style:'jazz 猫',lyrics:'',seed:42,maxDuration:30,steps:37,cfg:4.25,sampler:'euler',scheduler:'simple',outputPrefix:'raw/audio/exact',ace:{shift:2.5,lyricsStrength:1,bpm:123,timeSignature:'3',language:'de',keyScale:'D minor',generateAudioCodes:false,lmCfg:3.5,temperature:0.7,topP:0.8,topK:12,minP:0.03},diffusion:{textEncoder:'text.safetensors',textEncoder2:'code.safetensors',vae:'audio-vae.safetensors'},audioEnhance:{mode:'clean',profile:'mix'}} as const;

test('ACE metadata displays submitted identity, components, sampling and LM settings without YuE2 ABC rows',()=>{
 localStorage.setItem('deas.locale.v1','en');
 const before=JSON.stringify(aceIntent),view=render(<I18nProvider><GenerationMetadata job={{...job,intent:aceIntent}}/></I18nProvider>);
 const meta=view.container.querySelector('[data-music-metadata]')!;
 for(const value of [aceIntent.family,aceIntent.checkpoint,...Object.values(aceIntent.diffusion),'37','4.25','euler','simple','2.5','123','D minor','3.5','0.03','13.31 sec'])expect(meta.textContent).toContain(value);
 expect(meta.textContent).not.toMatch(/YuE2|ABC|SheetSage/);expect(view.container.querySelector('[data-metadata-apply]')).toBeNull();expect(JSON.stringify(aceIntent)).toBe(before);
 view.rerender(<I18nProvider><GenerationMetadata job={{...job,intent:{...aceIntent,family:'ace-step-1-0',diffusion:undefined}}}/></I18nProvider>);
 expect(meta.textContent).not.toContain('code.safetensors');expect(meta.textContent).not.toContain('ace-step-1-5-xl-sft');expect(meta.textContent).toContain('ace-step-1-0');
});
