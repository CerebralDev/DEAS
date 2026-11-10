# Workspace docking

Chat supports full-page, left, right and bottom placement, collapse and keyboard/pointer resizing. One retained portal preserves the conversation, draft, attachments and accepted work while moving. Monitoring is a separate retained instance with compact five-second or expanded one-second sampling; hidden windows pause UI polling. Samples/errors remain explicit. Reset workspace layout changes only placement and sizes.

The shell owns the viewport. Narrow side docks become overlays. The small common diagnostics/selection service carries job progress without importing contextual Chat. Neither docking nor navigation starts generation. Preferences use the existing workspace.* keys.

Validation: Docking.test.tsx, Monitoring tests, mandatory six-language/seven-size browser gate with dock permutations, and paired-output pointer checks. Chromium is fixture coverage, not GTK/WebKit or native Windows runtime acceptance.
