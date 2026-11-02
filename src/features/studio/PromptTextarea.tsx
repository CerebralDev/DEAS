import { forwardRef, useEffect, useId, useLayoutEffect, useRef, useState, type TextareaHTMLAttributes } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { useI18n } from '../../i18n/I18nProvider';
import { forgeError, tagSearch, type TagSuggestion } from '../../lib/forge';
import { spellcheckSuggest, usePromptTools } from '../../lib/promptTools';
import { useSpellcheck, type SpellMark } from '../../lib/spellcheck';
import './promptTextarea.css';

export interface PromptTextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement> { root?: string; target?: string }

function replacement(el: HTMLTextAreaElement, tag: string) {
  const start = Math.max(el.value.lastIndexOf(',', el.selectionStart - 1), el.value.lastIndexOf('\n', el.selectionStart - 1)) + 1;
  const tail = el.value.slice(el.selectionStart).search(/[,\n]/);
  const end = tail < 0 ? el.value.length : el.selectionStart + tail;
  const token = el.value.slice(start, end);
  const leading = token.match(/^\s*/)![0];
  const trailing = token.match(/\s*$/)![0];
  return { value: el.value.slice(0, start) + leading + tag + trailing + el.value.slice(end), caret: start + leading.length + tag.length };
}

const commonPrefix = (a: string, b: string) => { let n = 0; while (n < a.length && n < b.length && a[n] === b[n]) n++; return n; };
const commonSuffix = (a: string, b: string, floor: number) => { let n = 0; while (n < a.length - floor && n < b.length - floor && a[a.length - 1 - n] === b[b.length - 1 - n]) n++; return n; };
/** Typed through the editing command so Ctrl+Z can take it back: assigning `value` wipes the undo stack. `whole` is the fallback where that command is missing. */
function replaceRange(el: HTMLTextAreaElement, start: number, end: number, text: string, whole: string) {
  el.focus();
  el.setSelectionRange(start, end);
  if (document.execCommand?.('insertText', false, text) && el.value === whole) return;
  // Use the native input path so controlled and uncontrolled textarea callers behave alike.
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(el, whole);
  el.dispatchEvent(new Event('input', { bubbles: true }));
}

