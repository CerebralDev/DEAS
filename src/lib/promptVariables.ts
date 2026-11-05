/**
 * Mirror of the DaSiWa wildcard node's `$name` resolver (nodes_wildcard_preset_prompt_builder.py).
 * FNV-1a over `seed\x1fname:index`; same seed + same library + same pins = identical prompt.
 * Each occurrence of a known name picks independently; pins win; `$$` is a literal dollar;
 * unknown tokens stay in the text. Golden vectors: tests/prompt-variables.test.ts.
 */
export type VariableLibrary = Record<string, string[]>;
export type VariablePins = Record<string, string>;
const TOKEN = /\$\$|\$\{([A-Za-z_][A-Za-z0-9_]*)\}|\$([A-Za-z_][A-Za-z0-9_]*)/g;
export const isVariableName = (name: string) => /^[A-Za-z_][A-Za-z0-9_]*$/.test(name);
/** Deterministic name mapping for wildcard category labels (`art/negative prompt` -> `art_negative_prompt`). */
export function variableName(category: string): string {
  const name = category.replace(/[^A-Za-z0-9_]/g, "_");
  return /^[A-Za-z_]/.test(name) ? name : "_" + name;
}
export function fnv1a(text: string): number {
  let hash = 0x811c9dc5;
  for (const byte of new TextEncoder().encode(text)) hash = Math.imul(hash ^ byte, 0x01000193) >>> 0;
  return hash;
}
/** Pinned pick wins; otherwise the seed hash over `seed\x1fkey`. Mirrors `_hash_seed` + pin handling in the node. */
export function variablePick(name: string, key: string, library: VariableLibrary, pins: VariablePins, seed: number): string | undefined {
  const options = library[name];
  if (!options || !options.length) return undefined;
  const pinned = pins[key];
  if (typeof pinned === "string" && options.includes(pinned)) return pinned;
  return options[fnv1a(`${seed}\u001f${key}`) % options.length];
}
/** Occurrence scan using the node's key scheme (only known names consume occurrence indexes). */
export interface VariableToken { name: string; key: string; known: boolean }
export function scanVariables(text: string, library: VariableLibrary): VariableToken[] {
  const counts = new Map<string, number>();
  const found: VariableToken[] = [];
  for (const match of text.matchAll(TOKEN)) {
    const name = match[1] ?? match[2];
    if (name === undefined) continue;
    const known = Array.isArray(library[name]) && library[name].length > 0;
    const index = known ? (counts.get(name) ?? 0) : 0;
    if (known) counts.set(name, index + 1);
    found.push({ name, key: `${name}:${index}`, known });
  }
  return found;
}
export function resolveVariables(text: string, library: VariableLibrary, pins: VariablePins, seed: number): string {
  const counts = new Map<string, number>();
  return text.replace(TOKEN, (token: string, braced?: string, plain?: string) => {
    if (token === "$$") return "$";
    const name = braced ?? plain;
    if (typeof name !== "string") return token;
    const options = library[name];
    if (!options || !options.length) return token;
    const index = counts.get(name) ?? 0;
    counts.set(name, index + 1);
    return variablePick(name, `${name}:${index}`, library, pins, seed) ?? token;
  });
}
export function parseVariableLibrary(raw: unknown): VariableLibrary {
  let value = raw;
  if (typeof raw === "string") {
    if (raw.length > 1_000_000) throw new Error("Variable library exceeds 1000000 characters");
    value = JSON.parse(raw || "{}");
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Variable library must be a JSON object");
  const out: VariableLibrary = {};
  for (const [name, options] of Object.entries(value as Record<string, unknown>)) {
    if (!isVariableName(name)) throw new Error(`Invalid variable name: ${name}`);
    if (!Array.isArray(options) || !options.length || !options.every((x) => typeof x === "string" && x.trim().length > 0))
      throw new Error(`$${name} must contain a non-empty list of strings`);
    out[name] = [...options] as string[];
  }
  return out;
}
/** Wildcard categories become the variable library: flatten the style's wildcard/preset texts. */
export function flattenCategories(library: Record<string, { subject: string; [key: string]: unknown }[]>, style: "Booru" | "Natural Language"): VariableLibrary {
  const out: VariableLibrary = {};
  for (const [category, subjects] of Object.entries(library)) {
    const texts = subjects.flatMap((s) => (style === "Booru" ? [s.booru_wildcards, s.booru_presets] : [s.nl_wildcards, s.nl_presets]).flatMap((list) => Array.isArray(list) ? list.filter((x): x is string => typeof x === "string") : []));
    const name = variableName(category);
    if (texts.length && !out[name]) out[name] = texts;
  }
  return out;
}
/** Category library first, the user's custom JSON overrides on collisions, restricted to used names. */
export function mergeVariableLibrary(categories: VariableLibrary, custom: VariableLibrary, usedNames: Iterable<string>): VariableLibrary {
  const used = new Set(usedNames);
  const out: VariableLibrary = {};
  for (const [name, options] of Object.entries(categories)) if (used.has(name)) out[name] = options;
  for (const [name, options] of Object.entries(custom)) if (used.has(name)) out[name] = options;
  return out;
}
/** Drop pins whose occurrence no longer exists in the text. */
export function prunePins(text: string, library: VariableLibrary, pins: VariablePins): VariablePins {
  const keys = new Set(scanVariables(text, library).map((t) => t.key));
  return Object.fromEntries(Object.entries(pins).filter(([key]) => keys.has(key)));
}
export const parsePins = (raw: unknown): VariablePins => {
  try {
    const value = typeof raw === "string" ? JSON.parse(raw || "{}") : raw;
    if (!value || typeof value !== "object" || Array.isArray(value)) return {};
    return Object.fromEntries(Object.entries(value).filter(([, v]) => typeof v === "string")) as VariablePins;
  } catch { return {}; }
};
