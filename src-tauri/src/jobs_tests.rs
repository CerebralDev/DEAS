use super::*;
#[path = "jobs_cancel_tests.rs"]
mod cancel_tests;
#[path = "jobs_model_tests.rs"]
mod model_tests;
#[path = "jobs_observer_tests.rs"]
mod observer_tests;

#[tokio::test]
async fn watch_completion_sends_close_frame() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        socket
            .send(Message::Text(
                json!({"type":"execution_start","data":{"prompt_id":PROMPT_ID}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        socket
            .send(Message::Text(
                json!({"type":"execution_success","data":{"prompt_id":PROMPT_ID}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        assert!(
            matches!(socket.next().await, Some(Ok(Message::Close(_)))),
            "completion must send a WebSocket Close, not reset TCP"
        );
        socket.flush().await.unwrap();
    });
    let live: Arc<Mutex<Live>> = Arc::default();
    let completed = live.lock().unwrap().observer.completion();
    watch(&base, "test-client", live.clone()).await;
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
    observer_tests::finished(completed).await;
}

#[tokio::test]
async fn watch_ignores_foreign_completion_and_progress_but_answers_pings() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let live: Arc<Mutex<Live>> = Arc::default();
    let observed = live.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        for event in [
            json!({"type":"execution_interrupted","data":{"prompt_id":"foreign"}}),
            json!({"type":"execution_start","data":{"prompt_id":PROMPT_ID}}),
            json!({"type":"execution_start","data":{"prompt_id":"foreign"}}),
            json!({"type":"progress","data":{"value":1,"max":10}}),
            json!({"type":"progress","data":{"prompt_id":PROMPT_ID,"value":2,"max":10}}),
            json!({"type":"progress","data":{"prompt_id":"foreign","value":99,"max":100}}),
            json!({"type":"execution_success","data":{"prompt_id":"foreign"}}),
            json!({"type":"execution_error","data":{"prompt_id":"foreign"}}),
            json!({"type":"execution_interrupted","data":{"prompt_id":"foreign"}}),
            json!({"type":"execution_success","data":{}}),
        ] {
            socket
                .send(Message::Text(event.to_string().into()))
                .await
                .unwrap();
        }
        socket
            .send(Message::Binary(
                [0, 0, 0, 1, 0, 0, 0, 1, 0xff, 0xd8, 0xff].to_vec().into(),
            ))
            .await
            .unwrap();
        socket
            .send(Message::Ping(b"heartbeat".to_vec().into()))
            .await
            .unwrap();
        assert_eq!(
            socket.next().await.unwrap().unwrap(),
            Message::Pong(b"heartbeat".to_vec().into())
        );
        {
            let live = observed.lock().unwrap();
            assert_eq!((live.value, live.max), (2, 10));
            assert!(live
                .preview
                .as_ref()
                .unwrap()
                .starts_with("data:image/jpeg;base64,"));
        }
        socket
            .send(Message::Text(
                json!({"type":"execution_success","data":{"prompt_id":PROMPT_ID}})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        assert!(matches!(socket.next().await, Some(Ok(Message::Close(_)))));
        socket.flush().await.unwrap();
    });
    let completed = live.lock().unwrap().observer.completion();
    watch(&base, "test-client", live.clone()).await;
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
    observer_tests::finished(completed).await;
}

#[tokio::test]
async fn watch_close_wait_is_bounded_without_holding_live_mutex() {
    use futures_util::{SinkExt, StreamExt};
    use tokio::io::AsyncReadExt;
    use tokio_tungstenite::tungstenite::Message;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let live: Arc<Mutex<Live>> = Arc::default();
    let observed = live.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        for kind in ["execution_start", "execution_error"] {
            socket
                .send(Message::Text(
                    json!({"type":kind,"data":{"prompt_id":PROMPT_ID}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        }
        assert!(matches!(socket.next().await, Some(Ok(Message::Close(_)))));
        assert!(
            observed.try_lock().is_ok(),
            "close wait must not hold the live-view mutex"
        );
        // Deliberately do not flush the automatic close reply; read TCP directly.
        let mut byte = [0];
        assert!(
            tokio::time::timeout(Duration::from_millis(50), socket.get_mut().read(&mut byte))
                .await
                .is_err(),
            "client should wait for the peer close reply"
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), socket.get_mut().read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0
        );
    });
    let completed = live.lock().unwrap().observer.completion();
    watch(&base, "test-client", live.clone()).await;
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
    observer_tests::finished(completed).await;
}

#[tokio::test]
async fn watch_answers_peer_close() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        socket.send(Message::Close(None)).await.unwrap();
        assert!(matches!(socket.next().await, Some(Ok(Message::Close(_)))));
    });
    let live: Arc<Mutex<Live>> = Arc::default();
    let completed = live.lock().unwrap().observer.completion();
    watch(&base, "test-client", live.clone()).await;
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
    observer_tests::finished(completed).await;
}

fn prepared_video() -> Value {
    json!({"kind":"video","filename":"folder/日本 clip.mp4","clip_id":"a".repeat(64),
        "sha256":"b".repeat(64),"width":1920,"height":1080,"fps":29.97,"seconds":12.5,
        "has_audio":true,"prompt":"secret-prompt","output_path":"secret-host-path","frames":999})
}

#[tokio::test]
async fn h3_video_prepare_posts_only_filename_and_returns_sanitized_source() {
    let (base, task) = server(|path, body| {
        assert_eq!(path, "POST /comfy/df_h3_continuity/video HTTP/1.1");
        assert_eq!(body, json!({"filename":"folder/日本 clip.mp4"}));
        (200, prepared_video().to_string())
    })
    .await;
    let value = native_h3_video_prepare(base, "folder/日本 clip.mp4".into())
        .await
        .unwrap();
    assert_eq!(
        value,
        json!({"sourceId":"a".repeat(64),"filename":"folder/日本 clip.mp4",
        "sha256":"b".repeat(64),"width":1920,"height":1080,"fps":29.97,"seconds":12.5,"hasAudio":true})
    );
    task.abort();
}

#[tokio::test]
async fn h3_session_imported_video_metadata_identifies_backend_parent_without_leaks() {
    let imported = json!({"clip_id":"backend-chosen-parent","frames":124,"width":640,"height":352,
        "mode_family":"fl2va","provenance":{"kind":"imported_video","source_video_id":"a".repeat(64),
        "filename":"secret-input.mp4","sha256":"b".repeat(64)},"output_path":"secret-host"});
    let mut entries = vec![
        imported.clone(),
        json!({"clip_id":"extension","parent_id":"backend-chosen-parent","frames":243}),
    ];
    for (key, value) in [
        ("source_video_id", json!("A".repeat(64))),
        ("kind", json!("generated")),
    ] {
        let mut bad = imported.clone();
        bad["provenance"][key] = value;
        entries.push(bad);
    }
    for (key, value) in [
        ("width", json!(0)),
        ("height", json!(65537)),
        ("mode_family", json!("T2VA")),
    ] {
        let mut bad = imported.clone();
        bad[key] = value;
        entries.push(bad);
    }
    let mut nested = imported.clone();
    nested.as_object_mut().unwrap().remove("provenance");
    nested["metadata"] = json!({"provenance":imported["provenance"]});
    entries.push(nested);
    let mut refmode = imported;
    refmode["mode_family"] = json!("ref2va");
    entries.push(refmode);
    let (base, task) = server(move |_, _| (200, json!({"clips":entries}).to_string())).await;
    let value = native_h3_session(base, "imports".into()).await.unwrap();
    assert_eq!(
        value["clips"][0]["importedVideo"],
        json!({"sourceId":"a".repeat(64),"width":640,"height":352,"modeFamily":"fl2va"})
    );
    assert_eq!(value["clips"][1]["parent"], "backend-chosen-parent");
    for clip in &value["clips"].as_array().unwrap()[1..8] {
        assert!(clip.get("importedVideo").is_none());
        assert_eq!(clip.as_object().unwrap().len(), 5);
    }
    assert_eq!(value["clips"][8]["importedVideo"]["modeFamily"], "ref2va");
    assert!(!value.to_string().contains("secret-"));
    task.abort();
}

#[tokio::test]
async fn h3_video_prepare_rejects_unsafe_inputs_before_network() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    let mut invalid: Vec<String> = [
        "",
        "/abs.mp4",
        "C:/video.mp4",
        "C:video.mp4",
        "a\\b.mp4",
        "//server/video",
        "../a.mp4",
        "a/../b.mp4",
        "a/./b.mp4",
        "a//b.mp4",
        "a/",
        "a\0.mp4",
        "a\n.mp4",
        "a\u{7f}.mp4",
        "a\u{85}.mp4",
        "a.mp4 [output]",
        "a.mp4 [temp]",
        "a.mp4 [output] ",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    invalid.push("a".repeat(4097));
    for filename in invalid {
        assert_eq!(
            native_h3_video_prepare(base.clone(), filename)
                .await
                .unwrap_err(),
            "Invalid input-relative video filename"
        );
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
    assert_eq!(
        native_h3_video_prepare(
            "http://secret-user:secret-pass@private.invalid/?secret-token".into(),
            "video.mp4".into()
        )
        .await
        .unwrap_err(),
        "Backend URL must not contain credentials, a query, or a fragment"
    );
}

#[tokio::test]
async fn h3_video_prepare_rejects_tampered_fields_http_redirects_and_oversize_with_safe_logs() {
    for key in [
        "filename",
        "kind",
        "clip_id",
        "sha256",
        "width",
        "height",
        "fps",
        "seconds",
        "has_audio",
    ] {
        let mut missing = prepared_video();
        missing.as_object_mut().unwrap().remove(key);
        let mut cases = vec![missing];
        let invalid = match key {
            "filename" => vec![json!("secret-other.mp4")],
            "kind" => vec![json!("image")],
            "clip_id" | "sha256" => vec![
                json!("a".repeat(63)),
                json!("a".repeat(65)),
                json!("A".repeat(64)),
                json!("g".repeat(64)),
            ],
            "width" | "height" => {
                vec![json!(0), json!(-1), json!(65537), json!(1.5), json!("1920")]
            }
            "fps" | "seconds" => vec![json!(0), json!(-1), json!("NaN"), Value::Null],
            _ => vec![json!(1), json!("true")],
        };
        for bad in invalid {
            let mut v = prepared_video();
            v[key] = bad;
            cases.push(v);
        }
        for v in cases {
            let (base, task) = server(move |_, _| (200, v.to_string())).await;
            assert_eq!(
                native_h3_video_prepare(base, "folder/日本 clip.mp4".into())
                    .await
                    .unwrap_err(),
                "Malformed video preparation response",
                "{key}"
            );
            task.abort();
        }
    }
    let mut bodies = vec![
        (302, "{}".into()),
        (307, "{}".into()),
        (500, json!({"error":"secret-body secret-token"}).to_string()),
        (200, "secret-invalid-json".into()),
        (200, "x".repeat(RESPONSE_LIMIT + 1)),
    ];
    for literal in ["NaN", "Infinity", "1e999"] {
        bodies.push((200, prepared_video().to_string().replace("29.97", literal)));
    }
    for (status, body) in bodies {
        let (base, task) = server(move |_, _| (status, body.clone())).await;
        let error = native_h3_video_prepare(base, "folder/日本 clip.mp4".into())
            .await
            .unwrap_err();
        assert!(
            !error.contains("secret-") && !error.contains("日本") && !error.contains("http://")
        );
        if status == 302 || status == 307 {
            assert!(error.contains("redirects forbidden"));
        }
        task.abort();
    }
    let reports: Vec<_> = crate::logs::native_logs(0)
        .into_iter()
        .filter(|e| e.text.starts_with("ERROR: H3 continuity video-prepare:"))
        .collect();
    // The concurrent byte-cap test may evict throttled diagnostics; inspect only retained reports.
    assert!(
        reports.len() <= 2,
        "fixed fifth slot must bound all repeated failures"
    );
    for entry in reports {
        assert_eq!(entry.source, crate::logs::Source::Deas);
        for secret in ["secret-", "private.invalid", "日本", "http://", "session="] {
            assert!(!entry.text.contains(secret));
        }
    }
}

#[tokio::test]
async fn h3_video_prepare_accepts_maximum_path_and_edges_and_silent_video() {
    let filename = "a".repeat(4096);
    let expected = filename.clone();
    let (base, task) = server(move |_, body| {
        assert_eq!(body, json!({"filename":expected}));
        let mut v = prepared_video();
        v["filename"] = json!(expected);
        v["width"] = json!(65536);
        v["height"] = json!(1);
        v["has_audio"] = json!(false);
        (200, v.to_string())
    })
    .await;
    let result = native_h3_video_prepare(base, filename).await.unwrap();
    assert_eq!(result["width"], 65536);
    assert_eq!(result["hasAudio"], false);
    task.abort();
}

#[tokio::test]
async fn h3_video_prepare_bounds_chunked_bodies_and_hides_transport_errors() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![];
        loop {
            let mut buf = [0; 4096];
            let n = socket.read(&mut buf).await.unwrap();
            assert!(n > 0);
            request.extend_from_slice(&buf[..n]);
            if request.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n")
            .await
            .unwrap();
        let body = "x".repeat(RESPONSE_LIMIT + 1);
        let _ = socket
            .write_all(format!("{:x}\r\n{body}\r\n0\r\n\r\n", body.len()).as_bytes())
            .await;
    });
    assert_eq!(
        native_h3_video_prepare(base, "secret-video.mp4".into())
            .await
            .unwrap_err(),
        "Native job response size limit exceeded"
    );
    task.await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/secret-endpoint/", listener.local_addr().unwrap());
    drop(listener);
    assert_eq!(
        native_h3_video_prepare(base, "secret-video.mp4".into())
            .await
            .unwrap_err(),
        "Native job request failed; execution may be unknown"
    );
}

#[test]
fn h3_video_prepare_is_exported_and_uses_existing_local_permission() {
    let main = include_str!("main.rs");
    assert!(main.contains("deas::native_h3_video_prepare,"));
    let permission: toml::Value = toml::from_str(include_str!("../permissions/api.toml")).unwrap();
    let jobs = permission["permission"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["identifier"].as_str() == Some("allow-native-diagnostic-jobs"))
        .unwrap();
    assert!(jobs["commands"]["allow"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c.as_str() == Some("native_h3_video_prepare")));
    let capability: Value =
        serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
    assert_eq!(capability["local"], true);
    assert_eq!(capability["windows"], json!(["main"]));
    assert!(capability["permissions"]
        .as_array()
        .unwrap()
        .contains(&json!("allow-native-diagnostic-jobs")));
    let _ = crate::native_h3_video_prepare;
}

#[test]
fn wildcard_plugin_keeps_base_prompts_and_rule_snapshot() {
    let mut i = intent();
    i["wildcard"] = json!({"style":"Booru","seed":7,"reroll":3,"token_budget":200,"auto_reroll":true,"selection_state":{"Art/Light/wildcards":{"enabled":true,"weight":1.2}}});
    let graph = compile(&i).unwrap();
    assert_eq!(
        graph["WP"]["class_type"],
        "DaSiWa_WildcardPresetPromptBuilder"
    );
    assert_eq!(graph["WP"]["inputs"]["positive_input"], i["positivePrompt"]);
    assert_eq!(graph["WP"]["inputs"]["negative_input"], i["negativePrompt"]);
    assert_eq!(graph["WP"]["inputs"]["auto_reroll"], true);
    assert_eq!(
        serde_json::from_str::<Value>(graph["WP"]["inputs"]["selection_state"].as_str().unwrap())
            .unwrap(),
        i["wildcard"]["selection_state"]
    );
    assert_eq!(graph["2"]["inputs"]["text"], json!(["WP", 0]));
    assert_eq!(graph["3"]["inputs"]["text"], json!(["WP", 1]));
    let mut saved = graph.clone();
    civitai_saver(
        &mut saved,
        &json!(["DaSiWa_MetadataConfig", "DaSiWa_MetadataImageSaver"]),
        &json!({}),
    );
    assert_eq!(saved["M"]["inputs"]["text_positive"], json!(["WP", 0]));
    assert_eq!(saved["M"]["inputs"]["text_negative"], json!(["WP", 1]));
    for patch in [
        json!({"seed":-1}),
        json!({"seed":4294967296_u64}),
        json!({"reroll":2147483648_u64}),
        json!({"style":"other"}),
        json!({"token_budget":0}),
        json!({"auto_reroll":"yes"}),
        json!({"selection_state":[]}),
        json!({"extra":true}),
    ] {
        let mut bad = i.clone();
        for (k, v) in patch.as_object().unwrap() {
            bad["wildcard"][k] = v.clone();
        }
        assert!(compile(&bad).unwrap_err().contains("wildcard"));
    }
}

pub(super) fn intent() -> Value {
    json!({"recipeId":"checkpoint-text-to-image-diagnostic","version":1,
        "positivePrompt":"a cat", "negativePrompt":"", "checkpoint":"sd/model.safetensors",
        "seed":123,"width":512,"height":768,"steps":20,"cfg":7,
        "sampler":"euler","scheduler":"normal","outputPrefix":"DEAS_test"})
}

#[test]
fn windows_model_identifiers_survive_compilation_without_relaxing_input_paths() {
    let mut i = intent();
    i["checkpoint"] = json!(r"SDXL\日本語 model.safetensors");
    i["loras"] = json!([{"name":r"styles\detail.safetensors","strength":0.5}]);
    let graph = compile(&i).unwrap();
    assert_eq!(graph["1"]["inputs"]["ckpt_name"], i["checkpoint"]);
    assert!(graph
        .as_object()
        .unwrap()
        .values()
        .any(|n| n["inputs"]["lora_name"] == i["loras"][0]["name"]));
    for bad in [
        r"..\model.safetensors",
        r"a\..\model.safetensors",
        r"a/..\model.safetensors",
        r"\model.safetensors",
        r"C:\model.safetensors",
        r"a\\model.safetensors",
    ] {
        i["checkpoint"] = json!(bad);
        assert!(compile(&i).is_err(), "{bad}");
    }
    i["checkpoint"] = json!(r"SDXL\model.safetensors");
    i["inputImage"] = json!(r"deas-inputs\image.png");
    i["denoise"] = json!(0.5);
    assert!(compile(&i).is_err());
}

#[test]
fn windows_output_subfolders_are_normalized_and_cannot_escape() {
    let entry = |folder: &str| json!({"outputs":{"7":{"images":[{"filename":"image.png","subfolder":folder,"type":"output"}]}}});
    // Stored with `/` like the relocation repair; history validation and availability split on `/` only.
    assert_eq!(
        outputs(&entry(r"raw\image\2026-10-26")).unwrap()[0]["subfolder"],
        "raw/image/2026-10-26"
    );
    for bad in [
        r"..\secret",
        r"raw/..\secret",
        r"\root",
        r"C:\root",
        r"\\host\share",
        r"raw\\image",
    ] {
        assert!(outputs(&entry(bad)).is_err(), "{bad}");
    }
}

#[test]
fn output_naming_paths_are_bounded_relative_and_safe_on_windows() {
    let mut i = intent();
    i["outputPrefix"] = json!("image/2026_10_18/12-34-56-123-My_Model");
    let graph = compile(&i).unwrap();
    assert_eq!(graph["7"]["inputs"]["filename_prefix"], i["outputPrefix"]);
    for bad in [
        "", "../out", "/out", "a//b", "a/./b", "a/../b", "a\\b", "a:b", "a\n", "a/%seed%", "CON",
        "aux.png", "a/NUL", "a/name.",
    ] {
        i["outputPrefix"] = json!(bad);
        assert!(compile(&i).is_err(), "{bad:?}");
    }
    for bad in ["x".repeat(181), vec!["x".repeat(170); 3].join("/") + "/x"] {
        i["outputPrefix"] = json!(bad);
        assert!(compile(&i).is_err());
    }
}

use tokio::io::{AsyncReadExt, AsyncWriteExt};

const PROMPT_ID: &str = "cd5936d0-9f0f-4b82-8cda-b01c241bc669";

#[tokio::test]
async fn wildcard_library_is_bounded_read_only_and_preserves_category_order() {
    let raw = r#"{"categories":{"Z first":[],"A second":[]}}"#;
    let (base, task) = server(move |path, _| {
        assert_eq!(
            path,
            "GET /comfy/dasiwa/wildcard-preset-prompt-builder/library HTTP/1.1"
        );
        (200, raw.to_owned())
    })
    .await;
    assert_eq!(
        crate::wildcard::native_wildcard_library(base)
            .await
            .unwrap(),
        raw
    );
    task.abort();
    for (status, body) in [
        (302, "{}".to_owned()),
        (404, "{}".to_owned()),
        (200, "{\"categories\":[]}".to_owned()),
        (200, "x".repeat(2 * 1024 * 1024 + 1)),
    ] {
        let (base, task) = server(move |_, _| (status, body.clone())).await;
        assert!(crate::wildcard::native_wildcard_library(base)
            .await
            .is_err());
        task.abort();
    }
}

#[tokio::test]
async fn wildcard_prepare_requires_original_plugin_without_submitting() {
    for installed in [false, true] {
        let (base, task) = server(move |path, _| {
            assert_eq!(path, "GET /comfy/object_info HTTP/1.1");
            let mut n = nodes();
            if installed {
                n[crate::wildcard::NODE] = json!({});
            }
            (200, n.to_string())
        })
        .await;
        let mut i = intent();
        i["wildcard"] = json!({"style":"Booru","seed":0,"reroll":0,"token_budget":200,"auto_reroll":true,"selection_state":{}});
        let result = Jobs::default().prepare(base, i).await;
        if installed {
            assert_eq!(
                result.unwrap()["graph"]["WP"]["class_type"],
                crate::wildcard::NODE
            );
        } else {
            let error = result.unwrap_err();
            assert!(error.contains(crate::wildcard::NODE), "{error}");
        }
        task.abort();
    }
}

#[tokio::test]
async fn sam3_prepare_checks_only_emitted_nodes_explicit_checkpoint_and_safe_sources() {
    for scenario in [
        "ok",
        "model",
        "node",
        "image",
        "uploaded-mask",
        "uploaded-input",
        "output-id",
        "unrelated-sam-missing",
    ] {
        let mut i = json!({"recipeId":"sam3-mask","version":1,"checkpoint":"sam3.1.safetensors","inputImage":"ref.png","width":641,"height":479,"outputPrefix":"DEAS/mask","prompt":"eyes","threshold":0.5});
        if scenario == "uploaded-mask" {
            i["inputImage"] = json!("deas-masks/12345678-1234-1234-1234-123456789abc.png");
        }
        if scenario == "uploaded-input" {
            i["inputImage"] = json!("deas-inputs/source.png");
        }
        if scenario == "output-id" {
            i["inputImage"] = json!("DEAS/out.png [output]");
        }
        let (base, task) = server(move |path, _| {
            assert_eq!(path, "GET /comfy/object_info HTTP/1.1");
            let mut n = nodes();
            if scenario != "model" {
                n["CheckpointLoaderSimple"]["input"]["required"]["ckpt_name"] =
                    json!([["sam3.1.safetensors"]]);
            }
            if scenario != "node" && scenario != "unrelated-sam-missing" {
                n["SAM3_Detect"] = json!({});
            }
            n["MaskToImage"] = json!({});
            // The optional metadata saver must never replace the lossless mask saver.
            n["DaSiWa_MetadataImageSaver"] = json!({});
            n["DaSiWa_MetadataConfig"] = json!({});
            if scenario == "image" {
                n["LoadImage"]["input"]["required"]["image"] = json!([[]]);
            }
            (200, n.to_string())
        })
        .await;
        let jobs = Jobs::default();
        if scenario == "unrelated-sam-missing" {
            let mut ordinary = intent();
            ordinary["checkpoint"] = json!("sam3.1.safetensors");
            assert!(jobs.prepare(base, ordinary).await.is_ok());
        } else {
            let result = jobs.prepare(base, i).await;
            match scenario {
                "model" => assert!(result.unwrap_err().contains("checkpoints")),
                "node" => assert!(result.unwrap_err().contains("SAM3_Detect")),
                "image" | "output-id" => assert!(result.unwrap_err().contains("images")),
                _ => {
                    let result = result.unwrap();
                    assert_eq!(result["graph"]["7"]["class_type"], "SaveImage");
                }
            }
        }
        task.abort();
    }
}

#[tokio::test]
async fn sam3_workbench_submits_and_correlates_mask_completion_without_generation_fields() {
    let i = json!({"recipeId":"sam3-mask","version":1,"checkpoint":"sam3.1.safetensors","inputImage":"ref.png","width":641,"height":479,"outputPrefix":"DEAS/mask","prompt":"eyes","threshold":0.5});
    let expected = compile(&i).unwrap();
    let mut tuple = Value::Null;
    let (base, task) = server(move |path, body| match path.as_str() {
        "GET /comfy/object_info HTTP/1.1" => {
            let mut n = nodes();
            n["CheckpointLoaderSimple"]["input"]["required"]["ckpt_name"] = json!([["sam3.1.safetensors"]]);
            n["SAM3_Detect"] = json!({}); n["MaskToImage"] = json!({});
            (200, n.to_string())
        }
        "POST /comfy/prompt HTTP/1.1" => {
            assert_eq!(body["prompt"], expected);
            tuple = json!([1,PROMPT_ID,body["prompt"],{"client_id":body["client_id"]},["7"]]);
            (200, json!({"prompt_id":PROMPT_ID,"number":1,"node_errors":{}}).to_string())
        }
        p if p == format!("GET /comfy/history/{PROMPT_ID} HTTP/1.1") => (200, json!({PROMPT_ID:{"prompt":tuple,"outputs":{"7":{"images":[{"filename":"mask.png","subfolder":"DEAS","type":"output"}]}},"status":{"completed":true,"status_str":"success","messages":[]}}}).to_string()),
        _ => panic!("Unexpected SAM3 request: {path}"),
    }).await;
    let jobs = Jobs::default();
    let queued = jobs
        .workbench_run(base, i.clone(), true, 0, None)
        .await
        .unwrap();
    assert_eq!(queued["state"], "queued");
    let status = jobs.workbench_poll().await.unwrap();
    assert_eq!(status["jobs"][0]["state"], "succeeded", "{status}");
    assert_eq!(status["jobs"][0]["intent"], i);
    assert_eq!(status["jobs"][0]["outputs"][0]["filename"], "mask.png");
    let history = serde_json::to_value(jobs.3.view()).unwrap();
    assert_eq!(history["records"], json!([]));
    assert!(history["error"].is_null());
    task.abort();
}

#[tokio::test]
async fn sam3_workbench_validation_failures_use_shared_redacted_logs() {
    workbench::reset_workbench_failure_logging();
    let i = json!({"recipeId":"sam3-mask","version":1,"checkpoint":"secret-token","prompt":"private prompt"});
    assert!(Jobs::default()
        .workbench_run("http://127.0.0.1:1/".into(), i, true, 0, None)
        .await
        .is_err());
    let entries = crate::logs::native_logs(0);
    let log = entries
        .iter()
        .rev()
        .find(|e| e.text.starts_with("ERROR: Workbench run:"))
        .unwrap();
    assert_eq!(log.source, crate::logs::Source::Deas);
    assert!(!log.text.contains("secret-token") && !log.text.contains("private prompt"));
}

fn nodes() -> Value {
    json!({"CheckpointLoaderSimple":{"input":{"required":{"ckpt_name":[["sd/model.safetensors"]]}}},
        "CLIPTextEncode":{},"EmptyLatentImage":{},"KSampler":{"input":{"required":{"sampler_name":[["euler"]],"scheduler":[["normal"]]}}},"VAEDecode":{},"SaveImage":{},
        "LoadImage":{"input":{"required":{"image":[["ref.png","my ref ä.png","../bad.png"]]}}},"VAEEncode":{}})
}

pub(super) async fn server<F>(mut respond: F) -> (String, tokio::task::JoinHandle<()>)
where
    F: FnMut(String, Value) -> (u16, String) + Send + 'static,
{
    server_with_headers(move |path, body, _| respond(path, body)).await
}

async fn server_with_headers<F>(mut respond: F) -> (String, tokio::task::JoinHandle<()>)
where
    F: FnMut(String, Value, String) -> (u16, String) + Send + 'static,
{
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/comfy", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![];
            let (head_end, length) = loop {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&bytes[..end]);
                    let length = head
                        .lines()
                        .filter_map(|l| l.split_once(':'))
                        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                        .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                        .unwrap_or(0);
                    break (end + 4, length);
                }
            };
            while bytes.len() < head_end + length {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
            }
            let path = String::from_utf8_lossy(&bytes[..head_end])
                .lines()
                .next()
                .unwrap()
                .to_owned();
            let body = if length == 0 {
                Value::Null
            } else {
                serde_json::from_slice(&bytes[head_end..head_end + length]).unwrap()
            };
            // The live-preview websocket is best effort: refusing it must not affect the job.
            let (code, body) = if path.starts_with("GET /comfy/ws?clientId=") {
                (404, "{}".to_owned())
            } else {
                respond(
                    path,
                    body,
                    String::from_utf8_lossy(&bytes[..head_end]).into_owned(),
                )
            };
            if code == 0 {
                continue;
            } // Simulate a lost POST reply after receipt.
            let head = format!(
                "HTTP/1.1 {code} Fixture\r\nContent-Type: application/json\r\nLocation: http://127.0.0.1:9/forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(body.as_bytes()).await;
        }
    });
    (base, task)
}

