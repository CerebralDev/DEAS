import { invoke, isTauri } from '@tauri-apps/api/core';
import { isImageIntent, type JobIntent, type JobStatus } from '../../lib/jobs';
import { safeOutputPrefix } from '../../lib/outputNaming';
import type { RecipeAllowlist } from '../../lib/execution';
import { defaultVideoOutput, isVideoEnhanceIntent, isVideoIntent, videoFrames } from '../../lib/videoIntent';
import { aceDurationStep, aceMusicProfile, isAceMusicIntent, isAceSettings, isMusicIntent, type AceStepMusicIntent } from '../../lib/musicIntent';
import { defaultOptions, type OptionsPref } from './OptionsPanel';
import { relativeMediaPath } from '../../lib/media';
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
/** Exact lexical candidate only; native availability binds it to the current approved root. */
export async function readHistoryForMedia(path:string):Promise<HistoryRecord|null> {
 const snapshot=await readHistory();
 const matches=snapshot.records.flatMap(record=>{
  if(record.origin!=='studio'||!record.outputRoot||isVideoEnhanceIntent(record.intent))return [];
  const relative=relativeMediaPath(path,record.outputRoot);
  return record.outputs.flatMap((output,index)=>relative&&relative===[output.subfolder,output.filename].filter(Boolean).join('/')?[{record,index}]:[]);
 });
 if(matches.length!==1)return null;
 const {record,index}=matches[0];
 const available=await checkHistoryOutputs(record.id);
 return available[index]==='available'?record:null;
}

