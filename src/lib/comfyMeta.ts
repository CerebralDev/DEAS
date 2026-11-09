// What ComfyUI wrote inside a picture: seed, sampler settings, checkpoint, LoRAs and the
// prompt text. Ported from PromptForge's server/comfymeta.mjs for Media Analysis'
// "Made with".
//
// ComfyUI's SaveImage writes the graph twice: `prompt` (API form: every node's class_type
// and inputs) and `workflow` (UI form). Only `prompt` holds settings, so that is the one read.
// Settings are often not on the sampler itself: seeds come from seed nodes (rgthree's Seed),
// the split sampler layout keeps steps/cfg on sibling nodes, and prompts are assembled at run
// time by concat/wildcard nodes. So links are followed, and an assembled prompt is reported
// as the pieces that were typed, marked partial, never guessed into a whole.
import { generationSettings, parseMetadataJson, type ImageMetadata } from './imageMetadata';

type Node = { class_type?: unknown; inputs?: Record<string, unknown> };
type Graph = Record<string, Node>;
type Link = [string | number, number];
type Value = string | number | boolean;

export interface ComfyMeta {
  /** Exact digits for long seeds (ComfyUI's go to 2^64-1, past what a JS number holds). */
  seed: string | null;
  /** The class of the first sampler in pipeline order (KSampler, SamplerCustomAdvanced…). */
  sampler: string | null;
  settings: Record<string, Value>;
  checkpoints: string[];
  loras: { name: string; model: number | null; clip: number | null }[];
  prompt: string | null;
  promptPartial: boolean;
  negative: string | null;
  negativePartial: boolean;
}

const SEED_KEYS = ['seed', 'noise_seed'];
const SAMPLER_MARKERS = ['sampler_name', 'guider', 'sigmas'];
const EFFORT_MARKERS = ['steps', 'cfg', 'denoise', 'guider', 'positive'];
const isLink = (v: unknown): v is Link => Array.isArray(v) && v.length > 0 && (typeof v[0] === 'string' || typeof v[0] === 'number');
const has = (node: Node | undefined, keys: string[]) => keys.some(k => node?.inputs?.[k] !== undefined);
const isSampler = (node: Node) => (has(node, SAMPLER_MARKERS) && has(node, EFFORT_MARKERS)) || (has(node, SEED_KEYS) && has(node, ['steps', 'cfg']));

/** The seed for a sampler, written on it or wired into it from a seed node. */
function resolveSeed(graph: Graph, node: Node | undefined, depth = 0, seen = new Set<string>()): Value | undefined {
  if (!node || depth > 6) return undefined;
  for (const key of SEED_KEYS) {
    const v = node.inputs?.[key];
    if (typeof v === 'number' || typeof v === 'string') return v;
    if (isLink(v) && !seen.has(String(v[0]))) {
      seen.add(String(v[0]));
      const found = resolveSeed(graph, graph[String(v[0])], depth + 1, seen);
      if (found !== undefined) return found;
    }
  }
  return undefined;
}

/** Long seed literals quoted before parsing, so they survive as exact digits. */
const quoteLongSeeds = (raw: string) => raw.replace(/"(seed|noise_seed)"\s*:\s*(\d{16,})/g, (_m, key, digits) => `"${key}": "${digits}"`);

const CONTROL_KEYS = new Set(['delimiter', 'clean_whitespace', 'mode', 'seed', 'noise_seed', 'strip_whitespace', 'separator', 'return_with_leading_comma',
  'ckpt_name', 'unet_name', 'vae_name', 'lora_name', 'clip_name', 'model_name', 'style_model_name', 'control_net_name', 'upscale_model_name']);
const NOT_TEXT_LINKS = new Set(['clip', 'model', 'vae', 'images', 'image', 'latent_image', 'samples', 'control_net', 'mask', 'sigmas', 'sampler']);
const ASSEMBLES_TEXT = /concat|wildcard|switch|combine|join|dynamicprompt/i;

