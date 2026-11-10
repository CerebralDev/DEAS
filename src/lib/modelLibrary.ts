import { invoke } from '@tauri-apps/api/core';
import type { LocalModel, ModelScanStatus } from './models';
export interface VirtualFolder { id:string; name:string; parent:string|null }
export interface LibraryEntry { folder:string|null; tags:string[]; kind:string; pinned?:boolean;notes?:string;thumbnail?:string|null }
export interface SavedLibraryView { name:string;scope:string;mode:'disk'|'auto'|'manual';folder:string;kind:string;family:string;tag:string;query:string;order:'default'|'type'|'manual';pinned:boolean;missing:boolean }
export interface ModelLibrary { schemaVersion:1; revision:number; folders:VirtualFolder[]; entries:Record<string,LibraryEntry>; order:string[]; views:SavedLibraryView[] }
export const emptyLibrary=():ModelLibrary=>({schemaVersion:1,revision:0,folders:[],entries:{},order:[],views:[]});
export const libraryChanged='deas:model-library-changed';
export const readModelLibrary=async()=>{const v=await invoke<ModelLibrary>('native_model_library_read');if(!v||v.schemaVersion!==1||!Array.isArray(v.folders)||!Array.isArray(v.order)||!v.entries)throw new Error('Model library unavailable; existing organization preserved.');return {...v,views:v.views??[]};};
export const writeModelLibrary=(library:ModelLibrary)=>invoke<ModelLibrary>('native_model_library_write',{library});
export const refreshSelectedModels=(paths:string[],authenticated:boolean)=>invoke<ModelScanStatus>('native_model_scan_selected',{paths,authenticated});
export const cancelModelRefresh=(id:string)=>invoke<ModelScanStatus>('native_model_scan_cancel',{id});
export const modelKey=(m:LocalModel)=>m.path?(/^[a-z]:[\\/]/i.test(m.path)?m.path.replace(/\\/g,'/').toLowerCase():m.path):`${m.category}:${m.name}`;
export const modelKind=(m:LocalModel,lib:ModelLibrary)=>lib.entries[modelKey(m)]?.kind||m.info?.civitai?.category||m.info?.civitai?.modelType||m.category;
export const modelFamily=(m:LocalModel)=>m.info?.civitai?.base||m.family||'Unknown';
export const tagsOf=(m:LocalModel,lib:ModelLibrary)=>lib.entries[modelKey(m)]?.tags??[];
export const matchesModel=(m:LocalModel,lib:ModelLibrary,query:string)=>[m.name,m.info?.civitai?.name,m.info?.civitai?.version,m.info?.civitai?.modelId,m.info?.civitai?.versionId,modelKind(m,lib),modelFamily(m),...tagsOf(m,lib),...(m.info?.civitai?.trainedWords??[])].filter(Boolean).join(' ').toLowerCase().includes(query.trim().toLowerCase());
export function folderLabel(id:string,lib:ModelLibrary):string {const parts:string[]=[];let f=lib.folders.find(f=>f.id===id);const seen=new Set<string>();while(f&&!seen.has(f.id)){seen.add(f.id);parts.unshift(f.name);f=lib.folders.find(x=>x.id===f!.parent);}return parts.join(' / ');}
export function inFolder(m:LocalModel,id:string,lib:ModelLibrary):boolean {let f:string|null|undefined=lib.entries[modelKey(m)]?.folder;if(id==='@unfiled')return !f;const seen=new Set<string>();while(f&&!seen.has(f)){if(f===id)return true;seen.add(f);f=lib.folders.find(x=>x.id===f)?.parent;}return false;}
export function reorder<T>(items:readonly T[],item:T,direction:number):T[]{const next=[...items],i=next.indexOf(item),j=i+direction;if(i>=0&&j>=0&&j<next.length)[next[i],next[j]]=[next[j],next[i]];return next;}
