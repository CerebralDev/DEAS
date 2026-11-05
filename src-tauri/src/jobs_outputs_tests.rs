use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![];
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&vec![127; width as usize * height as usize * 4])
            .unwrap();
    }
    bytes
}

fn completed(base: &str) -> Jobs {
    let jobs = Jobs::default();
    let intent = super::super::tests::intent();
    jobs.lock().unwrap().job = Some(Job {
        id: "job-a".into(),
        plan: Plan {
            id: "plan".into(),
            base: reqwest::Url::parse(base).unwrap(),
            graph: compile(&intent).unwrap(),
            intent,
            consumed: true,
            media_root: None,
            local_models: LocalModels::default(),
        },
        client_id: "client".into(),
        prompt_id: None,
        state: "succeeded",
        outputs: vec![json!({"filename":"image.png","subfolder":"DEAS/run-1","type":"output"})],
        history_outputs: vec![],
        error: None,
        submitting: false,
        polling: false,
        metadata: None,
        live: Arc::default(),
    });
    jobs
}

#[tokio::test]
async fn latent_video_history_requires_encoded_pairs_and_retains_terminal_products() {
    for (status, present, expected) in [
        ("success", "both", "succeeded"),
        ("success", "frames", "failed"),
        ("success", "7", "failed"),
        ("success", "B7", "failed"),
        ("error", "B7", "failed"),
        ("interrupted", "B7", "cancelled"),
        ("error", "7", "failed"),
        ("interrupted", "7", "cancelled"),
        ("error", "none", "failed"),
        ("interrupted", "none", "cancelled"),
    ] {
        let graph = json!({
            "7":{"class_type":"DaSiWa_EnhancedVideoCombine","inputs":{}},
            "B7":{"class_type":"DaSiWa_EnhancedVideoCombine","inputs":{}}
        });
        let tuple = json!([0,"prompt-video",graph,{"client_id":"client"},["7","B7"]]);
        let (base, task) = super::super::tests::server(move |path, _| {
            assert_eq!(path, "GET /comfy/history/prompt-video HTTP/1.1");
            let mut outputs = json!({});
            for node in ["7", "B7"] {
                if present != "none" {
                    outputs[node] = json!({"images":[{"filename":format!("{node}_first.png"),"subfolder":"video","type":"output"}]});
                    if present == "both" || present == node {
                        outputs[node]["gifs"] = json!([{"filename":format!("{node}.webm"),"subfolder":"video","type":"output","format":"video/webm"}]);
                    }
                }
            }
            let messages = match status {
                "error" => json!([["execution_error",{"prompt_id":"prompt-video","node_id":"LU","node_type":"LatentUpscale","exception_message":"upscale failed"}]]),
                "interrupted" => json!([["execution_interrupted",{"prompt_id":"prompt-video"}]]),
                _ => json!([]),
            };
            (200, json!({"prompt-video":{"prompt":tuple,"outputs":outputs,"status":{
                "completed":status=="success","status_str":if status=="success" {"success"} else {"error"},"messages":messages
            }}}).to_string())
        }).await;
        let dir = tempfile::tempdir().unwrap();
        let jobs = completed(&format!("{base}/"));
        {
            let mut session = jobs.lock().unwrap();
            let job = session.job.as_mut().unwrap();
            job.plan.graph = graph;
            // Result handling uses the immutable graph, not current authoring settings.
            job.plan.intent =
                json!({"kind":"video","latentUpscale":{"model":"learned.safetensors"}});
            job.plan.media_root = Some(MediaRoot::capture(dir.path().into()).unwrap());
            job.prompt_id = Some("prompt-video".into());
            job.state = "running";
            job.outputs.clear();
        }
        let snapshot = jobs.status("job-a".into()).await.unwrap();
        assert_eq!(
            snapshot["state"], expected,
            "{status}/{present}: {snapshot}"
        );
        let session = jobs.lock().unwrap();
        let job = session.job.as_ref().unwrap();
        let videos: Vec<_> = job
            .outputs
            .iter()
            .filter(|o| o["format"] == "video/webm")
            .collect();
        assert_eq!(
            videos.len(),
            match present {
                "both" => 2,
                "7" | "B7" => 1,
                _ => 0,
            }
        );
        for video in videos {
            assert_eq!(
                video["role"],
                if video["filename"] == "7.webm" {
                    "processed"
                } else {
                    "original"
                }
            );
        }
        if present == "none" {
            assert!(session.completed.is_empty());
        } else {
            assert_eq!(session.completed.len(), 1);
            assert_eq!(session.completed[0].outputs, job.outputs);
        }
        if status == "error" {
            assert!(job.error.as_deref().unwrap().contains("upscale failed"));
        } else if status == "interrupted" {
            assert!(job.error.is_none());
        } else if expected == "failed" {
            assert!(job.error.is_some());
        }
        task.abort();
    }
}

#[test]
fn retained_video_pair_relocation_preserves_roles_indices_and_source_bytes() {
    for state in ["succeeded", "failed", "cancelled"] {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let jobs = completed("http://127.0.0.1:8188/");
        let outputs = vec![
            json!({"filename":"processed.webm","subfolder":"video/run-1","type":"output","format":"video/webm","role":"processed"}),
            json!({"filename":"original.webm","subfolder":"video/run-1","type":"output","format":"video/webm","role":"original"}),
        ];
        {
            let mut session = jobs.lock().unwrap();
            let job = session.job.as_mut().unwrap();
            job.state = state;
            job.outputs = outputs.clone();
            job.history_outputs = outputs.clone();
            job.plan.media_root = Some(MediaRoot::capture(root.clone()).unwrap());
            session.retain_completed();
        }
        let source = output_path(&root, &outputs[0]);
        let original = output_path(&root, &outputs[1]);
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        // Opaque bytes: relocation must not transcode or claim to validate encoded media.
        std::fs::write(&source, b"processed-video-fixture").unwrap();
        std::fs::write(&original, b"original-video-fixture").unwrap();
        let target = root.join("starred/processed.webm");
        std::fs::create_dir(target.parent().unwrap()).unwrap();
        let intent = jobs
            .lock()
            .unwrap()
            .job
            .as_ref()
            .unwrap()
            .plan
            .intent
            .clone();
        jobs.relocate_media(&root, &source, || {
            std::fs::rename(&source, &target).unwrap();
            Ok(target.clone())
        })
        .unwrap();
        let session = jobs.lock().unwrap();
        let job = session.job.as_ref().unwrap();
        assert_eq!(job.outputs[0]["filename"], "processed.webm");
        assert_eq!(job.outputs[0]["subfolder"], "starred");
        assert_eq!(job.outputs[0]["role"], "processed");
        assert_eq!(job.outputs[0]["format"], "video/webm");
        assert_eq!(job.outputs[1], outputs[1]);
        assert_eq!(job.history_outputs, job.outputs);
        assert_eq!(session.completed[0].outputs, job.outputs);
        assert_eq!(job.plan.intent, intent);
        assert_eq!(std::fs::read(target).unwrap(), b"processed-video-fixture");
        assert_eq!(std::fs::read(original).unwrap(), b"original-video-fixture");
        assert!(!source.exists());
    }
}

