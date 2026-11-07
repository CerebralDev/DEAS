import test from 'node:test';
import assert from 'node:assert/strict';
import { compileRecipe, RecipeCompileError, type DiagnosticImageIntentV1, type RecipeAllowlist } from '../src/lib/execution.ts';

const intent: DiagnosticImageIntentV1 = {
  recipeId: 'checkpoint-text-to-image-diagnostic', version: 1,
  positivePrompt: 'a quiet forest', negativePrompt: 'blur', checkpoint: 'baseline/model.safetensors',
  seed: 42, width: 512, height: 768, steps: 20, cfg: 7,
  sampler: 'euler', scheduler: 'normal', outputPrefix: 'DEAS_diagnostic',
};
const qwenDiffusion = { textEncoder: 'qwen.safetensors', clipType: 'qwen_image', vae: 'qwen-vae.safetensors' };
const nativeReference = { method: 'qwen-image-2.1', images: ['first.png', 'second.png', 'third.png', 'fourth.png'] };
const allowlist: RecipeAllowlist = {
  nodes: ['CheckpointLoaderSimple', 'CLIPTextEncode', 'EmptyLatentImage', 'KSampler', 'VAEDecode', 'SaveImage'],
  checkpoints: ['baseline/model.safetensors'], samplers: ['euler'], schedulers: ['normal'],
};

const qwenAllowlist: RecipeAllowlist = {
  ...allowlist,
  nodes: ['UNETLoader', 'CLIPLoader', 'VAELoader', 'TextEncodeQwenImage21', 'LoadImage', 'ImageScale', 'KSampler', 'VAEDecode', 'SaveImage'],
  diffusionModels: [intent.checkpoint], textEncoders: [qwenDiffusion.textEncoder], clipTypes: ['qwen_image'], vaes: [qwenDiffusion.vae], images: nativeReference.images,
};

test('Qwen native references encode both prompts in order and use the exact first-reference empty latent', () => {
  const result = compileRecipe({ ...intent, diffusion: qwenDiffusion, nativeImageReference: nativeReference }, qwenAllowlist);
  const g = result.graph;
  assert.deepEqual(g.QI, { class_type: 'TextEncodeQwenImage21', inputs: {
    clip: ['C', 0], vae: ['V', 0], prompt: intent.positivePrompt, negative_prompt: intent.negativePrompt, resolution: 0,
    'images.image_1': ['QR', 0], 'images.image_2': ['RI1', 0], 'images.image_3': ['RI2', 0], 'images.image_4': ['RI3', 0],
  } });
  assert.deepEqual(g.QR, { class_type: 'ImageScale', inputs: { image: ['RI0', 0], upscale_method: 'lanczos', width: 512, height: 768, crop: 'center' } });
  nativeReference.images.forEach((image, n) => assert.deepEqual(g[`RI${n}`], { class_type: 'LoadImage', inputs: { image } }));
  assert.deepEqual(g['5'].inputs.positive, ['QI', 0]);
  assert.deepEqual(g['5'].inputs.negative, ['QI', 1]);
  assert.deepEqual(g['5'].inputs.latent_image, ['QI', 2]);
  for (const id of ['2', '3', '4']) assert.equal(g[id], undefined);
  assert.ok(!result.requiredNodes.includes('CLIPTextEncode'));
  assert.ok(result.requiredNodes.includes('TextEncodeQwenImage21'));
  assert.deepEqual(result.requiredModels, [
    { kind: 'diffusionModel', identifier: intent.checkpoint }, { kind: 'textEncoder', identifier: qwenDiffusion.textEncoder }, { kind: 'vae', identifier: qwenDiffusion.vae },
  ]);
});

test('Qwen native reference trust boundary rejects malformed, mixed, unsupported and unaligned requests', () => {
  const qwen = { ...intent, diffusion: qwenDiffusion, nativeImageReference: nativeReference };
  for (const r of [null, [], {}, { images: ['first.png'] }, { ...nativeReference, method: 'other' },
    { ...nativeReference, images: [] }, { ...nativeReference, images: Array(5).fill('first.png') },
    { ...nativeReference, images: [null] }, { ...nativeReference, images: [{ image: 'first.png', strength: 1 }] },
    { ...nativeReference, images: new Array(1) }, { ...nativeReference, strength: 1 }, { ...nativeReference, visionModel: 'vision' },
    ...['../x', '/x', 'x\\y', 'https://x', 'x%20', 'x\u0000', ' x'].map(image => ({ ...nativeReference, images: [image] })),
  ]) rejects({ ...qwen, nativeImageReference: r }, qwenAllowlist, 'INVALID_INPUT', 'nativeImageReference');
  for (const clipType of ['sdxl', 'qwen_image_edit', 'krea2', 'anima', 'moss_image']) {
    rejects({ ...qwen, diffusion: { ...qwenDiffusion, clipType } }, qwenAllowlist, 'INVALID_INPUT', 'nativeImageReference');
  }
  rejects({ ...qwen, diffusion: undefined }, qwenAllowlist, 'INVALID_INPUT', 'nativeImageReference');
  assert.throws(() => compileRecipe({ ...qwen, imageReference: { visionModel: 'v', images: [{ image: 'first.png', strength: 1 }] } }, qwenAllowlist), RecipeCompileError);
  for (const field of ['width', 'height']) rejects({ ...qwen, [field]: 520 }, qwenAllowlist, 'INVALID_INPUT', 'nativeImageReference');
  for (const node of ['TextEncodeQwenImage21', 'ImageScale', 'LoadImage']) rejects(qwen, { ...qwenAllowlist, nodes: qwenAllowlist.nodes.filter(n => n !== node) }, 'MISSING_CAPABILITY', node);
  for (const image of nativeReference.images) rejects(qwen, { ...qwenAllowlist, images: nativeReference.images.filter(n => n !== image) }, 'MISSING_MODEL', 'inputImage');
  for (const field of ['diffusionModels', 'textEncoders', 'clipTypes', 'vaes']) rejects(qwen, { ...qwenAllowlist, [field]: [] }, 'MISSING_MODEL', field === 'diffusionModels' ? 'checkpoint' : 'diffusion');
  const single = compileRecipe({ ...qwen, nativeImageReference: { ...nativeReference, images: ['first.png'] } }, qwenAllowlist).graph;
  assert.equal(single.QI.inputs['images.image_2'], undefined);
});

