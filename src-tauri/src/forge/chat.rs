//! One chat round with tools against the configured LLM server — Ollama's
//! native API or an OpenAI-compatible one (llama.cpp, llama-swap, vLLM).
//! Thinking is switched off the way PromptForge does it, because a prompt
//! rewrite gains nothing from reasoning but latency. A `local:` model runs the
//! same round through the managed GGUF worker instead.

use crate::llm::{LlmConfig, LocalRunner, Provider};
use serde_json::{Value, json};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

const LOCAL_PREFIX: &str = "local:";
tokio::task_local! {
    static RUNNER: Arc<LocalRunner>;
}

/// The local model of everything `work` writes stays loaded until it ends.
pub async fn with_local_runner<T>(root: &std::path::Path, work: impl Future<Output = T>) -> T {
    RUNNER.scope(Arc::new(LocalRunner::new(root)), work).await
}

pub fn is_local(model: &str) -> bool {
    model.starts_with(LOCAL_PREFIX)
}

const MAX_RESPONSE: usize = 4 * 1024 * 1024;
const MAX_TOOL_CALLS: usize = 128;
/// Ollama loads context at request time; PromptForge runs prompts at 32k.
const OLLAMA_NUM_CTX: u32 = 32_768;

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: Option<String>,
    pub name: String,
    /// As sent: a JSON string (OpenAI) or an object (Ollama).
    pub arguments: Value,
}

#[derive(Debug)]
pub struct Reply {
    pub content: String,
    pub calls: Vec<ToolCall>,
    pub truncated: bool,
}

pub struct Round<'a> {
    pub config: &'a LlmConfig,
    pub model: &'a str,
    pub messages: &'a [Value],
    pub tools: Option<&'a [Value]>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    /// A hard ceiling on the answer (num_predict / max_tokens).
    pub max_tokens: Option<u32>,
    pub timeout: Duration,
}

/// The request for a round: endpoint, body and which wire shape answers.
fn request(r: &Round<'_>, stream: bool) -> Result<(reqwest::Url, Value, bool), String> {
    let ollama = match r.config.provider {
        Provider::Ollama => true,
        Provider::Openai => false,
        Provider::Local => {
            return Err(
                "No LLM server is set: pick a local GGUF, or set a server under Backend → LLM."
                    .into(),
            );
        }
    };
    let mut body = if ollama {
        let mut options = json!({ "num_ctx": OLLAMA_NUM_CTX });
        if let Some(t) = r.temperature {
            options["temperature"] = json!(t);
        }
        if let Some(p) = r.top_p {
            options["top_p"] = json!(p);
        }
        if let Some(n) = r.max_tokens {
            options["num_predict"] = json!(n);
        }
        json!({ "model": r.model, "messages": r.messages, "stream": stream, "think": false, "options": options })
    } else {
        let mut b = json!({ "model": r.model, "messages": r.messages, "stream": stream, "chat_template_kwargs": { "enable_thinking": false } });
        if let Some(t) = r.temperature {
            b["temperature"] = json!(t);
        }
        if let Some(p) = r.top_p {
            b["top_p"] = json!(p);
        }
        if let Some(n) = r.max_tokens {
            b["max_tokens"] = json!(n);
        }
        b
    };
    if let Some(tools) = r.tools.filter(|t| !t.is_empty()) {
        body["tools"] = json!(tools);
    }
    let url = crate::llm::endpoint(
        r.config,
        if ollama {
            "api/chat"
        } else {
            "chat/completions"
        },
    )?;
    Ok((url, body, ollama))
}

async fn post(
    config: &LlmConfig,
    url: reqwest::Url,
    body: &Value,
    timeout: Duration,
) -> Result<reqwest::Response, String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .build()
        .map_err(|_| "Cannot create LLM HTTP client")?;
    let mut builder = client.post(url).json(body);
    // OpenAI-compatible servers need the saved key; Ollama and an empty key stay unauthenticated.
    if config.provider == Provider::Openai && !config.api_key.is_empty() {
        builder = builder.bearer_auth(&config.api_key);
    }
    builder.send().await.map_err(|e| {
        if e.is_timeout() {
            "The LLM took too long to answer".to_string()
        } else {
            "LLM server unavailable or transport failed".to_string()
        }
    })
}

