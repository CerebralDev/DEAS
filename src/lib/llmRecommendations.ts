/** Curated 2026-11-09 from OpenAI's model catalog. Never adds undiscovered models. */
const recommendations = ['gpt-6.1-terra', 'gpt-6.1-sol', 'gpt-6-luna'] as const;
export function recommendationKey(id: string): string | undefined {
  const match = /^(?:openaiapi|openai):(.+)$/.exec(id);
  const index = match ? recommendations.indexOf(match[1] as typeof recommendations[number]) : -1;
  return index < 0 ? undefined : ['llm.recommend.terra', 'llm.recommend.sol', 'llm.recommend.luna'][index];
}
export function recommendedFirst<T extends { id: string }>(models: readonly T[]): T[] {
  const rank = (id: string) => {
    const match = /^(?:openaiapi|openai):(.+)$/.exec(id);
    const index = match ? recommendations.indexOf(match[1] as typeof recommendations[number]) : -1;
    return index < 0 ? recommendations.length : index;
  };
  return [...models].sort((a, b) => rank(a.id) - rank(b.id));
}
