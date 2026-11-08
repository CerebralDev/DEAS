# Make and edit DEAS themes

[Documentation index](README.md) · [Studio and Quick Settings](STUDIO.md#layout-last-used-state-and-quick-settings) · [Credits](CREDITS.md)

A theme pack is a small local data folder: one validated `theme.json` manifest plus optional PNG or static WebP images. It cannot contain or execute JavaScript, HTML, CSS or programs. It may change the existing UI's colour tokens and background/cursor/scrollbar images; it does not change models, recipes, navigation, downloads or job execution.

The nine built-in schemes remain compiled into DEAS and are always available. External packs are only changes layered over a built-in `base`. The nine built-ins are therefore **not duplicated** as supplied external packs.

## Quick Settings flow

Open **Quick Settings → Edit themes**.

```mermaid
flowchart LR
  A["Quick Settings → Edit themes"] --> B{"Is there a themes/new-theme folder?"}
  B -- "No" --> C["Copy the bundled starter theme"]
  B -- "Yes" --> D["Keep the existing folder and edits"]
  C --> E["Open the theme editor inside DEAS"]
  D --> E
  E --> F["Choose one of your environment's themes"]
  F --> G["Give it a unique Theme ID"]
  G --> H["Change colors or images"]
  H --> I["Save theme.json and assets"]
  I --> J["Quick Settings → Refresh themes"]
  J --> K["Choose the theme in Colour scheme"]
```

The editor runs inside DEAS in a frame of its own and only talks to the app for the manifest, images and backups of the one theme folder you selected. The same file still opens in Chrome or Edge if you prefer to edit a folder through the browser's own directory permission.

## Create your first theme

1. Select your DEAS environment in **Backend**.
2. Click **Quick Settings → Edit themes**.
3. DEAS creates `themes/new-theme` in the selected environment only when absent and opens the editor.
4. Pick that `new-theme` in the **Edit theme** list; it lists only the theme folders of the selected environment. Nothing is uploaded.
5. Replace its Theme ID with a unique lowercase value, for example `my-space-blue`. `new-theme` is only the initial template.
6. Pick a built-in `base` and change only the colours you want. Omitted colours inherit the base palette.
7. Optionally replace images through the image rows, or drag a PNG/WebP onto a row.
8. Click **Save changes**.
9. DEAS reloads the theme list after each save. If needed, click **Quick Settings → Refresh themes**, then select the theme under **Colour scheme**.

Deleting a custom theme folder and refreshing removes only its entry. A saved theme that cannot be loaded falls back to Default without erasing the saved selection.

## Make another theme or a shareable pack

Duplicate an existing folder inside `<environment>/themes/`, then give the duplicate a unique Theme ID. Sharing means sending the complete theme folder. Keep any third-party artwork's attribution/license files beside it.

A minimal manual theme is valid without images:

```json
{
  "schemaVersion": 1,
  "id": "my-space-blue",
  "name": "My Space Blue",
  "author": "Your name",
  "base": "sage",
  "colors": {
    "deep": "#070b18",
    "panel": "#141b2c",
    "chalk": "#f0f5ff",
    "blue": "#91b8ff",
    "ok": "#8fd7a3",
    "on-ok": "#102018",
    "line": "#39445c"
  }
}
```

Folder layout:

```text
my-space-blue/
├── theme.json
└── assets/
    ├── background.webp
    └── cursor.png
```

`theme.json` references them as `assets/background.webp` and `assets/cursor.png`. The older layout may place the PNG/WebP directly in the theme folder instead.

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

- `id`: unique lowercase letters, digits and hyphens, at most 64 characters.
- `name` and `author`: printable text, at most 120 characters.
- `base`: one built-in scheme ID—`default`, `sage`, `liiti-pink`, `voud`, `alluring-abyss`, `slimy-teal`, `clawy-grey`, `android-white` or `dragons-hoard`.
- `colors`: supported colour tokens, using `#RGB`, `#RRGGBB` or `#RRGGBBAA`.
- `assets`: optional `background`, `cursor` and `scrollbar` roles.
- Asset filenames are lowercase letters, digits and hyphens with `.png` or `.webp`, either in the theme folder or one level under `assets/`.
- Cursors use a 3×4 hotspot; 32×32 transparent PNG is recommended.
- A pack may contain at most 16 assets. Each image may be at most 2 MiB, 4096 pixels per side and 16 MiB decoded. The whole scan has a 16 MiB compressed-image budget.
- `theme.json` is at most 64 KiB and unknown fields are rejected.

The authoritative limits live in `src-tauri/src/themes.rs`.

## Offline editor details

The canonical editor is:

```text
theme-packs/EDIT-THEMES.html
```

It is bundled with the application. Quick Settings opens it; users do not need to copy a separate per-theme editor.

What the editor does:

- Inside DEAS it lists the selected environment's theme folders; every read and write goes through DEAS and stays inside that one folder. Opened directly in a browser it uses that browser's directory permission instead.
- Edits identity, colours and images in memory.
- Writes new selected images under `assets/` with fresh filenames.
- Backs up the previous manifest and replaced images in `backups/`.
- Refuses external edits made to `theme.json` after the folder was opened.
- Offers JSON download when the browser lacks directory access; image replacement then remains disabled.

For the direct browser route Chrome and Edge are the supported choices; Firefox does not provide the required directory-editing API. The editor is English-only and has no network code.

## Recovery and safety

Before saving:

- The editor creates `backups/theme-<timestamp>.json`.
- A replaced image is copied into `backups/` under its recorded name.

If DEAS shows a bad result, choose **Default**, restore the backup files into the theme folder, and refresh. An external pack that reuses a built-in ID overrides only that entry in that environment; deleting it restores the built-in behavior.

DEAS rejects parent traversal, absolute paths, URLs, drive syntax, backslashes, symlinked or reparse-point theme paths, malformed JSON and oversize images/manifests. Invalid packs are skipped independently. Duplicate IDs are rejected together instead of choosing one by directory order.

Native commands are `native_theme_packs(root)`, `native_theme_editor(app, root)`, `native_theme_folders(root)`, `native_theme_read(root, folder, path)` and `native_theme_write(root, folder, path, base64)`, all behind `allow-native-theme-packs`. The selected root must be an existing absolute environment. Edit themes creates the environment's `themes/` directory and copies the bundled `new-theme` starter only if that starter folder is absent. It never overwrites an existing `new-theme` folder and never copies the nine built-in schemes. The file commands accept only `theme.json`, an image under `assets/` or a backup under `backups/` inside one named theme folder, re-validate the manifest or image bytes before replacing, and swap files atomically.

## Verification boundaries

Rust filesystem tests, React component tests and the Chromium editor tools verify catalog parsing, no-clobber starter copying, traversal/link rejection, caps, editor save races and the framed editor's bridge against a host fixture. They do not prove the framed editor renders in the shipped WebKitGTK/WebView2 window or the standalone browser's directory permission dialog. Those native acceptance gates remain open in [PLAN.md](PLAN.md).

Pack integrity is recorded in [`theme-packs/FILES.json`](../theme-packs/FILES.json).
