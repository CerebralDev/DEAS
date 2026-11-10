import { readSelectedContext } from '../../lib/workspaceSelection';
import { useEffect, useRef, useState } from 'react';
import { isTauri, invoke } from '@tauri-apps/api/core';
import { useI18n } from '../../i18n/I18nProvider';
import type { JobIntent } from '../../lib/jobs';
import { isVideoEnhanceIntent, isVideoIntent, type VideoEnhanceIntentV1 } from '../../lib/videoIntent';
import { isMusicIntent } from '../../lib/musicIntent';
import { historicalRows, readHistory, type HistoryRecord } from '../studio/history';
import './monitoring.css';

/** Native `native_monitoring_sample` reply; every field may be absent on an unsupported host. */
export interface MonitoringSample {
 at:number; source:string;
 cpu:{percent:number;count:number}|null; ram:{used:number;total:number}|null; swap:{used:number;total:number}|null;
 disks:{path:string;used:number;total:number}[];
 diskIo:{readMibS:number;writeMibS:number}|null;
 gpus:{index:number;name:string;utilization:number|null;memoryUsed:number|null;memoryTotal:number|null;temperature:number|null;powerW:number|null;powerLimitW:number|null}[];
 warnings?:string[];
}
export const SAMPLE_LIMIT=60;
/** One graph per component, like the DaSiWa System Monitor's Full mode. `max` undefined = scale to the series. */
const metrics=[
 {key:'cpu',color:'#38bdf8',max:100,value:(s:MonitoringSample)=>s.cpu?.percent??null},
 {key:'ram',color:'#a78bfa',max:100,value:(s:MonitoringSample)=>s.ram&&s.ram.total>0?s.ram.used/s.ram.total*100:null},
 {key:'gpu',color:'#4ade80',max:100,value:(s:MonitoringSample)=>s.gpus[0]?.utilization??null},
 {key:'vram',color:'#22d3ee',max:100,value:(s:MonitoringSample)=>{const g=s.gpus[0];return g?.memoryUsed!=null&&g.memoryTotal?g.memoryUsed/g.memoryTotal*100:null;}},
 {key:'ioRead',color:'#34d399',max:undefined,value:(s:MonitoringSample)=>s.diskIo?.readMibS??null},
 {key:'ioWrite',color:'#f472b6',max:undefined,value:(s:MonitoringSample)=>s.diskIo?.writeMibS??null},
] as const;

/** What a finished attempt ran, read from its immutable effective intent. */
export function describeIntent(intent:JobIntent|VideoEnhanceIntentV1):{pipeline:string;model:string;size:string;steps:number|null;seed:number|null} {
 if(isVideoEnhanceIntent(intent))return {pipeline:'video-enhance',model:'',size:'',steps:null,seed:intent.seed};
 if(isMusicIntent(intent))return {pipeline:`music/${intent.mode}`,model:intent.checkpoint,size:`${intent.maxDuration}s`,steps:null,seed:intent.seed};
 if(isVideoIntent(intent))return {pipeline:`${intent.family}${intent.family==='minimax-h3'||intent.family==='minimax-h3-hyperflow'?`/${intent.h3.mode}`:''}`,model:intent.model,size:`${intent.width}×${intent.height} · ${intent.frames}f @ ${intent.frameRate}`,steps:intent.steps,seed:intent.seed};
 return {pipeline:intent.inputImage?'i2i':'t2i',model:intent.checkpoint,size:`${intent.width}×${intent.height}`,steps:intent.steps,seed:intent.seed};
}
/** queued = admission → start; running = start → generation end (or last observation while no end is known). */
export function timeline(r:HistoryRecord):{queuedMs:number;runningMs:number;wallMs:number} {
 const started=r.startedAt??null;
 const queuedMs=Math.max(0,(started??r.observedAt)-r.admittedAt);
 const runningMs=started==null?0:Math.max(0,r.generationMs??(r.observedAt-started));
 return {queuedMs,runningMs,wallMs:queuedMs+runningMs};
}
const gib=(bytes:number,locale:string)=>new Intl.NumberFormat(locale,{maximumFractionDigits:1}).format(bytes/2**30)+' GiB';
const seconds=(ms:number,locale:string)=>new Intl.NumberFormat(locale,{style:'unit',unit:'second',unitDisplay:'short',maximumFractionDigits:1}).format(ms/1000);
const pct=(v:number|null|undefined)=>v==null?'—':`${Math.round(v)} %`;
const mibs=(v:number|null|undefined)=>v==null?'—':`${v<10?v.toFixed(1):Math.round(v)} MiB/s`;
/** Peak line for one finished attempt; absent components are left out, nothing sampled = null. */
export function peakSummary(p:HistoryRecord['peaks'],locale:string):string|null {
 if(!p||!p.samples)return null;
 const parts=[p.cpuPercent!=null&&`CPU ${pct(p.cpuPercent)}`,p.ramUsed!=null&&`RAM ${gib(p.ramUsed,locale)}`,p.gpuUtilization!=null&&`GPU ${pct(p.gpuUtilization)}`,p.vramUsed!=null&&`VRAM ${gib(p.vramUsed,locale)}`,p.temperature!=null&&`${Math.round(p.temperature)} °C`,p.powerW!=null&&`${Math.round(p.powerW)} W`,(p.diskReadMibS!=null||p.diskWriteMibS!=null)&&`I/O ${mibs(p.diskReadMibS)} / ${mibs(p.diskWriteMibS)}`].filter(Boolean);
 return parts.length?parts.join(' · '):null;
}
/** One line of current values for the header; GPU index/name only, the rest as compact pairs. */
export function currentSummary(s:MonitoringSample,locale:string,t:(k:string)=>string):string {
 const parts=[s.cpu&&`${t('monitoring.cpu')} ${pct(s.cpu.percent)}`,s.ram&&`${t('monitoring.ram')} ${gib(s.ram.used,locale)} / ${gib(s.ram.total,locale)}`,s.swap&&s.swap.total>0&&`${t('monitoring.swap')} ${gib(s.swap.used,locale)}`,...s.disks.map(d=>`${t('monitoring.disk')} ${gib(d.used,locale)} / ${gib(d.total,locale)}`),s.diskIo&&`I/O ${mibs(s.diskIo.readMibS)} / ${mibs(s.diskIo.writeMibS)}`,...s.gpus.map(g=>`${t('monitoring.gpu')}${g.index} ${g.name.replace(/^NVIDIA (GeForce )?/,'')} ${pct(g.utilization)} · ${t('monitoring.vram')} ${g.memoryUsed!=null&&g.memoryTotal!=null?`${gib(g.memoryUsed,locale)} / ${gib(g.memoryTotal,locale)}`:'—'}${g.temperature!=null?` · ${Math.round(g.temperature)} °C`:''}${g.powerW!=null?` · ${Math.round(g.powerW)} W`:''}`)].filter(Boolean);
 return parts.join(' · ');
}

