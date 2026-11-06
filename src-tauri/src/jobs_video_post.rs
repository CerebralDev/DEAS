//! TVE post-processing on decoded frames, the way the owner's "Post Processing" subgraph orders it:
//! model upscale → RTX refine (denoise/deblur/VSR) → frame generation (RTX or core RIFE) → combine.
//! Used as the optional `videoPost` block of `video-generation` intents and as the standalone
//! `video-enhance` recipe of the TVE tab (LoadVideo → chain → combine).
use crate::jobs::{comfy_model_name, model_name, safe_output_prefix};
use crate::jobs_video::{combine, valid_output};
use serde_json::{json, Value};

pub(crate) const RTX_QUALITIES: [&str; 4] = ["Low", "Medium", "High", "Ultra"];
pub(crate) const RTX_UPSCALE_MODES: [&str; 4] =
    ["VSR", "High Bitrate", "Streaming Medium", "Streaming Ultra"];
pub(crate) const FRAME_GEN_MODES: [&str; 3] = ["LOW", "MEDIUM", "HIGH"];
pub(crate) const POST_NODES: [&str; 6] = [
    "UpscaleModelLoader",
    "ImageUpscaleWithModel",
    "DaSiWa_RTX_UpscalerRefiner",
    "DaSiWa_RTX_FrameGeneration",
    "FrameInterpolationModelLoader",
    "FrameInterpolate",
];
const ENHANCE_FIELDS: [&str; 6] = [
    "recipeId",
    "version",
    "sourceVideo",
    "frameRate",
    "seed",
    "videoPost",
];
const ENHANCE_OPTIONAL: [&str; 2] = ["outputPrefix", "output"];

pub(crate) fn is_enhance(i: &Value) -> bool {
    i["recipeId"] == "video-enhance"
}

fn quality(v: &Value) -> bool {
    v.as_str().is_some_and(|q| RTX_QUALITIES.contains(&q))
}

fn keys_in(v: &Value, allowed: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| !o.is_empty() && o.keys().all(|k| allowed.contains(&k.as_str())))
}

fn multiplier_ok(v: &Value) -> bool {
    v.as_u64().is_some_and(|m| (2..=16).contains(&m))
}

/// `videoPost`: at least one stage; each stage only with the node's own enum/range values.
pub(crate) fn validate(p: &Value) -> Result<(), String> {
    if !keys_in(p, &["upscaleModel", "rtx", "frameGeneration", "useMmap"]) {
        return Err("videoPost needs upscaleModel, rtx or frameGeneration".into());
    }
    if p.get("useMmap").is_some_and(|v| !v.is_boolean()) {
        return Err("videoPost.useMmap must be a boolean".into());
    }
    if p.get("upscaleModel")
        .is_some_and(|m| !m.as_str().is_some_and(comfy_model_name))
    {
        return Err("Unsafe videoPost.upscaleModel".into());
    }
    if let Some(r) = p.get("rtx") {
        let upscale_ok = r.get("upscale").is_none_or(|u| {
            keys_in(u, &["mode", "quality", "scale"])
                && u["mode"]
                    .as_str()
                    .is_some_and(|m| RTX_UPSCALE_MODES.contains(&m))
                && quality(&u["quality"])
                && u["scale"]
                    .as_f64()
                    .is_some_and(|s| (1.0..=4.0).contains(&s))
        });
        if !keys_in(r, &["denoise", "deblur", "upscale", "chunkFrames"])
            || !r.get("denoise").is_none_or(quality)
            || !r.get("deblur").is_none_or(quality)
            || !upscale_ok
            || !r
                .get("chunkFrames")
                .is_none_or(|n| n.as_u64().is_some_and(|n| (1..=1024).contains(&n)))
        {
            return Err("Invalid videoPost.rtx: denoise/deblur quality Low|Medium|High|Ultra, upscale {mode, quality, scale 1..4}, chunkFrames 1..1024".into());
        }
    }
    if let Some(f) = p.get("frameGeneration") {
        let ok = match f["engine"].as_str() {
            Some("rtx") => {
                keys_in(f, &["engine", "multiplier", "mode"])
                    && multiplier_ok(&f["multiplier"])
                    && f["mode"]
                        .as_str()
                        .is_some_and(|m| FRAME_GEN_MODES.contains(&m))
            }
            Some("rife") => {
                keys_in(f, &["engine", "multiplier", "model"])
                    && multiplier_ok(&f["multiplier"])
                    && f["model"].as_str().is_some_and(comfy_model_name)
            }
            _ => false,
        };
        if !ok {
            return Err("Invalid videoPost.frameGeneration: {engine rtx, multiplier 2..16, mode LOW|MEDIUM|HIGH} or {engine rife, multiplier 2..16, model}".into());
        }
    }
    Ok(())
}

