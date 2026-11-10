# Contextual Chat

Text context from the current non-Chat workspace is captured at Send time, with a preview and Off switch. Images are explicit attachments. Full-page Chat retains the last area. Context excludes credentials, filesystem paths and workflow graphs; imported descriptions are reference data, never execution instructions. Each sent message retains its own snapshot.

Prompt and supported image-setting suggestions require review and explicit Apply. Stale setup changes block Apply/Undo; no generation starts from advice or context. A separate one-message authorization preserves explicit Chat generation. Native validation is authoritative.

Depends on workspace-docks. The optional shared selection feed allows independently adopted library/Forge modules to publish relevant state without a Chat import. Existing conversation records remain readable; context is optional. Tests: workspace_context, assistant, ContextualChat.test.tsx and Chat.test.tsx. No live paid calls are required for verification.
