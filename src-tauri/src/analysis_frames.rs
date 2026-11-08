//! Media Analysis' frame picker: every frame of a video as a strip, frame-exact.
//!
//! PromptForge draws frames in the browser by seeking to the middle of each frame's
//! slot. DEAS cannot rely on the WebView for video (Linux plays through libmpv), so
//! frames come from FFmpeg by number instead: `select=eq(n,N)` counts decoded frames,
//! which is exact whatever the frame rate or timestamps say.
//!
//! - Open: ffprobe counts the frames (packets, nothing decoded), then one FFmpeg pass
//!   writes a small JPEG per frame (every k-th past 2,000 frames) into a cache that
//!   keeps the five newest clips.
//! - One frame: decoded at full size as a PNG into the runtime's `frames` folder (the
//!   200 newest are kept), where tagging, Describe and the Generate inputs read it.
//! - Save: one folder dialog, then each picked frame as `<video>-f0001.png`.
//!
//! The video is read, never written; nothing keeps running after a call.

use crate::video_post::{inspect_source, run_tool};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicBool,
    time::Duration,
};

/// The strip holds a thumbnail per frame up to this many, as PromptForge's does.
pub const MAX_THUMBS: u32 = 2000;
const THUMB_HEIGHT: u32 = 90;
const KEEP_CLIPS: usize = 5;
const KEEP_FRAMES: usize = 200;

static RUNNING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub count: u32,
    pub duration: f64,
    /// Thumbnail i shows frame i × stride.
    pub stride: u32,
    pub thumbs: Vec<String>,
}

fn root() -> PathBuf {
    crate::analysis::runtime_root()
}

fn quiet(command: &mut Command) {
    command.stdin(std::process::Stdio::null());
}

/// ffmpeg/ffprobe input options for a source, the same guards as Studio's reference thumbnails.
fn input(command: &mut Command, source: &Path) {
    command.args([
        "-v",
        "error",
        "-protocol_whitelist",
        "file",
        "-probesize",
        "4194304",
        "-analyzeduration",
        "1000000",
    ]);
    command.arg("-i").arg(source);
}

fn ffmpeg_error(e: String, what: &str) -> String {
    if e.starts_with("VIDEO_TOOL_MISSING") {
        return "ANALYSIS_FRAMES: FFmpeg is not installed. Install FFmpeg (with ffprobe) and make it available on PATH, then restart DEAS.".into();
    }
    if e.contains("TIMEOUT") {
        return format!("ANALYSIS_FRAMES: Reading {what} took too long and was stopped; try a shorter clip.");
    }
    format!("ANALYSIS_FRAMES: FFmpeg could not read {what}; the video may be damaged or in a format FFmpeg cannot decode.")
}

/// Frames in the first video stream, from the container's packets (nothing decoded).
fn count_frames(source: &Path) -> Result<u32, String> {
    let mut command = Command::new("ffprobe");
    command.args(["-count_packets", "-select_streams", "v:0"]);
    input(&mut command, source);
    command.args(["-show_entries", "stream=nb_read_packets", "-of", "csv=p=0"]);
    quiet(&mut command);
    let out = run_tool(command, Duration::from_secs(60), &AtomicBool::new(false), None)
        .map_err(|e| ffmpeg_error(e, "the video"))?;
    String::from_utf8_lossy(&out)
        .trim()
        .trim_end_matches(',')
        .parse::<u32>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| "ANALYSIS_FRAMES: The video has no frames FFmpeg can count.".into())
}

fn prune(dir: &Path, keep: usize) {
    let mut items: Vec<_> = fs::read_dir(dir)
        .map(|r| {
            r.flatten()
                .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
                .collect()
        })
        .unwrap_or_default();
    items.sort();
    for (_, p) in items.iter().rev().skip(keep) {
        let _ = if p.is_dir() {
            fs::remove_dir_all(p)
        } else {
            fs::remove_file(p)
        };
    }
}

