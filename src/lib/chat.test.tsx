import { expect, it } from 'vitest';
import { chatRenderSize } from './chat';

const model = { width: 1024, height: 1024 };
const current = { width: 832, height: 1216 };

it('keeps Generate\'s own size when chat asked for none', () => {
  expect(chatRenderSize({}, model, current, 16)).toBeUndefined();
  expect(chatRenderSize({ aspect: ' ', resolution: null }, model, current, 16)).toBeUndefined();
});

it('reads sizes the way an LLM writes them, snapped to the model grid', () => {
  // A pixel size gives both sides, in either field.
  expect(chatRenderSize({ resolution: '1920x1080' }, model, current, 16)).toEqual({ width: 1920, height: 1088 });
  expect(chatRenderSize({ aspect: '1024×1536' }, model, current, 16)).toEqual({ width: 1024, height: 1536 });
  // A ratio keeps the model's area.
  expect(chatRenderSize({ aspect: '1:1' }, model, current, 16)).toEqual({ width: 1024, height: 1024 });
  expect(chatRenderSize({ aspect: 'square' }, model, current, 16)).toEqual({ width: 1024, height: 1024 });
  const wide = chatRenderSize({ aspect: '16:9' }, model, current, 16)!;
  expect(wide.width / wide.height).toBeCloseTo(16 / 9, 1);
  expect(wide.width * wide.height).toBeGreaterThan(0.9 * 1024 * 1024);
  // A short side with a ratio, PromptForge's "1:1 at around 1080p".
  expect(chatRenderSize({ aspect: '1:1', resolution: '1080p' }, model, current, 16)).toEqual({ width: 1088, height: 1088 });
  expect(chatRenderSize({ aspect: '9:16', resolution: '720' }, model, current, 16)).toEqual({ width: 720, height: 1280 });
  // A short side alone keeps Generate's current shape (portrait here).
  expect(chatRenderSize({ resolution: '1080p' }, model, current, 16)).toEqual({ width: 1088, height: 1584 });
});

it('refuses sizes it cannot read or Generate cannot draw', () => {
  expect(chatRenderSize({ aspect: 'cinematic' }, model, current, 16)).toBeNull();
  expect(chatRenderSize({ resolution: 'big' }, model, current, 16)).toBeNull();
  expect(chatRenderSize({ resolution: '20000x20000' }, model, current, 16)).toBeNull();
});

const locales = import.meta.glob<Record<string, string>>('../features/chat/locales/*.json', { eager: true, import: 'default' });
it.each(['de', 'fr', 'ja', 'tr', 'zh'])('has every chat render string in %s', locale => {
  const en = locales['../features/chat/locales/en.json'];
  const keys = Object.keys(en).filter(k => k.startsWith('chat.render.'));
  expect(keys.length).toBeGreaterThan(15);
  const own = locales[`../features/chat/locales/${locale}.json`];
  for (const key of keys) expect(own[key], `${locale} ${key}`).toBeTruthy();
});
