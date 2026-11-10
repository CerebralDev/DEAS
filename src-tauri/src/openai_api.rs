//! Official OpenAI Responses transport. The compatible-server adapter stays unchanged.
use crate::llm::LlmConfig;
use serde_json::{json, Value};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};

const MAX_REPLY: usize = 4 * 1024 * 1024;
const MAX_REQUEST: usize = 32 * 1024 * 1024;

pub fn text_model(id: &str) -> bool {
    (id.starts_with("gpt-") || id.starts_with("o1") || id.starts_with("o3") || id.starts_with("o4"))
        && ![
            "image",
            "audio",
            "realtime",
            "live",
            "transcribe",
            "tts",
            "search",
            "deep-research",
            "codex",
            "instruct",
        ]
        .iter()
        .any(|part| id.contains(part))
}

/// Convert the existing internal chat contract, including image and tool turns.
/// Only allowlisted fields reach OpenAI; local-server sampling knobs never do.
pub fn request(chat: &Value) -> Result<Value, String> {
    let model = chat["model"]
        .as_str()
        .filter(|id| text_model(id))
        .ok_or("Select an OpenAI text/vision model")?;
    let messages = chat["messages"]
        .as_array()
        .ok_or("OpenAI messages are missing")?;
    let mut input = vec![];
    for message in messages {
        let role = message["role"]
            .as_str()
            .ok_or("Invalid OpenAI message role")?;
        if role == "tool" {
            input.push(json!({"type":"function_call_output", "call_id":message["tool_call_id"], "output":message["content"]}));
            continue;
        }
        if !matches!(role, "user" | "assistant" | "system" | "developer") {
            return Err("Unsupported OpenAI message role".into());
        }
        let mut content = vec![];
        if let Some(text) = message["content"].as_str().filter(|s| !s.is_empty()) {
            content.push(json!({"type":"input_text", "text":text}));
        } else if let Some(parts) = message["content"].as_array() {
            for part in parts {
                match part["type"].as_str() {
                    Some("text") => content.push(json!({"type":"input_text","text":part["text"]})),
                    Some("image_url") => {
                        let url = part["image_url"]["url"]
                            .as_str()
                            .filter(|s| s.starts_with("data:image/"))
                            .ok_or("OpenAI image must be a prepared image, not an external URL")?;
                        content.push(json!({"type":"input_image","image_url":url,"detail":"auto"}));
                    }
                    _ => return Err("Unsupported OpenAI content type".into()),
                }
            }
        }
        if !content.is_empty() {
            input.push(json!({"role":role,"content":content}));
        }
        if let Some(calls) = message["tool_calls"].as_array() {
            for call in calls {
                input.push(json!({"type":"function_call","call_id":call["id"],"name":call["function"]["name"],"arguments":call["function"]["arguments"]}));
            }
        }
    }
    let mut body = json!({"model":model,"input":input,"store":false,"stream":false,
        "max_output_tokens":chat["max_tokens"].as_u64().unwrap_or(4096).clamp(16,32768)});
    // These families support low reasoning. Avoid model-dependent temperature/top_p entirely.
    if model.starts_with("gpt-5")
        || model.starts_with("gpt-6")
        || model.starts_with("o3")
        || model.starts_with("o4")
    {
        // Pro variants have different effort ranges: retain their API default.
        if !model.contains("-pro") {
            body["reasoning"] = json!({"effort":"low"});
        }
        body["max_output_tokens"] = json!(body["max_output_tokens"].as_u64().unwrap().max(4096));
    }
    if let Some(tools) = chat["tools"].as_array().filter(|t| !t.is_empty()) {
        let mut converted = vec![];
        for tool in tools {
            if tool["type"] != "function" {
                return Err("Unsupported OpenAI tool type".into());
            }
            let mut function = tool["function"].clone();
            function["type"] = json!("function");
            function["strict"] = json!(false);
            converted.push(function);
        }
        body["tools"] = json!(converted);
    }
    Ok(body)
}

