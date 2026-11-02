# DEAS plan

[Documentation index](README.md) · [Project rules](PROJECT-RULES.md) · [Product decisions](PRODUCT-DECISIONS.md) · [Verification journal](VERIFICATION.md) · [Changelog](CHANGELOG.md)

This is the **single current plan** for DEAS: what is in place, what is still open and what has to be proven before an item counts as accepted. It replaces the earlier `IMPLEMENTATION-STATUS.md`, `VIDEO-PLAN.md`, `YUE2-MUSIC-PLAN.md`, `PLUGIN-ROADMAP.md`, `NEXT-SESSION.md`, `DRAFT-VIDEO-PLAYBACK-FIX.md` and the old milestone checklist that used this file name; they remain in Git history.

- Delivered work is described in the [changelog](CHANGELOG.md) (per version) and the user guides; evidence lives in the [verification journal](VERIFICATION.md). Finished slices are not repeated here.
- [PROJECT-RULES.md](PROJECT-RULES.md), [COMFYUI-RULES.md](COMFYUI-RULES.md) and [EXECUTION-CONTRACT.md](EXECUTION-CONTRACT.md) govern how work is done; [PRODUCT-DECISIONS.md](PRODUCT-DECISIONS.md) governs scope.
- An open row here is **not** authorization to start it. New integrations and large slices need an explicit owner priority.

Current source: **0.2.0 BETA** (unreleased), previously ALPHA. The label does not close any acceptance gates below. Last published baseline: **0.1.66** (tag `v0.1.66`), reviewed 2026-10-29.

## 1. Evidence levels

An item is only as accepted as its strongest evidence. Keep the levels separate when updating this plan.

| Level | Meaning |
| --- | --- |
| Implemented, bounded | Code exists for the named slice; it does not mean the whole area is done |
| Fixture-verified | Unit/component/HTTP/Chromium fixtures or offline graph validation passed |
| Native Linux | Exercised in the real GTK/WebKitGTK (or libmpv) app on Linux |
| Real model | Actual model computation produced a checked artifact on named hardware |
| Windows runtime | Exercised in the real WebView2 app on Windows; a cross-build is not this |

**Cross-cutting open gates.** Most delivered slices are fixture-verified and many are native-Linux verified. Unless a slice says otherwise, these remain open product-wide: Windows/WebView2 runtime acceptance, real-model quality/performance per family, measured RAM/VRAM/idle resources, and native click-through on hosts where input automation is blocked (KDE Wayland ignores XTest).

## 2. Status by area

| Area | State (0.1.66) | Open |
| --- | --- | --- |
| Shell, navigation, layout | Implemented. Tabs: Studio, Library, Prompt Forge, Chat, TVE, Run Plan, Comfy, Backend, Logs. Shared viewport gate `npm run test:layout` | Windows/native visual acceptance |
| Provisioning (installer) | Partial. Immutable consented plans, Linux/Windows x86_64 CPU/NVIDIA baseline, pinned ComfyUI-GGUF, checked reopening, foreign-install takeover, explicit Upgrade/Reinstall | §4.8 |
| Owned backend lifecycle | Implemented, bounded. Approved ownership, default-on autostart, explicit Stop, Windows Job Object / Linux process group; graceful maintenance stop and previous-state resume | Native Windows graceful-stop/update/move acceptance, real GPU restart matrix, crash/power-loss recovery |
| Downloads and Library | Partial. Catalogs, providers, managed placement, integrity, cancellation, Scan Files | Resume, free-space UX, §4.7 backlog, broad authenticated-provider acceptance |
| Authentication | Partial. Native exact-origin credentials, metadata-only readback | Real OS-store and provider acceptance beyond fixtures |
| Localization | Implemented, bounded. Six languages, module-owned locales | Native-speaker review |
| API negotiation | Partial. Classic/v2 discovery, exact node/model/sampler choices | Complete v2/SDK execution adapter |
| Image generation and families | Partial. Image recipes, LoRA stacks, family constraints, MOSS, SDXL/Qwen/Krea/MOSS image references, paired refine products, GGUF split loaders, Edit meta… family override, wrong-folder gating | Per-family real-model matrix; §4.4, §4.5 |
| Video generation | Partial. LTX 2.3, Wan 2.2, MiniMax H3 (T2VA/I2VA/FL2VA/L2VA/REF2VA, image edit), LTX v2v/keyframes, learned H3/LTX latent upscale | §4.1 |
| Director and Continuity | Partial. Continuity for all H3 pipelines, sessions, checkpoint/imported/uploaded sources, overlap, use-references, Forge Next action, References modal, RefMod reading, packs, people boxes, cast/pose cards | RefMod creation, joined clips, automatic card reading; §4.1 |
| Music (YuE2) | Implemented 0.1.66, fixture-verified. Text to Music (ABC Off/Full/Melody), Cover Music, FLAC results/history (all-in-one checkpoints only) | §4.3; split diffusion-model loading §4.5 |
| Prompt Forge / Chat / shared LLM | Partial. Native writers, MOSS Layout Studio, streamed Chat with HTTP cancellation, model-exact routing, tag suggestions | Forge request streaming (Forge still sends `stream:false`), remote API-key auth where absent, real-LLM acceptance |
| Presets, wildcards, seeds | Partial. Separate Presets/Wildcards tabs, reproducible seeds | Custom wildcard library editing/import, versioned cross-feature smart presets, hardware-aware recommendations |
| Image editing | Partial. Layers/masks, paint, magic wand, outpaint frame, full-size drop, crop-inpaint-composite | Undo, transparent inserts, outpaint seam quality, §4.4 |
| Queue, jobs, history | Implemented, bounded. Repeated Run, split Run/Stop (scoped pending delete + `/deas/jobs/interrupt`), all-job polling, owned preview observers, last-10 recent history | Queue statistics expansion (§4.6); real GPU cancellation |
| Media and playback | Partial. Media dock, star/unstar raw↔starred, metadata, Linux video via embedded libmpv, WebKit `<audio>` with bundled GStreamer plugins | §4.1 playback acceptance |
| Output and export | Partial. PNG/FLAC bounded retrieval, cache, no-clobber export; paired image products | Strict baseline-before-refine ordering; paired video products (§4.2) |
| Projects | Partial. Image-draft projects, single-writer lock, managed PNG import, draft ZIP | Full persistence is deferred by owner scope (§4.6) |
| TVE video post-processing | Partial. TVE tab (Rust FFprobe inspection, chooser or Media-panel source, native source retention), TVE stages (upscale model, RTX denoise/deblur/VSR, RTX/RIFE frame generation) in every video pipeline as paired Original/Processed products, and the TVE tab's backend enhance recipe (0.1.67). The standalone Rust ONNX engine was retired in 0.1.67 | §4.2 |
| Video comparison | Missing dedicated integration (image compare exists) | §4.9 |
| Video assembly | Missing | §4.9 |
| Training / datasets | Missing | §4.9 |
| Telemetry / optimization | Partial primitives (`hardware.rs`) | Resource profiles, shared GPU scheduling, opt-in benchmarks |
| Plugin platform | Metadata/data plugins only | §4.10 |
| Updates and migrations | Partial. Confirmed app-binary update (Linux self-replace; Windows manual), in-place backend update | Staged update/rollback, project migrations, packed release assets (§4.8) |
| Distribution | Partial. Local unsigned Linux/Windows builds, release manifest/SHA256SUMS; hosted workflows disabled | Signed/portable distribution, fresh-machine acceptance, glibc portability |

## 3. Owner scope

Current scope (see [product decisions](PRODUCT-DECISIONS.md)): the lightweight global recent-job history is delivered; managed generated-PNG adoption, immutable i2i revisions and full persistent-project recovery are **deferred**. Integrations delivered since that decision (MOSS, image references, TVE, learned latent upscale, GGUF, YuE2) were each explicitly prioritized by the owner. There is no automatic “next slice”: pick from §4 only on explicit owner priority.

## 4. Open work

### 4.1 Video, H3 Director and playback

