import { invoke, isTauri } from '@tauri-apps/api/core';
import { isImageIntent, type JobIntent, type JobStatus } from '../../lib/jobs';
import type { RecipeAllowlist } from '../../lib/execution';
import { defaultVideoOutput, isVideoEnhanceIntent, isVideoIntent, videoFrames } from '../../lib/videoIntent';
import { isMusicIntent } from '../../lib/musicIntent';
import { defaultOptions, type OptionsPref } from './OptionsPanel';
import builtinFamilies from '../../families/builtin.json' with { type: 'json' };

/** Historical data only: these identities never grant live job or filesystem authority. */
export interface HistoryRecord {
 id:string; clientId:string|null; promptId:string|null; origin:'studio'|'diagnostic'; appVersion:string;
 admittedAt:number; observedAt:number; revision:number; state:JobStatus['state']; intent:JobIntent;
 outputs:{filename:string;subfolder:string;type:'output';format?:string}[];
 outputRoot:string|null; outputRootIdentity?:string|null; backendOrigin:string; error:string|null;
 startedAt?:number|null;generationMs?:number|null;
 /** Host/GPU maxima sampled natively while the job was active; absent for older records. */
 peaks?:{samples:number;cpuPercent:number|null;ramUsed:number|null;gpuUtilization:number|null;vramUsed:number|null;temperature:number|null;powerW:number|null;diskReadMibS:number|null;diskWriteMibS:number|null}|null;
}
export interface HistorySnapshot {schemaVersion:1;records:HistoryRecord[];readOnly:boolean;error:string|null}
export async function readHistory():Promise<HistorySnapshot> {
 if(!isTauri())throw new Error('Native history requires the DEAS desktop app.');
 try{
  const value=await invoke<HistorySnapshot>('native_jobs_history',{});
  if(value?.schemaVersion!==1||!Array.isArray(value.records)||value.records.length>10||typeof value.readOnly!=='boolean')throw new Error('Invalid native history response.');
  return value;
 }catch(error){void invoke('native_logs_history_error').catch(()=>{});throw error;}
}
export type HistoryOutputAvailability='available'|'missing'|'unvalidated';
/** Explicit local check; native requires the existing selected environment/root identity. */
export async function checkHistoryOutputs(jobId:string):Promise<HistoryOutputAvailability[]> {
 try{
  const value=await invoke<HistoryOutputAvailability[]>('native_jobs_history_outputs',{jobId});
  if(!Array.isArray(value)||value.length>128||value.some(v=>!['available','missing','unvalidated'].includes(v)))throw new Error('Invalid native history availability.');
  return value;
 }catch(error){void invoke('native_logs_history_error',{operation:'outputs'}).catch(()=>{});throw error;}
}
export interface RestoreContext {
 endpoint:string; inputRoot?:string; allow:RecipeAllowlist|null;
 models:readonly {name:string;family?:string}[];
}
export interface RestoredReference {name:string;strength:number;path?:string}
export interface HistoryRestore {
 intent:JobIntent; pipeline:'t2i'|'i2i'|'t2v'|'i2v'|'v2v'|'ref2v'|'t2m'|'cover'; options:OptionsPref;
 references:RestoredReference[]; source?:RestoredReference; lastFrame?:RestoredReference;
 /** Unsupported authoring data is disclosed before apply and keeps Run blocked. */
 unsupported:string[]; unavailable:string[]; environmentMismatch:boolean; backendOrigin:string;
}
const origin=(url:string)=>{try{return new URL(url).origin;}catch{return '';}};
/** Transient authoring scope only, never proof of a historical file's identity.
 * Keep backend URL namespaces distinct without retaining userinfo/query secrets. */
export function historyInputSelectionContext(endpoint:string,inputRoot?:string):string {
 let namespace='';
 try{const url=new URL(endpoint);namespace=url.origin+url.pathname.replace(/\/+$/,'')+'/';}catch{/* Invalid endpoints cannot execute. */}
 return JSON.stringify([namespace,(inputRoot??'').replace(/\\/g,'/').replace(/\/+$/,'')]);
}

