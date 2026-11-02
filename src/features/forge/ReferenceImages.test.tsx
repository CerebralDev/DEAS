import { useState } from 'react';
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { I18nProvider } from '../../i18n/I18nProvider';
import { ReferenceImages, type RefPicture } from './ReferenceImages';
import type { NativeReferences } from '../../lib/forge';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn(() => false), convertFileSrc: (p: string) => `asset://${p}` }));
const os = vi.hoisted(() => ({ drop: undefined as undefined | ((e: { payload: unknown }) => void) }));
vi.mock('@tauri-apps/api/webview', () => ({ getCurrentWebview: () => ({ onDragDropEvent: async (h: (e: { payload: unknown }) => void) => { os.drop = h; return () => {}; } }) }));
const vocab: NativeReferences = { roles: [{ key: 'style', label: 'Style' }], fidelity: [{ key: 'loose', label: 'Loose' }], defaultRole: 'style', defaultFidelity: 'loose', max: 4, analyze: true };
const picture = (id: string): RefPicture => ({ id, name: `${id}.png`, image: `data:image/png;base64,${id}`, role: 'style', fidelity: 'loose', note: '', character: '' });
function Harness() {
  const [pictures, setPictures] = useState([picture('a'), picture('b')]);
  return <ReferenceImages root="/env" target="illustrious" model="qwen" vocab={vocab} pictures={pictures} onPictures={setPictures}/>;
}
let finish!: (text: string) => void;
beforeEach(() => {
  localStorage.clear(); localStorage.setItem('deas.locale.v1', 'en');
  vi.mocked(invoke).mockImplementation(() => new Promise(resolve => { finish = resolve as (text: string) => void; }));
});
afterEach(() => { cleanup(); vi.resetAllMocks(); });

it('does not restore a reference removed while analysis was pending', async () => {
  render(<I18nProvider><Harness/></I18nProvider>);
  fireEvent.click(screen.getAllByRole('button', { name: 'Analyze' })[0]);
  fireEvent.click(document.querySelector('[data-forge-ref-remove]')!);
  expect(screen.queryByAltText('a.png')).toBeNull();
  await act(async () => finish('Old analysis'));
  expect(screen.queryByAltText('a.png')).toBeNull();
  expect(screen.getByAltText('b.png')).toBeTruthy();
});

it('keeps edits to the analyzed reference and its neighbors when analysis returns', async () => {
  render(<I18nProvider><Harness/></I18nProvider>);
  fireEvent.click(screen.getAllByRole('button', { name: 'Analyze' })[0]);
  const notes = document.querySelectorAll('[data-forge-ref-note]');
  fireEvent.change(notes[0], { target: { value: 'new note' } });
  fireEvent.change(notes[1], { target: { value: 'neighbor edit' } });
  await act(async () => finish('Old analysis'));
  expect((notes[0] as HTMLInputElement).value).toBe('new note');
  expect((notes[1] as HTMLInputElement).value).toBe('neighbor edit');
  expect(document.querySelector('[data-forge-readback]')?.hasAttribute('open')).toBe(false);
});

describe('native file drops', () => {
  const zone = () => document.querySelector('.forge-refdrop') as HTMLElement;
  const at = (x: number, y: number, paths: string[]) => act(async () => { os.drop!({ payload: { type: 'drop', position: { x, y }, paths } }); await new Promise(r => setTimeout(r)); });
  const entry = (name: string, media: string | null = 'image') => ({ name, path: `/pics/${name}`, kind: 'file', media });
  const answer = (entries: unknown[]) => vi.mocked(invoke).mockImplementation(async cmd => cmd === 'native_media_describe' ? entries : undefined);
  const start = async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, blob: async () => new Blob(['x'], { type: 'image/png' }) })));
    render(<I18nProvider><Harness/></I18nProvider>);
    await act(async () => {});
    zone().getBoundingClientRect = () => ({ left: 10, right: 110, top: 10, bottom: 110 }) as DOMRect;
  };
  afterEach(() => vi.unstubAllGlobals());

  it('adds a picture dropped on the zone from its path', async () => {
    await start(); answer([entry('c.png')]);
    await at(50 * devicePixelRatio, 50 * devicePixelRatio, ['/pics/c.png']);
    expect(await screen.findByAltText('c.png')).toBeTruthy();
    expect(fetch).toHaveBeenCalledWith('asset:///pics/c.png');
  });

  it('ignores a drop outside the zone', async () => {
    await start(); answer([entry('c.png')]);
    await at(500, 500, ['/pics/c.png']);
    expect(invoke).not.toHaveBeenCalledWith('native_media_describe', expect.anything());
    expect(screen.queryByAltText('c.png')).toBeNull();
  });

  it('ignores a drop while another dialog holds the screen', async () => {
    await start(); answer([entry('c.png')]);
    const dialog = document.body.appendChild(document.createElement('dialog')); dialog.setAttribute('open', '');
    await at(50 * devicePixelRatio, 50 * devicePixelRatio, ['/pics/c.png']);
    dialog.remove();
    expect(screen.queryByAltText('c.png')).toBeNull();
  });

  it('says why when nothing dropped is a picture', async () => {
    await start(); answer([entry('clip.mp4', 'video')]);
    await at(50 * devicePixelRatio, 50 * devicePixelRatio, ['/pics/clip.mp4']);
    expect((await screen.findByRole('alert')).textContent).toBe('That picture could not be read.');
  });

  it('stops at the picture limit', async () => {
    await start(); answer(['c', 'd', 'e'].map(n => entry(`${n}.png`)));
    await at(50 * devicePixelRatio, 50 * devicePixelRatio, ['/pics/c.png', '/pics/d.png', '/pics/e.png']);
    expect((await screen.findByRole('alert')).textContent).toContain('At most 4');
    await waitFor(() => expect(document.querySelectorAll('.forge-refcard')).toHaveLength(4));
  });
});
