import { Fragment, useEffect, useRef, useState, type ClipboardEvent, type ReactNode } from 'react';
import { useI18n } from '../../i18n/I18nProvider';
import { PromptTextarea } from '../studio/PromptTextarea';
import { convertFileSrc } from '@tauri-apps/api/core';
import { cancelChat, chatSettings, deleteChat, getChat, listChats, recordRender, renameChat, saveChatSettings, sendChat, workingContext, type ChatEvent, type ChatHit, type ChatMessage, type ChatRenderOutcome, type ChatRenderRequest, type ChatSettings, type ChatStudio, type ChatSummary, type RenderResult, type WrittenPrompt } from '../../lib/chat';
import { pollWorkbench, stopWorkbenchJob, type JobStatus } from '../../lib/jobs';
import { forgeError, nativeModelId } from '../../lib/forge';
import { isBoolean, isString, usePref } from '../../lib/preferences';
import { ModelSelect, useForgeModels } from '../forge/ForgeButton';
import { prepare } from '../forge/ReferenceImages';
import './chat.css';

const MAX_PICTURES = 4;

/** Light markdown: headings, lists, code blocks, **bold**, `code` and [n] citations. Built as elements, never HTML. */
function Rich({ text, citations }: { text: string; citations: ChatHit[] }) {
  const inline = (line: string, key: string): ReactNode[] => line.split(/(`[^`]+`|\*\*[^*]+\*\*|\[\d+\])/g).map((part, i) => {
    if (/^`[^`]+`$/.test(part)) return <code key={`${key}-${i}`}>{part.slice(1, -1)}</code>;
    if (/^\*\*[^*]+\*\*$/.test(part)) return <strong key={`${key}-${i}`}>{part.slice(2, -2)}</strong>;
    const cite = /^\[(\d+)\]$/.exec(part);
    const hit = cite && citations.find(c => c.n === Number(cite[1]));
    if (hit) return <a key={`${key}-${i}`} className="chat-cite" href={hit.url} target="_blank" rel="noreferrer" title={hit.title}>{part}</a>;
    return <Fragment key={`${key}-${i}`}>{part}</Fragment>;
  });
  const blocks: ReactNode[] = [];
  const lines = text.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (line.trim().startsWith('```')) {
      const body: string[] = [];
      for (i++; i < lines.length && !lines[i].trim().startsWith('```'); i++) body.push(lines[i]);
      blocks.push(<pre key={i}><code>{body.join('\n')}</code></pre>);
      continue;
    }
    const head = /^(#{1,4})\s+(.*)$/.exec(line);
    if (head) { blocks.push(<p key={i} className="chat-h">{inline(head[2], `h${i}`)}</p>); continue; }
    if (/^\s*([-*]|\d+[.)])\s+/.test(line)) {
      const items: string[] = [];
      const ordered = /^\s*\d+[.)]/.test(line);
      for (; i < lines.length && /^\s*([-*]|\d+[.)])\s+/.test(lines[i]); i++) items.push(lines[i].replace(/^\s*([-*]|\d+[.)])\s+/, ''));
      i--;
      const List = ordered ? 'ol' : 'ul';
      blocks.push(<List key={i}>{items.map((it, k) => <li key={k}>{inline(it, `l${i}-${k}`)}</li>)}</List>);
      continue;
    }
    if (line.trim()) blocks.push(<p key={i}>{inline(line, `p${i}`)}</p>);
  }
  return <div className="chat-rich">{blocks}</div>;
}

/** A chat render in progress, shown on its prompt card. */
interface Draw { state: 'starting' | 'queued' | 'running'; value?: number; max?: number; preview?: string }

