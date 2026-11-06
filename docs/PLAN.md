# DEAS plan

[Documentation index](README.md) · [Project rules](PROJECT-RULES.md) · [Product decisions](PRODUCT-DECISIONS.md) · [Verification journal](VERIFICATION.md) · [Changelog](CHANGELOG.md)

This is the **single current plan** for DEAS: what is in place, what is still open and what has to be proven before an item counts as accepted. It replaces the earlier `IMPLEMENTATION-STATUS.md`, `VIDEO-PLAN.md`, `YUE2-MUSIC-PLAN.md`, `PLUGIN-ROADMAP.md`, `NEXT-SESSION.md`, `DRAFT-VIDEO-PLAYBACK-FIX.md` and the old milestone checklist that used this file name; they remain in Git history.

- Delivered work is described in the [changelog](CHANGELOG.md) (per version) and the user guides; evidence lives in the [verification journal](VERIFICATION.md). Finished slices are not repeated here.
- [PROJECT-RULES.md](PROJECT-RULES.md), [COMFYUI-RULES.md](COMFYUI-RULES.md) and [EXECUTION-CONTRACT.md](EXECUTION-CONTRACT.md) govern how work is done; [PRODUCT-DECISIONS.md](PRODUCT-DECISIONS.md) governs scope.
- An open row here is **not** authorization to start it. New integrations and large slices need an explicit owner priority.

Current source: **0.2.17 BETA** (published, tag `v0.2.17`). The label does not close any acceptance gates below. Earlier published baseline: **0.1.66** (tag `v0.1.66`).

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

| Area | State (0.2.17) | Open |
| --- | --- | --- |
| Shell, navigation, layout | Implemented. Creative tabs left: Studio, Library, Prompt Forge, Chat, TVE, Comfy; utility tabs right: Monitoring, Run Plan, Backend, Logs. Shared viewport gate `npm run test:layout` | Windows/native visual acceptance |
| Provisioning (installer) | Partial. Immutable consented plans, Linux/Windows x86_64 CPU/NVIDIA baseline, pinned ComfyUI-GGUF, checked reopening, foreign-install takeover, explicit Upgrade/Reinstall | §4.8 |
| Owned backend lifecycle | Implemented, bounded. Approved ownership, default-on autostart, explicit Stop, Windows Job Object / Linux process group; graceful maintenance stop and previous-state resume | Native Windows graceful-stop/update/move acceptance, real GPU restart matrix, crash/power-loss recovery |
| Downloads and Library | Partial. Catalogs, providers, managed placement, integrity, cancellation, Scan Files | Resume, free-space UX, §4.7 backlog, broad authenticated-provider acceptance |
| Authentication | Partial. Native exact-origin credentials, metadata-only readback | Real OS-store and provider acceptance beyond fixtures |
| Localization | Implemented, bounded. Six languages, module-owned locales | Native-speaker review |
| API negotiation | Partial. Classic/v2 discovery, exact node/model/sampler choices | Complete v2/SDK execution adapter |
| Image generation and families | Partial. Image recipes, LoRA stacks, family constraints, MOSS, SDXL/Qwen/Krea/MOSS image references, paired refine products, GGUF split loaders, Edit meta… family override, wrong-folder gating. 0.2.12–0.2.13 added DualCLIPLoader/Flux, SD3/SD3.5, Chroma/Radiance, HiDream, PixArt/HunyuanDiT, Lumina2, SD1.5/SD2, Kandinsky5, Ideogram4, Boogu, OmniGen2, Fibo2, Ernie, Flux.2/3 and Mage-Flow | Per-family real-model matrix; §4.4, §4.5 |
| Video generation | Partial. LTX 2.3, Wan 2.2, MiniMax H3 (T2VA/I2VA/FL2VA/L2VA/REF2VA, image edit), LTX v2v/keyframes, learned H3/LTX latent upscale | §4.1 |
| Director and Continuity | Partial. Continuity for all H3 pipelines, sessions, checkpoint/imported/uploaded sources, overlap, use-references, Forge Next action, References modal, RefMod reading, packs, people boxes, cast/pose cards | RefMod creation, joined clips, automatic card reading; §4.1 |
| Music (YuE2 and ACE-Step) | YuE2 implemented 0.1.66, fixture-verified: Text to Music (ABC Off/Full/Melody), Cover Music, FLAC results/history (all-in-one checkpoints only). ACE-Step Text to Music for original 1.0 and resolved 1.5 Base/SFT/Turbo/XL implemented 0.2.16, fixture/CPU-schema verified | §4.3; split diffusion-model loading §4.5; real ACE model/native acceptance in F3 |
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
| Plugin platform | Data-only metadata/catalog/theme plugins only; no executable or UI-plugin loading | §4.10 |
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

- **Multi-reference Auto aspect: real-run check.** Source rule ported from the Director's `sourceDimensions` (`firstVisualRef`). Open: one real H3 run with a portrait first picture, and the canvas differs by design from the Director's `auto` resolution preset.

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
- **H3 input scaling default: real-run check.** Default is **Auto**, sent explicitly because an absent field means Fill and crop on older jobs. Open: one throwaway-backend H3 run with an aspect-mismatched reference, Auto against Fill and crop.
- **Playback acceptance.** On Linux, embedded libmpv has replaced the old WebKit/GStreamer path (and its Matroska crash) for all Workbench video. Still open: libmpv runtime acceptance (audible audio, A/V sync, seek/EOF/reopen, resources), WebView2 video on Windows, and WebKit `<audio>` (music/FLAC, audio references) with the bundled GStreamer plugins. Keep `src-tauri/gstreamer/`: the plugins are compiled in via `include_bytes!` and serve `<audio>`.

