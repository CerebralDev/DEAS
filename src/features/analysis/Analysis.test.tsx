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
  overview = { runtimeReady: true, install, taggers: [WD], unusable: [PYTORCH], defaultSize: 1_260_744_467, runtimeSize: 300_000_000, toolsSize: 300_000_000 };
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
  // The model plus the runtime it still needs.
  fireEvent.click(screen.getByRole('button', { name: 'Download WD v3 EVA02 and set up (1.5 GB in total)' }));
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

it('offers to convert a PyTorch-only animetimm checkpoint, says what it costs, and lists the copy once done', async () => {
  const folder = 'E:/models/animetimm/convnextv2_huge.dbv4-full';
  const offer = { ...PYTORCH, reason: 'Only PyTorch weights here (pytorch_model.bin); Media Analysis runs ONNX taggers. DEAS can convert it to ONNX once, on this PC.', convert: { folder, bytes: 2_770_612_051 } };
  const CONVERTED = { ...WD, id: 'R:/converted/x/model.onnx', name: 'animetimm ConvNeXtV2-Huge', place: 'models/animetimm/convnextv2_huge.dbv4-full (converted)', generalThreshold: 0.38, characterThreshold: 0.51 };
  overview = { ...overview, unusable: [offer] };
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === 'native_analysis_convert') { overview = { ...overview, taggers: [WD, CONVERTED], unusable: [] }; return overview; }
    return base(command, args);
  });
  ui();
  expect(await screen.findByText(/downloads about 0\.3 GB of conversion tools .* writes a 2\.6 GB copy/)).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Convert to ONNX' }));
  await waitFor(() => expect(calls('native_analysis_convert')).toEqual([{ models: ['E:/models', 'E:/env/ComfyUI/models'], folder }]));
  expect(await screen.findByText('animetimm ConvNeXtV2-Huge is converted and ready to use.')).toBeTruthy();
  expect(screen.queryByRole('button', { name: 'Convert to ONNX' })).toBeNull();
  expect([...(document.querySelector('[data-analysis-tagger]') as HTMLSelectElement).options].map(o => o.textContent)).toContain('animetimm ConvNeXtV2-Huge');
});

it('lists the models before Describe is pressed, runs on the one picked, with the sliders\' sampling', async () => {
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === 'native_llm_models') return { models: [{ provider: 'ollama', id: 'big-27b', name: 'big-27b' }, { provider: 'ollama', id: 'qwen3-vl:8b-instruct', name: 'qwen3-vl:8b-instruct' }], warnings: [] };
    return base(command, args);
  });
  ui({ incoming: picture });
  await screen.findByText('book');
  // Listed without pressing anything, and the first is not run on its own.
  const select = document.querySelector('[data-analysis-describe-model]') as HTMLSelectElement;
  await waitFor(() => expect([...select.options].map(o => o.value)).toEqual(['ollama:big-27b', 'ollama:qwen3-vl:8b-instruct']));
  expect(calls('native_analysis_describe')).toEqual([]);
  fireEvent.change(select, { target: { value: 'ollama:qwen3-vl:8b-instruct' } });
  fireEvent.change(document.querySelector('[data-analysis-temperature]')!, { target: { value: '0.7' } });
  fireEvent.change(document.querySelector('[data-analysis-penalty]')!, { target: { value: '1.5' } });
  expect(screen.getByText('0.70')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Describe' }));
  await waitFor(() => expect(calls('native_analysis_describe')[0]).toMatchObject({ model: 'qwen3-vl:8b-instruct', temperature: 0.7, presencePenalty: 1.5 }));
});

it('does not list models (or ask for an LLM) on opening a picture when no LLM is set up, and the list still opens on a click', async () => {
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === 'native_llm_read') return { provider: 'openai', baseUrl: '' };
    return base(command, args);
  });
  ui({ incoming: picture });
  await screen.findByText('book');
  await new Promise(r => setTimeout(r, 50));
  expect(calls('native_llm_models')).toEqual([]);
  const select = document.querySelector('[data-analysis-describe-model]') as HTMLSelectElement;
  expect(select.disabled).toBe(false);
  expect(select.options[0].textContent).toBe('Click to list the models');
  fireEvent.mouseDown(select);
  await waitFor(() => expect(calls('native_llm_models').length).toBe(1));
});

