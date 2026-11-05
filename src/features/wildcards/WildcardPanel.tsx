import { useState } from 'react';
import { useI18n } from '../../i18n/I18nProvider';
import { usePref, isString } from '../../lib/preferences';
import { mergeVariableLibrary, parsePins, parseVariableLibrary, resolveVariables, scanVariables, variableName, variablePick, type WildcardLibrary, type WildcardRules } from '../../lib/wildcard';
import './wildcards.css';

const kinds=['wildcards','presets'] as const;
type Library=WildcardLibrary;
/** Same rejection-sampled crypto picks as the node's Random Select. */
function randomIndex(limit:number){const range=0x100000000,ceiling=range-range%limit,n=new Uint32Array(1);do{crypto.getRandomValues(n);}while(n[0]>=ceiling);return n[0]%limit;}
export function WildcardPanel({library,loading,error,onReload,value,onChange,enabled,onEnabled,positive,negative,onInsert}:{library:Library|null;loading:boolean;error:string;onReload:()=>void;value:WildcardRules;onChange:(v:WildcardRules)=>void;enabled:boolean;onEnabled:(v:boolean)=>void,positive:string;negative:string;onInsert:(name:string)=>void}){
 const {t}=useI18n();const [query,setQuery]=useState('');
 const prefix=value.style==='Booru'?'booru':'nl';
 const entries=Object.entries(library??{}).flatMap(([category,subjects])=>subjects.flatMap(subject=>kinds.flatMap(kind=>{
  const text=subject[`${prefix}_${kind}`]??[];
  return text.length?[{category,subject:subject.subject,kind,text,key:`${category}/${subject.subject}/${kind}`}]:[];
 })));
 const used=entries.filter(e=>value.selection_state[e.key]?.enabled);
 const isNegativeCategory=(category:string)=>category.toLowerCase().includes('negative prompt');
 const update=(patch:Partial<WildcardRules>)=>onChange({...value,...patch});
 const select=(key:string,patch:{enabled?:boolean;weight?:number})=>update({selection_state:{...value.selection_state,[key]:{...(value.selection_state[key]??{enabled:false,weight:1}),...patch}}});
 const randomSelect=()=>{const candidates=[...entries],state:WildcardRules['selection_state']={};const count=Math.min(candidates.length,1+randomIndex(10));
  for(let i=0;i<count;i++){const j=i+randomIndex(candidates.length-i);[candidates[i],candidates[j]]=[candidates[j],candidates[i]];const key=candidates[i].key;state[key]={enabled:true,weight:value.selection_state[key]?.weight??1};}update({selection_state:state});
 };
 const [customVariables,setCustomVariables]=usePref('studio.run.customVariables','{}',isString);
 const customLibrary=parseCustomLibrary(customVariables);
 const variableCategories:Record<string,string[]>={};for(const e of entries){const label=variableName(e.category);const target=variableCategories[label]??(variableCategories[label]=[]);for(const x of e.text){const trimmed=x.trim();if(trimmed&&!target.includes(trimmed))target.push(trimmed);}}
 const variableNames=new Set([...scanVariables(positive,{}).map(x=>x.name),...scanVariables(negative,{}).map(x=>x.name),...Object.keys(customLibrary.library)]);
 const variableLibrary=mergeVariableLibrary(variableCategories,customLibrary.library,variableNames);
 const pins=parsePins(value.variable_selections);
 const varTokens=(text:string)=>scanVariables(text,variableLibrary);
 const positiveTokens=varTokens(positive),negativeTokens=varTokens(negative);
 const setPin=(key:string,choice:string)=>{const next={...pins};if(choice)next[key]=choice;else delete next[key];update({variable_selections:JSON.stringify(next)});};
 return <section className="wildcards" aria-label={t('wildcards.title')}>
  <div className="wildcard-toolbar">
   <label className="wildcard-check"><input type="checkbox" checked={enabled} onChange={e=>onEnabled(e.target.checked)}/>{t('wildcards.enabled')}</label>
   <label>{t('wildcards.style')}<select value={value.style} onChange={e=>update({style:e.target.value as WildcardRules['style']})}><option value="Booru">{t('wildcards.booru')}</option><option value="Natural Language">{t('wildcards.nl')}</option></select></label>
   <label>{t('wildcards.seed')}<input type="number" min={0} max={0xffffffff} step={1} value={value.seed} onChange={e=>{const n=Number(e.target.value);if(Number.isInteger(n)&&n>=0&&n<=0xffffffff)update({seed:n});}}/></label>
   <label>{t('wildcards.budget')}<input type="number" min={1} max={10000} step={1} value={value.token_budget} onChange={e=>{const n=Number(e.target.value);if(Number.isInteger(n)&&n>=1&&n<=10000)update({token_budget:n});}}/></label>
   <label className="wildcard-check"><input type="checkbox" checked={value.auto_reroll} onChange={e=>update({auto_reroll:e.target.checked})}/>{t('wildcards.auto')}</label>
   <button type="button" onClick={()=>update({reroll:randomIndex(0x80000000)})}>{t('wildcards.picks')}</button>
   <button type="button" disabled={!entries.length} onClick={randomSelect}>{t('wildcards.random')}</button>
   <button type="button" disabled={!Object.keys(value.selection_state).length} onClick={()=>update({selection_state:{}})}>{t('wildcards.clear')}</button>
   <button type="button" onClick={onReload}>{t('wildcards.reload')}</button>
   <input type="search" value={query} aria-label={t('wildcards.search')} placeholder={t('wildcards.search')} onChange={e=>setQuery(e.target.value)}/>
  </div>
  <p className="wildcard-help">{t('wildcards.help')} <strong role="status">{t('wildcards.selected',{count:used.length})}</strong></p>
  {error?<p className="error-message" role="alert">{error}</p>:loading||!library?<p role="status">{t('wildcards.loading')}</p>:entries.length===0?<p>{t('wildcards.empty')}</p>:<div className="wildcard-body"><div className="wildcard-categories">
   {Object.keys(library).map(category=>{const list=entries.filter(e=>e.category===category&&`${category} ${e.subject} ${e.text.join(' ')}`.toLowerCase().includes(query.trim().toLowerCase()));if(!list.length)return null;const count=entries.filter(e=>e.category===category&&value.selection_state[e.key]?.enabled).length;const variableLabel=variableName(category);
    return <details key={category} className={count?'is-selected':''} open={query.trim()?true:undefined}>
     <summary><span>{category}</span><small>{t(isNegativeCategory(category)?'wildcards.negative':'wildcards.positive')}</small>{count>0&&<strong>{t('wildcards.selected',{count})}</strong>}<button type="button" className="wildcard-insert" data-wildcard-insert={variableLabel} aria-label={t('wildcards.insert',{name:`$${variableLabel}`})} title={t('wildcards.insert',{name:`$${variableLabel}`})} onClick={e=>{e.preventDefault();e.stopPropagation();onInsert(variableLabel);}}>+</button></summary>
     {kinds.map(kind=>{const items=list.filter(e=>e.kind===kind);return items.length>0&&<div key={kind} className="wildcard-section"><h4>{t(`wildcards.${kind}`)}</h4>{items.map(e=>{const state=value.selection_state[e.key];const label=`${e.subject} · ${t(`wildcards.${kind}`)}`;return <div key={e.key} className="wildcard-row">
      <label className="wildcard-check"><input type="checkbox" checked={state?.enabled??false} aria-label={label} onChange={event=>select(e.key,{enabled:event.target.checked})}/>{e.subject}</label>
      <input type="number" min={-10} max={10} step={0.05} value={state?.weight??1} aria-label={t('wildcards.weight',{name:label})} onChange={event=>{const n=Number(event.target.value);if(Number.isFinite(n)&&Math.abs(n)<=10)select(e.key,{weight:Math.round(n*100)/100});}}/>
      <small>{e.text.join(', ')}</small>
     </div>;})}</div>;})}
    </details>;
   })}
  </div>
  <aside className="wildcard-preview" aria-label={t('wildcards.preview')}><h3>{t('wildcards.preview')}</h3>
   <div className="wildcard-vars" data-wildcard-vars>
    <h4>{t('wildcards.varsTitle')}</h4>
    <p className="wildcard-vars-help">{t('wildcards.varsHelp')}</p>
    <label className="wildcard-vars-custom"><span>{t('wildcards.custom')}</span>
     <textarea value={customVariables} onChange={e=>setCustomVariables(e.target.value)} rows={4} spellCheck={false} aria-label={t('wildcards.custom')}/>
     {customLibrary.error&&<small className="error-message">{t('wildcards.customInvalid',{message:customLibrary.error})}</small>}
    </label>
    {positiveTokens.length+negativeTokens.length===0&&<p className="wildcard-vars-empty">{t('wildcards.varsNone')}</p>}
    {(['positive','negative'] as const).map(side=>varTokens(side==='positive'?positive:negative).map(tok=>
     <div className="wildcard-var-row" data-wildcard-var={`${side}:${tok.key}`} key={`${side}:${tok.key}`}>
      <span className="wildcard-var-name">{`$${tok.name}`}<small>{t(`wildcards.${side}`)}</small></span>
      {tok.known
       ?<select aria-label={t('wildcards.varPick',{key:tok.key})} value={pins[tok.key]??''} onChange={e=>setPin(tok.key,e.target.value)}><option value="">{t('wildcards.varRandom')}</option>{(variableLibrary[tok.name]??[]).map(o=><option key={o} value={o}>{o}</option>)}</select>
       :<span className="wildcard-var-unknown">{t('wildcards.varUnknown',{name:tok.name})}</span>}
      {tok.known&&<code className="wildcard-var-pick">{variablePick(tok.name,tok.key,variableLibrary,pins,value.seed)}</code>}
     </div>))}
    {positiveTokens.length+negativeTokens.length>0&&<div className="wildcard-vars-resolved">
     {positiveTokens.length>0&&<p><strong>{t('wildcards.positive')}</strong> <span className="wildcard-vars-text">{resolveVariables(positive,variableLibrary,pins,value.seed)}</span></p>}
     {negativeTokens.length>0&&<p><strong>{t('wildcards.negative')}</strong> <span className="wildcard-vars-text">{resolveVariables(negative,variableLibrary,pins,value.seed)}</span></p>}
    </div>}
   </div>
   {[false,true].map(neg=>{const side=used.filter(e=>isNegativeCategory(e.category)===neg);return side.length>0&&<div key={String(neg)}><h4>{t(neg?'wildcards.negative':'wildcards.positive')}</h4><ul>
    {side.map(e=>{const weight=value.selection_state[e.key].weight;return <li key={e.key}><strong>{e.subject} · {t(`wildcards.${e.kind}`)}{weight!==1&&` ×${weight}`}</strong> {e.text.join(', ')}</li>;})}
   </ul></div>;})}
  </aside></div>}
 </section>;
}
/** Parses the custom `$name` library; returns {} plus a user-readable error when invalid. */
function parseCustomLibrary(text:string):{library:Record<string,string[]>;error:string}{
 try{return {library:parseVariableLibrary(text),error:''};}catch(e){return {library:{},error:String(e)};}
}
