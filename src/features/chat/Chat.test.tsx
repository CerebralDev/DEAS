import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { I18nProvider } from '../../i18n/I18nProvider';
import { PreferencePersistenceContext, isString, usePref } from '../../lib/preferences';
import { Chat } from './Chat';

function StudioControls() {
  const [positive, setPositive] = usePref('studio.run.positive', '', isString);
  const [negative] = usePref('studio.run.negative', '', isString);
  const [pipeline] = usePref('studio.run.pipeline', 't2i', isString);
  void negative;void pipeline;
  const [checkpoint] = usePref('studio.run.checkpoint', '', isString);
  return <><input aria-label="Studio draft" value={positive} onChange={e => setPositive(e.target.value)}/><span>{checkpoint}</span></>;
}

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => true),
  convertFileSrc: (p: string) => p,
  Channel: class { onmessage: (e: unknown) => void = () => {}; },
}));

const ID = 'a0000000-0000-4000-8000-000000000001';
const old = { id: ID, title: 'how does LTX prompting work?', model: 'qwen', created: 1, updated: 2, messages: [
  { role: 'user', content: 'how does LTX prompting work?', pictures: 0, prompts: [], citations: [], tools: [], at: 1 },
  { role: 'assistant', content: 'One **flowing paragraph**:\n\n1. Shot\n2. Scene', pictures: 0, prompts: [], citations: [], tools: ['read_guide: ltx'], at: 2 },
] };
let settings = { search: 'off', searxngUrl: '' };
const sends = () => vi.mocked(invoke).mock.calls.filter(([c]) => c === 'native_chat_send').map(([, a]) => (a as { request: Record<string, unknown> }).request);

beforeEach(() => {
  localStorage.clear(); localStorage.setItem('deas.locale.v1', 'en');
  settings = { search: 'off', searxngUrl: '' };
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    const a = args as Record<string, any>;
    if (command === 'native_llm_read') return { provider: 'openai', baseUrl: 'http://127.0.0.1:8080' };
    if (command === 'native_llm_models') return { models: [{ provider: 'openai', id: 'qwen', name: 'qwen' }], warnings: [] };
    if (command === 'native_forge') return { models: [{ id: 'openai:qwen', label: 'qwen' }], errors: {}, creativity: [], shot_counts: ['Auto'], default_shots: 'Auto' };
    if (command === 'native_chat_list') return [{ id: ID, title: old.title, updated: 2, count: 2 }];
    if (command === 'native_chat_get') return old;
    if (command === 'native_chat_settings') return settings;
    if (command === 'native_chat_settings_write') { settings = a.settings; return settings; }
    if (command === 'native_chat_delete' || command === 'native_chat_rename') return undefined;
    if(command==='native_workspace_validate_suggestion')return a.proposed;
    if (command === 'native_chat_send') {
      const emit = (e: unknown) => a.onEvent.onmessage(e);
      const prompt = { target: 'illustrious', label: 'Illustrious XL', prompt: 'frieren, reading, under tree', negative: null };
      emit({ event: 'chat', id: ID, title: 't' });
      emit({ event: 'status', state: 'writing_prompt', detail: 'Illustrious XL' });
      emit({ event: 'prompt', prompt });
      emit({ event: 'text', text: 'Here it is.' });
      return { role: 'assistant', content: 'Here it is.\n\nIllustrious XL prompt, written by the Illustrious XL prompt writer:\n\nfrieren, reading, under tree', pictures: 0, prompts: [prompt], citations: [], tools: ['write_prompt: illustrious'], at: 3 };
    }
    throw new Error(`Unexpected ${command}`);
  });
});
afterEach(() => { cleanup(); vi.resetAllMocks(); });

