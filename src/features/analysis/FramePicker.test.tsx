import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { I18nProvider } from '../../i18n/I18nProvider';
import type { MediaAction } from '../media/MediaLibrary';
import { FramePicker } from './FramePicker';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn(() => true), convertFileSrc: (p: string) => `asset://${p}` }));

const clip = { name: 'clip.mp4', width: 832, height: 480, fps: 16, count: 49, duration: 3.0625, stride: 1, thumbs: Array.from({ length: 49 }, (_, i) => `C:/strip/${String(i + 1).padStart(6, '0')}.jpg`) };
const calls = (name: string) => vi.mocked(invoke).mock.calls.filter(([c]) => c === name).map(([, a]) => a as Record<string, any>);
const video = { path: 'E:/out/clip.mp4', name: 'clip.mp4' };

beforeEach(() => {
  localStorage.clear(); localStorage.setItem('deas.locale.v1', 'en');
  Element.prototype.scrollIntoView = vi.fn();
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    const a = args as Record<string, any>;
    if (command === 'native_analysis_frames_open') return clip;
    if (command === 'native_analysis_frame') return `C:/frames/clip-f${String(a.index + 1).padStart(4, '0')}.png`;
    if (command === 'native_media_describe') return [{ kind: 'file', name: a.paths[0].split('/').pop(), path: a.paths[0], media: 'image' }];
    if (command === 'native_analysis_frames_save') return a.indices.length;
    throw new Error(`Unexpected ${command}`);
  });
});
afterEach(() => { cleanup(); vi.resetAllMocks(); });

const ui = (props: Partial<Parameters<typeof FramePicker>[0]> = {}) =>
  render(<I18nProvider><FramePicker video={video} onUse={vi.fn()} onClose={vi.fn()} {...props}/></I18nProvider>);

it('lays out every frame, steps one at a time and shows the exact frame at full size', async () => {
  ui();
  expect(await screen.findByText('clip.mp4 · 832×480 · 49 frames · 16 fps · 0:03.06')).toBeTruthy();
  expect(document.querySelectorAll('[data-frame-cell]').length).toBe(49);
  expect(screen.getByText('Frame 1 / 49 · 0:00.00')).toBeTruthy();
  await waitFor(() => expect(document.querySelector('[data-frame-current="full"]')?.getAttribute('src')).toBe('asset://C:/frames/clip-f0001.png'));
  fireEvent.click(screen.getByTitle('Next frame (→, Shift+→ for 10)'));
  const picker = document.querySelector('[data-frame-picker]')!;
  fireEvent.keyDown(picker, { key: 'ArrowRight', shiftKey: true });
  expect(screen.getByText('Frame 12 / 49 · 0:00.69')).toBeTruthy();
  await waitFor(() => expect(calls('native_analysis_frame').at(-1)).toEqual({ path: 'E:/out/clip.mp4', index: 11, count: 49 }));
  fireEvent.keyDown(picker, { key: 'End' });
  expect(screen.getByText('Frame 49 / 49 · 0:03.00')).toBeTruthy();
  fireEvent.keyDown(picker, { key: 'ArrowRight' });
  expect(screen.getByText('Frame 49 / 49 · 0:03.00')).toBeTruthy();
});

it('picks frames by button, Space, Ctrl- and Shift-click, and saves them in order', async () => {
  ui();
  await screen.findByText(/49 frames/);
  fireEvent.click(document.querySelector('[data-frame-cell="5"]')!);
  fireEvent.click(screen.getByRole('button', { name: 'Pick' }));
  fireEvent.click(document.querySelector('[data-frame-cell="2"]')!, { ctrlKey: true });
  fireEvent.click(document.querySelector('[data-frame-cell="8"]')!, { shiftKey: true });
  expect(screen.getByRole('button', { name: 'Save picked (7)…' })).toBeTruthy();
  fireEvent.keyDown(document.querySelector('[data-frame-picker]')!, { key: ' ' });
  expect(screen.getByRole('button', { name: 'Save picked (6)…' })).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Save picked (6)…' }));
  await waitFor(() => expect(calls('native_analysis_frames_save')[0]).toMatchObject({ path: 'E:/out/clip.mp4', indices: [2, 3, 4, 6, 7, 8], count: 49 }));
  expect(await screen.findByText('Saved 6 frames as PNG.')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Clear picks' }));
  expect((screen.getByRole('button', { name: 'Save picked (0)…' }) as HTMLButtonElement).disabled).toBe(true);
});

it('uses a frame as the picture to analyse', async () => {
  const onUse = vi.fn();
  ui({ onUse });
  await screen.findByText(/49 frames/);
  fireEvent.click(document.querySelector('[data-frame-cell="20"]')!);
  fireEvent.click(screen.getByRole('button', { name: 'Use this frame' }));
  await waitFor(() => expect(onUse).toHaveBeenCalledWith({ name: 'clip-f0021.png', path: 'C:/frames/clip-f0021.png', media: 'image', src: 'asset://C:/frames/clip-f0021.png' }));
});

it('sends a frame with whatever Generate offers, refusing what Generate blocks', async () => {
  const first = vi.fn(), last = vi.fn();
  const frameActions = (item: { path?: string }): MediaAction[] => [
    { label: 'Use as first frame', onClick: () => first(item.path), input: true },
    { label: 'Use as last frame', onClick: () => last(item.path), blocked: item.path?.endsWith('.frame-1.png') ? undefined : undefined },
    { label: 'Add as reference', onClick: vi.fn(), blocked: 'Up to 4 references' },
  ];
  ui({ frameActions });
  await screen.findByText(/49 frames/);
  fireEvent.click(document.querySelector('[data-frame-cell="3"]')!);
  fireEvent.click(screen.getByRole('button', { name: 'Use as first frame' }));
  await waitFor(() => expect(first).toHaveBeenCalledWith('C:/frames/clip-f0004.png'));
  expect(await screen.findByText('Frame 4: Use as first frame. It is set in the Generate tab.')).toBeTruthy();
  expect((screen.getByRole('button', { name: 'Add as reference' }) as HTMLButtonElement).disabled).toBe(true);
  cleanup();
  ui({ frameActions: () => [] });
  expect(await screen.findByText(/choose that model in the Generate tab first/)).toBeTruthy();
});

it('says plainly when a video cannot be read', async () => {
  vi.mocked(invoke).mockImplementation(async () => { throw 'ANALYSIS_FRAMES: The frame picker reads MP4, MOV, MKV and WebM videos.'; });
  const onClose = vi.fn();
  ui({ onClose });
  expect((await screen.findByRole('alert')).textContent).toBe('The frame picker reads MP4, MOV, MKV and WebM videos.');
  fireEvent.click(screen.getByRole('button', { name: 'Close' }));
  expect(onClose).toHaveBeenCalled();
});
