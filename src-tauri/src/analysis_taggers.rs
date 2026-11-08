//! Media Analysis: finding the image taggers people already have, and reading their tag lists.
//!
//! Any ONNX tagger that comes with a tag list is used where it lies, never copied or
//! downloaded. Looked for where people keep them:
//! - `<models>/taggers`, `<models>/onnx`, `<models>/animetimm`, and any other folder in
//!   `<models>` whose name says it holds taggers (`wd14_tagger`, `joytag`, `camie`, ...);
//! - tagger nodes in `<ComfyUI>/custom_nodes` (the WD14 Tagger node's `models`, and any
//!   other node whose folder name says tagger or caption);
//! - Hugging Face cache layouts under those (`models--org--repo/snapshots/<rev>/`).
//!
//! Tag lists come in several shapes and all are read into one list of (tag, kind):
//! WD/animetimm `selected_tags.csv` (`tag_id,name,category,count`), other CSVs with or
//! without a header, JSON (a plain list, an index map, `tag_map` + `tag_split` as PixAI
//! ships it, Camie's `idx_to_tag` + `tag_to_category`, objects with `tag`/`category`)
//! and plain text, one tag per line. A list without categories is sorted with DEAS's own
//! Danbooru tag table, so characters still show as characters.
//!
//! Something that looks like a tagger but cannot be used (PyTorch weights only, no tag
//! list beside it, an unreadable list) is listed with the reason, never hidden.

use serde::Serialize;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};

#[derive(Clone, Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tagger {
    /// Stable id: the model path.
    pub id: String,
    pub name: String,
    pub model: String,
    pub labels: String,
    /// Where it was found, as people know the folder: `custom_nodes/comfyui-wd14-tagger`.
    pub place: String,
    /// How many tags its list holds.
    pub tags: usize,
    /// The list said which tags are characters and ratings; otherwise the tag table did.
    pub categories: bool,
    pub general_threshold: f64,
    pub character_threshold: f64,
}

/// A tagger that was found but cannot be used, with the reason in plain words.
#[derive(Clone, Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Unusable {
    pub name: String,
    pub place: String,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    General,
    Character,
    Rating,
    /// Artist, copyright, meta: scored by the model, never shown as tags.
    Other,
}

impl Kind {
    pub fn code(self) -> &'static str {
        match self {
            Kind::General => "g",
            Kind::Character => "c",
            Kind::Rating => "r",
            Kind::Other => "o",
        }
    }
}

/// A tag list as read: names in model output order, kinds when the file gave them.
#[derive(Clone, Debug, PartialEq)]
pub struct Labels {
    pub names: Vec<String>,
    pub kinds: Option<Vec<Kind>>,
}

const MAX_LABEL_FILE: u64 = 64 << 20;
const MAX_TAGS: usize = 200_000;

// ── tag lists ───────────────────────────────────────────────────────────────

fn kind_of(value: &str) -> Kind {
    let v = value.trim().trim_matches('"').to_ascii_lowercase();
    match v.as_str() {
        // Danbooru's numbers, as WD and animetimm write them; e621 shares 0 and 4.
        "0" | "general" | "tag" => Kind::General,
        "4" | "character" | "characters" => Kind::Character,
        "9" | "rating" | "ratings" => Kind::Rating,
        _ => Kind::Other,
    }
}

