//! Director *Create RefMod*: one isolated `DaSiWaH3RefModExport` prompt per request. It is never a
//! Workbench job, so it cannot enter generation history, the queue view or Stop all; the node itself
//! checks interruption before it writes and never overwrites an existing archive.
use super::*;

const CONCEPTS: [&str; 5] = ["generic", "identity", "style", "motion", "audio"];
const OUTPUT: &str = "refmod";

/// Empty = the node picks a unique `director-<uuid>` name. Stricter than the node's own check on purpose.
fn bundle_name(n: &str) -> bool {
    n.is_empty()
        || (n.chars().count() <= 120
            && n == n.trim()
            && !n.starts_with('.')
            && !n.ends_with('.')
            && n.chars()
                .all(|c| c.is_alphanumeric() || " -_.()".contains(c)))
}

fn text(v: Option<&Value>, max: usize) -> Result<String, String> {
    match v {
        None => Ok(String::new()),
        Some(Value::String(s)) if s.chars().count() <= max => Ok(s.clone()),
        _ => Err(format!(
            "RefMod text fields must be strings of at most {max} characters"
        )),
    }
}

fn prompt_id(id: &str) -> bool {
    id.len() == 36 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

/// `request`: `{name?, description?, conceptType?, media:[H3 media item + description?], refmods:[name], vae?, audioVae?}`.
pub(crate) fn compile(r: &Value, request_id: &str) -> Result<Value, String> {
    let o = r.as_object().ok_or("Invalid RefMod request")?;
    let known = [
        "name",
        "description",
        "conceptType",
        "media",
        "refmods",
        "vae",
        "audioVae",
    ];
    if o.keys().any(|k| !known.contains(&k.as_str())) {
        return Err("Unknown RefMod request field".into());
    }
    let name = text(o.get("name"), 120)?;
    if !bundle_name(&name) {
        return Err(
            "RefMod name: letters, digits, space, - _ . ( ) only, no leading/trailing dot or space"
                .into(),
        );
    }
    let description = text(o.get("description"), 4000)?;
    let concept = o
        .get("conceptType")
        .map_or(Some("generic"), Value::as_str)
        .unwrap_or_default();
    if !CONCEPTS.contains(&concept) {
        return Err("Invalid RefMod concept type".into());
    }
    let media = o
        .get("media")
        .and_then(Value::as_array)
        .ok_or("RefMod media must be an array")?;
    let rows = o
        .get("refmods")
        .and_then(Value::as_array)
        .ok_or("RefMod refmods must be an array")?;
    if media.is_empty() && rows.is_empty() {
        return Err("Select at least one reference or saved RefMod".into());
    }
    if rows.len() > 8 || !rows.iter().all(|n| n.as_str().is_some_and(model_name)) {
        return Err("Saved RefMods: up to 8 library names".into());
    }
    // Same item contract (deas-inputs/ files, lanes, modes, trims) as a REF2VA run; descriptions ride along.
    let mut plain = Vec::with_capacity(media.len());
    let mut descriptions = Vec::with_capacity(media.len());
    for m in media {
        let mut item = m.clone();
        let described = item
            .as_object_mut()
            .ok_or("Invalid RefMod media item")?
            .remove("description");
        descriptions.push(text(described.as_ref(), 2000)?);
        plain.push(item);
    }
    crate::jobs_video::validate_h3_media(&json!(plain), true, true)?;
    let mode = |m: &Value| m["mediaMode"].as_str().unwrap_or("video").to_owned();
    let visual = plain
        .iter()
        .any(|m| m["type"] == "image" || (m["type"] == "video" && mode(m) != "audio"));
    let audio = plain
        .iter()
        .any(|m| m["type"] == "audio" || (m["type"] == "video" && mode(m) != "video"));
    let vae = |key: &str| o.get(key).and_then(Value::as_str).filter(|v| model_name(v));
    let mut inputs = json!({"request_id":request_id,"name":name,"description":description,"concept_type":concept});
    let mut graph = json!({});
    for (needed, key, input, node) in [
        (visual, "vae", "vae", "V"),
        (audio, "audioVae", "audio_vae", "AV"),
    ] {
        if !needed {
            continue;
        }
        let file = vae(key).ok_or(if key == "vae" {
            "Video references need the MiniMax H3 video VAE"
        } else {
            "Audio references need the MiniMax H3 audio VAE"
        })?;
        graph[node] = json!({"class_type":"VAELoader","inputs":{"vae_name":file}});
        inputs[input] = json!([node, 0]);
    }
    // Director item contract, as in the REF2VA timeline: value = input-relative file, trims in seconds.
    let items: Vec<Value> = plain
        .iter()
        .zip(descriptions)
        .enumerate()
        .map(|(n, (m, description))| {
            let mut item = json!({"id":format!("m{n}"),"type":m["type"],"value":m["file"],"enabled":true,"description":description});
            for (from, to) in [("mediaMode", "media_mode"), ("trimStart", "trim_start"), ("trimEnd", "trim_end")] {
                if let Some(v) = m.get(from) {
                    item[to] = v.clone();
                }
            }
            item
        })
        .collect();
    let refmods: Vec<Value> = rows.iter().map(|n| json!({"name":n})).collect();
    inputs["timeline_data"] = json!(json!({"items":items,"refmods":refmods}).to_string());
    graph[OUTPUT] = json!({"class_type":"DaSiWaH3RefModExport","inputs":inputs});
    Ok(graph)
}

fn log(line: &str) {
    crate::logs::push(crate::logs::Source::Deas, line);
}

/// Prompts this app queued: status/cancel act only on these, with the exact graph the owned adapter verifies.
struct Submitted {
    prompt_id: String,
    base: reqwest::Url,
    client_id: String,
    graph: Value,
}
static SUBMITTED: Mutex<Vec<Submitted>> = Mutex::new(Vec::new());

fn submitted(base: &reqwest::Url, prompt_id: &str) -> Result<(String, Value), String> {
    SUBMITTED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|s| s.prompt_id == prompt_id && &s.base == base)
        .map(|s| (s.client_id.clone(), s.graph.clone()))
        .ok_or_else(|| "Unknown RefMod export".into())
}

