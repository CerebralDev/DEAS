import {useEffect,useState} from 'react';
import './models.css';
import {invoke,isTauri} from '@tauri-apps/api/core';
import {useI18n} from '../../i18n/I18nProvider';
import {oneOf,usePref} from '../../lib/preferences';
import {readDownloadCatalog,type DownloadCatalog,type CatalogEntry,type DownloadStatus} from '../../lib/downloads';
import type {LocalModel} from '../../lib/models';
import {MODEL_GUIDES,baseGuide,installedGuide,matchesGuide,type ModelGuide} from './modelGuides';
import {BeginnerModelDownload} from './BeginnerModelDownload';
import {DownloadPanel} from '../downloads/DownloadPluginPanel';
import {catalogModels,filterInstalled,modelFamily,installedMedia,type ModelMedia} from './modelCatalog';

export function BeginnerModels({models,root,modelsDir,onUse,onRefresh,reason,loading}:{models:LocalModel[];root?:string;modelsDir?:string;onUse:(model:LocalModel)=>void;onRefresh:()=>void;reason:(model:LocalModel)=>string;loading:boolean}) {
 const {t}=useI18n();const [query,setQuery]=useState('');
 const cleanName=(name:string)=>(name.split(/[\\/]/).pop()??name).replace(/\.(safetensors|sft|gguf|ckpt|pth|pt)$/i,'').replace(/_/g,' ');
 const matches=(name:string,guide?:ModelGuide)=>matchesGuide(query,name,guide);
 const [guideDownload,setGuideDownload]=useState<ModelGuide|null>(null);
 const [tab,setTab]=usePref('beginner.models.tab','installed',oneOf('installed','explore','all'));
 const [media,setMedia]=usePref<ModelMedia>('beginner.models.media','all',oneOf('all','image','video'));
 const [catalog,setCatalog]=useState<DownloadCatalog|null>(null),[error,setError]=useState(false),[revision,setRevision]=useState(0);
 const [entry,setEntry]=useState<CatalogEntry|null>(null),[preview,setPreview]=useState<{src:string;name:string}|null>(null),[downloading,setDownloading]=useState(false);
 useEffect(()=>{if(tab==='installed'||!isTauri())return;let live=true;setError(false);
  readDownloadCatalog().then(value=>{if(live)setCatalog(value);},()=>{if(live)setError(true);void invoke('native_logs_beginner_error',{operation:'catalog'}).catch(()=>{});});
  return()=>{live=false;};
 },[tab,revision]);
 const status=(value:DownloadStatus)=>{setDownloading(value.state==='running');if(value.state==='succeeded')onRefresh();};
 const installed=filterInstalled(models,media).filter(m=>matches(m.name+' '+(m.info?.civitai?.name??''),installedGuide(m)));const offered=catalogModels(catalog?.entries??[],media).filter(e=>matches(e.label));
 const featured=media==='video'?[]:MODEL_GUIDES.filter(g=>matches(g.name,g));
 const isDownload=!!entry||!!guideDownload;
 const tags=(guide:ModelGuide)=><div className="beginner-model-tags" aria-label={t('beginner.tags')}>{guide.tags.map(tag=><button key={tag} type="button" aria-label={t('beginner.filterTag',{tag})} onClick={()=>setQuery(tag)}>{tag}</button>)}</div>;
 const guideCopy=(guide:ModelGuide,familyOnly=false)=><>{familyOnly&&<small className="beginner-family-guide">{t('beginner.familyGuide')}</small>}<p>{guide.summary}</p>{tags(guide)}<div className="beginner-prompt-guide"><h4>{t('beginner.howToPrompt')}</h4><p>{guide.prompting}</p></div><p className="beginner-model-note">{guide.note}</p></>;
 // The native file scan and the download catalog have different identities. Do not claim a
 // remote pack is installed merely because its label resembles a local filename.
 const image=(src:string|undefined,name:string,credit?:string)=><div className="beginner-model-visual">{src?<button type="button" className="beginner-model-image" aria-label={t('beginner.enlarge',{name})} onClick={()=>setPreview({src,name})}><img src={src} alt={name} loading="lazy"/></button>:<div className="beginner-model-placeholder"><span className="beginner-cover-orbit" aria-hidden="true"/><strong>{cleanName(name).slice(0,40)}</strong><small>{t('beginner.noPreview')}</small></div>}{credit&&<small className="beginner-image-credit">{credit}</small>}</div>;
 return <section className="beginner-models" aria-label={t('beginner.models')}>
  <div className="beginner-page-heading"><div><p className="eyebrow">{"DEAS · "}{t('beginner.models')}</p><h1>{t('beginner.modelsTitle')}</h1><p>{t('beginner.modelsHint')}</p></div><button type="button" disabled={loading} onClick={onRefresh}>{t('beginner.refresh')}</button></div>
  <div className="beginner-model-toolbar"><div role="tablist" aria-label={t('beginner.modelSource')}>{(['installed','explore','all'] as const).map(id=><button key={id} role="tab" type="button" aria-selected={tab===id} onClick={()=>setTab(id)}>{t(`beginner.source.${id}`)}</button>)}</div><div role="group" aria-label={t('beginner.mediaFilter')}>{(['all','image','video'] as const).map(id=><button key={id} type="button" aria-pressed={media===id} onClick={()=>setMedia(id)}>{t(`beginner.media.${id}`)}</button>)}</div></div>
  <label className="beginner-model-search"><span aria-hidden="true">⌕</span><input type="search" aria-label={t('beginner.searchModels')} placeholder={t('beginner.searchModels')} value={query} onChange={e=>setQuery(e.target.value)}/><small>{tab==='installed'?installed.length:tab==='explore'?featured.length+offered.length:installed.length+featured.length+offered.length} {t('beginner.results')}</small></label>
  <p className="beginner-search-hint">{t('beginner.tagHint')}</p>
  {isDownload&&<div className="beginner-download"><button type="button" disabled={downloading} onClick={()=>{setEntry(null);setGuideDownload(null);}}>← {t('beginner.backModels')}</button><h2>{guideDownload?.name??entry?.label}</h2><p>{t('beginner.downloadHint')}</p>{guideDownload?<BeginnerModelDownload key={guideDownload.id} guide={guideDownload} root={root} modelsDir={modelsDir} onStatus={status}/>:entry&&<DownloadPanel key={entry.id} embedded installTarget={root} modelsDir={modelsDir} initialEntryId={entry.id} onStatus={status}/>}</div>}
  <div hidden={isDownload}>
   {tab!=='explore'&&<><h2>{t('beginner.source.installed')}</h2>{loading?<p role="status">{t('beginner.loading')}</p>:!installed.length?<p>{t('beginner.noInstalled')}</p>:<div className="beginner-model-grid">{installed.map(m=>{const family=modelFamily(m),blocked=reason(m),guide=installedGuide(m);return <article className="beginner-model-card" key={m.path||m.category+'/'+m.name}>
    {image(m.info?.civitai?.preview??guide?.image,m.name,m.info?.civitai?.preview?undefined:guide?.credit)}<div className="beginner-model-copy"><small className="beginner-installed">● {t('beginner.source.installed')}</small><h3>{m.info?.civitai?.name??cleanName(m.name)}</h3>{guide?guideCopy(guide,!baseGuide(m)):<p>{family?t('beginner.capabilities',{name:family.name,modes:family.pipelines.map(p=>t(`studio.run.pipe.${p}`)).join(' · ')}):t('beginner.unknownFamily')}</p>}<small>{installedMedia(m)?t(`beginner.media.${installedMedia(m)}`):t('beginner.unknownMedia')} · {(m.size/1073741824).toFixed(1)} {"GiB"}</small><details className="beginner-model-filename"><summary>{t('beginner.fileDetails')}</summary><small>{m.name}</small></details>{blocked&&<p className="field-help">{blocked}</p>}<button type="button" className="aio-confirm-button" disabled={!!blocked} title={blocked||undefined} onClick={()=>onUse(m)}>{t('beginner.useModel')} →</button></div>
   </article>;})}</div>}</>}
   {tab!=='installed'&&<><h2>{t('beginner.featuredModels')}</h2><div className="beginner-model-grid" data-featured-models>{featured.map(guide=>{const local=models.find(m=>baseGuide(m)?.id===guide.id),blocked=local?reason(local):'';return <article className="beginner-model-card" data-guide={guide.id} key={guide.id}>
    {image(guide.image,guide.name,guide.credit)}<div className="beginner-model-copy"><small className="beginner-model-version">{guide.id==='moss-sft'?'SFT · OpenMOSS':'BASE MODEL'}</small><h3>{guide.name}</h3>{guideCopy(guide)}<div className="beginner-model-actions">{local?<button type="button" className="aio-confirm-button" disabled={!!blocked} title={blocked||undefined} onClick={()=>onUse(local)}>{t('beginner.useModel')} →</button>:<small>{t('beginner.notInstalled')}</small>}<button type="button" onClick={()=>setGuideDownload(guide)}>{t('beginner.downloadModel')} ↓</button></div><details className="beginner-model-filename"><summary>{t('beginner.sourceCredits')}</summary><p>{guide.credit}</p><a href={guide.source} target="_blank" rel="noreferrer">{guide.source}</a></details></div>
   </article>;})}</div>{!featured.length&&<p>{t('beginner.noCatalog')}</p>}</>}
   {tab!=='installed'&&<><h2>{t('beginner.moreModels')}</h2><p className="field-help">{t('beginner.catalogHint')}</p>{error?<p role="alert">{t('beginner.catalogError')} <button type="button" onClick={()=>setRevision(r=>r+1)}>{t('beginner.retry')}</button></p>:!catalog?<p role="status">{t('beginner.loading')}</p>:!offered.length?<p>{t('beginner.noCatalog')}</p>:<div className="beginner-model-grid">{offered.map(e=><article className="beginner-model-card" key={e.id}>{image(e.preview,e.label)}<div className="beginner-model-copy"><small>{e.media?t(`beginner.media.${e.media}`):t('beginner.unknownMedia')}</small><h3>{e.label}</h3><p>{t('beginner.packDescription',{name:e.base??e.label})}</p>{e.auth&&<p className="field-help">{t('beginner.authHint')}</p>}<button type="button" onClick={()=>setEntry(e)}>{t('beginner.downloadModel')} ↓</button></div></article>)}</div>}</>}
  </div>
  {preview&&<PreviewDialog src={preview.src} name={preview.name} onClose={()=>setPreview(null)}/>}
 </section>;
}

function PreviewDialog({src,name,onClose}:{src:string;name:string;onClose:()=>void}){
 const {t}=useI18n();const [dialog,setDialog]=useState<HTMLDialogElement|null>(null);
 useEffect(()=>{if(dialog&&typeof dialog.showModal==='function')dialog.showModal();},[dialog]);
 return <dialog ref={setDialog} className="beginner-preview-dialog" open={typeof HTMLDialogElement==='undefined'||!HTMLDialogElement.prototype.showModal?true:undefined} onCancel={onClose}><button type="button" autoFocus onClick={onClose}>{t('beginner.close')} ×</button><img src={src} alt={name}/><p>{name}</p></dialog>;
}
