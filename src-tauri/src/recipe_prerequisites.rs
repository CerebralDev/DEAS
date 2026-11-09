//! Read-only core object_info diagnosis, not model-load or generation proof.

use serde_json::{json, Value};
use std::time::Duration;

const REQUIRED_NODES: [&str; 6] = [
    "CheckpointLoaderSimple",
    "CLIPTextEncode",
    "EmptyLatentImage",
    "KSampler",
    "VAEDecode",
    "SaveImage",
];
/// Image-to-image (`LoadImage`, `VAEEncode`), split diffusion-model loading and the DaSiWa
/// Civitai-metadata saver; their absence blocks only the recipe variant that needs them.
const OPTIONAL_NODES: [&str; 37] = [
    "DEAS_ImagePostprocessGate",
    "DEAS_VideoPostprocessGate",
    "TextEncodeQwenImage21",
    "DEAS_ImagePromptEncode",
    "EmptyFlux2LatentImage",
    "SplitSigmas",
    "EmptySD3LatentImage",
    "ModelSamplingAuraFlow",
    "ConditioningZeroOut",
    "LoraLoader",
    "CLIPVisionLoader",
    "CLIPVisionEncode",
    "unCLIPConditioning",
    crate::wildcard::NODE,
    "ImageScale",
    "ImagePadForOutpaint",
    "ImageCrop",
    "CropMask",
    "MaskToImage",
    "ImageToMask",
    "ImageBlur",
    // Inpaint mask and image layers.
    "LoadImageMask",
    "SetLatentNoiseMask",
    "ImageCompositeMasked",
    "LoadImage",
    "VAEEncode",
    // Model upscale (image-to-image source and refine pass).
    "UpscaleModelLoader",
    "ImageUpscaleWithModel",
    "UNETLoader",
    "UnetLoaderGGUF",
    "CLIPLoaderGGUF",
    "DualCLIPLoaderGGUF",
    "CLIPLoader",
    "VAELoader",
    "DaSiWa_MetadataImageSaver",
    "DaSiWa_MetadataConfig",
    "TextEncodeMageFlowEdit",
];
/// Model-fixed sigma schedules (Qwen Image 2.1 Turbo, Flux.2): `ManualSigmas`/`Flux2Scheduler` → `SamplerCustomAdvanced` instead of `KSampler`.
pub(crate) const SIGMA_NODES: [&str; 6] = [
    "RandomNoise",
    "CFGGuider",
    "KSamplerSelect",
    "ManualSigmas",
    "Flux2Scheduler",
    "SamplerCustomAdvanced",
];
/// Workbench Options patches (attention, step caches, compile, tiled VAE);
/// each one is offered only when the backend lists it.
pub(crate) const OPTION_NODES: [&str; 9] = [
    "ModelAttentionBackend",
    "DaSiWaTiledDiffusion",
    "PathchSageAttentionKJ",
    "EasyCache",
    "LazyCache",
    "TorchCompileModel",
    "VAEDecodeTiled",
    "VAEEncodeTiled",
    "SpectrumApplyMiniMaxH3",
];
/// File-name combos; names outside the compiler's grammar (traversal, URLs, control characters) are dropped, not fatal.
const FILE_FIELDS: [&str; 8] = [
    "ckpt_name",
    "lora_name",
    "image",
    "unet_name",
    "clip_name",
    "clip_name1",
    "clip_name2",
    "vae_name",
];

fn combo(nodes: &Value, node: &str, field: &str) -> Result<Vec<String>, String> {
    if nodes.get(node).is_none() {
        return Ok(Vec::new());
    }
    let schema = nodes[node]["input"]["required"][field]
        .as_array()
        .filter(|schema| {
            (1..=2).contains(&schema.len()) && schema.get(1).is_none_or(Value::is_object)
        })
        .ok_or_else(|| format!("object_info {node}.{field} has malformed combo schema"))?;
    let choices = schema[0]
        .as_array()
        .ok_or_else(|| format!("object_info {node}.{field} must be a combo array"))?;
    let mut seen = std::collections::HashSet::new();
    choices
        .iter()
        .map(|value| {
            let identifier = value.as_str().ok_or_else(|| {
                format!("object_info {node}.{field} contains a non-string choice")
            })?;
            // Files use the compiler's model-name grammar; enum choices stay short safe ASCII.
            let file = FILE_FIELDS.contains(&field);
            let safe = if file {
                if field == "image" {
                    crate::jobs::model_name(identifier)
                } else {
                    crate::jobs::comfy_model_name(identifier)
                }
            } else {
                !identifier.is_empty()
                    && identifier.len() <= 255
                    && identifier
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
            };

            if file && (!safe || seen.contains(identifier)) {
                return Ok(None);
            }
            if !safe || !seen.insert(identifier) {
                return Err(format!(
                    "object_info {node}.{field} contains an unsafe or duplicate choice"
                ));
            }
            Ok(Some(identifier.to_owned()))
        })
        .filter_map(Result::transpose)
        .collect()
}

/// Combo in either object_info form (v1 `[[...], {}]`, v3 `["COMBO", {"options": [...]}]`).
/// Values `keep` rejects are dropped: an odd custom-node choice must not block generation.
fn options(nodes: &Value, node: &str, field: &str, keep: fn(&str) -> bool) -> Vec<String> {
    let schema = &nodes[node]["input"]["required"][field];
    let list = if schema[0] == "COMBO" {
        &schema[1]["options"]
    } else {
        &schema[0]
    };
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|s| keep(s))
        .map(str::to_owned)
        .collect()
}

fn option(s: &str) -> bool {
    crate::jobs::safe_option(s) && s != "disabled"
}

// Advertise the gate only with the effective saver's post-save socket and our lazy ABI.
fn gate_abi(gate: &Value) -> bool {
    gate["output"] == json!(["IMAGE", "AUDIO"])
        && gate["input"]["required"]
            == json!({"baseline":["IMAGE,STRING"],"image":["IMAGE",{"lazy":true}]})
        && gate["input"]["optional"] == json!({"audio":["AUDIO",{"lazy":true}]})
}

