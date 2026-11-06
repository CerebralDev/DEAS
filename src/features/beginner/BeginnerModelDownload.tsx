import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import {useI18n} from '../../i18n/I18nProvider';
import {browseDownloads,cancelDownloads,comfyFolder,planDownloads,readAuthStatus,readDownloadStatus,startDownloads,type DownloadPlan,type DownloadResource,type DownloadStatus} from '../../lib/downloads';
import type {ModelGuide} from './modelGuides';

/** Beginner-only presentation of the existing native browse / plan / confirm flow.
 * Does not import a catalog, change Library preferences or add a download engine. */
export function BeginnerModelDownload({guide,root,modelsDir,onStatus}:{guide:ModelGuide;root?:string;modelsDir?:string;onStatus:(status:DownloadStatus)=>void}){
 const {t}=useI18n();
 const [files,setFiles]=useState<DownloadResource[]>([]),[selected,setSelected]=useState(''),[busy,setBusy]=useState(false),[error,setError]=useState(false);
 const [plan,setPlan]=useState<DownloadPlan|null>(null),[status,setStatus]=useState<DownloadStatus|null>(null),[authenticated,setAuthenticated]=useState(false),[more,setMore]=useState<string|null>(null);
 const live=useRef(false),lock=useRef(false);const publish=useRef(onStatus);publish.current=onStatus;
 const request=async<T,>(operation:()=>Promise<T>,accept:(value:T)=>void)=>{
  if(lock.current||!live.current)return;lock.current=true;setBusy(true);setError(false);
  try{const value=await operation();if(live.current)accept(value);}catch{if(live.current){setError(true);report();}}finally{lock.current=false;if(live.current)setBusy(false);}
 };
 const report=()=>void invoke('native_logs_beginner_error',{operation:'download'}).catch(()=>{});
 const update=(value:DownloadStatus)=>{setStatus(value);publish.current(value);if(value.state==='failed')report();};
 const browse=(cursor?:string)=>void request(async()=>{
  const auth=(await readAuthStatus()).some(p=>p.providerId==='huggingface'&&p.configured);
  const listing=await browseDownloads({providerId:'huggingface',scope:'repo',locator:'models:'+guide.repo,authenticated:auth,...(cursor?{cursor}:{})});
  return {auth,listing};
 },({auth,listing})=>{setAuthenticated(auth);setFiles(old=>[...new Map([...old,...listing.resources.filter(r=>guide.files.includes((r.locator??r.label).split('/').pop()??''))].map(r=>[r.id,r])).values()]);setMore(listing.nextCursor);});
 useEffect(()=>{live.current=true;browse();return()=>{live.current=false;};},[]);
 useEffect(()=>{if(status?.state!=='running')return;let stopped=false;let timer:ReturnType<typeof setTimeout>;
  const poll=async()=>{try{const value=await readDownloadStatus();if(!stopped&&live.current){setError(false);update(value);if(value.state==='running')timer=setTimeout(()=>void poll(),1000);}}catch{if(!stopped&&live.current){setError(true);report();timer=setTimeout(()=>void poll(),2000);}}};
  timer=setTimeout(()=>void poll(),1000);return()=>{stopped=true;clearTimeout(timer);};
 },[status?.state]);
 const running=status?.state==='running';const resource=files.find(f=>f.id===selected);
 return <section className="beginner-base-download" aria-label={'Download '+guide.name}>
  <p>{t('beginner.downloadPick')}</p>
  <p className="beginner-download-source">{'Hugging Face · '+guide.repo}</p>
  <label>{t('beginner.downloadFile')}<select aria-label={t('beginner.downloadFile')} value={selected} disabled={busy||running} onChange={e=>{setSelected(e.target.value);setPlan(null);setStatus(null);}}><option value="">{t('beginner.downloadChooseFile')}</option>{files.map(f=><option key={f.id} value={f.id} disabled={!f.downloadable}>{f.label}</option>)}</select></label>
  {!busy&&!files.length&&<p>{t('beginner.downloadNoFile')}</p>}
  {!root&&!modelsDir&&<p>{t('beginner.downloadNeedFolder')}</p>}
  <button type="button" disabled={busy||running} onClick={()=>browse(more??undefined)}>{more?t('beginner.downloadMore'):t('beginner.downloadRefresh')}</button>
  <button type="button" disabled={busy||running||!resource||(!root&&!modelsDir)} onClick={()=>void request(()=>planDownloads({providerId:'huggingface',resources:[resource!],parentPath:comfyFolder(root??'',guide.target,modelsDir),folderName:'',authenticated}),setPlan)}>{t('beginner.downloadReview')}</button>
  {plan&&<div className="beginner-download-plan"><strong>{t('beginner.downloadPlanTitle')}</strong><p>{plan.target}</p><p>{plan.totalKnownBytes?(plan.totalKnownBytes/1073741824).toFixed(2)+' GiB':t('beginner.downloadSizeUnknown')}</p>{plan.warnings.map((warning,i)=><p key={i}>{warning}</p>)}<button type="button" className="aio-confirm-button" disabled={busy||running||!plan.canExecute||status?.state==='succeeded'} onClick={()=>void request(()=>startDownloads(plan.id),update)}>{t('beginner.downloadConfirm')}</button></div>}
  {busy&&<p role="status">{t('beginner.loading')}</p>}
  {status&&<p role="status">{status.state}{status.total?' · '+status.completed+' / '+status.total:''}</p>}
  {running&&<button type="button" disabled={busy} onClick={()=>void request(cancelDownloads,update)}>{t('beginner.downloadCancel')}</button>}
  {error&&<p role="alert">{t('beginner.downloadError')}</p>}
 </section>;
}
