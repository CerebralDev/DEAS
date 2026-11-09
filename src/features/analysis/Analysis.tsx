import { useContext, useEffect, useRef, useState, type ClipboardEvent, type DragEvent } from 'react';
import { convertFileSrc, isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { useI18n } from '../../i18n/I18nProvider';
import { analysisError, analysisOverview, cancelAnalysisSetup, cancelDescribe, convertTagger, describePicture, modelFolders, pickMedia, setupAnalysis, stagePicture, tagPicture, tagsAsPrompt, type AnalysisOverview, type Reading, type TagResult } from '../../lib/analysis';
import { embeddedWorkflow, OpenComfyWorkflow, type ComfyWorkflow } from '../../lib/comfyWorkflow';
import { nativeModelId } from '../../lib/forge';
import { imageMetadataRows, readImageMetadata, type ImageMetadata } from '../../lib/imageMetadata';
import { describeMedia, itemFromEntry, type MediaItem } from '../../lib/media';
import { isBoolean, isString, usePref } from '../../lib/preferences';
import { hasLlmConfiguration } from '../../lib/llm';
import { madeSummary, type ComfyMeta } from '../../lib/comfyMeta';
import { useForgeModels } from '../forge/ForgeButton';
import { prepare } from '../forge/ReferenceImages';
import type { MediaAction } from '../media/MediaLibrary';
import { FramePicker } from './FramePicker';
import './analysis.css';

export interface AnalysisPicture {
  path: string; src: string; name: string; media: 'image' | 'video';
  /** A video: the frame to open the picker on. */
  start?: number;
  /** A frame taken from a video: the video and the frame, for "Back to the video". */
  fromVideo?: { path: string; src: string; name: string; frame: number };
}
export interface AnalysisProps {
  active?: boolean;
  /** The ComfyUI backend, for the LLM model list Describe picks from. */
  endpoint?: string;
  roots?: { models?: string; extraModels?: string[] };
  root?: string;
  /** A picture or video sent from the Media dock ("Open in Media Analysis"); `n` changes per send. */
  incoming?: { item: MediaItem; n: number };
  onSend: (positive: string, negative: string) => void;
  /** What the Generate tab would do with a picture now: the frame picker's send buttons. */
  frameActions?: (item: MediaItem) => MediaAction[];
}

const gb = (bytes: number) => (bytes / 1024 ** 3).toFixed(1);
const isNumber = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const percent = (p: number) => `${Math.round(p * 100)}%`;
const IMAGE = /\.(png|jpe?g|webp)$/i;
const VIDEO = /\.(mp4|mov|m4v|mkv|webm)$/i;

/** The picture as the vision model takes it: scaled down when large, as Prompt Forge sends pictures. */
async function dataUri(picture: AnalysisPicture): Promise<string> {
  const response = await fetch(picture.src);
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const blob = await response.blob();
  const type = blob.type.startsWith('image/') ? blob.type : IMAGE.test(picture.name) ? `image/${picture.name.split('.').pop()!.toLowerCase().replace('jpg', 'jpeg')}` : 'image/png';
  return prepare(new File([blob], picture.name, { type }));
}

/** Media Analysis: what is in a picture, as tags and in words (and the settings that made it); videos open in the frame picker. Ported from PromptForge. */
export function Analysis({ active = true, endpoint = '', roots, root, incoming, onSend, frameActions }: AnalysisProps) {
  const { t } = useI18n();
  const models = modelFolders(roots?.models, roots?.extraModels, root);
  const key = models.join('\n');
  const [overview, setOverview] = useState<AnalysisOverview | null>(null);
  const [tagger, setTagger] = usePref('analysis.tagger', '', isString);
  const [spaces, setSpaces] = usePref('analysis.spaces', true, isBoolean);
  const [general, setGeneral] = useState<number | null>(null);
  const [character, setCharacter] = useState<number | null>(null);
  const [picture, setPicture] = useState<AnalysisPicture | null>(null);
  // Frames picked per video, kept while the tab is open so a trip to one frame and back keeps them.
  const picks = useRef(new Map<string, number[]>());
  const [result, setResult] = useState<TagResult | null>(null);
  const [added, setAdded] = useState<string[]>([]);
  const [made, setMade] = useState<ImageMetadata | null>(null);
  const [summary, setSummary] = useState<ComfyMeta | null>(null);
  const [workflow, setWorkflow] = useState<ComfyWorkflow | null>(null);
  const [opening, setOpening] = useState(false);
  const [openError, setOpenError] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [over, setOver] = useState(false);
  const [copied, setCopied] = useState('');
  const openWorkflow = useContext(OpenComfyWorkflow);
  // Describe: the configured LLM, its own pick (not the one that writes prompts).
  const forge = useForgeModels(endpoint, root, active);
  const [describeModel, setDescribeModel] = usePref('analysis.describeModel', '', isString);
  // PromptForge's defaults: 0.2 and no penalty (a reading task); its hint says when to raise them.
  const [temperature, setTemperature] = usePref('analysis.describeTemperature', 0.2, isNumber);
  const [penalty, setPenalty] = usePref('analysis.describePenalty', 0, isNumber);
  const [reading, setReading] = useState<Reading | null>(null);
  const [streamed, setStreamed] = useState('');
  const [describing, setDescribing] = useState<string | null>(null);
  const [castShown, setCastShown] = useState<[string, number][]>([]);
  const [describeError, setDescribeError] = useState('');
  const [wantDescribe, setWantDescribe] = useState(false);
  const [pickFirst, setPickFirst] = useState(false);
  // The checkpoint folder being converted to ONNX, and how the last conversion ended.
  const [converting, setConverting] = useState('');
  const [converted, setConverted] = useState<{ folder: string; note: string; error: boolean } | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const run = useRef(0);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  const refresh = () => analysisOverview(models).then(setOverview, e => setError(analysisError(e)));
  useEffect(() => { if (active) void refresh(); }, [active, key]); // eslint-disable-line react-hooks/exhaustive-deps
  // While setup runs and the tab is shown, its progress is read once a second without a folder
  // scan; the full look follows once it has finished.
  useEffect(() => {
    if (!active || !overview?.install.running) return;
    const timer = setInterval(() => void analysisOverview(models, false).then(o => {
      if (!o.install.running) void refresh();
      else setOverview(p => p && { ...p, install: o.install, runtimeReady: o.runtimeReady });
    }, () => {}), 1000);
    return () => clearInterval(timer);
  }, [active, overview?.install.running]); // eslint-disable-line react-hooks/exhaustive-deps

  const taggers = overview?.taggers ?? [];
  const unusable = overview?.unusable ?? [];
  const chosen = tagger === 'all' && taggers.length > 1 ? 'all' : (taggers.find(x => x.id === tagger) ?? taggers[0])?.id ?? '';
  const current = taggers.find(x => x.id === chosen);
  const ready = !!overview?.runtimeReady && taggers.length > 0;
  // A different tagger starts from its own thresholds (WD's 0.35/0.85 hid animetimm characters).
  useEffect(() => { setGeneral(null); setCharacter(null); }, [chosen]);

  const llmModels = forge.usable;
  const llmModel = llmModels.some(m => m.id === describeModel) ? describeModel : forge.model;

  const tag = async (pic = picture) => {
    if (!pic || pic.media !== 'image' || !ready || !chosen) return;
    const mine = ++run.current;
    setBusy(true); setError(''); setResult(null); setAdded([]);
    try {
      const out = await tagPicture(models, chosen, pic.path, chosen === 'all' ? undefined : general ?? undefined, chosen === 'all' ? undefined : character ?? undefined);
      if (mine === run.current) setResult(out);
    } catch (e) { if (mine === run.current) setError(analysisError(e)); }
    finally { if (mine === run.current) setBusy(false); }
  };
  const stopDescribe = () => { if (describing) void cancelDescribe(describing); };
  const load = (pic: AnalysisPicture) => {
    run.current++;
    stopDescribe();
    setPicture(pic); setResult(null); setAdded([]); setMade(null); setSummary(null); setWorkflow(null); setOpenError(''); setError(''); setBusy(false);
    setReading(null); setStreamed(''); setDescribeError(''); setCastShown([]);
    if (pic.media === 'image') {
      readImageMetadata(pic.path).then(entries => { setMade(imageMetadataRows(entries)); setSummary(madeSummary(entries)); setWorkflow(embeddedWorkflow(entries)); }, () => setMade([]));
      void tag(pic);
    }
  };
  const fromItem = (item: MediaItem) => item.path && (item.media === 'image' || item.media === 'video')
    ? load({ path: item.path, src: item.src, name: item.name, media: item.media }) : setError(t('analysis.unsupported'));
  useEffect(() => { if (incoming) fromItem(incoming.item); }, [incoming?.n]); // eslint-disable-line react-hooks/exhaustive-deps
  // Tags as soon as the runtime is ready for a picture that arrived before it was.
  useEffect(() => { if (ready && picture && !result && !busy && !error) void tag(); }, [ready]); // eslint-disable-line react-hooks/exhaustive-deps

  const fromPath = async (path: string) => {
    try {
      const item = (await describeMedia([path])).map(itemFromEntry).find(Boolean);
      if (item) fromItem(item); else setError(t('analysis.unsupported'));
    } catch (e) { setError(analysisError(e)); }
  };
  const fromFile = async (file?: File) => {
    if (!file) return;
    if (!/^image\/(png|jpeg|webp)$/.test(file.type)) { setError(t(file.type.startsWith('video/') ? 'analysis.videoByPath' : 'analysis.unsupported')); return; }
    try { const path = await stagePicture(file); load({ path, src: isTauri() ? convertFileSrc(path) : URL.createObjectURL(file), name: file.name || t('analysis.pasted'), media: 'image' }); }
    catch (e) { setError(analysisError(e)); }
  };
  const choose = async () => {
    setError('');
    try { const path = await pickMedia(t('analysis.chooseTitle')); if (path) await fromPath(path); }
    catch (e) { setError(analysisError(e)); }
  };
  const paste = (e: ClipboardEvent) => {
    const files = [...e.clipboardData.files];
    const f = files.find(x => x.type.startsWith('image/')) ?? files.find(x => x.type.startsWith('video/'));
    if (f) { e.preventDefault(); void fromFile(f); }
  };
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
      const path = e.payload.paths[0];
      if (alive && path && (IMAGE.test(path) || VIDEO.test(path))) void fromPath(path);
      else if (alive) setError(t('analysis.unsupported'));
    }).then(stop => { if (alive) unlisten = stop; else stop(); }, () => {});
    return () => { alive = false; unlisten?.(); };
  }, [active, key]); // eslint-disable-line react-hooks/exhaustive-deps

  const setup = async (download = false) => {
    setError('');
    const started = setupAnalysis(models, download);
    setTimeout(() => { if (mounted.current) void refresh(); }, 300);
    // A cancel is said calmly under the setup (analysis.cancelled), not as an error.
    try { setOverview(await started); } catch (e) { if (!String(e).includes('ANALYSIS_CANCELLED')) setError(analysisError(e)); void refresh(); }
  };
  const convert = async (folder: string, name: string) => {
    setConverting(folder); setConverted(null);
    const started = convertTagger(models, folder);
    setTimeout(() => { if (mounted.current) void refresh(); }, 300);
    try { setOverview(await started); setConverted({ folder, note: t('analysis.convert.done', { name }), error: false }); }
    catch (e) {
      // A cancel is said calmly; anything else says what went wrong and what to do.
      setConverted({ folder, note: String(e).includes('ANALYSIS_CANCELLED') ? t('analysis.convert.cancelled') : analysisError(e), error: !String(e).includes('ANALYSIS_CANCELLED') });
      void refresh();
    } finally { setConverting(''); }
  };
  const tagNames = result ? [...result.character, ...result.general].map(([n]) => n) : [];
  const tags = [...tagNames, ...added.filter(a => !tagNames.includes(a))];
  const copy = async (what: string, text: string) => { try { await navigator.clipboard.writeText(text); setCopied(what); setTimeout(() => setCopied(''), 1500); } catch { /* the text is on screen */ } };
  const rating = result ? Object.entries(result.rating).sort((a, b) => b[1] - a[1])[0] : undefined;
  const install = overview?.install;

  const openInComfy = async () => {
    if (!openWorkflow || !workflow || !picture) return;
    setOpening(true); setOpenError('');
    try { await openWorkflow(workflow, picture.name); } catch { setOpenError(t('analysis.comfyFailed')); } finally { setOpening(false); }
  };
  const comfyBlocked = !workflow ? t('analysis.noWorkflow') : !openWorkflow ? t('analysis.comfyOff') : '';

  const describe = async () => {
    if (!picture || picture.media !== 'image' || describing) return;
    if (!root) { setDescribeError(t('analysis.describe.noEnv')); return; }
    if (!llmModel) { setWantDescribe(true); forge.load(); return; }
    const id = crypto.randomUUID();
    const mine = run.current;
    setDescribing(id); setReading(null); setStreamed(''); setDescribeError(''); setCastShown([]); setPickFirst(false);
    try {
      const image = await dataUri(picture);
      const out = await describePicture(root, nativeModelId(llmModel), image, tagNames, { temperature, presencePenalty: penalty }, id, e => {
        if (mine !== run.current) return;
        if (e.type === 'started') setCastShown(e.cast);
        else setStreamed(s => s + e.text);
      });
      if (mine === run.current) setReading(out);
    } catch (e) { if (mine === run.current) setDescribeError(analysisError(e)); }
    finally { setDescribing(d => d === id ? null : d); }
  };
  // The model list is there before anyone presses Describe, so the model can be picked first. Only
  // when an LLM is set up: opening a picture must not pop the LLM setup at someone who only tags.
  useEffect(() => {
    if (!active || picture?.media !== 'image' || !root || forge.list) return;
    let alive = true;
    hasLlmConfiguration(root).then(ok => { if (alive && ok) forge.load(); }, () => {});
    return () => { alive = false; };
  }, [active, picture?.path, root, forge.list]); // eslint-disable-line react-hooks/exhaustive-deps
  // Describe pressed before the list was there: it lists the models and stops, so the run is
  // always on the model shown in the list, never one nobody saw.
  useEffect(() => {
    if (wantDescribe && (llmModel || forge.error || (forge.list && !llmModels.length))) { setWantDescribe(false); setPickFirst(!!llmModel); }
  }, [wantDescribe, llmModel, forge.error, forge.list, llmModels.length]);
  // Only what is new: real Danbooru tags it suggested, and the characters it named, that the list does not have yet.
  const addable = reading ? [...new Set([...reading.characters, ...reading.suggested.filter(s => !reading.unknown.includes(s))])].filter(t => !tags.includes(t)) : [];
  const addSuggested = () => setAdded(a => [...new Set([...a, ...addable])]);

  const convertedNote = converted && !converted.error && !unusable.some(u => u.convert?.folder === converted.folder) && <p className="forge-note" role="status" data-analysis-converted>{converted.note}</p>;
  const unusableList = unusable.length > 0 && <details className="analysis-unusable" data-analysis-unusable open={!taggers.length}>
    <summary>{t('analysis.unusable', { count: String(unusable.length) })}</summary>
    <ul>{unusable.map((u, i) => <li key={i}><strong>{u.name}</strong> <span className="forge-note">{u.place}</span><br/>{u.reason}
      {u.convert && <div className="analysis-convert" data-analysis-convert={u.convert.folder}>
        {converting === u.convert.folder && install?.running
          ? <div className="analysis-progress" data-analysis-converting>
            <span>{t(`analysis.stage.${install.stage}`, { done: '', total: '' })}</span>
            <button type="button" className="forge-ghost" data-analysis-convert-cancel onClick={() => void cancelAnalysisSetup()}>{t('analysis.cancel')}</button>
          </div>
          : <>
            <p className="forge-note">{t('analysis.convert.note', { tools: gb(overview?.toolsSize ?? 0), size: gb(u.convert.bytes) })}</p>
            <button type="button" className="forge-ghost" data-analysis-convert-run disabled={!!converting || !!install?.running} onClick={() => void convert(u.convert!.folder, u.name)}>{t('analysis.convert.run')}</button>
          </>}
        {converted?.folder === u.convert.folder && converting !== u.convert.folder && <p className={converted.error ? 'error-message' : 'forge-note'} role={converted.error ? 'alert' : 'status'}>{converted.note}</p>}
      </div>}
    </li>)}</ul>
  </details>;

  return <section className="analysis-tab" aria-label={t('studio.tab.analysis')} onPaste={paste}>
    <div className="forge-card analysis-picture" ref={box}>
      <div className="forge-card-head"><h3 className="forge-card-title">{t('analysis.picture')}</h3>
        {isTauri()
          ? <button type="button" className="forge-ghost" data-analysis-choose onClick={() => void choose()}>{t('analysis.choose')}</button>
          : <label className="forge-ghost analysis-choose"><input type="file" data-analysis-file accept="image/png,image/jpeg,image/webp" onChange={e => { void fromFile(e.target.files?.[0]); e.target.value = ''; }}/>{t('analysis.choose')}</label>}
      </div>
      {picture?.media === 'video'
        ? <FramePicker key={picture.path} video={{ path: picture.path, name: picture.name }} start={picture.start} startPicked={picks.current.get(picture.path)}
          onPicked={list => picks.current.set(picture.path, list)} frameActions={frameActions} onClose={() => setPicture(null)}
          onUse={(frame, index) => load({ path: frame.path!, src: frame.src, name: frame.name, media: 'image', fromVideo: { path: picture.path, src: picture.src, name: picture.name, frame: index } })}/>
        : <div className={`analysis-drop${over ? ' over' : ''}${picture ? ' filled' : ''}`} data-analysis-drop onDragOver={e => { e.preventDefault(); setOver(true); }} onDragLeave={() => setOver(false)} onDrop={drop}>
          {picture?.media === 'image' ? <img src={picture.src} alt={picture.name} data-analysis-image/>
            : <div className="forge-empty"><strong>{t('analysis.emptyTitle')}</strong><span>{t('analysis.emptyBody')}</span></div>}
        </div>}
      {picture?.media === 'image' && <p className="forge-note analysis-name" title={picture.name}>{picture.name}</p>}
      {picture?.media === 'image' && picture.fromVideo && <button type="button" className="forge-ghost" data-analysis-back-to-video
        onClick={() => { const v = picture.fromVideo!; load({ path: v.path, src: v.src, name: v.name, media: 'video', start: v.frame }); }}>
        {t('analysis.frames.back', { name: picture.fromVideo.name, frame: String(picture.fromVideo.frame + 1) })}</button>}
      {picture?.media === 'image' && made && <div className="analysis-made-head">
        <button type="button" className="forge-ghost" data-analysis-open-comfy disabled={!!comfyBlocked || opening} title={comfyBlocked || t('analysis.openComfyHelp')} onClick={() => void openInComfy()}>{t(opening ? 'analysis.openingComfy' : 'analysis.openComfy')}</button>
        {comfyBlocked && <span className="forge-note" data-analysis-comfy-reason>{comfyBlocked}</span>}
        {openError && <span className="error-message" role="alert">{openError}</span>}
      </div>}
      {made && made.length > 0 && <div className="analysis-made" data-analysis-made>
        <span className="forge-note analysis-made-title">{t('analysis.madeWith')}</span>
        {summary && <>
          <dl className="analysis-made-summary" data-analysis-made-summary>
            {([
              ['analysis.made.checkpoint', summary.checkpoints.join(', ')],
              ['analysis.made.loras', summary.loras.map(l => l.model != null ? `${l.name} @ ${l.model}` : l.name).join(', ')],
              ['analysis.made.seed', summary.seed ?? ''],
              ['analysis.made.sampler', [summary.sampler, summary.settings.sampler_name, summary.settings.scheduler].filter(Boolean).join(' · ')],
              ['analysis.made.effort', [summary.settings.steps, summary.settings.cfg, summary.settings.denoise].filter(v => v !== undefined).join(' / ')],
            ] as const).filter(([, v]) => v).map(([k, v]) => <div key={k}><dt>{t(k)}</dt><dd>{v}</dd></div>)}
          </dl>
          {summary.prompt && <p className="analysis-made-prompt" data-analysis-made-prompt><strong>{t('analysis.made.prompt')}:</strong> {summary.prompt}{summary.promptPartial && <span className="forge-note"> {t('analysis.made.partial')}</span>}</p>}
          {summary.negative && <p className="analysis-made-prompt"><strong>{t('analysis.made.negative')}:</strong> {summary.negative}{summary.negativePartial && <span className="forge-note"> {t('analysis.made.partial')}</span>}</p>}
          {summary.prompt && <div className="forge-actions">
            <button type="button" className="forge-ghost" data-analysis-made-copy onClick={() => void copy('prompt', summary.prompt!)}>{t(copied === 'prompt' ? 'forge.copied' : 'analysis.made.copy')}</button>
            <button type="button" className="forge-ghost" data-analysis-made-send onClick={() => onSend(summary.prompt!, summary.negative ?? '')}>{t('analysis.made.send')}</button>
          </div>}
        </>}
        <details className="analysis-made-all"><summary>{t(summary ? 'analysis.made.all' : 'analysis.madeWith')}</summary>
          <dl>{made.slice(0, 24).map(([k, v], i) => <div key={i}><dt>{k}</dt><dd>{v.length > 600 ? `${v.slice(0, 600)}…` : v}</dd></div>)}</dl>
        </details>
      </div>}
      {made && made.length === 0 && picture?.media === 'image' && <p className="forge-note" data-analysis-no-made>{t('analysis.noMade')}</p>}
    </div>

    <div className="forge-card analysis-results">
      <div className="forge-card-head"><h3 className="forge-card-title">{t('analysis.tags')}</h3></div>
      {!overview ? <p className="forge-note">{t('analysis.loading')}</p>
        : !ready ? <div className="analysis-setup" data-analysis-setup>
          <p>{t('analysis.setupBody')}</p>
          {taggers.length > 0
            ? <ul className="forge-note">
              <li>{t('analysis.setupFound', { names: taggers.map(x => x.name).join(', ') })}</li>
              <li>{t('analysis.setupRuntime')}</li>
            </ul>
            : <>
              <p data-analysis-none><strong>{t('analysis.noneFound')}</strong></p>
              <p className="forge-note">{t('analysis.lookedIn')}</p>
              <p className="forge-note">{t('analysis.downloadOffer', { size: gb(overview.defaultSize) })}</p>
            </>}
          {unusableList}{convertedNote}
          {install?.running && !converting
            ? <div className="analysis-progress" data-analysis-progress>
              <span>{t(`analysis.stage.${install.stage}`, { done: gb(install.downloaded), total: gb(install.total) })}</span>
              {install.total > 0 && <progress max={install.total} value={install.downloaded}/>}
              <button type="button" className="forge-ghost" data-analysis-cancel onClick={() => void cancelAnalysisSetup()}>{t('analysis.cancel')}</button>
            </div>
            : install?.running ? null : <div className="forge-actions">
              {taggers.length > 0
                ? <button type="button" className="action-button" data-analysis-setup-run onClick={() => void setup()}>{t('analysis.setup')}</button>
                : <button type="button" className="action-button" data-analysis-download disabled={!models.length} onClick={() => void setup(true)}>{t('analysis.download', { size: gb(overview.defaultSize + (overview.runtimeReady ? 0 : overview.runtimeSize)) })}</button>}
              <button type="button" className="forge-ghost" data-analysis-rescan onClick={() => void refresh()}>{t('analysis.rescan')}</button>
            </div>}
          {install && !install.running && install.stage === 'cancelled' && <p className="forge-note" role="status">{t('analysis.cancelled')}</p>}
          {!models.length && <p className="forge-note">{t('analysis.noModels')}</p>}
        </div>
        : <>
          <div className="analysis-controls">
            <label className="forge-field"><span>{t('analysis.tagger')}</span>
              <select data-analysis-tagger value={chosen} onChange={e => setTagger(e.target.value)}>
                {taggers.map(x => <option key={x.id} value={x.id} title={x.place}>{x.name}</option>)}
                {taggers.length > 1 && <option value="all">{t('analysis.allTaggers')}</option>}
              </select></label>
            {current && chosen !== 'all' && <>
              <label className="forge-field"><span>{t('analysis.general')}</span><input type="number" data-analysis-general min={0.05} max={1} step={0.01} value={general ?? current.generalThreshold} onChange={e => setGeneral(Number(e.target.value))}/></label>
              <label className="forge-field"><span>{t('analysis.character')}</span><input type="number" data-analysis-character min={0.05} max={1} step={0.01} value={character ?? current.characterThreshold} onChange={e => setCharacter(Number(e.target.value))}/></label>
            </>}
            <button type="button" className="action-button" data-analysis-tag disabled={busy || picture?.media !== 'image'} onClick={() => void tag()}>{t(busy ? 'analysis.tagging' : result ? 'analysis.again' : 'analysis.tag')}</button>
          </div>
          {current && chosen !== 'all' && <p className="forge-note analysis-where" data-analysis-where>{t(current.categories ? 'analysis.taggerFrom' : 'analysis.taggerFromNoCategories', { place: current.place, count: String(current.tags) })}</p>}
          {unusableList}{convertedNote}
          {busy && <p className="chat-status">{t('analysis.reading', { tagger: chosen === 'all' ? t('analysis.allTaggers') : current?.name ?? '' })}</p>}
          {result && <div className="analysis-out" data-analysis-result>
            <p className="forge-note">{t('analysis.took', { tagger: result.tagger, seconds: (result.ms / 1000).toFixed(1) })}{rating ? ` · ${t('analysis.rating')}: ${rating[0]} ${percent(rating[1])}` : ''}</p>
            {result.character.length > 0 && <><span className="forge-note">{t('analysis.characters')}</span>
              <div className="analysis-chips">{result.character.map(([n, p]) => <span key={n} className="forge-chip analysis-chip character" title={percent(p)} data-analysis-character-tag>{n}</span>)}</div></>}
            <span className="forge-note">{t('analysis.generalTags', { count: String(result.general.length) })}</span>
            <div className="analysis-chips">{result.general.map(([n, p]) => <span key={n} className="forge-chip analysis-chip" title={percent(p)}>{n}</span>)}</div>
            {added.length > 0 && <><span className="forge-note">{t('analysis.addedTags')}</span>
              <div className="analysis-chips" data-analysis-added>{added.map(n => <span key={n} className="forge-chip analysis-chip added">{n}</span>)}</div></>}
            <div className="forge-actions analysis-actions">
              <label className="forge-switch"><input type="checkbox" data-analysis-spaces checked={spaces} onChange={e => setSpaces(e.target.checked)}/><span>{t('analysis.spaces')}</span></label>
              <button type="button" className="forge-ghost" data-analysis-copy onClick={() => void copy('tags', tagsAsPrompt(tags, spaces))}>{t(copied === 'tags' ? 'forge.copied' : 'analysis.copy')}</button>
              <button type="button" className="forge-ghost" data-analysis-send onClick={() => onSend(tagsAsPrompt(tags, spaces), '')}>{t('chat.toGenerate')}</button>
            </div>
          </div>}
        </>}
      {error && <p className="error-message" role="alert">{error}</p>}

      {picture?.media === 'image' && <div className="analysis-describe" data-analysis-describe>
        <div className="forge-card-head"><h3 className="forge-card-title">{t('analysis.describe.title')}</h3></div>
        <p className="forge-note">{t('analysis.describe.body')}</p>
        <div className="analysis-controls">
          <label className="forge-field"><span>{t('forge.model')}</span>
            <select data-analysis-describe-model value={llmModel} onMouseDown={() => { if (!forge.list) forge.load(); }} onFocus={() => { if (!forge.list) forge.load(); }} onChange={e => { setDescribeModel(e.target.value); setPickFirst(false); }}>
              {llmModels.length ? llmModels.map(m => <option key={m.id} value={m.id}>{m.label}</option>) : <option value="">{t(forge.list ? 'forge.noModels' : 'analysis.describe.modelsOnClick')}</option>}
            </select></label>
          {describing
            ? <button type="button" className="forge-ghost" data-analysis-describe-stop onClick={stopDescribe}>{t('analysis.describe.stop')}</button>
            : <button type="button" className="action-button" data-analysis-describe-run disabled={busy || wantDescribe} onClick={() => void describe()}>{t(wantDescribe ? 'analysis.describe.loading' : reading ? 'analysis.describe.again' : 'analysis.describe.run')}</button>}
        </div>
        <div className="analysis-sampling" data-analysis-sampling>
          <label className="forge-field"><span>{t('analysis.describe.temperature')} <output data-analysis-temperature-value>{temperature.toFixed(2)}</output></span>
            <input type="range" min={0} max={1.5} step={0.05} value={temperature} data-analysis-temperature aria-label={t('analysis.describe.temperature')} onChange={e => setTemperature(Number(e.target.value))}/></label>
          <label className="forge-field"><span>{t('analysis.describe.penalty')} <output data-analysis-penalty-value>{penalty.toFixed(1)}</output></span>
            <input type="range" min={0} max={2} step={0.1} value={penalty} data-analysis-penalty aria-label={t('analysis.describe.penalty')} onChange={e => setPenalty(Number(e.target.value))}/></label>
        </div>
        <p className="forge-note">{t('analysis.describe.samplingHint')}</p>
        {pickFirst && <p className="forge-note" role="status" data-analysis-pick-first>{t('analysis.describe.pickFirst')}</p>}
        {!result && !busy && ready && <p className="forge-note">{t('analysis.describe.noTags')}</p>}
        {forge.error && <p className="error-message" role="alert">{t(forge.error)}</p>}
        {describing && <p className="chat-status" role="status">{castShown.length ? t('analysis.describe.readingCast', { series: castShown.map(([s, n]) => `${s} (${n})`).join(', ') }) : t('analysis.describe.reading')}</p>}
        {describing && streamed && <pre className="analysis-stream" data-analysis-stream>{streamed}</pre>}
        {reading && <div className="analysis-reading" data-analysis-reading>
          {reading.stopped && <p className="forge-note">{t('analysis.describe.stopped')}</p>}
          <span className="forge-note">{t('analysis.describe.description')}</span>
          <p className="analysis-description" data-analysis-description>{reading.description}</p>
          <span className="forge-note">{t('analysis.describe.characters')}</span>
          {reading.characters.length
            ? <div className="analysis-chips" data-analysis-named>{reading.characters.map(c => <span key={c} className="forge-chip analysis-chip character">{c}</span>)}</div>
            : <p className="forge-note">{t('analysis.describe.nobody')}</p>}
          {reading.cast.length > 0 && <p className="forge-note">{t('analysis.describe.castNote', { series: reading.cast.map(([s, n]) => `${s} (${n})`).join(', ') })}</p>}
          <span className="forge-note">{t('analysis.describe.missed')}</span>
          {reading.suggested.length
            ? <div className="analysis-chips" data-analysis-missed>{reading.suggested.map(s => reading.unknown.includes(s)
              ? <span key={s} className="forge-chip analysis-chip unknown" title={t('analysis.describe.unknownHelp')} data-analysis-unknown>{s} ?</span>
              : <span key={s} className="forge-chip analysis-chip">{s}</span>)}</div>
            : <p className="forge-note">{t('analysis.describe.nothing')}</p>}
          {reading.unknown.length > 0 && <p className="forge-note">{t('analysis.describe.unknownNote', { count: String(reading.unknown.length) })}</p>}
          <span className="forge-note">{t('analysis.describe.doubtful')}</span>
          <p className="analysis-description" data-analysis-doubtful>{reading.doubtful || t('analysis.describe.nothing')}</p>
          <div className="forge-actions analysis-actions">
            <button type="button" className="forge-ghost" data-analysis-copy-description onClick={() => void copy('description', reading.description)}>{t(copied === 'description' ? 'forge.copied' : 'analysis.describe.copy')}</button>
            <button type="button" className="forge-ghost" data-analysis-add-suggested disabled={!result || !addable.length} onClick={addSuggested}>{t('analysis.describe.add')}</button>
          </div>
        </div>}
        {describeError && <p className="error-message" role="alert" data-analysis-describe-error>{describeError}</p>}
      </div>}
    </div>
  </section>;
}
