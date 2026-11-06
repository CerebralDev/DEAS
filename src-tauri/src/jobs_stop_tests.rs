use super::*;

#[tokio::test]
async fn whole_stop_admission_epoch_snapshot_contract() {
    let jobs = Jobs::default();
    let initial = jobs.workbench_current().unwrap();
    assert_eq!(initial["admissionEpoch"], 0);
    assert_eq!(initial["stopping"], false);
    assert_eq!(initial["stopAvailable"], true);
    assert!(jobs
        .workbench_stop(false, owner(), interrupt_owner())
        .await
        .is_err());
    assert_eq!(jobs.workbench_current().unwrap(), initial);
    for epoch in 1..=2 {
        let result = jobs
            .workbench_stop(true, owner(), interrupt_owner())
            .await
            .unwrap();
        assert_eq!(result["admissionEpoch"], epoch);
        assert_eq!(result["stopping"], false);
        assert_eq!(result["outcome"], "empty");
        assert_eq!(
            jobs.workbench_poll().await.unwrap()["admissionEpoch"],
            epoch
        );
    }
}

#[tokio::test]
async fn whole_stop_fences_run_first_polled_after_stop_completion_then_accepts_current_epoch() {
    let requests = Arc::new(AtomicUsize::new(0));
    let posts = Arc::new(AtomicUsize::new(0));
    let (reads, submissions) = (requests.clone(), posts.clone());
    let (base, task) = server(move |path, _| {
        reads.fetch_add(1, Ordering::SeqCst);
        match path.as_str() {
            "GET /comfy/object_info HTTP/1.1" => (200, nodes().to_string()),
            "POST /comfy/prompt HTTP/1.1" => {
                submissions.fetch_add(1, Ordering::SeqCst);
                (
                    200,
                    json!({"prompt_id":PROMPT,"number":0,"node_errors":{}}).to_string(),
                )
            }
            _ => panic!("unexpected {path}"),
        }
    })
    .await;
    let jobs = Jobs::default();
    let old_epoch = jobs.workbench_current().unwrap()["admissionEpoch"]
        .as_u64()
        .unwrap();
    assert!(jobs
        .workbench_run(base.clone(), intent(), false, old_epoch, None)
        .await
        .is_err());
    assert!(jobs
        .workbench_run(base.clone(), intent(), true, old_epoch + 1, None)
        .await
        .unwrap_err()
        .contains("Stale"));
    assert_eq!(
        jobs.workbench_current().unwrap()["admissionEpoch"],
        old_epoch
    );
    // Construct before Stop but do not poll: models an overtaken, delayed Run IPC.
    let delayed = jobs.workbench_run(base.clone(), intent(), true, old_epoch, None);
    let stopped = jobs
        .workbench_stop(true, owner(), interrupt_owner())
        .await
        .unwrap();
    assert_eq!(stopped["admissionEpoch"], old_epoch + 1);
    assert_eq!(stopped["stopping"], false);
    assert!(delayed.await.unwrap_err().contains("Stale"));
    assert_eq!(
        requests.load(Ordering::SeqCst),
        0,
        "stale Run must not even prepare"
    );
    assert_eq!(posts.load(Ordering::SeqCst), 0);
    let current = jobs.workbench_current().unwrap();
    let epoch = current["admissionEpoch"].as_u64().unwrap();
    let result = jobs
        .workbench_run(base, intent(), true, epoch, None)
        .await
        .unwrap();
    assert_eq!(result["state"], "queued");
    assert_eq!(posts.load(Ordering::SeqCst), 1);
    assert_eq!(jobs.workbench_current().unwrap()["admissionEpoch"], epoch);
    task.abort();
}