test('Qwen native references preserve i2i denoise and route wildcard conditioning through sigmas and both refiners', () => {
  const nodes = [...qwenAllowlist.nodes, 'DEAS_ImagePostprocessGate', 'VAEEncode', 'RandomNoise', 'CFGGuider', 'KSamplerSelect', 'ManualSigmas', 'SamplerCustomAdvanced', 'DaSiWaTiledDiffusion', 'DaSiWa_WildcardPresetPromptBuilder', 'LoraLoader'];
  for (const custom of [false, true]) {
    const qwen = { ...intent, diffusion: qwenDiffusion, nativeImageReference: nativeReference, inputImage: 'first.png', denoise: 0.35,
      loras: [{ name: 'style.safetensors', strength: 0.5 }],
      wildcard: { style: 'Booru', seed: 7, reroll: 3, token_budget: 200, auto_reroll: true, selection_state: {} },
      ...(custom ? { steps: 2, sigmas: [1, 0.5, 0] } : {}),
      refine: { width: 1024, height: 1536, denoise: 0.4, ...(custom ? { sigmas: [0.5, 0] } : {}), tiled: { tileWidth: 512, tileHeight: 768, overlap: 64 } },
    };
    const g = compileRecipe(qwen, { ...qwenAllowlist, nodes, loras: ['style.safetensors'] }).graph;
    assert.deepEqual(g.QI.inputs.prompt, ['WP', 0]);
    assert.deepEqual(g.QI.inputs.negative_prompt, ['WP', 1]);
    assert.deepEqual(g.QI.inputs.clip, ['L1', 1]);
    assert.deepEqual(g['5'].inputs.latent_image, ['9', 0]);
    assert.deepEqual(g['9'].inputs, { pixels: ['8', 0], vae: ['V', 0] });
    assert.equal(g['5'].inputs.denoise, custom ? undefined : 0.35);
    for (const id of custom ? ['G', 'RG'] : ['5', 'R5']) {
      assert.deepEqual(g[id].inputs.positive, ['QI', 0]);
      assert.deepEqual(g[id].inputs.negative, ['QI', 1]);
    }
    assert.deepEqual(g.R5.inputs.latent_image, ['RE', 0]);
    assert.ok(!Object.values(g).some(n => ['CLIPTextEncode', 'EmptyLatentImage'].includes(n.class_type)));
  }
});

test('Qwen native metadata has no dangling text encoders and keeps resolved prompt text', () => {
  const g = compileRecipe({ ...intent, diffusion: qwenDiffusion, nativeImageReference: nativeReference }, { ...qwenAllowlist, nodes: [...qwenAllowlist.nodes, 'DaSiWa_MetadataImageSaver', 'DaSiWa_MetadataConfig'] }).graph;
  assert.deepEqual(g.M.inputs.node_positive, ['QI', 0]);
  assert.deepEqual(g.M.inputs.node_negative, ['QI', 1]);
  assert.equal(g.M.inputs.text_positive, intent.positivePrompt);
  assert.equal(g.M.inputs.text_negative, intent.negativePrompt);
});

test('Krea and MOSS native references use the adapter without changing their latent or geometry', () => {
  for (const [method, clipType, width, latent] of [['krea-2', 'krea2', 520, 'EmptyLatentImage'], ['moss-image', 'moss_image', 528, 'EmptyFlux2LatentImage']] as const) {
    const d = { ...qwenDiffusion, clipType };
    const a = { ...qwenAllowlist, clipTypes: [clipType], nodes: [...qwenAllowlist.nodes.filter(n => !['TextEncodeQwenImage21', 'ImageScale'].includes(n)), 'DEAS_ImagePromptEncode', latent, 'VAEEncode', 'CLIPTextEncode'] };
    for (const i2i of [false, true]) {
      const input = { ...intent, width, diffusion: d, ...(i2i ? { inputImage: 'first.png', denoise: 0.4 } : {}) };
      const before = compileRecipe(input, a).graph;
      const compiled = compileRecipe({ ...input, nativeImageReference: { method, images: nativeReference.images } }, { ...a, nodes: a.nodes.filter(n => n !== 'CLIPTextEncode') });
      const after = compiled.graph;
      assert.ok(compiled.requiredNodes.includes('DEAS_ImagePromptEncode'));
      assert.ok(!compiled.requiredNodes.includes('CLIPTextEncode'));
      assert.deepEqual(after.QI, { class_type: 'DEAS_ImagePromptEncode', inputs: {
        clip: ['C', 0], prompt: intent.positivePrompt, negative_prompt: intent.negativePrompt,
        image_1: ['RI0', 0], image_2: ['RI1', 0], image_3: ['RI2', 0], image_4: ['RI3', 0],
      } });
      assert.deepEqual(after['5'].inputs.latent_image, before['5'].inputs.latent_image);
      assert.deepEqual(after['4'], before['4']); assert.deepEqual(after['9'], before['9']);
      assert.equal(after.QR, undefined); assert.equal(after['2'], undefined); assert.equal(after['3'], undefined);
      assert.deepEqual(after['5'].inputs.positive, ['QI', 0]); assert.deepEqual(after['5'].inputs.negative, ['QI', 1]);
      rejects({ ...input, nativeImageReference: { method, images: ['first.png'] } }, { ...a, nodes: a.nodes.filter(n => n !== 'DEAS_ImagePromptEncode') }, 'MISSING_CAPABILITY', 'DEAS_ImagePromptEncode');
      rejects({ ...input, nativeImageReference: { method: method === 'krea-2' ? 'moss-image' : 'krea-2', images: ['first.png'] } }, a, 'INVALID_INPUT', 'nativeImageReference');
    }
  }
});

test('baseline diagnostic compiles deterministic built-in graph, never Director readiness', () => {
  const result = compileRecipe(intent, allowlist);
  assert.equal(result.recipeId, intent.recipeId);
  assert.equal(result.version, 1);
  assert.equal(result.generationReady, false);
  assert.deepEqual(result.requiredNodes, allowlist.nodes);
  assert.deepEqual(result.requiredModels, [{ kind: 'checkpoint', identifier: intent.checkpoint }]);
  assert.deepEqual(result.graph, {
    '1': { class_type: 'CheckpointLoaderSimple', inputs: { ckpt_name: intent.checkpoint } },
    '2': { class_type: 'CLIPTextEncode', inputs: { text: intent.positivePrompt, clip: ['1', 1] } },
    '3': { class_type: 'CLIPTextEncode', inputs: { text: intent.negativePrompt, clip: ['1', 1] } },
    '4': { class_type: 'EmptyLatentImage', inputs: { width: 512, height: 768, batch_size: 1 } },
    '5': { class_type: 'KSampler', inputs: { model: ['1', 0], positive: ['2', 0], negative: ['3', 0], latent_image: ['4', 0], seed: 42, steps: 20, cfg: 7, sampler_name: 'euler', scheduler: 'normal', denoise: 1 } },
    '6': { class_type: 'VAEDecode', inputs: { samples: ['5', 0], vae: ['1', 2] } },
    '7': { class_type: 'SaveImage', inputs: { images: ['6', 0], filename_prefix: intent.outputPrefix } },
  });
  assert.deepEqual(compileRecipe(intent, allowlist), result);
});

