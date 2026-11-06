//! Canonical appearance for the characters a brief names, from the local
//! character table (PromptForge's `character_traits.sqlite`) — no network.
//! Ported from PromptForge server/traits.mjs (readTable, repairTableTags,
//! cleanTableTraits) and characterCanon/characterBlock in server.mjs.

use super::appearance::canon_traits;
use super::character::{Ambiguous, Resolved};
use super::tags::{key_of, TagStore};
use rusqlite::{Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

const FRAMING: &[&str] = &[
    "1girl",
    "1boy",
    "2girls",
    "2boys",
    "3girls",
    "3boys",
    "multiple_girls",
    "multiple_boys",
    "male_focus",
    "female_focus",
    "solo",
    "solo_focus",
    "full_body",
    "upper_body",
    "lower_body",
    "cowboy_shot",
    "portrait",
    "close-up",
    "from_side",
    "from_behind",
    "from_above",
    "from_below",
    "looking_at_viewer",
    "looking_away",
    "simple_background",
    "white_background",
    "transparent_background",
    "greyscale",
    "monochrome",
    "artist_name",
    "signature",
    "watermark",
    "english_text",
    "speech_bubble",
    "commentary",
    "commentary_request",
    "comic",
    "4koma",
    "manga",
    "translated",
    "translation_request",
    "sketch",
    "lineart",
    "partially_colored",
    "spot_color",
];
const BODY_FOCUS: &[&str] = &[
    "breasts",
    "cleavage",
    "ass",
    "thighs",
    "collarbone",
    "navel",
    "armpits",
    "nipples",
    "midriff",
    "stomach",
    "sideboob",
    "underboob",
    "groin",
    "bare_shoulders",
    "bare_arms",
    "bare_legs",
    "feet",
    "barefoot",
    "toes",
    "soles",
];
const COLOUR_WORDS: &[&str] = &[
    "white",
    "grey",
    "gray",
    "silver",
    "platinum_blonde",
    "blonde",
    "blond",
    "black",
    "brown",
    "light_brown",
    "dark_brown",
    "red",
    "dark_red",
    "pink",
    "blue",
    "light_blue",
    "dark_blue",
    "green",
    "light_green",
    "dark_green",
    "purple",
    "light_purple",
    "orange",
    "aqua",
    "teal",
    "yellow",
];
const MULTICOLOUR: &[&str] = &[
    "multicolored_hair",
    "two-tone_hair",
    "gradient_hair",
    "streaked_hair",
    "colored_inner_hair",
    "colored_tips",
    "split-color_hair",
    "rainbow_hair",
    "heterochromia",
    "multicolored_eyes",
    "gradient_eyes",
];
const BREAST_SIZE: &[&str] = &[
    "flat_chest",
    "small_breasts",
    "medium_breasts",
    "large_breasts",
    "huge_breasts",
    "gigantic_breasts",
];
const LENGTH_BASE: &[&str] = &["very_short_hair", "short_hair", "medium_hair", "long_hair"];
const LENGTH_LONG: &[&str] = &["very_long_hair", "absurdly_long_hair"];
/// Fewer cleaned traits than this and the table is not trusted.
const TABLE_MIN_TRAITS: usize = 3;

#[derive(Debug, Clone, serde::Deserialize)]
struct Item {
    tag: String,
    #[serde(default)]
    sources: Vec<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
pub struct Corrections {
    #[serde(default)]
    pub trait_remove: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub table_skip: Vec<String>,
    #[serde(default)]
    pub table_contaminants: HashMap<String, Vec<String>>,
}

pub struct CanonTable {
    db: Mutex<Connection>,
    corrections: Corrections,
}

/// `white_hair` -> Some("hair"), `long_hair` -> Some("length"), etc.
fn family_of(tag: &str) -> Option<&'static str> {
    if BREAST_SIZE.contains(&tag) {
        return Some("breasts");
    }
    if LENGTH_BASE.contains(&tag) {
        return Some("length");
    }
    for (suffix, family) in [("_hair", "hair"), ("_eyes", "eyes")] {
        if let Some(colour) = tag.strip_suffix(suffix) {
            if COLOUR_WORDS.contains(&colour) {
                return Some(family);
            }
        }
    }
    None
}

/// One hair colour, one eye colour, one breast size, one base hair length.
/// The row both lists carry wins, then the earlier one; a multicolour marker
/// spares its whole feature; very long hair rides with long hair.
fn resolve_families(rows: Vec<Item>) -> Vec<Item> {
    let mut skip = HashSet::new();
    for r in &rows {
        if MULTICOLOUR.contains(&r.tag.as_str()) {
            skip.insert(if r.tag.contains("eye") || r.tag == "heterochromia" {
                "eyes"
            } else {
                "hair"
            });
        }
    }
    let mut winner: HashMap<&str, usize> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        let Some(family) = family_of(&r.tag) else {
            continue;
        };
        if skip.contains(family) {
            continue;
        }
        match winner.get(family) {
            Some(&held) if rows[held].sources.len() >= r.sources.len() => {}
            _ => {
                winner.insert(family, i);
            }
        }
    }
    let length = winner.get("length").map(|&i| rows[i].tag.clone());
    rows.iter()
        .enumerate()
        .filter(|(i, r)| {
            if LENGTH_LONG.contains(&r.tag.as_str()) {
                return length.as_deref().is_none_or(|l| l == "long_hair");
            }
            match family_of(&r.tag) {
                Some(f) if !skip.contains(f) => winner.get(f) == Some(i),
                _ => true,
            }
        })
        .map(|(_, r)| r.clone())
        .collect()
}