it('shows what made a ComfyUI picture like PromptForge: checkpoint, seed from a seed node, sampler, and the typed part of a wildcard prompt', async () => {
  const graph = {
    '4': { class_type: 'CheckpointLoaderSimple', inputs: { ckpt_name: 'miaomiao3DHarem_animaLH3D10.safetensors' } },
    '10': { class_type: 'Power Lora Loader (rgthree)', inputs: { lora_1: { on: true, lora: 'styles\\unicorn.safetensors', strength: 0.8 }, lora_2: { on: false, lora: 'off.safetensors', strength: 1 }, model: ['4', 0], clip: ['4', 1] } },
    '20': { class_type: 'Seed (rgthree)', inputs: { seed: 453679749698980 } },
    '30': { class_type: 'ImpactWildcardProcessor', inputs: { wildcard_text: 'unicorn horn, {spiral horn|crystal horn}', mode: 'populate', seed: ['20', 0] } },
    '31': { class_type: 'CLIPTextEncode', inputs: { text: ['30', 0], clip: ['10', 1] } },
    '32': { class_type: 'CLIPTextEncode', inputs: { text: 'worst quality, blurry', clip: ['10', 1] } },
    '3': { class_type: 'KSampler', inputs: { seed: ['20', 0], steps: 30, cfg: 4, sampler_name: 'euler', scheduler: 'normal', denoise: 1, model: ['10', 0], positive: ['31', 0], negative: ['32', 0], latent_image: ['5', 0] } },
    '9': { class_type: 'SaveImage', inputs: { images: ['8', 0] } },
    '8': { class_type: 'VAEDecode', inputs: { samples: ['3', 0] } },
  };
  metadata = [['prompt', JSON.stringify(graph)], ['workflow', WORKFLOW]];
  const onSend = vi.fn();
  ui({ incoming: picture, onSend });
  await waitFor(() => expect(document.querySelector('[data-analysis-made-summary]')).toBeTruthy());
  const rows = [...document.querySelector('[data-analysis-made-summary]')!.querySelectorAll('div')].map(d => d.textContent);
  expect(rows).toEqual(['Checkpointmiaomiao3DHarem_animaLH3D10.safetensors', 'LoRAsstyles/unicorn.safetensors @ 0.8', 'Seed453679749698980', 'SamplerKSampler · euler · normal', 'Steps / CFG / denoise30 / 4 / 1']);
  expect(document.querySelector('[data-analysis-made-prompt]')!.textContent).toBe('Prompt: unicorn horn, {spiral horn|crystal horn} (built at run time — this is only the part that was typed)');
  fireEvent.click(screen.getByRole('button', { name: 'Send it to Generate' }));
  expect(onSend).toHaveBeenCalledWith('unicorn horn, {spiral horn|crystal horn}', 'worst quality, blurry');
  fireEvent.click(screen.getByRole('button', { name: 'Copy the original prompt' }));
  await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalledWith('unicorn horn, {spiral horn|crystal horn}'));
  // A 64-bit seed keeps its exact digits.
  cleanup();
  metadata = [['prompt', JSON.stringify(graph).replace('453679749698980', '18446744073709551615')]];
  ui({ incoming: picture });
  await waitFor(() => expect(document.querySelector('[data-analysis-made-summary]')?.textContent).toContain('Seed18446744073709551615'));
});

it('goes back from a used frame to its video, on the same frame', async () => {
  const clip = { name: 'clip.mp4', width: 64, height: 64, fps: 16, count: 49, duration: 3.0625, stride: 1, thumbs: Array.from({ length: 49 }, (_, i) => `C:/strip/${i}.jpg`) };
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    const a = args as Record<string, any>;
    if (command === 'native_analysis_frames_open') return clip;
    if (command === 'native_analysis_frame') return `C:/frames/clip-f${String(a.index + 1).padStart(4, '0')}.png`;
    if (command === 'native_media_describe') return [{ kind: 'file', name: a.paths[0].split('/').pop(), path: a.paths[0], media: 'image' }];
    return base(command, args);
  });
  Element.prototype.scrollIntoView = vi.fn();
  ui({ incoming: { item: { name: 'clip.mp4', path: 'E:/out/clip.mp4', src: 'asset://E:/out/clip.mp4', media: 'video' }, n: 1 } });
  await screen.findByText(/49 frames/);
  fireEvent.click(document.querySelector('[data-frame-cell="2"]')!, { ctrlKey: true });
  fireEvent.click(document.querySelector('[data-frame-cell="11"]')!);
  fireEvent.click(screen.getByRole('button', { name: 'Use this frame' }));
  const back = await screen.findByRole('button', { name: '← Back to clip.mp4 (frame 12)' });
  expect(calls('native_analysis_tag').at(-1)).toMatchObject({ image: 'C:/frames/clip-f0012.png' });
  fireEvent.click(back);
  expect(await screen.findByText('Frame 12 / 49 · 0:00.69')).toBeTruthy();
  // The picks made before are still there.
  expect(document.querySelector('[data-frame-cell="2"]')!.getAttribute('aria-selected')).toBe('true');
});

it('reads setup progress without a folder scan, only while the tab is shown, and looks again once done', async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  try {
    overview = { ...overview, runtimeReady: false, install: { ...install, running: true, stage: 'packages' } };
    const view = ui();
    expect(await screen.findByText('Installing the tagger packages…')).toBeTruthy();
    await act(async () => { vi.advanceTimersByTime(2100); });
    expect(calls('native_analysis_overview').filter(a => a.scan === false).length).toBe(2);
    // Hidden tab: no polling.
    view.rerender(<I18nProvider><OpenComfyWorkflow.Provider value={null}><Analysis active={false} root="E:/env" roots={{ models: 'E:/models' }} onSend={vi.fn()}/></OpenComfyWorkflow.Provider></I18nProvider>);
    const before = calls('native_analysis_overview').length;
    await act(async () => { vi.advanceTimersByTime(3100); });
    expect(calls('native_analysis_overview').length).toBe(before);
    // Shown again and finished: one full look.
    overview = { ...overview, runtimeReady: true, install };
    view.rerender(<I18nProvider><OpenComfyWorkflow.Provider value={null}><Analysis active root="E:/env" roots={{ models: 'E:/models' }} onSend={vi.fn()}/></OpenComfyWorkflow.Provider></I18nProvider>);
    await act(async () => { vi.advanceTimersByTime(1100); });
    await waitFor(() => expect(document.querySelector('[data-analysis-tagger]')).toBeTruthy());
    expect(calls('native_analysis_overview').at(-1)!.scan).toBe(true);
  } finally { vi.useRealTimers(); }
});
