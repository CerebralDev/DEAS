//! Shared text-only LLM transport. No API keys or secrets are accepted or persisted.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{io::Write, path::Path, process::Stdio, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt};

const FILE: &str = "deas-llm.json";
const MAX_RESPONSE: usize = 1024 * 1024;
const MAX_INPUT: usize = 128 * 1024;
const REMOTE_TIMEOUT: Duration = Duration::from_secs(120);
const LOCAL_TIMEOUT: Duration = Duration::from_secs(180);
static RUNS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Local,
    Ollama,
    Openai,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LlmConfig {
    pub provider: Provider,
    pub base_url: String,
    /// Bearer token for OpenAI-compatible servers; the node pack's `openai_api_key`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_key: String,
}
impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: Provider::Local,
            base_url: "http://127.0.0.1:11434".into(),
            api_key: String::new(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LlmRequest {
    pub model: ModelRef,
    pub system: String,
    pub prompt: String,
    pub max_tokens: u32,
    pub temperature: f64,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LlmResult {
    pub text: String,
    pub backend: Provider,
}

pub(crate) fn endpoint(config: &LlmConfig, route: &str) -> Result<reqwest::Url, String> {
    let raw = &config.base_url;
    if raw.len() > 2048
        || raw.trim() != raw
        || raw.chars().any(char::is_control)
        || raw.contains(['?', '#'])
    {
        return Err("LLM base URL must be an HTTP(S) URL without query or fragment".into());
    }
    let mut url = reqwest::Url::parse(raw).map_err(|_| "Invalid LLM base URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("LLM URL must use HTTP(S), without credentials".into());
    }
    let mut path = url.path().trim_end_matches('/').to_owned();
    // Same rule as the node pack's OpenAICompatible, so one URL works in both.
    if config.provider == Provider::Openai && !path.ends_with("/v1") {
        path.push_str("/v1");
    }
    url.set_path(&format!("{path}/"));
    url.join(route).map_err(|_| "Invalid LLM endpoint".into())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRef {
    pub id: String,
    pub provider: Provider,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub provider: Provider,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ModelList {
    pub models: Vec<Model>,
    pub warnings: Vec<String>,
}
fn model_id(id: &str, provider: Provider) -> Result<(), String> {
    if id.is_empty() || id.len() > 512 || id.trim() != id || id.chars().any(char::is_control) {
        return Err("Invalid LLM model identifier".into());
    }
    if provider == Provider::Local
        && (id.contains(['\\', ':', '%'])
            || id.split('/').any(|s| {
                s.is_empty() || s == "." || s == ".." || s.trim() != s || s.starts_with('.')
            })
            || !id.to_ascii_lowercase().ends_with(".gguf"))
    {
        return Err("Local LLM identifier must be a relative GGUF name without traversal".into());
    }
    Ok(())
}
fn llm_root(root: &Path) -> Result<std::path::PathBuf, String> {
    let folders = crate::settings::read(root).map_err(|_| "Cannot read model folder settings")?;
    let base = folders
        .models
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("ComfyUI/models"));
    if !base.is_absolute() {
        return Err("Models folder must be absolute".into());
    }
    // The configured root may be an ordinary symlink alias to an external model collection.
    let base = dunce::canonicalize(base).map_err(|_| "Local models folder is unavailable")?;
    let llm = base.join("llm");
    match std::fs::symlink_metadata(&llm) {
        Ok(m) if m.is_dir() && !m.file_type().is_symlink() => Ok(llm),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(llm),
        _ => Err("Local llm folder must be a non-symlink directory".into()),
    }
}

// Read bounded metadata, never tensors. Unknown/new architectures need a text-family entry here.
fn language_gguf(path: &Path) -> bool {
    use std::io::{BufReader, Read};
    fn number<const N: usize>(r: &mut impl Read) -> std::io::Result<[u8; N]> {
        let mut b = [0; N];
        r.read_exact(&mut b)?;
        Ok(b)
    }
    fn string(r: &mut impl Read) -> std::io::Result<String> {
        let n = u64::from_le_bytes(number(r)?);
        if n > 1024 * 1024 {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        let mut bytes = vec![0; n as usize];
        r.read_exact(&mut bytes)?;
        String::from_utf8(bytes).map_err(|_| std::io::ErrorKind::InvalidData.into())
    }
    fn skip(r: &mut impl Read, kind: u32, depth: u8) -> std::io::Result<()> {
        let n = match kind {
            0 | 1 | 7 => 1,
            2 | 3 => 2,
            4..=6 => 4,
            10..=12 => 8,
            8 => {
                string(r)?;
                return Ok(());
            }
            9 if depth == 0 => {
                let element = u32::from_le_bytes(number(r)?);
                let count = u64::from_le_bytes(number(r)?);
                if count > 1_000_000 {
                    return Err(std::io::ErrorKind::InvalidData.into());
                }
                for _ in 0..count {
                    skip(r, element, depth + 1)?;
                }
                return Ok(());
            }
            _ => return Err(std::io::ErrorKind::InvalidData.into()),
        };
        let mut bytes = [0; 8];
        r.read_exact(&mut bytes[..n])
    }
    let inspect = || -> std::io::Result<bool> {
        let file = std::fs::File::open(path)?;
        let mut r = BufReader::new(file).take(16 * 1024 * 1024);
        if number::<4>(&mut r)? != *b"GGUF" || !matches!(u32::from_le_bytes(number(&mut r)?), 2 | 3)
        {
            return Ok(false);
        }
        let tensors = u64::from_le_bytes(number(&mut r)?);
        let count = u64::from_le_bytes(number(&mut r)?);
        if tensors == 0 || count > 100_000 {
            return Ok(false);
        }
        let mut arch = String::new();
        let mut kind = String::new();
        for _ in 0..count {
            let key = string(&mut r)?;
            let ty = u32::from_le_bytes(number(&mut r)?);
            if matches!(key.as_str(), "general.architecture" | "general.type") {
                if ty != 8 {
                    return Ok(false);
                }
                let value = string(&mut r)?;
                if key == "general.architecture" {
                    arch = value;
                } else {
                    kind = value;
                }
            } else {
                skip(&mut r, ty, 0)?;
            }
        }
        Ok((kind.is_empty() || kind == "model")
            && matches!(
                arch.as_str(),
                "llama"
                    | "deci"
                    | "falcon"
                    | "baichuan"
                    | "grok"
                    | "gpt2"
                    | "gptj"
                    | "gptneox"
                    | "mpt"
                    | "starcoder"
                    | "starcoder2"
                    | "bloom"
                    | "stablelm"
                    | "qwen"
                    | "qwen2"
                    | "qwen2moe"
                    | "qwen2vl"
                    | "qwen3"
                    | "qwen3vl"
                    | "qwen3vlmoe"
                    | "qwen3moe"
                    | "qwen3next"
                    | "qwen35"
                    | "qwen35moe"
                    | "phi2"
                    | "phi3"
                    | "phimoe"
                    | "gemma"
                    | "gemma2"
                    | "gemma3"
                    | "gemma3n"
                    | "gemma4"
                    | "command-r"
                    | "cohere2"
                    | "dbrx"
                    | "olmo"
                    | "olmo2"
                    | "olmoe"
                    | "openelm"
                    | "deepseek"
                    | "deepseek2"
                    | "deepseek32"
                    | "chatglm"
                    | "glm4"
                    | "glm4moe"
                    | "glm4moelite"
                    | "bitnet"
                    | "t5"
                    | "t5encoder"
                    | "jais"
                    | "nemotron"
                    | "nemotron_h"
                    | "exaone"
                    | "exaone4"
                    | "granite"
                    | "granitemoe"
                    | "granitehybrid"
                    | "internlm2"
                    | "minicpm"
                    | "minicpm3"
                    | "mamba"
                    | "mamba2"
                    | "rwkv6"
                    | "rwkv6qwen2"
                    | "rwkv7"
                    | "arctic"
                    | "gpt-oss"
                    | "hunyuan-moe"
                    | "hunyuan-dense"
                    | "smollm3"
                    | "lfm2"
                    | "lfm2moe"
                    | "minimax-m2"
            ))
    };
    !path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().to_ascii_lowercase().contains("mmproj"))
        && inspect().unwrap_or(false)
}
fn local_models(root: &Path) -> Result<Vec<Model>, String> {
    let base = llm_root(root)?;
    if !base.exists() {
        return Ok(vec![]);
    }
    fn walk(
        base: &Path,
        dir: &Path,
        depth: usize,
        seen: &mut usize,
        models: &mut Vec<Model>,
    ) -> Result<(), String> {
        if depth > 32 || *seen >= 5000 {
            return Ok(());
        }
        let entries = std::fs::read_dir(dir).map_err(|_| "Cannot scan local llm folder")?;
        for entry in entries {
            if *seen >= 5000 {
                break;
            }
            let entry = entry.map_err(|_| "Cannot scan local llm entry")?;
            *seen += 1;
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let ty = entry
                .file_type()
                .map_err(|_| "Cannot inspect local llm entry")?;
            let path = entry.path();
            if ty.is_symlink() {
                continue;
            }
            if ty.is_dir() {
                walk(base, &path, depth + 1, seen, models)?;
            } else if ty.is_file()
                && path
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("gguf"))
            {
                let Some(relative) = path.strip_prefix(base).ok().and_then(Path::to_str) else {
                    continue;
                };
                let id = relative.replace(std::path::MAIN_SEPARATOR, "/");
                if model_id(&id, Provider::Local).is_ok() && language_gguf(&path) {
                    models.push(Model {
                        name: id.clone(),
                        id,
                        provider: Provider::Local,
                    });
                }
            }
        }
        Ok(())
    }
    let mut models = vec![];
    walk(&base, &base, 0, &mut 0, &mut models)?;
    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models)
}
fn resolve_local(root: &Path, id: &str) -> Result<std::path::PathBuf, String> {
    model_id(id, Provider::Local)?;
    if !local_models(root)?.iter().any(|m| m.id == id) {
        return Err("Selected local LLM model is no longer available".into());
    }
    let base = llm_root(root)?;
    let path = base.join(id);
    // Recheck every component: a previously scanned directory/file may have changed.
    let mut current = base.clone();
    for segment in id.split('/') {
        current.push(segment);
        if std::fs::symlink_metadata(&current)
            .map_err(|_| "Local model changed")?
            .file_type()
            .is_symlink()
        {
            return Err("Local model symlinks are not allowed".into());
        }
    }
    let actual = dunce::canonicalize(&path).map_err(|_| "Local model is unavailable")?;
    // Compare the std forms: on Windows both carry `\\?\`, while dunce keeps it only for a
    // path over 260 chars, so a deep llm folder failed `starts_with` against its own base.
    let inside = std::fs::canonicalize(&path)
        .and_then(|a| std::fs::canonicalize(&base).map(|b| a.starts_with(b)))
        .map_err(|_| "Local model is unavailable")?;
    if !inside || !actual.is_file() || !language_gguf(&actual) {
        return Err("Invalid local GGUF model".into());
    }
    Ok(actual)
}
async fn remote_models(config: &LlmConfig) -> Result<Vec<Model>, String> {
    if config.provider == Provider::Local {
        return Ok(vec![]);
    }
    let ollama = config.provider == Provider::Ollama;
    let value = http_json(
        config,
        if ollama { "api/tags" } else { "models" },
        None,
        Duration::from_secs(10),
    )
    .await?;
    let entries = value[if ollama { "models" } else { "data" }]
        .as_array()
        .filter(|v| v.len() <= 5000)
        .ok_or("Invalid LLM model listing")?;
    let mut models = Vec::new();
    for entry in entries {
        let id = entry[if ollama { "name" } else { "id" }]
            .as_str()
            .ok_or("Invalid LLM model identifier")?;
        model_id(id, config.provider)?;
        models.push(Model {
            id: id.into(),
            name: id.into(),
            provider: config.provider,
        });
    }
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    Ok(models)
}
const LOCAL_TIMED_OUT: &str = "Local LLM timed out. If a game or another app is using the GPU, close it and try again, or pick a smaller GGUF.";

pub async fn models(root: &Path, config: &LlmConfig) -> Result<ModelList, String> {
    root_guard(root)?;
    validate(config)?;
    let mut result = ModelList {
        models: vec![],
        warnings: vec![],
    };
    // Remote listed first in join! so its request starts before the blocking folder scan.
    let (remote, local) = tokio::join!(remote_models(config), async { local_models(root) });
    match local {
        Ok(models) => result.models = models,
        Err(e) => result.warnings.push(e),
    }
    match remote {
        Ok(models) => result.models.extend(models),
        Err(e) => result.warnings.push(e),
    }
    Ok(result)
}
fn validate(config: &LlmConfig) -> Result<(), String> {
    if config.provider != Provider::Local || !config.base_url.is_empty() {
        endpoint(config, "")?;
    }
    if !config.api_key.is_empty()
        && (config.provider != Provider::Openai
            || config.api_key.len() > 512
            || config.api_key.trim() != config.api_key
            || config.api_key.bytes().any(|b| !(0x21..0x7f).contains(&b)))
    {
        return Err("LLM API key is only for OpenAI-compatible servers: printable ASCII, at most 512 characters".into());
    }
    Ok(())
}
fn validate_request(request: &LlmRequest) -> Result<(), String> {
    if request.prompt.trim().is_empty()
        || request.prompt.len() + request.system.len() > MAX_INPUT
        || request.prompt.contains('\0')
        || request.system.contains('\0')
    {
        return Err("LLM prompt must be nonempty; system and prompt together must be at most 128 KiB without NUL".into());
    }
    if !(1..=4095).contains(&request.max_tokens)
        || !request.temperature.is_finite()
        || !(0.0..=2.0).contains(&request.temperature)
    {
        return Err("LLM maxTokens must be 1–4095; temperature must be finite in [0, 2]".into());
    }
    model_id(&request.model.id, request.model.provider)?;
    Ok(())
}
fn root_guard(root: &Path) -> Result<crate::downloads::fs::Anchor, String> {
    let anchor = crate::downloads::fs::Anchor::open(root)?;
    anchor.writable()?;
    Ok(anchor)
}
fn target_guard(root: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(root.join(FILE)) {
        Ok(m) => {
            if !m.is_file() || m.file_type().is_symlink() {
                return Err("LLM settings must be a regular non-symlink file".into());
            }
            #[cfg(target_os = "linux")]
            {
                use std::os::unix::fs::MetadataExt;
                if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o022 != 0 {
                    return Err("LLM settings must be owned and not group/world writable".into());
                }
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("Cannot inspect LLM settings".into()),
    }
}
pub fn read(root: &Path) -> Result<LlmConfig, String> {
    let _anchor = root_guard(root)?;
    target_guard(root)?;
    if !root.join(FILE).exists() {
        return Ok(LlmConfig::default());
    }
    let bytes = crate::backend::read_owned(&root.join(FILE), 16 * 1024)?;
    // Only disk reads accept removed fields; the IPC write contract remains strict.
    let mut value: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid LLM settings JSON")?;
    if let Some(object) = value.as_object_mut() {
        for key in ["model", "localModel", "fallback", "nCtx", "nGpuLayers"] {
            object.remove(key);
        }
    }
    let config = serde_json::from_value(value).map_err(|_| "Invalid LLM settings JSON")?;
    validate(&config)?;
    Ok(config)
}
pub fn write(root: &Path, config: LlmConfig) -> Result<LlmConfig, String> {
    let anchor = root_guard(root)?;
    validate(&config)?;
    target_guard(root)?;
    let tmp = format!(".{FILE}.{}", uuid::Uuid::new_v4());
    let mut file = anchor.partial(&tmp)?;
    let result = (|| {
        file.write_all(
            &serde_json::to_vec_pretty(&config).map_err(|_| "Cannot serialize LLM settings")?,
        )
        .map_err(|_| "Cannot write LLM settings")?;
        file.sync_all().map_err(|_| "Cannot sync LLM settings")?;
        target_guard(root)?;
        if crate::downloads::fs::Anchor::open(root)?.identity()? != anchor.identity()? {
            return Err("Environment root changed".into());
        }
        std::fs::rename(root.join(&tmp), root.join(FILE))
            .map_err(|_| "Cannot publish LLM settings")?;
        // shortcut: post-rename directory sync is best effort, not power-loss transactional.
        if let Ok(dir) = std::fs::File::open(root) {
            let _ = dir.sync_all();
        }
        Ok(config)
    })();
    if result.is_err() {
        anchor.remove_partial(&tmp);
    }
    result
}

fn response_text(value: &Value, provider: Provider) -> Result<String, String> {
    if value.get("error").is_some() {
        return Err("LLM server returned a model error".into());
    }
    if provider == Provider::Ollama
        && (value["done"] != true || value["message"]["role"] != "assistant")
    {
        return Err("Invalid Ollama chat response".into());
    }
    if provider == Provider::Openai && value["choices"][0]["message"]["role"] != "assistant" {
        return Err("Invalid OpenAI chat response".into());
    }
    let text = if provider == Provider::Ollama {
        value["message"]["content"].as_str()
    } else {
        value["choices"][0]["message"]["content"].as_str()
    };
    let text = text
        .filter(|t| !t.trim().is_empty() && t.len() <= MAX_RESPONSE && !t.contains('\0'))
        .ok_or("LLM response has no valid text")?;
    Ok(text.to_owned())
}
fn http_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "LLM request timed out"
    } else {
        "LLM server unavailable or transport failed"
    }
    .into()
}
async fn http_json(
    config: &LlmConfig,
    route: &str,
    body: Option<Value>,
    timeout: Duration,
) -> Result<Value, String> {
    let work = async {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_secs(10))
            .timeout(timeout)
            .build()
            .map_err(|_| "Cannot create LLM HTTP client")?;
        let url = endpoint(config, route)?;
        let mut builder = if let Some(body) = body {
            client.post(url).json(&body)
        } else {
            client.get(url)
        };
        if config.provider == Provider::Openai && !config.api_key.is_empty() {
            builder = builder.bearer_auth(&config.api_key);
        }
        let mut response = builder.send().await.map_err(http_error)?;
        if !response.status().is_success() {
            return Err(format!(
                "LLM server returned HTTP {}",
                response.status().as_u16()
            ));
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE as u64)
        {
            return Err("LLM response exceeds byte limit".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(http_error)? {
            if bytes.len() + chunk.len() > MAX_RESPONSE {
                return Err("LLM response exceeds byte limit".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| "LLM server returned invalid JSON".into())
    };
    tokio::time::timeout(timeout, work)
        .await
        .map_err(|_| "LLM request timed out")?
}
async fn remote(
    config: &LlmConfig,
    request: &LlmRequest,
    timeout: Duration,
) -> Result<String, String> {
    let messages = json!([{"role":"system","content":request.system},{"role":"user","content":request.prompt}]);
    let (route, body) = if config.provider == Provider::Ollama {
        (
            "api/chat",
            // think:false — thinking models (Qwen3.5, gpt-oss) otherwise spend num_predict in `message.thinking` and return empty `content`.
            json!({"model":request.model.id,"messages":messages,"stream":false,"think":false,"options":{"num_ctx":4096,"num_predict":request.max_tokens,"temperature":request.temperature}}),
        )
    } else {
        (
            "chat/completions",
            json!({"model":request.model.id,"messages":messages,"stream":false,"max_tokens":request.max_tokens,"temperature":request.temperature}),
        )
    };
    response_text(
        &http_json(config, route, Some(body), timeout).await?,
        config.provider,
    )
}
async fn bounded_read(reader: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| "Cannot read local LLM output")?;
    if bytes.len() > limit {
        return Err("Local LLM output exceeds byte limit".into());
    }
    Ok(bytes)
}
fn owned_environment(root: &Path) -> Result<(), String> {
    root_guard(root)?;
    let bytes = crate::backend::read_owned(&root.join("deas-plan.json"), 1024 * 1024)?;
    let plan: crate::install::Plan =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid owned environment plan")?;
    if Path::new(&plan.request.storage_path) != root {
        return Err("Owned environment root does not match plan".into());
    }
    crate::backend::Approved::capture(&plan)?.validate()
}
async fn run_worker(root: &Path, input: Vec<u8>, timeout: Duration) -> Result<String, String> {
    let mut command = tokio::process::Command::new(root.join(crate::install::VENV_PYTHON));
    command
        .args(["-I", "-c", include_str!("llm_worker.py")])
        .current_dir(root)
        .env("PYTHONUTF8", "1")
        .env_remove("PYTHONPATH")
        .env_remove("PYTHONHOME")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command
        .spawn()
        .map_err(|_| "Cannot start managed local LLM worker")?;
    let mut stdin = child.stdin.take().ok_or("Local LLM stdin unavailable")?;
    let stdout = child.stdout.take().ok_or("Local LLM stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("Local LLM stderr unavailable")?;
    let work = async {
        let send = async {
            stdin
                .write_all(&input)
                .await
                .map_err(|_| "Cannot send local LLM request".to_string())?;
            drop(stdin);
            Ok::<(), String>(())
        };
        let wait = async {
            child
                .wait()
                .await
                .map_err(|_| "Cannot wait for local LLM worker".to_string())
        };
        let (_, stdout, _, status) = tokio::try_join!(
            send,
            bounded_read(stdout, MAX_RESPONSE),
            bounded_read(stderr, 256 * 1024),
            wait
        )?;
        if !status.success() {
            return Err("Local LLM failed; verify llama-cpp-python, GGUF compatibility and available memory/context".into());
        }
        let value: Value =
            serde_json::from_slice(&stdout).map_err(|_| "Invalid local LLM response")?;
        response_text(
            &json!({"choices":[{"message":{"content":value["text"]}}]}),
            Provider::Local,
        )
    };
    let result = tokio::time::timeout(timeout, work)
        .await
        .unwrap_or_else(|_| Err(LOCAL_TIMED_OUT.into()));
    if result.is_err() {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    result
}
async fn local(root: &Path, request: &LlmRequest) -> Result<String, String> {
    let path = resolve_local(root, &request.model.id)?;
    // Use backend startup's manifest, ownership, interpreter and launch fingerprint guards.
    owned_environment(root)?;
    let input = serde_json::to_vec(&json!({"modelPath":path,"request":request}))
        .map_err(|_| "Cannot encode local LLM request")?;
    run_worker(root, input, LOCAL_TIMEOUT).await
}

const LOCAL_FAILED: &str =
    "Local LLM failed; verify llama-cpp-python, GGUF compatibility and available memory/context";

struct LocalWorker {
    model: String,
    _permit: tokio::sync::SemaphorePermit<'static>,
    // kill_on_drop: losing the worker, however that happens, frees the model.
    _child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    stdout: tokio::io::BufReader<tokio::process::ChildStdout>,
}
impl LocalWorker {
    async fn exchange(&mut self, request: &Value) -> Result<Value, String> {
        let mut line =
            serde_json::to_vec(request).map_err(|_| "Cannot encode local LLM request")?;
        line.push(b'\n');
        self.stdin
            .write_all(&line)
            .await
            .map_err(|_| LOCAL_FAILED.to_string())?;
        let mut reply = Vec::new();
        (&mut self.stdout)
            .take(MAX_RESPONSE as u64 * 4)
            .read_until(b'\n', &mut reply)
            .await
            .map_err(|_| LOCAL_FAILED.to_string())?;
        let value: Value = serde_json::from_slice(&reply).map_err(|_| LOCAL_FAILED.to_string())?;
        match value["error"].as_str() {
            Some("context") => Err("This write is too long for the context the local model loaded with. Free GPU memory by stopping the backend, lower Detail, or pick a server under Backend → LLM.".into()),
            Some("memory") => Err("Not enough free GPU memory for this local model. Close other apps using the GPU, stop the backend, or pick a smaller GGUF.".into()),
            Some("runtime") => Err("The local LLM runtime did not start. Use Backend → Re-run installation to repair it.".into()),
            Some("load") => Err("The local model did not load. It may be too large for the free memory, so stop the backend or pick a smaller GGUF.".into()),
            Some(_) => Err(LOCAL_FAILED.into()),
            None => Ok(value),
        }
    }
}

/// One Prompt Forge command's local model: loaded by its first round, kept for
/// the rounds after it, and gone when the command ends.
pub struct LocalRunner {
    root: std::path::PathBuf,
    worker: tokio::sync::Mutex<Option<LocalWorker>>,
}
impl LocalRunner {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            worker: tokio::sync::Mutex::new(None),
        }
    }

    async fn open(&self, model: &str) -> Result<LocalWorker, String> {
        root_guard(&self.root)?;
        let path = resolve_local(&self.root, model)?;
        owned_environment(&self.root)?;
        let permit = RUNS
            .try_acquire()
            .map_err(|_| "An LLM generation is already running")?;
        let mut command = tokio::process::Command::new(self.root.join(crate::install::VENV_PYTHON));
        command
            .args(["-I", "-c", include_str!("llm_worker.py"), "session"])
            .current_dir(&self.root)
            .env("PYTHONUTF8", "1")
            .env_remove("PYTHONPATH")
            .env_remove("PYTHONHOME")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command
            .spawn()
            .map_err(|_| "Cannot start managed local LLM worker")?;
        let stdin = child.stdin.take().ok_or("Local LLM stdin unavailable")?;
        let stdout = child.stdout.take().ok_or("Local LLM stdout unavailable")?;
        let mut worker = LocalWorker {
            model: model.to_string(),
            _permit: permit,
            _child: child,
            stdin,
            stdout: tokio::io::BufReader::new(stdout),
        };
        worker.exchange(&json!({ "modelPath": path })).await?;
        Ok(worker)
    }

    /// One chat round; the answer's text and finish reason.
    pub async fn round(
        &self,
        model: &str,
        request: Value,
        timeout: Duration,
    ) -> Result<(String, Option<String>), String> {
        let mut slot = self.worker.lock().await;
        let work = async {
            if slot.as_ref().is_none_or(|w| w.model != model) {
                // Free the old model before the new one needs its memory.
                *slot = None;
                *slot = Some(self.open(model).await?);
            }
            slot.as_mut()
                .ok_or(LOCAL_FAILED.to_string())?
                .exchange(&request)
                .await
        };
        let result = tokio::time::timeout(timeout, work)
            .await
            .unwrap_or_else(|_| Err(LOCAL_TIMED_OUT.into()));
        if result.is_err() {
            *slot = None;
        }
        let value = result?;
        Ok((
            value["text"].as_str().unwrap_or("").to_string(),
            value["finish"].as_str().map(String::from),
        ))
    }
}

pub async fn generate(
    root: &Path,
    config: &LlmConfig,
    request: &LlmRequest,
) -> Result<LlmResult, String> {
    validate(config)?;
    validate_request(request)?;
    root_guard(root)?;
    let _permit = RUNS
        .try_acquire()
        .map_err(|_| "An LLM generation is already running")?;
    let text = if request.model.provider == Provider::Local {
        local(root, request).await?
    } else {
        if request.model.provider != config.provider {
            return Err("Requested LLM provider is not configured".into());
        }
        let models = remote_models(config).await?;
        if !models.iter().any(|m| m.id == request.model.id) {
            return Err("Selected remote LLM model is no longer available".into());
        }
        remote(config, request, REMOTE_TIMEOUT).await?
    };
    Ok(LlmResult {
        text,
        backend: request.model.provider,
    })
}
#[tauri::command]
pub async fn native_llm_models(root: String) -> Result<ModelList, String> {
    let config = native_llm_read(root.clone())?;
    let root = std::path::PathBuf::from(root);
    let result = models(&root, &config).await;
    if result
        .as_ref()
        .map_or(true, |list| !list.warnings.is_empty())
    {
        crate::logs::push(
            crate::logs::Source::Deas,
            "ERROR: LLM model discovery: failed to read one or more model sources",
        );
    }
    result
}
#[tauri::command]
pub fn native_llm_read(root: String) -> Result<LlmConfig, String> {
    read(Path::new(&root)).inspect_err(|_| {
        crate::logs::push(
            crate::logs::Source::Deas,
            "ERROR: LLM configuration read: settings unavailable or invalid",
        );
    })
}
#[tauri::command]
pub fn native_llm_write(
    root: String,
    config: LlmConfig,
    operations: tauri::State<'_, crate::FolderOperations>,
) -> Result<LlmConfig, String> {
    let _guard = operations.enter()?;
    write(Path::new(&root), config)
}
#[tauri::command]
pub async fn native_llm_generate(root: String, request: LlmRequest) -> Result<LlmResult, String> {
    let root = std::path::PathBuf::from(root);
    let config = read(&root)?;
    generate(&root, &config, &request).await
}
/// The node pack's own LLM settings shape, filled from DEAS's saved endpoint; the webview never names a server.
fn forge_settings(config: &LlmConfig) -> Value {
    match config.provider {
        Provider::Local => json!({}),
        Provider::Ollama => json!({ "ollama_url": config.base_url }),
        Provider::Openai => {
            json!({ "openai_url": config.base_url, "openai_api_key": config.api_key })
        }
    }
}
fn forge_route(route: &str) -> Result<&'static str, String> {
    Ok(match route {
        "models" => "dasiwa/h3/forge/models",
        "h3" => "dasiwa/h3/forge",
        "cancel" => "dasiwa/h3/forge/cancel",
        "cast" => "dasiwa/h3/forge/cast",
        "people" => "dasiwa/h3/forge/people",
        "write" => "dasiwa/llm/write",
        _ => return Err("Unknown Prompt Forge route".into()),
    })
}
/// Prompt Forge runs in the DaSiWa node pack on the backend (Director Forge and Prompt Writer); this forwards one JSON call.
#[tauri::command]
pub async fn native_forge(
    base_url: String,
    root: String,
    route: String,
    body: Value,
) -> Result<Value, String> {
    let correlation = if route == "people" {
        body["request_id"]
            .as_str()
            .filter(|id| id.len() == 36 && uuid::Uuid::parse_str(id).is_ok())
            .map(|id| format!(" request={id}"))
            .unwrap_or_default()
    } else {
        String::new()
    };
    forge(&base_url, &root, &route, body)
        .await
        .inspect_err(|e| {
            if e.starts_with(crate::forge::message::CANVAS_ERROR) {
                crate::forge::message::report_canvas_failure("h3");
                return;
            }
            crate::logs::push(
                crate::logs::Source::Deas,
                &format!(
                    "{}: Prompt Forge {route}{correlation}: {e}",
                    if route == "people" && e.ends_with("People detection cancelled") {
                        "INFO"
                    } else {
                        "ERROR"
                    }
                ),
            )
        })
}
fn validate_forge_continuity(value: &Value) -> Result<(), String> {
    let invalid = "Invalid H3 continuity request";
    let spec = value.as_object().ok_or(invalid)?;
    let valid_id = |value: &Value| {
        value.as_str().is_some_and(|id| {
            (1..=80).contains(&id.len())
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
    };
    if spec.keys().any(|key| {
        !matches!(
            key.as_str(),
            "source_kind"
                | "session"
                | "clip_id"
                | "use_references"
                | "current_prompt"
                | "overlap_frames"
        )
    }) || !match value["source_kind"].as_str() {
        Some("checkpoint") => valid_id(&value["clip_id"]),
        Some("video") => value["clip_id"].as_str().is_some_and(|id| {
            id.len() == 64
                && id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }),
        _ => false,
    } || !valid_id(&value["session"])
        || value["session"] == "_imports"
        || !value["use_references"].is_boolean()
        || !value["current_prompt"]
            .as_str()
            .is_some_and(|s| s.len() <= 50_000)
        || spec
            .get("overlap_frames")
            .is_some_and(|v| !matches!(v.as_u64(), Some(73 | 56 | 39 | 22 | 5)))
    {
        return Err(invalid.into());
    }
    Ok(())
}
async fn forge(base_url: &str, root: &str, route: &str, mut body: Value) -> Result<Value, String> {
    let path = forge_route(route)?;
    if !body.is_object() {
        return Err("Invalid Prompt Forge request".into());
    }
    if route == "h3" {
        if let Some(canvas) = body.get("output_canvas") {
            let canvas: crate::forge::message::Canvas = serde_json::from_value(canvas.clone())
                .map_err(|_| {
                    format!(
                        "{}: invalid H3 output_canvas",
                        crate::forge::message::CANVAS_ERROR
                    )
                })?;
            canvas.validate(false)?;
        }
        if let Some(spec) = body.get("continuity") {
            validate_forge_continuity(spec)?;
            if spec["use_references"] == true && body["mode"] != "REF2VA" {
                return Err("Invalid H3 continuity request".into());
            }
        }
    }
    if route == "people" {
        let invalid = "Invalid people detection request";
        let model = body["model"].as_str().ok_or(invalid)?;
        let path = body["path"].as_str().ok_or(invalid)?;
        let id = body["request_id"].as_str().ok_or(invalid)?;
        if model.trim().is_empty()
            || model.len() > 1024
            || model.chars().any(char::is_control)
            || path.is_empty()
            || path.len() > 4096
            || path.starts_with('/')
            || path.contains(['\\', ':'])
            || path.chars().any(char::is_control)
            || path
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
            || path.ends_with(" [output]")
            || path.ends_with(" [temp]")
            || id.len() != 36
            || uuid::Uuid::parse_str(id).is_err()
        {
            return Err(invalid.into());
        }
        body = json!({"model":model,"path":path,"request_id":id});
    }
    body["settings"] = forge_settings(&read(Path::new(root))?);
    let url = crate::validate_base_url(base_url)?
        .join(path)
        .map_err(|_| "Invalid backend URL")?;
    // A local 27B can take minutes to load and write; the node pack's own limit is 600 s.
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(900))
        .build()
        .map_err(|_| "Cannot create Prompt Forge HTTP client")?;
    let mut response = client.post(url).json(&body).send().await.map_err(|e| {
        if e.is_connect() {
            "forge.backendRequired"
        } else {
            "Prompt Forge request failed"
        }
    })?;
    let status = response.status();
    let limit = 4 * MAX_RESPONSE;
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err("Prompt Forge response too large".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Prompt Forge response failed")?
    {
        if chunk.len() > limit - bytes.len() {
            return Err("Prompt Forge response too large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).unwrap_or_default();
    if route == "people" {
        if !status.is_success() {
            let code = value["error"].as_str().unwrap_or("");
            let reason = match code {
                "cancelled" => "People detection cancelled",
                "no_vision" => "People detection failed (no_vision)",
                "busy" => "People detection failed (busy)",
                _ => "People detection failed",
            };
            return Err(format!("{} {reason}", status.as_u16()));
        }
        let invalid = "Invalid people detection response";
        let people = value["people"].as_array().ok_or(invalid)?;
        if people.len() > 32 {
            return Err(invalid.into());
        }
        let mut last = -1.0;
        let mut out = Vec::new();
        for person in people {
            let b = person["box"].as_array().ok_or(invalid)?;
            if b.len() != 4 {
                return Err(invalid.into());
            }
            let b: Vec<f64> = b
                .iter()
                .map(|v| {
                    v.as_f64()
                        .filter(|n| n.is_finite() && *n >= 0.0 && *n <= 1.0)
                        .ok_or(invalid)
                })
                .collect::<Result<_, _>>()?;
            let center = b[0] + b[2];
            if b[2] - b[0] < 0.02 || b[3] - b[1] < 0.02 || center < last {
                return Err(invalid.into());
            }
            last = center;
            out.push(json!({"box":b}));
        }
        return Ok(json!({"people":out}));
    }
    if status.is_success() {
        if route == "h3"
            && body.get("continuity").is_some()
            && (value["continuity"] != true
                || value["source_kind"] != body["continuity"]["source_kind"]
                || !value["simple_prompt"]
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty() && s.len() <= 50_000))
        {
            let kind = body["continuity"]["source_kind"].as_str().unwrap();
            return Err(format!(
                "Invalid H3 continuity response: backend did not confirm {kind} prompting"
            ));
        }
        Ok(value)
    } else if route == "h3" && body.get("continuity").is_some() {
        // Backend messages may echo private continuity prompts or credentials.
        Err(format!(
            "{} Prompt Forge continuity request failed",
            status.as_u16()
        ))
    } else {
        Err(format!(
            "{} {}",
            status.as_u16(),
            value["message"].as_str().unwrap_or("")
        )
        .trim()
        .into())
    }
}
#[cfg(test)]
#[path = "llm_tests.rs"]
mod tests;

#[cfg(test)]
mod continuity_tests {
    use super::*;

    fn checkpoint() -> Value {
        json!({"source_kind":"checkpoint", "session":"Session-1", "clip_id":"_imports",
            "use_references":false, "current_prompt":"private continuity prompt"})
    }

    async fn fixture(
        status: u16,
        reply: Value,
    ) -> (String, tokio::task::JoinHandle<(String, Value)>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
                .await
                .unwrap()
                .unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                header.push(stream.read_u8().await.unwrap());
            }
            let header = String::from_utf8(header).unwrap();
            let length = header
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|n| n.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            let mut bytes = vec![0; length];
            stream.read_exact(&mut bytes).await.unwrap();
            let reply = serde_json::to_vec(&reply).unwrap();
            stream.write_all(format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", reply.len()).as_bytes()).await.unwrap();
            stream.write_all(&reply).await.unwrap();
            (
                header.lines().next().unwrap().into(),
                serde_json::from_slice(&bytes).unwrap(),
            )
        });
        (url, task)
    }

    #[tokio::test]
    async fn forge_h3_canvas_rejects_invalid_geometry_before_settings_or_http_with_shared_logs() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let root = tempfile::tempdir().unwrap();
        let before = crate::logs::native_logs(0)
            .last()
            .map_or(0, |entry| entry.seq);
        for canvas in [
            Value::Null,
            json!({"width":63,"height":64}),
            json!({"width":512.5,"height":512}),
            json!({"width":8192,"height":8192}),
            json!({"width":512,"height":512,"token":"private-secret"}),
            json!({"width":"512","height":512}),
            json!({"width":512}),
        ] {
            let error = native_forge(
                format!("http://{}", listener.local_addr().unwrap()),
                root.path().to_string_lossy().into(),
                "h3".into(),
                json!({"output_canvas":canvas,"brief":"private-prompt"}),
            )
            .await
            .unwrap_err();
            assert!(error.starts_with(crate::forge::message::CANVAS_ERROR));
            assert!(!error.contains("private"));
        }
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert!(!root.path().join(FILE).exists());
        let entries: Vec<_> = crate::logs::native_logs(before)
            .into_iter()
            .filter(|entry| {
                entry
                    .text
                    .contains("origin=prompt-forge operation=h3 canvas validation failed")
            })
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, crate::logs::Source::Deas);
        assert!(!entries[0].text.contains("private"));
        assert!(!entries[0].text.contains("http"));
    }

    #[tokio::test]
    async fn continuity_rejects_success_responses_without_source_evidence_marker() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            LlmConfig {
                provider: Provider::Ollama,
                base_url: "http://127.0.0.1:11434".into(),
                api_key: String::new(),
            },
        )
        .unwrap();
        for kind in ["checkpoint", "video"] {
            let mut spec = checkpoint();
            spec["source_kind"] = json!(kind);
            spec["clip_id"] = json!("0123456789abcdef".repeat(4));
            for reply in [
                json!({"simple_prompt":"ordinary draft"}),
                json!({"simple_prompt":"draft","continuity":false,"source_kind":kind}),
                json!({"simple_prompt":"draft","continuity":true,"source_kind":if kind == "checkpoint" { "video" } else { "checkpoint" }}),
                json!({"simple_prompt":"draft","continuity":true,"source_kind":null}),
                json!({"simple_prompt":"","continuity":true,"source_kind":kind}),
                json!({"simple_prompt":" \n\t","continuity":true,"source_kind":kind}),
                json!({"simple_prompt":"x".repeat(50_001),"continuity":true,"source_kind":kind}),
            ] {
                let (url, task) = fixture(200, reply).await;
                let result = native_forge(
                    url,
                    root.path().to_string_lossy().into(),
                    "h3".into(),
                    json!({"continuity":spec}),
                )
                .await;
                assert_eq!(
                    result.unwrap_err(),
                    format!(
                        "Invalid H3 continuity response: backend did not confirm {kind} prompting"
                    )
                );
                task.await.unwrap();
            }
        }
    }

    #[tokio::test]
    async fn continuity_and_ordinary_h3_forward_exact_body_with_saved_settings() {
        let root = tempfile::tempdir().unwrap();
        let saved = LlmConfig {
            provider: Provider::Ollama,
            base_url: "http://127.0.0.1:11434/saved".into(),
            api_key: String::new(),
        };
        write(root.path(), saved.clone()).unwrap();
        for overlap in [None, Some(73), Some(56), Some(39), Some(22), Some(5)] {
            let mut spec = checkpoint();
            spec["session"] = json!("aB09_-".repeat(13) + "ab");
            spec["current_prompt"] = json!("é".repeat(25_000));
            if let Some(overlap) = overlap {
                spec["overlap_frames"] = json!(overlap);
            }
            let mut body = json!({"mode":"T2VA", "brief":"next", "continuity":spec, "settings":{"untrusted":true}});
            let (url, task) = fixture(
                200,
                json!({"simple_prompt":"next shot","continuity":true,"source_kind":"checkpoint"}),
            )
            .await;
            assert_eq!(
                native_forge(
                    url,
                    root.path().to_string_lossy().into(),
                    "h3".into(),
                    body.clone()
                )
                .await
                .unwrap(),
                json!({"simple_prompt":"next shot","continuity":true,"source_kind":"checkpoint"})
            );
            body["settings"] = forge_settings(&saved);
            let (line, sent) = task.await.unwrap();
            assert_eq!(line, "POST /dasiwa/h3/forge HTTP/1.1");
            assert_eq!(sent, body);
        }
        for route in ["h3", "cast", "write", "models", "cancel"] {
            let mut body = json!({"brief":"ordinary", "existing_field":true});
            if route == "h3" {
                body["output_canvas"] = json!({"width":1216,"height":832});
            } else {
                body["continuity"] = Value::Null;
            }
            let (url, task) = fixture(200, json!({"ok":true})).await;
            native_forge(
                url,
                root.path().to_string_lossy().into(),
                route.into(),
                body.clone(),
            )
            .await
            .unwrap();
            body["settings"] = forge_settings(&saved);
            let (line, sent) = task.await.unwrap();
            assert_eq!(
                line,
                format!("POST /{} HTTP/1.1", forge_route(route).unwrap())
            );
            assert_eq!(sent, body);
            if route == "h3" {
                assert!(sent.get("continuity").is_none());
            }
        }
        let mut spec = checkpoint();
        spec["session"] = json!("x");
        spec["clip_id"] = json!("c".repeat(80));
        spec["current_prompt"] = json!("");
        assert!(validate_forge_continuity(&spec).is_ok());
    }

    #[tokio::test]
    async fn video_continuity_forwards_exact_body_and_requires_matching_response_kind() {
        let root = tempfile::tempdir().unwrap();
        let saved = LlmConfig {
            provider: Provider::Ollama,
            base_url: "http://127.0.0.1:11434/saved".into(),
            api_key: String::new(),
        };
        write(root.path(), saved.clone()).unwrap();
        for overlap in [None, Some(73), Some(56), Some(39), Some(22), Some(5)] {
            let mut spec = checkpoint();
            spec["source_kind"] = json!("video");
            spec["clip_id"] = json!("0123456789abcdef".repeat(4));
            if let Some(frames) = overlap {
                spec["overlap_frames"] = json!(frames);
            }
            let mut body = json!({"mode":"T2VA", "brief":"next", "continuity":spec, "settings":{"untrusted":true}});
            for kind in ["video", "checkpoint"] {
                let reply =
                    json!({"simple_prompt":"next shot","continuity":true,"source_kind":kind});
                let (url, task) = fixture(200, reply.clone()).await;
                let result = native_forge(
                    url,
                    root.path().to_string_lossy().into(),
                    "h3".into(),
                    body.clone(),
                )
                .await;
                if kind == "video" {
                    assert_eq!(result.unwrap(), reply);
                } else {
                    assert_eq!(
                        result.unwrap_err(),
                        "Invalid H3 continuity response: backend did not confirm video prompting"
                    );
                }
                let (line, sent) = task.await.unwrap();
                assert_eq!(line, "POST /dasiwa/h3/forge HTTP/1.1");
                body["settings"] = forge_settings(&saved);
                assert_eq!(sent, body);
            }
        }
    }

    #[tokio::test]
    async fn ref2va_continuity_forwards_opted_in_references_and_rejects_other_modes() {
        let root = tempfile::tempdir().unwrap();
        let saved = LlmConfig {
            provider: Provider::Ollama,
            base_url: "http://127.0.0.1:11434".into(),
            api_key: String::new(),
        };
        write(root.path(), saved.clone()).unwrap();
        for kind in ["checkpoint", "video"] {
            let mut spec = checkpoint();
            spec["source_kind"] = json!(kind);
            spec["clip_id"] = json!("a".repeat(64));
            spec["use_references"] = json!(true);
            let mut body = json!({"mode":"REF2VA", "brief":"continue", "continuity":spec, "references":[{"kind":"image","path":"deas-inputs/ref.png"}]});
            let reply = json!({"simple_prompt":"next shot","continuity":true,"source_kind":kind});
            let (url, task) = fixture(200, reply.clone()).await;
            assert_eq!(
                native_forge(
                    url,
                    root.path().to_string_lossy().into(),
                    "h3".into(),
                    body.clone()
                )
                .await
                .unwrap(),
                reply
            );
            body["settings"] = forge_settings(&saved);
            assert_eq!(task.await.unwrap().1, body);
            for mode in ["T2VA", "I2VA", "FL2VA", "L2VA", "Image Inpaint"] {
                body["mode"] = json!(mode);
                let error = native_forge(
                    "http://127.0.0.1:1".into(),
                    root.path().to_string_lossy().into(),
                    "h3".into(),
                    body.clone(),
                )
                .await
                .unwrap_err();
                assert_eq!(error, "Invalid H3 continuity request");
            }
        }
    }

    #[tokio::test]
    async fn continuity_http_failure_is_surfaced_and_logged_without_backend_body() {
        let root = tempfile::tempdir().unwrap();
        let (url, task) = fixture(
            422,
            json!({"message":"private continuity prompt hf_private_continuity_secret"}),
        )
        .await;
        let err = native_forge(
            url,
            root.path().to_string_lossy().into(),
            "h3".into(),
            json!({"continuity":checkpoint()}),
        )
        .await
        .unwrap_err();
        task.await.unwrap();
        assert_eq!(err, "422 Prompt Forge continuity request failed");
        assert!(crate::logs::native_logs(0)
            .iter()
            .any(|e| e.source == crate::logs::Source::Deas
                && e.text == format!("ERROR: Prompt Forge h3: {err}")));
        assert!(crate::logs::native_logs(0)
            .iter()
            .all(|e| !e.text.contains("hf_private_continuity_secret")
                && !e.text.contains("private continuity prompt")));
    }

    #[tokio::test]
    async fn malformed_continuity_is_rejected_before_settings_or_network_and_logged_without_body() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(FILE), "invalid settings").unwrap();
        let mut invalid = vec![
            Value::Null,
            json!(false),
            json!([]),
            json!("checkpoint"),
            json!({}),
        ];
        for field in [
            "source_kind",
            "session",
            "clip_id",
            "use_references",
            "current_prompt",
        ] {
            let mut spec = checkpoint();
            spec.as_object_mut().unwrap().remove(field);
            invalid.push(spec);
        }
        for (field, value) in [
            ("source_kind", json!("video")),
            ("source_kind", json!(null)),
            ("session", json!("_imports")),
            ("session", json!("")),
            ("session", json!("a".repeat(81))),
            ("session", json!(42)),
            ("clip_id", json!("")),
            ("clip_id", json!("a".repeat(81))),
            ("clip_id", json!(null)),
            ("use_references", json!(true)),
            ("use_references", json!(0)),
            ("use_references", json!(null)),
            ("current_prompt", json!(null)),
            ("current_prompt", json!(42)),
            ("current_prompt", json!("é".repeat(25_001))),
            ("overlap_frames", json!(null)),
            ("overlap_frames", json!(22.0)),
            ("overlap_frames", json!(true)),
            ("overlap_frames", json!("22")),
            ("overlap_frames", json!(21)),
            ("overlap_frames", json!(-5)),
            ("overlap_frames", json!(18446744073709551615u64)),
            ("unknown_private_key", json!("hf_private_continuity_secret")),
        ] {
            let mut spec = checkpoint();
            spec[field] = value;
            invalid.push(spec);
        }
        for field in ["session", "clip_id"] {
            for id in [
                "../clip", "a/b", "a\\b", "a.b", "a:b", "a%20b", "a b", "é", "a\n", "a\0",
            ] {
                let mut spec = checkpoint();
                spec[field] = json!(id);
                invalid.push(spec);
            }
        }
        for kind in ["checkpoint", "video"] {
            let mut valid = checkpoint();
            valid["source_kind"] = json!(kind);
            valid["clip_id"] = json!("0123456789abcdef".repeat(4));
            assert!(validate_forge_continuity(&valid).is_ok());
            for (field, bad) in [
                ("source_kind", json!("unsupported")),
                ("source_kind", json!(null)),
                ("source_kind", json!(true)),
                ("session", json!("_imports")),
                ("session", json!(null)),
                ("use_references", json!(true)),
                ("use_references", json!(null)),
                ("current_prompt", json!("x".repeat(50_001))),
                ("current_prompt", json!(null)),
                ("overlap_frames", json!(null)),
                ("overlap_frames", json!(22.0)),
                ("overlap_frames", json!(21)),
                ("unknown_private_key", json!("hf_private_continuity_secret")),
            ] {
                let mut spec = valid.clone();
                spec[field] = bad;
                invalid.push(spec);
            }
            for bad in [
                json!(""),
                json!(null),
                json!(42),
                json!("../clip"),
                json!("a".repeat(81)),
            ] {
                let mut spec = valid.clone();
                spec["clip_id"] = bad;
                invalid.push(spec);
            }
            if kind == "video" {
                for bad in [
                    "clip_1".into(),
                    "a".repeat(63),
                    "a".repeat(65),
                    "A".repeat(64),
                    "g".repeat(64),
                    "é".repeat(32),
                ] {
                    let mut spec = valid.clone();
                    spec["clip_id"] = json!(bad);
                    invalid.push(spec);
                }
            }
        }
        for spec in invalid {
            let err = native_forge(
                "not a URL".into(),
                root.path().to_string_lossy().into(),
                "h3".into(),
                json!({"continuity":spec, "brief":"hf_private_continuity_secret"}),
            )
            .await
            .unwrap_err();
            assert_eq!(err, "Invalid H3 continuity request");
        }
        let entries = crate::logs::native_logs(0);
        assert!(entries.iter().any(|e| e.source == crate::logs::Source::Deas
            && e.text == "ERROR: Prompt Forge h3: Invalid H3 continuity request"));
        assert!(entries
            .iter()
            .all(|e| !e.text.contains("hf_private_continuity_secret")
                && !e.text.contains("private continuity prompt")));
    }
}
