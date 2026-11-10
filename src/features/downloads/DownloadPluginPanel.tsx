import { Fragment, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { NeuralBlob } from '../../components/neural/NeuralBlob';
import { useI18n } from '../../i18n/I18nProvider';
import { filesPresent } from '../../lib/settings';
import { verifyAuthToken, importDownloadCatalog, restoreAuthToken, browseDownloads, cancelDownloads, pickDownloadParent, planDownloads, readAuthStatus, readDownloadCatalog, readDownloadStatus, removeAuthToken, setAuthToken, startDownloads, comfyFolder, openTokenPage } from '../../lib/downloads';
import type { AuthVerification, AuthMetadata, CatalogEntry, DownloadCatalog, DownloadListing, DownloadPlan, DownloadResource, DownloadScope, DownloadStatus, PlanRequest } from '../../lib/downloads';
import { isString, oneOf, usePref } from '../../lib/preferences';
import { OrganizedModels } from '../../components/models/ModelLibrary';
import { fetchModelInfo, readModelScanStatus, scanModels, startModelScan } from '../../lib/models';
import type { LocalModel, ModelScanScope, ModelScanStatus } from '../../lib/models';
import './downloads.css';
import { ModelMetadataTarget, ModelDeclaredBadge, useModelMetadataUpdates } from '../../components/models/ModelMetadataEditor';
const validSort=(v:unknown):v is {by:'newest'|'name';dir:1|-1}=>typeof v==='object'&&v!==null&&['newest','name'].includes((v as {by:unknown}).by as string)&&[1,-1].includes((v as {dir:unknown}).dir as number);

/** One operation at a time; generation changes and unmount discard late responses. */
function useNativeRequest() {
 const native=isTauri();const live=useRef(false);const epoch=useRef(0);const lock=useRef(false);
 const [busy,setBusy]=useState(false);const [error,setError]=useState(false);
 useEffect(()=>{live.current=true;return()=>{live.current=false;epoch.current++;};},[]);
 const invalidate=useCallback(()=>{epoch.current++;setError(false);},[]);
 const request=useCallback(async <T,>(operation:()=>Promise<T>,publish:(value:T)=>void)=>{
  if(!native||!live.current||lock.current)return;
  lock.current=true;setBusy(true);const generation=epoch.current;
  try {const value=await operation();if(live.current&&generation===epoch.current){publish(value);setError(false);}}
  catch {if(live.current&&generation===epoch.current)setError(true);}
  finally {lock.current=false;if(live.current)setBusy(false);}
 },[native]);
 return {native,busy,error,request,invalidate};
}

export const authProviders=[{id:'huggingface',label:'Hugging Face',origin:'https://huggingface.co'},{id:'civitai',label:'CivitAI',origin:'https://civitai.com'},{id:'github',label:'GitHub',origin:'https://api.github.com'}];
/** `embedded` panels are always open and have no bubble; a parent tab strip decides which one is mounted. */
export interface AuthPanelProps { onChange?:()=>void; embedded?:boolean }
/** Fixed native credential audiences; never infer audiences from external catalog entries. */
export function AuthPanel({onChange,embedded=false}:AuthPanelProps={}) {
 const {t}=useI18n();const io=useNativeRequest();const [opened,setOpened]=useState(false);const open=embedded||opened;
 const [providerId,setProviderId]=useState('huggingface');const [persist,setPersist]=useState(true);const [metadata,setMetadata]=useState<AuthMetadata[]>([]);
 const token=useRef<HTMLInputElement>(null);const [hasToken,setHasToken]=useState(false);
 const clear=()=>{if(token.current)token.current.value='';setHasToken(false);};
 useEffect(()=>{const input=token.current;return()=>{if(input)input.value='';};},[open]);
 const [statusError,setStatusError]=useState(false);const metadataEpoch=useRef(0);
 useEffect(()=>{if(!open||!io.native)return;let alive=true;const epoch=metadataEpoch.current;readAuthStatus().then(value=>{if(alive&&epoch===metadataEpoch.current){setMetadata(value);setStatusError(false);}},()=>{if(alive&&epoch===metadataEpoch.current)setStatusError(true);});return()=>{alive=false;};},[open,io.native]);
 const current=metadata.find(p=>p.providerId===providerId);
 const [checkRevision,setCheckRevision]=useState(0);const [check,setCheck]=useState<{providerId:string;status:AuthVerification['status']|'checking'}|null>(null);
 useEffect(()=>{
  if(!open||!io.native||!current?.configured){setCheck(null);return;}
  let alive=true;setCheck({providerId,status:'checking'});
  verifyAuthToken(providerId).then(value=>{if(alive)setCheck(value);},()=>{if(alive)setCheck({providerId,status:'unavailable'});});
  return()=>{alive=false;};
 },[open,io.native,providerId,current?.configured,checkRevision]);
 const checkStatus=check?.providerId===providerId?check.status:null;
 const changed=(value:AuthMetadata)=>{setCheckRevision(v=>v+1);metadataEpoch.current++;setStatusError(false);setMetadata(old=>[...old.filter(p=>p.providerId!==value.providerId),value]);clear();onChange?.();};
 return <section className="feature-panel download-module" aria-label={t('downloads.auth')}>
  {!embedded&&<NeuralBlob id="provider-auth" label={t('downloads.auth')} description={t('downloads.authBubble')} selected={open} onSelect={()=>{clear();io.invalidate();setOpened(v=>!v);}}/>}
  {open&&<div className="download-controls">
   <p className="field-help">{t(io.native?'downloads.tokenHelp':'downloads.browser')}</p>
   {/* Overview: one button per provider with its saved-token state; the pressed one shows its settings below. */}
   <div className="auth-providers" role="group" aria-label={t('downloads.provider')}>{authProviders.map(p=>{const m=metadata.find(x=>x.providerId===p.id);return <button key={p.id} type="button" data-provider={p.id} className={`auth-provider${m?.configured?' is-set':''}`} aria-pressed={providerId===p.id} disabled={io.busy} onClick={()=>{if(p.id===providerId)return;clear();io.invalidate();setProviderId(p.id);}}><strong>{m?.configured&&<span className="auth-set-mark" aria-hidden="true">✓ </span>}{p.label}</strong><small>{t(m?.configured?'downloads.auth.set':'downloads.auth.none')}</small></button>;})}</div>
   <div className="auth-settings" aria-label={authProviders.find(p=>p.id===providerId)?.label} role="region">
   <p className="download-identity">{authProviders.find(p=>p.id===providerId)?.origin} <button type="button" className="download-link" disabled={!io.native||io.busy} onClick={()=>void io.request(()=>openTokenPage(providerId),()=>{})}>{t('downloads.openTokenPage')}</button></p>
   <p className="field-help">{t('downloads.tokenPageHelp')}</p>
   <p role="status"><span className={current?.configured&&checkStatus==='verified'?'auth-verified':undefined}>{current?.configured&&checkStatus==='verified'&&<span aria-hidden="true">✓ </span>}{t(current?.configured?(checkStatus?`downloads.authCheck.${checkStatus}`:'downloads.supplied'):'downloads.anonymous')}</span>{current?.persisted&&<> · {t('downloads.persisted')}</>}</p>
   <label>{t('downloads.token')}<input aria-label={t('downloads.token')} ref={token} type="password" autoComplete="off" spellCheck={false} disabled={!io.native||io.busy} onChange={e=>setHasToken(!!e.target.value.trim())}/></label>
   <label className="download-check"><input type="checkbox" checked={persist} disabled={io.busy} onChange={e=>setPersist(e.target.checked)}/>{t('downloads.persist')}</label>
   <div className="aio-actions">
    <button className="action-button" disabled={!io.native||io.busy||!hasToken} onClick={()=>{const secret=token.current?.value;if(secret)void io.request(()=>setAuthToken(providerId,secret,persist),changed);}}>{t('downloads.applyToken')}</button>
    <button className="action-button" disabled={!io.native||io.busy} onClick={()=>void io.request(()=>removeAuthToken(providerId),changed)}>{t('downloads.clearToken')}</button>
    <button className="action-button" disabled={!io.native||io.busy} onClick={()=>void io.request(()=>restoreAuthToken(providerId),changed)}>{t('downloads.restoreAuth')}</button>
    <button className="action-button" disabled={!io.native||io.busy} onClick={()=>void io.request(readAuthStatus,value=>{setCheckRevision(v=>v+1);metadataEpoch.current++;setStatusError(false);setMetadata(value);onChange?.();})}>{t('downloads.authStatus')}</button>
   </div>
   </div>
   {(io.error||statusError)&&<p role="alert">{t('downloads.authError')}</p>}
  </div>}
 </section>;
}

/** Catalog previews are plain HTTPS stills; CSP restricts them to civitai hosts. */
function Preview({src,alt}:{src?:string;alt:string}) {
 if(!src)return <span className="download-preview is-empty" aria-hidden="true"/>;
 return <img className="download-preview" src={src} alt={alt} loading="lazy"/>;
}
// ComfyUI model folder order; unknown catalog targets follow alphabetically.
const categoryOrder=['models','loras','vae','text_encoders','clip_vision','controlnet','upscale_models','embeddings','workflows'];
// Checkpoints and diffusion models share one "Models" tab; the entry's own target stays the destination folder.
const modelFolders=['checkpoints','diffusion_models'];
const tabOf=(target:string)=>modelFolders.includes(target)?'models':target;
/** Files already inside the ComfyUI models folder; CivitAI metadata is fetched per file on request and cached natively. */
function InstalledModels({root,extraModels=[],locked,category,setCategory}:{root:string;extraModels?:string[];locked:boolean;category:string;setCategory:(c:string)=>void}) {
 const {t}=useI18n();const io=useNativeRequest();
 const extraKey=JSON.stringify(extraModels);const [models,setModels]=useState<LocalModel[]|null>(null);useModelMetadataUpdates(setModels);const [scanError,setScanError]=useState(false);const [selected,setSelected]=useState('');
 useEffect(()=>{if(!io.native)return;let alive=true;setModels(null);setScanError(false);setSelected('');scanModels(root,extraModels).then(v=>{if(alive)setModels(v);},()=>{if(alive)setScanError(true);});return()=>{alive=false;};},[root,io.native,extraKey]);
 const tabs=sortCategories([...new Set((models??[]).map(m=>tabOf(m.category)))]);
 const tab=tabs.includes(category)?category:tabs[0]??'';
 const shown=(models??[]).filter(m=>tabOf(m.category)===tab).sort((a,b)=>a.name.localeCompare(b.name));
 const fetch=(m:LocalModel)=>void io.request(async()=>fetchModelInfo(m.path,(await readAuthStatus()).some(p=>p.providerId==='civitai'&&p.configured)),v=>setModels(old=>old?.map(x=>x.path===v.path?v:x)??old));
 const [scanScope,setScanScope]=useState<ModelScanScope>('all');
 const [scanStatus,setScanStatus]=useState<ModelScanStatus|null>(null);const [statusError,setStatusError]=useState(false);const scanEpoch=useRef(0);
 const scanning=scanStatus?.state==='running';
 useEffect(()=>{if(!io.native)return;let alive=true;let timer:ReturnType<typeof setTimeout>;let observed='';setScanStatus(null);
  const poll=async()=>{const epoch=scanEpoch.current;try{const value=await readModelScanStatus();if(!alive)return;
   if(epoch===scanEpoch.current){setScanStatus(value);setStatusError(false);
    const terminal=value.id&&value.state!=='running'?value.id:'';
    if(terminal&&terminal!==observed){const models=await scanModels(root,extraModels);if(alive)setModels(models);}
    observed=terminal;
   }
  }catch{if(alive)setStatusError(true);}finally{if(alive)timer=setTimeout(()=>void poll(),1000);}};
  void poll();return()=>{alive=false;clearTimeout(timer);};
 },[root,io.native,extraKey]);
 const startScan=()=>{scanEpoch.current++;void io.request(async()=>startModelScan(root,scanScope,(await readAuthStatus()).some(p=>p.providerId==='civitai'&&p.configured)),value=>{scanEpoch.current++;setScanStatus(value);});};
 const detail=(m:LocalModel)=>{const c=m.info?.civitai;const s=c?.settings;const settings=s?[s.sampler,s.scheduler,s.steps!==undefined&&t('downloads.installed.steps',{n:s.steps}),s.cfgScale!==undefined&&t('downloads.installed.cfg',{n:s.cfgScale}),s.size,s.clipSkip!==undefined&&t('downloads.installed.clipSkip',{n:s.clipSkip})].filter(Boolean).join(' · '):'';
  return <div className="download-picker"><ModelDeclaredBadge model={m}/><dl className="download-identity">
   <dt>{t('downloads.installed.path')}</dt><dd>{m.path}</dd><dt>{t('downloads.size')}</dt><dd>{t('downloads.installed.sizeGib',{n:(m.size/1073741824).toFixed(2)})}</dd>{m.integrity&&<><dt>{t('downloads.installed.integrity')}</dt><dd>{t('downloads.installed.truncated')}</dd></>}<dt>{t('downloads.digest')}</dt><dd>{m.info?.sha256??t('downloads.installed.unhashed')}</dd>
   {m.info&&<><dt>{t('downloads.installed.civitai')}</dt><dd>{c?[c.name,c.version,c.base].filter(Boolean).join(' · '):t('downloads.installed.none')}</dd></>}
   {c&&c.trainedWords.length>0&&<><dt>{t('downloads.installed.words')}</dt><dd>{c.trainedWords.join(', ')}</dd></>}
   {settings&&<><dt>{t('downloads.installed.settings')}</dt><dd>{settings}</dd></>}
  </dl><p className="field-help">{t('downloads.installed.hashHelp')}</p>
  <button className="action-button" disabled={io.busy||locked||scanning||!scanStatus||statusError} onClick={()=>fetch(m)}>{t(m.info?'downloads.installed.refresh':'downloads.installed.fetch')}</button>
  {io.busy&&<p role="status"><span className="aio-spinner" aria-hidden="true"/> {t('downloads.pending')}</p>}{io.error&&<p role="alert">{t('downloads.installed.fetchError')}</p>}</div>;};
 return <>
  {io.native&&!!models&&models.length>0&&<div className="download-source-row">
   <label>{t('downloads.installed.scanScope')}<select data-scan-scope aria-label={t('downloads.installed.scanScope')} value={scanScope} disabled={locked||scanning||io.busy} onChange={e=>setScanScope(e.target.value as 'all'|'models'|'loras')}><option value="all">{t('downloads.installed.scopeAll')}</option><option value="models">{t('downloads.installed.scopeModels')}</option><option value="loras">{t('downloads.installed.scopeLoras')}</option></select></label>
   <button className="action-button" type="button" data-scan-files disabled={locked||scanning||io.busy||!scanStatus||statusError} onClick={startScan}>{t('downloads.installed.scanAll')}</button>
   <p className="field-help">{t('downloads.installed.scanAllHelp')}</p>
  </div>}
  {scanStatus?.id&&<p role="status">{scanning&&<span className="aio-spinner" aria-hidden="true"/>} {t(scanning?'downloads.installed.scanning':scanStatus.state==='failed'?'downloads.installed.scanFailed':'downloads.installed.scanDone')} {t('downloads.installed.scanProgress',{completed:scanStatus.completed,total:scanStatus.total,remaining:scanStatus.remaining,failed:scanStatus.failed})} {scanStatus.file}</p>}
  {(statusError||(io.error&&!selected))&&<p role="alert">{t('downloads.installed.fetchError')}</p>}
  {tabs.length>0&&<div className="download-categories" role="tablist" aria-label={t('downloads.category')}>{tabs.map(c=><button key={c} role="tab" type="button" aria-selected={tab===c} disabled={locked} onClick={()=>{setSelected('');setCategory(c);}}>{t(`downloads.folder.${c}`)}</button>)}</div>}
  {models===null&&!scanError&&<p role="status">{t('downloads.pending')}</p>}{scanError&&<p role="alert">{t('downloads.installed.scanError')}</p>}{models&&!models.length&&<p className="field-help">{t('downloads.installed.empty',{root})}</p>}
  <OrganizedModels models={shown} scope="installed" listClass="download-cards" onRefresh={()=>{void scanModels(root,extraModels).then(setModels,()=>setScanError(true));}} renderTile={m=><li key={m.path}><ModelMetadataTarget model={m}><button type="button" aria-label={m.name} aria-haspopup="menu" aria-keyshortcuts="Shift+F10" className="download-card" aria-pressed={selected===m.path} disabled={locked} onClick={()=>setSelected(v=>v===m.path?'':m.path)}><Preview src={m.info?.civitai?.preview} alt=""/>{m.info?.civitai?.nsfw&&<em className="download-auth download-nsfw-tag" title={t('downloads.nsfwHelp')}>{t('downloads.nsfwTag')}</em>}<span>{m.name}</span><ModelDeclaredBadge model={m}/></button></ModelMetadataTarget>{selected===m.path&&detail(m)}</li>}/>
 </>;
}
export interface DownloadPanelProps { installTarget?:string; modelsDir?:string;extraModels?:string[]; authRevision?:number; onStatus?:(status:DownloadStatus)=>void; embedded?:boolean; initialEntryId?:string; initialEntryIds?:string[]; filenameFilter?:string|string[] }
// Undated sources stay last; never infer publication dates from filenames or IDs.
const published=(r:{publishedAt?:string})=>Date.parse(r.publishedAt??'')||0;
const key=(r:DownloadResource)=>JSON.stringify([r.id,r.url,r.locator]);
const views=['catalog','installed'] as const;
// The bundled DaSiWa catalog stays the default source; `civitai-creator-*` user entries add live creator listings.
/** Brand name of the bundled catalog, not a translatable label. */
const BUNDLED_SOURCE='DaSiWa';
const isCreatorSource=(e:CatalogEntry)=>e.id.startsWith('civitai-creator-')&&e.scope==='user';
/** A ticked file keeps the browse context it was picked in, so selections from many cards download together. */
interface Picked {resource:DownloadResource;providerId:string;authenticated:boolean;folder:string}
interface Group {request:PlanRequest;keys:string[]}
export function DownloadPanel({installTarget,modelsDir,extraModels,authRevision=0,onStatus,embedded=false,initialEntryId,initialEntryIds,filenameFilter}:DownloadPanelProps={}) {
 const {t}=useI18n();const io=useNativeRequest();const [opened,setOpened]=useState(false);const open=embedded||opened;
 const suggested=useMemo(()=>initialEntryIds??(initialEntryId?[initialEntryId]:undefined),[initialEntryIds,initialEntryId]);
 const [catalogText,setCatalogText]=useState('');const [sort,setSort]=usePref<{by:'newest'|'name';dir:1|-1}>('library.sort',{by:'newest',dir:-1},validSort);
 // Undated items stay last in both directions; name breaks date ties.
 const compare=(a:{label:string;publishedAt?:string},b:{label:string;publishedAt?:string})=>sort.by==='newest'?(+!published(a)-+!published(b))||sort.dir*(published(a)-published(b))||a.label.localeCompare(b.label):sort.dir*a.label.localeCompare(b.label);
 const sortBy=(by:'newest'|'name')=>setSort(s=>s.by===by?{by,dir:s.dir===1?-1:1}:{by,dir:by==='newest'?-1:1});
 const [catalog,setCatalog]=useState<DownloadCatalog|null>(null);const [providerId,setProviderId]=useState('');
 const [scope,setScope]=useState<DownloadScope>('index');const [locator,setLocator]=useState('');const [entryId,setEntryId]=useState('');const [authenticated,setAuthenticated]=useState(false);
 const [category,setCategory]=usePref('library.category','',isString);const [folder,setFolder]=useState('');const [savedBase,setBase]=usePref('library.base','',isString);
 const [mediaFilter,setMediaFilter]=usePref('library.media','all',oneOf('all','image','video'));const [search,setSearch]=useState('');
 const [listing,setListing]=useState<DownloadListing|null>(null);const [basket,setBasket]=useState<Picked[]>([]);const [version,setVersion]=useState('');
 const [parentPath,setParentPath]=useState('');const [folderName,setFolderName]=useState('');const [blocked,setBlocked]=useState<DownloadPlan|null>(null);const [queue,setQueue]=useState<Group[]>([]);const [status,setStatus]=useState<DownloadStatus|null>(null);
 const [view,setView]=useState<'catalog'|'installed'>('catalog');const modelsRoot=installTarget?modelsDir??`${installTarget}/ComfyUI/models`:'';
 const selectView=(next:'catalog'|'installed')=>{if(next==='catalog'&&catalog){const tabs=categories(catalog);setCategory(c=>tabs.includes(c)?c:tabs[0]??'');}setView(next);};
 // Global search selects the Library view without touching any draft.
 const selectViewLive=useRef(selectView);selectViewLive.current=selectView;
 useEffect(()=>{const on=(e:Event)=>{const next=(e as CustomEvent).detail;if(next==='catalog'||next==='installed')selectViewLive.current(next);};window.addEventListener('deas:library-view',on);return()=>window.removeEventListener('deas:library-view',on);},[]);
 const running=status?.state==='running';const executionLock=useRef(false);const locked=running||executionLock.current||!!suggested&&!status;
 const edit=()=>{io.invalidate();setBlocked(null);};
 const sourceEdit=()=>{edit();setListing(null);setVersion('');};
 useEffect(()=>{if(!executionLock.current)io.invalidate();setBlocked(null);setListing(null);},[authRevision,io.invalidate]);
 // Embedded panels load the catalog on mount; a direct call (not io.request) survives StrictMode's effect replay.
 useEffect(()=>{if(!embedded||catalog||!io.native)return;let alive=true;readDownloadCatalog().then(value=>{if(alive)applyCatalog(value);},()=>{});return()=>{alive=false;};},[embedded,io.native]); // eslint-disable-line react-hooks/exhaustive-deps
 const authChoices=useRef(new Map<string,boolean>());
 // Providers with a saved token; null until known so the login hint never flashes.
 const [tokens,setTokens]=useState<Set<string>|null>(null);const needsLogin=(auth:boolean|undefined,id:string)=>!!auth&&!!tokens&&!tokens.has(id);
 useEffect(()=>{if(!io.native||!open)return;let alive=true;readAuthStatus().then(v=>{if(alive)setTokens(new Set(v.filter(m=>m.configured).map(m=>m.providerId)));},()=>{});return()=>{alive=false;};},[io.native,open,authRevision]);
 const provider=catalog?.providers.find(p=>p.id===providerId);
 const applyCatalog=(value:DownloadCatalog)=>{sourceEdit();setCatalog(value);setProviderId(value.providers[0]?.id??'');setScope(value.providers[0]?.scopes[0]??'index');setLocator('');setEntryId('');const first=categories(value)[0];setCategory(c=>categories(value).includes(c)?c:first||'');};
 const load=()=>void io.request(readDownloadCatalog,applyCatalog);
 const publishStatus=useCallback((value:DownloadStatus)=>{executionLock.current=value.state==='running';setStatus(value);onStatus?.(value);},[onStatus]);
 useEffect(()=>{
  if(!suggested||!io.native)return;
  let alive=true;
  // Catalog browsing invalidates io.request; recovery must survive that independent read.
  readDownloadStatus().then(value=>{if(alive)publishStatus(value);},error=>{if(alive)void io.request(()=>Promise.reject(error),()=>{});});
  return()=>{alive=false;};
 },[suggested,io.native]);
 useEffect(()=>{
  if(!io.native||!running)return;
  let alive=true;let timer:ReturnType<typeof setTimeout>;
  const poll=async()=>{await io.request(readDownloadStatus,value=>{if(alive)publishStatus(value);});if(alive)timer=setTimeout(()=>void poll(),1000);};
  timer=setTimeout(()=>void poll(),1000);
  return()=>{alive=false;clearTimeout(timer);};
 },[io.native,io.request,running,publishStatus]);
 const browse=(request:{providerId:string;scope:DownloadScope;locator:string},cursor?:string)=>{edit();console.log('DBG browse',request);void io.request(async()=>{console.log('DBG factory');
  const authenticated=authChoices.current.get(request.providerId)??(await readAuthStatus()).some(p=>p.providerId===request.providerId&&p.configured);
  return {authenticated,listing:await browseDownloads({...request,authenticated,nsfw:true,...(cursor?{cursor}:{})})};
 },value=>{console.log('DBG browse-ok');setAuthenticated(value.authenticated);setListing(previous=>({...value.listing,resources:cursor?[...new Map([...(previous?.resources??[]),...value.listing.resources].map(r=>[key(r),r])).values()]:value.listing.resources}));});};
 const current={providerId,scope,locator};
 const openEntry=(entry:CatalogEntry)=>{sourceEdit();setProviderId(entry.providerId);setScope(entry.scope);setLocator(entry.locator);setEntryId(entry.id);setFolder(entry.target);browse({providerId:entry.providerId,scope:entry.scope,locator:entry.locator});};
 const initialOpened=useRef(false);
 useEffect(()=>{if(!catalog||!suggested||initialOpened.current)return;const entry=catalog.entries.find(e=>e.id===suggested[0]);if(entry){initialOpened.current=true;setCategory(tabOf(entry.target));setBase(entry.base??'');openEntry(entry);}},[catalog,suggested]);
 const openResource=(r:DownloadResource,nextScope:DownloadScope)=>{sourceEdit();setEntryId('');setScope(nextScope);setLocator(r.locator!);if(r.category)setFolder(r.category);browse({providerId,scope:nextScope,locator:r.locator!});};
 const runSearch=()=>{const civitai=catalog?.providers.find(p=>p.kind==='civitai'&&p.scopes.includes('search'));if(!civitai||!search.trim())return;sourceEdit();setEntryId('');setProviderId(civitai.id);setScope('search');setLocator(search.trim());browse({providerId:civitai.id,scope:'search',locator:search.trim()});};
 const resourceDetails=(r:DownloadResource)=><dl className="download-identity"><dt>{t('downloads.identity')}</dt><dd>{r.id}</dd><dt>{t('downloads.category')}</dt><dd>{r.category??r.kind}</dd><dt>{t('downloads.source')}</dt><dd>{r.url??r.locator??t('downloads.unknown')}</dd><dt>{t('downloads.size')}</dt><dd>{r.size??t('downloads.unknown')}</dd><dt>{t('downloads.digest')}</dt><dd>{r.sha256??t('downloads.unknown')}</dd><dt>{t('downloads.license')}</dt><dd>{r.license??t('downloads.unknown')}</dd></dl>;
 const [source,setSource]=useState('');
 // A creator source lists CivitAI models with their own category/base model; group them like the bundled catalog.
 const [sourceCategory,setSourceCategory]=useState('');
 const creatorSources=catalog?.entries.filter(isCreatorSource)??[];
 const visibleEntries=source?[]:(catalog?.entries.filter(e=>tabOf(e.target)===category&&!isCreatorSource(e))??[]).filter(e=>!suggested||suggested.includes(e.id));
 const sourceResources=source?listing?.resources??[]:[];
 const sourceCategories=sortCategories([...new Set(sourceResources.map(r=>tabOf(r.category??'other')))]);
 const activeSourceCategory=sourceCategories.includes(sourceCategory)?sourceCategory:'';
 const bases=[...new Set((source?sourceResources.filter(r=>!activeSourceCategory||tabOf(r.category??'other')===activeSourceCategory):visibleEntries).map(e=>e.base).filter((b):b is string=>!!b))].sort();
 // A remembered family that this tab no longer offers falls back to All.
 const base=bases.includes(savedBase)?savedBase:'';
 // Typing narrows the catalog cards live; Enter additionally searches CivitAI.
 const needle=search.trim().toLowerCase();
 const entries=visibleEntries.filter(e=>(!needle||e.label.toLowerCase().includes(needle))&&(!base||e.base===base)&&(category!=='models'||mediaFilter==='all'||e.media===mediaFilter)).sort(compare);
 const allResources=listing?.resources.filter(r=>(!filenameFilter||(filenameFilter as string[]).includes((r.locator??r.label).split('/').pop()!))&&(!base||!r.base||r.base===base)&&(!source||!activeSourceCategory||tabOf(r.category??'other')===activeSourceCategory)).sort(compare)??[];
 // Files of one model version (quants/formats) are picked together; the first (newest by default) version is preselected.
 const versions=[...new Set(allResources.map(r=>r.version).filter((v):v is string=>!!v))];
 const activeVersion=versions.includes(version)?version:versions[0]??'';
 const resources=allResources.filter(r=>!activeVersion||r.version===activeVersion);
 // Managed install folders receive files in place; a chosen parent (with optional new subfolder) overrides.
 const placeOf=(f:string)=>parentPath?{parentPath,folderName:folderName.trim()}:(installTarget||modelsDir)&&f?{parentPath:comfyFolder(installTarget??'',f,modelsDir),folderName:''}:null;
 const destination=placeOf(folder);
 const picked=(r:DownloadResource)=>basket.some(b=>key(b.resource)===key(r));
 const toggle=(r:DownloadResource)=>{edit();setBasket(old=>picked(r)?old.filter(b=>key(b.resource)!==key(r)):[...old,{resource:r,providerId,authenticated,folder}]);};
 // One native plan per provider, access mode and folder; only the first group creates a new subfolder, later ones stage into it.
 const groups=():Group[]=>{const out=new Map<string,Group>();const made=new Set<string>();
  for(const b of basket){const place=placeOf(b.folder)!;const dir=place.folderName?`${place.parentPath}/${place.folderName}`:place.parentPath;const id=JSON.stringify([b.providerId,b.authenticated,dir]);let g=out.get(id);
   if(!g){g={request:{providerId:b.providerId,resources:[],...(made.has(dir)?{parentPath:dir,folderName:''}:place),authenticated:b.authenticated},keys:[]};made.add(dir);out.set(id,g);}
   g.request.resources.push(b.resource);g.keys.push(key(b.resource));}
  return [...out.values()];};
 // The Download-all click is the explicit authorization; a plan the native side marks non-executable stops the run.
 const startGroups=(next:Group[])=>{if(executionLock.current||!next.length)return;executionLock.current=true;setQueue(next);let published=false;
  void io.request(async()=>{const plan=await planDownloads(next[0].request);return {plan,status:plan.canExecute?await startDownloads(plan.id):null};},({plan,status})=>{if(status){published=true;publishStatus(status);}else setBlocked(plan);}).finally(()=>{if(!published){executionLock.current=false;setQueue([]);}});};
 // A finished group hands over to the next; a failed or cancelled one ends the run and keeps its files selected.
 useEffect(()=>{if(!status||status.state==='running'||!queue.length)return;const [done,...rest]=queue;setQueue([]);if(status.state!=='succeeded')return;setBasket(b=>b.filter(x=>!done.keys.includes(key(x.resource))));startGroups(rest);},[status]); // eslint-disable-line react-hooks/exhaustive-deps
 const where=(b:Picked)=>{const d=placeOf(b.folder);return d?d.folderName||d.parentPath.split(/[\\/]/).pop():'';};
 const canStart=io.native&&!io.busy&&!running&&!queue.length&&basket.length>0&&basket.every(b=>placeOf(b.folder));
 const [present,setPresent]=useState<Set<string>>(new Set());
 const destinationDir=destination?(destination.folderName?`${destination.parentPath}/${destination.folderName}`:destination.parentPath):'';
 // Mark files that already exist in the destination as local; a failed check simply shows nothing.
 useEffect(()=>{setPresent(new Set());if(!io.native||!destinationDir||!listing)return;const names=listing.resources.filter(r=>r.downloadable&&r.locator).map(r=>r.locator!);if(!names.length)return;let alive=true;filesPresent(names.map(n=>`${destinationDir}/${n}`)).then(flags=>{if(alive)setPresent(new Set(names.filter((_,i)=>flags[i])));},()=>{});return()=>{alive=false;};},[io.native,destinationDir,listing]);
 // The file picker opens right under the clicked card; search/advanced listings keep their place below.
 const pickerInline=entries.some(e=>e.id===entryId);
 // Loading and failure of the clicked card's listing show in its picker, not at the far end of the panel.
 const browsingInline=(pickerInline||!!source)&&!listing&&(io.busy||io.error);
 const picker=(listing||browsingInline)&&<div className="download-picker">
    {browsingInline&&(io.busy?<p role="status"><span className="aio-spinner" aria-hidden="true"/> {t('downloads.pending')}</p>:<p role="alert">{t('downloads.browseError')}</p>)}
    {versions.length>0&&<div className="download-categories download-media-filter download-sort" role="group" aria-label={t('downloads.version')}><span aria-hidden="true">{t('downloads.version')}</span>{versions.map(v=><button key={v} type="button" aria-pressed={activeVersion===v} disabled={locked} onClick={()=>setVersion(v)}>{v}</button>)}</div>}
    {listing&&!resources.length&&<p>{t('downloads.empty')}</p>}
    {resources.map(r=><div className={`download-resource${picked(r)?' is-selected':''}${r.locator&&present.has(r.locator)?' is-local':''}`} key={key(r)}><Preview src={r.preview} alt=""/><div>{needsLogin(r.auth,providerId)&&<em className="download-auth">{t('downloads.authNeeded')}</em>}{r.nsfw&&<em className="download-auth download-nsfw-tag" title={t('downloads.nsfwHelp')}>{t('downloads.nsfwTag')}</em>}{r.downloadable&&r.locator&&present.has(r.locator)?<p className="download-local"><span aria-hidden="true">✓</span> {r.label} · {t('downloads.local')}</p>:r.downloadable?<label className="download-check"><input type="checkbox" disabled={locked} checked={picked(r)} onChange={()=>toggle(r)}/><span>{r.label}{r.variant&&<small className="download-variant">{r.variant}</small>}</span></label>:r.kind==='repo'&&r.locator&&provider?<button className="action-button" disabled={io.busy||locked} onClick={()=>openResource(r,provider.kind==='civitai'?'model':'repo')}>{t('downloads.openRepo',{name:r.label})}</button>:<p><strong>{r.label}</strong> · {t('downloads.backendResource')}</p>}<details><summary>{t('downloads.resourceDetails')}</summary>{resourceDetails(r)}</details></div></div>)}
    {listing?.nextCursor&&<button className="action-button" disabled={io.busy||running} onClick={()=>browse(current,listing.nextCursor!)}>{t('downloads.next')}</button>}
    {listing&&<p className="field-help">{t('downloads.more')}</p>}{listing?.warnings.map((w,i)=><p key={i}>{w}</p>)}
 </div>;
 return <section className="feature-panel download-module" aria-label={t('downloads.title')}>
  {!embedded&&<NeuralBlob id="optional-downloads" label={t('downloads.title')} description={t('downloads.bubble')} selected={open} onSelect={()=>{if(locked)return;setOpened(v=>!v);if(!open&&!catalog)load();}}/>}
  {open&&<div className="download-controls" aria-busy={io.busy}>
   <div className="download-head">
    <div className="studio-tabs" role="tablist" aria-label={t('downloads.view')}>{views.map(v=><button key={v} role="tab" type="button" data-library-tab={v} aria-selected={view===v} disabled={locked||(v==='installed'&&!modelsRoot)} onClick={()=>selectView(v)}>{t(`downloads.view.${v}`)}</button>)}</div>
    <button type="button" className="action-button download-start-all" disabled={!canStart} onClick={()=>{edit();startGroups(groups());}}>{t('downloads.startAll',{count:basket.length})}</button>
   </div>
   <p className="field-help">{t(io.native?'downloads.help':'downloads.browser')}</p>
   {catalog&&<>
    {view==='installed'&&<InstalledModels root={modelsRoot} extraModels={extraModels} locked={locked} category={category} setCategory={setCategory}/>}
    {view==='catalog'&&<>
    <div className="download-filter-row"><div className="download-filters">
    <div className="download-source-row"><label className="download-source">{t('downloads.catalogSource')}<select aria-label={t('downloads.catalogSource')} value={source} disabled={locked} data-download-source onChange={e=>{const entry=creatorSources.find(c=>c.id===e.target.value);setSource(entry?.id??'');if(entry)openEntry(entry);else{sourceEdit();setEntryId('');}}}><option value="">{BUNDLED_SOURCE}</option>{creatorSources.map(c=><option key={c.id} value={c.id}>{c.locator}</option>)}</select></label><form className="download-search" onSubmit={e=>{e.preventDefault();runSearch();}}><label>{t('downloads.search')}<input data-download-search aria-label={t('downloads.search')} value={search} maxLength={120} disabled={locked} onChange={e=>setSearch(e.target.value)}/></label><button className="download-search-go" type="submit" aria-label={t('downloads.searchRun')} title={t('downloads.searchRun')} disabled={io.busy||locked||!search.trim()||!catalog.providers.some(p=>p.kind==='civitai')}><span aria-hidden="true">↵</span></button></form></div>
    {!source&&<div className="download-categories" role="tablist" aria-label={t('downloads.category')}>{categories(catalog).map(c=><button key={c} role="tab" type="button" aria-selected={category===c} disabled={locked} onClick={()=>{sourceEdit();setEntryId('');setCategory(c);}}>{t(`downloads.folder.${c}`)}</button>)}</div>}
    {source&&sourceCategories.length>1&&<div className="download-categories" role="tablist" aria-label={t('downloads.category')} data-source-categories><button role="tab" type="button" aria-selected={!activeSourceCategory} disabled={locked} onClick={()=>setSourceCategory('')}>{t('downloads.media.all')}</button>{sourceCategories.map(c=><button key={c} role="tab" type="button" aria-selected={activeSourceCategory===c} disabled={locked} onClick={()=>setSourceCategory(c)}>{t(`downloads.folder.${c}`)}</button>)}</div>}
    {bases.length>1&&<div className="download-categories download-media-filter" role="group" aria-label={t('downloads.base')}><button type="button" aria-pressed={!base} disabled={locked} onClick={()=>setBase('')}>{t('downloads.media.all')}</button>{bases.map(b=><button key={b} type="button" aria-pressed={base===b} disabled={locked} onClick={()=>setBase(b)}>{b}</button>)}</div>}
    {!source&&category==='models'&&<div className="download-categories download-media-filter" role="group" aria-label={t('downloads.media')}>{(['all','image','video'] as const).map(m=><button key={m} type="button" aria-pressed={mediaFilter===m} disabled={locked} onClick={()=>setMediaFilter(m)}>{t(`downloads.media.${m}`)}</button>)}</div>}
    <div className="download-categories download-media-filter download-sort" role="group" aria-label={t('downloads.sort')}><span aria-hidden="true">{t('downloads.sort')}</span>{(['newest','name'] as const).map(by=><button key={by} type="button" aria-pressed={sort.by===by} disabled={locked} onClick={()=>sortBy(by)}>{t(`downloads.sort.${by}`)} <span aria-hidden="true">{(sort.by===by?sort.dir:by==='newest'?-1:1)===-1?'↓':'↑'}</span></button>)}</div>
    </div><section className="download-basket" aria-label={t('downloads.basket')}><h3>{t('downloads.basket')} · {basket.length}</h3><ul>{basket.map(b=><li key={key(b.resource)}><span>{b.resource.label}</span><small>{where(b)}</small><button type="button" className="download-link" aria-label={t('downloads.remove',{name:b.resource.label})} disabled={locked||queue.length>0} onClick={()=>{edit();setBasket(old=>old.filter(x=>x!==b));}}>×</button></li>)}</ul></section></div>
    {source?picker:<div className="download-cards">{entries.map(e=>{const platform=catalog.providers.find(p=>p.id===e.providerId)?.label??e.providerId;return <Fragment key={e.id}><button type="button" className="download-card" aria-pressed={entryId===e.id} disabled={locked||io.busy} onClick={()=>openEntry(e)}><Preview src={e.preview} alt=""/>{needsLogin(e.auth,e.providerId)&&<em className="download-auth">{t('downloads.authNeeded')}</em>}{e.nsfw&&<em className="download-auth download-nsfw-tag" title={t('downloads.nsfwHelp')}>{t('downloads.nsfwTag')}</em>}<em className="download-auth download-platform" aria-hidden="true" title={platform}>{platform}</em><span>{e.label}</span></button>{entryId===e.id&&picker}</Fragment>;})}</div>}
    {!source&&!entries.length&&<p className="field-help">{t('downloads.emptyCategory')}</p>}
    <details><summary>{t('downloads.advanced')}</summary><div className="download-controls">
     <p className="field-help">{t('downloads.safety')}</p>
     <button className="action-button" disabled={!io.native||io.busy||running} onClick={load}>{t('downloads.catalog')}</button>
     <details><summary>{t('downloads.importTitle')}</summary><p className="field-help">{t('downloads.importHelp')}</p><label>{t('downloads.catalogText')}<textarea aria-label={t('downloads.catalogText')} value={catalogText} maxLength={1048576} disabled={!io.native||io.busy||running} onChange={e=>setCatalogText(e.target.value)}/></label><button className="action-button" disabled={!io.native||io.busy||running||!catalogText.trim()} onClick={()=>{edit();void io.request(()=>importDownloadCatalog(catalogText),value=>{applyCatalog(value);setCatalogText('');});}}>{t('downloads.import')}</button></details>
     <label>{t('downloads.provider')}<select aria-label={t('downloads.provider')} value={providerId} disabled={locked} onChange={e=>{sourceEdit();setProviderId(e.target.value);setScope(catalog.providers.find(p=>p.id===e.target.value)?.scopes[0]??'index');setLocator('');setEntryId('');}}>{catalog.providers.map(p=><option key={p.id} value={p.id}>{p.label}</option>)}</select></label>
     <p className="download-identity">{provider?.origin}</p><p className="field-help">{provider?.anonymousNotice}</p>
     <label>{t('downloads.authMode')}<select aria-label={t('downloads.authMode')} value={String(authenticated)} disabled={locked} onChange={e=>{authChoices.current.set(providerId,e.target.value==='true');sourceEdit();setAuthenticated(e.target.value==='true');}}><option value="false">{t('downloads.anonymous')}</option><option value="true">{t('downloads.authenticated')}</option></select></label>
     <label>{t('downloads.curated')}<select aria-label={t('downloads.curated')} value={entryId} disabled={locked} onChange={e=>{sourceEdit();setEntryId(e.target.value);const entry=catalog.entries.find(r=>r.id===e.target.value);if(entry){setScope(entry.scope);setLocator(entry.locator);setFolder(entry.target);}}}><option value="">{t('downloads.custom')}</option>{catalog.entries.filter(e=>e.providerId===providerId).map(e=><option key={e.id} value={e.id}>{e.label}</option>)}</select></label>
     <label>{t('downloads.mode')}<select aria-label={t('downloads.mode')} disabled={locked} value={scope} onChange={e=>{sourceEdit();setEntryId('');setScope(e.target.value as DownloadScope);}}>{provider?.scopes.map(s=><option key={s} value={s}>{t(`downloads.scope.${s}`)}</option>)}</select></label>
     <label>{t('downloads.query')}<input aria-label={t('downloads.query')} value={locator} disabled={locked} onChange={e=>{sourceEdit();setEntryId('');setLocator(e.target.value);}}/></label>
     <button className="action-button" disabled={io.busy||running||!locator.trim()||!provider} onClick={()=>browse(current)}>{t('downloads.list')}</button>
    </div></details>
    {!pickerInline&&!source&&picker}
    {destination&&<p>{t('downloads.destination')}: <span className="download-identity">{destination.folderName?`${destination.parentPath}/${destination.folderName}`:destination.parentPath}</span></p>}
    {/* Managed environments route files to the Settings models folder; the picker is only needed without one (external ComfyUI). */}
    {!installTarget&&<button className="action-button" disabled={io.busy||running} onClick={()=>void io.request(()=>pickDownloadParent(t('downloads.pick')),path=>{if(path){edit();setParentPath(path);}})}>{t('downloads.pick')}</button>}
    {parentPath&&<label>{t('downloads.subfolder')}<input aria-label={t('downloads.subfolder')} value={folderName} disabled={locked} onChange={e=>{edit();setFolderName(e.target.value);}}/></label>}
    </>}
   </>}
   {view==='catalog'&&<>{blocked&&<div className="download-plan"><p role="alert">{t('downloads.blocked')}</p><p className="download-identity">{blocked.target}</p>{blocked.warnings.map((w,i)=><p key={i}>{w}</p>)}</div>}
   <div className="aio-actions"><button className="action-button" disabled={!io.native||io.busy} onClick={()=>void io.request(readDownloadStatus,value=>{setBlocked(null);publishStatus(value);})}>{t('downloads.refresh')}</button>{running&&<button className="action-button" disabled={io.busy} onClick={()=>void io.request(cancelDownloads,publishStatus)}>{t('downloads.cancel')}</button>}</div>
   {io.busy&&!browsingInline&&<p role="status">{t('downloads.pending')}</p>}{status&&<div><p role="status">{t(`downloads.state.${status.state}`)}</p><p>{t('downloads.bytes')}: <output>{status.bytesDownloaded}</output> · {status.completed}/{status.total}</p>{status.error&&<p role="alert">{t('downloads.error')}: {status.error}</p>}</div>}{io.error&&!browsingInline&&<p role="alert">{t('downloads.error')}</p>}</>}
  </div>}
 </section>;
}
function categories(catalog:DownloadCatalog):string[] {
 return sortCategories([...new Set(catalog.entries.map(e=>tabOf(e.target)))]);
}
function sortCategories(found:string[]):string[] {
 return found.sort((a,b)=>(categoryOrder.indexOf(a)+1||99)-(categoryOrder.indexOf(b)+1||99)||a.localeCompare(b));
}
/** Compatibility export for existing consumers; auth is intentionally separate. */
export const DownloadPluginPanel=DownloadPanel;