- **Prompt Forge must see the actual selected input/reference media, regardless of source folder. Status: planned, not implemented.** Owner reports that Forge shows only People & order and the input-folder restriction, without a usable way to let the writer inspect the chosen media. Cover all supported inputs and selected references, including generated output/library files and external selections, not only files already under the backend input folder. Distinguish people-box detection from writer vision; neither a filename nor manual character labels mean the writer has seen the picture.
  - Source facts: `src/features/studio/PeopleDetector.tsx` requires endpoint/root/`reference.path` and shows `forge.detectInput` otherwise; inspect how reference paths are resolved before treating the folder hint as a proven transport limitation. `src/features/forge/ForgeButton.tsx` already has `data-forge-see`, but renders it only for `h3.easy` and sends `see_pictures` only when that condition is true. The Director reference `../ComfyUI-DaSiWa-Nodes/js/minimax_h3_forge.js` has **Let the model see the pictures** for labelled-image drafts; its normal-reference hint says pictures are automatically sent to vision-capable models. This is existing conditional functionality to repair/expose, not evidence that no vision path exists.
  - [ ] Trace both standalone and run-bound Forge contexts from current inputs/references through native validation and node-pack requests. Make the applicable vision choice visible with an accurate enabled/unavailable reason; match the Director's labelled-picture versus normal-reference semantics without blindly forcing `see_pictures` on every mode. Keep people detection and prompt-writing vision separately understandable and actionable.
  - [ ] On explicit analysis/draft action, resolve the exact selected media through existing safe job-bound reads/import/upload paths as needed; do not require the user to move files manually into input. Preserve reference order, labels, boxes, trims and source identity. Pictures must reach the selected vision model as image content, not just names. For video, verify the supported frame-sampling path and intended trims; do not claim audio understanding or arbitrary media support where the chosen model/route lacks it. Explain genuine missing model/capability errors rather than source-folder gating otherwise valid media.
  - [ ] No analysis/upload merely on navigation, selection or opening Forge; keep explicit consent for sending local content to an external model endpoint. Preserve originals, manual boxes, drafts and cancellation; stale analysis must not overwrite changed references. Reuse existing APIs, with separately authorized upstream work if a required node-pack route is genuinely missing.
  - [ ] Add Forge/PeopleDetector/native transport tests for input, generated output, library/external images, frame inputs, selected references and supported video sources; labelled and mixed-media modes, unsupported vision, refusal/cancellation and selection races. Inspect captured requests for real media payloads and exact source correlation. Prove an actual vision-backed draft and people detection on non-input-folder media, plus supported video-frame understanding, separately from fixtures; native Linux/Windows acceptance, six locales, global search and `docs/STUDIO.md` updates remain required.

- **Multi-reference → Video: Auto aspect must follow the first input image (Director parity). Status: planned, not implemented.** Owner reports that Auto aspect selects the wrong aspect instead of using the first input image as the Director does. Source entry points: `src/features/studio/Workbench.tsx` derives `source` from `mediaSize` matched to `media.src`, and its Auto ratio falls back to 1 when that source is absent; multi-reference inputs are held separately in `refs`. Trace this path and the actual Director first-image selection/geometry before fixing it; the reported case has not yet been reproduced.
  - [ ] For Multi-reference → Video with Auto aspect, resolve dimensions from the first input image in reference order, not the browsed preview, an unrelated source or a square fallback. Match the Director's handling of preceding video/audio references and first-image selection; inspect its constants, orientation handling and rounding rules. Explicit aspect/custom/fixed-size overrides retain their documented precedence; missing/unreadable dimensions must not silently produce a wrong square result.
  - [ ] Recompute after reference addition, removal, replacement and reorder; fence asynchronous dimension reads so a stale image cannot overwrite the current first-image geometry. Preserve reference order, timing, strengths and submitted-job snapshots. Derive final dimensions with the active family's exact pixel-budget/grid rules, and keep displayed dimensions, Run Plan and submitted intent identical rather than promising an arbitrary source ratio is exactly representable on that grid.
  - [ ] Add focused `Workbench.test.tsx`/geometry regressions using portrait and landscape first images, conflicting later-image aspects, reorder/removal/replacement, delayed reads, non-image leading references and explicit overrides. Compare exact expected dimensions with the Director source and verify an affected H3 real run; check other supported multi-reference families independently. Native Linux/Windows acceptance and `docs/STUDIO.md` updates remain required; no changes to unrelated pipelines or manual-aspect graphs.

- **Animated live video previews during generation. Status: planned, not implemented.** Owner request. Replace the uninformative blurry still with a meaningful temporal preview wherever the active model/backend supports it. Owner reference: H3 workflow using KJAI and a tiny VAE, approximately 120 preview frames at 8 fps; inspect that workflow and its actual preview transport before choosing the adapter. Check LTX separately rather than assuming it emits the same format. These are preview parameters, not changes to generated frame counts or output fps.
  - Source baseline: `src-tauri/src/jobs.rs::preview_frame` accepts only individual JPEG/PNG binary event-1 images; `src/features/studio/Workbench.tsx` renders a running job's `shown.preview` as an `<img>`. This establishes the still-image path, not the precise cause of blurry decoding or whether H3/LTX already emit usable temporal previews.
  - [ ] Trace preview production, tiny-VAE availability and temporal framing through the exact H3/KJAI reference and LTX implementation; record supported contracts and missing capabilities before integration. Prefer existing preview APIs; do not patch dependencies or silently install/download preview models.
  - [ ] Add an **Enhanced preview** switch under Options to enable or disable the temporal preview. Default **ON when a compatible tiny VAE is available**, including for existing settings with no saved preference; an explicitly saved OFF remains OFF. Without a compatible tiny VAE, show the switch as unavailable with a reason and retain the ordinary preview; do not silently download a VAE. This owner-requested default is an explicit exception to the usual default-OFF rule for new optional switches. Localize the label/reason in all six languages and expose the control through global search.
  - [ ] Carry and play the supported frame sequence/animated preview at its preview fps when Enhanced preview is enabled, correlated to the selected job; bound buffering, discard stale updates and release resources on selection changes, completion and cancellation. Switching it OFF restores the ordinary preview without changing generated frame counts, output fps or final video/audio products. Test tiny-VAE availability, absent saved preference, saved ON/OFF and toggling; explain a missing animated-preview capability instead of presenting a still as a motion preview.
  - [ ] Add native transport/parser fixtures and UI playback/lifecycle tests; real H3 and LTX runs must demonstrate visible motion during sampling, measured memory/performance and unchanged generated outputs. Confirm the owner's approximate 120-frame/8-fps reference rather than imposing it on every family. Linux WebKitGTK/libmpv and Windows WebView2 acceptance, six locales and `docs/STUDIO.md` remain required.

- **Continuity enable/disable: native acceptance.** The **Continuity active** switch landed in 0.1.67 (frontend-only; intents unchanged, see [Studio](STUDIO.md)). Open: native Linux WebKitGTK and Windows WebView2 click-through.

- **Paste and replace-on-paste for references (W2b′).** Clipboard files have no path; this needs a new native byte-upload command (sniffed, size-capped, written to `deas-inputs/`), which is a trust-boundary change of its own.
- **Trim editor remainder.** Whole-crop move: needs two finite endpoints, preserves the 2–15 s span, clamps to `[-start, 3600-end]` and updates both ends atomically (3600 is a safety bound, not source duration). The Director's keys move the playhead (Left/Right ±1 s, Shift ±0.1 s); do not call trim nudges Director parity. Waveforms and thumbnail strips.
- **Create RefMod (W2e3).** Offer the Director's *Create RefMod*: encode selected references via the node pack's `DaSiWaH3RefModExport` into `models/refmods` so they appear in `/dasiwa/refmods`.
  - [ ] Source-check the node signature, required H3 VAEs, input kinds and output naming; no pack patching from DEAS.
  - [ ] Separate export intent/graph, validated offline like `dump_video_graphs`; explicit Run, cancellation checked before any write.
  - [ ] **Create RefMod** on selected references with the Director's strength/name semantics, six locales; refresh the RefMod library afterwards.
  - [ ] Component/native/offline-graph tests; real H3 VAE encoding and GPU time separately.
