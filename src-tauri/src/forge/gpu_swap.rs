//! One GPU, two models. Most people run the chat LLM and ComfyUI on the same card, and
//! neither lets go on its own: the LLM server keeps the writer loaded (Ollama for five
//! minutes by default) and ComfyUI keeps the last checkpoint cached. So a chat render
//! first asks the LLM server to unload, and a chat answer first frees an idle ComfyUI
//! when the LLM would not fit beside it. The server loads the model again on the next
//! request by itself.
//!
//! Whether the LLM is on *this* PC is measured, never guessed from the address: a
//! loopback URL can be a tunnel to another machine. The local card has to hold at least
//! the model's size before anything is unloaded, and the unload has to show up as freed
//! memory here; a server that does not is remembered as remote and left alone.

use crate::llm::{LlmConfig, Provider};
use reqwest::Client;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

/// Unknown model size (llama-swap does not report one): the smallest LLM worth unloading.
const UNKNOWN_SIZE: u64 = 3 << 30;
/// How long an unload may take to show up on the card.
const SETTLE: Duration = Duration::from_secs(12);

/// Servers whose unload freed nothing here: they run on another machine.
static REMOTE: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// The model this app unloaded last, so the next answer can say it is loading again.
static UNLOADED: Mutex<Option<String>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Ollama,
    LlamaSwap,
}

#[derive(Debug, Clone, PartialEq)]
struct Loaded {
    kind: Kind,
    /// (model name, bytes in VRAM if the server says).
    models: Vec<(String, Option<u64>)>,
}

impl Loaded {
    fn size(&self) -> u64 {
        self.models
            .iter()
            .map(|(_, b)| b.unwrap_or(UNKNOWN_SIZE))
            .sum()
    }
}

/// What a chat render's make-room step did, for the Logs and the page.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Freed {
    pub unloaded: Vec<String>,
    pub freed_bytes: u64,
    /// Why nothing was unloaded, when nothing was.
    pub skipped: Option<String>,
}

fn base(config: &LlmConfig) -> String {
    let b = config.base_url.trim().trim_end_matches('/');
    b.strip_suffix("/v1").unwrap_or(b).to_string()
}

fn remote(base: &str) -> bool {
    REMOTE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|s| s.contains(base))
}

fn client() -> Result<Client, String> {
    Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())
}

fn with_key(r: reqwest::RequestBuilder, config: &LlmConfig) -> reqwest::RequestBuilder {
    if config.api_key.trim().is_empty() {
        r
    } else {
        r.bearer_auth(config.api_key.trim())
    }
}

/// Ollama's `/api/ps` or llama-swap's `/running`; anything else cannot say what it holds.
fn parse_loaded(kind: Kind, body: &Value) -> Option<Loaded> {
    let models: Vec<(String, Option<u64>)> = match kind {
        Kind::Ollama => body["models"]
            .as_array()?
            .iter()
            .filter_map(|m| {
                let vram = m["size_vram"].as_u64();
                // A model held in system RAM only is not in the way of the card.
                let name = m["name"].as_str()?.to_string();
                (vram != Some(0)).then_some((name, vram))
            })
            .collect(),
        Kind::LlamaSwap => body["running"]
            .as_array()?
            .iter()
            .filter_map(|m| Some((m["model"].as_str()?.to_string(), None)))
            .collect(),
    };
    Some(Loaded { kind, models })
}

async fn loaded(client: &Client, config: &LlmConfig) -> Option<Loaded> {
    let base = base(config);
    let order: &[Kind] = if config.provider == Provider::Ollama {
        &[Kind::Ollama]
    } else {
        &[Kind::LlamaSwap, Kind::Ollama]
    };
    for &kind in order {
        let path = match kind {
            Kind::Ollama => "/api/ps",
            Kind::LlamaSwap => "/running",
        };
        let Ok(r) = with_key(client.get(format!("{base}{path}")), config)
            .send()
            .await
        else {
            continue;
        };
        if !r.status().is_success() {
            continue;
        }
        if let Some(found) = r
            .json::<Value>()
            .await
            .ok()
            .and_then(|b| parse_loaded(kind, &b))
        {
            return Some(found);
        }
    }
    None
}

async fn unload(client: &Client, config: &LlmConfig, kind: Kind, model: &str) -> bool {
    let base = base(config);
    let request = match kind {
        Kind::Ollama => client
            .post(format!("{base}/api/generate"))
            .json(&json!({ "model": model, "keep_alive": 0 })),
        Kind::LlamaSwap => client.post(format!(
            "{base}/api/models/unload/{}",
            model
                .replace('%', "%25")
                .replace('?', "%3F")
                .replace('#', "%23")
        )),
    };
    with_key(request, config)
        .send()
        .await
        .is_ok_and(|r| r.status().is_success())
}

