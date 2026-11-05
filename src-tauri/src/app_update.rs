//! Explicit application-only update. No environment, backend or process lifecycle changes.
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
use reqwest::Url;
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
use serde::Deserialize;
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
use std::time::Duration;
use tauri::Manager;

#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
const API: &str = "https://api.github.com/repos/darksidewalker/DEAS/releases/latest";
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
const MAX_BINARY: u64 = 256 * 1024 * 1024;
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
const MAX_METADATA: u64 = 1024 * 1024;
static UPDATING: AtomicBool = AtomicBool::new(false);
static RESTART_READY: AtomicBool = AtomicBool::new(false);

#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
#[derive(Deserialize)]
struct Asset {
    id: u64,
    url: String,
    name: String,
    size: u64,
    digest: Option<String>,
    browser_download_url: String,
}

pub(crate) fn version(tag: &str) -> Option<(u64, u64, u64)> {
    let parts: Vec<_> = tag.strip_prefix('v').unwrap_or(tag).split('.').collect();
    if parts.len() != 3
        || parts.iter().any(|p| {
            p.is_empty()
                || (p.len() > 1 && p.starts_with('0'))
                || !p.bytes().all(|c| c.is_ascii_digit())
        })
    {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
fn select<'a>(release: &'a Release, current: &str) -> Result<Option<&'a Asset>, String> {
    let latest = version(&release.tag_name).ok_or("Invalid release version")?;
    if release.draft || release.prerelease {
        return Err("Expected a published stable release".into());
    }
    if latest <= version(current).ok_or("Invalid application version")? {
        return Ok(None);
    }
    // Only the published raw executable is supported: never unpack release archives/installers.
    let mut matching = release
        .assets
        .iter()
        .filter(|a| a.name == "deas-linux-amd64");
    let asset = matching
        .next()
        .ok_or("Latest release has no Linux x86_64 raw executable; install manually")?;
    if matching.next().is_some() || asset.size < 64 || asset.size > MAX_BINARY {
        return Err("Invalid or ambiguous release asset size/selection".into());
    }
    let expected = format!(
        "https://github.com/darksidewalker/DEAS/releases/download/{}/deas-linux-amd64",
        release.tag_name
    );
    if asset.browser_download_url != expected
        || asset.id == 0
        || asset.url
            != format!(
                "https://api.github.com/repos/darksidewalker/DEAS/releases/assets/{}",
                asset.id
            )
    {
        return Err("Untrusted release asset URL".into());
    }
    let hash = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .ok_or("Release asset has no GitHub SHA-256 digest; install manually")?;
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid release asset SHA-256 digest".into());
    }
    Ok(Some(asset))
}

fn log(level: &str, message: &str) {
    let text = format!("{level}: app-update: {message}");
    crate::logs::push(crate::logs::Source::Deas, &text);
    eprintln!("[DEAS] {text}");
}

#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
fn allowed_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.fragment().is_none()
        && matches!(
            url.host_str(),
            Some(
                "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
            )
        )
}

#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .no_proxy()
        .user_agent("DEAS-app-update")
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Cannot create update client".into())
}

