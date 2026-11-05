import {useI18n} from '../../i18n/I18nProvider';
import { useEffect,useRef,useState } from 'react';
import { createPortal } from 'react-dom';


import { CHASE_MS,moveMouse,rapidKnocks,type Point } from './mouseMotion';
import './mouse.css';

type Phase='idle'|'chasing'|'caught'|'returning';
/** Visual Easter egg only: never captures, moves, or disables the user's actual pointer. */
export function MouseMischief({mouse,cheese}:{mouse:string;cheese:string}){
 const {t}=useI18n();
 const [enabled,setEnabled]=useState(document.documentElement.dataset.themeMouse==='true');
 const [phase,setPhase]=useState<Phase>('idle');
 const [message,setMessage]=useState('theme.ui.mouseIdle');
 const door=useRef<HTMLButtonElement>(null),actor=useRef<HTMLDivElement>(null);
 const phaseRef=useRef<Phase>('idle'),pointer=useRef<Point|null>(null),position=useRef<Point>({x:0,y:0});
 const start=useRef(()=>{}),knocks=useRef<number[]>([]),held=useRef(false);
 useEffect(()=>{
  const sync=()=>setEnabled(document.documentElement.dataset.themeMouse==='true');
  const observer=new MutationObserver(sync);observer.observe(document.documentElement,{attributes:true,attributeFilter:['data-theme-mouse']});sync();
  return()=>observer.disconnect();
 },[]);
 useEffect(()=>{
  if(!enabled)return;
  let timer=0,frame=0,last=0,began=0,retreatAt=0;
  const root=document.documentElement;
  const home=():Point=>{const r=door.current?.getBoundingClientRect();return r?.width?{x:r.left+r.width/2,y:r.top+r.height/2}:{x:innerWidth-35,y:35};};
  const change=(next:Phase)=>{phaseRef.current=next;setPhase(next);};
  const cleanCheese=()=>delete root.dataset.cheeseStolen;
  const schedule=(rest=false)=>{clearTimeout(timer);timer=window.setTimeout(()=>{
   if(!document.hidden&&document.hasFocus()&&!matchMedia('(prefers-reduced-motion: reduce)').matches&&pointer.current&&!held.current&&!document.querySelector('dialog[open]')&&!document.activeElement?.matches('input,textarea,[contenteditable=true]'))start.current();
   else schedule(rest);
  },rest?90_000+Math.random()*90_000:45_000+Math.random()*60_000);};
  const idle=()=>{cancelAnimationFrame(frame);cleanCheese();change('idle');schedule(true);};
  const retreat=(caught:boolean,now:number)=>{
   change(caught?'caught':'returning');retreatAt=now;
   setMessage(caught?'theme.ui.mouseCaught':'theme.ui.mouseEscaped');
   if(caught)root.dataset.cheeseStolen='true';
  };
  const tick=(now:number)=>{
   const dt=Math.min(.05,Math.max(0,(now-last)/1000));last=now;
   if(document.hidden){idle();return;}
   if(phaseRef.current==='chasing'){
    const target=pointer.current;
    if(now-began>=CHASE_MS)retreat(false,now);
    else if(target){position.current=moveMouse(position.current,target,dt);if(now-began>900&&Math.hypot(position.current.x-target.x,position.current.y-target.y)<22)retreat(true,now);}
   }else if(phaseRef.current==='caught'||phaseRef.current==='returning'){
    const target=home();position.current=moveMouse(position.current,target,dt,700);
    if(now-retreatAt>3000||(now-retreatAt>1000&&Math.hypot(position.current.x-target.x,position.current.y-target.y)<12)){idle();return;}
   }else return;
   if(actor.current){actor.current.style.left=`${Math.max(27,Math.min(innerWidth-27,position.current.x))}px`;actor.current.style.top=`${Math.max(27,Math.min(innerHeight-27,position.current.y))}px`;actor.current.style.setProperty('--mouse-facing',String(((phaseRef.current==='chasing'?pointer.current:home())?.x??0)<position.current.x?1:-1));}
   frame=requestAnimationFrame(tick);
  };
  start.current=()=>{
   if(phaseRef.current!=='idle'||document.hidden)return;
   clearTimeout(timer);knocks.current=[];cleanCheese();position.current=home();began=performance.now();last=began;
   change('chasing');setMessage('theme.ui.mouseChasing');frame=requestAnimationFrame(tick);
  };
  const move=(e:PointerEvent)=>{if(e.pointerType!=='touch')pointer.current={x:e.clientX,y:e.clientY};};
  const escape=(e:KeyboardEvent)=>{if(e.key==='Escape'&&phaseRef.current==='chasing'){retreat(false,performance.now());setMessage('theme.ui.mouseSafe');}};
  const down=()=>{held.current=true;};const up=()=>{held.current=false;};
  const visibility=()=>{if(document.hidden){held.current=false;idle();}};
  window.addEventListener('pointermove',move,{passive:true});window.addEventListener('pointerdown',down,{passive:true});window.addEventListener('pointerup',up,{passive:true});window.addEventListener('pointercancel',up,{passive:true});window.addEventListener('keydown',escape);document.addEventListener('visibilitychange',visibility);schedule();
  return()=>{clearTimeout(timer);cancelAnimationFrame(frame);cleanCheese();phaseRef.current='idle';setPhase('idle');start.current=()=>{};window.removeEventListener('pointermove',move);window.removeEventListener('pointerdown',down);window.removeEventListener('pointerup',up);window.removeEventListener('pointercancel',up);window.removeEventListener('keydown',escape);document.removeEventListener('visibilitychange',visibility);};
 },[enabled]);
 if(!enabled)return null;
 const active=phase!=='idle';
 return <><span className="mouse-house" data-mouse-house data-phase={phase}>
  <button ref={door} type="button" data-theme-mouse-door className={`mouse-door${active?' is-open':''}`} aria-label={t('theme.ui.mouseDoorKnockFourTimesToInviteThe')} title={t('theme.ui.knockFourTimesQuicklyDodgeTheMouseFor')} onClick={()=>{if(phaseRef.current!=='idle')return;knocks.current=rapidKnocks(knocks.current,performance.now());if(knocks.current.length>=4)start.current();else setMessage('theme.ui.mouseIdle');}}><span className="mouse-door-leaf"><i/></span></button>
  <span className="mouse-house-message" role="status">{t(message)}</span>
 </span>{active&&createPortal(<div ref={actor} className={`cheese-mouse ${phase}`} data-cheese-mouse aria-hidden="true" style={{left:position.current.x,top:position.current.y}}><img className="cheese-mouse-body" src={mouse} alt=""/>{phase==='caught'&&<img className="mouse-loot" src={cheese} alt=""/>}</div>,document.body)}</>;
}
