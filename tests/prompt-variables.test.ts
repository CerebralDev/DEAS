import test from 'node:test';
import assert from 'node:assert/strict';
import { fnv1a, flattenCategories, mergeVariableLibrary, parsePins, parseVariableLibrary, prunePins, resolveVariables, scanVariables, variableName, variablePick } from '../src/lib/promptVariables.ts';

// Golden vectors captured from the node pack's resolve_variables (nodes_wildcard_preset_prompt_builder.py, 0.5.39).
const library = { color: ['red', 'blue', 'green'], hairstyle: ['bob', 'ponytail'], a1: ['x', 'y'] };
const vectors: [string, Record<string, string>, number, string][] = [
  ['$color and $color at ${color}-light, $$ stays, $a1-1, $unknown', {}, 42, 'blue and green at red-light, $ stays, x-1, $unknown'],
  ['$color', {}, 0, 'red'],
  ['$color', { 'color:0': 'green' }, 42, 'green'],
  ['${hairstyle}color $color$hairstyle', {}, 123456789, 'bobcolor redponytail'],
  ['$color $color', { 'color:1': 'blue' }, 7, 'red blue'],
  ['$$money ${a1}-1 ${a1}-1 $$', {}, 20261105, '$money y-1 x-1 $'],
  ['$a1-1 and $a1-10 and $a1-11', {}, 99, 'x-1 and y-10 and x-11'],
];
for (const [text, pins, seed, expected] of vectors) {
  test(`golden vector seed ${seed}: ${text.slice(0, 40)}`, () => {
    assert.equal(resolveVariables(text, library, pins, seed), expected);
  });
}
test('FNV-1a matches the node _hash_seed over seed\\x1fname:index', () => {
  assert.equal(fnv1a('42\u001fcolor:0'), 3263871769);
  assert.equal(fnv1a('0\u001fcolor:0'), 3528979125);
});
test('pins outside the options fall back to the seed pick; occurrence keys count only known names', () => {
  assert.equal(resolveVariables('$color', library, { 'color:0': 'pink' }, 42), 'blue');
  assert.deepEqual(scanVariables('$unknown $color $color', library), [
    { name: 'unknown', key: 'unknown:0', known: false },
    { name: 'color', key: 'color:0', known: true },
    { name: 'color', key: 'color:1', known: true },
  ]);
});
test('category names sanitize to legal identifiers with first-wins collisions and custom override', () => {
  assert.equal(variableName('art/negative prompt'), 'art_negative_prompt');
  assert.equal(variableName('3D style'), '_3D_style');
  assert.equal(variableName('Manga/A CG'), 'Manga_A_CG');
  const cats = { 'a b': ['one'], 'a_b': ['two'], Light: ['lamp'] };
  const flat = mergeVariableLibrary(cats, { Light: ['custom'] }, ['a_b', 'Light']);
  assert.deepEqual(flat, { a_b: ['two'], Light: ['custom'] });
});
test('category flattening keeps only string entries of the active style', () => {
  const subjects = { Art: [{ subject: 'Light', booru_wildcards: ['{red|blue} light'], nl_presets: ['soft light'] }] };
  assert.deepEqual(flattenCategories(subjects, 'Booru'), { Art: ['{red|blue} light'] });
  assert.deepEqual(flattenCategories(subjects, 'Natural Language'), { Art: ['soft light'] });
});
test('library validation matches the node', () => {
  assert.deepEqual(parseVariableLibrary('{"color": ["red"]}'), { color: ['red'] });
  assert.deepEqual(parseVariableLibrary(''), {});
  for (const bad of ['{"3x": ["a"]}', '{"x": []}', '{"x": "y"}', '{"x": ["  "]}', 'not json', '"text"']) {
    assert.throws(() => parseVariableLibrary(bad));
  }
});
test('pins round-trip through the JSON string and drop stale occurrences', () => {
  const pins = parsePins('{"color:0":"green","gone:9":"x"}');
  assert.deepEqual(pins, { 'color:0': 'green', 'gone:9': 'x' });
  assert.deepEqual(parsePins(prunePins('$color', library, pins)), { 'color:0': 'green' });
  assert.equal(variablePick('color', 'color:0', library, pins, 42), 'green');
  assert.equal(variablePick('missing', 'missing:0', library, {}, 42), undefined);
});