fn clip_key(source: &Path) -> Result<String, String> {
    let meta = fs::metadata(source).map_err(|_| "ANALYSIS_FRAMES: That video no longer exists")?;
    let canonical = dunce::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    Ok(format!(
        "{:x}",
        Sha256::digest(format!(
            "strip-v1\0{}\0{}\0{:?}",
            canonical.display(),
            meta.len(),
            meta.modified().ok()
        ))
    ))
}

fn sorted_thumbs(dir: &Path) -> Vec<String> {
    let mut thumbs: Vec<PathBuf> = fs::read_dir(dir)
        .map(|r| {
            r.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "jpg"))
                .collect()
        })
        .unwrap_or_default();
    thumbs.sort();
    thumbs.iter().map(|p| p.to_string_lossy().into()).collect()
}

pub fn open(source: &Path) -> Result<Clip, String> {
    let meta = inspect_source(source).map_err(|e| {
        if e.starts_with("VIDEO_TYPE") {
            "ANALYSIS_FRAMES: The frame picker reads MP4, MOV, MKV and WebM videos.".to_string()
        } else if e.starts_with("VIDEO_TOOL_MISSING") {
            ffmpeg_error(e, "the video")
        } else {
            format!(
                "ANALYSIS_FRAMES: That video cannot be read ({}).",
                e.split_once(": ").map_or(e.as_str(), |(_, m)| m).trim_end_matches('.')
            )
        }
    })?;
    let fps = if meta.fps_denominator > 0 {
        f64::from(meta.fps_numerator) / f64::from(meta.fps_denominator)
    } else {
        0.0
    };
    let mut count = count_frames(source)?;
    let stride = count.div_ceil(MAX_THUMBS).max(1);
    let dir = root().join("frame-strips").join(clip_key(source)?);
    let done = dir.join("done.txt");
    if !done.is_file() {
        if dir.exists() {
            let _ = fs::remove_dir_all(&dir);
        }
        fs::create_dir_all(&dir)
            .map_err(|_| "ANALYSIS_FRAMES: Cannot create the frame strip folder")?;
        let mut command = Command::new("ffmpeg");
        command.arg("-nostdin");
        input(&mut command, source);
        command.args([
            "-map",
            "0:v:0",
            "-an",
            "-sn",
            "-dn",
            "-fps_mode",
            "passthrough",
            "-vf",
            &format!(
                "select='not(mod(n\\,{stride}))',scale=-2:{THUMB_HEIGHT}:flags=bilinear"
            ),
            "-q:v",
            "5",
        ]);
        command.arg(dir.join("%06d.jpg"));
        quiet(&mut command);
        run_tool(command, Duration::from_secs(600), &AtomicBool::new(false), None)
            .map_err(|e| ffmpeg_error(e, "the video's frames"))?;
        fs::write(&done, count.to_string()).map_err(|_| "ANALYSIS_FRAMES: Cannot finish the frame strip")?;
        prune(&root().join("frame-strips"), KEEP_CLIPS);
    }
    let thumbs = sorted_thumbs(&dir);
    if thumbs.is_empty() {
        return Err("ANALYSIS_FRAMES: FFmpeg found no frames in that video.".into());
    }
    // Every frame was decoded: trust the decoder's count over the container's.
    if stride == 1 {
        count = thumbs.len() as u32;
    }
    Ok(Clip {
        name: meta.name,
        width: meta.width,
        height: meta.height,
        fps,
        count,
        duration: meta.duration_seconds.unwrap_or(count as f64 / fps.max(1.0)),
        stride,
        thumbs,
    })
}

fn frame_name(source: &Path, index: u32, count: u32) -> String {
    let stem: String = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video")
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .take(120)
        .collect();
    let digits = count.max(1).to_string().len().max(4);
    format!("{stem}-f{:0digits$}.png", index + 1)
}

