//! Media Analysis: image taggers, ported from PromptForge's Media Analysis tab.
//!
//! The taggers are ONNX models run on the CPU by `tagger_worker.py` in a private
//! analysis runtime (an isolated uv venv with onnxruntime, numpy and Pillow), the same
//! shape as the audio runtimes: Rust owns install, discovery and the process; Python
//! only runs the model, one picture per process, nothing left running.
//!
//! CPU on purpose (PromptForge's call too): a tagger on the GPU would fight the LLM and
//! ComfyUI for the card, and on CPU it takes a few seconds.
//!
//! Taggers are found where people already keep them, never copied:
//! - `<models>/taggers/**`: DEAS's own folder, where the default WD v3 is downloaded;
//! - `<ComfyUI>/custom_nodes/comfyui-wd14-tagger/models`: the WD14 Tagger node's folder;
//! - `<models>/animetimm/<name>/`: animetimm models that have an ONNX file.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};

#[cfg(not(windows))]
const REQUIREMENTS: &str = include_str!("analysis_runtime_requirements.txt");
#[cfg(windows)]
const REQUIREMENTS: &str = include_str!("analysis_runtime_requirements_windows.txt");
const WORKER: &str = include_str!("tagger_worker.py");

/// The default tagger: open (Apache-2.0), not gated, ONNX as published.
const WD_REPO: &str = "SmilingWolf/wd-eva02-large-tagger-v3";
const WD_REVISION: &str = "b25b82a03f7282e41aa2f257a52c7583b710bd1c";
const WD_FILES: &[(&str, &str, u64)] = &[
    (
        "model.onnx",
        "9e768793060c7939b277ccb382783e8670e8a042d29d77aa736be0c8cc898bfc",
        1_260_435_999,
    ),
    (
        "selected_tags.csv",
        "298633d94d0031d2081c0893f29c82eab7f0df00b08483ba8f29d1e979441217",
        308_468,
    ),
];
const WD_DIR: &str = "wd-eva02-large-tagger-v3";
/// A tagger run: model load plus one picture on a slow CPU, with room to spare.
const RUN_TIMEOUT: Duration = Duration::from_secs(300);

static CANCEL: AtomicBool = AtomicBool::new(false);
static INSTALLING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
static RUNNING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
static STATUS: Mutex<InstallStatus> = Mutex::new(InstallStatus {
    running: false,
    stage: String::new(),
    downloaded: 0,
    total: 0,
    error: None,
});

#[derive(Clone, Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InstallStatus {
    pub running: bool,
    pub stage: String,
    pub downloaded: u64,
    pub total: u64,
    pub error: Option<String>,
}

fn status(stage: &str, downloaded: u64, total: u64, error: Option<String>) {
    if let Ok(mut s) = STATUS.lock() {
        *s = InstallStatus {
            running: !matches!(stage, "completed" | "failed" | "cancelled" | ""),
            stage: stage.into(),
            downloaded,
            total,
            error,
        };
    }
}

fn root() -> PathBuf {
    if let Some(path) = std::env::var_os("DEAS_ANALYSIS_RUNTIME_DIR") {
        return PathBuf::from(path);
    }
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    base.unwrap_or_default().join("deas/analysis-runtime")
}

fn runtime_dir() -> PathBuf {
    root().join("taggers")
}

fn python(dir: &Path) -> PathBuf {
    dir.join(if cfg!(windows) {
        "venv/Scripts/python.exe"
    } else {
        "venv/bin/python"
    })
}

fn receipt() -> String {
    format!(
        "analysis-taggers;requirements={:x}",
        Sha256::digest(REQUIREMENTS.as_bytes())
    )
}

/// The runtime is installed and matches this DEAS's package lock.
pub(crate) fn runtime_ready() -> bool {
    let dir = runtime_dir();
    fs::read_to_string(dir.join("installed.txt")).is_ok_and(|r| r == receipt())
        && python(&dir).is_file()
}

