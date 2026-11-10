import { invoke } from '@tauri-apps/api/core';

export type LlmProvider = 'local' | 'ollama' | 'openai' | 'openaiapi';
export interface LlmConfig { provider: LlmProvider; baseUrl: string; apiKey?: string }
export interface LlmModelRef { id: string; provider: LlmProvider }
export interface LlmModel extends LlmModelRef { name: string }
export interface LlmModels { models: LlmModel[]; warnings: string[] }
export interface LlmRequest { model: LlmModelRef; system: string; prompt: string; maxTokens: number; temperature: number }
export interface LlmResult { text: string; backend: LlmProvider }
export const defaultLlmConfig: LlmConfig = { provider: 'local', baseUrl: 'http://127.0.0.1:11434' };

export const llmSetupEvent = 'deas:llm-setup-required';
export function requestLlmSetup(): void {
  window.dispatchEvent(new Event(llmSetupEvent));
}

/** Configuration only: never probe a configured remote service or start/load a model. */
export async function hasLlmConfiguration(root?: string): Promise<boolean> {
  if (!root) return false;
  const config = await invoke<LlmConfig>('native_llm_read', { root });
  if (config.provider !== 'local') return !!config.baseUrl.trim();
  const local = await discoverLlmModels(root);
  // An unreadable model folder is a diagnostic failure, not proof of missing configuration.
  if (!local.models.length && local.warnings.length) throw new Error(local.warnings.join('\n'));
  return local.models.length > 0;
}

/** Read-only discovery of local models and the saved optional external endpoint. */
export function discoverLlmModels(root: string): Promise<LlmModels> {
  return invoke<LlmModels>('native_llm_models', { root });
}

/** Explicit completion with a per-request model, never a global model preference. */
export function generateLlm(root: string, request: LlmRequest): Promise<LlmResult> {
  return invoke<LlmResult>('native_llm_generate', { root, request });
}
