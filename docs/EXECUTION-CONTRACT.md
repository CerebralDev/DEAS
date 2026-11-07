# DEAS execution-only backend contract

## Accepted project-level target

Apply [PRODUCT-DECISIONS.md](PRODUCT-DECISIONS.md) alongside this execution boundary. A durable project must retain managed input/result media, immutable revisions and exact executed job/dependency identities. Recovery reconciles jobs without retrying uncertain submissions; retries are explicit new jobs/stage attempts. Preserve partial results, serialize owned GPU-intensive work by default, and never stop external services. The separate Comfy editor remains outside the Studio project contract without automatic graph↔Studio synchronization; protect Studio jobs from global editor actions or disable those actions where isolation cannot be established. These are accepted target requirements, not proof that current session-held jobs or output caches already satisfy them.

## Product decision

DEAS owns the entire product interface: feature/package selection, model and VAE pickers, prompts, reference media, RefMods, continuity controls, timelines, settings, validation, queue presentation, progress, previews, playback and exported results. ComfyUI is an execution service, not the studio interface. Within Studio, do not embed its web editor, node widgets, custom-node HTML/JavaScript panels or node-side upload/preview UI. The separate **Comfy** tab is an explicit exception: embed the original owned-backend frontend in a cross-origin frame, with no DEAS native-command permissions. Keep only the shared tab band and footer visible; preserve editor state across tab switches. Comfy actions use the backend directly (including its shared queue/global interrupt), not the DEAS job-scoped adapter. A changed endpoint reloads the editor; save workflows before a backend restart.

ComfyUI may still execute built-in/custom computational nodes internally. A computational node is an implementation detail of a recipe, not a DEAS feature or its visual control. The legacy DEAS diagnostics graph describes feature ownership, not a mirrored ComfyUI execution graph. Studio Run Plan is a separate read-only authoring preview from the same image/video intent builders as Run; it does not compile a second execution graph or authorize jobs. Upload handles, backend defaults and native internal pass settings remain unresolved until preparation/execution. Required-node introspection diagnoses backend compatibility; it must not generate the product UI from node input/widget definitions.

## Boundaries

| Layer | Owns | Does not own |
| --- | --- | --- |
| React DEAS UX | Authoring, selected settings, media references, validation presentation, job controls, output viewer | Backend filesystem paths, arbitrary shell commands, node editor controls |
| Domain recipes | Versioned feature intent, typed inputs, semantic defaults, required models/capabilities, deterministic compilation | HTTP clients, widget/canvas state, backend-dependent UI labels |
| Native assets/jobs | Explicit file access, upload/export, validated paths, transport, scoped process ownership | Inferring consent or editing unrelated environments |
| Comfy SDK/API adapter | Capability negotiation, input handles, graph submission, job identity, progress/status, typed failures, output handles | Studio selections, implicit cloud endpoints or default credentials |
| ComfyUI/Python | Model computation, graph execution, backend-side temporary input/output storage | DEAS authoring state, user-facing feature controls or result presentation |

Internal node savers/loaders may be needed to compute and transfer files. Their widgets and previews are not exposed as DEAS UX. DEAS owns input selection and final output destinations; returned backend asset references are validated and transferred into app-owned project/output storage before exporting or playback.

## API and SDK decision

Prefer a first-party SDK/API contract behind a narrow adapter, not imports from the ComfyUI frontend. The official SDKs are external clients: Python `comfy-sdk` and TypeScript `@comfyorg/sdk`, currently documented as beta `0.1.x`. They cover assets, API-format graph submission, execution, outputs, errors and cancellation; model-library management and node introspection are outside this first SDK surface.[1]

Self-hosted API v2 currently requires the documented `comfy-api-proxy` alongside ComfyUI during beta. Do not assume installing core ComfyUI automatically provides v2. Do not silently add this process or a cloud dependency to provisioning. Evaluate/pin its versions, ownership, authentication, resources and stop behavior before offering it as an explicitly planned managed runtime component.[1][2]

The official SDK chooses its base URL through `COMFY_BASE_URL`; leaving it blank defaults to Comfy Cloud. DEAS must bind an explicitly validated local/selected endpoint before constructing a client, and fail closed when none is set. No automatic cloud fallback, account creation, credential request or paid-service use.[1]

Capability negotiation selects the v2 SDK/API adapter only after a real compatible endpoint probe. For directly installed core ComfyUI without v2, retain a separate documented classic HTTP/WebSocket adapter; its existing `/prompt`, `/history`, `/ws`, upload and view routes remain available.[2][3] The domain job contract and UI must not change when adapters change. Negotiation must distinguish an absent optional API from authentication errors, HTML redirects, malformed responses and incompatible versions rather than silently hiding failures.