#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
async fn response(
    client: &reqwest::Client,
    url: &str,
    limit: u64,
    mut authorization: Option<reqwest::header::HeaderValue>,
    binary: bool,
) -> Result<reqwest::Response, String> {
    let mut url = Url::parse(url).map_err(|_| "Invalid update URL")?;
    if authorization.is_some() && url.origin().ascii_serialization() != "https://api.github.com" {
        return Err("Update credentials require the exact GitHub API origin".into());
    }
    for redirects in 0..=5 {
        let mut request = client.get(url.clone()).header(
            reqwest::header::ACCEPT,
            if binary {
                "application/octet-stream"
            } else {
                "application/vnd.github+json"
            },
        );
        if let Some(value) = authorization.take() {
            request = request.header(reqwest::header::AUTHORIZATION, value);
        }
        let response = request
            .send()
            .await
            .map_err(|_| "Update download transport failed")?;
        if response.status().is_redirection() {
            if !binary || redirects == 5 {
                return Err("Unexpected or excessive update redirects".into());
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|h| h.to_str().ok())
                .ok_or("Missing update redirect destination")?;
            let destination = url
                .join(location)
                .map_err(|_| "Invalid update redirect destination")?;
            if !allowed_url(&destination) {
                return Err("Untrusted update redirect destination".into());
            }
            // Never forward API credentials to release hosts, even on same-origin redirects.
            url = destination;
            continue;
        }
        if response.status() != reqwest::StatusCode::OK {
            return Err(format!(
                "Update server answered {} (private releases require a saved GitHub token)",
                response.status()
            ));
        }
        if response.content_length().is_some_and(|size| size > limit) {
            return Err("Update response exceeds size limit".into());
        }
        return Ok(response);
    }
    Err("Excessive update redirects".into())
}

