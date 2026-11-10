import { PromptTextarea } from '../studio/PromptTextarea';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { WildcardRules } from '../../lib/wildcard';
import { useI18n } from '../../i18n/I18nProvider';
import { cancelForge, forgeError, forgeH3, forgeModels, labelCharacters, nativeForgeCancel, nativeForgeWrite, nativeTarget, nativeWriteModel, readCast, writePrompt, type ForgeModels, type H3Continuity, type H3Reference } from '../../lib/forge';
import { isString, usePref } from '../../lib/preferences';
import { discoverLlmModels, hasLlmConfiguration, requestLlmSetup } from '../../lib/llm';
import './forge.css';
import { recommendedFirst } from '../../lib/llmRecommendations';
import { RecommendedModelOptions } from '../llm/RecommendedModelOptions';

/** Read-only native discovery by default; video/node-pack tools also need their metadata. */
export function useForgeModels(endpoint: string, root?: string, active = true, nodepack = false, localGguf = false) {
  const [list, setList] = useState<ForgeModels | null>(null);
  const [error, setError] = useState('');
  const [model, setModel] = usePref('forge.model', '', isString);
  const [rev, setRev] = useState(0);
  const identity = JSON.stringify([endpoint, root, rev, active, nodepack, localGguf]);
  const scope = useRef<{ identity: string; active: boolean; loaded: boolean; pending: boolean } | null>(null);
  useEffect(() => {
    const bump = () => setRev(n => n + 1);
    window.addEventListener('deas:llm-changed', bump);
    return () => window.removeEventListener('deas:llm-changed', bump);
  }, []);
  useEffect(() => {
    const current = { identity, active, loaded: false, pending: false };
    scope.current = current;
    setList(null); setError('');
    return () => { current.active = false; };
  }, [endpoint, root, rev, active, nodepack, localGguf]);
  const load = () => {
    const current = scope.current;
    if (!current?.active || current.pending) return;
    if (!root) { requestLlmSetup(); return; }
    if (current.loaded) { if (list && !list.models.some(m => !m.disabled) && !Object.keys(list.errors).length) requestLlmSetup(); return; }
    current.pending = true;
    setError('');
    (async (): Promise<ForgeModels | null> => {
      if (!nodepack) {
        const result = await discoverLlmModels(root);
        const warnings = [...result.warnings];
        // Only when local GGUFs are all there is: beside a usable server they are just greyed out.
        if (!localGguf && result.models.length && result.models.every(m => m.provider === 'local')) warnings.push('forge.native.serverRequired');
        // A server, when there is one, stays the default over a local GGUF.
        const listed = [...result.models.filter(m => m.provider !== 'local'), ...result.models.filter(m => m.provider === 'local')];
        return { models: listed.map(m => ({ id: `${m.provider}:${m.id}`, label: m.name, disabled: !localGguf && m.provider === 'local' })), errors: Object.fromEntries(warnings.map((w, i) => [String(i), w])), creativity: [], shot_counts: [], default_shots: 'Auto' };
      }
      // The node pack lists its models concurrently with the readiness check.
      const models = forgeModels(endpoint, root);
      if (!await hasLlmConfiguration(root)) {
        // A node-pack failure while unconfigured must not become an alert.
        await models.catch(() => null);
        if (current.active) requestLlmSetup();
        return null;
      }
      return current.active ? await models : null;
    })().then(result => {
      if (!current.active) return;
      current.pending = false;
      if (!result) return;
      current.loaded = true; setList(result);
      if (!result.models.some(m => !m.disabled) && !Object.keys(result.errors).length) requestLlmSetup();
    }, e => {
      if (!current.active) return;
      current.pending = false;
      setError(forgeError(e));
    });
  };
  const usable = recommendedFirst(scope.current?.identity === identity && scope.current.loaded ? list?.models.filter(m => !m.disabled) ?? [] : []);
  // A remembered model only counts while the backend still lists it.
  const current = usable.some(m => m.id === model) ? model : usable[0]?.id ?? '';
  // The list empties on every reload (tab switch, backend start), which is not a model change.
  const settled = useRef('');
  if (current) settled.current = current;
  return { list, error, load, rev, usable, model: current, settledModel: settled.current, serverOnly: !nodepack && !localGguf, setModel };
}

