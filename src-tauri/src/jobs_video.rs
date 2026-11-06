//! Video recipes wired the way the owner's DaSiWa workflows do it (LTX OmniForge, Wan 2.2
//! FastFidelity, MiniMax H3 MythicAlchemy), not the ComfyUI templates. One intent,
//! `recipeId: "video-generation"`, discriminated by `family`.
use crate::jobs::{comfy_model_name, model_name, safe_choice, safe_output_prefix, valid_options};
use serde_json::{json, Value};

pub(crate) const FAMILIES: [&str; 3] = ["ltx-2", "wan", "minimax-h3"];
const FIELDS: [&str; 17] = [
    "recipeId",
    "version",
    "family",
    "positivePrompt",
    "negativePrompt",
    "model",
    "textEncoder",
    "vae",
    "seed",
    "width",
    "height",
    "frames",
    "frameRate",
    "steps",
    "cfg",
    "sampler",
    "scheduler",
];
const OPTIONAL: [&str; 10] = [
    "latentUpscale",
    "videoPost",
    "outputPrefix",
    "sourceVideo",
    "output",
    "loras",
    "options",
    "firstFrame",
    "lastFrame",
    "chunks",
];
/// DaSiWa Enhanced Video Combine enums.
pub(crate) const CODECS: [&str; 5] = ["Auto", "AV1", "VP9", "H.265 (HEVC)", "H.264"];
pub(crate) const CONTAINERS: [&str; 6] = [
    "Auto",
    "WebM",
    "MKV",
    "MP4",
    "Animated WebP",
    "Animated AVIF",
];
pub(crate) const BIT_DEPTHS: [&str; 3] = ["Auto", "8-bit", "10-bit"];
pub(crate) const AUDIO_CODECS: [&str; 4] = ["Auto", "AAC", "Opus", "MP3"];
pub(crate) const AUDIO_BITRATES: [&str; 7] = ["64k", "96k", "128k", "160k", "192k", "256k", "320k"];
const H3_MODES: [&str; 5] = ["T2VA", "I2VA", "L2VA", "FL2VA", "REF2VA"];
/// DaSiWa Director REF2VA lanes (slots per lane) and media modes of a video item.
const H3_LANES: [(&str, u64); 3] = [("image", 9), ("video", 3), ("audio", 3)];
const H3_MEDIA_MODES: [&str; 3] = ["video", "audio", "video_audio"];
/// Director `timeline_data.resolution.input_scaling` values (`helper_minimax_h3_director.py::INPUT_SCALING_MODES`).
const H3_INPUT_SCALING: [&str; 7] = [
    "Off",
    "Auto",
    "Target",
    "Fit",
    "Fill and crop",
    "Fit and pad",
    "Long side with divisible crop",
];
/// Every class the three graphs can emit; prepare() checks each against object_info.
pub(crate) const VIDEO_NODES: [&str; 36] = [
    "DaSiWaH3TiledUpscale",
    "DualCLIPLoader",
    "LoraLoaderModelOnly",
    "RandomNoise",
    "KSamplerSelect",
    "BasicScheduler",
    "CFGGuider",
    "BasicGuider",
    "SamplerCustomAdvanced",
    "KSamplerAdvanced",
    "ModelSamplingSD3",
    "ImageScale",
    "LoadImage",
    "LoadVideo",
    "GetVideoComponents",
    "ImageFromBatch",
    "LTXVConditioning",
    "EmptyLTXVLatentVideo",
    "LTXVEmptyLatentAudio",
    "LTXVPreprocess",
    "LTXVAddGuide",
    "LTXVCropGuides",
    "LTXVConcatAVLatent",
    "LTXVSeparateAVLatent",
    "LTXVAudioVAEDecode",
    "LatentUpscaleModelLoader",
    "LTXVLatentUpsampler",
    "LTXVChunkFeedForward",
    "WanImageToVideo",
    "WanFirstLastFrameToVideo",
    "WanChunkFeedForward",
    "MiniMaxH3Director",
    "MiniMaxH3DirectorGuide",
    "MiniMaxH3SigmaShift",
    "MiniMaxChunkFeedForward",
    "DaSiWa_EnhancedVideoCombine",
];
pub(crate) const H3_CONTINUITY_NODES: [&str; 3] = [
    "DaSiWaH3ContinuitySegmentPrepare",
    "DaSiWaH3ContinuitySegmentDecode",
    "DaSiWaH3ContinuityPublish",
];

pub(crate) fn is_video(intent: &Value) -> bool {
    intent["recipeId"] == "video-generation" || crate::jobs_video_post::is_enhance(intent)
}

/// Canvas multiple, frame step and frame offset: EmptyLTXVLatentVideo /32 & 8k+1, Wan /16 & 4k+1, H3 /32 & 17k+5.
fn grid(family: &str) -> (u64, u64, u64) {
    match family {
        "ltx-2" => (32, 8, 1),
        "wan" => (16, 4, 1),
        _ => (32, 17, 5),
    }
}

fn pick(o: &Value, key: &str, list: &[&str]) -> bool {
    o[key].as_str().is_some_and(|v| list.contains(&v))
}

pub(crate) fn continuity_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn keys_in(v: &Value, allowed: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.keys().all(|k| allowed.contains(&k.as_str())))
}

pub(crate) fn valid_output(o: &Value) -> bool {
    keys_in(
        o,
        &[
            "codec",
            "container",
            "bitDepth",
            "quality",
            "audioCodec",
            "audioBitrate",
            "saveFirstFrame",
            "saveLastFrame",
            "imageFormat",
            "imageCompression",
        ],
    ) && pick(o, "codec", &CODECS)
        && pick(o, "container", &CONTAINERS)
        && pick(o, "bitDepth", &BIT_DEPTHS)
        && o["quality"].as_u64().is_some_and(|q| q <= 51)
        && pick(o, "audioCodec", &AUDIO_CODECS)
        && pick(o, "audioBitrate", &AUDIO_BITRATES)
        && o["saveFirstFrame"].is_boolean()
        && o["saveLastFrame"].is_boolean()
        && o.get("imageFormat")
            .is_none_or(|f| f.as_str().is_some_and(|f| ["png", "webp"].contains(&f)))
        && o.get("imageCompression")
            .is_none_or(|c| c.as_u64().is_some_and(|c| c <= 100))
}

fn finite(v: &Value, min: f64, max: f64) -> bool {
    v.as_f64()
        .is_some_and(|n| n.is_finite() && (min..=max).contains(&n))
}

pub(crate) fn learned_latent_model(name: &str) -> bool {
    comfy_model_name(name) && (name.ends_with(".safetensors") || name.ends_with(".sft"))
}

/// Post-upscale pixel canvas; H3 uses Python round's ties-to-even 32-pixel grid.
pub(crate) fn latent_upscale_size(i: &Value) -> (u64, u64) {
    let dimension = |key: &str| {
        let n = i[key].as_u64().unwrap_or(0);
        let Some(u) = i.get("latentUpscale") else {
            return n;
        };
        if i["family"] == "minimax-h3" {
            ((n as f64 * u["scale"].as_f64().unwrap_or(1.0) / 32.0).round_ties_even() as u64) * 32
        } else {
            n * 2
        }
    };
    (dimension("width"), dimension("height"))
}

pub(crate) fn h3_latent_model(name: &str) -> bool {
    let file = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    learned_latent_model(name)
        && file.starts_with("minimax_h3_latent_upscal")
        && !file.contains("ltx")
}

fn ltx_latent_model(name: &str) -> bool {
    let file = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    learned_latent_model(name)
        && file.starts_with("ltx")
        && file.contains("spatial")
        && file.contains("upscal")
}