export const PromptTextarea = forwardRef<HTMLTextAreaElement, PromptTextareaProps>(function PromptTextarea({ root, target, onChange, onSelect, onFocus, onBlur, onKeyDown, onCompositionStart, onCompositionEnd, ...props }, ref) {
  const tools = usePromptTools();
  const { t } = useI18n();
  const id = useId();
  const local = useRef<HTMLTextAreaElement>(null);
  const generation = useRef(0);
  const composing = useRef(false);
  const focused = useRef(false);
  const dismissed = useRef('');
  const [selection, setSelection] = useState('');
  const [hits, setHits] = useState<TagSuggestion[]>([]);
  const [cursor, setCursor] = useState(0);
  const [error, setError] = useState('');
  // In-app spellcheck: the mirror draws underlines; the native WebView checker is off in Tauri.
  const native = isTauri();
  const text = typeof props.value === 'string' ? props.value : local.current?.value ?? '';
  const marks = useSpellcheck(text, tools.language, native && tools.spellcheck && !props.disabled && !props.readOnly);
  const mirror = useRef<HTMLDivElement>(null);
  const [fix, setFix] = useState<{ mark: SpellMark; items: string[] | null } | null>(null);
  const [fixCursor, setFixCursor] = useState(0);
  // Render-time scope also rejects promises resolved before effect cleanup runs.
  const scope = JSON.stringify([root, target, tools.autocomplete, tools.spellcheck, tools.language, props.disabled, props.readOnly, props.value, props.maxLength]);
  const latest = useRef(scope);
  latest.current = scope;
  const fingerprint = () => { const el = local.current; return el ? JSON.stringify([el.value, el.selectionStart, el.selectionEnd, focused.current, composing.current]) : ''; };
  const sync = () => {
    const next = fingerprint();
    if (next !== selection) { generation.current++; setHits([]); setError(''); setSelection(next); }
  };
  const close = () => { generation.current++; dismissed.current = fingerprint(); setHits([]); setError(''); };

  useLayoutEffect(() => {
    const el = local.current, twin = mirror.current;
    if (!el || !twin) return;
    const copy = () => {
      const style = getComputedStyle(el);
      for (const key of ['font', 'letterSpacing', 'lineHeight', 'padding', 'borderWidth', 'textIndent', 'textTransform', 'wordSpacing'] as const) twin.style[key] = style[key];
      twin.style.width = `${el.offsetWidth}px`; twin.style.height = `${el.offsetHeight}px`;
      twin.scrollTop = el.scrollTop; twin.scrollLeft = el.scrollLeft;
    };
    copy();
    const observer = new ResizeObserver(copy);
    observer.observe(el);
    const scroll = () => { twin.scrollTop = el.scrollTop; twin.scrollLeft = el.scrollLeft; };
    el.addEventListener('scroll', scroll);
    return () => { observer.disconnect(); el.removeEventListener('scroll', scroll); };
  }, [marks.length > 0]);

  const markAt = (caret: number) => marks.find(m => m.start <= caret && caret <= m.end);
  const openFix = (mark: SpellMark) => {
    setFix({ mark, items: null }); setFixCursor(0);
    spellcheckSuggest(tools.language, mark.word).then(items => setFix(f => f?.mark === mark ? { mark, items } : f), () => setFix(f => f?.mark === mark ? { mark, items: [] } : f));
  };
  const applyFix = (word: string) => {
    const el = local.current!, { mark } = fix!;
    const value = el.value.slice(0, mark.start) + word + el.value.slice(mark.end);
    if (props.maxLength !== undefined && props.maxLength >= 0 && value.length > props.maxLength) return;
    setFix(null);
    replaceRange(el, mark.start, mark.end, word, value);
    el.setSelectionRange(mark.start + word.length, mark.start + word.length);
    sync();
  };

  useEffect(() => {
    const request = ++generation.current;
    setHits([]); setError('');
    const el = local.current!;
    const mark = fingerprint();
    const valid = () => generation.current === request && latest.current === scope && fingerprint() === mark;
    if (!tools.autocomplete || !root || !target || props.disabled || props.readOnly || !focused.current || composing.current || el.selectionStart !== el.selectionEnd || dismissed.current === mark) return;
    const prefix = el.value.slice(0, el.selectionStart).split(/[,\n]/).pop()!.trim();
    if (prefix.length < 2) return;
    const timer = setTimeout(() => {
      if (!valid()) return;
      void tagSearch(root, target, prefix).then(result => {
        if (valid()) {
          setHits(result.filter(hit => props.maxLength === undefined || props.maxLength < 0 || replacement(el, hit.tag).value.length <= props.maxLength).slice(0, 8));
          setCursor(0);
        }
      }, e => { if (valid()) setError(forgeError(e)); });
    }, 150);
    return () => { generation.current++; clearTimeout(timer); };
  }, [selection, scope]);

  useEffect(() => {
    document.getElementById(`${id}-${cursor}`)?.scrollIntoView?.({ block: 'nearest' });
  }, [cursor, hits, id]);

  const pick = (hit: TagSuggestion) => {
    const el = local.current!;
    const { value, caret } = replacement(el, hit.tag);
    if (props.maxLength !== undefined && props.maxLength >= 0 && value.length > props.maxLength) return;
    close();
    const start = commonPrefix(el.value, value), tail = commonSuffix(el.value, value, start);
    replaceRange(el, start, el.value.length - tail, value.slice(start, value.length - tail), value);
    el.setSelectionRange(caret, caret);
    dismissed.current = fingerprint();
    sync();
  };

  return <div className="prompt-textarea">
    <textarea {...props} spellCheck={native ? false : tools.spellcheck} lang={tools.language} ref={node => {
      local.current = node;
      if (typeof ref === 'function') ref(node); else if (ref) ref.current = node;
    }}
      aria-autocomplete={tools.autocomplete ? 'list' : undefined}
      aria-controls={hits.length ? id : undefined}
      aria-expanded={tools.autocomplete ? hits.length > 0 : undefined}
      aria-activedescendant={hits.length ? `${id}-${cursor}` : undefined}
      onChange={e => { setFix(null); sync(); onChange?.(e); }}
      onSelect={e => { sync(); onSelect?.(e); }}
      onFocus={e => { focused.current = true; sync(); onFocus?.(e); }}
      onBlur={e => { focused.current = false; setFix(null); sync(); onBlur?.(e); }}
      onContextMenu={e => {
        // The browser moves the caret to the clicked word before this event fires.
        const mark = marks.length ? markAt((e.target as HTMLTextAreaElement).selectionStart) : undefined;
        if (mark) { e.preventDefault(); openFix(mark); }
      }}
      onCompositionStart={e => { composing.current = true; sync(); onCompositionStart?.(e); }}
      onCompositionEnd={e => { composing.current = false; sync(); onCompositionEnd?.(e); }}
      onKeyDown={e => {
        if (fix) {
          const items = fix.items ?? [];
          if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); setFix(null); return; }
          if (items.length && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) { e.preventDefault(); setFixCursor(i => (i + (e.key === 'ArrowDown' ? 1 : items.length - 1)) % items.length); return; }
          if (items.length && e.key === 'Enter' && !e.ctrlKey && !e.metaKey) { e.preventDefault(); applyFix(items[fixCursor]); return; }
        }
        // Ctrl+. opens suggestions for the misspelled word under the caret (keyboard twin of right-click).
        if ((e.ctrlKey || e.metaKey) && e.key === '.' && marks.length) { const mark = markAt(e.currentTarget.selectionStart); if (mark) { e.preventDefault(); openFix(mark); return; } }
        if (!composing.current && !e.nativeEvent.isComposing && hits.length) {
          if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close(); return; }
          if (e.key === 'ArrowDown' || e.key === 'ArrowUp') { e.preventDefault(); setCursor(i => (i + (e.key === 'ArrowDown' ? 1 : hits.length - 1)) % hits.length); return; }
          if ((e.key === 'Enter' && !e.ctrlKey && !e.metaKey) || (e.key === 'Tab' && !e.shiftKey)) { e.preventDefault(); pick(hits[cursor]); return; }
        }
        onKeyDown?.(e);
      }} />
    {marks.length > 0 && <div ref={mirror} className="prompt-mirror" aria-hidden="true">{segments(text, marks)}</div>}
    {fix && <ul className="prompt-suggestions prompt-fixes" role="listbox" aria-label={t('spellcheck.suggestions', { word: fix.mark.word })}>
      {fix.items === null ? <li className="prompt-fix-none">…</li>
        : fix.items.length === 0 ? <li className="prompt-fix-none">{t('spellcheck.noSuggestions')}</li>
        : fix.items.map((item, i) => <li key={item} role="option" aria-selected={fixCursor === i} onMouseDown={e => { e.preventDefault(); applyFix(item); }}><span>{item}</span></li>)}
    </ul>}
    {hits.length > 0 && <ul id={id} className="prompt-suggestions" role="listbox" aria-label={t('forge.tags.search')}>
      {hits.map((hit, i) => <li id={`${id}-${i}`} key={`${hit.tag}-${i}`} role="option" aria-selected={cursor === i}
        onMouseDown={e => { e.preventDefault(); pick(hit); }}>
        <span>{hit.tag}</span><em>{hit.category} · {hit.count.toLocaleString()}</em>
      </li>)}
    </ul>}
    {error && <p className="error-message prompt-error" role="alert">{t(error)}</p>}
  </div>;
});

/** Text split into plain runs and <mark>ed misspellings; the trailing newline keeps the last line's height like the textarea. */
function segments(text: string, marks: SpellMark[]) {
  const out = [];
  let at = 0;
  for (const mark of marks) {
    if (mark.start > at) out.push(text.slice(at, mark.start));
    out.push(<mark key={mark.start}>{text.slice(mark.start, mark.end)}</mark>);
    at = mark.end;
  }
  out.push(text.slice(at) + '\n');
  return out;
}
