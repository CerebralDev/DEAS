use super::*;
use serde_json::json;

#[test]
fn gguf_video_models_load_via_gguf_loader_and_loras_stay_rejected() {
    for original in [ltx(), wan(), h3()] {
        let mut i = original.clone();
        i["model"] = json!("model.gguf");
        let graph = compile(&i).unwrap();
        assert_eq!(graph["1"]["class_type"], "UnetLoaderGGUF");
        assert!(graph["1"]["inputs"].get("weight_dtype").is_none());
        i["loras"] = json!([{"name":"style.safetensors","strength":0.5}]);
        assert!(compile(&i).unwrap_err().contains("LoRA on a GGUF base"));
        i["loras"] = json!([{"name":"style.gguf","strength":0.5}]);
        assert!(compile(&i).unwrap_err().contains("GGUF LoRA"));
    }
    for name in ["model.gguf", "model.GGUF"] {
        let mut i = ltx();
        i["model"] = json!(name);
        assert_eq!(compile(&i).unwrap()["1"]["class_type"], "UnetLoaderGGUF");
    }
    let mut i = wan();
    i["wan"]["lowModel"] = json!("low.gguf");
    let graph = compile(&i).unwrap();
    assert_eq!(graph["1L"]["class_type"], "UnetLoaderGGUF");
    i["loras"] = json!([{"name":"style.safetensors","strength":0.5}]);
    assert!(compile(&i).unwrap_err().contains("LoRA on a GGUF base"));
}

#[test]
fn gguf_video_encoders_and_requirements_follow_the_selected_loader() {
    for mut i in [ltx(), wan(), h3()] {
        i["textEncoder"] = json!("encoder.GGUF");
        let graph = compile(&i).unwrap();
        let dual = i["family"] == "ltx-2";
        assert_eq!(
            graph["C"]["class_type"],
            if dual {
                "DualCLIPLoaderGGUF"
            } else {
                "CLIPLoaderGGUF"
            }
        );
        let required = requirements(&i);
        let field = if dual {
            "ggufDualTextEncoders"
        } else {
            "ggufTextEncoders"
        };
        assert!(required
            .iter()
            .any(|(key, names)| *key == field && names.contains(&"encoder.GGUF")));
        if dual {
            assert!(required.iter().any(|(key, names)| *key == field
                && names.contains(&i["ltx"]["textProjection"].as_str().unwrap())));
        }
        i["vae"] = json!("vae.gguf");
        assert!(compile(&i).unwrap_err().contains("GGUF VAE"));
    }
}

#[test]
fn ref2va_continuation_uses_requested_seconds_without_new_take_frame_roundtrip() {
    let mut intent = h3();
    intent["frames"] = json!(158);
    intent["h3"]["mode"] = json!("REF2VA");
    intent["h3"]["references"] = json!([]);
    intent["h3"]["media"] = json!([]);
    intent["h3"]["continuity"] = json!({"session":"duration-test","sourceId":"parent","capture":true,"action":"turn","outputMode":"segment","durationSeconds":6.0});
    assert_eq!(compile(&intent).unwrap()["D"]["inputs"]["duration"], 6.0);
    for invalid in [
        json!(0),
        json!(-1),
        json!(15.01),
        json!("6"),
        json!(null),
        json!(true),
    ] {
        let mut bad = intent.clone();
        bad["h3"]["continuity"]["durationSeconds"] = invalid;
        assert!(compile(&bad).is_err());
    }
    let mut first = intent.clone();
    first["h3"]["continuity"]["sourceId"] = json!("");
    first["h3"]["media"] = json!([{"type":"image","file":"deas-inputs/ref.png","slot":0}]);
    assert!(compile(&first).is_err());
    let mut frame_mode = h3();
    frame_mode["h3"]["continuity"] = intent["h3"]["continuity"].clone();
    assert!(compile(&frame_mode).is_err());
    intent["h3"]["continuity"]
        .as_object_mut()
        .unwrap()
        .remove("durationSeconds");
    assert_eq!(
        compile(&intent).unwrap()["D"]["inputs"]["duration"],
        158.0 / 24.0
    );
}

#[test]
fn windows_video_models_preserve_ids_but_inputs_stay_portable() {
    for mut i in [ltx(), wan(), h3()] {
        for key in ["model", "textEncoder", "vae"] {
            i[key] = json!(format!(
                "models\\{}",
                i[key].as_str().unwrap().replace('/', "\\")
            ));
        }
        let block = block(i["family"].as_str().unwrap());
        for key in ["textProjection", "audioVae", "lowModel"] {
            if let Some(name) = i[block][key].as_str() {
                i[block][key] = json!(name.replace('/', "\\"));
            }
        }
        i["loras"] = json!([{"name":r"styles\detail.safetensors","strength":0.55}]);
        let graph = compile(&i).unwrap();
        assert!(graph
            .as_object()
            .unwrap()
            .values()
            .any(|node| { node["inputs"]["lora_name"] == r"styles\detail.safetensors" }));
        assert!(serde_json::to_string(&graph)
            .unwrap()
            .contains(&serde_json::to_string(&i["model"]).unwrap()));
        i["firstFrame"] = json!(r"deas-inputs\image.png");
        assert!(compile(&i).is_err());
    }
}

#[test]
fn omitted_video_output_prefix_defaults_to_raw() {
    let mut intent = ltx();
    intent.as_object_mut().unwrap().remove("outputPrefix");
    intent.as_object_mut().unwrap().remove("output");
    let graph = compile(&intent).unwrap();
    assert_eq!(graph["7"]["inputs"]["filename_prefix"], "raw/video/DEAS");
    let mut explicit = intent.clone();
    explicit["outputPrefix"] = effective_output_prefix(&intent);
    explicit["output"] = effective_output(&intent);
    assert_eq!(compile(&explicit).unwrap(), graph);
}

#[test]
fn latent_post_upscale_ltx_preserves_final_pass_baseline_and_audio() {
    for passes in 1..=3 {
        let mut i = ltx();
        i["ltx"]["passes"] = json!(passes);
        if passes > 1 {
            i["ltx"]["spatialUpscaler"] = json!("old.safetensors");
        }
        let before = compile(&i).unwrap();
        i["latentUpscale"] =
            json!({"model":"post/ltx-2.3-spatial-upscaler-x2-1.1.safetensors","scale":2});
        let g = compile(&i).unwrap();
        assert_eq!(
            g["U"]["inputs"]["samples"],
            before["6"]["inputs"]["samples"]
        );
        assert_eq!(g["U"]["class_type"], "LTXVLatentUpsampler");
        assert_eq!(g["U"]["inputs"]["upscale_model"], json!(["UML", 0]));
        assert_eq!(
            g["UML"]["inputs"]["model_name"],
            "post/ltx-2.3-spatial-upscaler-x2-1.1.safetensors"
        );
        assert_eq!(g["6"]["inputs"]["samples"], json!(["U", 0]));
        assert_eq!(g["B6"], before["6"]);
        assert_eq!(g["AD"], before["AD"]);
        let mut baseline = g["7"].clone();
        baseline["inputs"]["images"] = json!(["B6", 0]);
        baseline["inputs"]["filename_prefix"] = json!("video/2026-10-18/x_original");
        assert_eq!(g["B7"], baseline);
        let required = requirements(&i);
        assert_eq!(
            required
                .iter()
                .find(|(k, _)| *k == "latentUpscaleModels")
                .unwrap()
                .1,
            if passes > 1 {
                vec![
                    "old.safetensors",
                    "post/ltx-2.3-spatial-upscaler-x2-1.1.safetensors",
                ]
            } else {
                vec!["post/ltx-2.3-spatial-upscaler-x2-1.1.safetensors"]
            }
        );
    }
}

#[test]
fn latent_post_upscale_rejects_untrusted_intent() {
    for (mut i, options) in [
        (ltx(), json!(null)),
        (ltx(), json!({"model":"interpolation","scale":2})),
        (ltx(), json!({"model":"../bad.safetensors","scale":2})),
        (ltx(), json!({"model":"x.pt","scale":2})),
        (ltx(), json!({"model":"x.safetensors","scale":1})),
        (ltx(), json!({"model":"x.safetensors","scale":2,"steps":1})),
        (wan(), json!({"model":"x.safetensors","scale":2})),
        (
            h3(),
            json!({"model":"x.safetensors","scale":0.99,"steps":1,"denoise":0.2}),
        ),
        (
            h3(),
            json!({"model":"x.safetensors","scale":8.01,"steps":1,"denoise":0.2}),
        ),
        (
            h3(),
            json!({"model":"x.safetensors","scale":2,"steps":0,"denoise":0.2}),
        ),
        (
            h3(),
            json!({"model":"x.safetensors","scale":2,"steps":101,"denoise":0.2}),
        ),
        (
            h3(),
            json!({"model":"x.safetensors","scale":2,"steps":1,"denoise":1.01}),
        ),
        (h3(), json!({"model":"x.safetensors","scale":2,"steps":1})),
        (
            h3(),
            json!({"model":"x.safetensors","scale":2,"steps":1,"denoise":0.2,"advanced":{"cfg":20.5}}),
        ),
        (
            h3(),
            json!({"model":"x.safetensors","scale":2,"steps":1,"denoise":0.2,"advanced":{"precision":"int8"}}),
        ),
        (
            h3(),
            json!({"model":"x.safetensors","scale":2,"steps":1,"denoise":0.2,"advanced":{"scheduler":"Simple; rm"}}),
        ),
        (
            h3(),
            json!({"model":"x.safetensors","scale":2,"steps":1,"denoise":0.2,"advanced":{"memoryBudgetMb":131073}}),
        ),
        (
            h3(),
            json!({"model":"x.safetensors","scale":2,"steps":1,"denoise":0.2,"advanced":{"guardSensitivity":1.4}}),
        ),
        (
            h3(),
            json!({"model":"x.safetensors","scale":2,"steps":1,"denoise":0.2,"advanced":{"guardMode":"any"}}),
        ),
        (
            h3(),
            json!({"model":"x.safetensors","scale":2,"steps":1,"denoise":0.2,"advanced":{"spikeGuard":"yes"}}),
        ),
        (
            h3(),
            json!({"model":"x.safetensors","scale":2,"steps":1,"denoise":0.2,"advanced":{"unknown":1}}),
        ),
        (
            h3(),
            json!({"model":"x.safetensors","scale":2,"steps":1,"denoise":0.2,"advanced":[]}),
        ),
    ] {
        let mut options = options;
        if options["model"] == "x.safetensors" {
            options["model"] = if i["family"] == "ltx-2" {
                json!("ltx-spatial-upscaler.safetensors")
            } else {
                json!("minimax_h3_latent_upscaler.safetensors")
            };
        }
        i["latentUpscale"] = options;
        assert!(compile(&i).is_err(), "accepted {}", i["latentUpscale"]);
    }
    for mut i in [ltx(), h3()] {
        i["latentUpscale"] = if i["family"] == "ltx-2" {
            json!({"model":"folder/ltx-spatial-upscaler.sft","scale":2})
        } else {
            json!({"model":"folder/minimax_h3_latent_upscaler.sft","scale":1,"steps":100,"denoise":0})
        };
        assert!(compile(&i).is_ok());
    }
}