/// Frame `index` (from 0) at full size as a PNG at `target`.
fn extract(source: &Path, index: u32, target: &Path) -> Result<(), String> {
    let part = target.with_extension("part.png");
    let mut command = Command::new("ffmpeg");
    command.arg("-nostdin");
    input(&mut command, source);
    command.args([
        "-map",
        "0:v:0",
        "-an",
        "-sn",
        "-dn",
        "-fps_mode",
        "passthrough",
        "-vf",
        &format!("select='eq(n\\,{index})'"),
        "-frames:v",
        "1",
        "-pix_fmt",
        "rgb24",
        "-c:v",
        "png",
        "-y",
    ]);
    command.arg(&part);
    quiet(&mut command);
    run_tool(command, Duration::from_secs(300), &AtomicBool::new(false), None)
        .map_err(|e| ffmpeg_error(e, "that frame"))?;
    if !fs::metadata(&part).is_ok_and(|m| m.len() > 0) {
        let _ = fs::remove_file(&part);
        return Err(format!("ANALYSIS_FRAMES: The video has no frame {}.", index + 1));
    }
    fs::rename(&part, target).map_err(|_| "ANALYSIS_FRAMES: Cannot save the frame".into())
}

pub fn frame(source: &Path, index: u32, count: u32) -> Result<PathBuf, String> {
    if index >= count.max(1) {
        return Err(format!("ANALYSIS_FRAMES: The video has no frame {}.", index + 1));
    }
    let dir = root().join("frames").join(&clip_key(source)?[..16]);
    fs::create_dir_all(&dir).map_err(|_| "ANALYSIS_FRAMES: Cannot create the frames folder")?;
    let target = dir.join(frame_name(source, index, count));
    if !target.is_file() {
        extract(source, index, &target)?;
    }
    // Touch so the newest stay; prune clips (folders) past the frame budget.
    prune(&root().join("frames"), KEEP_FRAMES / 10);
    prune(&dir, KEEP_FRAMES);
    Ok(target)
}

