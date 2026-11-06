import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { bundledCatalog, I18nProvider } from '../../i18n/I18nProvider';
import { ForgeButton, type H3Context } from './ForgeButton';
import { PromptForge } from './PromptForge';
import * as references from './ReferenceImages';
import { dialectOf } from '../../lib/forge';
import { aspects as settingsAspects } from '../../lib/resolution';

it('global search finds the H3 picture switch or automatic-vision status', async () => {
  const { matchEntries, searchIndex } = await import('../../lib/search');
  expect(matchEntries(searchIndex, 'vision', key => bundledCatalog.dictionaries.en[key] ?? key).map(entry => entry.target)).toContain('[data-forge-see], [data-forge-auto-see]');
});

it('routes the MOSS generation family to the existing MOSS prompt writer', () => {
  expect(dialectOf('moss-image')).toBe('MOSS');
});

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn(() => true), convertFileSrc: (p: string) => p }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }) }));
const models = { models: [{ id: 'ollama:qwen', label: 'qwen' }], errors: {}, creativity: ['literal', 'precise', 'faithful', 'measured', 'balanced', 'loose', 'inventive', 'creative', 'daring', 'wild'], shot_counts: ['Auto', 1, 2], default_shots: 'Auto', durations: [5, 8] };
const nativeTargets = { source: { commit: 'test' }, creativity: models.creativity, targets: ['illustrious', 'anima', 'krea2', 'moss'].map(key => ({ key, label: key, modes: [], defaultMode: null, optional: ['Reasoning', 'Negative prompt'], qualityPresets: [], styles: [], aspectRatios: null, hasNegative: key !== 'krea2', tools: true, guidance: [] })) };
const nativeCalls = () => vi.mocked(invoke).mock.calls.filter(([c]) => c === 'native_forge_write').map(([, a]) => a as { root: string; request: Record<string, unknown> });
const forgeCalls = () => vi.mocked(invoke).mock.calls.filter(([c]) => c === 'native_forge').map(([, a]) => a as { route: string; body: Record<string, unknown> });
beforeEach(() => {
  localStorage.clear(); localStorage.setItem('deas.locale.v1', 'en');
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === 'native_llm_read') return { provider: 'ollama', baseUrl: 'http://localhost:11434' };
    if (command === 'native_llm_models') return { models: [{ provider: 'ollama', id: 'qwen', name: 'qwen' }], warnings: [] };
    if (command === 'native_media_star_status') return { starred: false, unavailable: null };
    const { route } = args as { route: string };
    if (command === 'native_forge' && route === 'models') return models;
    if (command === 'native_forge' && route === 'write') return { prompt: '1girl, rain', saw_images: 0 };
    if (command === 'native_forge' && route === 'h3') return { simple_prompt: 'H3 structured prompt', warnings: ['check cut'] };
    if (command === 'native_forge_targets') return nativeTargets;
    if (command === 'native_forge_write') return { segments: [{ label: 'Positive prompt', body: '1girl, rain' }], refused: false, prompt: '1girl, rain', negative: null, characters: [], tools: [], rounds: 0, warnings: [], fixes: [] };
    if (command === 'native_media_list') return { path: '/env/input', parent: null, truncated: false, entries: [{ name: 'a.png', path: '/env/input/a.png', kind: 'file', symlink: false, media: 'image', size: 1, modified: 1 }] };
    throw new Error(`Unexpected command: ${command}`);
  });
});
afterEach(() => { cleanup(); vi.resetAllMocks(); });

it('retained inline H3 editor waits for its tab and keeps shots, cards, drafts and Apply/Undo', async () => {
  const apply=vi.fn();
  const h3:H3Context={mode:'I2VA',duration:5,canvas:{width:768,height:512},references:[{kind:'image',role:'keyframe',path:'first.png',who:[2]}],easy:false};
  const view=(active:boolean)=><I18nProvider><ForgeButton inline active={active} endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="original" onApply={apply} h3={h3}/></I18nProvider>;
  const {rerender}=render(view(false));
  expect(forgeCalls()).toEqual([]);
  expect(screen.queryByRole('dialog')).toBeNull();
  rerender(view(true));
  await screen.findByRole('option',{name:'qwen'});
  fireEvent.change(screen.getByRole('combobox',{name:'Shots'}),{target:{value:'2'}});
  fireEvent.change(document.querySelector('[data-forge-shot="2"]')!,{target:{value:'turn left'}});
  fireEvent.change(screen.getByRole('textbox',{name:'Card of Character 2'}),{target:{value:'blue coat'}});
  fireEvent.change(screen.getByRole('slider',{name:'Detail 5'}),{target:{value:'7'}});
  fireEvent.click(screen.getByRole('button',{name:'Write'}));
  await screen.findByRole('button',{name:'Apply'});
  expect(forgeCalls().find(c=>c.route==='h3')!.body).toMatchObject({detail:7,shots:2,shot_briefs:['','turn left'],cast:{2:'blue coat'}});
  rerender(view(false));rerender(view(true));
  expect(screen.getByRole('textbox',{name:'Card of Character 2'})).toHaveProperty('value','blue coat');
  expect(document.querySelector('[data-forge-shot="2"]')).toHaveProperty('value','turn left');
  expect(document.querySelector('[data-forge-draft="1"]')).toBeTruthy();
  fireEvent.click(screen.getByRole('button',{name:'Apply'}));
  expect(apply).toHaveBeenLastCalledWith('H3 structured prompt');
  fireEvent.click(screen.getByRole('button',{name:'Undo'}));
  expect(apply).toHaveBeenLastCalledWith('original');
});

it('open hammer switches from native discovery to node-pack discovery with its dialect', async () => {
  const view = (dialect: string) => <I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect={dialect} value="idea" onApply={vi.fn()}/></I18nProvider>;
  const { rerender } = render(view('Anima'));
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await screen.findByRole('option', { name: 'qwen' });
  expect(forgeCalls()).toEqual([]);
  rerender(view('Wan 2.2'));
  await waitFor(() => expect(forgeCalls()).toEqual([expect.objectContaining({ route: 'models' })]));
  await screen.findByRole('option', { name: 'qwen' });
});

it('accepts functional reference updates and opens the references fold after additions', async () => {
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((c, a) => c === 'native_forge_targets' ? Promise.resolve({ ...nativeTargets, references: { roles: [{ key: 'style', label: 'Style' }], fidelity: [], defaultRole: null, defaultFidelity: null, max: 4, analyze: false } }) : base(c, a));
  vi.spyOn(references, 'ReferenceImages').mockImplementation(({ pictures, onPictures }) => {
    const update = onPictures as React.Dispatch<React.SetStateAction<references.RefPicture[]>>;
    return <button onClick={() => update(current => [...current, { id: String(current.length), name: 'reference', image: 'data:image/png;base64,A', role: '', fidelity: '', note: '', character: '' }])}>Add reference {pictures.length}</button>;
  });
  render(<I18nProvider><PromptForge endpoint="" root="/env" onSend={vi.fn()}/></I18nProvider>);
  const add = await screen.findByRole('button', { name: 'Add reference 0', hidden: true });
  expect(document.querySelector('[data-forge-fold="refs"]')!.classList.contains('open')).toBe(false);
  fireEvent.click(add);
  await screen.findByRole('button', { name: 'Add reference 1' });
  expect(document.querySelector('[data-forge-fold="refs"]')!.classList.contains('open')).toBe(true);
  fireEvent.click(document.querySelector('[data-forge-fold="refs"] > button')!);
  expect(document.querySelector('[data-forge-fold="refs"]')!.classList.contains('open')).toBe(false);
  fireEvent.click(screen.getByRole('button', { name: 'Add reference 1', hidden: true }));
  await screen.findByRole('button', { name: 'Add reference 2' });
  expect(document.querySelector('[data-forge-fold="refs"]')!.classList.contains('open')).toBe(true);
});

it('shares brief/detail/creativity across native and node-pack target switches', async () => {
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((c, a) => c === 'native_forge_targets' ? Promise.resolve({ ...nativeTargets, targets: nativeTargets.targets.map(t => ({ ...t, detailLevels: Array.from({ length: 10 }, (_, i) => [i + 1, String(i + 1)]) })) }) : base(c, a));
  render(<I18nProvider><PromptForge endpoint="" root="/env" onSend={vi.fn()}/></I18nProvider>);
  await screen.findByRole('option', { name: 'qwen' });
  fireEvent.change(screen.getByRole('textbox', { name: 'Idea or prompt to enhance' }), { target: { value: 'preserved idea' } });
  fireEvent.change(document.querySelector('[data-forge-ladder="detail"]')!, { target: { value: '8' } });
  fireEvent.change(document.querySelector('[data-forge-ladder="creativity"]')!, { target: { value: '7' } });
  fireEvent.change(document.querySelector('[data-forge-target]')!, { target: { value: 'MiniMax H3' } });
  await screen.findByRole('option', { name: 'qwen' });
  expect(screen.getByRole('textbox', { name: 'Idea or prompt to enhance' })).toHaveProperty('value', 'preserved idea');
  expect(screen.getByRole('slider', { name: 'Detail 9' })).toHaveProperty('value', '9');
  expect(screen.getByRole('slider', { name: 'Creativity 8' })).toHaveProperty('value', '8');
  fireEvent.change(document.querySelector('[data-forge-target]')!, { target: { value: 'Anima' } });
  expect(screen.getByRole('textbox', { name: 'Idea or prompt to enhance' })).toHaveProperty('value', 'preserved idea');
});

const continuityRun: H3Context & { continuity: NonNullable<H3Context['continuity']> } = {
  mode: 'T2VA', duration: 5, canvas: { width: 1024, height: 576 }, references: [], easy: false,
  continuity: { source_kind: 'checkpoint' as const, session: 'session-a', clip_id: 'clip-a', overlap_frames: 22 as const, current_prompt: 'previous action', use_references: false as const },
};