/// Optional `DaSiWaH3TiledUpscale` inputs (intent key, node input); omitted keys keep the node defaults.
const H3_LATENT_ADVANCED_INPUTS: [(&str, &str); 14] = [
    ("cfg", "cfg"),
    ("scheduler", "scheduler"),
    ("precision", "upscale_precision"),
    ("memoryBudgetMb", "memory_budget_mb"),
    ("spatialTiling", "spatial_tiling"),
    ("temporalChunking", "temporal_chunking"),
    ("spikeGuard", "spike_guard"),
    ("guardSensitivity", "guard_sensitivity"),
    ("guardStrength", "guard_strength"),
    ("guardMode", "guard_mode"),
    ("memoryPlanning", "enforce_memory_budget"),
    ("memoryReservePct", "memory_reserve_pct"),
    ("continuitySoftRefine", "continuity_soft_refine"),
    ("continuityMaskStrength", "continuity_mask_strength"),
];

fn valid_h3_latent_advanced(a: &Value) -> bool {
    let Some(o) = a.as_object() else {
        return false;
    };
    o.iter().all(|(k, v)| match k.as_str() {
        "cfg" => finite(v, 0.0, 20.0),
        // Exact backend scheduler names are lowercase identifiers; the backend rejects unknown ones.
        "scheduler" => v.as_str().is_some_and(|s| {
            !s.is_empty()
                && s.len() <= 64
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        }),
        "precision" => matches!(v.as_str(), Some("auto" | "bf16" | "fp16" | "fp32")),
        "memoryBudgetMb" => v.as_u64().is_some_and(|n| n <= 131_072),
        "spatialTiling"
        | "temporalChunking"
        | "spikeGuard"
        | "memoryPlanning"
        | "continuitySoftRefine" => v.is_boolean(),
        "guardSensitivity" => finite(v, 1.5, 12.0),
        "guardStrength" | "continuityMaskStrength" => finite(v, 0.0, 1.0),
        "guardMode" => matches!(v.as_str(), Some("burn_direction" | "both")),
        "memoryReservePct" => finite(v, 0.0, 100.0),
        _ => false,
    })
}

fn validate_latent_upscale(i: &Value, family: &str) -> Result<(), String> {
    let Some(u) = i.get("latentUpscale") else {
        return Ok(());
    };
    let settings = match family {
        "ltx-2" => keys_in(u, &["model", "scale"]) && u["scale"].as_f64() == Some(2.0),
        "minimax-h3" => {
            keys_in(u, &["model", "scale", "steps", "denoise", "advanced"])
                && finite(&u["scale"], 1.0, 8.0)
                && u["steps"].as_u64().is_some_and(|n| (1..=100).contains(&n))
                && finite(&u["denoise"], 0.0, 1.0)
                && u.get("advanced").is_none_or(valid_h3_latent_advanced)
        }
        _ => false,
    };
    if !settings
        || !u["model"].as_str().is_some_and(if family == "minimax-h3" {
            h3_latent_model
        } else {
            ltx_latent_model
        })
    {
        return Err("Invalid latentUpscale: learned .safetensors/.sft model and family-specific settings required".into());
    }
    let (w, h) = latent_upscale_size(i);
    if w > 8192 || h > 8192 || w * h > 16_777_216 {
        return Err("latentUpscale canvas exceeds dimension or pixel limits".into());
    }
    let prefix = format!("{}_original", effective_output_prefix(i).as_str().unwrap());
    if !safe_output_prefix(&prefix) {
        return Err("latentUpscale original output prefix exceeds limits".into());
    }
    if family == "minimax-h3"
        && i["h3"]["continuity"]["capture"] == true
        && i["h3"]["continuity"]["outputMode"] == "segment"
        && segment_crop_plan(i).is_none()
    {
        return Err("latentUpscale with captured segment output needs a pinned checkpoint clip (imported-video sources are unsupported) with its sourceFrames from the session list".into());
    }
    Ok(())
}

/// Compile-time copy of the node pack's segment_policy.render_layout crop fields: the core decode of
/// the prepared render window starts at the global frame offset, so the new segment is its final
/// extension frames. shortcut: mirrors the pack's layout constants; on pack drift the pack's own
/// render validation fails closed at runtime.
fn segment_crop_plan(i: &Value) -> Option<(u64, u64)> {
    let c = &i["h3"]["continuity"];
    if c["capture"] != true || c["outputMode"] != "segment" {
        return None;
    }
    // The node pack renders a segment crop only over a source window: a first take has none, and
    // an imported-video source builds its window latent inside SegmentPrepare (not reproducible
    // from compile-time fields), so only checkpoint pins are supported here.
    if c["sourceKind"] == "video" {
        return None;
    }
    let source = c["sourceFrames"].as_u64()?;
    // Continuation timing: newly visible seconds snapped to 17-frame steps. Without authored
    // duration the Director receives frames/24 seconds, so the snap runs on frames/17 directly.
    let extension = match c.get("durationSeconds").and_then(Value::as_f64) {
        Some(s) => ((s * 24.0 / 17.0 + 0.5).floor().max(1.0) as u64) * 17,
        None => ((i["frames"].as_f64()? / 17.0 + 0.5).floor().max(1.0) as u64) * 17,
    };
    Some((source - source.saturating_sub(102) / 51 * 51, extension))
}

