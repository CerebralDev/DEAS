import { Channel, invoke } from '@tauri-apps/api/core';

/** Media Analysis (ported from PromptForge): image taggers run on the CPU by DEAS's private analysis runtime. */
export interface Tagger { id: string; name: string; model: string; labels: string; place: string; tags: number; categories: boolean; generalThreshold: number; characterThreshold: number }
/** Found where taggers are kept, but not usable as it is: the reason says why, in plain words. */
export interface UnusableTagger { name: string; place: string; reason: string }
export interface AnalysisInstall { running: boolean; stage: string; downloaded: number; total: number; error: string | null }
export interface AnalysisOverview { runtimeReady: boolean; install: AnalysisInstall; taggers: Tagger[]; unusable: UnusableTagger[]; defaultSize: number }
export interface TagResult { tagger: string; rating: Record<string, number>; general: [string, number][]; character: [string, number][]; ms: number }

/** The model folders to look in: the shared one, the environment's own and any extra roots. */
export const modelFolders = (models?: string, extra: string[] = [], root?: string) =>
  [...new Set([models, root ? `${root}/ComfyUI/models` : undefined, ...extra].filter((m): m is string => !!m && !!m.trim()))];

export const analysisOverview = (models: string[]) => invoke<AnalysisOverview>('native_analysis_overview', { models });
/** Installs the runtime; `download` also fetches WD v3 EVA02, and only when no tagger is found at all. */
export const setupAnalysis = (models: string[], download = false) => invoke<AnalysisOverview>('native_analysis_setup', { models, download });
export const cancelAnalysisSetup = () => invoke<void>('native_analysis_cancel');
export const tagPicture = (models: string[], tagger: string, image: string, general?: number, character?: number) =>
  invoke<TagResult>('native_analysis_tag', { models, tagger, image, general, character });
/** Choose…: a picture or video by path, through the system's file dialog; null when cancelled. */
export const pickMedia = (title: string) => invoke<string | null>('native_analysis_pick', { title });
/** A pasted or picked picture has no path; DEAS keeps it in its inbox so a tagger can read it. */
export async function stagePicture(file: File): Promise<string> {
  const ext = (file.type.split('/')[1] ?? file.name.split('.').pop() ?? '').replace('jpeg', 'jpg');
  return invoke<string>('native_analysis_stage', { bytes: Array.from(new Uint8Array(await file.arrayBuffer())), ext });
}

/** The frame picker: frames come from FFmpeg by number, so each is exact. */
export interface Clip { name: string; width: number; height: number; fps: number; count: number; duration: number; stride: number; thumbs: string[] }
export const openClip = (path: string) => invoke<Clip>('native_analysis_frames_open', { path });
/** Frame `index` (from 0) at full size: a PNG path the tagger, Describe and Generate can read. */
export const clipFrame = (path: string, index: number, count: number) => invoke<string>('native_analysis_frame', { path, index, count });
/** Saves frames into a folder the person chooses; null when they cancel. */
export const saveFrames = (path: string, indices: number[], count: number, title: string) => invoke<number | null>('native_analysis_frames_save', { path, indices, count, title });

/** Describe: the picture in words from the vision LLM, with the tagger's tags and, when it recognised a character, that series' cast. */
export interface Reading { text: string; description: string; characters: string[]; suggested: string[]; unknown: string[]; doubtful: string; cast: [string, number][]; stopped: boolean }
export type DescribeEvent = { type: 'started'; cast: [string, number][] } | { type: 'text'; text: string };
export function describePicture(root: string, model: string, image: string, tags: string[], requestId: string, onEvent: (e: DescribeEvent) => void) {
  const channel = new Channel<DescribeEvent>();
  channel.onmessage = onEvent;
  return invoke<Reading>('native_analysis_describe', { root, model, image, tags, requestId, onEvent: channel });
}
export const cancelDescribe = (requestId: string) => invoke<boolean>('native_analysis_describe_cancel', { requestId });

/** Booru tags as a prompt: underscores become spaces except in kaomoji like ^_^, brackets are escaped. */
export function tagsAsPrompt(tags: string[], spaces = true) {
  return tags.map(t => {
    const kept = spaces && !/^[^a-z0-9]*_[^a-z0-9]*$/i.test(t) && t.length > 3 ? t.replace(/_/g, ' ') : t;
    return kept.replace(/([()])/g, '\\$1');
  }).join(', ');
}

/** "ANALYSIS_RUN: message" codes stay out of what people read. */
export const analysisError = (e: unknown) => String(e instanceof Error ? e.message : e).replace(/^[A-Z_]+: /, '');

/** `m:ss.ss`, as the frame picker shows a frame's time. */
export const clock = (s: number) => `${Math.floor(s / 60)}:${(s % 60).toFixed(2).padStart(5, '0')}`;
