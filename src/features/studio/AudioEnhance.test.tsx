import { act, cleanup, fireEvent, render, renderHook, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { I18nProvider, LOCALE_STORAGE_KEY } from '../../i18n/I18nProvider';
import { revealTarget, searchIndex } from '../../lib/search';
import { AudioEnhance } from './AudioEnhance';
import { useAudioEnhance } from './useAudioEnhance';
import { exportEnhancedAudio, readAudioEnhanceCapabilities } from '../../lib/audioEnhance';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn(() => true) }));
beforeEach(() => {
  Object.defineProperty(HTMLDialogElement.prototype, 'showModal', { configurable: true, value: function (this: HTMLDialogElement) { this.open = true; } });
  localStorage.setItem(LOCALE_STORAGE_KEY, 'en');
  vi.mocked(isTauri).mockReturnValue(true);
  vi.mocked(invoke).mockImplementation(async command => command === 'native_audio_enhance_capabilities' ? { clean: true, deep: false, stems: false } : command === 'native_audio_enhance_install_status' ? { running: false, mode: '', stage: '', downloaded: 0, total: 0 } : null);
});
afterEach(() => { cleanup(); localStorage.clear(); vi.resetAllMocks(); });
const mode = () => document.querySelector('[data-audio-enhance-mode]') as HTMLSelectElement;
const run = () => document.querySelector('[data-audio-enhance-run]') as HTMLButtonElement;
const consentDialog = () => document.querySelector('dialog.confirm-dialog');
const acceptConsent = async () => { await act(async () => { fireEvent.click(document.querySelector('[data-confirm-dialog]')!); }); };
async function mount(source?: { sourceId: string }) {
  await act(async () => { render(<I18nProvider><AudioEnhance scope="tve" source={source}/></I18nProvider>); });
  await act(async () => { fireEvent.focus(mode()); });
}

test.each(['post', 'tve'] as const)('optional engines stay visible and installed buttons are blocked in %s', async scope => {
  vi.mocked(invoke).mockImplementation(async command => command === 'native_audio_enhance_capabilities'
    ? { clean: true, deep: true, stems: false }
    : command === 'native_audio_enhance_install_status' ? { running: false, mode: '', stage: '', downloaded: 0, total: 0 } : null);
  await act(async () => { render(<I18nProvider><AudioEnhance scope={scope}/></I18nProvider>); });
  await act(async () => { fireEvent.focus(mode()); });
  const group = screen.getByRole('group', { name: 'Optional audio engines' });
  expect(group.tagName).toBe('DIV');
  const deep = group.querySelector<HTMLButtonElement>('[data-audio-enhance-install="deep"]')!;
  const stems = group.querySelector<HTMLButtonElement>('[data-audio-enhance-install="stems"]')!;
  expect(deep.disabled).toBe(true);
  expect(stems.disabled).toBe(false);
  fireEvent.click(deep);
  expect(invoke).not.toHaveBeenCalledWith('native_audio_enhance_install', expect.anything());
});

test('defaults off, disables unavailable engines and requires a source', async () => {
  await mount();
  expect(mode().value).toBe('off');
  expect(mode().querySelector<HTMLOptionElement>('[value="deep"]')!.disabled).toBe(true);
  expect(mode().querySelector<HTMLOptionElement>('[value="stems"]')!.disabled).toBe(true);
  expect(revealTarget('[data-audio-enhance-mode]').state).toBe('revealed');
  fireEvent.change(mode(), { target: { value: 'clean' } });
  expect(run().disabled).toBe(true);
  expect(screen.getByText(/Select an inspected/)).toBeTruthy();
  expect(invoke).not.toHaveBeenCalledWith('native_audio_enhance_export', expect.anything());
});

