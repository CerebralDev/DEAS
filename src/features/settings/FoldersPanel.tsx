import { useEffect, useRef, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { useI18n } from '../../i18n/I18nProvider';
import { readFolders, writeFolders, moveFolders, type Folders } from '../../lib/settings';
import { generationActive } from '../../lib/jobs';
import './settings.css';
const keys=['models','input','output'] as const;
/** Shared chooser fields: setup edits an intent; Settings saves or explicitly moves it. */
export function FolderFields({folders,onChange,disabled=false,onBusyChange}:{folders:Folders;onChange:(folders:Folders)=>void;disabled?:boolean;onBusyChange?:(busy:boolean)=>void}) {
 const {t}=useI18n();const lock=useRef(false);const live=useRef(false);
 useEffect(()=>{live.current=true;return()=>{live.current=false;};},[]);const [picking,setPicking]=useState(false);const [error,setError]=useState('');
 const pick=async(key:typeof keys[number])=>{if(lock.current||disabled)return;lock.current=true;setPicking(true);onBusyChange?.(true);setError('');try{const dir=await invoke<string|null>('native_pick_directory',{title:t(`settings.${key}`)});if(dir&&live.current)onChange({...folders,[key]:dir});}catch(e){if(live.current)setError(String(e));}finally{lock.current=false;if(live.current)setPicking(false);onBusyChange?.(false);}};
 const extraModels=folders.extraModels??[];
 const pickExtra=async(index:number)=>{if(lock.current||disabled)return;lock.current=true;setPicking(true);onBusyChange?.(true);setError('');try{const dir=await invoke<string|null>('native_pick_directory',{title:t('settings.extraModel',{index:index+1})});if(dir&&live.current)onChange({...folders,extraModels:extraModels.map((v,i)=>i===index?dir:v)});}catch(e){if(live.current)setError(String(e));}finally{lock.current=false;if(live.current)setPicking(false);onBusyChange?.(false);}};
 return <>{keys.map(key=><div key={key} className="settings-row" data-folder={key} title={key==='models'?t('settings.modelsHelp'):undefined}><span className="settings-label">{t(`settings.${key}`)}</span><code className="download-identity">{folders[key]??t('settings.default')}</code><button type="button" className="action-button" aria-label={t('settings.chooseFor',{folder:t(`settings.${key}`)})} disabled={!isTauri()||disabled||picking} onClick={()=>void pick(key)}>{t('settings.choose')}</button><button type="button" className="action-button" aria-label={t('settings.resetFor',{folder:t(`settings.${key}`)})} disabled={!folders[key]||disabled||picking} onClick={()=>{const next={...folders};delete next[key];onChange(next);}}>{t('settings.reset')}</button></div>)}
 {/* Extra model roots: external folders on other drives, registered beside the primary root; the mover never touches them. */}
 {extraModels.map((dir,index)=><div key={`extra-${index}`} className="settings-row" title={t('settings.modelsHelp')}><span className="settings-label">{t('settings.extraModel',{index:index+1})}</span><code className="download-identity">{dir||t('settings.default')}</code><button type="button" className="action-button" aria-label={t('settings.chooseExtraFor',{index:index+1})} disabled={!isTauri()||disabled||picking} onClick={()=>void pickExtra(index)}>{t('settings.choose')}</button><button type="button" className="action-button" aria-label={t('settings.removeExtraFor',{index:index+1})} disabled={disabled||picking} onClick={()=>onChange({...folders,extraModels:extraModels.filter((_,i)=>i!==index)})}>{t('settings.reset')}</button></div>)}
 <button type="button" className="action-button" disabled={!isTauri()||disabled||picking} onClick={()=>onChange({...folders,extraModels:[...extraModels,'']})}>{t('settings.addExtraModel')}</button>
 {error&&<p role="alert">{error}</p>}</>;
}
/** Before install, draft paths join the immutable installation plan without writing files. */
/** `onRestart` restarts a running owned backend after a save (ComfyUI reads folders only at start); a stopped one stays stopped. */
export function FoldersPanel({root,onChange,draft,disabled=false,onRestart}:{root?:string;onChange?:(folders:Folders)=>void;draft?:Folders;disabled?:boolean;onRestart?:()=>void}) {
 const {t}=useI18n();const native=isTauri();const generation=useRef(0);const lock=useRef(false);
 const [folders,setFolders]=useState<Folders>({});const [saved,setSaved]=useState<Folders>({});const [error,setError]=useState('');const [busy,setBusy]=useState(false);const [picking,setPicking]=useState(false);const [move,setMove]=useState(false);const [report,setReport]=useState<{movedFiles:number;warnings:string[]}|null>(null);const [applied,setApplied]=useState('');
 useEffect(()=>{generation.current++;setFolders({});setSaved({});setError('');setMove(false);setReport(null);if(!native||!root||draft!==undefined)return;let alive=true;setBusy(true);readFolders(root).then(f=>{if(alive){const loaded={...f,extraModels:f.extraModels??[]};setFolders(loaded);setSaved(loaded);onChange?.(loaded);}},e=>{if(alive)setError(String(e));}).finally(()=>{if(alive)setBusy(false);});return()=>{alive=false;};},[native,root,draft!==undefined]); // eslint-disable-line react-hooks/exhaustive-deps
 const save=async()=>{if(!root||lock.current||disabled||picking)return;lock.current=true;setBusy(true);setError('');setReport(null);setApplied('');const version=generation.current;try{const result=move?await moveFolders(root,folders):{folders:await writeFolders(root,folders),movedFiles:0,warnings:[]};if(version===generation.current){setFolders(result.folders);setSaved(result.folders);setMove(false);if(move)setReport(result);onChange?.(result.folders);
   if(onRestart){if(await generationActive())setApplied(t('startup.jobRunning'));else{onRestart();setApplied(t('settings.applied'));}}}}catch(e){if(version===generation.current)setError(String(e));}finally{lock.current=false;if(version===generation.current)setBusy(false);}};
 const dirty=JSON.stringify(folders)!==JSON.stringify(saved);
 return <section className="feature-panel download-module" aria-label={t('settings.title')}><div className="download-controls">
  <p className="field-help">{t(!native?'downloads.browser':!root?'settings.noRoot':draft!==undefined?'settings.draftHelp':'settings.help')}</p>
  {root&&<FolderFields folders={draft??folders} onChange={next=>{setReport(null);if(draft!==undefined)onChange?.(next);else setFolders(next);}} disabled={disabled||busy} onBusyChange={setPicking}/>}
  {root&&draft===undefined&&<><label className="download-check"><input type="checkbox" checked={move} disabled={disabled||busy||picking} onChange={e=>setMove(e.target.checked)}/>{t('settings.move')}</label><p className="field-help">{t('settings.moveHelp')}</p><div className="aio-actions"><button type="button" className="action-button" disabled={!native||!dirty||busy||picking||disabled} onClick={()=>void save()}>{t(move?'settings.moveSave':'settings.save')}</button></div></>}
  {applied&&<p role="status" data-folders-applied>{applied}</p>}{report&&<div role="status"><p>{t('settings.moved',{count:report.movedFiles})}</p>{report.warnings.map((w,i)=><p key={i}>{w}</p>)}</div>}{error&&<p role="alert">{error}</p>}
 </div></section>;
}