/// prepare() must look up the pinned clip's frame count before compiling this intent.
pub(crate) fn needs_source_frames(i: &Value) -> bool {
    is_video(i)
        && i["family"] == "minimax-h3"
        && i.get("latentUpscale").is_some()
        && i["h3"]["continuity"]["capture"] == true
        && i["h3"]["continuity"]["outputMode"] == "segment"
        && i["h3"]["continuity"]["sourceKind"] != "video"
        && i["h3"]["continuity"]["sourceId"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
}

/// The pinned source's frame count from the read-only session clip list: the staged import for video
/// sources (matching canvas and mode family), otherwise the pinned checkpoint clip.
pub(crate) fn source_frames(list: &Value, i: &Value) -> Option<u64> {
    let c = &i["h3"]["continuity"];
    let id = c["sourceId"].as_str().filter(|s| !s.is_empty())?;
    let clip = list["clips"].as_array()?.iter().find(|cl| {
        if c["sourceKind"] == "video" {
            let p = &cl["importedVideo"];
            p["sourceId"] == id
                && p["width"] == i["width"]
                && p["height"] == i["height"]
                && p["modeFamily"]
                    == (if i["h3"]["mode"] == "REF2VA" {
                        "ref2va"
                    } else {
                        "fl2va"
                    })
        } else {
            cl["clipId"] == id
        }
    })?;
    clip["frames"].as_u64()
}

fn validate_ltx(l: &Value, frames: u64, source: bool) -> Result<(), String> {
    let passes = l["passes"].as_u64().unwrap_or(0);
    // Keyframes (multi-reference): an image at a pixel frame (-1 = last) with its guide strength.
    let keyframes_ok = l.get("keyframes").is_none_or(|k| {
        k.as_array().is_some_and(|a| {
            !a.is_empty()
                && a.len() <= 8
                && a.iter().all(|x| {
                    keys_in(x, &["image", "frame", "strength"])
                        && x["image"].as_str().is_some_and(model_name)
                        && x["frame"]
                            .as_i64()
                            .is_some_and(|f| f == -1 || (0..frames as i64).contains(&f))
                        && finite(&x["strength"], 0.0, 1.0)
                })
        })
    });
    if !keyframes_ok {
        return Err(
            "Invalid ltx.keyframes: 1-8 {image, frame 0..frames-1 or -1, strength 0-1}".into(),
        );
    }
    if source != l.get("sourceStrength").is_some()
        || l.get("sourceStrength")
            .is_some_and(|s| !finite(s, 0.0, 1.0))
    {
        return Err("ltx.sourceStrength (0-1) belongs exactly to a sourceVideo".into());
    }
    if keys_in(
        l,
        &[
            "textProjection",
            "audioVae",
            "passes",
            "spatialUpscaler",
            "keyframes",
            "sourceStrength",
        ],
    ) && ["textProjection", "audioVae"]
        .iter()
        .all(|k| l[*k].as_str().is_some_and(comfy_model_name))
        && (1..=3).contains(&passes)
        && (passes == 1) == l.get("spatialUpscaler").is_none()
        && l.get("spatialUpscaler")
            .is_none_or(|s| s.as_str().is_some_and(comfy_model_name))
    {
        Ok(())
    } else {
        Err("Invalid ltx: {textProjection, audioVae, passes 1-3, spatialUpscaler exactly when passes > 1}".into())
    }
}

fn validate_wan(w: &Value, steps: u64) -> Result<(), String> {
    if keys_in(w, &["lowModel", "shiftHigh", "shiftLow", "refinerStep"])
        && w["lowModel"].as_str().is_some_and(comfy_model_name)
        && finite(&w["shiftHigh"], 0.0, 100.0)
        && finite(&w["shiftLow"], 0.0, 100.0)
        && w["refinerStep"]
            .as_u64()
            .is_some_and(|r| r >= 1 && r < steps)
    {
        Ok(())
    } else {
        Err("Invalid wan: {lowModel, shiftHigh, shiftLow, refinerStep below steps}".into())
    }
}

fn validate_h3(h: &Value) -> Result<(), String> {
    let mode = h["mode"]
        .as_str()
        .filter(|m| H3_MODES.contains(m))
        .ok_or("Unsupported H3 mode (Image Inpaint is not a recipe yet)")?;
    let refs = h["references"].as_array().cloned().unwrap_or_default();
    if (mode == "REF2VA") != h.get("media").is_some() {
        return Err("h3.media belongs exactly to REF2VA".into());
    }
    let refmods = h.get("refmods").is_some();
    if refmods && mode != "REF2VA" {
        return Err("h3.refmods belong to REF2VA".into());
    }
    if refmods {
        validate_refmods(&h["refmods"])?;
    }
    if mode == "REF2VA" {
        let allow_empty = c_source(h) && h["continuity"]["useReferences"] != true;
        validate_h3_media(&h["media"], refmods, allow_empty)?;
    }
    let slots: Vec<u64> = refs.iter().filter_map(|r| r["slot"].as_u64()).collect();
    // Director slot semantics: I2VA = slot 0, L2VA = slot 1, FL2VA = {0,1}, T2VA = none.
    let slots_ok = refs.len() == slots.len()
        && refs.iter().all(|r| {
            r.as_object().is_some_and(|o| o.len() == 2)
                && r["image"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("deas-inputs/") && model_name(s))
        })
        && match mode {
            "T2VA" | "REF2VA" => slots.is_empty(),
            "I2VA" => slots == [0],
            "L2VA" => slots == [1],
            _ => {
                !slots.is_empty()
                    && slots.len() <= 2
                    && slots.iter().all(|s| *s <= 1)
                    && slots.windows(2).all(|w| w[0] < w[1])
            }
        };
    let cont_ok = h.get("continuity").is_none_or(|c| {
        keys_in(
            c,
            &[
                "session",
                "sourceId",
                "sourceKind",
                "capture",
                "action",
                "outputMode",
                "overlapFrames",
                "useReferences",
                "durationSeconds",
                "sourceFrames",
            ],
        ) && c["session"]
            .as_str()
            .is_some_and(|s| continuity_id(s) && s != "_imports")
            && c["sourceId"]
                .as_str()
                .is_some_and(|s| s.is_empty() || continuity_id(s))
            && c.get("sourceKind").is_none_or(|kind| {
                kind == "video"
                    && c["sourceId"].as_str().is_some_and(|s| {
                        s.len() == 64
                            && s.bytes()
                                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    })
            })
            && c["capture"].is_boolean()
            && c.get("useReferences").is_none_or(Value::is_boolean)
            && c.get("durationSeconds").is_none_or(|seconds| {
                mode == "REF2VA"
                    && c_source(h)
                    && seconds
                        .as_f64()
                        .is_some_and(|s| s.is_finite() && s > 0.0 && s <= 15.0)
            })
            && c.get("sourceFrames")
                .is_none_or(|n| n.as_u64().is_some_and(|n| n >= 5 && (n - 5) % 17 == 0))
            && c["action"].as_str().is_some_and(|a| a.len() <= 50_000)
            && pick(c, "outputMode", &["full", "segment"])
            && c.get("overlapFrames")
                .is_none_or(|v| v.as_u64().is_some_and(|n| [73, 56, 39, 22, 5].contains(&n)))
    });
    if keys_in(
        h,
        &[
            "mode",
            "audioVae",
            "shiftVideo",
            "shiftAudio",
            "references",
            "continuity",
            "media",
            "inputScaling",
            "refmods",
        ],
    ) && h["audioVae"].as_str().is_some_and(comfy_model_name)
        && (h.get("inputScaling").is_none() || pick(h, "inputScaling", &H3_INPUT_SCALING))
        && finite(&h["shiftVideo"], 0.01, 100.0)
        && finite(&h["shiftAudio"], 0.01, 100.0)
        && slots_ok
        && cont_ok
    {
        Ok(())
    } else {
        Err("Invalid h3: {mode, audioVae, shiftVideo, shiftAudio, references[{slot, image in deas-inputs/}], continuity?, inputScaling?}".into())
    }
}

fn c_source(h: &Value) -> bool {
    h["continuity"]["sourceId"]
        .as_str()
        .is_some_and(|s| !s.is_empty())
}

/// REF2VA timeline media, mirroring the DaSiWa Director limits: ≤ 9 images, ≤ 3 videos, ≤ 3 audio, ≤ 12 files;
/// audio needs an image or video; unique slots per lane. Clip durations (2–15 s) are checked by the node.
/// Director RefMod rows (`_load_refmod_rows`): 1–8 unique slots 1–8, a library name, strength in (0, 1]
/// (the node skips 0), optional description. DEAS sends enabled rows only.
fn validate_refmods(r: &Value) -> Result<(), String> {
    let rows = r
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= 8)
        .ok_or("h3.refmods: 1-8 rows")?;
    let mut slots = vec![];
    for x in rows {
        let slot = x["slot"].as_u64().filter(|s| (1..=8).contains(s));
        let ok = keys_in(x, &["slot", "name", "strength", "description"])
            && slot.is_some_and(|s| !slots.contains(&s))
            && x["name"]
                .as_str()
                .is_some_and(|n| model_name(n) && n != "None" && n != "(none)")
            && x["strength"].as_f64().is_some_and(|s| s > 0.0 && s <= 1.0)
            && x.get("description")
                .is_none_or(|d| d.as_str().is_some_and(|d| d.chars().count() <= 2000));
        if !ok {
            return Err(
                "Invalid h3.refmods row: {slot 1-8 unique, name, strength (0, 1], description?}"
                    .into(),
            );
        }
        slots.extend(slot);
    }
    Ok(())
}