/// Output frame-rate multiplier of the chain (1 without frame generation).
pub(crate) fn multiplier(p: &Value) -> u64 {
    p["frameGeneration"]["multiplier"].as_u64().unwrap_or(1)
}

/// Emits the chain on `images` and returns the processed images link.
pub(crate) fn chain(g: &mut Value, mut images: Value, p: &Value) -> Value {
    if let Some(model) = p.get("upscaleModel") {
        g["PU"] = json!({"class_type":"UpscaleModelLoader","inputs":{"model_name":model}});
        g["PM"] = json!({"class_type":"ImageUpscaleWithModel","inputs":{"upscale_model":["PU",0],"image":images}});
        images = json!(["PM", 0]);
    }
    if let Some(r) = p.get("rtx") {
        let u = &r["upscale"];
        // Required inputs only; resize_type Scale keeps the owner's node defaults for the unused resize controls.
        let mut inputs = json!({
            "images":images,
            "denoise":r.get("denoise").is_some(),"denoise_quality":r.get("denoise").cloned().unwrap_or(json!("Ultra")),
            "deblur":r.get("deblur").is_some(),"deblur_quality":r.get("deblur").cloned().unwrap_or(json!("Ultra")),
            "upscale":u.get("mode").cloned().unwrap_or(json!("Off")),"upscale_quality":u.get("quality").cloned().unwrap_or(json!("Ultra")),
            "resize_type":"Scale","scale":u.get("scale").cloned().unwrap_or(json!(1.0)),
            "megapixels":2.0,"width":1920,"height":1080,"divisible_by":"8","ratio_preset":"16:9","resize_method":"Center Crop (Fill)","device_id":0
        });
        // Node optional inputs, only when selected: the complete IMAGE batch is still allocated up front,
        // so these bound SDK work / move the batch to disk, they never shrink the required storage.
        if p["useMmap"] == true {
            inputs["use_mmap"] = json!(true);
        }
        if let Some(frames) = r["chunkFrames"].as_u64() {
            inputs["chunking"] = json!(true);
            inputs["chunk_frames"] = json!(frames);
        }
        g["PR"] = json!({"class_type":"DaSiWa_RTX_UpscalerRefiner","inputs":inputs});
        images = json!(["PR", 0]);
    }
    if let Some(f) = p.get("frameGeneration") {
        if f["engine"] == "rife" {
            g["PFL"] = json!({"class_type":"FrameInterpolationModelLoader","inputs":{"model_name":f["model"]}});
            g["PF"] = json!({"class_type":"FrameInterpolate","inputs":{"interp_model":["PFL",0],"images":images,"multiplier":f["multiplier"]}});
        } else {
            let mut inputs = json!({
                "images":images,"generation_type":"frame rate multiplier","multiplier":f["multiplier"],"timestep":0.5,"mode":f["mode"],
                "automatic_shot_change_detection":true,"shot_change":false,"device_id":0
            });
            if p["useMmap"] == true {
                inputs["use_mmap"] = json!(true);
            }
            g["PF"] = json!({"class_type":"DaSiWa_RTX_FrameGeneration","inputs":inputs});
        }
        images = json!(["PF", 0]);
    }
    images
}

pub(crate) fn validate_enhance(i: &Value) -> Result<(), String> {
    let object = i.as_object().ok_or("Expected a video intent object")?;
    if object
        .keys()
        .any(|k| !(ENHANCE_FIELDS.contains(&k.as_str()) || ENHANCE_OPTIONAL.contains(&k.as_str())))
        || ENHANCE_FIELDS.iter().any(|k| !object.contains_key(*k))
    {
        return Err("Missing or unknown video-enhance intent fields".into());
    }
    if i["version"].as_f64() != Some(1.0) {
        return Err("Unsupported video recipe version".into());
    }
    // core LoadVideo lists top-level input files only.
    if !i["sourceVideo"]
        .as_str()
        .is_some_and(|s| model_name(s) && !s.contains('/'))
    {
        return Err("sourceVideo: a top-level input video".into());
    }
    if !i["frameRate"]
        .as_f64()
        .is_some_and(|f| f.is_finite() && (1.0..=120.0).contains(&f))
    {
        return Err("Invalid frameRate".into());
    }
    if !i["seed"]
        .as_u64()
        .is_some_and(|s| s <= 9_007_199_254_740_991)
    {
        return Err("Invalid seed".into());
    }
    if i.get("output").is_some_and(|o| !valid_output(o)) {
        return Err("Invalid output settings".into());
    }
    if i.get("outputPrefix")
        .is_some_and(|p| !p.as_str().is_some_and(safe_output_prefix))
    {
        return Err("Unsafe output prefix".into());
    }
    validate(&i["videoPost"])
}

