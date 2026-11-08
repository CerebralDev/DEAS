import { useEffect, useRef, useState } from 'react';
import { useI18n } from '../../i18n/I18nProvider';
import { cancelRefMod, refModStatus, submitRefMod, type RefModConcept, type RefModCreated } from '../../lib/jobs';
import { slotsOf, type VideoRef } from './ReferencesPanel';

const concepts:RefModConcept[]=['generic','identity','style','motion','audio'];
type Phase='idle'|'uploading'|'queued'|'running'|'done'|'failed'|'cancelled';
const busyPhases:Phase[]=['uploading','queued','running'];

export interface RefModCreateProps { endpoint:string; refs:VideoRef[]; /** Director tags (`<Picture 1>`…) per reference. */ tags:string[]; /** Saved RefMod rows that can be re-bundled. */ saved:string[]; vae:string; audioVae:string; /** Copies a local file into the backend input folder; returns its `deas-inputs/…` name. */ upload:(path:string)=>Promise<string>; onSaved:(name:string)=>void }

/** Director *Create RefMod*: encodes the checked references into one new file in `models/refmods`. References and prompt stay unchanged. */
export function RefModCreate({endpoint,refs,tags,saved,vae,audioVae,upload,onSaved}:RefModCreateProps){
 const {t}=useI18n();
 const [name,setName]=useState(''),[concept,setConcept]=useState<RefModConcept>('generic'),[description,setDescription]=useState('');
 // Like the Director, every row starts checked; descriptions default to the reference's own description.
 const [picked,setPicked]=useState(()=>refs.map(()=>true)),[pickedSaved,setPickedSaved]=useState(()=>saved.map(()=>true));
 const [notes,setNotes]=useState(()=>refs.map(r=>r.description||r.instructions||''));
 const [phase,setPhase]=useState<Phase>('idle'),[error,setError]=useState(''),[result,setResult]=useState<RefModCreated|null>(null);
 const prompt=useRef(''),alive=useRef(true);
 useEffect(()=>()=>{alive.current=false;},[]);
 const chosen=refs.filter((_,i)=>picked[i]);
 const visual=chosen.some(r=>r.item.media==='image'||(r.item.media==='video'&&r.mediaMode!=='audio'));
 const sound=chosen.some(r=>r.item.media==='audio'||(r.item.media==='video'&&r.mediaMode!=='video'));
 const reason=!chosen.length&&!pickedSaved.some(Boolean)?'studio.run.video.refmodNothing':visual&&!vae?'studio.run.video.refmodNeedsVae':sound&&!audioVae?'studio.run.video.refmodNeedsAudioVae':'';
 const busy=busyPhases.includes(phase),locked=busy||phase==='done';
 async function create(){
  setError('');setResult(null);setPhase('uploading');prompt.current='';
  try{
   const indices=refs.flatMap((_,i)=>picked[i]?[i]:[]);const slots=slotsOf(chosen);
   const files=await Promise.all(chosen.map(r=>upload(r.item.path!)));
   if(!alive.current)return;
   const media=chosen.map((r,i)=>({type:r.item.media as 'image'|'video'|'audio',file:files[i],slot:slots[i],...(r.item.media==='video'?{mediaMode:r.mediaMode??'video'}:{}),
    ...(r.item.media!=='image'&&r.trimStart!==undefined?{trimStart:r.trimStart}:{}),...(r.item.media!=='image'&&r.trimEnd!==undefined?{trimEnd:r.trimEnd}:{}),
    ...(notes[indices[i]].trim()?{description:notes[indices[i]].trim()}:{})}));
   const {promptId}=await submitRefMod(endpoint,{name:name.trim(),description:description.trim(),conceptType:concept,media,refmods:saved.filter((_,i)=>pickedSaved[i]),...(visual?{vae}:{}),...(sound?{audioVae}:{})});
   prompt.current=promptId;
   for(;;){
    if(!alive.current)return;
    const s=await refModStatus(endpoint,promptId);
    if(!alive.current)return;
    if(s.state==='done'){setResult(s.result);setPhase('done');onSaved(s.result.name);return;}
    if(s.state==='failed'){setError(s.error);setPhase('failed');return;}
    if(s.state==='cancelled'||s.state==='missing'){setPhase('cancelled');return;}
    setPhase(s.state);
    await new Promise(resolve=>setTimeout(resolve,1000));
   }
  }catch(e){if(alive.current){setError(e instanceof Error?e.message:String(e));setPhase('failed');}}
 }
 const cancel=()=>{if(prompt.current)cancelRefMod(endpoint,prompt.current).catch(e=>{if(alive.current)setError(e instanceof Error?e.message:String(e));});};
 return <div className="refmod-create" data-refmod-create-form>
  <p className="media-help">{t('studio.run.video.refmodCreateHint')}</p>
  <label><span>{t('studio.run.video.refmodName')}</span><input type="text" data-refmod-name maxLength={120} placeholder={t('studio.run.video.refmodNameAuto')} value={name} disabled={locked} onChange={e=>setName(e.target.value)}/></label>
  <label><span>{t('studio.run.video.refmodConcept')}</span><select data-refmod-concept value={concept} disabled={locked} onChange={e=>setConcept(e.target.value as RefModConcept)}>{concepts.map(c=><option key={c} value={c}>{t(`studio.run.video.refmodConcept.${c}`)}</option>)}</select></label>
  <label><span>{t('studio.run.video.refmodBundleDescription')}</span><textarea data-refmod-description maxLength={4000} value={description} disabled={locked} onChange={e=>setDescription(e.target.value)}/></label>
  <ul className="refmod-create-members">
   {refs.map((r,i)=><li key={r.item.src}>
    <label><input type="checkbox" checked={picked[i]} disabled={locked} onChange={e=>setPicked(p=>p.map((v,m)=>m===i?e.target.checked:v))}/><span className="media-mark">{tags[i]}</span> {r.item.name}</label>
    <input type="text" aria-label={t('studio.run.video.refmodMemberDescriptionOf',{name:r.item.name})} maxLength={2000} value={notes[i]} disabled={locked||!picked[i]} onChange={e=>setNotes(n=>n.map((v,m)=>m===i?e.target.value:v))}/>
   </li>)}
   {saved.map((n,i)=><li key={`saved:${n}:${i}`}>
    <label><input type="checkbox" checked={pickedSaved[i]} disabled={locked} onChange={e=>setPickedSaved(p=>p.map((v,m)=>m===i?e.target.checked:v))}/><span className="media-mark">{`<RefMod ${i+1}>`}</span> {n}</label>
   </li>)}
  </ul>
  <div className="wb-ref-toolbar">
   <button type="button" className="wb-dock-action" data-refmod-create-run disabled={locked||!!reason} title={reason?t(reason):undefined} onClick={()=>void create()}>{t('studio.run.video.refmodCreateRun')}</button>
   {busy&&<button type="button" className="wb-dock-action" data-refmod-cancel disabled={!prompt.current} onClick={cancel}>{t('studio.run.video.refmodCancel')}</button>}
  </div>
  {reason&&!locked&&<p className="media-help">{t(reason)}</p>}
  <p role="status" data-refmod-status>{phase==='uploading'?t('studio.run.video.refmodUploading'):phase==='queued'?t('studio.run.video.refmodQueued'):phase==='running'?t('studio.run.video.refmodRunning'):phase==='cancelled'?t('studio.run.video.refmodCancelled'):result?t('studio.run.video.refmodSaved',{name:result.name,members:String(result.members)}):''}</p>
  {result?.warnings.map(w=><p key={w} className="media-help">{w}</p>)}
  {error&&<p className="error-message" role="alert">{error}</p>}
 </div>;
}