/// Empty media is valid with RefMods or a continuation that ignores references; present items still validate.
fn validate_h3_media(m: &Value, refmods: bool, allow_empty: bool) -> Result<(), String> {
    let items = m
        .as_array()
        .filter(|a| (allow_empty || refmods || !a.is_empty()) && a.len() <= 12)
        .ok_or("h3.media: 1-12 items")?;
    let mut used: Vec<(&str, u64)> = vec![];
    let (mut videos, mut audio, mut visual) = (0, 0, 0);
    let (mut video_seconds, mut audio_seconds) = (0.0, 0.0);
    for x in items {
        let kind = x["type"].as_str().unwrap_or_default();
        let mode = x.get("mediaMode").and_then(Value::as_str);
        let trims = ["trimStart", "trimEnd"]
            .iter()
            .all(|k| x.get(*k).is_none_or(|t| finite(t, 0.0, 3600.0)))
            && match (x["trimStart"].as_f64(), x["trimEnd"].as_f64()) {
                (Some(a), Some(b)) => a < b,
                _ => true,
            };
        let ok = keys_in(
            x,
            &["type", "file", "slot", "mediaMode", "trimStart", "trimEnd"],
        ) && x["file"].as_str().is_some_and(|f| {
            (f.starts_with("deas-inputs/") || f.starts_with("deas-input-")) && model_name(f)
        }) && match kind {
            "image" => mode.is_none() && x.get("trimStart").is_none() && x.get("trimEnd").is_none(),
            "video" => mode.is_none_or(|m| H3_MEDIA_MODES.contains(&m)) && trims,
            "audio" => mode.is_none() && trims,
            _ => false,
        };
        // A video used as sound only sits in the audio lane.
        let lane = if kind == "video" && mode == Some("audio") {
            "audio"
        } else {
            kind
        };
        let slot = x["slot"].as_u64();
        let max = H3_LANES
            .iter()
            .find(|(l, _)| *l == lane)
            .map_or(0, |(_, n)| *n);
        if !ok || !slot.is_some_and(|s| s < max) || used.contains(&(lane, slot.unwrap())) {
            return Err("Invalid h3.media item: {type image|video|audio, file in deas-inputs/, slot unique per lane, mediaMode? (video), trimStart/trimEnd? (video/audio)}".into());
        }
        used.push((lane, slot.unwrap()));
        if kind != "image" {
            if let Some(end) = x["trimEnd"].as_f64() {
                let seconds = end - x["trimStart"].as_f64().unwrap_or(0.0);
                if !(2.0..=15.0).contains(&seconds) {
                    return Err("REF2VA clips must last 2-15 seconds".into());
                }
                if kind == "video" && mode != Some("audio") {
                    video_seconds += seconds;
                }
                if kind == "audio" || mode == Some("audio") || mode == Some("video_audio") {
                    audio_seconds += seconds;
                }
            }
        }
        match (kind, mode) {
            ("image", _) => visual += 1,
            ("video", Some("audio")) => audio += 1,
            ("video", Some("video_audio")) => {
                videos += 1;
                audio += 1;
                visual += 1;
            }
            ("video", _) => {
                videos += 1;
                visual += 1;
            }
            _ => audio += 1,
        }
    }
    if visual + audio > 12 {
        return Err("REF2VA allows at most 12 references (video with sound counts twice)".into());
    }
    if video_seconds > 15.0 || audio_seconds > 15.0 {
        return Err("REF2VA video and audio totals must each be at most 15 seconds".into());
    }
    if videos > 3 || audio > 3 {
        return Err("REF2VA allows at most 3 videos and 3 audio clips".into());
    }
    if audio > 0 && visual == 0 && !refmods {
        return Err("REF2VA audio needs an image or video reference".into());
    }
    Ok(())
}

/// Family-specific block key.
pub(crate) fn block(family: &str) -> &'static str {
    match family {
        "ltx-2" => "ltx",
        "wan" => "wan",
        _ => "h3",
    }
}

pub(crate) fn validate(i: &Value) -> Result<&str, String> {
    let object = i.as_object().ok_or("Expected a video intent object")?;
    let family = i["family"]
        .as_str()
        .filter(|f| FAMILIES.contains(f))
        .ok_or("Unknown video family")?;
    let block = block(family);
    if object
        .keys()
        .any(|k| !(FIELDS.contains(&k.as_str()) || OPTIONAL.contains(&k.as_str()) || k == block))
        || FIELDS.iter().any(|k| !object.contains_key(*k))
        || !object.contains_key(block)
    {
        return Err("Missing or unknown video intent fields".into());
    }
    if i["version"].as_f64() != Some(1.0) {
        return Err("Unsupported video recipe version".into());
    }
    for f in ["positivePrompt", "negativePrompt"] {
        if !i[f]
            .as_str()
            .is_some_and(|s| s.encode_utf16().count() <= 16384 && !s.contains('\0'))
        {
            return Err(format!("Invalid {f}"));
        }
    }
    if i.get("sourceVideo").is_some_and(|v| {
        // core LoadVideo lists top-level input files only.
        !v.as_str()
            .is_some_and(|s| model_name(s) && !s.contains('/'))
    }) || (i.get("sourceVideo").is_some() && family != "ltx-2")
    {
        return Err("sourceVideo: a top-level input video, LTX only".into());
    }
    for f in ["model", "textEncoder", "vae", "firstFrame", "lastFrame"] {
        if let Some(v) = i.get(f) {
            let valid = if ["model", "textEncoder", "vae"].contains(&f) {
                comfy_model_name
            } else {
                model_name
            };
            if !v.as_str().is_some_and(valid) {
                return Err(format!("Unsafe {f}"));
            }
        }
    }
    let (multiple, step, offset) = grid(family);
    let int = |k: &str, min: u64, max: u64| {
        i[k].as_u64()
            .filter(|n| (min..=max).contains(n))
            .ok_or(format!("Invalid {k}"))
    };
    let (w, h) = (int("width", 64, 8192)?, int("height", 64, 8192)?);
    let frames = int("frames", offset.max(1), 3600)?;
    if w % multiple != 0 || h % multiple != 0 || w * h > 16_777_216 || (frames - offset) % step != 0
    {
        return Err(format!(
            "Canvas must be a multiple of {multiple} and frames {step}k+{offset}"
        ));
    }
    int("seed", 0, 9_007_199_254_740_991)?;
    let steps = int("steps", 1, 150)?;
    if !finite(&i["frameRate"], 1.0, 120.0) || !finite(&i["cfg"], 0.0, 30.0) {
        return Err("Invalid frameRate or cfg".into());
    }
    if !["sampler", "scheduler"]
        .iter()
        .all(|f| i[*f].as_str().is_some_and(safe_choice))
    {
        return Err("Unsafe sampler or scheduler".into());
    }
    if i.get("chunks")
        .is_some_and(|c| !c.as_u64().is_some_and(|n| (2..=16).contains(&n)))
    {
        return Err("Invalid chunks: 2..=16".into());
    }
    if i.get("output").is_some_and(|o| !valid_output(o)) {
        return Err("Invalid output settings".into());
    }
    if let Some(p) = i.get("videoPost") {
        crate::jobs_video_post::validate(p)?;
    }
    if i.get("loras").is_some_and(|l| {
        !l.as_array().is_some_and(|a| {
            a.len() <= 8
                && a.iter().all(|x| {
                    x.as_object().is_some_and(|o| o.len() == 2)
                        && x["name"].as_str().is_some_and(comfy_model_name)
                        && finite(&x["strength"], -10.0, 10.0)
                })
        })
    }) {
        return Err("Invalid loras".into());
    }
    if i.get("options").is_some_and(|o| !valid_options(o)) {
        return Err("Invalid options".into());
    }
    if i.get("outputPrefix")
        .is_some_and(|p| !p.as_str().is_some_and(safe_output_prefix))
    {
        return Err("Unsafe output prefix".into());
    }
    match family {
        "ltx-2" => validate_ltx(&i["ltx"], frames, i.get("sourceVideo").is_some())?,
        "wan" => validate_wan(&i["wan"], steps)?,
        _ => validate_h3(&i["h3"])?,
    }
    for name in [&i["vae"], &i["ltx"]["audioVae"], &i["h3"]["audioVae"]] {
        if crate::jobs::is_gguf(name.as_str().unwrap_or_default()) {
            return Err("MODEL_FORMAT_UNSUPPORTED: GGUF VAE is unsupported".into());
        }
    }
    if crate::jobs::is_gguf(i["ltx"]["textProjection"].as_str().unwrap_or_default()) {
        return Err("MODEL_FORMAT_UNSUPPORTED: GGUF text projection is unsupported".into());
    }
    validate_latent_upscale(i, family)?;
    for lora in i["loras"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l["name"].as_str())
    {
        crate::jobs::validate_lora_format(lora)?;
    }
    // LoRA on a quantized base needs execution proof per pipeline; not advertised yet.
    if i["loras"].as_array().is_some_and(|l| !l.is_empty())
        && [&i["model"], &i["wan"]["lowModel"]]
            .iter()
            .any(|name| crate::jobs::is_gguf(name.as_str().unwrap_or_default()))
    {
        return Err("MODEL_FORMAT_UNSUPPORTED: LoRA on a GGUF base is not proven".into());
    }
    Ok(family)
}