#[test]
fn snapshot_output_root_requires_original_directory_identity() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("output");
    std::fs::create_dir(&root).unwrap();
    let jobs = completed("http://original:7801/");
    assert!(jobs.current().unwrap().get("outputRoot").is_none());
    jobs.lock().unwrap().job.as_mut().unwrap().plan.media_root =
        Some(MediaRoot::capture(root.clone()).unwrap());
    assert_eq!(jobs.current().unwrap()["outputRoot"], json!(root));
    std::fs::rename(&root, temp.path().join("displaced")).unwrap();
    assert!(jobs.current().unwrap().get("outputRoot").is_none());
    std::fs::create_dir(&root).unwrap();
    assert!(jobs.current().unwrap().get("outputRoot").is_none());
}

fn register_completed(jobs: &Jobs, root: &std::path::Path) {
    let mut session = jobs.lock().unwrap();
    session.job.as_mut().unwrap().plan.media_root = Some(MediaRoot::capture(root.into()).unwrap());
    session.retain_completed();
    for output in &session.job.as_ref().unwrap().outputs {
        let path = output_path(root, output);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, png_bytes(1, 1)).unwrap();
    }
}

#[test]
fn history_physical_move_succeeds_when_optional_metadata_publication_fails() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("output");
    std::fs::create_dir(&root).unwrap();
    let jobs = completed("http://127.0.0.1:8188/");
    {
        let mut session = jobs.lock().unwrap();
        let job = session.job.as_mut().unwrap();
        job.id = uuid::Uuid::new_v4().to_string();
        job.client_id = uuid::Uuid::new_v4().to_string();
    }
    register_completed(&jobs, &root);
    let history = temp.path().join("history");
    jobs.initialize_history(&history);
    {
        let session = jobs.lock().unwrap();
        jobs.3.admit(session.job.as_ref().unwrap(), "studio");
    }
    jobs.flush_history();
    let saved = std::fs::read(history.join("recent-generation-history.json")).unwrap();
    // Replace only our scratch lock to force a publication refusal while keeping good JSON intact.
    std::fs::rename(
        history.join(".recent-generation-history.lock"),
        history.join("held-lock"),
    )
    .unwrap();
    std::fs::write(history.join(".recent-generation-history.lock"), b"").unwrap();
    let source = root.join("DEAS/run-1/image.png");
    let original = std::fs::read(&source).unwrap();
    let target = root.join("starred/image.png");
    std::fs::create_dir(target.parent().unwrap()).unwrap();
    let moves = std::sync::atomic::AtomicUsize::new(0);
    let moved = jobs
        .relocate_media(&root, &source, || {
            moves.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::fs::rename(&source, &target).unwrap();
            Ok(target.clone())
        })
        .unwrap();
    jobs.flush_history();
    assert_eq!(moved, target);
    assert_eq!(moves.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(!source.exists());
    assert_eq!(std::fs::read(&target).unwrap(), original);
    assert_eq!(
        std::fs::read(history.join("recent-generation-history.json")).unwrap(),
        saved
    );
    let view = serde_json::to_value(jobs.3.view()).unwrap();
    assert!(view["readOnly"].as_bool().unwrap());
    assert!(view["error"].is_string());
    assert_eq!(view["records"][0]["outputs"][0]["subfolder"], "starred");
}

#[cfg(target_os = "linux")]
#[test]
fn owned_completion_root_tracks_launch_not_saved_settings_or_another_instance() {
    use crate::backend::tests::{SERVER, await_state, fixture};
    let script = SERVER.replace(
        "port=int(args[args.index('--port')+1])",
        "port=int(args[args.index('--port')+1])\nwith open('launch-output.txt','w') as f: f.write(args[args.index('--output-directory')+1])",
    );
    let (environment, approved) = fixture(&script);
    let output_a = environment.path().join("output-a");
    let output_b = environment.path().join("output-b");
    for path in [&output_a, &output_b] {
        std::fs::create_dir(path).unwrap();
    }
    let save_output = |output: &std::path::Path| {
        crate::settings::write(
            environment.path(),
            crate::settings::Folders {
                output: Some(output.to_str().unwrap().into()),
                ..Default::default()
            },
        )
        .unwrap();
    };
    save_output(&output_a);
    let engine = crate::install::Engine::default();
    engine.backend.start(approved.clone()).unwrap();
    let status = await_state(&engine.backend, "running");
    let endpoint = status.endpoint.unwrap();
    save_output(&output_b);
    assert_eq!(
        std::fs::read_to_string(environment.path().join("ComfyUI/launch-output.txt")).unwrap(),
        output_a.to_str().unwrap()
    );
    assert_eq!(
        engine.backend.status().endpoint.as_deref(),
        Some(endpoint.as_str())
    );
    let proof = MediaRoot::owned(&engine, &endpoint).expect("running launch retains output A");
    assert_eq!(proof.path, output_a);
    let jobs = completed(&endpoint);
    {
        let mut session = jobs.lock().unwrap();
        session.job.as_mut().unwrap().plan.media_root = Some(proof);
        session.retain_completed();
    }
    std::fs::create_dir_all(output_a.join("DEAS/run-1")).unwrap();
    std::fs::write(output_a.join("DEAS/run-1/image.png"), png_bytes(1, 1)).unwrap();
    jobs.media_move_ready(&output_a, &output_a.join("DEAS/run-1/image.png"))
        .unwrap();
    assert!(
        jobs.media_move_ready(&output_b, &output_b.join("DEAS/run-1/image.png"))
            .is_err()
    );
    save_output(&output_a);
    assert_eq!(MediaRoot::owned(&engine, &endpoint).unwrap().path, output_a);
    assert!(MediaRoot::owned(&engine, "http://127.0.0.1:9/").is_none());
    let other = crate::install::Engine::default();
    assert!(MediaRoot::owned(&other, &endpoint).is_none());
    engine.backend.stop(); // Only the scratch fixture, never the user's backend.
    assert!(MediaRoot::owned(&engine, &endpoint).is_none());
    save_output(&output_b);
    engine.backend.start(approved).unwrap();
    let replacement = await_state(&engine.backend, "running");
    let replacement_endpoint = replacement.endpoint.clone().unwrap();
    assert_eq!(
        MediaRoot::owned(&engine, replacement.endpoint.as_deref().unwrap())
            .unwrap()
            .path,
        output_b
    );
    assert_eq!(
        serde_json::to_value(replacement).unwrap(),
        json!({"state":"running", "endpoint":replacement_endpoint,
            "error":null, "generationReady":false}),
        "launch paths must not enter the status wire format"
    );
}