/// Queue the export; the prompt runs in the backend's normal queue.
#[tauri::command]
pub async fn native_h3_refmod_submit(base_url: String, request: Value) -> Result<Value, String> {
    let base = crate::validate_base_url(&base_url)?;
    let graph = compile(&request, &uuid::Uuid::new_v4().to_string())?;
    let client_id = format!("deas-refmod-{}", uuid::Uuid::new_v4());
    let (status, v) = request_json(
        &base,
        "prompt",
        Some(json!({"prompt":graph,"client_id":client_id})),
    )
    .await?;
    let id = v["prompt_id"].as_str().filter(|id| prompt_id(id));
    match (status.is_success(), id) {
        (true, Some(id)) => {
            let mut list = SUBMITTED.lock().unwrap_or_else(|e| e.into_inner());
            if list.len() >= 16 {
                list.remove(0);
            }
            list.push(Submitted {
                prompt_id: id.to_owned(),
                base,
                client_id,
                graph,
            });
            log(&format!("INFO: RefMod export queued: prompt={id}"));
            Ok(json!({"promptId":id}))
        }
        _ => {
            let reason = bounded(&rejection(&v));
            log(&format!(
                "ERROR: RefMod export rejected (HTTP {status}): {reason}"
            ));
            Err(format!("RefMod export rejected: {reason}"))
        }
    }
}

/// ComfyUI names the failing node (missing DaSiWa pack, unknown VAE) in `error` + `node_errors`.
fn rejection(v: &Value) -> String {
    let Some(message) = v["error"]["message"].as_str() else {
        return v.to_string();
    };
    let nodes: Vec<&str> = v["node_errors"]
        .as_object()
        .into_iter()
        .flat_map(|n| n.values())
        .flat_map(|e| e["errors"].as_array().into_iter().flatten())
        .filter_map(|e| {
            e["details"]
                .as_str()
                .filter(|d| !d.is_empty())
                .or(e["message"].as_str())
        })
        .collect();
    if nodes.is_empty() {
        message.to_owned()
    } else {
        format!("{message}: {}", nodes.join("; "))
    }
}

async fn request_json(
    base: &reqwest::Url,
    path: &str,
    body: Option<Value>,
) -> Result<(reqwest::StatusCode, Value), String> {
    request(&client()?, base, path, body).await
}

/// Where the prompt is in the backend queue: `running`, `queued` or neither.
fn queued_state(queue: &Value, id: &str) -> Option<&'static str> {
    let has = |key: &str| {
        queue[key]
            .as_array()
            .is_some_and(|a| a.iter().any(|e| e[1] == id))
    };
    if has("queue_running") {
        Some("running")
    } else if has("queue_pending") {
        Some("queued")
    } else {
        None
    }
}

