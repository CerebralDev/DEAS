import {useI18n} from '../../i18n/I18nProvider';
import {aceDurationStep,normalizeAceDuration,aceKeyScales,aceLanguages,type AceStepSettings} from '../../lib/musicIntent';

/** ACE conditioning controls; diffusion sampling remains in the shared Workbench controls. */
export function AceMusicSettings({family,version,value,onChange,duration,onDuration}:{family:string;version:'1.0'|'1.5';value:AceStepSettings;onChange:(v:AceStepSettings)=>void;duration:number;onDuration:(v:number)=>void}){
 const {t}=useI18n();
 const number=(key:keyof AceStepSettings,min:number,max:number,step:number)=><label className="wb-field" key={key} title={t(`studio.music.ace.${key}Hint`)}><span>{t(`studio.music.ace.${key}`)}</span><input data-ace-setting={key} type="number" value={value[key] as number} min={min} max={max} step={step} onChange={e=>{const n=Number(e.target.value);if(e.target.value!==''&&Number.isFinite(n)&&n>=min&&n<=max&&(step!==1||Number.isInteger(n)))onChange({...value,[key]:n});}}/></label>;
 const select=(key:'timeSignature'|'language'|'keyScale',options:readonly string[])=><label className="wb-field" key={key} title={t(`studio.music.ace.${key}Hint`)}><span>{t(`studio.music.ace.${key}`)}</span><select data-ace-setting={key} value={value[key]} onChange={e=>onChange({...value,[key]:e.target.value})}>{options.map(v=><option key={v} value={v}>{v}</option>)}</select></label>;
 const durationStep=aceDurationStep(family);
 return <>
  <label className="wb-field" title={t('studio.music.ace.durationHint')}><span>{t('studio.music.maxDuration')}</span><input data-music-duration data-ace-duration type="number" value={duration} min={1} max={1000} step={durationStep} onChange={e=>{const n=Number(e.target.value);if(e.target.value!==''&&Number.isFinite(n)&&n>=1&&n<=1000)onDuration(normalizeAceDuration(n,family));}}/></label>
  {number('shift',0,100,0.01)}
  {version==='1.0'?number('lyricsStrength',0,10,0.01):<>
   {number('bpm',10,300,1)}{select('timeSignature',['2','3','4','6'])}{select('language',aceLanguages)}{select('keyScale',aceKeyScales)}
   <label className="wb-field wb-wide" title={t('studio.music.ace.generateAudioCodesHint')}><span><input data-ace-setting="generateAudioCodes" type="checkbox" checked={value.generateAudioCodes} onChange={e=>onChange({...value,generateAudioCodes:e.target.checked})}/>{t('studio.music.ace.generateAudioCodes')}</span></label>
   {number('lmCfg',0,100,0.1)}{number('temperature',0,2,0.01)}{number('topP',0,1,0.01)}{number('topK',0,100,1)}{number('minP',0,1,0.001)}
  </>}
 </>;
}