#[tokio::test]
async fn whole_stop_admission_epoch_exhaustion_refuses_before_invalidating_plans() {
    let jobs = Jobs::default();
    let held = Arc::new(Jobs::default());
    held.lock().unwrap().preparing = Some("retained-preparation".into());
    jobs.lock().unwrap().workbench.push(held.clone());
    jobs.lock().unwrap().admission_epoch = workbench::MAX_ADMISSION_EPOCH - 1;
    let last = jobs
        .workbench_stop(true, owner(), interrupt_owner())
        .await
        .unwrap();
    assert_eq!(last["admissionEpoch"], workbench::MAX_ADMISSION_EPOCH);
    assert_eq!(last["discarded"], 1);
    let held = Arc::new(Jobs::default());
    held.lock().unwrap().preparing = Some("must-survive-refusal".into());
    jobs.lock().unwrap().workbench.push(held.clone());
    let before = jobs.workbench_current().unwrap();
    assert!(jobs
        .workbench_stop(true, owner(), interrupt_owner())
        .await
        .unwrap_err()
        .contains("exhausted"));
    assert_eq!(jobs.workbench_current().unwrap(), before);
    assert!(!held.lock().unwrap().stop_requested);
    assert_eq!(
        held.lock().unwrap().preparing.as_deref(),
        Some("must-survive-refusal")
    );
    for epoch in [workbench::MAX_ADMISSION_EPOCH + 1, u64::MAX] {
        assert!(jobs
            .workbench_run("http://127.0.0.1:9".into(), intent(), true, epoch, None)
            .await
            .unwrap_err()
            .contains("safe integer"));
        assert_eq!(jobs.workbench_current().unwrap(), before);
    }
}

#[test]
fn workbench_admission_epoch_ipc_integer_decoding_is_strict_and_required() {
    // Tauri's command argument decoder uses serde_json for the required u64 parameter.
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Args {
        admission_epoch: u64,
    }
    for malformed in [
        json!({}),
        json!({"admissionEpoch":null}),
        json!({"admissionEpoch":-1}),
        json!({"admissionEpoch":0.5}),
        json!({"admissionEpoch":0.0}),
        json!({"admissionEpoch":"0"}),
        json!({"admissionEpoch":true}),
        json!({"admissionEpoch":[]}),
        json!({"admissionEpoch":{}}),
    ] {
        assert!(
            serde_json::from_value::<Args>(malformed.clone()).is_err(),
            "{malformed}"
        );
    }
    assert_eq!(
        serde_json::from_value::<Args>(json!({"admissionEpoch":0}))
            .unwrap()
            .admission_epoch,
        0
    );
}

#[tokio::test]
async fn whole_stop_targeted_states_lost_replies_and_repeat_markers() {
    for (mode, code, expected, posts) in [
        ("race-running", 200, "cancelled", 2),
        ("race-before-delete", 200, "cancelled", 1),
        ("interrupt-ok", 200, "cancelled", 1),
        ("interrupt-still-running", 200, "running", 1),
        ("interrupt-partial-interrupted", 200, "cancelled", 1),
        ("completed", 200, "succeeded", 0),
        ("race-completed", 200, "succeeded", 1),
        ("absent", 200, "unknown", 0),
        ("graph", 200, "unknown", 0),
        ("client", 200, "unknown", 0),
        ("id", 200, "unknown", 0),
        ("auth", 401, "unknown", 0),
        ("ok", 0, "unknown", 1),
        ("nonempty", 200, "unknown", 1),
        ("still-pending", 200, "unknown", 1),
        ("interrupt-vanished", 0, "unknown", 1),
    ] {
        let (jobs, id, f, task) = cancel_fixture(mode, code).await;
        let result = jobs
            .workbench_stop(true, owner(), interrupt_owner())
            .await
            .unwrap();
        assert_eq!(result["jobs"][0]["state"], expected, "{mode}: {result}");
        assert_eq!(
            result["outcome"],
            if matches!(expected, "succeeded" | "cancelled") {
                "settled"
            } else {
                "partial"
            },
            "{mode}"
        );
        assert_eq!(f.lock().unwrap().posts, posts, "{mode}");
        jobs.workbench_stop(true, owner(), interrupt_owner())
            .await
            .unwrap();
        assert_eq!(f.lock().unwrap().posts, posts, "repeat {mode}");
        assert_eq!(f.lock().unwrap().submissions, 1);
        let held = jobs.workbench_job(&id).unwrap().unwrap();
        let entry = held.lock().unwrap();
        assert!(!entry.stop_requested);
        if mode == "race-running" {
            assert!(entry.cancel_attempted && entry.interrupt_attempted);
        }
        if mode == "race-before-delete" {
            assert!(!entry.cancel_attempted && entry.interrupt_attempted);
        }
        if mode == "interrupt-partial-interrupted" {
            assert_eq!(result["jobs"][0]["outputs"][0]["filename"], "partial.png");
        }
        task.abort();
    }
}

