import { Channel, invoke } from '@tauri-apps/api/core';
import { captureStudioSettings, isString, readPref } from './preferences';
import type { JobStatus } from './jobs';
import { validCanvas, type Size } from './resolution';

const isNumber = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);

export interface ChatHit { n: number; title: string; url: string; domain: string; snippet: string }
/** The size a chat render asked for, as the person said it ("16:9", "1080p"). */
export interface RenderAsk { aspect?: string | null; resolution?: string | null }
/** What came of a chat render: the picture on disk, or why there is none. */
export interface RenderResult { path?: string | null; width: number; height: number; seed?: number | null; checkpoint: string; ms?: number | null; error?: string | null }
export interface WrittenPrompt { target: string; label: string; prompt: string; negative?: string | null; render?: RenderAsk; picture?: RenderResult }
/** What Studio's Generate can draw with, sent with each chat message. `blocked` = why it cannot right now. */
export interface ChatStudio { checkpoint: string; dialect?: string; blocked?: string }
/** A picture Chat hands to Generate: drawn with Generate's settings, its own prompt and (optionally) size. */
export interface ChatRenderRequest extends RenderAsk { id: string; positive: string; negative: string }
export type ChatRenderOutcome = { job: JobStatus; width: number; height: number; seed: number; checkpoint: string } | { error: string };

const snap = (n: number, grid: number) => Math.max(grid, Math.round(n / grid) * grid);
/**
 * The canvas a chat render asked for, or undefined to keep Generate's own size; null when it cannot be read.
 * Lenient on purpose, as PromptForge's renderSizeArgs: an LLM writes "1:1", "1x1", "square", "1080p", "1080" or "1920x1080".
 * A pixel size gives both; a short side alone keeps Generate's shape; a ratio alone keeps the model's area.
 */
export function chatRenderSize(ask: RenderAsk, model: Size, current: Size, grid: number): Size | undefined | null {
  const aspect = (ask.aspect ?? '').trim().toLowerCase();
  const res = (ask.resolution ?? '').trim().toLowerCase();
  if (!aspect && !res) return undefined;
  const pixel = /^(\d{3,5})\s*[x×*]\s*(\d{3,5})$/.exec(res) ?? /^(\d{3,5})\s*[x×*]\s*(\d{3,5})$/.exec(aspect);
  if (pixel) {
    const size = { width: snap(Number(pixel[1]), grid), height: snap(Number(pixel[2]), grid) };
    return validCanvas(size) ? size : null;
  }
  const named: Record<string, number> = { square: 1, portrait: 2 / 3, landscape: 3 / 2, wide: 16 / 9, widescreen: 16 / 9, tall: 9 / 16 };
  const ratioMatch = /^(\d+(?:\.\d+)?)\s*[:x×/]\s*(\d+(?:\.\d+)?)$/.exec(aspect);
  const ratio = aspect ? (ratioMatch ? Number(ratioMatch[1]) / Number(ratioMatch[2]) : named[aspect]) : current.width / current.height;
  if (!ratio || !Number.isFinite(ratio) || ratio <= 0) return null;
  const short = res ? Number((/^(\d{3,4})\s*p?$/.exec(res) ?? [])[1] ?? (res === '4k' ? 2160 : res === '2k' ? 1440 : NaN)) : NaN;
  if (res && !short) return null;
  let width: number, height: number;
  if (short) { width = ratio >= 1 ? short * ratio : short; height = ratio >= 1 ? short : short / ratio; }
  else { const area = model.width * model.height; width = Math.sqrt(area * ratio); height = Math.sqrt(area / ratio); }
  const size = { width: snap(width, grid), height: snap(height, grid) };
  return validCanvas(size) ? size : null;
}
export interface ChatMessage { role: 'user' | 'assistant'; content: string; pictures: number; prompts: WrittenPrompt[]; citations: ChatHit[]; tools: string[]; at: number }
export interface ChatThread { id: string; title: string; model: string; created: number; updated: number; messages: ChatMessage[] }
export interface ChatSummary { id: string; title: string; updated: number; count: number }
export interface ChatSettings { search: 'off' | 'duckduckgo' | 'searxng'; searxngUrl: string }
export type ChatEvent =
  | { event: 'chat'; id: string; title: string }
  | { event: 'searched'; query: string; results: ChatHit[]; detail: string | null }
  | { event: 'status'; state: string; detail: string | null }
  | { event: 'text'; text: string }
  | { event: 'prompt'; prompt: WrittenPrompt }
  | { event: 'done'; message: ChatMessage };
