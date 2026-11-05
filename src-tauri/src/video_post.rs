//! Read-only, chooser-authorized source metadata. No inference or execution readiness.
use serde::Serialize;
use serde_json::Value;
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const INVALID: &str = "VIDEO_METADATA: Invalid or incomplete video metadata; choose a supported MP4 or Matroska video.";
const SOURCE: &str =
    "VIDEO_SOURCE: Choose an unchanged, regular local MP4 or Matroska file (up to 64 GiB).";
const SYMLINK: &str = "VIDEO_SYMLINK: The selected file is a symbolic link; choose the real file.";
const SIZE: &str = "VIDEO_SIZE: Video sources must be between 12 bytes and 64 GiB.";
const TYPE: &str = "VIDEO_TYPE: The selected file is not an MP4/MOV/MKV/WebM video (container signature mismatch).";
const OUTPUT_LIMIT: usize = 1024 * 1024;
const STDERR_LIMIT: usize = 16 * 1024;
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) static INSPECTING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSourceInspection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps_numerator: u32,
    pub fps_denominator: u32,
    pub duration_seconds: Option<f64>,
    pub pixel_format: String,
    pub field_order: String,
    pub color_transfer: String,
    pub audio_streams: usize,
    pub subtitle_streams: usize,
    pub blockers: Vec<String>,
    pub timing_verified: bool,
    pub processing_ready: bool,
}

pub(crate) fn finish_operation<T>(
    operation: &str,
    success: &str,
    id: uuid::Uuid,
    result: Result<Option<T>, String>,
) -> Result<Option<T>, String> {
    let (severity, outcome) = match &result {
        Ok(None) => ("INFO", "cancelled"),
        Ok(Some(_)) => ("INFO", success),
        Err(_) => ("ERROR", "failed"),
    };
    // Never forward titles, source names, tool output or OS errors to shared Logs.
    crate::logs::push(
        crate::logs::Source::Deas,
        &format!("{severity}: video-post operation={operation} operationId={id} outcome={outcome}"),
    );
    result
}

fn finish<T>(id: uuid::Uuid, result: Result<Option<T>, String>) -> Result<Option<T>, String> {
    finish_operation("inspect", "inspected", id, result)
}
pub(crate) fn validate_title(title: &str) -> Result<(), String> {
    if title.trim().is_empty() || title.len() > 240 || title.chars().any(char::is_control) {
        return Err("VIDEO_TITLE: Use a nonempty chooser title of at most 240 bytes without control characters.".into());
    }
    Ok(())
}

/// OS-chooser seam: no caller-provided file path, program or probe arguments.
#[tauri::command]
pub async fn native_video_post_inspect(
    title: String,
) -> Result<Option<VideoSourceInspection>, String> {
    let id = uuid::Uuid::new_v4();
    if let Err(error) = validate_title(&title) {
        return finish(id, Err(error));
    }
    let Some(permit) = acquire() else {
        return busy(id);
    };
    let selected = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter("Video", &["mp4", "mov", "m4v", "mkv", "webm"])
        .pick_file()
        .await;
    let Some(selected) = selected else {
        return finish(id, Ok(None));
    };
    inspect_owned(id, permit, selected.path().to_owned()).await
}

/// Media-panel seam: an absolute path of a file DEAS already lists (input/output roots); the same
/// source checks, admission and retention as the chooser, so a listed tile can become the TVE source.
#[tauri::command]
pub async fn native_video_post_inspect_path(
    path: String,
) -> Result<Option<VideoSourceInspection>, String> {
    let id = uuid::Uuid::new_v4();
    let path = std::path::PathBuf::from(&path);
    if !path.is_absolute() || path.as_os_str().is_empty() {
        return finish(
            id,
            Err(format!(
                "{SOURCE}: Media selection must be an absolute path."
            )),
        );
    }
    let Some(permit) = acquire() else {
        return busy(id);
    };
    inspect_owned(id, permit, path).await
}

fn acquire() -> Option<tokio::sync::SemaphorePermit<'static>> {
    INSPECTING.try_acquire().ok()
}
fn busy(id: uuid::Uuid) -> Result<Option<VideoSourceInspection>, String> {
    finish(
        id,
        Err(
            "VIDEO_BUSY: A video inspection is already open; finish it before trying again.".into(),
        ),
    )
}

