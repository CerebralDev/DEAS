//! Opt-in isolated audio runtimes; discovery never imports Torch or loads a model.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
const MODEL: &str = "mbr-win10-sink8.ckpt";
const REVISION: &str = "d9642ed592120dca935ebb783c77f2faa1e6c32b";
const SOURCE_REVISION: &str = "cc43c280518ee5ebab7826d8efe3b5a342df8857";
const MODEL_SHA: &str = "7afe79947745faf8cddd650afc822aa7c387882047e65fe65f136f539365a656";
const SOURCES: &[(&str, &str)] = &[
    (
        "main.py",
        "49cf17862ca45e3089ea66b915b6d9d7488550a80c3bd9b3e81e7889e15e6a86",
    ),
    (
        "model.py",
        "391f188e8f071aeeabd33d5881b8a955091d49488066811ee4ab434f8e09d7e3",
    ),
    (
        "modules.py",
        "5aecf4732ebceb204684b0162a5bdecb79876b48ab8cc221d92932503cc696c6",
    ),
    (
        "flex_attention_utils.py",
        "ac7a3f54c790eb24bc2c1baf4efd953f22393d119eaa81cddc9096895cccb875",
    ),
    (
        "LICENSE",
        "5e2d5d73d953223454aeaa2c819edc8c95b4082caad92186ca81dac22f5b7f4e",
    ),
];
#[cfg(not(windows))]
const REQUIREMENTS: &str = include_str!("audio_runtime_requirements.txt");
#[cfg(windows)]
const REQUIREMENTS: &str = include_str!("audio_runtime_requirements_windows.txt");
#[cfg(not(windows))]
const GPU_REQUIREMENTS: &str = include_str!("audio_gpu_requirements.txt");
// The Windows provisioner is a no-op until a checked GPU profile exists there.
#[cfg_attr(windows, allow(dead_code))]
const GPU_BOOTSTRAP: &str = include_str!("audio_gpu_bootstrap.py");
fn receipt(mode: &str) -> String {
    if mode == "deep" {
        return "deep-filter=0.5.6".into();
    }
    format!(
        "smule-windowed-roformer={SOURCE_REVISION};model={REVISION}:{MODEL_SHA};requirements={:x}",
        Sha256::digest(REQUIREMENTS.as_bytes())
    )
}
static CANCEL: AtomicBool = AtomicBool::new(false);
static INSTALLING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
static STATUS: Mutex<Status> = Mutex::new(Status {
    running: false,
    mode: String::new(),
    stage: String::new(),
    downloaded: 0,
    total: 0,
});
#[derive(Clone, Serialize)]
pub struct Status {
    running: bool,
    mode: String,
    stage: String,
    downloaded: u64,
    total: u64,
}
fn status(mode: &str, stage: &str, downloaded: u64, total: u64) {
    if let Ok(mut s) = STATUS.lock() {
        *s = Status {
            running: stage != "completed" && stage != "failed" && stage != "cancelled",
            mode: mode.into(),
            stage: stage.into(),
            downloaded,
            total,
        };
    }
}
pub(crate) fn root() -> PathBuf {
    if let Some(path) = std::env::var_os("DEAS_AUDIO_RUNTIME_DIR") {
        return PathBuf::from(path);
    }
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    base.unwrap_or_default().join("deas/audio-runtime")
}
fn deep_asset() -> Result<(&'static str, &'static str), String> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Ok((
            "deep-filter-0.5.6-x86_64-unknown-linux-musl",
            "70775e251eee44c0f2451a1e833326cf8bcbbe304d3e7cd12851e6fce72ef7da",
        ))
    } else if cfg!(all(windows, target_arch = "x86_64")) {
        Ok((
            "deep-filter-0.5.6-x86_64-pc-windows-msvc.exe",
            "75e11fa16445f560cb6b021521ddb89e89270d13b83089705d98776f58fd7915",
        ))
    } else {
        Err("AUDIO_PLATFORM: Audio runtimes support x86_64 Linux and Windows".into())
    }
}
fn runtime_dir(mode: &str) -> PathBuf {
    root().join(if mode == "stems" { "smule-stems" } else { mode })
}
fn snapshot(dir: &Path) -> PathBuf {
    dir.join("smule")
}
fn python(dir: &Path) -> PathBuf {
    dir.join(if cfg!(windows) {
        "venv/Scripts/python.exe"
    } else {
        "venv/bin/python"
    })
}
/// GPU bindings are optional: installed only when an owned DEAS environment with CUDA
/// Torch existed at install time. `DEAS_AUDIO_GPU_PYTHON` is an operator-trusted
/// acceptance override at the same level as `DEAS_AUDIO_RUNTIME_DIR`, never frontend input.
struct GpuBinding {
    python: PathBuf,
    abi: String,
    site: PathBuf,
}
fn gpu_binding() -> Option<GpuBinding> {
    let dir = runtime_dir("stems");
    let raw = fs::read_to_string(dir.join("gpu-bound.json")).ok()?;
    let bound: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let abi = bound["abi"].as_str()?;
    // Relative to the mode folder so publishing/renames cannot invalidate the binding.
    let rel = bound["siteRel"].as_str()?;
    let site = dir.join(rel);
    if abi.is_empty()
        || abi.chars().any(char::is_control)
        || rel.is_empty()
        || PathBuf::from(rel).is_absolute()
        || PathBuf::from(rel).components().count()
            != PathBuf::from(rel)
                .components()
                .filter(|c| matches!(c, std::path::Component::Normal(_)))
                .count()
        || !site.starts_with(&dir)
        || !safe_directory(&site)
        || !regular(&dir.join("gpu_bootstrap.py"))
        || !safe_directory(&dir.join("gpu-extras"))
    {
        return None;
    }
    let python = crate::resources::owned_venv_python().or_else(|| {
        std::env::var_os("DEAS_AUDIO_GPU_PYTHON")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && regular(p))
    })?;
    Some(GpuBinding {
        python,
        abi: abi.to_owned(),
        site,
    })
}
pub(crate) fn gpu_ready() -> bool {
    gpu_binding().is_some()
}
fn resolve_device(requested: Option<&str>) -> Result<(&'static str, Option<GpuBinding>), String> {
    match requested {
        None | Some("auto") => match gpu_binding() {
            Some(binding) => Ok(("cuda", Some(binding))),
            None => Ok(("cpu", None)),
        },
        Some("cpu") => Ok(("cpu", None)),
        Some("cuda") => gpu_binding()
            .map(|binding| ("cuda", Some(binding)))
            .ok_or("AUDIO_DEVICE: CUDA requires stems installed alongside a compatible owned DEAS environment; select cpu or reinstall stems with that environment".into()),
        Some(_) => Err("AUDIO_DEVICE: Select auto, cpu or cuda".into()),
    }
}
fn regular(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
}
fn safe_directory(path: &Path) -> bool {
    path.is_absolute()
        && fs::symlink_metadata(path).is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink())
}
pub(crate) fn available(mode: &str) -> bool {
    let dir = runtime_dir(mode);
    let installed = fs::read_to_string(dir.join("installed.txt")).unwrap_or_default();
    // Retain already installed native DeepFilterNet; its binary is verified before inference.
    let compatible = installed == receipt(mode)
        || (mode == "deep" && installed.starts_with("deep-filter=0.5.6;"));
    if !safe_directory(&dir) || !compatible {
        return false;
    }
    match mode {
        "deep" => regular(&dir.join("deep-filter.exe")),
        "stems" => {
            python(&dir).is_file()
                && regular(&snapshot(&dir).join(MODEL))
                && SOURCES
                    .iter()
                    .all(|(name, _)| regular(&snapshot(&dir).join(name)))
        }
        _ => false,
    }
}
fn verify(path: &Path, expected: &str) -> Result<(), String> {
    if !regular(path) {
        return Err("AUDIO_INTEGRITY: Missing or unsafe runtime artifact".into());
    }
    let mut file = fs::File::open(path).map_err(|_| "AUDIO_INTEGRITY: Cannot read artifact")?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|_| "AUDIO_INTEGRITY: Cannot hash artifact")?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    if format!("{:x}", hash.finalize()) != expected {
        return Err(
            "AUDIO_INTEGRITY: Runtime/model checksum mismatch; reinstall explicitly".into(),
        );
    }
    Ok(())
}
fn check_cancel() -> Result<(), String> {
    if CANCEL.load(Ordering::Relaxed) {
        Err("AUDIO_CANCELLED: Installation cancelled".into())
    } else {
        Ok(())
    }
}
async fn download(mode: &str, url: &str, path: &Path, sha: &str, limit: u64) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(3600))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || attempt.url().scheme() != "https" {
                attempt.error("Audio artifact redirect refused")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|_| "AUDIO_INSTALL: Cannot create download client")?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "AUDIO_INSTALL: Download connection failed")?
        .error_for_status()
        .map_err(|_| "AUDIO_INSTALL: Download rejected")?;
    let total = response.content_length().unwrap_or(0);
    if total > limit {
        return Err("AUDIO_INSTALL: Artifact exceeds expected size".into());
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "AUDIO_INSTALL: Cannot create artifact")?;
    let mut size = 0;
    loop {
        check_cancel()?;
        let chunk = tokio::select! { result=response.chunk()=>result.map_err(|_|"AUDIO_INSTALL: Download interrupted")?, _=tokio::time::sleep(Duration::from_millis(200))=>{check_cancel()?; continue;} };
        let Some(chunk) = chunk else { break };
        size += chunk.len() as u64;
        if size > limit {
            return Err("AUDIO_INSTALL: Artifact exceeds expected size".into());
        }
        file.write_all(&chunk)
            .map_err(|_| "AUDIO_INSTALL: Cannot write artifact")?;
        status(mode, "downloading", size, total);
    }
    file.sync_all()
        .map_err(|_| "AUDIO_INSTALL: Cannot sync artifact")?;
    verify(path, sha)
}
struct OwnedCli {
    child: std::process::Child,
    #[cfg(windows)]
    job: crate::win::Job,
}
impl std::ops::Deref for OwnedCli {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.child
    }
}
impl std::ops::DerefMut for OwnedCli {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}
impl Drop for OwnedCli {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        #[cfg(windows)]
        self.job.kill();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn run(mut command: Command, cancel: &AtomicBool) -> Result<(), String> {
    use std::process::Stdio;
    command
        .env("OMP_NUM_THREADS", "1")
        .env("MKL_NUM_THREADS", "1")
        .env("PYTHONNOUSERSITE", "1")
        .env("UV_CACHE_DIR", root().join("uv-cache"))
        .env("UV_PYTHON_INSTALL_DIR", root().join("python"));
    let mut diagnostics =
        tempfile::tempfile_in(root()).map_err(|_| "AUDIO_RUNTIME: Cannot create diagnostics")?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            diagnostics
                .try_clone()
                .map_err(|_| "AUDIO_RUNTIME: Cannot capture diagnostics")?,
        ))
        .stderr(Stdio::from(
            diagnostics
                .try_clone()
                .map_err(|_| "AUDIO_RUNTIME: Cannot capture diagnostics")?,
        ));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        #[cfg(target_os = "linux")]
        unsafe {
            let parent = libc::getpid();
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    libc::_exit(1);
                }
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(crate::win::CREATE_NO_WINDOW);
    }
    #[cfg(windows)]
    let job = crate::win::Job::new()?;
    #[allow(unused_mut)] // Windows assignment failure must terminate the unowned child.
    let mut child = command.spawn().map_err(|_| {
        "AUDIO_RUNTIME: Cannot launch CLI; install uv on PATH for provisioning, then restart DEAS"
    })?;
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        if let Err(e) = job.assign(child.as_raw_handle()) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    }
    #[allow(unused_mut)] // Windows waits for the process handle before Job cleanup.
    let mut child = OwnedCli {
        child,
        #[cfg(windows)]
        job,
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(3600);
    let result: Result<(), String> = loop {
        if cancel.load(Ordering::Relaxed) {
            break Err("AUDIO_CANCELLED: Audio operation cancelled".into());
        }
        if std::time::Instant::now() > deadline {
            break Err("AUDIO_RUNTIME: CLI exceeded one-hour deadline".into());
        }
        if diagnostics
            .metadata()
            .map_or(true, |m| m.len() > 1024 * 1024)
        {
            break Err("AUDIO_RUNTIME: CLI diagnostics exceeded 1 MiB".into());
        }
        #[cfg(unix)]
        {
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            if unsafe {
                libc::waitid(
                    libc::P_PID,
                    child.id(),
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            } != 0
            {
                break Err("AUDIO_RUNTIME: Cannot monitor CLI".into());
            }
            if unsafe { info.si_pid() } != 0 {
                break if info.si_code == libc::CLD_EXITED && unsafe { info.si_status() } == 0 {
                    Ok(())
                } else {
                    Err("AUDIO_RUNTIME: CLI failed; verify runtime dependencies, input and available memory".into())
                };
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            match crate::win::exited(child.as_raw_handle()){Ok(true)=>break child.wait().map_err(|_|"AUDIO_RUNTIME: Cannot read CLI status".into()).and_then(|s|if s.success(){Ok(())}else{Err("AUDIO_RUNTIME: CLI failed; verify runtime dependencies, input and available memory".into())}),Err(_)=>break Err("AUDIO_RUNTIME: Cannot monitor CLI".into()),_=>()}
        }
        std::thread::sleep(Duration::from_millis(30));
    };
    drop(child);
    if let Err(error) = result {
        if error.starts_with("AUDIO_CANCELLED:") {
            return Err(error);
        }
        let size = diagnostics.metadata().map_or(0, |m| m.len());
        let _ = diagnostics.seek(SeekFrom::Start(size.saturating_sub(4096)));
        let mut text = String::new();
        let _ = diagnostics.take(4096).read_to_string(&mut text);
        let home = std::env::var("HOME").unwrap_or_default();
        let mut safe = text
            .lines()
            .filter(|l| {
                !["token", "password", "secret", "authorization"]
                    .iter()
                    .any(|s| l.to_ascii_lowercase().contains(s))
            })
            .collect::<Vec<_>>()
            .join(" ");
        safe = safe
            .split_whitespace()
            .map(|word| {
                if word.contains("://") {
                    "<download-url>"
                } else {
                    word
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        safe = safe.replace(&root().to_string_lossy().to_string(), "<audio-runtime>");
        if !home.is_empty() {
            safe = safe.replace(&home, "<home>");
        }
        safe = safe
            .chars()
            .filter(|c| !c.is_control())
            .take(2048)
            .collect();
        crate::logs::push(
            crate::logs::Source::Deas,
            &format!(
                "ERROR: audio-enhance operation=cli stage={} reason={safe}",
                native_audio_enhance_install_status().stage
            ),
        );
        return Err(format!("{error}: {safe}"));
    }
    Ok(())
}
async fn install(mode: &str, preferred_uv: Option<PathBuf>) -> Result<(), String> {
    deep_asset()?;
    let base = root();
    if !base.is_absolute() {
        return Err(
            "AUDIO_INSTALL: Runtime root must be absolute; configure a host data directory".into(),
        );
    }
    fs::create_dir_all(&base).map_err(|_| "AUDIO_INSTALL: Cannot create private runtime folder")?;
    if !safe_directory(&base) {
        return Err("AUDIO_INSTALL: Runtime folder must not contain symlinks".into());
    }
    let target = runtime_dir(mode);
    if available(mode) {
        // Additive repair only: an already installed stems runtime gains the optional GPU
        // binding without touching its verified CPU artifacts.
        if mode == "stems" && !gpu_ready() {
            let dir = target.clone();
            tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
                match provision_gpu(&dir, preferred_uv) {
                    Ok(bound) => {
                        crate::logs::push(
                            crate::logs::Source::Deas,
                            &format!(
                                "INFO: audio-enhance operation=install mode=stems stage=gpu outcome={}",
                                if bound { "bound" } else { "cpu-only" }
                            ),
                        );
                        Ok(())
                    }
                    Err(error) => {
                        crate::logs::push(
                            crate::logs::Source::Deas,
                            &format!("WARN: audio-enhance operation=install mode=stems stage=gpu outcome=cpu-only reason={error}"),
                        );
                        Ok(())
                    }
                }
            })
            .await
            .map_err(|_| "AUDIO_INSTALL: GPU worker failed")??;
        }
        return Ok(());
    }
    if target.exists() {
        return Err(
            "AUDIO_INSTALL: Incomplete runtime exists; remove its mode folder before reinstalling"
                .into(),
        );
    }
    let stage = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(&base)
        .map_err(|_| "AUDIO_INSTALL: Cannot stage runtime")?;
    if mode == "deep" {
        let (asset, sha) = deep_asset()?;
        let path = stage.path().join("deep-filter.exe");
        download(
            mode,
            &format!("https://github.com/Rikorose/DeepFilterNet/releases/download/v0.5.6/{asset}"),
            &path,
            sha,
            40_000_000,
        )
        .await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                .map_err(|_| "AUDIO_INSTALL: Cannot mark executable")?;
        }
        let mut cmd = Command::new(path);
        cmd.arg("--version");
        run(cmd, &CANCEL)?;
    } else {
        let repo = snapshot(stage.path());
        fs::create_dir_all(&repo).map_err(|_| "AUDIO_INSTALL: Cannot create model repo")?;
        download(
            mode,
            &format!(
                "https://huggingface.co/smulelabs/windowed-roformer/resolve/{REVISION}/{MODEL}"
            ),
            &repo.join(MODEL),
            MODEL_SHA,
            920_000_000,
        )
        .await?;
        for (name, sha) in SOURCES {
            download(mode,
                &format!("https://raw.githubusercontent.com/smulelabs/windowed-roformer/{SOURCE_REVISION}/{name}"),
                &repo.join(name), sha, 131072).await?;
        }
        fs::write(stage.path().join("requirements.txt"), REQUIREMENTS)
            .map_err(|_| "AUDIO_INSTALL: Cannot write package lock")?;
        status(mode, "python-environment", 0, 0);
        // Specialized upstream CLI only: this venv never touches ComfyUI or a user Python.
        let dir = stage.path().to_owned();
        tauri::async_runtime::spawn_blocking(move || {
            let uv = preferred_uv
                .filter(|path| path.is_absolute() && path.is_file())
                .or_else(|| {
                    std::env::var_os("PATH").and_then(|paths| {
                        std::env::split_paths(&paths)
                            .map(|p| p.join(if cfg!(windows) { "uv.exe" } else { "uv" }))
                            .find(|p| p.is_file())
                    })
                })
                .ok_or("AUDIO_INSTALL: Install uv on PATH before provisioning audio runtimes")?;
            let uv = dunce::canonicalize(uv)
                .map_err(|_| "AUDIO_INSTALL: Cannot resolve uv executable")?;
            let mut cmd = Command::new(&uv);
            cmd.arg("--version");
            run(cmd, &CANCEL)?;
            let mut cmd = Command::new(&uv);
            cmd.args([
                "--no-config",
                "venv",
                "--python",
                "3.12",
                "--managed-python",
                "--relocatable",
            ])
            .arg(dir.join("venv"));
            run(cmd, &CANCEL)?;
            let mut cmd = Command::new(&uv);
            cmd.args([
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
                "--build-constraint",
            ])
            .arg(dir.join("requirements.txt"))
            .arg("-r")
            .arg(dir.join("requirements.txt"));
            run(cmd, &CANCEL)?;
            let mut cmd = Command::new(python(&dir));
            cmd.args(["-s", "-E"])
                .arg(snapshot(&dir).join("main.py"))
                .arg("--help");
            run(cmd, &CANCEL)?;
            status("stems", "inference-check", 0, 0);
            let input = dir.join("smoke-input.wav");
            let output = dir.join("smoke-output.wav");
            let mut cmd = Command::new("ffmpeg");
            cmd.args([
                "-v",
                "error",
                "-nostdin",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=44100:cl=stereo",
                "-t",
                "8",
                "-c:a",
                "pcm_f32le",
            ])
            .arg(&input);
            run(cmd, &CANCEL)?;
            // Compile the real upstream CPU FlexAttention path before advertising readiness.
            run(smule_command(&dir, &input, &output, "cpu", None), &CANCEL)?;
            fs::remove_file(input).map_err(|_| "AUDIO_INSTALL: Cannot clean smoke input")?;
            fs::remove_file(output).map_err(|_| "AUDIO_INSTALL: Cannot clean smoke output")?;
            // Optional GPU binding; failures degrade to CPU-only and never fail the install.
            status("stems", "gpu-optional", 0, 0);
            match provision_gpu(&dir, Some(uv.clone())) {
                Ok(bound) => crate::logs::push(
                    crate::logs::Source::Deas,
                    &format!(
                        "INFO: audio-enhance operation=install mode=stems stage=gpu outcome={}",
                        if bound { "bound" } else { "cpu-only" }
                    ),
                ),
                Err(error) => crate::logs::push(
                    crate::logs::Source::Deas,
                    &format!("WARN: audio-enhance operation=install mode=stems stage=gpu outcome=cpu-only reason={error}"),
                ),
            }
            Ok::<(), String>(())
        })
        .await
        .map_err(|_| "AUDIO_INSTALL: Environment worker failed")??;
    }
    check_cancel()?;
    fs::write(stage.path().join("installed.txt"), receipt(mode))
        .map_err(|_| "AUDIO_INSTALL: Cannot write receipt")?;
    fs::rename(stage.path(), &target).map_err(|_| "AUDIO_INSTALL: Cannot publish runtime")?;
    Ok(())
}
/// Optional CUDA binding for the pinned upstream Smule CLI. Read-only reuse of the
/// approved owned environment's GPU Torch plus the isolated extras already installed
/// for CPU; nothing is ever installed into the owned environment itself. GPU-only
/// codec bridges go to an isolated gpu-extras directory. Failures degrade to CPU-only.
/// Linux x86_64 has a checked codec profile; other platforms report cpu-only.
fn provision_gpu(dir: &Path, preferred_uv: Option<PathBuf>) -> Result<bool, String> {
    let cleanup = |dir: &Path| {
        let _ = fs::remove_dir_all(dir.join("gpu-extras"));
        let _ = fs::remove_file(dir.join("gpu_bootstrap.py"));
        let _ = fs::remove_file(dir.join("gpu-bound.json"));
    };
    match provision_gpu_inner(dir, preferred_uv) {
        Ok(bound) => Ok(bound),
        Err(error) => {
            cleanup(dir);
            Err(error)
        }
    }
}
#[cfg(not(windows))]
fn provision_gpu_inner(dir: &Path, preferred_uv: Option<PathBuf>) -> Result<bool, String> {
    // Operator-trusted acceptance override, same level as DEAS_AUDIO_RUNTIME_DIR.
    let Some(managed) = crate::resources::owned_venv_python().or_else(|| {
        std::env::var_os("DEAS_AUDIO_GPU_PYTHON")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute() && regular(p))
    }) else {
        return Ok(false); // No approved owned environment to borrow CUDA Torch from.
    };
    check_cancel()?;
    // The CPU venv ABI must match the managed environment's CPython exactly.
    let abi_file = dir.join("gpu-abi.txt");
    let mut query = Command::new(python(dir));
    query
        .args(["-s", "-E", "-c", "import os,sysconfig;open(os.environ['DEAS_ABI_OUT'],'w').write(f\"{sysconfig.get_config_var('SOABI')}\\n{sysconfig.get_paths()['purelib']}\")"])
        .env("DEAS_ABI_OUT", &abi_file);
    let queried = (|| {
        run(query, &CANCEL)?;
        let text = fs::read_to_string(&abi_file).map_err(|_| "AUDIO_GPU: ABI query failed")?;
        let mut lines = text.lines();
        let abi = lines.next().unwrap_or_default().to_owned();
        let site = PathBuf::from(lines.next().unwrap_or_default());
        fs::remove_file(&abi_file).ok();
        if abi.is_empty() || site == PathBuf::new() {
            return Err("AUDIO_GPU: ABI query incomplete".to_string());
        }
        Ok((abi, site))
    })();
    let (abi, site) = queried?;
    check_cancel()?;
    fs::write(dir.join("gpu_bootstrap.py"), GPU_BOOTSTRAP)
        .map_err(|_| "AUDIO_GPU: Cannot stage bootstrap")?;
    let mut probe = Command::new(&managed);
    probe
        .args(["-s", "-E"])
        .arg(dir.join("gpu_bootstrap.py"))
        .arg(&site)
        .arg(&abi)
        .arg(snapshot(dir))
        .arg("probe")
        .env("TORCHINDUCTOR_CACHE_DIR", dir.join("inductor-gpu"))
        .env("NUMBA_CACHE_DIR", dir.join("numba-gpu"))
        .env("HF_HUB_OFFLINE", "1");
    run(probe, &CANCEL)?;
    check_cancel()?;
    // Isolated checked codec bridge for managed torchaudio; --no-deps never drags Torch in.
    let uv = preferred_uv
        .filter(|path| path.is_absolute() && path.is_file())
        .or_else(|| {
            std::env::var_os("PATH").and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|p| p.join(if cfg!(windows) { "uv.exe" } else { "uv" }))
                    .find(|p| p.is_file())
            })
        })
        .ok_or("AUDIO_GPU: uv required for the isolated codec bridge")?;
    let uv = dunce::canonicalize(uv).map_err(|_| "AUDIO_GPU: Cannot resolve uv")?;
    fs::write(dir.join("gpu-requirements.txt"), GPU_REQUIREMENTS)
        .map_err(|_| "AUDIO_GPU: Cannot write codec lock")?;
    let mut pip = Command::new(&uv);
    pip.args([
        "--no-config",
        "pip",
        "install",
        "--require-hashes",
        "--no-deps",
        "--target",
    ])
    .arg(dir.join("gpu-extras"))
    .arg("-r")
    .arg(dir.join("gpu-requirements.txt"));
    run(pip, &CANCEL)?;
    check_cancel()?;
    // Publish the binding only after one real CUDA inference window succeeds.
    let input = dir.join("gpu-smoke-input.wav");
    let output = dir.join("gpu-smoke-output.wav");
    let mut synth = Command::new("ffmpeg");
    synth
        .args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=44100:cl=stereo",
            "-t",
            "1",
            "-c:a",
            "pcm_f32le",
        ])
        .arg(&input);
    run(synth, &CANCEL)?;
    let binding = GpuBinding {
        python: managed,
        abi: abi.clone(),
        site,
    };
    // The smoke respects the same VRAM admission as production inference.
    crate::resources::admit(
        crate::resources::Need {
            ram_bytes: 0,
            vram_bytes: VRAM_RESERVE,
            device_index: Some(0),
        },
        &CANCEL,
    )?;
    let smoke = run(
        smule_command(dir, &input, &output, "cuda", Some(&binding)),
        &CANCEL,
    );
    let _ = fs::remove_file(&input);
    let _ = fs::remove_file(&output);
    smoke?;
    let rel = binding
        .site
        .strip_prefix(dir)
        .map_err(|_| "AUDIO_GPU: codec extras site escaped the runtime folder")?
        .to_string_lossy()
        .replace('\\', "/");
    fs::write(
        dir.join("gpu-bound.json"),
        serde_json::to_string(&serde_json::json!({"abi": abi, "siteRel": rel}))
            .map_err(|_| "AUDIO_GPU: Cannot write binding")?,
    )
    .map_err(|_| "AUDIO_GPU: Cannot publish binding")?;
    Ok(true)
}
#[cfg(windows)]
fn provision_gpu_inner(_dir: &Path, _preferred_uv: Option<PathBuf>) -> Result<bool, String> {
    // No checked GPU codec/compiler profile for the owned Windows environment yet.
    Ok(false)
}
#[tauri::command]
pub fn native_audio_enhance_install_status() -> Status {
    STATUS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}