/** The picture a render made (or why it did not), and its live progress while it draws. */
function Picture({ prompt, draw, onRedraw }: { prompt: WrittenPrompt; draw?: Draw; onRedraw?: () => void }) {
  const { t } = useI18n();
  const pic = prompt.picture;
  if (draw) return <div className="chat-drawing" data-chat-drawing={draw.state}>
    {draw.preview && <img src={draw.preview} alt=""/>}
    <span className="chat-status">{draw.state === 'running' && draw.max ? t('chat.render.progress', { value: String(draw.value ?? 0), max: String(draw.max) }) : t(`chat.render.${draw.state}`)}</span>
  </div>;
  if (!prompt.render) return null;
  return <>
    {pic?.path && <figure className="chat-picture" data-chat-picture-done>
      <img src={convertFileSrc(pic.path)} alt={prompt.prompt.slice(0, 160)}/>
      <figcaption>{[`${pic.width}×${pic.height}`, pic.checkpoint.replace(/\.(safetensors|gguf|ckpt)$/i, ''), pic.seed != null ? t('chat.render.seed', { seed: String(pic.seed) }) : '', pic.ms ? `${(pic.ms / 1000).toFixed(1)} s` : ''].filter(Boolean).join(' · ')}</figcaption>
    </figure>}
    {pic?.error && <p className="forge-note chat-render-error" data-chat-render-error>{t('chat.render.notDrawn', { error: pic.error })}</p>}
    {onRedraw && pic && <button type="button" className="forge-ghost" data-chat-redraw onClick={onRedraw}>{t('chat.render.again')}</button>}
  </>;
}

function PromptCard({ prompt, onSend, draw, onRedraw }: { prompt: WrittenPrompt; onSend: (positive: string, negative: string) => void; draw?: Draw; onRedraw?: () => void }) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);
  const copy = async () => { try { await navigator.clipboard.writeText(prompt.negative ? `${prompt.prompt}\n\n${t('forge.negativeShort')}: ${prompt.negative}` : prompt.prompt); setCopied(true); setTimeout(() => setCopied(false), 1500); } catch { /* the text is on screen to copy by hand */ } };
  return <div className="chat-prompt" data-chat-prompt={prompt.target}>
    <div className="chat-prompt-head"><strong>{prompt.label}</strong>
      <div className="forge-actions">
        <button type="button" className="forge-ghost" onClick={() => void copy()}>{t(copied ? 'forge.copied' : 'chat.copy')}</button>
        <button type="button" className="forge-ghost" data-chat-send-prompt onClick={() => onSend(prompt.prompt, prompt.negative ?? '')}>{t('chat.toGenerate')}</button>
      </div>
    </div>
    <pre>{prompt.prompt}</pre>
    {prompt.negative && <><span className="forge-note">{t('forge.negativeShort')}</span><pre className="chat-prompt-neg">{prompt.negative}</pre></>}
    <Picture prompt={prompt} draw={draw} onRedraw={onRedraw}/>
  </div>;
}

/** A finished message: prompts as cards (their text is already in the reply), citations, what it looked at. */
function Bubble({ m, onSend, draws = {}, onRedraw }: { m: ChatMessage; onSend: (p: string, n: string) => void; draws?: Record<string, Draw>; onRedraw?: (i: number) => void }) {
  const { t } = useI18n();
  // The reply carries the prompt text for the model's history; the card shows it, so the body drops it.
  const body = m.prompts.reduce((text, p) => {
    const at = text.indexOf(`${p.label} prompt, written by`);
    return at >= 0 ? text.slice(0, at).trim() : text;
  }, m.content);
  return <div className={`chat-msg ${m.role}`} data-chat-msg={m.role}>
    {m.pictures > 0 && <span className="forge-note">{t('chat.picturesSent', { count: String(m.pictures) })}</span>}
    {body && (m.role === 'assistant' ? <Rich text={body} citations={m.citations}/> : <p className="chat-user-text">{body}</p>)}
    {m.prompts.map((p, i) => <PromptCard key={i} prompt={p} onSend={onSend} draw={draws[`${m.at}:${i}`]} onRedraw={onRedraw && (() => onRedraw(i))}/>)}
    {m.citations.length > 0 && <ol className="chat-sources">{m.citations.map(c => <li key={c.n}><a href={c.url} target="_blank" rel="noreferrer">{c.title}</a> <span>{c.domain}</span></li>)}</ol>}
    {m.tools.length > 0 && <p className="chat-tools">{m.tools.join(' · ')}</p>}
  </div>;
}

/**
 * Chat: PromptForge's chat, natively. A streamed conversation with the LLM that can write prompts with the image
 * models' own writers, read how any model (or DEAS) works, look tags up, and search the web when switched on.
 */