#[test]
fn h3_latent_advanced_options_override_only_the_given_node_inputs() {
    let mut i = h3();
    i["latentUpscale"] =
        json!({"model":"minimax_h3_latent_upscaler.safetensors","scale":2,"steps":3,"denoise":0.3});
    let plain = compile(&i).unwrap();
    i["latentUpscale"]["advanced"] = json!({
        "cfg":1.5,"scheduler":"beta","precision":"bf16","memoryBudgetMb":24000,
        "spatialTiling":false,"temporalChunking":false,"spikeGuard":true,"guardSensitivity":3.5,
        "guardStrength":0.5,"guardMode":"both","memoryPlanning":false,"memoryReservePct":10.0,
        "continuitySoftRefine":true,"continuityMaskStrength":0.25
    });
    let g = compile(&i).unwrap();
    let u = &g["U"]["inputs"];
    for (input, value) in [
        ("cfg", json!(1.5)),
        ("scheduler", json!("beta")),
        ("upscale_precision", json!("bf16")),
        ("memory_budget_mb", json!(24000)),
        ("spatial_tiling", json!(false)),
        ("temporal_chunking", json!(false)),
        ("spike_guard", json!(true)),
        ("guard_sensitivity", json!(3.5)),
        ("guard_strength", json!(0.5)),
        ("guard_mode", json!("both")),
        ("enforce_memory_budget", json!(false)),
        ("memory_reserve_pct", json!(10.0)),
        ("continuity_soft_refine", json!(true)),
        ("continuity_mask_strength", json!(0.25)),
    ] {
        assert_eq!(u[input], value, "{input}");
    }
    // Refine steps/denoise stay the upscaler's own values, never the generation steps.
    assert_eq!(u["steps"], json!(3));
    assert_eq!(u["denoise"], json!(0.3));
    // Omitted advanced options keep the node defaults: an empty object compiles like none.
    i["latentUpscale"]["advanced"] = json!({});
    assert_eq!(compile(&i).unwrap(), plain);
    assert_eq!(plain["U"]["inputs"]["scheduler"], json!("simple"));
    assert_eq!(plain["U"]["inputs"]["memory_reserve_pct"], json!(5.0));
}

#[test]
fn latent_post_upscale_h3_wires_prepared_context_and_processed_publish() {
    let mut i = h3();
    let before = compile(&i).unwrap();
    i["latentUpscale"] = json!({"model":"h3/minimax_h3_latent_upscaler_3d_conv_v1_bf16.safetensors","scale":1.5,"steps":3,"denoise":0.25});
    let g = compile(&i).unwrap();
    assert_eq!(g["U"]["class_type"], "DaSiWaH3TiledUpscale");
    let inputs = &g["U"]["inputs"];
    for (key, value) in [
        ("noise", json!(["N", 0])),
        ("model", json!(["SS", 0])),
        ("conditioning", json!(["G", 0])),
        ("latent", json!(["SP", 0])),
        ("continuity_context", json!(["SP", 1])),
        ("director_guide", json!(["D", 0])),
        ("vae", json!(["V", 0])),
        ("sampler", json!(["Q", 0])),
        (
            "upscale_model",
            json!("h3/minimax_h3_latent_upscaler_3d_conv_v1_bf16.safetensors"),
        ),
        ("scale", json!(1.5)),
        ("steps", json!(3)),
        ("denoise", json!(0.25)),
        ("seed", json!(7)),
        ("cfg", json!(1.0)),
        ("scheduler", json!("simple")),
        ("upscale_precision", json!("auto")),
        ("memory_budget_mb", json!(0)),
        ("spatial_tiling", json!(true)),
        ("temporal_chunking", json!(true)),
        ("enforce_memory_budget", json!(true)),
        ("memory_reserve_pct", json!(5.0)),
        ("continuity_soft_refine", json!(false)),
        ("continuity_mask_strength", json!(1.0)),
        ("spike_guard", json!(false)),
        ("guard_sensitivity", json!(4.3)),
        ("guard_strength", json!(1.0)),
        ("guard_mode", json!("burn_direction")),
    ] {
        assert_eq!(inputs[key], value, "{key}");
    }
    assert_eq!(g["SP"], before["SP"]);
    assert_eq!(g["BSD"], before["SD"]);
    let mut processed = before["SD"].clone();
    processed["inputs"]["latent"] = json!(["U", 0]);
    assert_eq!(g["SD"], processed);
    assert_eq!(g["7"], before["7"]);
    assert_eq!(g["P"], before["P"]);
    let mut baseline = before["7"].clone();
    baseline["inputs"]["images"] = json!(["BSD", 0]);
    baseline["inputs"]["audio"] = json!(["BSD", 1]);
    baseline["inputs"]["filename_prefix"] = json!("video/2026-10-18/x_original");
    assert_eq!(g["B7"], baseline);
    let required = requirements(&i);
    assert_eq!(
        required
            .iter()
            .find(|(k, _)| *k == "h3LatentUpscaleModels")
            .unwrap()
            .1,
        vec!["h3/minimax_h3_latent_upscaler_3d_conv_v1_bf16.safetensors"]
    );
    assert!(required
        .iter()
        .find(|(k, _)| *k == "latentUpscaleModels")
        .unwrap()
        .1
        .is_empty());
}

#[test]
fn latent_post_upscale_bounds_canvas_prefix_and_h3_capture() {
    for (family, width, height, scale, ok) in [
        ("ltx-2", 4096, 4096, 2.0, false),
        ("ltx-2", 4096, 1024, 2.0, true),
        ("minimax-h3", 4096, 64, 2.00390625, true), // 8208: tie rounds down to even 8192.
        ("minimax-h3", 4096, 64, 2.01171875, false), // 8240: tie rounds up to even 8256.
        ("minimax-h3", 4096, 4096, 1.00390625, true), // Both 4112 ties round down.
        ("minimax-h3", 4096, 4096, 1.004, false),
    ] {
        let mut i = if family == "ltx-2" { ltx() } else { h3() };
        i["width"] = json!(width);
        i["height"] = json!(height);
        i["latentUpscale"] = if family == "ltx-2" {
            json!({"model":"ltx-spatial-upscaler.safetensors","scale":scale})
        } else {
            json!({"model":"minimax_h3_latent_upscaler.safetensors","scale":scale,"steps":1,"denoise":0.2})
        };
        assert_eq!(
            compile(&i).is_ok(),
            ok,
            "{family} {width}x{height} * {scale}"
        );
    }
    let mut i = h3();
    i["latentUpscale"] =
        json!({"model":"minimax_h3_latent_upscaler.safetensors","scale":2,"steps":1,"denoise":0.2});
    for capture in [false, true] {
        for mode in ["full", "segment"] {
            i["h3"]["continuity"] = json!({"session":"take","sourceId":"","action":"","capture":capture,"outputMode":mode});
            assert_eq!(compile(&i).is_ok(), !(capture && mode == "segment"));
        }
    }
    i["h3"].as_object_mut().unwrap().remove("continuity");
    for (length, ok) in [(171, true), (172, false), (180, false)] {
        i["outputPrefix"] = json!("x".repeat(length));
        assert_eq!(compile(&i).is_ok(), ok, "prefix {length}");
    }
}