#[tokio::test]
async fn whole_stop_unknowns_only_reconcile_and_missing_ids_never_request() {
    for missing in [false, true] {
        let (jobs, id, f, task) = cancel_fixture("ok", 200).await;
        let held = jobs.workbench_job(&id).unwrap().unwrap();
        {
            let mut entry = held.lock().unwrap();
            let job = entry.job.as_mut().unwrap();
            job.state = "unknown";
            job.outputs = vec![json!({"filename":"retained.png"})];
            if missing {
                job.prompt_id = None;
            }
        }
        let result = jobs
            .workbench_stop(true, owner(), interrupt_owner())
            .await
            .unwrap();
        assert_eq!(result["outcome"], "partial");
        assert_eq!(f.lock().unwrap().posts, 0);
        if missing {
            assert_eq!(f.lock().unwrap().reads, 0);
            assert_eq!(result["jobs"][0]["outputs"][0]["filename"], "retained.png");
            assert_eq!(result["issues"].as_array().unwrap().len(), 1);
        } else {
            assert_eq!(result["jobs"][0]["state"], "queued");
        }
        task.abort();
    }
}

#[tokio::test]
async fn whole_stop_partial_ownership_failure_does_not_skip_eligible_peers_or_diagnostic() {
    let (running, _, rf, rt) = cancel_fixture("interrupt-still-running", 200).await;
    let (pending, _, pf, pt) = cancel_fixture("ok", 200).await;
    let (done, id, df, dt) = cancel_fixture("completed", 200).await;
    done.status_for(id).await.unwrap();
    let terminal = done.lock().unwrap().workbench[0].current().unwrap();
    let jobs = Jobs::default();
    for source in [&running, &pending, &done] {
        jobs.lock()
            .unwrap()
            .workbench
            .extend(source.lock().unwrap().workbench.clone());
    }
    let diagnostic = jobs.prepare("http://127.0.0.1:9".into(), intent()).await;
    assert!(diagnostic.is_err());
    let result = jobs
        .workbench_stop(
            true,
            owner(),
            Arc::new(|_| Err("Source-pinned interruption unsupported".into())),
        )
        .await
        .unwrap();
    assert_eq!(result["outcome"], "partial");
    assert_eq!(result["issues"].as_array().unwrap().len(), 1);
    assert_eq!(result["jobs"][0]["state"], "running");
    assert_eq!(result["jobs"][1]["state"], "cancelled");
    assert_eq!(result["jobs"][2], terminal);
    assert_eq!(rf.lock().unwrap().posts, 0);
    assert_eq!(pf.lock().unwrap().posts, 1);
    assert_eq!(df.lock().unwrap().posts, 0);
    assert!(jobs.lock().unwrap().job.is_none());
    for task in [rt, pt, dt] {
        task.abort();
    }
}

#[tokio::test]
async fn whole_stop_ownership_refusal_and_lifecycle_change_between_paths_never_mutate() {
    for changed in [false, true] {
        let (jobs, _, f, task) = cancel_fixture("race-running", 200).await;
        let owner = if changed {
            owner()
        } else {
            Arc::new(|_: &reqwest::Url| Err("Backend not owned".into()))
                as crate::jobs::cancel::Owner
        };
        let result = jobs
            .workbench_stop(
                true,
                owner,
                Arc::new(|_| Ok((2, "fixture-private-authority".into()))),
            )
            .await
            .unwrap();
        assert_eq!(result["outcome"], "partial");
        assert_eq!(result["issues"].as_array().unwrap().len(), 1);
        assert_eq!(f.lock().unwrap().posts, usize::from(changed));
        assert!(
            !jobs.lock().unwrap().workbench[0]
                .lock()
                .unwrap()
                .interrupt_attempted
        );
        task.abort();
    }
}

#[tokio::test]
async fn whole_stop_unreachable_retains_unknown_products_and_does_not_retry() {
    let (jobs, id, f, task) = cancel_fixture("ok", 200).await;
    task.abort();
    let _ = task.await;
    let held = jobs.workbench_job(&id).unwrap().unwrap();
    held.lock().unwrap().job.as_mut().unwrap().outputs =
        vec![json!({"filename":"partial.png","type":"output","subfolder":""})];
    let result = jobs
        .workbench_stop(true, owner(), interrupt_owner())
        .await
        .unwrap();
    assert_eq!(result["outcome"], "partial");
    assert_eq!(result["jobs"][0]["state"], "unknown");
    assert_eq!(result["jobs"][0]["outputs"][0]["filename"], "partial.png");
    assert_eq!(f.lock().unwrap().posts, 0);
}

