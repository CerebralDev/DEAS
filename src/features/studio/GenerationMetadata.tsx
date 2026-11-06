import { isVideoEnhanceIntent, isVideoIntent, latentUpscaleSize, videoPostFrameRate } from '../../lib/videoIntent';
import { describeVideoPost } from './TvePostControls';
import { isAceMusicIntent, isMusicIntent } from '../../lib/musicIntent';
import { useContext, useEffect, useState, type ReactNode } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import type { MediaItem } from '../../lib/media';
import { generationSettings, imageMetadataRows, readImageMetadata, type GenerationSettings, type ImageMetadata } from '../../lib/imageMetadata';
import { embeddedWorkflow, OpenComfyWorkflow, type ComfyWorkflow } from '../../lib/comfyWorkflow';
import { isImageIntent, type JobStatus } from '../../lib/jobs';
import { useI18n } from '../../i18n/I18nProvider';
import { generationDuration } from './generationDuration';

function FileMetadata({media}:{media:MediaItem}) {
 const {t}=useI18n();const path=media.path;
 const openWorkflow=useContext(OpenComfyWorkflow);
 const [sending,setSending]=useState(false);const [sendError,setSendError]=useState(false);
 const [file,setFile]=useState<{path:string;rows:ImageMetadata;workflow:ComfyWorkflow|null;settings:GenerationSettings|null;error:boolean}|null>(null);
 useEffect(()=>{
  if(!path||!isTauri())return;
  let alive=true;setFile(null);
  readImageMetadata(path).then(entries=>{if(alive)setFile({path,rows:imageMetadataRows(entries),workflow:embeddedWorkflow(entries),settings:generationSettings(entries),error:false});},()=>{if(alive)setFile({path,rows:[],workflow:null,settings:null,error:true});});
  return()=>{alive=false;};
 },[path,media.src]);
 if(!path||!isTauri())return <p className="media-help">{t('studio.run.metadata.native')}</p>;
 if(file?.path!==path)return <p className="media-help" role="status">{t('studio.run.metadata.loading')}</p>;
 if(file.error)return <p className="media-help" role="status">{t('studio.run.metadata.failed')}</p>;
 if(!file.rows.length&&!file.workflow)return <p className="media-help">{t('studio.run.metadata.none')}</p>;
 const labels:Record<string,string>={prompt:'studio.run.positive',negativeprompt:'studio.run.negative',negative_prompt:'studio.run.negative',model:'studio.run.checkpoint',seed:'studio.run.seed',steps:'studio.run.steps',cfgscale:'studio.run.cfg',width:'studio.run.width',height:'studio.run.height',sampler:'studio.recipe.sampler',scheduler:'studio.recipe.scheduler',vae:'studio.run.vae',loras:'studio.run.loras',date:'studio.run.metadata.date',generation_time:'studio.run.metadata.time'};
 return <>{file.workflow&&<><button type="button" className="wb-dock-action" disabled={!openWorkflow||sending} title={t('studio.run.metadata.workflowHelp')} onClick={async()=>{if(!openWorkflow||!file.workflow)return;setSending(true);setSendError(false);try{await openWorkflow(file.workflow,media.name);}catch{setSendError(true);}finally{setSending(false);}}}>{t(sending?'studio.run.metadata.workflowLoading':'studio.run.metadata.workflow')}</button>{!openWorkflow&&<p className="media-help">{t('studio.run.metadata.workflowBackend')}</p>}{sendError&&<p className="media-help" role="alert">{t('studio.run.metadata.workflowFailed')}</p>}</>}{/* "Use these settings" lives in the preview's More menu now; the dock only displays the file's metadata. */}
  <dl>{file.rows.map(([name,value],index)=><div key={index}><dt>{Object.hasOwn(labels,name.toLowerCase())?t(labels[name.toLowerCase()]):name.replaceAll('_',' ')}</dt><dd>{value}</dd></div>)}</dl></>;
}

