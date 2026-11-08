import { invoke } from '@tauri-apps/api/core';

/** Media Analysis (ported from PromptForge): image taggers run on the CPU by DEAS's private analysis runtime. */
export interface Tagger { id: string; name: string; model: string; labels: string; source: 'deas' | 'wd14-node' | 'animetimm'; generalThreshold: number; characterThreshold: number }
export interface AnalysisInstall { running: boolean; stage: string; downloaded: number; total: number; error: string | null }
export interface AnalysisOverview { runtimeReady: boolean; install: AnalysisInstall; taggers: Tagger[]; defaultMissing: boolean; defaultSize: number }
export interface TagResult { tagger: string; rating: Record<string, number>; general: [string, number][]; character: [string, number][]; ms: number }

/** The model folders to look in: the shared one, the environment's own and any extra roots. */
export const modelFolders = (models?: string, extra: string[] = [], root?: string) =>
  [...new Set([models, root ? `${root}/ComfyUI/models` : undefined, ...extra].filter((m): m is string => !!m && !!m.trim()))];

export const analysisOverview = (models: string[]) => invoke<AnalysisOverview>('native_analysis_overview', { models });
export const setupAnalysis = (models: string[]) => invoke<AnalysisOverview>('native_analysis_setup', { models });
export const cancelAnalysisSetup = () => invoke<void>('native_analysis_cancel');
export const tagPicture = (models: string[], tagger: string, image: string, general?: number, character?: number) =>
  invoke<TagResult>('native_analysis_tag', { models, tagger, image, general, character });
/** A pasted or picked picture has no path; DEAS keeps it in its inbox so a tagger can read it. */
export async function stagePicture(file: File): Promise<string> {
  const ext = (file.type.split('/')[1] ?? file.name.split('.').pop() ?? '').replace('jpeg', 'jpg');
  return invoke<string>('native_analysis_stage', { bytes: Array.from(new Uint8Array(await file.arrayBuffer())), ext });
}

/** Booru tags as a prompt: underscores become spaces except in kaomoji like ^_^, brackets are escaped. */
export function tagsAsPrompt(tags: string[], spaces = true) {
  return tags.map(t => {
    const kept = spaces && !/^[^a-z0-9]*_[^a-z0-9]*$/i.test(t) && t.length > 3 ? t.replace(/_/g, ' ') : t;
    return kept.replace(/([()])/g, '\\$1');
  }).join(', ');
}

/** "ANALYSIS_RUN: message" codes stay out of what people read. */
export const analysisError = (e: unknown) => String(e instanceof Error ? e.message : e).replace(/^[A-Z_]+: /, '');
