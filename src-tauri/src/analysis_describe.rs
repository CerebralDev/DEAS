//! Media Analysis' Describe: the picture in words, from the vision LLM people already
//! write prompts with. Ported from PromptForge (server.mjs handleAnalyzeDescribe,
//! server/charcast.mjs); its instructions are PromptForge's prompts/analysis/describe.md,
//! bundled with the Forge data as `analysis-describe.md`.
//!
//! The model is given the tagger's tags, so it can say what they missed or got wrong,
//! and the cast of any series the tagger recognised a character from, so it can name
//! the others by eye rather than from memory: on PromptForge's own test picture the
//! model described Aura perfectly and could not name her until it was shown Frieren's
//! cast. The series comes from a character the tagger is already sure of, never from a
//! search by looks (that ranked her 278th and steered the model to a wrong name).
//!
//! The answer has three parts: Description, Characters, Tagger check. Every tag it
//! suggests under "Missed" is checked against DEAS's Danbooru table before anyone copies
//! it: a tag Danbooru does not have is one the image model was never trained on.

use crate::forge::{chat, data, engine::data_dir};
use crate::llm::Provider;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc, Mutex, OnceLock},
    time::Duration,
};

/// Framing tags every character carries, which say nothing about who they are.
const NOT_A_LOOK: &[&str] = &[
    "1girl",
    "1boy",
    "2girls",
    "2boys",
    "solo",
    "multiple_girls",
    "multiple_boys",
    "breasts",
    "large_breasts",
    "medium_breasts",
    "small_breasts",
];
/// PromptForge's cast settings: characters drawn at least this often, this many per
/// series, this many looks each.
const MIN_POSTS: i64 = 50;
const MAX_CAST: usize = 25;
const MAX_LOOK: usize = 14;
const MAX_TAGS: usize = 200;
const MAX_IMAGE: usize = 24 << 20;
const TIMEOUT: Duration = Duration::from_secs(900);

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CastMember {
    pub tag: String,
    pub posts: i64,
    pub look: Vec<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SeriesCast {
    pub series: String,
    pub cast: Vec<CastMember>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub text: String,
    pub description: String,
    /// The booru tags it named under Characters (empty for "none recognised").
    pub characters: Vec<String>,
    /// Tags it suggested under Missed, as tags.
    pub suggested: Vec<String>,
    /// Suggested tags Danbooru does not have.
    pub unknown: Vec<String>,
    pub doubtful: String,
    /// Which series' casts it was shown, and how many characters each.
    pub cast: Vec<(String, usize)>,
    pub stopped: bool,
}

#[derive(Serialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DescribeEvent {
    Started { cast: Vec<(String, usize)> },
    Text { text: String },
}

// ── the cast list ───────────────────────────────────────────────────────────

/// tag -> series, and series -> its characters, from `character_series.json`.
pub struct SeriesIndex {
    series: HashMap<String, String>,
    members: HashMap<String, Vec<String>>,
}

/// A character tag's post count and usual look, when known.
type MemberLookup<'a> = dyn Fn(&str) -> Option<(i64, Vec<String>)> + 'a;

impl SeriesIndex {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let series: HashMap<String, String> = serde_json::from_slice(bytes)
            .map_err(|_| "The character series list is unreadable".to_string())?;
        let mut members: HashMap<String, Vec<String>> = HashMap::new();
        for (tag, s) in &series {
            members.entry(s.clone()).or_default().push(tag.clone());
        }
        Ok(Self { series, members })
    }

    /// The cast of every series the given characters belong to, most-drawn first, each
    /// with their usual look; none for a series with only one known member.
    pub fn cast_for(&self, characters: &[String], member: &MemberLookup) -> Vec<SeriesCast> {
        let mut names: Vec<&String> = vec![];
        for c in characters {
            if let Some(s) = self.series.get(c) {
                if !names.contains(&s) {
                    names.push(s);
                }
            }
        }
        let mut out = vec![];
        for name in names {
            let mut cast: Vec<CastMember> = self
                .members
                .get(name)
                .into_iter()
                .flatten()
                .filter_map(|tag| {
                    let (posts, traits) = member(tag)?;
                    if posts < MIN_POSTS {
                        return None;
                    }
                    let look: Vec<String> = traits
                        .into_iter()
                        .filter(|t| !NOT_A_LOOK.contains(&t.as_str()))
                        .take(MAX_LOOK)
                        .collect();
                    (!look.is_empty()).then(|| CastMember {
                        tag: tag.clone(),
                        posts,
                        look,
                    })
                })
                .collect();
            cast.sort_by(|a, b| b.posts.cmp(&a.posts).then_with(|| a.tag.cmp(&b.tag)));
            cast.truncate(MAX_CAST);
            if cast.len() > 1 {
                out.push(SeriesCast {
                    series: name.clone(),
                    cast,
                });
            }
        }
        out
    }
}