/** Read the selected job's native snapshot or the selected image; controls change only through the explicit apply button. */
export function GenerationMetadata({job,media=null,onRestore}:{job:JobStatus|null;media?:MediaItem|null;onRestore?:(job:JobStatus)=>void}) {
 const {t,locale}=useI18n();
 const duration=generationDuration(job,locale);
 // Image intents list every field; video intents share the sampling rows plus frames/fps and the family block.
 const intent=job?.intent;const v=intent&&isVideoIntent(intent)?intent:undefined;const i=intent&&isImageIntent(intent)?intent:undefined;const m=intent&&isMusicIntent(intent)?intent:undefined;const e=intent&&isVideoEnhanceIntent(intent)?intent:undefined;
 const ace=m&&isAceMusicIntent(m)?m:undefined;
 const extensionSeconds=v&&(v.family==='minimax-h3'||v.family==='minimax-h3-hyperflow')?v.h3.continuity?.durationSeconds:undefined;
 const row=(key:string,value:ReactNode)=><div key={key}><dt>{t(key)}</dt><dd>{value}</dd></div>;
 const audio=(v??m??e)?.audioEnhance;
 const audioRow=audio&&row('studio.audioEnhance.title',`${t(`studio.audioEnhance.${audio.mode}`)} · ${t(`studio.audioEnhance.${audio.profile}`)}${audio.mode==='stems'?` · ${t(`studio.audioEnhance.device.${audio.device??'auto'}`)}`:''}`);
 const timing=<>{job?.metadata&&Number.isFinite(new Date(job.metadata.startedAt).getTime())&&row('studio.run.metadata.date',new Date(job.metadata.startedAt).toLocaleString(locale))}{duration!==null&&row('studio.run.metadata.time',duration)}</>;
 return <section className="wb-metadata" aria-label={t('studio.run.metadata')}>
  <h3>{t('studio.run.metadata')}</h3>
  {e?<dl data-tve-metadata>{row('studio.tve.source',e.sourceVideo)}{row('studio.tve.post.title',describeVideoPost(e.videoPost))}{row('studio.tve.fps',`${e.frameRate} → ${videoPostFrameRate(e.frameRate,e.videoPost)}`)}{row('studio.run.seed',e.seed)}{audioRow}{timing}{job?.appVersion&&row('studio.run.metadata.version',job?.appVersion)}</dl>
  :m?<dl data-music-metadata>{/* Music has no image "use these settings"; Recent history restores the whole record. */}
   {row('studio.music.style',m.style)}{m.lyrics&&row('studio.music.lyrics',m.lyrics)}{row('studio.run.checkpoint',m.checkpoint)}
   {ace?<>{row('studio.plan.family',ace.family)}{row('studio.run.textEncoder',ace.diffusion?.textEncoder??t('studio.plan.checkpointParts'))}{ace.diffusion?.textEncoder2&&row('studio.run.textEncoder2',ace.diffusion.textEncoder2)}{row('studio.run.vae',ace.diffusion?.vae??t('studio.plan.checkpointParts'))}{row('studio.music.maxDuration',m.maxDuration)}</>:<>{row('studio.music.planning',t(`studio.music.${m.planning}`))}{row('studio.music.maxDuration',`${m.maxDuration} · ${t('studio.music.durationHint')}`)}</>}
   {m.mode==='cover'&&<>{row('studio.music.source',m.sourceAudio)}{row('studio.music.audioEncoder',m.audioEncoder)}</>}
   {row('studio.run.seed',m.seed)}{row('studio.run.steps',m.steps)}{row('studio.run.cfg',m.cfg)}{row('studio.recipe.sampler',m.sampler)}{row('studio.recipe.scheduler',m.scheduler)}
   {audioRow}{ace?Object.entries(ace.ace).map(([k,v])=>row(`studio.music.ace.${k}`,typeof v==='boolean'?t(v?'studio.plan.yes':'studio.plan.no'):v)):row('studio.music.generationSettings',JSON.stringify(m.generation))}{!ace&&m.mode==='t2m'&&m.abc&&row('studio.music.abcSettings',JSON.stringify(m.abc))}
   {timing}{job?.appVersion&&row('studio.run.metadata.version',job?.appVersion)}
  </dl>:v?<>{onRestore&&job&&<button type="button" className="wb-dock-action" data-metadata-apply onClick={()=>onRestore(job)}>{t('studio.run.metadata.apply')}</button>}<dl>
   {row('studio.run.positive',v.positivePrompt)}{v.negativePrompt&&row('studio.run.negative',v.negativePrompt)}{row('studio.run.checkpoint',v.model)}{row('studio.run.resolution.title',`${v.width} × ${v.height} · ${extensionSeconds===undefined?`${v.frames} ${t('studio.run.video.frames')} · `:''}${v.frameRate} fps`)}
   {extensionSeconds!==undefined&&row('studio.run.cont.seconds',extensionSeconds)}{row('studio.run.seed',v.seed)}{row('studio.run.steps',v.steps)}{row('studio.run.cfg',v.cfg)}{row('studio.recipe.sampler',v.sampler)}{row('studio.recipe.scheduler',v.scheduler)}{row('studio.run.textEncoder',v.textEncoder)}{row('studio.run.vae',v.vae)}
   {!!v.loras?.length&&row('studio.run.loras',v.loras.map(l=>`${l.name} (${l.strength})`).join(', '))}{(v.firstFrame||v.lastFrame)&&row('studio.run.pipe.input',[v.firstFrame,v.lastFrame].filter(Boolean).join(' → '))}
   {row('studio.run.video.settings',JSON.stringify(v.family==='ltx-2'?v.ltx:v.family==='wan'?v.wan:{...v.h3,references:v.h3.references.map(r=>r.image)}))}
   {v.latentUpscale&&<>{row('studio.run.latent.model',v.latentUpscale.model)}{row('studio.run.dock.post',JSON.stringify(v.latentUpscale))}{row('studio.run.latent.enabled',`${latentUpscaleSize(v.family,v,v.latentUpscale.scale).width} × ${latentUpscaleSize(v.family,v,v.latentUpscale.scale).height}`)}</>}
   {audioRow}
   {v.videoPost&&row('studio.tve.post.title',`${describeVideoPost(v.videoPost)} · ${videoPostFrameRate(v.frameRate,v.videoPost)} fps`)}
   {timing}{job?.appVersion&&row('studio.run.metadata.version',job?.appVersion)}
  </dl></>:i?<>{onRestore&&job&&<button type="button" className="wb-dock-action" data-metadata-apply onClick={()=>onRestore(job)}>{t('studio.run.metadata.apply')}</button>}<dl>
   {row('studio.run.positive',i.positivePrompt)}
   {i.negativePrompt&&row('studio.run.negative',i.negativePrompt)}
   {i.wildcard&&<>{row('wildcards.title',<><strong>{t(i.wildcard.auto_reroll?'wildcards.activeAuto':'wildcards.active')}</strong><p>{t('wildcards.baseNotice')}</p>{Object.entries(i.wildcard.selection_state).filter(([,s])=>s.enabled).map(([key,s])=><div key={key}>{key} ({s.weight})</div>)}</>)}{row('wildcards.style',i.wildcard.style)}{row('wildcards.seed',i.wildcard.seed)}{row('wildcards.budget',i.wildcard.token_budget)}</>}
   {row('studio.run.checkpoint',i.checkpoint)}
   {row('studio.run.resolution.title',`${i.width} × ${i.height}`)}
   {row('studio.run.seed',i.seed)}
   {row('studio.run.steps',i.steps)}
   {row('studio.run.cfg',i.cfg)}
   {row('studio.recipe.sampler',i.sampler)}
   {row('studio.recipe.scheduler',i.scheduler)}
   {i.diffusion&&<>{row('studio.run.textEncoder',i.diffusion.textEncoder)}{row('studio.run.vae',i.diffusion.vae)}</>}
   {!!i.loras?.length&&row('studio.run.loras',i.loras.map(l=>`${l.name} (${l.strength})`).join(', '))}
   {i.inputImage&&<>{row('studio.run.pipe.input',i.inputImage)}{row('studio.run.denoise',i.denoise)}</>}
   {i.maskBlur!=null&&row('studio.run.maskBlur',i.maskBlur)}
   {i.inpaintCrop&&row('studio.run.inpaintOnlyMasked',`${i.inpaintCrop.x}, ${i.inpaintCrop.y} · ${i.inpaintCrop.width} × ${i.inpaintCrop.height} → ${i.inpaintCrop.targetWidth} × ${i.inpaintCrop.targetHeight}`)}
   {i.options&&row('studio.run.dock.options',Object.entries(i.options).map(([k,v])=>typeof v==='object'?`${v.node} ${v.threshold}`:v===true?k:`${k}: ${v}`).join(' · '))}
   {timing}{job?.appVersion&&row('studio.run.metadata.version',job?.appVersion)}
  </dl></>:!job&&media&&media.media!=='audio'?<FileMetadata key={media.src} media={media}/>:<p className="media-help">{t('studio.run.metadata.empty')}</p>}
 </section>;
}