export function Chat({ endpoint, root, active = true, onSend, studio, onRender }: { endpoint: string; root?: string; active?: boolean; onSend: (positive: string, negative: string) => void; /** What Generate can draw with; absent = chat cannot render. */ studio?: ChatStudio; /** Draws a prompt with Generate's settings. */ onRender?: (ask: ChatRenderRequest) => Promise<ChatRenderOutcome> }) {
  const { t } = useI18n();
  const forge = useForgeModels(endpoint, root, active);
  const [chats, setChats] = useState<ChatSummary[]>([]);
  const [id, setId] = useState<string | undefined>();
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = usePref('chat.draft', '', isString);
  const [pictures, setPictures] = useState<string[]>([]);
  const [search, setSearch] = usePref('chat.search', false, isBoolean);
  const [includeWork, setIncludeWork] = usePref('chat.includeWork', false, isBoolean);
  const [settings, setSettings] = useState<ChatSettings | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [busy, setBusy] = useState(false);
  const [opening, setOpening] = useState(false);
  const pending = useRef(false);
  const sending = useRef(false);
  const revision = useRef(0);
  const latestRoot = useRef(root);
  latestRoot.current = root;
  const [live, setLive] = useState<{ text: string; status: string; detail: string | null; prompts: WrittenPrompt[]; searched: ChatHit[] | null; searchDetail: string | null } | null>(null);
  const [error, setError] = useState('');
  const [renaming, setRenaming] = useState<{ id: string; title: string } | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const thread = useRef<HTMLDivElement>(null);
  const current = useRef<string | undefined>(undefined);
  const [draws, setDraws] = useState<Record<string, Draw>>({});
  /** The Generate job a chat render is waiting on, so Stop stops it. */
  const drawing = useRef<JobStatus | null>(null);
  const stopDrawing = useRef(false);

  /** Draws one render prompt with Generate, follows the job, and records the picture (or why not) on the message. */
  const draw = async (chatId: string, m: ChatMessage, i: number, valid: () => boolean) => {
    const p = m.prompts[i];
    if (!p?.render || !onRender || !root) return;
    const key = `${m.at}:${i}`;
    const show = (d?: Draw) => { if (valid()) setDraws(all => { const next = { ...all }; if (d) next[key] = d; else delete next[key]; return next; }); };
    const started = Date.now();
    const fail = (error: string): RenderResult => ({ width: 0, height: 0, checkpoint: studio?.checkpoint ?? '', error });
    show({ state: 'starting' });
    let result: RenderResult;
    try {
      const outcome = await onRender({ id: crypto.randomUUID(), positive: p.prompt, negative: p.negative ?? '', aspect: p.render.aspect, resolution: p.render.resolution });
      if ('error' in outcome) result = fail(outcome.error);
      else {
        let job = outcome.job;
        drawing.current = job;
        if (stopDrawing.current) job = await stopWorkbenchJob(job).catch(() => job);
        // Generate's own poller watches the same job; this one only reads it. Bounded so a lost job cannot spin forever.
        while (['queued', 'running', 'unknown'].includes(job.state) && Date.now() - started < 30 * 60_000) {
          show({ state: job.state === 'running' ? 'running' : 'queued', value: job.progress?.value, max: job.progress?.max, preview: job.preview });
          await new Promise(r => setTimeout(r, 750));
          const found = (await pollWorkbench()).jobs.find(j => j.id === job.id);
          if (!found) { job = { ...job, state: 'failed', error: t('chat.render.lost') }; break; }
          job = found; drawing.current = job;
        }
        const out = job.outputs.find(o => !o.format?.startsWith('video/'));
        if (job.state === 'succeeded' && out && job.outputRoot) {
          const sep = job.outputRoot.includes('\\') ? '\\' : '/';
          const path = [job.outputRoot.replace(/[\\/]+$/, ''), ...out.subfolder.split(/[\\/]/).filter(Boolean), out.filename].join(sep);
          result = { path, width: outcome.width, height: outcome.height, seed: outcome.seed, checkpoint: outcome.checkpoint, ms: job.metadata?.generationMs ?? Date.now() - started };
        } else result = fail(job.state === 'cancelled' ? t('chat.render.stopped') : job.state === 'succeeded' ? t('chat.render.noFile') : job.error || (['queued', 'running', 'unknown'].includes(job.state) ? t('chat.render.timeout') : t('chat.render.failed')));
      }
    } catch (e) { result = fail(e instanceof Error ? e.message : String(e)); }
    finally { drawing.current = null; }
    try { const saved = await recordRender(root, chatId, m.at, i, result); if (valid()) setMessages(ms => ms.map(x => x.role === 'assistant' && x.at === m.at ? saved : x)); }
    catch (e) { if (valid()) setError(forgeError(e)); }
    show(undefined);
  };
  const redraw = (m: ChatMessage, i: number) => {
    const chatId = current.current;
    if (!chatId || sending.current) return;
    const request = revision.current;
    const valid = () => revision.current === request && latestRoot.current === root;
    sending.current = true; stopDrawing.current = false; setBusy(true); setError('');
    void draw(chatId, m, i, valid).finally(() => { if (valid()) { sending.current = false; setBusy(false); } });
  };

  const rootRevision = useRef(0);
  useEffect(() => {
    ++revision.current;
    current.current = undefined; pending.current = false; sending.current = false;
    setId(undefined); setMessages([]); setLive(null); setPictures([]); setError('');
    setBusy(false); setOpening(false); setChats([]); setSettings(null);
    setRenaming(null); setConfirmDelete(null); setShowSettings(false);
    return () => { ++revision.current; ++rootRevision.current; };
  }, [root]);
  useEffect(() => { if (active) forge.load(); }, [root, endpoint, forge.rev, active]); // eslint-disable-line react-hooks/exhaustive-deps
  const refresh = () => {
    const request = rootRevision.current;
    const valid = () => request === rootRevision.current && latestRoot.current === root;
    if (root) listChats(root).then(c => { if (valid()) setChats(c); }, e => { if (valid()) setError(forgeError(e)); });
  };
  useEffect(() => {
    if (!active || !root) return;
    let alive = true;
    refresh();
    chatSettings(root).then(s => { if (alive) setSettings(s); }, () => { if (alive) setSettings({ search: 'off', searxngUrl: '' }); });
    return () => { alive = false; };
  }, [active, root]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => { const el = thread.current; if (el) el.scrollTop = el.scrollHeight; }, [messages, live?.text, live?.prompts.length]);

  const open = async (next?: string) => {
    if (sending.current) return;
    const request = ++revision.current;
    const valid = () => revision.current === request && latestRoot.current === root;
    setError(''); setLive(null); setPictures([]); setConfirmDelete(null); setMessages([]);
    current.current = next; setId(next);
    pending.current = !!next && !!root; setOpening(pending.current);
    if (!next || !root) return;
    try { const chat = await getChat(root, next); if (valid()) setMessages(chat.messages); }
    catch (e) { if (valid()) { current.current = undefined; setId(undefined); setError(forgeError(e)); } }
    finally { if (valid()) { pending.current = false; setOpening(false); } }
  };
  const searchOn = !!settings && settings.search !== 'off';

  const send = async () => {
    const text = draft.trim();
    if (!root || !forge.model || sending.current || pending.current || (!text && !pictures.length)) return;
    const request = revision.current;
    const valid = () => revision.current === request && latestRoot.current === root;
    sending.current = true; stopDrawing.current = false;
    const mine: ChatMessage = { role: 'user', content: text, pictures: pictures.length, prompts: [], citations: [], tools: [], at: Date.now() };
    setMessages(m => [...m, mine]); setDraft(''); setError(''); setBusy(true);
    const sentPictures = pictures; setPictures([]);
    setLive({ text: '', status: 'thinking', detail: null, prompts: [], searched: null, searchDetail: null });
    const onEvent = (e: ChatEvent) => {
      if (!valid()) return;
      if (e.event === 'chat') { current.current = e.id; setId(e.id); }
      else if (e.event === 'text') setLive(l => l && { ...l, text: l.text + e.text });
      else if (e.event === 'status') setLive(l => l && { ...l, status: e.state, detail: e.detail });
      else if (e.event === 'prompt') setLive(l => l && { ...l, prompts: [...l.prompts, e.prompt] });
      else if (e.event === 'searched') setLive(l => l && { ...l, searched: e.results, searchDetail: e.detail });
    };
    try {
      const done = await sendChat(root, { id: current.current, message: text, model: nativeModelId(forge.model), pictures: sentPictures, context: includeWork ? workingContext() || undefined : undefined, search: searchOn && search,
        ...(studio && onRender ? { studio: { ...studio, endpoint, writerModel: forge.model } } : {}) }, onEvent);
      if (valid()) setMessages(m => [...m, done]);
      // A render_picture answer is drawn now, by Generate; the turn stays busy (and Stop stops it) until it is done.
      const chatId = current.current;
      if (chatId && valid()) {
        setLive(null);
        for (let i = 0; i < done.prompts.length; i++) if (done.prompts[i].render && !stopDrawing.current) await draw(chatId, done, i, valid);
      }
    } catch (e) {
      if (!valid()) return;
      setError(forgeError(e));
      // The question is saved before the answer; reload so the thread matches the file.
      if (current.current) {
        try { const chat = await getChat(root, current.current); if (valid()) setMessages(chat.messages); }
        catch { /* Keep the original send error visible. */ }
      }
    } finally { if (valid()) { sending.current = false; setBusy(false); setLive(null); refresh(); } }
  };
  const stop = () => {
    stopDrawing.current = true;
    if (drawing.current) { void stopWorkbenchJob(drawing.current).catch(e => setError(forgeError(e))); return; }
    if (current.current) void cancelChat(current.current);
  };

  const addPictures = async (files: File[]) => {
    const room = MAX_PICTURES - pictures.length;
    const usable = files.filter(f => f.type.startsWith('image/')).slice(0, Math.max(0, room));
    if (files.length > room) setError(t('chat.tooManyPictures', { max: String(MAX_PICTURES) }));
    try { const added = await Promise.all(usable.map(prepare)); setPictures(p => [...p, ...added]); }
    catch { setError(t('forge.refs.unreadable')); }
  };
  const paste = (e: ClipboardEvent) => {
    const files = [...e.clipboardData.files].filter(f => f.type.startsWith('image/'));
    if (files.length) { e.preventDefault(); void addPictures(files); }
  };
  const statusText = (s: string, d: string | null) => t(`chat.status.${s}`, { detail: d ?? '' });

  if (!root) return <section className="chat-tab"><p className="field-help">{t('forge.noBackend')}</p></section>;
  return <section className="chat-tab" aria-label={t('studio.tab.chat')}>
    <aside className="forge-card chat-side">
      <button type="button" className="action-button" data-chat-new disabled={busy} onClick={() => void open(undefined)}>{t('chat.new')}</button>
      <nav className="chat-list" aria-label={t('chat.history')}>
        {chats.map(c => <div key={c.id} className={`chat-row${c.id === id ? ' on' : ''}`} data-chat-row={c.id}>
          {renaming?.id === c.id
            ? <input type="text" autoFocus aria-label={t('chat.rename')} value={renaming.title} maxLength={120}
              onChange={e => setRenaming({ id: c.id, title: e.target.value })}
              onKeyDown={e => { if (e.key === 'Enter') { void renameChat(root, c.id, renaming.title).then(refresh, err => setError(forgeError(err))); setRenaming(null); } else if (e.key === 'Escape') setRenaming(null); }}
              onBlur={() => setRenaming(null)}/>
            : <button type="button" className="chat-row-open" disabled={busy} onClick={() => void open(c.id)}>{c.title}</button>}
          <div className="chat-row-actions">
            <button type="button" className="forge-ghost" aria-label={t('chat.rename')} title={t('chat.rename')} onClick={() => setRenaming({ id: c.id, title: c.title })}>✎</button>
            {confirmDelete === c.id
              ? <button type="button" className="forge-ghost danger" data-chat-delete-confirm onClick={() => { void deleteChat(root, c.id).then(() => { if (c.id === id) void open(undefined); refresh(); }, err => setError(forgeError(err))); setConfirmDelete(null); }}>{t('chat.deleteConfirm')}</button>
              : <button type="button" className="forge-ghost" aria-label={t('chat.delete')} title={t('chat.delete')} disabled={busy && c.id === id} onClick={() => setConfirmDelete(c.id)}>×</button>}
          </div>
        </div>)}
        {!chats.length && <p className="forge-note">{t('chat.noHistory')}</p>}
      </nav>
    </aside>
    <div className="forge-card chat-main">
      <div className="forge-card-head chat-head">
        <h3 className="forge-card-title">{t('studio.tab.chat')}</h3>
        <div className="chat-model"><ModelSelect forge={forge}/></div>
        <button type="button" className="forge-ghost" data-chat-settings aria-expanded={showSettings} onClick={() => setShowSettings(!showSettings)}>{t('chat.searchSettings')}</button>
      </div>
      {showSettings && settings && <SearchSettings root={root} settings={settings} onSaved={s => { setSettings(s); setShowSettings(false); }}/>}
      <div className="chat-thread" ref={thread} data-chat-thread>
        {!messages.length && !live && <div className="forge-empty"><strong>{t('chat.emptyTitle')}</strong><span>{t('chat.emptyBody')}</span></div>}
        {messages.map((m, i) => <Bubble key={i} m={m} onSend={onSend} draws={draws} onRedraw={onRender && !busy && !studio?.blocked ? k => redraw(m, k) : undefined}/>)}
        {live && <div className="chat-msg assistant live" aria-live="polite" data-chat-live>
          {live.searched && <p className="forge-note">{live.searched.length ? t('chat.searchedCount', { count: String(live.searched.length) }) : live.searchDetail}</p>}
          {live.text ? <Rich text={live.text} citations={live.searched ?? []}/> : null}
          {live.prompts.map((p, i) => <PromptCard key={i} prompt={p} onSend={onSend}/>)}
          <p className="chat-status">{statusText(live.status, live.detail)}</p>
        </div>}
      </div>
      {error && <p className="error-message" role="alert">{t(error)}</p>}
      {pictures.length > 0 && <div className="chat-pictures">{pictures.map((p, i) => <span key={i} className="chat-pic"><img src={p} alt=""/><button type="button" aria-label={t('chat.removePicture')} onClick={() => setPictures(pictures.filter((_, k) => k !== i))}>×</button></span>)}</div>}
      <div className="chat-compose">
        <PromptTextarea data-chat-input aria-label={t('chat.message')} value={draft} maxLength={32000} rows={Math.min(8, Math.max(2, draft.split('\n').length))} placeholder={t('chat.placeholder')}
          onChange={e => setDraft(e.target.value)} onPaste={paste}
          onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); void send(); } }}/>
        <div className="chat-compose-bar">
          <label className="chat-attach forge-ghost">
            <input type="file" data-chat-picture accept="image/png,image/jpeg,image/webp" multiple onChange={e => { if (e.target.files) void addPictures([...e.target.files]); e.target.value = ''; }}/>
            {t('chat.picture')}
          </label>
          {searchOn && <button type="button" className="forge-chip" data-chat-search aria-pressed={search} title={t(settings!.search === 'duckduckgo' ? 'chat.searchDdgNote' : 'chat.searchSearxNote')} onClick={() => setSearch(!search)}>{t('chat.search')}</button>}
          <label className="forge-switch chat-include" title={t('chat.includeHelp')}><input type="checkbox" data-chat-include checked={includeWork} onChange={e => setIncludeWork(e.target.checked)}/><span>{t('chat.include')}</span></label>
          {busy
            ? <button type="button" className="action-button" data-chat-stop onClick={stop}>{t('chat.stop')}</button>
            : <button type="button" className="action-button" data-chat-send disabled={opening || !forge.model || (!draft.trim() && !pictures.length)} onClick={() => void send()}>{t('chat.send')}</button>}
        </div>
      </div>
    </div>
  </section>;
}