test('DaSiWa metadata saver replaces SaveImage only when both of its nodes are listed', () => {
  assert.equal(compileRecipe(intent, { ...allowlist, nodes: [...allowlist.nodes, 'DaSiWa_MetadataImageSaver'] }).graph['7'].class_type, 'SaveImage');
  const graph = compileRecipe(intent, { ...allowlist, nodes: [...allowlist.nodes, 'DaSiWa_MetadataImageSaver', 'DaSiWa_MetadataConfig'] }).graph;
  assert.deepEqual(graph['7'], { class_type: 'DaSiWa_MetadataImageSaver', inputs: { images: ['6', 0], filename_prefix: intent.outputPrefix, file_format: 'png', compression: 44, save_output: true, metadata_config: ['M', 0] } });
  assert.deepEqual(graph.M, { class_type: 'DaSiWa_MetadataConfig', inputs: { save_workflow: true, model_hash: '', node_positive: ['2', 0], node_negative: ['3', 0], node_model: ['1', 0], node_latent: ['5', 0] } });
});

function rejects(input: unknown, verified: unknown, code: string, field: string) {
  assert.throws(() => compileRecipe(input, verified), (error: unknown) => {
    assert.ok(error instanceof RecipeCompileError);
    assert.equal(error.code, code);
    assert.equal(error.field, field);
    assert.ok(error.remedy.length > 10);
    return true;
  });
}

test('rejects malformed intents, unknown recipes, versions and enums at runtime', () => {
  for (const value of [null, undefined, [], 'intent', 1]) rejects(value, allowlist, 'INVALID_INTENT', 'intent');
  for (const recipeId of [undefined, 'director', 'video', 1]) rejects({ ...intent, recipeId }, allowlist, 'UNKNOWN_RECIPE', 'recipeId');
  for (const version of [undefined, 0, 2, '1', NaN]) rejects({ ...intent, version }, allowlist, 'UNSUPPORTED_VERSION', 'version');
  rejects({ ...intent, widgetState: {} }, allowlist, 'INVALID_INPUT', 'widgetState');
  for (const field of ['sampler', 'scheduler']) {
    for (const value of ['../x', 'a b', '', undefined, {}, 1]) {
      rejects({ ...intent, [field]: value }, allowlist, 'INVALID_INPUT', field);
    }
  }
});

test('validates bounded finite values, safe integer seeds and aligned dimensions', () => {
  const invalid = {
    seed: [-1, 0.5, Number.MAX_SAFE_INTEGER + 1, NaN, Infinity, '42', undefined],
    width: [0, 63, 513, 8200, 512.5, NaN, Infinity, '512', undefined],
    height: [0, 63, 513, 8200, 512.5, NaN, Infinity, '512', undefined],
    steps: [0, 151, 1.5, NaN, Infinity, '20', undefined],
    cfg: [-1, 31, NaN, Infinity, '7', undefined],
  };
  for (const [field, values] of Object.entries(invalid)) {
    for (const value of values) rejects({ ...intent, [field]: value }, allowlist, 'INVALID_INPUT', field);
  }
  for (const seed of [0, Number.MAX_SAFE_INTEGER]) {
    assert.equal(compileRecipe({ ...intent, seed, width: 64, height: 2048, steps: 150, cfg: 30 }, allowlist).graph['5'].inputs.seed, seed);
  }
  assert.doesNotThrow(() => compileRecipe({ ...intent, steps: 1, cfg: 0 }, allowlist));
});

test('rejects unsafe model identifiers even if listed, without normalizing identity', () => {
  for (const checkpoint of ['', '../m.safetensors', 'a/../m.safetensors', '/m.safetensors', 'a\\\\m.safetensors', 'C:m.safetensors', 'https://host/m', 'a//m', './m', ' m.safetensors', 'a\u0000m', 'a/%2e%2e/m']) {
    rejects({ ...intent, checkpoint }, { ...allowlist, checkpoints: [checkpoint] }, 'INVALID_INPUT', 'checkpoint');
  }
});

test('accepts model and image names with spaces and non-ASCII letters exactly as listed', () => {
  for (const checkpoint of ['MiniMaxH3/MM-H3 - Ahegao Face.safetensors', 'Größe/模型 v2.safetensors']) {
    assert.equal(compileRecipe({ ...intent, checkpoint }, { ...allowlist, checkpoints: [checkpoint] }).graph['1'].inputs.ckpt_name, checkpoint);
  }
});

test('validates prompts and app-owned output prefix without altering prompt text', () => {
  for (const field of ['positivePrompt', 'negativePrompt']) {
    for (const value of [null, undefined, 1, 'x'.repeat(16385), 'x\u0000y']) rejects({ ...intent, [field]: value }, allowlist, 'INVALID_INPUT', field);
  }
  for (const outputPrefix of ['', '../out', '/out', 'a//b', 'a\\\\b', 'a:b', 'a\n', 'x'.repeat(181), undefined]) {
    rejects({ ...intent, outputPrefix }, allowlist, 'INVALID_INPUT', 'outputPrefix');
  }
  const text = '  café\n森 🌲  ';
  const graph = compileRecipe({ ...intent, positivePrompt: text, negativePrompt: '' }, allowlist).graph;
  assert.equal(graph['2'].inputs.text, text);
  assert.equal(graph['3'].inputs.text, '');
});

test('does not mutate or retain input objects and returns deeply immutable compilation', () => {
  const frozenIntent = Object.freeze({ ...intent });
  const frozenAllowlist = Object.freeze(Object.fromEntries(Object.entries(allowlist).map(([key, values]) => [key, Object.freeze([...values])])));
  const before = JSON.stringify([frozenIntent, frozenAllowlist]);
  const result = compileRecipe(frozenIntent, frozenAllowlist);
  assert.equal(JSON.stringify([frozenIntent, frozenAllowlist]), before);
  assert.ok(Object.isFrozen(result));
  assert.ok(Object.isFrozen(result.graph));
  assert.ok(Object.isFrozen(result.graph['2'].inputs.clip));
  assert.ok(Object.isFrozen(result.graph['5'].inputs));
  assert.ok(Object.isFrozen(result.requiredNodes));
  assert.ok(Object.isFrozen(result.requiredModels[0]));
  const mutable = { ...intent };
  const compiled = compileRecipe(mutable, allowlist);
  mutable.positivePrompt = 'changed';
  assert.equal(compiled.graph['2'].inputs.text, intent.positivePrompt);
});

test('rejects inherited and sparse allowlists instead of trusting ambiguous data', () => {
  rejects(intent, Object.create(allowlist), 'INVALID_ALLOWLIST', 'allowlist');
  rejects(intent, { ...allowlist, nodes: [...allowlist.nodes, ...new Array(1)] }, 'INVALID_ALLOWLIST', 'allowlist');
  const nodes = [...allowlist.nodes];
  nodes.length += 1;
  rejects(intent, { ...allowlist, nodes }, 'INVALID_ALLOWLIST', 'allowlist');
});

test('requires every intent field explicitly instead of inferring defaults', () => {
  for (const field of Object.keys(intent)) {
    const missing: Record<string, unknown> = { ...intent };
    delete missing[field];
    rejects(missing, allowlist, field === 'recipeId' ? 'UNKNOWN_RECIPE' : field === 'version' ? 'UNSUPPORTED_VERSION' : 'INVALID_INPUT', field);
  }
});

