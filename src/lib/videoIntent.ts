import type { AudioEnhanceIntent } from './audioEnhance';
import type { RunOptions } from './execution';
/** Video recipe intent (native compiler `jobs_video.rs`), one per model family; the TS side only types it and derives frame counts. */
export type VideoFamily='ltx-2'|'wan'|'minimax-h3';
export const videoCodecs=['Auto','AV1','VP9','H.265 (HEVC)','H.264'] as const;
export const videoContainers=['Auto','WebM','MKV','MP4','Animated WebP','Animated AVIF'] as const;
export const bitDepths=['Auto','8-bit','10-bit'] as const;
export const audioCodecs=['Auto','AAC','Opus','MP3'] as const;
export const audioBitrates=['64k','96k','128k','160k','192k','256k','320k'] as const;
/** DaSiWa Enhanced Video Combine settings plus the image saver's format; chosen in the Options tab. */
export interface VideoOutput { codec:typeof videoCodecs[number]; container:typeof videoContainers[number]; bitDepth:typeof bitDepths[number]; quality:number; audioCodec:typeof audioCodecs[number]; audioBitrate:typeof audioBitrates[number]; saveFirstFrame:boolean; saveLastFrame:boolean; imageFormat:'png'|'webp'; imageCompression:number }
export const defaultVideoOutput:VideoOutput={codec:'Auto',container:'Auto',bitDepth:'Auto',quality:20,audioCodec:'Auto',audioBitrate:'192k',saveFirstFrame:false,saveLastFrame:false,imageFormat:'png',imageCompression:44};
const inList=(list:readonly string[],v:unknown)=>typeof v==='string'&&list.includes(v);
export const isVideoOutput=(v:unknown):v is VideoOutput=>{
 const o=v as VideoOutput;
 return !!o&&typeof o==='object'&&inList(videoCodecs,o.codec)&&inList(videoContainers,o.container)&&inList(bitDepths,o.bitDepth)&&Number.isInteger(o.quality)&&o.quality>=0&&o.quality<=51
  &&inList(audioCodecs,o.audioCodec)&&inList(audioBitrates,o.audioBitrate)&&typeof o.saveFirstFrame==='boolean'&&typeof o.saveLastFrame==='boolean'&&inList(['png','webp'],o.imageFormat)&&Number.isInteger(o.imageCompression)&&o.imageCompression>=0&&o.imageCompression<=100;
};
/** A DaSiWa Director REF2VA timeline item; `file` is an uploaded input name. */
export interface H3Media { type:'image'|'video'|'audio'; file:string; slot:number; mediaMode?:'video'|'audio'|'video_audio'; trimStart?:number; trimEnd?:number }
/** DaSiWa Director input scaling of references (`timeline_data.resolution.input_scaling`); absent = `Fill and crop`. */
export const h3Scalings=['Off','Auto','Target','Fit','Fill and crop','Fit and pad','Long side with divisible crop'] as const;
export type H3Scaling=typeof h3Scalings[number];
export const h3OverlapFrames=[73,56,39,22,5] as const;
export type H3OverlapFrames=typeof h3OverlapFrames[number];
export const isH3OverlapFrames=(v:unknown):v is H3OverlapFrames=>h3OverlapFrames.some(n=>n===v);
export interface H3Continuity { sourceKind?:'video'; session:string; sourceId:string; capture:boolean; action:string; outputMode:'full'|'segment'; overlapFrames?:H3OverlapFrames; useReferences?:boolean; durationSeconds?:number }
/** TVE post-processing on the decoded frames (native `jobs_video_post.rs`): model upscale → RTX refine → frame generation. */
export const rtxQualities=['Low','Medium','High','Ultra'] as const;
export const rtxUpscaleModes=['VSR','High Bitrate','Streaming Medium','Streaming Ultra'] as const;
export const frameGenModes=['LOW','MEDIUM','HIGH'] as const;
export type RtxQuality=typeof rtxQualities[number];
export interface VideoPost {
 upscaleModel?:string;
 /** RTX disk fallback (`use_mmap`) for both RTX nodes; only for batches that exceed available RAM. */
 useMmap?:boolean;
 rtx?:{ denoise?:RtxQuality; deblur?:RtxQuality; upscale?:{ mode:typeof rtxUpscaleModes[number]; quality:RtxQuality; scale:number }; chunkFrames?:number };
 frameGeneration?:{ engine:'rtx'; multiplier:number; mode:typeof frameGenModes[number] }|{ engine:'rife'; multiplier:number; model:string };
}
const multiplierOk=(m:unknown)=>Number.isInteger(m)&&(m as number)>=2&&(m as number)<=16;
const chunkFramesOk=(n:unknown)=>Number.isInteger(n)&&(n as number)>=1&&(n as number)<=1024;
export const isVideoPost=(v:unknown):v is VideoPost=>{
 const p=v as VideoPost;
 if(!p||typeof p!=='object'||!Object.keys(p).length||Object.keys(p).some(k=>!['upscaleModel','rtx','frameGeneration','useMmap'].includes(k)))return false;
 if(p.upscaleModel!==undefined&&typeof p.upscaleModel!=='string')return false;
 if(p.useMmap!==undefined&&typeof p.useMmap!=='boolean')return false;
 const r=p.rtx;
 if(r!==undefined&&(!r||typeof r!=='object'||!Object.keys(r).length||(r.denoise!==undefined&&!inList(rtxQualities,r.denoise))||(r.deblur!==undefined&&!inList(rtxQualities,r.deblur))
  ||(r.chunkFrames!==undefined&&!chunkFramesOk(r.chunkFrames))
  ||(r.upscale!==undefined&&!(inList(rtxUpscaleModes,r.upscale.mode)&&inList(rtxQualities,r.upscale.quality)&&Number.isFinite(r.upscale.scale)&&r.upscale.scale>=1&&r.upscale.scale<=4))))return false;
 const f=p.frameGeneration;
 return f===undefined||(f.engine==='rtx'&&multiplierOk(f.multiplier)&&inList(frameGenModes,f.mode))||(f.engine==='rife'&&multiplierOk(f.multiplier)&&typeof f.model==='string');
};
/** Output frame rate of a post chain; frame generation multiplies the source cadence. */
export const videoPostFrameRate=(fps:number,post:VideoPost|undefined)=>fps*(post?.frameGeneration?.multiplier??1);
interface VideoBase { readonly audioEnhance?:AudioEnhanceIntent; readonly recipeId:'video-generation'; readonly version:1; readonly positivePrompt:string; readonly negativePrompt:string; readonly model:string; readonly textEncoder:string; readonly vae:string; readonly seed:number; readonly width:number; readonly height:number; readonly frames:number; readonly frameRate:number; readonly steps:number; readonly cfg:number; readonly sampler:string; readonly scheduler:string; readonly outputPrefix:string; readonly output:Omit<VideoOutput,'imageFormat'|'imageCompression'>; readonly firstFrame?:string; readonly lastFrame?:string; readonly sourceVideo?:string; readonly loras?:readonly {name:string;strength:number}[]; readonly options?:RunOptions; readonly chunks?:number; readonly videoPost?:VideoPost }
/** Optional `DaSiWaH3TiledUpscale` inputs; omitted keys keep the node defaults in `h3LatentDefaults`. */
export interface H3LatentAdvanced { cfg?:number; scheduler?:string; precision?:'auto'|'bf16'|'fp16'|'fp32'; memoryBudgetMb?:number; spatialTiling?:boolean; temporalChunking?:boolean; spikeGuard?:boolean; guardSensitivity?:number; guardStrength?:number; guardMode?:'burn_direction'|'both'; memoryPlanning?:boolean; memoryReservePct?:number; continuitySoftRefine?:boolean; continuityMaskStrength?:number }
export const h3LatentDefaults:Required<H3LatentAdvanced>={cfg:1,scheduler:'simple',precision:'auto',memoryBudgetMb:0,spatialTiling:true,temporalChunking:true,spikeGuard:false,guardSensitivity:4.3,guardStrength:1,guardMode:'burn_direction',memoryPlanning:true,memoryReservePct:5,continuitySoftRefine:false,continuityMaskStrength:1};
/** Only values that differ from the node defaults, so untouched options leave intents and graphs unchanged. */
export const h3LatentChanged=(a:H3LatentAdvanced|undefined):H3LatentAdvanced|undefined=>{
 const changed=Object.fromEntries(Object.entries(a??{}).filter(([k,v])=>k in h3LatentDefaults&&v!==h3LatentDefaults[k as keyof H3LatentAdvanced]));
 return Object.keys(changed).length?changed as H3LatentAdvanced:undefined;
};
export interface H3LatentUpscale { model:string; scale:number; steps:number; denoise:number; advanced?:H3LatentAdvanced }
export interface LtxLatentUpscale { model:string; scale:2 }
/** Matches Python round, not JavaScript's half-up rounding. */
export function latentUpscaleSize(family:VideoFamily,size:{width:number;height:number},scale:number){
 const edge=(n:number)=>{if(family==='ltx-2')return n*2;const x=n*scale/32,lo=Math.floor(x);return Math.max(32,(x-lo===0.5?(lo%2===0?lo:lo+1):Math.round(x))*32);};
 return {width:edge(size.width),height:edge(size.height)};
}
export type VideoIntentV1=VideoBase&(
 |{readonly family:'ltx-2';readonly latentUpscale?:LtxLatentUpscale;readonly ltx:{textProjection:string;audioVae:string;passes:1|2|3;spatialUpscaler?:string;keyframes?:{image:string;frame:number;strength:number}[];sourceStrength?:number}}
 |{readonly family:'wan';readonly latentUpscale?:never;readonly wan:{lowModel:string;shiftHigh:number;shiftLow:number;refinerStep:number}}
 |{readonly family:'minimax-h3';readonly latentUpscale?:H3LatentUpscale;readonly h3:{mode:'T2VA'|'I2VA'|'L2VA'|'FL2VA'|'REF2VA';audioVae:string;shiftVideo:number;shiftAudio:number;references:{slot:0|1;image:string}[];media?:H3Media[];continuity?:H3Continuity;inputScaling?:H3Scaling;refmods?:{slot:number;name:string;strength:number;description?:string}[]}});
