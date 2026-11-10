import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { applyStudioSettings, captureStudioSettings } from '../../lib/preferences';
import { useI18n } from '../../i18n/I18nProvider';
import { captureWorkspaceContext, settingsFingerprint, type WorkspaceSnapshot } from './context';

/** Pure authoring handoff. No generation API is reachable from this component. */
export function Suggestion({proposed,context}:{proposed:Record<string,string|number>;context?:string|null}){
 const {t}=useI18n();const [review,setReview]=useState<{before:Record<string,unknown>;fingerprint:string}|null>(null);
 const [undo,setUndo]=useState<{before:Record<string,unknown>;fingerprint:string}|null>(null);
 const [error,setError]=useState(''),[busy,setBusy]=useState(false);
 let snapshot:WorkspaceSnapshot|null=null;try{snapshot=context?JSON.parse(context):null;}catch{/* Older messages have no snapshot. */}
 const start=()=>{const before=captureStudioSettings();setReview({before,fingerprint:settingsFingerprint(before)});setError('');};
 const apply=async()=>{
  if(!review||busy)return;setBusy(true);setError('');
  try{
   const current=captureStudioSettings();if(settingsFingerprint(current)!==review.fingerprint)throw new Error(t('workspace.suggestionStale'));
   const allowed=await invoke<Record<string,string|number>>('native_workspace_validate_suggestion',{proposed,pipeline:String(current['studio.run.pipeline']??'')});
   if(settingsFingerprint(captureStudioSettings())!==review.fingerprint)throw new Error(t('workspace.suggestionStale'));
   const patch=Object.fromEntries(Object.entries(allowed).map(([k,v])=>[`studio.run.${k}`,v]));
   if(!applyStudioSettings(patch))throw new Error(t('workspace.suggestionInvalid'));
   // React commits transfer listeners after this turn. Capture the expected values for guarded Undo.
   setUndo({before:Object.fromEntries(Object.keys(patch).map(k=>[k,current[k]])),fingerprint:settingsFingerprint({...current,...patch})});setReview(null);
  }catch(e){setError(String(e));void invoke('native_logs_workspace_error',{operation:'suggestion'}).catch(()=>{});}
  finally{setBusy(false);}
 };
 return <section className="workspace-proposal" aria-label={t('workspace.suggestion')}>
  <strong>{t('workspace.suggestion')}</strong><p>{t('workspace.suggestionHelp')}</p>
  {snapshot&&<details><summary>{t('workspace.originalContext')}</summary><pre>{JSON.stringify(snapshot.values,null,2)}</pre></details>}
  <pre>{JSON.stringify(proposed,null,2)}</pre>
  {<button title={t('workspace.reviewHelp')} onClick={start}>{t('workspace.reviewChanges')}</button>}
  {review&&<><span>{t('workspace.currentValues')}</span><pre>{JSON.stringify(Object.fromEntries(Object.keys(proposed).map(k=>[k,review.before[`studio.run.${k}`]])),null,2)}</pre><button disabled={busy} title={t('workspace.suggestionApplyHelp')} onClick={()=>void apply()}>{t('forge.apply')}</button></>}
  {undo&&<button title={t('workspace.undoHelp')} onClick={()=>{if(settingsFingerprint(captureStudioSettings())!==undo.fingerprint){setError(t('workspace.suggestionStale'));return;}if(applyStudioSettings(undo.before)){setUndo(null);setError('');}}}>{t('forge.undo')}</button>}
  {error&&<p role="alert">{error}</p>}
 </section>;
}

export function ContextChip({area,enabled,onEnabled}:{area:string;enabled:boolean;onEnabled:(v:boolean)=>void}){
 const {t}=useI18n();const [preview,setPreview]=useState('');
 return <div className="chat-context"><label title={t('workspace.contextHelp')}><input type="checkbox" checked={enabled} onChange={e=>onEnabled(e.target.checked)}/>{t('workspace.context')}: {enabled?t(`workspace.area.${area}`):t('workspace.off')}</label>
  <details onToggle={e=>{if(e.currentTarget.open)setPreview(JSON.stringify(captureWorkspaceContext(area),null,2));}}><summary>{t('workspace.previewContext')}</summary><p>{t('workspace.contextHelp')}</p><button onClick={()=>setPreview(JSON.stringify(captureWorkspaceContext(area),null,2))}>{t('workspace.refreshPreview')}</button><pre>{enabled?preview:t('workspace.off')}</pre></details>
 </div>;
}