/// The bundled index, read once; an environment override is read each time it is used.
fn series_index(dir: &Path) -> Result<Arc<SeriesIndex>, String> {
    static BUNDLED: OnceLock<Result<Arc<SeriesIndex>, String>> = OnceLock::new();
    let path = dir.join("character_series.json");
    if !path.exists() {
        return BUNDLED
            .get_or_init(|| {
                let bytes = data::read(&path).map_err(|e| e.to_string())?;
                SeriesIndex::parse(&bytes).map(Arc::new)
            })
            .clone();
    }
    let bytes =
        data::read(&path).map_err(|e| format!("The character series list cannot be read: {e}"))?;
    SeriesIndex::parse(&bytes).map(Arc::new)
}

/// The user turn, as PromptForge writes it.
pub fn question(tags: &[String], cast: &[SeriesCast]) -> String {
    if tags.is_empty() {
        return "No tags were found for this picture. Describe it, and list the main tags it should have under Missed.".into();
    }
    let mut block = String::new();
    if !cast.is_empty() {
        let parts: Vec<String> = cast
            .iter()
            .map(|s| {
                let mut lines = vec![format!(
                    "Characters from {}, with how they usually look:",
                    s.series
                )];
                lines.extend(
                    s.cast
                        .iter()
                        .map(|c| format!("- {}: {}", c.tag, c.look.join(", "))),
                );
                lines.join("\n")
            })
            .collect();
        block = format!(
            "{}\n\nIf anyone in the picture is one of these characters, name them with that exact tag. Go by what you can see; do not pick one just because it is on the list.\n\n",
            parts.join("\n\n")
        );
    }
    format!(
        "The tagger found these tags:\n{}\n\n{block}Describe the picture.",
        tags.join(", ")
    )
}

// ── reading the answer ──────────────────────────────────────────────────────

/// The text under a heading (`Description:`, `Characters:`, `Tagger check:`) up to the next.
fn section(text: &str, heading: &str) -> String {
    let headings = ["description", "characters", "tagger check"];
    let mut out = vec![];
    let mut inside = false;
    for line in text.lines() {
        let bare = line
            .trim()
            .trim_start_matches(['#', '*', ' '])
            .trim_end_matches('*')
            .trim();
        let lower = bare.to_ascii_lowercase();
        if let Some(h) = headings.iter().find(|h| {
            lower.starts_with(*h) && lower[h.len()..].trim_start_matches('*').starts_with(':')
        }) {
            inside = *h == heading;
            if inside {
                let rest = bare[h.len()..]
                    .trim_start_matches('*')
                    .trim_start_matches(':');
                let rest = rest.trim_start_matches('*').trim();
                if !rest.is_empty() {
                    out.push(rest.to_string());
                }
            }
            continue;
        }
        if inside {
            out.push(line.to_string());
        }
    }
    out.join("\n").trim().to_string()
}

