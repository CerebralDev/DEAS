//! The Forge data pack: PromptForge's composed system prompts, segment blocks,
//! tool definitions, guidance documents and registry settings for the image
//! models, exported by PromptForge's own loaders so the text is exactly what
//! PromptForge sends. Read once per write; it is a small JSON file.

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

pub const SEPARATOR: &str = "\n\n---\n\n";

#[derive(Debug, Deserialize)]
pub struct Pack {
    pub source: Value,
    #[serde(default)]
    pub creativity_order: Vec<String>,
    pub config: Value,
    pub defaults: Value,
    pub shared: Shared,
    pub models: HashMap<String, PackModel>,
    /// The reference roles in file order: the order pictures are numbered in.
    #[serde(skip)]
    pub role_order: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct Shared {
    #[serde(default)]
    pub meta_segment: Option<String>,
    #[serde(default)]
    pub tag_search: Option<String>,
    #[serde(default)]
    pub guidance: Option<String>,
    /// PromptForge's second-pass instruction; without it there is no Second pass.
    #[serde(default)]
    pub second_pass: Option<String>,
    /// The reading pass before a write with reference pictures.
    #[serde(default)]
    pub reference_blueprint: Option<String>,
    /// Analyze's read-back of one reference picture.
    #[serde(default)]
    pub reference_analysis: Option<String>,
    /// Chat's system prompt (PromptForge's prompts/chat/default.md).
    #[serde(default)]
    pub chat: Option<String>,
    /// The video models' composed prompts, which Chat reads as guides.
    #[serde(default)]
    pub video_guides: HashMap<String, Guide>,
}

#[derive(Debug, Deserialize)]
pub struct Guide {
    pub label: String,
    pub system: String,
}

#[derive(Debug, Deserialize)]
pub struct PackModel {
    pub entry: Value,
    /// The Layout Studio's Rewrite instruction (MOSS only).
    #[serde(default)]
    pub layout_rewrite: Option<String>,
    pub system: String,
    #[serde(default)]
    pub system_by_mode: HashMap<String, String>,
    #[serde(default)]
    pub segment_blocks: HashMap<String, String>,
    #[serde(default)]
    pub guidance_tool: Vec<Value>,
    #[serde(default)]
    pub guidance_docs: HashMap<String, String>,
    #[serde(default)]
    pub search_tool: Vec<Value>,
}

impl Pack {
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = super::data::read(path).map_err(|e| {
            format!(
                "Prompt Forge data pack unavailable ({}): {e}",
                path.display()
            )
        })?;
        let mut pack: Pack = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Prompt Forge data pack is invalid: {e}"))?;
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Defaults {
            image_reference_roles: Keys,
        }
        #[derive(Deserialize)]
        struct Order {
            #[serde(default)]
            defaults: Defaults,
        }
        pack.role_order = serde_json::from_slice::<Order>(&bytes)
            .map(|o| o.defaults.image_reference_roles.0)
            .unwrap_or_default();
        pack.add_deas_models(&path.with_file_name("deas-models.json"))?;
        Ok(pack)
    }

    /// DEAS-owned writers (`deas-models.json`) for models the upstream pack lacks: each
    /// inherits its `base` model's entry and shared rules, then overrides them with its own.
    fn add_deas_models(&mut self, path: &Path) -> Result<(), String> {
        #[derive(Deserialize)]
        struct Extra {
            base: String,
            entry: serde_json::Map<String, Value>,
            system: String,
            #[serde(default)]
            segment_blocks: HashMap<String, String>,
        }
        #[derive(Deserialize)]
        struct Extras {
            models: HashMap<String, Extra>,
        }
        let bytes = super::data::read(path)
            .map_err(|e| format!("Prompt Forge DEAS writers unavailable: {e}"))?;
        let extras: Extras = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Prompt Forge DEAS writers are invalid: {e}"))?;
        for (key, extra) in extras.models {
            if self.models.contains_key(&key) {
                continue; // Upstream gained its own writer: it wins.
            }
            // A pack without the base (test fixtures, partial overrides) simply lacks this writer.
            let Some(base) = self.models.get(&extra.base) else {
                continue;
            };
            let mut entry = base.entry.as_object().cloned().unwrap_or_default();
            entry.extend(extra.entry);
            entry.insert("base".into(), Value::String(extra.base.clone()));
            // The base system is the shared rules, the separator, then its own model file.
            let shared = base.system.split(SEPARATOR).next().unwrap_or_default();
            let model = PackModel {
                entry: Value::Object(entry),
                layout_rewrite: None,
                system: format!("{shared}{SEPARATOR}{}", extra.system),
                system_by_mode: HashMap::new(),
                segment_blocks: extra.segment_blocks,
                guidance_tool: vec![],
                guidance_docs: HashMap::new(),
                search_tool: vec![],
            };
            self.models.insert(key, model);
        }
        Ok(())
    }
}