fn http_error(status: reqwest::StatusCode, bytes: &[u8]) -> String {
    let detail = String::from_utf8_lossy(bytes);
    let detail: String = detail.chars().take(300).collect();
    format!(
        "LLM server returned HTTP {}: {}",
        status.as_u16(),
        detail.trim()
    )
}

/// Gemma's `{query:<|"|>red kimono<|"|>,limit:3}`, as JSON: its string quote is a token and its keys are bare.
fn gemma_arguments(body: &str) -> Option<Value> {
    let mut json = String::new();
    for (index, part) in body.split("<|\"|>").enumerate() {
        if index % 2 == 1 {
            json.push_str(&Value::String(part.to_string()).to_string());
            continue;
        }
        let mut word = String::new();
        for c in part.chars() {
            if c.is_alphanumeric() || c == '_' {
                word.push(c);
                continue;
            }
            if c == ':' && !word.is_empty() {
                json.push_str(&Value::String(std::mem::take(&mut word)).to_string());
            } else {
                json.push_str(&std::mem::take(&mut word));
            }
            json.push(c);
        }
        json.push_str(&word);
    }
    serde_json::from_str(&json).ok()
}

/// One call's name and arguments, in whichever shape this model's template taught it.
fn local_call(raw: &str) -> Option<(String, Value)> {
    let raw = raw.trim();
    if let Ok(call) = serde_json::from_str::<Value>(raw) {
        return Some((
            call["name"].as_str()?.to_string(),
            call["arguments"].clone(),
        ));
    }
    if let Some(rest) = raw.strip_prefix("<function=") {
        let (name, mut rest) = rest.split_once('>')?;
        let mut arguments = serde_json::Map::new();
        while let Some((_, after)) = rest.split_once("<parameter=") {
            let (key, after) = after.split_once('>')?;
            let (value, tail) = after.split_once("</parameter>").unwrap_or((after, ""));
            let value = value.trim();
            let value = serde_json::from_str(value)
                .ok()
                .filter(|v: &Value| !v.is_string())
                .unwrap_or_else(|| json!(value));
            arguments.insert(key.to_string(), value);
            rest = tail;
        }
        return Some((name.trim().to_string(), Value::Object(arguments)));
    }
    let (name, body) = raw.strip_prefix("call:")?.split_once('{')?;
    let body = body.rsplit_once('}')?.0;
    Some((
        name.trim().to_string(),
        gemma_arguments(&format!("{{{body}}}"))?,
    ))
}

/// `<think>` blocks and tool calls come back as plain text from a local model.
fn local_reply(text: &str, finish: Option<&str>) -> Reply {
    let mut rest = text;
    // Gemma closes its (empty, with thinking off) channel its own way.
    for close in ["</think>", "<channel|>"] {
        if let Some((_, after)) = rest.rsplit_once(close) {
            rest = after;
        }
    }
    let mut content = String::new();
    let mut calls = vec![];
    loop {
        let plain = rest.find("<tool_call>");
        let gemma = rest.find("<|tool_call>");
        let (start, open, close) = match (plain, gemma) {
            (Some(p), Some(g)) if g < p => (g, "<|tool_call>", "<tool_call|>"),
            (None, Some(g)) => (g, "<|tool_call>", "<tool_call|>"),
            (Some(p), _) => (p, "<tool_call>", "</tool_call>"),
            (None, None) => break,
        };
        content.push_str(&rest[..start]);
        let after = &rest[start + open.len()..];
        let (raw, tail) = after.split_once(close).unwrap_or((after, ""));
        rest = tail;
        let Some((name, arguments)) = local_call(raw).filter(|_| calls.len() < MAX_TOOL_CALLS)
        else {
            continue;
        };
        calls.push(ToolCall {
            id: Some(format!("call_{}", calls.len())),
            name,
            arguments: json!(arguments.to_string()),
        });
    }
    content.push_str(rest);
    Reply {
        content: content.trim().to_string(),
        calls,
        truncated: finish == Some("length"),
    }
}