test('requires explicit verified capability and model allowlists, failing closed', () => {
  for (const missing of [undefined, null, {}, { ...allowlist, nodes: 'KSampler' }, { ...allowlist, checkpoints: [null] }]) {
    rejects(intent, missing, 'INVALID_ALLOWLIST', 'allowlist');
  }
  for (const node of allowlist.nodes) {
    rejects(intent, { ...allowlist, nodes: allowlist.nodes.filter(n => n !== node) }, 'MISSING_CAPABILITY', node);
  }
  rejects(intent, { ...allowlist, checkpoints: ['other.safetensors'] }, 'MISSING_MODEL', 'checkpoint');
  rejects(intent, { ...allowlist, checkpoints: [] }, 'MISSING_MODEL', 'checkpoint');
  rejects(intent, { ...allowlist, samplers: [] }, 'MISSING_CAPABILITY', 'sampler');
  rejects(intent, { ...allowlist, schedulers: [] }, 'MISSING_CAPABILITY', 'scheduler');
});

test('optional LoRAs chain after the checkpoint and must be verified', () => {
  const loras = [{ name: 'style/a.safetensors', strength: 0.8 }, { name: 'b.safetensors', strength: -1 }];
  const graph = compileRecipe({ ...intent, loras }, { ...allowlist, nodes: [...allowlist.nodes, 'LoraLoader'], loras: loras.map(l => l.name) }).graph;
  assert.deepEqual(graph.L1.inputs, { model: ['1', 0], clip: ['1', 1], lora_name: 'style/a.safetensors', strength_model: 0.8, strength_clip: 0.8 });
  assert.deepEqual(graph.L2.inputs.model, ['L1', 0]);
  assert.deepEqual(graph['5'].inputs.model, ['L2', 0]);
  assert.deepEqual(graph['2'].inputs.clip, ['L2', 1]);
  assert.deepEqual(compileRecipe({ ...intent, loras: [] }, allowlist).graph, compileRecipe(intent, allowlist).graph);
  rejects({ ...intent, loras }, { ...allowlist, nodes: [...allowlist.nodes, 'LoraLoader'] }, 'MISSING_MODEL', 'loras');
  rejects({ ...intent, loras }, { ...allowlist, loras: loras.map(l => l.name) }, 'MISSING_CAPABILITY', 'LoraLoader');
  for (const bad of ['x', [{ name: '../a', strength: 1 }], [{ name: 'a', strength: 11 }], [{ name: 'a', strength: 1, extra: 1 }], Array(9).fill({ name: 'a', strength: 1 })]) {
    rejects({ ...intent, loras: bad }, allowlist, 'INVALID_INPUT', 'loras');
  }
});

test('inputImage + denoise compile image-to-image through LoadImage/VAEEncode, verified against the backend lists', () => {
  const i2i = { ...intent, inputImage: 'ref.png', denoise: 0.6 };
  const verified = { ...allowlist, nodes: [...allowlist.nodes, 'LoadImage', 'VAEEncode'], images: ['ref.png'] };
  const graph = compileRecipe(i2i, verified).graph;
  assert.equal(graph['4'], undefined);
  assert.deepEqual(graph['8'], { class_type: 'LoadImage', inputs: { image: 'ref.png' } });
  assert.deepEqual(graph['9'].inputs, { pixels: ['8', 0], vae: ['1', 2] });
  assert.deepEqual(graph['5'].inputs.latent_image, ['9', 0]);
  assert.equal(graph['5'].inputs.denoise, 0.6);
  rejects(i2i, allowlist, 'MISSING_CAPABILITY', 'LoadImage');
  rejects(i2i, { ...verified, images: [] }, 'MISSING_MODEL', 'inputImage');
  rejects({ ...intent, denoise: 0.5 }, verified, 'INVALID_INPUT', 'inputImage');
  rejects({ ...i2i, inputImage: '../ref.png' }, verified, 'INVALID_INPUT', 'inputImage');
  rejects({ ...i2i, denoise: 1.5 }, verified, 'INVALID_INPUT', 'denoise');
});

test('inputMask inpaints an [output] image: noise mask on the encoded source, composite onto it, same as native', () => {
  const mask = 'deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png';
  const inpaint = { ...intent, inputImage: 'out.png [output]', denoise: 0.8, inputMask: mask };
  const verified = { ...allowlist, nodes: [...allowlist.nodes, 'LoadImage', 'VAEEncode', 'LoadImageMask', 'SetLatentNoiseMask', 'ImageCompositeMasked'], images: [] };
  const graph = compileRecipe(inpaint, verified).graph;
  assert.deepEqual(graph.W.inputs, { image: mask, channel: 'red' });
  assert.deepEqual(graph['5'].inputs.latent_image, ['X', 0]);
  assert.deepEqual(graph.Y.inputs.destination, ['8', 0]);
  assert.deepEqual(graph['7'].inputs.images, ['Y', 0]);
  assert.deepEqual(compileRecipe(inpaint, { ...verified, nodes: [...verified.nodes, 'DaSiWa_MetadataImageSaver', 'DaSiWa_MetadataConfig'] }).graph['7'].inputs.images, ['Y', 0]);
  rejects(inpaint, { ...verified, nodes: verified.nodes.filter(n => n !== 'SetLatentNoiseMask') }, 'MISSING_CAPABILITY', 'SetLatentNoiseMask');
  for (const bad of [{ inputMask: 'ref.png' }, { inputMask: 'deas-masks/x.png' }, { inputResize: { width: 512, height: 768, crop: 'disabled' } }]) rejects({ ...inpaint, ...bad }, verified, 'INVALID_INPUT', 'inputMask');
  rejects({ ...intent, inputMask: mask }, verified, 'INVALID_INPUT', 'inputMask');
});

