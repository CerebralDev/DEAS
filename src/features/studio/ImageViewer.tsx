import { useEffect, useRef, useState } from 'react';
import { useI18n } from '../../i18n/I18nProvider';
import { StarMediaButton } from '../media/StarMediaButton';
import type { MediaItem } from '../../lib/media';
import { zoomAt, type View } from './imageEditor';

/** Full-window picture view: wheel zooms at the cursor, drag pans, edge-fit/100 %, backdrop click, Escape or ✕ closes; Left/Right step through `position` siblings. Read-only. */
export function ImageViewer({src,alt,onClose,item,onMoved,onPrev,onNext,position}:{src:string;alt:string;onClose:()=>void;item?:MediaItem;onMoved?:(item:MediaItem)=>void;onPrev?:()=>void;onNext?:()=>void;position?:{index:number;total:number}}){
 const {t}=useI18n();
 const [relocated,setRelocated]=useState<MediaItem|null>(null);
 // A different picture refits; a star relocation of the same picture keeps the current zoom.
 useEffect(()=>{if(relocated?.src!==src){setSize(null);setView(null);}setRelocated(null);},[src]); // eslint-disable-line react-hooks/exhaustive-deps
 const box=useRef<HTMLDivElement>(null);const close=useRef(onClose);close.current=onClose;const step=useRef({onPrev,onNext});step.current={onPrev,onNext};
 const [size,setSize]=useState<{width:number;height:number}|null>(null);
 const [view,setView]=useState<View|null>(null);
 const dragged=useRef(false),stopPan=useRef<(()=>void)|null>(null);
 const zoomLimit=useRef(16);
 const centre=()=>{const r=box.current?.getBoundingClientRect();return {x:(r?.width??0)/2,y:(r?.height??0)/2};};
 const fit=()=>{const r=box.current?.getBoundingClientRect();if(!r||!size||!r.width||!r.height)return;const scale=Math.min(r.width/size.width,r.height/size.height);zoomLimit.current=Math.max(16,scale*16);setView({scale,x:(r.width-size.width*scale)/2,y:(r.height-size.height*scale)/2});};
 useEffect(fit,[size]); // eslint-disable-line react-hooks/exhaustive-deps
 useEffect(()=>{
  const el=box.current;if(!el)return;
  const wheel=(e:WheelEvent)=>{e.preventDefault();const r=el.getBoundingClientRect();setView(v=>v&&zoomAt(v,{x:e.clientX-r.left,y:e.clientY-r.top},v.scale*Math.exp(-e.deltaY*.001),zoomLimit.current));};
  const key=(e:KeyboardEvent)=>{if(e.key==='Escape')close.current();
   else if((e.key==='ArrowLeft'||e.key==='ArrowRight')&&!e.altKey&&!e.ctrlKey&&!e.metaKey&&!e.shiftKey&&!(e.target instanceof Element&&e.target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"])'))){const go=e.key==='ArrowLeft'?step.current.onPrev:step.current.onNext;if(go){e.preventDefault();go();}}};
  el.addEventListener('wheel',wheel,{passive:false});window.addEventListener('keydown',key);
  return()=>{el.removeEventListener('wheel',wheel);window.removeEventListener('keydown',key);stopPan.current?.();};
 },[]);
 const pan=(e:React.PointerEvent)=>{
  if(!view||e.button!==0||e.target!==e.currentTarget)return;e.preventDefault();stopPan.current?.();dragged.current=false;
  const start={x:e.clientX,y:e.clientY},from=view;
  const move=(ev:PointerEvent)=>{const dx=ev.clientX-start.x,dy=ev.clientY-start.y;if(Math.abs(dx)+Math.abs(dy)>3)dragged.current=true;setView({...from,x:from.x+dx,y:from.y+dy});};
  const stop=()=>{window.removeEventListener('pointermove',move);window.removeEventListener('pointerup',stop);window.removeEventListener('pointercancel',stop);stopPan.current=null;};stopPan.current=stop;
  window.addEventListener('pointermove',move);window.addEventListener('pointerup',stop);window.addEventListener('pointercancel',stop);
 };
 const zoom=(scale:(s:number)=>number)=>setView(v=>v&&zoomAt(v,centre(),scale(v.scale),zoomLimit.current));
 return <div className="wb-viewer" role="dialog" aria-modal="true" aria-label={alt} onClick={e=>{
   const target=e.target;
   if((target===e.currentTarget||target===box.current||(target instanceof Element&&target.classList.contains('wb-viewer-tools')))&&!dragged.current)onClose();
   dragged.current=false;
  }}>
  <div className="wb-viewer-tools">
   <StarMediaButton item={relocated??item??{name:alt,src,media:'image'}} onMoved={next=>{setRelocated(next);onMoved?.(next);}}/>
   {position&&<button type="button" data-viewer-prev aria-label={t('studio.run.viewer.prev')} title={t('studio.run.viewer.prev')} disabled={!onPrev} onClick={onPrev}>‹</button>}
   <span className="wb-viewer-name">{position&&<>{position.index+1} / {position.total} · </>}{alt}{view&&<> · {Math.round(view.scale*100)} %</>}</span>
   {position&&<button type="button" data-viewer-next aria-label={t('studio.run.viewer.next')} title={t('studio.run.viewer.next')} disabled={!onNext} onClick={onNext}>›</button>}
   <button type="button" aria-label={t('studio.editor.zoomOut')} onClick={()=>zoom(s=>s/1.25)}>−</button>
   <button type="button" onClick={fit}>{t('studio.editor.fit')}</button>
   <button type="button" onClick={()=>zoom(()=>1)}>{t('studio.editor.actual')}</button>
   <button type="button" aria-label={t('studio.editor.zoomIn')} onClick={()=>zoom(s=>s*1.25)}>+</button>
   <button type="button" aria-label={t('studio.run.close')} autoFocus onClick={onClose}>✕</button>
  </div>
  <div ref={box} className="wb-viewer-box">
   <img src={relocated?.src??src} alt={alt} draggable={false} onPointerDown={pan} onLoad={e=>setSize({width:e.currentTarget.naturalWidth,height:e.currentTarget.naturalHeight})}
    style={view?{width:size!.width,height:size!.height,transform:`translate(${view.x}px, ${view.y}px) scale(${view.scale})`}:{visibility:'hidden'}}/>
  </div>
 </div>;
}