/// LoadVideo → GetVideoComponents → chain → combine "7"; the imported source stays the baseline.
pub(crate) fn compile_enhance(i: &Value) -> Result<Value, String> {
    validate_enhance(i)?;
    let mut g = json!({
        "SV":{"class_type":"LoadVideo","inputs":{"file":i["sourceVideo"]}},
        "SVC":{"class_type":"GetVideoComponents","inputs":{"video":["SV",0]}},
    });
    let images = chain(&mut g, json!(["SVC", 0]), &i["videoPost"]);
    let fps = i["frameRate"].as_f64().unwrap() * multiplier(&i["videoPost"]) as f64;
    combine(&mut g, images, Some(json!(["SVC", 1])), json!(fps), i);
    Ok(g)
}

/// Model names prepare() must find in object_info for the chain (both recipes); only used fields,
/// so older prerequisite fixtures without the new list stay valid.
pub(crate) fn requirements(p: &Value) -> Vec<(&'static str, Vec<&str>)> {
    let fields = [
        (
            "upscaleModels",
            p["upscaleModel"].as_str().into_iter().collect(),
        ),
        (
            "frameInterpolationModels",
            (p["frameGeneration"]["engine"] == "rife")
                .then(|| p["frameGeneration"]["model"].as_str())
                .flatten()
                .into_iter()
                .collect(),
        ),
    ];
    fields
        .into_iter()
        .filter(|(_, names): &(&str, Vec<&str>)| !names.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post() -> Value {
        json!({"upscaleModel":"2x-AnimeSharpV4_RCAN.safetensors",
               "rtx":{"denoise":"High","upscale":{"mode":"VSR","quality":"Ultra","scale":1.5}},
               "frameGeneration":{"engine":"rife","multiplier":2,"model":"rife_v4.26.safetensors"}})
    }

    #[test]
    fn chain_follows_the_owners_post_order_and_multiplies_the_frame_rate() {
        let mut g = json!({});
        let out = chain(&mut g, json!(["6", 0]), &post());
        assert_eq!(out, json!(["PF", 0]));
        assert_eq!(g["PM"]["inputs"]["image"], json!(["6", 0]));
        assert_eq!(g["PR"]["inputs"]["images"], json!(["PM", 0]));
        assert_eq!(g["PR"]["inputs"]["denoise"], true);
        assert_eq!(g["PR"]["inputs"]["deblur"], false);
        assert_eq!(g["PR"]["inputs"]["upscale"], "VSR");
        assert_eq!(g["PR"]["inputs"]["scale"], 1.5);
        assert_eq!(g["PF"]["inputs"]["images"], json!(["PR", 0]));
        assert_eq!(g["PF"]["class_type"], "FrameInterpolate");
        assert_eq!(g["PFL"]["inputs"]["model_name"], "rife_v4.26.safetensors");
        assert_eq!(multiplier(&post()), 2);
        let mut rtx = post();
        rtx["frameGeneration"] = json!({"engine":"rtx","multiplier":3,"mode":"HIGH"});
        rtx.as_object_mut().unwrap().remove("rtx");
        let mut g = json!({});
        assert_eq!(chain(&mut g, json!(["6", 0]), &rtx), json!(["PF", 0]));
        assert_eq!(g["PF"]["class_type"], "DaSiWa_RTX_FrameGeneration");
        assert_eq!(g["PF"]["inputs"]["images"], json!(["PM", 0]));
        assert!(g.get("PR").is_none());
        assert_eq!(multiplier(&json!({"upscaleModel":"x.pth"})), 1);
    }

    #[test]
    fn rtx_refine_without_upscale_turns_the_stage_off() {
        let mut g = json!({});
        chain(&mut g, json!(["6", 0]), &json!({"rtx":{"deblur":"Low"}}));
        assert_eq!(g["PR"]["inputs"]["upscale"], "Off");
        assert_eq!(g["PR"]["inputs"]["deblur"], true);
        assert_eq!(g["PR"]["inputs"]["deblur_quality"], "Low");
        assert_eq!(g["PR"]["inputs"]["denoise"], false);
    }

    #[test]
    fn rtx_memory_options_reach_the_nodes_only_when_selected() {
        let mut g = json!({});
        chain(&mut g, json!(["6", 0]), &json!({"rtx":{"denoise":"Low"}}));
        assert!(g["PR"]["inputs"].get("use_mmap").is_none());
        assert!(g["PR"]["inputs"].get("chunk_frames").is_none());
        let p = json!({"useMmap":true,
            "rtx":{"denoise":"Low","chunkFrames":8},
            "frameGeneration":{"engine":"rtx","multiplier":2,"mode":"HIGH"}});
        let mut g = json!({});
        chain(&mut g, json!(["6", 0]), &p);
        assert_eq!(g["PR"]["inputs"]["use_mmap"], true);
        assert_eq!(g["PR"]["inputs"]["chunking"], true);
        assert_eq!(g["PR"]["inputs"]["chunk_frames"], 8);
        assert_eq!(g["PF"]["inputs"]["use_mmap"], true);
        assert!(validate(&p).is_ok());
        for bad in [
            json!({"rtx":{"denoise":"Low","chunkFrames":0}}),
            json!({"rtx":{"denoise":"Low","chunkFrames":1025}}),
            json!({"useMmap":"yes","rtx":{"denoise":"Low"}}),
        ] {
            assert!(validate(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn validation_rejects_empty_unknown_and_out_of_range_stages() {
        assert!(validate(&post()).is_ok());
        for bad in [
            json!({}),
            json!({"extra":1}),
            json!({"upscaleModel":"../x.pth"}),
            json!({"rtx":{}}),
            json!({"rtx":{"denoise":"Best"}}),
            json!({"rtx":{"upscale":{"mode":"VSR","quality":"Ultra","scale":4.5}}}),
            json!({"rtx":{"upscale":{"mode":"Bilinear","quality":"Ultra","scale":2}}}),
            json!({"frameGeneration":{"engine":"rtx","multiplier":1,"mode":"HIGH"}}),
            json!({"frameGeneration":{"engine":"rtx","multiplier":2,"mode":"BEST"}}),
            json!({"frameGeneration":{"engine":"rife","multiplier":2}}),
            json!({"frameGeneration":{"engine":"rife","multiplier":17,"model":"rife.safetensors"}}),
            json!({"frameGeneration":{"engine":"other","multiplier":2}}),
        ] {
            assert!(validate(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn enhance_loads_the_source_and_keeps_its_audio() {
        let i = json!({"recipeId":"video-enhance","version":1,"sourceVideo":"deas-input-1.mp4","frameRate":24,"seed":7,
            "videoPost":post(),"output":{"codec":"AV1","container":"MKV","bitDepth":"10-bit","quality":18,"audioCodec":"Opus","audioBitrate":"128k","saveFirstFrame":false,"saveLastFrame":false}});
        let g = compile_enhance(&i).unwrap();
        assert_eq!(g["SV"]["inputs"]["file"], "deas-input-1.mp4");
        assert_eq!(g["PM"]["inputs"]["image"], json!(["SVC", 0]));
        assert_eq!(g["7"]["inputs"]["images"], json!(["PF", 0]));
        assert_eq!(g["7"]["inputs"]["audio"], json!(["SVC", 1]));
        assert_eq!(g["7"]["inputs"]["frame_rate"], 48.0);
        assert_eq!(g["7"]["inputs"]["codec"], "AV1");
        assert_eq!(g["7"]["inputs"]["filename_prefix"], "raw/tve/DEAS");
        assert!(g.get("B7").is_none());
        let names: Vec<_> = requirements(&i["videoPost"]);
        assert!(names.contains(&("upscaleModels", vec!["2x-AnimeSharpV4_RCAN.safetensors"])));
        assert!(names.contains(&("frameInterpolationModels", vec!["rife_v4.26.safetensors"])));
        for (key, value) in [
            ("sourceVideo", json!("sub/clip.mp4")),
            ("frameRate", json!(0)),
            ("videoPost", json!({})),
            ("extra", json!(1)),
        ] {
            let mut bad = i.clone();
            bad[key] = value;
            assert!(compile_enhance(&bad).is_err(), "{key}");
        }
    }
}