#[tauri::command]
pub fn native_audio_enhance_install_cancel() {
    CANCEL.store(true, Ordering::Relaxed);
}
#[tauri::command]
pub async fn native_audio_enhance_install(
    engine: tauri::State<'_, crate::install::Engine>,
    mode: String,
    confirmed: bool,
    weights_acknowledged: Option<bool>,
) -> Result<crate::audio_enhance::Capabilities, String> {
    let managed_uv = engine.approved_plan().map(|plan| {
        PathBuf::from(plan.request.storage_path).join(if cfg!(windows) {
            "runtime/uv.exe"
        } else {
            "runtime/uv"
        })
    });
    install_requested(mode, confirmed, weights_acknowledged, managed_uv).await
}

pub(crate) async fn install_requested(
    mode: String,
    confirmed: bool,
    weights_acknowledged: Option<bool>,
    preferred_uv: Option<PathBuf>,
) -> Result<crate::audio_enhance::Capabilities, String> {
    let _ = weights_acknowledged; // Compatibility only; the selected code and checkpoint are MIT.
    if !confirmed {
        return Err("AUDIO_CONSENT: Confirm runtime, package and model installation (DeepFilterNet MIT/Apache-2.0; Smule Windowed RoFormer code and checkpoint MIT)".into());
    }
    if !matches!(mode.as_str(), "deep" | "stems") {
        return Err("AUDIO_MODE: Install deep or stems".into());
    }
    let _permit = INSTALLING
        .try_acquire()
        .map_err(|_| "AUDIO_BUSY: An audio installation is active")?;
    CANCEL.store(false, Ordering::Relaxed);
    status(&mode, "starting", 0, 0);
    let result = async {
        if mode == "stems" && !available("deep") {
            install("deep", preferred_uv.clone()).await?;
        }
        install(&mode, preferred_uv).await
    }
    .await;
    let stage = if result.is_ok() {
        "completed"
    } else if CANCEL.load(Ordering::Relaxed) {
        "cancelled"
    } else {
        "failed"
    };
    status(&mode, stage, 0, 0);
    crate::logs::push(
        crate::logs::Source::Deas,
        &format!(
            "{}: audio-enhance operation=install mode={mode} outcome={stage}",
            if result.is_err() && stage != "cancelled" {
                "ERROR"
            } else {
                "INFO"
            }
        ),
    );
    result?;
    tauri::async_runtime::spawn_blocking(crate::audio_enhance::capabilities)
        .await
        .map_err(|_| "AUDIO_PROBE: Capability worker failed".into())
}

