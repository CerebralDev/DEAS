# DEAS theme editor and starter theme

DEAS supplies one offline editor and one starter theme. Built-in colour schemes are not duplicated as external packs.

Open **Quick Settings → Edit themes** in DEAS. DEAS creates `new-theme` in the selected environment's `themes/` folder if it is missing and opens this editor in your browser.

Choose that folder, change its Theme ID to something unique, edit its colors, then click **Save changes**. Duplicate the folder first when you want to keep more than one custom theme. Return to DEAS and click **Refresh themes**.

The editor works offline, backs up previous manifests/images and places new images in `assets/` automatically. Keep attribution/license files when sharing reused artwork. Full examples, schema, limits and recovery notes are in [docs/THEME-PACKS.md](../docs/THEME-PACKS.md). DEAS never executes the HTML as a plugin.
