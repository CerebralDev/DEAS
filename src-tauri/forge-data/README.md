# Bundled Prompt Forge data

Source: [PromptForge-Public](https://github.com/yoimtrashgaming-prog/PromptForge-Public/tree/29a37c4e43f144f6ff4c0bc8c02f96b440b64826), commit `29a37c4e43f144f6ff4c0bc8c02f96b440b64826`.

These files are embedded in the Linux and Windows executable. Discovery does not download or write anything. An existing same-named file in `<environment>/forge-data` (or the directory named by `DEAS_FORGE_DATA`) takes precedence. Missing files use the bundle; invalid or unreadable existing overrides remain errors, not silent replacement. `character-corrections.json` remains an optional local override; upstream does not ship this file.

| File | Upstream source | Preparation |
| --- | --- | --- |
| `forge-pack.json` | Prompt, registry, guidance and tool loaders | `node tools/export-forge-pack.mjs /path/to/PromptForge 29a37c4e43f144f6ff4c0bc8c02f96b440b64826`; JSON compacted without changing values |
| `randomizer.json` | `config/randomizer.json` | Unchanged |
| `tags.sqlite` | `data/seed/tags.sqlite` | SQLite backup, journal mode DELETE, integrity checked |
| `character_traits.sqlite` | `data/seed/character_traits.sqlite` | SQLite backup, journal mode DELETE, integrity checked |
| `character_series.json` | `data/seed/character_series.json` | Unchanged; zstd-compressed (Media Analysis' Describe cast list) |
| `analysis-describe.md` | `prompts/analysis/describe.md` | Unchanged; zstd-compressed so checkout line endings cannot alter it (Media Analysis' Describe instructions) |

The databases are opened read-only. Bundled databases are deserialized into memory: their upstream WAL headers were normalized through SQLite backup so they do not require writable sidecars. `MANIFEST.json` records source digests and the bundled payload digests. **The embedded payload is zstd-compressed** (`zstd -19`, deterministic: fixed level, no frame timestamp), decoded before the SQLite deserialize; `bytes`/`sha256` in the manifest describe the compressed payload, `upstreamSha256` the upstream file. A raw override file (or an uncompressed fixture) is read as-is, detected by the frame magic. Compression keeps the executable ~34 MB instead of ~78 MB. The export uses upstream's reduced H3 guide when its separately licensed documents are absent.

## Attribution and terms

PromptForge's prompts, configuration and native port originate from yoimtrashgaming-prog/PromptForge-Public, licensed GPL-3.0-or-later; see the repository-root [LICENSE](../../LICENSE). DEAS's maintainers approved GPL-3.0-or-later for DEAS and confirmed permission to redistribute the optional data in this integration. That confirmation is not a claim that all Danbooru content is GPL.

The tag list contains factual tag names, categories and counts from [Danbooru](https://danbooru.donmai.us/), as distributed by PromptForge. It contains no images. Upstream identifies its snapshot as 7 September 2026 and does not assign it a separate standard SPDX license. Redistribution here relies on the maintainer-confirmed permission, not a presumed public-domain grant.

Upstream credits the character table to [Laxhar/noob-wiki](https://huggingface.co/datasets/Laxhar/noob-wiki) (Apache-2.0) and [deepghs/character_index](https://huggingface.co/datasets/deepghs/character_index) (MIT). Their dataset-card license declarations were checked; no additional LICENSE/NOTICE files are present in their repository roots. Corresponding license texts and retained dataset cards are under `licenses/`. These datasets retain their own terms. Character/franchise names do not imply endorsement or ownership of the characters.

To reproduce the database conversion, open each pinned upstream database with Python's `sqlite3.connect('file:…?mode=ro&immutable=1', uri=True)`, back it up to a new destination with `source.backup(destination)`, set `PRAGMA journal_mode=DELETE`, and verify `PRAGMA integrity_check` returns `ok`. Never perform the conversion on a user's environment override.