fn check_cancel() -> Result<(), String> {
    if CANCEL.load(Ordering::Relaxed) {
        Err("ANALYSIS_CANCELLED: Installation cancelled".into())
    } else {
        Ok(())
    }
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        fs::File::open(path).map_err(|_| "ANALYSIS_INTEGRITY: Cannot read a downloaded file")?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|_| "ANALYSIS_INTEGRITY: Cannot hash a downloaded file")?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn quiet(command: &mut Command) {
    command
        .env("PYTHONNOUSERSITE", "1")
        .env("UV_CACHE_DIR", root().join("uv-cache"))
        .env("UV_PYTHON_INSTALL_DIR", root().join("python"))
        .stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(crate::win::CREATE_NO_WINDOW);
    }
}

/// Runs an install step; on failure, the last useful line of its output (no paths).
fn step(mut command: Command) -> Result<(), String> {
    quiet(&mut command);
    let out = command
        .output()
        .map_err(|e| format!("ANALYSIS_INSTALL: Could not start an install step ({e})"))?;
    check_cancel()?;
    if out.status.success() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&out.stderr);
    let last = text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("no output")
        .replace(&root().to_string_lossy().to_string(), "<analysis-runtime>");
    Err(format!(
        "ANALYSIS_INSTALL: {}",
        last.chars().take(400).collect::<String>()
    ))
}

fn find_uv(preferred: Option<PathBuf>) -> Result<PathBuf, String> {
    preferred
        .filter(|p| p.is_absolute() && p.is_file())
        .or_else(|| {
            std::env::var_os("PATH").and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|p| p.join(if cfg!(windows) { "uv.exe" } else { "uv" }))
                    .find(|p| p.is_file())
            })
        })
        .ok_or_else(|| "ANALYSIS_INSTALL: DEAS's uv was not found. Finish the DEAS install under Backend first, then try again.".into())
}

async fn install_runtime(preferred_uv: Option<PathBuf>) -> Result<(), String> {
    if runtime_ready() {
        return Ok(());
    }
    let base = root();
    if !base.is_absolute() {
        return Err("ANALYSIS_INSTALL: No user data folder to install into".into());
    }
    fs::create_dir_all(&base)
        .map_err(|_| "ANALYSIS_INSTALL: Cannot create the analysis runtime folder")?;
    let target = runtime_dir();
    if target.exists() {
        // An older lock or a broken install: replaced whole, never patched.
        fs::remove_dir_all(&target).map_err(|_| "ANALYSIS_INSTALL: Cannot replace the old tagger runtime; close other DEAS windows and try again")?;
    }
    let stage = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(&base)
        .map_err(|_| "ANALYSIS_INSTALL: Cannot stage the tagger runtime")?;
    let dir = stage.path().to_owned();
    fs::write(dir.join("requirements.txt"), REQUIREMENTS)
        .map_err(|_| "ANALYSIS_INSTALL: Cannot write the package lock")?;
    status("python-environment", 0, 0, None);
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let uv = find_uv(preferred_uv)?;
        let mut venv = Command::new(&uv);
        venv.args([
            "--no-config",
            "venv",
            "--python",
            "3.12",
            "--managed-python",
            "--relocatable",
        ])
        .arg(dir.join("venv"));
        step(venv)?;
        status("packages", 0, 0, None);
        let mut pip = Command::new(&uv);
        pip.args([
            "--no-config",
            "pip",
            "install",
            "--require-hashes",
            "--python",
        ])
        .arg(python(&dir))
        .args([
            "--index-url",
            "https://pypi.org/simple",
            "--index-strategy",
            "first-index",
            "-r",
        ])
        .arg(dir.join("requirements.txt"));
        step(pip)?;
        status("check", 0, 0, None);
        let mut check = Command::new(python(&dir));
        check.args(["-s", "-E", "-c", "import onnxruntime, numpy, PIL"]);
        step(check)
    })
    .await
    .map_err(|_| "ANALYSIS_INSTALL: Install worker failed")??;
    check_cancel()?;
    fs::write(stage.path().join("installed.txt"), receipt())
        .map_err(|_| "ANALYSIS_INSTALL: Cannot write the install receipt")?;
    let staged = stage.keep();
    fs::rename(&staged, &target)
        .map_err(|_| "ANALYSIS_INSTALL: Cannot publish the tagger runtime")?;
    Ok(())
}

