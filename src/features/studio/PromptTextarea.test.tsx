import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { useState } from 'react';
import { PromptTextarea } from './PromptTextarea';
import { tagSearch } from '../../lib/forge';
import { spellcheckSuggest, spellcheckWords } from '../../lib/promptTools';
import { resetSpellcheckCache, tokenize } from '../../lib/spellcheck';

const tools = vi.hoisted(() => ({ spellcheck: true, language: 'de-DE', autocomplete: false }));
const native = vi.hoisted(() => ({ on: false }));
vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => native.on, invoke: vi.fn() }));
vi.mock('../../lib/promptTools', () => ({ usePromptTools: () => tools, DICTIONARY_EVENT: 'deas:spellcheck-dictionary', spellcheckWords: vi.fn(), spellcheckSuggest: vi.fn() }));
vi.mock('../../i18n/I18nProvider', () => ({ useI18n: () => ({ t: (s: string) => `translated:${s}` }) }));
vi.mock('../../lib/forge', () => ({ tagSearch: vi.fn(), forgeError: (e: unknown) => String(e).includes('404') ? 'forge.outdated' : String(e) }));
const hits = [{ tag: 'blue_eyes', category: 'general', count: 12 }, { tag: 'blue_hair', category: 'general', count: 8 }];
function Editor(props: React.ComponentProps<typeof PromptTextarea>) {
  const [value, setValue] = useState(String(props.defaultValue ?? ''));
  return <PromptTextarea root="/env" target="anima" aria-label="Prompt" {...props} defaultValue={undefined} value={value} onChange={e => setValue(e.target.value)} />;
}
const textarea = () => screen.getByRole('textbox') as HTMLTextAreaElement;
function select(value: string, caret = value.length) {
  act(() => textarea().focus());
  fireEvent.change(textarea(), { target: { value, selectionStart: caret, selectionEnd: caret } });
  fireEvent.select(textarea(), { target: { selectionStart: caret, selectionEnd: caret } });
}
async function debounce() { await act(async () => { vi.advanceTimersByTime(150); }); }
beforeEach(() => { vi.useFakeTimers(); native.on = false; (globalThis as { ResizeObserver?: unknown }).ResizeObserver ??= class { observe() {} disconnect() {} unobserve() {} }; resetSpellcheckCache(); vi.mocked(spellcheckWords).mockResolvedValue([]); vi.mocked(spellcheckSuggest).mockResolvedValue([]); tools.autocomplete = false; tools.spellcheck = true; tools.language = 'de-DE'; vi.mocked(tagSearch).mockResolvedValue(hits); });
afterEach(() => { cleanup(); vi.useRealTimers(); vi.clearAllMocks(); });

it('mounts the enlarged editor autofocus before the ref is assigned without throwing', () => {
  render(<Editor autoFocus />);
  expect(document.activeElement).toBe(textarea());
});

it('debounces the caret prefix and replaces its whole token without losing the remaining prompt', async () => {
  tools.autocomplete = true;
  render(<Editor />);
  select('rain,  bluZZ  , sky\ncloud', 10);
  await act(async () => { vi.advanceTimersByTime(149); });
  expect(tagSearch).not.toHaveBeenCalled();
  await act(async () => { vi.advanceTimersByTime(1); });
  expect(tagSearch).toHaveBeenCalledWith('/env', 'anima', 'blu');
  const list = screen.getByRole('listbox');
  expect(textarea().getAttribute('aria-controls')).toBe(list.id);
  fireEvent.mouseDown(screen.getByRole('option', { name: /blue_eyes/ }));
  fireEvent.mouseUp(textarea());
  expect(textarea().value).toBe('rain,  blue_eyes  , sky\ncloud');
  expect(textarea().selectionStart).toBe(16);
  expect(document.activeElement).toBe(textarea());
  expect(screen.queryByRole('listbox')).toBeNull();
});