fn release(f: &Arc<Mutex<CancelFixture>>) {
    let release = f.lock().unwrap().release.clone();
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
}

async fn stopping(jobs: &Jobs) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while jobs.lock().unwrap().stop.is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn whole_stop_coalesces_survives_waiter_loss_blocks_admission_and_finishes_peers() {
    let (jobs, _, f, task) = cancel_fixture("slow", 200).await;
    let (peer, _, pf, pt) = cancel_fixture("ok", 200).await;
    jobs.lock()
        .unwrap()
        .workbench
        .extend(peer.lock().unwrap().workbench.clone());
    let entered = f.lock().unwrap().entered.clone();
    let clone = jobs.clone();
    let first =
        tokio::spawn(async move { clone.workbench_stop(true, owner(), interrupt_owner()).await });
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    assert!(jobs
        .workbench_run("http://127.0.0.1:9".into(), intent(), true, 0, None)
        .await
        .unwrap_err()
        .contains("Stop"));
    let clone = jobs.clone();
    let second = clone.workbench_stop(true, owner(), interrupt_owner());
    tokio::pin!(second);
    assert!(std::future::Future::poll(
        second.as_mut(),
        &mut std::task::Context::from_waker(std::task::Waker::noop()),
    )
    .is_pending());
    let ongoing = jobs.workbench_current().unwrap();
    assert_eq!(ongoing["admissionEpoch"], 1);
    assert_eq!(ongoing["stopping"], true);
    assert_eq!(jobs.workbench_poll().await.unwrap()["stopping"], true);
    first.abort();
    assert!(first.await.unwrap_err().is_cancelled());
    release(&f);
    let result = second.await.unwrap();
    assert_eq!(
        result["admissionEpoch"], 1,
        "overlapping Stops advance once"
    );
    assert_eq!(result["stopping"], false);
    assert_eq!(result["outcome"], "settled");
    assert_eq!(f.lock().unwrap().posts, 1);
    assert_eq!(pf.lock().unwrap().posts, 1);
    assert!(jobs.lock().unwrap().stop.is_none());
    assert!(jobs
        .workbench_run("http://127.0.0.1:9".into(), intent(), true, 1, None)
        .await
        .is_err());
    assert_eq!(jobs.workbench_current().unwrap()["preparing"], 0);
    task.abort();
    pt.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn whole_stop_finishes_all_targets_and_reopens_gate_with_no_waiters() {
    let (jobs, _, f, task) = cancel_fixture("slow", 200).await;
    let base = jobs.workbench_current().unwrap()["jobs"][0]["baseUrl"]
        .as_str()
        .unwrap()
        .to_owned();
    let delayed = jobs.workbench_run(base, intent(), true, 0, None);
    let (peer, _, pf, pt) = cancel_fixture("ok", 200).await;
    jobs.lock()
        .unwrap()
        .workbench
        .extend(peer.lock().unwrap().workbench.clone());
    let entered = f.lock().unwrap().entered.clone();
    let clone = jobs.clone();
    let waiter =
        tokio::spawn(async move { clone.workbench_stop(true, owner(), interrupt_owner()).await });
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    release(&f);
    tokio::time::timeout(Duration::from_secs(3), async {
        while jobs.lock().unwrap().stop.is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(jobs.workbench_current().unwrap()["admissionEpoch"], 1);
    assert_eq!(jobs.workbench_current().unwrap()["stopping"], false);
    let reads = f.lock().unwrap().reads;
    assert!(delayed.await.unwrap_err().contains("Stale"));
    assert_eq!(f.lock().unwrap().reads, reads);
    assert_eq!(f.lock().unwrap().submissions, 1);
    assert_eq!(
        jobs.workbench_current().unwrap()["jobs"][0]["state"],
        "cancelled"
    );
    assert_eq!(
        jobs.workbench_current().unwrap()["jobs"][1]["state"],
        "cancelled"
    );
    assert_eq!(f.lock().unwrap().posts, 1);
    assert_eq!(pf.lock().unwrap().posts, 1);
    // Both detached Stop targets commit history even after their only Stop waiter disappears.
    let historical = serde_json::to_value(jobs.3.view()).unwrap();
    assert_eq!(historical["records"][0]["state"], "cancelled");
    let peer_history = serde_json::to_value(peer.3.view()).unwrap();
    assert_eq!(peer_history["records"][0]["state"], "cancelled");
    task.abort();
    pt.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn whole_stop_waits_for_poll_or_existing_targeted_cancellation_without_duplicate_mutation() {
    for operation in ["poll", "status", "cancel"] {
        let (jobs, id, f, task) = cancel_fixture("slow", 200).await;
        let entered = f.lock().unwrap().entered.clone();
        let clone = jobs.clone();
        let first = tokio::spawn(async move {
            match operation {
                "poll" => clone.workbench_poll().await,
                "status" => clone.status_for(id).await,
                _ => clone.cancel_pending(id, true, owner()).await,
            }
        });
        tokio::time::timeout(Duration::from_secs(2), entered.notified())
            .await
            .unwrap();
        let clone = jobs.clone();
        let stop =
            tokio::spawn(
                async move { clone.workbench_stop(true, owner(), interrupt_owner()).await },
            );
        stopping(&jobs).await;
        // Polling cannot start another per-job request while Stop waits for the admitted one.
        let reads = f.lock().unwrap().reads;
        jobs.workbench_poll().await.unwrap();
        let id = jobs.workbench_current().unwrap()["jobs"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        jobs.status_for(id.clone()).await.unwrap();
        assert!(jobs
            .cancel_pending(id.clone(), true, owner())
            .await
            .unwrap_err()
            .contains("Stop"));
        assert!(jobs
            .interrupt_running(id, true, interrupt_owner())
            .await
            .unwrap_err()
            .contains("Stop"));
        assert_eq!(f.lock().unwrap().reads, reads);
        release(&f);
        first.await.unwrap().unwrap();
        assert_eq!(
            stop.await.unwrap().unwrap()["outcome"],
            "settled",
            "{operation}"
        );
        assert_eq!(f.lock().unwrap().posts, 1, "{operation}");
        task.abort();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn whole_stop_pins_lifecycle_before_waiting_for_inflight_poll() {
    let (jobs, _, f, task) = cancel_fixture("slow", 200).await;
    let entered = f.lock().unwrap().entered.clone();
    let clone = jobs.clone();
    let poll = tokio::spawn(async move { clone.workbench_poll().await });
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    let epoch = Arc::new(std::sync::atomic::AtomicU64::new(1));
    let captured = Arc::new(tokio::sync::Notify::new());
    let (generation, signal) = (epoch.clone(), captured.clone());
    let clone = jobs.clone();
    let stop = tokio::spawn(async move {
        clone
            .workbench_stop(
                true,
                Arc::new(move |_| {
                    let epoch = generation.load(Ordering::SeqCst);
                    signal.notify_one();
                    Ok(epoch)
                }),
                interrupt_owner(),
            )
            .await
    });
    let capture = tokio::time::timeout(Duration::from_secs(1), captured.notified()).await;
    epoch.store(2, Ordering::SeqCst);
    release(&f);
    poll.await.unwrap().unwrap();
    let result = stop.await.unwrap().unwrap();
    assert!(
        capture.is_ok(),
        "Stop must pin ownership before waiting for the earlier operation"
    );
    assert_eq!(result["outcome"], "partial");
    assert_eq!(f.lock().unwrap().posts, 0);
    task.abort();
}

#[tokio::test]
async fn whole_stop_discards_prepared_snapshot_without_backend_mutation() {
    let posts = Arc::new(AtomicUsize::new(0));
    let requests = posts.clone();
    let (base, task) = server(move |path, _| {
        if path.starts_with("POST") {
            requests.fetch_add(1, Ordering::SeqCst);
        }
        assert_eq!(path, "GET /comfy/object_info HTTP/1.1");
        (200, nodes().to_string())
    })
    .await;
    let jobs = Jobs::default();
    let held = Arc::new(Jobs::default());
    held.prepare(base, intent()).await.unwrap();
    jobs.lock().unwrap().workbench.push(held.clone());
    let result = jobs
        .workbench_stop(true, owner(), interrupt_owner())
        .await
        .unwrap();
    assert_eq!(result["discarded"], 1);
    assert_eq!(result["outcome"], "settled");
    assert_eq!(result["preparing"], 0);
    assert!(result["jobs"].as_array().unwrap().is_empty());
    assert!(held.lock().unwrap().plan.is_none());
    assert_eq!(posts.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn whole_stop_preparation_race_discards_late_plan_without_submission() {
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    let (signal, gate) = (entered.clone(), release.clone());
    let (base, task) = server(move |path, _| {
        assert_eq!(path, "GET /comfy/object_info HTTP/1.1");
        signal.notify_one();
        let _ = gate
            .1
            .wait_timeout_while(gate.0.lock().unwrap(), Duration::from_secs(5), |released| {
                !*released
            })
            .unwrap();
        (200, nodes().to_string())
    })
    .await;
    let jobs = Arc::new(Jobs::default());
    let clone = jobs.clone();
    let run = tokio::spawn(async move { clone.workbench_run(base, intent(), true, 0, None).await });
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    let held = jobs.lock().unwrap().workbench[0].clone();
    let result = jobs
        .workbench_stop(true, owner(), interrupt_owner())
        .await
        .unwrap();
    assert_eq!(result["discarded"], 1);
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    assert!(run.await.unwrap().is_err());
    assert!(held.lock().unwrap().plan.is_none());
    assert!(held.lock().unwrap().job.is_none());
    assert_eq!(jobs.workbench_current().unwrap()["preparing"], 0);
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn whole_stop_submission_race_survives_lost_run_waiter_without_resubmit_or_guessed_id() {
    for lost_reply in [false, true] {
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let (signal, gate) = (entered.clone(), release.clone());
        let posts = Arc::new(AtomicUsize::new(0));
        let submissions = posts.clone();
        let mut tuple = Value::Null;
        let mut deleted = false;
        let (base, task) = server(move |path, body| {
            if path == "GET /comfy/object_info HTTP/1.1" { return (200, nodes().to_string()); }
            if path == "POST /comfy/prompt HTTP/1.1" {
                submissions.fetch_add(1, Ordering::SeqCst);
                tuple = json!([0,PROMPT,body["prompt"],{"client_id":body["client_id"]},["7"]]);
                signal.notify_one();
                let _ = gate.1.wait_timeout_while(gate.0.lock().unwrap(), Duration::from_secs(5), |released| !*released).unwrap();
                return (if lost_reply {0} else {200}, json!({"prompt_id":PROMPT,"number":0,"node_errors":{}}).to_string());
            }
            assert!(!lost_reply, "unknown prompt IDs must not be guessed from the backend queue");
            if path.starts_with("GET /comfy/history/") { return (200,"{}".into()); }
            if path == "GET /comfy/queue HTTP/1.1" { return (200,json!({"queue_running":[],"queue_pending":if deleted { vec![] } else { vec![tuple.clone()] }}).to_string()); }
            assert_eq!(path, "POST /comfy/queue HTTP/1.1");
            assert_eq!(body, json!({"delete":[PROMPT]}));
            assert!(!deleted);
            deleted = true;
            (200,String::new())
        }).await;
        let jobs = Arc::new(Jobs::default());
        let clone = jobs.clone();
        let run =
            tokio::spawn(async move { clone.workbench_run(base, intent(), true, 0, None).await });
        tokio::time::timeout(Duration::from_secs(2), entered.notified())
            .await
            .unwrap();
        run.abort();
        assert!(run.await.unwrap_err().is_cancelled());
        let clone = jobs.clone();
        let stop =
            tokio::spawn(
                async move { clone.workbench_stop(true, owner(), interrupt_owner()).await },
            );
        stopping(&jobs).await;
        assert!(jobs
            .workbench_run("http://127.0.0.1:9".into(), intent(), true, 0, None)
            .await
            .is_err());
        *release.0.lock().unwrap() = true;
        release.1.notify_all();
        let result = stop.await.unwrap().unwrap();
        assert_eq!(
            result["jobs"][0]["state"],
            if lost_reply { "unknown" } else { "cancelled" }
        );
        assert_eq!(
            result["outcome"],
            if lost_reply { "partial" } else { "settled" }
        );
        jobs.workbench_stop(true, owner(), interrupt_owner())
            .await
            .unwrap();
        assert_eq!(posts.load(Ordering::SeqCst), 1);
        assert!(
            !jobs.lock().unwrap().workbench[0]
                .lock()
                .unwrap()
                .job
                .as_ref()
                .unwrap()
                .submitting
        );
        task.abort();
    }
}
