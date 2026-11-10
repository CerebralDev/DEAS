import { invoke } from '@tauri-apps/api/core';
/** Installed models under the ComfyUI models folder; CivitAI metadata is cached natively beside each file. */
export interface ModelSettings { sampler?:string; scheduler?:string; steps?:number; cfgScale?:number; size?:string; clipSkip?:number }
export interface CivitaiInfo { modelId:number; versionId:number; name:string; version:string; modelType:string; category:string; base?:string; nsfw:boolean; trainedWords:string[]; preview?:string; settings?:ModelSettings }
export interface ModelInfo { sha256:string; size:number; fetchedAt:number; civitai:CivitaiInfo|null }
/** `family` is effective (declaration or detection); metadataError blocks execution, even for unknown checkpoints. */
export type FamilyEvidence = 'header' | 'filename' | 'unresolved' | 'conflict';
export interface LocalModel { category:string; name:string; path:string; size:number; info:ModelInfo|null; family?:string; detectedFamily?:string; /** ACE-Step automatic identity evidence, independent of manual declaration. */ familyEvidence?:FamilyEvidence; familyOverride?:string; metadataError?:string; /** Header-detected companion architecture; absent means unknown, not compatible. */ component?:string; /** `truncated` when the safetensors header covers payload the file does not hold. */ integrity?:string }
export const scanModels=(root:string,extraRoots:string[]=[])=>invoke<LocalModel[]>('native_models_scan',extraRoots.length?{root,extraRoots}:{root});
/** Hashes the file once (cached), asks CivitAI by hash and writes `<file>.deas.json`. */
export const fetchModelInfo=(path:string,authenticated:boolean)=>invoke<LocalModel>('native_model_info',{path,authenticated});
/** Explicit source-bound declaration only; null restores automatic detection. Never hashes or fetches. */
export const writeModelFamily=(path:string,family:string|null)=>invoke<LocalModel>('native_model_family_write',{path,family});
export type ModelScanScope='all'|'models'|'loras';
export interface ModelScanStatus { id:string; root:string; state:'idle'|'running'|'succeeded'|'failed'|'cancelled'; file:string|null; completed:number; total:number; remaining:number; failed:number;matched?:number;unmatched?:number;cancelRequested?:boolean;outcomes?:{path:string;state:'matched'|'unmatched'|'failed'}[] }
/** Native task lifetime is independent of the Library view; one batch at a time app-wide. */
export const startModelScan=(root:string,scope:ModelScanScope,authenticated:boolean)=>invoke<ModelScanStatus>('native_model_scan_start',{root,scope,authenticated});
export const readModelScanStatus=()=>invoke<ModelScanStatus>('native_model_scan_status');
