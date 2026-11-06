# Music — Text to Music with YuE2 and ACE-Step

[Documentation index](README.md) · [Studio](STUDIO.md) · [Open work](PLAN.md#43-music-yue2)

Studio → Generate offers **Music (Text to Music)** for YuE2 and resolved ACE-Step models, plus **Music (Cover Music)** for YuE2. Recipes compile natively to ComfyUI core nodes; no custom node pack is required for generation.

## ACE-Step Text to Music

Select **Music (Text to Music)** and an ACE-Step model in the existing model picker. There is no second required variant selector. Header/shape detection distinguishes original 1.0, 1.5 and XL, including the observed `ace-step-1_5` export alias when matching 1.5 tensor structure is present. Base/SFT/Turbo share tensor structures, so an explicit architecture declaration or a conservative filename fallback identifies the training variant. Unknown/conflicting identities stay blocked; **Edit meta…** shows header versus filename evidence and lets you declare the exact family or reset to automatic detection. A declaration does not upgrade filename evidence to header confirmation.

Original 1.0 uses an all-in-one checkpoint under `checkpoints`, its AceT5/MusicDCAE path, lyrics strength and its original sampling/tonemap nodes. ACE-Step 1.5 accepts an all-in-one checkpoint or split weights under `diffusion_models`/`unet`. Split models expose the existing Components controls: `ace_1.5_vae.safetensors`, `qwen_0.6b_ace15.safetensors`, and the code LM (`qwen_1.7b_ace15.safetensors` by default for regular models, `qwen_4b_ace15.safetensors` for XL). XL describes the diffusion model, not the code-LM size. Native preparation checks that local AIO checkpoints actually contain the embedded VAE/text-encoder namespaces and refuses bare split weights misplaced under `checkpoints`. All split components must be listed by the backend and the `ace` dual-encoder loader type must be available; missing actual graph nodes/components block Run. No download starts implicitly.

| Variant | Steps | Diffusion CFG | Shift |
| --- | --- | --- | --- |
| Original 1.0 | 50 | 5 | 5 |
| 1.5 Base / SFT | 50 | 7 | 1 |
| 1.5 Turbo | 8 | 1 | 3 |
| 1.5 XL Base | 50 | 6 | 3 |
| 1.5 XL SFT | 50 | 7 | 3 |
| 1.5 XL Turbo | 8 | 1 | 3 |

All use `euler` / `simple` as starting values. Regular Base/SFT's 50 steps is DEAS's starting choice inside the manufacturer's recommended 32–64 range, not a claimed single upstream default. Other rows follow the official Comfy templates; sources and distinctions are in [MODEL_DEFAULTS](MODEL_DEFAULTS.md#ace-step-variant-specific-reference-settings). First model selection applies the variant profile; returning to an edited model restores its overrides. A checkpoint-only change preserves current Style/Lyrics, including deliberately empty text. Queued jobs retain their submitted snapshot.

ACE-Step 1.5 provides BPM, time signature, language, key, optional audio-code generation and separate LM sampling controls. LM CFG starts at 2, temperature at 0.85, top-p at 0.9, top-k at 0 and min-p at 0; these are not diffusion CFG. Original 1.0 instead exposes lyrics strength (initially 1). The YuE2 ABC/SheetSage2 controls and YuE2-only writing hammers are not used for ACE-Step.

Duration is displayed and submitted on the model's latent clock: 1.5 uses 0.04 seconds (1920 samples at 48 kHz); original uses 4096 samples at 44.1 kHz. Editing duration snaps before display/submission; the compiler rejects off-grid values rather than silently changing them. Original's nominal 120-second starting request is shown as its representable latent-derived duration, not falsely labelled exactly 120. The joint latent-node bound is 1–1000 seconds. These checks establish graph timing, not measured decoder/output length without a model run.

ACE-Step shares the native FLAC saver, job queue, optional audio post-processing, metadata and history/Run Plan. Cover/Repaint is not enabled for ACE-Step. Real ComfyUI CPU schema validation uses scratch filename fixtures without loading weights; it does not prove model inference or perceptual output quality. Real-model and native Windows runtime acceptance remain outstanding in [PLAN](PLAN.md).

## Selecting a music pipeline

- **Music (Text to Music)** and **Music (Cover Music)** sit in the Pipeline picker next to the image and video modes.
- The checkpoint picker lists only files whose safetensors header carries both YuE2 keys (`model.diffusion_model.llm2vae.weight` and `text_encoders.model.embed_tokens.weight`). Put the checkpoint under `models/checkpoints`. A file that only has the YuE2 name is not listed; a renamed file with the keys is.
- Covers need a SheetSage2 audio encoder under `audio_encoders` in your configured models folder (or the backend's own `models/audio_encoders`). The picker shows the names the running backend reports for `AudioEncoderLoader`; nothing is downloaded automatically.
- Missing YuE2 or SheetSage2 nodes on the backend disable only the dependent pipeline. Run names the missing node or encoder in its tooltip and the Run Plan notices.

## Style and Lyrics

Music replaces the positive/negative prompt pair with two independent editors. **Style** is required (genre, mood, instruments, vocal description, language); **Lyrics** is optional and may use section tags such as `[Verse]` and `[Chorus]`; leave it empty for an instrumental. Each editor has its own expand, copy and clear buttons and its own Prompt Forge hammer: Style writes with the DaSiWa `YuE2 Song Style` dialect, Lyrics with `YuE2 Lyrics`. There is no negative prompt and no image reference.

Music texts are stored separately from image prompts (`studio.run.music.*`). Switching to an image or video pipeline and back restores each pipeline's own draft; the Prompt Forge "Send to Generate" handoff is refused while a music pipeline is active because it carries image prompts. Projects (image drafts) never serialize music settings.

Without a configured LLM the hammers stay disabled and point to LLM setup, like the image hammers.

## ABC planning and generation settings

Text to Music chooses **ABC planning**:

| Planning | Graph | Extra settings |
| --- | --- | --- |
| Off | `YuE2GenerateMusic` only; the planning input is empty | AR settings only (the AR guidance control switches to 1.01 the first time you choose Off while it is still at 1) |
| Full | `YuE2GenerateABC` writes melody and chords, then `YuE2GenerateMusic` follows it | ABC settings: max tokens, temperature, top-p, top-k, repetition penalty, penalty window |
| Melody | same as Full in melody mode | same ABC settings |

Autoregressive (AR) settings apply in every mode: temperature, top-p, top-k, repetition penalty and AR guidance (`cfg_scale`). Seed, steps, CFG, sampler and scheduler are the usual sampling controls (defaults 32 / 1.0 / `dpm_2` / `sgm_uniform`, as in the core template). **Maximum duration** bounds the request in seconds (0.04 s steps, at most 900 s); the generated song may be shorter. All bounds mirror the native validation in `src-tauri/src/jobs_music.rs`; the UI rejects values outside them.

## Covers

```
selected audio or video ──LoadAudio──► SheetSage2AudioToABC ──melody ABC──► YuE2GenerateMusic ──► VAEDecodeAudio ──► FLAC
                                        (AudioEncoderLoader)        + Style + Lyrics
```

- Select the source in the Media dock with **Use for music cover** (context menu). Audio files and videos are accepted; images are not offered. Only one source is active; the Media dock marks it.
- Core `LoadAudio` reads the first audio stream. A video without an audio track fails at the backend with `No audio stream found in the file.` — there is no silent text-only fallback.
- The source is uploaded once when you press Run; the submitted intent carries the uploaded name and is frozen. Edits after Run belong to the next job.
- A cover is a melody reinterpretation: planning is always Melody, Lyrics come from you, and nothing copies the singer's voice, transcribes words, or preserves tempo, timbre or exact duration.

## Results

- Output is one FLAC under the output prefix (`raw/audio/…` with the default naming template). Native result handling decodes the whole file (pure-Rust `claxon`, CRC-checked) before it is cached, previewed or exported; a truncated or corrupt file is rejected as an invalid output rather than shown.
- The Workbench renders a standard audio player with controls and no autoplay; the queue shows ♪ for audio jobs. Generation metadata lists the music fields; the image "use these settings" action is not offered for music.
- Recent generation history restores a music record completely (pipeline, texts, planning, durations, encoder, sampling). The cover source must be re-selected explicitly; until then Run stays blocked and names the missing file.
- Save As follows the existing no-clobber output export; the decoded bytes are what was retained, never a re-fetch.

## What was verified, and what was not

Tested revisions: ComfyUI core `b1a247c9` (2026-10-19) for the YuE2/SheetSage2 node schemas and `SaveAudioAdvanced` history shape; ComfyUI-DaSiWa-Nodes `ab364c5` for the `YuE2 Song Style`/`YuE2 Lyrics` writer presets. The node schemas were captured in-process from a throwaway CPU-only ComfyUI (`--cpu`, port 8199, stopped afterwards): all compiled Full/Melody/Off/cover graphs pass `validate_prompt`; `LoadAudio` decodes a FLAC and an MP4 with audio and rejects a silent MP4 with the error above; `SaveAudioAdvanced` lists its FLAC under the `audio` history key.

Automated: Rust compiler/validation/prerequisite/FLAC-transport tests, header-detection test, Vitest suites for `MusicPrompts`, `MusicSettings`, Run Plan, metadata, history restore and the Workbench music flows, six-locale key parity.

**Not verified:** the layout/screenshot gate does not switch Generate to the music pipelines, so narrow-width geometry of the music controls is unchecked; no song has been generated with real weights (GPU runs were not authorized in this slice), so music quality, ABC-mode behavior, cover melody relationship, scoped cancellation during generation and actual duration/sample-rate/VRAM are unmeasured. FLAC playback was exercised in the Chromium test runtime only — Linux WebKitGTK and Windows WebView2 playback and the Windows runtime are not verified. SheetSage2 list multiplicity on a live backend was not observed.

## License notice

The inspected YuE2 checkpoint header declares CC-BY-NC-4.0. Verify the model card, the SheetSage2 encoder terms and the ComfyUI core license before adding either to a download catalog or redistributing outputs commercially. This page is a usage note, not legal advice.
