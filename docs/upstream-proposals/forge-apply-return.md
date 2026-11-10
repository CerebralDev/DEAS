# Forge Apply & return

Prepared locally for possible upstream adoption. **Not submitted.**

## Problem and resulting behavior

Apply a reviewed H3 prompt and optionally return to Studio after successful validation, without starting generation.

## Scope and dependencies

Review branch: `codex/forge-apply-return`. Base: `main`. [Comparison](https://github.com/CerebralDev/DEAS/compare/main...codex%2Fforge-apply-return).

ForgeButton; Workbench; App callback.

Optional onReturn / onReturnStudio callbacks; onApply returning false rejects application. No persisted schema change.

## Validation

Forge.test.tsx; layout and paired-output browser gates. See [the fork verification record](../FORK-VERIFICATION.md) for executed checks, platform boundaries and baseline failures. Fixtures do not establish live provider access or native runtime acceptance.

## Limitations

Standalone Forge retains its existing send actions. No library, provider or Chat prerequisite.

This proposal excludes fork branding, private deployment records and automatic releases. It preserves upstream licensing and execution boundaries. No migration, model download, test render or paid inference is requested by this packaging change.
