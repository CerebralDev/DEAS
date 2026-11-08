// Pure planning data. Text is resolved by the owning module's language resources.
export type Motion = 'light' | 'paws' | 'off';
export type StepId = 'route' | 'storage' | 'pack' | 'acceleration' | 'backend' | 'review';
export type FeatureId = 'director' | 'forge' | 'refmods' | 'continuity';
export type Backend = 'cpu' | 'nvidia' | 'amd' | 'intel';
export interface InstallOptions { sage: boolean; radial: boolean; flash: boolean; ffmpeg: boolean }
export interface SetupState { mode?: 'automatic'; version: 2; opened: boolean; motion: Motion; values: Partial<Record<StepId,string>>; confirmed: Partial<Record<StepId,boolean>>; features: FeatureId[]; options: InstallOptions; comfyRef: string }
export interface SetupChoice { included?: {id:string;label:string}[]; suggested?: boolean; detected?: boolean; id: string; label: string; description?: string; status?: string; selected?: boolean; disabled?: boolean; suggests?: string[] }
export interface SetupNode { included?: {id:string;label:string}[]; suggested?: boolean; detected?: boolean; id: string; label: string; description?: string; status?: string; parentId: string|null; column: number; order: number; selected?: boolean; choices?: SetupChoice[]; selectionMode?: 'single'|'multi' }
/** Join an existing native-selected parent with a new, single safe child name. */
export function storageTarget(parent: string, child: string): string|null {
 if (!child || child.trim()!==child || /[\/\\\x00-\x1f<>:"|?*]/.test(child) || child==='.' || child==='..' || /[. ]$/.test(child) || /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(child)) return null;
 const windows=/^[A-Za-z]:[\/\\]/.test(parent);
 if (!(parent==='/' || validateStorage(parent) || /^[A-Za-z]:[\/\\]$/.test(parent))) return null;
 const separator=windows && parent.includes('\\')?'\\':'/';
 const target=parent.endsWith('/')||parent.endsWith('\\')?parent+child:parent+separator+child;
 return validateStorage(target)?target:null;
}
export function validateBackend(value: string): boolean {
 if (!/^https?:\/\//i.test(value) || /[\s\\?#]/.test(value)) return false;
 try { const u=new URL(value); return !!u.hostname && !u.username && !u.password && !value.includes('@'); } catch { return false; }
}
export function validateStorage(value: string): boolean {
 if (!value || value.trim()!==value || /[\x00-\x1f<>"|?*]/.test(value) || value.split(/[\\/]/).some(p=>p==='..'||p==='.')) return false;
 return (/^\/[^/]/.test(value) && !value.includes('\\') && !value.includes(':')) || (/^[A-Za-z]:[\\/].+/.test(value) && !value.slice(2).includes(':'));
}
const knownFeatures: FeatureId[] = ['director','forge','refmods','continuity'];
export const optionalFeatures: FeatureId[] = ['director'];
export function effectiveSetupFeatures(features: readonly FeatureId[]): FeatureId[] { return knownFeatures.filter(id=>features.includes(id)||features.includes('director')); }
export function initialSetupState(): SetupState { return {version:2,opened:false,motion:'light',values:{},confirmed:{},features:[],options:{sage:false,radial:false,flash:false,ffmpeg:false},comfyRef:'master'}; }
export function visibleSetupSteps(state: SetupState): StepId[] {
 return state.opened&&state.mode==='automatic'?['route','storage','pack','acceleration','review']:[];
}
export function activeSetupStep(state: SetupState): StepId|null { return visibleSetupSteps(state).at(-1)??null; }
export type SetupEvent = {type:'automatic'}|{type:'select';step:'storage'|'acceleration';value:string}|{type:'feature';id:FeatureId}|{type:'option';id:keyof InstallOptions;enabled:boolean}|{type:'ref';value:string};
export function transitionSetup(state: SetupState,event: SetupEvent): SetupState {
 if(event.type==='automatic') return {...state,mode:'automatic',opened:true,values:{...state.values,route:'managed',pack:'complete'},features:state.features.length?state.features:['director']}; // Director is the recommended package; untick to install core only
 if(state.mode!=='automatic') return state;
 if(event.type==='select'&&event.step==='storage') return {...state,values:{...state.values,storage:event.value}};
 if(event.type==='select'&&event.step==='acceleration'&&['cpu','nvidia','amd','intel'].includes(event.value)) return {...state,values:{...state.values,acceleration:event.value}};
 if(event.type==='feature'&&optionalFeatures.includes(event.id)) return {...state,features:state.features.includes(event.id)?state.features.filter(id=>id!==event.id):[...state.features,event.id]};
 if(event.type==='option'&&event.id in state.options) return {...state,options:{...state.options,[event.id]:event.enabled}};
 if(event.type==='ref') return {...state,comfyRef:event.value};
 return state;
}
export interface NativeInstallDefaults {storagePath:string;backend:'cpu'|'nvidia'|'amd';hardware:Record<string,unknown>|null;warnings:string[]}
export interface NativeHardwareDetection {backend: 'nvidia'|'amd'|null; hardware?:Record<string,unknown>|null; warnings?:string[]}
export interface NativeInstallRequest {storagePath:string;backend:Backend;features:FeatureId[];options:InstallOptions;comfyRef:string;folders?:import('./settings').Folders;takeover?:boolean}
export interface NativeInstallPlan {id:string;ggufRef?:string;reopenId?:string;request:NativeInstallRequest;steps:{id:string;labelKey:string;detail:string}[];warnings:string[];unsupportedOptions:string[];canExecute:boolean;hardware?:{vendor:string;name:string;uuid:string;driverVersion:string;computeCapability:string}|null;torch?:{packages:string[];indexUrl:string;cuda:string|null}|null}
export interface NativeInstallStatus {state:'idle'|'running'|'succeeded'|'failed'|'cancelled';stepId:string|null;logs:string[];error:string|null}
export interface NativeBackendStatus {state:'starting'|'running'|'failed'|'stopped';endpoint:string|null;error:string|null;generationReady:false}
export interface DependencyCheck {id:string;ok:boolean}
export type SetupPlan = {version:2;provisioningAvailable:false;status:'NOT INSTALLED';route:'managed'} & NativeInstallRequest;
export function setupPlan(state: SetupState): SetupPlan|null {
 if(activeSetupStep(state)!=='review'||! /^[\w./-]{1,120}$/.test(state.comfyRef)) return null;
 const v=state.values;
 if(v.route==='managed'&&v.storage&&validateStorage(v.storage)&&v.pack==='complete'&&['cpu','nvidia','amd','intel'].includes(v.acceleration??'')) return {version:2,status:'NOT INSTALLED',route:'managed',storagePath:v.storage,backend:v.acceleration as Backend,features:effectiveSetupFeatures(state.features),options:{...state.options},comfyRef:state.comfyRef,provisioningAvailable:false};
 return null;
}
