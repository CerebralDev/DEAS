//! YuE2 music recipes (`recipeId: "music-generation"`): text-to-music with ABC planning
//! Off/Full/Melody, and melody covers transcribed from an uploaded audio/video source by
//! SheetSage2. Graphs mirror the core `audio_yue2_*` templates; saver keeps ID `7`.
use crate::jobs::{comfy_model_name, model_name, safe_choice, safe_output_prefix};
use serde::Deserialize;
use serde_json::{json, Value};

/// Every class the graphs can emit; prepare() checks the compiled graph's nodes against object_info.
/// (`CheckpointLoaderSimple`/`KSampler` are already core-required.)
pub(crate) const MUSIC_NODES: [&str; 9] = [
    "YuE2GenerateABC",
    "YuE2GenerateMusic",
    "ConditioningZeroOut",
    "EmptyYuE2LatentAudio",
    "VAEDecodeAudio",
    "SaveAudioAdvanced",
    "LoadAudio",
    "AudioEncoderLoader",
    "SheetSage2AudioToABC",
];

pub(crate) fn is_music(i: &Value) -> bool {
    i["recipeId"] == "music-generation"
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Abc {
    max_tokens: u64,
    temperature: f64,
    top_p: f64,
    top_k: u64,
    repetition_penalty: f64,
    penalty_window: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Generation {
    temperature: f64,
    top_p: f64,
    top_k: u64,
    repetition_penalty: f64,
    cfg_scale: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Intent {
    recipe_id: String,
    version: u64,
    family: String,
    mode: String,
    planning: String,
    checkpoint: String,
    style: String,
    lyrics: String,
    seed: u64,
    max_duration: f64,
    steps: u64,
    cfg: f64,
    sampler: String,
    scheduler: String,
    generation: Generation,
    output_prefix: String,
    abc: Option<Abc>,
    source_audio: Option<String>,
    audio_encoder: Option<String>,
}

fn range(name: &str, v: f64, min: f64, max: f64) -> Result<(), String> {
    if v.is_finite() && (min..=max).contains(&v) {
        Ok(())
    } else {
        Err(format!("Invalid {name}: {min}..{max}"))
    }
}

fn text(name: &str, s: &str) -> Result<(), String> {
    if s.chars().count() <= 16384 && s.len() <= 65536 && !s.contains('\0') {
        Ok(())
    } else {
        Err(format!("Invalid {name}: at most 16384 characters, no NUL"))
    }
}

fn abc_settings(a: &Abc) -> Result<(), String> {
    range("abc.maxTokens", a.max_tokens as f64, 1.0, 20000.0)?;
    range("abc.temperature", a.temperature, 0.0, 5.0)?;
    range("abc.topP", a.top_p, 0.01, 1.0)?;
    range("abc.topK", a.top_k as f64, 1.0, 32768.0)?;
    range("abc.repetitionPenalty", a.repetition_penalty, 0.01, 10.0)?;
    range("abc.penaltyWindow", a.penalty_window as f64, 1.0, 20000.0)
}

fn validate(i: &Value) -> Result<Intent, String> {
    let m: Intent =
        serde_json::from_value(i.clone()).map_err(|e| format!("Invalid music intent: {e}"))?;
    if m.recipe_id != "music-generation" || m.version != 1 || m.family != "yue2" {
        return Err("Unsupported music recipe".into());
    }
    match (
        m.mode.as_str(),
        m.planning.as_str(),
        m.abc.is_some(),
        m.source_audio.is_some(),
        m.audio_encoder.is_some(),
    ) {
        ("t2m", "off", false, false, false)
        | ("t2m", "full" | "melody", true, false, false)
        | ("cover", "melody", false, true, true) => {}
        _ => return Err("Invalid mode/planning/abc/source combination".into()),
    }
    text("style", &m.style)?;
    if m.style.trim().is_empty() {
        return Err("Style must not be blank".into());
    }
    text("lyrics", &m.lyrics)?;
    if !comfy_model_name(&m.checkpoint) {
        return Err("Unsafe checkpoint".into());
    }
    if m.audio_encoder
        .as_deref()
        .is_some_and(|e| !comfy_model_name(e))
    {
        return Err("Unsafe audioEncoder".into());
    }
    if m.source_audio
        .as_deref()
        .is_some_and(|s| !model_name(s) || s.ends_with(" [output]"))
    {
        return Err("Unsafe sourceAudio: an uploaded input file".into());
    }
    if !safe_output_prefix(&m.output_prefix) {
        return Err("Unsafe outputPrefix".into());
    }
    if !safe_choice(&m.sampler) || !safe_choice(&m.scheduler) {
        return Err("Unsafe sampler/scheduler".into());
    }
    range("seed", m.seed as f64, 0.0, 9007199254740991.0)?;
    range("maxDuration", m.max_duration, 0.04, 900.0)?;
    // The node's 0.04 s step: ComfyUI validation rounds silently, DEAS does not.
    let scaled = m.max_duration / 0.04;
    if (scaled - scaled.round()).abs() > 1e-7 {
        return Err("Invalid maxDuration: multiple of 0.04 seconds".into());
    }
    range("steps", m.steps as f64, 1.0, 10000.0)?;
    range("cfg", m.cfg, 0.0, 100.0)?;
    let g = &m.generation;
    range("generation.temperature", g.temperature, 0.0, 5.0)?;
    range("generation.topP", g.top_p, 0.01, 1.0)?;
    range("generation.topK", g.top_k as f64, 1.0, 32768.0)?;
    range(
        "generation.repetitionPenalty",
        g.repetition_penalty,
        0.01,
        10.0,
    )?;
    range("generation.cfgScale", g.cfg_scale, 0.0, 100.0)?;
    if let Some(a) = &m.abc {
        abc_settings(a)?;
    }
    Ok(m)
}

pub(crate) fn compile(i: &Value) -> Result<Value, String> {
    let m = validate(i)?;
    let g = &m.generation;
    let (abc, mode) = match (m.mode.as_str(), m.planning.as_str()) {
        ("t2m", "off") => (json!(""), "full"),
        ("cover", _) => (json!(["C", 0]), "melody"),
        (_, planning) => (json!(["A", 0]), planning),
    };
    let mut graph = json!({
        "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":m.checkpoint}},
        "M":{"class_type":"YuE2GenerateMusic","inputs":{"clip":["1",1],"style":m.style,"lyrics":m.lyrics,"abc":abc,"seed":m.seed,"mode":mode,"max_duration":m.max_duration,"temperature":g.temperature,"top_p":g.top_p,"top_k":g.top_k,"repetition_penalty":g.repetition_penalty,"cfg_scale":g.cfg_scale}},
        "Z":{"class_type":"ConditioningZeroOut","inputs":{"conditioning":["M",0]}},
        "L":{"class_type":"EmptyYuE2LatentAudio","inputs":{"seconds":["M",1],"batch_size":1}},
        "S":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["M",0],"negative":["Z",0],"latent_image":["L",0],"seed":m.seed,"steps":m.steps,"cfg":m.cfg,"sampler_name":m.sampler,"scheduler":m.scheduler,"denoise":1.0}},
        "D":{"class_type":"VAEDecodeAudio","inputs":{"samples":["S",0],"vae":["1",2]}},
        "7":{"class_type":"SaveAudioAdvanced","inputs":{"audio":["D",0],"filename_prefix":m.output_prefix,"format":"flac"}}
    });
    if let Some(a) = &m.abc {
        graph["A"] = json!({"class_type":"YuE2GenerateABC","inputs":{"clip":["1",1],"style":m.style,"lyrics":m.lyrics,"seed":m.seed,"mode":mode,"max_abc_tokens":a.max_tokens,"temperature":a.temperature,"top_p":a.top_p,"top_k":a.top_k,"repetition_penalty":a.repetition_penalty,"penalty_window":a.penalty_window}});
    }
    if let (Some(source), Some(encoder)) = (&m.source_audio, &m.audio_encoder) {
        graph["I"] = json!({"class_type":"LoadAudio","inputs":{"audio":source}});
        graph["E"] =
            json!({"class_type":"AudioEncoderLoader","inputs":{"audio_encoder_name":encoder}});
        graph["C"] = json!({"class_type":"SheetSage2AudioToABC","inputs":{"audio_encoder":["E",0],"audio":["I",0],"mode":"melody"}});
    }
    Ok(graph)
}

/// Model prerequisites by object_info field; called only on a validated intent.
pub(crate) fn requirements(i: &Value) -> Vec<(&'static str, Vec<&str>)> {
    let mut r = vec![(
        "checkpoints",
        vec![i["checkpoint"].as_str().unwrap_or_default()],
    )];
    if let Some(encoder) = i["audioEncoder"].as_str() {
        r.push(("audioEncoders", vec![encoder]));
    }
    r
}

#[cfg(test)]
#[path = "jobs_music_tests.rs"]
pub(crate) mod tests;
