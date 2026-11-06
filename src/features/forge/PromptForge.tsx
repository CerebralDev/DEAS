import { PromptTextarea } from '../studio/PromptTextarea';
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useI18n } from '../../i18n/I18nProvider';
import { H3, dialects as familyDialects, forgeError, forgeH3, nativeTarget, writePrompt, type H3Reference } from '../../lib/forge';
import { NativeForge } from './NativeForge';
import { canonicalForgeAspect, forgeAspectChoices, resolveForgeCanvas, type ForgeCanvasContext, type ForgeSendCanvas } from '../../lib/forgeCanvas';
import type { Size } from '../../lib/resolution';

/** MOSS has a native writer but no generation family yet. */
const dialects = [...new Set([...familyDialects, 'MOSS'])];
import { isString, oneOf, usePref } from '../../lib/preferences';
import { relativeMediaPath, type MediaItem } from '../../lib/media';
import { mediaTypes, mediaViews, type MediaSort, type MediaType, type MediaView } from '../media/MediaLibrary';
import { MediaPanel } from '../media/MediaPanel';
import { ModelSelect, useForgeModels } from './ForgeButton';

const modes = ['T2VA', 'I2VA', 'FL2VA', 'L2VA', 'REF2VA'] as const;
/** Frame modes take exactly these keyframes (data/h3_forge.json mode_limits). */
const frames: Record<string, number> = { T2VA: 0, I2VA: 1, FL2VA: 2, L2VA: 1 };
const isNumber = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const roles = ['character-1', 'character-2', 'character-3', 'character-4', 'place', 'style', 'first-frame', 'last-frame'];
/** The Prompt Writer takes at most 8 pictures, REF2VA 9. */
const maxPictures = (target: string, mode: string) => target !== H3 ? 8 : mode === 'REF2VA' ? 9 : frames[mode];
interface Picture { path: string; nativePath: string; src: string; role: string }
interface Association { scope: object; canvas?: Size; contextKey: string }

