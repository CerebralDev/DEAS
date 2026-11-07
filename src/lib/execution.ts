import families from '../families/builtin.json' with { type: 'json' };
import { safeOutputPrefix } from './outputNaming.ts';
import { isWildcardRules, wildcardNode, type WildcardRules } from './wildcard.ts';
/** DEAS-owned diagnostic intent, not a Director/video recipe or widget schema. */
/** Names `native_mask_upload` hands out (`deas-masks/<uuid>.png`): masks and flattened painted layers; object_info never lists that subfolder. */
export const isUploadedMask = (name: string) => /^deas-masks\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.png$/u.test(name);

export interface DiagnosticImageIntentV1 {
  readonly recipeId: 'checkpoint-text-to-image-diagnostic';
  readonly version: 1;
  readonly positivePrompt: string;
  readonly negativePrompt: string;
  readonly checkpoint: string;
  readonly seed: number;
  readonly width: number;
  readonly height: number;
  readonly steps: number;
  readonly cfg: number;
  /** Any sampler/scheduler the backend lists in object_info. */
  readonly sampler: string;
  readonly scheduler: string;
  readonly outputPrefix: string;
  /** Optional LoRA chain applied after the checkpoint; strength feeds both model and CLIP. */
  readonly loras?: readonly { readonly name: string; readonly strength: number }[];
  /** Image-to-image: a file name from the backend's input folder plus denoise strength (given together). */
  readonly inputImage?: string;
  readonly denoise?: number;
  /** Inpaint mask uploaded by `native_mask_upload` (`deas-masks/<uuid>.png`, white = repaint), at the unresized source's size. */
  readonly inputMask?: string;
  /** Gaussian mask radius in source pixels; 0 disables it (same kernel/sigma as SwarmUI). */
  readonly maskBlur?: number;
  /** Source-pixel context crop, processed at exact model-aligned target dimensions, then composited back. */
  readonly inpaintCrop?: {readonly x:number;readonly y:number;readonly width:number;readonly height:number;readonly targetWidth:number;readonly targetHeight:number};
  /** Up to 4 extra images (input or `[output]` files) pasted onto the source in source pixels before encoding. */
  readonly inputLayers?: readonly { readonly image: string; readonly x: number; readonly y: number; readonly width: number; readonly height: number }[];
  readonly inputResize?: { readonly width: number; readonly height: number; readonly crop: 'disabled' | 'center'; readonly pad?: { readonly left: number; readonly right: number; readonly top: number; readonly bottom: number } };
  /** Load `checkpoint` as a diffusion model (UNETLoader) with its own text encoder (CLIPLoader type) and VAE. */
  readonly diffusion?: { readonly textEncoder: string; readonly textEncoder2?: string; readonly clipType: string; readonly vae: string; /** Family id when several families share a CLIP type (Chroma / Chroma Radiance). */ readonly family?: string };
  /** Family id for checkpoint-loader families whose recipe points (latent, patch) differ from the plain graph. */
  readonly family?: string;
  /** MiniMax H3 image edit (native compiler only): sigma shifts; the native graph samples the source as the first frame of a 5-frame pass. */
  readonly h3Edit?: { readonly shiftVideo: number; readonly shiftAudio: number };
  /** SDXL ReVision (checkpoint-loader families only): a reference picture from the backend input folder, encoded by a CLIP-vision model and blended into the positive conditioning at `strength`. */
  readonly imageReference?: { readonly visionModel: string; readonly images: readonly { readonly image: string; readonly strength: number }[] };
  /** Family-native conditioning; ordered references, no CLIP-vision model or strength. */
  readonly nativeImageReference?: { readonly method: 'qwen-image-2.1' | 'krea-2' | 'moss-image'; readonly images: readonly string[] };
  /** Workbench Options: model patches (one attention backend, step cache, torch.compile) and tiled VAE. */
  readonly options?: RunOptions;
  /** Model-fixed schedule (steps + 1 values, descending to 0): ManualSigmas → SamplerCustomAdvanced replaces KSampler; `scheduler` is then unused. */
  readonly sigmas?: readonly number[];
  /** Optional original DaSiWa preset/wildcard plugin, prepended by the manual prompts. */
  readonly wildcard?: WildcardRules;
  /** Image-to-image: upscale model run on the source before `inputResize`, which then sets the exact size. */
  readonly upscaleModel?: string;
  /** Post pass after the first decode (SwarmUI refiner/upscale): optional upscale model, exact resize, second sampling at `denoise` with the same model, prompts and seed; `sigmas` is required exactly when the first pass has them. */
  readonly refine?: { readonly width: number; readonly height: number; readonly denoise: number; readonly upscaleModel?: string; readonly sigmas?: readonly number[];
    /** Sample the refine pass in overlapping latent tiles (DaSiWa Tiled Diffusion): tile = the model's native size, overlap blended per step. */
    readonly tiled?: { readonly tileWidth: number; readonly tileHeight: number; readonly overlap: number } };
  /** Options → Output: DaSiWa saver file format and 0–100 compression (PNG level / WebP quality); core SaveImage ignores them. */
  readonly imageFormat?: 'png' | 'webp';
  readonly imageCompression?: number;
}
/** Native conditioning node shared by recipe emission and Studio prerequisite checks. */
export const nativeImageReferenceEncoders = Object.freeze({
  'qwen-image-2.1': 'TextEncodeQwenImage21',
  'krea-2': 'DEAS_ImagePromptEncode',
  'moss-image': 'DEAS_ImagePromptEncode',
} as const);

export interface RunOptions {
  /** `ModelAttentionBackend` choice; exclusive with `sage`. */
  readonly attention?: string;
  /** `PathchSageAttentionKJ` mode (KJNodes). */
  readonly sage?: string;
  readonly cache?: { readonly node: 'EasyCache' | 'LazyCache'; readonly threshold: number };
  /** `TorchCompileModel` backend. */
  readonly compile?: string;
  /** Tile edge for VAEDecodeTiled/VAEEncodeTiled, 256–4096 in steps of 64. */
  readonly vaeTile?: number;
}

