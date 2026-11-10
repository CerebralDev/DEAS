# Fork verification — 2026-11-10

This records source packaging and test evidence for the CerebralDev fork. The installed custom v0.2.22 executable was not replaced, restarted or deployed. No model weights, user media, rendering or paid AI calls were used for this verification. Test builds are not published releases.

Local toolchain: Windows x86-64, Rust 1.99.1, Node 24.20.0, npm 11.19.0 and the already-installed Python 3.12.11 for subprocess fixtures. Hosted verification retains upstream Node 22 and stable Rust. These environment differences are recorded rather than hidden by changing CI.

## Local branch checks

Each feature was checked on its declared branch, with only its documented prerequisites. `npm run build` includes TypeScript checking. Every branch passed `cargo fmt --check`, a Windows native check and a linked Windows release build. The local Clippy column is **scoped**, as explained below; it is not a strict-CI pass.

| Branch suffix | Built source | Typecheck + production frontend | Format | Scoped Clippy | Windows release build |
| --- | --- | --- | --- | --- | --- |
| `openai-provider` | `5eba3dd01dcd` | Pass | Pass | Pass | 311.2 s |
| `forge-apply-return` | `6546755d71ae` | Pass | Pass | Pass | 304.6 s |
| `model-library` | `921ae911608c` | Pass | Pass | Pass | 258.2 s |
| `model-enrichment` | `e7ba65df0ba4` | Pass | Pass | Pass | 295.6 s |
| `workspace-docks` | `2f0e573340ee` | Pass | Pass | Pass | 266.5 s |
| `contextual-chat` | `cbd95aefe291` | Pass | Pass | Pass | 269.5 s |
| `workspace-integration` | `0a1f68e9aba1` | Pass | Pass | Pass | 276.1 s |

The integration source receipt predates only subsequent fork documentation/verification updates. Executable hashes and complete local command logs are retained privately; these builds were never installed. No dependency or application version was changed.

Focused module suites passed:

- OpenAI: 33 UI tests and 12 native fixture tests.
- Forge Apply & return: 92 UI tests, shared layout and paired-output gates.
- Model library: 40 UI tests, 3 native persistence/validation tests, layout and paired-output gates. Two Workbench integration cases also passed after their fixture was supplied with a valid empty organization store.
- Enrichment: 40 UI tests, 43 native tests with 1 pre-existing ignored case, and the layout gate.
- Workspace docks: 16 UI tests, layout and paired-output gates.
- Contextual Chat: 13 UI tests, 2 native context/proposal tests, and the layout gate.

Focused commands (run from the relevant branch root after installing locked dependencies):

```sh
# OpenAI
npx vitest run src/features/llm src/features/forge/ForgeModels.test.tsx src/lib/llmRecommendations.test.tsx
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib openai -- --test-threads=1
# Forge
npx vitest run src/features/forge/Forge.test.tsx
# Library, then enrichment on its dependent branch
npx vitest run src/features/workspace/ModelLibrary.test.tsx src/features/downloads/DownloadPluginPanel.test.tsx
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib model_library -- --test-threads=1
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib downloads::models -- --test-threads=1
# Docks
npx vitest run src/features/workspace/Docking.test.tsx src/features/monitoring/Monitoring.test.tsx src/features/chat/Chat.test.tsx
# Contextual Chat
npx vitest run src/features/workspace/ContextualChat.test.tsx src/features/chat/Chat.test.tsx
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib workspace_context -- --test-threads=1
# Layout changes, on their relevant branches
npm run test:layout
uv run --no-project --with playwright==1.64.1 python tools/verify_workspace_paired.py
```

## Combined and baseline suites

