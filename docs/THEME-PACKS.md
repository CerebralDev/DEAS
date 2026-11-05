# Drop-in theme packs

[Documentation index](README.md) · [Studio and Quick Settings](STUDIO.md#layout-last-used-state-and-quick-settings) · [Credits](CREDITS.md)

Theme packs extend DEAS's existing colour picker with local palettes and optional images. They do not change navigation, model recipes, downloads or job execution. The theme-only changes from `DEAS-0.2.13-THEME-PLUGIN-UPDATE.zip` are adapted to the current application; the separate Beginner experience and Windows executable are not imported.

## Install and refresh

1. Select your existing DEAS environment in Backend. No reinstall or backend restart is needed.
2. Open **Quick Settings → Open themes folder**. This explicitly creates/opens `<your-environment>/themes`.
3. Copy individual folders from the repository's `theme-packs/classic/themes` into that directory. For example: `<your-environment>/themes/sage/theme.json`. Do not overwrite an existing customized folder; back it up or give a copy a new unique ID.
4. Click **Refresh themes**, then choose a theme in Quick Settings. Refresh applies colour/image changes without rebuilding or restarting the backend. Restarting DEAS also scans the folder.

Nine palettes are supplied: Sage, Default, Liiti Pink, VOID (legacy ID `voud`), Alluring Abyss, Slimy Teal, Clawy Grey, Android White and Dragon's Hoard. Before an environment is selected, the compiled choices remain available. Once an environment is selected, pack folders own availability: deleting Android White and refreshing removes it from the menu. Only Default remains permanently available for recovery. The application does not automatically copy or overwrite packs in your environment.

French and Paris are optional separate packs in `theme-packs/optional/themes`; copy either in the same way. Neither is installed or selected automatically. Their artwork retains its source/license files.

French supplies a cheese cursor, baguette scrollbars, Eiffel scene, mouse game and wine progress. The first mouse visit is scheduled after 45–105 seconds of eligible idle use, with later cooldowns of 90–180 seconds. Typing, dragging, dialogs, unfocused/hidden windows and reduced-motion preferences pause or suppress activity. Four rapid door clicks invite the mouse; evade for ten seconds or press Escape. The real pointer stays functional. Wine progress uses actual retained job progress, not a timer. Paris supplies a dark palette, lamps, moon and Eiffel scene.

## Offline editor

Every supplied pack contains `theme.json`, `assets/`, `EDIT-THEME.html` and `README.md`. Optional packs also retain attribution/license files. Images are optional; the original flat PNG layout remains supported.

1. Open the pack's **EDIT-THEME.html** deliberately in Chrome or Edge, or use the shared `theme-packs/EDIT-THEMES.html`. It works offline; DEAS never executes this HTML.
2. Click **Choose theme folder** and select the folder containing `theme.json`. Browser permission grants access to that chosen folder, not an upload.
3. Edit the name, colours and optional PNG/static WebP images. The editor has English labels; More colors exposes advanced tokens. Duplicate a pack first and assign a new unique Theme ID to create a separate theme.
4. Click **Save changes**, then **Refresh themes** in DEAS. Preserve attribution when sharing the complete folder.

The editor backs up the previous manifest and any overwritten images under `backups/`. Undo discards unsaved edits only. Restore by copying the backed-up manifest and any replaced images back to their recorded locations. It checks for external manifest changes before saving and again immediately before publishing. The browser API cannot make that last check/write atomic across processes: do not edit the same pack simultaneously in another program. A failed save may leave a newly copied image; keep backups until recovery is complete. DEAS loads only referenced images.

The preview demonstrates colours/background/cursor, not the full application. Always verify in DEAS. Browsers without directory access offer JSON download for colour-only changes; replace `theme.json` manually. Image replacement requires directory access in Chrome/Edge. Actual browser folder-permission and native desktop click-through remain separate acceptance gates.

## Data format (schema version 1)

```json
{
  "schemaVersion": 1,
  "id": "my-theme",
  "name": "My theme",
  "author": "Your name",
  "base": "default",
  "colors": {"panel": "#1c241e", "ok": "#c7e9a7"},
  "assets": {"background": "assets/background.webp", "cursor": "assets/cursor.png"}
}
```

`id` is a unique lowercase slug of letters, digits and hyphens (up to 64 characters); name and author are required bounded text. `base` selects one of the nine compiled scheme IDs, supplying omitted tokens. `colors` accepts supported colour tokens as hex RGB/RRGGBB/RRGGBBAA; the host derives RGB-channel tokens. The authoritative lists and limits live in `src-tauri/src/themes.rs`. The supplied manifests contain the exported palettes.

Generic asset roles are `background`, `cursor` and `scrollbar`. Cursors use a 3×4 hotspot (32×32 PNG recommended). Existing French/Paris presentations use additional named artwork roles. Assets may be flat filenames or one level under exactly `assets/`; deeper nesting, backslashes, traversal, URLs, drive syntax, symlinks and Windows reparse points are rejected. Names use lowercase letters, digits and hyphens followed by `.png` or `.webp`. SVG, HTML, CSS and JavaScript are not runtime pack inputs.

Optional `presentation` is `french` or `paris`; optional boolean effects `mouse` and `wine` require the French presentation and their assets. Effects are reviewed host components, not arbitrary executable plugins. New effect types require an application update.

## Recovery, safety and diagnostics

Discovery reads only the selected environment's `themes/` directory and creates nothing. A missing directory yields an empty external catalog. Invalid packs are skipped independently; duplicate IDs are rejected together rather than selecting a winner by directory order. One external pack may reuse a compiled ID without duplicating the menu entry.

The existing `appearance.scheme` preference and IDs remain stable. A missing/rejected selected pack falls back to Default without erasing the saved ID; restoring the folder restores the selection unless another theme was explicitly chosen. Switching clears old colour/image overrides, effects, listeners and timers. Late scans from a previous environment cannot replace the current catalog.

Local-window-only commands `native_theme_packs(root)` and `native_theme_folder(root)` use the `allow-native-theme-packs` permission. The root must be an existing absolute environment containing `deas-install.json`. Manifests reject unknown fields and are limited to 64 KiB. Discovery considers at most 128 folder entries, 64 colour overrides and 16 assets per pack. PNG/static WebP assets are decoded and validated: at most 2 MiB per file, 4096 pixels per dimension, 16 MiB decoded, and a 16 MiB compressed-image catalog budget.

Invalid scans and folder-open failures report fixed, redacted, rate-limited messages to central Logs. Discovery/selection do not start backends, installations, downloads or jobs. Only the explicit folder action starts the OS folder opener (Explorer or `xdg-open`).

## Verification boundaries

See the current [verification journal](VERIFICATION.md) for tests actually run on this adaptation. Source ZIP logs describe the separate older build, not acceptance of this checkout. Rust filesystem tests, component tests and simulated-IPC browser checks do not prove Windows WebView2, Linux GTK/WebKitGTK click-through, real folder permissions or GPU execution.

Pack content integrity is recorded in [`theme-packs/FILES.json`](../theme-packs/FILES.json); attribution and decoder notices are linked from [Credits](CREDITS.md).
