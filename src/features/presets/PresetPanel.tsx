import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { useI18n } from '../../i18n/I18nProvider';
import { applyStudioSettings, captureStudioSettings, checkStudioSettings, studioDefaults } from '../../lib/preferences';
import { changedCount, equal, getAt, leaves, overrides, resolve, setAt, type Settings } from '../../lib/presetSettings';
import { ConfirmDialog } from '../../components/ConfirmDialog';
import './presets.css';

type Preset={name:string;settings:Settings};
/** Presets hold only overrides of the Studio defaults (see lib/presetSettings); older full snapshots are reduced the same way when read. */
export function PresetPanel({disabled}:{disabled:boolean}){
 const {t}=useI18n();const [presets,setPresets]=useState<Preset[]|null>(null),[selected,setSelected]=useState(''),[error,setError]=useState(''),[status,setStatus]=useState(''),[saving,setSaving]=useState(false);
 // `editing.name===''` is a new preset: the editor shows the current overrides and asks for the name.
 const [menu,setMenu]=useState<{name:string;x:number;y:number}|null>(null),[editing,setEditing]=useState<Preset|null>(null),[ask,setAsk]=useState<{name:string;run:()=>void}|null>(null);
 const live=useRef(false),lock=useRef(false),firstItem=useRef<HTMLButtonElement>(null),list=useRef<HTMLUListElement>(null);const native=isTauri();
 useEffect(()=>{live.current=true;if(native)invoke<Preset[]>('native_presets_read').then(p=>{if(live.current)setPresets(p);},e=>{if(live.current)setError(String(e));});return()=>{live.current=false;};},[native]);
 useEffect(()=>{if(!menu)return;firstItem.current?.focus();const dismiss=(e:PointerEvent)=>{if(!(e.target as Element).closest?.('.preset-menu'))setMenu(null);};document.addEventListener('pointerdown',dismiss);return()=>document.removeEventListener('pointerdown',dismiss);},[menu]);
 const defaults=studioDefaults();
 const focusTile=(target:string)=>[...list.current?.querySelectorAll<HTMLButtonElement>('[data-preset]')??[]].find(b=>b.dataset.preset===target)?.focus();
 /** Write the whole list and read it back before reporting success. */
 const persist=async(next:Preset[],target:string)=>{
  if(lock.current)return false;
  lock.current=true;setSaving(true);setError('');setStatus('');
  try{
   await invoke('native_presets_write',{presets:next});
   const stored=await invoke<Preset[]>('native_presets_read');
   if(live.current){setPresets(stored);setSelected(target);setStatus(t('presets.saved'));}
   return true;
  }catch(e){if(live.current)setError(String(e));return false;}
  finally{lock.current=false;if(live.current)setSaving(false);}
 };
 const current=()=>overrides(captureStudioSettings(),studioDefaults());
 const overwrite=(target:string)=>{
  if(!presets||disabled)return;
  setAsk({name:target,run:()=>void persist(presets.map(p=>p.name===target?{name:target,settings:current()}:p),target)});
 };
 const apply=(preset:Preset)=>{
  if(disabled||saving)return;
  setError('');setStatus('');setSelected(preset.name);
  const now=studioDefaults();
  if(!applyStudioSettings(resolve(overrides(preset.settings,now),now))){setError(t('presets.invalid'));if(native)void invoke('native_presets_invalid').catch(()=>{});return;}
  setStatus(t('presets.loaded'));
 };
 const openMenu=(target:string,x:number,y:number)=>{setSelected(target);setMenu({name:target,x,y});};
 const closeMenu=()=>{if(menu){setMenu(null);focusTile(menu.name);}};
 const chosen=menu&&presets?.find(p=>p.name===menu.name);
 return <section className="presets" aria-label={t('presets.title')}>
  <p className="aio-help">{t('presets.help')}</p>
  {!native?<p>{t('presets.native')}</p>:<>
   <div className="preset-controls"><button type="button" disabled={!presets||disabled||saving||presets.length>=100} onClick={()=>{setStatus('');setEditing({name:'',settings:current()});}}>{t('presets.save')}</button></div>
   {presets?.length===0&&<p className="aio-help">{t('presets.empty')}</p>}
   {!!presets?.length&&<ul ref={list} className="wb-tiles preset-tiles" aria-label={t('presets.list')}>{presets.map(p=><li key={p.name}>
    <button type="button" data-preset={p.name} aria-pressed={selected===p.name} aria-haspopup="menu" aria-keyshortcuts="Shift+F10" title={t('presets.tileHint')}
     onClick={()=>setSelected(p.name)} onContextMenu={e=>{e.preventDefault();openMenu(p.name,e.clientX,e.clientY);}}
     onKeyDown={e=>{if(e.key==='ContextMenu'||(e.shiftKey&&e.key==='F10')){e.preventDefault();const r=e.currentTarget.getBoundingClientRect();openMenu(p.name,r.left,r.bottom);}}}>
     <span className="wb-thumb" aria-hidden="true">{p.name.trim().slice(0,2).toUpperCase()}</span><span className="wb-tile-name">{p.name}</span><small>{t('presets.changed',{count:String(changedCount(overrides(p.settings,defaults)))})}</small>
    </button></li>)}</ul>}
  </>}
  {menu&&chosen&&createPortal(<div role="menu" aria-label={menu.name} className="preset-menu" style={{left:Math.max(0,Math.min(menu.x,window.innerWidth-240)),top:Math.max(0,Math.min(menu.y,window.innerHeight-140))}}
   onKeyDown={e=>{const items=[...e.currentTarget.querySelectorAll<HTMLButtonElement>('[role=menuitem]:not(:disabled)')];const i=items.indexOf(document.activeElement as HTMLButtonElement);
    if(e.key==='Escape'||e.key==='Tab'){e.preventDefault();closeMenu();}else if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();items[(i+(e.key==='ArrowDown'?1:items.length-1))%items.length]?.focus();}}}>
   <button ref={firstItem} type="button" role="menuitem" disabled={disabled||saving} onClick={()=>{setMenu(null);apply(chosen);}}>{t('presets.apply')}</button>
   <button type="button" role="menuitem" disabled={saving} onClick={()=>{setMenu(null);setEditing(chosen);}}>{t('presets.edit')}</button>
   <button type="button" role="menuitem" disabled={disabled||saving} onClick={()=>{setMenu(null);overwrite(chosen.name);}}>{t('presets.update')}</button>
  </div>,document.body)}
  {editing&&presets&&createPortal(<PresetEditor preset={editing} taken={presets.map(p=>p.name).filter(n=>n!==editing.name)} saving={saving}
   onCancel={()=>{const n=editing.name;setEditing(null);setTimeout(()=>focusTile(n));}}
   onSave={edited=>void persist(editing.name?presets.map(p=>p.name===editing.name?edited:p):[...presets,edited],edited.name).then(ok=>{if(ok&&live.current){setEditing(null);setTimeout(()=>focusTile(edited.name));}})}/>,document.body)}
  {error&&<p role="alert" className="error-message">{error}</p>}
  {status&&<p role="status">{status}</p>}
  {ask&&<ConfirmDialog message={t('presets.confirm',{name:ask.name})} onDone={accepted=>{const run=ask.run;setAsk(null);if(accepted)run();}}/>}
 </section>;
}