async fn download(
    url: &str,
    path: &Path,
    sha: &str,
    size: u64,
    done: u64,
    total: u64,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(3600))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || attempt.url().scheme() != "https" {
                attempt.error("Tagger download redirect refused")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|_| "ANALYSIS_DOWNLOAD: Cannot create the download client")?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| {
            "ANALYSIS_DOWNLOAD: Could not reach Hugging Face; check the connection and try again"
        })?
        .error_for_status()
        .map_err(|e| {
            format!(
                "ANALYSIS_DOWNLOAD: Hugging Face refused the download ({})",
                e.status().map_or(0, |s| s.as_u16())
            )
        })?;
    let part = path.with_extension("part");
    let mut file = fs::File::create(&part)
        .map_err(|_| "ANALYSIS_DOWNLOAD: Cannot write into the models folder")?;
    let mut got = 0u64;
    loop {
        check_cancel()?;
        let chunk = tokio::select! {
            r = response.chunk() => r.map_err(|_| "ANALYSIS_DOWNLOAD: The download was interrupted; try again")?,
            _ = tokio::time::sleep(Duration::from_millis(200)) => continue,
        };
        let Some(chunk) = chunk else { break };
        got += chunk.len() as u64;
        if got > size {
            return Err("ANALYSIS_DOWNLOAD: The file is larger than expected".into());
        }
        file.write_all(&chunk)
            .map_err(|_| "ANALYSIS_DOWNLOAD: Cannot write the tagger (disk full?)")?;
        status("downloading", done + got, total, None);
    }
    file.sync_all()
        .map_err(|_| "ANALYSIS_DOWNLOAD: Cannot save the tagger")?;
    drop(file);
    if sha256_file(&part)? != sha {
        let _ = fs::remove_file(&part);
        return Err("ANALYSIS_DOWNLOAD: The download did not match its checksum; try again".into());
    }
    fs::rename(&part, path).map_err(|_| "ANALYSIS_DOWNLOAD: Cannot save the tagger".into())
}

/// Downloads the default WD v3 tagger into `<models>/taggers/` unless one is already found.
async fn download_default(models: &Path) -> Result<(), String> {
    let dir = models.join("taggers").join(WD_DIR);
    fs::create_dir_all(&dir)
        .map_err(|_| "ANALYSIS_DOWNLOAD: Cannot create the taggers folder in the models folder")?;
    let total: u64 = WD_FILES.iter().map(|(_, _, s)| s).sum();
    let mut done = 0;
    for (name, sha, size) in WD_FILES {
        let path = dir.join(name);
        if !(path.is_file() && fs::metadata(&path).is_ok_and(|m| m.len() == *size)) {
            let url = format!("https://huggingface.co/{WD_REPO}/resolve/{WD_REVISION}/{name}");
            download(&url, &path, sha, *size, done, total).await?;
        }
        done += size;
    }
    Ok(())
}

// ── discovery ───────────────────────────────────────────────────────────────

#[derive(Clone, Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tagger {
    /// Stable id: the model path.
    pub id: String,
    pub name: String,
    pub model: String,
    pub labels: String,
    /// Where it was found: "deas", "wd14-node" or "animetimm".
    pub source: String,
    pub general_threshold: f64,
    pub character_threshold: f64,
}

fn thresholds(name: &str) -> (f64, f64) {
    // PromptForge's measured values: WD's 0.35/0.85 hid animetimm characters it was 51-85% sure of.
    if name.to_ascii_lowercase().contains("animetimm") || name.contains("dbv4") {
        (0.38, 0.51)
    } else {
        (0.35, 0.85)
    }
}

fn pretty(stem: &str) -> String {
    let s = stem.to_ascii_lowercase();
    let known = [
        ("wd-eva02-large-tagger-v3", "WD v3 EVA02-Large"),
        ("wd-vit-large-tagger-v3", "WD v3 ViT-Large"),
        ("wd-vit-tagger-v3", "WD v3 ViT"),
        ("wd-swinv2-tagger-v3", "WD v3 SwinV2"),
        ("wd-convnext-tagger-v3", "WD v3 ConvNeXt"),
        ("wd-v1-4-moat-tagger-v2", "WD v1.4 MOAT"),
        ("wd-v1-4-swinv2-tagger-v2", "WD v1.4 SwinV2"),
        ("wd-v1-4-convnextv2-tagger-v2", "WD v1.4 ConvNeXtV2"),
        ("convnextv2_huge.dbv4-full", "animetimm ConvNeXtV2-Huge"),
        ("caformer_s36.dbv4-full", "animetimm CAFormer-S36"),
    ];
    known
        .iter()
        .find(|(k, _)| s.contains(k))
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| stem.to_string())
}

