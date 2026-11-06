//! The danbooru tag database (PromptForge's `tags.sqlite`, read-only): the
//! search behind `search_tags`, post counts for the clean-up passes, and the
//! name indexes the character resolver needs. Ported from PromptForge
//! server/tags.mjs. A missing database is a normal state: every caller
//! degrades to the behaviour it had without one.

use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

pub const CATEGORIES: [(&str, i64); 4] = [
    ("general", 0),
    ("artist", 1),
    ("copyright", 3),
    ("character", 4),
];

pub fn category_name(code: i64) -> &'static str {
    match code {
        1 => "artist",
        3 => "copyright",
        4 => "character",
        5 => "meta",
        _ => "general",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub tag: String,
    pub category: i64,
    pub post_count: i64,
    pub deprecated: bool,
}

pub struct TagStore {
    db: Mutex<Connection>,
    has_deprecated: bool,
    by_name_part: OnceLock<HashMap<String, Vec<(String, i64)>>>,
    general_tokens: OnceLock<HashMap<String, i64>>,
    alias_index: OnceLock<HashMap<String, (String, i64)>>,
}

/// Everything before `_(` — `makima_(chainsaw_man)` -> `makima`.
pub fn name_part(tag: &str) -> &str {
    tag.find("_(").map_or(tag, |i| &tag[..i])
}

/// Any spelling in, the stored underscore key out.
pub fn key_of(tag: &str) -> String {
    tag.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
}

fn escape_like(s: &str) -> String {
    s.chars().fold(String::new(), |mut out, c| {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
        out
    })
}

