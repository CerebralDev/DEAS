use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn job(intent: Value) -> Job {
    Job {
        id: uuid::Uuid::new_v4().to_string(),
        plan: Plan {
            id: uuid::Uuid::new_v4().to_string(),
            base: reqwest::Url::parse("http://127.0.0.1:8188/private-path/").unwrap(),
            graph: compile(&intent).unwrap(),
            intent,
            consumed: true,
            media_root: None,
            local_models: LocalModels::default(),
        },
        client_id: uuid::Uuid::new_v4().to_string(),
        prompt_id: None,
        state: "unknown",
        outputs: vec![],
        history_outputs: vec![],
        error: Some("secret HTTP body token=private".into()),
        submitting: true,
        polling: false,
        metadata: None,
        live: Arc::default(),
    }
}
fn image() -> Value {
    super::super::tests::intent()
}
fn document(history: &History) -> Document {
    history.lock().document.clone()
}

#[test]
fn history_ten_eleven_admission_order_and_late_old_updates_never_resurrect() {
    let history = History::default();
    let mut first = job(image());
    history.admit(&first, "diagnostic");
    for n in 1..=10 {
        let mut intent = image();
        intent["seed"] = json!(n);
        history.admit(&job(intent), "studio");
    }
    assert_eq!(history.view().records.len(), 10);
    first.state = "succeeded";
    first.error = None;
    first.submitting = false;
    history.observe(&first);
    assert!(!history.view().records.iter().any(|r| r.id == first.id));
    assert_eq!(history.view().records[0].intent.0["seed"], 10);
    let oldest = history.view().records.last().unwrap().id.clone();
    assert_eq!(document(&history).records.first().unwrap().id, oldest);
}

#[test]
fn history_immutable_rich_settings_references_and_video_roundtrip() {
    let mut revision = image();
    revision["imageReference"] = json!({"visionModel":"clip_vision.safetensors","images":[{"image":"a.png","strength":0.25},{"image":"b.png","strength":0.875}]});
    revision["loras"] = json!([{"name":"detail.safetensors","strength":0.33}]);
    revision["inputImage"] = json!("source.png");
    revision["denoise"] = json!(0.55);
    revision["inputMask"] = json!("deas-masks/11111111-1111-4111-8111-111111111111.png");
    revision["maskBlur"] = json!(3);
    revision["inputLayers"] = json!([{"image":"layer.png","x":0,"y":0,"width":128,"height":128}]);
    revision["inpaintCrop"] =
        json!({"x":0,"y":0,"width":128,"height":128,"targetWidth":128,"targetHeight":128});
    revision["refine"] = json!({"width":1024,"height":1024,"denoise":0.4});
    let mut native = image();
    native["diffusion"] =
        json!({"textEncoder":"qwen.safetensors","clipType":"qwen_image","vae":"vae.safetensors"});
    native["nativeImageReference"] = json!({"method":"qwen-image-2.1","images":["b.png","a.png"]});
    let mut krea = native.clone();
    krea["diffusion"]["clipType"] = json!("krea2");
    krea["nativeImageReference"]["method"] = json!("krea-2");
    let mut moss = native.clone();
    moss["diffusion"]["clipType"] = json!("moss_image");
    moss["nativeImageReference"]["method"] = json!("moss-image");
    let history = History::default();
    for intent in [
        revision,
        native,
        krea,
        moss,
        crate::jobs_video::tests::ltx(),
        crate::jobs_video::tests::h3(),
    ] {
        let mut attempt = job(intent.clone());
        history.admit(&attempt, "studio");
        attempt.plan.intent["seed"] = json!(999);
        attempt.state = "cancelled";
        attempt.error = None;
        attempt.submitting = false;
        history.observe(&attempt);
        let current = history.view().records[0].clone();
        assert_eq!(current.intent.0, intent);
        let encoded = serde_json::to_vec(&current).unwrap();
        let read: HistoryRecord = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(read, current);
        validate_record(&read).unwrap();
    }
}

