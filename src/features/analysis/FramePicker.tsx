import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent } from 'react';
import { convertFileSrc, isTauri } from '@tauri-apps/api/core';
import { useI18n } from '../../i18n/I18nProvider';
import { analysisError, clipFrame, clock, openClip, saveFrames, type Clip } from '../../lib/analysis';
import { describeMedia, itemFromEntry, type MediaItem } from '../../lib/media';
import type { MediaAction } from '../media/MediaLibrary';

const src = (path: string) => (isTauri() ? convertFileSrc(path) : path);

export interface FramePickerProps {
  video: { path: string; name: string };
  /** "Use this frame": the frame becomes the picture Media Analysis reads (tags, Describe). */
  onUse: (frame: MediaItem) => void;
  /** What the Generate tab would do with a picture right now (Wan first/last frame, H3 pictures, ...). */
  frameActions?: (item: MediaItem) => MediaAction[];
  onClose: () => void;
}

/** Every frame of a video in a strip; step, pick, save, or send a frame on. Ported from PromptForge's frame picker, with frames from FFmpeg by number. */
export function FramePicker({ video, onUse, frameActions, onClose }: FramePickerProps) {
  const { t } = useI18n();
  const [clip, setClip] = useState<Clip | null>(null);
  const [error, setError] = useState('');
  const [index, setIndex] = useState(0);
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const [last, setLast] = useState<number | null>(null);
  const [full, setFull] = useState<{ index: number; path: string } | null>(null);
  const [busy, setBusy] = useState('');
  const [note, setNote] = useState('');
  const strip = useRef<HTMLDivElement>(null);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let alive = true;
    setClip(null); setError(''); setIndex(0); setPicked(new Set()); setFull(null); setNote('');
    openClip(video.path).then(c => { if (alive) setClip(c); }, e => { if (alive) setError(analysisError(e)); });
    return () => { alive = false; };
  }, [video.path]);

  // The full-size frame follows the current one, a moment after stepping stops.
  useEffect(() => {
    if (!clip) return;
    let alive = true;
    const timer = setTimeout(() => {
      clipFrame(video.path, index, clip.count).then(path => { if (alive) setFull({ index, path }); }, e => { if (alive) setError(analysisError(e)); });
    }, 180);
    return () => { alive = false; clearTimeout(timer); };
  }, [clip, index, video.path]);

  const thumb = (i: number) => clip ? clip.thumbs[Math.min(clip.thumbs.length - 1, Math.floor(i / clip.stride))] : undefined;
  useEffect(() => {
    if (!clip) return;
    strip.current?.querySelector<HTMLElement>(`[data-frame-cell="${Math.floor(index / clip.stride) * clip.stride}"]`)?.scrollIntoView?.({ block: 'nearest', inline: 'center' });
  }, [index, clip]);

  if (error && !clip) return <div className="analysis-frames" data-frame-picker>
    <p className="error-message" role="alert">{error}</p>
    <button type="button" className="forge-ghost" onClick={onClose}>{t('analysis.frames.close')}</button>
  </div>;
  if (!clip) return <div className="analysis-frames" data-frame-picker><p className="chat-status" role="status">{t('analysis.frames.reading', { name: video.name })}</p></div>;

  const go = (i: number) => setIndex(Math.max(0, Math.min(clip.count - 1, i)));
  const toggle = (i: number) => setPicked(p => { const n = new Set(p); if (n.has(i)) n.delete(i); else n.add(i); return n; });
  const time = (i: number) => clip.fps > 0 ? clock(i / clip.fps) : '';
  const frameItem = async (i: number): Promise<MediaItem> => {
    const path = await clipFrame(video.path, i, clip.count);
    const described = isTauri() ? (await describeMedia([path])).map(itemFromEntry).find(Boolean) : null;
    return described ?? { name: path.split(/[\\/]/).pop() ?? 'frame.png', path, src: src(path), media: 'image' };
  };
  const work = async (label: string, fn: () => Promise<void>) => {
    setBusy(label); setError(''); setNote('');
    try { await fn(); } catch (e) { setError(analysisError(e)); } finally { setBusy(''); }
  };
  const use = () => work('use', async () => onUse(await frameItem(index)));
  const save = (indices: number[]) => work('save', async () => {
    const n = await saveFrames(video.path, [...indices].sort((a, b) => a - b), clip.count, t('analysis.frames.saveTitle'));
    if (n) setNote(t('analysis.frames.saved', { count: String(n) }));
  });
  // Sends are the Generate tab's own actions for a picture, asked for the frame that is about to go.
  const placeholder: MediaItem = { name: `${video.name}-f${index + 1}.png`, path: `${video.path}.frame-${index + 1}.png`, src: thumb(index) ? src(thumb(index)!) : '', media: 'image' };
  const sends = (frameActions?.(placeholder) ?? []);
  // A send that collects pictures (references) takes every picked frame, in order; the others take the current frame.
  const manyAt = (at: number) => !!sends[at]?.many && picked.size > 0;
  const send = (at: number) => work(`send-${at}`, async () => {
    const label = sends[at]?.label ?? '';
    const order = manyAt(at) ? [...picked].sort((a, b) => a - b) : [index];
    const sent: number[] = [];
    const skipped = new Map<string, number[]>(); // reason ('' = already there) → frames
    for (const i of order) {
      const item = await frameItem(i);
      // Let Generate render the previous frame's send, so its actions (and limits) are current.
      if (sent.length) await new Promise(r => setTimeout(r, 0));
      const action = (frameActions?.(item) ?? [])[at];
      if (!action) throw new Error(t('analysis.frames.sendGone'));
      // This frame is already there (the action turned into its Remove): say so, change nothing.
      if (action.label !== label) {
        if (order.length === 1) { setNote(t('analysis.frames.already', { action: label })); return; }
        skipped.set('', [...(skipped.get('') ?? []), i + 1]); continue;
      }
      if (action.blocked) {
        if (order.length === 1) throw new Error(action.blocked);
        skipped.set(action.blocked, [...(skipped.get(action.blocked) ?? []), i + 1]); continue;
      }
      action.onClick();
      sent.push(i + 1);
    }
    const done = sent.length === 1 ? t('analysis.frames.sent', { action: label, frame: String(sent[0]) })
      : sent.length ? t('analysis.frames.sentMany', { count: String(sent.length), frames: sent.join(', '), action: label }) : '';
    const frames = (list: number[]) => list.length === 1 ? t('analysis.frames.listOne', { n: String(list[0]) }) : t('analysis.frames.listMany', { list: list.join(', ') });
    const why = [...skipped].map(([reason, list]) => reason ? t('analysis.frames.skipReason', { frames: frames(list), reason }) : t('analysis.frames.skipAlready', { frames: frames(list) }));
    setNote([done, why.length ? t('analysis.frames.notSent', { list: why.join(' · ').replace(/[.。]$/, '') }) : ''].filter(Boolean).join(' '));
  });
  const click = (i: number, e: MouseEvent) => {
    if (e.shiftKey && last !== null) {
      const [a, b] = [Math.min(last, i), Math.max(last, i)];
      setPicked(p => { const n = new Set(p); for (let k = a; k <= b; k += clip.stride) n.add(k); return n; });
    } else if (e.ctrlKey || e.metaKey) toggle(i);
    else go(i);
    setLast(i);
  };
  const key = (e: KeyboardEvent) => {
    if ((e.target as HTMLElement).closest('input, select, textarea')) return;
    const step = e.shiftKey ? 10 : 1;
    if (e.key === 'ArrowLeft') { e.preventDefault(); go(index - step); }
    else if (e.key === 'ArrowRight') { e.preventDefault(); go(index + step); }
    else if (e.key === 'Home') { e.preventDefault(); go(0); }
    else if (e.key === 'End') { e.preventDefault(); go(clip.count - 1); }
    else if (e.key === ' ' && !(e.target as HTMLElement).closest('button')) { e.preventDefault(); toggle(index); }
    else if (e.key === 'Enter' && !(e.target as HTMLElement).closest('button')) { e.preventDefault(); void use(); }
  };
  const isPicked = picked.has(index);
  const shown = full?.index === index ? src(full.path) : thumb(index) ? src(thumb(index)!) : '';
  const cells = clip.thumbs.map((path, k) => k * clip.stride);

  return <div className="analysis-frames" data-frame-picker ref={root} tabIndex={0} onKeyDown={key} aria-label={t('analysis.frames.title')}>
    <div className="analysis-frames-head">
      <strong>{t('analysis.frames.title')}</strong>
      <span className="forge-note" data-frame-meta>{t('analysis.frames.meta', { name: clip.name, width: String(clip.width), height: String(clip.height), count: String(clip.count), fps: clip.fps ? String(Math.round(clip.fps * 1000) / 1000) : '?', length: clock(clip.duration) })}</span>
      <button type="button" className="forge-ghost" data-frame-close onClick={onClose}>{t('analysis.frames.close')}</button>
    </div>
    <div className="analysis-frames-stage">
      <button type="button" className="analysis-frames-side" disabled={index === 0} aria-label={t('analysis.frames.prev')} onClick={() => go(index - 1)}>{index > 0 && thumb(index - 1) && <img src={src(thumb(index - 1)!)} alt=""/>}</button>
      <div className={`analysis-frames-current${isPicked ? ' picked' : ''}`}>
        {shown && <img src={shown} alt={t('analysis.frames.frame', { n: String(index + 1) })} data-frame-current={full?.index === index ? 'full' : 'thumb'}/>}
        {isPicked && <span className="analysis-frames-badge">✓ {t('analysis.frames.picked')}</span>}
      </div>
      <button type="button" className="analysis-frames-side" disabled={index >= clip.count - 1} aria-label={t('analysis.frames.next')} onClick={() => go(index + 1)}>{index < clip.count - 1 && thumb(index + 1) && <img src={src(thumb(index + 1)!)} alt=""/>}</button>
    </div>
    <div className="analysis-frames-nav">
      <button type="button" className="forge-ghost" data-frame-prev title={t('analysis.frames.prevHelp')} onClick={() => go(index - 1)}>‹</button>
      <span data-frame-pos>{t('analysis.frames.position', { n: String(index + 1), count: String(clip.count), time: time(index) })}</span>
      <button type="button" className="forge-ghost" data-frame-next title={t('analysis.frames.nextHelp')} onClick={() => go(index + 1)}>›</button>
    </div>
    <div className="analysis-frames-strip" ref={strip} role="listbox" aria-label={t('analysis.frames.strip')} aria-multiselectable="true">
      {cells.map(i => <button type="button" key={i} role="option" aria-selected={picked.has(i)} data-frame-cell={i}
        className={`analysis-frames-cell${Math.floor(index / clip.stride) * clip.stride === i ? ' current' : ''}${picked.has(i) ? ' picked' : ''}`}
        title={t('analysis.frames.frame', { n: String(i + 1) })} onClick={e => click(i, e)}>
        <img src={src(clip.thumbs[i / clip.stride])} alt="" loading="lazy"/><span>{i + 1}</span>
      </button>)}
    </div>
    {clip.stride > 1 && <p className="forge-note">{t('analysis.frames.strided', { stride: String(clip.stride) })}</p>}
    <div className="forge-actions analysis-frames-actions">
      <button type="button" className="action-button" data-frame-use disabled={!!busy} title={t('analysis.frames.useHelp')} onClick={() => void use()}>{t(busy === 'use' ? 'analysis.frames.working' : 'analysis.frames.use')}</button>
      <button type="button" className="forge-ghost" data-frame-pick title={t('analysis.frames.pickHelp')} onClick={() => toggle(index)}>{t(isPicked ? 'analysis.frames.unpick' : 'analysis.frames.pick')}</button>
      <button type="button" className="forge-ghost" data-frame-save disabled={!!busy} onClick={() => void save([index])}>{t('analysis.frames.save')}</button>
      <button type="button" className="forge-ghost" data-frame-save-picked disabled={!!busy || !picked.size} onClick={() => void save([...picked])}>{t('analysis.frames.savePicked', { count: String(picked.size) })}</button>
      <button type="button" className="forge-ghost" data-frame-clear disabled={!picked.size} onClick={() => setPicked(new Set())}>{t('analysis.frames.clear')}</button>
    </div>
    {sends.length > 0 && <div className="forge-actions analysis-frames-sends" data-frame-sends>
      <span className="forge-note">{t('analysis.frames.sendTo')}</span>
      {sends.map((a, at) => <button type="button" key={`${at}-${a.label}`} className="forge-ghost" data-frame-send={at} disabled={!!busy || !!a.blocked} title={a.blocked || (manyAt(at) ? t('analysis.frames.sendManyHelp', { count: String(picked.size) }) : undefined)} data-frame-send-many={manyAt(at) ? picked.size : undefined} onClick={() => void send(at)}>{busy === `send-${at}` ? t('analysis.frames.working') : manyAt(at) ? `${a.label} (${picked.size})` : a.label}</button>)}
    </div>}
    {!sends.length && <p className="forge-note" data-frame-no-sends>{t('analysis.frames.noSends')}</p>}
    {note && <p className="forge-note" role="status" data-frame-note>{note}</p>}
    {error && <p className="error-message" role="alert">{error}</p>}
    <p className="forge-note">{t('analysis.frames.privacy')}</p>
  </div>;
}
