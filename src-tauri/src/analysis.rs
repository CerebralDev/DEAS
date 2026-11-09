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
//! Taggers are found where people already keep them, never copied (analysis_taggers.rs).
//! Nothing is downloaded unless the person asks: the WD v3 EVA02 download is offered only
//! when no tagger is found at all, and setup with an existing tagger installs only the runtime.

use crate::downloads::fs::Anchor;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
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

pub(crate) static CANCEL: AtomicBool = AtomicBool::new(false);
pub(crate) static INSTALLING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
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

pub(crate) fn status(stage: &str, downloaded: u64, total: u64, error: Option<String>) {
    if let Ok(mut s) = STATUS.lock() {
        if s.stage != stage && !stage.is_empty() {
            crate::logs::push(
                crate::logs::Source::Deas,
                &format!("INFO: Media Analysis: {stage}"),
            );
        }
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

/// The analysis runtime folder: the tagger venv, its inbox, frame strips and frames.
pub(crate) fn runtime_root() -> PathBuf {
    root()
}

fn runtime_dir() -> PathBuf {
    root().join("taggers")
}

pub(crate) fn python(dir: &Path) -> PathBuf {
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

pub(crate) fn check_cancel() -> Result<(), String> {
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

/// The child environment, whitelisted as install.rs `isolated` does: inherited UV_*/PIP_*/
/// PYTHON* variables, proxies and credentials must not redirect a hash-locked install.
pub(crate) fn quiet(command: &mut Command) {
    command.env_clear();
    for name in [
        "PATH",
        "HOME",
        "USERPROFILE",
        "SYSTEMROOT",
        "SYSTEMDRIVE",
        "WINDIR",
        "COMSPEC",
        "TEMP",
        "TMP",
        "TMPDIR",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .env("PYTHONUTF8", "1")
        .env("UV_NO_CONFIG", "1")
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

/// Sends one CLI line to central Logs (secret-bearing lines dropped, paths and URLs hidden)
/// and returns what was logged.
fn log_line(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    if ["token", "password", "secret", "authorization"]
        .iter()
        .any(|s| lower.contains(s))
    {
        return None;
    }
    let safe: String = private(line).chars().filter(|c| !c.is_control()).collect();
    if safe.trim().is_empty() {
        return None;
    }
    crate::logs::push(crate::logs::Source::Cli, &format!("media-analysis: {safe}"));
    Some(safe)
}

/// Runs a step that can take minutes (`code` prefixes its error): each output line goes to
/// central Logs while it runs and stdout lines also to `on_line`; Cancel kills it mid-way.
/// On failure, the last useful stderr line, without paths.
pub(crate) fn run(
    mut command: Command,
    code: &str,
    on_line: impl Fn(&str) + Send + 'static,
) -> Result<(), String> {
    quiet(&mut command);
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("{code}: Could not start a step ({e})"))?;
    let (out, err) = (child.stdout.take(), child.stderr.take());
    let last = Arc::new(Mutex::new(String::new()));
    let sink = last.clone();
    let readers = [
        std::thread::spawn(move || {
            for line in out
                .into_iter()
                .flat_map(|p| BufReader::new(p).lines().map_while(Result::ok))
            {
                on_line(&line);
                log_line(&line);
            }
        }),
        std::thread::spawn(move || {
            for line in err
                .into_iter()
                .flat_map(|p| BufReader::new(p).lines().map_while(Result::ok))
            {
                if let (Some(safe), Ok(mut l)) = (log_line(&line), sink.lock()) {
                    *l = safe;
                }
            }
        }),
    ];
    let status = loop {
        if CANCEL.load(Ordering::Relaxed) {
            // shortcut: Windows kills only the direct child (no Job object), use crate::win::Job if a step ever spawns children.
            #[cfg(unix)]
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            for r in readers {
                let _ = r.join();
            }
            return check_cancel();
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(e) => return Err(format!("{code}: A step was lost ({e})")),
        }
    };
    for r in readers {
        let _ = r.join();
    }
    if status.success() {
        return Ok(());
    }
    let last = last.lock().map(|l| l.clone()).unwrap_or_default();
    Err(format!(
        "{code}: {}",
        if last.is_empty() {
            "it stopped without saying why"
        } else {
            &last
        }
    ))
}

pub(crate) fn find_uv(preferred: Option<PathBuf>) -> Result<PathBuf, String> {
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
        run(venv, "ANALYSIS_INSTALL", |_| {})?;
        status("packages", 0, 0, None);
        let mut pip = Command::new(&uv);
        pip.args([
            "--no-config",
            "pip",
            "install",
            "--require-hashes",
            "--only-binary",
            ":all:",
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
        run(pip, "ANALYSIS_INSTALL", |_| {})?;
        status("check", 0, 0, None);
        let mut check = Command::new(python(&dir));
        check.args([
            "-s",
            "-E",
            "-X",
            "utf8",
            "-c",
            "import onnxruntime, numpy, PIL",
        ]);
        run(check, "ANALYSIS_INSTALL", |_| {})
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
    let dir = path
        .parent()
        .ok_or("ANALYSIS_DOWNLOAD: Cannot write into the models folder")?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("ANALYSIS_DOWNLOAD: Cannot write into the models folder")?;
    let anchor =
        Anchor::open(dir).map_err(|_| "ANALYSIS_DOWNLOAD: Cannot write into the models folder")?;
    let partial = format!(".{name}.{}.part", uuid::Uuid::new_v4());
    let mut file = anchor
        .partial(&partial)
        .map_err(|_| "ANALYSIS_DOWNLOAD: Cannot write into the models folder")?;
    let written = async {
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
            .map_err(|_| "ANALYSIS_DOWNLOAD: Cannot save the tagger".to_string())
    }
    .await;
    drop(file);
    keep(&anchor, dir, &partial, name, sha, written)
}

/// Publishes a finished `.part` under `name` only when its checksum matches and nothing has
/// that name yet; the part file goes on every failure or cancel.
fn keep(
    anchor: &Anchor,
    dir: &Path,
    partial: &str,
    name: &str,
    sha: &str,
    written: Result<(), String>,
) -> Result<(), String> {
    let result = written
        .and_then(|()| {
            if sha256_file(&dir.join(partial))? == sha {
                Ok(())
            } else {
                Err("ANALYSIS_DOWNLOAD: The download did not match its checksum; try again".into())
            }
        })
        .and_then(|()| {
            anchor.publish(partial, name).map_err(|_| {
                format!("ANALYSIS_DOWNLOAD: A different {name} is already in the taggers folder; DEAS does not replace it. Move it away, then try again")
            })
        });
    if result.is_err() {
        anchor.remove_partial(partial);
    }
    result
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

// ── discovery: see analysis_taggers.rs ─────────────────────────────────────────

use crate::analysis_taggers::discover;
pub use crate::analysis_taggers::{Tagger, Unusable};

pub(crate) fn model_roots(models: &[String]) -> Vec<PathBuf> {
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

/// DEAS's Danbooru tag table, for tag lists that carry no categories (bundled; read once).
fn tag_table() -> Option<&'static crate::forge::tags::TagStore> {
    static TABLE: std::sync::OnceLock<Option<crate::forge::tags::TagStore>> =
        std::sync::OnceLock::new();
    TABLE
        .get_or_init(|| {
            crate::forge::tags::TagStore::open(&root().join("bundled-only/tags.sqlite")).ok()
        })
        .as_ref()
}

/// The tagger's tag list in the one shape the worker reads: `[[tag, kind], ...]` in model
/// output order, kind g/c/r/o. Written once per list version beside the runtime.
fn worker_labels(tagger: &Tagger) -> Result<PathBuf, String> {
    let source = Path::new(&tagger.labels);
    let meta = fs::metadata(source)
        .map_err(|_| "ANALYSIS_RUN: The tagger's tag list is gone from its folder")?;
    let key = Sha256::digest(format!(
        "labels-v1\0{}\0{}\0{:?}",
        tagger.labels,
        meta.len(),
        meta.modified().ok()
    ));
    let path = root().join("labels").join(format!("{key:x}.json"));
    if path.is_file() {
        return Ok(path);
    }
    let labels = crate::analysis_taggers::read_labels(source).map_err(|e| {
        format!(
            "ANALYSIS_RUN: {}'s tag list cannot be used: {e}",
            tagger.name
        )
    })?;
    let table = if labels.kinds.is_none() {
        tag_table()
    } else {
        None
    };
    let kinds = crate::analysis_taggers::resolve_kinds(&labels, table);
    let rows: Vec<(&str, &str)> = labels
        .names
        .iter()
        .zip(&kinds)
        .map(|(n, k)| (n.as_str(), k.code()))
        .collect();
    fs::create_dir_all(path.parent().unwrap())
        .map_err(|_| "ANALYSIS_RUN: Cannot write the tag list for the tagger")?;
    let text =
        serde_json::to_string(&rows).map_err(|_| "ANALYSIS_RUN: Cannot write the tag list")?;
    let temp = path.with_extension("part");
    fs::write(&temp, text)
        .and_then(|_| fs::rename(&temp, &path))
        .map_err(|_| "ANALYSIS_RUN: Cannot write the tag list for the tagger")?;
    Ok(path)
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
    let labels = worker_labels(tagger)?;
    let mut command = Command::new(python(&runtime_dir()));
    command
        .args(["-s", "-E", "-X", "utf8"])
        .arg(worker)
        .arg("--model")
        .arg(&tagger.model)
        .arg("--labels")
        .arg(&labels)
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
    /// Found, but not usable as they are, each with the reason.
    pub unusable: Vec<Unusable>,
    /// The size of the optional WD v3 download, offered only when no tagger is found.
    pub default_size: u64,
    /// What Set up downloads for the tagger runtime, roughly.
    pub runtime_size: u64,
    /// What a conversion downloads for its tools, roughly.
    pub tools_size: u64,
}

/// The tagger runtime's download, roughly (Python, onnxruntime, numpy, Pillow).
const RUNTIME_BYTES: u64 = 300_000_000;

fn overview(models: &[String], scan: bool) -> Overview {
    let found = if scan {
        discover(&model_roots(models))
    } else {
        Default::default()
    };
    Overview {
        runtime_ready: runtime_ready(),
        install: STATUS.lock().map(|s| s.clone()).unwrap_or(InstallStatus {
            running: false,
            stage: String::new(),
            downloaded: 0,
            total: 0,
            error: None,
        }),
        default_size: WD_FILES.iter().map(|(_, _, s)| s).sum(),
        runtime_size: RUNTIME_BYTES,
        tools_size: crate::analysis_convert::TOOLS_BYTES,
        taggers: found.taggers,
        unusable: found.unusable,
    }
}

/// `scan: false` skips the folder scan (empty lists): the cheap read for progress polling.
#[tauri::command]
pub async fn native_analysis_overview(
    models: Vec<String>,
    scan: Option<bool>,
) -> Result<Overview, String> {
    tauri::async_runtime::spawn_blocking(move || overview(&models, scan != Some(false)))
        .await
        .map_err(|_| "ANALYSIS_RUN: Looking for taggers failed".into())
}

/// DEAS's own uv, from the approved install plan.
pub(crate) fn managed_uv(engine: &crate::install::Engine) -> Option<PathBuf> {
    engine.approved_plan().map(|plan| {
        PathBuf::from(plan.request.storage_path).join(if cfg!(windows) {
            "runtime/uv.exe"
        } else {
            "runtime/uv"
        })
    })
}

/// Installs the tagger runtime. With `download`, and only when no tagger is found at all,
/// it also downloads WD v3 EVA02 into the first models folder: never pushed on anyone.
#[tauri::command]
pub async fn native_analysis_setup(
    engine: tauri::State<'_, crate::install::Engine>,
    models: Vec<String>,
    download: Option<bool>,
) -> Result<Overview, String> {
    let roots = model_roots(&models);
    let first = roots.first().cloned();
    if download == Some(true) && first.is_none() {
        return Err(
            "ANALYSIS_SETUP: No models folder is set. Choose one under Backend → Folders first."
                .into(),
        );
    }
    let _permit = INSTALLING
        .try_acquire()
        .map_err(|_| "ANALYSIS_BUSY: Tagger setup is already running")?;
    CANCEL.store(false, Ordering::Relaxed);
    status("starting", 0, 0, None);
    let managed_uv = managed_uv(&engine);
    let result = async {
        install_runtime(managed_uv).await?;
        if let (Some(true), Some(first)) = (download, &first) {
            if discover(&roots).taggers.is_empty() {
                download_default(first).await?;
            }
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
    native_analysis_overview(models, None).await
}

/// A pasted or picked picture has no path on disk: it is kept in the runtime's inbox
/// (the 20 newest only) so a tagger can read it.
#[tauri::command]
pub async fn native_analysis_stage(bytes: Vec<u8>, ext: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || stage(bytes, ext))
        .await
        .map_err(|_| "ANALYSIS_STAGE: Cannot save the picture".to_string())?
}

fn stage(bytes: Vec<u8>, ext: String) -> Result<String, String> {
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

/// Choose…: a picture or a video from disk, by path (nothing copied); `None` on cancel.
#[tauri::command]
pub async fn native_analysis_pick(title: String) -> Result<Option<String>, String> {
    if title.trim().is_empty() || title.len() > 240 || title.chars().any(char::is_control) {
        return Err("ANALYSIS_PICK: Invalid dialog title".into());
    }
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter(
            "Pictures and videos",
            &[
                "png", "jpg", "jpeg", "webp", "mp4", "mov", "m4v", "mkv", "webm",
            ],
        )
        .add_filter("Pictures", &["png", "jpg", "jpeg", "webp"])
        .add_filter("Videos", &["mp4", "mov", "m4v", "mkv", "webm"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    file.path()
        .to_str()
        .map(|p| Some(p.to_string()))
        .ok_or_else(|| "ANALYSIS_PICK: That file's name cannot be read".into())
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
    let found = discover(&model_roots(&models)).taggers;
    let image = PathBuf::from(image);
    if tagger == "all" {
        if found.is_empty() {
            return Err("ANALYSIS_RUN: No taggers found".into());
        }
        let mut results = vec![];
        let mut seen = vec![];
        for t in &found {
            // The same tagger in two folders is read once.
            let same = (fs::metadata(&t.model).map(|m| m.len()).unwrap_or(0), t.tags);
            if seen.contains(&same) {
                continue;
            }
            seen.push(same);
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
    fn a_download_never_replaces_a_file_and_leaves_no_part_behind() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let anchor = Anchor::open(dir).unwrap();
        let sha = format!("{:x}", Sha256::digest(b"new"));
        let part = |n: &str| {
            let mut f = anchor.partial(n).unwrap();
            f.write_all(b"new").unwrap();
        };
        let names = || {
            let mut n: Vec<String> = fs::read_dir(dir)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into())
                .collect();
            n.sort();
            n
        };
        // Interrupted or cancelled: the part goes.
        part(".a.part");
        assert!(keep(
            &anchor,
            dir,
            ".a.part",
            "model.onnx",
            &sha,
            Err("cancel".into())
        )
        .is_err());
        // Wrong checksum: the part goes.
        part(".b.part");
        assert!(keep(&anchor, dir, ".b.part", "model.onnx", "00", Ok(())).is_err());
        assert!(names().is_empty());
        // An existing file of that name stays as it was.
        fs::write(dir.join("model.onnx"), b"old").unwrap();
        part(".c.part");
        let err = keep(&anchor, dir, ".c.part", "model.onnx", &sha, Ok(())).unwrap_err();
        assert!(err.contains("does not replace it"), "{err}");
        assert_eq!(fs::read(dir.join("model.onnx")).unwrap(), b"old");
        assert_eq!(names(), ["model.onnx"]);
        // Free name: published.
        part(".d.part");
        keep(&anchor, dir, ".d.part", "selected_tags.csv", &sha, Ok(())).unwrap();
        assert_eq!(fs::read(dir.join("selected_tags.csv")).unwrap(), b"new");
        assert_eq!(names(), ["model.onnx", "selected_tags.csv"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_step_streams_its_lines_and_fails_with_its_last_error_line() {
        let seen = Arc::new(Mutex::new(vec![]));
        let sink = seen.clone();
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "echo STAGE exporting; echo 'cannot open /home/someone/x.bin' >&2; exit 3",
        ]);
        let err = run(command, "ANALYSIS_TEST", move |l| {
            sink.lock().unwrap().push(l.to_string())
        })
        .unwrap_err();
        assert_eq!(err, "ANALYSIS_TEST: cannot open <file>");
        assert_eq!(*seen.lock().unwrap(), ["STAGE exporting"]);
    }

    #[test]
    fn children_get_no_inherited_uv_or_pip_settings() {
        let mut command = Command::new("uv");
        command.env("PIP_INDEX_URL", "https://evil.example/simple");
        command.env("UV_INDEX_URL", "https://evil.example/simple");
        quiet(&mut command);
        let keys: Vec<String> = command
            .get_envs()
            .filter(|(_, v)| v.is_some())
            .map(|(k, _)| k.to_string_lossy().into())
            .collect();
        assert!(!keys.iter().any(|k| k.starts_with("PIP_")), "{keys:?}");
        assert!(
            keys.iter().filter(|k| k.starts_with("UV_")).all(|k| [
                "UV_NO_CONFIG",
                "UV_CACHE_DIR",
                "UV_PYTHON_INSTALL_DIR"
            ]
            .contains(&k.as_str())),
            "{keys:?}"
        );
        assert!(keys.contains(&"PYTHONUTF8".to_string()));
    }

    #[test]
    fn the_worker_handles_both_layouts() {
        assert!(WORKER.contains("def wd_input") && WORKER.contains("def timm_input"));
        assert!(REQUIREMENTS.contains("onnxruntime==") && REQUIREMENTS.contains("--hash=sha256:"));
    }
}
