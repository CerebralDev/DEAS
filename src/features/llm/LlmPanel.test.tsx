import { StrictMode } from 'react';
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { I18nProvider } from '../../i18n/I18nProvider';
import { defaultLlmConfig, discoverLlmModels, generateLlm, type LlmConfig, type LlmModels, type LlmResult } from '../../lib/llm';
import { LlmPanel } from './LlmPanel';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn(() => true) }));
const config: LlmConfig = { ...defaultLlmConfig, provider: 'ollama' };
const local = { id: 'same/"model', name: 'Chat', provider: 'local' as const };
const remote = { ...local, provider: 'ollama' as const };
const listing: LlmModels = { models: [local, remote, { ...remote, id: 'second' }], warnings: [] };
const completion: LlmResult = { text: 'Hello from the actual backend.', backend: 'local' };
const key = (provider = 'local', id = local.id) => JSON.stringify([provider, id]);
const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
const ui = (root = '/env') => <I18nProvider><LlmPanel root={root} /></I18nProvider>;
const button = (name: string) => screen.getByRole('button', { name }) as HTMLButtonElement;
const commands = () => vi.mocked(invoke).mock.calls.map(([command]) => command);
const ready = async () => { await waitFor(() => expect(button('Refresh models').disabled).toBe(false)); };
const choose = (provider = 'local') => fireEvent.change(screen.getByRole('combobox', { name: 'Test model' }), { target: { value: key(provider) } });
const edit = () => fireEvent.change(screen.getByRole('textbox', { name: 'Endpoint URL' }), { target: { value: 'http://localhost:8080/v1' } });
beforeEach(() => {
  localStorage.clear(); localStorage.setItem('deas.locale.v1', 'en');
  vi.mocked(isTauri).mockReturnValue(true);
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === 'native_llm_read') return { ...config };
    if (command === 'native_llm_models') return listing;
    if (command === 'native_llm_write') return (args as { config: LlmConfig }).config;
    if (command === 'native_llm_generate') return completion;
    throw new Error(`Unexpected command: ${command}`);
  });
});
afterEach(() => { cleanup(); vi.resetAllMocks(); });

it('puts discovered recommendations first, defaults Terra and preserves the test selection on refresh without inference', async () => {
  const ids = ['gpt-4.1', 'gpt-6-luna', 'gpt-6.1-sol', 'gpt-6.1-terra'];
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'native_llm_read') return { provider: 'openaiapi', baseUrl: 'https://api.openai.com/v1', apiKey: 'fixture' };
    if (command === 'native_llm_models') return { models: ids.map(id => ({ id, name: id, provider: 'openaiapi' })), warnings: [] };
    throw Error(`Unexpected ${command}`);
  });
  render(ui()); await ready();
  const select = screen.getByRole('combobox', { name: 'Test model' }) as HTMLSelectElement;
  expect(select.value).toBe(key('openaiapi', 'gpt-6.1-terra'));
  expect(Array.from(select.options).slice(1).map(o => o.value)).toEqual(['gpt-6.1-terra', 'gpt-6.1-sol', 'gpt-6-luna', 'gpt-4.1'].map(id => key('openaiapi', id)));
  expect(select.querySelector('optgroup')?.label).toBe('Recommended for prompts & images');
  fireEvent.change(select, { target: { value: key('openaiapi', 'gpt-4.1') } });
  fireEvent.click(button('Refresh models')); await ready();
  expect(select.value).toBe(key('openaiapi', 'gpt-4.1'));
  expect(commands()).not.toContain('native_llm_generate');
});

it('adds official OpenAI separately, fixes its URL, requires a key and never infers on save', async () => {
  render(ui()); await ready();
  fireEvent.change(screen.getByRole('combobox', { name: 'Provider' }), { target: { value: 'openaiapi' } });
  expect(screen.getByRole('option', { name: 'OpenAI compatible' })).toBeTruthy();
  const url = screen.getByRole('textbox', { name: 'Endpoint URL' }) as HTMLInputElement;
  expect(url.value).toBe('https://api.openai.com/v1'); expect(url.readOnly).toBe(true);
  expect(button('Save LLM settings').disabled).toBe(true);
  fireEvent.change(screen.getByLabelText('OpenAI API key (required)'), { target: { value: 'fixture-key' } });
  fireEvent.click(button('Save LLM settings')); await ready();
  expect(invoke).toHaveBeenCalledWith('native_llm_write', { root: '/env', config: { provider: 'openaiapi', baseUrl: 'https://api.openai.com/v1', apiKey: 'fixture-key' } });
  expect(commands()).not.toContain('native_llm_generate');
});