#[test]
fn relocation_repairs_output_handles_without_rewriting_intent_or_media_bytes() {
    let jobs = completed("http://127.0.0.1:7801/");
    let dir = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(dir.path()).unwrap();
    register_completed(&jobs, &root);
    std::fs::create_dir_all(root.join("DEAS/run-1")).unwrap();
    std::fs::create_dir_all(root.join("starred/DEAS/run-1")).unwrap();
    let source = root.join("DEAS/run-1/image.png");
    let target = root.join("starred/DEAS/run-1/image.png");
    std::fs::write(&source, png_bytes(1, 1)).unwrap();
    let intent = jobs
        .lock()
        .unwrap()
        .job
        .as_ref()
        .unwrap()
        .plan
        .intent
        .clone();
    let moved = jobs
        .relocate_media(&root, &source, || {
            std::fs::rename(&source, &target).unwrap();
            Ok(target.clone())
        })
        .unwrap();
    assert_eq!(moved, target);
    assert!(!source.exists());
    assert_eq!(std::fs::read(&target).unwrap(), png_bytes(1, 1));
    let session = jobs.lock().unwrap();
    let job = session.job.as_ref().unwrap();
    assert_eq!(job.plan.intent, intent);
    assert_eq!(job.outputs[0]["subfolder"], "starred/DEAS/run-1");
    assert_eq!(job.outputs[0]["filename"], "image.png");
}
#[test]
fn relocation_blocks_incomplete_jobs_before_touching_the_file() {
    let jobs = completed("http://127.0.0.1:7801/");
    jobs.lock().unwrap().job.as_mut().unwrap().state = "running";
    assert!(jobs.media_move_idle().is_err());
    let dir = tempfile::tempdir().unwrap();
    assert!(
        Jobs::default()
            .media_move_ready(dir.path(), &dir.path().join("a.png"))
            .unwrap_err()
            .contains("missing or unreadable")
    );
    assert!(
        jobs.relocate_media(
            std::path::Path::new("/output"),
            std::path::Path::new("/output/a.png"),
            || panic!("must not move running output")
        )
        .is_err()
    );
}

async fn complete_via_status(jobs: &Jobs, root: &std::path::Path, id: &str, video: bool) {
    let mut job = jobs.lock().unwrap().job.as_ref().unwrap().clone();
    job.id = id.into();
    job.state = "queued";
    job.prompt_id = Some(id.into());
    job.outputs.clear();
    job.plan.media_root = Some(MediaRoot::capture(root.into()).unwrap());
    let output = json!({"filename": if video {"clip.webm"} else {"image.png"},
        "subfolder":"DEAS/run-1","type":"output"});
    let products = if video {
        json!({"gifs":[output]})
    } else {
        json!({"images":[output]})
    };
    let history = json!({id:{
        "prompt":[1,id,job.plan.graph,{"client_id":job.client_id},["7"]],
        "status":{"completed":true,"status_str":"success","messages":[]},
        "outputs":{"7":products}
    }});
    let (base, server) = binary_server(
        history.to_string().into_bytes(),
        200,
        "Content-Type: application/json",
    )
    .await;
    job.plan.base = reqwest::Url::parse(&base).unwrap();
    jobs.lock().unwrap().job = Some(job);
    assert_eq!(jobs.status(id.into()).await.unwrap()["state"], "succeeded");
    server.await.unwrap();
}

#[tokio::test]
async fn whole_stop_retains_terminal_cache_completed_records_and_diagnostic_slot() {
    let dir = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(dir.path()).unwrap();
    let held = Arc::new(completed("http://127.0.0.1:9/"));
    complete_via_status(&held, &root, "terminal", false).await;
    let cached = Arc::new(CachedOutput {
        job_id: "terminal".into(),
        index: 0,
        mime_type: "image/png",
        width: 1,
        height: 1,
        sha256: format!("{:x}", Sha256::digest([42])),
        bytes: vec![42],
    });
    held.lock().unwrap().output = Some(cached.clone());
    let before = held.current().unwrap();
    let jobs = completed("http://127.0.0.1:9/");
    let diagnostic = jobs.current().unwrap();
    jobs.lock().unwrap().workbench.push(held.clone());
    let result = jobs
        .workbench_stop(
            true,
            Arc::new(|_| panic!("terminal ownership must not be probed")),
            Arc::new(|_| panic!("terminal must not be interrupted")),
        )
        .await
        .unwrap();
    assert_eq!(result["outcome"], "settled");
    assert_eq!(result["jobs"][0], before);
    assert_eq!(jobs.current().unwrap(), diagnostic);
    let entry = held.lock().unwrap();
    assert!(Arc::ptr_eq(entry.output.as_ref().unwrap(), &cached));
    assert_eq!(entry.completed.len(), 1);
    assert_eq!(entry.completed[0].job_id, "terminal");
    assert_eq!(
        entry.completed[0].outputs,
        before["outputs"].as_array().unwrap().clone()
    );
}

#[tokio::test]
async fn retains_two_correlated_completions_and_repairs_earlier_output_after_latest_changes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(dir.path()).unwrap();
    let jobs = completed("http://127.0.0.1:9/");
    complete_via_status(&jobs, &root, "first", false).await;
    complete_via_status(&jobs, &root, "second", true).await;
    assert_eq!(jobs.lock().unwrap().completed.len(), 2);
    std::fs::create_dir_all(root.join("DEAS/run-1")).unwrap();
    // Arbitrary byte-preservation fixture, not a decoded video sample.
    std::fs::write(root.join("DEAS/run-1/clip.webm"), b"unparsed video bytes").unwrap();
    std::fs::create_dir(root.join("starred")).unwrap();
    let source = root.join("DEAS/run-1/image.png");
    let target = root.join("starred/image-collision-safe.png");
    std::fs::write(&source, png_bytes(1, 1)).unwrap();
    let cached = Arc::new(CachedOutput {
        job_id: "second".into(),
        index: 0,
        mime_type: "image/png",
        width: 1,
        height: 1,
        sha256: format!("{:x}", Sha256::digest([42])),
        bytes: vec![42],
    });
    jobs.lock().unwrap().output = Some(cached.clone());
    let latest = jobs.current().unwrap();
    jobs.media_move_ready(&root, &root.join("DEAS/run-1/clip.webm"))
        .unwrap();
    assert_eq!(
        jobs.relocate_media(&root, &source, || {
            assert!(
                jobs.0.try_lock().is_err(),
                "move must exclude submission/preparation"
            );
            std::fs::rename(&source, &target).unwrap();
            Ok(target.clone())
        })
        .unwrap(),
        target
    );
    assert!(jobs.media_move_ready(&root, &source).is_err());
    jobs.media_move_ready(&root, &target).unwrap();
    assert_eq!(
        jobs.relocate_media(&root, &target, || Ok(target.clone()))
            .unwrap(),
        target
    );
    assert_eq!(jobs.status("second".into()).await.unwrap(), latest);
    {
        let session = jobs.lock().unwrap();
        assert!(Arc::ptr_eq(session.output.as_ref().unwrap(), &cached));
        assert_eq!(session.output.as_ref().unwrap().bytes, [42]);
        assert_eq!(session.completed[0].outputs[0]["subfolder"], "starred");
        assert_eq!(
            session.completed[0].outputs[0]["filename"],
            "image-collision-safe.png"
        );
        assert_eq!(session.completed[1].outputs[0]["filename"], "clip.webm");
    }
    assert_eq!(std::fs::read(&target).unwrap(), png_bytes(1, 1));
    // Another submission's terminal failure must not erase either completion.
    {
        let mut session = jobs.lock().unwrap();
        let job = session.job.as_mut().unwrap();
        job.id = "third".into();
        job.state = "failed";
        job.outputs.clear();
    }
    jobs.media_move_ready(&root, &target).unwrap();
    jobs.media_move_ready(&root, &root.join("DEAS/run-1/clip.webm"))
        .unwrap();
}