/// A line of the Tagger check (`- Missed: ...`), with any lines that continue it.
fn check_line(check: &str, label: &str) -> String {
    let mut out = vec![];
    let mut inside = false;
    for line in check.lines() {
        let bare = line
            .trim()
            .trim_start_matches(['-', '*', ' '])
            .replace("**", "");
        let lower = bare.to_ascii_lowercase();
        if lower.starts_with("missed:") || lower.starts_with("doubtful:") {
            inside = lower.starts_with(&format!("{label}:"));
            if inside {
                out.push(bare[label.len() + 1..].trim().to_string());
            }
            continue;
        }
        if inside {
            out.push(line.trim().to_string());
        }
    }
    out.join("\n").trim().to_string()
}

fn as_tag(s: &str) -> Option<String> {
    let t = s
        .trim()
        .trim_start_matches(['-', '*', ' '])
        .trim_matches(['.', ';'])
        .trim()
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_");
    let ok = t.len() >= 2
        && t.len() <= 61
        && t.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_()'.-".contains(c));
    ok.then_some(t)
}

/// Backticked tags when the model marked them (its explanations in brackets are prose),
/// else the plain comma list.
pub fn suggested_tags(missed: &str) -> Vec<String> {
    let candidates: Vec<String> = if missed.contains('`') {
        missed
            .split('`')
            .skip(1)
            .step_by(2)
            .map(String::from)
            .collect()
    } else {
        missed.split([',', '\n']).map(String::from).collect()
    };
    let mut out: Vec<String> = vec![];
    for c in candidates {
        if let Some(t) = as_tag(&c) {
            // "Missed: none" is an answer, not a tag.
            let empty = matches!(
                t.as_str(),
                "none" | "nothing" | "n/a" | "na" | "no_tags" | "none_missed"
            );
            if !empty && !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

pub fn read_answer(
    text: &str,
    known: &dyn Fn(&str) -> bool,
    // A small model may list the tagger's general tags here: only character tags (or ones the table does not know) stay.
    is_character: &dyn Fn(&str) -> bool,
    cast: Vec<(String, usize)>,
    stopped: bool,
) -> Reading {
    let description = section(text, "description");
    let characters_text = section(text, "characters");
    let characters = if characters_text
        .to_ascii_lowercase()
        .contains("none recognised")
        || characters_text
            .to_ascii_lowercase()
            .contains("none recognized")
    {
        vec![]
    } else {
        characters_text
            .split([',', '\n'])
            .filter_map(|c| as_tag(c.trim().trim_matches('`')))
            .filter(|t| is_character(t))
            .collect()
    };
    let check = section(text, "tagger check");
    let suggested = suggested_tags(&check_line(&check, "missed"));
    let unknown = suggested.iter().filter(|t| !known(t)).cloned().collect();
    Reading {
        text: text.to_string(),
        // A model that skipped the headings still answered: show all of it.
        description: if description.is_empty() {
            text.trim().to_string()
        } else {
            description
        },
        characters,
        suggested,
        unknown,
        doubtful: check_line(&check, "doubtful"),
        cast,
        stopped,
    }
}

// ── the call ────────────────────────────────────────────────────────────────

fn stop_flags() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static FLAGS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    FLAGS.get_or_init(Default::default)
}

const NO_VISION: &str = "cannot see pictures. Pick a model that can (a vision model) in the Model list, or set one up under Backend → LLM.";

/// Ollama says what a model can do; other servers are only found out by asking.
async fn ollama_cannot_see(config: &crate::llm::LlmConfig, model: &str) -> bool {
    let Ok(url) = crate::llm::endpoint(config, "api/show") else {
        return false;
    };
    let Ok(client) = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(15))
        .build()
    else {
        return false;
    };
    let Ok(response) = client
        .post(url)
        .json(&json!({ "model": model }))
        .send()
        .await
    else {
        return false;
    };
    let Ok(value) = response.json::<Value>().await else {
        return false;
    };
    value["capabilities"]
        .as_array()
        .is_some_and(|c| !c.iter().any(|x| x.as_str() == Some("vision")))
}

/// A server's refusal of the picture, in plain words.
fn vision_error(model: &str, e: String) -> String {
    let lower = e.to_ascii_lowercase();
    if [
        "image input is not supported",
        "mmproj",
        "multimodal",
        "does not support image",
        "vision",
        "image_url",
    ]
    .iter()
    .any(|k| lower.contains(k))
    {
        format!("ANALYSIS_DESCRIBE: {model} {NO_VISION}")
    } else {
        format!("ANALYSIS_DESCRIBE: {e}")
    }
}

/// Temperature and presence penalty for Describe: PromptForge's sliders and ranges. At
/// 0.2 with no penalty the small vision models repeat whole sentences; Qwen's own advice
/// for its vision models is 0.7 with a presence penalty of 1.5.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sampling {
    pub temperature: f64,
    pub presence_penalty: f64,
}