/** Supply fresh, verified backend identities. This pure compiler performs no discovery. */
export const isGguf = (name: string) => /\.gguf$/i.test(name);
export const clipLoaderName = (name: string, dual = false) => `${dual ? 'DualCLIPLoader' : 'CLIPLoader'}${isGguf(name) ? 'GGUF' : ''}`;
export interface RecipeAllowlist {
  readonly videoPreviews?: Record<string, import('./videoPreview').VideoPreviewConfig>;
  readonly nodes: readonly string[];
  readonly checkpoints: readonly string[];
  readonly loras?: readonly string[];
  readonly images?: readonly string[];
  readonly samplers: readonly string[];
  readonly schedulers: readonly string[];
  readonly diffusionModels?: readonly string[];
  readonly ggufDiffusionModels?: readonly string[];
  readonly ggufTextEncoders?: readonly string[];
  readonly ggufClipTypes?: readonly string[];
  readonly ggufDualTextEncoders?: readonly string[];
  readonly ggufDualClipTypes?: readonly string[];
  readonly textEncoders?: readonly string[];
  /** `DualCLIPLoader` combos for two-encoder families (Flux.1, SD3); absent on older backends. */
  readonly dualTextEncoders?: readonly string[];
  readonly dualClipTypes?: readonly string[];
  readonly clipTypes?: readonly string[];
  readonly visionModels?: readonly string[];
  readonly vaes?: readonly string[];
  readonly attention?: readonly string[];
  readonly sage?: readonly string[];
  readonly compile?: readonly string[];
  readonly upscaleModels?: readonly string[];
  readonly latentUpscaleModels?: readonly string[];
  /** Core `FrameInterpolationModelLoader` files (RIFE) and the installed RTX node's upscale modes, for TVE post-processing. */
  readonly frameInterpolationModels?: readonly string[];
  readonly rtxUpscaleModes?: readonly string[];
  readonly h3LatentUpscaleModels?: readonly string[];
  /** `AudioEncoderLoader` choices (SheetSage2 for YuE2 covers); absent on older backends. */
  readonly audioEncoders?: readonly string[];
}
export type ApiLink = readonly [nodeId: string, outputIndex: number];
export interface ApiNode {
  readonly class_type: string;
  readonly inputs: Readonly<Record<string, string | number | boolean | ApiLink>>;
}
export interface CompiledRecipe {
  readonly recipeId: DiagnosticImageIntentV1['recipeId'];
  readonly version: 1;
  readonly graph: Readonly<Record<string, ApiNode>>;
  readonly requiredNodes: readonly string[];
  readonly requiredModels: readonly { readonly kind: 'checkpoint' | 'diffusionModel' | 'textEncoder' | 'vae'; readonly identifier: string }[];
  readonly generationReady: false;
}

export type RecipeErrorCode = 'INVALID_ALLOWLIST' | 'MISSING_CAPABILITY' | 'MISSING_MODEL' | 'INVALID_INTENT' | 'UNKNOWN_RECIPE' | 'UNSUPPORTED_VERSION' | 'INVALID_INPUT';
export class RecipeCompileError extends Error {
  readonly code: RecipeErrorCode;
  readonly field: string;
  readonly remedy: string;
  constructor(code: RecipeErrorCode, field: string, message: string, remedy: string) {
    super(`${field}: ${message}`);
    this.name = 'RecipeCompileError';
    this.code = code;
    this.field = field;
    this.remedy = remedy;
  }
}
function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