/// One CSV line, with quoted cells (`"a, b"`) kept whole.
fn csv_cells(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cell)),
            _ => cell.push(c),
        }
    }
    out.push(cell);
    out.into_iter().map(|c| c.trim().to_string()).collect()
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn read_csv(text: &str) -> Result<Labels, String> {
    let rows: Vec<Vec<String>> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(csv_cells)
        .collect();
    let Some(first) = rows.first() else {
        return Err("the tag list is empty".into());
    };
    let lower: Vec<String> = first.iter().map(|c| c.to_ascii_lowercase()).collect();
    let find = |names: &[&str]| lower.iter().position(|c| names.contains(&c.as_str()));
    let named = find(&["name", "tag", "tag_name", "label", "tags"]);
    let (body, name_col, kind_col) = if let Some(n) = named {
        (
            &rows[1..],
            n,
            find(&["category", "category_id", "type", "tag_category", "kind"]),
        )
    } else if first.len() >= 2 && is_number(&first[0]) && !is_number(&first[1]) {
        // `id,name[,category,...]` without a header.
        (&rows[..], 1, (first.len() >= 3).then_some(2))
    } else {
        (&rows[..], 0, (first.len() >= 2).then_some(1))
    };
    let mut names = Vec::with_capacity(body.len());
    let mut kinds = Vec::with_capacity(body.len());
    for row in body {
        let name = row.get(name_col).cloned().unwrap_or_default();
        if name.is_empty() {
            return Err("the tag list has a row without a tag".into());
        }
        names.push(name);
        if let Some(k) = kind_col {
            kinds.push(kind_of(row.get(k).map(String::as_str).unwrap_or("")));
        }
    }
    Ok(Labels {
        names,
        kinds: kind_col.map(|_| kinds),
    })
}

fn read_txt(text: &str) -> Result<Labels, String> {
    let names: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    Ok(Labels { names, kinds: None })
}

/// `{ "0": .., "1": .. }` or `{ tag: 0, .. }`: names put in index order; gaps refused.
fn by_index(pairs: Vec<(usize, String)>) -> Result<Vec<String>, String> {
    let mut slots: Vec<Option<String>> = vec![None; pairs.len()];
    for (i, name) in pairs {
        let slot = slots
            .get_mut(i)
            .ok_or("the tag list's numbering has gaps")?;
        if slot.replace(name).is_some() {
            return Err("the tag list numbers two tags the same".into());
        }
    }
    slots
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| "the tag list's numbering has gaps".into())
}

fn object_entry(v: &serde_json::Value) -> Option<(String, Option<Kind>)> {
    match v {
        serde_json::Value::String(s) => Some((s.clone(), None)),
        serde_json::Value::Object(o) => {
            let name = ["tag", "name", "label"]
                .iter()
                .find_map(|k| o.get(*k).and_then(|v| v.as_str()))?;
            let kind = ["category", "type", "kind"].iter().find_map(|k| {
                o.get(*k).map(|v| match v {
                    serde_json::Value::String(s) => kind_of(s),
                    other => kind_of(&other.to_string()),
                })
            });
            Some((name.to_string(), kind))
        }
        _ => None,
    }
}

fn from_entries(entries: Vec<(String, Option<Kind>)>) -> Labels {
    let has_kinds = entries.iter().all(|(_, k)| k.is_some()) && !entries.is_empty();
    let kinds = has_kinds.then(|| entries.iter().map(|(_, k)| k.unwrap()).collect());
    Labels {
        names: entries.into_iter().map(|(n, _)| n).collect(),
        kinds,
    }
}