export interface ChatSend { id?: string; message: string; model: string; pictures: string[]; context?: string; search: boolean; studio?: ChatStudio & { endpoint?: string; writerModel?: string } }

/** One turn, streamed: `onEvent` sees the text as it is written. Resolves with the finished answer. */
export function sendChat(root: string, request: ChatSend, onEvent: (e: ChatEvent) => void) {
  const channel = new Channel<ChatEvent>();
  channel.onmessage = onEvent;
  return invoke<ChatMessage>('native_chat_send', { root, request, onEvent: channel });
}
export const cancelChat = (id: string) => invoke<boolean>('native_chat_cancel', { id });
export const listChats = (root: string) => invoke<ChatSummary[]>('native_chat_list', { root });
export const getChat = (root: string, id: string) => invoke<ChatThread>('native_chat_get', { root, id });
export const renameChat = (root: string, id: string, title: string) => invoke<void>('native_chat_rename', { root, id, title });
export const deleteChat = (root: string, id: string) => invoke<void>('native_chat_delete', { root, id });
/** Writes what came of a render onto the chat message that asked for it; resolves with the updated message. */
export const recordRender = (root: string, id: string, at: number, index: number, result: RenderResult) => invoke<ChatMessage>('native_chat_render_result', { root, id, at, index, result });
/** Before a chat render: unloads the chat LLM if it sits on this PC's GPU. Never throws to the caller's render. */
export const freeChatLlm = (root: string) => invoke<{ unloaded: string[]; freedBytes: number; skipped: string | null }>('native_chat_free_llm', { root }).catch(() => null);
export const chatSettings = (root: string) => invoke<ChatSettings>('native_chat_settings', { root });
export const saveChatSettings = (root: string, settings: ChatSettings) => invoke<ChatSettings>('native_chat_settings_write', { root, settings });

/** What the person is working on, read from Studio and Prompt Forge at the moment they send. */
export function workingContext() {
  const studio = captureStudioSettings();
  const s = (key: string) => {
    const value = key.startsWith('studio.run.') ? studio[key] : readPref(key, '', isString);
    return isString(value) ? value.trim() : '';
  };
  const n = (key: string) => isNumber(studio[key]) ? studio[key] : 0;
  const lines = [
    s('studio.tab') && `Open tab: ${s('studio.tab')}`,
    s('studio.run.pipeline') && `Studio pipeline: ${s('studio.run.pipeline')}`,
    s('studio.run.checkpoint') && `Model / checkpoint: ${s('studio.run.checkpoint')}`,
    n('studio.run.width') && n('studio.run.height') ? `Size: ${n('studio.run.width')}×${n('studio.run.height')}` : '',
    n('studio.run.steps') ? `Steps ${n('studio.run.steps')}, CFG ${n('studio.run.cfg')}, sampler ${s('studio.run.sampler') || 'default'} / ${s('studio.run.scheduler') || 'default'}` : '',
    s('studio.run.positive') && `Studio positive prompt:\n${s('studio.run.positive')}`,
    s('studio.run.negative') && `Studio negative prompt:\n${s('studio.run.negative')}`,
    s('forge.brief') && `Prompt Forge brief${s('forge.target') ? ` (writing for ${s('forge.target')})` : ''}:\n${s('forge.brief')}`,
  ].filter(Boolean);
  return lines.join('\n');
}