pub(crate) fn check_memory(
    duration: f64,
    mode: &str,
    channels: u32,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if !duration.is_finite() || duration <= 0.0 {
        return Err("AUDIO_MEMORY: Source duration is invalid".into());
    }
    // Conservative admission: Smule holds one window; DeepFilterNet retains full waveforms.
    let estimate = if mode == "stems" {
        4_294_967_296 + (WINDOW * 2 * 4 * 8) as u64
    } else {
        (134_217_728.0 + duration * 48000.0 * f64::from(channels) * 16.0) as u64
    };
    crate::resources::admit(
        crate::resources::Need {
            ram_bytes: estimate,
            vram_bytes: 0,
            device_index: None,
        },
        cancel,
    )
    .map_err(|error| {
        format!(
            "AUDIO_MEMORY: {:.0} MiB estimated; {error}. Free memory or use a shorter clip",
            estimate as f64 / 1048576.0
        )
    })
}

pub(crate) fn enhance(
    input: &Path,
    dir: &Path,
    mode: &str,
    profile: &str,
    requested: Option<&str>,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let runtime = runtime_dir(mode);
    if !available(mode) {
        return Err("AUDIO_UNAVAILABLE: Explicit installation required".into());
    }
    if mode == "deep" {
        verify(&runtime.join("deep-filter.exe"), deep_asset()?.1)?;
        let out = dir.join("deep");
        fs::create_dir(&out).map_err(|_| "AUDIO_RUNTIME: Cannot create work directory")?;
        let mut probe = Command::new("ffprobe");
        probe
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "json",
            ])
            .arg(input);
        let bytes = crate::video_post::run_tool(probe, Duration::from_secs(10), cancel, None)?;
        let metadata: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| "AUDIO_RUNTIME: Invalid PCM duration")?;
        let duration = metadata["format"]["duration"]
            .as_str()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
            .ok_or("AUDIO_RUNTIME: Missing PCM duration")?;
        let padded = dir.join("padded.wav");
        let mut pad = Command::new("ffmpeg");
        pad.args(["-v", "error", "-nostdin", "-i"])
            .arg(input)
            .args(["-af", "apad=pad_dur=1", "-ar", "48000", "-c:a", "pcm_s16le"])
            .arg(&padded);
        run(pad, cancel)?;
        let mut cmd = Command::new(runtime.join("deep-filter.exe"));
        cmd.args(["-D", "-o"]).arg(&out).arg(&padded);
        run(cmd, cancel)?;
        let target = dir.join("deep-enhanced.wav");
        let mut trim = Command::new("ffmpeg");
        trim.args(["-v", "error", "-nostdin", "-i"])
            .arg(out.join("padded.wav"))
            .args([
                "-af",
                &format!("atrim=end_sample={}", (duration * 48000.0).round() as u64),
                "-c:a",
                "pcm_f32le",
            ])
            .arg(&target);
        run(trim, cancel)?;
        return Ok(target);
    }
    verify(&snapshot(&runtime).join(MODEL), MODEL_SHA)?;
    for (name, sha) in SOURCES {
        verify(&snapshot(&runtime).join(name), sha)?;
    }
    let (mut device, mut binding) = resolve_device(requested)?;
    if device == "cuda" {
        // VRAM admission requests an owned-idle release and remeasures before launch.
        if let Err(error) = crate::resources::admit(
            crate::resources::Need {
                ram_bytes: 0,
                vram_bytes: VRAM_RESERVE,
                device_index: Some(0),
            },
            cancel,
        ) {
            if matches!(requested, None | Some("auto")) {
                crate::logs::push(crate::logs::Source::Deas,
                    &format!("WARN: audio-enhance operation=separate engine=smule requested=cuda outcome=cpu-fallback reason={error}"));
                device = "cpu";
                binding = None;
            } else {
                return Err(format!("AUDIO_DEVICE: {error}"));
            }
        }
    }
    crate::logs::push(
        crate::logs::Source::Deas,
        &format!(
            "INFO: audio-enhance operation=separate engine=smule requested={} device={device}",
            requested.unwrap_or("auto")
        ),
    );
    let (mut vocal, samples) = separate(input, dir, &runtime, device, binding.as_ref(), cancel)?;
    let instrumental = dir.join("instrumental.wav");
    let mut subtract = Command::new("ffmpeg");
    subtract.args(["-v", "error", "-nostdin", "-i"]).arg(input)
        .arg("-i").arg(&vocal)
        .args(["-filter_complex", "[0:a]aformat=sample_fmts=flt:channel_layouts=stereo[m];[1:a]volume=-1[v];[m][v]amix=inputs=2:normalize=0:duration=first[out]", "-map", "[out]", "-c:a", "pcm_f32le"])
        .arg(&instrumental);
    run(subtract, cancel)?;
    if profile == "speech" {
        let voice = dir.join("voice.wav");
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-v", "error", "-nostdin", "-i"])
            .arg(&vocal)
            .args(["-ar", "48000", "-c:a", "pcm_s16le"])
            .arg(&voice);
        run(cmd, cancel)?;
        vocal = enhance(&voice, dir, "deep", "speech", None, cancel)?;
    }
    let target = dir.join("remix.wav");
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-v", "error", "-nostdin", "-i"]).arg(instrumental)
        .arg("-i").arg(vocal)
        // No speech isolation on singing or independent normalization: retain relative stem gains.
        .args(["-filter_complex", &format!("[0:a]highpass=f=20[i];[1:a]aresample=44100,highpass=f=40[v];[i][v]amix=inputs=2:normalize=0:duration=first,apad,atrim=end_sample={samples}[out]"), "-map", "[out]", "-ar", "44100", "-c:a", "pcm_f32le"])
        .arg(&target);
    run(cmd, cancel)?;
    Ok(target)
}

