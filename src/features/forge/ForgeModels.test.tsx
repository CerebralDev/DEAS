import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { useForgeModels } from './ForgeButton';
import { nativeModelId } from '../../lib/forge';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { localStorage.clear(); });
afterEach(() => { cleanup(); vi.resetAllMocks(); });

test.each([false, true])('OpenAI recommendations default to Terra and preserve explicit choices (nodepack=%s)', async nodepack => {
  const prefix = nodepack ? 'openai' : 'openaiapi';
  const ids = ['gpt-4.1', 'gpt-6-luna', 'gpt-6.1-sol', 'gpt-6.1-terra'];
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'native_llm_read') return { provider: 'openaiapi', baseUrl: 'https://api.openai.com/v1' };
    if (command === 'native_llm_models') return { models: ids.map(id => ({ id, name: id, provider: 'openaiapi' })), warnings: [] };
    if (command === 'native_forge') return { models: ids.map(id => ({ id: `${prefix}:${id}`, label: id })), errors: {}, creativity: [], shot_counts: [] };
    throw Error(`Unexpected ${command}`);
  });
  const { result } = renderHook(() => useForgeModels('http://localhost:8188', '/env', true, nodepack));
  act(() => result.current.load());
  await waitFor(() => expect(result.current.model).toBe(`${prefix}:gpt-6.1-terra`));
  expect(result.current.usable.map(m => m.id)).toEqual(['gpt-6.1-terra', 'gpt-6.1-sol', 'gpt-6-luna', 'gpt-4.1'].map(id => `${prefix}:${id}`));
  act(() => result.current.setModel(`${prefix}:gpt-4.1`));
  act(() => window.dispatchEvent(new Event('deas:llm-changed')));
  act(() => result.current.load());
  await waitFor(() => expect(result.current.model).toBe(`${prefix}:gpt-4.1`));
  expect(vi.mocked(invoke).mock.calls.every(([cmd, args]) => cmd !== 'native_forge' || (args as { route: string }).route === 'models')).toBe(true);
});

test.each(['local:nested/model.gguf', 'ollama:qwen:latest', 'openai:remote'])('native request strips only the provider prefix from %s', id => {
  expect(nativeModelId(id)).toBe(id.slice(id.indexOf(':') + 1));
});

test('native model discovery lists provider-qualified models and diagnostics without ComfyUI', async () => {
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'native_llm_models') return { models: [
      { provider: 'local', id: 'nested/model.gguf', name: 'Local model' },
      { provider: 'ollama', id: 'qwen:latest', name: 'Qwen' },
      { provider: 'openai', id: 'remote', name: 'Remote' },
    ], warnings: ['Optional endpoint unavailable'] };
    throw Error(`Unexpected ${command}`);
  });
  const { result } = renderHook(() => useForgeModels('', '/env'));
  act(() => result.current.load());
  await waitFor(() => expect(result.current.usable.map(m => m.id)).toEqual(['ollama:qwen:latest', 'openai:remote']));
  expect(result.current.usable.map(m => m.label)).toEqual(['Qwen', 'Remote']);
  expect(result.current.model).toBe('ollama:qwen:latest');
  expect(result.current.list!.models.find(m => m.id.startsWith('local:'))?.disabled).toBe(true);
  expect(Object.values(result.current.list!.errors)).toEqual(['Optional endpoint unavailable']);
  expect(vi.mocked(invoke).mock.calls).toEqual([['native_llm_models', { root: '/env' }]]);
});

test('the native writer can pick a local GGUF, with a server staying the default', async () => {
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'native_llm_models') return { models: [{ provider: 'local', id: 'nested/model.gguf', name: 'Local model' }, { provider: 'openai', id: 'server-model', name: 'Server model' }], warnings: [] };
    throw Error(`Unexpected ${command}`);
  });
  const { result } = renderHook(() => useForgeModels('', '/env', true, false, true));
  act(() => result.current.load());
  await waitFor(() => expect(result.current.list).not.toBeNull());
  expect(result.current.usable.map(m => m.id)).toEqual(['openai:server-model', 'local:nested/model.gguf']);
  expect(result.current.model).toBe('openai:server-model');
  expect(result.current.serverOnly).toBe(false);
  expect(Object.values(result.current.list!.errors)).toEqual([]);
});

test('native model discovery asks for a server only when local GGUFs are all it found', async () => {
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'native_llm_models') return { models: [{ provider: 'local', id: 'nested/model.gguf', name: 'Local model' }], warnings: [] };
    throw Error(`Unexpected ${command}`);
  });
  const { result } = renderHook(() => useForgeModels('', '/env'));
  act(() => result.current.load());
  await waitFor(() => expect(result.current.list).not.toBeNull());
  expect(result.current.usable).toEqual([]);
  expect(Object.values(result.current.list!.errors)).toEqual(['forge.native.serverRequired']);
});
