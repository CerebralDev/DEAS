# Chat

**Studio → Chat** is a conversation with the LLM set under **Backend → LLM backend**, ported from PromptForge's chat. It knows this project: it routes prompt requests to the requested model’s compatible writer, reads how a model (or DEAS itself) wants to be used, looks tags up, and searches the web when you switch that on.

## Requirements

- An Ollama or OpenAI-compatible LLM server under **Backend → LLM backend** (llama.cpp, llama-swap, vLLM, Ollama). The local GGUF runner cannot call tools yet, so Chat says so instead of answering.
- Prompt Forge data is included in the executable, including the chat prompt, model guides and tag database. Existing files in `<environment>/forge-data` (or `DEAS_FORGE_DATA`) override the bundled defaults. No separate download or running ComfyUI backend is required for Chat. See [Prompting](PROMPTING.md#prompt-forge).
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
| `write_prompt` | Routes every prompt-writing or revision request for the specified generation model: **Prompt Forge mode first → compatible simple writer → refusal**. A Forge route gives the exact **Studio → Prompt Forge → Write for** selection; it does not change tabs, replace drafts, run a writer or select another model automatically. Only a model without a Forge mode can use its registered simple writer with a nonempty system prompt and supported output contract in its effective default mode; its result appears verbatim as a card with **Copy** and **Send to Generate**. If neither exists, Chat says no compatible prompt writer is available. |
| `read_guide` | Reads model prompt-writing instructions, the MiniMax H3/Wan/LTX guides and bundled DEAS user guides, including Music and App updates. Each call reads at most 16,000 Unicode characters. Optional `page` is a 1-based positive integer (default 1); the response gives page count, next-page instructions and an end marker. Later sections are reachable without dumping the whole document into a small model’s context. `deas-tve` explicitly uses the Studio guide, where TVE is documented; `deas-music` and `deas-app-updates` read their own guides. Native validation rejects invalid or out-of-range pages. |
| `search_tags` | Looks tags up in the local Danbooru tag database: whether a tag exists, its spelling and how common it is. |

## Web search

Off by default: Chat does not go online unless you switch it on. **Web search** at the top of the tab chooses how:

- **DuckDuckGo** needs nothing installed. Your message is sent to DuckDuckGo and its plain results page is read, which means the query leaves this PC and the page format can change.
- **SearXNG** sends the search to your own SearXNG server (its address, e.g. `http://127.0.0.1:8080`), which proxies it. SearXNG must be running and allow JSON results (`search.formats` including `json` in its `settings.yml`).

With a search mode chosen, the **Search** button appears beside the message box. While it is on, your message is searched before the answer and the answer cites the results as [1], [2] with the sources listed under it. The setting is stored in `<environment>/deas-chat.json`.

## Where things are kept

- Conversations: `<environment>/chats/<id>.json`, one file each, newest first (at most 500 listed). Pictures are never written.
- The chat system prompt and guides come from PromptForge's own files, exported into the data pack by `tools/export-forge-pack.mjs`.

## Limits

- Chat allows four tool rounds per answer. Long guides may need a follow-up message; the model is instructed to disclose unread pages rather than invent their content. Pagination makes the full text reachable, not a guarantee that every LLM will request the relevant pages.

- No rendering or picture editing from chat; make pictures in Studio's Generate (send a prompt there with **Send to Generate**).
- The family plugin currently provides Prompt Forge modes for **Illustrious/SDXL/Pony, Anima, Krea2, MOSS, MiniMax H3, Wan and LTX**. Chat directs writing requests to those modes, including image models; it no longer asks you to choose one of the four image writers instead of the requested video model.
- A guide alone does not establish a compatible simple writer. Explanation-only guides and DEAS user guides never become prompt writers. Unsupported models are refused rather than written from the chat model’s memory or mapped to an unrelated model. The bundled writers currently all have Forge modes; the simple fallback is exercised with a registered-writer fixture, not advertised as extra bundled model support.
- The LLM must call `write_prompt` for native routing to enforce this policy. A model that ignores tool instructions can still answer in prose; the system prompt explicitly forbids inventing a writer or substituting another model.
- Small models may skip a tool and answer from memory; ask again ("read the LTX guide first") if an answer looks invented.

## Troubleshooting

- *"Chat needs an LLM server…"*: set an Ollama or OpenAI-compatible server under **Backend → LLM backend**.
- *"The LLM took too long to answer"*: the server may be loading the model or busy with another job; try again.
- Search says *"refused the JSON request (403)"*: enable JSON in SearXNG's `settings.yml`.
- Errors are also written to **Logs**.
