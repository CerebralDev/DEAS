import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { I18nProvider, LOCALE_STORAGE_KEY } from '../../i18n/I18nProvider';
import { TrueVideoEnhancer } from './TrueVideoEnhancer';
import type { VideoPostInspection } from '../../lib/videoPost';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn(() => true), convertFileSrc: (p: string) => `asset://${p}` }));
vi.mock('../../lib/media', async importOriginal => ({ ...await importOriginal<typeof import('../../lib/media')>(), listMedia: vi.fn(async () => ({ path: '/out', parent: null, entries: [{ name: 'clip.mp4', path: '/out/clip.mp4', kind: 'file', symlink: false, media: 'video', size: 1, modified: 1 }], truncated: false })) }));
beforeEach(() => { localStorage.setItem(LOCALE_STORAGE_KEY, 'en'); vi.mocked(isTauri).mockReturnValue(true); });
afterEach(() => { cleanup(); localStorage.clear(); vi.resetAllMocks(); });
const source: VideoPostInspection = { sourceId: '6f1c2b3a-4d5e-4f60-8a9b-0c1d2e3f4a5b', name: 'clip.mp4', width: 1280, height: 720, fpsNumerator: 24, fpsDenominator: 1, durationSeconds: 3, pixelFormat: 'yuv420p', fieldOrder: 'progressive', colorTransfer: 'bt709', audioStreams: 1, subtitleStreams: 0, blockers: [], timingVerified: false, processingReady: false };

test('mount only reads audio installation recovery; chooser inspection feeds the backend panel without execution', async () => {
  vi.mocked(invoke).mockImplementation(async command => command === 'native_audio_enhance_install_status' ? { running: false, mode: '', stage: '', downloaded: 0, total: 0 } : source);
  render(<I18nProvider><TrueVideoEnhancer endpoint="http://127.0.0.1:7801/"/></I18nProvider>);
  expect(vi.mocked(invoke).mock.calls.map(([command]) => command).sort()).toEqual(['native_audio_enhance_capabilities', 'native_audio_enhance_install_status']); // Mount may only read: install status plus the read-only capability probe.
  vi.mocked(invoke).mockClear();
  expect(document.querySelector('[data-tve-scale]')).toBeNull();
  expect(document.querySelector('[data-tve-bind]')).toBeNull();
  expect(document.querySelector('[data-tve-standalone]')).toBeNull();
  expect(document.querySelector('[data-tve-enhance]')).toHaveProperty('disabled', true);
  await act(async () => { fireEvent.click(document.querySelector('[data-video-post-inspect]')!); });
  expect(invoke).toHaveBeenCalledExactlyOnceWith('native_video_post_inspect', { title: 'Choose video source' });
  expect(screen.getByText('Enable at least one TVE stage.')).toBeTruthy();
  fireEvent.click(document.querySelector('[data-tve-post="denoise"]')!);
  expect(document.querySelector('[data-tve-enhance]')).toHaveProperty('disabled', false);
});

test('without media roots there is no Media panel; with roots a video tile is inspected by path', async () => {
  vi.mocked(invoke).mockImplementation(async command => command === 'native_audio_enhance_install_status' ? { running: false, mode: '', stage: '', downloaded: 0, total: 0 } : source);
  const { unmount } = render(<I18nProvider><TrueVideoEnhancer endpoint=""/></I18nProvider>);
  expect(document.querySelector('[data-tve-media]')).toBeNull();
  unmount();
  render(<I18nProvider><TrueVideoEnhancer endpoint="" roots={{ output: '/out' }}/></I18nProvider>);
  expect(document.querySelector('[data-tve-media]')).toBeTruthy();
  const tile = await screen.findByRole('button', { name: 'clip.mp4' });
  await act(async () => { fireEvent.click(tile); });
  expect(invoke).toHaveBeenCalledWith('native_video_post_inspect_path', { path: '/out/clip.mp4' });
  expect(document.querySelector('[data-video-post-source]')?.textContent).toContain('clip.mp4');
});
