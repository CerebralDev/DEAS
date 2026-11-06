import { useEffect, useRef, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { useI18n } from '../../i18n/I18nProvider';
import { describeMedia, dropCssPoint, draggedItem, itemFromEntry, MEDIA_DRAG, normalizedMediaPath, relativeMediaPath, type MediaItem } from '../../lib/media';
import type { RefModEntry } from '../../lib/jobs';
import { labelCharacters, readPeopleBoxes, type PeopleBox, type H3Reference } from '../../lib/forge';
import type { H3Context } from '../forge/ForgeButton';
import {PeopleBoxes} from './PeopleBoxes';
import {PeopleDetector} from './PeopleDetector';
import {ReferenceMedia} from './ReferenceMedia';
import {ReferenceEditorDialog} from './ReferenceEditorDialog';
import {ReferenceThumbnail} from './ReferenceThumbnail';

/** A Multi-reference → Video pick: LTX uses images at a second with a guide strength; H3 (REF2VA) also takes videos/audio, optionally trimmed (seconds). */
export interface VideoRef { item:MediaItem; second:number; strength:number; mediaMode:'video'|'audio'|'video_audio'; trimStart?:number; trimEnd?:number; sourceDuration?:number;
 /** Director Forge picture label (REF2VA images; unset = next free Character); `instructions` = what to take from this reference. */ label?:string; instructions?:string; who?:number[]; whoAxis?:'y'|'z'|'none'; boxes?:PeopleBox[];
 /** Forge keep / drop details (`forge_keep`/`forge_drop`) and the prefill's user description (the Director's `prompt`, not in packs). */ keep?:string; drop?:string; description?:string }
/** Director picture labels (individual characters 1–32 and its seven left-to-right groups). */
export const pictureLabels=[...Array.from({length:32},(_,i)=>`character-${i+1}`),'group-12','group-21','group-13','group-31','group-23','group-32','group-123','place','style','first-frame','last-frame','pose','custom'];
/** Forge labels per reference as the Director assigns them (`pictureLabels`): a chosen label is kept, other images take the next free Character number; non-images get none. */
export const forgeLabels=(refs:VideoRef[])=>{
 const used=new Set(refs.flatMap(r=>r.item.media==='image'&&pictureLabels.includes(r.label??'')?(r.boxes?.length?r.boxes.map(b=>String(b.n)):r.who?.map(String)??labelCharacters(r.label)):[]));
 const take=()=>{let n=1;while(used.has(String(n)))n++;used.add(String(n));return `character-${n}`;};
 return refs.map(r=>r.item.media!=='image'?'':r.label&&pictureLabels.includes(r.label)?r.label:take());
};
export type FramePeople = Pick<VideoRef,'who'|'whoAxis'|'boxes'>;
const labelRole=(label:string)=>label.startsWith('character-')?{role:'subject',subject_group:'ABCDEFGHIJKLMNOPQRSTUVWXYZ'[Number(label.slice(10))-1]??label}:{role:({style:'style','first-frame':'keyframe','last-frame':'keyframe',pose:'pose',custom:'custom'} as Record<string,string>)[label]??'subject',subject_group:''};
type Lane='image'|'video'|'audio';
/** Director lane of a reference: a video used only as sound sits in the audio lane. */
export const laneOf=(r:VideoRef):Lane=>r.item.media==='video'&&r.mediaMode==='audio'?'audio':r.item.media as Lane;
/** Slot per lane in list order — the Director's `<Picture N>`/`<Video N>`/`<Audio N>` numbering is slot + 1. */
export const slotsOf=(refs:VideoRef[])=>{const used:Record<string,number>={};return refs.map(r=>used[laneOf(r)]=(used[laneOf(r)]??-1)+1);};
/** The reference that sets Auto aspect, as the Director's `sourceDimensions` picks it: lowest slot, then list order, among pictures and video (sound-only uses don't count). */
export const firstVisualRef=(refs:VideoRef[])=>{const slots=slotsOf(refs);return refs.map((_,i)=>i).filter(i=>refs[i].item.media==='image'||(refs[i].item.media==='video'&&refs[i].mediaMode!=='audio')).sort((a,b)=>slots[a]-slots[b]||a-b).map(i=>refs[i])[0];};
const tagName={image:'Picture',video:'Video',audio:'Audio'} as const;
/** Director prompt labels per reference (`<Picture 1>`, `<Video 2> <Audio 1>`), numbered as the node loads media: images, then video items
 * (picture and/or sound), then audio files, each by slot — the same order as the Director's own label prefill. */
export const directorTags=(refs:VideoRef[])=>{
 const slots=slotsOf(refs);const rank={image:0,video:1,audio:2} as Record<string,number>;const n={image:0,video:0,audio:0};const out=refs.map(()=>[] as string[]);
 for(const i of refs.map((_,i)=>i).sort((a,b)=>rank[refs[a].item.media??'']-rank[refs[b].item.media??'']||slots[a]-slots[b]||a-b)){
  const r=refs[i];const kinds=r.item.media==='video'?(['video','audio'] as const).filter(k=>k==='video'?r.mediaMode!=='audio':r.mediaMode!=='video'):[r.item.media as 'image'|'audio'];
  for(const k of kinds)out[i].push(`<${tagName[k]} ${++n[k]}>`);
 }
 return out.map(x=>x.join(' '));
};
/** Director Forge references in node order (`forgeReferences`): images as subjects (REF2VA) or keyframes, videos as motion with their sound stream, audio;
 * `path` only for files in the input folder (the node reads pictures from there), clip length only when both trims are set. */
export function forgeReferences(refs:VideoRef[],mode:string,inputRoot?:string):H3Reference[]{
 const slots=slotsOf(refs);const rank={image:0,video:1,audio:2} as Record<string,number>;
 const rel=(r:VideoRef)=>{const path=inputRoot?relativeMediaPath(r.item.path??'',inputRoot):'';return path?{path}:{};};
 const length=(r:VideoRef)=>{const seconds=referenceSeconds(r);return seconds===undefined?{}:{duration_seconds:seconds};};
 const labels=forgeLabels(refs);
 return refs.map((r,i)=>({r,i})).sort((a,b)=>rank[a.r.item.media??'']-rank[b.r.item.media??'']||slots[a.i]-slots[b.i]||a.i-b.i).map(({r}):H3Reference=>
  ({...(r.item.media==='image'?{kind:'image' as const,...rel(r),...(mode==='REF2VA'?{...labelRole(labels[refs.indexOf(r)]),easy_role:labels[refs.indexOf(r)],...(r.boxes?.length?{boxes:r.boxes}:{}),...(labelCharacters(labels[refs.indexOf(r)]).length&&(r.who?.length||r.whoAxis)?{picture_kinds:['character'],who:r.who??labelCharacters(labels[refs.indexOf(r)]).map(Number),...(r.whoAxis?{who_axis:r.whoAxis}:{})}:{})}:{role:'keyframe',subject_group:'',...(r.who?.length?{who:r.who}:{}),...(r.whoAxis?{who_axis:r.whoAxis}:{}),...(r.boxes?.length?{boxes:r.boxes}:{})})}
  :r.item.media==='video'?{kind:'video' as const,role:'motion',stream:r.mediaMode==='audio'?'audio' as const:r.mediaMode==='video_audio'?'both' as const:'video' as const,...length(r)}
  :{kind:'audio' as const,...length(r)}),...(r.instructions?.trim()?{instructions:r.instructions.trim()}:{}),...(r.keep?.trim()?{keep:r.keep.trim()}:{}),...(r.drop?.trim()?{drop:r.drop.trim()}:{})}));
}
/** What the Director Forge gets for a MiniMax H3 video run: the mode and references the intent will carry (REF2VA for
 * Video → Video and Multi-reference, else the first/last-frame mode), the duration and the exact output canvas. */
export function h3ForgeContext(o:{pipeline:string;media:MediaItem|null;lastFrame:MediaItem|null;refs:VideoRef[];sourceAudio:boolean;seconds:number;canvas:{width:number;height:number};inputRoot?:string;framePeople?:Record<string,FramePeople>}):H3Context{
 const ref=(item:MediaItem,mediaMode:VideoRef['mediaMode']='video'):VideoRef=>({item,second:0,strength:1,mediaMode});
 const base={duration:o.seconds,canvas:{width:o.canvas.width,height:o.canvas.height}};
 if(o.pipeline==='v2v'||o.pipeline==='ref2v'){
  const picks=o.pipeline==='v2v'?(o.media?.media==='video'?[ref(o.media,o.sourceAudio?'video_audio':'video')]:[]):o.refs;
  // Picture labels (the Director's easy mode) apply when every reference is a labelled picture.
  return {...base,mode:'REF2VA',references:forgeReferences(picks,'REF2VA',o.inputRoot),easy:picks.length>0&&picks.every(r=>r.item.media==='image')};
 }
 const first=o.pipeline==='i2v'&&o.media?.media==='image'?o.media:null;
 const frames=[first,o.lastFrame].flatMap(i=>i?[{...ref(i),...o.framePeople?.[i.src]}]:[]);
 return {...base,mode:first&&o.lastFrame?'FL2VA':first?'I2VA':o.lastFrame?'L2VA':'T2VA',references:forgeReferences(frames,'I2VA',o.inputRoot),easy:false};
}
/** Timeline references in the Director prefill's order (images, video items, audio files; each by slot). */
export const prefillItems=(refs:VideoRef[])=>{const slots=slotsOf(refs);const rank={image:0,video:1,audio:2} as Record<string,number>;
 return refs.map((r,i)=>({type:r.item.media as Lane,slot:slots[i],mediaMode:r.mediaMode,description:r.description??'',i})).sort((a,b)=>rank[a.type]-rank[b.type]||a.slot-b.slot||a.i-b.i).map(({i:_,...x})=>x);};
/** Counts the Director limits apply to; a video with sound counts as video and sound track (as native validation does). */
export const refCounts=(refs:VideoRef[])=>({images:refs.filter(r=>r.item.media==='image').length,videos:refs.filter(r=>r.item.media==='video'&&r.mediaMode!=='audio').length,
 audio:refs.filter(r=>r.item.media==='audio'||(r.item.media==='video'&&r.mediaMode!=='video')).length,files:refs.length});
export const over=(c:ReturnType<typeof refCounts>)=>c.images+c.videos+c.audio>12||c.images>9||c.videos>3||c.audio>3;
/** Director REF2VA limits: 9 images, 3 videos, 3 sound tracks, 12 files. */
export const h3Full=(refs:VideoRef[],item:MediaItem)=>over(refCounts([...refs,{item,second:0,strength:1,mediaMode:'video'}]));
/** The Director takes 2–15 s per clip; checkable here only when both trim ends are set (the node checks the rest). */
export const referenceSeconds=(r:VideoRef)=>(r.trimEnd??r.sourceDuration)===undefined?undefined:(r.trimEnd??r.sourceDuration!)-(r.trimStart??0);
export const badTrim=(r:VideoRef)=>{if(r.item.media==='image')return false;const seconds=referenceSeconds(r);return seconds!==undefined&&(!(seconds>=2&&seconds<=15)||r.sourceDuration!==undefined&&(r.trimEnd??r.sourceDuration)>r.sourceDuration);};
/** Why the reference list cannot run as is (locale key), else ''. */
export const refsProblem=(refs:VideoRef[])=>{
 if(over(refCounts(refs)))return 'studio.run.video.refFull';
 if(refs.some(badTrim))return 'studio.run.video.trimInvalid';
 for(const kind of ['video','audio'] as const){
  const total=refs.reduce((sum,r)=>{const used=r.item.media==='video'?(kind==='video'?r.mediaMode!=='audio':r.mediaMode!=='video'):r.item.media===kind;return sum+(used?(referenceSeconds(r)??0):0);},0);
  if(total>15)return 'studio.run.video.trimTotal';
 }
 return '';
};
/** A RefMod row (saved REF2VA reference in `models/refmods`); its Director slot is its position + 1. */
export interface RefMod { name:string; strength:number; description:string; /** Library media kinds; unknown for rows loaded from a pack until the library is read. */ kinds?:string[] }
/** A reference to add; omitted fields take the defaults (picture-only video, untrimmed). */
export type RefAdd={item:MediaItem}&Partial<Pick<VideoRef,'mediaMode'|'trimStart'|'trimEnd'|'label'|'instructions'|'keep'|'drop'|'who'|'whoAxis'|'boxes'>>;
const PACK='dasiwa_minimax_h3_reference_pack';
const rank={image:0,video:1,audio:2} as Record<string,number>;

/** A Director reference pack (its `saveReferencePack` "files" format): items hold input-folder-relative names, so references outside the input folder are skipped and named. */
export function toPack(refs:VideoRef[],inputRoot:string,refmods:RefMod[]=[]){
 const slots=slotsOf(refs);const ranks:Record<string,number>={};const skipped:string[]=[];
 const items=refs.map((r,i)=>({r,i})).sort((a,b)=>rank[a.r.item.media??'']-rank[b.r.item.media??'']||slots[a.i]-slots[b.i]||a.i-b.i).flatMap(({r})=>{
  const path=relativeMediaPath(r.item.path??'',inputRoot);if(!path){skipped.push(r.item.name);return [];}
  const type=r.item.media as Lane;
  return [{type,value:path,...(type==='video'?{media_mode:r.mediaMode}:{}),...(type==='image'&&r.label?{forge_label:r.label}:{}),...(type==='image'&&r.boxes?.length?{forge_boxes:JSON.stringify(r.boxes)}:{}),...(type==='image'&&r.who?.length?{forge_kinds:'character',forge_who:r.who.join(','),...(r.whoAxis?{forge_who_axis:r.whoAxis}:{})}:{}),...(r.instructions?.trim()?{forge_instructions:r.instructions.trim()}:{}),...(r.keep?.trim()?{forge_keep:r.keep.trim()}:{}),...(r.drop?.trim()?{forge_drop:r.drop.trim()}:{}),...(r.trimStart!==undefined?{trim_start:r.trimStart}:{}),...(r.trimEnd!==undefined?{trim_end:r.trimEnd}:{}),_rank:ranks[type]=(ranks[type]??-1)+1}];
 });
  const rows=refmods.map((r,i)=>({slot:i+1,name:r.name,enabled:true,strength:r.strength,description:r.description}));
 return {pack:{[PACK]:true,schema_version:1,saved_at:new Date().toISOString(),model_mode:'REF2VA',items,...(rows.length?{refmods:rows}:{})},skipped};
}
const safeValue=(v:unknown):v is string=>typeof v==='string'&&v.length>0&&v.length<=1024&&!/^[\\/]|^[A-Za-z]:|\\|[\u0000-\u001f]/.test(v)&&v.split('/').every(s=>s&&s!=='.'&&s!=='..');
const seconds=(v:unknown)=>typeof v==='number'&&Number.isFinite(v)&&v>0&&v<=3600?v:undefined;
/** Reads a Director reference pack into input-folder paths in Director order (type, then rank); unsafe names are dropped and counted. */
export function fromPack(json:unknown,inputRoot:string):{adds:{path:string;mediaMode?:VideoRef['mediaMode'];trimStart?:number;trimEnd?:number;label?:string;instructions?:string;keep?:string;drop?:string;who?:number[];whoAxis?:VideoRef['whoAxis'];boxes?:PeopleBox[]}[];refmods:RefMod[];invalid:number}|null{
 const p=json as {[PACK]?:unknown;items?:unknown;refmods?:unknown};
 if(!p||typeof p!=='object'||p[PACK]!==true||!Array.isArray(p.items))return null;
 const root=normalizedMediaPath(inputRoot);let invalid=0;
 const items=(p.items as Record<string,unknown>[]).filter(x=>{const ok=!!x&&typeof x==='object'&&typeof x.type==='string'&&x.type in rank&&safeValue(x.value);if(!ok)invalid++;return ok;})
  .sort((a,b)=>rank[a.type as string]-rank[b.type as string]||(Number(a._rank)||0)-(Number(b._rank)||0));
 // Enabled RefMod rows in slot order; the slot itself is re-derived from the position (DEAS keeps them packed).
 const strengthOf=(r:Record<string,unknown>)=>r.strength===undefined||r.strength===''?1:Number(r.strength); // the Director defaults to 1 and accepts numeric strings
 const refmods=(Array.isArray(p.refmods)?p.refmods as Record<string,unknown>[]:[]).filter(r=>r&&typeof r==='object'&&r.enabled!==false).filter(r=>{const ok=safeValue(r.name)&&strengthOf(r)>0&&strengthOf(r)<=1;if(!ok)invalid++;return ok;})
  .sort((a,b)=>Number(a.slot)-Number(b.slot)).slice(0,8).map(r=>({name:r.name as string,strength:strengthOf(r),description:typeof r.description==='string'?r.description.slice(0,2000):''}));
 const people=new Map<Record<string,unknown>,{who:number[];whoAxis?:VideoRef['whoAxis']}>();
 for(const x of items){
  if(x.type!=='image'||x.forge_kinds!=='character'||x.forge_who===undefined)continue;
  const raw=x.forge_who;const who=typeof raw==='string'&&/^(?:[1-9]|[12][0-9]|3[0-2])(?:,(?:[1-9]|[12][0-9]|3[0-2]))*$/.test(raw)?raw.split(',').map(Number):[];
  if(!who.length||who.length>32||new Set(who).size!==who.length||!pictureLabels.includes(x.forge_label as string)||!labelCharacters(x.forge_label as string).length){invalid++;continue;}
  const axis=x.forge_who_axis;
  if(axis!==undefined&&axis!==''&&!['y','z','none'].includes(axis as string)){invalid++;continue;}
  people.set(x,{who,...(axis?{whoAxis:axis as VideoRef['whoAxis']}:{})});
 }
 const boxed=new Map<Record<string,unknown>,PeopleBox[]>();
 for(const x of items){
  if(x.type!=='image'||x.forge_boxes===undefined)continue;
  let boxes:PeopleBox[]|null=null;try{boxes=typeof x.forge_boxes==='string'?readPeopleBoxes(JSON.parse(x.forge_boxes)):null;}catch{/* Invalid pack metadata is counted, never applied. */}
  if(boxes===null||!pictureLabels.includes(x.forge_label as string)||!labelCharacters(x.forge_label as string).length){invalid++;continue;}
  boxed.set(x,boxes);
 }
 return {invalid,refmods,adds:items.map(x=>({path:`${root}/${x.value}`,...people.get(x),...(boxed.has(x)?{boxes:boxed.get(x)}:{}),...(x.type==='video'&&['video','audio','video_audio'].includes(x.media_mode as string)?{mediaMode:x.media_mode as VideoRef['mediaMode']}:{}),
  ...(x.type!=='image'&&seconds(x.trim_start)!==undefined?{trimStart:seconds(x.trim_start)}:{}),...(x.type!=='image'&&seconds(x.trim_end)!==undefined?{trimEnd:seconds(x.trim_end)}:{}),
  ...(x.type==='image'&&pictureLabels.includes(x.forge_label as string)?{label:x.forge_label as string}:{}),...(typeof x.forge_instructions==='string'&&x.forge_instructions.trim()?{instructions:x.forge_instructions.slice(0,4000)}:{}),...(typeof x.forge_keep==='string'&&x.forge_keep.trim()?{keep:x.forge_keep.slice(0,2000)}:{}),...(typeof x.forge_drop==='string'&&x.forge_drop.trim()?{drop:x.forge_drop.slice(0,2000)}:{})}))};
}
const lanes:Record<string,Lane[]>={'minimax-h3':['image','video','audio'],'minimax-h3-hyperflow':['image','video','audio'],'ltx-2':['image']};

export function ReferencePeople({r,label,onChange,endpoint,root,inputRoot,allowEmpty=false,prominent=false,title}:{r:VideoRef;label:string;onChange:(x:Partial<VideoRef>)=>void;endpoint:string;root?:string;inputRoot?:string;allowEmpty?:boolean;/** Prompt Forge tab: an always-open titled section instead of a collapsed menu. */prominent?:boolean;title?:string}){
 const {t}=useI18n();const [expanded,setOpen]=useState(false);const open=prominent||expanded;const who=r.who??labelCharacters(label).map(Number);
 const labelOf=(next:number[],axis?:VideoRef['whoAxis'])=>{const group=`group-${next.join('')}`;return !next.length?'':!axis&&next.length>1&&pictureLabels.includes(group)?group:`character-${next[0]}`;};
 const toggle=(n:number)=>{const next=who.includes(n)?who.filter(x=>x!==n):[...who,n];onChange({who:next,label:labelOf(next,r.whoAxis)});};
 const body=<>
  <p className="media-help">{t('forge.peopleHint')}</p>
  {open&&<PeopleDetector endpoint={endpoint} root={root} reference={forgeReferences([{...r,label}],'REF2VA',inputRoot)[0]} onApply={boxes=>onChange({boxes,label:labelOf([...boxes].sort((a,b)=>a.box[0]+a.box[2]-b.box[0]-b.box[2]).map(b=>b.n))})}/>}
  {open&&<PeopleBoxes src={r.item.src} boxes={r.boxes??[]} onChange={boxes=>onChange({boxes,...(boxes.length?{label:labelOf([...boxes].sort((a,b)=>a.box[0]+a.box[2]-b.box[0]-b.box[2]).map(b=>b.n))}:{})})}/>}
  {!r.boxes?.length&&<><div className="wb-ref-actions">{Array.from({length:Math.min(32,Math.max(6,...who.map(n=>n+1)))},(_,i)=>i+1).map(n=><button key={n} type="button" aria-label={t('forge.personIn',{n:String(n),name:r.item.name})} aria-pressed={who.includes(n)} disabled={!allowEmpty&&who.length===1&&who[0]===n} onClick={()=>toggle(n)}>{n}</button>)}</div>
  <p className="media-help">{t('forge.peopleSelected',{n:who.join(' → ')})}</p>
  <label><span>{t('forge.order')}</span><select aria-label={t('forge.orderIn',{name:r.item.name})} value={r.whoAxis??'x'} onChange={e=>{const axis=e.target.value==='x'?undefined:e.target.value as VideoRef['whoAxis'];onChange({label:labelOf(who,axis),who,whoAxis:axis});}}>{(['x','y','z','none'] as const).map(axis=><option key={axis} value={axis}>{t(`forge.axis.${axis}`)}</option>)}</select></label></>}
 </>;
 return prominent?<section className="forge-people" data-ref-people aria-label={title?`${t('forge.people')} · ${title}`:t('forge.people')}><h3>{t('forge.people')}{title&&<span> · {title}</span>}</h3>{body}</section>
  :<details className="wb-ref-more" data-ref-people onToggle={e=>setOpen(e.currentTarget.open)}><summary>{t('forge.people')}</summary>{body}</details>;
}

export interface ReferencesPanelProps { endpoint?:string; root?:string; refs:VideoRef[]; onChange:(refs:VideoRef[])=>void; family:'ltx-2'|'minimax-h3'|'minimax-h3-hyperflow'; /** Video length: LTX reference seconds stay inside it. */ seconds:number; /** Adds references in order (dropped tiles/files, loaded packs); `replace` starts from an empty list. */ onAdd:(adds:RefAdd[],replace?:boolean)=>void; /** ComfyUI input folder: Director packs name files relative to it. */ inputRoot?:string; label:(r:VideoRef,i:number)=>string; /** Append a Director label such as `<Picture 1>` to the prompt. */ onInsert:(label:string)=>void; /** H3 RefMod rows (Director slots 1–8 in order). */ refmods?:RefMod[]; onRefmods?:(rows:RefMod[])=>void; /** Reads the backend's RefMod library on demand. */ library?:()=>Promise<RefModEntry[]> }
/** Multi-reference → Video editor in Director lanes; used inside the References overlay. */
export function ReferencesPanel({endpoint='',root,refs,onChange,family,seconds,label,onAdd,onInsert,inputRoot,refmods=[],onRefmods,library}:ReferencesPanelProps){
 const {t}=useI18n();
 const box=useRef<HTMLDivElement>(null);const [error,setError]=useState('');const [hovering,setHovering]=useState(false);
 const [editing,setEditing]=useState<string|null>(null);
 const add=useRef(onAdd);add.current=onAdd;
 // Native webviews deliver OS file drops as paths for the whole window; only drops over this tab add references.
 useEffect(()=>{
  if(!isTauri())return;
  let alive=true,unlisten:(()=>void)|undefined;
  const inside=(p:{x:number;y:number})=>{const r=box.current?.getBoundingClientRect(),{x,y}=dropCssPoint(p);return !!r&&Array.from(document.querySelectorAll('dialog[open]')).every(modal=>modal.contains(box.current))&&x>=r.left&&x<=r.right&&y>=r.top&&y<=r.bottom;};
  void getCurrentWebview().onDragDropEvent(e=>{
   if(e.payload.type==='leave'){setHovering(false);return;}
   setHovering(e.payload.type!=='drop'&&inside(e.payload.position));
   if(e.payload.type==='drop'&&inside(e.payload.position))describeMedia(e.payload.paths).then(entries=>add.current(entries.map(itemFromEntry).flatMap(item=>item?[{item}]:[])),err=>setError(String(err)));
  }).then(stop=>{if(alive)unlisten=stop;else stop();},err=>{if(alive)setError(String(err));});
  return()=>{alive=false;unlisten?.();};
 },[]);
 const dragged=useRef(-1); // card being reordered by drag
 const [status,setStatus]=useState('');
 // RefMod library: read on demand (a backend round trip), then picked from a list.
 const [entries,setEntries]=useState<RefModEntry[]|null>(null);const [pick,setPick]=useState('');
 const readLibrary=()=>{setError('');library?.().then(list=>{setEntries(list);setPick(list[0]?.name??'');
  // Rows from a pack learn their media kinds here (the prefill needs them).
  if(refmods.some(r=>!r.kinds))onRefmods?.(refmods.map(r=>({...r,kinds:r.kinds??list.find(e=>e.name===r.name)?.kinds})));},e=>setError(String(e)));};
 const setRow=(n:number,x:Partial<RefMod>)=>onRefmods?.(refmods.map((r,m)=>m===n?{...r,...x}:r));
 // Director reference packs: same JSON as the node's Save/Load (files only), so packs move between DEAS and ComfyUI.
 const save=async()=>{
  const {pack,skipped}=toPack(refs,inputRoot!,refmods);setError('');
  try{const saved=await invoke<boolean>('native_json_save',{title:t('studio.run.video.packSave'),name:'minimax-h3-files-pack.json',text:JSON.stringify(pack,null,2)});
   setStatus(saved?t('studio.run.video.packSaved',{count:String(pack.items.length)})+(skipped.length?` · ${t('studio.run.video.packSkipped',{names:skipped.join(', ')})}`:''):'');}
  catch(e){setError(String(e));}
 };
 const load=async(replace:boolean)=>{
  setError('');setStatus('');
  try{
   const text=await invoke<string|null>('native_json_open',{title:t(replace?'studio.run.video.packReplace':'studio.run.video.packAdd')});if(text===null)return;
   const read=fromPack(JSON.parse(text),inputRoot!);if(!read){setError(t('studio.run.video.packInvalid'));return;}
   // Missing files are reported, never added (the Director checks existence the same way before loading).
   const found=await Promise.all(read.adds.map(a=>describeMedia([a.path]).then(([e])=>e?itemFromEntry(e):null,()=>null)));
   const adds=read.adds.flatMap(({path:_,...a},i)=>{const item=found[i];return item?[{item,...a}]:[];});
   onAdd(adds,replace);
   if(onRefmods&&(replace||read.refmods.length))onRefmods([...(replace?[]:refmods),...read.refmods.map(r=>({...r,kinds:entries?.find(e=>e.name===r.name)?.kinds}))].slice(0,8));
   const missing=read.adds.length-adds.length+read.invalid;
   setStatus(t('studio.run.video.packLoaded',{count:String(adds.length)})+(missing?` · ${t('studio.run.video.packMissing',{count:String(missing)})}`:''));
  }catch(e){setError(String(e));}
 };
 const h3=family!=='ltx-2';const c=refCounts(refs);
 const set=(n:number,x:Partial<VideoRef>)=>onChange(refs.map((r,m)=>m===n?{...r,...x}:r));
 // Order matters only inside a lane (slot numbers), so a move swaps with the nearest item of the same lane.
 const neighbour=(n:number,d:-1|1)=>{for(let m=n+d;m>=0&&m<refs.length;m+=d)if(laneOf(refs[m])===laneOf(refs[n]))return m;return -1;};
 const move=(n:number,d:-1|1)=>{const m=neighbour(n,d);if(m<0)return;const next=[...refs];[next[n],next[m]]=[next[m],next[n]];onChange(next);};

 const tags=directorTags(refs);const labels=forgeLabels(refs);
 const editIndex=refs.findIndex(r=>r.item.src===editing);
 const editor=(r:VideoRef,n:number)=><>
    {/* Director Forge: which picture label (REF2VA) and what to take from this reference. */}
    {h3&&r.item.media==='image'&&<select data-ref-label aria-label={t('studio.run.video.labelOf',{name:r.item.name})} value={labels[n]} onChange={e=>set(n,{label:e.target.value,who:undefined,whoAxis:undefined,boxes:undefined})}>{pictureLabels.map(l=><option key={l} value={l}>{l.startsWith('group-')?t('forge.role.group',{n:labelCharacters(l).join(' + ')}):t(`forge.role.${l.replace(/-\d+$/,'')}`,{n:l.slice(10)})}</option>)}</select>}
    {h3&&r.item.media==='image'&&labelCharacters(labels[n]).length>0&&<ReferencePeople r={r} label={labels[n]} onChange={x=>set(n,x)} endpoint={endpoint} root={root} inputRoot={inputRoot}/>}
    {h3&&<input type="text" data-ref-instructions aria-label={t('studio.run.video.instructionsOf',{name:r.item.name})} placeholder={t(labels[n]==='custom'?'studio.run.video.instructionsNeeded':'studio.run.video.instructions')} maxLength={4000} value={r.instructions??''} onChange={e=>set(n,{instructions:e.target.value})}/>}
    {h3&&<details className="wb-ref-more"><summary>{t('studio.run.video.refDetails')}</summary>
     {(['description','keep','drop'] as const).map(key=><label key={key}><span>{t(`studio.run.video.ref.${key}`)}</span><input type="text" aria-label={t(`studio.run.video.ref.${key}Of`,{name:r.item.name})} maxLength={2000} value={r[key]??''} onChange={e=>set(n,{[key]:e.target.value})}/></label>)}
    </details>}
    <ReferenceMedia key={`${r.item.src}:${h3}`} reference={r} onChange={x=>set(n,x)}/>
 </>;
 // Dropping a card on another card of its lane moves it there; a dock tile dropped anywhere on the tab is added.
 const dropOn=(e:React.DragEvent,target:number)=>{const from=dragged.current;dragged.current=-1;if(from<0||from===target||laneOf(refs[from])!==laneOf(refs[target]))return;e.preventDefault();e.stopPropagation();const next=refs.filter((_,m)=>m!==from);next.splice(next.indexOf(refs[target])+(from<target?1:0),0,refs[from]);onChange(next);};
 return <div ref={box} className={`wb-refs-panel${hovering?' is-dragging':''}`} data-references
 onDragOver={e=>{if(dragged.current>=0||e.dataTransfer.types.includes(MEDIA_DRAG)){e.preventDefault();setHovering(dragged.current<0);}}} onDragLeave={()=>setHovering(false)}
 onDrop={e=>{setHovering(false);const item=draggedItem(e.dataTransfer);if(item){e.preventDefault();onAdd([{item}]);}}}>
  {error&&<p className="error-message" role="alert">{error}</p>}
  <div className="wb-ref-toolbar">
   {h3&&isTauri()&&inputRoot&&<><button type="button" className="wb-dock-action" disabled={!refs.length&&!refmods.length} onClick={()=>void save()}>{t('studio.run.video.packSave')}</button>
    <button type="button" className="wb-dock-action" onClick={()=>void load(false)}>{t('studio.run.video.packAdd')}</button>
    <button type="button" className="wb-dock-action" onClick={()=>void load(true)}>{t('studio.run.video.packReplace')}</button></>}
   <button type="button" className="wb-dock-action" disabled={!refs.length} onClick={()=>{onChange([]);setStatus('');}}>{t('studio.run.video.refsClear')}</button>
   {status&&<span role="status">{status}</span>}
  </div>
  <p className="media-help">{t('studio.run.video.refsDrop')}</p>
  {h3&&<p className={`wb-ref-meter${over(c)?' error-message':''}`}>{t('studio.run.video.refMeter',{images:String(c.images),videos:String(c.videos),audio:String(c.audio),files:String(c.images+c.videos+c.audio)})}</p>}
  {!refs.length&&<p className="media-help">{t(h3?'studio.run.video.refsHintH3':'studio.run.video.refsHint')}</p>}
  <div className="wb-lanes">{lanes[family].map(lane=><section key={lane} className="wb-lane" aria-label={t(`studio.run.dock.type.${lane}`)}>
   {h3&&<h4>{t(`studio.run.dock.type.${lane}`)}</h4>}
   <ul>{refs.map((r,n)=>laneOf(r)!==lane?null:<li key={r.item.src} className="wb-ref" data-ref={r.item.name} onDragOver={e=>{if(dragged.current>=0)e.preventDefault();}} onDrop={e=>dropOn(e,n)}>
    {/* The name row is the drag handle, so text selection in the fields below keeps working. */}
    <strong title={r.item.path} draggable onDragStart={e=>{dragged.current=n;e.dataTransfer.effectAllowed='move';e.dataTransfer.setData('text/plain',r.item.name);}} onDragEnd={()=>{dragged.current=-1;}}>{!h3&&r.item.media==='image'&&<img src={r.item.src} alt="" loading="lazy" draggable={false}/>}<span className="media-mark">{label(r,n)}</span> {r.item.name}</strong>
    <span className="wb-ref-actions">
     {h3&&<button type="button" data-edit-reference={r.item.src} aria-label={t('studio.run.video.editReference',{name:r.item.name})} onClick={()=>setEditing(r.item.src)}>{t('studio.run.video.edit')}</button>}
     <button type="button" aria-label={t('studio.run.video.moveEarlier',{name:r.item.name})} disabled={neighbour(n,-1)<0} onClick={()=>move(n,-1)}>←</button>
     <button type="button" aria-label={t('studio.run.video.moveLater',{name:r.item.name})} disabled={neighbour(n,1)<0} onClick={()=>move(n,1)}>→</button>
     {h3&&<button type="button" aria-label={t('studio.run.video.insertLabel',{label:tags[n]})} title={t('studio.run.video.insertLabel',{label:tags[n]})} onClick={()=>onInsert(tags[n])}>{t('studio.run.video.insert')}</button>}
     <button type="button" aria-label={t('studio.run.video.removeRef',{name:r.item.name})} onClick={()=>onChange(refs.filter((_,m)=>m!==n))}>✕</button>
    </span>
    {!h3&&<><label><span>{t('studio.run.video.refSecond')}</span><input type="number" aria-label={t('studio.run.video.refSecondOf',{name:r.item.name})} min={0} max={seconds} step={0.1} value={r.second} onChange={e=>set(n,{second:Math.min(seconds,Math.max(0,Number(e.target.value)||0))})}/></label>
     <label><span>{t('studio.run.video.refStrength')}</span><input type="number" aria-label={t('studio.run.video.refStrengthOf',{name:r.item.name})} min={0} max={1} step={0.05} value={r.strength} onChange={e=>set(n,{strength:Math.min(1,Math.max(0,Number(e.target.value)||0))})}/></label></>}
    {h3?<><div className="reference-thumb"><ReferenceThumbnail reference={r}/></div>
     <p className="reference-thumb-summary">{r.item.media==='image'?labels[n].startsWith('group-')?t('forge.role.group',{n:labelCharacters(labels[n]).join(' + ')}):t(`forge.role.${labels[n].replace(/-\d+$/,'')}`,{n:labels[n].slice(10)}):<>{r.item.media==='video'&&t(`studio.run.video.mode.${r.mediaMode}`)} · {t('studio.run.video.preview.selection',{start:(r.trimStart??0).toFixed(2),end:(r.trimEnd??r.sourceDuration)?.toFixed(2)??t('studio.run.video.preview.unknown'),seconds:referenceSeconds(r)?.toFixed(2)??t('studio.run.video.preview.unknown')})}</>}</p>
     {r.instructions&&<p className="reference-thumb-summary" title={r.instructions}>{r.instructions}</p>}
    </>:<ReferenceMedia reference={r} onChange={x=>set(n,x)}/>}
   </li>)}</ul>
  </section>)}
  {h3&&onRefmods&&<section className="wb-lane" aria-label={t('studio.run.video.refmods')} data-refmods>
   <h4>{t('studio.run.video.refmods')} · {`${refmods.length}/8`}</h4>
   <div className="wb-ref-toolbar">{entries===null?<button type="button" className="wb-dock-action" disabled={!library} onClick={readLibrary}>{t('studio.run.video.refmodsRead')}</button>
    :entries.length===0?<span>{t('studio.run.video.refmodsNone')}</span>
    :<><select aria-label={t('studio.run.video.refmods')} value={pick} onChange={e=>setPick(e.target.value)}>{entries.map(e=><option key={e.name} value={e.name}>{e.name} ({e.kinds.join(' + ')})</option>)}</select>
     <button type="button" className="wb-dock-action" disabled={!pick||refmods.length>=8} onClick={()=>onRefmods([...refmods,{name:pick,strength:1,description:entries.find(e=>e.name===pick)?.description??'',kinds:entries.find(e=>e.name===pick)?.kinds}])}>{t('studio.run.video.refmodAdd')}</button></>}</div>
   <ul>{refmods.map((r,n)=><li key={n} className="wb-ref" data-refmod={r.name}>
    <strong title={r.name}><span className="media-mark">{`<RefMod ${n+1}>`}</span> {r.name}</strong>
    <span className="wb-ref-actions"><button type="button" aria-label={t('studio.run.video.insertLabel',{label:`<RefMod ${n+1}>`})} onClick={()=>onInsert(`<RefMod ${n+1}>`)}>{t('studio.run.video.insert')}</button>
     <button type="button" aria-label={t('studio.run.video.removeRef',{name:r.name})} onClick={()=>onRefmods(refmods.filter((_,m)=>m!==n))}>✕</button></span>
    <label><span>{t('studio.run.video.refStrength')}</span><input type="number" aria-label={t('studio.run.video.refStrengthOf',{name:r.name})} min={0.05} max={1} step={0.05} value={r.strength} onChange={e=>setRow(n,{strength:Math.min(1,Math.max(0.05,Number(e.target.value)||1))})}/></label>
    <label><span>{t('studio.run.video.refmodDescription')}</span><input type="text" aria-label={t('studio.run.video.refmodDescriptionOf',{name:r.name})} maxLength={2000} value={r.description} onChange={e=>setRow(n,{description:e.target.value})}/></label>
   </li>)}</ul>
  </section>}
  </div>
  {h3&&editIndex>=0&&<ReferenceEditorDialog name={refs[editIndex].item.name} restoreFocus={()=>Array.from(box.current?.querySelectorAll<HTMLButtonElement>('[data-edit-reference]')??[]).find(b=>b.dataset.editReference===editing)??null} onClose={()=>setEditing(null)}>{editor(refs[editIndex],editIndex)}</ReferenceEditorDialog>}
 </div>;
}
