# Media Analysis

**Studio → Media Analysis** reads what is in a picture: its booru **tags** from an image tagger, the **settings that made it**, and — on request — a **description** from your vision LLM. A video opens in the **frame picker**, where every frame can be stepped through, picked, saved, analysed or sent to Generate. Ported from PromptForge's Media Analysis tab.

Everything is read on this PC. Nothing is uploaded except the one picture Describe sends to the LLM server you configured.

## Opening a picture or video

- **Drop** a file on the picture area (PNG, JPEG, WebP; MP4, MOV, M4V, MKV, WebM).
- **Paste** a picture (Ctrl+V). Videos cannot be pasted; drop the file or use Choose….
- **Choose…** opens the system file dialog. The file is read where it is, never copied.
- **Media → right-click → Open in Media Analysis** on any picture or video.

## Tags

Taggers run on the **CPU** in a private analysis runtime (an isolated `uv` venv with hash-locked `onnxruntime`, `numpy` and `Pillow`, under `%LOCALAPPDATA%\deas\analysis-runtime` or `~/.local/share/deas/analysis-runtime`), so they never take GPU memory from ComfyUI or the LLM. Rust owns install, discovery and the process; `tagger_worker.py` reads one picture and exits.

### Which taggers are used

DEAS uses the taggers you already have, where they are. It never copies them and never downloads one unless you ask. It looks in every models folder (the shared one, the environment's own and any extra roots):

| Where | Example |
| --- | --- |
| `models/taggers/**` | DEAS's own folder; the optional download goes here |
| `models/onnx/**`, `models/animetimm/**` | animetimm, Hugging Face cache copies (`models--org--repo/snapshots/…`) |
| any `models/<folder>` named for a tagger | `wd14_tagger`, `joytag`, `camie`, `pixai`, `z3d`, `cl_tagger`, … |
| `ComfyUI/custom_nodes/<node>` named for a tagger | `comfyui-wd14-tagger/models`, a JoyTag or captioning node |

Any `.onnx` model with a tag list beside it is a tagger. The tag list is the first of `<model>.csv`, `selected_tags.csv`, `<model>.json`, `<model>_tags.json`, `<model>.txt`, then any CSV/JSON/text file whose name says tags, labels, vocab, metadata or mapping (a list that belongs to another `.onnx` in the same folder is never borrowed). Formats read:

- CSV — WD/animetimm `selected_tags.csv` (`tag_id,name,category,count`), any CSV with a `name`/`tag` header and optional `category`, `id,name[,category]` without a header, or bare names. Categories may be Danbooru numbers (0 general, 4 character, 9 rating) or words.
- JSON — a list of tags or of `{tag|name, category}` objects; an index map (`{"0": …}`); a tag map (`{tag: index}`); PixAI's `tag_map` + `tag_split`; Camie's `idx_to_tag` + `tag_to_category`; any of these under `tags`/`labels`/`names`.
- Text — one tag per line.

A list without categories is sorted with DEAS's bundled Danbooru tag table, so characters still show as characters; the tab says so under the tagger.

Something that looks like a tagger but cannot be used is **listed under "Found but cannot be used" with the reason**: PyTorch weights only (e.g. an animetimm folder with `pytorch_model.bin` and no `model.onnx`), an ONNX file in a tagger folder with no tag list beside it, or a tag list in a shape DEAS cannot read. Other ONNX models (a face detector in `models/onnx`) are not taggers and are not listed.

Model input is read from the model itself: NHWC models get WD's preprocessing (white square pad, BGR, 0–255); NCHW models with a `preprocess.json` get animetimm's timm steps; other NCHW models get a white square pad, RGB 0–1 and the mean/std named in a `config.json` beside them, else 0.5/0.5. The scores used are the output with one score per tag (the last such output, for models that return several).

> Verified on real files: WD v3 EVA02-Large and WD v1.4 MOAT in the WD14 Tagger node's folder. The other formats are covered by tests on their published file shapes; their preprocessing defaults have not been checked against real Camie/PixAI/JoyTag/cl_tagger models.

### Setup

- **A tagger was found:** *Set up taggers* installs only the runtime (about 300 MB). Nothing else is downloaded.
- **No tagger was found:** the tab says where it looked, and offers **Download WD v3 EVA02 (1.2 GB) and set up** — WD v3 EVA02-Large (Apache-2.0, not gated), pinned revision and SHA-256, into `models/taggers/wd-eva02-large-tagger-v3`. *Look again* re-scans after you move a tagger in.
- *Cancel* stops setup; nothing half-installed is kept. Errors go to **Logs**.

### Reading tags

Pick one tagger or **All taggers (merged)** (every tag any of them found, at its highest score; a tagger present in two folders is read once). Thresholds start at PromptForge's measured values — WD 0.35 general / 0.85 character, animetimm 0.38 / 0.51 — and can be changed per run. **Copy tags** and **Send to Generate** write them as a prompt (spaces instead of underscores unless switched off, brackets escaped).

## Made with, and Open in ComfyUI

**Made with** lists the generation settings embedded in the picture (ComfyUI, A1111/Forge, Swarm). **Open in ComfyUI** loads the picture's embedded ComfyUI workflow into the **Comfy** tab through DEAS's existing editor hand-off (the same one Media's metadata pane uses: a ready/load handshake with the owned ComfyUI's page). It is disabled with the reason when the picture carries no workflow, or when the backend is not running.

## Describe

**Describe** sends the picture and the tagger's tags to the configured LLM (Backend → LLM) with PromptForge's instructions (`prompts/analysis/describe.md`, bundled with the Forge data as `analysis-describe.md`). The answer has three parts:

- **Description** — the picture in words, as you would describe it to an artist.
- **Characters** — the booru tag of everyone it recognises.
- **Tagger check** — what the tagger *missed*, and what in its list is *doubtful*.

When the tagger recognised a character, the model is also given **that series' cast with how each usually looks** (from the bundled `character_series.json` and character table: characters with at least 50 posts, up to 25 per series, up to 14 looks each), so it can name the others by sight. The series always comes from a character the tagger is sure of; there is no search by looks.

Every tag it suggests is checked against the Danbooru tag table; tags Danbooru does not have are struck through and left out when you press **Add its tags to the tags above**.

Describe has its own model pick (it does not change the model Prompt Forge writes with). The list loads the first time you press Describe. A model that cannot see pictures is refused with a plain message: Ollama is asked up front (`/api/show` capabilities); an OpenAI-compatible server's refusal of the picture is translated; local GGUF models cannot read pictures. **Stop** ends it and keeps what was written.

## Frame picker

A video opens as a strip of every frame (every *k*-th past 2,000 frames; the arrows still step one at a time).

- **Frame-exact.** Frames come from FFmpeg **by number** (`select=eq(n,N)` over decoded frames), not by seeking to a time, so the frame you see is the frame you get — B-frames, variable rates and odd timestamps included. ffprobe counts the frames; one FFmpeg pass writes the thumbnails (cached for the five newest clips). FFmpeg with ffprobe must be on PATH.
- **Step** with ‹ › or ←/→ (Shift for 10), Home/End; click a thumbnail to go there.
- **Pick** with the button, Space, Ctrl-click or Shift-click (a range). **Save frame…** / **Save picked…** ask for a folder once and write `<video>-f0001.png` files; an existing file is never replaced.
- **Use this frame** (Enter) makes it the picture Media Analysis reads: tags, Made with (none for a frame) and Describe.
- **Send this frame to Generate** shows exactly what the Generate tab would do with a picture right now — for a Wan or H3 video model, *Use as first frame*, *Use as last frame*, *Use as both*, and H3's *Add as reference*; blocked actions are greyed with Generate's reason. With no such model chosen in Generate, the picker says to choose one first.

Frames for analysis and sends are kept in the runtime's `frames` folder (the newest clips only). The video is never changed or uploaded.

## Files

| File | Role |
| --- | --- |
| `src-tauri/src/analysis.rs` | Runtime install, the optional download, staging, running the worker, commands |
| `src-tauri/src/analysis_taggers.rs` | Discovery and tag-list reading |
| `src-tauri/src/analysis_frames.rs` | The frame picker's FFmpeg reads and saves |
| `src-tauri/src/analysis_describe.rs` | Describe: cast list, the LLM call, reading the answer |
| `src-tauri/src/tagger_worker.py` | One picture through one ONNX tagger |
| `src/features/analysis/` | The tab and the frame picker |

## Not yet

- Several picked frames cannot be sent to Generate in one click (PromptForge's "send picked" for H3 pictures); send them one at a time.
- Describe takes one picture; a frame range is not read as a clip.
