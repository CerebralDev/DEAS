import { useEffect, useRef, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { useI18n } from '../../i18n/I18nProvider';
import { discoverLlmModels, generateLlm, type LlmConfig, type LlmModel, type LlmProvider, type LlmResult } from '../../lib/llm';
import './llm.css';
import { recommendationKey, recommendedFirst } from '../../lib/llmRecommendations';
import { RecommendedModelOptions } from './RecommendedModelOptions';

const modelKey = (model: LlmModel) => JSON.stringify([model.provider, model.id]);

// A different environment gets fresh drafts, locks and result ownership immediately.
export function LlmPanel({ root }: { root: string }) {
  return <EnvironmentLlmPanel key={root} root={root} />;
}

function EnvironmentLlmPanel({ root }: { root: string }) {
  const { t } = useI18n();
  const native = isTauri();
  const [config, setConfig] = useState<LlmConfig | null>(null);
  const [saved, setSaved] = useState<LlmConfig | null>(null);
  const [models, setModels] = useState<LlmModel[]>([]);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [selected, setSelected] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [message, setMessage] = useState(false);
  const [result, setResult] = useState<LlmResult | null>(null);
  const [prompt, setPrompt] = useState('');
  const epoch = useRef(0);
  const lock = useRef(false);

  const refresh = async (token: number) => {
    try {
      const next = await discoverLlmModels(root);
      if (epoch.current !== token) return;
      setModels(next.models); setWarnings(next.warnings);
      setSelected(previous => next.models.some(m => modelKey(m) === previous) ? previous :
        recommendedFirst(next.models.map(m => ({ ...m, id: `${m.provider}:${m.id}`, value: modelKey(m) })))
          .find(m => recommendationKey(m.id))?.value ?? '');
    } catch (e) {
      if (epoch.current !== token) return;
      // A failed external refresh must not remove previously discovered local models.
      setModels(previous => previous.filter(model => model.provider === 'local'));
      setError(String(e));
    }
  };

  useEffect(() => {
    const token = ++epoch.current;
    if (native && root) {
      lock.current = true; setBusy(true);
      void (async () => {
        try {
          const value = await invoke<LlmConfig>('native_llm_read', { root });
          if (epoch.current !== token) return;
          setConfig(value); setSaved(value);
          await refresh(token);
        } catch (e) { if (epoch.current === token) setError(String(e)); }
        finally { if (epoch.current === token) { lock.current = false; setBusy(false); } }
      })();
    }
    return () => { ++epoch.current; lock.current = false; };
  }, [native, root]);

  const dirty = config?.provider !== saved?.provider || config?.baseUrl !== saved?.baseUrl || (config?.apiKey ?? '') !== (saved?.apiKey ?? '');
  const remote = config?.provider !== 'local';
  const valid = config !== null && (!remote || !!config.baseUrl.trim()) && (config.provider !== 'openaiapi' || !!config.apiKey?.trim());
  const model = models.find(value => modelKey(value) === selected);
  const edit = (next: LlmConfig) => {
    if (lock.current) return;
    setConfig(next); setModels([]); setWarnings([]); setSelected('');
    setMessage(false); setResult(null); setError('');
  };
  const request = async (action: 'save' | 'refresh' | 'test') => {
    if (lock.current || !config || !native || !root || (action === 'save' ? !dirty || !valid : dirty || (action === 'test' && !model))) return;
    const token = ++epoch.current;
    lock.current = true; setBusy(true); setError(''); setMessage(false); setResult(null);
    if (action !== 'test') { setWarnings([]); }
    try {
      if (action === 'save') {
        const next = await invoke<LlmConfig>('native_llm_write', { root, config });
        if (epoch.current !== token) return;
        setConfig(next); setSaved(next); setMessage(true);
        window.dispatchEvent(new Event('deas:llm-changed'));
        await refresh(token);
      } else if (action === 'refresh') {
        window.dispatchEvent(new Event('deas:llm-changed'));
        await refresh(token);
      } else if (model) {
        const next = await generateLlm(root, { model: { id: model.id, provider: model.provider }, system: t('llm.test.system'), prompt: prompt.trim() || t('llm.test.defaultPrompt'), maxTokens: model.provider === 'openaiapi' ? 4096 : 64, temperature: 0.2 });
        if (epoch.current === token) setResult(next);
      }
    } catch (e) { if (epoch.current === token) setError(String(e)); }
    finally { if (epoch.current === token) { lock.current = false; setBusy(false); } }
  };
  return <section className="aio-detail startup-panel llm-panel" aria-label={t('llm.title')}>
    <h3>{t('llm.title')}</h3>
    <p className="aio-help">{t('llm.help')}</p>
    {!config ? <p className="aio-help">{t(!native ? 'llm.native' : !root ? 'llm.root' : error ? 'llm.unavailable' : 'llm.loading')}</p> : <>
      <fieldset disabled={busy} className="llm-fields">
        <label className="startup-field"><span>{t('llm.provider')}</span><select value={config.provider} onChange={e => { const provider = e.target.value as LlmProvider; const { apiKey, ...rest } = config; const next = { ...rest, provider, baseUrl: provider === 'openaiapi' ? 'https://api.openai.com/v1' : config.baseUrl }; const sameOfficialHost = config.baseUrl.replace(/\/$/, '') === 'https://api.openai.com/v1'; edit((provider === 'openai' || (provider === 'openaiapi' && sameOfficialHost)) && apiKey ? { ...next, apiKey } : next); }}>
          <option value="local">{t('llm.provider.local')}</option><option value="ollama">{t('llm.provider.ollama')}</option><option value="openaiapi">{t('llm.provider.openaiapi')}</option><option value="openai">{t('llm.provider.openai')}</option>
        </select></label>
        {remote && <label className="startup-field"><span>{t('llm.baseUrl')}</span><input type="url" readOnly={config.provider === 'openaiapi'} value={config.baseUrl} onChange={e => edit({ ...config, baseUrl: e.target.value })} /></label>}
        {(config.provider === 'openai' || config.provider === 'openaiapi') && <label className="startup-field"><span>{t(config.provider === 'openaiapi' ? 'llm.apiKey.required' : 'llm.apiKey')}</span><input type="password" autoComplete="off" value={config.apiKey ?? ''} onChange={e => { const apiKey = e.target.value.trim(); const { apiKey: _, ...rest } = config; edit(apiKey ? { ...rest, apiKey } : rest); }} /></label>}
      </fieldset>
      <div className="aio-actions"><button type="button" className="aio-confirm-button" disabled={busy || !dirty || !valid} onClick={() => void request('save')}>{t('llm.save')}</button><button type="button" disabled={busy || dirty} onClick={() => void request('refresh')}>{t('llm.refresh')}</button></div>
      {config.provider === 'openaiapi' && <p className="aio-help">{t('llm.openai.help')}</p>}
      {dirty && <p className="aio-help">{t('llm.dirty')}</p>}
      {dirty && !valid && <p className="aio-help">{t('llm.invalid')}</p>}
      <label className="startup-field"><span>{t('llm.test.model')}</span><select value={selected} disabled={busy || dirty} onChange={e => { setSelected(e.target.value); setResult(null); }}>
        <option value="">{t('llm.test.choose')}</option>
        {models.some(m => recommendationKey(`${m.provider}:${m.id}`)) ? <RecommendedModelOptions models={models.map(m => ({
          id: `${m.provider}:${m.id}`, value: modelKey(m), label: `${m.name} · ${t(m.provider === 'local' ? 'llm.models.local' : `llm.provider.${m.provider}`)}`,
        }))}/> : (['local', 'ollama', 'openaiapi', 'openai'] as const).map(provider => {
          const group = models.filter(value => value.provider === provider);
          return group.length > 0 && <optgroup key={provider} label={t(provider === 'local' ? 'llm.models.local' : `llm.provider.${provider}`)}>
            {group.map(value => <option key={modelKey(value)} value={modelKey(value)}>{value.name === value.id ? value.id : `${value.name} · ${value.id}`}</option>)}
          </optgroup>;
        })}
      </select></label>
      {!busy && !dirty && models.length === 0 && <p className="aio-help">{t('llm.models.empty')}</p>}
      {warnings.map((warning, index) => <p key={index} role="status" className="aio-help llm-warning">{warning}</p>)}
      <label className="startup-field"><span>{t('llm.test.prompt')}</span><input value={prompt} placeholder={t('llm.test.defaultPrompt')} disabled={busy} onChange={e => { setPrompt(e.target.value); setResult(null); }} /></label>
      <div className="aio-actions"><button type="button" disabled={busy || dirty || !model} onClick={() => void request('test')}>{t('llm.test')}</button></div>
      {busy && <p role="status" className="aio-help">{t('llm.busy')}</p>}
    </>}
    {message && <p role="status" className="aio-help">{t('llm.saved')}</p>}
    {result && <div className="llm-result" role="status"><p>{t('llm.backend')}: {t(result.backend === 'local' ? 'llm.models.local' : `llm.provider.${result.backend}`)}</p><pre>{result.text}</pre></div>}
    {error && <p role="alert" className="aio-error">{t('llm.error')}: {error}</p>}
  </section>;
}