async fn local_round(r: &Round<'_>) -> Result<Reply, String> {
    let model = &r.model[LOCAL_PREFIX.len()..];
    if r.messages.iter().any(|m| m["content"].is_array()) {
        return Err("The local GGUF runner cannot read pictures.".into());
    }
    let runner = RUNNER
        .try_with(Arc::clone)
        .map_err(|_| "Local GGUF models are not available here")?;
    let request = json!({
        "messages": r.messages,
        "tools": r.tools.filter(|t| !t.is_empty()),
        "temperature": r.temperature,
        "top_p": r.top_p,
        "max_tokens": r.max_tokens,
    });
    let (text, finish) = runner.round(model, request.clone(), r.timeout).await?;
    let reply = local_reply(&text, finish.as_deref());
    // A model that ignores or fumbles its tools still writes when asked without them.
    if r.tools.is_some() && reply.calls.is_empty() && !reply.content.contains("===SEGMENT:") {
        let mut plain = request;
        plain["tools"] = Value::Null;
        let (text, finish) = runner.round(model, plain, r.timeout).await?;
        return Ok(local_reply(&text, finish.as_deref()));
    }
    Ok(reply)
}

pub async fn round(r: Round<'_>) -> Result<Reply, String> {
    if is_local(r.model) {
        return local_round(&r).await;
    }
    let (url, body, ollama) = request(&r, false)?;
    let mut response = post(r.config, url, &body, r.timeout).await?;
    let status = response.status();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "LLM response failed")? {
        if bytes.len() + chunk.len() > MAX_RESPONSE {
            return Err("LLM response exceeds byte limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    if !status.is_success() {
        return Err(http_error(status, &bytes));
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "LLM server returned invalid JSON")?;
    parse(&value, ollama)
}

/// A round whose text arrives as it is written: `on_text` gets each piece, and
/// `stop` ends it early (what has arrived so far is the reply).
pub async fn stream_round(
    r: Round<'_>,
    on_text: &mut (dyn FnMut(&str) + Send),
    stop: &std::sync::atomic::AtomicBool,
) -> Result<Reply, String> {
    let (url, body, ollama) = request(&r, true)?;
    let mut parser = StreamParser::new(ollama);
    let receive = async {
        let mut response = post(r.config, url, &body, r.timeout).await?;
        let status = response.status();
        if !status.is_success() {
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| "LLM response failed")? {
                let take = chunk.len().min(4096 - bytes.len());
                bytes.extend_from_slice(&chunk[..take]);
                if bytes.len() == 4096 {
                    break;
                }
            }
            return Err(http_error(status, &bytes));
        }
        let mut total = 0usize;
        while let Some(chunk) = response.chunk().await.map_err(|_| "LLM response failed")? {
            total += chunk.len();
            if total > MAX_RESPONSE {
                return Err("LLM response exceeds byte limit".into());
            }
            for piece in parser.feed(&chunk) {
                on_text(&piece);
            }
            if let Some(error) = &parser.error {
                return Err(error.clone());
            }
        }
        Ok(())
    };
    tokio::select! {
        biased;
        _ = cancelled(stop) => parser.stopped = true,
        result = receive => result?,
    }
    parser.finish()
}

