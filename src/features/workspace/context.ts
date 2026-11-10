import { captureStudioSettings, isString, readPref } from '../../lib/preferences';
export interface WorkspaceSnapshot {version:1;area:string;capturedAt:number;values:Record<string,string|number|boolean|null>}
import { readSelectedContext } from '../../lib/workspaceSelection';
export { selectContext, readSelectedContext } from '../../lib/workspaceSelection';
export function cleanContextText(text:string){return text.split('\n').map(line=>line.replace(/(?:[A-Za-z]:[\\/]|\\\\|(?:^|[\s\"'(])\/)[^\r\n]*/g,'[redacted]')).join('\n').replace(/(?:[A-Za-z]:[\\/]|\\\\|\/(?:home|Users|mnt|tmp)\/)[^\s"<>]*/g,'[redacted]').replace(/\bsk-[\w-]+\b/g,'[redacted]').replace(/(?:api[_-]?key|authorization|token)\s*[:=][^\r\n]*/gi,'[redacted]').slice(0,6000);}
const basename=(s:string)=>s.split(/[\\/]/).at(-1)??'';
/** Read live authoring state, never screen text or credential preferences. */
export function captureWorkspaceContext(area:string):WorkspaceSnapshot {
 const values:WorkspaceSnapshot['values']={};
 if(area==='Studio'||area==='Forge'){
  const current=captureStudioSettings();
  for(const key of ['positive','negative','pipeline','checkpoint','steps','cfg','sampler','scheduler','width','height','seed']){
   const value=current[`studio.run.${key}`];
   if(typeof value==='string')values[key]=cleanContextText(key==='checkpoint'?basename(value):value);
   else if(typeof value==='number'&&Number.isFinite(value))values[key]=value;
  }
  const loras=current['studio.run.loras'];
  if(Array.isArray(loras))values.loras=cleanContextText(loras.slice(0,10).map(l=>l&&typeof l==='object'?`${basename(String(l.name??''))} × ${Number(l.strength??l.model??0)}`:'').join('; '));
  if(area==='Forge')values.brief=cleanContextText(readPref('forge.brief','',isString));
 }
 for(const [key,value] of Object.entries(readSelectedContext(area)))values[key]=cleanContextText(value);
 return {version:1,area,capturedAt:Date.now(),values};
}
export function suggestionFromText(text:string):Record<string,string|number>|null {
 const match=/```deas-suggestion\s*\n([\s\S]*?)```/.exec(text);
 if(!match||match[1].length>32000)return null;
 try{const v=JSON.parse(match[1]);if(!v||Array.isArray(v)||typeof v!=='object'||Object.keys(v).some(k=>!['positive','negative','steps','cfg'].includes(k)))return null;return v;}catch{return null;}
}

export function settingsFingerprint(value:unknown):string {const canonical=(v:unknown):unknown=>Array.isArray(v)?v.map(canonical):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b)).map(([k,x])=>[k,canonical(x)])):v;return JSON.stringify(canonical(value));}
