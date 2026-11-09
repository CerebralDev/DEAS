import { useEffect, useRef, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { useI18n } from '../../i18n/I18nProvider';
import type { RecipeAllowlist } from '../../lib/execution';
import { checkRecipePrerequisites } from '../../lib/recipePrerequisites';
import { pollWorkbench, readCurrentWorkbench, runWorkbenchJob, type JobStatus } from '../../lib/jobs';
import { usePref } from '../../lib/preferences';
import { audioBitrates, audioCodecs, bitDepths, defaultVideoOutput, isVideoOutput, videoCodecs, videoContainers, videoPostFrameRate, type VideoEnhanceIntentV1, type VideoOutput } from '../../lib/videoIntent';
import { uploadVideoPostSource, type VideoPostInspection } from '../../lib/videoPost';
import { AudioEnhance } from './AudioEnhance';
import { useAudioEnhance } from './useAudioEnhance';
import { TvePostControls, defaultTve, isTvePref, toVideoPost } from './TvePostControls';

const terminal = (s: JobStatus['state']) => ['succeeded', 'failed', 'cancelled', 'unknown'].includes(s);
/** Enhance an inspected source on the owned backend with the same TVE chain the video pipelines use; the job lands in the Studio queue. */
export function TveBackendEnhance({ endpoint, source, disabled = false }: { endpoint: string; source: VideoPostInspection | null; disabled?: boolean }) {
  const { t } = useI18n();
  const audioEnhance = useAudioEnhance('tve');
  const [tve, setTve] = usePref('studio.tve.post', defaultTve, isTvePref);
  const [output, setOutput] = usePref<VideoOutput>('studio.tve.output', defaultVideoOutput, isVideoOutput);
  const [choices, setChoices] = useState<RecipeAllowlist | null>(null);
  const [job, setJob] = useState<JobStatus | null>(null);
  const [jobSource, setJobSource] = useState<string>();
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const live = useRef(true);
  const reading = useRef(false);
  useEffect(() => { live.current = true; return () => { live.current = false; }; }, []);
  useEffect(() => { setChoices(null); }, [endpoint]);
  const loadChoices = () => {
    if (!isTauri() || choices || reading.current || !endpoint.trim()) return;
    reading.current = true;
    void checkRecipePrerequisites(endpoint).then(v => { if (live.current) setChoices(v.allowlist); }, () => undefined).finally(() => { reading.current = false; });
  };
  // Follow the submitted job through the shared native session until it settles. The TVE panel owns its
  // own poll: `native_workbench_current` only replays the last snapshot, so a queued job never advanced
  // while the Studio queue was closed.
  useEffect(() => {
    if (!job || terminal(job.state) || !isTauri()) return;
    let on = true;
    const timer = setInterval(() => { void pollWorkbench().then(s => {
      if (!on) return;
      const next = s.jobs.find(j => j.id === job.id);
      // A failed preparation drops the held job; polling a vanished id would stay "queued" forever.
      setJob(next ?? { ...job, state: 'unknown', error: t('studio.tve.jobGone') });
    }, () => undefined); }, 1500);
    return () => { on = false; clearInterval(timer); };
  }, [job?.id, job?.state]); // eslint-disable-line react-hooks/exhaustive-deps
  const videoPost = toVideoPost(tve);
  const fps = source ? source.fpsNumerator / source.fpsDenominator : undefined;
  const ready = !!source?.sourceId && !!videoPost && !!endpoint.trim() && isTauri() && !busy && !disabled && audioEnhance.ready && !(tve.frameGen && tve.engine === 'rife' && !tve.rifeModel) && !(tve.loop && tve.loopBackend === 'rife' && !tve.loopModel);
  async function run() {
    if (!ready || !source?.sourceId || !videoPost) return;
    const audio = audioEnhance.intent ? { ...audioEnhance.intent } : undefined;
    setBusy(true); setError(null);
    try {
      const { imageFormat, imageCompression, ...out } = output; void imageFormat; void imageCompression;
      const epoch = (await readCurrentWorkbench()).admissionEpoch;
      const sourceVideo = await uploadVideoPostSource(endpoint, source.sourceId);
      const intent: VideoEnhanceIntentV1 = { recipeId: 'video-enhance', version: 1, sourceVideo, frameRate: Number((source.fpsNumerator / source.fpsDenominator).toFixed(3)), seed: Math.floor(Math.random() * 2 ** 32), videoPost, output: out, ...(audio ? { audioEnhance: audio } : {}) };
      const value = await runWorkbenchJob(endpoint, intent, epoch);
      if (live.current) { setJob(value); setJobSource(source.sourceId); }
    } catch (failure) { if (live.current) setError(failure instanceof Error ? failure.message : String(failure)); }
    finally { if (live.current) setBusy(false); }
  }
  const choose = <K extends keyof VideoOutput>(k: K, list: readonly VideoOutput[K][]) => <label key={k}>{t(`studio.run.opt.output.${k}`)}<select data-tve-output={k} value={String(output[k])} onChange={e => setOutput({ ...output, [k]: e.target.value })}>{list.map(v => <option key={String(v)} value={String(v)}>{String(v)}</option>)}</select></label>;
  const audioOutput = jobSource === source?.sourceId && job && ['succeeded', 'failed', 'cancelled'].includes(job.state) && job.outputs.length ? job : null;
  const audioOutputIndex = Math.max(0, audioOutput?.outputs.findIndex(o => o.role === 'processed') ?? -1);
  return <section className="tve-panel" aria-label={t('studio.tve.backend')} data-tve-backend>
    <AudioEnhance scope="tve" settings={audioEnhance} automatic={!!videoPost} source={audioOutput?{jobId:audioOutput.id,index:audioOutputIndex}:source?.sourceId?{sourceId:source.sourceId}:undefined} name={audioOutput?audioOutput.outputs[audioOutputIndex].filename:source?.name} disabled={disabled || busy}/>
    <h3>{t('studio.tve.backend')}</h3>
    <p className="field-help">{t('studio.tve.backendHint')}</p>
    <TvePostControls value={tve} onChange={setTve} allow={choices} onRead={loadChoices} disabled={disabled || busy} fps={fps}/>
    <div className="tve-options">
      {choose('codec', videoCodecs)}{choose('container', videoContainers)}{choose('bitDepth', bitDepths)}
      <label>{t('studio.run.opt.output.quality')}<input type="number" data-tve-output="quality" min={0} max={51} step={1} value={output.quality} onChange={e => { const n = Number(e.target.value); if (Number.isInteger(n) && n >= 0 && n <= 51) setOutput({ ...output, quality: n }); }}/></label>
      {choose('audioCodec', audioCodecs)}{choose('audioBitrate', audioBitrates)}
    </div>
    <div className="tve-actions">
      <button type="button" className="wb-dock-action" data-tve-enhance disabled={!ready} onClick={() => void run()}>{t(busy ? 'studio.tve.enhancing' : 'studio.tve.enhance')}</button>
    </div>
    {!source?.sourceId && <p className="field-help">{t('studio.tve.needsSource')}</p>}
    {source && !videoPost && <p className="field-help">{t('studio.tve.post.needsStage')}</p>}
    {error && <p role="alert" data-tve-enhance-error>{t('studio.tve.failed')}: {error}</p>}
    {job && <div className="tve-draft" data-tve-enhance-job data-state={job.state} aria-live="polite">
      <strong>{t(`studio.jobs.${job.state}`)}</strong>
      <dl>
        {source && <div><dt>{t('studio.tve.fps')}</dt><dd>{fps} → {videoPostFrameRate(fps ?? 0, videoPost)}</dd></div>}
        {job.outputs.map(o => <div key={`${o.subfolder}/${o.filename}`}><dt>{t(o.role === 'original' ? 'studio.jobs.role.original' : 'studio.tve.outputFile')}</dt><dd>{o.filename}</dd></div>)}
      </dl>
      {job.error && <p role="alert">{job.error}</p>}
      <p className="field-help">{t('studio.tve.queueHint')}</p>
    </div>}
  </section>;
}