pub(crate) fn compile(i: &Value) -> Result<Value, String> {
    if crate::jobs_video_post::is_enhance(i) {
        return crate::jobs_video_post::compile_enhance(i);
    }
    match validate(i)? {
        "ltx-2" => Ok(ltx(i)),
        "wan" => wan(i),
        _ => Ok(h3(i)),
    }
}

/// LoRAs, then the Options patches, then the family's feed-forward chunking; returns the final model link.
/// `clip` is `None` for model-only chains (Wan high/low, H3).
fn model_chain(
    g: &mut Value,
    prefix: &str,
    mut model: Value,
    mut clip: Option<&mut Value>,
    i: &Value,
    chunk_node: &str,
    threshold_key: &str,
) -> Value {
    for (n, lora) in i["loras"].as_array().into_iter().flatten().enumerate() {
        let id = format!("{prefix}L{}", n + 1);
        match clip.as_deref_mut() {
            Some(c) => {
                g[&id] = json!({"class_type":"LoraLoader","inputs":{"model":model,"clip":c.clone(),"lora_name":lora["name"],"strength_model":lora["strength"],"strength_clip":lora["strength"]}});
                *c = json!([id, 1]);
            }
            None => {
                g[&id] = json!({"class_type":"LoraLoaderModelOnly","inputs":{"model":model,"lora_name":lora["name"],"strength_model":lora["strength"]}})
            }
        }
        model = json!([id, 0]);
    }
    let o = &i["options"];
    let mut patch = |suffix: &str, node: &str, mut inputs: Value| {
        let id = format!("{prefix}{suffix}");
        inputs["model"] = model.clone();
        g[&id] = json!({"class_type":node,"inputs":inputs});
        model = json!([id, 0]);
    };
    if let Some(a) = o["attention"].as_str() {
        patch("A", "ModelAttentionBackend", json!({"attention":a}));
    }
    if let Some(s) = o["sage"].as_str() {
        patch("K", "PathchSageAttentionKJ", json!({"sage_attention":s}));
    }
    if let Some(c) = o.get("cache") {
        patch(
            "E",
            c["node"].as_str().unwrap(),
            json!({"reuse_threshold":c["threshold"],"start_percent":0.15,"end_percent":0.95,"verbose":false}),
        );
    }
    if let Some(n) = i["chunks"].as_u64() {
        // 4096 is the KJ default; the owner's workflows keep it.
        patch("H", chunk_node, json!({"chunks":n, threshold_key:4096}));
    }
    if let Some(b) = o["compile"].as_str() {
        patch("T", "TorchCompileModel", json!({"backend":b}));
    }
    model
}

/// Reference image → canvas (the workflows' TorchResize "Fill and crop" ≙ ImageScale crop=center).
fn input_image(g: &mut Value, id: &str, name: &Value, w: &Value, h: &Value) -> Value {
    g[id] = json!({"class_type":"LoadImage","inputs":{"image":name}});
    let scaled = format!("{id}S");
    g[&scaled] = json!({"class_type":"ImageScale","inputs":{"image":[id,0],"upscale_method":"lanczos","width":w,"height":h,"crop":"center"}});
    json!([scaled, 0])
}

fn decode(g: &mut Value, id: &str, samples: Value, vae: Value, i: &Value) {
    g[id] = match i["options"]["vaeTile"].as_u64() {
        Some(t) => {
            json!({"class_type":"VAEDecodeTiled","inputs":{"samples":samples,"vae":vae,"tile_size":t,"overlap":64,"temporal_size":64,"temporal_overlap":8}})
        }
        None => json!({"class_type":"VAEDecode","inputs":{"samples":samples,"vae":vae}}),
    };
}

/// Effective native combine settings, shared by execution and immutable history projection.
pub(crate) fn effective_output(i: &Value) -> Value {
    let mut output = json!({
        "codec":"Auto","container":"Auto","bitDepth":"Auto","quality":20,
        "audioCodec":"Auto","audioBitrate":"192k","saveFirstFrame":false,"saveLastFrame":false
    });
    if let Some(selected) = i["output"].as_object() {
        for (key, value) in selected {
            output[key] = value.clone();
        }
    }
    output
}

pub(crate) fn effective_output_prefix(i: &Value) -> Value {
    i.get("outputPrefix").cloned().unwrap_or_else(|| {
        json!(if crate::jobs_video_post::is_enhance(i) {
            "raw/tve/DEAS"
        } else {
            "raw/video/DEAS"
        })
    })
}

/// DaSiWa Enhanced Video Combine as node "7": the history reader looks there.
pub(crate) fn combine(
    g: &mut Value,
    images: Value,
    audio: Option<Value>,
    frame_rate: Value,
    i: &Value,
) {
    let o = effective_output(i);
    let mut inputs = json!({
        "images":images,"frame_rate":frame_rate,
        "codec":o["codec"],"container":o["container"],"bit_depth":o["bitDepth"],
        "quality":o["quality"],"log_level":"Standard","pingpong":false,"save_metadata":true,
        "filename_prefix":effective_output_prefix(i),"save_output":true,"pass_frames":false,"crop_to_audio":false,
        "audio_codec":o["audioCodec"],"audio_bitrate":o["audioBitrate"],
        "save_first_frame":o["saveFirstFrame"],"save_last_frame":o["saveLastFrame"],
        "seed":i["seed"]
    });
    if let Some(a) = audio {
        inputs["audio"] = a;
    }
    g["7"] = json!({"class_type":"DaSiWa_EnhancedVideoCombine","inputs":inputs});
}

/// Unprocessed baseline as "B7" at the pre-chain frame rate.
fn baseline_combine(
    g: &mut Value,
    images: Value,
    audio: Option<Value>,
    frame_rate: Value,
    i: &Value,
) {
    let mut saver = g["7"].clone();
    saver["inputs"]["images"] = images;
    match audio {
        Some(a) => saver["inputs"]["audio"] = a,
        None => {
            saver["inputs"].as_object_mut().unwrap().remove("audio");
        }
    }
    saver["inputs"]["frame_rate"] = frame_rate;
    saver["inputs"]["filename_prefix"] = json!(format!(
        "{}_original",
        effective_output_prefix(i).as_str().unwrap()
    ));
    g["B7"] = saver;
}

/// Optional TVE chain after the decode: returns (combine images, combine frame rate).
fn post(g: &mut Value, images: Value, frame_rate: Value, i: &Value) -> (Value, Value) {
    match i.get("videoPost") {
        Some(p) => {
            let out = crate::jobs_video_post::chain(g, images, p);
            let m = crate::jobs_video_post::multiplier(p);
            let fps = if m > 1 {
                json!(i["frameRate"].as_f64().unwrap() * m as f64)
            } else {
                frame_rate
            };
            (out, fps)
        }
        None => (images, frame_rate),
    }
}

/// The baseline is the first decode: before the latent upscale when there is one, else before the TVE chain.
fn needs_post_baseline(i: &Value) -> bool {
    i.get("videoPost").is_some() && i.get("latentUpscale").is_none()
}

