//! Media browsing, cached thumbnails, and explicit native-authorized starring.
//! Browsing initializes only missing raw/starred folders under the native-approved Output root.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

#[path = "media_star.rs"]
pub(crate) mod star;
pub use star::{
    native_media_star, native_media_star_notice, native_media_star_status, native_media_unstar,
};

const MAX_ENTRIES: usize = 5000;
/// Longest thumbnail edge: twice the 5.6rem tile so HiDPI tiles stay sharp.
const THUMB_EDGE: u32 = 192;
const THUMB_DECODE_LIMIT: usize = 256 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaEntry {
    pub name: String,
    pub path: String,
    pub kind: &'static str,
    pub symlink: bool,
    pub media: Option<&'static str>,
    pub size: u64,
    /// Seconds since the Unix epoch; 0 when the filesystem reports none.
    pub modified: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaListing {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<MediaEntry>,
    pub truncated: bool,
}

pub fn media_kind(name: &str) -> Option<&'static str> {
    let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "avif" | "svg" | "tif" | "tiff" => {
            Some("image")
        }
        "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v" | "mpg" | "mpeg" => Some("video"),
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "aac" | "opus" => Some("audio"),
        _ => None,
    }
}

/// Follows symlinks for the kind so linked folders browse like real ones; the flag stays visible.
fn describe(path: &Path) -> Result<MediaEntry, String> {
    let shown = path.to_str().ok_or("Path is not valid UTF-8")?.to_owned();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| shown.clone());
    let symlink = std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    let meta = std::fs::metadata(path).map_err(|e| format!("{shown}: {e}"))?;
    let dir = meta.is_dir();
    Ok(MediaEntry {
        media: if dir { None } else { media_kind(&name) },
        kind: if dir { "directory" } else { "file" },
        size: if dir { 0 } else { meta.len() },
        modified: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs()),
        name,
        path: shown,
        symlink,
    })
}

fn absolute(path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err("Path must be absolute".into());
    }
    Ok(path)
}

/// `depth` 1 lists only this folder; deeper levels add files from subfolders (name = relative path),
/// never descending into symlinked folders (loops) or hidden ones. Folders are listed at the top level only.
pub fn list_directory(path: &Path, depth: u32) -> Result<MediaListing, String> {
    let shown = path.to_str().ok_or("Path is not valid UTF-8")?.to_owned();
    let mut entries = vec![];
    let mut truncated = false;
    let mut pending = vec![(path.to_path_buf(), 1u32)];
    while let Some((dir, level)) = pending.pop() {
        let read = if level == 1 {
            std::fs::read_dir(&dir).map_err(|e| format!("{shown}: {e}"))?
        } else {
            match std::fs::read_dir(&dir) {
                Ok(read) => read,
                Err(_) => continue, // unreadable subfolders are skipped, not fatal
            }
        };
        for entry in read.flatten() {
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue; // shortcut: hidden entries are skipped, add a toggle when someone needs them
            }
            if entries.len() >= MAX_ENTRIES {
                truncated = true;
                break;
            }
            // Unreadable entries (dangling links, permission errors) are omitted, not fatal.
            let Ok(mut described) = describe(&entry.path()) else {
                continue;
            };
            if described.kind == "directory" {
                if level < depth.max(1) && !described.symlink {
                    pending.push((entry.path(), level + 1));
                }
                if level > 1 {
                    continue;
                }
            } else if level > 1 {
                described.name = entry
                    .path()
                    .strip_prefix(path)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or(described.name);
            }
            entries.push(described);
        }
    }
    entries.sort_by(|a, b| {
        (a.kind != "directory")
            .cmp(&(b.kind != "directory"))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(MediaListing {
        parent: path.parent().and_then(|p| p.to_str()).map(str::to_owned),
        path: shown,
        entries,
        truncated,
    })
}

/// Downscaled PNG copy of an image, cached by path + size + mtime so each original is decoded once.
/// Returns the original path when it already fits the tile.
// shortcut: PNG only (ComfyUI output), other formats fall back to the full file in the UI; add the `image` crate when JPEG/WebP folders matter
pub fn thumbnail(path: &Path, cache: &Path) -> Result<PathBuf, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let is_png = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("png"));
    if !meta.is_file() || !is_png {
        return Err("No thumbnail for this file".into());
    }
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    let key = Sha256::digest(format!("{}\0{}\0{modified}", path.display(), meta.len()));
    let target = cache.join(format!("{key:x}.png"));
    if target.is_file() {
        return Ok(target);
    }
    let mut options = png::DecodeOptions::default();
    options.set_ignore_text_chunk(true);
    options.set_ignore_iccp_chunk(true);
    let file = BufReader::new(std::fs::File::open(path).map_err(|e| e.to_string())?);
    let mut decoder = png::Decoder::new_with_options(file, options);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    decoder.set_limits(png::Limits {
        bytes: THUMB_DECODE_LIMIT,
    });
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let (w, h) = (reader.info().width, reader.info().height);
    if w.max(h) <= THUMB_EDGE {
        return Ok(path.to_path_buf());
    }
    let mut frame = vec![0u8; reader.output_buffer_size()];
    let out = reader.next_frame(&mut frame).map_err(|e| e.to_string())?;
    let samples = out.color_type.samples();
    let (tw, th) = if w >= h {
        (THUMB_EDGE, (h * THUMB_EDGE / w).max(1))
    } else {
        ((w * THUMB_EDGE / h).max(1), THUMB_EDGE)
    };
    // Box filter: every output pixel averages its source rectangle, so thumbnails do not alias.
    let mut small = Vec::with_capacity((tw * th) as usize * samples);
    let (w, h) = (w as usize, h as usize);
    for oy in 0..th as usize {
        let (y0, y1) = (
            oy * h / th as usize,
            ((oy + 1) * h / th as usize).max(oy * h / th as usize + 1),
        );
        for ox in 0..tw as usize {
            let (x0, x1) = (
                ox * w / tw as usize,
                ((ox + 1) * w / tw as usize).max(ox * w / tw as usize + 1),
            );
            let count = ((y1 - y0) * (x1 - x0)) as u64;
            for c in 0..samples {
                let sum: u64 = (y0..y1)
                    .flat_map(|y| (x0..x1).map(move |x| (y * w + x) * samples + c))
                    .map(|i| frame[i] as u64)
                    .sum();
                small.push((sum / count) as u8);
            }
        }
    }
    std::fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let tmp = cache.join(format!("{key:x}.{}.tmp", std::process::id()));
    {
        let writer = BufWriter::new(std::fs::File::create(&tmp).map_err(|e| e.to_string())?);
        let mut encoder = png::Encoder::new(writer, tw, th);
        encoder.set_color(out.color_type);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .and_then(|mut w| w.write_image_data(&small))
            .map_err(|e| e.to_string())?;
    }
    std::fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
    Ok(target)
}

