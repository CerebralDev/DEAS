import { useContext, useEffect, useRef, useState, type ClipboardEvent, type DragEvent } from 'react';
import { convertFileSrc, isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { useI18n } from '../../i18n/I18nProvider';
import { analysisError, analysisOverview, cancelAnalysisSetup, cancelDescribe, describePicture, modelFolders, pickMedia, setupAnalysis, stagePicture, tagPicture, tagsAsPrompt, type AnalysisOverview, type Reading, type TagResult } from '../../lib/analysis';
import { embeddedWorkflow, OpenComfyWorkflow, type ComfyWorkflow } from '../../lib/comfyWorkflow';
import { nativeModelId } from '../../lib/forge';
import { imageMetadataRows, readImageMetadata, type ImageMetadata } from '../../lib/imageMetadata';
import { describeMedia, itemFromEntry, type MediaItem } from '../../lib/media';
import { isBoolean, isString, usePref } from '../../lib/preferences';
import { useForgeModels } from '../forge/ForgeButton';
import { prepare } from '../forge/ReferenceImages';
import type { MediaAction } from '../media/MediaLibrary';
import { FramePicker } from './FramePicker';
import './analysis.css';

export interface AnalysisPicture { path: string; src: string; name: string; media: 'image' | 'video' }
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
  const [result, setResult] = useState<TagResult | null>(null);
  const [added, setAdded] = useState<string[]>([]);
  const [made, setMade] = useState<ImageMetadata | null>(null);
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
  const [reading, setReading] = useState<Reading | null>(null);
  const [streamed, setStreamed] = useState('');
  const [describing, setDescribing] = useState<string | null>(null);
  const [castShown, setCastShown] = useState<[string, number][]>([]);
  const [describeError, setDescribeError] = useState('');
  const [wantDescribe, setWantDescribe] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const run = useRef(0);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  const refresh = () => analysisOverview(models).then(setOverview, e => setError(analysisError(e)));
  useEffect(() => { if (active) void refresh(); }, [active, key]); // eslint-disable-line react-hooks/exhaustive-deps
  // While setup runs, its progress is read once a second.
  useEffect(() => {
    if (!overview?.install.running) return;
    const timer = setInterval(() => void refresh(), 1000);
    return () => clearInterval(timer);
  }, [overview?.install.running]); // eslint-disable-line react-hooks/exhaustive-deps

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
    setPicture(pic); setResult(null); setAdded([]); setMade(null); setWorkflow(null); setOpenError(''); setError(''); setBusy(false);
    setReading(null); setStreamed(''); setDescribeError(''); setCastShown([]);
    if (pic.media === 'image') {
      readImageMetadata(pic.path).then(entries => { setMade(imageMetadataRows(entries)); setWorkflow(embeddedWorkflow(entries)); }, () => setMade([]));
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
    setDescribing(id); setReading(null); setStreamed(''); setDescribeError(''); setCastShown([]);
    try {
      const image = await dataUri(picture);
      const out = await describePicture(root, nativeModelId(llmModel), image, tagNames, id, e => {
        if (mine !== run.current) return;
        if (e.type === 'started') setCastShown(e.cast);
        else setStreamed(s => s + e.text);
      });
      if (mine === run.current) setReading(out);
    } catch (e) { if (mine === run.current) setDescribeError(analysisError(e)); }
    finally { setDescribing(d => d === id ? null : d); }
  };
  // The model list loads on the first Describe (it may ask for an LLM setup); then it runs.
  useEffect(() => { if (wantDescribe && llmModel) { setWantDescribe(false); void describe(); } }, [wantDescribe, llmModel]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => { if (wantDescribe && (forge.error || (forge.list && !llmModels.length))) setWantDescribe(false); }, [wantDescribe, forge.error, forge.list, llmModels.length]);
  // Only what is new: real Danbooru tags it suggested, and the characters it named, that the list does not have yet.
  const addable = reading ? [...new Set([...reading.characters, ...reading.suggested.filter(s => !reading.unknown.includes(s))])].filter(t => !tags.includes(t)) : [];
  const addSuggested = () => setAdded(a => [...new Set([...a, ...addable])]);

  const unusableList = unusable.length > 0 && <details className="analysis-unusable" data-analysis-unusable open={!taggers.length}>
    <summary>{t('analysis.unusable', { count: String(unusable.length) })}</summary>
    <ul>{unusable.map((u, i) => <li key={i}><strong>{u.name}</strong> <span className="forge-note">{u.place}</span><br/>{u.reason}</li>)}</ul>
  </details>;

  return <section className="analysis-tab" aria-label={t('studio.tab.analysis')} onPaste={paste}>
    <div className="forge-card analysis-picture" ref={box}>
      <div className="forge-card-head"><h3 className="forge-card-title">{t('analysis.picture')}</h3>
        {isTauri()
          ? <button type="button" className="forge-ghost" data-analysis-choose onClick={() => void choose()}>{t('analysis.choose')}</button>
          : <label className="forge-ghost analysis-choose"><input type="file" data-analysis-file accept="image/png,image/jpeg,image/webp" onChange={e => { void fromFile(e.target.files?.[0]); e.target.value = ''; }}/>{t('analysis.choose')}</label>}
      </div>
      {picture?.media === 'video'
        ? <FramePicker video={{ path: picture.path, name: picture.name }} frameActions={frameActions} onClose={() => setPicture(null)}
          onUse={frame => load({ path: frame.path!, src: frame.src, name: frame.name, media: 'image' })}/>
        : <div className={`analysis-drop${over ? ' over' : ''}${picture ? ' filled' : ''}`} data-analysis-drop onDragOver={e => { e.preventDefault(); setOver(true); }} onDragLeave={() => setOver(false)} onDrop={drop}>
          {picture?.media === 'image' ? <img src={picture.src} alt={picture.name} data-analysis-image/>
            : <div className="forge-empty"><strong>{t('analysis.emptyTitle')}</strong><span>{t('analysis.emptyBody')}</span></div>}
        </div>}
      {picture?.media === 'image' && <p className="forge-note analysis-name" title={picture.name}>{picture.name}</p>}
      {picture?.media === 'image' && made && <div className="analysis-made-head">
        <button type="button" className="forge-ghost" data-analysis-open-comfy disabled={!!comfyBlocked || opening} title={comfyBlocked || t('analysis.openComfyHelp')} onClick={() => void openInComfy()}>{t(opening ? 'analysis.openingComfy' : 'analysis.openComfy')}</button>
        {comfyBlocked && <span className="forge-note" data-analysis-comfy-reason>{comfyBlocked}</span>}
        {openError && <span className="error-message" role="alert">{openError}</span>}
      </div>}
      {made && made.length > 0 && <details className="analysis-made" data-analysis-made><summary>{t('analysis.madeWith')}</summary>
        <dl>{made.slice(0, 24).map(([k, v], i) => <div key={i}><dt>{k}</dt><dd>{v.length > 600 ? `${v.slice(0, 600)}…` : v}</dd></div>)}</dl>
      </details>}
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
          {unusableList}
          {install?.running
            ? <div className="analysis-progress" data-analysis-progress>
              <span>{t(`analysis.stage.${install.stage}`, { done: gb(install.downloaded), total: gb(install.total) })}</span>
              {install.total > 0 && <progress max={install.total} value={install.downloaded}/>}
              <button type="button" className="forge-ghost" data-analysis-cancel onClick={() => void cancelAnalysisSetup()}>{t('analysis.cancel')}</button>
            </div>
            : <div className="forge-actions">
              {taggers.length > 0
                ? <button type="button" className="action-button" data-analysis-setup-run onClick={() => void setup()}>{t('analysis.setup')}</button>
                : <button type="button" className="action-button" data-analysis-download disabled={!models.length} onClick={() => void setup(true)}>{t('analysis.download', { size: gb(overview.defaultSize) })}</button>}
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
          {unusableList}
          {busy && <p className="chat-status">{t('analysis.reading', { tagger: chosen === 'all' ? t('analysis.allTaggers') : current?.name ?? '' })}</p>}
          {result && <div className="analysis-out" data-analysis-result>
            <p className="forge-note">{result.tagger} · {(result.ms / 1000).toFixed(1)} s{rating ? ` · ${t('analysis.rating')}: ${rating[0]} ${percent(rating[1])}` : ''}</p>
            {result.character.length > 0 && <><span className="forge-note">{t('analysis.characters')}</span>
              <div className="analysis-chips">{result.character.map(([n, p]) => <span key={n} className="forge-chip analysis-chip character" title={percent(p)} data-analysis-character-tag>{n}</span>)}</div></>}
            <span className="forge-note">{t('analysis.generalTags', { count: String(result.general.length) })}</span>
            <div className="analysis-chips">{result.general.map(([n, p]) => <span key={n} className="forge-chip analysis-chip" title={percent(p)}>{n}</span>)}</div>
            {added.length > 0 && <><span className="forge-note">{t('analysis.addedTags')}</span>
              <div className="analysis-chips" data-analysis-added>{added.map(n => <span key={n} className="forge-chip analysis-chip added">{n}</span>)}</div></>}
            <div className="forge-actions analysis-actions">
              <label className="forge-switch"><input type="checkbox" checked={spaces} onChange={e => setSpaces(e.target.checked)}/><span>{t('analysis.spaces')}</span></label>
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
            <select data-analysis-describe-model value={llmModel} disabled={!llmModels.length} onMouseDown={() => { if (!forge.list) forge.load(); }} onFocus={() => { if (!forge.list) forge.load(); }} onChange={e => setDescribeModel(e.target.value)}>
              {llmModels.length ? llmModels.map(m => <option key={m.id} value={m.id}>{m.label}</option>) : <option value="">{t(forge.list ? 'forge.noModels' : 'analysis.describe.modelsOnClick')}</option>}
            </select></label>
          {describing
            ? <button type="button" className="forge-ghost" data-analysis-describe-stop onClick={stopDescribe}>{t('analysis.describe.stop')}</button>
            : <button type="button" className="action-button" data-analysis-describe-run disabled={busy || wantDescribe} onClick={() => void describe()}>{t(wantDescribe ? 'analysis.describe.loading' : reading ? 'analysis.describe.again' : 'analysis.describe.run')}</button>}
        </div>
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
