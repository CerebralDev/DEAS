//! Export without metadata: a clean copy of an owned output, written where the user chooses.
//! Pictures are rewritten chunk-by-chunk (no re-encode); video/audio is an FFmpeg stream-copy remux.
use crate::downloads::fs::Anchor;
use serde::Serialize;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

const MAX_PICTURE: u64 = 512 * 1024 * 1024;
const BAD_PICTURE: &str = "The picture is truncated or malformed; nothing was exported";

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CleanExport {
    pub path: String,
    pub size: u64,
}

/// Canonical source inside the canonical Output root, plus its lowercase extension.
fn owned_source(root: &Path, source: &Path) -> Result<(PathBuf, String), String> {
    if !source.is_absolute()
        || source
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err("Media path must be absolute without traversal".into());
    }
    let meta = std::fs::symlink_metadata(source).map_err(|_| "Media is missing or unreadable")?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("Only regular media files can be exported".into());
    }
    let root = std::fs::canonicalize(root).map_err(|_| "The Output folder is unavailable")?;
    let canonical = std::fs::canonicalize(source).map_err(|_| "Media is missing or unreadable")?;
    if canonical == root || !canonical.starts_with(&root) {
        return Err("External/unmanaged media cannot be exported".into());
    }
    let name = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid media filename")?;
    let kind = crate::media::media_kind(name).ok_or("Unsupported media type")?;
    let ext = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if kind == "image" && !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp") {
        return Err("Export without metadata supports PNG, JPEG and WebP pictures".into());
    }
    Ok((canonical, ext))
}

