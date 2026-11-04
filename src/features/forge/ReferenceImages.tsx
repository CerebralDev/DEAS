import { useEffect, useRef, useState, type Dispatch, type DragEvent, type SetStateAction } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { useI18n } from '../../i18n/I18nProvider';
import { forgeError, referenceReadback, tagSearch, type NativeReferences } from '../../lib/forge';
import { describeMedia, itemFromEntry, type MediaItem } from '../../lib/media';

export interface RefPicture { id: string; name: string; image: string; role: string; fidelity: string; note: string; character: string; readback?: { text: string; key: string } }

/** The longest side a picture goes up at: enough for a vision model to read, small enough to send. */
const MAX_SIDE = 1536;
const TYPES = ['image/png', 'image/jpeg', 'image/webp'];
const readAs = (file: Blob) => new Promise<string>((ok, no) => { const r = new FileReader(); r.onload = () => ok(String(r.result)); r.onerror = () => no(r.error); r.readAsDataURL(file); });

/** A data URI the model can take: PNG, JPEG or WebP, scaled down when it is large. */
export async function prepare(file: File): Promise<string> {
  const bitmap = typeof createImageBitmap === 'function' ? await createImageBitmap(file).catch(() => null) : null;
  if (!bitmap) {
    if (TYPES.includes(file.type)) return readAs(file);
    throw new Error('unreadable');
  }
  const scale = Math.min(1, MAX_SIDE / Math.max(bitmap.width, bitmap.height));
  if (scale === 1 && TYPES.includes(file.type) && file.size < 4 * 1024 * 1024) return readAs(file);
  const canvas = document.createElement('canvas');
  canvas.width = Math.round(bitmap.width * scale); canvas.height = Math.round(bitmap.height * scale);
  canvas.getContext('2d')!.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  return canvas.toDataURL('image/jpeg', 0.92);
}

/** A dropped file as `prepare` takes it. The asset server may not name a type, and only `add`'s filter reads it. */
async function fileFrom(item: MediaItem): Promise<File> {
  const response = await fetch(item.src);
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const blob = await response.blob();
  return new File([blob], item.name, { type: blob.type.startsWith('image/') ? blob.type : 'image/*' });
}

/** A read-back as the model wrote it, with its `**heading**` lines shown as headings rather than asterisks. */
export function Reading({ text }: { text: string }) {
  return <div className="forge-reading">{text.split(/\r?\n/).map((line, i) => {
    const head = /^\s*\*\*(.+?)\*\*:?\s*$/.exec(line)?.[1] ?? /^\s*#{1,4}\s+(.+)$/.exec(line)?.[1];
    return head ? <strong key={i}>{head}</strong> : line.trim() ? <p key={i}>{line.replace(/\*\*(.+?)\*\*/g, '$1')}</p> : null;
  })}</div>;
}

/** Changing what a picture is for, how closely to follow it or the note makes an old read-back wrong. */
const readKey = (p: RefPicture) => JSON.stringify([p.role, p.fidelity, p.note.trim(), p.character.trim()]);

/**
 * PromptForge's reference pictures: each says what it is FOR and how closely to follow it, with a note that outranks
 * both. Before writing, the model reads them under their roles and the prompt is written from that reading; Analyze
 * shows the read-back first, so a wrong role is fixed before a write is spent.
 */
