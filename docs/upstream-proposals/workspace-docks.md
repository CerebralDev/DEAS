# Retained workspace docks

Prepared locally for possible upstream adoption. **Not submitted.**

## Problem and resulting behavior

Move a single Chat session and monitoring feed around the shell, preserving drafts and accepted work.

## Scope and dependencies

Review branch: `codex/workspace-docks`. Base: `main`. [Comparison](https://github.com/CerebralDev/DEAS/compare/main...codex%2Fworkspace-docks).

App; AiOSetup; RetainedPane; DockResize; Monitoring; compact Chat history.

Existing workspace.* preferences retain meaning. Common diagnostics and selection feed are bundled; no contextual Chat import. No generation schema change.

## Validation

Docking, Chat and Monitoring UI; dock permutations; six locales/seven sizes; paired gate. See [the fork verification record](../FORK-VERIFICATION.md) for executed checks, platform boundaries and baseline failures. Fixtures do not establish live provider access or native runtime acceptance.

## Limitations

Narrow side docks use overlays. Browser fixtures do not replace native Windows or GTK/WebKit runtime acceptance.

This proposal excludes fork branding, private deployment records and automatic releases. It preserves upstream licensing and execution boundaries. No migration, model download, test render or paid inference is requested by this packaging change.