export function ModelSelect({ forge }: { forge: ReturnType<typeof useForgeModels> }) {
  const { t } = useI18n();
  return <label className="forge-field"><span>{t('forge.model')}</span>
    <select data-forge-model value={forge.model} disabled={!forge.usable.length} onChange={e => forge.setModel(e.target.value)}>
      {forge.usable.length ? <RecommendedModelOptions models={forge.usable}/> : <option value="">{forge.error ? '—' : t(forge.list ? forge.serverOnly ? 'forge.noServer' : 'forge.noModels' : 'forge.loading')}</option>}
    </select>
    {forge.list && Object.values(forge.list.errors).map(text => <small key={text} className="forge-warning">{t(text)}</small>)}
    {forge.error && <span className="error-message" role="alert">{t(forge.error)} <button type="button" data-forge-retry onClick={forge.load}>{t('forge.retry')}</button></span>}
  </label>;
}

/** What the Director Forge must know about the run it writes for (MiniMax H3 video). */
export interface H3Context { continuity?: H3Continuity; mode: string; duration: number; canvas: { width: number; height: number }; references: H3Reference[]; /** Every reference is a labelled picture: the Director's picture-label writer applies. */ easy: boolean }
const isNumber = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const isShots = (v: unknown): v is string | number => v === 'Auto' || isNumber(v);

/** Hammer in a prompt field: write or enhance that prompt in the selected model's dialect, review, then apply (undoable).
 * With `h3`, it is the Director's Prompt Forge for the current run: Shots, Detail, Creativity, Cancel, the last three drafts and the `tools` prompt helpers. */
/** Lucide "hammer" (ISC). */
export const HammerIcon = () => <svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="m15 12-8.373 8.373a1 1 0 1 1-3-3L12 9"/><path d="m18 15 4-4"/><path d="m21.5 11.5-1.914-1.914A2 2 0 0 1 19 8.172V7l-2.26-2.26a6 6 0 0 0-4.202-1.756L9 2.96l.92.82A6.18 6.18 0 0 1 12 8.4V10l2 2h1.172a2 2 0 0 1 1.414.586L18.5 14.5"/></svg>;