test('TVE audio-only exports without a backend, upload or video stage', async () => {
  await mount({ sourceId: 'retained' });
  fireEvent.change(mode(), { target: { value: 'clean' } });
  expect(run().disabled).toBe(false);
  await act(async () => { fireEvent.click(run()); });
  expect(invoke).toHaveBeenCalledWith('native_audio_enhance_export', { request: { sourceId: 'retained', mode: 'clean', profile: 'mix', confirmed: true, title: 'Enhance audio and save…' } });
  expect(vi.mocked(invoke).mock.calls.every(([c]) => !String(c).includes('workbench') && !String(c).includes('upload'))).toBe(true);
  expect(document.querySelector('[data-audio-enhance-saved]')).toBeNull();
});

test('completed output authority is job and index, not arbitrary paths', async () => {
  await act(async () => { render(<I18nProvider><AudioEnhance scope="post" source={{ jobId: 'owned', index: 2 }}/></I18nProvider>); });
  await act(async () => { fireEvent.focus(mode()); });
  fireEvent.change(mode(), { target: { value: 'clean' } });
  await act(async () => { fireEvent.click(run()); });
  expect(invoke).toHaveBeenCalledWith('native_audio_enhance_export', { request: expect.objectContaining({ jobId: 'owned', index: 2, mode: 'clean' }) });
});

test('deep cannot process a mixed music track', async () => {
  vi.mocked(invoke).mockResolvedValue({ clean: true, deep: true, stems: true });
  await mount({ sourceId: 'retained' });
  fireEvent.change(mode(), { target: { value: 'deep' } });
  expect(run().disabled).toBe(true);
  expect(screen.getByRole('alert').textContent).toContain('speech only');
  fireEvent.change(document.querySelector('[data-audio-enhance-profile]')!, { target: { value: 'speech' } });
  expect(run().disabled).toBe(false);
});

test('single admission, success and operational failure remain visible', async () => {
  let resolve!: (v: unknown) => void;
  vi.mocked(invoke).mockImplementation(async command => command === 'native_audio_enhance_capabilities' ? { clean: true, deep: false, stems: false } : new Promise(r => { resolve = r; }));
  await mount({ sourceId: 'retained' });
  fireEvent.change(mode(), { target: { value: 'clean' } });
  await act(async () => { fireEvent.click(run()); fireEvent.click(run()); });
  expect(vi.mocked(invoke).mock.calls.filter(([c]) => c === 'native_audio_enhance_export')).toHaveLength(1);
  await act(async () => { resolve({ path: '/chosen/new.mkv', sha256: 'a'.repeat(64), size: 42 }); });
  expect(screen.getByRole('status').textContent).toContain('/chosen/new.mkv');
  vi.mocked(invoke).mockRejectedValue(new Error('AUDIO_ENHANCE_FAILED'));
  await act(async () => { fireEvent.click(run()); });
  expect(screen.getByRole('alert').textContent).toContain('AUDIO_ENHANCE_FAILED');
});

test('cancel stays available under parent busy and results retain their operation source', async () => {
  let resolve!: (v: unknown) => void;
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'native_audio_enhance_install_status') return { running: false, mode: '', stage: '', downloaded: 0, total: 0 };
    if (command === 'native_audio_enhance_capabilities') return { clean: true, deep: false, stems: false };
    if (command === 'native_audio_enhance_cancel') return;
    return new Promise(r => { resolve = r; });
  });
  let view!: ReturnType<typeof render>;
  await act(async () => { view = render(<I18nProvider><AudioEnhance scope="tve" name="first.mkv" source={{ sourceId: 'first' }}/></I18nProvider>); });
  await act(async () => { fireEvent.focus(mode()); });
  fireEvent.change(mode(), { target: { value: 'clean' } });
  await act(async () => { fireEvent.click(run()); });
  await act(async () => { view.rerender(<I18nProvider><AudioEnhance scope="tve" name="first.mkv" source={{ sourceId: 'first' }} disabled/></I18nProvider>); });
  const cancel = document.querySelector('[data-audio-enhance-cancel]') as HTMLButtonElement;
  expect(cancel.disabled).toBe(false);
  await act(async () => { fireEvent.click(cancel); });
  expect(invoke).toHaveBeenCalledWith('native_audio_enhance_cancel');
  await act(async () => { view.rerender(<I18nProvider><AudioEnhance scope="tve" source={{ sourceId: 'second' }}/></I18nProvider>); });
  await act(async () => { resolve({ path: '/first.mkv', sha256: 'a'.repeat(64), size: 42 }); });
  expect(document.querySelector('[data-audio-enhance-saved]')?.textContent).toContain('first.mkv');
  expect(document.querySelector('[data-audio-enhance-saved]')?.textContent).toContain('/first.mkv');
});