fn push(out: &mut Vec<Tagger>, model: PathBuf, labels: PathBuf, label_stem: &str, source: &str) {
    if !model.is_file() || !labels.is_file() {
        return;
    }
    let id = model.to_string_lossy().to_string();
    if out.iter().any(|t| t.id == id) {
        return;
    }
    let name = pretty(label_stem);
    let (g, c) = thresholds(label_stem);
    out.push(Tagger {
        id,
        name,
        model: model.to_string_lossy().into(),
        labels: labels.to_string_lossy().into(),
        source: source.into(),
        general_threshold: g,
        character_threshold: c,
    });
}

/// `<file>.onnx` with `<file>.csv` beside it, or `model.onnx` with `selected_tags.csv`.
fn scan_dir(out: &mut Vec<Tagger>, dir: &Path, source: &str, depth: usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            if depth > 0
                && !path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                scan_dir(out, &path, source, depth - 1);
            }
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("onnx") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let folder = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if stem == "model" {
            push(
                out,
                path.clone(),
                path.with_file_name("selected_tags.csv"),
                &folder,
                source,
            );
        } else {
            push(out, path.clone(), path.with_extension("csv"), &stem, source);
        }
    }
}

/// Every tagger in these model folders, DEAS's own first.
pub fn discover(models: &[PathBuf]) -> Vec<Tagger> {
    let mut out = vec![];
    for m in models {
        scan_dir(&mut out, &m.join("taggers"), "deas", 2);
    }
    for m in models {
        if let Some(comfy) = m.parent() {
            scan_dir(
                &mut out,
                &comfy
                    .join("custom_nodes")
                    .join("comfyui-wd14-tagger")
                    .join("models"),
                "wd14-node",
                0,
            );
        }
        scan_dir(&mut out, &m.join("animetimm"), "animetimm", 1);
    }
    out
}

fn model_roots(models: &[String]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = vec![];
    for m in models {
        let p = PathBuf::from(m.trim());
        if p.is_absolute() && p.is_dir() && !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

// ── running ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TagResult {
    pub tagger: String,
    pub rating: std::collections::BTreeMap<String, f64>,
    pub general: Vec<(String, f64)>,
    pub character: Vec<(String, f64)>,
    pub ms: u64,
}

/// The worker beside the runtime, rewritten whenever this DEAS carries a different one.
fn worker_path() -> Result<PathBuf, String> {
    let path = root().join("tagger_worker.py");
    if fs::read_to_string(&path).ok().as_deref() != Some(WORKER) {
        fs::write(&path, WORKER).map_err(|_| "ANALYSIS_RUNTIME: Cannot write the tagger script")?;
    }
    Ok(path)
}

fn parse_output(stdout: &[u8], name: &str, ms: u64) -> Result<TagResult, String> {
    #[derive(Deserialize)]
    struct Raw {
        rating: std::collections::BTreeMap<String, f64>,
        general: Vec<(String, f64)>,
        character: Vec<(String, f64)>,
    }
    let raw: Raw = serde_json::from_slice(stdout).map_err(|_| {
        "ANALYSIS_RUN: The tagger returned something unreadable; see Logs".to_string()
    })?;
    Ok(TagResult {
        tagger: name.into(),
        rating: raw.rating,
        general: raw.general,
        character: raw.character,
        ms,
    })
}

pub async fn tag(
    tagger: &Tagger,
    image: &Path,
    general: f64,
    character: f64,
) -> Result<TagResult, String> {
    if !runtime_ready() {
        return Err("ANALYSIS_RUNTIME: The tagger runtime is not installed yet. Press Set up taggers in Media Analysis.".into());
    }
    if !image.is_file() {
        return Err("ANALYSIS_RUN: That picture no longer exists".into());
    }
    if !(0.0..=1.0).contains(&general) || !(0.0..=1.0).contains(&character) {
        return Err("ANALYSIS_RUN: Thresholds go from 0 to 1".into());
    }
    let _permit = RUNNING
        .acquire()
        .await
        .map_err(|_| "ANALYSIS_RUN: Tagger busy")?;
    let worker = worker_path()?;
    let mut command = Command::new(python(&runtime_dir()));
    command
        .args(["-s", "-E"])
        .arg(worker)
        .arg("--model")
        .arg(&tagger.model)
        .arg("--labels")
        .arg(&tagger.labels)
        .arg("--image")
        .arg(image)
        .args([
            "--general",
            &general.to_string(),
            "--character",
            &character.to_string(),
        ]);
    quiet(&mut command);
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let started = std::time::Instant::now();
    let child = tokio::process::Command::from(command)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("ANALYSIS_RUN: Could not start the tagger ({e})"))?;
    let out = tokio::time::timeout(RUN_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| "ANALYSIS_RUN: The tagger took over five minutes and was stopped".to_string())?
        .map_err(|e| format!("ANALYSIS_RUN: The tagger failed ({e})"))?;
    let ms = started.elapsed().as_millis() as u64;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let last = private(
            err.lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("no output"),
        );
        crate::logs::push(
            crate::logs::Source::Deas,
            &format!("ERROR: Media Analysis: {} failed: {last}", tagger.name),
        );
        return Err(format!(
            "ANALYSIS_RUN: {} could not read the picture: {last}",
            tagger.name
        ));
    }
    parse_output(&out.stdout, &tagger.name, ms)
}