fn read_json_value(v: &serde_json::Value, depth: usize) -> Result<Labels, String> {
    use serde_json::Value;
    let unreadable = || "the JSON tag list is in a shape DEAS does not know".to_string();
    if depth > 3 {
        return Err(unreadable());
    }
    match v {
        Value::Array(items) => {
            let entries: Option<Vec<_>> = items.iter().map(object_entry).collect();
            entries.map(from_entries).ok_or_else(unreadable)
        }
        Value::Object(o) => {
            // PixAI: tag -> index, and how many of them are general then character tags.
            if let Some(Value::Object(map)) = o.get("tag_map") {
                let pairs: Option<Vec<(usize, String)>> = map
                    .iter()
                    .map(|(t, i)| Some((i.as_u64()? as usize, t.clone())))
                    .collect();
                let names = by_index(pairs.ok_or_else(unreadable)?)?;
                let split = o.get("tag_split");
                let count = |k: &str| {
                    split
                        .and_then(|s| s.get(k))
                        .and_then(|v| v.as_u64())
                        .map(|n| n as usize)
                };
                let kinds = match (count("gen_tag_count"), count("character_tag_count")) {
                    (Some(g), Some(c)) if g + c == names.len() => Some(
                        (0..names.len())
                            .map(|i| if i < g { Kind::General } else { Kind::Character })
                            .collect(),
                    ),
                    _ => None,
                };
                return Ok(Labels { names, kinds });
            }
            // Camie: index -> tag and tag -> category, possibly under dataset_info.tag_mapping.
            let mapping = o
                .get("dataset_info")
                .and_then(|d| d.get("tag_mapping"))
                .unwrap_or(v);
            if let Some(Value::Object(idx)) = mapping.get("idx_to_tag") {
                let pairs: Option<Vec<(usize, String)>> = idx
                    .iter()
                    .map(|(i, t)| Some((i.parse().ok()?, t.as_str()?.to_string())))
                    .collect();
                let names = by_index(pairs.ok_or_else(unreadable)?)?;
                let kinds = mapping
                    .get("tag_to_category")
                    .and_then(|c| c.as_object())
                    .and_then(|c| {
                        names
                            .iter()
                            .map(|n| c.get(n).and_then(|k| k.as_str()).map(kind_of))
                            .collect::<Option<Vec<_>>>()
                    });
                return Ok(Labels { names, kinds });
            }
            for key in ["tags", "labels", "names", "tag_names", "classes"] {
                if let Some(inner) = o.get(key) {
                    return read_json_value(inner, depth + 1);
                }
            }
            // An index map ("0": tag) or a tag map (tag: 0).
            if !o.is_empty() && o.keys().all(|k| is_number(k)) {
                let mut entries = vec![];
                for (i, val) in o {
                    let (name, kind) = object_entry(val).ok_or_else(unreadable)?;
                    entries.push((i.parse::<usize>().map_err(|_| unreadable())?, name, kind));
                }
                let kinds: Option<Vec<(usize, Kind)>> =
                    entries.iter().map(|(i, _, k)| Some((*i, (*k)?))).collect();
                let names = by_index(entries.into_iter().map(|(i, n, _)| (i, n)).collect())?;
                let kinds = kinds.map(|mut k| {
                    k.sort_by_key(|(i, _)| *i);
                    k.into_iter().map(|(_, k)| k).collect()
                });
                return Ok(Labels { names, kinds });
            }
            if !o.is_empty() && o.values().all(|v| v.is_u64()) {
                let pairs = o
                    .iter()
                    .map(|(t, i)| (i.as_u64().unwrap() as usize, t.clone()))
                    .collect();
                return Ok(Labels {
                    names: by_index(pairs)?,
                    kinds: None,
                });
            }
            Err(unreadable())
        }
        _ => Err(unreadable()),
    }
}

/// A tag list file, whichever of the known shapes it is in.
pub fn read_labels(path: &Path) -> Result<Labels, String> {
    let size = fs::metadata(path)
        .map_err(|_| "the tag list cannot be read".to_string())?
        .len();
    if size > MAX_LABEL_FILE {
        return Err("the tag list is over 64 MB".into());
    }
    let bytes = fs::read(path).map_err(|_| "the tag list cannot be read".to_string())?;
    let text = String::from_utf8(bytes).map_err(|_| "the tag list is not UTF-8 text".to_string())?;
    let text = text.trim_start_matches('\u{feff}');
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let labels = match ext.as_str() {
        "csv" => read_csv(text)?,
        "json" => {
            let v: serde_json::Value = serde_json::from_str(text)
                .map_err(|_| "the JSON tag list does not parse".to_string())?;
            read_json_value(&v, 0)?
        }
        "txt" => read_txt(text)?,
        _ => return Err("the tag list is not CSV, JSON or text".into()),
    };
    if labels.names.is_empty() {
        return Err("the tag list is empty".into());
    }
    if labels.names.len() > MAX_TAGS {
        return Err("the tag list has over 200,000 tags".into());
    }
    Ok(labels)
}

