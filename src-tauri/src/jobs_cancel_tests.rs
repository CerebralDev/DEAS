use super::*;
#[path = "jobs_observer_stop_tests.rs"]
mod observer_tests;
#[path = "jobs_stop_tests.rs"]
mod stop_tests;
use std::sync::atomic::{AtomicUsize, Ordering};

const PROMPT: &str = "11111111-1111-4111-8111-111111111111";

struct CancelFixture {
    mode: &'static str,
    interrupt: bool,
    tuple: Value,
    posts: usize,
    reads: usize,
    submissions: usize,
    code: u16,
    entered: Arc<tokio::sync::Notify>,
    release: Arc<(Mutex<bool>, std::sync::Condvar)>,
}

async fn cancel_fixture(
    mode: &'static str,
    code: u16,
) -> (
    Arc<Jobs>,
    String,
    Arc<Mutex<CancelFixture>>,
    tokio::task::JoinHandle<()>,
) {
    let fixture = Arc::new(Mutex::new(CancelFixture {
        mode: mode.strip_prefix("interrupt-").unwrap_or(mode),
        interrupt: mode.starts_with("interrupt-"),
        tuple: Value::Null,
        posts: 0,
        reads: 0,
        submissions: 0,
        code,
        entered: Arc::default(),
        release: Arc::default(),
    }));
    let data = fixture.clone();
    let (base, task) = server_with_headers(move |path, body, headers| {
        let mut f = data.lock().unwrap();
        if path == "GET /comfy/object_info HTTP/1.1" { return (200,nodes().to_string()); }
        if path == "POST /comfy/prompt HTTP/1.1" {
            f.submissions+=1;
            f.tuple=json!([0,PROMPT,body["prompt"],{"client_id":body["client_id"]},["7"]]);
            return (200,json!({"prompt_id":PROMPT,"number":0,"node_errors":{}}).to_string());
        }
        if path.starts_with("GET /comfy/history/") {
            assert_eq!(path,format!("GET /comfy/history/{PROMPT} HTTP/1.1"));
            f.reads+=1;
            if f.mode=="slow" && f.reads==1 {
                let entered=f.entered.clone();
                let release=f.release.clone();
                drop(f);
                entered.notify_one();
                let (lock, condition) = &*release;
                let (_guard, timeout) = condition.wait_timeout_while(lock.lock().unwrap(), Duration::from_secs(5), |released| !*released).unwrap();
                assert!(!timeout.timed_out(), "test did not release HTTP readback");
                f=data.lock().unwrap();
            }
            if f.mode=="auth" { return (f.code,"private-token-response".into()); }
            if f.mode=="foreign-history" {return (200,json!({"foreign":{}}).to_string());}
            if f.interrupt && (matches!(f.mode,"race-running"|"race-before-delete") || f.mode=="interrupted-before" || (f.posts>0 && matches!(f.mode,"ok"|"foreign-interrupted"|"partial-interrupted"))) {
                let id=if f.mode=="foreign-interrupted" {"foreign"} else {PROMPT};
                let outputs=if f.mode=="partial-interrupted" {json!({"7":{"images":[{"filename":"partial.png","subfolder":"","type":"output"}]}})} else {json!({})};
                return (200,json!({PROMPT:{"prompt":f.tuple,"outputs":outputs,"status":{"completed":false,"status_str":"error","messages":[["execution_start",{"prompt_id":PROMPT,"timestamp":1000}],["execution_interrupted",{"prompt_id":id,"timestamp":1500}]]}}}).to_string());
            }
            if f.mode=="completed" || (f.mode=="race-completed" && f.posts>0) || (f.mode=="late-completed" && f.reads>=3) {
                return (200,json!({PROMPT:{"prompt":f.tuple,"outputs":{"7":{"images":[{"filename":"out.png","subfolder":"","type":"output"}]}},"status":{"completed":true,"status_str":"success","messages":[]}}}).to_string());
            }
            return (200,"{}".into());
        }
        if path=="GET /comfy/queue HTTP/1.1" {
            let mut tuple=f.tuple.clone();
            match f.mode {
                "graph" => tuple[2]["3"]["inputs"]["text"]=json!("foreign-private-graph"),
                "client" => tuple[3]["client_id"]=json!("foreign"),
                "id" => tuple[1]=json!("foreign"),
                _=>{},
            }
            let running=f.mode=="running" || (f.mode=="race-before-delete" && f.reads>=2) || (f.mode=="race-running" && f.posts>0) || (f.interrupt && f.mode!="pending" && f.mode!="absent" && (f.posts==0 || f.mode=="still-running"));
            let absent=f.mode=="absent" || (f.posts>0 && f.mode!="still-pending" && !running);
            return (200,json!({"queue_running":if running{vec![tuple.clone()]}else{vec![]},"queue_pending":if !absent && !running {vec![tuple]}else{vec![]}}).to_string());
        }
        if matches!(f.mode,"race-running"|"race-before-delete") && path == "POST /comfy/deas/jobs/interrupt HTTP/1.1" { f.interrupt = true; }
        if f.interrupt {
            assert!(headers.to_ascii_lowercase().contains("x-deas-interrupt-token: fixture-private-authority\r\n"));
            assert!(!headers.to_ascii_lowercase().contains("authorization:"));
            assert_eq!(path,"POST /comfy/deas/jobs/interrupt HTTP/1.1");
            assert_eq!(body,json!({"prompt_id":PROMPT,"client_id":f.tuple[3]["client_id"],"prompt":f.tuple[2]}));
        } else {
            assert_eq!(path,"POST /comfy/queue HTTP/1.1");
            assert_eq!(body,json!({"delete":[PROMPT]}));
        }
        f.posts+=1;
        (f.code,if f.mode=="oversized" {"x".repeat(RESPONSE_LIMIT+1)}else if f.mode=="nonempty" {"<html>not a queue acknowledgement</html>".into()}else{String::new()})
    }).await;
    let jobs = Arc::new(Jobs::default());
    let job = jobs
        .workbench_run(base, intent(), true, 0, None)
        .await
        .unwrap();
    assert_eq!(job["state"], "queued");
    (jobs, job["id"].as_str().unwrap().into(), fixture, task)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn standalone_status_abort_releases_polling_for_retry_and_pending_cancel() {
    let (jobs, id, f, task) = cancel_fixture("slow", 200).await;
    let entered = f.lock().unwrap().entered.clone();
    let polling = jobs.clone();
    let job_id = id.clone();
    let waiter = tokio::spawn(async move { polling.status_for(job_id).await });
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    let held = jobs.workbench_job(&id).unwrap().unwrap();
    assert!(!held.lock().unwrap().job.as_ref().unwrap().polling);
    let release = f.lock().unwrap().release.clone();
    *release.0.lock().unwrap() = true;
    release.1.notify_one();
    assert_eq!(
        jobs.status_for(id.clone()).await.unwrap()["state"],
        "queued"
    );
    assert_eq!(
        jobs.cancel_pending(id, true, owner()).await.unwrap()["state"],
        "cancelled"
    );
    assert_eq!(f.lock().unwrap().posts, 1);
    task.abort();
}

#[tokio::test]
async fn whole_stop_confirmation_empty_pending_and_repeat() {
    let empty = Jobs::default();
    assert!(empty
        .workbench_stop(false, owner(), interrupt_owner())
        .await
        .is_err());
    assert_eq!(
        empty
            .workbench_stop(true, owner(), interrupt_owner())
            .await
            .unwrap()["outcome"],
        "empty"
    );
    let (jobs, id, f, task) = cancel_fixture("ok", 200).await;
    assert!(jobs
        .workbench_stop(false, owner(), interrupt_owner())
        .await
        .is_err());
    assert!(jobs.lock().unwrap().stop.is_none());
    assert_eq!(f.lock().unwrap().posts, 0);
    let stopped = jobs
        .workbench_stop(true, owner(), interrupt_owner())
        .await
        .unwrap();
    let logs: Vec<_> = crate::logs::native_logs(0)
        .into_iter()
        .filter(|entry| entry.text.contains(&format!("Workbench stop: job={id}")))
        .collect();
    assert!(!logs.is_empty());
    assert!(logs
        .iter()
        .all(|entry| entry.source == crate::logs::Source::Deas
            && !entry.text.contains("fixture-private-authority")
            && !entry.text.contains("http://")));
    assert_eq!(stopped["outcome"], "settled");
    assert_eq!(stopped["jobs"][0]["state"], "cancelled");
    assert_eq!(stopped["stopAvailable"], true);
    assert_eq!(
        jobs.workbench_stop(true, owner(), interrupt_owner())
            .await
            .unwrap()["outcome"],
        "settled"
    );
    assert_eq!(f.lock().unwrap().posts, 1);
    task.abort();
}

fn interrupt_owner() -> crate::jobs::cancel::InterruptOwner {
    Arc::new(|_| Ok((1, "fixture-private-authority".into())))
}

#[tokio::test]
async fn running_interrupt_exact_target_terminal_proof_and_readonly_states() {
    for (mode, expected, posts) in [
        ("interrupt-ok", "cancelled", 1),
        ("interrupt-partial-interrupted", "cancelled", 1),
        ("interrupt-still-running", "running", 1),
        ("interrupt-vanished", "unknown", 1),
        ("interrupt-pending", "queued", 0),
        ("interrupt-completed", "succeeded", 0),
        ("interrupt-interrupted-before", "cancelled", 0),
        ("interrupt-race-completed", "succeeded", 1),
        ("interrupt-late-completed", "succeeded", 1),
        ("interrupt-graph", "unknown", 0),
        ("interrupt-client", "unknown", 0),
        ("interrupt-id", "unknown", 0),
        ("interrupt-foreign-history", "unknown", 0),
        ("interrupt-foreign-interrupted", "unknown", 1),
        ("interrupt-absent", "unknown", 0),
    ] {
        let (jobs, id, f, task) = cancel_fixture(mode, 200).await;
        let before = jobs.workbench_current().unwrap()["jobs"][0].clone();
        let root = tempfile::tempdir().unwrap();
        let held = jobs.workbench_job(&id).unwrap().unwrap();
        held.lock().unwrap().job.as_mut().unwrap().plan.media_root = Some(
            job_outputs::MediaRoot::capture(dunce::canonicalize(root.path()).unwrap()).unwrap(),
        );
        let result = jobs
            .interrupt_running(id.clone(), true, interrupt_owner())
            .await
            .unwrap();
        assert_eq!(result["state"], expected, "{mode}: {result}");
        assert_eq!(result["intent"], before["intent"]);
        assert_eq!(
            result["interruptCancellationAttempted"],
            if posts == 1 { json!(true) } else { Value::Null }
        );
        assert!(result["pendingCancellationAttempted"].is_null());
        assert_eq!(f.lock().unwrap().posts, posts, "{mode}");
        if posts == 1 && !matches!(expected, "succeeded" | "cancelled") {
            assert!(jobs
                .interrupt_running(id.clone(), true, interrupt_owner())
                .await
                .is_err());
        }
        if expected == "succeeded" {
            assert_eq!(result["outputs"][0]["filename"], "out.png");
            assert!(!held.lock().unwrap().completed.is_empty());
        }
        if mode == "interrupt-partial-interrupted" {
            assert_eq!(result["outputs"][0]["filename"], "partial.png");
            assert_eq!(
                result["metadata"],
                json!({"startedAt":1000,"generationMs":null})
            );
        }
        if mode == "interrupt-ok" {
            assert_eq!(
                jobs.status_for(id.clone()).await.unwrap()["state"],
                "cancelled"
            );
            let logs: Vec<_> = crate::logs::native_logs(0)
                .into_iter()
                .filter(|e| e.text.contains(&format!("running-interrupt: job={id}")))
                .collect();
            assert_eq!(logs.len(), 1);
            assert_eq!(logs[0].source, crate::logs::Source::Deas);
            assert!(logs[0].text.contains("state=cancelled"));
            assert!(!logs[0].text.contains("fixture-private-authority"));
        }
        assert_eq!(jobs.workbench_current().unwrap()["stopAvailable"], true);
        task.abort();
    }
}

#[tokio::test]
async fn running_interrupt_rejected_lost_invalid_reply_never_retries_and_preserves_products() {
    for (mode, code) in [
        ("interrupt-vanished", 0),
        ("interrupt-vanished", 302),
        ("interrupt-vanished", 401),
        ("interrupt-vanished", 403),
        ("interrupt-vanished", 409),
        ("interrupt-still-running", 401),
        ("interrupt-oversized", 200),
        ("interrupt-nonempty", 200),
    ] {
        let (jobs, id, f, task) = cancel_fixture(mode, code).await;
        let held = jobs.workbench_job(&id).unwrap().unwrap();
        held.lock().unwrap().job.as_mut().unwrap().outputs =
            vec![json!({"filename":"retained.png"})];
        let result = jobs
            .interrupt_running(id.clone(), true, interrupt_owner())
            .await
            .unwrap();
        assert_eq!(
            result["state"],
            if mode == "interrupt-still-running" {
                "running"
            } else {
                "unknown"
            },
            "{mode}/{code}: {result}"
        );
        assert_eq!(result["outputs"][0]["filename"], "retained.png");
        assert_eq!(result["interruptCancellationAttempted"], true);
        let _ = jobs.status_for(id.clone()).await.unwrap();
        assert!(jobs
            .interrupt_running(id, true, interrupt_owner())
            .await
            .is_err());
        assert_eq!(f.lock().unwrap().posts, 1);
        assert!(!result.to_string().contains("fixture-private-authority"));
        task.abort();
    }
}

#[tokio::test]
async fn running_interrupt_requires_authority_confirmation_idle_known_identity_and_lifecycle() {
    let (jobs, id, f, task) = cancel_fixture("interrupt-still-running", 200).await;
    assert!(jobs
        .interrupt_running(id.clone(), false, interrupt_owner())
        .await
        .is_err());
    assert!(jobs
        .interrupt_running(PROMPT.into(), true, interrupt_owner())
        .await
        .is_err());
    assert!(jobs
        .interrupt_running(
            id.clone(),
            true,
            Arc::new(crate::backend::Lifecycle::default().interruption_owner())
        )
        .await
        .is_err());
    let held = jobs.workbench_job(&id).unwrap().unwrap();
    for busy in [true, false] {
        {
            let mut s = held.lock().unwrap();
            let j = s.job.as_mut().unwrap();
            j.submitting = busy;
            j.polling = !busy;
        }
        assert!(jobs
            .interrupt_running(id.clone(), true, interrupt_owner())
            .await
            .is_err());
    }
    {
        let mut s = held.lock().unwrap();
        let j = s.job.as_mut().unwrap();
        j.polling = false;
        j.prompt_id = None;
    }
    assert_eq!(
        jobs.interrupt_running(id.clone(), true, interrupt_owner())
            .await
            .unwrap()["state"],
        "queued"
    );
    {
        let mut s = held.lock().unwrap();
        let j = s.job.as_mut().unwrap();
        j.prompt_id = Some(PROMPT.into());
        j.state = "unknown";
    }
    assert_eq!(
        jobs.interrupt_running(id, true, interrupt_owner())
            .await
            .unwrap()["state"],
        "running"
    );
    assert_eq!(f.lock().unwrap().posts, 0);
    task.abort();
    for change_at in [3, 4, 5] {
        let (jobs, id, f, task) = cancel_fixture("interrupt-ok", 200).await;
        let calls = AtomicUsize::new(0);
        let result = jobs
            .interrupt_running(
                id,
                true,
                Arc::new(move |_| {
                    let n = calls.fetch_add(1, Ordering::SeqCst);
                    Ok((
                        if n >= change_at { 2 } else { 1 },
                        "fixture-private-authority".into(),
                    ))
                }),
            )
            .await
            .unwrap();
        assert_eq!(result["state"], "unknown");
        assert_eq!(f.lock().unwrap().posts, usize::from(change_at > 3));
        task.abort();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn running_interrupt_dropped_waiter_keeps_busy_guard_until_owned_task_finishes() {
    let (jobs, id, f, task) = cancel_fixture("interrupt-slow", 200).await;
    let entered = f.lock().unwrap().entered.clone();
    let caller = jobs.clone();
    let job_id = id.clone();
    let waiter = tokio::spawn(async move {
        caller
            .interrupt_running(job_id, true, interrupt_owner())
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    assert!(jobs
        .interrupt_running(id.clone(), true, interrupt_owner())
        .await
        .is_err());
    assert_eq!(
        jobs.status_for(id.clone()).await.unwrap()["state"],
        "queued"
    );
    let release = f.lock().unwrap().release.clone();
    *release.0.lock().unwrap() = true;
    release.1.notify_one();
    let held = jobs.workbench_job(&id).unwrap().unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if !held.lock().unwrap().job.as_ref().unwrap().polling {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        held.current().unwrap()["interruptCancellationAttempted"],
        true
    );
    assert!(jobs
        .interrupt_running(id, true, interrupt_owner())
        .await
        .is_err());
    assert_eq!(f.lock().unwrap().posts, 1);
    task.abort();
}

#[tokio::test]
async fn regular_status_distinguishes_correlated_terminal_interruption_from_failure() {
    for (mode, expected) in [
        ("interrupt-interrupted-before", "cancelled"),
        ("interrupt-foreign-interrupted", "unknown"),
    ] {
        let (jobs, id, f, task) = cancel_fixture(mode, 200).await;
        if mode == "interrupt-foreign-interrupted" {
            f.lock().unwrap().posts = 1;
        }
        assert_eq!(jobs.status_for(id).await.unwrap()["state"], expected);
        task.abort();
    }
}

fn owner() -> crate::jobs::cancel::Owner {
    Arc::new(|_| Ok(1))
}

#[tokio::test]
async fn pending_cancel_deletes_exact_prompt_and_is_terminal() {
    let (jobs, id, f, task) = cancel_fixture("pending", 200).await;
    let before = jobs.workbench_current().unwrap()["jobs"][0].clone();
    let result = jobs
        .cancel_pending(id.clone(), true, owner())
        .await
        .unwrap();
    assert_eq!(result["state"], "cancelled");
    assert_eq!(result["pendingCancellationAttempted"], true);
    assert_eq!(result["intent"], before["intent"]);
    assert_eq!(
        jobs.cancel_pending(
            id.clone(),
            true,
            Arc::new(|_| panic!("terminal ownership check"))
        )
        .await
        .unwrap(),
        result
    );
    assert_eq!(jobs.status_for(id).await.unwrap()["state"], "cancelled");
    let snapshot = jobs.workbench_poll().await.unwrap();
    assert_eq!(snapshot["stopAvailable"], true);
    assert_eq!(snapshot["jobs"][0]["state"], "cancelled");
    assert_eq!(f.lock().unwrap().posts, 1);
    assert_eq!(f.lock().unwrap().submissions, 1);
    let reports: Vec<_> = crate::logs::native_logs(0)
        .into_iter()
        .filter(|e| {
            e.text.contains("Workbench pending-cancel:")
                && e.text.contains(result["id"].as_str().unwrap())
        })
        .collect();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].source, crate::logs::Source::Deas);
    assert!(reports[0].text.contains("state=cancelled"));
    assert!(!reports[0].text.contains("a cat"));
    task.abort();
}

#[tokio::test]
async fn pending_cancel_refuses_foreign_and_preserves_observed_running_completed_unknown() {
    for (mode, expected) in [
        ("pending", "queued"),
        ("running", "running"),
        ("completed", "succeeded"),
        ("graph", "unknown"),
        ("client", "unknown"),
        ("id", "unknown"),
        ("foreign-history", "unknown"),
        ("absent", "unknown"),
    ] {
        let (jobs, id, f, task) = cancel_fixture(mode, 200).await;
        let held = jobs.workbench_job(&id).unwrap().unwrap();
        if matches!(mode, "pending" | "absent") {
            held.lock().unwrap().job.as_mut().unwrap().state = "unknown";
        }
        let before = held.current().unwrap();
        let result = jobs.cancel_pending(id, true, owner()).await.unwrap();
        assert_eq!(result["state"], expected, "{mode}: {result}");
        assert_eq!(result["intent"], before["intent"]);
        assert_eq!(f.lock().unwrap().posts, 0);
        if expected == "succeeded" {
            assert_eq!(result["outputs"][0]["filename"], "out.png");
        }
        if mode == "graph" {
            assert!(!result.to_string().contains("foreign-private-graph"));
        }
        task.abort();
    }
}

#[tokio::test]
async fn pending_cancel_requires_confirmation_held_id_owned_backend_and_idle_submission() {
    let (jobs, id, f, task) = cancel_fixture("pending", 200).await;
    assert!(jobs
        .cancel_pending(id.clone(), false, owner())
        .await
        .is_err());
    assert!(jobs
        .cancel_pending(PROMPT.into(), true, owner())
        .await
        .is_err());
    let diagnostic = jobs.prepare("http://127.0.0.1:9".into(), intent()).await;
    assert!(diagnostic.is_err()); // Active Workbench ownership excludes diagnostic preparation.
    assert!(jobs
        .cancel_pending(
            id.clone(),
            true,
            Arc::new(crate::backend::Lifecycle::default().cancellation_owner())
        )
        .await
        .is_err());
    let held = jobs.workbench_job(&id).unwrap().unwrap();
    held.lock().unwrap().job.as_mut().unwrap().submitting = true;
    assert!(jobs
        .cancel_pending(id.clone(), true, owner())
        .await
        .is_err());
    held.lock().unwrap().job.as_mut().unwrap().submitting = false;
    held.lock().unwrap().job.as_mut().unwrap().prompt_id = None;
    held.lock().unwrap().job.as_mut().unwrap().state = "unknown";
    assert_eq!(
        jobs.cancel_pending(id, true, owner()).await.unwrap()["state"],
        "unknown"
    );
    assert_eq!(f.lock().unwrap().reads, 0);
    assert_eq!(f.lock().unwrap().posts, 0);
    task.abort();
}

#[tokio::test]
async fn pending_cancel_lost_rejected_redirect_oversized_are_unknown_without_second_post() {
    for (mode, code) in [
        ("pending", 0),
        ("pending", 400),
        ("pending", 302),
        ("pending", 401),
        ("pending", 403),
        ("oversized", 200),
        ("nonempty", 200),
        ("still-pending", 200),
    ] {
        let (jobs, id, f, task) = cancel_fixture(mode, code).await;
        let held = jobs.workbench_job(&id).unwrap().unwrap();
        held.lock().unwrap().job.as_mut().unwrap().outputs =
            vec![json!({"filename":"retained.png"})];
        let result = jobs
            .cancel_pending(id.clone(), true, owner())
            .await
            .unwrap();
        assert_eq!(result["state"], "unknown", "{mode}/{code}: {result}");
        assert_eq!(result["outputs"][0]["filename"], "retained.png");
        assert_eq!(result["pendingCancellationAttempted"], true);
        assert!(jobs
            .cancel_pending(id.clone(), true, owner())
            .await
            .is_err());
        let _ = jobs.status_for(id.clone()).await.unwrap();
        assert!(jobs.cancel_pending(id, true, owner()).await.is_err());
        assert_eq!(f.lock().unwrap().posts, 1);
        assert_eq!(f.lock().unwrap().submissions, 1);
        assert!(!result.to_string().contains("private-token"));
        task.abort();
    }
}

#[tokio::test]
async fn pending_cancel_authentication_rejection_before_delete_has_no_token_or_post() {
    for code in [401, 403] {
        let (jobs, id, f, task) = cancel_fixture("auth", code).await;
        let result = jobs.cancel_pending(id, true, owner()).await.unwrap();
        assert_eq!(result["state"], "unknown");
        assert!(!result.to_string().contains("private-token-response"));
        assert_eq!(f.lock().unwrap().posts, 0);
        task.abort();
    }
}

#[tokio::test]
async fn pending_cancel_readback_preserves_running_and_completed_races() {
    for (mode, state) in [
        ("race-running", "running"),
        ("race-completed", "succeeded"),
        ("late-completed", "succeeded"),
    ] {
        let (jobs, id, f, task) = cancel_fixture(mode, 200).await;
        let root = tempfile::tempdir().unwrap();
        jobs.workbench_job(&id)
            .unwrap()
            .unwrap()
            .lock()
            .unwrap()
            .job
            .as_mut()
            .unwrap()
            .plan
            .media_root = Some(
            job_outputs::MediaRoot::capture(dunce::canonicalize(root.path()).unwrap()).unwrap(),
        );
        let result = jobs
            .cancel_pending(id.clone(), true, owner())
            .await
            .unwrap();
        assert_eq!(result["state"], state);
        if state == "succeeded" {
            assert_eq!(result["outputs"][0]["filename"], "out.png");
            assert!(!jobs
                .workbench_job(&id)
                .unwrap()
                .unwrap()
                .lock()
                .unwrap()
                .completed
                .is_empty());
            assert_eq!(
                jobs.cancel_pending(id, true, owner()).await.unwrap()["state"],
                "succeeded"
            );
        } else {
            assert!(jobs.cancel_pending(id, true, owner()).await.is_err());
        }
        assert_eq!(f.lock().unwrap().posts, 1);
        task.abort();
    }
}

#[tokio::test]
async fn pending_cancel_rechecks_lifecycle_before_mutation_and_after_readback() {
    for change_at in [3, 4, 5] {
        let (jobs, id, f, task) = cancel_fixture("pending", 200).await;
        let calls = Arc::new(AtomicUsize::new(0));
        let result = jobs
            .cancel_pending(
                id,
                true,
                Arc::new(move |_| {
                    let call = calls.fetch_add(1, Ordering::SeqCst);
                    Ok(if call >= change_at { 2 } else { 1 })
                }),
            )
            .await
            .unwrap();
        assert_eq!(
            result["state"], "unknown",
            "change at {change_at}: {result}"
        );
        assert_eq!(f.lock().unwrap().posts, usize::from(change_at > 3));
        task.abort();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pending_cancel_poll_overlap_and_dropped_waiter_do_not_duplicate_or_strand() {
    let (jobs, id, f, task) = cancel_fixture("slow", 200).await;
    let entered = f.lock().unwrap().entered.clone();
    let owner_jobs = jobs.clone();
    let job_id = id.clone();
    let waiter =
        tokio::spawn(async move { owner_jobs.cancel_pending(job_id, true, owner()).await });
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    waiter.abort();
    assert!(jobs
        .cancel_pending(id.clone(), true, owner())
        .await
        .is_err());
    assert_eq!(
        jobs.workbench_poll().await.unwrap()["jobs"][0]["state"],
        "queued"
    );
    let release = f.lock().unwrap().release.clone();
    *release.0.lock().unwrap() = true;
    release.1.notify_one();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if jobs.workbench_current().unwrap()["jobs"][0]["state"] == "cancelled" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        !jobs
            .workbench_job(&id)
            .unwrap()
            .unwrap()
            .lock()
            .unwrap()
            .job
            .as_ref()
            .unwrap()
            .polling
    );
    assert_eq!(f.lock().unwrap().posts, 1);
    task.abort();

    // The inverse ordering must not let cancellation overwrite an in-flight status clone.
    let (jobs, id, f, task) = cancel_fixture("slow", 200).await;
    let entered = f.lock().unwrap().entered.clone();
    let polling = jobs.clone();
    let waiter = tokio::spawn(async move { polling.workbench_poll().await });
    tokio::time::timeout(Duration::from_secs(2), entered.notified())
        .await
        .unwrap();
    assert!(jobs
        .cancel_pending(id.clone(), true, owner())
        .await
        .is_err());
    let release = f.lock().unwrap().release.clone();
    *release.0.lock().unwrap() = true;
    release.1.notify_one();
    waiter.await.unwrap().unwrap();
    assert_eq!(f.lock().unwrap().posts, 0);
    assert!(
        !jobs
            .workbench_job(&id)
            .unwrap()
            .unwrap()
            .lock()
            .unwrap()
            .job
            .as_ref()
            .unwrap()
            .polling
    );
    task.abort();
}
