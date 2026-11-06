import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, expect, test } from 'vitest';
import { I18nProvider } from '../../i18n/I18nProvider';
import { RunPlan } from './RunPlan';
import { GenerationMetadata } from './GenerationMetadata';
import type { VideoIntentV1 } from '../../lib/videoIntent';
import type { DiagnosticImageIntentV1 } from '../../lib/execution';
afterEach(cleanup);
const intent:DiagnosticImageIntentV1={recipeId:'checkpoint-text-to-image-diagnostic',version:1,checkpoint:'test.safetensors',positivePrompt:'forest',negativePrompt:'blur',seed:42,width:896,height:1152,steps:20,cfg:5,sampler:'euler',scheduler:'normal',outputPrefix:'image/42',inputImage:'source.png',denoise:0.6,inputResize:{width:896,height:1152,crop:'center'},loras:[{name:'style.safetensors',strength:0.7}],refine:{width:1792,height:2304,denoise:0.25,tiled:{tileWidth:1024,tileHeight:1024,overlap:128}}};
test.each(['ltx-2','minimax-h3'] as const)('latent post stage and held metadata show final canvas without changing source or audio (%s)',family=>{
 localStorage.setItem('deas.locale.v1','en');
 const video:VideoIntentV1={recipeId:'video-generation',version:1,positivePrompt:'film',negativePrompt:'',model:'model',textEncoder:'text',vae:'v',seed:7,width:768,height:512,frames:family==='ltx-2'?121:124,frameRate:24,steps:8,cfg:1,sampler:'euler',scheduler:'simple',outputPrefix:'raw/video/7',output:{codec:'Auto',container:'Auto',bitDepth:'Auto',quality:20,audioCodec:'AAC',audioBitrate:'192k',saveFirstFrame:false,saveLastFrame:false},...(family==='ltx-2'?{family,ltx:{passes:3,audioVae:'audio',textProjection:'projection',spatialUpscaler:'generation-upscale'},latentUpscale:{model:'post.safetensors',scale:2}}:{family,h3:{mode:'T2VA',audioVae:'audio',shiftVideo:10,shiftAudio:3,references:[]},latentUpscale:{model:'post.safetensors',scale:2,steps:1,denoise:0.2}})};
 const original=JSON.stringify(video);
 const view=render(<I18nProvider><RunPlan pipeline="t2v" intent={video} notices={[]}/></I18nProvider>);
 expect(view.container.querySelector('[data-plan-stage="latentUpscale"]')?.textContent).toContain('post.safetensors');
 expect(view.container.querySelector('[data-plan-stage="output"]')?.textContent).toContain('1536 × 1024');
 expect(view.container.querySelector('[data-plan-stage="audio"]')?.textContent).toContain('audio');
 view.rerender(<I18nProvider><GenerationMetadata job={{id:'v',baseUrl:'http://localhost',promptId:'p',state:'succeeded',error:null,generationReady:false,outputs:[],intent:video}}/></I18nProvider>);
 expect(view.container.textContent).toContain('post.safetensors');expect(view.container.textContent).toContain('1536 × 1024');
 expect(JSON.stringify(video)).toBe(original);
});