const show=(v:unknown)=>{const s=JSON.stringify(v)??'—';return s.length>80?`${s.slice(0,77)}…`:s;};
const parses=(s:string)=>{try{JSON.parse(s);return true;}catch{return false;}};
/** Edits one preset field by field against today's defaults: changed rows are marked, Reset drops the override, settings this version no longer has can be removed. */
function PresetEditor({preset,taken,saving,onCancel,onSave}:{preset:Preset;taken:string[];saving:boolean;onCancel:()=>void;onSave:(p:Preset)=>void}){
 const {t}=useI18n();const dialog=useRef<HTMLDialogElement>(null);const isNew=!preset.name;
 const [defaults]=useState(studioDefaults);
 const [name,setName]=useState(preset.name),[over,setOver]=useState(()=>overrides(preset.settings,defaults)),[all,setAll]=useState(false),[text,setText]=useState<Record<string,string>>({});
 useEffect(()=>{dialog.current?.showModal();},[]);
 const resolved=resolve(over,defaults);const invalid=checkStudioSettings(resolved).invalid;
 const setLeaf=(key:string,path:string[],value:unknown)=>setOver(o=>{const next={...o};const d=overrides({[key]:setAt(resolve(o,defaults)[key],path,value)},defaults)[key];if(d===undefined)delete next[key];else next[key]=d;return next;});
 const trimmed=name.trim();const nameOk=!!trimmed&&!taken.includes(trimmed);const badJson=Object.values(text).some(s=>!parses(s));
 const rows=[...new Set([...Object.keys(defaults),...Object.keys(over)])].sort().flatMap(key=>{const known=Object.hasOwn(defaults,key);
  return leaves(resolved[key]).map(({path,value})=>{const fallback=known?getAt(defaults[key],path):undefined;return {key,path,value,fallback,known,id:[key,...path].join('.'),label:[key.replace(/^studio\.run\./,''),...path].join(' › '),changed:!known||!equal(value,fallback)};});
 }).filter(r=>all||r.changed);
 const field=(r:typeof rows[number])=>{
  if(typeof r.value==='boolean')return <input type="checkbox" aria-label={r.label} checked={r.value} onChange={e=>setLeaf(r.key,r.path,e.target.checked)}/>;
  if(typeof r.value==='number')return <input type="number" aria-label={r.label} step="any" value={r.value} onChange={e=>{const n=e.target.valueAsNumber;if(Number.isFinite(n))setLeaf(r.key,r.path,n);}}/>;
  if(typeof r.value==='string')return r.value.length>60||r.value.includes('\n')?<textarea aria-label={r.label} rows={3} value={r.value} onChange={e=>setLeaf(r.key,r.path,e.target.value)}/>:<input aria-label={r.label} value={r.value} onChange={e=>setLeaf(r.key,r.path,e.target.value)}/>;
  // Lists, empty objects and null are edited as JSON; text that does not parse keeps the last valid value and blocks Save.
  const raw=text[r.id]??JSON.stringify(r.value);
  return <textarea aria-label={r.label} rows={2} aria-invalid={!parses(raw)} value={raw} onChange={e=>{const s=e.target.value;setText(m=>({...m,[r.id]:s}));if(parses(s))setLeaf(r.key,r.path,JSON.parse(s));}}/>;
 };
 const reset=(r:typeof rows[number])=>{setText(m=>{const next={...m};delete next[r.id];return next;});if(r.known)setLeaf(r.key,r.path,r.fallback);else setOver(o=>{const next={...o};delete next[r.key];return next;});};
 return <dialog ref={dialog} className="preset-editor" aria-label={t(isNew?'presets.newTitle':'presets.editTitle')} onCancel={e=>{e.preventDefault();if(!saving)onCancel();}}>
  <h2>{t(isNew?'presets.newTitle':'presets.editTitle')}</h2>
  <label className="preset-editor-name">{t('presets.name')}<input value={name} maxLength={100} aria-invalid={!nameOk} onChange={e=>setName(e.target.value)}/></label>
  <label className="preset-editor-all"><input type="checkbox" checked={all} onChange={e=>setAll(e.target.checked)}/>{t('presets.showAll')}</label>
  {!rows.length&&<p className="aio-help">{t('presets.noChanges')}</p>}
  <ul className="preset-rows">{rows.map(r=><li key={r.id} data-changed={r.changed} data-invalid={!r.known||invalid.includes(r.key)||undefined}>
   <span className="preset-row-label"><code>{r.label}</code>{r.changed&&<small className="preset-mark">{t('presets.changedMark')}</small>}</span>
   {r.known?field(r):<code>{show(r.value)}</code>}
   <span className="preset-row-meta"><small>{r.known?t('presets.default',{value:show(r.fallback)}):t('presets.unknown')}</small>
    {invalid.includes(r.key)&&<small role="alert">{t('presets.invalidValue')}</small>}
    {r.changed&&<button type="button" onClick={()=>reset(r)}>{t(r.known?'presets.reset':'presets.remove')}</button>}</span>
  </li>)}</ul>
  <div className="preset-editor-actions">
   <button type="button" disabled={saving||!nameOk||badJson||invalid.length>0} onClick={()=>onSave({name:trimmed,settings:over})}>{t(isNew?'presets.saveNew':'presets.saveEdit')}</button>
   <button type="button" disabled={saving} onClick={onCancel}>{t('presets.cancel')}</button>
  </div>
 </dialog>;
}