export function ForgeButton({ endpoint, root, dialect, value, image, onApply, onWildcards, wildcard, h3, tools, onBusyChange, inline = false, active = true }: { active?: boolean; inline?: boolean; endpoint: string; root?: string; dialect?: string; value: string; /** Image-to-image/inpaint source, read by a vision model. */ image?: string; onApply: (text: string) => void; onBusyChange?: (busy: boolean) => void; onWildcards?: () => void; /** Active wildcard rules: the node resolves them and the picks join the idea. */ wildcard?: WildcardRules; h3?: H3Context; /** Director prompt helpers shown in the panel (H3 video only). */ tools?: ReactNode }) {
  const { t } = useI18n();
  const forge = useForgeModels(endpoint, root, true, !!h3 || !nativeTarget(dialect), true);
  const opener = useRef<HTMLButtonElement>(null);
  const [shots, setShots] = usePref<string | number>('forge.h3.shots', 'Auto', isShots);
  const [shotBriefs, setShotBriefs] = useState<string[]>([]);
  const [detail, setDetail] = usePref('forge.detail', 5, isNumber);
  const [creativity, setCreativity] = usePref('forge.creativity', 5, isNumber);
  const [see, setSee] = usePref('forge.h3.see', false, (v): v is boolean => typeof v === 'boolean');
  const request = useRef<{ id: string; cancelled: boolean; endpoint: string; root: string } | null>(null);
  const busyCallback = useRef(onBusyChange);
  busyCallback.current = onBusyChange;
  const inFlight = useRef(false);
  useEffect(() => () => {
    const current = request.current;
    request.current = null;
    if (!inFlight.current) return;
    inFlight.current = false;
    if (current) {
      current.cancelled = true;
      if (current.id) void cancelForge(current.endpoint, current.root, current.id).catch(() => {}); // Native cancellation logs its own failures.
    }
    busyCallback.current?.(false);
  }, []);
  // A draft fits only the run it was written for: mode, duration, canvas, references and model (as the Director checks on Apply).
  const hasPictures = !!h3?.references.some(r => r.kind === 'image' && r.path?.trim());
  const labelledPictures = !!h3?.easy && h3.mode === 'REF2VA';
  const seeing = labelledPictures && hasPictures && see;
  const frames = !!h3 && ['I2VA', 'FL2VA', 'L2VA'].includes(h3.mode);
  const castKeys = frames
    ? [...new Set(h3!.references.filter(r => r.kind === 'image' && r.path).flatMap(r => r.boxes?.length ? r.boxes.map(b => String(b.n)) : r.who?.map(String) ?? []))].sort((a, b) => Number(a) - Number(b))
    : h3?.easy ? [...new Set(h3.references.flatMap(r => [r.easy_role, r.picture_kinds?.[0]].some(kind => kind === 'pose' || kind === 'custom') ? [] : r.easy_role === 'place' || r.easy_role === 'style' ? [r.easy_role] : (r.boxes?.length?r.boxes.map(b=>String(b.n)):r.picture_kinds?.includes('character')&&r.who?.length?r.who.map(String):labelCharacters(r.easy_role))))].sort((a, b) => (Number(a) || 99) - (Number(b) || 99)) : [];
  // Pose cards use the input path as identity; only the wire key follows picture order.
  const pictures = h3?.references.filter(r => r.kind === 'image') ?? [];
  if (h3?.easy && !frames) castKeys.push(...pictures.filter(r => r.easy_role === 'pose' && r.path).map(r => `pose:${r.path}`));
  const wireKey = (key: string) => key.startsWith('pose:') ? `pose@${pictures.findIndex(r => `pose:${r.path}` === key) + 1}` : key;
  const [cards, setCards] = useState<Record<string, string>>({});
  const [reading, setReading] = useState('');
  const readRequest = useRef<object | null>(null);
  const readFit = JSON.stringify([endpoint, root, forge.model, h3?.mode, h3?.easy, h3?.references]);
  useEffect(() => {
    setReading('');
    return () => { readRequest.current = null; };
  }, [readFit]);
  const cast = Object.fromEntries(castKeys.flatMap(k => cards[k]?.trim() ? [[wireKey(k), cards[k].trim().slice(0, 600)]] : []));
  const fit = h3 ? JSON.stringify([endpoint, root, h3, forge.model, seeing, cast]) : '';
  const [drafts, setDrafts] = useState<{ text: string; fit: string }[]>([]);
  const [draftFit, setDraftFit] = useState('');
  const [expanded, setOpen] = useState(false);
  const open = inline || expanded;
  const [choosing, setChoosing] = useState(false);
  const [brief, setBrief] = useState(inline ? value : '');
  const [result, setResult] = useState('');
  const [busy, setBusy] = useState(false);
  const changeBusy = (next: boolean) => {
    if (inFlight.current === next) return;
    inFlight.current = next; setBusy(next); busyCallback.current?.(next);
  };
  const [error, setError] = useState('');
  const [undo, setUndo] = useState<string | null>(null);
  const [blind, setBlind] = useState(false);
  const [warnings, setWarnings] = useState<string[]>([]);
  const disabled = !dialect;
  useEffect(() => { if (active && open && !choosing) forge.load(); }, [active, open, choosing, endpoint, root, forge.rev, dialect, !!h3]); // eslint-disable-line react-hooks/exhaustive-deps
  const wildcardResolved = useRef(false);
  useEffect(() => {
    if (!open || choosing || !forge.model || !wildcard || wildcardResolved.current) return;
    wildcardResolved.current = true;
    // Resolve picks only after readiness, never while redirecting to configuration.
    invoke<{ positive: string }>('native_wildcard_build', { baseUrl: endpoint, rules: wildcard }).then(r => setBrief(b => [b, r.positive].filter(s => s.trim()).join(', ')), e => setError(String(e)));
  }, [open, choosing, forge.model, wildcard, endpoint]);
  const toggle = () => {
    if (!open) {
      if (!root && !onWildcards) { requestLlmSetup(); return; }
      wildcardResolved.current = false;
      setBrief(value); setResult(''); setError(''); setChoosing(!!onWildcards);
    }
    setOpen(!open);
  };
  const write = async () => {
    const current = request.current = { id: '', cancelled: false, endpoint, root: root! };
    changeBusy(true); setError(''); setBlind(false); setWarnings([]);
    if (!h3) setResult(''); // a Director draft stays visible (and in history) while the next one is written or cancelled
    try {
      if (h3) {
        current.id = crypto.randomUUID();
        const count = typeof shots === 'number' ? shots : 0;
        const out = await forgeH3(endpoint, root!, { model: forge.model, mode: h3.mode, brief, duration: h3.duration, shots, shot_briefs: Array.from({ length: count }, (_, i) => shotBriefs[i] ?? ''), detail, creativity: forge.list?.creativity[creativity - 1] ?? '', references: h3.references, easy: h3.easy, see_pictures: seeing, ...(Object.keys(cast).length ? { cast } : {}), output_canvas: h3.canvas, request_id: current.id, ...(h3.continuity ? { continuity: h3.continuity } : {}) });
        if (request.current !== current || current.cancelled) return;
        setWarnings(Array.isArray(out.warnings) ? out.warnings.filter((w): w is string => typeof w === 'string').slice(0, 8).map(w => w.slice(0, 2048)) : []);
        setResult(out.simple_prompt); setDraftFit(fit); setDrafts(d => [{ text: out.simple_prompt, fit }, ...d].slice(0, 3));
      } else if (nativeTarget(dialect)) {
        // PromptForge's whole pipeline in DEAS (canon, tag search, clean-up) as if only the prompt had been filled in:
        // every control at PromptForge's default. The hammer is a quick text rewrite: pictures belong to the Prompt Forge tab.
        const out = await nativeForgeWrite(root!, { target: nativeTarget(dialect)!, brief, model: nativeWriteModel(forge.model), controls: {} });
        if (request.current !== current || current.cancelled) return;
        setResult(out.prompt); setWarnings(out.warnings);
      } else {
        const out = await writePrompt(endpoint, root!, { model: forge.model, write_for: dialect!, idea: brief, images: image ? [image] : [] });
        if (request.current !== current || current.cancelled) return;
        setResult(out.prompt); setBlind(!!image && !out.saw_images);
      }
    }
    catch (e) { if (request.current === current && !current.cancelled) setError(forgeError(e)); }
    finally { if (request.current === current) changeBusy(false); }
  };
  const read = async (key: string) => {
    const current = readRequest.current = {};
    setReading(key); setError('');
    try {
      const count = typeof shots === 'number' ? shots : 0;
      const out = await readCast(endpoint, root!, { model: forge.model, mode: h3!.mode, ...(/^\d+$/.test(key) ? { character: Number(key) } : { subject: wireKey(key) as 'place' | 'style' | `pose@${number}` }), references: h3!.references, brief, shots, shot_briefs: Array.from({ length: count }, (_, i) => shotBriefs[i] ?? ''), redo: !!cards[key]?.trim() });
      if (readRequest.current === current) setCards(c => ({ ...c, [key]: out.text }));
    } catch (e) { if (readRequest.current === current) setError(forgeError(e)); }
    finally { if (readRequest.current === current) { setReading(''); readRequest.current = null; } }
  };
  const cancel = () => { const current = request.current; if (!current) return; current.cancelled = true; changeBusy(false); (current.id ? cancelForge(current.endpoint, current.root, current.id) : nativeForgeCancel()).catch(e => { if (request.current === current) setError(forgeError(e)); }); };
  const stale = !!h3 && !!result && draftFit !== fit;
  const level = (key: string, v: number, set: (n: number) => void) => <label className="forge-field"><span>{t(key)} {v}</span><input type="range" min={1} max={10} value={v} onChange={e => set(Number(e.target.value))}/></label>;
  return <div className={inline ? 'forge-controls' : 'forge-anchor'}>
    {!inline && <button ref={opener} type="button" className="forge-hammer" data-forge-open aria-expanded={open} disabled={disabled && !onWildcards} aria-label={t('forge.button')} title={t(onWildcards ? 'wildcards.choose' : !root ? 'forge.noBackend' : !dialect ? 'forge.noDialect' : 'forge.button')} onClick={toggle}>
      <HammerIcon/>
    </button>}
    {undo !== null && (!open || inline) && <button type="button" className="forge-undo" data-forge-undo onClick={() => { onApply(undo); setUndo(null); }}>{t('forge.undo')}</button>}
    {open && <div className={inline ? 'forge-controls' : `forge-pop${result && !h3 ? ' comparing' : ''}`} role={inline ? 'region' : 'dialog'} aria-label={t('forge.button')} onKeyDown={e => { if (!inline && e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); setOpen(false); opener.current?.focus(); } }}>
      {h3 && tools && <div className="forge-row" role="group" aria-label={t('forge.tools')}>{tools}</div>}
      {choosing ? <>
        <button type="button" disabled={disabled} title={disabled ? t(!root ? 'forge.noBackend' : 'forge.noDialect') : undefined} onClick={() => setChoosing(false)}>{t('wildcards.llm')}</button>
        <button type="button" onClick={() => { setOpen(false); onWildcards?.(); }}>{t('wildcards.title')}</button>
      </> : <>
      <p className="forge-dialect">{t('forge.target')}: <strong>{dialect}</strong>{image && !nativeTarget(dialect) && <> · {t('forge.picture')}: <strong data-forge-picture>{image}</strong></>}</p>
      {h3 && <p className="forge-dialect" data-forge-run>{h3.mode} · {`${h3.duration} s`} · {`${h3.canvas.width}×${h3.canvas.height}`} · {t('forge.references', { count: String(h3.references.length) })}</p>}
      <ModelSelect forge={forge}/>
      {h3 && <>
        <div className="forge-row">{level('forge.detail', detail, setDetail)}{level('forge.creativity', creativity, setCreativity)}</div>
        <label className="forge-field"><span>{t('forge.shots')}</span><select data-forge-shots value={shots} onChange={e => setShots(e.target.value === 'Auto' ? 'Auto' : Number(e.target.value))}>{(forge.list?.shot_counts ?? ['Auto']).map(s => <option key={s} value={s}>{s === 'Auto' ? t('forge.shotsAuto') : s}</option>)}</select></label>
        {hasPictures && !labelledPictures && <p className="field-help" data-forge-auto-see>{t('forge.autoSee')}</p>}
        {hasPictures && labelledPictures && <label className="forge-check"><input type="checkbox" data-forge-see checked={see} onChange={e => setSee(e.target.checked)}/>{t('forge.see')}</label>}
        {castKeys.length > 0 && <fieldset className="forge-cast" data-forge-cast><legend>{t('forge.cast')}</legend>
          {castKeys.map(k => { const name = /^\d+$/.test(k) ? t('forge.role.character', { n: k }) : k.startsWith('pose:') ? `${t('forge.role.pose')} · ${t('forge.picture')} ${wireKey(k).slice(5)}` : t(`forge.role.${k}`); return <label key={k} className="forge-field"><span>{name} <button type="button" data-forge-read={k} disabled={!!reading || busy || !forge.model} onClick={() => void read(k)}>{t(reading === k ? 'forge.reading' : cards[k]?.trim() ? 'forge.readAgain' : 'forge.read')}</button></span>
            <textarea rows={2} maxLength={600} aria-label={t('forge.cardOf', { name })} value={cards[k] ?? ''} onChange={e => setCards({ ...cards, [k]: e.target.value })}/></label>; })}
        </fieldset>}
        {typeof shots === 'number' && Array.from({ length: shots }, (_, i) => <label key={i} className="forge-field"><span>{t('forge.shot', { n: String(i + 1) })}</span><textarea data-forge-shot={i + 1} rows={2} maxLength={4000} value={shotBriefs[i] ?? ''} onChange={e => setShotBriefs(Object.assign([...shotBriefs], { [i]: e.target.value }))}/></label>)}
      </>}
      <label className="forge-field"><span>{t(h3?.continuity ? 'forge.nextAction' : 'forge.brief')}</span><PromptTextarea data-forge-brief value={brief} maxLength={16384} onChange={e => setBrief(e.target.value)}/></label>
      {busy && (h3 || nativeTarget(dialect)) ? <button type="button" className="action-button" data-forge-cancel onClick={cancel}>{t('forge.cancel')}</button>
        : <button type="button" className="action-button" data-forge-write disabled={busy || !forge.model || (!brief.trim() && !(image && !nativeTarget(dialect)) && !(h3 && shotBriefs.some(s => s.trim())))} onClick={() => void write()}>{t(busy ? 'forge.writing' : result ? 'forge.regenerate' : 'forge.write')}</button>}
      {error && <p className="error-message" role="alert">{t(error)}</p>}
      {blind && <p className="forge-warning">{t('forge.blind')}</p>}
      {warnings.map((warning, i) => <p key={i} className="forge-warning">{warning}</p>)}
      {result && !h3 && <>
        {/* The prompt in the box and the rewrite, side by side: nothing changes until the new one is chosen. */}
        <div className="forge-compare" data-forge-compare>
          <label className="forge-field"><span>{t('forge.current')}</span><textarea data-forge-current readOnly value={value}/></label>
          <label className="forge-field forge-new"><span>{t('forge.new')}</span><PromptTextarea data-forge-result aria-label={t('forge.result')} value={result} onChange={e => setResult(e.target.value)}/></label>
        </div>
        <div className="forge-row">
          <button type="button" className="action-button" data-forge-apply onClick={() => { setUndo(value); onApply(result); setOpen(false); opener.current?.focus(); }}>{t('forge.useNew')}</button>
          <button type="button" data-forge-keep onClick={() => { setOpen(false); opener.current?.focus(); }}>{t('forge.keepCurrent')}</button>
        </div>
      </>}
      {result && h3 && <>
        <label className="forge-field"><span>{t('forge.result')}</span><PromptTextarea data-forge-result value={result} onChange={e => setResult(e.target.value)}/></label>
        {stale && <p className="forge-warning" role="status">{t('forge.stale')}</p>}
        <button type="button" className="action-button" data-forge-apply disabled={stale} onClick={() => { setUndo(value); onApply(result); setOpen(false); opener.current?.focus(); }}>{t('forge.apply')}</button>
      </>}
      {drafts.length > 0 && <div className="forge-row" role="group" aria-label={t('forge.history')}>{drafts.map((d, i) => <button key={i} type="button" data-forge-draft={i + 1} title={d.text} aria-pressed={result === d.text && draftFit === d.fit} onClick={() => { setResult(d.text); setDraftFit(d.fit); }}>{t('forge.draft', { n: String(i + 1) })}{d.fit !== fit && ' ⚠'}</button>)}</div>}
      </>}
    </div>}
  </div>;
}
