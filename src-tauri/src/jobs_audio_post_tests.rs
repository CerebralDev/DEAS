use super::*;

fn intent() -> Value {
    let mut intent = crate::jobs_music::tests::full();
    intent["audioEnhance"] = json!({"mode":"clean","profile":"mix"});
    intent
}

#[test]
fn audio_intent_is_optional_strict_and_does_not_change_backend_graph() {
    let mut value = intent();
    let graph = compile(&value).unwrap();
    value.as_object_mut().unwrap().remove("audioEnhance");
    assert_eq!(compile(&value).unwrap(), graph);
    assert!(settings(&value).unwrap().is_none());
    for invalid in [
        Value::Null,
        json!({}),
        json!({"mode":"off","profile":"mix"}),
        json!({"mode":"deep","profile":"mix"}),
        json!({"mode":"clean","profile":"mix","extra":true}),
    ] {
        value["audioEnhance"] = invalid;
        assert!(compile(&value).is_err());
    }
    value["audioEnhance"] = json!({"mode":"deep","profile":"speech"});
    assert!(
        compile(&value).is_ok(),
        "history compilation must not require installed engines"
    );
    let mut image = super::super::tests::intent();
    image["audioEnhance"] = json!({"mode":"clean","profile":"mix"});
    assert!(compile(&image).is_err());
}

async fn held_fixture(root: &std::path::Path) -> (Jobs, Arc<Jobs>, tokio::task::JoinHandle<()>) {
    held_fixture_with(root, intent(), "original.flac").await
}

async fn held_fixture_with(
    root: &std::path::Path,
    intent: Value,
    filename: &str,
) -> (Jobs, Arc<Jobs>, tokio::task::JoinHandle<()>) {
    let graph = compile(&intent).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    let client = uuid::Uuid::new_v4().to_string();
    let history = json!({id.clone():{
        "prompt":[1,id,graph,{"client_id":client},["7"]],
        "status":{"completed":true,"status_str":"success","messages":[]},
        "outputs":{"7":{"audio":[{"filename":filename,"subfolder":"","type":"output"}]}}
    }});
    let (base, server) = super::super::tests::server(move |_, _| (200, history.to_string())).await;
    let jobs = Jobs::default();
    let held = Arc::new(Jobs(
        Arc::default(),
        Arc::default(),
        Arc::default(),
        jobs.3.clone(),
    ));
    let job = Job {
        id: id.clone(),
        plan: Plan {
            id: id.clone(),
            base: reqwest::Url::parse(&base).unwrap(),
            intent,
            graph,
            consumed: true,
            media_root: Some(job_outputs::MediaRoot::capture(root.into()).unwrap()),
            local_models: LocalModels::default(),
        },
        client_id: client,
        prompt_id: Some(id),
        state: "queued",
        outputs: vec![],
        history_outputs: vec![],
        error: None,
        submitting: false,
        polling: false,
        metadata: None,
        live: Arc::default(),
    };
    held.3.admit(&job, "studio");
    held.lock().unwrap().job = Some(job);
    jobs.lock().unwrap().workbench.push(held.clone());
    (jobs, held, server)
}

async fn settle(held: &Jobs) -> Value {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let snapshot = held.current().unwrap();
            if matches!(
                snapshot["state"].as_str(),
                Some("succeeded" | "failed" | "cancelled")
            ) {
                break snapshot;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}

fn original(root: &std::path::Path) -> Vec<u8> {
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100:duration=8",
            "-c:a",
            "flac",
        ])
        .arg(root.join("original.flac"))
        .output()
        .expect("FFmpeg fixture prerequisite");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::read(root.join("original.flac")).unwrap()
}

