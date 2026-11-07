import {useI18n} from '../../i18n/I18nProvider';

export interface Sam3Settings {checkpoint:string;targets:string;denoise:number;threshold:number;enabled:boolean}
export const samTargets=(text:string)=>text.split(/[,;\n]/u).map(s=>s.trim()).filter(Boolean);
/** One shared draft: editor selection and post-generation refinement use the same SAM checkpoint. */
export function Sam3Controls({value,onChange,models,margin,onMargin,unavailable,refineUnavailable,status,busy,inpaint,onMask,onRefine,canRefine}:{value:Sam3Settings;onChange:(value:Sam3Settings)=>void;models:readonly string[];margin:number;onMargin:(value:number)=>void;unavailable?:string;refineUnavailable?:string;status?:string;busy:boolean;inpaint?:boolean;onMask?:()=>void;onRefine?:()=>void;canRefine?:boolean}){
 const {t}=useI18n();const label=(key:string)=>t(`studio.sam.${key}`);
 const num=(key:'denoise'|'threshold',min:number)=><label className="wb-option"><span>{label(key)}</span><input data-sam={key} type="number" min={min} max={1} step={0.05} value={value[key]} onChange={e=>{const n=Number(e.target.value);if(Number.isFinite(n)&&n>=min&&n<=1)onChange({...value,[key]:n});}}/></label>;
 return <fieldset className="wb-group wb-wide" data-sam-controls><legend>{label('title')}</legend><div className="wb-options">
  <label className="wb-option"><span>{label('model')}</span><select data-sam="model" value={value.checkpoint} onChange={e=>onChange({...value,checkpoint:e.target.value})}><option value="">{label('choose')}</option>{(value.checkpoint&&!models.includes(value.checkpoint)?[value.checkpoint,...models]:models).map(name=><option key={name} value={name}>{name}</option>)}</select></label>
  <label className="wb-option"><span>{label('targets')}</span><input data-sam="targets" type="text" maxLength={2048} value={value.targets} placeholder={label('targetsPlaceholder')} title={label('targetsHelp')} onChange={e=>onChange({...value,targets:e.target.value})}/></label>
  {num('threshold',0)}{num('denoise',0.05)}
  <label className="wb-option"><span>{t('studio.run.maskGrow')}</span><input data-sam="margin" type="number" min={0} max={512} step={1} value={margin} title={t('studio.run.maskGrowHelp')} onChange={e=>{const n=Number(e.target.value);if(Number.isInteger(n)&&n>=0&&n<=512)onMargin(n);}}/></label>
  <div className="wb-option"><label className="wb-option-row"><input data-sam="enabled" type="checkbox" checked={value.enabled} onChange={e=>onChange({...value,enabled:e.target.checked})}/>{label('enabled')}</label></div>
 </div>
 <small>{label('help')}</small>
 {unavailable&&<p role="status">{unavailable}</p>}
 {status&&<p role="status">{status}</p>}
 {inpaint&&<button type="button" className="wb-dock-action" data-sam="mask" disabled={busy||!!unavailable||!samTargets(value.targets).length} onClick={onMask}>{label('mask')}</button>}
 <button type="button" className="wb-dock-action" data-sam="refine" disabled={busy||!!refineUnavailable||!canRefine} title={label('refineHelp')} onClick={onRefine}>{label('refine')}</button>
 </fieldset>;
}