impl Sampling {
    /// Clamped to PromptForge's ranges; anything missing keeps the old 0.2 and no penalty.
    pub fn from(temperature: Option<f64>, presence_penalty: Option<f64>) -> Self {
        let pick = |v: Option<f64>, lo: f64, hi: f64, default: f64| {
            v.filter(|x| x.is_finite())
                .map_or(default, |x| x.clamp(lo, hi))
        };
        Self {
            temperature: pick(temperature, 0.0, 1.5, 0.2),
            presence_penalty: pick(presence_penalty, 0.0, 2.0, 0.0),
        }
    }
}

pub async fn describe(
    root: &Path,
    model: &str,
    image: &str,
    tags: Vec<String>,
    sampling: Sampling,
    stop: &AtomicBool,
    emit: &mut (dyn FnMut(DescribeEvent) + Send),
) -> Result<Reading, String> {
    if model.trim().is_empty() {
        return Err("ANALYSIS_DESCRIBE: No model is selected. Pick one in the Model list, or set up an LLM under Backend → LLM.".into());
    }
    if chat::is_local(model) {
        return Err(format!(
            "ANALYSIS_DESCRIBE: The local GGUF runner {NO_VISION}"
        ));
    }
    if !(image.starts_with("data:image/png;base64,")
        || image.starts_with("data:image/jpeg;base64,")
        || image.starts_with("data:image/webp;base64,"))
        || image.len() > MAX_IMAGE
    {
        return Err(
            "ANALYSIS_DESCRIBE: The picture must be a PNG, JPEG or WebP under 24 MB.".into(),
        );
    }
    let config = crate::llm::read(root).map_err(|e| format!("ANALYSIS_DESCRIBE: {e}"))?;
    let dir = data_dir(root);
    let load_dir = dir.clone();
    let (instructions, forge) = tokio::task::spawn_blocking(move || {
        let text = data::read(&load_dir.join("analysis-describe.md"))
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        (text, crate::forge::engine::Data::load(&load_dir))
    })
    .await
    .map_err(|_| "ANALYSIS_DESCRIBE: The Forge data failed to load")?;
    let instructions = instructions.ok_or(
        "ANALYSIS_DESCRIBE: The Describe instructions (analysis-describe.md) are missing from the Forge data, so Describe is unavailable.",
    )?;
    let forge = forge.map_err(|e| format!("ANALYSIS_DESCRIBE: {e}"))?;
    let tags: Vec<String> = tags
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .take(MAX_TAGS)
        .collect();
    // The cast of any series the tagger recognised a character from.
    let cast = match (&forge.canon, series_index(&dir)) {
        (Some(canon), Ok(index)) => {
            let underscored: Vec<String> = tags.iter().map(|t| t.replace(' ', "_")).collect();
            index.cast_for(&underscored, &|tag| canon.cast_member(tag))
        }
        (_, Err(e)) => {
            crate::logs::push(
                crate::logs::Source::Deas,
                &format!("WARN: Media Analysis Describe: no cast list ({e})"),
            );
            vec![]
        }
        _ => vec![],
    };
    let shown: Vec<(String, usize)> = cast
        .iter()
        .map(|s| (s.series.clone(), s.cast.len()))
        .collect();
    if config.provider == Provider::Ollama && ollama_cannot_see(&config, model).await {
        return Err(format!("ANALYSIS_DESCRIBE: {model} {NO_VISION}"));
    }
    emit(DescribeEvent::Started {
        cast: shown.clone(),
    });
    let messages = vec![
        json!({ "role": "system", "content": instructions }),
        chat::user_with_images(
            &question(&tags, &cast),
            &[image.to_string()],
            config.provider == Provider::Ollama,
        ),
    ];
    let mut on_text = |piece: &str| {
        emit(DescribeEvent::Text {
            text: piece.to_string(),
        })
    };
    // A reading task: thinking off, sampling from the tab (PromptForge's settings).
    let reply = chat::stream_round(
        chat::Round {
            config: &config,
            model,
            messages: &messages,
            tools: None,
            temperature: Some(sampling.temperature),
            top_p: None,
            presence_penalty: (sampling.presence_penalty > 0.0)
                .then_some(sampling.presence_penalty),
            max_tokens: Some(2048),
            timeout: TIMEOUT,
        },
        &mut on_text,
        stop,
    )
    .await
    .map_err(|e| vision_error(model, e))?;
    let stopped = stop.load(std::sync::atomic::Ordering::Relaxed);
    let text = reply.content.trim().to_string();
    if text.is_empty() && !stopped {
        return Err(format!("ANALYSIS_DESCRIBE: {model} returned no description. If it is not a vision model, pick one that can see pictures."));
    }
    let store = forge.tags.as_ref();
    Ok(read_answer(
        &text,
        &|t| store.is_none_or(|s| s.post_count(t) > 0),
        &|t| {
            store.is_none_or(|s| {
                let rows = s.exact_rows(t);
                rows.is_empty() || rows.iter().any(|(category, _)| *category == 4)
            })
        },
        shown,
        stopped,
    ))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri commands take their IPC arguments flat.
pub async fn native_analysis_describe(
    root: String,
    model: String,
    image: String,
    tags: Vec<String>,
    temperature: Option<f64>,
    presence_penalty: Option<f64>,
    request_id: String,
    on_event: tauri::ipc::Channel<DescribeEvent>,
) -> Result<Reading, String> {
    let root = PathBuf::from(&root);
    if !root.is_dir() {
        return Err("ANALYSIS_DESCRIBE: Open a DEAS environment first.".into());
    }
    if request_id.len() > 64 || request_id.is_empty() {
        return Err("ANALYSIS_DESCRIBE: Invalid request".into());
    }
    let flag = Arc::new(AtomicBool::new(false));
    stop_flags()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(request_id.clone(), flag.clone());
    let mut emit = |e: DescribeEvent| {
        let _ = on_event.send(e);
    };
    let sampling = Sampling::from(temperature, presence_penalty);
    let out = describe(&root, &model, &image, tags, sampling, &flag, &mut emit).await;
    stop_flags()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&request_id);
    out.inspect_err(|e| {
        crate::logs::push(
            crate::logs::Source::Deas,
            &format!("ERROR: Media Analysis Describe: {e}"),
        );
    })
}

