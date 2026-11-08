import { useEffect, useId, useRef, type ReactNode } from 'react';
import { useI18n } from '../../i18n/I18nProvider';
import './continuity-overlay.css';
import './references-overlay.css';

export function ReferenceEditorDialog({name,heading,onClose,restoreFocus,children}:{name:string;/** Replaces the default "Edit {name}" heading. */heading?:string;onClose:()=>void;restoreFocus:()=>HTMLElement|null;children:ReactNode}){
 const restore=useRef(restoreFocus);restore.current=restoreFocus;
 const {t}=useI18n(),title=useId(),dialog=useRef<HTMLDialogElement>(null),close=useRef<HTMLButtonElement>(null);
 useEffect(()=>{
  const opener=document.activeElement as HTMLElement|null,el=dialog.current!;
  el.showModal();close.current?.focus();
  return()=>{if(el.open)el.close();const target=restore.current()??opener;if(target?.isConnected)target.focus();};
 },[]);
 return <dialog ref={dialog} className="continuity-overlay reference-editor-dialog" aria-labelledby={title} onCancel={e=>{e.preventDefault();e.stopPropagation();onClose();}}>
  <header className="continuity-overlay-header"><h2 id={title}>{heading??t('studio.run.video.editReference',{name})}</h2><button ref={close} type="button" onClick={onClose}>{t('studio.close')}</button></header>
  <div className="reference-editor-body">{children}</div>
 </dialog>;
}