test('read-only ordered stages show effective image settings and no inactive video stages',()=>{
 localStorage.setItem('deas.locale.v1','en');
 const view=render(<I18nProvider><RunPlan intent={intent} pipeline="i2i" notices={['Backend validation pending']} outputRoot="/output"/></I18nProvider>);
 const plan=screen.getByRole('region',{name:'Run Plan'});
 expect([...plan.querySelectorAll('[data-plan-stage]')].map(x=>x.getAttribute('data-plan-stage'))).toEqual(['input','model','prompt','generate','refine','output']);
 for(const text of ['896 × 1152','1792 × 2304','source.png','style.safetensors','0.7','forest','/output','Backend validation pending'])expect(plan.textContent).toContain(text);
 expect(view.container.querySelector('input,select,textarea,button')).toBeNull();
 expect(view.container.querySelector('[data-plan-stage="audio"]')).toBeNull();
});
test('video family passes, audio and exact output choices are visible without image refinement',()=>{
 localStorage.setItem('deas.locale.v1','en');
 render(<I18nProvider><RunPlan pipeline="t2v" notices={[]} intent={{recipeId:'video-generation',version:1,family:'ltx-2',model:'ltx.safetensors',textEncoder:'gemma',vae:'v',seed:7,width:768,height:512,frames:121,frameRate:24,steps:8,cfg:1,sampler:'euler',scheduler:'normal',positivePrompt:'a film',negativePrompt:'',outputPrefix:'video/7',output:{codec:'H.264',container:'MP4',bitDepth:'8-bit',quality:20,audioCodec:'AAC',audioBitrate:'192k',saveFirstFrame:true,saveLastFrame:false},ltx:{passes:2,audioVae:'audio.safetensors',textProjection:'projection',spatialUpscaler:'upscale'}}}/></I18nProvider>);
 const plan=screen.getByRole('region',{name:'Run Plan'});
 for(const text of ['121','24','H.264','MP4','AAC','audio.safetensors','upscale'])expect(plan.textContent).toContain(text);
 expect(plan.querySelector('[data-plan-stage="refine"]')).toBeNull();
 expect(plan.querySelector('[data-plan-stage="audio"]')).toBeTruthy();
});

test('image prompt references appear as ordered inputs without an image-to-image source',()=>{
 localStorage.setItem('deas.locale.v1','en');
 const {inputImage,denoise,inputResize,...base}=intent;void inputImage;void denoise;void inputResize;
 const native={...base,nativeImageReference:{method:'qwen-image-2.1' as const,images:['first.png','second.png']}};
 const view=render(<I18nProvider><RunPlan intent={native} pipeline="t2i" notices={[]}/></I18nProvider>);
 const input=view.container.querySelector('[data-plan-stage="input"]');
 expect(input?.textContent).toContain('first.png');
 expect(input?.textContent).toContain('second.png');
 view.rerender(<I18nProvider><RunPlan intent={{...base,imageReference:{visionModel:'bigG.safetensors',images:[{image:'style.png',strength:0.6}]}}} pipeline="t2i" notices={[]}/></I18nProvider>);
 expect(view.container.querySelector('[data-plan-stage="input"]')?.textContent).toContain('style.png');
 expect(view.container.textContent).toContain('bigG.safetensors');
});
test('removing refinement removes its stage and unresolved outputs are not readiness claims',()=>{
 localStorage.setItem('deas.locale.v1','en');
 const {refine,...plain}=intent;void refine;
 const view=render(<I18nProvider><RunPlan intent={plain} pipeline="i2i" notices={['Missing mask']} inputNotes={['Mask upload pending; blur 4 px, crop margin 8 px']}/></I18nProvider>);
 expect(view.container.querySelector('[data-plan-stage="refine"]')).toBeNull();
 expect(view.container.textContent).toContain('Mask upload pending');
 expect(view.container.textContent).toContain('Unresolved / not selected');
 expect(view.container.textContent).toContain('Missing mask');
 view.rerender(<I18nProvider><RunPlan intent={null} pipeline="t2v" notices={['Missing model']}/></I18nProvider>);
 expect(screen.getByRole('status').textContent).toBe('Unresolved / not selected');
 expect(view.container.querySelector('[data-plan-stage]')).toBeNull();
});