impl TagStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        let db = super::data::database(path)
            .map_err(|e| format!("Tag database could not be opened: {e}"))?;
        let has_deprecated = db
            .prepare("PRAGMA table_info(tags)")
            .and_then(|mut s| {
                s.query_map([], |r| r.get::<_, String>(1))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map(|cols| cols.iter().any(|c| c == "deprecated"))
            .map_err(|e| format!("Tag database is not readable: {e}"))?;
        Ok(Self {
            db: Mutex::new(db),
            has_deprecated,
            by_name_part: OnceLock::new(),
            general_tokens: OnceLock::new(),
            alias_index: OnceLock::new(),
        })
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn retired(&self) -> &'static str {
        if self.has_deprecated {
            "deprecated"
        } else {
            "0"
        }
    }

    fn live(&self) -> &'static str {
        if self.has_deprecated {
            "AND deprecated = 0"
        } else {
            ""
        }
    }

    /// Substring search for the model's `search_tags`: exact, then prefix, then
    /// anywhere; retired tags last and flagged. Spelled in the model's style.
    pub fn lookup(
        &self,
        query: &str,
        spaced: bool,
        category: Option<i64>,
        limit: usize,
    ) -> Result<Vec<Hit>, String> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Ok(vec![]);
        }
        let needle = q.replace(' ', "_");
        let esc = escape_like(&needle);
        let sql = format!(
            "SELECT tag, category, post_count, {r} AS retired,
                    CASE WHEN tag = ?1 THEN 0 WHEN tag LIKE ?2 ESCAPE '\\' THEN 1 ELSE 2 END AS rank
               FROM tags
              WHERE variant = 'underscore' AND tag LIKE ?3 ESCAPE '\\' {cat}
              ORDER BY retired ASC, rank ASC, post_count DESC
              LIMIT ?4",
            r = self.retired(),
            cat = if category.is_some() {
                "AND category = ?5"
            } else {
                ""
            },
        );
        let db = self.db();
        let mut stmt = db.prepare_cached(&sql).map_err(|e| e.to_string())?;
        let map = |r: &rusqlite::Row| {
            let tag: String = r.get(0)?;
            Ok(Hit {
                tag: if spaced { tag.replace('_', " ") } else { tag },
                category: r.get(1)?,
                post_count: r.get(2)?,
                deprecated: r.get::<_, i64>(3)? == 1,
            })
        };
        let prefix = format!("{esc}%");
        let anywhere = format!("%{esc}%");
        let rows = match category {
            Some(c) => stmt.query_map(
                rusqlite::params![needle, prefix, anywhere, limit as i64, c],
                map,
            ),
            None => stmt.query_map(
                rusqlite::params![needle, prefix, anywhere, limit as i64],
                map,
            ),
        }
        .map_err(|e| e.to_string())?;
        rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
    }

    /// Danbooru-wide posts for one tag in either spelling; 0 if unknown.
    pub fn post_count(&self, tag: &str) -> i64 {
        let key = key_of(tag);
        self.db()
            .prepare_cached("SELECT post_count FROM tags WHERE tag = ?1 AND variant = 'underscore'")
            .and_then(|mut s| s.query_row([key], |r| r.get(0)).optional())
            .ok()
            .flatten()
            .unwrap_or(0)
    }

    /// Every exact row for a name, across categories: (category, posts).
    pub fn exact_rows(&self, tag: &str) -> Vec<(i64, i64)> {
        let key = key_of(tag);
        self.db()
            .prepare_cached(
                "SELECT category, post_count FROM tags WHERE tag = ?1 AND variant = 'underscore'",
            )
            .and_then(|mut s| {
                s.query_map([key], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The retired names danbooru now answers with this tag.
    pub fn aliases_of(&self, tag: &str) -> Vec<String> {
        let key = key_of(tag);
        self.db()
            .prepare_cached("SELECT aliases FROM tags WHERE tag = ?1 AND variant = 'underscore'")
            .and_then(|mut s| {
                s.query_row([key], |r| r.get::<_, Option<String>>(0))
                    .optional()
            })
            .ok()
            .flatten()
            .flatten()
            .map(|a| {
                a.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The live tag that replaced this retired one (the busiest, if several).
    pub fn alias_target(&self, tag: &str) -> Option<String> {
        let index = self.alias_index.get_or_init(|| {
            let mut index: HashMap<String, (String, i64)> = HashMap::new();
            let rows: Vec<(String, String, i64)> = self
                .db()
                .prepare("SELECT tag, aliases, post_count FROM tags WHERE variant = 'underscore' AND aliases IS NOT NULL AND aliases <> ''")
                .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect())
                .unwrap_or_default();
            for (tag, aliases, posts) in rows {
                for retired in aliases.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    if index.get(retired).is_none_or(|(_, held)| posts > *held) {
                        index.insert(retired.to_string(), (tag.clone(), posts));
                    }
                }
            }
            index
        });
        index.get(&key_of(tag)).map(|(t, _)| t.clone())
    }

    /// Live character tags whose name part is exactly `name`: (tag, posts).
    pub fn characters_named(&self, name: &str) -> Vec<(String, i64)> {
        let index = self.by_name_part.get_or_init(|| {
            let mut index: HashMap<String, Vec<(String, i64)>> = HashMap::new();
            let sql = format!(
                "SELECT tag, post_count FROM tags WHERE variant = 'underscore' AND category = 4 {}",
                self.live()
            );
            let rows: Vec<(String, i64)> = self
                .db()
                .prepare(&sql)
                .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
                .unwrap_or_default();
            for (tag, posts) in rows {
                index
                    .entry(name_part(&tag).to_string())
                    .or_default()
                    .push((tag, posts));
            }
            index
        });
        index.get(&key_of(name)).cloned().unwrap_or_default()
    }

    /// The most-used general tag containing this word as a word (`long` ->
    /// long_hair's count). Parenthetical general tags are not counted.
    pub fn general_token_top(&self, word: &str) -> i64 {
        let index = self.general_tokens.get_or_init(|| {
            let mut index: HashMap<String, i64> = HashMap::new();
            let rows: Vec<(String, i64)> = self
                .db()
                .prepare("SELECT tag, post_count FROM tags WHERE variant = 'underscore' AND category = 0")
                .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
                .unwrap_or_default();
            for (tag, posts) in rows {
                if tag.contains('(') {
                    continue;
                }
                for t in tag.split('_') {
                    let cur = index.entry(t.to_string()).or_insert(0);
                    *cur = (*cur).max(posts);
                }
            }
            index
        });
        index.get(&word.trim().to_lowercase()).copied().unwrap_or(0)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A small database with the real schema and real counts from the cases
    /// PromptForge's comments measured.
    pub fn fixture() -> TagStore {
        let dir = std::env::temp_dir().join(format!("deas-forge-tags-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tags.sqlite");
        let db = Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE tags (tag TEXT NOT NULL, variant TEXT NOT NULL, category INTEGER NOT NULL,
                                post_count INTEGER NOT NULL, aliases TEXT, deprecated INTEGER NOT NULL DEFAULT 0);",
        )
        .unwrap();
        let rows: &[(&str, i64, i64, &str, i64)] = &[
            ("long_hair", 0, 4_323_141, "", 0),
            ("thigh_boots", 0, 120_000, "", 0),
            ("boots", 0, 600_000, "", 0),
            ("rubber_boots", 0, 9_000, "", 0),
            ("black_footwear", 0, 284_390, "", 1),
            ("qipao", 0, 50_000, "china_dress", 0),
            ("frieren", 4, 8_253, "", 0),
            ("fern", 0, 1_038, "", 0),
            ("fern_(sousou_no_frieren)", 4, 5_355, "", 0),
            ("sousou_no_frieren", 3, 20_000, "", 0),
            ("makima_(chainsaw_man)", 4, 7_069, "", 0),
            ("chainsaw_man", 3, 30_000, "", 0),
            ("tea", 0, 12_292, "", 0),
            ("tea_(cafe-chan_to_break_time)", 4, 129, "", 0),
            ("rem_(re:zero)", 4, 9_042, "", 0),
            ("rem_(death_note)", 4, 88, "", 0),
            ("long_(chainsaw_man)", 4, 85, "", 0),
            ("zhongli_(genshin_impact)", 4, 20_000, "", 0),
            ("zhongli_(archon)_(genshin_impact)", 4, 900, "", 0),
            ("platinum_(arknights)", 4, 1_188, "", 0),
            ("red_eyes", 0, 1_267_681, "", 0),
            ("blonde_hair", 0, 2_000_000, "", 0),
        ];
        for (tag, cat, posts, aliases, dep) in rows {
            db.execute(
                "INSERT INTO tags VALUES (?1, 'underscore', ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    tag,
                    cat,
                    posts,
                    if aliases.is_empty() {
                        None
                    } else {
                        Some(aliases)
                    },
                    dep
                ],
            )
            .unwrap();
        }
        drop(db);
        TagStore::open(&path).unwrap()
    }

    #[test]
    fn lookup_ranks_exact_then_prefix_then_anywhere_retired_last() {
        let store = fixture();
        let hits = store.lookup("boots", true, None, 10).unwrap();
        let tags: Vec<&str> = hits.iter().map(|h| h.tag.as_str()).collect();
        assert_eq!(tags, ["boots", "thigh boots", "rubber boots"]);
        let feet = store.lookup("black foot", false, None, 10).unwrap();
        assert!(feet[0].deprecated);
        assert!(store
            .lookup("frieren", false, Some(4), 10)
            .unwrap()
            .iter()
            .all(|h| h.category == 4));
    }

    #[test]
    fn counts_names_and_aliases() {
        let store = fixture();
        assert_eq!(store.post_count("long hair"), 4_323_141);
        assert_eq!(store.post_count("wet_boots"), 0);
        assert_eq!(store.characters_named("rem").len(), 2);
        assert_eq!(store.general_token_top("long"), 4_323_141);
        assert_eq!(store.general_token_top("makima"), 0);
        assert_eq!(store.alias_target("china_dress").as_deref(), Some("qipao"));
        assert_eq!(store.aliases_of("qipao"), ["china_dress"]);
        assert_eq!(store.exact_rows("tea"), [(0, 12_292)]);
    }
}
