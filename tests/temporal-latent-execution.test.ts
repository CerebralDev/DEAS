import test from 'node:test';
import assert from 'node:assert/strict';
import { compileRecipe, RecipeCompileError, type DiagnosticImageIntentV1 } from '../src/lib/execution.ts';

const base: DiagnosticImageIntentV1 = {
  recipeId: 'checkpoint-text-to-image-diagnostic', version: 1,
  positivePrompt: 'forest', negativePrompt: '', checkpoint: 'model.safetensors',
  seed: 42, width: 512, height: 768, steps: 20, cfg: 4,
  sampler: 'euler', scheduler: 'normal', outputPrefix: 'DEAS_test',
  refine: { width: 1024, height: 1536, denoise: 0.3 },
};
const tiled = { tileWidth: 512, tileHeight: 768, overlap: 64 };
const allow = {
  nodes: ['CheckpointLoaderSimple', 'UNETLoader', 'CLIPLoader', 'VAELoader', 'CLIPTextEncode', 'EmptyLatentImage', 'KSampler', 'VAEDecode', 'SaveImage', 'DEAS_ImagePostprocessGate', 'VAEEncode', 'ImageScale', 'DaSiWaTiledDiffusion'],
  checkpoints: [base.checkpoint], diffusionModels: [base.checkpoint],
  textEncoders: ['clip.safetensors'], vaes: ['vae.safetensors'],
  clipTypes: ['stable_diffusion', 'krea2', 'minimax'], samplers: ['euler'], schedulers: ['normal'],
};

for (const [family, clipType] of [['anima', 'stable_diffusion'], ['krea-2', 'krea2'], ['minimax-h3', 'minimax']]) {
  for (const selection of ['family', 'diffusion', 'clipType']) {
    test(`temporal latent tiled refine rejects direct ${family}/${selection} preview`, () => {
      const intent = {
        ...base,
        ...(selection === 'family' ? { family } : {}),
        diffusion: { textEncoder: 'clip.safetensors', clipType, vae: 'vae.safetensors',
          ...(selection === 'family' ? { family: 'sdxl' } : selection === 'diffusion' ? { family } : {}) },
      };
      if (family !== 'minimax-h3') {
        const graph = compileRecipe(intent, allow).graph;
        assert.equal(graph.RT, undefined);
        assert.equal(graph.R5.class_type, 'KSampler');
      }
      const rejectsTiled = (input: unknown) => assert.throws(() => compileRecipe(input, allow),
        (error: unknown) => error instanceof RecipeCompileError && error.code === 'INVALID_INTENT'
          && error.field === 'refine.tiled' && /temporal latent/.test(error.message));
      rejectsTiled({ ...intent, refine: { ...base.refine, tiled } });
      const scheduled = { ...intent, steps: 2, sigmas: [1, 0.5, 0], refine: { ...base.refine, sigmas: [0.5, 0] } };
      rejectsTiled({ ...scheduled, refine: { ...scheduled.refine, tiled } });
    });
  }
}

test('SDXL tiled refine follows the actual explicit family, not a conflicting diffusion hint', () => {
  for (const diffusion of [undefined, { family: 'anima', textEncoder: 'clip.safetensors', clipType: 'stable_diffusion', vae: 'vae.safetensors' }]) {
    const graph = compileRecipe({ ...base, family: 'sdxl', ...(diffusion ? { diffusion } : {}), refine: { ...base.refine, tiled } }, allow).graph;
    assert.equal(graph.RT.class_type, 'DaSiWaTiledDiffusion');
    assert.deepEqual(graph.R5.inputs.model, ['RT', 0]);
    assert.notDeepEqual(graph['5'].inputs.model, ['RT', 0]);
  }
});
