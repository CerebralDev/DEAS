import { useState } from 'react';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { Workbench } from './Workbench';
import { defaultOptions } from './OptionsPanel';
import {normalizeAceDuration,defaultAceSettings,type AceStepFamily} from '../../lib/musicIntent';
import { applyStudioSettings, captureStudioSettings } from '../../lib/preferences';
import { aspects, resolutions } from '../../lib/resolution';
import type { HistoryRecord } from './history';
import type { ForgeCanvasContext } from '../../lib/forgeCanvas';
import { I18nProvider } from '../../i18n/I18nProvider';
import { checkRecipePrerequisites } from '../../lib/recipePrerequisites';
import { scanModels, writeModelFamily } from '../../lib/models';
import { autoRefine, waitForWorkbenchJob, stopWorkbench, stopWorkbenchJob, runWorkbenchJob, readCurrentWorkbench, pollWorkbench, uploadMask, uploadInput, readH3Session, prepareH3Video, readRefMods, loadDiagnosticJobOutput, exportDiagnosticJobOutput, type JobStatus } from '../../lib/jobs';
import { readImageMetadata } from '../../lib/imageMetadata';
vi.mock('@tauri-apps/api/core',()=>({isTauri:vi.fn(()=>true),convertFileSrc:(p:string)=>`asset://localhost/${p}`,invoke:vi.fn(async()=>({path:'/env/ComfyUI/output',parent:'/env/ComfyUI',entries:[],truncated:false}))}));
vi.mock('@tauri-apps/api/window',()=>({getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}})}));
const nativeDrops=vi.hoisted(()=>new Set<(event:{payload:{type:'drop';paths:string[];position:{x:number;y:number}}})=>void>());
const nativeDropFailure=vi.hoisted(()=>({sync:false,async:false}));
vi.mock('@tauri-apps/api/webview',()=>({getCurrentWebview:()=>{if(nativeDropFailure.sync)throw Error('missing webview');return {onDragDropEvent:async(handler:(event:{payload:{type:'drop';paths:string[];position:{x:number;y:number}}})=>void)=>{if(nativeDropFailure.async)throw Error('unavailable listener');nativeDrops.add(handler);return()=>nativeDrops.delete(handler);}};}}));
vi.mock('../../lib/recipePrerequisites',()=>({checkRecipePrerequisites:vi.fn(async()=>{throw new Error('offline');})}));
vi.mock('../../lib/models',()=>({scanModels:vi.fn(),writeModelFamily:vi.fn()}));
vi.mock('../../lib/imageMetadata',async importOriginal=>({...await importOriginal<typeof import('../../lib/imageMetadata')>(),readImageMetadata:vi.fn(async()=>[] as [string,string][])}));
vi.mock('../../lib/jobs',()=>({autoRefine:vi.fn(),waitForWorkbenchJob:vi.fn(),stopWorkbenchJob:vi.fn(),uploadMask:vi.fn(),uploadInput:vi.fn(),readH3Session:vi.fn(),prepareH3Video:vi.fn(),readRefMods:vi.fn(),stopWorkbench:vi.fn(),runWorkbenchJob:vi.fn(),readCurrentWorkbench:vi.fn(),pollWorkbench:vi.fn(),loadDiagnosticJobOutput:vi.fn(),exportDiagnosticJobOutput:vi.fn(),isImageIntent:(i:{recipeId?:string})=>i?.recipeId==='checkpoint-text-to-image-diagnostic'}));
const collection=(jobs:JobStatus[]=[],preparing=0)=>({jobs,preparing,capacity:20,stopAvailable:true,admissionEpoch:0,stopping:false});
const evidence={baseUrl:'http://127.0.0.1:7801/',recipeId:'checkpoint-text-to-image-diagnostic' as const,version:1 as const,missingNodes:[],generationReady:false as const,
 allowlist:{nodes:['CheckpointLoaderSimple','CLIPTextEncode','EmptyLatentImage','KSampler','VAEDecode','SaveImage'],checkpoints:['a.safetensors','b.safetensors'],loras:['style.safetensors'],samplers:['euler','euler_ancestral','euler_cfg_pp','er_sde','dpmpp_2m','dpm_2','res_multistep','uni_pc','lcm'],schedulers:['normal','simple','linear_quadratic','karras','sgm_uniform','beta']}};
beforeEach(()=>{
 nativeDrops.clear();nativeDropFailure.sync=false;nativeDropFailure.async=false;
 vi.mocked(stopWorkbench).mockResolvedValue({...collection(),admissionEpoch:1,outcome:'empty',discarded:0,issues:[]});
 vi.mocked(readCurrentWorkbench).mockImplementation(async()=>{
  const submitted=await Promise.all(vi.mocked(runWorkbenchJob).mock.results.filter(result=>result.type==='return').map(async result=>{try{return await result.value as JobStatus;}catch{return null;}}));
  return collection([...new Map(submitted.filter((job):job is JobStatus=>!!job).map(job=>[job.id,job])).values()]);
 });
 vi.mocked(pollWorkbench).mockResolvedValue(collection());
 localStorage.clear();localStorage.setItem('deas.locale.v1','en');vi.mocked(isTauri).mockReturnValue(true);
 Object.defineProperty(HTMLDialogElement.prototype,'showModal',{configurable:true,value:function(this:HTMLDialogElement){this.open=true;}});
 Object.defineProperty(HTMLDialogElement.prototype,'close',{configurable:true,value:function(this:HTMLDialogElement){this.open=false;}});
 vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue({save:vi.fn(),restore:vi.fn(),arc:vi.fn(),fill:vi.fn(),beginPath:vi.fn(),clearRect:vi.fn(),fillRect:vi.fn(),drawImage:vi.fn(),getImageData:vi.fn(()=>({data:new Uint8ClampedArray([0,0,0,0])}))} as unknown as CanvasRenderingContext2D);
});
afterEach(()=>{cleanup();Reflect.deleteProperty(HTMLDialogElement.prototype,'showModal');Reflect.deleteProperty(HTMLDialogElement.prototype,'close');Reflect.deleteProperty(HTMLVideoElement.prototype,'src');vi.clearAllMocks();vi.unstubAllGlobals();});
test('SAM refinement defaults off, preserves saved OFF/ON and uses an explicit checkpoint',async()=>{
 const sam='SAM3/sam3.1_multiplex_fp16.safetensors';
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'SAM3_Detect'],checkpoints:[...evidence.allowlist.checkpoints,sam]}});
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',path:'/m/a',family:'sdxl',size:1,info:null}]);
 const view=render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 fireEvent.click(screen.getByRole('tab',{name:'Post-Processing'}));
 const enabled=await waitFor(()=>{const el=document.querySelector<HTMLInputElement>('[data-sam="enabled"]');expect(el).not.toBeNull();return el!;});
 expect(enabled.checked).toBe(false);
 expect(document.querySelector<HTMLInputElement>('[data-sam="denoise"]')!.value).toBe('0.25');
 expect(document.querySelector<HTMLInputElement>('[data-sam="margin"]')!.value).toBe('32');
 await waitFor(()=>expect(document.querySelector<HTMLSelectElement>('[data-sam="model"]')!.options.length).toBe(2));
 expect(document.querySelector<HTMLSelectElement>('[data-sam="model"]')!.value).toBe('');
 fireEvent.click(enabled);expect(enabled.checked).toBe(true);
 expect(autoRefine).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();
 view.unmount();render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 expect(document.querySelector<HTMLInputElement>('[data-sam="enabled"]')!.checked).toBe(true);
 fireEvent.click(document.querySelector('[data-sam="enabled"]')!);
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.samEnabled')!)).toBe(false);
});
test('SAM automatic refinement hands the exact submitted job to the native serial group chain',async()=>{
 const sam='SAM3/sam3.1_multiplex_fp16.safetensors';
 for(const [key,value] of Object.entries({samCheckpoint:sam,samTargets:'eyes, feet; hands',samEnabled:true,samDenoise:0.35,steps:23}))localStorage.setItem('deas.pref.studio.run.'+key,JSON.stringify(value));
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'SAM3_Detect'],checkpoints:[...evidence.allowlist.checkpoints,sam]}});
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',path:'/m/a',family:'sdxl',size:1,info:null}]);
 const job:JobStatus={id:'base-job',baseUrl:evidence.baseUrl,promptId:'prompt',state:'queued',outputs:[],error:null,generationReady:false};
 vi.mocked(runWorkbenchJob).mockResolvedValue(job);vi.mocked(autoRefine).mockResolvedValue({job:null,skipped:0});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 const run=screen.getByRole('button',{name:'Run'});await waitFor(()=>expect(run).toHaveProperty('disabled',false));fireEvent.click(run);
 await waitFor(()=>expect(autoRefine).toHaveBeenCalledTimes(1));
 const [url,intent,settings,fence,sourceJob]=vi.mocked(autoRefine).mock.calls[0];
 expect(url).toBe(evidence.baseUrl);expect(intent).toEqual(vi.mocked(runWorkbenchJob).mock.calls[0][1]);
 expect(settings).toEqual({checkpoint:sam,targets:['eyes','feet','hands'],denoise:0.35,margin:32,threshold:0.5});
 expect(fence).toBe(0);expect(sourceJob).toBe(job.id);expect(waitForWorkbenchJob).not.toHaveBeenCalled();
});
test('model metadata action enables an unknown supported file while GGUF stays usable',async()=>{
 const unknown={category:'diffusion_models',name:'unknown.safetensors',path:'/m/unknown.safetensors',size:1,info:null};
 const bad={...unknown,name:'bad.safetensors',path:'/m/bad.safetensors',family:'anima',metadataError:'invalid sidecar'};
 const gguf={...unknown,name:'declared.gguf',path:'/m/declared.gguf',family:'anima',familyOverride:'anima'};
 vi.mocked(scanModels).mockResolvedValue([unknown,bad,gguf]);
 vi.mocked(writeModelFamily).mockResolvedValue({...unknown,family:'anima',familyOverride:'anima'});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 fireEvent.click(screen.getByRole('tab',{name:'Models'}));
 const tile=await screen.findByRole('button',{name:unknown.name});
 expect(tile.getAttribute('aria-disabled')).toBe('true');
 expect(tile).toHaveProperty('disabled',false);tile.focus();expect(document.activeElement).toBe(tile);
 fireEvent.keyDown(tile,{key:'F10',shiftKey:true});fireEvent.click(screen.getByRole('menuitem',{name:'Edit meta…'}));
 fireEvent.change(screen.getByLabelText('Model family'),{target:{value:'anima'}});fireEvent.click(screen.getByRole('button',{name:'Save'}));
 await waitFor(()=>expect(tile.getAttribute('aria-disabled')).toBe('false'));
 const picker=document.querySelector('[data-model-picker]') as HTMLSelectElement;
 expect(picker.value).toBe(gguf.name);
 expect([...document.querySelector<HTMLSelectElement>('[data-pipeline]')!.options].find(o=>o.value==='t2v')).toHaveProperty('disabled',true);
 expect([...picker.options].find(o=>o.value===bad.name)).toBeUndefined();
 expect([...picker.options].find(o=>o.value===gguf.name)?.disabled).toBe(false);
 expect(tile.textContent).toContain('User-declared family');
 fireEvent.contextMenu(tile);fireEvent.click(screen.getByRole('menuitem',{name:'Edit meta…'}));
 vi.mocked(writeModelFamily).mockResolvedValue(unknown);fireEvent.click(screen.getByRole('button',{name:'Reset to detection'}));
 await waitFor(()=>expect(tile.getAttribute('aria-disabled')).toBe('true'));
 expect([...picker.options].find(o=>o.value===unknown.name)).toBeUndefined();
 // The usable declared GGUF stays selected; Run stays gated by the offline backend, not by the unknown file.
 expect(picker.value).toBe(gguf.name);
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test('Ctrl+Enter presses Run once, and never while Run is disabled, hidden or behind a dialog',async()=>{
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 const view=render(<I18nProvider><div data-host><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></div></I18nProvider>);
 const run=document.querySelector<HTMLButtonElement>('[data-run]')!;
 const prompt=document.querySelector('#run-negative')!;
 const clicked=vi.spyOn(run,'click');clicked.mockImplementation(()=>{});
 await waitFor(()=>expect(run.disabled).toBe(false));
 fireEvent.keyDown(prompt,{key:'Enter'});
 fireEvent.keyDown(prompt,{key:'Enter',ctrlKey:true,repeat:true});
 const dialog=document.body.appendChild(document.createElement('dialog'));dialog.setAttribute('open','');
 fireEvent.keyDown(prompt,{key:'Enter',ctrlKey:true});dialog.remove();
 const host=view.container.querySelector<HTMLElement>('[data-host]')!;host.hidden=true;
 fireEvent.keyDown(prompt,{key:'Enter',ctrlKey:true});host.hidden=false;
 expect(clicked).not.toHaveBeenCalled();
 fireEvent.keyDown(prompt,{key:'Enter',ctrlKey:true});
 expect(clicked).toHaveBeenCalledTimes(1);
});
test('checkpoint picker stays a compact native select without a custom filter or list',async()=>{
 const files=['anima_a','anima_b','illu_c','krea_d','wan_e','ltx_f','sdxl_g','pony_h','moss_i'].map(n=>({category:'checkpoints',name:`${n}.safetensors`,path:`/m/${n}.safetensors`,size:1,info:null,family:'sdxl'}));
 vi.mocked(scanModels).mockResolvedValue(files);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 const picker=await waitFor(()=>{const p=document.querySelector<HTMLSelectElement>('[data-model-picker]')!;expect(p.options.length).toBe(9);return p;});
 expect(picker.tagName).toBe('SELECT');expect(picker.multiple).toBe(false);expect(picker.size).toBe(0);
 expect(fireEvent.mouseDown(picker)).toBe(true);
 expect(fireEvent.keyDown(picker,{key:'ArrowDown'})).toBe(true);
 expect(document.querySelector('[data-model-filter]')).toBeNull();
 expect(document.querySelector('[data-model-list]')).toBeNull();
 expect(picker.options.length).toBe(9);
 fireEvent.change(picker,{target:{value:'krea_d.safetensors'}});
 expect(picker.value).toBe('krea_d.safetensors');
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.checkpoint')!)).toBe('krea_d.safetensors');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test('checkpoint picker filters mixed inventories on image/video pipeline changes and restores drafts',async()=>{
 const files=[
  {category:'checkpoints',name:'image.safetensors',family:'sdxl'},
  {category:'checkpoints',name:'unknown.safetensors'},
  {category:'checkpoints',name:'wrong-folder.safetensors',family:'anima'},
  {category:'diffusion_models',name:'video.safetensors',family:'ltx-2'},
  {category:'diffusion_models',name:'unknown-split.safetensors'},
  {category:'diffusion_models',name:'invalid.safetensors',family:'anima',metadataError:'invalid sidecar'},
 ].map(m=>({...m,path:'/m/'+m.name,size:1,info:null}));
 vi.mocked(scanModels).mockResolvedValue(files);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 const picker=document.querySelector<HTMLSelectElement>('[data-model-picker]')!;
 const values=()=>[...picker.options].map(o=>o.value);
 await waitFor(()=>expect(values()).toEqual(['image.safetensors','unknown.safetensors']));
 fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'  image draft\n'}});
 fireEvent.change(screen.getByLabelText('Negative prompt'),{target:{value:''}});
 fireEvent.change(picker,{target:{value:'unknown.safetensors'}});
 expect(screen.getByLabelText('Prompt')).toHaveProperty('value','  image draft\n');
 expect(screen.getByLabelText('Negative prompt')).toHaveProperty('value','');
 fireEvent.change(document.querySelector('[data-pipeline]')!,{target:{value:'t2v'}});
 expect(values()).toEqual(['video.safetensors']);expect(picker.value).toBe('video.safetensors');
 fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'video draft'}});
 fireEvent.change(document.querySelector('[data-pipeline]')!,{target:{value:'t2i'}});
 expect(values()).toEqual(['image.safetensors','unknown.safetensors']);expect(picker.value).toBe('unknown.safetensors');
 expect(screen.getByLabelText('Prompt')).toHaveProperty('value','  image draft\n');
 expect(screen.getByLabelText('Negative prompt')).toHaveProperty('value','');
 fireEvent.change(document.querySelector('[data-pipeline]')!,{target:{value:'t2v'}});
 expect(screen.getByLabelText('Prompt')).toHaveProperty('value','video draft');
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
});
test.each(['missing','incompatible','invalid','wrong-folder','unsupported','empty'] as const)('checkpoint picker retains a saved %s selection with its disabled reason',async kind=>{
 const name='saved.safetensors';
 localStorage.setItem('deas.pref.studio.run.checkpoint',JSON.stringify(name));
 const saved={category:kind==='incompatible'?'diffusion_models':'checkpoints',name,path:'/m/'+name,size:1,info:null,...(kind==='incompatible'?{family:'ltx-2'}:kind==='invalid'?{metadataError:'bad sidecar'}:kind==='wrong-folder'?{family:'anima'}:{})};
 const selected=kind==='unsupported'?{...saved,name:'saved.gguf',family:'sdxl'}:saved;
 localStorage.setItem('deas.pref.studio.run.checkpoint',JSON.stringify(selected.name));
 vi.mocked(scanModels).mockResolvedValue(kind==='empty'?[]:[{category:'checkpoints',name:'compatible.safetensors',path:'/m/compatible',size:1,info:null},...(kind==='missing'?[]:[selected])]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await act(async()=>{});
 const picker=document.querySelector<HTMLSelectElement>('[data-model-picker]')!;
 const option=picker.selectedOptions[0];
 expect(picker.value).toBe(selected.name);expect(option.disabled).toBe(true);
 expect(option.title).toBeTruthy();expect(option.textContent).toContain(option.title);expect(picker.title).toBe(option.title);
 expect(option.title).toMatch(kind==='incompatible'?/recipe/i:kind==='invalid'?/metadata/i:kind==='wrong-folder'?/diffusion_models/i:kind==='unsupported'?/GGUF/i:/unavailable/i);
 expect([...picker.options].map(o=>o.value)).toEqual(kind==='empty'?[selected.name]:[selected.name,'compatible.safetensors']);
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.checkpoint')!)).toBe(selected.name);
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test.each([false,true])('checkpoint picker shows the disabled empty state without compatible models (empty inventory: %s)',async empty=>{
 vi.mocked(scanModels).mockResolvedValue(empty?[]:[{category:'diffusion_models',name:'video.safetensors',family:'ltx-2',path:'/m/video',size:1,info:null}]);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await act(async()=>{});
 const picker=document.querySelector<HTMLSelectElement>('[data-model-picker]')!;
 expect([...picker.options].map(o=>o.value)).toEqual(['']);expect(picker.selectedOptions[0].disabled).toBe(true);
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);
});
test('declared GGUF runs through the GGUF loader while invalid metadata fails closed',async()=>{
 const file={category:'diffusion_models',name:'declared.gguf',path:'/m/declared.gguf',size:1,info:null,family:'anima',familyOverride:'anima'};
 const companions=[{category:'text_encoders',name:'qwen_3_600m.gguf',path:'/m/text_encoders/qwen_3_600m.gguf',size:1,info:null},{category:'vae',name:'wan_2.1_vae.safetensors',path:'/m/vae/wan_2.1_vae.safetensors',size:1,info:null}];
 vi.mocked(scanModels).mockResolvedValue([file,...companions]);vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'UnetLoaderGGUF','CLIPLoaderGGUF','VAELoader'],ggufDiffusionModels:[file.name],ggufTextEncoders:[companions[0].name],ggufClipTypes:['stable_diffusion'],vaes:[companions[1].name]}});
 const view=render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-model-picker]')?.textContent).toContain(file.name));
 expect([...document.querySelector<HTMLSelectElement>('[data-model-picker]')!.options].find(o=>o.value===file.name)).toHaveProperty('disabled',false);
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',false);
 view.unmount();
 vi.mocked(scanModels).mockResolvedValue([{...file,category:'checkpoints'},...companions]);
 localStorage.setItem('deas.pref.studio.run.checkpoint',JSON.stringify(file.name));
 const second=render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-model-picker]')?.textContent).toContain(file.name));
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);
 second.unmount();
 localStorage.setItem('deas.pref.studio.run.checkpoint',JSON.stringify('invalid.safetensors'));
 vi.mocked(scanModels).mockResolvedValue([{...file,name:'invalid.safetensors',path:'/m/invalid.safetensors',metadataError:'invalid metadata'}]);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);await waitFor(()=>expect(document.querySelector('[data-model-picker]')?.textContent).toContain('invalid.safetensors'));
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test.each(['node','file','encoder','vae','plain-base'])('GGUF Run stays blocked when quantized backend %s evidence is missing',async missing=>{
 const file={category:'diffusion_models',name:missing==='plain-base'?'base.safetensors':'base.gguf',path:'/m/base',size:1,info:null,family:'anima'};
 const encoder=missing==='plain-base'?'qwen_3_600m.gguf':'qwen_3_600m.safetensors';
 vi.mocked(scanModels).mockResolvedValue([file,{category:'text_encoders',name:encoder,path:'/m/'+encoder,size:1,info:null},{category:'vae',name:'wan_2.1_vae.safetensors',path:'/m/wan_2.1_vae.safetensors',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'UNETLoader','CLIPLoader','CLIPLoaderGGUF','VAELoader',...(missing==='node'?[]:['UnetLoaderGGUF'])],ggufDiffusionModels:missing==='file'?[]:[file.name],diffusionModels:[],textEncoders:missing==='encoder'?[]:[encoder],ggufTextEncoders:[encoder],clipTypes:['stable_diffusion'],ggufClipTypes:['stable_diffusion'],vaes:missing==='vae'?[]:['wan_2.1_vae.safetensors']}});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-model-picker]')?.textContent).toContain(file.name));
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test('Wan LOW metadata errors remain visible dependencies of the selected HIGH model',async()=>{
 localStorage.setItem('deas.pref.studio.run.pipeline','"i2v"');
 const file={category:'diffusion_models',name:'wan_high.gguf',path:'/m/wan_high.gguf',size:1,info:null,family:'wan'};
 vi.mocked(scanModels).mockResolvedValue([file,{...file,name:'wan_low.gguf',path:'/m/wan_low.gguf',metadataError:'stale LOW sidecar'}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'UnetLoaderGGUF'],ggufDiffusionModels:['wan_high.gguf','wan_low.gguf']}});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLSelectElement>('[data-model-picker]')?.value).toBe(file.name));
 expect(screen.getByText(/Invalid model metadata blocks generation/)).toBeTruthy();
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test.each(['metadata','gguf'])('selected LoRA %s errors block Run rather than silently dropping the adapter' ,async issue=>{
 const name=issue==='gguf'?'bad.gguf':'bad.safetensors';
 localStorage.setItem('deas.pref.studio.run.loras',JSON.stringify([{name,strength:1}]));
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',path:'/m/a.safetensors',size:1,info:null},{category:'loras',name,path:'/m/'+name,size:1,info:null,...(issue==='metadata'?{metadataError:'invalid metadata'}:{family:'sdxl',familyOverride:'sdxl'})}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-model-picker]')?.textContent).toContain('a.safetensors'));
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);expect(screen.getByRole('alert').textContent).toContain(name);
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});
const heldJob=(id:string,state:JobStatus['state'],baseUrl='http://original:7801/'):JobStatus=>({id,state,baseUrl,promptId:'p-'+id,outputs:[],error:null,generationReady:false});
test('automatic audio readiness blocks video while probe is pending/missing; Off restores ordinary Run',async()=>{
 let resolve!: (value:unknown)=>void;
 localStorage.setItem('deas.pref.studio.run.pipeline',JSON.stringify('t2v'));
 localStorage.setItem('deas.pref.studio.audioEnhance.post.mode',JSON.stringify('stems'));
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(invoke).mockImplementation(async command=>command==='native_audio_enhance_capabilities'?new Promise(r=>{resolve=r;}):{path:'/out',parent:'/',entries:[],truncated:false});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-model-picker]')?.textContent).toContain('LTX2/ltx.safetensors'));
 const run=screen.getByRole('button',{name:'Run'});
 expect(run).toHaveProperty('disabled',true);
 await act(async()=>{resolve({clean:true,deep:false,stems:false});});
 expect(run).toHaveProperty('disabled',true);
 expect(run.title).toContain('Audio enhancement is not ready');
 fireEvent.click(screen.getByRole('tab',{name:'Post-Processing'}));
 fireEvent.change(document.querySelector('[data-audio-enhance-mode]')!,{target:{value:'off'}});
 await waitFor(()=>expect(run).toHaveProperty('disabled',false));
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test.each((['ltx-2','minimax-h3','yue2'] as const).flatMap(family=>(['off','clean','deep','stems'] as const).flatMap(mode=>(mode==='stems'?['auto','cpu','cuda'] as const:['auto'] as const).map(device=>({family,mode,device})))))('automatic audio $family / $mode / $device reaches Run Plan and immutable native Run',async({family,mode,device})=>{
 const h3=family==='minimax-h3',musicFamily=family==='yue2';
 localStorage.setItem('deas.pref.studio.audioEnhance.post.device',JSON.stringify(device));
 const files=musicFamily?[...yue2Models]:h3?[
  {category:'diffusion_models',name:'h3.safetensors',family},
  {category:'text_encoders',name:'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors'},
  {category:'vae',name:'minimax_h3_video_vae_fp16.safetensors'},
  {category:'vae',name:'minimax_h3_audio_vae_fp32.safetensors'}
 ].map(m=>({...m,path:'/m/'+m.name,size:1,info:null})):[...ltxModels];
 vi.mocked(scanModels).mockResolvedValue(files);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(musicFamily?musicEvidence():evidence);
 vi.mocked(invoke).mockImplementation(async command=>command==='native_audio_enhance_capabilities'?{clean:true,deep:true,stems:true,cuda:true}:command==='native_jobs_history'?{schemaVersion:1,records:[],readOnly:false,error:null}:{path:'/out',parent:'/',entries:[],truncated:false});
 vi.mocked(runWorkbenchJob).mockResolvedValue({...heldJob('audio-chain','failed'),error:'fixture'});
 localStorage.setItem('deas.pref.studio.run.pipeline',JSON.stringify(musicFamily?'t2m':'t2v'));
 const props={endpoint:evidence.baseUrl,roots:{models:'/m'}};
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 if(musicFamily)fireEvent.change(music().style,{target:{value:'indie pop'}});
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',false));
 fireEvent.click(screen.getByRole('tab',{name:'Post-Processing'}));
 await act(async()=>{fireEvent.focus(document.querySelector('[data-audio-enhance-mode]')!);});
 fireEvent.change(document.querySelector('[data-audio-enhance-mode]')!,{target:{value:mode}});
 if(mode==='deep'){
  expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);
  fireEvent.change(document.querySelector('[data-audio-enhance-profile]')!,{target:{value:'speech'}});
 }
 view.rerender(<I18nProvider><Workbench {...props} planVisible/></I18nProvider>);
 const plan=JSON.parse(document.querySelector('[data-plan-intent]')!.textContent!);
 const audio=mode==='off'?undefined:{mode,profile:mode==='deep'?'speech':'mix',...(mode==='stems'?{device}:{})};
 if(audio){expect(plan.audioEnhance).toEqual(audio);expect(document.querySelector('[data-plan-stage="audioEnhance"]')).not.toBeNull();}
 else expect(plan).not.toHaveProperty('audioEnhance');
 view.rerender(<I18nProvider><Workbench {...props}/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 if(mode==='stems')fireEvent.change(document.querySelector('[data-audio-enhance-device]')!,{target:{value:device==='cpu'?'auto':'cpu'}});
 fireEvent.change(document.querySelector('[data-audio-enhance-mode]')!,{target:{value:mode==='off'?'clean':'off'}});
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 const submitted=vi.mocked(runWorkbenchJob).mock.lastCall![1];
 if(audio)expect(submitted).toHaveProperty('audioEnhance',audio);else expect(submitted).not.toHaveProperty('audioEnhance');
 expect(invoke).not.toHaveBeenCalledWith('native_audio_enhance_export',expect.anything());
});

test.each((['ltx-2','minimax-h3'] as const).flatMap(family=>([undefined,true,false,'missing'] as const).map(preference=>({family,preference}))))('enhanced preview $family / $preference shares Run Plan and immutable Run',async({family,preference})=>{
 const h3=family==='minimax-h3';
 const config={decoder:'tiny' as const,model:h3?'taeh3.safetensors':'taeltx.safetensors'};
 const files=h3?[
  {category:'diffusion_models',name:'h3.safetensors',family},
  {category:'text_encoders',name:'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors'},
  {category:'vae',name:'minimax_h3_video_vae_fp16.safetensors'},
  {category:'vae',name:'minimax_h3_audio_vae_fp32.safetensors'}
 ].map(m=>({...m,path:'/m/'+m.name,size:1,info:null})):[...ltxModels];
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 if(typeof preference==='boolean')localStorage.setItem('deas.pref.studio.run.options',JSON.stringify({...defaultOptions,enhancedPreview:preference}));
 vi.mocked(scanModels).mockResolvedValue(files);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,videoPreviews:preference==='missing'?{}:{[family]:config}}});
 vi.mocked(runWorkbenchJob).mockResolvedValue({...heldJob('preview-run','failed'),error:'fixture'});
 const props={endpoint:evidence.baseUrl,roots:{models:'/m'}};
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',false));
 fireEvent.click(screen.getByRole('tab',{name:'Options'}));
 const toggle=screen.getByRole('switch',{name:'Enhanced preview'});
 const enabled=preference!==false&&preference!=='missing';
 expect(toggle).toHaveProperty('checked',enabled);
 expect(toggle).toHaveProperty('disabled',preference==='missing');
 view.rerender(<I18nProvider><Workbench {...props} planVisible/></I18nProvider>);
 const planned=JSON.parse(document.querySelector('[data-plan-intent]')!.textContent!);
 if(enabled)expect(planned.enhancedPreview).toEqual(config);else expect(planned).not.toHaveProperty('enhancedPreview');
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
 view.rerender(<I18nProvider><Workbench {...props}/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 if(preference!=='missing')fireEvent.click(screen.getByRole('switch',{name:'Enhanced preview'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledOnce());
 const submitted=vi.mocked(runWorkbenchJob).mock.lastCall![1];
 if(enabled)expect(submitted).toHaveProperty('enhancedPreview',config);else expect(submitted).not.toHaveProperty('enhancedPreview');
});

test('enhanced preview follows only the selected running video and cleans up on OFF, browsing, hidden panes, overlays and terminal selection',async()=>{
 const config={decoder:'tiny' as const,model:'taeltx.safetensors'};
 const previewImages:{src:string;onload:unknown;onerror:unknown}[]=[];
 vi.stubGlobal('Image',class{src='';complete=false;onload=null;onerror=null;constructor(){previewImages.push(this);}});
 const intent={recipeId:'video-generation',version:1,family:'ltx-2',positivePrompt:'preview',negativePrompt:'',model:ltxModels[0].name,textEncoder:ltxModels[1].name,vae:ltxModels[3].name,seed:42,width:768,height:512,frames:49,frameRate:24,steps:8,cfg:1,sampler:'euler',scheduler:'simple',outputPrefix:'raw/preview',output:defaultOptions.output!,ltx:{textProjection:ltxModels[2].name,audioVae:ltxModels[4].name,passes:1},enhancedPreview:config} as const;
 const running:JobStatus={...heldJob('running-video','running'),intent,preview:'data:image/png;base64,ordinary',temporalPreview:{revision:1,nodeId:'preview',width:320,height:192,fps:8,frames:[{src:'data:image/png;base64,frame-a',durationMs:125},{src:'data:image/png;base64,frame-b',durationMs:125}]}};
 const other:JobStatus={...running,id:'other-video',temporalPreview:{...running.temporalPreview!,width:160}};
 const inactive=(['queued','succeeded','failed','cancelled','unknown'] as const).map(state=>({...running,id:state,state}));
 const image:JobStatus={...running,id:'image-job',intent:undefined};
 const jobs=[running,other,...inactive,image];
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection(jobs));vi.mocked(pollWorkbench).mockResolvedValue(collection(jobs));
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,videoPreviews:{'ltx-2':config}}});
 vi.mocked(invoke).mockImplementation(async command=>command==='native_media_star_status'?{starred:false,unavailable:null}:{path:'/input',parent:'/',entries:[mediaEntries[0]],truncated:false});
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 const props={endpoint:evidence.baseUrl,roots:{models:'/m',input:'/input'}};
 const content=(hidden=false,planVisible=false)=><div hidden={hidden}><I18nProvider><Workbench {...props} planVisible={planVisible}/></I18nProvider></div>;
 const view=render(content());
 const canvas=()=>document.querySelector<HTMLCanvasElement>('.wb-shown canvas');
 const select=(id:string)=>fireEvent.click(document.querySelector(`[data-job-id="${id}"] .wb-queue-select`)!);
 await waitFor(()=>expect(document.querySelector('[data-job-id="running-video"]')).not.toBeNull());
 expect(canvas()).toBeNull();
 select(running.id);await waitFor(()=>expect(canvas()?.width).toBe(320));
 select(other.id);expect(canvas()?.width).toBe(160);
 for(const job of [...inactive,image]){select(job.id);expect(canvas()).toBeNull();}
 select(running.id);expect(canvas()?.width).toBe(320);
 fireEvent.click(screen.getByRole('tab',{name:'Options'}));
 fireEvent.click(screen.getByRole('switch',{name:'Enhanced preview'}));
 expect(canvas()).toBeNull();
 expect(previewImages.length).toBeGreaterThan(0);
 expect(previewImages.every(image=>image.src===''&&image.onload===null&&image.onerror===null)).toBe(true);
 expect(document.querySelector('.wb-shown img')?.getAttribute('src')).toBe(running.preview);
 expect(running.intent).toHaveProperty('enhancedPreview',config);
 fireEvent.click(screen.getByRole('switch',{name:'Enhanced preview'}));expect(canvas()?.width).toBe(320);
 view.rerender(content(true));await waitFor(()=>expect(canvas()).toBeNull());
 view.rerender(content());await waitFor(()=>expect(canvas()?.width).toBe(320));
 view.rerender(content(false,true));await waitFor(()=>expect(canvas()).toBeNull());
 view.rerender(content());await waitFor(()=>expect(canvas()?.width).toBe(320));
 fireEvent.click(screen.getByRole('button',{name:'Stop'}));expect(canvas()).toBeNull();
 fireEvent.click(document.querySelector('[data-run-stop-cancel]')!);
 await waitFor(()=>expect(canvas()?.width).toBe(320));
 fireEvent.click(screen.getByRole('tab',{name:'Media'}));fireEvent.click(await screen.findByRole('button',{name:'ref.png'}));
 expect(canvas()).toBeNull();expect(document.querySelector('[data-browsed-preview]')).not.toBeNull();
 fireEvent.click(document.querySelector('[data-preview-close]')!);expect(canvas()?.width).toBe(320);
 vi.mocked(pollWorkbench).mockResolvedValue(collection(jobs.map(job=>job.id===running.id?{...job,state:'cancelled'}:job)));
 await waitFor(()=>expect(document.querySelector('.wb-shown [data-state]')?.getAttribute('data-state')).toBe('cancelled'),{timeout:2000});
 expect(canvas()).toBeNull();
 fireEvent.click(document.querySelector('[data-preview-close]')!);expect(canvas()).toBeNull();
 view.unmount();expect(canvas()).toBeNull();
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
});

test('enhanced preview capability refresh preserves saved OFF without computation',async()=>{
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 localStorage.setItem('deas.pref.studio.run.options',JSON.stringify({...defaultOptions,enhancedPreview:false}));
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',false));
 fireEvent.click(screen.getByRole('tab',{name:'Options'}));
 expect(screen.getByRole('switch',{name:'Enhanced preview'})).toHaveProperty('disabled',true);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,videoPreviews:{'ltx-2':{decoder:'tiny',model:'taeltx.safetensors'}}}});
 fireEvent.click(screen.getByRole('tab',{name:'Media'}));fireEvent.click(screen.getByRole('tab',{name:'Options'}));
 await waitFor(()=>expect(screen.getByRole('switch',{name:'Enhanced preview'})).toHaveProperty('disabled',false));
 expect(screen.getByRole('switch',{name:'Enhanced preview'})).toHaveProperty('checked',false);
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.options')!).enhancedPreview).toBe(false);
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
});

test.each(['ltx-2','minimax-h3'] as const)('latent post-processing is opt-in, family scoped, and held in Run Plan and Run (%s)',async family=>{
 const h3=family==='minimax-h3';
 const files=h3?[
  {category:'diffusion_models',name:'h3.safetensors',family},
  {category:'text_encoders',name:'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors'},
  {category:'vae',name:'minimax_h3_video_vae_fp16.safetensors'},
  {category:'vae',name:'minimax_h3_audio_vae_fp32.safetensors'}
 ].map(m=>({...m,path:'/m/'+m.name,size:1,info:null})):[...ltxModels];
 vi.mocked(scanModels).mockResolvedValue(files);
 const ltx='ltx-2.3-spatial-upscaler-x2-1.1.safetensors',h3Model='minimax_h3_latent_upscaler_3d_conv_v1_bf16.safetensors';
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'DaSiWaH3TiledUpscale','LatentUpscaleModelLoader','LTXVLatentUpsampler'],latentUpscaleModels:[ltx,h3Model,'ltx-2.3-temporal-upscaler-x2-1.0.safetensors'],h3LatentUpscaleModels:[h3Model,ltx,'interpolation']}});
 vi.mocked(runWorkbenchJob).mockResolvedValue({...heldJob('latent','failed'),error:'fixture'});
 const props={endpoint:evidence.baseUrl,roots:{models:'/m'}};
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 const view=render(<I18nProvider><Workbench {...props} planVisible/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-plan-intent]')?.textContent).toContain('video-generation'));
 expect(JSON.parse(document.querySelector('[data-plan-intent]')!.textContent!)).not.toHaveProperty('latentUpscale');
 view.rerender(<I18nProvider><Workbench {...props}/></I18nProvider>);
 fireEvent.click(screen.getByRole('tab',{name:'Post-Processing'}));
 fireEvent.click(screen.getByLabelText('Latent upscale after sampling'));
 const picker=screen.getByLabelText('Latent upscale model') as HTMLSelectElement;
 expect([...picker.options].map(o=>o.value)).toContain(h3?h3Model:ltx);
 expect([...picker.options].map(o=>o.value)).not.toContain(h3?ltx:h3Model);
 expect([...picker.options].map(o=>o.value)).not.toContain('ltx-2.3-temporal-upscaler-x2-1.0.safetensors');
 fireEvent.change(picker,{target:{value:h3?h3Model:ltx}});
 view.rerender(<I18nProvider><Workbench {...props} planVisible/></I18nProvider>);
 const plan=JSON.parse(document.querySelector('[data-plan-intent]')!.textContent!);
 expect(plan.latentUpscale).toEqual(h3?{model:h3Model,scale:2,steps:1,denoise:0.2}:{model:ltx,scale:2});
 expect(plan.output).toMatchObject({audioCodec:'Auto',audioBitrate:'192k'});
 view.rerender(<I18nProvider><Workbench {...props}/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 fireEvent.click(document.querySelector('[data-latent-upscale-enabled]')!);
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(vi.mocked(runWorkbenchJob).mock.lastCall![1]).toMatchObject({latentUpscale:plan.latentUpscale,width:plan.width,height:plan.height,frames:plan.frames,output:plan.output});
});

test('TVE post stages are opt-in on video pipelines, badge-marked, and travel as videoPost into Run Plan and Run',async()=>{
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'DaSiWa_RTX_UpscalerRefiner','DaSiWa_RTX_FrameGeneration','FrameInterpolate'],upscaleModels:['2x-AnimeSharpV4_RCAN.safetensors'],frameInterpolationModels:['rife_v4.26.safetensors'],rtxUpscaleModes:['Off','VSR','High Bitrate']}});
 vi.mocked(runWorkbenchJob).mockResolvedValue({...heldJob('tve','failed'),error:'fixture'});
 const props={endpoint:evidence.baseUrl,roots:{models:'/m'}};
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 const view=render(<I18nProvider><Workbench {...props} planVisible/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-plan-intent]')?.textContent).toContain('video-generation'));
 expect(JSON.parse(document.querySelector('[data-plan-intent]')!.textContent!)).not.toHaveProperty('videoPost');
 view.rerender(<I18nProvider><Workbench {...props}/></I18nProvider>);
 fireEvent.click(screen.getByRole('tab',{name:'Post-Processing'}));
 expect(document.querySelectorAll('[data-tve-post] .tve-badge').length).toBeGreaterThan(0);
 fireEvent.change(document.querySelector('[data-tve-post="upscaleModel"]')!,{target:{value:'2x-AnimeSharpV4_RCAN.safetensors'}});
 fireEvent.click(document.querySelector('[data-tve-post="rtxUpscale"]')!);
 const modes=[...(document.querySelector('[data-tve-post="rtxMode"]') as HTMLSelectElement).options].map(o=>o.value);
 expect(modes).toEqual(['VSR','High Bitrate']);
 fireEvent.click(document.querySelector('[data-tve-post="frameGenEnabled"]')!);
 fireEvent.change(document.querySelector('[data-tve-post="engine"]')!,{target:{value:'rife'}});
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);
 fireEvent.change(document.querySelector('[data-tve-post="rifeModel"]')!,{target:{value:'rife_v4.26.safetensors'}});
 view.rerender(<I18nProvider><Workbench {...props} planVisible/></I18nProvider>);
 const plan=JSON.parse(document.querySelector('[data-plan-intent]')!.textContent!);
 expect(plan.videoPost).toEqual({upscaleModel:'2x-AnimeSharpV4_RCAN.safetensors',rtx:{upscale:{mode:'VSR',quality:'Ultra',scale:2}},frameGeneration:{engine:'rife',multiplier:2,model:'rife_v4.26.safetensors'}});
 expect(document.querySelector('[data-plan-stage="tve"]')?.textContent).toContain('48');
 view.rerender(<I18nProvider><Workbench {...props}/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(vi.mocked(runWorkbenchJob).mock.lastCall![1]).toMatchObject({videoPost:plan.videoPost});
});

test('H3 latent upscaler options sit in their own menu, keep separate refine steps and reach Run Plan only when changed',async()=>{
 const files=[
  {category:'diffusion_models',name:'h3.safetensors',family:'minimax-h3'},
  {category:'text_encoders',name:'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors'},
  {category:'vae',name:'minimax_h3_video_vae_fp16.safetensors'},
  {category:'vae',name:'minimax_h3_audio_vae_fp32.safetensors'}
 ].map(m=>({...m,path:'/m/'+m.name,size:1,info:null}));
 vi.mocked(scanModels).mockResolvedValue(files);
 const h3Model='minimax_h3_latent_upscaler_3d_conv_v1_bf16.safetensors';
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,schedulers:['simple','beta'],nodes:[...evidence.allowlist.nodes,'DaSiWaH3TiledUpscale'],h3LatentUpscaleModels:[h3Model]}});
 const props={endpoint:evidence.baseUrl,roots:{models:'/m'}};
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 localStorage.setItem('deas.pref.studio.run.latent.h3',JSON.stringify({enabled:true,model:h3Model,scale:2,steps:1,denoise:0.2}));
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 fireEvent.click(await screen.findByRole('tab',{name:'Post-Processing'}));
 await waitFor(()=>expect(document.querySelector('[data-latent-upscale-more]')).not.toBeNull());
 const more=document.querySelector('[data-latent-upscale-more]') as HTMLDetailsElement;
 expect(more.open).toBe(false);
 fireEvent.change(document.querySelector('[data-latent-upscale-steps]')!,{target:{value:'4'}});
 expect(screen.getByText('Upscale refine steps')).toBeTruthy();
 more.open=true;
 fireEvent.change(more.querySelector('[data-latent-advanced="cfg"]')!,{target:{value:'1.5'}});
 fireEvent.change(more.querySelector('[data-latent-advanced="scheduler"]')!,{target:{value:'beta'}});
 expect(more.querySelector('[data-latent-advanced="guardSensitivity"]')).toBeNull();
 fireEvent.click(more.querySelector('[data-latent-advanced="spikeGuard"]')!);
 expect(more.querySelector('[data-latent-advanced="guardSensitivity"]')).not.toBeNull();
 view.rerender(<I18nProvider><Workbench {...props} planVisible/></I18nProvider>);
 await waitFor(()=>expect(JSON.parse(document.querySelector('[data-plan-intent]')!.textContent!).latentUpscale).toEqual({model:h3Model,scale:2,steps:4,denoise:0.2,advanced:{cfg:1.5,scheduler:'beta',spikeGuard:true}}));
 view.rerender(<I18nProvider><Workbench {...props}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-latent-advanced-reset]')).not.toBeNull());
 fireEvent.click(document.querySelector('[data-latent-advanced-reset]')!);
 view.rerender(<I18nProvider><Workbench {...props} planVisible/></I18nProvider>);
 await waitFor(()=>expect(JSON.parse(document.querySelector('[data-plan-intent]')!.textContent!).latentUpscale).toEqual({model:h3Model,scale:2,steps:4,denoise:0.2}));
});
test('latent models added after the first capability read are found when Post-Processing opens',async()=>{
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);
 const ltx='ltx-2.3-spatial-upscaler-x2-1.1.safetensors';
 const allow=(latentUpscaleModels:string[])=>({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'LatentUpscaleModelLoader','LTXVLatentUpsampler'],latentUpscaleModels}});
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(allow([]));
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(checkRecipePrerequisites).toHaveBeenCalled());
 await act(async()=>{await new Promise(resolve=>setTimeout(resolve,50));});
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(allow([ltx]));
 fireEvent.click(screen.getByRole('tab',{name:'Post-Processing'}));
 await waitFor(()=>expect((screen.getByLabelText('Latent upscale model') as HTMLSelectElement).value).toBe(ltx));
 fireEvent.click(screen.getByLabelText('Latent upscale after sampling'));
 expect(document.querySelector('[data-latent-upscale-download]')).toBeNull();
});

test('latent family preferences persist independently and never replace an explicit missing model',async()=>{
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 localStorage.setItem('deas.pref.studio.run.checkpoint',JSON.stringify('LTX2/ltx.safetensors'));
 localStorage.setItem('deas.pref.studio.run.latent.ltx',JSON.stringify({enabled:false,model:'ltx-missing.safetensors',scale:2,steps:1,denoise:0.2}));
 localStorage.setItem('deas.pref.studio.run.latent.h3',JSON.stringify({enabled:true,model:'h3-missing.safetensors',scale:3,steps:2,denoise:0.3}));
 vi.mocked(scanModels).mockResolvedValue([...ltxModels,{category:'diffusion_models',name:'h3.safetensors',family:'minimax-h3',path:'/m/h3',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,latentUpscaleModels:['ltx-present.safetensors'],h3LatentUpscaleModels:['h3-present.safetensors']}});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-model-picker]')?.textContent).toContain('LTX2/ltx.safetensors'));
 fireEvent.click(document.querySelector('[data-dock-tab="post"]')!);
 expect(document.querySelector('[data-latent-upscale-model]')).toHaveProperty('value','ltx-missing.safetensors');
 expect(document.querySelector('[data-latent-upscale-enabled]')).toHaveProperty('checked',false);
 fireEvent.change(document.querySelector('[data-model-picker]')!,{target:{value:'h3.safetensors'}});
 expect(document.querySelector('[data-latent-upscale-model]')).toHaveProperty('value','h3-missing.safetensors');
 expect(document.querySelector('[data-latent-upscale-scale]')).toHaveProperty('value','3');
 expect(document.querySelector('[data-latent-upscale-enabled]')).toHaveProperty('checked',true);
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);
 fireEvent.change(document.querySelector('[data-model-picker]')!,{target:{value:'LTX2/ltx.safetensors'}});
 expect(document.querySelector('[data-latent-upscale-model]')).toHaveProperty('value','ltx-missing.safetensors');
 expect(document.querySelector('[data-latent-upscale-enabled]')).toHaveProperty('checked',false);
 expect(document.querySelector('[data-latent-upscale-scale]')).toHaveProperty('value','2');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('latent H3 segment capture is allowed in Continuity with the core baseline decode',async()=>{
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 localStorage.setItem('deas.pref.studio.run.latent.h3',JSON.stringify({enabled:true,model:'h3-post.safetensors',scale:2,steps:1,denoise:0.2}));
 localStorage.setItem('deas.pref.studio.run.continuity',JSON.stringify({enabled:true,session:'test',outputMode:'segment',steps:[{id:'one',seconds:5,action:''}]}));
 vi.mocked(scanModels).mockResolvedValue([{category:'diffusion_models',name:'h3.safetensors',family:'minimax-h3'},{category:'text_encoders',name:'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors'},{category:'vae',name:'minimax_h3_video_vae_fp16.safetensors'},{category:'vae',name:'minimax_h3_audio_vae_fp32.safetensors'}].map(m=>({...m,path:'/m/'+m.name,size:1,info:null})));
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'DaSiWaH3TiledUpscale'],h3LatentUpscaleModels:['h3-post.safetensors']}});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',false));
 fireEvent.click(screen.getByRole('button',{name:'Continuity'}));
 const dialog=screen.getByRole('dialog',{name:'Continuity'});
 expect(dialog.textContent).not.toContain('turn off latent upscale');
 expect(dialog.querySelector('[data-cont-run]')).toHaveProperty('disabled',false);
 expect(dialog.querySelector('[data-cont-run-all]')).toHaveProperty('disabled',false);
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('Continuity is off by default: planner runs are blocked, ordinary Run stays unconditioned and enabling submits nothing',async()=>{
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 const stored={session:'test',outputMode:'segment',steps:[{id:'one',seconds:5,action:''}]};
 localStorage.setItem('deas.pref.studio.run.continuity',JSON.stringify(stored));
 vi.mocked(scanModels).mockResolvedValue([{category:'diffusion_models',name:'h3.safetensors',family:'minimax-h3'},{category:'text_encoders',name:'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors'},{category:'vae',name:'minimax_h3_video_vae_fp16.safetensors'},{category:'vae',name:'minimax_h3_audio_vae_fp32.safetensors'}].map(m=>({...m,path:'/m/'+m.name,size:1,info:null})));
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',false));
 fireEvent.click(screen.getByRole('button',{name:'Continuity · off'}));
 const dialog=screen.getByRole('dialog',{name:'Continuity'});
 expect(dialog.querySelector('[data-cont-run]')).toHaveProperty('disabled',true);
 expect(dialog.querySelector('[data-cont-run-all]')).toHaveProperty('disabled',true);
 fireEvent.click(within(dialog).getByRole('checkbox',{name:'Continuity active'}));
 expect(dialog.querySelector('[data-cont-run]')).toHaveProperty('disabled',false);
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.continuity')!)).toEqual({...stored,enabled:true});
 expect(runWorkbenchJob).not.toHaveBeenCalled();
 fireEvent.click(within(dialog).getByRole('checkbox',{name:'Continuity active'}));
 fireEvent.click(within(dialog).getByRole('button',{name:'Close'}));
 vi.mocked(runWorkbenchJob).mockResolvedValue({...heldJob('plain','failed'),error:'fixture'});
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.lastCall![1]).toMatchObject({family:'minimax-h3'});
 expect((vi.mocked(runWorkbenchJob).mock.lastCall![1] as unknown as {h3:object}).h3).not.toHaveProperty('continuity');
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.continuity')!)).toEqual({...stored,enabled:false});
});

test('late latent capability replies from an old endpoint cannot enable the new backend',async()=>{
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 localStorage.setItem('deas.pref.studio.run.latent.ltx',JSON.stringify({enabled:true,model:'ltx-post.safetensors',scale:2,steps:1,denoise:0.2}));
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);
 let finish!:(value:Awaited<ReturnType<typeof checkRecipePrerequisites>>)=>void;
 vi.mocked(checkRecipePrerequisites).mockImplementation(url=>url.includes('old')?new Promise(resolve=>{finish=resolve;}):Promise.resolve(evidence));
 const props={roots:{models:'/m'}};
 const view=render(<I18nProvider><Workbench {...props} endpoint="http://old:7801"/></I18nProvider>);
 await waitFor(()=>expect(finish).toBeTypeOf('function'));
 view.rerender(<I18nProvider><Workbench {...props} endpoint="http://new:7801"/></I18nProvider>);
 await act(async()=>{});
 await act(async()=>finish({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'LatentUpscaleModelLoader','LTXVLatentUpsampler'],latentUpscaleModels:['ltx-post.safetensors']}}));
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);
 expect(document.querySelector('.wb-stage-warning')?.textContent).toContain('LatentUpscaleModelLoader');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('a pending capability probe is not a missing latent upscale model',async()=>{
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 localStorage.setItem('deas.pref.studio.run.latent.ltx',JSON.stringify({enabled:true,model:'ltx-2.3-spatial-upscaler-x2-1.1.safetensors',scale:2,steps:1,denoise:0.2}));
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);
 let finish!:(value:Awaited<ReturnType<typeof checkRecipePrerequisites>>)=>void;
 vi.mocked(checkRecipePrerequisites).mockImplementation(()=>new Promise(resolve=>{finish=resolve;}));
 render(<I18nProvider><Workbench roots={{models:'/m'}} endpoint="http://127.0.0.1:7801"/></I18nProvider>);
 await waitFor(()=>expect(finish).toBeTypeOf('function'));
 // Backend still answering: Run is disabled by the pending notice alone, the selected model is not reported missing.
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);
 expect(document.body.textContent).not.toContain('Missing compatible latent upscale model');
 await act(async()=>finish({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'LatentUpscaleModelLoader','LTXVLatentUpsampler'],latentUpscaleModels:['ltx-2.3-spatial-upscaler-x2-1.1.safetensors']}}));
 expect(document.body.textContent).not.toContain('Missing compatible latent upscale model');
});

test.each(['cancel','download'] as const)('missing latent model opens the actual Library offer without automatic work (%s)',async action=>{
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 localStorage.setItem('deas.pref.studio.run.latent.ltx',JSON.stringify({enabled:false,model:'explicit-missing.safetensors',scale:2,steps:1,denoise:0.2}));
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);
 const filename='ltx-2.3-spatial-upscaler-x2-1.1.safetensors';
 let installed=false,started=false;
 vi.mocked(checkRecipePrerequisites).mockImplementation(async()=>({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'LatentUpscaleModelLoader','LTXVLatentUpsampler'],latentUpscaleModels:installed?[filename]:[]}}));
 const file={id:'file',label:filename,locator:filename,downloadable:true,kind:'file',url:'https://huggingface.co/file',size:1};
 vi.mocked(invoke).mockImplementation(async c=>{
  if(c==='native_download_catalog')return {version:1,providers:[{id:'huggingface',kind:'huggingface',label:'Hugging Face',origin:'https://huggingface.co',scopes:['repo'],anonymousNotice:''}],entries:[{id:'ltx-latent-upscaler',label:'LTX',providerId:'huggingface',scope:'repo',locator:'models:Lightricks/LTX-2.3',kind:'repo',target:'latent_upscale_models'}]};
  if(c==='native_auth_status')return [];
  if(c==='native_download_browse')return {resources:[file],warnings:[],nextCursor:null};
  if(c==='native_files_present')return [false];
  if(c==='native_download_plan')return {id:'consented',target:'/configured/latent_upscale_models',resources:[file],canExecute:true,warnings:[],totalKnownBytes:1,unknownSizeCount:0};
  if(c==='native_download_start'){started=true;return {state:'running',completed:0,total:1,bytesDownloaded:0,error:null};}
  if(c==='native_download_status'){if(!started)return {state:'idle',completed:0,total:0,bytesDownloaded:0,error:null};installed=true;return {state:'succeeded',completed:1,total:1,bytesDownloaded:1,error:null};}
  return {path:'/input',parent:'/',entries:[],truncated:false};
 });
 const view=render(<I18nProvider><Workbench endpoint={evidence.baseUrl} root="/env" roots={{models:'/configured'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-model-picker]')?.textContent).toContain('LTX2/ltx.safetensors'));
 fireEvent.click(screen.getByRole('tab',{name:'Post-Processing'}));
 fireEvent.click(screen.getByLabelText('Latent upscale after sampling'));
 const dialog=screen.getByRole('dialog',{name:'Get latent upscale model…'});
 await within(dialog).findByLabelText(filename);
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);
 expect(vi.mocked(invoke).mock.calls.some(([c])=>c==='native_download_plan'||c==='native_download_start')).toBe(false);
 const before=vi.mocked(checkRecipePrerequisites).mock.calls.length;
 if(action==='download'){
  fireEvent.click(within(dialog).getByLabelText(filename));
  fireEvent.click(within(dialog).getByRole('button',{name:'Download all (1)'}));
  await within(dialog).findByText('Running');
  for(let i=0;i<4;i++){
   await act(async()=>{await new Promise(resolve=>setTimeout(resolve,300));});
   view.rerender(<I18nProvider><Workbench endpoint={evidence.baseUrl} root="/env" roots={{models:'/configured'}}/></I18nProvider>);
  }
  expect(vi.mocked(invoke).mock.calls.filter(([c])=>c==='native_download_status')).toHaveLength(2);
  await within(dialog).findByText('Succeeded');
  expect(invoke).toHaveBeenCalledWith('native_download_plan',{request:{providerId:'huggingface',resources:[file],parentPath:'/configured/latent_upscale_models',folderName:'',authenticated:false}});
  await waitFor(()=>expect(checkRecipePrerequisites).toHaveBeenCalledTimes(before+1));
 }
 fireEvent.click(within(dialog).getByRole('button',{name:'Close'}));
 await waitFor(()=>expect(screen.queryByRole('dialog')).toBeNull());
 expect(screen.getByLabelText('Latent upscale model')).toHaveProperty('value','explicit-missing.safetensors');
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test.each([true,false])('paired video role selection renders one player even without output root (%s)',async owned=>{
 const job:JobStatus={...heldJob('video-pair','succeeded'),...(owned?{outputRoot:'/old/output'}:{}),outputs:[{filename:'processed.mp4',subfolder:'',type:'output',format:'video/mp4',role:'processed'},{filename:'original.mp4',subfolder:'',type:'output',format:'video/mp4',role:'original'},{filename:'processed-first.png',subfolder:'frames',type:'output',role:'processed'},{filename:'original-last.png',subfolder:'frames',type:'output',role:'original'}]};
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
 vi.mocked(loadDiagnosticJobOutput).mockImplementation(async(jobId,index)=>({jobId,index,mimeType:'image/png',width:1,height:1,sha256:'a'.repeat(64),bytes:[137,80,78,71]}));
 vi.stubGlobal('URL',Object.assign(class extends URL {},{createObjectURL:vi.fn(()=>'blob:video-side-frame'),revokeObjectURL:vi.fn()}));
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-output-role="original"]')).toBeTruthy());
 expect(document.querySelectorAll('.wb-shown video')).toHaveLength(1);
 fireEvent.click(document.querySelector('[data-output-role="original"]')!);
 expect(document.querySelectorAll('.wb-shown video')).toHaveLength(1);
 expect(document.querySelector('.wb-shown video')?.getAttribute('src')).toContain('original.mp4');
 for(const index of [2,3]){
  fireEvent.click(document.querySelectorAll('.wb-output-choices button')[index]);
  if(owned){
   expect(document.querySelector('.wb-shown img')?.getAttribute('src')).toContain(`/old/output/frames/${job.outputs[index].filename}`);
   expect(loadDiagnosticJobOutput).not.toHaveBeenCalled();
  }else{
   await waitFor(()=>expect(loadDiagnosticJobOutput).toHaveBeenLastCalledWith(job.id,index));
   await waitFor(()=>expect(document.querySelector('[data-job-output-preview]')?.getAttribute('src')).toBe('blob:video-side-frame'));
   expect(loadDiagnosticJobOutput).toHaveBeenCalledTimes(index-1);
  }
  expect(document.querySelectorAll('.wb-shown video')).toHaveLength(0);
 }
 fireEvent.click(document.querySelector('[data-output-role="processed"]')!);
 expect(document.querySelectorAll('.wb-shown video')).toHaveLength(1);
 expect(document.querySelector('.wb-shown video')?.getAttribute('src')).toContain('processed.mp4');
 expect(document.querySelector('.wb-shown img')).toBeNull();
 expect(document.querySelector('[data-job-output-preview]')).toBeNull();
 if(!owned)expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:video-side-frame');
});

test.each([false,true])('video workspace navigation seam is optional (%s) and leaves image Post settings intact',async connected=>{
 const onOpen=vi.fn();
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" onOpenVideoPost={connected?onOpen:undefined}/></I18nProvider>);
 await act(async()=>{});
 fireEvent.click(screen.getByRole('tab',{name:'Post-Processing'}));
 expect(document.querySelector('[data-video-post]')).toBeNull();
 const upscale=screen.getByLabelText('Upscale factor');
 fireEvent.change(upscale,{target:{value:'3'}});
 const navigate=document.querySelector('[data-open-video-post]');
 expect(!!navigate).toBe(connected);
 // The TVE link lives inside its own category, never loose above the groups.
 if(navigate)expect(navigate.closest('[data-post-group="tve"]')).not.toBeNull();
 const calls=vi.mocked(invoke).mock.calls.length;
 const probes=vi.mocked(checkRecipePrerequisites).mock.calls.length;
 if(navigate)fireEvent.click(navigate);
 expect(onOpen).toHaveBeenCalledTimes(connected?1:0);
 expect(upscale).toHaveProperty('value','3');
 expect(invoke).toHaveBeenCalledTimes(calls);
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(checkRecipePrerequisites).toHaveBeenCalledTimes(probes);
});

test('paired products select their own actions and compare the encoded baseline, not the editor input',async()=>{
 const intent={recipeId:'checkpoint-text-to-image-diagnostic',version:1,positivePrompt:'held',negativePrompt:'',checkpoint:'a.safetensors',seed:42,cfg:6,sampler:'euler',scheduler:'normal',outputPrefix:'raw/old',width:1024,height:1024,steps:20,inputImage:'unrelated.png',refine:{width:2048,height:2048,denoise:0.25}} as const;
 const job:JobStatus={...heldJob('pair','succeeded'),outputRoot:'/old/output',intent,outputs:[{filename:'one.png',subfolder:'raw',type:'output',role:'processed'},{filename:'two.png',subfolder:'raw',type:'output',role:'original'}]};
 const captured=JSON.stringify(intent);
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
 render(<I18nProvider><Workbench endpoint="http://new:7802/"/></I18nProvider>);
 await waitFor(()=>expect(document.querySelectorAll('[data-job-id="pair"] .wb-queue-select')).toHaveLength(2));
 const queue=document.querySelector('[data-job-id="pair"]')!;
 expect(queue.textContent).toContain('Original (before processing)');expect(queue.textContent).toContain('Processed');
 expect(document.querySelector('.wb-shown img')?.getAttribute('src')).toContain('/old/output/raw/one.png');
 fireEvent.click(document.querySelector('[data-output-role="original"]')!);
 expect(document.querySelector('.wb-shown img')?.getAttribute('src')).toContain('/old/output/raw/two.png');
 clickMedia(document.querySelector<HTMLImageElement>('.wb-shown img')!);
 expect(document.querySelector('.wb-viewer img')?.getAttribute('src')).toContain('/old/output/raw/two.png');
 fireEvent.keyDown(window,{key:'Escape'});
 fireEvent.click(document.querySelector('[data-edit-compare]')!);
 expect(document.querySelector('.wb-compare > img')?.getAttribute('src')).toContain('/old/output/raw/two.png');
 expect(document.querySelector('.wb-compare-top img')?.getAttribute('src')).toContain('/old/output/raw/one.png');
 expect(document.querySelector('.wb-compare')?.textContent).not.toContain('unrelated.png');
 fireEvent.click(queue.querySelectorAll('.wb-queue-select')[0]);
 expect(document.querySelector('.wb-compare')).toBeNull();
 expect(document.querySelector('.wb-shown img')?.getAttribute('src')).toContain('/old/output/raw/one.png');
 expect(document.querySelector('.wb-stage > [data-media-tools] [data-use-input]')).toBeTruthy();
 expect(JSON.stringify(intent)).toBe(captured);expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test.each(['failed','cancelled'] as const)('%s keeps a retained baseline accessible without inventing a processed result',async state=>{
 const job:JobStatus={...heldJob('partial',state),outputRoot:'/old/output',outputs:[{filename:'baseline.png',subfolder:'raw',type:'output',role:'original'}]};
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
 render(<I18nProvider><Workbench endpoint="http://new:7802/"/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('.wb-shown img')?.getAttribute('src')).toContain('/old/output/raw/baseline.png'));
 expect(document.querySelector('[data-job-id="partial"]')?.textContent).toContain('Original (before processing)');
 expect(document.querySelector('[data-output-role="processed"]')).toBeNull();
 expect(document.querySelector('[data-edit-compare]')).toBeNull();
 expect(document.querySelector('.wb-stage > [data-media-tools] [data-use-input]')).toBeTruthy();
 expect(document.querySelector('.wb-shown .media-star')).toBeTruthy();
});
test.each(['succeeded','failed','cancelled'] as const)('rootless %s baseline uses its native index for preview and export',async state=>{
 const job:JobStatus={...heldJob('bound-pair',state),outputs:[...(state==='succeeded'?[{filename:'final.png',subfolder:'',type:'output' as const,role:'processed' as const}]:[]),{filename:'baseline.png',subfolder:'',type:'output',role:'original'}]};
 const index=job.outputs.length-1;
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
 vi.mocked(loadDiagnosticJobOutput).mockImplementation(async(jobId,index)=>({jobId,index,mimeType:'image/png',width:1,height:1,sha256:'a'.repeat(64),bytes:[137,80,78,71]}));
 vi.mocked(exportDiagnosticJobOutput).mockResolvedValue(null);
 vi.stubGlobal('URL',Object.assign(class extends URL {},{createObjectURL:vi.fn(()=>'blob:paired'),revokeObjectURL:vi.fn()}));
 render(<I18nProvider><Workbench endpoint="http://new:7802/"/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-output-role="original"]')).toBeTruthy());
 fireEvent.click(document.querySelectorAll('[data-job-id="bound-pair"] .wb-queue-select')[index]);
 await waitFor(()=>expect(loadDiagnosticJobOutput).toHaveBeenLastCalledWith(job.id,index));
 await waitFor(()=>expect(document.querySelector('[data-job-output-preview]')).toBeTruthy());
 fireEvent.click(document.querySelector('[data-job-output-export]')!);
 await waitFor(()=>expect(exportDiagnosticJobOutput).toHaveBeenCalledWith(job.id,index,expect.any(String)));
 expect(document.querySelector('[data-edit-compare]')).toBeNull();expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('ordinary results keep their existing multi-output presentation without paired labels',async()=>{
 const job:JobStatus={...heldJob('ordinary','succeeded'),outputRoot:'/old/output',outputs:[{filename:'a.png',subfolder:'',type:'output'},{filename:'b.png',subfolder:'',type:'output'}]};
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
 render(<I18nProvider><Workbench endpoint="http://new:7802/"/></I18nProvider>);
 await waitFor(()=>expect(document.querySelectorAll('.wb-shown img')).toHaveLength(2));
 expect(document.querySelector('[data-output-role]')).toBeNull();expect(document.querySelector('[data-edit-compare]')).toBeNull();
});

test.each(['http://old:7801/','http://new:7802/'])('recovered output uses native root, not endpoint or current environment (%s)',async baseUrl=>{
 const job={...heldJob('old-environment','succeeded',baseUrl),outputRoot:'/old-env/output',outputs:[{filename:'result.png',subfolder:'raw',type:'output' as const}]};
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
 vi.mocked(invoke).mockImplementation(async command=>command==='native_media_star_status'?{starred:false,unavailable:null}:{entries:[],truncated:false});
 render(<I18nProvider><Workbench endpoint="http://new:7802/" roots={{output:'/new-env/output'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-job-id="old-environment"] img')?.getAttribute('src')).toContain('/old-env/output/raw/result.png'));
 expect(document.querySelector('.wb-shown img')?.getAttribute('src')).toContain('/old-env/output/raw/result.png');
 expect(document.querySelectorAll('[src*="/new-env/output/"]')).toHaveLength(0);
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_media_star_status',{path:'/old-env/output/raw/result.png'}));
 clickMedia(document.querySelector<HTMLImageElement>('.wb-shown img')!);
 expect(document.querySelector('.wb-viewer img')?.getAttribute('src')).toContain('/old-env/output/raw/result.png');
 fireEvent.keyDown(window,{key:'Escape'});
 fireEvent(window,new CustomEvent('deas-media-relocated',{detail:{oldPath:'/old-env/output/raw/result.png',path:'/old-env/output/starred/result.png'}}));
 await waitFor(()=>expect(document.querySelector('[data-job-id="old-environment"] img')?.getAttribute('src')).toContain('/old-env/output/starred/result.png'));
});
test('unknown output root uses bound preview/export with no local-path actions',async()=>{
 const job={...heldJob('remote-result','succeeded'),outputs:[{filename:'result.png',subfolder:'raw',type:'output' as const}]};
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
 vi.mocked(loadDiagnosticJobOutput).mockResolvedValue({jobId:job.id,index:0,mimeType:'image/png',width:1,height:1,sha256:'a'.repeat(64),bytes:[137,80,78,71]});
 vi.mocked(exportDiagnosticJobOutput).mockResolvedValue({path:'/chosen/result.png',sha256:'a'.repeat(64),size:4});
 vi.stubGlobal('URL',Object.assign(class extends URL {},{createObjectURL:vi.fn(()=>'blob:bound-output'),revokeObjectURL:vi.fn()}));
 render(<I18nProvider><Workbench endpoint="http://new:7802/" roots={{output:'/new-env/output'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-job-output-preview]')?.getAttribute('src')).toBe('blob:bound-output'));
 expect(loadDiagnosticJobOutput).toHaveBeenCalledWith(job.id,0);
 expect(document.querySelector('[data-job-id="remote-result"] img')).toBeNull();
 expect(document.querySelector('[data-run-post]')).toBeNull();
 expect(document.querySelector('[data-use-input]')).toBeNull();
 expect(invoke).not.toHaveBeenCalledWith('native_media_star_status',expect.anything());
 expect((screen.getByRole('button',{name:'Star result.png'}) as HTMLButtonElement).disabled).toBe(true);
 fireEvent.click(document.querySelector('[data-job-output-export]')!);
 await waitFor(()=>expect(exportDiagnosticJobOutput).toHaveBeenCalledWith(job.id,0,expect.any(String)));
});
test.each(['/new-env/output','/old-env/output'])('recovered image Post-process transfers its native-held file even when UI roots match (%s)',async output=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 localStorage.setItem('deas.pref.studio.run.pipeline','"i2i"');
 const intent={recipeId:'checkpoint-text-to-image-diagnostic',version:1,positivePrompt:'old',negativePrompt:'',checkpoint:'a.safetensors',seed:42,cfg:6,sampler:'euler',scheduler:'normal',outputPrefix:'raw/old',width:1024,height:1024,steps:20} as const;
 const job={...heldJob('old-image','succeeded'),outputRoot:'/old-env/output',intent,outputs:[{filename:'result.png',subfolder:'raw',type:'output' as const}]};
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
 vi.mocked(uploadInput).mockResolvedValue('deas-inputs/original.png');
 vi.mocked(runWorkbenchJob).mockResolvedValue({...heldJob('post','failed'),baseUrl:evidence.baseUrl});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output}}/></I18nProvider>);
 const post=await waitFor(()=>{const post=document.querySelector<HTMLButtonElement>('[data-run-post]');expect(post).not.toBeNull();expect(post!.disabled).toBe(false);return post!;});
 fireEvent.click(post);
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(uploadInput).toHaveBeenCalledWith(evidence.baseUrl,'/old-env/output/raw/result.png');
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({inputImage:'deas-inputs/original.png'});
});
test('More stays on a selected image in a video pipeline, between Inpaint and the star, with only its passes disabled',async()=>{
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 vi.mocked(invoke).mockResolvedValue({path:'/out',parent:'/',entries:[{name:'still.png',path:'/out/still.png',kind:'file',symlink:false,media:'image',size:1,modified:1}],truncated:false});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/out'}}/></I18nProvider>);
 fireEvent.click(await screen.findByRole('button',{name:'still.png'}));
 const bar=await waitFor(()=>{const el=document.querySelector<HTMLElement>('[data-media-tools]');expect(el?.querySelector('summary[data-more]')).not.toBeNull();return el!;});
 const order=[...bar.querySelectorAll('[data-edit="inpaint"], details.wb-more, .media-star')].map(el=>el.matches('.media-star')?'star':el.matches('details')?'more':'inpaint');
 expect(order.indexOf('more')).toBe(order.indexOf('inpaint')+1);
 if(order.includes('star'))expect(order.indexOf('star')).toBeGreaterThan(order.indexOf('more'));
 fireEvent.click(bar.querySelector('summary[data-more]')!);
 expect(bar.querySelector<HTMLButtonElement>('[data-run-post]')).toMatchObject({disabled:true});
 expect(bar.querySelector<HTMLButtonElement>('[data-run-post]')!.title).toContain('image pipeline');
 expect(bar.querySelector<HTMLButtonElement>('[data-more-apply]')!.disabled).toBe(false);
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test('More offers Refine and Upscale; Upscale runs the pass alone (denoise 0, no sigmas)',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 localStorage.setItem('deas.pref.studio.run.pipeline','"i2i"');
 localStorage.setItem('deas.pref.studio.run.upscale','2');
 const intent={recipeId:'checkpoint-text-to-image-diagnostic',version:1,positivePrompt:'old',negativePrompt:'',checkpoint:'a.safetensors',seed:42,cfg:6,sampler:'euler',scheduler:'normal',outputPrefix:'raw/old',width:1024,height:1024,steps:20} as const;
 const job={...heldJob('more-image','succeeded'),outputRoot:'/old-env/output',intent,outputs:[{filename:'result.png',subfolder:'raw',type:'output' as const}]};
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
 vi.mocked(uploadInput).mockResolvedValue('deas-inputs/result.png');
 vi.mocked(runWorkbenchJob).mockResolvedValue({...heldJob('upscaled','failed'),baseUrl:evidence.baseUrl});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/old-env/output'}}/></I18nProvider>);
 expect(await screen.findByText('More')).toBeTruthy();
 const refine=document.querySelector<HTMLButtonElement>('[data-run-post]')!,up=document.querySelector<HTMLButtonElement>('[data-run-upscale]')!;
 expect(refine.disabled).toBe(false);expect(up.disabled).toBe(false);
 expect(refine.title).toContain('Refine AND Upscale');expect(up.title).toContain('Upscale pass');
 fireEvent.click(up);
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 const sent=vi.mocked(runWorkbenchJob).mock.calls[0][1];
 expect(sent).toMatchObject({denoise:0,inputImage:'deas-inputs/result.png',width:2048,height:2048});
 expect(sent.sigmas).toBeUndefined(); // upscale-only: sigmas absent (the backend's valid_sigmas rejects an empty [])
 expect(sent.steps).toBeGreaterThan(0); // no empty-tail steps = -1
});
const settingsDrop=(path:string)=>fireEvent.drop(screen.getByRole('region',{name:'Preview'}),{dataTransfer:{types:['application/x-deas-media'],getData:()=>JSON.stringify({name:path.split('/').at(-1),path,src:'asset://localhost/'+path,media:path.endsWith('.mp4')?'video':'image'})}});
test('preview settings drop reads the dropped file without submitting or replacing the editor source',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(readImageMetadata).mockResolvedValue([['parameters','dropped prompt\nNegative prompt: dropped negative\nSteps: 7, Sampler: Euler, CFG scale: 3, Seed: 11, Size: 512x512, Model: a.safetensors']]);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 settingsDrop('/outside/dropped.png');
 await waitFor(()=>expect(screen.getByLabelText('Prompt')).toHaveProperty('value','dropped prompt'));
 expect(screen.getByLabelText('Steps')).toHaveProperty('value','7');
 expect(document.querySelector('[data-browsed-preview]')).not.toBeNull();
 expect(readImageMetadata).toHaveBeenCalledWith('/outside/dropped.png');
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
});
test.each(['layer','base'] as const)('queued settings ask before discarding editor %s edits and cancellation preserves the draft',async edited=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 const intent={recipeId:'checkpoint-text-to-image-diagnostic',version:1,positivePrompt:'queued prompt',negativePrompt:'',checkpoint:'a.safetensors',seed:42,cfg:6,sampler:'euler',scheduler:'normal',outputPrefix:'raw/old',width:1024,height:1024,steps:20} as const;
 const job={...heldJob('layer-reuse','succeeded'),baseUrl:evidence.baseUrl,outputRoot:'/out',intent,outputs:[{filename:'result.png',subfolder:'',type:'output' as const}]};
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
 vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_media_star_status'?{starred:false,unavailable:null}:{path:(args as {path?:string})?.path??'/out',parent:'/',entries:['source.png','insert.png'].map(name=>({name,path:'/out/'+name,kind:'file',media:'image',symlink:false,size:1,modified:1})),truncated:false});
 vi.stubGlobal('Image',class{naturalWidth=200;naturalHeight=100;onload=()=>{};set src(_:string){queueMicrotask(()=>this.onload());}});
 const confirm=vi.spyOn(window,'confirm').mockReturnValue(false);
 try{
  render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/out'}}/></I18nProvider>);
  fireEvent.click(await screen.findByRole('button',{name:'source.png'}));adopt();
  const img=screen.getByRole('img',{name:'source.png'});Object.defineProperties(img,{naturalWidth:{value:1024},naturalHeight:{value:640}});fireEvent.load(img);
  fireEvent.click(screen.getByRole('button',{name:'Image to Image'}));
  let layer:HTMLElement;
  if(edited==='base'){layer=screen.getByRole('group',{name:'Base image'});fireEvent.keyDown(layer,{key:'ArrowRight'});expect(layer.style.left).toBe('8px');}
  else{dropTile('insert.png');layer=await screen.findByRole('group',{name:'Inserted image insert.png [output]'});}
  const row=document.querySelector('[data-job-id="layer-reuse"] .wb-queue-select')!;
  (row as HTMLElement).focus();fireEvent.keyDown(row,{key:'F10',shiftKey:true});
  const menuItem=screen.getByRole('menuitem',{name:'Use these settings'});expect(document.activeElement).toBe(menuItem);
  fireEvent.keyDown(menuItem,{key:'Escape'});expect(screen.queryByRole('menu')).toBeNull();expect(document.activeElement).toBe(row);
  fireEvent.keyDown(row,{key:'F10',shiftKey:true});fireEvent.click(screen.getByRole('menuitem',{name:'Use these settings'}));
  expect(confirm).toHaveBeenCalledTimes(1);expect(document.querySelector('[data-editor-viewport]')).not.toBeNull();expect(layer.isConnected).toBe(true);
  expect(screen.getByLabelText('Prompt')).not.toHaveProperty('value','queued prompt');
  confirm.mockReturnValue(true);fireEvent.keyDown(row,{key:'ContextMenu'});fireEvent.click(screen.getByRole('menuitem',{name:'Use these settings'}));
  expect(screen.getByLabelText('Prompt')).toHaveProperty('value','queued prompt');expect(layer.isConnected).toBe(false);
  expect(job.intent).toEqual(intent);expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
 }finally{confirm.mockRestore();}
});
test('file named schedules and explicit empty LoRAs replace stale same-family settings',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null},{category:'loras',name:'style.safetensors',family:'sdxl',path:'/m/style',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 localStorage.setItem('deas.pref.studio.run.options',JSON.stringify({...defaultOptions,sigmas:'2, 1, 0'}));
 localStorage.setItem('deas.pref.studio.run.loras',JSON.stringify([{name:'style.safetensors',strength:1}]));
 vi.mocked(readImageMetadata).mockResolvedValue([['prompt',JSON.stringify({'1':{class_type:'CheckpointLoaderSimple',inputs:{ckpt_name:'a.safetensors'}},'5':{class_type:'KSampler',inputs:{model:['1',0],seed:42,steps:20,cfg:4,sampler_name:'euler',scheduler:'normal'}}})]]);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-model-picker]')).toHaveProperty('value','a.safetensors'));
 settingsDrop('/outside/named.png');
 await waitFor(()=>expect(captureStudioSettings()['studio.run.options']).toMatchObject({sigmas:''}));
 expect(captureStudioSettings()['studio.run.loras']).toEqual([]);expect(screen.getByLabelText('Steps')).toHaveProperty('value','20');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test('file settings resolve LoRAs against the restored model family, not the previous draft',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'diffusion_models',name:'qwen.safetensors',family:'qwen-image-2.1',path:'/m/qwen',size:1,info:null},{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null},{category:'loras',name:'style.safetensors',family:'sdxl',path:'/m/style',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 localStorage.setItem('deas.pref.studio.run.checkpoint','"qwen.safetensors"');
 vi.mocked(readImageMetadata).mockResolvedValue([['parameters','restored <lora:style.safetensors:0.75>\nSteps: 7, Sampler: Euler, CFG scale: 3, Seed: 11, Size: 512x512, Model: a.safetensors']]);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-model-picker]')).toHaveProperty('value','qwen.safetensors'));
 settingsDrop('/outside/restored.png');
 await waitFor(()=>expect(document.querySelector('[data-model-picker]')).toHaveProperty('value','a.safetensors'));
 expect(captureStudioSettings()['studio.run.loras']).toEqual([{name:'style.safetensors',strength:0.75}]);
 expect(screen.queryByText(/Not carried over:/)).toBeNull();expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test.each(['missing.safetensors','same.safetensors','exact.safetensors'])('file settings keep a missing or ambiguous model blocked (%s)',async name=>{
 vi.mocked(scanModels).mockResolvedValue(['a.safetensors','one/same.safetensors','two/same.safetensors','exact.safetensors','exact.safetensors'].map((name,index)=>({category:'checkpoints',name,family:'sdxl',path:'/m/'+index+'/'+name,size:1,info:null})));
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(readImageMetadata).mockResolvedValue([['parameters',`restore\nSteps: 7, Sampler: Euler, CFG scale: 3, Seed: 11, Size: 512x512, Model: ${name}`]]);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 settingsDrop('/outside/missing.png');
 await waitFor(()=>expect(captureStudioSettings()['studio.run.checkpoint']).toBe(name));
 expect(document.querySelector('[data-run]')).toHaveProperty('disabled',true);
 expect(screen.getByText(new RegExp('Not carried over:.*'+name.replaceAll('.','\\.')))).toBeTruthy();
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test.each(['sync','async'] as const)('unavailable native drop API does not crash Studio (%s)',async failure=>{
 nativeDropFailure[failure]=true;
 vi.mocked(scanModels).mockResolvedValue([]);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 expect(await screen.findByText('Native file drops are unavailable in this view. Use the media library instead.')).toBeTruthy();
 expect(document.querySelector('[data-workbench]')).not.toBeNull();expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test('native preview drop is bounded to the visible stage and resolves a local video without uploading',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 const previous=vi.mocked(invoke).getMockImplementation()!;
 vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_media_describe'?[{name:'clip.mp4',path:'/outside/clip.mp4',kind:'file',media:'video',size:1,modified:1,symlink:false}]:previous(command,args));
 vi.mocked(readImageMetadata).mockResolvedValue([['parameters','native clip prompt\nSteps: 7, Sampler: Euler, CFG scale: 3, Seed: 11, Size: 512x512']]);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 const stage=screen.getByRole('region',{name:'Preview'});
 vi.spyOn(stage,'getBoundingClientRect').mockReturnValue({left:100,top:100,right:500,bottom:500,width:400,height:400,x:100,y:100,toJSON:()=>({})});
 const drop=(x:number,y:number)=>act(()=>{for(const handler of nativeDrops)handler({payload:{type:'drop',paths:['/outside/clip.mp4'],position:{x,y}}});});
 drop(20,20);expect(readImageMetadata).not.toHaveBeenCalled();
 drop(150*window.devicePixelRatio,150*window.devicePixelRatio);
 await waitFor(()=>expect(screen.getByLabelText('Prompt')).toHaveProperty('value','native clip prompt'));
 expect(readImageMetadata).toHaveBeenCalledWith('/outside/clip.mp4');
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
 vi.mocked(invoke).mockImplementation(previous);
});
test('native drop positions arrive already logical on WebKitGTK despite fractional scaling',async()=>{
 localStorage.setItem('deas.pref.interface.experience',JSON.stringify('advanced'));
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 const previous=vi.mocked(invoke).getMockImplementation()!;
 vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_media_describe'?[{name:'clip.mp4',path:'/outside/clip.mp4',kind:'file',media:'video',size:1,modified:1,symlink:false}]:previous(command,args));
 vi.mocked(readImageMetadata).mockResolvedValue([['parameters','scaled drop prompt\nSteps: 7, Sampler: Euler, CFG scale: 3, Seed: 11, Size: 512x512']]);
 Object.defineProperty(window,'devicePixelRatio',{configurable:true,value:1.25});
 try{
  render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
  await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
  const stage=screen.getByRole('region',{name:'Preview'});
  vi.spyOn(stage,'getBoundingClientRect').mockReturnValue({left:100,top:600,right:500,bottom:800,width:400,height:200,x:100,y:600,toJSON:()=>({})});
  act(()=>{for(const handler of nativeDrops)handler({payload:{type:'drop',paths:['/outside/clip.mp4'],position:{x:300,y:700}}});});
  await waitFor(()=>expect(screen.getByLabelText('Prompt')).toHaveProperty('value','scaled drop prompt'));
 }finally{Object.defineProperty(window,'devicePixelRatio',{configurable:true,value:1});vi.mocked(invoke).mockImplementation(previous);}
});
test('beginner starting image row accepts a native OS drop',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 const previous=vi.mocked(invoke).getMockImplementation()!;
 vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_media_describe'?[{name:'clip.png',path:'/m/clip.png',kind:'file',media:'image',size:1,modified:1,symlink:false}]:previous(command,args));
 vi.mocked(readImageMetadata).mockResolvedValue([]);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}} beginner/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-beginner-aspect]')).not.toBeNull());
 fireEvent.change(document.querySelector('#beginner-mode')!,{target:{value:'i2v'}});
 expect(document.querySelector('.beginner-preview-hint')!.textContent).toContain('Pick an image');
 expect(document.querySelector('.beginner-empty-preview')!.textContent).toContain('watch your imagination take shape here');
 const row=await waitFor(()=>{const el=document.querySelector('.beginner-input');if(!el)throw Error('no input row');return el;});
 vi.spyOn(row as HTMLElement,'getBoundingClientRect').mockReturnValue({left:20,top:20,right:320,bottom:80,width:300,height:60,x:20,y:20,toJSON:()=>({})});
 act(()=>{for(const handler of nativeDrops)handler({payload:{type:'drop',paths:['/m/clip.png'],position:{x:170,y:50}}});});
 await waitFor(()=>expect(row.textContent).toContain('clip.png'));
 vi.mocked(invoke).mockImplementation(previous);
});
test.each(['edit','edit back','selection','base','base revert','new drop','unmount'] as const)('late metadata cannot overwrite a newer %s',async change=>{
 vi.mocked(scanModels).mockResolvedValue(['a','b'].map(n=>({category:'checkpoints',name:n+'.safetensors',family:'sdxl',path:'/m/'+n,size:1,info:null})));
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 let finish!:(entries:[string,string][])=>void;
 vi.mocked(readImageMetadata).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;})).mockResolvedValue([]);
 if(change.startsWith('base'))vi.mocked(invoke).mockImplementation(async command=>command==='native_media_star_status'?{starred:false,unavailable:null}:{path:'/out',parent:'/',entries:[{name:'source.png',path:'/out/source.png',kind:'file',media:'image',symlink:false,size:1,modified:1}],truncated:false});
 const mounted=render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/out'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 if(change.startsWith('base')){fireEvent.click(await screen.findByRole('button',{name:'source.png'}));adopt();const img=screen.getByRole('img',{name:'source.png'});Object.defineProperties(img,{naturalWidth:{value:1024},naturalHeight:{value:640}});fireEvent.load(img);fireEvent.click(screen.getByRole('button',{name:'Image to Image'}));}
 settingsDrop('/outside/old.png');await waitFor(()=>expect(finish).toBeTypeOf('function'));
 if(change==='edit'||change==='edit back'){const prompt=screen.getByLabelText('Prompt');const previous=(prompt as HTMLTextAreaElement).value;fireEvent.change(prompt,{target:{value:'newer edit'}});if(change==='edit back')fireEvent.change(prompt,{target:{value:previous}});}
 else if(change==='base'||change==='base revert'){const base=screen.getByRole('group',{name:'Base image'});fireEvent.keyDown(base,{key:'ArrowRight'});if(change==='base revert')fireEvent.keyDown(base,{key:'ArrowLeft'});}
 else if(change==='selection')fireEvent.change(document.querySelector('[data-model-picker]')!,{target:{value:'b.safetensors'}});
 else if(change==='new drop'){settingsDrop('/outside/new.png');await screen.findByText('The shown file carries no reusable settings or prompt.');}
 else mounted.unmount();
 await act(async()=>finish([['parameters','stale prompt\nSteps: 7, Sampler: Euler, CFG scale: 3, Seed: 11, Size: 512x512']]));
 if(change!=='unmount')expect(screen.getByLabelText('Prompt')).not.toHaveProperty('value','stale prompt');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test('More closes on its own summary and outside clicks; metadata actions apply settings and reuse only the prompt',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 localStorage.setItem('deas.pref.studio.run.pipeline','"i2i"');
 const job={...heldJob('more-menu','succeeded'),outputRoot:'/old-env/output',outputs:[{filename:'result.png',subfolder:'raw',type:'output' as const}]};
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
 vi.mocked(readImageMetadata).mockResolvedValue([['parameters','imported prompt\nSteps: 7, Sampler: Euler, CFG scale: 3, Seed: 11, Size: 512x512, Model: a.safetensors']]);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/old-env/output'}}/></I18nProvider>);
 const more=await waitFor(()=>{const el=document.querySelector<HTMLButtonElement>('[data-run-post]');expect(el).not.toBeNull();return el;});
 const summary=document.querySelector('summary[data-more]')!;
 expect(more!.disabled).toBe(false);
 fireEvent.click(summary);
 expect(summary.closest('details')!.open).toBe(true);
 fireEvent.click(summary);
 expect(summary.closest('details')!.open).toBe(false);
 fireEvent.click(summary);
 fireEvent.mouseDown(document.body);
 expect(summary.closest('details')!.open).toBe(false);
 fireEvent.click(summary);
 fireEvent.click(document.querySelector('[data-more-apply]')!);
 await waitFor(()=>expect(screen.getByText('Only the settings found in this file were applied; missing settings were left unchanged.')).toBeTruthy());
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('imported prompt');
 expect((screen.getByRole('spinbutton',{name:'Seed'}) as HTMLInputElement).value).toBe('11');
 expect(summary.closest('details')!.open).toBe(false);
 fireEvent.click(summary);
 fireEvent.click(document.querySelector('[data-more-reuse]')!);
 await waitFor(()=>expect(screen.getByText('All settings applied.')).toBeTruthy());
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('imported prompt');
 expect((screen.getByRole('spinbutton',{name:'Seed'}) as HTMLInputElement).value).toBe('11');
});
test('a video gets the More menu: its own job refills the prompt, an uncorrelated file reads metadata',async()=>{
 const made={...heldJob('made','succeeded'),outputRoot:'/out',outputs:[{filename:'clip.mp4',subfolder:'',type:'output' as const}],intent:{recipeId:'video-generation',version:1,family:'ltx-2',positivePrompt:'made clip',negativePrompt:'no blur'} as unknown as JobStatus['intent']};
 const bare={...heldJob('bare','succeeded'),outputRoot:'/out',outputs:[{filename:'other.mp4',subfolder:'',type:'output' as const}]};
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([made,bare]));
 render(<I18nProvider><Workbench endpoint="http://original:7801/" roots={{output:'/out'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-job-id="made"]')).not.toBeNull());
 fireEvent.click(document.querySelector('[data-job-id="bare"] .wb-queue-select')!);
 expect((await waitFor(()=>document.querySelector<HTMLButtonElement>('[data-more-apply]')!)).disabled).toBe(false);
 vi.mocked(readImageMetadata).mockResolvedValueOnce([]);
 fireEvent.click(document.querySelector('[data-more-apply]')!);
 await screen.findByText('The shown file carries no reusable settings or prompt.');
 fireEvent.click(document.querySelector('[data-job-id="made"] .wb-queue-select')!);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-more-reuse]')!.disabled).toBe(false));
 fireEvent.click(document.querySelector('[data-more-reuse]')!);
 expect(screen.getByLabelText('Prompt')).toHaveProperty('value','made clip');
 expect(document.querySelector('#run-negative')).toHaveProperty('value','no blur');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test.each(['all','prompt','stale'] as const)('video More reuses validated saved history before broken file metadata (%s)',async mode=>{
 const {imageFormat,imageCompression,...output}=defaultOptions.output!;void imageFormat;void imageCompression;
 const intent={recipeId:'video-generation',version:1,family:'ltx-2',positivePrompt:'historical film',negativePrompt:'',model:ltxModels[0].name,textEncoder:ltxModels[1].name,vae:ltxModels[3].name,seed:42,width:768,height:512,frames:49,frameRate:24,steps:8,cfg:1,sampler:'euler',scheduler:'simple',outputPrefix:'raw/video/history',output,ltx:{textProjection:ltxModels[2].name,audioVae:ltxModels[4].name,passes:1}} as const;
 const old={...recentHistoryRecord(intent),outputRoot:'/out',outputs:[{filename:'clip.mp4',subfolder:'starred/video/date',type:'output' as const}]};
 const bare={...heldJob('history-file','succeeded'),outputRoot:'/out',outputs:old.outputs};
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([bare]));
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,textEncoders:ltxModels.filter(m=>m.category==='text_encoders').map(m=>m.name),vaes:ltxModels.filter(m=>m.category==='vae').map(m=>m.name)}});
 const previous=vi.mocked(invoke).getMockImplementation()!;
 let resolve!:(value:unknown)=>void;
 const snapshot={schemaVersion:1,records:[old],readOnly:false,error:null};
 vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_jobs_history'?(mode==='stale'?new Promise(done=>{resolve=done;}):snapshot):command==='native_jobs_history_outputs'?['available']:previous(command,args));
 try{
  render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/out'}}/></I18nProvider>);
  await waitFor(()=>expect(document.querySelector('[data-job-id="history-file"]')).not.toBeNull());
  fireEvent.click(document.querySelector('[data-job-id="history-file"] .wb-queue-select')!);
  await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-more-apply]')?.disabled).toBe(false));
  fireEvent.click(document.querySelector(mode==='prompt'?'[data-more-reuse]':'[data-more-apply]')!);
  if(mode==='stale'){
   await waitFor(()=>expect(resolve).toBeTypeOf('function'));
   fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'new draft'}});
   await act(async()=>resolve(snapshot));
   expect(screen.getByLabelText('Prompt')).toHaveProperty('value','new draft');
  }else{
   await waitFor(()=>expect(screen.getByLabelText('Prompt')).toHaveProperty('value','historical film'));
   expect(document.querySelector('#run-negative')).toHaveProperty('value','');
   if(mode==='all'){
    expect(document.querySelector('[data-num="steps"]')).toHaveProperty('value','8');
    expect(document.querySelector('[data-num="cfg"]')).toHaveProperty('value','1');
   }
  }
  expect(readImageMetadata).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
 }finally{vi.mocked(invoke).mockImplementation(previous);}
});

test('video metadata reserves preview space without reopening the player and More stays keyboard usable',async()=>{
 const agent=vi.spyOn(navigator,'userAgent','get').mockReturnValue('Linux');
 const previous=vi.mocked(invoke).getMockImplementation()!;
 const status={position:2,duration:20,width:640,height:360,paused:true,muted:false,volume:100};
 vi.mocked(invoke).mockImplementation(async(command,args)=>command.startsWith('native_video_')?status:previous(command,args));
 const job={...heldJob('metadata-video','succeeded'),outputRoot:'/out',outputs:[{filename:'clip.mp4',subfolder:'raw',type:'output' as const}]};
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
 try{
  render(<I18nProvider><Workbench endpoint="http://original:7801/" roots={{output:'/out'}}/></I18nProvider>);
  await waitFor(()=>expect(document.querySelector('[data-job-id="metadata-video"]')).not.toBeNull());
  fireEvent.click(document.querySelector('[data-job-id="metadata-video"] .wb-queue-select')!);
  await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_video_open',expect.anything()));
  const player=document.querySelector('[data-native-video]');
  fireEvent.click(document.querySelector('[data-metadata-toggle]')!);
  expect(document.querySelector('.wb-stage')?.getAttribute('data-video-metadata')).toBe('true');
  const more=document.querySelector<HTMLElement>('[data-more]')!;
  fireEvent.click(more);expect(more.parentElement).toHaveProperty('open',true);
  fireEvent.keyDown(more,{key:'Escape'});expect(more.parentElement).toHaveProperty('open',false);expect(document.activeElement).toBe(more);
  fireEvent.click(document.querySelector('[data-metadata-toggle]')!);
  expect(document.querySelector('.wb-stage')?.hasAttribute('data-video-metadata')).toBe(false);
  expect(document.querySelector('[data-native-video]')).toBe(player);
  expect(vi.mocked(invoke).mock.calls.filter(([command])=>command==='native_video_open')).toHaveLength(1);
 }finally{cleanup();vi.mocked(invoke).mockImplementation(previous);agent.mockRestore();}
});

test.each([true,false])('native video playback uses captured root when available (owned=%s)',async owned=>{
 const agent=vi.spyOn(navigator,'userAgent','get').mockReturnValue('Linux');
 try{
  const job={...heldJob('old-video','succeeded'),...(owned?{outputRoot:'/old-env/output'}:{}),outputs:[{filename:'clip.mp4',subfolder:'raw',type:'output' as const}]};
  vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
  render(<I18nProvider><Workbench endpoint="http://new:7802/" roots={{output:'/new-env/output'}}/></I18nProvider>);
  await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_video_open',expect.objectContaining({source:owned?{path:'/old-env/output/raw/clip.mp4'}:{baseUrl:job.baseUrl,filename:'clip.mp4',subfolder:'raw',type:'output'}})));
 }finally{cleanup();agent.mockRestore();}
});
test.each(['reply','lost reply','rejected'] as const)('real IPC admission survives remount before native reservation (%s)',async outcome=>{
 const actual=await vi.importActual<typeof import('../../lib/jobs')>('../../lib/jobs');
 const previous=[vi.mocked(runWorkbenchJob).getMockImplementation(),vi.mocked(readCurrentWorkbench).getMockImplementation(),vi.mocked(pollWorkbench).getMockImplementation()] as const;
 const previousInvoke=vi.mocked(invoke).getMockImplementation();
 let finish!:(job:JobStatus)=>void;let fail!:(error:Error)=>void;
 let snapshot=collection();
 const commands:string[]=[];
 vi.mocked(runWorkbenchJob).mockImplementation(actual.runWorkbenchJob);
 vi.mocked(readCurrentWorkbench).mockImplementation(actual.readCurrentWorkbench);
 vi.mocked(pollWorkbench).mockImplementation(actual.pollWorkbench);
 vi.mocked(invoke).mockImplementation((command,args)=>{
  commands.push(command);
  if(command==='native_workbench_run')return new Promise((resolve,reject)=>{finish=resolve;fail=reject;});
  if(command==='native_workbench_current'||command==='native_workbench_poll')return Promise.resolve(snapshot);
  return previousInvoke!(command,args);
 });
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 const props={endpoint:'http://original:7801/',roots:{models:'/m'}};
 try{
  const first=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
  await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
  fireEvent.click(document.querySelector('[data-run]')!);
  await waitFor(()=>expect(commands.filter(c=>c==='native_workbench_run')).toHaveLength(1));
  vi.useFakeTimers();first.unmount();
  render(<I18nProvider><Workbench {...props}/></I18nProvider>);
  await act(async()=>{});
  const run=document.querySelector<HTMLButtonElement>('[data-run]')!;
  expect(run.disabled).toBe(true);
  fireEvent.click(run);
  expect(commands.filter(c=>c==='native_workbench_run')).toHaveLength(1);
  await act(async()=>{await vi.advanceTimersByTimeAsync(1000);});
  expect(commands.filter(c=>c==='native_workbench_current')).toHaveLength(3);
  expect(commands).not.toContain('native_workbench_poll');
  expect(run.disabled).toBe(true);
  const job=heldJob('late-admitted','queued',props.endpoint);
  await act(async()=>{
   if(outcome!=='rejected')snapshot=collection([job]);
   if(outcome==='reply')finish(job);else fail(new Error(outcome==='lost reply'?'IPC response lost':'Native rejected'));
  });
  // The old mount ignores its reply; only continued collection observation can restore this job.
  expect(document.querySelector('[data-job-id="late-admitted"]')).toBeNull();
  await act(async()=>{await vi.advanceTimersByTimeAsync(1000);});
  expect(commands.filter(c=>c==='native_workbench_current')).toHaveLength(4);
  if(outcome!=='rejected'){
   expect(document.querySelector('[data-job-id="late-admitted"]')).not.toBeNull();
   expect(run.disabled).toBe(false);
   snapshot=collection([{...job,state:'failed'}]);
   await act(async()=>{await vi.advanceTimersByTimeAsync(1000);});
  }
  expect(run.disabled).toBe(false);
  const reads=commands.filter(c=>c.startsWith('native_workbench_')).length;
  await act(async()=>{await vi.advanceTimersByTimeAsync(3000);});
  expect(commands.filter(c=>c.startsWith('native_workbench_'))).toHaveLength(reads);
 }finally{
  cleanup();vi.useRealTimers();
  vi.mocked(runWorkbenchJob).mockImplementation(previous[0]!);
  vi.mocked(readCurrentWorkbench).mockImplementation(previous[1]!);
  vi.mocked(pollWorkbench).mockImplementation(previous[2]!);
  vi.mocked(invoke).mockImplementation(previousInvoke!);
 }
});
test('terminal admission reconciles native eviction before reopening Run',async()=>{
 const old=Array.from({length:20},(_,i)=>heldJob('old-'+i,'failed'));
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection(old));
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(runWorkbenchJob).mockImplementation(async()=>{const job=heldJob('newest','failed');vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([...old.slice(1),job]));return job;});
 render(<I18nProvider><Workbench endpoint="http://original:7801/" roots={{models:'/m'}}/></I18nProvider>);
 const run=document.querySelector<HTMLButtonElement>('[data-run]')!;
 await waitFor(()=>expect(run.disabled).toBe(false));fireEvent.click(run);
 await waitFor(()=>expect(document.querySelector('[data-job-id="newest"]')).not.toBeNull());
 await waitFor(()=>expect(document.querySelectorAll('[data-job-id]')).toHaveLength(20));
 expect(document.querySelector('[data-job-id="old-0"]')).toBeNull();expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
});
test.each(['unknown','preparing'] as const)('ambiguous Run rejection recovers authoritative %s before another admission',async state=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 let recover!:(value:ReturnType<typeof collection>)=>void;
 vi.mocked(runWorkbenchJob).mockImplementation(async()=>{vi.mocked(readCurrentWorkbench).mockReturnValueOnce(new Promise(yes=>{recover=yes;}));throw new Error('IPC response lost');});
 render(<I18nProvider><Workbench endpoint="http://original:7801/" roots={{models:'/m'}}/></I18nProvider>);
 const run=document.querySelector<HTMLButtonElement>('[data-run]')!;
 await waitFor(()=>expect(run.disabled).toBe(false));fireEvent.click(run);
 await screen.findByText('IPC response lost');expect(run.disabled).toBe(true);
 await waitFor(()=>expect(readCurrentWorkbench).toHaveBeenCalledTimes(2));
 const snapshot=state==='unknown'?collection([heldJob('lost-response','unknown')]):collection([],1);
 vi.mocked(pollWorkbench).mockResolvedValue(snapshot);await act(async()=>recover(snapshot));
 expect(run.disabled).toBe(false);expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
 if(state==='unknown')expect(document.querySelector('[data-job-id="lost-response"]')).not.toBeNull();
});
test('unmount after native admission recovers held work without cancelling or resubmitting',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection());
 let release!:(job:JobStatus)=>void;const held=heldJob('admitted','unknown');
 vi.mocked(runWorkbenchJob).mockImplementation(()=>{vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([held]));return new Promise(yes=>{release=yes;});});
 const props={endpoint:'http://original:7801/',roots:{models:'/m'}};
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 const run=document.querySelector<HTMLButtonElement>('[data-run]')!;
 await waitFor(()=>expect(run.disabled).toBe(false));fireEvent.click(run);
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));view.unmount();
 render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-job-id="admitted"]')).not.toBeNull());
 await act(async()=>release({...held,state:'failed'}));
 expect(document.querySelector('[data-job-id="admitted"] [data-state]')?.getAttribute('data-state')).toBe('unknown');
 expect((document.querySelector('[data-run]') as HTMLButtonElement).disabled).toBe(false);
 expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
});
test('collection recovery is read-only, preserves every held job and polls mixed endpoints including unknown',async()=>{
 const held=[heldJob('old','running'),heldJob('uncertain','unknown','http://other:7802/'),heldJob('waiting','queued')];
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection(held,1));
 vi.mocked(pollWorkbench).mockResolvedValueOnce(collection([ {...held[0],state:'succeeded'},held[1],{...held[2],state:'failed'}]));
 render(<I18nProvider><Workbench endpoint="http://changed:9999/"/></I18nProvider>);
 await waitFor(()=>expect(document.querySelectorAll('.wb-queue-output')).toHaveLength(3));
 expect(readCurrentWorkbench).toHaveBeenCalledWith();expect(runWorkbenchJob).not.toHaveBeenCalled();
 expect((document.querySelector('[data-run]') as HTMLButtonElement).disabled).toBe(true);
 await waitFor(()=>expect(pollWorkbench).toHaveBeenCalledTimes(1),{timeout:2500});
 await screen.findByRole('button',{name:/Succeeded/});
 expect(document.querySelectorAll('.wb-queue-output')).toHaveLength(3);
 expect((document.querySelector('[data-run]') as HTMLButtonElement).disabled).toBe(true);
 expect(pollWorkbench).toHaveBeenCalledWith();expect(readH3Session).not.toHaveBeenCalled();
});
test('collection IPC failure never fabricates unknown states or removes unrelated peers',async()=>{
 const held=[heldJob('one','running'),heldJob('two','queued')];
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection(held));
 vi.mocked(pollWorkbench).mockRejectedValueOnce(new Error('IPC unavailable')).mockResolvedValue(collection(held));
 render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 await screen.findByText('IPC unavailable',{}, {timeout:2500});
 expect(screen.getAllByRole('button',{name:/Running/}).length).toBeGreaterThan(0);
 expect(screen.getByRole('button',{name:/Queued/})).toBeTruthy();
 expect(screen.queryByRole('button',{name:/Unknown/})).toBeNull();
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test('late recovery after unmount cannot replace a new Workbench collection',async()=>{
 let resolve!:(value:ReturnType<typeof collection>)=>void;
 vi.mocked(readCurrentWorkbench).mockReturnValueOnce(new Promise(yes=>{resolve=yes;}));
 const view=render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 await waitFor(()=>expect(readCurrentWorkbench).toHaveBeenCalledTimes(1));view.unmount();
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([heldJob('new','cancelled')]));
 render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 await screen.findByRole('button',{name:/Cancelled/});
 await act(async()=>resolve(collection([heldJob('stale','running')])));
 expect(screen.queryByRole('button',{name:/Running/})).toBeNull();expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test('unresolved recovery blocks admission; recovered preparation keeps Run and Stop reachable',async()=>{
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 let resolve!:(value:ReturnType<typeof collection>)=>void;
 vi.mocked(readCurrentWorkbench).mockReturnValueOnce(new Promise(yes=>{resolve=yes;}));
 vi.mocked(pollWorkbench).mockResolvedValue(collection());
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await screen.findByRole('option',{name:'a.safetensors'});
 const run=screen.getByRole('button',{name:'Run'}) as HTMLButtonElement;
 expect(run.disabled).toBe(true);fireEvent.click(run);expect(runWorkbenchJob).not.toHaveBeenCalled();
 await act(async()=>resolve(collection([],1)));
 expect(run.disabled).toBe(false);expect(screen.getByRole('button',{name:'Stop'})).toBeTruthy();
 await waitFor(()=>expect(run.disabled).toBe(false),{timeout:2500});
 expect(runWorkbenchJob).not.toHaveBeenCalled();await waitFor(()=>expect(pollWorkbench).toHaveBeenCalledTimes(1),{timeout:2500});
});
test('late collection poll after unmount cannot alter a remounted queue',async()=>{
 vi.mocked(readCurrentWorkbench).mockResolvedValueOnce(collection([heldJob('old','running')]));
 let resolve!:(value:ReturnType<typeof collection>)=>void;
 vi.mocked(pollWorkbench).mockReturnValueOnce(new Promise(yes=>{resolve=yes;}));
 const view=render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 await waitFor(()=>expect(pollWorkbench).toHaveBeenCalledTimes(1),{timeout:2500});view.unmount();
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([heldJob('new','cancelled')]));
 render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 await screen.findByRole('button',{name:/Cancelled/});
 await act(async()=>resolve(collection([heldJob('old','succeeded')])));
 expect(screen.queryByRole('button',{name:/Succeeded/})).toBeNull();expect(runWorkbenchJob).not.toHaveBeenCalled();
});
const clickMedia=(image:HTMLImageElement,clientX=400,clientY=300)=>{
 if(!image.naturalWidth)Object.defineProperties(image,{naturalWidth:{configurable:true,value:1024},naturalHeight:{configurable:true,value:640}});
 const bounds=Object.getOwnPropertyDescriptor(image,'getBoundingClientRect');
 Object.defineProperty(image,'getBoundingClientRect',{configurable:true,value:()=>({left:0,top:0,width:800,height:600})});
 fireEvent.click(image,{clientX,clientY});
 if(bounds)Object.defineProperty(image,'getBoundingClientRect',bounds);else Reflect.deleteProperty(image,'getBoundingClientRect');
};
test.each([
 ['BACKEND_UNAVAILABLE', 'Backend not reachable'],
 ['BACKEND_TIMEOUT', 'did not respond in time'],
])('Studio displays friendly discovery failure %s without claiming the model is ready',async(code,expected)=>{
 const previous=vi.mocked(checkRecipePrerequisites).getMockImplementation()!;
 vi.mocked(checkRecipePrerequisites).mockRejectedValue(new Error(`${code}: object_info request failed http://127.0.0.1:45519/object_info`));
 try{
  render(<I18nProvider><Workbench endpoint="http://127.0.0.1:45519"/></I18nProvider>);
  dock('Models');
  fireEvent.click(document.querySelector('[data-run-load]')!);
  await screen.findByText(new RegExp(expected));
  expect(document.body.textContent).not.toContain('object_info request failed');
  expect((document.querySelector('[data-run]') as HTMLButtonElement).disabled).toBe(true);
  expect(runWorkbenchJob).not.toHaveBeenCalled();
 }finally{vi.mocked(checkRecipePrerequisites).mockImplementation(previous);}
});

test.each(['success','failure'] as const)('switching model folders scans the installed root and ignores an old scan %s',async(outcome)=>{
 let resolve!:(models:Awaited<ReturnType<typeof scanModels>>)=>void;let reject!:(reason:Error)=>void;
 const old=new Promise<Awaited<ReturnType<typeof scanModels>>>((yes,no)=>{resolve=yes;reject=no;});
 const model=(name:string)=>({category:'checkpoints',name,family:'sdxl',path:'/installed/'+name,size:1,info:null});
 vi.mocked(scanModels).mockImplementation(path=>path==='/old'?old:Promise.resolve([model('installed.safetensors')]));
 const view=render(<I18nProvider><Workbench endpoint="" roots={{models:'/old'}}/></I18nProvider>);
 await waitFor(()=>expect(scanModels).toHaveBeenCalledWith('/old',[]));
 view.rerender(<I18nProvider><Workbench endpoint="" roots={{models:'/installed'}}/></I18nProvider>);
 await waitFor(()=>expect(scanModels).toHaveBeenCalledWith('/installed',[]));
 dock('Models');await screen.findByRole('button',{name:'installed.safetensors'});
 await act(async()=>{if(outcome==='success')resolve([model('stale.safetensors')]);else reject(new Error('stale scan failed'));});
 expect(screen.getByRole('button',{name:'installed.safetensors'})).toBeTruthy();
 expect(screen.queryByRole('button',{name:'stale.safetensors'})).toBeNull();
 expect(screen.queryByText('stale scan failed')).toBeNull();
});

test('separate Presets saves live settings, reloads across remounts and updates explicitly without executing',async()=>{
 let stored:unknown[]=[];
 vi.mocked(invoke).mockImplementation(async(command,args)=>{
  if(command==='native_presets_read')return structuredClone(stored);
  if(command==='native_presets_write'){stored=structuredClone((args as {presets:unknown[]}).presets);return;}
  return {path:'/input',parent:'/',entries:[],truncated:false};
 });
 const mount=()=>render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 let view=mount();
 expect(screen.getByRole('tab',{name:'Wildcards'})).toBeTruthy();
 fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'saved prompt'}});
 fireEvent.change(screen.getByLabelText('Steps'),{target:{value:'31'}});
 dock('Presets');
 await waitFor(()=>expect((screen.getByRole('button',{name:'New preset…'}) as HTMLButtonElement).disabled).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'New preset…'}));
 const presetDialog=screen.getByRole('dialog',{name:'New preset'});
 fireEvent.change(within(presetDialog).getByRole('textbox',{name:'Preset name'}),{target:{value:'My settings'}});
 fireEvent.click(within(presetDialog).getByRole('button',{name:'Save preset'}));
 await screen.findByText('Preset saved.');
 expect(stored).toEqual([{name:'My settings',settings:expect.objectContaining({'studio.run.positive':'saved prompt','studio.run.steps':31})}]);
 expect((stored[0] as {settings:object}).settings).not.toHaveProperty('studio.run.recentSeeds');
 expect((stored[0] as {settings:object}).settings).not.toHaveProperty('studio.run.continuity');
 // Only differences from the defaults are stored, so later default changes reach the rest.
 expect((stored[0] as {settings:object}).settings).not.toHaveProperty('studio.run.negative');
 fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'changed'}});
 view.unmount();view=mount();
 const menu=async()=>{fireEvent.contextMenu(await screen.findByRole('button',{name:/My settings/}));return within(screen.getByRole('menu',{name:'My settings'}));};
 fireEvent.click((await menu()).getByRole('menuitem',{name:'Apply'}));
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('saved prompt');
 expect((screen.getByLabelText('Steps') as HTMLInputElement).value).toBe('31');
 fireEvent.change(screen.getByLabelText('Steps'),{target:{value:'27'}});
 const confirm=vi.spyOn(window,'confirm').mockReturnValue(false);
 fireEvent.click((await menu()).getByRole('menuitem',{name:'Overwrite with current settings'}));
 expect((stored[0] as {settings:Record<string,unknown>}).settings['studio.run.steps']).toBe(31);
 confirm.mockReturnValue(true);fireEvent.click((await menu()).getByRole('menuitem',{name:'Overwrite with current settings'}));
 await screen.findByText('Preset saved.');
 expect((stored[0] as {settings:Record<string,unknown>}).settings['studio.run.steps']).toBe(27);
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
 confirm.mockRestore();
});

test('Linux native input never creates a WebKit video decoder',async()=>{
 vi.spyOn(window.navigator,'userAgent','get').mockReturnValue('Linux');
 vi.mocked(invoke).mockResolvedValue({path:'/input',parent:'/',entries:[{name:'clip.webm',path:'/input/clip.webm',kind:'file',media:'video',size:1,modified:1}],truncated:false});
 const view=render(<I18nProvider><Workbench endpoint="" roots={{input:'/input'}}/></I18nProvider>);
 fireEvent.click(await screen.findByRole('button',{name:'clip.webm'}));
 expect(view.container.querySelector('video')).toBeNull();
 expect(view.container.querySelector('[data-native-video]')).not.toBeNull();
 vi.restoreAllMocks();
});
const adopt=()=>fireEvent.click(document.querySelector('.wb-stage > [data-media-tools] [data-use-input]')!);
const dock=(name:string)=>{if(document.querySelector('.references-overlay[open]'))fireEvent.click(within(screen.getByRole('dialog',{name:'References'})).getByRole('button',{name:'Close'}));fireEvent.click(name==='References'?document.querySelector('[data-open-references]')!:screen.getByRole('tab',{name}));};
// The pipeline picker is one select at the top of the settings; disabled options name pipelines without a runnable model.
const pipe=(name:string)=>screen.getByRole('option',{name}) as HTMLOptionElement;
const choosePipe=(name:string)=>fireEvent.change(screen.getByRole('combobox',{name:'Pipeline'}),{target:{value:pipe(name).value}});
// HTML5 drag of a dock tile onto the editor: the tile's dragstart fills a DataTransfer the drop reads back.
const dropTile=(name:string)=>{const store:Record<string,string>={};const dt={clearData:()=>{for(const key of Object.keys(store))delete store[key];},setData:(k:string,v:string)=>{store[k]=v;},getData:(k:string)=>store[k]??'',get types(){return Object.keys(store);}};
 fireEvent.dragStart(screen.getByRole('button',{name}),{dataTransfer:dt});fireEvent.drop(document.querySelector('[data-editor-viewport]')!,{dataTransfer:dt});};
test.each(['/output','/external'])('media thumbnail drop retains the prompt and uploads ReVision input on explicit Run (root=%s)',async output=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'CLIPVisionLoader','LoadImage','CLIPVisionEncode','unCLIPConditioning'],visionModels:['clip_vision_g.safetensors']}});
 vi.mocked(invoke).mockImplementation(async command=>command==='native_media_star_status'?{starred:false,unavailable:null}:{path:'/output',parent:null,entries:[{name:'ref.png',path:'/output/ref.png',kind:'file',media:'image',size:1,modified:1,symlink:false}],truncated:false});
 vi.mocked(uploadInput).mockResolvedValue('deas-inputs/ref.png');
 vi.mocked(runWorkbenchJob).mockResolvedValue(heldJob('reference-job','failed'));
 localStorage.setItem('deas.pref.studio.run.positive','"unchanged prompt"');
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output}}/></I18nProvider>);
 const tile=await screen.findByRole('button',{name:'ref.png'});
 const data=new Map([['text/uri-list','asset://localhost/thumbnail.png']]);
 const transfer={clearData:()=>data.clear(),setData:(k:string,v:string)=>data.set(k,v),getData:(k:string)=>data.get(k)??'',get types(){return [...data.keys()];},effectAllowed:'uninitialized',dropEffect:'none'};
 const image=await waitFor(()=>{const image=tile.querySelector('img');expect(image).not.toBeNull();return image!;});
 fireEvent.dragStart(image,{dataTransfer:transfer});
 expect(transfer.types).toEqual(['application/x-deas-media','application/x-deas-image']);
 const prompt=screen.getByRole('textbox',{name:'Prompt'});
 expect(fireEvent.dragOver(prompt,{dataTransfer:transfer})).toBe(false);
 expect(transfer.dropEffect).toBe('copy');
 fireEvent.drop(prompt,{dataTransfer:transfer});
 expect(document.querySelector('[data-reference-controls] img')?.getAttribute('alt')).toBe('ref.png');
 expect((prompt as HTMLTextAreaElement).value).toBe('unchanged prompt');
 expect(uploadInput).not.toHaveBeenCalled();
 fireEvent.change(document.querySelector('[data-reference-chip] select')!,{target:{value:'clip_vision_g.safetensors'}});
 fireEvent.click(document.querySelector('[data-run]')!);
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(uploadInput).toHaveBeenCalledWith(evidence.baseUrl,'/output/ref.png');
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({positivePrompt:'unchanged prompt',imageReference:{visionModel:'clip_vision_g.safetensors',images:[{image:'deas-inputs/ref.png',strength:0.6}]}});
});
const promptDrop=(media='image',path:string|undefined='/outside/ref.png')=>{const name=typeof path==='string'?path.split(/[\\/]/).at(-1)||'ref.png':'ref.png';const item={name,src:`asset://localhost/${name}`,media,path};const transfer={types:['application/x-deas-media'],getData:()=>JSON.stringify(item),dropEffect:'none'};const prompt=screen.getByRole('textbox',{name:'Prompt'});fireEvent.dragOver(prompt,{dataTransfer:transfer});fireEvent.drop(prompt,{dataTransfer:transfer});return transfer;};
const referenceNodes=['TextEncodeQwenImage21','LoadImage','ImageScale'];
const referenceModels=(family='qwen-image-2.1')=>[
 {category:'diffusion_models',name:'qwen.safetensors',family,path:'/m/q',size:1,info:null},
 {category:'text_encoders',name:family==='krea-2'?'qwen3vl_4b.safetensors':family==='moss-image'?'qwen3.5_9b.safetensors':'qwen3vl_8b.safetensors',path:'/m/e',size:1,info:null},
 {category:'vae',name:family==='krea-2'?'qwen_image_vae.safetensors':family==='moss-image'?'flux2-vae.safetensors':'qwen_image_2.1_vae.safetensors',path:'/m/v',size:1,info:null},
 {category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null},
 {category:'diffusion_models',name:'anima.safetensors',family:'anima',path:'/m/anima',size:1,info:null}
];
test.each(['qwen-image-2.1','qwen-image-2.1-turbo','krea-2','moss-image'])('native prompt references preview and submit an immutable upload snapshot (%s)',async family=>{
 const method=family==='qwen-image-2.1-turbo'?'qwen-image-2.1':family;
 const width=method==='qwen-image-2.1'?1024:1008;
 vi.mocked(scanModels).mockResolvedValue(referenceModels(family));
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,...referenceNodes,'DEAS_ImagePromptEncode','UNETLoader','CLIPLoader','VAELoader','EmptyFlux2LatentImage'],clipTypes:['moss_image'],visionModels:['clip_vision_g.safetensors']}});
 vi.mocked(runWorkbenchJob).mockResolvedValue(heldJob('qwen-reference','failed'));
 localStorage.setItem('deas.pref.studio.run.checkpoint','"qwen.safetensors"');
 localStorage.setItem('deas.pref.studio.run.width','1008');localStorage.setItem('deas.pref.studio.run.height','1008');
 const props={endpoint:evidence.baseUrl,roots:{models:'/m'}};
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 expect((screen.getByLabelText('Width') as HTMLInputElement).value).toBe('1008');
 expect(promptDrop().dropEffect).toBe('copy');
 expect(document.querySelector('[data-reference-controls] img')?.getAttribute('alt')).toBe('ref.png');
 expect(document.querySelector('[data-reference-chip] select')).toBeNull();
 expect(screen.queryByLabelText('Reference strength')).toBeNull();
 expect((screen.getByLabelText('Width') as HTMLInputElement).value).toBe(String(width));
 view.rerender(<I18nProvider><Workbench {...props} planVisible/></I18nProvider>);
 const preview=JSON.parse(document.querySelector('[data-plan-intent]')!.textContent!);
 expect(preview.nativeImageReference).toEqual({method,images:['/outside/ref.png']});
 expect(document.querySelector('.run-plan')!.textContent).toContain('ref.png');
 expect(uploadInput).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();
 view.rerender(<I18nProvider><Workbench {...props}/></I18nProvider>);
 let release!:(name:string)=>void;vi.mocked(uploadInput).mockReturnValueOnce(new Promise(yes=>{release=yes;}));
 fireEvent.click(document.querySelector('[data-run]')!);
 await waitFor(()=>expect(uploadInput).toHaveBeenCalledWith(evidence.baseUrl,'/outside/ref.png'));
 fireEvent.click(document.querySelector('[data-reference-remove]')!);
 expect((screen.getByLabelText('Width') as HTMLInputElement).value).toBe('1008');
 fireEvent.change(screen.getByRole('combobox',{name:'Checkpoint'}),{target:{value:'a.safetensors'}});
 await act(async()=>release('deas-inputs/captured.png'));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({checkpoint:'qwen.safetensors',width,height:width,nativeImageReference:{method,images:['deas-inputs/captured.png']}});
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).not.toHaveProperty('imageReference');
});
test('vision offer card shows the hosting platform and downloads via the Library flow',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 let installed=false,started=false;
 const file={id:'cv',label:'clip_vision_g.safetensors',locator:'clip_vision_g.safetensors',downloadable:true,kind:'file',url:'https://huggingface.co/file',size:1};
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'CLIPVisionLoader','LoadImage','CLIPVisionEncode','unCLIPConditioning'],visionModels:installed?['clip_vision_g.safetensors']:[]}});
 vi.mocked(invoke).mockImplementation(async c=>{
  if(c==='native_download_catalog')return {version:1,providers:[{id:'huggingface',kind:'huggingface',label:'Hugging Face',origin:'https://huggingface.co',scopes:['repo'],anonymousNotice:''},{id:'civitai',kind:'civitai',label:'CivitAI',origin:'https://civitai.com',scopes:['model','search'],anonymousNotice:''}],entries:[{id:'clip-vision-revision',label:'SDXL ReVision vision tower',providerId:'huggingface',scope:'repo',locator:'models:comfyanonymous/clip_vision_g',kind:'repo',target:'clip_vision'},{id:'clip-vision-revision-civitai',label:'ReVision CivitAI mirror',providerId:'civitai',scope:'search',locator:'clip vision bigG',kind:'model',target:'clip_vision'}]};
  if(c==='native_auth_status')return [];
  if(c==='native_download_browse')return {resources:[file],warnings:[],nextCursor:null};
  if(c==='native_files_present')return [installed];
  if(c==='native_download_plan')return {id:'cvplan',target:'/configured/clip_vision',resources:[file],canExecute:true,warnings:[],totalKnownBytes:1,unknownSizeCount:0};
  if(c==='native_download_start'){started=true;return {state:'running',completed:0,total:1,bytesDownloaded:0,error:null};}
  if(c==='native_download_status'){if(!started)return {state:'idle',completed:0,total:0,bytesDownloaded:0,error:null};installed=true;return {state:'succeeded',completed:1,total:1,bytesDownloaded:1,error:null};}
  return {path:'/input',parent:'/',entries:[],truncated:false};
 });
 const view=render(<I18nProvider><Workbench endpoint={evidence.baseUrl} root="/env" roots={{models:'/configured'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-model-picker]')?.textContent).toContain('a.safetensors'));
 expect(promptDrop().dropEffect).toBe('copy');
 const dialog=await screen.findByRole('dialog',{name:/No CLIP vision model installed/});
 expect(document.querySelector('[data-vision-download]')).not.toBeNull();
 for(const [text,tag] of [['SDXL ReVision vision tower','span'],['ReVision CivitAI mirror','span'],['Hugging Face','em.download-auth'],['CivitAI','em.download-auth']] as const)expect(within(dialog).getByText(text,{selector:tag})).not.toBeNull();
 const before=vi.mocked(checkRecipePrerequisites).mock.calls.length;
 await within(dialog).findByLabelText('clip_vision_g.safetensors');
 fireEvent.click(within(dialog).getByLabelText('clip_vision_g.safetensors'));
 fireEvent.click(within(dialog).getByRole('button',{name:'Download all (1)'}));
 await within(dialog).findByText('Running');
 for(let i=0;i<4;i++){
  await act(async()=>{await new Promise(resolve=>setTimeout(resolve,300));});
  view.rerender(<I18nProvider><Workbench endpoint={evidence.baseUrl} root="/env" roots={{models:'/configured'}}/></I18nProvider>);
 }
 await screen.findByText('Succeeded',undefined,{timeout:2000});
 expect(invoke).toHaveBeenCalledWith('native_download_plan',{request:{providerId:'huggingface',resources:[file],parentPath:'/configured/clip_vision',folderName:'',authenticated:false}});
 await waitFor(()=>expect(checkRecipePrerequisites).toHaveBeenCalledTimes(before+1));
});
function reorderReferences(from:number,to:number){
 const cards=document.querySelectorAll('[data-reference-controls]'),data=new Map<string,string>();
 const transfer={clearData:()=>data.clear(),setData:(key:string,value:string)=>data.set(key,value),getData:(key:string)=>data.get(key)??'',get types(){return [...data.keys()];},effectAllowed:'uninitialized',dropEffect:'none'};
 fireEvent.dragStart(cards[from],{dataTransfer:transfer});fireEvent.dragOver(cards[to],{dataTransfer:transfer});fireEvent.drop(cards[to],{dataTransfer:transfer});fireEvent.dragEnd(cards[from],{dataTransfer:transfer});
}
test.each(['qwen-image-2.1','qwen-image-2.1-turbo','krea-2','moss-image'])('reference thumbnails reorder, renumber and submit captured positions (%s)',async family=>{
 vi.mocked(scanModels).mockResolvedValue(referenceModels(family));
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,...referenceNodes,'DEAS_ImagePromptEncode','UNETLoader','CLIPLoader','VAELoader','EmptyFlux2LatentImage'],clipTypes:['moss_image']}});
 localStorage.setItem('deas.pref.studio.run.checkpoint','"qwen.safetensors"');
 vi.mocked(runWorkbenchJob).mockResolvedValue(heldJob('reordered-reference','failed'));
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 const cards=()=>Array.from(document.querySelectorAll<HTMLElement>('[data-reference-controls]'));
 const names=()=>cards().map(card=>card.querySelector('img')!.alt);
 promptDrop('image','/outside/one.png');promptDrop('image','/outside/two.png');
 const data=new Map([['text/uri-list','asset://localhost/one.png'],['application/x-deas-media','stale']]);
 const transfer={clearData:()=>data.clear(),setData:(key:string,value:string)=>data.set(key,value),getData:(key:string)=>data.get(key)??'',get types(){return [...data.keys()];},effectAllowed:'uninitialized',dropEffect:'none'};
 fireEvent.dragStart(cards()[0],{dataTransfer:transfer});
 expect(transfer.types).toEqual(['application/x-deas-reference-order']);expect(transfer.effectAllowed).toBe('move');
 const prompt=screen.getByRole('textbox',{name:'Prompt'});
 expect(fireEvent.dragOver(prompt,{dataTransfer:transfer})).toBe(true);fireEvent.drop(prompt,{dataTransfer:transfer});
 expect(names()).toEqual(['one.png','two.png']);
 expect(fireEvent.dragOver(cards()[1],{dataTransfer:transfer})).toBe(false);expect(transfer.dropEffect).toBe('move');
 fireEvent.drop(cards()[1],{dataTransfer:transfer});fireEvent.dragEnd(cards()[0],{dataTransfer:transfer});
 expect(names()).toEqual(['two.png','one.png']);
 promptDrop('image','/outside/three.png');promptDrop('image','/outside/four.png');
 expect(promptDrop('image','/outside/five.png').dropEffect).toBe('none');expect(cards()).toHaveLength(4);
 expect(cards().map(card=>card.dataset.referencePosition)).toEqual(['1','2','3','4']);
 expect(cards().map(card=>card.querySelector('.wb-reference-position')!.textContent)).toEqual(['1','2','3','4']);
 for(const card of cards()){
  const img=card.querySelector('img')!;expect(img.draggable).toBe(false);expect(img.title).toBe(img.alt);
  expect(card.textContent).not.toContain('.png');
  for(const button of card.querySelectorAll('button'))expect(button.type).toBe('button');
 }
 expect(document.querySelector('[data-reference-move]')).toBeNull();
 reorderReferences(1,2);
 expect(names()).toEqual(['two.png','three.png','one.png','four.png']);
 let release!:(name:string)=>void;
 vi.mocked(uploadInput).mockReturnValueOnce(new Promise(yes=>{release=yes;})).mockImplementation(async(_endpoint,path)=>`uploaded/${path.split('/').at(-1)}`);
 fireEvent.click(document.querySelector('[data-run]')!);
 await waitFor(()=>expect(uploadInput).toHaveBeenCalledTimes(4));
 reorderReferences(3,2);
 fireEvent.click(screen.getByRole('button',{name:'Remove reference · 2'}));
 expect(names()).toEqual(['two.png','four.png','one.png']);
 expect(cards().map(card=>card.dataset.referencePosition)).toEqual(['1','2','3']);
 await act(async()=>release('uploaded/two.png'));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(uploadInput).mock.calls.map(call=>call[1])).toEqual(['/outside/two.png','/outside/three.png','/outside/one.png','/outside/four.png']);
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({nativeImageReference:{method:family==='qwen-image-2.1-turbo'?'qwen-image-2.1':family,images:['uploaded/two.png','uploaded/three.png','uploaded/one.png','uploaded/four.png']}});
});
test.each([
 ['en','Reference 1'],['de','Referenz 1'],['fr','Référence 1'],
 ['ja','参照 1'],['tr','Referans 1'],['zh','参考图 1'],
])('reference controls are localized and duplicate attachments remain independent (%s)',async(locale,position)=>{
 localStorage.setItem('deas.locale.v1',locale);localStorage.setItem('deas.pref.studio.run.checkpoint','"qwen.safetensors"');
 vi.mocked(scanModels).mockResolvedValue(referenceModels());
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,...referenceNodes]}});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 const item={name:'same.png',src:'asset://localhost/same.png',path:'/outside/same.png',media:'image'};
 const transfer={types:['application/x-deas-media'],getData:()=>JSON.stringify(item)};
 for(let i=0;i<2;i++)fireEvent.drop(document.querySelector('[data-run-positive]')!,{dataTransfer:transfer});
 expect(screen.getByRole('group',{name:position})).toBeTruthy();
 reorderReferences(0,1);
 expect(document.querySelectorAll('[data-reference-controls]')).toHaveLength(2);
 fireEvent.click(document.querySelector('[data-reference-remove]')!);
 expect(document.querySelectorAll('[data-reference-controls]')).toHaveLength(1);
 expect(document.querySelector('[data-reference-controls]')?.getAttribute('data-reference-position')).toBe('1');
 expect(uploadInput).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test('ReVision uploads ordered external references with captured vision model and strengths despite authoring edits',async()=>{
 vi.mocked(scanModels).mockResolvedValue(referenceModels());
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'CLIPVisionLoader','LoadImage','CLIPVisionEncode','unCLIPConditioning'],visionModels:['first.safetensors','second.safetensors']}});
 localStorage.setItem('deas.pref.studio.run.checkpoint','"a.safetensors"');
 localStorage.setItem('deas.pref.studio.run.imageRef.vision','"first.safetensors"');
 vi.mocked(runWorkbenchJob).mockResolvedValue(heldJob('revision-snapshot','failed'));
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 promptDrop();promptDrop('image','C:\\Pictures\\second.png');
 fireEvent.change(screen.getAllByLabelText('Reference strength')[1],{target:{value:'0.2'}});
 reorderReferences(1,0);
 expect(screen.getAllByLabelText('Reference strength').map(input=>(input as HTMLInputElement).value)).toEqual(['0.2','0.6']);
 let release!:(name:string)=>void;
 vi.mocked(uploadInput).mockReturnValueOnce(new Promise(yes=>{release=yes;})).mockResolvedValueOnce('deas-inputs/second.png');
 fireEvent.click(document.querySelector('[data-run]')!);
 await waitFor(()=>expect(uploadInput).toHaveBeenCalledTimes(2));
 reorderReferences(0,1);
 fireEvent.change(screen.getByLabelText('CLIP-vision model'),{target:{value:'second.safetensors'}});
 fireEvent.change(screen.getAllByLabelText('Reference strength')[0],{target:{value:'0.9'}});
 fireEvent.click(document.querySelector('[data-reference-remove]')!);
 fireEvent.change(screen.getByRole('combobox',{name:'Checkpoint'}),{target:{value:'qwen.safetensors'}});
 await act(async()=>release('deas-inputs/first.png'));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(uploadInput).mock.calls.map(call=>call[1])).toEqual(['C:\\Pictures\\second.png','/outside/ref.png']);
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({checkpoint:'a.safetensors',imageReference:{visionModel:'first.safetensors',images:[{image:'deas-inputs/first.png',strength:0.2},{image:'deas-inputs/second.png',strength:0.6}]}});
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).not.toHaveProperty('nativeImageReference');
});
test.each(['qwen-image-2.1','qwen-image-2.1-turbo','krea-2','moss-image'])('native references coexist with the image-to-image source (%s)',async family=>{
 const method=family==='qwen-image-2.1-turbo'?'qwen-image-2.1':family,width=method==='qwen-image-2.1'?992:1008;
 vi.mocked(scanModels).mockResolvedValue(referenceModels(family));
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,...referenceNodes,'DEAS_ImagePromptEncode','UNETLoader','CLIPLoader','VAELoader','EmptyFlux2LatentImage','VAEEncode'],clipTypes:['moss_image']}});
 vi.mocked(invoke).mockImplementation(async()=>({path:'/env/input',parent:'/env',entries:[{name:'source.png',path:'/env/input/source.png',kind:'file',symlink:false,media:'image',size:1,modified:1}],truncated:false}));
 vi.mocked(uploadInput).mockResolvedValue('deas-inputs/ref.png');
 vi.mocked(runWorkbenchJob).mockResolvedValue(heldJob('qwen-i2i','failed'));
 localStorage.setItem('deas.pref.studio.run.checkpoint','"qwen.safetensors"');
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',input:'/env/input'}}/></I18nProvider>);
 fireEvent.click(await screen.findByRole('button',{name:'source.png'}));adopt();choosePipe('Image → Image');
 const source=screen.getByRole('img',{name:'source.png'});Object.defineProperties(source,{naturalWidth:{value:1008},naturalHeight:{value:1008}});fireEvent.load(source);
 expect(promptDrop().dropEffect).toBe('copy');
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 fireEvent.click(document.querySelector('[data-run]')!);
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({inputImage:'source.png',width,height:width,nativeImageReference:{method,images:['deas-inputs/ref.png']}});
});
test('native reference upload failure never submits an unresolved local filename',async()=>{
 vi.mocked(scanModels).mockResolvedValue(referenceModels());
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,...referenceNodes]}});
 localStorage.setItem('deas.pref.studio.run.checkpoint','"qwen.safetensors"');
 vi.mocked(uploadInput).mockRejectedValueOnce(new Error('Reference upload failed'));
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));promptDrop();
 fireEvent.click(document.querySelector('[data-run]')!);
 await screen.findByText('Reference upload failed');expect(runWorkbenchJob).not.toHaveBeenCalled();
 expect(document.querySelector('[data-reference-controls] img')?.getAttribute('alt')).toBe('ref.png');
});
test.each(['krea-2','moss-image'])('native adapter references require the installed image-prompt node (%s)',async family=>{
 vi.mocked(scanModels).mockResolvedValue(referenceModels(family));
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'UNETLoader','CLIPLoader','VAELoader','EmptyFlux2LatentImage','LoadImage'],clipTypes:['moss_image']}});
 localStorage.setItem('deas.pref.studio.run.checkpoint','"qwen.safetensors"');
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 expect(promptDrop().dropEffect).toBe('copy');
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(true);
 expect(screen.getByRole('alert').textContent).toContain('DEAS_ImagePromptEncode');
 expect(document.querySelector('[data-reference-chip] select')).toBeNull();expect(screen.queryByLabelText('Reference strength')).toBeNull();
 fireEvent.click(document.querySelector('[data-reference-remove]')!);
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false);
 expect(uploadInput).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test.each(referenceNodes)('native prompt reference blocks Run when %s is missing',async missing=>{
 vi.mocked(scanModels).mockResolvedValue(referenceModels());
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,...referenceNodes.filter(n=>n!==missing)]}});
 localStorage.setItem('deas.pref.studio.run.checkpoint','"qwen.safetensors"');
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));promptDrop();
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(true);
 expect(screen.getByRole('alert').textContent).toContain(missing);
 expect(uploadInput).not.toHaveBeenCalled();
});
test('retains inactive reference chips across unsupported family changes and never submits without them',async()=>{
 vi.mocked(scanModels).mockResolvedValue(referenceModels());
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,...referenceNodes]}});
 localStorage.setItem('deas.pref.studio.run.checkpoint','"qwen.safetensors"');
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));promptDrop();
 fireEvent.change(screen.getByRole('combobox',{name:'Checkpoint'}),{target:{value:'anima.safetensors'}});
 expect(document.querySelector('[data-reference-controls] img')?.getAttribute('alt')).toBe('ref.png');
 expect(document.querySelector('[data-reference-chip]')?.textContent).toContain('inactive');
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(true);
 expect(promptDrop().dropEffect).toBe('none');
 fireEvent.change(screen.getByRole('combobox',{name:'Checkpoint'}),{target:{value:'qwen.safetensors'}});
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false);
 fireEvent.click(document.querySelector('[data-reference-remove]')!);expect(document.querySelector('[data-reference-controls]')).toBeNull();
 expect(uploadInput).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test.each([['audio','/outside/a.wav'],['video','/outside/a.mp4'],['image',''],['image','relative.png'],['image',123]])('prompt rejects invalid reference media/path (%s %s)',async(media,path)=>{
 vi.mocked(scanModels).mockResolvedValue(referenceModels());
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,...referenceNodes]}});
 localStorage.setItem('deas.pref.studio.run.checkpoint','"qwen.safetensors"');
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 expect(promptDrop(String(media),path as string).dropEffect).toBe('none');
 expect(document.querySelector('[data-reference-chip]')).toBeNull();expect(uploadInput).not.toHaveBeenCalled();
});
test('unsupported model refuses prompt image drops without adding a notice below the prompt',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'diffusion_models',name:'anima.safetensors',family:'anima',path:'/m/anima',size:1,info:null}]);
 render(<I18nProvider><Workbench endpoint="" roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('combobox',{name:'Checkpoint'}).getAttribute('title')).toBe('anima.safetensors'));
 const prompt=screen.getByRole('textbox',{name:'Prompt'});
 const item={name:'ref.png',path:'/ref.png',src:'asset://localhost/ref.png',media:'image'};
 const transfer={types:['application/x-deas-media'],getData:()=>JSON.stringify(item),dropEffect:'copy'};
 fireEvent.dragOver(prompt,{dataTransfer:transfer});
 expect(transfer.dropEffect).toBe('none');
 expect(fireEvent.drop(prompt,{dataTransfer:transfer})).toBe(false);
 expect(document.querySelector('[data-reference-chip]')).toBeNull();
 expect(document.querySelector('[data-reference-reject]')).toBeNull();
 expect(uploadInput).not.toHaveBeenCalled();
});
test.each(['image','video','audio'] as const)('protected drag payload advertises only attachable images (%s)',async kind=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,visionModels:['clip_vision_g.safetensors']}});
 const name=`protected-${kind}.${kind==='image'?'png':kind==='video'?'mp4':'wav'}`,path=`/protected-${kind}/${name}`;
 vi.mocked(invoke).mockImplementation(async command=>command==='native_media_star_status'?{starred:false,unavailable:null}:{path:`/protected-${kind}`,parent:null,entries:[{name,path,kind:'file',media:kind,size:1,modified:1,symlink:false}],truncated:false});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:`/protected-${kind}`}}/></I18nProvider>);
 const source=await screen.findByRole('button',{name});
 const data=new Map<string,string>();let readable=false;
 const transfer={clearData:()=>data.clear(),setData:(key:string,value:string)=>data.set(key,value),getData:(key:string)=>readable?(data.get(key)??''):'',get types(){return [...data.keys()];},effectAllowed:'uninitialized',dropEffect:'none'};
 fireEvent.dragStart(source,{dataTransfer:transfer});
 const prompt=screen.getByRole('textbox',{name:'Prompt'});
 fireEvent.dragOver(prompt,{dataTransfer:transfer});
 expect(transfer.dropEffect).toBe(kind==='image'?'copy':'none');
 readable=true;fireEvent.drop(prompt,{dataTransfer:transfer});
 expect(document.querySelectorAll('[data-reference-controls]')).toHaveLength(kind==='image'?1:0);
});
test('column keyboard resizing persists independently and layout reset leaves authoring intact',async()=>{
 vi.mocked(isTauri).mockReturnValue(false);
 render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 const left=screen.getByRole('separator',{name:'Resize Settings'}),right=screen.getByRole('separator',{name:'Resize Queue'});
 fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'keep'}});
 fireEvent.keyDown(left,{key:'ArrowRight'});fireEvent.keyDown(right,{key:'ArrowLeft'});
 expect(Number(left.getAttribute('aria-valuenow'))).toBe(288);expect(Number(right.getAttribute('aria-valuenow'))).toBe(168);
 expect(localStorage.getItem('deas.pref.studio.layout.settingsWidth')).toBe('288');
 act(()=>window.dispatchEvent(new Event('deas:reset-studio-layout')));
 expect(left.getAttribute('aria-valuenow')).toBe('272');expect(right.getAttribute('aria-valuenow')).toBe('152');
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('keep');
});
test('edge arrows fully hide and show Settings and the bottom dock, persist, and layout reset restores them',()=>{
 vi.mocked(isTauri).mockReturnValue(false);
 const view=render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'keep'}});
 const settings=document.getElementById('studio-settings')!,dock=document.getElementById('studio-dock')!;
 fireEvent.click(screen.getByRole('button',{name:'Collapse Settings'}));
 expect(settings.hidden).toBe(true);
 const expand=screen.getByRole('button',{name:'Expand Settings'});
 expect(expand.getAttribute('aria-expanded')).toBe('false');
 // A collapsed pane is not resized by its line.
 fireEvent.keyDown(screen.getByRole('separator',{name:'Resize Settings'}),{key:'ArrowRight'});
 expect(screen.getByRole('separator',{name:'Resize Settings'}).getAttribute('aria-valuenow')).toBe('272');
 fireEvent.click(screen.getByRole('button',{name:'Collapse bottom panel'}));
 expect(dock.hasAttribute('data-collapsed')).toBe(true);
 expect(localStorage.getItem('deas.pref.studio.layout.settingsHidden')).toBe('true');
 expect(localStorage.getItem('deas.pref.studio.dock.hidden')).toBe('true');
 view.unmount();
 render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 expect(document.getElementById('studio-settings')!.hidden).toBe(true);
 fireEvent.click(screen.getByRole('button',{name:'Expand Settings'}));
 expect(document.getElementById('studio-settings')!.hidden).toBe(false);
 act(()=>window.dispatchEvent(new Event('deas:reset-studio-layout')));
 expect(document.getElementById('studio-dock')!.hasAttribute('data-collapsed')).toBe(false);
 expect(screen.getByRole('button',{name:'Collapse bottom panel'}).getAttribute('aria-expanded')).toBe('true');
});
test('unexecuted settings survive layout reset and remount; confirmed defaults survive the next mount',async()=>{
 vi.mocked(scanModels).mockResolvedValue(['base.safetensors','last.safetensors'].map(name=>({category:'checkpoints',name,family:'sdxl',path:'/m/'+name,size:1,info:null})));
 const confirm=vi.spyOn(window,'confirm').mockReturnValue(false);
 const mount=()=>render(<I18nProvider><Workbench endpoint="" roots={{models:'/m'}}/></I18nProvider>);
 let view=mount();
 await waitFor(()=>expect(pipe('Image → Image').disabled).toBe(false));
 dock('Models');fireEvent.click(screen.getByRole('button',{name:'last.safetensors'}));
 choosePipe('Image → Image');
 fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'  unexecuted draft\n'}});
 fireEvent.change(screen.getByLabelText('Steps'),{target:{value:'31'}});
 fireEvent.change(screen.getByLabelText('CFG'),{target:{value:'0'}});
 fireEvent.change(screen.getByLabelText('Width'),{target:{value:'768'}});
 fireEvent.change(screen.getByLabelText('Seed'),{target:{value:'0'}});
 dock('Options');
 fireEvent.change(screen.getByRole('textbox',{name:'Output naming'}),{target:{value:'draft/%seed%'}});
 fireEvent.keyDown(screen.getByRole('separator',{name:'Resize Settings'}),{key:'ArrowRight'});
 act(()=>window.dispatchEvent(new Event('deas:reset-studio-layout')));
 expect(confirm).not.toHaveBeenCalled();
 view.unmount();view=mount();
 await waitFor(()=>expect(pipe('Image → Image').disabled).toBe(false));
 expect(pipe('Image → Image').selected).toBe(true);
 expect((screen.getByRole('combobox',{name:'Checkpoint'}) as HTMLSelectElement).value).toBe('last.safetensors');
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('  unexecuted draft\n');
 for(const [name,value] of [['Steps','31'],['CFG','0'],['Width','768'],['Seed','0']])expect((screen.getByLabelText(name) as HTMLInputElement).value).toBe(value);
 expect(screen.getByRole('button',{name:'Fixed',pressed:true})).toBeTruthy();
 expect((screen.getByRole('textbox',{name:'Output naming'}) as HTMLInputElement).value).toBe('draft/%seed%');
 expect(screen.getByRole('separator',{name:'Resize Settings'}).getAttribute('aria-valuenow')).toBe('272');
 const stored=Object.fromEntries(Object.entries(localStorage));
 act(()=>window.dispatchEvent(new Event('deas:request-studio-defaults')));
 expect(Object.fromEntries(Object.entries(localStorage))).toEqual(stored);
 confirm.mockReturnValue(true);
 act(()=>window.dispatchEvent(new Event('deas:request-studio-defaults')));
 view.unmount();mount();
 await waitFor(()=>expect(pipe('Text → Image').disabled).toBe(false));
 expect(pipe('Text → Image').selected).toBe(true);
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('');
 expect((screen.getByLabelText('Steps') as HTMLInputElement).value).toBe('28');
 expect((screen.getByLabelText('Width') as HTMLInputElement).value).toBe('1024');
 expect((screen.getByRole('textbox',{name:'Output naming'}) as HTMLInputElement).value).toBe(defaultOptions.outputNaming);
 expect(screen.getByRole('button',{name:'Random',pressed:true})).toBeTruthy();
 expect(runWorkbenchJob).not.toHaveBeenCalled();
 confirm.mockRestore();
});
test('column pointer cancellation, independent queue dragging, restored bounds and dock keyboard resize',()=>{
 vi.mocked(isTauri).mockReturnValue(false);
 vi.stubGlobal('PointerEvent',class extends MouseEvent {pointerId:number;constructor(type:string,init:PointerEventInit={}){super(type,init);this.pointerId=init.pointerId??1;}});
 const view=render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 const left=screen.getByRole('separator',{name:'Resize Settings'}),right=screen.getByRole('separator',{name:'Resize Queue'});
 fireEvent.pointerDown(left,{button:0,pointerId:1,clientX:272});fireEvent.pointerMove(window,{pointerId:1,clientX:322});expect(left.getAttribute('aria-valuenow')).toBe('322');
 fireEvent.keyDown(window,{key:'Escape'});expect(left.getAttribute('aria-valuenow')).toBe('272');
 fireEvent.pointerMove(window,{pointerId:1,clientX:500});expect(left.getAttribute('aria-valuenow')).toBe('272');
 fireEvent.pointerDown(right,{button:0,pointerId:1,clientX:700});fireEvent.pointerMove(window,{pointerId:1,clientX:650});fireEvent.pointerUp(window,{pointerId:1});expect(right.getAttribute('aria-valuenow')).toBe('202');
 fireEvent.pointerDown(left,{button:0,pointerId:1,clientX:272});fireEvent.pointerMove(window,{pointerId:1,clientX:350});fireEvent.pointerCancel(window,{pointerId:1});expect(left.getAttribute('aria-valuenow')).toBe('272');
 const bottom=screen.getByRole('separator',{name:'Drag to resize the bottom panel'});fireEvent.keyDown(bottom,{key:'ArrowUp'});expect(bottom.getAttribute('aria-valuenow')).toBe('236');
 view.unmount();render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 expect(screen.getByRole('separator',{name:'Resize Queue'}).getAttribute('aria-valuenow')).toBe('202');
 const shell=document.querySelector('[data-workbench]')!;Object.defineProperty(shell,'clientWidth',{configurable:true,value:910});fireEvent.resize(window);
 fireEvent.keyDown(screen.getByRole('separator',{name:'Resize Settings'}),{key:'End'});
 const settings=Number(screen.getByRole('separator',{name:'Resize Settings'}).getAttribute('aria-valuenow')),queue=Number(screen.getByRole('separator',{name:'Resize Queue'}).getAttribute('aria-valuenow'));
 expect(settings+queue+20+320).toBeLessThanOrEqual(910);
});
test('defaults reset confirms, uses family defaults and leaves jobs, libraries and layout untouched',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'base.safetensors',family:'sdxl',path:'/m/base',size:1,info:null}]);
 const initial:Record<string,unknown>={positive:'edited',checkpoint:'missing.safetensors',options:{...defaultOptions,batch:3},loras:[{name:'missing-lora',strength:0.5}],wildcardEnabled:true,wildcard:{style:'Booru',seed:10,reroll:2,token_budget:300,auto_reroll:true,selection_state:{}},post:true,'post.tiled':true,upscale:2,refineDenoise:0.7};
 for(const [key,value] of Object.entries(initial))localStorage.setItem('deas.pref.studio.run.'+key,JSON.stringify(value));
 localStorage.setItem('library.sentinel','keep');localStorage.setItem('deas.pref.studio.dock.height','250');
 const confirm=vi.spyOn(window,'confirm').mockReturnValue(false);
 render(<I18nProvider><Workbench endpoint="" roots={{models:'/m'}}/></I18nProvider>);
 await screen.findByRole('option',{name:'base.safetensors'});
 await waitFor(()=>expect(pipe('Image → Image').disabled).toBe(false));
 act(()=>window.dispatchEvent(new Event('deas:request-studio-defaults')));
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('edited');
 confirm.mockReturnValue(true);act(()=>window.dispatchEvent(new Event('deas:request-studio-defaults')));
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('');
 expect(localStorage.getItem('deas.pref.studio.run.checkpoint')).toBe('"base.safetensors"');
 expect(localStorage.getItem('deas.pref.studio.run.options')).toBe(JSON.stringify(defaultOptions));
 for(const key of ['post','post.tiled','wildcardEnabled'])expect(localStorage.getItem('deas.pref.studio.run.'+key)).toBe('false');
 expect(localStorage.getItem('deas.pref.studio.run.loras')).toBe('[]');expect(localStorage.getItem('deas.pref.studio.run.upscale')).toBe('1');
 expect(localStorage.getItem('deas.pref.studio.dock.height')).toBe('250');expect(localStorage.getItem('library.sentinel')).toBe('keep');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
 expect((screen.getByRole('spinbutton',{name:'Steps'}) as HTMLInputElement).value).toBe('28');
 confirm.mockRestore();
});
test('starring an adopted preview repairs its location without remounting or losing editor geometry',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 let moved=false;
 vi.mocked(invoke).mockImplementation(async(command,args)=>{
  if(command==='native_media_star_status')return {starred:(args as {path:string}).path.includes('/starred/'),unavailable:null};
  if(command==='native_media_star'){moved=true;return {oldPath:'/out/a.png',path:'/out/starred/a.png'};}
  if(command==='native_media_star_notice')return;
  return {path:'/out',parent:'/',entries:[{name:'a.png',path:moved?'/out/starred/a.png':'/out/a.png',kind:'file',media:'image',symlink:false,size:1,modified:1}],truncated:false};
 });
 render(<I18nProvider><Workbench endpoint="" roots={{models:'/m',output:'/out'}}/></I18nProvider>);
 fireEvent.click(await screen.findByRole('button',{name:'a.png'}));
 const preview=document.querySelector('.wb-stage > [data-media-tools]')!;
 const previewFigure=document.querySelector('[data-browsed-preview]')!;
 await waitFor(()=>expect(within(preview as HTMLElement).getByRole('button',{name:'Star a.png'}).hasAttribute('disabled')).toBe(false));
 clickMedia(previewFigure.querySelector('img')!);
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_media_star_status',{path:'/out/a.png'}));
 fireEvent.keyDown(window,{key:'Escape'});adopt();
 const sourceImage=screen.getByRole('img',{name:'a.png'});
 Object.defineProperties(sourceImage,{naturalWidth:{configurable:true,value:1024},naturalHeight:{configurable:true,value:1024}});fireEvent.load(sourceImage);
 fireEvent.click(screen.getByRole('button',{name:'Select'}));
 const base=document.querySelector<HTMLElement>('[data-layer="base"]')!;
 fireEvent.keyDown(base,{key:'ArrowLeft'});expect(base.style.left).toBe('-8px');
 const editor=document.querySelector('[data-editor-viewport]');
 const toolbar=screen.getByRole('toolbar',{name:'Edit image'});
 await waitFor(()=>expect(within(toolbar).getByRole('button',{name:'Star a.png'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(within(toolbar).getByRole('button',{name:'Star a.png'}));
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_media_star',{path:'/out/a.png'}));
 await waitFor(()=>expect(base.querySelector('img')?.getAttribute('src')).toContain('/out/starred/a.png'));
 expect(document.querySelector('[data-editor-viewport]')).toBe(editor);
 expect(editor?.closest('[hidden]')).toBeNull();
 expect(document.querySelector('[data-browsed-preview]')).toBeNull();
 expect(screen.getByRole('toolbar',{name:'Edit image'})).toBe(toolbar);
 expect(screen.getByRole('button',{name:'a.png'}).getAttribute('aria-current')).toBe('true');
 expect(document.querySelector('[data-layer="base"]')).toBe(base);expect(base.style.left).toBe('-8px');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('stars every queued result and repairs output locations without changing its submitted intent',async()=>{
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(invoke).mockImplementation(async(command,args)=>{
  if(command==='native_media_star_status')return {starred:(args as {path:string}).path.includes('/starred/'),unavailable:null};
  if(command==='native_media_star'){const path=(args as {path:string}).path;return {oldPath:path,path:path.replace('/out/','/out/starred/')};}
  if(command==='native_media_unstar'){const path=(args as {path:string}).path;return {oldPath:path,path:path.replace('/starred/','/')};}
  return {path:'/out',parent:'/',entries:[],truncated:false};
 });
 let intent:JobStatus['intent'];
 const outputs:JobStatus['outputs']=[{filename:'one.png',subfolder:'',type:'output'},{filename:'two.png',subfolder:'',type:'output'}];
 vi.mocked(runWorkbenchJob).mockImplementation(async(_baseUrl,value)=>{intent=value;return ({id:'star-job',baseUrl:evidence.baseUrl,promptId:'p',state:'succeeded',outputRoot:'/out',outputs,intent,error:null,generationReady:false});});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/out'}}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 const queue=screen.getByRole('region',{name:'Queue'});
 await within(queue).findByRole('button',{name:'Star one.png'});
 expect(within(queue).getByRole('button',{name:'Star two.png'})).toBeTruthy();
 for(const name of ['one.png','two.png']){
  const star=within(queue).getByRole('button',{name:`Star ${name}`});
  const tile=star.closest('.wb-queue-output')!;
  expect(tile).not.toBeNull();
  const select=within(tile as HTMLElement).getByRole('button',{name:new RegExp(`Succeeded.*${name.replace('.', '\\.')}`)});
  expect(select.contains(star)).toBe(false);expect(star.closest('button')).toBe(star);
  expect(select.querySelector('img')?.getAttribute('src')).toContain(name);
  select.focus();expect(document.activeElement).toBe(select);
  fireEvent.click(select);expect(select.getAttribute('aria-current')).toBe('true');
  star.focus();expect(document.activeElement).toBe(star);
 }
 expect(queue.querySelector('button button')).toBeNull();
 const snapshot=JSON.stringify(intent);
 const result=document.querySelector('.wb-shown')!;
 await waitFor(()=>expect(within(result as HTMLElement).getByRole('button',{name:'Star one.png'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(within(result as HTMLElement).getByRole('button',{name:'Star one.png'}));
 await waitFor(()=>expect(result.querySelector('img')?.getAttribute('src')).toContain('/out/starred/one.png'));
 expect(within(queue).getByRole('button',{name:'Unstar one.png'})).toBeTruthy();
 const selectedBefore=Array.from(queue.querySelectorAll('.wb-queue-select')).map(button=>button.getAttribute('aria-current'));
 await waitFor(()=>expect(within(queue).getByRole('button',{name:'Star two.png'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(within(queue).getByRole('button',{name:'Star two.png'}));
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_media_star',{path:'/out/two.png'}));
 expect(within(queue).getByRole('button',{name:'Unstar two.png'}).closest('.wb-queue-output')?.querySelector('img')?.getAttribute('src')).toContain('/out/starred/two.png');
 await waitFor(()=>expect(within(queue).getByRole('button',{name:'Unstar one.png'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(within(queue).getByRole('button',{name:'Unstar one.png'}));
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_media_unstar',{path:'/out/starred/one.png'}));
 expect(within(queue).getByRole('button',{name:'Star one.png'}).closest('.wb-queue-output')?.querySelector('img')?.getAttribute('src')).toContain('/out/one.png');
 expect(Array.from(queue.querySelectorAll('.wb-queue-select')).map(button=>button.getAttribute('aria-current'))).toEqual(selectedBefore);
 expect(document.querySelector('.wb-shown')).toBe(result);
 expect(outputs[0].subfolder).toBe('');expect(JSON.stringify(intent)).toBe(snapshot);
 expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
});

test.each([['de','Abgebrochen'],['en','Cancelled'],['fr','Annulé'],['ja','キャンセル済み'],['tr','İptal edildi'],['zh','已取消']])('cancelled job is a terminal localized result without output actions (%s)',async(locale,label)=>{
 localStorage.setItem('deas.locale.v1',locale);
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'cancelled-job',baseUrl:evidence.baseUrl,promptId:'p',state:'cancelled',outputs:[],error:null,generationReady:false});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/out'}}/></I18nProvider>);
 const run=document.querySelector<HTMLButtonElement>('[data-run]')!;
 await waitFor(()=>expect(run.disabled).toBe(false));fireEvent.click(run);
 const tile=await waitFor(()=>{const tile=document.querySelector<HTMLButtonElement>('.wb-queue-select');expect(tile).not.toBeNull();return tile!;});
 expect(tile.textContent).toContain(label);expect(tile.querySelector('.wb-thumb')?.textContent).toBe('×');
 expect(tile.querySelector('[data-generation-duration]')).toBeNull();
 expect(document.querySelector('.wb-queue-output')?.querySelectorAll('button')).toHaveLength(1);
 await waitFor(()=>expect(run.disabled).toBe(false));expect(pollWorkbench).not.toHaveBeenCalled();
 expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
});

test.each([
 ...['de','en','fr','ja','tr','zh'].map(locale=>({locale,generationMs:13310,state:'succeeded' as const,filename:'result.png'})),
 {locale:'en',generationMs:0,state:'succeeded' as const,filename:'zero.png'},
 {locale:'en',generationMs:null,state:'succeeded' as const,filename:'unknown.png'},
 {locale:'en',generationMs:13310,state:'failed' as const,filename:'failed.png'},
 {locale:'en',generationMs:13310,state:'succeeded' as const,filename:'clip.webm'},
])('queue and selected details use actual timing: $locale / $filename',async({locale,generationMs,state,filename})=>{
 localStorage.setItem('deas.locale.v1',locale);
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(invoke).mockImplementation(async command=>command==='native_media_star_status'?{starred:false,unavailable:null}:{path:'/out',parent:'/',entries:[],truncated:false});
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'timing-job',baseUrl:evidence.baseUrl,promptId:'p',state,outputRoot:'/out',outputs:[{filename,subfolder:'',type:'output'}],error:null,generationReady:false,metadata:{startedAt:1700000000000,generationMs}});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/out'}}/></I18nProvider>);
 const run=document.querySelector<HTMLButtonElement>('[data-run]')!;
 await waitFor(()=>expect(run.disabled).toBe(false));fireEvent.click(run);
 await waitFor(()=>expect(document.querySelector('.wb-queue-select')).not.toBeNull());
 const tile=document.querySelector<HTMLButtonElement>('.wb-queue-select')!;
 const header=document.querySelector('.wb-job-line')!;
 expect(tile.textContent).toContain(filename);expect(header.textContent).toContain('timing-j');
 const expected=generationMs!==null&&state==='succeeded'?new Intl.NumberFormat(locale,{style:'unit',unit:'second',unitDisplay:'short',maximumFractionDigits:2}).format(generationMs/1000):null;
 if(expected){expect(tile.textContent).toContain(expected);expect(header.textContent).toContain(expected);}
 else {expect(tile.querySelector('[data-generation-duration]')).toBeNull();expect(header.querySelector('[data-generation-duration]')).toBeNull();}
 fireEvent.click(tile);expect(tile.getAttribute('aria-current')).toBe('true');
 expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
});

test('missing remembered model and LoRAs are explicit blockers, never silently substituted',async()=>{
 localStorage.setItem('deas.pref.studio.run.checkpoint','"missing.safetensors"');
 localStorage.setItem('deas.pref.studio.run.loras',JSON.stringify([{name:'missing-lora.safetensors',strength:1}]));
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'other.safetensors',path:'/m/other',size:1,info:null}]);
 render(<I18nProvider><Workbench endpoint="" roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Image → Image').disabled).toBe(false));
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
 expect((screen.getByRole('combobox',{name:'Checkpoint'}) as HTMLSelectElement).value).toBe('missing.safetensors');expect(screen.getByText(/missing-lora.safetensors/)).toBeTruthy();
 expect(screen.getByRole('alert').closest('.wb-stage')).toBeTruthy();
 expect(screen.getByRole('alert').classList.contains('wb-stage-warning')).toBe(true);
 dock('Models');fireEvent.click(screen.getByRole('button',{name:'other.safetensors'}));
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test('dependency warnings follow corrected selections after a blocked post-process and reopen',async()=>{
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 const m=(category:string,name:string,family?:string)=>({category,name,family,path:'/m/'+name,size:1,info:null});
 vi.mocked(scanModels).mockResolvedValue([m('diffusion_models','k2.safetensors','krea-2'),m('text_encoders','qwen3vl_4b.safetensors'),m('vae','qwen_image_vae.safetensors')]);
 vi.mocked(invoke).mockImplementation(async()=>({path:'/out',parent:'/',entries:[{name:'source.png',path:'/out/source.png',kind:'file',symlink:false,media:'image',size:1,modified:1}],truncated:false}));
 for(const [key,value] of Object.entries({checkpoint:'missing.safetensors',vae:'minimax_h3_video_vae.safetensors'}))localStorage.setItem('deas.pref.studio.run.'+key,JSON.stringify(value));
 const mount=()=>render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/out'}}/></I18nProvider>);
 const view=mount();
 await waitFor(()=>expect(pipe('Image → Image').disabled).toBe(false));
 expect(screen.getByRole('alert').textContent).toContain('missing.safetensors');
 dock('Models');fireEvent.click(screen.getByRole('button',{name:'k2.safetensors'}));
 expect(screen.getByRole('alert').textContent).not.toContain('missing.safetensors');
 expect(screen.getByRole('alert').textContent).toContain('minimax_h3_video_vae.safetensors');
 dock('Media');fireEvent.click(await screen.findByRole('button',{name:'source.png'}));adopt();
 const img=screen.getByRole('img',{name:'source.png'});
 Object.defineProperties(img,{naturalWidth:{value:1024},naturalHeight:{value:1024}});fireEvent.load(img);
 fireEvent.click(screen.getByRole('button',{name:'Image to Image'}));
 fireEvent.click(screen.getByRole('button',{name:'Refine'}));
 expect(runWorkbenchJob).not.toHaveBeenCalled();
 fireEvent.change(screen.getByRole('combobox',{name:'VAE'}),{target:{value:'qwen_image_vae.safetensors'}});
 expect(screen.queryByRole('alert')).toBeNull();
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false);
 view.unmount();
 localStorage.setItem('deas.pref.studio.run.vae','"minimax_h3_video_vae.safetensors"');
 const reopened=mount();
 await waitFor(()=>expect(screen.getByRole('combobox',{name:'VAE'})).toBeTruthy());
 expect(screen.getByRole('alert').textContent).toContain('minimax_h3_video_vae.safetensors');
 expect(screen.getByRole('alert').textContent).not.toContain('missing.safetensors');
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
 fireEvent.change(screen.getByRole('combobox',{name:'VAE'}),{target:{value:'qwen_image_vae.safetensors'}});
 reopened.unmount();mount();
 await waitFor(()=>expect(screen.getByRole('combobox',{name:'VAE'})).toBeTruthy());
 expect(screen.queryByRole('alert')).toBeNull();
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('pending model refresh validates the latest selection, not the selection at scan start',async()=>{
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 const models=[{category:'diffusion_models',name:'k2.safetensors',family:'krea-2',path:'/m/k2',size:1,info:null},{category:'text_encoders',name:'qwen3vl_4b.safetensors',path:'/m/encoder',size:1,info:null},{category:'vae',name:'qwen_image_vae.safetensors',path:'/m/vae',size:1,info:null}];
 vi.mocked(scanModels).mockResolvedValue(models);
 localStorage.setItem('deas.pref.studio.run.vae','"minimax_h3_video_vae.safetensors"');
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await screen.findByRole('combobox',{name:'VAE'});
 let finish!:(value:typeof models)=>void;
 vi.mocked(scanModels).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));
 dock('Models');fireEvent.click(document.querySelector('[data-run-load]')!);
 fireEvent.change(screen.getByRole('combobox',{name:'VAE'}),{target:{value:'qwen_image_vae.safetensors'}});
 await act(async()=>finish(models));
 expect(screen.queryByRole('alert')).toBeNull();
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false);
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('selection changes do not erase an unrelated preparation error',async()=>{
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'base.safetensors',path:'/m/base',size:1,info:null}]);
 vi.mocked(runWorkbenchJob).mockRejectedValueOnce(new Error('Backend connection failed'));
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await screen.findByText('Backend connection failed');
 fireEvent.change(screen.getByRole('combobox',{name:'Pipeline'}),{target:{value:'i2i'}});
 expect(screen.getByRole('alert').textContent).toContain('Backend connection failed');
});

test('dock clicks preview only and preserve the adopted editor until explicit replacement',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(invoke).mockImplementation(async()=>({path:'/out',parent:'/',entries:['one.png','two.png'].map(name=>({name,path:'/out/'+name,kind:'file',media:'image',symlink:false,size:1,modified:1})),truncated:false}));
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/out'}}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Image → Image').disabled).toBe(false));choosePipe('Image → Image');
 fireEvent.click(await screen.findByRole('button',{name:'one.png'}));
 expect(document.querySelector('[data-browsed-preview]')).not.toBeNull();
 expect(document.querySelector('[data-editor-viewport]')).toBeNull();
 const bounds=vi.spyOn(HTMLElement.prototype,'getBoundingClientRect').mockReturnValue({left:0,top:0,width:800,height:600} as DOMRect);
 const previewImage=document.querySelector<HTMLImageElement>('[data-browsed-preview] img')!;
 clickMedia(previewImage,400,10);expect(screen.queryByRole('dialog')).toBeNull();
 fireEvent.click(screen.getByRole('region',{name:'Preview'}));expect(screen.queryByRole('dialog')).toBeNull();
 clickMedia(previewImage);
 const dialog=screen.getByRole('dialog',{name:'one.png'}),full=dialog.querySelector('img')!;
 Object.defineProperties(full,{naturalWidth:{value:1024},naturalHeight:{value:640}});fireEvent.load(full);
 const before=full.style.transform;fireEvent.wheel(dialog.querySelector('.wb-viewer-box')!,{deltaY:-200,clientX:200,clientY:200});
 expect(full.style.transform).not.toBe(before);
 fireEvent.keyDown(window,{key:'Escape'});expect(screen.queryByRole('dialog')).toBeNull();bounds.mockRestore();
 expect(document.querySelector('[data-browsed-preview]')).not.toBeNull();
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
 expect(uploadInput).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();
 adopt();const img=screen.getByRole('img',{name:'one.png'});
 Object.defineProperties(img,{naturalWidth:{value:1024},naturalHeight:{value:640}});fireEvent.load(img);
 const editor=document.querySelector('[data-editor-viewport]');expect(editor).not.toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'two.png'}));
 expect(document.querySelector('[data-editor-viewport]')).toBe(editor);
 expect(editor?.closest('[hidden]')).not.toBeNull();
 fireEvent.click(document.querySelector('[data-preview-close]')!);
 expect(document.querySelector('[data-editor-viewport]')).toBe(editor);
 expect(editor?.closest('[hidden]')).toBeNull();
});
test('an explicit Run replaces a browsed preview with the accepted job',async()=>{
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(invoke).mockImplementation(async()=>({path:'/out',parent:'/',entries:[{name:'browse.png',path:'/out/browse.png',kind:'file',media:'image',symlink:false,size:1,modified:1}],truncated:false}));
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'browse-job',baseUrl:evidence.baseUrl,promptId:'p',state:'succeeded',outputs:[],error:null,generationReady:false});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/out'}}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(await screen.findByRole('button',{name:'browse.png'}));
 expect(document.querySelector('[data-browsed-preview]')).not.toBeNull();
 fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'immutable submission'}});
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(document.querySelector('.wb-shown')).not.toBeNull());
 const submitted=vi.mocked(runWorkbenchJob).mock.calls[0][1];
 const confirm=vi.spyOn(window,'confirm').mockReturnValue(true);
 act(()=>window.dispatchEvent(new Event('deas:request-studio-defaults')));
 expect(submitted.positivePrompt).toBe('immutable submission');expect(document.querySelector('.wb-shown')).not.toBeNull();
 expect(runWorkbenchJob).toHaveBeenCalledTimes(1);confirm.mockRestore();
 expect(document.querySelector('[data-browsed-preview]')).toBeNull();
 expect(document.querySelector('.wb-current-preview')?.hasAttribute('hidden')).toBe(false);
});
const sessionModels=['folder/a.safetensors','other/a.safetensors','c.safetensors'].map(name=>({category:'checkpoints',name,family:'sdxl',path:'/m/'+name,size:1,info:null}));
const sessionModel=(name:string)=>fireEvent.change(document.querySelector('[data-model-picker]')!,{target:{value:name}});
const sessionSettings=(settings:Record<string,unknown>)=>act(()=>{expect(applyStudioSettings(Object.fromEntries(Object.entries(settings).map(([key,value])=>['studio.run.'+key,value])))).toBe(true);});
test('session drafts restore last edited controls and prompts per full model path and pipeline without Run',async()=>{
 vi.mocked(scanModels).mockResolvedValue(sessionModels);vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-model-picker]')?.textContent).toContain('folder/a.safetensors'));
 sessionModel('folder/a.safetensors');
 const a={positive:'A edited, never run',negative:'A negative',steps:37,cfg:4.5,seed:123,seedMode:'fixed',loras:[{name:'style.safetensors',strength:0.35}],options:{...defaultOptions,outputNaming:'a/%seed%'},post:true,upscale:2,textEncoder:'a-encoder',vae:'a-vae',componentOverrides:{textEncoders:true,vaes:true,audioVaes:false,textProjections:false}};
 sessionSettings(a);sessionModel('other/a.safetensors');
 expect((screen.getByLabelText('Steps') as HTMLInputElement).value).toBe('28'); // first visit retains family defaults
 sessionSettings({positive:'B draft',negative:'B negative',steps:41,seed:456,post:false});
 sessionModel('folder/a.safetensors');
 // The checkpoint's settings return; the prompts on screen stay.
 expect(captureStudioSettings()).toMatchObject(Object.fromEntries(Object.entries({...a,positive:'B draft',negative:'B negative'}).map(([key,value])=>['studio.run.'+key,value])));
 choosePipe('Image → Image');sessionSettings({positive:'i2i draft',steps:53});
 sessionModel('other/a.safetensors');sessionSettings({positive:'i2i B',steps:55});
 choosePipe('Text → Image');
 expect(document.querySelector('[data-model-picker]')).toHaveProperty('value','folder/a.safetensors');
 expect(screen.getByLabelText('Prompt')).toHaveProperty('value','B draft');
 expect(screen.getByLabelText('Steps')).toHaveProperty('value','37');
 choosePipe('Image → Image');
 expect(document.querySelector('[data-model-picker]')).toHaveProperty('value','other/a.safetensors');
 expect(screen.getByLabelText('Prompt')).toHaveProperty('value','i2i B');
 sessionModel('folder/a.safetensors');expect(screen.getByLabelText('Prompt')).toHaveProperty('value','i2i B');
 sessionModel('other/a.safetensors');choosePipe('Text → Image');sessionModel('other/a.safetensors');
 expect(screen.getByLabelText('Prompt')).toHaveProperty('value','B draft');
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
});
test('session draft cache resets on a fresh Workbench session without adding storage records',async()=>{
 vi.mocked(scanModels).mockResolvedValue(sessionModels);vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 const mount=()=>render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 let view=mount();await waitFor(()=>expect(document.querySelector('[data-model-picker]')?.textContent).toContain('folder/a.safetensors'));
 sessionSettings({positive:'old A',steps:39});sessionModel('other/a.safetensors');sessionSettings({positive:'last global',steps:42});
 const keys=Object.keys(localStorage);expect(keys.some(k=>/cache|draft|session/i.test(k))).toBe(false);expect(sessionStorage.length).toBe(0);
 view.unmount();view=mount();await waitFor(()=>expect(document.querySelector('[data-model-picker]')?.textContent).toContain('folder/a.safetensors'));
 sessionModel('folder/a.safetensors');expect(screen.getByLabelText('Steps')).toHaveProperty('value','28');
 expect(screen.getByLabelText('Prompt')).toHaveProperty('value','last global');expect(runWorkbenchJob).not.toHaveBeenCalled();view.unmount();
});
test('session drafts restore prompt image ordering, removals and input sources rather than another models media',async()=>{
 vi.mocked(scanModels).mockResolvedValue(sessionModels);vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,visionModels:['clip_vision_g.safetensors']}});
 vi.mocked(invoke).mockResolvedValue({path:'/out',parent:'/',entries:['one.png','two.png'].map(name=>({name,path:'/out/'+name,kind:'file',symlink:false,media:'image',size:1,modified:1})),truncated:false});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/out'}}/></I18nProvider>);
 await screen.findByRole('button',{name:'one.png'});sessionModel('folder/a.safetensors');
 promptDrop('image','/out/one.png');promptDrop('image','/out/two.png');reorderReferences(0,1);
 fireEvent.click(screen.getByRole('button',{name:'one.png'}));adopt();
 const img=document.querySelector<HTMLImageElement>('.wb-current-preview img')!;Object.defineProperties(img,{naturalWidth:{value:1024},naturalHeight:{value:640}});fireEvent.load(img);
 fireEvent.click(screen.getByRole('button',{name:'Image to Image'}));
 fireEvent.keyDown(document.querySelector('[data-layer="base"]')!,{key:'ArrowLeft'});
 let finish=()=>{};vi.stubGlobal('Image',class{naturalWidth=64;naturalHeight=64;onload=()=>{};onerror=()=>{};set src(_:string){finish=()=>this.onload();}});
 dropTile('two.png');act(()=>finish());dropTile('two.png');
 sessionModel('other/a.safetensors');act(()=>finish());expect(document.querySelectorAll('[data-layer]')).toHaveLength(2);
 fireEvent.click(document.querySelector('[data-reference-remove]')!);
 fireEvent.click(screen.getByRole('button',{name:'two.png'}));adopt();
 const second=document.querySelector<HTMLImageElement>('.wb-current-preview img')!;Object.defineProperties(second,{naturalWidth:{value:1024},naturalHeight:{value:1024}});fireEvent.load(second);
 sessionModel('folder/a.safetensors');
 expect(Array.from(document.querySelectorAll<HTMLImageElement>('[data-reference-controls] img')).map(img=>img.alt)).toEqual(['two.png','one.png']);
 expect(document.querySelector('[data-layer="base"] img')?.getAttribute('src')).toContain('/out/one.png');
 expect(document.querySelector<HTMLElement>('[data-layer="base"]')?.style.left).toBe('-8px');expect(document.querySelectorAll('[data-layer]')).toHaveLength(2);
 sessionModel('other/a.safetensors');
 expect(Array.from(document.querySelectorAll<HTMLImageElement>('[data-reference-controls] img')).map(img=>img.alt)).toEqual(['one.png']);
 expect(document.querySelector('[data-layer="base"] img')?.getAttribute('src')).toContain('/out/two.png');
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
});
test('session video drafts restore video/audio references and trims without uploading',async()=>{
 const files=['h3-a.safetensors','h3-b.safetensors'].map(name=>({category:'diffusion_models',name,family:'minimax-h3',path:'/m/'+name,size:1,info:null}));
 vi.mocked(scanModels).mockResolvedValue(files);vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(invoke).mockResolvedValue({path:'/out',parent:'/',entries:[],truncated:false});
 vi.spyOn(HTMLMediaElement.prototype,'pause').mockImplementation(()=>{});
 const props={endpoint:evidence.baseUrl,roots:{models:'/m',input:'/input'}};
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Multi-reference → Video').disabled).toBe(false));choosePipe('Multi-reference → Video');
 const add=(name:string,media:string)=>fireEvent.drop(document.querySelector('[data-open-references]')!,{dataTransfer:{types:['application/x-deas-media'],getData:()=>JSON.stringify({name,media,path:'/input/'+name,src:'asset://localhost/'+name})}});
 add('clip.mp4','video');add('voice.wav','audio');add('picture.png','image');
 fireEvent.click(document.querySelector('[data-open-references]')!);fireEvent.click(screen.getByRole('button',{name:'Edit clip.mp4'}));
 fireEvent.change(screen.getByLabelText('Use of clip.mp4'),{target:{value:'audio'}});
 fireEvent.change(screen.getByLabelText('Start of clip.mp4'),{target:{value:'2'}});
 fireEvent.change(screen.getByLabelText('End of clip.mp4'),{target:{value:'6'}});
 fireEvent.click(within(screen.getByRole('dialog',{name:'Edit clip.mp4'})).getByRole('button',{name:'Close'}));dock('Media');
 sessionSettings({positive:'video A',video:{seconds:7,fps:24,passes:1,upscaler:'',shiftHigh:5,shiftLow:5,refinerStep:2,shiftVideo:11,shiftAudio:4,audioVae:'a-vae',textProjection:''}});
 sessionModel('h3-b.safetensors');sessionSettings({positive:'video B'});
 fireEvent.click(document.querySelector('[data-open-references]')!);fireEvent.click(screen.getByRole('button',{name:'Remove reference clip.mp4'}));dock('Media');
 sessionModel('h3-a.safetensors');
 expect(document.querySelector('[data-open-references]')?.textContent).toContain('3');expect(screen.getByLabelText('Prompt')).toHaveProperty('value','video B');
 fireEvent.click(document.querySelector('[data-open-references]')!);fireEvent.click(screen.getByRole('button',{name:'Edit clip.mp4'}));
 expect(screen.getByLabelText('Use of clip.mp4')).toHaveProperty('value','audio');expect(screen.getByLabelText('Start of clip.mp4')).toHaveProperty('value','2');expect(screen.getByLabelText('End of clip.mp4')).toHaveProperty('value','6');
 fireEvent.click(within(screen.getByRole('dialog',{name:'Edit clip.mp4'})).getByRole('button',{name:'Close'}));dock('Media');
 choosePipe('Text → Video');sessionSettings({positive:'text video'});choosePipe('Multi-reference → Video');
 expect(screen.getByLabelText('Prompt')).toHaveProperty('value','video B');sessionModel('h3-b.safetensors');expect(document.querySelector('[data-open-references]')?.textContent).toContain('2');
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();view.unmount();
});

test('session cache is environment scoped and never substitutes a disappeared exact model path',async()=>{
 vi.mocked(scanModels).mockResolvedValue(sessionModels);vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 const ui=(root:string)=><I18nProvider><Workbench endpoint={evidence.baseUrl} root={root} roots={{models:root+'/models'}}/></I18nProvider>;
 const view=render(ui('/first'));await waitFor(()=>expect(document.querySelector('[data-model-picker]')?.textContent).toContain('folder/a.safetensors'));
 sessionModel('folder/a.safetensors');sessionSettings({positive:'first environment A'});sessionModel('other/a.safetensors');
 const second=sessionModels.map(m=>({...m,path:'/second/'+m.name}));vi.mocked(scanModels).mockResolvedValue(second);view.rerender(ui('/second'));
 await waitFor(()=>expect(scanModels).toHaveBeenCalledWith('/second/models',[]));
 await waitFor(()=>expect(screen.getByRole('alert').textContent).toContain('other/a.safetensors'));
 sessionModel('folder/a.safetensors');sessionSettings({positive:'second environment A'});sessionModel('other/a.safetensors');sessionModel('folder/a.safetensors');
 expect(screen.getByLabelText('Prompt')).toHaveProperty('value','second environment A');
 vi.mocked(scanModels).mockResolvedValue(sessionModels);view.rerender(ui('/first'));
 await waitFor(()=>expect(scanModels).toHaveBeenCalledWith('/first/models',[]));
 await waitFor(()=>expect(document.querySelector('[data-model-picker]')).toHaveProperty('value','other/a.safetensors'));
 sessionModel('folder/a.safetensors');expect(screen.getByLabelText('Prompt')).toHaveProperty('value','first environment A');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test.each([false,true])('environment transition saves the active last edit under its outgoing scope (same paths=%s)',async samePaths=>{
 vi.mocked(scanModels).mockResolvedValue(sessionModels);vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 const ui=(root:string)=><I18nProvider><Workbench endpoint={evidence.baseUrl} root={root} roots={{models:root+'/models'}}/></I18nProvider>;
 const view=render(ui('/first'));await waitFor(()=>expect(document.querySelector('[data-model-picker]')?.textContent).toContain('folder/a.safetensors'));
 sessionModel('folder/a.safetensors');sessionSettings({positive:'first original',steps:31});
 sessionModel('other/a.safetensors');sessionModel('folder/a.safetensors');sessionSettings({positive:'first last unsaved edit',steps:47});
 vi.mocked(scanModels).mockResolvedValue(samePaths?sessionModels:sessionModels.map(m=>({...m,path:'/second/'+m.name})));
 view.rerender(ui('/second'));await waitFor(()=>expect(scanModels).toHaveBeenLastCalledWith('/second/models',[]));
 await act(async()=>{});sessionModel('folder/a.safetensors');sessionSettings({positive:'second own draft',steps:52});
 sessionModel('other/a.safetensors');sessionModel('folder/a.safetensors');expect(screen.getByLabelText('Prompt')).toHaveProperty('value','second own draft');
 vi.mocked(scanModels).mockResolvedValue(sessionModels);view.rerender(ui('/first'));
 await waitFor(()=>expect(screen.getByLabelText('Prompt')).toHaveProperty('value','first last unsaved edit'));
 expect(screen.getByLabelText('Steps')).toHaveProperty('value','47');
 view.rerender(ui('/second'));await waitFor(()=>expect(screen.getByLabelText('Prompt')).toHaveProperty('value','second own draft'));
 expect(screen.getByLabelText('Steps')).toHaveProperty('value','52');expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test.each([['anima','Anima'],['krea-2','Krea 2']])('tiled refine is refused with a reason on %s, whose latents are not flat',async(familyId)=>{
 localStorage.setItem('deas.pref.studio.run.post.tiled','true');
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'DaSiWaTiledDiffusion']}});
 vi.mocked(scanModels).mockResolvedValue([{category:'diffusion_models',name:'a.safetensors',family:familyId,path:'/m/a',size:1,info:null}]);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 fireEvent.click(await screen.findByRole('tab',{name:/Post-Processing/}));
 const box=await screen.findByRole('checkbox',{name:'Tiled refine (DaSiWa Tiled Diffusion)'});
 await waitFor(()=>expect(box.hasAttribute('disabled')).toBe(true));
 expect(box).toHaveProperty('checked',false);
 expect(screen.getByText(/cannot run on it/)).toBeTruthy();
});
test('session cache distinguishes same names in separate categories and absolute model roots',async()=>{
 const files=[{...sessionModels[0],name:'same.safetensors',path:'/one/same.safetensors'},{...sessionModels[0],name:'same.safetensors',category:'diffusion_models',family:'qwen-image-2.1',path:'/two/same.safetensors'},{...sessionModels[0],name:'same.safetensors',path:'/three/same.safetensors'}];
 vi.mocked(scanModels).mockResolvedValue(files);vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelectorAll('[data-model-picker] option')).toHaveLength(3));
 const ids=files.map(m=>JSON.stringify([m.category,m.path,m.name]));
 sessionSettings({steps:31});sessionModel(ids[1]);sessionSettings({steps:32});sessionModel(ids[2]);sessionSettings({steps:33});
 sessionModel(ids[0]);expect(screen.getByLabelText('Steps')).toHaveProperty('value','31');
 sessionModel(ids[1]);expect(screen.getByLabelText('Steps')).toHaveProperty('value','32');
 sessionModel(ids[2]);expect(screen.getByLabelText('Steps')).toHaveProperty('value','33');
});

const projectFixture={id:'project-1',root:'/projects/one',revision:1,draft:{positivePrompt:'saved project prompt',negativePrompt:'saved negative',checkpoint:'missing.safetensors',width:1024,height:1024,steps:24,cfg:6,sampler:'euler',scheduler:'normal',seed:42,denoise:0.5,pipeline:'i2i' as const},input:null};
test('opening a project restores its draft without substituting a missing checkpoint or overwriting global defaults',async()=>{
 localStorage.setItem('deas.pref.studio.run.positive',JSON.stringify('global prompt'));
 localStorage.setItem('deas.pref.studio.run.checkpoint',JSON.stringify('a.safetensors'));
 vi.mocked(scanModels).mockResolvedValue([]);
 vi.mocked(invoke).mockImplementation(async c=>c==='native_project_open'?projectFixture:c==='native_project_close'?null:{entries:[],truncated:false});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(scanModels).toHaveBeenCalled());
 fireEvent.click(screen.getByRole('button',{name:'Open project'}));
 await waitFor(()=>expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('saved project prompt'));
 expect((screen.getByRole('spinbutton',{name:'Seed'}) as HTMLInputElement).value).toBe('42');
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
 expect(localStorage.getItem('deas.pref.studio.run.positive')).toBe(JSON.stringify('global prompt'));
 await waitFor(()=>expect(screen.getByRole('button',{name:'Close project'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Close project'}));
 await waitFor(()=>expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('global prompt'));
});
test('project defaults reset stays project-local and closing restores global last-used controls',async()=>{
 const global={positive:'global draft',checkpoint:'base.safetensors',steps:33,cfg:0,seed:0,seedMode:'fixed',options:{...defaultOptions,outputNaming:'global/%seed%'},upscale:2,refineDenoise:0.7,textEncoder:'global-encoder.safetensors',vae:'global-vae.safetensors',componentOverrides:{textEncoders:true,vaes:true,audioVaes:false,textProjections:false}};
 for(const [key,value] of Object.entries(global))localStorage.setItem('deas.pref.studio.run.'+key,JSON.stringify(value));
 localStorage.setItem('deas.pref.studio.layout.settingsWidth','304');
 localStorage.setItem('deas.pref.backend.autostart','false');
 const saved={...projectFixture,draft:{...projectFixture.draft,checkpoint:'base.safetensors'}};
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'base.safetensors',family:'sdxl',path:'/m/base',size:1,info:null}]);
 vi.mocked(invoke).mockImplementation(async(command,args)=>{
  if(command==='native_project_open')return saved;
  if(command==='native_project_save')return {...saved,revision:2,draft:(args as {draft:typeof saved.draft}).draft};
  if(command==='native_project_close')return null;
  return {entries:[],truncated:false};
 });
 const confirm=vi.spyOn(window,'confirm').mockReturnValue(false);
 const mount=()=>render(<I18nProvider><Workbench endpoint="" roots={{models:'/m'}}/></I18nProvider>);
 const view=mount();
 await waitFor(()=>expect(pipe('Image → Image').disabled).toBe(false));
 const stored=Object.fromEntries(Object.entries(localStorage).filter(([key])=>key.startsWith('deas.pref.studio.run.')));
 fireEvent.click(screen.getByRole('button',{name:'Open project'}));
 await waitFor(()=>expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('saved project prompt'));
 act(()=>window.dispatchEvent(new Event('deas:request-studio-defaults')));
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('saved project prompt');
 expect(invoke).not.toHaveBeenCalledWith('native_project_save',expect.anything());
 confirm.mockReturnValue(true);act(()=>window.dispatchEvent(new Event('deas:request-studio-defaults')));
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('');
 expect((screen.getByLabelText('Steps') as HTMLInputElement).value).toBe('28');
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_project_save',expect.objectContaining({projectId:saved.id,draft:expect.objectContaining({positivePrompt:'',steps:28,pipeline:'t2i'})})));
 expect(Object.fromEntries(Object.entries(localStorage).filter(([key])=>key.startsWith('deas.pref.studio.run.')))).toEqual(stored);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Close project'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Close project'}));
 await waitFor(()=>expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('global draft'));
 view.unmount();mount();
 await waitFor(()=>expect(pipe('Image → Image').disabled).toBe(false));
 for(const [name,value] of [['Steps','33'],['CFG','0'],['Seed','0']])expect((screen.getByLabelText(name) as HTMLInputElement).value).toBe(value);
 dock('Options');expect((screen.getByRole('textbox',{name:'Output naming'}) as HTMLInputElement).value).toBe(global.options.outputNaming);
 dock('Post-Processing');expect((screen.getByLabelText('Upscale factor') as HTMLInputElement).value).toBe('2');
 expect(screen.getByRole('separator',{name:'Resize Settings'}).getAttribute('aria-valuenow')).toBe('304');
 expect(localStorage.getItem('deas.pref.backend.autostart')).toBe('false');
 for(const key of ['textEncoder','vae','componentOverrides'] as const)expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.'+key)!)).toEqual(global[key]);
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
 confirm.mockRestore();
});
test('pending image upload is dropped on unmount before native admission',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a.safetensors',size:1,info:null}]);
 const saved={...projectFixture,draft:{...projectFixture.draft,checkpoint:'a.safetensors',pipeline:'i2i'},input:{id:'input-1',path:'/projects/one/assets/input.png',sha256:'a'.repeat(64),width:1024,height:1024}};
 vi.mocked(invoke).mockImplementation(async c=>c==='native_project_open'?saved:{entries:[],truncated:false});
 let release!:(name:string)=>void;vi.mocked(uploadInput).mockReturnValueOnce(new Promise(yes=>{release=yes;}));
 const props={endpoint:evidence.baseUrl,roots:{models:'/m',input:'/env/in'}};
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Open project'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Open project'}));
 await waitFor(()=>expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('saved project prompt'));
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));await waitFor(()=>expect(uploadInput).toHaveBeenCalledTimes(1));
 view.unmount();localStorage.setItem('deas.pref.studio.run.pipeline','"t2i"');
 render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 await waitFor(()=>expect(readCurrentWorkbench).toHaveBeenCalledTimes(2));
 await act(async()=>release('deas-inputs/late.png'));
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(document.querySelectorAll('[data-job-id]')).toHaveLength(0);
});
test('a reopened managed PNG becomes an image source and uploads only on explicit Run',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a.safetensors',size:1,info:null}]);
 const saved={...projectFixture,draft:{...projectFixture.draft,checkpoint:'a.safetensors',pipeline:'i2i'},input:{id:'input-1',path:'/projects/one/assets/input.png',sha256:'a'.repeat(64),width:1024,height:1024}};
 vi.mocked(invoke).mockImplementation(async c=>c==='native_project_open'?saved:{entries:[],truncated:false});
 vi.mocked(uploadInput).mockResolvedValue('deas-inputs/project.png');
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'project-job',baseUrl:evidence.baseUrl,promptId:'p',state:'succeeded',outputs:[],error:null,generationReady:false});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',input:'/env/in'}}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Open project'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Open project'}));
 await waitFor(()=>expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('saved project prompt'));
 expect(uploadInput).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(uploadInput).toHaveBeenCalledWith(evidence.baseUrl,'/projects/one/assets/input.png');
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({inputImage:'deas-inputs/project.png',seed:42,denoise:0.5});
});
test('wildcard dock and hammer keep base text, snapshot active rules per run and allow disabling without clearing',async()=>{
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,checkpoints:['m.safetensors'],nodes:[...evidence.allowlist.nodes,'DaSiWa_WildcardPresetPromptBuilder']}});
 localStorage.setItem('deas.pref.studio.run.wildcard',JSON.stringify({style:'Booru',seed:7,reroll:3,token_budget:200,auto_reroll:true,selection_state:{'Art/Light/wildcards':{enabled:true,weight:1.2}}}));
 vi.mocked(invoke).mockImplementation(async c=>c==='native_wildcard_library'?JSON.stringify({categories:{Art:[{subject:'Light',booru_wildcards:['{red|blue} light']}]}}):{entries:[],truncated:false});
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'m.safetensors',path:'/m/checkpoints/m.safetensors',size:1,info:null}]);
 let submits=0;
 vi.mocked(runWorkbenchJob).mockImplementation(async()=>({id:`wild-job-${++submits}`,baseUrl:evidence.baseUrl,promptId:'p',state:'succeeded',outputs:[],error:null,generationReady:false,intent:vi.mocked(runWorkbenchJob).mock.calls.at(-1)![1]}));
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'manual base'}});
 fireEvent.click(screen.getByRole('button',{name:'Prompt Forge'}));
 fireEvent.click(screen.getByRole('button',{name:'Wildcards'}));
 expect(document.activeElement).toBe(screen.getByRole('tab',{name:'Wildcards'}));
 await screen.findByText('Art');
 fireEvent.click(screen.getByRole('checkbox',{name:'Active for generation'}));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 const first=vi.mocked(runWorkbenchJob).mock.calls[0][1];
 expect(first.positivePrompt).toBe('manual base');expect(first.wildcard?.auto_reroll).toBe(true);
 fireEvent.click(screen.getByRole('button',{name:'New picks'}));
 expect(first.wildcard?.reroll).toBe(3);
 fireEvent.change(screen.getByRole('spinbutton',{name:'Wildcard seed'}),{target:{value:'9'}});
 // Metadata is an overlay opened from the preview, not a permanent column.
 expect(screen.queryByRole('region',{name:'Generation metadata'})).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'Metadata'}));
 fireEvent.click(within(screen.getByRole('region',{name:'Generation metadata'})).getByRole('button',{name:'Use these settings'}));
 expect((screen.getByRole('spinbutton',{name:'Wildcard seed'}) as HTMLInputElement).value).toBe('7');
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Disable wildcards'}));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(2));
 expect(vi.mocked(runWorkbenchJob).mock.calls[1][1].wildcard).toBeUndefined();
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('manual base');
 fireEvent.click(screen.getByRole('checkbox',{name:'Active for generation'}));
 fireEvent.click(screen.getByRole('button',{name:'Use these settings'}));
 expect((screen.getByRole('checkbox',{name:'Active for generation'}) as HTMLInputElement).checked).toBe(false);
});
test('Options naming persists and resolves the submitted seed/model, rejecting unsafe templates before preparation',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'folder/My Model.safetensors',path:'/m/checkpoints/folder/My Model.safetensors',size:1,info:null}]);
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'naming-job',baseUrl:evidence.baseUrl,promptId:'p',state:'succeeded',outputs:[],error:null,generationReady:false});
 const view=render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 dock('Options');
 const field=screen.getByRole('textbox',{name:'Output naming'}) as HTMLInputElement;
 expect(field.value).toBe('%type%/%date_yyyy-mm-dd%/%time%-%seed%-%model%');
 fireEvent.change(screen.getByRole('spinbutton',{name:'Seed'}),{target:{value:'42'}});
 fireEvent.change(field,{target:{value:'custom/%type%/%seed%-%model%'}});
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1].outputPrefix).toBe('raw/custom/image/42-My-Model');
 fireEvent.change(field,{target:{value:'../%seed%'}});
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
 expect(field.getAttribute('aria-invalid')).toBe('true');
 view.unmount();
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 dock('Options');expect((screen.getByRole('textbox',{name:'Output naming'}) as HTMLInputElement).value).toBe('../%seed%');
});
test('seed controls restore submitted seeds, preserve mode on New and wrap exact integer steps',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'seed.safetensors',path:'/m/checkpoints/seed.safetensors',size:1,info:null}]);
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'seed-job',baseUrl:evidence.baseUrl,promptId:'p',state:'succeeded',outputs:[],error:null,generationReady:false});
 const view=render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(scanModels).toHaveBeenCalled());
 const input=()=>screen.getByRole('spinbutton',{name:'Seed'}) as HTMLInputElement;
 fireEvent.change(input(),{target:{value:'42'}});
 expect(screen.getByRole('button',{name:'Fixed',pressed:true})).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1].seed).toBe(42);
 expect(input().value).toBe('42');
 fireEvent.click(screen.getByRole('button',{name:'New random seed'}));
 expect(screen.getByRole('button',{name:'Fixed',pressed:true})).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'Use last seed'}));
 expect(input().value).toBe('42');
 fireEvent.change(input(),{target:{value:'0'}});
 fireEvent.click(screen.getByRole('button',{name:'Decrease seed'}));
 expect(input().value).toBe(String(Number.MAX_SAFE_INTEGER));
 fireEvent.click(screen.getByRole('button',{name:'Increase seed'}));
 expect(input().value).toBe('0');
 fireEvent.click(screen.getByRole('button',{name:'Use seed 42'}));
 expect(input().value).toBe('42');
 fireEvent.click(screen.getByRole('button',{name:'Copy seed 42'}));
 await screen.findByText('Copy failed; select the seed and copy it manually.'); // A project has its own independent status region.
 const writeText=vi.fn().mockResolvedValue(undefined);
 Object.defineProperty(navigator,'clipboard',{value:{writeText},configurable:true});
 fireEvent.click(screen.getByRole('button',{name:'Copy seed 42'}));
 await waitFor(()=>expect(writeText).toHaveBeenCalledWith('42'));
 expect(screen.getByText('Seed copied')).toBeTruthy();
 const promptBox=screen.getByLabelText('Prompt') as HTMLTextAreaElement;const before=promptBox.value;
 fireEvent.change(promptBox,{target:{value:'a red fox'}});
 fireEvent.click(screen.getByRole('button',{name:'Copy Prompt'}));
 await waitFor(()=>expect(writeText).toHaveBeenCalledWith('a red fox'));
 fireEvent.click(screen.getByRole('button',{name:'Clear Prompt'}));
 expect(promptBox.value).toBe('');
 expect((screen.getByRole('button',{name:'Clear Prompt'}) as HTMLButtonElement).disabled).toBe(true);
 fireEvent.change(promptBox,{target:{value:before}});
 fireEvent.click(screen.getByRole('button',{name:'Random'}));
 vi.mocked(runWorkbenchJob).mockResolvedValueOnce({id:'seed-job-2',baseUrl:evidence.baseUrl,promptId:'p2',state:'succeeded',outputs:[],error:null,generationReady:false});
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(2));
 expect(input().value).not.toBe('42');
 view.unmount();
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl}/></I18nProvider>);
 expect(screen.getByRole('button',{name:'Random',pressed:true})).toBeTruthy();
 expect(screen.getByRole('button',{name:'Use seed 42'})).toBeTruthy();
});
test('seed history caps at ten, deduplicates submitted values and ignores failed preparation',async()=>{
 localStorage.setItem('deas.pref.studio.run.recentSeeds',JSON.stringify([1,2,3,4,5,6,7,8,9,10]));
 localStorage.setItem('deas.pref.studio.run.seed','12');
 localStorage.setItem('deas.pref.studio.run.seedMode',JSON.stringify('fixed'));
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'seed.safetensors',path:'/m/checkpoints/seed.safetensors',size:1,info:null}]);
 vi.mocked(runWorkbenchJob).mockRejectedValueOnce(new Error('prepare failed'));
 vi.mocked(runWorkbenchJob).mockImplementation(async()=>({id:String(Math.random()),baseUrl:evidence.baseUrl,promptId:'p',state:'succeeded',outputs:[],error:null,generationReady:false}));
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await screen.findByText('prepare failed');
 const history=()=>JSON.parse(localStorage.getItem('deas.pref.studio.run.recentSeeds')!);
 expect(history()).toEqual([1,2,3,4,5,6,7,8,9,10]);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(history()).toEqual([12,1,2,3,4,5,6,7,8,9]));
 fireEvent.change(screen.getByRole('spinbutton',{name:'Seed'}),{target:{value:'3'}});
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(history()).toEqual([3,12,1,2,4,5,6,7,8,9]));
});

test('IPC fixture: Load models → pick LoRA → Run prepares and submits one intent, polls to success and previews the output file',async()=>{
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 const queued:JobStatus={id:'job-1234',baseUrl:evidence.baseUrl,promptId:'p',state:'queued',outputs:[],error:null,generationReady:false};
 vi.mocked(runWorkbenchJob).mockResolvedValue(queued);
 vi.mocked(pollWorkbench).mockResolvedValue(collection([{...queued,state:'succeeded',outputRoot:'/env/ComfyUI/output',outputs:[{filename:'DEAS_0001.png',subfolder:'',type:'output'}]}]));
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{output:'/env/ComfyUI/output'}}/></I18nProvider>);
 expect(screen.getByRole('complementary',{name:'Media'})).toBeTruthy(); // the media dock tab is the default
 expect(checkRecipePrerequisites).toHaveBeenCalledTimes(1); // sampler/scheduler lists are read once per endpoint; models still need the explicit Load
 dock('Models');fireEvent.click(screen.getByRole('button',{name:'Load models'}));
 fireEvent.click(await screen.findByRole('button',{name:'b.safetensors'}));
 expect(screen.getByRole('button',{name:'b.safetensors'}).getAttribute('aria-pressed')).toBe('true');
 dock('LoRAs');fireEvent.click(screen.getByRole('button',{name:'style.safetensors'}));
 fireEvent.change(screen.getByLabelText('Strength of style.safetensors'),{target:{value:'0.7'}});
 fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'a quiet forest'}});
 fireEvent.change(screen.getByLabelText('Width'),{target:{value:'768'}});
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 const [baseUrl,intent]=vi.mocked(runWorkbenchJob).mock.calls[0];
 expect(baseUrl).toBe('http://127.0.0.1:7801');
 expect(intent).toMatchObject({positivePrompt:'a quiet forest',checkpoint:'b.safetensors',width:768,height:1024,loras:[{name:'style.safetensors',strength:0.7}],outputPrefix:expect.stringMatching(new RegExp(`^raw/image/\\d{4}-\\d{2}-\\d{2}/\\d{2}-\\d{2}-\\d{2}-${intent.seed}-b$`))});
 expect(Object.isFrozen(intent)).toBe(true);
 expect(screen.getByRole('button',{name:/Queued/})).toBeTruthy();
 await waitFor(()=>expect((screen.getByRole('img',{name:'DEAS_0001.png'}) as HTMLImageElement).src).toBe('asset://localhost//env/ComfyUI/output/DEAS_0001.png'),{timeout:3000});
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false);
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.loras')!)).toEqual([{name:'style.safetensors',strength:0.7}]);
});

test('IPC fixture: with an environment the models folder is scanned on mount instead of probing the backend',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'sdxl/base.safetensors',path:'/m/checkpoints/sdxl/base.safetensors',size:1,info:null},{category:'loras',name:'grain.safetensors',path:'/m/loras/grain.safetensors',size:1,info:null},{category:'vae',name:'x.safetensors',path:'/m/vae/x.safetensors',size:1,info:null}]);
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m'}}/></I18nProvider>);
 dock('Models');
 expect((await screen.findByRole('button',{name:'sdxl/base.safetensors'})).getAttribute('aria-pressed')).toBe('true');
 dock('LoRAs');expect(screen.getByRole('button',{name:'grain.safetensors'}).getAttribute('aria-pressed')).toBe('false');
 expect(scanModels).toHaveBeenCalledExactlyOnceWith('/m',[]);
 expect(checkRecipePrerequisites).toHaveBeenCalledTimes(1); // only the sampler/scheduler lists, never the model list
});

test('Windows model tree filtering and short names preserve the exact selected identifier',async()=>{
 const name='sdxl\\anime\\My Model.safetensors';
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name,path:'C:\\models\\'+name,size:1,info:null}]);
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'C:\\models'}}/></I18nProvider>);
 dock('Models');
 const tree=await screen.findByRole('navigation',{name:'Folders'});
 fireEvent.click(within(tree).getByRole('button',{name:'anime'}));
 const tile=screen.getByRole('button',{name});
 expect(tile.querySelector('.wb-tile-name')!.textContent).toBe('My Model');
 fireEvent.click(tile);
 expect((screen.getByRole('combobox',{name:'Checkpoint'}) as HTMLSelectElement).value).toBe(name);
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.checkpoint')!)).toBe(name);
});

test('model and LoRA docks get a subfolder tree from the scanned names that filters the tiles per tab',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'sdxl/base.safetensors',path:'/m/checkpoints/sdxl/base.safetensors',size:1,info:null},{category:'checkpoints',name:'sd15/old.safetensors',path:'/m/checkpoints/sd15/old.safetensors',size:1,info:null},{category:'checkpoints',name:'sdxl/anime/pony.safetensors',path:'/m/checkpoints/sdxl/anime/pony.safetensors',size:1,info:null},{category:'loras',name:'grain.safetensors',path:'/m/loras/grain.safetensors',size:1,info:null}]);
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m'}}/></I18nProvider>);
 dock('Models');
 const tree=await screen.findByRole('navigation',{name:'Folders'});
 fireEvent.click(within(tree).getByRole('button',{name:'anime'}));
 expect(screen.getByRole('button',{name:'sdxl/anime/pony.safetensors'})).toBeTruthy();expect(screen.queryByRole('button',{name:'sdxl/base.safetensors'})).toBeNull();
 fireEvent.click(within(tree).getByRole('button',{name:'sdxl'}));
 expect(screen.getByRole('button',{name:'sdxl/base.safetensors'})).toBeTruthy();expect(screen.queryByRole('button',{name:'sd15/old.safetensors'})).toBeNull();
 dock('LoRAs');expect(screen.queryByRole('navigation',{name:'Folders'})).toBeNull();expect(screen.getByRole('button',{name:'grain.safetensors'})).toBeTruthy(); // no subfolders, no tree, unfiltered
 dock('Models');expect(within(screen.getByRole('navigation',{name:'Folders'})).getByRole('button',{name:'sdxl'}).getAttribute('aria-current')).toBe('location');
 fireEvent.click(within(screen.getByRole('navigation',{name:'Folders'})).getByRole('button',{name:'Models'}));
 expect(screen.getByRole('button',{name:'sd15/old.safetensors'})).toBeTruthy();
 // Tiles print the short name; the full path stays the accessible name. The view choice is remembered.
 expect(screen.getByRole('button',{name:'sd15/old.safetensors'}).querySelector('.wb-tile-name')!.textContent).toBe('old');
 fireEvent.change(screen.getByRole('combobox',{name:'View'}),{target:{value:'list'}});
 expect(document.querySelector('.wb-dock')!.getAttribute('data-view')).toBe('list');
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.dock.view')!)).toBe('list');
});

test.each([
 ['/env/ComfyUI/input','/env/ComfyUI/input/ref.png'],
 ['/env/ComfyUI/input/','/env/ComfyUI/input/ref.png'],
 ['C:\\env\\ComfyUI\\input\\','C:\\env\\ComfyUI\\input\\ref.png'],
 ['C:/env/ComfyUI/input/','C:\\env\\ComfyUI\\input\\ref.png'],
])('IPC fixture: Image→Image keeps native input-folder names and denoise (%s)',async(input,path)=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',path:'/m/checkpoints/a.safetensors',size:1,info:null,family:'sdxl'}]);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:[{name:'ref.png',path,kind:'file',symlink:false,media:'image',size:1,modified:1}],truncated:false}));
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'job-2',baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'queued',outputs:[],error:null,generationReady:false});
 vi.mocked(pollWorkbench).mockResolvedValue(collection([{id:'job-2',baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'failed',outputs:[],error:'x',generationReady:false}]));
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m',input,output:'/env/ComfyUI/output'}}/></I18nProvider>);
 await screen.findByRole('option',{name:'Text → Image',selected:true});
 expect(pipe('Image → Image').disabled).toBe(false);
 for(const name of ['Text → Video','Image → Video','Video → Video','Multi-reference → Video'])expect(pipe(name).disabled).toBe(true);
 choosePipe('Image → Image');
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
 // The missing pick is named once, on the preview, not in the settings column.
 expect(within(screen.getByRole('region',{name:'Preview'})).getByRole('status').textContent).toBe('Pick an image from the Input or Output folder in the Media dock.');
 expect(screen.getAllByText('Pick an image from the Input or Output folder in the Media dock.').length).toBe(1);
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 fireEvent.click(await screen.findByRole('button',{name:'ref.png'}));adopt();
 const ref=screen.getByRole('img',{name:'ref.png'});
 Object.defineProperties(ref,{naturalWidth:{value:1920},naturalHeight:{value:1080}});fireEvent.load(ref);
 // The slider under a number field drives the same value.
 fireEvent.change(screen.getByLabelText('Denoise').parentElement!.querySelector('input.wb-slider')!,{target:{value:'0.3'}});
 expect((screen.getByLabelText('Denoise') as HTMLInputElement).value).toBe('0.3');
 fireEvent.change(screen.getByLabelText('Denoise'),{target:{value:'0.4'}});
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({inputImage:'ref.png',denoise:0.4,checkpoint:'a.safetensors'});
 // ✕ on the preview drops the pick (and the shown job): the stage names the missing input again.
 fireEvent.click(screen.getByRole('button',{name:'Close preview'}));
 expect(screen.queryByRole('img',{name:'ref.png'})).toBeNull();
 expect(screen.queryByRole('button',{name:'Close preview'})).toBeNull();
 expect(screen.getByRole('button',{name:'ref.png'}).getAttribute('aria-current')).toBe('false');
 expect(within(screen.getByRole('region',{name:'Preview'})).getByRole('status').textContent).toBe('Pick an image from the Input or Output folder in the Media dock.');
});

test('IPC fixture: with an edit draft open, a queue pick opens beside the editor; a render opens in a zoomable full view',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',path:'/m/checkpoints/a.safetensors',size:1,info:null,family:'sdxl'}]);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:[{name:'out.png',path:'/env/ComfyUI/output/out.png',kind:'file',symlink:false,media:'image',size:1,modified:1}],truncated:false}));
 let made=0;
 vi.mocked(runWorkbenchJob).mockImplementation(async()=>({id:`job-${++made}`,baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'succeeded',outputRoot:'/env/ComfyUI/output',outputs:[{filename:'DEAS_0001.png',subfolder:'',type:'output'}],error:null,generationReady:false,intent:vi.mocked(runWorkbenchJob).mock.lastCall![1]} as JobStatus));
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
 await screen.findByRole('option',{name:'Text → Image',selected:true});
 fireEvent.click(await screen.findByRole('button',{name:'out.png'}));adopt();
 const img=screen.getByRole('img',{name:'out.png'});
 Object.defineProperties(img,{naturalWidth:{value:1024},naturalHeight:{value:640}});fireEvent.load(img);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await screen.findByRole('img',{name:'DEAS_0001.png'});
 expect(document.querySelector('.wb-editor-host')?.hasAttribute('hidden')).toBe(true); // text pipeline: the render takes the stage
 choosePipe('Image → Image');
 fireEvent.click(screen.getByRole('button',{name:/Succeeded/}));
 expect(document.querySelector('.wb-editor-host')?.hasAttribute('hidden')).toBe(false);
 expect(document.querySelector('.wb-stage')?.classList.contains('wb-split')).toBe(true);
 clickMedia(screen.getByRole('img',{name:'DEAS_0001.png'}) as HTMLImageElement);
 const dialog=screen.getByRole('dialog',{name:'DEAS_0001.png'});
 expect(within(dialog).getByRole('button',{name:'Fit'})).toBeTruthy();
 fireEvent.keyDown(window,{key:'Escape'});
 expect(screen.queryByRole('dialog')).toBeNull();
 // A queue pick is preview only: after ✕ it does not bring the job's source back into the editor.
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(2));
 fireEvent.click(screen.getByRole('button',{name:'Close preview'}));
 expect(document.querySelector('.wb-editor-host')).toBeNull();
 fireEvent.click(screen.getAllByRole('button',{name:/Succeeded/})[0]); // newest first = the image-to-image job
 expect(document.querySelector('.wb-editor-host')).toBeNull();
 // Slider compare zooms with the wheel at the cursor; double-click resets.
 fireEvent.click(within(document.querySelector<HTMLElement>('.wb-stage > [data-media-tools]')!).getByRole('button',{name:'Slider'}));
 const compare=document.querySelector<HTMLElement>('.wb-compare')!;
 act(()=>{compare.dispatchEvent(new WheelEvent('wheel',{deltaY:-500,clientX:10,clientY:10,cancelable:true}));});
 expect(Number(compare.dataset.zoom)).toBeGreaterThan(1);
 fireEvent.doubleClick(compare);
 expect(compare.dataset.zoom).toBe('1.00');
});

test('a queue click only previews; Use these settings is the explicit restore',async()=>{
 vi.mocked(scanModels).mockResolvedValue(sessionModels);vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/',entries:[{name:'source.png',path:'/out/source.png',kind:'file',symlink:false,media:'image',size:1,modified:1}],truncated:false}));
 vi.mocked(runWorkbenchJob).mockImplementation(async()=>({...heldJob('edit-job','succeeded'),baseUrl:evidence.baseUrl,outputRoot:'/out',outputs:[{filename:'result.png',subfolder:'',type:'output'}],intent:vi.mocked(runWorkbenchJob).mock.lastCall![1]}));
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/out'}}/></I18nProvider>);
 fireEvent.click(await screen.findByRole('button',{name:'source.png'}));adopt();
 const img=document.querySelector<HTMLImageElement>('.wb-current-preview img')!;Object.defineProperties(img,{naturalWidth:{value:1024},naturalHeight:{value:640}});fireEvent.load(img);
 choosePipe('Image → Image');sessionModel('folder/a.safetensors');sessionSettings({positive:'remembered i2i',steps:36});
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',false));fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await screen.findByRole('img',{name:'result.png'});fireEvent.click(screen.getByRole('button',{name:'Close preview'}));
 choosePipe('Text → Image');sessionSettings({positive:'outgoing t2i',steps:49});fireEvent.click(screen.getByRole('button',{name:'Close preview'}));
 const row=document.querySelector('[data-job-id="edit-job"] .wb-queue-select')!;
 fireEvent.click(row);
 expect(document.querySelector('[data-pipeline]')).toHaveProperty('value','t2i');expect(screen.getByLabelText('Prompt')).toHaveProperty('value','outgoing t2i');
 expect(screen.getByLabelText('Steps')).toHaveProperty('value','49');
 fireEvent.contextMenu(row);
 expect(document.querySelector('[data-pipeline]')).toHaveProperty('value','t2i');
 fireEvent.click(screen.getByRole('menuitem',{name:'Reuse prompt'}));
 expect(screen.getByLabelText('Prompt')).toHaveProperty('value','remembered i2i');
 expect(screen.getByLabelText('Steps')).toHaveProperty('value','49');
 expect(document.querySelector('[data-pipeline]')).toHaveProperty('value','t2i');
 fireEvent.contextMenu(row);
 fireEvent.click(screen.getByRole('menuitem',{name:'Use these settings'}));
 await waitFor(()=>expect(document.querySelector('[data-pipeline]')).toHaveProperty('value','i2i'));
 await waitFor(()=>expect(screen.getByLabelText('Prompt')).toHaveProperty('value','remembered i2i'));
 expect(screen.getByLabelText('Steps')).toHaveProperty('value','36');
 expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
});
test('changing only the checkpoint keeps the prompts exactly, empty ones included',async()=>{
 vi.mocked(scanModels).mockResolvedValue(sessionModels);vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-model-picker]')?.textContent).toContain('folder/a.safetensors'));
 sessionModel('folder/a.safetensors');sessionSettings({positive:'old A',negative:'old A negative',steps:31});
 sessionModel('other/a.safetensors');sessionSettings({positive:'  edited, with spaces  ',negative:'',steps:32});
 sessionModel('folder/a.safetensors');
 expect(screen.getByLabelText('Prompt')).toHaveProperty('value','  edited, with spaces  ');
 expect(document.querySelector('#run-negative')).toHaveProperty('value','');
 expect(screen.getByLabelText('Steps')).toHaveProperty('value','31');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test.each(['click','Enter',' '] as const)('retained original opens and stars source A, not edited draft B (%s)',async activation=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',path:'/m/a.safetensors',size:1,info:null,family:'sdxl'}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(invoke).mockImplementation(async(command,args)=>{
  if(command==='native_media_star_status')return {starred:false,unavailable:null};
  if(command==='native_media_star')return {oldPath:'/out/source-a.png',path:'/out/starred/source-a.png'};
  return {path:'/out',parent:'/',entries:['source-a.png','draft-b.png'].map(name=>({name,path:'/out/'+name,kind:'file',media:'image',symlink:false,size:1,modified:1})),truncated:false};
 });
 vi.mocked(runWorkbenchJob).mockImplementation(async(_endpoint,intent)=>({...heldJob('source-a-job','succeeded',evidence.baseUrl),intent,outputRoot:'/out',outputs:[{filename:'render.png',subfolder:'',type:'output'}]}));
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',input:'/input',output:'/out'}}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Image → Image').disabled).toBe(false));choosePipe('Image → Image');
 fireEvent.click(await screen.findByRole('button',{name:'source-a.png'}));adopt();
 const source=screen.getByRole('img',{name:'source-a.png'});
 Object.defineProperties(source,{naturalWidth:{value:1024},naturalHeight:{value:640}});fireEvent.load(source);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await screen.findByRole('img',{name:'render.png'});
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({inputImage:'source-a.png [output]'});
 fireEvent.click(screen.getByRole('button',{name:'draft-b.png'}));adopt();
 const draft=screen.getByRole('img',{name:'draft-b.png'});
 Object.defineProperties(draft,{naturalWidth:{value:1024},naturalHeight:{value:640}});fireEvent.load(draft);
 fireEvent.click(screen.getByRole('button',{name:'Select'}));
 const base=document.querySelector<HTMLElement>('[data-layer="base"]')!;
 fireEvent.keyDown(base,{key:'ArrowLeft'});expect(base.style.left).toBe('-8px');
 fireEvent.click(document.querySelector('[data-job-id="source-a-job"] .wb-queue-select')!);
 expect(base.querySelector('img')?.getAttribute('src')).toContain('/out/draft-b.png');
 expect(base.style.left).toBe('-8px');
 const original=document.querySelector<HTMLImageElement>('.wb-current-preview > figure.media-preview img')!;
 expect(original.getAttribute('src')).toContain('/out/source-a.png');
 expect(original.getAttribute('role')).toBe('button');expect(original.tabIndex).toBe(0);
 if(activation==='click')clickMedia(original);else{original.focus();expect(document.activeElement).toBe(original);expect(fireEvent.keyDown(original,{key:activation})).toBe(false);}
 const viewer=screen.getByRole('dialog',{name:'source-a.png'});
 expect(viewer.querySelector('img')?.getAttribute('src')).toContain('/out/source-a.png');
 expect(within(viewer).queryByRole('button',{name:'Star draft-b.png'})).toBeNull();
 const star=within(viewer).getByRole('button',{name:'Star source-a.png'});
 await waitFor(()=>expect(star.hasAttribute('disabled')).toBe(false));fireEvent.click(star);
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_media_star',{path:'/out/source-a.png'}));
 expect(invoke).not.toHaveBeenCalledWith('native_media_star',{path:'/out/draft-b.png'});
 expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
});

test('baseline-first readiness blocks only enabled refine and recovers when disabled',async()=>{
 localStorage.setItem('deas.pref.studio.run.post','true');
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',path:'/m/a',size:1,info:null,family:'sdxl'}]);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await screen.findByText(/Refine requires the DEAS baseline-first adapter/);
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);
 expect(runWorkbenchJob).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('tab',{name:/Post-Processing/}));
 fireEvent.click(screen.getByRole('checkbox',{name:'Refine / Upscale after every run'}));
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',false));
 expect(screen.queryByText(/Refine requires the DEAS baseline-first adapter/)).toBeNull();
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('IPC fixture: Post-process upscales the shown image alone, then the render; the opt-in tab adds a refine pass to Run',async()=>{
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'DEAS_ImagePostprocessGate']}});
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',path:'/m/checkpoints/a.safetensors',size:1,info:null,family:'sdxl'}]);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:[{name:'out.png',path:'/env/ComfyUI/output/out.png',kind:'file',symlink:false,media:'image',size:1,modified:1}],truncated:false}));
 let n=0;
 vi.mocked(runWorkbenchJob).mockImplementation(async()=>({id:`job-${++n}`,baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'succeeded',outputRoot:'/env/ComfyUI/output',outputs:[{filename:`DEAS_000${n}.png`,subfolder:'',type:'output'}],error:null,generationReady:false,intent:vi.mocked(runWorkbenchJob).mock.lastCall![1]} as JobStatus));
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
 fireEvent.click(await screen.findByRole('button',{name:'out.png'}));adopt();
 const img=screen.getByRole('img',{name:'out.png'});
 Object.defineProperties(img,{naturalWidth:{value:1000},naturalHeight:{value:600}});fireEvent.load(img);
 // Refine/Upscale live in the preview toolbar on every image (a video model would get an image job — videos are excluded).
 expect(screen.getByRole('button',{name:'Refine'})).toBeTruthy();
 expect(screen.getByRole('button',{name:'Upscale'})).toBeTruthy();
 fireEvent.click(screen.getByRole('tab',{name:'Post-Processing'}));
 fireEvent.change(screen.getByLabelText('Upscale factor'),{target:{value:'2'}});
 expect(screen.getByRole('button',{name:'Refine'})).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'Image to Image'}));
 await waitFor(()=>expect(screen.getByRole('button',{name:'Refine'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Refine'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 const first=vi.mocked(runWorkbenchJob).mock.calls[0][1];
 expect(first).toMatchObject({inputImage:'out.png [output]',denoise:0.25,width:1984,height:1216,inputResize:{width:1984,height:1216,crop:'disabled'}}); // 2000×1200 snapped per edge to the SDXL 64 grid
 expect(first.refine).toBeUndefined();expect(first.upscaleModel).toBeUndefined();
 // Default: side by side, the still-mounted editor on the source left, the render right.
 expect(await screen.findByRole('img',{name:'DEAS_0001.png'})).toBeTruthy();
 expect(document.querySelector('.wb-editor-host')?.hasAttribute('hidden')).toBe(false);
 expect(screen.queryByRole('slider',{name:'Original / edited divider'})).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'Slider',pressed:false}));
 expect(screen.getByRole('img',{name:'Original'})).toBeTruthy();
 expect(screen.getByRole('slider',{name:'Original / edited divider'})).toBeTruthy();
 expect(document.querySelector('.wb-editor-host')?.hasAttribute('hidden')).toBe(true);
 fireEvent.click(screen.getByRole('button',{name:'Slider',pressed:true}));
 expect(document.querySelector('.wb-editor-host')?.hasAttribute('hidden')).toBe(false);
 // Instant again on the shown render: transfer its actual file, never infer the backend root.
 vi.mocked(uploadInput).mockResolvedValue('deas-inputs/shown.png');
 fireEvent.click(within(document.querySelector<HTMLElement>('.wb-stage > [data-media-tools]')!).getByRole('button',{name:'Refine'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(2));
 expect(vi.mocked(runWorkbenchJob).mock.calls[1][1]).toMatchObject({inputImage:'deas-inputs/shown.png',width:3968,height:2432});
 // Opt-in: every Run gets the refine pass at the upscaled grid size.
 choosePipe('Text → Image');expect(screen.getByRole('button',{name:'Refine'})).toBeTruthy(); // t2i checkpoints are image models too
 fireEvent.click(screen.getByRole('checkbox',{name:'Refine / Upscale after every run'}));
 expect(screen.getByRole('tab',{name:/Post-Processing/}).textContent).toContain('✓');
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(3));
 const third=vi.mocked(runWorkbenchJob).mock.calls[2][1];
 expect(third.refine).toEqual({width:2048,height:2048,denoise:0.25});
 expect(third.inputImage).toBeUndefined();
 // Tiled refine is offered only when the backend lists the DaSiWa node; tiles = SDXL native 1024², overlap 128 on the 64 grid.
 const tiledBox=screen.getByRole('checkbox',{name:'Tiled refine (DaSiWa Tiled Diffusion)'});
 expect(tiledBox.hasAttribute('disabled')).toBe(true);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'DEAS_ImagePostprocessGate','DaSiWaTiledDiffusion']}});
 cleanup();
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
 await screen.findByRole('option',{name:'Text → Image',selected:true});
 const box=await screen.findByRole('checkbox',{name:'Tiled refine (DaSiWa Tiled Diffusion)'});
 await waitFor(()=>expect(box.hasAttribute('disabled')).toBe(false));
 fireEvent.click(box);
 expect(screen.getByText('Samples the refine pass in 1024×1024 tiles with 128 px overlap blended every step — the model’s native size, so large outputs stay sharp and seamless.')).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(4));
 expect(vi.mocked(runWorkbenchJob).mock.calls[3][1].refine).toEqual({width:2048,height:2048,denoise:0.25,tiled:{tileWidth:1024,tileHeight:1024,overlap:128}});
});

test.each([{cropped:false,paintBase:false,delayed:false,paintLayer:false},{cropped:true,paintBase:false,delayed:false,paintLayer:false},{cropped:false,paintBase:true,delayed:false,paintLayer:false},{cropped:true,paintBase:false,delayed:true,paintLayer:true}])('IPC fixture: Inpaint preserves draft and submits edited base ($cropped, $paintBase)',async({cropped,paintBase,delayed,paintLayer})=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',path:'/m/checkpoints/a.safetensors',size:1,info:null,family:'sdxl'}]);
 const entry=(name:string)=>({name,path:`/env/ComfyUI/output/${name}`,kind:'file',symlink:false,media:'image',size:1,modified:1});
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:[entry('out.png'),entry('cat.png')],truncated:false}));
 vi.stubGlobal('Image',class{naturalWidth=200;naturalHeight=100;onload=()=>{};onerror=()=>{};decode=async()=>{};set src(_:string){setTimeout(()=>this.onload());}});
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'job-in',baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'failed',outputs:[],error:null,generationReady:false});
 const uploadedMask='deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png';
 let releaseMask!:(name:string)=>void;
 vi.mocked(uploadMask).mockResolvedValue(uploadedMask);
 if(delayed)vi.mocked(uploadMask).mockReturnValueOnce(new Promise(yes=>{releaseMask=yes;}));
 const ctx={canvas:{width:1024,height:640},save:vi.fn(),restore:vi.fn(),getImageData:vi.fn(()=>({data:new Uint8ClampedArray([255,255,255,255])})),arc:vi.fn(),fill:vi.fn(),beginPath:vi.fn(),moveTo:vi.fn(),lineTo:vi.fn(),stroke:vi.fn(),clearRect:vi.fn(),fillRect:vi.fn(),drawImage:vi.fn()};
 const restore=[vi.spyOn(HTMLCanvasElement.prototype,'getContext').mockReturnValue(ctx as unknown as CanvasRenderingContext2D),
  vi.spyOn(HTMLCanvasElement.prototype,'toBlob').mockImplementation(cb=>cb({arrayBuffer:async()=>new Uint8Array([137,80,78,71]).buffer} as Blob)), // jsdom Blob has no arrayBuffer
  vi.spyOn(HTMLElement.prototype,'getBoundingClientRect').mockReturnValue({left:0,top:0,width:512,height:320} as DOMRect)];
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
 fireEvent.click(await screen.findByRole('button',{name:'out.png'}));adopt();
 const img=screen.getByRole('img',{name:'out.png'});
 Object.defineProperties(img,{naturalWidth:{value:1024},naturalHeight:{value:640}});fireEvent.load(img);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Inpaint'}).hasAttribute('disabled')).toBe(false));
 expect(screen.queryByRole('checkbox',{name:'Inpaint only masked'})).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'Inpaint'}));
 expect((screen.getByRole('spinbutton',{name:'Mask Blur (px)'}) as HTMLInputElement).value).toBe('4');
 if(cropped){fireEvent.click(screen.getByRole('checkbox',{name:'Inpaint only masked'}));fireEvent.change(screen.getByRole('spinbutton',{name:'Mask Shrink Grow (px)'}),{target:{value:'32'}});fireEvent.change(screen.getByRole('spinbutton',{name:'Mask Blur (px)'}),{target:{value:'8'}});}
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true); // nothing painted yet
 const mask=screen.getByLabelText('Inpaint mask',{selector:'canvas'}) as HTMLCanvasElement;
 const viewport=document.querySelector<HTMLElement>('[data-editor-viewport]')!;
 viewport.setPointerCapture=vi.fn();
 expect([mask.width,mask.height]).toEqual([1024,640]);
 fireEvent(viewport,new MouseEvent('pointerdown',{clientX:256,clientY:160,buttons:1,bubbles:true}));
 fireEvent(viewport,new MouseEvent('pointerup',{clientX:256,clientY:160,bubbles:true}));
 expect(ctx.arc).toHaveBeenCalledWith(512,320,24,0,Math.PI*2);
 fireEvent.click(screen.getByRole('button',{name:'Image to Image'}));
 fireEvent.click(screen.getByRole('button',{name:'Inpaint'}));
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false);
 dropTile('cat.png');
 const layer=await screen.findByRole('group',{name:'Inserted image cat.png [output]'});
 await waitFor(()=>{
  expect(layer.classList.contains('is-selected')).toBe(true);
  expect(screen.getByRole('button',{name:'Select',pressed:true})).toBeTruthy();
 });
 fireEvent.keyDown(layer,{key:'ArrowRight'});
 fireEvent.click(screen.getByRole('button',{name:'out.png'}));adopt();
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false);
 expect(screen.getByRole('group',{name:'Inserted image cat.png [output]'})).toBe(layer);
 expect(screen.getByLabelText('Inpaint mask',{selector:'canvas'})).toBe(mask);
 expect(document.querySelector('[data-editor-viewport]')).toBe(viewport);
 if(paintLayer){
  fireEvent.click(screen.getByRole('button',{name:'Brush'}));
  fireEvent(viewport,new MouseEvent('pointerdown',{clientX:256,clientY:160,buttons:1,bubbles:true}));
  fireEvent(viewport,new MouseEvent('pointerup',{clientX:256,clientY:160,bubbles:true}));
 }
 if(paintBase){
  vi.stubGlobal('fetch',vi.fn(async()=>({ok:true,blob:async()=>new Blob()})));
  vi.stubGlobal('createImageBitmap',vi.fn(async()=>({close:vi.fn()})));
  fireEvent.click(screen.getByRole('button',{name:'Base image'}));fireEvent.click(screen.getByRole('button',{name:'Brush'}));
  fireEvent(viewport,new MouseEvent('pointerdown',{clientX:200,clientY:100,buttons:1,bubbles:true}));
  fireEvent(viewport,new MouseEvent('pointerup',{clientX:200,clientY:100,bubbles:true}));
 }
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 if(delayed){
  await waitFor(()=>expect(uploadMask).toHaveBeenCalledTimes(1));
  fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'next draft'} });
  fireEvent.change(screen.getByLabelText('Mask Blur (px)'),{target:{value:'12'}});
  fireEvent.keyDown(layer,{key:'ArrowRight'});
  expect(runWorkbenchJob).not.toHaveBeenCalled();
  await act(async()=>releaseMask(uploadedMask));
 }
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(uploadMask).toHaveBeenCalledWith('http://127.0.0.1:7801',new Uint8Array([137,80,78,71]));
 const intent=vi.mocked(runWorkbenchJob).mock.calls[0][1];
 expect(intent).toMatchObject({inputImage:paintBase?'deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png':'out.png [output]',inputMask:'deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png',denoise:0.8,width:1024,height:640});
 expect(intent.inputResize).toBeUndefined();
 if(cropped){expect(intent.maskBlur).toBe(8);expect(intent.inpaintCrop).toEqual({x:0,y:0,width:41,height:41,targetWidth:1024,targetHeight:1024});}
 else{expect(intent.maskBlur).toBe(4);expect(intent.inpaintCrop).toBeUndefined();}
 if(paintBase){expect(intent.inputLayers).toBeUndefined();expect(uploadMask).toHaveBeenCalledTimes(2);}
 else expect(intent.inputLayers).toEqual([{image:paintLayer?uploadedMask:'cat.png [output]',x:420,y:270,width:200,height:100}]); // original size, centred then moved
 // The edit job opens side by side: the same editor stays live on the left, no return needed.
 expect(screen.queryByRole('button',{name:'Return to edit draft'})).toBeNull();
 expect(document.querySelector('.wb-editor-host')?.hasAttribute('hidden')).toBe(false);
 fireEvent.click(screen.getByRole('button',{name:'out.png'}));adopt();
 expect(document.querySelector('[data-editor-viewport]')).toBe(viewport);
 expect(screen.getByRole('group',{name:'Inserted image cat.png [output]'})).toBe(layer);
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false);
 fireEvent.click(screen.getByRole('button',{name:'cat.png'}));adopt();
 const next=screen.getByRole('img',{name:'cat.png'});
 Object.defineProperties(next,{naturalWidth:{value:1024},naturalHeight:{value:640}});fireEvent.load(next);
 expect(screen.queryByRole('group',{name:'Inserted image cat.png [output]'})).toBeNull();
 expect(screen.getByLabelText('Inpaint mask',{selector:'canvas'})).not.toBe(mask);
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
 restore.forEach(s=>s.mockRestore());
 vi.unstubAllGlobals();
});

test('IPC fixture: dropping a dock tile pastes the Output image as a movable layer and submits it as inputLayers in source pixels',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',path:'/m/checkpoints/a.safetensors',size:1,info:null,family:'sdxl'}]);
 const entry=(name:string)=>({name,path:`/env/ComfyUI/output/${name}`,kind:'file',symlink:false,media:'image',size:1,modified:1});
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:[entry('src.png'),entry('cat.png')],truncated:false}));
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'job-l',baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'queued',outputs:[],error:null,generationReady:false});
 // jsdom never loads images: report a 2:1 picture.
 vi.stubGlobal('Image',class{naturalWidth=200;naturalHeight=100;onload=()=>{};onerror=()=>{};set src(_:string){setTimeout(()=>this.onload());}});
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
 fireEvent.click(await screen.findByRole('button',{name:'src.png'}));adopt();
 const img=screen.getByRole('img',{name:'src.png'});
 Object.defineProperties(img,{naturalWidth:{value:1000},naturalHeight:{value:600}});fireEvent.load(img);
 await screen.findByRole('button',{name:'Image to Image'});
 fireEvent.click(screen.getByRole('button',{name:'Image to Image'}));
 await waitFor(()=>expect(document.querySelector('[data-editor-viewport]')).not.toBeNull());
 dropTile('cat.png');
 expect(screen.getByRole('button',{name:'Image to Image',pressed:true})).toBeTruthy();
 const layer=await screen.findByRole('group',{name:'Inserted image cat.png [output]'});
 expect(screen.getByRole('img',{name:'src.png'})).toBeTruthy(); // the source stays selected
 fireEvent.keyDown(layer,{key:'ArrowRight'});
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({inputImage:'src.png [output]',inputLayers:[{image:'cat.png [output]',x:408,y:250,width:200,height:100}]}); // original size centred (400,250) then ArrowRight +8
 vi.unstubAllGlobals();
});

test('checkpoint dropdown and Models tiles share selection/defaults and missing models are not substituted',async()=>{
 const model=(name:string,family='sdxl',category='checkpoints')=>({category,name,family,path:'/m/'+name,size:1,info:null});
 vi.mocked(scanModels).mockResolvedValue([model('first.safetensors'),model('second.safetensors'),model('unsupported.safetensors','unknown','diffusion_models')]);
 render(<I18nProvider><Workbench endpoint="" roots={{models:'/m'}}/></I18nProvider>);
 const select=await screen.findByRole('combobox',{name:'Checkpoint'});
 await waitFor(()=>expect((select as HTMLSelectElement).value).toBe('first.safetensors'));
 expect(within(select).queryByRole('option',{name:'unsupported.safetensors'})).toBeNull();
 fireEvent.change(select,{target:{value:'second.safetensors'}});
 dock('Models');expect(screen.getByRole('button',{name:'second.safetensors'}).getAttribute('aria-pressed')).toBe('true');
 expect((screen.getByLabelText('Steps') as HTMLInputElement).value).toBe('28');
 fireEvent.click(screen.getByRole('button',{name:'first.safetensors'}));expect((select as HTMLSelectElement).value).toBe('first.safetensors');
 expect(localStorage.getItem('deas.pref.studio.run.checkpoint')).toBe('"first.safetensors"');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test('known models in the wrong loader folder are hidden from the picker but remain disabled in Models, including duplicate names',async()=>{
 const m=(category:string,name:string,family?:string)=>({category,name,family,path:`/m/${category}/${name}`,size:1,info:null});
 const anima='dasiwaAnima_obsidianArchivesV2+Int8.safetensors';
 vi.mocked(scanModels).mockResolvedValue([m('checkpoints',anima,'anima'),m('diffusion_models',anima,'anima'),m('diffusion_models','misplaced-sdxl.safetensors','sdxl'),m('unet','legacy-anima.safetensors','anima'),m('checkpoints','unknown.safetensors')]);
 render(<I18nProvider><Workbench endpoint="" roots={{models:'/m'}}/></I18nProvider>);
 const select=await screen.findByRole('combobox',{name:'Checkpoint'});
 await waitFor(()=>expect(within(select).getAllByRole('option',{name:anima})).toHaveLength(1));
 expect((within(select).getByRole('option',{name:anima}) as HTMLOptionElement).value).toBe(JSON.stringify(['diffusion_models',`/m/diffusion_models/${anima}`,anima]));
 expect((select as HTMLSelectElement).selectedOptions[0].disabled).toBe(false);
 expect(within(select).queryByRole('option',{name:'misplaced-sdxl.safetensors'})).toBeNull();
 expect((within(select).getByRole('option',{name:'legacy-anima.safetensors'}) as HTMLOptionElement).disabled).toBe(false);
 expect((within(select).getByRole('option',{name:'unknown.safetensors'}) as HTMLOptionElement).disabled).toBe(false);
 dock('Models');
 const tiles=screen.getAllByRole('button',{name:anima}) as HTMLButtonElement[];
 expect(tiles.filter(b=>b.getAttribute('aria-disabled')==='true')).toHaveLength(1);
 expect(tiles.find(b=>b.getAttribute('aria-disabled')==='false')?.getAttribute('aria-pressed')).toBe('true');
 expect(screen.getByRole('button',{name:'misplaced-sdxl.safetensors'}).getAttribute('aria-disabled')).toBe('true');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('a remembered misplaced Anima stays blocked with an actionable warning rather than selecting another model',async()=>{
 const name='misplaced-anima.safetensors';
 localStorage.setItem('deas.pref.studio.run.checkpoint',JSON.stringify(name));
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name,family:'anima',path:'/m/'+name,size:1,info:null},{category:'checkpoints',name:'other.safetensors',path:'/m/other',size:1,info:null}]);
 render(<I18nProvider><Workbench endpoint="" roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('alert').textContent).toContain('diffusion_models'));
 expect((screen.getByRole('combobox',{name:'Checkpoint'}) as HTMLSelectElement).value).toBe(name);
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('companion files are collapsed below the other settings and preserve their choices while closed',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'diffusion_models',name:'anima.safetensors',family:'anima',path:'/m/a',size:1,info:null},{category:'text_encoders',name:'qwen_3_600m.safetensors',path:'/m/e',size:1,info:null},{category:'vae',name:'wan_2.1_vae.safetensors',path:'/m/v',size:1,info:null}]);
 render(<I18nProvider><Workbench endpoint="" roots={{models:'/m'}}/></I18nProvider>);
 const summary=await screen.findByText('Model files',{selector:'summary'}),details=summary.parentElement!;
 expect(details.hasAttribute('open')).toBe(false);expect(details.contains(screen.getByLabelText('Text encoder'))).toBe(true);
 expect(details.previousElementSibling?.classList.contains('wb-controls')).toBe(true);
 fireEvent.click(summary);const encoder=screen.getByRole('combobox',{name:'Text encoder'});
 fireEvent.change(encoder,{target:{value:'qwen_3_600m.safetensors'}});
 fireEvent.click(summary);expect(details.hasAttribute('open')).toBe(false);
 fireEvent.click(summary);expect((screen.getByRole('combobox',{name:'Text encoder'}) as HTMLSelectElement).value).toBe('qwen_3_600m.safetensors');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('Anima recommends renamed header-detected companions and ignores misleading known-incompatible names',async()=>{
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 const m=(category:string,name:string,family?:string,component?:string)=>({category,name,family,component,path:'/m/'+name,size:1,info:null});
 const encoder='dasiwaAnima_luminousLabyrinthV1_txt.safetensors',vae='provider-renamed-vae.safetensors';
 vi.mocked(scanModels).mockResolvedValue([m('diffusion_models','anima.safetensors','anima'),m('text_encoders',encoder,undefined,'qwen3-0.6b'),m('vae',vae,undefined,'wan-2.1-vae'),m('text_encoders','qwen_3_600m-impostor.safetensors',undefined,'qwen3-vl-8b')]);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await screen.findByRole('combobox',{name:'Text encoder'});
 for(const [label,name] of [['Text encoder',encoder],['VAE',vae]]){
  const select=screen.getByRole('combobox',{name:label}) as HTMLSelectElement;
  expect(select.selectedOptions[0].textContent).toBe('Automatic · '+name);
  expect(screen.getByRole('switch',{name:label+': All files'}).getAttribute('aria-checked')).toBe('false');
 }
 expect(within(screen.getByRole('combobox',{name:'Text encoder'})).queryByRole('option',{name:'qwen_3_600m-impostor.safetensors'})).toBeNull();
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false);
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('companion overrides are independently selectable without inferred matches, persist and gate Run when switched off',async()=>{
 const m=(category:string,name:string,family?:string)=>({category,name,family,path:'/m/'+category+'/'+name,size:1,info:null});
 const renamed='dasiwaAnima_luminousLabyrinthV1_txt.safetensors';
 vi.mocked(scanModels).mockResolvedValue([m('diffusion_models','anima.safetensors','anima'),m('text_encoders',renamed),m('vae','custom-vae.safetensors'),m('checkpoints','not-an-encoder.safetensors')]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'component-job',baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'failed',outputs:[],error:null,generationReady:false});
 const mount=()=>render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m'}}/></I18nProvider>);
 const view=mount();await screen.findByRole('combobox',{name:'Text encoder'});
 const encoder=screen.getByRole('combobox',{name:'Text encoder'}),vae=screen.getByRole('combobox',{name:'VAE'});
 const encoderSwitch=screen.getByRole('switch',{name:'Text encoder: All files'}),vaeSwitch=screen.getByRole('switch',{name:'VAE: All files'});
 expect(encoderSwitch.getAttribute('aria-checked')).toBe('false');
 expect(within(encoder).queryByRole('option',{name:renamed})).toBeNull();
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
 fireEvent.click(encoderSwitch);expect(vaeSwitch.getAttribute('aria-checked')).toBe('false');
 expect(within(encoder).queryByRole('option',{name:'not-an-encoder.safetensors'})).toBeNull();
 expect((encoder as HTMLSelectElement).value).toBe(''); // widening the list never auto-picks an arbitrary file
 fireEvent.change(encoder,{target:{value:renamed}});
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
 fireEvent.click(vaeSwitch);fireEvent.change(vae,{target:{value:'custom-vae.safetensors'}});
 expect(runWorkbenchJob).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'Run'}));await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({checkpoint:'anima.safetensors',diffusion:{textEncoder:renamed,clipType:'stable_diffusion',vae:'custom-vae.safetensors'}});
 fireEvent.click(encoderSwitch);
 expect((encoder as HTMLSelectElement).value).toBe(renamed); // never silently replace the manual choice
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
 expect(screen.getByRole('alert').textContent).toContain(renamed);
 view.unmount();mount();await screen.findByRole('combobox',{name:'Text encoder'});
 expect((screen.getByRole('combobox',{name:'Text encoder'}) as HTMLSelectElement).value).toBe(renamed);
 expect(screen.getByRole('switch',{name:'Text encoder: All files'}).getAttribute('aria-checked')).toBe('false');
 expect(screen.getByRole('switch',{name:'VAE: All files'}).getAttribute('aria-checked')).toBe('true');
});

test('audio VAE and text-projection overrides submit exact manual choices and never silently fall back after filtering or removal',async()=>{
 const m=(category:string,name:string,family?:string)=>({category,name,family,path:'/m/'+name,size:1,info:null});
 let files=[m('diffusion_models','ltx.safetensors','ltx-2'),m('text_encoders','gemma.safetensors'),m('text_encoders','text_projection.safetensors'),m('text_encoders','custom-projection.safetensors'),m('vae','video_vae.safetensors'),m('vae','audio_vae.safetensors'),m('vae','custom-audio.safetensors')];
 vi.mocked(scanModels).mockImplementation(async()=>files);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'video-components-job',baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'failed',outputs:[],error:null,generationReady:false});
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m'}}/></I18nProvider>);
 await screen.findByRole('combobox',{name:'Audio VAE'});
 for(const [label,file] of [['Audio VAE','custom-audio.safetensors'],['Text projection','custom-projection.safetensors']]){
  fireEvent.click(screen.getByRole('switch',{name:label+': All files'}));
  fireEvent.change(screen.getByRole('combobox',{name:label}),{target:{value:file}});
 }
 expect(screen.getByRole('switch',{name:'Text encoder: All files'}).getAttribute('aria-checked')).toBe('false');
 fireEvent.click(screen.getByRole('button',{name:'Run'}));await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({family:'ltx-2',ltx:{audioVae:'custom-audio.safetensors',textProjection:'custom-projection.safetensors'}});
 fireEvent.click(screen.getByRole('switch',{name:'Audio VAE: All files'}));
 expect((screen.getByRole('combobox',{name:'Audio VAE'}) as HTMLSelectElement).value).toBe('custom-audio.safetensors');
 expect(screen.getByRole('alert').textContent).toContain('custom-audio.safetensors');
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
 fireEvent.click(screen.getByRole('switch',{name:'Audio VAE: All files'}));
 files=files.filter(file=>file.name!=='custom-audio.safetensors');dock('Models');fireEvent.click(document.querySelector('[data-run-load]')!);
 await waitFor(()=>expect(screen.getByRole('alert').textContent).toContain('custom-audio.safetensors'));
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
 expect((screen.getByRole('combobox',{name:'Audio VAE'}) as HTMLSelectElement).value).toBe('custom-audio.safetensors');
 expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
});

test('IPC fixture: families gate models and LoRAs, picking applies the family defaults and a diffusion model submits its encoder and VAE',async()=>{
 const m=(category:string,name:string,family?:string)=>({category,name,path:`/m/${category}/${name}`,size:1,info:null,...(family?{family}:{})});
 vi.mocked(scanModels).mockResolvedValue([m('checkpoints','ill.safetensors','sdxl'),m('diffusion_models','k2.safetensors','krea-2'),m('diffusion_models','h3.safetensors','minimax-h3'),m('unet','odd.safetensors'),
  m('text_encoders','qwen3vl_4b.safetensors'),m('vae','Q/qwen_image_vae.safetensors'),m('loras','k-style.safetensors','krea-2'),m('loras','ill-style.safetensors','sdxl'),m('loras','mystery.safetensors')]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,samplers:['euler','er_sde','res_multistep'],schedulers:['normal','simple','beta']}});
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'job-3',baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'failed',outputs:[],error:'x',generationReady:false});
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m'}}/></I18nProvider>);
 dock('Models');
 expect((await screen.findByRole('button',{name:/h3\.safetensors/})).getAttribute('aria-disabled')).toBe('true'); // video family: no image recipe
 expect(screen.getByRole('button',{name:/odd\.safetensors/}).getAttribute('aria-disabled')).toBe('true'); // unknown diffusion model
 fireEvent.click(screen.getByRole('button',{name:/k2\.safetensors/}));
 expect((screen.getByLabelText('Steps') as HTMLInputElement).value).toBe('8');
 expect((screen.getByLabelText('CFG') as HTMLInputElement).value).toBe('1');
 expect((screen.getByLabelText('Text encoder') as HTMLSelectElement).selectedOptions[0].textContent).toBe('Automatic · qwen3vl_4b.safetensors');
 dock('LoRAs');
 expect(screen.queryByRole('button',{name:/ill-style/})).toBeNull(); // other family hidden, unknown kept
 fireEvent.click(screen.getByRole('button',{name:/k-style/}));expect(screen.getByRole('button',{name:/mystery/})).toBeTruthy();
 expect(checkRecipePrerequisites).toHaveBeenCalledTimes(1); // the backend list is read once when the endpoint is known, so the first click already shows it
 await waitFor(()=>expect(Array.from((screen.getByLabelText('Sampler') as HTMLSelectElement).options,o=>o.value)).toEqual(['euler','er_sde','res_multistep']));
 fireEvent.focus(screen.getByLabelText('Sampler'));expect(checkRecipePrerequisites).toHaveBeenCalledTimes(1);
 fireEvent.change(screen.getByLabelText('Sampler'),{target:{value:'res_multistep'}});
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({checkpoint:'k2.safetensors',sampler:'res_multistep',scheduler:'simple',loras:[{name:'k-style.safetensors',strength:1}],diffusion:{textEncoder:'qwen3vl_4b.safetensors',clipType:'krea2',vae:'Q/qwen_image_vae.safetensors'}});
});

test('IPC fixture: a Turbo family locks steps and scheduler to its saved schedule, submits the sigmas and keeps the base family LoRAs',async()=>{
 const m=(category:string,name:string,family?:string)=>({category,name,path:`/m/${category}/${name}`,size:1,info:null,...(family?{family}:{})});
 vi.mocked(scanModels).mockResolvedValue([m('diffusion_models','turbo.safetensors','qwen-image-2.1-turbo'),m('text_encoders','qwen3vl_8b.safetensors'),m('vae','qwen_image_2.1_vae.safetensors'),m('loras','q-style.safetensors','qwen-image-2.1')]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'job-4',baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'failed',outputs:[],error:'x',generationReady:false});
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m'}}/></I18nProvider>);
 dock('Models');fireEvent.click(await screen.findByRole('button',{name:/turbo\.safetensors/}));
 const steps=screen.getByLabelText('Steps') as HTMLInputElement;expect(steps.value).toBe('8');expect(steps.disabled).toBe(true);
 const scheduler=screen.getByLabelText('Scheduler') as HTMLSelectElement;expect(scheduler.disabled).toBe(true);expect(scheduler.options[scheduler.selectedIndex].text).toBe('CUSTOM');
 expect((screen.getByLabelText('Sampler') as HTMLSelectElement).disabled).toBe(false);
 dock('LoRAs');expect(screen.getByRole('button',{name:/q-style/})).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({checkpoint:'turbo.safetensors',steps:8,cfg:1,sampler:'euler',sigmas:[1.0,0.978453,0.95418,0.926626,0.89508,0.845148,0.704534,0.414568,0]});
 // A typed schedule in Options overrides the model's own; steps follow it.
 dock('Options');fireEvent.change(screen.getByRole('textbox',{name:'Custom sigmas'}),{target:{value:'1, 0.6, 0.3, 0'}});
 expect((screen.getByLabelText('Steps') as HTMLInputElement).value).toBe('3');
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(2));
 expect(vi.mocked(runWorkbenchJob).mock.calls[1][1]).toMatchObject({steps:3,sigmas:[1,0.6,0.3,0]});
});

test('automatic aspect is confined to media pipelines and text pipelines ignore image and video previews',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',path:'/m/checkpoints/a.safetensors',size:1,info:null,family:'sdxl'}]);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:[
  {name:'wide.png',path:'/env/input/wide.png',kind:'file',symlink:false,media:'image',size:1,modified:1},
  {name:'portrait.mp4',path:'/env/input/portrait.mp4',kind:'file',symlink:false,media:'video',size:1,modified:2},
 ],truncated:false}));
 const {container}=render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m',input:'/env/input'}}/></I18nProvider>);
 await screen.findByRole('option',{name:'Text → Image',selected:true});
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 const aspectPicker=screen.getByLabelText('Aspect ratio') as HTMLSelectElement;
 expect(Array.from(aspectPicker.options,o=>o.value)).not.toContain('auto');
 fireEvent.change(screen.getByLabelText('Resolution'),{target:{value:'model'}});
 fireEvent.click(await screen.findByRole('button',{name:'wide.png'}));adopt();
 const image=screen.getByRole('img',{name:'wide.png'});
 Object.defineProperties(image,{naturalWidth:{value:1920},naturalHeight:{value:1080}});fireEvent.load(image);
 expect((screen.getByLabelText('Width') as HTMLInputElement).value).toBe('1024');
 choosePipe('Image → Image');
 expect(Array.from(aspectPicker.options,o=>o.value)).toContain('auto');
 expect(aspectPicker.value).toBe('auto');
 fireEvent.change(screen.getByLabelText('Input scaling'),{target:{value:'target'}});
 expect((screen.getByLabelText('Width') as HTMLInputElement).value).toBe('1344');
 choosePipe('Text → Image');
 expect(Array.from(aspectPicker.options,o=>o.value)).not.toContain('auto');
 expect(aspectPicker.value).toBe('1:1');
 // Native: the stage video plays a blob of the asset bytes (WebKitGTK rejects asset:// media), fetched once per selection.
 vi.stubGlobal('fetch',vi.fn(async()=>({ok:true,blob:async()=>new Blob(['x'],{type:'video/mp4'})})));
 Object.defineProperty(URL,'createObjectURL',{configurable:true,value:vi.fn(()=>'blob:deas/1')});Object.defineProperty(URL,'revokeObjectURL',{configurable:true,value:vi.fn()});
 fireEvent.click(screen.getByRole('button',{name:'portrait.mp4'}));adopt();
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false);
 const video=container.querySelector('.wb-stage video')!;
 expect(fetch).toHaveBeenCalledWith('asset://localhost//env/input/portrait.mp4');
 await waitFor(()=>expect(video.getAttribute('src')).toBe('blob:deas/1'));
 Object.defineProperties(video,{videoWidth:{value:1080},videoHeight:{value:1920}});fireEvent.loadedMetadata(video);
 expect((screen.getByLabelText('Width') as HTMLInputElement).value).toBe('1024');
 expect((screen.getByLabelText('Height') as HTMLInputElement).value).toBe('1024');
 expect(container.querySelector('.wb-resolution-summary')).toBeNull();
 expect(container.querySelector('.wb-aspect-picker .wb-aspect-swatch')).not.toBeNull();
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false);
 expect(pipe('Image → Video').disabled).toBe(true);
 fireEvent.change(screen.getByLabelText('Width'),{target:{value:'640'}});
 fireEvent.change(screen.getByLabelText('Height'),{target:{value:'480'}});
 fireEvent.click(screen.getByRole('button',{name:'wide.png'}));adopt();
 const nextImage=screen.getByRole('img',{name:'wide.png'});
 Object.defineProperties(nextImage,{naturalWidth:{value:1920},naturalHeight:{value:1080}});fireEvent.load(nextImage);
 expect((screen.getByLabelText('Width') as HTMLInputElement).value).toBe('640');
 expect((screen.getByLabelText('Height') as HTMLInputElement).value).toBe('480');
 expect((screen.getByLabelText('Resolution') as HTMLSelectElement).value).toBe('fixed');
});

test('Inpaint rejects a source outside the model grid instead of silently rounding its pixels',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',path:'/m/a',size:1,info:null,family:'sdxl'}]);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:[{name:'odd.png',path:'/env/out/odd.png',kind:'file',symlink:false,media:'image',size:1,modified:1}],truncated:false}));
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/env/out'}}/></I18nProvider>);
 fireEvent.click(await screen.findByRole('button',{name:'odd.png'}));adopt();
 const img=screen.getByRole('img',{name:'odd.png'});
 Object.defineProperties(img,{naturalWidth:{value:1000},naturalHeight:{value:600}});fireEvent.load(img);
 fireEvent.click(screen.getByRole('button',{name:'Inpaint'}));
 expect(screen.getByText('Resample this image to the selected model’s resolution grid before inpainting.')).toBeTruthy();
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
 expect(uploadMask).not.toHaveBeenCalled();
});

test.each(['late','error'])('Dropped tile %s loading cannot create a guessed or stale image layer',async(mode)=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',path:'/m/a',size:1,info:null,family:'sdxl'}]);
 const entry=(name:string)=>({name,path:`/env/out/${name}`,kind:'file',symlink:false,media:'image',size:1,modified:1});
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:[entry('base.png'),entry('cat.png'),entry('other.png')],truncated:false}));
 let loaded=()=>{},failed=()=>{};
 vi.stubGlobal('Image',class{naturalWidth=200;naturalHeight=100;onload=()=>{};onerror=()=>{};set src(_:string){loaded=()=>this.onload();failed=()=>this.onerror();}});
 try{
  render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/env/out'}}/></I18nProvider>);
  fireEvent.click(await screen.findByRole('button',{name:'base.png'}));adopt();
  const img=screen.getByRole('img',{name:'base.png'});
  Object.defineProperties(img,{naturalWidth:{value:1024},naturalHeight:{value:640}});fireEvent.load(img);
  await waitFor(()=>expect(document.querySelector('[data-editor-viewport]')).not.toBeNull());
  dropTile('cat.png');
  if(mode==='late'){
   fireEvent.click(screen.getByRole('button',{name:'other.png'}));adopt();
   const other=screen.getByRole('img',{name:'other.png'});
   Object.defineProperties(other,{naturalWidth:{value:1024},naturalHeight:{value:640}});fireEvent.load(other);
   await act(async()=>loaded());
  }
  else{await act(async()=>failed());expect(screen.getByRole('alert').textContent).toBe('This image could not be loaded for insertion.');}
  expect(screen.queryByRole('group',{name:'Inserted image cat.png [output]'})).toBeNull();
 }finally{vi.unstubAllGlobals();}
});

test('native resolution is one model-driven choice and migrates the removed 768px preference',async()=>{
 localStorage.setItem('deas.pref.studio.run.resolution',JSON.stringify('auto'));
 localStorage.setItem('deas.pref.studio.run.width','768');
 localStorage.setItem('deas.pref.studio.run.height','768');
 const m=(category:string,name:string,family?:string)=>({category,name,path:`/m/${category}/${name}`,size:1,info:null,...(family?{family}:{})});
 vi.mocked(scanModels).mockResolvedValue([m('checkpoints','base.safetensors','sdxl'),m('diffusion_models','qwen.safetensors','qwen-image-2.1'),m('text_encoders','qwen3vl_8b.safetensors'),m('vae','qwen_image_2.1_vae.safetensors')]);
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m'}}/></I18nProvider>);
 dock('Models');
 await screen.findByRole('button',{name:/base.safetensors/});
 const picker=screen.getByLabelText('Resolution') as HTMLSelectElement;
 expect(Array.from(picker.options,o=>o.value)).not.toContain('auto');
 expect(Array.from(picker.options).filter(o=>o.textContent==='Native')).toHaveLength(1);
 expect(picker.value).toBe('model');
 expect((screen.getByLabelText('Width') as HTMLInputElement).value).toBe('1024');
 fireEvent.click(screen.getByRole('button',{name:/qwen.safetensors/}));
 expect((screen.getByLabelText('Width') as HTMLInputElement).value).toBe('2048');
 expect((screen.getByLabelText('Height') as HTMLInputElement).value).toBe('2048');
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.resolution')!)).toBe('model');
});

test('resolution picker follows reference metadata, forwards padded pixels to the model and remembers settings',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',path:'/m/checkpoints/a.safetensors',size:1,info:null,family:'sdxl'}]);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:[{name:'wide.png',path:'/env/input/wide.png',kind:'file',symlink:false,media:'image',size:1,modified:1}],truncated:false}));
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'resolution-job',baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'failed',outputs:[],error:'fixture',generationReady:false});
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m',input:'/env/input'}}/></I18nProvider>);
 await screen.findByRole('option',{name:'Text → Image',selected:true});
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 fireEvent.click(await screen.findByRole('button',{name:'wide.png'}));adopt();
 choosePipe('Image → Image');
 fireEvent.change(screen.getByLabelText('Resolution'),{target:{value:'model'}});
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
 const ref=screen.getByRole('img',{name:'wide.png'});
 Object.defineProperties(ref,{naturalWidth:{value:1920},naturalHeight:{value:1080}});fireEvent.load(ref);
 expect((screen.getByLabelText('Width') as HTMLInputElement).value).toBe('1344');
 expect((screen.getByLabelText('Height') as HTMLInputElement).value).toBe('768');
 fireEvent.change(screen.getByLabelText('Aspect ratio'),{target:{value:'1:1'}});
 fireEvent.change(screen.getByLabelText('Resolution'),{target:{value:'1024p'}});
 fireEvent.change(screen.getByLabelText('Input scaling'),{target:{value:'pad'}});
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({width:1024,height:1024,inputImage:'wide.png',inputResize:{width:1024,height:576,crop:'disabled',pad:{left:0,right:0,top:224,bottom:224}}});
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.inputScaling')!)).toBe('pad');
 fireEvent.change(screen.getByLabelText('Resolution'),{target:{value:'fixed'}});
 fireEvent.change(screen.getByLabelText('Width'),{target:{value:'8192'}});
 fireEvent.change(screen.getByLabelText('Height'),{target:{value:'8192'}});
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
});

test('IPC fixture: video families enable Image→Video; LTX submits a video intent with frames on the 8k+1 grid and a video output plays in the stage',async()=>{
 vi.mocked(scanModels).mockResolvedValue([
  {category:'unet',name:'LTX2/ltx.safetensors',path:'/m/unet/LTX2/ltx.safetensors',size:1,info:null,family:'ltx-2'},
  {category:'text_encoders',name:'gemma_3_12B_it_fp8.safetensors',path:'/m/text_encoders/gemma.safetensors',size:1,info:null},
  {category:'text_encoders',name:'ltx-2.3_text_projection_bf16.safetensors',path:'/m/text_encoders/proj.safetensors',size:1,info:null},
  {category:'vae',name:'LTX2/LTX23_video_vae_bf16.safetensors',path:'/m/vae/v.safetensors',size:1,info:null},
  {category:'vae',name:'LTX2/LTX23_audio_vae_bf16.safetensors',path:'/m/vae/a.safetensors',size:1,info:null}]);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:[{name:'ref.png',path:'/env/ComfyUI/input/ref.png',kind:'file',symlink:false,media:'image',size:1,modified:1}],truncated:false}));
 vi.mocked(runWorkbenchJob).mockImplementation(async()=>({id:'job-v',baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'succeeded',outputs:[{filename:'x_00001.webm',subfolder:'video',type:'output',format:'video/webm'}],error:null,generationReady:false,intent:vi.mocked(runWorkbenchJob).mock.lastCall![1]} as JobStatus));
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Text → Video').disabled).toBe(false));
 expect(pipe('Image → Video').disabled).toBe(false);
 expect(pipe('Video → Video').disabled).toBe(false);
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 fireEvent.click(await screen.findByRole('button',{name:'ref.png'}));adopt();
 choosePipe('Image → Video');
 expect((screen.getByLabelText('Steps') as HTMLInputElement).value).toBe('8');
 const ref=screen.getByRole('img',{name:'ref.png'});
 Object.defineProperties(ref,{naturalWidth:{value:1280},naturalHeight:{value:704}});fireEvent.load(ref);
 fireEvent.change(screen.getByLabelText('Seconds'),{target:{value:'2'}});
 expect(screen.getByText('49 frames · 2.00 s')).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({recipeId:'video-generation',family:'ltx-2',model:'LTX2/ltx.safetensors',textEncoder:'gemma_3_12B_it_fp8.safetensors',vae:'LTX2/LTX23_video_vae_bf16.safetensors',frames:49,frameRate:24,firstFrame:'ref.png',output:{codec:'Auto',container:'Auto',quality:20},ltx:{textProjection:'ltx-2.3_text_projection_bf16.safetensors',audioVae:'LTX2/LTX23_audio_vae_bf16.safetensors',passes:1}});
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).not.toHaveProperty('imageFormat');
 await waitFor(()=>expect(document.querySelector('.wb-stage video')?.getAttribute('src')).toBe('http://127.0.0.1:7801/view?filename=x_00001.webm&subfolder=video&type=output'));
 expect(screen.queryByRole('tab',{name:'Continuity'})).toBeNull();
});

test.each(['ltx-2','wan','sdxl','minimax-h3'])('Continuity is absent from non-H3-video pipelines (%s)',async family=>{
 const model={category:'diffusion_models',name:'model.safetensors',path:'/m/model.safetensors',size:1,info:null,family};
 vi.mocked(scanModels).mockResolvedValue(family==='ltx-2'?[...ltxModels]:[model]);
 render(<I18nProvider><Workbench onOpenPromptForge={vi.fn()} endpoint="http://127.0.0.1:7801" roots={{models:'/m'}}/></I18nProvider>);
 await act(async()=>{});
 for(const pipeline of family==='ltx-2'||family==='wan'?['t2v','i2v','v2v','ref2v']:['t2i','i2i']){
  fireEvent.change(screen.getByRole('combobox',{name:'Pipeline'}),{target:{value:pipeline}});
  expect(screen.queryByRole('button',{name:'Continuity'})).toBeNull();
  expect(screen.queryByRole('dialog',{name:'Continuity'})).toBeNull();
  expect(document.querySelector('[data-open-prompt-forge]')).toBeNull();
 }
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
});

test('H3 continuity opens outside the dock, preserves edits and restores focus when closed',async()=>{
 localStorage.setItem('deas.pref.studio.dock.tab',JSON.stringify('continuity'));
 localStorage.setItem('deas.pref.studio.run.continuity',JSON.stringify({enabled:true,session:'kept-session',outputMode:'segment',steps:[{id:'one',seconds:5,action:''},{id:'two',seconds:6,action:'turn'}]}));
 vi.mocked(scanModels).mockResolvedValue([
  {category:'diffusion_models',name:'MiniMaxH3/h3.safetensors',path:'/m/h3.safetensors',size:1,info:null,family:'minimax-h3'},
  {category:'text_encoders',name:'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors',path:'/m/t.safetensors',size:1,info:null},
  {category:'vae',name:'MiniMaxH3/minimax_h3_video_vae_fp16.safetensors',path:'/m/v.safetensors',size:1,info:null},
  {category:'vae',name:'MiniMaxH3/minimax_h3_audio_vae_fp32.safetensors',path:'/m/a.safetensors',size:1,info:null}]);
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Text → Video').disabled).toBe(false));choosePipe('Text → Video');
 expect(screen.queryByRole('tab',{name:'Continuity'})).toBeNull();
 expect(screen.getByRole('tab',{name:'Media'}).getAttribute('aria-selected')).toBe('true');
 const open=screen.getByRole('button',{name:'Continuity'});open.focus();fireEvent.click(open);
 const dialog=screen.getByRole('dialog',{name:'Continuity'});
 expect(within(dialog).getByText('kept-session')).toBeTruthy();
 fireEvent.change(within(dialog).getByLabelText('Next action'),{target:{value:'a new action'}});
 fireEvent.click(within(dialog).getByRole('button',{name:'Close'}));
 expect(screen.queryByRole('dialog',{name:'Continuity'})).toBeNull();expect(document.activeElement).toBe(open);
 fireEvent.click(open);
 expect((within(screen.getByRole('dialog',{name:'Continuity'})).getByLabelText('Next action') as HTMLTextAreaElement).value).toBe('a new action');
 fireEvent(screen.getByRole('dialog',{name:'Continuity'}),new Event('cancel',{cancelable:true}));
 expect(screen.queryByRole('dialog',{name:'Continuity'})).toBeNull();expect(document.activeElement).toBe(open);
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('H3 frame people are opt-in, reach the Forge and reset on a new source picture',async()=>{
 vi.mocked(scanModels).mockResolvedValue([
  {category:'diffusion_models',name:'MiniMaxH3/h3.safetensors',path:'/m/h3.safetensors',size:1,info:null,family:'minimax-h3'},
  {category:'text_encoders',name:'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors',path:'/m/t.safetensors',size:1,info:null},
  {category:'vae',name:'MiniMaxH3/minimax_h3_video_vae_fp16.safetensors',path:'/m/v.safetensors',size:1,info:null},
  {category:'vae',name:'MiniMaxH3/minimax_h3_audio_vae_fp32.safetensors',path:'/m/a.safetensors',size:1,info:null}]);
 const calls:{route:string;body:Record<string,unknown>}[]=[];
 vi.mocked(invoke).mockImplementation(async(c,a)=>{
  if(c==='native_llm_read')return {provider:'ollama',baseUrl:'http://127.0.0.1:11434'};
  if(c==='native_forge'){const x=a as {route:string;body:Record<string,unknown>};calls.push(x);return x.route==='models'?{models:[{id:'m',label:'M'}],errors:{},creativity:['balanced'],shot_counts:['Auto'],default_shots:'Auto'}:{simple_prompt:'a scene',warnings:[]};}
  return {path:(a as {path:string})?.path,parent:'/env',entries:['first.png','other.png'].map(name=>({name,path:`/env/ComfyUI/input/${name}`,kind:'file',symlink:false,media:'image',size:1,modified:1})),truncated:false};
 });
 function Workspace(){
  const [host,setHost]=useState<HTMLDivElement|null>(null),[tab,setTab]=useState('generate');
  return <><button onClick={()=>setTab('generate')}>Return to Generate</button><button onClick={()=>setTab('forge')}>Return to Forge</button>
   <div hidden={tab!=='generate'}><Workbench forgeHost={host} onOpenPromptForge={()=>setTab('forge')} endpoint="http://127.0.0.1:7801" root="/env" roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></div>
   <div ref={setHost} hidden={tab!=='forge'}/></>;
 }
 render(<I18nProvider><Workspace/></I18nProvider>);
 expect(document.querySelector('[data-open-prompt-forge]')).toBeNull();
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 fireEvent.click(await screen.findByRole('button',{name:'first.png'}));adopt();
 await waitFor(()=>expect(pipe('Image → Video').disabled).toBe(false));choosePipe('Image → Video');
 expect(document.querySelector('.wb-settings [data-ref-people]')).toBeNull();
 // The prompt's Forge link keeps the hammer (colour-inverted), not an arrow.
 expect(document.querySelector('.forge-hammer.forge-link svg')).toBeTruthy();
 fireEvent.click(document.querySelector('[data-open-prompt-forge]')!);
 expect(document.querySelector('[data-h3-forge-tab]')).toBeTruthy();
 expect(screen.queryByRole('dialog',{name:'Prompt Forge'})).toBeNull();
 // People & order is an always-open section in the Forge tab, not a collapsed menu.
 expect(document.querySelector('[data-h3-forge-tab] details[data-ref-people]')).toBeNull();
 expect(screen.getByRole('region',{name:/^People & order · .*first\.png$/})).toBeTruthy();
 const people=screen.getByRole('button',{name:'Character 2 in first.png'});
 expect(people.getAttribute('aria-pressed')).toBe('false');
 fireEvent.click(people);
 fireEvent.change(screen.getByRole('combobox',{name:'Character order in first.png'}),{target:{value:'y'}});
 expect(people.getAttribute('aria-pressed')).toBe('true');
 fireEvent.click(people);expect(people.getAttribute('aria-pressed')).toBe('false');
 fireEvent.click(people);
 await screen.findAllByRole('option',{name:'M'});
 fireEvent.change(screen.getByRole('textbox',{name:'Card of Character 2'}),{target:{value:'adult traveller, blue coat'}});
 fireEvent.change(screen.getByRole('textbox',{name:'Idea or prompt to enhance'}),{target:{value:'a walk'}});
 fireEvent.click(screen.getByRole('button',{name:'Write'}));
 await screen.findByRole('textbox',{name:'Result'});
 expect(calls.find(c=>c.route==='h3')!.body).toMatchObject({mode:'I2VA',easy:false,references:[{path:'first.png',who:[2],who_axis:'y'}],cast:{2:'adult traveller, blue coat'}});
 fireEvent.click(screen.getByRole('button',{name:'Apply'}));
 fireEvent.click(screen.getByRole('button',{name:'Return to Generate'}));
 expect(screen.getByRole('textbox',{name:'Prompt'})).toHaveProperty('value','a scene');
 fireEvent.click(screen.getByRole('button',{name:'Return to Forge'}));
 expect(screen.getByRole('textbox',{name:'Card of Character 2'})).toHaveProperty('value','adult traveller, blue coat');
 expect(screen.getByRole('textbox',{name:'Result'})).toHaveProperty('value','a scene');
 expect(document.querySelector('[data-forge-draft="1"]')).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'Return to Generate'}));
 fireEvent.click(screen.getByRole('button',{name:'other.png'}));adopt();
 fireEvent.click(document.querySelector('[data-open-prompt-forge]')!);
 expect(screen.getByRole('button',{name:'Character 2 in other.png'}).getAttribute('aria-pressed')).toBe('false');
 fireEvent.click(screen.getByRole('button',{name:'Return to Generate'}));
 choosePipe('Multi-reference → Video');
 fireEvent.click(menuOf('first.png').getByRole('menuitem',{name:'Add as reference'}));
 fireEvent.click(document.querySelector('[data-open-prompt-forge]')!);
 fireEvent.click(screen.getByRole('button',{name:'Character 2 in first.png'}));
 fireEvent.change(screen.getByRole('combobox',{name:'Character order in first.png'}),{target:{value:'z'}});
 fireEvent.click(screen.getByRole('button',{name:'Regenerate'}));
 await waitFor(()=>expect(calls.filter(c=>c.route==='h3')).toHaveLength(2));
 expect(calls.filter(c=>c.route==='h3')[1].body).toMatchObject({mode:'REF2VA',references:[{path:'first.png',who:[1,2],who_axis:'z'}]});
 fireEvent.click(screen.getByRole('button',{name:'Return to Generate'}));
 fireEvent.click(document.querySelector('[data-open-references]')!);
 fireEvent.click(within(screen.getByRole('dialog',{name:'References'})).getByRole('button',{name:'Edit first.png'}));
 expect(within(screen.getByRole('dialog',{name:'Edit first.png'})).getByRole('combobox',{name:'Character order in first.png'})).toHaveProperty('value','z');
});

test.each(['t2v','i2v','v2v','ref2v'].flatMap(pipeline=>['new','checkpoint','video'].flatMap(sourceMode=>
 (pipeline==='ref2v'?[false,true]:[false]).map(useReferences=>({pipeline,sourceMode,useReferences})))))(
 'IPC fixture: MiniMax H3 uploads dock references and pins continuity clips ($pipeline / $sourceMode / references=$useReferences)',async({pipeline,sourceMode,useReferences})=>{
 const selected=sourceMode!=='new';const video=sourceMode==='video';const videoId='a'.repeat(64);
 const ref2va=pipeline==='v2v'||pipeline==='ref2v';const mode=ref2va?'REF2VA':pipeline==='i2v'?'I2VA':'T2VA';
 const outputMode=sourceMode==='checkpoint'?'segment':'full';
 vi.mocked(prepareH3Video).mockResolvedValue({sourceId:videoId,filename:'clips/source.mp4',sha256:'b'.repeat(64),width:1920,height:1080,fps:24,seconds:8,hasAudio:true});
 vi.mocked(scanModels).mockResolvedValue([
  {category:'diffusion_models',name:'MiniMaxH3/h3.safetensors',path:'/m/diffusion_models/MiniMaxH3/h3.safetensors',size:1,info:null,family:'minimax-h3'},
  {category:'text_encoders',name:'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors',path:'/m/t.safetensors',size:1,info:null},
  {category:'vae',name:'MiniMaxH3/minimax_h3_video_vae_fp16.safetensors',path:'/m/v.safetensors',size:1,info:null},
  {category:'vae',name:'MiniMaxH3/minimax_h3_audio_vae_fp32.safetensors',path:'/m/a.safetensors',size:1,info:null}]);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:mediaEntries,truncated:false}));
 vi.mocked(uploadInput).mockImplementation(async(_endpoint,path)=>path.endsWith('.mp4')?'deas-inputs/clip.mp4':'deas-inputs/u1.png');
 const expectedMedia=pipeline==='v2v'?[{type:'video',file:'deas-inputs/clip.mp4',slot:0,mediaMode:'video'}]:[
  {type:'image',file:'deas-inputs/u1.png',slot:0},
  {type:'video',file:'deas-inputs/clip.mp4',slot:0,mediaMode:'video',trimStart:1,trimEnd:4.5}];
 const expectedRefmods=[{slot:1,name:'chars/hero',strength:0.6,description:'the hero'}];
 let n=0;
 vi.mocked(runWorkbenchJob).mockImplementation(async()=>({id:`h3-${++n}`,baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'queued',outputs:[],error:null,generationReady:false}));
 let peerObserved=false;
 vi.mocked(pollWorkbench).mockImplementation(async()=>{
  const submitted=await Promise.all(vi.mocked(runWorkbenchJob).mock.results.map(async result=>await result.value));
  if(submitted.at(-1)?.id==='h3-2'&&!peerObserved){peerObserved=true;return collection([heldJob('unrelated-success','succeeded'),heldJob('unrelated-failure','failed'),...submitted]);}
  return collection([heldJob('unrelated-success','succeeded'),heldJob('unrelated-failure','failed'),...submitted.map(job=>({...job,state:'succeeded' as const}))]);
 });
 vi.mocked(readH3Session).mockImplementation(async()=>({latestId:null,clips:[{clipId:'chosen',parent:null,frames:124,completed:0,export:null},{clipId:'backend-import',parent:null,frames:192,completed:0,export:null,importedVideo:{sourceId:videoId,width:1024,height:768,modeFamily:ref2va?'ref2va':'fl2va'}},{clipId:'root1',parent:video?'backend-import':selected?'chosen':null,frames:124,completed:1,export:null},{clipId:'kid1',parent:'root1',frames:200,completed:2,export:null}]}));
 const view=render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 fireEvent.click(await screen.findByRole('button',{name:'ref.png'}));adopt();
 await waitFor(()=>expect(pipe('Image → Video').disabled).toBe(false));
 choosePipe(({t2v:'Text → Video',i2v:'Image → Video',v2v:'Video → Video',ref2v:'Multi-reference → Video'} as Record<string,string>)[pipeline]);
 const ref=screen.getByRole('img',{name:'ref.png'});
 Object.defineProperties(ref,{naturalWidth:{value:1024},naturalHeight:{value:768}});fireEvent.load(ref);
 if(pipeline==='v2v'){
  fireEvent.click(screen.getByRole('button',{name:'clip.mp4'}));adopt();loadVideo('clip.mp4');
  fireEvent.change(screen.getByLabelText('Width'),{target:{value:'1024'}});
  fireEvent.change(screen.getByLabelText('Height'),{target:{value:'768'}});
 }
 if(pipeline==='ref2v'){
  fireEvent.click(menuOf('ref.png').getByRole('menuitem',{name:'Add as reference'}));
  fireEvent.click(menuOf('clip.mp4').getByRole('menuitem',{name:'Add as reference'}));
  dock('References');
  fireEvent.click(screen.getByRole('button',{name:'Edit clip.mp4'}));
  fireEvent.change(screen.getByLabelText('Start of clip.mp4'),{target:{value:'1'}});
  fireEvent.change(screen.getByLabelText('End of clip.mp4'),{target:{value:'4.5'}});
  fireEvent.click(within(screen.getByRole('dialog',{name:'Edit clip.mp4'})).getByRole('button',{name:'Close'}));
  vi.mocked(readRefMods).mockResolvedValue([{name:'chars/hero',kinds:['image'],description:'the hero'}]);
  fireEvent.click(screen.getByRole('button',{name:'Read RefMod library'}));
  fireEvent.click(await screen.findByRole('button',{name:'Add RefMod'}));
  fireEvent.change(screen.getByLabelText('Strength of chars/hero'),{target:{value:'0.6'}});
 }
 fireEvent.change(screen.getByLabelText('Resolution'),{target:{value:'fixed'}});
 fireEvent.change(screen.getByLabelText('Width'),{target:{value:'1024'}});
 fireEvent.change(screen.getByLabelText('Height'),{target:{value:'768'}});
 expect(screen.getByRole('button',{name:'Continuity · off'})).toBeTruthy();
 expect((screen.getByLabelText('FPS') as HTMLInputElement).disabled).toBe(true);
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 if(pipeline==='t2v')expect(uploadInput).not.toHaveBeenCalled();
 else expect(uploadInput).toHaveBeenCalledWith('http://127.0.0.1:7801',`/env/ComfyUI/input/${pipeline==='v2v'?'clip.mp4':'ref.png'}`);
 const first=vi.mocked(runWorkbenchJob).mock.calls[0][1] as unknown as {frames:number;h3:object;firstFrame?:string};
 expect(first).toMatchObject({family:'minimax-h3',frames:124,frameRate:24,h3:{mode,references:pipeline==='i2v'?[{slot:0,image:'deas-inputs/u1.png'}]:[]}});
 if(ref2va)expect(first).toMatchObject({h3:{media:expectedMedia}});
 if(pipeline==='ref2v')expect(first).toMatchObject({h3:{refmods:expectedRefmods}});
 expect(first.firstFrame).toBeUndefined();
 expect((first.h3 as {continuity?:object}).continuity).toBeUndefined();
 expect(readH3Session).not.toHaveBeenCalled();
 await waitFor(()=>expect(screen.getAllByText('Succeeded').length).toBeGreaterThan(0),{timeout:4000});
 if(pipeline==='v2v'&&selected){
  fireEvent.click(document.querySelector('[data-preview-close]')!);
  expect(document.querySelector('[data-editor-viewport]')).toBeNull();
  expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(true);
 }
 fireEvent.click(screen.getByRole('button',{name:'Continuity · off'}));
 fireEvent.click(screen.getByRole('checkbox',{name:'Continuity active'}));
 fireEvent.change(screen.getByRole('textbox',{name:'Session name'}),{target:{value:'Named_session-1'}});
 fireEvent.click(screen.getByRole('button',{name:'Use session'}));
 if(video){
  fireEvent.change(screen.getByRole('textbox',{name:'Video in ComfyUI input'}),{target:{value:'clips/source.mp4'}});
  fireEvent.click(screen.getByRole('button',{name:'Prepare video'}));
  fireEvent.click(await screen.findByRole('button',{name:'Use video'}));
  expect(prepareH3Video).toHaveBeenCalledWith('http://127.0.0.1:7801','clips/source.mp4');
  expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
 }else if(selected){
  fireEvent.click(screen.getByRole('button',{name:'Load checkpoints'}));
  await screen.findByRole('option',{name:'chosen'});
  fireEvent.change(screen.getByRole('combobox',{name:'Source checkpoint'}),{target:{value:'chosen'}});
  expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
 }
 fireEvent.change(screen.getByRole('combobox',{name:'Output'}),{target:{value:outputMode}});
 if(useReferences)fireEvent.click(screen.getByRole('checkbox',{name:'Use current references for continuations'}));
 fireEvent.click(screen.getByRole('button',{name:'Add step'}));fireEvent.click(screen.getByRole('button',{name:'Add step'}));
 if(ref2va)fireEvent.change(document.querySelector('[data-step="0"] input[type="number"]')!,{target:{value:'6'}});
 const uploadsBeforePlan=vi.mocked(uploadInput).mock.calls.length;
 fireEvent.change(screen.getAllByLabelText('Next action')[0],{target:{value:'she turns around'}});
 fireEvent.change(screen.getByRole('combobox',{name:'Overlap (frames)'}),{target:{value:'56'}});
 fireEvent.click(screen.getByRole('button',{name:'Run all'}));
 expect((screen.getByRole('combobox',{name:'Overlap (frames)'}) as HTMLSelectElement).disabled).toBe(true);
 fireEvent.click(within(screen.getByRole('dialog',{name:'Continuity'})).getByRole('button',{name:'Close'}));
 expect(screen.queryByRole('dialog',{name:'Continuity'})).toBeNull();
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(3),{timeout:5000});
 fireEvent.click(screen.getByRole('button',{name:'Continuity'}));
 const steps=vi.mocked(runWorkbenchJob).mock.calls.slice(1).map(c=>(c[1] as unknown as {h3:{continuity:{sourceId:string;action:string;capture:boolean}}}).h3.continuity);
 expect(steps[0]).toMatchObject({session:'Named_session-1',sourceId:video?videoId:selected?'chosen':'',capture:true,action:selected?'she turns around':'',outputMode});
 if(video)expect(steps[0]).toMatchObject({sourceKind:'video',overlapFrames:56});
 if(selected)expect(steps[0]).toMatchObject({overlapFrames:56});
 expect(Object.hasOwn(steps[0],'overlapFrames')).toBe(selected);
 expect(Object.hasOwn(steps[0],'sourceKind')).toBe(video);
 expect(Object.hasOwn(steps[1],'sourceKind')).toBe(false);
 expect(steps[1]).toMatchObject({session:'Named_session-1',sourceId:'root1',capture:true,action:selected?'':'she turns around',outputMode,overlapFrames:56});
 const planned=vi.mocked(runWorkbenchJob).mock.calls.slice(1).map(call=>call[1] as unknown as {h3:{mode:string;media?:object[];refmods?:object[];continuity:{useReferences?:boolean}}});
 for(const [index,intent] of planned.entries()){
  expect(intent.h3.mode).toBe(mode);
  if(ref2va){
   if(selected||index>0)expect(intent.h3.continuity).toHaveProperty('durationSeconds',index===0?6:5);
   else expect(intent.h3.continuity).not.toHaveProperty('durationSeconds');
   const keepRefs=!selected&&index===0||useReferences;
   if(useReferences&&(selected||index>0))expect(intent.h3.continuity.useReferences).toBe(true);
   else expect(intent.h3.continuity).not.toHaveProperty('useReferences');
   expect(intent.h3.media).toEqual(keepRefs?expectedMedia:[]);
   if(pipeline==='ref2v'&&keepRefs)expect(intent.h3.refmods).toEqual(expectedRefmods);
   else expect(intent.h3).not.toHaveProperty('refmods');
  }
 }
 if(ref2va)expect(uploadInput).toHaveBeenCalledTimes(uploadsBeforePlan+(useReferences?2:!selected?1:0)*(pipeline==='ref2v'?2:1));
 expect((screen.getByRole('combobox',{name:'Overlap (frames)'}) as HTMLSelectElement).value).toBe('56');
 await waitFor(()=>expect(screen.getByText('kid1')).toBeTruthy(),{timeout:5000});
 expect(readH3Session).toHaveBeenCalledWith('http://127.0.0.1:7801/','Named_session-1');
 expect(peerObserved).toBe(true);
 const pinned=JSON.parse(localStorage.getItem('deas.pref.studio.run.continuity')!);
 expect(pinned.steps.filter((s:{jobId?:string})=>s.jobId).map((s:{jobId:string})=>s.jobId)).toEqual(['h3-2','h3-3']);
 expect(pinned.steps.map((s:{clipId?:string})=>s.clipId)).toEqual(['root1','kid1']);
 expect((screen.getByRole('textbox',{name:'Session name'}) as HTMLInputElement).value).toBe('Named_session-1');
 if(selected){
  fireEvent.click(screen.getByRole('button',{name:'Add step'}));
  expect((screen.getByRole('button',{name:'Run next'}) as HTMLButtonElement).disabled).toBe(false);
  view.rerender(<I18nProvider><Workbench endpoint="http://127.0.0.1:7802" roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
  expect((screen.getByRole('button',{name:'Run next'}) as HTMLButtonElement).disabled).toBe(true);
  expect(screen.getByRole('alert').textContent).toContain('Backend changed');
  expect(runWorkbenchJob).toHaveBeenCalledTimes(3);
 }
},10000);

test('pending video upload is dropped on unmount before native admission',async()=>{
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:mediaEntries,truncated:false}));
 let release!:(name:string)=>void;
 vi.mocked(uploadInput).mockReturnValueOnce(new Promise(yes=>{release=yes;}));
 vi.mocked(runWorkbenchJob).mockImplementation(async(baseUrl,intent)=>{
  const job={...heldJob('late-upload-job','queued',baseUrl),intent};
  vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
  vi.mocked(pollWorkbench).mockResolvedValue(collection([job]));
  return job;
 });
 const props={endpoint:'http://127.0.0.1:7801',roots:{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}};
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Video → Video').disabled).toBe(false));choosePipe('Video → Video');
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 fireEvent.click(await screen.findByRole('button',{name:'clip.mp4'}));adopt();loadVideo('clip.mp4');
 await waitFor(()=>expect((document.querySelector('[data-run]') as HTMLButtonElement).disabled).toBe(false));
 fireEvent.click(document.querySelector('[data-run]')!);
 await waitFor(()=>expect(uploadInput).toHaveBeenCalledTimes(1));
 view.unmount();localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 await waitFor(()=>expect(readCurrentWorkbench).toHaveBeenCalledTimes(2));
 await waitFor(()=>expect((document.querySelector('[data-run]') as HTMLButtonElement).disabled).toBe(false));
 await act(async()=>release('deas-inputs/late.mp4'));
 expect(runWorkbenchJob).not.toHaveBeenCalled();
 expect(document.querySelectorAll('[data-job-id]')).toHaveLength(0);
});

test('delayed video upload submits the captured endpoint, settings, naming and source, never a rebuilt draft',async()=>{
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:mediaEntries,truncated:false}));
 let release!:(name:string)=>void;
 vi.mocked(uploadInput).mockReturnValueOnce(new Promise(yes=>{release=yes;}));
 vi.mocked(runWorkbenchJob).mockImplementation(async(baseUrl,intent)=>({...heldJob('captured','failed',baseUrl),intent}));
 localStorage.setItem('deas.pref.studio.run.seed','42');localStorage.setItem('deas.pref.studio.run.seedMode','"fixed"');
 localStorage.setItem('deas.pref.studio.run.options',JSON.stringify({...defaultOptions,outputNaming:'saved/%seed%'}));
 const props={endpoint:'http://127.0.0.1:7801',roots:{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}};
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Video → Video').disabled).toBe(false));choosePipe('Video → Video');
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 fireEvent.click(await screen.findByRole('button',{name:'clip.mp4'}));adopt();loadVideo('clip.mp4');
 fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'/env/ComfyUI/input/clip.mp4'}});
 fireEvent.change(screen.getByLabelText('Source strength'),{target:{value:'0.7'}});
 await waitFor(()=>expect((document.querySelector('[data-run]') as HTMLButtonElement).disabled).toBe(false));
 fireEvent.click(document.querySelector('[data-run]')!);
 await waitFor(()=>expect(uploadInput).toHaveBeenCalledTimes(1));
 fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'new prompt'}});
 fireEvent.change(screen.getByLabelText('Source strength'),{target:{value:'0.2'}});
 fireEvent.change(screen.getByLabelText('Steps'),{target:{value:'35'}});
 view.rerender(<I18nProvider><Workbench {...props} endpoint="http://changed:9999/"/></I18nProvider>);
 expect(runWorkbenchJob).not.toHaveBeenCalled();
 await act(async()=>release('deas-inputs/captured.mp4'));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.calls[0]).toEqual(['http://127.0.0.1:7801',expect.objectContaining({positivePrompt:'/env/ComfyUI/input/clip.mp4',seed:42,steps:8,outputPrefix:'raw/saved/42',sourceVideo:'deas-inputs/captured.mp4',ltx:expect.objectContaining({sourceStrength:0.7})}),0]);
});
const ltxModels=[
 {category:'unet',name:'LTX2/ltx.safetensors',path:'/m/unet/LTX2/ltx.safetensors',size:1,info:null,family:'ltx-2'},
 {category:'text_encoders',name:'gemma_3_12B_it_fp8.safetensors',path:'/m/text_encoders/gemma.safetensors',size:1,info:null},
 {category:'text_encoders',name:'ltx-2.3_text_projection_bf16.safetensors',path:'/m/text_encoders/proj.safetensors',size:1,info:null},
 {category:'vae',name:'LTX2/LTX23_video_vae_bf16.safetensors',path:'/m/vae/v.safetensors',size:1,info:null},
 {category:'vae',name:'LTX2/LTX23_audio_vae_bf16.safetensors',path:'/m/vae/a.safetensors',size:1,info:null}] as const;
const mediaEntries=[
 {name:'ref.png',path:'/env/ComfyUI/input/ref.png',kind:'file',symlink:false,media:'image',size:1,modified:1},
 {name:'clip.mp4',path:'/env/ComfyUI/input/clip.mp4',kind:'file',symlink:false,media:'video',size:1,modified:2},
 {name:'voice.wav',path:'/env/ComfyUI/input/voice.wav',kind:'file',symlink:false,media:'audio',size:1,modified:3},
 {name:'a2.wav',path:'/env/ComfyUI/input/a2.wav',kind:'file',symlink:false,media:'audio',size:1,modified:4},
 {name:'a3.wav',path:'/env/ComfyUI/input/a3.wav',kind:'file',symlink:false,media:'audio',size:1,modified:5}];
/** Right-click a Media dock tile and return its menu. */
const menuOf=(name:string)=>{fireEvent.contextMenu(screen.getByRole('button',{name}),{clientX:5,clientY:5});return within(screen.getByRole('menu',{name}));};
const markOf=(name:string)=>screen.getByRole('button',{name}).closest('li')!.dataset.mark;
const loadVideo=(name:string)=>{const v=document.querySelector<HTMLVideoElement>(`.wb-stage video`)!;Object.defineProperties(v,{videoWidth:{value:1280},videoHeight:{value:704}});fireEvent.loadedMetadata(v);void name;};

test.each(['Image → Video','Multi-reference → Video'])('every right-click media action is mirrored in the preview toolbar (%s)',async pipeline=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'diffusion_models',name:'MiniMaxH3/h3_ref2va.safetensors',path:'/m/h3.safetensors',size:1,info:null,family:'minimax-h3'}]);
 vi.mocked(invoke).mockImplementation(async command=>command==='native_media_star_status'?{starred:false,unavailable:null}:{path:'/outside',parent:'/',entries:[{name:'pic.png',path:'/outside/pic.png',kind:'file',media:'image',symlink:false,size:1,modified:1}],truncated:false});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',input:'/input',output:'/out'}}/></I18nProvider>);
 await waitFor(()=>expect(pipe(pipeline).disabled).toBe(false));choosePipe(pipeline);
 fireEvent.click(await screen.findByRole('button',{name:'pic.png'}));
 const menu=menuOf('pic.png').getAllByRole('menuitem').map(b=>b.firstChild?.textContent??'').filter(label=>label!=="Open in folder");
 fireEvent.keyDown(document.activeElement??document.body,{key:'Escape'});
 const toolbar=screen.getByRole('region',{name:'Preview'}).querySelector<HTMLElement>(':scope > [data-media-tools]')!;
 expect(menu.length).toBeGreaterThan(0);
 expect([...toolbar.querySelectorAll('[data-media-action]')].map(b=>b.textContent)).toEqual(menu);
});
test.each([['video','clip.mp4'],['audio','voice.wav']] as const)('external %s reference preview has one stage toolbar and no figure toolbar',async(media,name)=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'diffusion_models',name:'MiniMaxH3/h3_ref2va.safetensors',path:'/m/h3.safetensors',size:1,info:null,family:'minimax-h3'}]);
 vi.mocked(invoke).mockImplementation(async command=>command==='native_media_star_status'?{starred:false,unavailable:null}:{path:'/outside',parent:'/',entries:[{name,path:'/outside/'+name,kind:'file',media,symlink:false,size:1,modified:1}],truncated:false});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',input:'/input',output:'/out'}}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Multi-reference → Video').disabled).toBe(false));choosePipe('Multi-reference → Video');
 fireEvent.click(await screen.findByRole('button',{name}));adopt();
 const stage=screen.getByRole('region',{name:'Preview'});
 expect(stage.querySelector('[data-browsed-preview]')).toBeNull();
 expect(stage.querySelector(`.wb-current-preview ${media}`)).not.toBeNull();
 expect(stage.querySelectorAll('[data-media-tools]')).toHaveLength(1);
 expect(stage.querySelector('.wb-current-preview figure [data-media-tools]')).toBeNull();
 const toolbar=stage.querySelector<HTMLElement>(':scope > [data-media-tools]')!;
 expect(within(toolbar).getByRole('button',{name:'Use as input'})).toBeTruthy();
 expect(within(stage).getAllByRole('button',{name:`Star ${name}`})).toHaveLength(1);
 expect(within(toolbar).getByRole('button',{name:'Add as reference'})).toBeTruthy();
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
});

test.each([
 ['Image → Video','ref.png','clip.mp4','Use as first frame'],
 ['Video → Video','clip.mp4','ref.png','Use as source video'],
])('Media context menu selects the clicked %s input without preview adoption or execution',async(pipeline,name,other,label)=>{
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:mediaEntries,truncated:false}));
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
 await waitFor(()=>expect(pipe(pipeline).disabled).toBe(false));choosePipe(pipeline);
 await screen.findByRole('button',{name});
 expect(menuOf(other).queryByRole('menuitem',{name:label})).toBeNull();
 fireEvent.keyDown(window,{key:'Escape'});
 fireEvent.click(screen.getByRole('button',{name:other}));
 expect(document.querySelector('[data-browsed-preview]')).not.toBeNull();
 fireEvent.click(menuOf(name).getByRole('menuitem',{name:label}));
 expect(screen.queryByRole('menu')).toBeNull();
 expect(document.querySelector('[data-browsed-preview]')).toBeNull();
 expect([...document.querySelectorAll('.wb-active')].some(el=>el.textContent?.includes(name))).toBe(true);
 expect(uploadInput).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test.each(['REF2VA','FL2VA'] as const)('HyperFlow %s follows the global pipeline and submits its own reference mode',async mode=>{
 const name=`MiniMaxH3/minimax_h3_${mode.toLowerCase()}_hyperflow_flash_14b-Q8_0.gguf`;
 vi.mocked(scanModels).mockResolvedValue([
  {category:'diffusion_models',name,path:'/m/'+name,size:1,info:null,family:'minimax-h3-hyperflow'},
  {category:'text_encoders',name:'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors',path:'/m/t',size:1,info:null},
  {category:'vae',name:'minimax_h3_video_vae_fp16.safetensors',path:'/m/v',size:1,info:null},
  {category:'vae',name:'minimax_h3_audio_vae_fp32.safetensors',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,
  nodes:[...evidence.allowlist.nodes,'HyperFlowGGUFLoader','HyperFlowSigmas','CLIPLoader','VAELoader'],
  ggufDiffusionModels:[name],textEncoders:['qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors'],clipTypes:['minimax'],
  vaes:['minimax_h3_video_vae_fp16.safetensors','minimax_h3_audio_vae_fp32.safetensors']}});
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string})?.path,parent:'/env',entries:mediaEntries,truncated:false}));
 vi.mocked(uploadInput).mockResolvedValue('deas-inputs/u.png');
 vi.mocked(runWorkbenchJob).mockResolvedValue({...heldJob('hyperflow','failed'),error:'fixture'});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',input:'/env/ComfyUI/input'}}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Text → Video').disabled).toBe(false));
 choosePipe('Text → Video');
 // A checkpoint's name is not a request to change the user's data pipeline.
 expect(document.querySelector('[data-open-references]')).toBeNull();
 expect(screen.getByLabelText('Pipeline')).toHaveProperty('value','t2v');
 expect(pipe('Video → Video').disabled).toBe(true);
 choosePipe(mode==='REF2VA'?'Multi-reference → Video':'Image → Video');
 fireEvent.change(screen.getByLabelText('Aspect ratio'),{target:{value:'4:3'}});
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 await screen.findByRole('button',{name:'ref.png'});
 if(mode==='REF2VA'){
  expect(document.querySelector('[data-open-references]')).toBeTruthy();
  expect(menuOf('clip.mp4').getByRole('menuitem',{name:/^Add as reference/})).toHaveProperty('disabled',true);
  fireEvent.keyDown(window,{key:'Escape'});
  fireEvent.click(menuOf('ref.png').getByRole('menuitem',{name:'Add as reference'}));
  fireEvent.click(document.querySelector('[data-open-references]')!);
  const dialog=screen.getByRole('dialog',{name:'References'});
  expect(within(dialog).getByRole('region',{name:'Images'})).toBeTruthy();
  expect(within(dialog).getByText('Images 1/9')).toBeTruthy();
  expect(within(dialog).queryByText(/Select images, videos or audio/)).toBeNull();
  expect(within(dialog).queryByRole('region',{name:'Videos'})).toBeNull();
  expect(within(dialog).queryByRole('region',{name:'Audio'})).toBeNull();
  for(const [name,media] of [['clip.mp4','video'],['voice.wav','audio']]){
   const item={name,media,path:`/env/ComfyUI/input/${name}`,src:`asset://${name}`};
   fireEvent.drop(document.querySelector('[data-references]')!,{dataTransfer:{types:['application/x-deas-media'],getData:()=>JSON.stringify(item)}});
   expect(within(dialog).getByRole('alert').textContent).toContain('HyperFlow GGUF accepts image references only.');
   expect(document.querySelectorAll('[data-ref]')).toHaveLength(1);
  }
  fireEvent.click(within(dialog).getByRole('button',{name:'Close'}));
 }else{
  expect(document.querySelector('[data-open-references]')).toBeNull();
  fireEvent.click(menuOf('ref.png').getByRole('menuitem',{name:'Use as first and last frame'}));
  const image=screen.getByRole('img',{name:'ref.png'});
  Object.defineProperties(image,{naturalWidth:{value:1024},naturalHeight:{value:768}});fireEvent.load(image);
 }
 fireEvent.change(screen.getByLabelText('Resolution'),{target:{value:'fixed'}});
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.lastCall![1]).toMatchObject({family:'minimax-h3-hyperflow',model:name,h3:mode==='REF2VA'
  ?{mode,references:[],media:[{type:'image',file:'deas-inputs/u.png',slot:0}]}
  :{mode,references:[{slot:0,image:'deas-inputs/u.png'},{slot:1,image:'deas-inputs/u.png'}]}});
});

test('Image → Video media menu assigns first, last or both frame roles',async()=>{
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:mediaEntries,truncated:false}));
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Image → Video').disabled).toBe(false));choosePipe('Image → Video');
 await screen.findByRole('button',{name:'ref.png'});
 const last=()=>[...document.querySelectorAll('.wb-resolution-hint')].find(el=>el.textContent?.startsWith('Last frame:'))?.textContent??'';
 fireEvent.click(menuOf('ref.png').getByRole('menuitem',{name:'Use as first and last frame'}));
 expect([...document.querySelectorAll('.wb-active')].some(el=>el.textContent?.includes('ref.png'))).toBe(true);
 expect(last()).toContain('ref.png');
 // The image is now the last frame, so the menu offers removal; "first frame" alone drops the last role.
 expect(menuOf('ref.png').getByRole('menuitem',{name:'Remove last frame'})).toBeTruthy();
 fireEvent.click(menuOf('ref.png').getByRole('menuitem',{name:'Use as first frame'}));
 expect(last()).toBe('');
 fireEvent.click(menuOf('ref.png').getByRole('menuitem',{name:'Use as last frame'}));
 expect(last()).toContain('ref.png');
 expect(uploadInput).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('MiniMax H3 Image → Video runs a last frame alone (L2VA) and badges frame roles in Media',async()=>{
 vi.mocked(scanModels).mockResolvedValue([
  {category:'diffusion_models',name:'MiniMaxH3/h3.safetensors',path:'/m/diffusion_models/MiniMaxH3/h3.safetensors',size:1,info:null,family:'minimax-h3'},
  {category:'text_encoders',name:'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors',path:'/m/t.safetensors',size:1,info:null},
  {category:'vae',name:'MiniMaxH3/minimax_h3_video_vae_fp16.safetensors',path:'/m/v.safetensors',size:1,info:null},
  {category:'vae',name:'MiniMaxH3/minimax_h3_audio_vae_fp32.safetensors',path:'/m/a.safetensors',size:1,info:null}]);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:mediaEntries,truncated:false}));
 vi.mocked(uploadInput).mockResolvedValue('deas-inputs/u1.png');
 vi.mocked(runWorkbenchJob).mockResolvedValue({...heldJob('plain','failed'),error:'fixture'});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 await screen.findByRole('button',{name:'ref.png'});
 await waitFor(()=>expect(pipe('Image → Video').disabled).toBe(false));choosePipe('Image → Video');
 const badge=()=>screen.getByRole('button',{name:'ref.png'}).closest('li')!.getAttribute('data-mark');
 fireEvent.click(menuOf('ref.png').getByRole('menuitem',{name:'Use as last frame'}));
 expect(badge()).toBe('Last frame');
 expect(document.querySelector('[data-frame-people="first"]')).toBeNull();
 const img=screen.getByRole('img',{name:'ref.png'});
 Object.defineProperties(img,{naturalWidth:{value:1024},naturalHeight:{value:768}});fireEvent.load(img);
 fireEvent.change(screen.getByLabelText('Resolution'),{target:{value:'fixed'}});
 fireEvent.change(screen.getByLabelText('Width'),{target:{value:'1024'}});
 fireEvent.change(screen.getByLabelText('Height'),{target:{value:'768'}});
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',false));fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.lastCall![1]).toMatchObject({family:'minimax-h3',h3:{mode:'L2VA',references:[{slot:1,image:'deas-inputs/u1.png'}]}});
 fireEvent.click(menuOf('ref.png').getByRole('menuitem',{name:'Use as first frame'}));
 expect(badge()).toBe('First frame');
 fireEvent.click(menuOf('ref.png').getByRole('menuitem',{name:'Use as first and last frame'}));
 expect(badge()).toBe('First frame + Last frame');
});

test('IPC fixture: LTX Video → Video uploads the source video; Multi-reference → Video sends image keyframes at their seconds',async()=>{
 // jsdom never loads media: the first reference's size is what Auto aspect waits for.
 vi.stubGlobal('Image',class{naturalWidth=1024;naturalHeight=640;onload?:()=>void;set src(_:string){queueMicrotask(()=>this.onload?.());}});
 vi.spyOn(HTMLMediaElement.prototype,'load').mockImplementation(()=>{});
 Object.defineProperty(HTMLVideoElement.prototype,'src',{configurable:true,set(this:HTMLVideoElement){queueMicrotask(()=>{Object.defineProperties(this,{videoWidth:{value:1024},videoHeight:{value:640}});this.onloadedmetadata?.(new Event('loadedmetadata'));});}});
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:mediaEntries,truncated:false}));
 vi.mocked(uploadInput).mockImplementation(async(_b,path)=>path.endsWith('.mp4')?'deas-input-u.mp4':'deas-inputs/u.png');
 vi.mocked(runWorkbenchJob).mockImplementation(async()=>({id:'job-v',baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'queued',outputs:[],error:null,generationReady:false} as JobStatus));
 vi.mocked(pollWorkbench).mockImplementation(async()=>collection(await Promise.all(vi.mocked(runWorkbenchJob).mock.results.map(async result=>({...await result.value,state:'succeeded' as const})))));
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Video → Video').disabled).toBe(false));
 expect(pipe('Multi-reference → Video').disabled).toBe(false);
 choosePipe('Video → Video');
 expect(within(screen.getByRole('region',{name:'Preview'})).getByRole('status').textContent).toBe('Pick a video from the Media dock as the source.');
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 await screen.findByRole('button',{name:'clip.mp4'});
 expect(menuOf('ref.png').queryByRole('menuitem',{name:'Use as source video'})).toBeNull();fireEvent.keyDown(window,{key:'Escape'});
 fireEvent.click(menuOf('clip.mp4').getByRole('menuitem',{name:'Use as source video'}));
 loadVideo('clip.mp4');
 expect(markOf('clip.mp4')).toBe('Source video');
 expect(document.querySelector('.wb-stage-mark')?.textContent).toBe('Source video');
 fireEvent.change(screen.getByLabelText('Source strength'),{target:{value:'0.7'}});
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(uploadInput).toHaveBeenCalledWith('http://127.0.0.1:7801','/env/ComfyUI/input/clip.mp4');
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({family:'ltx-2',sourceVideo:'deas-input-u.mp4',width:1280,height:704,ltx:{sourceStrength:0.7}});
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).not.toHaveProperty('firstFrame');
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false),{timeout:5000});
 // Multi-reference: images only for LTX (no reference button on a video), placed at a second with a strength.
 choosePipe('Multi-reference → Video');
 expect(screen.queryByRole('button',{name:'Add as reference'})).toBeNull();
 expect(screen.getByText('Add at least one reference with its “Add as reference” button.')).toBeTruthy();
 const blocked=menuOf('clip.mp4').getByRole('menuitem',{name:/Add as reference/}) as HTMLButtonElement;
 expect(blocked.disabled).toBe(true);expect(blocked.textContent).toContain('LTX takes images as references only.');
 fireEvent.keyDown(window,{key:'Escape'});
 fireEvent.click(screen.getByRole('button',{name:'ref.png'}));adopt();
 const img=await screen.findByRole('img',{name:'ref.png'});
 Object.defineProperties(img,{naturalWidth:{value:1280},naturalHeight:{value:704}});fireEvent.load(img);
 fireEvent.click(screen.getAllByRole('button',{name:'Add as reference'})[0]);
 expect(markOf('ref.png')).toBe('@ 0 s');
 fireEvent.click(document.querySelector('[data-open-references]')!);
 expect(screen.getByRole('dialog',{name:'References'})).toBeTruthy();
 // A dock video dropped on the tab is refused with the reason (LTX references are images).
 const clip={name:'clip.mp4',path:'/env/ComfyUI/input/clip.mp4',media:'video',src:'asset://localhost//env/ComfyUI/input/clip.mp4'};
 fireEvent.drop(document.querySelector('[data-references]')!,{dataTransfer:{types:['application/x-deas-media'],getData:()=>JSON.stringify(clip)}});
 expect(within(screen.getByRole('dialog',{name:'References'})).getByRole('alert').textContent).toBe('clip.mp4: LTX takes images as references only.');
 // The References tab button takes dock drops too (the Media tab is the drag source).
 const voice={name:'voice.wav',path:'/env/ComfyUI/input/voice.wav',media:'audio',src:'asset://localhost//env/ComfyUI/input/voice.wav'};
 fireEvent.drop(document.querySelector('[data-open-references]')!,{dataTransfer:{types:['application/x-deas-media'],getData:()=>JSON.stringify(voice)}});
 expect(within(screen.getByRole('dialog',{name:'References'})).getByRole('alert').textContent).toBe('voice.wav: LTX takes images as references only.');
 fireEvent.change(screen.getByLabelText('Second of ref.png'),{target:{value:'1'}});
 fireEvent.change(screen.getByLabelText('Strength of ref.png'),{target:{value:'0.5'}});
 expect(screen.queryByRole('button',{name:/Insert/})).toBeNull(); // Director labels are H3-only
 dock('Media');
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 await screen.findByRole('button',{name:'ref.png'});
 expect(markOf('ref.png')).toBe('@ 1 s');
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(2));
 const second=vi.mocked(runWorkbenchJob).mock.calls[1][1];
 expect(second).toMatchObject({family:'ltx-2',ltx:{keyframes:[{image:'ref.png',frame:24,strength:0.5}]}});
 expect(second).not.toHaveProperty('sourceVideo');
 fireEvent.click(menuOf('ref.png').getByRole('menuitem',{name:'Remove reference ref.png'}));
 expect(markOf('ref.png')).toBeUndefined();
 // Leaving Multi-reference hides the References tab and falls back to Media.
 fireEvent.click(document.querySelector('[data-open-references]')!);dock('Media');choosePipe('Video → Video');
 expect(screen.queryByRole('tab',{name:'References'})).toBeNull();
 await waitFor(()=>expect(screen.getByRole('tab',{name:'Media'}).getAttribute('aria-selected')).toBe('true'));
});

test('IPC fixture: MiniMax H3 runs Video → Video and Multi-reference → Video as REF2VA timeline media',async()=>{
 // jsdom never loads media: the first reference's size is what Auto aspect waits for.
 vi.stubGlobal('Image',class{naturalWidth=1024;naturalHeight=640;onload?:()=>void;set src(_:string){queueMicrotask(()=>this.onload?.());}});
 vi.spyOn(HTMLMediaElement.prototype,'load').mockImplementation(()=>{});
 Object.defineProperty(HTMLVideoElement.prototype,'src',{configurable:true,set(this:HTMLVideoElement){queueMicrotask(()=>{Object.defineProperties(this,{videoWidth:{value:1024},videoHeight:{value:640}});this.onloadedmetadata?.(new Event('loadedmetadata'));});}});
 vi.mocked(scanModels).mockResolvedValue([
  {category:'diffusion_models',name:'MiniMaxH3/h3_ref2va.safetensors',path:'/m/diffusion_models/MiniMaxH3/h3_ref2va.safetensors',size:1,info:null,family:'minimax-h3'},
  {category:'text_encoders',name:'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors',path:'/m/t.safetensors',size:1,info:null},
  {category:'vae',name:'MiniMaxH3/minimax_h3_video_vae_fp16.safetensors',path:'/m/v.safetensors',size:1,info:null},
  {category:'vae',name:'MiniMaxH3/minimax_h3_audio_vae_fp32.safetensors',path:'/m/a.safetensors',size:1,info:null}]);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>_c==='native_forge'?({models:[{id:'m',label:'M'}],errors:{},creativity:['x'],shot_counts:['Auto'],default_shots:'Auto'}):({path:(args as {path:string}).path,parent:'/env',entries:mediaEntries,truncated:false}));
 vi.mocked(uploadInput).mockImplementation(async(_b,path)=>path.endsWith('.mp4')?'deas-input-v.mp4':path.endsWith('.wav')?'deas-inputs/s.wav':'deas-inputs/i.png');
 vi.mocked(runWorkbenchJob).mockImplementation(async()=>({id:'job-h',baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'queued',outputs:[],error:null,generationReady:false} as JobStatus));
 vi.mocked(pollWorkbench).mockImplementation(async()=>collection(await Promise.all(vi.mocked(runWorkbenchJob).mock.results.map(async result=>({...await result.value,state:'succeeded' as const})))));
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" root="/env" roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Video → Video').disabled).toBe(false));
 choosePipe('Video → Video');
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 fireEvent.click(await screen.findByRole('button',{name:'clip.mp4'}));adopt();
 loadVideo('clip.mp4');
 fireEvent.click(screen.getByRole('checkbox',{name:'Use the source sound'}));
 expect(screen.queryByText('MiniMax H3 runs these as REF2VA; pick a REF2VA checkpoint for best results.')).toBeNull();
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({family:'minimax-h3',h3:{mode:'REF2VA',references:[],media:[{type:'video',file:'deas-input-v.mp4',slot:0,mediaMode:'video_audio'}]}});
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false),{timeout:5000});
 // Multi-reference: the video (sound only), an image and an audio clip; slots count per lane.
 choosePipe('Multi-reference → Video');
 fireEvent.click(screen.getByRole('button',{name:'clip.mp4'}));adopt();
 fireEvent.click(await screen.findByRole('button',{name:'Add as reference'}));
 fireEvent.click(document.querySelector('[data-open-references]')!);
 fireEvent.click(screen.getByRole('button',{name:'Edit clip.mp4'}));
 fireEvent.change(screen.getByLabelText('Use of clip.mp4'),{target:{value:'audio'}});
 fireEvent.click(within(screen.getByRole('dialog',{name:'Edit clip.mp4'})).getByRole('button',{name:'Close'}));
 dock('Media');
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 await screen.findByRole('button',{name:'voice.wav'});
 fireEvent.click(menuOf('voice.wav').getByRole('menuitem',{name:'Add as reference'}));
 fireEvent.click(screen.getByRole('button',{name:'ref.png'}));adopt();
 const img=await screen.findByRole('img',{name:'ref.png'});
 Object.defineProperties(img,{naturalWidth:{value:1024},naturalHeight:{value:768}});fireEvent.load(img);
 fireEvent.click(screen.getAllByRole('button',{name:'Add as reference'})[0]);
 expect(document.querySelector('.wb-stage-mark')?.textContent).toBe('<Picture 1>');
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(2));
 expect(vi.mocked(runWorkbenchJob).mock.calls[1][1]).toMatchObject({h3:{mode:'REF2VA',media:[
  {type:'video',file:'deas-input-v.mp4',slot:0,mediaMode:'audio'},{type:'audio',file:'deas-inputs/s.wav',slot:1},{type:'image',file:'deas-inputs/i.png',slot:0}]}});
 // Badges are the Director's prompt labels; the third sound track fills the audio lane, so a fourth is refused with the reason.
 expect([markOf('clip.mp4'),markOf('voice.wav'),markOf('ref.png')]).toEqual(['<Audio 1>','<Audio 2>','<Picture 1>']);
 fireEvent.click(menuOf('a2.wav').getByRole('menuitem',{name:'Add as reference'}));
 const full=menuOf('a3.wav').getByRole('menuitem',{name:/Add as reference/}) as HTMLButtonElement;
 expect(full.disabled).toBe(true);expect(full.textContent).toContain('Director limit reached');
 fireEvent.keyDown(window,{key:'Escape'});
 fireEvent.click(menuOf('voice.wav').getByRole('menuitem',{name:'Remove reference voice.wav'}));
 expect(markOf('voice.wav')).toBeUndefined();expect(markOf('a2.wav')).toBe('<Audio 2>');
 // References tab: a too-short trim blocks Run with the reason; a valid one reaches h3.media; labels go into the prompt.
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false),{timeout:5000});
 dock('References');
 expect(screen.getByText('Images 1/9 · Videos 0/3 · Sound 2/3 · Files 3/12')).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'Edit a2.wav'}));
 fireEvent.change(screen.getByLabelText('Start of a2.wav'),{target:{value:'1'}});
 fireEvent.change(screen.getByLabelText('End of a2.wav'),{target:{value:'2'}});
 expect(screen.getByRole('alert').textContent).toBe('A trimmed clip must last 2–15 s (end after start).');
 expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(true);
 fireEvent.change(screen.getByLabelText('End of a2.wav'),{target:{value:'4.5'}});
 fireEvent.click(within(screen.getByRole('dialog',{name:'Edit a2.wav'})).getByRole('button',{name:'Close'}));
 fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'a singer'}});
 fireEvent.click(screen.getByRole('button',{name:'Insert <Audio 2> into the prompt'}));
 // Director input scaling: the default is Fill and crop, and it is always sent (absent also means Fill and crop on older jobs).
 expect((vi.mocked(runWorkbenchJob).mock.calls[1][1] as unknown as {h3:{inputScaling?:string}}).h3.inputScaling).toBe('Fill and crop');
 fireEvent.change(document.querySelector('[data-h3-scaling]')!,{target:{value:'Fit and pad'}});
 // RefMods: read the backend library on demand, add one; it travels as a Director RefMod row.
 vi.mocked(readRefMods).mockResolvedValue([{name:'chars/hero',kinds:['image'],description:'the hero'}]);
 fireEvent.click(screen.getByRole('button',{name:'Read RefMod library'}));
 fireEvent.click(await screen.findByRole('button',{name:'Add RefMod'}));
 expect(readRefMods).toHaveBeenCalledWith('http://127.0.0.1:7801');
 fireEvent.change(screen.getByLabelText('Strength of chars/hero'),{target:{value:'0.6'}});
 expect(screen.getByText('Fit the whole reference and pad to the canvas.')).toBeTruthy();
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('a singer <Audio 2>');
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(3));
 expect(vi.mocked(runWorkbenchJob).mock.calls[2][1]).toMatchObject({h3:{media:[
  {type:'video',slot:0,mediaMode:'audio'},{type:'image',slot:0},{type:'audio',slot:1,trimStart:1,trimEnd:4.5}],inputScaling:'Fit and pad',refmods:[{slot:1,name:'chars/hero',strength:0.6,description:'the hero'}]}});
 expect((vi.mocked(runWorkbenchJob).mock.calls[2][1] as unknown as {h3:{media:object[]}}).h3.media[0]).not.toHaveProperty('trimStart');
 // Director prompt helpers live in the hammer panel: open it, pick the LLM step. Prefill fills the REF2VA sections around the written text (RefMod after the timeline media); [Shot N] numbers on.
 fireEvent.click(screen.getByRole('button',{name:'Prompt Forge'}));
 fireEvent.click(screen.getByRole('button',{name:'LLM · Prompt Forge'}));
 fireEvent.click(screen.getByRole('button',{name:'Prefill labels & summary'}));
 const prompt=()=>(screen.getByLabelText('Prompt') as HTMLTextAreaElement).value;
 expect(prompt()).toMatch(/^subject_definitions:\n<Picture 1> is an image reference.*\n<Audio 1> is the audio track selected from a video.*\n<Audio 2> is an audio reference.*\n<Picture 2> is a saved image reference \(RefMod 1\) User description: the hero\./);
 expect(prompt()).toContain('detailed_description:\na singer <Audio 2>');
 fireEvent.click(screen.getByRole('button',{name:'Insert [Shot N]'}));
 expect(prompt().endsWith('[Shot 1] ')).toBe(true);
});

test('IPC fixture: MiniMax H3 offers Image → Image as the Director-style edit: shifts instead of denoise, no refine pass',async()=>{
 vi.mocked(scanModels).mockResolvedValue([
  {category:'diffusion_models',name:'MiniMaxH3/h3.safetensors',path:'/m/diffusion_models/MiniMaxH3/h3.safetensors',size:1,info:null,family:'minimax-h3'},
  {category:'text_encoders',name:'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors',path:'/m/t.safetensors',size:1,info:null},
  {category:'vae',name:'MiniMaxH3/minimax_h3_video_vae_fp16.safetensors',path:'/m/v.safetensors',size:1,info:null},
  {category:'vae',name:'MiniMaxH3/minimax_h3_audio_vae_fp32.safetensors',path:'/m/a.safetensors',size:1,info:null}]);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:mediaEntries,truncated:false}));
 vi.mocked(runWorkbenchJob).mockImplementation(async()=>({id:'job-e',baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'queued',outputs:[],error:null,generationReady:false} as JobStatus));
 localStorage.setItem('deas.pref.studio.run.post',JSON.stringify(true)); // refine after every run is on, but H3 skips it
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Image → Image').disabled).toBe(false));
 choosePipe('Image → Image');
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 fireEvent.click(await screen.findByRole('button',{name:'ref.png'}));adopt();
 const img=await screen.findByRole('img',{name:'ref.png'});
 Object.defineProperties(img,{naturalWidth:{value:1024},naturalHeight:{value:768}});fireEvent.load(img);
 expect(screen.queryByLabelText('Denoise')).toBeNull();
 // H3 has no refine pass: More keeps the Refine entry, disabled with its reason.
 expect(screen.getByRole('button',{name:'Refine'})).toMatchObject({disabled:true,title:expect.stringContaining('MiniMax H3')});
 fireEvent.change(screen.getByLabelText('Shift video'),{target:{value:'11'}});
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 const intent=vi.mocked(runWorkbenchJob).mock.calls[0][1];
 expect(intent).toMatchObject({recipeId:'checkpoint-text-to-image-diagnostic',checkpoint:'MiniMaxH3/h3.safetensors',inputImage:'ref.png',diffusion:{clipType:'minimax'},h3Edit:{shiftVideo:11,shiftAudio:3}});
 expect(intent).not.toHaveProperty('refine');
});


test.each(['image','ltx-2','wan','minimax-h3'] as const)('Run Plan uses the exact next Run intent without preparing, uploading or losing draft state (%s)',async family=>{
 const m=(category:string,name:string,f?:string)=>({category,name,path:`/m/${category}/${name}`,size:1,info:null,...(f?{family:f}:{})});
 const list=family==='image'?[m('checkpoints','base.safetensors','sdxl'),m('loras','style.safetensors','sdxl')]
  :family==='ltx-2'?[m('unet','ltx.safetensors',family),m('text_encoders','gemma_3_12B_it_fp8.safetensors'),m('text_encoders','ltx-2.3_text_projection_bf16.safetensors'),m('vae','LTX23_video_vae_bf16.safetensors'),m('vae','LTX23_audio_vae_bf16.safetensors')]
  :family==='wan'?[m('diffusion_models','wan_high_noise.safetensors',family),m('diffusion_models','wan_low_noise.safetensors',family),m('text_encoders','umt5_xxl_fp8.safetensors'),m('vae','wan_2.1_vae.safetensors')]
  :[m('diffusion_models','h3.safetensors',family),m('text_encoders','qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors'),m('vae','minimax_h3_video_vae_fp16.safetensors'),m('vae','minimax_h3_audio_vae_fp32.safetensors')];
 vi.mocked(scanModels).mockResolvedValue(list);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'DEAS_ImagePostprocessGate','DaSiWa_WildcardPresetPromptBuilder']}});
 const options={...defaultOptions,outputNaming:'test/%seed%'};
 localStorage.setItem('deas.pref.studio.run.options',JSON.stringify(options));
 localStorage.setItem('deas.pref.studio.run.seed','42');
 localStorage.setItem('deas.pref.studio.run.seedMode',JSON.stringify('fixed'));
 localStorage.setItem('deas.pref.studio.run.positive',JSON.stringify('live forest'));
 localStorage.setItem('deas.pref.studio.run.post','true');
 localStorage.setItem('deas.pref.studio.run.wildcardEnabled','true');
 if(family==='image'){
  localStorage.setItem('deas.pref.studio.run.upscale','2');
  localStorage.setItem('deas.pref.studio.run.loras',JSON.stringify([{name:'style.safetensors',strength:0.7}]));
  localStorage.setItem('deas.pref.studio.run.wildcardEnabled','true');
 }
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'same-job',baseUrl:evidence.baseUrl,promptId:'p',state:'queued',outputs:[],error:null,generationReady:false});
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:[{name:'ref.png',path:'/input/ref.png',kind:'file',symlink:false,media:'image',size:1,modified:1}],truncated:false}));
 const props={endpoint:evidence.baseUrl,roots:{models:'/m',input:'/input',output:'/output'}};
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 await waitFor(()=>expect(scanModels).toHaveBeenCalled());
 if(family!=='image')choosePipe(family==='wan'?'Image → Video':'Text → Video');
 if(family==='wan'){
  fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
  fireEvent.click(await screen.findByRole('button',{name:'ref.png'}));adopt();
  const ref=screen.getByRole('img',{name:'ref.png'});
  Object.defineProperties(ref,{naturalWidth:{value:720},naturalHeight:{value:1280}});fireEvent.load(ref);
 }
 fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'edited live forest'}});
 const shell=view.container.querySelector('[data-workbench]');
 view.rerender(<I18nProvider><Workbench {...props} planVisible/></I18nProvider>);
 const shown=()=>JSON.parse(view.container.querySelector('[data-plan-intent]')!.textContent!);
 expect(shown()).toMatchObject({positivePrompt:'edited live forest',seed:42,outputPrefix:'raw/test/42'});
 const preview=shown();
 if(family==='image')expect(preview).toMatchObject({refine:{width:2048,height:2048},loras:[{name:'style.safetensors',strength:0.7}],wildcard:expect.any(Object)});
 else {expect(preview.family).toBe(family);expect(preview.refine).toBeUndefined();expect(preview.wildcard).toBeUndefined();expect(screen.getByRole('region',{name:'Run Plan'}).querySelector('[data-plan-stage="refine"]')).toBeNull();}
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();expect(uploadMask).not.toHaveBeenCalled();
 view.rerender(<I18nProvider><Workbench {...props}/></I18nProvider>);
 expect(view.container.querySelector('[data-workbench]')).toBe(shell);
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toEqual(preview);
});

test('prompt fields expand into a floating editor that edits the same text and closes with Escape',async()=>{
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801"/></I18nProvider>);
 fireEvent.change(screen.getByLabelText('Prompt'),{target:{value:'a castle'}});
 fireEvent.click(screen.getByRole('button',{name:'Expand Prompt into a floating editor'}));
 const big=within(screen.getByRole('dialog',{name:'Prompt'})).getByRole('textbox',{name:'Prompt'}) as HTMLTextAreaElement;
 expect(big.value).toBe('a castle');
 expect(document.querySelector('[data-prompt-count="positive"]')?.textContent).toBe('8 / 16384');
 expect(document.querySelector('[data-prompt-count="negative"]')?.textContent).toBe('0 / 16384');
 expect(big.getAttribute('spellcheck')).toBe('false'); // in the app DEAS checks the text itself; the WebView checker stays off
 fireEvent.change(big,{target:{value:'a castle at dawn'}});
 expect(within(screen.getByRole('dialog',{name:'Prompt'})).getByRole('button',{name:'Prompt Forge'})).toBeTruthy();
 expect((document.querySelector('[data-run-positive]') as HTMLTextAreaElement).value).toBe('a castle at dawn');
 fireEvent.keyDown(window,{key:'Escape'});
 expect(screen.queryByRole('dialog',{name:'Prompt'})).toBeNull();
 // Both directions: the inline field feeds the open editor too.
 fireEvent.click(screen.getByRole('button',{name:'Expand Prompt into a floating editor'}));
 fireEvent.change(document.querySelector('[data-run-positive]')!,{target:{value:'inline edit'}});
 expect((within(screen.getByRole('dialog',{name:'Prompt'})).getByRole('textbox',{name:'Prompt'}) as HTMLTextAreaElement).value).toBe('inline edit');
 fireEvent.keyDown(window,{key:'Escape'});
 fireEvent.click(screen.getByRole('button',{name:'Expand Negative prompt into a floating editor'}));
 fireEvent.change(within(screen.getByRole('dialog',{name:'Negative prompt'})).getByRole('textbox'),{target:{value:'blurry'}});
 expect((document.querySelector('[data-run-negative]') as HTMLTextAreaElement).value).toBe('blurry');
 fireEvent.click(within(screen.getByRole('dialog',{name:'Negative prompt'})).getByRole('button',{name:'Close editor'}));
 expect(screen.queryByRole('dialog')).toBeNull();
});

const mossNodes=['UNETLoader','CLIPLoader','VAELoader','CLIPTextEncode','KSampler','VAEDecode','SaveImage','EmptyFlux2LatentImage','LoadImage','VAEEncode','ImageScale','LoadImageMask','SetLatentNoiseMask','ImageCompositeMasked'];
async function mountMoss(nodes=mossNodes,clipTypes=['moss_image']){
 vi.mocked(scanModels).mockResolvedValue([
  {category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null},
  {category:'diffusion_models',name:'moss.safetensors',family:'moss-image',path:'/m/moss',size:1,info:null},
  {category:'text_encoders',name:'qwen3.5_9b.safetensors',component:'qwen3.5-9b',path:'/m/clip',size:1,info:null},
  {category:'vae',name:'flux2-vae.safetensors',component:'flux2-vae',path:'/m/vae',size:1,info:null},
  {category:'loras',name:'unknown.safetensors',path:'/m/lora',size:1,info:null},
 ]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,missingNodes:['CheckpointLoaderSimple','EmptyLatentImage'],allowlist:{...evidence.allowlist,nodes,clipTypes}});
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'moss-job',promptId:'p',baseUrl:evidence.baseUrl,state:'succeeded',outputs:[],error:null,generationReady:false} as JobStatus);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',input:'/input'}}/></I18nProvider>);
 await screen.findByRole('option',{name:'moss.safetensors'});
 fireEvent.change(screen.getByLabelText('Checkpoint'),{target:{value:'moss.safetensors'}});
}
test.each(['moss.safetensors','a.safetensors'])('image Wildcards block %s until its plugin is available or explicitly disabled',async(checkpoint)=>{
 localStorage.setItem('deas.pref.studio.run.wildcardEnabled','true');
 await mountMoss([...mossNodes,...evidence.allowlist.nodes]);
 fireEvent.change(screen.getByLabelText('Checkpoint'),{target:{value:checkpoint}});
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(true);
 expect(screen.getByRole('alert').textContent).toContain('DaSiWa_WildcardPresetPromptBuilder');
 fireEvent.click(screen.getByRole('button',{name:'Run'}));expect(runWorkbenchJob).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'Disable wildcards'}));
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(false);
 fireEvent.click(screen.getByRole('button',{name:'Run'}));await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(vi.mocked(runWorkbenchJob).mock.lastCall![1]).not.toHaveProperty('wildcard');
});
test.each(['moss.safetensors','a.safetensors'])('image Wildcards preserve enabled intent for %s with its plugin available',async(checkpoint)=>{
 localStorage.setItem('deas.pref.studio.run.wildcardEnabled','true');
 await mountMoss([...mossNodes,...evidence.allowlist.nodes,'DaSiWa_WildcardPresetPromptBuilder']);
 fireEvent.change(screen.getByLabelText('Checkpoint'),{target:{value:checkpoint}});
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(false);
 fireEvent.click(screen.getByRole('button',{name:'Run'}));await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(vi.mocked(runWorkbenchJob).mock.lastCall![1]).toHaveProperty('wildcard');
});
test.each([false,true])('MOSS reference readiness follows the active encoder without CLIPTextEncode (i2i=%s)',async imageToImage=>{
 if(imageToImage)vi.mocked(invoke).mockResolvedValue({path:'/input',parent:'/',entries:[{name:'source.png',path:'/input/source.png',kind:'file',media:'image',size:1,modified:1}],truncated:false});
 await mountMoss([...mossNodes.filter(n=>n!=='CLIPTextEncode'),'DEAS_ImagePromptEncode']);
 if(imageToImage)await mossInput(512,768);
 const run=screen.getByRole('button',{name:'Run'}) as HTMLButtonElement;
 expect(run.disabled).toBe(true);
 expect(screen.getByRole('alert').textContent).toContain('CLIPTextEncode');
 expect(promptDrop().dropEffect).toBe('copy');
 expect(run.disabled).toBe(false);
 vi.mocked(uploadInput).mockResolvedValue('deas-inputs/ref.png');
 fireEvent.click(run);await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.lastCall![1]).toMatchObject({
  checkpoint:'moss.safetensors',nativeImageReference:{method:'moss-image',images:['deas-inputs/ref.png']},
  ...(imageToImage?{inputImage:'source.png'}:{}),
 });
 fireEvent.click(document.querySelector('[data-reference-remove]')!);
 expect(run.disabled).toBe(true);
 expect(screen.getByRole('alert').textContent).toContain('CLIPTextEncode');
});
test.each([...mossNodes.filter(n=>!['LoadImage','VAEEncode','ImageScale','LoadImageMask','SetLatentNoiseMask','ImageCompositeMasked'].includes(n)), 'moss_image'])('MOSS blocks t2i when emitted capability %s is missing',async(missing)=>{
 await mountMoss(mossNodes.filter(n=>n!==missing),missing==='moss_image'?[]:['moss_image']);
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(true);
 expect(screen.getByRole('alert').textContent).toContain(missing);
 fireEvent.click(screen.getByRole('button',{name:'Run'}));expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test.each([['SaveImage'],['DaSiWa_MetadataImageSaver','DaSiWa_MetadataConfig']])('MOSS uses the available saver %s without checkpoint diagnostic nodes',async(...saver)=>{
 await mountMoss([...mossNodes.filter(n=>n!=='SaveImage'),...saver]);
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(false);
 fireEvent.click(screen.getByRole('button',{name:'Run'}));await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
});
async function mossInput(width=520,height=768){
 vi.mocked(invoke).mockResolvedValue({path:'/input',parent:'/',entries:[{name:'source.png',path:'/input/source.png',kind:'file',media:'image',size:1,modified:1}],truncated:false});
 fireEvent.click(await screen.findByRole('button',{name:'source.png'}));adopt();
 const image=screen.getByRole('img',{name:'source.png'});
 Object.defineProperties(image,{naturalWidth:{value:width},naturalHeight:{value:height}});fireEvent.load(image);
 choosePipe('Image → Image');
}
test.each(['off','auto'])('MOSS blocks an unaligned source in %s without hiding its declared size',async(mode)=>{
 localStorage.setItem('deas.pref.studio.run.inputScaling',JSON.stringify(mode));
 vi.mocked(invoke).mockResolvedValue({path:'/input',parent:'/',entries:[{name:'source.png',path:'/input/source.png',kind:'file',media:'image',size:1,modified:1}],truncated:false});
 await mountMoss(mossNodes.filter(n=>n!=='EmptyFlux2LatentImage'));
 await mossInput();
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(true);
 expect(document.body.textContent).toContain('Choose Target, Fit or Crop');
 expect(screen.getByTestId('editor-world').style.width).toBe('520px');
 fireEvent.click(screen.getByRole('button',{name:'Run'}));expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test.each(['fit','crop','target'])('MOSS explicit %s resizes an unaligned source and i2i does not need an empty latent',async(mode)=>{
 vi.mocked(invoke).mockResolvedValue({path:'/input',parent:'/',entries:[{name:'source.png',path:'/input/source.png',kind:'file',media:'image',size:1,modified:1}],truncated:false});
 await mountMoss(mossNodes.filter(n=>n!=='EmptyFlux2LatentImage'));await mossInput();
 fireEvent.change(screen.getByLabelText('Input scaling'),{target:{value:mode}});
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(false);
 fireEvent.click(screen.getByRole('button',{name:'Run'}));await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(vi.mocked(runWorkbenchJob).mock.lastCall![1]).toMatchObject({inputImage:'source.png',inputResize:{crop:mode==='crop'?'center':'disabled'}});
 expect(screen.getByTestId('editor-world').style.width).toBe('520px');
});
test.each([
 ['attention',{attention:'a:test'},'Attention backend'],
 ['cache',{cache:'EasyCache'},'Step cache'],
 ['compile',{compile:'inductor'},'torch.compile'],
 ['vae',{vae:'tiled'},'VAE processing'],
 ['sigmas',{sigmas:'1, 0.5, 0'},'Custom sigmas'],
] as const)('MOSS blocks remembered %s independently and preserves unrelated options on reset',async(_key,patch,label)=>{
 localStorage.setItem('deas.pref.studio.run.options',JSON.stringify({...defaultOptions,...patch,outputNaming:'keep/%seed%'}));
 await mountMoss();
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(true);
 expect(screen.getByRole('alert').textContent).toContain('MOSS-Image supports plain generation');
 fireEvent.click(screen.getByRole('button',{name:`Restore ${label} defaults`}));
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(false);
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.options')!).outputNaming).toBe('keep/%seed%');
});
test('MOSS blocks an unknown-family LoRA until it is explicitly removed',async()=>{
 localStorage.setItem('deas.pref.studio.run.loras',JSON.stringify([{name:'unknown.safetensors',strength:0.5}]));
 await mountMoss();
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(true);
 fireEvent.click(screen.getByRole('button',{name:'Restore LoRAs defaults'}));
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(false);
});
test.each(['LoadImage','VAEEncode','ImageScale'])('MOSS i2i blocks its emitted %s capability',async(missing)=>{
 vi.mocked(invoke).mockResolvedValue({path:'/input',parent:'/',entries:[{name:'source.png',path:'/input/source.png',kind:'file',media:'image',size:1,modified:1}],truncated:false});
 await mountMoss(mossNodes.filter(n=>n!==missing));await mossInput(512);
 if(missing==='ImageScale'){
  expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(false);
  fireEvent.change(screen.getByLabelText('Input scaling'),{target:{value:'fit'}});
 }
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(true);
 expect(screen.getByRole('alert').textContent).toContain(missing);
});
test('MOSS retains unsupported pad until a visible reset',async()=>{
 vi.mocked(invoke).mockResolvedValue({path:'/input',parent:'/',entries:[{name:'source.png',path:'/input/source.png',kind:'file',media:'image',size:1,modified:1}],truncated:false});
 await mountMoss();await mossInput(512);
 fireEvent.change(screen.getByLabelText('Input scaling'),{target:{value:'pad'}});
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(true);
 fireEvent.click(screen.getByRole('button',{name:'Restore Input scaling defaults'}));
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(false);
 // More stays visible on MOSS images; only its Refine/Upscale passes are unavailable.
 expect(document.querySelector('summary[data-more]')).not.toBeNull();
 for(const pass of ['[data-run-post]','[data-run-upscale]'])expect(document.querySelector<HTMLButtonElement>(pass)).toMatchObject({disabled:true,title:expect.stringContaining('MOSS')});
});
test('MOSS Inpaint is compatible and checks its mask nodes',async()=>{
 vi.mocked(invoke).mockResolvedValue({path:'/input',parent:'/',entries:[{name:'source.png',path:'/input/source.png',kind:'file',media:'image',size:1,modified:1}],truncated:false});
 await mountMoss(mossNodes.filter(n=>n!=='SetLatentNoiseMask'));await mossInput(512);
 fireEvent.click(document.querySelector('[data-edit="inpaint"]')!);
 expect(screen.queryByRole('button',{name:'Restore Inpaint defaults'})).toBeNull();
 expect(screen.getByRole('alert').textContent).toContain('SetLatentNoiseMask');
});
test('MOSS disables inserted layers explicitly without deleting the editor draft',async()=>{
 vi.mocked(invoke).mockResolvedValue({path:'/input',parent:'/',entries:[{name:'source.png',path:'/input/source.png',kind:'file',media:'image',size:1,modified:1}],truncated:false});
 vi.stubGlobal('Image',class{naturalWidth=200;naturalHeight=100;onload=()=>{};onerror=()=>{};set src(_:string){setTimeout(()=>this.onload());}});
 await mountMoss();await mossInput(512);dropTile('source.png');
 await screen.findByRole('group',{name:'Inserted image source.png'});
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(true);
 fireEvent.click(screen.getByRole('button',{name:'Restore Layers defaults'}));
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(false);
 expect(screen.queryByRole('group',{name:'Inserted image source.png'})).toBeNull();
 expect(screen.getByTestId('editor-world').style.width).toBe('512px');
});
test('MOSS retains incompatible options and post on a family switch until explicit per-feature resets',async()=>{
 localStorage.setItem('deas.pref.studio.run.options',JSON.stringify({...defaultOptions,cache:'EasyCache',outputNaming:'kept/%seed%'}));
 localStorage.setItem('deas.pref.studio.run.post','true');
 await mountMoss([...mossNodes,'EasyCache']);
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(true);
 expect(localStorage.getItem('deas.pref.studio.run.post')).toBe('true');
 fireEvent.click(screen.getByRole('tab',{name:/^Options/}));expect((screen.getByLabelText('Step cache') as HTMLSelectElement).value).toBe('EasyCache');
 fireEvent.click(screen.getByRole('button',{name:'Restore Step cache defaults'}));
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(true);
 fireEvent.click(screen.getByRole('button',{name:'Restore Post-Processing defaults'}));
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.options')!).outputNaming).toBe('kept/%seed%');
 expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(false);
 fireEvent.click(screen.getByRole('button',{name:'Run'}));await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalled());
 expect(vi.mocked(runWorkbenchJob).mock.lastCall![1]).not.toHaveProperty('options');
 expect(vi.mocked(runWorkbenchJob).mock.lastCall![1]).not.toHaveProperty('refine');
});

test('three rapid Run presses capture prompts and distinct random seeds while IPC is pending',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 const replies:((job:JobStatus)=>void)[]=[];
 vi.mocked(runWorkbenchJob).mockImplementation(()=>new Promise(resolve=>replies.push(resolve)));
 const view=render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 const run=document.querySelector<HTMLButtonElement>('[data-run]')!;
 await waitFor(()=>expect(run.disabled).toBe(false));
 for(const prompt of ['one','two','three']){fireEvent.change(document.querySelector('[data-run-positive]')!,{target:{value:prompt}});fireEvent.click(run);}
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(3));
 const calls=vi.mocked(runWorkbenchJob).mock.calls;
 expect(calls.map(c=>c[1].positivePrompt)).toEqual(['one','two','three']);
 expect(new Set(calls.map(c=>c[1].seed)).size).toBe(3);
 expect(calls.map(c=>c[2])).toEqual([0,0,0]);expect(run.textContent).toContain('Run');
 view.unmount();await act(async()=>replies.forEach((reply,i)=>reply(heldJob('rapid-'+i,'queued'))));
});

test('Stop discards every unsent image preparation without claiming upload cancellation',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a.safetensors',size:1,info:null}]);
 const saved={...projectFixture,draft:{...projectFixture.draft,checkpoint:'a.safetensors',pipeline:'i2i'},input:{id:'input-1',path:'/projects/one/assets/input.png',sha256:'a'.repeat(64),width:1024,height:1024}};
 vi.mocked(invoke).mockImplementation(async c=>c==='native_project_open'?saved:{entries:[],truncated:false});
 let release!:(name:string)=>void;const pending=new Promise<string>(yes=>{release=yes;});vi.mocked(uploadInput).mockReturnValue(pending);
 const props={endpoint:evidence.baseUrl,roots:{models:'/m',input:'/env/in'}};
 render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Open project'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Open project'}));
 await waitFor(()=>expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('saved project prompt'));
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));await waitFor(()=>expect(uploadInput).toHaveBeenCalledTimes(1));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));await waitFor(()=>expect(uploadInput).toHaveBeenCalledTimes(2));
 stopAll();
 await waitFor(()=>expect(stopWorkbench).toHaveBeenCalledTimes(1));
 await act(async()=>release('deas-inputs/late.png'));
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(document.querySelectorAll('[data-job-id]')).toHaveLength(0);
 expect(document.querySelector('[data-run-stop]')).toBeNull();expect(pollWorkbench).not.toHaveBeenCalled();
});

test('Stop drops delayed video preparation and the next press uses the new native epoch',async()=>{
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(invoke).mockImplementation(async(_c,args)=>({path:(args as {path:string}).path,parent:'/env',entries:mediaEntries,truncated:false}));
 let release!:(name:string)=>void;
 vi.mocked(uploadInput).mockReturnValueOnce(new Promise(yes=>{release=yes;}));
 vi.mocked(runWorkbenchJob).mockImplementation(async(baseUrl,intent)=>{
  const job={...heldJob('late-upload-job','queued',baseUrl),intent};
  vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
  vi.mocked(pollWorkbench).mockResolvedValue(collection([job]));
  return job;
 });
 const props={endpoint:'http://127.0.0.1:7801',roots:{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}};
 render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Video → Video').disabled).toBe(false));choosePipe('Video → Video');
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 fireEvent.click(await screen.findByRole('button',{name:'clip.mp4'}));adopt();loadVideo('clip.mp4');
 await waitFor(()=>expect((document.querySelector('[data-run]') as HTMLButtonElement).disabled).toBe(false));
 fireEvent.click(document.querySelector('[data-run]')!);
 await waitFor(()=>expect(uploadInput).toHaveBeenCalledTimes(1));
 stopAll();
 await screen.findByText('No native work remained. Unsent preparations were discarded.');
 await act(async()=>release('deas-inputs/late.mp4'));
 expect(runWorkbenchJob).not.toHaveBeenCalled();
 expect(document.querySelectorAll('[data-job-id]')).toHaveLength(0);
 vi.mocked(uploadInput).mockResolvedValue('deas-inputs/new.mp4');fireEvent.click(document.querySelector('[data-run]')!);
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));expect(vi.mocked(runWorkbenchJob).mock.calls[0][2]).toBe(1);
});


const stopAll=()=>{fireEvent.click(document.querySelector('[data-run-stop]')!);fireEvent.click(screen.getByRole('button',{name:'Stop all'}));};
const imageModels=[{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}];
test.each(['empty','settled','partial'] as const)('Stop %s preserves native products, stays quiet on partial and frees the queue',async outcome=>{
 vi.mocked(scanModels).mockResolvedValue(imageModels);
 const product={...heldJob('product','succeeded'),outputs:[{filename:'keep.png',subfolder:'',type:'output' as const}],outputRoot:'/old/output'};
 const unknown=heldJob('foreign','unknown');
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([product,unknown]));
 vi.mocked(stopWorkbench).mockResolvedValue({...collection([product,unknown]),admissionEpoch:1,outcome,discarded:0,issues:outcome==='partial'?[{jobId:unknown.id,error:'Foreign backend: scoped interruption unsupported'}]:[]});
 render(<I18nProvider><Workbench endpoint="" roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-job-id="foreign"]')).not.toBeNull());
 fireEvent.change(screen.getByLabelText('Width'),{target:{value:'1'}});
 expect(document.querySelector<HTMLButtonElement>('[data-run-stop]')!.disabled).toBe(false);
 stopAll();
 await waitFor(()=>expect(stopWorkbench).toHaveBeenCalledTimes(1));
 if(outcome==='partial'){await waitFor(()=>expect(document.querySelector('[data-run-stop]')).toBeNull());expect(document.querySelector('[data-run-stop-status]')).toBeNull();expect(screen.queryByRole('alert')).toBeNull();}
 else await waitFor(()=>expect(document.querySelector('[data-run-stop-status]')!.textContent).toContain(outcome==='empty'?'No native work':'Stop finished'));
 expect(document.querySelector('[data-job-id="foreign"] [data-state]')!.getAttribute('data-state')).toBe('unknown');
 expect(document.querySelector('[data-job-id="product"] img')!.getAttribute('src')).toContain('/old/output/keep.png');
 expect(screen.queryByRole('button',{name:/Running/})).toBeNull();
 // The stopped unknown job no longer holds the Workbench: Stop is gone until new work starts.
 expect(document.querySelector('[data-run-stop]')).toBeNull();
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});
test('without backend capabilities Studio shows one calm notice and disables Run, never dependency warnings',async()=>{
 vi.mocked(scanModels).mockResolvedValue(imageModels);
 localStorage.setItem('deas.pref.studio.run.options',JSON.stringify({attention:'sage'}));
 render(<I18nProvider><Workbench endpoint="" roots={{models:'/m'}}/></I18nProvider>);
 const run=await screen.findByRole('button',{name:'Run'});
 await waitFor(()=>expect(run.title).toBe('Backend not ready – backend features unavailable.'));
 expect(run.hasAttribute('disabled')).toBe(true);
 expect(screen.getByText('Backend not ready – backend features unavailable.')).toBeTruthy();
 expect(screen.queryByRole('alert')).toBeNull();
 expect(document.body.textContent).not.toContain('Unavailable or incompatible');
});
test.each(['selected','cancel','escape'] as const)('Stop choice %s preserves a peer upload and admission epoch',async action=>{
 const previous={invoke:vi.mocked(invoke).getMockImplementation(),upload:vi.mocked(uploadInput).getMockImplementation(),run:vi.mocked(runWorkbenchJob).getMockImplementation()};
 try{
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(uploadInput).mockReset();vi.mocked(runWorkbenchJob).mockReset();
 vi.mocked(scanModels).mockResolvedValue(imageModels);
 const saved={...projectFixture,draft:{...projectFixture.draft,checkpoint:'a.safetensors',pipeline:'i2i'},input:{id:'input-1',path:'/projects/one/assets/input.png',sha256:'a'.repeat(64),width:1024,height:1024}};
 vi.mocked(invoke).mockImplementation(async c=>c==='native_project_open'?saved:{entries:[],truncated:false});
 let release!:(name:string)=>void;
 vi.mocked(uploadInput).mockResolvedValueOnce('deas-inputs/first.png').mockReturnValueOnce(new Promise(resolve=>{release=resolve;}));
 let snapshot={...collection(),admissionEpoch:7};
 vi.mocked(readCurrentWorkbench).mockImplementation(async()=>snapshot);vi.mocked(pollWorkbench).mockImplementation(async()=>snapshot);
 vi.mocked(runWorkbenchJob).mockImplementation(async(baseUrl,intent)=>{const job={...heldJob(snapshot.jobs.length?'second':'first','queued',baseUrl),intent};snapshot={...snapshot,jobs:[...snapshot.jobs,job]};return job;});
 vi.mocked(stopWorkbenchJob).mockImplementation(async job=>{const next={...job,state:'cancelled' as const};snapshot={...snapshot,jobs:[next]};return next;});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',input:'/env/in'}}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Open project'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Open project'}));
 await waitFor(()=>expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('saved project prompt'));
 const run=screen.getByRole('button',{name:'Run'});await waitFor(()=>expect(run.hasAttribute('disabled')).toBe(false));
 fireEvent.click(run);await waitFor(()=>expect(document.querySelector('[data-job-id="first"]')).not.toBeNull());
 await waitFor(()=>expect(run.hasAttribute('disabled')).toBe(false),{timeout:2500});
 fireEvent.click(run);await waitFor(()=>expect(uploadInput).toHaveBeenCalledTimes(2));
 fireEvent.click(document.querySelector('[data-run-stop]')!);
 const dialog=screen.getByRole('dialog',{name:'Stop Workbench work'});
 if(action==='escape')fireEvent(dialog,new Event('cancel',{cancelable:true}));
 else fireEvent.click(within(dialog).getByRole('button',{name:action==='selected'?'Stop selected item':'Cancel'}));
 if(action==='selected')await waitFor(()=>expect(stopWorkbenchJob).toHaveBeenCalledTimes(1));else expect(stopWorkbenchJob).not.toHaveBeenCalled();
 await act(async()=>release('deas-inputs/second.png'));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(2));
 expect(vi.mocked(runWorkbenchJob).mock.calls.map(call=>call[2])).toEqual([7,7]);expect(stopWorkbench).not.toHaveBeenCalled();
 await waitFor(()=>expect(document.querySelector('[data-job-id="second"]')).not.toBeNull());
 }finally{cleanup();vi.mocked(invoke).mockImplementation(previous.invoke!);vi.mocked(uploadInput).mockReset().mockImplementation(previous.upload!);vi.mocked(runWorkbenchJob).mockReset().mockImplementation(previous.run!);}
});
test('Stop choice disables a captured item that finishes while the dialog is open',async()=>{
 const selected=heldJob('selected','running'),peer=heldJob('peer','queued');
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([peer,selected]));
 vi.mocked(pollWorkbench).mockResolvedValue(collection([peer,{...selected,state:'succeeded'}]));
 render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-run-stop]')).not.toBeNull());
 fireEvent.click(document.querySelector('[data-run-stop]')!);
 const button=screen.getByRole('button',{name:'Stop selected item'});
 await waitFor(()=>expect(button.hasAttribute('disabled')).toBe(true),{timeout:2500});
 expect(button.title).toBe('The selected item is no longer queued or running.');
 fireEvent.click(button);expect(stopWorkbenchJob).not.toHaveBeenCalled();expect(stopWorkbench).not.toHaveBeenCalled();
});
test.each(['queued','running'] as const)('Stop choice pins the %s selection and leaves peers progressing',async state=>{
 const selected=heldJob('selected',state),peer=heldJob('peer','queued');
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([peer,selected]));
 vi.mocked(stopWorkbenchJob).mockResolvedValue({...selected,state:'cancelled'});
 vi.mocked(pollWorkbench).mockResolvedValue(collection([{...peer,state:'running'},{...selected,state:'cancelled'}]));
 render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-job-id="selected"]')).not.toBeNull());
 fireEvent.click(document.querySelector('[data-run-stop]')!);
 const dialog=screen.getByRole('dialog',{name:'Stop Workbench work'});
 expect(document.activeElement).toBe(within(dialog).getByRole('button',{name:'Cancel'}));
 expect(stopWorkbenchJob).not.toHaveBeenCalled();expect(stopWorkbench).not.toHaveBeenCalled();
 // A native modal prevents pointer changes, but asynchronous selection changes must not retarget it either.
 fireEvent.click(document.querySelector('[data-job-id="peer"] .wb-queue-select')!);
 expect(within(dialog).getByText('selected')).toBeTruthy();
 fireEvent.click(within(dialog).getByRole('button',{name:'Stop selected item'}));
 await waitFor(()=>expect(stopWorkbenchJob).toHaveBeenCalledWith(expect.objectContaining({id:'selected',state})));
 expect(stopWorkbench).not.toHaveBeenCalled();
 await waitFor(()=>expect(document.querySelector('[data-job-id="peer"] [data-state]')?.getAttribute('data-state')).toBe('running'),{timeout:2500});
});
test.each(['cancel','escape'] as const)('Stop choice %s leaves jobs and preparation untouched and restores focus',async action=>{
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([heldJob('held','running')],1));
 render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-run-stop]')).not.toBeNull());
 const trigger=document.querySelector<HTMLButtonElement>('[data-run-stop]')!;trigger.focus();fireEvent.click(trigger);
 const dialog=screen.getByRole('dialog',{name:'Stop Workbench work'});
 if(action==='cancel')fireEvent.click(within(dialog).getByRole('button',{name:'Cancel'}));else fireEvent(dialog,new Event('cancel',{cancelable:true}));
 expect(screen.queryByRole('dialog',{name:'Stop Workbench work'})).toBeNull();expect(document.activeElement).toBe(trigger);
 expect(stopWorkbench).not.toHaveBeenCalled();expect(stopWorkbenchJob).not.toHaveBeenCalled();
 expect(document.querySelector('[data-job-id="held"] [data-state]')?.getAttribute('data-state')).toBe('running');
});
test.each(['missing','succeeded','unknown','unsupported'] as const)('Stop choice explains disabled %s selection',async state=>{
 const selected=heldJob('selected',state==='unsupported'?'running':state==='missing'?'queued':state);
 vi.mocked(readCurrentWorkbench).mockResolvedValue({...collection(state==='missing'?[]:[selected],1),stopAvailable:state!=='unsupported'});
 render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-run-stop]')).not.toBeNull());
 fireEvent.click(document.querySelector('[data-run-stop]')!);
 const dialog=screen.getByRole('dialog',{name:'Stop Workbench work'}),button=within(dialog).getByRole('button',{name:'Stop selected item'});
 expect(button.hasAttribute('disabled')).toBe(true);expect(button.getAttribute('title')).toBeTruthy();
 expect(within(dialog).getByText(button.getAttribute('title')!)).toBeTruthy();
 fireEvent.click(button);expect(stopWorkbenchJob).not.toHaveBeenCalled();expect(stopWorkbench).not.toHaveBeenCalled();
});
test.each(['rejected','unknown'] as const)('selected Stop keeps %s truthful without global fallback',async outcome=>{
 const job={...heldJob('retained','running'),outputs:[{filename:'keep.png',subfolder:'',type:'output' as const}],outputRoot:'/old/output'};
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
 if(outcome==='rejected')vi.mocked(stopWorkbenchJob).mockRejectedValue(new Error('Scoped interrupt unsupported'));
 else vi.mocked(stopWorkbenchJob).mockResolvedValue({...job,state:'unknown',error:'Scoped interrupt outcome unknown'});
 render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-run-stop]')).not.toBeNull());
 fireEvent.click(document.querySelector('[data-run-stop]')!);fireEvent.click(screen.getByRole('button',{name:'Stop selected item'}));
 await waitFor(()=>expect(document.querySelector('.wb-stage-warning[role="alert"]')?.textContent).toContain(outcome==='rejected'?'Scoped interrupt unsupported':'Scoped interrupt outcome unknown'));
 expect(stopWorkbench).not.toHaveBeenCalled();expect(document.querySelector('[data-job-id="retained"] [data-state]')?.getAttribute('data-state')).toBe(outcome==='rejected'?'running':'unknown');
});
test('selected Stop owns only its H3 Run all sequence',async()=>{
 vi.mocked(scanModels).mockResolvedValue([
  {category:'diffusion_models',name:'MiniMaxH3/h3.safetensors',path:'/m/h3',size:1,info:null,family:'minimax-h3'},
  {category:'text_encoders',name:'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors',path:'/m/t',size:1,info:null},
  {category:'vae',name:'MiniMaxH3/minimax_h3_video_vae_fp16.safetensors',path:'/m/v',size:1,info:null},
  {category:'vae',name:'MiniMaxH3/minimax_h3_audio_vae_fp32.safetensors',path:'/m/a',size:1,info:null}]);
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 const job=heldJob('continuity-selected-stop','queued');
 vi.mocked(runWorkbenchJob).mockResolvedValue(job);
 vi.mocked(pollWorkbench).mockResolvedValue(collection([job]));
 vi.mocked(stopWorkbenchJob).mockResolvedValue({...job,state:'cancelled'});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 fireEvent.click(await screen.findByRole('button',{name:'Continuity · off'}));
 fireEvent.click(screen.getByRole('checkbox',{name:'Continuity active'}));
 fireEvent.click(screen.getByRole('button',{name:'Add step'}));fireEvent.click(screen.getByRole('button',{name:'Add step'}));
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run all'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run all'}));
 fireEvent.click(within(screen.getByRole('dialog',{name:'Continuity'})).getByRole('button',{name:'Close'}));
 await waitFor(()=>expect(document.querySelector('[data-job-id="continuity-selected-stop"]')).not.toBeNull());
 fireEvent.click(document.querySelector('[data-run-stop]')!);fireEvent.click(screen.getByRole('button',{name:'Stop selected item'}));
 await waitFor(()=>expect(document.querySelector('[data-job-id="continuity-selected-stop"] [data-state]')?.getAttribute('data-state')).toBe('cancelled'));
 expect(stopWorkbench).not.toHaveBeenCalled();expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false);
});
test('late selected Stop cannot overwrite a newer Stop all result',async()=>{
 const job=heldJob('selected','running');let finish!:(job:JobStatus)=>void;
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
 vi.mocked(stopWorkbenchJob).mockReturnValue(new Promise(resolve=>{finish=resolve;}));
 vi.mocked(stopWorkbench).mockResolvedValue({...collection([{...job,state:'succeeded',outputs:[{filename:'keep.png',subfolder:'',type:'output'}],outputRoot:'/old/output'}]),admissionEpoch:1,outcome:'settled',discarded:0,issues:[]});
 render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-run-stop]')).not.toBeNull());
 fireEvent.click(document.querySelector('[data-run-stop]')!);fireEvent.click(screen.getByRole('button',{name:'Stop selected item'}));
 await waitFor(()=>expect(stopWorkbenchJob).toHaveBeenCalledTimes(1));stopAll();
 await screen.findByText('Stop finished. Check the retained job states.');
 await act(async()=>finish({...job,state:'unknown',error:'Late unknown'}));
 expect(document.querySelector('[data-job-id="selected"] [data-state]')?.getAttribute('data-state')).toBe('succeeded');
 expect(document.querySelector('[data-job-id="selected"] img')?.getAttribute('src')).toContain('/old/output/keep.png');
});
test('right-click on an active queue entry stops only that generation',async()=>{
 vi.mocked(scanModels).mockResolvedValue(imageModels);
 const running=heldJob('running','running'),done=heldJob('done','succeeded');
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([done,running]));
 vi.mocked(pollWorkbench).mockImplementation(async()=>collection([done,vi.mocked(stopWorkbenchJob).mock.calls.length?{...running,state:'cancelled'}:running]));
 vi.mocked(stopWorkbenchJob).mockResolvedValue({...running,state:'cancelled'});
 render(<I18nProvider><Workbench endpoint="" roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-job-id="running"]')).not.toBeNull());
 fireEvent.contextMenu(document.querySelector('[data-job-id="done"] .wb-queue-select')!);
 expect(screen.queryByRole('menuitem',{name:'Stop this generation'})).toBeNull();
 fireEvent.contextMenu(document.querySelector('[data-job-id="running"] .wb-queue-select')!);
 fireEvent.click(screen.getByRole('menuitem',{name:'Stop this generation'}));
 await waitFor(()=>expect(stopWorkbenchJob).toHaveBeenCalledWith(expect.objectContaining({id:'running'})));
 expect(stopWorkbench).not.toHaveBeenCalled();
 await waitFor(()=>expect(document.querySelector('[data-job-id="running"] [data-state]')!.getAttribute('data-state')).toBe('cancelled'));
});
test.each(['unreachable','unsupported'])('Stop %s leaves held states intact and gives a localized standard alert',async failure=>{
 const job=heldJob('retained','running');
 vi.mocked(readCurrentWorkbench).mockResolvedValue({...collection([job]),stopAvailable:failure!=='unsupported'});
 vi.mocked(stopWorkbench).mockRejectedValue(new Error('IPC unavailable'));
 render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector('[data-run-stop]')).not.toBeNull());
 stopAll();
 await waitFor(()=>expect(screen.getByRole('alert').textContent).toContain(failure==='unsupported'?'does not support Workbench Stop':'IPC unavailable'));
 expect(document.querySelector('[data-job-id="retained"] [data-state]')!.getAttribute('data-state')).toBe('running');
 expect(stopWorkbench).toHaveBeenCalledTimes(failure==='unsupported'?0:1);
});
test('Stop fences a sent Run reply, coalesces presses and passes the captured native epoch',async()=>{
 vi.mocked(scanModels).mockResolvedValue(imageModels);
 vi.mocked(readCurrentWorkbench).mockResolvedValue({...collection(),admissionEpoch:7});
 let finishRun!:(job:JobStatus)=>void;let finishStop!:(value:Awaited<ReturnType<typeof stopWorkbench>>)=>void;
 vi.mocked(runWorkbenchJob).mockReturnValue(new Promise(resolve=>{finishRun=resolve;}));
 vi.mocked(stopWorkbench).mockReturnValue(new Promise(resolve=>{finishStop=resolve;}));
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 fireEvent.click(document.querySelector('[data-run]')!);await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][2]).toBe(7);
 const stop=document.querySelector<HTMLButtonElement>('[data-run-stop]')!;
 fireEvent.click(stop);fireEvent.click(screen.getByRole('button',{name:'Stop all'}));fireEvent.click(stop);fireEvent.click(document.querySelector('[data-run]')!);
 expect(stopWorkbench).toHaveBeenCalledTimes(1);expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
 await act(async()=>finishStop({...collection([heldJob('stopped','cancelled')]),admissionEpoch:8,outcome:'settled',discarded:0,issues:[]}));
 await act(async()=>finishRun(heldJob('stale-run','running')));
 expect(document.querySelector('[data-job-id="stale-run"]')).toBeNull();
 expect(document.querySelector('[data-job-id="stopped"] [data-state]')!.getAttribute('data-state')).toBe('cancelled');
});
test('poll reply already in flight cannot overwrite Stop',async()=>{
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([heldJob('held','running')]));
 let finish!:(value:ReturnType<typeof collection>)=>void;
 vi.mocked(pollWorkbench).mockReturnValue(new Promise(resolve=>{finish=resolve;}));
 vi.mocked(stopWorkbench).mockResolvedValue({...collection([heldJob('held','cancelled')]),admissionEpoch:1,outcome:'settled',discarded:0,issues:[]});
 render(<I18nProvider><Workbench endpoint=""/></I18nProvider>);
 await waitFor(()=>expect(pollWorkbench).toHaveBeenCalledTimes(1),{timeout:2500});
 stopAll();
 await screen.findByText('Stop finished. Check the retained job states.');
 await act(async()=>finish(collection([heldJob('held','running')])));
 expect(document.querySelector('[data-job-id="held"] [data-state]')!.getAttribute('data-state')).toBe('cancelled');
});
test('native capacity rejection retains all active and unknown peers without frontend truncation',async()=>{
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(scanModels).mockResolvedValue(imageModels);
 const jobs=Array.from({length:20},(_,i)=>heldJob('capacity-'+i,i%2?'queued':'unknown'));
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection(jobs));
 vi.mocked(runWorkbenchJob).mockRejectedValue(new Error('Workbench capacity reached'));
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 fireEvent.click(document.querySelector('[data-run]')!);
 await screen.findByText('Workbench capacity reached');
 await waitFor(()=>expect(readCurrentWorkbench).toHaveBeenCalledTimes(2));
 expect(document.querySelectorAll('[data-job-id]')).toHaveLength(20);expect(stopWorkbench).not.toHaveBeenCalled();
});

test.each(['fixed','random'] as const)('rapid %s presses pin settings, naming and seed before uploads settle',async mode=>{
 vi.mocked(scanModels).mockResolvedValue(imageModels);
 localStorage.setItem('deas.pref.studio.run.seed','42');localStorage.setItem('deas.pref.studio.run.seedMode',JSON.stringify(mode));
 localStorage.setItem('deas.pref.studio.run.options',JSON.stringify({...defaultOptions,outputNaming:'press/%seed%'}));
 const pending:((job:JobStatus)=>void)[]=[];vi.mocked(runWorkbenchJob).mockImplementation(()=>new Promise(resolve=>pending.push(resolve)));
 const view=render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 const run=document.querySelector<HTMLButtonElement>('[data-run]')!;await waitFor(()=>expect(run.disabled).toBe(false));
 act(()=>{fireEvent.click(run);fireEvent.click(run);fireEvent.click(run);});
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(3));
 const intents=vi.mocked(runWorkbenchJob).mock.calls.map(call=>call[1]);
 expect(intents[0].seed).toBe(42);
 expect(new Set(intents.map(intent=>intent.seed)).size).toBe(mode==='fixed'?1:3);
 expect(intents.map(intent=>intent.outputPrefix)).toEqual(intents.map(intent=>`raw/press/${intent.seed}`));
 view.unmount();await act(async()=>pending.forEach((finish,i)=>finish(heldJob('fixed-'+i,'queued'))));
});
test('Stop disables serial Continuity before a pending clip read can continue Run all',async()=>{
 vi.mocked(scanModels).mockResolvedValue([
  {category:'diffusion_models',name:'MiniMaxH3/h3.safetensors',path:'/m/h3',size:1,info:null,family:'minimax-h3'},
  {category:'text_encoders',name:'qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors',path:'/m/t',size:1,info:null},
  {category:'vae',name:'MiniMaxH3/minimax_h3_video_vae_fp16.safetensors',path:'/m/v',size:1,info:null},
  {category:'vae',name:'MiniMaxH3/minimax_h3_audio_vae_fp32.safetensors',path:'/m/a',size:1,info:null}]);
 localStorage.setItem('deas.pref.studio.run.pipeline','"t2v"');
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 const job=heldJob('continuity-stop','queued');
 vi.mocked(runWorkbenchJob).mockResolvedValue(job);
 vi.mocked(pollWorkbench).mockResolvedValue(collection([{...job,state:'succeeded'}]));
 let finish!:(value:Awaited<ReturnType<typeof readH3Session>>)=>void;
 vi.mocked(readH3Session).mockReturnValue(new Promise(resolve=>{finish=resolve;}));
 vi.mocked(stopWorkbench).mockResolvedValue({...collection([{...job,state:'succeeded'}]),admissionEpoch:1,outcome:'empty',discarded:0,issues:[]});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 fireEvent.click(await screen.findByRole('button',{name:'Continuity · off'}));
 fireEvent.click(screen.getByRole('checkbox',{name:'Continuity active'}));
 fireEvent.click(screen.getByRole('button',{name:'Add step'}));fireEvent.click(screen.getByRole('button',{name:'Add step'}));
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run all'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run all'}));
 fireEvent.click(within(screen.getByRole('dialog',{name:'Continuity'})).getByRole('button',{name:'Close'}));
 await waitFor(()=>expect(readH3Session).toHaveBeenCalledTimes(1),{timeout:3000});
 expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
 stopAll();
 await waitFor(()=>expect(stopWorkbench).toHaveBeenCalledTimes(1));
 await act(async()=>finish({latestId:'root',clips:[{clipId:'root',parent:null,frames:124,completed:1,export:null}]}));
 await act(async()=>{});
 expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
 const plan=JSON.parse(localStorage.getItem('deas.pref.studio.run.continuity')!);
 expect(plan.steps.every((step:{clipId?:string})=>!step.clipId)).toBe(true);
});

test('real IPC Stop ownership survives remount, blocks Run and releases after current-state recovery',async()=>{
 const actual=await vi.importActual<typeof import('../../lib/jobs')>('../../lib/jobs');
 const previous={stop:vi.mocked(stopWorkbench).getMockImplementation(),read:vi.mocked(readCurrentWorkbench).getMockImplementation(),poll:vi.mocked(pollWorkbench).getMockImplementation(),invoke:vi.mocked(invoke).getMockImplementation()};
 let finish!:(value:Awaited<ReturnType<typeof stopWorkbench>>)=>void;
 let snapshot=collection([heldJob('owned-stop','running')]);
 vi.mocked(stopWorkbench).mockImplementation(actual.stopWorkbench);
 vi.mocked(readCurrentWorkbench).mockImplementation(actual.readCurrentWorkbench);vi.mocked(pollWorkbench).mockImplementation(actual.pollWorkbench);
 vi.mocked(invoke).mockImplementation((command,args)=>{
  if(command==='native_workbench_current'||command==='native_workbench_poll')return Promise.resolve(snapshot);
  if(command==='native_workbench_stop')return new Promise(resolve=>{finish=resolve;});
  return previous.invoke!(command,args);
 });
 vi.mocked(scanModels).mockResolvedValue(imageModels);
 const props={endpoint:evidence.baseUrl,roots:{models:'/m'}};
 try{
  const first=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
  await waitFor(()=>expect(document.querySelector('[data-run-stop]')).not.toBeNull());
  stopAll();first.unmount();
  render(<I18nProvider><Workbench {...props}/></I18nProvider>);
  await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run-stop]')?.disabled).toBe(true));
  expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(true);
  fireEvent.click(document.querySelector('[data-run]')!);expect(runWorkbenchJob).not.toHaveBeenCalled();
  snapshot={...collection([heldJob('owned-stop','cancelled')]),admissionEpoch:1};
  await act(async()=>finish({...snapshot,outcome:'settled',discarded:0,issues:[]}));
  await waitFor(()=>expect(document.querySelector('[data-job-id="owned-stop"] [data-state]')?.getAttribute('data-state')).toBe('cancelled'),{timeout:2500});
  expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false);
  expect(vi.mocked(invoke).mock.calls.filter(call=>call[0]==='native_workbench_stop')).toHaveLength(1);
 }finally{
  cleanup();vi.mocked(stopWorkbench).mockImplementation(previous.stop!);vi.mocked(readCurrentWorkbench).mockImplementation(previous.read!);vi.mocked(pollWorkbench).mockImplementation(previous.poll!);vi.mocked(invoke).mockImplementation(previous.invoke!);
 }
});

test('held jobs continue polling while another local Run IPC is still pending',async()=>{
 vi.mocked(scanModels).mockResolvedValue(imageModels);
 const peer=heldJob('poll-peer','queued');let finish!:(job:JobStatus)=>void;
 vi.mocked(runWorkbenchJob).mockResolvedValueOnce(peer).mockReturnValueOnce(new Promise(resolve=>{finish=resolve;}));
 vi.mocked(pollWorkbench).mockResolvedValue(collection([{...peer,state:'succeeded'}]));
 const view=render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 const run=document.querySelector<HTMLButtonElement>('[data-run]')!;await waitFor(()=>expect(run.disabled).toBe(false));
 fireEvent.click(run);fireEvent.click(run);
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(2));
 await waitFor(()=>expect(document.querySelector('[data-job-id="poll-peer"] [data-state]')?.getAttribute('data-state')).toBe('succeeded'),{timeout:2500});
 view.unmount();await act(async()=>finish(heldJob('late-peer','queued')));
});

test('overlapping fixed-seed Runs restart an invalidated in-flight peer poll while both IPCs remain pending',async()=>{
 vi.mocked(scanModels).mockResolvedValue(imageModels);
 localStorage.setItem('deas.pref.studio.run.seed','42');
 localStorage.setItem('deas.pref.studio.run.seedMode','"fixed"');
 const peer=heldJob('overlap-peer','queued');
 const succeeded=collection([{...peer,state:'succeeded' as const}]);
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([peer]));
 let finishPoll!:(value:ReturnType<typeof collection>)=>void;
 vi.mocked(pollWorkbench).mockReturnValueOnce(new Promise(resolve=>{finishPoll=resolve;})).mockResolvedValue(succeeded);
 const runs:((job:JobStatus)=>void)[]=[];
 vi.mocked(runWorkbenchJob).mockImplementation(()=>new Promise(resolve=>runs.push(resolve)));
 const view=render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 const run=document.querySelector<HTMLButtonElement>('[data-run]')!;
 await waitFor(()=>expect(run.disabled).toBe(false));
 vi.useFakeTimers();
 try{
  await act(async()=>fireEvent.click(run));
  expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
  await act(async()=>{await vi.advanceTimersByTimeAsync(1000);});
  expect(pollWorkbench).toHaveBeenCalledTimes(1);
  await act(async()=>fireEvent.click(run));
  expect(runWorkbenchJob).toHaveBeenCalledTimes(2);
  await act(async()=>finishPoll(succeeded));
  expect(document.querySelector('[data-job-id="overlap-peer"] [data-state]')?.getAttribute('data-state')).toBe('queued');
  await act(async()=>{await vi.advanceTimersByTimeAsync(1000);});
  expect(pollWorkbench).toHaveBeenCalledTimes(2);
  expect(document.querySelector('[data-job-id="overlap-peer"] [data-state]')?.getAttribute('data-state')).toBe('succeeded');
  // Neither Run reply has settled; observing a terminal peer must not submit or poll idle work.
  await act(async()=>{await vi.advanceTimersByTimeAsync(3000);});
  expect(runWorkbenchJob).toHaveBeenCalledTimes(2);
  expect(pollWorkbench).toHaveBeenCalledTimes(2);
  expect(readCurrentWorkbench).toHaveBeenCalledTimes(1);
 }finally{
  view.unmount();await act(async()=>runs.forEach((finish,i)=>finish(heldJob('overlap-'+i,'queued'))));
  vi.useRealTimers();
 }
});


const recentHistoryRecord=(intent:HistoryRecord['intent']):HistoryRecord=>({id:'historical-attempt',clientId:'historic-client',promptId:'historic-prompt',origin:'studio',appVersion:'0.1.48',admittedAt:1700000000000,observedAt:1700000001000,revision:3,state:'failed',intent,outputs:[],outputRoot:'/env/ComfyUI/output',backendOrigin:'http://127.0.0.1:7801',error:null});
const historicalImage={recipeId:'checkpoint-text-to-image-diagnostic',version:1,positivePrompt:'Exact historic prompt 猫',negativePrompt:'historic negative',checkpoint:'a.safetensors',seed:987654321,width:1216,height:832,steps:17,cfg:4.25,sampler:'euler',scheduler:'normal',outputPrefix:'raw/history/exact'} as const;
const openHistoryRecord=async()=>{
 fireEvent.click(screen.getByRole('button',{name:'Recent generations'}));
 await waitFor(()=>expect(document.querySelector('[data-history-id]')).not.toBeNull());
 fireEvent.click(document.querySelector('[data-history-id]')!);
 fireEvent.click(screen.getByRole('button',{name:'Use settings'}));
};
test('session cached targets never override explicit metadata, history or project restore and later edits win',async()=>{
 vi.mocked(scanModels).mockResolvedValue(sessionModels);vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 const intent={...historicalImage,checkpoint:'folder/a.safetensors'};
 const record=recentHistoryRecord(intent),before=JSON.stringify(record);
 const saved={...projectFixture,draft:{...projectFixture.draft,checkpoint:'folder/a.safetensors'}};
 vi.mocked(invoke).mockImplementation(async(command,args)=>{
  if(command==='native_jobs_history')return {schemaVersion:1,records:[record],readOnly:false,error:null};
  if(command==='native_project_open')return saved;
  if(command==='native_project_save')return {...saved,revision:2,draft:(args as {draft:typeof saved.draft}).draft};
  return {path:'/out',parent:'/',entries:[{name:'meta.png',path:'/out/meta.png',kind:'file',symlink:false,media:'image',size:1,modified:1}],truncated:false};
 });
 vi.mocked(readImageMetadata).mockResolvedValue([['parameters','metadata wins\nSteps: 7, Sampler: Euler, CFG scale: 3, Seed: 11, Size: 512x512, Model: folder/a.safetensors']]);
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',output:'/out'}}/></I18nProvider>);
 await screen.findByRole('button',{name:'meta.png'});sessionModel('folder/a.safetensors');
 sessionSettings({positive:'cached t2i',steps:39});choosePipe('Image → Image');sessionSettings({positive:'cached i2i',steps:40});choosePipe('Text → Image');sessionModel('other/a.safetensors');
 fireEvent.click(screen.getByRole('button',{name:'meta.png'}));fireEvent.click(document.querySelector('[data-more]')!);fireEvent.click(document.querySelector('[data-more-apply]')!);
 await waitFor(()=>expect(screen.getByLabelText('Prompt')).toHaveProperty('value','metadata wins'));
 expect(screen.getByLabelText('Steps')).toHaveProperty('value','7');sessionModel('other/a.safetensors');sessionModel('folder/a.safetensors');
 expect(screen.getByLabelText('Prompt')).toHaveProperty('value','metadata wins');
 sessionModel('other/a.safetensors');await openHistoryRecord();fireEvent.click(document.querySelector('[data-history-apply]')!);fireEvent.click(screen.getByRole('button',{name:'Close history'}));
 expect(screen.getByLabelText('Prompt')).toHaveProperty('value',intent.positivePrompt);expect(screen.getByLabelText('Steps')).toHaveProperty('value','17');
 sessionSettings({positive:'edited after history'});sessionModel('other/a.safetensors');sessionModel('folder/a.safetensors');
 expect(screen.getByLabelText('Prompt')).toHaveProperty('value','edited after history');
 fireEvent.click(screen.getByRole('button',{name:'Open project'}));
 await waitFor(()=>expect(screen.getByLabelText('Prompt')).toHaveProperty('value',saved.draft.positivePrompt));
 expect(screen.getByLabelText('Steps')).toHaveProperty('value','24');
 expect(JSON.stringify(record)).toBe(before);expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
});

test('history restores independent latent post settings without submitting or changing the record',async()=>{
 localStorage.setItem('deas.pref.studio.run.latent.h3',JSON.stringify({enabled:true,model:'h3-remembered.safetensors',scale:3,steps:2,denoise:0.3}));
 const original=recentHistoryRecord({recipeId:'video-generation',version:1,family:'ltx-2',positivePrompt:'historic video',negativePrompt:'',model:ltxModels[0].name,textEncoder:ltxModels[1].name,vae:ltxModels[3].name,seed:19,width:768,height:512,frames:121,frameRate:24,steps:8,cfg:1,sampler:'euler',scheduler:'simple',outputPrefix:'raw/video/old',output:{codec:'Auto',container:'Auto',bitDepth:'Auto',quality:20,audioCodec:'AAC',audioBitrate:'192k',saveFirstFrame:false,saveLastFrame:false},ltx:{passes:1,audioVae:ltxModels[4].name,textProjection:ltxModels[2].name},latentUpscale:{model:'ltx-held.safetensors',scale:2}});
 const before=JSON.stringify(original);
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'LatentUpscaleModelLoader','LTXVLatentUpsampler'],latentUpscaleModels:['ltx-held.safetensors'],textEncoders:[ltxModels[1].name,ltxModels[2].name],vaes:[ltxModels[3].name,ltxModels[4].name]}});
 vi.mocked(invoke).mockImplementation(async c=>c==='native_jobs_history'?{schemaVersion:1,records:[original],readOnly:false,error:null}:{path:'/env/input',parent:'/env',entries:[],truncated:false});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Text → Video').disabled).toBe(false));
 await openHistoryRecord();fireEvent.click(document.querySelector('[data-history-apply]')!);
 fireEvent.click(screen.getByRole('button',{name:'Close history'}));
 fireEvent.click(document.querySelector('[data-dock-tab="post"]')!);
 expect(document.querySelector('[data-latent-upscale-enabled]')).toHaveProperty('checked',true);
 expect(document.querySelector('[data-latent-upscale-model]')).toHaveProperty('value','ltx-held.safetensors');
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.latent.ltx')!)).toMatchObject({enabled:true,model:'ltx-held.safetensors',scale:2});
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.latent.h3')!)).toMatchObject({enabled:true,model:'h3-remembered.safetensors',scale:3});
 expect(JSON.stringify(original)).toBe(before);expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
});

test('history restores exact rich settings without probing or submitting and keeps native queue snapshots immutable',async()=>{
 const original=recentHistoryRecord({...historicalImage,loras:[{name:'style.safetensors',strength:0.75}],options:{attention:'sdpa',cache:{node:'EasyCache',threshold:0.03},compile:'inductor',vaeTile:1536},imageReference:{visionModel:'vision.safetensors',images:[{image:'second.png',strength:0.2},{image:'first.png',strength:0.9}]},refine:{width:2432,height:1664,denoise:0.35},imageFormat:'webp',imageCompression:83});
 const encoded=JSON.stringify(original);
 const live={...heldJob('existing-native-job','failed'),intent:{...historicalImage,positivePrompt:'Immutable existing run'}};
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([live]));
 const liveBefore=JSON.stringify(live);
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null},{category:'loras',name:'style.safetensors',family:'sdxl',path:'/m/lora',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,'DEAS_ImagePostprocessGate','ModelAttentionBackend','EasyCache','TorchCompileModel','VAEDecodeTiled','CLIPVisionLoader','LoadImage','CLIPVisionEncode','unCLIPConditioning'],attention:['sdpa'],compile:['inductor'],visionModels:['vision.safetensors'],images:['first.png','second.png']}});
 vi.mocked(invoke).mockImplementation(async c=>c==='native_jobs_history'?{schemaVersion:1,records:[original],readOnly:false,error:null}:{path:'/env/ComfyUI/input',parent:'/env',entries:['first.png','second.png'].map(name=>({name,path:'/env/ComfyUI/input/'+name,kind:'file',media:'image',size:1,modified:1,symlink:false})),truncated:false});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 const probes=vi.mocked(checkRecipePrerequisites).mock.calls.length;
 await openHistoryRecord();
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).not.toBe(historicalImage.positivePrompt);
 fireEvent.click(screen.getByRole('button',{name:'Apply partial settings'}));
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe(historicalImage.positivePrompt);
 expect((screen.getByRole('spinbutton',{name:'Seed'}) as HTMLInputElement).value).toBe(String(historicalImage.seed));
 expect(screen.getByRole('button',{name:'Fixed',pressed:true})).toBeTruthy();
 expect(document.querySelectorAll('[data-reference-controls]')).toHaveLength(2);
 expect(checkRecipePrerequisites).toHaveBeenCalledTimes(probes);expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();expect(uploadMask).not.toHaveBeenCalled();
 fireEvent.click(screen.getByRole('button',{name:'Close history'}));
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(true);
 expect(document.querySelectorAll('[data-reference-controls] img')).toHaveLength(0);
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 fireEvent.click(await screen.findByRole('button',{name:'second.png'}));
 fireEvent.click(document.querySelector('[data-history-reference-select="0"]')!);
 const transfer=new Map<string,string>();const data={clearData:()=>transfer.clear(),setData:(key:string,value:string)=>transfer.set(key,value),getData:(key:string)=>transfer.get(key)??'',get types(){return [...transfer.keys()];}};
 fireEvent.dragStart(screen.getByRole('button',{name:'first.png'}),{dataTransfer:data});
 fireEvent.drop(document.querySelectorAll('[data-reference-controls]')[1],{dataTransfer:data});
 expect(document.querySelectorAll('[data-reference-controls]')).toHaveLength(2);
 expect(document.querySelectorAll('[data-reference-controls] img')).toHaveLength(2);
 expect(checkRecipePrerequisites).toHaveBeenCalledTimes(probes);expect(uploadInput).not.toHaveBeenCalled();expect(uploadMask).not.toHaveBeenCalled();
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false);
 // This separate user Run is the first computation/transfer, using the restored order and strengths.
 vi.mocked(uploadInput).mockImplementation(async(_base,path)=>path.endsWith('second.png')?'uploaded-second.png':'uploaded-first.png');
 vi.mocked(runWorkbenchJob).mockResolvedValue({...heldJob('new-history-run','failed'),baseUrl:evidence.baseUrl});
 fireEvent.click(document.querySelector('[data-run]')!);
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({...historicalImage,loras:[{name:'style.safetensors',strength:0.75}],options:{attention:'sdpa',cache:{node:'EasyCache',threshold:0.03},compile:'inductor',vaeTile:1536},imageReference:{visionModel:'vision.safetensors',images:[{image:'uploaded-second.png',strength:0.2},{image:'uploaded-first.png',strength:0.9}]},refine:{width:2432,height:1664,denoise:0.35},imageFormat:'webp',imageCompression:83});
 expect(JSON.stringify(original)).toBe(encoded);expect(JSON.stringify(live)).toBe(liveBefore);
});
test('partial history restore clears unrelated input and keeps unresolved mask/reference gates blocking Run',async()=>{
 const original=recentHistoryRecord({...historicalImage,inputImage:'missing-input.png',inputMask:'deas-masks/missing.png',imageReference:{visionModel:'missing-vision.safetensors',images:[{image:'missing-ref.png',strength:0.4}]}});
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(invoke).mockImplementation(async c=>c==='native_jobs_history'?{schemaVersion:1,records:[original],readOnly:false,error:null}:{entries:[],truncated:false});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 await openHistoryRecord();expect(screen.getByText(/Cannot reconstruct.*inputMask/)).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'Apply partial settings'}));
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(true);
 expect(document.querySelector('[data-history-draft]')?.textContent).toContain('inputMask');
 expect(document.querySelector('[data-reference-controls]')?.textContent).toContain('missing-ref.png');
 expect(document.querySelector('.wb-editor-host')).toBeNull();
 fireEvent.click(screen.getByRole('button',{name:'Close history'}));
 fireEvent.click(document.querySelector('[data-run]')!);
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();expect(uploadMask).not.toHaveBeenCalled();
});
test('history authoring uses active project autosave and keeps failed-save unsaved protection',async()=>{
 const original=recentHistoryRecord(historicalImage);
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(invoke).mockImplementation(async c=>{if(c==='native_project_open')return projectFixture;if(c==='native_jobs_history')return {schemaVersion:1,records:[original],readOnly:false,error:null};if(c==='native_project_save')throw Error('disk full');return {entries:[],truncated:false};});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Open project'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Open project'}));
 await waitFor(()=>expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe(projectFixture.draft.positivePrompt));
 await openHistoryRecord();expect(screen.getByText(/active project keeps its autosave/)).toBeTruthy();
 fireEvent.click(screen.getByRole('button',{name:'Apply settings'}));
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_project_save',expect.objectContaining({projectId:projectFixture.id,draft:expect.objectContaining({positivePrompt:historicalImage.positivePrompt,seed:historicalImage.seed,width:historicalImage.width,height:historicalImage.height})})));
 expect(document.querySelector('#project-status-slot')?.textContent).not.toContain('Saved');
 const restart=new Event('deas:before-update-restart',{cancelable:true});window.dispatchEvent(restart);expect(restart.defaultPrevented).toBe(true);
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
});

test('history text-to-image keeps native-valid 1000 square canvas without family-preset snapping',async()=>{
 const original=recentHistoryRecord({...historicalImage,width:1000,height:1000});
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(invoke).mockImplementation(async c=>c==='native_jobs_history'?{schemaVersion:1,records:[original],readOnly:false,error:null}:{entries:[],truncated:false});
 vi.mocked(runWorkbenchJob).mockResolvedValue({...heldJob('exact-square-run','failed'),baseUrl:evidence.baseUrl});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 await openHistoryRecord();fireEvent.click(screen.getByRole('button',{name:'Apply settings'}));
 expect(runWorkbenchJob).not.toHaveBeenCalled();fireEvent.click(screen.getByRole('button',{name:'Close history'}));
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false);
 fireEvent.click(document.querySelector('[data-run]')!);
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({width:1000,height:1000,seed:historicalImage.seed});
});


test('Forge canvas handoff uses effective fixed dimensions and preserves immutable jobs until explicit Run',async()=>{
 for(const [key,value] of Object.entries({resolution:'fixed',width:1000,height:1000,seed:73,seedMode:'fixed',positive:'original draft',negative:'kept negative'}))localStorage.setItem('deas.pref.studio.run.'+key,JSON.stringify(value));
 const held={...heldJob('before-forge','failed'),intent:{...historicalImage,positivePrompt:'immutable job'}};
 const before=JSON.stringify(held);vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([held]));
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 const publish=vi.fn<(context:ForgeCanvasContext|undefined)=>void>();
 const props={endpoint:evidence.baseUrl,roots:{models:'/m'},onCanvasContext:publish};
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 const context=await waitFor(()=>{const value=publish.mock.calls.at(-1)?.[0];expect(value?.effective).toEqual({width:1000,height:1000});return value!;});
 expect(context.requiredGrid).toBe(8);expect(context.grid).toBe(64);expect(context.acceptsCanvas?.({width:1000,height:1000})).toBe(true);
 const probes=vi.mocked(checkRecipePrerequisites).mock.calls.length;
 // Publishing a read-only context never changes authoring or starts work.
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('original draft');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
 view.rerender(<I18nProvider><Workbench {...props} incoming={{positive:'sent landscape',negative:'',n:1,canvas:{contextKey:context.key,canvas:{width:1216,height:832}}}}/></I18nProvider>);
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('sent landscape');
 expect((screen.getByLabelText('Width') as HTMLInputElement).value).toBe('1216');expect((screen.getByLabelText('Height') as HTMLInputElement).value).toBe('832');
 expect((screen.getByLabelText('Resolution') as HTMLSelectElement).value).toBe('fixed');
 expect((screen.getByLabelText('Seed') as HTMLInputElement).value).toBe('73');
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.negative')!)).toBe('kept negative');
 expect(checkRecipePrerequisites).toHaveBeenCalledTimes(probes);expect(uploadInput).not.toHaveBeenCalled();expect(uploadMask).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();expect(JSON.stringify(held)).toBe(before);
 vi.mocked(runWorkbenchJob).mockResolvedValue({...heldJob('after-forge','failed'),baseUrl:evidence.baseUrl});
 fireEvent.click(document.querySelector('[data-run]')!);
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({positivePrompt:'sent landscape',negativePrompt:'kept negative',checkpoint:'a.safetensors',seed:73,width:1216,height:832});expect(JSON.stringify(held)).toBe(before);
});

test('Forge canvas rejects stale or invalid handoffs before changing text and keeps prompt-only callers compatible',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 localStorage.setItem('deas.pref.studio.run.positive',JSON.stringify('original'));
 const publish=vi.fn<(context:ForgeCanvasContext|undefined)=>void>();const props={endpoint:evidence.baseUrl,roots:{models:'/m'},onCanvasContext:publish};
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 const context=await waitFor(()=>{const value=publish.mock.calls.at(-1)?.[0];expect(value?.effective).not.toBeNull();expect(value?.acceptsCanvas?.({width:1024,height:1024})).toBe(true);return value!;});
 for(const [n,contextKey,canvas] of [[1,'stale-context',{width:1216,height:832}],[2,context.key,{width:8192,height:8192}]] as const){
  view.rerender(<I18nProvider><Workbench {...props} incoming={{positive:'must not apply',negative:'',n,canvas:{contextKey,canvas}}}/></I18nProvider>);
  expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('original');
 }
 view.rerender(<I18nProvider><Workbench {...props} incoming={{positive:'prompt-only',negative:'',n:3}}/></I18nProvider>);
 expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('prompt-only');
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();expect(uploadMask).not.toHaveBeenCalled();
});

test('Forge canvas uses actual inpaint source and preserves editor identity and the required mask gate',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(invoke).mockImplementation(async(_command,args)=>({path:(args as {path?:string})?.path??'/env/out',parent:'/env',entries:[{name:'wide.png',path:'/env/out/wide.png',kind:'file',symlink:false,media:'image',size:1,modified:1}],truncated:false}));
 const publish=vi.fn<(context:ForgeCanvasContext|undefined)=>void>();const props={endpoint:evidence.baseUrl,roots:{models:'/m',output:'/env/out'},onCanvasContext:publish};
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 fireEvent.click(await screen.findByRole('button',{name:'wide.png'}));adopt();
 const img=screen.getByRole('img',{name:'wide.png'});Object.defineProperties(img,{naturalWidth:{value:1024},naturalHeight:{value:640}});fireEvent.load(img);
 fireEvent.click(screen.getByRole('button',{name:'Inpaint'}));
 const context=await waitFor(()=>{const value=publish.mock.calls.at(-1)?.[0];expect(value?.effective).toEqual({width:1024,height:640});expect(value?.requiredGrid).toBe(64);return value!;});
 expect(context.acceptsCanvas?.({width:1216,height:832})).toBe(false);expect(context.acceptsCanvas?.({width:1024,height:640})).toBe(true);
 const editor=document.querySelector('.wb-editor-host');expect(editor).not.toBeNull();
 view.rerender(<I18nProvider><Workbench {...props} incoming={{positive:'inpaint draft',negative:'',n:1,canvas:{contextKey:context.key,canvas:{width:1024,height:640}}}}/></I18nProvider>);
 expect(document.querySelector('.wb-editor-host')).toBe(editor);expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe('inpaint draft');
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(true);expect(screen.getByRole('button',{name:'Inpaint',pressed:true})).toBeTruthy();
 expect(uploadInput).not.toHaveBeenCalled();expect(uploadMask).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test.each([['ltx-2',32],['minimax-h3',32]] as const)('Forge canvas retains required video alignment for %s without accepting an image-valid fixed square',async(family,grid)=>{
 for(const [key,value] of Object.entries({pipeline:'t2v',resolution:'fixed',width:1000,height:1000,checkpoint:'video.safetensors'}))localStorage.setItem('deas.pref.studio.run.'+key,JSON.stringify(value));
 vi.mocked(scanModels).mockResolvedValue([{category:'unet',name:'video.safetensors',family,path:'/m/video',size:1,info:null}]);
 const publish=vi.fn<(context:ForgeCanvasContext|undefined)=>void>();
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}} onCanvasContext={publish}/></I18nProvider>);
 await waitFor(()=>{const value=publish.mock.calls.at(-1)?.[0];expect(value?.grid).toBe(grid);expect(value?.requiredGrid).toBe(grid);expect(value?.effective).toBeNull();expect(value?.acceptsCanvas?.({width:1000,height:1000})).toBe(false);expect(value?.acceptsCanvas?.({width:1024,height:1024})).toBe(true);});
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();expect(uploadMask).not.toHaveBeenCalled();
});

test('Forge canvas handoff follows project autosave and retains failed-save unsaved protection',async()=>{
 const saved={...projectFixture,draft:{...projectFixture.draft,checkpoint:'a.safetensors',pipeline:'t2i'}};
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(invoke).mockImplementation(async command=>{
    if (command === 'native_audio_enhance_install_status') return { running: false, mode: '', stage: '', downloaded: 0, total: 0 };if(command==='native_project_open')return saved;if(command==='native_project_save')throw Error('disk full');return {entries:[],truncated:false};});
 const publish=vi.fn<(context:ForgeCanvasContext|undefined)=>void>();const props={endpoint:evidence.baseUrl,roots:{models:'/m'},onCanvasContext:publish};
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Open project'}).hasAttribute('disabled')).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Open project'}));
 await waitFor(()=>expect((screen.getByLabelText('Prompt') as HTMLTextAreaElement).value).toBe(saved.draft.positivePrompt));
 const context=await waitFor(()=>{const value=publish.mock.calls.at(-1)?.[0];expect(value?.key).toContain(saved.id);expect(value?.effective).toEqual({width:1024,height:1024});return value!;});
 view.rerender(<I18nProvider><Workbench {...props} incoming={{positive:'project landscape',negative:'',n:1,canvas:{contextKey:context.key,canvas:{width:1216,height:832}}}}/></I18nProvider>);
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('native_project_save',expect.objectContaining({projectId:saved.id,draft:expect.objectContaining({positivePrompt:'project landscape',width:1216,height:832,seed:42,checkpoint:'a.safetensors'})})));
 const restart=new Event('deas:before-update-restart',{cancelable:true});window.dispatchEvent(restart);expect(restart.defaultPrevented).toBe(true);
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();expect(uploadMask).not.toHaveBeenCalled();
});

const dropHistoryReference=(index:number,name:string,path:string)=>{
 const item={name,path,media:'image',src:'asset://localhost/'+path};
 fireEvent.drop(document.querySelectorAll('[data-reference-controls]')[index],{dataTransfer:{types:['application/x-deas-media'],getData:()=>JSON.stringify(item)}});
};
test('history source collision stays unresolved until explicit Media selection and invalidates again on input-root changes',async()=>{
 const original={...recentHistoryRecord({...historicalImage,inputImage:'same.png',denoise:0.6}),outputRoot:'/environment-B/output',outputRootIdentity:'output-is-not-input-provenance'};
 const before=JSON.stringify(original),endpoint='http://127.0.0.1:7801/backend-B/';
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',family:'sdxl',path:'/m/a',size:1,info:null}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,images:['same.png','unrelated.png']}});
 vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_jobs_history'?{schemaVersion:1,records:[original],readOnly:false,error:null}:{path:(args as {path?:string})?.path??'/environment-B/input',parent:'/environment-B',entries:['same.png','unrelated.png'].map(name=>({name,path:((args as {path?:string})?.path??'/environment-B/input')+'/'+name,kind:'file',media:'image',size:1,modified:1,symlink:false})),truncated:false});
 const props={endpoint,roots:{models:'/m',input:'/environment-B/input',output:'/environment-B/output'}};
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 fireEvent.click(await screen.findByRole('button',{name:'unrelated.png'}));adopt();
 const unrelated=screen.getByRole('img',{name:'unrelated.png'});Object.defineProperties(unrelated,{naturalWidth:{value:1216},naturalHeight:{value:832}});fireEvent.load(unrelated);
 const probes=vi.mocked(checkRecipePrerequisites).mock.calls.length;
 await openHistoryRecord();fireEvent.click(screen.getByRole('button',{name:'Apply partial settings'}));fireEvent.click(screen.getByRole('button',{name:'Close history'}));
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(true);expect(document.querySelector('.wb-editor-host')).toBeNull();
 expect(document.querySelector('[data-history-draft]')?.textContent).toContain('same.png');
 fireEvent.click(screen.getByRole('button',{name:'same.png'}));
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(true); // Preview alone is not source selection.
 adopt();const chosen=screen.getByRole('img',{name:'same.png'});Object.defineProperties(chosen,{naturalWidth:{value:1216},naturalHeight:{value:832}});fireEvent.load(chosen);
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false);
 expect(checkRecipePrerequisites).toHaveBeenCalledTimes(probes);expect(uploadInput).not.toHaveBeenCalled();expect(uploadMask).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();
 view.rerender(<I18nProvider><Workbench {...props} roots={{...props.roots,input:'/environment-C/input'}}/></I18nProvider>);
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(true);expect(document.querySelector('.wb-editor-host')).toBeNull();
 view.rerender(<I18nProvider><Workbench {...props}/></I18nProvider>);
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(true);expect(JSON.stringify(original)).toBe(before);
});
test('history native references require explicit ordered replacement and reselect after same-origin backend-prefix changes',async()=>{
 const original=recentHistoryRecord({...historicalImage,checkpoint:'qwen.safetensors',width:1024,height:1024,diffusion:{textEncoder:'qwen3vl_8b.safetensors',clipType:'qwen_image',vae:'qwen_image_2.1_vae.safetensors'},nativeImageReference:{method:'qwen-image-2.1',images:['same.png','second.png']}});
 const before=JSON.stringify(original),input='/environment-B/input';
 vi.mocked(scanModels).mockResolvedValue(referenceModels());
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,...referenceNodes,'UNETLoader','CLIPLoader','VAELoader'],images:['same.png','second.png'],textEncoders:['qwen3vl_8b.safetensors'],vaes:['qwen_image_2.1_vae.safetensors']}});
 vi.mocked(invoke).mockImplementation(async command=>command==='native_jobs_history'?{schemaVersion:1,records:[original],readOnly:false,error:null}:{entries:[],truncated:false});
 const props={endpoint:'http://127.0.0.1:7801/backend-A/',roots:{models:'/m',input}};
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 await waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false));
 const probes=vi.mocked(checkRecipePrerequisites).mock.calls.length;
 await openHistoryRecord();fireEvent.click(screen.getByRole('button',{name:'Apply partial settings'}));fireEvent.click(screen.getByRole('button',{name:'Close history'}));
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(true);expect(document.querySelectorAll('[data-reference-controls] img')).toHaveLength(0);
 for(const media of ['video','audio']){
  let readable=false;const transfer={types:['application/x-deas-media'],getData:()=>readable?JSON.stringify({name:'not-an-image',path:input+'/foreign.bin',media,src:'asset://foreign'}):'',dropEffect:'none'};
  fireEvent.dragOver(document.querySelectorAll('[data-reference-controls]')[0],{dataTransfer:transfer});expect(transfer.dropEffect).toBe('none');
  readable=true;fireEvent.drop(document.querySelectorAll('[data-reference-controls]')[0],{dataTransfer:transfer});
  expect(document.querySelectorAll('[data-reference-controls] img')).toHaveLength(0);
 }
 dropHistoryReference(0,'current-two.png',input+'/current-two.png');dropHistoryReference(1,'current-one.png',input+'/current-one.png');
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false);expect(screen.queryByLabelText('Reference strength')).toBeNull();
 expect(checkRecipePrerequisites).toHaveBeenCalledTimes(probes);expect(uploadInput).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();
 const nextEndpoint='http://127.0.0.1:7801/backend-B/';view.rerender(<I18nProvider><Workbench {...props} endpoint={nextEndpoint}/></I18nProvider>);
 await waitFor(()=>expect(checkRecipePrerequisites).toHaveBeenCalledTimes(probes+1));
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(true);expect(document.querySelectorAll('[data-reference-controls] img')).toHaveLength(0);
 dropHistoryReference(0,'current-two.png',input+'/current-two.png');dropHistoryReference(1,'current-one.png',input+'/current-one.png');
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false);
 vi.mocked(uploadInput).mockImplementation(async(_endpoint,path)=>path.endsWith('current-two.png')?'uploaded-two.png':'uploaded-one.png');
 vi.mocked(runWorkbenchJob).mockResolvedValue({...heldJob('explicit-history-reference','failed'),baseUrl:nextEndpoint});
 fireEvent.click(document.querySelector('[data-run]')!);await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][0]).toBe(nextEndpoint);
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({seed:historicalImage.seed,width:1024,height:1024,nativeImageReference:{method:'qwen-image-2.1',images:['uploaded-two.png','uploaded-one.png']}});
 expect(JSON.stringify(original)).toBe(before);
});
test('history video first and last frames remain unresolved until each explicit Media choice without upload or probes',async()=>{
 const original=recentHistoryRecord({recipeId:'video-generation',version:1,family:'ltx-2',positivePrompt:'historical video',negativePrompt:'',model:'LTX2/ltx.safetensors',textEncoder:'gemma_3_12B_it_fp8.safetensors',vae:'LTX2/LTX23_video_vae_bf16.safetensors',seed:43,width:1024,height:768,frames:49,frameRate:24,steps:20,cfg:5,sampler:'euler',scheduler:'normal',outputPrefix:'raw/history/video',output:{codec:'Auto',container:'Auto',bitDepth:'Auto',quality:20,audioCodec:'Auto',audioBitrate:'192k',saveFirstFrame:false,saveLastFrame:false},firstFrame:'same.png',lastFrame:'same-last.png',ltx:{passes:1,audioVae:'LTX2/LTX23_audio_vae_bf16.safetensors',textProjection:'ltx-2.3_text_projection_bf16.safetensors'}});
 const before=JSON.stringify(original),input='/environment-B/input';
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([{...heldJob('old-frame-result','succeeded'),outputRoot:'/environment-A/output',outputs:[{filename:'foreign-last.png',subfolder:'raw',type:'output'}]}]));
 vi.mocked(scanModels).mockResolvedValue([...ltxModels]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,images:['same.png','same-last.png'],textEncoders:['gemma_3_12B_it_fp8.safetensors','ltx-2.3_text_projection_bf16.safetensors'],vaes:['LTX2/LTX23_video_vae_bf16.safetensors','LTX2/LTX23_audio_vae_bf16.safetensors']}});
 vi.mocked(invoke).mockImplementation(async command=>command==='native_jobs_history'?{schemaVersion:1,records:[original],readOnly:false,error:null}:command==='native_media_star_status'?{starred:false,unavailable:null}:{path:input,parent:'/environment-B',entries:['same.png','same-last.png'].map(name=>({name,path:input+'/'+name,kind:'file',media:'image',size:1,modified:1,symlink:false})),truncated:false});
 render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801/backend-B/" roots={{models:'/m',input}}/></I18nProvider>);
 await waitFor(()=>expect(scanModels).toHaveBeenCalled());const probes=vi.mocked(checkRecipePrerequisites).mock.calls.length;
 await openHistoryRecord();fireEvent.click(screen.getByRole('button',{name:'Apply partial settings'}));fireEvent.click(screen.getByRole('button',{name:'Close history'}));
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(true);
 expect(document.querySelector('[data-history-draft]')?.textContent).toContain('same-last.png');
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 fireEvent.click(await screen.findByRole('button',{name:'same.png'}));adopt();
 const chosen=screen.getByRole('img',{name:'same.png'});Object.defineProperties(chosen,{naturalWidth:{value:1024},naturalHeight:{value:768}});fireEvent.load(chosen);
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(true);
 fireEvent.click(document.querySelector('[data-job-id="old-frame-result"] .wb-queue-select')!);
 fireEvent.click(within(document.querySelector<HTMLElement>('.wb-stage > [data-media-tools]')!).getByRole('button',{name:'Use as last frame'}));
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(true); // This LTX builder cannot resolve an old environment's frame handle.
 fireEvent.click(screen.getByRole('button',{name:'same-last.png'}));fireEvent.click(within(document.querySelector<HTMLElement>('.wb-stage > [data-media-tools]')!).getByRole('button',{name:'Use as last frame'}));
 expect(document.querySelector<HTMLButtonElement>('[data-run]')!.disabled).toBe(false);
 expect(checkRecipePrerequisites).toHaveBeenCalledTimes(probes);expect(uploadInput).not.toHaveBeenCalled();expect(uploadMask).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();expect(JSON.stringify(original)).toBe(before);
});

// ── YuE2 music ────────────────────────────────────────────────────────────────
const yue2Models=[
 {category:'checkpoints',name:'yue2_3b.safetensors',path:'/m/checkpoints/yue2_3b.safetensors',size:1,info:null,family:'yue2'},
 {category:'checkpoints',name:'a.safetensors',path:'/m/checkpoints/a.safetensors',size:1,info:null}] as const;
const musicNodes=['YuE2GenerateABC','YuE2GenerateMusic','ConditioningZeroOut','EmptyYuE2LatentAudio','VAEDecodeAudio','SaveAudioAdvanced','LoadAudio','AudioEncoderLoader','SheetSage2AudioToABC'];
const musicEvidence=(extra:Partial<typeof evidence.allowlist&{audioEncoders:string[];diffusionModels:string[];dualTextEncoders:string[];textEncoders:string[];vaes:string[]}>={})=>({...evidence,allowlist:{...evidence.allowlist,nodes:[...evidence.allowlist.nodes,...musicNodes],checkpoints:['yue2_3b.safetensors','a.safetensors'],samplers:[...evidence.allowlist.samplers,'dpm_2'],schedulers:[...evidence.allowlist.schedulers,'sgm_uniform'],audioEncoders:['sheetsage2_bf16.safetensors'],...extra}});
const musicJob=(id:string):JobStatus=>({id,baseUrl:'http://127.0.0.1:7801/',promptId:'p',state:'queued',outputs:[],error:null,generationReady:false});
async function mountMusic(pipeline:'Music (Text to Music)'|'Music (Cover Music)',planVisible=false,allow=musicEvidence()){
 vi.mocked(scanModels).mockResolvedValue([...yue2Models]);vi.mocked(checkRecipePrerequisites).mockResolvedValue(allow);
 if(planVisible)localStorage.setItem('deas.pref.studio.run.pipeline',JSON.stringify(pipeline==='Music (Cover Music)'?'cover':'t2m'));
 vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_media_star_status'?{starred:false,unavailable:null}:command==='native_jobs_history'?{schemaVersion:1,records:[],readOnly:false,error:null}:{path:(args as {path?:string})?.path??'/env/ComfyUI/input',parent:'/env',entries:mediaEntries,truncated:false});
 vi.mocked(runWorkbenchJob).mockImplementation(async()=>musicJob('job-m'));
 const view=render(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" root="/env" planVisible={planVisible} roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}}/></I18nProvider>);
 if(planVisible){await waitFor(()=>expect(document.querySelector('[data-plan-intent]')?.textContent).toContain('music-generation'));return view;}
 await waitFor(()=>expect(pipe(pipeline).disabled).toBe(false));choosePipe(pipeline);
 await waitFor(()=>expect((screen.getByRole('combobox',{name:'Checkpoint'}) as HTMLSelectElement).value).toBe('yue2_3b.safetensors'));
 return view;
}
const music=()=>({style:screen.getByRole('textbox',{name:'Style'}) as HTMLTextAreaElement,lyrics:screen.getByRole('textbox',{name:'Lyrics'}) as HTMLTextAreaElement});

test('remembered image refinement does not require its baseline gate for music',async()=>{
 localStorage.setItem('deas.pref.studio.run.post','true');
 await mountMusic('Music (Text to Music)');
 fireEvent.change(music().style,{target:{value:'Instrumental ambient'}});
 const run=screen.getByRole('button',{name:'Run'});
 await waitFor(()=>expect(run).toHaveProperty('disabled',false));
 expect(screen.queryByText(/Refine requires the DEAS baseline-first adapter/)).toBeNull();
 fireEvent.click(run);
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).not.toHaveProperty('refine');
});

test('text-to-music lists only YuE2 checkpoints, two music editors, no canvas/negative, and submits the music intent without uploads',async()=>{
 await mountMusic('Music (Text to Music)');
 const picker=screen.getByRole('combobox',{name:'Checkpoint'}) as HTMLSelectElement;
 expect([...picker.options].filter(o=>!o.disabled).map(o=>o.value)).toEqual(['yue2_3b.safetensors']);
 expect(screen.queryByRole('textbox',{name:'Prompt'})).toBeNull();expect(screen.queryByRole('textbox',{name:'Negative prompt'})).toBeNull();
 expect(screen.queryByLabelText('Width')).toBeNull();expect(screen.queryByLabelText('Aspect ratio')).toBeNull();
 expect(screen.getByLabelText('ABC Planning')).toHaveProperty('value','full');
 expect(screen.getByLabelText('Maximum duration (seconds)')).toHaveProperty('value','120');
 const run=screen.getByRole('button',{name:'Run'}) as HTMLButtonElement;
 await waitFor(()=>expect(run.title).toBe('Enter a song style.'));
 expect(run.disabled).toBe(true);
 // An empty Style is a draft hint, not an incompatible-dependency warning.
 expect(document.body.textContent).not.toMatch(/Unavailable or incompatible selections[^.]*Enter a song style/);
 fireEvent.change(music().style,{target:{value:'English, indie pop'}});fireEvent.change(music().lyrics,{target:{value:'[Verse]\nHello world'}});
 fireEvent.change(screen.getByLabelText('Seed'),{target:{value:'42'}});
 await waitFor(()=>expect(run.disabled).toBe(false));
 fireEvent.click(run);
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(uploadInput).not.toHaveBeenCalled();expect(uploadMask).not.toHaveBeenCalled();
 const intent=vi.mocked(runWorkbenchJob).mock.calls[0][1] as unknown as Record<string,unknown>;
 expect(intent).toMatchObject({recipeId:'music-generation',version:1,family:'yue2',mode:'t2m',planning:'full',checkpoint:'yue2_3b.safetensors',style:'English, indie pop',lyrics:'[Verse]\nHello world',seed:42,maxDuration:120,steps:32,cfg:1,sampler:'dpm_2',scheduler:'sgm_uniform',abc:{maxTokens:8192,temperature:0.7,topP:0.9,topK:30,repetitionPenalty:1.005,penaltyWindow:100},generation:{temperature:1,topP:0.95,topK:100,repetitionPenalty:1.2,cfgScale:1}});
 expect(intent.outputPrefix).toMatch(/^raw\/audio\//);
 for(const key of ['positivePrompt','negativePrompt','width','height','loras','options','sourceAudio','audioEncoder','imageFormat','refine'])expect(intent).not.toHaveProperty(key);
});

test('ABC Off drops the planner, switches AR guidance to 1.01 once, and keeps ABC values for returning to Full',async()=>{
 await mountMusic('Music (Text to Music)');
 fireEvent.change(music().style,{target:{value:'lofi'}});
 fireEvent.change(screen.getAllByLabelText('Maximum ABC tokens')[0],{target:{value:'2048'}});
 fireEvent.change(screen.getByLabelText('ABC Planning'),{target:{value:'off'}});
 expect(screen.queryByLabelText('Maximum ABC tokens')).toBeNull();
 const arCfg=screen.getByLabelText('Autoregressive guidance') as HTMLInputElement;
 expect(arCfg.value).toBe('1.01');
 fireEvent.change(arCfg,{target:{value:'2'}});
 fireEvent.change(screen.getByLabelText('ABC Planning'),{target:{value:'melody'}});
 fireEvent.change(screen.getByLabelText('ABC Planning'),{target:{value:'off'}});
 expect((screen.getByLabelText('Autoregressive guidance') as HTMLInputElement).value).toBe('2');
 await waitFor(()=>expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(false));
 fireEvent.click(screen.getByRole('button',{name:'Run'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 const off=vi.mocked(runWorkbenchJob).mock.calls[0][1] as unknown as Record<string,unknown>;
 expect(off).toMatchObject({mode:'t2m',planning:'off',generation:{cfgScale:2}});expect(off).not.toHaveProperty('abc');
 fireEvent.change(screen.getByLabelText('ABC Planning'),{target:{value:'full'}});
 expect((screen.getAllByLabelText('Maximum ABC tokens')[0] as HTMLInputElement).value).toBe('2048');
});

test('cover mode stage toolbar offers the same "Use for music cover" action as the media menu',async()=>{
 await mountMusic('Music (Cover Music)');
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 fireEvent.click(await screen.findByRole('button',{name:'voice.wav'}));
 const use=await waitFor(()=>{const b=document.querySelector('[data-media-tools] [data-media-action]') as HTMLButtonElement|null;expect(b).not.toBeNull();return b!;});
 expect(use.textContent).toBe('Use for music cover');
 expect(screen.queryByRole('button',{name:'Use as input'})).toBeNull();
 fireEvent.click(use);
 await waitFor(()=>expect(document.querySelector('[data-music-source]')?.textContent).toContain('voice.wav'));
 expect(markOf('voice.wav')).toBe('Cover source (audio or video)');
});

test('cover needs an explicitly selected audio or video source, uploads it once on Run, and is immutable to later edits',async()=>{
 vi.mocked(uploadInput).mockImplementation(async(_b,path)=>`deas-input-${path.split('/').pop()}`);
 await mountMusic('Music (Cover Music)');
 fireEvent.change(music().style,{target:{value:'bossa nova'}});
 expect(screen.queryByLabelText('ABC Planning')).toBeNull();
 expect(screen.getByLabelText('SheetSage2 audio encoder')).toHaveProperty('value','sheetsage2_bf16.safetensors');
 expect(screen.getByLabelText('Maximum duration (seconds)')).toHaveProperty('value','360');
 const run=screen.getByRole('button',{name:'Run'}) as HTMLButtonElement;
 await waitFor(()=>expect(run.title).toBe('Select an audio file or a video with an audio track.'));
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 await screen.findByRole('button',{name:'voice.wav'});
 expect(menuOf('ref.png').queryByRole('menuitem',{name:'Use for music cover'})).toBeNull();fireEvent.keyDown(window,{key:'Escape'});
 fireEvent.click(menuOf('clip.mp4').getByRole('menuitem',{name:'Use for music cover'}));
 expect(markOf('clip.mp4')).toBe('Cover source (audio or video)');
 fireEvent.click(menuOf('voice.wav').getByRole('menuitem',{name:'Use for music cover'}));
 expect(markOf('clip.mp4')).toBeUndefined();expect(document.querySelector('[data-music-source]')!.textContent).toContain('voice.wav');
 expect(uploadInput).not.toHaveBeenCalled();
 await waitFor(()=>expect(run.disabled).toBe(false));
 fireEvent.click(run);
 // Edits during the awaited upload belong to the next job.
 fireEvent.change(music().style,{target:{value:'changed after run'}});fireEvent.click(menuOf('clip.mp4').getByRole('menuitem',{name:'Use for music cover'}));
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(uploadInput).toHaveBeenCalledExactlyOnceWith('http://127.0.0.1:7801','/env/ComfyUI/input/voice.wav');
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).toMatchObject({recipeId:'music-generation',mode:'cover',planning:'melody',style:'bossa nova',sourceAudio:'deas-input-voice.wav',audioEncoder:'sheetsage2_bf16.safetensors',maxDuration:360});
 expect(vi.mocked(runWorkbenchJob).mock.calls[0][1]).not.toHaveProperty('abc');
 expect(Object.isFrozen(vi.mocked(runWorkbenchJob).mock.calls[0][1])).toBe(true);
});

test('music without the cover nodes still offers text-to-music, and a missing encoder blocks only covers',async()=>{
 await mountMusic('Music (Text to Music)',false,musicEvidence({nodes:[...evidence.allowlist.nodes,...musicNodes.filter(n=>!['LoadAudio','AudioEncoderLoader','SheetSage2AudioToABC'].includes(n))],audioEncoders:[]}));
 fireEvent.change(music().style,{target:{value:'jazz'}});
 await waitFor(()=>expect((screen.getByRole('button',{name:'Run'}) as HTMLButtonElement).disabled).toBe(false));
 choosePipe('Music (Cover Music)');
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 await screen.findByRole('button',{name:'voice.wav'});
 fireEvent.click(menuOf('voice.wav').getByRole('menuitem',{name:'Use for music cover'}));
 const run=screen.getByRole('button',{name:'Run'}) as HTMLButtonElement;
 await waitFor(()=>expect(run.title).toContain('SheetSage2AudioToABC'));
 expect(run.disabled).toBe(true);expect(run.title).toContain('LoadAudio');expect(run.title).toContain('Select an installed SheetSage2 audio encoder.');
});

test('switching t2i → music → t2i keeps image prompts and music texts as separate drafts and blocks Forge handoff while authoring music',async()=>{
 const view=await mountMusic('Music (Text to Music)');
 fireEvent.change(music().style,{target:{value:'synthwave'}});fireEvent.change(music().lyrics,{target:{value:'[Chorus]\nneon'}});
 choosePipe('Text → Image');
 await waitFor(()=>expect((screen.getByRole('combobox',{name:'Checkpoint'}) as HTMLSelectElement).value).toBe('a.safetensors'));
 expect(screen.getByRole('textbox',{name:'Prompt'})).toHaveProperty('value','');
 fireEvent.change(screen.getByRole('textbox',{name:'Prompt'}),{target:{value:'a cat'}});fireEvent.change(screen.getByRole('textbox',{name:'Negative prompt'}),{target:{value:'blurry'}});
 choosePipe('Music (Text to Music)');
 await waitFor(()=>expect(screen.getByRole('textbox',{name:'Style'})).toHaveProperty('value','synthwave'));
 expect(music().lyrics.value).toBe('[Chorus]\nneon');
 expect(screen.queryByRole('textbox',{name:'Negative prompt'})).toBeNull();
 view.rerender(<I18nProvider><Workbench endpoint="http://127.0.0.1:7801" root="/env" roots={{models:'/m',input:'/env/ComfyUI/input',output:'/env/ComfyUI/output'}} incoming={{positive:'from forge',negative:'forge negative',n:1}}/></I18nProvider>);
 expect(music().style.value).toBe('synthwave');expect(music().lyrics.value).toBe('[Chorus]\nneon');
 expect(screen.getByRole('alert').textContent).toContain('Prompt Forge sends image prompts');
 choosePipe('Text → Image');
 await waitFor(()=>expect(screen.getByRole('textbox',{name:'Prompt'})).toHaveProperty('value','a cat'));
 expect(screen.getByRole('textbox',{name:'Negative prompt'})).toHaveProperty('value','blurry');
 choosePipe('Music (Text to Music)');
 await waitFor(()=>expect(screen.getByRole('textbox',{name:'Style'})).toHaveProperty('value','synthwave'));
});

test('Run Plan in music mode shows the music stages from the captured intent and never uploads or submits',async()=>{
 await mountMusic('Music (Cover Music)',true);
 const plan=screen.getByRole('region',{name:'Run Plan'});
 expect(within(plan).getByText('Music (Cover Music)')).toBeTruthy();
 for(const stage of ['input','model','prompt','abc','generate','output'])expect(plan.querySelector(`[data-plan-stage="${stage}"]`)).not.toBeNull();
 expect(plan.querySelector('[data-plan-stage="refine"]')).toBeNull();
 const intent=JSON.parse(plan.querySelector('[data-plan-intent]')!.textContent!);
 expect(intent).toMatchObject({recipeId:'music-generation',mode:'cover',planning:'melody',sourceAudio:''});
 expect(intent).not.toHaveProperty('positivePrompt');expect(intent).not.toHaveProperty('width');
 expect(uploadInput).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test.each([false,true])('audio enhancement exports the clicked output and matching filename (paired=%s)',async paired=>{
 const job:JobStatus={...musicJob('audio-pair'),state:'succeeded',outputRoot:'/env/ComfyUI/output',outputs:[{filename:'first.flac',subfolder:'raw/audio',type:'output',...(paired?{role:'original' as const}:{})},{filename:'second.flac',subfolder:'raw/audio',type:'output',...(paired?{role:'processed' as const}:{})}]};
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
 await mountMusic('Music (Text to Music)');
 await waitFor(()=>expect(document.querySelectorAll('[data-job-id="audio-pair"] .wb-queue-select').length).toBe(2));
 fireEvent.click(document.querySelectorAll('[data-job-id="audio-pair"] .wb-queue-select')[1]);
 fireEvent.click(document.querySelector('[data-dock-tab="post"]')!);
 expect(document.querySelector('[data-audio-enhance-source]')?.textContent).toBe('second.flac');
 vi.mocked(invoke).mockImplementation(async command=>command==='native_audio_enhance_capabilities'?{clean:true,deep:false,stems:false}:null);
 await act(async()=>{fireEvent.focus(document.querySelector('[data-audio-enhance-mode]')!);});
 fireEvent.change(document.querySelector('[data-audio-enhance-mode]')!,{target:{value:'clean'}});
 await act(async()=>{fireEvent.click(document.querySelector('[data-audio-enhance-run]')!);});
 expect(invoke).toHaveBeenCalledWith('native_audio_enhance_export',{request:expect.objectContaining({jobId:'audio-pair',index:1})});
});

test('a retained music job renders a controls-only audio player, audio queue glyph, and music metadata',async()=>{
 const job:JobStatus={...musicJob('job-m'),state:'succeeded',outputRoot:'/env/ComfyUI/output',outputs:[{filename:'YuE2_00001.flac',subfolder:'raw/audio',type:'output'}],intent:{recipeId:'music-generation',version:1,family:'yue2',mode:'t2m',planning:'off',checkpoint:'yue2_3b.safetensors',style:'retained style',lyrics:'',seed:1,maxDuration:30,steps:32,cfg:1,sampler:'dpm_2',scheduler:'sgm_uniform',generation:{temperature:1,topP:0.95,topK:100,repetitionPenalty:1.2,cfgScale:1.01},outputPrefix:'raw/audio/x'}};
 vi.mocked(readCurrentWorkbench).mockResolvedValue(collection([job]));
 await mountMusic('Music (Text to Music)');
 await waitFor(()=>expect(document.querySelector('[data-job-id="job-m"]')).not.toBeNull());
 fireEvent.click(document.querySelector('[data-job-id="job-m"] .wb-queue-select')!);
 const player=await waitFor(()=>{const el=document.querySelector<HTMLAudioElement>('.wb-shown [data-output-audio]');expect(el).not.toBeNull();return el!;});
 expect(player.getAttribute('src')).toBe('asset://localhost//env/ComfyUI/output/raw/audio/YuE2_00001.flac');
 expect(player.hasAttribute('controls')).toBe(true);expect(player.hasAttribute('autoplay')).toBe(false);
 expect(document.querySelector('.wb-shown img')).toBeNull();
 expect(document.querySelector('[data-job-id="job-m"] .wb-thumb')!.textContent).toBe('♪');
 fireEvent.click(document.querySelector('[data-metadata-toggle]')!);
 const meta=document.querySelector('[data-music-metadata]')!;
 expect(meta.textContent).toContain('retained style');expect(meta.textContent).toContain('Off');expect(meta.textContent).not.toContain('Negative');
});

test.each(['off','clean','deep','stems'] as const)('Recent history restores audio selection %s without stale preferences or implicit installation',async mode=>{
 const record=recentHistoryRecord({recipeId:'music-generation',version:1,family:'yue2',mode:'t2m',planning:'off',checkpoint:'yue2_3b.safetensors',style:'historic audio',lyrics:'',seed:777,maxDuration:30,steps:20,cfg:1,sampler:'dpm_2',scheduler:'sgm_uniform',generation:{temperature:1,topP:0.95,topK:100,repetitionPenalty:1.2,cfgScale:1},outputPrefix:'raw/audio/old',...(mode==='off'?{}:{audioEnhance:{mode,profile:mode==='deep'?'speech' as const:'mix' as const}})});
 await mountMusic('Music (Text to Music)');
 vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_audio_enhance_capabilities'?{clean:true,deep:true,stems:true}:command==='native_jobs_history'?{schemaVersion:1,records:[record],readOnly:false,error:null}:command==='native_media_star_status'?{starred:false,unavailable:null}:{path:(args as {path?:string})?.path??'/env/ComfyUI/input',parent:'/env',entries:mediaEntries,truncated:false});
 fireEvent.click(screen.getByRole('tab',{name:'Post-Processing'}));
 await act(async()=>{fireEvent.focus(document.querySelector('[data-audio-enhance-mode]')!);});
 fireEvent.change(document.querySelector('[data-audio-enhance-mode]')!,{target:{value:mode==='stems'?'clean':'stems'}});
 await openHistoryRecord();fireEvent.click(document.querySelector('[data-history-apply]')!);fireEvent.click(screen.getByRole('button',{name:'Close history'}));
 fireEvent.click(screen.getByRole('tab',{name:'Post-Processing'}));
 expect(document.querySelector('[data-audio-enhance-mode]')).toHaveProperty('value',mode);
 expect(document.querySelector('[data-audio-enhance-profile]')).toHaveProperty('value',mode==='deep'?'speech':'mix');
 if(mode==='stems')expect(document.querySelector('[data-audio-enhance-device]')).toHaveProperty('value','auto');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
 expect(invoke).not.toHaveBeenCalledWith('native_audio_enhance_install',expect.anything());
});

test('Recent history restores a music record into the cover pipeline with every setting and an unresolved source',async()=>{
 const record=recentHistoryRecord({recipeId:'music-generation',version:1,family:'yue2',mode:'cover',planning:'melody',checkpoint:'yue2_3b.safetensors',style:'historic style',lyrics:'[Verse]\nold words',seed:777,maxDuration:200.04,steps:20,cfg:1,sampler:'dpm_2',scheduler:'sgm_uniform',generation:{temperature:0.8,topP:0.9,topK:50,repetitionPenalty:1.1,cfgScale:1.5},outputPrefix:'raw/audio/old',sourceAudio:'deas-input-old.wav',audioEncoder:'sheetsage2_bf16.safetensors'});
 await mountMusic('Music (Text to Music)');
 vi.mocked(invoke).mockImplementation(async(command,args)=>command==='native_jobs_history'?{schemaVersion:1,records:[record],readOnly:false,error:null}:command==='native_media_star_status'?{starred:false,unavailable:null}:{path:(args as {path?:string})?.path??'/env/ComfyUI/input',parent:'/env',entries:mediaEntries,truncated:false});
 fireEvent.change(music().style,{target:{value:'current draft'}});
 await openHistoryRecord();fireEvent.click(document.querySelector('[data-history-apply]')!);fireEvent.click(screen.getByRole('button',{name:'Close history'}));
 await waitFor(()=>expect((screen.getByRole('combobox',{name:'Pipeline'}) as HTMLSelectElement).value).toBe('cover'));
 expect(music().style.value).toBe('historic style');expect(music().lyrics.value).toBe('[Verse]\nold words');
 expect(screen.getByLabelText('Seed')).toHaveProperty('value','777');expect(screen.getByLabelText('Steps')).toHaveProperty('value','20');
 expect(screen.getByLabelText('Maximum duration (seconds)')).toHaveProperty('value','200.04');
 expect((screen.getByLabelText('Autoregressive guidance') as HTMLInputElement).value).toBe('1.5');
 const run=screen.getByRole('button',{name:'Run'}) as HTMLButtonElement;
 expect(run.disabled).toBe(true);expect(run.title).toContain('deas-input-old.wav');
 fireEvent.click(within(screen.getByRole('group',{name:'Environment folders'})).getByRole('button',{name:'Input'}));
 await screen.findByRole('button',{name:'voice.wav'});
 fireEvent.click(menuOf('voice.wav').getByRole('menuitem',{name:'Use for music cover'}));
 await waitFor(()=>expect(run.disabled).toBe(false));
 expect(uploadInput).not.toHaveBeenCalled();expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('variable chips show picks and remove exactly that token occurrence from the prompt',async()=>{
 vi.mocked(checkRecipePrerequisites).mockResolvedValue({...evidence,allowlist:{...evidence.allowlist,checkpoints:['m.safetensors'],nodes:[...evidence.allowlist.nodes,'DaSiWa_WildcardPresetPromptBuilder']}});
 localStorage.setItem('deas.pref.studio.run.wildcardEnabled',JSON.stringify(true));
 localStorage.setItem('deas.pref.studio.run.wildcard',JSON.stringify({style:'Booru',seed:7,reroll:3,token_budget:200,auto_reroll:false}));
 localStorage.setItem('deas.pref.studio.run.customVariables',JSON.stringify({color:['red','blue']}));
 vi.mocked(invoke).mockImplementation(async c=>c==='native_wildcard_library'?JSON.stringify({categories:{}}):{entries:[],truncated:false});
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'m.safetensors',path:'/m/checkpoints/m.safetensors',size:1,info:null}]);
 vi.mocked(runWorkbenchJob).mockResolvedValue({id:'chip-job',baseUrl:evidence.baseUrl,promptId:'p',state:'succeeded',outputs:[],error:null,generationReady:false,intent:undefined as never});
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'}).hasAttribute('disabled')).toBe(false));
 const positive=screen.getByLabelText('Prompt') as HTMLTextAreaElement;
 fireEvent.change(positive,{target:{value:'a $color cat with ${color}-light and $color'}});
 const chips=await screen.findAllByRole('button',{name:/^\$color/});
 expect(chips.length).toBe(3);
 fireEvent.click(screen.getAllByRole('button',{name:'Remove color from the prompt'})[0]);
 await waitFor(()=>expect(positive.value).toBe('a  cat with ${color}-light and $color'));
 fireEvent.click(screen.getAllByRole('button',{name:'Remove color from the prompt'})[0]);
 await waitFor(()=>expect(positive.value).toBe('a  cat with -light and $color'));
});

test('beginner submits through the real Workbench adapter and restores advanced technical settings',async()=>{
 localStorage.setItem('deas.pref.studio.run.cfg','7');localStorage.setItem('deas.pref.studio.run.negative',JSON.stringify('advanced negative'));
 vi.mocked(scanModels).mockResolvedValue([{name:'a.safetensors',category:'checkpoints',path:'/m/a',size:1,info:null,family:'sdxl'}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(runWorkbenchJob).mockResolvedValue({...heldJob('beginner','failed'),error:'fixture'});
 const props={endpoint:evidence.baseUrl,roots:{models:'/m'}};
 const view=render(<I18nProvider><Workbench {...props} beginner/></I18nProvider>);
 await waitFor(()=>expect(captureStudioSettings()['studio.run.cfg']).toBe(3.5));
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.cfg')!)).toBe(7);
 fireEvent.change(document.querySelector('[data-run-positive]')!,{target:{value:'A house by a lake'}});
 const button=screen.getByRole('button',{name:'Run'});await waitFor(()=>expect(button).toHaveProperty('disabled',false));
 expect(button.closest('#studio-queue')).not.toBeNull();
 expect(document.querySelector('.beginner-studio-heading')).toBeNull();
 expect(document.querySelector('.beginner-local-badge')).toBeNull();
 expect(document.querySelector('.beginner-composer-top')).toBeNull();
 expect(document.querySelector('[data-run-positive]')!.getAttribute('placeholder')).toBe('Describe what you imagine…');
 expect(document.querySelector('.beginner-composer')!.closest('#studio-settings')).not.toBeNull();
 expect(document.querySelector('#project-actions-slot')!.hasAttribute('hidden')).toBe(true);
 expect(document.querySelector('#project-status-slot')!.hasAttribute('hidden')).toBe(true);
 expect(screen.getByRole('heading',{name:'What will you bring to life?'})).toBeTruthy();
 expect(document.querySelector('.beginner-empty-preview')!.textContent).toContain('watch your imagination take shape here');
 expect(document.querySelectorAll('[data-run-positive]')).toHaveLength(1);
 expect(document.querySelector('[data-run-positive]')!.closest('.wb-center')).not.toBeNull();
 expect(screen.getByRole('button',{name:'Recent generations'}).closest('#studio-queue')).not.toBeNull();
 expect(runWorkbenchJob).not.toHaveBeenCalled();fireEvent.click(button);
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.lastCall![1]).toMatchObject({positivePrompt:'A house by a lake',negativePrompt:'',checkpoint:'a.safetensors',cfg:3.5,steps:28,sampler:'euler',scheduler:'normal'});
 view.rerender(<I18nProvider><Workbench {...props} beginner={false}/></I18nProvider>);
 await waitFor(()=>expect(captureStudioSettings()['studio.run.cfg']).toBe(7));
 expect(captureStudioSettings()['studio.run.negative']).toBe('advanced negative');
 expect((document.querySelector('[data-run-positive]') as HTMLTextAreaElement).value).toBe('A house by a lake');
});

test('beginner model selection keeps the prompt and changes the actual Studio model without executing',async()=>{
 vi.mocked(scanModels).mockResolvedValue(['a','b'].map(n=>({name:n+'.safetensors',category:'checkpoints',path:'/m/'+n,size:1,info:null,family:'sdxl'})));
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);const onPage=vi.fn();
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}} beginner beginnerPage="models" onBeginnerPage={onPage}/></I18nProvider>);
 await waitFor(()=>expect(screen.getAllByRole('button',{name:/Use this model/})).toHaveLength(2));
 fireEvent.change(document.querySelector('[data-run-positive]')!,{target:{value:'Keep this idea'}});
 fireEvent.click(screen.getAllByRole('button',{name:/Use this model/})[1]);
 await waitFor(()=>expect(captureStudioSettings()['studio.run.checkpoint']).toBe('b.safetensors'));
 expect(captureStudioSettings()['studio.run.positive']).toBe('Keep this idea');expect(onPage).toHaveBeenCalledWith('generate');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('beginner LoRAs use known matching families, native strengths and Ctrl+Enter without overwriting Advanced',async()=>{
 const advanced=[{name:'advanced-only.safetensors',strength:0.4}];localStorage.setItem('deas.pref.studio.run.loras',JSON.stringify(advanced));
 vi.mocked(scanModels).mockResolvedValue([
  {name:'a.safetensors',category:'checkpoints',path:'/m/a',size:1,info:null,family:'sdxl'},
  {name:'watercolor.safetensors',category:'loras',path:'/m/watercolor',size:1,info:null,family:'sdxl'},
  {name:'other.safetensors',category:'loras',path:'/m/other',size:1,info:null,family:'anima'},
  {name:'unknown.safetensors',category:'loras',path:'/m/unknown',size:1,info:null}
 ]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);vi.mocked(runWorkbenchJob).mockResolvedValue({...heldJob('beginner-lora','failed'),error:'fixture'});
 const props={endpoint:evidence.baseUrl,roots:{models:'/m'}};const view=render(<I18nProvider><Workbench {...props} beginner/></I18nProvider>);
 fireEvent.click(screen.getByText('LoRAs (optional)'));
 await waitFor(()=>expect(screen.getByRole('option',{name:'watercolor'})).toBeDefined());
 expect(screen.queryByRole('option',{name:'other'})).toBeNull();expect(screen.queryByRole('option',{name:'unknown'})).toBeNull();
 fireEvent.change(screen.getByRole('combobox',{name:'Add a LoRA…'}),{target:{value:'watercolor.safetensors'}});
 fireEvent.change(screen.getByRole('spinbutton',{name:/watercolor/}),{target:{value:'0.65'}});
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.loras')!)).toEqual(advanced);
 const run=screen.getByRole('button',{name:'Run'});await waitFor(()=>expect(run).toHaveProperty('disabled',false));
 fireEvent.keyDown(window,{key:'Enter',ctrlKey:true});await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 expect(vi.mocked(runWorkbenchJob).mock.lastCall![1]).toMatchObject({family:'sdxl',loras:[{name:'watercolor.safetensors',strength:0.65}]});
 view.rerender(<I18nProvider><Workbench {...props} beginner={false}/></I18nProvider>);await waitFor(()=>expect(captureStudioSettings()['studio.run.loras']).toEqual(advanced));
 view.rerender(<I18nProvider><Workbench {...props} beginner/></I18nProvider>);fireEvent.click(screen.getByText('LoRAs (optional)'));await waitFor(()=>expect(screen.getByRole('spinbutton',{name:/watercolor/})).toHaveProperty('value','0.65'));
 fireEvent.click(screen.getByRole('button',{name:'Remove watercolor.safetensors'}));expect(screen.queryByRole('spinbutton',{name:/watercolor/})).toBeNull();
});

test('beginner format and quality reuse the shared aspect and resolution options without a fork',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{name:'a.safetensors',category:'checkpoints',path:'/m/a',size:1,info:null,family:'sdxl'}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 const held=heldJob('beginner-size','queued');vi.mocked(runWorkbenchJob).mockResolvedValue(held);
 const view=render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}} beginner/></I18nProvider>);
 await waitFor(()=>expect(captureStudioSettings()['studio.run.cfg']).toBe(3.5));
 const aspect=document.querySelector('[data-beginner-aspect]')! as HTMLSelectElement;
 const resolution=document.querySelector('[data-beginner-resolution]')! as HTMLSelectElement;
 // same shared option sets as the advanced panel, no beginner-specific values
 expect(Array.from(aspect.options).map(o=>o.value)).toEqual(aspects.filter(a=>a!=='custom'&&a!=='auto'));
 expect(Array.from(resolution.options).map(o=>o.value)).toEqual(['model',...Object.keys(resolutions)]);
 fireEvent.change(aspect,{target:{value:'3:4'}});fireEvent.change(resolution,{target:{value:'1024p'}});
 expect(captureStudioSettings()['studio.run.aspect']).toBe('3:4');expect(captureStudioSettings()['studio.run.resolution']).toBe('1024p');
 fireEvent.change(document.querySelector('[data-run-positive]')!,{target:{value:'A house by a lake'}});
 const button=screen.getByRole('button',{name:'Run'});await waitFor(()=>expect(button).toHaveProperty('disabled',false));
 fireEvent.click(button);
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 // model-exact: SDXL 3:4 at 1 MP snaps to 896x1152 on the family grid
 expect(vi.mocked(runWorkbenchJob).mock.lastCall![1]).toMatchObject({width:896,height:1152});
 // shared canvas intent survives the round-trip into advanced
 view.rerender(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}} beginner={false}/></I18nProvider>);
 expect(captureStudioSettings()['studio.run.aspect']).toBe('3:4');expect(captureStudioSettings()['studio.run.resolution']).toBe('1024p');
});

test('beginner resizes and collapses the prompt below preview without changing the Advanced media dock',async()=>{
 vi.mocked(scanModels).mockResolvedValue([{name:'a.safetensors',category:'checkpoints',path:'/m/a',size:1,info:null,family:'sdxl'}]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 localStorage.setItem('deas.pref.studio.dock.height','300');
 const props={endpoint:evidence.baseUrl,roots:{models:'/m'}};
 const view=render(<I18nProvider><Workbench {...props} beginner/></I18nProvider>);
 await waitFor(()=>expect(captureStudioSettings()['studio.run.cfg']).toBe(3.5));
 const dock=document.getElementById('studio-dock')!,prompts=document.getElementById('studio-prompts')!;
 expect(dock.hidden).toBe(true);
 const handle=screen.getByRole('separator',{name:'Resize prompt area'});
 expect(handle.getAttribute('aria-controls')).toBe('studio-prompts');
 fireEvent.keyDown(handle,{key:'ArrowUp'});expect(handle.getAttribute('aria-valuenow')).toBe('126');
 fireEvent.change(document.querySelector('[data-run-positive]')!,{target:{value:'Keep this prompt'}});
 fireEvent.click(screen.getByRole('button',{name:'Collapse prompt area'}));expect(prompts.hidden).toBe(true);
 fireEvent.keyDown(handle,{key:'ArrowUp'});expect(handle.getAttribute('aria-valuenow')).toBe('126');
 fireEvent.click(screen.getByRole('button',{name:'Expand prompt area'}));expect(prompts.hidden).toBe(false);
 expect(document.querySelector('[data-run-positive]')).toHaveProperty('value','Keep this prompt');
 act(()=>window.dispatchEvent(new Event('deas:reset-studio-layout')));
 expect(handle.getAttribute('aria-valuenow')).toBe('110');
 expect(screen.getByRole('button',{name:'Collapse prompt area'}).getAttribute('aria-expanded')).toBe('true');
 view.rerender(<I18nProvider><Workbench {...props} beginner={false}/></I18nProvider>);
 expect(dock.hidden).toBe(false);expect(prompts.hidden).toBe(false);
 expect(screen.queryByRole('separator',{name:'Resize prompt area'})).toBeNull();
});

// ACE-Step authoring uses the shared music pipeline, never YuE2 conditioning.
const aceVariants=[
 ['ace-step-1-0',50,5,5],['ace-step-1-5-base',50,7,1],['ace-step-1-5-sft',50,7,1],['ace-step-1-5-turbo',8,1,3],
 ['ace-step-1-5-xl-base',50,6,3],['ace-step-1-5-xl-sft',50,7,3],['ace-step-1-5-xl-turbo',8,1,3],
] as const;
const aceModel=(family:string,category='checkpoints')=>({category,name:`${family}.safetensors`,path:`/m/${category}/${family}.safetensors`,size:1,info:null,family});
const aceNodes=['LatentOperationTonemapReinhard','LatentApplyOperationCFG','KSampler','ConditioningZeroOut','VAEDecodeAudio','SaveAudioAdvanced','CheckpointLoaderSimple','ModelSamplingSD3','ModelSamplingAuraFlow','TextEncodeAceStepAudio','EmptyAceStepLatentAudio','TextEncodeAceStepAudio1.5','EmptyAceStep1.5LatentAudio','UNETLoader','DualCLIPLoader','VAELoader'];
async function mountAce(category='checkpoints',missing:string[]=[]) {
 const ms=aceVariants.map(([f])=>aceModel(f,category));
 vi.mocked(scanModels).mockResolvedValue([...ms,aceModel('ace-step'),aceModel('ace-step-1-5'),aceModel('ace-step-1-5-xl'),...yue2Models]);
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(musicEvidence({nodes:aceNodes.filter(n=>!missing.includes(n)),checkpoints:ms.map(m=>m.name),diffusionModels:ms.map(m=>m.name)}));
 vi.mocked(invoke).mockImplementation(async command=>command==='native_media_star_status'?{starred:false,unavailable:null}:command==='native_jobs_history'?{schemaVersion:1,records:[],readOnly:false,error:null}:{path:'/env/ComfyUI/input',parent:'/env',entries:[],truncated:false});
 vi.mocked(runWorkbenchJob).mockImplementation(async()=>musicJob('job-ace'));
 const view=render(<I18nProvider><Workbench endpoint={evidence.baseUrl} root="/env" roots={{models:'/m'}}/></I18nProvider>);
 await waitFor(()=>expect(pipe('Music (Text to Music)').disabled).toBe(false));choosePipe('Music (Text to Music)');
 await screen.findByRole('textbox',{name:'Style'});
 return view;
}
const aceControl=(key:string)=>document.querySelector<HTMLInputElement|HTMLSelectElement>(`[data-ace-setting="${key}"]`)!;
const changeAceModel=(family:string)=>fireEvent.change(screen.getByRole('combobox',{name:'Checkpoint'}),{target:{value:`${family}.safetensors`}});
test.each(aceVariants)('ACE authoring %s submits variant defaults and no YuE2/image fields',async(family,steps,cfg,shift)=>{
 await mountAce();changeAceModel(family);
 const picker=screen.getByRole('combobox',{name:'Checkpoint'}) as HTMLSelectElement;
 for(const f of ['ace-step','ace-step-1-5','ace-step-1-5-xl'])expect([...picker.options].some(o=>o.value===`${f}.safetensors`)).toBe(false);
 expect(document.querySelector('[data-num="steps"]')).toHaveProperty('value',String(steps));expect(document.querySelector('[data-num="cfg"]')).toHaveProperty('value',String(cfg));
 expect(aceControl('shift')).toHaveProperty('value',String(shift));
 expect(screen.queryByLabelText('ABC Planning')).toBeNull();expect(document.querySelector('[data-music-generation-settings]')).toBeNull();expect(document.querySelector('.forge-hammer')).toBeNull();
 expect(screen.queryByRole('textbox',{name:'Negative prompt'})).toBeNull();expect(screen.queryByLabelText('Width')).toBeNull();
 if(family==='ace-step-1-0'){expect(aceControl('lyricsStrength')).toHaveProperty('value','1');expect(aceControl('bpm')).toBeNull();}
 else {expect(aceControl('generateAudioCodes')).toHaveProperty('checked',true);expect(aceControl('lmCfg')).toHaveProperty('value','2');expect(aceControl('temperature')).toHaveProperty('value','0.85');}
 fireEvent.change(music().style,{target:{value:'Ambient piano'}});fireEvent.change(music().lyrics,{target:{value:''}});
 const run=screen.getByRole('button',{name:'Run'});await waitFor(()=>expect(run).toHaveProperty('disabled',false));fireEvent.click(run);
 await waitFor(()=>expect(runWorkbenchJob).toHaveBeenCalledTimes(1));
 const intent=vi.mocked(runWorkbenchJob).mock.lastCall![1];expect(intent).toMatchObject({family,mode:'t2m',planning:'off',steps,cfg,sampler:'euler',scheduler:'simple',ace:{...defaultAceSettings,shift},style:'Ambient piano',lyrics:'',maxDuration:normalizeAceDuration(120,family)});
 for(const key of ['abc','generation','sourceAudio','audioEncoder','options','loras','diffusion'])expect(intent).not.toHaveProperty(key);
 expect(uploadInput).not.toHaveBeenCalled();
});
test('ACE model in unet remains selectable for Text to Music',async()=>{
 await mountAce('unet');changeAceModel('ace-step-1-5-turbo');
 expect(screen.getByRole('combobox',{name:'Checkpoint'})).toHaveProperty('value','ace-step-1-5-turbo.safetensors');
 expect(aceControl('shift')).toHaveProperty('value','3');
 expect(document.querySelector('[data-num="steps"]')).toHaveProperty('value','8');
 expect(screen.getByRole('combobox',{name:'Text encoder'})).toBeTruthy();
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('ACE initial auto-selection applies its profile before accepting authored overrides',async()=>{
 localStorage.setItem('deas.pref.studio.run.pipeline',JSON.stringify('t2m'));
 await mountAce();
 expect(document.querySelector('[data-num="steps"]')).toHaveProperty('value','50');
 fireEvent.change(document.querySelector('[data-num="steps"]')!,{target:{value:'21'}});
 expect(document.querySelector('[data-num="steps"]')).toHaveProperty('value','21');
 expect(runWorkbenchJob).not.toHaveBeenCalled();
});

test('ACE upgrades a remembered checkpoint to its profile without rewriting bound saved overrides',async()=>{
 localStorage.setItem('deas.pref.studio.run.pipeline',JSON.stringify('t2m'));
 localStorage.setItem('deas.pref.studio.run.checkpoint',JSON.stringify('ace-step-1-5-turbo.safetensors'));
 localStorage.setItem('deas.pref.studio.run.steps','20');
 const view=await mountAce();
 expect(document.querySelector('[data-num="steps"]')).toHaveProperty('value','8');
 fireEvent.change(document.querySelector('[data-num="steps"]')!,{target:{value:'14'}});
 view.unmount();
 await mountAce();
 expect(document.querySelector('[data-num="steps"]')).toHaveProperty('value','14');
});

test('ACE authoring keeps empty prompts and explicit per-model settings on revisits without submitting',async()=>{
 await mountAce();changeAceModel('ace-step-1-5-turbo');
 fireEvent.change(music().style,{target:{value:'will clear'}});fireEvent.change(music().lyrics,{target:{value:'will clear'}});
 fireEvent.change(aceControl('shift'),{target:{value:'4.5'}});fireEvent.change(document.querySelector('[data-num="steps"]')!,{target:{value:'11'}});fireEvent.click(aceControl('generateAudioCodes'));
 changeAceModel('ace-step-1-5-xl-sft');expect(aceControl('shift')).toHaveProperty('value','3');
 fireEvent.change(music().style,{target:{value:''}});fireEvent.change(music().lyrics,{target:{value:''}});
 changeAceModel('ace-step-1-5-turbo');expect(music().style.value).toBe('');expect(music().lyrics.value).toBe('');expect(aceControl('shift')).toHaveProperty('value','4.5');expect(aceControl('generateAudioCodes')).toHaveProperty('checked',false);expect(document.querySelector('[data-num="steps"]')).toHaveProperty('value','11');
 expect(runWorkbenchJob).not.toHaveBeenCalled();expect(uploadInput).not.toHaveBeenCalled();
});
test('ACE authoring missing actual conditioning node blocks Run without YuE2 requirements',async()=>{
 await mountAce('checkpoints',['TextEncodeAceStepAudio1.5']);changeAceModel('ace-step-1-5-turbo');fireEvent.change(music().style,{target:{value:'lofi'}});
 expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',true);expect(document.body.textContent).toContain('TextEncodeAceStepAudio1.5');expect(document.body.textContent).not.toContain('YuE2GenerateMusic');
});
test('Chat renders draw its own prompt and size with Generate settings and leave the Studio fields alone',async()=>{
 localStorage.setItem('deas.pref.studio.run.positive',JSON.stringify('my own studio prompt'));
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',path:'/m/a',family:'sdxl',size:1,info:null}]);
 const job:JobStatus={id:'chat-job',baseUrl:evidence.baseUrl,promptId:'prompt',state:'queued',outputs:[],error:null,generationReady:false};
 vi.mocked(runWorkbenchJob).mockResolvedValue(job);
 const onChatStudio=vi.fn(),onChatRender=vi.fn();
 const props={endpoint:evidence.baseUrl,roots:{models:'/m'},onChatStudio,onChatRender};
 const view=render(<I18nProvider><Workbench {...props}/></I18nProvider>);
 await waitFor(()=>expect(screen.getByRole('button',{name:'Run'})).toHaveProperty('disabled',false));
 // Chat learns what draws: the checkpoint and its Forge dialect, not blocked.
 await waitFor(()=>expect(onChatStudio).toHaveBeenLastCalledWith({checkpoint:'a.safetensors',dialect:'Illustrious'}));
 view.rerender(<I18nProvider><Workbench {...props} chatRender={{id:'r1',positive:'1girl, fox ears, snow',negative:'lowres',aspect:'1:1',resolution:'1080p'}}/></I18nProvider>);
 await waitFor(()=>expect(onChatRender).toHaveBeenCalledWith('r1',expect.objectContaining({job,width:1088,height:1088,checkpoint:'a.safetensors'})));
 const intent=vi.mocked(runWorkbenchJob).mock.calls[0][1] as {positivePrompt:string;negativePrompt:string;width:number;height:number};
 expect(intent).toMatchObject({positivePrompt:'1girl, fox ears, snow',negativePrompt:'lowres',width:1088,height:1088});
 expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.positive')!)).toBe('my own studio prompt');
 // A size it cannot read is refused with a reason, and nothing runs.
 view.rerender(<I18nProvider><Workbench {...props} chatRender={{id:'r2',positive:'x',negative:'',aspect:'cinematic'}}/></I18nProvider>);
 await waitFor(()=>expect(onChatRender).toHaveBeenCalledWith('r2',{error:expect.stringContaining('That size could not be used')}));
 expect(runWorkbenchJob).toHaveBeenCalledTimes(1);
});
test('Chat cannot render while Generate is not on text-to-image, and says what to change',async()=>{
 localStorage.setItem('deas.pref.studio.run.pipeline',JSON.stringify('i2i'));
 vi.mocked(checkRecipePrerequisites).mockResolvedValue(evidence);
 vi.mocked(scanModels).mockResolvedValue([{category:'checkpoints',name:'a.safetensors',path:'/m/a',family:'sdxl',size:1,info:null}]);
 const onChatStudio=vi.fn();
 render(<I18nProvider><Workbench endpoint={evidence.baseUrl} roots={{models:'/m'}} onChatStudio={onChatStudio}/></I18nProvider>);
 await waitFor(()=>expect(onChatStudio).toHaveBeenLastCalledWith(expect.objectContaining({checkpoint:'a.safetensors',blocked:expect.stringContaining('Switch its pipeline to text-to-image')})));
});