- **References in the main view (W7).** Multi-reference already uses the References modal. Still open: the Image → Video start image, the first/last-frame pair and the Video → Video source as stage slots with replace/remove/drop, keeping `refs`, `lastFrame` and the intent shape unchanged.
- **Join clips and H3 transitions in Continuity (Director parity gap).** The Director's Continuity row has **Join clips…** (`docs/h3_continuity.md#join-exported-clips`): (a) *Continuity segments* — assemble ready exports of one session into one video without loading a model (**Select chain to here**, superseding full-timeline exports); (b) *Video files* — join uploaded/timeline videos with trims, optional lossless packet copy, re-encode H.264/AAC otherwise; (c) *H3 transitions* — one short H3 sampling run per cut with `context_frames` guides from both neighbours. Nodes: `DaSiWaH3ClipAssemble` (`assembly_data {version:1, rows:[{kind:'media'|'checkpoint',…}], repair_transitions:false, audio_policy:'preserve', copy_video}`), `DaSiWaH3TransitionGuide` + `DaSiWaH3TransitionRender` (`transition_data {version:1, rows:[a,b], context_frames:22, transition_frames}`), all listed in the pack's `__init__.py`. DEAS Continuity (`ContinuityPlanner.tsx`, `jobs_video.rs`) emits only Prepare/Decode/Publish; `H3_CONTINUITY_NODES` does not contain the three join nodes, so the owner must switch to the Comfy tab to assemble. **Status: planned, not implemented.** Owner request.
  - [ ] Source-check the three nodes' signatures, `assembly_data`/`transition_data` schemas and the 24-fps/SDR/`yuv420p` media constraints; pin the JSON contract in a Rust test like the Director timeline contract.
  - [ ] Continuity overlay gains a **Join clips** section: session segments (chain selection from `native_h3_session` clips, export coverage check) and video files (Media picker, order, trims, copy-video switch), with the preflight duration/mode summary before Run.
  - [ ] Optional **H3 transitions** per cut (context/transition frame counts, defaults 22/29 as the Director), emitted as Guide → existing sampler → Render → Assemble; separate `videoPost`-style product (no Continuity checkpoint mutation).
  - [ ] Unchanged: Prepare/Decode/Publish graphs, Continuity plan shape and `continuity` intent when the section is not used; intent gains an optional `h3.join` block only.
  - [ ] Native validation + prerequisites (`H3_CONTINUITY_NODES` + join nodes only when used), offline graph validation (`tools/validate_video_graphs.py`), UI tests, six locales, STUDIO.md; one real join (copy and re-encode) and one real transition run on a throwaway backend.
