//! Read-only, bounded data-only theme packs under the selected DEAS environment.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{path::BaseDirectory, AppHandle, Manager};

const BASES: &[&str] = &[
    "default",
    "sage",
    "liiti-pink",
    "voud",
    "alluring-abyss",
    "slimy-teal",
    "clawy-grey",
    "android-white",
    "dragons-hoard",
];
const COLORS: &str = "deep field graphite panel surface raised line line-strong muted text-2 chalk blue blue-hi blue-light blue-2 blue-deep on-accent copper copper-hi copper-text ok ok-bg ok-mid ok-hi ok-soft ok-deep on-ok warn danger danger-bg danger-2 danger-text gold ink shadow warn-bg warn-text copper-bg";
const MANIFEST_LIMIT: u64 = 65_536;
const ASSET_LIMIT: u64 = 2 * 1024 * 1024;
const CATALOG_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    id: String,
    name: String,
    author: String,
    base: String,
    #[serde(default)]
    colors: BTreeMap<String, String>,
    #[serde(default)]
    assets: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemePack {
    id: String,
    name: String,
    author: String,
    base: String,
    colors: BTreeMap<String, String>,
    assets: BTreeMap<String, String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeCatalog {
    directory: String,
    packs: Vec<ThemePack>,
    warnings: Vec<String>,
}

fn slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn label(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 120 && !value.chars().any(char::is_control)
}
fn color(value: &str) -> bool {
    [4, 7, 9].contains(&value.len())
        && value.starts_with('#')
        && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}
fn linked(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}
fn plain(path: &Path, directory: bool) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|_| "Theme file or directory is unavailable")?;
    if linked(&meta) || (directory && !meta.is_dir()) || (!directory && !meta.is_file()) {
        return Err("Theme links and special files are not supported".into());
    }
    Ok(())
}
fn bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    plain(path, false)?;
    let parent = path.parent().ok_or("Theme file parent is unavailable")?;
    let anchor = crate::downloads::fs::Anchor::open(parent)
        .map_err(|_| "Theme parent components must be real directories")?;
    #[cfg(target_os = "linux")]
    let file = {
        use std::{
            ffi::CString,
            os::{
                fd::{AsRawFd, FromRawFd},
                unix::ffi::OsStrExt,
            },
        };
        let name = CString::new(
            path.file_name()
                .ok_or("Theme filename is missing")?
                .as_bytes(),
        )
        .map_err(|_| "Invalid theme filename")?;
        let fd = unsafe {
            libc::openat(
                anchor.0.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            )
        };
        if fd < 0 {
            return Err("Cannot open theme file safely".into());
        }
        unsafe { fs::File::from_raw_fd(fd) }
    };
    #[cfg(windows)]
    let file = crate::win::open_no_follow(path).map_err(|_| "Cannot open theme file safely")?;
    #[cfg(not(any(target_os = "linux", windows)))]
    let file = fs::File::open(path).map_err(|_| "Cannot open theme file")?;
    let meta = file.metadata().map_err(|_| "Cannot inspect theme file")?;
    if !meta.is_file() || linked(&meta) {
        return Err("Theme links and special files are not supported".into());
    }
    if meta.len() > limit {
        return Err("Theme file exceeds its size limit".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read theme file")?;
    if bytes.len() as u64 > limit {
        return Err("Theme file exceeds its size limit".into());
    }
    let again =
        crate::downloads::fs::Anchor::open(parent).map_err(|_| "Theme file parent was replaced")?;
    if again.identity()? != anchor.identity()? {
        return Err("Theme file parent was replaced".into());
    }
    Ok(bytes)
}
fn environment(root: &str) -> Result<PathBuf, String> {
    let path = Path::new(root);
    if !path.is_absolute() {
        return Err("Choose an installed DEAS environment first".into());
    }
    let real = dunce::canonicalize(path).map_err(|_| "Environment directory is unavailable")?;
    plain(&real, true)?;
    plain(&real.join("deas-install.json"), false)?;
    Ok(real)
}
// Support the original flat packs and the documented assets/ structure, without
// permitting arbitrary relative paths or links at any path component.
fn asset_path(folder: &Path, file: &str) -> Result<PathBuf, String> {
    let parts: Vec<_> = file.split('/').collect();
    let name = match parts.as_slice() {
        [name] => *name,
        ["assets", name] => {
            plain(&folder.join("assets"), true)?;
            *name
        }
        _ => return Err("Theme images must be inside assets/ or the theme folder".into()),
    };
    let (stem, extension) = name
        .rsplit_once('.')
        .ok_or("Theme image extension is missing")?;
    if file.len() > 120 || !slug(stem) || !["png", "webp"].contains(&extension) {
        return Err("Theme images must use local PNG or WebP filenames".into());
    }
    let path = folder.join(file);
    plain(&path, false)?;
    let real = fs::canonicalize(folder).map_err(|_| "Cannot resolve theme directory")?;
    if !fs::canonicalize(&path)
        .map_err(|_| "Cannot resolve theme image")?
        .starts_with(&real)
    {
        return Err("Theme image leaves its pack".into());
    }
    Ok(path)
}

fn image_mime(bytes: &[u8], webp: bool) -> Result<&'static str, String> {
    const PIXEL_LIMIT: usize = 16 * 1024 * 1024;
    if webp {
        let mut reader = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes))
            .map_err(|_| "Invalid WebP theme image")?;
        reader.set_memory_limit(PIXEL_LIMIT);
        let (width, height) = reader.dimensions();
        let size = reader
            .output_buffer_size()
            .ok_or("Theme image dimensions exceed limits")?;
        if width > 4096 || height > 4096 || size > PIXEL_LIMIT || reader.is_animated() {
            return Err("Theme image dimensions exceed limits or WebP is animated".into());
        }
        // VP8X reports canvas dimensions; the codec allocates the coded VP8 frame before comparing them.
        if bytes.get(12..16) == Some(b"VP8X") {
            let mut offset = 12usize;
            while let Some(header) = bytes.get(offset..offset.saturating_add(8)) {
                let length = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
                let end = offset
                    .checked_add(8)
                    .and_then(|n| n.checked_add(length))
                    .filter(|end| *end <= bytes.len())
                    .ok_or("Invalid WebP theme image")?;
                if &header[..4] == b"VP8 " || &header[..4] == b"VP8L" {
                    let chunk = &bytes[offset..end];
                    let mut standalone = b"RIFF".to_vec();
                    standalone.extend_from_slice(&(chunk.len() as u32 + 4).to_le_bytes());
                    standalone.extend_from_slice(b"WEBP");
                    standalone.extend_from_slice(chunk);
                    let coded = image_webp::WebPDecoder::new(std::io::Cursor::new(standalone))
                        .map_err(|_| "Invalid WebP theme image")?;
                    if coded.dimensions() != (width, height) {
                        return Err("Theme image coded dimensions differ from its canvas".into());
                    }
                    break;
                }
                offset = end
                    .checked_add(length % 2)
                    .ok_or("Invalid WebP theme image")?;
            }
        }
        reader
            .read_image(&mut vec![0; size])
            .map_err(|_| "Invalid WebP theme image")?;
        Ok("image/webp")
    } else {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_limits(png::Limits { bytes: PIXEL_LIMIT });
        let mut reader = decoder.read_info().map_err(|_| "Invalid PNG theme image")?;
        if reader.info().animation_control.is_some() {
            return Err("Animated PNG theme images are not supported".into());
        }
        if reader.info().width > 4096
            || reader.info().height > 4096
            || reader.output_buffer_size() > PIXEL_LIMIT
        {
            return Err("Theme image dimensions exceed limits".into());
        }
        let mut pixels = vec![0; reader.output_buffer_size()];
        reader
            .next_frame(&mut pixels)
            .map_err(|_| "Invalid PNG theme image")?;
        Ok("image/png")
    }
}
fn read_pack(folder: &Path, used: &mut usize) -> Result<ThemePack, String> {
    plain(folder, true)?;
    let bytes = bounded(&folder.join("theme.json"), MANIFEST_LIMIT)?;
    let m: Manifest =
        serde_json::from_slice(crate::bom_free(&bytes)).map_err(|_| "Invalid theme.json schema")?;
    if m.schema_version != 1
        || !slug(&m.id)
        || !label(&m.name)
        || !label(&m.author)
        || !BASES.contains(&m.base.as_str())
    {
        return Err("Unsupported theme version, identity or base palette".into());
    }
    if m.colors.len() > 64
        || m.colors
            .iter()
            .any(|(key, value)| !COLORS.split_whitespace().any(|k| k == key) || !color(value))
    {
        return Err("Theme colors must use supported tokens and hex values".into());
    }
    if m.assets.len() > 16 {
        return Err("Too many theme assets".into());
    }
    let mut assets = BTreeMap::new();
    let mut pack_bytes = 0;
    for (role, file) in &m.assets {
        if !slug(role) {
            return Err("Unsupported theme image role".into());
        }
        let path = asset_path(folder, file)?;
        let bytes = bounded(&path, ASSET_LIMIT)?;
        pack_bytes += bytes.len();
        if *used + pack_bytes > CATALOG_LIMIT {
            return Err("Theme catalog exceeds its image budget".into());
        }
        let mime = image_mime(&bytes, file.ends_with(".webp"))?;
        assets.insert(
            role.clone(),
            format!("data:{mime};base64,{}", STANDARD.encode(bytes)),
        );
    }
    *used += pack_bytes;
    Ok(ThemePack {
        id: m.id,
        name: m.name,
        author: m.author,
        base: m.base,
        colors: m.colors,
        assets,
    })
}
fn scan(root: &str) -> Result<ThemeCatalog, String> {
    let directory = environment(root)?.join("themes");
    let mut result = ThemeCatalog {
        directory: directory.to_string_lossy().into_owned(),
        packs: Vec::new(),
        warnings: Vec::new(),
    };
    if !directory
        .try_exists()
        .map_err(|_| "Cannot inspect themes folder")?
    {
        return Ok(result);
    }
    plain(&directory, true)?;
    let mut folders = fs::read_dir(&directory)
        .map_err(|_| "Cannot list themes folder")?
        .take(129)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Cannot read themes folder")?;
    if folders.len() > 128 {
        return Err("Themes folder contains more than 128 entries".into());
    }
    folders.sort_by_key(|entry| entry.file_name());
    let mut used = 0;
    let mut duplicates = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for entry in folders {
        let path = entry.path();
        if path.file_name().is_some_and(|name| name == "README.md") {
            continue;
        }
        if !path.is_dir() || !path.join("theme.json").try_exists().unwrap_or(false) {
            continue;
        }
        match read_pack(&path, &mut used) {
            Ok(pack) => {
                if !ids.insert(pack.id.clone()) {
                    duplicates.insert(pack.id.clone());
                }
                result.packs.push(pack);
            }
            Err(error) => result.warnings.push(error),
        }
    }
    result.packs.retain(|p| !duplicates.contains(&p.id));
    if !duplicates.is_empty() {
        result
            .warnings
            .push("Duplicate theme IDs were ignored".into());
    }
    if let Some(error) = result.warnings.first() {
        diagnostic(error);
    }
    Ok(result)
}
fn diagnostic(error: &str) {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last.is_some_and(|at| at.elapsed() < Duration::from_secs(60)) {
        return;
    }
    *last = Some(Instant::now());
    crate::logs::push(crate::logs::Source::Deas, &format!("ERROR: Theme packs scan/open: {error}; Default remains available for recovery (repeats limited to once per minute)"));
}
#[tauri::command]
pub async fn native_theme_packs(root: String) -> Result<ThemeCatalog, String> {
    let result = tauri::async_runtime::spawn_blocking(move || scan(&root))
        .await
        .map_err(|_| "Theme scan could not complete".to_owned())
        .and_then(|result| result);
    if let Err(error) = &result {
        diagnostic(error);
    }
    result
}
fn theme_editor_source(app: &AppHandle) -> Result<PathBuf, String> {
    let source = app
        .path()
        .resolve("theme-packs", BaseDirectory::Resource)
        .map_err(|_| "Theme editor is unavailable".to_string())?;
    if source.join("EDIT-THEMES.html").is_file() {
        return Ok(source);
    }
    let fallback = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Invalid application source path")?
        .join("theme-packs");
    if fallback.join("EDIT-THEMES.html").is_file() {
        return Ok(fallback);
    }
    Err("Theme editor is unavailable".into())
}

