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
const vocab: NativeReferences = { roles: [{ key: 'style', label: 'Style' }, { key: 'character', label: 'Character' }], fidelity: [{ key: 'loose', label: 'Loose' }, { key: 'strict', label: 'Strict' }], defaultRole: 'style', defaultFidelity: 'loose', max: 4, analyze: true };
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

describe('clipboard pictures', () => {
  const png = (name = 'clipboard.png') => new File(['pixels'], name, { type: 'image/png' });
  const paste = (target: Element, files: File[] = [], text = '') => fireEvent.paste(target, {
    clipboardData: { files, getData: () => text },
  });
  const editor = () => render(<I18nProvider><div className="forge-builder"><textarea aria-label="Brief"/><Harness/></div></I18nProvider>);
  afterEach(() => vi.unstubAllGlobals());

  it('adds clipboard images from the Forge brief using the selected role and fidelity', async () => {
    editor();
    fireEvent.change(document.querySelector('[data-forge-ref-select="next-role"]')!, { target: { value: 'character' } });
    fireEvent.change(document.querySelector('[data-forge-ref-select="next-fidelity"]')!, { target: { value: 'strict' } });
    expect(paste(screen.getByRole('textbox', { name: 'Brief' }), [png(), new File(['text'], 'note.txt', { type: 'text/plain' })])).toBe(false);
    const image = await screen.findByAltText('clipboard.png');
    expect(image.getAttribute('src')).toBe('data:image/png;base64,cGl4ZWxz');
    const card = image.closest('.forge-refcard')!;
    expect((card.querySelector('[data-forge-ref-select="role-3"]') as HTMLSelectElement).value).toBe('character');
    expect((card.querySelector('[data-forge-ref-select="fidelity-3"]') as HTMLSelectElement).value).toBe('strict');
    expect(invoke).not.toHaveBeenCalled();
  });

  it('leaves ordinary text and non-image files to the default paste behavior', () => {
    editor();
    expect(paste(screen.getByRole('textbox', { name: 'Brief' }), [], 'a city')).toBe(true);
    expect(paste(document.querySelector('[data-forge-ref-note]')!, [new File(['text'], 'note.txt', { type: 'text/plain' })], 'a note')).toBe(true);
    expect(document.querySelectorAll('.forge-refcard')).toHaveLength(2);
  });

  it('does not handle paste outside its editor or in a hidden retained editor', () => {
    const view = editor();
    expect(paste(document.body, [png()])).toBe(true);
    view.container.hidden = true;
    expect(paste(screen.getByLabelText('Brief'), [png()])).toBe(true);
    expect(document.querySelectorAll('.forge-refcard')).toHaveLength(2);
  });

  it('handles an image only in the innermost dialog, even when the outer listener mounts first', async () => {
    render(<I18nProvider><div role="dialog" aria-label="Outer"><Harness/>
      <dialog open aria-label="Inner"><textarea aria-label="Inner brief"/><Harness/></dialog>
    </div></I18nProvider>);
    expect(paste(screen.getByRole('textbox', { name: 'Inner brief' }), [png()])).toBe(false);
    await screen.findByAltText('clipboard.png');
    expect(screen.getAllByAltText('clipboard.png')).toHaveLength(1);
    expect(screen.getByAltText('clipboard.png').closest('dialog')?.getAttribute('aria-label')).toBe('Inner');
  });

  it('ignores paste in an unrelated nested dialog without references', () => {
    render(<I18nProvider><div className="forge-builder"><Harness/>
      <div role="dialog"><textarea aria-label="Other dialog"/></div>
    </div></I18nProvider>);
    expect(paste(screen.getByRole('textbox', { name: 'Other dialog' }), [png()])).toBe(true);
    expect(document.querySelectorAll('.forge-refcard')).toHaveLength(2);
  });

  it('respects an already handled paste and removes its listener on unmount', () => {
    const view = render(<I18nProvider><div className="forge-builder"><textarea aria-label="Brief" onPaste={e => e.preventDefault()}/><Harness/></div></I18nProvider>);
    const brief = screen.getByRole('textbox', { name: 'Brief' });
    expect(paste(brief, [png()])).toBe(false);
    expect(document.querySelectorAll('.forge-refcard')).toHaveLength(2);
    view.rerender(<I18nProvider><div className="forge-builder"><textarea aria-label="Brief"/></div></I18nProvider>);
    expect(paste(screen.getByRole('textbox', { name: 'Brief' }), [png()])).toBe(true);
  });

  it('keeps the existing picture cap and reports overflow even after the drop zone disappears', async () => {
    editor();
    paste(screen.getByRole('textbox', { name: 'Brief' }), [png('c.png'), png('d.png'), png('e.png')]);
    expect((await screen.findByRole('alert')).textContent).toContain('At most 4');
    await waitFor(() => expect(document.querySelectorAll('.forge-refcard')).toHaveLength(4));
    expect(document.querySelector('.forge-refdrop')).toBeNull();
    expect(paste(screen.getByRole('textbox', { name: 'Brief' }), [png('f.png')])).toBe(false);
    expect(screen.getByRole('alert').textContent).toContain('At most 4');
    expect(document.querySelectorAll('.forge-refcard')).toHaveLength(4);
  });

  it('uses the existing unreadable error and preserves current pictures on decode failure', async () => {
    vi.stubGlobal('createImageBitmap', vi.fn().mockRejectedValue(new Error('decode failed')));
    editor();
    paste(screen.getByRole('textbox', { name: 'Brief' }), [new File(['bad'], 'bad.gif', { type: 'image/gif' })]);
    expect((await screen.findByRole('alert')).textContent).toBe('That picture could not be read.');
    expect(document.querySelectorAll('.forge-refcard')).toHaveLength(2);
  });
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
