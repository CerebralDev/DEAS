//! Loopback protocol fixtures. Completion is the observer task's bounded-cleanup acknowledgement,
//! not merely a cancellation flag, peer Close receipt, or a sampled process-memory value.

#[tokio::test]
async fn watch_temporal_preview_disabled_graph_never_accepts_kj_payloads() {
    use base64::Engine;
    let mut server = fixture().await;
    let jobs = Jobs::default();
    let (_, mut peer) = server.diagnostic(&jobs, 92).await;
    let job = jobs.lock().unwrap().job.as_ref().unwrap().clone();
    peer.send(Message::Text(
        json!({"type":"execution_start","data":{"prompt_id":"own"}})
            .to_string()
            .into(),
    ))
    .await;
    peer.send(Message::Text(json!({"type":"kj_preview_override","data":{"node_id":"old-node","w":32,"h":32,"fps":8,"mime":"image/webp","image":base64::engine::general_purpose::STANDARD.encode(include_bytes!("../test-data/preview/h3.webp"))}}).to_string().into())).await;
    peer.ping().await;
    assert!(job.live.lock().unwrap().temporal.is_none());
    job.stop_observer();
    peer.closed().await;
}
#[tokio::test]
async fn watch_temporal_preview_is_directed_node_bound_latest_owned_and_stop_cleaned() {
    use base64::Engine;
    for terminal in [false, true] {
        let mut server = fixture().await;
        let jobs = Jobs::default();
        let (status, mut old_peer) = server.diagnostic(&jobs, 91).await;
        let id = status["id"].as_str().unwrap();
        let job = jobs.lock().unwrap().job.as_ref().unwrap().clone();
        job.stop_observer();
        old_peer.closed().await;
        {
            let mut live = job.live.lock().unwrap();
            live.preview_nodes = vec!["deas-preview-unique-plan-node".into()];
            live.preview_job = id.into();
        }
        let mut peer = attach_observer(&job).await;
        let event = |node: &str, bytes: &[u8], mime: &str| {
            Message::Text(json!({"type":"kj_preview_override","data":{"node_id":node,"w":32,"h":32,"fps":8,"mime":mime,"image":base64::engine::general_purpose::STANDARD.encode(bytes)}}).to_string().into())
        };
        let webp = include_bytes!("../test-data/preview/h3.webp");
        peer.send(event("deas-preview-unique-plan-node", webp, "image/webp"))
            .await;
        peer.ping().await;
        assert!(
            job.live.lock().unwrap().temporal.is_none(),
            "no directed execution_start yet"
        );
        peer.send(Message::Text(
            json!({"type":"execution_start","data":{"prompt_id":"own-prompt"}})
                .to_string()
                .into(),
        ))
        .await;
        peer.send(event("deas-preview-previous-plan-node", webp, "image/webp"))
            .await;
        peer.ping().await;
        assert!(
            job.live.lock().unwrap().temporal.is_none(),
            "foreign async producer on the current socket"
        );
        peer.send(event("deas-preview-unique-plan-node", webp, "image/webp"))
            .await;
        peer.ping().await;
        assert_eq!(
            job.snapshot()["temporalPreview"]["frames"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        // KJ's valid message intentionally has no prompt_id.
        peer.send(event(
            "deas-preview-unique-plan-node",
            include_bytes!("../test-data/preview/preview.mp4"),
            "video/mp4",
        ))
        .await;
        peer.ping().await;
        assert!(jobs.preview_video(id, 1).is_err());
        let lease = jobs.preview_video(id, 2).unwrap();
        let path = lease.path().to_owned();
        peer.send(event(
            "deas-preview-unique-plan-node",
            b"not a video",
            "video/mp4",
        ))
        .await;
        peer.ping().await;
        assert_eq!(
            job.snapshot()["temporalPreview"]["revision"],
            2,
            "malformed replacement preserves the latest playable asset"
        );
        if terminal {
            peer.send(Message::Text(
                json!({"type":"execution_success","data":{"prompt_id":"own-prompt"}})
                    .to_string()
                    .into(),
            ))
            .await;
        } else {
            job.stop_observer();
        }
        peer.closed().await;
        assert!(job.live.lock().unwrap().temporal.is_none());
        assert!(jobs.preview_video(id, 2).is_err());
        assert!(
            path.exists(),
            "active player retains its lease through close"
        );
        drop(lease);
        assert!(!path.exists());
    }
}

use super::*;
use futures_util::{SinkExt, StreamExt};
use std::collections::{HashMap, HashSet};
use tokio::sync::{mpsc, oneshot, watch as channel};
use tokio_tungstenite::tungstenite::Message;

const WATCHDOG: Duration = Duration::from_secs(5);

pub(super) async fn finished(mut completed: channel::Receiver<bool>) {
    tokio::time::timeout(WATCHDOG, completed.wait_for(|done| *done))
        .await
        .expect("observer cleanup watchdog")
        .expect("observer completion sender disappeared without acknowledgement");
}

struct SendFrame {
    frame: Message,
    sent: oneshot::Sender<bool>,
}

pub(super) struct Peer {
    commands: mpsc::UnboundedSender<SendFrame>,
    frames: mpsc::UnboundedReceiver<Message>,
    completed: Option<channel::Receiver<bool>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl Peer {
    fn bind(&mut self, job: &Job) {
        self.completed = Some(job.live.lock().unwrap().observer.completion());
    }

    pub(super) fn is_finished(&self) -> bool {
        *self.completed.as_ref().unwrap().borrow()
    }

    pub(super) async fn send(&self, frame: Message) {
        let (sent, received) = oneshot::channel();
        self.commands.send(SendFrame { frame, sent }).unwrap();
        assert!(tokio::time::timeout(WATCHDOG, received)
            .await
            .unwrap()
            .unwrap());
    }

    pub(super) async fn ping(&mut self) {
        self.send(Message::Ping(b"observer-fixture".to_vec().into()))
            .await;
        assert_eq!(
            tokio::time::timeout(WATCHDOG, self.frames.recv())
                .await
                .unwrap()
                .unwrap(),
            Message::Pong(b"observer-fixture".to_vec().into())
        );
    }

    pub(super) async fn closed(&mut self) {
        assert!(matches!(
            tokio::time::timeout(WATCHDOG, self.frames.recv())
                .await
                .unwrap(),
            Some(Message::Close(_))
        ));
        finished(self.completed.as_ref().unwrap().clone()).await;
        tokio::time::timeout(WATCHDOG, self.task.take().unwrap())
            .await
            .unwrap()
            .unwrap();
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort(); // Fixture-only failure cleanup; production cleanup is asserted above.
        }
    }
}

fn peer(socket: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) -> Peer {
    let (commands, mut receive) = mpsc::unbounded_channel::<SendFrame>();
    let (frames, observed) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        let mut socket = socket;
        loop {
            tokio::select! {
                command = receive.recv() => {
                    let Some(command) = command else { break };
                    let sent = socket.send(command.frame).await.is_ok();
                    let _ = command.sent.send(sent);
                    if !sent { break; }
                }
                frame = socket.next() => {
                    let Some(Ok(frame)) = frame else { break };
                    let closing = matches!(frame, Message::Close(_));
                    let ping = matches!(frame, Message::Ping(_));
                    if frames.send(frame).is_err() { break; }
                    if closing || ping {
                        let _ = socket.flush().await;
                    }
                    if closing { break; }
                }
            }
        }
    });
    Peer {
        commands,
        frames: observed,
        completed: None,
        task: Some(task),
    }
}

