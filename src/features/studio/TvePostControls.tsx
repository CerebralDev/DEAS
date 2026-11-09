import { useI18n } from '../../i18n/I18nProvider';
import type { RecipeAllowlist } from '../../lib/execution';
import { frameGenModes, rtxQualities, rtxUpscaleModes, type LoopAudio, type RtxQuality, type VideoPost } from '../../lib/videoIntent';

/** Remembered TVE post-processing controls; `toVideoPost` keeps only enabled stages so untouched intents stay byte-identical. */
export interface TvePref { upscaleModel:string; denoise:boolean; denoiseQuality:RtxQuality; deblur:boolean; deblurQuality:RtxQuality; rtxUpscale:boolean; rtxMode:typeof rtxUpscaleModes[number]; rtxQuality:RtxQuality; rtxScale:number; useMmap:boolean; chunkFrames:number; frameGen:boolean; engine:'rtx'|'rife'; multiplier:number; mode:typeof frameGenModes[number]; rifeModel:string; loop:boolean; loopBackend:'rife'|'rtx'; loopModel:string; loopAudio:LoopAudio }
export const defaultTve:TvePref={upscaleModel:'',denoise:false,denoiseQuality:'Ultra',deblur:false,deblurQuality:'Ultra',rtxUpscale:false,rtxMode:'VSR',rtxQuality:'Ultra',rtxScale:2,useMmap:false,chunkFrames:16,frameGen:false,engine:'rtx',multiplier:2,mode:'HIGH',rifeModel:'',loop:false,loopBackend:'rife',loopModel:'',loopAudio:'crossfade'};
/** Absent fields (a preference saved before the RTX memory controls) keep their defaults. */
export const isTvePref=(v:unknown):v is TvePref=>{const o=v as Record<string,unknown>;return !!o&&typeof o==='object'&&['rtx','rife'].includes(o.engine as string)&&Object.entries(defaultTve).every(([k,d])=>o[k]===undefined||typeof o[k]===typeof d);};
export function toVideoPost(p:TvePref):VideoPost|undefined{
 const rtx={...(p.denoise?{denoise:p.denoiseQuality}:{}),...(p.deblur?{deblur:p.deblurQuality}:{}),...(p.rtxUpscale?{upscale:{mode:p.rtxMode,quality:p.rtxQuality,scale:p.rtxScale}}:{}),
  // The node's own default is 16; only a changed value reaches the intent.
  ...(p.rtxUpscale&&p.chunkFrames!==undefined&&p.chunkFrames!==16?{chunkFrames:p.chunkFrames}:{})};
 const post:VideoPost={...(p.loop?{loop:p.loopBackend==='rife'?{backend:'rife' as const,model:p.loopModel,audio:p.loopAudio}:{backend:'rtx' as const,audio:p.loopAudio}}:{}),
  ...(p.upscaleModel?{upscaleModel:p.upscaleModel}:{}),...(Object.keys(rtx).length?{rtx}:{}),
  // use_mmap is a node option; without an RTX stage it would be a key with no effect.
  ...(p.useMmap&&(Object.keys(rtx).length||(p.frameGen&&p.engine==='rtx'))?{useMmap:true}:{}),
  ...(p.frameGen?{frameGeneration:p.engine==='rife'?{engine:'rife' as const,multiplier:p.multiplier,model:p.rifeModel}:{engine:'rtx' as const,multiplier:p.multiplier,mode:p.mode}}:{})};
 return Object.keys(post).length?post:undefined;
}
/** Controls from a job's immutable intent (history restore); absent = everything off. */
export function fromVideoPost(v:VideoPost|undefined):TvePref{
 if(!v)return defaultTve;
 const u=v.rtx?.upscale,f=v.frameGeneration,l=v.loop;
 return {...defaultTve,upscaleModel:v.upscaleModel??'',denoise:!!v.rtx?.denoise,denoiseQuality:v.rtx?.denoise??'Ultra',deblur:!!v.rtx?.deblur,deblurQuality:v.rtx?.deblur??'Ultra',
  rtxUpscale:!!u,rtxMode:u?.mode??'VSR',rtxQuality:u?.quality??'Ultra',rtxScale:u?.scale??2,useMmap:!!v.useMmap,chunkFrames:v.rtx?.chunkFrames??16,
  frameGen:!!f,engine:f?.engine??'rtx',multiplier:f?.multiplier??2,mode:f?.engine==='rtx'?f.mode:'HIGH',rifeModel:f?.engine==='rife'?f.model:'',
  loop:!!l,loopBackend:l?.backend??'rife',loopModel:l?.backend==='rife'?l.model:'',loopAudio:l?.audio??'crossfade'};
}
/** One-line summary of the enabled stages for Run Plan and metadata. */
export function describeVideoPost(v:VideoPost):string{
 return [v.loop&&`Seamless loop ${v.loop.backend==='rife'?v.loop.model:'RTX'} ${v.loop.audio==='beat'?'beat-aware':'crossfade'}`,v.upscaleModel,v.rtx?.denoise&&`Denoise ${v.rtx.denoise}`,v.rtx?.deblur&&`Deblur ${v.rtx.deblur}`,v.rtx?.upscale&&`RTX ${v.rtx.upscale.mode} ${v.rtx.upscale.quality} ×${v.rtx.upscale.scale}`,v.useMmap&&'RTX use_mmap',
  v.frameGeneration&&(v.frameGeneration.engine==='rife'?`RIFE ${v.frameGeneration.model} ×${v.frameGeneration.multiplier}`:`RTX FrameGen ${v.frameGeneration.mode} ×${v.frameGeneration.multiplier}`)].filter(Boolean).join(' → ');
}