fn copy_folder(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|_| "Cannot create theme folder")?;
    for entry in fs::read_dir(source).map_err(|_| "Cannot read bundled theme folder")? {
        let entry = entry.map_err(|_| "Cannot read bundled theme folder")?;
        let name = entry.file_name();
        let destination = target.join(name);
        if entry.path().is_dir() {
            copy_folder(&entry.path(), &destination)?;
        } else {
            fs::copy(entry.path(), destination).map_err(|_| "Cannot copy bundled theme")?;
        }
    }
    Ok(())
}

fn ensure_starter_pack(themes: &Path, source: &Path) -> Result<(), String> {
    let starter = source.join("new-theme");
    if !starter.is_dir() {
        return Err("Bundled starter theme is unavailable".into());
    }
    let target = themes.join("new-theme");
    if target.symlink_metadata().is_ok() {
        return Ok(());
    }
    copy_folder(&starter, &target)
}

fn open_local_file(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        command
    };
    #[cfg(windows)]
    let mut command = {
        let mut command = std::process::Command::new("cmd");
        let path = path.to_string_lossy();
        command.args(["/C", "start", "", path.as_str()]);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(path);
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|_| "Cannot open the theme editor".to_string())
}

#[tauri::command]
pub async fn native_theme_editor(app: AppHandle, root: String) -> Result<(), String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        let directory = environment(&root)?.join("themes");
        if !directory
            .try_exists()
            .map_err(|_| "Cannot inspect themes folder")?
        {
            fs::create_dir(&directory).map_err(|_| "Cannot create themes folder")?;
        }
        plain(&directory, true)?;
        let source = theme_editor_source(&app)?;
        ensure_starter_pack(&directory, &source)?;
        open_local_file(&source.join("EDIT-THEMES.html"))
    })
    .await
    .map_err(|_| "Theme editor operation could not complete".to_owned())
    .and_then(|result| result);
    if let Err(error) = &result {
        diagnostic(error);
    }
    result
}

#[cfg(test)]
#[path = "themes_tests.rs"]
mod tests;