For v2, polling is the authoritative status/output view; live SSE events enhance progress but cannot serve as the durable result store.[2] The classic adapter must correlate progress/history with its own prompt/client ID and recover results after reconnect. Never display fabricated progress or treat a successful HTTP submission as successful computation.

## Dynamic execution path (implemented for Studio image, video and music recipes)

1. UX creates a versioned feature intent from DEAS-owned inputs/settings.
2. Recipe validation checks required parameters, models, capability/node versions, media constraints and licenses. Missing prerequisites block Run without claiming readiness.
3. A deterministic recipe compiler creates an API-format execution graph dynamically. Node IDs, links and class types are internal to the adapter/compiler; no node widgets or editor canvas are mounted.
4. Native asset handling uploads only explicitly selected files and resolves returned asset handles into recipe inputs. Never forward arbitrary host paths or auto-download arbitrary URLs.
5. A user Run action submits the compiled graph through the negotiated SDK/API adapter. Persist client/job/prompt identity and the exact recipe/version/input choices for traceability.
6. Correlated status/progress/errors update the DEAS queue. Cancellation is job-scoped; classic global `/interrupt` or queue-clear operations must never affect unrelated/shared-backend jobs. If isolation cannot be proven, expose the limitation rather than silently interrupting everything.
7. Validate completion and collect only that job's outputs. Safely resolve returned names/handles, verify media type/size and download without traversal, foreign host redirects or server-side path assumptions.
8. DEAS displays/plays results, preserves generation metadata, and exports through its own output controls. ComfyUI node previews and savers are not the UX.

## Installer continuation and readiness

Provisioning, backend service lifecycle, API compatibility, recipe readiness and generation acceptance are distinct states. `generationReady:false` remains until a real recipe successfully executes with its required models and computational nodes.

The native lifecycle permits app-owned backend start/status/stop only after successful provisioning or checked reopening of a completed DEAS environment. The last selected environment is revalidated on app launch; its backend auto-starts by default unless disabled in Settings. Check native session status before one automatic attempt, never restart after explicit Stop and hold an OS-level environment lock to prevent duplicate instances. No adoption of an existing listener, arbitrary executable paths, or stop/restart of external services. Bind loopback, use the approved managed interpreter/backend selection, disable ComfyUI browser auto-launch, and require real bounded API responses before reporting the service running. On app exit stop only this app's owned child/process group.

A local process/HTTP fixture proves lifecycle and protocol checks, not a full newly installed ComfyUI/PyTorch stack. Full installation and model execution remain separate explicitly authorized acceptance runs.

## Diagnostic classic submission boundary

The bounded diagnostic adapter accepts only the versioned diagnostic checkpoint intent, never arbitrary frontend API graphs. Native preparation rechecks core prerequisites and stores the exact endpoint and compiled seven-node graph. An explicit Run authorizes one submission of that immutable plan. A lost or malformed submission response is an uncertain result, not permission to retry. Session-held job identity is the only authority for subsequent history/queue reads; correlate the prompt ID, client identity and exact graph before interpreting completion or exposing safe output metadata.

No global interrupt, clear-all queue operation or foreign job adoption is permitted. Running submissions remain independent of authoring edits. Output metadata is not a downloaded/verified media asset; PNG retrieval/preview/new-file export are implemented in the bounded slice below. Studio jobs (not this diagnostic route) have scoped pending deletion and interruption through the owned `/deas/jobs/interrupt` adapter (see [native IPC](../src-tauri/README.md)); real model-generation acceptance remains separate work. This diagnostic classic route is explicit, not automatic fallback from failed v2 authentication/negotiation.

## Diagnostic output transfer boundary

The PNG output slice resolves only an index from a native-held terminal job (succeeded, failed or cancelled) with a retained output at that exact index; queued, running and unknown states stay blocked. For optional image refinement, roles come from the immutable compiler graph: saver `7` is Processed and `B7` is Original, with final outputs kept first for compatibility. Successful paired completion requires both products; a later failure/cancellation does not discard products already reported in correlated history. For image `intent.refine`, the capability-checked `DEAS_ImagePostprocessGate` eagerly depends on `B7` output 0 and lazily requests the complete final-image closure, deferring independent post-only loaders until baseline saving finishes. Existing effective savers, output roles and no-refine graphs remain unchanged. Backend discovery checks the gate lazy ABI and the selected saver completion socket; incompatible backends block refinement rather than silently falling back. This does not sequence video sibling savers, recreate deleted cached files or promise outputs before baseline generation/save succeeds. The frontend cannot supply an endpoint, backend filename or export bytes. Native transport requests the exact original output via the stored endpoint, rejects redirects and oversized/non-PNG responses, fully decodes under memory/dimension bounds and preserves the original encoded bytes with a SHA-256 digest. One bounded session cache supplies both preview and export, so Save As never silently retrieves changed backend content.