test('inpaint crop and blur preserve source geometry, use model resolution and gate optional core nodes', () => {
 const inpaint={...intent,inputImage:'out.png [output]',denoise:0.8,inputMask:'deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png',maskBlur:8,inpaintCrop:{x:40,y:20,width:120,height:80,targetWidth:1024,targetHeight:1024}};
 const verified={...allowlist,nodes:[...allowlist.nodes,'LoadImage','VAEEncode','LoadImageMask','SetLatentNoiseMask','ImageCompositeMasked','ImageCrop','CropMask','ImageScale','MaskToImage','ImageToMask','ImageBlur'],images:[]};
 const g=compileRecipe(inpaint,verified).graph;
 assert.deepEqual(g.IC.inputs,{image:['8',0],x:40,y:20,width:120,height:80});
 assert.deepEqual(g.IS.inputs,{image:['IC',0],upscale_method:'lanczos',width:1024,height:1024,crop:'disabled'});
 assert.deepEqual(g['9'].inputs.pixels,['IS',0]);
 assert.deepEqual(g.MB.inputs,{image:['MI',0],blur_radius:8,sigma:1});
 assert.deepEqual(g.MC.inputs,{mask:['BM',0],x:40,y:20,width:120,height:80});
 assert.deepEqual(g.X.inputs.mask,['NM',0]);
 assert.deepEqual(g.Y.inputs,{destination:['8',0],source:['IR',0],x:40,y:20,resize_source:false,mask:['MC',0]});
 assert.equal(g.IR.inputs.width,120);assert.equal(g.IR.inputs.height,80);
 for(const node of ['ImageCrop','ImageBlur','CropMask'])rejects(inpaint,{...verified,nodes:verified.nodes.filter(n=>n!==node)},'MISSING_CAPABILITY',node);
 for(const maskBlur of [-1,1.5,32,65,null])rejects({...inpaint,maskBlur},verified,'INVALID_INPUT','maskBlur');
 for(const inpaintCrop of [{...inpaint.inpaintCrop,x:-1},{...inpaint.inpaintCrop,width:9000},{...inpaint.inpaintCrop,targetWidth:999},{...inpaint.inpaintCrop,extra:1},null])rejects({...inpaint,inpaintCrop},verified,'INVALID_INPUT','inpaintCrop');
 rejects({...intent,maskBlur:4},verified,'INVALID_INPUT','maskBlur');
 rejects({...intent,inpaintCrop:inpaint.inpaintCrop},verified,'INVALID_INPUT','inpaintCrop');
 const layered=compileRecipe({...inpaint,inputLayers:[{image:'out.png [output]',x:0,y:0,width:64,height:64}],options:{vaeTile:512}}, {...verified,nodes:[...verified.nodes,'VAEEncodeTiled','VAEDecodeTiled']}).graph;
 assert.deepEqual(layered.IC.inputs.image,['B0',0]);assert.deepEqual(layered.Y.inputs.destination,['B0',0]);assert.equal(layered['9'].class_type,'VAEEncodeTiled');assert.deepEqual(layered['9'].inputs.pixels,['IS',0]);
 const {inpaintCrop:_,maskBlur:__,...plain}=inpaint;assert.deepEqual(compileRecipe({...plain,maskBlur:0},verified).graph,compileRecipe(plain,verified).graph);
});

test('inputLayers paste scaled images onto the source before resize and inpaint, same as native', () => {
  const layered = { ...intent, inputImage: 'src.png [output]', denoise: 0.6, inputLayers: [{ image: 'cat.png', x: 10, y: 20, width: 100, height: 50 }, { image: 'dog.png [output]', x: 0, y: 0, width: 8, height: 8 }] };
  const verified = { ...allowlist, nodes: [...allowlist.nodes, 'LoadImage', 'VAEEncode', 'ImageScale', 'ImageCompositeMasked', 'LoadImageMask', 'SetLatentNoiseMask'], images: ['cat.png'] };
  const graph = compileRecipe(layered, verified).graph;
  assert.deepEqual(graph.B0.inputs, { destination: ['8', 0], source: ['R0', 0], x: 10, y: 20, resize_source: false });
  assert.deepEqual(graph.B1.inputs.destination, ['B0', 0]);
  assert.deepEqual(graph['9'].inputs.pixels, ['B1', 0]);
  assert.deepEqual(compileRecipe({ ...layered, inputResize: { width: 512, height: 768, crop: 'disabled' } }, verified).graph.S.inputs.image, ['B1', 0]);
  assert.deepEqual(compileRecipe({ ...layered, inputMask: 'deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png' }, verified).graph.Y.inputs.destination, ['B1', 0]);
  rejects(layered, { ...verified, images: [] }, 'MISSING_MODEL', 'inputImage');
  for (const bad of [[], [{ image: '../x.png', x: 0, y: 0, width: 8, height: 8 }], [{ image: 'x.png', x: -1, y: 0, width: 8, height: 8 }], [{ image: 'x.png', x: 0, y: 0, width: 0, height: 8 }], Array(5).fill({ image: 'x.png', x: 0, y: 0, width: 8, height: 8 })]) rejects({ ...layered, inputLayers: bad }, verified, 'INVALID_INPUT', 'inputLayers');
  rejects({ ...intent, inputLayers: layered.inputLayers }, verified, 'INVALID_INPUT', 'inputLayers');
  // A flattened painted layer is uploaded like a mask; object_info never lists that subfolder.
  const painted = 'deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png';
  assert.equal(compileRecipe({ ...layered, inputLayers: [{ image: painted, x: 0, y: 0, width: 8, height: 8 }] }, verified).graph.I0.inputs.image, painted);
  rejects({ ...layered, inputLayers: [{ image: 'deas-masks/not-a-uuid.png', x: 0, y: 0, width: 8, height: 8 }] }, verified, 'MISSING_MODEL', 'inputImage');
});

test('inputResize builds scale/pad chains and requires only the used optional nodes', () => {
  const image = { ...intent, inputImage: 'ref.png', denoise: 0.6 };
  const lists = { ...allowlist, nodes: [...allowlist.nodes, 'LoadImage', 'VAEEncode', 'ImageScale', 'ImagePadForOutpaint'], images: ['ref.png'] };
  for (const crop of ['disabled', 'center'] as const) {
    const resize = { width: 512, height: 768, crop };
    const graph = compileRecipe({ ...image, inputResize: resize }, lists).graph;
    assert.deepEqual(graph.S, { class_type: 'ImageScale', inputs: { image: ['8', 0], upscale_method: 'lanczos', ...resize } });
    assert.deepEqual(graph['9'].inputs.pixels, ['S', 0]);
    assert.equal(graph.P, undefined);
    const split = compileRecipe({ ...image, inputResize: resize, diffusion: { textEncoder: 'clip', clipType: 'sdxl', vae: 'vae' }, loras: [{ name: 'lora.safetensors', strength: 1 }] }, { ...lists, nodes: [...lists.nodes, 'UNETLoader', 'CLIPLoader', 'VAELoader', 'LoraLoader'], diffusionModels: [intent.checkpoint], textEncoders: ['clip'], clipTypes: ['sdxl'], vaes: ['vae'], loras: ['lora.safetensors'] }).graph;
    assert.deepEqual(split['9'].inputs.vae, ['V', 0]);
    assert.deepEqual(split['5'].inputs.model, ['L1', 0]);
    const pad = { left: 8, right: 16, top: 24, bottom: 32 };
    const padded = { ...image, width: 536, height: 824, inputResize: { ...resize, pad } };
    const g = compileRecipe(padded, lists).graph;
    assert.deepEqual(g.P, { class_type: 'ImagePadForOutpaint', inputs: { image: ['S', 0], ...pad, feathering: 0 } });
    assert.deepEqual(g['9'].inputs.pixels, ['P', 0]);
    assert.deepEqual(g['5'].inputs.latent_image, ['9', 0]);
    assert.equal(g['5'].inputs.denoise, 0.6);
    for (const node of ['ImageScale', 'ImagePadForOutpaint']) rejects(padded, { ...lists, nodes: lists.nodes.filter(n => n !== node) }, 'MISSING_CAPABILITY', node);
    assert.doesNotThrow(() => compileRecipe({ ...image, inputResize: resize }, { ...lists, nodes: lists.nodes.filter(n => n !== 'ImagePadForOutpaint') }));
    assert.doesNotThrow(() => compileRecipe(image, { ...lists, nodes: lists.nodes.filter(n => !n.startsWith('Image')) }));
  }
});

