import type { ReactNode } from 'react';
import { useI18n } from '../../i18n/I18nProvider';
import { isImageIntent, type JobIntent } from '../../lib/jobs';
import { isMusicIntent } from '../../lib/musicIntent';
import { isVideoIntent, latentUpscaleSize, videoPostFrameRate } from '../../lib/videoIntent';
import { describeVideoPost } from './TvePostControls';

/** Authoring preview only: native preparation remains the validation/compilation authority. */
export function RunPlan({intent,pipeline,notices,outputRoot,inputNotes=[]}:{intent:JobIntent|null;pipeline:string;notices:string[];outputRoot?:string;inputNotes?:string[]}){
 const {t}=useI18n();
 const unknown=t('studio.plan.unresolved');
 const row=(label:string,value:ReactNode)=><div key={label}><dt>{t(label)}</dt><dd>{value===undefined||value===null||value===''?unknown:value}</dd></div>;
 const size=(s:{width:number;height:number})=>`${s.width} × ${s.height}`;
 const data=(v:unknown)=><details><summary>{t('studio.plan.details')}</summary><pre>{JSON.stringify(v,null,2)}</pre></details>;
 const yesNo=(v:boolean)=>t(v?'studio.plan.yes':'studio.plan.no');
 const optionRows=(o:NonNullable<JobIntent['options']>)=><>
  {o.attention&&row('studio.run.opt.attention',o.attention)}{o.sage&&row('studio.run.opt.attention',o.sage)}
  {o.cache&&<>{row('studio.run.opt.cache',o.cache.node)}{row('studio.run.opt.cache.threshold',o.cache.threshold)}</>}
  {o.compile&&row('studio.run.opt.compile',o.compile)}{o.vaeTile&&row('studio.run.opt.vae.tile',o.vaeTile)}
 </>;

 const stage=(id:string,body:ReactNode)=><li data-plan-stage={id}><h3>{t(`studio.plan.${id}`)}</h3><dl className="run-plan-facts">{body}</dl></li>;
 if(intent&&isMusicIntent(intent)){
  const music=intent;
  // Music keeps the stage chain (input → model → prompt → ABC → generate → output) without any image/video fields.
  const settings=(o:object)=>Object.entries(o).map(([k,v])=>row(k==='cfgScale'?'studio.music.arCfg':`studio.music.${k}`,v));
  return <section className="feature-panel run-plan" aria-label={t('studio.tab.map')}>
   <div className="panel-heading"><h2>{t('studio.tab.map')}</h2><span>{t(`studio.run.pipe.${pipeline}`)}</span></div>
   <p>{t('studio.plan.notice')}</p>
   <ul className="run-plan-notices">{notices.map((s,i)=><li key={i}>{s}</li>)}</ul>
   <ol className="run-plan-stages">
    {(music.mode==='cover'||inputNotes.length>0)&&stage('input',<>{music.mode==='cover'&&<>{row('studio.music.source',music.sourceAudio)}{row('studio.music.audioEncoder',music.audioEncoder)}<dd>{t('studio.music.coverHint')}</dd></>}{inputNotes.map((s,i)=><dd key={i}>{s}</dd>)}</>)}
    {stage('model',<>{row('studio.run.checkpoint',music.checkpoint)}{row('studio.run.textEncoder',t('studio.plan.checkpointParts'))}{row('studio.run.vae',t('studio.plan.checkpointParts'))}</>)}
    {stage('prompt',<>{row('studio.music.style',music.style||'∅')}{row('studio.music.lyrics',music.lyrics||'∅')}</>)}
    {music.planning!=='off'&&stage('abc',<>{row('studio.music.planning',t(`studio.music.${music.planning}`))}{music.mode==='t2m'&&settings(music.abc)}</>)}
    {stage('generate',<>{row('studio.music.maxDuration',music.maxDuration)}<dd>{t('studio.music.durationHint')}</dd>{row('studio.run.seed',music.seed)}{row('studio.run.steps',music.steps)}{row('studio.run.cfg',music.cfg)}{row('studio.recipe.sampler',music.sampler)}{row('studio.recipe.scheduler',music.scheduler)}{settings(music.generation)}</>)}
    {music.audioEnhance&&stage('audioEnhance',<>{row('studio.audioEnhance.mode',t(`studio.audioEnhance.${music.audioEnhance.mode}`))}{row('studio.audioEnhance.profile',t(`studio.audioEnhance.${music.audioEnhance.profile}`))}{music.audioEnhance.mode==='stems'&&row('studio.audioEnhance.device',t(`studio.audioEnhance.device.${music.audioEnhance.device??'auto'}`))}<dd>{t('studio.audioEnhance.automatic')}</dd></>)}
    {stage('output',<>{row('studio.music.output','FLAC')}{row('settings.output',outputRoot)}{row('studio.run.opt.outputNaming',music.outputPrefix)}<dd>{t('studio.plan.outputPending')}</dd></>)}
   </ol>
   <details className="run-plan-intent"><summary>{t('studio.plan.intent')}</summary><pre data-plan-intent>{JSON.stringify(intent,null,2)}</pre></details>
  </section>;
 }
 const video=intent&&isVideoIntent(intent)?intent:null;
 const image=intent&&isImageIntent(intent)?intent:null;
 const videoSize=video?.latentUpscale?latentUpscaleSize(video.family,video,video.latentUpscale.scale):video;
 return <section className="feature-panel run-plan" aria-label={t('studio.tab.map')}>
  <div className="panel-heading"><h2>{t('studio.tab.map')}</h2><span>{t(`studio.run.pipe.${pipeline}`)}</span></div>
  <p>{t('studio.plan.notice')}</p>
  <ul className="run-plan-notices">{notices.map((s,i)=><li key={i}>{s}</li>)}</ul>
  {!intent?<p role="status">{unknown}</p>:<ol className="run-plan-stages">
   {(image?.inputImage||image?.imageReference||image?.nativeImageReference||video?.firstFrame||video?.lastFrame||video?.sourceVideo||video?.family==='minimax-h3'&&(video.h3.references.length||video.h3.media?.length)||video?.family==='ltx-2'&&video.ltx.keyframes?.length||inputNotes.length>0)?stage('input',<>
    {image?.inputImage&&row('studio.run.pipe.input',image.inputImage)}
    {image?.imageReference&&row('studio.run.dock.references',<ul>{image.imageReference.images.map((r,i)=><li key={i}>{r.image} · {r.strength}</li>)}</ul>)}
    {image?.nativeImageReference&&row('studio.run.dock.references',<ul>{image.nativeImageReference.images.map((name,i)=><li key={i}>{name}</li>)}</ul>)}
    {image?.inputResize&&row('studio.run.resolution.scaling',<>{size(image.inputResize)} · {image.inputResize.crop}{data(image.inputResize)}</>)}
    {image?.inputLayers&&row('studio.editor.layers',<ul>{image.inputLayers.map((l,i)=><li key={i}>{l.image} · {l.width} × {l.height} · ({l.x}, {l.y})</li>)}</ul>)}
    {video?.firstFrame&&row('studio.plan.firstFrame',video.firstFrame)}
    {video?.lastFrame&&row('studio.run.video.lastFrame',video.lastFrame)}
    {video?.sourceVideo&&row('studio.run.video.source',video.sourceVideo)}
    {video?.family==='ltx-2'&&video.ltx.keyframes&&row('studio.run.video.refs',<ul>{video.ltx.keyframes.map((r,i)=><li key={i}>{r.image} · {t('studio.plan.frames')}: {r.frame} · {t('studio.run.video.refStrength')}: {r.strength}</li>)}</ul>)}
    {video?.family==='minimax-h3'&&row('studio.run.video.refs',<ul>{video.h3.references.map((r,i)=><li key={`r${i}`}>{r.image} · {t('studio.plan.slot')}: {r.slot}</li>)}{video.h3.media?.map((r,i)=><li key={`m${i}`}>{r.file} · {r.type} · {t('studio.plan.slot')}: {r.slot}{r.mediaMode&&<> · {t(`studio.run.video.mode.${r.mediaMode}`)}</>}</li>)}</ul>)}
    {inputNotes.map((s,i)=><dd key={i}>{s}</dd>)}
   </>):null}
   {stage('model',<>
    {row('studio.run.checkpoint',image?.checkpoint??video?.model)}
    {image&&!image.diffusion&&<>{row('studio.run.textEncoder',t('studio.plan.checkpointParts'))}{row('studio.run.vae',t('studio.plan.checkpointParts'))}</>}
    {image?.imageReference&&row('studio.run.reference.vision',image.imageReference.visionModel)}
    {image?.diffusion&&<>{row('studio.run.textEncoder',image.diffusion.textEncoder)}{row('studio.run.vae',image.diffusion.vae)}</>}
    {video&&<>{row('studio.run.textEncoder',video.textEncoder)}{row('studio.run.vae',video.vae)}</>}
    {!!intent.loras?.length&&row('studio.run.loras',<ul>{intent.loras.map((l,i)=><li key={i}>{l.name} · {l.strength}</li>)}</ul>)}
    {intent.options&&optionRows(intent.options)}
   </>)}
   {stage('prompt',<>{row('studio.run.positive',intent.positivePrompt||'∅')}{row('studio.run.negative',intent.negativePrompt||'∅')}
    {image?.wildcard&&<>{row('wildcards.title',data(image.wildcard))}<dd>{t('studio.plan.wildcardPending')}</dd></>}
   </>)}
   {stage('generate',<>
    {row('studio.run.resolution.title',size(intent))}{row('studio.run.seed',intent.seed)}{row('studio.run.steps',intent.steps)}{row('studio.run.cfg',intent.cfg)}{row('studio.recipe.sampler',intent.sampler)}
    {row(image?.sigmas?'studio.run.opt.sigmas':'studio.recipe.scheduler',image?.sigmas?image.sigmas.join(', '):intent.scheduler)}
    {image?.denoise!==undefined&&row('studio.run.denoise',image.denoise)}
    {image?.h3Edit&&<>{row('studio.run.video.shiftVideo',image.h3Edit.shiftVideo)}{row('studio.run.video.shiftAudio',image.h3Edit.shiftAudio)}</>}
    {video&&<>{video.family==='minimax-h3'&&video.h3.continuity?.durationSeconds!==undefined?row('studio.run.cont.seconds',video.h3.continuity.durationSeconds):<>{row('studio.plan.frames',video.frames)}{row('studio.plan.duration',String((video.frames-1)/video.frameRate))}</>}{row('studio.run.video.fps',video.frameRate)}
     {video.chunks&&row('studio.run.opt.chunking',video.chunks)}
     {row('studio.plan.family',video.family)}
     {video.family==='ltx-2'&&<>{row('studio.run.video.passes',<ol>{Array.from({length:video.ltx.passes},(_,n)=><li key={n}>{t('studio.plan.pass',{n:String(n+1)})}{n===0?` · ${intent.steps} · ${intent.sampler}`:` · ${t('studio.plan.nativePass')}`}</li>)}</ol>)}{row('studio.run.video.textProjection',video.ltx.textProjection)}{video.ltx.spatialUpscaler!==undefined&&row('studio.run.video.upscaler',video.ltx.spatialUpscaler)}{video.ltx.sourceStrength!==undefined&&row('studio.run.video.sourceStrength',video.ltx.sourceStrength)}</>}
     {video.family==='wan'&&<>{row('studio.plan.lowModel',video.wan.lowModel)}{row('studio.run.video.shiftHigh',video.wan.shiftHigh)}{row('studio.run.video.shiftLow',video.wan.shiftLow)}{row('studio.run.video.refinerStep',video.wan.refinerStep)}</>}
     {video.family==='minimax-h3'&&<>{row('studio.plan.mode',video.h3.mode)}{row('studio.run.video.shiftVideo',video.h3.shiftVideo)}{row('studio.run.video.shiftAudio',video.h3.shiftAudio)}{(video.h3.references.length>0||!!video.h3.media?.length)&&row('studio.run.resolution.scaling',video.h3.inputScaling??'Fill and crop')}{video.h3.continuity&&row('studio.run.dock.continuity',data(video.h3.continuity))}</>}
</>}
   </>)}
   {image?.refine&&stage('refine',<>{row('studio.run.resolution.title',size(image.refine))}{row('studio.run.post.denoise',image.refine.denoise)}{row('studio.run.steps',image.refine.sigmas?image.refine.sigmas.length-1:intent.steps)}{row('studio.run.post.model',image.refine.upscaleModel||t('studio.run.post.lanczos'))}
    {image.refine.sigmas&&row('studio.run.opt.sigmas',image.refine.sigmas.join(', '))}{image.refine.tiled&&row('studio.run.post.tiled',`${image.refine.tiled.tileWidth} × ${image.refine.tiled.tileHeight} · ${t('studio.plan.overlap')}: ${image.refine.tiled.overlap} px`)}</>)}
   {video?.latentUpscale&&stage('latentUpscale',<>{row('studio.run.latent.model',video.latentUpscale.model)}{row('studio.run.upscale',video.latentUpscale.scale)}{row('studio.run.resolution.title',size(latentUpscaleSize(video.family,video,video.latentUpscale.scale)))}{video.family==='minimax-h3'&&<>{row('studio.run.latent.steps',video.latentUpscale.steps)}{row('studio.run.latent.denoise',video.latentUpscale.denoise)}{Object.entries(video.latentUpscale.advanced??{}).map(([k,v])=>row(`studio.run.latent.${k}`,typeof v==='boolean'?yesNo(v):v))}</>}<dd>{t('studio.run.latent.hint',{width:String(videoSize!.width),height:String(videoSize!.height)})}</dd></>)}
   {video&&video.family!=='wan'&&stage('audio',<>{row('studio.run.video.audioVae',video.family==='ltx-2'?video.ltx.audioVae:video.h3.audioVae)}<dd>{t('studio.plan.audioPending')}</dd></>)}
   {video?.videoPost&&stage('tve',<>{row('studio.tve.post.title',describeVideoPost(video.videoPost))}{row('studio.tve.fps',`${video.frameRate} → ${videoPostFrameRate(video.frameRate,video.videoPost)}`)}<dd>{t('studio.tve.post.baselineHint')}</dd></>)}
   {video?.audioEnhance&&stage('audioEnhance',<>{row('studio.audioEnhance.mode',t(`studio.audioEnhance.${video.audioEnhance.mode}`))}{row('studio.audioEnhance.profile',t(`studio.audioEnhance.${video.audioEnhance.profile}`))}{video.audioEnhance.mode==='stems'&&row('studio.audioEnhance.device',t(`studio.audioEnhance.device.${video.audioEnhance.device??'auto'}`))}<dd>{t('studio.audioEnhance.automatic')}</dd></>)}
   {stage('output',<>{row('studio.run.resolution.title',size(videoSize??image?.refine??intent))}{row('settings.output',outputRoot)}{row('studio.run.opt.outputNaming',intent.outputPrefix)}
    {video?<>{(['codec','container','bitDepth','quality','audioCodec','audioBitrate','saveFirstFrame','saveLastFrame'] as const).map(k=>row(`studio.run.opt.output.${k}`,typeof video.output[k]==='boolean'?yesNo(video.output[k] as boolean):video.output[k]==='Auto'?`${t('studio.run.opt.output.auto')} · ${unknown}`:video.output[k]))}</>:<>{row('studio.run.opt.output.imageFormat',image?.imageFormat??'png')}{row('studio.run.opt.output.imageCompression',image?.imageCompression??44)}</>}
    <dd>{t('studio.plan.outputPending')}</dd>
   </>)}
  </ol>}
  {intent&&<details className="run-plan-intent"><summary>{t('studio.plan.intent')}</summary><pre data-plan-intent>{JSON.stringify(intent,null,2)}</pre></details>}
 </section>;
}