#[tokio::test]
async fn workbench_poll_isolates_errors_preserves_intents_and_never_resubmits() {
    let posts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (sent, received) = (posts.clone(), reads.clone());
    let mut tuples = std::collections::HashMap::new();
    let (base, task) = server(move |path, body| {
        if path == "GET /comfy/object_info HTTP/1.1" {
            return (200, nodes().to_string());
        }
        if path == "POST /comfy/prompt HTTP/1.1" {
            sent.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let seed = body["prompt"]["5"]["inputs"]["seed"].as_u64().unwrap();
            if seed == 4 {
                return (0, String::new()); // Lost submission reply: no prompt ID to reconcile.
            }
            let id = uuid::Uuid::new_v4().to_string();
            tuples.insert(id.clone(), (seed, json!([1,id,body["prompt"],{"client_id":body["client_id"]},["7"]])));
            return (200, json!({"prompt_id":id,"number":1,"node_errors":{}}).to_string());
        }
        received.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if path.starts_with("GET /comfy/history/") {
            let id = path.split('/').nth(3).unwrap().split(' ').next().unwrap();
            let (seed, tuple) = &tuples[id];
            return match seed {
                1 => (0, String::new()), // Transport failure must not prevent later refreshes.
                2 => (200, json!({id:{"prompt":tuple,"outputs":{"7":{"images":[{"filename":"out.png","subfolder":"","type":"output"}]}},"status":{"completed":true,"status_str":"success","messages":[]}}}).to_string()),
                _ => (200, "{}".into()),
            };
        }
        assert_eq!(path, "GET /comfy/queue HTTP/1.1");
        let running: Vec<_> = tuples.values().filter(|(seed, _)| *seed == 5).map(|(_, tuple)| tuple).collect();
        (200, json!({"queue_running":running,"queue_pending":[]}).to_string())
    }).await;
    let jobs = Jobs::default();
    let mut plans = vec![];
    for seed in 1..=5 {
        let mut draft = intent();
        draft["seed"] = json!(seed);
        let result = jobs
            .workbench_run(base.clone(), draft.clone(), true, 0, None)
            .await
            .unwrap();
        let held = jobs
            .workbench_job(result["id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        let session = held.lock().unwrap();
        plans.push((
            session.plan.as_ref().unwrap().snapshot(),
            session.job.as_ref().unwrap().plan.snapshot(),
        ));
    }
    let before = jobs.workbench_current().unwrap();
    assert_eq!(
        reads.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "current must be network-free"
    );
    assert_eq!(before["jobs"][3]["state"], "unknown");
    let refreshed = jobs.workbench_poll().await.unwrap();
    assert_eq!(refreshed["capacity"], 20);
    assert_eq!(refreshed["preparing"], 0);
    assert_eq!(refreshed["stopAvailable"], true);
    for (index, state) in ["unknown", "succeeded", "unknown", "unknown", "running"]
        .iter()
        .enumerate()
    {
        assert_eq!(refreshed["jobs"][index]["state"], *state);
        assert_eq!(
            refreshed["jobs"][index]["intent"],
            before["jobs"][index]["intent"]
        );
        let held = jobs
            .workbench_job(before["jobs"][index]["id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        let session = held.lock().unwrap();
        assert_eq!(session.plan.as_ref().unwrap().snapshot(), plans[index].0);
        assert_eq!(
            session.job.as_ref().unwrap().plan.snapshot(),
            plans[index].1
        );
        assert!(session.plan.as_ref().unwrap().consumed);
    }
    assert_eq!(refreshed["jobs"][1]["outputs"][0]["filename"], "out.png");
    assert!(refreshed["jobs"][0]["error"]
        .as_str()
        .unwrap()
        .contains("request failed"));
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 6);
    let again = jobs.workbench_poll().await.unwrap();
    assert_eq!(
        again["jobs"].as_array().unwrap().len(),
        5,
        "unknowns are retained"
    );
    assert_eq!(
        reads.load(std::sync::atomic::Ordering::SeqCst),
        11,
        "terminal and missing-prompt jobs need no reads"
    );
    assert_eq!(posts.load(std::sync::atomic::Ordering::SeqCst), 5);
    task.abort();
}

#[tokio::test]
async fn workbench_poll_coalesces_concurrent_reads_and_survives_cancelled_waiter() {
    let (base, submission) = server(|path, _| match path.as_str() {
        "GET /comfy/object_info HTTP/1.1" => (200, nodes().to_string()),
        "POST /comfy/prompt HTTP/1.1" => (
            200,
            json!({"prompt_id":PROMPT_ID,"number":1,"node_errors":{}}).to_string(),
        ),
        _ => panic!("unexpected {path}"),
    })
    .await;
    let seed = Jobs::default();
    seed.workbench_run(base, intent(), true, 0, None)
        .await
        .unwrap();
    let template = seed.lock().unwrap().workbench[0]
        .lock()
        .unwrap()
        .job
        .clone()
        .unwrap();
    submission.abort();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let release = Arc::new(tokio::sync::Semaphore::new(0));
    let gate = release.clone();
    let (arrived, mut requests) = tokio::sync::mpsc::channel(64);
    let fixture = tokio::spawn(async move {
        let mut children = tokio::task::JoinSet::new();
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let (gate, arrived) = (gate.clone(), arrived.clone());
            children.spawn(async move {
                let mut bytes = vec![];
                while !bytes.windows(4).any(|b| b == b"\r\n\r\n") {
                    let mut buf = [0; 1024];
                    let n = socket.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                }
                let head = String::from_utf8(bytes).unwrap();
                assert_eq!(
                    head.lines().next().unwrap(),
                    format!("GET /history/{PROMPT_ID} HTTP/1.1")
                );
                arrived.send(()).await.unwrap();
                gate.acquire().await.unwrap().forget();
                socket
                    .write_all(
                        b"HTTP/1.1 503 Fixture\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                    )
                    .await
                    .unwrap();
            });
        }
    });
    let jobs = Arc::new(Jobs::default());
    for index in 0..20 {
        let held = Arc::new(Jobs::default());
        if index != 19 {
            let mut job = template.clone();
            job.id = uuid::Uuid::new_v4().to_string();
            job.plan.base = base.clone();
            match index {
                16 => job.state = "succeeded",
                17 => job.state = "failed",
                18 => job.submitting = true,
                _ => {}
            }
            held.lock().unwrap().job = Some(job);
        }
        jobs.lock().unwrap().workbench.push(held);
    }
    // Aggregate polling must not refresh the exclusive diagnostic slot either.
    jobs.lock().unwrap().job = Some(template.clone());
    for round in 0..4 {
        let owner = jobs.clone();
        let waiter = tokio::spawn(async move { owner.workbench_poll().await });
        // Every eligible request arrives before any reply: no serialized timeout chain.
        for _ in 0..16 {
            tokio::time::timeout(Duration::from_secs(3), requests.recv())
                .await
                .unwrap()
                .unwrap();
        }
        let snapshot = jobs.workbench_current().unwrap();
        let overlapping = tokio::time::timeout(Duration::from_secs(1), jobs.workbench_poll())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(overlapping, snapshot);
        assert_eq!(overlapping["preparing"], 1);
        assert_eq!(overlapping["stopAvailable"], true);
        assert!(
            requests.try_recv().is_err(),
            "overlap must not queue another pass"
        );
        if round == 1 {
            waiter.abort();
            assert!(waiter.await.unwrap_err().is_cancelled());
            assert_eq!(jobs.workbench_poll().await.unwrap(), snapshot);
            release.add_permits(16);
            // The detached pass owns the gate until every status has cleared polling.
            let guard = tokio::time::timeout(Duration::from_secs(3), jobs.2.lock())
                .await
                .unwrap();
            drop(guard);
        } else if round == 3 {
            // A whole stalled cohort must finish within one status deadline, not 16 of them.
            tokio::time::timeout(Duration::from_secs(18), waiter)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        } else {
            release.add_permits(16);
            tokio::time::timeout(Duration::from_secs(3), waiter)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
        assert!(requests.try_recv().is_err());
        let session = jobs.lock().unwrap();
        assert_eq!(
            session.job.as_ref().unwrap().snapshot(),
            template.snapshot()
        );
        for held in &session.workbench[..16] {
            let held = held.lock().unwrap();
            let job = held.job.as_ref().unwrap();
            assert!(!job.polling, "cancelled IPC must not strand status flags");
            assert_eq!(job.state, "unknown");
        }
        assert_eq!(
            session.workbench[16].current().unwrap()["state"],
            "succeeded"
        );
        assert_eq!(session.workbench[17].current().unwrap()["state"], "failed");
        assert!(
            session.workbench[18]
                .lock()
                .unwrap()
                .job
                .as_ref()
                .unwrap()
                .submitting
        );
    }
    fixture.abort();
}

#[tokio::test]
async fn workbench_failed_preparation_releases_reservation_and_diagnostic_exclusion() {
    let mut fail = true;
    let (base, task) = server(move |path, _| match path.as_str() {
        "GET /comfy/object_info HTTP/1.1" if fail => {
            fail = false;
            (503, "{}".into())
        }
        "GET /comfy/object_info HTTP/1.1" => (200, nodes().to_string()),
        "POST /comfy/prompt HTTP/1.1" => (
            200,
            json!({"prompt_id":PROMPT_ID,"number":1,"node_errors":{}}).to_string(),
        ),
        _ => panic!("unexpected {path}"),
    })
    .await;
    let jobs = Jobs::default();
    assert!(jobs.prepare(base.clone(), intent()).await.is_err());
    let job = jobs
        .workbench_run(base, intent(), true, 0, None)
        .await
        .unwrap();
    assert_eq!(job["state"], "queued");
    task.abort();
    let jobs = Jobs::default();
    assert!(jobs
        .workbench_run("http://127.0.0.1:9".into(), json!({}), true, 0, None)
        .await
        .is_err());
    assert_eq!(jobs.workbench_current().unwrap()["preparing"], 0);
    assert!(jobs.lock().unwrap().workbench.is_empty());
}

#[tokio::test]
async fn workbench_capacity_keeps_active_unknown_and_consumed_plans() {
    let posts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = posts.clone();
    let (base, task) = server(move |path, _| match path.as_str() {
        "GET /comfy/object_info HTTP/1.1" => (200, nodes().to_string()),
        "POST /comfy/prompt HTTP/1.1" => {
            let n = observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n == 0 { (0, "".into()) } else {
                (200, json!({"prompt_id":uuid::Uuid::new_v4().to_string(),"number":n,"node_errors":{}}).to_string())
            }
        }
        _ => panic!("unexpected {path}"),
    }).await;
    let jobs = Jobs::default();
    assert!(jobs
        .workbench_run(base.clone(), intent(), false, 0, None)
        .await
        .is_err());
    assert_eq!(posts.load(std::sync::atomic::Ordering::SeqCst), 0);
    let mut ids = vec![];
    for _ in 0..20 {
        ids.push(
            jobs.workbench_run(base.clone(), intent(), true, 0, None)
                .await
                .unwrap()["id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    let unknown = jobs.workbench_job(&ids[0]).unwrap().unwrap();
    assert_eq!(unknown.current().unwrap()["state"], "unknown");
    let plan_id = unknown.lock().unwrap().plan.as_ref().unwrap().id.clone();
    assert!(unknown.submit(plan_id, true).await.is_err());
    assert!(jobs
        .workbench_run(base.clone(), intent(), true, 0, None)
        .await
        .unwrap_err()
        .contains("capacity"));
    for id in &ids {
        assert!(jobs.workbench_job(id).unwrap().is_some());
    }
    assert_eq!(posts.load(std::sync::atomic::Ordering::SeqCst), 20);
    // Only a positively terminal record may make room; uncertainty must not be evicted.
    jobs.workbench_job(&ids[1])
        .unwrap()
        .unwrap()
        .lock()
        .unwrap()
        .job
        .as_mut()
        .unwrap()
        .state = "failed";
    jobs.workbench_run(base, intent(), true, 0, None)
        .await
        .unwrap();
    assert!(jobs.workbench_job(&ids[0]).unwrap().is_some());
    assert!(jobs.workbench_job(&ids[1]).unwrap().is_none());
    assert_eq!(
        jobs.workbench_current().unwrap()["jobs"]
            .as_array()
            .unwrap()
            .len(),
        20
    );
    assert_eq!(jobs.workbench_current().unwrap()["stopAvailable"], true);
    task.abort();
}

#[tokio::test]
async fn workbench_failures_reach_shared_logs_without_private_intent() {
    let jobs = Jobs::default();
    crate::jobs::workbench::reset_workbench_failure_logging();
    let start = crate::logs::native_logs(0).last().map_or(0, |e| e.seq);
    let mut draft = intent();
    draft["positivePrompt"] = json!("secret-queue-prompt");
    assert!(jobs
        .workbench_run(
            "http://127.0.0.1:9/private-endpoint".into(),
            draft.clone(),
            true,
            0,
            None
        )
        .await
        .is_err());
    assert!(jobs
        .workbench_run(
            "http://127.0.0.1:9/private-endpoint".into(),
            draft,
            true,
            0,
            None
        )
        .await
        .is_err());
    let reports: Vec<_> = crate::logs::native_logs(start)
        .into_iter()
        .filter(|e| e.text.starts_with("ERROR: Workbench run:"))
        .collect();
    // Per-job audit lines are unrate-limited; the generic admission notice is
    // rate-limited and must appear exactly once for both runs.
    let notices: Vec<_> = reports
        .iter()
        .filter(|e| e.text.starts_with("ERROR: Workbench run: preparation"))
        .collect();
    assert_eq!(notices.len(), 1, "start={start}; reports={reports:?}");
    assert!(!reports.is_empty(), "start={start}; reports={reports:?}");
    assert!(reports
        .iter()
        .all(|e| e.source == crate::logs::Source::Deas));
    assert!(reports
        .iter()
        .all(|e| !e.text.contains("secret-queue-prompt")));
    assert!(reports.iter().all(|e| !e.text.contains("private-endpoint")));
}

#[test]
fn workbench_commands_are_registered_exported_and_local_only() {
    let main = include_str!("main.rs");
    let permission: toml::Value = toml::from_str(include_str!("../permissions/api.toml")).unwrap();
    let jobs = permission["permission"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["identifier"].as_str() == Some("allow-native-diagnostic-jobs"))
        .unwrap();
    for command in [
        "native_workbench_run",
        "native_auto_refine",
        "native_workbench_current",
        "native_workbench_poll",
        "native_workbench_cancel_pending",
        "native_workbench_interrupt_running",
        "native_workbench_stop",
    ] {
        assert!(main.contains(&format!("deas::{command},")));
        assert!(jobs["commands"]["allow"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str() == Some(command)));
    }
    let capability: Value =
        serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
    assert_eq!(capability["local"], true);
    assert_eq!(capability["windows"], json!(["main"]));
    assert!(capability["permissions"]
        .as_array()
        .unwrap()
        .contains(&json!("allow-native-diagnostic-jobs")));
    let _ = crate::native_workbench_run;
    let _ = crate::native_auto_refine;
    let _ = crate::native_workbench_current;
    let _ = crate::native_workbench_poll;
    let _ = crate::native_workbench_cancel_pending;
    let _ = crate::native_workbench_interrupt_running;
    let _ = crate::native_workbench_stop;
}

#[tokio::test]
async fn workbench_runs_keep_three_immutable_correlated_jobs() {
    let mut tuples = std::collections::HashMap::new();
    let (base, task) = server(move |path, body| {
        if path == "GET /comfy/object_info HTTP/1.1" {
            return (200, nodes().to_string());
        }
        if path == "POST /comfy/prompt HTTP/1.1" {
            let id = uuid::Uuid::new_v4().to_string();
            tuples.insert(id.clone(), json!([1,id,body["prompt"],{"client_id":body["client_id"]},["7"]]));
            return (200, json!({"prompt_id":id,"number":1,"node_errors":{}}).to_string());
        }
        if path.starts_with("GET /comfy/history/") {
            let id = path.split('/').nth(3).unwrap().split(' ').next().unwrap();
            return (200, json!({id:{"prompt":tuples[id],"outputs":{"7":{"images":[{"filename":"out.png","subfolder":"","type":"output"}]}},"status":{"completed":true,"status_str":"success","messages":[]}}}).to_string());
        }
        panic!("Unexpected request: {path}");
    }).await;
    let jobs = Jobs::default();
    let mut submitted = vec![];
    for seed in 1..=3 {
        let mut draft = intent();
        draft["seed"] = json!(seed);
        let job = jobs
            .workbench_run(base.clone(), draft.clone(), true, 0, None)
            .await
            .unwrap();
        draft["seed"] = json!(999);
        assert_eq!(job["intent"]["seed"], seed);
        submitted.push(job);
    }
    assert_eq!(
        jobs.workbench_current().unwrap()["jobs"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(jobs.prepare(base.clone(), intent()).await.is_err());
    for (index, job) in submitted.iter().enumerate() {
        let id = job["id"].as_str().unwrap();
        assert!(submitted[..index]
            .iter()
            .all(|other| other["id"] != job["id"]));
        let held = jobs.workbench_job(id).unwrap().unwrap();
        assert_eq!(
            jobs.status_for(id.into()).await.unwrap()["state"],
            "succeeded"
        );
        assert_eq!(held.current().unwrap()["intent"]["seed"], index + 1);
    }
    assert!(jobs.workbench_job("foreign").unwrap().is_none());
    assert!(jobs.prepare(base, intent()).await.is_ok());
    task.abort();
}

#[tokio::test]
async fn explicit_prepare_submit_and_correlated_completion() {
    let dir = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(dir.path()).unwrap();
    let posts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = posts.clone();
    let mut tuple = Value::Null;
    let mut polls = 0;
    let (base, server) = server(move |path, body| {
        match path.as_str() {
            "GET /comfy/object_info HTTP/1.1" => (200,nodes().to_string()),
            "POST /comfy/prompt HTTP/1.1" => {
                observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                assert_eq!(body.as_object().unwrap().len(),3);
                assert_eq!(body["extra_data"],json!({"preview_method":"auto","extra_pnginfo":{"deas_output_naming":true}}));
                assert_eq!(body["prompt"],compile(&intent()).unwrap());
                assert!(uuid::Uuid::parse_str(body["client_id"].as_str().unwrap()).is_ok());
                tuple = json!([1,PROMPT_ID,body["prompt"],{"client_id":body["client_id"]},["7"]]);
                (200,json!({"prompt_id":PROMPT_ID,"number":1,"node_errors":{}}).to_string())
            },
            p if p == format!("GET /comfy/history/{PROMPT_ID} HTTP/1.1") => {
                polls += 1;
                (200,if polls < 3 {json!({})} else {json!({PROMPT_ID:{"prompt":tuple,"outputs":{"7":{"images":[{"filename":"DEAS_0001.png","subfolder":"","type":"output"}]}},"status":{"completed":true,"status_str":"success","messages":[]}}})}.to_string())
            },
            "GET /comfy/queue HTTP/1.1" => (200,if polls == 1 {json!({"queue_running":[],"queue_pending":[tuple]})} else {json!({"queue_running":[tuple],"queue_pending":[]})}.to_string()),
            _ => panic!("Unexpected request: {path}")
        }
    }).await;
    let jobs = Jobs::default();
    let plan = jobs.prepare(base.clone(), intent()).await.unwrap();
    assert_eq!(posts.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(plan["generationReady"], false);
    assert_eq!(plan["baseUrl"], format!("{base}/"));
    let id = plan["id"].as_str().unwrap().to_owned();
    jobs.lock().unwrap().plan.as_mut().unwrap().media_root =
        Some(job_outputs::MediaRoot::capture(root.clone()).unwrap());
    assert!(jobs.submit(id.clone(), false).await.is_err());
    assert!(jobs.submit("foreign-plan".into(), true).await.is_err());
    let job = jobs.submit(id.clone(), true).await.unwrap();
    assert_eq!(job["state"], "queued");
    assert_eq!(job["intent"], plan["intent"]);
    assert_eq!(job["appVersion"], env!("CARGO_PKG_VERSION"));
    assert_eq!(job["generationReady"], false);
    assert!(jobs.submit(id.clone(), true).await.is_err());
    assert!(jobs.prepare(base.clone(), intent()).await.is_err());
    assert!(jobs.status("foreign-job".into()).await.is_err());
    let job_id = job["id"].as_str().unwrap().to_owned();
    for expected in ["queued", "running", "succeeded"] {
        let status = jobs.status(job_id.clone()).await.unwrap();
        assert_eq!(status["state"], expected, "{status}");
        assert_eq!(status["intent"], plan["intent"]);
        assert_eq!(status["generationReady"], false);
        if expected == "succeeded" {
            assert_eq!(status["outputs"].as_array().unwrap().len(), 1);
        }
    }
    assert!(jobs.submit(id, true).await.is_err());
    assert_eq!(posts.load(std::sync::atomic::Ordering::SeqCst), 1);
    // A subsequent real prepare/submit must preserve the first completion's authority.
    let second = jobs.prepare(base, intent()).await.unwrap();
    jobs.lock().unwrap().plan.as_mut().unwrap().media_root =
        Some(job_outputs::MediaRoot::capture(root.clone()).unwrap());
    let second = jobs
        .submit(second["id"].as_str().unwrap().into(), true)
        .await
        .unwrap();
    let second = jobs
        .status(second["id"].as_str().unwrap().into())
        .await
        .unwrap();
    assert_eq!(second["state"], "succeeded");
    assert_ne!(second["id"], job_id);
    assert_eq!(jobs.lock().unwrap().completed.len(), 2);
    let source = root.join("DEAS_0001.png");
    std::fs::create_dir(root.join("starred")).unwrap();
    let target = root.join("starred/DEAS_0001.png");
    let mut png = Vec::new();
    png::Encoder::new(&mut png, 1, 1)
        .write_header()
        .unwrap()
        .write_image_data(&[127])
        .unwrap();
    std::fs::write(&source, &png).unwrap();
    jobs.relocate_media(&root, &source, || {
        std::fs::rename(&source, &target).unwrap();
        Ok(target.clone())
    })
    .unwrap();
    assert_eq!(jobs.current().unwrap()["intent"], second["intent"]);
    assert_eq!(
        jobs.current().unwrap()["outputs"][0]["subfolder"],
        "starred"
    );
    assert!(
        jobs.media_move_ready(&root, &source).is_err(),
        "all aliases must be repaired"
    );
    jobs.media_move_ready(&root, &target).unwrap();
    assert_eq!(posts.load(std::sync::atomic::Ordering::SeqCst), 2);
    server.abort();
}

#[tokio::test]
async fn h3_prepare_accepts_uploaded_reference_outside_top_level_image_combo() {
    let mut intent = crate::jobs_video::tests::h3();
    intent["h3"]["references"][0]["image"] =
        json!("deas-inputs/4907ef27-6ffc-4fe9-a5af-56bbac500c11.png");
    let graph = compile(&intent).unwrap();
    let mut all = nodes();
    all["UNETLoader"] = json!({"input":{"required":{"unet_name":[[intent["model"]]]}}});
    all["CLIPLoader"] =
        json!({"input":{"required":{"clip_name":[[intent["textEncoder"]]],"type":[["minimax"]]}}});
    all["VAELoader"] =
        json!({"input":{"required":{"vae_name":[[intent["vae"],intent["h3"]["audioVae"]]]}}});
    all["KSampler"]["input"]["required"]["scheduler"] = json!([["simple"]]);
    for node in graph.as_object().unwrap().values() {
        let name = node["class_type"].as_str().unwrap();
        if all.get(name).is_none() {
            all[name] = json!({});
        }
    }
    let (base, task) = server(move |path, _| {
        assert_eq!(path, "GET /comfy/object_info HTTP/1.1");
        (200, all.to_string())
    })
    .await;
    let result = Jobs::default().prepare(base, intent).await;
    task.abort();
    assert_eq!(result.unwrap()["graph"], graph);
}

#[tokio::test]
async fn video_prepare_checks_every_graph_node_and_reads_gifs_outputs() {
    let intent = crate::jobs_video::tests::ltx();
    let graph = compile(&intent).unwrap();
    let mut all = json!({"UNETLoader":{"input":{"required":{"unet_name":[["LTX2/ltx.safetensors"]]}}},
        "CLIPLoader":{"input":{"required":{"clip_name":[["gemma_3_12B_it_fp8.safetensors","ltx-2.3_text_projection_bf16.safetensors"]],"type":[["ltxv","wan","minimax"]]}}},
        "VAELoader":{"input":{"required":{"vae_name":[["LTX2/video_vae.safetensors","LTX2/audio_vae.safetensors"]]}}},
        "DualCLIPLoader":{"input":{"required":{"clip_name1":[["gemma_3_12B_it_fp8.safetensors","ltx-2.3_text_projection_bf16.safetensors"]],"type":[["ltxv"]]}}},
        "KSampler":{"input":{"required":{"sampler_name":[["euler_cfg_pp"]],"scheduler":[["linear_quadratic"]]}}}});
    for n in graph.as_object().unwrap().values() {
        let name = n["class_type"].as_str().unwrap();
        if all.get(name).is_none() {
            all[name] = json!({});
        }
    }
    let without = {
        let mut m = all.clone();
        m.as_object_mut()
            .unwrap()
            .remove("DaSiWa_EnhancedVideoCombine");
        m
    };
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = calls.clone();
    let (base, server) = server(move |path, _| {
        assert_eq!(path, "GET /comfy/object_info HTTP/1.1");
        let n = seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        (
            200,
            if n == 0 { without.clone() } else { all.clone() }.to_string(),
        )
    })
    .await;
    let jobs = Jobs::default();
    let err = jobs
        .prepare(base.clone(), intent.clone())
        .await
        .unwrap_err();
    assert!(
        err.contains("nodes") && err.contains("DaSiWa_EnhancedVideoCombine"),
        "{err}"
    );
    let plan = jobs.prepare(base, intent).await.unwrap();
    assert_eq!(
        plan["graph"]["7"]["class_type"],
        "DaSiWa_EnhancedVideoCombine"
    );
    server.abort();
    let out = outputs(&json!({"outputs":{"7":{"gifs":[{"filename":"x_00001.webm","subfolder":"video","type":"output","format":"video/webm","codec":"AV1"}],"images":[{"filename":"x_first.png","subfolder":"video","type":"output"}]}}})).unwrap();
    assert_eq!(out, json!([{"filename":"x_first.png","subfolder":"video","type":"output"},{"filename":"x_00001.webm","subfolder":"video","type":"output","format":"video/webm"}]).as_array().unwrap().clone());
}

#[tokio::test]
async fn music_prepare_dispatches_without_image_fallbacks_and_reads_audio_outputs() {
    let t2m = crate::jobs_music::tests::full();
    let cover: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/music/cover.intent.json")).unwrap();
    let graph = compile(&t2m).unwrap();
    assert_eq!(graph["7"]["class_type"], "SaveAudioAdvanced");
    assert!(graph
        .as_object()
        .unwrap()
        .values()
        .all(|n| n["class_type"] != "SaveImage"));
    // Captured from the installed core (A6 gate): V1 checkpoint combo, V3 encoder combo, DynamicCombo saver.
    let mut all = json!({"CheckpointLoaderSimple":{"input":{"required":{"ckpt_name":[["yue2.safetensors","sd/model.safetensors"]]}}},
        "AudioEncoderLoader":{"input":{"required":{"audio_encoder_name":["COMBO",{"multiselect":false,"options":["sheetsage2_bf16.safetensors","../x.safetensors"]}]}}},
        "LoadAudio":{"input":{"required":{"audio":["COMBO",{"options":["tone.flac"]}]}}},
        "KSampler":{"input":{"required":{"sampler_name":[["dpm_2"]],"scheduler":[["sgm_uniform"]]}}},
        "DaSiWa_MetadataImageSaver":{},"DaSiWa_MetadataConfig":{}});
    for n in crate::jobs_music::MUSIC_NODES {
        if all.get(n).is_none() {
            all[n] = json!({});
        }
    }
    let without = {
        let mut m = all.clone();
        m.as_object_mut().unwrap().remove("YuE2GenerateABC");
        m
    };
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = calls.clone();
    let (base, server) = server(move |path, _| {
        assert_eq!(path, "GET /comfy/object_info HTTP/1.1");
        let n = seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        (
            200,
            if n == 0 { without.clone() } else { all.clone() }.to_string(),
        )
    })
    .await;
    let jobs = Jobs::default();
    let err = jobs.prepare(base.clone(), t2m.clone()).await.unwrap_err();
    assert!(
        err.contains("nodes") && err.contains("YuE2GenerateABC"),
        "{err}"
    );
    let plan = jobs.prepare(base.clone(), t2m.clone()).await.unwrap();
    assert_eq!(plan["graph"]["7"]["class_type"], "SaveAudioAdvanced");
    assert_eq!(plan["intent"], t2m);
    assert!(plan["graph"].get("M").is_some() && plan["graph"].get("A").is_some());
    // Cover: the fresh upload name is not in the stale LoadAudio combo; the encoder must be listed.
    let plan = jobs.prepare(base.clone(), cover.clone()).await.unwrap();
    assert_eq!(
        plan["graph"]["I"]["inputs"]["audio"],
        "deas-input-source.mp4"
    );
    let mut other = cover.clone();
    other["audioEncoder"] = json!("missing.safetensors");
    let err = jobs.prepare(base.clone(), other).await.unwrap_err();
    assert!(
        err.contains("audioEncoders") && err.contains("missing.safetensors"),
        "{err}"
    );
    let mut other = t2m.clone();
    other["checkpoint"] = json!("sd/other.safetensors");
    assert!(jobs
        .prepare(base, other)
        .await
        .unwrap_err()
        .contains("checkpoints"));
    assert!(calls.load(std::sync::atomic::Ordering::SeqCst) >= 5);
    server.abort();
    // History: SaveAudioAdvanced lists FLAC descriptors under `audio` (A6-captured shape).
    let e = json!({"outputs":{"7":{"audio":[{"filename":"YuE2_00001.flac","subfolder":"raw","type":"output"}]},
        "foreign":{"audio":[{"filename":"foreign.flac","subfolder":"raw","type":"output"}]}}});
    let o = outputs(&e).unwrap();
    assert_eq!(
        o,
        json!([{"filename":"YuE2_00001.flac","subfolder":"raw","type":"output"}])
            .as_array()
            .unwrap()
            .clone()
    );
    assert!(outputs(&json!({"outputs":{"7":{"audio":[{"filename":"../escape.flac","subfolder":"raw","type":"output"}]}}})).is_err());
    assert!(outputs(
        &json!({"outputs":{"7":{"audio":[{"filename":"a.flac","subfolder":"raw","type":"temp"}]}}})
    )
    .is_err());
    assert!(outputs(&json!({"outputs":{"7":{"audio":{"filename":"a.flac"}}}})).is_err());
    let many: Vec<Value> = (0..65)
        .map(|n| json!({"filename":format!("a{n}.flac"),"subfolder":"raw","type":"output"}))
        .collect();
    assert!(outputs(&json!({"outputs":{"7":{"audio":many}}})).is_err());
    assert_eq!(
        outputs(&json!({"outputs":{"7":{"audio":[]}}}))
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn invalid_intents_cannot_smuggle_graphs_or_unbounded_inputs() {
    for field in intent().as_object().unwrap().keys() {
        let mut value = intent();
        value.as_object_mut().unwrap().remove(field);
        assert!(compile(&value).is_err(), "missing {field}");
    }
    for (field, bad) in [
        ("graph", json!({})),
        ("version", json!("1")),
        ("recipeId", json!("Director")),
        ("positivePrompt", json!("x".repeat(16385))),
        ("negativePrompt", json!("\0")),
        ("positivePrompt", json!("😀".repeat(8193))),
        ("positivePrompt", json!(false)),
        ("checkpoint", json!("../model")),
        ("checkpoint", json!("/model")),
        ("checkpoint", json!("dir\\..\\model")),
        ("checkpoint", json!(" model")),
        ("checkpoint", json!("dir//model")),
        ("checkpoint", json!("https://host/model")),
        ("checkpoint", json!("a".repeat(256))),
        ("checkpoint", json!("")),
        ("seed", json!(9_007_199_254_740_992_u64)),
        ("seed", json!(-1)),
        ("seed", json!(0.5)),
        ("width", json!(65)),
        ("width", json!(8200)),
        ("height", json!(56)),
        ("steps", json!(151)),
        ("steps", json!(0)),
        ("cfg", json!(30.1)),
        ("cfg", json!("7")),
        ("sampler", json!("../other")),
        ("scheduler", json!(null)),
        ("outputPrefix", json!("../x")),
        ("outputPrefix", json!("")),
        ("outputPrefix", json!("a".repeat(181))),
        ("outputPrefix", json!("image.")),
    ] {
        let mut value = intent();
        value[field] = bad;
        assert!(compile(&value).is_err(), "accepted {field}: {value}");
    }
    let mut edge = intent();
    edge["positivePrompt"] = json!("😀".repeat(8192));
    edge["cfg"] = json!(0.25);
    edge["seed"] = json!(9_007_199_254_740_991_u64);
    edge["version"] = json!(1.0);
    assert!(compile(&edge).is_ok());
}

#[test]
fn optional_loras_chain_between_checkpoint_and_sampler() {
    let mut with = intent();
    with["loras"] = json!([{"name":"style/a.safetensors","strength":0.8},{"name":"b.safetensors","strength":-1}]);
    let graph = compile(&with).unwrap();
    assert_eq!(graph["L1"]["inputs"]["model"], json!(["1", 0]));
    assert_eq!(graph["L2"]["inputs"]["clip"], json!(["L1", 1]));
    assert_eq!(graph["L2"]["inputs"]["strength_model"], json!(-1));
    assert_eq!(graph["5"]["inputs"]["model"], json!(["L2", 0]));
    assert_eq!(graph["2"]["inputs"]["clip"], json!(["L2", 1]));
    assert_eq!(graph["7"]["class_type"], "SaveImage");
    with["loras"] = json!([]);
    assert_eq!(compile(&with).unwrap(), compile(&intent()).unwrap());
    for bad in [
        json!("x"),
        json!([{"name":"../a","strength":1}]),
        json!([{"name":"a","strength":11}]),
        json!([{"name":"a","strength":1,"extra":1}]),
        json!(vec![json!({"name":"a","strength":1}); 9]),
    ] {
        with["loras"] = bad;
        assert!(compile(&with).is_err(), "accepted loras: {with}");
    }
}

#[test]
fn options_chain_model_patches_and_tile_the_vae() {
    let mut with = intent();
    with["loras"] = json!([{"name":"a.safetensors","strength":1}]);
    with["inputImage"] = json!("ref.png");
    with["denoise"] = json!(0.5);
    with["options"] = json!({"attention":"comfy kitchen attention","cache":{"node":"EasyCache","threshold":0.2},"compile":"inductor","vaeTile":512});
    let graph = compile(&with).unwrap();
    assert_eq!(graph["A"]["inputs"]["model"], json!(["L1", 0]));
    assert_eq!(graph["A"]["inputs"]["attention"], "comfy kitchen attention");
    assert_eq!(graph["E"]["class_type"], "EasyCache");
    assert_eq!(graph["E"]["inputs"]["model"], json!(["A", 0]));
    assert_eq!(graph["T"]["inputs"]["model"], json!(["E", 0]));
    assert_eq!(graph["5"]["inputs"]["model"], json!(["T", 0]));
    assert_eq!(graph["6"]["class_type"], "VAEDecodeTiled");
    assert_eq!(graph["6"]["inputs"]["tile_size"], 512);
    assert_eq!(graph["9"]["class_type"], "VAEEncodeTiled");
    assert_eq!(graph["9"]["inputs"]["pixels"], json!(["8", 0]));
    let mut sage = intent();
    sage["options"] = json!({"sage":"sageattn_qk_int8_pv_fp8_cuda++"});
    assert_eq!(
        compile(&sage).unwrap()["5"]["inputs"]["model"],
        json!(["K", 0])
    );
    let mut empty = intent();
    empty["options"] = json!({});
    assert_eq!(compile(&empty).unwrap(), compile(&intent()).unwrap());
    for bad in [
        json!({"attention":"pytorch attention","sage":"auto"}),
        json!({"fp16Accumulation":true}),
        json!({"cache":{"node":"TeaCache","threshold":0.2}}),
        json!({"cache":{"node":"EasyCache","threshold":4}}),
        json!({"vaeTile":500}),
        json!({"vaeTile":128}),
        json!({"attention":" padded"}),
        json!({"unknown":true}),
        json!([]),
    ] {
        empty["options"] = bad;
        assert!(compile(&empty).is_err(), "accepted options: {empty}");
    }
}

#[test]
fn diagnostic_availability_ignores_windows_separators() {
    let available = [
        "Anima/dasiwaAnima_obsidianArchivesV2+Int8.safetensors",
        "top.safetensors",
    ]
    .map(|s| json!(s));
    for field in ["diffusionModels", "checkpoints"] {
        assert!(listed(
            &available,
            field,
            "Anima\\dasiwaAnima_obsidianArchivesV2+Int8.safetensors"
        ));
        assert!(listed(
            &available,
            field,
            "Anima/dasiwaAnima_obsidianArchivesV2+Int8.safetensors"
        ));
        assert!(listed(&available, field, "top.safetensors"));
        assert!(!listed(&available, field, "Anima\\other.safetensors"));
    }
    assert!(listed(
        &[],
        "images",
        "deas-inputs/550e8400-e29b-41d4-a716-446655440000.png"
    ));
    assert!(!listed(&[], "diffusionModels", "deas-inputs/x.safetensors"));
}

#[tokio::test]
async fn prepare_wrong_loader_folder_blocks_submission_and_preserves_missing_errors() {
    for split in [false, true] {
        let (field, expected, wrong) = if split {
            (
                "diffusionModels",
                "models/diffusion_models",
                "models/checkpoints",
            )
        } else {
            (
                "checkpoints",
                "models/checkpoints",
                "models/diffusion_models",
            )
        };
        for (selected, listed_name) in [
            (
                r"Anima\model+Int8.safetensors",
                "Anima/model+Int8.safetensors",
            ),
            (
                "Anima/model+Int8.safetensors",
                r"Anima\model+Int8.safetensors",
            ),
        ] {
            for placement in ["wrong", "neither", "different", "both", "expected"] {
                let mut value = intent();
                value["checkpoint"] = json!(selected);
                if split {
                    value["diffusion"] = json!({"textEncoder":"encoder.safetensors","clipType":"anima","vae":"vae.safetensors"});
                }
                let mut info = nodes();
                info["CheckpointLoaderSimple"]["input"]["required"]["ckpt_name"][0] = json!([]);
                info["UNETLoader"] = json!({"input":{"required":{"unet_name":[[]]}}});
                info["CLIPLoader"] = json!({"input":{"required":{"clip_name":[["encoder.safetensors"]],"type":[["anima"]]}}});
                info["VAELoader"] =
                    json!({"input":{"required":{"vae_name":[["vae.safetensors"]]}}});
                for (node, input, is_expected) in [
                    ("UNETLoader", "unet_name", split),
                    ("CheckpointLoaderSimple", "ckpt_name", !split),
                ] {
                    info[node]["input"]["required"][input][0] = if placement == "both"
                        || (is_expected && placement == "expected")
                        || (!is_expected && placement == "wrong")
                    {
                        json!([listed_name])
                    } else if !is_expected && placement == "different" {
                        json!(["Other/model+Int8.safetensors"])
                    } else {
                        json!([])
                    };
                }
                let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
                let observed = reads.clone();
                let (base, task) = server(move |path, _| {
                    assert_eq!(path, "GET /comfy/object_info HTTP/1.1");
                    observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    (200, info.to_string())
                })
                .await;
                let jobs = Jobs::default();
                let result = jobs.prepare(base, value).await;
                if matches!(placement, "both" | "expected") {
                    let graph = &result.unwrap()["graph"];
                    assert_eq!(
                        graph["1"]["class_type"],
                        if split {
                            "UNETLoader"
                        } else {
                            "CheckpointLoaderSimple"
                        }
                    );
                } else {
                    let error = result.unwrap_err();
                    if placement == "wrong" {
                        assert!(error.starts_with("MODEL_WRONG_FOLDER:"), "{error}");
                        assert!(error.contains(selected), "{error}");
                        assert!(error.contains(&format!("expected {expected}")), "{error}");
                        assert!(error.contains(&format!("listed in {wrong}")), "{error}");
                        assert!(error.contains("Move"), "{error}");
                    } else {
                        assert_eq!(
                            error,
                            format!(
                                "Diagnostic {field} are not available on this backend: {selected}"
                            )
                        );
                    }
                    assert!(jobs.lock().unwrap().plan.is_none());
                    assert!(jobs.lock().unwrap().preparing.is_none());
                    assert!(jobs.submit("unprepared".into(), true).await.is_err());
                    assert!(jobs.lock().unwrap().job.is_none());
                }
                assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 1);
                assert!(!task.is_finished());
                task.abort();
            }
        }
    }
}

#[test]
fn dasiwa_saver_replaces_save_image_only_when_both_nodes_exist() {
    let mut graph = compile(&intent()).unwrap();
    civitai_saver(
        &mut graph,
        &json!(["SaveImage", "DaSiWa_MetadataImageSaver"]),
        &json!({}),
    );
    assert_eq!(graph, compile(&intent()).unwrap());
    civitai_saver(
        &mut graph,
        &json!(["DaSiWa_MetadataImageSaver", "DaSiWa_MetadataConfig"]),
        &json!({}),
    );
    assert_eq!(graph["7"]["class_type"], "DaSiWa_MetadataImageSaver");
    assert_eq!(graph["7"]["inputs"]["filename_prefix"], "DEAS_test");
    assert_eq!(graph["7"]["inputs"]["metadata_config"], json!(["M", 0]));
    assert_eq!(graph["M"]["inputs"]["node_latent"], json!(["5", 0]));
}

#[test]
fn optional_input_image_switches_to_image_to_image() {
    let mut with = intent();
    with["inputImage"] = json!("ref.png");
    with["denoise"] = json!(0.6);
    let graph = compile(&with).unwrap();
    assert_eq!(graph["8"]["class_type"], "LoadImage");
    assert_eq!(graph["9"]["inputs"]["pixels"], json!(["8", 0]));
    assert_eq!(graph["5"]["inputs"]["latent_image"], json!(["9", 0]));
    assert_eq!(graph["5"]["inputs"]["denoise"], json!(0.6));
    assert!(graph.get("4").is_none());
    for (field, bad) in [
        ("denoise", json!(1.5)),
        ("inputImage", json!("../x.png")),
        ("inputImage", json!(" padded.png")),
        ("inputImage", json!("C:/x.png")),
    ] {
        let mut v = with.clone();
        v[field] = bad;
        assert!(compile(&v).is_err(), "accepted {field}: {v}");
    }
    let mut alone = intent();
    alone["denoise"] = json!(0.5);
    assert!(compile(&alone).is_err());
}

#[test]
fn input_mask_resamples_only_the_painted_area_and_composites_onto_the_source() {
    let mut with = intent();
    with["inputImage"] = json!("out.png [output]");
    with["denoise"] = json!(0.8);
    with["inputMask"] = json!("deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png");
    let graph = compile(&with).unwrap();
    assert_eq!(
        graph["W"]["inputs"],
        json!({"image":"deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png","channel":"red"})
    );
    assert_eq!(graph["5"]["inputs"]["latent_image"], json!(["X", 0]));
    assert_eq!(graph["X"]["inputs"]["samples"], json!(["9", 0]));
    assert_eq!(graph["Y"]["inputs"]["destination"], json!(["8", 0]));
    assert_eq!(graph["7"]["inputs"]["images"], json!(["Y", 0]));
    // The DaSiWa saver keeps the composite.
    let mut saver = graph.clone();
    civitai_saver(
        &mut saver,
        &json!(["DaSiWa_MetadataImageSaver", "DaSiWa_MetadataConfig"]),
        &json!({"imageFormat":"webp","imageCompression":10}),
    );
    assert_eq!(saver["7"]["inputs"]["images"], json!(["Y", 0]));
    assert_eq!(saver["7"]["inputs"]["file_format"], "webp");
    assert_eq!(saver["7"]["inputs"]["compression"], 10);
    let mut formatted = with.clone();
    formatted["imageFormat"] = json!("webp");
    assert!(compile(&formatted).is_ok());
    formatted["imageFormat"] = json!("gif");
    assert!(compile(&formatted).is_err());
    for bad in [
        json!("deas-masks/x.png"),
        json!("ref.png"),
        json!("deas-masks/../3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png"),
    ] {
        let mut v = with.clone();
        v["inputMask"] = bad;
        assert!(compile(&v).is_err(), "accepted {v}");
    }
    let mut resized = with.clone();
    resized["inputResize"] = json!({"width":512,"height":768,"crop":"disabled"});
    assert!(
        compile(&resized).is_err(),
        "mask must match the unresized source"
    );
    let mut no_image = intent();
    no_image["inputMask"] = with["inputMask"].clone();
    assert!(compile(&no_image).is_err());
}

#[test]
fn minimax_h3_edits_the_crop_as_first_frame_and_composites_frame_zero_back() {
    let mut i = intent();
    i["checkpoint"] = json!("MiniMaxH3/fl2va.safetensors");
    i["diffusion"] = json!({"textEncoder":"qwen3vl_32b_minimax_h3.safetensors","clipType":"minimax","vae":"MiniMaxH3/video_vae.safetensors"});
    i["h3Edit"] = json!({"shiftVideo":10,"shiftAudio":3});
    (i["steps"], i["cfg"], i["scheduler"]) = (json!(4), json!(1), json!("simple"));
    i["inputImage"] = json!("out.png [output]");
    i["denoise"] = json!(1);
    i["inputMask"] = json!("deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png");
    i["maskBlur"] = json!(4);
    i["inpaintCrop"] =
        json!({"x":40,"y":20,"width":120,"height":80,"targetWidth":1024,"targetHeight":768});
    let g = compile(&i).unwrap();
    for gone in ["2", "3", "9", "X", "4"] {
        assert!(g.get(gone).is_none(), "{gone} left in the H3 graph");
    }
    assert_eq!(
        g["HE"]["inputs"],
        json!({"clip":["C",0],"vae":["V",0],"prompt":"a cat","width":1024,"height":768,"length":5,"first_frame":["IS",0]})
    );
    assert_eq!(
        g["SS"]["inputs"],
        json!({"model":["1",0],"shift_video":10,"shift_audio":3})
    );
    assert_eq!(
        g["B"]["inputs"],
        json!({"model":["SS",0],"conditioning":["HE",0]})
    );
    assert_eq!(
        g["Z"]["inputs"],
        json!({"model":["SS",0],"scheduler":"simple","steps":4,"denoise":1.0})
    );
    assert_eq!(g["5"]["inputs"]["latent_image"], json!(["HE", 1]));
    assert_eq!(g["6"]["inputs"]["samples"], json!(["SEP", 0]));
    assert_eq!(
        g["F"]["inputs"],
        json!({"image":["6",0],"batch_index":0,"length":1})
    );
    // Frame 0 replaces the image decode everywhere: rescaled to the crop, composited through the mask.
    assert_eq!(g["IR"]["inputs"]["image"], json!(["F", 0]));
    assert_eq!(g["Y"]["inputs"]["source"], json!(["IR", 0]));
    assert_eq!(g["7"]["inputs"]["images"], json!(["Y", 0]));
    // The metadata config must not reference the removed text encoders (ComfyUI: KeyError '2').
    let mut saver = g.clone();
    civitai_saver(
        &mut saver,
        &json!(["DaSiWa_MetadataImageSaver", "DaSiWa_MetadataConfig"]),
        &i,
    );
    assert_eq!(saver["M"]["inputs"]["node_positive"], json!(["HE", 0]));
    assert_eq!(saver["M"]["inputs"]["text_positive"], "a cat");
    assert!(saver["M"]["inputs"].get("node_negative").is_none());
    assert_eq!(saver["7"]["inputs"]["metadata_config"], json!(["M", 0]));
    let mut plain = i.clone();
    for k in ["inputMask", "maskBlur", "inpaintCrop"] {
        plain.as_object_mut().unwrap().remove(k);
    }
    plain["width"] = json!(1024);
    let g = compile(&plain).unwrap();
    assert_eq!(g["HE"]["inputs"]["first_frame"], json!(["8", 0]));
    assert_eq!(g["7"]["inputs"]["images"], json!(["F", 0]));
    let mut off = plain.clone();
    off["width"] = json!(1000);
    assert!(compile(&off).is_err(), "H3 canvas off its 32 px grid");
    for (k, v) in [
        ("h3Edit", Value::Null),
        ("sigmas", json!([1.0, 0.5, 0.2, 0.1, 0.0])),
        ("refine", json!({"width":1024,"height":768,"denoise":0.3})),
    ] {
        let mut bad = plain.clone();
        if v.is_null() {
            bad.as_object_mut().unwrap().remove(k);
        } else {
            bad[k] = v;
        }
        assert!(compile(&bad).is_err(), "accepted {k}");
    }
    let mut text = plain.clone();
    text.as_object_mut().unwrap().remove("inputImage");
    text.as_object_mut().unwrap().remove("denoise");
    assert!(
        compile(&text).is_err(),
        "H3 image recipe needs an input image"
    );
    let mut foreign = intent();
    foreign["h3Edit"] = json!({"shiftVideo":10,"shiftAudio":3});
    assert!(
        compile(&foreign).is_err(),
        "h3Edit outside the minimax encoder"
    );
}

#[test]
fn inpaint_crop_and_blur_use_model_resolution_and_restore_original_coordinates() {
    let mut i = intent();
    i["inputImage"] = json!("out.png [output]");
    i["denoise"] = json!(0.8);
    i["inputMask"] = json!("deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png");
    i["maskBlur"] = json!(8);
    i["inpaintCrop"] =
        json!({"x":40,"y":20,"width":120,"height":80,"targetWidth":1024,"targetHeight":1024});
    let g = compile(&i).unwrap();
    assert_eq!(
        g["IC"]["inputs"],
        json!({"image":["8",0],"x":40,"y":20,"width":120,"height":80})
    );
    assert_eq!(
        g["IS"]["inputs"],
        json!({"image":["IC",0],"upscale_method":"lanczos","width":1024,"height":1024,"crop":"disabled"})
    );
    assert_eq!(g["9"]["inputs"]["pixels"], json!(["IS", 0]));
    assert_eq!(
        g["MB"]["inputs"],
        json!({"image":["MI",0],"blur_radius":8,"sigma":1})
    );
    assert_eq!(g["MC"]["inputs"]["mask"], json!(["BM", 0]));
    assert_eq!(g["X"]["inputs"]["mask"], json!(["NM", 0]));
    assert_eq!(
        g["Y"]["inputs"],
        json!({"destination":["8",0],"source":["IR",0],"x":40,"y":20,"resize_source":false,"mask":["MC",0]})
    );
    assert_eq!(g["IR"]["inputs"]["width"], 120);
    assert_eq!(g["IR"]["inputs"]["height"], 80);
    for bad in [json!(-1), json!(1.5), json!(32), json!(65), Value::Null] {
        let mut v = i.clone();
        v["maskBlur"] = bad;
        assert!(compile(&v).is_err());
    }
    for (key, bad) in [
        ("x", json!(-1)),
        ("width", json!(9000)),
        ("targetWidth", json!(999)),
        ("extra", json!(1)),
    ] {
        let mut v = i.clone();
        v["inpaintCrop"][key] = bad;
        assert!(compile(&v).is_err());
    }
    let mut layered = i.clone();
    layered["inputLayers"] =
        json!([{"image":"out.png [output]","x":0,"y":0,"width":64,"height":64}]);
    layered["options"] = json!({"vaeTile":512});
    let layered = compile(&layered).unwrap();
    assert_eq!(layered["IC"]["inputs"]["image"], json!(["B0", 0]));
    assert_eq!(layered["Y"]["inputs"]["destination"], json!(["B0", 0]));
    assert_eq!(layered["9"]["class_type"], "VAEEncodeTiled");
    assert_eq!(layered["9"]["inputs"]["pixels"], json!(["IS", 0]));
    let mut no_mask = i.clone();
    no_mask.as_object_mut().unwrap().remove("inputMask");
    assert!(compile(&no_mask).is_err());
    i.as_object_mut().unwrap().remove("inpaintCrop");
    i["maskBlur"] = json!(0);
    let zero = compile(&i).unwrap();
    i.as_object_mut().unwrap().remove("maskBlur");
    assert_eq!(zero, compile(&i).unwrap());
}

#[tokio::test]
async fn prepare_requires_only_emitted_crop_and_blur_nodes() {
    let mut failures = vec![];
    for (crop, blur) in [(false, 0), (false, 8), (true, 0), (true, 8)] {
        let mut i = intent();
        i["inputImage"] = json!("ref.png");
        i["denoise"] = json!(0.8);
        i["inputMask"] = json!("deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png");
        i["maskBlur"] = json!(blur);
        if crop {
            i["inpaintCrop"] = json!({"x":40,"y":20,"width":120,"height":80,"targetWidth":1024,"targetHeight":1024});
        }
        let mut required = vec![
            "LoadImageMask",
            "SetLatentNoiseMask",
            "ImageCompositeMasked",
        ];
        if crop {
            required.extend(["ImageScale", "ImageCrop", "CropMask"]);
        }
        if crop || blur > 0 {
            required.extend(["MaskToImage", "ImageToMask"]);
        }
        if blur > 0 {
            required.push("ImageBlur");
        }
        for absent in std::iter::once(None).chain(required.iter().copied().map(Some)) {
            let mut info = nodes();
            for name in &required {
                info[*name] = json!({});
            }
            if let Some(name) = absent {
                info.as_object_mut().unwrap().remove(name);
            }
            let (base, task) = server(move |path, _| {
                assert_eq!(path, "GET /comfy/object_info HTTP/1.1");
                (200, info.to_string())
            })
            .await;
            let result = Jobs::default().prepare(base, i.clone()).await;
            task.abort();
            match (absent, result) {
                (None, Ok(_)) => {}
                (Some(name), Err(error)) if error.contains(name) => {}
                (absent, result) => failures.push(format!(
                    "crop={crop}, blur={blur}, absent={absent:?}: {result:?}"
                )),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn h3_session_logs_invalid_input_without_payloads() {
    for session in [
        "secret-session/credential".to_owned(),
        "secret-session\nforged".to_owned(),
        "é-secret-session".to_owned(),
        "s".repeat(81),
    ] {
        assert_eq!(
            native_h3_session(
                "http://secret-user:secret-pass@private.invalid/".into(),
                session
            )
            .await
            .unwrap_err(),
            "Invalid continuity session"
        );
    }
    let reports: Vec<_> = crate::logs::native_logs(0)
        .into_iter()
        .filter(|e| {
            e.text
                .starts_with("WARN: H3 continuity session-list: invalid session")
        })
        .collect();
    assert!(
        !reports.is_empty(),
        "validation failure must reach shared Logs"
    );
    assert!(
        reports.len() <= 2,
        "repeated invalid input must not flood Logs"
    );
    for entry in reports {
        assert_eq!(entry.source, crate::logs::Source::Deas);
        for secret in ["secret-", "private.invalid", "forged", "session=", "é"] {
            assert!(!entry.text.contains(secret), "{}", entry.text);
        }
    }
}

#[tokio::test]
async fn h3_session_logs_endpoint_http_and_shape_failures_without_secrets_or_flooding() {
    let endpoint_session = "log-endpoint-session";
    assert_eq!(
        native_h3_session(
            "http://secret-user:secret-pass@private.invalid/private-secret?secret-token".into(),
            endpoint_session.into(),
        )
        .await
        .unwrap_err(),
        "Backend URL must not contain credentials, a query, or a fragment"
    );
    let endpoint_logs: Vec<_> = crate::logs::native_logs(0)
        .into_iter()
        .filter(|e| e.text.contains(&format!("session={endpoint_session};")))
        .collect();
    assert_eq!(endpoint_logs.len(), 1);
    assert!(endpoint_logs[0]
        .text
        .starts_with("WARN: H3 continuity session-list: invalid backend endpoint"));

    let (base, task) = server(|path, _| {
        assert_eq!(path, "GET /comfy/df_h3_continuity/session/log-http-session HTTP/1.1");
        (503, json!({"error":"secret-body secret-token http://secret-user:secret-pass@private.invalid/"}).to_string())
    })
    .await;
    for _ in 0..6 {
        assert_eq!(
            native_h3_session(base.clone(), "log-http-session".into())
                .await
                .unwrap_err(),
            "Native job endpoint returned HTTP 503 Service Unavailable"
        );
    }
    task.abort();
    let request_logs: Vec<_> = crate::logs::native_logs(0)
        .into_iter()
        .filter(|e| e.text.contains("session=log-http-session;"))
        .collect();
    assert_eq!(
        request_logs.len(),
        2,
        "only first cause and first recurrence are immediate"
    );
    assert!(request_logs[0]
        .text
        .starts_with("ERROR: H3 continuity session-list: backend request failed"));
    assert!(request_logs[0].text.contains("repeated=0"));
    assert!(request_logs[1].text.contains("repeated=1"));

    let (base, task) = server(|_, _| {
        (
            200,
            json!({"clips":{"secret-body":"secret-token"}}).to_string(),
        )
    })
    .await;
    for _ in 0..6 {
        assert_eq!(
            native_h3_session(base.clone(), "log-shape-session".into())
                .await
                .unwrap_err(),
            "Malformed continuity session"
        );
    }
    task.abort();
    let shape_logs: Vec<_> = crate::logs::native_logs(0)
        .into_iter()
        .filter(|e| e.text.contains("session=log-shape-session;"))
        .collect();
    assert_eq!(shape_logs.len(), 2);
    assert!(shape_logs[0]
        .text
        .starts_with("ERROR: H3 continuity session-list: malformed response"));
    assert!(shape_logs[1].text.contains("repeated=1"));

    for entry in endpoint_logs.iter().chain(&request_logs).chain(&shape_logs) {
        assert_eq!(entry.source, crate::logs::Source::Deas);
        for secret in ["secret-", "private.invalid", "http://", "https://"] {
            assert!(!entry.text.contains(secret), "{}", entry.text);
        }
    }
}

#[tokio::test]
async fn h3_session_is_read_only_bounded_and_input_upload_checks_magic_bytes() {
    assert!(
        native_h3_session("http://127.0.0.1:1/".into(), "a/b".into())
            .await
            .is_err()
    );
    let (base, server) = server(|path, _| {
        assert_eq!(path, "GET /comfy/df_h3_continuity/session/deas-ab12 HTTP/1.1");
        (200, json!({"latest_id":"c2","ready_count":2,"clips":[
            {"clip_id":"c2","parent_id":"c1","frames":243,"completed_ns":20,"export":{"filename":"take_00002.webm","subfolder":"video","type":"output","kind":"full"},"status":"ready"},
            {"clip_id":"c1","parent_id":"","frames":124,"completed_ns":10,"export":{"filename":"../x.webm","subfolder":"","type":"output"}},
            {"clip_id":"../evil","frames":1,"completed_ns":1}]}).to_string())
    })
    .await;
    let value = native_h3_session(base, "deas-ab12".into()).await.unwrap();
    assert_eq!(
        value,
        json!({"latestId":"c2","clips":[
        {"clipId":"c2","parent":"c1","frames":243,"completed":20,"export":{"filename":"take_00002.webm","subfolder":"video","type":"output"}},
        {"clipId":"c1","parent":null,"frames":124,"completed":10,"export":null}]})
    );
    server.abort();
    let dir = tempfile::tempdir().unwrap();
    let gif = dir.path().join("a.gif");
    std::fs::write(&gif, b"GIF89a").unwrap();
    assert!(
        native_input_upload("http://127.0.0.1:1/".into(), gif.to_str().unwrap().into())
            .await
            .is_err()
    );
    assert!(native_input_upload(
        "http://127.0.0.1:1/".into(),
        dir.path().to_str().unwrap().into()
    )
    .await
    .is_err());
}

#[tokio::test]
async fn local_video_upload_cancel_validation_and_logs() {
    let base = "http://127.0.0.1:1/";
    let before_cancel = crate::logs::native_logs(0)
        .into_iter()
        .filter(|e| e.text.starts_with("ERROR: Native input-upload:"))
        .count();
    assert_eq!(selected_video_upload(base, None).await.unwrap(), None);
    assert_eq!(
        crate::logs::native_logs(0)
            .into_iter()
            .filter(|e| e.text.starts_with("ERROR: Native input-upload:"))
            .count(),
        before_cancel
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("secret-local-video.mp4");
    std::fs::write(&path, b"\x89PNG\r\n\x1a\n").unwrap();
    assert!(selected_video_upload(base, Some(path.clone()))
        .await
        .unwrap_err()
        .contains("video"));
    std::fs::write(&path, b"\0\0\0\x18ftypisom").unwrap();
    let before = std::fs::read(&path).unwrap();
    for _ in 0..5 {
        assert!(selected_video_upload(base, Some(path.clone()))
            .await
            .is_err());
    }
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let logs: Vec<_> = crate::logs::native_logs(0)
        .into_iter()
        .filter(|e| e.text.starts_with("ERROR: Native input-upload:"))
        .collect();
    assert!(!logs.is_empty());
    assert!(logs.len() <= 2);
    assert!(logs.iter().all(|e| e.source == crate::logs::Source::Deas
        && !e.text.contains("secret")
        && !e.text.contains("http")));
}

#[tokio::test]
async fn local_video_upload_posts_video_only_and_preserves_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Original 电影.mov");
    let original = b"\0\0\0\x14ftypqt  fixture";
    std::fs::write(&path, original).unwrap();
    for outcome in ["success", "http", "tampered", "oversized"] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/comfy/", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            while !bytes.ends_with(b"--\r\n") {
                let mut buf = [0; 4096];
                let n = socket.read(&mut buf).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
            }
            let text = String::from_utf8(bytes).unwrap();
            assert!(text.starts_with("POST /comfy/upload/image HTTP/1.1"));
            assert!(text.contains("name=\"subfolder\"\r\n\r\n\r\n"));
            assert!(text.contains("name=\"overwrite\"\r\n\r\nfalse\r\n"));
            assert!(text.contains("name=\"type\"\r\n\r\ninput\r\n"));
            assert!(!text.contains("Original"));
            let name = text
                .split("filename=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_owned();
            assert!(name.starts_with("deas-input-") && name.ends_with(".mov"));
            let body = match outcome {
                "tampered" => json!({"name":"../secret.mov","subfolder":""}).to_string(),
                "oversized" => format!(
                    "{}{}",
                    " ".repeat(RESPONSE_LIMIT + 1),
                    json!({"name":name,"subfolder":""})
                ),
                _ => json!({"name":name,"subfolder":""}).to_string(),
            };
            let status = if outcome == "http" {
                "503 Service Unavailable"
            } else {
                "200 OK"
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = socket.write_all(response.as_bytes()).await;
            name
        });
        let result = selected_video_upload(&base, Some(path.clone())).await;
        let name = task.await.unwrap();
        if outcome == "success" {
            assert_eq!(result.unwrap(), Some(name));
        } else {
            assert!(result.is_err(), "{outcome}");
        }
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len((512 << 20) + 1)
        .unwrap();
    assert!(selected_video_upload("http://127.0.0.1:1/", Some(path))
        .await
        .unwrap_err()
        .contains("512 MiB"));
}

#[test]
fn reference_files_are_sniffed_by_magic_bytes() {
    let mut mp4 = vec![0, 0, 0, 0x18];
    mp4.extend(b"ftypisom");
    let mut mov = vec![0, 0, 0, 0x14];
    mov.extend(b"ftypqt  ");
    let mut m4a = vec![0, 0, 0, 0x1c];
    m4a.extend(b"ftypM4A ");
    let mut webm = b"\x1a\x45\xdf\xa3\x9f\x42\x82\x84".to_vec();
    webm.extend(b"webm");
    for (head, want) in [
        (b"\x89PNG\r\n\x1a\n".to_vec(), Some(("image", "png"))),
        (b"RIFF\0\0\0\0WEBPVP8 ".to_vec(), Some(("image", "webp"))),
        (b"RIFF\0\0\0\0WAVEfmt ".to_vec(), Some(("audio", "wav"))),
        (mp4, Some(("video", "mp4"))),
        (mov, Some(("video", "mov"))),
        (m4a, Some(("audio", "m4a"))),
        (webm, Some(("video", "webm"))),
        (
            b"\x1a\x45\xdf\xa3\x9f\x42\x82\x88matroska".to_vec(),
            Some(("video", "mkv")),
        ),
        (b"ID3\x04".to_vec(), Some(("audio", "mp3"))),
        (b"fLaC".to_vec(), Some(("audio", "flac"))),
        (b"OggS".to_vec(), Some(("audio", "ogg"))),
        (b"GIF89a".to_vec(), None),
        (vec![], None),
    ] {
        assert_eq!(crate::jobs::sniff_media(&head), want);
    }
}

#[tokio::test]
async fn mask_upload_posts_a_fresh_png_name_into_the_mask_folder() {
    assert!(
        native_mask_upload("http://127.0.0.1:1/".into(), b"GIF89a".to_vec())
            .await
            .is_err()
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/comfy", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = vec![];
        while !bytes.ends_with(b"--\r\n") {
            let mut buf = [0; 4096];
            let n = socket.read(&mut buf).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        assert!(
            text.starts_with("POST /comfy/upload/image HTTP/1.1"),
            "{text}"
        );
        let name = text
            .split("filename=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .to_owned();
        assert!(
            text.contains("name=\"subfolder\"\r\n\r\ndeas-masks\r\n")
                && text.contains("\x0c\x0d\x0e")
        );
        let body = json!({"name":name,"subfolder":"deas-masks","type":"input"}).to_string();
        socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        name
    });
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend(b"\x0c\x0d\x0e");
    let stored = native_mask_upload(base, png).await.unwrap();
    assert_eq!(stored, format!("deas-masks/{}", task.await.unwrap()));
    assert!(mask_name(&stored));
}

#[test]
fn input_layers_paste_scaled_images_onto_the_source_before_resize_and_inpaint() {
    let mut with = intent();
    with["inputImage"] = json!("src.png [output]");
    with["denoise"] = json!(0.6);
    with["inputLayers"] = json!([{"image":"cat.png","x":10,"y":20,"width":100,"height":50},{"image":"dog.png [output]","x":0,"y":0,"width":8,"height":8}]);
    let graph = compile(&with).unwrap();
    assert_eq!(graph["R0"]["inputs"]["image"], json!(["I0", 0]));
    assert_eq!(
        graph["B0"]["inputs"],
        json!({"destination":["8",0],"source":["R0",0],"x":10,"y":20,"resize_source":false})
    );
    assert_eq!(graph["B1"]["inputs"]["destination"], json!(["B0", 0]));
    assert_eq!(graph["9"]["inputs"]["pixels"], json!(["B1", 0]));
    let mut resized = with.clone();
    resized["inputResize"] = json!({"width":512,"height":768,"crop":"disabled"});
    assert_eq!(
        compile(&resized).unwrap()["S"]["inputs"]["image"],
        json!(["B1", 0])
    );
    let mut masked = with.clone();
    masked["inputMask"] = json!("deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png");
    assert_eq!(
        compile(&masked).unwrap()["Y"]["inputs"]["destination"],
        json!(["B1", 0])
    );
    for bad in [
        json!([]),
        json!([{"image":"../x.png","x":0,"y":0,"width":8,"height":8}]),
        json!([{"image":"x.png","x":0,"y":0,"width":0,"height":8}]),
        json!([{"image":"x.png","x":-1,"y":0,"width":8,"height":8}]),
        json!([{"image":"x.png","x":0,"y":0,"width":8}]),
        json!(vec![
            json!({"image":"x.png","x":0,"y":0,"width":8,"height":8});
            5
        ]),
    ] {
        let mut v = with.clone();
        v["inputLayers"] = bad;
        assert!(compile(&v).is_err(), "accepted {v}");
    }
    let mut no_image = intent();
    no_image["inputLayers"] = with["inputLayers"].clone();
    assert!(compile(&no_image).is_err());
}

#[tokio::test]
async fn prepare_checks_input_layers_against_the_backend_list() {
    let mut list = nodes();
    list["ImageScale"] = json!({});
    list["ImageCompositeMasked"] = json!({});
    list["LoadImageMask"] = json!({});
    list["SetLatentNoiseMask"] = json!({});
    let (base, task) = server(move |_, _| (200, list.to_string())).await;
    let jobs = Jobs::default();
    let mut with = intent();
    with["inputImage"] = json!("ref.png");
    with["denoise"] = json!(0.6);
    // The uploaded mask is not in object_info's top-level input list; ComfyUI checks it.
    with["inputMask"] = json!("deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png");
    with["inputLayers"] = json!([{"image":"my ref ä.png","x":0,"y":0,"width":8,"height":8},{"image":"DEAS/out.png [output]","x":0,"y":0,"width":8,"height":8}]);
    assert!(jobs.prepare(base.clone(), with.clone()).await.is_ok());
    // A flattened painted layer carries the mask-upload name; only the exact uuid form passes.
    with["inputLayers"][0]["image"] = json!("deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png");
    assert!(jobs.prepare(base.clone(), with.clone()).await.is_ok());
    with["inputLayers"][0]["image"] = json!("deas-masks/other.png");
    assert!(jobs.prepare(base.clone(), with.clone()).await.is_err());
    with["inputLayers"][0]["image"] = json!("missing.png");
    assert!(jobs.prepare(base, with).await.is_err());
    task.abort();
}

/// `DEAS_LIVE_COMFY=<url> DEAS_LIVE_MASK=<png> cargo test --lib live_mask_upload -- --ignored --nocapture` (throwaway backend only).
#[tokio::test]
#[ignore]
async fn live_mask_upload() {
    let base = std::env::var("DEAS_LIVE_COMFY").unwrap();
    let png = std::fs::read(std::env::var("DEAS_LIVE_MASK").unwrap()).unwrap();
    println!("{}", native_mask_upload(base, png).await.unwrap());
}

#[test]
fn image_compile_rejects_video_only_families() {
    let mut i = intent();
    i["family"] = json!("minimax-h3-hyperflow");
    assert!(
        compile(&i).unwrap_err().contains("video-only"),
        "top-level family reached the image graph"
    );
    let mut i = intent();
    i["diffusion"] = json!({"textEncoder":"e.safetensors","clipType":"stable_diffusion","vae":"v.safetensors","family":"minimax-h3-hyperflow"});
    let e = compile(&i).unwrap_err();
    assert!(e.contains("video-only"), "diffusion.family: {e}");
}

#[test]
fn temporal_latent_tiled_refine_direct_intents_are_rejected() {
    for (family, clip_type) in [("anima", "stable_diffusion"), ("krea-2", "krea2")] {
        for selection in ["family", "diffusion", "clipType"] {
            let mut i = intent();
            i["diffusion"] = json!({"textEncoder":"clip.safetensors","clipType":clip_type,"vae":"vae.safetensors"});
            if selection == "family" {
                i["family"] = json!(family);
                i["diffusion"]["family"] = json!("sdxl");
            } else if selection == "diffusion" {
                i["diffusion"]["family"] = json!(family);
            }
            i["refine"] = json!({"width":1024,"height":1536,"denoise":0.3});
            assert!(compile(&i).is_ok(), "untiled {family}/{selection}");
            i["refine"]["tiled"] = json!({"tileWidth":512,"tileHeight":768,"overlap":64});
            assert!(
                compile(&i).unwrap_err().contains("temporal latent"),
                "{family}/{selection}"
            );
        }
    }
    for family in ["anima", "krea-2", "minimax-h3"] {
        let mut i = intent();
        i["family"] = json!(family);
        i["refine"] = json!({"width":1024,"height":1536,"denoise":0.3,"tiled":{"tileWidth":512,"tileHeight":768,"overlap":64}});
        assert!(compile(&i).unwrap_err().contains("temporal latent"));
        i["steps"] = json!(2);
        i["sigmas"] = json!([1.0, 0.5, 0.0]);
        i["refine"]["sigmas"] = json!([0.5, 0.0]);
        assert!(compile(&i).unwrap_err().contains("temporal latent"));
        if family != "minimax-h3" {
            i["refine"].as_object_mut().unwrap().remove("tiled");
            assert_eq!(
                compile(&i).unwrap()["R5"]["class_type"],
                "SamplerCustomAdvanced"
            );
        }
    }
    // An explicit SDXL selection wins over a conflicting diffusion-family hint.
    let mut i = intent();
    i["family"] = json!("sdxl");
    i["diffusion"] = json!({"family":"anima","textEncoder":"clip.safetensors","clipType":"stable_diffusion","vae":"vae.safetensors"});
    i["refine"] = json!({"width":1024,"height":1536,"denoise":0.3,"tiled":{"tileWidth":512,"tileHeight":768,"overlap":64}});
    assert_eq!(
        compile(&i).unwrap()["RT"]["class_type"],
        "DaSiWaTiledDiffusion"
    );
    // H3 already rejects every refine pass, including direct tiled requests.
    i["family"] = json!("minimax-h3");
    i["diffusion"]["clipType"] = json!("minimax");
    i["diffusion"]["family"] = json!("minimax-h3");
    i["inputImage"] = json!("ref.png");
    i["denoise"] = json!(0.5);
    i["h3Edit"] = json!({"shiftVideo":3,"shiftAudio":3});
    assert!(compile(&i)
        .unwrap_err()
        .contains("refine pass are not supported"));
}

#[tokio::test]
async fn temporal_latent_tiled_refine_prepare_rejects_before_backend_access() {
    let jobs = Jobs::default();
    for family in ["anima", "krea-2", "minimax-h3"] {
        let mut i = intent();
        i["family"] = json!(family);
        i["refine"] = json!({"width":1024,"height":1536,"denoise":0.3,"tiled":{"tileWidth":512,"tileHeight":768,"overlap":64}});
        // A valid but unbound endpoint proves rejection happens before capability discovery.
        let error = jobs
            .prepare("http://127.0.0.1:1".into(), i)
            .await
            .unwrap_err();
        assert!(error.contains("temporal latent"), "{family}: {error}");
        assert!(jobs.lock().unwrap().plan.is_none());
    }
}

#[test]
fn refine_pass_upscales_resizes_and_resamples_after_the_first_decode() {
    let mut i = intent();
    i["refine"] =
        json!({"width":1024,"height":1536,"denoise":0.3,"upscaleModel":"2x-Sharp.safetensors"});
    let g = compile(&i).unwrap();
    assert_eq!(g["RU"]["inputs"]["image"], json!(["6", 0]));
    assert_eq!(g["RL"]["inputs"]["model_name"], "2x-Sharp.safetensors");
    assert_eq!(
        g["RS"]["inputs"],
        json!({"image":["RU",0],"upscale_method":"lanczos","width":1024,"height":1536,"crop":"disabled"})
    );
    assert_eq!(g["RE"]["inputs"]["pixels"], json!(["RS", 0]));
    assert_eq!(g["R5"]["inputs"]["latent_image"], json!(["RE", 0]));
    assert_eq!(g["R5"]["inputs"]["denoise"], json!(0.3));
    assert_eq!(g["R5"]["inputs"]["seed"], json!(123));
    assert_eq!(g["7"]["inputs"]["images"], json!(["PG", 0]));
    assert_eq!(
        g["PG"],
        json!({"class_type":"DEAS_ImagePostprocessGate","inputs":{"baseline":["B7",0],"image":["R6",0]}})
    );
    // Same size without a model: no resample of the first pass.
    i["refine"] = json!({"width":512,"height":768,"denoise":0.3});
    let g = compile(&i).unwrap();
    assert!(g.get("RS").is_none() && g.get("RL").is_none());
    assert_eq!(g["RE"]["inputs"]["pixels"], json!(["6", 0]));
    // Model-fixed schedules carry their own refine tail.
    i["steps"] = json!(2);
    i["sigmas"] = json!([1.0, 0.5, 0.0]);
    assert!(compile(&i).is_err());
    i["refine"]["sigmas"] = json!([0.5, 0.0]);
    let g = compile(&i).unwrap();
    assert_eq!(g["RZ"]["inputs"]["sigmas"], "0.5, 0");
    assert_eq!(g["R5"]["class_type"], "SamplerCustomAdvanced");
    for (k, bad) in [
        ("width", json!(1001)),
        ("denoise", json!(0)),
        ("upscaleModel", json!("../x.pth")),
        ("extra", json!(1)),
        (
            "tiled",
            json!({"tileWidth":1024,"tileHeight":1024,"overlap":1024}),
        ),
        (
            "tiled",
            json!({"tileWidth":1024,"tileHeight":1024,"overlap":12}),
        ),
        (
            "tiled",
            json!({"tileWidth":1004,"tileHeight":1024,"overlap":64}),
        ),
        ("tiled", json!({"tileWidth":1024,"tileHeight":1024})),
    ] {
        let mut v = i.clone();
        v["refine"][k] = bad;
        assert!(compile(&v).is_err(), "accepted {k}");
    }
    // Tiled refine: the DaSiWa tiling patch feeds only the refine sampler (own guider on the sigmas path).
    i["refine"]["tiled"] = json!({"tileWidth":1024,"tileHeight":1024,"overlap":128});
    let g = compile(&i).unwrap();
    assert_eq!(
        g["RT"]["inputs"],
        json!({"model":g["G"]["inputs"]["model"],"tile_width":1024,"tile_height":1024,"overlap":128})
    );
    assert_eq!(g["RG"]["inputs"]["model"], json!(["RT", 0]));
    assert_eq!(g["R5"]["inputs"]["guider"], json!(["RG", 0]));
    assert_eq!(g["5"]["inputs"]["guider"], json!(["G", 0]));
    i.as_object_mut().unwrap().remove("sigmas");
    i["refine"].as_object_mut().unwrap().remove("sigmas");
    let g = compile(&i).unwrap();
    assert_eq!(g["RT"]["inputs"]["model"], g["5"]["inputs"]["model"]);
    assert_eq!(g["R5"]["inputs"]["model"], json!(["RT", 0]));
    assert!(g.get("RG").is_none());
}

#[test]
fn upscale_model_runs_on_the_source_before_the_exact_resize() {
    let mut i = intent();
    i["inputImage"] = json!("ref.png");
    i["denoise"] = json!(0.35);
    i["upscaleModel"] = json!("2x-Sharp.safetensors");
    assert!(compile(&i).is_err(), "upscaleModel needs inputResize");
    i["inputResize"] = json!({"width":512,"height":768,"crop":"disabled"});
    let g = compile(&i).unwrap();
    assert_eq!(g["UM"]["inputs"]["image"], json!(["8", 0]));
    assert_eq!(g["S"]["inputs"]["image"], json!(["UM", 0]));
    assert_eq!(g["9"]["inputs"]["pixels"], json!(["S", 0]));
}

fn image_postprocess_nodes() -> Value {
    let mut info = nodes();
    info["SaveImage"]["output"] = json!(["IMAGE"]);
    info["DEAS_ImagePostprocessGate"] = json!({"input":{"required":{"baseline":["IMAGE,STRING"],"image":["IMAGE",{"lazy":true}]}},"output":["IMAGE"]});
    info
}

#[tokio::test]
async fn image_postprocess_prepare_rechecks_effective_saver_without_blocking_plain_jobs() {
    for metadata in [false, true] {
        for compatible in [false, true] {
            let mut info = image_postprocess_nodes();
            if metadata {
                info["DaSiWa_MetadataConfig"] = json!({});
                info["DaSiWa_MetadataImageSaver"] =
                    json!({"output":if compatible {json!(["STRING"])} else {json!([])}});
            } else if !compatible {
                info["SaveImage"]["output"] = json!([]);
            }
            let (base, task) = server(move |path, _| {
                assert_eq!(path, "GET /comfy/object_info HTTP/1.1");
                (200, info.to_string())
            })
            .await;
            let jobs = Jobs::default();
            assert!(jobs.prepare(base.clone(), intent()).await.is_ok());
            let mut i = intent();
            i["refine"] = json!({"width":512,"height":768,"denoise":0.3});
            let result = jobs.prepare(base, i).await;
            if compatible {
                result.unwrap();
                let session = jobs.lock().unwrap();
                let g = &session.plan.as_ref().unwrap().graph;
                assert_eq!(g["PG"]["inputs"]["baseline"], json!(["B7", 0]));
                assert_eq!(
                    g["B7"]["class_type"],
                    if metadata {
                        "DaSiWa_MetadataImageSaver"
                    } else {
                        "SaveImage"
                    }
                );
            } else {
                assert!(result.unwrap_err().contains("baseline-first lazy gate"));
            }
            task.abort();
        }
    }
}

#[tokio::test]
async fn prepare_checks_upscale_models_against_the_v3_backend_list() {
    let mut list = image_postprocess_nodes();
    list["UpscaleModelLoader"] = json!({"input":{"required":{"model_name":["COMBO",{"multiselect":false,"options":["2x-Sharp.safetensors"]}]}}});
    list["ImageUpscaleWithModel"] = json!({});
    list["ImageScale"] = json!({});
    let (base, task) = server(move |_, _| (200, list.to_string())).await;
    let jobs = Jobs::default();
    let mut with = intent();
    with["refine"] =
        json!({"width":1024,"height":1536,"denoise":0.3,"upscaleModel":"2x-Sharp.safetensors"});
    assert!(jobs.prepare(base.clone(), with.clone()).await.is_ok());
    with["refine"]["upscaleModel"] = json!("missing.pth");
    assert!(jobs.prepare(base, with).await.is_err());
    task.abort();
}

#[tokio::test]
async fn prepare_validates_input_image_against_the_backend_list() {
    let (base, task) = server(|_, _| (200, nodes().to_string())).await;
    let jobs = Jobs::default();
    let mut with = intent();
    with["inputImage"] = json!("ref.png");
    with["denoise"] = json!(0.6);
    assert!(jobs.prepare(base.clone(), with.clone()).await.is_ok());
    with["inputImage"] = json!("my ref ä.png"); // user file names with spaces/umlauts work
    assert!(jobs.prepare(base.clone(), with.clone()).await.is_ok());
    with["inputImage"] = json!("DEAS/out.png [output]"); // output images are resolved by ComfyUI, not object_info
    assert!(jobs.prepare(base.clone(), with.clone()).await.is_ok());
    with["inputImage"] = json!("missing.png");
    assert!(jobs.prepare(base, with).await.is_err());
    task.abort();
}

#[tokio::test]
async fn prepare_requires_fresh_native_prerequisites_and_replaces_only_unsubmitted_plan() {
    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = count.clone();
    let (base, task) = server(move |path, _| {
        assert_eq!(path, "GET /comfy/object_info HTTP/1.1");
        observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        (200, nodes().to_string())
    })
    .await;
    let jobs = Jobs::default();
    let first = jobs.prepare(base.clone(), intent()).await.unwrap();
    let second = jobs.prepare(base.clone(), intent()).await.unwrap();
    assert_ne!(first["id"], second["id"]);
    assert!(jobs
        .submit(first["id"].as_str().unwrap().into(), true)
        .await
        .is_err());
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
    task.abort();
    for field in ["node", "model", "sampler", "scheduler"] {
        let mut info = nodes();
        match field {
            "node" => {
                info.as_object_mut().unwrap().remove("SaveImage");
            }
            "model" => {
                info["CheckpointLoaderSimple"]["input"]["required"]["ckpt_name"][0] = json!([])
            }
            "sampler" => info["KSampler"]["input"]["required"]["sampler_name"][0] = json!([]),
            _ => info["KSampler"]["input"]["required"]["scheduler"][0] = json!([]),
        }
        let (base, task) = server(move |path, _| {
            assert!(path.starts_with("GET "));
            (200, info.to_string())
        })
        .await;
        assert!(
            Jobs::default().prepare(base, intent()).await.is_err(),
            "{field}"
        );
        task.abort();
    }
    for url in [
        "http://user:secret@localhost:9",
        "http://@localhost:9",
        "http://localhost:9/?token=x",
        "http://localhost:9/#x",
        "file:///etc/passwd",
    ] {
        assert!(jobs.prepare(url.into(), intent()).await.is_err());
    }
}

#[tokio::test]
async fn post_ambiguity_keeps_native_id_without_retry_or_success_claim() {
    for (code,reply,expected) in [
        (0,String::new(),"unknown"), (302,"{}".into(),"unknown"),
        (200,"<html>not comfy</html>".into(),"unknown"), (200,"{broken".into(),"unknown"),
        (500,"{}".into(),"unknown"), (401,"{}".into(),"unknown"),
        (200,json!({"prompt_id":"../unsafe","number":1,"node_errors":{}}).to_string(),"unknown"),
        (200,json!({"prompt_id":PROMPT_ID.to_uppercase(),"number":1,"node_errors":{}}).to_string(),"unknown"),
        (200,json!({"prompt_id":PROMPT_ID,"number":-1,"node_errors":{}}).to_string(),"unknown"),
        (200,json!({"prompt_id":PROMPT_ID,"number":1}).to_string(),"unknown"),
        (400,json!({"error":{"message":"invalid prompt"},"node_errors":{"5":{"errors":["bad model"]}}}).to_string(),"failed"),
        (200,json!({"prompt_id":PROMPT_ID,"number":1,"node_errors":{"5":{"errors":["invalid"]}}}).to_string(),"failed"),
        (200," ".repeat(RESPONSE_LIMIT+1),"unknown"),
    ] {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = count.clone();
        let (base,task) = server(move |path,_| {
            if path == "GET /comfy/object_info HTTP/1.1" {return (200,nodes().to_string());}
            assert_eq!(path,"POST /comfy/prompt HTTP/1.1");
            observed.fetch_add(1,std::sync::atomic::Ordering::SeqCst);
            (code,reply.clone())
        }).await;
        let jobs = Jobs::default();
        let plan = jobs.prepare(base,intent()).await.unwrap();
        let plan_id = plan["id"].as_str().unwrap().to_owned();
        let job = jobs.submit(plan_id.clone(),true).await.unwrap();
        assert_eq!(job["state"],expected,"{job}");
        assert!(job["promptId"].is_null());
        assert!(canonical_uuid(job["id"].as_str().unwrap()));
        assert_eq!(job["outputs"],json!([]));
        assert_eq!(job["generationReady"],false);
        assert!(job["error"].as_str().unwrap().chars().count() <= 800);
        assert_eq!(jobs.status(job["id"].as_str().unwrap().into()).await.unwrap(),job);
        assert!(jobs.submit(plan_id,true).await.is_err());
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst),1);
        task.abort();
    }
}

#[tokio::test]
async fn concurrent_submit_reserves_job_before_any_network_await() {
    let (base, task) = server(|path, _| {
        if path.starts_with("GET ") {
            (200, nodes().to_string())
        } else {
            (
                400,
                json!({"error":{"message":"validation failed"},"node_errors":{}}).to_string(),
            )
        }
    })
    .await;
    let jobs = Jobs::default();
    let plan = jobs.prepare(base.clone(), intent()).await.unwrap();
    let id = plan["id"].as_str().unwrap().to_owned();
    let (first, duplicate) =
        tokio::join!(jobs.submit(id.clone(), true), jobs.submit(id.clone(), true));
    assert!(first.is_ok());
    assert!(duplicate.is_err());
    assert_eq!(first.unwrap()["state"], "failed");
    assert!(jobs.submit(id, true).await.is_err());
    let next = jobs.prepare(base, intent()).await.unwrap();
    assert_ne!(plan["id"], next["id"]);
    task.abort();
}

#[tokio::test]
async fn prepare_latent_upscale_requires_exact_catalog_id_without_changing_legacy_admission() {
    for mut intent in [
        crate::jobs_video::tests::ltx(),
        crate::jobs_video::tests::h3(),
    ] {
        let ltx = intent["family"] == "ltx-2";
        let filename = if ltx {
            "ltx-2.3-spatial-upscaler-x2-1.1.safetensors"
        } else {
            "minimax_h3_latent_upscaler_3d_conv_v1_bf16.safetensors"
        };
        for separator in ["/", "\\"] {
            let exact = format!("nested{separator}{filename}");
            intent["latentUpscale"] = if ltx {
                json!({"model":exact,"scale":2})
            } else {
                json!({"model":exact,"scale":1.5,"steps":2,"denoise":0.2})
            };
            if ltx {
                intent["ltx"]["passes"] = json!(2);
                intent["ltx"]["spatialUpscaler"] = json!(r"legacy\old.safetensors");
            }
            let graph = compile(&intent).unwrap();
            let mut info = nodes();
            for node in graph.as_object().unwrap().values() {
                let class = node["class_type"].as_str().unwrap();
                if info.get(class).is_none() {
                    info[class] = json!({});
                }
            }
            for (field, names) in crate::jobs_video::requirements(&intent) {
                let (node, input) = match field {
                    "diffusionModels" => ("UNETLoader", "unet_name"),
                    "ggufDiffusionModels" => ("UnetLoaderGGUF", "unet_name"),
                    "textEncoders" => ("CLIPLoader", "clip_name"),
                    "clipTypes" => ("CLIPLoader", "type"),
                    "vaes" => ("VAELoader", "vae_name"),
                    "images" => ("LoadImage", "image"),
                    "latentUpscaleModels" => ("LatentUpscaleModelLoader", "model_name"),
                    "h3LatentUpscaleModels" => ("DaSiWaH3TiledUpscale", "upscale_model"),
                    _ => unreachable!("{field}"),
                };
                let names: Vec<_> = names
                    .iter()
                    .map(|name| {
                        if *name == r"legacy\old.safetensors" {
                            "legacy/old.safetensors"
                        } else {
                            *name
                        }
                    })
                    .collect();
                info[node]["input"]["required"][input] = json!([names]);
            }
            info["KSampler"]["input"]["required"]["sampler_name"] = json!([[intent["sampler"]]]);
            info["KSampler"]["input"]["required"]["scheduler"] = json!([[intent["scheduler"]]]);
            // LTX compiles DualCLIPLoader (Gemma + text projection); the prerequisites list its combo.
            if graph["C"]["class_type"] == "DualCLIPLoader" {
                info["DualCLIPLoader"]["input"]["required"]["clip_name1"] = json!([[
                    graph["C"]["inputs"]["clip_name1"],
                    graph["C"]["inputs"]["clip_name2"]
                ]]);
                info["DualCLIPLoader"]["input"]["required"]["type"] =
                    json!([[graph["C"]["inputs"]["type"]]]);
            }
            let (base, task) = server(move |path, _| {
                assert_eq!(
                    path, "GET /comfy/object_info HTTP/1.1",
                    "prepare must never submit"
                );
                (200, info.to_string())
            })
            .await;
            let jobs = Jobs::default();
            let prepared = jobs.prepare(base.clone(), intent.clone()).await.unwrap();
            assert_eq!(prepared["intent"]["latentUpscale"]["model"], exact);
            let mut mismatched = intent.clone();
            mismatched["latentUpscale"]["model"] =
                json!(exact.replace(separator, if separator == "/" { "\\" } else { "/" }));
            let result = jobs.prepare(base, mismatched).await;
            assert!(
                result.is_err(),
                "accepted separator-different {exact}: {result:?}"
            );
            assert!(jobs.lock().unwrap().plan.is_none());
            assert!(jobs.lock().unwrap().job.is_none());
            task.abort();
        }
    }
}

#[test]
fn paired_output_roles_are_graph_owned_final_first_and_bounded() {
    let image =
        json!({"filename":"not-a-role.png","subfolder":"DEAS","type":"output","role":"spoofed"});
    let entry = json!({"outputs":{"7":{"images":[image]},"B7":{"images":[image]},"foreign":{"images":[image]}}});
    let plain = compile(&intent()).unwrap();
    assert_eq!(
        job_outputs(&entry, &plain).unwrap(),
        vec![json!({"filename":"not-a-role.png","subfolder":"DEAS","type":"output"})]
    );
    let mut i = intent();
    i["refine"] = json!({"width":512,"height":768,"denoise":0.3});
    let graph = compile(&i).unwrap();
    for listed in [
        json!([]),
        json!(["DaSiWa_MetadataImageSaver"]),
        json!(["DaSiWa_MetadataConfig"]),
    ] {
        let mut fallback = graph.clone();
        civitai_saver(&mut fallback, &listed, &i);
        assert_eq!(fallback["7"]["class_type"], "SaveImage");
        assert_eq!(fallback["B7"]["class_type"], "SaveImage");
        assert!(fallback.get("M").is_none());
    }
    let parsed = job_outputs(&entry, &graph).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0]["role"], "processed");
    assert_eq!(parsed[1]["role"], "original");
    for node in ["7", "B7"] {
        let mut bad = entry.clone();
        bad["outputs"][node]["images"][0]["filename"] = json!("../unsafe.png");
        assert!(job_outputs(&bad, &graph).is_err());
        bad["outputs"][node]["images"] = json!(false);
        assert!(job_outputs(&bad, &graph).is_err());
    }
    let oversized =
        json!({"outputs":{"7":{"images":vec![image.clone();33]},"B7":{"images":vec![image;32]}}});
    assert!(job_outputs(&oversized, &graph).is_err());
    i["outputPrefix"] = json!("x".repeat(172));
    assert!(compile(&i).unwrap_err().contains("prefix"));
    i["outputPrefix"] = json!("x".repeat(171));
    assert!(compile(&i).is_ok());
}

#[test]
fn paired_video_outputs_keep_processed_video_primary_and_validate_both_paths() {
    let graph = json!({
        "7":{"class_type":"DaSiWa_EnhancedVideoCombine","inputs":{}},
        "B7":{"class_type":"DaSiWa_EnhancedVideoCombine","inputs":{}}
    });
    let saver = |node: &str| {
        json!({
            "images":[{"filename":format!("{node}_first.png"),"subfolder":"video/run-1","type":"output","role":"spoofed"}],
            "gifs":[{"filename":format!("{node}.webm"),"subfolder":"video/run-1","type":"output","format":"video/webm","role":"spoofed"}]
        })
    };
    let entry = json!({"outputs":{"7":saver("7"),"B7":saver("B7"),"foreign":saver("foreign")}});
    let parsed = job_outputs(&entry, &graph).unwrap();
    assert_eq!(parsed.len(), 4);
    assert_eq!(parsed[0]["filename"], "7.webm");
    assert_eq!(parsed[0]["format"], "video/webm");
    assert_eq!(parsed[0]["role"], "processed");
    assert_eq!(
        parsed.iter().filter(|o| o["role"] == "processed").count(),
        2
    );
    assert_eq!(parsed.iter().filter(|o| o["role"] == "original").count(), 2);
    assert!(parsed
        .iter()
        .any(|o| o["filename"] == "B7.webm" && o["role"] == "original"));
    // An unpaired legacy combine must retain its existing frame-first indices.
    let legacy = job_outputs(&entry, &json!({"7":graph["7"]})).unwrap();
    assert_eq!(legacy, outputs(&entry).unwrap());
    assert_eq!(legacy[0]["filename"], "7_first.png");
    assert!(legacy.iter().all(|o| o.get("role").is_none()));
    for node in ["7", "B7"] {
        for field in ["images", "gifs"] {
            for (key, value) in [
                ("filename", json!("../unsafe.webm")),
                ("subfolder", json!(r"video\..\private")),
                ("type", json!("temp")),
            ] {
                let mut unsafe_entry = entry.clone();
                unsafe_entry["outputs"][node][field][0][key] = value;
                assert!(
                    job_outputs(&unsafe_entry, &graph).is_err(),
                    "{node}/{field}/{key}"
                );
            }
        }
    }
}

#[tokio::test]
async fn paired_history_preserves_terminal_partial_outputs_but_never_partial_success() {
    for (status, present, expected) in [
        ("success", "both", "succeeded"),
        ("success", "7", "failed"),
        ("success", "B7", "failed"),
        ("error", "B7", "failed"),
        ("interrupted", "B7", "cancelled"),
        ("error", "7", "failed"),
    ] {
        let mut tuple = Value::Null;
        let (base, task) = server(move |path, body| {
            if path == "GET /comfy/object_info HTTP/1.1" { return (200, image_postprocess_nodes().to_string()); }
            if path == "POST /comfy/prompt HTTP/1.1" {
                tuple = json!([0,PROMPT_ID,body["prompt"],{"client_id":body["client_id"]},["7","B7"]]);
                return (200,json!({"prompt_id":PROMPT_ID,"number":0,"node_errors":{}}).to_string());
            }
            assert_eq!(path, format!("GET /comfy/history/{PROMPT_ID} HTTP/1.1"));
            let mut out = json!({});
            for node in ["7", "B7"] {
                if present == "both" || present == node {
                    out[node] = json!({"images":[{"filename":format!("{node}.png"),"subfolder":"DEAS","type":"output","role":"spoofed"}]});
                }
            }
            let messages = match status {
                "error" => json!([["execution_error",{"prompt_id":PROMPT_ID,"node_id":"R5","exception_message":"refine failed"}]]),
                "interrupted" => json!([["execution_interrupted",{"prompt_id":PROMPT_ID}]]),
                _ => json!([]),
            };
            (200,json!({PROMPT_ID:{"prompt":tuple,"outputs":out,"status":{"completed":status=="success","status_str":if status=="success" {"success"} else {"error"},"messages":messages}}}).to_string())
        }).await;
        let jobs = Jobs::default();
        let mut i = intent();
        i["refine"] = json!({"width":512,"height":768,"denoise":0.3});
        let plan = jobs.prepare(base, i).await.unwrap();
        let queued = jobs
            .submit(plan["id"].as_str().unwrap().into(), true)
            .await
            .unwrap();
        let job = jobs
            .status(queued["id"].as_str().unwrap().into())
            .await
            .unwrap();
        assert_eq!(job["state"], expected, "{status}/{present}: {job}");
        assert_eq!(
            job["outputs"].as_array().unwrap().len(),
            if present == "both" { 2 } else { 1 }
        );
        assert_eq!(
            job["outputs"][0]["role"],
            if present == "B7" {
                "original"
            } else {
                "processed"
            }
        );
        task.abort();
    }
}

#[tokio::test]
async fn history_and_queue_refuse_foreign_identity_and_unsafe_outputs() {
    for (case, expected) in [
        ("six_fields", "succeeded"),
        ("error", "failed"),
        ("error_newline", "failed"),
        ("loader_error", "failed"),
        ("foreign_id", "unknown"),
        ("foreign_client", "unknown"),
        ("foreign_graph", "unknown"),
        ("foreign_key", "unknown"),
        ("bad_tuple", "unknown"),
        ("bad_status", "unknown"),
        ("empty_outputs", "unknown"),
        ("other_node", "unknown"),
        ("filename_path", "unknown"),
        ("filename_url", "unknown"),
        ("subfolder_traversal", "unknown"),
        ("subfolder_absolute", "unknown"),
        ("backslash", "succeeded"),
        ("backslash_traversal", "unknown"),
        ("encoded_path", "unknown"),
        ("temp_output", "unknown"),
        ("too_many", "unknown"),
        ("absent", "unknown"),
        ("foreign_queue", "unknown"),
        ("queue_graph", "unknown"),
        ("queue_client", "unknown"),
        ("duplicate_queue", "unknown"),
        ("incomplete", "running"),
        ("history_html", "unknown"),
        ("history_redirect", "unknown"),
        ("queue_bad_json", "unknown"),
    ] {
        let mut tuple = Value::Null;
        let (base,task) = server(move |path,body| {
            if path == "GET /comfy/object_info HTTP/1.1" {return (200,nodes().to_string());}
            if path == "POST /comfy/prompt HTTP/1.1" {
                tuple = json!([0,PROMPT_ID,body["prompt"],{"client_id":body["client_id"],"create_time":1234},["7"]]);
                return (200,json!({"prompt_id":PROMPT_ID,"number":0,"node_errors":{}}).to_string());
            }
            if path == format!("GET /comfy/history/{PROMPT_ID} HTTP/1.1") {
                if case == "history_html" {return (200,"<html>error</html>".into());}
                if case == "history_redirect" {return (302,"{}".into());}
                if ["absent","foreign_queue","queue_graph","queue_client","duplicate_queue","queue_bad_json"].contains(&case) {return (200,"{}".into());}
                let mut entry = json!({"prompt":tuple,"outputs":{"7":{"images":[{"filename":"image.png","subfolder":"DEAS/run-1","type":"output"}]}},"status":{"completed":true,"status_str":"success","messages":[]}});
                match case {
                    "six_fields" => entry["prompt"].as_array_mut().unwrap().push(json!({"hidden":"not returned"})),
                    "error" => entry["status"] = json!({"completed":true,"status_str":"error","messages":[["execution_error",{"node_id":"5","node_type":"KSampler","exception_type":"RuntimeError","exception_message":"failure".repeat(1000)}]]}),
                    "error_newline" => entry["status"] = json!({"completed":true,"status_str":"error","messages":[["execution_error",{"node_id":"5","node_type":"KSampler","exception_type":"RuntimeError","exception_message":"Missing checkpoint\nSelect a compatible dependency.\n"}]]}),
                    "loader_error" => entry["status"] = json!({"completed":true,"status_str":"error","messages":[["execution_error",{"node_id":"1","node_type":"CheckpointLoaderSimple","exception_type":"SafetensorError","exception_message":"Error while deserializing header: incomplete metadata, file not fully covered"}]]}),
                    "foreign_id" => entry["prompt"][1] = json!("other"),
                    "foreign_client" => entry["prompt"][3]["client_id"] = json!("other"),
                    "foreign_graph" => entry["prompt"][2]["5"]["inputs"]["seed"] = json!(999),
                    "bad_tuple" => entry["prompt"] = json!([]),
                    "bad_status" => entry["status"]["completed"] = json!("true"),
                    "empty_outputs" => entry["outputs"]["7"]["images"] = json!([]),
                    "other_node" => entry["outputs"] = json!({"99":{"images":[{"filename":"image.png","subfolder":"","type":"output"}]}}),
                    "filename_path" => entry["outputs"]["7"]["images"][0]["filename"] = json!("../image.png"),
                    "filename_url" => entry["outputs"]["7"]["images"][0]["filename"] = json!("https://host/image.png"),
                    "subfolder_traversal" => entry["outputs"]["7"]["images"][0]["subfolder"] = json!("../private"),
                    "subfolder_absolute" => entry["outputs"]["7"]["images"][0]["subfolder"] = json!("/private"),
                    "backslash" => entry["outputs"]["7"]["images"][0]["subfolder"] = json!("dir\\private"),
                    "backslash_traversal" => entry["outputs"]["7"]["images"][0]["subfolder"] = json!(r"dir/..\private"),
                    "encoded_path" => entry["outputs"]["7"]["images"][0]["filename"] = json!("%2e%2e%2fimage.png"),
                    "temp_output" => entry["outputs"]["7"]["images"][0]["type"] = json!("temp"),
                    "too_many" => entry["outputs"]["7"]["images"] = json!(vec![json!({"filename":"image.png","subfolder":"","type":"output"});65]),
                    "incomplete" => entry["status"]["completed"] = json!(false),
                    _ => {},
                }
                return (200,if case == "foreign_key" {json!({"other":entry})} else {json!({PROMPT_ID:entry})}.to_string());
            }
            assert_eq!(path,"GET /comfy/queue HTTP/1.1");
            if case == "queue_bad_json" {return (200,"[".into());}
            let mut matching = tuple.clone();
            if case == "foreign_queue" {matching[1] = json!("other");}
            if case == "queue_graph" {matching[2] = json!({});}
            if case == "queue_client" {matching[3]["client_id"] = json!("other");}
            (200,match case {
                "absent" => json!({"queue_running":[],"queue_pending":[]}),
                "duplicate_queue" => json!({"queue_running":[matching],"queue_pending":[tuple]}),
                _ => json!({"queue_running":[matching],"queue_pending":[]}),
            }.to_string())
        }).await;
        let jobs = Jobs::default();
        let plan = jobs.prepare(base, intent()).await.unwrap();
        let queued = jobs
            .submit(plan["id"].as_str().unwrap().into(), true)
            .await
            .unwrap();
        let job = jobs
            .status(queued["id"].as_str().unwrap().into())
            .await
            .unwrap();
        assert_eq!(job["state"], expected, "case {case}: {job}");
        assert_eq!(job["generationReady"], false);
        if expected != "succeeded" && !["error", "error_newline", "loader_error"].contains(&case) {
            assert_eq!(job["outputs"], json!([]));
        }
        if let Some(error) = job["error"].as_str() {
            assert!(error.chars().count() <= 800);
        }
        if case == "error_newline" {
            let text = job["error"].as_str().unwrap();
            assert_eq!(
                text,
                "node 5 (KSampler): RuntimeError: Missing checkpoint Select a compatible dependency."
            );
            assert!(!text.contains("\\n"));
        }
        if case == "loader_error" {
            assert_eq!(
                job["error"].as_str().unwrap(),
                "node 1 (CheckpointLoaderSimple): SafetensorError: Error while deserializing header: incomplete metadata, file not fully covered [sd/model.safetensors]"
            );
        }
        if case == "error" {
            assert!(job["error"].as_str().unwrap().contains("KSampler"));
        }
        assert!(jobs
            .submit(plan["id"].as_str().unwrap().into(), true)
            .await
            .is_err());
        task.abort();
    }
}

#[tokio::test]
async fn streamed_responses_and_total_request_duration_are_bounded() {
    for chunked in [false, true] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = crate::validate_base_url(&format!("http://{}", listener.local_addr().unwrap()))
            .unwrap();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            if chunked {
                socket.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").await.unwrap();
                let chunk = format!(
                    "{:x}\r\n{}\r\n0\r\n\r\n",
                    RESPONSE_LIMIT + 1,
                    " ".repeat(RESPONSE_LIMIT + 1)
                );
                let _ = socket.write_all(chunk.as_bytes()).await;
            } else {
                tokio::time::sleep(Duration::from_secs(11)).await;
            }
        });
        let start = std::time::Instant::now();
        let error = get(&client().unwrap(), &base, "history/test")
            .await
            .unwrap_err();
        if chunked {
            assert!(error.contains("size limit"));
        } else {
            assert!(start.elapsed() >= Duration::from_secs(9));
            assert!(start.elapsed() < Duration::from_secs(11));
        }
        task.abort();
    }
}

#[tokio::test]
async fn current_session_snapshot_survives_frontend_remount_without_network() {
    let jobs = Jobs::default();
    assert_eq!(jobs.current().unwrap(), Value::Null);
    let (base, task) = server(|path, _| {
        if path.starts_with("GET ") {
            (200, nodes().to_string())
        } else {
            (0, String::new())
        }
    })
    .await;
    let plan = jobs.prepare(base, intent()).await.unwrap();
    assert_eq!(jobs.current().unwrap(), Value::Null);
    let job = jobs
        .submit(plan["id"].as_str().unwrap().into(), true)
        .await
        .unwrap();
    task.abort();
    assert_eq!(job["state"], "unknown");
    assert_eq!(jobs.current().unwrap(), job);
    assert_eq!(jobs.current().unwrap(), job);
    assert_eq!(Jobs::default().current().unwrap(), Value::Null);
}

#[test]
fn native_graph_matches_fixed_typescript_compiler() {
    let graph = compile(&intent()).unwrap();
    assert_eq!(
        graph,
        json!({
            "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"sd/model.safetensors"}},
            "2":{"class_type":"CLIPTextEncode","inputs":{"text":"a cat","clip":["1",1]}},
            "3":{"class_type":"CLIPTextEncode","inputs":{"text":"","clip":["1",1]}},
            "4":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":768,"batch_size":1}},
            "5":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["2",0],"negative":["3",0],"latent_image":["4",0],"seed":123,"steps":20,"cfg":7,"sampler_name":"euler","scheduler":"normal","denoise":1}},
            "6":{"class_type":"VAEDecode","inputs":{"samples":["5",0],"vae":["1",2]}},
            "7":{"class_type":"SaveImage","inputs":{"images":["6",0],"filename_prefix":"DEAS_test"}}
        })
    );
}

#[test]
fn resize_graph_and_validation() {
    let mut image = intent();
    image["inputImage"] = json!("ref.png");
    image["denoise"] = json!(0.6);
    let old = compile(&image).unwrap();
    for crop in ["disabled", "center"] {
        let resize = json!({"width":512,"height":768,"crop":crop});
        image["inputResize"] = resize.clone();
        let graph = compile(&image).unwrap();
        assert_eq!(
            graph["S"],
            json!({"class_type":"ImageScale","inputs":{"image":["8",0],"upscale_method":"lanczos","width":512,"height":768,"crop":crop}})
        );
        assert_eq!(graph["9"]["inputs"]["pixels"], json!(["S", 0]));
        assert!(graph.get("P").is_none());
        let mut split = image.clone();
        split["diffusion"] = json!({"textEncoder":"clip","clipType":"sdxl","vae":"vae"});
        split["loras"] = json!([{"name":"lora.safetensors","strength":1}]);
        let split = compile(&split).unwrap();
        assert_eq!(split["9"]["inputs"]["vae"], json!(["V", 0]));
        assert_eq!(split["5"]["inputs"]["model"], json!(["L1", 0]));
        let pad = json!({"left":8,"right":16,"top":24,"bottom":32});
        image["inputResize"]["pad"] = pad.clone();
        image["width"] = json!(536);
        image["height"] = json!(824);
        let graph = compile(&image).unwrap();
        assert_eq!(
            graph["P"],
            json!({"class_type":"ImagePadForOutpaint","inputs":{"image":["S",0],"left":8,"right":16,"top":24,"bottom":32,"feathering":0}})
        );
        assert_eq!(graph["9"]["inputs"]["pixels"], json!(["P", 0]));
        assert_eq!(graph["5"], old["5"]);
        image["width"] = json!(512);
        image["height"] = json!(768);
        let mut bad = vec![
            json!(null),
            json!([]),
            json!("resize"),
            json!({}),
            json!({"width":512,"height":768}),
            json!({"width":512,"height":768,"crop":crop,"extra":1}),
        ];
        for field in ["width", "height"] {
            for value in [
                json!(0),
                json!(-8),
                json!(7),
                json!(511),
                json!(512.5),
                json!(8200),
                json!("512"),
            ] {
                let mut r = resize.clone();
                r[field] = value;
                bad.push(r);
            }
        }
        for value in [json!("fit"), json!(null), json!(1)] {
            let mut r = resize.clone();
            r["crop"] = value;
            bad.push(r);
        }
        for value in [
            json!(null),
            json!([]),
            json!({}),
            json!({"left":0,"right":0,"top":0}),
            json!({"left":0,"right":0,"top":0,"bottom":0,"extra":0}),
            json!({"left":8,"right":0,"top":0,"bottom":0}),
        ] {
            let mut r = resize.clone();
            r["pad"] = value;
            bad.push(r);
        }
        for value in [json!(-8), json!(1), json!(8.5), json!(8200), json!("8")] {
            let mut r = resize.clone();
            r["pad"] = pad.clone();
            r["pad"]["left"] = value;
            bad.push(r);
        }
        for r in bad {
            image["inputResize"] = r;
            assert!(compile(&image).is_err(), "accepted {image}");
        }
        let mut text = intent();
        text["inputResize"] = resize;
        assert!(compile(&text).is_err());
    }
    image.as_object_mut().unwrap().remove("inputResize");
    assert_eq!(compile(&image).unwrap(), old);
    for (width, height, valid) in [
        (3840, 2160, true),
        (4096, 4096, true),
        (8192, 2048, true),
        (2048, 8192, true),
        (8192, 2056, false),
    ] {
        let mut v = intent();
        v["width"] = json!(width);
        v["height"] = json!(height);
        assert_eq!(compile(&v).is_ok(), valid);
    }
}

#[tokio::test]
async fn prepare_requires_only_used_resize_nodes() {
    for absent in [None, Some("ImageScale"), Some("ImagePadForOutpaint")] {
        let mut info = nodes();
        info["ImageScale"] = json!({});
        info["ImagePadForOutpaint"] = json!({});
        if let Some(name) = absent {
            info.as_object_mut().unwrap().remove(name);
        }
        let (base, task) = server(move |_, _| (200, info.to_string())).await;
        let jobs = Jobs::default();
        let mut v = intent();
        v["inputImage"] = json!("ref.png");
        v["denoise"] = json!(0.6);
        assert!(jobs.prepare(base.clone(), v.clone()).await.is_ok());
        v["inputResize"] = json!({"width":512,"height":768,"crop":"disabled"});
        assert_eq!(
            jobs.prepare(base.clone(), v.clone()).await.is_ok(),
            absent != Some("ImageScale")
        );
        v["inputResize"]["pad"] = json!({"left":0,"right":0,"top":0,"bottom":0});
        assert_eq!(jobs.prepare(base, v).await.is_ok(), absent.is_none());
        task.abort();
    }
}

#[test]
fn mage_flow_compiles_the_combined_encode_node() {
    let mut value = intent();
    value["checkpoint"] = json!("mage_flow_int8_convrot.safetensors");
    value["steps"] = json!(30);
    value["cfg"] = json!(5);
    value["sampler"] = json!("euler");
    value["scheduler"] = json!("simple");
    value["diffusion"] = json!({"textEncoder":"qwen3vl_4b_bf16.safetensors","clipType":"mage","vae":"mage_flow_vae_bf16.safetensors","family":"mage"});
    let g = compile(&value).unwrap();
    assert_eq!(g["2"]["class_type"], json!("TextEncodeMageFlowEdit"));
    assert_eq!(g["2"]["inputs"]["negative_prompt"], json!(""));
    assert_eq!(g["2"]["inputs"]["batch_size"], json!(1));
    assert!(g.get("3").is_none());
    assert!(g.get("4").is_none());
    assert_eq!(g["5"]["inputs"]["negative"], json!(["2", 1]));
    assert_eq!(g["5"]["inputs"]["latent_image"], json!(["2", 2]));
}

#[test]
fn flux2_compiles_the_family_scheduler_instead_of_ksampler() {
    let mut value = intent();
    value["checkpoint"] = json!("flux-2-klein-base-4b-fp8.safetensors");
    value["cfg"] = json!(5);
    value["diffusion"] = json!({"textEncoder":"qwen_3_4b.safetensors","clipType":"flux2","vae":"flux2-vae.safetensors","family":"flux2"});
    let g = compile(&value).unwrap();
    assert_eq!(g["4"]["class_type"], json!("EmptyFlux2LatentImage"));
    assert_eq!(
        g["Z"],
        json!({"class_type":"Flux2Scheduler","inputs":{"steps":20,"width":512,"height":768}})
    );
    assert_eq!(g["5"]["class_type"], json!("SamplerCustomAdvanced"));
    assert_eq!(g["5"]["inputs"]["sigmas"], json!(["Z", 0]));
    assert_eq!(g["G"]["inputs"]["cfg"], json!(5));
}

#[test]
fn lumina2_compiles_the_checkpoint_recipe_points_by_family_id() {
    let mut value = intent();
    value["steps"] = json!(30);
    value["cfg"] = json!(4);
    value["sampler"] = json!("res_multistep");
    value["scheduler"] = json!("simple");
    value["family"] = json!("lumina2");
    let g = compile(&value).unwrap();
    assert_eq!(g["1"]["class_type"], json!("CheckpointLoaderSimple"));
    assert_eq!(g["4"]["class_type"], json!("EmptySD3LatentImage"));
    assert_eq!(
        g["M"],
        json!({"class_type":"ModelSamplingAuraFlow","inputs":{"model":["1",0],"shift":4.0}})
    );
    assert_eq!(g["5"]["inputs"]["model"], json!(["M", 0]));
    assert_eq!(g["3"]["class_type"], json!("CLIPTextEncode"));
}

#[test]
fn sd3_and_chroma_compile_the_family_recipe_points() {
    let mut value = intent();
    value["checkpoint"] = json!("sd3.safetensors");
    value["diffusion"] = json!({"textEncoder":"clip_l.safetensors","textEncoder2":"t5xxl_fp16.safetensors","clipType":"sd3","vae":"ae.safetensors","family":"sd3"});
    value["steps"] = json!(28);
    value["cfg"] = json!(4.5);
    value["sampler"] = json!("dpmpp_2m");
    value["scheduler"] = json!("karras");
    let g = compile(&value).unwrap();
    assert_eq!(g["4"]["class_type"], json!("EmptySD3LatentImage"));
    assert_eq!(
        g["M"],
        json!({"class_type":"ModelSamplingSD3","inputs":{"model":["1",0],"shift":3.0}})
    );
    assert_eq!(g["3"]["class_type"], json!("CLIPTextEncode"));
    // Chroma Radiance shares the chroma CLIP type but uses the pixel latent; the family id picks it.
    value["checkpoint"] = json!("chroma-radiance.safetensors");
    value["diffusion"] = json!({"textEncoder":"t5xxl_fp16.safetensors","clipType":"chroma","vae":"ae.safetensors","family":"chroma-radiance"});
    let radiance = compile(&value).unwrap();
    assert_eq!(
        radiance["4"]["class_type"],
        json!("EmptyChromaRadianceLatentImage")
    );
    assert!(radiance.get("M").is_none());
    value["diffusion"] =
        json!({"textEncoder":"t5xxl_fp16.safetensors","clipType":"chroma","vae":"ae.safetensors"});
    let shared = compile(&value).unwrap();
    assert_eq!(shared["4"]["class_type"], json!("EmptySD3LatentImage"));
}

#[test]
fn flux_compiles_the_dual_clip_loader_and_family_points() {
    let mut value = intent();
    value["checkpoint"] = json!("flux1-dev.safetensors");
    value["diffusion"] = json!({"textEncoder":"clip_l.safetensors","textEncoder2":"t5xxl_fp16.safetensors","clipType":"flux","vae":"ae.safetensors"});
    value["steps"] = json!(20);
    value["cfg"] = json!(1);
    value["sampler"] = json!("euler");
    value["scheduler"] = json!("simple");
    let g = compile(&value).unwrap();
    assert_eq!(
        g["C"],
        json!({"class_type":"DualCLIPLoader","inputs":{"clip_name1":"clip_l.safetensors","clip_name2":"t5xxl_fp16.safetensors","type":"flux"}})
    );
    assert_eq!(g["2"]["inputs"]["clip"], json!(["C", 0]));
    assert_eq!(g["4"]["class_type"], json!("EmptySD3LatentImage"));
    assert_eq!(
        g["3"],
        json!({"class_type":"ConditioningZeroOut","inputs":{"conditioning":["2",0]}})
    );
    // The second encoder must differ from the first and stay a safe model name.
    value["diffusion"] = json!({"textEncoder":"clip_l.safetensors","textEncoder2":"clip_l.safetensors","clipType":"flux","vae":"ae.safetensors"});
    assert!(compile(&value).is_err());
}

#[test]
fn z_image_compiles_the_family_recipe_points() {
    let mut value = intent();
    value["checkpoint"] = json!("z_image_turbo_bf16.safetensors");
    value["diffusion"] =
        json!({"textEncoder":"qwen_3_4b.safetensors","clipType":"lumina2","vae":"ae.safetensors"});
    value["steps"] = json!(8);
    value["cfg"] = json!(1);
    value["sampler"] = json!("res_multistep");
    value["scheduler"] = json!("simple");
    let g = compile(&value).unwrap();
    assert_eq!(g["4"]["class_type"], json!("EmptySD3LatentImage"));
    assert_eq!(
        g["3"],
        json!({"class_type":"ConditioningZeroOut","inputs":{"conditioning":["2",0]}})
    );
    assert_eq!(
        g["M"],
        json!({"class_type":"ModelSamplingAuraFlow","inputs":{"model":["1",0],"shift":3.0}})
    );
    assert_eq!(g["5"]["inputs"]["model"], json!(["M", 0]));
    assert_eq!(g["5"]["inputs"]["negative"], json!(["3", 0]));
    // Unknown clip types keep the plain graph: generic latent, encoded negative, no patch.
    value["diffusion"] = json!({"textEncoder":"qwen_3_4b.safetensors","clipType":"unknown_type","vae":"ae.safetensors"});
    let plain = compile(&value).unwrap();
    assert_eq!(plain["4"]["class_type"], json!("EmptyLatentImage"));
    assert_eq!(plain["3"]["class_type"], json!("CLIPTextEncode"));
    assert!(plain.get("M").is_none());
}

#[test]
fn moss_compiles_flux2_t2i_and_plain_vae_i2i() {
    let mut value = intent();
    value["diffusion"] = json!({"textEncoder":"encoder.safetensors","clipType":"moss_image","vae":"flux2.safetensors"});
    let g = compile(&value).unwrap();
    assert_eq!(
        g["4"],
        json!({"class_type":"EmptyFlux2LatentImage","inputs":{"width":512,"height":768,"batch_size":1}})
    );
    assert_eq!(
        g["C"]["inputs"],
        json!({"clip_name":"encoder.safetensors","type":"moss_image"})
    );
    assert_eq!(g["2"]["inputs"]["clip"], json!(["C", 0]));
    assert_eq!(g["6"]["inputs"]["vae"], json!(["V", 0]));
    value["inputImage"] = json!("ref.png");
    value["denoise"] = json!(0.37);
    let g = compile(&value).unwrap();
    assert!(g.get("4").is_none());
    assert_eq!(
        g["9"],
        json!({"class_type":"VAEEncode","inputs":{"pixels":["8",0],"vae":["V",0]}})
    );
    assert_eq!(g["5"]["inputs"]["latent_image"], json!(["9", 0]));
    assert_eq!(g["5"]["inputs"]["denoise"], 0.37);
    // Masked inpaint rides the same model-generic graph: the mask goes on the encoded latent and the
    // decoded pixels are composited onto the untouched source.
    value["inputMask"] = json!("deas-masks/4907ef27-6ffc-4fe9-a5af-56bbac500c11.png");
    let g = compile(&value).unwrap();
    assert_eq!(g["X"]["class_type"], "SetLatentNoiseMask");
    assert_eq!(g["5"]["inputs"]["latent_image"], json!(["X", 0]));
    assert_eq!(g["Y"]["class_type"], "ImageCompositeMasked");
}

#[test]
fn moss_rejects_unaligned_dimensions_and_unproved_combinations() {
    let mut value = intent();
    value["diffusion"] = json!({"textEncoder":"encoder.safetensors","clipType":"moss_image","vae":"flux2.safetensors"});
    for field in ["width", "height"] {
        let mut bad = value.clone();
        bad[field] = json!(520);
        assert!(compile(&bad).unwrap_err().contains("multiples of 16"));
    }
    value["inputImage"] = json!("ref.png");
    value["denoise"] = json!(0.37);
    for (key, extra) in [
        ("refine", json!({"width":1024,"height":1024,"denoise":0.5})),
        (
            "inputLayers",
            json!([{"image":"ref.png","x":0,"y":0,"width":512,"height":768}]),
        ),
        ("sigmas", json!([1, 0])),
        ("loras", json!([{"name":"style.safetensors","strength":1}])),
        ("options", json!({"vaeTile":512})),
        (
            "inputResize",
            json!({"width":512,"height":768,"crop":"disabled","pad":{"left":0,"right":0,"top":0,"bottom":0}}),
        ),
    ] {
        let mut bad = value.clone();
        bad[key] = extra;
        let error = compile(&bad).unwrap_err();
        assert!(
            error.contains("MOSS-Image supports plain"),
            "{key}: {error}"
        );
    }
    value["inputResize"] = json!({"width":512,"height":768,"crop":"disabled"});
    assert!(compile(&value).is_ok());
}

#[tokio::test]
async fn moss_prepare_requires_exact_emitted_nodes_and_components_only() {
    for (image, metadata) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut value = intent();
        value["diffusion"] = json!({"textEncoder":"encoder.safetensors","clipType":"moss_image","vae":"flux2.safetensors"});
        if image {
            value["inputImage"] = json!("ref.png");
            value["denoise"] = json!(0.37);
        }
        let mut graph = compile(&value).unwrap();
        let mut info = json!({
            "UNETLoader":{"input":{"required":{"unet_name":[[value["checkpoint"]]]}}},
            "CLIPLoader":{"input":{"required":{"clip_name":[["encoder.safetensors"]],"type":[["moss_image"]]}}},
            "VAELoader":{"input":{"required":{"vae_name":[["flux2.safetensors"]]}}},
            "KSampler":{"input":{"required":{"sampler_name":[["euler"]],"scheduler":[["normal"]]}}}
        });
        if image {
            info["LoadImage"] = json!({"input":{"required":{"image":[["ref.png"]]}}});
        }
        for node in graph.as_object().unwrap().values() {
            let name = node["class_type"].as_str().unwrap();
            if info.get(name).is_none() {
                info[name] = json!({});
            }
        }
        if metadata {
            info.as_object_mut().unwrap().remove("SaveImage");
            info["DaSiWa_MetadataImageSaver"] = json!({});
            info["DaSiWa_MetadataConfig"] = json!({});
            civitai_saver(
                &mut graph,
                &json!(["DaSiWa_MetadataImageSaver", "DaSiWa_MetadataConfig"]),
                &value,
            );
        }
        let complete = info.clone();
        let (base, task) = server(move |_, _| (200, complete.to_string())).await;
        let prepared = Jobs::default().prepare(base, value.clone()).await;
        task.abort();
        assert_eq!(prepared.unwrap()["graph"], graph);
        for missing in graph
            .as_object()
            .unwrap()
            .values()
            .filter_map(|n| n["class_type"].as_str())
        {
            let mut incomplete = info.clone();
            incomplete.as_object_mut().unwrap().remove(missing);
            let (base, task) = server(move |_, _| (200, incomplete.to_string())).await;
            let result = Jobs::default().prepare(base, value.clone()).await;
            task.abort();
            assert!(result.is_err(), "accepted absent {missing}, image={image}");
        }
        for (node, field) in [
            ("UNETLoader", "unet_name"),
            ("CLIPLoader", "clip_name"),
            ("CLIPLoader", "type"),
            ("VAELoader", "vae_name"),
        ] {
            let mut incomplete = info.clone();
            incomplete[node]["input"]["required"][field] = json!([[]]);
            let (base, task) = server(move |_, _| (200, incomplete.to_string())).await;
            let result = Jobs::default().prepare(base, value.clone()).await;
            task.abort();
            assert!(result.is_err(), "accepted absent {node}.{field}");
        }
    }
}

#[test]
fn diffusion_intent_splits_model_text_encoder_and_vae() {
    let mut value = intent();
    value["diffusion"] = json!({"textEncoder":"qwen3vl_4b.safetensors","clipType":"krea2","vae":"q/vae.safetensors"});
    value["loras"] = json!([{"name":"k.safetensors","strength":0.5}]);
    let g = compile(&value).unwrap();
    assert_eq!(g["1"]["class_type"], "UNETLoader");
    assert_eq!(g["1"]["inputs"]["unet_name"], value["checkpoint"]);
    assert_eq!(
        g["C"]["inputs"],
        json!({"clip_name":"qwen3vl_4b.safetensors","type":"krea2"})
    );
    assert_eq!(g["L1"]["inputs"]["clip"], json!(["C", 0]));
    assert_eq!(g["2"]["inputs"]["clip"], json!(["L1", 1]));
    assert_eq!(g["6"]["inputs"]["vae"], json!(["V", 0]));
    for bad in [
        json!({"textEncoder":"a.safetensors","clipType":"krea2"}),
        json!({"textEncoder":"../a","clipType":"krea2","vae":"v"}),
        json!({"textEncoder":"a","clipType":"kr ea","vae":"v"}),
    ] {
        value["diffusion"] = bad;
        assert!(compile(&value).is_err(), "accepted {value}");
    }
}

/// Qwen Image 2.1 Turbo: the checkpoint's saved 8-step schedule (official ComfyUI template) replaces KSampler.
#[test]
fn fixed_sigmas_compile_to_manual_sigmas_and_custom_sampler() {
    let mut value = intent();
    value["steps"] = json!(8);
    value["sigmas"] =
        json!([1.0, 0.978453, 0.95418, 0.926626, 0.89508, 0.845148, 0.704534, 0.414568, 0]);
    let g = compile(&value).unwrap();
    assert_eq!(
        g["Z"]["inputs"]["sigmas"],
        "1, 0.978453, 0.95418, 0.926626, 0.89508, 0.845148, 0.704534, 0.414568, 0"
    );
    assert_eq!(g["5"]["class_type"], "SamplerCustomAdvanced");
    assert_eq!(g["5"]["inputs"]["latent_image"], json!(["4", 0]));
    assert_eq!(g["G"]["inputs"]["model"], json!(["1", 0]));
    assert_eq!(g["G"]["inputs"]["cfg"], value["cfg"]);
    assert_eq!(g["Q"]["inputs"]["sampler_name"], value["sampler"]);
    assert_eq!(g["N"]["inputs"]["noise_seed"], value["seed"]);
    assert_eq!(g["6"]["inputs"]["samples"], json!(["5", 0]));
    assert!(g.get("KSampler").is_none());
    // Image-to-image: the tail of the schedule carries the strength; SamplerCustomAdvanced has no denoise.
    value["inputImage"] = json!("in.png");
    value["denoise"] = json!(0.5);
    value["steps"] = json!(4);
    value["sigmas"] = json!([0.89508, 0.845148, 0.704534, 0.414568, 0]);
    let g = compile(&value).unwrap();
    assert_eq!(g["5"]["inputs"]["latent_image"], json!(["9", 0]));
    assert!(g["5"]["inputs"].get("denoise").is_none());
    for bad in [
        json!([1.0, 0.5]),                // steps mismatch
        json!([1.0, 0.5, 0.5, 0.2, 0]),   // not descending
        json!([1.0, 0.8, 0.6, 0.4, 0.2]), // no zero tail
        json!([1.0, 0.8, 0.6, "0.4", 0]),
        json!("1, 0"),
    ] {
        value["sigmas"] = bad;
        assert!(compile(&value).is_err(), "accepted {value}");
    }
}

#[test]
fn graph_correlation_accepts_comfy_float_rewrites_only() {
    let sent =
        json!({"5":{"inputs":{"cfg":1,"denoise":1,"seed":18446744073709551615u64,"steps":8}}});
    let stored =
        json!({"5":{"inputs":{"cfg":1.0,"denoise":1.0,"seed":18446744073709551615u64,"steps":8}}});
    assert!(same(&stored, &sent));
    assert!(!same(
        &json!({"5":{"inputs":{"cfg":1.5}}}),
        &json!({"5":{"inputs":{"cfg":1}}})
    ));
    assert!(!same(
        &json!({"5":{"inputs":{"cfg":1,"x":0}}}),
        &json!({"5":{"inputs":{"cfg":1}}})
    ));
    assert!(!same(&json!(["1", 0]), &json!(["1", 0, 0])));
}

#[test]
fn preview_frames_need_known_format_and_magic() {
    let jpeg = [&[0, 0, 0, 1, 0, 0, 0, 1][..], &[0xFF, 0xD8, 0xFF, 0xE0]].concat();
    assert_eq!(
        preview_frame(&jpeg).unwrap(),
        "data:image/jpeg;base64,/9j/4A=="
    );
    let png = [&[0, 0, 0, 1, 0, 0, 0, 2][..], b"\x89PNG\r\n\x1a\n"].concat();
    assert!(preview_frame(&png)
        .unwrap()
        .starts_with("data:image/png;base64,"));
    assert!(preview_frame(&[0, 0, 0, 1, 0, 0, 0, 2, 0xFF, 0xD8, 0xFF]).is_none());
    assert!(preview_frame(&[0, 0, 0, 4, 0, 0, 0, 1, 0xFF, 0xD8, 0xFF]).is_none());
    assert!(preview_frame(&[0, 0, 0, 1]).is_none());
}

/// `cargo test --lib dump_h3_edit_graph -- --ignored --nocapture` prints the H3 image edit for `tools/validate_video_graphs.py`.
#[test]
#[ignore]
fn dump_h3_edit_graph() {
    let mut i = intent();
    i["checkpoint"] = json!("MiniMaxH3/fl2va.safetensors");
    i["diffusion"] = json!({"textEncoder":"qwen3vl_32b_minimax_h3_nvfp4_awq.safetensors","clipType":"minimax","vae":"MiniMaxH3/video_vae.safetensors"});
    i["h3Edit"] = json!({"shiftVideo":10,"shiftAudio":3});
    (i["width"], i["height"], i["scheduler"]) = (json!(1024), json!(768), json!("simple"));
    i["inputImage"] = json!("ref.png");
    i["denoise"] = json!(1);
    i["inputResize"] = json!({"width":1024,"height":768,"crop":"center"});
    println!("{}", json!({"h3_edit": compile(&i).unwrap()}));
}

#[test]
fn h3_refmod_library_keeps_safe_names_known_kinds_and_bounded_descriptions() {
    let long = "x".repeat(2500);
    let listed = refmod_entries(&json!([
        {"name":"chars/hero","kind":"image","kinds":["image","audio"],"description":long,"tokens":12,"mtime":1.0},
        {"name":"../escape","kinds":["image"]},
        {"name":"odd","kinds":["latent"]},
        {"name":"empty","kinds":[]},
        {"name":"bare"}
    ]));
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["name"], "chars/hero");
    assert_eq!(listed[0]["kinds"], json!(["image", "audio"]));
    assert_eq!(listed[0]["description"].as_str().unwrap().len(), 2000);
    assert!(listed[0].get("tokens").is_none());
    assert_eq!(refmod_entries(&json!({"error":"x"})), json!([]));
}

fn qwen_intent() -> Value {
    let mut i = intent();
    i["diffusion"] = json!({"textEncoder":"qwen.safetensors","clipType":"qwen_image","vae":"qwen-vae.safetensors"});
    i["nativeImageReference"] = json!({"method":"qwen-image-2.1","images":["first.png","second.png","third.png","fourth.png"]});
    i
}

#[test]
fn qwen_native_references_encode_both_prompts_and_exact_first_reference_latent() {
    let i = qwen_intent();
    let g = compile(&i).unwrap();
    assert_eq!(
        g["QI"],
        json!({"class_type":"TextEncodeQwenImage21","inputs":{
            "clip":["C",0],"vae":["V",0],"prompt":"a cat","negative_prompt":"","resolution":0,
            "images.image_1":["QR",0],"images.image_2":["RI1",0],"images.image_3":["RI2",0],"images.image_4":["RI3",0]
        }})
    );
    assert_eq!(
        g["QR"],
        json!({"class_type":"ImageScale","inputs":{"image":["RI0",0],"upscale_method":"lanczos","width":512,"height":768,"crop":"center"}})
    );
    for (n, image) in i["nativeImageReference"]["images"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        assert_eq!(
            g[format!("RI{n}")],
            json!({"class_type":"LoadImage","inputs":{"image":image}})
        );
    }
    assert_eq!(g["5"]["inputs"]["positive"], json!(["QI", 0]));
    assert_eq!(g["5"]["inputs"]["negative"], json!(["QI", 1]));
    assert_eq!(g["5"]["inputs"]["latent_image"], json!(["QI", 2]));
    for id in ["2", "3", "4"] {
        assert!(g.get(id).is_none());
    }
}

#[test]
fn qwen_native_references_validate_schema_and_family_fail_closed() {
    let i = qwen_intent();
    for r in [
        json!(null),
        json!([]),
        json!({}),
        json!({"images":["first.png"]}),
        json!({"method":"other","images":["first.png"]}),
        json!({"method":"qwen-image-2.1","images":[]}),
        json!({"method":"qwen-image-2.1","images":["a","b","c","d","e"]}),
        json!({"method":"qwen-image-2.1","images":[null]}),
        json!({"method":"qwen-image-2.1","images":[{"image":"a","strength":1}]}),
        json!({"method":"qwen-image-2.1","images":["a"],"strength":1}),
        json!({"method":"qwen-image-2.1","images":["a"],"visionModel":"v"}),
    ] {
        let mut bad = i.clone();
        bad["nativeImageReference"] = r;
        assert!(compile(&bad).unwrap_err().contains("nativeImageReference"));
    }
    for image in ["../x", "/x", "x\\y", "https://x", "x%20", "x\0", " x"] {
        let mut bad = i.clone();
        bad["nativeImageReference"]["images"] = json!([image]);
        assert!(compile(&bad).is_err());
    }
    for clip in ["sdxl", "qwen_image_edit", "krea2", "anima", "moss_image"] {
        let mut bad = i.clone();
        bad["diffusion"]["clipType"] = json!(clip);
        assert!(compile(&bad).is_err());
    }
    let mut bad = i.clone();
    bad.as_object_mut().unwrap().remove("diffusion");
    assert!(compile(&bad).is_err());
    bad = i.clone();
    bad["imageReference"] = json!({"visionModel":"v","images":[{"image":"a","strength":1}]});
    assert!(compile(&bad).is_err());
    for field in ["width", "height"] {
        let mut bad = i.clone();
        bad[field] = json!(520);
        assert!(compile(&bad).unwrap_err().contains("nativeImageReference"));
    }
}

#[test]
fn qwen_native_references_preserve_i2i_and_wildcard_sigmas_refiner_conditioning() {
    for custom in [false, true] {
        let mut i = qwen_intent();
        i["inputImage"] = json!("first.png");
        i["denoise"] = json!(0.35);
        i["loras"] = json!([{"name":"style.safetensors","strength":0.5}]);
        i["wildcard"] = json!({"style":"Booru","seed":7,"reroll":3,"token_budget":200,"auto_reroll":true,"selection_state":{}});
        i["refine"] = json!({"width":1024,"height":1536,"denoise":0.4,"tiled":{"tileWidth":512,"tileHeight":768,"overlap":64}});
        if custom {
            i["steps"] = json!(2);
            i["sigmas"] = json!([1, 0.5, 0]);
            i["refine"]["sigmas"] = json!([0.5, 0]);
        }
        let g = compile(&i).unwrap();
        assert_eq!(g["QI"]["inputs"]["prompt"], json!(["WP", 0]));
        assert_eq!(g["QI"]["inputs"]["negative_prompt"], json!(["WP", 1]));
        assert_eq!(g["QI"]["inputs"]["clip"], json!(["L1", 1]));
        assert_eq!(g["5"]["inputs"]["latent_image"], json!(["9", 0]));
        assert_eq!(g["9"]["inputs"], json!({"pixels":["8",0],"vae":["V",0]}));
        assert_eq!(
            g["5"]["inputs"].get("denoise").cloned(),
            if custom { None } else { Some(json!(0.35)) }
        );
        for id in if custom { ["G", "RG"] } else { ["5", "R5"] } {
            assert_eq!(g[id]["inputs"]["positive"], json!(["QI", 0]));
            assert_eq!(g[id]["inputs"]["negative"], json!(["QI", 1]));
        }
        assert_eq!(g["R5"]["inputs"]["latent_image"], json!(["RE", 0]));
        for id in ["2", "3", "4"] {
            assert!(g.get(id).is_none());
        }
    }
}

#[test]
fn qwen_native_metadata_keeps_resolved_prompts_without_dangling_encoders() {
    let i = qwen_intent();
    let mut g = compile(&i).unwrap();
    civitai_saver(
        &mut g,
        &json!(["DaSiWa_MetadataImageSaver", "DaSiWa_MetadataConfig"]),
        &i,
    );
    assert_eq!(g["M"]["inputs"]["node_positive"], json!(["QI", 0]));
    assert_eq!(g["M"]["inputs"]["node_negative"], json!(["QI", 1]));
    assert_eq!(g["M"]["inputs"]["text_positive"], i["positivePrompt"]);
    assert_eq!(g["M"]["inputs"]["text_negative"], i["negativePrompt"]);
}

#[tokio::test]
async fn qwen_native_prepare_checks_all_images_models_and_required_nodes_without_submitting() {
    for (method, clip, encoder) in [
        ("qwen-image-2.1", "qwen_image", "TextEncodeQwenImage21"),
        ("krea-2", "krea2", "DEAS_ImagePromptEncode"),
        ("moss-image", "moss_image", "DEAS_ImagePromptEncode"),
    ] {
        for missing in [
            None,
            Some(encoder),
            Some("ImageScale"),
            Some("LoadImage"),
            Some("first.png"),
            Some("second.png"),
            Some("third.png"),
            Some("fourth.png"),
            Some("textEncoder"),
            Some("clipType"),
            Some("vae"),
            Some("model"),
        ] {
            let mut info = nodes();
            info["UNETLoader"] =
                json!({"input":{"required":{"unet_name":[["sd/model.safetensors"]]}}});
            info["CLIPLoader"] =
                json!({"input":{"required":{"clip_name":[["qwen.safetensors"]],"type":[[clip]]}}});
            info["VAELoader"] =
                json!({"input":{"required":{"vae_name":[["qwen-vae.safetensors"]]}}});
            info["LoadImage"] = json!({"input":{"required":{"image":[["first.png","second.png","third.png","fourth.png"]]}}});
            info["ImageScale"] = json!({});
            info[encoder] = json!({});
            info["EmptyFlux2LatentImage"] = json!({});
            match missing {
                Some("first.png" | "second.png" | "third.png" | "fourth.png") => {
                    info["LoadImage"]["input"]["required"]["image"][0]
                        .as_array_mut()
                        .unwrap()
                        .retain(|v| v != missing.unwrap());
                }
                Some("textEncoder") => {
                    info["CLIPLoader"]["input"]["required"]["clip_name"][0] = json!([])
                }
                Some("clipType") => info["CLIPLoader"]["input"]["required"]["type"][0] = json!([]),
                Some("vae") => info["VAELoader"]["input"]["required"]["vae_name"][0] = json!([]),
                Some("model") => {
                    info["UNETLoader"]["input"]["required"]["unet_name"][0] = json!([])
                }
                Some(node) => {
                    info.as_object_mut().unwrap().remove(node);
                }
                None => {}
            }
            let (base, task) = server(move |path, _| {
                assert_eq!(path, "GET /comfy/object_info HTTP/1.1");
                (200, info.to_string())
            })
            .await;
            let mut i = qwen_intent();
            i["diffusion"]["clipType"] = json!(clip);
            i["nativeImageReference"]["method"] = json!(method);
            let result = Jobs::default().prepare(base, i).await;
            assert_eq!(
                result.is_ok(),
                missing.is_none() || (missing == Some("ImageScale") && method != "qwen-image-2.1"),
                "{method} missing {missing:?}: {result:?}"
            );
            task.abort();
        }
    }
}

#[test]
fn native_adapter_references_preserve_krea_moss_latents_and_dimensions() {
    for (method, clip, width) in [("krea-2", "krea2", 520), ("moss-image", "moss_image", 528)] {
        for i2i in [false, true] {
            let mut i = qwen_intent();
            i["diffusion"]["clipType"] = json!(clip);
            i["width"] = json!(width);
            i["nativeImageReference"]["method"] = json!(method);
            if i2i {
                i["inputImage"] = json!("first.png");
                i["denoise"] = json!(0.4);
            }
            let g = compile(&i).unwrap();
            let mut plain = i.clone();
            plain
                .as_object_mut()
                .unwrap()
                .remove("nativeImageReference");
            let before = compile(&plain).unwrap();
            assert_eq!(
                g["QI"],
                json!({"class_type":"DEAS_ImagePromptEncode","inputs":{
                    "clip":["C",0],"prompt":"a cat","negative_prompt":"",
                    "image_1":["RI0",0],"image_2":["RI1",0],"image_3":["RI2",0],"image_4":["RI3",0]
                }})
            );
            assert_eq!(
                g["5"]["inputs"]["latent_image"],
                before["5"]["inputs"]["latent_image"]
            );
            assert_eq!(g.get("4"), before.get("4"));
            assert_eq!(g.get("9"), before.get("9"));
            assert!(g.get("QR").is_none());
            assert!(g.get("2").is_none());
            assert!(g.get("3").is_none());
            assert_eq!(g["5"]["inputs"]["positive"], json!(["QI", 0]));
            assert_eq!(g["5"]["inputs"]["negative"], json!(["QI", 1]));
            i["nativeImageReference"]["method"] = json!(if method == "krea-2" {
                "moss-image"
            } else {
                "krea-2"
            });
            assert!(compile(&i).is_err());
        }
    }
}

#[test]
fn native_reference_graphs_match_typescript_compiler_end_to_end() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut cases = Vec::new();
    let mut expected = Vec::new();
    let saver = json!(["DaSiWa_MetadataImageSaver", "DaSiWa_MetadataConfig"]);
    for (method, clip) in [
        ("qwen-image-2.1", "qwen_image"),
        ("krea-2", "krea2"),
        ("moss-image", "moss_image"),
    ] {
        for variant in 0..4 {
            let mut i = qwen_intent();
            i["diffusion"]["clipType"] = json!(clip);
            i["nativeImageReference"]["method"] = json!(method);
            if variant > 0 {
                i["inputImage"] = json!("first.png");
                i["denoise"] = json!(0.4);
                i["wildcard"] = json!({"style":"Booru","seed":7,"reroll":3,"token_budget":200,"auto_reroll":true,"selection_state":{}});
            }
            if clip != "moss_image" && variant > 1 {
                i["refine"] = json!({"width":1024,"height":1536,"denoise":0.4,"tiled":{"tileWidth":512,"tileHeight":768,"overlap":64}});
                if crate::model_families::image_recipe(clip).temporal_latent {
                    assert!(compile(&i).unwrap_err().contains("temporal latent"));
                    i["refine"].as_object_mut().unwrap().remove("tiled");
                }
                if variant == 3 {
                    i["steps"] = json!(2);
                    i["sigmas"] = json!([1, 0.5, 0]);
                    i["refine"]["sigmas"] = json!([0.5, 0]);
                }
            }
            let mut g = compile(&i).unwrap();
            civitai_saver(&mut g, &saver, &i);
            cases.push(i);
            expected.push(g);
        }
    }
    // Include the bounded paired-baseline slice in actual TS/native graph parity, not parallel hand-written expectations.
    for variant in 0..6 {
        for metadata in [false, true] {
            let mut i = intent();
            if variant > 0 {
                i["refine"] = json!({"width":512,"height":768,"denoise":0.3});
            }
            if variant == 2 {
                i["refine"] = json!({"width":1024,"height":1536,"denoise":0.3,"upscaleModel":"2x-Sharp.safetensors"});
            }
            if variant == 3 {
                i["inputImage"] = json!("first.png");
                i["denoise"] = json!(0.4);
                i["inputMask"] = json!("deas-masks/3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d.png");
                i["inpaintCrop"] = json!({"x":16,"y":16,"width":128,"height":128,"targetWidth":256,"targetHeight":256});
            }
            if variant == 4 {
                i["options"] = json!({"vaeTile":512});
                i["refine"]["tiled"] = json!({"tileWidth":512,"tileHeight":512,"overlap":64});
            }
            if variant == 5 {
                i["steps"] = json!(2);
                i["sigmas"] = json!([1, 0.5, 0]);
                i["refine"]["sigmas"] = json!([0.5, 0]);
            }
            i["imageFormat"] = json!("webp");
            i["imageCompression"] = json!(73);
            let mut g = compile(&i).unwrap();
            if metadata {
                civitai_saver(&mut g, &saver, &i);
            }
            if variant > 0 {
                let mut first = i.clone();
                first.as_object_mut().unwrap().remove("refine");
                let mut before = compile(&first).unwrap();
                if metadata {
                    civitai_saver(&mut before, &saver, &first);
                }
                before["7"]["inputs"]["filename_prefix"] = json!("DEAS_test_original");
                assert_eq!(g["B7"], before["7"]);
                assert_eq!(g["7"]["inputs"]["images"], json!(["PG", 0]));
                assert_eq!(
                    g["PG"],
                    json!({"class_type":"DEAS_ImagePostprocessGate","inputs":{"baseline":["B7",0],"image":["R6",0]}})
                );
                assert_eq!(
                    g.as_object()
                        .unwrap()
                        .values()
                        .filter(|n| matches!(
                            n["class_type"].as_str(),
                            Some("KSampler" | "SamplerCustomAdvanced")
                        ))
                        .count(),
                    2
                );
            } else {
                assert!(g.get("B7").is_none());
            }
            cases.push(i);
            expected.push(g);
        }
    }
    let script = r#"
        import fs from 'node:fs';
        import { compileRecipe } from './src/lib/execution.ts';
        const { cases, expected } = JSON.parse(fs.readFileSync(0, 'utf8'));
        const allow = { checkpoints: ['sd/model.safetensors'], diffusionModels: ['sd/model.safetensors'],
            textEncoders: ['qwen.safetensors'], vaes: ['qwen-vae.safetensors'], clipTypes: ['qwen_image','krea2','moss_image'],
            upscaleModels: ['2x-Sharp.safetensors'],
            images: ['first.png','second.png','third.png','fourth.png'], samplers: ['euler'], schedulers: ['normal'] };
        process.stdout.write(JSON.stringify(cases.map((i,n) => compileRecipe(i, { ...allow, nodes: [...new Set(Object.values(expected[n]).map(n => n.class_type))] }).graph)));
    "#;
    let mut child = Command::new("node")
        .args([
            "--experimental-strip-types",
            "--input-type=module",
            "-e",
            script,
        ])
        .current_dir(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap(),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Node is required for compiler parity tests");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            serde_json::to_string(&json!({"cases":cases,"expected":expected}))
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn image_reference_compiles_core_revision_and_rejects_split_families() {
    let mut i = intent();
    i["imageReference"] = json!({"visionModel":"clip-vit.safetensors","images":[{"image":"deas-inputs/ref.png","strength":0.5},{"image":"deas-inputs/second.png","strength":0.2}]});
    let g = compile(&i).unwrap();
    assert_eq!(g["CV"]["class_type"], "CLIPVisionLoader");
    assert_eq!(g["CV"]["inputs"]["clip_name"], "clip-vit.safetensors");
    assert_eq!(g["RI0"]["class_type"], "LoadImage");
    assert_eq!(g["CE0"]["inputs"]["clip_vision"], json!(["CV", 0]));
    assert_eq!(g["CE0"]["inputs"]["image"], json!(["RI0", 0]));
    assert_eq!(g["UC0"]["inputs"]["conditioning"], json!(["2", 0]));
    assert_eq!(g["UC1"]["inputs"]["conditioning"], json!(["UC0", 0]));
    assert_eq!(g["UC1"]["inputs"]["strength"], 0.2);
    assert_eq!(g["UC0"]["inputs"]["noise_augmentation"], 0.0);
    assert_eq!(g["5"]["inputs"]["positive"], json!(["UC1", 0]));
    let mut s = i.clone();
    s["steps"] = json!(2);
    s["sigmas"] = json!([1.0, 0.5, 0.0]);
    let g2 = compile(&s).unwrap();
    assert_eq!(g2["G"]["inputs"]["positive"], json!(["UC1", 0]));
    let mut bad = i.clone();
    bad["imageReference"]["images"][0]["strength"] = json!(1.5);
    assert!(compile(&bad).unwrap_err().contains("imageReference"));
    bad["imageReference"]["images"][0]["strength"] = json!(0.5);
    bad["diffusion"] =
        json!({"textEncoder":"te.safetensors","clipType":"sdxl","vae":"v.safetensors"});
    assert!(compile(&bad).unwrap_err().contains("imageReference"));
    bad.as_object_mut().unwrap().remove("diffusion");
    bad["imageReference"]["images"][0]["image"] = json!("..");
    assert!(compile(&bad).unwrap_err().contains("imageReference"));
    bad["imageReference"]["images"][0]["image"] = json!("deas-inputs/ref.png");
    bad["imageReference"]["images"] = json!([]);
    assert!(compile(&bad).unwrap_err().contains("imageReference"));
}
