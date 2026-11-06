import type { LocalModel } from '../../lib/models';
import { useI18n } from '../../i18n/I18nProvider';

export type BeginnerLora={name:string;strength:number};
export type BeginnerLoraDraft={model:string;loras:BeginnerLora[]};
export function isBeginnerLoraDraft(value:unknown):value is BeginnerLoraDraft|null {
 if(value===null)return true;
 if(!value||typeof value!=='object')return false;
 const v=value as BeginnerLoraDraft;
 return typeof v.model==='string'&&v.model.length<4096&&Array.isArray(v.loras)&&v.loras.length<=32&&v.loras.every(l=>l&&typeof l.name==='string'&&l.name.length<4096&&Number.isFinite(l.strength)&&Math.abs(l.strength)<=10);
}
export function BeginnerLoras({models,selected,onChange,onRefresh,disabled=false,reason}:{models:LocalModel[];selected:BeginnerLora[];onChange:(next:BeginnerLora[])=>void;onRefresh:()=>void;disabled?:boolean;reason?:string}){
 const {t}=useI18n();
 return <details className="beginner-loras" data-beginner-loras>
  <summary>{t('beginner.loras.title')}<small>{selected.length||'+'}</small></summary>
  <div className="beginner-loras-content">
   <p>{reason||t('beginner.loras.hint')}</p>
   <label><span className="sr-only">{t('beginner.loras.add')}</span><select aria-label={t('beginner.loras.add')} value="" disabled={disabled||models.length===0||selected.length>=32} onChange={e=>{const name=e.target.value;if(name&&selected.length<32&&!selected.some(l=>l.name===name))onChange([...selected,{name,strength:1}]);}}>
    <option value="">{t('beginner.loras.add')}</option>
    {models.filter(m=>!selected.some(l=>l.name===m.name)).map(m=><option key={m.path||m.name} value={m.name}>{m.name.replace(/\.safetensors$/i,'')}</option>)}
   </select></label>
   {!models.length&&!reason&&<p>{t('beginner.loras.empty')}</p>}
   <ul>{selected.map(l=><li key={l.name}><span title={l.name}>{l.name.replace(/\.safetensors$/i,'')}</span><label><span>{t('beginner.loras.strength')}</span><input type="number" aria-label={t('studio.run.strength',{name:l.name})} value={l.strength} min={-10} max={10} step={0.05} disabled={disabled} onChange={e=>{const value=Number(e.target.value);if(e.target.value!==''&&Number.isFinite(value)&&Math.abs(value)<=10)onChange(selected.map(x=>x.name===l.name?{...x,strength:value}:x));}}/></label><button type="button" disabled={disabled} aria-label={t('beginner.loras.remove',{name:l.name})} onClick={()=>onChange(selected.filter(x=>x.name!==l.name))}>×</button></li>)}</ul>
   <button type="button" className="beginner-loras-refresh" disabled={disabled} onClick={onRefresh}>{t('beginner.refresh')}</button>
  </div>
 </details>;
}