fn history_state(entry: &Value) -> Option<Value> {
    let failure = entry["status"]["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|m| {
            let kind = m[0].as_str()?;
            ["execution_error", "execution_interrupted"]
                .contains(&kind)
                .then_some((kind, &m[1]))
        });
    if let Some((kind, detail)) = failure {
        return Some(if kind == "execution_interrupted" {
            json!({"state":"cancelled"})
        } else {
            json!({"state":"failed","error":bounded(detail["exception_message"].as_str().unwrap_or("RefMod export failed"))})
        });
    }
    if let Some(r) = entry["outputs"][OUTPUT]["dasiwa_refmod_export"].get(0) {
        let name = r["name"].as_str().filter(|n| model_name(n));
        let members = r["members"].as_u64().filter(|n| (1..=256).contains(n));
        let count = |k: &str| r["counts"][k].as_u64().unwrap_or(0);
        return Some(match (name, members) {
            (Some(name), Some(members)) => {
                json!({"state":"done","result":{"name":name,"members":members,
                "counts":{"image":count("image"),"video":count("video"),"audio":count("audio")},
                "warnings":r["warnings"].as_array().into_iter().flatten().filter_map(Value::as_str).take(32).map(bounded).collect::<Vec<_>>()}})
            }
            _ => json!({"state":"failed","error":"RefMod export returned an invalid result"}),
        });
    }
    (entry["status"]["completed"] == true || entry["status"]["status_str"] == "error").then(|| {
        json!({"state":"failed","error":"RefMod export finished without a saved result; check the backend log"})
    })
}

/// `queued` | `running` | `done` (+result) | `failed` (+error) | `cancelled` | `missing` (left the queue without history).
#[tauri::command]
pub async fn native_h3_refmod_status(base_url: String, prompt_id: String) -> Result<Value, String> {
    let base = crate::validate_base_url(&base_url)?;
    submitted(&base, &prompt_id)?;
    let history = get(&client()?, &base, &format!("history/{prompt_id}")).await?;
    if let Some(state) = history.get(&prompt_id).and_then(history_state) {
        let detail = match state["state"].as_str() {
            Some("done") => format!(
                "INFO: RefMod export saved: {}.safetensors ({} members)",
                state["result"]["name"].as_str().unwrap_or_default(),
                state["result"]["members"]
            ),
            Some("failed") => format!(
                "ERROR: RefMod export failed: prompt={prompt_id}; {}",
                state["error"].as_str().unwrap_or_default()
            ),
            _ => format!("INFO: RefMod export cancelled: prompt={prompt_id}"),
        };
        log(&detail);
        return Ok(state);
    }
    let queue = get(&client()?, &base, "queue").await?;
    Ok(json!({"state":queued_state(&queue, &prompt_id).unwrap_or("missing")}))
}

/// Scoped to this app's own prompt on the owned backend: a pending one is deleted, a running one goes through
/// the authenticated DEAS interrupt adapter, which re-checks prompt, client and graph. Never core `/interrupt`.
#[tauri::command]
pub async fn native_h3_refmod_cancel(
    engine: tauri::State<'_, crate::install::Engine>,
    base_url: String,
    prompt_id: String,
) -> Result<Value, String> {
    let base = crate::validate_base_url(&base_url)?;
    let (client_id, graph) = submitted(&base, &prompt_id)?;
    let queue = get(&client()?, &base, "queue").await?;
    let Some(state) = queued_state(&queue, &prompt_id) else {
        return Ok(json!({"cancelled":"none"}));
    };
    let http = client()?;
    let request = if state == "queued" {
        engine.backend.cancellation_owner()(&base)?;
        http.post(base.join("queue").map_err(|_| "Invalid backend endpoint")?)
            .json(&json!({"delete":[prompt_id]}))
    } else {
        let (_, token) = engine.backend.interruption_owner()(&base)?;
        let mut header = reqwest::header::HeaderValue::from_str(&token)
            .map_err(|_| "Invalid native interruption authority")?;
        header.set_sensitive(true);
        http.post(
            base.join("deas/jobs/interrupt")
                .map_err(|_| "Invalid backend endpoint")?,
        )
        .header("X-DEAS-Interrupt-Token", header)
        .json(&json!({"prompt_id":prompt_id,"client_id":client_id,"prompt":graph}))
    };
    let status = request
        .send()
        .await
        .map_err(|_| "RefMod cancel reply lost; check the queue")?
        .status();
    if !status.is_success() {
        return Err(format!("RefMod cancel rejected (HTTP {})", status.as_u16()));
    }
    log(&format!(
        "INFO: RefMod export cancel requested: prompt={prompt_id}; {state}"
    ));
    Ok(json!({"cancelled":state}))
}

#[cfg(test)]
#[path = "jobs_refmod_tests.rs"]
mod tests;