export function ReferenceImages({ root, target, model, vocab, pictures, onPictures }: { root: string; target: string; model: string; vocab: NativeReferences; pictures: RefPicture[]; onPictures: Dispatch<SetStateAction<RefPicture[]>> }) {
  const { t } = useI18n();
  const [role, setRole] = useState(vocab.defaultRole ?? vocab.roles[0]?.key ?? '');
  const [fidelity, setFidelity] = useState(vocab.defaultFidelity ?? vocab.fidelity[0]?.key ?? '');
  const [reading, setReading] = useState('');
  const [over, setOver] = useState(false);
  const [error, setError] = useState('');
  const [names, setNames] = useState<string[]>([]);

  const add = async (files: FileList | File[]) => {
    setError('');
    const room = vocab.max - pictures.length;
    const usable = [...files].filter(f => f.type.startsWith('image/'));
    if (usable.length > room) setError(t('forge.refs.tooMany', { max: String(vocab.max) }));
    try {
      const added = await Promise.all(usable.slice(0, Math.max(0, room)).map(async f => ({ id: crypto.randomUUID(), name: f.name, image: await prepare(f), role, fidelity, note: '', character: '' })));
      onPictures(current => [...current, ...added].slice(0, vocab.max));
    } catch { setError(t('forge.refs.unreadable')); }
  };
  const box = useRef<HTMLDivElement>(null);
  const refs = useRef<HTMLDivElement>(null);
  const adding = useRef(add); adding.current = add;
  useEffect(() => {
    const element = refs.current;
    if (!element) return;
    const dialogs = 'dialog, [role="dialog"]';
    const scope = element.closest(`.forge-builder, ${dialogs}`) ?? element;
    const paste = (event: ClipboardEvent) => {
      if (event.defaultPrevented || scope.closest('[hidden], dialog:not([open])') || !(event.target instanceof Element)
        || !scope.contains(event.target) || event.target.closest(dialogs) !== element.closest(dialogs)) return;
      const files = [...(event.clipboardData?.files ?? [])].filter(file => file.type.startsWith('image/'));
      if (!files.length) return;
      event.preventDefault();
      void adding.current(files);
    };
    document.addEventListener('paste', paste);
    return () => document.removeEventListener('paste', paste);
  }, []);
  // Native webviews deliver OS file drops as paths for the whole window, never as DOM files.
  useEffect(() => {
    if (!isTauri()) return;
    let alive = true, unlisten: (() => void) | undefined;
    const inside = (p: { x: number; y: number }) => {
      const r = box.current?.getBoundingClientRect(), x = p.x / devicePixelRatio, y = p.y / devicePixelRatio;
      return !!r && Array.from(document.querySelectorAll('dialog[open]')).every(modal => modal.contains(box.current)) && x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
    };
    void getCurrentWebview().onDragDropEvent(e => {
      if (e.payload.type === 'leave') { setOver(false); return; }
      const hit = inside(e.payload.position);
      setOver(e.payload.type !== 'drop' && hit);
      if (e.payload.type !== 'drop' || !hit) return;
      void describeMedia(e.payload.paths)
        .then(entries => Promise.all(entries.map(itemFromEntry).flatMap(item => item?.media === 'image' ? [fileFrom(item)] : [])))
        .then(files => { if (alive) { if (files.length) void adding.current(files); else setError(t('forge.refs.unreadable')); } })
        .catch(() => { if (alive) setError(t('forge.refs.unreadable')); });
    }).then(stop => { if (alive) unlisten = stop; else stop(); }, err => { if (alive) setError(String(err)); });
    return () => { alive = false; unlisten?.(); };
  }, [t]);
  const set = (id: string, change: Partial<RefPicture>) => onPictures(current => current.map(p => p.id === id ? { ...p, ...change } : p));
  const analyze = async (p: RefPicture) => {
    setReading(p.id); setError('');
    try { set(p.id, { readback: { text: await referenceReadback(root, model, { role: p.role, fidelity: p.fidelity, note: p.note, character: p.character, image: p.image }), key: readKey(p) } }); }
    catch (e) { setError(forgeError(e)); }
    finally { setReading(''); }
  };
  const lookup = (q: string) => { if (q.trim().length >= 2) tagSearch(root, target, q).then(h => setNames(h.filter(x => x.category === 'character').map(x => x.tag)), () => {}); };
  const drop = (e: DragEvent) => { e.preventDefault(); setOver(false); void add(e.dataTransfer.files); };
  const pick = (label: string, value: string, set: (v: string) => void, list: { key: string; label: string }[], name: string) =>
    <label className="forge-field"><span>{label}</span><select data-forge-ref-select={name} value={value} onChange={e => set(e.target.value)}>{list.map(o => <option key={o.key} value={o.key}>{o.label}</option>)}</select></label>;
  const ruleOf = (key: string) => vocab.roles.find(r => r.key === key)?.rule;

  return <div ref={refs} className="forge-refs">
    {pictures.map((p, i) => {
      const fresh = p.readback && p.readback.key === readKey(p);
      return <div key={p.id} className="forge-refcard" data-forge-ref={i + 1}>
        <img src={p.image} alt={p.name}/>
        <div className="forge-refcard-body">
          <div className="forge-length">
            {pick(t('forge.refs.role'), p.role, v => set(p.id, { role: v }), vocab.roles, `role-${i + 1}`)}
            {pick(t('forge.refs.fidelity'), p.fidelity, v => set(p.id, { fidelity: v }), vocab.fidelity, `fidelity-${i + 1}`)}
          </div>
          {ruleOf(p.role) && <p className="forge-note">{ruleOf(p.role)}</p>}
          {p.role === 'character' && <label className="forge-field"><span>{t('forge.refs.character')}</span>
            <input type="text" list={`forge-ref-names-${p.id}`} data-forge-ref-character value={p.character} placeholder={t('forge.refs.characterHint')} onChange={e => { set(p.id, { character: e.target.value }); lookup(e.target.value); }}/>
            <datalist id={`forge-ref-names-${p.id}`}>{names.map(n => <option key={n} value={n}/>)}</datalist></label>}
          <label className="forge-field"><span>{t('forge.refs.note')} <em>{t('forge.refs.noteHint')}</em></span><input type="text" data-forge-ref-note maxLength={2000} value={p.note} onChange={e => set(p.id, { note: e.target.value })}/></label>
          <div className="forge-actions">
            {vocab.analyze && <button type="button" className="forge-ghost" data-forge-ref-analyze disabled={!!reading || !model} onClick={() => void analyze(p)}>{t(reading === p.id ? 'forge.refs.reading' : fresh ? 'forge.refs.analyzeAgain' : 'forge.refs.analyze')}</button>}
            <button type="button" className="forge-ghost" data-forge-ref-remove aria-label={t('forge.refs.remove', { n: String(i + 1) })} onClick={() => onPictures(current => current.filter(x => x.id !== p.id))}>{t('forge.refs.removeShort')}</button>
          </div>
          {p.readback && <details className="forge-readback" open={!!fresh} data-forge-readback>
            <summary>{t(fresh ? 'forge.refs.readback' : 'forge.refs.readbackStale')}</summary>
            <Reading text={p.readback.text}/>
          </details>}
        </div>
      </div>;
    })}
    {pictures.length < vocab.max && <div ref={box} className={`forge-refdrop${over ? ' over' : ''}`} onDragOver={e => { e.preventDefault(); setOver(true); }} onDragLeave={() => setOver(false)} onDrop={drop}>
      <div className="forge-length">
        {pick(t('forge.refs.role'), role, setRole, vocab.roles, 'next-role')}
        {pick(t('forge.refs.fidelity'), fidelity, setFidelity, vocab.fidelity, 'next-fidelity')}
      </div>
      <label className="forge-refadd">
        <input type="file" data-forge-ref-file accept={TYPES.join(',')} multiple onChange={e => { if (e.target.files) void add(e.target.files); e.target.value = ''; }}/>
        <span>{t('forge.refs.add')}</span>
      </label>
      <p className="forge-note">{t('forge.refs.help', { count: String(pictures.length), max: String(vocab.max) })}</p>
    </div>}
    {error && <p className="error-message" role="alert">{t(error)}</p>}
  </div>;
}
