import { expect, test } from 'vitest';
import { recommendationKey, recommendedFirst } from './llmRecommendations';

test('recommendations never fabricate missing models or relabel local lookalikes', () => {
  const models = ['ollama:gpt-6.1-terra', 'openaiapi:gpt-4.1', 'openaiapi:gpt-6.1-sol', 'openaiapi:gpt-6.1-terra-unknown'].map(id => ({ id }));
  expect(recommendedFirst(models).map(m => m.id)).toEqual([models[2].id, models[0].id, models[1].id, models[3].id]);
  expect(models[0].id).toBe('ollama:gpt-6.1-terra');
  expect(recommendationKey(models[0].id)).toBeUndefined();
  expect(recommendationKey(models[3].id)).toBeUndefined();
  expect(recommendedFirst([])).toEqual([]);
});
