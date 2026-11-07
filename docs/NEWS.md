# News

## 0.2.19 BETA

- **Image refinement saves Original first.** Optional image refinement now waits for the encoded Original before starting its post-processing branch, so a later upscale/refine failure does not lose that completed image. Requires the compatible DEAS adapter and saver; start the owned backend explicitly with this build to install the adapter. This does not yet change video ordering.

- **Stop one generation or the whole queue.** Stop now opens a choice instead of immediately stopping everything. Cancel or Escape leaves work running. The selected-item action keeps other jobs and preparations intact; real-backend/native-platform acceptance is still open.

- **H3 opening-frame seams corrected.** Image → Video now defaults to *Fill and crop* reference scaling, so a picture with a different canvas ratio no longer distorts the first frames; the H3 scaling option formerly named *Auto* is now **Native (ShortEdge 2048)** because it only shrinks and never fits.
- **SAM masking and Auto Refine are being integrated.** Select objects with text, points (Alt-click excludes) or a box; refine ordered targets such as `eyes; feet; hands` without changing the generation prompt. Automatic post-generation refinement is opt-in, with manual refinement for existing images and retained originals. SAM weights require separate license acceptance and manual installation. This unreleased integration is not yet real-model or native Linux/Windows accepted. [Guide](STUDIO.md#sam-31-masking-and-auto-refine).

## 0.2.18 BETA

- **Audio enhancement is ready when you open the tab.** Processing options load immediately, **Enhance audio and save…** accepts bookmarked (symlinked) folders, and enhanced video keeps the container and audio codec your TVE/video-combine output picked instead of forcing MKV/FLAC.

- **Use output pictures in Prompt Forge.** Generated reference images and first/last frames are readable by Forge, Cast and Find people without moving them manually into input. Vision controls explain when images are sent.

- **HyperFlow H3 Flash (fast MiniMax H3) works in the Studio.** Put a `minimax_h3_*_hyperflow_flash_14b-*.gguf` from [VideoRebirthAI/hyperflow-h3-flash](https://huggingface.co/VideoRebirthAI/hyperflow-h3-flash) into `diffusion_models/MiniMaxH3/`: DEAS detects it as a HyperFlow variant, defaults to 6 steps with euler at CFG 1 (4/6/8 selectable) and runs the usual H3 pipelines with Continuity. The node pack installs itself with the ComfyUI update. The INT8-convrot files on that hub are not a ComfyUI format — use the GGUF quants (Q4_K_M ≈ 9 GB).

- **Edit themes from Quick Settings.** DEAS creates a starter theme in your environment only when needed, opens the offline editor, and no longer ships duplicate copies of the built-in palettes. Save a unique theme, refresh, and choose it. [Theme guide](THEME-PACKS.md).
- **Smaller Beginner guide bundle.** The four large model-guide illustrations were removed. The guide text, tags, credits and publisher links remain.

## 0.2.17 BETA

- Monitoring, Run Plan, Backend and Logs are grouped on the right; creative tabs remain on the left.
- ACE-Step exports using the `ace-step-1_5` header identifier are recognized correctly, including models found under UNet.
- This release also publishes the Beginner workspace, optional audio enhancement and ACE-Step improvements listed below.

## 0.2.16 BETA (untagged; shipped in 0.2.17)

- Beginner layout refinement: resize directly between preview and prompt, no lower Media dock, rounded Run/Stop ends, and a full-width Beginner/Advanced light switch in Quick Settings.

- ACE-Step now has Text to Music authoring and native recipes for original 1.0 and resolved 1.5/XL Base, SFT and Turbo, with distinct defaults, AIO/split components and separate code-LM controls. Model detection explains uncertain filename evidence in Edit meta; prompts and model-specific overrides are retained. Graph/UI validation is covered; real-model inference remains unverified.

- Beginner now has one creative invitation in the preview, with pipeline instructions kept below it, no Local workspace badge or project controls, and restored pane resizing/collapse arrows. Switch Beginner/Advanced in Quick Settings just above Reset layout.

- Clicking a Linux preview video now pauses it; clicking again resumes playback. The existing playback buttons remain available.

- Beginner mode now looks and works like the advanced Studio: the positive prompt field with tag help sits under the preview without extra buttons; the job queue and all Generate controls appear on the right. Both modes share the same tab styling without a separate DEAS Studio logo; the separate right-hand Engine button is now a matching **Backend** tab, and **Collection** is now **Media**.
- Drag-and-drop of files from your desktop into DEAS now lands correctly on Linux screens with scaling, and dropped images can target the Beginner starting-image row directly; theme editors accept images dragged onto their image rows.

## 0.2.15 BETA (previously local-only; shipped in 0.2.17)

- DEAS now asks **You are… A beginner / An advanced user** on first launch. Beginner mode keeps only Generation (model, LoRAs optional, prompt), Models and Collection; the Engine button reaches setup, and the top-right Beginner/Advanced switch restores your full technical workbench untouched. [Beginner mode](BEGINNER-MODE.md).

- Audio enhancement now offers **Clean**, **Deep** for speech and **Stems** with Smule Labs Windowed RoFormer. Select a mode before video/music generation or TVE enhancement to chain it automatically; audio-only new-copy export also works without video enhancement. Originals remain available. Model/runtime downloads are explicit; Smule's CPU path is currently slow and requires a C++ compiler. [Audio enhancement](AUDIO-ENHANCEMENT.md).

## 0.2.14 BETA

- Install local colour/image themes from `theme-packs/`, customize them with the offline editor and apply changes through Quick Settings → Refresh themes. Existing packs and saved selections are not overwritten. [Theme packs](THEME-PACKS.md).

- Reuse SwarmUI `Stable-Diffusion` and `Lora` folders through the existing model-root settings, including existing directory links. Model-family sidecars are saved beside the actual file without moving weights. The checkpoint dropdown filters candidates for the current pipeline and keeps an unavailable saved selection explicit.

- Hand-edited config files saved with a UTF-8 BOM (PowerShell, VS Code "UTF-8 with BOM") now read correctly: startup settings, model folders, Studio presets, wildcard libraries and opened JSON documents.

- Output-naming failures preserve originals and roll back incomplete publications. Temporal tiled-refine restrictions now also apply to direct native requests. Old guided-setup-only code is removed without changing automatic installation.

- Paste clipboard pictures into the active Forge editor; ordinary text paste still works. Chat can read later guide pages, including Music, App updates and TVE documentation, within its existing tool budget.

- Reuse image/video settings from the media context menu, preview toolbar or a preview drop. Queue prompt reuse is separate and never starts generation. Embedded metadata is explicitly partial; exact queued intents take precedence.
- Restoring compatible Qwen Image 2.1 Turbo LoRAs no longer reports the base model family as incompatible; missing models remain blocked.

- Wildcard categories and your own JSON variables now work as `$name` prompt variables: same seed = same pick, per-occurrence pins win over the seed, the Wildcards live pane shows the exact resolved prompt, chips under the prompts show picks and remove a token with ×. [Prompting guide](PROMPTING.md#wildcards).

- Default sampler is euler everywhere (Illustrious CFG 3.5); family defaults follow the current official recommendations.

## 0.2.13 BETA

- **SD3 and SD3.5 are selectable.** Pick `clip_l`/`clip_g` and `t5xxl` in the two encoder slots plus the SD3 VAE; the recipe uses the dual CLIP loader, the SD3 latent and a shift-3 sampling patch.
- **Chroma and Chroma Radiance are selectable.** Both use the T5-XXL encoder with type `chroma`; Chroma runs the Flux latent, Radiance the pixel latent. The family id keeps the two apart.
- **HiDream is selectable.** `llama_3_8b` plus `t5xxl` in the two encoder slots, the Flux VAE and latent, no sampling patch.
- **PixArt and HunyuanDiT are selectable.** PixArt takes the single T5 encoder, HunyuanDiT the dual `clip_l` + `mt5xl` pair; both use the SD latent and need no sampling patch.
- **Lumina2 is selectable.** Pick the all-in-one NetaYume checkpoint; the family brings the SD3 latent and the AuraFlow shift-4 sampling patch. Z-Image stays separate through the tensor-shape check.
- **SD1.5 and SD2 are selectable.** Plain checkpoint families: SD latent, no sampling patch, grid 8, 512×512 at 20 steps CFG 7. SDXL keeps its own two-encoder signature.
- **Kandinsky 5 Image is selectable.** Qwen2.5-VL-7B encoder, Flux latent, no sampling patch.
- **Ideogram 4 and Boogu Image are selectable.** Ideogram 4 uses the Qwen3-8B encoder and the Flux2 latent; Boogu the full Qwen3-VL-8B encoder and the Flux latent.
- **OmniGen2 is selectable.** Qwen2.5-VL-3B encoder, Flux latent, no sampling patch.
- **Fibo 2.0 and Ernie Image are selectable.** Fibo 2.0 uses Qwen3-VL-4B and Ernie Ministral-3-3B; both take the Flux2 latent.
- **Flux.2 is selectable.** Klein/Dev with the Qwen3-4B or Mistral-3-small encoder, Flux2 latent and VAE; the graph uses Flux2Scheduler with CFGGuider instead of KSampler.
- **Flux.3 is selectable (unverified).** Qwen3-VL-4B encoder with the Flux2 latent and the same scheduler path.
- **Mage-Flow is selectable.** Qwen3-VL-4B encoder and Mage-Flow VAE; prompt, negative and latent come from one encode node, 30 steps at CFG 5.
- **PixelDiT is selectable (unverified).** Gemma-2 encoder, pixel latent, no sampling patch.

## 0.2.12 BETA (untagged; shipped in 0.2.14)

- **Flux.1 is selectable.** Pick the Flux.1 diffusion file, its `clip_l` and `t5xxl` encoders (the second one in the new **Second text encoder** picker) and the Flux VAE, then run Text to Image or Image to Image. The recipe uses the dual CLIP loader, the SD3 latent, a zeroed negative and the blueprint's 20 steps at CFG 1.
- **Two-encoder families no longer need compiler code.** A family declares its dual loader and the second encoder slot; both compilers read the same optional field.

## 0.2.11 BETA (untagged; shipped in 0.2.14)

- **Z-Image is selectable.** Choose the Z-Image Turbo diffusion file, its Qwen3-4B text encoder and the Flux VAE, then run Text to Image or Image to Image. The family brings its own SD3 latent, sampling shift and zeroed negative, so the numbers are the model's, not generic ones.
- **New image families are data, not code.** A family now declares its latent node, negative handling and sampling patch in the bundled family file; unknown models keep the plain recipe.

## 0.2.10 BETA

- **Prompt Forge:** dropping a picture on the Reference images zone works in the native shell, and a drop with no usable picture says so.
- **Ctrl+Enter runs**, through the same Run button, so a disabled Run stays disabled; plain Enter still picks a tag or spelling suggestion.
- **Tiled refine is refused up front on Anima, Krea 2 and H3**, whose latents are 5-D; plain refine still runs.
- **H3 input scaling starts on Auto**, like the Director; a saved choice and jobs saved before this keep restoring as before.
- **Multi-reference → Video Auto aspect** follows the first picture or video by slot, as the Director does; sound-only uses don't count.
- **Undo survives tag and spelling picks**: Ctrl+Z works after picking a suggestion.
- **Videos get the More menu** with **Use these settings** and **Reuse prompt** from the job that made them.
- **Clicking the queue only previews**; right-click a job for **Use these settings**, and Stop stays for active jobs.
- **Changing the checkpoint keeps your prompts** exactly; the model's other settings still come back.
- **GitHub issue forms** for bug, enhancement, question and documentation reports, with version/update fields and a reviewed log export.
- **Checkpoint picker:** the compact native dropdown is restored, without the custom expanding list or extra filter field.
- **MOSS-Image inpaint:** a painted mask (Inpaint mode) works with MOSS now, with the same graph as every other image model.
- **Prompt Forge and Chat use the saved LLM key** for an OpenAI-compatible server, instead of failing with HTTP 401.
- **Library downloads:** files up to 128 GiB, a job up to 256 GiB, no one-hour cutoff, and an interrupted transfer resumes with `Range` and verifies the full SHA-256.

## 0.2.9 BETA

- **Monitoring tab:** the last ten generations with queue wait, generation time, a timeline bar and per-job hardware peaks, plus live CPU, RAM, GPU, VRAM and disk I/O graphs, three per row and sharp at any window size. Works with the backend stopped.
- **Image models can use a local GGUF** for Prompt Forge and the hammer, with no LLM server; Prompt Forge can now be cancelled, and Regenerate on a local GGUF gives a new result.
- **Windows:** RTX VSR / frame generation loads again (`NvVFX_Load … code -12`; takes effect on the next backend start), a local GGUF in a deep folder is no longer "invalid", and the local GGUF runner finds its CUDA DLLs.
- **Checkpoint picker filter:** type to narrow the Studio model list when more than eight models are installed.
- **Logs show progress bars live** instead of only the finished bar.
- **One button look everywhere**, drawn from the active theme.
- **Global search** finds Options cards and Backend controls; Library groups creator sources and names all category tabs.
- Latent upscale no longer complains about a missing upscaler model right after start while the backend is still answering.
- Video prompts say the backend isn't running instead of "Prompt Forge request failed"; a missing spellcheck dictionary is a warning, not an error; cleaner Prompt Forge image prompts.

## 0.2.8 BETA

- **Stop really stops.** The Stop button next to Run interrupts the running generation (long videos, many-step images) again instead of only clearing the queue. Takes effect after the next backend start.
- **MiniMax H3 image edit / inpaint runs again**, with and without a mask (the saver's metadata config pointed at nodes the H3 graph removes).
- **LLM backend:** thinking models (Qwen3.5, gpt-oss) on Ollama answer again instead of "LLM response has no valid text"; OpenAI-compatible servers accept an **API key**, also used by Prompt Forge.
- Click a video preview to play/pause it.
- Presets tab: **New preset…** opens the preset overlay with your changed settings; Post-Processing tab is grouped by stage (Latent upscale, Upscale model, TVE).
- Quick Settings: the spellcheck and tag-autocompletion toggles now show whether they are on.
- **Spellcheck now works the same on Linux and Windows.** DEAS checks prompt fields itself (Studio prompts, Prompt Forge, Chat) with the downloaded Hunspell dictionary — wavy underline, right-click or Ctrl+. for suggestions. Install the dictionary once under Backend → Language; no restart, no admin rights, nothing leaves your PC.

## 0.2.7 BETA

- TVE **Enhance** now shows queued/running/failed itself and prints the backend error instead of staying on "Queued" while the Studio queue is closed.
- TVE RTX stages gained **Disk fallback (use_mmap)** and **Frames per chunk**, so the out-of-RAM advice from the node is reachable from the app.
- Loader errors name the model file, and truncated safetensors are flagged in the Library and never auto-selected.
- **Global search.** A magnifier in the top tab band (Ctrl+K) finds controls across every tab by breadcrumb — Studio settings and prompts, Forge, Chat, TVE, Run Plan stages, Library, Comfy, Backend and Logs — and switches to the hit, opens its group, scrolls to it and flashes it. A control that does not apply right now shows why instead of doing nothing. Search never writes a setting, submits a job or calls the backend.
- **Spellcheck dictionary install (Linux).** When no dictionary exists for the spellcheck language, Backend → Language offers **Install dictionary** for German, English, French and Turkish: the pinned Hunspell pair goes into your user folder, no admin rights. Japanese and Chinese say that no Hunspell dictionary is published instead.
- Windows: installing into a folder with a space (including a user name with a space) no longer fails at Requirements.
- Prompt Forge keeps a written result through a tab switch or the backend finishing its start, and says "No LLM server found" instead of "no models" when that is the actual gap.
- Setup steps past a folder left by a failed or cancelled install (`…-2`) with a note; the old folder is never reused or deleted.
- The footer update arrow clears as soon as a ComfyUI or node-pack update finishes.

## 0.2.6 BETA

- Windows: the backend starts again instead of hanging on "Starting — verifying owned API" / the Backend API check and timing out after 120 s (`object_info … BACKEND_UNAVAILABLE` in Logs).
- After a driver update, Reinstall or Update ComfyUI clears the "hardware/driver changed" start error.

## 0.2.5 BETA

- Quick settings grey out the spellcheck button when no dictionary is available instead of showing a long message.
- Videos in the Comfy tab play again on NVIDIA systems instead of freezing on the first frame.
- In the Prompt Forge tab, **People & order** is shown openly for each frame/reference picture. The button that opens the Forge tab is the hammer again, with inverted colours.
- Studio: an arrow on the Settings edge and on the bottom panel's edge collapses that pane fully and brings it back.
- Everything in a media file's right-click menu (reference, first/last frame, source, cover) is now also a button in the preview toolbar.
- Library: **Catalog source** now also offers the CivitAI creators TrashAI, LolTurdFerguson and DustyDrab; DaSiWa stays the default.

## 0.2.4 BETA

- Backend updates, reinstalls and environment moves now stop a running ComfyUI gracefully and start it again afterwards; a stopped backend stays stopped. Updated Linux DEAS binaries reopen by themselves.
- **Move environment…** in Backend moves your whole DEAS environment to another folder or drive without reinstalling; DEAS fixes the paths inside the venv and checks the moved Python before switching. A running owned backend is stopped gracefully and restarted afterwards; a stopped backend stays stopped.
- H3's **Prompt Forge** button beside Continuity opens the existing tab with the current run's references, people/order and full Director prompt controls. First/last-frame people editing moved out of pipeline settings; drafts and cards stay when switching tabs.
- Right-click an image in the Media menu to use it as **first frame**, **last frame** or both. MiniMax H3 can run Image → Video from a last frame alone.

## 0.2.3 BETA

- MiniMax H3: the new Sol VAE (`minimax_h3_sol_vae_fp16.safetensors`) shows up in the VAE picker. Update ComfyUI first — it needs the upstream Sol-H3-VAE support.
- Backend shows where your DEAS environment is installed, with a button to open the folder.
- The startup splash no longer just flashes on fast starts.

## 0.2.2 BETA

- Changing folders in Backend → Settings now restarts a running backend, so newly pointed model folders (e.g. MOSS) are found right away. A running generation is never interrupted; you get a note to restart afterwards.

## 0.2.1 BETA

- **Startup splash** with the real startup phase; the main window appears once your workspace is restored. The owner's Catgirl artwork slot is ready but still empty.
- **Backend not ready?** Studio says so in one calm line and disables Run, instead of a red list of "incompatible" dependencies.
- **Stop** is quiet and frees the queue; right-click a queue entry to stop just that generation.
- The preview toolbar always sits top right; Linux audio previews (YuE2 FLAC) play through the native player with controls at the bottom.

See the [changelog](CHANGELOG.md#0211-beta-untagged-shipped-in-0214) for details.

## 0.2.0 BETA

DEAS moves from ALPHA to **BETA**. Version `0.2.0` includes the `0.1.67` development work below. BETA is not a production-readiness claim: platform, model and playback acceptance remain as recorded in [the plan](PLAN.md) and [verification](VERIFICATION.md).

## 0.1.67 (untagged; shipped in 0.2.0)

H3 latent upscale has a **☰ More upscaler options** menu in Post-Processing with all remaining node settings (refine CFG/scheduler, precision, tiling/chunking, memory planning, Spike Guard, Continuity soft refine), starting at the node defaults. Cover Music now finds SheetSage2 encoders in your models folder's `audio_encoders`. See [Studio](STUDIO.md#learned-video-latent-upscale-0164-shipped-in-0166) and [Music](MUSIC.md).

## 0.1.66

Generate now offers **Music (Text to Music)** and **Music (Cover Music)** for YuE2 checkpoints: independent Style/Lyrics editors with ABC Off/Full/Melody planning, maximum duration and advanced sampling; covers build a new song from the melody of a selected audio file or video-with-audio. Outputs are FLAC, kept in Recent history with full settings restore. Real-model generation and native playback acceptance are still open. [Usage and limits](MUSIC.md).

## 0.1.64 (untagged; shipped in 0.1.66)

Generate's Post-Processing adds learned H3 and LTX latent upscale, separate from TVS and LTX's existing multipass refinement. Enable it, select the compatible latent model, and review the final canvas before Run. If weights are missing, a Library popup offers the family-specific model in `latent_upscale_models`; opening it never starts a download or generation. [Usage and limits](STUDIO.md#learned-video-latent-upscale-0164-shipped-in-0166). Shipped in 0.1.66.

## 0.1.63 (untagged; shipped in 0.1.66)

Studio adds inline character counts and shared writing aids for normal/enlarged prompts. Configure the independent spellcheck language below Backend’s language selector; toggle spellcheck and optional local Forge-tag completion there or in Quick Settings. Completion starts off. See [usage and dictionary requirements](STUDIO.md#prompt-writing-aids-0163-shipped-in-0166). Shipped in 0.1.66.

## 0.1.66 — Chat prompt routing fix

Chat now keeps the requested generation model: it refers to that model’s Prompt Forge mode first, falls back only to a compatible registered simple writer, and otherwise says no compatible writer exists. H3 requests no longer end by offering four unrelated image models. Referrals name the exact Write for mode without switching tabs or modifying drafts automatically. [Usage and limits](CHAT.md).

## 0.1.66 — model folder fix

Studio now explains known models in the wrong loader folder instead of offering them as runnable. Anima belongs in `diffusion_models` (or `unet`) for its split recipe, not `checkpoints`. Files remain visible; move them yourself and refresh the list. Native preparation also distinguishes an opposite-loader listing from a missing model. No model moves or automatic loader changes. See [Studio model controls](STUDIO.md).

## 0.1.62

H3 References separates selection from editing: compact thumbnails show the current crop and stream choice, and the copper-glowing Edit button opens one larger media dialog. Close/Escape retains changes and returns to the card; the underlying reference picker stays open. Existing crop/playback, people and Forge controls are retained. Linux keeps native libmpv playback and uses cached FFmpeg thumbnails. [Usage](STUDIO.md#multi-reference-overlay). The preview **More** menu closes on its own click and outside clicks; **Use these settings** moved there from the metadata dock, next to the new **Reuse prompt** which refills prompt and sampler settings from a file's embedded metadata.

## 0.1.59 — References overlay

Multi-reference authoring now has a Continuity-style References popup with its own media picker and inline previews. Choose picture, sound or both from a video, set crop endpoints with fields/sliders or the playhead, and audition only that segment. Existing Director labels, people controls, packs and RefMods stay available; edits do not rewrite source files. See [Studio](STUDIO.md#multi-reference-overlay) for supported paths and remaining parity limits.


## 0.1.58 (untagged; shipped in 0.1.59)

H3 Continuity is now available for Video → Video and Multi-reference → Video as well as text/image inputs. Continue a pinned checkpoint or prepared input video with the existing serial planner, and opt in to current reference conditioning when needed. Generation and Next action Forge use the same reference policy. The Director’s mode-family and canvas checks remain authoritative; this does not add LTX/Wan continuity or establish real-model seam quality. [Usage](STUDIO.md#ref2va-continuation-references). Shipped in 0.1.59.

## 0.1.54 (untagged; shipped in 0.1.55)

TVE adds native file-authority preparation and an optional Linux rendering engine. Explicitly bind trusted runtime/model files, validate and render a short SDR clip, or Stop its owned Rust worker. A real four-frame CUDA video passed the managed parent/worker/publication path with audio and source preservation; this is not full-resolution/general-model or Windows-render acceptance. See [workflow and limits](STUDIO.md#tve-workspace). Shipped in 0.1.55; since 0.1.56 every build includes the engine (still an explicit runtime opt-in).

## 0.1.53

TVE now has its own main tab. Inspect a video source, choose a scale/export-profile draft and explicitly check its exact output metadata in Rust; source/draft state survives navigation without computation. Image refinement stays in Studio. The draft is not a runnable enhancement job and does not verify an encoder, GPU or source cadence. See [TVE workspace](STUDIO.md#tve-workspace) and [remaining execution work](PLAN.md#42-video-post-processing-tve-and-paired-products).

## 0.1.52

Post-Processing can inspect a local video source without uploading or processing it. The native Rust check shows dimensions, exact frame-rate fractions and source limitations; cancel retains the last inspected result. This is a preparation feature, not video upscaling/interpolation/HDR rendering. The standalone video pipeline is required to use Rust without Go/Python workers. See [usage and dependencies](STUDIO.md#tve-workspace) and [video plan](PLAN.md#42-video-post-processing-tve-and-paired-products).

## 0.1.51 (untagged; shipped in 0.1.52)

Optional image refinement now produces labelled Original and Processed products for individual selection and comparison. Already-reported completed products remain available when later work fails or is cancelled. This is the first bounded post-processing slice, not a full video-enhancement port or a guarantee that a baseline saver runs before every failure. See [usage and limits](STUDIO.md#original-and-processed-image-products-0151-shipped-in-0152) and [remaining work](PLAN.md#42-video-post-processing-tve-and-paired-products). Shipped in 0.1.52.

## 0.1.50

Prerequisite diagnostics now accept Windows subfolder model paths (`Folder\model.safetensors`) against the backend's slash-separated model lists, fixing "diffusionModels are not available on this backend" for models inside subfolders (the `+` in model names was never the problem). Krea2 keeps full torch.compile support. See [verification](VERIFICATION.md).

## 0.1.49

Windows/Linux path fixes preserve exact backend model identifiers and output subfolders, repair media-reference/Starred matching and remove developer-specific script assumptions. MOSS reference readiness now checks its active encoder. Ordinary native Prompt Forge modes offer all fixed aspect presets from the main Settings while keeping legacy saved choices; Auto/Custom and MOSS layout/Generate geometry remain deferred. See [path compatibility](PATH-COMPATIBILITY.md), [Prompt Forge](PROMPTING.md) and [verification](VERIFICATION.md). Local unsigned binaries; native Windows runtime acceptance remains open.

## 0.1.48

This release includes the model-aware SDXL/Qwen/Krea/MOSS prompt-reference wiring developed under 0.1.47 below.
Prompt references use compact numbered picture thumbnails rather than full-width filenames. Remove a picture with its red X, drag pictures into a new order. The numbering follows the order used by the next generation; each picture keeps its own ReVision strength. See [image references](STUDIO.md#image-references).

## 0.1.47 (untagged; shipped in 0.1.48)

Studio’s prompt picture drop is now wired to the selected image family: SDXL ReVision, Qwen Image 2.1/Turbo native text/vision plus reference latents, and Swarm-style Krea 2/MOSS visual conditioning. Attached filenames and Run Plan show the inputs; model-specific prerequisites still apply. The small Krea/MOSS adapter is bundled for approved DEAS-owned backend starts, not silently installed into foreign backends. See [Studio usage/support](STUDIO.md#image-references) and [verification](VERIFICATION.md#unified-image-reference-wiring-0147). Shipped in 0.1.48; not a universal quality/identity guarantee.

## 0.1.46

The native-only cancellation foundation can remove one correlated queued prompt or request a scoped interruption of one running prompt on the current owned backend, with explicit confirmation and readback. Running interruption uses an authenticated DEAS queue-locked route and is confined to inspected backend sources; no Studio cancellation button or whole-queue Stop is enabled. Confirmed cancellation has a six-language terminal label. Repeated Run/Stop still awaits whole-queue lifecycle integration. See [running-interrupt verification](VERIFICATION.md#scoped-running-interruption-and-status-abort-recovery-0146) and [pending-cancel verification](VERIFICATION.md#native-pending-cancellation-foundation-0146).

Image Run now reports a missing Wildcards plugin before preparation instead of failing after the click; enabled settings remain until explicitly disabled. The native all-job polling foundation is tested but is not connected to Studio yet, so repeated Run/Stop remains planned. See [verification](VERIFICATION.md#workbench-all-job-polling-foundation-and-wildcards-readiness-0146).

## 0.1.45

MOSS-Image gains a bounded t2i/i2i generation adapter through the existing image pipeline. Queue thumbnails now place stars on their corresponding outputs and show measured generation duration. This is not MOSS inference acceptance. Repeated Run/Stop, generation-reference prompting and Prompt Forge aspect-ratio parity remain open. See [Studio](STUDIO.md) and [verification](VERIFICATION.md#moss-image-adapter-and-queue-presentation-0145).

## 0.1.44

Native image Prompt Forge, Studio Chat and colour schemes are integrated. The required prompts, Randomizer and optional tag/character tables ship in the executable; native LLM model selection works without starting ComfyUI. DEAS is now licensed under GPL-3.0-or-later. See [Prompting](PROMPTING.md), [Chat](CHAT.md) and [data provenance](../src-tauri/forge-data/README.md).

## 0.1.42

Save custom Studio configurations in **Presets**, load them later and edit/update them for quick reuse. **Wildcards** remains a separate tab with the existing prompt-library controls. See [Presets](PROMPTING.md#presets).

[Documentation index](README.md) · [Plan](PLAN.md)

News summarizes user-visible development changes; it is not proof that a tagged release, signed artifact or platform acceptance exists. Check [official releases](https://github.com/darksidewalker/DEAS/releases), the [full changelog](CHANGELOG.md) and [verification evidence](VERIFICATION.md).

## 0.1.41 (untagged; shipped in 0.1.42)

- Studio keeps companion file choices under the compact **Model files** disclosure beneath its other settings. Its Checkpoint dropdown and Models tiles update each other; all Settings dropdowns share the compact selector style.

## 0.1.40 (untagged; shipped in 0.1.42)

- Studio can recognize supported companion architectures without relying on their download names. Each text-encoder/VAE selector also has its own **All files** switch for manual selection, including audio VAEs and text projections where required. Switches default off; selecting unverified components is not a compatibility guarantee. See [Studio component selection](STUDIO.md#text-encoders-vaes-and-companion-files).

## 0.1.39

- Opening raw/starred now creates the missing fixed directory under the selected native-approved Output folder; arbitrary missing browse paths still show errors.

- New renders use Output/raw; the star moves them to Output/starred under the same named subpath, and Unstar reverses that switch. Older stars need no new records. Input tiles have no star. Output, Starred and Input buttons navigate directly; the Output-relative tree starts at root instead of showing the full filesystem trail.

## 0.1.38 — 2026-10-21

- Resize the Settings and Queue columns against the central preview. Quick Settings beside GitHub separates layout reset from confirmed generation-default reset. Missing LLM setup takes you directly to the LLM Backend pane.
- Project actions move beside Run. Export/import a portable **draft ZIP** containing baseline image settings and its managed input PNG; generated results and full Studio-state persistence are still not included.
- Star eligible generated media to physically move it into Output/starred. Native safety checks keep unmanaged/project-linked/sidecar-associated files unavailable rather than risking broken references.

## 0.1.37 (untagged; shipped in 0.1.38)

- Backend configuration now uses a left-hand menu for Startup optimizations, LLM backend and Provider authorization, shown in one matching right-hand pane. Switching panels retains unsaved drafts; explicit Save/Apply behavior is unchanged.

## 0.1.36 (untagged; shipped in 0.1.38)

- Image previews now fit to the available window edge over a semi-transparent, non-checkered background. Click outside the image to close; zoom and pan remain available. The transparent DEAS v… footer shows backend startup progress, then a brief green check that vanishes after 3 seconds.

## 0.1.35 (untagged; shipped in 0.1.38)

- Use nearly the whole window: 3px exterior edges and a floating version hint. Backend and Logs join the top tabs; GitHub/star move to the header right. Backend now shows its settings and provider authorization directly, with a compact status overview. Click a read-only image preview for zoomable full-window viewing; edit canvases keep their existing behavior.

## 0.1.34 (untagged; shipped in 0.1.38)

- More vertical space for media: one top tab band, footer utilities and Backend-owned installer/authorization. Browse media without replacing your edit draft, and return shifted images to a clearly marked original frame.

## 0.1.33 (untagged; shipped in 0.1.38)

- Closing the main window waits for pending project changes: save first, or explicitly discard a failed draft. A missing close guard is reported instead of silently implying protection.

- Keep baseline image settings and an explicitly imported PNG in a native project folder, then reopen them without starting a job. Global Studio defaults remain separate. Jobs, generated results, optional processing and editor layers are not restored by this first slice; see [Projects](PROJECTS.md).

## 0.1.32 (untagged; shipped in 0.1.38)

- Upload a local video directly from Continuity, then prepare and select it explicitly. Originals are preserved; no playback or generation starts automatically.

- Continue from a prepared video already in ComfyUI input: prepare, inspect metadata, select, then Run or draft Next action with Forge. Local upload and native playback remain separate work.

- Ask Prompt Forge to draft a checkpoint continuation’s Next action, review it and apply only to that step. No automatic generation or replacement of the main prompt.

- Continue from an explicitly selected checkpoint in the active H3 session. Loading/selecting is read-only; imported-video sources and Forge-written Next action remain planned.

- Name Continuity sessions explicitly without deleting backend clips. Applying a name retains the plan settings and starts fresh local clip tracking; backend-wide session browsing remains planned.

- H3 Continuity adds an overlap selector for later segments, retaining the 22-frame default and explaining backend shortening for limited context.

- H3 Continuity now opens as its own overlay instead of occupying the dock, preserving the existing plan and sequential run controls. New Director continuity parameters and Forge Next action remain planned.

- H3 first/last-frame modes gain explicit character/order/box assignments and editable character Cast cards in the Generate hammer. No automatic card inference or video graph change.

- Generate's H3 REF2VA hammer supports editable Pose cards with explicit vision Read. Cards stay with their picture when references are reordered. Automatic reads are still planned; no new model/platform acceptance or distributable is implied.

## 0.1.31 — 2026-10-19

- Director-compatible group labels and Characters 1–32, people/order selection, manual person boxes, and explicit **Find people** candidates with separate application. Cancellation is request-scoped and best effort; real vision quality remains unverified.
- Outpaint areas within the source frame, original-size inserted pictures, taller prompt fields with Copy/Clear, and corrected release staging/provenance.
- Source and local unsigned release builds do not imply Windows runtime, GPU quality/performance or oldest-Linux portability acceptance. See the release manifest and verification journal for exact evidence.

## Included maintenance (0.1.31)

- H3 character references gain manual people boxes for directing Cast reads. **Find people** now offers explicitly applied candidates through the selected vision model; stopping is best effort. Real vision quality and native-platform interaction are not newly verified.
- H3 character references gain **People & order** for selecting people and their spatial arrangement. Manual/detected boxes are available separately; frame-mode Cast controls remain planned.
- MiniMax H3 reference pictures support ordered character-group labels and separate Cast cards per member, with Director-compatible pack storage. Vision/model and native-platform acceptance are still open.
- Rust-first, cross-platform, atomic-plugin and ComfyUI coexistence rules are now project-local. A consolidated implementation plan distinguishes code, recorded acceptance and missing work.
- User documentation now lives in focused guides under `docs/`; README is the short installation/start/documentation entry point. Changelog history is preserved here, with a root compatibility pointer.
- NVIDIA preflight retains the host driver environment and returns bounded failure stdout/stderr and raw exit diagnostics; regression tests accompany the change. This is not Windows runtime acceptance.

## 0.1.30 — 2026-10-19

- References dock for image/video/audio inputs, Director-compatible reference packs, MiniMax H3 RefMods, input scaling, prompt helpers and run-aware Prompt Forge controls.
- The base image is now paintable and movable/resizable like inserted images; drag layer rows to reorder. Changed base compositions are uploaded separately from inpaint masks. Mask display opacity defaults to 75%.
- Fix Windows installation Verify failing on Unix-only `signal.alarm`, and installation consent blocked by reordered folder fields.
- Regional image prompting is planned, not shipped. Native playback/audio, real-model quality/performance, native drag/dialog and Windows-runtime limitations remain; see [verification](VERIFICATION.md).

## 0.1.29 — 2026-10-19

- **Run Plan** (German **Ablaufplan**) replaces Workspace map: inspect the current generation stages and effective settings before Run, including image refinement and video/audio/output choices. Edit in Generate; opening the plan never starts generation or uploads media.
- Backend-resolved values are explicitly unresolved. Engine diagnostics and old local drafts remain available below the plan. This is not the roadmap's editable workflow planner or runtime tracking.

## 0.1.28 — 2026-10-19

- Generate adds **Video → Video** and **Multi-reference → Video** for LTX 2.3 and MiniMax H3 (H3 through the Director's REF2VA), and MiniMax H3 **Image → Image / Inpaint** in the Director's Image Inpaint style. Graphs validate offline against a real ComfyUI checkout; no GPU run of these paths yet.
- Editor and workspace: magic-wand masks, Ctrl+wheel brush size, zoomable slider compare and full view, metadata overlay, dock views (tiles/large/list), pipeline dropdown, run hints on the preview, “Open in folder” for media.

## 0.1.27

- Generate has family-specific LTX 2.3, Wan 2.2 and MiniMax H3 video recipes, output settings and an H3 Continuity dock. A real owner-run H3 I2VA job reached succeeded; broad model/platform acceptance remains open.
- **Known blocker:** native video playback can crash in GStreamer Matroska demuxing after playback starts. Audible audio is unconfirmed. Do not treat the alpha as playback-complete.
- Prompt Forge, presets/wildcards, image masks/layers and opt-in image refine/upscale share the generation workspace; see [Studio](STUDIO.md) and [prompting](PROMPTING.md).