test('restored Clean probes without selector interaction; probe failure is not reported as missing tools', async () => {
  localStorage.setItem('deas.pref.studio.audioEnhance.tve.mode', JSON.stringify('clean'));
  vi.mocked(invoke).mockRejectedValue(new Error('AUDIO_PROBE: IPC unavailable'));
  await act(async () => { render(<I18nProvider><AudioEnhance scope="tve" source={{ sourceId: 'retained' }}/></I18nProvider>); });
  expect(invoke).toHaveBeenCalledWith('native_audio_enhance_capabilities');
  expect(screen.getByRole('alert').textContent).toContain('IPC unavailable');
  expect(screen.queryByText(/FFmpeg \/ FFprobe unavailable/)).toBeNull();
  expect(run().disabled).toBe(true);
});

test('installation needs explicit consent and never reports readiness before a successful capability probe', async () => {
  let finish!: () => void;
  let installed = false;
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'native_audio_enhance_install_status') return { running: false, mode: '', stage: '', downloaded: 0, total: 0 };
    if (command === 'native_audio_enhance_capabilities') return { clean: true, deep: installed, stems: false };
    if (command === 'native_audio_enhance_install') { await new Promise<void>(resolve => { finish = resolve; }); installed = true; return { clean: true, deep: true, stems: false }; }
    return null;
  });
  await mount({ sourceId: 'retained' });
  const button = document.querySelector('[data-audio-enhance-install="deep"]') as HTMLButtonElement;
  fireEvent.click(button);
  expect(consentDialog()).not.toBeNull();
  expect(invoke).not.toHaveBeenCalledWith('native_audio_enhance_install', expect.anything());
  await acceptConsent();
  expect(invoke).toHaveBeenCalledWith('native_audio_enhance_install', { mode: 'deep', confirmed: true });
  expect(button.disabled).toBe(true);
  expect(mode().disabled).toBe(true);
  expect(run().disabled).toBe(true);
  expect(mode().querySelector<HTMLOptionElement>('[value="deep"]')!.disabled).toBe(true);
  await act(async () => { finish(); });
  expect(mode().querySelector<HTMLOptionElement>('[value="deep"]')!.disabled).toBe(false);
  expect((document.querySelector('[data-audio-enhance-install="deep"]') as HTMLButtonElement).disabled).toBe(true);
  expect(mode().value).toBe('off');
});

test('running install reports the real localized stage and stays honest without byte totals', async () => {
  let status = { running: true, mode: 'stems', stage: 'python-environment', downloaded: 0, total: 0 };
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'native_audio_enhance_install_status') return status;
    if (command === 'native_audio_enhance_capabilities') return { clean: true, deep: true, stems: false };
    if (command === 'native_audio_enhance_install') return new Promise(() => {}); // the install stays in flight for this test
    return null;
  });
  await mount({ sourceId: 'retained' });
  const line = () => document.querySelector('[data-audio-enhance-install-status]')!.textContent!;
  await act(async () => { fireEvent.click(document.querySelector('[data-audio-enhance-install="stems"]') as HTMLButtonElement); }); // already installing: the button is blocked, the poller reports the running stage
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 900)); }); // one 750 ms poll tick
  expect(line()).toContain('Creating the Python environment');
  expect(line()).not.toContain('bytes');
  status = { running: true, mode: 'stems', stage: 'packages', downloaded: 2048, total: 4096 };
  await act(async () => { await new Promise(resolve => setTimeout(resolve, 900)); });
  expect(line()).toContain('Installing packages');
  expect(line()).toContain('2048 / 4096');
  expect((document.querySelector('[data-audio-enhance-install-cancel]') as HTMLButtonElement).disabled).toBe(false);
});

