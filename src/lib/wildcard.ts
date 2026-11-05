/** Original node inputs; DEAS selects rules, the installed node resolves them. */
export { parseVariableLibrary, parsePins, prunePins, mergeVariableLibrary, flattenCategories, resolveVariables, scanVariables, variablePick, variableName, fnv1a, type VariableLibrary, type VariablePins, type VariableToken } from './promptVariables.ts';
import { parsePins, prunePins } from './promptVariables.ts';
export interface WildcardRules {
  style: 'Booru' | 'Natural Language';
  seed: number;
  reroll: number;
  token_budget: number;
  auto_reroll: boolean;
  selection_state: Record<string,{enabled:boolean;weight:number}>;
  /** Optional JSON `{variable: [choices]}` for `$name` prompt variables (node pack 0.5.39). */
  variables_json?: string;
  /** Optional JSON `{"name:index": "pinned choice"}`; pins win over the seed pick. */
  variable_selections?: string;
}
/** Wildcard library as the node serves it: category -> subjects with per-style entry lists. */
export type WildcardSubject={subject:string;booru_wildcards?:string[];nl_wildcards?:string[];booru_presets?:string[];nl_presets?:string[]};
export type WildcardLibrary=Record<string,WildcardSubject[]>;
const libraryKinds=['wildcards','presets'] as const;
export function parseWildcardLibrary(text:string):WildcardLibrary {
 const value=JSON.parse(text)?.categories;
 if(!value||typeof value!=='object'||Array.isArray(value)||!Object.entries(value).every(([category,subjects])=>category.length>0&&Array.isArray(subjects)&&subjects.every(s=>s&&typeof s.subject==='string'&&libraryKinds.every(k=>['booru','nl'].every(style=>s[`${style}_${k}`]===undefined||(Array.isArray(s[`${style}_${k}`])&&s[`${style}_${k}`].every((v:unknown)=>typeof v==='string')))))))throw new Error('Invalid wildcard library');
 return value as WildcardLibrary;
}
export const wildcardNode='DaSiWa_WildcardPresetPromptBuilder';
export const defaultWildcard:WildcardRules={style:'Booru',seed:0,reroll:0,token_budget:200,auto_reroll:false,selection_state:{},variables_json:'{}',variable_selections:'{}'};
export function isWildcardRules(v:unknown):v is WildcardRules {
  if(!v||typeof v!=='object'||Array.isArray(v))return false;
  const r=v as WildcardRules;
  const integer=(n:number,max:number,min=0)=>Number.isInteger(n)&&n>=min&&n<=max;
  const keys=Object.keys(r);
  return keys.length>=6&&keys.length<=8&&keys.every(k=>['style','seed','reroll','token_budget','auto_reroll','selection_state','variables_json','variable_selections'].includes(k))
    &&['Booru','Natural Language'].includes(r.style)&&integer(r.seed,0xffffffff)&&integer(r.reroll,0x7fffffff)
    &&integer(r.token_budget,10000,1)&&typeof r.auto_reroll==='boolean'&&!!r.selection_state&&typeof r.selection_state==='object'&&!Array.isArray(r.selection_state)
    &&Object.keys(r.selection_state).length<=4096&&Object.entries(r.selection_state).every(([k,s])=>k.length>0&&k.length<=512&&!k.includes('\0')&&!!s&&typeof s==='object'&&Object.keys(s).length===2&&typeof s.enabled==='boolean'&&typeof s.weight==='number'&&Number.isFinite(s.weight)&&Math.abs(s.weight)<=10)
    &&(r.variables_json===undefined||(typeof r.variables_json==='string'&&r.variables_json.length<=1000000))
    &&(r.variable_selections===undefined||(typeof r.variable_selections==='string'&&r.variable_selections.length<=256000));
}