/// Poll the existing IPC stop flag while network futures are pending.
pub(super) async fn cancelled(stop: &std::sync::atomic::AtomicBool) {
    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Assembles a streamed answer: OpenAI's SSE `data:` lines with `delta`s (tool
/// calls arrive in pieces by index), or Ollama's NDJSON with whole messages.
pub struct StreamParser {
    ollama: bool,
    pending: Vec<u8>,
    content: String,
    calls: Vec<(Option<String>, String, String, Option<Value>)>,
    finish: Option<String>,
    pub stopped: bool,
    saw_any: bool,
    error: Option<String>,
}

impl StreamParser {
    pub fn new(ollama: bool) -> Self {
        Self {
            ollama,
            pending: vec![],
            content: String::new(),
            calls: vec![],
            finish: None,
            stopped: false,
            saw_any: false,
            error: None,
        }
    }

    /// The text pieces in this chunk, in order.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<String> {
        if self.error.is_some() {
            return vec![];
        }
        self.pending.extend_from_slice(chunk);
        let mut out = vec![];
        while let Some(at) = self.pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=at).collect();
            let line = String::from_utf8_lossy(&line).trim().to_string();
            if let Some(piece) = self.line(&line) {
                out.push(piece);
            }
        }
        out
    }

    fn line(&mut self, line: &str) -> Option<String> {
        if self.error.is_some() {
            return None;
        }
        let json = if self.ollama {
            line
        } else {
            let data = line.strip_prefix("data:")?.trim();
            if data == "[DONE]" {
                return None;
            }
            data
        };
        let v: Value = serde_json::from_str(json).ok()?;
        self.saw_any = true;
        if self.ollama {
            if let Some(error) = v["error"].as_str() {
                self.error = Some(format!(
                    "LLM server error: {}",
                    error.chars().take(300).collect::<String>()
                ));
                return None;
            }
            if let Some(calls) = v["message"]["tool_calls"].as_array() {
                for c in calls {
                    let f = &c["function"];
                    if let Some(name) = f["name"].as_str() {
                        if self.calls.len() >= MAX_TOOL_CALLS {
                            self.error = Some("LLM tool call count exceeds limit".into());
                            return None;
                        }
                        self.calls.push((
                            c["id"].as_str().map(String::from),
                            name.to_string(),
                            String::new(),
                            Some(f["arguments"].clone()),
                        ));
                    }
                }
            }
            if let Some(reason) = v["done_reason"].as_str() {
                self.finish = Some(reason.to_string());
            }
            let text = v["message"]["content"].as_str().unwrap_or("");
            if text.is_empty() {
                return None;
            }
            self.content.push_str(text);
            return Some(text.to_string());
        }
        let choice = &v["choices"][0];
        if let Some(reason) = choice["finish_reason"].as_str() {
            self.finish = Some(reason.to_string());
        }
        let delta = &choice["delta"];
        if let Some(calls) = delta["tool_calls"].as_array() {
            for c in calls {
                let index = match c.get("index") {
                    None => self.calls.len().saturating_sub(1),
                    Some(value) => match value.as_u64().and_then(|n| usize::try_from(n).ok()) {
                        Some(n) if n < MAX_TOOL_CALLS => n,
                        _ => {
                            self.error =
                                Some("LLM tool call index exceeds limit or is invalid".into());
                            return None;
                        }
                    },
                };
                while self.calls.len() <= index {
                    self.calls.push((None, String::new(), String::new(), None));
                }
                let slot = &mut self.calls[index];
                if let Some(id) = c["id"].as_str() {
                    slot.0 = Some(id.to_string());
                }
                if let Some(name) = c["function"]["name"].as_str() {
                    slot.1.push_str(name);
                }
                match &c["function"]["arguments"] {
                    Value::String(a) => slot.2.push_str(a),
                    Value::Null => {}
                    other => slot.3 = Some(other.clone()),
                }
            }
        }
        let text = delta["content"].as_str().unwrap_or("");
        if text.is_empty() {
            return None;
        }
        self.content.push_str(text);
        Some(text.to_string())
    }

    pub fn finish(mut self) -> Result<Reply, String> {
        if !self.pending.is_empty() {
            let rest = String::from_utf8_lossy(&std::mem::take(&mut self.pending))
                .trim()
                .to_string();
            self.line(&rest);
        }
        if let Some(error) = self.error {
            return Err(error);
        }
        if !self.saw_any && !self.stopped {
            return Err("LLM server returned no message".into());
        }
        let calls = self
            .calls
            .into_iter()
            .filter(|c| !c.1.is_empty())
            .map(|(id, name, args, whole)| ToolCall {
                id,
                name,
                arguments: whole.unwrap_or(Value::String(args)),
            })
            .collect();
        Ok(Reply {
            content: self.content,
            calls,
            truncated: self.finish.as_deref() == Some("length"),
        })
    }
}