const RATE: usize = 44100;
const WINDOW: usize = RATE * 8;
const HOP: usize = RATE * 4;
// Conservative GPU reserve for one 8-second window plus weights and CUDA context;
// the measured peak from acceptance runs stays below this (recorded in VERIFICATION).
const VRAM_RESERVE: u64 = 10_737_418_240;

fn smule_command(
    runtime: &Path,
    input: &Path,
    output: &Path,
    device: &str,
    binding: Option<&GpuBinding>,
) -> Command {
    let model = snapshot(runtime).join(MODEL);
    if device == "cuda" {
        let binding = binding.expect("cuda requires a binding");
        let mut cmd = Command::new(&binding.python);
        cmd.args(["-s", "-E"])
            .arg(runtime.join("gpu_bootstrap.py"))
            .arg(&binding.site)
            .arg(&binding.abi)
            .arg(snapshot(runtime))
            .arg("run")
            .arg("--checkpoint")
            .arg(&model)
            .arg("--input")
            .arg(input)
            .arg("--output")
            .arg(output)
            .args([
                "--device",
                "cuda",
                "--sample_rate",
                "44100",
                "--chunk_size",
                "8",
                "--batch_size",
                "1",
            ])
            .env("TORCHINDUCTOR_CACHE_DIR", runtime.join("inductor-gpu"))
            .env("NUMBA_CACHE_DIR", runtime.join("numba-gpu"))
            .env("HF_HUB_OFFLINE", "1");
        return cmd;
    }
    let mut cmd = Command::new(python(runtime));
    cmd.args(["-s", "-E"])
        .arg(snapshot(runtime).join("main.py"))
        .arg("--checkpoint")
        .arg(&model)
        .arg("--input")
        .arg(input)
        .arg("--output")
        .arg(output)
        .args([
            "--device",
            "cpu",
            "--sample_rate",
            "44100",
            "--chunk_size",
            "8",
            "--batch_size",
            "1",
        ])
        .env("CUDA_VISIBLE_DEVICES", "")
        .env("TORCHINDUCTOR_CACHE_DIR", runtime.join("inductor"))
        .env("NUMBA_CACHE_DIR", runtime.join("numba"))
        .env("HF_HUB_OFFLINE", "1");
    cmd
}