test('failed install remains unavailable and can be retried with fresh consent', async () => {
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'native_audio_enhance_install_status') return { running: false, mode: '', stage: '', downloaded: 0, total: 0 };
    if (command === 'native_audio_enhance_install') throw new Error('AUDIO_INSTALL_FAILED: offline');
    return { clean: true, deep: false, stems: false };
  });
  await mount({ sourceId: 'retained' });
  const button = document.querySelector('[data-audio-enhance-install="stems"]') as HTMLButtonElement;
  expect(button.disabled).toBe(false);
  expect(document.querySelector('[data-audio-enhance-weights-ack]')).toBeNull();
  await act(async () => { fireEvent.click(button); });
  expect(consentDialog()).not.toBeNull();
  await acceptConsent();
  expect(screen.getByRole('alert').textContent).toContain('offline');
  expect(mode().querySelector<HTMLOptionElement>('[value="stems"]')!.disabled).toBe(true);
  expect(button.disabled).toBe(false);
  await act(async () => { fireEvent.click(button); });
  expect(consentDialog()).not.toBeNull(); // retry requires fresh consent
  await acceptConsent();
  expect(invoke).toHaveBeenCalledWith('native_audio_enhance_install', { mode: 'stems', confirmed: true });
  expect(vi.mocked(invoke).mock.calls.filter(([c]) => c === 'native_audio_enhance_install')).toHaveLength(2);
});

test.each(['en', 'de', 'fr', 'ja', 'tr', 'zh'])('Stems uses explicit Smule MIT code + checkpoint consent in %s, without a legacy weights gate', async locale => {
  localStorage.setItem(LOCALE_STORAGE_KEY, locale);
  await mount({ sourceId: 'retained' });
  const button = document.querySelector('[data-audio-enhance-install="stems"]') as HTMLButtonElement;
  expect(button.disabled).toBe(false);
  expect(document.querySelector('[data-audio-enhance-weights-ack]')).toBeNull();
  fireEvent.click(button);
  const message = consentDialog()!.textContent!;
  expect(message).toContain('Smule Labs Windowed RoFormer');
  expect(message).toContain('MIT');
  expect(invoke).not.toHaveBeenCalledWith('native_audio_enhance_install', expect.anything());
  expect(document.body.textContent).not.toMatch(/Demucs|HTDemucs|unclear|rights are/i);
});

test('Smule installation is searchable in both locations and returns readiness only after probing', async () => {
  const entries = searchIndex.filter(entry => entry.aliases?.includes('Smule'));
  expect(entries.map(entry => entry.target)).toEqual([
    '[data-audio-enhance="post"] [data-audio-enhance-install="stems"]',
    '[data-audio-enhance="tve"] [data-audio-enhance-install="stems"]',
  ]);
  let finish!: () => void;
  let installed = false;
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'native_audio_enhance_install_status') return { running: false, mode: '', stage: '', downloaded: 0, total: 0 };
    if (command === 'native_audio_enhance_capabilities') return { clean: true, deep: false, stems: installed };
    if (command === 'native_audio_enhance_install') { await new Promise<void>(resolve => { finish = resolve; }); installed = true; return { clean: true, deep: false, stems: true }; }
    return null;
  });
  await mount({ sourceId: 'retained' });
  await act(async () => { fireEvent.click(document.querySelector('[data-audio-enhance-install="stems"]')!); });
  await acceptConsent();
  expect(invoke).toHaveBeenCalledWith('native_audio_enhance_install', { mode: 'stems', confirmed: true });
  expect(run().disabled).toBe(true);
  expect(mode().disabled).toBe(true);
  await act(async () => { finish(); });
  expect(mode().querySelector<HTMLOptionElement>('[value="stems"]')!.disabled).toBe(false);
  expect(mode().value).toBe('off');
  fireEvent.change(mode(), { target: { value: 'stems' } });
  expect(run().disabled).toBe(false);
});