it.each(['Enter', 'Tab'])('navigates options and accepts with %s', async key => {
  tools.autocomplete = true;
  render(<Editor />); select('blue'); await debounce();
  fireEvent.keyDown(textarea(), { key: 'ArrowDown' });
  expect(screen.getAllByRole('option')[1].getAttribute('aria-selected')).toBe('true');
  fireEvent.keyDown(textarea(), { key: 'ArrowUp' });
  expect(screen.getAllByRole('option')[0].id).toBe(textarea().getAttribute('aria-activedescendant'));
  fireEvent.keyDown(textarea(), { key });
  expect(textarea().value).toBe('blue_eyes');
});

it('keeps backward Tab navigation and scrolls the active keyboard option into view', async () => {
  tools.autocomplete = true;
  const scroll = vi.fn();
  const previous = HTMLElement.prototype.scrollIntoView;
  HTMLElement.prototype.scrollIntoView = scroll;
  try {
    render(<Editor />); select('blue'); await debounce();
    scroll.mockClear();
    fireEvent.keyDown(textarea(), { key: 'ArrowDown' });
    expect(scroll).toHaveBeenCalledWith({ block: 'nearest' });
    expect(fireEvent.keyDown(textarea(), { key: 'Tab', shiftKey: true })).toBe(true);
    expect(textarea().value).toBe('blue');
  } finally { HTMLElement.prototype.scrollIntoView = previous; }
});

it('consumes Escape without closing the containing dialog or reopening suggestions', async () => {
  tools.autocomplete = true;
  const outer = vi.fn();
  render(<div onKeyDown={outer}><Editor /></div>); select('blue'); await debounce();
  fireEvent.keyDown(textarea(), { key: 'Escape' });
  expect(outer).not.toHaveBeenCalled();
  expect(screen.queryByRole('listbox')).toBeNull();
  await debounce(); expect(tagSearch).toHaveBeenCalledTimes(1);
});

it.each([{ root: undefined }, { target: undefined }, { disabled: true }, { readOnly: true }])('does not query unavailable or noneditable inputs: %j', async props => {
  tools.autocomplete = true;
  render(<Editor {...props} />); select('blue'); await debounce();
  expect(tagSearch).not.toHaveBeenCalled();
});

it('leaves Ctrl+Enter to Studio while suggestions are open', async () => {
  tools.autocomplete = true;
  render(<Editor />);
  select('rain,  bluZZ  , sky\ncloud', 10);
  await act(async () => { vi.advanceTimersByTime(150); });
  expect(screen.getByRole('listbox')).toBeTruthy();
  fireEvent.keyDown(textarea(), { key: 'Enter', ctrlKey: true });
  expect(textarea().value).toBe('rain,  bluZZ  , sky\ncloud');
  expect(screen.getByRole('listbox')).toBeTruthy();
});
it('requires two characters and a collapsed selection', async () => {
  tools.autocomplete = true;
  render(<Editor />); select('b'); await debounce();
  expect(tagSearch).not.toHaveBeenCalled();
  select('blue'); textarea().setSelectionRange(1, 3); fireEvent.select(textarea());
  expect(textarea().selectionStart).toBe(1); expect(textarea().selectionEnd).toBe(3); await debounce();
  expect(tagSearch).not.toHaveBeenCalled();
});

it('rejects replacements exceeding maxLength without truncating the prompt', async () => {
  tools.autocomplete = true;
  render(<Editor maxLength={10} />); select('bl, sky');
  fireEvent.select(textarea(), { target: { selectionStart: 2, selectionEnd: 2 } }); await debounce();
  expect(screen.queryByRole('listbox')).toBeNull();
  expect(fireEvent.keyDown(textarea(), { key: 'Enter' })).toBe(true);
  expect(fireEvent.keyDown(textarea(), { key: 'Tab' })).toBe(true);
  expect(textarea().value).toBe('bl, sky');
});

it('accepts an exact maxLength match in an uncontrolled textarea', async () => {
  tools.autocomplete = true;
  render(<PromptTextarea root="/env" target="anima" maxLength={9} defaultValue="bl" />);
  act(() => textarea().focus()); textarea().setSelectionRange(2, 2); fireEvent.select(textarea()); await debounce();
  expect(tagSearch).toHaveBeenCalledWith('/env', 'anima', 'bl');
  expect(screen.getAllByRole('option')).toHaveLength(2);
  fireEvent.keyDown(textarea(), { key: 'Tab' });
  expect(textarea().value).toBe('blue_eyes');
});

