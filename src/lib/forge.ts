import { invoke } from '@tauri-apps/api/core';
import families from '../families/builtin.json' with { type: 'json' };
import type { Size } from './resolution';

/** Prompt Forge runs in the DaSiWa node pack on the backend; `native_forge` forwards one call and adds DEAS's saved LLM endpoint. */
export interface ForgeModel { id: string; label: string; disabled?: boolean }
export interface ForgeModels { models: ForgeModel[]; errors: Record<string, string>; creativity: string[]; shot_counts: (string | number)[]; default_shots: string | number; durations?: number[] }
export interface WriteRequest { model: string; write_for: string; idea: string; detail?: number; creativity?: number; /** ComfyUI LoadImage names: input-folder relative, or `name [output]`. */ images?: string[] }
/** One Director Forge reference (`js/minimax_h3_forge_state.js::forgeReferences`): `path` uses ComfyUI LoadImage names: input-relative or `name [output]`. */
export interface PeopleBox { n:number; box:[number,number,number,number] }
/** Reject unsafe or ambiguous imported box data rather than silently changing identities/geometry. */
export function readPeopleBoxes(raw:unknown):PeopleBox[]|null {
 if(!Array.isArray(raw)||raw.length>32)return null;const seen=new Set<number>();
 for(const value of raw){
  const b=value as PeopleBox;if(!b||typeof b!=='object'||!Number.isInteger(b.n)||b.n<1||b.n>32||seen.has(b.n)||!Array.isArray(b.box)||b.box.length!==4||b.box.some(v=>typeof v!=='number'||!Number.isFinite(v)||v<0||v>1)||b.box[2]-b.box[0]<0.02||b.box[3]-b.box[1]<0.02)return null;seen.add(b.n);
 }
 return raw as PeopleBox[];
}
export interface H3Reference { kind: 'image' | 'video' | 'audio'; role?: string; path?: string; easy_role?: string; boxes?: PeopleBox[]; picture_kinds?: string[]; who?: number[]; who_axis?: 'y' | 'z' | 'none'; stream?: 'video' | 'audio' | 'both'; subject_group?: string; duration_seconds?: number; instructions?: string }
export interface H3Continuity { source_kind: 'checkpoint' | 'video'; session: string; clip_id: string; overlap_frames?: 73 | 56 | 39 | 22 | 5; current_prompt: string; use_references: boolean }
export interface H3Request { continuity?: H3Continuity; model: string; mode: string; brief: string; duration: number; shots: string | number; detail: number; creativity: string; references: H3Reference[]; easy: boolean; see_pictures: boolean; shot_briefs?: string[]; output_canvas?: { width: number; height: number }; request_id?: string; /** Cast & scene cards: `{"1": text, place: text, style: text, "pose@3": text}`, each ≤ 600 chars. */ cast?: Record<string, string> }
export interface H3Result { simple_prompt: string; warnings: string[] }

/** Director label identities in picture order (`group-21` = Character 2 on the left). */
export const labelCharacters = (label = ''): string[] => label.startsWith('character-') ? [label.slice(10)] : label.startsWith('group-') ? [...label.slice(6)] : [];

export const H3 = 'MiniMax H3';
/** Dialects named by the family plugin, in family order. */
export const dialects = [...new Set((families as { forge?: string }[]).flatMap(f => f.forge ? [f.forge] : []))];
export const dialectOf = (family?: string) => (families as { id: string; forge?: string }[]).find(f => f.id === family)?.forge;

