import { defaultOptions, type OptionsPref } from './OptionsPanel';
import type { TvePref } from './TvePostControls';
import { defaultWildcard, type WildcardRules } from '../../lib/wildcard';

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

/** Options fields that differ from their defaults (absent fields from older saves count as default). */
export function changedOptions(pref: OptionsPref): string[] {
  return (Object.keys(defaultOptions) as (keyof OptionsPref)[]).filter(k => pref[k] !== undefined && !same(pref[k], defaultOptions[k]));
}

/** Wildcards: the switch plus authored rules; seed, rerolls and picked values change on their own and are not "settings". */
export function activeWildcards(enabled: boolean, rules: WildcardRules): string[] {
  const authored = (['style', 'token_budget', 'auto_reroll', 'variables_json'] as const).filter(k => !same(rules[k], defaultWildcard[k]));
  return enabled ? ['enabled', ...authored] : [];
}

/** Post-processing stages that will run for the current pipeline. */
export function activePost(p: { video: boolean; audio: boolean; latent: boolean; tve: TvePref; refine: boolean; sam: boolean; audioMode: string }): string[] {
  const audio = p.audio && p.audioMode !== 'off' ? ['audio'] : [];
  if (p.video) return [
    ...(p.latent ? ['latent'] : []),
    ...(p.tve.upscaleModel ? ['upscale'] : []),
    ...(['denoise', 'deblur', 'rtxUpscale', 'frameGen'] as const).filter(k => p.tve[k]),
    ...audio,
  ];
  return [...(p.refine ? ['refine'] : []), ...(p.sam ? ['sam'] : []), ...audio];
}