#[tokio::test]
async fn latest_relocation_preserves_original_png_cache_and_executed_settings_on_repeated_status() {
    let dir = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(dir.path()).unwrap();
    let bytes = png_bytes(512, 768);
    let (base, server) = binary_server(bytes.clone(), 200, "Content-Type: image/png").await;
    let jobs = completed(&base);
    register_completed(&jobs, &root);
    jobs.output("job-a".into(), 0).await.unwrap();
    server.await.unwrap();
    let cached = jobs.cached_output("job-a", 0).unwrap();
    let before = jobs.current().unwrap();
    let graph = jobs
        .lock()
        .unwrap()
        .job
        .as_ref()
        .unwrap()
        .plan
        .graph
        .clone();
    std::fs::create_dir_all(root.join("DEAS/run-1")).unwrap();
    std::fs::create_dir(root.join("starred")).unwrap();
    let source = root.join("DEAS/run-1/image.png");
    let target = root.join("starred/image.png");
    std::fs::write(&source, &bytes).unwrap();
    jobs.relocate_media(&root, &source, || {
        std::fs::rename(&source, &target).unwrap();
        Ok(target.clone())
    })
    .unwrap();
    for _ in 0..2 {
        jobs.relocate_media(&root, &target, || Ok(target.clone()))
            .unwrap();
        let status = jobs.status("job-a".into()).await.unwrap();
        assert_eq!(status["intent"], before["intent"]);
        assert_eq!(status["outputs"][0]["subfolder"], "starred");
        assert_eq!(jobs.output("job-a".into(), 0).await.unwrap().bytes, bytes);
        assert!(Arc::ptr_eq(
            &jobs.cached_output("job-a", 0).unwrap(),
            &cached
        ));
    }
    let session = jobs.lock().unwrap();
    assert_eq!(session.job.as_ref().unwrap().plan.graph, graph);
    assert_eq!(
        session.completed[0].outputs,
        session.job.as_ref().unwrap().outputs
    );
    assert_eq!(std::fs::read(target).unwrap(), bytes);
}

#[tokio::test]
async fn workbench_all_job_png_lookup_and_alias_repair() {
    let dir = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(dir.path()).unwrap();
    let jobs = Jobs::default();
    let mut entries = vec![];
    for index in 0..3 {
        let bytes = png_bytes(512, 768);
        let (base, server) = binary_server(bytes.clone(), 200, "Content-Type: image/png").await;
        let held = Arc::new(completed(&base));
        let id = format!("job-{index}");
        held.lock().unwrap().job.as_mut().unwrap().id = id.clone();
        register_completed(&held, &root);
        jobs.lock().unwrap().workbench.push(held.clone());
        entries.push((held, id, bytes, server));
    }
    for (_, id, bytes, server) in &mut entries {
        assert_eq!(jobs.output(id.clone(), 0).await.unwrap().bytes, *bytes);
        (&mut *server).await.unwrap();
    }
    let source = root.join("DEAS/run-1/image.png");
    std::fs::create_dir(root.join("starred")).unwrap();
    let target = root.join("starred/image.png");
    jobs.relocate_media(&root, &source, || {
        std::fs::rename(&source, &target).unwrap();
        Ok(target.clone())
    })
    .unwrap();
    for (held, id, bytes, _) in entries {
        assert_eq!(
            held.current().unwrap()["outputs"][0]["subfolder"],
            "starred"
        );
        assert_eq!(
            held.lock().unwrap().completed[0].outputs[0]["subfolder"],
            "starred"
        );
        assert_eq!(jobs.output(id, 0).await.unwrap().bytes, bytes);
    }
}

#[test]
fn workbench_nonterminal_jobs_block_media_moves() {
    let jobs = Jobs::default();
    let held = Arc::new(completed("http://127.0.0.1:9/"));
    jobs.lock().unwrap().workbench.push(held.clone());
    assert!(jobs.media_move_idle().is_ok());
    for state in ["queued", "running", "unknown"] {
        held.lock().unwrap().job.as_mut().unwrap().state = state;
        assert!(jobs.media_move_idle().is_err(), "{state}");
    }
    held.lock().unwrap().job = None;
    assert!(jobs.media_move_idle().is_err(), "preparation reservation");
}

#[test]
fn native_move_rejects_missing_active_uncertain_and_unsafe_roots() {
    let dir = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(dir.path()).unwrap();
    let jobs = completed("http://127.0.0.1:9/");
    let source = root.join("DEAS/run-1/image.png");
    assert!(
        jobs.media_move_ready(&root, &source).is_err(),
        "unbound endpoint"
    );
    assert!(MediaRoot::owned(&crate::install::Engine::default(), "http://127.0.0.1:9/").is_none());
    register_completed(&jobs, &root);
    for state in ["preparing", "queued", "running", "unknown"] {
        jobs.lock().unwrap().job.as_mut().unwrap().state = state;
        assert!(jobs.media_move_ready(&root, &source).is_err());
        assert!(
            jobs.relocate_media(&root, &source, || panic!("active move"))
                .is_err()
        );
    }
    jobs.lock().unwrap().job.as_mut().unwrap().state = "succeeded";
    jobs.lock().unwrap().preparing = Some("next plan".into());
    assert!(
        jobs.relocate_media(&root, &source, || panic!("preparing move"))
            .is_err()
    );
    jobs.lock().unwrap().preparing = None;
    for unknown in [
        root.join("unregistered.png"),
        root.join("DEAS/run-1/unknown.png"),
    ] {
        assert!(
            jobs.relocate_media(&root, &unknown, || panic!("unregistered move"))
                .is_err()
        );
    }
    let other = tempfile::tempdir().unwrap();
    assert!(
        jobs.media_move_ready(other.path(), &other.path().join("DEAS/run-1/image.png"))
            .is_err()
    );
    let old = root.with_extension("old");
    std::fs::rename(&root, &old).unwrap();
    std::fs::create_dir(&root).unwrap();
    assert!(
        jobs.relocate_media(&root, &source, || panic!("replaced root"))
            .is_err()
    );
    std::fs::remove_dir(&root).unwrap();
    std::fs::rename(&old, &root).unwrap();
}