fn clean_table_traits(items: Vec<Item>) -> Vec<Item> {
    let mut seen = HashSet::new();
    let rows = items
        .into_iter()
        .filter(|it| {
            !FRAMING.contains(&it.tag.as_str())
                && !BODY_FOCUS.contains(&it.tag.as_str())
                && seen.insert(it.tag.clone())
        })
        .collect();
    resolve_families(rows)
}

/// Renamed tags take their new name; deprecated ones with no successor go.
fn repair_table_tags(items: Vec<Item>, store: Option<&TagStore>) -> Vec<Item> {
    let Some(store) = store else { return items };
    items
        .into_iter()
        .filter_map(|it| {
            if store.post_count(&it.tag) > 0 {
                return Some(it);
            }
            let renamed = store.alias_target(&it.tag)?;
            (store.post_count(&renamed) > 0).then_some(Item {
                tag: renamed,
                sources: it.sources,
            })
        })
        .collect()
}

impl CanonTable {
    pub fn open(path: &Path, corrections: Corrections) -> Result<Self, String> {
        let db = super::data::database(path)
            .map_err(|e| format!("Character table could not be opened: {e}"))?;
        Ok(Self {
            db: Mutex::new(db),
            corrections,
        })
    }

    fn row(&self, name: &str) -> Option<String> {
        self.db
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .prepare_cached("SELECT traits FROM characters WHERE tag = ?1")
            .and_then(|mut s| s.query_row([name], |r| r.get(0)).optional())
            .ok()
            .flatten()
    }

    fn read(&self, key: &str, store: Option<&TagStore>) -> Option<Vec<String>> {
        // Danbooru renames characters; the table may hold her under a retired name.
        let raw = self
            .row(key)
            .or_else(|| store?.aliases_of(key).iter().find_map(|old| self.row(old)))?;
        let items: Vec<Item> = serde_json::from_str(&raw).ok()?;
        let traits: Vec<String> = clean_table_traits(repair_table_tags(items, store))
            .into_iter()
            .map(|i| i.tag)
            .collect();
        (traits.len() >= TABLE_MIN_TRAITS).then_some(traits)
    }

    /// Her traits from the table, with the group's hand corrections applied.
    pub fn lookup(&self, character: &str, store: Option<&TagStore>) -> Option<Vec<String>> {
        let key = key_of(character);
        if self.corrections.table_skip.iter().any(|s| key_of(s) == key) {
            return None;
        }
        let mut traits = self.read(&key, store)?;
        if let Some(others) = self.corrections.table_contaminants.get(&key) {
            let theirs: HashSet<String> = others
                .iter()
                .filter_map(|o| self.read(&key_of(o), store))
                .flatten()
                .collect();
            traits.retain(|t| !theirs.contains(t));
            if traits.len() < TABLE_MIN_TRAITS {
                return None;
            }
        }
        if let Some(drop) = self.corrections.trait_remove.get(&key) {
            let drop: HashSet<String> = drop.iter().map(|t| key_of(t)).collect();
            traits.retain(|t| !drop.contains(t));
        }
        Some(traits)
    }
}

