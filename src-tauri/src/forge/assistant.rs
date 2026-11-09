//! Chat: PromptForge's chat (server.mjs handleChat) run natively. A conversation
//! with the configured LLM, streamed as it is written, that knows this project:
//!
//! - `write_prompt` routes to the requested model's Prompt Forge mode first,
//!   otherwise runs its compatible simple writer, or refuses if neither exists.
//!   Simple-writer output is shown verbatim, never retouched by the chat model.
//! - `read_guide` reads how a model wants to be prompted (the image models' own
//!   prompt files and guidance documents, the H3/Wan/LTX prompts) and DEAS's own
//!   user guides.
//! - `search_tags` looks a tag up in the local database.
//! - Web search, when switched on, runs on the person's message before the answer
//!   (server/search.mjs) and the answer cites it.
//!
//! Conversations live in `<environment>/chats/<id>.json`. Pictures are held for
//! the turn they arrive in only and never written to disk.

use super::engine::{self, data_dir, Data, WriteRequest, ROUND_TIMEOUT};
use super::{chat, search, tools};
use crate::llm::{LlmConfig, Provider};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const DIR: &str = "chats";
const MAX_CHATS: usize = 500;
const MAX_MESSAGE: usize = 32_000;
const MAX_PICTURES: usize = 4;
/// Tool rounds before the model has to answer.
const MAX_ROUNDS: usize = 4;
/// Bound each guide page: small local models often run 32k contexts.
const GUIDE_LIMIT: usize = 16_000;