it('suppresses requests and acceptance while IME is composing', async () => {
  tools.autocomplete = true;
  render(<Editor />);
  fireEvent.focus(textarea()); fireEvent.compositionStart(textarea()); select('blue'); await debounce();
  expect(tagSearch).not.toHaveBeenCalled();
  fireEvent.compositionEnd(textarea()); await debounce();
  expect(tagSearch).toHaveBeenCalledTimes(1);
  fireEvent.keyDown(textarea(), { key: 'Enter', isComposing: true });
  expect(textarea().value).toBe('blue');
});

it.each(['blur', 'selection', 'value', 'root', 'target', 'settings', 'language', 'unmount'])('discards stale responses after %s changes', async change => {
  tools.autocomplete = true;
  let resolve!: (value: typeof hits) => void;
  vi.mocked(tagSearch).mockImplementationOnce(() => new Promise(r => { resolve = r; }));
  const view = render(<Editor />); select('blue'); await debounce();
  if (change === 'blur') fireEvent.blur(textarea());
  if (change === 'selection') fireEvent.select(textarea(), { target: { selectionStart: 2, selectionEnd: 2 } });
  if (change === 'value') select('green');
  if (change === 'root') view.rerender(<Editor root="/other" />);
  if (change === 'target') view.rerender(<Editor target="illustrious" />);
  if (change === 'settings') { tools.autocomplete = false; view.rerender(<Editor />); }
  if (change === 'language') { tools.language = 'fr'; view.rerender(<Editor />); }
  if (change === 'unmount') view.unmount();
  await act(async () => { resolve(hits); });
  expect(screen.queryByRole('listbox')).toBeNull();
});

it('limits the rendered results and translates inline Forge errors', async () => {
  tools.autocomplete = true;
  vi.mocked(tagSearch).mockResolvedValueOnce(Array.from({ length: 20 }, (_, i) => ({ ...hits[0], tag: `blue_${i}` })));
  render(<Editor />); select('blue'); await debounce();
  expect(screen.getAllByRole('option')).toHaveLength(8);
  vi.mocked(tagSearch).mockRejectedValueOnce('404 Not Found');
  select('green'); await debounce();
  expect(screen.getByRole('alert').textContent).toBe('translated:forge.outdated');
  expect(screen.queryByRole('listbox')).toBeNull();
});

it('keeps newer suggestions when an older response arrives last', async () => {
  tools.autocomplete = true;
  let resolve!: (value: typeof hits) => void;
  vi.mocked(tagSearch).mockImplementationOnce(() => new Promise(r => { resolve = r; }));
  render(<Editor />); select('blue'); await debounce();
  select('rain\ngreSuffix\ncloud', 8); await debounce();
  expect(tagSearch).toHaveBeenLastCalledWith('/env', 'anima', 'gre');
  await act(async () => { resolve([{ ...hits[0], tag: 'obsolete' }]); });
  expect(screen.queryByText('obsolete')).toBeNull();
  fireEvent.keyDown(textarea(), { key: 'Enter' });
  expect(textarea().value).toBe('rain\nblue_eyes\ncloud');
});

it('discards stale errors and pending timers after opt-out', async () => {
  tools.autocomplete = true;
  let reject!: (error: unknown) => void;
  vi.mocked(tagSearch).mockImplementationOnce(() => new Promise((_, r) => { reject = r; }));
  const view = render(<Editor />); select('blue'); await debounce();
  tools.autocomplete = false; view.rerender(<Editor />);
  await act(async () => { reject('404 Not Found'); });
  expect(screen.queryByRole('alert')).toBeNull();
  tools.autocomplete = true; view.rerender(<Editor />);
  tools.autocomplete = false; view.rerender(<Editor />); await debounce();
  expect(tagSearch).toHaveBeenCalledTimes(1);
});

it('forwards native textarea props and independent spelling settings without opt-in requests', async () => {
  render(<Editor data-prompt="positive" rows={4} />);
  select('blue'); await debounce();
  expect(tagSearch).not.toHaveBeenCalled();
  expect(textarea().getAttribute('spellcheck')).toBe('true'); // browser mode keeps the browser's checker
  expect(textarea().lang).toBe('de-DE');
  expect(textarea().dataset.prompt).toBe('positive');
  expect(textarea().rows).toBe(4);
  expect(spellcheckWords).not.toHaveBeenCalled();
});

