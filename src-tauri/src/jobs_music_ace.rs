//! ACE-Step text-to-music. Recipe/defaults and latent clocks come from the shared family registry.
use super::{range, text};
use crate::jobs::{comfy_model_name, safe_choice, safe_output_prefix};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::LazyLock;

pub(crate) const NODES: [&str; 8] = [
    "TextEncodeAceStepAudio",
    "TextEncodeAceStepAudio1.5",
    "EmptyAceStepLatentAudio",
    "EmptyAceStep1.5LatentAudio",
    "ModelSamplingSD3",
    "LatentOperationTonemapReinhard",
    "LatentApplyOperationCFG",
    "DualCLIPLoader",
];
static FAMILIES: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../src/families/builtin.json")).unwrap()
});
static DATA: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(include_str!("../../src/lib/aceStepData.json")).unwrap());
fn profile(family: &str) -> Result<&'static Value, String> {
    FAMILIES
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == family)
        .and_then(|f| f.get("music"))
        .ok_or_else(|| "Unresolved or unsupported ACE-Step family".into())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Diffusion {
    text_encoder: String,
    text_encoder2: String,
    vae: String,
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
    output_prefix: String,
    ace: Value,
    diffusion: Option<Diffusion>,
}
fn settings(a: &Value) -> Result<(), String> {
    let o = a.as_object().ok_or("Invalid ace settings")?;
    let bounds = DATA["bounds"].as_object().unwrap();
    if o.len() != bounds.len() + 4
        || o.keys().any(|k| {
            !bounds.contains_key(k)
                && ![
                    "timeSignature",
                    "language",
                    "keyScale",
                    "generateAudioCodes",
                ]
                .contains(&k.as_str())
        })
    {
        return Err("Invalid ace settings fields".into());
    }
    for (key, b) in bounds {
        let v = a[key]
            .as_f64()
            .ok_or_else(|| format!("Invalid ace.{key}"))?;
        range(
            &format!("ace.{key}"),
            v,
            b[0].as_f64().unwrap(),
            b[1].as_f64().unwrap(),
        )?;
        if b[2] == true && v.fract() != 0.0 {
            return Err(format!("Invalid ace.{key}: integer required"));
        }
    }
    for (key, choices) in [
        ("language", "languages"),
        ("keyScale", "keyScales"),
        ("timeSignature", "timeSignatures"),
    ] {
        if !DATA[choices].as_array().unwrap().contains(&a[key]) {
            return Err(format!("Invalid ace.{key}"));
        }
    }
    if !a["generateAudioCodes"].is_boolean() {
        return Err("Invalid ace.generateAudioCodes".into());
    }
    Ok(())
}
fn validate(i: &Value) -> Result<(Intent, &'static Value), String> {
    let m: Intent =
        serde_json::from_value(i.clone()).map_err(|e| format!("Invalid ACE music intent: {e}"))?;
    let p = profile(&m.family)?;
    if m.recipe_id != "music-generation" || m.version != 1 || m.mode != "t2m" || m.planning != "off"
    {
        return Err("ACE-Step supports text-to-music with planning off only".into());
    }
    if i.get("diffusion").is_some_and(Value::is_null) {
        return Err("Invalid diffusion".into());
    }
    text("style", &m.style)?;
    text("lyrics", &m.lyrics)?;
    if m.style.trim().is_empty() {
        return Err("Style must not be blank".into());
    }
    if !comfy_model_name(&m.checkpoint) || m.checkpoint.to_ascii_lowercase().ends_with(".gguf") {
        return Err("Unsafe or unsupported ACE model".into());
    }
    if !safe_output_prefix(&m.output_prefix)
        || !safe_choice(&m.sampler)
        || !safe_choice(&m.scheduler)
    {
        return Err("Unsafe ACE output/sampling choice".into());
    }
    range("seed", m.seed as f64, 0.0, 9007199254740991.0)?;
    range("steps", m.steps as f64, 1.0, 10000.0)?;
    range("cfg", m.cfg, 0.0, 100.0)?;
    range("maxDuration", m.max_duration, 1.0, 1000.0)?;
    let rate = p["sampleRate"].as_f64().unwrap();
    let samples = p["samplesPerLatent"].as_f64().unwrap();
    let frames = m.max_duration * rate / samples;
    if (frames - frames.round()).abs() > 1e-7 {
        return Err("Invalid maxDuration: use the model's native latent grid".into());
    }
    // The original core truncates rather than rounds: refuse floats that lose a latent frame.
    if p["version"] == "1.0" && (m.max_duration * rate / 512.0 / 8.0).floor() != frames.round() {
        return Err("Invalid maxDuration: floating-point value truncates a latent frame".into());
    }
    settings(&m.ace)?;
    if let Some(d) = &m.diffusion {
        if p["version"] != "1.5" {
            return Err("ACE-Step 1.0 requires an all-in-one checkpoint".into());
        }
        if [&d.text_encoder, &d.text_encoder2, &d.vae]
            .iter()
            .any(|s| !comfy_model_name(s) || s.to_ascii_lowercase().ends_with(".gguf"))
            || d.text_encoder == d.text_encoder2
        {
            return Err("Invalid ACE split components".into());
        }
    }
    Ok((m, p))
}
pub(super) fn compile(i: &Value) -> Result<Value, String> {
    let (m, p) = validate(i)?;
    let a = &m.ace;
    let mut g = json!({
        "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":m.checkpoint}},
        "P":{"class_type":p["patch"],"inputs":{"model":["1",0],"shift":a["shift"]}},
        "M":{"class_type":p["encode"],"inputs":{"clip":["1",1],"tags":m.style,"lyrics":m.lyrics}},
        "Z":{"class_type":"ConditioningZeroOut","inputs":{"conditioning":["M",0]}},
        "L":{"class_type":p["latent"],"inputs":{"seconds":m.max_duration,"batch_size":1}},
        "S":{"class_type":"KSampler","inputs":{"model":["P",0],"positive":["M",0],"negative":["Z",0],"latent_image":["L",0],"seed":m.seed,"steps":m.steps,"cfg":m.cfg,"sampler_name":m.sampler,"scheduler":m.scheduler,"denoise":1.0}},
        "D":{"class_type":"VAEDecodeAudio","inputs":{"samples":["S",0],"vae":["1",2]}},
        "7":{"class_type":"SaveAudioAdvanced","inputs":{"audio":["D",0],"filename_prefix":m.output_prefix,"format":"flac"}}
    });
    if p["version"] == "1.0" {
        g["M"]["inputs"]["lyrics_strength"] = a["lyricsStrength"].clone();
        g["T"] = json!({"class_type":"LatentOperationTonemapReinhard","inputs":{"multiplier":1.0}});
        g["C"] = json!({"class_type":"LatentApplyOperationCFG","inputs":{"model":["P",0],"operation":["T",0]}});
        g["S"]["inputs"]["model"] = json!(["C", 0]);
    } else {
        for (key, value) in [
            ("seed", json!(m.seed)),
            ("duration", json!(m.max_duration)),
            ("bpm", a["bpm"].clone()),
            ("timesignature", a["timeSignature"].clone()),
            ("language", a["language"].clone()),
            ("keyscale", a["keyScale"].clone()),
            ("generate_audio_codes", a["generateAudioCodes"].clone()),
            ("cfg_scale", a["lmCfg"].clone()),
            ("temperature", a["temperature"].clone()),
            ("top_p", a["topP"].clone()),
            ("top_k", a["topK"].clone()),
            ("min_p", a["minP"].clone()),
        ] {
            g["M"]["inputs"][key] = value;
        }
    }
    if let Some(d) = &m.diffusion {
        g["1"] = json!({"class_type":"UNETLoader","inputs":{"unet_name":m.checkpoint,"weight_dtype":"default"}});
        g["E"] = json!({"class_type":"DualCLIPLoader","inputs":{"clip_name1":d.text_encoder,"clip_name2":d.text_encoder2,"type":"ace","device":"default"}});
        g["V"] = json!({"class_type":"VAELoader","inputs":{"vae_name":d.vae}});
        g["M"]["inputs"]["clip"] = json!(["E", 0]);
        g["D"]["inputs"]["vae"] = json!(["V", 0]);
    }
    Ok(g)
}

#[cfg(test)]
#[path = "jobs_music_ace_tests.rs"]
mod tests;
