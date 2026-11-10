# Contextual Chat and reviewed suggestions

Prepared locally for possible upstream adoption. **Not submitted.**

## Problem and resulting behavior

Freeze relevant text context when sending; preview it and review prompt/settings suggestions before explicit Apply.

## Scope and dependencies

Review branch: `codex/contextual-chat`. Base: `codex/workspace-docks`. [Comparison](https://github.com/CerebralDev/DEAS/compare/codex%2Fworkspace-docks...codex%2Fcontextual-chat).

workspace_context.rs; assistant.rs; context.ts; Suggestion; Chat.

Depends on workspace-docks. Snapshot version 1; optional Message.context preserves old conversations. Native validator permits only supported authoring values. Existing native command identifiers remain intact.

## Validation

Native context/proposal tests; ContextualChat and Chat UI; layout gate. See [the fork verification record](../FORK-VERIFICATION.md) for executed checks, platform boundaries and baseline failures. Fixtures do not establish live provider access or native runtime acceptance.

## Limitations

Images remain explicit attachments. Context is data, never rendering authority. A separate per-message execution permission remains required. Forge live-context publishing is integration wiring.

This proposal excludes fork branding, private deployment records and automatic releases. It preserves upstream licensing and execution boundaries. No migration, model download, test render or paid inference is requested by this packaging change.