pub fn completion(response: &Value) -> Result<Value, String> {
    if response["status"] != "completed" || !response["error"].is_null() {
        return Err("OpenAI response did not complete. Original prompt retained; retry explicitly (a timed-out request may have been billed).".into());
    }
    let output = response["output"]
        .as_array()
        .ok_or("Invalid OpenAI output")?;
    let mut text = String::new();
    let mut calls = vec![];
    for item in output {
        match item["type"].as_str() {
            Some("message") => {
                for part in item["content"].as_array().ok_or("Invalid OpenAI message")? {
                    if part["type"] == "refusal" { return Err("OpenAI declined this request; revise it before retrying.".into()); }
                    if part["type"] == "output_text" { text.push_str(part["text"].as_str().ok_or("Invalid OpenAI text")?); }
                }
            }
            Some("function_call") => calls.push(json!({"id":item["call_id"],"type":"function","function":{"name":item["name"],"arguments":item["arguments"]}})),
            _ => (),
        }
    }
    if text.trim().is_empty() && calls.is_empty() {
        return Err("OpenAI returned no text or tool calls".into());
    }
    Ok(
        json!({"choices":[{"finish_reason":if calls.is_empty(){"stop"}else{"tool_calls"},"message":{"role":"assistant","content":text,"tool_calls":calls}}],
        "usage":{"prompt_tokens":response["usage"]["input_tokens"],"completion_tokens":response["usage"]["output_tokens"]}}),
    )
}

fn safe_token(value: &Value) -> String {
    value
        .as_str()
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "_.-[]".contains(*c))
        .take(100)
        .collect()
}

pub fn api_error(status: u16, body: &Value) -> String {
    // Provider messages can echo prompts or secrets. Report bounded code/field + actionable guidance instead.
    let reason = match status {
        401 => "API key rejected. Check the saved OpenAI API key.",
        403 => "Account or project lacks access to this model.",
        404 => "Model unavailable. Refresh models and select an accessible text/vision model.",
        429 => "Rate limit or API quota reached. Check billing/limits before an explicit retry.",
        400 => "Request rejected. Check the model and the indicated parameter.",
        500..=599 => "OpenAI service error. No automatic retry was sent.",
        _ => "OpenAI request failed. No automatic retry was sent.",
    };
    format!(
        "OpenAI HTTP {status}: {reason} code={} param={}",
        safe_token(&body["error"]["code"]),
        safe_token(&body["error"]["param"])
    )
}

pub async fn send(config: &LlmConfig, chat: &Value, timeout: Duration) -> Result<Value, String> {
    let body = request(chat)?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .build()
        .map_err(|_| "Cannot create OpenAI client")?;
    let result = async {
        let mut response = client.post(crate::llm::endpoint(config,"responses")?).bearer_auth(&config.api_key).json(&body).send().await
            .map_err(|_| "OpenAI transport failed or timed out; acceptance is unknown. No automatic retry was sent.")?;
        let status = response.status();
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| "OpenAI response interrupted; do not automatically replay this request")? {
            if bytes.len() + chunk.len() > MAX_REPLY { return Err("OpenAI response exceeds limit".into()); }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid OpenAI JSON response")?;
        if !status.is_success() { return Err(api_error(status.as_u16(), &value)); }
        completion(&value)
    }.await;
    if let Err(error) = &result {
        crate::logs::push(
            crate::logs::Source::Deas,
            &format!("ERROR: OpenAI operation=responses: {error}"),
        );
    }
    result
}

/// Per-operation authenticated loopback adapter for the node pack's public compatible API.
/// The cloud key remains here; Comfy receives only a disposable local bearer token.
pub struct Bridge {
    pub settings: Value,
    pub error: Arc<Mutex<Option<String>>>,
    task: tokio::task::JoinHandle<()>,
    identity: Option<(String, String, String)>,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}
type ActiveBridges = std::collections::HashMap<
    (String, String, String),
    (tokio::task::AbortHandle, Arc<std::sync::atomic::AtomicBool>),