/// Hook-specific cancellation fixtures use their existing HTTP-only server. Its first optional
/// WebSocket attempt has already failed and completed. Replace ONLY that finished fixture
/// controller to attach a real socket; production never rearms a controller or retries transport.
pub(super) async fn attach_observer(job: &Job) -> Peer {
    let previous = job.live.lock().unwrap().observer.completion();
    finished(previous).await;
    job.live.lock().unwrap().observer = observer::Observer::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let accepted = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        tokio_tungstenite::accept_async(stream).await.unwrap()
    });
    watch(&base, &job.client_id, job.live.clone()).await;
    let mut peer = peer(
        tokio::time::timeout(WATCHDOG, accepted)
            .await
            .unwrap()
            .unwrap(),
    );
    peer.bind(job);
    peer
}

#[derive(Default)]
struct HttpEvidence {
    tuples: HashMap<String, (u64, Value)>,
    terminal: HashMap<u64, &'static str>,
    absent: HashSet<u64>,
    pause: Option<(u64, Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    submission_pause: Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
    rejected: bool,
    posts: usize,
    mutations: usize,
}

struct Fixture {
    base: String,
    evidence: Arc<Mutex<HttpEvidence>>,
    peers: mpsc::UnboundedReceiver<(String, Peer)>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Fixture {
    async fn run(&mut self, jobs: &Jobs, seed: u64) -> (Value, Peer) {
        let mut draft = intent();
        draft["seed"] = json!(seed);
        let result = jobs
            .workbench_run(self.base.clone(), draft, true, 0, None)
            .await
            .unwrap();
        let held = jobs
            .workbench_job(result["id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        let job = held.lock().unwrap().job.as_ref().unwrap().clone();
        let (client, mut peer) = tokio::time::timeout(WATCHDOG, self.peers.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(client, job.client_id);
        peer.bind(&job);
        (result, peer)
    }

    async fn diagnostic(&mut self, jobs: &Jobs, seed: u64) -> (Value, Peer) {
        let mut draft = intent();
        draft["seed"] = json!(seed);
        let plan = jobs.prepare(self.base.clone(), draft).await.unwrap();
        let result = jobs
            .submit(plan["id"].as_str().unwrap().into(), true)
            .await
            .unwrap();
        let job = jobs.lock().unwrap().job.as_ref().unwrap().clone();
        let (client, mut peer) = tokio::time::timeout(WATCHDOG, self.peers.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(client, job.client_id);
        peer.bind(&job);
        (result, peer)
    }
}

async fn fixture() -> Fixture {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/comfy", listener.local_addr().unwrap());
    let evidence = Arc::new(Mutex::new(HttpEvidence::default()));
    let data = evidence.clone();
    let (sender, peers) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let data = data.clone();
            let sender = sender.clone();
            tokio::spawn(async move {
                // Peek without consuming the handshake accepted by Tungstenite below.
                let mut peek = [0; 8192];
                let first = tokio::time::timeout(WATCHDOG, async {
                    loop {
                        let count = stream.peek(&mut peek).await.unwrap();
                        assert!(count > 0 && count < peek.len());
                        if let Some(end) = peek[..count].windows(4).position(|w| w == b"\r\n\r\n") {
                            break String::from_utf8_lossy(&peek[..end]).into_owned();
                        }
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
                if first.starts_with("GET /comfy/ws?clientId=") {
                    let url = first
                        .lines()
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .nth(1)
                        .unwrap();
                    let url = reqwest::Url::parse(&format!("http://fixture{url}")).unwrap();
                    let client = url
                        .query_pairs()
                        .find(|(key, _)| key == "clientId")
                        .unwrap()
                        .1
                        .into_owned();
                    let socket = tokio_tungstenite::accept_async(stream).await.unwrap();
                    assert!(sender.send((client, peer(socket))).is_ok());
                    return;
                }
                let mut bytes = Vec::new();
                let (head_end, length) = loop {
                    let mut buffer = [0; 4096];
                    let count = stream.read(&mut buffer).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buffer[..count]);
                    if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&bytes[..end]);
                        let length = head
                            .lines()
                            .filter_map(|line| line.split_once(':'))
                            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                            .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                            .unwrap_or(0);
                        break (end + 4, length);
                    }
                };
                while bytes.len() < head_end + length {
                    let mut buffer = [0; 4096];
                    let count = stream.read(&mut buffer).await.unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&buffer[..count]);
                }
                let path = first.lines().next().unwrap();
                let body = if length == 0 {
                    Value::Null
                } else {
                    serde_json::from_slice(&bytes[head_end..head_end + length]).unwrap()
                };
                let pause = {
                    let mut state = data.lock().unwrap();
                    let seed = path
                        .strip_prefix("GET /comfy/history/")
                        .and_then(|tail| tail.split_whitespace().next())
                        .and_then(|id| state.tuples.get(id))
                        .map(|(seed, _)| *seed);
                    if path == "POST /comfy/prompt HTTP/1.1" {
                        state
                            .submission_pause
                            .take()
                            .map(|(entered, release)| (0, entered, release))
                    } else if state
                        .pause
                        .as_ref()
                        .is_some_and(|(paused, _, _)| Some(*paused) == seed)
                    {
                        state.pause.take()
                    } else {
                        None
                    }
                };
                if let Some((_, entered, release)) = pause {
                    entered.notify_one();
                    release.notified().await;
                }
                let body = {
                    let mut state = data.lock().unwrap();
                    if path == "GET /comfy/object_info HTTP/1.1" {
                        nodes()
                    } else if path == "POST /comfy/prompt HTTP/1.1" {
                        state.posts += 1;
                        if state.rejected {
                            json!({"error":"fixture rejection","node_errors":{"5":{}}})
                        } else {
                            let id = uuid::Uuid::new_v4().to_string();
                            let seed = body["prompt"]["5"]["inputs"]["seed"].as_u64().unwrap();
                            let tuple = json!([0,id,body["prompt"],{"client_id":body["client_id"]},["7"]]);
                            state.tuples.insert(id.clone(), (seed, tuple));
                            json!({"prompt_id":id,"number":0,"node_errors":{}})
                        }
                    } else if path.starts_with("GET /comfy/history/") {
                        let id = path.split('/').nth(3).unwrap().split_whitespace().next().unwrap();
                        let (seed, tuple) = &state.tuples[id];
                        if let Some(outcome) = state.terminal.get(seed) {
                            let (status, completed, messages) = match *outcome {
                                "succeeded" => ("success", true, json!([])),
                                "cancelled" => ("error", false, json!([["execution_interrupted",{"prompt_id":id}]])),
                                "failed" => ("error", false, json!([["execution_error",{"prompt_id":id}]])),
                                _ => unreachable!(),
                            };
                            json!({id:{"prompt":tuple,"outputs":{"7":{"images":[{"filename":format!("out-{seed}.png"),"subfolder":"","type":"output"}]}},"status":{"completed":completed,"status_str":status,"messages":messages}}})
                        } else {
                            json!({})
                        }
                    } else if path == "GET /comfy/queue HTTP/1.1" {
                        let pending: Vec<_> = state.tuples.values().filter(|(seed, _)| !state.terminal.contains_key(seed) && !state.absent.contains(seed)).map(|(_, tuple)| tuple).collect();
                        json!({"queue_running":[],"queue_pending":pending})
                    } else {
                        state.mutations += 1;
                        panic!("observer must not send a backend mutation: {path}");
                    }
                }.to_string();
                let head = format!(
                    "HTTP/1.1 200 Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(head.as_bytes()).await.unwrap();
                stream.write_all(body.as_bytes()).await.unwrap();
            });
        }
    });
    Fixture {
        base,
        evidence,
        peers,
        task,
    }
}

#[tokio::test]
async fn watch_native_rest_terminal_commits_close_silent_and_pinging_observers() {
    for (index, outcome) in ["succeeded", "failed", "cancelled"].into_iter().enumerate() {
        let mut fixture = fixture().await;
        let jobs = Jobs::default();
        let (submitted, mut peer) = fixture.run(&jobs, index as u64).await;
        let before = submitted["intent"].clone();
        let id = submitted["id"].as_str().unwrap().to_owned();
        let result = if index == 0 {
            fixture
                .evidence
                .lock()
                .unwrap()
                .terminal
                .insert(index as u64, outcome);
            jobs.status_for(id).await.unwrap()
        } else {
            let entered = Arc::new(tokio::sync::Notify::new());
            let release = Arc::new(tokio::sync::Notify::new());
            fixture.evidence.lock().unwrap().pause =
                Some((index as u64, entered.clone(), release.clone()));
            let polling = jobs.clone();
            let status = tokio::spawn(async move { polling.status_for(id).await });
            tokio::time::timeout(WATCHDOG, entered.notified())
                .await
                .unwrap();
            // Heartbeats continue while authoritative REST completion is pending. No peer
            // execution-success/error/interrupted event is sent in any of these cases.
            peer.ping().await;
            peer.ping().await;
            fixture
                .evidence
                .lock()
                .unwrap()
                .terminal
                .insert(index as u64, outcome);
            release.notify_one();
            tokio::time::timeout(WATCHDOG, status)
                .await
                .unwrap()
                .unwrap()
                .unwrap()
        };
        assert_eq!(result["state"], outcome);
        assert_eq!(result["intent"], before);
        peer.closed().await;
        assert!(peer.is_finished());
        let history = serde_json::to_value(jobs.3.view()).unwrap();
        assert_eq!(history["records"][0]["state"], outcome);
        assert_eq!(
            history["records"][0]["outputs"][0]["filename"],
            format!("out-{index}.png")
        );
        assert_eq!(fixture.evidence.lock().unwrap().posts, 1);
        assert_eq!(fixture.evidence.lock().unwrap().mutations, 0);
    }
}

#[tokio::test]
async fn watch_final_native_owner_release_wakes_idle_receive_but_temporary_clone_does_not() {
    let mut fixture = fixture().await;
    let jobs = Jobs::default();
    let (_, mut peer) = fixture.run(&jobs, 1).await;
    let temporary = jobs.clone();
    drop(temporary);
    peer.ping().await;
    assert!(!peer.is_finished());
    drop(jobs);
    peer.closed().await;
    assert_eq!(fixture.evidence.lock().unwrap().mutations, 0);
}

#[tokio::test]
async fn watch_unknown_status_retains_observer_and_reconciles_without_resubmission() {
    let mut fixture = fixture().await;
    let jobs = Jobs::default();
    let (submitted, mut peer) = fixture.run(&jobs, 1).await;
    fixture.evidence.lock().unwrap().absent.insert(1);
    let id = submitted["id"].as_str().unwrap().to_owned();
    assert_eq!(
        jobs.status_for(id.clone()).await.unwrap()["state"],
        "unknown"
    );
    peer.ping().await;
    assert!(!peer.is_finished());
    assert_eq!(
        jobs.workbench_current().unwrap()["jobs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    fixture
        .evidence
        .lock()
        .unwrap()
        .terminal
        .insert(1, "succeeded");
    assert_eq!(jobs.status_for(id).await.unwrap()["state"], "succeeded");
    peer.closed().await;
    assert_eq!(fixture.evidence.lock().unwrap().posts, 1);
    assert_eq!(fixture.evidence.lock().unwrap().mutations, 0);
}

#[tokio::test]
async fn watch_abandoned_workbench_poll_waiter_preserves_native_observer_until_terminal_commit() {
    let mut fixture = fixture().await;
    let jobs = Jobs::default();
    let (submitted, mut peer) = fixture.run(&jobs, 1).await;
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    fixture.evidence.lock().unwrap().pause = Some((1, entered.clone(), release.clone()));
    let polling = jobs.clone();
    let waiter = tokio::spawn(async move { polling.workbench_poll().await });
    tokio::time::timeout(WATCHDOG, entered.notified())
        .await
        .unwrap();
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    peer.ping().await;
    assert!(
        !peer.is_finished(),
        "native ownership outlives a dropped IPC waiter"
    );
    fixture
        .evidence
        .lock()
        .unwrap()
        .terminal
        .insert(1, "succeeded");
    release.notify_one();
    peer.closed().await;
    let current = jobs.workbench_current().unwrap();
    assert_eq!(current["jobs"][0]["id"], submitted["id"]);
    assert_eq!(current["jobs"][0]["state"], "succeeded");
    assert_eq!(current["jobs"][0]["intent"], submitted["intent"]);
    assert_eq!(fixture.evidence.lock().unwrap().posts, 1);
}

#[tokio::test]
async fn watch_abandoned_run_waiter_retains_original_observer_through_submission_rejection() {
    let mut fixture = fixture().await;
    let jobs = Jobs::default();
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    {
        let mut evidence = fixture.evidence.lock().unwrap();
        evidence.rejected = true;
        evidence.submission_pause = Some((entered.clone(), release.clone()));
    }
    let submitting = jobs.clone();
    let base = fixture.base.clone();
    let waiter = tokio::spawn(async move {
        submitting
            .workbench_run(base, intent(), true, 0, None)
            .await
    });
    tokio::time::timeout(WATCHDOG, entered.notified())
        .await
        .unwrap();
    let current = jobs.workbench_current().unwrap();
    let held = jobs
        .workbench_job(current["jobs"][0]["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    let job = held.lock().unwrap().job.as_ref().unwrap().clone();
    let (client, mut peer) = tokio::time::timeout(WATCHDOG, fixture.peers.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(client, job.client_id);
    peer.bind(&job);
    drop(job);
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    assert!(held.lock().unwrap().job.as_ref().unwrap().submitting);
    peer.ping().await;
    assert!(!peer.is_finished());
    release.notify_one();
    // The terminal rejection commit is itself the native finalization checkpoint. No polling,
    // synthetic completion hook, timeout sleep, or retry is needed after abandoning Run's waiter.
    peer.closed().await;
    let current = jobs.workbench_current().unwrap();
    assert_eq!(current["jobs"][0]["state"], "failed");
    assert!(!held.lock().unwrap().job.as_ref().unwrap().submitting);
    let history = serde_json::to_value(jobs.3.view()).unwrap();
    assert_eq!(history["records"][0]["state"], "failed");
    assert_eq!(fixture.evidence.lock().unwrap().posts, 1);
    assert_eq!(fixture.evidence.lock().unwrap().mutations, 0);
}

#[tokio::test]
async fn watch_diagnostic_slot_replacement_does_not_let_old_cleanup_cancel_new_owner() {
    let mut fixture = fixture().await;
    let jobs = Jobs::default();
    let (first, mut old_peer) = fixture.diagnostic(&jobs, 1).await;
    let old = jobs.lock().unwrap().job.as_ref().unwrap().clone();
    fixture
        .evidence
        .lock()
        .unwrap()
        .terminal
        .insert(1, "succeeded");
    jobs.status(first["id"].as_str().unwrap().into())
        .await
        .unwrap();
    old_peer.closed().await;
    let (second, mut new_peer) = fixture.diagnostic(&jobs, 2).await;
    assert_ne!(first["id"], second["id"]);
    drop(old);
    new_peer.ping().await;
    assert!(!new_peer.is_finished());
    fixture
        .evidence
        .lock()
        .unwrap()
        .terminal
        .insert(2, "succeeded");
    let result = jobs
        .status(second["id"].as_str().unwrap().into())
        .await
        .unwrap();
    assert_eq!(result["outputs"][0]["filename"], "out-2.png");
    new_peer.closed().await;
    let history = serde_json::to_value(jobs.3.view()).unwrap();
    assert_eq!(history["records"].as_array().unwrap().len(), 2);
    assert_eq!(history["records"][1]["outputs"][0]["filename"], "out-1.png");
    assert_eq!(fixture.evidence.lock().unwrap().posts, 2);
}

#[tokio::test]
async fn watch_job_clones_independence_and_late_old_cleanup_preserve_new_observer() {
    let mut fixture = fixture().await;
    let jobs = Jobs::default();
    let (a, mut peer_a) = fixture.run(&jobs, 1).await;
    let (b, mut peer_b) = fixture.run(&jobs, 2).await;
    let held_a = jobs
        .workbench_job(a["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    let clone_a = held_a.lock().unwrap().job.as_ref().unwrap().clone();
    drop(clone_a.clone());
    peer_a.ping().await;
    fixture
        .evidence
        .lock()
        .unwrap()
        .terminal
        .insert(1, "succeeded");
    jobs.status_for(a["id"].as_str().unwrap().into())
        .await
        .unwrap();
    peer_a.closed().await;
    let (c, mut peer_c) = fixture.run(&jobs, 3).await;
    drop(clone_a);
    drop(held_a);
    peer_b
        .send(Message::Text(
            json!({"type":"progress","data":{"value":7,"max":10}})
                .to_string()
                .into(),
        ))
        .await;
    peer_b
        .send(Message::Binary(
            [0, 0, 0, 1, 0, 0, 0, 1, 0xff, 0xd8, 0xff].to_vec().into(),
        ))
        .await;
    peer_b.ping().await;
    peer_c.ping().await;
    let state_b = jobs
        .workbench_job(b["id"].as_str().unwrap())
        .unwrap()
        .unwrap()
        .current()
        .unwrap();
    assert_eq!(state_b["progress"], json!({"value":7,"max":10}));
    assert!(state_b["preview"]
        .as_str()
        .unwrap()
        .starts_with("data:image/jpeg;base64,"));
    assert!(!peer_b.is_finished() && !peer_c.is_finished());
    for (result, seed) in [(b, 2), (c, 3)] {
        fixture
            .evidence
            .lock()
            .unwrap()
            .terminal
            .insert(seed, "succeeded");
        jobs.status_for(result["id"].as_str().unwrap().into())
            .await
            .unwrap();
    }
    peer_b.closed().await;
    peer_c.closed().await;
    assert_eq!(fixture.evidence.lock().unwrap().posts, 3);
}

#[tokio::test]
async fn watch_submission_rejection_closes_observer_without_backend_cancel() {
    let mut fixture = fixture().await;
    fixture.evidence.lock().unwrap().rejected = true;
    let jobs = Jobs::default();
    let (result, mut peer) = fixture.run(&jobs, 1).await;
    assert_eq!(result["state"], "failed");
    assert!(result["promptId"].is_null());
    peer.closed().await;
    assert_eq!(fixture.evidence.lock().unwrap().posts, 1);
    assert_eq!(fixture.evidence.lock().unwrap().mutations, 0);
}

#[tokio::test]
async fn watch_observed_cancellation_rejects_queued_frames_and_preserves_last_preview() {
    let mut fixture = fixture().await;
    let jobs = Jobs::default();
    let (submitted, mut peer) = fixture.run(&jobs, 1).await;
    let held = jobs
        .workbench_job(submitted["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    let live = held.lock().unwrap().job.as_ref().unwrap().live.clone();
    peer.send(Message::Text(
        json!({"type":"progress","data":{"value":7,"max":10}})
            .to_string()
            .into(),
    ))
    .await;
    peer.send(Message::Binary(
        [0, 0, 0, 1, 0, 0, 0, 1, 0xff, 0xd8, 0xff].to_vec().into(),
    ))
    .await;
    peer.ping().await; // Ordered roundtrip proves both prior frames were consumed.
    let previous = {
        let live = live.lock().unwrap();
        live.observer.cancel();
        (live.value, live.max, live.preview.clone())
    };
    // Race queued peer data against the cancellation wakeup; it must never publish afterward.
    // Sending may lose to Close, so the fixture deliberately does not require a successful write.
    for frame in [
        Message::Text(
            json!({"type":"progress","data":{"value":99,"max":100}})
                .to_string()
                .into(),
        ),
        Message::Binary(
            [0, 0, 0, 1, 0, 0, 0, 1, 0xff, 0xd8, 0xff, 0]
                .to_vec()
                .into(),
        ),
    ] {
        let (sent, _) = oneshot::channel();
        let _ = peer.commands.send(SendFrame { frame, sent });
    }
    peer.closed().await;
    let live = live.lock().unwrap();
    assert_eq!((live.value, live.max, live.preview.clone()), previous);
    drop(live);
    assert_eq!(held.current().unwrap()["state"], "queued");
    assert_eq!(fixture.evidence.lock().unwrap().mutations, 0);
}

#[tokio::test]
async fn watch_final_owner_release_interrupts_an_unfinished_connection_handshake() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let (entered, accepted) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
            let mut bytes = [0; 1024];
            let count = stream.read(&mut bytes).await.unwrap();
            assert!(count > 0);
            request.extend_from_slice(&bytes[..count]);
        }
        entered.send(()).unwrap();
        // Never send an Upgrade response: cancellation must release the connecting socket.
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).await.unwrap(), 0);
    });
    let live: Arc<Mutex<Live>> = Arc::default();
    let completed = live.lock().unwrap().observer.completion();
    let weak = Arc::downgrade(&live);
    let connecting = live.clone();
    let watcher = tokio::spawn(async move { watch(&base, "connecting-client", connecting).await });
    tokio::time::timeout(WATCHDOG, accepted)
        .await
        .unwrap()
        .unwrap();
    drop(live);
    assert!(
        weak.upgrade().is_none(),
        "connection future must not retain Live ownership"
    );
    // A whole-second watchdog is shorter than the existing three-second connection deadline.
    tokio::time::timeout(Duration::from_secs(2), watcher)
        .await
        .unwrap()
        .unwrap();
    finished(completed).await;
    tokio::time::timeout(WATCHDOG, server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn watch_repeated_admission_terminal_eviction_quiesces_without_losing_history_or_outputs() {
    let mut fixture = fixture().await;
    let jobs = Jobs::default();
    let mut completed = Vec::new();
    let mut first = None;
    for seed in 0..25 {
        let (result, mut peer) = fixture.run(&jobs, seed).await;
        first.get_or_insert_with(|| result["id"].as_str().unwrap().to_owned());
        fixture
            .evidence
            .lock()
            .unwrap()
            .terminal
            .insert(seed, "succeeded");
        let result = jobs
            .status_for(result["id"].as_str().unwrap().into())
            .await
            .unwrap();
        assert_eq!(result["outputs"][0]["filename"], format!("out-{seed}.png"));
        peer.closed().await;
        assert!(peer.is_finished());
        completed.push(peer.completed.as_ref().unwrap().clone());
    }
    assert_eq!(completed.len(), 25);
    assert!(
        completed.iter().all(|receiver| *receiver.borrow()),
        "zero surviving observers after bounded cleanup"
    );
    assert!(jobs
        .workbench_job(first.as_deref().unwrap())
        .unwrap()
        .is_none());
    let queue = jobs.workbench_current().unwrap();
    assert_eq!(queue["jobs"].as_array().unwrap().len(), 20);
    assert_eq!(queue["jobs"][0]["intent"]["seed"], 5);
    let history = serde_json::to_value(jobs.3.view()).unwrap();
    assert_eq!(history["records"].as_array().unwrap().len(), 10);
    assert_eq!(history["records"][0]["intent"]["seed"], 24);
    assert_eq!(history["records"][9]["intent"]["seed"], 15);
    assert!(history["records"]
        .as_array()
        .unwrap()
        .iter()
        .all(|record| record["state"] == "succeeded"
            && record["outputs"].as_array().unwrap().len() == 1));
    assert_eq!(fixture.evidence.lock().unwrap().posts, 25);
    assert_eq!(fixture.evidence.lock().unwrap().mutations, 0);
}