test('inputResize validates exact fields, aligned integer ranges, image-only use and final geometry', () => {
  const resize = { width: 512, height: 768, crop: 'disabled' };
  const image = { ...intent, inputImage: 'ref.png', denoise: 0.6 };
  rejects({ ...intent, inputResize: resize }, allowlist, 'INVALID_INPUT', 'inputResize');
  const bad = [undefined, null, [], 'resize', {}, { ...resize, extra: 1 }, { width: 512, height: 768 },
    ...[0, -8, 7, 511, 512.5, 8200, '512', NaN, Infinity].map(width => ({ ...resize, width })),
    ...[0, 767, 768.5, 8200, '768'].map(height => ({ ...resize, height })),
    ...['fit', '', null, 1].map(crop => ({ ...resize, crop })),
    { ...resize, width: 504 }, { ...resize, height: 760 },
    ...[undefined, null, [], {}, { left: 0, right: 0, top: 0 }, { left: 0, right: 0, top: 0, bottom: 0, extra: 0 },
      ...[-8, 1, 8.5, 8200, '8', NaN].map(left => ({ left, right: 0, top: 0, bottom: 0 })),
      { left: 8, right: 0, top: 0, bottom: 0 }].map(pad => ({ ...resize, pad }))];
  for (const inputResize of bad) rejects({ ...image, inputResize }, allowlist, 'INVALID_INPUT', 'inputResize');
});

test('4K dimensions and pixel boundary are supported without allowing oversized areas', () => {
  for (const [width, height] of [[3840, 2160], [4096, 4096], [8192, 2048], [2048, 8192]]) assert.doesNotThrow(() => compileRecipe({ ...intent, width, height }, allowlist));
  rejects({ ...intent, width: 8192, height: 2056 }, allowlist, 'INVALID_INPUT', 'width');
});

test('diffusion loads the model through UNETLoader with its own text encoder and VAE, all verified', () => {
  const split = { ...intent, checkpoint: 'krea/k2.safetensors', diffusion: { textEncoder: 'qwen3vl_4b.safetensors', clipType: 'krea2', vae: 'q/qwen_image_vae.safetensors' }, loras: [{ name: 'k.safetensors', strength: 0.5 }] };
  const lists = { ...allowlist, nodes: [...allowlist.nodes, 'UNETLoader', 'CLIPLoader', 'VAELoader', 'LoraLoader'], loras: ['k.safetensors'], diffusionModels: ['krea/k2.safetensors'], textEncoders: ['qwen3vl_4b.safetensors'], clipTypes: ['krea2'], vaes: ['q/qwen_image_vae.safetensors'] };
  const g = compileRecipe(split, lists).graph;
  assert.deepEqual(g['1'], { class_type: 'UNETLoader', inputs: { unet_name: 'krea/k2.safetensors', weight_dtype: 'default' } });
  assert.deepEqual(g.C.inputs, { clip_name: 'qwen3vl_4b.safetensors', type: 'krea2' });
  assert.deepEqual(g.L1.inputs.clip, ['C', 0]);
  assert.deepEqual(g['2'].inputs.clip, ['L1', 1]);
  assert.deepEqual(g['6'].inputs.vae, ['V', 0]);
  rejects(split, { ...lists, clipTypes: [] }, 'MISSING_MODEL', 'diffusion');
  rejects(split, { ...lists, diffusionModels: [] }, 'MISSING_MODEL', 'checkpoint');
  rejects({ ...split, diffusion: { ...split.diffusion, vae: '../x' } }, lists, 'INVALID_INPUT', 'diffusion');
});

test('options chain model patches after LoRAs, tile the VAE and must be listed by the backend', () => {
  const nodes = [...allowlist.nodes, 'LoraLoader', 'LoadImage', 'VAEEncode', 'ModelAttentionBackend', 'EasyCache', 'TorchCompileModel', 'VAEDecodeTiled', 'VAEEncodeTiled'];
  const full = { ...allowlist, nodes, loras: ['a.safetensors'], images: ['ref.png'], attention: ['pytorch attention', 'comfy kitchen attention'], compile: ['inductor'] };
  const options = { attention: 'comfy kitchen attention', cache: { node: 'EasyCache', threshold: 0.2 }, compile: 'inductor', vaeTile: 512 } as const;
  const graph = compileRecipe({ ...intent, loras: [{ name: 'a.safetensors', strength: 1 }], inputImage: 'ref.png', denoise: 0.5, options }, full).graph;
  assert.deepEqual(graph.A, { class_type: 'ModelAttentionBackend', inputs: { attention: 'comfy kitchen attention', model: ['L1', 0] } });
  assert.deepEqual(graph.E.inputs, { reuse_threshold: 0.2, start_percent: 0.15, end_percent: 0.95, verbose: false, model: ['A', 0] });
  assert.deepEqual(graph.T.inputs.model, ['E', 0]);
  assert.deepEqual(graph['5'].inputs.model, ['T', 0]);
  assert.deepEqual(graph['6'], { class_type: 'VAEDecodeTiled', inputs: { samples: ['5', 0], vae: ['1', 2], tile_size: 512, overlap: 64, temporal_size: 64, temporal_overlap: 8 } });
  assert.equal(graph['9'].class_type, 'VAEEncodeTiled');
  assert.deepEqual(graph['9'].inputs.pixels, ['8', 0]);
  assert.deepEqual(compileRecipe({ ...intent, options: {} }, allowlist).graph, compileRecipe(intent, allowlist).graph);
  rejects({ ...intent, options: { attention: 'comfy kitchen attention' } }, { ...allowlist, attention: ['pytorch attention'] }, 'MISSING_CAPABILITY', 'options.attention');
  rejects({ ...intent, options: { vaeTile: 512 } }, allowlist, 'MISSING_CAPABILITY', 'VAEDecodeTiled');
  for (const bad of [{ attention: 'pytorch attention', sage: 'auto' }, { fp16Accumulation: true }, { cache: { node: 'TeaCache', threshold: 0.2 } }, { cache: { node: 'EasyCache', threshold: 4 } }, { vaeTile: 500 }, { vaeTile: 128 }, { attention: ' padded' }, { unknown: true }, []]) {
    rejects({ ...intent, options: bad }, allowlist, 'INVALID_INPUT', 'options');
  }
});