### 4.2 Video post-processing (TVE) and paired products

Rules: ComfyUI keeps model-dependent work; Rust owns inspection, source retention, recipe compilation and validation. When optional post-processing is enabled the user gets **two real encoded products** (unprocessed baseline and processed result), both selectable/exportable and kept after failure/cancellation; the imported original is preserved. No extra lossy round trip just to save the baseline.

Owner decisions 2026-10-30: (1) the TVE tab compiles the same ComfyUI post recipe the pipelines use (`video-enhance`: LoadVideo → upscale model / `DaSiWa_RTX_UpscalerRefiner` / `DaSiWa_RTX_FrameGeneration` or core RIFE → `DaSiWa_EnhancedVideoCombine`) on the owned backend; (2) the standalone Rust ONNX engine (`video-upscale` feature, `ort`, worker process, output draft/preparation/binding) is retired — it duplicated a strict subset of the backend chain under ≤512 px/≤10 s limits. Shipped in 0.1.67 (see STUDIO.md "TVE post-processing on the backend"): pipeline `videoPost` with B7/7 paired products, all combine codecs/containers/bit depths, model selectors from `upscale_models`/`frame_interpolation`, queue/preview/export through the shared session. 0.2.7 adds the TVE panel's own job polling (a submitted job no longer stays "Queued" while the Studio queue is closed), the RTX `use_mmap`/frames-per-chunk controls and a truncated-safetensors check in the model scan. Open: real GPU acceptance of each stage on the owned backend (RTX nodes need the installed `nvidia_vfx` SDK), HDR, restoration beyond SDK denoise/deblur, baseline-before-processed ordering (PP1 applies to the video savers too), and exact processed geometry in the first/last-frame PNG check (accepted at decoded size).

- [ ] **PP1 strict ordering.** Paired image refine products ship (0.1.51), but sibling savers do not enforce baseline-before-refinement; a failure before the baseline saver runs can leave no baseline.
- [ ] **Paired-video acceptance.** Baseline boundary per chain; both products in results/library/compare/export with six-language role labels; decoded readback (dimensions, frame count/timing, audio, colour/bit depth); measured RAM/VRAM/throughput per platform.

Reference: TrueVideoEnhancer `67ddb2508fdb6da7f1897bbd964f316592f762d0` (`backend/custom_pipeline.py`, `streaming_pipeline.py`, `render_contract.py`, `nvidia_vfx_pipeline.py`) is a behavioural reference only. Its rawvideo path loses source PTS, its host budgets exclude GPU tensors and its final publication lacks a cancellation check. GPL code reuse does not clear model, NVIDIA SDK or FFmpeg terms.

### 4.3 Music (YuE2 and ACE-Step)

YuE2 delivered in 0.1.66; ACE-Step Text to Music delivered in 0.2.16 (see [MUSIC.md](MUSIC.md)). Open:

- [ ] Cover source preflight: actionable no-audio-stream error and decoded-PCM memory bounds for the input (only output FLAC is bounded today).
- [ ] Live-backend SheetSage2 list multiplicity with real weights.
- [ ] External-backend FLAC export: `jobs_outputs_tests.rs::music_flac_outputs_are_decoded_before_preview_and_export` covers fetch/decode/no-clobber export; confirm it runs without `outputRoot`, then close.
- [ ] Real Text to Music (Full/Melody/Off), audio and video covers, scoped cancellation.
- [ ] Native FLAC playback on WebKitGTK and WebView2; Windows runtime.
- [ ] ACE-Step: real-model inference for authorized local original 1.0 and representative 1.5 AIO/split Base/SFT/Turbo/XL weights, plus native Linux/Windows picker, FLAC playback/export and audio post-processing acceptance (§4.5 F3b).

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