#[test]
fn root_bound_alias_repair_uses_nested_parent_and_failed_move_preserves_every_record() {
    let dir = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(dir.path()).unwrap();
    let other = tempfile::tempdir().unwrap();
    let jobs = completed("http://127.0.0.1:9/");
    register_completed(&jobs, &root);
    let source = root.join("DEAS/run-1/image.png");
    {
        let mut session = jobs.lock().unwrap();
        let matching = session.completed[0].outputs.clone();
        let mut input = matching[0].clone();
        input["type"] = json!("input");
        session.completed[0].outputs.push(input);
        session.completed.push_back(CompletedOutputs {
            job_id: "foreign-root".into(),
            root: MediaRoot::capture(other.path().into()).unwrap(),
            outputs: matching.clone(),
        });
        session.completed.push_back(CompletedOutputs {
            job_id: "replaced-root".into(),
            root: MediaRoot {
                path: root.clone(),
                identity: (0, 0),
            },
            outputs: matching,
        });
    }
    let before: Vec<_> = jobs
        .lock()
        .unwrap()
        .completed
        .iter()
        .map(|record| record.outputs.clone())
        .collect();
    std::fs::write(root.join("starred"), b"blocked destination").unwrap();
    assert!(
        jobs.relocate_media(&root, &source, || crate::media::star::move_media(
            &root, &source
        ))
        .is_err()
    );
    assert_eq!(std::fs::read(&source).unwrap(), png_bytes(1, 1));
    assert_eq!(
        jobs.lock()
            .unwrap()
            .completed
            .iter()
            .map(|record| record.outputs.clone())
            .collect::<Vec<_>>(),
        before
    );
    assert_eq!(
        jobs.lock().unwrap().job.as_ref().unwrap().outputs[0],
        before[0][0]
    );
    std::fs::remove_file(root.join("starred")).unwrap();
    #[cfg(target_os = "linux")]
    for (after_commit, replace_source) in [(false, false), (true, false), (true, true)] {
        let outside = tempfile::tempdir().unwrap();
        let displaced = outside.path().join("renamed-destination");
        let error = jobs
            .relocate_media(&root, &source, || {
                crate::media::star::move_media_checked(&root, &source, |committed| {
                    if committed == after_commit {
                        let destination = root.join("starred/DEAS/run-1");
                        std::fs::rename(&destination, &displaced).unwrap();
                        std::fs::create_dir(&destination).unwrap();
                        if replace_source {
                            std::fs::write(&source, b"concurrent replacement").unwrap();
                        }
                    }
                })
            })
            .unwrap_err();
        if replace_source {
            assert!(error.contains("could not be confirmed restored"), "{error}");
            assert_eq!(std::fs::read(&source).unwrap(), b"concurrent replacement");
            assert_eq!(
                std::fs::read(displaced.join("image.png")).unwrap(),
                png_bytes(1, 1)
            );
            std::fs::remove_file(&source).unwrap();
            std::fs::rename(displaced.join("image.png"), &source).unwrap();
        } else {
            if after_commit {
                assert!(error.contains("rolled back to source"), "{error}");
            }
            assert_eq!(std::fs::read(&source).unwrap(), png_bytes(1, 1));
            assert!(!displaced.join("image.png").exists());
        }
        let session = jobs.lock().unwrap();
        assert_eq!(
            session
                .completed
                .iter()
                .map(|record| record.outputs.clone())
                .collect::<Vec<_>>(),
            before
        );
        assert_eq!(session.job.as_ref().unwrap().outputs[0], before[0][0]);
    }
    let target = jobs
        .relocate_media(&root, &source, || {
            crate::media::star::move_media(&root, &source)
        })
        .unwrap();
    assert_eq!(target, root.join("starred/DEAS/run-1/image.png"));
    let session = jobs.lock().unwrap();
    assert_eq!(
        session.job.as_ref().unwrap().outputs[0]["subfolder"],
        "starred/DEAS/run-1"
    );
    assert_eq!(
        session.completed[0].outputs[0]["subfolder"],
        "starred/DEAS/run-1"
    );
    assert_eq!(session.completed[0].outputs[1], before[0][1]);
    assert_eq!(session.completed[1].outputs, before[1]);
    assert_eq!(session.completed[2].outputs, before[2]);
}
#[test]
fn unstar_repairs_root_bound_aliases_without_changing_submitted_intent() {
    let dir = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(dir.path()).unwrap();
    let jobs = completed("http://127.0.0.1:9/");
    register_completed(&jobs, &root);
    let intent = jobs.current().unwrap()["intent"].clone();
    let source = root.join("DEAS/run-1/image.png");
    let target = jobs
        .relocate_media(&root, &source, || {
            crate::media::star::move_media(&root, &source)
        })
        .unwrap();
    let returned = root.join("raw/DEAS/run-1/image.png");
    std::fs::create_dir_all(returned.parent().unwrap()).unwrap();
    std::fs::write(&returned, b"occupied").unwrap();
    let before = jobs.current().unwrap();
    assert!(
        jobs.relocate_media(&root, &target, || crate::media::star::unstar_media(
            &root, &target
        ))
        .is_err()
    );
    assert_eq!(jobs.current().unwrap(), before);
    assert_eq!(std::fs::read(&returned).unwrap(), b"occupied");
    std::fs::remove_file(&returned).unwrap();
    assert_eq!(
        jobs.relocate_media(&root, &target, || crate::media::star::unstar_media(
            &root, &target
        ))
        .unwrap(),
        returned
    );
    let session = jobs.lock().unwrap();
    assert_eq!(
        session.job.as_ref().unwrap().outputs[0]["subfolder"],
        "raw/DEAS/run-1"
    );
    assert_eq!(
        session.completed[0].outputs[0]["subfolder"],
        "raw/DEAS/run-1"
    );
    assert_eq!(session.completed[0].outputs[0]["filename"], "image.png");
    drop(session);
    assert_eq!(jobs.current().unwrap()["intent"], intent);
    assert_eq!(std::fs::read(&returned).unwrap(), png_bytes(1, 1));
    assert!(!target.exists());
}
#[test]
fn completion_retention_is_bounded_deduplicated_and_failed_moves_preserve_records() {
    let dir = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(dir.path()).unwrap();
    let jobs = completed("http://127.0.0.1:9/");
    for index in 0..=COMPLETED_LIMIT {
        {
            let mut session = jobs.lock().unwrap();
            let job = session.job.as_mut().unwrap();
            job.id = format!("job-{index}");
            job.outputs[0]["filename"] = json!(format!("{index}.png"));
        }
        register_completed(&jobs, &root);
        register_completed(&jobs, &root);
    }
    assert_eq!(jobs.lock().unwrap().completed.len(), COMPLETED_LIMIT);
    jobs.media_move_ready(&root, &root.join("DEAS/run-1/0.png"))
        .unwrap();
    jobs.media_move_ready(&root, &root.join("DEAS/run-1/1.png"))
        .unwrap();
    let source = root.join(format!("DEAS/run-1/{COMPLETED_LIMIT}.png"));
    let before = jobs.current().unwrap();
    assert!(
        jobs.relocate_media(&root, &source, || Err("fixture move failed".into()))
            .is_err()
    );
    assert_eq!(jobs.current().unwrap(), before);
    jobs.media_move_ready(&root, &source).unwrap();
    assert_eq!(jobs.lock().unwrap().completed.len(), COMPLETED_LIMIT);
}