test('music plans show input/ABC/generate stages from the music intent and no image or video rows',()=>{
 const cover={recipeId:'music-generation',version:1,family:'yue2',mode:'cover',planning:'melody',checkpoint:'yue2.safetensors',style:'bossa nova',lyrics:'',seed:7,maxDuration:360,steps:32,cfg:1,sampler:'dpm_2',scheduler:'sgm_uniform',generation:{temperature:1,topP:0.95,topK:100,repetitionPenalty:1.2,cfgScale:1},outputPrefix:'raw/audio/c',sourceAudio:'song.wav',audioEncoder:'sheetsage2_bf16.safetensors'} as const;
 const {rerender}=render(<I18nProvider><RunPlan intent={cover} pipeline="cover" notices={[]} outputRoot="/out"/></I18nProvider>);
 const stages=()=>[...document.querySelectorAll('[data-plan-stage]')].map(el=>el.getAttribute('data-plan-stage'));
 expect(stages()).toEqual(['input','model','prompt','abc','generate','output']);
 expect(screen.getByText('song.wav')).toBeTruthy();expect(screen.getByText('sheetsage2_bf16.safetensors')).toBeTruthy();expect(screen.getByText('bossa nova')).toBeTruthy();
 expect(screen.getByText('FLAC')).toBeTruthy();expect(screen.queryByText(/×/)).toBeNull();expect(screen.queryByText('Negative prompt')).toBeNull();
 const off={...cover,mode:'t2m',planning:'off',sourceAudio:undefined,audioEncoder:undefined} as unknown as typeof cover;
 rerender(<I18nProvider><RunPlan intent={off} pipeline="t2m" notices={[]} outputRoot="/out"/></I18nProvider>);
 expect(stages()).toEqual(['model','prompt','generate','output']);
 rerender(<I18nProvider><GenerationMetadata job={{id:'j',baseUrl:'http://x/',promptId:'p',state:'succeeded',outputs:[],error:null,generationReady:false,intent:cover}}/></I18nProvider>);
 const meta=document.querySelector('[data-music-metadata]')!;
 expect(meta.textContent).toContain('bossa nova');expect(meta.textContent).toContain('song.wav');expect(meta.textContent).toContain('Melody');
 expect(document.querySelector('[data-metadata-apply]')).toBeNull();
});

const aceIntent={recipeId:'music-generation',version:1,family:'ace-step-1-5-xl-sft',mode:'t2m',planning:'off',checkpoint:'ace-xl.safetensors',style:'jazz 猫',lyrics:'',seed:42,maxDuration:30,steps:37,cfg:4.25,sampler:'euler',scheduler:'simple',outputPrefix:'raw/audio/exact',ace:{shift:2.5,lyricsStrength:1,bpm:123,timeSignature:'3',language:'de',keyScale:'D minor',generateAudioCodes:false,lmCfg:3.5,temperature:0.7,topP:0.8,topK:12,minP:0.03},diffusion:{textEncoder:'text.safetensors',textEncoder2:'code.safetensors',vae:'audio-vae.safetensors'},audioEnhance:{mode:'clean',profile:'mix'}} as const;

test('ACE plan shows exact family, split components, diffusion and LM settings, FLAC and postprocessing without ABC',()=>{
 localStorage.setItem('deas.locale.v1','en');
 const before=JSON.stringify(aceIntent),view=render(<I18nProvider><RunPlan intent={aceIntent} pipeline="t2m" notices={[]} outputRoot="/audio"/></I18nProvider>);
 expect([...view.container.querySelectorAll('[data-plan-stage]')].map(el=>el.getAttribute('data-plan-stage'))).toEqual(['model','prompt','generate','audioEnhance','output']);
 const model=view.container.querySelector('[data-plan-stage="model"]')!;
 for(const value of [aceIntent.family,aceIntent.checkpoint,...Object.values(aceIntent.diffusion)])expect(model.textContent).toContain(value);
 const generate=view.container.querySelector('[data-plan-stage="generate"]')!;
 for(const value of ['37','4.25','euler','simple','2.5','123','D minor','3.5','0.03'])expect(generate.textContent).toContain(value);
 expect(view.container.querySelector('[data-plan-stage="output"]')?.textContent).toContain('FLAC');
 expect(view.container.querySelector('[data-plan-stage="output"]')?.textContent).toContain('raw/audio/exact');
 expect(view.container.textContent).not.toMatch(/YuE2|ABC|SheetSage/);expect(JSON.stringify(aceIntent)).toBe(before);
 expect(JSON.parse(view.container.querySelector('[data-plan-intent]')!.textContent!)).toEqual(aceIntent);
 view.rerender(<I18nProvider><RunPlan intent={{...aceIntent,diffusion:undefined}} pipeline="t2m" notices={[]}/></I18nProvider>);
 expect(view.container.querySelector('[data-plan-stage="model"]')?.textContent).not.toContain('code.safetensors');
});