>;
static ACTIVE: std::sync::LazyLock<Mutex<ActiveBridges>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));
pub fn cancel(base: &str, root: &str, id: &str) {
    if let Ok(active) = ACTIVE.lock() {
        if let Some((task, flag)) = active.get(&(base.into(), root.into(), id.into())) {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
            task.abort();
        }
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.task.abort();
        if let Some(key) = &self.identity {
            if let Ok(mut active) = ACTIVE.lock() {
                active.remove(key);
            }
        }
    }
}
impl Bridge {
    pub fn register(&mut self, base: &str, root: &str, id: &str) -> Result<(), String> {
        let key = (base.into(), root.into(), id.into());
        let mut active = ACTIVE
            .lock()
            .map_err(|_| "OpenAI operation lock unavailable")?;
        if active.contains_key(&key) {
            return Err("OpenAI request is already running".into());
        }
        active.insert(
            key.clone(),
            (self.task.abort_handle(), self.cancelled.clone()),
        );
        self.identity = Some(key);
        Ok(())
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub async fn start(config: LlmConfig) -> Result<Self, String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| "Cannot start OpenAI bridge")?;
        let address = listener
            .local_addr()
            .map_err(|_| "Cannot read bridge address")?;
        let token = uuid::Uuid::new_v4().to_string();
        let settings = json!({"openai_url":format!("http://{address}/v1"),"openai_api_key":token});
        let error = Arc::new(Mutex::new(None));
        let last_error = error.clone();
        let task = tokio::spawn(async move {
            // Bounded lifetime and sequential requests: no unbounded spawned cloud work.
            for _ in 0..64 {
                let Ok(Ok((socket, _))) =
                    tokio::time::timeout(Duration::from_secs(900), listener.accept()).await
                else {
                    break;
                };
                let result = tokio::time::timeout(
                    Duration::from_secs(600),
                    serve(socket, &token, &config, &last_error),
                )
                .await;
                if let Ok(Err(e)) = result {
                    if let Ok(mut last) = last_error.lock() {
                        *last = Some(e);
                    }
                }
            }
        });
        Ok(Self {
            settings,
            error,
            task,
            identity: None,
            cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }
}

async fn serve(
    socket: tokio::net::TcpStream,
    token: &str,
    config: &LlmConfig,
    last_error: &Mutex<Option<String>>,
) -> Result<(), String> {
    let mut stream = BufReader::new(socket);
    let mut header = vec![];
    // Read headers one byte at a time, bounded even without a newline.
    while !header.ends_with(b"\r\n\r\n") {
        if header.len() >= 16384 {
            return Err("OpenAI bridge headers too large".into());
        }
        header.push(stream.read_u8().await.map_err(|_| "Bridge disconnected")?);
    }
    let header = String::from_utf8(header).map_err(|_| "Invalid bridge headers")?;
    let mut lines = header.lines();
    let route = lines.next().unwrap_or("");
    let headers: Vec<_> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_owned()))
        .collect();
    let authorized = headers
        .iter()
        .any(|(k, v)| k == "authorization" && v == &format!("Bearer {token}"));
    if !authorized {
        stream
            .write_all(
                b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .ok();
        return Ok(());
    }
    let length: usize = headers
        .iter()
        .find(|(k, _)| k == "content-length")
        .map(|(_, v)| v.parse())
        .transpose()
        .map_err(|_| "Invalid bridge length")?
        .unwrap_or(0);
    if length > MAX_REQUEST || headers.iter().any(|(k, _)| k == "transfer-encoding") {
        return Err("OpenAI bridge request exceeds bounds".into());
    }
    let mut bytes = vec![0; length];
    stream
        .read_exact(&mut bytes)
        .await
        .map_err(|_| "Incomplete bridge request")?;
    let mut mime = "application/json";
    let result: Result<String, String> = if route.starts_with("GET /v1/models ") {
        crate::llm::remote_models(config).await.map(|models| {
            json!({"data":models.into_iter().map(|m|json!({"id":m.id})).collect::<Vec<_>>()})
                .to_string()
        })
    } else if route.starts_with("POST /v1/chat/completions ") {
        let chat: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid bridge JSON")?;
        match send(config, &chat, Duration::from_secs(540)).await {
            Ok(value) => {
                if chat["stream"] == true {
                    mime = "text/event-stream";
                    let chunk = json!({"choices":[{"index":0,"delta":value["choices"][0]["message"],"finish_reason":"stop"}],"usage":value["usage"]});
                    Ok(format!("data: {chunk}\n\ndata: [DONE]\n\n"))
                } else {
                    Ok(value.to_string())
                }
            }
            Err(e) => Err(e),
        }
    } else if route.starts_with("POST /api/models/unload/") {
        Ok("{}".into())
    } else {
        Err("Unsupported OpenAI bridge route".into())
    };
    let status = if result.is_ok() { 200 } else { 502 };
    if let Err(e) = &result {
        if let Ok(mut last) = last_error.lock() {
            *last = Some(e.clone());
        }
    }
    let body = match &result {
        Ok(body) => body.clone(),
        Err(e) => json!({"error":{"message":e}}).to_string(),
    };
    let reply = format!("HTTP/1.1 {status} Result\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
    stream
        .write_all(reply.as_bytes())
        .await
        .map_err(|_| "Bridge response disconnected")?;
    result.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::Provider;
    fn chat() -> Value {
        json!({"model":"gpt-6.1-terra","max_tokens":64,"temperature":0.7,"top_p":0.8,"chat_template_kwargs":{"enable_thinking":false},"messages":[{"role":"system","content":"Preserve exact H3 dialogue"},{"role":"user","content":[{"type":"text","text":"Singer at microphone"},{"type":"image_url","image_url":{"url":"data:image/jpeg;base64,fixture"}}]}]})
    }
    fn success() -> Value {
        json!({"status":"completed","output":[{"type":"reasoning"},{"type":"message","role":"assistant","content":[{"type":"output_text","text":"First shot: Singer sings."}]}],"usage":{"input_tokens":10,"output_tokens":20}})
    }
    #[test]
    fn openai_payload_preserves_images_instructions_and_excludes_local_options() {
        let b = request(&chat()).unwrap();
        assert_eq!(
            b["input"][0]["content"][0]["text"],
            "Preserve exact H3 dialogue"
        );
        assert_eq!(b["input"][1]["content"][1]["type"], "input_image");
        assert_eq!(b["store"], false);
        assert_eq!(b["reasoning"]["effort"], "low");
        assert_eq!(b["max_output_tokens"], 4096);
        for key in [
            "max_tokens",
            "temperature",
            "top_p",
            "presence_penalty",
            "chat_template_kwargs",
        ] {
            assert!(b.get(key).is_none());
        }
        let mut legacy = chat();
        legacy["model"] = json!("gpt-4.1");
        assert!(request(&legacy).unwrap().get("reasoning").is_none());
        legacy["model"] = json!("gpt-image-1");
        assert!(request(&legacy).is_err());
        legacy["model"] = json!("gpt-live-1");
        assert!(request(&legacy).is_err());
        legacy["model"] = json!("gpt-5-pro");
        assert!(request(&legacy).unwrap().get("reasoning").is_none());
    }
    #[test]
    fn openai_tool_turns_roundtrip_without_executing_anything() {
        let history=request(&json!({"model":"gpt-6.1-terra","messages":[{"role":"assistant","content":"Previous draft"},{"role":"user","content":"Revise it"}]})).unwrap();
        assert_eq!(history["input"][0]["content"][0]["type"], "input_text");
        let c = json!({"model":"gpt-6.1-terra","messages":[{"role":"assistant","tool_calls":[{"id":"call1","function":{"name":"read_guidance","arguments":"{}"}}]},{"role":"tool","tool_call_id":"call1","content":"guidance"}],"tools":[{"type":"function","function":{"name":"read_guidance","parameters":{"type":"object"}}}]});
        let b = request(&c).unwrap();
        assert_eq!(b["input"][0]["type"], "function_call");
        assert_eq!(b["input"][1]["call_id"], "call1");
        assert_eq!(b["tools"][0]["strict"], false);
        let v=completion(&json!({"status":"completed","output":[{"type":"function_call","call_id":"call1","name":"read_guidance","arguments":"{}"}]})).unwrap();
        assert_eq!(v["choices"][0]["message"]["tool_calls"][0]["id"], "call1");
    }
    #[test]
    fn openai_incomplete_refused_and_empty_are_not_usable_drafts() {
        let mut v = success();
        v["status"] = json!("incomplete");
        assert!(completion(&v).is_err());
        v = success();
        v["output"][1]["content"][0] = json!({"type":"refusal","refusal":"no"});
        assert!(completion(&v).is_err());
        v = json!({"status":"completed","output":[]});
        assert!(completion(&v).is_err());
        assert_eq!(
            completion(&success()).unwrap()["choices"][0]["message"]["content"],
            "First shot: Singer sings."
        );
    }
    #[test]
    fn openai_error_redaction_and_endpoint_boundary() {
        let error = api_error(
            400,
            &json!({"error":{"message":"secret-key and private prompt","code":"unsupported_parameter","param":"temperature"}}),
        );
        assert!(error.contains("temperature"));
        assert!(!error.contains("secret-key"));
        assert!(!error.contains("private prompt"));
        for url in [
            "https://evil.test/v1",
            "http://api.openai.com/v1",
            "https://api.openai.com/v1/responses",
            "https://api.openai.com:444/v1",
        ] {
            assert!(crate::llm::endpoint(
                &LlmConfig {
                    provider: Provider::OpenaiApi,
                    base_url: url.into(),
                    api_key: "fixture".into()
                },
                "responses"
            )
            .is_err());
        }
        assert_eq!(
            crate::llm::endpoint(
                &LlmConfig {
                    provider: Provider::OpenaiApi,
                    base_url: "https://api.openai.com/v1".into(),
                    api_key: "fixture".into()
                },
                "responses"
            )
            .unwrap()
            .as_str(),
            "https://api.openai.com/v1/responses"
        );
    }
    async fn fixture(response: Value, status: u16) -> (LlmConfig, tokio::task::JoinHandle<Value>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = BufReader::new(socket);
            let mut headers = vec![];
            while !headers.ends_with(b"\r\n\r\n") {
                headers.push(socket.read_u8().await.unwrap());
            }
            let headers = String::from_utf8(headers).unwrap();
            assert!(headers.starts_with("POST /v1/responses "));
            assert!(headers
                .to_lowercase()
                .contains("authorization: bearer fixture-key"));
            let length = headers
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length: ")
                        .map(|n| n.parse::<usize>().unwrap())
                })
                .unwrap();
            let mut data = vec![0; length];
            socket.read_exact(&mut data).await.unwrap();
            let body = response.to_string();
            let reply = format!(
                "HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(reply.as_bytes()).await.unwrap();
            serde_json::from_slice(&data).unwrap()
        });
        (
            LlmConfig {
                provider: Provider::Openai,
                base_url: format!("http://{address}/v1"),
                api_key: "fixture-key".into(),
            },
            task,
        )
    }
    #[tokio::test]
    async fn openai_http_and_nodepack_bridge_send_exact_response_payload_once() {
        let (config, server) = fixture(success(), 200).await;
        let bridge = Bridge::start(config).await.unwrap();
        let client = reqwest::Client::new();
        let url = format!(
            "{}/chat/completions",
            bridge.settings["openai_url"].as_str().unwrap()
        );
        let unauthorized = client.post(&url).json(&chat()).send().await.unwrap();
        assert_eq!(unauthorized.status(), 401);
        let mut body = chat();
        body["stream"] = json!(true);
        let response = client
            .post(&url)
            .bearer_auth(bridge.settings["openai_api_key"].as_str().unwrap())
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let sse = response.text().await.unwrap();
        assert!(sse.contains("First shot: Singer sings."));
        assert!(sse.contains("[DONE]"));
        let outgoing = server.await.unwrap();
        assert!(outgoing.get("chat_template_kwargs").is_none());
        assert_eq!(outgoing["input"][1]["content"][1]["type"], "input_image");
        drop(bridge);
        assert!(client.post(url).send().await.is_err());
    }
    #[tokio::test]
    async fn openai_http_errors_are_reported_without_body_or_replay() {
        let (config,server)=fixture(json!({"error":{"message":"private secret","code":"unsupported_parameter","param":"temperature"}}),400).await;
        let error = send(&config, &chat(), Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(error.contains("HTTP 400"));
        assert!(!error.contains("private"));
        server.await.unwrap();
    }
    #[tokio::test]
    async fn openai_bridge_cancel_is_scoped_and_duplicate_registration_is_blocked() {
        let config = LlmConfig {
            provider: Provider::OpenaiApi,
            base_url: "https://api.openai.com/v1".into(),
            api_key: "fixture".into(),
        };
        let mut first = Bridge::start(config.clone()).await.unwrap();
        first.register("backend", "root", "request").unwrap();
        let mut second = Bridge::start(config).await.unwrap();
        assert!(second.register("backend", "root", "request").is_err());
        cancel("other", "root", "request");
        assert!(!first.is_cancelled());
        cancel("backend", "root", "request");
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());
        drop(first);
        second.register("backend", "root", "request").unwrap();
    }
}
