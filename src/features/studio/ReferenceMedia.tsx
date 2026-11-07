import { useEffect, useRef, useState, type SyntheticEvent } from 'react';
import { useI18n } from '../../i18n/I18nProvider';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { badTrim, type VideoRef } from './ReferencesPanel';
import { NativeVideo, nativeVideo, type NativeVideoControl } from './NativeVideo';
import './reference-media.css';

type Props={reference:VideoRef;onChange:(x:Partial<VideoRef>)=>void};
/** A new source or stream gets a fresh player; saved trim values remain untouched. */
export function ReferenceMedia(props:Props){
 const r=props.reference;
 return <ReferencePlayer key={`${r.item.path}:${r.item.src}:${r.item.media}:${r.mediaMode}`} {...props}/>;
}
function ReferencePlayer({reference:r,onChange}:Props){
 const {t}=useI18n();const media=useRef<HTMLMediaElement|null>(null),nativeControl=useRef<NativeVideoControl|null>(null);
 const native=nativeVideo()&&r.item.media!=='image'&&!!r.item.path;
 const [duration,setDuration]=useState<number|null>(null),[position,setPosition]=useState(0),[playing,setPlaying]=useState(false),[error,setError]=useState('');
 const crop=useRef(false),timer=useRef<ReturnType<typeof setTimeout>|undefined>(undefined),request=useRef(0);
 const start=r.trimStart??0,end=r.trimEnd??duration??0;
 const known=duration!==null&&Number.isFinite(duration)&&duration>0;
 const valid=known&&Number.isFinite(start)&&Number.isFinite(end)&&start>=0&&end>start&&end<=duration&&start<=3600&&end<=3600;
 const bounds=useRef({start,end,valid});bounds.current={start,end,valid};
 const clearTimer=()=>{clearTimeout(timer.current);timer.current=undefined;};
 const stop=(atEnd=false)=>{
  clearTimer();crop.current=false;request.current++;setPlaying(false);
  if(native){nativeControl.current?.pause();return;}
  const el=media.current;if(!el)return;
  el.pause();if(atEnd){el.currentTime=bounds.current.end;setPosition(el.currentTime);}
 };
 const logged=useRef(false);
 const reportError=(key:string,detail?:string)=>{
  setError(`${t(`studio.run.video.preview.${key}`)}${detail?` ${detail}`:''}`);
  if(isTauri()&&!logged.current){logged.current=true;void invoke('native_logs_reference_media_error').catch(()=>{});}
 };
 const fail=(key:string,detail?:string)=>{stop();reportError(key,detail);};
 const arm=()=>{
  clearTimer();const el=media.current;if(!el||!crop.current)return;
  const b=bounds.current;
  if(!b.valid){stop();return;}
  if(el.currentTime>=b.end){stop(true);return;}
  // Native timeupdate can be hundreds of milliseconds apart; also schedule the crop boundary.
  timer.current=setTimeout(()=>stop(true),Math.max(0,(b.end-el.currentTime)/el.playbackRate*1000));
 };
 useEffect(()=>{
  const el=media.current;
  return()=>{request.current++;crop.current=false;clearTimeout(timer.current);el?.pause();};
 },[]);
 useEffect(()=>{if(crop.current)stop();},[r.trimStart,r.trimEnd,duration]);
 const toggle=async()=>{
  if(crop.current){stop();return;}
  if(native){if(valid){setError('');crop.current=true;setPlaying(true);nativeControl.current?.playCrop(start,end);}return;}
  const el=media.current;if(!el||!valid)return;
  setError('');crop.current=true;setPlaying(true);const id=++request.current;
  try{el.currentTime=start;setPosition(start);await el.play();if(request.current===id&&crop.current)arm();else if(!crop.current)el.pause();}
  catch{if(request.current===id)fail('playError');}
 };
 const progress=()=>{
  const el=media.current;if(!el)return;
  if(crop.current){
   if(el.currentTime>=bounds.current.end){stop(true);return;}
   if(el.currentTime<bounds.current.start)el.currentTime=bounds.current.start;
   arm();
  }
  setPosition(el.currentTime);
 };
 const metadata=(e:SyntheticEvent<HTMLMediaElement>)=>{
  const n=e.currentTarget.duration;const seconds=Number.isFinite(n)&&n>0?n:undefined;setDuration(seconds??null);if(r.sourceDuration!==seconds)onChange({sourceDuration:seconds});
 };
 const edit=(key:'trimStart'|'trimEnd',value:string)=>{
  if(value===''){onChange({[key]:undefined});return;}
  const n=Number(value);if(Number.isFinite(n))onChange({[key]:Math.max(0,Math.min(known?Math.min(duration,3600):3600,n))});
 };
 const events={controls:true,preload:'metadata',onLoadedMetadata:metadata,onDurationChange:metadata,onTimeUpdate:progress,onSeeking:progress,onRateChange:arm,onPlaying:arm,onWaiting:clearTimer,onPause:()=>{clearTimer();crop.current=false;request.current++;setPlaying(false);},onEnded:()=>stop(),onError:()=>fail('loadError')};
 const image=r.item.media==='image',audio=r.item.media==='audio'||r.mediaMode==='audio';
 const seconds=(n:number)=>Number.isFinite(n)?n.toFixed(2):t('studio.run.video.preview.unknown');
 return <div className="reference-media">
  {image?<img className="reference-media-visual" src={r.item.src} alt={r.item.name} onError={()=>reportError('loadError')}/>:<>
   {native?<NativeVideo source={{path:r.item.path!}} label={r.item.name} muted={!audio&&r.mediaMode==='video'} audioOnly={audio} controlRef={nativeControl} onStatus={s=>{const seconds=!s.error&&Number.isFinite(s.duration)&&s.duration>0?s.duration:null;setDuration(seconds);if(s.error){fail('loadError',s.error);return;}if(seconds!==null&&r.sourceDuration!==seconds)onChange({sourceDuration:seconds});setPosition(s.position);if(s.paused){crop.current=false;setPlaying(false);}}}/>:audio?<audio ref={el=>{media.current=el;}} src={r.item.src} aria-label={r.item.name} {...events}/>:<video className="reference-media-visual" ref={el=>{media.current=el;}} src={r.item.src} aria-label={r.item.name} muted={r.mediaMode==='video'} {...events}/>}
   {r.item.media==='video'&&<select aria-label={t('studio.run.video.refMode',{name:r.item.name})} value={r.mediaMode} onChange={e=>onChange({mediaMode:e.target.value as VideoRef['mediaMode']})}>
    {(['video','video_audio','audio'] as const).map(mode=><option key={mode} value={mode}>{t(`studio.run.video.mode.${mode}`)}</option>)}
   </select>}
   <p className="reference-media-summary">{t('studio.run.video.preview.duration',{seconds:known?seconds(duration):t('studio.run.video.preview.unknown')})}</p>
   <p className="reference-media-summary">{t('studio.run.video.preview.selection',{start:seconds(start),end:known||r.trimEnd!==undefined?seconds(end):t('studio.run.video.preview.unknown'),seconds:valid?seconds(end-start):t('studio.run.video.preview.unknown')})}</p>
   <div className="reference-media-trims">{(['trimStart','trimEnd'] as const).map(key=><label key={key}>
    <span>{t(`studio.run.video.${key}`)}</span>
    <input type="number" aria-label={t(`studio.run.video.${key}Of`,{name:r.item.name})} min={0} max={known?Math.min(duration,3600):3600} step={0.01} value={r[key]??''} placeholder={t(key==='trimStart'?'studio.run.video.trimFromStart':'studio.run.video.trimToEnd')} onChange={e=>edit(key,e.target.value)}/>
    <input type="range" aria-label={t(`studio.run.video.preview.${key}Range`,{name:r.item.name})} min={0} max={known?Math.min(duration,3600):0} step={0.01} disabled={!known} value={Math.min(known?Math.min(duration,3600):0,Math.max(0,(key==='trimStart'?start:end)||0))} onChange={e=>edit(key,e.target.value)}/>
   </label>)}</div>
   {badTrim({...r,sourceDuration:duration??r.sourceDuration})&&<p className="error-message" role="alert">{t('studio.run.video.trimInvalid')}</p>}
   <div className="reference-media-actions">
    <button type="button" className="wb-dock-action" disabled={!valid} onClick={()=>void toggle()}>{t(`studio.run.video.preview.${playing?'pause':'play'}`)}</button>
    <button type="button" className="wb-dock-action" disabled={!known||!Number.isFinite(position)||position<0||position>duration||position>3600} onClick={()=>onChange({trimStart:position})}>{t('studio.run.video.preview.setIn')}</button>
    <button type="button" className="wb-dock-action" disabled={!known||!Number.isFinite(position)||position<0||position>duration||position>3600} onClick={()=>onChange({trimEnd:position})}>{t('studio.run.video.preview.setOut')}</button>
    <button type="button" className="wb-dock-action" onClick={()=>onChange({trimStart:undefined,trimEnd:undefined})}>{t('studio.run.video.preview.reset')}</button>
   </div>
  </>}
  {error&&<p role="alert" className="error-message">{error}</p>}
 </div>;
}