#[test]
fn history_projection_excludes_secrets_preview_graph_and_authority_and_keeps_timing() {
    let history = History::default();
    let mut attempt = job(image());
    attempt.live.lock().unwrap().preview = Some("data:image/png;base64,PRIVATE_PREVIEW".into());
    attempt.metadata =
        Some(json!({"startedAt":100,"generationMs":250,"token":"PRIVATE_TOKEN","bytes":[1,2,3]}));
    history.admit(&attempt, "diagnostic");
    let value = serde_json::to_value(history.view()).unwrap();
    let text = value.to_string();
    for forbidden in [
        "PRIVATE_PREVIEW",
        "PRIVATE_TOKEN",
        "secret HTTP",
        "private-path",
        "graph",
        "admissionEpoch",
        "preview",
        "extra_pnginfo",
    ] {
        assert!(!text.contains(forbidden), "{forbidden}");
    }
    assert_eq!(value["records"][0]["startedAt"], 100);
    assert_eq!(value["records"][0]["generationMs"], 250);
    assert_eq!(
        value["records"][0]["backendOrigin"],
        "http://127.0.0.1:8188"
    );
    let mut tampered = value["records"][0].clone();
    tampered["preview"] = json!("forbidden");
    assert!(serde_json::from_value::<HistoryRecord>(tampered).is_err());
    let mut tampered = value["records"][0].clone();
    tampered["intent"]["token"] = json!("forbidden");
    assert!(serde_json::from_value::<HistoryRecord>(tampered).is_err());
}

#[test]
fn history_unchanged_progress_does_not_schedule_writes_and_unknown_keeps_partial_products() {
    let history = History::default();
    let mut attempt = job(image());
    history.admit(&attempt, "studio");
    let revision = history.lock().revision;
    for n in 0..20 {
        attempt.live.lock().unwrap().value = n;
        history.observe(&attempt);
    }
    assert_eq!(history.lock().revision, revision);
    attempt.state = "cancelled";
    attempt.submitting = false;
    attempt.error = None;
    attempt.history_outputs =
        vec![json!({"filename":"partial.png","subfolder":"raw","type":"output"})];
    history.observe(&attempt);
    attempt.unknown("secret transport failure".into());
    attempt.history_outputs.clear();
    history.observe(&attempt);
    let record = &history.view().records[0];
    assert_eq!(record.state, "unknown");
    assert_eq!(record.outputs.len(), 1);
    assert!(!record.error.as_ref().unwrap().contains("secret"));
}

#[test]
fn history_restart_read_is_local_preserves_nonterminal_truth_and_never_touches_media() {
    let temp = tempfile::tempdir().unwrap();
    let media = temp.path().join("original.png");
    std::fs::write(&media, b"original").unwrap();
    let history = History::default();
    history.admit(&job(image()), "diagnostic");
    let expected = document(&history);
    let mut writer = Writer::open(temp.path()).unwrap();
    writer.save(&expected).unwrap();
    drop(writer);
    let reopened = Writer::open(temp.path()).unwrap();
    assert_eq!(reopened.document.records, expected.records);
    assert_eq!(reopened.document.records[0].state, "unknown");
    assert_eq!(std::fs::read(media).unwrap(), b"original");
}

#[test]
fn history_successful_worker_flush_reopens_latest_committed_updates() {
    let temp = tempfile::tempdir().unwrap();
    let history = History::default();
    history.initialize(temp.path());
    assert!(!history.view().read_only);
    assert!(history.view().error.is_none());

    let mut attempt = job(image());
    let original_intent = attempt.plan.intent.clone();
    history.admit(&attempt, "studio");
    let diagnostic = job(image());
    history.admit(&diagnostic, "diagnostic");
    attempt.submitting = false;
    attempt.state = "running";
    attempt.error = None;
    attempt.prompt_id = Some("11111111-1111-4111-8111-111111111111".into());
    attempt.metadata = Some(json!({"startedAt":1000,"generationMs":null}));
    // Submit a burst through the real bounded worker notification path. Its scheduling may
    // coalesce any number of these updates; only the final committed snapshot must survive.
    for index in 0..16 {
        attempt.outputs = vec![json!({
            "filename":format!("partial-{index}.png"),"subfolder":"raw","type":"output"
        })];
        history.observe(&attempt);
    }
    attempt.plan.intent["seed"] = json!(999);
    attempt.state = "failed";
    attempt.error = Some("private backend response token=secret".into());
    attempt.history_outputs = attempt.outputs.clone();
    attempt.outputs.clear();
    history.observe(&attempt);

    let committed_revision = history.lock().revision;
    let expected = history.view().records;
    history.flush();
    assert!(history.view().error.is_none());
    assert!(!history.view().read_only);
    assert_eq!(history.lock().persisted_revision, committed_revision);
    assert_eq!(expected[0].id, diagnostic.id);
    assert_eq!(expected[0].state, "unknown");
    assert_eq!(expected[1].state, "failed");
    assert_eq!(expected[1].outputs[0].filename, "partial-15.png");
    assert_eq!(expected[1].intent.0, original_intent);
    drop(history);

    // Synchronize with worker shutdown via its actual writer lock, with a bounded deadline.
    // No delay or backend activity is needed before opening a fresh production History.
    let root = crate::projects::Root::open(temp.path()).unwrap();
    let lock = root.file(LOCK, true).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while lock.try_lock().is_err() {
        assert!(
            std::time::Instant::now() < deadline,
            "The history worker did not release its scratch writer lock"
        );
        std::thread::yield_now();
    }
    drop(lock);
    let reopened = History::default();
    reopened.initialize(temp.path());
    assert!(!reopened.view().read_only);
    assert!(reopened.view().error.is_none());
    assert_eq!(reopened.view().records, expected);
}

