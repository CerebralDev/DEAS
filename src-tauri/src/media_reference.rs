//! Read-only video reference frames; no WebView video decoder or persistent worker.
use crate::video_post::{identity, open_source, run_probe};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

static EXTRACTING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

fn thumbnail(path: &Path, seconds: f64, cache: &Path) -> Result<PathBuf, String> {
    if !seconds.is_finite() || !(0.0..=3600.0).contains(&seconds) {
        return Err(
            "Reference thumbnail time must be finite and between 0 and 3600 seconds".into(),
        );
    }
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let demuxer = match ext.as_str() {
        "mp4" | "mov" | "m4v" => "mov",
        "mkv" | "webm" => "matroska",
        "avi" => "avi",
        "mpg" | "mpeg" => "mpeg",
        _ => return Err("Reference thumbnails require a video file".into()),
    };
    let file = open_source(path)?;
    let before = identity(&file)?;
    let source = dunce::canonicalize(path).map_err(|_| "Cannot resolve reference video")?;
    let key = Sha256::digest(format!(
        "reference-v1\0{}\0{before:?}\0{}",
        source.display(),
        seconds.to_bits()
    ));
    let target = cache.join(format!("{key:x}.png"));
    if target.is_file() {
        return Ok(target);
    }
    // The shared runner caps stdout at 1 MiB, stderr at 16 KiB and owns process-tree cleanup.
    let mut command = Command::new("ffmpeg");
    command.args([
        "-v",
        "error",
        "-nostdin",
        "-max_alloc",
        "67108864",
        "-protocol_whitelist",
        "file",
        "-f",
        demuxer,
        "-probesize",
        "4194304",
        "-analyzeduration",
        "1000000",
        "-threads",
        "1",
    ]);
    if demuxer == "mov" {
        command.args(["-enable_drefs", "0", "-use_absolute_path", "0"]);
    }
    command
        .args(["-ss", &seconds.to_string(), "-i"])
        .arg(&source)
        .args([
            "-map",
            "0:v:0",
            "-an",
            "-sn",
            "-dn",
            "-frames:v",
            "1",
            "-vf",
            "scale=320:320:force_original_aspect_ratio=decrease,setsar=1",
            "-threads",
            "1",
            "-filter_threads",
            "1",
            "-pix_fmt",
            "rgb24",
            "-c:v",
            "png",
            "-f",
            "image2pipe",
            "pipe:1",
        ]);
    let bytes = run_probe(command, Duration::from_secs(10)).map_err(|error| {
        let kind = error.split(':').next().unwrap_or("VIDEO_PROBE");
        format!("Reference thumbnail FFmpeg extraction failed ({kind}); check FFmpeg is on PATH and the video is decodable")
    })?;
    // Seeking past EOF can succeed without producing a frame; never cache that as a PNG.
    let reader = png::Decoder::new(bytes.as_slice())
        .read_info()
        .map_err(|_| "FFmpeg produced no reference thumbnail")?;
    let info = reader.info();
    if info.width == 0 || info.height == 0 || info.width.max(info.height) > 320 {
        return Err("FFmpeg reference thumbnail exceeded 320 pixels".into());
    }
    if identity(&file)? != before || identity(&open_source(path)?)? != before {
        return Err("Reference video changed during thumbnail extraction".into());
    }
    std::fs::create_dir_all(cache).map_err(|_| "Cannot create reference thumbnail cache")?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(cache).map_err(|_| "Cannot stage reference thumbnail")?;
    temporary
        .write_all(&bytes)
        .map_err(|_| "Cannot write reference thumbnail")?;
    match temporary.persist_noclobber(&target) {
        Ok(_) => {}
        Err(error)
            if error.error.kind() == std::io::ErrorKind::AlreadyExists && target.is_file() => {}
        Err(_) => return Err("Cannot publish reference thumbnail".into()),
    }
    Ok(target)
}

fn finish<T>(result: Result<T, String>) -> Result<T, String> {
    if result.is_err() {
        crate::logs::native_logs_reference_media_error();
    }
    result
}