/// The block for the user turn: canon as fact, never clothing or props, and
/// names that could not be confirmed listed as "do not invent".
pub fn character_block(
    table: Option<&CanonTable>,
    store: Option<&TagStore>,
    resolved: &[Resolved],
    ambiguous: &[Ambiguous],
    underscore: bool,
) -> String {
    let mut unresolved: Vec<String> = ambiguous.iter().map(|a| a.name.clone()).collect();
    let mut entries: Vec<(String, Vec<String>)> = vec![];
    for hit in resolved {
        let traits = table
            .and_then(|t| t.lookup(&hit.tag, store))
            .unwrap_or_default();
        let (kept, _withheld) = canon_traits(&traits);
        if kept.is_empty() {
            unresolved.push(hit.name.clone());
        } else {
            entries.push((hit.tag.clone(), kept));
        }
    }
    let spell = |t: &str| {
        if underscore {
            t.to_string()
        } else {
            t.replace('_', " ")
        }
    };
    let mut lines: Vec<String> = vec![];
    if !entries.is_empty() {
        lines.push(
            "Canonical appearance for the characters named in this brief, read from danbooru on this machine. \
             These are facts, not suggestions: use them exactly and do not add remembered details of your own to them. \
             Where an attached reference image contradicts anything below, the reference wins — the user chose that picture. \
             There is deliberately no clothing in this list: what she wears is the brief's to decide, not canon's."
                .into(),
        );
        for (tag, traits) in &entries {
            lines.push(format!(
                "  {} — {}",
                spell(tag),
                traits
                    .iter()
                    .map(|t| spell(t))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        lines.push(
            "The name before each dash is that character's exact tag. Write it exactly as given. \
             Do not add, remove or change a parenthetical series after it, do not translate it, and do not merge two of \
             them into one name — a tag spelled any other way is not a tag the model knows."
                .into(),
        );
    }
    if !unresolved.is_empty() {
        lines.push(format!(
            "{}these names could not be confirmed against danbooru: {}. Do not invent hair colour, eye colour, or an \
             outfit for them. Use only what the brief itself states and say so in your notes segment.",
            if lines.is_empty() { "" } else { "Also: " },
            unresolved.join(", ")
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(v: &[(&str, usize)]) -> Vec<Item> {
        v.iter()
            .map(|(t, n)| Item {
                tag: t.to_string(),
                sources: vec!["x".into(); *n],
            })
            .collect()
    }

    #[test]
    fn one_colour_per_feature_both_lists_win() {
        let out = clean_table_traits(items(&[
            ("pointy_ears", 1),
            ("grey_hair", 1),
            ("white_hair", 2),
            ("green_eyes", 2),
            ("1girl", 2),
            ("breasts", 1),
            ("long_hair", 2),
            ("very_long_hair", 1),
        ]));
        let tags: Vec<&str> = out.iter().map(|i| i.tag.as_str()).collect();
        assert_eq!(
            tags,
            [
                "pointy_ears",
                "white_hair",
                "green_eyes",
                "long_hair",
                "very_long_hair"
            ]
        );
    }

    #[test]
    fn multicolour_spares_the_feature() {
        let out = clean_table_traits(items(&[
            ("white_hair", 2),
            ("blue_hair", 2),
            ("gradient_hair", 1),
        ]));
        assert_eq!(out.len(), 3);
    }

    /// Against the real PromptForge data: `DEAS_FORGE_DATA=<dir> cargo test -- --ignored real_canon`.
    #[test]
    #[ignore = "needs DEAS_FORGE_DATA with tags.sqlite and character_traits.sqlite"]
    fn real_canon() {
        let dir = std::path::PathBuf::from(std::env::var("DEAS_FORGE_DATA").unwrap());
        let store = TagStore::open(&dir.join("tags.sqlite")).unwrap();
        let corrections = serde_json::from_str(
            &std::fs::read_to_string(dir.join("character-corrections.json")).unwrap(),
        )
        .unwrap();
        let table = CanonTable::open(&dir.join("character_traits.sqlite"), corrections).unwrap();
        for brief in [
            "frieren sitting by a campfire at night in a forest, fern asleep against her shoulder",
            "Makima from Chainsaw Man on a rain-slick Tokyo rooftop at night",
            "a girl in a yellow raincoat waiting at a rural bus stop in heavy rain",
            "a cup of tea on a low table, hauling rope in a storm, sitting on a rock",
            "Rem and Ram in the mansion kitchen",
        ] {
            let (resolved, ambiguous) =
                crate::forge::character::resolve(brief, &store, &Default::default());
            println!(
                "BRIEF {brief}\n{}\n",
                character_block(Some(&table), Some(&store), &resolved, &ambiguous, false)
            );
        }
    }

    #[test]
    fn block_wording_and_spelling() {
        let resolved = vec![];
        let ambiguous = vec![Ambiguous {
            name: "nagisa".into(),
            candidates: vec![],
        }];
        let text = character_block(None, None, &resolved, &ambiguous, false);
        assert!(text.starts_with("these names could not be confirmed against danbooru: nagisa."));
    }
}