function SearchSettings({ root, settings, onSaved }: { root: string; settings: ChatSettings; onSaved: (s: ChatSettings) => void }) {
  const { t } = useI18n();
  const [mode, setMode] = useState(settings.search);
  const [url, setUrl] = useState(settings.searxngUrl);
  const [error, setError] = useState('');
  return <div className="chat-settings" data-chat-settings-panel>
    <div className="forge-group" role="radiogroup" aria-label={t('chat.searchSettings')}>
      {(['off', 'duckduckgo', 'searxng'] as const).map(m => <label key={m} className="forge-switch"><input type="radio" name="chat-search" data-chat-search-mode={m} checked={mode === m} onChange={() => setMode(m)}/><span>{t(`chat.search.${m}`)}</span></label>)}
    </div>
    {mode === 'searxng' && <label className="forge-field"><span>{t('chat.searxngUrl')}</span><input type="text" data-chat-searxng value={url} placeholder="http://127.0.0.1:8080" onChange={e => setUrl(e.target.value)}/></label>}
    <p className="forge-note">{t(mode === 'duckduckgo' ? 'chat.searchDdgNote' : mode === 'searxng' ? 'chat.searchSearxNote' : 'chat.searchOffNote')}</p>
    {error && <p className="error-message" role="alert">{t(error)}</p>}
    <button type="button" className="action-button" data-chat-settings-save onClick={() => void saveChatSettings(root, { search: mode, searxngUrl: url.trim() }).then(onSaved, e => setError(forgeError(e)))}>{t('chat.save')}</button>
  </div>;
}
