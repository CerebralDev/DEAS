//! Typed playback IPC. No frontend-supplied mpv commands or arbitrary network URLs.
use serde::Deserialize;
use serde_json::Value;
#[cfg(target_os = "linux")]
use tauri::Manager;

#[cfg(target_os = "linux")]
#[path = "video_linux.rs"]
mod linux;

#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum Source {
    File {
        path: String,
    },
    Output {
        #[serde(rename = "baseUrl")]
        base_url: String,
        filename: String,
        subfolder: String,
        #[serde(rename = "type")]
        kind: String,
    },
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Action {
    Bounds {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        visible: bool,
        #[serde(default, rename = "audioOnly")]
        audio_only: bool,
    },
    Pause {
        paused: bool,
    },
    Seek {
        seconds: f64,
    },
    Crop {
        start: f64,
        end: f64,
    },
    Mute {
        muted: bool,
    },
    Volume {
        volume: f64,
    },
    Status,
    Close,
}
impl Action {
    fn validate(&self) -> Result<(), String> {
        let valid = match self {
            Self::Bounds {
                x,
                y,
                width,
                height,
                ..
            } => {
                *x >= 0
                    && *y >= 0
                    && *x <= 32768
                    && *y <= 32768
                    && (0..=16384).contains(width)
                    && (0..=16384).contains(height)
            }
            Self::Seek { seconds } => seconds.is_finite() && (0.0..=86400.0).contains(seconds),
            Self::Crop { start, end } => {
                start.is_finite()
                    && end.is_finite()
                    && *start >= 0.0
                    && *start < *end
                    && *end <= 86400.0
            }
            Self::Volume { volume } => volume.is_finite() && (0.0..=100.0).contains(volume),
            _ => true,
        };
        if valid {
            Ok(())
        } else {
            Err("Invalid video control value".into())
        }
    }
}
#[cfg(any(target_os = "linux", test))]
fn output_url(
    base: &str,
    filename: &str,
    subfolder: &str,
    kind: &str,
) -> Result<reqwest::Url, String> {
    let base = crate::validate_base_url(base)?;
    if crate::media::media_kind(filename) != Some("video")
        || filename.contains(['/', '\\', '\0'])
        || filename.len() > 255
        || subfolder.len() > 4096
        || subfolder.contains([':', '\0'])
        || subfolder.split(['/', '\\']).any(|p| p == ".." || p == ".")
        || subfolder.starts_with(['/', '\\'])
        || !["output", "temp", "input"].contains(&kind)
    {
        return Err("Invalid backend video output".into());
    }
    let mut url = reqwest::Url::parse(&format!("{base}view")).map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("filename", filename)
        .append_pair("subfolder", subfolder)
        .append_pair("type", kind);
    Ok(url)
}
#[cfg(any(target_os = "linux", test))]
fn local_path(path: &str) -> Result<std::path::PathBuf, String> {
    let p = std::path::Path::new(path);
    if !p.is_absolute() || !matches!(crate::media::media_kind(path), Some("video" | "audio")) {
        return Err("Select an absolute video or audio file path".into());
    }
    let p = p
        .canonicalize()
        .map_err(|e| format!("Cannot open video: {e}"))?;
    if !p.is_file() {
        return Err("Video is not a regular file".into());
    }
    Ok(p)
}
#[cfg(target_os = "linux")]
async fn on_main<T: Send + 'static>(
    _app: &tauri::AppHandle,
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    // Tauri's idle-priority dispatch can starve behind continuous GTK rendering.
    let mut work = Some((tx, f));
    gtk::glib::idle_add_full(gtk::glib::Priority::DEFAULT, move || {
        if let Some((tx, f)) = work.take() {
            let _ = tx.send(f());
        }
        gtk::glib::ControlFlow::Break
    });
    rx.await.map_err(|_| "Video UI stopped".to_string())?
}
#[tauri::command]
pub async fn native_video_open(
    window: tauri::WebviewWindow,
    id: String,
    source: Source,
    auto_play: bool,
    repeat: bool,
    muted: Option<bool>,
) -> Result<(), String> {
    let result = open_inner(
        &window,
        id.clone(),
        source,
        auto_play,
        repeat,
        muted.unwrap_or(false),
    )
    .await;
    let cancelled = result
        .as_ref()
        .err()
        .is_some_and(|e| e == "Video preview cancelled");
    notice(
        &id,
        if cancelled { "cancel" } else { "open" },
        result.is_err() && !cancelled,
    );
    result
}
async fn open_inner(
    window: &tauri::WebviewWindow,
    id: String,
    source: Source,
    auto_play: bool,
    repeat: bool,
    muted: bool,
) -> Result<(), String> {
    if window.label() != "main" || uuid::Uuid::parse_str(&id).is_err() {
        return Err("Invalid video owner".into());
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (source, auto_play, repeat, muted);
        Err("Native video is Linux-only".into())
    }
    #[cfg(target_os = "linux")]
    {
        let pending = Pending::start(&id)?;
        let mut cancelled = pending.cancelled.clone();
        tokio::select! {
          _=cancelled.changed()=>Err("Video preview cancelled".into()),
          result=prepare_video(window,id,source,auto_play,repeat,muted,pending)=>result,
        }
    }
}
#[cfg(target_os = "linux")]
async fn prepare_video(
    window: &tauri::WebviewWindow,
    id: String,
    source: Source,
    auto_play: bool,
    repeat: bool,
    muted: bool,
    pending: Pending,
) -> Result<(), String> {
    let app = window.app_handle();
    let (path, temporary) = match source {
        Source::File { path } => {
            let path = local_path(&path)?;
            if !app.asset_protocol_scope().is_allowed(&path) {
                return Err("Video is outside the media scope".into());
            }
            (path, None)
        }
        Source::Output {
            base_url,
            filename,
            subfolder,
            kind,
        } => {
            let url = output_url(&base_url, &filename, &subfolder, &kind)?;
            // Backend data is bounded and never passed as a URL to libmpv (redirects/playlists).
            let mut response = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .map_err(|e| e.to_string())?
                .get(url)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err("Backend video redirect rejected".into());
            }
            const LIMIT: u64 = 1024 * 1024 * 1024;
            if response.content_length().is_some_and(|n| n > LIMIT) {
                return Err("Backend video exceeds 1 GiB preview limit".into());
            }
            let file = tempfile::Builder::new()
                .suffix(".video")
                .tempfile()
                .map_err(|e| e.to_string())?;
            let mut bytes = 0;
            use std::io::Write;
            while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
                bytes += chunk.len() as u64;
                if bytes > LIMIT {
                    return Err("Backend video exceeds 1 GiB preview limit".into());
                }
                (&file).write_all(&chunk).map_err(|e| e.to_string())?;
            }
            (file.path().to_owned(), Some(file))
        }
    };
    check_signature(&path)?;
    let cancelled = pending.cancelled.clone();
    on_main(app, move || {
        if *cancelled.borrow() {
            return Err("Video preview cancelled".into());
        }
        linux::open(id, path, temporary, auto_play, repeat, muted)
    })
    .await
}
#[tauri::command]
pub async fn native_video_update(
    window: tauri::WebviewWindow,
    id: String,
    action: Action,
) -> Result<Value, String> {
    if window.label() != "main" || uuid::Uuid::parse_str(&id).is_err() {
        return Err("Invalid video owner".into());
    }
    let result = update_inner(&window, id.clone(), action).await;
    if result.is_err() {
        notice(&id, "control", true);
    }
    result
}
async fn update_inner(
    window: &tauri::WebviewWindow,
    id: String,
    action: Action,
) -> Result<Value, String> {
    action.validate()?;
    #[cfg(target_os = "linux")]
    if matches!(action, Action::Close) {
        Pending::cancel(&id);
    }
    #[cfg(target_os = "linux")]
    {
        on_main(window.app_handle(), move || linux::update(&id, action)).await
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window, id);
        Err("Native video is Linux-only".into())
    }
}
pub fn install(app: &tauri::AppHandle) {
    #[cfg(target_os = "linux")]
    if linux::install(app).is_err() {
        notice("shell", "install", true);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = app;
}
pub fn shutdown() {
    #[cfg(target_os = "linux")]
    linux::shutdown();
}
#[cfg(target_os = "linux")]
static PENDING: std::sync::Mutex<
    std::collections::BTreeMap<String, tokio::sync::watch::Sender<bool>>,
> = std::sync::Mutex::new(std::collections::BTreeMap::new());
#[cfg(target_os = "linux")]
struct Pending {
    id: String,
    cancelled: tokio::sync::watch::Receiver<bool>,
}
#[cfg(target_os = "linux")]
impl Pending {
    fn start(id: &str) -> Result<Self, String> {
        let mut pending = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        if pending.len() >= 8 || pending.contains_key(id) {
            return Err("Too many pending previews".into());
        }
        let (tx, cancelled) = tokio::sync::watch::channel(false);
        pending.insert(id.into(), tx);
        Ok(Self {
            id: id.into(),
            cancelled,
        })
    }
    fn cancel(id: &str) {
        if let Some(tx) = PENDING.lock().unwrap_or_else(|e| e.into_inner()).get(id) {
            let _ = tx.send(true);
        }
    }
}
#[cfg(target_os = "linux")]
impl Drop for Pending {
    fn drop(&mut self) {
        PENDING
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.id);
    }
}
#[cfg(any(target_os = "linux", test))]
fn check_signature(path: &std::path::Path) -> Result<(), String> {
    use std::io::Read;
    let mut header = [0; 64];
    let n = std::fs::File::open(path)
        .and_then(|mut f| f.read(&mut header))
        .map_err(|_| "Cannot read video".to_string())?;
    // Audio files (YuE2 FLAC, references) play audio-only: WebKitGTK may lack their GStreamer decoders.
    if crate::jobs::sniff_media(&header[..n])
        .is_some_and(|(kind, _)| kind == "video" || kind == "audio")
    {
        Ok(())
    } else {
        Err(
            "Preview requires a video (MP4, MOV, WebM, Matroska) or audio file, not a playlist"
                .into(),
        )
    }
}
pub(super) fn notice(id: &str, operation: &str, error: bool) {
    use std::{
        collections::VecDeque,
        sync::Mutex,
        time::{Duration, Instant},
    };
    static RECENT: Mutex<VecDeque<(String, String, Instant)>> = Mutex::new(VecDeque::new());
    let id = uuid::Uuid::parse_str(id)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| "shell".into());
    let mut recent = RECENT.lock().unwrap_or_else(|e| e.into_inner());
    if error {
        if recent
            .iter()
            .any(|(i, o, t)| i == &id && o == operation && t.elapsed() < Duration::from_secs(60))
        {
            return;
        }
        if recent.len() >= 64 {
            recent.pop_front();
        }
        recent.push_back((id.clone(), operation.to_string(), Instant::now()));
    }
    crate::logs::push(
        crate::logs::Source::Deas,
        &format!(
            "{}: Studio NativeVideo {operation} player={id}{}",
            if error { "ERROR" } else { "INFO" },
            if error {
                " failed; repeats limited to once per minute"
            } else {
                " completed"
            }
        ),
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostics_are_shared_redacted_and_rate_limited() {
        let id = uuid::Uuid::new_v4().to_string();
        notice(&id, "open", true);
        notice(&id, "open", true);
        notice(&id, "cancel", false);
        let logs: Vec<_> = crate::logs::native_logs(0)
            .into_iter()
            .filter(|e| e.text.contains(&id))
            .collect();
        assert_eq!(logs.len(), 2);
        assert!(logs[0].text.starts_with("ERROR: Studio NativeVideo open"));
        assert!(logs[1].text.starts_with("INFO: Studio NativeVideo cancel"));
    }
    #[test]
    fn rejects_disguised_playlists_and_accepts_video_signatures() {
        let file = tempfile::Builder::new().suffix(".webm").tempfile().unwrap();
        std::fs::write(file.path(), b"#EXTM3U\nhttps://secret.invalid/media").unwrap();
        assert!(check_signature(file.path()).is_err());
        std::fs::write(file.path(), b"\x1a\x45\xdf\xa3webm").unwrap();
        assert!(check_signature(file.path()).is_ok());
        std::fs::write(file.path(), b"fLaC\0\0\0\x22").unwrap();
        assert!(check_signature(file.path()).is_ok());
        assert!(local_path(file.path().to_str().unwrap()).is_ok());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn cancelling_a_pending_open_is_scoped_and_releases_its_slot() {
        let id = uuid::Uuid::new_v4().to_string();
        let pending = Pending::start(&id).unwrap();
        assert!(Pending::start(&id).is_err());
        Pending::cancel(&id);
        assert!(*pending.cancelled.borrow());
        drop(pending);
        assert!(Pending::start(&id).is_ok());
    }
    #[test]
    fn crop_action_is_typed_and_bounded() {
        let crop: Action =
            serde_json::from_value(serde_json::json!({"kind":"crop","start":1.0,"end":3.0}))
                .unwrap();
        assert!(crop.validate().is_ok());
        assert!(Action::Crop {
            start: 0.0,
            end: 86400.0
        }
        .validate()
        .is_ok());
        let bounds = serde_json::json!({"kind":"bounds","x":0,"y":0,"width":100,"height":100,"visible":true});
        assert!(matches!(
            serde_json::from_value::<Action>(bounds.clone()).unwrap(),
            Action::Bounds {
                audio_only: false,
                ..
            }
        ));
        let mut audio_bounds = bounds;
        audio_bounds["audioOnly"] = serde_json::json!(true);
        assert!(matches!(
            serde_json::from_value::<Action>(audio_bounds).unwrap(),
            Action::Bounds {
                audio_only: true,
                ..
            }
        ));
        for (start, end) in [
            (-1.0, 2.0),
            (2.0, 2.0),
            (3.0, 2.0),
            (0.0, 86401.0),
            (f64::NAN, 3.0),
            (0.0, f64::INFINITY),
        ] {
            assert!(Action::Crop { start, end }.validate().is_err());
        }
        assert!(serde_json::from_value::<Action>(
            serde_json::json!({"kind":"crop","start":1,"end":2,"command":"quit"})
        )
        .is_err());
    }
    #[test]
    fn playback_boundary_rejects_commands_urls_and_invalid_values() {
        assert!(serde_json::from_value::<Action>(
            serde_json::json!({"kind":"command","args":["run","sh"]})
        )
        .is_err());
        assert!(Action::Seek { seconds: f64::NAN }.validate().is_err());
        assert!(Action::Volume { volume: 101.0 }.validate().is_err());
        assert!(Action::Bounds {
            x: 0,
            y: 0,
            width: 20000,
            height: 10,
            visible: true,
            audio_only: false,
        }
        .validate()
        .is_err());
        assert!(local_path("https://example.org/a.mp4").is_err());
        assert!(local_path("/etc/passwd").is_err());
        assert!(output_url("file:///etc/", "x.mp4", "", "output").is_err());
        assert!(output_url("http://127.0.0.1:8188/", "x.mp4", "../secret", "output").is_err());
        assert!(output_url("http://127.0.0.1:8188/", "x.m3u", "", "output").is_err());
        let folder = r"raw\video\2026-10-26";
        let url = output_url("http://127.0.0.1:8188/", "a.webm", folder, "output").unwrap();
        assert_eq!(
            url.query_pairs().find(|(k, _)| k == "subfolder").unwrap().1,
            folder
        );
        for bad in [
            r"..\secret",
            r"raw/..\secret",
            r"\root",
            r"C:\root",
            r"\\host\share",
        ] {
            assert!(output_url("http://127.0.0.1:8188/", "a.webm", bad, "output").is_err());
        }
        let url = output_url("http://127.0.0.1:8188/", "a & ü.webm", "sub", "output").unwrap();
        assert_eq!(url.path(), "/view");
        assert_eq!(url.query_pairs().next().unwrap().1, "a & ü.webm");
    }
}
