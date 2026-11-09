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
    if (command === 'native_media_copy_in') return a.path.replace('C:/frames/', 'E:/input/');
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
  await waitFor(() => expect(onUse).toHaveBeenCalledWith({ name: 'clip-f0021.png', path: 'C:/frames/clip-f0021.png', media: 'image', src: 'asset://C:/frames/clip-f0021.png' }, 20));
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
  // Generate gets a copy in Input, not the pruned frame cache.
  await waitFor(() => expect(first).toHaveBeenCalledWith('E:/input/clip-f0004.png'));
  expect(calls('native_media_copy_in')).toEqual([{ path: 'C:/frames/clip-f0004.png' }]);
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

it('sends every picked frame through a collecting send, in order, and says which it skipped and why', async () => {
  // A small Generate: references collect up to 3; one already there turns into its Remove.
  const refs = new Set<string>(['C:/frames/clip-f0003.png']);
  const frameActions = (item: { path?: string }): MediaAction[] => [
    { label: 'Use as first frame', onClick: vi.fn(), input: true },
    refs.has(item.path!) ? { label: 'Remove reference', onClick: () => refs.delete(item.path!) }
      : { label: 'Add as reference', many: true, onClick: () => refs.add(item.path!), blocked: refs.size >= 3 ? 'References are full.' : undefined },
  ];
  ui({ frameActions });
  await screen.findByText(/49 frames/);
  for (const i of [9, 2, 5, 7]) fireEvent.click(document.querySelector(`[data-frame-cell="${i}"]`)!, { ctrlKey: true });
  expect(screen.getByRole('button', { name: 'Use as first frame' })).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Add as reference (4)' }));
  expect(await screen.findByText('Sent 2 frames (6, 8): Add as reference. They are set in the Generate tab. Not sent: frame 3 already there · frame 10: References are full.')).toBeTruthy();
  expect([...refs]).toEqual(['C:/frames/clip-f0003.png', 'E:/input/clip-f0006.png', 'E:/input/clip-f0008.png']);
  expect((screen.getByRole('button', { name: 'Add as reference (4)' }) as HTMLButtonElement).disabled).toBe(true);
  // One taken out in Generate: the same picks again, the skipped ones grouped by why they did not go.
  refs.delete('E:/input/clip-f0008.png');
  fireEvent.click(document.querySelector('[data-frame-cell="20"]')!);
  fireEvent.click(screen.getByRole('button', { name: 'Add as reference (4)' }));
  expect(await screen.findByText('Frame 8: Add as reference. It is set in the Generate tab. Not sent: frames 3, 6 already there · frame 10: References are full.')).toBeTruthy();
  // A frame sent again is the same copy: nothing new in Input.
  expect(calls('native_media_copy_in').map(a => a.path)).toEqual(['C:/frames/clip-f0006.png', 'C:/frames/clip-f0008.png']);
});

it('plays the clip with the strip following, and pausing stops on the frame on screen with the exact frame', async () => {
  const played: number[] = [];
  const time = new WeakMap<HTMLMediaElement, number>();
  const proto = HTMLMediaElement.prototype;
  const saved = { play: proto.play, pause: proto.pause, time: Object.getOwnPropertyDescriptor(proto, 'currentTime') };
  proto.play = vi.fn(async function (this: HTMLMediaElement) { played.push(time.get(this) ?? 0); });
  proto.pause = vi.fn();
  Object.defineProperty(proto, 'currentTime', { configurable: true, get(this: HTMLMediaElement) { return time.get(this) ?? 0; }, set(this: HTMLMediaElement, v: number) { time.set(this, v); } });
  try {
    ui();
    await screen.findByText(/49 frames/);
    fireEvent.click(document.querySelector('[data-frame-cell="4"]')!);
    fireEvent.click(screen.getByRole('button', { name: '▶ Play' }));
    // From the frame on screen: frame 5 at 16 fps, a quarter frame in.
    await waitFor(() => expect(played).toEqual([4.25 / 16]));
    expect((document.querySelector('[data-frame-video]') as HTMLVideoElement).hidden).toBe(false);
    const video = document.querySelector('[data-frame-video]') as HTMLVideoElement;
    video.currentTime = 30 / 16;
    await waitFor(() => expect(screen.getByText('Frame 31 / 49 · 0:01.88')).toBeTruthy());
    const before = calls('native_analysis_frame').length;
    fireEvent.click(screen.getByRole('button', { name: '❚❚ Pause' }));
    expect(proto.pause).toHaveBeenCalled();
    await waitFor(() => expect(calls('native_analysis_frame').slice(before).at(-1)).toEqual({ path: 'E:/out/clip.mp4', index: 30, count: 49 }));
    expect((document.querySelector('[data-frame-video]') as HTMLVideoElement).hidden).toBe(true);
    // A clip the player cannot read says so and stays steppable.
    fireEvent.error(video);
    expect(await screen.findByText(/cannot be played here/)).toBeTruthy();
    expect((screen.getByRole('button', { name: '▶ Play' }) as HTMLButtonElement).disabled).toBe(true);
  } finally {
    proto.play = saved.play; proto.pause = saved.pause;
    if (saved.time) Object.defineProperty(proto, 'currentTime', saved.time);
  }
});