/// VRAM used on this PC's first card, in bytes.
async fn used_here() -> Option<u64> {
    crate::monitoring::sample(None)
        .await
        .gpus
        .first()
        .and_then(|g| g.memory_used)
}

/// Before a chat render: unload the chat LLM if it sits on this PC's card.
pub async fn free_llm(config: &LlmConfig) -> Freed {
    free_llm_with(config, SETTLE, used_here).await
}

async fn free_llm_with<F, S>(config: &LlmConfig, settle: Duration, mut used: S) -> Freed
where
    F: std::future::Future<Output = Option<u64>>,
    S: FnMut() -> F,
{
    let skip = |why: &str| Freed {
        skipped: Some(why.into()),
        ..Default::default()
    };
    if config.provider == Provider::Local {
        // DEAS's own runner loads, answers and exits: nothing stays on the card.
        return skip("local runner holds nothing");
    }
    let server = base(config);
    if remote(&server) {
        return skip("LLM server runs on another machine");
    }
    let Ok(client) = client() else {
        return skip("no HTTP client");
    };
    let Some(found) = loaded(&client, config).await else {
        return skip("server cannot report loaded models");
    };
    if found.models.is_empty() {
        return skip("no model loaded");
    }
    let Some(before) = used().await else {
        return skip("no GPU telemetry on this PC");
    };
    let size = found.size();
    if before < size {
        // The card here holds less than the model: it cannot be on this card.
        return skip("loaded model is not on this PC's GPU");
    }
    let mut unloaded = vec![];
    for (name, _) in &found.models {
        if unload(&client, config, found.kind, name).await {
            unloaded.push(name.clone());
        }
    }
    if unloaded.is_empty() {
        return skip("server refused the unload");
    }
    let deadline = tokio::time::Instant::now() + settle;
    let mut freed = 0;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(400)).await;
        if let Some(now) = used().await {
            freed = before.saturating_sub(now);
            if freed >= size / 2 {
                break;
            }
        }
    }
    if freed < size / 2 {
        REMOTE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_or_insert_with(HashSet::new)
            .insert(server);
        crate::logs::push(
            crate::logs::Source::Deas,
            "INFO: Chat: the LLM server unloaded its model but no GPU memory came free on this PC; it runs elsewhere and is left loaded from now on",
        );
        return Freed {
            unloaded,
            freed_bytes: freed,
            skipped: Some("LLM server runs on another machine".into()),
        };
    }
    *UNLOADED.lock().unwrap_or_else(|e| e.into_inner()) = unloaded.first().cloned();
    crate::logs::push(
        crate::logs::Source::Deas,
        &format!(
            "INFO: Chat: unloaded the chat LLM to make room for the picture ({:.1} GB freed); it loads again with the next message",
            freed as f64 / (1u64 << 30) as f64
        ),
    );
    Freed {
        unloaded,
        freed_bytes: freed,
        skipped: None,
    }
}

/// True once after a chat render unloaded the LLM: the next answer starts with a load.
pub fn take_reloading() -> bool {
    UNLOADED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .is_some()
}

