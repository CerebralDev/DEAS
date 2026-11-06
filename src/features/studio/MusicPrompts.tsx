import {useState,type ReactNode} from 'react';
import {useI18n} from '../../i18n/I18nProvider';
import {ForgeButton} from '../forge/ForgeButton';
import {PromptTextarea} from './PromptTextarea';
import {PromptOverlay} from './PromptOverlay';

export type MusicField='style'|'lyrics';
/** Exact DaSiWa Prompt Writer dialects; neither is a native image Forge target, so the hammer takes the writer route. */
export const musicDialects:Record<MusicField,string>={style:'YuE2 Song Style',lyrics:'YuE2 Lyrics'};
export const MUSIC_TEXT_LIMIT=16384;

/** Two independent YuE2 editors (Style, Lyrics) in the `.wb-prompts` slot: no negative prompt, no image references. */
export function MusicPrompts({endpoint,root,style,lyrics,onStyle,onLyrics,actions,writerEnabled=true}:{
 endpoint:string;root?:string;style:string;lyrics:string;writerEnabled?:boolean;
 onStyle:(v:string)=>void;onLyrics:(v:string)=>void;
 /** Copy/clear buttons shared with the image prompts (Workbench owns clipboard feedback). */
 actions:(field:MusicField,value:string,set:(v:string)=>void)=>ReactNode;
}){
 const {t}=useI18n();const [expanded,setExpanded]=useState<MusicField|null>(null);
 const fields=[{id:'style' as const,value:style,set:onStyle},{id:'lyrics' as const,value:lyrics,set:onLyrics}];
 const writer=(f:typeof fields[number])=>writerEnabled&&<ForgeButton key={f.id} endpoint={endpoint} root={root} dialect={musicDialects[f.id]} value={f.value} onApply={f.set}/>;
 const selected=fields.find(f=>f.id===expanded);
 return <>
  {fields.map(f=>{const title=t(`studio.music.${f.id}`);return <div className="wb-prompt" key={f.id} data-music-field={f.id}>
   <div className="wb-prompt-label"><label htmlFor={`run-music-${f.id}`}>{title}</label><small data-prompt-count={f.id}>{f.value.length} / {MUSIC_TEXT_LIMIT}</small></div>
   <PromptTextarea root={root} target="generic" id={`run-music-${f.id}`} data-music-prompt={f.id} value={f.value} maxLength={MUSIC_TEXT_LIMIT} placeholder={t(`studio.music.${f.id}Hint`)} onChange={e=>f.set(e.target.value)}/>
   <button type="button" className="wb-prompt-expand" data-prompt-expand={f.id} aria-label={t('studio.run.prompt.expand',{name:title})} title={t('studio.run.prompt.expand',{name:title})} aria-pressed={expanded===f.id} onClick={()=>setExpanded(expanded===f.id?null:f.id)}>⤢</button>
   {actions(f.id,f.value,f.set)}{writer(f)}
  </div>;})}
  {selected&&<PromptOverlay root={root} target="generic" key={selected.id} title={t(`studio.music.${selected.id}`)} value={selected.value} onChange={selected.set} onClose={()=>setExpanded(null)} placeholder={t(`studio.music.${selected.id}Hint`)} maxLength={MUSIC_TEXT_LIMIT} closeLabel={t('studio.run.prompt.close')}>{writer(selected)}</PromptOverlay>}
 </>;
}