#[test]
fn history_corrupt_unknown_schema_oversized_and_unsafe_records_preserved() {
    let temp = tempfile::tempdir().unwrap();
    for bytes in [
        b"not json".to_vec(),
        br#"{"schemaVersion":9,"records":[]}"#.to_vec(),
        br#"{"schemaVersion":1,"records":[],"token":"secret"}"#.to_vec(),
    ] {
        std::fs::write(temp.path().join(FILE), &bytes).unwrap();
        assert!(Writer::open(temp.path()).is_err());
        assert_eq!(std::fs::read(temp.path().join(FILE)).unwrap(), bytes);
    }
    let file = File::create(temp.path().join(FILE)).unwrap();
    file.set_len(FILE_BYTES + 1).unwrap();
    drop(file);
    assert!(Writer::open(temp.path()).is_err());
    assert_eq!(
        std::fs::metadata(temp.path().join(FILE)).unwrap().len(),
        FILE_BYTES + 1
    );
    let history = History::default();
    history.admit(&job(image()), "studio");
    let mut data = document(&history);
    data.records[0].outputs = vec![HistoryOutput {
        filename: "../private".into(),
        subfolder: "".into(),
        output_type: "output".into(),
        format: None,
    }];
    assert!(validate_record(&data.records[0]).is_err());
    data.records[0].outputs.clear();
    data.records[0].backend_origin += "/secret";
    assert!(validate_record(&data.records[0]).is_err());
}

