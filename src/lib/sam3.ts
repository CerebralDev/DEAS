import { isUploadedMask, RecipeCompileError, type ApiNode, type RecipeAllowlist } from './execution.ts';
import { safeOutputPrefix } from './outputNaming.ts';

export interface Sam3MaskIntentV1 {
  recipeId: 'sam3-mask';
  version: 1;
  checkpoint: string;
  inputImage: string;
  width: number;
  height: number;
  outputPrefix: string;
  prompt?: string;
  positivePoints?: { x: number; y: number }[];
  negativePoints?: { x: number; y: number }[];
  box?: { x: number; y: number; width: number; height: number };
  threshold: number;
}
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const name = (v: unknown): v is string => typeof v === 'string' && new TextEncoder().encode(v).length <= 255 && v.split('/').every(s => s.length > 0 && s !== '.' && s !== '..' && s.trim() === s && !/[\p{Cc}\\:%]/u.test(s));
const integer = (v: unknown, min: number, max: number): v is number => typeof v === 'number' && Number.isSafeInteger(v) && v >= min && v <= max;
const required = ['recipeId', 'version', 'checkpoint', 'inputImage', 'width', 'height', 'outputPrefix', 'threshold'];
const optional = ['prompt', 'positivePoints', 'negativePoints', 'box'];
const categories = (text: string) => text.replace(/[()]/gu, '').split(',').map(s => s.trim()).filter(Boolean).map(s => s.replace(/\s*:\s*[\d.]+\s*$/u, '').trim()).filter(Boolean);

export function isSam3Intent(input: unknown): input is Sam3MaskIntentV1 {
  if (!record(input) || required.some(k => !Object.hasOwn(input, k)) || Object.keys(input).some(k => !required.includes(k) && !optional.includes(k))
    || input.recipeId !== 'sam3-mask' || input.version !== 1
    || typeof input.checkpoint !== 'string' || !name(input.checkpoint.replace(/\\/gu, '/')) || !name(input.inputImage)
    || !integer(input.width, 1, 8192) || !integer(input.height, 1, 8192) || input.width * input.height > 16_777_216
    || !safeOutputPrefix(input.outputPrefix) || typeof input.threshold !== 'number' || !Number.isFinite(input.threshold) || input.threshold < 0 || input.threshold > 1) return false;
  if (Object.hasOwn(input, 'prompt') && (typeof input.prompt !== 'string' || new TextEncoder().encode(input.prompt).length > 16384 || input.prompt.includes('\0'))) return false;
  for (const key of ['positivePoints', 'negativePoints']) {
    if (Object.hasOwn(input, key) && (!Array.isArray(input[key]) || !Array.from(input[key]).every(p => record(p) && Object.keys(p).length === 2 && ['x', 'y'].every(k => Object.hasOwn(p, k))
      && integer(p.x, 0, (input.width as number) - 1) && integer(p.y, 0, (input.height as number) - 1)))) return false;
  }
  const b = input.box;
  if (Object.hasOwn(input, 'box') && (!record(b) || Object.keys(b).length !== 4 || !['x', 'y', 'width', 'height'].every(k => Object.hasOwn(b, k)) || !integer(b.x, 0, input.width - 1) || !integer(b.y, 0, input.height - 1)
    || !integer(b.width, 1, input.width - b.x) || !integer(b.height, 1, input.height - b.y))) return false;
  return (typeof input.prompt === 'string' && categories(input.prompt).length > 0) || input.box !== undefined
    || (Array.isArray(input.positivePoints) && input.positivePoints.length > 0)
    || (Array.isArray(input.negativePoints) && input.negativePoints.length > 0);
}

/** One request unions every match in this target group; callers keep eyes/feet/hands as separate requests. */
export function compileSam3Mask(input: unknown, allow: RecipeAllowlist) {
  if (!isSam3Intent(input)) throw new RecipeCompileError('INVALID_INTENT', 'sam3-mask', 'Invalid SAM3 mask intent.', 'Supply a selected checkpoint, safe source and source-pixel prompts.');
  if (!record(allow) || !['nodes', 'checkpoints', 'samplers', 'schedulers'].every(k => Object.hasOwn(allow, k) && Array.isArray(allow[k]) && Array.from(allow[k]).every(v => typeof v === 'string' && v.length > 0))
    || (allow.images !== undefined && (!Array.isArray(allow.images) || !Array.from(allow.images).every(v => typeof v === 'string' && v.length > 0)))) {
    throw new RecipeCompileError('INVALID_ALLOWLIST', 'allowlist', 'Invalid capability lists.', 'Use verified backend prerequisites.');
  }
  const graph: Record<string, ApiNode> = {
    '1': { class_type: 'CheckpointLoaderSimple', inputs: { ckpt_name: input.checkpoint } },
    '2': { class_type: 'LoadImage', inputs: { image: input.inputImage } },
    '4': { class_type: 'SAM3_Detect', inputs: { model: ['1', 0], image: ['2', 0], threshold: input.threshold, refine_iterations: 2, individual_masks: false,
      ...(input.positivePoints?.length ? { positive_coords: JSON.stringify(input.positivePoints.map(({ x, y }) => ({ x, y }))) } : {}),
      ...(input.negativePoints?.length ? { negative_coords: JSON.stringify(input.negativePoints.map(({ x, y }) => ({ x, y }))) } : {}),
      ...(input.prompt && categories(input.prompt).length ? { conditioning: ['3', 0] as const } : {}),
      ...(input.box ? { bboxes: ['B', 0] as const } : {}) } },
    '6': { class_type: 'MaskToImage', inputs: { mask: ['4', 0] } },
    '7': { class_type: 'SaveImage', inputs: { images: ['6', 0], filename_prefix: input.outputPrefix } },
  };
  // Core SAM3Detector has 200 queries; an unsuffixed category defaults to top-1 in sam3_clip.py.
  if (input.prompt && categories(input.prompt).length) graph['3'] = { class_type: 'CLIPTextEncode', inputs: { clip: ['1', 1], text: categories(input.prompt).map(s => `${s}:200`).join(', ') } };
  if (input.box) graph.B = { class_type: 'PrimitiveBoundingBox', inputs: { ...input.box } };
  const requiredNodes = [...new Set(Object.values(graph).map(n => n.class_type))].sort();
  for (const node of requiredNodes) if (!allow.nodes.includes(node)) throw new RecipeCompileError('MISSING_CAPABILITY', node, 'SAM3 node unavailable.', 'Install a compatible core backend.');
  if (!allow.checkpoints.some(s => s.replace(/\\/gu, '/') === input.checkpoint.replace(/\\/gu, '/'))) throw new RecipeCompileError('MISSING_MODEL', 'checkpoint', 'Selected SAM3.1 checkpoint unavailable.', 'Select the explicitly installed SAM3.1 checkpoint.');
  if (!isUploadedMask(input.inputImage) && !input.inputImage.startsWith('deas-inputs/') && !allow.images?.includes(input.inputImage)) throw new RecipeCompileError('INVALID_INPUT', 'inputImage', 'Source is not available.', 'Upload the source or select a backend input.');
  return { recipeId: input.recipeId, version: 1 as const, graph, requiredNodes, requiredModels: [{ kind: 'checkpoint' as const, identifier: input.checkpoint }], generationReady: false as const };
}