// Seamless loop only from node packs whose loop also maps the audio (DaSiWa nodes 0.5.41+).
fn seamless_loop_ready(nodes: &Value) -> bool {
    let node = &nodes["DaSiWa_SeamlessLoop"];
    let input = |f: &str| {
        node["input"]["required"]
            .get(f)
            .unwrap_or(&node["input"]["optional"][f])
    };
    let modes = input("audio_loop");
    let modes = if modes[0] == "COMBO" {
        &modes[1]["options"]
    } else {
        &modes[0]
    };
    node["output"][1] == "AUDIO"
        && input("audio")[0] == "AUDIO"
        && input("frame_rate")[0] == "FLOAT"
        && crate::jobs_video_post::LOOP_AUDIO
            .iter()
            .all(|(_, m)| modes.as_array().is_some_and(|a| a.contains(&json!(m))))
}

fn image_postprocess_ready(nodes: &Value) -> bool {
    let metadata = nodes.get("DaSiWa_MetadataImageSaver").is_some()
        && nodes.get("DaSiWa_MetadataConfig").is_some();
    let (saver, output) = if metadata {
        ("DaSiWa_MetadataImageSaver", "STRING")
    } else {
        ("SaveImage", "IMAGE")
    };
    gate_abi(&nodes["DEAS_ImagePostprocessGate"]) && nodes[saver]["output"][0] == output
}

// The video baseline saver B7 feeds the gate through its `filename` STRING socket.
fn video_postprocess_ready(nodes: &Value) -> bool {
    gate_abi(&nodes["DEAS_VideoPostprocessGate"])
        && nodes["DaSiWa_EnhancedVideoCombine"]["output"][1] == "STRING"
}

fn preview_input<'a>(nodes: &'a Value, field: &str) -> &'a Value {
    let input = &nodes["ModelPreviewOverrideKJ"]["input"];
    input["required"]
        .get(field)
        .unwrap_or(&input["optional"][field])
}

fn preview_choices(nodes: &Value) -> Vec<String> {
    let schema = preview_input(nodes, "tiny_vae");
    let choices = if schema[0] == "COMBO" {
        &schema[1]["options"]
    } else {
        &schema[0]
    };
    choices
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|name| *name == "none" || crate::jobs::comfy_model_name(name))
        .map(str::to_owned)
        .collect()
}

// An older/same-named wrapper must not advertise support for fields it cannot execute.
fn video_preview_ready(nodes: &Value) -> bool {
    let node = &nodes["ModelPreviewOverrideKJ"];
    if node["output"] != json!(["MODEL"])
        || preview_input(nodes, "model")[0] != "MODEL"
        || preview_input(nodes, "suppress_default_preview")[0] != "BOOLEAN"
        || preview_input(nodes, "vae")[0] != "VAE"
        || node["input"]["required"].get("vae").is_some()
        || !preview_choices(nodes).iter().any(|c| c == "none")
    {
        return false;
    }
    let required = [
        "model",
        "max_resolution",
        "jpeg_quality",
        "suppress_default_preview",
        "preview_frames",
        "preview_fps",
        "tiny_vae",
        "vae",
    ];
    if node["input"]["required"]
        .as_object()
        .is_none_or(|r| r.keys().any(|k| !required.contains(&k.as_str())))
    {
        return false;
    }
    [
        ("max_resolution", 512),
        ("jpeg_quality", 80),
        ("preview_frames", 120),
        ("preview_fps", 8),
    ]
    .iter()
    .all(|(field, value)| {
        let schema = preview_input(nodes, field);
        schema[0] == "INT"
            && schema[1]["min"].as_i64().is_some_and(|min| min <= *value)
            && schema[1]["max"].as_i64().is_some_and(|max| max >= *value)
    })
}

fn video_previews(nodes: &Value) -> Value {
    let mut previews = json!({});
    if !video_preview_ready(nodes) {
        return previews;
    }
    let tiny = preview_choices(nodes);
    let vaes = options(
        nodes,
        "VAELoader",
        "vae_name",
        crate::jobs::comfy_model_name,
    );
    for family in crate::jobs_video::FAMILIES {
        for (decoder, models) in [("tiny", &tiny), ("vae", &vaes)] {
            if let Some(model) = models
                .iter()
                .find(|model| crate::jobs_video::preview_model_compatible(family, decoder, model))
            {
                previews[family] = json!({"decoder":decoder,"model":model});
                break;
            }
        }
    }
    previews
}

fn parse(base: &reqwest::Url, nodes: &Value) -> Result<Value, String> {
    let definitions = nodes
        .as_object()
        .ok_or("object_info must be a JSON object")?;
    if definitions.values().any(|node| !node.is_object()) {
        return Err("object_info contains malformed node definitions".into());
    }
    Ok(json!({
        "baseUrl":base.as_str(), "recipeId":"checkpoint-text-to-image-diagnostic", "version":1,
        "allowlist":{
            "nodes":REQUIRED_NODES.iter().chain(OPTIONAL_NODES.iter()).chain(SIGMA_NODES.iter()).chain(OPTION_NODES.iter()).chain(crate::jobs_sam3::NODES.iter()).chain(crate::jobs::H3_EDIT_NODES.iter()).chain(crate::jobs_video::VIDEO_NODES.iter()).chain(crate::jobs_video::H3_CONTINUITY_NODES.iter()).chain(crate::jobs_video_post::POST_NODES.iter()).chain(crate::jobs_music::MUSIC_NODES.iter()).chain(crate::jobs_music::ace::NODES.iter()).filter(|name| definitions.contains_key(**name) && (**name != "DEAS_ImagePostprocessGate" || image_postprocess_ready(nodes)) && (**name != "DEAS_VideoPostprocessGate" || video_postprocess_ready(nodes)) && (**name != "ModelPreviewOverrideKJ" || video_preview_ready(nodes)) && (**name != "DaSiWa_SeamlessLoop" || seamless_loop_ready(nodes))).collect::<Vec<_>>(),
            "checkpoints":combo(nodes, "CheckpointLoaderSimple", "ckpt_name")?,
            "loras":combo(nodes, "LoraLoader", "lora_name")?,
            "images":combo(nodes, "LoadImage", "image")?,
            "samplers":combo(nodes, "KSampler", "sampler_name")?,
            "schedulers":combo(nodes, "KSampler", "scheduler")?,
            "diffusionModels":combo(nodes, "UNETLoader", "unet_name")?,
            "ggufDiffusionModels":combo(nodes, "UnetLoaderGGUF", "unet_name")?,
            "textEncoders":combo(nodes, "CLIPLoader", "clip_name")?,
            "dualTextEncoders":combo(nodes, "DualCLIPLoader", "clip_name1")?,
            "dualClipTypes":combo(nodes, "DualCLIPLoader", "type")?,
            "ggufTextEncoders":combo(nodes, "CLIPLoaderGGUF", "clip_name")?,
            "ggufClipTypes":combo(nodes, "CLIPLoaderGGUF", "type")?,
            "ggufDualTextEncoders":combo(nodes, "DualCLIPLoaderGGUF", "clip_name1")?,
            "ggufDualClipTypes":combo(nodes, "DualCLIPLoaderGGUF", "type")?,
            "clipTypes":combo(nodes, "CLIPLoader", "type")?,
            "vaes":combo(nodes, "VAELoader", "vae_name")?,
            "videoPreviews":video_previews(nodes),
            "visionModels":combo(nodes, "CLIPVisionLoader", "clip_name")?,
            "attention":options(nodes, "ModelAttentionBackend", "attention", option),
            "sage":options(nodes, "PathchSageAttentionKJ", "sage_attention", option),
            "compile":options(nodes, "TorchCompileModel", "backend", option),
            // Core v3 node: its file combo uses the v3 form.
            "upscaleModels":options(nodes, "UpscaleModelLoader", "model_name", crate::jobs::comfy_model_name),
            "latentUpscaleModels":options(nodes, "LatentUpscaleModelLoader", "model_name", crate::jobs::comfy_model_name),
            "h3LatentUpscaleModels":options(nodes, "DaSiWaH3TiledUpscale", "upscale_model", crate::jobs_video::h3_latent_model),
            "frameInterpolationModels":options(nodes, "FrameInterpolationModelLoader", "model_name", crate::jobs::comfy_model_name),
            // TVE RTX upscale modes the installed node pack offers (streaming presets are SDK-dependent).
            "rtxUpscaleModes":options(nodes, "DaSiWa_RTX_UpscalerRefiner", "upscale", option),
            "audioEncoders":options(nodes, "AudioEncoderLoader", "audio_encoder_name", crate::jobs::comfy_model_name)
        }, "missingNodes":REQUIRED_NODES.iter().filter(|name| !definitions.contains_key(**name)).collect::<Vec<_>>(), "generationReady":false
    }))
}

