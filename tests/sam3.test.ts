import test from 'node:test';
import assert from 'node:assert/strict';
import { compileSam3Mask, isSam3Intent, type Sam3MaskIntentV1 } from '../src/lib/sam3.ts';
import type { RecipeAllowlist } from '../src/lib/execution.ts';
const intent = (): Sam3MaskIntentV1 => ({ recipeId: 'sam3-mask', version: 1, checkpoint: 'sam3.1.safetensors', inputImage: 'source.png', width: 641, height: 479, outputPrefix: 'DEAS/mask', prompt: 'eyes', threshold: 0.5 });
const allow: RecipeAllowlist = { nodes: ['CheckpointLoaderSimple', 'LoadImage', 'CLIPTextEncode', 'SAM3_Detect', 'MaskToImage', 'SaveImage', 'PrimitiveBoundingBox'], checkpoints: ['sam3.1.safetensors'], images: ['source.png'], samplers: [], schedulers: [] };

test('SAM3 text keeps all 200 detector queries per category and saves an unscaled RGB union', () => {
  const i = { ...intent(), prompt: '(eyes:1), feet' };
  const before = structuredClone(i);
  const result = compileSam3Mask(i, allow);
  assert.deepEqual(i, before);
  assert.equal(result.graph['3'].inputs.text, 'eyes:200, feet:200');
  assert.equal(result.graph['4'].inputs.individual_masks, false);
  assert.equal(result.graph['4'].inputs.refine_iterations, 2);
  assert.deepEqual(result.graph['6'], { class_type: 'MaskToImage', inputs: { mask: ['4', 0] } });
  assert.deepEqual(result.graph['7'], { class_type: 'SaveImage', inputs: { images: ['6', 0], filename_prefix: 'DEAS/mask' } });
  assert.ok(!result.requiredNodes.includes('ImageScale'));
  assert.equal(result.generationReady, false);
  assert.deepEqual(compileSam3Mask(i, allow), result);
});

test('SAM3 clicks, boxes and combined prompts use real core inputs', () => {
  const i = intent(); delete i.prompt;
  i.positivePoints = [{ x: 640, y: 478 }]; i.negativePoints = [{ x: 0, y: 0 }];
  i.box = { x: 1, y: 2, width: 640, height: 477 };
  const { graph } = compileSam3Mask(i, allow);
  assert.equal(graph['4'].inputs.positive_coords, '[{"x":640,"y":478}]');
  assert.equal(graph['4'].inputs.negative_coords, '[{"x":0,"y":0}]');
  assert.deepEqual(graph.B, { class_type: 'PrimitiveBoundingBox', inputs: i.box });
  assert.deepEqual(graph['4'].inputs.bboxes, ['B', 0]);
  assert.equal(graph['3'], undefined);
  delete i.positivePoints; delete i.negativePoints;
  assert.ok(isSam3Intent(i));
  i.prompt = 'hands';
  assert.deepEqual(compileSam3Mask(i, allow).graph['4'].inputs.conditioning, ['3', 0]);
  delete i.box; delete i.prompt;
  assert.equal(isSam3Intent(i), false);
});

test('SAM3 validates the complete strict source-pixel contract', () => {
  for (const [key, value] of [ ['width', 0], ['height', 8193], ['threshold', 1.01], ['threshold', NaN], ['threshold', null],
    ['checkpoint', '../sam3.1.safetensors'], ['inputImage', '/etc/passwd'], ['inputImage', 'http://host/image'],
    ['outputPrefix', '../mask'], ['prompt', '\0'], ['prompt', '(), ,'], ['version', 2], ['extra', true],
    ['positivePoints', [{ x: 641, y: 0 }]], ['negativePoints', [{ x: 0.5, y: 0 }]], ['positivePoints', [{ x: 1, y: 1, extra: true }]],
    ['box', { x: 1, y: 0, width: 641, height: 1 }], ['box', null], ['positivePoints', undefined], ['prompt', 'x'.repeat(16385)] ] as const) {
    const i = { ...intent(), [key]: value };
    assert.equal(isSam3Intent(i), false, `${key}: ${JSON.stringify(value)}`);
    assert.throws(() => compileSam3Mask(i, allow), { code: 'INVALID_INTENT' });
  }
  assert.equal(isSam3Intent({ ...intent(), width: 8192, height: 8192 }), false);
  for (const key of Object.keys(intent()).filter(k => k !== 'prompt')) {
    const i: Record<string, unknown> = { ...intent() }; delete i[key]; assert.equal(isSam3Intent(i), false, key);
  }
  assert.equal(isSam3Intent(null), false);
});

test('SAM3 preflight independently requires only selected model, source and emitted nodes', () => {
  assert.throws(() => compileSam3Mask(intent(), { ...allow, checkpoints: [] }), { code: 'MISSING_MODEL' });
  assert.throws(() => compileSam3Mask(intent(), { ...allow, nodes: allow.nodes.filter(n => n !== 'SAM3_Detect') }), { code: 'MISSING_CAPABILITY' });
  assert.throws(() => compileSam3Mask(intent(), { ...allow, images: [] }), { code: 'INVALID_INPUT' });
  assert.throws(() => compileSam3Mask(intent(), null as unknown as RecipeAllowlist), { code: 'INVALID_ALLOWLIST' });
  assert.doesNotThrow(() => compileSam3Mask(intent(), { ...allow, nodes: allow.nodes.filter(n => n !== 'PrimitiveBoundingBox') }));
  for (const inputImage of ['deas-masks/12345678-1234-1234-1234-123456789abc.png', 'deas-inputs/source.png']) assert.doesNotThrow(() => compileSam3Mask({ ...intent(), inputImage }, { ...allow, images: [] }));
  for (const inputImage of ['deas-masks/arbitrary.png', 'DEAS/output.png [output]', 'deas-inputs/../secret.png']) assert.throws(() => compileSam3Mask({ ...intent(), inputImage }, { ...allow, images: [] }));
  assert.doesNotThrow(() => compileSam3Mask({ ...intent(), checkpoint: 'SAM\\sam3.1.safetensors' }, { ...allow, checkpoints: ['SAM/sam3.1.safetensors'] }));
});