/** Bounds are diagnostic resource guards, not a claim of available GPU memory. */
function validateIntent(input: unknown): DiagnosticImageIntentV1 {
  if (!isRecord(input)) throw new RecipeCompileError('INVALID_INTENT', 'intent', 'Expected a domain intent object.', 'Provide the explicit versioned diagnostic text-to-image intent.');
  if (input.recipeId !== 'checkpoint-text-to-image-diagnostic') throw new RecipeCompileError('UNKNOWN_RECIPE', 'recipeId', 'Unknown recipe identity.', 'Use checkpoint-text-to-image-diagnostic; Director/video recipes are not implemented.');
  if (input.version !== 1) throw new RecipeCompileError('UNSUPPORTED_VERSION', 'version', 'Only numeric version 1 is supported.', 'Provide version: 1 without coercion or migrate the original intent explicitly.');
  const fields = ['recipeId', 'version', 'positivePrompt', 'negativePrompt', 'checkpoint', 'seed', 'width', 'height', 'steps', 'cfg', 'sampler', 'scheduler', 'outputPrefix'];
  const invalid = (field: string, message: string, remedy: string): never => {
    throw new RecipeCompileError('INVALID_INPUT', field, message, remedy);
  };
  for (const key of Object.keys(input)) {
    if (!fields.includes(key) && !['loras', 'inputImage', 'denoise', 'diffusion', 'family', 'inputResize', 'inputMask', 'maskBlur', 'inpaintCrop', 'inputLayers', 'options', 'sigmas', 'wildcard', 'upscaleModel', 'refine', 'imageFormat', 'imageCompression', 'imageReference', 'nativeImageReference'].includes(key)) invalid(key, 'Unknown domain intent field.', 'Remove unsupported fields; do not pass widget or canvas state.');
  }
  for (const field of fields) {
    if (!Object.hasOwn(input, field)) invalid(field, 'Missing required intent field.', 'Supply this explicit DEAS-owned intent field; no defaults are inferred.');
  }
  for (const field of ['positivePrompt', 'negativePrompt']) {
    const text = input[field];
    if (typeof text !== 'string' || text.length > 16384 || text.includes('\u0000')) invalid(field, 'Expected a prompt string of at most 16384 characters without NUL.', 'Supply a bounded prompt string; an empty prompt is permitted.');
  }
  // Same grammar as native `model_name`: spaces and non-ASCII letters allowed; no traversal, host paths, URLs or control characters.
  const safeModel = (id: unknown): id is string => typeof id === 'string' && new TextEncoder().encode(id).length <= 255 && id.split('/').every(segment =>
    segment.length > 0 && segment !== '.' && segment !== '..' && segment.trim() === segment && !/[\p{Cc}\\:%]/u.test(segment));
  // ComfyUI model IDs use native separators; validate both without rewriting submitted IDs.
  const safeComfyModel = (id: unknown): id is string => typeof id === 'string' && safeModel(id.replace(/\\/g, '/'));
  if (!safeComfyModel(input.checkpoint)) invalid('checkpoint', 'Unsafe model identifier.', 'Use the exact verified relative identifier, never a host path or URL.');
  if (Object.hasOwn(input, 'inputImage') !== Object.hasOwn(input, 'denoise')) invalid('inputImage', 'inputImage and denoise must be given together.', 'Supply both for image-to-image or neither for text-to-image.');
  if (Object.hasOwn(input, 'inputImage') && !safeModel(input.inputImage)) invalid('inputImage', 'Unsafe input image identifier.', 'Use a file name from the backend input folder.');
  if (Object.hasOwn(input, 'denoise') && !(typeof input.denoise === 'number' && Number.isFinite(input.denoise) && input.denoise >= 0 && input.denoise <= 1)) invalid('denoise', 'Denoise must be a finite number in [0, 1].', 'Supply a denoise strength between 0 and 1.');
  if (Object.hasOwn(input, 'inputMask') && (!Object.hasOwn(input, 'inputImage') || Object.hasOwn(input, 'inputResize')
    || !isUploadedMask(String(input.inputMask)))) invalid('inputMask', 'Invalid inpaint mask.', 'Upload the mask natively and use it with an unresized inputImage.');
  if (Object.hasOwn(input, 'maskBlur') && (!input.inputMask || !Number.isSafeInteger(input.maskBlur) || (input.maskBlur as number) < 0 || (input.maskBlur as number) > 31 || (input.maskBlur as number) >= Math.min(input.width as number,input.height as number))) invalid('maskBlur','Invalid mask blur.','Use an integer radius 0..31 smaller than the source edges, with inputMask.');
  if (Object.hasOwn(input,'inpaintCrop')) {
    const c=input.inpaintCrop;
    if (!input.inputMask || !isRecord(c) || Object.keys(c).length!==6 || !['x','y','width','height','targetWidth','targetHeight'].every(k=>Number.isSafeInteger(c[k]) && (c[k] as number)>=(['x','y'].includes(k)?0:1) && (c[k] as number)<=8192)
      || (c.x as number)+(c.width as number)>(input.width as number) || (c.y as number)+(c.height as number)>(input.height as number)
      || !['targetWidth','targetHeight'].every(k=>(c[k] as number)>=64 && (c[k] as number)%8===0) || (c.targetWidth as number)*(c.targetHeight as number)>16_777_216) invalid('inpaintCrop','Invalid inpaint crop.','Use a bounded source rectangle and model-aligned target dimensions with inputMask.');
  }
  const layers = input.inputLayers;
  if (Object.hasOwn(input, 'inputLayers') && (!Object.hasOwn(input, 'inputImage') || !Array.isArray(layers) || layers.length < 1 || layers.length > 4
    || !layers.every(l => isRecord(l) && Object.keys(l).length === 5 && safeModel(l.image) && ['x', 'y', 'width', 'height'].every(k =>
      Number.isSafeInteger(l[k]) && (l[k] as number) <= 8192 && ((l[k] as number) > 0 || ((k === 'x' || k === 'y') && l[k] === 0)))))) invalid('inputLayers', 'Invalid image layers.', 'Supply 1-4 {image, x, y, width, height} in source pixels on an inputImage.');
  if (Object.hasOwn(input,'wildcard')&&!isWildcardRules(input.wildcard)) invalid('wildcard','Invalid wildcard rules.','Use the original node inputs with bounded selections, weights, seed and token budget.');
  const loras = input.loras ?? [];
  if (!Array.isArray(loras) || loras.length > 8 || !loras.every(l => isRecord(l) && Object.keys(l).length === 2 && safeComfyModel(l.name)
    && typeof l.strength === 'number' && Number.isFinite(l.strength) && Math.abs(l.strength) <= 10)) {
    invalid('loras', 'Invalid LoRA list.', 'Supply at most 8 verified {name, strength in [-10, 10]} entries.');
  }
  for (const [field, min, max, integer, alignment] of [
    ['seed', 0, Number.MAX_SAFE_INTEGER, true, 1],
    ['width', 64, 8192, true, 8], ['height', 64, 8192, true, 8],
    ['steps', 1, 150, true, 1], ['cfg', 0, 30, false, 1],
  ] as const) {
    const value = input[field];
    if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max ||
      (integer && (!Number.isSafeInteger(value) || value % alignment !== 0))) {
      invalid(field, 'Numeric value is out of bounds, unsafe, or unaligned.', `Supply a finite ${integer ? 'safe integer' : 'number'} in [${min}, ${max}]${alignment > 1 ? ` aligned to ${alignment}` : ''}.`);
    }
  }
  if ((input.width as number) * (input.height as number) > 16_777_216) invalid('width', 'Pixel limit exceeded.', 'Use at most 16,777,216 output pixels.');
  if (Object.hasOwn(input, 'inputResize')) {
    const r = input.inputResize;
    const aligned = (v: unknown, min: number) => typeof v === 'number' && Number.isSafeInteger(v) && v >= min && v <= 8192 && v % 8 === 0;
    if (!Object.hasOwn(input, 'inputImage') || !isRecord(r) || ![3, 4].includes(Object.keys(r).length)
      || Object.keys(r).some(k => !['width', 'height', 'crop', 'pad'].includes(k)) || !['width', 'height', 'crop'].every(k => Object.hasOwn(r, k))
      || !aligned(r.width, 8) || !aligned(r.height, 8) || !['disabled', 'center'].includes(r.crop as string)) {
      invalid('inputResize', 'Invalid image resize.', 'Supply inputImage + denoise and {width, height, crop} with positive dimensions aligned to 8.');
    }
    const resize = r as Record<string, unknown>;
    const pad = resize.pad;
    if (Object.hasOwn(resize, 'pad') && (!isRecord(pad) || Object.keys(pad).length !== 4 || !['left', 'right', 'top', 'bottom'].every(k => Object.hasOwn(pad, k) && aligned(pad[k], 0)))) {
      invalid('inputResize', 'Invalid padding.', 'Supply exactly four nonnegative pads aligned to 8, at most 8192 each.');
    }
    const pads = pad as Record<string, number> | undefined;
    if ((resize.width as number) + (pads?.left ?? 0) + (pads?.right ?? 0) !== input.width
      || (resize.height as number) + (pads?.top ?? 0) + (pads?.bottom ?? 0) !== input.height) {
      invalid('inputResize', 'Resize plus padding must match output dimensions.', 'Use resize dimensions plus pads equal to intent width and height.');
    }
  }
  const safeChoice = (v: unknown) => typeof v === 'string' && /^[A-Za-z0-9_.-]{1,64}$/u.test(v);
  for (const field of ['sampler', 'scheduler']) {
    if (!safeChoice(input[field])) invalid(field, 'Unsafe recipe enum.', `Choose a ${field} the backend lists.`);
  }
  const d = input.diffusion;
  if (d !== undefined && !(isRecord(d) && [3, 4, 5].includes(Object.keys(d).length) && Object.keys(d).every(k => ['textEncoder', 'textEncoder2', 'clipType', 'vae', 'family'].includes(k))
    && safeComfyModel(d.textEncoder) && safeComfyModel(d.vae) && safeChoice(d.clipType)
    && (d.family === undefined || safeChoice(d.family))
    && (d.textEncoder2 === undefined || (safeComfyModel(d.textEncoder2) && d.textEncoder2 !== d.textEncoder)))) {
    invalid('diffusion', 'Invalid diffusion loader.', 'Supply {textEncoder, clipType, vae}, an optional textEncoder2 for dual encoders and an optional family id.');
  }
  if (input.family !== undefined && !safeChoice(input.family)) invalid('family', 'Unsafe family id.', 'Pass the bundled family id exactly (e.g. lumina2).');
  if (isRecord(d) && d.clipType === 'moss_image') {
    const multiple = families.find(f => f.id === 'moss-image')!.multiple!;
    if ((input.width as number) % multiple !== 0 || (input.height as number) % multiple !== 0) invalid('dimensions', `MOSS-Image dimensions must be multiples of ${multiple}.`, 'Use the MOSS-Image canvas grid.');
    if (['inputLayers', 'refine', 'sigmas', 'upscaleModel'].some(k => Object.hasOwn(input, k))
      || (Array.isArray(input.loras) && input.loras.length > 0)
      || (isRecord(input.options) && Object.keys(input.options).length > 0)
      || (isRecord(input.inputResize) && Object.hasOwn(input.inputResize, 'pad'))) {
      invalid('diffusion', 'MOSS-Image supports plain generation, VAE image-to-image and masked inpaint.', 'Disable unverified LoRAs, layers, refine, custom sigmas, padding and model options.');
    }
  }
  // Same grammar as native `safe_option`: option enums may carry spaces and `+` (`pytorch attention`, `…fp8_cuda++`).
  const safeOption = (v: unknown) => typeof v === 'string' && /^[A-Za-z0-9 _.+-]{1,64}$/u.test(v) && v.trim() === v;
  const o = input.options;
  if (o !== undefined) {
    const c = isRecord(o) ? o.cache : undefined;
    if (!isRecord(o) || Object.keys(o).some(k => !['attention', 'sage', 'cache', 'compile', 'vaeTile'].includes(k))
      || ['attention', 'sage', 'compile'].some(k => Object.hasOwn(o, k) && !safeOption(o[k]))
      || (Object.hasOwn(o, 'attention') && Object.hasOwn(o, 'sage'))
      || (c !== undefined && !(isRecord(c) && Object.keys(c).length === 2 && ['EasyCache', 'LazyCache'].includes(c.node as string)
        && typeof c.threshold === 'number' && Number.isFinite(c.threshold) && c.threshold >= 0 && c.threshold <= 3))
      || (Object.hasOwn(o, 'vaeTile') && !(Number.isSafeInteger(o.vaeTile) && (o.vaeTile as number) >= 256 && (o.vaeTile as number) <= 4096 && (o.vaeTile as number) % 64 === 0))) {
      invalid('options', 'Invalid options.', 'Use one attention backend, listed choices, a cache threshold in [0, 3] and a VAE tile of 256–4096 in steps of 64.');
    }
  }
  const schedule = (s: unknown): s is number[] => Array.isArray(s) && s.length >= 2 && s.length <= 151 && s.every(v => typeof v === 'number' && Number.isFinite(v) && v >= 0 && v <= 1000)
    && s.every((v, n) => n === 0 || v < s[n - 1]) && s[s.length - 1] === 0;
  const s = input.sigmas;
  if (s !== undefined && !(schedule(s) && s.length === (input.steps as number) + 1)) {
    invalid('sigmas', 'Invalid sigmas.', 'Supply steps + 1 finite values, strictly descending and ending at 0.');
  }
  if (Object.hasOwn(input, 'imageFormat') && !['png', 'webp'].includes(input.imageFormat as string)) invalid('imageFormat', 'Invalid image format.', 'Use png or webp.');
  if (Object.hasOwn(input, 'imageCompression') && !(Number.isInteger(input.imageCompression) && (input.imageCompression as number) >= 0 && (input.imageCompression as number) <= 100)) invalid('imageCompression', 'Invalid image compression.', 'Use an integer from 0 to 100.');
  if (Object.hasOwn(input, 'upscaleModel') && (!Object.hasOwn(input, 'inputResize') || !safeComfyModel(input.upscaleModel))) invalid('upscaleModel', 'Invalid upscale model.', 'Use a listed upscale model together with inputResize.');
  if (Object.hasOwn(input, 'refine')) {
    const r = input.refine;
    const edge = (v: unknown) => Number.isSafeInteger(v) && (v as number) >= 64 && (v as number) <= 8192 && (v as number) % 8 === 0;
    const tiledOK = (v: unknown) => isRecord(v) && Object.keys(v).length === 3 && edge(v.tileWidth) && edge(v.tileHeight) && Number.isSafeInteger(v.overlap) && (v.overlap as number) >= 0 && (v.overlap as number) % 8 === 0 && (v.overlap as number) < Math.min(v.tileWidth as number, v.tileHeight as number);
    if (!isRecord(r) || Object.keys(r).some(k => !['width', 'height', 'denoise', 'upscaleModel', 'sigmas', 'tiled'].includes(k)) || !edge(r.width) || !edge(r.height) || (Object.hasOwn(r, 'tiled') && !tiledOK(r.tiled))
      || (r.width as number) * (r.height as number) > 16_777_216 || !(typeof r.denoise === 'number' && r.denoise > 0 && r.denoise <= 1)
      || (Object.hasOwn(r, 'upscaleModel') && !safeComfyModel(r.upscaleModel)) || Object.hasOwn(r, 'sigmas') !== (s !== undefined) || (Object.hasOwn(r, 'sigmas') && !schedule(r.sigmas))) {
      invalid('refine', 'Invalid refine pass.', 'Supply aligned width/height (at most 16,777,216 pixels), denoise in (0, 1], an optional listed upscale model, sigmas exactly when the first pass has them, and tiled {tileWidth, tileHeight, overlap} aligned to 8 with overlap below the tile.');
    }
  }
  if (Object.hasOwn(input, 'imageReference')) {
    const r = input.imageReference;
    if (input.diffusion !== undefined
      || !isRecord(r) || Object.keys(r).length !== 2
      || !safeComfyModel(r.visionModel)
      || !Array.isArray(r.images) || r.images.length < 1 || r.images.length > 4
      || r.images.some(e => !isRecord(e) || Object.keys(e).length !== 2 || !safeModel(e.image)
        || !(typeof e.strength === 'number' && Number.isFinite(e.strength) && e.strength >= 0 && e.strength <= 1))) {
      invalid('imageReference', 'Invalid image reference.', 'Supply {visionModel, images: [{image, strength in [0, 1]}] x 1..4} on a checkpoint (SDXL) family; split-loader families use their native image paths (input image, video references).');
    }
  }
  if (Object.hasOwn(input, 'nativeImageReference')) {
    const r = input.nativeImageReference;
    if (Object.hasOwn(input, 'imageReference') || !isRecord(d)
      || !isRecord(r) || Object.keys(r).length !== 2
      || !((r.method === 'qwen-image-2.1' && d.clipType === 'qwen_image')
        || (r.method === 'krea-2' && d.clipType === 'krea2') || (r.method === 'moss-image' && d.clipType === 'moss_image'))
      || !Array.isArray(r.images) || r.images.length < 1 || r.images.length > 4 || !Array.from(r.images).every(safeModel)
      || (r.method === 'qwen-image-2.1' && ((input.width as number) % 32 !== 0 || (input.height as number) % 32 !== 0))) {
      invalid('nativeImageReference', 'Invalid native image reference.', 'Use 1..4 safe image names and a method matching the diffusion encoder: qwen-image-2.1/qwen_image, krea-2/krea2 or moss-image/moss_image. Qwen dimensions must be aligned to 32.');
    }
  }
  const prefix = input.outputPrefix;
  if (!safeOutputPrefix(prefix) || (input.refine !== undefined && !safeOutputPrefix(`${prefix}_original`))) invalid('outputPrefix', 'Unsafe output prefix.', 'Use a relative output path of at most 512 ASCII characters, with safe segments of at most 180 characters, including the _original suffix when refining.');
  return input as unknown as DiagnosticImageIntentV1;
}