const call = <T>(baseUrl: string, root: string, route: string, body: object) => invoke<T>('native_forge', { baseUrl, root, route, body });
export const forgeModels = (baseUrl: string, root: string) => call<ForgeModels>(baseUrl, root, 'models', {});
export const writePrompt = (baseUrl: string, root: string, body: WriteRequest) => call<{ prompt: string; saw_images: number }>(baseUrl, root, 'write', body);
export const forgeH3 = (baseUrl: string, root: string, body: H3Request) => call<H3Result>(baseUrl, root, 'h3', body);
/** Reads one Cast or scene card from the labelled pictures (needs a vision model). */
export interface CastRequest { model: string; mode: string; character?: number; subject?: 'place' | 'style' | `pose@${number}`; references: H3Reference[]; brief: string; shots: string | number; shot_briefs: string[]; redo?: boolean; request_id?: string }
export const readCast = (baseUrl: string, root: string, body: CastRequest) => call<{ text: string }>(baseUrl, root, 'cast', body);
/** Explicit vision detection, bounded and sanitized by the native people route. */
export const findPeople = (baseUrl:string,root:string,body:{model:string;path:string;request_id:string}) => call<{people:{box:PeopleBox['box']}[]}>(baseUrl,root,'people',body);
/** Stops the running Director Forge call that carries this `request_id`. */
export const cancelForge = (baseUrl: string, root: string, request_id: string) => call<{ cancelled: boolean }>(baseUrl, root, 'cancel', { request_id });
/** The native Prompt Forge (image models): PromptForge's whole pipeline in DEAS — character canon, tag-search and guidance tools, clean-up — straight to the LLM server. */
export interface NativeControls { creativity?: number; detail?: number; aspectRatio?: string; canvas?: Size; qualityPreset?: string; style?: string; positive?: string; negative?: string; tags?: string[]; cameraAngle?: string; cameraShot?: string; cameraLight?: string; lighting?: number; imageReferences?: NativeImageRef[]; layout?: { width: number; height: number; items: { name: string; label: string; caption: string; text?: string; bbox: number[] }[] }; maxLength?: { unit: 'words' | 'tokens'; value: number } }
export interface NativeWrite { target: string; brief: string; model: string; mode?: string; segmentsOn?: string[]; controls?: NativeControls }
export interface NativeSegment { label: string; body: string }
export interface NativeResult { segments: NativeSegment[]; refused: boolean; prompt: string; negative: string | null; characters: string[]; tools: { name: string; queries: string[]; document: string | null; ok: boolean }[]; rounds: number; warnings: string[]; fixes: string[]; changed?: number; referenceAnalysis?: string | null }
export interface NativeTarget { key: string; label: string; modes: string[]; defaultMode: string | null; optional: string[]; qualityPresets: string[]; styles: string[]; aspectRatios: string[] | null; hasNegative: boolean; tools: boolean; guidance: string[]; secondPass?: boolean; tagStyle?: string | null; layoutCanvas?: Record<string, [number, number]> | null; layoutRewrite?: boolean; weightRange?: { min: number; max: number; step: number; default: number } | null; detailLevels?: [number, string][]; lightingLevels?: [number, string][]; camera?: Record<'angle' | 'shot' | 'light', NativeCameraOption[]> }
export interface NativeCameraOption { key: string; label: string; icon: string | null; payload: string }
export interface NativeImageRef { role: string; fidelity: string; note: string; character: string; image: string }
export interface NativeReferences { roles: { key: string; label: string; rule?: string }[]; fidelity: { key: string; label: string; rule?: string }[]; defaultRole: string | null; defaultFidelity: string | null; max: number; analyze: boolean }
export interface NativeTargets { source: { commit?: string }; creativity: string[]; targets: NativeTarget[]; references?: NativeReferences }
export const nativeForgeCancel = () => invoke<void>('native_forge_cancel');
export const nativeForgeWrite = (root: string, request: NativeWrite) => invoke<NativeResult>('native_forge_write', { root, request });
export const nativeForgeRevise = (root: string, request: NativeWrite & { segments: NativeSegment[]; instruction?: string; referenceAnalysis?: string }) => invoke<NativeResult>('native_forge_revise', { root, request });
export interface RandomizerBox { id: string; label: string; children: { id: string; label: string; options: number }[] }
export interface RandomizerRolled { boxes: { id: string; label: string; picks: { id: string; label: string; text: string }[] }[]; cast: string[]; text: string }
export interface RandomizerRoll { categories: string[]; women: number; men: number; surprise: boolean; limits: Record<string, number> }
export const randomizerList = (root: string) => invoke<RandomizerBox[]>('native_forge_randomizer_list', { root });
export const randomizerRoll = (root: string, request: RandomizerRoll) => invoke<RandomizerRolled>('native_forge_randomizer_roll', { root, request });
export interface TagSuggestion { tag: string; category: string; count: number }
export const tagSearch = (root: string, target: string, query: string) => invoke<TagSuggestion[]>('native_forge_tag_search', { root, target, query });
export const characterTraits = (root: string, target: string, character: string) => invoke<string[]>('native_forge_character_traits', { root, target, character });
export const referenceReadback = (root: string, model: string, reference: NativeImageRef) => invoke<string>('native_forge_reference_readback', { root, model, reference });
export interface LayoutRewriteRequest { task: 'scene' | 'item'; width: number; height: number; layout: unknown; description: string; element: unknown }
export const layoutRewrite = (root: string, model: string, request: LayoutRewriteRequest) => invoke<string>('native_forge_layout_rewrite', { root, model, request });
export const nativeForgeTargets = (root: string) => invoke<NativeTargets>('native_forge_targets', { root });
/** Family dialect -> native writer key; dialects without one stay on the node pack. */
export const nativeTarget = (dialect?: string) => ({ Illustrious: 'illustrious', Anima: 'anima', Krea2: 'krea2', MOSS: 'moss', 'Qwen Image 2.1': 'qwen_image' } as Record<string, string>)[dialect ?? ''];
/** Discovery qualifies model ids by provider; native requests take the bare id. */
export const nativeModelId = (id: string) => id.replace(/^(local|openai|ollama):/, '');
/** The native writer runs a local GGUF itself, so it keeps the `local:` prefix that says so. */
export const nativeWriteModel = (id: string) => id.startsWith('local:') ? id : nativeModelId(id);
/** A 404 — or 405, since ComfyUI's GET-only static `/` route catches unknown POST paths — means the node pack predates these routes. */
/** A write the user stopped is not an error, so it reads as none. */
export const forgeError = (e: unknown) => { const text = e instanceof Error ? e.message : String(e); return text === 'FORGE_CANCELLED' ? '' : /^40[45]\b/.test(text) ? 'forge.outdated' : text; };
