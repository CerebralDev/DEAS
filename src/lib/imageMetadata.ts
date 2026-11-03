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
/** Recognize Swarm's JSON envelope and ComfyUI API graphs; preserve unfamiliar metadata as text. */
export function imageMetadataRows(entries:ImageMetadata):ImageMetadata {
 const rows:ImageMetadata=[];
 const hasPrompt=entries.some(([key])=>key.toLowerCase()==='prompt');
 for(const [key,value] of entries){
  if(key.toLowerCase()==='workflow'&&hasPrompt)continue; // The API graph already contains the execution settings, not editor coordinates.
  let parsed:unknown;try{parsed=JSON.parse(value);}catch{rows.push([key,value]);continue;}
  if(object(parsed)&&object(parsed.sui_image_params)){
   for(const section of ['sui_image_params','sui_extra_data']){
    const data=parsed[section];if(object(data))for(const [name,v] of Object.entries(data))rows.push([name,text(v)]);
   }
  }else if(key.toLowerCase()==='prompt'&&object(parsed)){
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
 positive?:string;negative?:string;checkpoint?:string;seed?:number;steps?:number;cfg?:number;sampler?:string;scheduler?:string;width?:number;height?:number;
 loras?:{name:string;strength:number}[];
 /** Values found but not translatable to ComfyUI (`Sampler: PLMS`), shown to the user instead of applied. */
 skipped?:string[];
}
// A1111 display names → ComfyUI sampler ids (Civitai's samplerMap plus the newer schedulers A1111 writes as suffixes).
const samplerNames=new Map<string,string>([['euler a','euler_ancestral'],['euler','euler'],['lms','lms'],['heun','heun'],['dpm2','dpm_2'],['dpm2 a','dpm_2_ancestral'],['dpm++ 2s a','dpmpp_2s_ancestral'],['dpm++ 2m','dpmpp_2m'],['dpm++ sde','dpmpp_sde'],['dpm++ 2m sde','dpmpp_2m_sde'],['dpm++ 2m sde heun','dpmpp_2m_sde'],['dpm++ 3m sde','dpmpp_3m_sde'],['dpm fast','dpm_fast'],['dpm adaptive','dpm_adaptive'],['ddim','ddim'],['ddpm','ddpm'],['uni_pc','uni_pc'],['unipc','uni_pc'],['lcm','lcm'],['restart','res_multistep']]);
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
 return {positive:prompt,negative:negative.join('Negative prompt:').trim(),checkpoint:str(fields.Model),seed:num(fields.Seed),steps:num(fields.Steps),cfg:num(fields['CFG scale']),sampler:mapped?.sampler,scheduler:schedule??mapped?.scheduler,width,height,loras:loras.length?loras:undefined,skipped:skipped.length?skipped:undefined};
}
/** ComfyUI API graph: the KSampler and what feeds it, as DEAS and the Civitai parser read it. */
function comfy(graph:Record<string,unknown>):GenerationSettings|null {
 const nodes=Object.values(graph).filter(object);
 const node=(v:unknown)=>Array.isArray(v)&&typeof v[0]==='string'?graph[v[0]]:undefined;
 const inputs=(n:unknown)=>object(n)&&object(n.inputs)?n.inputs:{};
 const sampler=nodes.find(n=>typeof n.class_type==='string'&&/^KSampler/.test(n.class_type));if(!sampler)return null;
 const i=inputs(sampler);
 const textOf=(link:unknown)=>{const t=inputs(node(link)).text;return typeof t==='string'?t:undefined;};
 const latent=inputs(node(i.latent_image));
 const loader=nodes.find(n=>n.class_type==='CheckpointLoaderSimple'||n.class_type==='UNETLoader');
 const loras=nodes.filter(n=>n.class_type==='LoraLoader'||n.class_type==='LoraLoaderModelOnly').map(n=>({name:String(inputs(n).lora_name??''),strength:num(inputs(n).strength_model)??1})).filter(l=>l.name);
 const li=inputs(loader);
 return {positive:textOf(i.positive),negative:textOf(i.negative),checkpoint:str(li.ckpt_name??li.unet_name),seed:num(i.seed??i.noise_seed),steps:num(i.steps),cfg:num(i.cfg),sampler:str(i.sampler_name),scheduler:str(i.scheduler),width:num(latent.width),height:num(latent.height),loras:loras.length?loras:undefined};
}
/** SwarmUI `sui_image_params`. */
function swarm(p:Record<string,unknown>):GenerationSettings {
 const names=typeof p.loras==='string'?p.loras.split(',').map(s=>s.trim()).filter(Boolean):Array.isArray(p.loras)?p.loras.map(String):[];
 const weights=typeof p.loraweights==='string'?p.loraweights.split(','):Array.isArray(p.loraweights)?p.loraweights:[];
 const loras=names.map((name,i)=>({name,strength:num(weights[i])??1}));
 return {positive:str(p.prompt),negative:str(p.negativeprompt),checkpoint:str(p.model),seed:num(p.seed),steps:num(p.steps),cfg:num(p.cfgscale),sampler:str(p.sampler),scheduler:str(p.scheduler),width:num(p.width),height:num(p.height),loras:loras.length?loras:undefined};
}
/** Reusable generation settings from embedded metadata (A1111 text first, like Civitai; then Swarm; then a ComfyUI graph), or null. */
export function generationSettings(entries:ImageMetadata):GenerationSettings|null {
 let found:GenerationSettings|null=null;
 const byKey=(key:string)=>entries.find(([k])=>k.toLowerCase()===key)?.[1];
 const parameters=byKey('parameters')??byKey('usercomment')??byKey('imagedescription');
 if(parameters){let parsed:unknown;try{parsed=JSON.parse(parameters);}catch{parsed=undefined;}found=object(parsed)&&object(parsed.sui_image_params)?swarm(parsed.sui_image_params):a1111(parameters);}
 const prompt=byKey('prompt');
 if(!found&&prompt){try{const parsed:unknown=JSON.parse(prompt);if(object(parsed))found=comfy(parsed);}catch{found=null;}}
 if(!found)return null;
 // The DaSiWa metadata saver embeds `*_not_found` sentinels for empty/undetected fields: they mean absent, not a value.
 const clean=Object.fromEntries(Object.entries(found).filter(([,v])=>v!==undefined&&v!==''&&!(typeof v==='string'&&/^[a-z_]+_not_found(?:\.safetensors)?$/.test(v)))) as GenerationSettings;
 return clean.positive!==undefined||clean.seed!==undefined||clean.steps!==undefined?clean:null;
}