async fn inspect_owned(
    id: uuid::Uuid,
    permit: tokio::sync::SemaphorePermit<'static>,
    path: std::path::PathBuf,
) -> Result<Option<VideoSourceInspection>, String> {
    // The owner keeps admission, cleanup and diagnostic delivery if its IPC waiter disappears.
    match tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        finish(
            id,
            inspect_retained(&path)
                .and_then(crate::video_post_source::retain_source)
                .map(Some),
        )
    })
    .await
    {
        Ok(result) => result,
        Err(_) => finish(
            id,
            Err("VIDEO_INSPECT: Inspection worker failed; retry the selection.".into()),
        ),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    id: (u64, u64),
    size: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    changed: (i64, i64),
}
pub(crate) fn identity(file: &File) -> Result<Identity, String> {
    let m = file.metadata().map_err(|_| SOURCE)?;
    if !m.is_file() {
        return Err(SOURCE.into());
    }
    if !(12..=64 * 1024 * 1024 * 1024).contains(&m.len()) {
        return Err(SIZE.into());
    }
    #[cfg(unix)]
    let id = {
        use std::os::unix::fs::MetadataExt;
        (m.dev(), m.ino())
    };
    #[cfg(windows)]
    let id = crate::win::identity(file).map_err(|_| SOURCE)?;
    #[cfg(not(any(unix, windows)))]
    let id = return Err("VIDEO_PLATFORM: Source inspection supports Linux and Windows.".into());
    Ok(Identity {
        id,
        size: m.len(),
        modified: m.modified().map_err(|_| SOURCE)?,
        #[cfg(unix)]
        changed: {
            use std::os::unix::fs::MetadataExt;
            (m.ctime(), m.ctime_nsec())
        },
    })
}
pub(crate) fn open_source(path: &Path) -> Result<File, String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(SOURCE.into());
    }
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        if !matches!(path.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
        {
            return Err(SOURCE.into());
        }
    }
    // Parent-directory symlinks are ordinary navigation (bookmarks like ~/Downloads);
    // only the leaf is rejected — O_NOFOLLOW/open-no-follow enforce it at open time.
    let leaf = fs::symlink_metadata(path).map_err(|_| SOURCE)?;
    if leaf.file_type().is_symlink() {
        return Err(SYMLINK.into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if leaf.file_attributes() & 0x400 != 0 {
            return Err(SYMLINK.into());
        }
    }
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|_| SOURCE)?
    };
    #[cfg(windows)]
    let file = crate::win::open_no_follow(path).map_err(|_| SOURCE)?;
    #[cfg(not(any(unix, windows)))]
    let file = File::open(path).map_err(|_| SOURCE)?;
    identity(&file)?;
    Ok(file)
}

/// Native-only inspection helper; not registered as IPC. The tool is always ffprobe on PATH.
/// shortcut: pre/post identity checks are not a sandbox against hostile same-user path races.
pub fn inspect_source(path: &Path) -> Result<VideoSourceInspection, String> {
    inspect_retained(path).map(|source| source.metadata)
}

pub(crate) struct RetainedSource {
    path: std::path::PathBuf,
    pub(crate) metadata: VideoSourceInspection,
    file: File,
    identity: Identity,
}
impl RetainedSource {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn recheck(&self) -> Result<(), String> {
        if identity(&self.file)? != self.identity
            || identity(&open_source(&self.path)?)? != self.identity
        {
            return Err(SOURCE.into());
        }
        Ok(())
    }
}
fn inspect_retained(path: &Path) -> Result<RetainedSource, String> {
    let mut file = open_source(path)?;
    let before = identity(&file)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.chars().any(char::is_control) && n.len() <= 1024)
        .ok_or(SOURCE)?;
    let mut header = [0u8; 12];
    file.read_exact(&mut header).map_err(|_| SOURCE)?;
    let demuxer = if &header[4..8] == b"ftyp" {
        "mov"
    } else if header[..4] == [0x1a, 0x45, 0xdf, 0xa3] {
        "matroska"
    } else {
        return Err(TYPE.into());
    };
    let mut command = Command::new("ffprobe");
    command.args([
        "-v",
        "error",
        "-protocol_whitelist",
        "file",
        "-f",
        demuxer,
        "-probesize",
        "4194304",
        "-analyzeduration",
        "1000000",
        "-max_alloc",
        "67108864",
    ]);
    if demuxer == "mov" {
        command.args(["-enable_drefs", "0", "-use_absolute_path", "0"]);
    }
    command
        .args(["-show_streams", "-show_format", "-of", "json", "-i"])
        .arg(path);
    let bytes = run_probe(command, PROBE_TIMEOUT)?;
    if identity(&file)? != before || identity(&open_source(path)?)? != before {
        return Err("VIDEO_CHANGED: Source changed during inspection; select it again.".into());
    }
    let metadata = parse_metadata(name.to_owned(), &bytes)?;
    Ok(RetainedSource {
        path: path.to_owned(),
        metadata,
        file,
        identity: before,
    })
}

