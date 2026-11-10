# Official OpenAI provider

Local extension of DEAS 0.2.22, November 9, 2026. This is not an upstream release.

Choose **Backend → LLM backend → Provider → OpenAI API (official)**. The endpoint is fixed to `https://api.openai.com/v1`. Enter your OpenAI API key, save and refresh models. Saving/refreshing only lists models; **Test request**, drafting and Describe incur API usage. Your existing compatible-server settings are not automatically migrated. A key already saved for the official URL is retained when switching to the official provider; keys for other hosts are not copied.

## Recommended models

Prompt Forge, its Studio popup, people detection, Media Analysis, Chat and the connection test put discovered recommendations in a **Recommended for prompts & images** group above the rest of the catalog:

- **GPT-6.1 Terra**: recommended default for prompt writing, visual review and people detection.
- **GPT-6.1 Sol**: optional choice for more complex prompts and deeper reasoning.
- **GPT-6 Luna**: economical option for simple descriptions.

These are curated suggestions, dated November 9, 2026, based on the [official model catalog](https://developers.openai.com/api/docs/models) and [Terra documentation](https://developers.openai.com/api/docs/models/gpt-6.1-terra), not measured DEAS quality rankings. Only models actually returned by discovery are shown. With no remembered usable choice, the shared selector prefers the first available recommendation; an explicit usable choice stays selected. Refreshing the connection-test list also preserves its selection. Other models remain available below; no inference or fallback request runs when selecting or refreshing. Existing compatible OpenAI model identifiers receive the same labels; local and Ollama names are unchanged.

The separate **OpenAI compatible** option keeps its existing Chat Completions behavior for llama.cpp, LM Studio and other compatible servers. Ollama and local GGUF behavior is unchanged.

## Supported paths

- Backend connection test, native Prompt Forge, image description/review, reference readback and assistant tools use the Responses API.
- H3 Prompt Forge, cast/people review and the node-pack prompt writer retain the installed DaSiWa node pack's image preparation, prompt templates and validation. A short-lived authenticated loopback adapter translates its requests to Responses. It requires Comfy on the same machine; remote Comfy receives an explicit limitation instead of a cloud key.
- Images are prepared data URLs. Requests preserve prompt text and reference contents, set `store:false`, and omit local-server-only parameters. Local temperature/top-p/repetition controls do not affect this provider. GPT-5/GPT-6 and o3/o4 families request low reasoning, with at least 4,096 total output tokens (including reasoning); the upper bound is 32,768. Pro models use their API default reasoning instead. Legacy non-reasoning models omit reasoning settings. The native official-provider validator accepts 16–32,768 tokens, fixing the initial Test request rejection at 4,096; legacy providers retain their existing limits.
- Tools use Responses function-call items and replies. No generation tools are added. The existing explicit execution boundaries remain in force.
- Model discovery uses your account's `/v1/models` list filtered for text/vision candidates. A listed identifier is not proof of model access or vision compatibility; provider rejections remain visible and never cause a model substitution.
- Responses are buffered until completion. Stop cancels local waiting/transport; it cannot undo a cloud request already accepted or guarantee no billing. There is no automatic network retry or paid fallback. H3 retains its existing multi-stage observation, writing and bounded structural-repair calls, so one Forge action can make several API requests.

## Troubleshooting and privacy

HTTP 400 shows the provider's error code/parameter; 401 identifies a key rejection; 403/404 indicates access/model availability; 429 identifies rate limits/quota; 5xx identifies provider failure. Raw provider error messages are not logged because they can echo prompts or credentials. Incomplete, refused and empty replies fail instead of becoming usable prompts. Existing prompt text remains for review. Timeout/interrupted requests have uncertain acceptance; retry only deliberately.

The cloud key uses the existing bounded environment LLM configuration storage. It is not added to presets, browser URLs or logs. H3's node pack receives a disposable loopback token, not the OpenAI key. This feature does not change OpenAI's account retention policies; `store:false` disables response storage, not all provider processing.

## Implementation and verification

`src-tauri/src/openai_api.rs` is the bounded Responses adapter and per-operation bridge. `llm.rs`, `forge/chat.rs` and the shared LLM selector integrate it. No installed node-pack files are patched. Existing serialized `openai` configurations remain compatible; the new provider is `openaiapi`.

Windows and Linux use the same Tokio/reqwest transport. HTTP/component fixtures cover payloads, images, tool turns, endpoint/key boundaries, incomplete/refused replies, bridge authentication/lifetime and redacted errors. See [verification journal](VERIFICATION.md) for actual results. The owner reported successful Prompt Forge and input-image people detection with this provider. Agent-run paid inference, comparative model quality/performance and Linux native UI acceptance remain unverified.

Sources: [OpenAI Responses/text guide](https://developers.openai.com/api/docs/guides/text), [image inputs](https://developers.openai.com/api/docs/guides/images-vision), [reasoning parameter compatibility](https://developers.openai.com/api/docs/guides/latest-model).

Rollback: reopen the retained original executable and choose OpenAI compatible or Ollama. If the original executable cannot read the new provider ID, restore the backed-up `deas-llm.json` only; preserve newer images, drafts, history and presets.
