import type { AudioEnhanceIntent } from './audioEnhance';
import families from '../families/builtin.json' with { type: 'json' };
import aceData from './aceStepData.json' with { type: 'json' };

export type AceStepFamily='ace-step-1-0'|'ace-step-1-5-base'|'ace-step-1-5-sft'|'ace-step-1-5-turbo'|'ace-step-1-5-xl-base'|'ace-step-1-5-xl-sft'|'ace-step-1-5-xl-turbo';
export type AceStepSettings={shift:number;lyricsStrength:number;bpm:number;timeSignature:'2'|'3'|'4'|'6';language:string;keyScale:string;generateAudioCodes:boolean;lmCfg:number;temperature:number;topP:number;topK:number;minP:number};
export const defaultAceSettings:AceStepSettings={shift:3,lyricsStrength:1,bpm:120,timeSignature:'4',language:'en',keyScale:'C major',generateAudioCodes:true,lmCfg:2,temperature:.85,topP:.9,topK:0,minP:0};
export type AceMusicProfile={version:'1.0'|'1.5';steps:number;cfg:number;sampler:string;scheduler:string;shift:number};
const aceProfileData=(family?:string)=>families.find(f=>f.id===family)?.music;
export function aceMusicProfile(family?:string):AceMusicProfile|undefined {
 const p=aceProfileData(family);
 return p ? {version:p.version as '1.0'|'1.5',steps:p.steps,cfg:p.cfg,sampler:p.sampler,scheduler:p.scheduler,shift:p.shift} : undefined;
}
export const aceLanguages:string[]=aceData.languages;
export const aceKeyScales:string[]=aceData.keyScales;
export const aceBounds=aceData.bounds;
export function isAceSettings(value:unknown):value is AceStepSettings {
 if(!value||typeof value!=='object'||Array.isArray(value))return false;
 const v=value as Record<string,unknown>;
 const keys=[...Object.keys(aceBounds),'timeSignature','language','keyScale','generateAudioCodes'];
 return Object.keys(v).length===keys.length&&Object.keys(v).every(k=>keys.includes(k))&&
 Object.entries(aceBounds).every(([key,[min,max,integer]])=>typeof v[key]==='number'&&Number.isFinite(v[key])&&(v[key] as number)>=(min as number)&&(v[key] as number)<=(max as number)&&(!integer||Number.isInteger(v[key])))&&
 aceData.timeSignatures.includes(v.timeSignature as string)&&aceLanguages.includes(v.language as string)&&aceKeyScales.includes(v.keyScale as string)&&typeof v.generateAudioCodes==='boolean';
}
export function aceDurationStep(family?:string):number {
 const p=aceProfileData(family);return p?p.samplesPerLatent/p.sampleRate:0.04;
}
/** Normalize the editable value BEFORE displaying/submitting it, never inside the compiler. */
export function normalizeAceDuration(seconds:number,family?:string):number {
 const p=aceProfileData(family);if(!p||!Number.isFinite(seconds))return seconds;
 const frames=Math.max(Math.ceil(p.sampleRate/p.samplesPerLatent),Math.min(Math.floor(1000*p.sampleRate/p.samplesPerLatent),Math.round(seconds*p.sampleRate/p.samplesPerLatent)));
 let duration=frames*p.samplesPerLatent/p.sampleRate;
 // Original core truncates, so move one floating-point epsilon above a roundoff-short boundary.
 if(p.version==='1.0'&&Math.floor(duration*p.sampleRate/512/8)!==frames)duration+=Number.EPSILON*duration;
 return duration;
}
export interface AceStepMusicIntent {
 readonly recipeId:'music-generation';readonly version:1;readonly family:AceStepFamily;readonly mode:'t2m';readonly planning:'off';
 readonly checkpoint:string;readonly style:string;readonly lyrics:string;readonly seed:number;readonly maxDuration:number;
 readonly steps:number;readonly cfg:number;readonly sampler:string;readonly scheduler:string;readonly outputPrefix:string;readonly ace:AceStepSettings;
 readonly diffusion?:{textEncoder:string;textEncoder2:string;vae:string};readonly audioEnhance?:AudioEnhanceIntent;
 readonly generation?:never;readonly abc?:never;readonly sourceAudio?:never;readonly audioEncoder?:never;readonly options?:never;readonly loras?:never;
}
export type MusicIntentV1=Yue2MusicIntentV1|AceStepMusicIntent;
export const isAceMusicIntent=(i:MusicIntentV1):i is AceStepMusicIntent=>i.family!=='yue2';
/** YuE2 music recipe intent (native compiler `jobs_music.rs`): text-to-music with ABC planning, or a SheetSage2 melody cover. */
export interface AbcSettings { maxTokens:number; temperature:number; topP:number; topK:number; repetitionPenalty:number; penaltyWindow:number }
export interface MusicGenerationSettings { temperature:number; topP:number; topK:number; repetitionPenalty:number; cfgScale:number }
export const defaultAbc:AbcSettings={maxTokens:8192,temperature:0.7,topP:0.9,topK:30,repetitionPenalty:1.005,penaltyWindow:100};
export const defaultMusicGeneration:MusicGenerationSettings={temperature:1,topP:0.95,topK:100,repetitionPenalty:1.2,cfgScale:1};
/** Template maxima: 120 s text-to-music, 360 s cover; the node's hard ceiling is 900 s on a 0.04 s grid. */
export const musicMaxDuration={t2m:120,cover:360,limit:900,step:0.04} as const;
export type MusicPlanning='off'|'full'|'melody';
/** `[field,min,max,integer]` as validated natively; the UI keeps controls inside these bounds. */
export const musicBounds={
 generation:{temperature:[0,5,false],topP:[0.01,1,false],topK:[1,32768,true],repetitionPenalty:[0.01,10,false],cfgScale:[0,100,false]},
 abc:{maxTokens:[1,20000,true],temperature:[0,5,false],topP:[0.01,1,false],topK:[1,32768,true],repetitionPenalty:[0.01,10,false],penaltyWindow:[1,20000,true]},
} as const satisfies {generation:Record<keyof MusicGenerationSettings,readonly [number,number,boolean]>;abc:Record<keyof AbcSettings,readonly [number,number,boolean]>};
export interface MusicBase { readonly audioEnhance?:AudioEnhanceIntent;
 readonly recipeId:'music-generation'; readonly version:1; readonly family:'yue2';
 readonly checkpoint:string; readonly style:string; readonly lyrics:string;
 readonly seed:number; readonly maxDuration:number;
 readonly steps:number; readonly cfg:number; readonly sampler:string; readonly scheduler:string;
 readonly generation:MusicGenerationSettings; readonly outputPrefix:string;
 readonly options?:never; readonly loras?:never;
}
export type Yue2MusicIntentV1=MusicBase&(
 |{readonly mode:'t2m';readonly planning:'off';readonly abc?:never}
 |{readonly mode:'t2m';readonly planning:'full'|'melody';readonly abc:AbcSettings}
 |{readonly mode:'cover';readonly planning:'melody';readonly sourceAudio:string;readonly audioEncoder:string}
);
export const isMusicIntent=(i:{recipeId:string}):i is MusicIntentV1=>i.recipeId==='music-generation';
export const musicPipelines=['t2m','cover'] as const;
export const isMusicPipeline=(p:string):p is 't2m'|'cover'=>p==='t2m'||p==='cover';
