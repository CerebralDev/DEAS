# Chat

**Studio → Chat** is a conversation with the LLM set under **Backend → LLM backend**, ported from PromptForge's chat. It knows this project: it writes prompts with the requested model’s own Prompt Forge writer, makes pictures with Studio’s Generate, reads how a model (or DEAS itself) wants to be used, looks tags up, and searches the web when you switch that on.

## Requirements

- An Ollama or OpenAI-compatible LLM server under **Backend → LLM backend** (llama.cpp, llama-swap, vLLM, Ollama). The local GGUF runner cannot call tools yet, so Chat says so instead of answering.
- Prompt Forge data is included in the executable, including the chat prompt, model guides and tag database. Existing files in `<environment>/forge-data` (or `DEAS_FORGE_DATA`) override the bundled defaults. No separate download or running ComfyUI backend is required to talk or write the four native image prompts; making a picture, and writing for the other image families, needs the backend running (see [Making pictures](#making-pictures)). See [Prompting](PROMPTING.md#prompt-forge).
- The model is picked at the top of the tab, from the same list as Prompt Forge.

## Using it

- Type and press **Enter** to send (**Shift+Enter** for a new line). Answers appear as they are written; **Stop** ends one early and keeps what arrived.
- **Pictures:** **Picture** or paste an image (up to four per message, PNG/JPEG/WebP, large ones scaled down). They go to the model with that message only and are never saved; the history notes that pictures were sent. The model has to be able to see pictures.
- **Include what I'm working on:** sends Studio's current prompt, negative prompt, model, size and sampler settings, and your Prompt Forge draft with each message, so you can ask about them directly ("why does this prompt keep giving two people?").
- **New chat**, and the list of conversations on the left: open, rename (✎) or delete (×, then **Delete?** to confirm).

## What it can do on its own

The model decides when to use these; what it used is listed under its answer.

| Tool | What it does |
| --- | --- |
| `render_picture` | Makes a picture: writes the prompt with the Prompt Forge writer of the checkpoint selected in **Studio → Generate**, then draws it there (see [Making pictures](#making-pictures)). Offered only while Generate can draw; otherwise the model is told why and says so. |
| `write_prompt` | Writes a prompt for the specified generation model with that model’s own writer, the same engine as the **Prompt Forge** tab with its default settings: the native writer (character canon, tag tools, clean-up) for **Illustrious, Anima, Krea2 and MOSS**, the node pack’s writer for the other image families (needs the backend running). The result appears verbatim as a card with **Copy** and **Send to Generate**. **Video models** (MiniMax H3, Wan, LTX) are pointed to **Studio → Prompt Forge → Write for**, because their shots, timing and references are set there. A model without a Forge mode can use its registered simple writer; if neither exists, Chat says no compatible prompt writer is available and never substitutes another model. |
| `read_guide` | Reads model prompt-writing instructions, the MiniMax H3/Wan/LTX guides and bundled DEAS user guides, including Music and App updates. Each call reads at most 16,000 Unicode characters. Optional `page` is a 1-based positive integer (default 1); the response gives page count, next-page instructions and an end marker. Later sections are reachable without dumping the whole document into a small model’s context. `deas-tve` explicitly uses the Studio guide, where TVE is documented; `deas-music` and `deas-app-updates` read their own guides. Native validation rejects invalid or out-of-range pages. |
| `search_tags` | Looks tags up in the local Danbooru tag database: whether a tag exists, its spelling and how common it is. |

## Making pictures

Ask for a picture ("draw a fox in the snow", "make that 16:9", "render that again") and Chat draws it with **Studio → Generate**:

- **What draws it:** the checkpoint selected in Generate, with Generate’s sampler, steps, CFG, seed mode, LoRAs and options. The prompt is written for that checkpoint’s family. If you name a different model, Chat tells you to select one of that model’s checkpoints in Generate instead of drawing with the wrong one.
- **Your Generate fields are not touched.** Chat’s prompt and size go into that one job only. Its negative is the writer’s, or Generate’s own negative when the writer gives none.
- **Changing it:** say what to change ("make the highlights teal", "now have her sitting on the bed"). Chat writes the whole picture again with that change and draws it, as PromptForge’s chat does: earlier pictures go back to the model as the tool call and its result, so it builds the new description from the last one.
- **Size:** say a ratio (`16:9`, `2:3`, `square`), a short side (`1080p`, `720`) or a size (`1920x1080`); Chat snaps it to the model’s grid. A later picture that names no size keeps the last size asked for in that chat; with none ever asked, Generate’s size is used.
- **Same prompt:** "render that again" draws the last prompt Chat wrote for that family without rewriting it. **Draw again** under a picture does the same with Generate’s current settings.
- The picture appears under its prompt with its size, checkpoint, seed and time, and is saved in the output folder like any Generate result, so it is also in **Media**. Progress and the sampler preview show while it draws; **Stop** stops the job.
- It needs: the DEAS desktop app, the backend running, an image checkpoint selected, and Generate on **text-to-image**. When one is missing, Chat says which and what to do, and can still write the prompt.

## Web search

Off by default: Chat does not go online unless you switch it on. **Web search** at the top of the tab chooses how:

- **DuckDuckGo** needs nothing installed. Your message is sent to DuckDuckGo and its plain results page is read, which means the query leaves this PC and the page format can change.
- **SearXNG** sends the search to your own SearXNG server (its address, e.g. `http://127.0.0.1:8080`), which proxies it. SearXNG must be running and allow JSON results (`search.formats` including `json` in its `settings.yml`).

With a search mode chosen, the **Search** button appears beside the message box. While it is on, your message is searched before the answer and the answer cites the results as [1], [2] with the sources listed under it. The setting is stored in `<environment>/deas-chat.json`.

## Where things are kept

- Conversations: `<environment>/chats/<id>.json`, one file each, newest first (at most 500 listed). Pictures you send are never written; pictures Chat makes are saved by Generate in the output folder, and the conversation keeps their path.
- The chat system prompt and guides come from PromptForge's own files, exported into the data pack by `tools/export-forge-pack.mjs`.

## Limits

- Chat allows four tool rounds per answer. Long guides may need a follow-up message; the model is instructed to disclose unread pages rather than invent their content. Pagination makes the full text reachable, not a guarantee that every LLM will request the relevant pages.

- Chat makes pictures with text-to-image only. Picture editing (change this picture) and video are not in Chat yet; use Generate’s image-to-image and the Prompt Forge tab’s video modes.
- One picture per answer.
- The family plugin currently provides Prompt Forge modes for **Illustrious/SDXL/Pony, Anima, Krea2, MOSS, MiniMax H3, Wan and LTX**. Chat writes the image models’ prompts itself and points video requests to their Forge mode; it never asks you to choose one of the four image writers instead of the requested video model.
- A guide alone does not establish a compatible simple writer. Explanation-only guides and DEAS user guides never become prompt writers. Unsupported models are refused rather than written from the chat model’s memory or mapped to an unrelated model. The bundled writers currently all have Forge modes; the simple fallback is exercised with a registered-writer fixture, not advertised as extra bundled model support.
- The LLM must call `write_prompt` for native routing to enforce this policy. A model that ignores tool instructions can still answer in prose; the system prompt explicitly forbids inventing a writer or substituting another model.
- Small models may skip a tool and answer from memory; ask again ("read the LTX guide first") if an answer looks invented.

## Troubleshooting

- *"Chat needs an LLM server…"*: set an Ollama or OpenAI-compatible server under **Backend → LLM backend**.
- *"The LLM took too long to answer"*: the server may be loading the model or busy with another job; try again.
- Search says *"refused the JSON request (403)"*: enable JSON in SearXNG's `settings.yml`.
- *"Not drawn: The backend is not running…"* (or no checkpoint, or not text-to-image): fix what it names in Studio → Generate, then **Draw again** or ask again.
- *"Generate could not start this picture"*: Generate’s own message says why (shown in Studio → Generate and in **Logs**).
- Errors are also written to **Logs**.
