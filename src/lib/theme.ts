import { readPref, usePref, writePref } from './preferences';
import {useEffect,useSyncExternalStore} from 'react';
import {invoke,isTauri} from '@tauri-apps/api/core';

/**
 * Colour schemes, one per OC (owner, 23 Oct 2026). Each is a `data-theme` block in src/themes.css over the colour
 * tokens in src/styles.css; Default is DEAS's own look and sets no attribute. The swatches are the scheme's background
 * and accent, for the picker only.
 */
export const SCHEMES = [
  { id: 'default', name: 'Default', swatch: ['#171b1e', '#7c9cac'] },
  { id: 'liiti-pink', name: 'Liiti Pink', swatch: ['#0f0c10', '#f29cc6'] },
  { id: 'voud', name: 'VOID', swatch: ['#07070a', '#9d8cff'] }, // Keep the persisted ID for existing selections.
  { id: 'alluring-abyss', name: 'Alluring Abyss', swatch: ['#14090c', '#df6678'] },
  { id: 'slimy-teal', name: 'Slimy Teal', swatch: ['#081414', '#2cc7c0'] },
  { id: 'clawy-grey', name: 'Clawy Grey', swatch: ['#1b1c1e', '#b9bcc2'] },
  { id: 'android-white', name: 'Android White', swatch: ['#eef1f5', '#0a84ff'] },
  { id: 'dragons-hoard', name: "Dragon's Hoard", swatch: ['#16110b', '#e3b341'] },
] as const;
export type SchemeId = string;
const KEY = 'appearance.scheme';
const valid=(v:unknown):v is string=>typeof v==='string'&&/^[a-z0-9-]{1,64}$/.test(v);
export interface ThemePack {id:string;name:string;author:string;base:string;colors:Record<string,string>;assets:Record<string,string>;presentation:'french'|'paris'|null;effects:{mouse:boolean;wine:boolean}}
interface ThemeCatalog {directory:string;packs:ThemePack[];warnings:string[]}
let state:ThemeCatalog&{root:string;loading:boolean;error:boolean;active:ThemePack|null}={directory:'',packs:[],warnings:[],root:'',loading:false,error:false,active:null};
const listeners=new Set<()=>void>();let epoch=0;const applied=new Set<string>();
const subscribe=(listener:()=>void)=>{listeners.add(listener);return()=>{listeners.delete(listener);};};
const snapshot=()=>state;
const publish=(patch:Partial<typeof state>)=>{state={...state,...patch};for(const listener of listeners)listener();};
export const useThemeState=()=>useSyncExternalStore(subscribe,snapshot,snapshot);
const image=(src:string)=>/^data:image\/(?:png|webp);base64,[A-Za-z0-9+/]+=*$/.test(src);
function property(name:string,value:string){document.documentElement.style.setProperty(name,value);applied.add(name);}

export function applyScheme(id: SchemeId) {
  const root = document.documentElement;
  for(const key of applied)root.style.removeProperty(key);applied.clear();
  delete root.dataset.themePack;delete root.dataset.themePresentation;delete root.dataset.themeMouse;
  delete root.dataset.themeCursor;delete root.dataset.themeScrollbar;
  // Built-in schemes are always available; packs are additive and may
  // override a built-in ID as their base for this environment.
  const pack=state.packs.find(p=>p.id===id),base=pack?.base??(SCHEMES.some(s=>s.id===id)?id:'default');
  if (base === 'default') delete root.dataset.theme;
  else root.dataset.theme = base;
  if(pack){
   root.dataset.themePack=pack.id;if(pack.presentation)root.dataset.themePresentation=pack.presentation;
   if(pack.effects.mouse)root.dataset.themeMouse='true';
   for(const [key,value]of Object.entries(pack.colors)){
    if(!/^[a-z0-9-]{1,40}$/.test(key)||!/^#(?:[a-f\d]{3}|[a-f\d]{6}|[a-f\d]{8})$/i.test(value))continue;
    property('--'+key,value);const hex=value.slice(1);const full=hex.length===3?[...hex].map(c=>c+c).join(''):hex;
    property('--'+key+'-rgb',[0,2,4].map(i=>parseInt(full.slice(i,i+2),16)).join(' '));
   }
   for(const [role,src]of Object.entries(pack.assets))if(/^[a-z0-9-]{1,64}$/.test(role)&&image(src))property('--theme-asset-'+role,`url("${src}")`);
   if(pack.assets.cursor&&image(pack.assets.cursor))root.dataset.themeCursor='true';
   if(pack.assets.scrollbar&&image(pack.assets.scrollbar))root.dataset.themeScrollbar='true';
  }
  publish({active:pack??null});
}

export async function refreshThemePacks(root?:string){
 const version=++epoch;
 if(state.root!==(root??'')){publish({root:root??'',directory:'',packs:[],warnings:[],active:null});applySavedScheme();}
 if(!root||!isTauri()){publish({loading:false,error:false});return;}
 publish({loading:true,error:false});
 try{const result=await invoke<ThemeCatalog>('native_theme_packs',{root});
  if(version!==epoch)return;
  publish({...result,loading:false,error:false});applySavedScheme();
 }catch{if(version===epoch){publish({packs:[],loading:false,error:true});applySavedScheme();}}
}
export function useThemeCatalog(root?:string){
 const catalog=useThemeState();
 useEffect(()=>{void refreshThemePacks(root);return()=>{epoch++;};},[root]);
 return catalog;
}
export function availableSchemes(){
 const ids=new Set(state.packs.map(p=>p.id));
 return [...SCHEMES.filter(s=>!ids.has(s.id)),...state.packs.map(p=>({id:p.id,name:p.name,swatch:[p.colors.graphite??p.colors.panel??'#20262b',p.colors.ok??p.colors.blue??'#7c9cac'] as readonly[string,string]}))];
}

/** Reapply the saved choice against the current environment catalog. */
export const applySavedScheme = () => applyScheme(readPref(KEY, 'default', valid));

export function useScheme() {
  const [scheme, setScheme] = usePref<SchemeId>(KEY, 'default', valid);
  return [scheme, (id: SchemeId) => { if(!valid(id))return;writePref(KEY,id);applyScheme(id); setScheme(id); }] as const;
}
