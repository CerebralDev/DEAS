import { useEffect, useRef, useState } from 'react';
import { useI18n } from '../../i18n/I18nProvider';
import { availableSchemes, useScheme, useThemeCatalog,refreshThemePacks } from '../../lib/theme';
import './quickSettings.css';
import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core';
import { DICTIONARY_EVENT, reportPromptSpellcheckError, usePromptTools } from '../../lib/promptTools';
import { ExperienceSwitch, type Experience } from '../beginner/Experience';

export function QuickSettings({root,experience='advanced',onExperience}:{root?:string;experience?:Experience;onExperience?:(value:Experience)=>void}={}){
 const themes=useThemeCatalog(root);
 const [folderError,setFolderError]=useState(false);
 const [editor,setEditor]=useState<string|null>(null);
 const [editorOpen,setEditorOpen]=useState(false);
 const editorFrame=useRef<HTMLIFrameElement>(null);
 const tools=usePromptTools();
 // Dictionary status only (read-only, no network); the install lives under Backend → Language.
 const [dictionaries,setDictionaries]=useState(0);
 useEffect(()=>{const bump=()=>setDictionaries(n=>n+1);window.addEventListener(DICTIONARY_EVENT,bump);return()=>window.removeEventListener(DICTIONARY_EVENT,bump);},[]);
 useEffect(()=>{
  if(!isTauri())return;
  let alive=true;reportPromptSpellcheckError('');
  if(!tools.spellcheck)return;
  void invoke<{available:boolean;installed:boolean}>('native_spellcheck_status',{language:tools.language}).then(status=>{if(alive&&typeof status?.installed==='boolean'&&!status.installed)reportPromptSpellcheckError(status.available===true?'settings.prompt.dictionaryMissing':'settings.spellcheck.unavailable');},()=>{if(alive)reportPromptSpellcheckError('settings.prompt.failed');});
  return()=>{alive=false;};
 },[tools.spellcheck,tools.language,dictionaries]);
 const {t}=useI18n();const [open,setOpen]=useState(false);const [scheme,setScheme]=useScheme();const host=useRef<HTMLDivElement>(null),button=useRef<HTMLButtonElement>(null);
 useEffect(()=>{
  if(!open)return;
  const outside=(e:PointerEvent)=>{if(!host.current?.contains(e.target as Node))setOpen(false);};
  const escape=(e:KeyboardEvent)=>{if(e.key==='Escape'){setOpen(false);button.current?.focus();}};
  window.addEventListener('pointerdown',outside);window.addEventListener('keydown',escape);
  return()=>{window.removeEventListener('pointerdown',outside);window.removeEventListener('keydown',escape);};
 },[open]);
 // Studio is lazy-mounted; reset events have no receiver before its first visit.
 const ready=!!document.querySelector('[data-workbench]');
 const request=(event:string)=>{setOpen(false);button.current?.focus();window.dispatchEvent(new Event(event));};
 // The offline editor renders in a frame of its own; it reaches the environment through
 // these bounded native file calls instead of browser folder permissions.
 useEffect(()=>{
  if(!editor||!root)return; // the frame stays mounted while closed so an unfinished theme edit survives
  const frame=editorFrame.current;
  const onMessage=async(event:MessageEvent)=>{
   const request=event.data as {source?:string;id?:string;op?:string;folder?:string;path?:string;base64?:string};
   if(!frame||event.source!==frame.contentWindow||request?.source!=='deas-theme-editor'||!request.id||!request.op)return;
   const reply=(body:Record<string,unknown>)=>frame.contentWindow?.postMessage({source:'deas-theme-host',id:request.id,...body},'*');
   try{
    if(request.op==='folders')reply({folders:await invoke<string[]>('native_theme_folders',{root})});
    else if(request.op==='read')reply({base64:await invoke<string|null>('native_theme_read',{root,folder:request.folder,path:request.path})});
    else if(request.op==='write'){
     await invoke('native_theme_write',{root,folder:request.folder,path:request.path,base64:request.base64});
     reply({written:true});
     void refreshThemePacks(root);
    }else reply({error:'Unknown request'});
   }catch(error){reply({error:error instanceof Error?error.message:String(error)});}
  };
  const escape=(event:KeyboardEvent)=>{if(event.key==='Escape')setEditorOpen(false);};
  window.addEventListener('message',onMessage);
  window.addEventListener('keydown',escape);
  return()=>{window.removeEventListener('message',onMessage);window.removeEventListener('keydown',escape);};
 },[editor,root]);
 return <div className="studio-quick" ref={host}>
  <button type="button" ref={button} className="repo-button" aria-label={t('studio.quick.title')} title={t('studio.quick.title')} aria-expanded={open} aria-controls="studio-quick-actions" onClick={()=>setOpen(!open)}>{t('studio.quick.title')}</button>
  {open&&<div id="studio-quick-actions" className="studio-quick-actions" aria-label={t('studio.quick.title')}>
   {!ready&&<p id="studio-quick-unavailable">{t('studio.quick.openStudioFirst')}</p>}
   {onExperience&&<ExperienceSwitch value={experience} onChange={onExperience}/>}
   <button type="button" disabled={!ready} aria-describedby={!ready?'studio-quick-unavailable':undefined} onClick={()=>request('deas:reset-studio-layout')}>{t('studio.quick.layout')}</button>
   <button type="button" disabled={!ready} aria-describedby={!ready?'studio-quick-unavailable':undefined} onClick={()=>request('deas:request-studio-defaults')}>{t('studio.quick.defaults')}</button>
   {/* Unavailable spellcheck is only greyed here; the explanation lives under Backend. */}
   <button type="button" aria-pressed={tools.spellcheck} disabled={!!tools.error} title={tools.error?t(tools.error):undefined} onClick={()=>tools.update({spellcheck:!tools.spellcheck})}>{t('settings.prompt.spellcheck')}</button>
   <button type="button" aria-pressed={tools.autocomplete} onClick={()=>tools.update({autocomplete:!tools.autocomplete})}>{t('settings.prompt.autocomplete')}</button>
   <div className="studio-quick-schemes" role="group" aria-label={t('studio.quick.scheme')}>
    <span className="studio-quick-label">{t('studio.quick.scheme')}</span>
    {availableSchemes().map(s=><button key={s.id} type="button" data-scheme={s.id} aria-pressed={scheme===s.id} onClick={()=>setScheme(s.id)}>
     <span className="studio-quick-swatch" aria-hidden="true" style={{background:s.swatch[0],boxShadow:`inset -9px 0 0 ${s.swatch[1]}`}}/>{s.name}
    </button>)}
   </div>
   <div className="theme-pack-actions">
    {!root&&<small>{t('theme.ui.chooseEnvironment')}</small>}
    <button type="button" data-theme-action="refresh" disabled={!root||!isTauri()||themes.loading} onClick={()=>void refreshThemePacks(root)}>{t(themes.loading?'theme.ui.loading':'theme.ui.refresh')}</button>
    <button type="button" data-theme-action="edit" disabled={!root||!isTauri()} onClick={()=>{setFolderError(false);void invoke<string>('native_theme_editor',{root}).then(path=>{setEditor(path);setEditorOpen(true);}).catch(()=>setFolderError(true));}}>{t('theme.ui.editThemes')}</button>
    {folderError&&<p role="status">{t('theme.ui.folderFailed')}</p>}
    {themes.directory&&<small className="theme-pack-directory">{themes.directory}</small>}
    {(themes.error||themes.warnings.length>0)&&<p role="status">{t('theme.ui.someThemePacksCouldNotBeLoadedBuilt')}</p>}
    {!availableSchemes().some(s=>s.id===scheme)&&<p role="status">{t('theme.ui.theSavedThemeIsUnavailableDefaultIsShown')}</p>}
   </div>
  </div>}
  {editor&&<div className="theme-editor" role="dialog" aria-modal="true" hidden={!editorOpen} aria-label={t('theme.ui.editThemes')}>
   <div className="theme-editor-bar"><strong>{t('theme.ui.editThemes')}</strong><button type="button" data-theme-action="close" onClick={()=>setEditorOpen(false)}>{t('theme.ui.closeEditor')}</button></div>
   <iframe ref={editorFrame} title={t('theme.ui.editThemes')} src={convertFileSrc(editor)}/>
  </div>}
 </div>;
}