/// DEAS's own user guides, readable by `read_guide`.
const DOCS: &[(&str, &str, &str)] = &[
    (
        "deas-getting-started",
        "Getting started with DEAS",
        include_str!("../../../docs/GETTING-STARTED.md"),
    ),
    (
        "deas-studio",
        "Studio: Generate, settings, Quick Settings, colour schemes",
        include_str!("../../../docs/STUDIO.md"),
    ),
    (
        "deas-prompting",
        "Prompt Forge, the hammer, presets and wildcards",
        include_str!("../../../docs/PROMPTING.md"),
    ),
    (
        "deas-library",
        "Library: models, LoRAs, downloads",
        include_str!("../../../docs/LIBRARY.md"),
    ),
    (
        "deas-media",
        "Media browser",
        include_str!("../../../docs/MEDIA.md"),
    ),
    (
        "deas-backend",
        "Backend: install, ComfyUI, LLM backend",
        include_str!("../../../docs/BACKEND.md"),
    ),
    (
        "deas-music",
        "Music: YuE2 generation and covers",
        include_str!("../../../docs/MUSIC.md"),
    ),
    (
        "deas-tve",
        "TVE workspace and video post-processing (in the Studio guide)",
        include_str!("../../../docs/STUDIO.md"),
    ),
    (
        "deas-app-updates",
        "App updates",
        include_str!("../../../docs/APP-UPDATES.md"),
    ),
    ("deas-logs", "Logs", include_str!("../../../docs/LOGS.md")),
    (
        "deas-projects",
        "Projects",
        include_str!("../../../docs/PROJECTS.md"),
    ),
    (
        "deas-comfy",
        "The Comfy tab",
        include_str!("../../../docs/COMFY.md"),
    ),
    (
        "deas-chat",
        "Chat: tools, pictures, web search",
        include_str!("../../../docs/CHAT.md"),
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WrittenPrompt {
    pub target: String,
    pub label: String,
    pub prompt: String,
    #[serde(default)]
    pub negative: Option<String>,
    /// What the writer was asked for; replayed as the tool call's arguments in later turns.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub brief: String,
    /// Set by `render_picture`: the page draws this prompt with Studio's Generate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render: Option<RenderAsk>,
    /// What came of the render, written back by the page (`native_chat_render_result`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<RenderResult>,
}

/// The size a render asked for; both are as the person said them ("16:9", "1080p").
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderAsk {
    #[serde(default)]
    pub aspect: Option<String>,
    #[serde(default)]
    pub resolution: Option<String>,
}

/// A finished (or failed) chat render: the picture on disk, or why there is none.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderResult {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub checkpoint: String,
    #[serde(default)]
    pub ms: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

/// What Studio's Generate is set to when the person sends, so chat can render with it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioState {
    /// The selected checkpoint's file name.
    #[serde(default)]
    pub checkpoint: String,
    /// Its family's Prompt Forge dialect ("Illustrious", "Flux", ...).
    #[serde(default)]
    pub dialect: Option<String>,
    /// Why Generate cannot draw a chat picture right now; None = it can.
    #[serde(default)]
    pub blocked: Option<String>,
    /// The backend endpoint, for dialects the node pack writes.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// The Forge model id as the picker holds it (provider-qualified), for the node pack.
    #[serde(default)]
    pub writer_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    /// "user" or "assistant".
    pub role: String,
    pub content: String,
    /// Pictures sent with this message; only the count is kept.
    #[serde(default)]
    pub pictures: usize,
    #[serde(default)]
    pub prompts: Vec<WrittenPrompt>,
    #[serde(default)]
    pub citations: Vec<search::Hit>,
    /// What the answer looked at, for the person: "read_guide: ltx".
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chat {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub model: String,
    pub created: u64,
    pub updated: u64,
    /// Pinned to the top of the list (PromptForge's Pin to top).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
    pub messages: Vec<Message>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub updated: u64,
    pub count: usize,
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    tag = "event",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ChatEvent {
    Chat {
        id: String,
        title: String,
    },
    Searched {
        query: String,
        results: Vec<search::Hit>,
        detail: Option<String>,
    },
    Status {
        state: String,
        detail: Option<String>,
    },
    Text {
        text: String,
    },
    Prompt {
        prompt: WrittenPrompt,
    },
    Done {
        message: Message,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendRequest {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub message: String,
    pub model: String,
    #[serde(default)]
    pub pictures: Vec<String>,
    /// What the person is working on, gathered by the page at send time.
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub search: bool,
    /// Studio's Generate settings; absent = chat cannot render.
    #[serde(default)]
    pub studio: Option<StudioState>,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Today as YYYY-MM-DD (UTC), without a date crate.
fn today() -> String {
    let days = (now() / 86_400_000) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

// ── storage ─────────────────────────────────────────────────────────────────

fn valid_id(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn file(root: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err("Unknown conversation.".into());
    }
    Ok(root.join(DIR).join(format!("{id}.json")))
}

pub fn load(root: &Path, id: &str) -> Result<Chat, String> {
    let bytes =
        std::fs::read(file(root, id)?).map_err(|_| "That conversation no longer exists.")?;
    serde_json::from_slice(&bytes).map_err(|_| "That conversation could not be read.".into())
}

pub fn save(root: &Path, chat: &Chat) -> Result<(), String> {
    let dir = root.join(DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    let path = file(root, &chat.id)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(chat).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
}

pub fn list(root: &Path) -> Vec<Summary> {
    let Ok(entries) = std::fs::read_dir(root.join(DIR)) else {
        return vec![];
    };
    let mut out: Vec<Summary> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .take(MAX_CHATS * 2)
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|b| serde_json::from_slice::<Chat>(&b).ok())
        .map(|c| Summary {
            id: c.id,
            title: c.title,
            updated: c.updated,
            count: c.messages.len(),
            pinned: c.pinned,
        })
        .collect();
    // Pinned first, then newest; a pinned chat is never past the cut-off below.
    out.sort_by_key(|s| (std::cmp::Reverse(s.pinned), std::cmp::Reverse(s.updated)));
    out.truncate(MAX_CHATS);
    out
}

fn title_from(text: &str, pictures: usize) -> String {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if line.is_empty() {
        return if pictures > 0 {
            "A picture".into()
        } else {
            "New chat".into()
        };
    }
    let mut t: String = line.chars().take(60).collect();
    if line.chars().count() > 60 {
        t = format!("{}…", t.trim_end());
    }
    t
}

// ── tools ───────────────────────────────────────────────────────────────────

fn label_of(data: &Data, key: &str) -> String {
    data.pack
        .models
        .get(key)
        .and_then(|m| m.str("label"))
        .unwrap_or(key)
        .to_string()
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum PromptRoute<'a> {
    Forge(&'static str),
    Simple(&'a str),
    Missing,
}

fn prompt_route<'a>(data: &'a Data, name: &str) -> PromptRoute<'a> {
    let name = name.trim();
    let model = data.pack.models.iter().find(|(key, m)| {
        key.eq_ignore_ascii_case(name)
            || m.str("label")
                .is_some_and(|label| label.eq_ignore_ascii_case(name))
    });
    let guide = data
        .pack
        .shared
        .video_guides
        .iter()
        .find(|(key, g)| key.eq_ignore_ascii_case(name) || g.label.eq_ignore_ascii_case(name));
    let forge = crate::model_families::prompt_forge_target(name)
        .or_else(|| {
            model.and_then(|(_, m)| crate::model_families::prompt_forge_target(m.str("label")?))
        })
        .or_else(|| guide.and_then(|(_, g)| crate::model_families::prompt_forge_target(&g.label)));
    if let Some(target) = forge {
        return PromptRoute::Forge(target);
    }
    match model {
        Some((key, m)) => {
            let mode = m
                .str("default_mode")
                .filter(|mode| m.modes().iter().any(|v| v == mode));
            if !m.system_for(mode).trim().is_empty() && !m.expected(mode).is_empty() {
                PromptRoute::Simple(key)
            } else {
                PromptRoute::Missing
            }
        }
        _ => PromptRoute::Missing,
    }
}

/// Dialects with a native writer in this engine, and its pack key (`lib/forge.ts::nativeTarget`).
const NATIVE_WRITERS: &[(&str, &str)] = &[
    ("Illustrious", "illustrious"),
    ("Anima", "anima"),
    ("Krea2", "krea2"),
    ("MOSS", "moss"),
    ("Qwen Image 2.1", "qwen_image"),
];
/// Video dialects: their writers need shots, references and timing, which live in the Forge tab.
const VIDEO_DIALECTS: &[&str] = &["MiniMax H3", "LTX 2.3", "Wan 2.2"];

fn native_key(dialect: &str) -> Option<&'static str> {
    NATIVE_WRITERS
        .iter()
        .find(|(d, _)| d.eq_ignore_ascii_case(dialect))
        .map(|(_, k)| *k)
}

fn is_video(dialect: &str) -> bool {
    VIDEO_DIALECTS
        .iter()
        .any(|d| d.eq_ignore_ascii_case(dialect))
}

/// Writes a prompt for a Forge dialect with the same writer the Prompt Forge tab uses:
/// the native engine for the four image models, the node pack for the other image families.
/// `Err` is the sentence to show the person.
#[allow(clippy::too_many_arguments)]
async fn write_for(
    root: &Path,
    data: &Data,
    config: &LlmConfig,
    req: &SendRequest,
    dialect: &str,
    brief: &str,
    stop: &AtomicBool,
    emit: &mut (dyn FnMut(ChatEvent) + Send),
) -> Result<Option<WrittenPrompt>, String> {
    let (target, label) = match native_key(dialect) {
        Some(key) => (key.to_string(), label_of(data, key)),
        None => (dialect.to_string(), dialect.to_string()),
    };
    emit(ChatEvent::Status {
        state: "writing_prompt".into(),
        detail: Some(label.clone()),
    });
    let (prompt, negative) = if native_key(dialect).is_some() {
        let write = WriteRequest {
            target: target.clone(),
            brief: brief.to_string(),
            model: req.model.clone(),
            mode: None,
            segments_on: None,
            controls: Default::default(),
        };
        let written = tokio::select! {
            biased;
            _ = chat::cancelled(stop) => return Ok(None),
            result = engine::write(data, config, &write) => result,
        };
        match written {
            Ok(r) if r.refused => {
                return Err(format!("The {label} prompt writer declined this one."))
            }
            Ok(r) => (r.prompt, r.negative.filter(|n| !n.trim().is_empty())),
            Err(e) => {
                return Err(format!(
                    "The {label} prompt writer could not write this one: {e}"
                ))
            }
        }
    } else {
        let studio = req.studio.as_ref();
        let endpoint = studio
            .and_then(|s| s.endpoint.as_deref())
            .filter(|e| !e.trim().is_empty());
        let Some(endpoint) = endpoint else {
            return Err(format!("The {label} prompt writer runs in the ComfyUI backend. Start the backend (Backend → Start) and ask again."));
        };
        let model = studio
            .and_then(|s| s.writer_model.clone())
            .unwrap_or_else(|| req.model.clone());
        let body = json!({ "model": model, "write_for": dialect, "idea": brief, "images": [] });
        let root = root.to_string_lossy();
        let written = tokio::select! {
            biased;
            _ = chat::cancelled(stop) => return Ok(None),
            result = crate::llm::forge(endpoint, &root, "write", body) => result,
        };
        match written {
            Ok(v) => match v["prompt"]
                .as_str()
                .map(str::trim)
                .filter(|p| !p.is_empty())
            {
                Some(p) => (p.to_string(), None),
                None => return Err(format!("The {label} prompt writer returned nothing.")),
            },
            Err(e) => {
                return Err(format!(
                    "The {label} prompt writer could not write this one: {e}"
                ))
            }
        }
    };
    runaway(&label, &prompt)?;
    Ok(Some(WrittenPrompt {
        target,
        label,
        prompt,
        negative,
        brief: brief.to_string(),
        render: None,
        picture: None,
    }))
}

/// Generate's ceiling on a prompt (`execution.ts`, UTF-16 units).
const PROMPT_LIMIT: usize = 16_384;

/// A prompt past what Generate can take is a writer stuck repeating itself (seen 7 Nov 2026:
/// an 8B wrote 131,772 characters). It is never shown or drawn.
fn runaway(label: &str, prompt: &str) -> Result<(), String> {
    let units = prompt.encode_utf16().count();
    if units <= PROMPT_LIMIT {
        return Ok(());
    }
    crate::logs::push(
        crate::logs::Source::Deas,
        &format!("WARN: Chat: the {label} prompt writer ran away ({units} characters); discarded"),
    );
    Err(format!("The {label} prompt writer got stuck repeating itself ({units} characters, more than Generate can take), so nothing was kept or drawn. Ask again, or pick a bigger model at the top of Chat."))
}

/// A registered simple writer (a pack model without a Forge mode), written verbatim.
async fn write_simple(
    data: &Data,
    config: &LlmConfig,
    req: &SendRequest,
    key: &str,
    brief: &str,
    stop: &AtomicBool,
    emit: &mut (dyn FnMut(ChatEvent) + Send),
) -> Result<Option<WrittenPrompt>, String> {
    let label = label_of(data, key);
    emit(ChatEvent::Status {
        state: "writing_prompt".into(),
        detail: Some(label.clone()),
    });
    let write = WriteRequest {
        target: key.to_string(),
        brief: brief.to_string(),
        model: req.model.clone(),
        mode: None,
        segments_on: None,
        controls: Default::default(),
    };
    let written = tokio::select! {
        biased;
        _ = chat::cancelled(stop) => return Ok(None),
        result = engine::write(data, config, &write) => result,
    };
    match written {
        Ok(r) if r.refused => Err(format!("The {label} prompt writer declined this one.")),
        Ok(r) if runaway(&label, &r.prompt).is_err() => {
            Err(runaway(&label, &r.prompt).unwrap_err())
        }
        Ok(r) => Ok(Some(WrittenPrompt {
            target: key.to_string(),
            label,
            prompt: r.prompt,
            negative: r.negative.filter(|n| !n.trim().is_empty()),
            brief: brief.to_string(),
            render: None,
            picture: None,
        })),
        Err(e) => Err(format!(
            "The {label} prompt writer could not write this one: {e}"
        )),
    }
}

/// Every guide by name, with what it is.
fn guides(data: &Data) -> Vec<(String, String)> {
    let mut out = vec![];
    let mut keys: Vec<_> = data.pack.models.keys().collect();
    keys.sort();
    for key in keys {
        if let Some(m) = data.pack.models.get(key) {
            out.push((
                key.to_string(),
                format!(
                    "how {} prompts are written (its prompt writer's own instructions)",
                    label_of(data, key)
                ),
            ));
            let mut docs: Vec<&String> = m.guidance_docs.keys().collect();
            docs.sort();
            for d in docs {
                out.push((
                    d.clone(),
                    format!("{} guidance document", label_of(data, key)),
                ));
            }
        }
    }
    let mut video: Vec<(&String, &super::pack::Guide)> =
        data.pack.shared.video_guides.iter().collect();
    video.sort_by_key(|(k, _)| k.as_str());
    for (k, g) in video {
        out.push((
            k.clone(),
            format!("how {} video prompts are written", g.label),
        ));
    }
    for (k, what, _) in DOCS {
        out.push((k.to_string(), format!("DEAS guide: {what}")));
    }
    out
}

fn read_guide(data: &Data, name: &str, page: Option<&Value>) -> Result<String, String> {
    // Tool schemas are hints to the LLM, not native validation.
    let page = match page {
        None => 1,
        Some(value) => value
            .as_u64()
            .filter(|n| *n > 0)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or("Guide page must be a positive integer (starting at 1).")?,
    };
    let name = name.trim().to_lowercase();
    let text = if let Some(m) = data.pack.models.get(&name) {
        m.system.as_str()
    } else if let Some(doc) = data
        .pack
        .models
        .values()
        .find_map(|m| m.guidance_docs.get(&name))
    {
        doc.as_str()
    } else if let Some(g) = data.pack.shared.video_guides.get(&name) {
        g.system.as_str()
    } else {
        DOCS.iter()
            .find(|(k, _, _)| *k == name)
            .map(|(_, _, t)| *t)
            .ok_or_else(|| {
                format!(
                    "There is no guide called \"{name}\". The guides are: {}.",
                    guides(data)
                        .into_iter()
                        .map(|(k, _)| k)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?
    };
    let pages = text.chars().count().div_ceil(GUIDE_LIMIT).max(1);
    if page > pages {
        return Err(format!(
            "Guide \"{name}\" has {pages} page(s); request a page from 1 to {pages}."
        ));
    }
    let content: String = text
        .chars()
        .skip((page - 1) * GUIDE_LIMIT)
        .take(GUIDE_LIMIT)
        .collect();
    let next = if page < pages {
        format!(
            "Next: read_guide with name=\"{name}\" and page={}. Only this page has been read.",
            page + 1
        )
    } else {
        "End of guide.".into()
    };
    Ok(format!("[Guide page {page}/{pages}. {next}]\n\n{content}"))
}

/// The checkpoint chat can draw with right now and its dialect, when it can.
fn renderer(req: &SendRequest) -> Option<(&str, &str)> {
    let s = req.studio.as_ref()?;
    if s.blocked.is_some() || s.checkpoint.trim().is_empty() {
        return None;
    }
    let dialect = s
        .dialect
        .as_deref()
        .filter(|d| !d.is_empty() && !is_video(d))?;
    Some((s.checkpoint.as_str(), dialect))
}

fn tool_definitions(data: &Data, req: &SendRequest) -> Vec<Value> {
    let names: Vec<String> = guides(data).into_iter().map(|(k, _)| k).collect();
    let mut out = vec![];
    if let Some((checkpoint, dialect)) = renderer(req) {
        out.push(json!({ "type": "function", "function": {
            "name": "render_picture",
            "description": format!("Make a picture: writes the prompt with the {dialect} prompt writer and draws it with {checkpoint}, the checkpoint selected in Studio's Generate, with Generate's own settings. Call it when the person asks you to make, draw, render or generate an image (not just a prompt). If they want the prompt you just wrote drawn as it is, set same_prompt to true. The picture is shown to them in this chat."),
            "parameters": { "type": "object", "properties": {
                "brief": { "type": "string", "description": "What the picture should show, in the person's own words, with everything they asked for. When they change the last picture (\"make the highlights teal\"), the whole picture again with that change in it. Do not write the prompt here." },
                "same_prompt": { "type": "boolean", "description": "True only to draw the last prompt again exactly as it is (\"render that again\", \"another one\"). Any change to the picture, however small, needs a new brief instead." },
                "model": { "type": "string", "description": "Only if the person named a model to draw with: that model, unchanged." },
                "aspect": { "type": "string", "description": "The aspect ratio they asked for, as width:height, e.g. \"1:1\", \"16:9\", \"2:3\". Leave out if they did not say." },
                "resolution": { "type": "string", "description": "The resolution they asked for, e.g. \"1080p\", \"720p\" or \"1920x1080\". Leave out if they did not say." }
            }, "required": ["brief"] }
        }}));
    }
    out.push(json!({ "type": "function", "function": {
            "name": "write_prompt",
            "description": "Write or revise a prompt for a generation model with that model's own Prompt Forge writer; the prompt is shown to the person as written. Keep the requested model exactly and never choose a different one. If no model was specified, ask which model first. Use render_picture instead when they want the picture made.",
            "parameters": { "type": "object", "properties": {
                "model": { "type": "string", "description": "The requested generation model or family, unchanged; not the LLM. Unknown models are allowed here so the tool can refuse them accurately." },
                "brief": { "type": "string", "description": "What the picture should show, in the person's own words, with everything they asked for (subject, style, mood, characters, setting). Do not write the prompt here." }
            }, "required": ["model", "brief"] }
        }}));
    out.push(json!({ "type": "function", "function": {
        "name": "read_guide",
        "description": "Read a guide before explaining how a model wants to be prompted or how a part of DEAS works, instead of answering from memory. The image and video model guides are those models' own prompt-writing instructions.",
        "parameters": { "type": "object", "properties": {
            "name": { "type": "string", "enum": names, "description": "Which guide." },
            "page": { "type": "integer", "minimum": 1, "default": 1, "description": "Page number (1-based). Each page contains at most 16000 Unicode characters. The result gives the page count and next page; request further pages as needed." }
        }, "required": ["name"] }
    }}));
    if data.tags.is_some() {
        out.push(json!({ "type": "function", "function": {
            "name": "search_tags",
            "description": "Look tags up in the local Danbooru tag database: whether a tag exists, how it is spelled, how common it is. Use it before recommending a tag for Illustrious or Anima.",
            "parameters": { "type": "object", "properties": {
                "queries": { "type": "array", "items": { "type": "string" }, "description": "One or more words or tags to look up." }
            }, "required": ["queries"] }
        }}));
    }
    out
}

fn arguments(raw: &Value) -> Value {
    match raw {
        Value::String(s) => serde_json::from_str(s).unwrap_or(json!({})),
        other => other.clone(),
    }
}

// ── the conversation ────────────────────────────────────────────────────────

fn system_prompt(data: &Data, req: &SendRequest, searched: bool, search_on: bool) -> String {
    // The upstream fallback invents writers for unsupported models; DEAS routes all writes.
    let base = data.pack.shared.chat.clone().unwrap_or_else(|| "You are the assistant inside DEAS, a local desktop studio for AI images and video. Answer directly and accurately.".into())
        .replace("A prompt for\nanything else, write yourself.", "For any other model, use write_prompt too.");
    let mut lines = vec![
        format!("Today is {} (UTC).", today()),
        "You are running inside DEAS (Dark Engine AI Studio), a local desktop app that installs and runs ComfyUI, with Studio for generating, Prompt Forge for writing prompts, a Library of models and LoRAs, and this Chat. Where your base instructions mention PromptForge's own tabs, the same tools exist here: Prompt Forge is the tab for prompts, Studio's Generate is where pictures are made.".to_string(),
        format!(
            "Guides you can read with read_guide: {}.",
            guides(data).iter().map(|(k, what)| format!("{k} ({what})")).collect::<Vec<_>>().join("; ")
        ),
        "When the person asks how a model is prompted or how part of DEAS works, read the guide first and answer from it. Guides are paginated: use read_guide's page argument to read relevant later pages; follow the returned page count/next page rather than claiming the first page is the whole guide. If the tool budget is exhausted, state what remains unread and offer to continue, never invent the missing material.".into(),
        "For EVERY request to write or revise a generation prompt, call write_prompt with the requested model, including models not in the guide list. Never substitute another model, offer other models as replacements, or write a prompt yourself from memory. If the model is unspecified, ask which model. These DEAS routing rules override the base prompt's generic prompt-writing fallback.".into(),
    ];
    lines.push(match (renderer(req), req.studio.as_ref()) {
        (Some((checkpoint, dialect)), _) => format!("You can make pictures with render_picture. Studio's Generate has {checkpoint} selected (a {dialect} model), and that is what draws them, with Generate's sampler, steps and seed settings. When the person asks for a picture, call render_picture; do not just describe one. Chat cannot make videos."),
        (None, Some(s)) if s.blocked.is_some() => format!("You cannot make pictures right now: {} If the person asks for one, tell them that and what to do; you can still write the prompt with write_prompt.", s.blocked.as_deref().unwrap_or_default()),
        (None, Some(s)) if s.dialect.as_deref().is_some_and(is_video) => format!("You cannot make pictures right now: Studio's Generate has a video model selected ({}), and chat only draws pictures. If asked, tell the person to select an image checkpoint in Studio → Generate.", s.checkpoint),
        (None, Some(s)) if !s.checkpoint.is_empty() => format!("You cannot make pictures right now: the checkpoint selected in Studio's Generate ({}) has no Prompt Forge writer. If asked, tell the person to select a checkpoint of a supported image model in Studio → Generate.", s.checkpoint),
        _ => "You cannot make pictures right now: no checkpoint is selected in Studio's Generate. If asked, tell the person to select one there (and start the backend if it is not running).".into(),
    });
    lines.push(if !search_on {
        "Web search is not switched on in this DEAS. You cannot look anything up; when an answer needs current information, say so.".into()
    } else if searched {
        "Web search ran for this message; its results are in front of the question.".into()
    } else {
        "Web search did not run for this message: the Search button was off, and you cannot search on your own. Never say you are searching. When an answer needs current information, say so and suggest asking again with Search on.".into()
    });
    let mut parts = vec![base, lines.join("\n")];
    if let Some(ctx) = req
        .context
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        parts.push(format!(
            "What the person is working on in DEAS right now (shared because they switched \"Include what I'm working on\" on; refer to it when it helps):\n{}",
            ctx.chars().take(8000).collect::<String>()
        ));
    }
    parts.join(super::pack::SEPARATOR)
}

/// The stored conversation as the model sees it. Only this turn's pictures go up.
fn history(chat: &Chat, pictures: &[String], lead: Option<&str>, ollama: bool) -> Vec<Value> {
    let last = chat.messages.len().saturating_sub(1);
    chat.messages
        .iter()
        .enumerate()
        .flat_map(|(i, m)| {
            if i == last && m.role == "user" {
                let text = match lead {
                    Some(l) => format!("{l}

The question:
{}", m.content),
                    None if m.content.trim().is_empty() => "Here is a picture.".into(),
                    None => m.content.clone(),
                };
                return vec![chat::user_with_images(&text, pictures, ollama)];
            }
            // PromptForge (2 Oct 2026): a reply a tool wrote goes back as the tool call and
            // its result. Replayed as plain assistant text, it taught the model to write the
            // next prompt itself and copy the "written by" line; as a call, the model sees
            // its own brief and folds the next change ("make the highlights teal") into a
            // new one.
            if m.role == "assistant" {
                if let Some(p) = m.prompts.first() {
                    let render = p.render.is_some();
                    // Chats saved before the brief was kept: the prompt stands in, never an empty brief to copy.
                    let brief = if p.brief.trim().is_empty() { &p.prompt } else { &p.brief };
                    let call = chat::ToolCall {
                        id: Some(format!("call_{i}")),
                        name: if render { "render_picture" } else { "write_prompt" }.into(),
                        arguments: json!(if render {
                            {
                            // The size it asked for, so "now make it teal" keeps the 2:3.
                            let mut args = json!({ "brief": brief });
                            if let Some(ask) = &p.render {
                                if let Some(v) = &ask.aspect { args["aspect"] = json!(v); }
                                if let Some(v) = &ask.resolution { args["resolution"] = json!(v); }
                            }
                            args
                        }
                        } else {
                            json!({ "model": p.label, "brief": brief })
                        }
                        .to_string()),
                    };
                    return vec![
                        chat::assistant_turn("", std::slice::from_ref(&call), ollama),
                        chat::tool_result(&call, &m.content),
                    ];
                }
            }
            let mut content = m.content.clone();
            if m.pictures > 0 {
                content = format!("{content}

[{} picture(s) were attached here; they are no longer held.]", m.pictures).trim().to_string();
            }
            vec![json!({ "role": if m.role == "assistant" { "assistant" } else { "user" }, "content": content })]
        })
        .collect()
}

/// PromptForge's promptReply: the prompt as written, labelled, never retouched.
fn prompt_reply(label: &str, prompt: &str, negative: Option<&str>) -> String {
    let mut body = prompt.trim().to_string();
    if let Some(n) = negative.map(str::trim).filter(|n| !n.is_empty()) {
        body = format!("Positive prompt:\n{body}\n\nNegative prompt:\n{n}");
    }
    format!("{label} prompt, written by the {label} prompt writer:\n\n{body}")
}

fn stop_flags() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static FLAGS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    FLAGS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub async fn send(
    root: &Path,
    data: &Data,
    config: &LlmConfig,
    req: SendRequest,
    emit: &mut (dyn FnMut(ChatEvent) + Send),
) -> Result<Message, String> {
    let text = req.message.trim().to_string();
    if text.is_empty() && req.pictures.is_empty() {
        return Err("Say something first.".into());
    }
    if text.chars().count() > MAX_MESSAGE {
        return Err(format!("Messages are limited to {MAX_MESSAGE} characters."));
    }
    if req.pictures.len() > MAX_PICTURES {
        return Err(format!("Up to {MAX_PICTURES} pictures per message."));
    }
    if req.pictures.iter().any(|p| !super::refs::sendable(p)) {
        return Err("Pictures must be PNG, JPEG or WebP.".into());
    }
    // A `local:` GGUF runs through Prompt Forge's managed worker (scoped by native_chat_send).
    if config.provider == Provider::Local && !chat::is_local(&req.model) {
        return Err(
            "Chat needs a model: pick a local GGUF, or set an LLM server (Ollama or OpenAI-compatible) in Backend → LLM backend."
                .into(),
        );
    }
    let ollama = config.provider == Provider::Ollama;

    let mut chat = match req.id.as_deref() {
        Some(id) => load(root, id)?,
        None => {
            let t = now();
            Chat {
                id: uuid::Uuid::new_v4().to_string(),
                title: title_from(&text, req.pictures.len()),
                model: req.model.clone(),
                created: t,
                updated: t,
                pinned: false,
                messages: vec![],
            }
        }
    };
    chat.messages.push(Message {
        role: "user".into(),
        content: text.clone(),
        pictures: req.pictures.len(),
        prompts: vec![],
        citations: vec![],
        tools: vec![],
        at: now(),
    });
    chat.model = req.model.clone();
    chat.updated = now();
    // Saved before anything is generated: the question survives a failed answer.
    save(root, &chat)?;
    emit(ChatEvent::Chat {
        id: chat.id.clone(),
        title: chat.title.clone(),
    });

    let stop = Arc::new(AtomicBool::new(false));
    stop_flags()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(chat.id.clone(), stop.clone());
    let result = answer(root, data, config, &req, &mut chat, &stop, ollama, emit).await;
    stop_flags()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&chat.id);
    result
}

#[allow(clippy::too_many_arguments)]
async fn answer(
    root: &Path,
    data: &Data,
    config: &LlmConfig,
    req: &SendRequest,
    chat: &mut Chat,
    stop: &AtomicBool,
    ollama: bool,
    emit: &mut (dyn FnMut(ChatEvent) + Send),
) -> Result<Message, String> {
    // One GPU: a model the last chat render unloaded loads again now, and an idle
    // ComfyUI lets go of its checkpoint first if the model would not fit beside it.
    if super::gpu_swap::take_reloading() {
        emit(ChatEvent::Status {
            state: "loading_model".into(),
            detail: None,
        });
    }
    super::gpu_swap::make_room_for_llm(config, &req.model).await;
    let settings = search::read(root);
    let mut citations = vec![];
    let mut lead = None;
    let searched = req.search && settings.enabled() && !req.message.trim().is_empty();
    if searched {
        emit(ChatEvent::Status {
            state: "searching".into(),
            detail: Some(req.message.trim().chars().take(120).collect()),
        });
        let found = tokio::select! {
            biased;
            _ = chat::cancelled(stop) => None,
            found = search::run(&settings, &req.message) => Some(found),
        };
        if let Some(found) = found {
            emit(ChatEvent::Searched {
                query: found.query.clone(),
                results: found.results.clone(),
                detail: found.detail.clone(),
            });
            lead = search::as_context(&found);
            citations = found.results;
        }
    }

    let mut messages = vec![
        json!({ "role": "system", "content": system_prompt(data, req, searched, settings.enabled()) }),
    ];
    messages.extend(history(chat, &req.pictures, lead.as_deref(), ollama));
    let defs = tool_definitions(data, req);
    let mut reply = String::new();
    let mut prompts = vec![];
    let mut used = vec![];
    for round in 0..=MAX_ROUNDS {
        let offer = round < MAX_ROUNDS;
        emit(ChatEvent::Status {
            state: "thinking".into(),
            detail: None,
        });
        let out = {
            let mut on_text = |t: &str| {
                reply.push_str(t);
                emit(ChatEvent::Text {
                    text: t.to_string(),
                });
            };
            chat::stream_round(
                chat::Round {
                    config,
                    model: &req.model,
                    messages: &messages,
                    tools: offer.then_some(defs.as_slice()),
                    temperature: None,
                    top_p: None,
                    presence_penalty: None,
                    max_tokens: None,
                    timeout: ROUND_TIMEOUT,
                },
                &mut on_text,
                stop,
            )
            .await?
        };
        if stop.load(Ordering::Relaxed) || out.calls.is_empty() {
            break;
        }
        // write_prompt and render_picture end the turn: the prompt is the answer, as written.
        if let Some(call) = out
            .calls
            .iter()
            .find(|c| c.name == "write_prompt" || c.name == "render_picture")
        {
            let args = arguments(&call.arguments);
            let brief = args["brief"].as_str().unwrap_or("").trim().to_string();
            let rest = if call.name == "render_picture" {
                let asked = args["model"].as_str().unwrap_or("").trim().to_string();
                let same =
                    args["same_prompt"] == json!(true) || args["same_prompt"] == json!("true");
                let ask = RenderAsk {
                    aspect: args["aspect"]
                        .as_str()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from),
                    resolution: args["resolution"]
                        .as_str()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from),
                };
                // "Now make it teal" says nothing about size and means the same picture: an ask
                // without a size keeps the last one asked for in this chat (the LLM does not repeat it).
                let ask = if ask.aspect.is_none() && ask.resolution.is_none() {
                    chat.messages
                        .iter()
                        .rev()
                        .flat_map(|m| m.prompts.iter().rev())
                        .find_map(|p| {
                            p.render
                                .clone()
                                .filter(|r| r.aspect.is_some() || r.resolution.is_some())
                        })
                        .unwrap_or(ask)
                } else {
                    ask
                };
                match renderer(req) {
                    None => "I can't make a picture right now. Select an image checkpoint in Studio → Generate (and start the backend if it isn't running), then ask again.".into(),
                    Some((checkpoint, dialect)) => {
                        // Only a name that is clearly another known model is refused; a small
                        // model often puts the checkpoint's file name here (seen 7 Nov 2026, 4B).
                        let wanted = (!asked.is_empty()).then(|| {
                            crate::model_families::prompt_forge_target(&asked)
                                .map(String::from)
                                .or_else(|| match prompt_route(data, &asked) {
                                    PromptRoute::Forge(d) => Some(d.to_string()),
                                    _ => None,
                                })
                        }).flatten();
                        crate::logs::push(
                            crate::logs::Source::Deas,
                            &format!("INFO: Chat: render_picture chat={} dialect={dialect} same={same}", chat.id),
                        );
                        used.push(format!("render_picture: {dialect}"));
                        if let Some(w) = wanted.filter(|w| !w.eq_ignore_ascii_case(dialect)) {
                            format!("Studio's Generate has {checkpoint} selected, which is a {dialect} model, so I can't draw this with {w}. Select a {w} checkpoint in Studio → Generate and ask again, or ask me to draw it with {dialect}.")
                        } else {
                            let previous = same
                                .then(|| {
                                    chat.messages.iter().rev().flat_map(|m| m.prompts.iter().rev()).find(|p| {
                                        p.target.eq_ignore_ascii_case(dialect)
                                            || native_key(dialect).is_some_and(|k| p.target == k)
                                    })
                                })
                                .flatten()
                                .cloned();
                            let written = match previous {
                                Some(p) => runaway(&p.label, &p.prompt).map(|()| Some(WrittenPrompt { picture: None, ..p })),
                                None if brief.is_empty() => Err("What should the picture show?".to_string()),
                                None => write_for(root, data, config, req, dialect, &brief, stop, emit).await,
                            };
                            match written {
                                Ok(None) => break,
                                Ok(Some(mut p)) => {
                                    p.render = Some(ask);
                                    emit(ChatEvent::Prompt { prompt: p.clone() });
                                    let text = format!(
                                        "{}\n\nDrawing it with {checkpoint} in Studio's Generate.",
                                        prompt_reply(&p.label, &p.prompt, p.negative.as_deref())
                                    );
                                    prompts.push(p);
                                    text
                                }
                                Err(e) => e,
                            }
                        }
                    }
                }
            } else {
                let target = args["model"].as_str().unwrap_or("").trim().to_string();
                let route = prompt_route(data, &target);
                let route_name = match route {
                    PromptRoute::Forge(_) => "prompt-forge",
                    PromptRoute::Simple(_) => "simple-writer",
                    PromptRoute::Missing => "no-compatible-writer",
                };
                crate::logs::push(
                    crate::logs::Source::Deas,
                    &format!(
                        "INFO: Chat: write_prompt chat={} route={route_name}",
                        chat.id
                    ),
                );
                let target = match route {
                    PromptRoute::Simple(key) => key.to_string(),
                    PromptRoute::Forge(mode) => mode.to_string(),
                    PromptRoute::Missing => target,
                };
                used.push(format!("write_prompt: {target}"));
                if matches!(route, PromptRoute::Forge(mode) if is_video(mode)) {
                    format!(
                        "Use Studio → Prompt Forge → Write for → {target}. Video prompts need shots, timing and references, which are set there. No other model has been selected."
                    )
                } else if route == PromptRoute::Missing {
                    if target.is_empty() {
                        "Which generation model should this prompt be for?".into()
                    } else {
                        format!(
                            "No compatible prompt writer exists for '{target}': no Prompt Forge mode or compatible simple writer with a system prompt is available. I cannot write this prompt or substitute another model."
                        )
                    }
                } else if brief.is_empty() {
                    format!("No description was given for the {target} prompt.")
                } else {
                    let written = match route {
                        PromptRoute::Forge(mode) => {
                            write_for(root, data, config, req, mode, &brief, stop, emit).await
                        }
                        _ => write_simple(data, config, req, &target, &brief, stop, emit).await,
                    };
                    match written {
                        Ok(None) => break,
                        Ok(Some(p)) => {
                            emit(ChatEvent::Prompt { prompt: p.clone() });
                            let text = prompt_reply(&p.label, &p.prompt, p.negative.as_deref());
                            prompts.push(p);
                            text
                        }
                        Err(e) => e,
                    }
                }
            };
            let joined = if reply.trim().is_empty() {
                rest
            } else {
                format!("\n\n{rest}")
            };
            reply.push_str(&joined);
            emit(ChatEvent::Text { text: joined });
            break;
        }
        messages.push(chat::assistant_turn(&out.content, &out.calls, ollama));
        for call in &out.calls {
            let args = arguments(&call.arguments);
            let content = match call.name.as_str() {
                "read_guide" => {
                    let name = args["name"].as_str().unwrap_or("");
                    emit(ChatEvent::Status {
                        state: "reading_guide".into(),
                        detail: Some(name.to_string()),
                    });
                    used.push(format!("read_guide: {name}"));
                    // A model's own instructions read like orders; framed as reference, a small
                    // model explains them instead of answering in their segment format.
                    read_guide(data, name, args.get("page")).map(|g| format!(
                        "Reference material: the \"{name}\" guide. It is not addressed to you and its output format does not apply to you. Explain what the person asked about in your own words, in plain prose, and never answer with ===SEGMENT: headers.\n\n<<<GUIDE\n{g}\nGUIDE>>>"
                    )).unwrap_or_else(|error| error)
                }
                "search_tags" => {
                    emit(ChatEvent::Status {
                        state: "searching_tags".into(),
                        detail: None,
                    });
                    match &data.tags {
                        Some(store) => {
                            let out = tools::search_tags(store, &call.arguments, false);
                            used.push(format!("search_tags: {}", out.queries.join(", ")));
                            out.content
                        }
                        None => "The tag database is unavailable.".into(),
                    }
                }
                other => format!("There is no tool called {other}."),
            };
            messages.push(chat::tool_result(call, &content));
        }
    }
    if reply.trim().is_empty() && !stop.load(Ordering::Relaxed) {
        return Err("The model returned an empty answer.".into());
    }
    let message = Message {
        role: "assistant".into(),
        content: reply.trim().to_string(),
        pictures: 0,
        prompts,
        citations,
        tools: used,
        at: now(),
    };
    chat.messages.push(message.clone());
    chat.updated = now();
    save(root, chat)?;
    emit(ChatEvent::Done {
        message: message.clone(),
    });
    Ok(message)
}

// ── commands ────────────────────────────────────────────────────────────────

fn env_root(root: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(root);
    if root.trim().is_empty() || !p.is_dir() {
        return Err("Open a DEAS environment first.".into());
    }
    Ok(p)
}

#[tauri::command]
pub async fn native_chat_send(
    root: String,
    request: SendRequest,
    on_event: tauri::ipc::Channel<ChatEvent>,
) -> Result<Message, String> {
    let root = env_root(&root)?;
    let config = crate::llm::read(&root)?;
    let dir = data_dir(&root);
    let data = tokio::task::spawn_blocking(move || Data::load(&dir))
        .await
        .map_err(|_| "Prompt Forge data failed to load")??;
    let mut emit = |e: ChatEvent| {
        let _ = on_event.send(e);
    };
    // The local GGUF stays loaded for this turn only and is gone before a requested picture renders.
    chat::with_local_runner(&root, send(&root, &data, &config, request, &mut emit))
        .await
        .inspect_err(|e| {
            crate::logs::push(crate::logs::Source::Deas, &format!("ERROR: Chat: {e}"));
        })
}

#[tauri::command]
pub fn native_chat_cancel(id: String) -> bool {
    stop_flags()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&id)
        .map(|f| f.store(true, Ordering::Relaxed))
        .is_some()
}

#[tauri::command]
pub async fn native_chat_list(root: String) -> Result<Vec<Summary>, String> {
    let root = env_root(&root)?;
    tokio::task::spawn_blocking(move || list(&root))
        .await
        .map_err(|_| "Chat history failed to load".into())
}

#[tauri::command]
pub async fn native_chat_get(root: String, id: String) -> Result<Chat, String> {
    let root = env_root(&root)?;
    tokio::task::spawn_blocking(move || load(&root, &id))
        .await
        .map_err(|_| "Chat history failed to load")?
}

#[tauri::command]
pub async fn native_chat_rename(root: String, id: String, title: String) -> Result<(), String> {
    let root = env_root(&root)?;
    let title: String = title.trim().chars().take(120).collect();
    if title.is_empty() {
        return Err("A conversation needs a title.".into());
    }
    let mut chat = load(&root, &id)?;
    chat.title = title;
    save(&root, &chat)
}

/// Pins a conversation to the top of the list, or unpins it. Does not change when it was last spoken to.
#[tauri::command]
pub async fn native_chat_pin(root: String, id: String, pinned: bool) -> Result<(), String> {
    let root = env_root(&root)?;
    tokio::task::spawn_blocking(move || {
        let mut chat = load(&root, &id)?;
        chat.pinned = pinned;
        save(&root, &chat)
    })
    .await
    .map_err(|_| "Chat history failed to save")?
}

#[tauri::command]
pub async fn native_chat_delete(root: String, id: String) -> Result<(), String> {
    let root = env_root(&root)?;
    let path = file(&root, &id)?;
    std::fs::remove_file(path).map_err(|_| "That conversation could not be deleted.".into())
}

/// Records what came of a chat render on the prompt that asked for it, and tells the
/// model's next turn in plain words, so "make it brighter" knows a picture exists.
pub fn record_render(
    root: &Path,
    id: &str,
    at: u64,
    index: usize,
    result: RenderResult,
) -> Result<Message, String> {
    let mut chat = load(root, id)?;
    let message = chat
        .messages
        .iter_mut()
        .rev()
        .find(|m| m.role == "assistant" && m.at == at)
        .ok_or("That chat message no longer exists.")?;
    let prompt = message
        .prompts
        .get_mut(index)
        .filter(|p| p.render.is_some())
        .ok_or("That chat message has no picture to record.")?;
    let note = match (&result.path, &result.error) {
        (Some(_), _) => format!(
            "[The picture was drawn: {}×{}, {}{}.]",
            result.width,
            result.height,
            result.checkpoint,
            result
                .seed
                .map(|s| format!(", seed {s}"))
                .unwrap_or_default()
        ),
        (None, Some(e)) => format!("[The picture was not drawn: {e}]"),
        (None, None) => "[The picture was not drawn.]".into(),
    };
    let had = prompt.picture.is_some();
    prompt.picture = Some(result);
    if !had {
        message.content = format!("{}\n\n{note}", message.content.trim_end());
    }
    let out = message.clone();
    chat.updated = now();
    save(root, &chat)?;
    Ok(out)
}

#[tauri::command]
pub async fn native_chat_render_result(
    root: String,
    id: String,
    at: u64,
    index: usize,
    result: RenderResult,
) -> Result<Message, String> {
    let root = env_root(&root)?;
    if let Some(e) = &result.error {
        crate::logs::push(
            crate::logs::Source::Deas,
            &format!("ERROR: Chat render: {e}"),
        );
    }
    tokio::task::spawn_blocking(move || record_render(&root, &id, at, index, result))
        .await
        .map_err(|_| "Chat history failed to save")?
}

/// Before a chat render: unload the chat LLM when it sits on this PC's GPU, so the picture
/// gets the card. The next message loads it again.
#[tauri::command]
pub async fn native_chat_free_llm(root: String) -> Result<super::gpu_swap::Freed, String> {
    let root = env_root(&root)?;
    let config = crate::llm::read(&root)?;
    Ok(super::gpu_swap::free_llm(&config).await)
}

#[tauri::command]
pub async fn native_chat_settings(root: String) -> Result<search::Settings, String> {
    Ok(search::read(&env_root(&root)?))
}

#[tauri::command]
pub async fn native_chat_settings_write(
    root: String,
    settings: search::Settings,
) -> Result<search::Settings, String> {
    let root = env_root(&root)?;
    search::write(&root, &settings)?;
    Ok(search::read(&root))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_studio() -> SendRequest {
        SendRequest {
            id: None,
            message: String::new(),
            model: String::new(),
            pictures: vec![],
            context: None,
            search: false,
            studio: None,
        }
    }

    fn temp_root() -> PathBuf {
        let d = std::env::temp_dir().join(format!("deas-chat-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn routing_data() -> Data {
        Data {
            pack: serde_json::from_value(json!({
                "source": {}, "config": {}, "defaults": {},
                "shared": {"video_guides": {
                    "h3": {"label": "MiniMax H3", "system": "H3 instructions"},
                    "ltx": {"label": "LTX 2.3", "system": "LTX instructions"},
                    "guide-only": {"label": "Guide only", "system": "Explanation only"}
                }},
                "models": {
                    "anima": {"entry": {"label": "Anima", "segments": ["Positive prompt"]}, "system": "Anima instructions"},
                    "basic": {"entry": {"label": "Basic Model", "segments": ["Positive prompt"]}, "system": "Basic instructions"},
                    "empty": {"entry": {"label": "Empty"}, "system": " "}
                }
            })).unwrap(),
            tags: None,
            canon: None,
        }
    }

    #[test]
    fn guide_pages_are_bounded_lossless_and_validate_untrusted_page_numbers() {
        let mut data = routing_data();
        for text in [
            String::new(),
            "界🦀é\n".repeat(GUIDE_LIMIT),
            "a".repeat(GUIDE_LIMIT),
        ] {
            data.pack.models.get_mut("basic").unwrap().system = text.clone();
            let pages = text.chars().count().div_ceil(GUIDE_LIMIT).max(1);
            let mut restored = String::new();
            for page in 1..=pages {
                let result = read_guide(&data, " BASIC ", Some(&json!(page))).unwrap();
                assert!(result.starts_with(&format!("[Guide page {page}/{pages}.")));
                assert_eq!(result.contains("Next: read_guide"), page < pages);
                let body = result.split_once("\n\n").unwrap().1;
                assert!(body.chars().count() <= GUIDE_LIMIT);
                restored.push_str(body);
            }
            assert_eq!(restored, text);
            assert_eq!(
                read_guide(&data, "basic", None),
                read_guide(&data, "basic", Some(&json!(1)))
            );
            assert!(read_guide(&data, "basic", Some(&json!(pages + 1))).is_err());
        }
        for page in [
            json!(0),
            json!(-1),
            json!(1.5),
            json!("2"),
            json!(null),
            json!(true),
            json!([]),
            json!({}),
            json!(u64::MAX),
        ] {
            assert!(read_guide(&data, "basic", Some(&page)).is_err(), "{page}");
        }
        assert!(read_guide(&data, "../../secret", None)
            .unwrap_err()
            .contains("There is no guide"));
    }

    #[test]
    fn guide_registry_schema_and_all_sources_offer_complete_pages() {
        let mut data = routing_data();
        data.pack
            .models
            .get_mut("basic")
            .unwrap()
            .guidance_docs
            .insert("guidance".into(), "Guidance".into());
        let defs = tool_definitions(&data, &no_studio());
        let params = &defs[1]["function"]["parameters"];
        assert_eq!(params["required"], json!(["name"]));
        assert_eq!(params["properties"]["page"]["type"], "integer");
        assert_eq!(params["properties"]["page"]["minimum"], 1);
        for name in [
            "deas-music",
            "deas-tve",
            "deas-app-updates",
            "guidance",
            "h3",
            "basic",
        ] {
            assert!(params["properties"]["name"]["enum"]
                .as_array()
                .unwrap()
                .contains(&json!(name)));
            assert!(read_guide(&data, name, None).is_ok());
        }
        for (name, _, text) in DOCS {
            let mut restored = String::new();
            for page in 1..=text.chars().count().div_ceil(GUIDE_LIMIT).max(1) {
                let result = read_guide(&data, name, Some(&json!(page))).unwrap();
                restored.push_str(result.split_once("\n\n").unwrap().1);
            }
            assert_eq!(restored, *text, "{name}");
        }
    }

    #[tokio::test]
    async fn guide_pagination_reaches_chat_tool_results_as_reference_only() {
        use chat::tests::complete;
        let tool_reply = |page: Option<Value>| {
            let mut args = json!({"name":"basic"});
            if let Some(page) = page {
                args["page"] = page;
            }
            complete(&format!(
                "data: {}\n\n",
                json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"guide","function":{"name":"read_guide","arguments":args.to_string()}}]},"finish_reason":"tool_calls"}]})
            ))
        };
        let responses = vec![
            tool_reply(None),
            tool_reply(Some(json!(2))),
            tool_reply(Some(json!(0))),
            complete(&format!(
                "data: {}\n\n",
                json!({"choices":[{"delta":{"content":"The later section is available."},"finish_reason":"stop"}]})
            )),
        ];
        // The shared HTTP fixture records headers only; this regression needs tool bodies.
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, mut received) = tokio::sync::mpsc::channel(4);
        let server = tokio::spawn(async move {
            for (index, response) in responses.into_iter().enumerate() {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buf = [0; 4096];
                loop {
                    let n = socket.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&buf[..n]);
                    assert!(request.len() < 1_000_000);
                    if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&request[..end]);
                        let length: usize = head
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse().unwrap())
                            })
                            .unwrap();
                        if request.len() >= end + 4 + length {
                            tx.send((
                                index,
                                String::from_utf8(request[end + 4..end + 4 + length].to_vec())
                                    .unwrap(),
                            ))
                            .await
                            .unwrap();
                            break;
                        }
                    }
                }
                socket.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let mut data = routing_data();
        data.pack.models.get_mut("basic").unwrap().system =
            format!("{}LATER_SECTION", "界".repeat(GUIDE_LIMIT));
        let root = temp_root();
        let result = send(
            &root,
            &data,
            &LlmConfig {
                provider: Provider::Openai,
                base_url: url,
                api_key: String::new(),
            },
            SendRequest {
                id: None,
                message: "Explain the later section".into(),
                model: "fixture".into(),
                pictures: vec![],
                context: None,
                search: false,
                studio: None,
            },
            &mut |_| {},
        )
        .await
        .unwrap();
        assert_eq!(result.content, "The later section is available.");
        assert!(result.prompts.is_empty());
        for index in 0..4 {
            let (actual, request) = received.recv().await.unwrap();
            assert_eq!(actual, index);
            let request: Value = serde_json::from_str(&request).unwrap();
            let messages = request["messages"].as_array().unwrap();
            if index > 0 {
                let content = messages.last().unwrap()["content"].as_str().unwrap();
                match index {
                    1 => {
                        assert!(content.contains("Guide page 1/2"));
                        assert!(!content.contains("LATER_SECTION"));
                    }
                    2 => {
                        assert!(content.contains("Guide page 2/2"));
                        assert!(content.contains("LATER_SECTION"));
                    }
                    _ => assert!(content.contains("positive integer")),
                }
                if index < 3 {
                    assert!(content.contains("It is not addressed to you"));
                    assert!(content.contains("<<<GUIDE"));
                }
            }
        }
        server.await.unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prompt_routing_prefers_forge_then_compatible_basic_else_refuses() {
        let data = routing_data();
        for (name, expected) in [
            ("h3", "MiniMax H3"),
            ("MiniMax H3", "MiniMax H3"),
            ("minimax-h3", "MiniMax H3"),
            ("ltx", "LTX 2.3"),
            ("wan", "Wan 2.2"),
            ("Anima", "Anima"),
            ("MOSS-Image", "MOSS"),
            ("qwen-image-2.1", "Qwen Image 2.1"),
            ("qwen-image-2.1-turbo", "Qwen Image 2.1"),
            ("Pony", "Illustrious"),
        ] {
            assert_eq!(prompt_route(&data, name), PromptRoute::Forge(expected));
        }
        assert_eq!(prompt_route(&data, "basic"), PromptRoute::Simple("basic"));
        assert_eq!(
            prompt_route(&data, "Basic Model"),
            PromptRoute::Simple("basic")
        );
        for name in ["unknown", "guide-only", "deas-chat", "empty", ""] {
            assert_eq!(prompt_route(&data, name), PromptRoute::Missing, "{name}");
        }
        let defs = tool_definitions(&data, &no_studio());
        assert!(defs[0]["function"]["parameters"]["properties"]["model"]
            .get("enum")
            .is_none());
        let request = SendRequest {
            id: None,
            message: "test".into(),
            model: "fixture".into(),
            pictures: vec![],
            context: None,
            search: false,
            studio: None,
        };
        let system = system_prompt(&data, &request, false, false);
        assert!(!system.contains("A prompt for anything else, write yourself."));
        assert!(system.contains("Never substitute another model"));
    }

    #[test]
    fn simple_writer_compatibility_uses_the_engines_effective_default_mode() {
        let mut data = routing_data();
        let writer = serde_json::from_value(json!({
            "entry": {"modes": ["Simple"], "default_mode": "Simple", "segments_by_mode": {"Simple": ["Positive prompt"]}},
            "system": "", "system_by_mode": {"Simple": "Mode-specific instructions"}
        })).unwrap();
        data.pack.models.insert("mode-only".into(), writer);
        assert_eq!(
            prompt_route(&data, "mode-only"),
            PromptRoute::Simple("mode-only")
        );
        data.pack
            .models
            .get_mut("mode-only")
            .unwrap()
            .system_by_mode
            .insert("Simple".into(), " ".into());
        data.pack.models.get_mut("mode-only").unwrap().system = "Base is overridden".into();
        assert_eq!(prompt_route(&data, "mode-only"), PromptRoute::Missing);
    }

    #[tokio::test]
    async fn prompt_routing_executes_forge_refusal_and_simple_paths() {
        use chat::tests::{complete, fixture};
        use std::time::Duration;
        let tool_reply = |name: &str, args: Value| {
            complete(&format!(
                "data: {}\n\n",
                json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call","function":{"name":name,"arguments":args.to_string()}}]},"finish_reason":"tool_calls"}]})
            ))
        };
        for (target, expected, simple) in [
            ("h3", "Write for → MiniMax H3", false),
            ("Wan 2.2", "Write for → Wan 2.2", false),
            ("anima", "a tree in rain", true),
            (
                "NotSupported",
                "No compatible prompt writer exists for 'NotSupported'",
                false,
            ),
            ("guide-only", "No compatible prompt writer exists", false),
            ("Basic Model", "a tree in rain", true),
        ] {
            let mut responses = vec![];
            if target == "h3" {
                responses.push(tool_reply("read_guide", json!({"name":"h3"})));
            }
            responses.push(tool_reply(
                "write_prompt",
                json!({"model":target,"brief":"a tree"}),
            ));
            if simple {
                responses.push(complete(&json!({"choices":[{"message":{"content":"===SEGMENT: Positive prompt===\na tree in rain"},"finish_reason":"stop"}]}).to_string()));
            }
            let count = responses.len();
            let (url, mut received, server) = fixture(responses).await;
            let root = temp_root();
            let mut events = vec![];
            let result = tokio::time::timeout(
                Duration::from_secs(3),
                send(
                    &root,
                    &routing_data(),
                    &LlmConfig {
                        provider: Provider::Openai,
                        base_url: url,
                        api_key: String::new(),
                    },
                    SendRequest {
                        id: None,
                        message: format!("Write a {target} prompt"),
                        model: "fixture".into(),
                        pictures: vec![],
                        context: None,
                        search: false,
                        studio: None,
                    },
                    &mut |e| events.push(e),
                ),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(
                result.content.contains(expected),
                "{target}: {}",
                result.content
            );
            assert!(!result
                .content
                .contains("Which model should this prompt be for?"));
            assert_eq!(result.prompts.len(), usize::from(simple));
            if simple {
                assert_eq!(
                    result.prompts[0].target,
                    if target == "anima" { "anima" } else { "basic" }
                );
            }
            assert_eq!(
                result
                    .tools
                    .iter()
                    .filter(|t| t.starts_with("write_prompt:"))
                    .count(),
                1
            );
            let id = events
                .iter()
                .find_map(|e| match e {
                    ChatEvent::Chat { id, .. } => Some(id),
                    _ => None,
                })
                .unwrap();
            assert_eq!(load(&root, id).unwrap().messages.last(), Some(&result));
            let expected_route = if target == "anima" {
                "prompt-forge"
            } else if simple {
                "simple-writer"
            } else if expected.starts_with("Write for") {
                "prompt-forge"
            } else {
                "no-compatible-writer"
            };
            assert!(crate::logs::native_logs(0).iter().any(|entry| {
                entry.source == crate::logs::Source::Deas
                    && entry.text
                        == format!("INFO: Chat: write_prompt chat={id} route={expected_route}")
            }));
            for i in 0..count {
                assert_eq!(received.recv().await.map(|(index, _)| index), Some(i));
            }
            tokio::time::timeout(Duration::from_secs(1), server)
                .await
                .unwrap()
                .unwrap();
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[tokio::test]
    async fn stop_cancels_blocked_web_search_without_starting_llm() {
        use chat::tests::fixture;
        use std::time::Duration;
        let (url, mut received, server) = fixture(vec![String::new()]).await;
        let root = temp_root();
        search::write(
            &root,
            &search::Settings {
                search: "searxng".into(),
                searxng_url: url.clone(),
            },
        )
        .unwrap();
        let data = Data::load(&root.join("forge-data")).unwrap();
        let config = LlmConfig {
            provider: Provider::Openai,
            base_url: url,
            api_key: String::new(),
        };
        let id = Mutex::new(None);
        let mut events = vec![];
        let mut emit = |event: ChatEvent| {
            if let ChatEvent::Chat { id: chat_id, .. } = &event {
                *id.lock().unwrap() = Some(chat_id.clone());
            }
            events.push(event);
        };
        let work = send(
            &root,
            &data,
            &config,
            SendRequest {
                id: None,
                message: "search fixture".into(),
                model: "fixture".into(),
                pictures: vec![],
                context: None,
                search: true,
                studio: None,
            },
            &mut emit,
        );
        let cancel = async {
            assert_eq!(received.recv().await.map(|(index, _)| index), Some(0));
            assert!(native_chat_cancel(id.lock().unwrap().clone().unwrap()));
        };
        let (reply, ()) =
            tokio::time::timeout(Duration::from_secs(1), async { tokio::join!(work, cancel) })
                .await
                .expect("Stop must cancel the blocked search before its eight-second timeout");
        assert!(reply.unwrap().content.is_empty());
        assert!(!events
            .iter()
            .any(|e| matches!(e, ChatEvent::Searched { .. } | ChatEvent::Text { .. })));
        assert!(matches!(events.last(), Some(ChatEvent::Done { .. })));
        let chat_id = id.lock().unwrap().clone().unwrap();
        assert_eq!(load(&root, &chat_id).unwrap().messages.len(), 2);
        assert!(!native_chat_cancel(chat_id));
        tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn stop_cancels_nested_prompt_writer_and_saves_partial_reply() {
        use chat::tests::{complete, fixture};
        use std::time::Duration;
        for stalled in ["", "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n"] {
            let tool = "data: {\"choices\":[{\"delta\":{\"content\":\"Before\",\"tool_calls\":[{\"index\":0,\"id\":\"write\",\"function\":{\"name\":\"write_prompt\",\"arguments\":\"{\\\"model\\\":\\\"basic\\\",\\\"brief\\\":\\\"a tree\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n";
            let (url, mut received, server) = fixture(vec![complete(tool), stalled.into()]).await;
            let config = LlmConfig {
                provider: Provider::Openai,
                base_url: url,
                api_key: String::new(),
            };
            let data = Data {
                pack: serde_json::from_value(json!({
                    "source": {}, "config": {}, "defaults": {}, "shared": {},
                    "models": {"basic": {"entry": {"label": "Basic Model", "segments": ["Positive prompt"]}, "system": "Write a prompt"}}
                }))
                .unwrap(),
                tags: None,
                canon: None,
            };
            let root = temp_root();
            let id = Mutex::new(None);
            let mut events = vec![];
            let mut emit = |event: ChatEvent| {
                if let ChatEvent::Chat { id: chat_id, .. } = &event {
                    *id.lock().unwrap() = Some(chat_id.clone());
                }
                events.push(event);
            };
            let work = send(
                &root,
                &data,
                &config,
                SendRequest {
                    id: None,
                    message: "Write a prompt".into(),
                    model: "fixture".into(),
                    pictures: vec![],
                    context: None,
                    search: false,
                    studio: None,
                },
                &mut emit,
            );
            let cancel = async {
                assert_eq!(received.recv().await.map(|(index, _)| index), Some(0));
                assert_eq!(received.recv().await.map(|(index, _)| index), Some(1));
                assert!(native_chat_cancel(id.lock().unwrap().clone().unwrap()));
            };
            let (result, ()) =
                tokio::time::timeout(Duration::from_secs(1), async { tokio::join!(work, cancel) })
                    .await
                    .expect("stop must drop the nested engine HTTP future");
            let reply = result.unwrap();
            assert_eq!(reply.content, "Before");
            assert!(reply.prompts.is_empty());
            assert!(!events.iter().any(|e| matches!(e, ChatEvent::Prompt { .. })));
            assert!(matches!(events.last(), Some(ChatEvent::Done { .. })));
            let chat_id = id.lock().unwrap().clone().unwrap();
            assert_eq!(load(&root, &chat_id).unwrap().messages.len(), 2);
            assert!(!native_chat_cancel(chat_id));
            tokio::time::timeout(Duration::from_secs(1), server)
                .await
                .unwrap()
                .unwrap();
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    fn studio(checkpoint: &str, dialect: &str, blocked: Option<&str>) -> Option<StudioState> {
        Some(StudioState {
            checkpoint: checkpoint.into(),
            dialect: Some(dialect.into()),
            blocked: blocked.map(String::from),
            endpoint: None,
            writer_model: None,
        })
    }

    #[test]
    fn render_tool_is_offered_only_when_generate_can_draw() {
        let data = routing_data();
        let names = |s: Option<StudioState>| {
            let req = SendRequest {
                studio: s,
                ..no_studio()
            };
            tool_definitions(&data, &req)
                .iter()
                .map(|d| d["function"]["name"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        };
        assert!(
            names(studio("anima-v1.safetensors", "Anima", None)).contains(&"render_picture".into())
        );
        assert!(!names(None).contains(&"render_picture".into()));
        assert!(!names(studio(
            "anima-v1.safetensors",
            "Anima",
            Some("Start the backend.")
        ))
        .contains(&"render_picture".into()));
        assert!(
            !names(studio("ltx.safetensors", "LTX 2.3", None)).contains(&"render_picture".into())
        );
        let blocked = system_prompt(
            &data,
            &SendRequest {
                studio: studio("a", "Anima", Some("Start the backend.")),
                ..no_studio()
            },
            false,
            false,
        );
        assert!(blocked.contains("You cannot make pictures right now: Start the backend."));
    }

    #[tokio::test]
    async fn render_picture_writes_with_the_checkpoints_writer_and_records_the_result() {
        use chat::tests::{complete, fixture};
        use std::time::Duration;
        let call = |args: Value| {
            complete(&format!(
                "data: {}\n\n",
                json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"r","function":{"name":"render_picture","arguments":args.to_string()}}]},"finish_reason":"tool_calls"}]})
            ))
        };
        let writer = || {
            complete(&json!({"choices":[{"message":{"content":"===SEGMENT: Positive prompt===\na fox in snow"},"finish_reason":"stop"}]}).to_string())
        };
        let config = |url| LlmConfig {
            provider: Provider::Openai,
            base_url: url,
            api_key: String::new(),
        };
        let root = temp_root();
        let data = routing_data();
        let req = |id: Option<String>, message: &str| SendRequest {
            id,
            message: message.into(),
            model: "fixture".into(),
            studio: studio("anima-v1.safetensors", "Anima", None),
            ..no_studio()
        };

        // Written by the Anima writer, marked for Generate, with its size ask.
        let (url, _rx, server) = fixture(vec![
            call(json!({"brief":"a fox","aspect":"16:9"})),
            writer(),
        ])
        .await;
        let first = tokio::time::timeout(
            Duration::from_secs(3),
            send(
                &root,
                &data,
                &config(url),
                req(None, "draw a fox"),
                &mut |_| {},
            ),
        )
        .await
        .unwrap()
        .unwrap();
        server.await.unwrap();
        assert_eq!(first.prompts.len(), 1);
        assert_eq!(first.prompts[0].target, "anima");
        assert_eq!(first.prompts[0].prompt, "a fox in snow");
        assert_eq!(
            first.prompts[0].render.as_ref().unwrap().aspect.as_deref(),
            Some("16:9")
        );
        assert!(first
            .content
            .contains("Drawing it with anima-v1.safetensors"));
        let id = list(&root)[0].id.clone();

        // The page records the picture; the history says so in words.
        let recorded = record_render(
            &root,
            &id,
            first.at,
            0,
            RenderResult {
                path: Some("C:/out/a.png".into()),
                width: 1344,
                height: 768,
                seed: Some(7),
                checkpoint: "anima-v1.safetensors".into(),
                ms: Some(9000),
                error: None,
            },
        )
        .unwrap();
        assert!(recorded
            .content
            .ends_with("[The picture was drawn: 1344×768, anima-v1.safetensors, seed 7.]"));
        assert!(record_render(&root, &id, first.at, 3, RenderResult::default()).is_err());

        // same_prompt draws that prompt again without a writer call.
        let (url, _rx, server) =
            fixture(vec![call(json!({"brief":"again","same_prompt":true}))]).await;
        let again = tokio::time::timeout(
            Duration::from_secs(3),
            send(
                &root,
                &data,
                &config(url),
                req(Some(id.clone()), "render that again"),
                &mut |_| {},
            ),
        )
        .await
        .unwrap()
        .unwrap();
        server.await.unwrap();
        assert_eq!(again.prompts[0].prompt, "a fox in snow");
        assert!(again.prompts[0].picture.is_none());
        // No size asked: the last picture's 16:9 carries over.
        assert_eq!(
            again.prompts[0].render.as_ref().unwrap().aspect.as_deref(),
            Some("16:9")
        );

        // A writer stuck repeating itself: nothing kept, nothing drawn, a way forward.
        // Distinct phrases: the clean-up would fold plain repeats away.
        let looped = (0..2500)
            .map(|i| format!("the fox turns to look at the snow drift number {i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let runaway_writer = complete(&json!({"choices":[{"message":{"content":format!("===SEGMENT: Positive prompt===\n{looped}")},"finish_reason":"length"}]}).to_string());
        let (url, _rx, server) =
            fixture(vec![call(json!({"brief":"a fox at dawn"})), runaway_writer]).await;
        let ran = tokio::time::timeout(
            Duration::from_secs(3),
            send(
                &root,
                &data,
                &config(url),
                req(Some(id.clone()), "a fox at dawn"),
                &mut |_| {},
            ),
        )
        .await
        .unwrap()
        .unwrap();
        server.await.unwrap();
        assert!(ran.prompts.is_empty());
        assert!(
            ran.content.contains("got stuck repeating itself"),
            "{}",
            ran.content
        );

        // The checkpoint's own file name as "model" is not another model.
        let (url, _rx, server) = fixture(vec![
            call(json!({"brief":"a fox at dusk","model":"anima-v1.safetensors"})),
            writer(),
        ])
        .await;
        let named = tokio::time::timeout(
            Duration::from_secs(3),
            send(
                &root,
                &data,
                &config(url),
                req(Some(id.clone()), "a fox at dusk"),
                &mut |_| {},
            ),
        )
        .await
        .unwrap()
        .unwrap();
        server.await.unwrap();
        assert_eq!(named.prompts.len(), 1, "{}", named.content);

        // A model the selected checkpoint is not: no write, a way forward.
        let (url, _rx, server) =
            fixture(vec![call(json!({"brief":"a fox","model":"MiniMax H3"}))]).await;
        let other = tokio::time::timeout(
            Duration::from_secs(3),
            send(
                &root,
                &data,
                &config(url),
                req(Some(id), "draw it with h3"),
                &mut |_| {},
            ),
        )
        .await
        .unwrap()
        .unwrap();
        server.await.unwrap();
        assert!(other.prompts.is_empty());
        assert!(
            other
                .content
                .contains("Select a MiniMax H3 checkpoint in Studio → Generate"),
            "{}",
            other.content
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn conversations_save_list_newest_first_and_refuse_bad_ids() {
        let root = temp_root();
        let mk = |id: &str, updated: u64| Chat {
            id: id.into(),
            title: format!("t{updated}"),
            model: "m".into(),
            created: 1,
            updated,
            pinned: false,
            messages: vec![],
        };
        let a = "00000000-0000-4000-8000-000000000001";
        let b = "00000000-0000-4000-8000-000000000002";
        save(&root, &mk(a, 5)).unwrap();
        save(&root, &mk(b, 9)).unwrap();
        let l = list(&root);
        assert_eq!(
            l.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec![b, a]
        );
        // Pinned goes first even when older, and pinning does not touch "updated".
        let mut older = load(&root, a).unwrap();
        older.pinned = true;
        save(&root, &older).unwrap();
        let l = list(&root);
        assert_eq!(
            l.iter()
                .map(|s| (s.id.as_str(), s.pinned))
                .collect::<Vec<_>>(),
            vec![(a, true), (b, false)]
        );
        assert_eq!(load(&root, a).unwrap().updated, 5);
        assert_eq!(load(&root, a).unwrap().title, "t5");
        assert!(load(&root, "../../etc/passwd").is_err());
        assert!(file(&root, "x").is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn titles_dates_and_the_prompt_reply() {
        assert_eq!(
            title_from("\n  how does ltx prompting work?  \nmore", 0),
            "how does ltx prompting work?"
        );
        assert_eq!(title_from("", 2), "A picture");
        assert!(title_from(&"a".repeat(80), 0).ends_with('…'));
        let d = today();
        assert_eq!(d.len(), 10);
        assert!(d.starts_with("20"));
        let r = prompt_reply("Anima", "1girl, rain", Some("lowres"));
        assert!(r.starts_with("Anima prompt, written by the Anima prompt writer:"));
        assert!(r.contains("Positive prompt:\n1girl, rain\n\nNegative prompt:\nlowres"));
    }

    #[test]
    fn tool_replies_go_back_as_the_call_and_its_result() {
        let msg = |role: &str, content: &str, prompts: Vec<WrittenPrompt>| Message {
            role: role.into(),
            content: content.into(),
            pictures: 0,
            prompts,
            citations: vec![],
            tools: vec![],
            at: 1,
        };
        let drawn = WrittenPrompt {
            target: "krea2".into(),
            label: "Krea2".into(),
            prompt: "a cat girl in a neon bodysuit".into(),
            negative: None,
            brief: "cat girl, neon bodysuit, red/pink hair".into(),
            render: Some(RenderAsk {
                aspect: Some("2:3".into()),
                resolution: None,
            }),
            picture: None,
        };
        let chat = Chat {
            id: "x".into(),
            title: "t".into(),
            model: "m".into(),
            created: 0,
            updated: 0,
            pinned: false,
            messages: vec![
                msg("user", "draw a cat girl", vec![]),
                msg(
                    "assistant",
                    "Krea2 prompt, written by the Krea2 prompt writer:\n\na cat girl",
                    vec![drawn],
                ),
                msg("user", "make the highlights teal", vec![]),
            ],
        };
        let out = history(&chat, &[], None, false);
        assert_eq!(out.len(), 4);
        assert_eq!(out[1]["role"], "assistant");
        assert_eq!(out[1]["content"], "");
        assert_eq!(
            out[1]["tool_calls"][0]["function"]["name"],
            "render_picture"
        );
        let args: Value = serde_json::from_str(
            out[1]["tool_calls"][0]["function"]["arguments"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(args["brief"], "cat girl, neon bodysuit, red/pink hair");
        assert_eq!(args["aspect"], "2:3");
        assert!(args.get("resolution").is_none());
        assert_eq!(out[2]["role"], "tool");
        assert!(out[2]["content"]
            .as_str()
            .unwrap()
            .contains("written by the Krea2 prompt writer"));
        assert_eq!(out[3]["content"], "make the highlights teal");
    }

    #[test]
    fn history_sends_only_this_turns_pictures_and_puts_search_first() {
        let chat = Chat {
            id: "x".into(),
            title: "t".into(),
            model: "m".into(),
            created: 0,
            updated: 0,
            pinned: false,
            messages: vec![
                Message {
                    role: "user".into(),
                    content: "old".into(),
                    pictures: 2,
                    prompts: vec![],
                    citations: vec![],
                    tools: vec![],
                    at: 0,
                },
                Message {
                    role: "assistant".into(),
                    content: "ok".into(),
                    pictures: 0,
                    prompts: vec![],
                    citations: vec![],
                    tools: vec![],
                    at: 0,
                },
                Message {
                    role: "user".into(),
                    content: "new".into(),
                    pictures: 1,
                    prompts: vec![],
                    citations: vec![],
                    tools: vec![],
                    at: 0,
                },
            ],
        };
        let h = history(
            &chat,
            &["data:image/png;base64,AAAA".into()],
            Some("RESULTS"),
            false,
        );
        assert!(h[0]["content"].as_str().unwrap().contains("no longer held"));
        assert_eq!(h[1]["role"], "assistant");
        assert_eq!(h[2]["content"][0]["text"], "RESULTS\n\nThe question:\nnew");
        assert_eq!(h[2]["content"][1]["type"], "image_url");
    }

    /// The guides and tools against the real pack.
    /// `DEAS_FORGE_DATA=<dir> cargo test --lib -- --ignored real_chat_guides --nocapture`
    #[test]
    #[ignore = "needs the data pack"]
    fn real_chat_guides() {
        let data = Data::load(&data_dir(Path::new(""))).unwrap();
        let names: Vec<String> = guides(&data).into_iter().map(|(k, _)| k).collect();
        println!("{names:?}");
        for want in ["illustrious", "moss", "ltx", "wan", "h3", "deas-prompting"] {
            assert!(names.iter().any(|n| n == want), "{want}");
        }
        assert!(read_guide(&data, "LTX", None).unwrap().len() > 1000);
        assert!(read_guide(&data, "nope", None).is_err());
        let defs = tool_definitions(&data, &no_studio());
        assert_eq!(
            defs.iter()
                .map(|d| d["function"]["name"].as_str().unwrap())
                .collect::<Vec<_>>(),
            if data.tags.is_some() {
                vec!["write_prompt", "read_guide", "search_tags"]
            } else {
                vec!["write_prompt", "read_guide"]
            }
        );
        assert!(data.pack.shared.chat.as_deref().unwrap_or("").len() > 1000);
        for (key, mode) in [("h3", "MiniMax H3"), ("wan", "Wan 2.2"), ("ltx", "LTX 2.3")] {
            assert_eq!(prompt_route(&data, key), PromptRoute::Forge(mode));
        }
        let req = SendRequest {
            id: None,
            message: "test".into(),
            model: "fixture".into(),
            pictures: vec![],
            context: None,
            search: false,
            studio: None,
        };
        assert!(!system_prompt(&data, &req, false, false)
            .contains("A prompt for\nanything else, write yourself."));
    }

    /// A real conversation against a live server, printed in full:
    /// `DEAS_FORGE_DATA=<dir> DEAS_FORGE_LLM=<url> DEAS_FORGE_MODEL=<id> cargo test --lib -- --ignored live_chat --nocapture`
    #[tokio::test]
    #[ignore = "needs a live LLM server and the data pack"]
    async fn live_chat() {
        let data = Data::load(&PathBuf::from(std::env::var("DEAS_FORGE_DATA").unwrap())).unwrap();
        let config = LlmConfig {
            provider: Provider::Openai,
            base_url: std::env::var("DEAS_FORGE_LLM").unwrap(),
            api_key: String::new(),
        };
        let model = std::env::var("DEAS_FORGE_MODEL").unwrap();
        let root = temp_root();
        let mut id = None;
        for q in [
            "how does LTX prompting work? keep it short",
            "write me an illustrious prompt: frieren reading a book under a tree",
        ] {
            let mut events = vec![];
            let mut emit = |e: ChatEvent| events.push(e);
            let m = send(
                &root,
                &data,
                &config,
                SendRequest {
                    id: id.clone(),
                    message: q.into(),
                    model: model.clone(),
                    pictures: vec![],
                    context: None,
                    search: false,
                    studio: None,
                },
                &mut emit,
            )
            .await
            .unwrap();
            id = events.iter().find_map(|e| {
                if let ChatEvent::Chat { id, .. } = e {
                    Some(id.clone())
                } else {
                    None
                }
            });
            let texts = events
                .iter()
                .filter(|e| matches!(e, ChatEvent::Text { .. }))
                .count();
            println!(
                "=== {q}\n[tools {:?} | {texts} streamed pieces | prompts {}]\n{}\n",
                m.tools,
                m.prompts.len(),
                m.content
            );
        }
        println!(
            "history: {:?}",
            list(&root)
                .iter()
                .map(|s| (&s.title, s.count))
                .collect::<Vec<_>>()
        );
        std::fs::remove_dir_all(&root).ok();
    }
}

#[cfg(test)]
mod acl_tests {
    /// Every command main.rs registers must be allowed by a permission the main window has.
    /// Tauri refuses the rest at run time ("not allowed by ACL"), which no unit test sees.
    #[test]
    fn every_registered_command_is_allowed_for_the_main_window() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let main = std::fs::read_to_string(root.join("src/main.rs")).unwrap();
        let start = main.find("generate_handler![").expect("the handler list");
        let list = &main[start..start + main[start..].find(']').unwrap()];
        let registered: Vec<&str> = list
            .split([',', '\n', ' ', '['])
            .filter_map(|p| p.trim().rsplit("::").next())
            .filter(|n| n.starts_with("native_") || n.starts_with("inspect_"))
            .collect();
        let caps: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("capabilities/main.json")).unwrap(),
        )
        .unwrap();
        let granted: Vec<String> = caps["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p.as_str().map(String::from))
            .collect();
        let mut allowed = String::new();
        for entry in std::fs::read_dir(root.join("permissions"))
            .unwrap()
            .flatten()
        {
            let text = std::fs::read_to_string(entry.path()).unwrap();
            let doc: toml::Value = toml::from_str(&text).unwrap();
            for p in doc["permission"].as_array().unwrap() {
                if granted.iter().any(|g| p["identifier"].as_str() == Some(g)) {
                    allowed.push_str(&p["commands"]["allow"].to_string());
                }
            }
        }
        let missing: Vec<&&str> = registered
            .iter()
            .filter(|n| !allowed.contains(&format!("\"{n}\"")))
            .collect();
        assert!(registered.len() > 20, "{registered:?}");
        assert!(
            missing.is_empty(),
            "registered but not allowed for the main window: {missing:?}"
        );
    }
}