/// Kinds for every tag: the list's own, else DEAS's Danbooru table (characters as
/// characters, artists and series left out), with the four ratings recognised by name.
pub fn resolve_kinds(labels: &Labels, store: Option<&crate::forge::tags::TagStore>) -> Vec<Kind> {
    if let Some(kinds) = &labels.kinds {
        return kinds.clone();
    }
    labels
        .names
        .iter()
        .map(|name| {
            let bare = name.strip_prefix("rating:").unwrap_or(name);
            if name.starts_with("rating:")
                || matches!(bare, "general" | "sensitive" | "questionable" | "explicit")
            {
                return Kind::Rating;
            }
            let Some(store) = store else {
                return Kind::General;
            };
            let rows = store.exact_rows(&name.replace(' ', "_"));
            match rows.iter().max_by_key(|(_, posts)| *posts).map(|(c, _)| *c) {
                Some(4) => Kind::Character,
                Some(1 | 3 | 5) => Kind::Other,
                _ => Kind::General,
            }
        })
        .collect()
}

// ── discovery ───────────────────────────────────────────────────────────────

/// Folder and file names that say "image tagger".
fn taggerish(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [
        "tagger", "tagging", "wd14", "wd-", "wd_", "joytag", "camie", "pixai", "z3d", "e621",
        "animetimm", "dbv4", "cl_tagger", "interrogat", "caption", "booru",
    ]
    .iter()
    .any(|k| n.contains(k))
}

const SKIP_DIRS: &[&str] = &[
    "blobs",
    "__pycache__",
    "node_modules",
    "venv",
    ".venv",
    "site-packages",
    "web",
    "js",
    "docs",
    "examples",
    "example_workflows",
    "tests",
];

const WEIGHTS: &[&str] = &["pytorch_model.bin", "model.safetensors", "model.pth", "model.pt"];

/// Tag list candidates beside a model, best first.
fn label_candidates(dir: &Path, stem: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = [
        format!("{stem}.csv"),
        "selected_tags.csv".into(),
        format!("{stem}.json"),
        format!("{stem}_tags.json"),
        format!("{stem}.txt"),
    ]
    .iter()
    .map(|n| dir.join(n))
    .filter(|p| p.is_file())
    .collect();
    let mut rest: Vec<PathBuf> = fs::read_dir(dir)
        .map(|r| r.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    rest.sort();
    // In a flat folder of several taggers, `<other>.csv` belongs to `<other>.onnx`.
    let others: Vec<String> = rest
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("onnx")))
        .filter_map(|p| p.file_stem()?.to_str().map(str::to_ascii_lowercase))
        .filter(|s| *s != stem.to_ascii_lowercase())
        .collect();
    for p in rest {
        if p
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| others.contains(&s.to_ascii_lowercase()))
        {
            continue;
        }
        let Some(name) = p.file_name().and_then(|n| n.to_str()).map(str::to_ascii_lowercase) else {
            continue;
        };
        let ext_ok = [".csv", ".json", ".txt"].iter().any(|e| name.ends_with(e));
        let labelish = ["tag", "label", "vocab", "metadata", "mapping", "classes"]
            .iter()
            .any(|k| name.contains(k));
        let not_config = !matches!(
            name.as_str(),
            "config.json" | "preprocess.json" | "requirements.txt" | "readme.txt" | "license.txt"
        );
        if ext_ok && labelish && not_config && p.is_file() && !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// The user-facing place: the path from the models or ComfyUI folder down.
fn place_of(dir: &Path, base: &Path, prefix: &str) -> String {
    let rel = dir.strip_prefix(base).unwrap_or(dir);
    let mut parts: Vec<String> = vec![prefix.to_string()];
    parts.extend(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            // An HF cache revision is noise: models--org--repo/snapshots/<hash>.
            .filter(|c| c != "snapshots" && !(c.len() == 40 && c.chars().all(|x| x.is_ascii_hexdigit()))),
    );
    parts.retain(|p| !p.is_empty());
    parts.join("/")
}