/// A worker error line without file paths: they carry the Windows user name.
fn private(line: &str) -> String {
    let is_path =
        |w: &str| w.contains(":\\") || w.contains(":/") || w.starts_with('/') || w.contains("\\\\");
    let words = |s: &str| {
        s.split(' ')
            .map(|w| if is_path(w) { "<file>" } else { w })
            .collect::<Vec<_>>()
            .join(" ")
    };
    // Quoted paths first: they may hold spaces ("C:\Users\Some One\...").
    let mut out = String::new();
    let mut rest = line;
    while let Some(i) = rest.find(['\'', '"']) {
        let quote = &rest[i..i + 1];
        out.push_str(&words(&rest[..i]));
        let after = &rest[i + 1..];
        match after.find(quote) {
            Some(j) => {
                let inner = &after[..j];
                out.push_str(quote);
                out.push_str(if is_path(inner) { "<file>" } else { inner });
                out.push_str(quote);
                rest = &after[j + 1..];
            }
            None => {
                out.push_str(quote);
                rest = after;
            }
        }
    }
    out.push_str(&words(rest));
    out.chars().take(400).collect()
}

/// "All taggers": every tag any of them found, at its highest score (PromptForge's merge).
pub fn merge(results: &[TagResult]) -> TagResult {
    let mut general: Vec<(String, f64)> = vec![];
    let mut character: Vec<(String, f64)> = vec![];
    let mut rating = std::collections::BTreeMap::new();
    for r in results {
        for (into, from) in [(&mut general, &r.general), (&mut character, &r.character)] {
            for (tag, p) in from {
                match into.iter_mut().find(|(t, _)| t == tag) {
                    Some(have) => have.1 = have.1.max(*p),
                    None => into.push((tag.clone(), *p)),
                }
            }
        }
        for (k, v) in &r.rating {
            let e = rating.entry(k.clone()).or_insert(0.0f64);
            *e = e.max(*v);
        }
    }
    general.sort_by(|a, b| b.1.total_cmp(&a.1));
    character.sort_by(|a, b| b.1.total_cmp(&a.1));
    TagResult {
        tagger: results
            .iter()
            .map(|r| r.tagger.as_str())
            .collect::<Vec<_>>()
            .join(" + "),
        rating,
        general,
        character,
        ms: results.iter().map(|r| r.ms).sum(),
    }
}

// ── commands ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub runtime_ready: bool,
    pub install: InstallStatus,
    pub taggers: Vec<Tagger>,
    /// The default tagger is missing and can be downloaded (about 1.3 GB).
    pub default_missing: bool,
    pub default_size: u64,
}