/** The TVE post stages (seamless loop → model upscale → RTX refine → frame generation), each marked as pulled from TVE. */
export function TvePostControls({value,onChange,allow,onRead,disabled=false,fps,modelCard=true}:{value:TvePref;onChange:(next:TvePref)=>void;allow:RecipeAllowlist|null;onRead?:()=>void;disabled?:boolean;fps?:number;/** false = the Post dock renders the upscale-model card in its own group. */modelCard?:boolean}){
 const {t}=useI18n();
 // Defaults first: a preference saved before a stage existed lacks its fields.
 const set=(patch:Partial<TvePref>)=>onChange({...defaultTve,...value,...patch});
 const rtxNode=!allow||allow.nodes.includes('DaSiWa_RTX_UpscalerRefiner');
 const frameGenNode=!allow||allow.nodes.includes('DaSiWa_RTX_FrameGeneration');
 const rifeNode=!allow||allow.nodes.includes('FrameInterpolate');
 const loopNode=!allow||allow.nodes.includes('DaSiWa_SeamlessLoop');
 const modes=allow?.rtxUpscaleModes?.length?rtxUpscaleModes.filter(m=>allow.rtxUpscaleModes!.includes(m)):rtxUpscaleModes;
 const badge=<span className="tve-badge" title={t('studio.tve.post.badgeHint')}>{t('studio.tab.tve')}</span>;
 const quality=(k:'denoiseQuality'|'deblurQuality'|'rtxQuality')=><select data-tve-post={k} disabled={disabled} value={value[k]} onChange={e=>set({[k]:e.target.value as RtxQuality})}>{rtxQualities.map(q=><option key={q} value={q}>{q}</option>)}</select>;
 const rifeModels=allow?.frameInterpolationModels??[];
 const rifeSelect=(k:'rifeModel'|'loopModel')=><select data-tve-post={k} aria-label={t('studio.tve.post.engineRife')} disabled={disabled} value={value[k]} onFocus={onRead} onPointerDown={onRead} onChange={e=>set({[k]:e.target.value})}>{!rifeModels.includes(value[k])&&<option value={value[k]}>{value[k]||t('studio.run.latent.choose')}</option>}{rifeModels.filter(m=>k==='rifeModel'||m.toLowerCase().endsWith('.safetensors')).map(m=><option key={m} value={m}>{m}</option>)}</select>;
 return <div className="tve-post" data-tve-post>
  <div className="wb-option" data-tve-post="loop"><span><label className="wb-option-row"><input type="checkbox" data-tve-post="loopEnabled" disabled={disabled||!loopNode} checked={!!value.loop} onChange={e=>set({loop:e.target.checked})}/>{t('studio.tve.post.loop')}</label>{badge}</span>
   {!loopNode&&<small>{t('studio.tve.post.loopMissing')}</small>}
   {value.loop&&<>
    <div className="wb-option-row">
     <select data-tve-post="loopBackend" aria-label={t('studio.tve.post.loopBackend')} disabled={disabled} value={value.loopBackend} onChange={e=>set({loopBackend:e.target.value as TvePref['loopBackend']})}><option value="rife">{t('studio.tve.post.engineRife')}</option><option value="rtx" disabled={!frameGenNode}>{t('studio.tve.post.engineRtx')}</option></select>
     {value.loopBackend==='rife'&&rifeSelect('loopModel')}
    </div>
    <label className="wb-option-row">{t('studio.tve.post.loopAudio')}<select data-tve-post="loopAudio" disabled={disabled} value={value.loopAudio} onChange={e=>set({loopAudio:e.target.value as LoopAudio})}><option value="crossfade">{t('studio.tve.post.loopCrossfade')}</option><option value="beat">{t('studio.tve.post.loopBeat')}</option></select></label>
    <small>{t('studio.tve.post.loopAudioHint')}</small>
   </>}
   <small>{t('studio.tve.post.loopHint')}</small>
  </div>
  {modelCard&&<label className="wb-option"><span>{t('studio.tve.post.model')}{badge}</span><select data-tve-post="upscaleModel" disabled={disabled} value={value.upscaleModel} onFocus={onRead} onPointerDown={onRead} onChange={e=>set({upscaleModel:e.target.value})}><option value="">{t('studio.tve.post.off')}</option>{(allow?.upscaleModels??[]).map(m=><option key={m} value={m}>{m}</option>)}{value.upscaleModel&&!allow?.upscaleModels?.includes(value.upscaleModel)&&<option value={value.upscaleModel}>{value.upscaleModel}</option>}</select></label>}
  <div className="wb-option" data-tve-post="rtx"><span>{t('studio.tve.post.rtx')}{badge}</span>
   {!rtxNode&&<small>{t('studio.tve.post.rtxMissing')}</small>}
   <label className="wb-option-row"><input type="checkbox" data-tve-post="denoise" disabled={disabled||!rtxNode} checked={value.denoise} onChange={e=>set({denoise:e.target.checked})}/>{t('studio.tve.post.denoise')}{value.denoise&&quality('denoiseQuality')}</label>
   <label className="wb-option-row"><input type="checkbox" data-tve-post="deblur" disabled={disabled||!rtxNode} checked={value.deblur} onChange={e=>set({deblur:e.target.checked})}/>{t('studio.tve.post.deblur')}{value.deblur&&quality('deblurQuality')}</label>
   <label className="wb-option-row"><input type="checkbox" data-tve-post="rtxUpscale" disabled={disabled||!rtxNode} checked={value.rtxUpscale} onChange={e=>set({rtxUpscale:e.target.checked})}/>{t('studio.tve.post.rtxUpscale')}</label>
   {value.rtxUpscale&&<div className="wb-option-row">
    <select data-tve-post="rtxMode" disabled={disabled} value={value.rtxMode} onChange={e=>set({rtxMode:e.target.value as TvePref['rtxMode']})}>{(modes.includes(value.rtxMode)?modes:[value.rtxMode,...modes]).map(m=><option key={m} value={m}>{m}</option>)}</select>
    {!value.rtxMode.startsWith('Streaming')&&quality('rtxQuality')}
    <input type="number" data-tve-post="rtxScale" aria-label={t('studio.run.upscale')} disabled={disabled} min={1} max={4} step={0.05} value={value.rtxScale} onChange={e=>{const n=Number(e.target.value);if(Number.isFinite(n))set({rtxScale:Math.min(4,Math.max(1,n))});}}/>
   </div>}
  </div>
  {(value.denoise||value.deblur||value.rtxUpscale||(value.frameGen&&value.engine==='rtx'))&&<div className="wb-option" data-tve-post="rtxMemory"><span>{t('studio.tve.post.rtxMemory')}{badge}</span>
   <label className="wb-option-row"><input type="checkbox" data-tve-post="useMmap" disabled={disabled} checked={!!value.useMmap} onChange={e=>set({useMmap:e.target.checked})}/>{t('studio.tve.post.useMmap')}</label>
   {(value.denoise||value.deblur||value.rtxUpscale)&&<label className="wb-option-row">{t('studio.tve.post.chunkFrames')}<input type="number" data-tve-post="chunkFrames" disabled={disabled} min={1} max={1024} step={1} value={value.chunkFrames??16} onChange={e=>{const n=Number(e.target.value);if(Number.isInteger(n))set({chunkFrames:Math.min(1024,Math.max(1,n))});}}/></label>}
   <small>{t('studio.tve.post.rtxMemoryHint')}</small>
  </div>}
  <div className="wb-option" data-tve-post="frameGen"><span><label className="wb-option-row"><input type="checkbox" data-tve-post="frameGenEnabled" disabled={disabled||(!frameGenNode&&!rifeNode)} checked={value.frameGen} onChange={e=>set({frameGen:e.target.checked})}/>{t('studio.tve.post.frameGen')}</label>{badge}</span>
   {value.frameGen&&<>
    <div className="wb-option-row">
     <select data-tve-post="engine" disabled={disabled} value={value.engine} onChange={e=>set({engine:e.target.value as TvePref['engine']})}><option value="rtx" disabled={!frameGenNode}>{t('studio.tve.post.engineRtx')}</option><option value="rife" disabled={!rifeNode}>{t('studio.tve.post.engineRife')}</option></select>
     <input type="number" data-tve-post="multiplier" aria-label={t('studio.tve.post.multiplier')} disabled={disabled} min={2} max={16} step={1} value={value.multiplier} onChange={e=>{const n=Number(e.target.value);if(Number.isInteger(n))set({multiplier:Math.min(16,Math.max(2,n))});}}/>
     {value.engine==='rtx'?<select data-tve-post="mode" disabled={disabled} value={value.mode} onChange={e=>set({mode:e.target.value as TvePref['mode']})}>{frameGenModes.map(m=><option key={m} value={m}>{m}</option>)}</select>
     :rifeSelect('rifeModel')}
    </div>
    {fps!==undefined&&<small>{t('studio.tve.post.fpsHint',{from:String(fps),to:String(fps*value.multiplier)})}</small>}
   </>}
  </div>
 </div>;
}
