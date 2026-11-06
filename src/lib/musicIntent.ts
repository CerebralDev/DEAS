import type { AudioEnhanceIntent } from './audioEnhance';
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
export type MusicIntentV1=MusicBase&(
 |{readonly mode:'t2m';readonly planning:'off';readonly abc?:never}
 |{readonly mode:'t2m';readonly planning:'full'|'melody';readonly abc:AbcSettings}
 |{readonly mode:'cover';readonly planning:'melody';readonly sourceAudio:string;readonly audioEncoder:string}
);
export const isMusicIntent=(i:{recipeId:string}):i is MusicIntentV1=>i.recipeId==='music-generation';
export const musicPipelines=['t2m','cover'] as const;
export const isMusicPipeline=(p:string):p is 't2m'|'cover'=>p==='t2m'||p==='cover';