#[tauri::command]
pub fn native_analysis_overview(models: Vec<String>) -> Overview {
    let taggers = discover(&model_roots(&models));
    Overview {
        runtime_ready: runtime_ready(),
        install: STATUS.lock().map(|s| s.clone()).unwrap_or(InstallStatus {
            running: false,
            stage: String::new(),
            downloaded: 0,
            total: 0,
            error: None,
        }),
        default_missing: !taggers
            .iter()
            .any(|t| t.model.to_ascii_lowercase().contains("eva02")),
        default_size: WD_FILES.iter().map(|(_, _, s)| s).sum(),
        taggers,
    }
}

/// Installs the tagger runtime and, when no EVA02 is found, downloads it into the first models folder.
#[tauri::command]
pub async fn native_analysis_setup(
    engine: tauri::State<'_, crate::install::Engine>,
    models: Vec<String>,
) -> Result<Overview, String> {
    let roots = model_roots(&models);
    let first = roots.first().cloned().ok_or(
        "ANALYSIS_SETUP: No models folder is set. Choose one under Backend → Folders first.",
    )?;
    let _permit = INSTALLING
        .try_acquire()
        .map_err(|_| "ANALYSIS_BUSY: Tagger setup is already running")?;
    CANCEL.store(false, Ordering::Relaxed);
    status("starting", 0, 0, None);
    let managed_uv = engine.approved_plan().map(|plan| {
        PathBuf::from(plan.request.storage_path).join(if cfg!(windows) {
            "runtime/uv.exe"
        } else {
            "runtime/uv"
        })
    });
    let result = async {
        install_runtime(managed_uv).await?;
        if !discover(&roots)
            .iter()
            .any(|t| t.model.to_ascii_lowercase().contains("eva02"))
        {
            download_default(&first).await?;
        }
        Ok::<(), String>(())
    }
    .await;
    match &result {
        Ok(()) => status("completed", 0, 0, None),
        Err(e) if CANCEL.load(Ordering::Relaxed) => status("cancelled", 0, 0, Some(e.clone())),
        Err(e) => {
            crate::logs::push(
                crate::logs::Source::Deas,
                &format!("ERROR: Media Analysis setup: {e}"),
            );
            status("failed", 0, 0, Some(e.clone()))
        }
    }
    result?;
    Ok(native_analysis_overview(models))
}

/// A pasted or picked picture has no path on disk: it is kept in the runtime's inbox
/// (the 20 newest only) so a tagger can read it.
#[tauri::command]
pub fn native_analysis_stage(bytes: Vec<u8>, ext: String) -> Result<String, String> {
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp") {
        return Err("ANALYSIS_STAGE: Pictures must be PNG, JPEG or WebP".into());
    }
    if bytes.is_empty() || bytes.len() > 64 << 20 {
        return Err("ANALYSIS_STAGE: Pictures must be under 64 MB".into());
    }
    let picture = bytes.starts_with(b"\x89PNG")
        || bytes.starts_with(&[0xff, 0xd8, 0xff])
        || (bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP");
    if !picture {
        return Err("ANALYSIS_STAGE: That file is not a readable picture".into());
    }
    let inbox = root().join("inbox");
    fs::create_dir_all(&inbox).map_err(|_| "ANALYSIS_STAGE: Cannot create the inbox folder")?;
    let mut old: Vec<_> = fs::read_dir(&inbox)
        .map(|r| {
            r.flatten()
                .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
                .collect()
        })
        .unwrap_or_default();
    old.sort();
    for (_, p) in old.iter().rev().skip(19) {
        let _ = fs::remove_file(p);
    }
    let path = inbox.join(format!("{}.{ext}", uuid::Uuid::new_v4()));
    fs::write(&path, bytes).map_err(|_| "ANALYSIS_STAGE: Cannot save the picture")?;
    Ok(path.to_string_lossy().into())
}

#[tauri::command]
pub fn native_analysis_cancel() {
    CANCEL.store(true, Ordering::Relaxed);
}

