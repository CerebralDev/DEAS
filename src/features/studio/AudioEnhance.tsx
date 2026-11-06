import { useEffect, useRef, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { useI18n } from '../../i18n/I18nProvider';
import { useAudioEnhance, type AudioEnhanceSettings } from './useAudioEnhance';
import { cancelAudioEnhance, exportEnhancedAudio, type AudioEnhanceMode, type AudioEnhanceProfile, type AudioEnhanceDevice, type AudioEnhanceSource } from '../../lib/audioEnhance';

/** Explicit post-processing of a retained source/result. The original is never replaced. */
type Props = { source?: AudioEnhanceSource; name?: string; scope: 'post' | 'tve'; disabled?: boolean; automatic?: boolean };
export function AudioEnhance(props: Props & { settings?: AudioEnhanceSettings }) {
  return props.settings ? <AudioEnhanceControls {...props} settings={props.settings}/> : <StandaloneAudioEnhance {...props}/>;
}
function StandaloneAudioEnhance(props: Props) {
  const settings = useAudioEnhance(props.scope);
  return <AudioEnhanceControls {...props} settings={settings}/>;
}
function AudioEnhanceControls({ source, name, scope, disabled = false, automatic = false, settings }: Props & { settings: AudioEnhanceSettings }) {
  const { t } = useI18n();
  const { mode, setMode, profile, setProfile, device, setDevice, capabilities, checking, probe, available, installing } = settings;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [saved, setSaved] = useState('');
  const [operationName, setOperationName] = useState('');

  const lock = useRef(false);
  const live = useRef(true);
  const sourceKey = JSON.stringify(source);
  useEffect(() => { void settings.readInstallStatus(); }, []);
  useEffect(() => { live.current = true; return () => { live.current = false; }; }, []);
  useEffect(() => { if (!lock.current) { setError(''); setSaved(''); setOperationName(''); } }, [sourceKey]);
  async function run() {
    if (lock.current || !source || mode === 'off' || !available || disabled) return;
    setOperationName(name ?? t('studio.audioEnhance.title'));
    lock.current = true; setBusy(true); setError(''); setSaved('');
    try {
      const result = await exportEnhancedAudio(source, mode, profile, t('studio.audioEnhance.save'), device);
      if (live.current && result) setSaved(result.path);
    } catch (failure) {
      if (live.current) setError(failure instanceof Error ? failure.message : String(failure));
    } finally { lock.current = false; if (live.current) setBusy(false); }
  }
  return <fieldset className="wb-group" data-audio-enhance={scope} disabled={disabled && !busy && !installing} aria-busy={busy || !!installing}>
    <legend>{t('studio.audioEnhance.title')}</legend>
    <div className="wb-options">
      <label className="wb-option"><span>{t('studio.audioEnhance.mode')}</span><select data-audio-enhance-mode disabled={busy || !!installing} onFocus={() => void probe()} onPointerDown={() => void probe()} value={mode} onChange={e => setMode(e.target.value as AudioEnhanceMode)}>
        {(['off', 'clean', 'deep', 'stems'] as const).map(value => <option key={value} value={value} disabled={value !== 'off' && !capabilities?.[value]}>{t(`studio.audioEnhance.${value}`)}</option>)}
      </select></label>
      <label className="wb-option"><span>{t('studio.audioEnhance.profile')}</span><select data-audio-enhance-profile disabled={busy || !!installing} value={profile} onChange={e => setProfile(e.target.value as AudioEnhanceProfile)}>
        <option value="mix">{t('studio.audioEnhance.mix')}</option><option value="speech">{t('studio.audioEnhance.speech')}</option>
      </select></label>
      {mode === 'stems' && <>
        <label className="wb-option"><span>{t('studio.audioEnhance.device')}</span><select data-audio-enhance-device disabled={busy || !!installing} value={device} onChange={e => setDevice(e.target.value as AudioEnhanceDevice)}>
          {(['auto', 'cpu', 'cuda'] as const).map(value => <option key={value} value={value} disabled={value === 'cuda' && capabilities?.cuda !== true}>{t(`studio.audioEnhance.device.${value}`)}</option>)}
        </select></label>
        <small data-audio-enhance-device-status>{t(capabilities?.cuda ? 'studio.audioEnhance.cudaCompatible' : 'studio.audioEnhance.cudaUnavailable')}</small>
        <small>{t('studio.audioEnhance.deviceHint')}</small>
      </>}
      {automatic && <small>{t('studio.audioEnhance.automatic')}</small>}
      <small>{t('studio.audioEnhance.hint')}</small>
      <details data-audio-enhance-engines><summary>{t('studio.audioEnhance.engines')}</summary>
      {(['deep', 'stems'] as const).map(value => !capabilities?.[value] && <button key={value} type="button" className="wb-dock-action" data-audio-enhance-install={value} disabled={busy || !!installing || checking || !isTauri()} onClick={() => { if (window.confirm(t(value === 'stems' ? 'studio.audioEnhance.stemsInstallConsent' : 'studio.audioEnhance.installConsent', { mode: t(`studio.audioEnhance.${value}`) }))) void settings.install(value); }}>{t('studio.audioEnhance.install', { mode: t(`studio.audioEnhance.${value}`) })}</button>)}
      </details>
      {installing && <><small role="status" data-audio-enhance-install-status>{t('studio.audioEnhance.installing', { mode: t(`studio.audioEnhance.${installing}`) })}{settings.installStatus?.running && <> · {t('studio.audioEnhance.installProgress', { stage: settings.installStatus.stage, downloaded: settings.installStatus.downloaded, total: settings.installStatus.total || '—' })}</>}</small><button type="button" className="wb-dock-action" data-audio-enhance-install-cancel onClick={() => void settings.cancelInstall()}>{t('studio.audioEnhance.installCancel')}</button></>}
      {settings.error && <small role="alert">{settings.error}</small>}
      {checking && <small role="status">{t('studio.audioEnhance.checking')}</small>}
      {busy && <small role="status">{t('studio.audioEnhance.processingSource', { name: operationName })}</small>}
      {name && <small data-audio-enhance-source>{name}</small>}
      {!source && !automatic && <small>{t('studio.audioEnhance.needsSource')}</small>}
      {capabilities && (!capabilities.deep || !capabilities.stems) && <small>{t('studio.audioEnhance.modelsMissing')}</small>}
      {capabilities && !capabilities.clean && <small role="status">{t('studio.audioEnhance.toolsMissing')}</small>}
      {mode === 'deep' && profile !== 'speech' && <small role="alert">{t('studio.audioEnhance.speechOnly')}</small>}
      <button type="button" className="wb-dock-action" data-audio-enhance-run disabled={busy || !source || !available || !isTauri()} onClick={() => void run()}>{t(busy ? 'studio.audioEnhance.working' : 'studio.audioEnhance.save')}</button>
      {busy && <button type="button" className="wb-dock-action" data-audio-enhance-cancel onClick={() => { void cancelAudioEnhance().catch(failure => { if (live.current) setError(String(failure)); }); }}>{t('studio.audioEnhance.cancel')}</button>}
      {error && <small role="alert">{operationName && `${operationName}: `}{error}</small>}
      {saved && <small role="status" data-audio-enhance-saved>{t('studio.audioEnhance.saved', { path: saved, name: operationName })}</small>}
    </div>
  </fieldset>;
}
