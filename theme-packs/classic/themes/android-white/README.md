# Android White — editing guide

1. Open **EDIT-THEME.html** in Microsoft Edge or Chrome. No installation or internet connection is needed.
2. Click **Choose theme folder**, then select this folder (the one containing theme.json). Grant the browser access to this folder when prompted.
3. Change colors with the color pickers. Under Images, choose a PNG or static WebP to add or replace a background, cursor or scrollbar. Existing special-theme artwork is listed there too.
4. Click **Save changes**. The editor places images in assets/ and updates theme.json. It preserves a copy of the previous manifest and replaced images in backups/.
5. In DEAS, click **Quick Settings → Refresh themes**. Choose the theme if it is not already selected. No restart is needed for later theme edits.

## Create your own theme

Duplicate this entire folder in Explorer first. Open the copy with the editor, change its name, and give it a unique Theme ID such as my-night-theme. Two installed folders with the same ID are ignored. The folder name alone does not change the ID. Keep credit and license files when reusing artwork.

## Files

- theme.json: theme name, ID, colors, image paths and optional effect switches. Prefer the editor if you are unfamiliar with JSON.
- assets/: images used by the theme; only referenced images are loaded. Not every theme needs a background or cursor.
- EDIT-THEME.html: the same offline editor supplied with every theme. It runs only when you open it in your browser; DEAS does not execute it.
- backups/: created by the editor on save. To restore, copy a saved JSON over theme.json and restore any replaced images to the paths named in that JSON.
- Attribution/license files: retain them when sharing the theme. SOURCES.json may describe original SVG sources; the installed visual assets are PNG renders of those originals.

PNG and static WebP are supported. Use a transparent 32 × 32 PNG for cursors. Each image must be at most 2 MiB, at most 4096 px per side, and at most 16 MiB decoded; all images together have a 16 MiB compressed budget. Names use lowercase letters, numbers and hyphens. The editor chooses compatible filenames automatically. Unknown files and backups are not scanned as themes.

## If something goes wrong

Use Undo unsaved changes before saving, or restore a backup after saving. If DEAS rejects a pack, check Logs and the image limits. Default remains available for recovery. Deleting this folder and clicking Refresh themes removes its menu item; restoring it restores the saved choice. Older themes with images beside theme.json continue to work.

In browsers without folder access, the editor offers a JSON download for color edits; replace theme.json yourself. Image replacement and direct saving require Edge or Chrome. The palette preview is a small sample, not an exact DEAS screenshot. French mouse/wine and Paris scenery use the existing host presentations; arbitrary new scripts are not theme assets.
