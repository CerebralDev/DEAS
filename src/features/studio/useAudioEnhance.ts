import { useEffect, useRef, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { oneOf, usePref } from '../../lib/preferences';
import { cancelAudioEnhanceInstall, installAudioEnhance, readAudioEnhanceInstallStatus, readAudioEnhanceCapabilities, type AudioEnhanceInstallStatus, type AudioEnhanceCapabilities, type AudioEnhanceIntent, type AudioEnhanceMode, type AudioEnhanceProfile, type AudioEnhanceDevice } from '../../lib/audioEnhance';

/** One live selection for both the controls and immutable submission snapshot. */
export function useAudioEnhance(scope: 'post' | 'tve') {
  const [mode, setMode] = usePref<AudioEnhanceMode>(`studio.audioEnhance.${scope}.mode`, 'off', oneOf('off', 'clean', 'deep', 'stems'));
  const [profile, setProfile] = usePref<AudioEnhanceProfile>(`studio.audioEnhance.${scope}.profile`, 'mix', oneOf('mix', 'speech'));
  const [device, setDevice] = usePref<AudioEnhanceDevice>(`studio.audioEnhance.${scope}.device`, 'auto', oneOf('auto', 'cpu', 'cuda'));
  const [capabilities, setCapabilities] = useState<AudioEnhanceCapabilities | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState<'deep' | 'stems' | null>(null);
  const [error, setError] = useState('');
  const [installStatus, setInstallStatus] = useState<AudioEnhanceInstallStatus | null>(null);
  const statusReading = useRef(false);
  const wasRunning = useRef(false);
  const live = useRef(true), probing = useRef(false), installingLock = useRef(false);
  useEffect(() => { live.current = true; return () => { live.current = false; }; }, []);
  async function probe(clearError = true) {
    if (!isTauri() || probing.current) return;
    probing.current = true; setChecking(true); if (clearError) setError('');
    try { const value = await readAudioEnhanceCapabilities(); if (live.current) setCapabilities(value); }
    catch (failure) { if (live.current) { setCapabilities(null); setError(String(failure)); } }
    finally { probing.current = false; if (live.current) setChecking(false); }
  }
  useEffect(() => { if (mode !== 'off' && !capabilities) void probe(); }, [mode]); // Restored preferences never imply engine readiness.
  async function readInstallStatus() {
    if (!isTauri() || statusReading.current) return;
    statusReading.current = true;
    try {
      const status = await readAudioEnhanceInstallStatus();
      if (!live.current) return;
      setInstallStatus(status);
      if (wasRunning.current && !status.running) {
        setInstalling(null); installingLock.current = false;
        await probe(false); window.dispatchEvent(new Event('deas:audio-enhance-installed'));
      }
      wasRunning.current = status.running;
    } catch (failure) { if (live.current) setError(String(failure)); }
    finally { statusReading.current = false; }
  }
  const activeInstall = installing ?? (installStatus?.running ? installStatus.mode as 'deep' | 'stems' : null);
  useEffect(() => {
    const refresh = () => { setCapabilities(null); void probe(false); };
    window.addEventListener('deas:audio-enhance-installed', refresh);
    return () => window.removeEventListener('deas:audio-enhance-installed', refresh);
  }, []);
  useEffect(() => {
    if (!activeInstall) return;
    const timer = setInterval(() => { void readInstallStatus(); }, 750);
    return () => clearInterval(timer);
  }, [activeInstall]);
  async function cancelInstall() {
    try { await cancelAudioEnhanceInstall(); }
    catch (failure) { if (live.current) setError(String(failure)); }
  }
  async function install(value: 'deep' | 'stems') {
    if (installingLock.current || activeInstall || probing.current || !isTauri()) return;
    installingLock.current = true; setInstalling(value); setError(''); setCapabilities(null);
    try { await installAudioEnhance(value); await probe(false); window.dispatchEvent(new Event('deas:audio-enhance-installed')); }
    catch (failure) { if (live.current) setError(String(failure)); }
    finally { installingLock.current = false; if (live.current) { setInstalling(null); void readInstallStatus(); } }
  }
  const available = mode !== 'off' && !checking && !activeInstall && !!capabilities?.[mode] && (mode !== 'deep' || profile === 'speech') && (mode !== 'stems' || device !== 'cuda' || capabilities?.cuda === true);
  const intent: AudioEnhanceIntent | undefined = mode === 'off' ? undefined : { mode, profile, ...(mode === 'stems' ? { device } : {}) };
  return { mode, setMode, profile, setProfile, device, setDevice, capabilities, checking, installing: activeInstall, installStatus, readInstallStatus, cancelInstall, error, probe, install, available, ready: mode === 'off' || available, intent,
    restore(value?: AudioEnhanceIntent) { setMode(value?.mode ?? 'off'); setProfile(value?.profile ?? 'mix'); setDevice(value?.device ?? 'auto'); } };
}
export type AudioEnhanceSettings = ReturnType<typeof useAudioEnhance>;