test('reopened installation observes real progress and cancellation without inferring readiness', async () => {
  vi.useFakeTimers();
  let running = true;
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'native_audio_enhance_install_status') return { running, mode: 'stems', stage: 'checkpoint', downloaded: 12, total: 24 };
    if (command === 'native_audio_enhance_install_cancel') { running = false; return; }
    if (command === 'native_audio_enhance_capabilities') return { clean: true, deep: false, stems: false };
    throw new Error(`unexpected ${command}`);
  });
  try {
    await mount({ sourceId: 'retained' });
    expect(document.querySelector('[data-audio-enhance-install-status]')?.textContent).toContain('checkpoint: 12 / 24 bytes');
    expect(mode().disabled).toBe(true);
    await act(async () => { fireEvent.click(document.querySelector('[data-audio-enhance-install-cancel]')!); });
    expect(invoke).toHaveBeenCalledWith('native_audio_enhance_install_cancel');
    await act(async () => { await vi.advanceTimersByTimeAsync(800); });
    expect(document.querySelector('[data-audio-enhance-install-status]')).toBeNull();
    expect(mode().disabled).toBe(false);
    expect(mode().querySelector<HTMLOptionElement>('[value="stems"]')!.disabled).toBe(true);
    expect(invoke).not.toHaveBeenCalledWith('native_audio_enhance_install', expect.anything());
  } finally { vi.useRealTimers(); }
});

test('terminal install polling preserves the concrete compiler failure', async () => {
  vi.useFakeTimers();
  let running = false;
  let fail!: (error: Error) => void;
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'native_audio_enhance_capabilities') return { clean: true, deep: false, stems: false };
    if (command === 'native_audio_enhance_install_status') return { running, mode: 'stems', stage: running ? 'inference-check' : 'failed', downloaded: 0, total: 0 };
    if (command === 'native_audio_enhance_install') { running = true; return await new Promise((_, reject) => { fail = reject; }); }
    return null;
  });
  try {
    await mount({ sourceId: 'retained' });
    await act(async () => { fireEvent.click(document.querySelector('[data-audio-enhance-install="stems"]')!); });
    await acceptConsent();
    await act(async () => { await vi.advanceTimersByTimeAsync(800); });
    await act(async () => { running = false; fail(new Error('AUDIO_RUNTIME: C++ compiler missing')); });
    await act(async () => { await vi.advanceTimersByTimeAsync(800); });
    expect(screen.getByRole('alert').textContent).toContain('C++ compiler missing');
    expect(mode().querySelector<HTMLOptionElement>('[value="stems"]')!.disabled).toBe(true);
  } finally { vi.useRealTimers(); }
});

test('IPC rejects malformed capabilities and malformed success receipts', async () => {
  vi.mocked(invoke).mockResolvedValue({ clean: 'yes' });
  await expect(readAudioEnhanceCapabilities()).rejects.toThrow('Invalid');
  vi.mocked(invoke).mockResolvedValue({ path: '/x', sha256: 'bad', size: 1 });
  await expect(exportEnhancedAudio({ jobId: 'owned', index: 0 }, 'clean', 'mix', 'Save')).rejects.toThrow('Invalid');
});

const device = () => document.querySelector('[data-audio-enhance-device]') as HTMLSelectElement;
test.each(['auto', 'cpu', 'cuda'] as const)('manual Stems snapshots %s and preserves native errors without fallback', async selected => {
  vi.mocked(invoke).mockImplementation(async command => {
    if (command === 'native_audio_enhance_capabilities') return { clean: true, deep: true, stems: true, cuda: true };
    if (command === 'native_audio_enhance_export') throw new Error('AUDIO_CUDA_ADMISSION: insufficient memory');
    return { running: false, mode: '', stage: '', downloaded: 0, total: 0 };
  });
  await mount({ sourceId: 'retained' });
  expect(device()).toBeNull();
  fireEvent.change(mode(), { target: { value: 'stems' } });
  expect(device().value).toBe('auto');
  fireEvent.change(device(), { target: { value: selected } });
  expect(run().disabled).toBe(false);
  expect(document.querySelector('[data-audio-enhance-device-status]')?.textContent).toContain('CUDA compatible');
  await act(async () => { fireEvent.click(run()); });
  expect(invoke).toHaveBeenCalledWith('native_audio_enhance_export', { request: { sourceId: 'retained', mode: 'stems', profile: 'mix', device: selected, confirmed: true, title: 'Enhance audio and save…' } });
  expect(screen.getByRole('alert').textContent).toContain('AUDIO_CUDA_ADMISSION');
  expect(vi.mocked(invoke).mock.calls.filter(([c]) => c === 'native_audio_enhance_export')).toHaveLength(1);
  expect(device().value).toBe(selected);
});