/** The words somebody typed, from a conditioning link; partial when a node assembled text at run time. */
function collectText(graph: Graph, link: unknown, depth = 0, seen = new Set<string>(), out = { parts: [] as string[], partial: false }) {
  if (depth > 12 || !isLink(link)) return out;
  const id = String(link[0]);
  if (seen.has(id)) return out;
  seen.add(id);
  const node = graph[id];
  if (!node) return out;
  if (ASSEMBLES_TEXT.test(String(node.class_type ?? ''))) out.partial = true;
  for (const [key, v] of Object.entries(node.inputs ?? {})) {
    if (CONTROL_KEYS.has(key) || key.startsWith('Select')) continue;
    if (typeof v === 'string' && v.trim().length > 2) {
      const value = v.trim();
      if (!out.parts.includes(value)) out.parts.push(value);
    } else if (isLink(v) && !NOT_TEXT_LINKS.has(key)) collectText(graph, v, depth + 1, seen, out);
  }
  return out;
}
function textFrom(graph: Graph, link: unknown) {
  const { parts, partial } = collectText(graph, link);
  const text = parts.join(', ').trim();
  return text ? { text, partial } : null;
}

/** The node holding positive/negative: the sampler, or (split layout) a guider upstream of it. */
function findConditioning(graph: Graph, id: string, depth = 0, seen = new Set<string>()): Node | null {
  if (depth > 6 || seen.has(id)) return null;
  seen.add(id);
  const node = graph[id];
  if (!node) return null;
  if (node.inputs?.positive !== undefined || node.inputs?.negative !== undefined) return node;
  for (const v of Object.values(node.inputs ?? {})) if (isLink(v)) {
    const found = findConditioning(graph, String(v[0]), depth + 1, seen);
    if (found) return found;
  }
  return null;
}

const SPLIT_KEYS = ['steps', 'cfg', 'sampler_name', 'scheduler', 'denoise'];
/** Settings on sibling nodes upstream of this sampler; only adds keys it does not carry. */
function fillSplitSettings(graph: Graph, id: string, settings: Record<string, Value>, depth = 0, seen = new Set<string>()) {
  if (depth > 6 || seen.has(id)) return settings;
  seen.add(id);
  const node = graph[id];
  if (!node) return settings;
  for (const key of SPLIT_KEYS) {
    const v = node.inputs?.[key];
    if (settings[key] === undefined && (typeof v === 'number' || typeof v === 'string')) settings[key] = v;
  }
  for (const v of Object.values(node.inputs ?? {})) if (isLink(v)) fillSplitSettings(graph, String(v[0]), settings, depth + 1, seen);
  return settings;
}

const scalars = (node: Node) => {
  const out: Record<string, Value> = {};
  for (const [k, v] of Object.entries(node.inputs ?? {})) if (typeof v === 'number' || typeof v === 'string' || typeof v === 'boolean') out[k] = v;
  return out;
};

/** How far each node sits upstream of a saved image, so samplers come in pipeline order. */
function depthsFromSave(graph: Graph) {
  const terminals = Object.entries(graph).filter(([, n]) => /SaveImage|SaveAnimated|Image Save/i.test(String(n?.class_type ?? ''))).map(([id]) => id);
  if (!terminals.length) return null;
  const depth = new Map<string, number>();
  let frontier = terminals, d = 0;
  while (frontier.length && d < 1000) {
    const next: string[] = [];
    for (const id of frontier) {
      if (depth.has(id)) continue;
      depth.set(id, d);
      for (const v of Object.values(graph[id]?.inputs ?? {})) if (isLink(v) && graph[String(v[0])] && !depth.has(String(v[0]))) next.push(String(v[0]));
    }
    frontier = next; d += 1;
  }
  return depth;
}

