//! Explicit lifecycle for the current app-owned, successfully provisioned environment.
//! Linux and Windows, matching native provisioning. No target/PID/URL IPC inputs.
use crate::install::{Backend, Plan, VENV_BIN, VENV_PYTHON};
use serde::Serialize;
fn install_output_naming(root: &Path) -> Result<(), String> {
    install_adapter(
        root,
        "deas_output_naming.py",
        include_str!("output_naming.py").as_bytes(),
    )
}
fn install_adapter(root: &Path, name: &str, source: &[u8]) -> Result<(), String> {
    use crate::downloads::fs::Anchor;
    use std::io::Write;
    let directory = root.join("ComfyUI/custom_nodes");
    if !directory.exists() {
        fs::create_dir(&directory).map_err(|e| e.to_string())?;
    }
    let anchor = Anchor::open(&directory)?;
    anchor.validate_directory(false)?;
    if directory.join(name).symlink_metadata().is_ok() {
        if read_owned(&directory.join(name), 1024 * 1024)? == source {
            return Ok(());
        }
        anchor.remove_partial(name);
    }
    let temporary = format!(".deas-naming-{}.partial", uuid::Uuid::new_v4());
    let result = (|| {
        let mut file = anchor.partial(&temporary)?;
        file.write_all(source).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        anchor.validate_directory(false)?;
        anchor.publish(&temporary, name)
    })();
    anchor.remove_partial(&temporary);
    result
}

use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: String,
    pub endpoint: Option<String>,
    pub error: Option<String>,
    pub generation_ready: bool,
    #[serde(skip)]
    pub(crate) output_root: Option<PathBuf>,
    #[serde(skip)]
    pub(crate) model_roots: Vec<PathBuf>,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            state: "stopped".into(),
            endpoint: None,
            error: None,
            generation_ready: false,
            output_root: None,
            model_roots: vec![],
        }
    }
}