/// The name people know a tagger by.
fn pretty(model: &Path) -> String {
    let stem = model
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let generic = {
        let s = stem.to_ascii_lowercase();
        s == "model" || s.starts_with("model_") || s.starts_with("model-") || s == "tagger"
    };
    // A generic file name is named by its folder; an HF cache folder by its repo.
    let mut base = stem.clone();
    if generic {
        let mut dir = model.parent();
        while let Some(d) = dir {
            let n = d.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let hash = n.len() == 40 && n.chars().all(|x| x.is_ascii_hexdigit());
            if !(n.is_empty() || hash || n == "snapshots" || n == "onnx") {
                base = n.rsplit("--").next().unwrap_or(n).to_string();
                break;
            }
            dir = d.parent();
        }
        let lower = stem.to_ascii_lowercase();
        if (lower.starts_with("model_") || lower.starts_with("model-")) && stem.len() > 6 {
            // model_fp16.onnx beside model.onnx: keep the variant visible.
            base = format!("{base} ({})", &stem[6..]);
        }
    }
    let s = base.to_ascii_lowercase();
    let known = [
        ("wd-eva02-large-tagger-v3", "WD v3 EVA02-Large"),
        ("wd-vit-large-tagger-v3", "WD v3 ViT-Large"),
        ("wd-vit-tagger-v3", "WD v3 ViT"),
        ("wd-swinv2-tagger-v3", "WD v3 SwinV2"),
        ("wd-convnext-tagger-v3", "WD v3 ConvNeXt"),
        ("wd-v1-4-moat-tagger-v2", "WD v1.4 MOAT"),
        ("wd-v1-4-swinv2-tagger-v2", "WD v1.4 SwinV2"),
        ("wd-v1-4-convnextv2-tagger-v2", "WD v1.4 ConvNeXtV2"),
        ("wd-v1-4-convnext-tagger-v2", "WD v1.4 ConvNeXt"),
        ("wd-v1-4-vit-tagger-v2", "WD v1.4 ViT"),
        ("convnextv2_huge.dbv4-full", "animetimm ConvNeXtV2-Huge"),
        ("caformer_s36.dbv4-full", "animetimm CAFormer-S36"),
    ];
    known
        .iter()
        .find(|(k, _)| s.contains(k))
        .map(|(_, v)| {
            // Keep a variant suffix such as " (fp16)".
            match base.find(" (") {
                Some(i) => format!("{v}{}", &base[i..]),
                None => v.to_string(),
            }
        })
        .unwrap_or(base)
}

fn thresholds(model: &Path) -> (f64, f64) {
    let s = model.to_string_lossy().to_ascii_lowercase();
    // PromptForge's measured values: WD's 0.35/0.85 hid animetimm characters it was 51-85% sure of.
    if s.contains("animetimm") || s.contains("dbv4") {
        (0.38, 0.51)
    } else {
        (0.35, 0.85)
    }
}

type Key = (PathBuf, u64, Option<SystemTime>);
/// Tag lists read once per file version: discovery runs each time the tab opens.
static SUMMARIES: Mutex<Option<HashMap<Key, Result<(usize, bool), String>>>> = Mutex::new(None);

fn summary(path: &Path) -> Result<(usize, bool), String> {
    let meta = fs::metadata(path).ok();
    let key = (
        path.to_path_buf(),
        meta.as_ref().map_or(0, |m| m.len()),
        meta.and_then(|m| m.modified().ok()),
    );
    if let Some(hit) = SUMMARIES
        .lock()
        .ok()
        .and_then(|m| m.as_ref().and_then(|m| m.get(&key).cloned()))
    {
        return hit;
    }
    let out = read_labels(path).map(|l| (l.names.len(), l.kinds.is_some()));
    if let Ok(mut m) = SUMMARIES.lock() {
        m.get_or_insert_with(HashMap::new).insert(key, out.clone());
    }
    out
}

#[derive(Default)]
pub struct Found {
    pub taggers: Vec<Tagger>,
    pub unusable: Vec<Unusable>,
}