/** The settings out of a picture's ComfyUI graph, or null when it has none to read. */
export function comfyMeta(entries: ImageMetadata): ComfyMeta | null {
  const raw = entries.find(([key]) => key.toLowerCase() === 'prompt')?.[1];
  if (!raw) return null;
  let graph: unknown;
  try { graph = parseMetadataJson(quoteLongSeeds(raw)); } catch { return null; }
  if (!graph || typeof graph !== 'object' || Array.isArray(graph)) return null;
  const g = graph as Graph;
  const depth = depthsFromSave(g);
  const nodes = Object.entries(g).filter(([, n]) => n && typeof n === 'object');
  const samplers = nodes.filter(([, n]) => isSampler(n)).map(([id, n]) => {
    const settings = scalars(n);
    const seed = resolveSeed(g, n);
    if (seed !== undefined && settings.seed === undefined && settings.noise_seed === undefined) settings.seed = seed;
    fillSplitSettings(g, id, settings);
    return { id, type: String(n.class_type ?? 'unknown'), depth: depth?.get(id) ?? 0, settings };
  }).sort((a, b) => b.depth - a.depth || a.id.localeCompare(b.id));
  const checkpoints = nodes.filter(([, n]) => /CheckpointLoader|UNETLoader|Checkpoint Loader/i.test(String(n.class_type ?? '')))
    .map(([, n]) => n.inputs?.ckpt_name ?? n.inputs?.unet_name).filter((v): v is string => typeof v === 'string');
  const num = (v: unknown) => (typeof v === 'number' ? v : null);
  const loraName = (v: unknown) => String(v ?? '').replace(/\\/g, '/');
  const loras: ComfyMeta['loras'] = [];
  for (const [, n] of nodes) {
    if (!/LoraLoader|Lora Loader/i.test(String(n.class_type ?? ''))) continue;
    if (typeof n.inputs?.lora_name === 'string') { loras.push({ name: loraName(n.inputs.lora_name), model: num(n.inputs.strength_model), clip: num(n.inputs.strength_clip) }); continue; }
    // rgthree's Power Lora Loader: lora_1, lora_2… objects, each switchable.
    for (const [key, slot] of Object.entries(n.inputs ?? {})) {
      if (!/^lora_\d+$/i.test(key) || !slot || typeof slot !== 'object' || Array.isArray(slot)) continue;
      const s = slot as Record<string, unknown>;
      if (s.on === false || typeof s.lora !== 'string' || !s.lora.trim()) continue;
      loras.push({ name: loraName(s.lora), model: num(s.strength), clip: num(s.strengthTwo) ?? num(s.strength) });
    }
  }
  if (!samplers.length && !checkpoints.length && !loras.length) return null;
  const lead = samplers[0] ?? null;
  const cond = lead ? findConditioning(g, lead.id) : null;
  const positive = cond ? textFrom(g, cond.inputs?.positive) : null;
  const negative = cond ? textFrom(g, cond.inputs?.negative) : null;
  const seed = lead ? lead.settings.seed ?? lead.settings.noise_seed : undefined;
  return {
    seed: seed === undefined ? null : String(seed),
    sampler: lead?.type ?? null,
    settings: lead?.settings ?? {},
    checkpoints: [...new Set(checkpoints)],
    loras,
    prompt: positive?.text ?? null,
    promptPartial: !!positive?.partial,
    negative: negative?.text ?? null,
    negativePartial: !!negative?.partial,
  };
}

/** "Made with" for any picture: a ComfyUI graph read as above, else the A1111/Forge/Swarm
 *  parameters Generate's "Apply settings from file" reads. Null when there are no settings. */
export function madeSummary(entries: ImageMetadata): ComfyMeta | null {
  const comfy = comfyMeta(entries);
  if (comfy) return comfy;
  const s = generationSettings(entries);
  if (!s) return null;
  const settings: Record<string, Value> = {};
  for (const [key, v] of [['steps', s.steps], ['cfg', s.cfg], ['sampler_name', s.sampler], ['scheduler', s.scheduler]] as const) if (v !== undefined) settings[key] = v;
  return {
    seed: s.seed === undefined ? null : String(s.seed),
    sampler: null,
    settings,
    checkpoints: s.checkpoint ? [s.checkpoint] : [],
    loras: (s.loras ?? []).map(l => ({ name: l.name, model: l.strength, clip: null })),
    prompt: s.positive || null,
    promptPartial: false,
    negative: s.negative || null,
    negativePartial: false,
  };
}