fn parse(value: &Value, ollama: bool) -> Result<Reply, String> {
    let (message, finish) = if ollama {
        (&value["message"], value["done_reason"].as_str())
    } else {
        (
            &value["choices"][0]["message"],
            value["choices"][0]["finish_reason"].as_str(),
        )
    };
    if !message.is_object() {
        return Err("LLM server returned no message".into());
    }
    let content = message["content"].as_str().unwrap_or("").to_string();
    let calls = message["tool_calls"]
        .as_array()
        .map(|calls| {
            calls
                .iter()
                .filter_map(|c| {
                    let f = &c["function"];
                    let name = f["name"].as_str()?.to_string();
                    Some(ToolCall {
                        id: c["id"].as_str().map(String::from),
                        name,
                        arguments: f["arguments"].clone(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Reply {
        content,
        calls,
        truncated: finish == Some("length"),
    })
}

/// A user turn with pictures (data URIs): content parts for an OpenAI-compatible
/// server, an `images` list of bare base64 for Ollama.
pub fn user_with_images(text: &str, images: &[String], ollama: bool) -> Value {
    if images.is_empty() {
        return json!({ "role": "user", "content": text });
    }
    if ollama {
        let bare: Vec<&str> = images
            .iter()
            .map(|u| u.split_once(",").map_or(u.as_str(), |(_, b)| b))
            .collect();
        return json!({ "role": "user", "content": text, "images": bare });
    }
    let mut parts = vec![json!({ "type": "text", "text": text })];
    parts.extend(
        images
            .iter()
            .map(|u| json!({ "type": "image_url", "image_url": { "url": u } })),
    );
    json!({ "role": "user", "content": parts })
}

/// The model's own tool turn, echoed back before its results.
pub fn assistant_turn(content: &str, calls: &[ToolCall], ollama: bool) -> Value {
    let calls: Vec<Value> = calls
        .iter()
        .map(|c| {
            // Ollama's template walks an object; the OpenAI wire shape is a string.
            let args = if ollama {
                match &c.arguments {
                    Value::String(s) => serde_json::from_str::<Value>(s)
                        .ok()
                        .filter(Value::is_object)
                        .unwrap_or(json!({})),
                    Value::Object(_) => c.arguments.clone(),
                    _ => json!({}),
                }
            } else {
                match &c.arguments {
                    Value::String(s) => json!(s),
                    other => json!(other.to_string()),
                }
            };
            let mut call =
                json!({ "type": "function", "function": { "name": c.name, "arguments": args } });
            if let Some(id) = &c.id {
                call["id"] = json!(id);
            }
            call
        })
        .collect();
    json!({ "role": "assistant", "content": content, "tool_calls": calls })
}

pub fn tool_result(call: &ToolCall, content: &str) -> Value {
    let mut msg = json!({ "role": "tool", "content": content, "tool_name": call.name });
    if let Some(id) = &call.id {
        msg["tool_call_id"] = json!(id);
    }
    msg
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn local_reply_lifts_tool_calls_and_thinking_out_of_the_text() {
        let reply = local_reply(
            "<think>hm</think>Looking.\n<tool_call>\n{\"name\": \"search_tags\", \"arguments\": {\"query\": \"red kimono\"}}\n</tool_call>\n<tool_call>not json</tool_call>\n<tool_call>{\"name\": \"read_guidance\", \"arguments\": {}}",
            Some("stop"),
        );
        assert_eq!(reply.content, "Looking.");
        assert_eq!(
            local_reply(
                "<|channel>thought
<channel|>===SEGMENT: A===",
                None
            )
            .content,
            "===SEGMENT: A==="
        );
        assert_eq!(reply.calls.len(), 2);
        assert_eq!(reply.calls[0].name, "search_tags");
        assert_eq!(reply.calls[0].id.as_deref(), Some("call_0"));
        assert_eq!(
            crate::forge::tools::arguments(&reply.calls[0].arguments)["query"],
            "red kimono"
        );
        assert_eq!(reply.calls[1].name, "read_guidance");
        assert!(!reply.truncated);
        // Qwen 3.8's XML and Gemma 4's own shape are the same call.
        let xml = local_reply(
            "<tool_call>\n<function=search_tags>\n<parameter=query>\nred kimono\n</parameter>\n<parameter=limit>\n3\n</parameter>\n</function>\n</tool_call>",
            None,
        );
        let gemma = local_reply(
            "<|tool_call>call:search_tags{query:<|\"|>red: kimono, {x}<|\"|>,limit:3}<tool_call|>",
            None,
        );
        for (call, query) in [
            (&xml.calls[0], "red kimono"),
            (&gemma.calls[0], "red: kimono, {x}"),
        ] {
            assert_eq!(call.name, "search_tags");
            let args = crate::forge::tools::arguments(&call.arguments);
            assert_eq!(
                (args["query"].as_str(), args["limit"].as_u64()),
                (Some(query), Some(3))
            );
        }
        let plain = local_reply("===SEGMENT: Prompt===\n1girl", Some("length"));
        assert_eq!(plain.content, "===SEGMENT: Prompt===\n1girl");
        assert!(plain.calls.is_empty() && plain.truncated);
    }

    #[tokio::test]
    async fn local_rounds_need_a_runner_and_refuse_pictures() {
        let config = &LlmConfig::default();
        let round = |messages: Vec<Value>| async move {
            let r = Round {
                config,
                model: "local:model.gguf",
                messages: &messages,
                tools: None,
                temperature: None,
                top_p: None,
                max_tokens: None,
                timeout: Duration::from_secs(1),
            };
            local_round(&r).await.map(|_| ())
        };
        let picture = user_with_images("look", &["data:image/png;base64,AA".into()], false);
        assert!(round(vec![picture]).await.unwrap_err().contains("pictures"));
        let text = json!({ "role": "user", "content": "write" });
        assert!(
            round(vec![text])
                .await
                .unwrap_err()
                .contains("not available")
        );
    }

    #[tokio::test]
    async fn openai_rounds_send_the_saved_key_while_ollama_and_empty_keys_stay_plain() {
        for stream in [false, true] {
            for (provider, key) in [
                (Provider::Openai, "fixture-key"),
                (Provider::Ollama, "fixture-key"),
                (Provider::Openai, ""),
            ] {
                let body = if stream {
                    match provider {
                        Provider::Ollama => "{\"message\":{\"content\":\"ok\"},\"done\":true}\n",
                        Provider::Openai => "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\ndata: [DONE]\n\n",
                        Provider::Local => unreachable!(),
                    }
                    .to_string()
                } else {
                    match provider {
                        Provider::Ollama => "{\"message\":{\"content\":\"ok\"},\"done\":true}",
                        Provider::Openai => {
                            "{\"choices\":[{\"message\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}"
                        }
                        Provider::Local => unreachable!(),
                    }
                    .to_string()
                };
                let response = if stream {
                    format!(
                        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",
                        body.len()
                    )
                } else {
                    complete(&body)
                };
                let (url, mut received, server) = fixture(vec![response]).await;
                let expects_auth = provider == Provider::Openai && !key.is_empty();
                let config = LlmConfig {
                    provider,
                    base_url: url,
                    api_key: key.into(),
                };
                let r = Round {
                    config: &config,
                    model: "fixture",
                    messages: &[],
                    tools: None,
                    temperature: None,
                    top_p: None,
                    max_tokens: None,
                    timeout: Duration::from_secs(2),
                };
                let reply = if stream {
                    stream_round(r, &mut |_| {}, &std::sync::atomic::AtomicBool::new(false)).await
                } else {
                    round(r).await
                }
                .unwrap_or_else(|e| panic!("{provider:?} stream={stream}: {e}"));
                assert_eq!(reply.content, "ok");
                let head = received.recv().await.unwrap().1.to_ascii_lowercase();
                let auth = head.lines().find(|line| line.starts_with("authorization:"));
                if expects_auth {
                    let expected = ["authorization: bearer ", key].concat();
                    assert!(auth.is_some_and(|line| line == expected), "{head}");
                } else {
                    assert!(auth.is_none(), "{head}");
                }
                server.await.unwrap();
            }
        }
    }

    // Real HTTP peers: an unfinished response stays open until the client drops it.
    pub(in crate::forge) async fn fixture(
        responses: Vec<String>,
    ) -> (
        String,
        tokio::sync::mpsc::Receiver<(usize, String)>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = tokio::sync::mpsc::channel(responses.len());
        let server = tokio::spawn(async move {
            for (index, response) in responses.into_iter().enumerate() {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = vec![];
                let mut head;
                let mut buf = [0; 4096];
                loop {
                    let n = socket.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&buf[..n]);
                    if let Some(end) = request.windows(4).position(|b| b == b"\r\n\r\n") {
                        head = String::from_utf8_lossy(&request[..end]).into_owned();
                        let length: usize = head
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|n| n.trim().parse().ok())
                            })
                            .unwrap_or(0);
                        if request.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                socket.write_all(response.as_bytes()).await.unwrap();
                tx.send((index, head)).await.unwrap();
                if !response.contains("Content-Length:") {
                    socket.read_to_end(&mut vec![]).await.unwrap();
                }
            }
        });
        (url, rx, server)
    }

    pub(in crate::forge) fn complete(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[tokio::test]
    async fn stop_cancels_waiting_headers_chunks_and_http_error_body() {
        for response in [
            "",
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n",
            "HTTP/1.1 500 Error\r\nTransfer-Encoding: chunked\r\n\r\n",
        ] {
            let (url, mut received, server) = fixture(vec![response.into()]).await;
            let config = LlmConfig {
                provider: Provider::Openai,
                base_url: url,
                api_key: String::new(),
            };
            let stop = std::sync::atomic::AtomicBool::new(false);
            let mut on_text = |_: &str| {};
            let work = stream_round(
                Round {
                    config: &config,
                    model: "fixture",
                    messages: &[],
                    tools: None,
                    temperature: None,
                    top_p: None,
                    max_tokens: None,
                    timeout: Duration::from_secs(30),
                },
                &mut on_text,
                &stop,
            );
            let cancel = async {
                received.recv().await.unwrap();
                stop.store(true, std::sync::atomic::Ordering::Relaxed);
            };
            let (reply, ()) =
                tokio::time::timeout(Duration::from_secs(1), async { tokio::join!(work, cancel) })
                    .await
                    .expect("stop must interrupt blocked network await");
            assert!(reply.unwrap().content.is_empty());
            tokio::time::timeout(Duration::from_secs(1), server)
                .await
                .unwrap()
                .unwrap();
        }
    }

    #[tokio::test]
    async fn ollama_stream_error_is_not_a_successful_empty_reply() {
        let body = "{\"error\":\"fixture model failed\"}\n";
        let response = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{body}\r\n",
            body.len()
        );
        let (url, _received, server) = fixture(vec![response]).await;
        let config = LlmConfig {
            provider: Provider::Ollama,
            base_url: url,
            api_key: String::new(),
        };
        let stop = std::sync::atomic::AtomicBool::new(false);
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            stream_round(
                Round {
                    config: &config,
                    model: "fixture",
                    messages: &[],
                    tools: None,
                    temperature: None,
                    top_p: None,
                    max_tokens: None,
                    timeout: Duration::from_secs(30),
                },
                &mut |_| {},
                &stop,
            ),
        )
        .await
        .expect("error object must fail without waiting for EOF");
        assert!(result.unwrap_err().contains("fixture model failed"));
        tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn loopback_preserves_streams_and_rejects_bad_indices_before_eof() {
        for ollama in [false, true] {
            let body = if ollama {
                "{\"message\":{\"content\":\"Hello\",\"tool_calls\":[{\"function\":{\"name\":\"read_guide\",\"arguments\":{\"name\":\"ltx\"}}}]},\"done\":true}\n"
            } else {
                "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\",\"tool_calls\":[{\"index\":0,\"function\":{\"name\":\"read_guide\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n"
            };
            let (url, _received, server) = fixture(vec![complete(body)]).await;
            let config = LlmConfig {
                provider: if ollama {
                    Provider::Ollama
                } else {
                    Provider::Openai
                },
                base_url: url,
                api_key: String::new(),
            };
            let mut text = String::new();
            let reply = stream_round(
                Round {
                    config: &config,
                    model: "fixture",
                    messages: &[],
                    tools: None,
                    temperature: None,
                    top_p: None,
                    max_tokens: None,
                    timeout: Duration::from_secs(2),
                },
                &mut |s| text.push_str(s),
                &std::sync::atomic::AtomicBool::new(false),
            )
            .await
            .unwrap();
            assert_eq!(text, "Hello");
            assert_eq!(reply.content, text);
            assert_eq!(reply.calls.len(), 1);
            assert_eq!(reply.calls[0].name, "read_guide");
            server.await.unwrap();
        }
        let body = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":18446744073709551615,\"function\":{\"name\":\"read_guide\"}}]}}]}\n\n";
        let response = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{body}\r\n",
            body.len()
        );
        let (url, _received, server) = fixture(vec![response]).await;
        let config = LlmConfig {
            provider: Provider::Openai,
            base_url: url,
            api_key: String::new(),
        };
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            stream_round(
                Round {
                    config: &config,
                    model: "fixture",
                    messages: &[],
                    tools: None,
                    temperature: None,
                    top_p: None,
                    max_tokens: None,
                    timeout: Duration::from_secs(30),
                },
                &mut |_| {},
                &std::sync::atomic::AtomicBool::new(false),
            ),
        )
        .await
        .unwrap();
        assert!(result.unwrap_err().contains("index"));
        tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .unwrap()
            .unwrap();
    }

    #[test]
    fn openai_stream_assembles_text_and_tool_calls_split_across_chunks() {
        let mut p = StreamParser::new(false);
        let mut text = vec![];
        let sse = concat!(
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"Hel\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"read_",
        );
        text.extend(p.feed(sse.as_bytes()));
        let rest = concat!(
            "guide\",\"arguments\":\"{\\\"name\\\":\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"ltx\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        text.extend(p.feed(rest.as_bytes()));
        assert_eq!(text, vec!["Hel", "lo"]);
        let r = p.finish().unwrap();
        assert_eq!(r.content, "Hello");
        assert_eq!(r.calls.len(), 1);
        assert_eq!(r.calls[0].name, "read_guide");
        assert_eq!(r.calls[0].id.as_deref(), Some("c1"));
        assert_eq!(r.calls[0].arguments, json!("{\"name\":\"ltx\"}"));
    }

    #[test]
    fn ollama_stream_assembles_text_tool_calls_and_length_stop() {
        let mut p = StreamParser::new(true);
        let nd = concat!(
            "{\"message\":{\"role\":\"assistant\",\"content\":\"A \"},\"done\":false}\n",
            "{\"message\":{\"role\":\"assistant\",\"content\":\"\",\"tool_calls\":[{\"function\":{\"name\":\"search_tags\",\"arguments\":{\"query\":\"boots\"}}}]},\"done\":false}\n",
            "{\"message\":{\"role\":\"assistant\",\"content\":\"b\"},\"done\":true,\"done_reason\":\"length\"}",
        );
        let mut text = p.feed(nd.as_bytes());
        let r = {
            text.extend(std::iter::empty::<String>());
            p.finish().unwrap()
        };
        assert_eq!(text, vec!["A "]);
        assert_eq!(r.content, "A b");
        assert!(r.truncated);
        assert_eq!(r.calls[0].arguments, json!({ "query": "boots" }));
        assert!(StreamParser::new(false).finish().is_err());
    }

    #[test]
    fn streamed_tool_allocation_is_bounded_and_fails_closed() {
        let mut p = StreamParser::new(false);
        p.feed(b"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":10000,\"function\":{\"name\":\"read_guide\"}}]}}]}\n");
        assert!(p.calls.len() <= 128, "untrusted index allocated slots");
        assert!(p.finish().is_err());
        for index in [
            json!(128),
            json!(u64::MAX),
            json!(-1),
            json!(1.5),
            json!("0"),
            Value::Null,
        ] {
            let mut p = StreamParser::new(false);
            let event = json!({"choices":[{"delta":{"tool_calls":[{"index":index,"function":{"name":"read_guide"}}]}}]});
            p.feed(format!("data: {event}\n").as_bytes());
            assert!(p.calls.is_empty());
            assert!(p.finish().is_err());
        }
        let mut p = StreamParser::new(false);
        p.feed(b"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":127,\"function\":{\"name\":\"read_guide\"}}]}}]}\n");
        assert_eq!(p.calls.len(), 128);
        assert_eq!(p.finish().unwrap().calls[0].name, "read_guide");

        let mut p = StreamParser::new(true);
        for _ in 0..129 {
            p.feed(b"{\"message\":{\"tool_calls\":[{\"function\":{\"name\":\"read_guide\"}}]}}\n");
        }
        assert!(p.calls.len() <= 128, "untrusted count allocated slots");
        assert!(p.finish().is_err());
    }

    #[test]
    fn both_wire_shapes_parse() {
        let openai = json!({"choices":[{"finish_reason":"tool_calls","message":{"role":"assistant","content":"","tool_calls":[{"id":"c1","type":"function","function":{"name":"search_tags","arguments":"{\"query\":\"boots\"}"}}]}}]});
        let r = parse(&openai, false).unwrap();
        assert_eq!(r.calls[0].name, "search_tags");
        assert_eq!(r.calls[0].arguments, json!("{\"query\":\"boots\"}"));
        let ollama = json!({"done":true,"done_reason":"length","message":{"role":"assistant","content":"x","tool_calls":[{"function":{"name":"read_guidance","arguments":{"name":"anima-model"}}}]}});
        let r = parse(&ollama, true).unwrap();
        assert!(r.truncated);
        assert_eq!(
            assistant_turn("", &r.calls, true)["tool_calls"][0]["function"]["arguments"]["name"],
            "anima-model"
        );
        assert_eq!(
            assistant_turn("", &r.calls, false)["tool_calls"][0]["function"]["arguments"],
            json!("{\"name\":\"anima-model\"}")
        );
    }
}
