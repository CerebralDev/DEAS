import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { I18nProvider } from '../../i18n/I18nProvider';
import { OpenComfyWorkflow } from '../../lib/comfyWorkflow';
import { Analysis } from './Analysis';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => true),
  convertFileSrc: (p: string) => `asset://${p}`,
  Channel: class { onmessage: (e: unknown) => void = () => {}; },
}));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }) }));

const WD = { id: 'E:/models/taggers/wd/model.onnx', name: 'WD v3 EVA02-Large', model: 'E:/models/taggers/wd/model.onnx', labels: 'E:/models/taggers/wd/selected_tags.csv', place: 'models/taggers/wd', tags: 10861, categories: true, generalThreshold: 0.35, characterThreshold: 0.85 };
const PYTORCH = { name: 'animetimm ConvNeXtV2-Huge', place: 'models/animetimm/convnextv2_huge.dbv4-full', reason: 'Only PyTorch weights here (pytorch_model.bin); Media Analysis runs ONNX taggers.' };
const install = { running: false, stage: '', downloaded: 0, total: 0, error: null };
const WORKFLOW = JSON.stringify({ nodes: [{ id: 1, type: 'KSampler' }], links: [] });
let overview: Record<string, unknown>;
let metadata: [string, string][];
const calls = (name: string) => vi.mocked(invoke).mock.calls.filter(([c]) => c === name).map(([, a]) => a as Record<string, any>);
const picture = { item: { name: 'frieren.png', path: 'E:/out/frieren.png', src: 'asset://E:/out/frieren.png', media: 'image' as const }, n: 1 };
const reading = { text: '…', description: 'A white-haired elf reads under a tree.', characters: ['frieren'], suggested: ['tree', 'glossy_skin'], unknown: ['glossy_skin'], doubtful: 'nothing', cast: [['sousou_no_frieren', 20]], stopped: false };

beforeEach(() => {
  localStorage.clear(); localStorage.setItem('deas.locale.v1', 'en');
  overview = { runtimeReady: true, install, taggers: [WD], unusable: [PYTORCH], defaultSize: 1_260_744_467 };
  metadata = [['workflow', WORKFLOW], ['parameters', 'frieren, steps: 28']];
  Object.assign(navigator, { clipboard: { writeText: vi.fn(async () => {}) } });
  globalThis.fetch = vi.fn(async () => new Response(new Blob(['png'], { type: 'image/png' }))) as typeof fetch;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    const a = args as Record<string, any>;
    if (command === 'native_analysis_overview') return overview;
    if (command === 'native_analysis_setup') return { ...overview, runtimeReady: true };
    if (command === 'native_analysis_tag') return { tagger: 'WD v3 EVA02-Large', rating: { general: 0.9 }, general: [['1girl', 0.99], ['book', 0.7]], character: [['frieren', 0.95]], ms: 2700 };
    if (command === 'native_media_metadata') return metadata;
    if (command === 'native_llm_read') return { provider: 'ollama', baseUrl: 'http://127.0.0.1:11434' };
    if (command === 'native_llm_models') return { models: [{ provider: 'ollama', id: 'qwen3-vl:8b-instruct', name: 'qwen3-vl:8b-instruct' }], warnings: [] };
    if (command === 'native_analysis_describe') {
      a.onEvent.onmessage({ type: 'started', cast: [['sousou_no_frieren', 20]] });
      a.onEvent.onmessage({ type: 'text', text: 'Description: A white-haired elf' });
      return reading;
    }
    if (command === 'native_analysis_frames_open') return new Promise(() => {});
    throw new Error(`Unexpected ${command}`);
  });
});
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const ui = (props: Partial<Parameters<typeof Analysis>[0]> = {}, open: ((w: unknown, n: string) => Promise<void>) | null = null) =>
  render(<I18nProvider><OpenComfyWorkflow.Provider value={open}><Analysis root="E:/env" roots={{ models: 'E:/models' }} onSend={vi.fn()} {...props}/></OpenComfyWorkflow.Provider></I18nProvider>);

it('sets up only the runtime when a tagger is found, and lists what cannot be used with the reason', async () => {
  overview = { ...overview, runtimeReady: false };
  ui();
  expect(await screen.findByText('Found: WD v3 EVA02-Large.')).toBeTruthy();
  expect(screen.queryByText(/Download WD v3/)).toBeNull();
  expect(screen.getByText(/Only PyTorch weights here/)).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Set up taggers' }));
  await waitFor(() => expect(calls('native_analysis_setup')).toEqual([{ models: ['E:/models', 'E:/env/ComfyUI/models'], download: false }]));
});

it('offers the download only when no tagger is found, and can look again', async () => {
  overview = { ...overview, runtimeReady: false, taggers: [] };
  ui();
  expect(await screen.findByText('No image tagger was found.')).toBeTruthy();
  expect(document.querySelector('[data-analysis-unusable]')?.hasAttribute('open')).toBe(true);
  fireEvent.click(screen.getByRole('button', { name: 'Look again' }));
  await waitFor(() => expect(calls('native_analysis_overview').length).toBe(2));
  fireEvent.click(screen.getByRole('button', { name: 'Download WD v3 EVA02 (1.2 GB) and set up' }));
  await waitFor(() => expect(calls('native_analysis_setup')[0].download).toBe(true));
});