#[tokio::test]
#[ignore = "requires real CPU FFmpeg/FFprobe; no model execution"]
async fn automatic_audio_name_preserves_resolved_source_and_subfolder() {
    let dir = tempfile::tempdir().unwrap();
    let subfolder = "raw/music/2026-11-07";
    let folder = dir.path().join(subfolder);
    std::fs::create_dir_all(&folder).unwrap();
    let before = original(&folder);
    let stem = "20-38-22-4041897001-dasiwa-model-00001-audio";
    let filename = format!("{stem}.flac");
    std::fs::rename(folder.join("original.flac"), folder.join(&filename)).unwrap();
    let (_, held, server) = held_fixture(dir.path()).await;
    let mut job = held.lock().unwrap().job.as_ref().unwrap().clone();
    job.outputs = vec![json!({"filename":filename,"subfolder":subfolder,"type":"output"})];
    let mut outputs = job.outputs.clone();
    process(&job, &AtomicBool::new(false), &mut outputs).unwrap();
    assert_eq!(
        outputs[0]["filename"],
        format!("{stem}-audio-enhanced.flac")
    );
    assert_eq!(outputs[0]["subfolder"], subfolder);
    assert!(folder
        .join(outputs[0]["filename"].as_str().unwrap())
        .exists());
    assert_eq!(std::fs::read(folder.join(filename)).unwrap(), before);
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 2);
    server.abort();
}

#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe; run audio_post --ignored explicitly"]
async fn automatic_audio_clean_holds_job_then_retains_pair_and_history_without_frontend() {
    let dir = tempfile::tempdir().unwrap();
    let before = original(dir.path());
    std::fs::write(
        dir.path().join("original-audio-enhanced.flac"),
        b"keep existing",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("original-audio-enhanced-00002.flac"),
        b"keep second",
    )
    .unwrap();
    let (jobs, held, server) = held_fixture(dir.path()).await;
    let id = held.current().unwrap()["id"].as_str().unwrap().to_owned();
    let stage = held.status(id.clone()).await.unwrap();
    assert_eq!(stage["state"], "running");
    assert!(held.lock().unwrap().audio_post.is_some());
    assert_eq!(
        serde_json::to_value(jobs.3.view()).unwrap()["records"][0]["state"],
        "running"
    );
    let done = settle(&held).await;
    assert_eq!(done["state"], "succeeded", "{done}");
    assert_eq!(done["outputs"].as_array().unwrap().len(), 2);
    assert_eq!(done["outputs"][0]["role"], "processed");
    assert_eq!(done["outputs"][1]["role"], "original");
    assert_eq!(
        std::fs::read(dir.path().join("original.flac")).unwrap(),
        before
    );
    let processed = jobs.audio_source(&id, 0).unwrap();
    let processed = std::fs::read(processed).unwrap();
    assert_ne!(processed, before);
    job_outputs::validate_audio_post_flac(&processed).unwrap();
    assert!(job_outputs::validate_audio_post_flac(&processed[..processed.len() / 2]).is_err());
    let record = serde_json::to_value(jobs.3.view()).unwrap()["records"][0].clone();
    assert_eq!(record["state"], "succeeded");
    assert_eq!(record["outputs"][0]["role"], "processed");
    assert_eq!(
        done["outputs"][0]["filename"],
        "original-audio-enhanced-00003.flac"
    );
    assert_eq!(
        record["outputs"][0]["filename"],
        done["outputs"][0]["filename"]
    );
    assert_eq!(
        std::fs::read(dir.path().join("original-audio-enhanced.flac")).unwrap(),
        b"keep existing"
    );
    assert_eq!(
        std::fs::read(dir.path().join("original-audio-enhanced-00002.flac")).unwrap(),
        b"keep second"
    );
    assert!(!std::fs::read_dir(dir.path()).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".deas-audio-")));
    assert_eq!(record["intent"]["audioEnhance"], intent()["audioEnhance"]);
    assert!(jobs
        .audio_source(&id, 1)
        .unwrap()
        .ends_with("original.flac"));
    server.abort();
}

#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe; run audio_post --ignored explicitly"]
async fn automatic_audio_tve_video_retains_encoded_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("baseline.MKV");
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=black:s=32x32:r=12:d=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100:duration=1",
            "-c:v",
            "ffv1",
            "-c:a",
            "flac",
            "-shortest",
        ])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let before = std::fs::read(&source).unwrap();
    let value = json!({"recipeId":"video-enhance", "version":1, "sourceVideo":"source.mkv", "frameRate":12, "seed":7, "videoPost":{"upscaleModel":"2x.safetensors"}, "audioEnhance":{"mode":"clean","profile":"mix"}});
    let mut generation = crate::jobs_video::tests::ltx();
    generation["audioEnhance"] = json!({"mode":"clean","profile":"mix"});
    for value in [value, generation] {
        let (jobs, held, server) = held_fixture_with(dir.path(), value, "baseline.MKV").await;
        held.start_audio_observer();
        let done = settle(&held).await;
        assert_eq!(done["state"], "succeeded", "{done}");
        assert_eq!(done["outputs"][0]["format"], "video/x-matroska");
        assert_eq!(done["outputs"][1]["role"], "original");
        assert_eq!(std::fs::read(&source).unwrap(), before);
        let id = done["id"].as_str().unwrap();
        assert!(jobs.audio_source(id, 0).unwrap().exists());
        server.abort();
    }
}

