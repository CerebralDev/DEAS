import {ThemeScene,ThemeProgress} from '../themes/ThemeEffects';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { backendErrorText } from '../../lib/backendErrors';
import { type DownloadStatus } from '../../lib/downloads';
import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { useI18n } from '../../i18n/I18nProvider';
import { isGguf, clipLoaderName, nativeImageReferenceEncoders, type DiagnosticImageIntentV1, type RecipeAllowlist } from '../../lib/execution';
import { stopWorkbench, stopWorkbenchJob, runWorkbenchJob, readCurrentWorkbench, pollWorkbench, uploadMask, uploadInput, readH3Session, readRefMods, isImageIntent, type JobStatus, type JobIntent, type H3ImportedVideo } from '../../lib/jobs';
import { chunkCount, defaultVideoOutput, h3LatentChanged, h3LatentDefaults, h3Scalings, isVideoEnhanceIntent, isVideoIntent, latentUpscaleSize, videoFrames, type H3LatentAdvanced, type H3Scaling, type VideoFamily, type VideoIntentV1, type VideoOutput } from '../../lib/videoIntent';
import { defaultAbc, defaultMusicGeneration, isMusicIntent, isMusicPipeline, musicBounds, musicMaxDuration, type AbcSettings, type MusicGenerationSettings, type MusicIntentV1, type MusicPlanning } from '../../lib/musicIntent';
import { MusicPrompts, type MusicField } from './MusicPrompts';
import { MusicSettings } from './MusicSettings';
import { RunPlan } from './RunPlan';
import { DownloadPanel } from '../downloads/DownloadPluginPanel';
import { ContinuityOverlay } from './ContinuityOverlay';
import { ReferencesOverlay } from './ReferencesOverlay';
import { defaultPlan, isContinuityPlan, nextStep, resolveClip, continuitySource, type ContinuityPlan } from '../../lib/continuity';
import { checkRecipePrerequisites } from '../../lib/recipePrerequisites';
import { scanModels, type LocalModel } from '../../lib/models';
import { ModelMetadataTarget, ModelDeclaredBadge, useModelMetadataUpdates } from '../../components/models/ModelMetadataEditor';
import { componentFiles, componentFilesNamed, defaultComponentOverrides, isComponentOverrides, type ComponentFamily, type ComponentSlot } from '../../lib/modelComponents';
import { ComponentPicker } from './ComponentPicker';
import { TvePostControls, defaultTve, fromVideoPost, isTvePref, toVideoPost } from './TvePostControls';
import { ReferencePeople, type FramePeople, directorTags, firstVisualRef, forgeLabels, h3ForgeContext, prefillItems, h3Full, over, refCounts, refsProblem, slotsOf, type RefAdd, type RefMod, type VideoRef } from './ReferencesPanel';
import { mediaTypes, mediaViews, type MediaAction, type MediaRoots, type MediaType, type MediaView } from '../media/MediaLibrary';
import { MediaPanel, MediaControls } from '../media/MediaPanel';
import { describeMedia, itemFromEntry, dragItem, draggedItem, MEDIA_DRAG, MEDIA_IMAGE_DRAG, normalizedMediaPath, relativeMediaPath, relocatedJob, relocatedMediaItem, type MediaItem, type MediaRelocation } from '../../lib/media';
import { isBoolean, isString, oneOf, readPref, usePref, PreferencePersistenceContext, resetStudioPreferences, captureStudioSettings, applyStudioSettings } from '../../lib/preferences';
import { ProjectBar } from '../projects/ProjectBar';
import type { ImageDraft, ProjectSnapshot } from '../../lib/projects';
import { JobOutputPreview } from './JobOutputPreview';
import { GenerationMetadata } from './GenerationMetadata';
import { RecentHistory } from './RecentHistory';
import { historyInputSelectionContext, historyRestore, type HistoryRestore } from './history';
import type { ForgeCanvasContext, ForgeSendCanvas } from '../../lib/forgeCanvas';
import { generationDuration } from './generationDuration';
import { generationSettings, readImageMetadata, type GenerationSettings } from '../../lib/imageMetadata';
import { outputNamingTemplate, outputPrefix } from '../../lib/outputNaming';
import { OptionsPanel, defaultOptions, isOptionsPref, parseSigmas, runOptions } from './OptionsPanel';
import builtinFamilies from '../../families/builtin.json' with { type: 'json' };
import { aspects, resolutions, scalingModes, canvasSize, scaleInput, validCanvas, type Size, type ScalingMode } from '../../lib/resolution';
import { StudioImageEditor, copyImageLayer, type EditorHandle, type EditorSnapshot } from './StudioImageEditor';
import { ImageViewer } from './ImageViewer';
import { StarMediaButton } from '../media/StarMediaButton';
import { CompareView } from './CompareView';
import { PromptOverlay } from './PromptOverlay';
import { PromptTextarea } from './PromptTextarea';
import { createPortal } from 'react-dom';
import { ForgeButton, HammerIcon } from '../forge/ForgeButton';
import { applyRefPrefill, nextShot, promptStructure, refPrefill } from '../../lib/h3Prompt';
import { dialectOf, labelCharacters, nativeTarget } from '../../lib/forge';
import { PresetPanel } from '../presets/PresetPanel';
import { WildcardPanel } from '../wildcards/WildcardPanel';
import { defaultWildcard, isWildcardRules, wildcardNode, parseWildcardLibrary, flattenCategories, mergeVariableLibrary, parsePins, parseVariableLibrary, prunePins, scanVariables, variablePick, type WildcardLibrary } from '../../lib/wildcard';
import { containedImageHit, flattenLayer, visibleImages, type ImageLayer } from './imageEditor';
import './workbench.css';
import { NativeVideo, nativeVideo } from './NativeVideo';
/** Data-only family plugin (also read natively for header detection): loader, runnable pipelines, encoder/VAE file patterns, starting settings. */
interface Family extends ComponentFamily { id:string; name:string; multiple?:number; temporalLatent?:boolean; imagePrompting?:'revision'|'qwen-image-2.1'|'krea-2'|'moss-image'; loader:string; pipelines:string[]; clipType?:string; /** Two-encoder families (Flux.1, SD3): slot 2 is picked by its filename pattern. */ clip?:{loader:'single'|'dual';encoders?:string[]}; textEncoders?:string[]; textProjections?:string[]; vaes?:string[]; audioVaes?:string[]; defaults?:{width:number;height:number;steps:number;cfg:number;sampler:string;scheduler:string}; sigmas?:number[]; loraFamily?:string; /** Video families: native fps/seconds, fixed fps (H3 is 24), Wan's high→low file-name pair. */ video?:{fps:number;seconds:number;fixedFps?:boolean;lowModel?:[string,string]} }
/** Remembered video controls; family rows read only their own keys. */
interface VideoPref { seconds:number; fps:number; passes:1|2|3; upscaler:string; shiftHigh:number; shiftLow:number; refinerStep:number; shiftVideo:number; shiftAudio:number; audioVae:string; textProjection:string }
interface LatentPref {enabled:boolean;model:string;scale:number;steps:number;denoise:number;advanced?:H3LatentAdvanced}
const defaultLatent:LatentPref={enabled:false,model:'',scale:2,steps:1,denoise:0.2};
const isLatentPref=(v:unknown):v is LatentPref=>{const p=v as LatentPref;return !!p&&typeof p.enabled==='boolean'&&typeof p.model==='string'&&[p.scale,p.steps,p.denoise].every(Number.isFinite);};
const latentModels=(family:VideoFamily|undefined,allow:RecipeAllowlist|null)=>(family==='minimax-h3'?allow?.h3LatentUpscaleModels:allow?.latentUpscaleModels)?.filter(m=>{
 const file=m.split(/[/\\]/).pop()!.toLowerCase();
 return /\.(safetensors|sft)$/.test(m)&&(family==='minimax-h3'?!/ltx/i.test(m):file.startsWith('ltx')&&file.includes('spatial')&&file.includes('upscal'));
})??[];
const defaultLayout={settings:272,queue:152,dock:220};
const defaultVideo:VideoPref={seconds:5,fps:24,passes:1,upscaler:'',shiftHigh:5,shiftLow:5,refinerStep:2,shiftVideo:10,shiftAudio:3,audioVae:'',textProjection:''};
const isVideoPref=(v:unknown):v is VideoPref=>{const o=v as VideoPref;return !!o&&typeof o==='object'&&Object.keys(defaultVideo).every(k=>typeof (o as unknown as Record<string,unknown>)[k]===typeof (defaultVideo as unknown as Record<string,unknown>)[k])&&[1,2,3].includes(o.passes);};
const families=new Map((builtinFamilies as Family[]).map(f=>[f.id,f]));
const modelCategories=['checkpoints','diffusion_models','unet'];
const supportedModelFormat=(m:LocalModel)=>/\.(safetensors|sft|ckpt|pt|pth|gguf)$/i.test(m.name);
// LoRAs stay safetensors-only: a GGUF LoRA needs a proven LoRA loader first.
const supportedLoraFormat=(m:LocalModel)=>/\.safetensors$/i.test(m.name);
type Lora={name:string;strength:number};
const isLoras=(v:unknown):v is Lora[]=>Array.isArray(v)&&v.every(l=>l&&typeof l.name==='string'&&typeof l.strength==='number');
const isNumber=(v:unknown):v is number=>typeof v==='number'&&Number.isFinite(v);
const isMaskGrow=(v:unknown):v is number=>isNumber(v)&&Number.isInteger(v)&&v>=0&&v<=512;
const isMaskBlur=(v:unknown):v is number=>isMaskGrow(v)&&v<=31;
const clamp=(n:number,min:number,max:number)=>Math.min(max,Math.max(min,n));
const isUnit=(v:unknown):v is number=>typeof v==='number'&&v>=0&&v<=1;
/** Locale keys of the Director scaling modes (shared with the image input-scaling labels). */
const h3ScalingKey:Record<H3Scaling,string>={Off:'off',Auto:'auto',Target:'target',Fit:'fit','Fill and crop':'crop','Fit and pad':'pad','Long side with divisible crop':'long'};
const videoPipelines:readonly string[]=['t2v','i2v','v2v','ref2v'];
const randomSeed=()=>Math.floor(Math.random()*2**32);
/** WebKitGTK media needs a blob URL; asset fetches also need connect-src permission in the app CSP. */
function useBlobSrc(src:string|undefined,active:boolean):{url?:string;error?:string}{
 const [state,setState]=useState<{url?:string;error?:string}>({});
 useEffect(()=>{
  setState({});if(!active||!src)return;
  let live=true,made='';
  fetch(src).then(r=>{if(!r.ok)throw new Error(`HTTP ${r.status}`);return r.blob();}).then(b=>{if(!live)return;made=URL.createObjectURL(b);setState({url:made});}).catch(e=>{if(live)setState({error:e instanceof Error?e.message:String(e)});});
  return()=>{live=false;if(made)URL.revokeObjectURL(made);};
 },[src,active]);
 return state;
}
/** Media element failures are invisible in the native webview; the caption shows them. */
const mediaError=(e:React.SyntheticEvent<HTMLMediaElement>)=>{const m=e.currentTarget.error;return m?`error ${m.code}${m.message?`: ${m.message}`:''}`:'';};
const isSeed=(v:unknown):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=0;
const isSeeds=(v:unknown):v is number[]=>Array.isArray(v)&&v.length<=10&&v.every(isSeed);
const dockTabs=['media','models','loras','presets','wildcards','post','options'] as const;
/** Pipeline taxonomy. `t2i`/`i2i` (images) and `t2v`/`i2v` (LTX, Wan, MiniMax H3 per family data) have native recipes; `v2v`/`ref2v` stay listed but disabled. */
const pipelines=['t2i','i2i','t2v','i2v','v2v','ref2v','t2m','cover'] as const;
type Pipeline=typeof pipelines[number];
const inBounds=<T,>(bounds:Record<keyof T&string,readonly [number,number,boolean]>)=>(v:unknown):v is T=>!!v&&typeof v==='object'&&Object.keys(bounds).every(k=>{const n=(v as Record<string,unknown>)[k];const [min,max,integer]=bounds[k as keyof T&string];return typeof n==='number'&&n>=min&&n<=max&&(!integer||Number.isInteger(n));});
const isAbc=inBounds<AbcSettings>(musicBounds.abc);
const isMusicGeneration=inBounds<MusicGenerationSettings>(musicBounds.generation);
const isMusicDuration=(v:unknown):v is {t2m:number;cover:number}=>!!v&&typeof v==='object'&&(['t2m','cover'] as const).every(k=>{const n=(v as Record<string,unknown>)[k];return typeof n==='number'&&n>=musicMaxDuration.step&&n<=musicMaxDuration.limit;});
/** Edits of the shown image: plain image-to-image (`resample`) or inpaint; refine/upscale live in the Post-Processing tab. */
const editModes=['resample','inpaint'] as const;
export interface WorkbenchProps { endpoint:string; planVisible?:boolean; forgeHost?:HTMLElement|null; forgeActive?:boolean; onOpenPromptForge?:()=>void; onH3ForgeAvailable?:(available:boolean)=>void; onOpenVideoPost?:()=>void; /** Environment folders: models are listed directly from disk (without it the running backend is asked), output enables direct previews of finished files. `extraModels` are additional model roots on other drives. */ roots?:MediaRoots&{models?:string;extraModels?:string[]}; /** Environment root for Prompt Forge (its saved LLM endpoint). */ root?:string; /** Prompts sent from the Prompt Forge tab; `n` changes per send. */ incoming?:{positive:string;negative:string;n:number;canvas?:ForgeSendCanvas}; onCanvasContext?:(context:ForgeCanvasContext|undefined)=>void }
/** Text-to-image workbench: settings left, preview over the prompts, queue right, bottom dock with Media · Models · LoRAs tabs. */
export function Workbench(props:WorkbenchProps){
 const [project,setProject]=useState<ProjectSnapshot|null>(null);
 return <PreferencePersistenceContext.Provider value={!project}><WorkbenchContent {...props} project={project} onProject={setProject}/></PreferencePersistenceContext.Provider>;
}
function WorkbenchContent({endpoint,roots={},root,incoming,onCanvasContext,onOpenVideoPost,forgeHost,forgeActive=true,onOpenPromptForge,onH3ForgeAvailable,planVisible=false,project,onProject}:WorkbenchProps&{project:ProjectSnapshot|null;onProject:(project:ProjectSnapshot|null)=>void}){
 const {t,locale}=useI18n();const native=isTauri();const linuxVideo=nativeVideo();const modelsDir=roots.models;const outputDir=roots.output;const extraModelsKey=(roots.extraModels??[]).join('\n');
 const [media,setMedia]=useState<MediaItem|null>(null);
 const [browsed,setBrowsed]=useState<MediaItem|null>(null);
 const browsedBlob=useBlobSrc(browsed?.src,native&&browsed?.media!=='image'&&!linuxVideo);
 const mediaBlob=useBlobSrc(media?.src,native&&media?.media!=='image'&&!linuxVideo);const [mediaFault,setMediaFault]=useState('');
 useEffect(()=>setMediaFault(''),[media?.src]);
 const [outputRevision,setOutputRevision]=useState(0);
 const [dockTab,setDockTab]=usePref('studio.dock.tab','media',oneOf(...dockTabs));
 const focusDockTab=useRef(false);
 useLayoutEffect(()=>{if(focusDockTab.current){document.querySelector<HTMLButtonElement>(`[data-dock-tab="${dockTab}"]`)?.focus();focusDockTab.current=false;}},[dockTab]);
 const openWildcards=()=>{focusDockTab.current=true;setDockTab('wildcards');document.querySelector<HTMLButtonElement>('[data-dock-tab="wildcards"]')?.focus();};
 const [dockHeight,setDockHeight]=usePref('studio.dock.height',defaultLayout.dock,isNumber);
 const [settingsWidth,setSettingsWidth]=usePref('studio.layout.settingsWidth',defaultLayout.settings,isNumber);
 const [queueWidth,setQueueWidth]=usePref('studio.layout.queueWidth',defaultLayout.queue,isNumber);
 const [settingsHidden,setSettingsHidden]=usePref('studio.layout.settingsHidden',false,isBoolean);
 const [dockHidden,setDockHidden]=usePref('studio.dock.hidden',false,isBoolean);
 const [bodyWidth,setBodyWidth]=useState(1000);
 const [sortPref,setSort]=usePref('studio.dock.sort','date',oneOf('date','name','size'));
 const [dockView,setDockView]=usePref<MediaView>('studio.dock.view','tiles',oneOf(...mediaViews));
 const [dockType,setDockType]=usePref<MediaType>('studio.dock.type','all',oneOf(...mediaTypes));
 const [query,setQuery]=useState('');
 const [models,setModels]=useState<LocalModel[]|null>(null);
 useModelMetadataUpdates(setModels);
 const [working,setBusy]=useState(false);const [scanning,setScanning]=useState(false);const busy=working||scanning;const [error,setError]=useState('');
 const [positive,setPositive]=usePref('studio.run.positive','',isString);
 const [negative,setNegative]=usePref('studio.run.negative','',isString);
 // YuE2 music authoring is separate state: never relabel image prompts, never serialize into image projects.
 const [musicStyle,setMusicStyle]=usePref('studio.run.music.style','',isString);
 const [musicLyrics,setMusicLyrics]=usePref('studio.run.music.lyrics','',isString);
 const [musicPlanning,setMusicPlanning]=usePref<MusicPlanning>('studio.run.music.planning','full',oneOf('off','full','melody'));
 const [musicDuration,setMusicDuration]=usePref('studio.run.music.maxDuration',{t2m:musicMaxDuration.t2m,cover:musicMaxDuration.cover},isMusicDuration);
 const [musicEncoderPref,setMusicEncoder]=usePref('studio.run.music.audioEncoder','',isString);
 const [musicAbc,setMusicAbc]=usePref('studio.run.music.abc',defaultAbc,isAbc);
 const [musicGeneration,setMusicGeneration]=usePref('studio.run.music.generation',defaultMusicGeneration,isMusicGeneration);
 const [musicSource,setMusicSource]=useState<MediaItem|null>(null);
 const [wildcard,setWildcard]=usePref('studio.run.wildcard',defaultWildcard,isWildcardRules);
 const [wildcardEnabled,setWildcardEnabled]=usePref('studio.run.wildcardEnabled',false,isBoolean);
 const [wildcardLibrary,setWildcardLibrary]=useState<WildcardLibrary|null>(null);
 const [wildcardLoading,setWildcardLoading]=useState(false);
 const [wildcardLibraryError,setWildcardLibraryError]=useState('');
 const [wildcardRevision,setWildcardRevision]=useState(0);
 const [customVariables]=usePref('studio.run.customVariables','{}',isString);
 useEffect(()=>{setWildcardLibrary(null);setWildcardLibraryError('');
  if(!native||!endpoint.trim()){setWildcardLibraryError(t('wildcards.noBackend'));return;}
  let live=true;setWildcardLoading(true);
  invoke<string>('native_wildcard_library',{baseUrl:endpoint}).then(text=>{const next=parseWildcardLibrary(text);if(live){setWildcardLibrary(next);setWildcardLoading(false);}}).catch(e=>{if(live){setWildcardLibrary(null);setWildcardLibraryError(String(e));setWildcardLoading(false);}});
  return()=>{live=false;};
 },[native,endpoint,wildcardRevision]);
 const wildcardVars=useMemo(()=>{
  const categories=wildcardLibrary?flattenCategories(wildcardLibrary,wildcard.style):{};
  let custom:Record<string,string[]>={};try{custom=JSON.parse(customVariables||'{}');}catch{custom={};}
  let customLibrary:Record<string,string[]>={};try{customLibrary=parseVariableLibrary(custom);}catch{customLibrary={};}
  const names=new Set([...scanVariables(positive,{}).map(x=>x.name),...scanVariables(negative,{}).map(x=>x.name)]);
  const lib=mergeVariableLibrary(categories,customLibrary,names);
  const pins=parsePins(wildcard.variable_selections);
  return {lib,pins,
   positive:scanVariables(positive,lib).map(tok=>({...tok,pick:variablePick(tok.name,tok.key,lib,pins,wildcard.seed)})),
   negative:scanVariables(negative,lib).map(tok=>({...tok,pick:variablePick(tok.name,tok.key,lib,pins,wildcard.seed)}))};
 },[wildcardLibrary,wildcard.style,wildcard.seed,wildcard.variable_selections,customVariables,positive,negative]);
 const wildcardVarChips=wildcardEnabled&&wildcardVars.positive.length+wildcardVars.negative.length>0?wildcardVars.positive.concat(wildcardVars.negative):[];
 const [checkpointPref,setCheckpoint]=usePref('studio.run.checkpoint','',isString);
 const [modelIdentity,setModelIdentity]=useState('');
 const identity=(m:LocalModel)=>JSON.stringify([m.category,m.path,m.name]);
 const [loras,setLoras]=usePref<Lora[]>('studio.run.loras',[],isLoras);
 // Prompt pictures share attachment UI; only ReVision uses a strength and vision model.
 const [imageRefs,setImageRefs]=useState<{item:MediaItem;strength:number}[]>([]);
 const [historyDraft,setHistoryDraft]=useState<HistoryRestore|null>(null);
 const historyInputContext=historyInputSelectionContext(endpoint,roots.input);
 const [historySelectionContext,setHistorySelectionContext]=useState('');
 const [historySourceSelected,setHistorySourceSelected]=useState(false);
 const [historyOpen,setHistoryOpen]=useState(false);
 const referenceDrag='application/x-deas-reference-order';
 const draggedReference=useRef<(typeof imageRefs)[number]|null>(null);
 const moveReference=(from:number,to:number)=>setImageRefs(refs=>{
  if(from===to||from<0||to<0||from>=refs.length||to>=refs.length)return refs;
  const reordered=[...refs];reordered.splice(to,0,...reordered.splice(from,1));return reordered;
 });
 const [refVision,setRefVision]=usePref('studio.run.imageRef.vision','',isString);
 const [width,setWidth]=usePref('studio.run.width',1024,isNumber);
 const [height,setHeight]=usePref('studio.run.height',1024,isNumber);
 const [aspectPref,setAspect]=usePref('studio.run.aspect','auto',oneOf(...aspects));
 const [resolutionPref,setResolution]=usePref('studio.run.resolution',readPref('studio.run.resolution','',isString)==='auto'?'model':readPref('studio.run.width',0,isNumber)?'fixed':'model',oneOf('fixed','model','custom',...Object.keys(resolutions)));
 const [aspectWidth,setAspectWidth]=usePref('studio.run.aspectWidth',16,isNumber);
 const [aspectHeight,setAspectHeight]=usePref('studio.run.aspectHeight',9,isNumber);
 const [megapixels,setMegapixels]=usePref('studio.run.megapixels',1,isNumber);
 const [inputScaling,setInputScaling]=usePref<ScalingMode>('studio.run.inputScaling','off',oneOf(...scalingModes));
 const [mediaSize,setMediaSize]=useState<(Size&{src:string})|null>(null);
 const [refSize,setRefSize]=useState<(Size&{src:string})|null>(null);
 const [steps,setSteps]=usePref('studio.run.steps',20,isNumber);
 const [cfg,setCfg]=usePref('studio.run.cfg',5,isNumber);
 const [sampler,setSampler]=usePref('studio.run.sampler','euler',isString);
 const [scheduler,setScheduler]=usePref('studio.run.scheduler','normal',isString);
 const [textEncoderPref,setTextEncoder]=usePref('studio.run.textEncoder','',isString);
 const [textEncoder2Pref,setTextEncoder2]=usePref('studio.run.textEncoder2','',isString);
 const [vaePref,setVae]=usePref('studio.run.vae','',isString);
 const [componentOverrides,setComponentOverrides]=usePref('studio.run.componentOverrides',defaultComponentOverrides,isComponentOverrides);
 // Sampler/scheduler choices are whatever the running backend lists (read-only object_info); read once per endpoint, before a picker can open.
 const [choices,setChoices]=useState<RecipeAllowlist|null>(null);
 const [optionsPref,setOptions]=usePref('studio.run.options',defaultOptions,isOptionsPref);
 const [seed,setSeed]=usePref('studio.run.seed',randomSeed(),isSeed);
 const nextSeed=useRef(seed);nextSeed.current=seed;
 const [seedMode,setSeedMode]=usePref('studio.run.seedMode','random',oneOf('random','fixed'));
 const [recentSeeds,setRecentSeeds]=usePref<number[]>('studio.run.recentSeeds',[],isSeeds);
 const [copyStatus,setCopyStatus]=useState('');
 const fixedSeed=(value:number)=>{setSeed(value);setSeedMode('fixed');};
 const copySeed=async(value:number)=>{try{await navigator.clipboard.writeText(String(value));setCopyStatus(t('studio.run.seedCopied'));}catch{setCopyStatus(t('studio.run.seedCopyFailed'));}};
 const [denoise,setDenoise]=usePref('studio.run.denoise',0.6,isNumber);
 const [pipelinePref,setPipeline]=usePref<Pipeline>('studio.run.pipeline','t2i',oneOf(...pipelines));
 const [video,setVideo]=usePref('studio.run.video',defaultVideo,isVideoPref);
 const [tve,setTve]=usePref('studio.run.tve',defaultTve,isTvePref);
 const [h3Latent,setH3Latent]=usePref('studio.run.latent.h3',defaultLatent,isLatentPref);
 const [ltxLatent,setLtxLatent]=usePref('studio.run.latent.ltx',defaultLatent,isLatentPref);
 const [latentDownload,setLatentDownload]=useState<VideoFamily|null>(null);
 const latentDialog=useRef<HTMLDialogElement>(null);
 const [visionDownload,setVisionDownload]=useState(false);
 const visionDialog=useRef<HTMLDialogElement>(null);
 const latentIds=useMemo(()=>latentDownload==='minimax-h3'?['h3-latent-upscaler','h3-latent-upscaler-civitai']:latentDownload?['ltx-latent-upscaler','ltx-latent-upscaler-civitai']:[],[latentDownload]);
 const visionIds=useMemo(()=>['clip-vision-revision','clip-vision-revision-civitai'],[]);
 const visionDownloadStatus=(status:DownloadStatus)=>{if(status.state==='succeeded')void refreshChoices();};
 const closeVisionDownload=()=>{setVisionDownload(false);void refreshChoices();};
 useEffect(()=>{if(!visionDownload)return;const dialog=visionDialog.current!;dialog.showModal();return()=>{if(dialog.open)dialog.close();};},[visionDownload]);
 const capabilityEpoch=useRef(0);
 const refreshChoices=useCallback(async(report=true)=>{const revision=++capabilityEpoch.current;try{const value=await checkRecipePrerequisites(endpoint);if(live.current&&revision===capabilityEpoch.current)setChoices(value.allowlist);}catch(e){
  // A reachable backend whose object_info is rejected is not "pending": show the rejection even from the silent re-read.
  const rejected=!/^BACKEND_(UNAVAILABLE|TIMEOUT|REQUEST_FAILED):/.test(String(e instanceof Error?e.message:e));
  if((report||rejected)&&live.current&&revision===capabilityEpoch.current)setError(backendErrorText(e,t));}},[endpoint,t]);
 const latentDownloadStatus=useCallback((status:DownloadStatus)=>{if(status.state==='succeeded')void refreshChoices();},[refreshChoices]);
 const closeLatentDownload=()=>{setLatentDownload(null);void refreshChoices();};
 useEffect(()=>{if(!latentDownload)return;const dialog=latentDialog.current!;dialog.showModal();return()=>{if(dialog.open)dialog.close();};},[latentDownload]);
 const [lastFrame,setLastFrame]=useState<MediaItem|null>(null);
 // H3 Image → Video with the picked image as last frame only (L2VA): holds that image's src while it is both input and last frame.
 const [lastOnlySrc,setLastOnlySrc]=useState<string|null>(null);
 // shortcut: source URLs identify frame pictures; use asset revisions when same-path edits are supported.
 const [framePeople,setFramePeople]=useState<Record<string,FramePeople>>({});
 useEffect(()=>{setFramePeople(p=>Object.fromEntries(Object.entries(p).filter(([src])=>src===media?.src||src===lastFrame?.src)));},[media?.src,lastFrame?.src]);
 const [refs,setRefs]=useState<VideoRef[]>([]);const [refmods,setRefmods]=useState<RefMod[]>([]);
 const [detached,setDetached]=useState<'positive'|'negative'|null>(null);
 const [copiedPrompt,setCopiedPrompt]=useState<'positive'|'negative'|MusicField|null>(null);
 // Copy + clear sit under the expand button of each prompt field.
 const promptActions=(kind:'positive'|'negative'|MusicField,value:string,set:(v:string)=>void)=>{const name=t(kind==='style'||kind==='lyrics'?`studio.music.${kind}`:`studio.run.${kind}`);return <>
  <button type="button" className="wb-prompt-expand wb-prompt-copy" data-prompt-copy={kind} aria-label={t('studio.run.prompt.copy',{name})} title={t('studio.run.prompt.copy',{name})} disabled={!value} onClick={()=>void navigator.clipboard.writeText(value).then(()=>{setCopiedPrompt(kind);setTimeout(()=>setCopiedPrompt(c=>c===kind?null:c),1500);},()=>setError(t('studio.run.prompt.copyFailed')))}>{copiedPrompt===kind?'✓':'⧉'}</button>
  <button type="button" className="wb-prompt-expand wb-prompt-clear" data-prompt-clear={kind} aria-label={t('studio.run.prompt.clear',{name})} title={t('studio.run.prompt.clear',{name})} disabled={!value} onClick={()=>set('')}>✕</button></>;};
 const [sourceStrength,setSourceStrength]=usePref('studio.run.video.sourceStrength',1,isUnit);
 const [sourceAudio,setSourceAudio]=usePref('studio.run.video.sourceAudio',false,isBoolean);
 const [h3Scaling,setH3Scaling]=usePref<H3Scaling>('studio.run.video.h3Scaling','Auto',oneOf(...h3Scalings));
 // MiniMax H3 continuity plan (overlay): steps run one by one, each pinning the clip the previous one produced.
 const [plan,setPlan]=usePref('studio.run.continuity',defaultPlan(),isContinuityPlan);
 const [referencesOpen,setReferencesOpen]=useState(false);const referencesButton=useRef<HTMLButtonElement>(null);
 const [continuityOpen,setContinuityOpen]=useState(false);const continuityButton=useRef<HTMLButtonElement>(null);
 const planRun=useRef<{jobId:string;index:number;all:boolean;imported?:H3ImportedVideo}|null>(null);const [autoPlan,setAutoPlan]=useState(false);const [resolving,setResolving]=useState(false);
 const [edit,setEdit]=usePref('studio.run.edit','resample',oneOf(...editModes));
 // Post-Processing (SwarmUI Refine / Upscale): opt-in second pass after every run, or applied alone to the shown image.
 const [post,setPost]=usePref('studio.run.post',false,isBoolean);
 const [postTiled,setPostTiled]=usePref('studio.run.post.tiled',false,isBoolean);
 const [refineDenoise,setRefineDenoise]=usePref('studio.run.refineDenoise',0.25,isNumber);
 const [upscale,setUpscale]=usePref('studio.run.upscale',1,isNumber);
 const [upscaleModelPref,setUpscaleModel]=usePref('studio.run.upscaleModel','',isString);
 const [inpaintDenoise,setInpaintDenoise]=usePref('studio.run.inpaintDenoise',0.8,isNumber);
 const [inpaintOnlyMasked,setInpaintOnlyMasked]=usePref('studio.run.inpaintOnlyMasked',false,isBoolean);
 const [maskGrow,setMaskGrow]=usePref('studio.run.maskGrow',8,isMaskGrow);
 const [maskBlur,setMaskBlur]=usePref('studio.run.maskBlur',4,isMaskBlur);
 const [masked,setMasked]=useState(false);
 const editorRef=useRef<EditorHandle>(null);
 const sourceEpoch=useRef(0);const draftEpoch=useRef(0);
 const [strength,setStrength]=edit==='inpaint'?[inpaintDenoise,setInpaintDenoise]:[denoise,setDenoise];
 // Source image of each edit job, for the original/edited split view.
 const [sources,setSources]=useState<Record<string,MediaItem>>({});const [slider,setSlider]=useState(false);
 const [jobs,setJobs]=useState<JobStatus[]>([]);const [selectedJob,setSelectedJob]=useState('');const [selectedOutput,setSelectedOutput]=useState(0);
 const live=useRef(false);const lock=useRef(false);const epoch=useRef(0);
 const preparationGeneration=useRef(0);const localPreparations=useRef(0);const stopLock=useRef(false);const stopped=useRef(new Set<string>());const admissionEpoch=useRef(0);
 const [stopping,setStopping]=useState(false);const [stopAvailable,setStopAvailable]=useState(false);const [stopStatus,setStopStatus]=useState('');
 const observation=useRef(0);const [recovered,setRecovered]=useState(!native);const [preparing,setPreparing]=useState(0);const [frontendAdmitting,setFrontendAdmitting]=useState(false);
 const MEDIA_IMAGE_RE=/\.(png|jpe?g|webp|bmp|avif|gif)$/i;
// shortcut: webkit-injected File.path only exists in Tauri; upgrade when Tauri's onDragDropEvent lands.
const droppedImage=(dt:DataTransfer):MediaItem|null=>{const f=dt.files?.[0] as (File&{path?:string})|undefined;return f?.path&&MEDIA_IMAGE_RE.test(f.name)?{name:f.name,path:f.path,media:'image',src:convertFileSrc(f.path)}:null;};
// A backend that was still warming up when first read is re-read until its capabilities arrive.
 useEffect(()=>{if(!native||!endpoint.trim()||choices)return;const timer=window.setInterval(()=>void refreshChoices(false),3000);return()=>window.clearInterval(timer);},[native,endpoint,choices,refreshChoices]);
const loadChoices=()=>{if(native&&!choices&&endpoint.trim())void refreshChoices(false);};
 useEffect(()=>{live.current=true;return()=>{live.current=false;preparationGeneration.current++;};},[]);
 // A native <select> opens synchronously on the first click; a list fetched on that click only shows on the second one.
 useEffect(()=>{setChoices(null);setLatentDownload(null);if(native&&endpoint.trim())void refreshChoices(false);return()=>{capabilityEpoch.current++;};},[native,endpoint,modelsDir,root]);
 // Models come from the environment's models folder (read-only scan, no backend probe); the backend list is the fallback.
 async function load(automatic=false){
  if(!native||(!automatic&&(lock.current||scanning))||(!modelsDir&&!endpoint.trim()))return;
  setScanning(true);setError('');const version=++epoch.current;
  try{
   const next=modelsDir?await scanModels(modelsDir,extraModelsKey?extraModelsKey.split('\n'):[])
    :await checkRecipePrerequisites(endpoint).then(e=>{if(live.current&&version===epoch.current)setChoices(e.allowlist);return [...e.allowlist.checkpoints.map(name=>({category:'checkpoints',name})),...(e.allowlist.loras??[]).map(name=>({category:'loras',name}))].map(m=>({...m,path:'',size:0,info:null}));});
   if(live.current&&version===epoch.current)setModels(next);
  }catch(e){if(live.current&&version===epoch.current)setError(e instanceof Error?e.message:String(e));}
  finally{if(live.current&&version===epoch.current)setScanning(false);}
 }
 // A new root supersedes a pending scan; old replies must not restore the previous folder.
 useEffect(()=>{setModels(null);if(modelsDir)void load(true);else setScanning(false);return()=>{epoch.current++;};},[modelsDir,extraModelsKey]); // eslint-disable-line react-hooks/exhaustive-deps
 // Remembered picks are only valid while the folder still holds them.
 const byName=(a:LocalModel,b:LocalModel)=>a.name.localeCompare(b.name);
 // Tiles show the file name without folder (the tree shows it) and model extension; the full name stays the accessible name.
 const shortName=(n:string)=>(n.split(/[\\/]/).pop()??n).replace(/\.(safetensors|sft|ckpt|pt|pth|gguf)$/i,'');
 const familyOf=(m?:LocalModel)=>m?.family?families.get(m.family):undefined;
 // Content-detected components take precedence; unknown formats retain the family filename hints.
 const parts=(f?:Family)=>f?.loader==='diffusion'?{encoders:componentFiles(models??[],f,'textEncoders',componentOverrides.textEncoders),vaes:componentFiles(models??[],f,'vaes',componentOverrides.vaes),audioVaes:componentFiles(models??[],f,'audioVaes',componentOverrides.audioVaes),projections:componentFiles(models??[],f,'textProjections',componentOverrides.textProjections)}:null;
 // Wan 2.2 ships HIGH/LOW files; the counterpart is the same name with the pair swapped (case kept).
 // shortcut: file-name swap; upgrade when a header flag tells high from low.
 const lowOf=(m:LocalModel,f?:Family)=>{const [hi,lo]=f?.video?.lowModel??[];if(!hi||!lo)return '';const low=m.name.replace(new RegExp(hi,'i'),x=>x===x.toUpperCase()?lo.toUpperCase():lo);return low!==m.name&&(models??[]).some(x=>x.name===low&&x.category===m.category)?low:'';};
 // Unknown checkpoints stay usable (self-contained loader, like SwarmUI); everything else must be a known family with a recipe for the pipeline.
 const blocked=(m:LocalModel,p:Pipeline):string=>{
 if(m.metadataError)return 'modelMeta.invalid';
 if(!supportedModelFormat(m))return 'modelMeta.format';
 // LoRA on a quantized base needs execution proof per pipeline; not advertised.
 if(/\.gguf$/i.test(m.name)&&loras.length)return 'studio.run.gguf.lora';
 const f=familyOf(m);
 if(isGguf(m.name)&&(!f||f.loader!=='diffusion'))return f?'studio.run.gguf.checkpoint':'studio.run.family.unknown';
 if(!f)return m.category==='checkpoints'&&(p==='t2i'||p==='i2i')?'':'studio.run.family.unknown';
 if(f.loader==='diffusion'&&m.category==='checkpoints')return 'studio.run.family.diffusionFolder';
 if(f.loader==='checkpoint'&&m.category!=='checkpoints')return 'studio.run.family.checkpointFolder';
 if(!f.pipelines.includes(p))return 'studio.run.pipe.noRecipe';
 // Missing companions block Run, not model selection: users must be able to open their pickers.
 if(videoPipelines.includes(p)&&f.video?.lowModel&&!lowOf(m,f))return 'studio.run.family.noEncoder';
 return '';
 };
 const allModels=(models??[]).filter(m=>modelCategories.includes(m.category)).sort(byName);
 // Dock search/sort are display-only: the pick logic keeps the name-sorted lists. Models have no date, so that option exists for media only.
 const sorts=dockTab==='media'?['date','name','size'] as const:['name','size'] as const;
 const sort=(sorts as readonly string[]).includes(sortPref)?sortPref:sorts[0];
 const [folders,setFolders]=useState<Record<string,string>>({});const folder=folders[dockTab]??'';
 const match=(m:LocalModel)=>(!folder||m.name.replace(/\\/g,'/').startsWith(folder+'/'))&&m.name.toLowerCase().includes(query.trim().toLowerCase());
 // Model names carry their subfolders (`sdxl/base.safetensors`); the tree is those prefixes, always open, filtering the tiles beside it.
 const folderTree=(list:LocalModel[])=>{
  const dirs=new Set(list.flatMap(m=>m.name.split(/[\\/]/).slice(0,-1).map((_,i,p)=>p.slice(0,i+1).join('/'))));
  const node=(path:string,label:string):React.ReactNode=><li key={path}><div className="media-folder-row"><button type="button" className="media-folder-name" aria-current={folder===path?'location':undefined} onClick={()=>setFolders({...folders,[dockTab]:path})}>{label}</button></div>
   <ul>{[...dirs].filter(d=>d.slice(0,Math.max(d.lastIndexOf('/'),0))===path).sort().map(d=>node(d,d.slice(d.lastIndexOf('/')+1)))}</ul></li>;
  return dirs.size>0&&<nav className="media-folders" aria-label={t('media.folders')}><ul>{node('',t(`studio.run.dock.${dockTab}`))}</ul></nav>;
 };
 const bySort=(a:LocalModel,b:LocalModel)=>sort==='size'?b.size-a.size:byName(a,b);
 // ComfyUI's LoadImage reads its input folder, and its output folder through the `name [output]` annotation (resolved and contained by ComfyUI).
 const under=(item:MediaItem|null,root?:string)=>item?.media==='image'&&item.path&&root?relativeMediaPath(item.path,root):'';
 // The overview names the picture only; the full ComfyUI name (folders, `[output]`) stays in the tooltip.
 const fileName=(name:string)=>(name.replace(/ \[output\]$/,'').split('/').pop()??name);
 const comfyName=(item:MediaItem|null)=>under(item,roots.input)||(under(item,outputDir)&&`${under(item,outputDir)} [output]`);
 const projectInput=project?.input?.path===media?.path?project?.input:null;
 const inputImage=comfyName(media)||(projectInput?projectInput.id+'.png':'');
 // Extra images pasted onto the source (SwarmUI editor image layers), in source pixels; they belong to one source.
 const [layers,setLayers]=useState<ImageLayer[]>([]);
 const [editorSnapshot,setEditorSnapshot]=useState<EditorSnapshot>();
 // Explicit selections belong to the currently selected backend namespace and
 // input root. A context change invalidates them; returning does not rebind them.
 useLayoutEffect(()=>{
  if(!historyDraft||historySelectionContext===historyInputContext)return;
  setHistorySelectionContext(historyInputContext);setHistorySourceSelected(false);
  sourceEpoch.current++;setEditorSnapshot(undefined);setMedia(null);setMediaSize(null);setLayers([]);setMasked(false);setLastFrame(null);
  setMusicSource(null);
  setImageRefs(values=>values.map(ref=>({...ref,item:{name:ref.item.name,media:'image',src:''}})));
 },[historyDraft,historySelectionContext,historyInputContext]);
 // Image pipelines stay clickable without an input; the missing picture only greys Run and names what to pick.
// A chip whose pipeline the remembered model cannot run picks the first compatible model (with its family defaults).
 const why=(id:Pipeline):string=>!allModels.some(m=>!blocked(m,id))?(id==='t2i'||id==='i2i'?'studio.run.noModels':'studio.run.pipe.noRecipe'):'';
 const pipeline:Pipeline=pipelinePref;
 // What the pipeline still needs picked: an image (i2i/i2v), a video (v2v) or at least one reference (ref2v).
 const mediaPipeline=!pipeline.startsWith('t2');
 const isVideo=videoPipelines.includes(pipeline);
 const isMusic=isMusicPipeline(pipeline);
 // First/last-frame controls belong to text/image-to-video; H3 continuity supports every video pipeline.
 const frameVideo=pipeline==='t2v'||pipeline==='i2v';
 const aspect=aspectPref==='auto'&&!mediaPipeline?'1:1':aspectPref;
 // Remembered picks are only valid while the folder still holds them and they fit the pipeline.
 const usable=allModels.filter(m=>!blocked(m,pipeline));
 const model=checkpointPref?(modelIdentity?usable.find(m=>m.name===checkpointPref&&identity(m)===modelIdentity):usable.find(m=>m.name===checkpointPref)):usable[0];
 const checkpoint=model?.name??'';
 const savedModel=allModels.find(m=>m.name===checkpointPref&&(!modelIdentity||identity(m)===modelIdentity));
 const unavailableModelReason=checkpointPref&&!model?(savedModel?t(blocked(savedModel,pipeline)):t('studio.quick.unavailable',{names:checkpointPref})):'';
 const family=familyOf(model);
 const resolution=resolutionPref==='model'&&!family?.defaults?'fixed':resolutionPref;
 const split=parts(family);
 const referenceMethod=(pipeline==='t2i'||pipeline==='i2i')?family?.imagePrompting:undefined;
 const revisionOk=referenceMethod==='revision';
 const qwenRefs=referenceMethod==='qwen-image-2.1'&&imageRefs.length>0;
 const promptEncoder=imageRefs.length&&referenceMethod&&referenceMethod!=='revision'?nativeImageReferenceEncoders[referenceMethod]:'CLIPTextEncode';
 const validReference=(item:MediaItem|null):item is MediaItem&{path:string}=>!!item&&item.media==='image'&&typeof item.path==='string'&&!item.path.includes('\0')&&/^(?:\/|[A-Za-z]:[\\/]|\\\\)/.test(item.path);
 const promptImage=(dt:DataTransfer)=>draggedItem(dt)??droppedImage(dt);
 const restoredReferenceMethod=historyDraft&&isImageIntent(historyDraft.intent)?historyDraft.intent.imageReference?'revision':historyDraft.intent.nativeImageReference?.method:undefined;
 const historyReferenceCompatible=!!historyDraft&&!!referenceMethod&&(!restoredReferenceMethod||referenceMethod===restoredReferenceMethod);
 const replaceHistoryReference=(index:number,item:MediaItem|null)=>{
  if(!historyReferenceCompatible||historySelectionContext!==historyInputContext||!validReference(item))return;
  // This explicit Media choice is a new authoring input, not inferred identity.
  setImageRefs(values=>values.map((ref,i)=>i===index?{...ref,item:{...item}}:ref));
 };
 const recommended=family?{textEncoders:componentFiles(models??[],family,'textEncoders'),vaes:componentFiles(models??[],family,'vaes'),audioVaes:componentFiles(models??[],family,'audioVaes'),textProjections:componentFiles(models??[],family,'textProjections')}:null;
 const textEncoder=textEncoderPref||recommended?.textEncoders[0]||'';
 const dualClip=family?.clip?.loader==='dual';
 const recommendedSecond=dualClip?componentFilesNamed(models??[],family?.clip?.encoders?.[1]?[family.clip.encoders[1]]:[]):[];
 const textEncoder2=textEncoder2Pref||recommendedSecond[0]||'';
 const vae=vaePref||recommended?.vaes[0]||'';
 const videoFamily=family?.video?family.id as VideoFamily:undefined;
 const latent=isVideo&&(videoFamily==='minimax-h3'||videoFamily==='ltx-2');
 const latentPref=videoFamily==='minimax-h3'?h3Latent:ltxLatent;
 const setLatentPref=videoFamily==='minimax-h3'?setH3Latent:setLtxLatent;
 const latentChoices=latentModels(videoFamily,choices);
 const latentModel=latentPref.model||latentChoices[0]||'';
 const latentEnabled=latent&&latentPref.enabled;
 const latentNodes=videoFamily==='minimax-h3'?['DaSiWaH3TiledUpscale']:['LatentUpscaleModelLoader','LTXVLatentUpsampler'];
 useEffect(()=>{setLatentDownload(null);},[videoFamily,isVideo,planVisible]);
 // The owned backend may still be booting at mount, or models were added since the last probe: re-read when the latent options are shown.
 useEffect(()=>{if(latent&&dockTab==='post'&&native&&endpoint.trim())void refreshChoices(false);},[latent,dockTab,native,endpoint,refreshChoices]);
 // Before the capability probe answers, `latentChoices` is empty: that is "backend pending", not a missing model.
 const latentProblems=!latentEnabled?[]:[...(!choices?[]:[...latentNodes.filter(n=>!choices.nodes.includes(n)),...(!latentChoices.includes(latentModel)?[t('studio.run.latent.missing',{name:latentModel||t('studio.run.latent.model')})]:[])]),...(!(videoFamily==='ltx-2'?latentPref.scale===2:latentPref.scale>=1&&latentPref.scale<=8&&Number.isInteger(latentPref.steps)&&latentPref.steps>=1&&latentPref.steps<=100&&latentPref.denoise>=0&&latentPref.denoise<=1)?[t('studio.run.latent.invalid')]:[])];
 const continuityAvailable=videoFamily==='minimax-h3'&&isVideo;
 const continuityOn=continuityAvailable&&plan.enabled===true;
 const continuityReferences=pipeline==='v2v'||pipeline==='ref2v';
 const continuationWithoutReferences=continuityAvailable&&continuityReferences&&!!nextStep(plan)?.continuity.sourceId&&!plan.useReferences;
 useEffect(()=>{if(!continuityAvailable||planVisible)setContinuityOpen(false);},[continuityAvailable,planVisible]);
 const needsInput=(pipeline==='i2i'||pipeline==='i2v')&&!inputImage?'studio.run.pipe.needsInput'
  :pipeline==='cover'&&!musicSource?.path?'studio.music.needsSource'
  :pipeline==='v2v'&&!(media?.media==='video'&&media.path)?'studio.run.pipe.needsVideo'
  :pipeline==='ref2v'&&!refs.length&&!(videoFamily==='minimax-h3'&&refmods.length)?'studio.run.pipe.needsRefs'
  :pipeline==='ref2v'?refsProblem(refs):'';
 const fps=family?.video?.fixedFps?family.video.fps:video.fps;
 const frames=videoFamily?videoFrames(videoFamily,video.seconds,fps):0;
 const audioVae=video.audioVae||recommended?.audioVaes[0]||'';
 const textProjection=video.textProjection||recommended?.textProjections[0]||'';
 const lastFrameName=comfyName(lastFrame);
 const lastOnly=videoFamily==='minimax-h3'&&pipeline==='i2v'&&!!media&&lastOnlySrc===media.src&&lastFrame?.src===media.src;
 const firstFrame=pipeline==='i2v'&&!lastOnly&&media?.media==='image'?media:null;
 // LoRAs of another known family are hidden; unknown ones stay (their target cannot be checked).
 const loraModels=(models??[]).filter(m=>m.category==='loras'&&(!family||!m.family||m.family===(family.loraFamily??family.id))).sort(byName);
 // A custom schedule (Options tab text, else the model's own, e.g. Turbo) replaces steps/scheduler; image-to-image keeps its tail like AlignYourSteps does (round(steps·denoise) steps).
 const schedule=parseSigmas(optionsPref.sigmas??'')??family?.sigmas;
 const tail=(d:number)=>schedule&&schedule.slice(-(Math.max(1,Math.round((schedule.length-1)*d))+1));
 const sigmas=schedule&&(pipeline==='i2i'?tail(strength):schedule);
 // Unread lists keep the remembered model; native preparation rejects one the backend no longer lists.
 const upscaleModel=!choices||choices.upscaleModels?.includes(upscaleModelPref)?upscaleModelPref:'';
 // TVE post stages ride on every video pipeline; the chain is omitted when nothing is enabled.
 const videoPost=isVideo?toVideoPost(tve):undefined;
 const loraNames=loraModels.map(m=>m.name);
 const activeLoras=loras.filter(l=>loraNames.includes(l.name));
 const options=runOptions(optionsPref,choices);
 const moss=family?.id==='moss-image';
 const mossIssues:{name:string;reset:()=>void}[]=!moss?[]:[
  ...(loras.length?[{name:t('studio.run.loras'),reset:()=>setLoras([])}]:[]),
  ...(['attention','cache','compile','vae','sigmas'] as const).filter(k=>k==='vae'?optionsPref.vae==='tiled':!!optionsPref[k]?.trim()).map(k=>({name:t(`studio.run.opt.${k}`),reset:()=>setOptions({...optionsPref,[k]:defaultOptions[k]})})),
  ...(post?[{name:t('studio.run.dock.post'),reset:()=>setPost(false)}]:[]),
  ...(pipeline==='i2i'&&visibleImages(layers).length?[{name:t('studio.editor.layers'),reset:()=>setLayers(layers.map(l=>({...l,visible:false})))}]:[]),
  ...(pipeline==='i2i'&&inputScaling==='pad'?[{name:t('studio.run.resolution.scaling'),reset:()=>setInputScaling('off')}]:[]),
 ];
 // Music omits every image/video dependency; only the YuE2 graph's own nodes, choices and the cover encoder matter.
 const musicEncoders=choices?.audioEncoders??[];
 const musicEncoder=musicEncoderPref||musicEncoders[0]||'';
 const musicNodes=['YuE2GenerateMusic','EmptyYuE2LatentAudio','VAEDecodeAudio','SaveAudioAdvanced','ConditioningZeroOut',...(pipeline==='cover'?['LoadAudio','AudioEncoderLoader','SheetSage2AudioToABC']:musicPlanning==='off'?[]:['YuE2GenerateABC'])];
 // An empty Style is an unfinished draft, not an incompatible dependency: it blocks Run without the dependency warning.
 const musicNeedsStyle=isMusic&&!musicStyle.trim();
 // No backend capabilities yet (starting, stopped, unreachable): one calm notice and a disabled Run, never dependency warnings.
 const backendPending=native&&!choices;
 const unavailable=isMusic?[
  ...(!choices?[]:[...musicNodes.filter(n=>!choices.nodes.includes(n)),...(!choices.samplers.includes(sampler)?[sampler]:[]),...(!choices.schedulers.includes(scheduler)?[scheduler]:[]),
   ...(pipeline==='cover'&&!musicEncoders.includes(musicEncoder)?[musicEncoder||t('studio.music.needsEncoder')]:[])]),
 ]:[
  ...latentProblems,
  ...imageRefs.filter(r=>!r.item.path).map(r=>t('studio.history.missingInput',{name:r.item.name})),
  ...mossIssues.map(i=>t('studio.run.moss.plain',{name:i.name})),
  ...(imageRefs.length&&!referenceMethod?[t('studio.run.reference.inactive')]:[]),
  ...(imageRefs.length&&referenceMethod?(!choices?[]:(revisionOk?['CLIPVisionLoader','LoadImage','CLIPVisionEncode','unCLIPConditioning']:[promptEncoder,'LoadImage',...(qwenRefs?['ImageScale']:[])]).filter(n=>!choices.nodes.includes(n))):[]),
  ...(models?loras.filter(l=>!loraNames.includes(l.name)).map(l=>`LoRA: ${l.name}`):[]),
 ...loraModels.filter(m=>(m.metadataError||!supportedLoraFormat(m))&&loras.some(l=>l.name===m.name)).map(m=>`${t(m.metadataError?'modelMeta.invalid':'modelMeta.format')} LoRA: ${m.name}`),
  ...(split&&!split.encoders.includes(textEncoder)?[textEncoder||t('studio.run.textEncoder')]:[]),
  ...(dualClip&&split&&(!textEncoder2||!split.encoders.includes(textEncoder2))?[textEncoder2||t('studio.run.textEncoder2')]:[]),
  ...(dualClip&&choices?[...(!choices.nodes.includes(isGguf(textEncoder)||isGguf(textEncoder2)?'DualCLIPLoaderGGUF':'DualCLIPLoader')?[isGguf(textEncoder)||isGguf(textEncoder2)?'DualCLIPLoaderGGUF':'DualCLIPLoader']:[]),
   ...((!choices.dualTextEncoders?.includes(textEncoder)||!choices.dualTextEncoders?.includes(textEncoder2))?[textEncoder2||t('studio.run.textEncoder2')]:[]),
   ...(!choices.dualClipTypes?.includes(family?.clipType??'')?[`DualCLIPLoader: ${family?.clipType??''}`]:[])]:[]),
  ...(split&&!split.vaes.includes(vae)?[vae||t('studio.run.vae')]:[]),
  ...(split&&isVideo&&family?.audioVaes&&!split.audioVaes.includes(audioVae)?[audioVae||t('studio.run.video.audioVae')]:[]),
  ...(split&&isVideo&&family?.textProjections&&!split.projections.includes(textProjection)?[textProjection||t('studio.run.video.textProjection')]:[]),
  ...(isVideo&&tve.frameGen&&tve.engine==='rife'&&!tve.rifeModel?[t('studio.tve.post.needsRifeModel')]:[]),
  ...(isVideo&&videoPost&&choices?[...(videoPost.upscaleModel&&!choices.upscaleModels?.includes(videoPost.upscaleModel)?[videoPost.upscaleModel]:[]),
   ...(videoPost.rtx&&!choices.nodes.includes('DaSiWa_RTX_UpscalerRefiner')?['DaSiWa_RTX_UpscalerRefiner']:[]),
   ...(videoPost.frameGeneration?.engine==='rtx'&&!choices.nodes.includes('DaSiWa_RTX_FrameGeneration')?['DaSiWa_RTX_FrameGeneration']:[]),
   ...(videoPost.frameGeneration?.engine==='rife'&&!choices.frameInterpolationModels?.includes(videoPost.frameGeneration.model)?[videoPost.frameGeneration.model]:[])]:[]),
  ...(choices?[...(!isVideo&&wildcardEnabled&&!choices.nodes.includes(wildcardNode)?[wildcardNode]:[]),...(!choices.samplers.includes(sampler)?[sampler]:[]),...(!choices.schedulers.includes(scheduler)?[scheduler]:[]),
  ...(imageRefs.length>0&&revisionOk&&(!refVision||!choices.visionModels?.includes(refVision))?[refVision||t('studio.run.reference.vision')]:[]),
   ...(!isVideo&&post&&upscaleModelPref&&!choices.upscaleModels?.includes(upscaleModelPref)?[upscaleModelPref]:[]),
   ...(!isVideo&&post&&postTiled&&!family?.temporalLatent&&!choices.nodes.includes('DaSiWaTiledDiffusion')?['DaSiWaTiledDiffusion']:[]),
   ...(optionsPref.attention&&!options?.attention&&!options?.sage?[optionsPref.attention]:[]),
   ...(optionsPref.cache&&!options?.cache?[optionsPref.cache]:[]),...(optionsPref.compile&&!options?.compile?[optionsPref.compile]:[]),
   ...(optionsPref.vae==='tiled'&&!options?.vaeTile?['VAEDecodeTiled']:[])]:[])
 ];
 const lowModel=isVideo&&model?lowOf(model,family):'';
 const lowMetadata=lowModel&&(models??[]).find(m=>m.name===lowModel&&m.category===model?.category)?.metadataError;
 if(lowMetadata)unavailable.push(`${t('modelMeta.invalid')} ${lowModel}`);
 if(!isMusic&&[checkpoint,textEncoder,lowModel].some(isGguf)){
  if(choices){
   const dual=videoFamily==='ltx-2'&&isVideo;
   const quantizedEncoder=isGguf(textEncoder);
   const loader=clipLoaderName(textEncoder,dual);
   const encoders=quantizedEncoder?(dual?choices.ggufDualTextEncoders:choices.ggufTextEncoders):choices.textEncoders;
   const types=quantizedEncoder?(dual?choices.ggufDualClipTypes:choices.ggufClipTypes):choices.clipTypes;
   for(const name of [checkpoint,lowModel].filter(Boolean)){
    const node=isGguf(name)?'UnetLoaderGGUF':'UNETLoader';
    if(!choices.nodes.includes(node))unavailable.push(node);
    if(!(isGguf(name)?choices.ggufDiffusionModels:choices.diffusionModels)?.includes(name))unavailable.push(name);
   }
   for(const node of [loader,'VAELoader'])if(!choices.nodes.includes(node))unavailable.push(node);
   for(const name of [textEncoder,...(dual?[textProjection]:[])].filter(Boolean))if(!encoders?.includes(name))unavailable.push(name);
   for(const name of [vae,...(isVideo&&family?.audioVaes?[audioVae]:[])].filter(Boolean))if(!choices.vaes?.includes(name))unavailable.push(name);
   if(family?.clipType&&!types?.includes(family.clipType))unavailable.push(`${loader}: ${family.clipType}`);
  }
 }
 const source=mediaSize&&mediaSize.src===media?.src?mediaSize:undefined;
 const modelSize=family?.defaults??{width:1024,height:1024};
 // Canvas edges snap to the family's latent/patch multiple (SDXL: 64 buckets, H3/LTX: 32); unknown models use the safe 16.
 const grid=qwenRefs?32:family?.multiple??16;
 // Multi-reference Auto follows the Director: the first picture or video by slot, never whatever the preview shows.
 const firstRef=pipeline==='ref2v'?firstVisualRef(refs):undefined;
 const firstRefSrc=firstRef?.item.src;const firstRefMedia=firstRef?.item.media;
 useEffect(()=>{
  if(!firstRefSrc)return;
  let live=true;
  const done=(width:number,height:number)=>{if(live&&width>0&&height>0)setRefSize({src:firstRefSrc,width,height});};
  if(firstRefMedia==='video'){const v=document.createElement('video');v.preload='metadata';v.onloadedmetadata=()=>done(v.videoWidth,v.videoHeight);v.src=firstRefSrc;}
  else{const i=new Image();i.onload=()=>done(i.naturalWidth,i.naturalHeight);i.src=firstRefSrc;}
  return()=>{live=false;};
 },[firstRefSrc,firstRefMedia]);
 const autoSource=pipeline==='ref2v'?(refSize&&refSize.src===firstRefSrc?refSize:undefined):source;
 const selectedCanvas=canvasSize(aspect,resolution,{width:aspectWidth,height:aspectHeight},megapixels,{width,height},modelSize,mediaPipeline?autoSource:undefined,grid);
 const canvas=qwenRefs?{width:Math.round(selectedCanvas.width/32)*32,height:Math.round(selectedCanvas.height/32)*32}:selectedCanvas;
 // Post-processing scales both edges and snaps each to the model grid.
 const up=(s:Size):Size=>({width:Math.max(grid,Math.round(s.width*upscale/grid)*grid),height:Math.max(grid,Math.round(s.height*upscale/grid)*grid)});
 // Tiled refine: tiles at the model's native size, overlap an eighth of the shorter edge (at least 64 px), all on the model grid.
 const snap=(n:number)=>Math.max(grid,Math.round(n/grid)*grid);
 const tiled=postTiled&&!family?.temporalLatent&&choices?.nodes?.includes('DaSiWaTiledDiffusion')?{tileWidth:snap(modelSize.width),tileHeight:snap(modelSize.height),overlap:snap(Math.max(64,Math.min(modelSize.width,modelSize.height)/8))}:null;
 const inpaint=pipeline==='i2i'&&edit==='inpaint';
 // MiniMax H3 edits a still like the Director's "Image Inpaint" mode: no denoise, no refine pass.
 const h3Image=pipeline==='i2i'&&family?.id==='minimax-h3';
 const scaled=pipeline!=='i2i'||!source?null:scaleInput(inpaint?'off':inputScaling,source,canvas,grid);
 if(moss){
  // Check this variant's emitted nodes, not the checkpoint diagnostic's missingNodes.
  const saver=['DaSiWa_MetadataImageSaver','DaSiWa_MetadataConfig'].every(n=>choices?.nodes.includes(n))?['DaSiWa_MetadataImageSaver','DaSiWa_MetadataConfig']:['SaveImage'];
  const required=[isGguf(checkpoint)?'UnetLoaderGGUF':'UNETLoader',clipLoaderName(textEncoder),'VAELoader',promptEncoder,'KSampler','VAEDecode',...saver,...(pipeline==='i2i'?['LoadImage','VAEEncode',...(scaled?.resize?['ImageScale']:[])]:['EmptyFlux2LatentImage']),...(inpaint?['LoadImageMask','SetLatentNoiseMask','ImageCompositeMasked']:[])];
  unavailable.push(...(!choices?[]:[...required.filter(n=>!choices.nodes.includes(n)),...(!(isGguf(textEncoder)?choices.ggufClipTypes:choices.clipTypes)?.includes('moss_image')?[`${clipLoaderName(textEncoder)}: moss_image`]:[])]));
 }
 // Video snaps the canvas to the family multiple like images; a video source only drives Auto aspect.
 const editableSource=media?.media==='image'&&source&&validCanvas(source)?source:null;
 const outputSize=(inpaint||(moss&&pipeline==='i2i'&&!scaled?.resize))&&source?source:(scaled?.size??canvas);
 const latentSize=latentEnabled&&videoFamily?latentUpscaleSize(videoFamily,outputSize,latentPref.scale):outputSize;
 if(latentEnabled&&!validCanvas(latentSize))unavailable.push(t('studio.run.resolution.invalid'));
 const sizeErrorFor=(continuing=false)=>isMusic?needsInput:inpaint&&source&&(!validCanvas(source)||source.width%grid!==0||source.height%grid!==0)?'studio.editor.sourceSize'
  :needsInput&&!continuing?needsInput
  :moss&&pipeline==='i2i'&&source&&!scaled?.resize&&(source.width%grid!==0||source.height%grid!==0)?'studio.run.moss.resize'
  :(!validCanvas(outputSize)||((moss||qwenRefs)&&(outputSize.width%grid!==0||outputSize.height%grid!==0)))?'studio.run.resolution.invalid'
  :(pipeline==='i2i'||pipeline==='i2v'||pipeline==='v2v')&&!source&&!(continuing&&resolution==='fixed')?'studio.run.resolution.waiting'
  :mediaPipeline&&aspect==='auto'&&resolution!=='fixed'&&(firstRef||(pipeline!=='ref2v'&&media&&media.media!=='audio'))&&!autoSource?'studio.run.resolution.waiting':'';
 const sizeError=sizeErrorFor();
 const continuitySizeError=sizeErrorFor(continuationWithoutReferences);
 const historyProblems=historyDraft?[
  ...historyDraft.unsupported,
  ...(!choices?['capabilities unvalidated']:[]),
  ...(historyDraft.source&&(!(isMusicIntent(historyDraft.intent)?musicSource?.path:media)||!historySourceSelected)?[t('studio.history.missingInput',{name:historyDraft.source.name})]:[]),
  ...(historyDraft.lastFrame&&(!lastFrame?.path||(videoFamily!=='minimax-h3'&&!lastFrameName))?[t('studio.history.missingInput',{name:historyDraft.lastFrame.name})]:[]),
  ...(historySelectionContext!==historyInputContext?['input context changed']:[]),
  ...(isVideoIntent(historyDraft.intent)&&videoFamily!==historyDraft.intent.family?['video.family']:[]),
  ...(isImageIntent(historyDraft.intent)&&imageRefs.length&&historyDraft.intent.imageReference&&referenceMethod!=='revision'?['imageReference.method']:[]),
  ...(isImageIntent(historyDraft.intent)&&imageRefs.length&&historyDraft.intent.nativeImageReference&&referenceMethod!==historyDraft.intent.nativeImageReference.method?['nativeImageReference.method']:[]),
  ...(isImageIntent(historyDraft.intent)&&pipeline==='i2i'&&source&&(outputSize.width!==historyDraft.intent.width||outputSize.height!==historyDraft.intent.height)?['effective dimensions']:[]),
 ]:[];
 // Forge reads this actual authoring calculation. It never guesses from saved
 // preferences or changes the source/model/scaling path to accept a canvas.
 const requiredCanvasGrid=(isVideo||moss||qwenRefs||inpaint)?grid:8;
 const acceptsForgeCanvas=(requested:Size)=>{
  if(!model||!validCanvas(requested)||requested.width%requiredCanvasGrid!==0||requested.height%requiredCanvasGrid!==0)return false;
  let produced=requested;
  if(pipeline==='i2i'){
   if(!source)return false;
   const next=scaleInput(inpaint?'off':inputScaling,source,requested,grid);
   produced=inpaint||(moss&&!next.resize)?source:next.size;
  }
  return produced.width===requested.width&&produced.height===requested.height;
 };
 const forgeCanvasKey=JSON.stringify([historyInputContext,root,roots.models,outputDir,project?.id,checkpoint,family?.id,pipeline,edit,inputScaling,media?.src,source?.width,source?.height,grid,requiredCanvasGrid,resolution,aspect,aspectWidth,aspectHeight,megapixels,width,height,outputSize.width,outputSize.height,sizeError]);
 useEffect(()=>{
  onCanvasContext?.({key:forgeCanvasKey,effective:model&&!sizeError&&validCanvas(outputSize)&&outputSize.width%requiredCanvasGrid===0&&outputSize.height%requiredCanvasGrid===0?{width:outputSize.width,height:outputSize.height}:null,model:modelSize,grid,requiredGrid:requiredCanvasGrid,resolution,megapixels,fixed:{width,height},acceptsCanvas:acceptsForgeCanvas});
 },[forgeCanvasKey,onCanvasContext]); // eslint-disable-line react-hooks/exhaustive-deps
 useEffect(()=>()=>onCanvasContext?.(undefined),[onCanvasContext]);
 useEffect(()=>{
  if(!incoming)return;
  // Image prompt handoff has no music target: Style/Lyrics are never filled from positive/negative.
  if(isMusic){setError(t('studio.music.handoffBlocked'));return;}
  if(incoming.canvas&&(incoming.canvas.contextKey!==forgeCanvasKey||!acceptsForgeCanvas(incoming.canvas.canvas))){setError(t('forge.canvas.handoffBlocked'));return;}
  setPositive(incoming.positive);if(incoming.negative.trim())setNegative(incoming.negative);
  if(incoming.canvas){setWidth(incoming.canvas.canvas.width);setHeight(incoming.canvas.canvas.height);setResolution('fixed');}
 },[incoming?.n]); // eslint-disable-line react-hooks/exhaustive-deps
 const ratio=aspect==='auto'?(source?source.width/source.height:1):aspect==='custom'?aspectWidth/aspectHeight:Number(aspect.split(':')[0])/Number(aspect.split(':')[1]);
 const swatchRatio=Number.isFinite(ratio)&&ratio>0?ratio:1;
 const readMediaSize=(width:number,height:number)=>{if(media&&width>0&&height>0)setMediaSize({src:media.src,width,height});};
 // Unknown jobs the user already stopped must not hold the next generation back.
 const current=jobs.find(j=>j.state==='queued'||j.state==='running'||(j.state==='unknown'&&!stopped.current.has(j.id)));
 const admissionBlocked=stopping||(!recovered&&!localPreparations.current)||(frontendAdmitting&&!localPreparations.current);
 const serialBlocked=!!current||busy||preparing>0||frontendAdmitting||!!planRun.current;
 const beginRun=()=>{localPreparations.current++;lock.current=true;setBusy(true);setError('');return {generation:preparationGeneration.current,admissionEpoch:admissionEpoch.current};};
 const activeRun=(ticket:{generation:number})=>live.current&&ticket.generation===preparationGeneration.current;
 // Press revisions must not delay reconciliation after the last Run reply settles.
 const finishRun=(ticket:{generation:number},admitted:boolean)=>{if(!activeRun(ticket))return;localPreparations.current--;lock.current=localPreparations.current>0;setBusy(lock.current);if(admitted&&!lock.current){setObservationRetry(0);setRecovered(false);}};
 function buildImageIntent(naming:string,out:VideoOutput,only?:{image:string;size:Size;upscaleOnly?:boolean}):DiagnosticImageIntentV1{
  const target=up(only?only.size:outputSize);
  const refinePass=post&&!h3Image;
  const inputLayers=visibleImages(layers);
  const refineSigmas=only?.upscaleOnly?[]:tail(refineDenoise); // Upscale alone: no diffusion tail, the pass is resize + model upscale.
  const wiredWildcard:typeof wildcard=wildcardEnabled?{...wildcard,...(Object.keys(wildcardVars.lib).length?{variables_json:JSON.stringify(wildcardVars.lib),variable_selections:JSON.stringify(prunePins(negative,wildcardVars.lib,prunePins(positive,wildcardVars.lib,parsePins(wildcard.variable_selections))))}:{})}:wildcard;
  const common={recipeId:'checkpoint-text-to-image-diagnostic',version:1,positivePrompt:positive,negativePrompt:negative,checkpoint,seed,cfg,sampler,scheduler,outputPrefix:naming,...(wildcardEnabled?{wildcard:wiredWildcard}:{}),...(activeLoras.length?{loras:activeLoras}:{}),...(split&&family?.clipType?{diffusion:{textEncoder,...(dualClip&&textEncoder2?{textEncoder2}:{}),clipType:family.clipType,vae,family:family.id}}:{}),...(family?{family:family.id}:{}),...(h3Image?{h3Edit:{shiftVideo:video.shiftVideo,shiftAudio:video.shiftAudio}}:{}),...(imageRefs.length&&referenceMethod?(referenceMethod==='revision'?{imageReference:{visionModel:refVision,images:imageRefs.map(r=>({image:comfyName(r.item)||r.item.path!,strength:r.strength}))}}:{nativeImageReference:{method:referenceMethod,images:imageRefs.map(r=>comfyName(r.item)||r.item.path!)}}):{}),...(options?{options}:{}),...(out.imageFormat!=='png'||out.imageCompression!==44?{imageFormat:out.imageFormat,imageCompression:out.imageCompression}:{})} as const;
  const resized=!!upscaleModel||target.width!==only?.size.width||target.height!==only?.size.height;
  return Object.freeze(only
   ?{...common,width:target.width,height:target.height,steps:refineSigmas?refineSigmas.length-1:steps,...(refineSigmas?{sigmas:refineSigmas}:{}),inputImage:only.image,denoise:only.upscaleOnly?0:refineDenoise,...(resized?{inputResize:{...target,crop:'disabled' as const}}:{}),...(upscaleModel?{upscaleModel}:{})}
   :{...common,width:outputSize.width,height:outputSize.height,steps:sigmas?sigmas.length-1:steps,...(sigmas?{sigmas}:{}),...(pipeline==='i2i'?{inputImage,denoise:strength,...(scaled?.resize?{inputResize:scaled.resize}:{}),...(inputLayers.length?{inputLayers}:{})}:{}),
    ...(refinePass?{refine:{...target,denoise:refineDenoise,...(upscaleModel?{upscaleModel}:{}),...(refineSigmas?{sigmas:refineSigmas}:{}),...(tiled?{tiled}:{})}}:{})});
 }
 /** `only` = Post-process: the refine/upscale pass alone on that image (an image-to-image job), without a first generation. */
 /** `step` = a continuity planner step (H3): its seconds and action replace the controls, and the finished job's clip is pinned to it. */
 async function run(only?:{image:string;size:Size;src:string;item:MediaItem;upscaleOnly?:boolean},step?:{index:number;all:boolean}){
  if(!native||stopLock.current||admissionBlocked||scanning||((step||autoPlan||planRun.current||resolving)&&(!step||serialBlocked))||(moss&&only))return;
  if(step&&!continuityOn)return;
  if(!checkpoint){setError(t('studio.run.noModels'));return;}
  if(backendPending||unavailable.length||musicNeedsStyle||historyProblems.length)return; // The overlay derives dependency warnings from the current selections.
  const editing=!only&&inpaint;
  if(!only&&(step?continuitySizeError:sizeError)){setError(t(step?continuitySizeError:sizeError,{multiple:String(grid)}));return;}
  if(editing&&!masked){setError(t('studio.run.edit.needsMask'));return;}
  const target=up(only?only.size:outputSize);
  const refinePass=post&&!h3Image&&!isVideo;
  if((refinePass||only)&&!validCanvas(target)){setError(t('studio.run.resolution.invalid'));return;}
  const capturedSeed=seedMode==='random'?nextSeed.current:seed;
  const naming=outputPrefix(outputNamingTemplate(optionsPref.outputNaming),{type:isMusic&&!only?'audio':isVideo&&!only?'video':'image',seed:capturedSeed,model:checkpoint});
  if(naming===null){setError(t('studio.run.opt.outputNaming.invalid'));return;}
  const out=optionsPref.output??defaultVideoOutput;
  if(seedMode==='random'){nextSeed.current=randomSeed();setSeed(nextSeed.current);}
  if(isMusic&&!only){await runMusic(naming,capturedSeed);return;}
  if(isVideo&&!only){await runVideo(naming,out,capturedSeed,step);return;}
  const layerSnapshot=layers.filter(l=>l.visible).map(l=>({...l}));const inputLayers=visibleImages(layerSnapshot);
  const refSnap=imageRefs.map(r=>({path:r.item.path!,strength:r.strength}));
  const methodSnapshot=referenceMethod,visionSnapshot=refVision;
  const intent={...structuredClone(buildImageIntent(naming,out,only)),seed:capturedSeed};
  const from=only?.item??(pipeline==='i2i'?media:null);
  observation.current++;setObservationRetry(n=>n+1);const ticket=beginRun();
  let admitted=false;
  try{
   // Canvas paint is mutable and cannot be structured-cloned.
   for(const layer of layerSnapshot){
    if(!layer.paint)continue;
    const paint=document.createElement('canvas');paint.width=layer.paint.width;paint.height=layer.paint.height;
    const context=paint.getContext('2d');if(!context)throw new Error('canvas');
    context.drawImage(layer.paint,0,0);layer.paint=paint;
   }
   // Capture geometry and PNG before any async upload; subsequent edits belong to the next job.
   const bounds=editing&&inpaintOnlyMasked?editorRef.current?.maskBounds(maskGrow+maskBlur):null;
   const target=bounds?canvasSize('auto','model',bounds,1,bounds,modelSize,bounds,grid):null;
   if(target&&!validCanvas(target))throw new Error(t('studio.run.resolution.invalid'));
   const inpaintCrop=bounds&&target?{...bounds,targetWidth:target.width,targetHeight:target.height}:null;
   const maskSnapshot=editing?editorRef.current?.maskPng():null;
   if(editing&&!maskSnapshot)throw new Error(t('studio.run.edit.needsMask'));
   const imageSnapshot=!only&&pipeline==='i2i'?editorRef.current?.imagePng():null;
   const [maskPng,imagePng]=await Promise.all([maskSnapshot,imageSnapshot]);
   const inputMask=maskPng?await uploadMask(endpoint,maskPng):null;
   // A changed base needs the complete image stack baked once, with the mask kept separate.
   const painted=imagePng||only||pipeline!=='i2i'?[]:await Promise.all(layerSnapshot.filter(l=>l.visible).map(async l=>{const png=await flattenLayer(l);return png?await uploadMask(endpoint,png):l.name;}));
   const withLayers=imagePng?{...intent,inputImage:await uploadMask(endpoint,imagePng)}:painted.some((name,i)=>name!==inputLayers[i].image)?{...intent,inputLayers:inputLayers.map((l,i)=>({...l,image:painted[i]}))}:{...intent};
   if(projectInput&&!imagePng&&!only&&pipeline==='i2i')withLayers.inputImage=await uploadInput(endpoint,projectInput.path);
   if(imagePng)delete withLayers.inputLayers;
   // Even matching UI roots cannot prove this backend still owns the shown job's directory.
   if(only&&!only.image&&only.item.path)withLayers.inputImage=await uploadInput(endpoint,only.item.path);
   if(refSnap.length&&methodSnapshot){
    const images=await Promise.all(refSnap.map(r=>uploadInput(endpoint,r.path)));
    if(methodSnapshot==='revision')withLayers.imageReference={visionModel:visionSnapshot,images:images.map((image,i)=>({image,strength:refSnap[i].strength}))};
    else withLayers.nativeImageReference={method:methodSnapshot,images};
   }
   if(!activeRun(ticket))return;
   admitted=true;
   const job=await runWorkbenchJob(endpoint,Object.freeze(inputMask?{...withLayers,inputMask,...(maskBlur?{maskBlur}:{}),...(inpaintCrop?{inpaintCrop}:{})}:withLayers),ticket.admissionEpoch);
   if(activeRun(ticket)){setJobs(list=>[job,...list.filter(j=>j.id!==job.id)]);setBrowsed(null);setSelectedOutput(0);setSelectedJob(job.id);if(from)setSources(s=>({...s,[job.id]:from}));setRecentSeeds(list=>[intent.seed,...list.filter(s=>s!==intent.seed)].slice(0,10));}
  }catch(e){if(activeRun(ticket))setError(e instanceof Error?e.message:String(e));}
  finally{finishRun(ticket,admitted);}
 }
 /** Music intent from the independent music state only (no image `common` spread); `sourceAudio` is the uploaded name for covers. */
 function buildMusicIntent(naming:string,capturedSeed:number,sourceAudio?:string):MusicIntentV1{
  const base={recipeId:'music-generation',version:1,family:'yue2',checkpoint,style:musicStyle,lyrics:musicLyrics,seed:capturedSeed,steps,cfg,sampler,scheduler,generation:structuredClone(musicGeneration),outputPrefix:naming} as const;
  if(pipeline==='cover')return {...base,mode:'cover',planning:'melody',maxDuration:musicDuration.cover,sourceAudio:sourceAudio??'',audioEncoder:musicEncoder};
  return musicPlanning==='off'?{...base,mode:'t2m',planning:'off',maxDuration:musicDuration.t2m}:{...base,mode:'t2m',planning:musicPlanning,maxDuration:musicDuration.t2m,abc:structuredClone(musicAbc)};
 }
 async function runMusic(naming:string,capturedSeed:number){
  const sourcePath=pipeline==='cover'?musicSource?.path:undefined;
  if(pipeline==='cover'&&(!sourcePath||!musicEncoder)){setError(t(sourcePath?'studio.music.needsEncoder':'studio.music.needsSource'));return;}
  // Snapshot everything before the first await; later edits belong to the next job.
  const draft=structuredClone(buildMusicIntent(naming,capturedSeed));
  observation.current++;setObservationRetry(n=>n+1);const ticket=beginRun();
  let admitted=false;
  try{
   const intent:MusicIntentV1=draft.mode==='cover'?{...draft,sourceAudio:await uploadInput(endpoint,sourcePath!)}:draft;
   if(!activeRun(ticket))return;
   admitted=true;
   const job=await runWorkbenchJob(endpoint,Object.freeze(intent),ticket.admissionEpoch);
   if(activeRun(ticket)){setJobs(list=>[job,...list.filter(j=>j.id!==job.id)]);setBrowsed(null);setSelectedOutput(0);setSlider(false);setSelectedJob(job.id);setRecentSeeds(list=>[intent.seed,...list.filter(s=>s!==intent.seed)].slice(0,10));}
  }catch(e){if(activeRun(ticket))setError(e instanceof Error?e.message:String(e));}
  finally{finishRun(ticket,admitted);}
 }
 function buildVideoIntent(vf:VideoFamily,naming:string,out:VideoOutput,nameOf:(item:MediaItem,force?:boolean)=>string,cont:ReturnType<typeof nextStep>=null):VideoIntentV1{
  const seconds=cont?plan.steps[cont.index].seconds:video.seconds;
  const count=videoFrames(vf,seconds,fps);
  const chunks=optionsPref.chunkHeadroom?chunkCount(count,outputSize.width,outputSize.height,optionsPref.chunkHeadroom):undefined;
  const {imageFormat,imageCompression,...output}=out;void imageFormat;void imageCompression;
  const base={recipeId:'video-generation',version:1,positivePrompt:positive,negativePrompt:negative,model:checkpoint,textEncoder,vae,seed,width:outputSize.width,height:outputSize.height,frames:count,frameRate:fps,steps,cfg,sampler,scheduler,outputPrefix:naming,output,
   ...(activeLoras.length?{loras:activeLoras}:{}),...(options?{options}:{}),...(chunks?{chunks}:{}),...(videoPost?{videoPost}:{})} as const;
   let intent:VideoIntentV1;
   // The Director's default canvas fit for DEAS stays implicit, so unchanged intents stay byte-identical.
   const scaling={inputScaling:h3Scaling}; // Always sent: an absent field means Fill and crop on jobs saved before Auto became the default.
   const last=frameVideo&&lastFrameName?{lastFrame:lastFrameName}:{};
   // Dock files outside the ComfyUI input/output folders (and every video, which core LoadVideo reads from the input root) are copied there.
   if(vf==='ltx-2'){
    const source=pipeline==='v2v'&&media?.path?{sourceVideo:nameOf(media,true)}:{};
    const keyframes=pipeline==='ref2v'?refs.map(r=>({image:nameOf(r.item),frame:Math.min(count-1,Math.round(r.second*fps)),strength:r.strength})):[];
    intent={...base,...(pipeline==='i2v'?{firstFrame:inputImage}:{}),...last,...source,family:'ltx-2',...(latentEnabled?{latentUpscale:{model:latentModel,scale:2 as const}}:{}),ltx:{textProjection,audioVae,passes:video.passes,...(video.passes>1?{spatialUpscaler:video.upscaler}:{}),
     ...(keyframes.length?{keyframes}:{}),...('sourceVideo' in source?{sourceStrength}:{})}};
   }
   else if(vf==='wan')intent={...base,firstFrame:inputImage,...last,family:'wan',wan:{lowModel:lowOf(model!,family),shiftHigh:video.shiftHigh,shiftLow:video.shiftLow,refinerStep:video.refinerStep}};
   else if(pipeline==='v2v'||pipeline==='ref2v'){
    // H3 has no video-to-video mode: both run REF2VA, the source video (with or without its sound) as a video reference.
    const includeReferences=!cont?.continuity.sourceId||!!plan.useReferences;
    const picks:VideoRef[]=!includeReferences?[]:pipeline==='v2v'&&media?[{item:media,second:0,strength:1,mediaMode:sourceAudio?'video_audio':'video'}]:refs;
    const slots=slotsOf(picks);
    const items=picks.map((r,i)=>({type:r.item.media as 'image'|'video'|'audio',file:nameOf(r.item,true),slot:slots[i],...(r.item.media==='video'?{mediaMode:r.mediaMode}:{}),
     ...(r.item.media!=='image'&&r.trimStart!==undefined?{trimStart:r.trimStart}:{}),...(r.item.media!=='image'&&r.trimEnd!==undefined?{trimEnd:r.trimEnd}:{})}));
    intent={...base,family:'minimax-h3',h3:{mode:'REF2VA',audioVae,shiftVideo:video.shiftVideo,shiftAudio:video.shiftAudio,references:[],media:items,...scaling,...(cont?{continuity:{...cont.continuity,...(cont.continuity.sourceId?{durationSeconds:seconds}:{})}}:{}),
     ...(includeReferences&&pipeline==='ref2v'&&refmods.length?{refmods:refmods.map((r,i)=>({slot:i+1,name:r.name,strength:r.strength,...(r.description.trim()?{description:r.description.trim()}:{})}))}:{})}};
   }
   else{
    // The Director owns H3 references; they must live in the backend input folder, so dock files are copied there explicitly.
    const refs:{slot:0|1;image:string}[]=[];
    if(pipeline==='i2v'&&!lastOnly&&media?.path)refs.push({slot:0,image:nameOf(media,true)});
    if(lastFrame?.path)refs.push({slot:1,image:nameOf(lastFrame,true)});
    const mode=refs.length===0?'T2VA':refs.length===2?'FL2VA':refs[0].slot===0?'I2VA':'L2VA';
    intent={...base,family:'minimax-h3',h3:{mode,audioVae,shiftVideo:video.shiftVideo,shiftAudio:video.shiftAudio,references:refs,...(cont?{continuity:cont.continuity}:{}),...(refs.length?scaling:{})}};
   }
   if(vf==='minimax-h3'&&intent.family==='minimax-h3'&&latentEnabled)intent={...intent,latentUpscale:{model:latentModel,scale:latentPref.scale,steps:latentPref.steps,denoise:latentPref.denoise,...(h3LatentChanged(latentPref.advanced)?{advanced:h3LatentChanged(latentPref.advanced)}:{})}};
   return intent;
 }
 async function runVideo(naming:string,out:VideoOutput,capturedSeed:number,step?:{index:number;all:boolean}){
  if(!videoFamily||!family||!split||!model)return;
  if(step&&plan.source&&plan.source.endpoint!==endpoint){setError(t('studio.run.cont.sourceEndpointChanged'));setAutoPlan(false);return;}
  const next=step?nextStep(plan):null;
  const cont=next?{...next,continuity:{...next.continuity}}:null;
  if(cont&&!continuityReferences)delete cont.continuity.useReferences;
  if(step&&cont?.index!==step.index){setError(t('studio.run.cont.stale'));return;}
  observation.current++;setObservationRetry(n=>n+1);const ticket=beginRun();
  let admitted=false;
  try{
   const uploads=new Set<string>();
   const identify=(item:MediaItem,force=false)=>{const name=!force&&comfyName(item);if(name)return name;uploads.add(item.path!);return item.path!;};
   const draft={...structuredClone(buildVideoIntent(videoFamily,naming,out,identify,cont)),seed:capturedSeed};
   const names=new Map(await Promise.all([...uploads.keys()].map(async path=>[path,await uploadInput(endpoint,path)] as const)));
   // Replace only upload placeholders in the captured draft, never rebuild from controls after awaits.
   const intent=JSON.parse(JSON.stringify(draft),(key,value:unknown)=>['image','file','sourceVideo'].includes(key)&&typeof value==='string'&&names.has(value)?names.get(value):value) as VideoIntentV1;
   if(!activeRun(ticket))return;
   admitted=true;
   const job=await runWorkbenchJob(endpoint,Object.freeze(intent),ticket.admissionEpoch);
   if(activeRun(ticket)){
    setJobs(list=>[job,...list.filter(j=>j.id!==job.id)]);setBrowsed(null);setSelectedOutput(0);setSlider(false);setSelectedJob(job.id);setRecentSeeds(list=>[intent.seed,...list.filter(s=>s!==intent.seed)].slice(0,10));
    if(cont&&step){planRun.current={jobId:job.id,...step,...(cont.continuity.sourceKind==='video'&&intent.family==='minimax-h3'?{imported:{sourceId:cont.continuity.sourceId,width:intent.width,height:intent.height,modeFamily:intent.h3.mode==='REF2VA'?'ref2va' as const:'fl2va' as const}}:{})};setPlan({...plan,steps:plan.steps.map((x,i)=>i===cont.index?{...x,jobId:job.id}:x)});}
   }
   if(cont&&step&&activeRun(ticket)){
    if(job.state==='succeeded')void pinClip(job);
    else if(job.state==='failed'||job.state==='cancelled'||job.state==='unknown'){planRun.current=null;setAutoPlan(false);}
   }
  }catch(e){if(activeRun(ticket)){setError(e instanceof Error?e.message:String(e));setAutoPlan(false);}}
  finally{finishRun(ticket,admitted);}
 }
 // Planner: after a step's job succeeded, read the session once and pin the newest clip that continues the previous one.
 async function pinClip(job:JobStatus){
  const generation=preparationGeneration.current;const step=planRun.current;
  if(!step||step.jobId!==job.id)return;
  planRun.current=null;
  setResolving(true);
  try{
   const session=await readH3Session(job.baseUrl,plan.session);
   const clipId=resolveClip(session.clips,continuitySource(plan,step.index),step.imported);
   if(!live.current||generation!==preparationGeneration.current)return;
   if(!clipId)throw new Error(t('studio.run.cont.noClip'));
   setPlan({...plan,steps:plan.steps.map((x,i)=>i===step.index?{...x,clipId,jobId:job.id}:x)});
  }catch(e){if(live.current&&generation===preparationGeneration.current){setError(e instanceof Error?e.message:String(e));setAutoPlan(false);}}
  finally{if(live.current&&generation===preparationGeneration.current)setResolving(false);}
 }
 const [observationRetry,setObservationRetry]=useState(0);
 useEffect(()=>{
  if(!native||stopLock.current||(recovered&&!current&&!preparing&&!frontendAdmitting&&!stopping))return;
  let cancelled=false;const version=observation.current;
  const observe=async()=>{
   try{
    const next=await (recovered&&!stopping&&(!frontendAdmitting||!!current)?pollWorkbench():readCurrentWorkbench());
    if(cancelled||!live.current||version!==observation.current||stopLock.current||next.admissionEpoch<admissionEpoch.current)return;
    admissionEpoch.current=next.admissionEpoch;setStopping(next.stopping);setStopAvailable(next.stopAvailable);setPreparing(next.preparing);setFrontendAdmitting(!!next.frontendAdmitting);setRecovered(true);
    // Native order is oldest first; the queue displays newest first without truncating active peers.
    setJobs([...next.jobs].reverse());
    if(!recovered)setSelectedJob(id=>id||next.jobs.at(-1)?.id||'');
    for(const job of next.jobs){
     if(job.state==='succeeded'&&jobs.find(j=>j.id===job.id)?.state!=='succeeded')setOutputRevision(r=>r+1);
     if(job.id!==planRun.current?.jobId)continue;
     if(job.state==='succeeded')void pinClip(job);
     else if(job.state==='failed'||job.state==='cancelled'||job.state==='unknown'){planRun.current=null;setAutoPlan(false);}
    }
   }catch(e){
    if(cancelled||!live.current||version!==observation.current)return;
    setError(e instanceof Error?e.message:String(e));setObservationRetry(n=>n+1);
   }
  };
  const timer=recovered||observationRetry?window.setTimeout(()=>void observe(),1000):undefined;
  if(timer===undefined)void observe();
  return()=>{cancelled=true;window.clearTimeout(timer);};
 },[jobs,native,working,stopping,preparing,frontendAdmitting,recovered,observationRetry]); // eslint-disable-line react-hooks/exhaustive-deps
 // Run all: start the next planned step once the previous clip is pinned and nothing is in flight.
 useEffect(()=>{
  if(!autoPlan||stopLock.current||current||admissionBlocked||serialBlocked||resolving)return;
  const next=nextStep(plan);
  if(!next||!continuityOn){setAutoPlan(false);return;}
  void run(undefined,{index:next.index,all:true});
 },[autoPlan,current,admissionBlocked,serialBlocked,busy,resolving,plan,continuityOn]); // eslint-disable-line react-hooks/exhaustive-deps
 const runPlan=(all:boolean)=>{if(stopLock.current||admissionBlocked||serialBlocked||resolving||historyProblems.length||unavailable.length)return;const next=nextStep(plan);if(!next||!continuityOn)return;if(all)setAutoPlan(true);else void run(undefined,{index:next.index,all:false});};
 async function stop(){
  if(stopLock.current)return;
  stopLock.current=true;setStopping(true);setAutoPlan(false);planRun.current=null;
  preparationGeneration.current++;observation.current++;localPreparations.current=0;lock.current=false;setBusy(false);setResolving(false);setError('');setStopStatus('');
  try{
   if(!stopAvailable)throw new Error(t('studio.run.stop.unsupported'));
   const next=await stopWorkbench();
   if(!live.current)return;
   admissionEpoch.current=next.admissionEpoch;setJobs([...next.jobs].reverse());setPreparing(next.preparing);setFrontendAdmitting(!!next.frontendAdmitting);setRecovered(true);
   for(const job of next.jobs)stopped.current.add(job.id);
   // Unresolved jobs settle via polling or stay dismissed; details go to the native log only.
   if(next.outcome!=='partial')setStopStatus(t(`studio.run.stop.${next.outcome}`));
  }catch(e){if(live.current){setError(`${t('studio.run.stop.failed')} ${e instanceof Error?e.message:String(e)}`);setRecovered(false);}}
  finally{stopLock.current=false;if(live.current)setStopping(false);}
 }
 // Ctrl+Enter presses Run itself, so a disabled Run stays disabled. Not from a retained hidden tab or over a dialog.
 const runButton=useRef<HTMLButtonElement>(null);
 useEffect(()=>{
  const key=(e:KeyboardEvent)=>{
   if(e.key!=='Enter'||!(e.ctrlKey||e.metaKey)||e.shiftKey||e.altKey||e.repeat||e.isComposing)return;
   const button=runButton.current;
   if(!button||button.closest('[hidden]')||document.querySelector('dialog[open]'))return;
   e.preventDefault();button.click();
  };
  window.addEventListener('keydown',key);return()=>window.removeEventListener('keydown',key);
 },[]);
 // Right-click on an active queue entry offers stopping just that generation.
 const [jobMenu,setJobMenu]=useState<{x:number;y:number;job:JobStatus;trigger:HTMLButtonElement}|null>(null);
 useEffect(()=>{if(!jobMenu)return;const close=()=>setJobMenu(null);const key=(e:KeyboardEvent)=>{if(e.key==='Escape'){e.preventDefault();jobMenu.trigger.focus();close();}};
  window.addEventListener('click',close);window.addEventListener('keydown',key);window.addEventListener('resize',close);
  return()=>{window.removeEventListener('click',close);window.removeEventListener('keydown',key);window.removeEventListener('resize',close);};},[jobMenu]);
 async function stopJob(job:JobStatus){
  try{const next=await stopWorkbenchJob(job);if(!live.current)return;stopped.current.add(job.id);setJobs(list=>list.map(j=>j.id===next.id?next:j));}
  catch(e){if(live.current)setError(`${t('studio.run.stop.failed')} ${e instanceof Error?e.message:String(e)}`);}
 }
 // Picking media on the strip takes over the preview until the next job is selected.
 const select=(item:MediaItem)=>{if(historyDraft)setHistorySourceSelected(true);setBrowsed(null);setSelectedJob('');if(media?.src===item.src)return;sourceEpoch.current++;setEditorSnapshot(undefined);setMedia(item);setLayers([]);setMasked(false);};
 // A dock tile dropped on the editor becomes a layer at its original size, centred on the drop point (image-to-image only).
 const insertLayer=(item:MediaItem,at:{x:number;y:number})=>{
  if(!source||!media)return;
  const name=comfyName(item),version=sourceEpoch.current;
  if(!name||item.media!=='image'||layers.length>=4){setError(t('studio.run.edit.insertHint'));return;}
  if(pipeline!=='i2i'){pickPipeline('i2i',true);setEdit('resample');}
  const draftVersion=draftEpoch.current;
  const probe=new Image();
  probe.onload=()=>{
   if(!live.current||sourceEpoch.current!==version||draftEpoch.current!==draftVersion)return;
   if(!(probe.naturalWidth>0&&probe.naturalHeight>0)){setError(t('studio.editor.insertFailed'));return;}
   const scale=Math.min(1,8192/probe.naturalWidth,8192/probe.naturalHeight); // original pixel size, capped at the 8192 px layer limit
   const width=Math.max(8,Math.round(probe.naturalWidth*scale)),height=Math.max(8,Math.round(probe.naturalHeight*scale));
   const cx=Number.isFinite(at.x)?at.x:source.width/2,cy=Number.isFinite(at.y)?at.y:source.height/2;
   const layer:ImageLayer={id:crypto.randomUUID(),name,src:item.src,visible:true,width,height,x:Math.max(1-width,Math.min(source.width-1,Math.round(cx-width/2))),y:Math.max(1-height,Math.min(source.height-1,Math.round(cy-height/2)))};
   setLayers(list=>list.length>=4?list:[...list,layer]);
  };
  probe.onerror=()=>{if(live.current&&sourceEpoch.current===version&&draftEpoch.current===draftVersion)setError(t('studio.editor.insertFailed'));};
  probe.src=item.src;
 };
 // The shared shell owns viewport sizing; observe this pane only to bound dock resizing.
 const shell=useRef<HTMLDivElement>(null);const [bodyHeight,setBodyHeight]=useState(600);
 useEffect(()=>{if(browsed)shell.current?.querySelectorAll<HTMLMediaElement>('.wb-current-preview video, .wb-current-preview audio').forEach(el=>{if(!el.paused)el.pause();});},[browsed?.src]);
 useLayoutEffect(()=>{
  const measure=()=>{const el=shell.current;if(el?.clientWidth)setBodyWidth(el.clientWidth);if(el?.clientHeight)setBodyHeight(el.clientHeight);};measure();
  window.addEventListener('resize',measure);
  const ro=typeof ResizeObserver==='undefined'?null:new ResizeObserver(measure);if(shell.current)ro?.observe(shell.current);
  return()=>{window.removeEventListener('resize',measure);ro?.disconnect();};
 },[]);
 const sideSize=clamp(settingsWidth,220,Math.max(220,bodyWidth-128-320-20));
 const queueSize=clamp(queueWidth,128,Math.max(128,bodyWidth-sideSize-320-20));
 const sideMax=Math.max(220,bodyWidth-queueSize-320-20),queueMax=Math.max(128,bodyWidth-sideSize-320-20);
 const dockMax=Math.max(120,bodyHeight-260),dockSize=clamp(dockHeight,120,dockMax);
 const dragCleanup=useRef<()=>void>(()=>{});
 useEffect(()=>()=>dragCleanup.current(),[]);
 const dragPane=(e:React.PointerEvent<HTMLDivElement>,start:number,min:number,max:number,set:(n:number)=>void,vertical=false,reverse=false)=>{
  if(e.button!==0)return;e.preventDefault();dragCleanup.current();
  const el=e.currentTarget,id=e.pointerId,at=vertical?e.clientY:e.clientX,sign=vertical||reverse?-1:1;
  el.setPointerCapture?.(id);
  const stop=()=>{window.removeEventListener('pointermove',move);window.removeEventListener('pointerup',up);window.removeEventListener('pointercancel',cancel);window.removeEventListener('keydown',escape);el.removeEventListener('lostpointercapture',cancel);if(el.hasPointerCapture?.(id))el.releasePointerCapture(id);dragCleanup.current=()=>{};};
  const move=(ev:PointerEvent)=>{if(ev.pointerId===id)set(clamp(start+((vertical?ev.clientY:ev.clientX)-at)*sign,min,max));};
  const up=(ev:PointerEvent)=>{if(ev.pointerId===id)stop();};
  const cancel=()=>{set(start);stop();};
  const escape=(ev:KeyboardEvent)=>{if(ev.key==='Escape'){ev.preventDefault();cancel();}};
  window.addEventListener('pointermove',move);window.addEventListener('pointerup',up);window.addEventListener('pointercancel',cancel);window.addEventListener('keydown',escape);el.addEventListener('lostpointercapture',cancel);dragCleanup.current=stop;
 };
 const resizeKey=(e:React.KeyboardEvent,start:number,min:number,max:number,set:(n:number)=>void,vertical=false,reverse=false)=>{
  const delta=(e.key===(vertical?'ArrowUp':'ArrowRight')?1:e.key===(vertical?'ArrowDown':'ArrowLeft')?-1:0)*(reverse?-1:1);
  if(e.key==='Home'||e.key==='End'||delta){e.preventDefault();set(e.key==='Home'?min:e.key==='End'?max:clamp(start+delta*(e.shiftKey?64:16),min,max));}
 };
 // Arrow on a pane's edge line: fully hides the pane (state stays mounted) and shows it again.
 const paneToggle=(hidden:boolean,setHidden:(v:boolean)=>void,pane:'settings'|'dock',controls:string,left?:number)=>{const label=t(`studio.layout.${pane}${hidden?'Expand':'Collapse'}`);return <button type="button" className="wb-pane-toggle" data-pane-toggle={pane} data-dir={pane==='settings'?(hidden?'right':'left'):(hidden?'up':'down')} aria-expanded={!hidden} aria-controls={controls} aria-label={label} title={label} style={left===undefined?undefined:{left}} onClick={()=>{dragCleanup.current();setHidden(!hidden);}}><svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round"><path d="m6 9 6 6 6-6"/></svg></button>;};
 const columnHandle=(side:'settings'|'queue')=>{const left=side==='settings',size=left?sideSize:queueSize,min=left?220:128,max=left?sideMax:queueMax,set=left?setSettingsWidth:setQueueWidth,locked=left&&settingsHidden;return <div className="wb-column-handle" role="separator" tabIndex={0} aria-orientation="vertical" aria-label={t(`studio.layout.${side}`)} title={t(`studio.layout.${side}`)} aria-controls={`studio-${side}`} aria-valuemin={min} aria-valuemax={max} aria-valuenow={size} data-locked={locked||undefined} onKeyDown={e=>{if(!locked)resizeKey(e,size,min,max,set,false,!left);}} onPointerDown={e=>{if(!locked)dragPane(e,size,min,max,set,false,!left);}}/>;};
 const shown=jobs.find(j=>j.id===selectedJob)??null;
 const paired=!!shown?.outputs.some(o=>o.role);
 const outputIndex=paired&&shown?.outputs[selectedOutput]?selectedOutput:0;
 const visibleOutputs=shown?(paired?[shown.outputs[outputIndex]]:shown.outputs):[];
 const terminalOutput=(job:JobStatus)=>(job.state==='succeeded'||job.state==='failed'||job.state==='cancelled')&&job.outputs.length>0;
 const duration=(job:JobStatus)=>{const value=generationDuration(job,locale);return value!==null?<span data-generation-duration> · {value}</span>:null;};
 // Metadata is an overlay on the preview, opened on demand, so the stage keeps the column's width.
 const [metaOpen,setMetaOpen]=usePref('studio.run.metadataOpen',false,isBoolean);
 // A queue pick of an edit job brings its source back into the editor (with the job's mode) when no other draft is open,
 // e.g. after ✕ or browsing elsewhere; an open draft with masks or inserted layers is never replaced.
 const pickJob=(j:JobStatus,index=0)=>{
  setSelectedOutput(index);setSlider(false);
  setBrowsed(null);
  setSelectedJob(j.id); // Preview only: restoring a job's settings is the explicit Reuse action.
 };
 const closePreview=()=>{if(browsed){setBrowsed(null);return;}setSelectedJob('');sourceEpoch.current++;setEditorSnapshot(undefined);setMedia(null);setLayers([]);setMasked(false);};
 const pairedOriginal=shown?.outputs.find(o=>o.role==='original');
 const pairedProcessed=shown?.outputs.find(o=>o.role==='processed');
 const originalItem=shown?sources[shown.id]:undefined;
 const original=paired?(shown?.outputRoot&&pairedOriginal&&pairedProcessed?convertFileSrc([shown.outputRoot,pairedOriginal.subfolder,pairedOriginal.filename].filter(Boolean).join('/')):undefined):shown?sources[shown.id]?.src:undefined;
 // Edit jobs open side by side: the live editor (masks/layers kept until a new image is sent to edit) left, the render right.
 // While an edit draft is open, any queue pick opens beside it, so its controls never vanish behind a result.
 const sideBySide=!paired&&!!shown&&(!!original||(pipeline==='i2i'&&!!editableSource))&&!(slider&&original);
 const editorLeft=sideBySide&&!!editableSource&&(!original||original===media?.src);
 // What still blocks Run is shown on the preview, where the missing pick or mask belongs (the mask only while the editor is visible).
 const hint=backendPending?'studio.run.backendPending':sizeError||(inpaint&&!masked&&(!shown||editorLeft)?'studio.run.edit.needsMask':'');
 // Click a render (or a browsed picture) for a full-window, zoomable view.
 const [viewer,setViewer]=useState<{src:string;alt:string;item?:MediaItem}|null>(null); const fileSrc=(job:JobStatus,o:JobStatus['outputs'][number])=>job.outputRoot?convertFileSrc([job.outputRoot,o.subfolder,o.filename].filter(Boolean).join('/')):null;
 const openImage=(event:React.MouseEvent<HTMLImageElement>,item:MediaItem)=>{
  const image=event.currentTarget;
  if(containedImageHit({x:event.clientX,y:event.clientY},{width:image.naturalWidth,height:image.naturalHeight},image.getBoundingClientRect()))setViewer({src:item.src,alt:item.name,item});
 };
 // Local outputs retain their captured root; rootless playback stays bound to the job endpoint (Linux uses NativeVideo).
 // Linux plays audio through libmpv like video: WebKitGTK has no FLAC decoder and never plays asset:// media.
 const audioPlayer=(item:MediaItem,src:string|undefined,active=true,output=false)=>linuxVideo&&item.path?<NativeVideo key={item.path} source={{path:item.path}} label={item.name} active={active} audioOnly/>
  :<audio key={item.src} data-output-audio={output||undefined} src={src} controls aria-label={item.name} onError={e=>setMediaFault(mediaError(e))}/>;
 const videoSrc=(job:JobStatus,o:JobStatus['outputs'][number])=>fileSrc(job,o)??`${job.baseUrl}view?${new URLSearchParams({filename:o.filename,subfolder:o.subfolder,type:o.type})}`;
 const num=(id:string,value:number,set:(n:number)=>void,min:number,max:number,step:number,fixed?:string,slider=false)=><label key={id} className="wb-field"><span>{t(`studio.run.${id}`)}</span><input data-num={id} type="number" inputMode="numeric" value={value} min={min} max={max} step={step} disabled={!!fixed} title={fixed} onChange={e=>{const n=Number(e.target.value);if(Number.isFinite(n))set(n);}}/>
  {/* Pointer shortcut for the number field above, which stays the labelled, keyboard-reachable control. */}
  {slider&&<input type="range" className="wb-slider" aria-hidden="true" tabIndex={-1} value={value} min={min} max={max} step={step} disabled={!!fixed} onChange={e=>set(Number(e.target.value))}/>}</label>;
 const outputItem=(job:JobStatus,o:JobStatus['outputs'][number]):MediaItem=>({name:o.filename,media:o.format?.startsWith('audio/')||/\.(wav|mp3|flac|ogg|m4a)$/i.test(o.filename)?'audio':isVideoOut(o)?'video':'image',src:fileSrc(job,o)??'',...(job.outputRoot?{path:[job.outputRoot,o.subfolder,o.filename].filter(Boolean).join('/')}:{})});
 useEffect(()=>{
  const moved=(event:Event)=>{
   const move=(event as CustomEvent<MediaRelocation>).detail;
   if(!move||typeof move.oldPath!=='string'||typeof move.path!=='string')return;
   const oldSrc=native?convertFileSrc(move.oldPath):move.oldPath,newSrc=native?convertFileSrc(move.path):move.path;
   const repair=(item:MediaItem|null)=>item?relocatedMediaItem(item,move):null;
   setMedia(repair);setBrowsed(repair);setLastFrame(repair);
   setMediaSize(value=>value?.src===oldSrc?{...value,src:newSrc}:value);
   setSources(values=>Object.fromEntries(Object.entries(values).map(([key,item])=>[key,relocatedMediaItem(item,move)])));
   setRefs(values=>values.map(ref=>({...ref,item:relocatedMediaItem(ref.item,move)})));
   setFramePeople(values=>{if(!values[oldSrc])return values;const next={...values,[newSrc]:values[oldSrc]};delete next[oldSrc];return next;});
   setJobs(values=>values.map(job=>job.outputRoot?relocatedJob(job,job.outputRoot,move):job));
   if(outputDir){
    const relative=move.path.replace(/\\/g,'/').slice(outputDir.replace(/\\/g,'/').replace(/\/+$/,'').length+1);
    setLayers(values=>values.map(layer=>layer.src===oldSrc?{...layer,src:newSrc,name:`${relative} [output]`}:layer));
   }
   setViewer(value=>value?.src===oldSrc?{src:newSrc,alt:move.path.split(/[\\/]/).at(-1)!,item:value.item?relocatedMediaItem(value.item,move):undefined}:value);
  };
  window.addEventListener('deas-media-relocated',moved);return()=>window.removeEventListener('deas-media-relocated',moved);
 },[native,outputDir]);
 // Post-process takes the shown render (its submitted size), else the current edit source.
 const isVideoOut=(o:JobStatus['outputs'][number])=>!!o.format?.startsWith('video/')||/\.(webm|mkv|mp4|avif)$/i.test(o.filename);
 const shownOutput=shown&&terminalOutput(shown)&&shown.outputRoot&&shown.outputs[outputIndex]&&shown.intent&&isImageIntent(shown.intent)?outputItem(shown,shown.outputs[outputIndex]):null;
 const postFor=(item:MediaItem)=>shownOutput?.src===item.src&&shown?.intent&&isImageIntent(shown.intent)?{image:'',size:shown.outputs[outputIndex]?.role==='original'?shown.intent:shown.intent.refine??shown.intent,src:shownOutput.src,item:shownOutput}
  :media?.src===item.src&&inputImage&&source?{image:inputImage,size:source,src:media.src,item:media}
  :item.media==='image'&&item.src&&item.path?{image:'',size:outputSize,src:item.src,item}:null; // any other image (browsed dock file): upload by path, canvas-size target
 // Floating edit buttons over an editable image: a mode makes it the image-to-image source and opens that mode's controls on the left.
 const noEditModel=!allModels.some(m=>!blocked(m,'i2i'));
 // References: LTX keyframes are images; H3 REF2VA also takes videos and audio within the Director limits. Returns why an item cannot be added.
 const isRef=(item:MediaItem)=>refs.some(r=>r.item.src===item.src);
 const refBlock=(item:MediaItem,list=refs)=>videoFamily!=='minimax-h3'?(item.media==='image'?'':'studio.run.video.refImagesOnly'):h3Full(list,item)?'studio.run.video.refFull':'';
 // Adds in order; already referenced or path-less (browser) items are skipped, refused ones are named with the reason.
 const addRefs=(adds:RefAdd[],replace=false)=>{
  let next=replace?[]:refs;const refused:string[]=[];
  for(const {item,...rest} of adds){if(!item.path||next.some(r=>r.item.src===item.src))continue;
   const ref:VideoRef={item,second:next.length?video.seconds:0,strength:1,mediaMode:'video',...rest};
   // Limits count the reference as it will be used (a pack may bring a sound-only video).
   const why=videoFamily!=='minimax-h3'?refBlock(item,next):over(refCounts([...next,ref]))?'studio.run.video.refFull':'';
   if(why)refused.push(`${item.name}: ${t(why)}`);else next=[...next,ref];}
  setRefs(next);setError(refused.join(' · '));
 };
 const toggleRef=(item:MediaItem)=>isRef(item)?setRefs(refs.filter(r=>r.item.src!==item.src)):addRefs([{item}]);
 // H3 badges are the Director's own prompt labels, so what the dock shows is what the prompt may reference.
 const refTags=directorTags(refs);
 const refLabel=(r:VideoRef,i:number)=>videoFamily==='ltx-2'?t('studio.run.video.mark.at',{second:String(r.second)}):refTags[i];
 const refsTab=pipeline==='ref2v'&&(videoFamily==='ltx-2'||videoFamily==='minimax-h3');
 useEffect(()=>{if(!refsTab||planVisible)setReferencesOpen(false);},[refsTab,planVisible]);
 // First/last-frame badges; one image in both roles gets both labels.
 const frameMarks=new Map<string,string>();
 for(const [item,key] of [[firstFrame,'forge.role.first-frame'],[lastFrame,'forge.role.last-frame']] as const)if(item?.path)frameMarks.set(item.path,frameMarks.has(item.path)?`${frameMarks.get(item.path)} + ${t(key)}`:t(key));
 const marks=new Map(pipeline==='ref2v'?refs.flatMap((r,i)=>r.item.path?[[r.item.path,refLabel(r,i)] as const]:[]):pipeline==='v2v'&&media?.media==='video'&&media.path?[[media.path,t('studio.run.video.source')] as const]:pipeline==='cover'?(musicSource?.path?[[musicSource.path,t('studio.music.source')] as const]:[]):frameVideo?[...frameMarks]:imageRefs.flatMap(r=>r.item.path?[[r.item.path,t('studio.run.reference.remove')] as const]:[]));
 const togglePromptRef=(item:MediaItem&{path:string})=>{const at=imageRefs.findIndex(r=>r.item.path===item.path);if(at>=0){setImageRefs(v=>v.filter((_,j)=>j!==at));return;}
  setImageRefs(v=>v.length>=4?v:[...v,{item:{...item},strength:0.6}]);
  if(revisionOk&&native&&!(choices?.visionModels?.length??0))setVisionDownload(true);};
 const inputActions=(item:MediaItem):MediaAction[]=>!item.path?[]:pipeline==='ref2v'&&videoFamily?[isRef(item)?{label:t('studio.run.video.removeRef',{name:item.name}),onClick:()=>toggleRef(item)}:{label:t('studio.run.video.addRef'),onClick:()=>toggleRef(item),blocked:refBlock(item)&&t(refBlock(item))}]
  // Cover sources are audio files or videos with an audio track (core LoadAudio reads the first stream); images cannot be covered.
  :pipeline==='cover'&&(item.media==='audio'||item.media==='video')?[musicSource?.path===item.path?{label:t('studio.music.removeSource'),input:true,onClick:()=>{setMusicSource(null);if(historyDraft)setHistorySourceSelected(false);}}:{label:t('studio.music.useSource'),input:true,onClick:()=>{setMusicSource(item);if(historyDraft)setHistorySourceSelected(true);}}]
  :referenceMethod&&item.media==='image'&&validReference(item)?[imageRefs.some(r=>r.item.path===item.path)?{label:t('studio.run.reference.remove'),onClick:()=>togglePromptRef(item)}:{label:t('studio.run.reference.use'),onClick:()=>togglePromptRef(item),blocked:(imageRefs.length>=4&&t('studio.run.reference.max',{count:imageRefs.length,max:4}))||undefined}]
  // Frame roles like the Director: first (i2v input), last, or the same image as both; t2v takes a last frame only.
  // H3 can run on a last frame alone (L2VA), so there "last frame" moves the role off the input instead of adding it.
  :frameVideo&&item.media==='image'?[
   ...(pipeline==='i2v'?[{label:t('studio.run.video.useFirst'),input:true,onClick:()=>{select(item);setLastOnlySrc(null);if(lastFrame?.src===item.src)setLastFrame(null);}}]:[]),
   lastFrame?.src===item.src?{label:t('studio.run.video.removeLast'),onClick:()=>{setLastFrame(null);setLastOnlySrc(null);}}
    :{label:t('studio.run.video.useLast'),onClick:()=>{if(pipeline==='i2v'&&videoFamily==='minimax-h3'&&(lastOnly||!firstFrame||media?.src===item.src)){select(item);setLastOnlySrc(item.src);}setLastFrame(item);}},
   ...(pipeline==='i2v'?[{label:t('studio.run.video.useBoth'),onClick:()=>{select(item);setLastOnlySrc(null);setLastFrame(item);}}]:[])]
  :pipeline==='v2v'&&item.media==='video'?[{label:t('studio.run.video.useSource'),input:true,onClick:()=>select(item)}]:[];
 const mediaActions=(item:MediaItem):MediaAction[]=>[...inputActions(item),...(item.path&&item.media!=='audio'?[{label:t('studio.run.metadata.apply'),onClick:()=>void applyFileSettings(item,'all'),blocked:(!native||scanning||working||autoPlan||resolving||!!planRun.current)?t('studio.run.metadata.native'):undefined}]:[])];
 const stageMark=media?.path&&marks.get(media.path);
 // MiniMax H3 video: the hammer is the Director's Prompt Forge for this run (same mode/references the intent gets).
 const forgeContext=videoFamily==='minimax-h3'&&isVideo?h3ForgeContext({pipeline,media:lastOnly?null:media,lastFrame,refs,framePeople,sourceAudio,seconds:video.seconds,canvas:outputSize,inputRoot:roots.input}):undefined;
 useEffect(()=>{onH3ForgeAvailable?.(!!forgeContext);},[!!forgeContext,onH3ForgeAvailable]);
 // Insert at the cursor of the positive prompt (else append), like the node's toolbar.
 const insertPrompt=(text:string)=>{const el=document.getElementById('run-positive') as HTMLTextAreaElement|null;const at=el&&document.activeElement===el?el.selectionStart:positive.length;setPositive(positive.slice(0,at)+text+positive.slice(at));};
 const prefill=()=>{
  if(refmods.some(r=>!r.kinds?.length)){setError(t('studio.run.prompt.prefillKinds'));return;}
  const picks=pipeline==='v2v'?(media?.media==='video'?[{type:'video' as const,slot:0,mediaMode:sourceAudio?'video_audio' as const:'video' as const}]:[]):prefillItems(refs);
  const fields=refPrefill(picks,pipeline==='ref2v'?refmods.map((r,i)=>({slot:i+1,kinds:r.kinds!,description:r.description})):[]);
  if(!fields){setError(t('studio.run.prompt.prefillEmpty'));return;}
  setPositive(applyRefPrefill(positive,fields));
};
// Director prompt helpers live inside the hammer panel (H3 video only); insert at the cursor of the positive prompt (else append), like the node's toolbar.
const promptTools=forgeContext&&<>
 <button type="button" data-insert-structure onClick={()=>insertPrompt(promptStructure(forgeContext.mode))}>{t('studio.run.prompt.structure')}</button>
 <button type="button" data-insert-shot onClick={()=>insertPrompt(nextShot(positive))}>{t('studio.run.prompt.shot')}</button>
 {forgeContext.mode==='REF2VA'&&<button type="button" data-prefill onClick={prefill}>{t('studio.run.prompt.prefill')}</button>}</>;
 const tools=(item:MediaItem,showStar=true)=>{const actions=mediaActions(item);return <div className="wb-edit-tools wb-media-tools" data-media-tools role="toolbar" aria-label={t('studio.run.edit.title')}>
  {/* Every right-click media action is mirrored here (same label, effect and block reason), so new menu options appear in the preview automatically. */}
  {!actions.some(a=>a.input)&&<button type="button" data-use-input onClick={()=>select(item)}>{t('studio.run.useInput')}</button>}
  {actions.map(a=><button key={a.label} type="button" data-media-action data-media-settings={a.label===t('studio.run.metadata.apply')||undefined} data-use-input={a.input||undefined} disabled={!!a.blocked} title={a.blocked||undefined} onClick={a.onClick}>{a.label}</button>)}
  {item.media==='image'&&editModes.map(m=><button key={m} type="button" data-edit={m} aria-pressed={pipeline==='i2i'&&edit===m&&media?.src===item.src} disabled={noEditModel} title={noEditModel?t('studio.run.noModels'):undefined} onClick={()=>{if(pipeline!=='i2i')pickPipeline('i2i',true);select(item);setEdit(m);}}>{t(`studio.run.edit.${m}`)}</button>)}
 {/* More: Refine = the refine+upscale pass alone, Upscale = the upscale pass alone (no diffusion) — both use the Post-Processing settings on the shown image. Metadata actions mirror the dock's "use these settings" and add a prompt-only refill. */}
  {/* More stays on every image; Refine/Upscale are disabled with a reason outside the image-model pipelines. */}
  {item.media==='image'&&(()=>{const only=postFor(item);const off=!native||scanning||admissionBlocked||autoPlan||resolving||!!planRun.current||historyProblems.length>0;const metaOff=off||!item.path;const postOff=isVideo||h3Image||moss||!only;return <details ref={moreRef} className="wb-more" open={moreOpen} onToggle={e=>{if(e.currentTarget.open!==moreOpen)setMoreOpen(e.currentTarget.open);}}><summary data-more aria-haspopup="menu" onClick={e=>{e.preventDefault();setMoreResult(null);setMoreOpen(!moreOpen);}}>{t('studio.run.more')}<span className="wb-more-caret" aria-hidden="true">▾</span></summary><div className="wb-more-menu">
   <button type="button" data-run-post disabled={off||postOff} title={t(postOff?'studio.run.post.imageOnly':'studio.run.post.refineHint')} onClick={()=>{if(only)selectMoreAction(true,()=>void run(only));}}>{t('studio.run.post.refine')}</button>
   <button type="button" data-run-upscale disabled={off||postOff} title={t(postOff?'studio.run.post.imageOnly':'studio.run.post.upscaleHint')} onClick={()=>{if(only)selectMoreAction(true,()=>void run({...only,upscaleOnly:true}));}}>{t('studio.run.post.upscale')}</button>
   <button type="button" data-more-apply disabled={metaOff} title={metaOff?t('studio.run.metadata.native'):t('studio.run.metadata.applyHint')} onClick={()=>selectMoreAction(true,()=>void applyFileSettings(item,'all'))}>{t('studio.run.metadata.apply')}</button>
   <button type="button" data-more-reuse disabled={metaOff} title={metaOff?t('studio.run.metadata.native'):t('studio.run.metadata.reuseHint')} onClick={()=>selectMoreAction(true,()=>void applyFileSettings(item,'prompt'))}>{t('studio.run.metadata.reuse')}</button></div></details>;})()}
  {item.media==='video'&&(()=>{const off=!native||!item.path||scanning||admissionBlocked||autoPlan||resolving||!!planRun.current;const hint=!native?t('studio.run.metadata.native'):'';return <details ref={moreRef} className="wb-more" open={moreOpen} onToggle={e=>{if(e.currentTarget.open!==moreOpen)setMoreOpen(e.currentTarget.open);}}><summary data-more aria-haspopup="menu" onClick={e=>{e.preventDefault();setMoreResult(null);setMoreOpen(!moreOpen);}}>{t('studio.run.more')}<span className="wb-more-caret" aria-hidden="true">▾</span></summary><div className="wb-more-menu">
   <button type="button" data-more-apply disabled={off} title={hint||t('studio.run.metadata.videoApplyHint')} onClick={()=>selectMoreAction(true,()=>void applyFileSettings(item,'all'))}>{t('studio.run.metadata.apply')}</button>
   <button type="button" data-more-reuse disabled={off} title={hint||t('studio.run.metadata.videoReuseHint')} onClick={()=>selectMoreAction(true,()=>void applyFileSettings(item,'prompt'))}>{t('studio.run.metadata.reuse')}</button></div></details>;})()}
  {original&&(paired||media?.src!==item.src)&&<button type="button" className="wb-edit-view" data-edit-compare aria-pressed={slider} onClick={()=>setSlider(!slider)}>{t('studio.run.edit.slider')}</button>}
  {showStar&&<StarMediaButton item={item}/>}
 </div>;};
 // Canonical preview toolbar: one stage slot (top right, beside Metadata/Close) fed by whatever the stage shows.
 // Only the split view keeps a second, per-pane row for the draft editor.
 const stageTools=()=>browsed?tools(browsed)
  :shown?(terminalOutput(shown)&&shown.outputRoot&&shown.outputs[outputIndex]?tools(outputItem(shown,shown.outputs[outputIndex]),false):null)
  :media?(inputImage||(!editableSource&&pipeline==='ref2v'&&media.path)?tools(media):editableSource?null:<div className="wb-media-tools" data-media-tools><StarMediaButton item={media}/></div>)
  :null;
 const modelValue=(m:LocalModel)=>allModels.filter(other=>other.name===m.name).length>1?identity(m):m.name;
 const toggleLora=(name:string,on:boolean)=>setLoras(on?[...loras.filter(l=>l.name!==name),{name,strength:1}]:loras.filter(l=>l.name!==name));
 // Session drafts are independent of persisted global defaults and immutable submitted jobs.
 const scope=JSON.stringify([root,modelsDir,extraModelsKey,endpoint,roots.input,outputDir,project?.id]);
 const captureDraft=()=>{
  const settings=captureStudioSettings();
  for(const key of ['studio.run.checkpoint','studio.run.pipeline','studio.run.brush'])delete settings[key];
  return {settings,media,mediaSize,layers:layers.map(copyImageLayer),editor:editorRef.current?.capture()??editorSnapshot,masked,lastFrame,lastOnlySrc,imageRefs,refs,refmods,framePeople,musicSource,historyDraft,historySelectionContext,historySourceSelected};
 };
 const drafts=useRef(new Map<string,ReturnType<typeof captureDraft>>());
 const pipelineModels=useRef(new Map<string,string>());
 const draftKey=(p:Pipeline,m:LocalModel)=>JSON.stringify([scope,p,identity(m)]);
 const outgoingDraft=useRef<{scope:string;pipeline:Pipeline;model:LocalModel;capture:typeof captureDraft}|null>(null);
 const activeScope=useRef(scope),explicitScopeRestore=useRef(false);
 const rememberDraft=()=>{const outgoing=outgoingDraft.current;if(outgoing){drafts.current.set(JSON.stringify([outgoing.scope,outgoing.pipeline,identity(outgoing.model)]),outgoing.capture());pipelineModels.current.set(JSON.stringify([outgoing.scope,outgoing.pipeline]),identity(outgoing.model));}};
 useLayoutEffect(()=>{
  if(activeScope.current!==scope){
   rememberDraft();draftEpoch.current++;outgoingDraft.current=null;activeScope.current=scope;
   if(!explicitScopeRestore.current){
    const previous=pipelineModels.current.get(JSON.stringify([scope,pipeline]));
    setModelIdentity(previous??modelIdentity);
    if(previous){
     setCheckpoint((JSON.parse(previous) as string[])[2]);
     const saved=drafts.current.get(JSON.stringify([scope,pipeline,previous]));
     if(saved){applyDraft(saved);if(saved.historyDraft)setHistorySelectionContext('');} // Returning never rebinds explicit history sources.
    }
   }
   explicitScopeRestore.current=false;return;
  }
  explicitScopeRestore.current=false;
  if(model)outgoingDraft.current={scope,pipeline,model,capture:captureDraft};
  else if(outgoingDraft.current?.scope===scope)outgoingDraft.current.capture=captureDraft;
 });
 const startingSettings=(m:LocalModel)=>{setResolution('model');const d=familyOf(m)?.defaults;if(d){setWidth(d.width);setHeight(d.height);setSteps(d.steps);setCfg(d.cfg);setSampler(d.sampler);setScheduler(d.scheduler);}};
 const switchDraft=(p:Pipeline,m:LocalModel,defaults=false,keepSource=false,keepPrompts=false)=>{
  if(p===pipeline&&model&&identity(m)===identity(model)){setCheckpoint(m.name);setModelIdentity(identity(m));if(!checkpointPref&&defaults)startingSettings(m);return;}
  rememberDraft();draftEpoch.current++;setPipeline(p);setCheckpoint(m.name);setModelIdentity(identity(m));
  const saved=drafts.current.get(draftKey(p,m));
  // Only the checkpoint changed: its remembered settings return, the prompts on screen stay.
  if(saved)applyDraft(keepPrompts?{...saved,settings:{...saved.settings,'studio.run.positive':positive,'studio.run.negative':negative}}:saved,keepSource);else if(defaults)startingSettings(m);
 };
 const applyDraft=(saved:ReturnType<typeof captureDraft>,keepSource=false)=>{
  if(!applyStudioSettings(saved.settings))return;
  if(!keepSource){sourceEpoch.current++;setMedia(saved.media);setMediaSize(saved.mediaSize);setLayers(saved.layers.map(copyImageLayer));setEditorSnapshot(saved.editor);setMasked(saved.masked);}
  setLastFrame(saved.lastFrame);setLastOnlySrc(saved.lastOnlySrc);setImageRefs(saved.imageRefs);setRefs(saved.refs);setRefmods(saved.refmods);setFramePeople(saved.framePeople);setMusicSource(saved.musicSource);
  setHistoryDraft(saved.historyDraft);setHistorySelectionContext(saved.historySelectionContext);setHistorySourceSelected(saved.historySourceSelected);
  setBrowsed(null);setSelectedJob('');
 };
 const pickModel=(m:LocalModel)=>switchDraft(pipeline,m,true,false,true);
 const pickPipeline=(p:Pipeline,keepSource=false)=>{
  const previous=pipelineModels.current.get(JSON.stringify([scope,p]));
  const next=allModels.find(m=>identity(m)===previous&&!blocked(m,p))??(model&&!blocked(model,p)?model:allModels.find(m=>!blocked(m,p)));
  if(next)switchDraft(p,next,!model||identity(next)!==identity(model),keepSource);else setPipeline(p);
 };
 useEffect(()=>{
  const layout=()=>{dragCleanup.current();setSettingsWidth(defaultLayout.settings);setQueueWidth(defaultLayout.queue);setDockHeight(defaultLayout.dock);setSettingsHidden(false);setDockHidden(false);};
  const defaults=()=>{if(!window.confirm(t(project?'studio.quick.confirmProject':'studio.quick.confirm')))return;drafts.current.clear();pipelineModels.current.clear();draftEpoch.current++;resetStudioPreferences();setResolution('model');const first=allModels.find(m=>!blocked(m,'t2i'));if(first){setCheckpoint(first.name);setModelIdentity(identity(first));startingSettings(first);}};
  window.addEventListener('deas:reset-studio-layout',layout);window.addEventListener('deas:request-studio-defaults',defaults);
  return()=>{window.removeEventListener('deas:reset-studio-layout',layout);window.removeEventListener('deas:request-studio-defaults',defaults);};
 });
 // More is a controlled menu: a native <details> toggle ignores document clicks, so a second summary click must close it explicitly.
 const [moreOpen,setMoreOpen]=useState(false);const [moreResult,setMoreResult]=useState<ReactNode>(null);const moreRef=useRef<HTMLDetailsElement>(null);
 const selectMoreAction=(_selected:true,action:()=>void)=>{setMoreOpen(false);action();};
 useEffect(()=>{if(!moreOpen)return;const onDown=(e:Event)=>{if(!moreRef.current?.contains(e.target as Node))setMoreOpen(false);};document.addEventListener('mousedown',onDown);return()=>document.removeEventListener('mousedown',onDown);},[moreOpen]);
 // Explicit "use these settings": only values the metadata carries; a model/LoRA is picked only if this folder holds it (by full name, else by basename). Returns what was skipped.
 const base=(n:string)=>n.replace(/\\/g,'/').split('/').at(-1)!.toLowerCase();
 const resolveMetadataModel=(list:LocalModel[],name:string)=>{const exact=list.filter(m=>m.name===name);if(exact.length)return exact.length===1?exact[0]:undefined;const matches=list.filter(m=>base(m.name)===base(name)||!name.includes('.')&&shortName(m.name).toLowerCase()===base(name));return matches.length===1?matches[0]:undefined;};
 const stoppable=(j:JobStatus)=>stopAvailable&&(j.state==='queued'||j.state==='running');
 const reusable=(j:JobStatus)=>!!j.intent&&!isVideoEnhanceIntent(j.intent);
 // The clicked job's own immutable intent, through the same projection as Recent history.
 const reusePrompt=(j:JobStatus)=>{metadataRead.current++;if(j.intent&&(isImageIntent(j.intent)||isVideoIntent(j.intent))){setPositive(j.intent.positivePrompt);setNegative(j.intent.negativePrompt);}};
 const reuseJob=(j:JobStatus)=>{
  if(!j.intent||isVideoEnhanceIntent(j.intent))return false;
  return restoreHistory(historyRestore({id:j.id,clientId:null,promptId:j.promptId,origin:'studio',appVersion:j.appVersion??'',admittedAt:0,observedAt:0,revision:0,state:j.state,intent:j.intent,outputs:[],outputRoot:j.outputRoot??null,backendOrigin:j.baseUrl,error:j.error},{endpoint,inputRoot:roots.input,allow:choices,models:models??[]}));
 };
 const applySettings=(s:GenerationSettings):string[]=>{
  rememberDraft();
  const skipped=(s.skipped??[]).map(item=>item==='video.intent'?t('studio.run.metadata.videoPartial'):item);
  const restoredModel=s.checkpoint?resolveMetadataModel(allModels,s.checkpoint):undefined;
  const restoredFamily=familyOf(restoredModel);
  if(s.checkpoint){
   draftEpoch.current++;setCheckpoint(restoredModel?.name??s.checkpoint);setModelIdentity(restoredModel?identity(restoredModel):'unresolved:'+s.checkpoint);
   if(!restoredModel)skipped.push(`${t('studio.run.checkpoint')}: ${s.checkpoint}`);
   else {
    if(restoredFamily?.id!==family?.id){
     setTextEncoder('');setTextEncoder2('');setVae('');setVideo(defaultVideo);setOptions({...structuredClone(defaultOptions),outputNaming:optionsPref.outputNaming});
     setPost(false);setPostTiled(false);setUpscale(1);setUpscaleModel('');setTve(defaultTve);setH3Latent({...h3Latent,enabled:false});setLtxLatent({...ltxLatent,enabled:false});
     setImageRefs([]);setRefs([]);setRefmods([]);setFramePeople({});setLastFrame(null);setLoras([]);setWildcardEnabled(false);
    }
    if(!restoredFamily?.pipelines.includes(pipeline)&&restoredFamily)setPipeline(restoredFamily.video?'t2v':'t2i');
    else if(!restoredFamily&&pipeline!=='t2i'&&pipeline!=='i2i')setPipeline('t2i');
   }
  }
  setHistoryDraft(null);
  if(s.wildcard&&isWildcardRules(s.wildcard)){setWildcardEnabled(true);setWildcard(structuredClone(s.wildcard));}
  if(s.sigmas)setOptions(v=>({...v,sigmas:s.sigmas!.join(', ')}));else if(s.scheduler&&(!choices?.schedulers||choices.schedulers.includes(s.scheduler)))setOptions(v=>({...v,sigmas:''}));
  if(s.media==='video'){
   if(!isVideo)setPipeline('t2v');
   if(s.frames!==undefined&&s.frameRate!==undefined&&s.frameRate>0)setVideo(v=>({...v,seconds:s.frames!/s.frameRate!,fps:s.frameRate!}));
  }
  if(s.positive!==undefined)setPositive(s.positive);if(s.negative!==undefined)setNegative(s.negative);
  if(s.seed!==undefined&&isSeed(s.seed))fixedSeed(s.seed);if(s.steps!==undefined)setSteps(s.steps);if(s.cfg!==undefined)setCfg(s.cfg);
  // Backend lists are checked when already read; otherwise native preparation rejects an unknown name later.
  for(const [key,value,list,set] of [['sampler',s.sampler,choices?.samplers,setSampler],['scheduler',s.scheduler,choices?.schedulers,setScheduler]] as const){
   if(!value)continue;if(list&&!list.includes(value))skipped.push(`${t(key==='sampler'?'studio.recipe.sampler':'studio.recipe.scheduler')}: ${value}`);else set(value);
  }
  if(s.width&&s.height){setWidth(s.width);setHeight(s.height);setResolution('fixed');}
  if(s.loras){const targetFamily=restoredFamily??family;const candidates=(models??[]).filter(m=>m.category==='loras'&&(!targetFamily||!m.family||m.family===(targetFamily.loraFamily??targetFamily.id)));setLoras(s.loras.flatMap(l=>{const m=resolveMetadataModel(candidates,l.name);if(!m)skipped.push(`LoRA: ${l.name}`);return m?[{name:m.name,strength:l.strength}]:[];}));}
  return skipped;
 };
 // Ignore reads overtaken by a newer action, selection, authoring edit or unmount.
 const metadataRead=useRef(0),metadataContext=useRef<()=>string>(()=> ''),lastMetadataContext=useRef('');
 metadataContext.current=()=>JSON.stringify([scope,captureStudioSettings(),browsed?.src,media?.src,selectedJob,sourceEpoch.current,draftEpoch.current,layers,masked,editorRef.current?.contentRevision()]);
 useEffect(()=>{const context=metadataContext.current();if(lastMetadataContext.current!==context){lastMetadataContext.current=context;metadataRead.current++;}});
 const applyFileSettings=async(item:MediaItem,mode:'all'|'prompt',preview=false)=>{
  setMoreResult(null);
  if(!native||!item.path)return;
  const revision=++metadataRead.current;
  const matches=jobs.filter(j=>reusable(j)&&j.outputs.some(o=>{const path=outputItem(j,o).path;return path&&normalizedMediaPath(path)===normalizedMediaPath(item.path!);}));
  const job=matches.length===1?matches[0]:undefined;
  if(job){if(mode==='prompt')reusePrompt(job);else if(!reuseJob(job))return;if(preview)setBrowsed(item);return;}
  const context=metadataContext.current();
  const current=()=>live.current&&revision===metadataRead.current&&context===metadataContext.current();
  let entries;try{entries=await readImageMetadata(item.path);}catch{if(current()){if(preview)setBrowsed(item);setMoreResult(t('studio.run.metadata.failed'));}return;}
  if(!current())return;
  if(preview)setBrowsed(item);
  const settings=generationSettings(entries);
  if(!settings||mode==='prompt'&&settings.positive===undefined&&settings.negative===undefined)return setMoreResult(t('studio.run.metadata.reuseEmpty'));
  if(mode==='prompt'){
   if(settings.positive!==undefined)setPositive(settings.positive);if(settings.negative!==undefined)setNegative(settings.negative);
   setMoreResult(t('studio.run.metadata.applied'));return;
  }
  const skipped=applySettings(settings);
  setMoreResult(<>{t('studio.run.metadata.partial')}{skipped.length>0&&<> {t('studio.run.metadata.skipped',{items:skipped.join(' · ')})}</>}</>);
 };
 const restoreHistory=(value:HistoryRestore)=>{
  if((masked||layers.length||editorRef.current?.imageEdited())&&!window.confirm(t('studio.run.metadata.replaceEditor')))return false;
  metadataRead.current++;rememberDraft();draftEpoch.current++;setEditorSnapshot(undefined);
  const intent=value.intent;
  const name=isVideoIntent(intent)?intent.model:intent.checkpoint,matching=allModels.filter(m=>m.name===name);
  setModelIdentity(matching.length===1?identity(matching[0]):'unresolved:'+name);
  // Use the same setters as ordinary authoring: an open project keeps its normal
  // autosave and unsaved-close protection. Native jobs and the queue are untouched.
  setHistoryDraft(value);setHistorySelectionContext(historyInputContext);setHistorySourceSelected(false);
  sourceEpoch.current++;setMedia(null);setMediaSize(null);setLayers([]);setMasked(false);setMusicSource(null);
  setLastFrame(null);setLastOnlySrc(null);setBrowsed(null);setSelectedJob('');setRefs([]);setRefmods([]);setFramePeople({});setImageRefs([]);
  if(isMusicIntent(intent)){
   // Music restores only music state; image/video controls keep their current values.
   setMusicStyle(intent.style);setMusicLyrics(intent.lyrics);setMusicPlanning(intent.planning);setMusicGeneration({...intent.generation});
   if(intent.mode==='t2m'){setMusicDuration({...musicDuration,t2m:intent.maxDuration});if(intent.abc)setMusicAbc({...intent.abc});}
   else {setMusicDuration({...musicDuration,cover:intent.maxDuration});setMusicEncoder(intent.audioEncoder);}
   fixedSeed(intent.seed);setCheckpoint(intent.checkpoint);setSteps(intent.steps);setCfg(intent.cfg);setSampler(intent.sampler);setScheduler(intent.scheduler);setOptions(value.options);
   setPipeline(value.pipeline);return true;
  }
  setPositive(intent.positivePrompt);setNegative(intent.negativePrompt);
  fixedSeed(intent.seed);setCheckpoint(isVideoIntent(intent)?intent.model:intent.checkpoint);
  setSteps(intent.steps);setCfg(intent.cfg);setSampler(intent.sampler);setScheduler(intent.scheduler);
  setWidth(intent.width);setHeight(intent.height);setResolution('fixed');setAspect('auto');
  setLoras((intent.loras??[]).map(l=>({...l})));setOptions(value.options);
  setPipeline(value.pipeline);setComponentOverrides({textEncoders:true,vaes:true,audioVaes:true,textProjections:true});
  setImageRefs(value.references.map(ref=>({item:{name:ref.name,media:'image',src:''},strength:ref.strength})));
  setPost(false);setPostTiled(false);setUpscale(1);setUpscaleModel('');setInputScaling('off');setEdit('resample');
  if(isVideoIntent(intent)){
   setTve(fromVideoPost(intent.videoPost));
   if(intent.family==='minimax-h3')setH3Latent(intent.latentUpscale?{...defaultLatent,...intent.latentUpscale,enabled:true}:{...h3Latent,enabled:false});
   if(intent.family==='ltx-2')setLtxLatent(intent.latentUpscale?{...defaultLatent,...intent.latentUpscale,enabled:true}:{...ltxLatent,enabled:false});
  }
  setWildcardEnabled(false);setRefVision('');setVideo(defaultVideo);setH3Scaling('Auto');setSourceStrength(1);setSourceAudio(false);
  if(isVideoIntent(intent)){
   setTextEncoder(intent.textEncoder);setTextEncoder2('');setVae(intent.vae);
   const shared={...defaultVideo,seconds:intent.frames/intent.frameRate,fps:intent.frameRate};
   if(intent.family==='ltx-2'){setVideo({...shared,passes:intent.ltx.passes,upscaler:intent.ltx.spatialUpscaler??'',audioVae:intent.ltx.audioVae,textProjection:intent.ltx.textProjection});setSourceStrength(intent.ltx.sourceStrength??1);}
   else if(intent.family==='wan')setVideo({...shared,shiftHigh:intent.wan.shiftHigh,shiftLow:intent.wan.shiftLow,refinerStep:intent.wan.refinerStep});
   else {setVideo({...shared,audioVae:intent.h3.audioVae,shiftVideo:intent.h3.shiftVideo,shiftAudio:intent.h3.shiftAudio});setH3Scaling(intent.h3.inputScaling??'Fill and crop');}
  }else{
   setTextEncoder(intent.diffusion?.textEncoder??'');setTextEncoder2(intent.diffusion?.textEncoder2??'');setVae(intent.diffusion?.vae??'');
   setDenoise(intent.denoise??0.6);setInpaintDenoise(intent.denoise??0.8);setEdit(intent.inputMask?'inpaint':'resample');setMaskBlur(intent.maskBlur??0);setInpaintOnlyMasked(false);
   setRefVision(intent.imageReference?.visionModel??'');
   if(intent.wildcard){setWildcard(structuredClone(intent.wildcard));setWildcardEnabled(true);}
   if(intent.h3Edit)setVideo({...defaultVideo,shiftVideo:intent.h3Edit.shiftVideo,shiftAudio:intent.h3Edit.shiftAudio});
   if(intent.refine){setPost(true);setPostTiled(!!intent.refine.tiled);setUpscale(intent.refine.width/intent.width);setRefineDenoise(intent.refine.denoise);setUpscaleModel(intent.refine.upscaleModel??'');}
  }
  return true;
 };
 const tile=(m:LocalModel,pressed:boolean,onClick:()=>void,extra?:React.ReactNode,reason='')=><li key={identity(m)}><ModelMetadataTarget model={m}><button type="button" aria-label={m.name} aria-haspopup="menu" aria-keyshortcuts="Shift+F10" aria-pressed={pressed} aria-disabled={!!reason} title={reason?t(reason):m.name} onClick={()=>{if(!reason)onClick();}}>{m.info?.civitai?.preview?<img src={m.info.civitai.preview} alt="" loading="lazy"/>:<span className="wb-thumb" aria-hidden="true">◆</span>}<span className="wb-tile-name">{shortName(m.name)}</span>{familyOf(m)&&<small className="wb-family">{familyOf(m)!.name}</small>}<ModelDeclaredBadge model={m}/></button></ModelMetadataTarget>{extra}</li>;
 const pick=(id:string,value:string,list:readonly string[],set:(v:string)=>void,onFocus?:()=>void)=><label key={id} className="wb-field"><span>{t(id)}</span><select data-pick={id} value={value} onFocus={onFocus} onPointerDown={onFocus} onChange={e=>set(e.target.value)}>{(list.includes(value)?list:[value,...list]).map(v=><option key={v} value={v}>{v}</option>)}</select></label>;
 const componentPick=(id:string,slot:ComponentSlot,value:string,list:readonly string[],set:(value:string)=>void)=><ComponentPicker key={slot} label={t(id)} value={value} automatic={recommended?.[slot][0]??''} files={list} all={componentOverrides[slot]} onAll={all=>setComponentOverrides({...componentOverrides,[slot]:all})} onChange={set}/>;
 const naming=outputPrefix(outputNamingTemplate(optionsPref.outputNaming),{type:isMusic?'audio':isVideo?'video':'image',seed,model:checkpoint});
 // The plan shows the cover source by its dock name only; the upload handle exists after Run.
 const preview=planVisible&&naming!==null?(isMusic?buildMusicIntent(naming,seed,musicSource?.name??''):isVideo?(videoFamily&&model&&split?buildVideoIntent(videoFamily,naming,optionsPref.output??defaultVideoOutput,(item,force=false)=>(!force&&comfyName(item))||item.path||item.name):null):buildImageIntent(naming,optionsPref.output??defaultVideoOutput)):null;
 const previewDrop=useRef(async(_paths:string[])=>{});
 previewDrop.current=async(paths:string[])=>{
  if(scanning||working||autoPlan||resolving||planRun.current)return;
  const revision=++metadataRead.current,context=metadataContext.current();
  try{
   const entries=await describeMedia(paths);
   if(!live.current||revision!==metadataRead.current||context!==metadataContext.current())return;
   const item=entries.map(itemFromEntry).find(item=>item&&item.media!=='audio');
   if(item)void applyFileSettings(item,'all',true);
  }catch{if(live.current&&revision===metadataRead.current&&context===metadataContext.current())setMoreResult(t('studio.run.metadata.failed'));}
 };
 useEffect(()=>{
  if(!native)return;
  let alive=true,unlisten:(()=>void)|undefined;
  void (async()=>getCurrentWebview().onDragDropEvent(e=>{
   if(!alive||e.payload.type!=='drop'||document.querySelector('dialog[open], [aria-modal="true"]'))return;
   const stage=shell.current?.querySelector('.wb-stage');if(!stage||stage.closest('[hidden]'))return;
   const r=stage.getBoundingClientRect(),x=e.payload.position.x/window.devicePixelRatio,y=e.payload.position.y/window.devicePixelRatio;
   if(r.width>0&&r.height>0&&x>=r.left&&x<=r.right&&y>=r.top&&y<=r.bottom)void previewDrop.current(e.payload.paths);
  }))().then(stop=>{if(alive)unlisten=stop;else stop();}).catch(()=>{if(alive)setMoreResult(t('studio.run.metadata.dropFailed'));});
  return()=>{alive=false;unlisten?.();};
 },[native]);
 const planNotices=[t('studio.plan.validation'),...(historyProblems.length?[t('studio.history.blocked',{items:historyProblems.join(' · ')})]:[]),...(backendPending?[t('studio.run.backendPending')]:[]),...(unavailable.length?[t('studio.quick.unavailable',{names:unavailable.join(', ')})]:[]),...(musicNeedsStyle?[t('studio.music.needsStyle')]:[]),...(!checkpoint?[t('studio.run.noModels')]:[]),...(sizeError?[t(sizeError,{multiple:String(grid)})]:[]),...(inpaint&&!masked?[t('studio.run.edit.needsMask')]:[]),...(naming===null?[t('studio.run.opt.outputNaming.invalid')]:[]),...(!choices?[]:[]),...((isVideo&&(post||wildcardEnabled||optionsPref.sigmas?.trim()))||(h3Image&&post)?[t('studio.plan.imageOnly')]:[]),...(isMusic&&(post||wildcardEnabled||optionsPref.sigmas?.trim()||activeLoras.length)?[t('studio.plan.imageOnly')]:[])];
 const inputNotes=[...(inpaint?[t('studio.plan.mask',{blur:String(maskBlur),grow:String(maskGrow),crop:t(inpaintOnlyMasked?'studio.plan.yes':'studio.plan.no')})]:[]),...(pipeline==='i2i'&&layers.some(l=>l.visible)?[t('studio.plan.layersPending')]:[]),...(isVideo||imageRefs.length||pipeline==='cover'?[t('studio.plan.uploadPending')]:[])];
 const projectDraft:ImageDraft={positivePrompt:positive,negativePrompt:negative,checkpoint:project?checkpointPref:checkpoint,width:project&&!model?width:outputSize.width,height:project&&!model?height:outputSize.height,steps,cfg,sampler,scheduler,seed,denoise,pipeline:pipeline==='i2i'?'i2i':'t2i'};
 const restoreProject=(next:ProjectSnapshot|null)=>{
  rememberDraft();outgoingDraft.current=null;explicitScopeRestore.current=true;draftEpoch.current++;setModelIdentity('');setEditorSnapshot(undefined);
  setHistoryDraft(null);
  const d=next?.draft??{positivePrompt:readPref('studio.run.positive','',isString),negativePrompt:readPref('studio.run.negative','',isString),checkpoint:readPref('studio.run.checkpoint','',isString),width:readPref('studio.run.width',1024,isNumber),height:readPref('studio.run.height',1024,isNumber),steps:readPref('studio.run.steps',20,isNumber),cfg:readPref('studio.run.cfg',5,isNumber),sampler:readPref('studio.run.sampler','euler',isString),scheduler:readPref('studio.run.scheduler','normal',isString),seed:readPref('studio.run.seed',0,isSeed),denoise:readPref('studio.run.denoise',0.6,isNumber),pipeline:readPref('studio.run.pipeline','t2i',oneOf(...pipelines))};
  onProject(next);setPositive(d.positivePrompt);setNegative(d.negativePrompt);setCheckpoint(d.checkpoint);setWidth(d.width);setHeight(d.height);setSteps(d.steps);setCfg(d.cfg);setSampler(d.sampler);setScheduler(d.scheduler);setSeed(d.seed);setDenoise(d.denoise);setPipeline(d.pipeline);
  setSeedMode(next?'fixed':readPref('studio.run.seedMode','random',oneOf('random','fixed')));
  setResolution(next?'fixed':readPref('studio.run.resolution','fixed',oneOf('fixed','model','custom',...Object.keys(resolutions))));
  setAspect(next?'auto':readPref('studio.run.aspect','auto',oneOf(...aspects)));
  // This first project schema stores baseline image controls, not optional processing/editor state.
  setLoras(next?[]:readPref('studio.run.loras',[],isLoras));setPost(next?false:readPref('studio.run.post',false,isBoolean));setOptions(next?defaultOptions:readPref('studio.run.options',defaultOptions,isOptionsPref));
  setTextEncoder(next?'':readPref('studio.run.textEncoder','',isString));setVae(next?'':readPref('studio.run.vae','',isString));
  setComponentOverrides(next?defaultComponentOverrides:readPref('studio.run.componentOverrides',defaultComponentOverrides,isComponentOverrides));setVideo(next?defaultVideo:readPref('studio.run.video',defaultVideo,isVideoPref));
  setWildcardEnabled(next?false:readPref('studio.run.wildcardEnabled',false,isBoolean));setEdit(next?'resample':readPref('studio.run.edit','resample',oneOf(...editModes)));setInputScaling(next?'off':readPref('studio.run.inputScaling','off',oneOf(...scalingModes)));
  setRecentSeeds(readPref('studio.run.recentSeeds',[],isSeeds));
  sourceEpoch.current++;setLayers([]);setMasked(false);setBrowsed(null);setSelectedJob('');
  const input=next?.input;
  setMedia(input?{name:input.id+'.png',path:input.path,src:convertFileSrc(input.path),media:'image'}:null);
  setMediaSize(input?{src:convertFileSrc(input.path),width:input.width,height:input.height}:null);
 };
 const stageWarnings=[...new Set([
  ...(historyDraft?.source&&!media?[t('studio.history.missingInput',{name:historyDraft.source.name})]:[]),
  ...(historyProblems.length?[t('studio.history.blocked',{items:historyProblems.join(' · ')})]:[]),
  ...(checkpointPref&&models&&!model?[t('studio.quick.unavailable',{names:checkpointPref}),...allModels.filter(m=>m.name===checkpointPref).map(m=>blocked(m,pipeline)).filter(Boolean).map(reason=>t(reason))]:[]),
  ...(unavailable.length?[t('studio.quick.unavailable',{names:unavailable.join(', ')})]:[]),
  ...(error?[backendErrorText(error,t)]:[]),...(shown?.error?[backendErrorText(shown.error,t)]:[]),
 ])];
 const dockNavigation=<div className="wb-dock-tabs" role="tablist" aria-label={t('studio.run.dock')}>{dockTabs.map(id=><button key={id} role="tab" type="button" data-dock-tab={id} aria-selected={dockTab===id} onClick={()=>{focusDockTab.current=true;setDockTab(id);if(id==='options'||id==='post')loadChoices();}}>{t(id==='wildcards'?'wildcards.title':id==='presets'?'presets.title':`studio.run.dock.${id}`)}{id==='loras'&&activeLoras.length>0&&<small> {activeLoras.length}</small>}{id==='wildcards'&&wildcardEnabled&&<small aria-label={t('wildcards.active')}> ✓</small>}{id==='post'&&(isVideo?latentEnabled:post)&&<small aria-label={t('studio.run.post.enabled')}> ✓</small>}{id==='options'&&options&&<small> {Object.keys(options).length}</small>}</button>)}</div>;
 return <>
 <ProjectBar draft={projectDraft} busy={busy||!!current||autoPlan||resolving} onRestore={restoreProject}/>
 {planVisible&&<RunPlan intent={preview} pipeline={pipeline} notices={planNotices} outputRoot={outputDir} inputNotes={inputNotes}/>}
 <div className="workbench-host" hidden={planVisible}>
 <div className="workbench" data-workbench ref={shell}>
 {historyDraft&&<div className="wb-history-draft" role="status" data-history-draft>{t('studio.history.draft')}{(historyDraft.source||historyDraft.references.length>0||historyDraft.lastFrame)&&<p>{t('studio.history.inputReselect')}</p>}{historyProblems.length>0&&<><p>{t('studio.history.blocked',{items:historyProblems.join(' · ')})}</p><button type="button" className="wb-dock-action" data-history-new-draft onClick={()=>setHistoryDraft(null)}>{t('studio.history.newDraft')}</button></>}</div>}
  <div className="wb-body">
   <aside id="studio-settings" className="wb-side" hidden={settingsHidden} style={{flexBasis:sideSize}}>
    <section className="wb-settings" aria-label={t('studio.run.settings')}>
     <label className="wb-field wb-pipeline"><span>{t('studio.run.pipeline')}</span><select data-pipeline value={pipeline} onChange={e=>{pickPipeline(e.target.value as Pipeline);}}>{pipelines.map(id=>{const reason=why(id);return <option key={id} value={id} disabled={!!reason} title={reason?t(reason):undefined}>{t(`studio.run.pipe.${id}`)}</option>;})}</select></label>
     <div className="wb-checkpoint"><label className="wb-field"><span>{t('studio.run.checkpoint')}{family&&<> · {family.name}</>}</span><select data-model-picker aria-label={t('studio.run.checkpoint')} title={unavailableModelReason||checkpointPref||checkpoint} value={model?modelValue(model):checkpointPref||checkpoint} onChange={e=>{const next=allModels.find(m=>modelValue(m)===e.target.value&&!blocked(m,pipeline));if(next)pickModel(next);}}>
      {!checkpointPref&&!checkpoint&&<option value="" disabled>{t('studio.run.noModels')}</option>}
      {unavailableModelReason&&<option value={checkpointPref} disabled title={unavailableModelReason}>{checkpointPref} — {unavailableModelReason}</option>}
      {usable.map(m=><option key={identity(m)} value={modelValue(m)} title={m.name}>{m.name}</option>)}
     </select></label><small>{t('studio.run.loras')} {activeLoras.length}</small></div>
     {pipeline==='i2i'&&<p className="wb-active"><span>{t('studio.run.pipe.input')} · {t(`studio.run.edit.${edit}`)}</span><strong title={inputImage}>{fileName(inputImage)}</strong></p>}
    {pipeline==='i2v'&&!lastOnly&&<p className="wb-active"><span>{t('studio.run.pipe.input')}</span><strong title={inputImage}>{fileName(inputImage)}</strong></p>}
    {pipeline==='v2v'&&<p className="wb-active"><span>{t('studio.run.video.source')}</span><strong title={media?.path}>{media?.media==='video'?media.name:''}</strong></p>}
    {pipeline==='cover'&&<p className="wb-active" data-music-source><span>{t('studio.music.source')}</span><strong title={musicSource?.path}>{musicSource?.name??''}</strong>{musicSource&&<button type="button" aria-label={t('studio.music.removeSource')} title={t('studio.music.removeSource')} onClick={()=>setMusicSource(null)}>✕</button>}</p>}
     <div className="wb-controls">
     {h3Image&&<p className="wb-resolution-hint wb-wide">{t('studio.run.h3EditHint')}</p>}
     {isMusic?<MusicSettings pipeline={pipeline as 't2m'|'cover'} planning={musicPlanning} onPlanning={p=>{setMusicPlanning(p);if(p==='off'&&musicGeneration.cfgScale===1)setMusicGeneration({...musicGeneration,cfgScale:1.01});}} maxDuration={pipeline==='cover'?musicDuration.cover:musicDuration.t2m} onMaxDuration={n=>setMusicDuration({...musicDuration,[pipeline==='cover'?'cover':'t2m']:n})} encoder={musicEncoder} encoders={musicEncoders} onEncoder={setMusicEncoder} onReadChoices={loadChoices} abc={musicAbc} onAbc={setMusicAbc} generation={musicGeneration} onGeneration={setMusicGeneration}/>
     :inpaint?<>{!h3Image&&num('denoise',strength,setStrength,0,1,0.05,undefined,true)}<p className="wb-resolution-hint">{t('studio.run.edit.size',{width:String(outputSize.width),height:String(outputSize.height)})}</p></>:<>
      <label className="wb-field"><span>{t('studio.run.resolution.aspect')}</span><span className="wb-aspect-picker"><span className="wb-aspect-swatch" aria-hidden="true"><span style={{width:18*Math.min(1,swatchRatio),height:18*Math.min(1,1/swatchRatio)}}/></span><select data-aspect value={aspect} onChange={e=>{setAspect(e.target.value as typeof aspect);if(resolution==='fixed')setResolution('model');}}>{aspects.filter(a=>a!=='auto'||mediaPipeline).map(a=><option key={a} value={a}>{a==='auto'||a==='custom'?t(`studio.run.resolution.${a}`):a}</option>)}</select></span></label>
      <label className="wb-field"><span>{t('studio.run.resolution.title')}</span><select data-resolution value={resolution} onChange={e=>setResolution(e.target.value)}>{['fixed','model',...Object.keys(resolutions),'custom'].map(r=><option key={r} value={r} disabled={r==='model'&&!family?.defaults}>{r in resolutions?r:t(`studio.run.resolution.${r}`)}</option>)}</select></label>
      {aspect==='custom'&&<>{num('resolution.aspectWidth',aspectWidth,setAspectWidth,1,100,0.1)}{num('resolution.aspectHeight',aspectHeight,setAspectHeight,1,100,0.1)}</>}
      {resolution==='custom'&&num('resolution.megapixels',megapixels,setMegapixels,0.01,16,0.05)}
      {num('width',canvas.width,n=>{setWidth(n);setHeight(canvas.height);setResolution('fixed');},64,8192,8)}{num('height',canvas.height,n=>{setHeight(n);setWidth(canvas.width);setResolution('fixed');},64,8192,8)}</>}
      {isVideo&&family?.video&&split&&<>
       {num('video.seconds',video.seconds,n=>setVideo({...video,seconds:clamp(n,0.5,15)}),0.5,15,0.5)}{num('video.fps',fps,n=>setVideo({...video,fps:clamp(n,1,120)}),1,120,1,family.video.fixedFps?t('studio.run.video.fixedFps',{fps:String(family.video.fps)}):undefined)}
       <p className="wb-resolution-hint wb-wide">{t('studio.run.video.length',{frames:String(frames),seconds:((frames-1)/fps).toFixed(2)})}</p>
       {videoFamily==='ltx-2'&&<>
        <label className="wb-field"><span>{t('studio.run.video.passes')}</span><select value={video.passes} onChange={e=>setVideo({...video,passes:Number(e.target.value) as VideoPref['passes']})}>{[1,2,3].map(n=><option key={n} value={n} disabled={n>1&&!(choices?.latentUpscaleModels?.length||!choices)}>{n}</option>)}</select></label>
        {video.passes>1&&pick('studio.run.video.upscaler',video.upscaler||choices?.latentUpscaleModels?.[0]||'',choices?.latentUpscaleModels??[],v=>setVideo({...video,upscaler:v}),loadChoices)}</>}
       {videoFamily==='wan'&&<>{num('video.shiftHigh',video.shiftHigh,n=>setVideo({...video,shiftHigh:clamp(n,0.5,20)}),0.5,20,0.5)}{num('video.shiftLow',video.shiftLow,n=>setVideo({...video,shiftLow:clamp(n,0.5,20)}),0.5,20,0.5)}{num('video.refinerStep',Math.min(video.refinerStep,steps-1),n=>setVideo({...video,refinerStep:clamp(Math.round(n),1,Math.max(1,steps-1))}),1,Math.max(1,steps-1),1)}</>}
       {videoFamily==='minimax-h3'&&<>{num('video.shiftVideo',video.shiftVideo,n=>setVideo({...video,shiftVideo:clamp(n,0.01,100)}),0.01,100,0.5)}{num('video.shiftAudio',video.shiftAudio,n=>setVideo({...video,shiftAudio:clamp(n,0.01,100)}),0.01,100,0.5)}
        {/* The Director scales every reference picture/video (incl. first/last frame) to the canvas with this mode. */}
        {(pipeline!=='t2v'||!!lastFrame)&&<><label className="wb-field wb-wide"><span>{t('studio.run.resolution.scaling')}</span><select data-h3-scaling value={h3Scaling} title={t(`studio.run.resolution.hint.${h3ScalingKey[h3Scaling]}`)} onChange={e=>setH3Scaling(e.target.value as H3Scaling)}>{h3Scalings.map(s=><option key={s} value={s}>{t(`studio.run.resolution.scale.${h3ScalingKey[s]}`)}</option>)}</select></label><p className="wb-resolution-hint wb-wide">{t(`studio.run.resolution.hint.${h3ScalingKey[h3Scaling]}`)}</p></>}</>}
       {pipeline==='v2v'&&(videoFamily==='ltx-2'?num('video.sourceStrength',sourceStrength,n=>setSourceStrength(clamp(n,0,1)),0,1,0.05,undefined,true)
        :<label className="wb-field wb-wide"><span><input type="checkbox" style={{width:'auto'}} checked={sourceAudio} onChange={e=>setSourceAudio(e.target.checked)}/>{t('studio.run.video.sourceAudio')}</span></label>)}
       {refsTab&&<button ref={referencesButton} type="button" className="wb-dock-action wb-wide" data-open-references aria-haspopup="dialog" aria-expanded={referencesOpen} onClick={()=>setReferencesOpen(true)} onDragOver={e=>{if(e.dataTransfer.types.includes(MEDIA_DRAG))e.preventDefault();}} onDrop={e=>{const item=draggedItem(e.dataTransfer);if(item){e.preventDefault();addRefs([{item}]);setReferencesOpen(true);}}}>{t('studio.run.dock.references')} · {refs.length}</button>}
       {videoFamily==='minimax-h3'&&(pipeline==='v2v'||pipeline==='ref2v')&&!/ref2va/i.test(checkpoint)&&<p className="wb-resolution-hint wb-wide">{t('studio.run.video.ref2vaHint')}</p>}
       {forgeContext&&onOpenPromptForge&&<button type="button" className="wb-dock-action wb-wide" data-open-prompt-forge onClick={onOpenPromptForge}>{t('studio.tab.forge')}</button>}
       {continuityAvailable&&<button ref={continuityButton} type="button" className="wb-dock-action wb-wide" data-continuity-open aria-haspopup="dialog" aria-expanded={continuityOpen} data-continuity-enabled={continuityOn} onClick={()=>setContinuityOpen(true)}>{t(continuityOn?'studio.run.dock.continuity':'studio.run.dock.continuityOff')}</button>}
       {frameVideo&&<p className="wb-resolution-hint wb-wide">{lastFrame?<>{t('studio.run.video.lastFrame')}: <strong>{lastFrame.name}</strong> <button type="button" aria-label={t('studio.run.cont.remove')} title={t('studio.run.cont.remove')} onClick={()=>setLastFrame(null)}>✕</button></>:t('studio.run.video.lastFrameHint')}</p>}
      </>}
      {pipeline==='i2i'&&!inpaint&&<><label className="wb-field wb-wide"><span>{t('studio.run.resolution.scaling')}</span><select value={inputScaling} title={t(`studio.run.resolution.hint.${inputScaling}`)} onChange={e=>setInputScaling(e.target.value as ScalingMode)}>{scalingModes.map(s=><option key={s} value={s}>{t(`studio.run.resolution.scale.${s}`)}</option>)}</select></label><p className="wb-resolution-hint">{t(`studio.run.resolution.hint.${inputScaling}`)}</p>{!h3Image&&num('denoise',denoise,setDenoise,0,1,0.05,undefined,true)}{h3Image&&<>{num('video.shiftVideo',video.shiftVideo,n=>setVideo({...video,shiftVideo:clamp(n,0.01,100)}),0.01,100,0.5)}{num('video.shiftAudio',video.shiftAudio,n=>setVideo({...video,shiftAudio:clamp(n,0.01,100)}),0.01,100,0.5)}</>}</>}
      {inpaint&&<>
       <label className="wb-field wb-wide" title={t('studio.run.maskGrowHelp')}><span><input type="checkbox" style={{width:'auto'}} checked={inpaintOnlyMasked} onChange={e=>setInpaintOnlyMasked(e.target.checked)}/>{t('studio.run.inpaintOnlyMasked')}</span></label>
       {inpaintOnlyMasked&&num('maskGrow',maskGrow,n=>{if(isMaskGrow(n))setMaskGrow(n);},0,512,1,undefined,true)}
       {num('maskBlur',maskBlur,n=>{if(isMaskBlur(n))setMaskBlur(n);},0,31,1,undefined,true)}
       {h3Image&&<>{num('video.shiftVideo',video.shiftVideo,n=>setVideo({...video,shiftVideo:clamp(n,0.01,100)}),0.01,100,0.5)}{num('video.shiftAudio',video.shiftAudio,n=>setVideo({...video,shiftAudio:clamp(n,0.01,100)}),0.01,100,0.5)}</>}
      </>}
      {num('steps',sigmas?sigmas.length-1:steps,setSteps,1,150,1,sigmas&&t('studio.run.customSchedule.hint'),true)}{num('cfg',cfg,setCfg,0,30,0.5,undefined,true)}
      {pick('studio.recipe.sampler',sampler,choices?.samplers??[],setSampler,loadChoices)}
      {sigmas?<label className="wb-field"><span>{t('studio.recipe.scheduler')}</span><select value="custom" disabled title={t('studio.run.customSchedule.hint')}><option value="custom">{t('studio.run.customSchedule')}</option></select></label>
       :pick('studio.recipe.scheduler',scheduler,choices?.schedulers??[],setScheduler,loadChoices)}
      <div className="wb-field wb-seed">
       <label><span>{t('studio.run.seed')}</span><input data-seed aria-label={t('studio.run.seed')} type="number" inputMode="numeric" value={seed} min={0} max={Number.MAX_SAFE_INTEGER} step={1} onChange={e=>{const n=Number(e.target.value);if(isSeed(n))fixedSeed(n);}}/></label>
       <div className="wb-seed-row"><button type="button" aria-label={t('studio.run.seedDecrease')} title={t('studio.run.seedDecrease')} onClick={()=>fixedSeed(seed===0?Number.MAX_SAFE_INTEGER:seed-1)}>−</button><button type="button" aria-label={t('studio.run.seedIncrease')} title={t('studio.run.seedIncrease')} onClick={()=>fixedSeed(seed===Number.MAX_SAFE_INTEGER?0:seed+1)}>+</button>{(['random','fixed'] as const).map(mode=><button key={mode} type="button" aria-pressed={seedMode===mode} onClick={()=>setSeedMode(mode)}>{t(`studio.run.seedMode.${mode}`)}</button>)}<button type="button" aria-label={t('studio.run.newSeed')} title={t('studio.run.newSeed')} onClick={()=>setSeed(randomSeed())}>⟳</button></div>
       <button type="button" disabled={!recentSeeds.length} onClick={()=>fixedSeed(recentSeeds[0])}>{t('studio.run.seedLast')}</button>
       <details className="wb-seed-history"><summary>{t('studio.run.seedHistory')}</summary>{recentSeeds.map(value=><div className="wb-seed-row" key={value}><button type="button" aria-label={t('studio.run.seedUse',{seed:String(value)})} title={t('studio.run.seedUse',{seed:String(value)})} onClick={()=>fixedSeed(value)}>{value}</button><button type="button" aria-label={t('studio.run.seedCopy',{seed:String(value)})} title={t('studio.run.seedCopy',{seed:String(value)})} onClick={()=>void copySeed(value)}>⧉</button></div>)}</details>
       {copyStatus&&<span role="status">{copyStatus}</span>}
      </div>
     </div>
     {split&&<details className="wb-model-files"><summary>{t('studio.components.title')}</summary><div className="wb-component-menu">
      {componentPick('studio.run.textEncoder','textEncoders',textEncoderPref,split.encoders,setTextEncoder)}
      {dualClip&&<ComponentPicker key="textEncoders2" label={t('studio.run.textEncoder2')} value={textEncoder2Pref} automatic={recommendedSecond[0]??''} files={componentFilesNamed(models??[],family?.clip?.encoders?.[1]?[family.clip.encoders[1]]:[])} all={componentOverrides.textEncoders} onAll={all=>setComponentOverrides({...componentOverrides,textEncoders:all})} onChange={setTextEncoder2}/>}
      {componentPick('studio.run.vae','vaes',vaePref,split.vaes,setVae)}
      {isVideo&&family?.audioVaes&&componentPick('studio.run.video.audioVae','audioVaes',video.audioVae,split.audioVaes,v=>setVideo({...video,audioVae:v}))}
      {isVideo&&family?.textProjections&&componentPick('studio.run.video.textProjection','textProjections',video.textProjection,split.projections,v=>setVideo({...video,textProjection:v}))}
     </div></details>}
     {!native&&<p className="field-help">{t('studio.jobs.browser')}</p>}
    </section>
   </aside>
   {columnHandle('settings')}{paneToggle(settingsHidden,setSettingsHidden,'settings','studio-settings',settingsHidden?9:sideSize+5)}
   <div className="wb-center">
    <section className={`wb-stage${!browsed&&sideBySide?' wb-split':''}`} aria-label={t('studio.run.preview')} onDragOver={e=>{if(e.dataTransfer.types.includes(MEDIA_DRAG))e.preventDefault();}} onDrop={e=>{if(e.defaultPrevented)return;const item=draggedItem(e.dataTransfer);if(item&&item.media!=='audio'){e.preventDefault();void applyFileSettings(item,'all',true);}}}>
     <ThemeScene/><ThemeProgress active={!!current||working||resolving||preparing>0||frontendAdmitting} progress={current?.progress} label={t('studio.run.working')}/>
     {stageWarnings.length>0&&<div className="wb-stage-warning" role="alert">{stageWarnings.map(message=><p key={message}>{message}</p>)}{mossIssues.map(issue=><button type="button" key={issue.name} onClick={issue.reset}>{t('studio.run.opt.reset',{name:issue.name})}</button>)}</div>}
     {(browsed||media||shown)&&<button type="button" className="wb-stage-close" data-preview-close aria-label={t('studio.run.close')} title={t('studio.run.close')} onClick={closePreview}>✕</button>}
     {(browsed||media||shown)&&<button type="button" className="wb-stage-meta" data-metadata-toggle aria-pressed={metaOpen} onClick={()=>setMetaOpen(!metaOpen)}>{t('studio.run.metadata.button')}</button>}
     {stageTools()}
     {moreResult&&<p className="wb-more-feedback" role="status">{moreResult}</p>}
     {!browsed&&(media||shown)&&(hint||(!shown&&stageMark))&&<div className="wb-stage-notes">{!shown&&stageMark&&<span className="media-mark wb-stage-mark">{stageMark}</span>}{hint&&<p className="wb-stage-hint" role="status">{t(hint,{multiple:String(grid)})}</p>}</div>}
     {(browsed||media||shown)&&metaOpen&&<GenerationMetadata job={browsed?null:shown} media={browsed??(shown?null:media)} onRestore={reuseJob}/>}
     {browsed&&<figure className="media-preview" data-browsed-preview>{browsed.media==='image'?<img className="wb-zoomable" src={browsed.src} alt={browsed.name} role="button" tabIndex={0} aria-label={t('studio.run.viewer.open')} title={t('studio.run.viewer.open')} onClick={e=>openImage(e,browsed)} onKeyDown={e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();setViewer({src:browsed.src,alt:browsed.name,item:browsed});}}}/>:browsed.media==='video'?linuxVideo?<NativeVideo source={{path:browsed.path??''}} label={browsed.name}/>:<video src={native?browsedBlob.url:browsed.src} controls/>:audioPlayer(browsed,native?browsedBlob.url:browsed.src)}<figcaption>{browsed.name}{browsedBlob.error&&<span className="error-message"> · {browsedBlob.error}</span>}</figcaption></figure>}
     <div className="wb-current-preview" hidden={!!browsed}>
     {editableSource&&media&&<div className="wb-editor-host" hidden={!!shown&&!editorLeft}>
      <StudioImageEditor key={sourceEpoch.current} draftGeneration={scope+draftEpoch.current} snapshot={editorSnapshot} ref={editorRef} source={{src:media.src,name:media.name,width:editableSource.width,height:editableSource.height}} layers={layers} onLayers={setLayers} maskMode={inpaint} onMask={setMasked} onInsert={insertLayer} layout={sideBySide?'split':'full'}/>
      {sideBySide&&inputImage&&tools(media)}<button type="button" className="wb-editor-caption" data-editor-caption onClick={()=>setViewer({src:media.src,alt:media.name,item:media})}>{media.name} · {t('studio.run.edit.insertHint')}</button>
     </div>}
     {sideBySide&&!editorLeft&&originalItem&&<figure className="media-preview"><img className="wb-zoomable" data-original-preview src={original} alt={t('studio.run.edit.original')} role="button" tabIndex={0} aria-label={t('studio.run.viewer.open')} title={t('studio.run.viewer.open')} onClick={e=>openImage(e,originalItem)} onKeyDown={e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();setViewer({src:originalItem.src,alt:originalItem.name,item:originalItem});}}}/></figure>}
     {shown&&media&&!sideBySide&&<button type="button" className="wb-return-draft" data-editor-return onClick={()=>setSelectedJob('')}>{t('studio.editor.returnDraft')}</button>}
     {shown?<div className="wb-shown"><p className="wb-job-line"><span data-state={shown.state}>{t(`studio.jobs.${shown.state}`)}</span> · <code>{shown.id.slice(0,8)}</code>{duration(shown)}</p>
      {paired&&terminalOutput(shown)&&<div className="wb-output-choices" role="group" aria-label={t('studio.jobs.outputs')}>{shown.outputs.map((o,index)=><button type="button" key={index} data-output-role={o.role} aria-pressed={index===outputIndex} onClick={()=>{setSelectedOutput(index);setSlider(false);}}>{o.role?t(`studio.jobs.role.${o.role}`):o.filename}</button>)}</div>}
      {terminalOutput(shown)&&(shown.outputRoot?<figure className="media-preview"><div className="wb-media-content" data-media-content>{original&&slider?<CompareView key={`${original}|${shown.id}`} original={original} originalAlt={t(paired?'studio.jobs.role.original':'studio.run.edit.original')} edited={fileSrc(shown,pairedProcessed??shown.outputs[0])!} editedAlt={paired?t('studio.jobs.role.processed'):shown.outputs[0].filename} dividerLabel={t('studio.run.edit.divider')}/>
        :visibleOutputs.map(o=>outputItem(shown,o).media==='audio'?audioPlayer(outputItem(shown,o),fileSrc(shown,o)!,!browsed,true):isVideoOut(o)?linuxVideo?<NativeVideo key={o.filename} source={{path:outputItem(shown,o).path!}} label={o.filename} active={!browsed} autoPlay loop/>:<video key={o.filename} src={videoSrc(shown,o)} controls autoPlay loop onError={e=>setMediaFault(mediaError(e))}/>:<img key={o.filename} className="wb-zoomable" src={fileSrc(shown,o)!} alt={o.filename} title={t('studio.run.viewer.open')} draggable={!!outputItem(shown,o).path} onDragStart={e=>{const it=outputItem(shown,o);if(it.path)dragItem(e.dataTransfer,it);}} onClick={e=>openImage(e,outputItem(shown,o))}/>)}</div><figcaption><span className="wb-output-stars">{shown.outputs.map(o=><StarMediaButton key={o.filename} item={outputItem(shown,o)}/>)}</span>{original&&slider&&<>{t(paired?'studio.jobs.role.original':'studio.run.edit.original')} | {paired&&t('studio.jobs.role.processed')} · </>}{visibleOutputs.map(o=>o.role?`${t(`studio.jobs.role.${o.role}`)} · ${o.filename}`:o.filename).join(' · ')}{mediaFault&&<> · <span className="error-message">{mediaFault}</span></>}</figcaption></figure>:isVideoOut(shown.outputs[outputIndex])?<figure className="media-preview"><div className="wb-media-content" data-media-content>{visibleOutputs.filter(isVideoOut).map(o=>linuxVideo?<NativeVideo key={o.filename} source={{baseUrl:shown.baseUrl,filename:o.filename,subfolder:o.subfolder,type:o.type}} label={o.filename} active={!browsed} autoPlay loop/>:<video key={o.filename} src={videoSrc(shown,o)} controls autoPlay loop onError={e=>setMediaFault(mediaError(e))}/>)}</div><figcaption><span className="wb-output-stars">{shown.outputs.map(o=><StarMediaButton key={o.filename} item={outputItem(shown,o)}/>)}</span>{shown.outputs.map(o=>o.filename).join(' · ')}{mediaFault&&<> · <span className="error-message">{mediaFault}</span></>}</figcaption></figure>:<JobOutputPreview job={shown} outputRoot={shown.outputRoot} outputIndex={outputIndex} auto/>)}
      {(shown.state==='queued'||shown.state==='running')&&(shown.preview?<figure className="media-preview" aria-busy="true"><img src={shown.preview} alt={t('studio.run.working')}/>{shown.progress&&<figcaption><progress value={shown.progress.value} max={shown.progress.max}/> {shown.progress.value}/{shown.progress.max}</figcaption>}</figure>
       :<p className="empty-note" aria-busy="true"><span className="aio-spinner" aria-hidden="true"/> {t('studio.run.working')}{shown.progress&&<> {shown.progress.value}/{shown.progress.max}</>}</p>)}</div>
     :media&&!editableSource?<figure className="media-preview">{media.media==='image'?<img className="wb-zoomable" src={media.src} alt={media.name} title={t('studio.run.viewer.open')} draggable={!!media.path} onDragStart={e=>{if(media.path)dragItem(e.dataTransfer,media);}} onClick={e=>openImage(e,media)} onLoad={e=>readMediaSize(e.currentTarget.naturalWidth,e.currentTarget.naturalHeight)}/>:media.media==='video'?linuxVideo?<NativeVideo source={{path:media.path??''}} label={media.name} active={!browsed} onSize={readMediaSize}/>:<video src={native?mediaBlob.url:media.src} controls onError={e=>setMediaFault(mediaError(e))} onLoadedMetadata={e=>readMediaSize(e.currentTarget.videoWidth,e.currentTarget.videoHeight)}/>:audioPlayer(media,native?mediaBlob.url:media.src,!browsed)}
 <figcaption>{media.name}{(mediaBlob.error||mediaFault)&&<> · <span className="error-message">{mediaBlob.error??mediaFault}</span></>}</figcaption></figure>
     :!media?<p className="empty-note" role={hint?'status':undefined}>{t(hint||'studio.run.empty')}</p>:null}
     </div>
     {viewer&&<ImageViewer src={viewer.src} alt={viewer.alt} item={viewer.item} onClose={()=>setViewer(null)}/>}
    </section>
    <div className="wb-prompts">
     {isMusic?<MusicPrompts endpoint={endpoint} root={root} style={musicStyle} lyrics={musicLyrics} onStyle={setMusicStyle} onLyrics={setMusicLyrics} actions={promptActions}/>:<>
     <div className="wb-prompt" onDragOver={e=>{if(e.dataTransfer.types.includes(MEDIA_DRAG)||e.dataTransfer.types.includes('Files')){e.preventDefault();const item=promptImage(e.dataTransfer);e.dataTransfer.dropEffect=referenceMethod&&imageRefs.length<4&&(e.dataTransfer.types.includes(MEDIA_IMAGE_DRAG)||validReference(item))?'copy':'none';}}} onDrop={e=>{if(e.dataTransfer.types.includes(MEDIA_DRAG)||e.dataTransfer.types.includes('Files'))e.preventDefault();const item=promptImage(e.dataTransfer);if(referenceMethod&&validReference(item)&&imageRefs.length<4){setImageRefs(v=>v.length>=4?v:[...v,{item:{...item},strength:0.6}]);if(revisionOk&&native&&!(choices?.visionModels?.length??0))setVisionDownload(true);}}}><div className="wb-prompt-label"><label htmlFor="run-positive">{t('studio.run.positive')}</label><small data-prompt-count="positive">{positive.length} / {16384}</small></div><PromptTextarea root={root} target={nativeTarget(dialectOf(model?.family))??'generic'} id="run-positive" data-run-positive value={positive} maxLength={16384} placeholder={t('studio.run.positiveHint')} onChange={e=>setPositive(e.target.value)}/><button type="button" className="wb-prompt-expand" data-prompt-expand="positive" aria-label={t('studio.run.prompt.expand',{name:t('studio.run.positive')})} title={t('studio.run.prompt.expand',{name:t('studio.run.positive')})} aria-pressed={detached==='positive'} onClick={()=>setDetached(detached==='positive'?null:'positive')}>⤢</button>{promptActions('positive',positive,setPositive)}{forgeContext&&onOpenPromptForge?<button type="button" className="forge-anchor forge-hammer forge-link" data-forge-open aria-label={t('forge.button')} title={t('studio.tab.forge')} onClick={onOpenPromptForge}><HammerIcon/></button>:<ForgeButton endpoint={endpoint} root={root} dialect={dialectOf(model?.family)} value={positive} image={pipeline==='i2i'?inputImage||undefined:undefined} onApply={setPositive} onWildcards={openWildcards} wildcard={wildcardEnabled?wildcard:undefined} h3={forgeContext} tools={promptTools}/>}</div>
     <div className="wb-prompt wb-negative"><div className="wb-prompt-label"><label htmlFor="run-negative">{t('studio.run.negative')}</label><small data-prompt-count="negative">{negative.length} / {16384}</small></div><PromptTextarea root={root} target={nativeTarget(dialectOf(model?.family))??'generic'} id="run-negative" data-run-negative value={negative} maxLength={16384} placeholder={t('studio.run.negativeHint')} onChange={e=>setNegative(e.target.value)}/><button type="button" className="wb-prompt-expand" data-prompt-expand="negative" aria-label={t('studio.run.prompt.expand',{name:t('studio.run.negative')})} title={t('studio.run.prompt.expand',{name:t('studio.run.negative')})} aria-pressed={detached==='negative'} onClick={()=>setDetached(detached==='negative'?null:'negative')}>⤢</button>{promptActions('negative',negative,setNegative)}</div></>}
     {wildcardVarChips.length>0&&<div className="wb-var-chips wb-wide" data-var-chips aria-label={t('wildcards.varsTitle')}>{wildcardVarChips.map((tok,i)=><button key={`${tok.key}:${i}`} type="button" className="wb-var-chip" data-var-chip={tok.name} aria-label={`$${tok.name}`} title={tok.known?`$${tok.name} → ${tok.pick}`:t('wildcards.varUnknown',{name:tok.name})} onClick={()=>{setDockHidden(false);setDockTab('wildcards');}}>{`$${tok.name}`}{tok.known?` → ${tok.pick}`:' ?'}</button>)}</div>}
     {!isMusic&&imageRefs.length>0&&<p className="wb-active wb-wide" data-reference-chip><span>{t('studio.run.reference.max',{count:imageRefs.length,max:4})}{!referenceMethod&&<> · {t('studio.run.reference.inactive')}</>}</span>{revisionOk&&<>{pick('studio.run.reference.vision',refVision,choices?.visionModels??[],setRefVision,loadChoices)}{!(choices?.visionModels?.length??0)&&<span>{t('studio.run.reference.needs')}</span>}</>}</p>}
     {!isMusic&&imageRefs.length>0&&<div className="wb-reference-strip wb-wide">
      {imageRefs.map((r,i)=><div key={`${i}:${r.item.path}`} className="wb-reference-card" data-reference-controls data-history-reference-unresolved={historyDraft&&!r.item.path?true:undefined} data-reference-position={i+1} role="group" aria-label={t('studio.run.reference.position',{position:i+1})} draggable onDragStart={e=>{e.stopPropagation();e.dataTransfer.clearData();e.dataTransfer.setData(referenceDrag,String(i));e.dataTransfer.effectAllowed='move';draggedReference.current=r;}} onDragEnd={()=>{draggedReference.current=null;}} onDragOver={e=>{if(e.dataTransfer.types.includes(referenceDrag)&&draggedReference.current){e.preventDefault();e.stopPropagation();e.dataTransfer.dropEffect='move';}else if(historyDraft&&(e.dataTransfer.types.includes(MEDIA_DRAG)||e.dataTransfer.types.includes('Files'))){e.preventDefault();e.stopPropagation();e.dataTransfer.dropEffect=historyReferenceCompatible&&(e.dataTransfer.types.includes(MEDIA_IMAGE_DRAG)||validReference(promptImage(e.dataTransfer)))?'copy':'none';}}} onDrop={e=>{if(e.dataTransfer.types.includes(referenceDrag)){e.preventDefault();e.stopPropagation();moveReference(imageRefs.indexOf(draggedReference.current!),i);draggedReference.current=null;}else if(historyDraft&&(e.dataTransfer.types.includes(MEDIA_DRAG)||e.dataTransfer.types.includes('Files'))){e.preventDefault();e.stopPropagation();replaceHistoryReference(i,promptImage(e.dataTransfer));}}}>
       <div className="wb-reference-thumb">{r.item.src?<img src={r.item.src} alt={r.item.name} title={r.item.name} draggable={false}/>:<span role="status" title={r.item.name}>{t('studio.history.missingInput',{name:r.item.name})}</span>}<span className="wb-reference-position" title={t('studio.run.reference.position',{position:i+1})}>{i+1}</span><button type="button" className="wb-reference-remove" data-reference-remove aria-label={`${t('studio.run.reference.remove')} · ${i+1}`} title={t('studio.run.reference.remove')} onClick={()=>setImageRefs(v=>v.filter((_,j)=>j!==i))}>✕</button></div>
       {historyDraft&&!r.item.path&&<><small>{t('studio.history.referenceReselect')}</small><button type="button" data-history-reference-select={i} disabled={!historyReferenceCompatible||!validReference(browsed??media)} onClick={()=>replaceHistoryReference(i,browsed??media)}>{t('studio.history.referenceUseSelected',{position:i+1})}</button></>}
       {revisionOk&&num('reference.strength',r.strength,n=>setImageRefs(v=>v.map((e,j)=>j===i?{...e,strength:clamp(n,0,1)}:e)),0,1,0.05,undefined,true)}
      </div>)}
     </div>}
     {!isMusic&&detached&&<PromptOverlay root={root} target={nativeTarget(dialectOf(model?.family))??'generic'} key={detached} title={t(`studio.run.${detached}`)} value={detached==='positive'?positive:negative} onChange={detached==='positive'?setPositive:setNegative} onClose={()=>setDetached(null)} placeholder={t(detached==='positive'?'studio.run.positiveHint':'studio.run.negativeHint')} maxLength={16384} closeLabel={t('studio.run.prompt.close')}>{detached==='positive'&&(forgeContext&&onOpenPromptForge?<button type="button" className="forge-anchor forge-hammer forge-link" data-forge-open aria-label={t('forge.button')} title={t('studio.tab.forge')} onClick={onOpenPromptForge}><HammerIcon/></button>:<ForgeButton endpoint={endpoint} root={root} dialect={dialectOf(model?.family)} value={positive} image={pipeline==='i2i'?inputImage||undefined:undefined} onApply={setPositive} onWildcards={openWildcards} wildcard={wildcardEnabled?wildcard:undefined} h3={forgeContext} tools={promptTools}/>)}</PromptOverlay>}
    </div>
    {wildcardEnabled&&<div className="wildcard-prompt-status"><button type="button" className="wildcard-badge" title={t('wildcards.baseNotice')} onClick={()=>setDockTab('wildcards')}>{t(wildcard.auto_reroll?'wildcards.activeAuto':'wildcards.active')}</button><button type="button" className="wildcard-badge" onClick={()=>setWildcardEnabled(false)}>{t('wildcards.disable')}</button></div>}
   </div>
   {columnHandle('queue')}
   <section id="studio-queue" style={{flexBasis:queueSize}} className="wb-queue" aria-label={t('studio.run.queue')}>
    <RecentHistory liveIds={jobs.map(j=>j.id)} context={{endpoint,inputRoot:roots.input,allow:choices,models:models??[]}} outputRoot={outputDir} projectOpen={!!project} authoringBusy={working||autoPlan||resolving||!!planRun.current} onRestore={restoreHistory} onOpenChange={setHistoryOpen}/>
    <h3>{t('studio.run.queue')}</h3>
    {jobs.length===0?<p className="media-help">{t('studio.run.queueEmpty')}</p>:<ul hidden={historyOpen}>{jobs.map(j=><li key={j.id} data-job-id={j.id}>{(terminalOutput(j)?j.outputs:[null]).map((o,index)=><div className="wb-queue-output" key={index}><button className="wb-queue-select" type="button" aria-current={j.id===selectedJob&&(!j.outputs.some(o=>o.role)||index===outputIndex)} data-state={j.state} onClick={()=>pickJob(j,index)} onKeyDown={e=>{if((e.key==='ContextMenu'||e.key==='F10'&&e.shiftKey)&&native&&(reusable(j)||stoppable(j))){e.preventDefault();const r=e.currentTarget.getBoundingClientRect();setJobMenu({x:r.left,y:r.bottom,job:j,trigger:e.currentTarget});}}} onContextMenu={e=>{if(!native||!(reusable(j)||stoppable(j)))return;e.preventDefault();setJobMenu({x:e.clientX,y:e.clientY,job:j,trigger:e.currentTarget});}}>
     {o&&outputItem(j,o).media==='audio'?<span className="wb-thumb" aria-hidden="true">♪</span>:j.outputRoot&&o&&!isVideoOut(o)?<img src={fileSrc(j,o)!} alt="" draggable onDragStart={e=>dragItem(e.dataTransfer,outputItem(j,o))}/>:o&&isVideoOut(o)?<span className="wb-thumb" aria-hidden="true">▶</span>:j.preview?<img src={j.preview} alt=""/>:<span className="wb-thumb" aria-hidden="true">{j.state==='cancelled'?'×':j.state==='failed'||j.state==='unknown'?'!':j.state==='succeeded'?'✓':'…'}</span>}
     <span className="wb-queue-text"><strong>{t(`studio.jobs.${j.state}`)} · {j.id.slice(0,8)}{duration(j)}{j.progress&&<> {j.progress.value}/{j.progress.max}</>}</strong><small>{o?.role&&<strong>{t(`studio.jobs.role.${o.role}`)} · </strong>}{o?.filename??j.outputs[0]?.filename??j.id.slice(0,8)}</small></span></button>{o&&j.outputRoot&&<StarMediaButton item={outputItem(j,o)}/>}</div>)}</li>)}</ul>}
    {jobMenu&&<div className="media-menu" role="menu" aria-label={jobMenu.job.id.slice(0,8)} style={{left:Math.min(jobMenu.x,window.innerWidth-200),top:Math.min(jobMenu.y,window.innerHeight-48)}}>{reusable(jobMenu.job)&&<button type="button" role="menuitem" autoFocus data-job-reuse disabled={working||autoPlan||resolving||!!planRun.current} onClick={()=>{const job=jobMenu.job;setJobMenu(null);reuseJob(job);}}>{t('studio.run.metadata.apply')}</button>}{jobMenu.job.intent&&(isImageIntent(jobMenu.job.intent)||isVideoIntent(jobMenu.job.intent))&&<button type="button" role="menuitem" data-job-reuse-prompt disabled={working||autoPlan||resolving||!!planRun.current} onClick={()=>{const job=jobMenu.job;setJobMenu(null);reusePrompt(job);}}>{t('studio.run.metadata.reuse')}</button>}{stoppable(jobMenu.job)&&<button type="button" role="menuitem" autoFocus={!reusable(jobMenu.job)} data-job-stop onClick={()=>{const job=jobMenu.job;setJobMenu(null);void stopJob(job);}}>{t('studio.run.stop.job')}</button>}</div>}
     <div className="wb-run-actions">
     <button ref={runButton} type="button" className="aio-confirm-button wb-run" data-run disabled={!native||scanning||admissionBlocked||autoPlan||resolving||!!planRun.current||!!sizeError||(inpaint&&!masked)||!checkpoint||backendPending||unavailable.length>0||musicNeedsStyle||historyProblems.length>0} title={!native?t('studio.backendHelp'):backendPending?t('studio.run.backendPending'):historyProblems.length?t('studio.history.blocked',{items:historyProblems.join(' · ')}):busy||current?undefined:sizeError?t(sizeError,{multiple:String(grid)}):inpaint&&!masked?t('studio.run.edit.needsMask'):!checkpoint?t(why(pipeline)||'studio.run.noModels'):unavailable.length?t('studio.quick.unavailable',{names:unavailable.join(', ')}):musicNeedsStyle?t('studio.music.needsStyle'):undefined} onClick={()=>void run()}>{t('studio.run.run')}</button>{(working||current||autoPlan||resolving||preparing>0||frontendAdmitting||stopping||(!recovered&&!!error))&&<button type="button" className="wb-run-stop" data-run-stop disabled={stopping} aria-label={t('studio.run.stop.label')} title={t('studio.run.stop.hint')} onClick={()=>void stop()}>{t(stopping?'studio.run.stop.stopping':'studio.run.stop.label')}</button>}<span id="project-actions-slot"/></div>
     <div id="project-status-slot"/>{stopStatus&&<p role="status" data-run-stop-status>{stopStatus}</p>}
   </section>
  </div>
  <div id="studio-dock" className="wb-dock" data-view={dockView} data-collapsed={dockHidden||undefined} style={dockHidden?undefined:{height:dockSize}}>
   <div className="wb-dock-handle" role="separator" aria-orientation="horizontal" aria-label={t('studio.run.dockResize')} title={t('studio.run.dockResize')} tabIndex={0} aria-valuemin={120} aria-valuemax={dockMax} aria-valuenow={dockSize} data-locked={dockHidden||undefined} onKeyDown={e=>{if(!dockHidden)resizeKey(e,dockSize,120,dockMax,setDockHeight,true);}} onPointerDown={e=>{if(!dockHidden)dragPane(e,dockSize,120,dockMax,setDockHeight,true);}}/>
   {paneToggle(dockHidden,setDockHidden,'dock','studio-dock')}
   {dockTab==='media'?<MediaPanel selected={browsed??media} onSelect={setBrowsed} roots={roots} outputRevision={outputRevision} controls={{query,onQuery:setQuery,sort,onSort:setSort,view:dockView,onView:setDockView,type:dockType,onType:setDockType}} toolbar={dockNavigation} marks={marks} actions={mediaActions}/>:<div className="media-toolbar">{dockNavigation}
    {(dockTab==='models'||dockTab==='loras')&&<><MediaControls query={query} onQuery={setQuery} sort={sort} onSort={setSort} view={dockView} onView={setDockView} sorts={sorts}/><button type="button" className="wb-dock-action" data-run-load disabled={!native||busy||(!modelsDir&&!endpoint.trim())} onClick={()=>void load()}>{t(models?'studio.run.reload':'studio.run.load')}</button></>}
   </div>}
   {dockTab==='presets'&&<div className="wb-dock-panel"><PresetPanel disabled={busy||!!current||autoPlan||resolving}/></div>}
    {dockTab==='wildcards'&&<div className="wb-dock-panel"><WildcardPanel library={wildcardLibrary} loading={wildcardLoading} error={wildcardLibraryError} onReload={()=>setWildcardRevision(v=>v+1)} value={wildcard} onChange={setWildcard} enabled={wildcardEnabled} onEnabled={setWildcardEnabled} positive={positive} negative={negative} onInsert={name=>setPositive(p=>`${p.trim()?`${p} `:''}$${name}`)}/></div>}
{dockTab==='post'&&<section className="wb-dock-panel wb-post" aria-label={t('studio.run.dock.post')}>
    {latent&&<fieldset className="wb-group" data-post-group="latent"><legend>{t('studio.run.post.group.latent')}</legend><div className="wb-options">
     <div className="wb-option"><label className="wb-option-row"><input data-latent-upscale-enabled type="checkbox" checked={latentPref.enabled} onChange={e=>{setLatentPref({...latentPref,enabled:e.target.checked,model:latentPref.model||latentModel});if(e.target.checked&&choices&&!latentChoices.includes(latentModel))setLatentDownload(videoFamily!);}}/>{t('studio.run.latent.enabled')}</label><small>{t('studio.run.latent.hint',{width:String(latentSize.width),height:String(latentSize.height)})}</small></div>
     <label className="wb-option"><span>{t('studio.run.latent.model')}</span><select data-latent-upscale-model value={latentModel} onChange={e=>setLatentPref({...latentPref,model:e.target.value})}>{!latentChoices.includes(latentModel)&&<option value={latentModel}>{latentModel||t('studio.run.latent.choose')}</option>}{latentChoices.map(m=><option key={m} value={m}>{m}</option>)}</select></label>
     <label className="wb-option"><span>{t('studio.run.upscale')}</span><input data-latent-upscale-scale type="number" min={1} max={8} step={0.25} disabled={videoFamily==='ltx-2'} value={videoFamily==='ltx-2'?2:latentPref.scale} onChange={e=>setLatentPref({...latentPref,scale:Number(e.target.value)})}/></label>
     {videoFamily==='minimax-h3'&&<><label className="wb-option"><span>{t('studio.run.latent.steps')}</span><input data-latent-upscale-steps type="number" min={1} max={100} step={1} value={latentPref.steps} onChange={e=>setLatentPref({...latentPref,steps:Number(e.target.value)})}/><small>{t('studio.run.latent.stepsHint')}</small></label><label className="wb-option"><span>{t('studio.run.latent.denoise')}</span><input data-latent-upscale-denoise type="number" min={0} max={1} step={0.05} value={latentPref.denoise} onChange={e=>setLatentPref({...latentPref,denoise:Number(e.target.value)})}/></label></>}
    </div>
     {videoFamily==='minimax-h3'&&(()=>{const a={...h3LatentDefaults,...latentPref.advanced};const set=(patch:H3LatentAdvanced)=>setLatentPref({...latentPref,advanced:{...latentPref.advanced,...patch}});
       const num=(k:'cfg'|'memoryBudgetMb'|'guardSensitivity'|'guardStrength'|'memoryReservePct'|'continuityMaskStrength',min:number,max:number,step:number,hint?:string)=><label className="wb-option" key={k}><span>{t(`studio.run.latent.${k}`)}</span><input data-latent-advanced={k} type="number" min={min} max={max} step={step} value={a[k]} onChange={e=>set({[k]:Number(e.target.value)})}/>{hint&&<small>{t(hint)}</small>}</label>;
       const flag=(k:'spatialTiling'|'temporalChunking'|'spikeGuard'|'memoryPlanning'|'continuitySoftRefine',hint?:string)=><div className="wb-option" key={k}><label className="wb-option-row"><input data-latent-advanced={k} type="checkbox" checked={a[k]} onChange={e=>set({[k]:e.target.checked})}/>{t(`studio.run.latent.${k}`)}</label>{hint&&<small>{t(hint)}</small>}</div>;
       const choose=(k:'scheduler'|'precision'|'guardMode',options:readonly string[])=><label className="wb-option" key={k}><span>{t(`studio.run.latent.${k}`)}</span><select data-latent-advanced={k} value={a[k]} onChange={e=>set({[k]:e.target.value} as H3LatentAdvanced)}>{(options.includes(a[k])?options:[a[k],...options]).map(o=><option key={o} value={o}>{o}</option>)}</select></label>;
       return <details className="wb-model-files" data-latent-upscale-more><summary aria-label={t('studio.run.latent.more')}>☰ {t('studio.run.latent.more')}</summary><div className="wb-options">
        {num('cfg',0,20,0.1)}{choose('scheduler',choices?.schedulers??[h3LatentDefaults.scheduler])}{choose('precision',['auto','bf16','fp16','fp32'])}
        {flag('spatialTiling')}{flag('temporalChunking')}
        {flag('memoryPlanning','studio.run.latent.memoryPlanningHint')}{a.memoryPlanning&&num('memoryReservePct',0,100,5)}{num('memoryBudgetMb',0,131072,256,'studio.run.latent.memoryBudgetHint')}
        {flag('spikeGuard','studio.run.latent.spikeGuardHint')}{a.spikeGuard&&<>{num('guardSensitivity',1.5,12,0.1)}{num('guardStrength',0,1,0.05)}{choose('guardMode',['burn_direction','both'])}</>}
        {flag('continuitySoftRefine','studio.run.latent.continuityHint')}{a.continuitySoftRefine&&num('continuityMaskStrength',0,1,0.05)}
        </div><button type="button" className="wb-dock-action" data-latent-advanced-reset disabled={!h3LatentChanged(latentPref.advanced)} onClick={()=>setLatentPref({...latentPref,advanced:undefined})}>{t('studio.run.latent.reset')}</button>
       </details>;})()}
     {latentProblems.map(problem=><p role="status" key={problem}>{problem}</p>)}
     <button type="button" className="wb-dock-action" data-latent-upscale-offer onClick={()=>setLatentDownload(videoFamily!)}>{t('studio.run.latent.download')}</button>
    </fieldset>}
    {!isVideo&&<fieldset className="wb-group" data-post-group="refine"><legend>{t('studio.run.post.group.refine')}</legend><div className="wb-options">
     <div className="wb-option"><span><label className="wb-option-row"><input type="checkbox" checked={post} onChange={e=>setPost(e.target.checked)}/>{t('studio.run.post.enabled')}</label></span><small>{t('studio.run.post.hint',{width:String(up(outputSize).width),height:String(up(outputSize).height)})}</small></div>
     <div className="wb-option"><span><label className="wb-option-row"><input type="checkbox" checked={postTiled&&!family?.temporalLatent} disabled={!!family?.temporalLatent||(!!choices&&!choices.nodes?.includes('DaSiWaTiledDiffusion'))} onChange={e=>setPostTiled(e.target.checked)}/>{t('studio.run.post.tiled')}</label></span><small>{t(family?.temporalLatent?'studio.run.post.tiledFamily':choices&&!choices.nodes?.includes('DaSiWaTiledDiffusion')?'studio.run.post.tiledMissing':'studio.run.post.tiledHint',{width:String(snap(modelSize.width)),height:String(snap(modelSize.height)),overlap:String(snap(Math.max(64,Math.min(modelSize.width,modelSize.height)/8)))})}</small></div>
     <label className="wb-option"><span>{t('studio.run.post.denoise')}</span><input type="number" value={refineDenoise} min={0.05} max={1} step={0.05} onChange={e=>{const n=Number(e.target.value);if(Number.isFinite(n))setRefineDenoise(clamp(n,0.05,1));}}/></label>
     <label className="wb-option"><span>{t('studio.run.upscale')}</span><input type="number" value={upscale} min={1} max={4} step={0.25} onChange={e=>{const n=Number(e.target.value);if(Number.isFinite(n))setUpscale(clamp(n,1,4));}}/></label>
     <label className="wb-option"><span>{t('studio.run.post.model')}</span><select value={upscaleModel} onFocus={loadChoices} onPointerDown={loadChoices} onChange={e=>setUpscaleModel(e.target.value)}><option value="">{t('studio.run.post.lanczos')}</option>{(choices?.upscaleModels??[]).map(m=><option key={m} value={m}>{m}</option>)}</select></label>
    </div></fieldset>}
    {isVideo&&<>
     <fieldset className="wb-group" data-post-group="model"><legend>{t('studio.run.post.model')}</legend><div className="wb-options">
      <label className="wb-option"><select data-tve-post="upscaleModel" aria-label={t('studio.run.post.model')} value={tve.upscaleModel} onFocus={loadChoices} onPointerDown={loadChoices} onChange={e=>setTve({...tve,upscaleModel:e.target.value})}><option value="">{t('studio.tve.post.off')}</option>{(choices?.upscaleModels??[]).map(m=><option key={m} value={m}>{m}</option>)}{tve.upscaleModel&&!choices?.upscaleModels?.includes(tve.upscaleModel)&&<option value={tve.upscaleModel}>{tve.upscaleModel}</option>}</select><small>{t('studio.run.post.group.modelHint')}</small></label>
     </div></fieldset>
     <fieldset className="wb-group" data-post-group="tve"><legend>{t('studio.tve.post.title')}</legend><TvePostControls value={tve} onChange={setTve} allow={choices} onRead={loadChoices} fps={fps} modelCard={false}/>{onOpenVideoPost&&<button type="button" className="wb-dock-action" data-open-video-post onClick={onOpenVideoPost}>{t('studio.tve.open')}</button>}</fieldset>
    </>}
    {!isVideo&&onOpenVideoPost&&<fieldset className="wb-group" data-post-group="tve"><legend>{t('studio.tve.post.title')}</legend>{onOpenVideoPost&&<button type="button" className="wb-dock-action" data-open-video-post onClick={onOpenVideoPost}>{t('studio.tve.open')}</button>}</fieldset>}
   </section>}

   {dockTab==='options'&&<section className="wb-dock-panel" aria-label={t('studio.run.dock.options')}><OptionsPanel value={optionsPref} onChange={setOptions} allow={choices} family={family?.id} imageInput={pipeline==='i2i'} onRead={loadChoices} canRead={native&&!!endpoint.trim()}/></section>}
   {(dockTab==='models'||dockTab==='loras')&&<section className="wb-dock-panel" aria-label={t(`studio.run.dock.${dockTab}`)}>
    {!models&&<p className="media-help">{t(modelsDir?'studio.run.scanHelp':'studio.run.loadHelp')}</p>}
    {models&&dockTab==='models'&&(allModels.length===0?<p className="media-help">{t('studio.run.noModels')}</p>:<div className="media-browser">{folderTree(allModels)}<ul className="wb-tiles" data-run-checkpoint>{allModels.filter(match).sort(bySort).map(m=>tile(m,m===model,()=>pickModel(m),undefined,blocked(m,pipeline)))}</ul></div>)}
    {models&&dockTab==='loras'&&(loraNames.length===0?<p className="media-help">{t('studio.run.noLoras')}</p>:<div className="media-browser">{folderTree(loraModels)}<ul className="wb-tiles wb-loras">{loraModels.filter(match).sort(bySort).map(m=>{const on=loras.find(l=>l.name===m.name);return tile(m,!!on,()=>toggleLora(m.name,!on),on&&<input type="number" aria-label={t('studio.run.strength',{name:m.name})} value={on.strength} min={-10} max={10} step={0.05} onChange={e=>{const n=Number(e.target.value);if(Number.isFinite(n)&&Math.abs(n)<=10)setLoras(loras.map(l=>l.name===m.name?{...l,strength:n}:l));}}/>);})}</ul></div>)}
   </section>}
  </div>
 </div>
 {visionDownload&&<dialog ref={visionDialog} data-vision-download className="continuity-overlay" aria-label={t("studio.run.reference.dlAsk")} onCancel={e=>{e.preventDefault();closeVisionDownload();}}><header className="continuity-overlay-header"><h2>{t("studio.run.reference.dlAsk")}</h2><button type="button" onClick={closeVisionDownload}>{t("studio.close")}</button></header><div className="wb-dock-panel"><p>{t("studio.run.latent.consent")}</p><DownloadPanel embedded installTarget={root} modelsDir={modelsDir} initialEntryIds={visionIds} filenameFilter={["clip_vision_g.safetensors"]} onStatus={visionDownloadStatus}/></div></dialog>}
 {latentDownload&&<dialog ref={latentDialog} data-latent-upscale-download className="continuity-overlay" aria-label={t('studio.run.latent.download')} onCancel={e=>{e.preventDefault();closeLatentDownload();}}><header className="continuity-overlay-header"><h2>{t('studio.run.latent.download')}</h2><button type="button" onClick={closeLatentDownload}>{t('studio.close')}</button></header><div className="wb-dock-panel"><p>{t('studio.run.latent.consent')}</p><DownloadPanel embedded installTarget={root} modelsDir={modelsDir} initialEntryIds={latentIds} filenameFilter={latentDownload==='minimax-h3'?['minimax_h3_latent_upscaler_3d_conv_v1_bf16.safetensors','MiniMax H3 Latent Upscaler.safetensors','minimax h3 latent upscaler.safetensors','minimax_h3_latent_upscaler.safetensors']:['ltx-2.3-spatial-upscaler-x2-1.1.safetensors','LTX-2.3-Latent-Upscaler.safetensors']} onStatus={latentDownloadStatus}/></div></dialog>}
 {referencesOpen&&refsTab&&!planVisible&&<ReferencesOverlay endpoint={endpoint} root={root} refs={refs} onChange={setRefs} family={videoFamily as 'ltx-2'|'minimax-h3'} seconds={video.seconds} label={refLabel} onAdd={addRefs} inputRoot={roots.input} refmods={refmods} onRefmods={setRefmods} library={native&&endpoint.trim()?()=>readRefMods(endpoint):undefined} onInsert={tag=>setPositive(positive.trim()?`${positive.trimEnd()} ${tag}`:tag)} onClose={()=>{setReferencesOpen(false);referencesButton.current?.focus();}} picker={<MediaPanel toolbar={error?<p role="alert" className="error-message">{backendErrorText(error,t)}</p>:undefined} selected={null} onSelect={toggleRef} roots={roots} outputRevision={outputRevision} controls={{query,onQuery:setQuery,sort,onSort:setSort,view:dockView,onView:setDockView,type:dockType,onType:setDockType}} marks={marks} actions={mediaActions}/>}/> }
 {/* Keep H3 editors with their Workbench state; only their DOM lives in the retained Forge tab. */}
 {forgeHost&&forgeContext&&createPortal(<section className="forge-tab" aria-label={t('studio.tab.forge')} data-h3-forge-tab>
  <ForgeButton inline active={forgeActive} endpoint={endpoint} root={root} dialect={dialectOf(model?.family)} value={positive} onApply={setPositive} h3={forgeContext} tools={promptTools}/>
  <div className="forge-output">
   {frameVideo&&[firstFrame,lastFrame].map((item,i)=>item&&<div className="wb-wide" key={`${i}:${item.src}`} data-frame-people={i===0?'first':'last'}>
       <ReferencePeople key={item.src} prominent title={`${t(i===0?'forge.role.first-frame':'forge.role.last-frame')} · ${item.name}`} r={{item,second:0,strength:1,mediaMode:'video',who:[],...framePeople[item.src]}} label="" allowEmpty endpoint={endpoint} root={root} inputRoot={roots.input} onChange={x=>setFramePeople(p=>({...p,[item.src]:{...p[item.src],...x}}))}/>
      </div>)}
   {pipeline==='ref2v'&&refs.map((r,i)=>r.item.media==='image'&&labelCharacters(forgeLabels(refs)[i]).length>0&&<div key={r.item.src}>
    <ReferencePeople prominent title={r.item.name} r={r} label={forgeLabels(refs)[i]} endpoint={endpoint} root={root} inputRoot={roots.input} onChange={x=>setRefs(current=>current.map((pick,j)=>j===i?{...pick,...x}:pick))}/>
   </div>)}
  </div>
 </section>,forgeHost)}
 {continuityOpen&&continuityAvailable&&!planVisible&&<ContinuityOverlay endpoint={endpoint} root={root} forgeContext={forgeContext} allowReferences={continuityReferences} plan={plan} onChange={setPlan} onRun={runPlan} running={autoPlan||resolving||!!planRun.current||!!current} disabled={!native||busy||admissionBlocked||!isVideo||!!continuitySizeError||unavailable.length>0} onClose={()=>{setContinuityOpen(false);continuityButton.current?.focus();}}/>}
 </div></>;
}
