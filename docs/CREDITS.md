# Credits and upstream references

[Documentation index](README.md) · [Plan](PLAN.md)

## Project

DEAS — Dark Engine AI Studio is maintained under the [darksidewalker GitHub project](https://github.com/darksidewalker/DEAS). The [contributor history](https://github.com/darksidewalker/DEAS/graphs/contributors) and Git history identify individual contributions; this document does not invent contributor names or confer rights to upstream works.

## Current runtime and development foundations

| Project | Role | Source / notices |
| --- | --- | --- |
| Tauri and Rust ecosystem | Native shell, IPC and native services | [Tauri](https://tauri.app/), [Rust](https://www.rust-lang.org/), exact crates in `src-tauri/Cargo.lock` |
| zip-rs (`zip`) | Bounded native draft ZIP import/export; Stored/Deflate only | [zip-rs](https://github.com/zip-rs/zip2), MIT; exact dependency in `src-tauri/Cargo.lock` |
| rusqlite (bundled SQLite) | Read-only access to Prompt Forge's tag and character databases | [rusqlite](https://github.com/rusqlite/rusqlite), MIT; [SQLite](https://sqlite.org/) is public domain |
| PromptForge | The native image-model Prompt Forge: prompt files, tool definitions and guidance documents (exported verbatim into `forge-data/forge-pack.json`, which records the source commit) the ported canon, tool-loop, revision, randomizer and clean-up logic in `src-tauri/src/forge/`, and the builder's layout and camera/light tile drawings in `src/features/forge/`; the Randomizer vocabulary (`forge-data/randomizer.json`) comes from the owner's Total Chaos Randomizer workflow via PromptForge | [PromptForge-Public](https://github.com/yoimtrashgaming-prog/PromptForge-Public) |
| PromptForge Chat | Studio → Chat ports PromptForge's chat: its system prompt (exported into `forge-data/forge-pack.json`), the `write_prompt` tool design, picture handling and the DuckDuckGo/SearXNG web search (`src-tauri/src/forge/assistant.rs`, `search.rs`) | [PromptForge-Public](https://github.com/yoimtrashgaming-prog/PromptForge-Public) |
| MOSS Layout Studio | The MOSS layout editor in the Prompt Forge tab (`src/features/forge/LayoutStudio.tsx`) is a fresh implementation of Azmagane's MOSS Layout Studio as PromptForge embeds it: boxes on a canvas written out as MOSS's centred `yxyx` Layout JSON, the two-step generate and the Rewrite instruction | Azmagane, via PromptForge |
| Darksidewalker's OCs | The colour schemes (Liiti Pink, VOID, Slimy Teal, Clawy Grey, Android White, Dragon's Hoard) take their palettes from Darksidewalker's original characters | Darksidewalker |
| React, TypeScript and Vite | Web presentation and build tooling | [React](https://react.dev/), [TypeScript](https://www.typescriptlang.org/), [Vite](https://vite.dev/), exact packages in `package-lock.json` |
| ComfyUI | Managed model/graph execution and the original Comfy editor | [ComfyUI](https://github.com/Comfy-Org/ComfyUI); preserve upstream licensing/notices |
| ComfyUI-GGUF by city96 | Quantized diffusion and single/dual text-encoder computation, provisioned separately from the app and model weights | [ComfyUI-GGUF](https://github.com/city96/ComfyUI-GGUF/tree/6ea2651e7df66d7585f6ffee804b20e92fb38b8a), Apache-2.0; preserve upstream notices. No GGUF LoRA or full-checkpoint loader is advertised. |
| ComfyUI-DaSiWa-Nodes | Selected prompt, Director/continuity, metadata/output, wildcard computation and the `YuE2 Song Style`/`YuE2 Lyrics` writer presets | [DaSiWa nodes](https://github.com/darksidewalker/ComfyUI-DaSiWa-Nodes) |
| YuE2 and SheetSage2 | Music generation and melody transcription through ComfyUI core nodes; checkpoint and audio-encoder weights are separate downloads, not bundled — the inspected checkpoint header declares CC-BY-NC-4.0, review the model card and encoder terms before redistribution | [YuE2 in ComfyUI core](https://github.com/Comfy-Org/ComfyUI/blob/master/comfy_extras/nodes_yue2.py); see [Music](MUSIC.md) |
| H3 and LTX learned latent upscalers | Optional Library model offers for ComfyUI latent post-processing; weights are separate downloads, not bundled | [LBH-123-AI MiniMax H3 latent upscaler](https://huggingface.co/LBH-123-AI/Minimax_h3_latent_Upscaler), consumed through the DaSiWa nodepack; [Lightricks LTX-2.3](https://huggingface.co/Lightricks/LTX-2.3), consumed through ComfyUI core. Review the respective model/repository terms before use or redistribution. |
| ComfyUI-KJNodes | Optional model/attention/chunking capabilities where recipes require them | [KJNodes](https://github.com/kijai/ComfyUI-KJNodes); presence is not provisioning/compatibility proof |
| uv, Python and PyTorch | Managed runtime/bootstrap and tensor/model computation | [uv](https://github.com/astral-sh/uv), [Python](https://www.python.org/), [PyTorch](https://pytorch.org/) |
| libmpv (mpv) | Linux native video playback, loaded at runtime from the system (`libmpv.so.2`); not bundled | [mpv](https://github.com/mpv-player/mpv); LGPL-2.1-or-later or GPL-2.0-or-later depending on how the system library was built |
| claxon and ruzstd | Bounded FLAC decoding of YuE2 results; decoding the compressed bundled Forge data | [claxon](https://github.com/ruuda/claxon), Apache-2.0; [ruzstd](https://github.com/KillingSpark/zstd-rs), MIT; exact versions in `src-tauri/Cargo.lock` |
| GStreamer | Linux webview media support and bundled fallback plugins | [Bundled plugin provenance, LGPL-2.1-or-later and corresponding source](../src-tauri/gstreamer/README.md) |
| DM Sans and Manrope via Fontsource | Bundled UI fonts | [DM Sans](https://fontsource.org/fonts/dm-sans), [Manrope](https://fontsource.org/fonts/manrope); preserve packaged font licenses |
| SwarmUI | Controls/sizing/editing references and the Krea/MOSS text-vision prompt adapter behavior (picture markers, placeholders and template); Qwen uses the public ComfyUI node instead of copying Swarm’s custom encoder | [Pinned reference d74f8ce](https://github.com/mcmonkeyprojects/SwarmUI/tree/d74f8ce40d1e49161da61ddb74f87e6ebf11f7ad), Alex "mcmonkey" Goodwin; [MIT notice](../licenses/SwarmUI-MIT.txt). No Swarm server/dependency is bundled; behavior/quality need not be identical. |

Tests/builds also rely on Node/npm, Vitest/Testing Library, Playwright and their dependencies. Manifests/lockfiles identify packages and versions; this credits list is not an exhaustive legal notice inventory. Model weights, vendor SDKs and gated assets carry separate terms.

## Theme packs and image decoder

The drop-in theme implementation, offline editor, Sage palette and optional French/Paris presentations are adapted from the supplied `DEAS-0.2.13-THEME-PLUGIN-UPDATE.zip` (a separate CHOPIN-requested local edition). Original DEAS/character palettes remain credited to Dasiwa / Darksidewalker. The theme-only adoption does not import the separate Beginner application.

French/Paris artwork retains its source manifest and notices in each [optional pack](../theme-packs/optional/themes/): Twemoji graphics by Twitter and contributors (CC-BY 4.0, `LICENSE-TWEMOJI.txt`), Wikimedia Eiffel artwork and the Openclipart street lamp (`SOURCES.json`). Preserve these notices when sharing customized packs.

Static WebP validation uses the pure-Rust [image-webp 0.2.4](https://crates.io/crates/image-webp/0.2.4) (MIT OR Apache-2.0), with [byteorder-lite 0.1.0](https://crates.io/crates/byteorder-lite/0.1.0) (Unlicense OR MIT) and [quick-error 2.0.1](https://crates.io/crates/quick-error/2.0.1) (MIT OR Apache-2.0). Corresponding notices are retained under [`theme-decoder-licenses/`](theme-decoder-licenses/); exact versions/integrity are pinned by Cargo.lock. PNG uses the existing decoder.

## Behavioral and planned integration references

These are sources inspected for architecture/behavior, **not claims that their full implementations are bundled or ported into DEAS**:

- DaSiWa ComfyUI Installer: behavioral/compatibility reference for native Rust provisioning, not a bundled Go installer.
- DaSiWa-TrueVideoEnhancer: composable video enhancement, streaming and render contracts.
- DaSiWa-simple-video-compare: current Rust/Tauri/libmpv comparison/native playback reference; old Go code is not the target architecture.
- DaSiWa-simple-rtx-video-assambler: ordered layouts, audio and FFmpeg export behavior.
- DaSiWa-TrainFlow: trainer profiles, datasets, runtime and sd-scripts/Musubi orchestration behavior.
- PromptForge and the DaSiWa node pack: prompting/reference-authoring behavior; standalone deployment is not DEAS architecture. The image-model writer is now ported (see the table above); PromptForge's server, accounts and online lookups are not.
- Prompt Forge data: bundled from PromptForge-Public commit `29a37c4e43f144f6ff4c0bc8c02f96b440b64826`. The maintainer confirmed redistribution permission for the optional databases. Danbooru tag data and the Apache-2.0/MIT character sources retain their own terms; see [file provenance, licenses and digests](../src-tauri/forge-data/README.md).

Exact local source identities, capabilities and planned acceptance are recorded in [the consolidated plan](PLAN.md). Check each source’s current license and vendored dependencies before copying/distributing it; TrainFlow wrapper licensing does not clear its trainers, and GPL/libmpv/FFmpeg/SDK obligations need their own review. DEAS is licensed under [GPL-3.0-or-later](../LICENSE), adopted with maintainer approval for the PromptForge integration. This does not relicense third-party datasets, models or dependencies.

## Documentation assets and rules

Screenshots under `assets/` document the project; fixture screenshots are labelled as fixtures, not real model outputs or target-native acceptance. The [ComfyUI rules](COMFYUI-RULES.md) are a project-local adaptation of the author's `comfyui-dev` workflow. Preserve required source/author/license attribution when adding future code, models, screenshots or other assets.
