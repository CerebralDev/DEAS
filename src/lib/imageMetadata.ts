import { invoke } from '@tauri-apps/api/core';
import type { WildcardRules } from './wildcard';

export type ImageMetadata = [string,string][];
/** Both sides bound untrusted embedded text; only the selected image is read, never thumbnails. */
export async function readImageMetadata(path:string):Promise<ImageMetadata> {
 const value=await invoke<unknown>('native_media_metadata',{path});
 if(!Array.isArray(value)||value.length>64||!value.every(row=>Array.isArray(row)&&row.length===2&&row.every(v=>typeof v==='string'))
  ||value.reduce((n,row)=>n+row[0].length+row[1].length,0)>2*1024*1024)throw new Error('Invalid image metadata');
 return value as ImageMetadata;
}
const object=(v:unknown):v is Record<string,unknown>=>!!v&&typeof v==='object'&&!Array.isArray(v);
const text=(v:unknown)=>typeof v==='string'?v:JSON.stringify(v,null,2);
/** Legacy Python savers emitted bare NaN cache tokens. Never alter quoted prompt text or evaluate input. */
export function parseMetadataJson(value:string):unknown {
 if(value.length>2*1024*1024)throw new Error('Metadata too large');
 try{return JSON.parse(value);}catch{
  let quoted=false,escaped=false,out='';
  for(let i=0;i<value.length;i++){
   const c=value[i];
   if(!quoted&&value.startsWith('NaN',i)&&/[\s[:,]/.test(value[i-1]??'')&&/[\s,\]}]/.test(value[i+3]??'')){
    out+='null';i+=2;continue;
   }
   out+=c;
   if(escaped){escaped=false;continue;}
   if(quoted&&c==='\\'){escaped=true;continue;}
   if(c==='"')quoted=!quoted;
  }
  return JSON.parse(out);
 }
}
/** Recognize Swarm's JSON envelope and ComfyUI API graphs; preserve unfamiliar metadata as text. */
export function imageMetadataRows(entries:ImageMetadata):ImageMetadata {
 const rows:ImageMetadata=[];
 const hasPrompt=entries.some(([key])=>key.toLowerCase()==='prompt');
 for(const [key,value] of entries){
  if(key.toLowerCase()==='workflow'&&hasPrompt)continue; // The API graph already contains the execution settings, not editor coordinates.
  let parsed:unknown;try{parsed=parseMetadataJson(value);}catch{rows.push([key,value]);continue;}
  if(object(parsed)&&object(parsed.sui_image_params)){
   for(const section of ['sui_image_params','sui_extra_data']){
    const data=parsed[section];if(object(data))for(const [name,v] of Object.entries(data))rows.push([name,text(v)]);
   }
  }else if(key.toLowerCase()==='prompt'&&object(parsed)){
   const settings=comfy(parsed);
   if(settings?.media==='video'){
    const names:Record<string,string>={positive:'prompt',negative:'negative_prompt',checkpoint:'model',cfg:'cfgscale',frameRate:'fps'};
    for(const [name,v] of Object.entries(settings)){
     if(v===undefined||['media','skipped'].includes(name))continue;
     rows.push([names[name]??name,name==='loras'&&Array.isArray(v)?v.map(l=>`${l.name} (${l.strength})`).join(', '):text(v)]);
    }
    continue;
   }
   const fields:ImageMetadata=[];
   for(const [id,node] of Object.entries(parsed))if(object(node)&&typeof node.class_type==='string'&&object(node.inputs)){
    for(const [name,v] of Object.entries(node.inputs)){
     // Linked outputs are graph wiring, not the value of the setting.
     if(Array.isArray(v)&&v.length===2&&typeof v[0]==='string'&&Number.isInteger(v[1]))continue;
     fields.push([`${node.class_type} #${id} · ${name}`,text(v)]);
    }
   }
   rows.push(...(fields.length&&fields.length<=512?fields:[[key,value]] as ImageMetadata));
  }else rows.push([key,value]);
 }
 return rows;
}