/// One LTX sampling pass. `suffix` "" is the first pass; guides are re-added on the given video latent,
/// AV concat → SamplerCustomAdvanced → separate → crop guides. Returns (video latent link, audio latent link).
#[allow(clippy::too_many_arguments)]
fn ltx_pass(
    g: &mut Value,
    suffix: &str,
    video: Value,
    audio: Value,
    model: &Value,
    guides: &[(Value, i64, f64)],
    steps: Value,
    denoise: f64,
    i: &Value,
) -> (Value, Value) {
    let (mut cond, mut latent) = (json!(["CN"]), video);
    for (n, (image, idx, strength)) in guides.iter().enumerate() {
        let id = format!("G{}{suffix}", n + 1);
        g[&id] = json!({"class_type":"LTXVAddGuide","inputs":{"positive":[cond[0].clone(),0],"negative":[cond[0].clone(),1],"vae":["V",0],"latent":latent,"image":image,"frame_idx":idx,"strength":strength}});
        cond = json!([id]);
        latent = json!([id, 2]);
    }
    let (pos, neg) = (json!([cond[0].clone(), 0]), json!([cond[0].clone(), 1]));
    let (cat, z, gu, s, sep, cr) = (
        format!("CAT{suffix}"),
        format!("Z{suffix}"),
        format!("GU{suffix}"),
        format!("5{suffix}"),
        format!("SEP{suffix}"),
        format!("CR{suffix}"),
    );
    g[&cat] = json!({"class_type":"LTXVConcatAVLatent","inputs":{"video_latent":latent,"audio_latent":audio}});
    g[&z] = json!({"class_type":"BasicScheduler","inputs":{"model":model,"scheduler":i["scheduler"],"steps":steps,"denoise":denoise}});
    g[&gu] = json!({"class_type":"CFGGuider","inputs":{"model":model,"positive":pos,"negative":neg,"cfg":i["cfg"]}});
    g[&s] = json!({"class_type":"SamplerCustomAdvanced","inputs":{"noise":["N",0],"guider":[gu,0],"sampler":["Q",0],"sigmas":[z,0],"latent_image":[cat,0]}});
    g[&sep] = json!({"class_type":"LTXVSeparateAVLatent","inputs":{"av_latent":[s,0]}});
    let audio_out = json!([sep.clone(), 1]);
    if guides.is_empty() {
        return (json!([sep, 0]), audio_out);
    }
    g[&cr] = json!({"class_type":"LTXVCropGuides","inputs":{"positive":pos,"negative":neg,"latent":[sep,0]}});
    (json!([cr, 2]), audio_out)
}

fn ltx(i: &Value) -> Value {
    let l = &i["ltx"];
    let passes = l["passes"].as_u64().unwrap();
    let (w, h) = (i["width"].as_u64().unwrap(), i["height"].as_u64().unwrap());
    // Multipass samples the first pass at half canvas (OmniForge), then upsamples the latent 2×.
    let (w1, h1) = if passes > 1 { (w / 2, h / 2) } else { (w, h) };
    let mut g = json!({
        "1":unet_loader(&i["model"]),
        "C":crate::jobs::dual_clip_loader(&i["textEncoder"], &l["textProjection"], "ltxv"),
        "V":{"class_type":"VAELoader","inputs":{"vae_name":i["vae"]}},
        "AV":{"class_type":"VAELoader","inputs":{"vae_name":l["audioVae"]}},
        "N":{"class_type":"RandomNoise","inputs":{"noise_seed":i["seed"]}},
        "Q":{"class_type":"KSamplerSelect","inputs":{"sampler_name":i["sampler"]}},
        "EL":{"class_type":"EmptyLTXVLatentVideo","inputs":{"width":w1,"height":h1,"length":i["frames"],"batch_size":1}},
        "EA":{"class_type":"LTXVEmptyLatentAudio","inputs":{"frames_number":i["frames"],"frame_rate":i["frameRate"],"batch_size":1,"audio_vae":["AV",0]}},
    });
    let mut clip = json!(["C", 0]);
    let model = model_chain(
        &mut g,
        "",
        json!(["1", 0]),
        Some(&mut clip),
        i,
        "LTXVChunkFeedForward",
        "dim_threshold",
    );
    g["2"] =
        json!({"class_type":"CLIPTextEncode","inputs":{"text":i["positivePrompt"],"clip":clip}});
    g["3"] =
        json!({"class_type":"CLIPTextEncode","inputs":{"text":i["negativePrompt"],"clip":clip}});
    g["CN"] = json!({"class_type":"LTXVConditioning","inputs":{"positive":["2",0],"negative":["3",0],"frame_rate":i["frameRate"]}});
    // Guides the way the Director adds them, after LTXVPreprocess(img_compression 18): a source video as one
    // multi-frame guide at 0 (video-to-video), first frame at 0, keyframe images at their frame, last frame at -1.
    let mut sources: Vec<(String, Value, i64, f64)> = vec![];
    if let Some(video) = i.get("sourceVideo") {
        g["SV"] = json!({"class_type":"LoadVideo","inputs":{"file":video}});
        g["SVC"] = json!({"class_type":"GetVideoComponents","inputs":{"video":["SV",0]}});
        // shortcut: source frames are taken at the file's own rate (as the WhatDreamsCost Director does); resample when fps differ noticeably.
        g["SVT"] = json!({"class_type":"ImageFromBatch","inputs":{"image":["SVC",0],"batch_index":0,"length":i["frames"]}});
        g["SVS"] = json!({"class_type":"ImageScale","inputs":{"image":["SVT",0],"upscale_method":"lanczos","width":i["width"],"height":i["height"],"crop":"center"}});
        sources.push((
            "SV".into(),
            json!(["SVS", 0]),
            0,
            l["sourceStrength"].as_f64().unwrap(),
        ));
    }
    for (key, src, idx) in [("firstFrame", "8", 0i64), ("lastFrame", "9", -1)] {
        if let Some(name) = i.get(key) {
            let scaled = input_image(&mut g, src, name, &i["width"], &i["height"]);
            sources.push((src.into(), scaled, idx, 1.0));
        }
    }
    for (n, k) in l["keyframes"].as_array().into_iter().flatten().enumerate() {
        let src = format!("K{}", n + 1);
        let scaled = input_image(&mut g, &src, &k["image"], &i["width"], &i["height"]);
        sources.push((
            src,
            scaled,
            k["frame"].as_i64().unwrap(),
            k["strength"].as_f64().unwrap(),
        ));
    }
    let mut full = vec![];
    let mut half = vec![];
    for (src, scaled, idx, strength) in sources {
        g[&format!("{src}P")] =
            json!({"class_type":"LTXVPreprocess","inputs":{"image":scaled,"img_compression":18}});
        full.push((json!([format!("{src}P"), 0]), idx, strength));
        if passes > 1 {
            g[&format!("{src}H")] = json!({"class_type":"ImageScale","inputs":{"image":[format!("{src}P"),0],"upscale_method":"lanczos","width":w1,"height":h1,"crop":"disabled"}});
            half.push((json!([format!("{src}H"), 0]), idx, strength));
        }
    }
    let guides = if passes > 1 { &half } else { &full };
    let (mut video, mut audio) = ltx_pass(
        &mut g,
        "",
        json!(["EL", 0]),
        json!(["EA", 0]),
        &model,
        guides,
        i["steps"].clone(),
        1.0,
        i,
    );
    if passes > 1 {
        g["UL"] = json!({"class_type":"LatentUpscaleModelLoader","inputs":{"model_name":l["spatialUpscaler"]}});
        g["UP"] = json!({"class_type":"LTXVLatentUpsampler","inputs":{"samples":video,"upscale_model":["UL",0],"vae":["V",0]}});
        // OmniForge pass 2/3 steps and denoise.
        (video, audio) = ltx_pass(
            &mut g,
            "b",
            json!(["UP", 0]),
            audio,
            &model,
            &full,
            json!(4),
            0.4,
            i,
        );
        if passes == 3 {
            (video, audio) = ltx_pass(&mut g, "c", video, audio, &model, &full, json!(2), 0.2, i);
        }
    }
    if let Some(u) = i.get("latentUpscale") {
        decode(&mut g, "B6", video.clone(), json!(["V", 0]), i);
        g["UML"] =
            json!({"class_type":"LatentUpscaleModelLoader","inputs":{"model_name":u["model"]}});
        g["U"] = json!({"class_type":"LTXVLatentUpsampler","inputs":{"samples":video,"upscale_model":["UML",0],"vae":["V",0]}});
        video = json!(["U", 0]);
    }
    decode(&mut g, "6", video, json!(["V", 0]), i);
    g["AD"] =
        json!({"class_type":"LTXVAudioVAEDecode","inputs":{"samples":audio,"audio_vae":["AV",0]}});
    let (images, fps) = post(&mut g, json!(["6", 0]), i["frameRate"].clone(), i);
    combine(&mut g, images, Some(json!(["AD", 0])), fps, i);
    if i.get("latentUpscale").is_some() {
        baseline_combine(
            &mut g,
            json!(["B6", 0]),
            Some(json!(["AD", 0])),
            i["frameRate"].clone(),
            i,
        );
    } else if needs_post_baseline(i) {
        baseline_combine(
            &mut g,
            json!(["6", 0]),
            Some(json!(["AD", 0])),
            i["frameRate"].clone(),
            i,
        );
    }
    g
}