/** Y axis top for an auto-scaled series: a round MiB/s step above the maximum, never below 1. */
export function axisTop(values:(number|null)[]):number {
 const peak=Math.max(1,...values.map(v=>v??0));
 const step=10**Math.floor(Math.log10(peak));
 return Math.ceil(peak/step)*step;
}
const PAD={left:58,right:14,top:8,bottom:20};
function Graph({series,times,color,max,unit,label}:{times:number[];series:(number|null)[];color:string;max?:number;unit:string;label:string}){
 const canvas=useRef<HTMLCanvasElement>(null);
 // CSS box × devicePixelRatio, so the backing store is never stretched (a fixed 900 px store blurred at other widths).
 const [box,setBox]=useState({w:0,h:0,dpr:1});
 useEffect(()=>{
  const el=canvas.current;if(!el)return;
  const measure=()=>{const r=el.getBoundingClientRect();setBox({w:Math.round(r.width),h:Math.round(r.height),dpr:window.devicePixelRatio||1});};
  measure();
  const observer=typeof ResizeObserver!=='undefined'?new ResizeObserver(measure):null;observer?.observe(el);
  return()=>observer?.disconnect();
 },[]);
 useEffect(()=>{
  const el=canvas.current,ctx=el?.getContext('2d');if(!el||!ctx||box.w<PAD.left+PAD.right+10)return;
  el.width=Math.round(box.w*box.dpr);el.height=Math.round(box.h*box.dpr);ctx.setTransform(box.dpr,0,0,box.dpr,0,0);
  const w=box.w,h=box.h,x0=PAD.left,x1=w-PAD.right,y0=PAD.top,y1=h-PAD.bottom;ctx.clearRect(0,0,w,h);
  // Hairlines on the device-pixel grid stay one physical pixel wide instead of a 2 px grey smear.
  const crisp=(v:number)=>(Math.round(v*box.dpr)+0.5)/box.dpr;
  const css=getComputedStyle(el),theme=(name:string,fallback:string)=>css.getPropertyValue(name).trim()||fallback;
  const top=max??axisTop(series);
  // The plot frame holds only grid and series; the scale sits outside it.
  ctx.fillStyle=theme('--surface','#20262b');ctx.fillRect(x0,y0,x1-x0,y1-y0);
  ctx.font='11px system-ui, sans-serif';ctx.fillStyle=theme('--muted','rgba(255,255,255,.6)');ctx.strokeStyle='rgba(255,255,255,.1)';ctx.lineWidth=1/box.dpr;
  // Y axis: 0, 25, 50, 75, 100 % or quarters of the rounded top.
  ctx.textAlign='right';ctx.textBaseline='middle';
  for(const f of [0,0.25,0.5,0.75,1]){const y=crisp(y1-f*(y1-y0));ctx.beginPath();ctx.moveTo(x0,y);ctx.lineTo(x1,y);ctx.stroke();const v=top*f;ctx.fillText(`${max===100?Math.round(v):v>=10?Math.round(v):v.toFixed(1)}${unit}`,x0-6,y);}
  // X axis: seconds before now, one tick per 15 samples (1 Hz).
  ctx.textBaseline='top';
  for(let s=0;s<SAMPLE_LIMIT;s+=15){const x=crisp(x1-s/(SAMPLE_LIMIT-1)*(x1-x0));ctx.beginPath();ctx.moveTo(x,y0);ctx.lineTo(x,y1);ctx.stroke();ctx.textAlign=s===0?'right':'center';ctx.fillText(s===0?'0 s':`−${s} s`,s===0?x1:x,y1+5);}
  ctx.textAlign='left';ctx.fillText(`−${SAMPLE_LIMIT} s`,x0,y1+5);
  ctx.strokeStyle=theme('--line','#435256');ctx.strokeRect(crisp(x0),crisp(y0),Math.round(x1-x0),Math.round(y1-y0));
  const step=(x1-x0)/(SAMPLE_LIMIT-1);
  ctx.save();ctx.beginPath();ctx.rect(x0,y0,x1-x0,y1-y0);ctx.clip();
  ctx.strokeStyle=color;ctx.lineWidth=1.5;ctx.lineJoin='round';ctx.beginPath();let pen=false;
  series.forEach((v,i)=>{const x=x1-Math.max(0,(times[times.length-1]-times[i])/1000)*step;if(v==null){pen=false;return;}const y=y1-Math.min(top,Math.max(0,v))/top*(y1-y0);if(pen)ctx.lineTo(x,y);else ctx.moveTo(x,y);pen=true;});
  ctx.stroke();ctx.restore();
 },[series,times,color,max,unit,box]);
 return <canvas ref={canvas} className="monitoring-graph" role="img" aria-label={label} data-monitoring-graph/>;
}