#[test]
fn history_competing_writer_and_failed_publication_preserve_good_file_and_owned_temps() {
    let temp = tempfile::tempdir().unwrap();
    let history = History::default();
    history.admit(&job(image()), "studio");
    let mut writer = Writer::open(temp.path()).unwrap();
    writer.save(&document(&history)).unwrap();
    let original = std::fs::read(temp.path().join(FILE)).unwrap();
    assert!(Writer::open(temp.path()).is_err());
    // A new instance can still browse safely in explicit degraded/read-only mode.
    let reader = History::default();
    reader.initialize(temp.path());
    assert!(reader.view().read_only);
    assert!(reader.view().error.is_some());
    assert_eq!(reader.view().records.len(), 1);
    std::fs::write(temp.path().join(FILE), b"foreign writer").unwrap();
    assert!(writer.save(&document(&history)).is_err());
    assert_eq!(
        std::fs::read(temp.path().join(FILE)).unwrap(),
        b"foreign writer"
    );
    assert!(!original.is_empty());
    assert!(
        !std::fs::read_dir(temp.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp"))
    );
}

#[test]
fn history_failed_first_publication_does_not_clobber_a_competing_target() {
    let temp = tempfile::tempdir().unwrap();
    let mut writer = Writer::open(temp.path()).unwrap();
    std::fs::write(temp.path().join(FILE), b"competing target").unwrap();
    assert!(
        writer
            .save(&Document {
                schema_version: 1,
                records: vec![],
            })
            .is_err()
    );
    assert_eq!(
        std::fs::read(temp.path().join(FILE)).unwrap(),
        b"competing target"
    );
    assert!(
        !std::fs::read_dir(temp.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp"))
    );
}

#[test]
fn history_full_valid_wildcard_corpus_fits_without_truncation() {
    let mut intent = image();
    let mut selections = serde_json::Map::new();
    // Controls deliberately exercise JSON's longest 6-byte escaping for valid UTF16 keys.
    for n in 0..4096 {
        selections.insert(
            format!("{n:04}{}", "\u{1}".repeat(508)),
            json!({"enabled":false,"weight":0}),
        );
    }
    intent["wildcard"] = json!({"style":"Booru","seed":0,"reroll":0,"token_budget":10000,"auto_reroll":false,"selection_state":selections});
    let attempt = job(intent.clone());
    let history = History::default();
    history.admit(&attempt, "studio");
    let record = history.view().records.remove(0);
    let bytes = serde_json::to_vec(&record).unwrap();
    assert!(bytes.len() > 12 * 1024 * 1024 && bytes.len() < RECORD_BYTES);
    validate_record(&record).unwrap();
    let decoded: HistoryRecord = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded.intent.0, intent);
}

#[test]
fn history_video_omitted_output_defaults_preserve_native_execution_meaning() {
    let mut intent = crate::jobs_video::tests::ltx();
    intent.as_object_mut().unwrap().remove("outputPrefix");
    intent.as_object_mut().unwrap().remove("output");
    let graph = compile(&intent).unwrap();
    let history = History::default();
    history.admit(&job(intent.clone()), "diagnostic");
    let restored = &history.view().records[0].intent.0;
    assert_eq!(restored["outputPrefix"], "raw/video/DEAS");
    assert_eq!(restored["output"]["audioBitrate"], "192k");
    assert_eq!(
        restored["output"],
        crate::jobs_video::effective_output(&intent)
    );
    assert_eq!(compile(restored).unwrap(), graph);
}

#[test]
fn history_output_availability_requires_approved_root_identity_and_proven_absence() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("output");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(root.join("raw")).unwrap();
    std::fs::write(root.join("raw/exists.png"), b"unchanged media").unwrap();
    std::fs::create_dir(root.join("raw/directory.png")).unwrap();
    let history = History::default();
    let mut attempt = job(image());
    attempt.plan.media_root = Some(job_outputs::MediaRoot::capture(root.clone()).unwrap());
    attempt.outputs = vec![
        json!({"filename":"exists.png","subfolder":"raw","type":"output"}),
        json!({"filename":"missing.png","subfolder":"raw","type":"output"}),
        json!({"filename":"missing.png","subfolder":"missing-parent","type":"output"}),
        json!({"filename":"directory.png","subfolder":"raw","type":"output"}),
    ];
    history.admit(&attempt, "studio");
    assert_eq!(
        history.outputs(&attempt.id, &root).unwrap(),
        vec!["available", "missing", "missing", "unvalidated"]
    );
    assert!(history.outputs(&attempt.id, temp.path()).is_err());
    assert!(history.outputs("unknown", &root).is_err());
    #[cfg(target_os = "linux")]
    {
        std::os::unix::fs::symlink(root.join("raw/exists.png"), root.join("raw/linked.png"))
            .unwrap();
        attempt
            .outputs
            .push(json!({"filename":"linked.png","subfolder":"raw","type":"output"}));
        history.observe(&attempt);
        assert_eq!(
            history.outputs(&attempt.id, &root).unwrap()[4],
            "unvalidated"
        );
    }
    history.lock().document.records[0].output_root_identity = Some("0:0".into());
    assert!(history.outputs(&attempt.id, &root).is_err());
    assert_eq!(
        std::fs::read(root.join("raw/exists.png")).unwrap(),
        b"unchanged media"
    );
}

