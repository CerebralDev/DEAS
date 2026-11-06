# DEAS documentation

**Current version: 0.2.0 BETA**, previously ALPHA. Existing platform/model limitations and open acceptance gates still apply; see [the plan](PLAN.md).

[Documentation index](README.md) · [Plan](PLAN.md)

## Start and use DEAS

Top-level tabs: Studio · Library · Prompt Forge · Chat · TVE · Run Plan · Comfy · Backend · Logs.

| Feature, tab or action | Guide |
| --- | --- |
| Clean/Deep/Smule audio enhancement, automatic post-processing and TVE audio-only | [Audio enhancement](AUDIO-ENHANCEMENT.md) |
| Installation, Windows/Linux build, launch and first setup | [Getting started](GETTING-STARTED.md) |
| Backend tab: install review, foreign-installation takeover, lifecycle, startup settings, folders and language | [Backend and settings](BACKEND.md) |
| Confirmed application-binary updates, manual restart and platform limitations | [Application updates](APP-UPDATES.md) |
| Studio → Generate, model/LoRA docks, Options, image editing, post-processing, metadata and H3 Continuity | [Studio](STUDIO.md) |
| Image prompt references (SDXL ReVision, Qwen, Krea/MOSS) | [Image references](STUDIO.md#image-references) |
| Multi-reference → Video references overlay and media crops | [Multi-reference overlay](STUDIO.md#multi-reference-overlay) |
| GGUF models and **Edit meta…** architecture assignment | [GGUF and manual architecture](STUDIO.md#gguf-models-and-manual-architecture-assignment) |
| Per-pipeline session drafts, prompt counts, spellcheck and tag completion | [Session drafts](STUDIO.md#session-only-pipeline-and-model-drafts-0164-shipped-in-0166) · [Writing aids](STUDIO.md#prompt-writing-aids-0163-shipped-in-0166) |
| Quick Settings, layout reset and colour schemes | [Layout and Quick Settings](STUDIO.md#layout-last-used-state-and-quick-settings) |
| Install, refresh and edit local colour/image theme packs | [Theme packs](THEME-PACKS.md) |
| Beginner experience: generation, model guides and collection without technical settings | [Beginner mode](BEGINNER-MODE.md) |
| Music (Text to Music / Cover Music) with YuE2 | [Music](MUSIC.md) |
| TVE tab: video source inspection, output drafts and bounded native spatial rendering (Linux) | [TVE workspace](STUDIO.md#tve-workspace) |
| Monitoring tab: last ten generations with timing, live host/GPU telemetry | [Monitoring tab](STUDIO.md#monitoring-tab) |
| Native image-draft projects, autosave and managed PNG input (not job/result recovery) | [Projects](PROJECTS.md) |
| Prompt Forge, hammer, MOSS Layout Studio, presets/wildcards and shared LLM settings | [Prompting](PROMPTING.md) · [Presets](PROMPTING.md#presets) · [Wildcards](PROMPTING.md#wildcards) |
| Chat: conversation, prompt writing, model guides, tag lookups and optional web search | [Chat](CHAT.md) |
| Run Plan (Ablaufplan) and legacy Engine diagnostics | [Run Plan](STUDIO.md#run-plan-and-engine-diagnostics) |
| Media dock, starred media, preview, playback and embedded metadata | [Media](MEDIA.md) |
| Library → Catalog / Installed, Scan Files; Backend → Provider authorization | [Library and authorization](LIBRARY.md) |
| Original Comfy editor and workflow import | [Comfy tab](COMFY.md) |
| Logs, source/level/search filters, copy/export | [Logs](LOGS.md) |
| Bug reports, feature requests, questions and documentation issues | [GitHub issue forms](LOGS.md#github-issue-forms) |

## News, history and attribution

[News](NEWS.md) · [Changelog](CHANGELOG.md) · [Credits and upstream notices](CREDITS.md)

## Architecture and contributor contracts

- [Plan](PLAN.md) — the single current status and open-work list
- [Product decisions](PRODUCT-DECISIONS.md) — owner-approved scope and architecture boundaries
- [Project rules, goals and dependencies](PROJECT-RULES.md)
- [ComfyUI integration/coexistence rules](COMFYUI-RULES.md)
- [Execution ownership and trust boundaries](EXECUTION-CONTRACT.md)
- [Development, builds and diagnostic API](DEVELOPMENT.md)
- [Windows/Linux path contracts, audit and tooling limits](PATH-COMPATIBILITY.md)
- [Bundled PromptForge data updates and weekly review PRs](FORGE-DATA-UPDATES.md)
- [Installer integration ledger](INSTALLER-INTEGRATION.md) and [native IPC reference](../src-tauri/README.md)
- [Download catalogs and plugin contract](DOWNLOAD-PLUGINS.md), [authentication contract](AUTH-PLUGIN.md)
- [Localization contract](LOCALIZATION.md) and [neural primitives](NEURAL-PRIMITIVES.md)
- [Verification journal and known limitations](VERIFICATION.md)