it('steps with the arrow keys wherever focus is, but not while typing', async () => {
  ui();
  await screen.findByText(/49 frames/);
  fireEvent.keyDown(document.body, { key: 'ArrowRight' });
  fireEvent.keyDown(document.body, { key: 'ArrowRight', shiftKey: true });
  expect(screen.getByText('Frame 12 / 49 · 0:00.69')).toBeTruthy();
  const box = document.body.appendChild(document.createElement('input'));
  fireEvent.keyDown(box, { key: 'ArrowLeft' });
  // Space outside the picker presses whatever has focus there; it does not pick.
  fireEvent.keyDown(document.body, { key: ' ' });
  expect(screen.getByText('Frame 12 / 49 · 0:00.69')).toBeTruthy();
  expect(document.querySelector('[data-frame-cell="11"]')!.getAttribute('aria-selected')).toBe('false');
  box.remove();
});

it('plays through libmpv on Linux from the frame on screen, and pausing lands on the frame it showed', async () => {
  const ua = Object.getOwnPropertyDescriptor(navigator, 'userAgent');
  Object.defineProperty(navigator, 'userAgent', { configurable: true, get: () => 'Mozilla/5.0 (X11; Linux x86_64)' });
  let status = { position: 0, duration: 3.0625, width: 832, height: 480, paused: true, muted: false, volume: 100 };
  const base = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    const a = args as Record<string, any>;
    if (command === 'native_video_open') return null;
    if (command === 'native_video_update') {
      if (a.action.kind === 'status') return { ...status };
      if (a.action.kind === 'pause' && !a.action.paused) status = { ...status, paused: false, position: 30 / 16 };
      return null;
    }
    return base(command, args);
  });
  try {
    ui();
    await screen.findByText(/49 frames/);
    expect(document.querySelector('[data-frame-video]')).toBeNull();
    fireEvent.click(document.querySelector('[data-frame-cell="4"]')!);
    fireEvent.click(screen.getByRole('button', { name: '▶ Play' }));
    await waitFor(() => expect(calls('native_video_open')[0]).toMatchObject({ source: { path: 'E:/out/clip.mp4' } }));
    // Opened paused: seek to frame 5, a quarter frame in, then play.
    await waitFor(() => expect(calls('native_video_update').map(a => a.action).filter(x => x.kind === 'seek' || x.kind === 'pause')).toEqual([{ kind: 'seek', seconds: 4.25 / 16 }, { kind: 'pause', paused: false }]));
    await waitFor(() => expect(screen.getByText('Frame 31 / 49 · 0:01.88')).toBeTruthy(), { timeout: 2000 });
    const before = calls('native_analysis_frame').length;
    fireEvent.click(screen.getByRole('button', { name: '❚❚ Pause' }));
    await waitFor(() => expect(calls('native_analysis_frame').slice(before).at(-1)).toEqual({ path: 'E:/out/clip.mp4', index: 30, count: 49 }));
    expect(document.querySelector('[data-native-video]')).toBeNull();
  } finally {
    if (ua) Object.defineProperty(navigator, 'userAgent', ua); else delete (navigator as { userAgent?: string }).userAgent;
  }
});