- Combined `npm test`: core **150 passed, 1 skipped**; UI **1,681 passed** across 87 files.
- Combined native library suite: **803 passed, 62 ignored, 0 failed**, with one test thread and the existing managed Python on process-local PATH. Ignored tests were not claimed as exercised. A first attempt without Python on PATH failed two existing preview/auto-refine subprocess tests; no application dependency was changed to resolve that environment issue.
- The unchanged upstream baseline UI suite passed **1,668 tests** locally. Its two MediaLibrary relocation cases also passed independently; historical notes about those cases are not current failures.
- The initial combined UI run found two library/Workbench mock-contract errors, fixed in the library branch. One unchanged FoldersPanel timing case passed a focused retry; the complete combined suite then passed.
- `npm run test:layout` passed, including the deliberately broken negative control, navigation discovery, six locales/seven sizes, retained views, both docks, long content and 600-resource catalogs. Paired-output pointer/overflow checks passed separately, including narrow layouts and simulated explicit Run behavior. All IPC was fixture-backed; the simulated Run did not render.

Chromium geometry checks and Windows compilation are distinct from native Windows WebView2 or Linux GTK/WebKit runtime acceptance. No new native-runtime session, installer acceptance, real network enrichment or paid-provider test was performed in this fork-packaging pass.

## GitHub CI — known blockers, checks retained

Verification Actions are enabled. The original `verify.yml` checks remain unchanged, including strict `cargo clippy ... -- -D warnings`, full suites, the blocking layout gate and Windows installer build. Automatic releases and scheduled Forge-data publishing are disabled in repository settings, with additional fork guards only on the integration branch. No PR or binary release was created.

**The full hosted pipeline is not green.** Current outcomes are available in [GitHub Actions](https://github.com/CerebralDev/DEAS/actions/workflows/verify.yml).

- Stable Rust 1.99 reports `clippy::chunks_exact_to_as_chunks` in unchanged upstream `audio_runtime.rs`, `jobs_auto_refine.rs` and `jobs_preview.rs`. Their contents match the verified upstream baseline. Native CI stops at strict lint, so its later native tests/release/installer stages are not passing evidence.
- Local scoped Clippy used `-D warnings -A clippy::chunks_exact_to_as_chunks` to detect module-specific problems while excluding only that known upstream lint. This allowance is not committed to CI or source. It caught a new library test initialization lint, which was fixed rather than suppressed.
- Some hosted UI runs time out in unchanged Workbench H3 reference/continuity fixtures at their existing 5/10-second limits. The same cases pass locally; this is a remaining hosted timing issue, not proof of a generation defect or a fully green suite. No timeout, assertion or check was relaxed.
- The [Forge frontend job](https://github.com/CerebralDev/DEAS/actions/runs/42825976668), [model-library frontend job](https://github.com/CerebralDev/DEAS/actions/runs/42830293986) and [contextual Chat frontend job](https://github.com/CerebralDev/DEAS/actions/runs/42828002428) passed typecheck, the full frontend tests, production build and the mandatory layout gate. Their overall runs still fail in other jobs as described above.

Reproduce strict CI before proposing adoption. Fix upstream lint/timing separately and review it explicitly rather than treating the scoped local result as a clean hosted pipeline. Linux native release and Windows installer CI acceptance remain outstanding because earlier stages fail.

## Publication and compatibility checks

- GitHub recognizes `CerebralDev/DEAS` as a public fork of `darksidewalker/DEAS`.
- `main` retains the complete upstream history and exact verified upstream tree. Feature ancestry and parent-relative comparisons are checked; integration contains all six module tips.
- Every newly published commit, including intermediate commits and merge deltas, is checked for secrets, private endpoints/paths, operational backups and user media. Machine-specific recovery records remain private.
- Upstream GPL licensing, credits, package versions, dependency locks and verification CI are preserved. Stable native command names and existing persisted schemas remain compatible; this packaging introduces no data migration.
- The canonical checkout is moved only after source-hash drift checks, with the original tracked/untracked changes retained in a private snapshot and local preservation stash. Only explicit reviewed branch refs are pushed; preservation refs are never published.

For adoption ranges, interfaces and prepared descriptions see [UPSTREAM-MODULES.md](UPSTREAM-MODULES.md). See [FORK.md](FORK.md) for synchronization, installed-build provenance and rollback boundaries.