/** Pure compilation only: allowlists must be verified by the caller, not user declarations. */
export function compileRecipe(input: unknown, verified: unknown): CompiledRecipe {
  const intent = validateIntent(input);
  if (!isRecord(verified) || !['nodes', 'checkpoints', 'samplers', 'schedulers'].every(key =>
    Object.hasOwn(verified, key) && Array.isArray(verified[key]) && Array.from(verified[key]).every((v: unknown) => typeof v === 'string' && v.length > 0))) {
    throw new RecipeCompileError('INVALID_ALLOWLIST', 'allowlist', 'Explicit capability/model lists are required.', 'Supply all four string arrays from a verified backend probe.');
  }
  const allowlist = verified as unknown as RecipeAllowlist;
  if (intent.wildcard&&!allowlist.nodes.includes(wildcardNode)) throw new RecipeCompileError('MISSING_CAPABILITY',wildcardNode,'Wildcard plugin is unavailable.','Install the DaSiWa node pack or disable wildcards.');
  const d = intent.diffusion;
  // Family-declared recipe points (latent node, zeroed negative, sampling patch); unknown architectures keep the plain graph.
  const recipeList = families as unknown as { id?: string; clipType?: string; temporalLatent?: boolean; latent?: string; negative?: string; patch?: { node: string; shift?: number }; sigmaNode?: string; encodeNode?: string }[];
  const recipe = recipeList.find(f => f.id === intent.family) ?? recipeList.find(f => f.id === d?.family) ?? recipeList.find(f => d !== undefined && f.clipType === d.clipType);
  if (recipe?.temporalLatent && intent.refine?.tiled) throw new RecipeCompileError('INVALID_INTENT', 'refine.tiled', 'Tiled diffusion does not support temporal latent families.', 'Disable tiled refine for the selected family.');
  const loras = intent.loras ?? [];
  if (isGguf(intent.checkpoint) && !d) throw new RecipeCompileError('INVALID_INTENT','checkpoint','GGUF requires a diffusion recipe with separate text encoder and VAE.','Select a compatible split-model family.');
  if (d && isGguf(d.vae)) throw new RecipeCompileError('INVALID_INTENT','diffusion.vae','GGUF VAE is unsupported.','Select a supported VAE file.');
  if (loras.some(l => !/\.safetensors$/i.test(l.name))) throw new RecipeCompileError('INVALID_INTENT','loras','LoRA files must be safetensors; GGUF LoRA is unsupported.','Select a safetensors LoRA.');
  if (isGguf(intent.checkpoint) && loras.length) throw new RecipeCompileError('INVALID_INTENT','loras','LoRA on a GGUF base is not proven.','Remove the LoRA or select a non-GGUF base.');
  const o = intent.options ?? {};
  const tiled = o.vaeTile !== undefined;
  const patches: (readonly [id: string, node: string, inputs: Record<string, string | number | boolean>])[] = [
    ...(o.attention !== undefined ? [['A', 'ModelAttentionBackend', { attention: o.attention }] as const] : []),
    ...(o.sage !== undefined ? [['K', 'PathchSageAttentionKJ', { sage_attention: o.sage }] as const] : []),
    // start/end are the nodes' own defaults.
    ...(o.cache ? [['E', o.cache.node, { reuse_threshold: o.cache.threshold, start_percent: 0.15, end_percent: 0.95, verbose: false }] as const] : []),
    ...(o.compile !== undefined ? [['T', 'TorchCompileModel', { backend: o.compile }] as const] : []),
  ];
  for (const [field, value] of [['attention', o.attention], ['sage', o.sage], ['compile', o.compile]] as const) {
    if (value !== undefined && !allowlist[field]?.includes(value)) throw new RecipeCompileError('MISSING_CAPABILITY', `options.${field}`, 'Choice is not verified on this backend.', 'Select a choice the backend lists.');
  }
  for (const node of [...patches.map(p => p[1]), ...(tiled ? ['VAEDecodeTiled', ...(intent.inputImage !== undefined || intent.refine ? ['VAEEncodeTiled'] : [])] : [])]) {
    if (!allowlist.nodes.includes(node)) throw new RecipeCompileError('MISSING_CAPABILITY', node, 'Option node is unavailable.', 'Install the node pack or turn the option off.');
  }
  // LoraLoaders chain after the loaders; the last one feeds the sampler and both encoders.
  const dual = d?.textEncoder2;
  const chain: Record<string, ApiNode> = d ? {
    C: dual
      ? { class_type: isGguf(d.textEncoder) || isGguf(dual) ? 'DualCLIPLoaderGGUF' : 'DualCLIPLoader', inputs: { clip_name1: d.textEncoder, clip_name2: dual, type: d.clipType } }
      : { class_type: clipLoaderName(d.textEncoder), inputs: { clip_name: d.textEncoder, type: d.clipType } },
    V: { class_type: 'VAELoader', inputs: { vae_name: d.vae } },
  } : {};
  let model: ApiLink = ['1', 0], clip: ApiLink = d ? ['C', 0] : ['1', 1];
  const vae: ApiLink = d ? ['V', 0] : ['1', 2];
  loras.forEach((lora, n) => {
    const id = `L${n + 1}`;
    chain[id] = { class_type: 'LoraLoader', inputs: { model, clip, lora_name: lora.name, strength_model: lora.strength, strength_clip: lora.strength } };
    model = [id, 0]; clip = [id, 1];
  });
  // Patches chain after the LoRAs; compile last so it sees every other patch.
  for (const [id, node, inputs] of patches) {
    chain[id] = { class_type: node, inputs: { ...inputs, model } };
    model = [id, 0];
  }
  // Family-declared sampling patch chains after every model patch, so it sees the final model.
  if (recipe?.patch) {
    chain.M = { class_type: recipe.patch.node, inputs: { shift: recipe.patch.shift ?? 1, model } };
    model = ['M', 0];
  }
  // Spatial tiles; the temporal values are the node defaults (used by video VAEs only).
  const tiles: Record<string, number> = tiled ? { tile_size: o.vaeTile!, overlap: 64, temporal_size: 64, temporal_overlap: 8 } : {};
  // Mage-Flow encodes prompt, negative and latent in one node (outputs positive/negative/latent).
  const enc = recipe?.encodeNode;
  const negative: ApiLink = enc ? ['2', 1] : ['3', 0];
  const latent: ApiLink = enc ? ['2', 2] : ['4', 0];
  const result: CompiledRecipe = {
    recipeId: intent.recipeId, version: 1, generationReady: false,
    requiredNodes: [],
    requiredModels: d ? [{ kind: 'diffusionModel', identifier: intent.checkpoint }, { kind: 'textEncoder', identifier: d.textEncoder }, { kind: 'vae', identifier: d.vae }] : [{ kind: 'checkpoint', identifier: intent.checkpoint }],
    graph: {
      '1': d ? { class_type: isGguf(intent.checkpoint) ? 'UnetLoaderGGUF' : 'UNETLoader', inputs: { unet_name: intent.checkpoint, ...(!isGguf(intent.checkpoint) ? { weight_dtype: 'default' } : {}) } } : { class_type: 'CheckpointLoaderSimple', inputs: { ckpt_name: intent.checkpoint } },
      '2': enc
        ? { class_type: enc, inputs: { clip, prompt: intent.positivePrompt, negative_prompt: intent.negativePrompt, width: intent.width, height: intent.height, batch_size: 1 } }
        : { class_type: 'CLIPTextEncode', inputs: { text: intent.positivePrompt, clip } },
      ...(enc ? {} : {
        '3': recipe?.negative === 'zero'
          ? { class_type: 'ConditioningZeroOut', inputs: { conditioning: ['2', 0] } }
          : { class_type: 'CLIPTextEncode', inputs: { text: intent.negativePrompt, clip } },
        '4': { class_type: recipe?.latent ?? 'EmptyLatentImage', inputs: { width: intent.width, height: intent.height, batch_size: 1 } },
      }),
      '5': { class_type: 'KSampler', inputs: { model, positive: ['2', 0], negative, latent_image: latent, seed: intent.seed, steps: intent.steps, cfg: intent.cfg, sampler_name: intent.sampler, scheduler: intent.scheduler, denoise: 1 } },
      '6': { class_type: tiled ? 'VAEDecodeTiled' : 'VAEDecode', inputs: { samples: ['5', 0], vae, ...tiles } },
      '7': { class_type: 'SaveImage', inputs: { images: ['6', 0], filename_prefix: intent.outputPrefix } },
      ...chain,
      ...(intent.inputImage !== undefined ? { '8': { class_type: 'LoadImage', inputs: { image: intent.inputImage } }, '9': { class_type: 'VAEEncode', inputs: { pixels: ['8', 0], vae } } } : {}),
    },
  };
  // Match KSampler's denoise schedule while keeping the requested number of sampler steps.
  const inputDenoise=intent.denoise??1;
  const scheduleSteps=intent.inputImage&&inputDenoise>0&&inputDenoise<1?Math.floor(intent.steps/inputDenoise):intent.steps;
  if(recipe?.sigmaNode&&!intent.sigmas&&scheduleSteps>10000)throw new RecipeCompileError('INVALID_INPUT','denoise','Derived scheduler steps exceed 10000.','Increase denoise or reduce steps.');
  const scheduleSize=intent.inpaintCrop?{width:intent.inpaintCrop.targetWidth,height:intent.inpaintCrop.targetHeight}:intent.inputResize??intent;
  // Model-fixed schedule: explicit sigmas (ManualSigmas) or a family-declared scheduler node (Flux.2's Flux2Scheduler).
  const schedule: ApiNode | undefined = intent.sigmas
    ? { class_type: 'ManualSigmas', inputs: { sigmas: intent.sigmas.join(', ') } }
    : recipe?.sigmaNode ? { class_type: recipe.sigmaNode, inputs: { steps: scheduleSteps, width: scheduleSize.width, height: scheduleSize.height } } : undefined;
  if (schedule) {
    const graph = result.graph as Record<string, ApiNode>;
    graph.N = { class_type: 'RandomNoise', inputs: { noise_seed: intent.seed } };
    graph.G = { class_type: 'CFGGuider', inputs: { model, positive: ['2', 0], negative: ['3', 0], cfg: intent.cfg } };
    graph.Q = { class_type: 'KSamplerSelect', inputs: { sampler_name: intent.sampler } };
    graph.Z = schedule;
    graph['5'] = { class_type: 'SamplerCustomAdvanced', inputs: { noise: ['N', 0], guider: ['G', 0], sampler: ['Q', 0], sigmas: ['Z', 0], latent_image: latent } };
    if(recipe?.sigmaNode&&!intent.sigmas&&intent.inputImage&&inputDenoise<1){graph.ZD={class_type:'SplitSigmas',inputs:{sigmas:['Z',0],step:intent.denoise===0?intent.steps:scheduleSteps-intent.steps}};graph['5']={...graph['5'],inputs:{...graph['5'].inputs,sigmas:['ZD',1]}};}
  }
  if (intent.imageReference) {
    // Core ReVision chain: each reference picture's CLIP-vision features blended into the positive conditioning.
    const graph = result.graph as Record<string, ApiNode>;
    graph.CV = { class_type: 'CLIPVisionLoader', inputs: { clip_name: intent.imageReference.visionModel } };
    let cond: [string, number] = ['2', 0];
    intent.imageReference.images.forEach((ref, i) => {
      graph[`RI${i}`] = { class_type: 'LoadImage', inputs: { image: ref.image } };
      graph[`CE${i}`] = { class_type: 'CLIPVisionEncode', inputs: { clip_vision: ['CV', 0], image: [`RI${i}`, 0] } };
      graph[`UC${i}`] = { class_type: 'unCLIPConditioning', inputs: { conditioning: cond, clip_vision_output: [`CE${i}`, 0], strength: ref.strength, noise_augmentation: 0 } };
      cond = [`UC${i}`, 0];
    });
    for (const [id, node] of Object.entries(graph)) {
      const p = node.inputs.positive;
      if (Array.isArray(p) && p[0] === '2' && p[1] === 0) graph[id] = { ...node, inputs: { ...node.inputs, positive: cond } };
    }
  }
  if (intent.inputImage !== undefined) {
    const graph = result.graph as Record<string, ApiNode>;
    const r = intent.inputResize;
    // Layers are pasted in source pixels, so they come before any resize and stay under an inpaint composite.
    // shortcut: pasted opaque (alpha ignored); feed LoadImage's inverted mask when cut-outs matter.
    let base: ApiLink = ['8', 0];
    (intent.inputLayers ?? []).forEach((l, n) => {
      graph[`I${n}`] = { class_type: 'LoadImage', inputs: { image: l.image } };
      graph[`R${n}`] = { class_type: 'ImageScale', inputs: { image: [`I${n}`, 0], upscale_method: 'lanczos', width: l.width, height: l.height, crop: 'disabled' } };
      graph[`B${n}`] = { class_type: 'ImageCompositeMasked', inputs: { destination: base, source: [`R${n}`, 0], x: l.x, y: l.y, resize_source: false } };
      base = [`B${n}`, 0];
    });
    graph['9'] = { class_type: 'VAEEncode', inputs: { pixels: base, vae } };
    if (r) {
      let scaled = base;
      if (intent.upscaleModel) {
        graph.UL = { class_type: 'UpscaleModelLoader', inputs: { model_name: intent.upscaleModel } };
        graph.UM = { class_type: 'ImageUpscaleWithModel', inputs: { upscale_model: ['UL', 0], image: base } };
        scaled = ['UM', 0];
      }
      graph.S = { class_type: 'ImageScale', inputs: { image: scaled, upscale_method: 'lanczos', width: r.width, height: r.height, crop: r.crop } };
      if (r.pad) graph.P = { class_type: 'ImagePadForOutpaint', inputs: { image: ['S', 0], ...r.pad, feathering: 0 } };
      graph['9'] = { class_type: 'VAEEncode', inputs: { pixels: [r.pad ? 'P' : 'S', 0], vae } };
    }
    if (tiled) graph['9'] = { class_type: 'VAEEncodeTiled', inputs: { ...graph['9'].inputs, ...tiles } };
    delete graph['4'];
    graph['5'] = { ...graph['5'], inputs: { ...graph['5'].inputs, latent_image: ['9', 0], ...(intent.sigmas ? {} : { denoise: intent.denoise! }) } };
    if (intent.inputMask) {
      // Only the painted area is resampled, then composited onto the untouched source pixels.
      graph.W = { class_type: 'LoadImageMask', inputs: { image: intent.inputMask, channel: 'red' } };
      let mask:ApiLink=['W',0],noiseMask:ApiLink=mask,decoded:ApiLink=['6',0];
      if(intent.maskBlur){
        graph.MI={class_type:'MaskToImage',inputs:{mask}};
        graph.MB={class_type:'ImageBlur',inputs:{image:['MI',0],blur_radius:intent.maskBlur,sigma:1}};
        graph.BM={class_type:'ImageToMask',inputs:{image:['MB',0],channel:'red'}};
        mask=['BM',0];noiseMask=mask;
      }
      const c=intent.inpaintCrop;
      if(c){
        const {x,y,width,height,targetWidth,targetHeight}=c;
        graph.IC={class_type:'ImageCrop',inputs:{image:base,x,y,width,height}};
        graph.IS={class_type:'ImageScale',inputs:{image:['IC',0],upscale_method:'lanczos',width:targetWidth,height:targetHeight,crop:'disabled'}};
        graph['9']={...graph['9'],inputs:{...graph['9'].inputs,pixels:['IS',0]}};
        graph.MC={class_type:'CropMask',inputs:{mask,x,y,width,height}};
        graph.CI={class_type:'MaskToImage',inputs:{mask:['MC',0]}};
        graph.MS={class_type:'ImageScale',inputs:{image:['CI',0],upscale_method:'bilinear',width:targetWidth,height:targetHeight,crop:'disabled'}};
        graph.NM={class_type:'ImageToMask',inputs:{image:['MS',0],channel:'red'}};
        graph.IR={class_type:'ImageScale',inputs:{image:['6',0],upscale_method:'lanczos',width,height,crop:'disabled'}};
        noiseMask=['NM',0];mask=['MC',0];decoded=['IR',0];
      }
      graph.X = { class_type: 'SetLatentNoiseMask', inputs: { samples: ['9', 0], mask: noiseMask } };
      graph.Y = { class_type: 'ImageCompositeMasked', inputs: { destination: base, source: decoded, x: c?.x??0, y: c?.y??0, resize_source: false, mask } };
      graph['5'] = { ...graph['5'], inputs: { ...graph['5'].inputs, latent_image: ['X', 0] } };
      graph['7'] = { ...graph['7'], inputs: { ...graph['7'].inputs, images: ['Y', 0] } };
    }
  }
  if (intent.refine) {
    const graph = result.graph as Record<string, ApiNode>;
    const r = intent.refine;
    let image = graph['7'].inputs.images as ApiLink;
    graph.B7 = { ...graph['7'], inputs: { ...graph['7'].inputs, filename_prefix: `${intent.outputPrefix}_original` } };
    if (r.upscaleModel) {
      graph.RL = { class_type: 'UpscaleModelLoader', inputs: { model_name: r.upscaleModel } };
      graph.RU = { class_type: 'ImageUpscaleWithModel', inputs: { upscale_model: ['RL', 0], image } };
      image = ['RU', 0];
    }
    // The first pass always ends at width×height; a same-size lanczos pass would only resample.
    if (r.upscaleModel || r.width !== intent.width || r.height !== intent.height) {
      graph.RS = { class_type: 'ImageScale', inputs: { image, upscale_method: 'lanczos', width: r.width, height: r.height, crop: 'disabled' } };
      image = ['RS', 0];
    }
    graph.RE = { class_type: tiled ? 'VAEEncodeTiled' : 'VAEEncode', inputs: { pixels: image, vae, ...tiles } };
    // Tiled: the refine sampler gets the model through the tiling patch (its own guider on the sigmas path); the first pass keeps the plain model.
    let guider: ApiLink = ['G', 0], model = (r.sigmas ? graph.G : graph['5']).inputs.model as ApiLink;
    if (r.tiled) {
      graph.RT = { class_type: 'DaSiWaTiledDiffusion', inputs: { model, tile_width: r.tiled.tileWidth, tile_height: r.tiled.tileHeight, overlap: r.tiled.overlap } };
      model = ['RT', 0];
      if (r.sigmas) { graph.RG = { ...graph.G, inputs: { ...graph.G.inputs, model } }; guider = ['RG', 0]; }
    }
    graph.R5 = r.sigmas
      ? { class_type: 'SamplerCustomAdvanced', inputs: { noise: ['N', 0], guider, sampler: ['Q', 0], sigmas: ['RZ', 0], latent_image: ['RE', 0] } }
      : { class_type: 'KSampler', inputs: { ...graph['5'].inputs, model, latent_image: ['RE', 0], denoise: r.denoise } };
    if (r.sigmas) graph.RZ = { class_type: 'ManualSigmas', inputs: { sigmas: r.sigmas.join(', ') } };
    graph.R6 = { class_type: tiled ? 'VAEDecodeTiled' : 'VAEDecode', inputs: { samples: ['R5', 0], vae, ...tiles } };
    graph.PG = { class_type: 'DEAS_ImagePostprocessGate', inputs: { baseline: ['B7', 0], image: ['R6', 0] } };
    graph['7'] = { ...graph['7'], inputs: { ...graph['7'].inputs, images: ['PG', 0] } };
  }
  // Civitai links resources only by hash from an A1111 `parameters` chunk, which core SaveImage cannot write; the DaSiWa saver (Output package) adds it beside prompt/workflow.
  if (['DaSiWa_MetadataImageSaver', 'DaSiWa_MetadataConfig'].every(node => allowlist.nodes.includes(node))) {
    const graph = result.graph as Record<string, ApiNode>;
    graph.M = { class_type: 'DaSiWa_MetadataConfig', inputs: { save_workflow: true, model_hash: '', node_positive: ['2', 0], node_negative: ['3', 0], node_model: ['1', 0], node_latent: ['5', 0] } };
    for (const id of intent.refine ? ['7', 'B7'] : ['7']) {
      graph[id] = { class_type: 'DaSiWa_MetadataImageSaver', inputs: { images: graph[id].inputs.images, filename_prefix: graph[id].inputs.filename_prefix, file_format: intent.imageFormat ?? 'png', compression: intent.imageCompression ?? 44, save_output: true, metadata_config: ['M', 0] } };
    }
  }
  if (intent.wildcard) {
    const graph=result.graph as Record<string,ApiNode>;
    graph.WP={class_type:wildcardNode,inputs:{...intent.wildcard,selection_state:JSON.stringify(intent.wildcard.selection_state),positive_input:intent.positivePrompt,negative_input:intent.negativePrompt}};
    graph['2']={...graph['2'],inputs:{...graph['2'].inputs,text:['WP',0]}};
    if (graph['3'].class_type === 'CLIPTextEncode') graph['3']={...graph['3'],inputs:{...graph['3'].inputs,text:['WP',1]}};
    if (graph.M) graph.M={...graph.M,inputs:{...graph.M.inputs,text_positive:['WP',0],text_negative:['WP',1]}};
  }
  if (intent.nativeImageReference) {
    const graph = result.graph as Record<string, ApiNode>;
    const qwen = intent.nativeImageReference.method === 'qwen-image-2.1';
    const inputs: Record<string, string | number | boolean | ApiLink> = {
      clip, prompt: graph['2'].inputs.text, negative_prompt: graph['3'].inputs.text,
      ...(qwen ? { vae, resolution: 0 } : {}),
    };
    intent.nativeImageReference.images.forEach((image, n) => {
      graph[`RI${n}`] = { class_type: 'LoadImage', inputs: { image } };
      inputs[`${qwen ? 'images.' : ''}image_${n + 1}`] = [qwen && n === 0 ? 'QR' : `RI${n}`, 0];
    });
    // The first Qwen reference fixes output geometry; remaining references use the node's native /32 rounding.
    if (qwen) graph.QR = { class_type: 'ImageScale', inputs: { image: ['RI0', 0], upscale_method: 'lanczos', width: intent.width, height: intent.height, crop: 'center' } };
    graph.QI = { class_type: nativeImageReferenceEncoders[intent.nativeImageReference.method], inputs };
    if (graph.M) graph.M = { ...graph.M, inputs: { ...graph.M.inputs, text_positive: inputs.prompt, text_negative: inputs.negative_prompt } };
    delete graph['2']; delete graph['3']; if (qwen) delete graph['4'];
    for (const [id, node] of Object.entries(graph)) {
      const rewired = { ...node.inputs };
      for (const [key, value] of Object.entries(rewired)) {
        if (Array.isArray(value) && value[1] === 0) {
          if (value[0] === '2') rewired[key] = ['QI', 0];
          if (value[0] === '3') rewired[key] = ['QI', 1];
          if (qwen && value[0] === '4') rewired[key] = ['QI', 2];
        }
      }
      graph[id] = { ...node, inputs: rewired };
    }
  }
  const emitted = Object.freeze([...new Set(Object.values(result.graph).map(node => node.class_type))]);
  for (const node of emitted) {
    if (!allowlist.nodes.includes(node)) throw new RecipeCompileError('MISSING_CAPABILITY', node, node === 'DEAS_ImagePostprocessGate' ? 'Refine requires the DEAS lazy gate and a compatible post-save saver output; update the owned backend or disable post-processing.' : 'Required built-in node is unavailable.', 'Verify a compatible backend with this built-in node before compiling.');
  }
  if (!(d ? (isGguf(intent.checkpoint) ? allowlist.ggufDiffusionModels : allowlist.diffusionModels) ?? [] : allowlist.checkpoints).includes(intent.checkpoint)) throw new RecipeCompileError('MISSING_MODEL', 'checkpoint', 'Checkpoint is not in the verified model list.', 'Select an exact checkpoint identifier from the verified backend model list.');
  if (d && !((isGguf(d.textEncoder) ? allowlist.ggufTextEncoders : allowlist.textEncoders)?.includes(d.textEncoder) && allowlist.vaes?.includes(d.vae) && (isGguf(d.textEncoder) ? allowlist.ggufClipTypes : allowlist.clipTypes)?.includes(d.clipType))) throw new RecipeCompileError('MISSING_MODEL', 'diffusion', 'Text encoder, VAE or CLIP type is not in the verified backend lists.', 'Select files and a CLIP type the backend lists.');
  if (d?.textEncoder2) {
    const gguf = isGguf(d.textEncoder) || isGguf(d.textEncoder2);
    const encoders = gguf ? allowlist.ggufDualTextEncoders : allowlist.dualTextEncoders;
    const types = gguf ? allowlist.ggufDualClipTypes : allowlist.dualClipTypes;
    if (!encoders?.includes(d.textEncoder) || !encoders.includes(d.textEncoder2) || !types?.includes(d.clipType)) throw new RecipeCompileError('MISSING_MODEL', 'diffusion.textEncoder2', 'Dual text encoders or CLIP type are not in the verified backend lists.', 'Select both encoder files and a CLIP type the backend lists for the dual loader.');
  }
  // object_info lists only the input folder; ComfyUI resolves an `[output]` file itself (containment + existence in LoadImage).
  for (const image of [intent.inputImage, ...(intent.imageReference?.images.map(e => e.image) ?? []), ...(intent.nativeImageReference?.images ?? []), ...(intent.inputLayers ?? []).map(l => l.image)]) if (image !== undefined && !image.endsWith(' [output]') && !isUploadedMask(image) && !allowlist.images?.includes(image)) throw new RecipeCompileError('MISSING_MODEL', 'inputImage', 'Input image is not in the backend input folder list.', 'Pick a file the backend lists in its input folder.');
  if (intent.imageReference && !allowlist.visionModels?.includes(intent.imageReference.visionModel)) throw new RecipeCompileError('MISSING_MODEL', 'visionModel', 'CLIP-vision model is not in the verified vision model list.', 'Install the model in the backend vision folder and reload prerequisites.');
  for (const name of [intent.upscaleModel, intent.refine?.upscaleModel]) if (name !== undefined && !allowlist.upscaleModels?.includes(name)) throw new RecipeCompileError('MISSING_MODEL', 'upscaleModel', 'Upscale model is not in the verified model list.', 'Pick an upscale model the backend lists.');
  for (const lora of loras) {
    if (!allowlist.loras?.includes(lora.name)) throw new RecipeCompileError('MISSING_MODEL', 'loras', 'LoRA is not in the verified model list.', 'Select exact LoRA identifiers from the verified backend model list.');
  }
  for (const [field, available] of [['sampler', allowlist.samplers], ['scheduler', allowlist.schedulers]] as const) {
    if (!available.includes(intent[field])) throw new RecipeCompileError('MISSING_CAPABILITY', field, 'Choice is not verified on this backend.', 'Select a choice available in the verified backend capability list.');
  }
  (result as { requiredNodes: readonly string[] }).requiredNodes = emitted;
  for (const node of Object.values(result.graph)) {
    for (const value of Object.values(node.inputs)) if (Array.isArray(value)) Object.freeze(value);
    Object.freeze(node.inputs);
    Object.freeze(node);
  }
  Object.freeze(result.graph);
  for (const model of result.requiredModels) Object.freeze(model);
  Object.freeze(result.requiredModels);
  return Object.freeze(result);
}