fn wan(i: &Value) -> Result<Value, String> {
    let w = &i["wan"];
    if i.get("firstFrame").is_none() {
        return Err("Wan 2.2 image-to-video needs a first frame".into());
    }
    let mut g = json!({
        "1":unet_loader(&i["model"]),
        "1L":unet_loader(&w["lowModel"]),
        "C":crate::jobs::clip_loader(&i["textEncoder"], &json!("wan")),
        "V":{"class_type":"VAELoader","inputs":{"vae_name":i["vae"]}},
        "2":{"class_type":"CLIPTextEncode","inputs":{"text":i["positivePrompt"],"clip":["C",0]}},
        "3":{"class_type":"CLIPTextEncode","inputs":{"text":i["negativePrompt"],"clip":["C",0]}},
    });
    // shortcut: one LoRA list with one strength on both high and low; upgrade to per-model lists for split LoRAs (lightx2v).
    let high = model_chain(
        &mut g,
        "",
        json!(["1", 0]),
        None,
        i,
        "WanChunkFeedForward",
        "dim_threshold",
    );
    let low = model_chain(
        &mut g,
        "l",
        json!(["1L", 0]),
        None,
        i,
        "WanChunkFeedForward",
        "dim_threshold",
    );
    g["SH"] =
        json!({"class_type":"ModelSamplingSD3","inputs":{"model":high,"shift":w["shiftHigh"]}});
    g["SL"] = json!({"class_type":"ModelSamplingSD3","inputs":{"model":low,"shift":w["shiftLow"]}});
    let start = input_image(&mut g, "8", &i["firstFrame"], &i["width"], &i["height"]);
    let mut inputs = json!({"positive":["2",0],"negative":["3",0],"vae":["V",0],"width":i["width"],"height":i["height"],"length":i["frames"],"batch_size":1,"start_image":start});
    let node = match i.get("lastFrame") {
        Some(last) => {
            inputs["end_image"] = input_image(&mut g, "9", last, &i["width"], &i["height"]);
            "WanFirstLastFrameToVideo"
        }
        None => "WanImageToVideo",
    };
    g["W"] = json!({"class_type":node,"inputs":inputs});
    let sampler = |model: Value, latent: Value, first: bool| {
        json!({"class_type":"KSamplerAdvanced","inputs":{
            "model":model,"add_noise":if first {"enable"} else {"disable"},"noise_seed":i["seed"],"steps":i["steps"],"cfg":i["cfg"],
            "sampler_name":i["sampler"],"scheduler":i["scheduler"],"positive":["W",0],"negative":["W",1],"latent_image":latent,
            "start_at_step":if first {json!(0)} else {w["refinerStep"].clone()},"end_at_step":if first {w["refinerStep"].clone()} else {json!(10000)},
            "return_with_leftover_noise":if first {"enable"} else {"disable"}}})
    };
    g["KH"] = sampler(json!(["SH", 0]), json!(["W", 2]), true);
    g["KL"] = sampler(json!(["SL", 0]), json!(["KH", 0]), false);
    decode(&mut g, "6", json!(["KL", 0]), json!(["V", 0]), i);
    let (images, fps) = post(&mut g, json!(["6", 0]), i["frameRate"].clone(), i);
    combine(&mut g, images, None, fps, i);
    if needs_post_baseline(i) {
        baseline_combine(&mut g, json!(["6", 0]), None, i["frameRate"].clone(), i);
    }
    Ok(g)
}

