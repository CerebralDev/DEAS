import { useEffect, useRef, type ReactNode } from 'react';
import { useI18n } from '../i18n/I18nProvider';
import './confirmDialog.css';

/** In-DOM replacement for window.confirm. On Linux the NativeVideo/libmpv preview is a native surface that always composites over WebKit, so JS dialogs appear "behind" the video; a real <dialog> also triggers NativeVideo's occlusion rule that hides background native surfaces. Renders open immediately; onDone(false) on Cancel/Escape. */
export function ConfirmDialog({message,onDone}:{message:ReactNode;onDone:(accepted:boolean)=>void}) {
 const {t}=useI18n();const dialog=useRef<HTMLDialogElement>(null);
 useEffect(()=>{dialog.current?.showModal();},[]);
 return <dialog ref={dialog} className="confirm-dialog" onCancel={e=>{e.preventDefault();onDone(false);}}>
  <p>{message}</p>
  <div className="confirm-actions"><button type="button" autoFocus data-confirm-dialog onClick={()=>onDone(true)}>{t('app.confirm')}</button><button type="button" onClick={()=>onDone(false)}>{t('app.cancel')}</button></div>
 </dialog>;
}