#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
async fn latest(
    client: &reqwest::Client,
    auth: &crate::auth::AuthState,
) -> Result<Release, String> {
    let mut response = response(
        client,
        API,
        MAX_METADATA,
        auth.authorization("github", &Url::parse(API).unwrap())?,
        false,
    )
    .await?;
    // The API must not redirect into the asset CDN or another API.
    if response.url().as_str() != API {
        return Err("Unexpected release metadata destination".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Release metadata transfer failed")?
    {
        if bytes.len() as u64 + chunk.len() as u64 > MAX_METADATA {
            return Err("Release metadata exceeds size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid release metadata".into())
}

#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
fn validate_binary(file: &mut std::fs::File, size: u64, asset: &Asset) -> Result<(), String> {
    use std::io::{Read, Seek, SeekFrom};
    if size != asset.size {
        return Err("Update binary size mismatch".into());
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| "Cannot read staged update")?;
    let mut digest = Sha256::new();
    let mut header = [0; 64];
    file.read_exact(&mut header)
        .map_err(|_| "Truncated update binary")?;
    digest.update(header);
    let mut buffer = [0; 64 * 1024];
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|_| "Cannot hash staged update")?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    if !asset
        .digest
        .as_deref()
        .is_some_and(|d| d.eq_ignore_ascii_case(&format!("sha256:{:x}", digest.finalize())))
    {
        return Err("Update binary SHA-256 mismatch".into());
    }
    // ELF64 little-endian x86_64 executable/shared PIE; archives and other architectures fail closed.
    if &header[..7] != b"\x7fELF\x02\x01\x01"
        || !matches!(u16::from_le_bytes([header[16], header[17]]), 2 | 3)
        || u16::from_le_bytes([header[18], header[19]]) != 62
    {
        return Err("Release asset is not a Linux x86_64 ELF executable".into());
    }
    Ok(())
}

#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
async fn download(
    client: &reqwest::Client,
    asset: &Asset,
    staged: &mut std::fs::File,
    auth: &crate::auth::AuthState,
) -> Result<(), String> {
    use std::io::Write;
    let mut response = response(
        client,
        &asset.url,
        asset.size,
        auth.authorization(
            "github",
            &Url::parse(&asset.url).map_err(|_| "Invalid asset API URL")?,
        )?,
        true,
    )
    .await?;
    let mut size = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Update binary transfer failed")?
    {
        size = size
            .checked_add(chunk.len() as u64)
            .ok_or("Update size overflow")?;
        if size > asset.size {
            return Err("Update binary exceeds declared size".into());
        }
        staged
            .write_all(&chunk)
            .map_err(|_| "Cannot write staged update")?;
    }
    validate_binary(staged, size, asset)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod linux {
    use super::*;
    use std::{
        fs::File,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::{
                ffi::OsStrExt,
                fs::{MetadataExt, PermissionsExt},
            },
        },
        path::Path,
    };

    struct Target {
        dir: crate::downloads::fs::Anchor,
        old: File,
        name: std::ffi::CString,
    }
    impl Target {
        fn open(path: &Path) -> Result<Self, String> {
            let dir = crate::downloads::fs::Anchor::open(
                path.parent().ok_or("Executable has no parent")?,
            )?;
            dir.writable()?;
            // Cross-instance serialization without creating/deleting a persistent lock file.
            if unsafe { libc::flock(dir.0.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err("Another application update is active".into());
            }
            let name = std::ffi::CString::new(
                path.file_name().ok_or("Executable has no name")?.as_bytes(),
            )
            .map_err(|_| "Invalid executable name")?;
            let fd = unsafe {
                libc::openat(
                    dir.0.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err("Cannot open installed executable safely".into());
            }
            let old = unsafe { File::from_raw_fd(fd) };
            let m = old
                .metadata()
                .map_err(|_| "Cannot inspect installed executable")?;
            if !m.is_file()
                || m.uid() != unsafe { libc::geteuid() }
                || m.mode() & 0o6022 != 0
                || m.nlink() != 1
            {
                return Err("Executable must be an owned regular non-shared file without special/write permissions".into());
            }
            Ok(Self { dir, old, name })
        }
        fn identity_matches(&self, file: &File) -> Result<(), String> {
            let old = self
                .old
                .metadata()
                .map_err(|_| "Cannot inspect old executable")?;
            let new = file
                .metadata()
                .map_err(|_| "Cannot inspect executable identity")?;
            if (old.dev(), old.ino()) != (new.dev(), new.ino()) {
                return Err("Executable changed during update; restart before retrying".into());
            }
            Ok(())
        }
        fn stage_dir(&self) -> std::path::PathBuf {
            format!("/proc/self/fd/{}", self.dir.0.as_raw_fd()).into()
        }
        fn publish(&self, staged: tempfile::NamedTempFile) -> Result<(), String> {
            staged
                .as_file()
                .sync_all()
                .map_err(|_| "Cannot sync staged update")?;
            let fd = unsafe {
                libc::openat(
                    self.dir.0.as_raw_fd(),
                    self.name.as_ptr(),
                    libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err("Installed executable disappeared during update".into());
            }
            self.identity_matches(&unsafe { File::from_raw_fd(fd) })?;
            staged
                .persist(
                    self.stage_dir()
                        .join(std::ffi::OsStr::from_bytes(self.name.as_bytes())),
                )
                .map_err(|_| "Atomic executable replacement failed; original retained")?;
            // Rename committed: a durability warning cannot truthfully report a failed replacement.
            if self.dir.0.sync_all().is_err() {
                log(
                    "WARN",
                    "Executable replaced but parent directory sync failed",
                );
            }
            Ok(())
        }
    }

    pub(super) async fn update(auth: &crate::auth::AuthState) -> Result<bool, String> {
        if std::env::var_os("APPIMAGE").is_some() || std::env::var_os("APPDIR").is_some() {
            return Err(
                "AppImage self-update is unsupported; replace the AppImage manually".into(),
            );
        }
        let path = std::env::current_exe().map_err(|_| "Cannot locate application executable")?;
        let target = Target::open(&path)?;
        target.identity_matches(
            &File::open("/proc/self/exe").map_err(|_| "Cannot identify running application")?,
        )?;
        log("INFO", "Checking latest stable GitHub release");
        let client = client()?;
        let release = latest(&client, auth).await?;
        let Some(asset) = select(&release, env!("CARGO_PKG_VERSION"))? else {
            return Ok(false);
        };
        let mut staged = tempfile::Builder::new()
            .prefix(".deas-update-")
            .tempfile_in(target.stage_dir())
            .map_err(|_| "Cannot stage update beside executable")?;
        log(
            "INFO",
            &format!(
                "Downloading verified raw executable for {}",
                release.tag_name
            ),
        );
        download(&client, asset, staged.as_file_mut(), auth).await?;
        staged
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o755))
            .map_err(|_| "Cannot set update executable permissions")?;
        target.publish(staged)?;
        Ok(true)
    }

    #[cfg(test)]
    mod tests {
        include!("app_update_linux_tests.rs");
    }
}

struct Active;
impl Drop for Active {
    fn drop(&mut self) {
        UPDATING.store(false, Ordering::Release);
    }
}
async fn run(confirmed: bool, _auth: &crate::auth::AuthState) -> Result<bool, String> {
    if !confirmed {
        return Err("Explicit application-update confirmation is required".into());
    }
    if UPDATING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("An application update is already active".into());
    }
    let _active = Active;
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        tokio::time::timeout(Duration::from_secs(300), linux::update(_auth))
            .await
            .map_err(|_| "Application update timed out; original retained".to_string())?
    }
    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        unsupported(crate::repo::newer_release(_auth).await?)
    }
}

/// No replacement here, but an up-to-date build must not block ComfyUI/node-pack updates.
#[cfg(any(test, not(all(target_os = "linux", target_arch = "x86_64"))))]
fn unsupported(newer: Option<String>) -> Result<bool, String> {
    match newer {
        None => Ok(false),
        Some(tag) => Err(format!(
            "DEAS {tag} is available, but application self-update is unsupported on this platform (including Windows locked executables); download the latest release and replace it after quitting DEAS"
        )),
    }
}

/// `true`: replaced, restart authorized after pending work completes. `false`: no newer release.
#[tauri::command]
pub async fn native_app_update(
    app: tauri::AppHandle,
    confirmed: bool,
    auth: tauri::State<'_, crate::auth::AuthState>,
) -> Result<bool, String> {
    // Cache the original executable path before Linux replacement unlinks its inode.
    tauri::process::current_binary(&app.env()).map_err(|_| "Cannot locate restart executable")?;
    execute(confirmed, &auth).await
}

async fn execute(confirmed: bool, auth: &crate::auth::AuthState) -> Result<bool, String> {
    let result = run(confirmed, auth).await;
    match &result {
        Ok(true) => {
            RESTART_READY.store(true, Ordering::Release);
            log(
                "INFO",
                "Executable replaced; automatic restart pending completed work",
            );
        }
        Ok(false) => log("INFO", "No newer stable application release"),
        Err(error) => log("ERROR", error),
    }
    result
}

/// Only a successful, confirmed replacement authorizes restarting this process.
#[tauri::command]
pub fn native_app_restart(
    app: tauri::AppHandle,
    engine: tauri::State<'_, crate::install::Engine>,
    downloads: tauri::State<'_, crate::downloads::DownloadEngine>,
    scans: tauri::State<'_, crate::ModelScanEngine>,
    jobs: tauri::State<'_, crate::jobs::Jobs>,
    operations: tauri::State<'_, crate::FolderOperations>,
) -> Result<bool, String> {
    if !RESTART_READY.load(Ordering::Acquire) {
        return Err("No confirmed application update is awaiting restart".into());
    }
    let _guard = match operations.enter() {
        Ok(guard) => guard,
        Err(_) => return Ok(false),
    };
    if UPDATING.load(Ordering::Acquire)
        || crate::install::native_install_status(engine.clone()).state == "running"
        || downloads.status()?.state == "running"
        || scans.status().state == "running"
        || jobs.media_move_idle().is_err()
    {
        return Ok(false);
    }
    if RESTART_READY.swap(false, Ordering::AcqRel) {
        log("INFO", "Restarting DEAS to load the updated executable");
        // Tauri can restart directly if posting its exit event fails.
        engine.shutdown_backend();
        app.request_restart();
    }
    Ok(true)
}

#[cfg(test)]
#[path = "app_update_tests.rs"]
mod tests;