fn h3(i: &Value) -> Value {
    let h = &i["h3"];
    let mut items: Vec<Value> = h["references"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(n, r)| json!({"id":format!("ref{}",r["slot"]),"type":"image","value":r["image"],"slot":r["slot"],"order":n,"enabled":true}))
        .collect();
    // REF2VA timeline media (Director item contract: value = input-relative file, trims in seconds).
    for (n, m) in h["media"].as_array().into_iter().flatten().enumerate() {
        let mut item = json!({"id":format!("m{n}"),"type":m["type"],"value":m["file"],"slot":m["slot"],"order":n,"enabled":true});
        for (from, to) in [
            ("mediaMode", "media_mode"),
            ("trimStart", "trim_start"),
            ("trimEnd", "trim_end"),
        ] {
            if let Some(v) = m.get(from) {
                item[to] = v.clone();
            }
        }
        items.push(item);
    }
    // Director continuity contract (v3): an empty source_id is a new take; parse_settings fills the rest.
    let c = &h["continuity"];
    let mut continuity = if c_source(h) {
        let mut continuity = json!({"version":3,"session":c["session"],"capture":c["capture"],"output_mode":c["outputMode"],"continuation_prompt":c["action"]});
        if c["sourceKind"] == "video" {
            continuity["source_kind"] = json!("video");
            continuity["source_video_id"] = c["sourceId"].clone();
        } else {
            continuity["source_kind"] = json!("checkpoint");
            continuity["source_id"] = c["sourceId"].clone();
        }
        if let Some(frames) = c.get("overlapFrames") {
            continuity["overlap_frames"] = frames.clone();
        }
        continuity
    } else {
        json!({"version":3,"session":c.get("session").cloned().unwrap_or(json!("deas")),"capture":c.get("capture").cloned().unwrap_or(json!(false)),"output_mode":c.get("outputMode").cloned().unwrap_or(json!("full"))})
    };
    if let Some(use_references) = c.get("useReferences") {
        continuity["use_references"] = use_references.clone();
    }
    // DEAS already chose the canvas; by default the Director fills/crops the references to it.
    let scaling = h
        .get("inputScaling")
        .cloned()
        .unwrap_or(json!("Fill and crop"));
    let mut timeline = json!({"version":1,"items":items,"prompt_blocks":[],"resolution":{"input_scaling":scaling},"continuity":continuity});
    // RefMods (saved REF2VA references in models/refmods), loaded by the Director itself.
    if let Some(rows) = h["refmods"].as_array() {
        timeline["refmods"] = rows
            .iter()
            .map(|r| json!({"slot":r["slot"],"name":r["name"],"enabled":true,"strength":r["strength"],"description":r.get("description").cloned().unwrap_or(json!(""))}))
            .collect();
    }
    let mut g = json!({
        "1":unet_loader(&i["model"]),
        "C":crate::jobs::clip_loader(&i["textEncoder"], &json!("minimax")),
        "V":{"class_type":"VAELoader","inputs":{"vae_name":i["vae"]}},
        "AV":{"class_type":"VAELoader","inputs":{"vae_name":h["audioVae"]}},
        "N":{"class_type":"RandomNoise","inputs":{"noise_seed":i["seed"]}},
        "Q":{"class_type":"KSamplerSelect","inputs":{"sampler_name":i["sampler"]}},
    });
    let model = model_chain(
        &mut g,
        "",
        json!(["1", 0]),
        None,
        i,
        "MiniMaxChunkFeedForward",
        "seq_threshold",
    );
    // REF2VA extensions need authored seconds, not a roundtrip through the new-take 17k+5 grid.
    g["D"] = json!({"class_type":"MiniMaxH3Director","inputs":{"mode":h["mode"],"prompt":i["positivePrompt"],"width":i["width"],"height":i["height"],
        "duration":c.get("durationSeconds").cloned().unwrap_or_else(|| json!(i["frames"].as_f64().unwrap() / 24.0)),"ref_image_size":"match","timeline_data":timeline.to_string(),"builder_state":"","frame_rate":i["frameRate"]}});
    // The Director pulls only the socket of its mode: ref2va_model for REF2VA, fl2va_model otherwise.
    g["D"]["inputs"][if h["mode"] == "REF2VA" {
        "ref2va_model"
    } else {
        "fl2va_model"
    }] = model;
    g["G"] = json!({"class_type":"MiniMaxH3DirectorGuide","inputs":{"clip":["C",0],"vae":["V",0],"guide":["D",0],"audio_vae":["AV",0]}});
    g["SS"] = json!({"class_type":"MiniMaxH3SigmaShift","inputs":{"model":["D",5],"shift_video":h["shiftVideo"],"shift_audio":h["shiftAudio"]}});
    g["Z"] = json!({"class_type":"BasicScheduler","inputs":{"model":["SS",0],"scheduler":i["scheduler"],"steps":i["steps"],"denoise":1.0}});
    g["B"] = json!({"class_type":"BasicGuider","inputs":{"model":["SS",0],"conditioning":["G",0]}});
    g["5"] = json!({"class_type":"SamplerCustomAdvanced","inputs":{"noise":["N",0],"guider":["B",0],"sampler":["Q",0],"sigmas":["Z",0],"latent_image":["G",1]}});
    // Segment Prepare/Decode serve both Director output modes and are plain decode without capture.
    g["SP"] = json!({"class_type":"DaSiWaH3ContinuitySegmentPrepare","inputs":{"sampled":["5",0],"context":["G",2]}});
    g["SD"] = json!({"class_type":"DaSiWaH3ContinuitySegmentDecode","inputs":{"latent":["SP",0],"vae":["V",0],"audio_vae":["AV",0],"context":["SP",1]}});
    let (images, fps) = post(&mut g, json!(["SD", 0]), json!(["D", 8]), i);
    combine(&mut g, images, Some(json!(["SD", 1])), fps, i);
    if needs_post_baseline(i) {
        baseline_combine(
            &mut g,
            json!(["SD", 0]),
            Some(json!(["SD", 1])),
            json!(["D", 8]),
            i,
        );
    }
    if let Some(u) = i.get("latentUpscale") {
        // Capture mode allows exactly one SegmentDecode (node pack validation): in segment mode the
        // baseline side decodes the prepared window on core nodes, cropped to the new segment.
        let segment_capture = c["capture"] == true && c["outputMode"] == "segment";
        if !segment_capture {
            g["BSD"] = g["SD"].clone();
        }
        g["U"] = json!({"class_type":"DaSiWaH3TiledUpscale","inputs":{
            "model":["SS",0],"conditioning":["G",0],"latent":["SP",0],"continuity_context":["SP",1],
            "director_guide":["D",0],"vae":["V",0],"sampler":["Q",0],"noise":["N",0],"sampler_name":i["sampler"],
            "scale":u["scale"],"upscale_model":u["model"],"steps":u["steps"],"denoise":u["denoise"],"seed":i["seed"],
            "cfg":1.0,"scheduler":"simple","memory_budget_mb":0,"upscale_precision":"auto",
            "continuity_soft_refine":false,"continuity_mask_strength":1.0,
            "spatial_tiling":true,"temporal_chunking":true,"spike_guard":false,
            "guard_sensitivity":4.3,"guard_strength":1.0,"guard_mode":"burn_direction",
            "enforce_memory_budget":true,"memory_reserve_pct":5.0
        }});
        if let Some(a) = u.get("advanced") {
            for (key, input) in H3_LATENT_ADVANCED_INPUTS {
                if let Some(v) = a.get(key) {
                    g["U"]["inputs"][input] = v.clone();
                }
            }
        }
        g["SD"]["inputs"]["latent"] = json!(["U", 0]);
        if segment_capture {
            // The render plan is derived at runtime from the prepared latent tensor and context;
            // SegmentDecode accepts no duration input. The compile-time crop mirrors the pack's
            // continuation timing for the duration the Director received (durationSeconds else frames/24).
            let (crop_start, crop_frames) =
                segment_crop_plan(i).expect("validated in validate_latent_upscale");
            decode(&mut g, "BA", json!(["SP", 0]), json!(["V", 0]), i);
            g["CB"] = json!({"class_type":"ImageFromBatch","inputs":{"image":["BA",0],"batch_index":crop_start,"length":crop_frames}});
            baseline_combine(
                &mut g,
                json!(["CB", 0]),
                Some(json!(["SD", 1])),
                json!(["D", 8]),
                i,
            );
        } else {
            baseline_combine(
                &mut g,
                json!(["BSD", 0]),
                Some(json!(["BSD", 1])),
                json!(["D", 8]),
                i,
            );
        }
    }
    g["P"] = json!({"class_type":"DaSiWaH3ContinuityPublish","inputs":{"filename":["7",1],"ticket":["SP",2]}});
    g
}

fn strs<'a>(v: &[&'a Value]) -> Vec<&'a str> {
    v.iter().filter_map(|x| x.as_str()).collect()
}

/// GGUF files load through the ComfyUI-GGUF node, which takes no weight_dtype.
pub(crate) fn unet_loader(name: &Value) -> Value {
    if crate::jobs::is_gguf(name.as_str().unwrap_or_default()) {
        json!({"class_type":"UnetLoaderGGUF","inputs":{"unet_name":name}})
    } else {
        json!({"class_type":"UNETLoader","inputs":{"unet_name":name,"weight_dtype":"default"}})
    }
}

/// Model/encoder/VAE/image/upscaler names prepare() must find in object_info, per allowlist field.
pub(crate) fn requirements(i: &Value) -> Vec<(&'static str, Vec<&str>)> {
    if crate::jobs_video_post::is_enhance(i) {
        return crate::jobs_video_post::requirements(&i["videoPost"]);
    }
    let family = i["family"].as_str().unwrap_or_default();
    let b = &i[block(family)];
    let (gguf, plain): (Vec<&str>, Vec<&str>) = strs(&[&i["model"], &b["lowModel"]])
        .into_iter()
        .partition(|name| crate::jobs::is_gguf(name));
    let quantized_clip = crate::jobs::is_gguf(i["textEncoder"].as_str().unwrap_or_default());
    let (encoders, types) = match (quantized_clip, family == "ltx-2") {
        (true, true) => ("ggufDualTextEncoders", "ggufDualClipTypes"),
        (true, false) => ("ggufTextEncoders", "ggufClipTypes"),
        _ => ("textEncoders", "clipTypes"),
    };
    let mut required = vec![
        ("diffusionModels", plain),
        ("ggufDiffusionModels", gguf),
        (encoders, strs(&[&i["textEncoder"], &b["textProjection"]])),
        ("vaes", strs(&[&i["vae"], &b["audioVae"]])),
        (
            types,
            vec![match family {
                "ltx-2" => "ltxv",
                "wan" => "wan",
                _ => "minimax",
            }],
        ),
        (
            "images",
            // object_info lists only the input folder; ComfyUI resolves `[output]` files itself.
            strs(&[&i["firstFrame"], &i["lastFrame"]])
                .into_iter()
                .chain(
                    b["references"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|r| r["image"].as_str()),
                )
                .chain(
                    b["keyframes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|k| k["image"].as_str()),
                )
                .chain(
                    b["media"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|m| {
                            m["type"] == "image"
                                && !(family == "minimax-h3"
                                    && b["mode"] == "REF2VA"
                                    && c_source(b)
                                    && b["continuity"]["useReferences"] != true)
                        })
                        .filter_map(|m| m["file"].as_str()),
                )
                .filter(|s| !s.ends_with(" [output]"))
                .collect(),
        ),
        (
            "h3LatentUpscaleModels",
            (family == "minimax-h3")
                .then(|| i["latentUpscale"]["model"].as_str())
                .flatten()
                .into_iter()
                .collect(),
        ),
        (
            "latentUpscaleModels",
            strs(&[&b["spatialUpscaler"]])
                .into_iter()
                .chain(
                    (family == "ltx-2")
                        .then(|| i["latentUpscale"]["model"].as_str())
                        .flatten(),
                )
                .collect(),
        ),
    ];
    required.extend(crate::jobs_video_post::requirements(&i["videoPost"]));
    required
}

#[cfg(test)]
#[path = "jobs_video_tests.rs"]
pub(crate) mod tests;