/// Empty path opens the user's home directory; `depth` defaults to 1 (this folder only).
/// `async` keeps the directory walk off the main thread so the UI never stalls on big folders.
#[tauri::command(async)]
pub fn native_media_list(
    path: String,
    depth: Option<u32>,
    engine: tauri::State<'_, crate::install::Engine>,
) -> Result<MediaListing, String> {
    let output = star::output_root(&engine).ok();
    list_media(&path, depth, output.as_deref())
}

fn list_media(
    path: &str,
    depth: Option<u32>,
    output: Option<&Path>,
) -> Result<MediaListing, String> {
    let target = if path.is_empty() {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .ok_or("No home directory is known")?
    } else {
        absolute(path)?
    };
    if let Some(root) = output {
        if [root.join("raw"), root.join("starred")].contains(&target)
            && matches!(std::fs::symlink_metadata(&target), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
        {
            let initialized = (|| {
                let parent = crate::downloads::fs::Anchor::open(root)?;
                parent.writable()?;
                let name = target
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or("Invalid output folder")?;
                let child = match parent.create(name) {
                    Ok(child) => child,
                    Err(_) => crate::downloads::fs::Anchor::open(&target)?,
                };
                child.writable()?;
                if parent.identity()? != crate::downloads::fs::Anchor::open(root)?.identity()?
                    || child.identity()?
                        != crate::downloads::fs::Anchor::open(&target)?.identity()?
                {
                    return Err("Output folder namespace changed".into());
                }
                Ok::<(), String>(())
            })();
            if initialized.is_err() {
                crate::logs::push(
                    crate::logs::Source::Deas,
                    "ERROR: Media operation=initialize-output-folder failed",
                );
            }
            initialized?;
        }
    }
    list_directory(&target, depth.unwrap_or(1).min(8))
}

/// Describe dropped absolute paths; folders come back as directories the UI can open.
#[tauri::command(async)]
pub fn native_media_describe(paths: Vec<String>) -> Result<Vec<MediaEntry>, String> {
    paths
        .iter()
        .take(MAX_ENTRIES)
        .map(|p| describe(&absolute(p)?))
        .collect()
}

/// Thumbnail path for one image (decoded on the blocking pool); the UI falls back to the original on error.
/// Opens the folder holding a browsed file in the system file manager (Windows and macOS also select
/// the file). The path is passed as one argument, never through a shell, and must exist.
#[tauri::command]
pub fn native_media_reveal(path: String) -> Result<(), String> {
    let file = dunce::canonicalize(absolute(&path)?).map_err(|e| format!("{path}: {e}"))?;
    #[cfg(target_os = "linux")]
    let mut cmd = {
        let dir = if file.is_dir() {
            file.as_path()
        } else {
            file.parent().ok_or("No parent folder")?
        };
        let mut c = std::process::Command::new("xdg-open");
        c.arg(dir);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("explorer");
        c.arg(format!("/select,{}", file.display()));
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg("-R").arg(&file);
        c
    };
    cmd.spawn().map(|_| ()).map_err(|e| {
        let text = format!("Could not open the folder: {e}");
        crate::logs::push(
            crate::logs::Source::Deas,
            &format!("[error] media reveal: {text}"),
        );
        text
    })
}

#[tauri::command]
pub async fn native_media_thumbnail(app: tauri::AppHandle, path: String) -> Result<String, String> {
    use tauri::Manager;
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("thumbnails");
    let path = absolute(&path)?;
    let thumb = tauri::async_runtime::spawn_blocking(move || thumbnail(&path, &cache))
        .await
        .map_err(|e| e.to_string())??;
    thumb
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "Path is not valid UTF-8".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_media_by_extension_case_insensitively() {
        assert_eq!(media_kind("a.PNG"), Some("image"));
        assert_eq!(media_kind("clip.mkv"), Some("video"));
        assert_eq!(media_kind("song.flac"), Some("audio"));
        assert_eq!(media_kind("notes.txt"), None);
        assert_eq!(media_kind("noext"), None);
    }

    #[cfg(unix)]
    #[test]
    fn lists_sorted_entries_and_browses_symlinked_folders() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("b.mp4"), b"v").unwrap();
        std::fs::write(real.join("A.png"), b"i").unwrap();
        std::fs::write(real.join(".hidden"), b"").unwrap();
        std::os::unix::fs::symlink(&real, root.path().join("linked")).unwrap();
        std::os::unix::fs::symlink(root.path().join("missing"), root.path().join("dangling"))
            .unwrap();

        let top = list_directory(root.path(), 1).unwrap();
        assert_eq!(
            top.entries
                .iter()
                .map(|e| (&*e.name, e.kind, e.symlink))
                .collect::<Vec<_>>(),
            vec![("linked", "directory", true), ("real", "directory", false)]
        );
        assert_eq!(
            top.parent.as_deref(),
            root.path().parent().unwrap().to_str()
        );

        let linked = list_directory(&root.path().join("linked"), 1).unwrap();
        assert_eq!(
            linked
                .entries
                .iter()
                .map(|e| (&*e.name, e.media))
                .collect::<Vec<_>>(),
            vec![("A.png", Some("image")), ("b.mp4", Some("video"))]
        );
        assert_eq!(linked.entries[0].size, 1);

        // Deep listing: files from real subfolders with relative names, folders only at the top, no symlink descent.
        std::fs::create_dir_all(real.join("sub/deeper")).unwrap();
        std::fs::write(real.join("sub/c.png"), b"i").unwrap();
        std::fs::write(real.join("sub/deeper/d.png"), b"i").unwrap();
        let deep = list_directory(root.path(), 2).unwrap();
        assert_eq!(
            deep.entries.iter().map(|e| &*e.name).collect::<Vec<_>>(),
            vec!["linked", "real", "real/A.png", "real/b.mp4"]
        );
        let deeper = list_directory(&real, 3).unwrap();
        assert_eq!(
            deeper.entries.iter().map(|e| &*e.name).collect::<Vec<_>>(),
            vec!["sub", "A.png", "b.mp4", "sub/c.png", "sub/deeper/d.png"]
        );
        assert!(list_media("relative", None, None).is_err());
        assert!(list_media(root.path().join("nope").to_str().unwrap(), None, None).is_err());
        let dropped = native_media_describe(vec![real.to_str().unwrap().into()]).unwrap();
        assert_eq!(dropped[0].kind, "directory");
        assert!(native_media_describe(vec!["relative".into()]).is_err());
        // Reveal refuses relative and missing paths before anything is spawned.
        assert!(native_media_reveal("relative".into()).is_err());
        assert!(native_media_reveal(root.path().join("nope").to_str().unwrap().into()).is_err());
    }

    #[test]
    fn browsing_creates_only_fixed_owned_output_folders() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        for name in ["raw", "starred"] {
            let target = root.join(name);
            let listing = list_media(target.to_str().unwrap(), None, Some(&root)).unwrap();
            assert!(target.is_dir());
            assert!(listing.entries.is_empty());
            assert_eq!(listing.path, target.to_str().unwrap());
            assert!(list_media(target.to_str().unwrap(), None, Some(&root)).is_ok());
        }
        for name in ["missing", "raw/missing", "../foreign/raw"] {
            let target = root.join(name);
            assert!(list_media(target.to_str().unwrap(), None, Some(&root)).is_err());
            assert!(!target.exists());
        }
        let unmanaged = tempfile::tempdir().unwrap();
        let missing = unmanaged.path().join("raw");
        assert!(list_media(missing.to_str().unwrap(), None, None).is_err());
        assert!(!missing.exists());
    }
    #[cfg(unix)]
    #[test]
    fn output_folder_initialization_rejects_symlinked_root_and_preserves_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let linked = root.join("linked");
        std::os::unix::fs::symlink(outside.path(), &linked).unwrap();
        assert!(list_media(linked.join("raw").to_str().unwrap(), None, Some(&linked)).is_err());
        assert!(!outside.path().join("raw").exists());
        std::fs::write(root.join("raw"), b"existing file").unwrap();
        assert!(list_media(root.join("raw").to_str().unwrap(), None, Some(&root)).is_err());
        assert_eq!(std::fs::read(root.join("raw")).unwrap(), b"existing file");
        let entries = crate::logs::native_logs(0);
        assert!(entries
            .iter()
            .any(|e| e.text == "ERROR: Media operation=initialize-output-folder failed"));
    }
    #[test]
    fn thumbnails_are_cached_downscaled_pngs() {
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("cache");
        let write = |name: &str, w: u32, h: u32| {
            let p = root.path().join(name);
            let mut enc = png::Encoder::new(std::fs::File::create(&p).unwrap(), w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            // Left half black, right half white: the box filter must keep both sides.
            let px: Vec<u8> = (0..w * h)
                .flat_map(|i| {
                    if i % w < w / 2 {
                        [0, 0, 0, 255]
                    } else {
                        [255, 255, 255, 255]
                    }
                })
                .collect();
            enc.write_header().unwrap().write_image_data(&px).unwrap();
            p
        };
        let big = write("big.png", 768, 384);
        let thumb = thumbnail(&big, &cache).unwrap();
        assert!(thumb.starts_with(&cache));
        let decoder = png::Decoder::new(std::fs::File::open(&thumb).unwrap());
        let mut reader = decoder.read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (192, 96));
        let mut buf = vec![0; reader.output_buffer_size()];
        reader.next_frame(&mut buf).unwrap();
        assert_eq!(&buf[..4], &[0, 0, 0, 255]);
        assert_eq!(&buf[(96 * 4) + 4..(96 * 4) + 8], &[255, 255, 255, 255]);
        let written = std::fs::metadata(&thumb).unwrap().modified().unwrap();
        assert_eq!(thumbnail(&big, &cache).unwrap(), thumb); // cache hit, no re-encode
        assert_eq!(
            std::fs::metadata(&thumb).unwrap().modified().unwrap(),
            written
        );
        let small = write("small.png", 100, 50);
        assert_eq!(thumbnail(&small, &cache).unwrap(), small); // fits the tile already
        std::fs::write(root.path().join("photo.jpg"), b"x").unwrap();
        assert!(thumbnail(&root.path().join("photo.jpg"), &cache).is_err());
    }

    /// `DEAS_LIVE_PNG=/path/to/real.png cargo test --lib live_thumbnail -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_thumbnail() {
        let src = PathBuf::from(std::env::var("DEAS_LIVE_PNG").expect("DEAS_LIVE_PNG"));
        let cache = tempfile::tempdir().unwrap();
        let start = std::time::Instant::now();
        let thumb = thumbnail(&src, cache.path()).unwrap();
        println!(
            "{} -> {} in {:?}",
            src.display(),
            thumb.display(),
            start.elapsed()
        );
        assert!(thumb.is_file());
    }
}