/// Captured after native provisioning or explicit validated existing-environment approval.
#[derive(Clone)]
pub(crate) struct Approved {
    plan: Plan,
    files: Vec<(PathBuf, Vec<u8>)>,
    fingerprints: Vec<(PathBuf, [u8; 32])>,
    identities: Vec<(PathBuf, (u64, u64, u32))>,
}
fn safe_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Owned path must be absolute".into());
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        if matches!(
            component,
            std::path::Component::ParentDir | std::path::Component::CurDir
        ) {
            return Err("Owned path contains traversal".into());
        }
        current.push(component);
        let meta =
            fs::symlink_metadata(&current).map_err(|e| format!("Owned path missing: {e}"))?;
        if meta.file_type().is_symlink() {
            return Err("Owned paths must not contain symlinks".into());
        }
    }
    Ok(())
}
#[cfg(target_os = "linux")]
fn identity(path: &Path) -> Result<(u64, u64, u32), String> {
    use std::os::unix::fs::MetadataExt;
    safe_path(path)?;
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o022 != 0 {
        return Err(
            "Owned environment must be owned by this user and not group/world writable".into(),
        );
    }
    Ok((meta.dev(), meta.ino(), meta.uid()))
}
// Windows: handle identity only; ownership is ACL-based and the environment root was
// created by this app, so the uid slot is always 0.
#[cfg(windows)]
fn identity(path: &Path) -> Result<(u64, u64, u32), String> {
    safe_path(path)?;
    let (volume, index) = crate::win::identity(&crate::win::open_no_follow(path)?)?;
    Ok((volume, index, 0))
}
#[cfg(not(any(target_os = "linux", windows)))]
fn identity(_: &Path) -> Result<(u64, u64, u32), String> {
    Err("Owned lifecycle supports Linux and Windows only".into())
}
pub(crate) fn read_owned(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    safe_path(path)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > limit as u64 {
        return Err("Owned file is not regular or exceeds size limit".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("Owned file exceeds size limit".into());
    }
    Ok(bytes)
}
impl Approved {
    pub(crate) fn plan(&self) -> &Plan {
        &self.plan
    }
    pub(crate) fn capture(plan: &Plan) -> Result<Self, String> {
        let root = Path::new(&plan.request.storage_path);
        let expected = serde_json::to_value(plan).map_err(|e| e.to_string())?;
        let plan_bytes = read_owned(&root.join("deas-plan.json"), 1024 * 1024)?;
        if serde_json::from_slice::<serde_json::Value>(&plan_bytes).map_err(|e| e.to_string())?
            != expected
        {
            return Err("Owned plan manifest does not match approved plan".into());
        }
        let install_bytes = read_owned(&root.join("deas-install.json"), 1024 * 1024)?;
        let manifest: serde_json::Value =
            serde_json::from_slice(&install_bytes).map_err(|e| e.to_string())?;
        if manifest["planId"] != plan.id
            || manifest["python"]
                != root
                    .join(VENV_PYTHON)
                    .to_str()
                    .ok_or("Invalid Python path")?
            || manifest["backend"] != expected["request"]["backend"]
            || manifest["hardware"] != expected["hardware"]
            || manifest["torchProfile"] != expected["torch"]
            || manifest["generationReady"] != false
        {
            return Err("Owned install manifest does not match approved plan".into());
        }
        let mut identities = vec![];
        for name in [
            "",
            "ComfyUI",
            "ComfyUI/main.py",
            "venv",
            VENV_BIN,
            VENV_PYTHON,
            "venv/pyvenv.cfg",
            "runtime",
            "runtime/python",
            "home",
            "cache",
            "config",
            "tmp",
            "deas-plan.json",
            "deas-install.json",
        ] {
            let path = root.join(name);
            identities.push((path.clone(), identity(&path)?));
        }
        if !root.join(VENV_PYTHON).is_file() || !root.join("ComfyUI/main.py").is_file() {
            return Err("Owned executable/main.py must be regular files".into());
        }
        let mut fingerprints = vec![];
        for name in ["ComfyUI/main.py", VENV_PYTHON, "venv/pyvenv.cfg"] {
            let path = root.join(name);
            fingerprints.push((
                path.clone(),
                Sha256::digest(read_owned(&path, 128 * 1024 * 1024)?).into(),
            ));
        }
        let approved = Self {
            plan: plan.clone(),
            files: vec![
                (root.join("deas-plan.json"), plan_bytes),
                (root.join("deas-install.json"), install_bytes),
            ],
            identities,
            fingerprints,
        };
        approved.validate()?;
        Ok(approved)
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !self.plan.can_execute
            || !matches!(self.plan.request.backend, Backend::Cpu | Backend::Nvidia)
        {
            return Err("Approved backend is not executable".into());
        }
        crate::install::validate_gguf_install(
            &self.plan,
            Path::new(&self.plan.request.storage_path),
        )?;
        for (path, expected) in &self.identities {
            if identity(path)? != *expected {
                return Err("Owned environment identity changed; re-provision required".into());
            }
        }
        for (path, expected) in &self.files {
            let actual = read_owned(path, 1024 * 1024)?;
            if Sha256::digest(&actual) != Sha256::digest(expected) {
                return Err("Owned manifest changed; refusing launch".into());
            }
        }
        for (path, expected) in &self.fingerprints {
            let actual: [u8; 32] = Sha256::digest(read_owned(path, 128 * 1024 * 1024)?).into();
            if actual != *expected {
                return Err("Owned launch file changed; refusing launch".into());
            }
        }
        Ok(())
    }
}

/// Core files whose queue-to-interrupt protocol the adapter (`running_interrupt.py`) relies on.
/// Installs track `master`, so the protocol is checked by its statements, not by whole-file hashes.
const INTERRUPT_FILES: [&str; 4] = ["server.py", "execution.py", "main.py", "nodes.py"];
/// Body of a `def` up to the next definition at the same or lower indentation.
fn def_body<'a>(text: &'a str, def: &str) -> Option<&'a str> {
    let rest = &text[text.find(def)? + def.len()..];
    let end = [
        "\n    def ",
        "\n    async def ",
        "\ndef ",
        "\nasync def ",
        "\nclass ",
    ]
    .iter()
    .filter_map(|d| rest.find(d))
    .min()
    .unwrap_or(rest.len());
    Some(&rest[..end])
}
/// First `with` statement of a def body is the given lock.
fn locked_by(text: &str, def: &str, lock: &str) -> bool {
    def_body(text, def).is_some_and(|b| b.find("with ").is_some_and(|i| b[i..].starts_with(lock)))
}
fn interrupt_protocol(name: &str, text: &str) -> bool {
    match name {
        // get/task_done/get_current_queue share one RLock (the Condition wraps it); every prompt resets the flag first.
        "execution.py" => {
            let Some(queue) = text.find("class PromptQueue:").map(|i| &text[i..]) else {
                return false;
            };
            queue.contains("self.mutex = threading.RLock()")
                && queue.contains("self.not_empty = threading.Condition(self.mutex)")
                && locked_by(
                    queue,
                    "def get(self, timeout=None):",
                    "with self.not_empty:",
                )
                && locked_by(queue, "def task_done(", "with self.mutex:")
                && locked_by(queue, "def get_current_queue(", "with self.mutex:")
                && def_body(text, "async def execute_async(")
                    .is_some_and(|b| b.contains("nodes.interrupt_processing(False)"))
        }
        "main.py" => def_body(text, "def prompt_worker(")
            .is_some_and(|b| b.contains("q.get(") && b.contains("q.task_done(")),
        "nodes.py" => def_body(text, "def interrupt_processing(value=True):").is_some_and(|b| {
            b.contains("comfy.model_management.interrupt_current_processing(value)")
        }),
        "server.py" => text.contains("self.prompt_queue = execution.PromptQueue(self)"),
        _ => false,
    }
}
struct InterruptSource {
    files: Vec<(PathBuf, [u8; 32])>,
    token: String,
}
impl InterruptSource {
    fn capture(root: &Path) -> Result<Self, String> {
        let mut files = Vec::new();
        for name in INTERRUPT_FILES {
            let path = root.join("ComfyUI").join(name);
            let bytes = read_owned(&path, 1024 * 1024)?;
            // Accept Windows checkout CRLF, but retain and revalidate the exact launched raw bytes.
            let normalized = std::str::from_utf8(&bytes)
                .map_err(|_| "Invalid backend source encoding")?
                .replace("\r\n", "\n");
            if !interrupt_protocol(name, &normalized) {
                return Err("Scoped interruption requires inspected backend sources".into());
            }
            files.push((path, Sha256::digest(&bytes).into()));
        }
        Ok(Self {
            files,
            token: uuid::Uuid::new_v4().to_string(),
        })
    }
    fn validate(&self) -> Result<(), String> {
        for (path, digest) in &self.files {
            let bytes = read_owned(path, 1024 * 1024)?;
            if <[u8; 32]>::from(Sha256::digest(bytes)) != *digest {
                return Err("Scoped interruption backend sources changed since launch".into());
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct Inner {
    status: Status,
    child: Option<Child>,
    environment_lock: Option<fs::File>,
    #[cfg(windows)]
    job: Option<crate::win::Job>,
    serial: u64,
    port: u16,
    interrupt_source: Option<InterruptSource>,
}
#[derive(Default)]
pub struct Lifecycle {
    inner: Arc<Mutex<Inner>>,
}
fn locked(inner: &Arc<Mutex<Inner>>) -> std::sync::MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(|e| e.into_inner())
}
pub(crate) const MAX_LOG_LINE: u64 = 64 * 1024;
/// Child output as display text: ANSI escapes and control characters removed,
/// a carriage return overwrites the line like a terminal (progress bars).
pub(crate) fn clean_log_line(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_end_matches(['\r', '\n']);
    let text = text.rsplit('\r').next().unwrap_or_default();
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.next() {
                // CSI: parameters until a final byte in @..=~
                Some('[') => {
                    for n in chars.by_ref() {
                        if ('@'..='~').contains(&n) {
                            break;
                        }
                    }
                }
                // OSC: until BEL or ESC \\
                Some(']') => {
                    while let Some(n) = chars.next() {
                        if n == '\u{7}' || (n == '\u{1b}' && chars.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if c == '\t' || !c.is_control() {
            out.push(c);
        }
    }
    out.trim_end().to_string()
}
fn log(text: &str) {
    crate::logs::push(crate::logs::Source::Deas, text);
}
// Keep the leader unreaped until its private group is killed: no PID/group reuse window.
#[cfg(target_os = "linux")]
fn exited(child: &Child) -> Result<bool, String> {
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            child.id(),
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result != 0 {
        return Err(format!(
            "Owned child wait failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(unsafe { info.si_pid() } != 0)
}
#[cfg(windows)]
fn exited(child: &Child) -> Result<bool, String> {
    use std::os::windows::io::AsRawHandle;
    crate::win::exited(child.as_raw_handle())
}
#[cfg(not(any(target_os = "linux", windows)))]
fn exited(_: &Child) -> Result<bool, String> {
    Err("Unsupported platform".into())
}
fn lock_environment(root: &Path) -> Result<fs::File, String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Write access is what a second locker cannot share; readers (the owned manifest checks) can.
        options
            .write(true)
            .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
            .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(root.join("deas-install.json")).map_err(|_| {
        "Environment is unavailable or already in use by another DEAS instance".to_string()
    })?;
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("Environment is already running in another DEAS instance; no duplicate backend was started".into());
        }
    }
    Ok(file)
}
const STOP_GRACE: Duration = Duration::from_secs(5);
// CREATE_NO_WINDOW has no console for GenerateConsoleCtrlEvent. The private pipe
// asks Python to run its normal SIGINT handler, waking ComfyUI's asyncio loop.
#[cfg(any(windows, test))]
const WINDOWS_BOOTSTRAP: &str = r#"
import sys, runpy, threading, _thread, os, time
sys.argv = sys.argv[1:]
def deas_wait_stop():
    if os.name != 'nt':
        return sys.stdin.buffer.readline(6)
    # A blocking ReadFile on the synchronous stdin pipe stalls the main thread's
    # DLL loads (numpy during `import torch`); poll and read only when complete.
    import ctypes, msvcrt
    from ctypes import wintypes
    peek = ctypes.windll.kernel32.PeekNamedPipe
    peek.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD, ctypes.c_void_p, ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p]
    handle = msvcrt.get_osfhandle(sys.stdin.fileno())
    avail = wintypes.DWORD()
    while peek(handle, None, 0, None, ctypes.byref(avail), None):
        if avail.value >= 5:
            return sys.stdin.buffer.read(5)
        time.sleep(0.2)
    return b''
def deas_stop():
    if deas_wait_stop() == b'stop\n':
        loop = getattr(sys.modules.get('__main__'), 'event_loop', None)
        if loop is not None and loop.is_running():
            loop.call_soon_threadsafe(_thread.interrupt_main)
        else:
            _thread.interrupt_main()
threading.Thread(target=deas_stop, daemon=True).start()
runpy.run_path(sys.argv[0], run_name='__main__')
"#;

#[cfg(target_os = "linux")]
fn group_exited(child: &Child) -> Result<bool, String> {
    // Keep the zombie leader as the group identity anchor while descendants exit.
    for entry in fs::read_dir("/proc").map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_name().to_string_lossy().parse::<u32>().is_err() {
            continue;
        }
        if let Ok(stat) = fs::read_to_string(entry.path().join("stat")) {
            if let Some((_, fields)) = stat.rsplit_once(") ") {
                let fields: Vec<_> = fields.split_whitespace().take(3).collect();
                if fields.len() == 3
                    && fields[2].parse::<u32>().ok() == Some(child.id())
                    && fields[0] != "Z"
                    && fields[0] != "X"
                {
                    return Ok(false);
                }
            }
        }
    }
    exited(child)
}
#[cfg(windows)]
fn group_exited(inner: &Inner, child: &Child) -> Result<bool, String> {
    match &inner.job {
        Some(job) => job.empty(),
        None => exited(child),
    }
}
fn terminate(inner: &mut Inner, grace: Duration) {
    if let Some(mut child) = inner.child.take() {
        let mut graceful = false;
        if !grace.is_zero() {
            log(&format!(
                "Requesting graceful owned backend shutdown (PID {})",
                child.id()
            ));
            #[cfg(target_os = "linux")]
            if unsafe { libc::kill(-(child.id() as i32), libc::SIGINT) } != 0 {
                log(&format!(
                    "Owned shutdown signal failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            #[cfg(windows)]
            {
                use std::io::Write;
                if let Some(mut stdin) = child.stdin.take() {
                    if let Err(error) = stdin.write_all(b"stop\n") {
                        log(&format!("Owned shutdown request failed: {error}"));
                    }
                }
            }
            let deadline = Instant::now() + grace;
            loop {
                #[cfg(target_os = "linux")]
                let done = group_exited(&child);
                #[cfg(windows)]
                let done = group_exited(inner, &child);
                #[cfg(not(any(target_os = "linux", windows)))]
                let done = exited(&child);
                match done {
                    Ok(true) => {
                        graceful = true;
                        break;
                    }
                    Err(error) => {
                        log(&error);
                    }
                    Ok(false) => {}
                }
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            if graceful {
                log("Owned backend shut down gracefully");
            } else {
                log(
                    "Owned backend graceful shutdown timed out; escalating to forced owned-group cleanup",
                );
            }
        }
        if !graceful {
            #[cfg(target_os = "linux")]
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            #[cfg(windows)]
            if let Some(job) = inner.job.take() {
                job.kill();
            }
            let _ = child.kill();
        }
        #[cfg(windows)]
        {
            inner.job = None;
        }
        match child.wait() {
            Ok(status) => log(&format!("Owned child exited: {status}")),
            Err(error) => log(&format!("Owned child reap failed: {error}")),
        }
    }
    inner.interrupt_source = None;
    inner.environment_lock = None;
    inner.status.endpoint = None;
    inner.status.output_root = None;
    inner.status.model_roots.clear();
}
fn fail(inner: &mut Inner, error: String) {
    terminate(inner, Duration::ZERO);
    inner.status.state = "failed".into();
    log(&error);
    inner.status.error = Some(error);
}
// Require the listening socket to belong to the exact unreaped child, not just
// a successful response on a raced ephemeral port. Do not adopt any other PID.
#[cfg(target_os = "linux")]
fn owns_listener(inner: &Inner, port: u16) -> Result<bool, String> {
    let child = inner.child.as_ref().ok_or("No owned child")?;
    let table = fs::read_to_string("/proc/net/tcp").map_err(|e| e.to_string())?;
    let address = format!("0100007F:{port:04X}");
    let inodes: Vec<&str> = table
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() > 9 && fields[1] == address && fields[3] == "0A" {
                Some(fields[9])
            } else {
                None
            }
        })
        .collect();
    if inodes.len() != 1 {
        return Ok(false);
    }
    for entry in fs::read_dir(format!("/proc/{}/fd", child.id())).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if let Ok(target) = fs::read_link(entry.path()) {
            if inodes
                .iter()
                .any(|inode| target == Path::new(&format!("socket:[{inode}]")))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
// On Windows the venv python.exe is a launcher, so the listener is usually its
// child: accept the spawned PID or a member of its kill-on-close job, nothing else.
#[cfg(windows)]
fn owns_listener(inner: &Inner, port: u16) -> Result<bool, String> {
    let child = inner.child.as_ref().ok_or("No owned child")?;
    let loopback: Vec<u32> = crate::win::listeners(port)?
        .into_iter()
        .filter(|(addr, _)| *addr == Some(crate::win::LOOPBACK_V4))
        .map(|(_, pid)| pid)
        .collect();
    Ok(match loopback[..] {
        [pid] => pid == child.id() || inner.job.as_ref().is_some_and(|job| job.contains(pid)),
        _ => false,
    })
}
#[cfg(not(any(target_os = "linux", windows)))]
fn owns_listener(_: &Inner, _: u16) -> Result<bool, String> {
    Err("Unsupported platform".into())
}
#[cfg(windows)]
fn port_has_listener(port: u16) -> bool {
    crate::win::listeners(port).is_ok_and(|rows| !rows.is_empty())
}
#[cfg(not(windows))]
fn port_has_listener(port: u16) -> bool {
    ["/proc/net/tcp", "/proc/net/tcp6"].iter().any(|path| {
        fs::read_to_string(path).is_ok_and(|table| {
            table.lines().skip(1).any(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                fields.len() > 3
                    && fields[3] == "0A"
                    && fields[1].ends_with(&format!(":{port:04X}"))
            })
        })
    })
}
impl Lifecycle {
    pub(crate) fn active(&self) -> bool {
        locked(&self.inner).child.is_some()
    }
    pub(crate) fn start(&self, approved: Approved) -> Result<Status, String> {
        approved.validate()?;
        if let Some(expected) = &approved.plan.hardware {
            let actual = crate::install::torch_profile::probe()?;
            if actual.uuid != expected.uuid
                || actual.compute_capability != expected.compute_capability
                || actual.driver_version != expected.driver_version
            {
                return Err(
                    "NVIDIA hardware/driver changed; use Backend → Re-run installation".into(),
                );
            }
        }
        self.launch(approved, Duration::from_secs(120))
    }
    fn launch(&self, approved: Approved, timeout: Duration) -> Result<Status, String> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        self.spawn_reserved(approved, timeout, port, Some(listener))
    }
    #[cfg(all(test, target_os = "linux"))]
    fn launch_port(
        &self,
        approved: Approved,
        timeout: Duration,
        port: u16,
    ) -> Result<Status, String> {
        self.spawn_reserved(approved, timeout, port, None)
    }
    fn spawn_reserved(
        &self,
        approved: Approved,
        timeout: Duration,
        port: u16,
        listener: Option<std::net::TcpListener>,
    ) -> Result<Status, String> {
        let mut inner = locked(&self.inner);
        if inner.child.is_some() {
            return Err("An owned backend is already starting or running".into());
        }
        approved.validate()?;
        let root = Path::new(&approved.plan.request.storage_path);
        let environment_lock = lock_environment(root)?;
        install_output_naming(root)?;
        install_adapter(
            root,
            "deas_image_prompting.py",
            include_bytes!("image_prompting.py"),
        )?;
        // Optional capability: unsupported sources must not prevent normal backend startup.
        let mut interrupt_source = InterruptSource::capture(root).ok().and_then(|mut proof| {
            install_adapter(
                root,
                "deas_running_interrupt.py",
                include_bytes!("running_interrupt.py"),
            )
            .ok()?;
            proof.files.push((
                root.join("ComfyUI/custom_nodes/deas_running_interrupt.py"),
                Sha256::digest(include_bytes!("running_interrupt.py")).into(),
            ));
            proof.validate().ok()?;
            Some(proof)
        });
        let mut command = Command::new(root.join(VENV_PYTHON));
        command.arg("-u");
        #[cfg(windows)]
        command.args(["-c", WINDOWS_BOOTSTRAP]);
        command.args([
            "main.py",
            "--listen",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--disable-auto-launch",
            // ComfyUI's default origin middleware answers the Comfy tab's cross-site iframe navigation with 403; CORS mode replaces it.
            "--enable-cors-header",
            "tauri://localhost",
        ]);
        match approved.plan.request.backend {
            Backend::Cpu => {
                command.arg("--cpu");
            }
            Backend::Nvidia => {
                command.args(["--cuda-device", "0"]);
            }
            _ => return Err("Unsupported backend".into()),
        }
        let (folder_args, model_roots) = crate::settings::backend_configuration(root)?;
        // Snapshot the exact argv, not settings that can change while this child runs.
        let output_root = folder_args
            .windows(2)
            .find(|pair| pair[0] == "--output-directory")
            .map(|pair| PathBuf::from(&pair[1]))
            .unwrap_or_else(|| root.join("ComfyUI/output"));
        command.args(folder_args);
        command.args(crate::startup::args(root, Some(&approved.plan))?);
        command
            .env_clear()
            .env("PYTHONUTF8", "1")
            .env("PYTHONNOUSERSITE", "1")
            .env("PYTHONDONTWRITEBYTECODE", "1");
        #[cfg(windows)]
        {
            // Python refuses to start without SYSTEMROOT; System32 supplies only OS DLLs.
            let system_root = std::env::var_os("SYSTEMROOT").unwrap_or_default();
            let mut path = vec![Path::new(&system_root).join("System32")];
            if approved.plan.request.options.ffmpeg {
                path.push(root.join("venv").join("Scripts"));
            }
            command.env(
                "PATH",
                std::env::join_paths(path).map_err(|_| "FFmpeg path invalid".to_string())?,
            );
            // SYSTEMDRIVE: the driver's NGX runtime builds its paths from it; without it every
            // RTX VSR/frame-generation effect fails NvVFX_Load with code -12.
            for name in ["SYSTEMROOT", "SYSTEMDRIVE", "WINDIR", "COMSPEC"] {
                if let Some(value) = std::env::var_os(name) {
                    command.env(name, value);
                }
            }
        }
        #[cfg(not(windows))]
        command.env(
            "PATH",
            if approved.plan.request.options.ffmpeg {
                format!("/usr/bin:/bin:{}", root.join("venv").join("bin").display())
            } else {
                "/usr/bin:/bin".to_string()
            },
        );
        if let Some(proof) = &interrupt_source {
            command.env("DEAS_RUNNING_INTERRUPT_TOKEN", &proof.token);
        }
        for (name, folder) in [
            ("HOME", "home"),
            ("USERPROFILE", "home"),
            ("XDG_CACHE_HOME", "cache"),
            ("LOCALAPPDATA", "cache"),
            ("XDG_CONFIG_HOME", "config"),
            ("APPDATA", "config"),
            ("TMPDIR", "tmp"),
            ("TMP", "tmp"),
            ("TEMP", "tmp"),
        ] {
            command.env(name, root.join(folder));
        }
        if let Some(hardware) = &approved.plan.hardware {
            command.env("CUDA_VISIBLE_DEVICES", &hardware.uuid);
        }
        command
            .current_dir(root.join("ComfyUI"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        command.stdin(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(crate::win::CREATE_NO_WINDOW);
        }
        if interrupt_source
            .as_ref()
            .is_some_and(|proof| proof.validate().is_err())
        {
            interrupt_source = None;
            command.env_remove("DEAS_RUNNING_INTERRUPT_TOKEN");
        }
        // One reservation, one spawn. A bind race fails ownership/readiness; never retry/adopt.
        drop(listener);
        let mut child = command.spawn().map_err(|e| {
            inner.status = Status {
                state: "failed".into(),
                error: Some(format!("Owned backend spawn failed: {e}")),
                ..Status::default()
            };
            inner.status.error.clone().unwrap()
        })?;
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            let job = crate::win::Job::new()
                .and_then(|job| job.assign(child.as_raw_handle()).map(|_| job));
            match job {
                Ok(job) => inner.job = Some(job),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    inner.status = Status {
                        state: "failed".into(),
                        error: Some(error.clone()),
                        ..Status::default()
                    };
                    return Err(error);
                }
            }
        }
        inner.interrupt_source = interrupt_source;
        inner.environment_lock = Some(environment_lock);
        inner.serial += 1;
        let serial = inner.serial;
        inner.port = port;
        inner.status = Status {
            state: "starting".into(),
            output_root: Some(output_root),
            model_roots,
            ..Status::default()
        };
        log(&format!(
            "Starting owned backend on 127.0.0.1:{port}; API readiness only"
        ));
        let stdout = child.stdout.take().ok_or("Missing owned stdout")?;
        let stderr = child.stderr.take().ok_or("Missing owned stderr")?;
        inner.child = Some(child);
        let initial = inner.status.clone();
        drop(inner);
        capture(stdout, self.inner.clone(), serial);
        capture(stderr, self.inner.clone(), serial);
        let state = self.inner.clone();
        std::thread::spawn(move || supervise(state, serial, port, timeout));
        Ok(initial)
    }
    pub fn status(&self) -> Status {
        let mut inner = locked(&self.inner);
        if let Some(child) = &inner.child {
            match exited(child) {
                Ok(true) => fail(&mut inner, "Owned backend exited".into()),
                Err(error) => fail(&mut inner, error),
                Ok(false) => {
                    if inner.status.state == "running"
                        && !owns_listener(&inner, inner.port).unwrap_or(false)
                    {
                        fail(
                            &mut inner,
                            "Owned backend lost its loopback listener".into(),
                        );
                    }
                }
            }
        }
        inner.status.clone()
    }
    /// Revalidate exact owned listener and lifecycle, without adopting an endpoint.
    pub(crate) fn cancellation_owner(
        &self,
    ) -> impl Fn(&reqwest::Url) -> Result<u64, String> + Send + Sync + 'static {
        let inner = self.inner.clone();
        move |base| {
            let inner = locked(&inner);
            let valid = inner.status.state == "running"
                && inner
                    .child
                    .as_ref()
                    .is_some_and(|child| exited(child) == Ok(false))
                && owns_listener(&inner, inner.port).unwrap_or(false)
                && inner
                    .status
                    .endpoint
                    .as_deref()
                    .and_then(|s| reqwest::Url::parse(s).ok())
                    .as_ref()
                    == Some(base)
                && base.scheme() == "http"
                && base.host_str() == Some("127.0.0.1")
                && base.path() == "/"
                && base.username().is_empty()
                && base.password().is_none()
                && base.query().is_none()
                && base.fragment().is_none();
            if valid {
                Ok(inner.serial)
            } else {
                Err("Pending cancellation requires the exact running owned loopback backend".into())
            }
        }
    }

    pub(crate) fn interruption_owner(
        &self,
    ) -> impl Fn(&reqwest::Url) -> Result<(u64, String), String> + Send + Sync + 'static {
        let owner = self.cancellation_owner();
        let inner = self.inner.clone();
        move |base| {
            let epoch = owner(base)?;
            let inner = locked(&inner);
            if inner.serial != epoch {
                return Err("Owned backend lifecycle changed".into());
            }
            let proof = inner
                .interrupt_source
                .as_ref()
                .ok_or("Scoped interruption is unsupported by this launched backend source")?;
            proof
                .validate()
                .map_err(|_| "Scoped interruption source changed or unavailable")?;
            Ok((epoch, proof.token.clone()))
        }
    }

    pub fn stop(&self) -> Status {
        let mut inner = locked(&self.inner);
        inner.serial += 1;
        terminate(&mut inner, STOP_GRACE);
        inner.status.state = "stopped".into();
        inner.status.error = None;
        inner.status.clone()
    }
}
impl Drop for Lifecycle {
    fn drop(&mut self) {
        self.stop();
    }
}
/// Reads one log segment ending at `\n` or `\r` (progress bars redraw with `\r` only, so a
/// newline-only split showed a sampler's `1/4 … 3/4` steps only as the final `4/4`).
pub(crate) fn read_log_segment<R: std::io::BufRead>(
    reader: &mut R,
    line: &mut Vec<u8>,
) -> std::io::Result<usize> {
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(line.len());
        }
        let room = (MAX_LOG_LINE as usize).saturating_sub(line.len());
        let end = buffer.iter().position(|b| matches!(b, b'\n' | b'\r'));
        let take = end.map_or(buffer.len(), |i| i + 1).min(room);
        line.extend_from_slice(&buffer[..take]);
        reader.consume(take);
        if end.is_some_and(|i| i < room) || line.len() >= MAX_LOG_LINE as usize {
            return Ok(line.len());
        }
    }
}
fn capture<R: std::io::Read + Send + 'static>(reader: R, inner: Arc<Mutex<Inner>>, serial: u64) {
    std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(reader);
        let mut line = Vec::new();
        loop {
            line.clear();
            match read_log_segment(&mut reader, &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            // Drain during stop too: cleanup can fill stdout while holding the lifecycle lock.
            if let Ok(guard) = inner.try_lock() {
                if guard.serial != serial {
                    break;
                }
            }
            let text = clean_log_line(&line);
            if !text.is_empty() {
                crate::logs::push(crate::logs::Source::Comfyui, &text);
            }
        }
    });
}
fn supervise(inner: Arc<Mutex<Inner>>, serial: u64, port: u16, timeout: Duration) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let mut g = locked(&inner);
            if g.serial == serial {
                fail(&mut g, error.to_string());
            }
            return;
        }
    };
    let deadline = Instant::now() + timeout;
    let base =
        reqwest::Url::parse(&format!("http://127.0.0.1:{port}/")).expect("constant loopback URL");
    let client = match reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_millis(500))
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            let mut g = locked(&inner);
            if g.serial == serial {
                fail(&mut g, error.to_string());
            }
            return;
        }
    };
    let mut last_error = "Owned listener not ready".to_string();
    loop {
        {
            let mut guard = locked(&inner);
            if guard.serial != serial || guard.child.is_none() {
                return;
            }
            let child = guard.child.as_ref().unwrap();
            match exited(child) {
                Ok(false) => {}
                Ok(true) => {
                    fail(&mut guard, "Owned backend exited before readiness".into());
                    return;
                }
                Err(error) => {
                    fail(&mut guard, error);
                    return;
                }
            }
            if Instant::now() >= deadline {
                fail(
                    &mut guard,
                    format!("Backend readiness timed out: {last_error}"),
                );
                return;
            }
            if !owns_listener(&guard, port).unwrap_or(false) {
                if port_has_listener(port) && !owns_listener(&guard, port).unwrap_or(false) {
                    fail(
                        &mut guard,
                        "Startup collision: foreign listener owns the chosen port".into(),
                    );
                    return;
                }
                drop(guard);
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
        }
        let result = runtime
            .block_on(async {
                tokio::time::timeout(deadline.saturating_duration_since(Instant::now()), async {
                    let nodes = crate::get_json(&client, &base, "object_info", false)
                        .await?
                        .ok_or("Missing object_info")?;
                    let stats = crate::get_json(&client, &base, "system_stats", false)
                        .await?
                        .ok_or("Missing system_stats")?;
                    crate::parse_snapshot(&base, &nodes, &stats, None).map(|_| ())
                })
                .await
            })
            .unwrap_or_else(|_| {
                Err(format!(
                    "Overall readiness deadline reached; last probe: {last_error}"
                ))
            });
        let mut guard = locked(&inner);
        if guard.serial != serial || guard.child.is_none() {
            return;
        }
        let child = guard.child.as_ref().unwrap();
        if exited(child).unwrap_or(true) {
            fail(&mut guard, "Owned backend exited during API probe".into());
            return;
        }
        if result.is_ok()
            && Instant::now() < deadline
            && owns_listener(&guard, port).unwrap_or(false)
        {
            guard.status.state = "running".into();
            guard.status.endpoint = Some(base.to_string());
            log("Owned ComfyUI API responded; generation/model readiness remains unverified");
            // Liveness only; the UI polls once a second too, so 100 ms would be 10x wasted /proc scans.
            drop(guard);
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let mut g = locked(&inner);
                if g.serial != serial || g.child.is_none() {
                    return;
                }
                if exited(g.child.as_ref().unwrap()).unwrap_or(true) {
                    fail(&mut g, "Owned backend exited".into());
                    return;
                }
                if !owns_listener(&g, port).unwrap_or(false) {
                    fail(&mut g, "Owned backend lost its loopback listener".into());
                    return;
                }
            }
        }
        if let Err(error) = result {
            last_error = error;
        }
        drop(guard);
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "backend_tests.rs"]
pub(crate) mod tests;

#[tauri::command]
pub fn native_backend_start(
    engine: tauri::State<'_, crate::install::Engine>,
    operations: tauri::State<'_, crate::FolderOperations>,
) -> Result<Status, String> {
    let _guard = operations.enter()?;
    engine.backend_start()
}
#[derive(Serialize)]
pub struct DependencyCheck {
    pub id: &'static str,
    pub ok: bool,
}
#[tauri::command]
pub async fn native_backend_status(
    engine: tauri::State<'_, crate::install::Engine>,
) -> Result<Status, String> {
    let backend = engine.backend.clone();
    tauri::async_runtime::spawn_blocking(move || backend.status())
        .await
        .map_err(|error| format!("Backend status worker failed: {error}"))
}
#[tauri::command]
pub fn native_backend_test(
    engine: tauri::State<'_, crate::install::Engine>,
) -> Vec<DependencyCheck> {
    engine.backend_test()
}
#[tauri::command]
pub fn native_backend_stop(engine: tauri::State<'_, crate::install::Engine>) -> Status {
    engine.backend.stop()
}