/** Monitoring tab: the last ten native attempts with their timing and peaks, and live host/GPU telemetry while the tab is visible. */
export function Monitoring({active,root,compact=false}:{active:boolean;root?:string;compact?:boolean}){
 const {t,locale}=useI18n();
 const [records,setRecords]=useState<HistoryRecord[]>([]);
 const [historyFailed,setHistoryFailed]=useState(false);
 const [samples,setSamples]=useState<MonitoringSample[]>([]);
 const [sampleError,setSampleError]=useState('');
 const native=isTauri();
 const sampling=useRef(false),readingHistory=useRef(false);
 const interval=compact?5000:1000;
 useEffect(()=>{
  if(!active||!native)return;
  let alive=true,timer:ReturnType<typeof setTimeout>|undefined;
  const read=async()=>{
   if(document.visibilityState!=='visible'||readingHistory.current){if(alive)timer=setTimeout(()=>void read(),interval);return;}
   readingHistory.current=true;
   try{const value=await readHistory();if(alive){setRecords(historicalRows(value.records));setHistoryFailed(false);}}
   catch{if(alive)setHistoryFailed(true);}
   finally{readingHistory.current=false;if(alive)timer=setTimeout(()=>void read(),Math.max(interval,2000));}
  };
  void read();return()=>{alive=false;clearTimeout(timer);};
 },[active,native,interval]);
 useEffect(()=>{
  if(!active||!native)return;
  let alive=true,timer:ReturnType<typeof setTimeout>|undefined;
  const poll=async()=>{
   if(document.visibilityState==='visible'&&!sampling.current){
    sampling.current=true;
    try{const s=await invoke<MonitoringSample>('native_monitoring_sample',{root:root??null});if(alive){setSamples(prev=>[...prev,s].slice(-SAMPLE_LIMIT));setSampleError('');}}
    catch(e){if(alive)setSampleError(String(e));void invoke('native_logs_workspace_error',{operation:'monitoring'}).catch(()=>{});}
    finally{sampling.current=false;}
   }
   if(alive)timer=setTimeout(()=>void poll(),interval);
  };
  void poll();return()=>{alive=false;clearTimeout(timer);};
 },[active,native,root,interval]);
 const finished=records.filter(r=>r.state!=='queued'&&r.state!=='running').slice(0,10);
 const latest=finished.find(r=>r.state==='succeeded'&&r.generationMs!=null);
 const maxWall=Math.max(1,...finished.map(r=>timeline(r).wallMs));
 const current=samples[samples.length-1];
 const running=records.find(r=>r.state==='running'||r.state==='queued');
 const age=current?Math.max(0,Date.now()-current.at):null;
 const [liveProgress,setLiveProgress]=useState(()=>readSelectedContext('Monitoring').jobStatus);
 useEffect(()=>{const update=()=>setLiveProgress(readSelectedContext('Monitoring').jobStatus);window.addEventListener('deas:workspace-context',update);return()=>window.removeEventListener('deas:workspace-context',update);},[]);
 return <section className="monitoring" data-monitoring aria-label={t('monitoring.title')}>
  <header className="monitoring-heading"><div><p className="eyebrow">{t('monitoring.tab')}</p><h2>{t('monitoring.title')}</h2>
    {current&&<p className="monitoring-current" data-monitoring-values title={current.warnings?.join(' · ')}>{currentSummary(current,locale,t)} · {t('workspace.sampleAge',{seconds:String(Math.round((age??0)/1000))})}{(age??0)>15000?` · ${t('workspace.stale')}`:''}</p>}</div>
   {latest&&<p className="monitoring-headline" data-monitoring-headline><strong>{seconds(latest.generationMs!,locale)}</strong><span>{t('monitoring.lastDuration')}</span></p>}</header>
  {compact&&sampleError&&<p role="alert">{sampleError}</p>}
  {running&&<p className="monitoring-current">{liveProgress||t(`studio.jobs.${running.state}`)} · {seconds(Date.now()-(running.startedAt??running.admittedAt),locale)}</p>}
  {!current&&native&&<p className="field-help">{t('workspace.noSample')}</p>}
  {!native&&<p className="field-help">{t('studio.history.native')}</p>}
  <section className="monitoring-panel" aria-labelledby="monitoring-generations"><h3 id="monitoring-generations">{t('monitoring.generations')}</h3>
   {historyFailed&&<p className="error-message" role="alert">{t('studio.history.failed')}</p>}
   {native&&!historyFailed&&finished.length===0&&<p className="field-help">{t('studio.history.empty')}</p>}
   {finished.length>0&&<table className="monitoring-table"><thead><tr><th>{t('monitoring.col.outcome')}</th><th>{t('monitoring.col.pipeline')}</th><th>{t('monitoring.col.model')}</th><th>{t('monitoring.col.size')}</th><th>{t('monitoring.col.steps')}</th><th>{t('monitoring.col.seed')}</th><th>{t('monitoring.col.queued')}</th><th>{t('monitoring.col.duration')}</th><th>{t('monitoring.col.timeline')}</th><th>{t('monitoring.col.peaks')}</th></tr></thead>
    <tbody>{finished.map(r=>{const d=describeIntent(r.intent),tl=timeline(r),peaks=peakSummary(r.peaks,locale);return <tr key={r.id} data-monitoring-row={r.state}>
     <td><span data-state={r.state}>{t(`studio.jobs.${r.state}`)}</span></td><td>{d.pipeline}</td><td className="monitoring-model" title={d.model}>{d.model}</td><td>{d.size}</td><td>{d.steps??'—'}</td><td>{d.seed??'—'}</td>
     <td>{seconds(tl.queuedMs,locale)}</td><td>{r.generationMs!=null?seconds(r.generationMs,locale):'—'}</td>
     <td><div className="monitoring-timeline" role="img" aria-label={t('monitoring.timeline',{queued:seconds(tl.queuedMs,locale),running:seconds(tl.runningMs,locale)})}><span className="is-queued" style={{width:`${tl.queuedMs/maxWall*100}%`}}/><span className={`is-${r.state}`} style={{width:`${tl.runningMs/maxWall*100}%`}}/></div></td>
     <td className="monitoring-peaks" data-monitoring-peaks>{peaks??<span className="field-help">{t('monitoring.noPeaks')}</span>}</td>
    </tr>;})}</tbody></table>}
  </section>
  <section className="monitoring-panel" aria-labelledby="monitoring-system"><h3 id="monitoring-system">{t('monitoring.system')}</h3>
   {sampleError&&<p className="error-message" role="alert">{sampleError}</p>}
   {current&&<>
    <div className="monitoring-graphs">{metrics.map(m=>{const series=samples.map(m.value);const now=series[series.length-1];return <figure key={m.key} className="monitoring-metric" data-monitoring-metric={m.key}>
     <figcaption><span style={{background:m.color}}/>{t(`monitoring.${m.key}`)}<strong>{m.max===100?pct(now):mibs(now)}</strong></figcaption>
     <Graph times={samples.map(s=>s.at)} series={series} color={m.color} max={m.max} unit={m.max===100?' %':' MiB/s'} label={t('monitoring.graphOf',{metric:t(`monitoring.${m.key}`)})}/>
    </figure>;})}</div>
    {current.gpus.length===0&&<p className="field-help">{t('monitoring.noGpu')}</p>}
    <p className="field-help">{t('monitoring.source',{source:current.source})}{current.warnings?.length?` · ${current.warnings.join(' · ')}`:''}</p>
   </>}
  </section>
 </section>;
}