/// `name.png`, or `name (2).png` when that is taken: a save never replaces a file.
fn free_name(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, "png"));
    (2..10_000)
        .map(|n| dir.join(format!("{stem} ({n}).{ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

pub fn save(source: &Path, indices: &[u32], count: u32, dir: &Path) -> Result<usize, String> {
    for &i in indices {
        let target = free_name(dir, &frame_name(source, i, count));
        extract(source, i, &target)?;
    }
    Ok(indices.len())
}

// ── commands ────────────────────────────────────────────────────────────────

fn video_path(path: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(path);
    if !p.is_absolute() || !p.is_file() {
        return Err("ANALYSIS_FRAMES: That video no longer exists".into());
    }
    Ok(p)
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let _permit = tokio::time::timeout(Duration::from_secs(600), RUNNING.acquire())
        .await
        .map_err(|_| "ANALYSIS_FRAMES: The frame picker is busy; try again")?
        .map_err(|_| "ANALYSIS_FRAMES: The frame picker is unavailable")?;
    let out = tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| "ANALYSIS_FRAMES: The frame reader failed".to_string())?;
    if let Err(e) = &out {
        crate::logs::push(crate::logs::Source::Deas, &format!("ERROR: Media Analysis frames: {e}"));
    }
    out
}

/// Opens a video in the frame picker: its size, rate, frame count and the thumbnail strip.
#[tauri::command]
pub async fn native_analysis_frames_open(path: String) -> Result<Clip, String> {
    let source = video_path(&path)?;
    blocking(move || open(&source)).await
}

/// One frame at full size, as a PNG path the tagger, Describe and Generate can use.
#[tauri::command]
pub async fn native_analysis_frame(path: String, index: u32, count: u32) -> Result<String, String> {
    let source = video_path(&path)?;
    blocking(move || frame(&source, index, count).map(|p| p.to_string_lossy().into())).await
}

/// Saves picked frames as PNGs into a folder the person chooses; `None` when they cancel.
#[tauri::command]
pub async fn native_analysis_frames_save(
    path: String,
    indices: Vec<u32>,
    count: u32,
    title: String,
) -> Result<Option<usize>, String> {
    let source = video_path(&path)?;
    if indices.is_empty() || indices.len() > 10_000 || indices.iter().any(|i| *i >= count) {
        return Err("ANALYSIS_FRAMES: Pick the frames to save first".into());
    }
    if title.trim().is_empty() || title.len() > 240 || title.chars().any(char::is_control) {
        return Err("ANALYSIS_FRAMES: Invalid dialog title".into());
    }
    let Some(dir) = rfd::AsyncFileDialog::new()
        .set_title(title)
        .pick_folder()
        .await
    else {
        return Ok(None);
    };
    let dir = dir.path().to_owned();
    blocking(move || save(&source, &indices, count, &dir).map(Some)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ffmpeg() -> bool {
        Command::new("ffmpeg").arg("-version").output().is_ok()
    }

    /// 12 frames at 4 fps, each a different grey, so a frame's pixels say which it is.
    fn fixture(dir: &Path) -> PathBuf {
        let source = dir.join("clip ü space.mp4");
        let mut command = Command::new("ffmpeg");
        command.args([
            "-v", "error", "-nostdin", "-f", "lavfi", "-i",
            "color=black:s=64x48:r=4:d=3,geq=lum='N*20':cb=128:cr=128",
            "-c:v", "libx264", "-qp", "0", "-pix_fmt", "yuv420p", "-g", "5", "-bf", "2",
        ]);
        command.arg(&source);
        assert!(command.status().unwrap().success());
        source
    }

    fn grey(path: &Path) -> u8 {
        let bytes = fs::read(path).unwrap();
        let mut reader = png::Decoder::new(bytes.as_slice()).read_info().unwrap();
        let mut px = vec![0; reader.output_buffer_size()];
        reader.next_frame(&mut px).unwrap();
        px[0]
    }

    #[test]
    fn frames_are_exact_by_number_and_saves_never_replace() {
        if !ffmpeg() {
            eprintln!("ffmpeg unavailable; frame picker fixture skipped");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("DEAS_ANALYSIS_RUNTIME_DIR", tmp.path().join("runtime"));
        let source = fixture(tmp.path());
        let original = fs::read(&source).unwrap();
        let clip = open(&source).unwrap();
        assert_eq!((clip.count, clip.stride, clip.thumbs.len()), (12, 1, 12));
        assert_eq!((clip.width, clip.height, clip.fps), (64, 48, 4.0));
        // Opening again reuses the strip.
        assert_eq!(open(&source).unwrap().thumbs, clip.thumbs);
        // Frame N is the Nth decoded frame (B-frames and all), not a seek by time.
        for n in [0u32, 1, 6, 11] {
            let path = frame(&source, n, clip.count).unwrap();
            let g = grey(&path) as i32;
            // Y = 20n in limited range, so RGB = (Y - 16) * 255 / 219.
            let want = ((n as i32 * 20 - 16) * 255 / 219).clamp(0, 255);
            assert!((g - want).abs() <= 3, "frame {n}: {g}, want {want}");
            assert!(path.file_name().unwrap().to_string_lossy().ends_with(&format!("-f{:04}.png", n + 1)));
        }
        assert!(frame(&source, 12, clip.count).is_err());
        let out = tmp.path().join("saved");
        fs::create_dir_all(&out).unwrap();
        assert_eq!(save(&source, &[2, 3], clip.count, &out).unwrap(), 2);
        assert_eq!(save(&source, &[2], clip.count, &out).unwrap(), 1);
        let mut names: Vec<String> = fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into())
            .collect();
        names.sort();
        assert_eq!(names, ["clip ü space-f0003 (2).png", "clip ü space-f0003.png", "clip ü space-f0004.png"]);
        assert_eq!(fs::read(&source).unwrap(), original);
        // Not a video.
        let still = tmp.path().join("still.png");
        fs::write(&still, b"\x89PNG not really").unwrap();
        assert!(open(&still).unwrap_err().contains("MP4, MOV, MKV and WebM"));
        std::env::remove_var("DEAS_ANALYSIS_RUNTIME_DIR");
    }

    #[test]
    fn long_clips_get_every_kth_thumbnail() {
        assert_eq!(4000u32.div_ceil(MAX_THUMBS).max(1), 2);
        assert_eq!(1999u32.div_ceil(MAX_THUMBS).max(1), 1);
        assert_eq!(frame_name(Path::new("a/b:c.mp4"), 0, 12), "b_c-f0001.png");
        assert_eq!(frame_name(Path::new("x.mp4"), 99, 12000), "x-f00100.png");
    }
}