impl Found {
    fn add_model(&mut self, model: PathBuf, place: &str, folder_taggerish: bool) {
        let id = model.to_string_lossy().to_string();
        if self.taggers.iter().any(|t| t.id == id) {
            return;
        }
        let dir = model.parent().unwrap_or(Path::new(""));
        let stem = model.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let name = pretty(&model);
        let candidates = label_candidates(dir, stem);
        if candidates.is_empty() {
            // An ONNX file in a tagger folder without its tag list: say so. Any other
            // ONNX model (a face detector in models/onnx) is not a tagger at all.
            if folder_taggerish || taggerish(stem) {
                self.unusable.push(Unusable {
                    name,
                    place: place.into(),
                    reason: "No tag list beside the model file. Taggers ship one (selected_tags.csv, or a .csv/.json with the model's name); put it in the same folder.".into(),
                });
            }
            return;
        }
        let mut last_error = String::new();
        for labels in candidates {
            match summary(&labels) {
                Ok((tags, categories)) => {
                    let (g, c) = thresholds(&model);
                    self.taggers.push(Tagger {
                        id,
                        name,
                        model: model.to_string_lossy().into(),
                        labels: labels.to_string_lossy().into(),
                        place: place.into(),
                        tags,
                        categories,
                        general_threshold: g,
                        character_threshold: c,
                    });
                    return;
                }
                Err(e) => last_error = e,
            }
        }
        self.unusable.push(Unusable {
            name,
            place: place.into(),
            reason: format!("Its tag list cannot be used: {last_error}."),
        });
    }

    /// A folder with a tag list and PyTorch weights but no ONNX file.
    fn add_weights_only(&mut self, dir: &Path, place: &str) {
        let Some(weights) = WEIGHTS.iter().find(|w| dir.join(w).is_file()) else {
            return;
        };
        if label_candidates(dir, "model").is_empty() {
            return;
        }
        let name = pretty(&dir.join("model.onnx"));
        if self.unusable.iter().any(|u| u.place == place && u.name == name) {
            return;
        }
        self.unusable.push(Unusable {
            name,
            place: place.into(),
            reason: format!("Only PyTorch weights here ({weights}); Media Analysis runs ONNX taggers. An ONNX copy of this tagger (model.onnx) in the same folder would be used."),
        });
    }
}

fn scan(found: &mut Found, dir: &Path, base: &Path, prefix: &str, depth: usize, taggerish_dir: bool) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    let place = place_of(dir, base, prefix);
    let mut has_onnx = false;
    for path in &entries {
        if path.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("onnx"))
        {
            has_onnx = true;
            found.add_model(path.clone(), &place, taggerish_dir);
        }
    }
    if !has_onnx {
        found.add_weights_only(dir, &place);
    }
    if depth == 0 {
        return;
    }
    for path in entries {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if path.is_dir()
            && !name.starts_with('.')
            && !SKIP_DIRS.contains(&name.to_ascii_lowercase().as_str())
        {
            let t = taggerish_dir || taggerish(name);
            scan(found, &path, base, prefix, depth - 1, t);
        }
    }
}