- **Later recipe features (W6).** LTX keep source audio (audio VAE encode + 3-D noise mask), LTX IC-LoRA control (`iclora_parameters` on core `LTXVAddGuide`), LTX per-segment prompts (GPL Prompt Relay patch, explicit opt-in only), LTX retake, source-fps resampling, H3 Image Inpaint as its own pipeline label, Wan T2V/S2V, per-model Wan LoRA lists. Unplanned ideas: colour transfer, seamless loop, watermark/soundmark, NAG, TritonVAE.
- **Real GPU acceptance (W5).** On a throwaway ComfyUI on another port (never the owner's backend): one LTX v2v, one LTX keyframe run, one H3 REF2VA with a 2–15 s clip, one H3 image edit with and without mask. Record outputs, time, VRAM and visible quality. Questions: does H3 frame 0 carry the edit; is unresampled LTX source fps acceptable; does `VAEDecode` after `LTXVSeparateAVLatent` decode H3 correctly.
- **Learned latent upscale.** Real model quality/performance; strict baseline-first preservation for the paired Original/Processed outputs.
- **Default H3 input scaling: Auto, not Fill and crop (Director parity). Status: planned, not implemented.** Owner request. The owner calls the current default "fit and crop"; the actual option label is **Fill and crop**. DEAS defaults to it, while the Director node's own default is **Auto**: `nodes/nodes_minimax_h3_director.py` reads `input_scaling` with fallback `"Auto"`. Source entry points: `src/features/studio/Workbench.tsx` — `usePref('studio.run.video.h3Scaling','Fill and crop')`, the reset/pickPipeline defaults, and the intent builder that sends `inputScaling` only when it differs from `'Fill and crop'`; `src-tauri/src/jobs_video.rs` fills `timeline_data.resolution.input_scaling` with `unwrap_or("Fill and crop")`; `src/features/studio/RunPlan.tsx` displays `'Fill and crop'` for an absent field; `Workbench.test.tsx` and `jobs_video_tests.rs` pin the default.
  - [ ] An absent saved preference yields **Auto**: pick one representation — omit `inputScaling` (the Director treats a missing key as Auto) or send `"Auto"` explicitly — and pin it in the Run Plan display, native timeline fill and both test suites. Explicitly saved values, including a saved **Fill and crop**, restore unchanged; drafts and intents captured before the change round-trip unchanged.
  - [ ] The scaling select's options, labels and hints (six locales) and the non-H3 `inputScaling` control stay unchanged; no other family's graph changes.
  - [ ] One throwaway-backend H3 run with an aspect-mismatched reference compares Auto against the current Fill-and-crop result, and `docs/STUDIO.md` names the new default.
- **Playback acceptance.** On Linux, embedded libmpv has replaced the old WebKit/GStreamer path (and its Matroska crash) for all Workbench video. Still open: libmpv runtime acceptance (audible audio, A/V sync, seek/EOF/reopen, resources), WebView2 video on Windows, and WebKit `<audio>` (music/FLAC, audio references) with the bundled GStreamer plugins. Keep `src-tauri/gstreamer/`: the plugins are compiled in via `include_bytes!` and serve `<audio>`.

### 4.2 Video post-processing (TVE) and paired products

Rules: ComfyUI keeps model-dependent work; Rust owns inspection, source retention, recipe compilation and validation. When optional post-processing is enabled the user gets **two real encoded products** (unprocessed baseline and processed result), both selectable/exportable and kept after failure/cancellation; the imported original is preserved. No extra lossy round trip just to save the baseline.

Owner decisions 2026-10-30: (1) the TVE tab compiles the same ComfyUI post recipe the pipelines use (`video-enhance`: LoadVideo → upscale model / `DaSiWa_RTX_UpscalerRefiner` / `DaSiWa_RTX_FrameGeneration` or core RIFE → `DaSiWa_EnhancedVideoCombine`) on the owned backend; (2) the standalone Rust ONNX engine (`video-upscale` feature, `ort`, worker process, output draft/preparation/binding) is retired — it duplicated a strict subset of the backend chain under ≤512 px/≤10 s limits. Shipped in 0.1.67 (see STUDIO.md "TVE post-processing on the backend"): pipeline `videoPost` with B7/7 paired products, all combine codecs/containers/bit depths, model selectors from `upscale_models`/`frame_interpolation`, queue/preview/export through the shared session. 0.2.7 adds the TVE panel's own job polling (a submitted job no longer stays "Queued" while the Studio queue is closed), the RTX `use_mmap`/frames-per-chunk controls and a truncated-safetensors check in the model scan. Open: real GPU acceptance of each stage on the owned backend (RTX nodes need the installed `nvidia_vfx` SDK), HDR, restoration beyond SDK denoise/deblur, baseline-before-processed ordering (PP1 applies to the video savers too), and exact processed geometry in the first/last-frame PNG check (accepted at decoded size).

- [ ] **PP1 strict ordering.** Paired image refine products ship (0.1.51), but sibling savers do not enforce baseline-before-refinement; a failure before the baseline saver runs can leave no baseline.
- [ ] **Paired-video acceptance.** Baseline boundary per chain; both products in results/library/compare/export with six-language role labels; decoded readback (dimensions, frame count/timing, audio, colour/bit depth); measured RAM/VRAM/throughput per platform.

Reference: TrueVideoEnhancer `67ddb2508fdb6da7f1897bbd964f316592f762d0` (`backend/custom_pipeline.py`, `streaming_pipeline.py`, `render_contract.py`, `nvidia_vfx_pipeline.py`) is a behavioural reference only. Its rawvideo path loses source PTS, its host budgets exclude GPU tensors and its final publication lacks a cancellation check. GPL code reuse does not clear model, NVIDIA SDK or FFmpeg terms.

### 4.3 Music (YuE2)

Delivered in 0.1.66 (see [MUSIC.md](MUSIC.md)). Open:

- [ ] Cover source preflight: actionable no-audio-stream error and decoded-PCM memory bounds for the input (only output FLAC is bounded today).
- [ ] Live-backend SheetSage2 list multiplicity with real weights.
- [ ] External-backend FLAC export: `jobs_outputs_tests.rs::music_flac_outputs_are_decoded_before_preview_and_export` covers fetch/decode/no-clobber export; confirm it runs without `outputRoot`, then close.
- [ ] Real Text to Music (Full/Melody/Off), audio and video covers, scoped cancellation.
- [ ] Native FLAC playback on WebKitGTK and WebView2; Windows runtime.

Constraints kept from the implementation plan: signatures from one checkpoint do not cover every YuE2 variant (unknown variants stay unavailable; no filename fallback); missing optional nodes disable only the affected modes; no transcription, voice cloning, extra codecs, cloud fallback or automatic installer changes.

### 4.4 Image: masking, references, regional prompting

- **Inpaint colour pipette / screen eyedropper. Status: planned, not implemented.** Owner request. Add a pipette control beside the Inpaint editor's colour picker to select a colour from the screen and apply it to the existing colour setting. Source baseline: `src/features/studio/StudioImageEditor.tsx` exposes `<input type="color">` with `setColor`, but no explicit eyedropper control was found. Reuse that colour state; picking a colour must not paint, alter a mask or start generation.
  - [ ] Check screen-wide eyedropper support in the actual Linux WebKitGTK and Windows WebView2 runtimes before choosing browser `EyeDropper` or an existing native/desktop-portal path. Sampling only the image canvas does **not** satisfy screen-wide picking. Use explicit user activation and any required platform consent; no persistent screen capture, screenshot storage or silent permissions. If unavailable, explain the limitation and retain manual colour selection rather than claiming screen-wide support.
  - [ ] Provide an accessible pipette button and active picking feedback; a confirmed pick updates the colour swatch/value, while Escape, cancellation or permission refusal leaves the previous colour and editor state intact. Preserve brush/mask semantics, focus, zoom/pan, layers and existing input/intent/recipe contracts.
  - [ ] Add `StudioImageEditor.test.tsx` coverage for success, cancellation, unsupported capability and errors; extend `tools/verify_inpaint.py` for control placement/accessibility without pretending a mocked pick proves OS support. Native Linux and Windows acceptance must pick a known colour outside the app. Update six locales, global search and `docs/STUDIO.md`; run the layout gate for toolbar changes.

- **SAM 3.1 masking (planned).** Text → mask and click/box → object mask via core `SAM3_Detect` (`CheckpointLoaderSimple` → `CLIPTextEncode` → `SAM3_Detect` with `bboxes`, `positive_coords`/`negative_coords`, `threshold`, `individual_masks`; `SAM3_VideoTrack` later). Preferred over SAM2 + GroundingDINO (two packs, two models). DEAS already detects `sam3` checkpoints but has no pipeline for them. Limits: text prompts ≤32 tokens; the weights `sam3.1_multiplex_fp16.safetensors` are under the custom SAM License (license inventory and explicit download consent first); VRAM is unmeasured.
  - [ ] Recipe `sam3-mask` returning a new editor mask layer (never overwriting masks); both compilers and `recipe_prerequisites` list the node and checkpoint.
  - [ ] Prompt field plus click/box tool, mapped through the brush's source-pixel transform.
  - [ ] Missing model/node shows the install path; real run with measured time/VRAM.
  - Frontend-only follow-ups: select-all-similar colour (non-contiguous wand), edge stop (gradient barrier).
- **More reference techniques.** SDXL IP-Adapter (external `ComfyUI_IPAdapter_plus`; consent/license check first), Flux Redux via core style-model nodes, edit models (Flux Kontext/Flux.2/Qwen Image Edit) through their own reference nodes, OS-file drops onto the prompt, and video/audio chips reusing the H3 reference contract. Unsupported families must explain why instead of ignoring references.
- **Regional prompting (planned).** Opt-in regions (name, positive and optional negative prompt, strength, rectangle or painted mask), SDXL first; the global prompts stay. No enabled region means an unchanged graph.
  - [ ] Prove a two-region SDXL recipe on the owned backend first (`ConditioningSetArea`/`ConditioningSetAreaPercentage`/`ConditioningSetMask`/`ConditioningCombine`) against a fixed-seed baseline; record placement, leakage, latency and VRAM.
  - [ ] Typed region data in `execution.ts` and native validation in `jobs.rs`; required nodes in `recipe_prerequisites.rs`; families in `builtin.json`; bounded payloads and in-bounds geometry.
  - [ ] Region editor via Workbench and `StudioImageEditor`; text-to-image needs an output-sized canvas; region masks stay distinct from inpaint masks.
  - [ ] Coordinates against the generation canvas through zoom/pan/resize/crop-inpaint/refine; snapshot before uploads; Run Plan/metadata/preset serialization.
  - [ ] Compiler parity tests, UI tests and a real artifact per advertised family; six locales and guides.

### 4.5 Models

- **User-defined recipe profiles (M-02, planned).** M-01 (the Edit meta… family override) shipped in 0.1.66. M-02 would add:
  - a versioned, data-only local profile binding an unknown model to an existing recipe adapter, with exact VAE/encoder references;
  - adapter-supported defaults/constraints and explicit supported modes;
  - a stable profile ID, with revalidation when components are replaced;
  - distinct states *Configured* / *Backend requirements checked* / *Successfully executed*;
  - optional bounded import/export (no credentials or weights).

  A genuinely new architecture still needs a reviewed adapter. A later *Import example workflow…* may only assist the mapping; it must never execute arbitrary graphs.
- **Split diffusion models for checkpoint-only families (planned).** The Studio checkpoint selector and the Models tab list YuE2 only as an all-in-one checkpoint in `checkpoints/`: the family has `"loader": "checkpoint"` and its header rule needs both `model.diffusion_model.llm2vae.weight` and `text_encoders.model.embed_tokens.weight` (`builtin.json`, `model_families.rs`). A diffusion-only YuE2 file in `diffusion_models`/`unet` is therefore never listed. ComfyUI core already supports the split form (inspected at `b1a247c9`): `UNETLoader` detects it from `vae2llm.weight`, `llm2vae.weight`, `latent_pos_embed.pe`, `model.layers.0.self_attn.qkv_proj.weight` and `time_embedder.mlp.0.weight` (`model_detection.py`), `CLIPLoader` has type `yue2`, and `VAELoader` recognizes the YuE2 audio VAE (`sd.py`). (A file in a misnamed `checkpoint/` folder is not scanned by ComfyUI or DEAS; that is a placement problem, not this gap.)
  - Correct pipeline use comes first: list a loading form only where it really runs through Text to Music **and** Cover Music. Verify each supported form end-to-end — detection, Run Plan, compiled graph, SheetSage2 cover path, decoded FLAC result — before advertising it; a listed but non-working form is a defect, not partial support.
  - Detect diffusion-only YuE2 files by those header keys and list them in the selector and Models tab for Text to Music and Cover Music, alongside the existing checkpoint form.
  - Compile a split music recipe (`UNETLoader` + `CLIPLoader type=yue2` + `VAELoader`) with header-backed companion matching for the text encoder and VAE slots, missing-companion Run gates and unchanged graphs for the all-in-one checkpoint.
  - Apply the same audit to other families that are checkpoint-only today but whose core loaders accept split files; GGUF split variants follow the existing GGUF rules.
  - Native/TS compiler regressions, offline graph validation, six locales, then a real run per loading form in both music pipelines: the all-in-one checkpoint (including a quantized one such as `convrot_int8`, whose loading/quality is unverified) and the split files; MUSIC.md and the family table updated.
- **GGUF.** Real bounded image/video/audio-in-video GGUF inference and the Windows runtime. ComfyUI-GGUF has no full-checkpoint or GGUF VAE loader, so a manual family assignment can still fail on unsupported labels.

### 4.6 Studio UX

- **Media panel: clipboard drop/paste intake field. Status: planned, not implemented.** Owner request. Add a small field left of the "Drop files or folders here, or browse. Nothing is copied or moved" hint (`media.dropHelp`, `src/features/media/MediaLibrary.tsx` header, `.media-drop-hint`) that accepts a dropped image/video **and a Ctrl+V clipboard paste** and automatically brings the media into the input library for further usage. OS file drops already work via the native pattern (`onDragDropEvent` + `describeMedia(paths)`, `MediaLibrary.tsx` ~120); clipboard content has **no path**, so paste needs the byte-ingest native command already planned for W2b′ (sniffed, size-capped, written under the input root, never overwriting — timestamped unique names) — no parallel upload path.
  - [ ] One narrow drop/paste field on the hint row, fixed width, rows aligned (panel layout gate if the header reflows); accepts `image/*`/`video/*`, ignores folders/text/other files with a one-line status, never an alert. Paste active only while the media panel is visible, fenced by open-dialog/modal ownership (reuse MediaLibrary's modal-ownership pattern so a Forge dialog keeps its own drops/pastes).
  - [ ] After ingest, refresh the input listing and select the new entry so it is immediately usable as the authoring media; originals untouched, "Nothing is copied or moved" remains true of the folder browser — the new field is an explicit copy-in gesture and must be labeled as such.
  - [ ] `MediaLibrary.test.tsx` coverage: mocked native drop + paste intake for image/video, non-media ignored, unique-name no-overwrite, refresh+select, paste fencing behind modals, unmount cleanup. Native Linux WebKitGTK + Windows WebView2 paste acceptance (`navigator.clipboard.read()` support differs per webview — degrade with an accurate reason where absent), six-locale labels/tooltips, global-search entry and `docs/STUDIO.md`.

- **Ctrl+F: find in the current view. Status: planned, not implemented.** Owner request. Open a browser-like find bar over the current view with Ctrl+F; entering a search term highlights matching text in that view. Keep this separate from Ctrl+K global control/navigation search.
  - [ ] Reuse existing search/highlight mechanisms where suitable after inspecting current view containers and keyboard handlers. Search only the active visible view, excluding retained hidden tabs and closed overlays; do not search the whole application or change prompt contents. Highlight matches, show current/total count and offer previous/next navigation (Enter/Shift+Enter); Escape closes the bar, clears highlights and restores focus. Reopening retains the current query for the session. Empty queries and no matches have clear states.
  - [ ] Preserve editable-field text/selection, IME input, existing shortcuts and mounted-view/job lifecycles; highlighting must not rewrite controlled textarea values or disrupt native media. Check the embedded Comfy view separately: cross-origin iframe content needs its own supported find path, not a claim that parent-DOM search covers it. Report any unsupported surface explicitly.
  - [ ] Add focused UI tests for Ctrl+F opening, matching/highlighting, next/previous wrapping, empty/no matches, Escape/focus return, active-view changes and hidden-view exclusion. Verify editable prompt fields and embedded Comfy behavior on Linux WebKitGTK and Windows WebView2. Add accessible labels, six-language text, a global-search entry and `docs/STUDIO.md` shortcut documentation; run the shared layout gate if the bar changes layout.

- **Prompt Undo/Redo. Status: planned, not implemented.** Owner reports Ctrl+Z does not undo prompt edits and Ctrl+Y does not redo them. Audit the shared `src/features/studio/PromptTextarea.tsx` and its callers (Studio positive/negative prompts, enlarged editor and Forge prompt fields). Its tag/spelling replacement paths set the textarea value programmatically and dispatch input; whether native undo survives those edits needs reproduction, not an assumed diagnosis.
  - [ ] Restore field-local Undo/Redo for typing, paste, selection replacement, autocomplete and spelling corrections, including caret/selection restoration; a fresh edit after Undo clears Redo. Reuse browser editing/history where reliable rather than creating an application-wide history framework. Preserve drafts, IME composition and unrelated editor shortcuts.
  - [ ] Regression tests in `src/features/studio/PromptTextarea.test.tsx` and affected caller tests; native WebKitGTK and WebView2 keyboard checks with ordinary edits and assisted replacements. Include enlarged-editor open/close and field isolation; document shortcuts in `docs/STUDIO.md`, localizing any changed labels/help in all six languages.
- **Ctrl+Enter runs: native acceptance.** Landed with component tests. Open: Linux WebKitGTK keyboard check; held keys and IME composition on a real keyboard.
- **Stop asks: selected item or all. Status: planned, not implemented.** Owner request. The main Stop button must open a keyboard-accessible choice of **Stop selected item**, **Stop all** and **Cancel**, not immediately abort everything. Disable selected-item cancellation with an explanation if selection is absent or terminal. Current `Workbench.tsx::stop` invokes `stopWorkbench()` and clears global preparation/auto-plan state; `stopJob` already calls `stopWorkbenchJob(job)` from a queue-row context menu. Reuse these scoped versus whole-queue paths rather than introducing a second cancellation system.
  - [ ] Selected-item Stop captures the chosen job identity, removes only its pending prompt or safely interrupts only that owned running prompt, and leaves peer jobs, preparations and queue admission intact. If more items are queued, the backend continues with the next item without another Run press. Do not apply whole-queue preparation/auto-plan invalidation to a single selected item; reconcile H3 sequencing only where that selected job owns the sequence.
  - [ ] Stop all retains existing whole-queue admission fencing and cancellation safeguards; Cancel/Escape does not mutate jobs. Never silently fall back from unsupported scoped interruption to global interrupt. Preserve completed products and report partial failures/unknown states truthfully.
  - [ ] Extend `Workbench.test.tsx` and the existing native jobs cancellation tests: pending selection, running selection with another queued job progressing, changed selection while the choice is open, terminal/no selection, Cancel, whole queue, delayed preparation/admission and rejected/unsupported interruption. Real backend cancellation/next-job progression plus Linux/Windows dialog acceptance remain separate gates. Update tooltips, accessible names, global-search entry and all six locales alongside `docs/STUDIO.md` when implemented.

- **Startup splash (artwork and acceptance open).** The splash (`public/splash.html`, `src-tauri/src/splash.rs`) shows the wordmark and real phases, keeps main hidden until the restore settles, has a 30 s hand-off deadline and a reduced-motion bar. Open:
  - Owner-provided Catgirl idle animation into `public/splash/` (`catgirl.apng` preferred, `catgirl.webp`, static `catgirl.png` also used for reduced motion); do not substitute artwork.
  - Acceptance beyond the Linux XWayland probe (fresh environment): warm start with a remembered environment, slow/failed restore, disabled autostart, Wayland-native and Windows.
- **Global search (landed in 0.2.7, open).** The magnifier entry in the top tab band, Ctrl+K, the breadcrumb result list and reveal/flash of controls are implemented over the curated index in `src/lib/search.ts` (`GlobalSearch.tsx`); a disabled control reports its own tooltip first, then the entry's reason key, then the generic greyed note. Open:
  - Native acceptance on WebKitGTK and Windows: overlay placement at narrow widths, focus return after Escape, flash under reduced motion (Chromium at 1440/900/480 px and reduced motion are verified in `docs/VERIFICATION.md`).
  - The index is a curated set (Studio settings/prompts/dock incl. Presets and Wildcards, Forge prompts and ladders, Chat, TVE post incl. frame-rate multiplier, Run Plan stages, Library catalog+installed incl. scan scope, Comfy iframe, Backend panes/folders/language, Logs filters/actions); extend it only where a control has a stable anchor.
- **In-app spellcheck (landed in 0.2.8, acceptance open).** `spellbook` checks prompt fields on every platform against the pair installed under Backend → Language (`spellcheck.rs`, `src/lib/spellcheck.ts`, `PromptTextarea`); the WebView/Enchant path is gone. Open:
  - Native acceptance of the underline overlay (font/line metrics mirror) on WebKitGTK and WebView2, including resized Studio panes and the enlarged editor; the engine itself is verified with fixtures and the pinned pairs.
  - A personal "add to dictionary" list for recurring prompt vocabulary (`photorealistic`, model names) is not implemented; such words are flagged.
  - Fields outside `PromptTextarea` (Comfy iframe, Forge card/layout captions, Chat history) are not checked.
- **Queue statistics expansion.** Client-measured elapsed queued/running time for live rows (marked as client-side) and steps/seed/checkpoint in the detail view; no new tracking.
- **Monitoring tab (landed in 0.2.9, open).** Generations (last ten finished attempts with queue wait, generation time, timeline bar and per-job peaks from the native job sampler) and System (one graph per component from a native `/proc`/`/proc/diskstats`/Win32 + `nvidia-smi` sample per second). Open: native acceptance on WebKitGTK and WebView2 (six canvases repainting at 1 Hz, narrow widths); AMD/Intel GPUs and the node pack's `/dasiwa/system-monitor` route (would add non-NVIDIA GPUs when the backend runs) are not read; peaks are per first GPU only; the Queue panel keeps its rows (this tab supersedes the "Queue statistics expansion" row above).
- **Wizards tab (planned).** A top-band tab with "EZ wizards" that pick model, pipeline and settings themselves; the user supplies only media, a mask (optional) and a short instruction. A wizard checks the installed models against its preference list and, when nothing fits, offers the Library download (consent + license as today) instead of failing silently. Internally a wizard builds an ordinary image/video intent (`buildImageIntent`/`buildVideoIntent`) and runs through the shared Run/queue/preview — no new execution path; the Run Plan shows the chosen model and values. Order of research/implementation: (1) **Replace** a character/object/characteristic in an image or video: images via Qwen Image Edit 2.1 (`nativeImageReference` method `qwen-image-2.1`, already compiled) or inpaint with mask; optional SAM 3.1 text→mask (§4.4) for the masked area; videos via H3 image edit per first frame + I2VA or LTX v2v with keyframe guides — research which gives the better replacement (owner's guess: H3) and record the fixed-seed comparison before choosing the default. (2) **Mood / colour / emphasis overhaul** of an image: prompt built from Prompt Forge presets and wildcard preset mixtures (`wildcard_preset_prompt_builder.md`, DEAS presets `studio-presets.md`) plus a small curated list of cinematic/colour-grade preset mixtures, run as low-denoise i2i with the family's refine defaults. **Status: planned, not implemented.** Owner request.
  - [ ] Wizard definition = data (preferred families/models in order, pipeline, settings, prompt template, needs-mask flag), not code per wizard; unknown models never guessed.
  - [ ] Replace wizard for images (Qwen Edit → inpaint fallback), then videos after the H3-vs-LTX comparison is recorded in VERIFICATION.
  - [ ] Mood wizard with preset picker and a before/after Slider result (existing paired-products presentation).
  - [ ] Download suggestion path reuses Library consent/license dialogs; no automatic downloads.
  - [ ] Unchanged: Studio tabs, intents and graphs; a wizard run is an ordinary job in the queue.
  - [ ] Tests: wizard→intent mapping per family, missing-model suggestion, mask handling; one real run per wizard; six locales, STUDIO.md.
- **Project persistence (deferred).** The full target, if resumed:
  - complete Studio authoring state;
  - managed inputs, adopted results and job records;
  - an extension of the versioned `project.json` (no parallel schema);
  - one portable ZIP with credentials and weights excluded, and safe extraction;
  - reopening without computation.

  Existing draft projects stay as they are.
- **Native acceptance still open for shipped UX slices:**
  - star/unstar round-trips (Unicode, collisions, permissions, restart);
  - column resizing and the remembered session;
  - Quick Settings resets and project/global isolation;
  - LLM-setup routing from every entry point;
  - Settings dropdown rendering and dependency-warning clearing in WebKitGTK/WebView2;
  - contained bottom media controls (U-02).

### 4.7 Library and downloads backlog

- One flow per card (selection, destination, Prepare, consent and Start inside the picker); one-click single-variant download.
- Exact sizes (HEAD/Content-Length) and a free-space check in the plan.
- Per-version browsing beyond the 50-file listing cap; mark cards whose files are already local.
- Variant labels (quant suffix, size) for Hugging Face/HTTP sources; grouping exists only for CivitAI.
- Resumable downloads.

### 4.8 Installer, backend and distribution

- AMD/Intel profiles and preflight; multi-GPU selection (`install_torch.rs` refuses more than one GPU today); free-space estimates.
  - ROCm startup rules, from research (2026-11-01, no AMD hardware here): never offer `--fast fp16_accumulation` on ROCm — PyTorch applies `CUBLAS_COMPUTE_16F` only under `#ifndef USE_ROCM`, so it is a GEMM no-op, but ComfyUI still sets `PRIORITIZE_FP16` and picks fp16 over bf16 for UNet/text encoder (black/overflowing output on Flux/Qwen-class models; ComfyUI PR #9258 enabled it for AMD untested). Keep Speed+ NVIDIA-only (`startup.rs` already does). Offer `--force-fp32`/`--bf16-unet` as the first diagnostic for GGUF noise on RDNA3 (city96/ComfyUI-GGUF #300: noise on GPU, correct on CPU, 7900 XT/XTX, root cause open) and `--disable-smart-memory` for the ROCm 7.15 / torch 2.13 weight corruption (ComfyUI #15314, black frames). ComfyUI-GGUF has no dequant-dtype switch; there is no documented "fp16 accumulation bug with GGUF on AMD" beyond these two separate facts.
- Git bootstrap, or removal of the external Git prerequisite; full FFmpeg/attention-extension provisioning.
- Interrupted-install repair/resume; staged environment upgrade with rollback (reinstall replaces the venv, which is not rollback).
- Pin the DaSiWa node-pack ref. It is cloned from `main` with the revision recorded but not user-selected, which falls short of the explicit-updates decision.
- Settings relocation on Windows (`settings_move.rs` rejects non-Linux).
- **Selective backend updates: native acceptance.** Implemented in 0.1.67: Update asks Everything / DEAS app only / ComfyUI and node packs / PyTorch only (see [Studio](STUDIO.md)). Open: a real Linux run of each action (a PyTorch update needs an environment with an older profile) and Windows separately. The compatibility blockers named below need no extra gate today: owned environments cannot carry attention extensions (sage/flash/radial plans are refused), ComfyUI-GGUF is pure Python and is reinstalled under the new constraints, and TVE's ONNX runtime is a user-chosen native library outside the venv. Node packs DEAS did not install block the update. Original spec:
  - **Update ComfyUI**: ComfyUI checkout, installed node packs and their Python dependencies, under the existing Torch constraints so a node-pack requirement cannot silently replace Torch. No DEAS binary download.
  - **Update PyTorch**: only to a newer profile this DEAS build knows (`install_torch.rs`), never an arbitrary latest wheel. Offer it only when the new profile's hardware/driver gate passes, and block it when ComfyUI, the installed node packs or DEAS features are not known to work with it (e.g. attention/CUDA-extension pins, GGUF, the CUDA ONNX runtime used by TVE). Show the exact from → to versions before confirmation.
  - Both: stop the owned backend first, block while a job runs, reuse the existing plan/consent/cancellation/log paths, persist the updated plan only after the verify step passes, and report the previous state on failure (no rollback is claimed until staged updates exist). The full **Update** stays available.
  - Acceptance: fixtures for each action alone, failed verify (plan unchanged), a node-pack requirement that would change Torch, unsupported hardware for a new Torch profile, cancellation; a real Linux run of each action; Windows separately.
- **Packed release assets (planned).** Linux `deas-linux-amd64.tar.gz`, Windows `deas-windows-amd64.7z`.
  - Detect packing by magic bytes (`1f 8b` for gzip, `37 7A BC AF 27 1C` for 7z).
  - Verify the archive digest and size first.
  - Extract exactly one entry with the expected executable name: reject paths, dirs, symlinks and extra entries, and bound the decompressed size. Then apply the existing header and atomic-replacement checks.
  - Raw assets stay accepted in both directions (old updaters with new releases and the reverse).
  - Fixtures for raw, tar.gz, 7z, extra/symlink/traversal entries, decompression bomb, digest mismatch and mixed releases.
- Signed/update-verifiable distribution, fresh-machine/Git-free setup, glibc portability and current CI readback.

### 4.9 Reference-project ports

These are the local reference repositories; their functionality does not count as implemented in DEAS. Each port needs an explicit owner priority, a Rust-first native boundary and its own Linux/Windows acceptance.

- **Video comparison:** `<workspace>/DaSiWa-simple-video-compare` (Rust/Tauri/libmpv: `src-tauri/src/native.rs`, `embedded.rs`, `embedded_windows.rs`, `runtime.rs`, `desktop/src/model.ts`).
  - Share the player/probe/runtime boundary with DEAS's libmpv player.
  - One/two-source side-by-side, wipe, blend and difference; seek, frame-step and source audio.
  - Test mismatched dimensions/fps/duration, short-source EOF and missing audio.
  - Filters normalize to `yuv420p`, so this is not bit-exact HDR analysis.
  - Assess GPL/libmpv/FFmpeg redistribution first.
- **Video assembly:** `<workspace>/DaSiWa-simple-rtx-video-assambler` (`vid_tool.py`; spelling retained).
  - A Rust FFmpeg plugin: ordered sources, labels, grid layouts (contain/cover/stretch), audio selection/mixing, NVENC AV1/SVT-AV1.
  - Do not port its defects: the preview only computes resolution, the audio mix assumes every input has audio, the `p8`/`p9` presets are not universal, and `-y` plus timestamp names can overwrite files.
  - Spatial grid and temporal montage are separate contracts.
- **LoRA training:** `<workspace>/DaSiWa-TrainFlow` (`internal/trainer/profiles.go`, `config.go`, `defaults.go`, `musubi.go`, `manager.go`, `normalizer.go`).
  - Profiles: Anima, SDXL/Pony/Illustrious, LTX 2.3, Wan 2.2, Krea 2.
  - Rust owns datasets, typed config/argv, process groups, progress, cancel/resume and result indexing. Python stays the sd-scripts/Musubi engine, in an isolated environment.
  - The runtime uses cu130 (not the README's CUDA 12.4); ROCm is experimental; Musubi is single-process. Replace whitespace-split extra args with typed argv.
- **Full TVE parity**, beyond §4.2: AI/NVIDIA restoration, RIFE/NVIDIA interpolation and SDR→HDR, each only for tested combinations. The reference stack is CUDA-specific and must not become a universal environment.

Node-pack features and their intended DEAS destination (not yet planned in detail):

| Node-pack feature | DEAS destination |
| --- | --- |
| H3 Cache / Kitchen Attention | Opt-in inference optimization profiles |
| RTX Upscaler & Refiner | Enhancement plugin with an explicit NVIDIA SDK requirement |
| Watermark Overlay | Editor overlay |
| System Monitor | Native telemetry |
| Random String Picker | Prompt/seed services |
| Node Status Switch | Planner stage enable/bypass |
| LLM/VLM Analyze | Analysis/captioning plugin |

A PromptForge port keeps per-model dialects, structured video sections, reference labels, tagger integration and review/apply. It uses DEAS's execution layer rather than its own ComfyUI queue, and licensing is verified first.

### 4.10 Plugin platform and planner

`PluginManifest` holds identity/parent/text/node/include/suggest metadata; `validatePlugins` checks duplicates, missing parents and cycles. A manifest alone cannot install or run an implementation.

- [ ] Extend the smallest contract the first real new plugin needs: schema version, dependencies, capabilities/platforms, permissions, lifecycle, settings/locales, tests.
- [ ] Durable project/asset/recipe/job identities and immutable revisions before claiming restartable multi-pipeline editing.
- [ ] Shared job scheduling and scoped recovery across plugins (today: session-held jobs and serial H3 sequencing).
- [ ] Post-setup feature add/remove/update without resetting projects.
- [ ] A trusted built-in versus untrusted extension boundary before loading downloaded executable/UI plugins.
- [ ] A general editable planner (operation graph, revisions, downstream invalidation, review/pause/resume), only once real consumers need it.

### 4.11 Known defects and audits

- **Forge reference pictures: Ctrl+V paste. Status: planned, not implemented.** Native drops landed (`ReferenceImages.tsx`). Open: paste of clipboard pictures while the Forge dialog is open; `add` already takes `File`s and sends data URIs, so this needs no upload path. Linux WebKitGTK drop acceptance.

- **Checkpoint changes must preserve the current prompts; audit remembered-settings collisions. Status: planned, not implemented.** Owner reports that changing the checkpoint overwrites an already-filled prompt and suspects remembered settings are overwriting each other. A checkpoint selection alone must preserve the current positive and negative prompt text exactly, including intentional empty fields, whitespace and unsaved edits. The suspected cache collision is not yet proven.
  - Source entry points: `src/features/studio/Workbench.tsx::pickModel` calls `switchDraft`, which calls `rememberDraft` and restores a cached draft via `applyDraft` when one exists. `rememberDraft` keys drafts by scope, pipeline and model identity. Trace prompt restoration, cache identity, capture timing and asynchronous effects before changing the cache design; restoring a different model's remembered prompt can be a precedence problem even without a key collision.
  - [ ] Give current prompt edits precedence over per-checkpoint remembered prompts when only the checkpoint changes. Retain compatible remembered model-specific settings and ordinary readiness checks; do not disable all draft memory as a shortcut. Audit scope/pipeline/model separation and stale writes so switching models cannot overwrite another draft's saved values. Keep deliberate prompt replacement from explicit settings/history/media reuse or project loading distinct from checkpoint selection; preserve submitted job snapshots.
  - [ ] Add focused `Workbench.test.tsx` regressions for A→B→A with distinct cached prompts/settings, uncached checkpoints, positive/negative/empty prompts, edits immediately before switching and delayed callbacks. Assert exact prompt preservation on checkpoint-only changes, no cross-scope/pipeline draft contamination, unchanged intended explicit restoration and no generation. Verify Linux/Windows selector interactions and update `docs/STUDIO.md` plus any changed six-language help. Pipeline-navigation prompt policy outside a checkpoint-only change is not redefined by this fix.

- **Queue selection is preview-only: never overwrite settings or prompts. Status: planned, not implemented.** Owner clarification supersedes the earlier request for automatic queue-settings restoration. Clicking anything in the queue must only select the job/output for preview and inspection; it must **not** restore or overwrite authoring settings, positive/negative prompts, pipeline/model, input/reference media, masks/layers or remembered drafts. This applies to every pipeline and media type, not just videos.
  - Source entry point: `src/features/studio/Workbench.tsx::pickJob` selects the job/output but can also call `pickPipeline`, `setEdit`, `rememberDraft` and replace editor media/layers/mask state. Audit these side effects and selection-driven effects/callbacks; separate preview state from authoring state rather than routing plain queue clicks through settings restoration.
  - [ ] Keep selected-job metadata, preview, playback and output selection tied to the clicked job without changing the current authoring draft or its saved cache. Fence delayed preview/metadata callbacks so they cannot apply job settings after selection changes. Queue selection must never submit, cancel or mutate any job.
  - [ ] The queue media right-click/context menu must offer **Reuse the settings** alongside its other applicable actions (including scoped Stop where available), for images and videos. Bind reuse to the right-clicked job/output, not the previously selected row. Only activating that explicit action restores settings; opening the context menu or selecting its row must leave prompts/settings untouched. Reuse the shared restoration path and preserve existing context-menu actions, disabled reasons and keyboard access (context-menu key/Shift+F10).
  - [ ] Restore settings only through a separate explicit **Use these settings / Reuse the settings** action (More/media or queue context menu, or an explicit history-load action). **Reuse prompt** remains a separate explicit action. The requested drop-into-preview settings import remains an intentional import gesture, not equivalent to clicking a queue item; preserve its safeguards below.
  - [ ] Add `Workbench.test.tsx` coverage across supported pipelines/media types, queued/running/terminal jobs, multiple outputs, repeated/rapid selections and late callbacks. Snapshot the current prompts, settings, pipeline, model, input/references, masks/layers and remembered drafts before clicking; assert they remain unchanged while preview/metadata selection updates. Separately assert explicit reuse still restores the selected job's settings and no generation/cancellation occurs. Verify Linux/Windows queue behavior and update `docs/STUDIO.md` plus changed six-language guidance.

- **Reuse settings from media context menu and preview drop. Status: planned, not implemented.** Owner request. Add **Use these settings** to the media right-click menu, and make dropping media into the preview restore as many supported generation settings as the file or its exact correlated job record provides. Apply to images and videos; media without generation settings must remain usable without fabricated values. Reuse the same restoration path as the More menu and history, coordinated with the video-menu and false-warning fixes below.
  - Source integration points: `src/features/studio/Workbench.tsx::mediaActions` supplies the shared media context/toolbar actions; `tools`, `applyFileSettings`, `applySettings` and `restoreHistory` contain the current menu/restore paths. Inspect preview drop handlers, media import and native metadata readers before implementation; do not add a competing settings parser or assume every media format carries generation metadata.
  - [ ] Add the shared context-menu action for the clicked media item, not whichever item was previously selected. On preview drop, resolve the dropped media, read supported metadata/provenance and restore recoverable pipeline/model, prompts, seed, dimensions, sampling and video/component/options values through the shared path. Keep absent values unchanged where compatible; do not carry incompatible state across pipeline/family changes. Clearly summarize restored, unavailable and skipped settings with reasons, without false incompatibility warnings for valid round trips or silent substitutions for missing models.
  - [ ] Preserve the existing preview/source/layer drop behavior as appropriate to the actual drop target; settings reuse must not discard unsaved masks/layers silently, mutate completed jobs/files, download models or start generation. Fence asynchronous metadata reads by the current drop/selection revision so an older result cannot overwrite a later drop or user edits. Keep prompt-only reuse separate and preserve genuine readiness/compatibility checks.
  - [ ] Add context-menu and preview-drop tests for images/videos with complete, partial, absent or malformed metadata; exact job correlation; unavailable models; cross-pipeline restoration; successive drops/late reads; retained source/editor behavior and no submission. Verify native file drops and menu keyboard access on Linux and Windows, update six locales and `docs/STUDIO.md`, and expose the action in global search where applicable.

- **Missing More menu on videos blocks settings reuse. Status: planned, not implemented.** Owner request. Show the **More** control for videos as well as images, with **Use these settings** and **Reuse prompt** where valid generation data is available. Source cause for the missing control: `src/features/studio/Workbench.tsx::tools` renders the `data-more` menu only inside `item.media==='image'`; its reuse actions call `applyFileSettings`, which currently reads `readImageMetadata`. Removing the image-only render guard alone does not establish video metadata support.
  - [ ] Reuse the existing menu and shared settings-restore path; keep image-only post-processing actions image-only. Trace generated-video job/history provenance and embedded video metadata through native readers/parsers before enabling reuse. Prefer the selected video's exact correlated job intent when available; never borrow settings from another selected/recent job or infer generation settings from filename, codec or playback fps.
  - [ ] Ensure the selected generated video's More menu can restore full video settings or prompt only without starting generation, changing the file or losing immutable submitted jobs. For imported videos without supported generation metadata/history, retain a visible menu with an accurate unavailable reason rather than fabricating settings. Coordinate with the false incompatible-settings warning fix below so a valid generated-video round trip actually works.
  - [ ] Add `Workbench.test.tsx` and metadata/history tests for generated videos, imported videos with/without supported metadata, selection changes during asynchronous reads, prompt-only reuse and unchanged image actions. Verify menu visibility, pointer/keyboard access and native-video overlay interaction on Linux/libmpv and Windows/WebView2; update six-language labels/tooltips, global search where applicable and `docs/STUDIO.md`.

- **False incompatible-settings warning when reusing generated image/video settings. Status: planned, not implemented.** Owner reports that reusing settings from a successfully generated image or video displays an incompatible-settings warning even though those settings were valid for its generation. Restore valid settings without a false warning; do not merely suppress validation. A changed backend, missing model/component or genuinely unsupported value must still produce a specific, actionable explanation.
  - Source entry points: `src/features/studio/Workbench.tsx::applyFileSettings` → `generationSettings` → `applySettings` for file metadata, and `restoreHistory` for stored job intents. `applySettings` checks sampler/scheduler against current choices and resolves model/LoRA names, while history restoration changes pipeline, components and options. Trace both paths through metadata parsing, family/component resolution and readiness warnings; the exact failing value and cause are not yet reproduced.
  - [ ] Capture one affected generated image and one video with their original successful intent, embedded metadata/history, selected pipeline/model and backend capabilities. Identify whether the warning comes from lossy metadata, mismatched identifiers, stale defaults/options, restore ordering or a genuinely changed environment; fix the shared cause, not individual warning text.
  - [ ] Round-trip supported settings into the correct image/video pipeline, model family, components and generation/post-processing controls without retaining incompatible state from the previously edited draft. Preserve source media, immutable submitted jobs and the distinction between full settings reuse and prompt-only reuse; never silently replace a model or coerce model-exact values to hide the warning. Missing/unrepresentable metadata must be identified honestly, not treated as a complete successful restore.
  - [ ] Add parser/round-trip and `Workbench.test.tsx` regressions for valid image/video reuse, reuse after switching families/pipelines, prompt-only reuse and genuinely missing/incompatible dependencies. Under the same available models/backend capabilities, a successful job's settings must restore without an incompatibility warning and pass ordinary Run validation. Verify file-metadata and history paths in native Linux/Windows, with actionable six-language warnings and `docs/STUDIO.md` updates if behavior changes; generation remains explicit, never triggered by reuse.

- The Comfy tab's integrated view is sometimes blurry (scale factor / CSS sizing). Not yet investigated.
- **Tiled refine on temporal-latent families.** Gated in DEAS (`temporalLatent`: Anima, Krea 2, H3). Open: one real Anima refine run gated and one tiled SDXL run unchanged. Optional node-pack follow-up (separate authorized task, never patched from DEAS): tile the spatial plane at `T=1` and re-wrap, following `nodes/h3_tiled_sampling.py`; drop the gate only once such a pack version is a declared prerequisite.
- H3 video seams — investigated 2026-11-01, no open DEAS defect. The "known ComfyUI bug, should be fixed" the owner heard of is the MiniMax H3 video-VAE **tile** seam: the H3 VAE always decodes in internal tiles, so every H3 render ran through the buggy blending. Fixed upstream in PR #16436 (`comfy/ldm/minimax/vae.py`, 2026-09-22, "Blend H3 VAE tiles against composited neighbours"), with #15477 (VAEDecodeTiled NestedTensor crash on H3) and #15502 (broken tiled audio decode) before it. The DEAS-owned environment `deas-environment/ComfyUI` tracks upstream master and carried the fix as of HEAD 6d35280 (2026-10-31); DEAS decode paths (plain `VAEDecode`, tiled `VAEDecodeTiled` tile 64/overlap 64/temporal 8, and Continuity `SegmentDecode`) all route through this VAE. Rule: on a seam report, first check the owned backend's environment commit (Backend → ComfyUI update) and restart the owned backend after any update — a running backend keeps the pre-update code loaded. A seam **at a Continuity segment boundary** is the model-level overlap weld (node-pack territory, 22-frame context overlap), not this VAE bug; the upstream fix is spatial blending — temporal overlap handling is unchanged.
- Chat's `read_guide` cuts every guide at 16,000 characters (`forge/assistant.rs`). Most of [STUDIO.md](STUDIO.md) and the Presets/Wildcards sections of [PROMPTING.md](PROMPTING.md) are therefore unreachable from Chat, and MUSIC/TVE/APP-UPDATES are not offered at all.
- `src-tauri/src/output_naming.py::prepare` wraps core/DaSiWa saver methods. This global-patch compatibility exception needs an audit under the [ComfyUI rules](COMFYUI-RULES.md).
- `src/lib/setup.ts` still carries the guided-setup state machine used by core tests and the `existing` plan route; delete it only together with those tests.

## 5. H3 Director and video reference facts

Re-check after node-pack updates.

- **Recipe reference:** the owner's DaSiWa workflows (LTX *OmniForge*, Wan 2.2 *FastFidelity*, MiniMax H3 *MythicAlchemy*; not in this repository) define the wiring. ComfyUI templates and `comfy_extras/nodes_*.py` are only for node signatures. Read subgraph-promoted widget values (e.g. H3 steps 4, shift 10/3, `euler`).
- **MiniMax H3 Director** (DaSiWa node pack: `nodes/nodes_minimax_h3_director*.py`, `js/minimax_h3_director.js`):
  - Modes `T2VA, I2VA, FL2VA, L2VA, REF2VA, Image Inpaint`. `ref2va_model` serves REF2VA and `fl2va_model` the rest. There is **no video-to-video mode**.
  - `timeline_data` items: `{id, type, value, slot, order, enabled, trim_start?, trim_end?, media_mode?}`.
  - Lanes: images 0–8, video 0–2, audio 0–2. Limits: ≤9 images, ≤3 videos, ≤3 audio, ≤12 files. Audio needs an image or video (RefMods count as visual). Clips are 2–15 s; video and audio totals ≤15 s each.
  - Image Inpaint: one image in slot 0, no mask input, a 5-frame pass; frame 0 is the result.
  - All Forge routes return 409 while a workflow is queued.
  - HTTP routes used by the Director: `/upload/image`, `/view?type=input`, `/dasiwa/refmods*`, `/df_h3_continuity/*`, `/dasiwa/h3/forge*`, `/dasiwa/llm/write`, `/dasiwa/input-images`.
- **WhatDreamsCost LTX Director** (GPL-3.0): its guide step equals core `LTXVAddGuide` per segment. Do **not** emit its nodes: they are GPL, its timeline strings come from a 469 KB JS UI, and `/video_ui_custom_view` (arbitrary file read) and `/video_ui_upload_chunk` (path traversal) are unsafe.

Decisions (keep unless the owner changes them):
1. H3 Video → Video and Multi-reference → Video both run **REF2VA**; label it honestly. A REF2VA checkpoint is recommended (a hint, not a block).
2. LTX uses core nodes only (`LoadVideo`, `GetVideoComponents`, `ImageFromBatch`, `LTXVAddGuide`); Wan gets no v2v/ref2v.
3. H3 image edit uses the native node the Director calls, so DEAS masks, crops and layers keep working; frame 0 is the result.
4. DEAS drives node-pack features over their existing HTTP routes. New routes are node-pack work first (route + test + version bump there), never patched from DEAS.
5. Director prompt tokens (`<Picture N>`, `<Video N>`, `<Audio N>`, `<RefMod N>`) stay English in every locale, because they are model input.

## 6. Maintaining this plan

- When a slice lands: record the evidence in [VERIFICATION.md](VERIFICATION.md), the user-facing change in [CHANGELOG.md](CHANGELOG.md)/[NEWS.md](NEWS.md) and its guide, then **remove** the finished item here (or narrow it to what is still open). Do not keep delivered narratives in this file.
- Never promote source inspection, a form field, offline graph validation or an HTTP success to real inference, playback or platform acceptance.
- Gates:
  - frontend: `npm run typecheck`, `npm test`, `npm run build`, `npm run test:layout`;
  - native: `cd src-tauri && cargo test --locked`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`;
  - video graphs: `cargo test --lib dump_video_graphs -- --ignored --nocapture`, then `tools/validate_video_graphs.py <json> --comfy-root <checkout> --extra-model-paths-config <yaml>`. This proves node names, inputs and link types only.
- Documentation-only changes need link/anchor/diff checks, not a version bump.