export const isVideoIntent=(i:{recipeId:string}):i is VideoIntentV1=>i.recipeId==='video-generation';
/** TVE tab: enhance an uploaded source video with the same chain (native `compile_enhance`); the source stays the baseline. */
export interface VideoEnhanceIntentV1 { readonly audioEnhance?:AudioEnhanceIntent; readonly recipeId:'video-enhance'; readonly version:1; readonly sourceVideo:string; readonly frameRate:number; readonly seed:number; readonly videoPost:VideoPost; readonly output?:Omit<VideoOutput,'imageFormat'|'imageCompression'>; readonly outputPrefix?:string }
export const isVideoEnhanceIntent=(i:{recipeId:string}):i is VideoEnhanceIntentV1=>i.recipeId==='video-enhance';
/** Frame counts as the owner's workflows compute them: LTX/Wan `round(s·fps/8)·8+1`, MiniMax H3 `17k+5` at 24 fps (snapped up). */
export function videoFrames(family:VideoFamily,seconds:number,fps:number):number{
 if(family==='minimax-h3'){const n=Math.max(5,Math.round(seconds*24));return n+(((5-(n%17))%17)+17)%17;}
 return Math.max(family==='wan'?5:9,Math.round(seconds*fps/8)*8+1);
}
/** Feed-forward chunk count from the OmniForge/FastFidelity formula; `headroom` 1 ≈ 24 GB … 4 ≈ 8 GB. Undefined = no chunking. */
export function chunkCount(frames:number,width:number,height:number,headroom:number):number|undefined{
 const n=Math.round(((frames*width*height)/120_000_000)**1.3*0.5*headroom);
 return n>1?Math.min(16,n):undefined;
}
