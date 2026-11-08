import { useEffect, useRef, useState, type ClipboardEvent, type DragEvent } from 'react';
import { convertFileSrc, isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { useI18n } from '../../i18n/I18nProvider';
import { analysisError, analysisOverview, cancelAnalysisSetup, modelFolders, setupAnalysis, stagePicture, tagPicture, tagsAsPrompt, type AnalysisOverview, type TagResult } from '../../lib/analysis';
import { imageMetadataRows, readImageMetadata, type ImageMetadata } from '../../lib/imageMetadata';
import { describeMedia, itemFromEntry, type MediaItem } from '../../lib/media';
import { isBoolean, isString, usePref } from '../../lib/preferences';
import './analysis.css';

export interface AnalysisPicture { path: string; src: string; name: string; media: 'image' | 'video' }
export interface AnalysisProps {
  active?: boolean;
  roots?: { models?: string; extraModels?: string[] };
  root?: string;
  /** A picture sent from the Media dock ("Open in Media Analysis"); `n` changes per send. */
  incoming?: { item: MediaItem; n: number };
  onSend: (positive: string, negative: string) => void;
}

const gb = (bytes: number) => (bytes / 1024 ** 3).toFixed(1);
const percent = (p: number) => `${Math.round(p * 100)}%`;

/** Media Analysis: what is in a picture, as tags (and the settings that made it). Ported from PromptForge. */
export function Analysis({ active = true, roots, root, incoming, onSend }: AnalysisProps) {
  const { t } = useI18n();
  const models = modelFolders(roots?.models, roots?.extraModels, root);
  const key = models.join('\n');
  const [overview, setOverview] = useState<AnalysisOverview | null>(null);
  const [tagger, setTagger] = usePref('analysis.tagger', '', isString);
  const [spaces, setSpaces] = usePref('analysis.spaces', true, isBoolean);
  const [general, setGeneral] = useState<number | null>(null);
  const [character, setCharacter] = useState<number | null>(null);
  const [picture, setPicture] = useState<AnalysisPicture | null>(null);
  const [result, setResult] = useState<TagResult | null>(null);
  const [made, setMade] = useState<ImageMetadata | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [over, setOver] = useState(false);
  const [copied, setCopied] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const run = useRef(0);

  const refresh = () => analysisOverview(models).then(setOverview, e => setError(analysisError(e)));
  useEffect(() => { if (active) void refresh(); }, [active, key]); // eslint-disable-line react-hooks/exhaustive-deps
  // While setup runs, its progress is read once a second.
  useEffect(() => {
    if (!overview?.install.running) return;
    const timer = setInterval(() => void refresh(), 1000);
    return () => clearInterval(timer);
  }, [overview?.install.running]); // eslint-disable-line react-hooks/exhaustive-deps

  const taggers = overview?.taggers ?? [];
  const chosen = tagger === 'all' && taggers.length > 1 ? 'all' : (taggers.find(x => x.id === tagger) ?? taggers[0])?.id ?? '';
  const current = taggers.find(x => x.id === chosen);
  const ready = !!overview?.runtimeReady && taggers.length > 0;
  // A different tagger starts from its own thresholds (WD's 0.35/0.85 hid animetimm characters).
  useEffect(() => { setGeneral(null); setCharacter(null); }, [chosen]);

  const tag = async (pic = picture) => {
    if (!pic || pic.media !== 'image' || !ready || !chosen) return;
    const mine = ++run.current;
    setBusy(true); setError(''); setResult(null);
    try {
      const out = await tagPicture(models, chosen, pic.path, chosen === 'all' ? undefined : general ?? undefined, chosen === 'all' ? undefined : character ?? undefined);
      if (mine === run.current) setResult(out);
    } catch (e) { if (mine === run.current) setError(analysisError(e)); }
    finally { if (mine === run.current) setBusy(false); }
  };
  const load = (pic: AnalysisPicture) => {
    run.current++;
    setPicture(pic); setResult(null); setMade(null); setError(''); setBusy(false);
    if (pic.media === 'image') {
      readImageMetadata(pic.path).then(entries => setMade(imageMetadataRows(entries)), () => setMade([]));
      void tag(pic);
    }
  };
  const fromItem = (item: MediaItem) => item.path && (item.media === 'image' || item.media === 'video')
    ? load({ path: item.path, src: item.src, name: item.name, media: item.media }) : setError(t('analysis.unsupported'));
  useEffect(() => { if (incoming) fromItem(incoming.item); }, [incoming?.n]); // eslint-disable-line react-hooks/exhaustive-deps
  // Tags as soon as the runtime is ready for a picture that arrived before it was.
  useEffect(() => { if (ready && picture && !result && !busy && !error) void tag(); }, [ready]); // eslint-disable-line react-hooks/exhaustive-deps

  const fromFile = async (file?: File) => {
    if (!file) return;
    if (!/^image\/(png|jpeg|webp)$/.test(file.type)) { setError(t('analysis.unsupported')); return; }
    try { const path = await stagePicture(file); load({ path, src: isTauri() ? convertFileSrc(path) : URL.createObjectURL(file), name: file.name || t('analysis.pasted'), media: 'image' }); }
    catch (e) { setError(analysisError(e)); }
  };
  const paste = (e: ClipboardEvent) => { const f = [...e.clipboardData.files].find(x => x.type.startsWith('image/')); if (f) { e.preventDefault(); void fromFile(f); } };
  const drop = (e: DragEvent) => { e.preventDefault(); setOver(false); void fromFile(e.dataTransfer.files[0]); };
  // Native webviews deliver OS file drops as paths for the whole window, never as DOM files.
  useEffect(() => {
    if (!isTauri() || !active) return;
    let alive = true, unlisten: (() => void) | undefined;
    const inside = (p: { x: number; y: number }) => {
      const r = box.current?.getBoundingClientRect(), x = p.x / devicePixelRatio, y = p.y / devicePixelRatio;
      return !!r && x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
    };
    void getCurrentWebview().onDragDropEvent(e => {
      if (e.payload.type === 'leave') { setOver(false); return; }
      const hit = inside(e.payload.position);
      setOver(e.payload.type !== 'drop' && hit);
      if (e.payload.type !== 'drop' || !hit) return;
      void describeMedia(e.payload.paths.slice(0, 1)).then(entries => {
        const item = entries.map(itemFromEntry).find(Boolean);
        if (alive) { if (item) fromItem(item); else setError(t('analysis.unsupported')); }
      }, err => { if (alive) setError(analysisError(err)); });
    }).then(stop => { if (alive) unlisten = stop; else stop(); }, () => {});
    return () => { alive = false; unlisten?.(); };
  }, [active, key]); // eslint-disable-line react-hooks/exhaustive-deps

  const setup = async () => {
    setError('');
    const started = setupAnalysis(models);
    setTimeout(() => void refresh(), 300);
    try { setOverview(await started); } catch (e) { setError(analysisError(e)); void refresh(); }
  };
  const tags = result ? [...result.character, ...result.general].map(([n]) => n) : [];
  const copy = async () => { try { await navigator.clipboard.writeText(tagsAsPrompt(tags, spaces)); setCopied(true); setTimeout(() => setCopied(false), 1500); } catch { /* the tags are on screen */ } };
  const rating = result ? Object.entries(result.rating).sort((a, b) => b[1] - a[1])[0] : undefined;
  const install = overview?.install;

  return <section className="analysis-tab" aria-label={t('studio.tab.analysis')} onPaste={paste}>
    <div className="forge-card analysis-picture" ref={box}>
      <div className="forge-card-head"><h3 className="forge-card-title">{t('analysis.picture')}</h3>
        <label className="forge-ghost analysis-choose"><input type="file" data-analysis-file accept="image/png,image/jpeg,image/webp" onChange={e => { void fromFile(e.target.files?.[0]); e.target.value = ''; }}/>{t('analysis.choose')}</label>
      </div>
      <div className={`analysis-drop${over ? ' over' : ''}${picture ? ' filled' : ''}`} data-analysis-drop onDragOver={e => { e.preventDefault(); setOver(true); }} onDragLeave={() => setOver(false)} onDrop={drop}>
        {picture?.media === 'image' ? <img src={picture.src} alt={picture.name} data-analysis-image/>
          : picture?.media === 'video' ? <p className="forge-note">{t('analysis.videoSoon')}</p>
          : <div className="forge-empty"><strong>{t('analysis.emptyTitle')}</strong><span>{t('analysis.emptyBody')}</span></div>}
      </div>
      {picture && <p className="forge-note analysis-name" title={picture.path}>{picture.name}</p>}
      {made && made.length > 0 && <details className="analysis-made" data-analysis-made><summary>{t('analysis.madeWith')}</summary>
        <dl>{made.slice(0, 24).map(([k, v], i) => <div key={i}><dt>{k}</dt><dd>{v.length > 600 ? `${v.slice(0, 600)}…` : v}</dd></div>)}</dl>
      </details>}
      {made && made.length === 0 && picture?.media === 'image' && <p className="forge-note">{t('analysis.noMade')}</p>}
    </div>

    <div className="forge-card analysis-results">
      <div className="forge-card-head"><h3 className="forge-card-title">{t('analysis.tags')}</h3></div>
      {!overview ? <p className="forge-note">{t('analysis.loading')}</p>
        : !ready ? <div className="analysis-setup" data-analysis-setup>
          <p>{t('analysis.setupBody')}</p>
          <ul className="forge-note">
            <li>{t(overview.runtimeReady ? 'analysis.setupRuntimeDone' : 'analysis.setupRuntime')}</li>
            {overview.defaultMissing && <li>{t('analysis.setupDownload', { size: gb(overview.defaultSize) })}</li>}
            {!overview.defaultMissing && <li>{t('analysis.setupFound', { names: overview.taggers.map(x => x.name).join(', ') })}</li>}
          </ul>
          {install?.running
            ? <div className="analysis-progress" data-analysis-progress>
              <span>{t(`analysis.stage.${install.stage}`, { done: gb(install.downloaded), total: gb(install.total) })}</span>
              {install.total > 0 && <progress max={install.total} value={install.downloaded}/>}
              <button type="button" className="forge-ghost" onClick={() => void cancelAnalysisSetup()}>{t('analysis.cancel')}</button>
            </div>
            : <button type="button" className="action-button" data-analysis-setup-run disabled={!models.length} onClick={() => void setup()}>{t('analysis.setup')}</button>}
          {!models.length && <p className="forge-note">{t('analysis.noModels')}</p>}
        </div>
        : <>
          <div className="analysis-controls">
            <label className="forge-field"><span>{t('analysis.tagger')}</span>
              <select data-analysis-tagger value={chosen} onChange={e => setTagger(e.target.value)}>
                {taggers.map(x => <option key={x.id} value={x.id}>{x.name}</option>)}
                {taggers.length > 1 && <option value="all">{t('analysis.allTaggers')}</option>}
              </select></label>
            {current && chosen !== 'all' && <>
              <label className="forge-field"><span>{t('analysis.general')}</span><input type="number" data-analysis-general min={0.05} max={1} step={0.01} value={general ?? current.generalThreshold} onChange={e => setGeneral(Number(e.target.value))}/></label>
              <label className="forge-field"><span>{t('analysis.character')}</span><input type="number" data-analysis-character min={0.05} max={1} step={0.01} value={character ?? current.characterThreshold} onChange={e => setCharacter(Number(e.target.value))}/></label>
            </>}
            <button type="button" className="action-button" data-analysis-tag disabled={busy || picture?.media !== 'image'} onClick={() => void tag()}>{t(busy ? 'analysis.tagging' : result ? 'analysis.again' : 'analysis.tag')}</button>
          </div>
          {busy && <p className="chat-status">{t('analysis.reading', { tagger: chosen === 'all' ? t('analysis.allTaggers') : current?.name ?? '' })}</p>}
          {result && <div className="analysis-out" data-analysis-result>
            <p className="forge-note">{result.tagger} · {(result.ms / 1000).toFixed(1)} s{rating ? ` · ${t('analysis.rating')}: ${rating[0]} ${percent(rating[1])}` : ''}</p>
            {result.character.length > 0 && <><span className="forge-note">{t('analysis.characters')}</span>
              <div className="analysis-chips">{result.character.map(([n, p]) => <span key={n} className="forge-chip analysis-chip character" title={percent(p)} data-analysis-character-tag>{n}</span>)}</div></>}
            <span className="forge-note">{t('analysis.generalTags', { count: String(result.general.length) })}</span>
            <div className="analysis-chips">{result.general.map(([n, p]) => <span key={n} className="forge-chip analysis-chip" title={percent(p)}>{n}</span>)}</div>
            <div className="forge-actions analysis-actions">
              <label className="forge-switch"><input type="checkbox" checked={spaces} onChange={e => setSpaces(e.target.checked)}/><span>{t('analysis.spaces')}</span></label>
              <button type="button" className="forge-ghost" data-analysis-copy onClick={() => void copy()}>{t(copied ? 'forge.copied' : 'analysis.copy')}</button>
              <button type="button" className="forge-ghost" data-analysis-send onClick={() => onSend(tagsAsPrompt(tags, spaces), '')}>{t('chat.toGenerate')}</button>
            </div>
          </div>}
        </>}
      {error && <p className="error-message" role="alert">{error}</p>}
    </div>
  </section>;
}