/// Explicit core probe only; does not negotiate v2 or follow upstream metadata.
#[tauri::command]
pub async fn native_recipe_prerequisites(base_url: String) -> Result<Value, String> {
    let base = crate::validate_base_url(&base_url)?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .map_err(|error| format!("Could not create prerequisite client: {error}"))?;
    let nodes = crate::get_json(&client, &base, "object_info", false)
        .await?
        .ok_or("Missing object_info response")?;
    // A reachable backend whose object_info is rejected would otherwise keep Run disabled silently.
    parse(&base, &nodes).inspect_err(|error| {
        crate::warn_backend_limited("object_info:PREREQUISITES", || {
            format!(
                "WARN: backend prerequisites rejected: {error} (repeats limited to once per minute)"
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn video_preview_discovery_uses_only_compatible_backend_listed_decoders() {
        let mut info = nodes();
        info["ModelPreviewOverrideKJ"] = preview_schema();
        info["VAELoader"]["input"]["required"]["vae_name"] = json!([[
            "LTX2/taeltx2_3.safetensors",
            "Wan/lighttaew2_1.safetensors",
            "MiniMaxH3/taeh3.safetensors"
        ]]);
        let base = reqwest::Url::parse("http://127.0.0.1:8188/").unwrap();
        let allow = parse(&base, &info).unwrap()["allowlist"].clone();
        assert_eq!(
            allow["videoPreviews"],
            json!({
                "minimax-h3":{"decoder":"tiny","model":"H3/taeh3.safetensors"},
                "minimax-h3-hyperflow":{"decoder":"tiny","model":"H3/taeh3.safetensors"},
                "ltx-2":{"decoder":"vae","model":"LTX2/taeltx2_3.safetensors"},
                "wan":{"decoder":"tiny","model":"Wan/taew2_1.safetensors"}
            })
        );
        info["ModelPreviewOverrideKJ"]["input"]["optional"]["tiny_vae"] = json!(["COMBO",{"options":["none","taew2_2.safetensors","../taeh3.safetensors","unknown.safetensors"]}]);
        assert_eq!(
            video_previews(&info),
            json!({"ltx-2":{"decoder":"vae","model":"LTX2/taeltx2_3.safetensors"}})
        );
        info["VAELoader"]["input"]["required"]["vae_name"] = json!([[
            "taeltx.safetensors",
            "full-ltx-vae.safetensors",
            "taeltx2_3-copy.safetensors"
        ]]);
        assert_eq!(video_previews(&info), json!({}));
        info["ModelPreviewOverrideKJ"]["input"]["optional"]["tiny_vae"] =
            json!([["none", "Wan\\lighttaew2_1.safetensors"]]);
        assert_eq!(
            video_previews(&info)["wan"],
            json!({"decoder":"tiny","model":"Wan\\lighttaew2_1.safetensors"})
        );
    }

    #[test]
    fn video_preview_discovery_preserves_registered_nested_tiny_vae_identifiers() {
        let base = reqwest::Url::parse("http://127.0.0.1:8188/").unwrap();
        let mut info = nodes();
        info["ModelPreviewOverrideKJ"] = preview_schema();
        info["ModelPreviewOverrideKJ"]["input"]["optional"]["tiny_vae"] = json!(["COMBO", {"options":[
            "none", "Wan/taew2_2.safetensors", "MiniMaxH3/taeh3-copy.safetensors",
            "MiniMaxH3/taeh3.safetensors", "Wan/taew2_1.safetensors"
        ]}]);
        let previews = parse(&base, &info).unwrap()["allowlist"]["videoPreviews"].clone();
        for family in ["minimax-h3", "minimax-h3-hyperflow"] {
            assert_eq!(
                previews[family],
                json!({"decoder":"tiny","model":"MiniMaxH3/taeh3.safetensors"})
            );
        }
        assert_eq!(
            previews["wan"],
            json!({"decoder":"tiny","model":"Wan/taew2_1.safetensors"})
        );
        info.as_object_mut()
            .unwrap()
            .remove("ModelPreviewOverrideKJ");
        info["VAELoader"]["input"]["required"]["vae_name"] =
            json!([["MiniMaxH3/taeh3.safetensors", "Wan/taew2_1.safetensors"]]);
        assert_eq!(
            parse(&base, &info).unwrap()["allowlist"]["videoPreviews"],
            json!({})
        );
    }

    fn preview_schema() -> Value {
        json!({"input":{"required":{
            "model":["MODEL"],"max_resolution":["INT",{"min":0,"max":8192}],
            "jpeg_quality":["INT",{"min":30,"max":100}],"suppress_default_preview":["BOOLEAN"],
            "preview_frames":["INT",{"min":1,"max":1024}],"preview_fps":["INT",{"min":1,"max":60}]
        },"optional":{"vae":["VAE"],"tiny_vae":[["none","H3/taeh3.safetensors","Wan/taew2_1.safetensors","taew2_2.safetensors"]]}},"output":["MODEL"]})
    }

    #[test]
    fn video_preview_discovery_rejects_missing_or_incompatible_wrapper_without_blocking_probe() {
        let base = reqwest::Url::parse("http://127.0.0.1:8188/").unwrap();
        let mut info = nodes();
        assert_eq!(
            parse(&base, &info).unwrap()["allowlist"]["videoPreviews"],
            json!({})
        );
        for field in [
            "model",
            "preview_frames",
            "preview_fps",
            "suppress_default_preview",
        ] {
            info["ModelPreviewOverrideKJ"] = preview_schema();
            info["ModelPreviewOverrideKJ"]["input"]["required"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            let allow = parse(&base, &info).unwrap()["allowlist"].clone();
            assert_eq!(allow["videoPreviews"], json!({}));
            assert!(!allow["nodes"]
                .as_array()
                .unwrap()
                .contains(&json!("ModelPreviewOverrideKJ")));
        }
        info["ModelPreviewOverrideKJ"] = preview_schema();
        info["ModelPreviewOverrideKJ"]["input"]["required"]["preview_frames"][1]["max"] =
            json!(119);
        assert_eq!(video_previews(&info), json!({}));
        info["ModelPreviewOverrideKJ"] = preview_schema();
        info["ModelPreviewOverrideKJ"]["input"]["required"]["unknown"] = json!(["STRING"]);
        assert_eq!(video_previews(&info), json!({}));
    }

    #[test]
    fn image_postprocess_gate_requires_lazy_abi_and_effective_post_save_socket() {
        let base = reqwest::Url::parse("http://127.0.0.1:8188/").unwrap();
        let mut info = nodes();
        let gate = json!({"input":{"required":{"baseline":["IMAGE,STRING"],"image":["IMAGE",{"lazy":true}]},"optional":{"audio":["AUDIO",{"lazy":true}]}},"output":["IMAGE","AUDIO"]});
        info["DEAS_ImagePostprocessGate"] = gate.clone();
        let listed = |info: &Value| {
            parse(&base, info).unwrap()["allowlist"]["nodes"]
                .as_array()
                .unwrap()
                .contains(&json!("DEAS_ImagePostprocessGate"))
        };
        assert!(!listed(&info)); // Legacy SaveImage has no post-save output.
        info["SaveImage"]["output"] = json!(["IMAGE"]);
        assert!(listed(&info));
        for (field, value) in [
            ("image", json!(["IMAGE"])),
            ("image", json!(["IMAGE",{"lazy":false}])),
            ("baseline", json!(["IMAGE,STRING",{"lazy":true}])),
            ("baseline", json!(["IMAGE"])),
            ("extra", json!(["STRING"])),
        ] {
            info["DEAS_ImagePostprocessGate"]["input"]["required"][field] = value;
            assert!(!listed(&info));
            info["DEAS_ImagePostprocessGate"] = gate.clone();
        }
        info["DEAS_ImagePostprocessGate"]["output"] = json!(["STRING"]);
        assert!(!listed(&info));
        info["DEAS_ImagePostprocessGate"]["output"] = json!(["IMAGE"]);
        assert!(!listed(&info)); // Pre-audio adapter: video loops could not pass the gate.
        info["DEAS_ImagePostprocessGate"] = gate.clone();
        info["DEAS_ImagePostprocessGate"]["input"]["optional"]["audio"] = json!(["AUDIO"]);
        assert!(!listed(&info));
        info["DEAS_ImagePostprocessGate"] = gate;
        info["DaSiWa_MetadataImageSaver"] = json!({"output":[]});
        assert!(listed(&info)); // Incomplete metadata pair still selects core.
        info["DaSiWa_MetadataConfig"] = json!({});
        assert!(!listed(&info)); // Selection must not invalidate the baseline proof.
        info["DaSiWa_MetadataImageSaver"]["output"] = json!(["STRING"]);
        assert!(listed(&info));
        info["SaveImage"]["output"] = json!([]);
        assert!(listed(&info)); // Effective metadata saver needs no core socket.
        info.as_object_mut()
            .unwrap()
            .remove("DEAS_ImagePostprocessGate");
        assert!(!listed(&info));
        assert!(parse(&base, &info).is_ok()); // Plain generation stays available.
    }

    #[test]
    fn video_postprocess_gate_requires_lazy_abi_and_video_saver_filename_socket() {
        let base = reqwest::Url::parse("http://127.0.0.1:8188/").unwrap();
        let mut info = nodes();
        let gate = json!({"input":{"required":{"baseline":["IMAGE,STRING"],"image":["IMAGE",{"lazy":true}]},"optional":{"audio":["AUDIO",{"lazy":true}]}},"output":["IMAGE","AUDIO"]});
        info["DEAS_VideoPostprocessGate"] = gate.clone();
        let listed = |info: &Value| {
            parse(&base, info).unwrap()["allowlist"]["nodes"]
                .as_array()
                .unwrap()
                .contains(&json!("DEAS_VideoPostprocessGate"))
        };
        assert!(!listed(&info)); // No video saver.
        info["DaSiWa_EnhancedVideoCombine"] = json!({"output":["IMAGE"]});
        assert!(!listed(&info)); // Saver without the post-save filename.
        info["DaSiWa_EnhancedVideoCombine"]["output"] = json!(["IMAGE", "STRING"]);
        assert!(listed(&info));
        info["DEAS_VideoPostprocessGate"]["input"]["required"]["image"] = json!(["IMAGE"]);
        assert!(!listed(&info)); // Lazy ABI is mandatory.
        info["DEAS_VideoPostprocessGate"] = gate.clone();
        info["DEAS_VideoPostprocessGate"]["input"]
            .as_object_mut()
            .unwrap()
            .remove("optional");
        assert!(!listed(&info)); // Audio lazy input is part of the ABI.
        info["DEAS_VideoPostprocessGate"] = gate;
        info["DEAS_VideoPostprocessGate"]["output"] = json!(["STRING"]);
        assert!(!listed(&info));
    }

    #[test]
    fn seamless_loop_is_listed_only_with_its_audio_inputs() {
        let base = reqwest::Url::parse("http://127.0.0.1:8188/").unwrap();
        let mut info = nodes();
        let listed = |info: &Value| {
            parse(&base, info).unwrap()["allowlist"]["nodes"]
                .as_array()
                .unwrap()
                .contains(&json!("DaSiWa_SeamlessLoop"))
        };
        // Installed 0.5.40 pack: IMAGE in/out only.
        info["DaSiWa_SeamlessLoop"] =
            json!({"input":{"required":{"images":["IMAGE"]}},"output":["IMAGE"]});
        assert!(!listed(&info));
        let new = json!({"input":{"required":{"images":["IMAGE"]},"optional":{
            "model_name":[["rife_v4.26.safetensors"]],"exact_endpoint":["BOOLEAN",{"default":false}],
            "interpolation_backend":[["RIFE / FILM","RTX Frame Generation"]],
            "audio":["AUDIO"],"frame_rate":["FLOAT",{"default":0.0}],
            "audio_loop":[["Crossfade","Beat-aware"],{"default":"Crossfade"}]}},"output":["IMAGE","AUDIO"]});
        info["DaSiWa_SeamlessLoop"] = new.clone();
        assert!(listed(&info));
        info["DaSiWa_SeamlessLoop"]["input"]["optional"]["audio_loop"] =
            json!(["COMBO",{"options":["Crossfade","Beat-aware"]}]);
        assert!(listed(&info)); // V3 combo form.
        for (field, value) in [
            ("audio_loop", json!([["Crossfade"]])),
            ("frame_rate", json!(["INT"])),
            ("audio", json!(["IMAGE"])),
        ] {
            info["DaSiWa_SeamlessLoop"] = new.clone();
            info["DaSiWa_SeamlessLoop"]["input"]["optional"][field] = value;
            assert!(!listed(&info), "{field}");
        }
        info["DaSiWa_SeamlessLoop"] = new;
        info["DaSiWa_SeamlessLoop"]["output"] = json!(["IMAGE"]);
        assert!(!listed(&info));
    }

    #[test]
    fn latent_post_upscale_catalogs_use_own_safe_model_choices() {
        let base = reqwest::Url::parse("http://127.0.0.1:8188/").unwrap();
        let mut info = nodes();
        info["DaSiWaH3TiledUpscale"] = json!({"input":{"required":{"upscale_model":[[
            "interpolation", "minimax_h3_latent_upscaler_3d_conv_v1_bf16.safetensors",
            "H3/minimax_h3_latent_upscaler_3d_conv_v1_fp16.sft",
            r"H3\minimax_h3_latent_upscaler_3d_conv_v1_bf16.safetensors",
            "../minimax_h3_latent_upscaler.safetensors", "other.safetensors",
            "minimax_h3_latent_upscaler.pt", "ltx-spatial-upscaler.safetensors"
        ]]}}});
        // Retain the legacy multipass core catalog; only new post-upscale selection is family-strict.
        info["LatentUpscaleModelLoader"] = json!({"input":{"required":{"model_name":["COMBO",{"options":["old.safetensors","ltx-2.3-spatial-upscaler-x2-1.1.safetensors"]}]}}});
        let result = parse(&base, &info).unwrap();
        assert_eq!(
            result["allowlist"]["h3LatentUpscaleModels"],
            json!([
                "minimax_h3_latent_upscaler_3d_conv_v1_bf16.safetensors",
                "H3/minimax_h3_latent_upscaler_3d_conv_v1_fp16.sft",
                r"H3\minimax_h3_latent_upscaler_3d_conv_v1_bf16.safetensors"
            ])
        );
        assert_eq!(
            result["allowlist"]["latentUpscaleModels"],
            json!([
                "old.safetensors",
                "ltx-2.3-spatial-upscaler-x2-1.1.safetensors"
            ])
        );
        assert!(result["allowlist"]["nodes"]
            .as_array()
            .unwrap()
            .contains(&json!("DaSiWaH3TiledUpscale")));
    }

    fn nodes() -> Value {
        json!({
            "CheckpointLoaderSimple": {"input":{"required":{"ckpt_name":[["sd/model.safetensors"], {}]}}},
            "CLIPTextEncode": {}, "EmptyLatentImage": {},
            "KSampler": {"input":{"required":{"sampler_name":[["euler", "dpmpp_2m"]],"scheduler":[["normal", "karras"]]}}},
            "VAEDecode": {}, "SaveImage": {}, "UnrelatedNode": {},
            "LoraLoader": {"input":{"required":{"lora_name":[["style/detail.safetensors"], {}]}}},
            "CLIPVisionLoader": {"input":{"required":{"clip_name":[["clip-vit.safetensors"], {}]}}},
            "CLIPVisionEncode": {}, "unCLIPConditioning": {}
        })
    }

    #[test]
    fn music_nodes_and_audio_encoders_are_optional_and_scoped() {
        let base = reqwest::Url::parse("http://127.0.0.1:8188/").unwrap();
        let mut info = nodes();
        let without = parse(&base, &info).unwrap();
        assert_eq!(without["allowlist"]["audioEncoders"], json!([]));
        assert!(!without["allowlist"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n == "YuE2GenerateMusic"));
        assert_eq!(without["missingNodes"], json!([]));
        for n in crate::jobs_music::MUSIC_NODES {
            if info.get(n).is_none() {
                info[n] = json!({});
            }
        }
        // Core V3 combo shape as captured from the installed backend; unsafe names are dropped silently.
        info["AudioEncoderLoader"] = json!({"input":{"required":{"audio_encoder_name":["COMBO",{"multiselect":false,"options":["sheetsage2_bf16.safetensors","sub/other.safetensors","../escape.safetensors"]}]}}});
        let with = parse(&base, &info).unwrap();
        assert_eq!(
            with["allowlist"]["audioEncoders"],
            json!(["sheetsage2_bf16.safetensors", "sub/other.safetensors"])
        );
        let listed = with["allowlist"]["nodes"].as_array().unwrap();
        assert!([
            "YuE2GenerateMusic",
            "SheetSage2AudioToABC",
            "SaveAudioAdvanced"
        ]
        .iter()
        .all(|n| listed.iter().any(|l| l == n)));
        assert_eq!(with["missingNodes"], without["missingNodes"]);
        info["AudioEncoderLoader"] =
            json!({"input":{"required":{"audio_encoder_name":["COMBO",{"options":"broken"}]}}});
        assert_eq!(
            parse(&base, &info).unwrap()["allowlist"]["audioEncoders"],
            json!([])
        );
    }

    #[test]
    fn qwen_native_encoder_is_optional_and_discovered_only_when_present() {
        let base = reqwest::Url::parse("http://127.0.0.1:8188/").unwrap();
        let mut info = nodes();
        let without = parse(&base, &info).unwrap();
        assert!(!without["allowlist"]["nodes"]
            .as_array()
            .unwrap()
            .contains(&json!("TextEncodeQwenImage21")));
        info["TextEncodeQwenImage21"] = json!({});
        info["DEAS_ImagePromptEncode"] = json!({});
        let with = parse(&base, &info).unwrap();
        assert!(with["allowlist"]["nodes"]
            .as_array()
            .unwrap()
            .contains(&json!("TextEncodeQwenImage21")));
        assert!(with["allowlist"]["nodes"]
            .as_array()
            .unwrap()
            .contains(&json!("DEAS_ImagePromptEncode")));
        assert_eq!(with["missingNodes"], without["missingNodes"]);
    }

    async fn fixture(body: String) -> (String, tokio::task::JoinHandle<String>) {
        fixture_response(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()), Duration::ZERO).await
    }

    async fn fixture_response(
        response: String,
        delay: Duration,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut buffer = [0; 1024];
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buffer[..n]);
                if request.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            tokio::time::sleep(delay).await;
            // Oversize/timeout probes may close before the fixture finishes writing.
            let _ = socket.write_all(response.as_bytes()).await;
            String::from_utf8(request).unwrap()
        });
        (format!("http://{address}/comfy"), task)
    }

    #[tokio::test]
    async fn missing_nodes_are_diagnostics_with_empty_dependent_choices() {
        for absent in [
            vec!["CheckpointLoaderSimple"],
            vec!["KSampler"],
            REQUIRED_NODES.to_vec(),
        ] {
            let mut info = nodes();
            for name in &absent {
                info.as_object_mut().unwrap().remove(*name);
            }
            let (base, request) = fixture(info.to_string()).await;
            let result = native_recipe_prerequisites(base).await.unwrap();
            assert_eq!(result["missingNodes"], json!(absent));
            assert_eq!(
                result["allowlist"]["nodes"],
                json!(REQUIRED_NODES
                    .iter()
                    .copied()
                    .filter(|n| !absent.contains(n))
                    .chain([
                        "LoraLoader",
                        "CLIPVisionLoader",
                        "CLIPVisionEncode",
                        "unCLIPConditioning"
                    ])
                    .collect::<Vec<_>>())
            );
            if absent.contains(&"CheckpointLoaderSimple") {
                assert_eq!(result["allowlist"]["checkpoints"], json!([]));
            }
            if absent.contains(&"KSampler") {
                assert_eq!(result["allowlist"]["samplers"], json!([]));
                assert_eq!(result["allowlist"]["schedulers"], json!([]));
            }
            assert_eq!(result["generationReady"], false);
            request.await.unwrap();
        }
    }

    #[tokio::test]
    async fn rejects_unsafe_or_duplicate_enum_choices_without_normalizing() {
        for (field, choices) in [
            ("sampler_name", json!(["euler", "euler"])),
            ("scheduler", json!(["normal", "normal"])),
            ("sampler_name", json!([""])),
            ("scheduler", json!([" normal"])),
            ("scheduler", json!(["normal\u{0000}"])),
            ("scheduler", json!([null])),
            ("sampler_name", json!([42])),
        ] {
            let mut info = nodes();
            info["KSampler"]["input"]["required"][field][0] = choices.clone();
            let (base, request) = fixture(info.to_string()).await;
            assert!(
                native_recipe_prerequisites(base).await.is_err(),
                "accepted KSampler.{field}: {choices}"
            );
            request.await.unwrap();
        }
    }

    /// One oddly named file (spaces, umlauts, traversal) must not block every other model.
    #[tokio::test]
    async fn drops_unsafe_or_duplicate_model_files_without_normalizing() {
        for name in [
            "../model.safetensors",
            "/model.safetensors",
            "dir\\..\\model.safetensors",
            "https://host/model",
            " model.safetensors",
            "dir//model",
            "dir/./model",
            "dir/model.safetensors ",
            "C:/model.safetensors",
        ] {
            let mut info = nodes();
            info["CheckpointLoaderSimple"]["input"]["required"]["ckpt_name"][0] =
                json!([name, "ok.safetensors", "ok.safetensors"]);
            let (base, request) = fixture(info.to_string()).await;
            let result = native_recipe_prerequisites(base).await.unwrap();
            assert_eq!(
                result["allowlist"]["checkpoints"],
                json!(["ok.safetensors"]),
                "kept {name:?}"
            );
            request.await.unwrap();
        }
        let mut info = nodes();
        info["CheckpointLoaderSimple"]["input"]["required"]["ckpt_name"][0] =
            json!(["m".repeat(256)]);
        // GGUF dual text encoders are file combos too: one odd name must not reject the backend.
        info["DualCLIPLoaderGGUF"] = json!({"input":{"required":{"clip_name1":[[" odd.gguf", "ok.gguf", "ok.gguf"]],"type":[["flux"]]}}});
        let (base, request) = fixture(info.to_string()).await;
        let result = native_recipe_prerequisites(base).await.unwrap();
        assert_eq!(result["allowlist"]["checkpoints"], json!([]));
        assert_eq!(
            result["allowlist"]["ggufDualTextEncoders"],
            json!(["ok.gguf"])
        );
        request.await.unwrap();
        // ComfyUI lists user files with spaces and non-ASCII letters as they are; they stay usable.
        let mut info = nodes();
        info["LoraLoader"]["input"]["required"]["lora_name"][0] = json!([
            "MiniMaxH3/MM-H3 - Ahegao Face.safetensors",
            "Größe/模型 v2.safetensors",
            r"Größe\模型 v2.safetensors"
        ]);
        let (base, request) = fixture(info.to_string()).await;
        let result = native_recipe_prerequisites(base).await.unwrap();
        assert_eq!(
            result["allowlist"]["loras"],
            json!([
                "MiniMaxH3/MM-H3 - Ahegao Face.safetensors",
                "Größe/模型 v2.safetensors",
                r"Größe\模型 v2.safetensors"
            ])
        );
        request.await.unwrap();
    }

    #[tokio::test]
    async fn rejected_object_info_is_logged_for_the_user() {
        crate::reset_backend_probe_diagnostics();
        let mut info = nodes();
        info["KSampler"]["input"]["required"]["scheduler"][0] = json!([" normal"]);
        let (base, request) = fixture(info.to_string()).await;
        assert!(native_recipe_prerequisites(base).await.is_err());
        request.await.unwrap();
        // Sibling rejection tests share the once-per-minute limiter; only presence is stable.
        assert!(crate::logs::after(0)
            .iter()
            .any(|e| e.text.contains("prerequisites rejected: object_info ")));
    }

    #[tokio::test]
    async fn malformed_present_combo_schema_fails_closed() {
        for schema in [
            json!([["model"], null]),
            json!([["model"], {}, {}]),
            json!({"0":["model"]}),
            json!([]),
            json!("CHECKPOINT"),
            json!(["CHECKPOINT"]),
            json!([null]),
        ] {
            for (node, field) in [
                ("CheckpointLoaderSimple", "ckpt_name"),
                ("KSampler", "sampler_name"),
                ("KSampler", "scheduler"),
            ] {
                let mut info = nodes();
                info[node]["input"]["required"][field] = schema.clone();
                let (base, request) = fixture(info.to_string()).await;
                assert!(
                    native_recipe_prerequisites(base).await.is_err(),
                    "accepted {node}.{field}: {schema}"
                );
                request.await.unwrap();
            }
        }
    }

    #[tokio::test]
    async fn empty_model_inventory_is_not_generation_readiness() {
        let mut info = nodes();
        info["CheckpointLoaderSimple"]["input"]["required"]["ckpt_name"] = json!([[]]);
        let (base, request) = fixture(info.to_string()).await;
        let result = native_recipe_prerequisites(base).await.unwrap();
        assert_eq!(result["allowlist"]["checkpoints"], json!([]));
        assert_eq!(result["missingNodes"], json!([]));
        assert_eq!(result["generationReady"], false);
        request.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_invalid_object_info_or_missing_present_node_fields() {
        let mut cases = vec![json!([]), json!(null), json!({"KSampler":null})];
        for node in ["CheckpointLoaderSimple", "KSampler"] {
            let mut info = nodes();
            info[node] = json!({});
            cases.push(info);
        }
        for info in cases {
            let (base, request) = fixture(info.to_string()).await;
            assert!(native_recipe_prerequisites(base).await.is_err());
            request.await.unwrap();
        }
    }

    #[tokio::test]
    async fn http_errors_do_not_become_missing_node_diagnostics() {
        for status in [401, 403, 404, 500] {
            let response = format!(
                "HTTP/1.1 {status} Error\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
            );
            let (base, request) = fixture_response(response, Duration::ZERO).await;
            let error = native_recipe_prerequisites(base).await.unwrap_err();
            assert!(error.contains(&format!("HTTP {status}")), "{error}");
            request.await.unwrap();
        }
    }

    #[tokio::test]
    async fn rejects_html_and_invalid_json() {
        for body in ["<html>Not ComfyUI</html>", "{broken"] {
            let (base, request) = fixture(body.into()).await;
            assert!(native_recipe_prerequisites(base)
                .await
                .unwrap_err()
                .contains("invalid JSON"));
            request.await.unwrap();
        }
    }

    #[tokio::test]
    async fn redirects_never_contact_the_destination() {
        let destination = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let response = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://{}/object_info\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            destination.local_addr().unwrap()
        );
        let (base, request) = fixture_response(response, Duration::ZERO).await;
        assert!(native_recipe_prerequisites(base)
            .await
            .unwrap_err()
            .contains("HTTP 302"));
        request.await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), destination.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn bounds_declared_and_chunked_response_bytes() {
        let size = crate::MAX_RESPONSE_BYTES + 1;
        let declared =
            format!("HTTP/1.1 200 OK\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n");
        let chunked = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{size:x}\r\n{}\r\n0\r\n\r\n",
            " ".repeat(size)
        );
        for response in [declared, chunked] {
            let (base, request) = fixture_response(response, Duration::ZERO).await;
            assert!(native_recipe_prerequisites(base)
                .await
                .unwrap_err()
                .contains("response size limit"));
            request.await.unwrap();
        }
    }

    #[tokio::test]
    async fn enforces_ten_second_total_request_timeout() {
        let (base, request) = fixture_response(String::new(), Duration::from_secs(11)).await;
        let started = std::time::Instant::now();
        assert!(native_recipe_prerequisites(base)
            .await
            .unwrap_err()
            .starts_with("BACKEND_TIMEOUT:"));
        assert!(started.elapsed() >= Duration::from_secs(9));
        assert!(started.elapsed() < Duration::from_secs(11));
        request.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_unsafe_base_urls_before_network_access() {
        for base in [
            "file:///etc/passwd",
            "http://@localhost:8188",
            "http://user:secret@localhost:8188",
            "http://localhost:8188/?token=x",
            "http://localhost:8188/#fragment",
            "not a url",
        ] {
            assert!(native_recipe_prerequisites(base.into()).await.is_err());
        }
    }

    #[test]
    fn command_is_registered_with_local_read_only_permission() {
        assert!(include_str!("main.rs").contains("deas::native_recipe_prerequisites,"));
        let permissions: toml::Value =
            toml::from_str(include_str!("../permissions/api.toml")).unwrap();
        let permission = permissions["permission"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["identifier"].as_str() == Some("allow-native-recipe-prerequisites"))
            .expect("dedicated read-only permission missing");
        assert_eq!(
            permission["commands"]["allow"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["native_recipe_prerequisites"]
        );
        let capability: Value =
            serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
        assert_eq!(capability["local"], true);
        assert!(capability.get("remote").is_none());
        assert_eq!(capability["windows"], json!(["main"]));
        assert!(capability["permissions"]
            .as_array()
            .unwrap()
            .contains(&json!("allow-native-recipe-prerequisites")));
    }

    #[test]
    fn gguf_encoder_catalogs_are_independent_and_optional() {
        let base = crate::validate_base_url("http://127.0.0.1:8199").unwrap();
        let mut info = nodes();
        let empty = parse(&base, &info).unwrap();
        assert_eq!(empty["allowlist"]["ggufTextEncoders"], json!([]));
        info["CLIPLoaderGGUF"] = json!({"input":{"required":{"clip_name":[["encoder.gguf","plain.safetensors"]],"type":[["wan"]]}}});
        info["DualCLIPLoaderGGUF"] = json!({"input":{"required":{"clip_name1":[["gemma.gguf","projection.safetensors"]],"clip_name2":[["gemma.gguf","projection.safetensors"]],"type":[["ltxv"]]}}});
        let allow = parse(&base, &info).unwrap()["allowlist"].clone();
        assert_eq!(
            allow["ggufTextEncoders"],
            json!(["encoder.gguf", "plain.safetensors"])
        );
        assert_eq!(
            allow["ggufDualTextEncoders"],
            json!(["gemma.gguf", "projection.safetensors"])
        );
        assert_eq!(allow["ggufClipTypes"], json!(["wan"]));
        assert_eq!(allow["ggufDualClipTypes"], json!(["ltxv"]));
        assert!(allow["nodes"]
            .as_array()
            .unwrap()
            .contains(&json!("CLIPLoaderGGUF")));
        assert!(allow["nodes"]
            .as_array()
            .unwrap()
            .contains(&json!("DualCLIPLoaderGGUF")));
    }

    #[test]
    #[ignore = "read-only actual KJ schema replay; set DEAS_KJ_PREVIEW_SCHEMA to exported object_info entry"]
    fn video_preview_actual_kj_schema_replay() {
        let path = std::env::var_os("DEAS_KJ_PREVIEW_SCHEMA").unwrap();
        let actual: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let mut info = nodes();
        info["ModelPreviewOverrideKJ"] = actual;
        info["VAELoader"] =
            json!({"input":{"required":{"vae_name":[["LTX2/taeltx2_3.safetensors"]]}}});
        let result = video_previews(&info);
        assert_eq!(
            result["minimax-h3"],
            json!({"decoder":"tiny","model":"MiniMaxH3/taeh3.safetensors"})
        );
        assert_eq!(
            result["ltx-2"],
            json!({"decoder":"vae","model":"LTX2/taeltx2_3.safetensors"})
        );
        println!("Actual KJ preview discovery: {result}");
    }

    /// Real object_info shapes: core v3 combo (with spaces), KJNodes v1 combo (with `++`, `disabled`).
    #[tokio::test]
    async fn lists_option_nodes_and_choices_in_both_combo_forms() {
        let mut info = nodes();
        info["ModelAttentionBackend"] = json!({"input":{"required":{"model":["MODEL"],"attention":["COMBO",{"default":"pytorch attention","options":["pytorch attention","comfy kitchen attention"]}]}}});
        info["PathchSageAttentionKJ"] = json!({"input":{"required":{"model":["MODEL"],"sage_attention":[["disabled","auto","sageattn_qk_int8_pv_fp8_cuda++","bad\u{0000}"],{"default":false}]}}});
        info["VAEDecodeTiled"] = json!({});
        let (base, request) = fixture(info.to_string()).await;
        let result = native_recipe_prerequisites(base).await.unwrap();
        let allow = &result["allowlist"];
        assert_eq!(
            allow["attention"],
            json!(["pytorch attention", "comfy kitchen attention"])
        );
        assert_eq!(
            allow["sage"],
            json!(["auto", "sageattn_qk_int8_pv_fp8_cuda++"])
        );
        assert_eq!(allow["compile"], json!([]));
        let listed = allow["nodes"].as_array().unwrap();
        for node in [
            "ModelAttentionBackend",
            "PathchSageAttentionKJ",
            "VAEDecodeTiled",
        ] {
            assert!(listed.contains(&json!(node)), "{node}");
        }
        assert!(!listed.contains(&json!("EasyCache")));
        request.await.unwrap();
    }

    /// `DEAS_LIVE_COMFY=http://127.0.0.1:8199 cargo test --lib live_option_choices -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_option_choices() {
        let result = native_recipe_prerequisites(std::env::var("DEAS_LIVE_COMFY").unwrap())
            .await
            .unwrap();
        let allow = &result["allowlist"];
        println!(
            "nodes {}\nattention {}\nsage {}\ncompile {}",
            allow["nodes"], allow["attention"], allow["sage"], allow["compile"]
        );
        assert!(allow["attention"]
            .as_array()
            .unwrap()
            .contains(&json!("pytorch attention")));
    }

    #[tokio::test]
    async fn discovers_exact_core_choices_with_prefixed_get() {
        let (base, request) = fixture(nodes().to_string()).await;
        let result = native_recipe_prerequisites(base.clone()).await.unwrap();
        assert_eq!(
            result,
            json!({
                "baseUrl": format!("{base}/"), "recipeId":"checkpoint-text-to-image-diagnostic", "version":1,
                "allowlist":{
                    "nodes":["CheckpointLoaderSimple","CLIPTextEncode","EmptyLatentImage","KSampler","VAEDecode","SaveImage","LoraLoader","CLIPVisionLoader","CLIPVisionEncode","unCLIPConditioning"],
                    "checkpoints":["sd/model.safetensors"], "loras":["style/detail.safetensors"], "images":[], "samplers":["euler","dpmpp_2m"], "schedulers":["normal","karras"],
                    "diffusionModels":[], "ggufDiffusionModels":[], "ggufTextEncoders":[], "ggufClipTypes":[], "ggufDualTextEncoders":[], "ggufDualClipTypes":[], "textEncoders":[], "dualTextEncoders":[], "dualClipTypes":[], "clipTypes":[], "vaes":[], "visionModels":["clip-vit.safetensors"], "attention":[], "sage":[], "compile":[], "upscaleModels":[], "latentUpscaleModels":[], "h3LatentUpscaleModels":[], "frameInterpolationModels":[], "rtxUpscaleModes":[], "audioEncoders":[], "videoPreviews":{}
                }, "missingNodes":[], "generationReady":false
            })
        );
        assert!(request
            .await
            .unwrap()
            .starts_with("GET /comfy/object_info HTTP/1.1\r\n"));
    }
}