it('lists local GGUFs like Prompt Forge and sends them with their local: prefix', async () => {
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (command, args) => command === 'native_llm_read' ? { provider: 'local', baseUrl: '' }
    : command === 'native_llm_models' ? { models: [{ provider: 'local', id: 'gemma.gguf', name: 'gemma.gguf' }], warnings: [] } : base(command, args));
  render(<I18nProvider><Chat endpoint="http://127.0.0.1:8188" root="/env" onSend={vi.fn()}/></I18nProvider>);
  const option = await screen.findByRole('option', { name: 'gemma.gguf' }) as HTMLOptionElement;
  expect(option.disabled).toBe(false);
  fireEvent.change(screen.getByRole('textbox', { name: 'Message' }), { target: { value: 'hi' } });
  await waitFor(() => expect((screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole('button', { name: 'Send' }));
  await waitFor(() => expect(sends()[0]).toMatchObject({ model: 'local:gemma.gguf' }));
});

it('opens a saved conversation, streams a reply with a prompt card and sends that prompt to Generate', async () => {
  const onSend = vi.fn();
  localStorage.setItem('deas.pref.studio.run.positive', JSON.stringify('1girl, rain'));
  localStorage.setItem('deas.pref.studio.run.checkpoint', JSON.stringify('waiIllustrious.safetensors'));
  render(<I18nProvider><StudioControls/><Chat endpoint="http://127.0.0.1:8188" root="/env" onSend={onSend}/></I18nProvider>);
  fireEvent.click(await screen.findByRole('button', { name: old.title }));
  await screen.findByText('flowing paragraph');
  expect(screen.getByText('read_guide: ltx')).toBeTruthy();
  // Search stays hidden while it is switched off.
  expect(screen.queryByRole('button', { name: 'Search' })).toBeNull();
  await waitFor(() => expect((screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement).disabled).toBe(true));
  fireEvent.change(screen.getByRole('textbox', { name: 'Message' }), { target: { value: 'write me an illustrious prompt: frieren reading' } });
  fireEvent.keyDown(screen.getByRole('textbox', { name: 'Message' }), { key: 'Enter' });
  await screen.findByText('Here it is.');
  expect(sends()[0]).toMatchObject({ id: ID, message: 'write me an illustrious prompt: frieren reading', model: 'qwen', pictures: [], search: false });
  expect(JSON.parse(String(sends()[0].context)).values.positive).toBe('1girl, rain');
  expect(JSON.parse(String(sends()[0].context)).values.checkpoint).toBe('waiIllustrious.safetensors');
  // The prompt shows once, as a card, not again in the reply text.
  expect(screen.getAllByText('frieren, reading, under tree')).toHaveLength(1);
  fireEvent.click(screen.getByRole('button',{name:'Review changes'}));
  fireEvent.click(screen.getByRole('button',{name:'Apply'}));
  await waitFor(()=>expect((screen.getByRole('textbox',{name:'Studio draft'}) as HTMLInputElement).value).toBe('frieren, reading, under tree'));
  expect(onSend).not.toHaveBeenCalled();
  expect((screen.getByRole('textbox', { name: 'Message' }) as HTMLTextAreaElement).value).toBe('');
});

it('includes live project controls rather than the global Studio defaults', async () => {
  localStorage.setItem('deas.pref.studio.run.positive', JSON.stringify('global default'));
  render(<I18nProvider><PreferencePersistenceContext.Provider value={false}><StudioControls/></PreferencePersistenceContext.Provider><Chat endpoint="http://127.0.0.1:8188" root="/env" onSend={vi.fn()}/></I18nProvider>);
  fireEvent.change(screen.getByRole('textbox', { name: 'Studio draft' }), { target: { value: 'unsaved project prompt' } });
  expect(JSON.parse(localStorage.getItem('deas.pref.studio.run.positive')!)).toBe('global default');
  fireEvent.change(screen.getByRole('textbox', { name: 'Message' }), { target: { value: 'help me with this' } });
  await waitFor(() => expect((screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole('button', { name: 'Send' }));
  expect(JSON.parse(String(sends()[0].context)).values.positive).toBe('unsaved project prompt');
  expect(sends()[0].context).not.toContain('global default');
  await screen.findByText('Here it is.');
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

it('does not send while opening history and ignores an older open after New', async () => {
  const pending = deferred<typeof old>();
  const original = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((command, args) => command === 'native_chat_get' ? pending.promise as any : original(command, args));
  render(<I18nProvider><Chat endpoint="http://127.0.0.1:8188" root="/env" onSend={vi.fn()}/></I18nProvider>);
  await screen.findByRole('button', { name: old.title });
  fireEvent.change(screen.getByRole('textbox', { name: 'Message' }), { target: { value: 'new question' } });
  await waitFor(() => expect((screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole('button', { name: old.title }));
  fireEvent.keyDown(screen.getByRole('textbox', { name: 'Message' }), { key: 'Enter' });
  expect(sends()).toHaveLength(0);
  fireEvent.click(screen.getByRole('button', { name: 'New chat' }));
  await act(async () => pending.resolve(old));
  expect(screen.queryByText('flowing paragraph')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Send' }));
  await waitFor(() => expect(sends()).toHaveLength(1));
  expect(sends()[0].id).toBeUndefined();
});

it.each(['older open', 'root change'])('ignores a pending history result after %s', async reason => {
  const pending = deferred<typeof old>();
  const other = { ...old, id: 'other', title: 'Other conversation', messages: [{ ...old.messages[0], content: 'Other question' }] };
  const original = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((command, args) => {
    if (command === 'native_chat_list') return Promise.resolve([{ id: ID, title: old.title }, { id: other.id, title: other.title }]) as any;
    if (command === 'native_chat_get') return ((args as { id: string }).id === ID ? pending.promise : Promise.resolve(other)) as any;
    return original(command, args);
  });
  const view = render(<I18nProvider><Chat endpoint="http://127.0.0.1:8188" root="/env" onSend={vi.fn()}/></I18nProvider>);
  fireEvent.click(await screen.findByRole('button', { name: old.title }));
  if (reason === 'older open') {
    fireEvent.click(screen.getByRole('button', { name: other.title }));
    await screen.findByText('Other question');
  } else {
    view.rerender(<I18nProvider><Chat endpoint="http://127.0.0.1:8188" root="/other" onSend={vi.fn()}/></I18nProvider>);
  }
  await act(async () => pending.resolve(old));
  expect(screen.queryByText('flowing paragraph')).toBeNull();
  fireEvent.change(screen.getByRole('textbox', { name: 'Message' }), { target: { value: 'new question' } });
  await waitFor(() => expect((screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole('button', { name: 'Send' }));
  await waitFor(() => expect(sends()).toHaveLength(1));
  expect(sends()[0].id).toBe(reason === 'older open' ? other.id : undefined);
});

it('does not show an old root stream or reply in a new root', async () => {
  const pending = deferred<typeof old.messages[number]>();
  let emit!: (event: unknown) => void;
  const original = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((command, args) => {
    if (command === 'native_chat_send') { emit = (args as any).onEvent.onmessage; return pending.promise as any; }
    return original(command, args);
  });
  const view = render(<I18nProvider><Chat endpoint="http://127.0.0.1:8188" root="/env" onSend={vi.fn()}/></I18nProvider>);
  fireEvent.change(screen.getByRole('textbox', { name: 'Message' }), { target: { value: 'old root question' } });
  await waitFor(() => expect((screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole('button', { name: 'Send' }));
  view.rerender(<I18nProvider><Chat endpoint="http://127.0.0.1:8188" root="/other" onSend={vi.fn()}/></I18nProvider>);
  await act(async () => {
    emit({ event: 'chat', id: ID, title: old.title });
    emit({ event: 'text', text: 'stale streamed answer' });
    pending.resolve(old.messages[1]);
  });
  expect(screen.queryByText('old root question')).toBeNull();
  expect(screen.queryByText('stale streamed answer')).toBeNull();
  expect(screen.queryByText('flowing paragraph')).toBeNull();
  fireEvent.change(screen.getByRole('textbox', { name: 'Message' }), { target: { value: 'new root question' } });
  await waitFor(() => expect((screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole('button', { name: 'Send' }));
  expect(sends()[1].id).toBeUndefined();
});

it('web search is chosen in settings, then offered as a toggle; deleting asks once more', async () => {
  render(<I18nProvider><Chat endpoint="http://127.0.0.1:8188" root="/env" onSend={vi.fn()}/></I18nProvider>);
  fireEvent.click(await screen.findByRole('button', { name: 'Web search' }));
  fireEvent.click(screen.getByRole('radio', { name: 'DuckDuckGo (nothing to install)' }));
  expect(screen.getByText(/the query leaves this PC/)).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Save' }));
  await waitFor(() => expect(screen.getByRole('button', { name: 'Search' })).toBeTruthy());
  expect(vi.mocked(invoke)).toHaveBeenCalledWith('native_chat_settings_write', { root: '/env', settings: { search: 'duckduckgo', searxngUrl: '' } });
  fireEvent.click(screen.getByRole('button', { name: 'Search' }));
  fireEvent.change(screen.getByRole('textbox', { name: 'Message' }), { target: { value: 'latest illustrious checkpoints?' } });
  fireEvent.click(screen.getByRole('button', { name: 'Send' }));
  await waitFor(() => expect(sends()[0]).toMatchObject({ search: true, message: 'latest illustrious checkpoints?' }));
  expect(sends()[0].id).toBeUndefined();
  fireEvent.click(screen.getAllByRole('button', { name: 'Delete' })[0]);
  expect(vi.mocked(invoke).mock.calls.some(([c]) => c === 'native_chat_delete')).toBe(false);
  fireEvent.click(screen.getByRole('button', { name: 'Delete?' }));
  await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledWith('native_chat_delete', { root: '/env', id: ID }));
});

it('draws a render_picture answer with Generate, shows its progress and the picture, and records it on the message', async () => {
  const prompt = { target: 'anima', label: 'Anima', prompt: 'a fox in snow', negative: 'blurry', render: { aspect: '16:9' } };
  const job = { id: 'job1', baseUrl: 'http://127.0.0.1:8188', promptId: 'p', state: 'running', outputs: [], error: null, generationReady: false, progress: { value: 3, max: 20 } };
  let polls = 0;
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    const a = args as Record<string, any>;
    if (command === 'native_chat_send') {
      a.onEvent.onmessage({ event: 'chat', id: ID, title: 't' });
      return { role: 'assistant', content: 'Anima prompt, written by the Anima prompt writer:\n\na fox in snow\n\nDrawing it with anima.safetensors in Studio\'s Generate.', pictures: 0, prompts: [prompt], citations: [], tools: ['render_picture: Anima'], at: 7 };
    }
    if (command === 'native_workbench_poll') {
      polls++;
      return { jobs: [polls < 2 ? job : { ...job, state: 'succeeded', progress: undefined, outputRoot: 'C:\\env\\output\\', outputs: [{ filename: 'fox.png', subfolder: 'DEAS', type: 'output' }], metadata: { startedAt: 1, generationMs: 9100 } }], preparing: 0, capacity: 1, stopAvailable: true, admissionEpoch: 0, stopping: false };
    }
    if (command === 'native_chat_render_result') return { role: 'assistant', content: 'x', pictures: 0, prompts: [{ ...prompt, picture: a.result }], citations: [], tools: [], at: 7 };
    return base(command, args);
  });
  const onRender = vi.fn(async () => ({ job, width: 1344, height: 768, seed: 42, checkpoint: 'anima.safetensors' }));
  render(<I18nProvider><Chat endpoint="http://127.0.0.1:8188" root="/env" onSend={vi.fn()} studio={{ checkpoint: 'anima.safetensors', dialect: 'Anima' }} onRender={onRender as never}/></I18nProvider>);
  await waitFor(() => expect((screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement).disabled).toBe(false), { timeout: 3000 }).catch(() => {});
  fireEvent.click(screen.getByRole('checkbox',{name:'Allow image generation for this message'}));
  fireEvent.change(screen.getByRole('textbox', { name: 'Message' }), { target: { value: 'draw a fox in the snow, 16:9' } });
  fireEvent.keyDown(screen.getByRole('textbox', { name: 'Message' }), { key: 'Enter' });
  // Generate's settings travel with the message, so the LLM knows what draws.
  await waitFor(() => expect(sends()[0]?.studio).toMatchObject({ checkpoint: 'anima.safetensors', dialect: 'Anima', endpoint: 'http://127.0.0.1:8188', writerModel: 'openai:qwen' }));
  await waitFor(() => expect(onRender).toHaveBeenCalledWith(expect.objectContaining({ positive: 'a fox in snow', negative: 'blurry', aspect: '16:9' })));
  // The chat LLM is asked to step aside before the picture is handed over.
  const calls = vi.mocked(invoke).mock.calls.map(([c]) => c);
  expect(calls.indexOf('native_chat_free_llm')).toBeGreaterThan(calls.indexOf('native_chat_send'));
  await screen.findByText('Drawing… step 3 of 20');
  expect(screen.getByRole('button', { name: 'Stop' })).toBeTruthy();
  const img = await screen.findByRole('img', { name: 'a fox in snow' }, { timeout: 3000 });
  expect(img.getAttribute('src')).toBe('C:\\env\\output\\DEAS\\fox.png');
  expect(screen.getByText('1344×768 · anima · seed 42 · 9.1 s')).toBeTruthy();
  expect(vi.mocked(invoke)).toHaveBeenCalledWith('native_chat_render_result', { root: '/env', id: ID, at: 7, index: 0, result: { path: 'C:\\env\\output\\DEAS\\fox.png', width: 1344, height: 768, seed: 42, checkpoint: 'anima.safetensors', ms: 9100 } });
  await waitFor(() => expect(screen.getByRole('button', { name: 'Send' })).toBeTruthy());
});

it('says why a picture was not drawn when Generate refuses it', async () => {
  const prompt = { target: 'anima', label: 'Anima', prompt: 'a fox', negative: null, render: {} };
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    const a = args as Record<string, any>;
    if (command === 'native_chat_send') { a.onEvent.onmessage({ event: 'chat', id: ID, title: 't' }); return { role: 'assistant', content: 'Anima prompt, written by the Anima prompt writer:\n\na fox', pictures: 0, prompts: [prompt], citations: [], tools: [], at: 8 }; }
    if (command === 'native_chat_render_result') return { role: 'assistant', content: 'x', pictures: 0, prompts: [{ ...prompt, picture: a.result }], citations: [], tools: [], at: 8 };
    return base(command, args);
  });
  const onRender = vi.fn(async () => ({ error: 'The backend is not running. Start it under Backend, then ask again.' }));
  render(<I18nProvider><Chat endpoint="http://127.0.0.1:8188" root="/env" onSend={vi.fn()} studio={{ checkpoint: 'anima.safetensors', dialect: 'Anima' }} onRender={onRender as never}/></I18nProvider>);
  await waitFor(() => expect((screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement).disabled).toBe(true));
  fireEvent.click(screen.getByRole('checkbox',{name:'Allow image generation for this message'}));
  fireEvent.change(screen.getByRole('textbox', { name: 'Message' }), { target: { value: 'draw a fox' } });
  await waitFor(() => expect((screen.getByRole('button', { name: 'Send' }) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.keyDown(screen.getByRole('textbox', { name: 'Message' }), { key: 'Enter' });
  await screen.findByText('Not drawn: The backend is not running. Start it under Backend, then ask again.');
  expect(screen.getByRole('button', { name: 'Draw again' })).toBeTruthy();
});

it('right-click opens Rename, Pin to top and Delete; pinned chats show a pin', async () => {
  let pinned = false;
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    const a = args as Record<string, any>;
    if (command === 'native_chat_list') return [{ id: ID, title: old.title, updated: 2, count: 2, pinned }];
    if (command === 'native_chat_pin') { pinned = a.pinned; return undefined; }
    return base(command, args);
  });
  render(<I18nProvider><Chat endpoint="http://127.0.0.1:8188" root="/env" onSend={vi.fn()}/></I18nProvider>);
  const row = await screen.findByRole('button', { name: old.title });
  fireEvent.contextMenu(row);
  const menu = await screen.findByRole('menu');
  expect([...menu.querySelectorAll('button')].map(b => b.textContent)).toEqual(['Rename', 'Pin to top', 'Delete']);
  fireEvent.click(screen.getByRole('menuitem', { name: 'Pin to top' }));
  await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledWith('native_chat_pin', { root: '/env', id: ID, pinned: true }));
  expect(screen.queryByRole('menu')).toBeNull();
  await screen.findByLabelText('Pinned');
  // Unpin is offered once pinned; Escape closes without acting.
  fireEvent.contextMenu(screen.getByRole('button', { name: /how does LTX/ }));
  expect(await screen.findByRole('menuitem', { name: 'Unpin' })).toBeTruthy();
  fireEvent.keyDown(window, { key: 'Escape' });
  await waitFor(() => expect(screen.queryByRole('menu')).toBeNull());
  // Rename puts the title in an edit box; Delete still asks first.
  fireEvent.contextMenu(screen.getByRole('button', { name: /how does LTX/ }));
  fireEvent.click(await screen.findByRole('menuitem', { name: 'Rename' }));
  expect((await screen.findByRole('textbox', { name: 'Rename' }) as HTMLInputElement).value).toBe(old.title);
  fireEvent.keyDown(screen.getByRole('textbox', { name: 'Rename' }), { key: 'Escape' });
  fireEvent.contextMenu(screen.getByRole('button', { name: /how does LTX/ }));
  fireEvent.click(await screen.findByRole('menuitem', { name: 'Delete' }));
  expect(screen.getByRole('button', { name: 'Delete?' })).toBeTruthy();
  expect(vi.mocked(invoke).mock.calls.some(([c]) => c === 'native_chat_delete')).toBe(false);
});