/** Standalone Prompt Forge: every dialect the families name, H3 with labelled references; the result goes to Generate. */
export function PromptForge({ endpoint, root, inputDir, outputDir, canvasContext, onSend, active = true }: { active?: boolean; endpoint: string; root?: string; inputDir?: string; outputDir?: string; canvasContext?: ForgeCanvasContext; onSend: (positive: string, negative: string, canvas?: ForgeSendCanvas) => void }) {
  const { t } = useI18n();
  const [target, setTarget] = usePref('forge.target', dialects[0], oneOf(...dialects));
  const native = nativeTarget(target);
  const forge = useForgeModels(endpoint, root, active, !native, true);
  const [mode, setMode] = usePref('forge.h3.mode', 'T2VA', oneOf(...modes));
  const [duration, setDuration] = usePref('forge.h3.duration', 5, isNumber);
  const [shots, setShots] = usePref<string | number>('forge.h3.shots', 'Auto', (v): v is string | number => v === 'Auto' || isNumber(v));
  const [detail, setDetail] = usePref('forge.detail', 5, isNumber);
  const [creativity, setCreativity] = usePref('forge.creativity', 5, isNumber);
  const [see, setSee] = useState(false);
  const [aspect, setAspect] = usePref('forge.h3.aspect', 'N/A', isString);
  const [customWidth, setCustomWidth] = usePref('forge.h3.customWidth', 1, isNumber);
  const [customHeight, setCustomHeight] = usePref('forge.h3.customHeight', 1, isNumber);
  const [brief, setBrief] = usePref('forge.brief', '', isString);
  const [pictures, setPictures] = useState<Picture[]>([]);
  useLayoutEffect(() => { setPictures([]); }, [endpoint, root, inputDir, outputDir]);
  const [query, setQuery] = useState('');
  const [sort, setSort] = usePref<MediaSort>('forge.media.sort', 'date', oneOf('date', 'name', 'size'));
  const [view, setView] = usePref<MediaView>('forge.media.view', 'tiles', oneOf(...mediaViews));
  const [type, setType] = usePref<MediaType>('forge.media.type', 'all', oneOf(...mediaTypes));
  const [result, setResult] = useState('');
  const textRevision = useRef(0);
  const editResult = (text: string) => { textRevision.current++; setResult(text); };
  const [negative, setNegative] = useState('');
  const [warnings, setWarnings] = useState<string[]>([]);
  const [blind, setBlind] = useState(false);
  const [association, setAssociation] = useState<Association | null>(null);
  const [pending, setPending] = useState<{ scope: object; token: object } | null>(null);
  const [error, setError] = useState('');
  useEffect(() => { if (active) forge.load(); }, [root, endpoint, forge.rev, active, native]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => { const open = () => forge.load(); window.addEventListener('deas:forge-open', open); return () => window.removeEventListener('deas:forge-open', open); });
  const h3 = target === H3, limit = maxPictures(target, mode);
  const shown = pictures.slice(0, limit);
  const selectedAspect = canonicalForgeAspect(aspect);
  const resolved = resolveForgeCanvas(h3 ? aspect : 'N/A', { width: customWidth, height: customHeight }, canvasContext);
  // Result geometry comes from the consumed request. A -> B -> A still creates
  // a new identity, so a previous request/result cannot regain Send authority.
  const scopeKey = JSON.stringify([endpoint, root, inputDir, outputDir, target, native, active, mode, forge.model, forge.rev,
    brief, duration, shots, detail, creativity, see, shown, h3 ? aspect : 'N/A', customWidth, customHeight,
    canvasContext?.key, resolved.canvas, resolved.error]);
  const scope = useMemo(() => ({}), [scopeKey]);
  const latestScope = useRef<object | null>(null), request = useRef<object | null>(null);
  useLayoutEffect(() => {
    latestScope.current = active ? scope : null; request.current = null;
    return () => { latestScope.current = null; request.current = null; };
  }, [active, scope]);
  const busy = pending?.scope === scope;
  const resultFits = association?.scope === scope;
  const handoffBlocked = !!association?.canvas && canvasContext?.acceptsCanvas?.(association.canvas) === false;
  const send = (textOnly = false) => {
    if (!result.trim()) return;
    if (textOnly) { onSend(result, negative); return; }
    if (!resultFits || busy || resolved.error || handoffBlocked) return;
    if (association?.canvas) onSend(result, negative, { canvas: { ...association.canvas }, contextKey: association.contextKey });
    else onSend(result, negative);
  };
  const add = (item: MediaItem) => {
    if (item.media !== 'image' || !item.path) return;
    const input = inputDir ? relativeMediaPath(item.path, inputDir) : '';
    const output = outputDir ? relativeMediaPath(item.path, outputDir) : '';
    const path = input || (output ? `${output} [output]` : '');
    if (path && shown.length < limit && !shown.some(p => p.path === path)) setPictures([...shown, { path, nativePath: item.path!, src: item.src, role: roles[Math.min(shown.length, 3)] }]);
  };
  const missing = h3 && mode !== 'REF2VA' && shown.length !== limit;
  const run = async () => {
    if (!active || !root || !forge.model || busy || request.current || resolved.error || missing || (!brief.trim() && !shown.length)) return;
    const token = {}, snapshot: Association = { scope, ...(resolved.canvas ? { canvas: { ...resolved.canvas } } : {}), contextKey: canvasContext?.key ?? '' };
    const consumedTextRevision = textRevision.current;
    request.current = token; setPending({ scope, token }); setError(''); setWarnings([]); setBlind(false);
    const ownsRequest = () => latestScope.current === scope && request.current === token;
    const canPublish = () => ownsRequest() && textRevision.current === consumedTextRevision;
    try {
      if (h3) {
        const references: H3Reference[] = shown.map(p => mode === 'REF2VA' ? { kind: 'image', role: 'subject', path: p.path, easy_role: p.role } : { kind: 'image', role: 'keyframe', path: p.path });
        const out = await forgeH3(endpoint, root, { model: forge.model, mode, brief, duration, shots, detail, creativity: forge.list?.creativity[creativity - 1] ?? '', references, easy: mode === 'REF2VA' && references.length > 0, see_pictures: mode === 'REF2VA' && see && shown.length > 0, ...(snapshot.canvas ? { output_canvas: { ...snapshot.canvas } } : {}) });
        if (canPublish()) { setResult(out.simple_prompt); setWarnings(out.warnings); setAssociation(snapshot); }
      } else {
        const out = await writePrompt(endpoint, root, { model: forge.model, write_for: target, idea: brief, detail, creativity, images: shown.map(p => p.path) });
        if (canPublish()) { setResult(out.prompt); setBlind(shown.length > 0 && !out.saw_images); setAssociation(snapshot); }
      }
    } catch (e) { if (canPublish()) setError(forgeError(e)); }
    finally {
      // A manual edit discards publication, but the owning request still releases Write.
      if (ownsRequest()) { request.current = null; setPending(held => held?.scope === scope && held.token === token ? null : held); }
    }
  };
  const level = (key: string, value: number, set: (v: number) => void) => <label className="forge-field"><span>{t(key)} {value}</span><input type="range" min={1} max={10} value={value} onChange={e => set(Number(e.target.value))}/></label>;
  const picker = <label className="forge-field"><span>{t('forge.target')}</span><select data-forge-target value={target} onChange={e => setTarget(e.target.value)}>{dialects.map(d => <option key={d} value={d}>{d}</option>)}</select></label>;
  if (native) return <section className="forge-tab" aria-label={t('studio.tab.forge')}><NativeForge key={native} root={root} canvasContext={canvasContext} target={native} forge={forge} brief={brief} setBrief={setBrief} detail={detail} setDetail={setDetail} creativity={creativity} setCreativity={setCreativity} onSend={onSend} header={picker} active={active}/></section>;
  return <section className="forge-tab" aria-label={t('studio.tab.forge')}>
    <div className="forge-controls">
      {picker}
      <ModelSelect forge={forge}/>
      {!root && <p className="field-help">{t('forge.noBackend')}</p>}
      {h3 && <div className="forge-row">
        <label className="forge-field"><span>{t('forge.mode')}</span><select data-forge-mode value={mode} onChange={e => setMode(e.target.value as typeof modes[number])}>{modes.map(m => <option key={m}>{m}</option>)}</select></label>
        <label className="forge-field"><span>{t('forge.duration')}</span><select value={duration} onChange={e => setDuration(Number(e.target.value))}>{(forge.list?.durations ?? [duration]).map(d => <option key={d} value={d}>{`${d} s`}</option>)}</select></label>
        <label className="forge-field"><span>{t('forge.shots')}</span><select value={shots} onChange={e => setShots(e.target.value === 'Auto' ? 'Auto' : Number(e.target.value))}>{(forge.list?.shot_counts ?? ['Auto']).map(s => <option key={s} value={s}>{s === 'Auto' ? t('forge.shotsAuto') : s}</option>)}</select></label>
      </div>}
      {h3 ? <>
        <label className="forge-field"><span>{t('forge.aspect')}</span><select data-forge-aspect value={selectedAspect ?? aspect} onChange={e => setAspect(e.target.value)}>
          {selectedAspect === null && <option value={aspect}>{aspect}</option>}
          {forgeAspectChoices.map(value => <option key={value} value={value}>{value === 'auto' ? t('forge.canvas.auto') : value === 'custom' ? t('forge.canvas.custom') : value}</option>)}
        </select></label>
        {selectedAspect === 'custom' && <div className="forge-row">
          <label className="forge-field"><span>{t('forge.canvas.ratioWidth')}</span><input data-forge-custom-width type="number" min={1} max={100} step="any" value={customWidth} onChange={e => setCustomWidth(Number(e.target.value))}/></label>
          <label className="forge-field"><span>{t('forge.canvas.ratioHeight')}</span><input data-forge-custom-height type="number" min={1} max={100} step="any" value={customHeight} onChange={e => setCustomHeight(Number(e.target.value))}/></label>
        </div>}
        {resolved.canvas && <p className="field-help" data-forge-resolved-canvas>{t('forge.canvas.resolved', { width: String(resolved.canvas.width), height: String(resolved.canvas.height) })}</p>}
        {resolved.error && <p className="forge-warning" data-forge-canvas-error role="status">{t(resolved.error)}</p>}
      </> : <p className="field-help" data-forge-canvas-unsupported>{t('forge.canvas.unsupported')}</p>}
      <div className="forge-row">{level('forge.detail', detail, setDetail)}{level('forge.creativity', creativity, setCreativity)}</div>
      <label className="forge-field"><span>{t('forge.brief')}</span><PromptTextarea data-forge-brief value={brief} maxLength={16384} onChange={e => setBrief(e.target.value)}/></label>
      {limit > 0 && <div className="forge-pictures">
        <p className="field-help">{t('forge.pictures', { count: shown.length, limit })}</p>
        <ul>{shown.map((p, i) => <li key={p.path}>
          <img src={p.src} alt={p.path}/>
          {h3 && mode === 'REF2VA' ? <select data-forge-role aria-label={p.path} value={p.role} onChange={e => setPictures(shown.map((q, j) => j === i ? { ...q, role: e.target.value } : q))}>{roles.map(r => <option key={r} value={r}>{t(`forge.role.${r.replace(/-\d$/, '')}`, { n: r.slice(-1) })}</option>)}</select>
            : <span>{h3 ? t(mode === 'I2VA' || (mode === 'FL2VA' && !i) ? 'forge.role.first-frame' : 'forge.role.last-frame') : p.path}</span>}
          <button type="button" aria-label={t('forge.remove')} onClick={() => setPictures(shown.filter((_, j) => j !== i))}>×</button>
        </li>)}</ul>
        {h3 && mode !== 'REF2VA' && shown.length > 0 && <p className="field-help" data-forge-auto-see>{t('forge.autoSee')}</p>}
        {h3 && mode === 'REF2VA' && shown.length > 0 && <label className="forge-check"><input type="checkbox" data-forge-see checked={see} onChange={e => setSee(e.target.checked)}/>{t('forge.see')}</label>}
      </div>}
      <button type="button" className="action-button" data-forge-write disabled={busy || !active || !root || !forge.model || (!brief.trim() && !shown.length) || missing || !!resolved.error} onClick={() => void run()}>{t(busy ? 'forge.writing' : 'forge.write')}</button>
      {pending && pending.scope !== scope && <p className="forge-warning" data-forge-discarded role="status">{t('forge.canvas.replyStale')}</p>}
      {missing && <p className="field-help">{t('forge.frames', { count: limit })}</p>}
      {error && <p className="error-message" role="alert">{t(error)}</p>}
    </div>
    <div className="forge-output">
      <label className="forge-field"><span>{t('forge.result')}</span><PromptTextarea data-forge-result value={result} onChange={e => editResult(e.target.value)}/></label>
      {blind && <p className="forge-warning">{t('forge.blind')}</p>}
      {warnings.map(w => <p key={w} className="forge-warning">{w}</p>)}
      <label className="forge-field"><span>{t('forge.negative')}</span><PromptTextarea data-forge-negative className="forge-negative" value={negative} onChange={e => setNegative(e.target.value)}/></label>
      {result.trim() && !resultFits && <p className="forge-warning" data-forge-stale role="status">{t('forge.canvas.stale')}</p>}
      {handoffBlocked && <p className="forge-warning" data-forge-handoff-blocked role="status">{t('forge.canvas.handoffBlocked')}</p>}
      {association?.canvas && <p className="field-help" data-forge-result-canvas>{t('forge.canvas.consumed', { width: String(association.canvas.width), height: String(association.canvas.height) })}</p>}
      <button type="button" className="action-button" data-forge-send disabled={!result.trim() || !resultFits || busy || !!resolved.error || handoffBlocked} onClick={() => send()}>{t('forge.send')}</button>
      {result.trim() && (!resultFits || handoffBlocked) && <button type="button" className="action-button" data-forge-send-text-only onClick={() => send(true)}>{t('forge.canvas.sendTextOnly')}</button>}
      {forge.model && limit > 0 && <div className="forge-media"><p className="field-help">{t('forge.pick')}</p><MediaPanel selected={null} onSelect={add} roots={{ input: inputDir, output: outputDir }} controls={{ query, onQuery: setQuery, sort, onSort: setSort, view, onView: setView, type, onType: setType }} marks={new Map(shown.map(p => [p.nativePath, t(`forge.role.${p.role.replace(/-\d$/, '')}`, { n: p.role.slice(-1) })]))}/></div>}
    </div>
  </section>;
}