#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe; run audio_post --ignored explicitly"]
async fn automatic_audio_observer_finishes_without_ui_status_requests() {
    let dir = tempfile::tempdir().unwrap();
    original(dir.path());
    let (_, held, initial_server) = held_fixture(dir.path()).await;
    initial_server.abort();
    let prompt_id = uuid::Uuid::new_v4().to_string();
    let backend_id = prompt_id.clone();
    let submissions = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let submitted = submissions.clone();
    let history_requests = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let requests = history_requests.clone();
    let mut submission = Value::Null;
    let (base, server) = super::super::tests::server(move |path, body| {
        if path.starts_with("POST /comfy/prompt ") {
            submitted.fetch_add(1, Ordering::SeqCst);
            submission = body;
            return (200, json!({"prompt_id":backend_id,"number":1,"node_errors":{}}).to_string());
        }
        assert!(path.starts_with("GET /comfy/history/"), "{path}");
        if requests.fetch_add(1, Ordering::SeqCst) == 0 { return (503, "{}".into()); }
        (200, json!({backend_id.clone():{
            "prompt":[1,backend_id,submission["prompt"],{"client_id":submission["client_id"]},["7"]],
            "status":{"completed":true,"status_str":"success","messages":[]},
            "outputs":{"7":{"audio":[{"filename":"original.flac","subfolder":"","type":"output"}]}}
        }}).to_string())
    }).await;
    let plan_id = {
        let mut session = held.lock().unwrap();
        let mut plan = session.job.take().unwrap().plan;
        plan.base = crate::validate_base_url(&base).unwrap();
        plan.consumed = false;
        let id = plan.id.clone();
        session.plan = Some(plan);
        id
    };
    held.submit(plan_id, true).await.unwrap();
    // No status/poll calls after submission: the native task owns reconciliation.
    let done = settle(&held).await;
    assert_eq!(submissions.load(Ordering::SeqCst), 1);
    assert!(history_requests.load(Ordering::SeqCst) >= 2);
    assert_eq!(done["state"], "succeeded", "{done}");
    assert_eq!(done["outputs"].as_array().unwrap().len(), 2);
    server.abort();
}

#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe; run audio_post --ignored explicitly"]
async fn automatic_audio_silent_video_is_noop_without_receipt_or_pair() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("silent.mkv");
    assert!(std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=black:s=32x32:r=12:d=1",
            "-c:v",
            "ffv1"
        ])
        .arg(&source)
        .status()
        .unwrap()
        .success());
    let before = std::fs::read(&source).unwrap();
    let value = json!({"recipeId":"video-enhance", "version":1, "sourceVideo":"source.mkv", "frameRate":12, "seed":7, "videoPost":{"upscaleModel":"2x.safetensors"}, "audioEnhance":{"mode":"clean","profile":"mix"}});
    let (_, held, server) = held_fixture_with(dir.path(), value, "silent.mkv").await;
    held.start_audio_observer();
    let done = settle(&held).await;
    assert_eq!(done["state"], "succeeded", "{done}");
    assert_eq!(done["outputs"].as_array().unwrap().len(), 1);
    assert_eq!(done["outputs"][0]["filename"], "silent.mkv");
    assert!(done["outputs"][0].get("role").is_none());
    assert_eq!(std::fs::read(&source).unwrap(), before);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    server.abort();
}