/// Every tagger in these model folders and their ComfyUI's custom nodes.
pub fn discover(models: &[PathBuf]) -> Found {
    let mut found = Found::default();
    // DEAS's own folder first: that is where the optional download goes.
    for m in models {
        scan(&mut found, &m.join("taggers"), m, "models", 4, true);
    }
    for m in models {
        let mut dirs: Vec<PathBuf> = fs::read_dir(m)
            .map(|r| r.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
            .unwrap_or_default();
        dirs.sort();
        for d in dirs {
            let name = d.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
            if name.eq_ignore_ascii_case("taggers") {
                continue;
            }
            if name.eq_ignore_ascii_case("onnx") || taggerish(&name) {
                scan(&mut found, &d, m, "models", 4, taggerish(&name));
            }
        }
        if let Some(comfy) = m.parent() {
            let nodes = comfy.join("custom_nodes");
            let mut dirs: Vec<PathBuf> = fs::read_dir(&nodes)
                .map(|r| r.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
                .unwrap_or_default();
            dirs.sort();
            for d in dirs {
                let name = d.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if taggerish(name) && !name.ends_with(".disabled") {
                    scan(&mut found, &d, comfy, "", 3, true);
                }
            }
        }
    }
    // Two copies of one tagger: tell them apart by where they are.
    let names: Vec<String> = found.taggers.iter().map(|t| t.name.clone()).collect();
    for t in &mut found.taggers {
        if names.iter().filter(|n| **n == t.name).count() > 1 {
            t.name = format!("{} · {}", t.name, t.place);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    const WD_CSV: &str = "tag_id,name,category,count\n9999999,general,9,1\n1,1girl,0,5\n2,frieren,4,3\n";

    #[test]
    fn finds_taggers_wherever_people_keep_them() {
        let tmp = tempfile::tempdir().unwrap();
        let comfy = tmp.path().join("ComfyUI");
        let models = comfy.join("models");
        // DEAS's own download layout.
        write(&models.join("taggers/wd-eva02-large-tagger-v3/model.onnx"), "x");
        write(&models.join("taggers/wd-eva02-large-tagger-v3/selected_tags.csv"), WD_CSV);
        // The WD14 Tagger node's flat layout, the same tagger again, and a half copy.
        let node = comfy.join("custom_nodes/comfyui-wd14-tagger/models");
        write(&node.join("wd-v1-4-moat-tagger-v2.onnx"), "x");
        write(&node.join("wd-v1-4-moat-tagger-v2.csv"), WD_CSV);
        write(&node.join("wd-eva02-large-tagger-v3.onnx"), "x");
        write(&node.join("wd-eva02-large-tagger-v3.csv"), WD_CSV);
        write(&node.join("half-copied.onnx"), "x");
        // animetimm: PyTorch only is listed with a reason, an ONNX copy is used.
        write(&models.join("animetimm/convnextv2_huge.dbv4-full/pytorch_model.bin"), "x");
        write(&models.join("animetimm/convnextv2_huge.dbv4-full/selected_tags.csv"), WD_CSV);
        write(&models.join("animetimm/caformer_s36.dbv4-full/model.onnx"), "x");
        write(&models.join("animetimm/caformer_s36.dbv4-full/selected_tags.csv"), WD_CSV);
        // A Hugging Face cache copy in models/onnx, with a JSON list (PixAI's shape).
        let snap = models.join("onnx/models--pixai-labs--pixai-tagger-v0.9-onnx/snapshots/0123456789abcdef0123456789abcdef01234567");
        write(&snap.join("model.onnx"), "x");
        write(
            &snap.join("tags_v0.9_13k.json"),
            r#"{"tag_map":{"1girl":0,"solo":1,"frieren":2},"tag_split":{"gen_tag_count":2,"character_tag_count":1}}"#,
        );
        // An ONNX model in models/onnx that is no tagger (a face detector): not listed at all.
        write(&models.join("onnx/face_detector.onnx"), "x");
        // Another tagger node (JoyTag: a plain text list, no categories).
        write(&comfy.join("custom_nodes/ComfyUI-JoyTag/models/joytag/model.onnx"), "x");
        write(&comfy.join("custom_nodes/ComfyUI-JoyTag/models/joytag/top_tags.txt"), "1girl\nsolo\n");
        // A tagger whose list is broken.
        write(&models.join("wd14_tagger/broken.onnx"), "x");
        write(&models.join("wd14_tagger/broken.json"), "{not json");
        // Not a tagger node: never scanned.
        write(&comfy.join("custom_nodes/ComfyUI-Impact/models/thing.onnx"), "x");

        let found = discover(&[models]);
        let got: Vec<(&str, &str, usize, bool)> = found
            .taggers
            .iter()
            .map(|t| (t.name.as_str(), t.place.as_str(), t.tags, t.categories))
            .collect();
        assert_eq!(
            got,
            vec![
                ("WD v3 EVA02-Large · models/taggers/wd-eva02-large-tagger-v3", "models/taggers/wd-eva02-large-tagger-v3", 3, true),
                ("animetimm CAFormer-S36", "models/animetimm/caformer_s36.dbv4-full", 3, true),
                ("pixai-tagger-v0.9-onnx", "models/onnx/models--pixai-labs--pixai-tagger-v0.9-onnx", 3, true),
                ("joytag", "custom_nodes/ComfyUI-JoyTag/models/joytag", 2, false),
                ("WD v3 EVA02-Large · custom_nodes/comfyui-wd14-tagger/models", "custom_nodes/comfyui-wd14-tagger/models", 3, true),
                ("WD v1.4 MOAT", "custom_nodes/comfyui-wd14-tagger/models", 3, true),
            ]
        );
        let reasons: Vec<(&str, &str)> = found
            .unusable
            .iter()
            .map(|u| (u.name.as_str(), u.reason.split(['.', ';', ':']).next().unwrap()))
            .collect();
        assert_eq!(
            reasons,
            vec![
                ("animetimm ConvNeXtV2-Huge", "Only PyTorch weights here (pytorch_model"),
                ("broken", "Its tag list cannot be used"),
                ("half-copied", "No tag list beside the model file"),
            ]
        );
        assert_eq!(
            (found.taggers[1].general_threshold, found.taggers[1].character_threshold),
            (0.38, 0.51)
        );
        assert_eq!(
            (found.taggers[0].general_threshold, found.taggers[0].character_threshold),
            (0.35, 0.85)
        );
    }

    fn json(text: &str) -> Labels {
        read_json_value(&serde_json::from_str(text).unwrap(), 0).unwrap()
    }

    #[test]
    fn reads_every_tag_list_shape() {
        use Kind::*;
        // WD / animetimm selected_tags.csv.
        let wd = read_csv(WD_CSV).unwrap();
        assert_eq!(wd.names, ["general", "1girl", "frieren"]);
        assert_eq!(wd.kinds.unwrap(), [Rating, General, Character]);
        // Text categories, quoted cells, no header with an id column, bare names.
        let named = read_csv("name,category\n\"a, b\",general\nmiku,character\nexplicit,rating\nsomeone,artist\n").unwrap();
        assert_eq!(named.names[0], "a, b");
        assert_eq!(named.kinds.unwrap(), [General, Character, Rating, Other]);
        let ids = read_csv("0,1girl,0\n1,miku,4\n").unwrap();
        assert_eq!((ids.names.clone(), ids.kinds.unwrap()), (vec!["1girl".to_string(), "miku".into()], vec![General, Character]));
        let bare = read_csv("1girl\nsolo\n").unwrap();
        assert_eq!((bare.names.len(), bare.kinds), (2, None));
        // JSON: a plain list, objects, an index map, a tag map, Camie, nested.
        assert_eq!(json(r#"["1girl","solo"]"#).names, ["1girl", "solo"]);
        let objs = json(r#"[{"name":"1girl","category":0},{"tag":"miku","category":"Character"}]"#);
        assert_eq!(objs.kinds.unwrap(), [General, Character]);
        let idx = json(r#"{"1":{"tag":"miku","category":"Character"},"0":{"tag":"1girl","category":"General"}}"#);
        assert_eq!((idx.names.clone(), idx.kinds.unwrap()), (vec!["1girl".to_string(), "miku".into()], vec![General, Character]));
        assert_eq!(json(r#"{"solo":1,"1girl":0}"#).names, ["1girl", "solo"]);
        let camie = json(r#"{"dataset_info":{"tag_mapping":{"idx_to_tag":{"0":"1girl","1":"miku","2":"general"},"tag_to_category":{"1girl":"general","miku":"character","general":"rating"}}}}"#);
        assert_eq!(camie.kinds.unwrap(), [General, Character, Rating]);
        assert_eq!(json(r#"{"tags":["a","b"]}"#).names, ["a", "b"]);
        // Shapes that cannot be trusted are refused with a reason.
        assert!(read_json_value(&serde_json::json!({"0":"a","2":"b"}), 0).unwrap_err().contains("gaps"));
        assert!(read_json_value(&serde_json::json!({"x":{"y":1}}), 0).is_err());
        assert!(read_csv("").is_err());
        // Plain text.
        assert_eq!(read_txt("1girl\n\nsolo\r\n").unwrap().names, ["1girl", "solo"]);
    }

    #[test]
    fn a_list_without_categories_is_sorted_by_the_tag_table() {
        let store = crate::forge::tags::TagStore::open(Path::new("no-such-dir/tags.sqlite")).unwrap();
        let labels = Labels {
            names: vec!["general".into(), "1girl".into(), "frieren".into(), "rating:explicit".into()],
            kinds: None,
        };
        assert_eq!(
            resolve_kinds(&labels, Some(&store)),
            [Kind::Rating, Kind::General, Kind::Character, Kind::Rating]
        );
        assert_eq!(resolve_kinds(&labels, None)[2], Kind::General);
    }
}