async fn binary_server(
    body: Vec<u8>,
    status: u16,
    headers: &str,
) -> (String, tokio::task::JoinHandle<String>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/prefix/comfy/", listener.local_addr().unwrap());
    let headers = headers.to_owned();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = vec![];
        loop {
            let mut buf = [0; 4096];
            let n = socket.read(&mut buf).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
            if bytes.windows(4).any(|b| b == b"\r\n\r\n") {
                break;
            }
        }
        let head = format!("HTTP/1.1 {status} Fixture\r\n{headers}\r\nConnection: close\r\n\r\n");
        let _ = socket.write_all(head.as_bytes()).await;
        let _ = socket.write_all(&body).await;
        String::from_utf8(bytes).unwrap()
    });
    (base, task)
}

#[tokio::test]
async fn music_flac_outputs_are_decoded_before_preview_and_export() {
    let flac = include_bytes!("../../tests/fixtures/music/tone.flac").to_vec();
    let music = crate::jobs_music::tests::full();
    let with_music = |jobs: &Jobs, state: &'static str| {
        let mut session = jobs.lock().unwrap();
        let job = session.job.as_mut().unwrap();
        job.state = state;
        job.plan.graph = compile(&music).unwrap();
        job.plan.intent = music.clone();
        job.outputs = vec![
            json!({"filename":"YuE2_00001.flac","subfolder":"raw","type":"output"}),
            json!({"filename":"YuE2_00002.flac","subfolder":"raw","type":"output"}),
        ];
    };
    for (state, index) in [("succeeded", 0), ("failed", 1), ("cancelled", 1)] {
        let (base, server) = binary_server(flac.clone(), 200, "Content-Type: audio/x-flac").await;
        let jobs = completed(&base);
        with_music(&jobs, state);
        let result = jobs.output("job-a".into(), index).await.unwrap();
        assert_eq!(
            (result.mime_type, result.width, result.height, result.index),
            ("audio/flac", 0, 0, index)
        );
        assert_eq!(result.bytes, flac);
        assert_eq!(result.sha256, format!("{:x}", Sha256::digest(&flac)));
        server.await.unwrap();
        #[cfg(any(target_os = "linux", windows))]
        {
            let dir = tempfile::tempdir_in(std::env::temp_dir()).unwrap();
            assert!(
                jobs.export_to("job-a", index, Some(&dir.path().join("song.png")))
                    .is_err()
            );
            let path = dir.path().join("song ü.flac");
            let exported = jobs
                .export_to("job-a", index, Some(&path))
                .unwrap()
                .unwrap();
            assert_eq!(exported["sha256"], result.sha256);
            assert_eq!(std::fs::read(&path).unwrap(), flac);
            assert!(
                jobs.export_to("job-a", index, Some(&path)).is_err(),
                "no clobber"
            );
            assert_eq!(std::fs::read(&path).unwrap(), flac);
            assert!(
                jobs.export_to("job-a", index, None).unwrap().is_none(),
                "cancelled chooser"
            );
        }
    }
    // Rejected: truncated/corrupt streams, forged signatures, wrong content type, oversize and image jobs.
    let mut truncated = flac.clone();
    truncated.truncate(flac.len() - 200);
    // ffmpeg-written FLAC carries a large PADDING block; flip inside the last audio frame so the CRC-16 must catch it.
    let mut corrupt = flac.clone();
    corrupt[flac.len() - 50] ^= 0xff;
    let mut trailing = flac.clone();
    trailing.extend_from_slice(b"fLaC junk");
    for (bytes, header) in [
        (truncated, "Content-Type: audio/flac"),
        (corrupt, "Content-Type: audio/flac"),
        (trailing, "Content-Type: audio/flac"),
        (b"fLaC\0\0\0\0".to_vec(), "Content-Type: audio/flac"),
        (png_bytes(1, 1), "Content-Type: audio/flac"),
        (flac.clone(), "Content-Type: image/png"),
        (flac.clone(), "Content-Type: text/html"),
    ] {
        let (base, server) = binary_server(bytes, 200, header).await;
        let jobs = completed(&base);
        with_music(&jobs, "succeeded");
        assert!(jobs.output("job-a".into(), 0).await.is_err(), "{header}");
        assert!(jobs.lock().unwrap().output.is_none());
        server.await.unwrap();
    }
    for (bytes, header) in [
        (flac.clone(), "Content-Type: audio/flac"),
        (flac.clone(), "Content-Type: image/png"),
    ] {
        let (base, server) = binary_server(bytes, 200, header).await;
        let jobs = completed(&base);
        assert!(
            jobs.output("job-a".into(), 0).await.is_err(),
            "image job never accepts FLAC"
        );
        server.await.unwrap();
    }
    assert!(validate_flac(&flac).is_ok());
    assert!(validate_flac(b"").is_err());
}

