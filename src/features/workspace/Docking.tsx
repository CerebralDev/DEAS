import { useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { useI18n } from '../../i18n/I18nProvider';
import './workspace.css';

/** The portal container never changes: moving its DOM host preserves React state,
 * pending requests, focus and native media. There is only one copy of each pane. */
export function RetainedPane({ target, children }: { target: HTMLElement | null; children: ReactNode }) {
 const [host] = useState(() => Object.assign(document.createElement('div'), { className: 'workspace-retained' }));
 useLayoutEffect(() => { if (target) target.appendChild(host); else host.remove(); }, [host, target]);
 useLayoutEffect(() => () => host.remove(), [host]);
 return createPortal(children, host);
}

export function DockResize({ vertical, reverse = false, value, onChange }: { vertical: boolean; reverse?: boolean; value: number; onChange: (n:number)=>void }) {
 const {t}=useI18n(); const start=useRef<{point:number;value:number}|null>(null);
 const clamp=(n:number)=>Math.max(180,Math.min(vertical?520:720,n));
 return <div className={`workspace-resize ${vertical?'horizontal':'vertical'}`} role="separator" tabIndex={0}
  aria-label={t('workspace.resize')} aria-orientation={vertical?'horizontal':'vertical'} aria-valuenow={value} aria-valuemin={180} aria-valuemax={vertical?520:720} title={t('workspace.resizeHelp')}
  onKeyDown={e=>{if(['ArrowUp','ArrowDown','ArrowLeft','ArrowRight','Home','End'].includes(e.key)){e.preventDefault();onChange(e.key==='Home'?180:e.key==='End'?(vertical?520:720):clamp(value+(['ArrowUp','ArrowLeft'].includes(e.key)?-20:20)));}}}
  onPointerDown={e=>{e.currentTarget.setPointerCapture(e.pointerId);start.current={point:vertical?e.clientY:e.clientX,value};}}
  onPointerMove={e=>{if(start.current)onChange(clamp(start.current.value+(start.current.point-(vertical?e.clientY:e.clientX))*(reverse?-1:1)));}}
  onPointerUp={()=>{start.current=null;}} onPointerCancel={()=>{start.current=null;}}/>;
}

/** Focusable tooltips complement native hover titles. Essential help remains in labels. */
export function WorkspaceTooltips() {
 const [tip,setTip]=useState<{text:string;x:number;y:number}|null>(null);
 useLayoutEffect(()=>{
  let owner:HTMLElement|null=null;
  const clear=()=>{owner?.removeAttribute('aria-describedby');owner=null;setTip(null);};
  const show=(e:Event)=>{const el=e.target instanceof Element?e.target.closest<HTMLElement>('[title]'):null;if(!el?.title){clear();return;} const r=el.getBoundingClientRect();owner=el;el.setAttribute('aria-describedby','workspace-tooltip');setTip({text:el.title,x:Math.max(8,Math.min(r.left,window.innerWidth-310)),y:Math.max(8,Math.min(r.bottom+5,window.innerHeight-100))});};
  const key=(e:KeyboardEvent)=>{if(e.key==='Escape')clear();};
  document.addEventListener('focusin',show);document.addEventListener('focusout',clear);document.addEventListener('keydown',key);
  return()=>{clear();document.removeEventListener('focusin',show);document.removeEventListener('focusout',clear);document.removeEventListener('keydown',key);};
 },[]);
 return tip?createPortal(<div id="workspace-tooltip" role="tooltip" className="workspace-tooltip" style={{left:tip.x,top:tip.y}}>{tip.text}</div>,document.body):null;
}
