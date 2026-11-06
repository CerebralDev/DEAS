import { useEffect, useRef, useState, type Dispatch, type ReactNode, type SetStateAction } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { createPortal } from 'react-dom';
import { useI18n } from '../../i18n/I18nProvider';
import { writeModelFamily, type LocalModel } from '../../lib/models';
import families from '../../families/builtin.json' with { type: 'json' };
import './modelMetadata.css';
const changedEvent='deas:model-family-changed';
/** Patch exact paths only; no mount-time scan or unrelated metadata requests. */
export function useModelMetadataUpdates(setModels:Dispatch<SetStateAction<LocalModel[]|null>>) {
 useEffect(()=>{const update=(event:Event)=>{const model=(event as CustomEvent<LocalModel>).detail;setModels(old=>old?.map(m=>m.path===model.path?model:m)??old);};window.addEventListener(changedEvent,update);return()=>window.removeEventListener(changedEvent,update);},[setModels]);
}
export function ModelDeclaredBadge({model}:{model:LocalModel}) {const {t}=useI18n();return model.familyOverride?<small className="model-declared">{t('modelMeta.declared')}: {families.find(f=>f.id===model.familyOverride)?.name??model.familyOverride}</small>:null;}
export function ModelMetadataTarget({model,children}:{model:LocalModel;children:ReactNode}) {
 const {t}=useI18n();const [menu,setMenu]=useState<{x:number;y:number}|null>(null);const [edit,setEdit]=useState(false);const target=useRef<HTMLSpanElement>(null);const item=useRef<HTMLButtonElement>(null);
 useEffect(()=>{if(!menu)return;item.current?.focus();const dismiss=(event:PointerEvent)=>{if(!target.current?.contains(event.target as Node)&&!item.current?.contains(event.target as Node))setMenu(null);};document.addEventListener('pointerdown',dismiss);return()=>document.removeEventListener('pointerdown',dismiss);},[menu]);
 const restore=()=>target.current?.querySelector('button')?.focus();
 return <span className="model-meta-target" ref={target} onContextMenu={e=>{if(edit)return;e.preventDefault();setMenu({x:e.clientX,y:e.clientY});}} onKeyDown={e=>{if(edit)return;if(e.key==='ContextMenu'||(e.shiftKey&&e.key==='F10')){e.preventDefault();const rect=target.current?.querySelector('button')?.getBoundingClientRect();setMenu({x:rect?.left??0,y:rect?.top??0});}else if(e.key==='Escape'){setMenu(null);restore();}else if(menu&&(e.key==='ArrowDown'||e.key==='ArrowUp')){e.preventDefault();item.current?.focus();}else if(menu&&e.key==='Tab')setMenu(null);}}>
 {children}
 {menu&&createPortal(<span role="menu" aria-label={t('modelMeta.title')} className="model-meta-menu" style={{left:Math.max(0,Math.min(menu.x,window.innerWidth-240)),top:Math.max(0,Math.min(menu.y,window.innerHeight-60))}}><button type="button" role="menuitem" ref={item} onClick={()=>{setMenu(null);setEdit(true);}}>{t('modelMeta.title')}</button></span>,document.body)}
 {edit&&createPortal(<ModelMetadataEditor key={model.path} model={model} onClose={()=>{setEdit(false);restore();}}/>,document.body)}
 </span>;
}
export function ModelMetadataEditor({model,onClose}:{model:LocalModel;onClose:()=>void}) {
 const {t}=useI18n();const dialog=useRef<HTMLDialogElement>(null);const live=useRef(false);const lock=useRef(false);const [busy,setBusy]=useState(false);const [error,setError]=useState(false);const [family,setFamily]=useState(model.familyOverride??model.detectedFamily??'');
 const supported=['checkpoints','diffusion_models','unet','loras'].includes(model.category);const native=isTauri();
 useEffect(()=>{live.current=true;dialog.current?.showModal();return()=>{live.current=false;};},[]);
 const label=(id?:string)=>families.find(f=>f.id===id)?.name??id??t('modelMeta.unknown');
 const save=async(value:string|null)=>{if(lock.current||!supported||!native)return;lock.current=true;setBusy(true);setError(false);try{const updated=await writeModelFamily(model.path,value);if(live.current&&updated.path===model.path){window.dispatchEvent(new CustomEvent(changedEvent,{detail:updated}));onClose();}}catch{if(live.current)setError(true);}finally{lock.current=false;if(live.current)setBusy(false);}};
 return <dialog ref={dialog} className="model-meta-dialog" aria-label={t('modelMeta.title')} onCancel={e=>{e.preventDefault();if(!busy)onClose();}} onContextMenu={e=>e.stopPropagation()}>
 <h2>{t('modelMeta.title')}</h2><dl><dt>{t('modelMeta.filename')}</dt><dd>{model.name}</dd><dt>{t('modelMeta.path')}</dt><dd>{model.path}</dd><dt>{t('modelMeta.detected')}</dt><dd>{label(model.detectedFamily)}</dd>{model.familyEvidence&&<><dt>{t('modelMeta.evidence')}</dt><dd data-model-family-evidence={model.familyEvidence}>{t(`modelMeta.evidence.${model.familyEvidence}`)}</dd></>}<dt>{t('modelMeta.declared')}</dt><dd>{model.familyOverride?label(model.familyOverride):t('modelMeta.none')}</dd></dl>
 {model.metadataError&&<p role="alert">{t('modelMeta.invalid')}</p>}
 {!supported&&<p role="status">{t('modelMeta.unsupported')}</p>}
 <label>{t('modelMeta.family')}<select aria-label={t('modelMeta.family')} value={family} disabled={busy||!supported||!native} onChange={e=>setFamily(e.target.value)}><option value="">{t('modelMeta.unknown')}</option>{families.filter(f=>['checkpoint','diffusion'].includes(f.loader)&&(model.category!=='loras'||f.lora.length>0)).map(f=><option key={f.id} value={f.id}>{f.name}</option>)}</select></label>
 <p>{t('modelMeta.help')}</p>{error&&<p role="alert">{t('modelMeta.error')}</p>}
 <div className="model-meta-actions"><button type="button" disabled={busy||!supported||!native||!family} onClick={()=>void save(family)}>{t('modelMeta.save')}</button><button type="button" disabled={busy||!supported||!native} onClick={()=>void save(null)}>{t('modelMeta.reset')}</button><button type="button" disabled={busy} onClick={onClose}>{t('modelMeta.cancel')}</button></div>
 </dialog>;
}
