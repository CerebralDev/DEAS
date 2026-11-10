# Virtual model library

The Models/LoRAs dock and Installed Library share disk, automatic, and manual hierarchies. Organize resources with nested virtual folders, tags, descriptive types, pins, ordering, notes and saved views. Search covers names, tags, triggers and model/version IDs. Organization selection is separate from the active stack.

Native revision-checked atomic persistence uses the existing version 1 deas-model-library.json schema. Conflicts and unreadable data are preserved; Undo and unsaved-draft recovery protect edits. Deleting a virtual folder reparents memberships and children, never files. Technical compatibility and execution identifiers remain unchanged. Additional model roots are included.

This branch includes the small shared diagnostic/selection service, not Chat. Selected-file enrichment is a separate dependent module. Cached-thumbnail fields remain optional in the compatible store.

Tests: model_library Rust tests, ModelLibrary.test.tsx, focused installed-library/Workbench tests, and the mandatory layout and paired-output gates. Linux native runtime acceptance is separate from browser fixture evidence.