it('tags a picture from Media, copies and sends the tags, and opens its workflow in ComfyUI', async () => {
  const onSend = vi.fn();
  const open = vi.fn(async () => {});
  ui({ incoming: picture, onSend }, open);
  expect(await screen.findByText('frieren')).toBeTruthy();
  expect(calls('native_analysis_tag')[0]).toMatchObject({ tagger: WD.id, image: 'E:/out/frieren.png' });
  expect(screen.getByText('From models/taggers/wd · 10861 tags')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Copy tags' }));
  await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalledWith('frieren, 1girl, book'));
  fireEvent.click(screen.getByRole('button', { name: 'Send to Generate' }));
  expect(onSend).toHaveBeenCalledWith('frieren, 1girl, book', '');
  const comfy = await screen.findByRole('button', { name: 'Open in ComfyUI' });
  expect((comfy as HTMLButtonElement).disabled).toBe(false);
  fireEvent.click(comfy);
  await waitFor(() => expect(open).toHaveBeenCalledWith(JSON.parse(WORKFLOW), 'frieren.png'));
});

it('says why Open in ComfyUI is off: no workflow in the picture, or no backend running', async () => {
  metadata = [];
  ui({ incoming: picture }, vi.fn());
  expect(await screen.findByText('This picture carries no ComfyUI workflow.')).toBeTruthy();
  expect((screen.getByRole('button', { name: 'Open in ComfyUI' }) as HTMLButtonElement).disabled).toBe(true);
  expect(screen.getByText('This picture carries no generation settings.')).toBeTruthy();
  cleanup();
  metadata = [['workflow', WORKFLOW]];
  ui({ incoming: picture }, null);
  expect(await screen.findByText(/Start the backend first/)).toBeTruthy();
  expect((screen.getByRole('button', { name: 'Open in ComfyUI' }) as HTMLButtonElement).disabled).toBe(true);
});

it('describes the picture with its tags, shows the three parts, and adds only real tags', async () => {
  ui({ incoming: picture });
  await screen.findByText('book');
  fireEvent.click(screen.getByRole('button', { name: 'Describe' }));
  expect(await screen.findByText('A white-haired elf reads under a tree.')).toBeTruthy();
  const sent = calls('native_analysis_describe')[0];
  expect(sent).toMatchObject({ root: 'E:/env', model: 'qwen3-vl:8b-instruct', tags: ['frieren', '1girl', 'book'] });
  expect(sent.image).toMatch(/^data:image\/png;base64,/);
  const reading = document.querySelector('[data-analysis-reading]') as HTMLElement;
  expect(within(reading).getByText('frieren')).toBeTruthy();
  expect(within(reading).getByText(/sousou_no_frieren \(20\)/)).toBeTruthy();
  expect(within(reading).getByText('glossy_skin ?')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Add its tags to the tags above' }));
  fireEvent.click(screen.getByRole('button', { name: 'Copy tags' }));
  await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenLastCalledWith('frieren, 1girl, book, tree'));
});

it('shows the server\'s plain refusal when the model cannot see pictures', async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === 'native_analysis_overview') return overview;
    if (command === 'native_analysis_tag') return { tagger: 'WD', rating: {}, general: [['1girl', 0.9]], character: [], ms: 1 };
    if (command === 'native_media_metadata') return [];
    if (command === 'native_llm_read') return { provider: 'ollama', baseUrl: 'http://127.0.0.1:11434' };
    if (command === 'native_llm_models') return { models: [{ provider: 'ollama', id: 'promptforge-4b-r4', name: 'promptforge-4b-r4' }], warnings: [] };
    if (command === 'native_analysis_describe') throw 'ANALYSIS_DESCRIBE: promptforge-4b-r4 cannot see pictures. Pick a model that can (a vision model) in the Model list, or set one up under Backend → LLM.';
    throw new Error(`Unexpected ${command}`);
  });
  ui({ incoming: picture });
  await screen.findByText('1girl');
  await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Describe' })); });
  expect((await screen.findByText(/cannot see pictures/)).textContent).toBe('promptforge-4b-r4 cannot see pictures. Pick a model that can (a vision model) in the Model list, or set one up under Backend → LLM.');
});

it('opens a video from Media in the frame picker instead of tagging it', async () => {
  ui({ incoming: { item: { name: 'clip.mp4', path: 'E:/out/clip.mp4', src: 'asset://E:/out/clip.mp4', media: 'video' }, n: 1 } });
  expect(await screen.findByText('Reading the frames of clip.mp4…')).toBeTruthy();
  expect(calls('native_analysis_frames_open')).toEqual([{ path: 'E:/out/clip.mp4' }]);
  expect(calls('native_analysis_tag')).toEqual([]);
});

it('chooses a file through the native dialog and refuses a pasted video', async () => {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === 'native_analysis_overview') return overview;
    if (command === 'native_analysis_pick') return 'E:/out/frieren.png';
    if (command === 'native_media_describe') return [{ kind: 'file', name: 'frieren.png', path: 'E:/out/frieren.png', media: 'image' }];
    if (command === 'native_analysis_tag') return { tagger: 'WD', rating: {}, general: [['1girl', 0.9]], character: [], ms: 1 };
    if (command === 'native_media_metadata') return [];
    throw new Error(`Unexpected ${command}`);
  });
  ui();
  fireEvent.click(await screen.findByRole('button', { name: 'Choose…' }));
  expect(await screen.findByText('1girl')).toBeTruthy();
  const video = new File(['x'], 'clip.mp4', { type: 'video/mp4' });
  fireEvent.paste(document.querySelector('.analysis-tab')!, { clipboardData: { files: [video] } });
  expect(await screen.findByText(/Videos cannot be pasted/)).toBeTruthy();
});
