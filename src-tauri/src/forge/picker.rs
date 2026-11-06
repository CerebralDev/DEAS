//! The Tags section of the Prompt Forge tab: search the local tag database as
//! you type, and a picked character's traits from the local character table.
//! Tags come back in the spelling the target model wants (PromptForge's
//! tagForModel): underscores for an underscore model, spaces otherwise.

use super::canon::{CanonTable, Corrections};
use super::engine::data_dir;
use super::tags::{category_name, TagStore};
use serde::Serialize;
use std::path::Path;

/// Danbooru's character category.
const CHARACTER: i64 = 4;
const LIMIT: usize = 12;

#[derive(Debug, Serialize)]
pub struct Suggestion {
    pub tag: String,
    pub category: &'static str,
    pub count: i64,
}

fn spell(tag: &str, underscore: bool) -> String {
    if underscore {
        tag.to_string()
    } else {
        tag.replace('_', " ")
    }
}

fn underscore(dir: &Path, target: &str) -> bool {
    super::data::read(&dir.join("forge-pack.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|p| {
            p["models"][target]["entry"]["tag_style"]
                .as_str()
                .map(|s| s == "underscore")
        })
        .unwrap_or(false)
}

pub fn search(dir: &Path, target: &str, query: &str) -> Result<Vec<Suggestion>, String> {
    if query.trim().chars().count() < 2 {
        return Ok(vec![]);
    }
    let store = TagStore::open(&dir.join("tags.sqlite"))?;
    let under = underscore(dir, target);
    Ok(store
        .lookup(query, false, None, LIMIT * 2)?
        .into_iter()
        .filter(|h| !h.deprecated)
        .take(LIMIT)
        .map(|h| Suggestion {
            tag: spell(&h.tag, under),
            category: category_name(h.category),
            count: h.post_count,
        })
        .collect())
}

/// A character's canon from the table, or empty when the table does not know them.
pub fn traits(dir: &Path, target: &str, character: &str) -> Result<Vec<String>, String> {
    let store = TagStore::open(&dir.join("tags.sqlite")).ok();
    let key = character.trim().replace(' ', "_");
    if store
        .as_ref()
        .is_some_and(|s| s.exact_rows(&key).iter().all(|(cat, _)| *cat != CHARACTER))
    {
        return Ok(vec![]);
    }
    let corrections: Corrections = std::fs::read(dir.join("character-corrections.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let Ok(table) = CanonTable::open(&dir.join("character_traits.sqlite"), corrections) else {
        return Ok(vec![]);
    };
    let under = underscore(dir, target);
    Ok(table
        .lookup(&key, store.as_ref())
        .unwrap_or_default()
        .iter()
        .map(|t| spell(t, under))
        .collect())
}

#[tauri::command]
pub async fn native_forge_tag_search(
    root: String,
    target: String,
    query: String,
) -> Result<Vec<Suggestion>, String> {
    tokio::task::spawn_blocking(move || search(&data_dir(Path::new(&root)), &target, &query))
        .await
        .map_err(|_| "Tag search failed")?
        .inspect_err(|e| {
            crate::logs::push(
                crate::logs::Source::Deas,
                &format!("ERROR: Prompt Forge tags: {e}"),
            )
        })
}

#[tauri::command]
pub async fn native_forge_character_traits(
    root: String,
    target: String,
    character: String,
) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || traits(&data_dir(Path::new(&root)), &target, &character))
        .await
        .map_err(|_| "Character lookup failed")?
        .inspect_err(|e| {
            crate::logs::push(
                crate::logs::Source::Deas,
                &format!("ERROR: Prompt Forge tags: {e}"),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `DEAS_FORGE_DATA=<dir> cargo test --lib -- --ignored real_picker --nocapture`
    #[test]
    #[ignore = "needs the tag database and the character table"]
    fn real_picker() {
        let dir = std::path::PathBuf::from(std::env::var("DEAS_FORGE_DATA").unwrap());
        let hits = search(&dir, "illustrious", "frier").unwrap();
        println!(
            "{:?}",
            hits.iter()
                .map(|h| format!("{} [{}] {}", h.tag, h.category, h.count))
                .collect::<Vec<_>>()
        );
        assert_eq!(hits[0].tag, "frieren");
        let silver = search(&dir, "illustrious", "long ha").unwrap();
        println!("{:?}", silver.iter().map(|h| &h.tag).collect::<Vec<_>>());
        assert!(silver.iter().any(|h| h.tag == "long hair"));
        let t = traits(&dir, "illustrious", "frieren").unwrap();
        println!("frieren: {t:?}");
        assert!(t.iter().any(|x| x == "pointy ears"));
        assert!(traits(&dir, "illustrious", "silver hair")
            .unwrap()
            .is_empty());
    }
}