fn overlap(previous: &[f32], current: &mut [f32]) {
    for (index, (old, new)) in previous.iter().zip(current.iter_mut()).enumerate() {
        let fade = (index / 2) as f32 / (HOP - 1) as f32;
        *new = old * (1.0 - fade) + *new * fade;
    }
}

fn separate(
    input: &Path,
    dir: &Path,
    runtime: &Path,
    device: &str,
    binding: Option<&GpuBinding>,
    cancel: &AtomicBool,
) -> Result<(PathBuf, u64), String> {
    let mut probe = Command::new("ffprobe");
    probe
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=duration_ts,time_base,sample_rate",
            "-of",
            "json",
        ])
        .arg(input);
    let bytes = crate::video_post::run_tool(probe, Duration::from_secs(10), cancel, None)?;
    let metadata: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "AUDIO_RUNTIME: Invalid PCM metadata")?;
    let stream = &metadata["streams"][0];
    let samples = stream["duration_ts"]
        .as_u64()
        .filter(|n| *n > 0)
        .ok_or("AUDIO_RUNTIME: Missing exact PCM sample count")?;
    if stream["sample_rate"] != "44100" || stream["time_base"] != "1/44100" {
        return Err("AUDIO_RUNTIME: Separator requires exact 44100 Hz PCM".into());
    }
    let raw = dir.join("vocals.f32");
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&raw)
        .map_err(|_| "AUDIO_RUNTIME: Cannot create separated PCM")?;
    let mut previous = Vec::new();
    let mut start = 0_u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("AUDIO_CANCELLED: Separation cancelled".into());
        }
        let work = tempfile::Builder::new()
            .prefix("window-")
            .tempdir_in(dir)
            .map_err(|_| "AUDIO_RUNTIME: Cannot create window workspace")?;
        let chunk = work.path().join("input.wav");
        let vocal = work.path().join("vocals.wav");
        let pcm = work.path().join("vocals.f32");
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-v", "error", "-nostdin", "-i"]).arg(input)
            .args(["-af", &format!("atrim=start_sample={start}:end_sample={},apad=whole_len={WINDOW},atrim=end_sample={WINDOW}", start + WINDOW as u64), "-ac", "2", "-c:a", "pcm_f32le"]).arg(&chunk);
        run(cmd, cancel)?;
        let begin = std::time::Instant::now();
        // Upstream --overlap is ignored. Reloading the unmodified CLI per window costs startup/model time.
        run(
            smule_command(runtime, &chunk, &vocal, device, binding),
            cancel,
        )?;
        crate::logs::push(crate::logs::Source::Deas,
            &format!("INFO: audio-enhance operation=separate engine=smule device={device} startSample={start} totalSamples={samples} windowSamples={WINDOW} overlapSamples={HOP} elapsedMs={}", begin.elapsed().as_millis()));
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-v", "error", "-nostdin", "-i"])
            .arg(&vocal)
            .args(["-f", "f32le", "-c:a", "pcm_f32le"])
            .arg(&pcm);
        run(cmd, cancel)?;
        if fs::metadata(&pcm)
            .map_err(|_| "AUDIO_VERIFY: Missing separated PCM")?
            .len()
            != (WINDOW * 2 * 4) as u64
        {
            return Err("AUDIO_VERIFY: Separator changed window sample count".into());
        }
        let bytes = fs::read(&pcm).map_err(|_| "AUDIO_RUNTIME: Cannot read window PCM")?;
        let mut current: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        if current.iter().any(|n| !n.is_finite()) {
            return Err("AUDIO_VERIFY: Non-finite separated PCM".into());
        }
        overlap(&previous, &mut current);
        let last = samples - start <= WINDOW as u64;
        let count = if last {
            (samples - start) as usize
        } else {
            HOP
        } * 2;
        let bytes: Vec<u8> = current[..count]
            .iter()
            .flat_map(|n| n.to_le_bytes())
            .collect();
        output
            .write_all(&bytes)
            .map_err(|_| "AUDIO_RUNTIME: Cannot write separated PCM")?;
        if last {
            break;
        }
        previous = current[HOP * 2..].to_vec();
        start += HOP as u64;
    }
    drop(output);
    let vocal = dir.join("vocals.wav");
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-v", "error", "-nostdin", "-f", "f32le", "-ar", "44100", "-ac", "2", "-i",
    ])
    .arg(raw)
    .args(["-c:a", "pcm_f32le"])
    .arg(&vocal);
    run(cmd, cancel)?;
    Ok((vocal, samples))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smule_overlap_is_complementary_and_preserves_stereo() {
        let old = vec![1.0; HOP * 2];
        let mut new = vec![3.0; WINDOW * 2];
        overlap(&old, &mut new);
        assert_eq!(&new[..2], &[1.0, 1.0]);
        assert_eq!(&new[HOP * 2 - 2..HOP * 2], &[3.0, 3.0]);
        assert!((new[HOP] - 2.0).abs() < 0.00002);
        let mut same = old.clone();
        overlap(&old, &mut same);
        assert!(same.iter().all(|v| (*v - 1.0).abs() < f32::EPSILON));
    }

    #[test]
    fn smule_pins_and_cli_are_explicit_cpu_not_ignored_overlap() {
        assert!(!REQUIREMENTS.contains("demucs"));
        assert!(REQUIREMENTS.contains("torch-2.8.0"));
        assert_eq!(MODEL_SHA.len(), 64);
        assert!(SOURCES.iter().all(|(_, sha)| sha.len() == 64));
        let cmd = smule_command(
            Path::new("runtime"),
            Path::new("input.wav"),
            Path::new("output.wav"),
            "cpu",
            None,
        );
        let args: Vec<_> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert!(args.windows(2).any(|a| a == ["--device", "cpu"]));
        assert!(args.windows(2).any(|a| a == ["--batch_size", "1"]));
        assert!(!args.iter().any(|a| a == "--overlap"));
        // CUDA launches the read-only bootstrap around the pinned upstream CLI, never main.py directly.
        let cuda = GpuBinding {
            python: PathBuf::from("managed/python"),
            abi: "cpython-312-x86_64-linux-gnu".into(),
            site: PathBuf::from("runtime/venv/lib/python3.12/site-packages"),
        };
        let cmd = smule_command(
            Path::new("runtime"),
            Path::new("input.wav"),
            Path::new("output.wav"),
            "cuda",
            Some(&cuda),
        );
        assert_eq!(cmd.get_program().to_string_lossy(), "managed/python");
        let args: Vec<_> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert!(args.iter().any(|a| a.ends_with("gpu_bootstrap.py")));
        assert!(args.iter().any(|a| a == "run"));
        assert!(args.windows(2).any(|a| a == ["--device", "cuda"]));
        assert!(!args.iter().any(|a| a == "--overlap"));
    }

    #[test]
    #[ignore = "requires explicitly installed Smule runtime; real short/over-window CPU inference"]
    fn smule_real_short_and_overlapping_cpu() {
        assert!(std::env::var_os("DEAS_AUDIO_RUNTIME_DIR").is_some());
        assert!(available("stems"));
        let cancel = AtomicBool::new(false);
        for samples in [44101_u64, 396901] {
            let work = tempfile::Builder::new()
                .prefix("smule-acceptance-")
                .tempdir_in(root())
                .unwrap();
            let input = work.path().join("input.wav");
            let mut cmd = Command::new("ffmpeg");
            cmd.args([
                "-v",
                "error",
                "-nostdin",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=523:sample_rate=44100",
                "-af",
                &format!("atrim=end_sample={samples}"),
                "-ac",
                "2",
                "-c:a",
                "pcm_f32le",
            ])
            .arg(&input);
            run(cmd, &cancel).unwrap();
            let started = std::time::Instant::now();
            let remix = enhance(&input, work.path(), "stems", "mix", Some("cpu"), &cancel).unwrap();
            let pcm = |path: &Path| {
                let out = Command::new("ffmpeg")
                    .args(["-v", "error", "-i"])
                    .arg(path)
                    .args(["-f", "f32le", "pipe:1"])
                    .output()
                    .unwrap();
                assert!(out.status.success());
                out.stdout
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                    .collect::<Vec<_>>()
            };
            let source = pcm(&input);
            let vocal = pcm(&work.path().join("vocals.wav"));
            let instrumental = pcm(&work.path().join("instrumental.wav"));
            let processed = pcm(&remix);
            for stem in [&source, &vocal, &instrumental, &processed] {
                assert_eq!(stem.len() as u64, samples * 2);
                assert!(stem.iter().all(|n| n.is_finite()));
            }
            let error = source
                .iter()
                .zip(&vocal)
                .zip(&instrumental)
                .map(|((mix, v), i)| (mix - v - i).abs())
                .fold(0.0_f32, f32::max);
            assert!(error < 1e-6, "complement error {error}");
            assert_ne!(source, processed);
            assert!(vocal.iter().any(|v| v.abs() > 0.0));
            eprintln!("Smule CPU samples={samples} elapsed={:?} complementMaxError={error}; exact vocals/instrumental/remix length", started.elapsed());
        }
    }

    #[test]
    #[ignore = "requires provisioned GPU binding and >=10 GiB free VRAM; run when the owned GPU is idle"]
    fn smule_real_short_cuda() {
        assert!(std::env::var_os("DEAS_AUDIO_RUNTIME_DIR").is_some());
        assert!(available("stems"));
        assert!(gpu_ready(), "run gpu_provision_repair_real first");
        let cancel = AtomicBool::new(false);
        // Admission fails closed while the GPU lacks its reserve; that is an honest failure.
        let work = tempfile::Builder::new()
            .prefix("smule-cuda-acceptance-")
            .tempdir_in(root())
            .unwrap();
        let input = work.path().join("input.wav");
        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=523:sample_rate=44100",
            "-af",
            "atrim=end_sample=396901",
            "-ac",
            "2",
            "-c:a",
            "pcm_f32le",
        ])
        .arg(&input);
        run(cmd, &cancel).unwrap();
        let started = std::time::Instant::now();
        let remix = enhance(&input, work.path(), "stems", "mix", Some("cuda"), &cancel).unwrap();
        let pcm = |path: &Path| {
            let out = Command::new("ffmpeg")
                .args(["-v", "error", "-i"])
                .arg(path)
                .args(["-f", "f32le", "pipe:1"])
                .output()
                .unwrap();
            assert!(out.status.success());
            out.stdout
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect::<Vec<_>>()
        };
        let source = pcm(&input);
        let vocal = pcm(&work.path().join("vocals.wav"));
        let instrumental = pcm(&work.path().join("instrumental.wav"));
        for stem in [&source, &vocal, &instrumental, &pcm(&remix)] {
            assert_eq!(stem.len() as u64, 396901 * 2);
            assert!(stem.iter().all(|n| n.is_finite()));
        }
        let error = source
            .iter()
            .zip(&vocal)
            .zip(&instrumental)
            .map(|((mix, v), i)| (mix - v - i).abs())
            .fold(0.0_f32, f32::max);
        assert!(error < 1e-4, "cuda complement error {error}");
        eprintln!(
            "Smule CUDA multi-window elapsed={:?} complementMaxError={error}",
            started.elapsed()
        );
    }

    #[test]
    #[ignore = "provisions/repairs the optional GPU binding against the owned environment; needs CUDA Torch and VRAM for its inference smoke"]
    fn gpu_provision_repair_real() {
        assert!(std::env::var_os("DEAS_AUDIO_RUNTIME_DIR").is_some());
        assert!(available("stems"));
        let bound = provision_gpu(&runtime_dir("stems"), None).unwrap();
        eprintln!("gpu provision outcome={bound}");
    }
}