export interface RestoreContext {
 endpoint:string; inputRoot?:string; allow:RecipeAllowlist|null;
 models:readonly {name:string;family?:string}[];
}
export interface RestoredReference {name:string;strength:number;path?:string}
/** A REF2VA media file kept as a named authoring input; re-pick supplies the file. */
export interface RestoredVideoRef {name:string;type:'image'|'video'|'audio';strength:number;mediaMode?:'video'|'audio'|'video_audio';trimStart?:number;trimEnd?:number}
export interface HistoryRestore {
 intent:JobIntent; pipeline:'t2i'|'i2i'|'t2v'|'i2v'|'v2v'|'ref2v'|'t2m'|'cover'; options:OptionsPref;
 references:RestoredReference[]; source?:RestoredReference; lastFrame?:RestoredReference; videoRefs:RestoredVideoRef[];
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

/** Reject incompatible restored ACE data instead of filling missing fields from current defaults. */
function restorableAce(intent:AceStepMusicIntent):boolean {
 const raw=intent as unknown as Record<string,unknown>,profile=aceMusicProfile(intent.family);
 if(!profile||intent.version!==1||intent.mode!=='t2m'||intent.planning!=='off'||!isAceSettings(intent.ace))return false;
 if(Object.keys(raw).some(k=>!['recipeId','version','family','mode','planning','checkpoint','style','lyrics','seed','maxDuration','steps','cfg','sampler','scheduler','outputPrefix','ace','diffusion','audioEnhance'].includes(k)&&raw[k]!==undefined))return false;
 if(!safeOutputPrefix(intent.outputPrefix))return false;
 if(['checkpoint','sampler','scheduler','outputPrefix'].some(k=>typeof raw[k]!=='string'||!(raw[k] as string).trim()))return false;
 if(typeof intent.style!=='string'||typeof intent.lyrics!=='string'||!Number.isSafeInteger(intent.seed)||intent.seed<0)return false;
 if(!Number.isInteger(intent.steps)||intent.steps<1||intent.steps>10000||!Number.isFinite(intent.cfg)||intent.cfg<0||intent.cfg>100)return false;
 const ticks=intent.maxDuration/aceDurationStep(intent.family);
 if(!Number.isFinite(intent.maxDuration)||intent.maxDuration<1||intent.maxDuration>1000||Math.abs(ticks-Math.round(ticks))>1e-7)return false;
 const clock=builtinFamilies.find(f=>f.id===intent.family)?.music;
 if(profile.version==='1.0'&&clock&&Math.floor(intent.maxDuration*clock.sampleRate/clock.samplesPerLatent)!==Math.round(ticks))return false;
 if(intent.diffusion!==undefined){
  if(profile.version!=='1.5')return false;
  const d=intent.diffusion;
  if(!d||typeof d!=='object'||Object.keys(d).some(k=>!['textEncoder','textEncoder2','vae'].includes(k)))return false;
  if([d.textEncoder,d.vae,...(profile.version==='1.5'?[d.textEncoder2]:[])].some(v=>typeof v!=='string'||!v.trim()))return false;
  if(d.textEncoder===d.textEncoder2)return false;
 }
 if(intent.audioEnhance!==undefined){
  const a=intent.audioEnhance;
  if(!a||typeof a!=='object'||Object.keys(a).some(k=>!['mode','profile','device'].includes(k))||!['clean','deep','stems'].includes(a.mode)||!['mix','speech'].includes(a.profile)||a.device!==undefined&&!['auto','cpu','cuda'].includes(a.device))return false;
 }
 return true;
}

/** Pure authoring projection; deliberately performs no discovery, uploads or execution. */
export function historyRestore(record:HistoryRecord,context:RestoreContext):HistoryRestore {
 const intent=structuredClone(record.intent),video=isVideoIntent(intent),image=isImageIntent(intent);
 const unsupported:string[]=[],unavailable:string[]=[];
 const environmentMismatch=origin(context.endpoint)!==record.backendOrigin;
 if(isMusicIntent(intent)&&(intent.family==='yue2'?Object.hasOwn(intent,'ace')||Object.hasOwn(intent,'diffusion'):!isAceMusicIntent(intent)||!restorableAce(intent)))return {intent,pipeline:'t2m',options:structuredClone(defaultOptions),references:[],videoRefs:[],unsupported:['music intent'],unavailable,environmentMismatch,backendOrigin:record.backendOrigin};
 // TVE enhance jobs have no authoring controls in Studio; the record stays readable, not restorable.
 if(isVideoEnhanceIntent(intent))return {intent,pipeline:'v2v',options:structuredClone(defaultOptions),references:[],videoRefs:[],unsupported:['video-enhance'],unavailable,environmentMismatch,backendOrigin:record.backendOrigin};
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
 let references:RestoredReference[]=[],videoRefs:RestoredVideoRef[]=[],source:RestoredReference|undefined,lastFrame:RestoredReference|undefined;
 let pipeline:HistoryRestore['pipeline']='t2i';
 if(isMusicIntent(intent)){
  pipeline=intent.mode;
  if(isAceMusicIntent(intent)){
   const recipe=builtinFamilies.find(f=>f.id===intent.family)!.music!;
   const original=recipe.version==='1.0';
   const nodes=['VAEDecodeAudio','SaveAudioAdvanced','KSampler','ConditioningZeroOut',recipe.encode,recipe.latent,recipe.patch,...(original?['LatentOperationTonemapReinhard','LatentApplyOperationCFG']:[]),...(intent.diffusion?['UNETLoader','DualCLIPLoader','VAELoader']:['CheckpointLoaderSimple'])];
   for(const node of nodes)if(!context.allow?.nodes.includes(node))unavailable.push(node);
   if(modelFamily!==intent.family)unavailable.push(`family: ${intent.family}`);
   if(intent.diffusion){
    if(!context.allow?.dualClipTypes?.includes('ace'))unavailable.push('DualCLIPLoader: ace');
    for(const name of [intent.diffusion.textEncoder,intent.diffusion.textEncoder2])if(name&&!context.allow?.dualTextEncoders?.includes(name))unavailable.push(`textEncoder: ${name}`);
    if(!context.allow?.vaes?.includes(intent.diffusion.vae))unavailable.push(`vae: ${intent.diffusion.vae}`);
   }
  }else{
   for(const node of ['YuE2GenerateMusic','EmptyYuE2LatentAudio','VAEDecodeAudio','SaveAudioAdvanced',...(intent.planning==='off'?[]:intent.mode==='cover'?['LoadAudio','AudioEncoderLoader','SheetSage2AudioToABC']:['YuE2GenerateABC'])])if(!context.allow?.nodes.includes(node))unavailable.push(node);
   if(intent.mode==='cover'){
    source=resolve(intent.sourceAudio);
    if(!context.allow?.audioEncoders?.includes(intent.audioEncoder))unavailable.push(`audioEncoder: ${intent.audioEncoder}`);
   }
   if(modelFamily!=='yue2')unavailable.push('family: yue2');
  }
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
  if(intent.family==='minimax-h3'||intent.family==='minimax-h3-hyperflow'){
   component(intent.h3.audioVae,'vae');
   if(intent.firstFrame||intent.lastFrame)unsupported.push('h3.frame fields');
   if(intent.h3.mode==='REF2VA')pipeline='ref2v';
   for(const key of ['continuity','refmods'] as const)if(intent.h3[key])unsupported.push(`h3.${key}`);
   if(intent.h3.mode!=='REF2VA'){
    const first=intent.h3.references.find(r=>r.slot===0),last=intent.h3.references.find(r=>r.slot===1);
    if(first){source=resolve(first.image);pipeline='i2v';}if(last)lastFrame=resolve(last.image);
   }else if(intent.h3.references.length)unsupported.push('h3.references');
   // REF2VA media files are restorable inputs: hand them over unresolved (name only),
   // in Director lane order, so the Workbench lists each as a missing input to re-pick.
   else if(intent.h3.media?.length){
    const rank={image:0,video:1,audio:2} as const;
    videoRefs=[...intent.h3.media].sort((a,b)=>rank[a.type]-rank[b.type]||a.slot-b.slot).map(m=>({name:m.file,type:m.type,strength:1,...(m.mediaMode?{mediaMode:m.mediaMode}:{}),...(m.trimStart!==undefined?{trimStart:m.trimStart}:{}),...(m.trimEnd!==undefined?{trimEnd:m.trimEnd}:{})}));
   }
  }
  if(!context.allow?.textEncoders?.includes(intent.textEncoder))unavailable.push(`textEncoder: ${intent.textEncoder}`);
  if(!context.allow?.vaes?.includes(intent.vae))unavailable.push(`vae: ${intent.vae}`);
 }
 for(const ref of [source,lastFrame,...references])if(ref&&!ref.path)unavailable.push(`input: ${ref.name}`);
 if(environmentMismatch)unavailable.push('backendOrigin');
 return {intent,pipeline,options,references,videoRefs,source,lastFrame,unsupported,unavailable,environmentMismatch,backendOrigin:record.backendOrigin};
}

/** One metadata row per native identity; this never populates the live collection. */
export const historicalRows=(records:readonly HistoryRecord[])=>records.filter((record,index)=>records.findIndex(r=>r.id===record.id)===index);