#[tokio::test]
async fn paired_png_reads_use_role_dimensions_and_allow_terminal_partial_exports() {
    for (state, role, width, height) in [
        ("succeeded", "processed", 1024, 1536),
        ("failed", "original", 512, 768),
        ("cancelled", "original", 512, 768),
    ] {
        let bytes = png_bytes(width, height);
        let (base, server) = binary_server(bytes.clone(), 200, "Content-Type: image/png").await;
        let jobs = completed(&base);
        {
            let mut session = jobs.lock().unwrap();
            let job = session.job.as_mut().unwrap();
            job.state = state;
            job.plan.intent["refine"] = json!({"width":1024,"height":1536,"denoise":0.3});
            job.plan.graph = compile(&job.plan.intent).unwrap();
            job.outputs[0]["role"] = json!(role);
        }
        assert!(jobs.output("other".into(), 0).await.is_err());
        assert!(jobs.output("job-a".into(), 1).await.is_err());
        let result = jobs.output("job-a".into(), 0).await.unwrap();
        assert_eq!((result.width, result.height), (width, height));
        assert_eq!(result.bytes, bytes);
        server.await.unwrap();
        #[cfg(any(target_os = "linux", windows))]
        {
            let dir = tempfile::tempdir_in(std::env::temp_dir()).unwrap();
            let path = dir.path().join("partial.png");
            jobs.export_to("job-a", 0, Some(&path)).unwrap().unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        jobs.lock().unwrap().job.as_mut().unwrap().outputs.clear();
        assert!(jobs.output("job-a".into(), 0).await.is_err());
        assert!(jobs.export_to("job-a", 0, None).is_err());
    }
}

#[tokio::test]
async fn paired_video_png_frames_use_latent_geometry_and_original_source_bounds() {
    for mut intent in [
        crate::jobs_video::tests::ltx(),
        crate::jobs_video::tests::h3(),
    ] {
        intent["width"] = json!(320);
        intent["height"] = json!(352);
        intent["latentUpscale"] = if intent["family"] == "ltx-2" {
            json!({"model":"ltx-spatial-upscaler.safetensors","scale":2})
        } else {
            json!({"model":"minimax_h3_latent_upscaler.safetensors","scale":1.5,"steps":2,"denoise":0.2})
        };
        intent["output"]["saveFirstFrame"] = json!(true);
        intent["output"]["saveLastFrame"] = json!(true);
        let graph = compile(&intent).unwrap();
        assert!(intent["refine"].is_null());
        let processed = crate::jobs_video::latent_upscale_size(&intent);
        let source = (320, 352);
        for role in ["processed", "original"] {
            for frame in ["first", "last"] {
                for correct in [true, false] {
                    let expected = if role == "processed" {
                        processed
                    } else {
                        source
                    };
                    let wrong = if role == "processed" {
                        source
                    } else {
                        processed
                    };
                    let (w, h) = if correct { expected } else { wrong };
                    let bytes = png_bytes(w as u32, h as u32);
                    let (base, server) =
                        binary_server(bytes.clone(), 200, "Content-Type: image/png").await;
                    let jobs = completed(&base);
                    {
                        let mut session = jobs.lock().unwrap();
                        let job = session.job.as_mut().unwrap();
                        job.plan.intent = intent.clone();
                        job.plan.graph = graph.clone();
                        job.outputs = vec![
                            json!({"filename":format!("{role}_{frame}.png"),"subfolder":"video","type":"output","role":role}),
                        ];
                    }
                    let result = jobs.output("job-a".into(), 0).await;
                    assert_eq!(
                        result.is_ok(),
                        correct,
                        "{}/{role}/{frame}/{w}x{h}: {:?}",
                        intent["family"],
                        result.err()
                    );
                    assert_eq!(jobs.lock().unwrap().output.is_some(), correct);
                    server.await.unwrap();
                }
            }
        }
    }
}

#[tokio::test]
async fn refuses_non_png_transport_and_invalid_complete_images_without_caching() {
    let good = png_bytes(512, 768);
    let mut crc = good.clone();
    crc[29] ^= 1;
    let mut trailing = good.clone();
    trailing.extend_from_slice(b"<script>unsafe</script>");
    for (name, body, status, headers) in [
        (
            "mime",
            good.clone(),
            200,
            "Content-Type: image/svg+xml".to_owned(),
        ),
        ("missing_mime", good.clone(), 200, String::new()),
        (
            "redirect",
            good.clone(),
            302,
            "Content-Type: image/png\r\nLocation: http://127.0.0.1:9/forbidden".into(),
        ),
        (
            "http_error",
            good.clone(),
            500,
            "Content-Type: image/png".into(),
        ),
        (
            "html",
            b"<html>error</html>".to_vec(),
            200,
            "Content-Type: image/png".into(),
        ),
        (
            "svg",
            b"<svg/>".to_vec(),
            200,
            "Content-Type: image/png".into(),
        ),
        (
            "truncated",
            good[..good.len() - 1].to_vec(),
            200,
            "Content-Type: image/png".into(),
        ),
        ("crc", crc, 200, "Content-Type: image/png".into()),
        (
            "dimensions",
            png_bytes(64, 64),
            200,
            "Content-Type: image/png".into(),
        ),
        ("trailing", trailing, 200, "Content-Type: image/png".into()),
        (
            "advertised_size",
            good,
            200,
            format!(
                "Content-Type: image/png\r\nContent-Length: {}",
                PNG_LIMIT + 1
            ),
        ),
        (
            "actual_size",
            vec![0; PNG_LIMIT + 1],
            200,
            "Content-Type: image/png".into(),
        ),
        (
            "chunked_size",
            format!(
                "{:x}\r\n{}\r\n0\r\n\r\n",
                PNG_LIMIT + 1,
                "x".repeat(PNG_LIMIT + 1)
            )
            .into_bytes(),
            200,
            "Content-Type: image/png\r\nTransfer-Encoding: chunked".into(),
        ),
    ] {
        let (base, server) = binary_server(body, status, &headers).await;
        let jobs = completed(&base);
        assert!(
            jobs.output("job-a".into(), 0).await.is_err(),
            "accepted {name}"
        );
        assert!(jobs.lock().unwrap().output.is_none(), "cached {name}");
        server.await.unwrap();
    }
}

#[tokio::test]
async fn rejects_wrong_job_state_index_and_metadata_before_network() {
    let jobs = completed("http://127.0.0.1:9/");
    for state in ["queued", "running", "unknown"] {
        jobs.lock().unwrap().job.as_mut().unwrap().state = state;
        let error = jobs.output("job-a".into(), 0).await.err().unwrap();
        assert!(error.contains("completed"), "{error}");
    }
    jobs.lock().unwrap().job.as_mut().unwrap().state = "succeeded";
    assert!(
        jobs.output("other".into(), 0)
            .await
            .err()
            .unwrap()
            .contains("Unknown")
    );
    assert!(
        jobs.output("job-a".into(), 1)
            .await
            .err()
            .unwrap()
            .contains("index")
    );
    jobs.lock().unwrap().job.as_mut().unwrap().outputs[0]["filename"] = json!("../secret.png");
    assert!(
        jobs.output("job-a".into(), 0)
            .await
            .err()
            .unwrap()
            .contains("Unsafe")
    );
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn exports_only_cached_original_to_explicit_new_private_png() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    let dir = tempfile::tempdir_in(std::env::temp_dir()).unwrap();
    let bytes = png_bytes(512, 768);
    let (base, server) = binary_server(bytes.clone(), 200, "Content-Type: image/png").await;
    let jobs = completed(&base);
    let path = dir.path().join("new.png");
    assert!(jobs.export_to("job-a", 0, Some(&path)).is_err());
    assert!(!path.exists());
    jobs.output("job-a".into(), 0).await.unwrap();
    server.await.unwrap(); // Export must work with its HTTP server gone.
    assert_eq!(jobs.export_to("job-a", 0, None).unwrap(), None);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    let result = jobs.export_to("job-a", 0, Some(&path)).unwrap().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(result["size"], bytes.len());
    assert_eq!(result["sha256"], format!("{:x}", Sha256::digest(&bytes)));
    assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    for bad in [
        path.clone(),
        dir.path().join("bad.jpg"),
        std::path::PathBuf::from("relative.png"),
        dir.path().join("missing/new.png"),
    ] {
        assert!(jobs.export_to("job-a", 0, Some(&bad)).is_err());
    }
    let link = dir.path().join("link.png");
    symlink(&path, &link).unwrap();
    assert!(jobs.export_to("job-a", 0, Some(&link)).is_err());
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o640);
    let bad_parent = dir.path().join("writable");
    std::fs::create_dir(&bad_parent).unwrap();
    std::fs::set_permissions(&bad_parent, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(
        jobs.export_to("job-a", 0, Some(&bad_parent.join("x.png")))
            .is_err()
    );
    let parent_link = dir.path().join("linked");
    symlink(dir.path(), &parent_link).unwrap();
    assert!(
        jobs.export_to("job-a", 0, Some(&parent_link.join("x.png")))
            .is_err()
    );
    assert!(
        jobs.export_to("other", 0, Some(&dir.path().join("x.png")))
            .is_err()
    );
    assert!(
        jobs.export_to("job-a", 1, Some(&dir.path().join("x.png")))
            .is_err()
    );
    assert!(!dir.path().join("x.png").exists());
    assert!(!std::fs::read_dir(dir.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".deas-")
    }));
}

