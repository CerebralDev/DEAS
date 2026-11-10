# Virtual model library

Prepared locally for possible upstream adoption. **Not submitted.**

## Problem and resulting behavior

Organize resources without moving model files: folders, tags, pins, types, order, search and saved views in both library surfaces.

## Scope and dependencies

Review branch: `codex/model-library`. Base: `main`. [Comparison](https://github.com/CerebralDev/DEAS/compare/main...codex%2Fmodel-library).

model_library.rs; modelLibrary.ts; OrganizedModels; InstalledModels; Workbench.

Version 1 deas-model-library.json, revision checks and atomic writes. Membership identities remain source identities; names never become execution IDs. Shared diagnostics/selection helper is bundled in this branch.

## Validation

Native roundtrip/conflict/cycle/corrupt-store tests; library UI; installed view; layout/paired gates. See [the fork verification record](../FORK-VERIFICATION.md) for executed checks, platform boundaries and baseline failures. Fixtures do not establish live provider access or native runtime acceptance.

## Limitations

No file renaming or execution compatibility overrides. Enrichment is separately adoptable. Cached-thumbnail fields are optional.

This proposal excludes fork branding, private deployment records and automatic releases. It preserves upstream licensing and execution boundaries. No migration, model download, test render or paid inference is requested by this packaging change.