it.each(['endpoint', 'root', 'source', 'session', 'overlap', 'current_prompt', 'duration', 'canvas', 'model'])('checkpoint Forge draft/history become stale after changing %s', async change => {
  const view = (run = continuityRun, endpoint = 'http://127.0.0.1:8188', root = '/env') => <I18nProvider><ForgeButton endpoint={endpoint} root={root} dialect="MiniMax H3" value="turn left" onApply={vi.fn()} h3={run}/></I18nProvider>;
  const { rerender } = render(view());
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await screen.findByRole('option', { name: 'qwen' });
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await screen.findByRole('textbox', { name: 'Result' });
  const run = { ...continuityRun, continuity: { ...continuityRun.continuity } };
  if (change === 'source') run.continuity.clip_id = 'clip-b';
  if (change === 'session') run.continuity.session = 'session-b';
  if (change === 'overlap') run.continuity.overlap_frames = 39;
  if (change === 'current_prompt') run.continuity.current_prompt = 'edited action';
  if (change === 'duration') run.duration = 8;
  if (change === 'canvas') run.canvas = { width: 576, height: 1024 };
  if (change === 'model') {
    const base = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation((c, a) => c === 'native_forge' && (a as { route: string }).route === 'models' ? Promise.resolve({ ...models, models: [{ id: 'other', label: 'other' }] }) : base(c, a));
    act(() => window.dispatchEvent(new Event('deas:llm-changed')));
    await screen.findByRole('option', { name: 'other' });
  } else {
    rerender(view(run, change === 'endpoint' ? 'http://127.0.0.1:8288' : undefined, change === 'root' ? '/other' : undefined));
    await screen.findByRole('option', { name: 'qwen' });
  }
  expect((screen.getByRole('button', { name: 'Apply' }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole('button', { name: 'Draft 1 ⚠' }));
  expect((screen.getByRole('button', { name: 'Apply' }) as HTMLButtonElement).disabled).toBe(true);
});

it.each(['h3', 'write'])('Forge %s reports only actual busy transitions despite callback rerenders', async route => {
  let resolve!: (value: unknown) => void;
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((c, a) => c === 'native_forge' && (a as { route: string }).route === route ? new Promise(r => { resolve = r; }) as ReturnType<typeof invoke> : base(c, a));
  const changes = vi.fn();
  const view = () => <I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="turn left" onApply={vi.fn()} h3={route === 'h3' ? continuityRun : undefined} onBusyChange={busy => changes(busy)}/></I18nProvider>;
  const { rerender, unmount } = render(view());
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await screen.findByRole('option', { name: 'qwen' });
  expect(changes).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  expect(changes.mock.calls).toEqual([[true]]);
  rerender(view());
  fireEvent.change(document.querySelector('[data-forge-brief]')!, { target: { value: 'more action' } });
  expect(changes.mock.calls).toEqual([[true]]);
  await act(async () => resolve(route === 'h3' ? { simple_prompt: 'draft', warnings: [] } : { prompt: 'draft', saw_images: 0 }));
  expect(changes.mock.calls).toEqual([[true], [false]]);
  unmount();
  expect(changes.mock.calls).toEqual([[true], [false]]);
  expect(forgeCalls().filter(c => c.route === 'cancel')).toHaveLength(0);
});

it.each(['cancel', 'unmount', 'failure'])('checkpoint Forge ends busy on %s and ignores late owned results', async outcome => {
  let resolve!: (value: unknown) => void, reject!: (reason: Error) => void;
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((c, a) => {
    const route = (a as { route: string }).route;
    if (c === 'native_forge' && route === 'h3') return new Promise((yes, no) => { resolve = yes; reject = no; }) as ReturnType<typeof invoke>;
    if (c === 'native_forge' && route === 'cancel') return Promise.resolve({ cancelled: true });
    return base(c, a);
  });
  const changes = vi.fn(), apply = vi.fn();
  const view = (endpoint = 'http://127.0.0.1:8188', root = '/env') => <I18nProvider><ForgeButton endpoint={endpoint} root={root} dialect="MiniMax H3" value="turn left" onApply={apply} h3={continuityRun} onBusyChange={changes}/></I18nProvider>;
  const { unmount, rerender } = render(view());
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await screen.findByRole('option', { name: 'qwen' });
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  const request_id = forgeCalls().find(c => c.route === 'h3')!.body.request_id;
  if (outcome === 'unmount') { rerender(view('http://127.0.0.1:8288', '/other')); unmount(); }
  else if (outcome === 'cancel') fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
  else await act(async () => reject(new Error('write failed')));
  expect(changes.mock.calls).toEqual([[true], [false]]);
  if (outcome !== 'failure') {
    expect(vi.mocked(invoke)).toHaveBeenCalledWith('native_forge', { baseUrl: 'http://127.0.0.1:8188', root: '/env', route: 'cancel', body: { request_id } });
    await act(async () => resolve({ simple_prompt: 'late', warnings: [] }));
    expect(screen.queryByRole('textbox', { name: 'Result' })).toBeNull();
  }
  expect(changes.mock.calls).toEqual([[true], [false]]);
  expect(apply).not.toHaveBeenCalled();
  unmount();
  expect(forgeCalls().filter(c => c.route === 'cancel')).toHaveLength(outcome === 'failure' ? 0 : 1);
});

it('continuity uses the next-action locale label and consumes Escape inside its open popover', async () => {
  const outerEscape = vi.fn();
  const catalog = { ...bundledCatalog, dictionaries: { ...bundledCatalog.dictionaries, en: { ...bundledCatalog.dictionaries.en, 'forge.nextAction': 'Next action fixture' } } };
  render(<I18nProvider catalog={catalog}><div onKeyDown={outerEscape}><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="turn left" onApply={vi.fn()} h3={continuityRun}/></div></I18nProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await screen.findByRole('option', { name: 'qwen' });
  const brief = screen.getByRole('textbox', { name: 'Next action fixture' });
  expect(fireEvent.keyDown(brief, { key: 'Escape', cancelable: true })).toBe(false);
  expect(outerEscape).not.toHaveBeenCalled();
  expect(screen.queryByRole('dialog')).toBeNull();
});

it('checkpoint Forge displays bounded source-evidence warnings and clears them on a new Write', async () => {
  const base = vi.mocked(invoke).getMockImplementation()!;
  let writes = 0;
  vi.mocked(invoke).mockImplementation((c, a) => c === 'native_forge' && (a as { route: string }).route === 'h3'
    ? ++writes === 1 ? Promise.resolve({ simple_prompt: 'draft', warnings: [null, 17, 'Tail frames unavailable; source history is text-only', 'x'.repeat(3000), ...Array.from({ length: 10 }, (_, i) => `warning-${i}`)] }) : new Promise(() => {}) as ReturnType<typeof invoke>
    : base(c, a));
  render(<I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="turn left" onApply={vi.fn()} h3={continuityRun}/></I18nProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await screen.findByRole('option', { name: 'qwen' });
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await screen.findByRole('textbox', { name: 'Result' });
  expect(screen.getByText('Tail frames unavailable; source history is text-only').className).toBe('forge-warning');
  const warnings = document.querySelectorAll('p.forge-warning');
  expect(warnings).toHaveLength(8);
  expect(warnings[1].textContent).toHaveLength(2048);
  fireEvent.click(screen.getByRole('button', { name: 'Regenerate' }));
  expect(document.querySelectorAll('p.forge-warning')).toHaveLength(0);
});

it('checkpoint Forge forwards continuity through h3 and only applies on explicit review', async () => {
  const apply = vi.fn();
  render(<I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="turn left" onApply={apply} h3={continuityRun}/></I18nProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await screen.findByRole('option', { name: 'qwen' });
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await screen.findByRole('textbox', { name: 'Result' });
  expect(forgeCalls().find(c => c.route === 'h3')!.body).toMatchObject({ continuity: continuityRun.continuity, references: [], easy: false, see_pictures: false, brief: 'turn left' });
  expect(apply).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
  expect(apply).toHaveBeenCalledExactlyOnceWith('H3 structured prompt');
  fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
  expect(apply).toHaveBeenLastCalledWith('turn left');
});

it('Keep current closes the hammer without touching the prompt', async () => {
  const apply = vi.fn();
  render(<I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="Illustrious" value="girl in rain" onApply={apply}/></I18nProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await waitFor(() => expect(screen.getByRole('option', { name: 'qwen' })).toBeTruthy());
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  fireEvent.click(await screen.findByRole('button', { name: 'Keep current' }));
  expect(screen.queryByRole('dialog')).toBeNull();
  expect(apply).not.toHaveBeenCalled();
  expect(screen.queryByRole('button', { name: 'Undo' })).toBeNull();
});

it('hammer rewrites an image prompt with the native Forge, applies after review and undoes', async () => {
  const apply = vi.fn();
  render(<I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="Anima" value="girl in rain" onApply={apply}/></I18nProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  expect((screen.getByRole('textbox', { name: 'Idea or prompt to enhance' }) as HTMLTextAreaElement).value).toBe('girl in rain');
  await waitFor(() => expect(screen.getByRole('option', { name: 'qwen' })).toBeTruthy());
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await waitFor(() => expect(screen.getByRole('textbox', { name: 'Result' })).toBeTruthy());
  // The node's Prompt Writer is not involved: the model id loses its node prefix and the brief goes to DEAS's own engine.
  expect(forgeCalls().filter(c => c.route === 'write')).toEqual([]);
  expect(nativeCalls()).toEqual([{ root: '/env', request: expect.objectContaining({ target: 'anima', model: 'qwen', brief: 'girl in rain', controls: {} }) }]);
  // The prompt in the box and the rewrite side by side; nothing changes until the new one is chosen.
  expect((screen.getByRole('textbox', { name: 'Current prompt' }) as HTMLTextAreaElement).value).toBe('girl in rain');
  expect(apply).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: 'Use new prompt' }));
  expect(apply).toHaveBeenLastCalledWith('1girl, rain');
  fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
  expect(apply).toHaveBeenLastCalledWith('girl in rain');
});

it('hammer joins the resolved wildcard picks to the idea', async () => {
  const rules = { style: 'Booru', seed: 7, reroll: 3, token_budget: 200, auto_reroll: false, selection_state: { 'Art/Light/wildcards': { enabled: true, weight: 1.2 } } } as const;
  const build = vi.fn(async (_: unknown) => ({ positive: '(red light:1.2)', negative: '' }));
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((c, a) => c === 'native_wildcard_build' ? build(a) : base(c, a));
  render(<I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="Anima" value="girl in rain" onApply={vi.fn()} onWildcards={vi.fn()} wildcard={rules}/></I18nProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  fireEvent.click(screen.getByRole('button', { name: 'LLM · Prompt Forge' }));
  await waitFor(() => expect((screen.getByRole('textbox', { name: 'Idea or prompt to enhance' }) as HTMLTextAreaElement).value).toBe('girl in rain, (red light:1.2)'));
  expect(build).toHaveBeenCalledWith({ baseUrl: 'http://127.0.0.1:8188', rules });
});

it('hammer on an image model is a text rewrite even with an image-to-image source', async () => {
  render(<I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="Krea2" value="girl in rain" image="DEAS_00001_.png [output]" onApply={vi.fn()}/></I18nProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await waitFor(() => expect(screen.getByRole('option', { name: 'qwen' })).toBeTruthy());
  expect(document.querySelector('[data-forge-picture]')).toBeNull();
  fireEvent.change(screen.getByRole('textbox', { name: 'Idea or prompt to enhance' }), { target: { value: '' } });
  expect((screen.getByRole('button', { name: 'Write' }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.change(screen.getByRole('textbox', { name: 'Idea or prompt to enhance' }), { target: { value: 'girl in rain' } });
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await screen.findByRole('button', { name: 'Use new prompt' });
  expect(forgeCalls().filter(c => c.route === 'write')).toEqual([]);
  expect(nativeCalls()).toEqual([{ root: '/env', request: expect.objectContaining({ target: 'krea2', brief: 'girl in rain', controls: {} }) }]);
});

it('hammer is disabled for a model without a prompt dialect', () => {
  render(<I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" value="" onApply={vi.fn()}/></I18nProvider>);
  expect((screen.getByRole('button', { name: 'Prompt Forge' }) as HTMLButtonElement).disabled).toBe(true);
  expect(forgeCalls()).toEqual([]);
});

it.each([
  ['http://127.0.0.1:8288', '/env'],
  ['http://127.0.0.1:8188', '/other'],
])('forge reloads models after switching endpoint/root to %s %s', async (endpoint, root) => {
  const view = (endpoint: string, root: string) => <I18nProvider><PromptForge endpoint={endpoint} root={root} onSend={vi.fn()}/></I18nProvider>;
  const { rerender } = render(view('http://127.0.0.1:8188', '/env'));
  await screen.findByRole('option', { name: 'qwen' });
  // Command-specific: a late media thumbnail call must not consume the new model list.
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (command, args) => command === 'native_llm_models' ? { models: [{ provider: 'ollama', id: 'new', name: 'new backend' }], warnings: [] } : base(command, args));
  rerender(view(endpoint, root));
  await screen.findByRole('option', { name: 'new backend' });
  expect(vi.mocked(invoke).mock.calls.filter(([c]) => c === 'native_llm_models')).toHaveLength(2);
  expect(vi.mocked(invoke).mock.calls.filter(([c]) => c === 'native_llm_models').at(-1)).toEqual(['native_llm_models', { root }]);
  expect((screen.getByRole('combobox', { name: 'LLM model' }) as HTMLSelectElement).value).toBe('ollama:new');
});

it('forge and hammer reload models after the LLM backend setting changes', async () => {
  render(<I18nProvider><PromptForge endpoint="http://127.0.0.1:8188" root="/env" onSend={vi.fn()}/><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="Anima" value="" onApply={vi.fn()}/></I18nProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await waitFor(() => expect(screen.getAllByRole('option', { name: 'qwen' })).toHaveLength(2));
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (command, args) => command === 'native_llm_models' ? { models: [{ provider: 'openai', id: 'new', name: 'new server' }], warnings: [] } : base(command, args));
  act(() => { window.dispatchEvent(new Event('deas:llm-changed')); });
  await waitFor(() => expect(screen.getAllByRole('option', { name: 'new server' })).toHaveLength(2));
  expect(vi.mocked(invoke).mock.calls.filter(([c]) => c === 'native_llm_models')).toHaveLength(4);
});

it('reloads models when the backend changes with the hammer open', async () => {
  const view = (endpoint: string) => <I18nProvider><ForgeButton endpoint={endpoint} root="/env" dialect="Anima" value="idea" onApply={vi.fn()}/></I18nProvider>;
  const { rerender } = render(view('http://127.0.0.1:8188'));
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await screen.findByRole('option', { name: 'qwen' });
  rerender(view('http://127.0.0.1:8288'));
  await screen.findByRole('option', { name: 'qwen' });
  expect(vi.mocked(invoke).mock.calls.filter(([c]) => c === 'native_llm_models')).toHaveLength(2);
});

it.each([
  ['success', 'http://127.0.0.1:8288', '/env'],
  ['error', 'http://127.0.0.1:8288', '/env'],
  ['success', 'http://127.0.0.1:8188', '/other'],
  ['error', 'http://127.0.0.1:8188', '/other'],
])('ignores a late model %s after switching to %s %s', async (outcome, endpoint, root) => {
  let resolve!: (value: unknown) => void;
  let reject!: (error: Error) => void;
  const pending = new Promise<unknown>((yes, no) => { resolve = yes; reject = no; });
  const original = vi.mocked(invoke).getMockImplementation()!;
  let first = true;
  vi.mocked(invoke).mockImplementation((command, args) => {
    if (first && command === 'native_llm_models') {
      first = false;
      return pending as ReturnType<typeof invoke>;
    }
    return original(command, args);
  });
  const view = (endpoint: string, root: string) => <I18nProvider><PromptForge endpoint={endpoint} root={root} onSend={vi.fn()}/></I18nProvider>;
  const { rerender } = render(view('http://127.0.0.1:8188', '/env'));
  await waitFor(() => expect(first).toBe(false));
  rerender(view(endpoint, root));
  await screen.findByRole('option', { name: 'qwen' });
  await act(async () => {
    if (outcome === 'success') resolve({ models: [{ provider: 'ollama', id: 'old', name: 'old backend' }], warnings: [] });
    else reject(new Error('old backend failed'));
  });
  expect(screen.queryByRole('option', { name: 'old backend' })).toBeNull();
  expect(screen.queryByRole('alert')).toBeNull();
  expect((screen.getByRole('combobox', { name: 'LLM model' }) as HTMLSelectElement).value).toBe('ollama:qwen');
  expect(vi.mocked(invoke).mock.calls.filter(([c]) => c === 'native_llm_models')).toHaveLength(2);
});

it('Forge uses the shared media controls and roots without changing reference or execution rules', async () => {
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === 'native_media_list') {
      const { path } = args as { path: string };
      return { path, parent: null, truncated: false, entries: [
        { name: 'a.png', path: `${path}/a.png`, kind: 'file', symlink: false, media: 'image', size: 1, modified: 1 },
        { name: 'b.png', path: `${path}/b.png`, kind: 'file', symlink: false, media: 'image', size: 9, modified: 3 },
        { name: 'clip.mp4', path: `${path}/clip.mp4`, kind: 'file', symlink: false, media: 'video', size: 4, modified: 2 },
      ] };
    }
    return base(command, args);
  });
  // The image models use the native editor (pictures via Reference images); the video models keep this panel.
  localStorage.setItem('deas.pref.forge.target', JSON.stringify('Wan 2.2'));
  const { container } = render(<I18nProvider><PromptForge endpoint="http://127.0.0.1:8188" root="/env" inputDir="/shared-forge/input" outputDir="/shared-forge/output" onSend={vi.fn()}/></I18nProvider>);
  await screen.findByRole('option', { name: 'qwen' });
  const panel = container.querySelector('.forge-media')!;
  const files = () => [...panel.querySelectorAll('.media-list button[data-kind="file"]')].map(e => e.textContent?.replace(/^[▶♪]/, ''));
  await waitFor(() => expect(files()).toEqual(['b.png', 'clip.mp4', 'a.png']));
  expect(panel.querySelectorAll('[data-dock-search]')).toHaveLength(1);
  expect(panel.querySelectorAll('[data-dock-sort]')).toHaveLength(1);
  expect(panel.querySelectorAll('[data-dock-type]')).toHaveLength(1);
  expect(panel.querySelectorAll('[data-dock-view]')).toHaveLength(1);
  expect(panel.querySelectorAll('[data-media-root]')).toHaveLength(3);
  fireEvent.change(panel.querySelector('[data-dock-type]')!, { target: { value: 'image' } });
  fireEvent.change(panel.querySelector('[data-dock-sort]')!, { target: { value: 'name' } });
  fireEvent.change(panel.querySelector('[data-dock-view]')!, { target: { value: 'list' } });
  expect(files()).toEqual(['a.png', 'b.png']);
  expect(panel.querySelector('.media-library')?.getAttribute('data-view')).toBe('list');
  fireEvent.change(panel.querySelector('[data-dock-search]')!, { target: { value: 'a.' } });
  expect(files()).toEqual(['a.png']);
  expect(screen.queryAllByRole('button', { name: 'Remove picture' })).toHaveLength(0);
  fireEvent.click(panel.querySelector('[data-media-root="input"]')!);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('native_media_list', { path: '/shared-forge/input', depth: 3 }));
  await waitFor(() => expect(panel.querySelector('[data-media-root="input"]')?.getAttribute('aria-pressed')).toBe('true'));
  fireEvent.click(panel.querySelector('.media-list button[data-kind="file"]')!);
  fireEvent.click(panel.querySelector('.media-list button[data-kind="file"]')!);
  expect(screen.getAllByRole('button', { name: 'Remove picture' })).toHaveLength(1);
  fireEvent.click(panel.querySelector('[data-media-root="output"]')!);
  await waitFor(() => expect(panel.querySelector('[data-media-root="output"]')?.getAttribute('aria-pressed')).toBe('true'));
  fireEvent.click(panel.querySelector('.media-list button[data-kind="file"]')!);
  expect(screen.getAllByRole('button', { name: 'Remove picture' })).toHaveLength(2);
  expect(forgeCalls().filter(c => c.route !== 'models')).toHaveLength(0);
  expect(vi.mocked(invoke).mock.calls.some(([c]) => ['native_input_upload', 'native_job_submit'].includes(c))).toBe(false);
  expect(localStorage.getItem('deas.pref.forge.media.view')).toBe('"list"');
  expect(localStorage.getItem('deas.pref.studio.dock.view')).toBeNull();
});

it.each([
 ['/env/input', '/env/input/a.png'],
 ['/env/input/', '/env/input/a.png'],
 ['C:\\env\\input\\', 'C:\\env\\input\\a.png'],
 ['C:/env/input/', 'C:\\env\\input\\a.png'],
])('forge tab writes H3 REF2VA from labelled input pictures and sends the result to Generate (%s)', async (inputDir, path) => {
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((command, args) => command === 'native_media_list' ? Promise.resolve({path:inputDir,parent:null,truncated:false,entries:[{name:'a.png',path,kind:'file',symlink:false,media:'image',size:1,modified:1}]}) : base(command, args));
  const send = vi.fn();
  render(<I18nProvider><PromptForge endpoint="http://127.0.0.1:8188" root="/env" inputDir={inputDir} onSend={send}/></I18nProvider>);
  fireEvent.change(screen.getByRole('combobox', { name: 'Write for' }), { target: { value: 'MiniMax H3' } });
  fireEvent.change(screen.getByRole('combobox', { name: 'Mode' }), { target: { value: 'REF2VA' } });
  fireEvent.click(await screen.findByText('a.png'));
  expect(document.querySelector('.media-list li[data-mark]')?.getAttribute('data-mark')).toBe('Character 1');
  fireEvent.change(screen.getByRole('combobox', { name: 'a.png' }), { target: { value: 'place' } });
  fireEvent.change(screen.getByRole('textbox', { name: 'Idea or prompt to enhance' }), { target: { value: 'Character 1 walks in' } });
  await waitFor(() => expect(screen.getByRole('option', { name: 'qwen' })).toBeTruthy());
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await screen.findByText('check cut');
  expect(forgeCalls().at(-1)).toMatchObject({ route: 'h3', body: { mode: 'REF2VA', creativity: 'balanced', easy: true, references: [{ kind: 'image', role: 'subject', path: 'a.png', easy_role: 'place' }] } });
  fireEvent.click(screen.getByRole('button', { name: 'Send to Generate' }));
  expect(send).toHaveBeenCalledWith('H3 structured prompt', '');
});

it.each(['inputDir', 'outputDir'] as const)('standalone Forge clears picture identity and invalidates drafts when %s changes', async folder => {
  localStorage.setItem('deas.pref.forge.target', JSON.stringify('MiniMax H3'));
  localStorage.setItem('deas.pref.forge.h3.mode', JSON.stringify('I2VA'));
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((command, args) => command === 'native_media_list' ? Promise.resolve({ path: '/env/media', parent: null, truncated: false, entries: [
    { name: 'same.png', path: '/env/media/same.png', kind: 'file', symlink: false, media: 'image', size: 1, modified: 1 },
  ] }) : base(command, args));
  const view = (path: string) => <I18nProvider><PromptForge endpoint="http://127.0.0.1:8188" root="/env" {...{[folder]: path}} onSend={vi.fn()}/></I18nProvider>;
  const { container, rerender } = render(view('/env/media'));
  fireEvent.click(await screen.findByText('same.png'));
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await screen.findByText('check cut');
  expect((container.querySelector('[data-forge-send]') as HTMLButtonElement).disabled).toBe(false);
  rerender(view('/env/other-media'));
  expect(screen.queryByRole('button', { name: 'Remove picture' })).toBeNull();
  expect((container.querySelector('[data-forge-send]') as HTMLButtonElement).disabled).toBe(true);
  expect((screen.getByRole('button', { name: 'Write' }) as HTMLButtonElement).disabled).toBe(true);
  expect(forgeCalls().filter(c => c.route === 'h3')).toHaveLength(1);
});

it.each(['I2VA', 'FL2VA', 'L2VA', 'REF2VA'])('standalone H3 %s shows truthful vision controls for selected output pictures', async mode => {
  localStorage.setItem('deas.pref.forge.target', JSON.stringify('MiniMax H3'));
  localStorage.setItem('deas.pref.forge.h3.mode', JSON.stringify(mode));
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((command, args) => command === 'native_media_list' ? Promise.resolve({ path: '/env/output', parent: null, truncated: false, entries: [
    { name: 'first.png', path: '/env/output/scenes/first.png', kind: 'file', symlink: false, media: 'image', size: 1, modified: 2 },
    { name: 'last.png', path: '/env/output/scenes/last.png', kind: 'file', symlink: false, media: 'image', size: 1, modified: 1 },
  ] }) : base(command, args));
  const { container } = render(<I18nProvider><PromptForge endpoint="http://127.0.0.1:8188" root="/env" outputDir="/env/output/" onSend={vi.fn()}/></I18nProvider>);
  expect(screen.queryByRole('checkbox')).toBeNull();
  fireEvent.click(await screen.findByText('first.png'));
  if (mode === 'FL2VA') fireEvent.click(screen.getByText('last.png'));
  const toggle = screen.queryByRole('checkbox', { name: 'Let the model see the pictures (needs a vision model)' }) as HTMLInputElement | null;
  expect(!!toggle).toBe(mode === 'REF2VA');
  if (toggle) expect(toggle.checked).toBe(false);
  expect(!!container.querySelector('[data-forge-auto-see]')).toBe(mode !== 'REF2VA');
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await screen.findByText('check cut');
  const references = ['first', ...(mode === 'FL2VA' ? ['last'] : [])].map(name => ({ kind: 'image', path: `scenes/${name}.png [output]`, ...(mode === 'REF2VA' ? { role: 'subject', easy_role: 'character-1' } : { role: 'keyframe' }) }));
  expect(forgeCalls().at(-1)).toMatchObject({ route: 'h3', body: { mode, easy: mode === 'REF2VA', see_pictures: false, references } });
  if (toggle) fireEvent.click(toggle);
  else fireEvent.change(container.querySelector('[data-forge-brief]')!, { target: { value: 'walk' } });
  expect((container.querySelector('[data-forge-send]') as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await waitFor(() => expect(forgeCalls().filter(c => c.route === 'h3')).toHaveLength(2));
  expect(forgeCalls().at(-1)!.body).toMatchObject({ easy: mode === 'REF2VA', see_pictures: mode === 'REF2VA', references });
  fireEvent.click(screen.getAllByRole('button', { name: 'Remove picture' })[0]);
  if (mode === 'FL2VA') fireEvent.click(screen.getByRole('button', { name: 'Remove picture' }));
  expect(screen.queryByRole('checkbox')).toBeNull();
  expect(container.querySelector('[data-forge-auto-see]')).toBeNull();
  if (mode === 'REF2VA') {
    fireEvent.change(container.querySelector('[data-forge-brief]')!, { target: { value: 'text only' } });
    fireEvent.click(screen.getByRole('button', { name: 'Write' }));
    await waitFor(() => expect(forgeCalls().filter(c => c.route === 'h3')).toHaveLength(3));
    expect(forgeCalls().at(-1)!.body).toMatchObject({ easy: false, see_pictures: false, references: [] });
  }
});

it('maps a missing node-pack route (404, or 405 from ComfyUI static) to the upgrade hint', async () => {
  const { forgeError } = await import('../../lib/forge');
  expect(forgeError(new Error('405'))).toBe('forge.outdated');
  expect(forgeError('404 Not Found')).toBe('forge.outdated');
  expect(forgeError('4050 x')).toBe('4050 x');
  expect(forgeError('500 boom')).toBe('500 boom');
});

it('H3 hammer is the Director Forge for the run: context, shots, detail/creativity, cancel, drafts that go stale with the run', async () => {
  const apply = vi.fn();
  const run = { mode: 'REF2VA', duration: 5, canvas: { width: 1024, height: 576 }, references: [{ kind: 'image' as const, path: 'a.png', role: 'subject', subject_group: '' }, { kind: 'video' as const, role: 'motion', stream: 'both' as const }], easy: false };
  const base = vi.mocked(invoke).getMockImplementation()!;
  let release: (v: unknown) => void = () => {};
  let hold = false;
  vi.mocked(invoke).mockImplementation((c, a) => {
    const { route } = a as { route: string };
    if (c === 'native_forge' && route === 'cancel') return Promise.resolve({ cancelled: true });
    if (c === 'native_forge' && route === 'h3' && hold) return new Promise(r => { release = r; });
    return base(c, a);
  });
  const view = (h3: typeof run) => <I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="a duel" onApply={apply} h3={h3}/></I18nProvider>;
  const { rerender } = render(view(run));
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await waitFor(() => expect(screen.getByRole('option', { name: 'qwen' })).toBeTruthy());
  expect(document.querySelector('[data-forge-run]')!.textContent).toBe('REF2VA · 5 s · 1024×576 · 2 references');
  fireEvent.change(document.querySelector('[data-forge-shots]')!, { target: { value: '2' } });
  fireEvent.change(document.querySelector('[data-forge-shot="2"]')!, { target: { value: 'close-up' } });
  fireEvent.change(screen.getByRole('slider', { name: 'Creativity 5' }), { target: { value: '9' } });
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await waitFor(() => expect(screen.getByRole('textbox', { name: 'Result' })).toBeTruthy());
  const body = forgeCalls().find(c => c.route === 'h3')!.body;
  expect(body).toMatchObject({ model: 'ollama:qwen', mode: 'REF2VA', brief: 'a duel', duration: 5, shots: 2, shot_briefs: ['', 'close-up'], detail: 5, creativity: 'daring', references: run.references, easy: false, output_canvas: { width: 1024, height: 576 } });
  expect(body).not.toHaveProperty('continuity');
  expect(typeof body.request_id).toBe('string');
  // Another canvas: the draft no longer fits, Apply waits for a new draft; the history marks it.
  rerender(view({ ...run, canvas: { width: 576, height: 1024 } }));
  expect(screen.getByRole('status').textContent).toContain('written for other settings');
  expect((screen.getByRole('button', { name: 'Apply' }) as HTMLButtonElement).disabled).toBe(true);
  expect(screen.getByRole('button', { name: 'Draft 1 ⚠' })).toBeTruthy();
  // Regenerate while the LLM runs can be cancelled with the same request id.
  hold = true;
  fireEvent.click(screen.getByRole('button', { name: 'Regenerate' }));
  fireEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
  const pending = forgeCalls().filter(c => c.route === 'h3').at(-1)!.body.request_id;
  expect(forgeCalls().at(-1)).toMatchObject({ route: 'cancel', body: { request_id: pending } });
  await act(async () => { release({ simple_prompt: 'late', warnings: [] }); });
  expect((screen.getByRole('textbox', { name: 'Result' }) as HTMLTextAreaElement).value).toBe('H3 structured prompt'); // a cancelled answer is dropped
  hold = false;
  fireEvent.click(screen.getByRole('button', { name: 'Regenerate' }));
  await waitFor(() => expect(screen.getAllByRole('button', { name: /^Draft \d/ }).length).toBe(2));
  fireEvent.click(screen.getByRole('button', { name: 'Apply' }));
  expect(apply).toHaveBeenLastCalledWith('H3 structured prompt');
});

it('H3 hammer keeps labelled-picture semantics when see pictures is enabled', async () => {
  const run = { mode: 'REF2VA', duration: 5, canvas: { width: 1024, height: 576 }, references: [{ kind: 'image' as const, path: 'a.png', role: 'subject', subject_group: 'A', easy_role: 'character-1' }], easy: true };
  const view = (h3: H3Context = run) => <I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="a walk" onApply={vi.fn()} h3={h3}/></I18nProvider>;
  const { rerender } = render(view());
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await waitFor(() => expect(screen.getByRole('option', { name: 'qwen' })).toBeTruthy());
  const toggle = screen.getByRole('checkbox', { name: 'Let the model see the pictures (needs a vision model)' }) as HTMLInputElement;
  expect(toggle.checked).toBe(false);
  fireEvent.click(toggle);
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await waitFor(() => expect(screen.getByRole('textbox', { name: 'Result' })).toBeTruthy());
  expect(forgeCalls().find(c => c.route === 'h3')!.body).toMatchObject({ easy: true, see_pictures: true, references: run.references });
  rerender(view({ ...run, references: [{ ...run.references[0], path: '' }] }));
  expect(screen.queryByRole('checkbox')).toBeNull();
  expect((screen.getByRole('button', { name: 'Apply' }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole('button', { name: 'Regenerate' }));
  await waitFor(() => expect(forgeCalls().filter(c => c.route === 'h3')).toHaveLength(2));
  expect(forgeCalls().at(-1)!.body).toMatchObject({ easy: true, see_pictures: false });
});

it.each(['I2VA', 'FL2VA', 'L2VA', 'REF2VA'])('H3 %s shows automatic vision for readable frames or mixed references without a fake opt-out', async mode => {
  const references: H3Context['references'] = [
    { kind: 'image', role: mode === 'REF2VA' ? 'subject' : 'keyframe', path: 'first.png' },
    ...(mode === 'FL2VA' ? [{ kind: 'image' as const, role: 'keyframe', path: 'last.png' }] : []),
    ...(mode === 'REF2VA' ? [{ kind: 'video' as const, role: 'motion', path: 'motion.mp4' }] : []),
  ];
  // The easy flag must not turn keyframe modes into labelled REF2VA.
  const run: H3Context = { mode, duration: 5, canvas: { width: 1024, height: 576 }, references, easy: mode !== 'REF2VA' };
  const view = (h3 = run) => <I18nProvider><ForgeButton inline endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="a walk" onApply={vi.fn()} h3={h3}/></I18nProvider>;
  const { rerender } = render(view());
  await screen.findByRole('option', { name: 'qwen' });
  expect(screen.queryByRole('checkbox')).toBeNull();
  expect(document.querySelector('[data-forge-auto-see]')?.textContent).toContain('automatically');
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await screen.findByRole('button', { name: 'Regenerate' });
  // This is the request flag, not proof of an opt-out: the nodepack attaches normal references automatically.
  expect(forgeCalls().at(-1)!.body).toMatchObject({ easy: run.easy, see_pictures: false, references });
  rerender(view({ ...run, references: [{ kind: 'image', role: 'subject' }, { kind: 'video', path: 'motion.mp4', role: 'motion' }] }));
  expect(document.querySelector('[data-forge-auto-see]')).toBeNull();
  expect(screen.queryByRole('checkbox')).toBeNull();
  expect((screen.getByRole('button', { name: 'Apply' }) as HTMLButtonElement).disabled).toBe(true);
});

it.each(['success', 'error', 'cancel error'])('cancelled H3 request A cannot disturb pending B after late A %s', async outcome => {
  const pending = () => {
    let resolve!: (value: unknown) => void, reject!: (error: Error) => void;
    const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
    return { promise, resolve, reject };
  };
  const a = pending(), b = pending(), cancellation = pending();
  const base = vi.mocked(invoke).getMockImplementation()!;
  let writes = 0;
  vi.mocked(invoke).mockImplementation((c, args) => {
    const route = (args as { route: string }).route;
    if (c === 'native_forge' && route === 'h3') return (++writes === 1 ? a : b).promise as ReturnType<typeof invoke>;
    if (c === 'native_forge' && route === 'cancel') return cancellation.promise as ReturnType<typeof invoke>;
    return base(c, args);
  });
  const run = { mode: 'T2VA', duration: 5, canvas: { width: 64, height: 64 }, references: [], easy: false };
  render(<I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="a walk" onApply={vi.fn()} h3={run}/></I18nProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await screen.findByRole('option', { name: 'qwen' });
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));

  await act(async () => {
    if (outcome === 'success') a.resolve({ simple_prompt: 'stale A', warnings: [] });
    else if (outcome === 'error') a.reject(new Error('stale A error'));
    else cancellation.reject(new Error('stale cancel error'));
  });
  expect(screen.queryByRole('alert')).toBeNull();
  expect(screen.getByRole('button', { name: 'Cancel' })).toBeTruthy();
  expect(screen.queryByRole('textbox', { name: 'Result' })).toBeNull();
  await act(async () => { b.resolve({ simple_prompt: 'current B', warnings: [] }); });
  expect((screen.getByRole('textbox', { name: 'Result' }) as HTMLTextAreaElement).value).toBe('current B');
  expect(screen.getAllByRole('button', { name: /^Draft \d/ })).toHaveLength(1);
  // A's cancel failure must also stay stale after B has finished.
  await act(async () => { cancellation.reject(new Error('late cancel error')); });
  expect(screen.queryByRole('alert')).toBeNull();
});

it('reports a cancellation failure while the cancelled request still owns the UI', async () => {
  const base = vi.mocked(invoke).getMockImplementation()!;
  let reject!: (error: Error) => void;
  const cancellation = new Promise((_, no) => { reject = no; });
  vi.mocked(invoke).mockImplementation((c, args) => {
    const route = (args as { route: string }).route;
    if (c === 'native_forge' && route === 'h3') return new Promise(() => {}) as ReturnType<typeof invoke>;
    if (c === 'native_forge' && route === 'cancel') return cancellation as ReturnType<typeof invoke>;
    return base(c, args);
  });
  render(<I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="a walk" onApply={vi.fn()} h3={{ mode: 'T2VA', duration: 5, canvas: { width: 64, height: 64 }, references: [], easy: false }}/></I18nProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await screen.findByRole('option', { name: 'qwen' });
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
  await act(async () => { reject(new Error('cancel failed')); });
  expect(screen.getByRole('alert').textContent).toBe('cancel failed');
  expect((screen.getByRole('button', { name: 'Write' }) as HTMLButtonElement).disabled).toBe(false);
});

it.each(['REF2VA', 'I2VA', 'FL2VA', 'L2VA'].flatMap(mode => ['replace', 'reorder'].map(change => [mode, change])))('discards a %s Cast read after reference %s, including stale errors and cleanup during a new read', async (mode, change) => {
  const pending: { resolve: (value: unknown) => void; reject: (error: Error) => void }[] = [];
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((c, args) => c === 'native_forge' && (args as { route: string }).route === 'cast'
    ? new Promise((resolve, reject) => { pending.push({ resolve, reject }); }) as ReturnType<typeof invoke> : base(c, args));
  const refs = [{ kind: 'image' as const, path: 'a.png', easy_role: 'character-1', who:[1] }, { kind: 'image' as const, path: 'b.png', easy_role: 'character-2', who:[2] }];
  const run = { mode, duration: 5, canvas: { width: 64, height: 64 }, references: refs, easy: mode === 'REF2VA' };
  const view = (references: typeof refs) => <I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="a walk" onApply={vi.fn()} h3={{ ...run, references }}/></I18nProvider>;
  const { rerender } = render(view(refs));
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await screen.findByRole('option', { name: 'qwen' });
  const read = () => document.querySelector('[data-forge-read="1"]') as HTMLButtonElement;
  fireEvent.change(screen.getByLabelText('Card of Character 1'), { target: { value: 'manual card' } });
  fireEvent.click(read());
  const changed = change === 'replace' ? [{ ...refs[0], path: 'new.png' }, refs[1]] : [...refs].reverse();
  rerender(view(changed));
  expect(read().disabled).toBe(false);
  fireEvent.click(read());
  await act(async () => { pending[0].resolve({ text: 'stale card' }); });
  expect((screen.getByLabelText('Card of Character 1') as HTMLTextAreaElement).value).toBe('manual card');
  expect(read().disabled).toBe(true); // the new read still owns the busy state
  rerender(view(refs));
  fireEvent.click(read());
  await act(async () => { pending[1].reject(new Error('stale read error')); });
  expect(screen.queryByRole('alert')).toBeNull();
  expect(read().disabled).toBe(true);
  await act(async () => { pending[2].resolve({ text: 'current card' }); });
  expect((screen.getByLabelText('Card of Character 1') as HTMLTextAreaElement).value).toBe('current card');
  expect(read().disabled).toBe(false);
});

it('H3 group cards deduplicate characters without changing reference order, support Read and exclude removed characters from Write', async () => {
 const refs = [{ kind: 'image' as const, path: 'pair.png', easy_role: 'group-21' }, { kind: 'image' as const, path: 'trio.png', easy_role: 'group-123' }];
 const base = vi.mocked(invoke).getMockImplementation()!;
 vi.mocked(invoke).mockImplementation((c, a) => c === 'native_forge' && (a as { route: string }).route === 'cast' ? Promise.resolve({ text: 'adult woman, red coat' }) : base(c, a));
 const view = (references: typeof refs) => <I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="a meeting" onApply={vi.fn()} h3={{ mode:'REF2VA', duration:5, canvas:{width:64,height:64}, references, easy:true }}/></I18nProvider>;
 const { rerender } = render(view(refs));
 fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
 await screen.findByRole('option', { name: 'qwen' });
 expect([...document.querySelectorAll('[data-forge-read]')].map(b => b.getAttribute('data-forge-read'))).toEqual(['1', '2', '3']);
 fireEvent.click(document.querySelector('[data-forge-read="2"]')!);
 await waitFor(() => expect((screen.getByLabelText('Card of Character 2') as HTMLTextAreaElement).value).toBe('adult woman, red coat'));
 expect(forgeCalls().find(c => c.route === 'cast')!.body).toMatchObject({ character:2, references:refs });
 fireEvent.change(screen.getByLabelText('Card of Character 3'), { target:{value:'no longer present'} });
 rerender(view([refs[0]]));
 expect(screen.queryByLabelText('Card of Character 3')).toBeNull();
 fireEvent.click(screen.getByRole('button', { name:'Write' }));
 await screen.findByRole('textbox', { name:'Result' });
 expect(forgeCalls().find(c => c.route === 'h3')!.body.references).toEqual([refs[0]]);
 expect(forgeCalls().find(c => c.route === 'h3')!.body.cast).toEqual({2:'adult woman, red coat'});
});

it.each(['who','boxes'])('H3 explicit %s overrides the legacy label when choosing Cast cards',async source=>{
 render(<I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="adults meet" onApply={vi.fn()} h3={{mode:'REF2VA',duration:5,canvas:{width:64,height:64},easy:true,references:[{kind:'image',easy_role:'character-1',...(source==='boxes'?{boxes:[{n:4,box:[0.1,0.1,0.4,0.8] as [number,number,number,number]},{n:2,box:[0.6,0.1,0.9,0.8] as [number,number,number,number]}]}:{picture_kinds:['character'],who:[4,2],who_axis:'z' as const})}]}}/></I18nProvider>);
 fireEvent.click(screen.getByRole('button',{name:'Prompt Forge'}));
 await screen.findByRole('option',{name:'qwen'});
 expect([...document.querySelectorAll('[data-forge-read]')].map(b=>b.getAttribute('data-forge-read'))).toEqual(['2','4']);
});

it.each(['I2VA', 'FL2VA', 'L2VA'])('H3 %s frame Cast uses explicit identities for Read/edit/Write without changing references', async mode => {
  const refs = [
    { kind:'image' as const, path:'first.png', role:'first', who:[9], who_axis:'z' as const, boxes:[{n:4,box:[0.1,0.1,0.4,0.8] as [number,number,number,number]}, {n:2,box:[0.6,0.1,0.9,0.8] as [number,number,number,number]}] },
    { kind:'image' as const, path:'last.png', role:'last', who:[3,2], boxes:[], who_axis:'y' as const },
    { kind:'image' as const, path:'unlabelled.png', easy_role:'character-1' },
    { kind:'image' as const, who:[5] },
    { kind:'image' as const, path:'', who:[6] },
    { kind:'video' as const, path:'motion.mp4', who:[7] },
    { kind:'audio' as const, path:'voice.wav', who:[8] },
    ...['place','style','pose'].map(easy_role => ({kind:'image' as const,path:`${easy_role}.png`,easy_role})),
  ];
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((c,a) => c === 'native_forge' && (a as {route:string}).route === 'cast' ? Promise.resolve({text:'adult in a red coat'}) : base(c,a));
  const view = (easy=false, references=refs) => <I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="walk" onApply={vi.fn()} h3={{mode,duration:5,canvas:{width:64,height:64},references,easy}}/></I18nProvider>;
  const {rerender} = render(view());
  fireEvent.click(screen.getByRole('button',{name:'Prompt Forge'}));
  await screen.findByRole('option',{name:'qwen'});
  const keys = () => [...document.querySelectorAll('[data-forge-read]')].map(b => b.getAttribute('data-forge-read'));
  expect(keys()).toEqual(['2','3','4']);
  fireEvent.click(document.querySelector('[data-forge-read="2"]')!);
  await waitFor(() => expect((screen.getByLabelText('Card of Character 2') as HTMLTextAreaElement).value).toBe('adult in a red coat'));
  expect(forgeCalls().find(c => c.route === 'cast')!.body).toMatchObject({mode,character:2,references:refs,redo:false});
  fireEvent.change(screen.getByLabelText('Card of Character 4'),{target:{value:'  edited adult  '}});
  fireEvent.click(screen.getByRole('button',{name:'Write'}));
  await screen.findByRole('textbox',{name:'Result'});
  expect(forgeCalls().filter(c => c.route === 'h3').at(-1)!.body).toMatchObject({mode,references:refs,easy:false,cast:{2:'adult in a red coat',4:'edited adult'}});
  rerender(view(true));
  expect(keys()).toEqual(['2','3','4']);
  rerender(view(false,[refs[1]]));
  expect(keys()).toEqual(['2','3']);
  expect((screen.getByRole('button',{name:'Apply'}) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole('button',{name:'Regenerate'}));
  await waitFor(() => expect(forgeCalls().filter(c => c.route === 'h3')).toHaveLength(2));
  expect(forgeCalls().filter(c => c.route === 'h3').at(-1)!.body.cast).toEqual({2:'adult in a red coat'});
  rerender(view(false,refs.slice(2)));
  expect(document.querySelector('[data-forge-cast]')).toBeNull();
  fireEvent.click(screen.getByRole('button',{name:'Regenerate'}));
  await waitFor(() => expect(forgeCalls().filter(c => c.route === 'h3')).toHaveLength(3));
  expect(forgeCalls().filter(c => c.route === 'h3').at(-1)!.body).not.toHaveProperty('cast');
});

it('H3 REF2VA excludes pose takers and custom identities from numeric Cast while retaining scene and pose cards', async () => {
  const refs = [
    {kind:'image' as const,path:'person.png',easy_role:'character-2'},
    {kind:'image' as const,path:'pose.png',easy_role:'pose',picture_kinds:['pose','character'],who:[5],boxes:[{n:6,box:[0.1,0.1,0.9,0.9] as [number,number,number,number]}]},
    {kind:'image' as const,path:'custom.png',easy_role:'custom',picture_kinds:['character'],who:[7],boxes:[{n:8,box:[0.1,0.1,0.9,0.9] as [number,number,number,number]}]},
    {kind:'image' as const,path:'pose-kind.png',picture_kinds:['pose','character'],who:[9]},
    {kind:'image' as const,path:'custom-kind.png',picture_kinds:['custom','character'],who:[10]},
    {kind:'image' as const,path:'place.png',easy_role:'place'},
    {kind:'image' as const,path:'style.png',easy_role:'style'},
  ];
  render(<I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="dance" onApply={vi.fn()} h3={{mode:'REF2VA',duration:5,canvas:{width:64,height:64},references:refs,easy:true}}/></I18nProvider>);
  fireEvent.click(screen.getByRole('button',{name:'Prompt Forge'}));
  await screen.findByRole('option',{name:'qwen'});
  expect([...document.querySelectorAll('[data-forge-read]')].map(b => b.getAttribute('data-forge-read'))).toEqual(['2','place','style','pose:pose.png']);
});

it('H3 pose cards follow their picture across reorder and send its current picture number', async () => {
  const refs = [{ kind:'image' as const, path:'person.png', easy_role:'character-1' }, { kind:'image' as const, path:'pose.png', easy_role:'pose' }];
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((c, a) => c === 'native_forge' && (a as {route:string}).route === 'cast' ? Promise.resolve({text:'arms raised'}) : base(c, a));
  const view = (references: typeof refs) => <I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="a dance" onApply={vi.fn()} h3={{mode:'REF2VA',duration:5,canvas:{width:64,height:64},references,easy:true}}/></I18nProvider>;
  const { rerender } = render(view(refs));
  fireEvent.click(screen.getByRole('button', {name:'Prompt Forge'}));
  await screen.findByRole('option', {name:'qwen'});
  const card = await screen.findByRole('textbox', {name:'Card of Pose · Picture 2'});
  fireEvent.click(card.closest('label')!.querySelector('button')!);
  await waitFor(() => expect((card as HTMLTextAreaElement).value).toBe('arms raised'));
  expect(forgeCalls().find(c => c.route === 'cast')!.body).toMatchObject({subject:'pose@2',mode:'REF2VA',redo:false,references:refs});
  rerender(view([...refs].reverse()));
  expect((screen.getByRole('textbox', {name:'Card of Pose · Picture 1'}) as HTMLTextAreaElement).value).toBe('arms raised');
  fireEvent.click(screen.getByRole('button', {name:'Write'}));
  await screen.findByRole('textbox', {name:'Result'});
  expect(forgeCalls().filter(c => c.route === 'h3').at(-1)!.body.cast).toEqual({'pose@1':'arms raised'});
  rerender(view([refs[0]]));
  expect(screen.queryByRole('textbox', {name:/Card of Pose/})).toBeNull();
  fireEvent.click(screen.getByRole('button', {name:'Regenerate'}));
  await waitFor(() => expect(forgeCalls().filter(c => c.route === 'h3')).toHaveLength(2));
  expect(forgeCalls().filter(c => c.route === 'h3').at(-1)!.body).not.toHaveProperty('cast');
});

it('H3 pose cards omit unreadable pictures and reject stale reads after reference changes', async () => {
  const refs = [{kind:'image' as const,path:'one.png',easy_role:'pose'}, {kind:'image' as const,easy_role:'pose'}, {kind:'image' as const,path:'two.png',easy_role:'pose'}];
  let resolve!: (value: unknown) => void;
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((c,a) => c === 'native_forge' && (a as {route:string}).route === 'cast' ? new Promise(r => {resolve=r;}) as ReturnType<typeof invoke> : base(c,a));
  const view = (references: typeof refs, easy=true) => <I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="dance" onApply={vi.fn()} h3={{mode:'REF2VA',duration:5,canvas:{width:64,height:64},references,easy}}/></I18nProvider>;
  const {rerender} = render(view(refs));
  fireEvent.click(screen.getByRole('button',{name:'Prompt Forge'}));
  await screen.findByRole('option',{name:'qwen'});
  expect(screen.queryByRole('textbox',{name:'Card of Pose · Picture 2'})).toBeNull();
  const card = screen.getByRole('textbox',{name:'Card of Pose · Picture 3'});
  fireEvent.click(card.closest('label')!.querySelector('button')!);
  rerender(view([refs[2],refs[0]]));
  await act(async () => resolve({text:'stale stance'}));
  expect((screen.getByRole('textbox',{name:'Card of Pose · Picture 1'}) as HTMLTextAreaElement).value).toBe('');
  rerender(view(refs,false));
  expect(document.querySelector('[data-forge-cast]')).toBeNull();
});

it('H3 cast cards: one per labelled Character / place, Read fills a card, filled cards travel as cast', async () => {
  const refs = [{ kind: 'image' as const, path: 'b.png', role: 'subject', subject_group: 'B', easy_role: 'character-2' }, { kind: 'image' as const, path: 'a.png', role: 'subject', subject_group: 'A', easy_role: 'character-1' }, { kind: 'image' as const, path: 'p.png', role: 'subject', subject_group: '', easy_role: 'place' }];
  const run = { mode: 'REF2VA', duration: 5, canvas: { width: 64, height: 64 }, references: refs, easy: true };
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((c, a) => c === 'native_forge' && (a as { route: string }).route === 'cast' ? Promise.resolve({ text: 'tall woman, red coat' }) : base(c, a));
  render(<I18nProvider><ForgeButton endpoint="http://127.0.0.1:8188" root="/env" dialect="MiniMax H3" value="a duel" onApply={vi.fn()} h3={run}/></I18nProvider>);
  fireEvent.click(screen.getByRole('button', { name: 'Prompt Forge' }));
  await waitFor(() => expect(screen.getByRole('option', { name: 'qwen' })).toBeTruthy());
  expect([...document.querySelectorAll('[data-forge-read]')].map(b => b.getAttribute('data-forge-read'))).toEqual(['1', '2', 'place']);
  fireEvent.click(document.querySelector('[data-forge-read="1"]')!);
  await waitFor(() => expect((screen.getByLabelText('Card of Character 1') as HTMLTextAreaElement).value).toBe('tall woman, red coat'));
  expect(forgeCalls().find(c => c.route === 'cast')!.body).toMatchObject({ model: 'ollama:qwen', mode: 'REF2VA', character: 1, references: refs, brief: 'a duel', redo: false });
  fireEvent.change(screen.getByLabelText('Card of Place'), { target: { value: '  old  harbour ' } });
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await waitFor(() => expect(screen.getByRole('textbox', { name: 'Result' })).toBeTruthy());
  expect(forgeCalls().find(c => c.route === 'h3')!.body.cast).toEqual({ 1: 'tall woman, red coat', place: 'old  harbour' });
  expect([...document.querySelectorAll('[data-forge-read]')].map(b => b.textContent)).toEqual(['Read again', 'Read', 'Read again']); // filled cards are read again (`redo`)
});

const forgeCanvasContext = { key: 'studio', effective: { width: 1024, height: 1024 }, model: { width: 1024, height: 1024 }, grid: 16, resolution: 'model', megapixels: 1, fixed: { width: 1024, height: 1024 } };
it.each(['Illustrious','Anima','Krea2','MOSS'])('uses every Settings aspect in ordinary Prompt Forge modes (%s)', async target => {
  localStorage.setItem('deas.pref.forge.target', JSON.stringify(target));
  render(<I18nProvider><PromptForge endpoint="" root="/env" onSend={vi.fn()} canvasContext={forgeCanvasContext}/></I18nProvider>);
  await screen.findByRole('heading', { name: `Build · ${target.toLowerCase()}` });
  await screen.findByRole('button', { name: 'Open all' });
  const ratios = ['N/A', ...settingsAspects];
  expect([...document.querySelectorAll('[data-forge-tile^="aspect:"]')].map(e => e.getAttribute('data-forge-tile')!.slice(7))).toEqual(ratios);
  fireEvent.change(document.querySelector('[data-forge-brief]')!, { target: { value: 'a quiet forest' } });
  for (const [index, ratio] of ratios.entries()) {
    fireEvent.click(document.querySelector(`[data-forge-tile="aspect:${ratio}"]`)!);
    expect(document.querySelector(`[data-forge-tile="aspect:${ratio}"]`)!.getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(document.querySelector('[data-forge-write]')!);
    await waitFor(() => expect(nativeCalls()).toHaveLength(index + 1));
    expect(nativeCalls().at(-1)!.request.controls).toMatchObject({ aspectRatio: ratio });
    await waitFor(() => expect((document.querySelector('[data-forge-write]') as HTMLButtonElement).disabled).toBe(false));
  }
});

it.each([
  ['2:3 Portrait','2:3'], ['9:32','9:32'], ['3:2 forged','3:2'], ['auto','auto'], ['custom','custom'], ['N/A','N/A'],
])('keeps saved aspect %s intact while Write and Revise use %s', async (saved, effective) => {
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((c,a) => c === 'native_forge_targets'
    ? Promise.resolve({...nativeTargets,targets:nativeTargets.targets.map(t=>({...t,aspectRatios:['N/A','2:3 Portrait']}))})
    : c === 'native_forge_revise' ? base('native_forge_write',a) : base(c,a));
  localStorage.setItem('deas.pref.forge.target', JSON.stringify('Illustrious'));
  localStorage.setItem('deas.pref.forge.native.illustrious.aspect', JSON.stringify(saved));
  render(<I18nProvider><PromptForge endpoint="" root="/env" onSend={vi.fn()} canvasContext={forgeCanvasContext}/></I18nProvider>);
  await screen.findByRole('button', { name: 'Open all' });
  expect(document.querySelector(`[data-forge-tile="aspect:${effective}"]`)!.getAttribute('aria-pressed')).toBe('true');
  fireEvent.change(document.querySelector('[data-forge-brief]')!, {target:{value:'a quiet forest'}});
  fireEvent.click(document.querySelector('[data-forge-write]')!);
  await screen.findByDisplayValue('1girl, rain');
  expect(nativeCalls().at(-1)!.request.controls).toMatchObject({aspectRatio:effective});
  fireEvent.change(document.querySelector('[data-forge-refine]')!, {target:{value:'make it brighter'}});
  fireEvent.click(document.querySelector('[data-forge-refine-run]')!);
  await waitFor(()=>expect(vi.mocked(invoke).mock.calls.find(([c])=>c==='native_forge_revise')).toBeTruthy());
  expect((vi.mocked(invoke).mock.calls.find(([c])=>c==='native_forge_revise')![1] as {request:{controls:unknown}}).request.controls).toMatchObject({aspectRatio:effective});
  expect(localStorage.getItem('deas.pref.forge.native.illustrious.aspect')).toBe(JSON.stringify(saved));
});

it('Prompt Forge tab sends the builder picks: chips, tiles, lighting, kept-as-written', async () => {
  const rich = { ...nativeTargets, targets: nativeTargets.targets.map(t => t.key !== 'illustrious' ? t : { ...t, label: 'Illustrious XL', qualityPresets: ['masterpiece', 'high_quality', 'none'], styles: ['Anime', 'None'], aspectRatios: ['N/A', '2:3 Portrait'],
    detailLevels: [[1, 'Bare'], [5, 'Standard'], [10, 'Exhaustive']], lightingLevels: [[0, 'Off'], [3, 'Dark'], [10, 'Radiant']],
    camera: { angle: [{ key: 'birds_eye', label: "Bird's eye", icon: 'birds', payload: 'from above' }], shot: [{ key: 'medium', label: 'Medium', icon: 'medium', payload: 'upper body' }], light: [{ key: 'left', label: 'Left', icon: 'l_left', payload: 'light from the left' }] } }) };
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((c, a) => c === 'native_forge_targets' ? Promise.resolve(rich) : c === 'native_forge_write' ? Promise.resolve({ segments: [], refused: false, prompt: '1girl, campfire', negative: 'lowres', characters: ['frieren'], tools: [], rounds: 1, warnings: [], fixes: [] }) : base(c, a));
  localStorage.setItem('deas.pref.forge.target', JSON.stringify('Illustrious'));
  render(<I18nProvider><PromptForge endpoint="http://127.0.0.1:8188" root="/env" onSend={vi.fn()} canvasContext={{ key: 'studio', effective: { width: 1024, height: 1024 }, model: { width: 1024, height: 1024 }, grid: 16, resolution: 'model', megapixels: 1, fixed: { width: 1024, height: 1024 } }}/></I18nProvider>);
  await screen.findByRole('heading', { name: 'Build · Illustrious XL' });
  // Empty state until something is written; actions wait for a result.
  expect(screen.getByText('No prompt yet')).toBeTruthy();
  expect((screen.getByRole('button', { name: 'Copy all' }) as HTMLButtonElement).disabled).toBe(true);
  // Closed sections say what is set inside them.
  fireEvent.click(screen.getByRole('button', { name: 'Open all' }));
  fireEvent.click(document.querySelector('[data-forge-chip="quality:masterpiece"]')!);
  fireEvent.click(document.querySelector('[data-forge-chip="style:Anime"]')!);
  fireEvent.click(document.querySelector('[data-forge-tile="aspect:2:3"]')!);
  fireEvent.click(document.querySelector('[data-forge-tile="angle:birds_eye"]')!);
  fireEvent.click(document.querySelector('[data-forge-tile="shot:medium"]')!);
  fireEvent.click(document.querySelector('[data-forge-tile="light:left"]')!);
  fireEvent.change(document.querySelector('[data-forge-ladder="lighting"]')!, { target: { value: '1' } });
  fireEvent.change(document.querySelector('[data-forge-keep-positive]')!, { target: { value: 'lora_trigger' } });
  fireEvent.click(screen.getByRole('button', { name: 'Close all' }));
  const summary = (name: string) => document.querySelector(`[data-forge-fold="${name}"] .forge-fold-summary`)!.textContent;
  expect([summary('presets'), summary('camera'), summary('lighting'), summary('verbatim')]).toEqual(['Masterpiece · Anime · 2:3', "Bird's eye · Medium", 'Dark · from left', 'Custom positive']);
  fireEvent.change(document.querySelector('[data-forge-brief]')!, { target: { value: 'frieren by a campfire' } });
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await screen.findByDisplayValue('1girl, campfire');
  expect(nativeCalls().at(-1)!.request).toMatchObject({ target: 'illustrious', brief: 'frieren by a campfire', segmentsOn: ['Negative prompt'],
    controls: { qualityPreset: 'masterpiece', style: 'Anime', aspectRatio: '2:3', canvas: { width: 832, height: 1216 }, cameraAngle: 'birds_eye', cameraShot: 'medium', cameraLight: 'left', lighting: 3, positive: 'lora_trigger' } });
  expect((screen.getByRole('textbox', { name: 'Negative prompt' }) as HTMLTextAreaElement).value).toBe('lowres');
  expect(screen.getByText('Character canon from the local table: frieren')).toBeTruthy();
  // Picks belong to the model: Anima starts clean.
  fireEvent.change(document.querySelector('[data-forge-target]')!, { target: { value: 'Anima' } });
  await waitFor(() => expect(document.querySelector('[data-forge-native="anima"]')).toBeTruthy());
  expect(screen.getByText('No prompt yet')).toBeTruthy();
  fireEvent.change(document.querySelector('[data-forge-brief]')!, { target: { value: 'x' } });
  fireEvent.click(await screen.findByRole('button', { name: 'Write' }));
  await waitFor(() => expect(nativeCalls().at(-1)!.request.target).toBe('anima'));
  expect(nativeCalls().at(-1)!.request.controls).toMatchObject({ qualityPreset: 'none', style: 'None', aspectRatio: 'N/A', cameraAngle: undefined, lighting: undefined });
  fireEvent.change(document.querySelector('[data-forge-target]')!, { target: { value: 'Illustrious' } });
  await screen.findByRole('heading', { name: 'Build · Illustrious XL' });
  expect(document.querySelector('[data-forge-tile="aspect:2:3"]')!.getAttribute('aria-pressed')).toBe('true');
  expect(document.querySelector('[data-forge-chip="quality:masterpiece"]')!.getAttribute('aria-pressed')).toBe('true');
  expect(nativeCalls()).toHaveLength(2);
});

it('Prompt Forge tab refines, second-passes and undoes the draft on screen, with a length', async () => {
  const targets = { ...nativeTargets, targets: nativeTargets.targets.map(t => ({ ...t, secondPass: true })) };
  const drafts = [
    { segments: [{ label: 'Positive prompt', body: '1girl, campfire' }, { label: 'Negative prompt', body: 'lowres' }], refused: false, prompt: '1girl, campfire', negative: 'lowres', characters: [], tools: [], rounds: 0, warnings: [], fixes: [], changed: 0 },
    { segments: [{ label: 'Positive prompt', body: '1girl, blue fire' }, { label: 'Negative prompt', body: 'lowres' }], refused: false, prompt: '1girl, blue fire', negative: 'lowres', characters: [], tools: [], rounds: 0, warnings: [], fixes: [], changed: 1 },
  ];
  const revise = vi.fn(async (_: unknown) => drafts[1]);
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((c, a) => c === 'native_forge_targets' ? Promise.resolve(targets) : c === 'native_forge_write' ? Promise.resolve(drafts[0]) : c === 'native_forge_revise' ? revise(a) : base(c, a));
  localStorage.setItem('deas.pref.forge.target', JSON.stringify('Illustrious'));
  render(<I18nProvider><PromptForge endpoint="http://127.0.0.1:8188" root="/env" onSend={vi.fn()}/></I18nProvider>);
  await screen.findByRole('button', { name: 'Open all' });
  // Refine, Second pass and Undo wait for a draft.
  for (const name of ['Refine', 'Second pass', 'Undo']) expect((screen.getByRole('button', { name }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole('button', { name: 'Open all' }));
  fireEvent.change(document.querySelector('[data-forge-length-unit]')!, { target: { value: 'words' } });
  fireEvent.change(document.querySelector('[data-forge-length-value]')!, { target: { value: '5' } });
  fireEvent.change(document.querySelector('[data-forge-brief]')!, { target: { value: 'frieren by a campfire' } });
  expect((screen.getByRole('button', { name: 'Write' }) as HTMLButtonElement).disabled).toBe(true); // below the 10-word floor
  fireEvent.change(document.querySelector('[data-forge-length-value]')!, { target: { value: '60' } });
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await screen.findByDisplayValue('1girl, campfire');
  expect(nativeCalls().at(-1)!.request.controls).toMatchObject({ maxLength: { unit: 'words', value: 60 } });
  // The person's edit goes back with the refine request.
  fireEvent.change(screen.getByRole('textbox', { name: 'Result' }), { target: { value: '1girl, campfire, night' } });
  fireEvent.change(screen.getByRole('textbox', { name: 'Refine' }), { target: { value: 'colder light' } });
  fireEvent.click(screen.getByRole('button', { name: 'Refine' }));
  await screen.findByDisplayValue('1girl, blue fire');
  expect(revise).toHaveBeenLastCalledWith({ root: '/env', request: expect.objectContaining({ target: 'illustrious', instruction: 'colder light', segments: [{ label: 'Positive prompt', body: '1girl, campfire, night' }, { label: 'Negative prompt', body: 'lowres' }] }) });
  expect(screen.getByText('The revision rewrote 1 section(s).')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Second pass' }));
  await waitFor(() => expect(revise).toHaveBeenCalledTimes(2));
  expect((revise.mock.calls[1][0] as { request: { instruction?: string } }).request.instruction).toBeUndefined();
  // Undo walks back one revision at a time, to the edited draft.
  await waitFor(() => expect((screen.getByRole('button', { name: 'Undo' }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
  fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
  expect((screen.getByRole('textbox', { name: 'Result' }) as HTMLTextAreaElement).value).toBe('1girl, campfire, night');
  expect((screen.getByRole('button', { name: 'Undo' }) as HTMLButtonElement).disabled).toBe(true);
});

it('Prompt Forge tab picks tags from the local database, weights them and adds a character\'s traits', async () => {
  const targets = { ...nativeTargets, targets: nativeTargets.targets.map(t => ({ ...t, tagStyle: 'space', weightRange: { min: 0.5, max: 1.8, step: 0.05, default: 1 } })) };
  const search = vi.fn(async (_: unknown) => [{ tag: 'frieren', category: 'character', count: 16194 }, { tag: 'frieren (cosplay)', category: 'general', count: 316 }]);
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((c, a) => c === 'native_forge_targets' ? Promise.resolve(targets) : c === 'native_forge_tag_search' ? search(a)
    : c === 'native_forge_character_traits' ? Promise.resolve(['pointy ears', 'white hair']) : base(c, a));
  localStorage.setItem('deas.pref.forge.target', JSON.stringify('Illustrious'));
  render(<I18nProvider><PromptForge endpoint="http://127.0.0.1:8188" root="/env" onSend={vi.fn()}/></I18nProvider>);
  fireEvent.click(await screen.findByRole('button', { name: 'Open all' }));
  fireEvent.change(screen.getByRole('textbox', { name: 'Search tags…' }), { target: { value: 'frier' } });
  fireEvent.mouseDown(await screen.findByRole('option', { name: /^frieren character/ }));
  expect(search).toHaveBeenCalledWith({ root: '/env', target: 'illustrious', query: 'frier' });
  await screen.findByText('frieren: traits from the local character table');
  fireEvent.click(screen.getByRole('button', { name: 'Add all' }));
  expect((screen.getByRole('button', { name: 'Add all' }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole('button', { name: 'frieren' }));
  fireEvent.change(document.querySelector('[data-forge-tag-weight]')!, { target: { value: '1.2' } });
  fireEvent.click(screen.getByRole('button', { name: 'Remove white hair' }));
  fireEvent.change(document.querySelector('[data-forge-brief]')!, { target: { value: 'by a campfire' } });
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await waitFor(() => expect(nativeCalls()).toHaveLength(1));
  expect(nativeCalls()[0].request.controls).toMatchObject({ tags: ['(frieren:1.20)', 'pointy ears'] });
});

it('Randomizer rolls the ticked boxes with the cast and adds the result to the brief', async () => {
  const roll = vi.fn(async (_: unknown) => ({ boxes: [{ id: 'hair', label: 'Hair', picks: [{ id: 'h', label: 'Colour', text: 'grey hair' }] }], cast: ['1girl'], text: '1girl, grey hair' }));
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((c, a) => c === 'native_forge_randomizer_list' ? Promise.resolve([{ id: 'hair', label: 'Hair', children: [{ id: 'hair_colour', label: 'Colour', options: 40 }, { id: 'hair_len', label: 'Length', options: 9 }] }, { id: 'nsfw', label: 'NSFW', children: [{ id: 'x', label: 'X', options: 3 }] }])
    : c === 'native_forge_randomizer_roll' ? roll(a) : base(c, a));
  localStorage.setItem('deas.pref.forge.target', JSON.stringify('Illustrious'));
  render(<I18nProvider><PromptForge endpoint="http://127.0.0.1:8188" root="/env" onSend={vi.fn()}/></I18nProvider>);
  fireEvent.change(document.querySelector('[data-forge-brief]')!, { target: { value: 'by a campfire' } });
  fireEvent.click(await screen.findByRole('button', { name: 'Randomizer' }));
  const roller = await screen.findByRole('dialog', { name: 'Randomizer' });
  fireEvent.click(await screen.findByRole('checkbox', { name: 'Everything in Hair' }));
  fireEvent.change(roller.querySelector('[data-forge-random-cast="women"]')!, { target: { value: '1' } });
  fireEvent.click(screen.getByRole('button', { name: 'Roll' }));
  await screen.findByDisplayValue('1girl, grey hair');
  expect(roll).toHaveBeenCalledWith({ root: '/env', request: { categories: ['hair'], women: 1, men: 0, surprise: false, limits: {} } });
  fireEvent.click(screen.getByRole('button', { name: 'Add to brief' }));
  expect(screen.queryByRole('dialog')).toBeNull();
  expect((document.querySelector('[data-forge-brief]') as HTMLTextAreaElement).value).toBe('by a campfire, 1girl, grey hair');
});

it('Prompt Forge tab sends reference pictures with their roles, analyzes one and shows what the write was read from', async () => {
  const references = { roles: [{ key: 'general', label: 'General' }, { key: 'pose', label: 'Pose', rule: 'posture and hands' }], fidelity: [{ key: 'close', label: 'Keep most of it' }, { key: 'loose', label: 'Loose inspiration' }], defaultRole: 'general', defaultFidelity: 'close', max: 6, analyze: true };
  const readback = vi.fn(async (_: unknown) => '**What I see**\nA girl with her arms out.');
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((c, a) => c === 'native_forge_targets' ? Promise.resolve({ ...nativeTargets, references })
    : c === 'native_forge_reference_readback' ? readback(a)
    : c === 'native_forge_write' ? Promise.resolve({ segments: [], refused: false, prompt: 'frieren, arms out', negative: null, characters: [], tools: [], rounds: 0, warnings: [], fixes: [], referenceAnalysis: 'REFERENCE 1 — POSE\nUSE: arms horizontal' })
    : base(c, a));
  localStorage.setItem('deas.pref.forge.target', JSON.stringify('Illustrious'));
  render(<I18nProvider><PromptForge endpoint="http://127.0.0.1:8188" root="/env" onSend={vi.fn()}/></I18nProvider>);
  fireEvent.click(await screen.findByRole('button', { name: /Reference images/ }));
  fireEvent.change(document.querySelector('[data-forge-ref-select="next-role"]')!, { target: { value: 'pose' } });
  fireEvent.change(document.querySelector('[data-forge-ref-file]')!, { target: { files: [new File([new Uint8Array([137, 80, 78, 71])], 'pose.png', { type: 'image/png' })] } });
  await waitFor(() => expect(document.querySelectorAll('[data-forge-ref]')).toHaveLength(1));
  expect(screen.getByText('posture and hands')).toBeTruthy();
  fireEvent.change(document.querySelector('[data-forge-ref-note]')!, { target: { value: 'the arms only' } });
  fireEvent.click(screen.getByRole('button', { name: 'Analyze' }));
  await screen.findByText('A girl with her arms out.');
  expect(readback).toHaveBeenCalledWith({ root: '/env', model: 'qwen', reference: expect.objectContaining({ role: 'pose', fidelity: 'close', note: 'the arms only', image: expect.stringMatching(/^data:image\/png;base64,/) }) });
  // A changed note makes the read-back out of date.
  fireEvent.change(document.querySelector('[data-forge-ref-note]')!, { target: { value: 'the arms and the gaze' } });
  expect(screen.getByText(/out of date/)).toBeTruthy();
  fireEvent.change(document.querySelector('[data-forge-brief]')!, { target: { value: 'frieren in a meadow' } });
  fireEvent.click(screen.getByRole('button', { name: 'Write' }));
  await screen.findByDisplayValue('frieren, arms out');
  expect(nativeCalls().at(-1)!.request.controls).toMatchObject({ imageReferences: [{ role: 'pose', fidelity: 'close', note: 'the arms and the gaze', character: '', image: expect.stringMatching(/^data:image\/png;base64,/) }] });
  expect(screen.getByText(/What the pictures were read as/)).toBeTruthy();
});