test('legacy capabilities block restored explicit CUDA, never downgrade, and retain CPU and Auto', async () => {
  localStorage.setItem('deas.pref.studio.audioEnhance.tve.mode', JSON.stringify('stems'));
  localStorage.setItem('deas.pref.studio.audioEnhance.tve.device', JSON.stringify('cuda'));
  vi.mocked(invoke).mockImplementation(async command => command === 'native_audio_enhance_capabilities' ? { clean: true, deep: true, stems: true } : { running: false, mode: '', stage: '', downloaded: 0, total: 0 });
  await mount({ sourceId: 'retained' });
  expect(device().value).toBe('cuda');
  expect(device().querySelector('option[value="cuda"]')).toHaveProperty('disabled', true);
  expect(run().disabled).toBe(true);
  expect(document.querySelector('[data-audio-enhance-device-status]')?.textContent).toContain('CUDA unavailable');
  fireEvent.click(run());
  expect(invoke).not.toHaveBeenCalledWith('native_audio_enhance_export', expect.anything());
  for (const value of ['cpu', 'auto']) {
    fireEvent.change(device(), { target: { value } });
    expect(run().disabled).toBe(false);
  }
  expect(searchIndex.filter(entry => entry.target.endsWith('[data-audio-enhance-device]'))).toHaveLength(2);
  expect(revealTarget('[data-audio-enhance-device]').state).toBe('revealed');
});

test('device restoration defaults legacy intents to Auto, omits non-Stems and keeps snapshots immutable', async () => {
  const { result } = renderHook(() => useAudioEnhance('post'));
  expect(result.current.device).toBe('auto');
  expect(result.current.intent).toBeUndefined();
  act(() => result.current.restore({ mode: 'stems', profile: 'mix', device: 'cuda' }));
  const held = result.current.intent;
  expect(held).toEqual({ mode: 'stems', profile: 'mix', device: 'cuda' });
  act(() => result.current.setDevice('cpu'));
  expect(held?.device).toBe('cuda');
  act(() => result.current.restore({ mode: 'stems', profile: 'mix' }));
  expect(result.current.device).toBe('auto');
  for (const mode of ['clean', 'deep'] as const) {
    act(() => result.current.restore({ mode, profile: 'speech', device: 'cuda' }));
    expect(result.current.intent).toEqual({ mode, profile: 'speech' });
  }
  act(() => result.current.restore());
  expect(result.current.intent).toBeUndefined();
  expect(result.current.device).toBe('auto');
  await act(async () => {});
});

test('mount does not install, export, upload or submit; optional CUDA is validated', async () => {
  await act(async () => { render(<I18nProvider><AudioEnhance scope="tve"/></I18nProvider>); });
  expect(vi.mocked(invoke).mock.calls.map(([command]) => command).sort()).toEqual(['native_audio_enhance_capabilities', 'native_audio_enhance_install_status']); // Mount may only read: install status plus the read-only capability probe that enables the options immediately.
  vi.mocked(invoke).mockResolvedValue({ clean: true, deep: true, stems: true });
  expect(await readAudioEnhanceCapabilities()).toEqual({ clean: true, deep: true, stems: true, cuda: false });
  vi.mocked(invoke).mockResolvedValue({ clean: true, deep: true, stems: true, cuda: 'yes' });
  await expect(readAudioEnhanceCapabilities()).rejects.toThrow('Invalid');
});

