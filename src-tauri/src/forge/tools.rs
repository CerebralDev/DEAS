//! The two tools a writing model may call: `search_tags` (the danbooru
//! database) and `read_guidance` (the model's reference documents). Ported
//! from PromptForge server/tools.mjs and guidance.mjs. They report facts and
//! never give orders, and they never fail the write: a bad call is answered.

use super::pack::PackModel;
use super::tags::{CATEGORIES, TagStore, category_name};
use serde_json::Value;

const MAX_QUERIES: usize = 24;
const DEFAULT_LIMIT: usize = 15;
const MAX_LIMIT: usize = 30;
const MAX_DOC_CHARS: usize = 60_000;

pub struct Outcome {
    pub ok: bool,
    pub content: String,
    pub queries: Vec<String>,
    pub document: Option<String>,
}

fn fail(content: impl Into<String>) -> Outcome {
    Outcome {
        ok: false,
        content: content.into(),
        queries: vec![],
        document: None,
    }
}

/// Arguments as an object; a bare or unparseable string comes back as `__text`.
pub fn arguments(raw: &Value) -> Value {
    match raw {
        Value::Object(_) => raw.clone(),
        Value::String(s) => match serde_json::from_str::<Value>(s) {
            Ok(v @ Value::Object(_)) => v,
            Ok(other) => {
                serde_json::json!({ "__text": other.as_str().map_or(other.to_string(), String::from) })
            }
            Err(_) => serde_json::json!({ "__text": s }),
        },
        _ => Value::Object(Default::default()),
    }
}

/// `1234567` -> `1,234,567`, the en-GB grouping PromptForge prints.
fn grouped(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

pub fn search_tags(store: &TagStore, raw: &Value, spaced: bool) -> Outcome {
    let args = arguments(raw);
    let args = match args.get("__text") {
        Some(t) => serde_json::json!({ "query": t }),
        None => args,
    };
    let asked: Vec<String> = ["query", "queries", "q", "tag", "name"]
        .iter()
        .filter_map(|k| args.get(*k))
        .find(|v| !v.is_null())
        .map(|v| match v {
            Value::Array(a) => a
                .iter()
                .map(|x| x.as_str().map_or(x.to_string(), String::from))
                .collect(),
            Value::String(s) => vec![s.clone()],
            other => vec![other.to_string()],
        })
        .unwrap_or_default();
    let all: Vec<String> = asked
        .into_iter()
        .map(|q| q.trim().to_string())
        .filter(|q| !q.is_empty())
        .collect();
    if all.is_empty() {
        return fail("search_tags needs a \"query\" — a word to look for.");
    }
    let cleaned: Vec<String> = all.iter().take(MAX_QUERIES).cloned().collect();
    let category = args
        .get("category")
        .and_then(Value::as_str)
        .map(str::to_lowercase);
    let code = match &category {
        Some(c) => match CATEGORIES.iter().find(|(name, _)| name == c) {
            Some((_, code)) => Some(*code),
            None => {
                let names: Vec<&str> = CATEGORIES.iter().map(|(n, _)| *n).collect();
                return fail(format!(
                    "Unknown category \"{c}\". Valid categories are: {}. Omit it to search all of them.",
                    names.join(", ")
                ));
            }
        },
        None => None,
    };
    let per_query = args
        .get("limit")
        .and_then(Value::as_f64)
        .map_or(DEFAULT_LIMIT, |l| (l as usize).clamp(1, MAX_LIMIT));
    let mut blocks = vec![];
    for q in &cleaned {
        let block = match store.lookup(q, spaced, code, per_query) {
            Err(e) => format!("The tag database is unavailable: {e}"),
            Ok(hits) if hits.is_empty() => format!(
                "No danbooru tag contains \"{q}\"{}. Searched every tag name in the database.",
                category
                    .as_deref()
                    .map(|c| format!(" in category {c}"))
                    .unwrap_or_default()
            ),
            Ok(hits) => {
                let rows: Vec<String> = hits
                    .iter()
                    .map(|h| {
                        format!(
                            "{} — {} posts ({}){}",
                            h.tag,
                            grouped(h.post_count),
                            category_name(h.category),
                            if h.deprecated {
                                " [retired by danbooru]"
                            } else {
                                ""
                            }
                        )
                    })
                    .collect();
                format!(
                    "{} tag{} matching \"{q}\":\n{}",
                    hits.len(),
                    if hits.len() == 1 { "" } else { "s" },
                    rows.join("\n")
                )
            }
        };
        blocks.push(block);
    }
    if all.len() > cleaned.len() {
        blocks.push(format!(
            "That call carried {} queries and only the first {} were run. A list that long is usually a repeating one.",
            all.len(),
            cleaned.len()
        ));
    }
    Outcome {
        ok: true,
        content: blocks.join("\n\n"),
        queries: cleaned,
        document: None,
    }
}

pub fn read_guidance(model: &PackModel, raw: &Value) -> Outcome {
    let args = arguments(raw);
    let wanted = ["name", "document", "doc", "key", "id", "__text"]
        .iter()
        .filter_map(|k| args.get(*k)?.as_str())
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or("")
        .to_string();
    let mut have: Vec<&String> = model.guidance_docs.keys().collect();
    have.sort();
    let have = have
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if wanted.is_empty() {
        return fail(if have.is_empty() {
            "This model has no reference documents.".into()
        } else {
            format!("read_guidance needs a document name. Available: {have}.")
        });
    }
    match model.guidance_docs.get(&wanted) {
        None => fail(if have.is_empty() {
            "This model has no reference documents.".into()
        } else {
            format!("No document called \"{wanted}\". Available: {have}.")
        }),
        Some(text) if text.chars().count() > MAX_DOC_CHARS => fail(format!(
            "\"{wanted}\" is {} characters, over the {MAX_DOC_CHARS} limit, and was not read. Work from the system prompt.",
            text.chars().count()
        )),
        Some(text) => Outcome {
            ok: true,
            content: text.trim().to_string(),
            queries: vec![],
            document: Some(wanted),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::tags::tests::fixture;

    #[test]
    fn search_answers_every_shape_of_call() {
        let store = fixture();
        let out = search_tags(
            &store,
            &Value::String("{\"queries\":[\"boots\",\"wet boots\"]}".into()),
            true,
        );
        assert!(out.ok);
        assert!(
            out.content
                .starts_with("3 tags matching \"boots\":\nboots — 600,000 posts (general)")
        );
        assert!(
            out.content
                .contains("No danbooru tag contains \"wet boots\"")
        );
        let bare = search_tags(&store, &Value::String("boots".into()), false);
        assert_eq!(bare.queries, ["boots"]);
        assert!(
            !search_tags(
                &store,
                &serde_json::json!({"query":"x","category":"weapon"}),
                true
            )
            .ok
        );
        assert!(!search_tags(&store, &serde_json::json!({}), true).ok);
    }

    #[test]
    fn numbers_grouped_like_promptforge() {
        assert_eq!(grouped(4_323_141), "4,323,141");
        assert_eq!(grouped(999), "999");
    }
}