#[tauri::command]
pub fn native_analysis_describe_cancel(request_id: String) -> bool {
    stop_flags()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&request_id)
        .map(|f| f.store(true, std::sync::atomic::Ordering::Relaxed))
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_cast_list_names_frierens_cast() {
        let index = series_index(Path::new("no-such-environment/forge-data")).unwrap();
        let canon = crate::forge::canon::CanonTable::open(
            Path::new("no-such-environment/forge-data/character_traits.sqlite"),
            Default::default(),
        )
        .unwrap();
        let cast = index.cast_for(&["frieren".into(), "1girl".into()], &|t| {
            canon.cast_member(t)
        });
        assert_eq!(cast.len(), 1);
        let s = &cast[0];
        assert_eq!(s.series, "sousou_no_frieren");
        assert!(s.cast.len() > 5 && s.cast.len() <= MAX_CAST);
        assert!(s.cast.iter().any(|c| c.tag == "aura_(sousou_no_frieren)"));
        assert!(s.cast.windows(2).all(|w| w[0].posts >= w[1].posts));
        assert!(s.cast.iter().all(|c| c.posts >= MIN_POSTS
            && c.look.len() <= MAX_LOOK
            && !c.look.iter().any(|t| t == "1girl")));
        let q = question(&["frieren".into(), "1girl".into()], &cast);
        assert!(q.starts_with("The tagger found these tags:\nfrieren, 1girl\n\nCharacters from sousou_no_frieren, with how they usually look:\n- "));
        assert!(
            q.ends_with("do not pick one just because it is on the list.\n\nDescribe the picture.")
        );
        assert!(index
            .cast_for(&["no_such_character".into()], &|t| canon.cast_member(t))
            .is_empty());
        assert!(question(&[], &[]).contains("No tags were found"));
    }

    #[test]
    fn the_answer_splits_into_its_three_parts_and_tags_are_checked() {
        let text = "Description:\nA girl with white hair in twin tails stands in a field.\n\nShe smiles.\n\n**Characters:** frieren, `fern_(sousou_no_frieren)`\n\nTagger check:\n- Missed: `wheat_field` (the field behind her), `glossy_skin`, `twintails`\n- Doubtful: `night` — it is daytime.\n";
        let known = |t: &str| t != "glossy_skin";
        let r = read_answer(
            text,
            &known,
            &|_| true,
            vec![("sousou_no_frieren".into(), 20)],
            false,
        );
        assert_eq!(
            r.description,
            "A girl with white hair in twin tails stands in a field.\n\nShe smiles."
        );
        assert_eq!(r.characters, ["frieren", "fern_(sousou_no_frieren)"]);
        assert_eq!(r.suggested, ["wheat_field", "glossy_skin", "twintails"]);
        assert_eq!(r.unknown, ["glossy_skin"]);
        assert_eq!(r.doubtful, "`night` — it is daytime.");
        // Plain lists, nobody recognised, and an answer without headings.
        let plain = read_answer("Description: A cat.\nCharacters:\nnone recognised\nTagger check:\n- Missed: cat ears, tail\n- Doubtful: nothing", &|_| true, &|_| true, vec![], false);
        assert_eq!(
            (plain.characters.len(), plain.suggested.clone()),
            (0, vec!["cat_ears".to_string(), "tail".into()])
        );
        assert_eq!(plain.doubtful, "nothing");
        let general_as_names = read_answer(
            "Description: x
Characters:
1girl, frieren, solo",
            &|_| true,
            &|t| t == "frieren",
            vec![],
            false,
        );
        assert_eq!(general_as_names.characters, ["frieren"]);
        assert!(suggested_tags("none").is_empty() && suggested_tags("Nothing.").is_empty());
        assert_eq!(
            read_answer("Just a cat.", &|_| true, &|_| true, vec![], true).description,
            "Just a cat."
        );
    }

    #[test]
    fn a_server_refusing_the_picture_reads_as_no_vision() {
        assert!(vision_error("m", "LLM server error 500: image input is not supported - hint: you may need to provide the mmproj".into()).contains("cannot see pictures"));
        assert_eq!(
            vision_error("m", "The LLM took too long to answer".into()),
            "ANALYSIS_DESCRIBE: The LLM took too long to answer"
        );
    }

    #[test]
    fn sampling_keeps_promptforges_defaults_and_ranges() {
        assert_eq!(
            Sampling::from(None, None),
            Sampling {
                temperature: 0.2,
                presence_penalty: 0.0
            }
        );
        assert_eq!(
            Sampling::from(Some(0.7), Some(1.5)),
            Sampling {
                temperature: 0.7,
                presence_penalty: 1.5
            }
        );
        assert_eq!(
            Sampling::from(Some(9.0), Some(-1.0)),
            Sampling {
                temperature: 1.5,
                presence_penalty: 0.0
            }
        );
        assert_eq!(
            Sampling::from(Some(f64::NAN), Some(3.0)),
            Sampling {
                temperature: 0.2,
                presence_penalty: 2.0
            }
        );
    }
}
