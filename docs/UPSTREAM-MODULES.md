# Upstream adoption modules

Fork: [https://github.com/CerebralDev/DEAS](https://github.com/CerebralDev/DEAS). Baseline: [`0c2b1083d47e`](https://github.com/darksidewalker/DEAS/commit/0c2b1083d47ecc7c210b09f75ebedf90fbba1414). No upstream submissions have been made.

Each module can be reviewed independently against the base below. Compare links show all prerequisite plumbing included in independent branches. Dependent comparisons show only the child delta. Fork branding, CI release guards and maintenance documentation stay in separate integration commits.

| Module | Branch | Declared base | Review |
| --- | --- | --- | --- |
| Official OpenAI provider | [codex/openai-provider](https://github.com/CerebralDev/DEAS/tree/codex/openai-provider) | `main` | [Compare](https://github.com/CerebralDev/DEAS/compare/main...codex%2Fopenai-provider) |
| Forge Apply & return | [codex/forge-apply-return](https://github.com/CerebralDev/DEAS/tree/codex/forge-apply-return) | `main` | [Compare](https://github.com/CerebralDev/DEAS/compare/main...codex%2Fforge-apply-return) |
| Virtual model library | [codex/model-library](https://github.com/CerebralDev/DEAS/tree/codex/model-library) | `main` | [Compare](https://github.com/CerebralDev/DEAS/compare/main...codex%2Fmodel-library) |
| Selected metadata enrichment | [codex/model-enrichment](https://github.com/CerebralDev/DEAS/tree/codex/model-enrichment) | `codex/model-library` | [Compare](https://github.com/CerebralDev/DEAS/compare/codex%2Fmodel-library...codex%2Fmodel-enrichment) |
| Retained workspace docks | [codex/workspace-docks](https://github.com/CerebralDev/DEAS/tree/codex/workspace-docks) | `main` | [Compare](https://github.com/CerebralDev/DEAS/compare/main...codex%2Fworkspace-docks) |
| Contextual Chat and reviewed suggestions | [codex/contextual-chat](https://github.com/CerebralDev/DEAS/tree/codex/contextual-chat) | `codex/workspace-docks` | [Compare](https://github.com/CerebralDev/DEAS/compare/codex%2Fworkspace-docks...codex%2Fcontextual-chat) |

## Official OpenAI provider

Official Responses transport, a bounded H3 loopback bridge and discovered recommended model selectors. Existing compatible-server behavior remains separate.

- Guide: [OPENAI-API.md](OPENAI-API.md)
- Range: `0c2b1083d47ecc7c210b09f75ebedf90fbba1414..5eba3dd01dcdc79f47a93f1e73bdda6acc0e7fd5`
- Interfaces: openai_api.rs; llm.rs; forge/chat.rs; shared ModelSelect and LlmPanel.
- Compatibility: openaiapi is an additive provider identifier. Existing openai configs retain their meaning. H3 receives a disposable bridge token, never the cloud key. No installed dependency is patched.
- Verification targets: OpenAI Rust fixtures; LlmPanel, ForgeModels and llmRecommendations UI tests. Actual results: [verification record](FORK-VERIFICATION.md).
- Limits: Responses are buffered. Model rankings are curated, not benchmarks. Remote Comfy cannot use the local H3 bridge. Paid/live and Linux runtime checks are separate.
- Prepared description: [openai-provider](upstream-proposals/openai-provider.md)

```text
5849f78 feat(llm): add official OpenAI Responses transport and recommended selectors
fd5040c test(workspace): align read-only fixtures and verify modular layouts
5eba3dd Merge branch 'codex/verification-fixtures' into codex/openai-provider
```

Adopt by merging `codex/openai-provider` into a review branch after its declared prerequisite, or cherry-pick the concrete non-merge commits above in order. Keep native registrations, permissions, tests and all six locale catalogs together. Resolve shared App/Workbench wiring by behavior. Run the module checks and combined layout gate before deploying.

## Forge Apply & return

Apply a reviewed H3 prompt and optionally return to Studio after successful validation, without starting generation.

- Guide: [FORGE-APPLY-RETURN.md](FORGE-APPLY-RETURN.md)
- Range: `0c2b1083d47ecc7c210b09f75ebedf90fbba1414..6546755d71ae3875f203084a404d25214abe497b`
- Interfaces: ForgeButton; Workbench; App callback.
- Compatibility: Optional onReturn / onReturnStudio callbacks; onApply returning false rejects application. No persisted schema change.
- Verification targets: Forge.test.tsx; layout and paired-output browser gates. Actual results: [verification record](FORK-VERIFICATION.md).
- Limits: Standalone Forge retains its existing send actions. No library, provider or Chat prerequisite.
- Prepared description: [forge-apply-return](upstream-proposals/forge-apply-return.md)

```text
64e112f feat(forge): validate Apply and add optional return-to-Studio action
fd5040c test(workspace): align read-only fixtures and verify modular layouts
6546755 Merge branch 'codex/verification-fixtures' into codex/forge-apply-return
```

Adopt by merging `codex/forge-apply-return` into a review branch after its declared prerequisite, or cherry-pick the concrete non-merge commits above in order. Keep native registrations, permissions, tests and all six locale catalogs together. Resolve shared App/Workbench wiring by behavior. Run the module checks and combined layout gate before deploying.

## Virtual model library

Organize resources without moving model files: folders, tags, pins, types, order, search and saved views in both library surfaces.

- Guide: [MODEL-LIBRARY.md](MODEL-LIBRARY.md)
- Range: `0c2b1083d47ecc7c210b09f75ebedf90fbba1414..921ae911608c1b177c506fa5b1dec2455dbb9b21`
- Interfaces: model_library.rs; modelLibrary.ts; OrganizedModels; InstalledModels; Workbench.
- Compatibility: Version 1 deas-model-library.json, revision checks and atomic writes. Membership identities remain source identities; names never become execution IDs. Shared diagnostics/selection helper is bundled in this branch.
- Verification targets: Native roundtrip/conflict/cycle/corrupt-store tests; library UI; installed view; layout/paired gates. Actual results: [verification record](FORK-VERIFICATION.md).
- Limits: No file renaming or execution compatibility overrides. Enrichment is separately adoptable. Cached-thumbnail fields are optional.
- Prepared description: [model-library](upstream-proposals/model-library.md)

```text
f54d5f5 refactor(workspace): separate shared diagnostics and selection feed from Chat
0cdefdd feat(library): add virtual organization and shared searchable installed views
fd5040c test(workspace): align read-only fixtures and verify modular layouts
b772502 Merge branch 'codex/verification-fixtures' into codex/model-library
9eae73b style: format extracted native modules
7c50390 test(library): keep enrichment-only copy assertions in its dependent module
1f66825 test(library): supply valid organization state to Workbench fixtures
921ae91 test(library): initialize cycle fixture without field reassignment
```

Adopt by merging `codex/model-library` into a review branch after its declared prerequisite, or cherry-pick the concrete non-merge commits above in order. Keep native registrations, permissions, tests and all six locale catalogs together. Resolve shared App/Workbench wiring by behavior. Run the module checks and combined layout gate before deploying.

## Selected metadata enrichment

Refresh selected resources by exact identity, cache thumbnails and expose provenance, cancellation and outcomes.

- Guide: [MODEL-ENRICHMENT.md](MODEL-ENRICHMENT.md)
- Range: `921ae911608c1b177c506fa5b1dec2455dbb9b21..e7ba65df0ba47f04707d3113736a04f9285e8978`
- Interfaces: downloads_model_assets.rs; downloads_models_scan.rs; library refresh/details UI.
- Compatibility: Depends on model-library. Legacy cache structures remain readable. native_model_scan_selected, native_model_scan_cancel and native_model_assets retain their identifiers.
- Verification targets: Native metadata/scan fixtures; library/download UI; layout gate. Actual results: [verification record](FORK-VERIFICATION.md).
- Limits: Provider credentials and live network availability are separate from fixtures. Author guidance is distinguished from example settings; neither is applied automatically.
- Prepared description: [model-enrichment](upstream-proposals/model-enrichment.md)

```text
18da0ed feat(library): refresh exact-match metadata and bounded thumbnails on selection
2310634 Merge branch 'codex/model-library' into codex/model-enrichment
76f593c style: format extracted native modules
4ea42e3 refactor(enrichment): route asset errors through shared diagnostics
48cd2b9 Merge branch 'codex/model-library' into codex/model-enrichment
e7ba65d Merge library test lint correction into enrichment
```

Adopt by merging `codex/model-enrichment` into a review branch after its declared prerequisite, or cherry-pick the concrete non-merge commits above in order. Keep native registrations, permissions, tests and all six locale catalogs together. Resolve shared App/Workbench wiring by behavior. Run the module checks and combined layout gate before deploying.

## Retained workspace docks

Move a single Chat session and monitoring feed around the shell, preserving drafts and accepted work.

- Guide: [WORKSPACE-DOCKS.md](WORKSPACE-DOCKS.md)
- Range: `0c2b1083d47ecc7c210b09f75ebedf90fbba1414..2f0e573340ee3d5bd1d1cbda8f2e477df40839d4`
- Interfaces: App; AiOSetup; RetainedPane; DockResize; Monitoring; compact Chat history.
- Compatibility: Existing workspace.* preferences retain meaning. Common diagnostics and selection feed are bundled; no contextual Chat import. No generation schema change.
- Verification targets: Docking, Chat and Monitoring UI; dock permutations; six locales/seven sizes; paired gate. Actual results: [verification record](FORK-VERIFICATION.md).
- Limits: Narrow side docks use overlays. Browser fixtures do not replace native Windows or GTK/WebKit runtime acceptance.
- Prepared description: [workspace-docks](upstream-proposals/workspace-docks.md)

```text
f54d5f5 refactor(workspace): separate shared diagnostics and selection feed from Chat
5a30616 feat(workspace): retain docked Chat and shared monitoring across screens
fd5040c test(workspace): align read-only fixtures and verify modular layouts
963fb8d Merge branch 'codex/verification-fixtures' into codex/workspace-docks
2f0e573 style: format extracted native modules
```

Adopt by merging `codex/workspace-docks` into a review branch after its declared prerequisite, or cherry-pick the concrete non-merge commits above in order. Keep native registrations, permissions, tests and all six locale catalogs together. Resolve shared App/Workbench wiring by behavior. Run the module checks and combined layout gate before deploying.

## Contextual Chat and reviewed suggestions

Freeze relevant text context when sending; preview it and review prompt/settings suggestions before explicit Apply.

- Guide: [CONTEXTUAL-CHAT.md](CONTEXTUAL-CHAT.md)
- Range: `2f0e573340ee3d5bd1d1cbda8f2e477df40839d4..cbd95aefe291ce2cfd1941074fae73c7e794fde1`
- Interfaces: workspace_context.rs; assistant.rs; context.ts; Suggestion; Chat.
- Compatibility: Depends on workspace-docks. Snapshot version 1; optional Message.context preserves old conversations. Native validator permits only supported authoring values. Existing native command identifiers remain intact.
- Verification targets: Native context/proposal tests; ContextualChat and Chat UI; layout gate. Actual results: [verification record](FORK-VERIFICATION.md).
- Limits: Images remain explicit attachments. Context is data, never rendering authority. A separate per-message execution permission remains required. Forge live-context publishing is integration wiring.
- Prepared description: [contextual-chat](upstream-proposals/contextual-chat.md)

```text
948f552 feat(chat): capture bounded workspace context and review guarded suggestions
7d6c252 Merge verified docking base into contextual Chat
cbd95ae style: format extracted native modules
```

Adopt by merging `codex/contextual-chat` into a review branch after its declared prerequisite, or cherry-pick the concrete non-merge commits above in order. Keep native registrations, permissions, tests and all six locale catalogs together. Resolve shared App/Workbench wiring by behavior. Run the module checks and combined layout gate before deploying.

## Combined integration wiring

The integration merge supplies Forge’s optional live brief/resource selection to the shared presentation feed and joins independent App/Workbench callbacks. It does not change prompts, seeds, generation requests, native command identifiers or persisted schemas. Module-local CSS and locale ownership remove incidental dependencies. No feature requires fork-maintenance documentation or release guards.