/// Object keys in the order the pack file has them. The parsed pack's maps are
/// sorted, and PromptForge's tiles, styles and presets are shown in file order.
#[derive(Debug, Default)]
pub struct Keys(pub Vec<String>);

impl<'de> serde::Deserialize<'de> for Keys {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Keys;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an object or a list of names")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut a: A) -> Result<Keys, A::Error> {
                let mut out = vec![];
                while let Some((k, _)) = a.next_entry::<String, serde::de::IgnoredAny>()? {
                    out.push(k);
                }
                Ok(Keys(out))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Keys, A::Error> {
                let mut out = vec![];
                while let Some(v) = a.next_element::<Value>()? {
                    if let Some(s) = v.as_str() {
                        out.push(s.to_string());
                    }
                }
                Ok(Keys(out))
            }
        }
        d.deserialize_any(V)
    }
}

impl PackModel {
    pub fn str(&self, key: &str) -> Option<&str> {
        self.entry.get(key).and_then(Value::as_str)
    }
    pub fn strings(&self, key: &str) -> Vec<String> {
        self.entry
            .get(key)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn flag(&self, key: &str) -> bool {
        self.entry
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
    /// `tag_style: space|underscore` marks the booru models.
    pub fn tag_style(&self) -> Option<&str> {
        self.str("tag_style")
    }
    pub fn modes(&self) -> Vec<String> {
        self.strings("modes")
    }
    /// The contract for this mode: `segments_by_mode`, else `segments`.
    pub fn expected(&self, mode: Option<&str>) -> Vec<String> {
        mode.and_then(|m| {
            self.entry
                .get("segments_by_mode")?
                .get(m)?
                .as_array()
                .cloned()
        })
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_else(|| self.strings("segments"))
    }
    pub fn system_for(&self, mode: Option<&str>) -> &str {
        mode.and_then(|m| self.system_by_mode.get(m))
            .map_or(self.system.as_str(), String::as_str)
    }
    /// What a person may switch off: the explaining segments and the negative.
    pub fn optional(&self) -> Vec<String> {
        let mut out = self.strings("meta_segments");
        if self.entry.get("has_negative").and_then(Value::as_bool) != Some(false) {
            out.extend(
                self.strings("segments")
                    .into_iter()
                    .filter(|l| l.to_lowercase().contains("negative")),
            );
        }
        out.dedup();
        out
    }
}

/// `prompts.segmentTail`: the contract list, the shared note before the first
/// explaining segment, then each segment's own block.
pub fn segment_tail(pack: &Pack, model: &PackModel, labels: &[String]) -> String {
    if labels.is_empty() {
        return String::new();
    }
    let list = labels
        .iter()
        .enumerate()
        .map(|(i, l)| format!("{}. `{l}`", i + 1))
        .collect::<Vec<_>>()
        .join("\n");
    let mut parts = vec![format!("## Segments to emit\n\n{list}")];
    let meta = model.strings("meta_segments");
    let mut meta_done = false;
    for label in labels {
        if meta.contains(label) && !meta_done {
            if let Some(shared) = pack
                .shared
                .meta_segment
                .as_deref()
                .filter(|s| !s.is_empty())
            {
                parts.push(shared.to_string());
            }
            meta_done = true;
        }
        if let Some(block) = model.segment_blocks.get(label).filter(|b| !b.is_empty()) {
            parts.push(block.clone());
        }
    }
    parts.join(SEPARATOR)
}
