# Selected metadata enrichment

Prepared locally for possible upstream adoption. **Not submitted.**

## Problem and resulting behavior

Refresh selected resources by exact identity, cache thumbnails and expose provenance, cancellation and outcomes.

## Scope and dependencies

Review branch: `codex/model-enrichment`. Base: `codex/model-library`. [Comparison](https://github.com/CerebralDev/DEAS/compare/codex%2Fmodel-library...codex%2Fmodel-enrichment).

downloads_model_assets.rs; downloads_models_scan.rs; library refresh/details UI.

Depends on model-library. Legacy cache structures remain readable. native_model_scan_selected, native_model_scan_cancel and native_model_assets retain their identifiers.

## Validation

Native metadata/scan fixtures; library/download UI; layout gate. See [the fork verification record](../FORK-VERIFICATION.md) for executed checks, platform boundaries and baseline failures. Fixtures do not establish live provider access or native runtime acceptance.

## Limitations

Provider credentials and live network availability are separate from fixtures. Author guidance is distinguished from example settings; neither is applied automatically.

This proposal excludes fork branding, private deployment records and automatic releases. It preserves upstream licensing and execution boundaries. No migration, model download, test render or paid inference is requested by this packaging change.