struct OwnedProbe {
    child: Child,
    #[cfg(windows)]
    job: crate::win::Job,
}
impl OwnedProbe {
    fn spawn(mut command: Command) -> Result<Self, String> {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
            #[cfg(target_os = "linux")]
            unsafe {
                let parent = libc::getpid();
                // The initial inspector also runs inside a force-killable render worker.
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
        let job =
            crate::win::Job::new().map_err(|_| "VIDEO_PROBE: Cannot create owned probe job.")?;
        let child = command.spawn().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound {
            "VIDEO_TOOL_MISSING: Install FFmpeg (including ffprobe) and make ffprobe available on PATH, then restart DEAS."
        } else { "VIDEO_PROBE: Cannot start ffprobe; check the installed FFmpeg tools and permissions." })?;
        let owned = Self {
            child,
            #[cfg(windows)]
            job,
        };
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            owned
                .job
                .assign(owned.child.as_raw_handle())
                .map_err(|_| "VIDEO_PROBE: Cannot own ffprobe process.")?;
        }
        Ok(owned)
    }
    fn exited(&self) -> Result<bool, String> {
        #[cfg(unix)]
        {
            // Same WNOWAIT ownership proof as backend.rs: never reap before group cleanup.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            if unsafe {
                libc::waitid(
                    libc::P_PID,
                    self.child.id(),
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            } != 0
            {
                return Err("VIDEO_PROBE: Cannot monitor ffprobe.".into());
            }
            Ok(unsafe { info.si_pid() } != 0)
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            crate::win::exited(self.child.as_raw_handle())
                .map_err(|_| "VIDEO_PROBE: Cannot monitor ffprobe.".into())
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err("VIDEO_PLATFORM: Source inspection supports Linux and Windows.".into())
        }
    }
    fn terminate(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        #[cfg(windows)]
        self.job.kill();
        let _ = self.child.kill();
    }
}
impl Drop for OwnedProbe {
    fn drop(&mut self) {
        self.terminate();
        let _ = self.child.wait();
    }
}
fn capture(reader: impl Read, limit: usize, overflow: &AtomicBool) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "VIDEO_PROBE: Cannot read ffprobe response.")?;
    if bytes.len() > limit {
        overflow.store(true, Ordering::Relaxed);
    }
    Ok(bytes)
}
pub(crate) fn run_probe(command: Command, timeout: Duration) -> Result<Vec<u8>, String> {
    let mut owned = OwnedProbe::spawn(command)?;
    let stdout = owned
        .child
        .stdout
        .take()
        .ok_or("VIDEO_PROBE: Missing probe stdout.")?;
    let stderr = owned
        .child
        .stderr
        .take()
        .ok_or("VIDEO_PROBE: Missing probe stderr.")?;
    let overflow = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let output = scope.spawn(|| capture(stdout, OUTPUT_LIMIT, &overflow));
        let errors = scope.spawn(|| capture(stderr, STDERR_LIMIT, &overflow));
        let deadline = Instant::now() + timeout;
        let outcome = loop {
            if overflow.load(Ordering::Relaxed) {
                break Err("VIDEO_PROBE_LIMIT: ffprobe metadata exceeded its output limit.");
            }
            match owned.exited() {
                Ok(true) => break Ok(()),
                Err(_) => break Err("VIDEO_PROBE: Cannot monitor ffprobe."),
                Ok(false) if Instant::now() >= deadline => {
                    break Err(
                        "VIDEO_PROBE_TIMEOUT: ffprobe exceeded the 10-second inspection deadline; try a smaller or repaired source.",
                    );
                }
                Ok(false) => std::thread::sleep(Duration::from_millis(10)),
            }
        };
        // `exited` reads the exit code, which Windows sets before the process object is
        // signalled; the job kill below can still overwrite it with 1 until then.
        #[cfg(windows)]
        let status = match outcome {
            Ok(()) => Some(
                owned
                    .child
                    .wait()
                    .map_err(|_| "VIDEO_PROBE: Cannot read probe status.")?,
            ),
            Err(_) => None,
        };
        owned.terminate();
        // Join while the unreaped leader still protects the owned process-group ID.
        let output = output
            .join()
            .map_err(|_| "VIDEO_PROBE: Output reader failed.")?;
        let errors = errors
            .join()
            .map_err(|_| "VIDEO_PROBE: Error reader failed.")?;
        outcome?;
        let output = output?;
        let errors = errors?;
        if overflow.load(Ordering::Relaxed) {
            return Err("VIDEO_PROBE_LIMIT: ffprobe metadata exceeded its output limit.".into());
        }
        // Reap only in Drop, after process-group cleanup; waitid has preserved exit status.
        #[cfg(unix)]
        let success = {
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            let ok = unsafe {
                libc::waitid(
                    libc::P_PID,
                    owned.child.id(),
                    &mut info,
                    libc::WEXITED | libc::WNOWAIT,
                )
            } == 0;
            ok && info.si_code == libc::CLD_EXITED && unsafe { info.si_status() } == 0
        };
        #[cfg(windows)]
        let success = status.is_some_and(|s| s.success());
        #[cfg(not(any(unix, windows)))]
        let success = false;
        if !success || !errors.is_empty() {
            return Err("VIDEO_PROBE_FAILED: ffprobe could not inspect this source cleanly; try a repaired supported video.".into());
        }
        Ok(output)
    })
}

fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 128
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
        })
        .ok_or_else(|| INVALID.into())
}
fn optional_text<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    if v.get(key).is_none() {
        Ok("unknown")
    } else {
        text(v, key)
    }
}
fn rate(v: &Value, key: &str) -> Result<Option<(u32, u32)>, String> {
    let Some(value) = v.get(key) else {
        return Ok(None);
    };
    let raw = value.as_str().ok_or(INVALID)?;
    if raw == "0/0" || raw == "N/A" {
        return Ok(None);
    }
    let (n, d) = raw.split_once('/').ok_or(INVALID)?;
    let parse = |s: &str| -> Result<u32, String> {
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(INVALID.into());
        }
        s.parse::<u32>().map_err(|_| INVALID.into())
    };
    let (n, d) = (parse(n)?, parse(d)?);
    if d == 0 {
        return Err(INVALID.into());
    }
    if n == 0 {
        return Ok(None);
    }
    if u64::from(n) > 1000 * u64::from(d) {
        return Err(INVALID.into());
    }
    let (mut a, mut b) = (n, d);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    Ok(Some((n / a, d / a)))
}
fn duration(v: Option<&Value>) -> Result<Option<f64>, String> {
    let Some(v) = v else { return Ok(None) };
    let raw = v.as_str().ok_or(INVALID)?;
    if raw == "N/A" {
        return Ok(None);
    }
    let n = raw.parse::<f64>().map_err(|_| INVALID)?;
    if !n.is_finite() || n <= 0.0 || n > 7.0 * 24.0 * 60.0 * 60.0 {
        return Err(INVALID.into());
    }
    Ok(Some(n))
}
fn parse_metadata(name: String, bytes: &[u8]) -> Result<VideoSourceInspection, String> {
    let root: Value = serde_json::from_slice(bytes).map_err(|_| INVALID)?;
    let streams = root
        .get("streams")
        .and_then(Value::as_array)
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .ok_or(INVALID)?;
    let format = root
        .get("format")
        .filter(|f| f.is_object())
        .ok_or(INVALID)?;
    let format_name = format
        .get("format_name")
        .and_then(Value::as_str)
        .ok_or(INVALID)?;
    if !format_name
        .split(',')
        .any(|s| matches!(s, "mov" | "mp4" | "matroska" | "webm"))
    {
        return Err(INVALID.into());
    }
    let mut videos = Vec::new();
    let (mut audio, mut subtitles) = (0, 0);
    for stream in streams {
        match text(stream, "codec_type")? {
            "video" => videos.push(stream),
            "audio" => audio += 1,
            "subtitle" => subtitles += 1,
            "data" | "attachment" => (),
            _ => return Err(INVALID.into()),
        }
    }
    // Multiple video tracks are structurally ambiguous; never silently pick the first.
    if videos.len() != 1 {
        return Err("VIDEO_STREAMS: Select a source with exactly one video stream.".into());
    }
    let v = videos[0];
    let dimension = |key| -> Result<u32, String> {
        v.get(key)
            .and_then(Value::as_u64)
            .filter(|n| (1..=65536).contains(n))
            .map(|n| n as u32)
            .ok_or_else(|| INVALID.into())
    };
    let (width, height) = (dimension("width")?, dimension("height")?);
    let pixel = text(v, "pix_fmt")?;
    let field = optional_text(v, "field_order")?;
    let transfer = optional_text(v, "color_transfer")?;
    let primaries = optional_text(v, "color_primaries")?;
    let space = optional_text(v, "color_space")?;
    let nominal = rate(v, "r_frame_rate")?;
    let average = rate(v, "avg_frame_rate")?;
    let stream_duration = duration(v.get("duration"))?;
    let format_duration = duration(format.get("duration"))?;
    let mut blockers = Vec::new();
    let mut block = |code: &str| {
        if !blockers.iter().any(|b| b == code) {
            blockers.push(code.to_owned());
        }
    };
    if nominal.is_none() || average.is_none() {
        block("cadenceUnknown");
    } else if nominal != average {
        block("cadenceMismatch");
    }
    match field {
        "progressive" => (),
        "tt" | "bb" | "tb" | "bt" => block("interlaced"),
        "unknown" => block("fieldOrderUnknown"),
        _ => return Err(INVALID.into()),
    }
    if width < 16
        || height < 16
        || width > 8192
        || height > 8192
        || width % 2 != 0
        || height % 2 != 0
    {
        block("unsupportedDimensions");
    }
    if [
        "yuva", "gbrap", "rgba", "bgra", "argb", "abgr", "ya", "ayuv",
    ]
    .iter()
    .any(|prefix| pixel.starts_with(prefix))
        || matches!(pixel, "pal8" | "vuya")
    {
        block("alpha");
    }
    if pixel != "yuv420p" {
        block("unsupportedPixelFormat");
    }
    if matches!(transfer, "smpte2084" | "arib-std-b67")
        || primaries == "bt2020"
        || space.starts_with("bt2020")
    {
        block("hdr");
    }
    if transfer != "bt709" || primaries != "bt709" || space != "bt709" {
        block("colorUnknown");
    }
    match v.get("sample_aspect_ratio") {
        None => block("pixelAspectUnknown"),
        Some(Value::String(s)) if matches!(s.as_str(), "N/A" | "0:1" | "0:0") => {
            block("pixelAspectUnknown")
        }
        Some(Value::String(s)) => {
            let (n, d) = s.split_once(':').ok_or(INVALID)?;
            let parse = |s: &str| -> Result<u32, String> {
                if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(INVALID.into());
                }
                s.parse().map_err(|_| INVALID.into())
            };
            let (n, d) = (parse(n)?, parse(d)?);
            if n == 0 || d == 0 {
                return Err(INVALID.into());
            }
            if n != d {
                block("nonSquarePixels");
            }
        }
        Some(_) => return Err(INVALID.into()),
    }
    if let Some(tags) = v.get("tags") {
        let tags = tags.as_object().ok_or(INVALID)?;
        for (key, value) in tags {
            if key.eq_ignore_ascii_case("rotate") {
                let rotation = value
                    .as_str()
                    .ok_or(INVALID)?
                    .parse::<f64>()
                    .map_err(|_| INVALID)?;
                if !rotation.is_finite() {
                    return Err(INVALID.into());
                }
                if rotation != 0.0 {
                    block("rotation");
                }
            }
        }
    }
    if let Some(side) = v.get("side_data_list") {
        for entry in side.as_array().ok_or(INVALID)? {
            let kind = entry
                .get("side_data_type")
                .and_then(Value::as_str)
                .ok_or(INVALID)?;
            if kind == "Display Matrix" {
                // A matrix may contain mirroring/shear even with a zero rotation summary.
                block("rotation");
                let angle = entry
                    .get("rotation")
                    .and_then(Value::as_f64)
                    .ok_or(INVALID)?;
                if !angle.is_finite() {
                    return Err(INVALID.into());
                }
            } else if entry.get("rotation").is_some() {
                block("rotation");
            }
            if kind.contains("Mastering display")
                || kind.contains("Content light")
                || kind.contains("DOVI")
                || kind.contains("HDR")
            {
                block("hdr");
            }
        }
    }
    let (fps_numerator, fps_denominator) = average.or(nominal).unwrap_or((0, 1));
    Ok(VideoSourceInspection {
        source_id: None,
        name,
        width,
        height,
        fps_numerator,
        fps_denominator,
        duration_seconds: stream_duration.or(format_duration),
        pixel_format: pixel.into(),
        field_order: field.into(),
        color_transfer: transfer.into(),
        audio_streams: audio,
        subtitle_streams: subtitles,
        blockers,
        timing_verified: false,
        processing_ready: false,
    })
}

#[cfg(test)]
pub(crate) fn test_retained(path: &Path, metadata: VideoSourceInspection) -> RetainedSource {
    let file = open_source(path).unwrap();
    let identity = identity(&file).unwrap();
    RetainedSource {
        path: path.to_owned(),
        metadata,
        file,
        identity,
    }
}

#[cfg(test)]
#[path = "video_post_tests.rs"]
mod tests;
