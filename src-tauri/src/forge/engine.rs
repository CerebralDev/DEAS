//! One Prompt Forge write for an image model, start to finish: character
//! canon, the system prompt for the segments asked for, the writing loop with
//! the tag-search and guidance tools, the segment contract, then the
//! mechanical clean-up. Ported from PromptForge's handleGenerate.

use super::canon::{character_block, CanonTable, Corrections};
use super::character::{self, Resolved};
use super::message::{self, Controls};
use super::pack::{segment_tail, Keys, Pack, PackModel, SEPARATOR};
use super::refs;
use super::segments::{self, Segment};
use super::{chat, clean, tools};
use crate::llm::{LlmConfig, Provider};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) const ROUND_TIMEOUT: Duration = Duration::from_secs(600);
/// The most a write may run when no length was asked for: several times any real prompt.
pub(crate) const WRITE_CEILING: u32 = 8192;

/// A write that hit the ceiling without a length asked for did not stop on its own.
fn runaway() -> String {
    format!("The writer did not stop: it ran past {WRITE_CEILING} tokens without finishing, which usually means the model got stuck repeating itself. Nothing was kept. Try again, or pick a bigger model.")
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteRequest {
    /// `illustrious`, `anima`, `krea2` or `moss`.
    pub target: String,
    pub brief: String,
    /// The LLM model id on the configured server.
    pub model: String,
    #[serde(default)]
    pub mode: Option<String>,
    /// Optional segments to write. Absent = PromptForge's default for a person
    /// who never touched the switches.
    #[serde(default)]
    pub segments_on: Option<Vec<String>>,
    #[serde(default)]
    pub controls: Controls,
}

#[derive(Debug, Serialize)]
pub struct ToolUse {
    pub name: String,
    pub queries: Vec<String>,
    pub document: Option<String>,
    pub ok: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteResult {
    pub segments: Vec<Segment>,
    pub refused: bool,
    /// The prompt itself (Positive prompt / Enhanced prompt).
    pub prompt: String,
    pub negative: Option<String>,
    pub characters: Vec<String>,
    pub tools: Vec<ToolUse>,
    pub rounds: usize,
    pub warnings: Vec<String>,
    pub fixes: Vec<String>,
    /// Segments a revision rewrote; 0 on a first write.
    pub changed: usize,
    /// What the reference pictures were read as, which the prompt was written from.
    pub reference_analysis: Option<String>,
}

/// Where the data pack and databases live: `DEAS_FORGE_DATA`, else
/// `<environment>/forge-data`.
pub fn data_dir(root: &Path) -> PathBuf {
    std::env::var_os("DEAS_FORGE_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("forge-data"))
}

fn log(text: &str) {
    crate::logs::push(
        crate::logs::Source::Deas,
        &format!("INFO: Prompt Forge: {text}"),
    );
}

pub struct Data {
    pub pack: Pack,
    pub tags: Option<super::tags::TagStore>,
    pub canon: Option<CanonTable>,
}

impl Data {
    pub fn load(dir: &Path) -> Result<Self, String> {
        let pack = Pack::load(&dir.join("forge-pack.json"))?;
        let tags = super::tags::TagStore::open(&dir.join("tags.sqlite"))
            .inspect_err(|e| {
                log(&format!(
                    "no tag database, tools and tag checks are off ({e})"
                ))
            })
            .ok();
        let corrections: Corrections = std::fs::read(dir.join("character-corrections.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let canon = CanonTable::open(&dir.join("character_traits.sqlite"), corrections)
            .inspect_err(|e| log(&format!("no character table, canon is off ({e})")))
            .ok();
        Ok(Self { pack, tags, canon })
    }
}

fn spared(controls: &Controls) -> Vec<String> {
    let mut v = vec![controls.positive.clone().unwrap_or_default()];
    v.extend(controls.tags.iter().cloned());
    v
}

/// Everything a first pass is given, so a revision judges the draft on the same terms.
struct Prepared<'a> {
    model: &'a PackModel,
    mode: Option<String>,
    controls: Controls,
    wanted: Vec<String>,
    discard: Vec<String>,
    resolved: Vec<Resolved>,
    offered: Vec<Value>,
    system: String,
    user: String,
    temperature: Option<f64>,
    top_p: Option<f64>,
    max_tokens: Option<u32>,
    /// No length was asked for: `max_tokens` is only the runaway ceiling.
    ceiling_only: bool,
}

/// What the reference pictures contribute to the user turn.
#[derive(Default)]
struct RefState {
    /// The reading pass's text, when it ran and was usable.
    analysis: Option<String>,
    /// The pictures ride with the write itself (no usable reading).
    carries: bool,
}

fn prepare<'a>(
    data: &'a Data,
    req: &WriteRequest,
    refs_state: &RefState,
) -> Result<Prepared<'a>, String> {
    let pack = &data.pack;
    let (model, mode) = validate_request(data, req)?;
    // An unset control means the model's default, as PromptForge's form always sends one:
    // a quality preset left out entirely lets the model add a ladder of its own.
    let mut filled = req.controls.clone();
    if filled.quality_preset.is_none()
        && model
            .entry
            .get("quality_presets")
            .is_some_and(Value::is_object)
    {
        filled.quality_preset = Some(
            model
                .str("default_quality_preset")
                .unwrap_or("none")
                .to_string(),
        );
    }
    if filled.style.is_none() {
        filled.style = model.str("default_style").map(String::from);
    }
    let controls = &filled;
    let tag_style = model.tag_style();
    // Without a length asked for, a write still gets a ceiling (8 Nov 2026: an 8B wrote
    // 131,772 characters, filling its whole context over six minutes). Real prompts,
    // MOSS Detail 10 included, stay far below it; PromptForge's own safety net is a
    // two-minute generation limit, which this replaces with a token count.
    let asked = message::token_budget(controls)?;
    let ceiling_only = asked.is_none();
    let max_tokens = Some(asked.unwrap_or(WRITE_CEILING));

    // The contract for this request, minus what is switched off.
    let expected = model.expected(mode.as_deref());
    let optional = model.optional();
    let default_off = pack
        .defaults
        .get("optional_segments_default")
        .and_then(Value::as_str)
        == Some("off");
    let wanted: Vec<String> = expected
        .iter()
        .filter(|l| {
            !optional.contains(l)
                || req
                    .segments_on
                    .as_ref()
                    .map_or(!default_off, |on| on.contains(l))
        })
        .cloned()
        .collect();
    let discard: Vec<String> = expected
        .iter()
        .filter(|l| !wanted.contains(l))
        .cloned()
        .collect();

    // Character canon, from the local tables only.
    let characters_on = pack
        .config
        .get("characters")
        .and_then(|c| c.get("enabled"))
        .and_then(Value::as_bool)
        != Some(false);
    let (resolved, ambiguous): (Vec<Resolved>, Vec<character::Ambiguous>) =
        match (&data.tags, characters_on && controls.tags.is_empty()) {
            (Some(store), true) => {
                character::resolve(&req.brief, store, &character::Options::default())
            }
            _ => (vec![], vec![]),
        };
    let character_text = character_block(
        data.canon.as_ref(),
        data.tags.as_ref(),
        &resolved,
        &ambiguous,
        tag_style == Some("underscore"),
    );
    for r in &resolved {
        log(&format!("{}: {} via {}", req.target, r.tag, r.via));
    }

    // Tools, when the model takes them and there is a database behind them.
    let tools_on = pack
        .config
        .get("tools")
        .and_then(|t| t.get("enabled"))
        .and_then(Value::as_bool)
        != Some(false);
    let mut offered: Vec<Value> = vec![];
    if tools_on && model.flag("tools") && data.tags.is_some() {
        offered.extend(model.search_tool.iter().cloned());
    }
    if tools_on && !model.guidance_docs.is_empty() {
        offered.extend(model.guidance_tool.iter().cloned());
    }
    let offers = |name: &str| offered.iter().any(|t| t["function"]["name"] == name);
    let guide: Vec<&str> = [
        offers("search_tags")
            .then_some(pack.shared.tag_search.as_deref())
            .flatten(),
        offers("read_guidance")
            .then_some(pack.shared.guidance.as_deref())
            .flatten(),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut system = [
        model.system_for(mode.as_deref()).to_string(),
        segment_tail(pack, model, &wanted),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join(SEPARATOR);
    if !guide.is_empty() {
        system = format!("{system}{SEPARATOR}{}", guide.join(SEPARATOR));
    }

    let canvas = canvas_line(model, mode.as_deref(), controls);
    let negative_written = wanted.iter().any(|l| l.to_lowercase().contains("negative"));
    // The reference notes (and the reading under them) come before the canon, as in PromptForge.
    let refs_block = refs::block(
        &controls.image_references,
        pack,
        tag_style == Some("underscore"),
        refs_state.carries,
        refs_state.analysis.as_deref(),
    );
    let tail = [refs_block, character_text]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    let user = message::build(
        pack,
        model,
        &req.brief,
        controls,
        negative_written,
        &tail,
        canvas,
    );
    let (temperature, top_p) = message::sampling(pack, model, controls.creativity);
    Ok(Prepared {
        model,
        mode,
        wanted,
        discard,
        resolved,
        offered,
        system,
        user,
        temperature,
        top_p,
        max_tokens,
        ceiling_only,
        controls: filled,
    })
}

pub async fn write(
    data: &Data,
    config: &LlmConfig,
    req: &WriteRequest,
) -> Result<WriteResult, String> {
    // Validate consumed geometry before reference-reading can issue a vision request.
    validate_request(data, req)?;
    let pack = &data.pack;
    let mut warnings = vec![];
    let (refs_state, carried) = read_references(data, config, req, &mut warnings).await;
    let Prepared {
        model,
        mode,
        controls,
        wanted,
        discard,
        resolved,
        offered,
        system,
        user,
        temperature,
        top_p,
        max_tokens,
        ceiling_only,
    } = prepare(data, req, &refs_state)?;
    let controls = &controls;
    let tag_style = model.tag_style();
    let mut fixes = vec![];

    let max_rounds = pack
        .config
        .get("tools")
        .and_then(|t| t.get("max_rounds"))
        .and_then(Value::as_u64)
        .unwrap_or(3) as usize;
    let max_calls = pack
        .config
        .get("tools")
        .and_then(|t| t.get("max_calls"))
        .and_then(Value::as_u64)
        .unwrap_or(24) as usize;
    let ollama = config.provider == Provider::Ollama;
    let mut messages = vec![
        json!({ "role": "system", "content": system }),
        chat::user_with_images(&user, &carried, ollama),
    ];
    let mut used: Vec<ToolUse> = vec![];
    let mut rounds = 0;
    let mut tools_left = !offered.is_empty();
    let reply = loop {
        let reply = chat::round(chat::Round {
            config,
            model: &req.model,
            messages: &messages,
            tools: tools_left.then_some(offered.as_slice()),
            temperature,
            top_p,
            max_tokens,
            timeout: ROUND_TIMEOUT,
        })
        .await?;
        if !tools_left || reply.calls.is_empty() {
            break reply;
        }
        rounds += 1;
        messages.push(chat::assistant_turn(&reply.content, &reply.calls, ollama));
        for call in &reply.calls {
            let out = if call.name == "read_guidance" {
                tools::read_guidance(model, &call.arguments)
            } else {
                match &data.tags {
                    Some(store) => {
                        tools::search_tags(store, &call.arguments, tag_style == Some("space"))
                    }
                    None => tools::Outcome {
                        ok: false,
                        content: "The tag database is unavailable.".into(),
                        queries: vec![],
                        document: None,
                    },
                }
            };
            log(&format!(
                "{}: {}{}{}",
                req.target,
                call.name,
                out.document
                    .as_deref()
                    .map(|d| format!(" → {d}"))
                    .unwrap_or_default(),
                if out.queries.is_empty() {
                    String::new()
                } else {
                    format!(" → {}", out.queries.join(", "))
                }
            ));
            messages.push(chat::tool_result(call, &out.content));
            used.push(ToolUse {
                name: call.name.clone(),
                queries: out.queries,
                document: out.document,
                ok: out.ok,
            });
        }
        // Checked after the results go back, never before: a model left waiting
        // on an unanswered call writes nothing.
        if rounds >= max_rounds || used.len() >= max_calls {
            tools_left = false;
            messages.push(json!({
                "role": "user",
                "content": "You have used all the tag searches available for this prompt. Do not call search_tags again. Write the finished prompt now, in the exact segment format your instructions specify, using the tags you have already confirmed."
            }));
        }
    };

    if reply.truncated && ceiling_only {
        return Err(runaway());
    }
    let wanted_refs: Vec<&str> = wanted.iter().map(String::as_str).collect();
    let discard_refs: Vec<&str> = discard.iter().map(String::as_str).collect();
    let (mut segs, refused) = segments::parse(&reply.content, &wanted_refs, &discard_refs)
        .map_err(|e| {
            if reply.truncated {
                format!("The answer was cut off before it finished. {e}")
            } else {
                e.to_string()
            }
        })?;

    if !refused {
        post_process(
            data,
            model,
            mode.as_deref(),
            controls,
            &req.brief,
            &mut segs,
            &mut warnings,
            &mut fixes,
        );
    }
    for f in &fixes {
        log(&format!("{}: {f}", req.target));
    }

    let (prompt, negative) = prompt_and_negative(model, &segs);
    Ok(WriteResult {
        segments: segs,
        refused,
        prompt,
        negative,
        characters: resolved.into_iter().map(|r| r.tag).collect(),
        tools: used,
        rounds,
        warnings,
        fixes,
        changed: 0,
        reference_analysis: refs_state.analysis,
    })
}

/// The person names one change; everything else comes back word for word.
const REFINE_INSTRUCTION: &str = "The person has read the prompt you wrote and wants one change. Make that change and only that change. \
Every segment, shot and line the request does not touch comes back exactly as it is, word for word. \
Where the change has to reach into something else - a later cut that moves, a timing that shifts - change only what it must. \
Return the whole prompt in the same segment format.";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviseRequest {
    #[serde(flatten)]
    pub write: WriteRequest,
    /// The draft on screen, as the person may have edited it.
    pub segments: Vec<Segment>,
    /// Refine: the one change. Absent: a second pass.
    #[serde(default)]
    pub instruction: Option<String>,
    /// The reading the draft was written from, so the revision judges it on the same terms.
    #[serde(default)]
    pub reference_analysis: Option<String>,
}

/// Refine or Second pass (PromptForge's handleRevise): the model sees its draft under the
/// same system prompt and user turn as the first pass, with the revision instruction last.
pub async fn revise(
    data: &Data,
    config: &LlmConfig,
    req: &ReviseRequest,
) -> Result<WriteResult, String> {
    let refine = req
        .instruction
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let instruction = match refine {
        Some(_) => REFINE_INSTRUCTION,
        None => data.pack.shared.second_pass.as_deref().ok_or(
            "Second pass is unavailable: the Prompt Forge data has no second-pass instruction.",
        )?,
    };
    let refs_state = RefState {
        analysis: req.reference_analysis.clone().filter(|a| refs::usable(a)),
        carries: false,
    };
    let p = prepare(data, &req.write, &refs_state)?;
    let meta = p.model.strings("meta_segments");
    let revisable: Vec<&Segment> = req
        .segments
        .iter()
        .filter(|s| !meta.contains(&s.label) && !s.body.trim().is_empty())
        .collect();
    if revisable.is_empty() {
        return Err("Nothing to revise.".into());
    }
    // A pasted style comes off the draft and goes back on after (post_process).
    let style = message::pasted_style(p.model, &p.controls);
    let draft = revisable
        .iter()
        .map(|s| {
            let body = style
                .as_deref()
                .map_or(s.body.clone(), |b| s.body.replace(b, ""));
            format!("===SEGMENT: {}===\n{}", s.label, body.trim())
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let ask = refine.map_or("Revise it.".to_string(), |r| format!("Change this: {r}"));
    let messages = vec![
        json!({ "role": "system", "content": format!("{}{SEPARATOR}{instruction}", p.system) }),
        json!({ "role": "user", "content": p.user }),
        json!({ "role": "assistant", "content": draft }),
        json!({ "role": "user", "content": ask }),
    ];
    // Lower than a draft's: this is an edit, and a first draft's sampling makes it drift.
    let reply = chat::round(chat::Round {
        config,
        model: &req.write.model,
        messages: &messages,
        tools: None,
        temperature: Some(0.3),
        top_p: None,
        max_tokens: p.max_tokens,
        timeout: ROUND_TIMEOUT,
    })
    .await?;
    if reply.truncated && p.ceiling_only {
        return Err(runaway());
    }
    let given: Vec<&str> = req.segments.iter().map(|s| s.label.as_str()).collect();
    let discard: Vec<&str> = p.discard.iter().map(String::as_str).collect();
    let (parsed, refused) = segments::parse(&reply.content, &given, &discard).map_err(|e| {
        let why = if reply.truncated {
            "it was cut off"
        } else {
            "it did not follow the segment format"
        };
        format!(
            "The revision could not be read ({why}: {e}). Your prompt is unchanged - try again."
        )
    })?;
    if refused {
        return Err("The model refused the revision. Your prompt is unchanged.".into());
    }
    // Only a segment that came back with content is swapped in, and never a meta one.
    let mut out: Vec<Segment> = req
        .segments
        .iter()
        .map(|s| {
            let next = parsed
                .iter()
                .find(|r| r.label == s.label)
                .map(|r| r.body.trim())
                .filter(|b| !b.is_empty() && !meta.contains(&s.label));
            Segment {
                label: s.label.clone(),
                body: next.map_or(s.body.clone(), String::from),
            }
        })
        .collect();
    let (mut warnings, mut fixes) = (vec![], vec![]);
    post_process(
        data,
        p.model,
        p.mode.as_deref(),
        &p.controls,
        &req.write.brief,
        &mut out,
        &mut warnings,
        &mut fixes,
    );
    let changed = out
        .iter()
        .zip(&req.segments)
        .filter(|(a, b)| a.body != b.body)
        .count();
    log(&format!(
        "{}: {} rewrote {changed} of {} segment(s)",
        req.write.target,
        if refine.is_some() {
            "refine"
        } else {
            "second pass"
        },
        revisable.len()
    ));
    let (prompt, negative) = prompt_and_negative(p.model, &out);
    Ok(WriteResult {
        segments: out,
        refused: false,
        prompt,
        negative,
        characters: p.resolved.into_iter().map(|r| r.tag).collect(),
        tools: vec![],
        rounds: 0,
        warnings,
        fixes,
        changed,
        reference_analysis: refs_state.analysis,
    })
}

/// The prompt Generate takes and the negative, out of a set of segments.
fn prompt_and_negative(model: &PackModel, segs: &[Segment]) -> (String, Option<String>) {
    let meta = model.strings("meta_segments");
    let prompt = segs
        .iter()
        .find(|s| {
            !meta.contains(&s.label)
                && !s.label.to_lowercase().contains("negative")
                && s.label != "Layout JSON"
        })
        .map(|s| s.body.clone())
        .unwrap_or_default();
    let negative = segs
        .iter()
        .find(|s| s.label == "Negative prompt")
        .map(|s| s.body.clone());
    (prompt, negative)
}

/// Look first, write second: read the pictures under their roles before the write.
/// Returns the reading (or that the pictures must ride with the write) and the
/// pictures to attach to the write, which is none when the reading is usable.
async fn read_references(
    data: &Data,
    config: &LlmConfig,
    req: &WriteRequest,
    warnings: &mut Vec<String>,
) -> (RefState, Vec<String>) {
    let pack = &data.pack;
    let (sent, dropped) = refs::sendable_refs(&req.controls.image_references, pack);
    if dropped > 0 {
        warnings.push(format!(
            "{dropped} reference picture(s) could not be sent to the model. Their notes still travel."
        ));
    }
    if sent.is_empty() {
        return (RefState::default(), vec![]);
    }
    let images: Vec<String> = sent.iter().map(|r| r.image.clone()).collect();
    let Some(system) = pack.shared.reference_blueprint.as_deref() else {
        return (
            RefState {
                analysis: None,
                carries: true,
            },
            images,
        );
    };
    if chat::is_local(&req.model) && !chat::local_has_vision(&req.model) {
        // No projector paired next to the GGUF: keep writing from the brief
        // rather than failing the whole Forge write; with a paired mmproj the
        // pictures ride the rounds like on any server model.
        warnings.push(
            "The local GGUF runner cannot read pictures, so this was written from the brief alone."
                .into(),
        );
        return (
            RefState {
                analysis: None,
                carries: false,
            },
            vec![],
        );
    }
    let ollama = config.provider == Provider::Ollama;
    let question = refs::blueprint_message(&sent, &req.brief, pack);
    let messages = vec![
        json!({ "role": "system", "content": system }),
        chat::user_with_images(&question, &images, ollama),
    ];
    let started = std::time::Instant::now();
    let read = chat::round(chat::Round {
        config,
        model: &req.model,
        messages: &messages,
        tools: None,
        temperature: Some(refs::BLUEPRINT_TEMPERATURE),
        top_p: None,
        max_tokens: Some(refs::blueprint_budget(images.len())),
        timeout: ROUND_TIMEOUT,
    })
    .await;
    match read {
        Ok(reply) if refs::usable(&reply.content) => {
            log(&format!(
                "{}: {} reference picture(s) read in {:.1}s",
                req.target,
                images.len(),
                started.elapsed().as_secs_f32()
            ));
            (
                RefState {
                    analysis: Some(refs::strip_fence(&reply.content)),
                    carries: false,
                },
                vec![],
            )
        }
        Ok(_) => {
            warnings.push("The reference read did not come back usable, so the pictures went straight to the prompt writer instead.".into());
            (
                RefState {
                    analysis: None,
                    carries: true,
                },
                images,
            )
        }
        // Most often a model that cannot see: the notes still travel.
        Err(e) => {
            log(&format!("{}: reference read failed: {e}", req.target));
            warnings.push(format!("The reference pictures could not be read ({e}); the prompt was written from your notes only. Pick a model that can see images to use them."));
            (RefState::default(), vec![])
        }
    }
}

/// Analyze: the model looks at one picture under its role and says what it will take.
pub async fn reference_readback(
    data: &Data,
    config: &LlmConfig,
    model: &str,
    reference: &refs::ImageRef,
) -> Result<String, String> {
    let pack = &data.pack;
    let system = pack.shared.reference_analysis.as_deref().ok_or(
        "The reading instructions are not in the Prompt Forge data, so Analyze is unavailable.",
    )?;
    let (sent, _) = refs::sendable_refs(std::slice::from_ref(reference), pack);
    let Some(r) = sent.first() else {
        return Err("That picture cannot be sent to the model (PNG, JPEG or WebP only).".into());
    };
    let messages = vec![
        json!({ "role": "system", "content": system }),
        chat::user_with_images(
            &refs::readback_question(r, pack),
            std::slice::from_ref(&r.image),
            config.provider == Provider::Ollama,
        ),
    ];
    let reply = chat::round(chat::Round {
        config,
        model,
        messages: &messages,
        tools: None,
        temperature: Some(0.2),
        top_p: None,
        max_tokens: Some(1200),
        timeout: ROUND_TIMEOUT,
    })
    .await?;
    let text = refs::strip_fence(&reply.content);
    if text.is_empty() {
        return Err("The model returned no reading.".into());
    }
    Ok(text)
}

fn validate_request<'a>(
    data: &'a Data,
    req: &WriteRequest,
) -> Result<(&'a PackModel, Option<String>), String> {
    let model = data
        .pack
        .models
        .get(&req.target)
        .ok_or_else(|| format!("Prompt Forge has no writer for \"{}\"", req.target))?;
    if req.brief.trim().is_empty() {
        return Err("Write a brief first.".into());
    }
    let mode = req
        .mode
        .clone()
        .or_else(|| model.str("default_mode").map(String::from))
        .filter(|mode| model.modes().contains(mode));
    validate_controls(model, mode.as_deref(), &req.controls)?;
    Ok((model, mode))
}

fn validate_controls(
    model: &PackModel,
    mode: Option<&str>,
    controls: &Controls,
) -> Result<(), String> {
    let invalid = message::CANVAS_ERROR;
    let aspect = controls.aspect_ratio.as_deref().unwrap_or("");
    let unset = matches!(aspect, "" | "N/A");
    let numeric = message::numeric_aspect(aspect);
    // Exact registry membership is the explicit compatibility path for old saved labels.
    let legacy = model
        .strings("aspect_ratios")
        .iter()
        .any(|value| value == aspect);
    if !unset && !matches!(aspect, "auto" | "custom") && numeric.is_none() && !legacy {
        return Err(format!("{invalid}: unknown aspect selection"));
    }
    if unset && controls.canvas.is_some() {
        return Err(format!("{invalid}: N/A cannot request a new canvas"));
    }
    let layout_mode = model
        .expected(mode)
        .iter()
        .any(|label| label == "Layout JSON");
    if let Some(canvas) = controls.canvas {
        canvas.validate(layout_mode)?;
        if let Some((width, height)) = numeric {
            // Canonical buckets and snapping can differ from mathematical proportions.
            // Reject reversed orientation, but allow a snapped square for a non-square ratio.
            if (width == height && canvas.width != canvas.height)
                || (width > height && canvas.width < canvas.height)
                || (width < height && canvas.width > canvas.height)
            {
                return Err(format!("{invalid}: aspect and canvas orientation disagree"));
            }
        }
    } else if !unset && (!legacy || matches!(aspect, "auto" | "custom")) {
        return Err(format!(
            "{invalid}: the requested aspect needs a resolved canvas"
        ));
    }
    if let Some(layout) = &controls.layout {
        message::Canvas {
            width: layout.width,
            height: layout.height,
        }
        .validate(true)?;
        if controls
            .canvas
            .is_some_and(|canvas| canvas.width != layout.width || canvas.height != layout.height)
        {
            return Err(format!("{invalid}: layout and effective canvas disagree"));
        }
    }
    if layout_mode {
        let (width, height) = layout_size(model, mode, controls)
            .ok_or_else(|| format!("{invalid}: no canvas is defined for this layout request"))?;
        message::Canvas { width, height }.validate(true)?;
    }
    Ok(())
}

/// The validated consumed canvas, otherwise the exact legacy registry/default entry.
/// Explicit invalid/new selections never fall back to the registry's N/A canvas.
fn layout_size(model: &PackModel, mode: Option<&str>, controls: &Controls) -> Option<(f64, f64)> {
    if !model.expected(mode).iter().any(|l| l == "Layout JSON") {
        return None;
    }
    if let Some(canvas) = controls.canvas {
        return Some((canvas.width, canvas.height));
    }
    if let Some(l) = &controls.layout {
        return Some((l.width, l.height));
    }
    let table = model.entry.get("layout_canvas")?;
    let aspect = controls
        .aspect_ratio
        .as_deref()
        .filter(|aspect| !aspect.is_empty())
        .unwrap_or("N/A");
    let size = table.get(aspect)?.as_array()?;
    Some((size.first()?.as_f64()?, size.get(1)?.as_f64()?))
}

fn clamp_box(b: &[f64], (w, h): (f64, f64)) -> [i64; 4] {
    let c = |v: f64, lim: f64| v.round().clamp(-lim, lim) as i64;
    let (y0, y1) = (c(b[0], h / 2.0), c(b[2], h / 2.0));
    let (x0, x1) = (c(b[1], w / 2.0), c(b[3], w / 2.0));
    [y0.min(y1), x0.min(x1), y0.max(y1), x0.max(x1)]
}

/// The person's boxes, cleaned, or none when they drew none.
fn drawn_items(size: (f64, f64), controls: &Controls) -> Vec<Value> {
    let Some(l) = &controls.layout else {
        return vec![];
    };
    let cut = |s: &str, n: usize| s.chars().take(n).collect::<String>();
    l.items
        .iter()
        .take(100)
        .filter(|i| i.bbox.len() == 4 && i.bbox.iter().all(|v| v.is_finite()))
        .map(|i| {
            let text = i.label == "text";
            let mut v = json!({
                "name": cut(&i.name, 200),
                "label": if text { "text" } else { "object" },
                "caption": cut(&i.caption, 4000),
            });
            if text {
                v["text"] = json!(cut(&i.text, 4000));
            }
            v["bbox"] = json!(clamp_box(&i.bbox, size));
            v
        })
        .collect()
}

/// The Layout Studio's Rewrite: one section at a time, the whole scene as context.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutRewrite {
    /// "scene" (the Global prompt) or "item" (the selected box's caption).
    pub task: String,
    pub width: f64,
    pub height: f64,
    /// The studio's MOSS JSON as it stands.
    pub layout: Value,
    pub description: String,
    #[serde(default)]
    pub element: Option<Value>,
}

/// PromptForge's rewriteMessages: the studio's own instruction plus which section to rewrite.
fn rewrite_messages(base: &str, r: &LayoutRewrite) -> Result<(String, String), String> {
    message::Canvas {
        width: r.width,
        height: r.height,
    }
    .validate(true)?;
    let only = match r.task.as_str() {
        "item" => {
            "Rewrite ONLY the selected element prompt; do not rewrite other elements or the scene."
        }
        "scene" => {
            "Rewrite ONLY the global scene prompt; individual element prompts are context and must not be rewritten."
        }
        _ => {
            return Err(
                "Only prompt rewriting is supported. Geometry cannot be generated or changed."
                    .into(),
            );
        }
    };
    let items = r
        .layout
        .get("items")
        .and_then(Value::as_array)
        .ok_or("Global scene context is required.")?;
    let description = r.description.trim();
    if description.is_empty() {
        return Err("Write a prompt first.".into());
    }
    let context = json!({
        "section_to_rewrite": description.chars().take(20000).collect::<String>(),
        "global_scene": r.layout.get("summary").cloned().unwrap_or(Value::Null),
        "all_elements": items.iter().take(100).collect::<Vec<_>>(),
        "selected_element": r.element.clone().unwrap_or(Value::Null),
        "canvas_size": { "width": r.width, "height": r.height },
    });
    Ok((format!("{} {only}", base.trim()), context.to_string()))
}

pub async fn layout_rewrite(
    data: &Data,
    config: &LlmConfig,
    model: &str,
    r: &LayoutRewrite,
) -> Result<String, String> {
    let base = data
        .pack
        .models
        .values()
        .find_map(|m| m.layout_rewrite.as_deref())
        .ok_or("No model in the Prompt Forge data has a Layout Studio.")?;
    let (system, user) = rewrite_messages(base, r)?;
    let messages = vec![
        json!({ "role": "system", "content": system }),
        json!({ "role": "user", "content": user }),
    ];
    let reply = chat::round(chat::Round {
        config,
        model,
        messages: &messages,
        tools: None,
        temperature: Some(0.25),
        top_p: None,
        max_tokens: Some(1200),
        timeout: ROUND_TIMEOUT,
    })
    .await?;
    let text = reply.content.trim().to_string();
    if text.is_empty() {
        return Err("The model returned an empty prompt.".into());
    }
    Ok(text)
}

fn canvas_line(model: &PackModel, mode: Option<&str>, controls: &Controls) -> Option<String> {
    if !model
        .expected(mode)
        .iter()
        .any(|label| label == "Layout JSON")
    {
        return controls.canvas.map(|canvas| {
            format!(
                "Canvas: {}×{} pixels — the resolved output canvas for this request.",
                canvas.width, canvas.height
            )
        });
    }
    let (w, h) = layout_size(model, mode, controls)?;
    let (w, h) = (w as i64, h as i64);
    let mut line = format!(
        "Canvas: {w}×{h} — measure every Layout JSON bbox on this canvas: x from -{} to {}, y from -{} to {}, origin at the centre.",
        w / 2,
        w / 2,
        h / 2,
        h / 2
    );
    let drawn = drawn_items((w as f64, h as f64), controls);
    if !drawn.is_empty() {
        line.push_str("\nThe person drew this layout in the Layout Studio. It is the composition: keep every item, in this order, with its label, bbox and text exactly as given, and add none. `name` is what they called the box. Write each caption for its box; where they wrote one, it is their direction — keep what it says and only tighten it. The prose prompt describes this same arrangement.");
        for d in &drawn {
            line.push('\n');
            line.push_str(&d.to_string());
        }
    }
    Some(line)
}

#[allow(clippy::too_many_arguments)]
fn post_process(
    data: &Data,
    model: &PackModel,
    mode: Option<&str>,
    controls: &Controls,
    brief: &str,
    segs: &mut [Segment],
    warnings: &mut Vec<String>,
    fixes: &mut Vec<String>,
) {
    let tag_style = model.tag_style();
    let store = data.tags.as_ref();
    let known_fn = |t: &str| store.is_some_and(|s| s.post_count(t) > 0);
    let known: Option<&dyn Fn(&str) -> bool> = if tag_style.is_some() && store.is_some() {
        Some(&known_fn)
    } else {
        None
    };
    let spared = spared(controls);
    let spared_refs: Vec<&str> = spared.iter().map(String::as_str).collect();
    let is_prompt = |l: &str| {
        matches!(
            l,
            "Positive prompt" | "Enhanced prompt" | "Enhanced paragraph"
        )
    };

    for s in segs.iter_mut() {
        s.body = clean::tidy_commas(&s.body);
    }
    if let Some(seg) = segs.iter_mut().find(|s| s.label == "Layout JSON") {
        if let Some(w) = tidy_layout(model, mode, controls, &mut seg.body) {
            warnings.push(w);
        }
    }
    if let Some(aspect) = controls
        .aspect_ratio
        .as_deref()
        .filter(|a| *a != "N/A" && !a.is_empty())
    {
        let mut protected = spared_refs.clone();
        protected.push(brief);
        for s in segs.iter_mut().filter(|s| is_prompt(&s.label)) {
            let (body, removed) = clean::strip_aspect(&s.body, aspect, &protected);
            if !removed.is_empty() {
                s.body = body;
                fixes.push(format!("removed leaked ratio {}", removed.join(", ")));
            }
        }
    }
    if tag_style == Some("space") {
        let protected = [
            controls.positive.as_deref().unwrap_or(""),
            controls.negative.as_deref().unwrap_or(""),
        ];
        let mut protected: Vec<&str> = protected.to_vec();
        protected.extend(controls.tags.iter().map(String::as_str));
        for s in segs
            .iter_mut()
            .filter(|s| s.label == "Positive prompt" || s.label == "Negative prompt")
        {
            let (text, changed) = clean::space_underscores(&s.body, &protected);
            if changed > 0 {
                s.body = text;
                fixes.push(format!(
                    "{changed} tag(s) respelled with spaces in {}",
                    s.label
                ));
            }
        }
    }
    if let Some(s) = segs.iter_mut().find(|s| s.label == "Positive prompt") {
        let mut protected = spared_refs.clone();
        protected.push(brief);
        let out = clean::clean_positive(&s.body, known, &protected);
        if !out.duplicates.is_empty() || !out.invented.is_empty() {
            s.body = out.text;
            if !out.duplicates.is_empty() {
                fixes.push(format!("removed {} duplicate tag(s)", out.duplicates.len()));
            }
            if !out.invented.is_empty() {
                fixes.push(format!("removed invented {}", out.invented.join(", ")));
            }
        }
    }
    let positive = segs
        .iter()
        .find(|s| s.label == "Positive prompt")
        .map(|s| s.body.clone())
        .unwrap_or_default();
    if let Some(s) = segs.iter_mut().find(|s| s.label == "Negative prompt") {
        let mut positives = vec![positive.as_str()];
        positives.extend(controls.tags.iter().map(String::as_str));
        let defaults = model.str("negative_defaults").unwrap_or("");
        let custom = controls.negative.as_deref().unwrap_or("");
        let out = clean::clean_negative(&s.body, &positives, known, &[defaults, custom]);
        if !out.duplicates.is_empty() || !out.contradictions.is_empty() {
            fixes.push(format!(
                "negative: dropped {} duplicate(s), {} contradicting the positive",
                out.duplicates.len(),
                out.contradictions.len()
            ));
        }
        let floor = [defaults, custom]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        let (text, added) = clean::ensure_defaults(&out.text, &floor, &positives);
        if !added.is_empty() {
            fixes.push(format!("negative: restored {}", added.join(", ")));
        }
        let allowance = model
            .entry
            .get("negative_extra_max")
            .and_then(Value::as_u64)
            .or_else(|| {
                data.pack
                    .config
                    .get("negative")
                    .and_then(|n| n.get("extra_max"))
                    .and_then(Value::as_u64)
            })
            .unwrap_or(0) as usize;
        let (text, dropped) = clean::cap_extras(&text, &floor, allowance);
        if !dropped.is_empty() {
            fixes.push(format!(
                "negative: dropped {} tag(s) past the allowance of {allowance}",
                dropped.len()
            ));
        }
        s.body = text;
    }
    if model
        .entry
        .get("weight_range")
        .is_some_and(|w| !w.is_null())
    {
        for s in segs
            .iter_mut()
            .filter(|s| s.label == "Positive prompt" || s.label == "Negative prompt")
        {
            let (text, escaped) = clean::escape_literal_parens(&s.body);
            if !escaped.is_empty() {
                s.body = text;
                fixes.push(format!("escaped {}", escaped.join(", ")));
            }
        }
    }
    if let Some(block) = message::pasted_style(model, controls) {
        let meta = model.strings("meta_segments");
        let label = model
            .strings("segments")
            .into_iter()
            .find(|l| !meta.contains(l));
        if let Some(seg) = segs.iter_mut().find(|s| Some(&s.label) == label.as_ref()) {
            let body = seg.body.replace(&block, "").trim().to_string();
            let stop = body.ends_with(['.', '!', '?', '"', '\'', ')', ']']);
            seg.body = if body.is_empty() {
                block
            } else {
                format!("{body}{} {block}", if stop { "" } else { "." })
            };
        }
    }
    keep_as_written(model, controls, segs, fixes);
}

/// Custom positive, negative and camera picks, as typed, whatever the model did.
fn keep_as_written(
    model: &PackModel,
    controls: &Controls,
    segs: &mut [Segment],
    fixes: &mut Vec<String>,
) {
    let norm = |t: &str| {
        t.to_lowercase()
            .replace("\\(", "(")
            .replace("\\)", ")")
            .replace('_', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    // A new line separates tags too: a model that puts a trigger word on its
    // own line has still kept it.
    let split = |s: &str| {
        s.split([',', '\n'])
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(String::from)
            .collect::<Vec<_>>()
    };
    let positive = controls
        .positive
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();
    if let Some(pos) = segs
        .iter_mut()
        .find(|s| s.label == "Positive prompt" || s.label == "Enhanced prompt")
    {
        if model.tag_style().is_some() {
            let mut want = split(&positive);
            want.extend(
                message::camera_picks(model, controls)
                    .iter()
                    .flat_map(|c| split(c))
                    .map(|t| {
                        if model.tag_style() == Some("underscore") {
                            t.replace(' ', "_")
                        } else {
                            t
                        }
                    }),
            );
            let mut have: std::collections::HashSet<String> =
                split(&pos.body).iter().map(|t| norm(t)).collect();
            let missing: Vec<String> = want.into_iter().filter(|t| have.insert(norm(t))).collect();
            if !missing.is_empty() {
                let base = pos.body.trim().trim_end_matches(',').trim().to_string();
                pos.body = std::iter::once(base)
                    .filter(|b| !b.is_empty())
                    .chain(missing.iter().cloned())
                    .collect::<Vec<_>>()
                    .join(", ");
                fixes.push(format!("put back {}", missing.join(", ")));
            }
        } else if !positive.is_empty()
            && !pos.body.to_lowercase().contains(&positive.to_lowercase())
        {
            pos.body = format!("{} {positive}", pos.body.trim()).trim().to_string();
            fixes.push(format!("put back {positive}"));
        }
    }
    // A negative switched off is not added back here: the hammer leaves
    // Generate's own negative alone, and the tab shows only what was written.
    let negative = controls
        .negative
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();
    if !negative.is_empty() {
        if let Some(neg) = segs.iter_mut().find(|s| s.label == "Negative prompt") {
            let mut have: std::collections::HashSet<String> =
                split(&neg.body).iter().map(|t| norm(t)).collect();
            let missing: Vec<String> = split(&negative)
                .into_iter()
                .filter(|t| have.insert(norm(t)))
                .collect();
            if !missing.is_empty() {
                let base = neg.body.trim().trim_end_matches(',').trim().to_string();
                neg.body = std::iter::once(base)
                    .filter(|b| !b.is_empty())
                    .chain(missing.iter().cloned())
                    .collect::<Vec<_>>()
                    .join(", ");
            }
        }
    }
}

/// MOSS Layout JSON: fixed fields, boxes rounded and inside the canvas.
fn tidy_layout(
    model: &PackModel,
    mode: Option<&str>,
    controls: &Controls,
    body: &mut String,
) -> Option<String> {
    let (w, h) = layout_size(model, mode, controls)?;
    let raw = body
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let obj: Option<Value> = match (raw.find('{'), raw.rfind('}')) {
        (Some(a), Some(b)) if b > a => serde_json::from_str(&raw[a..=b]).ok(),
        _ => None,
    };
    let Some(Value::Object(obj)) = obj else {
        return Some("The Layout JSON did not come back as valid JSON — regenerate, or use the prose prompt on its own.".into());
    };
    let clamp = |v: &Value, lim: f64| (v.as_f64().unwrap_or(0.0).round()).clamp(-lim, lim) as i64;
    let items: Vec<Value> = obj
        .get("items")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .take(100)
                .filter_map(|i| {
                    let b = i.get("bbox")?.as_array().filter(|b| b.len() == 4)?;
                    let (mut y0, mut y1) = (clamp(&b[0], h / 2.0), clamp(&b[2], h / 2.0));
                    let (mut x0, mut x1) = (clamp(&b[1], w / 2.0), clamp(&b[3], w / 2.0));
                    if y0 > y1 {
                        std::mem::swap(&mut y0, &mut y1);
                    }
                    if x0 > x1 {
                        std::mem::swap(&mut x0, &mut x1);
                    }
                    let label = if i.get("label").and_then(Value::as_str) == Some("text") { "text" } else { "object" };
                    let mut out = json!({ "label": label, "caption": i.get("caption").and_then(Value::as_str).unwrap_or("") });
                    if label == "text" {
                        out["text"] = json!(i.get("text").and_then(Value::as_str).unwrap_or(""));
                    }
                    out["bbox"] = json!([y0, x0, y1, x1]);
                    Some(out)
                })
                .collect()
        })
        .unwrap_or_default();
    // Boxes the person drew are kept exactly; the model only writes their captions.
    let drawn = drawn_items((w, h), controls);
    let items: Vec<Value> = if drawn.is_empty() {
        items
    } else {
        let written: Vec<&Value> = obj
            .get("items")
            .and_then(Value::as_array)
            .map(|a| a.iter().take(100).filter(|i| i.is_object()).collect())
            .unwrap_or_default();
        drawn
            .iter()
            .enumerate()
            .map(|(n, d)| {
                let caption = written
                    .get(n)
                    .and_then(|i| i.get("caption"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|c| !c.is_empty())
                    .map_or_else(|| d["caption"].clone(), |c| json!(c));
                let mut v = json!({ "label": d["label"], "caption": caption });
                if d["label"] == "text" {
                    v["text"] = d["text"].clone();
                }
                v["bbox"] = d["bbox"].clone();
                v
            })
            .collect()
    };
    let mut summary = obj
        .get("summary")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let meta = match summary.remove("meta") {
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| json!(v.as_str().map_or(v.to_string(), String::from)))
            .collect(),
        Some(Value::Null) | None => vec![],
        Some(v) => vec![json!(v.as_str().map_or(v.to_string(), String::from))],
    };
    let caption = summary
        .remove("caption")
        .and_then(|c| c.as_str().map(String::from))
        .unwrap_or_default();
    let mut s = serde_json::Map::new();
    s.insert("meta".into(), Value::Array(meta));
    s.extend(summary);
    s.insert("canvas".into(), json!("single"));
    s.insert("caption".into(), json!(caption));
    let out = json!({ "summary": s, "items": items, "bbox_coord_space": "abs_centralized", "bbox_format": "yxyx" });
    let pretty = serde_json::to_string_pretty(&out).unwrap_or_default();
    // Each bbox on one line, as the guide prints them.
    let mut tidy = String::new();
    let mut rest = pretty.as_str();
    while let Some(at) = rest.find("\"bbox\": [") {
        let start = at + "\"bbox\": [".len();
        tidy.push_str(&rest[..start]);
        let end = rest[start..].find(']').map_or(rest.len(), |e| start + e);
        let nums: Vec<&str> = rest[start..end].split(',').map(str::trim).collect();
        tidy.push_str(&nums.join(", "));
        rest = &rest[end..];
    }
    tidy.push_str(rest);
    *body = tidy;
    None
}

#[cfg(test)]
#[path = "aspect_tests.rs"]
mod aspect_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancel_drops_a_write_in_flight() {
        struct Dropped(#[allow(dead_code)] tokio::sync::oneshot::Sender<()>);
        let (sender, dropped) = tokio::sync::oneshot::channel::<()>();
        let held = Dropped(sender);
        let write = tokio::spawn(cancellable(async move {
            let _held = held;
            std::future::pending::<Result<(), String>>().await
        }));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        native_forge_cancel();
        assert_eq!(write.await.unwrap(), Err(CANCELLED.to_string()));
        assert!(dropped.await.is_err());
    }

    /// Write, then Second pass, then Refine, against a real server, printed in full:
    /// `DEAS_FORGE_DATA=<dir> DEAS_FORGE_LLM=<url> DEAS_FORGE_MODEL=<id> cargo test --lib -- --ignored live_revise --nocapture`
    #[tokio::test]
    #[ignore = "needs a live LLM server and the data pack"]
    async fn live_revise() {
        let data = Data::load(&PathBuf::from(std::env::var("DEAS_FORGE_DATA").unwrap())).unwrap();
        let config = LlmConfig {
            provider: Provider::Openai,
            base_url: std::env::var("DEAS_FORGE_LLM").unwrap(),
            api_key: String::new(),
        };
        let target = std::env::var("DEAS_FORGE_TARGETS").unwrap_or_else(|_| "illustrious".into());
        let write_req = WriteRequest {
            target: target.clone(),
            brief: "frieren sitting by a campfire at night in a forest, fern asleep against her shoulder".into(),
            model: std::env::var("DEAS_FORGE_MODEL").unwrap(),
            mode: None,
            segments_on: Some(vec!["Negative prompt".into()]),
            controls: Controls {
                detail: Some(5),
                creativity: Some(5),
                lighting: Some(3),
                camera_light: Some("left".into()),
                max_length: Some(message::MaxLength { unit: "words".into(), value: 60 }),
                ..Default::default()
            },
        };
        let show = |what: &str, r: &WriteResult| {
            println!(
                "=== {what} | changed {} | fixes {:?} | warnings {:?}",
                r.changed, r.fixes, r.warnings
            );
            for s in &r.segments {
                println!("--- {}\n{}", s.label, s.body);
            }
            println!();
        };
        let first = write(&data, &config, &write_req).await.unwrap();
        show("write", &first);
        let second = revise(
            &data,
            &config,
            &ReviseRequest {
                write: write_req.clone(),
                segments: first.segments.clone(),
                instruction: None,
                reference_analysis: None,
            },
        )
        .await
        .unwrap();
        show("second pass", &second);
        let refined = revise(
            &data,
            &config,
            &ReviseRequest {
                write: write_req,
                segments: second.segments.clone(),
                instruction: Some("make the campfire light colder, bluish".into()),
                reference_analysis: None,
            },
        )
        .await
        .unwrap();
        show("refine: make the campfire light colder, bluish", &refined);
    }

    /// A write with reference pictures and an Analyze read-back, printed in full:
    /// `DEAS_FORGE_DATA=<dir> DEAS_FORGE_LLM=<url> DEAS_FORGE_MODEL=<id> DEAS_FORGE_PICTURE=<png>
    ///  cargo test --lib -- --ignored live_references --nocapture`
    #[tokio::test]
    #[ignore = "needs a live vision LLM, the data pack and a picture"]
    async fn live_references() {
        use base64::Engine as _;
        let data = Data::load(&PathBuf::from(std::env::var("DEAS_FORGE_DATA").unwrap())).unwrap();
        let config = LlmConfig {
            provider: Provider::Openai,
            base_url: std::env::var("DEAS_FORGE_LLM").unwrap(),
            api_key: String::new(),
        };
        let model = std::env::var("DEAS_FORGE_MODEL").unwrap();
        let png = std::fs::read(std::env::var("DEAS_FORGE_PICTURE").unwrap()).unwrap();
        let image = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        );
        let pose = refs::ImageRef {
            role: "pose".into(),
            fidelity: "close".into(),
            image: image.clone(),
            ..Default::default()
        };
        let outfit = refs::ImageRef {
            role: "outfit".into(),
            fidelity: "close".into(),
            image,
            ..Default::default()
        };
        let read = reference_readback(&data, &config, &model, &pose)
            .await
            .unwrap();
        println!("=== Analyze (pose)\n{read}\n");
        let target = std::env::var("DEAS_FORGE_TARGETS").unwrap_or_else(|_| "illustrious".into());
        let req = WriteRequest {
            target,
            brief: "frieren standing in a meadow".into(),
            model,
            mode: None,
            segments_on: Some(vec!["Negative prompt".into()]),
            controls: Controls {
                image_references: vec![pose, outfit],
                ..Default::default()
            },
        };
        let out = write(&data, &config, &req).await.unwrap();
        println!(
            "=== reading\n{}\n",
            out.reference_analysis.as_deref().unwrap_or("(none)")
        );
        println!("=== warnings {:?} | fixes {:?}", out.warnings, out.fixes);
        for s in &out.segments {
            println!("--- {}\n{}", s.label, s.body);
        }
        assert!(out.reference_analysis.is_some());
    }

    fn ceiling_data() -> Data {
        Data {
            pack: serde_json::from_value(json!({
                "source": {}, "config": {}, "defaults": {}, "shared": {},
                "models": { "anima": {"entry": {"label": "Anima", "segments": ["Positive prompt"]}, "system": "Anima instructions"} }
            }))
            .unwrap(),
            tags: None,
            canon: None,
        }
    }

    fn ceiling_req(max_length: Option<message::MaxLength>) -> WriteRequest {
        WriteRequest {
            target: "anima".into(),
            brief: "a fox in snow".into(),
            model: "fixture".into(),
            mode: None,
            segments_on: None,
            controls: Controls {
                max_length,
                ..Default::default()
            },
        }
    }

    #[test]
    fn every_write_has_a_ceiling_and_an_asked_length_still_wins() {
        let data = ceiling_data();
        let p = prepare(&data, &ceiling_req(None), &RefState::default()).unwrap();
        assert_eq!(p.max_tokens, Some(WRITE_CEILING));
        assert!(p.ceiling_only);
        let asked = message::MaxLength {
            unit: "tokens".into(),
            value: 300,
        };
        let p = prepare(&data, &ceiling_req(Some(asked)), &RefState::default()).unwrap();
        assert_eq!(p.max_tokens, Some(300));
        assert!(!p.ceiling_only);
    }

    #[tokio::test]
    async fn a_write_that_never_stops_is_refused_but_an_asked_limit_still_cuts() {
        use chat::tests::{complete, fixture};
        let cut = || {
            complete(&json!({"choices":[{"message":{"content":"===SEGMENT: Positive prompt===\na fox, a fox, a fox"},"finish_reason":"length"}]}).to_string())
        };
        let config = |base_url| LlmConfig {
            provider: Provider::Openai,
            base_url,
            api_key: String::new(),
        };
        let data = ceiling_data();

        // No length asked: hitting the ceiling means it ran away. Nothing is kept.
        let (url, _rx, server) = fixture(vec![cut()]).await;
        let err = write(&data, &config(url), &ceiling_req(None))
            .await
            .unwrap_err();
        server.await.unwrap();
        assert!(err.starts_with("The writer did not stop"), "{err}");

        // The person asked for 300 tokens: their cut-off is honoured as before.
        let asked = message::MaxLength {
            unit: "tokens".into(),
            value: 300,
        };
        let (url, _rx, server) = fixture(vec![cut()]).await;
        let out = write(&data, &config(url), &ceiling_req(Some(asked)))
            .await
            .unwrap();
        server.await.unwrap();
        assert!(out.prompt.contains("a fox"));
    }

    #[test]
    fn layout_rewrite_names_the_one_section_and_refuses_geometry() {
        let r = LayoutRewrite {
            task: "item".into(),
            width: 1024.0,
            height: 1024.0,
            layout: json!({ "summary": { "caption": "a camp" }, "items": [{ "label": "object", "caption": "a fire", "bbox": [0, 0, 1, 1] }] }),
            description: "a fire".into(),
            element: Some(json!({ "name": "Fire" })),
        };
        let (system, user) = rewrite_messages("Base rules.", &r).unwrap();
        assert!(system.starts_with("Base rules. Rewrite ONLY the selected element prompt"));
        let u: Value = serde_json::from_str(&user).unwrap();
        assert_eq!(u["section_to_rewrite"], "a fire");
        assert_eq!(u["selected_element"]["name"], "Fire");
        assert_eq!(u["canvas_size"]["width"], 1024.0);
        assert!(rewrite_messages(
            "x",
            &LayoutRewrite {
                task: "move".into(),
                ..r
            }
        )
        .is_err());
    }

    /// Boxes drawn in the Layout Studio: the canvas and the boxes reach the model, and the
    /// tidy keeps the boxes exactly while taking the model's captions.
    /// `DEAS_FORGE_DATA=<dir> cargo test --lib -- --ignored real_layout --nocapture`
    #[test]
    #[ignore = "needs the data pack"]
    fn real_layout() {
        let data = Data::load(&data_dir(Path::new(""))).unwrap();
        let moss = &data.pack.models["moss"];
        let controls = Controls {
            layout: Some(message::StudioLayout {
                width: 1536.0,
                height: 1024.0,
                items: vec![
                    message::DrawnItem {
                        name: "Frieren".into(),
                        label: "object".into(),
                        caption: "frieren sitting".into(),
                        bbox: vec![-300.0, -600.0, 400.0, -100.0],
                        ..Default::default()
                    },
                    message::DrawnItem {
                        name: "Sign".into(),
                        label: "text".into(),
                        text: "CAMP".into(),
                        bbox: vec![-480.0, 300.0, -380.0, 700.0],
                        ..Default::default()
                    },
                ],
            }),
            ..Default::default()
        };
        let line = canvas_line(moss, Some("Layout JSON"), &controls).unwrap();
        println!("{line}");
        assert!(line.starts_with("Canvas: 1536×1024"));
        assert!(line.contains("\"name\":\"Frieren\"") && line.contains("\"text\":\"CAMP\""));
        assert!(canvas_line(moss, Some("Natural language"), &controls).is_none());
        let mut body = r#"```json
{"summary": {"meta": "anime", "caption": "a camp"}, "items": [
 {"label": "object", "caption": "Frieren sits by the fire", "bbox": [0, 0, 10, 10]},
 {"label": "text", "caption": "", "text": "WRONG", "bbox": [1, 1, 2, 2]},
 {"label": "object", "caption": "an extra the model added", "bbox": [5, 5, 9, 9]}]}
```"#
            .to_string();
        assert!(tidy_layout(moss, Some("Layout JSON"), &controls, &mut body).is_none());
        println!("{body}");
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["items"].as_array().unwrap().len(), 2);
        assert_eq!(v["items"][0]["bbox"], json!([-300, -600, 400, -100]));
        assert_eq!(v["items"][0]["caption"], "Frieren sits by the fire");
        assert_eq!(v["items"][1]["text"], "CAMP");
        assert_eq!(v["items"][1]["caption"], "");
        assert_eq!(v["summary"]["meta"], json!(["anime"]));
    }

    /// The tab's controls from the real pack: file order, ladders, the lighting line.
    /// `DEAS_FORGE_DATA=<dir> cargo test --lib -- --ignored real_targets --nocapture`
    #[tokio::test]
    #[ignore = "needs the data pack"]
    async fn real_targets() {
        let v = native_forge_targets(String::new()).await.unwrap();
        if let Some(out) = std::env::var_os("DEAS_FORGE_DUMP") {
            std::fs::write(out, v.to_string()).unwrap();
        }
        let ill = v["targets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["key"] == "illustrious")
            .unwrap();
        let keys = |list: &Value| -> Vec<String> {
            list.as_array()
                .unwrap()
                .iter()
                .map(|o| o["key"].as_str().unwrap().to_string())
                .collect()
        };
        println!("angles {:?}", keys(&ill["camera"]["angle"]));
        println!("shots {:?}", keys(&ill["camera"]["shot"]));
        println!("lights {:?}", keys(&ill["camera"]["light"]));
        println!("lighting {}", ill["lightingLevels"]);
        println!("styles {}", ill["styles"]);
        assert_eq!(keys(&ill["camera"]["light"])[0], "front");
        assert_eq!(ill["lightingLevels"][0], json!([0, "Off"]));
        let moss = v["targets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["key"] == "moss")
            .unwrap();
        assert_eq!(moss["modes"], json!(["Natural language", "Layout JSON"]));

        let data = Data::load(&data_dir(Path::new(""))).unwrap();
        let model = &data.pack.models["illustrious"];
        let at = |lighting| {
            let c = Controls {
                lighting,
                ..Default::default()
            };
            message::build(&data.pack, model, "x", &c, true, "", None)
        };
        assert!(!at(None).contains("Lighting level"));
        assert!(!at(Some(0)).contains("Lighting level"));
        let lit = at(Some(3));
        assert!(lit.contains("Lighting level 3 of 10 - Dark."), "{lit}");
        assert!(lit.contains("LIGHTING LEVEL IS AUTHORITATIVE"));
    }

    /// A real write against a real server, printed in full:
    /// `DEAS_FORGE_DATA=<dir> DEAS_FORGE_LLM=<openai base url> DEAS_FORGE_MODEL=<id>
    ///  DEAS_FORGE_TARGETS=illustrious,anima cargo test -- --ignored live_write --nocapture`
    #[tokio::test]
    #[ignore = "needs a live LLM server and the data pack"]
    async fn live_write() {
        let data = Data::load(&PathBuf::from(std::env::var("DEAS_FORGE_DATA").unwrap())).unwrap();
        let config = LlmConfig {
            provider: Provider::Openai,
            base_url: std::env::var("DEAS_FORGE_LLM").unwrap(),
            api_key: String::new(),
        };
        let model = std::env::var("DEAS_FORGE_MODEL").unwrap();
        let targets = std::env::var("DEAS_FORGE_TARGETS")
            .unwrap_or_else(|_| "illustrious,anima,krea2,moss".into());
        let briefs = [
            "frieren sitting by a campfire at night in a forest, fern asleep against her shoulder",
            "a girl in a yellow raincoat waiting at a rural bus stop in heavy rain, puddles, overcast",
            "moody cyberpunk alley, neon signs reflected in wet asphalt, a lone figure with an umbrella, cinematic",
        ];
        for target in targets.split(',') {
            for brief in briefs {
                let req = WriteRequest {
                    target: target.into(),
                    brief: brief.into(),
                    model: model.clone(),
                    mode: None,
                    segments_on: Some(vec!["Negative prompt".into()]),
                    controls: Controls {
                        detail: Some(5),
                        creativity: Some(5),
                        ..Default::default()
                    },
                };
                let started = std::time::Instant::now();
                match write(&data, &config, &req).await {
                    Ok(r) => {
                        println!(
                            "=== {target} | {}s | {} rounds, {} tool calls | {brief}",
                            started.elapsed().as_secs(),
                            r.rounds,
                            r.tools.len()
                        );
                        for t in &r.tools {
                            println!(
                                "  tool {} {}{}",
                                t.name,
                                t.queries.join(", "),
                                t.document.as_deref().unwrap_or("")
                            );
                        }
                        if !r.characters.is_empty() {
                            println!("  canon: {}", r.characters.join(", "));
                        }
                        for f in &r.fixes {
                            println!("  fixed: {f}");
                        }
                        for s in &r.segments {
                            println!("--- {}\n{}", s.label, s.body);
                        }
                        println!();
                    }
                    Err(e) => println!("=== {target} | ERROR | {brief}\n{e}\n"),
                }
            }
        }
    }
}

/// Wakes every native write in flight; dropping one closes its server request or local worker.
static CANCEL: tokio::sync::Notify = tokio::sync::Notify::const_new();
const CANCELLED: &str = "FORGE_CANCELLED";

async fn cancellable<T>(
    work: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    let stop = CANCEL.notified();
    tokio::select! {
        out = work => out,
        () = stop => Err(CANCELLED.into()),
    }
}

#[tauri::command]
pub fn native_forge_cancel() {
    CANCEL.notify_waiters();
}

#[tauri::command]
pub async fn native_forge_write(
    root: String,
    request: WriteRequest,
) -> Result<WriteResult, String> {
    let root = PathBuf::from(root);
    let config = crate::llm::read(&root)?;
    let dir = data_dir(&root);
    let data = tokio::task::spawn_blocking(move || Data::load(&dir))
        .await
        .map_err(|_| "Prompt Forge data failed to load")??;
    cancellable(async {
        chat::with_local_runner(&root, write(&data, &config, &request))
            .await
            .inspect_err(|e| {
                if e.starts_with(message::CANVAS_ERROR) {
                    message::report_canvas_failure("write");
                    return;
                }
                crate::logs::push(
                    crate::logs::Source::Deas,
                    &format!("ERROR: Prompt Forge {}: {e}", request.target),
                );
            })
    })
    .await
}

#[tauri::command]
pub async fn native_forge_reference_readback(
    root: String,
    model: String,
    reference: refs::ImageRef,
) -> Result<String, String> {
    let root = PathBuf::from(root);
    let config = crate::llm::read(&root)?;
    let dir = data_dir(&root);
    let data = tokio::task::spawn_blocking(move || Data::load(&dir))
        .await
        .map_err(|_| "Prompt Forge data failed to load")??;
    cancellable(async {
        chat::with_local_runner(
            &root,
            reference_readback(&data, &config, &model, &reference),
        )
        .await
        .inspect_err(|e| {
            crate::logs::push(
                crate::logs::Source::Deas,
                &format!("ERROR: Prompt Forge reference read-back: {e}"),
            );
        })
    })
    .await
}

#[tauri::command]
pub async fn native_forge_layout_rewrite(
    root: String,
    model: String,
    request: LayoutRewrite,
) -> Result<String, String> {
    let root = PathBuf::from(root);
    let config = crate::llm::read(&root)?;
    let dir = data_dir(&root);
    let data = tokio::task::spawn_blocking(move || Data::load(&dir))
        .await
        .map_err(|_| "Prompt Forge data failed to load")??;
    cancellable(async {
        chat::with_local_runner(&root, layout_rewrite(&data, &config, &model, &request))
            .await
            .inspect_err(|e| {
                if e.starts_with(message::CANVAS_ERROR) {
                    message::report_canvas_failure("layout");
                    return;
                }
                crate::logs::push(
                    crate::logs::Source::Deas,
                    &format!("ERROR: Prompt Forge layout rewrite: {e}"),
                );
            })
    })
    .await
}

#[tauri::command]
pub async fn native_forge_revise(
    root: String,
    request: ReviseRequest,
) -> Result<WriteResult, String> {
    let root = PathBuf::from(root);
    let config = crate::llm::read(&root)?;
    let dir = data_dir(&root);
    let data = tokio::task::spawn_blocking(move || Data::load(&dir))
        .await
        .map_err(|_| "Prompt Forge data failed to load")??;
    cancellable(async {
        chat::with_local_runner(&root, revise(&data, &config, &request))
            .await
            .inspect_err(|e| {
                if e.starts_with(message::CANVAS_ERROR) {
                    message::report_canvas_failure("revise");
                    return;
                }
                crate::logs::push(
                    crate::logs::Source::Deas,
                    &format!("ERROR: Prompt Forge revise {}: {e}", request.write.target),
                );
            })
    })
    .await
}

/// A numbered ladder's notch labels, in order: `[[level, label], ...]`.
fn ladder(table: Option<&Value>) -> Vec<(u64, String)> {
    let mut out: Vec<(u64, String)> = table
        .and_then(Value::as_object)
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| {
                    let level = k.parse().ok()?;
                    let label = v
                        .get("label")
                        .and_then(Value::as_str)
                        .map_or(k.clone(), String::from);
                    Some((level, label))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by_key(|(l, _)| *l);
    out
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct OrderEntry {
    quality_presets: Keys,
    styles: Keys,
    camera_angles: Keys,
    camera_shots: Keys,
    camera_lights: Keys,
}

#[derive(serde::Deserialize)]
struct OrderModel {
    #[serde(default)]
    entry: OrderEntry,
}

#[derive(serde::Deserialize)]
struct OrderFile {
    models: HashMap<String, OrderModel>,
}

/// `keys` sorted into file order; anything the order does not name goes last.
fn in_order<'a>(keys: impl Iterator<Item = &'a String>, order: &Keys) -> Vec<&'a String> {
    let mut out: Vec<&String> = keys.collect();
    out.sort_by_key(|k| order.0.iter().position(|o| o == *k).unwrap_or(usize::MAX));
    out
}

fn names(table: Option<&Value>, order: &Keys) -> Vec<String> {
    match table {
        Some(Value::Object(o)) => in_order(o.keys(), order).into_iter().cloned().collect(),
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => vec![],
    }
}

/// The camera picks this model's dialect has a payload for, as tiles.
fn camera_options(m: &PackModel, table: &str, order: &Keys) -> Vec<Value> {
    let dialect = if m.tag_style().is_some() {
        "tags"
    } else {
        "prose"
    };
    let Some(o) = m.entry.get(table).and_then(Value::as_object) else {
        return vec![];
    };
    in_order(o.keys(), order)
        .into_iter()
        .filter_map(|key| {
            let v = &o[key];
            let payload = v.get(dialect)?.as_str()?;
            Some(json!({ "key": key, "label": v.get("label").and_then(Value::as_str).unwrap_or(key), "icon": v.get("icon"), "payload": payload }))
        })
        .collect()
}

/// Formats the tab does not offer (owner, 23 Oct 2026: MOSS hybrid is still in testing).
const HIDDEN_MODES: &[&str] = &["Hybrid"];

/// Which image models this install can write for, from the data pack.
#[tauri::command]
pub async fn native_forge_targets(root: String) -> Result<Value, String> {
    let dir = data_dir(Path::new(&root));
    let (pack, order) = tokio::task::spawn_blocking(move || {
        let file = dir.join("forge-pack.json");
        let pack = Pack::load(&file)?;
        // Best effort: without it the lists are alphabetical, not wrong.
        let order = super::data::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice::<OrderFile>(&b).ok())
            .map(|f| f.models)
            .unwrap_or_default();
        Ok::<_, String>((pack, order))
    })
    .await
    .map_err(|_| "Prompt Forge data failed to load")??;
    let none = OrderEntry::default();
    let mut targets: Vec<Value> = pack
        .models
        .iter()
        .map(|(key, m)| {
            let order = order.get(key).map_or(&none, |o| &o.entry);
            json!({
                "key": key,
                "label": m.str("label").unwrap_or(key),
                "modes": m.modes().into_iter().filter(|x| !HIDDEN_MODES.contains(&x.as_str())).collect::<Vec<_>>(),
                "defaultMode": m.str("default_mode"),
                "optional": m.optional(),
                "qualityPresets": names(m.entry.get("quality_presets"), &order.quality_presets),
                "styles": names(m.entry.get("styles"), &order.styles),
                "aspectRatios": m.entry.get("aspect_ratios").cloned().unwrap_or(Value::Null),
                "hasNegative": m.entry.get("has_negative").and_then(Value::as_bool) != Some(false),
                "tools": m.flag("tools"),
                "guidance": m.guidance_docs.keys().collect::<Vec<_>>(),
                "secondPass": pack.shared.second_pass.is_some(),
                "tagStyle": m.tag_style(),
                "weightRange": m.entry.get("weight_range"),
                "layoutCanvas": m.entry.get("layout_canvas"),
                "layoutRewrite": m.layout_rewrite.is_some(),
                "detailLevels": ladder(m.entry.get("detail_levels")),
                "lightingLevels": ladder(m.entry.get("lighting_levels")),
                "camera": {
                    "angle": camera_options(m, "camera_angles", &order.camera_angles),
                    "shot": camera_options(m, "camera_shots", &order.camera_shots),
                    "light": camera_options(m, "camera_lights", &order.camera_lights),
                },
            })
        })
        .collect();
    targets.sort_by(|a, b| a["key"].as_str().cmp(&b["key"].as_str()));
    let roles: Vec<Value> = pack
        .role_order
        .iter()
        .filter_map(|k| {
            let d = pack.defaults["image_reference_roles"].get(k)?;
            Some(json!({ "key": k, "label": d.get("label").and_then(Value::as_str).unwrap_or(k), "rule": d.get("rule") }))
        })
        .collect();
    let fidelity: Vec<Value> = ["exact", "close", "attribute", "loose"]
        .iter()
        .filter_map(|k| {
            let d = pack.defaults["reference_fidelity"].get(*k)?;
            Some(json!({ "key": k, "label": d.get("label").and_then(Value::as_str).unwrap_or(k), "rule": d.get("rule") }))
        })
        .collect();
    Ok(json!({
        "source": pack.source,
        "creativity": pack.creativity_order,
        "targets": targets,
        "references": {
            "roles": roles,
            "fidelity": fidelity,
            "defaultRole": pack.defaults.get("default_image_reference_role"),
            "defaultFidelity": pack.defaults.get("default_reference_fidelity"),
            "max": pack.defaults.get("max_image_references").and_then(Value::as_u64).unwrap_or(6),
            "analyze": pack.shared.reference_analysis.is_some(),
        },
    }))
}
