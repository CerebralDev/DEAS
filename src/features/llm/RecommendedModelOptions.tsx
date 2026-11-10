import { useI18n } from '../../i18n/I18nProvider';
import { recommendationKey, recommendedFirst } from '../../lib/llmRecommendations';

/** One list for Forge, people detection, analysis, Chat and the connection test. */
export function RecommendedModelOptions({ models }: { models: { id: string; label: string; value?: string }[] }) {
  const { t } = useI18n();
  const ordered = recommendedFirst(models);
  const recommended = ordered.filter(m => recommendationKey(m.id));
  const other = ordered.filter(m => !recommendationKey(m.id));
  const options = (items: typeof models) => items.map(m => {
    const key = recommendationKey(m.id);
    return <option key={m.id} value={m.value ?? m.id}>{m.label}{key ? ` — ${t(key)}` : ''}</option>;
  });
  if (!recommended.length) return <>{options(other)}</>;
  return <><optgroup label={t('llm.recommend.group')}>{options(recommended)}</optgroup>
    {other.length > 0 && <optgroup label={t('llm.recommend.other')}>{options(other)}</optgroup>}</>;
}