/** Settings an image's metadata can put back into the generation controls; absent keys were not found. */
export interface GenerationSettings {
 wildcard?:WildcardRules;
 /** Partial video graph settings are not an executable/restorable DEAS video intent. */
 media?:'video';frames?:number;frameRate?:number;sigmas?:number[];
 positive?:string;negative?:string;checkpoint?:string;seed?:number;steps?:number;cfg?:number;sampler?:string;scheduler?:string;width?:number;height?:number;
 loras?:{name:string;strength:number}[];
 /** Values found but not translatable to ComfyUI (`Sampler: PLMS`), shown to the user instead of applied. */
 skipped?:string[];
}
// A1111 display names → ComfyUI sampler ids (Civitai's samplerMap plus the newer schedulers A1111 writes as suffixes).
const samplerNames=new Map<string,string>([['euler a','euler_ancestral'],['euler','euler'],['lms','lms'],['heun','heun'],['dpm2','dpm_2'],['dpm2 a','dpm_2_ancestral'],['dpm++ 2s a','dpmpp_2s_ancestral'],['dpm++ 2m','dpmpp_2m'],['dpm++ sde','dpmpp_sde'],['dpm++ 2m sde','dpmpp_2m_sde'],['dpm++ 2m sde heun','dpmpp_2m_sde_heun'],['dpm++ 3m sde','dpmpp_3m_sde'],['dpm fast','dpm_fast'],['dpm adaptive','dpm_adaptive'],['ddim','ddim'],['ddpm','ddpm'],['uni_pc','uni_pc'],['unipc','uni_pc'],['lcm','lcm']]);
const schedulerNames=new Map<string,string>([['karras','karras'],['exponential','exponential'],['sgm uniform','sgm_uniform'],['simple','simple'],['normal','normal'],['beta','beta'],['ddim','ddim_uniform'],['ddim uniform','ddim_uniform'],['uniform','simple'],['automatic','normal'],['kl optimal','kl_optimal'],['linear quadratic','linear_quadratic']]);
/** A1111 `Sampler: DPM++ 2M Karras` → ComfyUI sampler + scheduler; already-ComfyUI ids pass through; unknown → null. */
export function comfySampler(name:string):{sampler:string;scheduler?:string}|null {
 const words=name.trim().toLowerCase().split(/\s+/);
 // A bare ComfyUI id passes through (`dpmpp_3m_sde_gpu`, `deis`); an A1111-only name like `PLMS` must not.
 if(words.length===1&&/^[a-z0-9_]+$/.test(words[0])&&!samplerNames.has(words[0])&&(words[0].includes('_')||[...samplerNames.values(),'deis','ipndm'].includes(words[0])))return {sampler:words[0]};
 for(let cut=words.length;cut>0;cut--){
  const sampler=samplerNames.get(words.slice(0,cut).join(' '));if(!sampler)continue;
  const rest=words.slice(cut).join(' ');if(!rest)return {sampler};
  const scheduler=schedulerNames.get(rest)??(/^[a-z0-9]+_[a-z0-9_]+$/.test(rest)?rest:undefined);
  return scheduler?{sampler,scheduler}:null;
 }
 return null;
}
const num=(v:unknown)=>{const n=typeof v==='string'?(v.trim()?Number(v):NaN):v;return typeof n==='number'&&Number.isFinite(n)?n:undefined;};
const str=(v:unknown)=>typeof v==='string'&&v.trim()?v:undefined;
/** `<lora:name:weight>` tags as A1111 and the DaSiWa saver write them; the prompt keeps its words only. */
function loraTags(prompt:string){
 const loras:{name:string;strength:number}[]=[];
 const text=prompt.replace(/<lora:([^>]+)>/gi,(_,body:string)=>{const parts=body.split(':');const strength=parts.length>1?num(parts[parts.length-1]):undefined;loras.push({name:(strength===undefined?parts:parts.slice(0,-1)).join(':').trim(),strength:strength??1});return '';}).replace(/\s*,\s*(,\s*)+/g,', ').replace(/^[\s,]+|[\s,]+$/g,'');
 return {text,loras};
}
/** A1111 `parameters` text: prompt lines, `Negative prompt:` lines, then one `Key: value, Key: "quoted, value"` line. */
function a1111(text:string):GenerationSettings|null {
 const lines=text.split('\n');const at=lines.findIndex(l=>/^Steps: ?\d/.test(l));if(at<0)return null;
 const fields:Record<string,string>={};
 for(const m of lines[at].matchAll(/([A-Za-z][^:,]*): ("(?:[^"\\]|\\.)*"|[^,]*)/g))fields[m[1].trim()]=m[2].startsWith('"')?m[2].slice(1,-1).replace(/\\(.)/g,'$1'):m[2].trim();
 const [positive,...negative]=lines.slice(0,at).join('\n').split('Negative prompt:');
 const {text:prompt,loras}=loraTags(positive.trim());
 const skipped:string[]=[];
 const mapped=fields.Sampler?comfySampler(fields.Sampler):null;if(fields.Sampler&&!mapped)skipped.push(`Sampler: ${fields.Sampler}`);
 const schedule=fields['Schedule type']?schedulerNames.get(fields['Schedule type'].toLowerCase()):undefined;if(fields['Schedule type']&&!schedule)skipped.push(`Schedule type: ${fields['Schedule type']}`);
 const [width,height]=(fields.Size??'').split('x').map(num);
 return {positive:prompt,negative:negative.length?negative.join('Negative prompt:').trim():undefined,checkpoint:str(fields.Model),seed:num(fields.Seed),steps:num(fields.Steps),cfg:num(fields['CFG scale']),sampler:mapped?.sampler,scheduler:schedule??mapped?.scheduler,width,height,loras:loras.length?loras:undefined,skipped:skipped.length?skipped:undefined};
}
/** Read literal inputs along known compiler wiring; never evaluate a graph or infer a full DEAS intent. */
function comfy(graph:Record<string,unknown>):GenerationSettings|null {
 const nodes=Object.values(graph).filter(object);
 const node=(v:unknown)=>Array.isArray(v)&&v.length===2&&typeof v[0]==='string'&&Number.isInteger(v[1])&&v[1]>=0&&Object.hasOwn(graph,v[0])?graph[v[0]]:undefined;
 const inputs=(n:unknown):Record<string,unknown>=>object(n)&&object(n.inputs)?n.inputs:{};
 const kind=(n:unknown)=>object(n)?n.class_type:undefined;
 const sampler=nodes.find(n=>['KSampler','KSamplerAdvanced','SamplerCustomAdvanced'].includes(String(n.class_type)));if(!sampler)return null;
 const i=inputs(sampler),custom=sampler.class_type==='SamplerCustomAdvanced';
 const guider=node(i.guider),g=inputs(guider),schedule=node(i.sigmas),z=inputs(schedule);
 const positive=custom?g.positive??g.conditioning:i.positive,negative=custom?g.negative:i.negative;
 const textOf=(link:unknown,depth=0):string|undefined=>{
  if(depth>=64)return undefined;
  const n=node(link),p=inputs(n),type=kind(n),port=Array.isArray(link)?link[1]:0;
  if(type==='CLIPTextEncode')return port===0&&typeof p.text==='string'?p.text:undefined;
  if(['TextEncodeQwenImage21','DEAS_ImagePromptEncode','TextEncodeMageFlowEdit'].includes(String(type)))return (port===0||port===1)&&typeof p[port===1?'negative_prompt':'prompt']==='string'?p[port===1?'negative_prompt':'prompt'] as string:undefined;
  if(['LTXVConditioning','LTXVAddGuide','WanImageToVideo','WanFirstLastFrameToVideo'].includes(String(type)))return port===0||port===1?textOf(p[port===1?'negative':'positive'],depth+1):undefined;
  if(type==='unCLIPConditioning')return textOf(p.conditioning,depth+1);
  if(type==='MiniMaxH3DirectorGuide'){
   const d=node(p.guide);return kind(d)==='MiniMaxH3Director'&&typeof inputs(d).prompt==='string'?inputs(d).prompt as string:undefined;
  }
  // ZeroOut and wildcard outputs are computed, not the original/realized prompt strings.
  return undefined;
 };
 let checkpoint:string|undefined;
 const loras:NonNullable<GenerationSettings['loras']>=[];let completeLoras=true;
 let model=custom?g.model:i.model;
 const seen=new Set<unknown>();
 for(let depth=0;depth<64;depth++){
  const n=node(model),p=inputs(n),type=kind(n);if(!n)break;
  if(seen.has(n)){loras.length=0;break;}seen.add(n);
  if(['CheckpointLoaderSimple','UNETLoader','UnetLoaderGGUF'].includes(String(type))){checkpoint=str(p.ckpt_name??p.unet_name);break;}
  if(type==='LoraLoader'||type==='LoraLoaderModelOnly'){
   const name=str(p.lora_name),strength=num(p.strength_model);
   if(name&&strength!==undefined)loras.unshift({name,strength});else completeLoras=false;
  }
  model=type==='MiniMaxH3Director'?p.ref2va_model??p.fl2va_model:p.model;
 }
 let latent=inputs(node(i.latent_image));
 let latentKind=kind(node(i.latent_image));
 for(let depth=0;depth<64;depth++){
  const next=latentKind==='LTXVConcatAVLatent'?latent.video_latent:latentKind==='LTXVAddGuide'?latent.latent:undefined;
  if(!next)break;latentKind=kind(node(next));latent=inputs(node(next));
 }
 const conditioning=inputs(node(positive)),directorNode=node(conditioning.guide),director=inputs(directorNode);
 const video=['EmptyLTXVLatentVideo','WanImageToVideo','WanFirstLastFrameToVideo'].includes(String(latentKind))||kind(directorNode)==='MiniMaxH3Director';
 const frameRate=video?num(conditioning.frame_rate??director.frame_rate):undefined;
 let sigmas:number[]|undefined;
 if(kind(schedule)==='ManualSigmas'&&typeof z.sigmas==='string'){
  const values=z.sigmas.split(',').map(num);
  if(values.length>=2&&values.every((v):v is number=>v!==undefined&&v>=0)&&values.at(-1)===0&&values.every((v,n)=>n===0||values[n-1]!>v))sigmas=values;
 }
 return {positive:textOf(positive),negative:textOf(negative),checkpoint,
  seed:num(custom?inputs(node(i.noise)).noise_seed:i.seed??i.noise_seed),
  steps:custom?(sigmas?sigmas.length-1:['BasicScheduler','Flux2Scheduler'].includes(String(kind(schedule)))?num(z.steps):undefined):num(i.steps),
  cfg:custom?(kind(guider)==='CFGGuider'?num(g.cfg):undefined):num(i.cfg),
  sampler:str(custom?inputs(node(i.sampler)).sampler_name:i.sampler_name),scheduler:str(custom?kind(schedule)==='BasicScheduler'?z.scheduler:undefined:i.scheduler),sigmas,
  width:num(latent.width??director.width),height:num(latent.height??director.height),loras:checkpoint&&completeLoras?loras:undefined,
  ...(video?{media:'video' as const,frames:num(latent.length),frameRate,skipped:['video.intent']}:{}),
 };
}
/** SwarmUI `sui_image_params`. */
function swarm(p:Record<string,unknown>):GenerationSettings {
 const names=typeof p.loras==='string'?p.loras.split(',').map(s=>s.trim()).filter(Boolean):Array.isArray(p.loras)?p.loras.map(String):[];
 const weights=typeof p.loraweights==='string'?p.loraweights.split(','):Array.isArray(p.loraweights)?p.loraweights:[];
 const loras=names.map((name,i)=>({name,strength:num(weights[i])??1}));
 return {positive:typeof p.prompt==='string'?p.prompt:undefined,negative:typeof p.negativeprompt==='string'?p.negativeprompt:undefined,checkpoint:str(p.model),seed:num(p.seed),steps:num(p.steps),cfg:num(p.cfgscale),sampler:str(p.sampler),scheduler:str(p.scheduler),width:num(p.width),height:num(p.height),loras:Object.hasOwn(p,'loras')?loras:undefined};
}
/** Literal Comfy inputs take precedence over lossy display parameters; missing values can come from A1111/Swarm. */
export function generationSettings(entries:ImageMetadata):GenerationSettings|null {
 const clean=(found:GenerationSettings|null):GenerationSettings=>Object.fromEntries(Object.entries(found??{}).filter(([key,v])=>v!==undefined&&(v!==''||key==='positive'||key==='negative')&&!(typeof v==='string'&&/^[a-z_]+_not_found(?:\.safetensors)?$/.test(v))));
 let parameters:GenerationSettings|null=null,graph:GenerationSettings|null=null;
 for(const [key,value] of entries){
  if(!['parameters','usercomment','imagedescription','prompt'].includes(key.toLowerCase()))continue;
  let parsed:unknown;try{parsed=parseMetadataJson(value);}catch{parsed=undefined;}
  if(key.toLowerCase()==='prompt'){if(object(parsed))graph??=comfy(parsed);}
  else parameters??=object(parsed)&&object(parsed.sui_image_params)?swarm(parsed.sui_image_params):a1111(value);
 }
 if(!graph&&!parameters)return null;
 const result:GenerationSettings={...clean(parameters),...clean(graph),skipped:[...(parameters?.skipped??[]),...(graph?.skipped??[])]};
 if(graph?.sampler&&result.skipped)result.skipped=result.skipped.filter(v=>!v.startsWith('Sampler:'));
 if(graph?.scheduler&&result.skipped)result.skipped=result.skipped.filter(v=>!v.startsWith('Schedule type:'));
 if(!result.skipped?.length)delete result.skipped;
 // Explicit sigmas are the schedule; a saver display field must not invent a named scheduler for them.
 if(result.sigmas)delete result.scheduler;
 return result.positive!==undefined||result.negative!==undefined||result.seed!==undefined||result.steps!==undefined?result:null;
}