#[test]
fn h3_latent_upscale_segment_capture_decodes_the_baseline_on_core_nodes() {
    let mut i = h3();
    i["h3"]["mode"] = json!("REF2VA");
    i["h3"]["references"] = json!([]);
    i["h3"]["media"] = json!([]);
    i["latentUpscale"] =
        json!({"model":"minimax_h3_latent_upscaler.safetensors","scale":2,"steps":1,"denoise":0.2});
    i["h3"]["continuity"] = json!({"session":"deas-ab12cd34","sourceId":"clip_1","capture":true,"action":"she turns around","outputMode":"segment"});
    // Fail closed without the pinned clip's frame count (prepare looks it up in the session list).
    assert!(compile(&i).is_err());
    i["h3"]["continuity"]["sourceFrames"] = json!(243);
    let g = compile(&i).unwrap();
    // Exactly one SegmentDecode, fed by the upscale chain and kept on the publish path.
    assert_eq!(g["SD"]["inputs"]["latent"], json!(["U", 0]));
    assert_eq!(g["U"]["inputs"]["latent"], json!(["SP", 0]));
    assert_eq!(g["7"]["inputs"]["images"], json!(["SD", 0]));
    assert!(
        g.get("BSD").is_none(),
        "segment capture uses exactly one SegmentDecode"
    );
    let decodes = g
        .as_object()
        .unwrap()
        .values()
        .filter(|n| n["class_type"] == "DaSiWaH3ContinuitySegmentDecode")
        .count();
    assert_eq!(decodes, 1);
    // Baseline side: core decode of the prepared window cropped to the new segment (the pinned
    // 243-frame clip starts the window at 102, frames/24 duration = 119 frames) plus its audio.
    assert_eq!(g["BA"]["inputs"]["samples"], json!(["SP", 0]));
    assert_eq!(
        g["CB"]["inputs"],
        json!({"image":["BA",0],"batch_index":141,"length":119})
    );
    assert_eq!(
        g["SD"]["inputs"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        vec!["audio_vae", "context", "latent", "vae"],
        "SegmentDecode accepts no duration input"
    );
    assert_eq!(g["B7"]["inputs"]["images"], json!(["CB", 0]));
    assert_eq!(g["B7"]["inputs"]["audio"], json!(["SD", 1]));
    assert_eq!(
        g["B7"]["inputs"]["filename_prefix"],
        json!("video/2026-10-18/x_original")
    );
    assert_eq!(
        g["7"]["inputs"]["filename_prefix"],
        json!("video/2026-10-18/x")
    );
    // An authored duration snaps the crop to 17-frame steps; video sources stay fail-closed.
    i["h3"]["continuity"]["durationSeconds"] = json!(10.0);
    let g = compile(&i).unwrap();
    assert_eq!(g["CB"]["inputs"]["length"], json!(238));
    i["h3"]["continuity"]["sourceKind"] = json!("video");
    i["h3"]["continuity"]["sourceId"] = json!("0123456789abcdef".repeat(4));
    assert!(compile(&i).is_err());
}

#[test]
fn latent_post_upscale_rejects_cross_family_models_even_when_core_lists_them() {
    for (mut i, model) in [
        (
            ltx(),
            "minimax_h3_latent_upscaler_3d_conv_v1_bf16.safetensors",
        ),
        (ltx(), "folder/other.safetensors"),
        (ltx(), "ltx-checkpoint.safetensors"),
        (h3(), "ltx-2.3-spatial-upscaler-x2-1.1.safetensors"),
        (h3(), "folder/other.safetensors"),
        (h3(), "minimax_h3_latent_upscaler_ltx.safetensors"),
    ] {
        i["latentUpscale"] = if i["family"] == "ltx-2" {
            json!({"model":model,"scale":2})
        } else {
            json!({"model":model,"scale":2,"steps":1,"denoise":0.2})
        };
        assert!(compile(&i).is_err(), "accepted {model}");
    }
    for (mut i, model) in [
        (ltx(), r"LTX2\ltx-2.3-spatial-upscaler-x2-1.1.safetensors"),
        (h3(), r"H3\minimax_h3_latent_upscaler_3d_conv_v1_fp16.sft"),
    ] {
        i["latentUpscale"] = if i["family"] == "ltx-2" {
            json!({"model":model,"scale":2})
        } else {
            json!({"model":model,"scale":2,"steps":1,"denoise":0.2})
        };
        assert!(compile(&i).is_ok());
    }
}

#[test]
fn tve_post_chain_runs_after_the_decode_with_a_source_rate_baseline() {
    let post = json!({"rtx":{"upscale":{"mode":"VSR","quality":"Ultra","scale":2}},"frameGeneration":{"engine":"rtx","multiplier":2,"mode":"HIGH"}});
    for (mut i, decoded, audio) in [
        (ltx(), json!(["6", 0]), Some(json!(["AD", 0]))),
        (wan(), json!(["6", 0]), None),
        (h3(), json!(["SD", 0]), Some(json!(["SD", 1]))),
    ] {
        i["videoPost"] = post.clone();
        let g = compile(&i).unwrap();
        assert_eq!(g["PR"]["inputs"]["images"], decoded);
        assert_eq!(g["PF"]["inputs"]["images"], json!(["PR", 0]));
        assert_eq!(g["7"]["inputs"]["images"], json!(["PF", 0]));
        assert_eq!(
            g["7"]["inputs"]["frame_rate"],
            i["frameRate"].as_f64().unwrap() * 2.0
        );
        assert_eq!(g["B7"]["inputs"]["images"], decoded);
        assert_eq!(
            g["B7"]["inputs"]["audio"],
            audio.clone().unwrap_or(Value::Null)
        );
        assert_eq!(
            g["B7"]["inputs"]["filename_prefix"],
            "video/2026-10-18/x_original"
        );
        assert_eq!(
            g["B7"]["inputs"]["frame_rate"],
            if i["family"] == "minimax-h3" {
                json!(["D", 8])
            } else {
                i["frameRate"].clone()
            }
        );
        assert!(requirements(&i)
            .iter()
            .all(|(k, _)| *k != "frameInterpolationModels"));
        i["videoPost"] = json!({"upscaleModel":"2x.safetensors","frameGeneration":{"engine":"rife","multiplier":3,"model":"rife_v4.26.safetensors"}});
        let g = compile(&i).unwrap();
        assert_eq!(g["PM"]["inputs"]["image"], decoded);
        assert_eq!(g["PF"]["inputs"]["interp_model"], json!(["PFL", 0]));
        assert!(requirements(&i)
            .contains(&("frameInterpolationModels", vec!["rife_v4.26.safetensors"])));
        assert!(requirements(&i).contains(&("upscaleModels", vec!["2x.safetensors"])));
        i["videoPost"] = json!({});
        assert!(compile(&i).unwrap_err().contains("videoPost"));
    }
    // With a latent upscale the baseline stays the pre-upscale decode and the chain follows the upscale.
    let mut i = ltx();
    i["latentUpscale"] = json!({"model":"ltx-2.3-spatial-upscaler-x2-1.1.safetensors","scale":2});
    i["videoPost"] = json!({"rtx":{"denoise":"High"}});
    let g = compile(&i).unwrap();
    assert_eq!(g["B7"]["inputs"]["images"], json!(["B6", 0]));
    assert_eq!(g["PR"]["inputs"]["images"], json!(["6", 0]));
    assert_eq!(g["7"]["inputs"]["images"], json!(["PR", 0]));
    assert_eq!(g["7"]["inputs"]["frame_rate"], 24);
}

pub(crate) fn ltx() -> Value {
    json!({
        "recipeId":"video-generation","version":1,"family":"ltx-2",
        "positivePrompt":"a fox","negativePrompt":"text and subtitles",
        "model":"LTX2/ltx.safetensors","textEncoder":"gemma_3_12B_it_fp8.safetensors","vae":"LTX2/video_vae.safetensors",
        "seed":7,"width":1280,"height":704,"frames":121,"frameRate":24,"steps":8,"cfg":1,"sampler":"euler_cfg_pp","scheduler":"linear_quadratic",
        "outputPrefix":"video/2026-10-18/x",
        "output":{"codec":"Auto","container":"Auto","bitDepth":"Auto","quality":20,"audioCodec":"Auto","audioBitrate":"192k","saveFirstFrame":false,"saveLastFrame":false},
        "ltx":{"textProjection":"ltx-2.3_text_projection_bf16.safetensors","audioVae":"LTX2/audio_vae.safetensors","passes":1}
    })
}

pub(crate) fn wan() -> Value {
    let mut i = ltx();
    i["family"] = json!("wan");
    i.as_object_mut().unwrap().remove("ltx");
    i["model"] = json!("WAN22/high.safetensors");
    i["textEncoder"] = json!("umt5_xxl_fp8.safetensors");
    i["vae"] = json!("Wan/wan_2.1_vae.safetensors");
    i["width"] = json!(528);
    i["height"] = json!(768);
    i["frames"] = json!(81);
    i["frameRate"] = json!(16);
    i["steps"] = json!(4);
    i["sampler"] = json!("euler");
    i["firstFrame"] = json!("first.png");
    i["wan"] =
        json!({"lowModel":"WAN22/low.safetensors","shiftHigh":5,"shiftLow":5,"refinerStep":2});
    i
}

pub(crate) fn h3() -> Value {
    let mut i = ltx();
    i["family"] = json!("minimax-h3");
    i.as_object_mut().unwrap().remove("ltx");
    i["model"] = json!("MiniMaxH3/fl2va.safetensors");
    i["textEncoder"] = json!("qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors");
    i["vae"] = json!("MiniMaxH3/video_vae.safetensors");
    i["width"] = json!(1024);
    i["height"] = json!(768);
    i["frames"] = json!(124);
    i["steps"] = json!(4);
    i["sampler"] = json!("euler");
    i["scheduler"] = json!("simple");
    i["h3"] = json!({"mode":"I2VA","audioVae":"MiniMaxH3/audio_vae.safetensors","shiftVideo":10,"shiftAudio":3,"references":[{"slot":0,"image":"deas-inputs/a.png"}]});
    i
}

#[test]
fn rejects_unknown_fields_bad_grids_and_foreign_family_blocks() {
    for (key, value) in [
        ("extra", json!(1)),
        ("frames", json!(120)),
        ("width", json!(1000)),
        ("frameRate", json!(0)),
        ("chunks", json!(1)),
        ("output", json!({"codec":"H.266"})),
        (
            "wan",
            json!({"lowModel":"a","shiftHigh":5,"shiftLow":5,"refinerStep":2}),
        ),
        (
            "ltx",
            json!({"textProjection":"a","audioVae":"b","passes":2}),
        ),
        (
            "ltx",
            json!({"textProjection":"a","audioVae":"b","passes":1,"spatialUpscaler":"u"}),
        ),
    ] {
        let mut bad = ltx();
        bad[key] = value;
        assert!(compile(&bad).is_err(), "accepted {key}");
    }
    let mut missing = ltx();
    missing.as_object_mut().unwrap().remove("ltx");
    assert!(compile(&missing).is_err());
    let mut wan_bad = wan();
    wan_bad["wan"]["refinerStep"] = json!(4);
    assert!(
        compile(&wan_bad).is_err(),
        "refiner step must be below steps"
    );
    wan_bad = wan();
    wan_bad.as_object_mut().unwrap().remove("firstFrame");
    assert!(compile(&wan_bad).is_err(), "Wan needs a first frame");
    for bad in [
        json!({"mode":"REF2VA"}),
        json!({"references":[]}),
        json!({"mode":"T2VA"}),
        json!({"mode":"FL2VA","references":[{"slot":2,"image":"deas-inputs/a.png"}]}),
        json!({"references":[{"slot":0,"image":"a.png"}]}),
        json!({"continuity":{"session":"x/y","sourceId":"","capture":true,"action":"","outputMode":"full"}}),
    ] {
        let mut i = h3();
        for (k, v) in bad.as_object().unwrap() {
            i["h3"][k] = v.clone();
        }
        assert!(compile(&i).is_err(), "accepted {bad}");
    }
}

#[test]
fn ltx_single_pass_matches_omniforge_wiring() {
    let g = compile(&ltx()).unwrap();
    assert_eq!(g["1"]["class_type"], "UNETLoader");
    assert_eq!(
        g["C"]["inputs"],
        json!({"clip_name1":"gemma_3_12B_it_fp8.safetensors","clip_name2":"ltx-2.3_text_projection_bf16.safetensors","type":"ltxv"})
    );
    assert_eq!(g["AV"]["inputs"]["vae_name"], "LTX2/audio_vae.safetensors");
    assert_eq!(g["CN"]["inputs"]["frame_rate"], 24);
    assert_eq!(
        g["EL"]["inputs"],
        json!({"width":1280,"height":704,"length":121,"batch_size":1})
    );
    assert_eq!(
        g["EA"]["inputs"],
        json!({"frames_number":121,"frame_rate":24,"batch_size":1,"audio_vae":["AV",0]})
    );
    assert_eq!(
        g["CAT"]["inputs"],
        json!({"video_latent":["EL",0],"audio_latent":["EA",0]})
    );
    assert_eq!(
        g["GU"]["inputs"],
        json!({"model":["1",0],"positive":["CN",0],"negative":["CN",1],"cfg":1})
    );
    assert_eq!(
        g["Z"]["inputs"],
        json!({"model":["1",0],"scheduler":"linear_quadratic","steps":8,"denoise":1.0})
    );
    assert_eq!(
        g["5"]["inputs"],
        json!({"noise":["N",0],"guider":["GU",0],"sampler":["Q",0],"sigmas":["Z",0],"latent_image":["CAT",0]})
    );
    assert_eq!(g["SEP"]["inputs"]["av_latent"], json!(["5", 0]));
    assert_eq!(g["6"]["inputs"], json!({"samples":["SEP",0],"vae":["V",0]}));
    assert_eq!(
        g["AD"]["inputs"],
        json!({"samples":["SEP",1],"audio_vae":["AV",0]})
    );
    assert_eq!(g["7"]["class_type"], "DaSiWa_EnhancedVideoCombine");
    assert_eq!(g["7"]["inputs"]["audio"], json!(["AD", 0]));
    assert_eq!(g["7"]["inputs"]["frame_rate"], 24);
    assert_eq!(g["7"]["inputs"]["quality"], 20);
    assert_eq!(g["7"]["inputs"]["filename_prefix"], "video/2026-10-18/x");
    assert!(g.get("G1").is_none() && g.get("CR").is_none());
    for n in g.as_object().unwrap().values() {
        assert!(
            VIDEO_NODES.contains(&n["class_type"].as_str().unwrap())
                || ["UNETLoader", "VAELoader", "CLIPTextEncode", "VAEDecode"]
                    .contains(&n["class_type"].as_str().unwrap()),
            "{n}"
        );
    }
}

#[test]
fn ltx_guides_chunking_loras_and_tiled_decode() {
    let mut i = ltx();
    i["firstFrame"] = json!("a.png");
    i["lastFrame"] = json!("b.png [output]");
    i["chunks"] = json!(3);
    i["loras"] = json!([{"name":"LTX2/x.safetensors","strength":0.55}]);
    i["options"] = json!({"attention":"comfy kitchen attention","vaeTile":512});
    let g = compile(&i).unwrap();
    assert_eq!(g["8"]["inputs"]["image"], "a.png");
    assert_eq!(
        g["8S"]["inputs"],
        json!({"image":["8",0],"upscale_method":"lanczos","width":1280,"height":704,"crop":"center"})
    );
    assert_eq!(
        g["8P"]["inputs"],
        json!({"image":["8S",0],"img_compression":18})
    );
    assert_eq!(
        g["G1"]["inputs"],
        json!({"positive":["CN",0],"negative":["CN",1],"vae":["V",0],"latent":["EL",0],"image":["8P",0],"frame_idx":0,"strength":1.0})
    );
    assert_eq!(g["G2"]["inputs"]["latent"], json!(["G1", 2]));
    assert_eq!(g["G2"]["inputs"]["frame_idx"], -1);
    assert_eq!(g["CAT"]["inputs"]["video_latent"], json!(["G2", 2]));
    assert_eq!(g["GU"]["inputs"]["positive"], json!(["G2", 0]));
    assert_eq!(
        g["CR"]["inputs"],
        json!({"positive":["G2",0],"negative":["G2",1],"latent":["SEP",0]})
    );
    assert_eq!(g["6"]["class_type"], "VAEDecodeTiled");
    assert_eq!(g["6"]["inputs"]["samples"], json!(["CR", 2]));
    assert_eq!(g["6"]["inputs"]["tile_size"], 512);
    assert_eq!(g["L1"]["inputs"]["model"], json!(["1", 0]));
    assert_eq!(g["L1"]["inputs"]["clip"], json!(["C", 0]));
    assert_eq!(g["A"]["inputs"]["model"], json!(["L1", 0]));
    assert_eq!(
        g["H"],
        json!({"class_type":"LTXVChunkFeedForward","inputs":{"model":["A",0],"chunks":3,"dim_threshold":4096}})
    );
    assert_eq!(g["GU"]["inputs"]["model"], json!(["H", 0]));
    assert_eq!(g["Z"]["inputs"]["model"], json!(["H", 0]));
    assert_eq!(g["2"]["inputs"]["clip"], json!(["L1", 1]));
    let req = requirements(&i);
    assert_eq!(
        req.iter().find(|(k, _)| *k == "images").unwrap().1,
        vec!["a.png"]
    );
    assert_eq!(
        req.iter().find(|(k, _)| *k == "textEncoders").unwrap().1,
        vec![
            "gemma_3_12B_it_fp8.safetensors",
            "ltx-2.3_text_projection_bf16.safetensors"
        ]
    );
}

#[test]
fn ltx_multipass_upsamples_the_latent_and_readds_guides() {
    let mut i = ltx();
    i["firstFrame"] = json!("a.png");
    i["ltx"]["passes"] = json!(3);
    i["ltx"]["spatialUpscaler"] = json!("ltx-2.3-spatial-upscaler-x2-1.1.safetensors");
    let g = compile(&i).unwrap();
    assert_eq!(g["EL"]["inputs"]["width"], 640);
    assert_eq!(g["EL"]["inputs"]["height"], 352);
    assert_eq!(
        g["8H"]["inputs"],
        json!({"image":["8P",0],"upscale_method":"lanczos","width":640,"height":352,"crop":"disabled"})
    );
    assert_eq!(g["G1"]["inputs"]["image"], json!(["8H", 0]));
    assert_eq!(
        g["UP"]["inputs"],
        json!({"samples":["CR",2],"upscale_model":["UL",0],"vae":["V",0]})
    );
    assert_eq!(g["G1b"]["inputs"]["latent"], json!(["UP", 0]));
    assert_eq!(g["G1b"]["inputs"]["image"], json!(["8P", 0]));
    assert_eq!(
        g["CATb"]["inputs"],
        json!({"video_latent":["G1b",2],"audio_latent":["SEP",1]})
    );
    assert_eq!(g["Zb"]["inputs"]["steps"], 4);
    assert_eq!(g["Zb"]["inputs"]["denoise"], 0.4);
    assert_eq!(g["G1c"]["inputs"]["latent"], json!(["CRb", 2]));
    assert_eq!(g["CATc"]["inputs"]["audio_latent"], json!(["SEPb", 1]));
    assert_eq!(g["Zc"]["inputs"]["steps"], 2);
    assert_eq!(g["6"]["inputs"]["samples"], json!(["CRc", 2]));
    assert_eq!(g["AD"]["inputs"]["samples"], json!(["SEPc", 1]));
    i["ltx"]["passes"] = json!(2);
    let g = compile(&i).unwrap();
    assert!(g.get("Zc").is_none());
    assert_eq!(g["6"]["inputs"]["samples"], json!(["CRb", 2]));
    assert_eq!(
        requirements(&i)
            .iter()
            .find(|(k, _)| *k == "latentUpscaleModels")
            .unwrap()
            .1,
        vec!["ltx-2.3-spatial-upscaler-x2-1.1.safetensors"]
    );
}

#[test]
fn wan_high_low_split_matches_fastfidelity() {
    let g = compile(&wan()).unwrap();
    assert_eq!(g["1"]["inputs"]["unet_name"], "WAN22/high.safetensors");
    assert_eq!(g["1L"]["inputs"]["unet_name"], "WAN22/low.safetensors");
    assert_eq!(
        g["C"]["inputs"],
        json!({"clip_name":"umt5_xxl_fp8.safetensors","type":"wan"})
    );
    assert_eq!(
        g["SH"],
        json!({"class_type":"ModelSamplingSD3","inputs":{"model":["1",0],"shift":5}})
    );
    assert_eq!(g["SL"]["inputs"]["model"], json!(["1L", 0]));
    assert_eq!(g["W"]["class_type"], "WanImageToVideo");
    assert_eq!(g["W"]["inputs"]["start_image"], json!(["8S", 0]));
    assert_eq!(g["W"]["inputs"]["length"], 81);
    assert_eq!(
        g["KH"]["inputs"],
        json!({"model":["SH",0],"add_noise":"enable","noise_seed":7,"steps":4,"cfg":1,"sampler_name":"euler","scheduler":"linear_quadratic","positive":["W",0],"negative":["W",1],"latent_image":["W",2],"start_at_step":0,"end_at_step":2,"return_with_leftover_noise":"enable"})
    );
    assert_eq!(g["KL"]["inputs"]["add_noise"], "disable");
    assert_eq!(g["KL"]["inputs"]["latent_image"], json!(["KH", 0]));
    assert_eq!(g["KL"]["inputs"]["start_at_step"], 2);
    assert_eq!(g["KL"]["inputs"]["end_at_step"], 10000);
    assert_eq!(g["6"]["inputs"]["samples"], json!(["KL", 0]));
    assert!(g["7"]["inputs"].get("audio").is_none());
    let mut flf = wan();
    flf["lastFrame"] = json!("last.png");
    flf["chunks"] = json!(2);
    let g = compile(&flf).unwrap();
    assert_eq!(g["W"]["class_type"], "WanFirstLastFrameToVideo");
    assert_eq!(g["W"]["inputs"]["end_image"], json!(["9S", 0]));
    assert_eq!(g["H"]["class_type"], "WanChunkFeedForward");
    assert_eq!(g["lH"]["inputs"]["model"], json!(["1L", 0]));
    assert_eq!(g["SL"]["inputs"]["model"], json!(["lH", 0]));
    assert_eq!(
        requirements(&flf)
            .iter()
            .find(|(k, _)| *k == "diffusionModels")
            .unwrap()
            .1,
        vec!["WAN22/high.safetensors", "WAN22/low.safetensors"]
    );
}

#[test]
fn h3_drives_director_guide_segment_nodes_and_publish() {
    let g = compile(&h3()).unwrap();
    assert_eq!(
        g["C"]["inputs"],
        json!({"clip_name":"qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors","type":"minimax"})
    );
    let d = &g["D"];
    assert_eq!(d["class_type"], "MiniMaxH3Director");
    assert_eq!(d["inputs"]["mode"], "I2VA");
    assert_eq!(d["inputs"]["prompt"], "a fox");
    assert_eq!(d["inputs"]["duration"], 124.0 / 24.0);
    assert_eq!(d["inputs"]["frame_rate"], 24);
    assert_eq!(d["inputs"]["fl2va_model"], json!(["1", 0]));
    assert!(d["inputs"].get("ref2va_model").is_none());
    let timeline: Value =
        serde_json::from_str(d["inputs"]["timeline_data"].as_str().unwrap()).unwrap();
    assert_eq!(
        timeline["items"],
        json!([{"id":"ref0","type":"image","value":"deas-inputs/a.png","slot":0,"order":0,"enabled":true}])
    );
    assert_eq!(timeline["resolution"]["input_scaling"], "Fill and crop");
    // The Director's input scaling is optional, from its fixed list only.
    let mut scaled = h3();
    scaled["h3"]["inputScaling"] = json!("Long side with divisible crop");
    let g = compile(&scaled).unwrap();
    let timeline: Value =
        serde_json::from_str(g["D"]["inputs"]["timeline_data"].as_str().unwrap()).unwrap();
    assert_eq!(
        timeline["resolution"]["input_scaling"],
        "Long side with divisible crop"
    );
    for bad in [json!("fill and crop"), json!(""), json!(1)] {
        scaled["h3"]["inputScaling"] = bad;
        assert!(compile(&scaled).is_err());
    }
    assert_eq!(
        timeline["continuity"],
        json!({"version":3,"session":"deas","capture":false,"output_mode":"full"})
    );
    assert_eq!(
        g["G"]["inputs"],
        json!({"clip":["C",0],"vae":["V",0],"guide":["D",0],"audio_vae":["AV",0]})
    );
    assert_eq!(
        g["SS"]["inputs"],
        json!({"model":["D",5],"shift_video":10,"shift_audio":3})
    );
    assert_eq!(
        g["B"]["inputs"],
        json!({"model":["SS",0],"conditioning":["G",0]})
    );
    assert_eq!(g["Z"]["inputs"]["model"], json!(["SS", 0]));
    assert_eq!(g["5"]["inputs"]["latent_image"], json!(["G", 1]));
    assert_eq!(
        g["SP"]["inputs"],
        json!({"sampled":["5",0],"context":["G",2]})
    );
    assert_eq!(
        g["SD"]["inputs"],
        json!({"latent":["SP",0],"vae":["V",0],"audio_vae":["AV",0],"context":["SP",1]})
    );
    assert_eq!(g["7"]["inputs"]["images"], json!(["SD", 0]));
    assert_eq!(g["7"]["inputs"]["audio"], json!(["SD", 1]));
    assert_eq!(g["7"]["inputs"]["frame_rate"], json!(["D", 8]));
    assert_eq!(
        g["P"]["inputs"],
        json!({"filename":["7",1],"ticket":["SP",2]})
    );
    assert!(g.get("3").is_none(), "H3 has no negative prompt");
    assert_eq!(
        requirements(&h3())
            .iter()
            .find(|(k, _)| *k == "images")
            .unwrap()
            .1,
        vec!["deas-inputs/a.png"]
    );
    // Continuity: a planned first take captures; later steps pin the previous clip and send new seconds.
    let mut c = h3();
    c["h3"]["continuity"] = json!({"session":"deas-ab12cd34","sourceId":"","capture":true,"action":"","outputMode":"segment"});
    let t: Value = serde_json::from_str(
        compile(&c).unwrap()["D"]["inputs"]["timeline_data"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        t["continuity"],
        json!({"version":3,"session":"deas-ab12cd34","capture":true,"output_mode":"segment"})
    );
    c["h3"]["continuity"]["sourceId"] = json!("clip_1");
    c["h3"]["continuity"]["action"] = json!("she turns around");
    let t: Value = serde_json::from_str(
        compile(&c).unwrap()["D"]["inputs"]["timeline_data"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        t["continuity"],
        json!({"version":3,"session":"deas-ab12cd34","source_kind":"checkpoint","source_id":"clip_1","capture":true,"output_mode":"segment","continuation_prompt":"she turns around"})
    );
}

#[test]
fn h3_continuity_video_maps_only_source_fields_for_all_options() {
    let mut i = h3();
    let source = "0123456789abcdef".repeat(4);
    for capture in [false, true] {
        for output in ["full", "segment"] {
            for overlap in [None, Some(73), Some(56), Some(39), Some(22), Some(5)] {
                i["h3"]["continuity"] = json!({"session":"deas-ab12cd34","sourceId":source,"capture":capture,"action":"she turns around","outputMode":output});
                if let Some(frames) = overlap {
                    i["h3"]["continuity"]["overlapFrames"] = json!(frames);
                }
                let baseline = compile(&i).unwrap();
                let mut expected = baseline.clone();
                let mut timeline: Value = serde_json::from_str(
                    baseline["D"]["inputs"]["timeline_data"].as_str().unwrap(),
                )
                .unwrap();
                timeline["continuity"]["source_kind"] = json!("video");
                timeline["continuity"]
                    .as_object_mut()
                    .unwrap()
                    .remove("source_id");
                timeline["continuity"]["source_video_id"] = json!(source);
                expected["D"]["inputs"]["timeline_data"] = json!(timeline.to_string());
                i["h3"]["continuity"]["sourceKind"] = json!("video");
                assert_eq!(compile(&i).unwrap(), expected);
                i["h3"]["continuity"]
                    .as_object_mut()
                    .unwrap()
                    .remove("sourceKind");
                assert_eq!(compile(&i).unwrap(), baseline);
            }
        }
    }
}

#[test]
fn h3_continuity_source_kind_and_video_ids_are_strict() {
    let mut i = h3();
    for source in [
        "",
        "clip_1",
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    ] {
        i["h3"]["continuity"] = json!({"session":"deas-ab12cd34","sourceId":source,"capture":true,"action":"","outputMode":"segment"});
        let baseline = compile(&i).unwrap();
        for kind in [
            json!(null),
            json!(false),
            json!(42),
            json!([]),
            json!({}),
            json!("checkpoint"),
            json!("Video"),
            json!(""),
        ] {
            i["h3"]["continuity"]["sourceKind"] = kind.clone();
            assert!(compile(&i).is_err(), "accepted sourceKind={kind}");
        }
        i["h3"]["continuity"]
            .as_object_mut()
            .unwrap()
            .remove("sourceKind");
        assert_eq!(compile(&i).unwrap(), baseline);
    }
    i["h3"]["continuity"]["sourceKind"] = json!("video");
    for source in [
        json!(""),
        json!("clip_1"),
        json!("a".repeat(63)),
        json!("a".repeat(65)),
        json!("A".repeat(64)),
        json!("g".repeat(64)),
        json!("é".repeat(32)),
        json!(null),
        json!(42),
        json!(false),
        json!([]),
        json!({}),
    ] {
        i["h3"]["continuity"]["sourceId"] = source.clone();
        assert!(compile(&i).is_err(), "accepted video sourceId={source}");
    }
}

#[test]
fn h3_continuity_rejects_reserved_session_only() {
    let mut i = h3();
    for source in ["", "clip_1", "_imports"] {
        i["h3"]["continuity"] = json!({"session":"_imports","sourceId":source,"capture":true,"action":"","outputMode":"segment"});
        assert!(validate(&i).is_err(), "accepted reserved session");
        assert!(compile(&i).is_err(), "compiled reserved session");
    }
    assert!(continuity_id("_imports"), "clip IDs are not session names");
}

#[test]
fn h3_continuity_identifier_contract_preserves_valid_graphs() {
    let mut i = h3();
    i["h3"]["continuity"] = json!({"session":"deas-ab12cd34","sourceId":"clip_1","capture":true,"action":"","outputMode":"segment"});
    let baseline = compile(&i).unwrap();
    let timeline: Value =
        serde_json::from_str(baseline["D"]["inputs"]["timeline_data"].as_str().unwrap()).unwrap();
    for id in [
        "a",
        "_",
        "-",
        "123",
        "My_session-42",
        "_imports2",
        "_IMPORTS",
        &"a".repeat(80),
    ] {
        for source in ["", "clip_1", "_imports", id] {
            i["h3"]["continuity"]["session"] = json!(id);
            i["h3"]["continuity"]["sourceId"] = json!(source);
            let mut expected = baseline.clone();
            let mut timeline = timeline.clone();
            timeline["continuity"]["session"] = json!(id);
            if source.is_empty() {
                for key in ["source_kind", "source_id", "continuation_prompt"] {
                    timeline["continuity"].as_object_mut().unwrap().remove(key);
                }
            } else {
                timeline["continuity"]["source_id"] = json!(source);
            }
            expected["D"]["inputs"]["timeline_data"] = json!(timeline.to_string());
            assert_eq!(
                compile(&i).unwrap(),
                expected,
                "session={id}, sourceId={source}"
            );
        }
    }
    for field in ["session", "sourceId"] {
        for bad in [
            "a/b",
            "a\\b",
            "..",
            "a.b",
            "a b",
            " a",
            "a ",
            "é",
            "a\n",
            "a\0",
            &"a".repeat(81),
        ] {
            i["h3"]["continuity"]["session"] = json!("deas-ab12cd34");
            i["h3"]["continuity"]["sourceId"] = json!("clip_1");
            i["h3"]["continuity"][field] = json!(bad);
            assert!(compile(&i).is_err(), "accepted {field}={bad:?}");
        }
    }
    i["h3"]["continuity"]["session"] = json!("");
    i["h3"]["continuity"]["sourceId"] = json!("");
    assert!(compile(&i).is_err(), "accepted empty session");
}

#[test]
fn h3_continuity_overlap_frames_maps_only_explicit_checkpoint_options() {
    let mut i = h3();
    i["h3"]["continuity"] = json!({"session":"deas-ab12cd34","sourceId":"clip_1","capture":true,"action":"she turns around","outputMode":"segment"});
    let omitted = compile(&i).unwrap();
    let timeline: Value =
        serde_json::from_str(omitted["D"]["inputs"]["timeline_data"].as_str().unwrap()).unwrap();
    assert_eq!(
        timeline["continuity"],
        json!({"version":3,"session":"deas-ab12cd34","source_kind":"checkpoint","source_id":"clip_1","capture":true,"output_mode":"segment","continuation_prompt":"she turns around"})
    );
    for frames in [73, 56, 39, 22, 5] {
        i["h3"]["continuity"]["overlapFrames"] = json!(frames);
        let mut expected = omitted.clone();
        let mut timeline = timeline.clone();
        timeline["continuity"]["overlap_frames"] = json!(frames);
        expected["D"]["inputs"]["timeline_data"] = json!(timeline.to_string());
        assert_eq!(compile(&i).unwrap(), expected, "overlapFrames={frames}");
    }
    i["h3"]["continuity"]
        .as_object_mut()
        .unwrap()
        .remove("overlapFrames");
    assert_eq!(compile(&i).unwrap(), omitted);

    i["h3"]["continuity"]["sourceId"] = json!("");
    let first_capture = compile(&i).unwrap();
    for frames in [73, 56, 39, 22, 5] {
        i["h3"]["continuity"]["overlapFrames"] = json!(frames);
        assert_eq!(
            compile(&i).unwrap(),
            first_capture,
            "first capture overlapFrames={frames}"
        );
    }
}

#[test]
fn h3_continuity_overlap_frames_rejects_malformed_values_and_unknown_keys() {
    let mut i = h3();
    for source in ["", "clip_1"] {
        i["h3"]["continuity"] = json!({"session":"deas-ab12cd34","sourceId":source,"capture":true,"action":"","outputMode":"segment"});
        for bad in [
            json!(null),
            json!(true),
            json!(false),
            json!("22"),
            json!(22.0),
            json!(22.5),
            json!(-1),
            json!(0),
            json!(4),
            json!(6),
            json!(21),
            json!(23),
            json!(38),
            json!(40),
            json!(55),
            json!(57),
            json!(72),
            json!(74),
            json!(u64::MAX),
            json!([]),
            json!({}),
        ] {
            i["h3"]["continuity"]["overlapFrames"] = bad.clone();
            assert!(
                compile(&i).is_err(),
                "accepted overlapFrames={bad}, sourceId={source}"
            );
        }
        i["h3"]["continuity"]["overlapFrames"] = json!(22);
        for key in ["unknown", "overlap_frames", "sourceKind"] {
            i["h3"]["continuity"][key] = json!(true);
            assert!(compile(&i).is_err(), "accepted continuity.{key}");
            i["h3"]["continuity"].as_object_mut().unwrap().remove(key);
        }
    }
}

#[test]
fn ltx_video_to_video_guides_the_trimmed_source_and_keyframes_at_their_frames() {
    let mut i = ltx();
    i["sourceVideo"] = json!("deas-input-1.mp4");
    i["ltx"]["sourceStrength"] = json!(0.8);
    i["ltx"]["keyframes"] = json!([{"image":"b.png","frame":48,"strength":0.6},{"image":"c.png [output]","frame":-1,"strength":1}]);
    let g = compile(&i).unwrap();
    assert_eq!(
        g["SV"],
        json!({"class_type":"LoadVideo","inputs":{"file":"deas-input-1.mp4"}})
    );
    assert_eq!(
        g["SVT"]["inputs"],
        json!({"image":["SVC",0],"batch_index":0,"length":121})
    );
    assert_eq!(g["SVS"]["inputs"]["crop"], "center");
    assert_eq!(
        g["SVP"]["inputs"],
        json!({"image":["SVS",0],"img_compression":18})
    );
    // Source first, then keyframes in order, each chained into the next guide.
    let guide = |n: &str| {
        (
            g[n]["inputs"]["image"].clone(),
            g[n]["inputs"]["frame_idx"].clone(),
            g[n]["inputs"]["strength"].clone(),
        )
    };
    assert_eq!(guide("G1"), (json!(["SVP", 0]), json!(0), json!(0.8)));
    assert_eq!(guide("G2"), (json!(["K1P", 0]), json!(48), json!(0.6)));
    assert_eq!(guide("G3"), (json!(["K2P", 0]), json!(-1), json!(1.0)));
    assert_eq!(g["G2"]["inputs"]["latent"], json!(["G1", 2]));
    assert_eq!(g["K2"]["inputs"]["image"], "c.png [output]");
    assert_eq!(g["CR"]["class_type"], "LTXVCropGuides");
    // Multipass: half-size copies for pass 1, full guides re-added in pass 2.
    i["ltx"]["passes"] = json!(2);
    i["ltx"]["spatialUpscaler"] = json!("up.safetensors");
    let g = compile(&i).unwrap();
    assert_eq!(g["G1"]["inputs"]["image"], json!(["SVH", 0]));
    assert_eq!(g["G1b"]["inputs"]["image"], json!(["SVP", 0]));
    assert_eq!(g["G3b"]["inputs"]["strength"], json!(1.0));
    let req = requirements(&i);
    assert_eq!(
        req.iter().find(|r| r.0 == "images").unwrap().1,
        vec!["b.png"]
    );
    for (path, value) in [
        ("sourceVideo", json!("sub/a.mp4")),
        ("sourceStrength", json!(1.5)),
        ("keyframes", json!([])),
        (
            "keyframes",
            json!([{"image":"b.png","frame":121,"strength":1}]),
        ),
        (
            "keyframes",
            json!([{"image":"../b.png","frame":0,"strength":1}]),
        ),
    ] {
        let mut bad = i.clone();
        if path == "sourceVideo" {
            bad[path] = value
        } else {
            bad["ltx"][path] = value
        }
        assert!(compile(&bad).is_err(), "accepted {path}");
    }
    let mut lone = ltx();
    lone["ltx"]["sourceStrength"] = json!(1);
    assert!(compile(&lone).is_err(), "strength without a source video");
    let mut foreign = h3();
    foreign["sourceVideo"] = json!("deas-input-1.mp4");
    assert!(compile(&foreign).is_err(), "sourceVideo is LTX only");
}

#[test]
fn h3_ref2va_puts_images_videos_and_audio_on_the_director_timeline() {
    let mut i = h3();
    i["model"] = json!("MiniMaxH3/ref2va_turbo.safetensors");
    i["h3"]["mode"] = json!("REF2VA");
    i["h3"]["references"] = json!([]);
    i["h3"]["media"] = json!([
        {"type":"image","file":"deas-inputs/a.png","slot":0},
        {"type":"video","file":"deas-input-v.mp4","slot":0,"mediaMode":"video_audio","trimStart":1,"trimEnd":6},
        {"type":"audio","file":"deas-inputs/s.wav","slot":1}
    ]);
    let g = compile(&i).unwrap();
    let d = &g["D"]["inputs"];
    assert_eq!(d["mode"], "REF2VA");
    assert_eq!(d["ref2va_model"], json!(["1", 0]));
    assert!(d.get("fl2va_model").is_none());
    let t: Value = serde_json::from_str(d["timeline_data"].as_str().unwrap()).unwrap();
    assert_eq!(
        t["items"],
        json!([
            {"id":"m0","type":"image","value":"deas-inputs/a.png","slot":0,"order":0,"enabled":true},
            {"id":"m1","type":"video","value":"deas-input-v.mp4","slot":0,"order":1,"enabled":true,"media_mode":"video_audio","trim_start":1,"trim_end":6},
            {"id":"m2","type":"audio","value":"deas-inputs/s.wav","slot":1,"order":2,"enabled":true}
        ])
    );
    let req = requirements(&i);
    assert_eq!(
        req.iter().find(|r| r.0 == "images").unwrap().1,
        vec!["deas-inputs/a.png"]
    );
    let item = |v: Value| json!([v]);
    for media in [
        json!([]),
        item(json!({"type":"audio","file":"deas-inputs/s.wav","slot":0})), // audio alone
        item(json!({"type":"image","file":"a.png","slot":0})),             // not uploaded
        item(json!({"type":"image","file":"deas-inputs/a.png","slot":9})), // lane has 0..8
        item(json!({"type":"image","file":"deas-inputs/a.png","slot":0,"trimStart":1})),
        item(json!({"type":"video","file":"deas-input-v.mp4","slot":0,"mediaMode":"loop"})),
        item(json!({"type":"video","file":"deas-input-v.mp4","slot":0,"trimStart":5,"trimEnd":2})),
        json!([{"type":"image","file":"deas-inputs/a.png","slot":0},{"type":"image","file":"deas-inputs/b.png","slot":0}]),
        json!((0..4)
            .map(|n| json!({"type":"video","file":"deas-input-v.mp4","slot":n}))
            .collect::<Vec<_>>()),
    ] {
        let mut bad = i.clone();
        bad["h3"]["media"] = media.clone();
        assert!(compile(&bad).is_err(), "accepted {media}");
    }
    let mut cont = i.clone();
    cont["h3"]["continuity"] =
        json!({"session":"s1","sourceId":"clip_1","capture":false,"action":"","outputMode":"full"});
    assert!(compile(&cont).is_ok(), "REF2VA continuation");
    let mut stray = h3();
    stray["h3"]["media"] = json!([{"type":"image","file":"deas-inputs/a.png","slot":0}]);
    assert!(compile(&stray).is_err(), "media outside REF2VA");
}

#[test]
fn h3_reference_crop_limits_match_director_stream_totals() {
    let mut intent = h3();
    intent["h3"]["mode"] = json!("REF2VA");
    intent["h3"]["references"] = json!([]);
    for end in [1.0, 15.1] {
        intent["h3"]["media"] =
            json!([{"type":"video","file":"deas-input-v.mp4","slot":0,"trimEnd":end}]);
        assert!(compile(&intent).is_err());
    }
    intent["h3"]["media"] = json!([
        {"type":"video","file":"deas-input-v.mp4","slot":0,"mediaMode":"video_audio","trimEnd":10},
        {"type":"audio","file":"deas-inputs/a.wav","slot":0,"trimEnd":6}
    ]);
    assert!(compile(&intent).is_err());
    intent["h3"]["media"][0]["mediaMode"] = json!("video");
    assert!(compile(&intent).is_ok());
    let mut media: Vec<Value> = (0..9)
        .map(|n| json!({"type":"image","file":"deas-inputs/a.png","slot":n}))
        .collect();
    media
        .push(json!({"type":"video","file":"deas-input-v.mp4","slot":0,"mediaMode":"video_audio"}));
    media.push(json!({"type":"audio","file":"deas-inputs/a.wav","slot":0}));
    intent["h3"]["media"] = json!(media);
    assert!(compile(&intent).is_ok());
    media.push(json!({"type":"audio","file":"deas-inputs/b.wav","slot":1}));
    intent["h3"]["media"] = json!(media);
    assert!(compile(&intent).is_err());
}

/// `cargo test --lib dump_video_graphs -- --ignored --nocapture` prints the three graphs for an offline
/// `execution.validate_prompt` run against the real ComfyUI checkout (see the deas-development skill).
#[test]
#[ignore]
fn dump_video_graphs() {
    let mut l = ltx();
    l["firstFrame"] = json!("ref.png");
    l["ltx"]["passes"] = json!(2);
    l["ltx"]["spatialUpscaler"] = json!("ltx-2.3-spatial-upscaler-x2-1.1.safetensors");
    let mut w = wan();
    w["lastFrame"] = json!("ref.png");
    w["chunks"] = json!(2);
    let mut v2v = ltx();
    v2v["sourceVideo"] = json!("source.mp4");
    v2v["ltx"]["sourceStrength"] = json!(0.8);
    v2v["ltx"]["keyframes"] = json!([{"image":"ref.png","frame":48,"strength":0.6}]);
    let mut r = h3();
    r["h3"]["mode"] = json!("REF2VA");
    r["h3"]["references"] = json!([]);
    r["h3"]["media"] = json!([{"type":"image","file":"deas-inputs/a.png","slot":0},{"type":"video","file":"deas-input-source.mp4","slot":0,"mediaMode":"video"}]);
    println!(
        "{}",
        json!({"ltx":compile(&l).unwrap(),"wan":compile(&w).unwrap(),"h3":compile(&h3()).unwrap(),"ltx_v2v":compile(&v2v).unwrap(),"h3_ref2va":compile(&r).unwrap()})
    );
}

#[test]
#[ignore]
fn dump_tve_graphs() {
    // Real file names of the owner's ComfyUI checkout, so validate_video_graphs.py checks the combos too.
    let post = json!({"upscaleModel":"2x-AnimeSharpV4_RCAN.safetensors",
        "rtx":{"denoise":"High","deblur":"Low","upscale":{"mode":"VSR","quality":"Ultra","scale":2}},
        "frameGeneration":{"engine":"rife","multiplier":2,"model":"rife_v4.26.safetensors"}});
    let mut rtx_fg = post.clone();
    rtx_fg["frameGeneration"] = json!({"engine":"rtx","multiplier":3,"mode":"HIGH"});
    let mut graphs = json!({});
    for (name, mut i) in [("ltx", ltx()), ("wan", wan()), ("h3", h3())] {
        i["videoPost"] = post.clone();
        graphs[format!("{name}_tve_rife")] = compile(&i).unwrap();
        i["videoPost"] = rtx_fg.clone();
        graphs[format!("{name}_tve_rtx")] = compile(&i).unwrap();
    }
    let enhance = json!({"recipeId":"video-enhance","version":1,"sourceVideo":"source.mp4","frameRate":24,"seed":7,"videoPost":post});
    graphs["enhance"] = compile(&enhance).unwrap();
    println!("{graphs}");
}

#[test]
#[ignore]
fn dump_latent_upscale_graphs() {
    let mut graphs = json!({});
    for passes in 1..=3 {
        let mut i = ltx();
        i["ltx"]["passes"] = json!(passes);
        if passes > 1 {
            i["ltx"]["spatialUpscaler"] = json!("ltx-2.3-spatial-upscaler-x2-1.1.safetensors");
        }
        i["latentUpscale"] =
            json!({"model":"ltx-2.3-spatial-upscaler-x2-1.1.safetensors","scale":2});
        graphs[format!("ltx_post_{passes}")] = compile(&i).unwrap();
    }
    for capture in [false, true] {
        let mut i = h3();
        i["h3"]["continuity"] = json!({"session":"deas","sourceId":"","action":"","capture":capture,"outputMode":"full"});
        i["latentUpscale"] = json!({"model":"minimax_h3_latent_upscaler_3d_conv_v1_bf16.safetensors","scale":2,"steps":1,"denoise":0.2});
        graphs[format!("h3_post_capture_{capture}")] = compile(&i).unwrap();
    }
    println!("{graphs}");
}

#[test]
fn h3_ref2va_refmods_reach_the_timeline_and_may_stand_alone() {
    let mut i = h3();
    i["h3"]["mode"] = json!("REF2VA");
    i["h3"]["references"] = json!([]);
    // RefMods alone (no timeline media) are a valid REF2VA set; with them, audio no longer needs a picture.
    i["h3"]["media"] = json!([{"type":"audio","file":"deas-inputs/s.wav","slot":0}]);
    i["h3"]["refmods"] = json!([{"slot":1,"name":"chars/hero","strength":0.8,"description":"the hero"},{"slot":3,"name":"place","strength":1}]);
    let g = compile(&i).unwrap();
    let t: Value =
        serde_json::from_str(g["D"]["inputs"]["timeline_data"].as_str().unwrap()).unwrap();
    assert_eq!(
        t["refmods"],
        json!([{"slot":1,"name":"chars/hero","enabled":true,"strength":0.8,"description":"the hero"},{"slot":3,"name":"place","enabled":true,"strength":1,"description":""}])
    );
    i["h3"]["media"] = json!([]);
    assert!(compile(&i).is_ok());
    for rows in [
        json!([]),
        json!([{"slot":0,"name":"a","strength":1}]),
        json!([{"slot":1,"name":"a","strength":1},{"slot":1,"name":"b","strength":1}]),
        json!([{"slot":1,"name":"../a","strength":1}]),
        json!([{"slot":1,"name":"a","strength":0}]),
        json!([{"slot":1,"name":"a","strength":1.5}]),
        json!([{"slot":1,"name":"a","strength":1,"enabled":false}]),
        json!((1..=9)
            .map(|n| json!({"slot":n,"name":"a","strength":1}))
            .collect::<Vec<_>>()),
    ] {
        i["h3"]["refmods"] = rows;
        assert!(compile(&i).is_err());
    }
    // RefMods are REF2VA-only; without them an empty media list stays invalid.
    let mut other = h3();
    other["h3"]["refmods"] = json!([{"slot":1,"name":"a","strength":1}]);
    assert!(compile(&other).is_err());
    i["h3"].as_object_mut().unwrap().remove("refmods");
    assert!(compile(&i).is_err());
}

#[test]
fn h3_ref2va_first_take_keeps_media_requirements_even_when_capture_is_enabled() {
    let mut i = h3();
    i["h3"]["mode"] = json!("REF2VA");
    i["h3"]["references"] = json!([]);
    i["h3"]["media"] = json!([{"type":"image","file":"deas-inputs/a.png","slot":0}]);
    for capture in [false, true] {
        for flag in [None, Some(false), Some(true)] {
            i["h3"]["continuity"] = json!({"session":"s1","sourceId":"","capture":capture,"action":"unused","outputMode":"segment","overlapFrames":22});
            let mut expected =
                json!({"version":3,"session":"s1","capture":capture,"output_mode":"segment"});
            if let Some(flag) = flag {
                i["h3"]["continuity"]["useReferences"] = json!(flag);
                expected["use_references"] = json!(flag);
            }
            let g = compile(&i).unwrap();
            let t: Value =
                serde_json::from_str(g["D"]["inputs"]["timeline_data"].as_str().unwrap()).unwrap();
            assert_eq!(t["continuity"], expected);
            assert_eq!(t["items"][0]["value"], "deas-inputs/a.png");
            assert_eq!(
                requirements(&i).iter().find(|r| r.0 == "images").unwrap().1,
                vec!["deas-inputs/a.png"]
            );
            let mut empty = i.clone();
            empty["h3"]["media"] = json!([]);
            assert!(
                compile(&empty).is_err(),
                "capture is not a continuation source"
            );
        }
    }
    i["h3"].as_object_mut().unwrap().remove("continuity");
    i["h3"]["media"] = json!([]);
    assert!(compile(&i).is_err());
}

#[test]
fn h3_ref2va_continuation_retains_strict_flags_media_and_source_validation() {
    let mut i = h3();
    i["h3"]["mode"] = json!("REF2VA");
    i["h3"]["references"] = json!([]);
    i["h3"]["media"] = json!([]);
    i["h3"]["continuity"] = json!({"session":"s1","sourceId":"clip_1","capture":false,"action":"","outputMode":"full","useReferences":true});
    assert!(compile(&i).is_err(), "useReferences true needs references");
    i["h3"]["refmods"] = json!([{"slot":1,"name":"chars/hero","strength":1}]);
    assert!(compile(&i).is_ok(), "RefMods alone are valid references");
    i["h3"].as_object_mut().unwrap().remove("refmods");
    for flag in [
        json!(null),
        json!(0),
        json!(1),
        json!("false"),
        json!("true"),
        json!([]),
        json!({}),
    ] {
        i["h3"]["continuity"]["useReferences"] = flag.clone();
        assert!(compile(&i).is_err(), "accepted useReferences={flag}");
    }
    i["h3"]["continuity"]["useReferences"] = json!(false);
    assert!(compile(&i).is_ok());
    for media in [
        json!(null),
        json!({}),
        json!([{"type":"image","file":"../a.png","slot":0}]),
        json!([{"type":"image","file":"deas-inputs/a.png","slot":9}]),
        json!([{"type":"audio","file":"deas-inputs/a.wav","slot":0}]),
        json!([{"type":"video","file":"deas-input-v.mp4","slot":0,"trimStart":6,"trimEnd":1}]),
        json!([{"type":"video","file":"deas-input-v.mp4","slot":0,"mediaMode":"loop"}]),
        json!([{"type":"image","file":"deas-inputs/a.png","slot":0,"extra":true}]),
    ] {
        i["h3"]["media"] = media.clone();
        assert!(
            compile(&i).is_err(),
            "accepted ignored malformed media={media}"
        );
    }
    i["h3"]["media"] = json!([]);
    i["h3"]["refmods"] = json!([{"slot":1,"name":"../hero","strength":1}]);
    assert!(compile(&i).is_err(), "ignored RefMods still validate");
    i["h3"].as_object_mut().unwrap().remove("refmods");
    for source in [json!(null), json!(1), json!("../clip"), json!("")] {
        i["h3"]["continuity"]["sourceId"] = source.clone();
        assert!(compile(&i).is_err(), "accepted empty-media source={source}");
    }
    i["h3"]["continuity"]
        .as_object_mut()
        .unwrap()
        .remove("sourceId");
    assert!(compile(&i).is_err());
    i["h3"]["continuity"]["sourceId"] = json!("clip_1");
    i["h3"]["continuity"]["sourceKind"] = json!("video");
    assert!(compile(&i).is_err(), "imported video IDs stay strict");
}

#[test]
fn h3_ref2va_continuation_requires_only_effective_reference_images() {
    let mut i = h3();
    i["h3"]["mode"] = json!("REF2VA");
    i["h3"]["references"] = json!([]);
    i["h3"]["media"] = json!([{"type":"image","file":"deas-inputs/stale.png","slot":0}]);
    i["h3"]["refmods"] = json!([{"slot":1,"name":"chars/stale","strength":1}]);
    i["h3"]["continuity"] =
        json!({"session":"s1","sourceId":"clip_1","capture":false,"action":"","outputMode":"full"});
    for flag in [None, Some(false), Some(true)] {
        if let Some(flag) = flag {
            i["h3"]["continuity"]["useReferences"] = json!(flag);
        }
        let g = compile(&i).unwrap();
        let t: Value =
            serde_json::from_str(g["D"]["inputs"]["timeline_data"].as_str().unwrap()).unwrap();
        // Preserve valid descriptors in the timeline; Director decides whether to consume them.
        assert_eq!(t["items"][0]["value"], "deas-inputs/stale.png");
        assert_eq!(t["refmods"][0]["name"], "chars/stale");
        let req = requirements(&i);
        assert_eq!(
            req.iter().find(|r| r.0 == "images").unwrap().1,
            if flag == Some(true) {
                vec!["deas-inputs/stale.png"]
            } else {
                vec![]
            }
        );
    }
}

#[test]
fn h3_ref2va_continuation_maps_reference_choice_and_preserves_segment_chain() {
    for video in [false, true] {
        for capture in [false, true] {
            for output in ["full", "segment"] {
                for flag in [None, Some(false), Some(true)] {
                    let mut i = h3();
                    i["h3"]["mode"] = json!("REF2VA");
                    i["h3"]["references"] = json!([]);
                    i["h3"]["media"] = json!([]);
                    i["h3"]["continuity"] = json!({"session":"s1","sourceId":"clip_1","capture":capture,"action":"  she turns around\n","outputMode":output,"overlapFrames":22});
                    if video {
                        i["h3"]["continuity"]["sourceKind"] = json!("video");
                        i["h3"]["continuity"]["sourceId"] = json!("a".repeat(64));
                    }
                    if let Some(flag) = flag {
                        i["h3"]["continuity"]["useReferences"] = json!(flag);
                    }
                    if flag == Some(true) {
                        i["h3"]["media"] = json!([
                            {"type":"image","file":"deas-inputs/a.png","slot":0},
                            {"type":"video","file":"deas-input-v.mp4","slot":0,"mediaMode":"video_audio","trimStart":1,"trimEnd":6}
                        ]);
                        i["h3"]["refmods"] = json!([{"slot":1,"name":"chars/hero","strength":0.8,"description":"the hero"}]);
                    }
                    let g = compile(&i).unwrap();
                    let t: Value =
                        serde_json::from_str(g["D"]["inputs"]["timeline_data"].as_str().unwrap())
                            .unwrap();
                    let mut expected = json!({"version":3,"session":"s1","capture":capture,"output_mode":output,"continuation_prompt":"  she turns around\n","overlap_frames":22});
                    expected["source_kind"] = json!(if video { "video" } else { "checkpoint" });
                    expected[if video {
                        "source_video_id"
                    } else {
                        "source_id"
                    }] = i["h3"]["continuity"]["sourceId"].clone();
                    if let Some(flag) = flag {
                        expected["use_references"] = json!(flag);
                    }
                    assert_eq!(t["continuity"], expected);
                    if flag == Some(true) {
                        assert_eq!(
                            t["items"][1],
                            json!({"id":"m1","type":"video","value":"deas-input-v.mp4","slot":0,"order":1,"enabled":true,"media_mode":"video_audio","trim_start":1,"trim_end":6})
                        );
                        assert_eq!(
                            t["refmods"],
                            json!([{"slot":1,"name":"chars/hero","enabled":true,"strength":0.8,"description":"the hero"}])
                        );
                    } else {
                        assert_eq!(t["items"], json!([]));
                    }
                    assert_eq!(g["D"]["inputs"]["ref2va_model"], json!(["1", 0]));
                    assert!(g["D"]["inputs"].get("fl2va_model").is_none());
                    assert_eq!(
                        g["SP"]["inputs"],
                        json!({"sampled":["5",0],"context":["G",2]})
                    );
                    assert_eq!(
                        g["SD"]["inputs"],
                        json!({"latent":["SP",0],"vae":["V",0],"audio_vae":["AV",0],"context":["SP",1]})
                    );
                    assert_eq!(g["7"]["inputs"]["images"], json!(["SD", 0]));
                    assert_eq!(g["7"]["inputs"]["audio"], json!(["SD", 1]));
                    assert_eq!(
                        g["P"]["inputs"],
                        json!({"filename":["7",1],"ticket":["SP",2]})
                    );
                }
            }
        }
    }
}