/// Tags one picture with one tagger, or with every tagger found ("all", merged).
#[tauri::command]
pub async fn native_analysis_tag(
    models: Vec<String>,
    tagger: String,
    image: String,
    general: Option<f64>,
    character: Option<f64>,
) -> Result<TagResult, String> {
    let found = discover(&model_roots(&models));
    let image = PathBuf::from(image);
    if tagger == "all" {
        if found.is_empty() {
            return Err("ANALYSIS_RUN: No taggers found".into());
        }
        let mut results = vec![];
        for t in &found {
            results.push(tag(t, &image, t.general_threshold, t.character_threshold).await?);
        }
        return Ok(merge(&results));
    }
    let t = found
        .iter()
        .find(|t| t.id == tagger)
        .ok_or("ANALYSIS_RUN: That tagger is no longer in the models folder")?;
    tag(
        t,
        &image,
        general.unwrap_or(t.general_threshold),
        character.unwrap_or(t.character_threshold),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    #[test]
    fn finds_taggers_where_people_keep_them_and_needs_their_tag_list() {
        let tmp = tempfile::tempdir().unwrap();
        let models = tmp.path().join("ComfyUI").join("models");
        // DEAS's own download layout.
        touch(&models.join("taggers/wd-eva02-large-tagger-v3/model.onnx"));
        touch(&models.join("taggers/wd-eva02-large-tagger-v3/selected_tags.csv"));
        // The WD14 Tagger node's flat layout.
        let node = tmp
            .path()
            .join("ComfyUI/custom_nodes/comfyui-wd14-tagger/models");
        touch(&node.join("wd-v1-4-moat-tagger-v2.onnx"));
        touch(&node.join("wd-v1-4-moat-tagger-v2.csv"));
        touch(&node.join("half-copied.onnx")); // no tag list: skipped
                                               // animetimm: PyTorch-only is skipped, an ONNX copy is used.
        touch(&models.join("animetimm/convnextv2_huge.dbv4-full/pytorch_model.bin"));
        touch(&models.join("animetimm/convnextv2_huge.dbv4-full/selected_tags.csv"));
        touch(&models.join("animetimm/caformer_s36.dbv4-full/model.onnx"));
        touch(&models.join("animetimm/caformer_s36.dbv4-full/selected_tags.csv"));
        let found = discover(&[models]);
        let names: Vec<(&str, &str)> = found
            .iter()
            .map(|t| (t.name.as_str(), t.source.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("WD v3 EVA02-Large", "deas"),
                ("WD v1.4 MOAT", "wd14-node"),
                ("animetimm CAFormer-S36", "animetimm"),
            ]
        );
        assert_eq!(
            (found[0].general_threshold, found[0].character_threshold),
            (0.35, 0.85)
        );
        assert_eq!(
            (found[2].general_threshold, found[2].character_threshold),
            (0.38, 0.51)
        );
    }

    #[test]
    fn worker_output_parses_and_all_taggers_merge_at_the_highest_score() {
        let a = parse_output(br#"{"layout":"wd","rating":{"general":0.9},"general":[["1girl",0.99],["cat_ears",0.6]],"character":[]}"#, "A", 10).unwrap();
        let b = parse_output(br#"{"layout":"timm","rating":{"general":0.7},"general":[["cat_ears",0.8],["fangs",0.4]],"character":[["aura_(sousou_no_frieren)",0.6]]}"#, "B", 20).unwrap();
        let m = merge(&[a, b]);
        assert_eq!(
            m.general,
            vec![
                ("1girl".into(), 0.99),
                ("cat_ears".into(), 0.8),
                ("fangs".into(), 0.4)
            ]
        );
        assert_eq!(m.character.len(), 1);
        assert_eq!(m.rating["general"], 0.9);
        assert_eq!(m.tagger, "A + B");
        assert_eq!(m.ms, 30);
        assert!(parse_output(b"Traceback", "A", 1).is_err());
        let leak = r"PIL.UnidentifiedImageError: cannot identify image file 'C:\\Users\\Some One\\AppData\\x.png'";
        assert_eq!(
            private(leak),
            "PIL.UnidentifiedImageError: cannot identify image file '<file>'"
        );
        assert_eq!(
            private("open /home/someone/x.png failed"),
            "open <file> failed"
        );
        assert_eq!(
            private("That file is not a picture the tagger can read"),
            "That file is not a picture the tagger can read"
        );
    }

    #[test]
    fn the_worker_handles_both_layouts() {
        assert!(WORKER.contains("def wd_input") && WORKER.contains("def timm_input"));
        assert!(REQUIREMENTS.contains("onnxruntime==") && REQUIREMENTS.contains("--hash=sha256:"));
    }
}