it('only reads config and all models on mount, with no global model/path/tuning controls or implicit selection', async () => {
  render(ui()); await ready();
  expect(commands()).toEqual(['native_llm_read', 'native_llm_models']);
  expect(screen.getAllByRole('textbox').map(input => input.getAttribute('type'))).toEqual(['url', null]);
  expect(screen.queryByRole('checkbox')).toBeNull();
  expect(screen.queryByRole('spinbutton')).toBeNull();
  expect(button('Test request').disabled).toBe(true);
  expect((screen.getByRole('combobox', { name: 'Test model' }) as HTMLSelectElement).value).toBe('');
  const groups = screen.getAllByRole('group');
  expect(within(groups.find(group => group.getAttribute('label') === 'Local llama.cpp')!).getAllByRole('option')).toHaveLength(1);
  const options = within(groups.find(group => group.getAttribute('label') === 'Ollama')!).getAllByRole('option');
  expect(new Set(options.map(option => option.textContent)).size).toBe(2);
  expect(screen.getByRole('option', { name: 'No external server (local automatic)' })).toBeTruthy();
});

it.each(['local', 'ollama'])('carries the explicit %s test model only in generation IPC and shows actual backend/text', async provider => {
  render(ui()); await ready(); choose(provider);
  fireEvent.change(screen.getByRole('textbox', { name: 'Test prompt' }), { target: { value: 'Private prompt' } });
  fireEvent.click(button('Test request'));
  await screen.findByText(completion.text);
  expect(invoke).toHaveBeenCalledWith('native_llm_generate', { root: '/env', request: { model: { id: local.id, provider }, system: 'Respond briefly and directly.', prompt: 'Private prompt', maxTokens: 64, temperature: 0.2 } });
  expect(screen.getByText('Actual backend: Local llama.cpp')).toBeTruthy();
  expect(commands()).not.toContain('native_llm_write');
  expect(Object.keys(localStorage)).toEqual(['deas.locale.v1']);
});

it.each(['ollama', 'openai', 'local'])('saves only the %s endpoint config, clears stale options on edit and refreshes after save', async provider => {
  render(ui()); await ready(); choose();
  edit();
  fireEvent.change(screen.getByRole('combobox', { name: 'Provider' }), { target: { value: provider } });
  expect(screen.queryByRole('option', { name: /Chat/ })).toBeNull();
  expect(button('Test request').disabled).toBe(true);
  expect(button('Refresh models').disabled).toBe(true);
  fireEvent.click(button('Save LLM settings'));
  await ready();
  expect(invoke).toHaveBeenCalledWith('native_llm_write', { root: '/env', config: { provider, baseUrl: 'http://localhost:8080/v1' } });
  expect(commands().slice(-2)).toEqual(['native_llm_write', 'native_llm_models']);
  expect(button('Test request').disabled).toBe(true);
  expect(screen.getAllByRole('option', { name: /Chat/ })).toHaveLength(3);
});

it('retains local options when discovery returns remote warnings and when a later refresh fails', async () => {
  vi.mocked(invoke).mockImplementation(async command => command === 'native_llm_read' ? config : { models: [local], warnings: ['External server unavailable'] });
  render(ui()); await ready();
  expect(screen.getByText('External server unavailable')).toBeTruthy();
  choose(); expect(button('Test request').disabled).toBe(false);
  vi.mocked(invoke).mockRejectedValueOnce('Discovery failed');
  fireEvent.click(button('Refresh models')); await ready();
  expect((await screen.findByRole('alert')).textContent).toContain('Discovery failed');
  expect(screen.getByRole('option', { name: /Chat/ })).toBeTruthy();
  choose(); expect(button('Test request').disabled).toBe(false);
});

it.each(['read', 'models', 'write', 'generate'])('shows %s errors without fabricated completion', async operation => {
  if (operation === 'read' || operation === 'models') {
    const previous = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation((command, args, options) => command === `native_llm_${operation}` ? Promise.reject('Request denied') : previous(command, args, options));
  }
  render(ui());
  if (operation === 'write' || operation === 'generate') {
    await ready(); vi.mocked(invoke).mockRejectedValueOnce('Request denied');
    if (operation === 'write') { edit(); fireEvent.click(button('Save LLM settings')); }
    else { choose(); fireEvent.click(button('Test request')); }
  }
  expect((await screen.findByRole('alert')).textContent).toContain('Request denied');
  expect(screen.queryByText(completion.text)).toBeNull();
  expect(screen.queryByText('LLM settings saved.')).toBeNull();
});

it.each(['write', 'generate', 'models'])('locks duplicate %s requests synchronously', async operation => {
  render(ui()); await ready();
  const pending = deferred<LlmConfig | LlmResult | LlmModels>();
  vi.mocked(invoke).mockReturnValueOnce(pending.promise);
  if (operation === 'write') edit();
  if (operation === 'generate') choose();
  const action = button(operation === 'write' ? 'Save LLM settings' : operation === 'generate' ? 'Test request' : 'Refresh models');
  const before = commands().filter(command => command === `native_llm_${operation}`).length;
  fireEvent.click(action); fireEvent.click(action);
  expect(commands().filter(command => command === `native_llm_${operation}`)).toHaveLength(before + 1);
  expect(button('Test request').disabled).toBe(true);
  await act(async () => { pending.resolve(operation === 'write' ? config : operation === 'generate' ? completion : listing); });
});

