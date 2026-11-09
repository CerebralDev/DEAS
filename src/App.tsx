import { useCallback, useEffect, useRef, useState } from 'react';
import { backendErrorText } from './lib/backendErrors';
import { useI18n } from './i18n/I18nProvider';
import { inspectBackend, type BackendSnapshot } from './lib/comfy';
import { builtinPlugins } from './lib/plugins';
import { NeuralGraph, type NeuralNode, type NeuralEdge } from './components/neural/NeuralGraph';
import { AiOSetup } from './components/setup/AiOSetup';
import { ApiNegotiationPanel } from './features/studio/ApiNegotiationPanel';
import { RecipePrerequisitesPanel } from './features/studio/RecipePrerequisitesPanel';
import { Workbench } from './features/studio/Workbench';
import { TrueVideoEnhancer } from './features/studio/TrueVideoEnhancer';
import { Monitoring } from './features/monitoring/Monitoring';
import { PromptForge } from './features/forge/PromptForge';
import { Chat } from './features/chat/Chat';
import { Analysis } from './features/analysis/Analysis';
import type { MediaItem } from './lib/media';
import type { MediaAction } from './features/media/MediaLibrary';
import type { Folders } from './lib/settings';
import type { ChatRenderOutcome, ChatRenderRequest, ChatStudio } from './lib/chat';
import type { ForgeCanvasContext, ForgeSendCanvas } from './lib/forgeCanvas';
import { oneOf, usePref } from './lib/preferences';
import {type Experience,type BeginnerPage} from './features/beginner/Experience';
const studioTabs=['generate','forge','chat','analysis','tve','monitor','map'] as const;
const presentation: Record<string,{column:number;order:number}>={engine:{column:0,order:0},director:{column:1,order:0},plugins:{column:1,order:1},forge:{column:2,order:0},refmods:{column:2,order:1},continuity:{column:2,order:2},output:{column:2,order:3}};
interface DirectorDraft {direction:string;framing:string;mood:string}
const defaultDraft:DirectorDraft={direction:'',framing:'wide',mood:'natural'};
const draftKey='deas.director-draft.v1';
const framing=['wide','medium','close'];
const moods=['natural','copper','blue','studio'];
// Existing stored enum values are a migration schema, never UI fallback text.
const legacyEnums:Record<string,string>={'Wide shot':'wide','Medium shot':'medium','Close-up':'close','Natural light':'natural','Copper dusk':'copper','Blue hour':'blue','Studio light':'studio'};
function readDraft():DirectorDraft {
 try{const v=JSON.parse(localStorage.getItem(draftKey)??'null');
  if(v&&typeof v.direction==='string'&&typeof v.framing==='string'&&typeof v.mood==='string'){
   const f=legacyEnums[v.framing]??v.framing,m=legacyEnums[v.mood]??v.mood;
   return {direction:v.direction.slice(0,8000),framing:framing.includes(f)?f:'wide',mood:moods.includes(m)?m:'natural'};
  }
 }catch{/* Storage may be unavailable. */}
 return {...defaultDraft};
}
export default function App(){
 const {t}=useI18n();
 const [experience,setExperience]=usePref<Experience>('interface.experience','unset',oneOf('unset','beginner','advanced'));
 const [beginnerPage,setBeginnerPage]=useState<BeginnerPage>('generate');
 const beginner=experience==='beginner';
 const [active,setActive]=useState<string|null>(null);
 const [endpoint,setEndpoint]=useState('http://127.0.0.1:7801');
 const [snapshot,setSnapshot]=useState<BackendSnapshot|null>(null);
 const [inspecting,setInspecting]=useState(false);
 const [inspectionError,setInspectionError]=useState('');
 const inspectionVersion=useRef(0);
 async function inspect(){
  if(inspecting)return;const version=++inspectionVersion.current;setInspecting(true);setSnapshot(null);setInspectionError('');
  try{const result=await inspectBackend(endpoint);if(version===inspectionVersion.current)setSnapshot(result);}
  catch(e){if(version===inspectionVersion.current)setInspectionError(e instanceof Error?e.message:String(e));}
  finally{if(version===inspectionVersion.current)setInspecting(false);}
 }
 const [draft,setDraft]=useState<DirectorDraft>(readDraft);
 const [draftStatus,setDraftStatus]=useState('');
 function updateDraft(patch:Partial<DirectorDraft>){setDraft(d=>({...d,...patch}));setDraftStatus('studio.draft.unsaved');}
 function saveDraft(){try{localStorage.setItem(draftKey,JSON.stringify(draft));setDraftStatus('studio.draft.saved');}catch{setDraftStatus('studio.draft.saveError');}}
 function closePanel(){document.querySelector<HTMLButtonElement>(`[data-node="${active}"] button`)?.focus();setActive(null);}
 const [tab,setTab]=usePref('studio.tab','generate',oneOf(...studioTabs));
 const [studioActive,setStudioActive]=useState(false);
 // ComfyUI's own input/output live beside the managed checkout unless Settings redirected them.
 const [environment,setEnvironment]=useState<{root:string;folders:Folders}|null>(null);
 const roots=environment?{input:environment.folders.input??`${environment.root}/ComfyUI/input`,output:environment.folders.output??`${environment.root}/ComfyUI/output`,models:environment.folders.models??`${environment.root}/ComfyUI/models`,extraModels:environment.folders.extraModels??[]}:undefined;
 // Prompt Forge mounts on first visit (it reads the backend's model list) and then keeps its draft.
 const forgeOpened=useRef(false);if(tab==='forge')forgeOpened.current=true;
 const chatOpened=useRef(false);if(tab==='chat')chatOpened.current=true;
 const analysisOpened=useRef(false);if(tab==='analysis')analysisOpened.current=true;
 // Media's "Open in Media Analysis" sends the picture here.
 const [toAnalyze,setToAnalyze]=useState<{item:MediaItem;n:number}>();
 const analyze=(item:MediaItem)=>{setToAnalyze(a=>({item,n:(a?.n??0)+1}));setTab('analysis');};
 // The frame picker's sends are whatever Generate would do with a picture right now.
 const frameActions=useRef<((item:MediaItem)=>MediaAction[])|undefined>(undefined);
 const tveOpened=useRef(false);if(tab==='tve')tveOpened.current=true;
 const monitorOpened=useRef(false);if(tab==='monitor')monitorOpened.current=true;
 const [forgeHost,setForgeHost]=useState<HTMLDivElement|null>(null);
 const [h3ForgeAvailable,setH3ForgeAvailable]=useState(false);
 const [forgeCanvasContext,setForgeCanvasContext]=useState<ForgeCanvasContext>();
 const [canvasSendError,setCanvasSendError]=useState('');
 const sendToGenerate=(positive:string,negative:string,canvas?:ForgeSendCanvas)=>{
  if(canvas&&(!forgeCanvasContext||canvas.contextKey!==forgeCanvasContext.key||!forgeCanvasContext.acceptsCanvas?.(canvas.canvas))){setCanvasSendError('forge.canvas.handoffBlocked');return false;}
  setCanvasSendError('');setSent(s=>({positive,negative,n:(s?.n??0)+1,...(canvas?{canvas}: {})}));setTab('generate');return true;
 };
 const [sent,setSent]=useState<{positive:string;negative:string;n:number;canvas?:ForgeSendCanvas}>();
 // Chat draws through Generate: it asks, Workbench runs it with its own settings and answers by id.
 const [chatStudio,setChatStudio]=useState<ChatStudio>();
 const [chatRender,setChatRender]=useState<ChatRenderRequest>();
 const chatWaiting=useRef(new Map<string,(outcome:ChatRenderOutcome)=>void>());
 const renderForChat=useCallback((ask:ChatRenderRequest)=>new Promise<ChatRenderOutcome>(resolve=>{chatWaiting.current.set(ask.id,resolve);setChatRender(ask);}),[]);
 const chatRendered=useCallback((id:string,outcome:ChatRenderOutcome)=>{const resolve=chatWaiting.current.get(id);chatWaiting.current.delete(id);resolve?.(outcome);},[]);
 const [pendingConnectionFocus,setPendingConnectionFocus]=useState(false);
 useEffect(()=>{if(pendingConnectionFocus&&active==='engine'){document.getElementById('endpoint')?.focus();setPendingConnectionFocus(false);}},[pendingConnectionFocus,active]);
 const shellRef=useRef<HTMLDivElement>(null);
 // Every red banner closes in one place: attach × to each [role=alert] as it mounts.
 useEffect(()=>{
  const root=shellRef.current;if(!root)return;
  const decorate=()=>root.querySelectorAll<HTMLElement>('[role="alert"]').forEach(el=>{
   if(el.querySelector('.error-message-close'))return;
   const button=document.createElement('button');
   button.type='button';button.className='error-message-close';button.setAttribute('aria-label',t('app.dismiss'));button.textContent='×';
   button.addEventListener('click',()=>el.classList.add('is-dismissed'));
   el.appendChild(button);
  });
  decorate();
  const observer=new MutationObserver(decorate);observer.observe(root,{childList:true,subtree:true});
  return()=>observer.disconnect();
 },[t]);
 // An explicit Inspect opens the Engine node; the owned backend's automatic handoff only binds the endpoint and leaves the user where they are.
 function connectExisting(baseUrl:string,inspect=false){inspectionVersion.current++;setInspecting(false);setEndpoint(baseUrl);setSnapshot(null);setInspectionError('');if(inspect){setTab('map');setActive('engine');setPendingConnectionFocus(true);}}
 const includedIds=new Set(builtinPlugins.flatMap(p=>p.includes??[]));
 const nodes:NeuralNode[]=builtinPlugins.filter(p=>!includedIds.has(p.id)&&p.kind!=='plugin').map((p,index)=>({id:p.id,label:t(p.nameKey!),description:t(`studio.hint.${p.id}`),included:(p.includes??[]).map(id=>({id,label:t(builtinPlugins.find(part=>part.id===id)!.nameKey!)})),parentId:p.parentId,...(presentation[p.id]??{column:2,order:index}),status:t(p.id==='engine'?inspecting?'studio.status.inspecting':snapshot?'studio.status.snapshot':inspectionError?'studio.status.error':'studio.status.notInspected':p.id==='director'?'studio.status.draft':p.id==='plugins'?'studio.status.registry':'studio.status.planned')}));
 const edges:NeuralEdge[]=builtinPlugins.flatMap(p=>active===p.id?(p.suggests??[]).map(id=>({id:`suggested:${p.id}:${id}`,fromId:p.id,toId:id,kind:'suggested' as const})):[]);
 const selected=builtinPlugins.find(p=>p.id===active);
 return <div className="studio-shell" ref={shellRef}>
  <main id="workspace">
   <AiOSetup experience={experience} onExperience={setExperience} beginnerPage={beginnerPage} onBeginnerPage={setBeginnerPage} studioTab={tab} onStudioTab={setTab} onStudioActiveChange={setStudioActive} onConnectExisting={connectExisting} onEnvironment={setEnvironment} studio={<div id="studio-workspace" className="studio-workspace">
    <div className="studio-columns">
    <div className="studio-main">
    <div className="studio-generate-host" hidden={tab==='forge'||tab==='chat'||tab==='analysis'||tab==='tve'||tab==='monitor'}><Workbench beginner={beginner} beginnerPage={beginnerPage} onBeginnerPage={setBeginnerPage} planVisible={tab==='map'} endpoint={endpoint} roots={roots} root={environment?.root} incoming={sent} onCanvasContext={setForgeCanvasContext} onOpenVideoPost={()=>setTab('tve')} forgeHost={forgeHost} forgeActive={studioActive&&tab==='forge'} onOpenPromptForge={()=>setTab('forge')} onH3ForgeAvailable={setH3ForgeAvailable} chatRender={chatRender} onChatRender={chatRendered} onChatStudio={setChatStudio} onAnalyze={analyze} onFrameActions={fn=>{frameActions.current=fn;}}/></div>
    {analysisOpened.current&&<div className="studio-generate-host" hidden={tab!=='analysis'}><Analysis active={studioActive&&tab==='analysis'} endpoint={endpoint} frameActions={item=>frameActions.current?.(item)??[]} roots={roots} root={environment?.root} incoming={toAnalyze} onSend={sendToGenerate}/></div>}
    {tveOpened.current&&<div className="studio-generate-host" hidden={tab!=='tve'}><TrueVideoEnhancer endpoint={endpoint} roots={roots}/></div>}
    {monitorOpened.current&&<div className="studio-generate-host" hidden={tab!=='monitor'}><Monitoring active={studioActive&&tab==='monitor'} root={environment?.root}/></div>}
    {forgeOpened.current&&<div className="studio-generate-host" hidden={tab!=='forge'}>{canvasSendError&&<p className="error-message" role="alert">{t(canvasSendError)}</p>}<div ref={setForgeHost} hidden={!h3ForgeAvailable}/><div hidden={h3ForgeAvailable}><PromptForge active={studioActive&&tab==='forge'&&!h3ForgeAvailable} endpoint={endpoint} root={environment?.root} inputDir={roots?.input} outputDir={roots?.output} onSend={sendToGenerate} canvasContext={forgeCanvasContext}/></div></div>}
    {chatOpened.current&&<div className="studio-generate-host" hidden={tab!=='chat'}><Chat active={studioActive&&tab==='chat'} endpoint={endpoint} root={environment?.root} onSend={sendToGenerate} studio={chatStudio} onRender={renderForChat}/></div>}
    {tab==='map'&&<details className="run-plan-diagnostics" open={active!==null}><summary>{t('studio.plan.diagnostics')}</summary>
    <section className="neural-field" aria-label={t('studio.features')}><div className="field-caption"><span className="crosshair" aria-hidden="true">+</span>{t('studio.map')} <span>{t('studio.select')}</span></div>
     <NeuralGraph nodes={nodes} edges={edges} selectedId={active} label={t('studio.features')} onSelect={id=>setActive(active===id?null:id)}/>
     <div className="field-footer"><span>{t('studio.hierarchy')}</span><span>{t('studio.yourProcess')}</span></div></section>
    {selected&&<section id={`panel-${selected.id}`} className={`feature-panel panel-${selected.id}`} aria-labelledby="panel-title" onKeyDown={e=>{if(e.key==='Escape'){e.preventDefault();closePanel();}}}>
     <div className="panel-heading"><div><p className="eyebrow">{selected.parentId?`${t(builtinPlugins.find(p=>p.id===selected.parentId)!.nameKey!)} / ${t(selected.nameKey!)}`:t('studio.workspaceEngine')}</p><h2 id="panel-title">{t(selected.nameKey!)}</h2></div><button className="close-button" aria-label={t('studio.closeNamed',{name:t(selected.nameKey!)})} onClick={closePanel}>{t('studio.close')} <span aria-hidden="true">×</span></button></div>
     {active==='engine'?<div className="panel-content"><div><h3>{t('studio.engine.title')}</h3><p>{t('studio.engine.notice')}</p></div><div><label htmlFor="endpoint">{t('studio.backendUrl')}</label><div className="endpoint-row"><input id="endpoint" type="url" value={endpoint} disabled={inspecting} onChange={e=>{setEndpoint(e.target.value);setSnapshot(null);setInspectionError('');}} spellCheck={false}/><button className="action-button" disabled={inspecting||!endpoint.trim()} onClick={()=>void inspect()}>{t(inspecting?'studio.inspecting':'studio.inspect')} <span aria-hidden="true">↗</span></button></div><p className="field-help">{t('studio.backendHelp')}</p></div></div>
     :active==='director'?<div className="panel-content"><div><h3>{t('studio.director.title')}</h3><p>{t('studio.director.notice')}</p><p className="field-help">{t('studio.director.local')}</p></div><form onSubmit={e=>{e.preventDefault();saveDraft();}}><div className="draft-form"><div className="full-width"><label htmlFor="direction">{t('studio.direction')}</label><textarea id="direction" maxLength={8000} placeholder={t('studio.placeholder')} value={draft.direction} onChange={e=>updateDraft({direction:e.target.value})}/></div><div><label htmlFor="framing">{t('studio.framing')}</label><select id="framing" value={draft.framing} onChange={e=>updateDraft({framing:e.target.value})}>{framing.map(id=><option key={id} value={id}>{t(`studio.framing.${id}`)}</option>)}</select></div><div><label htmlFor="mood">{t('studio.mood')}</label><select id="mood" value={draft.mood} onChange={e=>updateDraft({mood:e.target.value})}>{moods.map(id=><option key={id} value={id}>{t(`studio.mood.${id}`)}</option>)}</select></div></div><div className="draft-footer"><button className="action-button" type="submit">{t('studio.save')}</button><p role="status" aria-live="polite">{t(draftStatus||'studio.draft.notSubmitted')}</p></div></form></div>
     :active==='plugins'?<div className="panel-content"><div><h3>{t('studio.registry.title')}</h3><p>{t('studio.registry.notice')}</p></div><ul className="plugin-list">{builtinPlugins.map(p=><li key={p.id}><strong>{t(p.nameKey!)}</strong><span className="plugin-kind">{t(`studio.kind.${p.kind}`)} / {p.parentId?t(builtinPlugins.find(parent=>parent.id===p.parentId)!.nameKey!):t('studio.workspace')}</span><p>{t(p.descriptionKey!)}</p>{p.requiredNodes.length>0&&<p className="field-help">{t('studio.requires')}: {p.requiredNodes.join(', ')}</p>}</li>)}</ul></div>
     :<div className="panel-content"><div><h3>{t(`studio.hint.${selected.id}`)}</h3><p>{t('studio.roadmap')}</p></div><div className="planned-note"><span className="eyebrow">{t('studio.notConnected')}</span><p>{t('studio.plannedNotice')}</p></div></div>}
     {active==='engine'&&<ApiNegotiationPanel endpoint={endpoint}/>}
     {active==='engine'&&<RecipePrerequisitesPanel endpoint={endpoint}/>}
     {active==='engine'&&<div aria-live="polite" aria-busy={inspecting}>{inspecting&&<p className="status-message" role="status">{t('studio.reading')}</p>}{inspectionError&&<p className="status-message error-message" role="alert">{t('studio.technicalError')}: {backendErrorText(inspectionError,t)}</p>}
      {snapshot&&<><p className="status-message" role="status">{t('studio.complete',{url:snapshot.baseUrl})}</p><div className="result-summary"><div><strong>{snapshot.nodeCount}</strong><span>{t('studio.availableNodes')}</span></div><div><strong>{snapshot.devices.length}</strong><span>{t('studio.reportedDevices')}</span></div><div><strong>{snapshot.missingRequiredNodes.length}</strong><span>{t('studio.missingDirector')}</span></div></div><div className="inspection-detail"><div><h3>{t('studio.devices')}</h3>{snapshot.devices.length?<ul>{snapshot.devices.map((d,i)=><li key={`${d}-${i}`}>{d}</li>)}</ul>:<p className="empty-note">{t('studio.noDevices')}</p>}</div><div><h3>{t('studio.missingNodes')}</h3>{snapshot.missingRequiredNodes.length?<ul>{snapshot.missingRequiredNodes.map(n=><li key={n}>{n}</li>)}</ul>:<p className="empty-note">{t('studio.allNodes')}</p>}</div><div><h3>{t('studio.capabilities')}</h3>{snapshot.capabilities.length?<ul>{snapshot.capabilities.map(c=><li key={c}>{c}</li>)}</ul>:<p className="empty-note">{t('studio.noCapabilities')}</p>}</div></div></>}
     </div>}
    </section>}
    <aside className="studio-note"><span className="note-index">{t('studio.firstConnection')}</span><p>{t('studio.firstHelp')}</p><span className="note-end" aria-hidden="true">↖</span></aside>
    </details>}
    </div></div>
   </div>}/>
  </main>
 </div>;
}