- **Sampler coverage and MOSS-Image defaults. Status: partially implemented.** Owner request.
  - **Missing samplers/schedulers.** DEAS must offer the samplers and schedulers the installed backend's `KSampler` enum lists, including **`beta57`** and the **`res_2s`/`res_multistep`** samplers and the **`sgm_uniform`** scheduler. Source facts: the Studio selectors build from the negotiated `choices.samplers`/`choices.schedulers` (`Workbench.tsx`), so the UI list follows the backend; the pinned sampler/scheduler expectations live in `recipe_prerequisites.rs` (currently `euler`/`dpmpp_2m`, `normal`/`karras`) and the per-family defaults in `src/families/builtin.json` — none of these may omit a sampler the recipe can run on, and the graph-compatibility check must accept the samplers above where the graph accepts them. Open: extend the pinned prerequisites and add compiler tests for the listed samplers/schedulers.
  - **MOSS-Image defaults — landed in 0.2.14.** `builtin.json` now uses 50 steps / CFG 3.0 / `euler` / `sgm_uniform` with shift 5.0 baked into the recipe, matching the official recommendations. Dimensions stay multiples of 16 (the family already declares `multiple: 16`).
  - **MOSS Skip-Layer Guidance (SLG) — still open.** DEAS needs a MOSS field **Skip layers** — comma-separated list, default **`22`** — sent as `uncond_skip_layers` on the negative branch (the current bounded adapter omits the template's SLG entirely). Layers 22, 24 and 26 are the effective ones; disabling SLG can yield more realistic textures at the cost of fine detail, so the field must support an empty/disabled state.
  - **MOSS negative prompt — still open.** The Python API default is empty; the recommended general-generation default is `nsfw, worst quality, bad anatomy, watermark, jpeg artifact`.
  - **Turbo/speed variants — still open.** Support the 8-step DMD LoRA (CFG 1, 8 steps) and the 4-step `rapid-moss` adapter (no negative prompt, no SLG) as labelled MOSS variants with matching defaults.
  - [x] Update `builtin.json` defaults + `defaultsNote` — landed in 0.2.14. Open: add the SLG field to the MOSS adapter path (TS compiler + `jobs.rs` preflight + prerequisites), the recommended negative prompt and turbo/speed variants, six locales, `docs/STUDIO.md` family table; one real MOSS run per default set (base 50/3.0/euler/sgm_uniform with SLG 22, turbo 8-step) recorded in `docs/VERIFICATION.md`.
  - [ ] **Verify every family default against real suggested defaults.** `docs/MODEL_DEFAULTS.md` keeps a researched matrix (current `builtin.json` defaults vs official model-card / Comfy-Org-template recommendations, per row with source + confidence). 0.2.14 realigned fourteen families; residual: video/audio families outside the audited set in `MODEL_DEFAULTS.md` still need re-checking and any contradicted default fixed.

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
- **All ComfyUI core architectures (planned, owner request).** DEAS registers 41 family IDs (including ACE-Step variants) and detects the families in `builtin.json`. ComfyUI core at `b5a43215` (2026-10-29) ships ~55 distinct architectures (108 `ModelSupport` classes counting variants) and `blueprints/*.json` is the authoritative per-architecture graph. SwarmUI adds no architecture of its own: it is a ComfyUI frontend, and `T2IModelClassSorter.cs` narrows detection to 25 compat classes, so it is only a header-signature source. Scope: every architecture ComfyUI can load from the folders DEAS already scans (`settings.rs::MODEL_FOLDERS`, 16 folders incl. `text_encoders`, `clip`, `clip_vision`, `style_models`). Owner example: Z-Image.

  **Gap by cause, not by model.** The image recipe is one fixed graph (`execution.ts::compileRecipe`: `UNETLoader`/`CLIPLoader`/`VAELoader`/`CLIPTextEncode`/`EmptyLatentImage`/`KSampler`) with two hardcoded family branches (MOSS `EmptyFlux2LatentImage`, GGUF) and one text encoder. Nearly every missing architecture differs from it in only four declarative points — text-encoder loader form (single vs `DualCLIPLoader`), latent node, model-sampling patch, negative handling — plus defaults. Make those family data: then most new image architectures are a `builtin.json` entry + header rule, not a compiler branch.

  **F0 — declarative adapter (one slice, no new model).**
  - `src/families/builtin.json` gains `clip: {loader: "single"|"dual", encoders: [patterns]}` (dual needs two slots), `latent: {node, …}`, `patch: {node, shift}`, `negative: "text"|"zero"`; `defaults` and `multiple` already exist. **Landed in 0.2.12: `clip` dual loader (`diffusion.textEncoder2` + `dualTextEncoders`/`dualClipTypes` prerequisites + second encoder picker), Flux.1.** **Landed in 0.2.13: `modelShapes` (Z-Image 3840 vs Lumina2 2304) and the top-level `family` id for checkpoint families (Lumina2 latent + AuraFlow 4).** Mirror the fields in the Rust `Family` struct (`model_families.rs`) and the TS `Family` type (`Workbench.tsx`); use only existing ComfyUI core nodes, no new loader kinds.
  - Family detection gains `modelShapes: [{key, shape}]`, because several architectures share the same key set: Z-Image vs Lumina2 vs Ming-Image (`cap_embedder` + `noise_refiner` + `dim` 3840 vs 2304), Flux 2 vs Flux 3, Chroma vs Chroma-Radiance, HiDream vs HiDream-O1, LTX 2.3 vs 2.5, Wan 2.1 vs 2.2. `detect_component` already has the shape helper — reuse it. A key-only rule that misclassifies is a defect: wrong family means wrong latent, grid and defaults.
  - Dual encoders: the intent gains a second encoder slot; `jobs.rs` preflight and `recipe_prerequisites.rs` gain non-GGUF `dualTextEncoders`/`dualClipTypes` (the GGUF pairs already exist); `execution.ts` uses the existing unused `clipLoaderName(name, dual)` parameter; Workbench picker, capability allowlist, six locales.
  - Acceptance: one unit test per new field asserting the exact graph JSON, one real Z-Image run, `cargo test --lib model_families`, `src/lib/modelComponents.test.tsx`.

  **F0b — pipeline classes instead of per-family compilers.** Graph knowledge lives twice today: `execution.ts::compileRecipe` (image) and `jobs.rs` preflight + `recipe_prerequisites.rs` re-derive the same model/encoder/VAE/node facts; video is one Rust compiler with 25 `family ==` sites plus the TS discriminated union (`videoIntent.ts`); music is a YuE2-only graph. Adding one family therefore means editing a compiler in two languages, the union, and the UI — that is what must stop.
  - Pipeline **classes** become data, one entry per class with one graph template: `t2i`, `i2i`, `inpaint`, `edit` (reference conditioning), `refine`, `t2v`, `i2v`, `v2v`, `flf2v`, `t2m`, `cover`, `t2a`, `mask`, `depth`, `pose`, `3d`.
  - A family declares the classes it supports plus slot values (encoder(s), clip type, VAE, latent, patch, negative, frame grid/fps/audio rate, sampler defaults, option groups). The compiler becomes one template interpreter addressing nodes by role (`model`, `clip`, `vae`, `positive`, `negative`, `latent`, `sampler`, `decode`, `save`, …) instead of fixed IDs.
  - Structural differences are **templates**, never branches: Mage-Flow `TextEncodeMageFlowEdit`, Cascade B+C two-stage, H3 audio/video latent + continuity, LTX two-pass audio/video samplers, Wan high/low-noise pair. A family selects class + template + slots; no `if family == …` in the compiler.
  - **Base graph per class, complex parts as declared fragments (injections).** A class owns one base graph whose nodes are addressed by role slots (`model`, `clip`, `vae`, `positive`, `negative`, `latent`, `sampler`, `decode`, `save`). Every complex case is a named fragment declaring `insert` (new nodes with namespaced ids), `rewire` (claim an exclusive role, e.g. `sampler ← SamplerCustomAdvanced`, `decode ← VAEDecodeTiled`), `chain` (append to an additive role, e.g. LoRA → attention/sage/cache/compile/chunk patches), `requires` (nodes/models, so prerequisites derive instead of being restated) and `when` (family/intent/option predicate). A family lists class + fragment ids + slots; ControlNet, refiner, second pass, continuity and reference conditioning are fragments reused across families, not per-family code.
  - **Composition rules:** fixed order model-chain → conditioning → latent → sampler → decode → save → post; two fragments claiming an exclusive role is an error (no last-wins); additive chains compose in declared order; fragments never renumber nodes or hardcode ids; every rewire must resolve or the compile fails naming the slot; a fragment whose nodes are missing from `object_info` fails the Run gate by name.
  - The codebase already composes this way in miniature — the image `patches` array, the `sigmas` swap of node `5`, the `imageReference` CV/RI/CE/UC chain, the tiled-decode swap, and video `model_chain`/`decode`/`input_image`. F0b extracts those into the fragment registry rather than inventing a second mechanism.
  - Tests: fragment test (base + one fragment → exact graph), composition snapshot per family × class, TS↔Rust golden parity; adding a capability fragment must not touch any family compiler.
  - One writer: family data feeds the TS graph, the Rust preflight and the `recipe_prerequisites` node list; a parity test compiles the same family in both languages and asserts identical graph and prerequisites. A new family is a data entry + header rule + fixture — touching `compileRecipe`/`jobs_video.rs` means the slice failed.
  - Migration order: t2i (covers F1) → i2i/inpaint/edit (the existing MOSS/Krea/Qwen reference methods become templates) → video (LTX/Wan/H3 as the first three templates; their `family ==` sites disappear) → music. Existing families must emit byte-identical graphs: golden-graph tests prove the migration, not a new model run.
  - Capabilities stay negotiated: templates name only nodes/loaders the backend lists, and a missing node fails with the named Run gate instead of a silent fallback. Unknown architectures keep running the generic t2i template on grid 16.
  - Ties to M-02 above: a recipe profile binds an unknown model to class + template + slots; it adds no compiler code.

  **F1 — image families (t2i/i2i), data-only once F0/F0b land.** Values come from the blueprint, never guessed; a family with no blueprint (`Flux 3`) is verified from `supported_models.py`/`latent_formats.py` and labelled unverified until a real run.

  | family | loader/clip | vae | latent | patch | defaults |
  | --- | --- | --- | --- | --- | --- |
  | Z-Image Turbo / Base | `lumina2` + `qwen_3_4b` | `ae` | `EmptySD3LatentImage` | `ModelSamplingAuraFlow` 3 | turbo 8/1 `res_multistep` `simple` (landed); base 25/4 with encoded negative (open) |
  | Flux.1 dev/schnell/krea/fill | `DualCLIPLoader` clip_l + t5xxl `flux` | `ae` | `EmptySD3LatentImage` | — | 20/1 `euler` `simple`; dev/schnell/krea landed, Fill = inpaint path (open) |
  | Flux.2 dev / Klein | mistral-3-small / `qwen_3_4b` `flux2` | `flux2-vae` | `EmptyFlux2LatentImage` | `Flux2Scheduler` (sigmaNode) | landed: 20/5 euler; edit refs open |
  | Flux.3 | `qwen3vl_4b` `flux3` | flux2 | `EmptyFlux2LatentImage` | `Flux2Scheduler` (sigmaNode) | landed, unverified (no blueprint) |
  | SD3 / 3.5 large+medium | dual t5xxl + clip_l/clip_g `sd3` | `ae` | `EmptySD3LatentImage` | `ModelSamplingSD3` 3 | landed: 28/4.5 `dpmpp_2m` `karras` as starting values |
  | Lumina2 / NewBie / NetaYume | all-in-one checkpoint, encoder embedded | — | `EmptySD3LatentImage` | `ModelSamplingAuraFlow` 4 | landed (modelShapes 2304 vs Z-Image 3840) |
  | Chroma / Chroma-Radiance | chroma | pixel-space | `EmptyChromaRadianceLatentImage` | — | landed: Chroma = Flux latent, Radiance = pixel latent, family id disambiguates |
  | HiDream / HiDream-O1 | hidream dual llama3 + t5 | `ae` | `EmptySD3LatentImage` / `EmptyHiDreamO1LatentImage` | — | landed: I1 Full/Dev/Fast (no patch); O1 open |
  | Mage-Flow (+edit) | `mage` + `qwen3vl_4b` | `mage_flow_vae` | `TextEncodeMageFlowEdit` carries w/h (not data-only) | — | landed: t2i via `encodeNode` 30/5 euler; edit refs open |
  | Qwen-Image 2512 / Layered / edit refs | `qwen_image` + qwen2.5-vl | `qwen_image_vae` / layered vae | `EmptySD3LatentImage` / `EmptyQwenImageLayeredLatentImage` | `ModelSamplingAuraFlow` | per blueprint |
  | Ideogram v4 (landed), Boogu (+edit) (landed), OmniGen2 (landed), Fibo 2.0 (landed), Ernie (+turbo) (landed), LongCat (+edit), Joy Image edit, SenseNova U1.5, Ming-Image, Lens, AuraFlow | per blueprint | per blueprint | per blueprint | per blueprint | per blueprint |
  | PixelDiT/PiD T2I | `pixeldit` + gemma-2 | pixel-space | `EmptyChromaRadianceLatentImage` | — | landed, unverified (VAE name) |
  | Kandinsky5-Image Lite/Pro | `kandinsky5_image` + qwen2.5-vl-7b | `ae` | `EmptySD3LatentImage` | — | landed (shift internal) |
  | PixArt Alpha/Sigma, HunyuanDiT 1.0/1.5 | `pixart` single t5 / `hunyuan_dit` dual clip_l+mt5xl | SD15/SDXL | `EmptyLatentImage` | — | landed (grid 8, no patch) |
  | SD1.5 / SD2 (landed) / SDXL Refiner / SSD-1B / KOALA / instruct-pix2pix / SVD / SV3D / Zero123 / LotusD | checkpoint families | built-in | `EmptyLatentImage` | — | SD1.5/SD2 landed (grid 8); rest per blueprint |
  | Stable Cascade B + C | two-stage checkpoint pair | built-in | cascade latents | — | not data-only |

  Order: Z-Image → Flux.1/2 → SD3.5 → Lumina2 → Chroma/HiDream → Mage/Boogu/Ideogram/Ernie/LongCat → the remainder. Z-Image first because it is the owner's example and the newest split form; Flux/SD3.5 because they are the most-installed.

  **F2 — video families.** Extend the existing `wan` family first (`jobs_video.rs`, `videoIntent.ts` discriminated union): t2v high/low-noise pair, VACE FLF2V + inpaint, Animate 2 motion transfer, SCAIL-2 character replacement, Wan Dancer, S2V, HuMo, Camera, Fun-Control, Bernini-R. Then new families: HunyuanVideo (+I2V/SkyReels, 1.5, Image 2.1 + refiner, SR distilled), LTX 2.5 (Gemma4 encoder, int8), CogVideoX (+VOID inpaint), Mochi, Cosmos, Kandinsky5 video. Each new family is data (class + template + slots + frame grid/fps in the family entry); a template is added only where the graph shape is genuinely new. SeedVR2 video upscale is a post-processing node, not a generation family.

  **F3 — audio.** Stable Audio 3 (medium/base), MiniMax Music 3. The YuE2 split-form item above is folded in, not duplicated. ACE-Step is the owner's explicitly prioritized next music integration, split below rather than enabled by a picker-only change.

  **F3a — ACE-Step identity: native acceptance remaining.** Header/shape architecture classification, conservative filename-based variants, unresolved/conflicting evidence, six-locale Edit meta evidence and source-bound family declaration/reset are implemented and covered by native/unit/Chromium fixtures; evidence belongs in `VERIFICATION.md`. Remaining acceptance before executable integration is called supported:

  - [ ] Exercise model listing, declaration/reset and filename/header evidence in the real Linux WebKitGTK and Windows WebView2 app with an operator-selected local model. The inspected upstream headers do not encode Base/SFT/Turbo; a filename result must remain visibly weaker than header identity. No implicit model downloads or generation.

  **F3b — ACE-Step Text to Music: real-model/native acceptance remaining.** The implementation and source-linked defaults now live in the existing Text to Music path; implementation details and source/gate evidence belong in `MUSIC.md`, `MODEL_DEFAULTS.md` and `VERIFICATION.md`. No new required variant selector or separate execution transport. Outstanding acceptance, not missing UI/compiler wiring:

  - [ ] With explicitly authorized operator-selected local weights, run original 1.0 and representative 1.5 AIO/split Base/SFT/Turbo/XL combinations. Record model identity/provenance, exact submitted controls, initialization/peak RAM/VRAM/time, generated FLAC sample rate/sample count/duration and output quality. Schema validation with scratch filename fixtures does not prove model loading or inference.
  - [ ] Exercise the real Linux WebKitGTK and Windows WebView2 model/component pickers, native FLAC playback/export, history/settings restoration and enabled automatic audio post-processing; verify retained baseline/processed products without restarting foreign services. Windows crosschecks are not runtime acceptance.
  - [ ] Additional shift/continuous distillations remain excluded until their exact official recipe and defaults are verified. Keep unresolved/conflicting identity blocked, never silently select Turbo; preserve user overrides and authored Style/Lyrics during any future profile migration.

  **F3c — ACE-Step audio-to-audio/Cover/Repaint. Status: deferred, not implemented.** Not part of the initial Text to Music implementation. Original audio editing and newer 1.5 reference-timbre paths need separate graph/intent acceptance; Comfy 1.5 documentation and newer core expose different maturity levels. Do not advertise full Cover/Repaint or manufacturer-only tasks from node presence alone.


  **F4 — perception/3D (lowest priority).** SAM 3.1 masking (§4.4), RT-DETR v4 pose, Depth Anything 3, MoGe, Marigold, Lotus depth, Hunyuan3D 2/2.1, Trellis2, TripoSplat, BiRefNet. These need their own pipelines, not t2i, and only after F0–F2.

  **Model folders.** `MODEL_FOLDERS` already covers every folder these families load from, so no scan change is needed. Close while F1 lands: `style_models`/`model_patches`/`diffusers` are scanned but no recipe uses them (Flux Redux stays in §4.4), and the loader gate must keep checkpoint families out of `diffusion_models` and vice versa (`model_families.rs::validate_override` already enforces this). Library download catalog gets entries per family only where an official Comfy-Org/HF repo exists — not one bulk import.

  **Acceptance, uniform per family.** (1) header fixture from the real file names the family, not a neighbour (`model_families` test with the exact key set/shape); (2) compiler test asserting the exact graph: loader form, latent node, patch, negative handling, defaults; (3) golden-graph parity test (TS ↔ Rust) and offline validation against the blueprint; (4) one real GPU run in VERIFICATION.md with VRAM/time; (5) family and Forge label in six locales plus the `docs/STUDIO.md` family table; (6) version bump + CHANGELOG bullet. A family counts as supported only after (4); before that it is *detected*, not supported.

  **Explicit exclusions.** No ControlNet/LoRA variants per architecture (Z-Image ControlNet, Anima LLLite), no SwarmUI-only classes, no wizard, no 3D/audio UI surface until asked. Unknown checkpoints keep the strict grid-16 t2i/i2i path.

### 4.6 Studio UX

- **Pipeline-filtered model picker — native acceptance.** The compact selector now reuses the existing compatibility gate to list only candidates for the active pipeline; an unavailable remembered selection stays visibly disabled with its translated reason, without replacement. Component tests cover mixed/empty inventories, image/video changes, drafts, prompt preservation and selection events. Open: native Linux WebKitGTK and Windows WebView2 popup/keyboard behavior; filtering does not replace native Run validation.

- **Media panel: clipboard drop/paste intake field. Status: planned, not implemented.** Owner request. Add a small field left of the "Drop files or folders here, or browse. Nothing is copied or moved" hint (`media.dropHelp`, `src/features/media/MediaLibrary.tsx` header, `.media-drop-hint`) that accepts a dropped image/video **and a Ctrl+V clipboard paste** and automatically brings the media into the input library for further usage. OS file drops already work via the native pattern (`onDragDropEvent` + `describeMedia(paths)`, `MediaLibrary.tsx` ~120); clipboard content has **no path**, so paste needs the byte-ingest native command already planned for W2b′ (sniffed, size-capped, written under the input root, never overwriting — timestamped unique names) — no parallel upload path.
  - [ ] One narrow drop/paste field on the hint row, fixed width, rows aligned (panel layout gate if the header reflows); accepts `image/*`/`video/*`, ignores folders/text/other files with a one-line status, never an alert. Paste active only while the media panel is visible, fenced by open-dialog/modal ownership (reuse MediaLibrary's modal-ownership pattern so a Forge dialog keeps its own drops/pastes).
  - [ ] After ingest, refresh the input listing and select the new entry so it is immediately usable as the authoring media; originals untouched, "Nothing is copied or moved" remains true of the folder browser — the new field is an explicit copy-in gesture and must be labeled as such.
  - [ ] `MediaLibrary.test.tsx` coverage: mocked native drop + paste intake for image/video, non-media ignored, unique-name no-overwrite, refresh+select, paste fencing behind modals, unmount cleanup. Native Linux WebKitGTK + Windows WebView2 paste acceptance (`navigator.clipboard.read()` support differs per webview — degrade with an accurate reason where absent), six-locale labels/tooltips, global-search entry and `docs/STUDIO.md`.

- **Ctrl+F: find in the current view. Status: planned, not implemented.** Owner request. Open a browser-like find bar over the current view with Ctrl+F; entering a search term highlights matching text in that view. Keep this separate from Ctrl+K global control/navigation search.
  - [ ] Reuse existing search/highlight mechanisms where suitable after inspecting current view containers and keyboard handlers. Search only the active visible view, excluding retained hidden tabs and closed overlays; do not search the whole application or change prompt contents. Highlight matches, show current/total count and offer previous/next navigation (Enter/Shift+Enter); Escape closes the bar, clears highlights and restores focus. Reopening retains the current query for the session. Empty queries and no matches have clear states.
  - [ ] Preserve editable-field text/selection, IME input, existing shortcuts and mounted-view/job lifecycles; highlighting must not rewrite controlled textarea values or disrupt native media. Check the embedded Comfy view separately: cross-origin iframe content needs its own supported find path, not a claim that parent-DOM search covers it. Report any unsupported surface explicitly.
  - [ ] Add focused UI tests for Ctrl+F opening, matching/highlighting, next/previous wrapping, empty/no matches, Escape/focus return, active-view changes and hidden-view exclusion. Verify editable prompt fields and embedded Comfy behavior on Linux WebKitGTK and Windows WebView2. Add accessible labels, six-language text, a global-search entry and `docs/STUDIO.md` shortcut documentation; run the shared layout gate if the bar changes layout.

- **Ctrl+Enter runs: native acceptance.** Landed with component tests. Open: Linux WebKitGTK keyboard check; held keys and IME composition on a real keyboard.
- **Prompt Undo/Redo: WebKitGTK check.** Tag and spelling picks now go through `execCommand('insertText')`. Open: Ctrl+Z and Ctrl+Y on Linux WebKitGTK, and the enlarged editor.
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
  - [ ] Beginner alignment: wizard definitions carry the same pipeline/aspect/resolution/settings keys the Beginner composer writes (see the Beginner mode item), so a wizard can later surface inside Beginner Generate without a new execution path.
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

- **Theme authoring — remaining native acceptance.** DEAS bundles one canonical `EDIT-THEMES.html` and one `new-theme` starter; it does not duplicate the nine built-in schemes as external packs. **Quick Settings → Edit themes** replaces **Open themes folder**: it creates the environment's `themes/` directory, copies `new-theme` only when absent, and opens the offline editor in the system browser. The old command is removed. Exercise actual Linux GTK/WebKitGTK and Windows WebView2 behavior for editor opening, real Chrome/Edge folder permission, no-clobber starter creation, theme save/refresh, duplicate ID rejection and reduced-motion/full-scheme selection. Fixture/browser evidence does not close these gates. [Guide](THEME-PACKS.md).

- **Beginner mode (landed in 0.2.15, acceptance open).** Manual port of the `DEAS-0.2.13-Beginner-SHARE.zip` overlay (archive SHA-256 `b4adae309300e95224bf3ad5ecdf5c4d34162014b28a0d28117a23bf09c3f900`): one `interface.experience` preference, page band Generate/Models/Media, Engine button to the unchanged setup, per-mode settings isolation and model-bound Beginner LoRA draft. Deliberately not adopted: the fork's window-title rebranding, Sage as fresh-install theme fallback (the built-in Sage scheme already provides Sage), and the fork's hide-GlobalSearch choice (every control must stay reachable from Ctrl+K). Open gates:
  - [ ] Native acceptance on WebKitGTK and WebView2: welcome dialog first run, mode switch during a running job keeps the job, Engine flow with no installed environment, six locales at narrow widths.
  - [x] Beginner must-have settings gap closed in 0.2.15: the composer's **Format**/**Quality** selects reuse the centralized `aspects`/`resolutions` option sets, the shared `studio.run.aspect`/`studio.run.resolution` preferences and the advanced panel's exact label keys — advanced-only `fixed`/`custom` (own size fields) stay out; sizing is treated as shared authored intent (kept out of the mode-isolation snapshot, persisted by the Beginner intent effect); seed and negative prompts remain advanced. Acceptance: real-run size check below.
  - [ ] Wizard alignment (see Wizards tab): wizard definitions already carry pipeline plus the same resolution/aspect/settings keys the Beginner Format/Quality pair writes; a future wizard run must be presentable inside Beginner Generate without a new execution path. Record that contract when the first wizard definition lands.
  - [ ] One real Beginner run per supported pipeline (t2i/i2i/t2v/i2v) on real hardware; native download of one featured guide model through the existing plan/confirm dialog.
`PluginManifest` holds identity/parent/text/node/include/suggest metadata; `validatePlugins` checks duplicates, missing parents and cycles. A manifest alone cannot install or run an implementation.

- [ ] Extend the smallest contract the first real new plugin needs: schema version, dependencies, capabilities/platforms, permissions, lifecycle, settings/locales, tests.
- [ ] Durable project/asset/recipe/job identities and immutable revisions before claiming restartable multi-pipeline editing.
- [ ] Shared job scheduling and scoped recovery across plugins (today: session-held jobs and serial H3 sequencing).
- [ ] Post-setup feature add/remove/update without resetting projects.
- [ ] A trusted built-in versus untrusted extension boundary before loading downloaded executable/UI plugins.
- [ ] A general editable planner (operation graph, revisions, downstream invalidation, review/pause/resume), only once real consumers need it.

### 4.11 Known defects and audits

- **SwarmUI model folders/junctions — bounded implementation, remaining acceptance.** Category aliases, native metadata save/reset/readback and resolved category-target registration are implemented; Windows filesystem, actual ComfyUI Python configuration and real WebView2 persistence evidence are in [verification](VERIFICATION.md#swarmui-model-folders-and-pipeline-picker-0214-shipped-in-0214). Roots still contain category folders; a directly selected `Krea2`/category subfolder is not a mapping. No model bytes, SwarmUI settings or OS trust policies are changed.
  - [ ] **Nested untrusted Windows junctions remain a backend limitation.** Native headers/sidecars are readable, but ComfyUI’s recursive traversal/mtime lookup can raise OS448 and fail the entire category list. Resolving a category root does not preserve/bypass a nested alias prefix. Keep exact backend catalog and Run gates; do not flatten model names, claim these files runnable or patch ComfyUI/security policy without separately authorized work.
  - [ ] **Original reported files and interactive acceptance.** The reporter’s exact Krea2/MOSS/Z-image headers/loading forms and Windows paths are still needed for a matched reproduction and real generation check. Preserve the distinction between discovery, declared family and actual checkpoint/split loader support. Native Linux/Windows metadata-editor click-through and checkpoint-popup keyboard behavior remain separate from the completed native IPC save/restart/read/reset tests.

Implementation and audit evidence for the completed portions is in [verification](VERIFICATION.md#known-defects-and-audits-0214-shipped-in-0214). Only remaining work is listed here.

- **Forge clipboard/drop — native acceptance.** Exercise real OS picture clipboard paste and native file drops in the open Forge editor on Linux WebKitGTK and Windows WebView2, including nested dialogs, ordinary text paste, image caps and errors. Component fixtures do not close this gate.
- **Remembered settings — Linux selector acceptance.** Exercise checkpoint-only switches through the actual native selector: preserve positive/negative prompts exactly, including empty fields, while restoring the target's other draft settings. No key-collision defect is established. The GTK fixture rendered readable selectors but its separate warning-layout assertion failed; investigate that renderer-specific observation rather than weakening the gate.
- **Queue/media settings reuse — native and provenance acceptance.** Verify queue/context/toolbar keyboard actions and OS preview drops in native Linux/Windows, including retained editor masks/layers, cancellation of replacement, exact clicked-file ownership and no generation. Matching is currently a unique held-job output path in its captured root, not a media-content digest; files replaced externally or no longer correlated with a held job cannot be claimed to carry that job's exact provenance. Persisted recent history remains an explicit separate restore path.
- **File settings coverage.** Embedded PNG/JPEG/WebP and MP4/Matroska/WebM metadata can recover supported literal prompts/model/sampling/dimensions and partial video timing, not the complete DEAS component/options/post-processing/input intent. No full validated intent is embedded by the current saver contract. Keep partial restores explicit and missing/ambiguous models blocked. Remaining: recover additional supported component/operation fields from correlated graph wiring where unambiguous; coordinate any complete intent-embedding requirement with the actual saver API (node-pack changes require separate authorization). Do not invent an intent from node presence, a filename or a shared basename.
- **Owner-reported incompatible-settings warning — concrete samples still needed.** Reproduced fixes cover declared LoRA-family aliases, LoRAs resolved against the restored rather than previous family, lossy display metadata, stale explicit sigma schedules, explicit empty adapter lists and ambiguous exact model names. The original affected generated image and video plus their successful intents/backend capabilities have not been supplied. Capture those exact cases, compare file versus history restore under unchanged capabilities, and retain real missing-dependency/model-exact validation. Native playback/overlay and Linux/Windows round-trip acceptance remain open.
- **Comfy view sharpness.** Not reproduced. The retained iframe uses ordinary flex sizing and no local scale transform establishes the cause. Obtain the affected screenshot, display scale/DPR and reproducible steps; compare the original editor and embedded view on the same endpoint in actual WebKitGTK, including canvas backing dimensions and webview zoom. Do not replace the tab or add speculative scaling CSS.
- **Temporal-latent tiled refine — real-model acceptance.** Direct native intents and the TS preview now reject tiled refine through the existing family flag; frontend/history gates remain. Still run one real Anima case with the gate and one unchanged tiled SDXL case. No checkpoint/diffusion weights are present in the inspected owned environment's standard model folders. Optional spatial-at-T=1 support is separate node-pack work and needs explicit authorization plus a declared compatible prerequisite before removing the gate.
- **Saver compatibility exception — platform/lifecycle limits.** The owned-backend adapter still installs process-wide method wrappers after a marked submission; unmarked saves retain standalone behavior, and no safe public per-job teardown hook has been established. Failure rollback, saver capability checks, idempotence and actual Linux CPU savers are tested. Remaining: Windows native publication/failure acceptance (configured test host is currently unreachable: SSH `No route to host`) and broader coexistence with third-party wrappers; retain the narrow DEAS opt-in and do not extend this exception to unrelated backends.

### 4.12 Audio enhance across audio pipelines

- **Audio enhancement for Studio post-processing and TVE (0.2.15, shipped in 0.2.17).** Clean processing, paired baseline/processed publication and native automatic post-generation chaining exist; the user selected Smule Labs Windowed RoFormer instead of Demucs for Stems. Deep uses native DeepFilterNet. All modes are opt-in OFF initially, content-aware alternatives rather than a quality ladder. See [Audio enhancement](AUDIO-ENHANCEMENT.md).
  - Native automatic chaining retains immutable `audioEnhance: {mode, profile}` in music/video/TVE intents and watches backend completion independently of mounted frontend panes. Jobs remain running until post-processing settles. Local cancellation does not interrupt the backend after generation; silent video is a true no-op. Baselines and valid partial products remain accessible after failure/cancellation. Real-FFmpeg lifecycle checks are explicit ignored acceptance tests, separate from default dependency-free unit gates.
  - TVE supports audio-only processing from its retained inspected source without backend/video stages and automatically chained audio after selected video enhancements. Explicit new-copy export remains available on terminal selected products. Native source identity, stream mapping, sample-rate/channel/duration checks, no-clobber publication and central Logs apply to both paths.
  - **Stems selection:** [Smule Labs Windowed RoFormer](https://github.com/smulelabs/windowed-roformer), original HF revision `d9642ed592120dca935ebb783c77f2faa1e6c32b`, MIT code and checkpoint grant. This is vocal/instrumental separation, not four-stem drums/bass/other extraction. Demucs is not the selected shipping backend. No automatic music-to-speech-model routing or silent weaker-mode substitution.
  - Smule optional installation and actual CPU inference are exercised with pinned upstream sources/checkpoint, hash-locked isolated dependencies, explicit consent and an inference readiness check. Rust performs overlapping 8-second windows at 4-second stride, exact-length complementary assembly and instrumental subtraction. The upstream CLI is unmodified. The initial CPU path reloads the model per window and needs a compatible C++ compiler.
  - Stems device selection `auto | cpu | cuda` landed with read-only reuse of the owned environment's CUDA Torch, an ABI-checked binding published only after a real CUDA inference window, and RAM/VRAM admission that releases only owned queue-idle backend caches (re-measured, never trusted from the ack). A shared compute gate serializes admissions with backend submissions.
  - [ ] Real end-to-end CUDA inference acceptance (`gpu_provision_repair_real`, `smule_real_short_cuda` ignored tests) on an idle GPU: on the current host the GPU is held by an unrelated live LLM engine and the owned backend was not running, so the binding remains unpublished and the UI truthfully CPU-only until this runs.
  - [ ] Perceptual Deep/Smule quality and A/V-sync acceptance on representative generated speech/music and real TVE clips. Current real model/codec tests establish execution, exact sample lengths, unchanged video packets and source/no-clobber/cancel behavior, not subjective enhancement quality. Smule CPU tests measured roughly 73 s for 1 s input and 115 s for 9 s input on a 7800X3D; GPU speed remains unmeasured until the acceptance above runs.
  - [ ] Actual Windows runtime/installer/playback acceptance; configured test host is currently unreachable (SSH timeout). Cross-target compilation is not execution evidence.
  - [ ] A separate ComfyUI graph node, only if separately authorized in its owning project; preserve standalone consumers.

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
