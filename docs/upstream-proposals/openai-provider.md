# Official OpenAI provider

Prepared locally for possible upstream adoption. **Not submitted.**

## Problem and resulting behavior

Official Responses transport, a bounded H3 loopback bridge and discovered recommended model selectors. Existing compatible-server behavior remains separate.

## Scope and dependencies

Review branch: `codex/openai-provider`. Base: `main`. [Comparison](https://github.com/CerebralDev/DEAS/compare/main...codex%2Fopenai-provider).

openai_api.rs; llm.rs; forge/chat.rs; shared ModelSelect and LlmPanel.

openaiapi is an additive provider identifier. Existing openai configs retain their meaning. H3 receives a disposable bridge token, never the cloud key. No installed dependency is patched.

## Validation

OpenAI Rust fixtures; LlmPanel, ForgeModels and llmRecommendations UI tests. See [the fork verification record](../FORK-VERIFICATION.md) for executed checks, platform boundaries and baseline failures. Fixtures do not establish live provider access or native runtime acceptance.

## Limitations

Responses are buffered. Model rankings are curated, not benchmarks. Remote Comfy cannot use the local H3 bridge. Paid/live and Linux runtime checks are separate.

This proposal excludes fork branding, private deployment records and automatic releases. It preserves upstream licensing and execution boundaries. No migration, model download, test render or paid inference is requested by this packaging change.
