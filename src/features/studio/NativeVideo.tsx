import { useEffect, useImperativeHandle, useRef, useState, type RefObject, type CSSProperties } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { useI18n } from '../../i18n/I18nProvider';
import './nativeVideo.css';

export const nativeVideo = () => isTauri() && /Linux/.test(navigator.userAgent);
export type VideoSource = {previewJobId:string;previewRevision:number} | {path:string} | {baseUrl:string;filename:string;subfolder:string;type:string};
export type NativeVideoStatus = {position:number;duration:number;width:number;height:number;paused:boolean;muted:boolean;volume:number;error?:string};
export type NativeVideoControl = {seek:(seconds:number)=>void;pause:()=>void;play:()=>void;playCrop:(start:number,end:number)=>void};
const clock=(seconds:number)=>{const whole=Math.max(0,Math.floor(seconds));return `${Math.floor(whole/60)}:${String(whole%60).padStart(2,'0')}`;};
const initial:NativeVideoStatus={position:0,duration:0,width:0,height:0,paused:true,muted:false,volume:100};
export function NativeVideo({source,label,active=true,autoPlay=false,loop=false,onSize,onStatus,controlRef,muted,audioOnly=false}:{source:VideoSource;label:string;active?:boolean;autoPlay?:boolean;loop?:boolean;onSize?:(w:number,h:number)=>void;onStatus?:(status:NativeVideoStatus)=>void;controlRef?:RefObject<NativeVideoControl|null>;muted?:boolean;audioOnly?:boolean}) {
 const {t}=useI18n();
 const surface=useRef<HTMLDivElement>(null),size=useRef(onSize);size.current=onSize;
 const report=useRef(onStatus);report.current=onStatus;
 const modes=useRef({muted,audioOnly});modes.current={muted,audioOnly};
 const owner=useRef('');
 const [id,setId]=useState(''),[error,setError]=useState(''),[status,setStatus]=useState<NativeVideoStatus>(initial);
 const sourceKey=JSON.stringify(source);
 useEffect(()=>{
  setId('');setError('');setStatus(initial);
  if(!active)return;
  const token=crypto.randomUUID();owner.current=token;
  let disposed=false,ready=false,busy=false,last='',lastSize='';
  const close=()=>invoke('native_video_update',{id:token,action:{kind:'close'}}).catch(()=>{});
  const update=async()=>{
   if(!ready||disposed||busy||!surface.current)return;
   busy=true;
   try {
    const el=modes.current.audioOnly?surface.current.parentElement!:surface.current,r=el.getBoundingClientRect();
    // shortcut: hide clipped/occluded surfaces rather than paint native pixels over web controls.
    const visible=r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth&&r.bottom<=innerHeight&&
     !document.hidden&&Array.from(document.querySelectorAll('dialog[open], [aria-modal="true"]')).every(modal=>modal.contains(el))&&
     [[r.left+1,r.top+1],[r.right-1,r.top+1],[r.left+1,r.bottom-1],[r.right-1,r.bottom-1],[r.left+r.width/2,r.top+r.height/2]].every(([x,y])=>el.contains(document.elementFromPoint(x,y)));
    const bounds={x:Math.max(0,Math.round(r.left)),y:Math.max(0,Math.round(r.top)),width:Math.round(r.width),height:Math.round(r.height),visible,...(modes.current.audioOnly?{audioOnly:true}:{})};
    const key=JSON.stringify(bounds);
    if(key!==last){await invoke('native_video_update',{id:token,action:{kind:'bounds',...bounds}});last=key;}
    if(disposed)return;
    const next=await invoke<NativeVideoStatus>('native_video_update',{id:token,action:{kind:'status'}});
    if(!disposed){
     setStatus(next);if(next.error)setError(next.error);
     const dimensions=`${next.width}/${next.height}`;
     if(next.width>0&&next.height>0&&dimensions!==lastSize){lastSize=dimensions;size.current?.(next.width,next.height);}
    }
   } catch(e){if(!disposed){setError(String(e));ready=false;void close();}} finally {busy=false;}
  };
  void invoke('native_video_open',{id:token,source:JSON.parse(sourceKey),autoPlay,repeat:loop,muted:modes.current.muted??false}).then(()=>{
   if(disposed){void close();return;}
   ready=true;setId(token);void update();
  }).catch(e=>{if(!disposed)setError(String(e));});
  const timer=window.setInterval(()=>void update(),150);
  const refresh=()=>void update();
  window.addEventListener('resize',refresh);document.addEventListener('visibilitychange',refresh);
  const observer=new MutationObserver(refresh);observer.observe(document.body,{attributes:true,attributeFilter:["hidden","open","aria-modal","class","style"],childList:true,subtree:true});
  return ()=>{disposed=true;owner.current='';clearInterval(timer);observer.disconnect();window.removeEventListener('resize',refresh);document.removeEventListener('visibilitychange',refresh);void close();};
 },[sourceKey,active,autoPlay,loop]);
 const control=(action:object)=>{const token=id;if(token&&active)void invoke('native_video_update',{id:token,action}).catch(e=>{if(owner.current===token)setError(String(e));});};
 useImperativeHandle(controlRef,()=>({seek:seconds=>control({kind:'seek',seconds}),pause:()=>control({kind:'pause',paused:true}),play:()=>control({kind:'pause',paused:false}),playCrop:(start,end)=>control({kind:'crop',start,end})}));
 useEffect(()=>{if(muted!==undefined)control({kind:'mute',muted});},[id,active,muted]);
 useEffect(()=>{report.current?.({...status,muted:muted===true||status.muted,...(error?{error}:{})});},[status,error,muted]);
 const enabled=!!id&&active&&!error;
 const position=Math.max(0,Math.min(status.position,status.duration)),volume=Math.max(0,Math.min(status.volume,100));
 const time=`${clock(position)} / ${clock(status.duration)}`;
 const fill=(percent:number)=>({'--progress':`${percent}%`} as CSSProperties);
 const silent=muted===true||status.muted;
 return <div className="native-video" data-native-video data-audio-only={audioOnly||undefined} role="group" aria-label={label}>
  <div ref={surface} className="native-video-surface" style={{display:audioOnly?'none':undefined,aspectRatio:status.width&&status.height?`${status.width}/${status.height}`:'16/9'}} onClick={()=>{if(enabled)control({kind:'pause',paused:!status.paused});}}/>
  {error&&<p role="alert" className="error-message">{t('studio.video.error')} {error}</p>}
  <div className="native-video-controls">
   <button type="button" disabled={!enabled} title={t(status.paused?'studio.video.play':'studio.video.pause')} aria-label={t(status.paused?'studio.video.play':'studio.video.pause')} onClick={()=>control({kind:'pause',paused:!status.paused})}><svg viewBox="0 0 24 24" aria-hidden="true">{status.paused?<path d="m8 5 11 7-11 7Z" fill="currentColor" stroke="none"/>:<path d="M8 5v14M16 5v14" strokeWidth="4"/>}</svg></button>
   <input className="native-video-seek" type="range" aria-label={t('studio.video.seek')} aria-valuetext={time} min={0} max={status.duration||0} step="any" value={position} style={fill(status.duration?position/status.duration*100:0)} disabled={!enabled||!status.duration} onChange={e=>control({kind:'seek',seconds:Number(e.target.value)})}/>
   <span className="native-video-time">{time}</span>
   <button type="button" disabled={!enabled||muted===true} title={t(silent?'studio.video.unmute':'studio.video.mute')} aria-label={t(silent?'studio.video.unmute':'studio.video.mute')} aria-pressed={silent} onClick={()=>{if(muted!==true)control({kind:'mute',muted:!status.muted});}}><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M11 5 6 9H3v6h3l5 4Z"/>{silent?<path d="m16 9 5 6m0-6-5 6"/>:<path d="M15 8c3 2 3 6 0 8m3-11c5 4 5 10 0 14"/>}</svg></button>
   <input className="native-video-volume" type="range" aria-label={t('studio.video.volume')} aria-valuetext={`${Math.round(volume)}%`} min={0} max={100} value={volume} style={fill(volume)} disabled={!enabled} onChange={e=>control({kind:'volume',volume:Number(e.target.value)})}/>
   <span className="native-video-percent">{Math.round(volume)}%</span>
  </div>
 </div>;
}