#[tokio::test]
async fn rejects_animation_and_crc_errors_in_image_end_and_ignored_metadata() {
    let good = png_bytes(512, 768);
    let mut variants = vec![png_bytes(2056, 64)];
    let mut offset = 8;
    while offset < good.len() {
        let length = u32::from_be_bytes(good[offset..offset + 4].try_into().unwrap()) as usize;
        if [&b"IDAT"[..], &b"IEND"[..]].contains(&&good[offset + 4..offset + 8]) {
            let mut bad = good.clone();
            bad[offset + 8 + length] ^= 1;
            variants.push(bad);
        }
        offset += length + 12;
    }
    let mut metadata = vec![];
    {
        let encoder = png::Encoder::new(&mut metadata, 512, 768);
        let mut writer = encoder.write_header().unwrap();
        writer.write_chunk(png::chunk::tEXt, b"key\0value").unwrap();
        writer.write_image_data(&vec![0; 512 * 768]).unwrap();
    }
    let text = metadata.windows(4).position(|b| b == b"tEXt").unwrap();
    metadata[text + 4 + 9] ^= 1;
    variants.push(metadata);
    let mut animated = vec![];
    {
        let mut encoder = png::Encoder::new(&mut animated, 512, 768);
        encoder.set_animated(1, 0).unwrap();
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&vec![0; 512 * 768])
            .unwrap();
    }
    variants.push(animated);
    for (i, body) in variants.into_iter().enumerate() {
        let (base, server) = binary_server(body, 200, "Content-Type: image/png").await;
        let jobs = completed(&base);
        assert!(
            jobs.output("job-a".into(), 0).await.is_err(),
            "accepted corrupt/animated fixture {i}"
        );
        server.await.unwrap();
    }
}

#[tokio::test]
async fn output_timeout_is_total_and_does_not_cache_failure() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let jobs = completed(&format!("http://{}/", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0; 4096];
        assert!(socket.read(&mut buf).await.unwrap() > 0);
        socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nTransfer-Encoding: chunked\r\n\r\n",
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_secs(12)).await;
    });
    let start = std::time::Instant::now();
    assert!(jobs.output("job-a".into(), 0).await.is_err());
    assert!(start.elapsed() >= Duration::from_secs(9));
    assert!(start.elapsed() < Duration::from_secs(11));
    assert!(jobs.lock().unwrap().output.is_none());
    server.abort();
}

#[tokio::test]
async fn fetch_is_exclusive_and_replaced_jobs_cannot_publish_results() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let jobs = completed(&format!("http://{}/", listener.local_addr().unwrap()));
    let (received, ready) = tokio::sync::oneshot::channel();
    let (release, go) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0; 4096];
        assert!(socket.read(&mut buf).await.unwrap() > 0);
        received.send(()).unwrap();
        go.await.unwrap();
        let body = png_bytes(512, 768);
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await.unwrap();
        socket.write_all(&body).await.unwrap();
    });
    let fetch = jobs.output("job-a".into(), 0);
    let steer = async {
        ready.await.unwrap();
        let error =
            tokio::time::timeout(Duration::from_millis(200), jobs.output("job-a".into(), 0))
                .await
                .expect("duplicate fetch must fail immediately")
                .err()
                .unwrap();
        assert!(error.contains("progress"));
        jobs.lock().unwrap().job.as_mut().unwrap().id = "replacement".into();
        release.send(()).unwrap();
    };
    let (result, ()) = tokio::join!(fetch, steer);
    assert!(result.err().unwrap().contains("changed"));
    assert!(jobs.lock().unwrap().output.is_none());
    server.await.unwrap();
}

#[tokio::test]
async fn output_supports_4k_and_pixel_limit_with_bounded_decoding() {
    for (width, height, valid) in [
        (3840, 2160, true),
        (4096, 4096, true),
        (8192, 2048, true),
        (8200, 64, false),
        (8192, 2056, false),
    ] {
        let body = png_bytes(width, height);
        let (base, server) = binary_server(body, 200, "Content-Type: image/png").await;
        let jobs = completed(&base);
        {
            let mut state = jobs.lock().unwrap();
            let intent = &mut state.job.as_mut().unwrap().plan.intent;
            intent["width"] = json!(width);
            intent["height"] = json!(height);
        }
        assert_eq!(
            jobs.output("job-a".into(), 0).await.is_ok(),
            valid,
            "{width}x{height}"
        );
        assert_eq!(jobs.lock().unwrap().output.is_some(), valid);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn original_png_is_fetched_from_stored_metadata_and_cached() {
    let bytes = png_bytes(512, 768);
    let (base, server) = binary_server(
        bytes.clone(),
        200,
        &format!(
            "Content-Type: image/png; charset=binary\r\nContent-Length: {}",
            bytes.len()
        ),
    )
    .await;
    let jobs = completed(&base);
    let result = jobs.output("job-a".into(), 0).await.unwrap();
    assert_eq!(result.bytes, bytes);
    assert_eq!(result.sha256, format!("{:x}", Sha256::digest(&bytes)));
    assert_eq!((result.width, result.height), (512, 768));
    let request = server.await.unwrap();
    assert_eq!(
        request.lines().next().unwrap(),
        "GET /prefix/comfy/view?filename=image.png&subfolder=DEAS%2Frun-1&type=output HTTP/1.1"
    );
    assert_eq!(jobs.output("job-a".into(), 0).await.unwrap().bytes, bytes);
    assert!(jobs.current().unwrap().get("bytes").is_none());
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(value["mimeType"], "image/png");
    assert_eq!(value["jobId"], "job-a");
}
