# Audio enhancement

Audio enhancement is optional post-processing for Studio audio/video generation and TVE. It publishes a new product and retains the encoded baseline. Changing a setting never starts a generation, model download or service.

## Studio: automatic after generation

Open **Post-Processing → Audio enhancement**, choose a mode and content profile, then run the audio/video pipeline normally. The effective `audioEnhance` setting is captured in that job's immutable intent. Off omits the stage and leaves existing recipes unchanged.

The native job owner watches backend completion and runs the selected audio stage without requiring Studio or TVE to remain open. The job stays running until audio processing finishes. Original and processed products appear in the job/history; Stop targets the local audio stage once backend generation has completed, not another ComfyUI job.

This file-based stage applies to supported encoded audio/video products rather than model-specific latent graphs. It does not change the model's conditioning, generated frames or generation recipe. Silent videos are a no-op. Missing engines and invalid content/profile combinations fail explicitly; a requested Deep/Stems operation is never silently relabelled Clean.

## TVE: audio-only or combined with video enhancement

Select and inspect a video in TVE.

- **Audio only:** select the audio mode and use **Enhance audio and save…**. No video stage or ComfyUI backend is required for this operation on the inspected source.
- **Video and audio:** select the video stages and audio mode, then run TVE enhancement. The audio stage follows the completed video output automatically; a second audio-export click is not required.
- **Video only:** leave audio enhancement Off.

The explicit save action is also available for a selected completed Studio/TVE output. It names the actual retained source/result and opens a native save dialog. Cancelling the chooser publishes nothing. It never uses an unrelated current generation draft as source authority.

Studio and TVE remember their own settings, initially **Off**. Their modes are alternative strategies, not a promise that more machinery always sounds better.

## Modes and content

| Mode | Engine and operation | Appropriate content |
| --- | --- | --- |
| Off | No audio processing | Preserve the baseline exactly |
| Clean | FFmpeg mix loudness normalization, or speech-oriented highpass/denoising/deessing/dynamics | Conservative whole-mix treatment; speech cleanup only when selected |
| Deep | Native DeepFilterNet speech restoration followed by final mix-level loudness management | Speech; not a complete song/movie mix |
| Stems | Smule Labs Windowed RoFormer vocal extraction, instrumental complement, per-stem treatment and remix | Separate vocal/music treatment; two stems, not independent bass/drums/other |

Stems does not apply speech isolation indiscriminately to singing. A speech-oriented profile may additionally clean the extracted voice; a music/mixed profile retains the intended musical content. Relative stem gains must survive separation and remix rather than normalizing each stem independently.

Noise suppression can remove wanted quiet detail. Separation can create bleed/artifacts. These operations cannot reconstruct missing fidelity, undo arbitrary model artifacts or repair previously clipped samples merely by limiting future peaks. Keep the baseline for comparison.

## Installation and provenance

**Clean** needs compatible FFmpeg and FFprobe on the native application's PATH. They are external tools, not guaranteed bundled installer assets.

**Deep** installs an optional pinned DeepFilterNet native executable. Official v0.5.6 Linux/Windows releases include the default model, so the default engine does not require a separate model archive. Its code is dual MIT/Apache-2.0. The wrapper must pad/flush and trim exact samples: upstream delay compensation alone does not preserve the final partial hop.

**Stems** uses [Smule Labs Windowed RoFormer](https://github.com/smulelabs/windowed-roformer), not Demucs. The [original model publisher](https://huggingface.co/smulelabs/windowed-roformer) declares MIT; the [paper](https://arxiv.org/html/2510.25745v1) explicitly releases both code and checkpoints under MIT. Preserve those notices. An explicit model license is not a blanket guarantee about every training-data or input-music right.

Optional runtime/model acquisition requires an explicit **Install** action and confirmation. Opening the panel, selecting a mode and checking capabilities never downloads weights. Artifacts and dependencies are pinned and checked before use; inference uses local artifacts rather than fetching an unpinned default branch. The specialized runtime is isolated from ComfyUI's Python environment.

The Smule upstream CLI's overlap argument is not implemented. DEAS orchestrates overlapping 8-second windows with 4-second overlap and complementary linear assembly in Rust, retains the final partial window and trims exact samples. Instrumental is the input mix minus the extracted vocals. The unmodified upstream CLI is reloaded per window and uses CPU PyTorch 2.8/FlexAttention; it needs a working C++ compiler for CPU kernel compilation. Installation runs an actual inference check before declaring the runtime ready.

**Performance limitation:** real tests on a Ryzen 7 7800X3D took about 73 seconds for a one-second input and 115 seconds for a nine-second input. These are measured CPU tests, not a GPU benchmark. Research-paper attention-FLOP reductions must not be advertised as application speedups.

## Stems GPU acceleration and memory admission

Stems accepts a device choice `auto | cpu | cuda` (older saved settings behave as `auto`). Explicit `cuda` never falls back silently; `cpu` always remains available. Clean and Deep ignore the device field.

GPU inference reuses the approved owned DEAS environment's CUDA PyTorch **read-only** through a small bootstrap that imports the pinned upstream Smule sources from the isolated runtime; nothing is installed into that environment. Only a GPU codec bridge absent there (currently `torchcodec`) is installed into a separate hash-locked `gpu-extras` directory. The binding is published only after an ABI-matched probe and one real CUDA inference window succeed during install; otherwise the runtime stays CPU-only and the UI shows GPU as unavailable. The binding records only a CPython ABI and relative site path, so an incompatible environment change invalidates it instead of mixing binaries.

Before any Deep/Stems processing, and before the GPU smoke, native admission measures host RAM and GPU VRAM. If the reserve is insufficient, the owned ComfyUI backend is asked to release **its own** model/VRAM caches (`/free`, models + empty-cache) — but only while that backend is owned, running and queue-idle. HTTP acknowledgement is never trusted alone: memory is re-measured afterwards. Foreign services, running backend jobs and unrelated processes are never stopped, and no service is restarted. Insufficient resources fail with `AUDIO_MEMORY`/`AUDIO_DEVICE` instead of unbounded fallbacks.

One shared native compute gate serializes audio processing with owned-backend release requests and submission windows, so an admission decision cannot be invalidated by an interleaved start. The automatic post stage defers (bounded waits) while the gate is busy instead of interrupting other work.

## Outputs, safety and limits

Audio-only enhancement copies the encoded video stream instead of decoding/re-encoding pictures. Video products are written as **MKV with FLAC audio**; audio-only products use **FLAC**. Audio is necessarily decoded/re-encoded and may produce larger files than the original lossy track. Original audio rates/channels and decoded length are checked before publication. Unsupported stream/container combinations fail instead of silently discarding tracks.

Exports use a new destination, no-clobber publication and owned-process cancellation. Source replacement, changed owned output roots and malformed/native IPC results are rejected. A later audio failure or cancellation retains completed baseline/partial products and does not mark the whole job successful. Diagnostics go to central Logs without raw private paths or subprocess secrets.

Real model quality, device performance and Windows/native playback acceptance must be distinguished from unit/browser fixtures and cross-compilation. See [verification](VERIFICATION.md) for evidence and [PLAN](PLAN.md#412-audio-enhance-across-audio-pipelines) for remaining acceptance. A separate standalone ComfyUI enhancement node is not part of this app integration.