/// Before a chat answer on a local LLM server: let DEAS's own ComfyUI release an idle
/// checkpoint if the model would not fit beside it. Best effort; never blocks the answer.
pub async fn make_room_for_llm(config: &LlmConfig, model: &str) {
    // Only Ollama says how big a model is before loading it.
    if config.provider != Provider::Ollama || remote(&base(config)) {
        return;
    }
    let Some(need) = model_size(config, model).await else {
        return;
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let result = tokio::task::spawn_blocking(move || {
        crate::resources::admit(
            crate::resources::Need {
                ram_bytes: 0,
                vram_bytes: need,
                device_index: Some(0),
            },
            &cancel,
        )
    })
    .await;
    if let Ok(Err(e)) = result {
        crate::logs::push(
            crate::logs::Source::Deas,
            &format!("INFO: Chat: could not make room for the LLM before answering ({e}); answering anyway"),
        );
    }
}

/// What the model will take on the card, from its size in Ollama's list plus its context;
/// None when it is already loaded or the server cannot say.
async fn model_size(config: &LlmConfig, model: &str) -> Option<u64> {
    let client = client().ok()?;
    let base = base(config);
    if let Some(found) = loaded(&client, config).await {
        if found.models.iter().any(|(m, _)| m == model) {
            return None;
        }
    }
    let tags: Value = with_key(client.get(format!("{base}/api/tags")), config)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let size = tags["models"]
        .as_array()?
        .iter()
        .find(|m| m["name"].as_str() == Some(model) || m["model"].as_str() == Some(model))?["size"]
        .as_u64()?;
    // Measured 7 Nov 2026 on a 5080: a 6.1 GB 8B took 9.5 GB loaded with Ollama's context.
    Some(size + size / 2 + (512 << 20))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_each_server_holds_and_ignores_ram_only_models() {
        let ollama = json!({"models":[{"name":"qwen3-vl:8b-instruct","size_vram":7_000_000_000u64},{"name":"cpu-only","size_vram":0}]});
        let l = parse_loaded(Kind::Ollama, &ollama).unwrap();
        assert_eq!(
            l.models,
            vec![("qwen3-vl:8b-instruct".into(), Some(7_000_000_000))]
        );
        assert_eq!(l.size(), 7_000_000_000);
        let swap = json!({"running":[{"model":"hf.co/x:Q4_K_P","state":"ready"}]});
        let l = parse_loaded(Kind::LlamaSwap, &swap).unwrap();
        assert_eq!(l.size(), UNKNOWN_SIZE);
        assert!(parse_loaded(Kind::LlamaSwap, &json!({"data":[]})).is_none());
        assert_eq!(
            base(&LlmConfig {
                provider: Provider::Openai,
                base_url: "http://127.0.0.1:8099/v1/".into(),
                api_key: String::new()
            }),
            "http://127.0.0.1:8099"
        );
    }

    async fn server(running: Value) -> (String, tokio::sync::mpsc::Receiver<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = vec![0; 8192];
                let n = s.read(&mut buf).await.unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n])
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_string();
                let _ = tx.send(head.clone()).await;
                let body = if head.starts_with("GET /running") {
                    running.to_string()
                } else {
                    "{}".into()
                };
                let status = if head.starts_with("GET /api/ps") {
                    "404 Not Found"
                } else {
                    "200 OK"
                };
                let _ = s.write_all(format!("HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await;
            }
        });
        (url, rx)
    }

    #[tokio::test]
    async fn unloads_only_a_model_that_is_on_this_card_and_remembers_remote_servers() {
        let running = json!({"running":[{"model":"m","state":"ready"}]});
        // On this card: 8 GB used, 5 GB of it frees after the unload.
        let (url, mut rx) = server(running.clone()).await;
        let config = LlmConfig {
            provider: Provider::Openai,
            base_url: url,
            api_key: String::new(),
        };
        let mut readings = vec![8u64 << 30, 3 << 30].into_iter();
        let freed = free_llm_with(&config, Duration::from_secs(2), || {
            let v = readings.next().or(Some(3 << 30));
            async move { v }
        })
        .await;
        assert_eq!(freed.unloaded, vec!["m".to_string()]);
        assert!(freed.skipped.is_none());
        assert_eq!(freed.freed_bytes, 5 << 30);
        assert!(take_reloading());
        assert!(!take_reloading());
        let mut heads = vec![];
        while let Ok(h) = rx.try_recv() {
            heads.push(h);
        }
        assert!(
            heads
                .iter()
                .any(|h| h.starts_with("POST /api/models/unload/m")),
            "{heads:?}"
        );

        // The card here holds less than the model: never asked to unload.
        let (url, mut rx) = server(running.clone()).await;
        let config = LlmConfig {
            provider: Provider::Openai,
            base_url: url,
            api_key: String::new(),
        };
        let freed = free_llm_with(&config, Duration::from_millis(900), || async {
            Some(1u64 << 30)
        })
        .await;
        assert_eq!(
            freed.skipped.as_deref(),
            Some("loaded model is not on this PC's GPU")
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
        while let Ok(h) = rx.try_recv() {
            assert!(!h.starts_with("POST"), "{h}");
        }

        // Unloaded, but nothing came free here (a tunnel to another box): remembered, left alone.
        let (url, mut rx) = server(running).await;
        let config = LlmConfig {
            provider: Provider::Openai,
            base_url: url,
            api_key: String::new(),
        };
        let freed = free_llm_with(&config, Duration::from_millis(900), || async {
            Some(9u64 << 30)
        })
        .await;
        assert_eq!(
            freed.skipped.as_deref(),
            Some("LLM server runs on another machine")
        );
        while rx.try_recv().is_ok() {}
        let again = free_llm_with(&config, Duration::from_millis(900), || async {
            Some(9u64 << 30)
        })
        .await;
        assert_eq!(
            again.skipped.as_deref(),
            Some("LLM server runs on another machine")
        );
        assert!(again.unloaded.is_empty());
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(rx.try_recv().is_err());
    }
}
