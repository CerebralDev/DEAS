//! Optional display observation belongs to the native job's shared live state.
use super::{canonical_uuid, preview_frame, Live, RESPONSE_LIMIT};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::{
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::{protocol::WebSocketConfig, Error, Message};

#[derive(Clone, Copy)]
enum Failure {
    Connect,
    Receive,
    Flush,
    Close,
    Decode,
}

impl Failure {
    fn label(self) -> (usize, &'static str) {
        match self {
            Self::Connect => (0, "connect"),
            Self::Receive => (1, "receive"),
            Self::Flush => (2, "flush"),
            Self::Close => (3, "close"),
            Self::Decode => (4, "decode"),
        }
    }
}

struct FailureLog {
    last: [Option<Instant>; 5],
    suppressed: [u64; 5],
}

impl FailureLog {
    fn report(&mut self, failure: Failure, client_id: &str) {
        let (index, operation) = failure.label();
        if self.last[index].is_some_and(|at| at.elapsed() < Duration::from_secs(60)) {
            self.suppressed[index] = self.suppressed[index].saturating_add(1);
            return;
        }
        self.last[index] = Some(Instant::now());
        let suppressed = std::mem::take(&mut self.suppressed[index]);
        let client = if canonical_uuid(client_id) {
            client_id
        } else {
            "withheld"
        };
        crate::logs::push(
            crate::logs::Source::Deas,
            &format!(
                "WARN: preview-observer operation={operation} client={client}: optional preview transport failed; authoritative job state is unchanged (suppressed={suppressed}, repeats limited to once per minute per operation)"
            ),
        );
    }
}

fn report_failure(failure: Failure, client_id: &str) {
    static REPORTS: Mutex<FailureLog> = Mutex::new(FailureLog {
        last: [None; 5],
        suppressed: [0; 5],
    });
    REPORTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .report(failure, client_id);
}

fn normal_close(error: &Error) -> bool {
    matches!(error, Error::ConnectionClosed | Error::AlreadyClosed)
}

/// Shared by Job clones through Live, never owned by the socket task itself.
pub(super) struct Observer {
    started: bool,
    cancel: watch::Sender<bool>,
    completed: watch::Sender<bool>,
}

impl Default for Observer {
    fn default() -> Self {
        let (cancel, _) = watch::channel(false);
        let (completed, _) = watch::channel(false);
        Self {
            started: false,
            cancel,
            completed,
        }
    }
}

impl Observer {
    fn begin(&mut self) -> Option<(watch::Receiver<bool>, Completion)> {
        if self.started || *self.cancel.borrow() {
            return None;
        }
        self.started = true;
        Some((self.cancel.subscribe(), Completion(self.completed.clone())))
    }

    /// Called while holding Live, including native terminal and Stop commits.
    pub(super) fn cancel(&self) {
        self.cancel.send_replace(true);
        if !self.started {
            // No connection or task has been created, so there is nothing to clean up.
            self.completed.send_replace(true);
        }
    }

    #[cfg(test)]
    pub(super) fn completion(&self) -> watch::Receiver<bool> {
        self.completed.subscribe()
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Does not retain the controller or Live. Completion means all socket work has ended.
struct Completion(watch::Sender<bool>);

impl Drop for Completion {
    fn drop(&mut self) {
        self.0.send_replace(true);
    }
}

async fn cancelled(signal: &mut watch::Receiver<bool>) {
    loop {
        if *signal.borrow() {
            return;
        }
        if signal.changed().await.is_err() {
            return;
        }
    }
}

/// Cancellation and publication take the same Live lock, so a cancelled observer
/// cannot accept a later frame even when receive and cancellation become ready together.
fn publish(
    live: &Weak<Mutex<Live>>,
    cancel: &watch::Receiver<bool>,
    update: impl FnOnce(&mut Live),
) -> bool {
    let Some(owner) = live.upgrade() else {
        return false;
    };
    let Ok(mut live) = owner.lock() else {
        return false;
    };
    if *cancel.borrow() {
        return false;
    }
    update(&mut live);
    true
}

/// Best-effort live view over `/ws?clientId=`; failures only mean no preview.
// Plain ws only, preserving the existing optional transport and its lack of reconnects.
pub(super) async fn watch(base: &reqwest::Url, client_id: &str, live: Arc<Mutex<Live>>) {
    let (expected, job_id) = live
        .lock()
        .map(|l| (l.preview_nodes.clone(), l.preview_job.clone()))
        .unwrap_or_default();
    let started = live.lock().ok().and_then(|mut live| live.observer.begin());
    let weak = Arc::downgrade(&live);
    // Neither connection work nor the spawned receiver keeps native ownership alive.
    drop(live);
    let Some((mut cancel, completion)) = started else {
        return;
    };
    // One proportional budget per observer; telemetry must not retain Live or delay cancellation.
    let budget = if expected.is_empty() {
        0
    } else {
        tokio::select! {
            biased;
            _ = cancelled(&mut cancel) => return,
            sample = crate::monitoring::sample(None) => sample.ram.map_or(0, |m| (m.total.saturating_sub(m.used) / 128) as usize),
        }
    };
    let Ok(mut url) = base.join("ws") else {
        return;
    };
    if base.scheme() != "http" || url.set_scheme("ws").is_err() {
        return;
    }
    url.query_pairs_mut().append_pair("clientId", client_id);
    let config = WebSocketConfig::default().max_message_size(Some(RESPONSE_LIMIT * 16));
    let connect = tokio_tungstenite::connect_async_with_config(url.as_str(), Some(config), false);
    let connected = tokio::select! {
        biased;
        _ = cancelled(&mut cancel) => return,
        result = tokio::time::timeout(Duration::from_secs(3), connect) => result,
    };
    let Ok(Ok((socket, _))) = connected else {
        if !*cancel.borrow() {
            report_failure(Failure::Connect, client_id);
        }
        return;
    };
    // Keep only a validated correlation string, never the endpoint or request contents.
    let client_id = if canonical_uuid(client_id) {
        client_id.to_owned()
    } else {
        String::new()
    };
    tokio::spawn(async move {
        let _completion = completion;
        // Locals drop in reverse order: release the socket before reporting completion,
        // including the peer-Close early return and cancellation paths.
        let mut socket = socket;
        // execution_start is directed to this unique client, unlike broadcast interruptions.
        let mut prompt_id: Option<String> = None;
        loop {
            let received = tokio::select! {
                biased;
                _ = cancelled(&mut cancel) => break,
                result = tokio::time::timeout(Duration::from_secs(600), socket.next()) => result,
            };
            let message = match received {
                Ok(Some(Ok(message))) => message,
                Ok(None) => break,
                Ok(Some(Err(ref error))) if normal_close(error) => break,
                _ => {
                    if !*cancel.borrow() {
                        report_failure(Failure::Receive, &client_id);
                    }
                    break;
                }
            };
            match message {
                Message::Binary(frame) => {
                    if preview_frame(&frame).is_some_and(|preview| {
                        !publish(&weak, &cancel, |live| live.preview = Some(preview))
                    }) {
                        break;
                    }
                }
                Message::Text(text) => {
                    let event: Value = serde_json::from_str(&text).unwrap_or_default();
                    match event["type"].as_str() {
                        Some("execution_start") if prompt_id.is_none() => {
                            prompt_id = event["data"]["prompt_id"]
                                .as_str()
                                .filter(|id| !id.is_empty())
                                .map(str::to_owned);
                        }
                        Some("kj_preview_override")
                            if prompt_id.is_some()
                                && event["data"]["node_id"]
                                    .as_str()
                                    .is_some_and(|id| expected.iter().any(|n| n == id)) =>
                        {
                            let revision = weak
                                .upgrade()
                                .and_then(|l| {
                                    l.lock().ok().map(|l| l.preview_revision.saturating_add(1))
                                })
                                .unwrap_or(1);
                            let data = event["data"].clone();
                            let job = job_id.clone();
                            // Decode without Live held, then recheck cancellation under the publication lock.
                            let decode = tokio::task::spawn_blocking(move || {
                                let temporal = super::preview::parse(&data, &job, revision, budget);
                                let still = if temporal.is_none() {
                                    super::preview::jpeg(&data, budget)
                                } else {
                                    None
                                };
                                (temporal, still)
                            });
                            let decoded = tokio::select! {
                                biased;
                                _ = cancelled(&mut cancel) => break,
                                result = decode => result,
                            };
                            match decoded {
                                Ok((Some(preview), _)) => {
                                    if !publish(&weak, &cancel, |live| {
                                        live.preview_revision = revision;
                                        live.temporal = Some(preview);
                                    }) {
                                        break;
                                    }
                                }
                                Ok((_, Some(still))) => {
                                    if !publish(&weak, &cancel, |live| live.preview = Some(still)) {
                                        break;
                                    }
                                }
                                _ => report_failure(Failure::Decode, &client_id),
                            }
                        }
                        Some("progress")
                            if event["data"]["prompt_id"].is_null()
                                || (prompt_id.is_some()
                                    && event["data"]["prompt_id"].as_str()
                                        == prompt_id.as_deref()) =>
                        {
                            if !publish(&weak, &cancel, |live| {
                                live.value = event["data"]["value"].as_u64().unwrap_or(0);
                                live.max = event["data"]["max"].as_u64().unwrap_or(0);
                            }) {
                                break;
                            }
                        }
                        Some("execution_success" | "execution_error" | "execution_interrupted")
                            if prompt_id.is_some()
                                && event["data"]["prompt_id"].as_str() == prompt_id.as_deref() =>
                        {
                            publish(&weak, &cancel, |live| live.temporal = None);
                            break;
                        }
                        _ => {}
                    }
                }
                Message::Close(_) => {
                    // Tungstenite queues the peer's Close reply. Cancellation can wake this
                    // flush too; the common bounded close path then finishes cleanup.
                    let flushed = tokio::select! {
                        biased;
                        _ = cancelled(&mut cancel) => None,
                        result = tokio::time::timeout(Duration::from_secs(1), socket.flush()) => Some(result),
                    };
                    if let Some(result) = flushed {
                        if !matches!(result, Ok(Ok(())))
                            && !matches!(result, Ok(Err(ref error)) if normal_close(error))
                        {
                            report_failure(Failure::Flush, &client_id);
                        }
                        return;
                    }
                    break;
                }
                Message::Ping(_) => {
                    // Tungstenite queues the matching Pong, including on an idle socket.
                    let flushed = tokio::select! {
                        biased;
                        _ = cancelled(&mut cancel) => break,
                        result = tokio::time::timeout(Duration::from_secs(1), socket.flush()) => result,
                    };
                    if !matches!(flushed, Ok(Ok(()))) {
                        if !matches!(flushed, Ok(Err(ref error)) if normal_close(error))
                            && !*cancel.borrow()
                        {
                            report_failure(Failure::Flush, &client_id);
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
        // A TCP drop races ComfyUI's queued sends; retain the graceful Close handshake.
        // Cancellation never blocks a native mutex and an uncooperative peer is bounded.
        let closed = tokio::time::timeout(Duration::from_secs(1), async {
            socket.close(None).await?;
            while let Some(message) = socket.next().await {
                if matches!(message?, Message::Close(_)) {
                    break;
                }
            }
            Ok::<_, tokio_tungstenite::tungstenite::Error>(())
        })
        .await;
        if !matches!(closed, Ok(Ok(())))
            && !matches!(closed, Ok(Err(ref error)) if normal_close(error))
        {
            report_failure(Failure::Close, &client_id);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_malformed_preview_logging_is_bounded_and_shared() {
        let mut reports = FailureLog {
            last: [None; 5],
            suppressed: [0; 5],
        };
        let id = uuid::Uuid::new_v4().to_string();
        reports.report(Failure::Decode, &id);
        reports.report(Failure::Decode, &id);
        let entries: Vec<_> = crate::logs::native_logs(0)
            .into_iter()
            .filter(|e| e.text.contains(&id))
            .collect();
        assert_eq!(entries.len(), 1);
        assert!(entries[0]
            .text
            .starts_with("WARN: preview-observer operation=decode"));
        assert_eq!(entries[0].source, crate::logs::Source::Deas);
        assert_eq!(reports.suppressed[4], 1);
    }
    #[test]
    fn watch_failure_logging_is_shared_redacted_and_rate_limited() {
        let mut reports = FailureLog {
            last: [None; 5],
            suppressed: [0; 5],
        };
        let client = uuid::Uuid::new_v4().to_string();
        reports.report(Failure::Connect, &client);
        reports.report(Failure::Connect, &client);
        reports.report(
            Failure::Receive,
            "https://user:secret@private.invalid/?token=secret",
        );
        assert_eq!(reports.suppressed[0], 1);
        let entries = crate::logs::native_logs(0);
        let own: Vec<_> = entries
            .iter()
            .filter(|entry| entry.text.contains(&format!("client={client}:")))
            .collect();
        assert_eq!(own.len(), 1);
        assert_eq!(own[0].source, crate::logs::Source::Deas);
        assert!(own[0]
            .text
            .starts_with("WARN: preview-observer operation=connect "));
        assert!(own[0].text.contains("suppressed=0"));
        assert!(entries.iter().any(|entry| {
            entry
                .text
                .starts_with("WARN: preview-observer operation=receive client=withheld:")
        }));
        assert!(
            entries
                .iter()
                .filter(|entry| entry.text.starts_with("WARN: preview-observer "))
                .all(|entry| !entry.text.contains("secret")
                    && !entry.text.contains("private.invalid"))
        );
    }
}