it('tokenizes words only: tags, digits, acronyms and single letters are never checked', () => {
  expect(tokenize('1girl, long_hair, masterpiece, NSFW, a, Straßenbahn-Haltestelle, aujourd’hui').map(m => m.word))
    .toEqual(['girl', 'long', 'hair', 'masterpiece', 'Straßenbahn-Haltestelle', 'aujourd’hui']);
});

it('in the app, underlines misspelled words from the in-app dictionary and keeps the browser checker off', async () => {
  native.on = true;
  vi.mocked(spellcheckWords).mockResolvedValue(['wunderschoen']);
  render(<Editor />);
  expect(textarea().getAttribute('spellcheck')).toBe('false');
  select('ein wunderschoen Haus'); await act(async () => { vi.advanceTimersByTime(300); });
  expect(spellcheckWords).toHaveBeenCalledWith('de-DE', ['ein', 'wunderschoen', 'Haus']);
  const marks = document.querySelectorAll('.prompt-mirror mark');
  expect([...marks].map(m => m.textContent)).toEqual(['wunderschoen']);
  // Cached verdicts: retyping the same words asks nothing new.
  select('ein wunderschoen Haus Haus'); await act(async () => { vi.advanceTimersByTime(300); });
  expect(spellcheckWords).toHaveBeenCalledTimes(1);
});

it('right-click or Ctrl+. on an underlined word offers suggestions and replaces only that word', async () => {
  native.on = true;
  vi.mocked(spellcheckWords).mockResolvedValue(['wunderschoen']);
  vi.mocked(spellcheckSuggest).mockResolvedValue(['wunderschön', 'wunderschon']);
  render(<Editor />);
  select('ein wunderschoen Haus', 8); await act(async () => { vi.advanceTimersByTime(300); });
  fireEvent.keyDown(textarea(), { key: '.', ctrlKey: true });
  await act(async () => {});
  expect(spellcheckSuggest).toHaveBeenCalledWith('de-DE', 'wunderschoen');
  const options = screen.getAllByRole('option');
  expect(options.map(o => o.textContent)).toEqual(['wunderschön', 'wunderschon']);
  fireEvent.keyDown(textarea(), { key: 'ArrowDown' });
  fireEvent.keyDown(textarea(), { key: 'ArrowUp' });
  fireEvent.keyDown(textarea(), { key: 'Enter' });
  expect(textarea().value).toBe('ein wunderschön Haus');
  expect(screen.queryByRole('option')).toBeNull();
  // Right-click outside a misspelling keeps the browser's own context menu.
  select('ein wunderschön Haus', 1);
  const event = fireEvent.contextMenu(textarea());
  expect(event).toBe(true);
});

it('sends a picked tag through the editing command so Ctrl+Z can take it back', async () => {
  tools.autocomplete = true;
  const insert = vi.fn((_c: string, _u: boolean, text: string) => { const el = textarea(); el.setRangeText(text); fireEvent.input(el); return true; });
  Object.defineProperty(document, 'execCommand', { configurable: true, value: insert });
  try {
    render(<Editor />);
    select('rain,  bluZZ  , sky\ncloud', 10);
    await act(async () => { vi.advanceTimersByTime(150); });
    fireEvent.mouseDown(screen.getByRole('option', { name: /blue_eyes/ }));
    expect(insert).toHaveBeenCalledWith('insertText', false, expect.any(String));
    expect(textarea().value).toBe('rain,  blue_eyes  , sky\ncloud');
  } finally { Reflect.deleteProperty(document, 'execCommand'); }
});
it('a missing dictionary leaves the text unmarked instead of failing', async () => {
  native.on = true;
  vi.mocked(spellcheckWords).mockRejectedValue('SPELLCHECK_DICTIONARY_MISSING');
  render(<Editor />);
  select('ein Haus'); await act(async () => { vi.advanceTimersByTime(300); });
  expect(document.querySelector('.prompt-mirror')).toBeNull();
});