/** Pure authoring projection; deliberately performs no discovery, uploads or execution. */
export function historyRestore(record:HistoryRecord,context:RestoreContext):HistoryRestore {
 const intent=structuredClone(record.intent),video=isVideoIntent(intent),image=isImageIntent(intent);
 const unsupported:string[]=[],unavailable:string[]=[];
 const environmentMismatch=origin(context.endpoint)!==record.backendOrigin;
 // TVE enhance jobs have no authoring controls in Studio; the record stays readable, not restorable.
 if(isVideoEnhanceIntent(intent))return {intent,pipeline:'v2v',options:structuredClone(defaultOptions),references:[],unsupported:['video-enhance'],unavailable,environmentMismatch,backendOrigin:record.backendOrigin};
 // This schema records no original input-root identity or backend namespace.
 // A currently listed name, matching origin, or output root cannot supply it.
 // Keep every historical input unresolved until the user explicitly reselects.
 const resolve=(name:string,strength=1):RestoredReference=>({name,strength});
 const model=video?intent.model:intent.checkpoint;
 const modelFamily=context.models.find(m=>m.name===model)?.family;
 const family=(builtinFamilies as {id:string;loader:string;loraFamily?:string;temporalLatent?:boolean;clipType?:string;sigmas?:number[];imagePrompting?:string;defaults?:{width:number;height:number};multiple?:number;video?:{lowModel?:string[];fixedFps?:boolean;fps:number}}[]).find(f=>f.id===modelFamily);
 if(!context.models.some(m=>m.name===model))unavailable.push(`model: ${model}`);
 for(const l of intent.loras??[])if(!context.allow?.loras?.includes(l.name)||!context.models.some(m=>m.name===l.name&&(!m.family||!modelFamily||m.family===(family?.loraFamily??modelFamily))))unavailable.push(`LoRA: ${l.name}`);
 for(const [field,value,list] of [['sampler',intent.sampler,context.allow?.samplers],['scheduler',intent.scheduler,context.allow?.schedulers]] as const)if(!list?.includes(value))unavailable.push(`${field}: ${value}`);
 const o=intent.options;
 const options:OptionsPref={...structuredClone(defaultOptions),attention:o?.attention?`a:${o.attention}`:o?.sage?`s:${o.sage}`:'',cache:o?.cache?.node??'',threshold:o?.cache?.threshold??defaultOptions.threshold,compile:o?.compile??'',vae:o?.vaeTile?'tiled':'full',tile:o?.vaeTile??defaultOptions.tile,
  // The effective prefix is already a literal; preserving it avoids inventing a template.
  outputNaming:intent.outputPrefix.replace(/^raw\//,''),sigmas:image?intent.sigmas?.join(', ')??'':'',
  output:video?{...defaultVideoOutput,...intent.output}:{...defaultVideoOutput,imageFormat:(image&&intent.imageFormat)||'png',imageCompression:(image?intent.imageCompression:undefined)??44}};
 if(!intent.outputPrefix.startsWith('raw/'))unsupported.push('outputPrefix');
 if(o?.attention&&!context.allow?.attention?.includes(o.attention))unavailable.push(`attention: ${o.attention}`);
 if(o?.sage&&!context.allow?.sage?.includes(o.sage))unavailable.push(`sage: ${o.sage}`);
 if(o?.compile&&!context.allow?.compile?.includes(o.compile))unavailable.push(`compile: ${o.compile}`);
 for(const node of [o?.cache?.node,o?.vaeTile?'VAEDecodeTiled':undefined])if(node&&!context.allow?.nodes.includes(node))unavailable.push(node);
 let references:RestoredReference[]=[],source:RestoredReference|undefined,lastFrame:RestoredReference|undefined;
 let pipeline:HistoryRestore['pipeline']='t2i';
 if(isMusicIntent(intent)){
  pipeline=intent.mode;
  for(const node of ['YuE2GenerateMusic','EmptyYuE2LatentAudio','VAEDecodeAudio','SaveAudioAdvanced',...(intent.planning==='off'?[]:intent.mode==='cover'?['LoadAudio','AudioEncoderLoader','SheetSage2AudioToABC']:['YuE2GenerateABC'])])if(!context.allow?.nodes.includes(node))unavailable.push(node);
  if(intent.mode==='cover'){
   source=resolve(intent.sourceAudio);
   if(!context.allow?.audioEncoders?.includes(intent.audioEncoder))unavailable.push(`audioEncoder: ${intent.audioEncoder}`);
  }
  if(modelFamily!=='yue2')unavailable.push('family: yue2');
 }else if(image){
  pipeline=intent.inputImage?'i2i':'t2i';
  if(intent.inputImage)source=resolve(intent.inputImage);
  if(intent.imageReference){references=intent.imageReference.images.map(r=>resolve(r.image,r.strength));if(!context.allow?.visionModels?.includes(intent.imageReference.visionModel))unavailable.push(`visionModel: ${intent.imageReference.visionModel}`);}
  if(intent.nativeImageReference)references=intent.nativeImageReference.images.map(name=>resolve(name));
  // These fields require editor pixels/geometry or independent second-pass controls.
  // They are never approximated from a different current editor source.
  for(const key of ['inputMask','inpaintCrop','inputLayers','inputResize','upscaleModel'] as const)if(intent[key]!==undefined)unsupported.push(key);
  if(intent.inputImage&&intent.sigmas&&intent.denoise!==1)unsupported.push('sigmas');
  const effectiveGrid=intent.nativeImageReference?.method==='qwen-image-2.1'?32:family?.multiple??16;
  if((intent.inputImage||intent.nativeImageReference?.method==='qwen-image-2.1')&&(intent.width%effectiveGrid!==0||intent.height%effectiveGrid!==0))unsupported.push('effective dimensions');
  if(!intent.sigmas&&family?.sigmas)unsupported.push('model default sigmas');
  if(!intent.diffusion&&family?.loader==='diffusion')unsupported.push('diffusion components');
  if(intent.refine){
   const r=intent.refine,grid=intent.nativeImageReference?.method==='qwen-image-2.1'?32:family?.multiple??16,ratio=r.width/intent.width;
   if(ratio<1||ratio>4)unsupported.push('refine.scale');
   if(Math.max(grid,Math.round(intent.height*ratio/grid)*grid)!==r.height)unsupported.push('refine.dimensions');
   const expected=intent.sigmas?.slice(-(Math.max(1,Math.round((intent.sigmas.length-1)*r.denoise))+1));
   if(JSON.stringify(r.sigmas)!==JSON.stringify(expected))unsupported.push('refine.sigmas');
   if(r.tiled){
    if(family?.temporalLatent)unsupported.push('refine.tiled');
    const native=family?.defaults??{width:1024,height:1024};const snap=(n:number)=>Math.max(grid,Math.round(n/grid)*grid);
    if(r.tiled.tileWidth!==snap(native.width)||r.tiled.tileHeight!==snap(native.height)||r.tiled.overlap!==snap(Math.max(64,Math.min(native.width,native.height)/8)))unsupported.push('refine.tiled');
   }
  }
  const expectedMethod=intent.imageReference?'revision':intent.nativeImageReference?.method;
  if(expectedMethod&&expectedMethod!==family?.imagePrompting)unavailable.push(`reference method: ${expectedMethod}`);
  if(intent.diffusion){
   if(!context.allow?.textEncoders?.includes(intent.diffusion.textEncoder))unavailable.push(`textEncoder: ${intent.diffusion.textEncoder}`);
   if(!context.allow?.vaes?.includes(intent.diffusion.vae))unavailable.push(`vae: ${intent.diffusion.vae}`);
   if(family?.clipType!==intent.diffusion.clipType)unsupported.push('diffusion.clipType');
  }
  if(intent.h3Edit&&modelFamily!=='minimax-h3')unsupported.push('h3Edit');
 }else{
  if(modelFamily!==intent.family)unavailable.push(`video family: ${intent.family}`);
  if(intent.latentUpscale){
   const models=intent.family==='minimax-h3'?context.allow?.h3LatentUpscaleModels:context.allow?.latentUpscaleModels;
   if(!models?.includes(intent.latentUpscale.model))unavailable.push(`latentUpscale: ${intent.latentUpscale.model}`);
   for(const node of intent.family==='minimax-h3'?['DaSiWaH3TiledUpscale']:['LatentUpscaleModelLoader','LTXVLatentUpsampler'])if(!context.allow?.nodes.includes(node))unavailable.push(node);
  }
  // Native diagnostic intents may carry image-only output members, while Studio's
  // video builder deliberately omits them. Preserve that distinction explicitly.
  for(const key of ['imageFormat','imageCompression'])if(key in intent.output)unsupported.push(`output.${key}`);
  if(family?.video?.fixedFps&&family.video.fps!==intent.frameRate)unsupported.push('frameRate');
  pipeline=intent.sourceVideo?'v2v':intent.firstFrame?'i2v':'t2v';
  if(intent.firstFrame)source=resolve(intent.firstFrame);
  if(intent.lastFrame)lastFrame=resolve(intent.lastFrame);
  if(intent.sourceVideo)unsupported.push('sourceVideo');
  if(intent.chunks!==undefined)unsupported.push('chunks');
  if(videoFrames(intent.family,intent.frames/intent.frameRate,intent.frameRate)!==intent.frames)unsupported.push('frames');
  if(intent.family==='ltx-2'&&intent.ltx.keyframes?.length)unsupported.push('ltx.keyframes');
  const component=(name:string,slot:'vae'|'encoder'|'upscale')=>{
   const category=slot==='vae'?context.allow?.vaes:slot==='encoder'?context.allow?.textEncoders:context.allow?.latentUpscaleModels;
   if(!category?.includes(name))unavailable.push(`${slot}: ${name}`);
  };
  if(intent.family==='ltx-2'){
   component(intent.ltx.audioVae,'vae');component(intent.ltx.textProjection,'encoder');
   if(intent.ltx.spatialUpscaler)component(intent.ltx.spatialUpscaler,'upscale');
   if(intent.ltx.passes===1&&intent.ltx.spatialUpscaler)unsupported.push('ltx.spatialUpscaler');
  }
  if(intent.family==='wan'){
   if(!context.models.some(m=>m.name===intent.wan.lowModel))unavailable.push(`model: ${intent.wan.lowModel}`);
   const [high,low]=family?.video?.lowModel??[];
   const inferred=high&&low?intent.model.replace(new RegExp(high,'i'),x=>x===x.toUpperCase()?low.toUpperCase():low):'';
   if(inferred!==intent.wan.lowModel)unsupported.push('wan.lowModel');
  }
  if(intent.family==='minimax-h3'){
   component(intent.h3.audioVae,'vae');
   if(intent.firstFrame||intent.lastFrame)unsupported.push('h3.frame fields');
   if(intent.h3.mode==='REF2VA')pipeline='ref2v';
   for(const key of ['media','continuity','refmods'] as const)if(intent.h3[key])unsupported.push(`h3.${key}`);
   if(intent.h3.mode!=='REF2VA'){
    const first=intent.h3.references.find(r=>r.slot===0),last=intent.h3.references.find(r=>r.slot===1);
    if(first){source=resolve(first.image);pipeline='i2v';}if(last)lastFrame=resolve(last.image);
   }else if(intent.h3.references.length)unsupported.push('h3.references');
  }
  if(!context.allow?.textEncoders?.includes(intent.textEncoder))unavailable.push(`textEncoder: ${intent.textEncoder}`);
  if(!context.allow?.vaes?.includes(intent.vae))unavailable.push(`vae: ${intent.vae}`);
 }
 for(const ref of [source,lastFrame,...references])if(ref&&!ref.path)unavailable.push(`input: ${ref.name}`);
 if(environmentMismatch)unavailable.push('backendOrigin');
 return {intent,pipeline,options,references,source,lastFrame,unsupported,unavailable,environmentMismatch,backendOrigin:record.backendOrigin};
}

/** One metadata row per native identity; this never populates the live collection. */
export const historicalRows=(records:readonly HistoryRecord[])=>records.filter((record,index)=>records.findIndex(r=>r.id===record.id)===index);