test('refine fails closed without the verified baseline gate while plain graphs stay byte-identical', () => {
  const nodes = [...allowlist.nodes, 'VAEEncode', 'DEAS_ImagePostprocessGate'];
  const lists = {...allowlist, nodes};
  const without = {...lists, nodes:nodes.filter(n=>n!=='DEAS_ImagePostprocessGate')};
  assert.equal(JSON.stringify(compileRecipe(intent,lists).graph), JSON.stringify(compileRecipe(intent,without).graph));
  rejects({...intent,refine:{width:512,height:768,denoise:0.3}},without,'MISSING_CAPABILITY','DEAS_ImagePostprocessGate');
});

test('paired baseline saves the complete first pass without extra sampling for every refine path', () => {
  const nodes = [...allowlist.nodes, 'LoadImage', 'VAEEncode', 'LoadImageMask', 'SetLatentNoiseMask', 'ImageCompositeMasked', 'ImageCrop', 'ImageScale', 'CropMask', 'MaskToImage', 'ImageToMask', 'UpscaleModelLoader', 'ImageUpscaleWithModel', 'DEAS_ImagePostprocessGate', 'VAEDecodeTiled', 'VAEEncodeTiled', 'DaSiWaTiledDiffusion', 'RandomNoise', 'CFGGuider', 'KSamplerSelect', 'ManualSigmas', 'SamplerCustomAdvanced'];
  const mask = 'deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png';
  const variants = [
    {},
    { refine: { width: 1024, height: 1536, denoise: 0.3, upscaleModel: '2x-Sharp.safetensors' } },
    { inputImage: 'ref.png', denoise: 0.4, inputMask: mask, inpaintCrop: { x: 16, y: 16, width: 128, height: 128, targetWidth: 256, targetHeight: 256 } },
    { options: { vaeTile: 512 }, refine: { width: 1024, height: 1536, denoise: 0.3, tiled: { tileWidth: 512, tileHeight: 512, overlap: 64 } } },
    { steps: 2, sigmas: [1, 0.5, 0], refine: { width: 512, height: 768, denoise: 0.3, sigmas: [0.5, 0] } },
  ];
  for (const metadata of [[], ['DaSiWa_MetadataImageSaver'], ['DaSiWa_MetadataConfig'], ['DaSiWa_MetadataImageSaver', 'DaSiWa_MetadataConfig']]) {
    const lists = { ...allowlist, nodes: [...nodes, ...metadata], images: ['ref.png'], upscaleModels: ['2x-Sharp.safetensors'] };
    for (const variant of variants) {
      const input = { ...intent, refine: { width: 512, height: 768, denoise: 0.3 }, ...variant, imageFormat: 'webp', imageCompression: 73 };
      const graph = compileRecipe(input, lists).graph;
      const { refine: _, ...first } = input;
      const before = compileRecipe(first, lists).graph;
      assert.deepEqual(graph.B7, { ...before['7'], inputs: { ...before['7'].inputs, filename_prefix: `${intent.outputPrefix}_original` } });
      assert.deepEqual(graph.PG, { class_type: 'DEAS_ImagePostprocessGate', inputs: { baseline: ['B7', 0], image: ['R6', 0] } });
      assert.deepEqual(graph['7'].inputs.images, ['PG', 0]);
      assert.equal(graph['7'].inputs.filename_prefix, intent.outputPrefix);
      for (const [id, node] of Object.entries(before)) if (id !== '7') assert.deepEqual(graph[id], node, id);
      assert.equal(Object.values(graph).filter(n => ['KSampler', 'SamplerCustomAdvanced'].includes(n.class_type)).length, 2);
      for (const id of ['7', 'B7']) {
        assert.equal(graph[id].class_type, metadata.length === 2 ? 'DaSiWa_MetadataImageSaver' : 'SaveImage');
        if (metadata.length === 2) { assert.equal(graph[id].inputs.file_format, 'webp'); assert.equal(graph[id].inputs.compression, 73); }
      }
      assert.equal(before.B7, undefined);
      assert.equal(before.PG, undefined);
      assert.equal(JSON.stringify(before), JSON.stringify(compileRecipe(first, {...lists,nodes:lists.nodes.filter(n=>n!=='DEAS_ImagePostprocessGate')}).graph));
    }
    rejects({ ...intent, refine: { width: 512, height: 768, denoise: 0.3 } }, { ...lists, nodes: [] }, 'MISSING_CAPABILITY', 'CheckpointLoaderSimple');
  }
  const refine = { width: 512, height: 768, denoise: 0.3 };
  compileRecipe({ ...intent, outputPrefix: 'x'.repeat(171), refine }, { ...allowlist, nodes: [...nodes] });
  compileRecipe({ ...intent, outputPrefix: 'x'.repeat(180) }, allowlist);
  rejects({ ...intent, outputPrefix: 'x'.repeat(172), refine }, { ...allowlist, nodes }, 'INVALID_INPUT', 'outputPrefix');
  rejects({ ...intent, refine }, { ...allowlist, nodes: nodes.filter(n => n !== 'SaveImage') }, 'MISSING_CAPABILITY', 'SaveImage');
});