/// Returns a cached PNG path for convertFileSrc, on every native platform.
#[tauri::command]
pub async fn native_media_reference_thumbnail(
    app: tauri::AppHandle,
    path: String,
    seconds: f64,
) -> Result<String, String> {
    use tauri::Manager;
    let result = async {
        let cache = app
            .path()
            .app_cache_dir()
            .map_err(|e| e.to_string())?
            .join("reference-thumbnails");
        let permit = tokio::time::timeout(Duration::from_secs(30), EXTRACTING.acquire())
            .await
            .map_err(|_| "Reference thumbnail extraction is busy; try again")?
            .map_err(|_| "Reference thumbnail extraction is unavailable")?;
        tauri::async_runtime::spawn_blocking(move || {
            let _permit = permit;
            finish(thumbnail(Path::new(&path), seconds, &cache).and_then(|p| {
                p.into_os_string()
                    .into_string()
                    .map_err(|_| "Thumbnail path is not UTF-8".into())
            }))
        })
        .await
        .map_err(|_| "Reference thumbnail worker failed".to_string())?
    }
    .await;
    finish(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_time_and_paths_without_cache_writes() {
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("cache");
        for seconds in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 3600.01] {
            assert!(thumbnail(&root.path().join("video.mp4"), seconds, &cache)
                .unwrap_err()
                .contains("time"));
        }
        for path in [
            PathBuf::from("relative.mp4"),
            root.path().join("missing.mp4"),
            root.path().join("image.png"),
        ] {
            assert!(thumbnail(&path, 0.0, &cache).is_err());
        }
        std::fs::create_dir(root.path().join("directory.mp4")).unwrap();
        assert!(thumbnail(&root.path().join("directory.mp4"), 0.0, &cache).is_err());
        assert!(!cache.exists());
    }

    #[test]
    fn errors_reach_shared_reference_logging_without_paths() {
        let result: Result<(), String> = finish(Err("private-video.mp4".into()));
        assert!(result.is_err());
        assert!(crate::logs::native_logs(0)
            .iter()
            .any(|entry| entry.source == crate::logs::Source::Deas
                && entry.text.starts_with("ERROR: Studio ReferenceMedia")
                && !entry.text.contains("private-video.mp4")));
    }

    #[test]
    fn real_ffmpeg_frames_follow_trim_and_cache_without_touching_source() {
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            eprintln!("ffmpeg unavailable; reference thumbnail fixture skipped");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("reference ü space.mp4");
        let mut fixture = Command::new("ffmpeg");
        fixture
            .args([
                "-v",
                "error",
                "-nostdin",
                "-f",
                "lavfi",
                "-i",
                "color=red:s=640x360:r=4:d=1",
                "-f",
                "lavfi",
                "-i",
                "color=blue:s=640x360:r=4:d=1",
                "-filter_complex",
                "[0:v][1:v]concat=n=2:v=1:a=0[v]",
                "-map",
                "[v]",
                "-c:v",
                "mpeg4",
                "-threads",
                "1",
            ])
            .arg(&source);
        run_probe(fixture, Duration::from_secs(10)).unwrap();
        let original = std::fs::read(&source).unwrap();
        let cache = root.path().join("cache");
        let first = thumbnail(&source, 0.0, &cache).unwrap();
        let later = thumbnail(&source, 1.5, &cache).unwrap();
        assert_ne!(first, later);
        let pixels = |path: &Path| {
            let bytes = std::fs::read(path).unwrap();
            let mut reader = png::Decoder::new(bytes.as_slice()).read_info().unwrap();
            assert_eq!((reader.info().width, reader.info().height), (320, 180));
            let mut pixels = vec![0; reader.output_buffer_size()];
            reader.next_frame(&mut pixels).unwrap();
            pixels
        };
        let red = pixels(&first);
        let blue = pixels(&later);
        assert!(red[0] > 200 && red[2] < 30);
        assert!(blue[2] > 200 && blue[0] < 30);
        let modified = std::fs::metadata(&first).unwrap().modified().unwrap();
        assert_eq!(thumbnail(&source, 0.0, &cache).unwrap(), first);
        assert_eq!(
            std::fs::metadata(&first).unwrap().modified().unwrap(),
            modified
        );
        assert!(thumbnail(&source, 3600.0, &cache).is_err());
        let corrupt = root.path().join("corrupt.mp4");
        std::fs::write(&corrupt, b"not a video at all").unwrap();
        assert!(thumbnail(&corrupt, 0.0, &cache).is_err());
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 2);
        assert_eq!(std::fs::read(&source).unwrap(), original);

        let mut audio = Command::new("ffmpeg");
        let audio_path = root.path().join("audio-only.mp4");
        audio
            .args([
                "-v", "error", "-nostdin", "-f", "lavfi", "-i", "anullsrc", "-t", "0.1", "-c:a",
                "aac",
            ])
            .arg(&audio_path);
        run_probe(audio, Duration::from_secs(10)).unwrap();
        assert!(thumbnail(&audio_path, 0.0, &cache).is_err());
        assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 2);

        // A source mutation must not reuse its previous cached frame.
        let replacement = root.path().join("replacement.mp4");
        std::fs::copy(&source, &replacement).unwrap();
        let previous = thumbnail(&replacement, 0.0, &cache).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&replacement)
            .unwrap()
            .write_all(b"cache invalidation")
            .unwrap();
        assert_ne!(thumbnail(&replacement, 0.0, &cache).unwrap(), previous);
    }
}
