import { invoke } from '@tauri-apps/api/core';
import type { JobExport } from './jobs';

export type AudioEnhanceMode = 'off' | 'clean' | 'deep' | 'stems';
export type AudioEnhanceProfile = 'mix' | 'speech';
export type AudioEnhanceDevice = 'auto' | 'cpu' | 'cuda';
export interface AudioEnhanceIntent { mode: Exclude<AudioEnhanceMode, 'off'>; profile: AudioEnhanceProfile; device?: AudioEnhanceDevice }
export const installAudioEnhance = (mode: 'deep' | 'stems'): Promise<AudioEnhanceCapabilities> => invoke('native_audio_enhance_install', { mode, confirmed: true });
export type AudioEnhanceSource = { sourceId: string } | { jobId: string; index: number };
export interface AudioEnhanceInstallStatus { running: boolean; mode: string; stage: string; downloaded: number; total: number }
export async function readAudioEnhanceInstallStatus(): Promise<AudioEnhanceInstallStatus> {
  const value = await invoke<AudioEnhanceInstallStatus>('native_audio_enhance_install_status');
  if (!value || typeof value.running !== 'boolean' || typeof value.mode !== 'string' || value.mode.length > 32 || typeof value.stage !== 'string' || value.stage.length > 256 || !Number.isSafeInteger(value.downloaded) || value.downloaded < 0 || !Number.isSafeInteger(value.total) || value.total < 0 || value.running && !['deep', 'stems'].includes(value.mode)) throw new Error('Invalid audio enhancement installation status');
  return value;
}
export const cancelAudioEnhanceInstall = (): Promise<void> => invoke('native_audio_enhance_install_cancel');
export const cancelAudioEnhance = (): Promise<void> => invoke('native_audio_enhance_cancel');
export interface AudioEnhanceCapabilities { clean: boolean; deep: boolean; stems: boolean; cuda?: boolean; gpuRepair?: boolean }
export async function readAudioEnhanceCapabilities(): Promise<AudioEnhanceCapabilities> {
  const value = await invoke<AudioEnhanceCapabilities>('native_audio_enhance_capabilities');
  if (!value || !['clean', 'deep', 'stems'].every(k => typeof value[k as keyof AudioEnhanceCapabilities] === 'boolean') || ['cuda', 'gpuRepair'].some(k => value[k as keyof AudioEnhanceCapabilities] !== undefined && typeof value[k as keyof AudioEnhanceCapabilities] !== 'boolean')) throw new Error('Invalid audio enhancement capabilities');
  return { ...value, cuda: value.cuda ?? false, gpuRepair: value.gpuRepair ?? false };
}
/** Native-held source authority only; the frontend supplies neither paths nor executable/filter strings. */
export async function exportEnhancedAudio(source: AudioEnhanceSource, mode: Exclude<AudioEnhanceMode, 'off'>, profile: AudioEnhanceProfile, title: string, device: AudioEnhanceDevice = 'auto'): Promise<JobExport | null> {
  const value = await invoke<JobExport | null>('native_audio_enhance_export', { request: { ...source, mode, profile, ...(mode === 'stems' ? { device } : {}), title, confirmed: true } });
  if (value !== null && (!value || typeof value.path !== 'string' || !value.path || !/^[a-f0-9]{64}$/.test(value.sha256) || !Number.isSafeInteger(value.size) || value.size <= 0)) throw new Error('Invalid audio enhancement export');
  return value;
}