#[test]
fn history_restart_relocation_preserves_executed_intent_and_rejects_replaced_root() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("output");
    std::fs::create_dir(&output).unwrap();
    let root = job_outputs::MediaRoot::capture(output.clone()).unwrap();
    let identity = root.recorded_identity();
    let history = History::default();
    let mut attempt = job(image());
    attempt.plan.media_root = Some(root);
    attempt.outputs = vec![json!({"filename":"a.png","subfolder":"raw","type":"output"})];
    history.admit(&attempt, "studio");
    let intent = attempt.plan.intent.clone();
    let mut writer = Writer::open(temp.path()).unwrap();
    writer.save(&document(&history)).unwrap();
    drop(writer);
    let restarted = History::default();
    restarted.lock().document = Writer::open(temp.path()).unwrap().document;
    let source = output.join("raw/a.png");
    let target = output.join("starred/a.png");
    restarted.relocate(
        &output,
        (identity.0, identity.1.wrapping_add(1)),
        &source,
        &target,
    );
    assert_eq!(restarted.view().records[0].outputs[0].subfolder, "raw");
    restarted.relocate(&output, identity, &source, &target);
    assert_eq!(restarted.view().records[0].outputs[0].subfolder, "starred");
    assert_eq!(restarted.view().records[0].intent.0, intent);
}

#[cfg(target_os = "linux")]
#[test]
fn history_symlink_parent_cannot_create_directories_outside_app_data() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), temp.path().join("link")).unwrap();
    assert!(Writer::open(&temp.path().join("link/new-app-data")).is_err());
    assert!(!outside.path().join("new-app-data").exists());
}

#[tokio::test]
async fn history_consumed_diagnostic_submission_survives_waiter_loss_without_retry() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let posts = Arc::new(AtomicUsize::new(0));
    let observed = posts.clone();
    let (received, mut request_seen) = tokio::sync::mpsc::channel(1);
    let (release, released) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let mut released = Some(released);
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![];
            loop {
                let mut chunk = [0; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                if n == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..n]);
                if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let header_end = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
            let length = String::from_utf8_lossy(&bytes[..header_end])
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                .map_or(0, |(_, value)| value.trim().parse::<usize>().unwrap());
            while bytes.len() < header_end + length {
                let mut chunk = [0; 4096];
                let n = socket.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
            }
            let post = bytes.starts_with(b"POST /prompt ");
            if post {
                observed.fetch_add(1, Ordering::SeqCst);
                received.send(()).await.unwrap();
                released.take().unwrap().await.unwrap();
            }
            let body = if post {
                json!({"prompt_id":"11111111-1111-4111-8111-111111111111","number":0,"node_errors":{}}).to_string()
            } else {
                "{}".into()
            };
            let code = if post { 200 } else { 404 };
            socket
                .write_all(
                    format!("HTTP/1.1 {code} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
                        .as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    let jobs = Jobs::default();
    let mut attempt = job(image());
    attempt.plan.base = base;
    let id = attempt.plan.id.clone();
    jobs.lock().unwrap().plan = Some(Plan {
        consumed: false,
        ..attempt.plan
    });
    let waiter_jobs = jobs.clone();
    let waiter = tokio::spawn(async move { waiter_jobs.submit(id, true).await });
    request_seen.recv().await.unwrap();
    assert_eq!(jobs.3.view().records.len(), 1);
    assert_eq!(jobs.3.view().records[0].state, "unknown");
    waiter.abort();
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if jobs.3.view().records[0].state == "queued" {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(posts.load(Ordering::SeqCst), 1);
    assert_eq!(jobs.3.view().records[0].origin, "diagnostic");
    server.abort();
}

#[test]
fn history_error_delivery_is_shared_redacted_rate_limited_and_visible() {
    let history = History::default();
    let before = crate::logs::native_logs(0).last().map_or(0, |e| e.seq);
    history.unavailable();
    history.unavailable();
    assert!(history.view().error.is_some());
    assert!(history.view().read_only);
    let entries: Vec<_> = crate::logs::native_logs(before)
        .into_iter()
        .filter(|e| e.text.contains("Generation history operation=initialize"))
        .collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source, crate::logs::Source::Deas);
    assert!(!entries[0].text.contains("http"));
    let main = include_str!("main.rs");
    assert!(main.contains("deas::native_jobs_history,"));
    assert!(main.contains("deas::native_logs_history_error,"));
    let permission: toml::Value = toml::from_str(include_str!("../permissions/api.toml")).unwrap();
    let entry = permission["permission"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["identifier"].as_str() == Some("allow-native-jobs-history"))
        .unwrap();
    assert_eq!(entry["commands"]["allow"].as_array().unwrap().len(), 3);
}
