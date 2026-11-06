//! Observer ownership follows native cancellation/Stop, without granting execution authority.
// The existing cancellation HTTP fixture completes its optional WebSocket attempt with 404.
// attach_observer waits for that completion before replacing the finished controller solely
// in this fixture, then exercises the real native cancellation/Stop commits with a live peer.
// Production never replaces or retries an observer; primary admission fixtures cover that path.
use super::super::observer_tests::attach_observer;
use super::*;

fn historical(jobs: &Jobs, id: &str) -> Value {
    let view = serde_json::to_value(jobs.3.view()).unwrap();
    view["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["id"] == id)
        .unwrap()
        .clone()
}

fn retain_product(held: &Jobs) -> Value {
    let output = json!({"filename":"retained.png","subfolder":"","type":"output"});
    let mut entry = held.lock().unwrap();
    let job = entry.job.as_mut().unwrap();
    job.outputs = vec![output.clone()];
    job.live.lock().unwrap().preview = Some("data:image/png;base64,retained".into());
    held.3.observe(job);
    output
}

#[tokio::test]
async fn watch_pending_cancel_terminal_commit_closes_without_erasing_products_or_history() {
    let (jobs, id, fixture, task) = cancel_fixture("pending", 200).await;
    let held = jobs.workbench_job(&id).unwrap().unwrap();
    let output = retain_product(&held);
    let before = held.current().unwrap();
    let job = held.lock().unwrap().job.as_ref().unwrap().clone();
    let mut peer = attach_observer(&job).await;
    drop(job);
    peer.ping().await;

    let result = jobs
        .cancel_pending(id.clone(), true, owner())
        .await
        .unwrap();
    peer.closed().await;
    assert_eq!(result["state"], "cancelled");
    assert_eq!(result["intent"], before["intent"]);
    assert_eq!(result["outputs"], json!([output]));
    assert_eq!(fixture.lock().unwrap().posts, 1);
    assert_eq!(fixture.lock().unwrap().submissions, 1);
    let record = historical(&jobs, &id);
    assert_eq!(record["state"], "cancelled");
    assert_eq!(record["intent"], before["intent"]);
    assert_eq!(record["outputs"][0]["filename"], "retained.png");
    assert_eq!(
        held.lock()
            .unwrap()
            .job
            .as_ref()
            .unwrap()
            .live
            .lock()
            .unwrap()
            .preview
            .as_deref(),
        Some("data:image/png;base64,retained")
    );
    task.abort();
}

#[tokio::test]
async fn watch_unknown_cancel_keeps_observer_until_confirmed_stop_without_retrying_mutation() {
    let (jobs, id, fixture, task) = cancel_fixture("pending", 0).await;
    let held = jobs.workbench_job(&id).unwrap().unwrap();
    let output = retain_product(&held);
    let before = held.current().unwrap();
    let job = held.lock().unwrap().job.as_ref().unwrap().clone();
    let mut peer = attach_observer(&job).await;
    drop(job);

    let uncertain = jobs
        .cancel_pending(id.clone(), true, owner())
        .await
        .unwrap();
    assert_eq!(uncertain["state"], "unknown");
    assert_eq!(uncertain["outputs"], json!([output]));
    peer.ping().await;
    assert!(
        !peer.is_finished(),
        "unknown is not a terminal observer outcome"
    );
    assert_eq!(fixture.lock().unwrap().posts, 1);

    let stopped = jobs
        .workbench_stop(true, owner(), interrupt_owner())
        .await
        .unwrap();
    peer.closed().await;
    assert_eq!(stopped["outcome"], "partial");
    assert_eq!(stopped["jobs"][0]["state"], "unknown");
    assert_eq!(stopped["jobs"][0]["intent"], before["intent"]);
    assert_eq!(stopped["jobs"][0]["outputs"][0]["filename"], "retained.png");
    assert_eq!(
        fixture.lock().unwrap().posts,
        1,
        "Stop must reconcile unknown without retrying cancellation"
    );
    assert_eq!(fixture.lock().unwrap().submissions, 1);
    let record = historical(&jobs, &id);
    assert_eq!(record["state"], "unknown");
    assert_eq!(record["intent"], before["intent"]);
    assert_eq!(record["outputs"][0]["filename"], "retained.png");
    task.abort();
}

#[tokio::test]
async fn watch_stop_confirmation_and_partial_authority_preserve_execution_state() {
    let (jobs, id, fixture, task) = cancel_fixture("pending", 200).await;
    let held = jobs.workbench_job(&id).unwrap().unwrap();
    retain_product(&held);
    let before = held.current().unwrap();
    let history = historical(&jobs, &id);
    let job = held.lock().unwrap().job.as_ref().unwrap().clone();
    let mut peer = attach_observer(&job).await;
    drop(job);

    assert!(jobs
        .workbench_stop(false, owner(), interrupt_owner())
        .await
        .is_err());
    peer.ping().await;
    assert!(!peer.is_finished());
    assert_eq!(jobs.workbench_current().unwrap()["admissionEpoch"], 0);

    let stopped = jobs
        .workbench_stop(
            true,
            Arc::new(|_| Err("Fixture backend is not owned".into())),
            interrupt_owner(),
        )
        .await
        .unwrap();
    peer.closed().await;
    assert_eq!(stopped["outcome"], "partial");
    assert_eq!(stopped["jobs"][0]["state"], "queued");
    assert_eq!(stopped["jobs"][0]["intent"], before["intent"]);
    assert_eq!(stopped["jobs"][0]["outputs"], before["outputs"]);
    assert_eq!(stopped["issues"].as_array().unwrap().len(), 1);
    assert_eq!(stopped["admissionEpoch"], 1);
    assert_eq!(
        historical(&jobs, &id),
        history,
        "display cleanup cannot publish an invented terminal outcome"
    );
    let fixture = fixture.lock().unwrap();
    assert_eq!(fixture.posts, 0);
    assert_eq!(fixture.reads, 0);
    assert_eq!(fixture.submissions, 1);
    task.abort();
}

#[tokio::test]
async fn watch_scoped_cancel_observed_completion_and_interruption_close_only_after_terminal_proof()
{
    for (mode, running, expected, filename, posts) in [
        ("completed", false, "succeeded", "out.png", 0),
        (
            "interrupt-partial-interrupted",
            true,
            "cancelled",
            "partial.png",
            1,
        ),
    ] {
        let (jobs, id, fixture, task) = cancel_fixture(mode, 200).await;
        let held = jobs.workbench_job(&id).unwrap().unwrap();
        let before = held.current().unwrap();
        let job = held.lock().unwrap().job.as_ref().unwrap().clone();
        let mut peer = attach_observer(&job).await;
        drop(job);
        let result = if running {
            jobs.interrupt_running(id.clone(), true, interrupt_owner())
                .await
                .unwrap()
        } else {
            jobs.cancel_pending(id.clone(), true, owner())
                .await
                .unwrap()
        };
        peer.closed().await;
        assert_eq!(result["state"], expected, "{mode}");
        assert_eq!(result["intent"], before["intent"], "{mode}");
        assert_eq!(result["outputs"][0]["filename"], filename, "{mode}");
        assert_eq!(fixture.lock().unwrap().posts, posts, "{mode}");
        assert_eq!(fixture.lock().unwrap().submissions, 1, "{mode}");
        let record = historical(&jobs, &id);
        assert_eq!(record["state"], expected, "{mode}");
        assert_eq!(record["intent"], before["intent"], "{mode}");
        assert_eq!(record["outputs"][0]["filename"], filename, "{mode}");
        task.abort();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn watch_stop_observer_finishes_while_backend_readback_waits_and_waiter_is_dropped() {
    let (jobs, id, fixture, task) = cancel_fixture("slow", 200).await;
    let held = jobs.workbench_job(&id).unwrap().unwrap();
    let job = held.lock().unwrap().job.as_ref().unwrap().clone();
    let mut peer = attach_observer(&job).await;
    drop(job);
    let entered = fixture.lock().unwrap().entered.clone();
    let stopping = jobs.clone();
    let waiter = tokio::spawn(async move {
        stopping
            .workbench_stop(true, owner(), interrupt_owner())
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    peer.closed().await;
    let mut stop_result = jobs.lock().unwrap().stop.as_ref().unwrap().clone();
    assert!(held.lock().unwrap().job.as_ref().unwrap().polling);
    assert!(held
        .lock()
        .unwrap()
        .job
        .as_ref()
        .unwrap()
        .live
        .try_lock()
        .is_ok());
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());

    let release = fixture.lock().unwrap().release.clone();
    *release.0.lock().unwrap() = true;
    release.1.notify_one();
    tokio::time::timeout(
        Duration::from_secs(5),
        stop_result.wait_for(|result| result.is_some()),
    )
    .await
    .unwrap()
    .unwrap();
    drop(stop_result);
    let current = jobs.workbench_current().unwrap();
    assert_eq!(current["jobs"][0]["state"], "cancelled");
    assert_eq!(current["admissionEpoch"], 1);
    assert_eq!(historical(&jobs, &id)["state"], "cancelled");
    assert_eq!(fixture.lock().unwrap().posts, 1);
    assert_eq!(fixture.lock().unwrap().submissions, 1);
    task.abort();
}