/// `dest` is the save-dialog choice; `None` (cancelled) writes nothing.
pub(crate) fn export_clean(
    root: &Path,
    source: &Path,
    dest: Option<&Path>,
) -> Result<Option<CleanExport>, String> {
    let (canonical, ext) = owned_source(root, source)?;
    let Some(dest) = dest else {
        return Ok(None);
    };
    let name = dest
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.chars().any(char::is_control))
        .ok_or("Invalid export filename")?;
    if !dest.is_absolute()
        || dest
            .extension()
            .and_then(|s| s.to_str())
            .map(str::to_ascii_lowercase)
            != Some(ext.clone())
        || dest.components().any(|c| {
            !matches!(
                c,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
    {
        return Err(format!("Select a new absolute .{ext} file"));
    }
    let parent_path = dest.parent().ok_or("Missing export folder")?;
    if std::fs::canonicalize(parent_path).is_ok_and(|p| p.join(name) == canonical) {
        return Err("The clean copy cannot replace its source".into());
    }
    let parent = Anchor::open(parent_path)?;
    parent.validate_directory(false)?;
    // Same no-clobber publication as native_job_export: an existing file is never replaced.
    parent.absent(name)?;
    let temp = format!(".deas-{}.{ext}", uuid::Uuid::new_v4());
    let mut file = parent.partial(&temp)?;
    let result = (|| {
        if crate::media::media_kind(name) == Some("image") {
            let clean = strip_picture(&ext, &read_picture(source)?)?;
            file.write_all(&clean)
                .map_err(|_| "Cannot write the clean copy")?;
        } else {
            remux(&canonical, &parent_path.join(&temp))?;
        }
        file.sync_all().map_err(|_| "Cannot sync the clean copy")?;
        parent.validate_directory(false)?;
        if !parent.owns_partial(&temp, &file) {
            return Err("Export temporary file changed".to_string());
        }
        let size = file
            .metadata()
            .map_err(|_| "Cannot inspect the clean copy")?
            .len();
        parent.publish(&temp, name)?;
        Ok(size)
    })();
    if parent.owns_partial(&temp, &file) {
        parent.remove_partial(&temp);
    }
    Ok(Some(CleanExport {
        size: result?,
        path: dest.to_string_lossy().into_owned(),
    }))
}

fn read_picture(source: &Path) -> Result<Vec<u8>, String> {
    let file = crate::media::star::open_media(source)?;
    let meta = file.metadata().map_err(|_| "Cannot inspect media")?;
    if !meta.is_file() || meta.len() > MAX_PICTURE {
        return Err("Pictures must be regular files no larger than 512 MiB".into());
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.take(MAX_PICTURE)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read media")?;
    Ok(bytes)
}

fn strip_picture(ext: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
    match ext {
        "png" => strip_png(bytes),
        "jpg" | "jpeg" => strip_jpeg(bytes),
        _ => strip_webp(bytes),
    }
}

/// Drops text/EXIF chunks; colour (iCCP/sRGB/gAMA/cHRM) and image chunks are copied byte-identical.
fn strip_png(b: &[u8]) -> Result<Vec<u8>, String> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !b.starts_with(SIGNATURE) {
        return Err("The file is not a PNG picture".into());
    }
    let mut out = SIGNATURE.to_vec();
    let mut at = SIGNATURE.len();
    loop {
        let len = b.get(at..at + 4).ok_or(BAD_PICTURE)?;
        let len = u32::from_be_bytes(len.try_into().unwrap()) as usize;
        let chunk = b.get(at..at + 12 + len).ok_or(BAD_PICTURE)?;
        let kind = &chunk[4..8];
        if !matches!(kind, b"tEXt" | b"zTXt" | b"iTXt" | b"eXIf") {
            out.extend_from_slice(chunk);
        }
        at += chunk.len();
        if kind == b"IEND" {
            return Ok(out);
        }
    }
}

/// Drops APP1 (EXIF/XMP), APP13 (IPTC) and COM; APP0/APP2 ICC/APP14 and image data stay byte-identical.
fn strip_jpeg(b: &[u8]) -> Result<Vec<u8>, String> {
    if !b.starts_with(&[0xFF, 0xD8]) {
        return Err("The file is not a JPEG picture".into());
    }
    let mut out = vec![0xFF, 0xD8];
    let mut at = 2;
    loop {
        let marker = match b.get(at..at + 2) {
            Some([0xFF, marker]) => *marker,
            _ => return Err(BAD_PICTURE.into()),
        };
        match marker {
            // Fill byte before a marker.
            0xFF => at += 1,
            // shortcut: everything from the first scan on is copied verbatim; encoders write metadata before SOS, strip later segments if a file proves otherwise.
            0xDA | 0xD9 => {
                out.extend_from_slice(&b[at..]);
                return Ok(out);
            }
            0x01 | 0xD0..=0xD7 => {
                out.extend_from_slice(&b[at..at + 2]);
                at += 2;
            }
            _ => {
                let len = b.get(at + 2..at + 4).ok_or(BAD_PICTURE)?;
                let len = u16::from_be_bytes(len.try_into().unwrap()) as usize;
                if len < 2 {
                    return Err(BAD_PICTURE.into());
                }
                let segment = b.get(at..at + 2 + len).ok_or(BAD_PICTURE)?;
                if !matches!(marker, 0xE1 | 0xED | 0xFE) {
                    out.extend_from_slice(segment);
                }
                at += segment.len();
            }
        }
    }
}

/// Drops EXIF/XMP chunks, clears their VP8X flags and rewrites the RIFF size.
fn strip_webp(b: &[u8]) -> Result<Vec<u8>, String> {
    if b.len() < 12 || &b[..4] != b"RIFF" || &b[8..12] != b"WEBP" {
        return Err("The file is not a WebP picture".into());
    }
    let riff_end = u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize + 8;
    let body = b.get(12..riff_end).ok_or(BAD_PICTURE)?;
    let mut out = b[..12].to_vec();
    let mut at = 0;
    while at < body.len() {
        let size = body.get(at + 4..at + 8).ok_or(BAD_PICTURE)?;
        let size = u32::from_le_bytes(size.try_into().unwrap()) as usize;
        let chunk = body
            .get(at..at + 8 + size + (size & 1))
            .ok_or(BAD_PICTURE)?;
        match &chunk[..4] {
            b"EXIF" | b"XMP " => {}
            b"VP8X" => {
                let mut chunk = chunk.to_vec();
                *chunk.get_mut(8).ok_or(BAD_PICTURE)? &= !0x0C;
                out.extend_from_slice(&chunk);
            }
            _ => out.extend_from_slice(chunk),
        }
        at += chunk.len();
    }
    let size = u32::try_from(out.len() - 8).map_err(|_| BAD_PICTURE)?;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    Ok(out)
}

/// Stream-copy remux: no re-encode, container/stream/chapter metadata dropped.
fn remux(source: &Path, target: &Path) -> Result<(), String> {
    let mut command = Command::new("ffmpeg");
    command
        .args(["-hide_banner", "-v", "error", "-nostdin", "-i"])
        .arg(dunce::simplified(source))
        .args([
            "-map",
            "0",
            "-map_metadata",
            "-1",
            "-map_chapters",
            "-1",
            "-c",
            "copy",
            "-fflags",
            "+bitexact",
            "-y",
        ])
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(crate::win::CREATE_NO_WINDOW);
    }
    // shortcut: no deadline; a stream copy is bounded by the source size, add run_tool's timeout if a hang is reported.
    let output = command.output().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "Install FFmpeg and make it available on PATH, then restart DEAS"
        } else {
            "Cannot start FFmpeg"
        }
    })?;
    if output.status.success() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&output.stderr);
    let text = text.trim();
    let tail = &text[text.floor_char_boundary(text.len().saturating_sub(600))..];
    Err(format!(
        "FFmpeg could not remux this file without metadata: {tail}"
    ))
}

#[tauri::command]
pub async fn native_media_export_clean(
    path: String,
    title: String,
    engine: tauri::State<'_, crate::install::Engine>,
    operations: tauri::State<'_, crate::FolderOperations>,
) -> Result<Option<CleanExport>, String> {
    if title.chars().count() > 240 || title.chars().any(char::is_control) {
        return Err("Invalid save dialog title".into());
    }
    let root = crate::media::star::output_root(&engine)?;
    let source = PathBuf::from(&path);
    // Refuse before showing a dialog the user could not use.
    let (_, ext) = owned_source(&root, &source)?;
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("DEAS-output");
    let selected = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter(ext.to_ascii_uppercase(), &[&ext])
        .set_file_name(format!("{stem}-clean.{ext}"))
        .set_can_create_directories(false)
        .save_file()
        .await;
    // The guard is not Send; a remux may take a while, so tell the runtime this worker blocks.
    let result = tokio::task::block_in_place(|| {
        let _guard = operations.enter()?;
        export_clean(&root, &source, selected.as_ref().map(|f| f.path()))
    });
    crate::logs::push(
        crate::logs::Source::Deas,
        if result.is_ok() {
            "INFO: Media operation=export-clean completed"
        } else {
            "ERROR: Media operation=export-clean failed; inspect the returned error"
        },
    );
    result
}

#[cfg(test)]
#[path = "media_export_tests.rs"]
mod tests;