it.each(['read', 'models', 'write', 'generate'])('discards stale %s responses after root changes', async operation => {
  const pending = deferred<LlmConfig | LlmModels | LlmResult>();
  if (operation === 'read') vi.mocked(invoke).mockReturnValueOnce(pending.promise);
  if (operation === 'models') {
    const previous = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation((command, args, options) => command === 'native_llm_models' && (args as { root: string }).root === '/env' ? pending.promise : previous(command, args, options));
  }
  const view = render(ui());
  if (operation === 'write' || operation === 'generate') {
    await ready(); vi.mocked(invoke).mockReturnValueOnce(pending.promise);
    if (operation === 'write') { edit(); fireEvent.click(button('Save LLM settings')); }
    else { choose(); fireEvent.click(button('Test request')); }
  } else if (operation === 'models') await screen.findByRole('combobox', { name: 'Provider' });
  view.rerender(ui('/new')); await ready();
  await act(async () => { pending.resolve(operation === 'generate' ? completion : operation === 'models' ? { models: [], warnings: ['Stale warning'] } : { provider: 'openai', baseUrl: 'http://stale' }); });
  expect((screen.getByRole('combobox', { name: 'Provider' }) as HTMLSelectElement).value).toBe('ollama');
  expect(screen.getAllByRole('option', { name: /Chat/ })).toHaveLength(3);
  expect(screen.queryByText('Stale warning')).toBeNull();
  expect(screen.queryByText(completion.text)).toBeNull();
  expect(screen.queryByText('LLM settings saved.')).toBeNull();
});

it('ignores a late refresh error after unmount and old read results during StrictMode replay', async () => {
  const old = deferred<LlmConfig>();
  vi.mocked(invoke).mockReturnValueOnce(old.promise);
  const view = render(<StrictMode>{ui()}</StrictMode>); await ready();
  await act(async () => { old.resolve({ provider: 'openai', baseUrl: 'http://stale' }); });
  expect((screen.getByRole('combobox', { name: 'Provider' }) as HTMLSelectElement).value).toBe('ollama');
  const refresh = deferred<LlmModels>(); vi.mocked(invoke).mockReturnValueOnce(refresh.promise);
  fireEvent.click(button('Refresh models')); view.unmount();
  await act(async () => { refresh.reject('Late failure'); });
  expect(screen.queryByRole('alert')).toBeNull();
});

it('exports exact shared discovery and generation contracts', async () => {
  expect(defaultLlmConfig).toEqual({ provider: 'local', baseUrl: 'http://127.0.0.1:11434' });
  expect(await discoverLlmModels('/other')).toEqual(listing);
  expect(invoke).toHaveBeenCalledWith('native_llm_models', { root: '/other' });
  const request = { model: { id: local.id, provider: 'local' as const }, system: 'System', prompt: 'Prompt', maxTokens: 32, temperature: 0 };
  expect(await generateLlm('/other', request)).toEqual(completion);
  expect(invoke).toHaveBeenCalledWith('native_llm_generate', { root: '/other', request });
});

it('does not invoke IPC in a browser or with an empty root', async () => {
  vi.mocked(isTauri).mockReturnValue(false);
  const view = render(ui()); await screen.findByText('Available in the desktop app only.');
  expect(invoke).not.toHaveBeenCalled();
  vi.mocked(isTauri).mockReturnValue(true); view.rerender(ui(''));
  await screen.findByText('Choose an environment root first.');
  expect(invoke).not.toHaveBeenCalled();
});

it('offers an API key for OpenAI-compatible servers only, saves it trimmed and drops it with the provider', async () => {
  render(ui()); await ready();
  expect(screen.queryByLabelText('API key (optional)')).toBeNull();
  fireEvent.change(screen.getByRole('combobox', { name: 'Provider' }), { target: { value: 'openai' } });
  fireEvent.change(screen.getByLabelText('API key (optional)'), { target: { value: ' sk-test ' } });
  expect((screen.getByLabelText('API key (optional)') as HTMLInputElement).type).toBe('password');
  fireEvent.click(button('Save LLM settings'));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith('native_llm_write', { root: '/env', config: { provider: 'openai', baseUrl: config.baseUrl, apiKey: 'sk-test' } }));
  fireEvent.change(screen.getByRole('combobox', { name: 'Provider' }), { target: { value: 'ollama' } });
  expect(screen.queryByLabelText('API key (optional)')).toBeNull();
  fireEvent.click(button('Save LLM settings'));
  await waitFor(() => expect(invoke).toHaveBeenLastCalledWith('native_llm_write', { root: '/env', config: { provider: 'ollama', baseUrl: config.baseUrl } }));
});