Loading a preview and exporting are separate explicit actions. The studio uses local PNG Blob URLs, not remote image URLs, and revokes replaced/unmounted previews. Export uses a native chooser and no-clobber descriptor-anchored publication to a new file; cancelling never reports success. These media/transport fixtures do not establish actual model generation or native-webview media acceptance. Unsupported export platforms fail closed rather than fall back to unsafe writes.

## Implementation ledger

- [x] React/Tauri feature UX and read-only ComfyUI inspection exist (`src/lib/comfy.ts`, `src-tauri/src/lib.rs`).
- [x] Native immutable provisioning plans and separate install consent exist (`src-tauri/src/install.rs`).
- [x] Document execution-only ownership and SDK/API selection constraints.
- [x] Explicit owned backend lifecycle, localized frontend actions and process/API failure regressions (real local-process/HTTP fixtures plus mocked UI IPC; no full ComfyUI stack acceptance).
- [x] Read-only native API negotiation with explicit endpoint, real HTTP fixtures and localized desktop action; no cloud default.
- [x] Pure versioned diagnostic checkpoint text-to-image compiler; supplied capability/model allowlists, strict validation and deterministic immutable graph.
- [x] Native read-only diagnostic recipe-prerequisite discovery and DEAS-owned compile preview; HTTP/IPC fixtures verified, not architecture/model-load or generation acceptance.
- [x] Explicit diagnostic classic native prepare/submit/status plus read-only current-session job recovery; bounded HTTP fixtures and localized IPC/browser checks, not model-generation acceptance.
- [x] Indexed native PNG (and YuE2 FLAC) retrieval, bounded full decoding, original-byte cache, explicit local Blob preview and safe no-clobber export with Linux and Windows code paths (Windows runtime unverified); HTTP/filesystem/browser fixtures only.
- [ ] Complete capability-negotiated SDK/classic execution adapters.
- [x] Versioned recipes and dynamic API graph compilation from UX intent: image recipes (including MOSS and GGUF split loaders), the family-discriminated video recipe (LTX 2.3 / Wan 2.2 / MiniMax H3 through the DaSiWa Director) and YuE2 text-to-music/cover, fixture-verified and validated offline with ComfyUI's `validate_prompt`. Individual real runs are in the verification journal; broad real-model acceptance is open.
- [x] Explicit input uploads, correlated Workbench jobs with all-job polling, scoped pending deletion/running interruption/whole-queue Stop, result preview/playback and export.
- [ ] A durable DEAS-owned asset registry (managed media, revisions, provenance) — deferred by owner scope.
- [ ] A real Director recipe accepted on a newly provisioned compatible backend (an owner-run H3 I2VA job succeeded; acceptance on a newly provisioned backend remains open).

## Verification targets and likely files

- `src-tauri/src/backend.rs`, `install.rs`, `main.rs`, `permissions/backend.toml`, `capabilities/main.json`: owned lifecycle, successful-install authorization, canonical paths, loopback identity, real child exit/timeout/stop and app shutdown tests.
- `src/components/setup/AiOSetup.tsx`, setup locales/tests, `src/lib/setup.ts`: explicit backend controls only following actual successful provisioning; start/status/error handling, no generation claim or automatic foreign-service action.
- `src/lib/execution.ts` and `tests/execution.test.ts`: typed job/domain contract and the TypeScript compile preview, kept in parity with the native compiler; validate inputs and unknown capability/model failures before submission.
- Native API/job modules (`api.rs`, `jobs*.rs`): exact endpoint binding, bounded JSON/media access, correlated jobs and isolated cancellation with HTTP fixtures.
- UI/browser tests: Studio controls and outputs are DEAS-owned, no editor navigation from Studio, no submission without explicit Run; the separate Comfy tab may navigate its frame to the owned endpoint `/` and exposes original ComfyUI controls; preserve stable selection geometry and content-anchored confirmation.
- Gates: `npm run typecheck`, `npm test`, `npm run build`, Cargo test/clippy/fmt/release; targeted browser checks and real native rendering. Fixture passes must be labelled as fixture evidence.

## Sources

[1] https://docs.comfy.org/development/api-development/sdks
[2] https://docs.comfy.org/development/api-development/sdks-design
[3] https://docs.comfy.org/development/comfyui-server/comms_routes

Classic prompt/queue/history response shapes were checked against upstream ComfyUI commit `8a33128f2f8c5585c57486c07de481241e70a39c`: [server routes](https://github.com/Comfy-Org/ComfyUI/blob/8a33128f2f8c5585c57486c07de481241e70a39c/server.py) and [queue/history storage](https://github.com/Comfy-Org/ComfyUI/blob/8a33128f2f8c5585c57486c07de481241e70a39c/execution.py). Queue/history tuples retain the prompt ID, actual graph and `extra_data.client_id`; additional server timestamps are not part of identity. A missing history record is not completion. No compatibility claim is made for arbitrary custom proxy schemas.