test('refine pass and source upscale model mirror the native compiler', () => {
  const lists = { ...allowlist, nodes: [...allowlist.nodes, 'LoadImage', 'VAEEncode', 'ImageScale', 'UpscaleModelLoader', 'ImageUpscaleWithModel', 'DEAS_ImagePostprocessGate'], images: ['ref.png'], upscaleModels: ['2x-Sharp.safetensors'] };
  const g = compileRecipe({ ...intent, refine: { width: 1024, height: 1536, denoise: 0.3, upscaleModel: '2x-Sharp.safetensors' } }, lists).graph;
  assert.deepEqual(g.RU.inputs, { upscale_model: ['RL', 0], image: ['6', 0] });
  assert.deepEqual(g.RS.inputs, { image: ['RU', 0], upscale_method: 'lanczos', width: 1024, height: 1536, crop: 'disabled' });
  assert.deepEqual(g.R5.inputs, { ...g['5'].inputs, latent_image: ['RE', 0], denoise: 0.3 });
  assert.deepEqual(g['7'].inputs.images, ['PG', 0]);
  const same = compileRecipe({ ...intent, refine: { width: 512, height: 768, denoise: 0.3 } }, lists).graph;
  assert.equal(same.RS, undefined);
  assert.deepEqual(same.RE.inputs.pixels, ['6', 0]);
  rejects({ ...intent, refine: { width: 1024, height: 1536, denoise: 0.3, upscaleModel: 'missing.pth' } }, lists, 'MISSING_MODEL', 'upscaleModel');
  for (const bad of [{ width: 1001, height: 768, denoise: 0.3 }, { width: 512, height: 768, denoise: 0 }, { width: 512, height: 768, denoise: 0.3, extra: 1 }, { width: 512, height: 768, denoise: 0.3, sigmas: [0.5, 0] }]) rejects({ ...intent, refine: bad }, lists, 'INVALID_INPUT', 'refine');
  // Tiled refine: the DaSiWa tiling patch feeds only the refine sampler; the first pass keeps the plain model.
  const tiled = { tileWidth: 1024, tileHeight: 1024, overlap: 128 };
  const tg = compileRecipe({ ...intent, refine: { width: 2048, height: 2048, denoise: 0.3, tiled } }, { ...lists, nodes: [...lists.nodes, 'DaSiWaTiledDiffusion'] }).graph;
  assert.deepEqual(tg.RT, { class_type: 'DaSiWaTiledDiffusion', inputs: { model: tg['5'].inputs.model, tile_width: 1024, tile_height: 1024, overlap: 128 } });
  assert.deepEqual(tg.R5.inputs, { ...tg['5'].inputs, model: ['RT', 0], latent_image: ['RE', 0], denoise: 0.3 });
  const ts = compileRecipe({ ...intent, steps: 2, sigmas: [1, 0.5, 0], refine: { width: 2048, height: 2048, denoise: 0.3, sigmas: [0.5, 0], tiled } }, { ...lists, nodes: [...lists.nodes, 'DaSiWaTiledDiffusion', 'ManualSigmas', 'SamplerCustomAdvanced', 'CFGGuider', 'KSamplerSelect', 'RandomNoise'] }).graph;
  assert.deepEqual(ts.RG.inputs, { ...ts.G.inputs, model: ['RT', 0] });
  assert.deepEqual(ts.R5.inputs.guider, ['RG', 0]);
  assert.deepEqual(ts['5'].inputs.guider, ['G', 0]);
  rejects({ ...intent, refine: { width: 2048, height: 2048, denoise: 0.3, tiled } }, lists, 'MISSING_CAPABILITY', 'DaSiWaTiledDiffusion');
  for (const bad of [{ ...tiled, overlap: 1024 }, { ...tiled, overlap: 12 }, { ...tiled, tileWidth: 1004 }, { tileWidth: 1024, tileHeight: 1024 }]) rejects({ ...intent, refine: { width: 2048, height: 2048, denoise: 0.3, tiled: bad } }, lists, 'INVALID_INPUT', 'refine');
  const src = compileRecipe({ ...intent, inputImage: 'ref.png', denoise: 0.35, inputResize: { width: 512, height: 768, crop: 'disabled' }, upscaleModel: '2x-Sharp.safetensors' }, lists).graph;
  assert.deepEqual(src.UM.inputs.image, ['8', 0]);
  assert.deepEqual(src.S.inputs.image, ['UM', 0]);
  rejects({ ...intent, inputImage: 'ref.png', denoise: 0.35, upscaleModel: '2x-Sharp.safetensors' }, lists, 'INVALID_INPUT', 'upscaleModel');
});

const revisionLists = { ...allowlist, nodes: [...allowlist.nodes, 'CLIPVisionLoader', 'LoadImage', 'CLIPVisionEncode', 'unCLIPConditioning'], images: ['deas-inputs/ref.png', 'deas-inputs/second.png'], visionModels: ['clip-vit.safetensors'] };
const revisionRef = { visionModel: 'clip-vit.safetensors', images: [{ image: 'deas-inputs/ref.png', strength: 0.5 }] };

test('imageReference compiles the core ReVision chain onto the positive conditioning', () => {
  const result = compileRecipe({ ...intent, imageReference: revisionRef }, revisionLists);
  const graph = result.graph as Record<string, { class_type: string; inputs: Record<string, unknown> }>;
  assert.deepEqual(graph.CV, { class_type: 'CLIPVisionLoader', inputs: { clip_name: revisionRef.visionModel } });
  assert.deepEqual(graph.RI0, { class_type: 'LoadImage', inputs: { image: 'deas-inputs/ref.png' } });
  assert.deepEqual(graph.CE0, { class_type: 'CLIPVisionEncode', inputs: { clip_vision: ['CV', 0], image: ['RI0', 0] } });
  assert.deepEqual(graph.UC0, { class_type: 'unCLIPConditioning', inputs: { conditioning: ['2', 0], clip_vision_output: ['CE0', 0], strength: 0.5, noise_augmentation: 0 } });
  assert.deepEqual(graph['5'].inputs.positive, ['UC0', 0]);
  assert.ok(result.requiredNodes.includes('unCLIPConditioning'));
  const chain = compileRecipe({ ...intent, imageReference: { visionModel: revisionRef.visionModel, images: [{ image: 'deas-inputs/ref.png', strength: 0.5 }, { image: 'deas-inputs/second.png', strength: 0.2 }] } }, revisionLists).graph as Record<string, { inputs: Record<string, unknown> }>;
  assert.deepEqual(chain.UC1.inputs.conditioning, ['UC0', 0]);
  assert.deepEqual(chain.UC1.inputs.strength, 0.2);
  assert.deepEqual(chain['5'].inputs.positive, ['UC1', 0]);
  const sigmas = compileRecipe({ ...intent, steps: 2, sigmas: [1, 0.5, 0], imageReference: revisionRef }, { ...revisionLists, nodes: [...revisionLists.nodes, 'ManualSigmas', 'SamplerCustomAdvanced', 'CFGGuider', 'KSamplerSelect', 'RandomNoise'] }).graph as Record<string, { inputs: Record<string, unknown> }>;
  assert.deepEqual(sigmas.G.inputs.positive, ['UC0', 0]);
});

test('imageReference validates vision model, pictures, count and split loaders', () => {
  compileRecipe({ ...intent, imageReference: revisionRef }, revisionLists);
  rejects({ ...intent, imageReference: revisionRef }, { ...revisionLists, visionModels: [] }, 'MISSING_MODEL', 'visionModel');
  rejects({ ...intent, imageReference: { visionModel: revisionRef.visionModel, images: [{ image: 'absent.png', strength: 0.5 }] } }, revisionLists, 'MISSING_MODEL', 'inputImage');
  rejects({ ...intent, imageReference: { visionModel: revisionRef.visionModel, images: [] } }, revisionLists, 'INVALID_INPUT', 'imageReference');
  rejects({ ...intent, imageReference: { visionModel: revisionRef.visionModel, images: Array.from({ length: 5 }, (_, i) => ({ image: `deas-inputs/ref${i}.png`, strength: 0.5 })) } }, revisionLists, 'INVALID_INPUT', 'imageReference');
  rejects({ ...intent, diffusion: { textEncoder: 'clip', clipType: 'sdxl', vae: 'vae' }, imageReference: revisionRef }, { ...revisionLists, nodes: [...revisionLists.nodes, 'UNETLoader', 'CLIPLoader', 'VAELoader'] }, 'INVALID_INPUT', 'imageReference');
  for (const strength of [1.5, -0.1, Number.NaN, '0.5']) rejects({ ...intent, imageReference: { visionModel: revisionRef.visionModel, images: [{ image: 'deas-inputs/ref.png', strength }] } }, revisionLists, 'INVALID_INPUT', 'imageReference');
});