#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe; run audio_post --ignored explicitly"]
async fn automatic_audio_shutdown_cancels_held_stage() {
    let dir = tempfile::tempdir().unwrap();
    original(dir.path());
    let (jobs, held, server) = held_fixture(dir.path()).await;
    let id = held.current().unwrap()["id"].as_str().unwrap().to_owned();
    assert_eq!(held.status(id).await.unwrap()["state"], "running");
    jobs.cancel_audio_stages();
    let done = settle(&held).await;
    assert_eq!(done["state"], "cancelled", "{done}");
    assert_eq!(done["outputs"].as_array().unwrap().len(), 1);
    assert!(held.lock().unwrap().stop_requested);
    server.abort();
}

#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe; run audio_post --ignored explicitly"]
async fn automatic_audio_later_failure_exposes_completed_partial() {
    let dir = tempfile::tempdir().unwrap();
    original(dir.path());
    let (_, held, server) = held_fixture(dir.path()).await;
    let cancel = Arc::new(AtomicBool::new(false));
    let job = {
        let mut session = held.lock().unwrap();
        session.audio_post = Some(cancel.clone());
        let job = session.job.as_mut().unwrap();
        job.state = "running";
        job.outputs = vec![
            json!({"filename":"original.flac","subfolder":"","type":"output"}),
            json!({"filename":"missing.flac","subfolder":"","type":"output"}),
        ];
        job.clone()
    };
    held.start_audio_post(job, cancel);
    let done = settle(&held).await;
    assert_eq!(done["state"], "failed", "{done}");
    assert_eq!(done["outputs"].as_array().unwrap().len(), 3);
    assert_eq!(done["outputs"][0]["role"], "processed");
    assert!(dir
        .path()
        .join(done["outputs"][0]["filename"].as_str().unwrap())
        .exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    server.abort();
}

#[tokio::test]
async fn automatic_audio_failure_keeps_original_handle_and_history() {
    let dir = tempfile::tempdir().unwrap();
    let (jobs, held, server) = held_fixture(dir.path()).await;
    held.start_audio_observer();
    let done = settle(&held).await;
    assert_eq!(done["state"], "failed", "{done}");
    assert_eq!(done["outputs"][0]["filename"], "original.flac");
    assert_eq!(done["outputs"][0]["role"], "original");
    assert_eq!(
        serde_json::to_value(jobs.3.view()).unwrap()["records"][0]["outputs"][0]["role"],
        "original"
    );
    server.abort();
}

#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe; run audio_post --ignored explicitly"]
async fn automatic_audio_whole_stop_does_not_require_backend_authority() {
    let dir = tempfile::tempdir().unwrap();
    original(dir.path());
    let (jobs, held, server) = held_fixture(dir.path()).await;
    let id = held.current().unwrap()["id"].as_str().unwrap().to_owned();
    assert_eq!(held.status(id).await.unwrap()["state"], "running");
    let result = jobs
        .workbench_stop(
            true,
            Arc::new(|_| panic!("local audio Stop must not consult backend authority")),
            Arc::new(|_| panic!("local audio Stop must not interrupt backend")),
        )
        .await
        .unwrap();
    assert_eq!(result["outcome"], "settled", "{result}");
    assert_eq!(held.current().unwrap()["state"], "cancelled");
    server.abort();
}

#[tokio::test]
#[ignore = "requires real FFmpeg/FFprobe; run audio_post --ignored explicitly"]
async fn automatic_audio_cancel_is_scoped_and_preserves_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let before = original(dir.path());
    let (jobs, held, server) = held_fixture(dir.path()).await;
    let id = held.current().unwrap()["id"].as_str().unwrap().to_owned();
    assert_eq!(held.status(id.clone()).await.unwrap()["state"], "running");
    jobs.interrupt_running(
        id.clone(),
        true,
        Arc::new(|_| panic!("audio cancellation must not interrupt backend")),
    )
    .await
    .unwrap();
    let done = settle(&held).await;
    assert_eq!(done["state"], "cancelled", "{done}");
    assert_eq!(done["outputs"].as_array().unwrap().len(), 1);
    assert_eq!(done["outputs"][0]["role"], "original");
    assert_eq!(
        std::fs::read(jobs.audio_source(&id, 0).unwrap()).unwrap(),
        before
    );
    assert_eq!(
        serde_json::to_value(jobs.3.view()).unwrap()["records"][0]["state"],
        "cancelled"
    );
    server.abort();
}
