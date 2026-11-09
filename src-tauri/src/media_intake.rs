//! Explicit copy-in of pasted pictures and dropped media into the owned input root.
use crate::downloads::fs::Anchor;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const IMAGE_LIMIT: u64 = 64 << 20;
const VIDEO_LIMIT: u64 = 512 << 20;

pub(crate) fn input_root(engine: &crate::install::Engine) -> Result<PathBuf, String> {
    let plan = engine
        .approved_plan()
        .ok_or("Select an owned installed environment to copy media into Input")?;
    let environment = Path::new(&plan.request.storage_path);
    let folders = crate::settings::read(environment)?;
    Ok(folders
        .input
        .map(PathBuf::from)
        .unwrap_or_else(|| environment.join("ComfyUI/input")))
}

/// Readable stem: letters, digits, `-`, `_` and spaces only, at most 80 chars.
fn clean_stem(stem: &str) -> String {
    let cleaned: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    let trimmed = cleaned.trim().trim_matches('_');
    if trimmed.is_empty() {
        "media".into()
    } else {
        trimmed.into()
    }
}

/// Writes `bytes` (already sniffed) as `<stem>.<ext>` into `root`, adding `-2`, `-3`… instead of overwriting.
fn publish(root: &Path, stem: &str, ext: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let anchor = Anchor::open(root)?;
    anchor.writable()?;
    let partial = format!(".deas-intake-{}.partial", uuid::Uuid::new_v4());
    let mut file = anchor.partial(&partial)?;
    let written = file
        .write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Cannot write the copied media");
    if let Err(e) = written {
        anchor.remove_partial(&partial);
        return Err(e.into());
    }
    for n in 1..=999 {
        let name = if n == 1 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem}-{n}.{ext}")
        };
        if anchor.absent(&name).is_ok() && anchor.publish(&partial, &name).is_ok() {
            return Ok(root.join(name));
        }
    }
    anchor.remove_partial(&partial);
    Err("No free file name for the copied media".into())
}

fn sniffed(bytes: &[u8], images_only: bool) -> Result<&'static str, String> {
    let (kind, ext) = crate::jobs::sniff_media(&bytes[..bytes.len().min(64)])
        .filter(|(kind, _)| *kind == "image" || (!images_only && *kind == "video"))
        .ok_or(if images_only {
            "Only PNG, JPEG or WebP pictures can be pasted"
        } else {
            "Only pictures (PNG/JPEG/WebP) and videos (MP4/MOV/WebM/MKV) can be copied in"
        })?;
    let limit = if kind == "image" {
        IMAGE_LIMIT
    } else {
        VIDEO_LIMIT
    };
    if bytes.len() as u64 > limit {
        return Err(format!("Copied media must be at most {} MiB", limit >> 20));
    }
    Ok(ext)
}

pub(crate) fn paste_into(root: &Path, bytes: &[u8], stamp: &str) -> Result<PathBuf, String> {
    // The stamp only names the file; it must stay a plain local `YYYYMMDD-HHMMSS`.
    let ok = stamp.len() == 15
        && stamp
            .char_indices()
            .all(|(i, c)| if i == 8 { c == '-' } else { c.is_ascii_digit() });
    if !ok {
        return Err("Invalid paste timestamp".into());
    }
    let ext = sniffed(bytes, true)?;
    publish(root, &format!("pasted-{stamp}"), ext, bytes)
}

pub(crate) fn copy_into(root: &Path, source: &Path) -> Result<PathBuf, String> {
    let file = std::fs::File::open(source).map_err(|_| "Dropped file unreadable")?;
    let meta = file.metadata().map_err(|_| "Dropped file unreadable")?;
    if !meta.is_file() {
        return Err("Only files can be copied in".into());
    }
    if meta.len() > VIDEO_LIMIT {
        return Err(format!(
            "Copied media must be at most {} MiB",
            VIDEO_LIMIT >> 20
        ));
    }
    let mut bytes = Vec::new();
    file.take(VIDEO_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Dropped file unreadable")?;
    let ext = sniffed(&bytes, false)?;
    let stem = clean_stem(source.file_stem().and_then(|s| s.to_str()).unwrap_or(""));
    publish(root, &stem, ext, &bytes)
}

fn reply(result: Result<PathBuf, String>) -> Result<String, String> {
    crate::logs::push(
        crate::logs::Source::Deas,
        if result.is_ok() {
            "INFO: Media operation=copy-in completed"
        } else {
            "ERROR: Media operation=copy-in failed; inspect the returned error"
        },
    );
    result.and_then(|p| {
        p.to_str()
            .map(str::to_owned)
            .ok_or_else(|| "Non-Unicode media path".into())
    })
}

/// Pasted picture bytes (clipboard content has no path) become a new file in Input.
#[tauri::command]
pub fn native_media_paste(
    bytes: Vec<u8>,
    stamp: String,
    engine: tauri::State<'_, crate::install::Engine>,
    operations: tauri::State<'_, crate::FolderOperations>,
) -> Result<String, String> {
    reply((|| {
        let _guard = operations.enter()?;
        paste_into(&input_root(&engine)?, &bytes, &stamp)
    })())
}

/// A dropped picture/video file is copied (never moved) into Input under its own readable name.
#[tauri::command]
pub fn native_media_copy_in(
    path: String,
    engine: tauri::State<'_, crate::install::Engine>,
    operations: tauri::State<'_, crate::FolderOperations>,
) -> Result<String, String> {
    reply((|| {
        let _guard = operations.enter()?;
        copy_into(&input_root(&engine)?, Path::new(&path))
    })())
}

#[cfg(test)]
mod tests {
    use super::*;
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n rest of a picture";

    #[test]
    fn paste_names_by_stamp_and_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let a = paste_into(dir.path(), PNG, "20261108-153000").unwrap();
        let b = paste_into(dir.path(), PNG, "20261108-153000").unwrap();
        assert_eq!(a.file_name().unwrap(), "pasted-20261108-153000.png");
        assert_eq!(b.file_name().unwrap(), "pasted-20261108-153000-2.png");
        assert_eq!(std::fs::read(&a).unwrap(), PNG);
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".partial"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn paste_rejects_non_pictures_and_unsafe_stamps() {
        let dir = tempfile::tempdir().unwrap();
        assert!(paste_into(dir.path(), b"plain text", "20261108-153000").is_err());
        assert!(paste_into(dir.path(), PNG, "../../etc/x").is_err());
        assert!(paste_into(dir.path(), PNG, "20261108_153000").is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn copy_keeps_a_clean_stem_uses_the_sniffed_extension_and_leaves_the_source() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let source = src.path().join("my shot (1).jpeg.bin");
        std::fs::write(&source, PNG).unwrap();
        let copied = copy_into(dst.path(), &source).unwrap();
        assert_eq!(copied.file_name().unwrap(), "my shot _1__jpeg.png");
        assert!(source.is_file());
        let text = src.path().join("notes.txt");
        std::fs::write(&text, b"hello").unwrap();
        assert!(copy_into(dst.path(), &text).is_err());
        assert!(copy_into(dst.path(), src.path()).is_err());
    }
}
