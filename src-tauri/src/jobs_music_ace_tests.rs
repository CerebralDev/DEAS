use super::*;

fn intent(family: &str, split: bool) -> Value {
    let p = profile(family).unwrap();
    let duration = if p["version"] == "1.0" {
        1292.0 * 4096.0 / 44100.0
    } else {
        120.0
    };
    let mut i = json!({"recipeId":"music-generation","version":1,"family":family,"mode":"t2m","planning":"off",
        "checkpoint":"ace.safetensors","style":"English, indie pop","lyrics":"[Verse]\nHello world","seed":42,"maxDuration":duration,
        "steps":p["steps"],"cfg":p["cfg"],"sampler":p["sampler"],"scheduler":p["scheduler"],"outputPrefix":"raw/ACE",
        "ace":{"shift":p["shift"],"lyricsStrength":1,"bpm":120,"timeSignature":"4","language":"en","keyScale":"C major","generateAudioCodes":true,"lmCfg":2,"temperature":0.85,"topP":0.9,"topK":0,"minP":0}});
    if split {
        i["diffusion"] = json!({"textEncoder":"qwen_0.6b_ace15.safetensors","textEncoder2":if family.contains("xl") {"qwen_4b_ace15.safetensors"} else {"qwen_1.7b_ace15.safetensors"},"vae":"ace_1.5_vae.safetensors"});
    }
    i
}
#[test]
fn ace_graphs_preserve_explicit_controls_and_loader_forms() {
    for (family, steps, cfg, shift) in [
        ("ace-step-1-0", 50, 5, 5),
        ("ace-step-1-5-base", 50, 7, 1),
        ("ace-step-1-5-sft", 50, 7, 1),
        ("ace-step-1-5-turbo", 8, 1, 3),
        ("ace-step-1-5-xl-base", 50, 6, 3),
        ("ace-step-1-5-xl-sft", 50, 7, 3),
        ("ace-step-1-5-xl-turbo", 8, 1, 3),
    ] {
        for split in [false, true] {
            if split && family == "ace-step-1-0" {
                continue;
            }
            let mut i = intent(family, split);
            assert_eq!(i["steps"], steps);
            assert_eq!(i["cfg"], cfg);
            assert_eq!(i["ace"]["shift"], shift);
            i["ace"]["shift"] = json!(2.75);
            i["cfg"] = json!(4.25);
            i["steps"] = json!(17);
            let g = crate::jobs::compile(&i).unwrap();
            assert_eq!(g["S"]["inputs"]["cfg"], 4.25);
            assert_eq!(g["S"]["inputs"]["steps"], 17);
            assert_eq!(g["P"]["inputs"]["shift"], 2.75);
            assert_eq!(g["L"]["inputs"]["seconds"], i["maxDuration"]);
            assert_eq!(g["7"]["inputs"]["format"], "flac");
            assert_eq!(g["7"]["inputs"]["audio"], json!(["D", 0]));
            let serialized = g.to_string();
            assert!(!serialized.contains("YuE2"));
            assert!(!serialized.contains("SheetSage"));
            assert!(!serialized.contains("SaveImage"));
            let r = crate::jobs_music::requirements(&i);
            if split {
                assert_eq!(g["1"]["class_type"], "UNETLoader");
                assert_eq!(g["E"]["inputs"]["type"], "ace");
                assert_eq!(g["M"]["inputs"]["clip"], json!(["E", 0]));
                assert_eq!(g["D"]["inputs"]["vae"], json!(["V", 0]));
                assert!(!serialized.contains("CheckpointLoaderSimple"));
                assert!(!r.iter().any(|(k, _)| *k == "checkpoints"));
                assert_eq!(
                    r.iter()
                        .find(|(k, _)| *k == "dualTextEncoders")
                        .unwrap()
                        .1
                        .len(),
                    2
                );
            } else {
                assert_eq!(r, vec![("checkpoints", vec!["ace.safetensors"])]);
            }
            if family == "ace-step-1-0" {
                assert_eq!(g["P"]["class_type"], "ModelSamplingSD3");
                assert_eq!(g["T"]["inputs"]["multiplier"], 1.0);
                assert_eq!(g["C"]["inputs"]["model"], json!(["P", 0]));
                assert_eq!(g["S"]["inputs"]["model"], json!(["C", 0]));
                assert_eq!(g["M"]["inputs"]["lyrics_strength"], 1);
                assert!(g["M"]["inputs"].get("cfg_scale").is_none());
            } else {
                assert_eq!(g["P"]["class_type"], "ModelSamplingAuraFlow");
                assert!(g.get("C").is_none());
                assert_eq!(g["M"]["inputs"]["cfg_scale"], 2);
                assert_eq!(g["M"]["inputs"]["duration"], 120.0);
                assert_eq!(g["M"]["inputs"]["timesignature"], "4");
                assert_eq!(g["M"]["inputs"]["generate_audio_codes"], true);
            }
        }
    }
}
#[test]
fn ace_trust_boundary_rejects_cross_recipe_unknown_and_unsafe_values() {
    let base = intent("ace-step-1-5-turbo", false);
    for (key, value) in [
        ("family", json!("ace-step")),
        ("family", json!("ace-step-1-5")),
        ("family", json!("ace-step-1-5-xl")),
        ("family", json!("yue2")),
        ("mode", json!("cover")),
        ("planning", json!("full")),
        ("generation", json!({})),
        ("abc", json!({})),
        ("sourceAudio", json!("x.flac")),
        ("audioEncoder", json!("x.safetensors")),
        ("loras", json!([])),
        ("options", json!({})),
        ("diffusion", Value::Null),
        ("checkpoint", json!("../escape")),
        ("checkpoint", json!("model.gguf")),
        ("outputPrefix", json!("../x")),
        ("style", json!(" ")),
        ("lyrics", json!("\0")),
        ("seed", json!(9007199254740992_u64)),
        ("seed", json!(1.5)),
        ("steps", json!(0)),
        ("cfg", json!(101)),
        ("maxDuration", json!(120.01)),
        ("maxDuration", json!(1000.04)),
        ("maxDuration", json!(0.96)),
    ] {
        let mut i = base.clone();
        i[key] = value;
        assert!(crate::jobs::compile(&i).is_err(), "accepted {key}: {i}");
    }
    for key in base.as_object().unwrap().keys() {
        let mut i = base.clone();
        i.as_object_mut().unwrap().remove(key);
        assert!(compile(&i).is_err(), "missing {key}");
    }
    for (key, b) in DATA["bounds"].as_object().unwrap() {
        for value in [
            json!(b[0].as_f64().unwrap() - 0.01),
            json!(b[1].as_f64().unwrap() + 1.0),
            Value::Null,
            json!("1"),
        ] {
            let mut i = base.clone();
            i["ace"][key] = value;
            assert!(compile(&i).is_err(), "accepted {key}: {i}");
        }
        if b[2] == true {
            let mut i = base.clone();
            i["ace"][key] = json!(10.5);
            assert!(compile(&i).is_err());
        }
    }
    for (key, value) in [
        ("language", json!("xx")),
        ("keyScale", json!("C")),
        ("timeSignature", json!("5")),
        ("generateAudioCodes", json!(1)),
        ("unknown", json!(true)),
    ] {
        let mut i = base.clone();
        i["ace"][key] = value;
        assert!(compile(&i).is_err());
    }
    let mut i = intent("ace-step-1-0", true);
    assert!(compile(&i).is_err());
    i = intent("ace-step-1-5-turbo", true);
    for key in ["textEncoder", "textEncoder2", "vae"] {
        let mut bad = i.clone();
        bad["diffusion"][key] = json!("../escape");
        assert!(compile(&bad).is_err());
    }
    i["diffusion"]["unknown"] = json!(1);
    assert!(compile(&i).is_err());
    let mut i = base;
    i["audioEnhance"] = json!({"mode":"clean","profile":"mix"});
    assert!(
        crate::jobs::compile(&i).is_ok(),
        "existing audio wrapper remains supported"
    );
}
#[test]
fn ace_original_duration_never_silently_loses_a_frame() {
    let mut i = intent("ace-step-1-0", false);
    i["maxDuration"] = json!(120.0);
    assert!(compile(&i).is_err());
    for frames in 11..=10766 {
        let mut duration = frames as f64 * 4096.0 / 44100.0;
        if (duration * 44100.0 / 512.0 / 8.0).floor() != frames as f64 {
            duration += f64::EPSILON * duration;
        }
        i["maxDuration"] = json!(duration);
        assert!(validate(&i).is_ok(), "frames {frames}: {duration}");
        assert_eq!((duration * 44100.0 / 512.0 / 8.0).floor(), frames as f64);
    }
}
/// DEAS_ACE_GRAPH_DUMP=/path/in/scratch/ace-graphs.json cargo test --lib ace_dump_graphs -- --ignored
#[test]
#[ignore]
fn ace_dump_graphs() {
    let mut graphs = serde_json::Map::new();
    for family in [
        "ace-step-1-0",
        "ace-step-1-5-base",
        "ace-step-1-5-sft",
        "ace-step-1-5-turbo",
        "ace-step-1-5-xl-base",
        "ace-step-1-5-xl-sft",
        "ace-step-1-5-xl-turbo",
    ] {
        for split in [false, true] {
            if split && family == "ace-step-1-0" {
                continue;
            }
            graphs.insert(
                format!("{family}-{}", if split { "split" } else { "aio" }),
                crate::jobs::compile(&intent(family, split)).unwrap(),
            );
        }
    }
    std::fs::write(
        std::env::var("DEAS_ACE_GRAPH_DUMP").unwrap(),
        serde_json::to_vec_pretty(&graphs).unwrap(),
    )
    .unwrap();
}
