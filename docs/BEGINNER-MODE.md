# Beginner mode

Beginner mode is an overlay on the existing application, not a second program. The Workbench,
intents, Run pipeline, queue, previews and the owned backend stay exactly as they are; the overlay
reduces the visible surface and isolates technical settings behind the same state.

## Switching

- At first launch DEAS asks **You are… A beginner / An advanced user**. The choice is stored once
  in the global preference `interface.experience` and is remembered across restarts. With no installed
  environment yet, choosing Beginner lands on the Engine wizard first; when the installation finishes,
  Generation opens as usual.
- Switch at any time with the **Beginner / Advanced** group at the top right. Switching never
  terminates a running generation.

## Beginner pages

The tab band offers **Generation**, **Models** and **Collection**. The **Engine** button opens the
unchanged Backend/setup screens when configuration is needed; installation confirmations and
prerequisites still apply.

- **Generation** — creation mode (Text to Image, Image to Image, Text to Video, Image to Video,
  limited to the selected model family), the selected model, **Format** and **Quality** selects
  offering exactly the shared aspect and resolution options of the advanced panel (values snap to
  the model's own grid), optional LoRAs, an optional starting image and the prompt. DEAS applies the model family's starting settings; unsupported or unknown
  families cannot silently invent a recipe and the Generate button explains why it is disabled.
  Music, video-to-video and reference/video modes remain in Advanced.
- **Models** — Installed / Explore / All views with Image/Video capability filters and local
  search. Cards show publisher illustrations and prompting guides for four curated families; tags
  fill the search field, they are not compatibility declarations. **Use this model** changes the
  existing Workbench selection and keeps the prompt. **Download this model** presents only the
  featured variant's verified ComfyUI file names over the existing native browse/plan/confirm
  flow; nothing downloads without the normal plan review, and encoders/VAEs remain separate
  dependencies.
- **Collection** — the native media browser; select a file to preview it, star it, or use an image
  as the starting image.

## Isolation rules

- Entering Beginner resets only the Beginner view: technical preferences are kept in a separate
  in-session Advanced draft and restored when switching back. Prompts and the chosen model are
  shared. Only authored intent (prompt, model, pipeline) persists from Beginner; automatic
  Beginner settings never overwrite persisted advanced defaults.
- Beginner keeps a model-bound LoRA draft in `beginner.loras`. A model change clears the Beginner
  LoRAs unless the saved draft belongs to that exact model. Only valid safetensors LoRAs whose
  declared family matches the model's LoRA family are offered; GGUF LoRAs remain unavailable.
- One global theme preference (`appearance.scheme`) drives both experiences; Sage is available as
  part of the shipped `classic` theme pack like every other scheme.
- Global search (Ctrl+K), the shared Logs service and Ctrl+Enter work in both experiences.

## What Beginner does not change

Advanced keeps the full Workbench: technical settings, negative prompts, projects, processing
tools, Monitoring, global search and every pipeline. The Beginner overlay adds no execution path:
Generate submits through the same native adapter and appears in the same queue and Run Plan.

[Model illustration and guide sources](BEGINNER-MODEL-SOURCES.md)
