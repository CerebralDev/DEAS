use super::*;
use serde_json::json;

fn metadata() -> Value {
    json!({"streams":[{"codec_type":"video", "width":64,"height":48,
        "pix_fmt":"yuv420p","field_order":"progressive", "sample_aspect_ratio":"1:1",
        "color_transfer":"bt709","color_primaries":"bt709","color_space":"bt709",
        "r_frame_rate":"60000/2002","avg_frame_rate":"30000/1001","duration":"1.001"},
        {"codec_type":"audio"},{"codec_type":"subtitle"}],
        "format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"1.1"}})
}
fn parse(v: &Value) -> Result<VideoSourceInspection, String> {
    parse_metadata("safe.mp4".into(), &serde_json::to_vec(v).unwrap())
}
#[test]
fn parser_normalizes_exact_rates_and_never_claims_readiness() {
    let result = parse(&metadata()).unwrap();
    assert_eq!(
        (result.fps_numerator, result.fps_denominator),
        (30000, 1001)
    );
    assert_eq!((result.audio_streams, result.subtitle_streams), (1, 1));
    assert_eq!(result.duration_seconds, Some(1.001));
    assert!(result.blockers.is_empty());
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(value["name"], "safe.mp4");
    assert_eq!(value["timingVerified"], false);
    assert_eq!(value["processingReady"], false);
    assert_eq!(value.as_object().unwrap().len(), 14);
    assert!(value.get("path").is_none());
    assert!(
        value.get("sourceId").is_none(),
        "native-only metadata is not chooser authority"
    );
}
#[test]
fn parser_rejects_malformed_missing_and_unbounded_metadata() {
    assert!(parse_metadata("x".into(), b"not json").is_err());
    for key in ["width", "height", "pix_fmt", "codec_type"] {
        let mut v = metadata();
        v["streams"][0].as_object_mut().unwrap().remove(key);
        assert!(parse(&v).is_err(), "missing {key}");
    }
    for (key, bad) in [
        ("width", json!(0)),
        ("width", json!(65537)),
        ("width", json!(32.5)),
        ("height", json!(-1)),
        ("r_frame_rate", json!("1/0")),
        ("r_frame_rate", json!("4294967296/1")),
        ("avg_frame_rate", json!("-1/2")),
        ("avg_frame_rate", json!("25")),
        ("avg_frame_rate", json!("1001/1")),
        ("avg_frame_rate", json!(25)),
        ("duration", json!("NaN")),
        ("duration", json!("inf")),
        ("duration", json!("-1")),
        ("duration", json!("0")),
        ("duration", json!("604801")),
        ("pix_fmt", json!("private/path")),
        ("field_order", json!("surprise")),
        ("sample_aspect_ratio", json!("1:0")),
        ("sample_aspect_ratio", json!("unknown-malformed")),
        ("tags", json!({"rotate":"unknown"})),
        ("tags", json!({"rotate":"NaN"})),
        (
            "side_data_list",
            json!([{"side_data_type":"Display Matrix"}]),
        ),
    ] {
        let mut v = metadata();
        v["streams"][0][key] = bad;
        assert!(parse(&v).is_err(), "invalid {key}: {v}");
    }
    let mut v = metadata();
    v["format"]["duration"] = json!("NaN");
    assert!(
        parse(&v).is_err(),
        "must validate fallback even when stream duration exists"
    );
    for bad in [json!([]), json!({}), json!({"streams":[],"format":{}})] {
        assert!(parse(&bad).is_err());
    }
    let mut v = metadata();
    let video = v["streams"][0].clone();
    v["streams"].as_array_mut().unwrap().push(video);
    assert!(parse(&v).unwrap_err().starts_with("VIDEO_STREAMS:"));
}
#[test]
fn parser_reports_conservative_stable_blockers() {
    for (key, value, expected) in [
        ("avg_frame_rate", json!("0/0"), vec!["cadenceUnknown"]),
        ("avg_frame_rate", json!("25/1"), vec!["cadenceMismatch"]),
        ("field_order", json!("tt"), vec!["interlaced"]),
        ("field_order", json!("unknown"), vec!["fieldOrderUnknown"]),
        ("width", json!(63), vec!["unsupportedDimensions"]),
        ("height", json!(10000), vec!["unsupportedDimensions"]),
        (
            "pix_fmt",
            json!("yuva420p"),
            vec!["alpha", "unsupportedPixelFormat"],
        ),
        (
            "pix_fmt",
            json!("rgba64le"),
            vec!["alpha", "unsupportedPixelFormat"],
        ),
        (
            "color_transfer",
            json!("smpte2084"),
            vec!["hdr", "colorUnknown"],
        ),
        (
            "color_transfer",
            json!("arib-std-b67"),
            vec!["hdr", "colorUnknown"],
        ),
        (
            "color_primaries",
            json!("bt2020"),
            vec!["hdr", "colorUnknown"],
        ),
        ("color_transfer", json!("unknown"), vec!["colorUnknown"]),
        ("sample_aspect_ratio", json!("4:3"), vec!["nonSquarePixels"]),
        (
            "sample_aspect_ratio",
            json!("N/A"),
            vec!["pixelAspectUnknown"],
        ),
        ("tags", json!({"rotate":"90"}), vec!["rotation"]),
        (
            "side_data_list",
            json!([{"side_data_type":"Display Matrix","rotation":0}]),
            vec!["rotation"],
        ),
        (
            "side_data_list",
            json!([{"side_data_type":"Mastering display metadata"}]),
            vec!["hdr"],
        ),
    ] {
        let mut v = metadata();
        v["streams"][0][key] = value;
        let result = parse(&v).unwrap();
        assert_eq!(result.blockers, expected, "{key}");
        assert!(!result.processing_ready && !result.timing_verified);
    }
    for (key, blocker) in [
        ("field_order", "fieldOrderUnknown"),
        ("color_transfer", "colorUnknown"),
        ("sample_aspect_ratio", "pixelAspectUnknown"),
        ("r_frame_rate", "cadenceUnknown"),
    ] {
        let mut v = metadata();
        v["streams"][0].as_object_mut().unwrap().remove(key);
        assert!(parse(&v).unwrap().blockers.iter().any(|b| b == blocker));
    }
    let mut v = metadata();
    v["streams"][0].as_object_mut().unwrap().remove("duration");
    v["format"].as_object_mut().unwrap().remove("duration");
    assert_eq!(parse(&v).unwrap().duration_seconds, None);
    v["streams"][0]["tags"] = json!({"ROTATE":"0"});
    assert!(!parse(&v).unwrap().blockers.contains(&"rotation".into()));
}
#[test]
fn diagnostics_deliver_failure_and_cancel_without_sensitive_values() {
    let failure_id = uuid::Uuid::new_v4();
    let cancel_id = uuid::Uuid::new_v4();
    let start = crate::native_logs(0).last().map_or(0, |e| e.seq);
    assert!(finish::<()>(failure_id, Err("private path / secret stderr".into())).is_err());
    assert_eq!(finish::<()>(cancel_id, Ok(None)).unwrap(), None);
    let entries = crate::native_logs(start);
    let failure = entries
        .iter()
        .find(|e| e.text.contains(&failure_id.to_string()))
        .unwrap();
    assert_eq!(failure.source, crate::logs::Source::Deas);
    assert!(
        failure
            .text
            .starts_with("ERROR: video-post operation=inspect operationId=")
    );
    assert!(!failure.text.contains("private") && !failure.text.contains("secret"));
    let cancel = entries
        .iter()
        .find(|e| e.text.contains(&cancel_id.to_string()))
        .unwrap();
    assert!(cancel.text.starts_with("INFO:") && cancel.text.ends_with("outcome=cancelled"));
}
#[tokio::test]
async fn invalid_title_reaches_shared_log_without_opening_chooser() {
    let start = crate::native_logs(0).last().map_or(0, |e| e.seq);
    let error = native_video_post_inspect("secret\n/path".into())
        .await
        .unwrap_err();
    assert!(error.starts_with("VIDEO_TITLE:"));
    assert!(!error.contains("secret"));
    assert!(
        crate::native_logs(start)
            .iter()
            .any(|e| e.text.starts_with("ERROR: video-post operation=inspect"))
    );
}
#[tokio::test]
async fn admission_refuses_concurrent_ipc_before_chooser() {
    let permit = INSPECTING.try_acquire().unwrap();
    let result = native_video_post_inspect("Inspect video".into()).await;
    assert!(result.unwrap_err().starts_with("VIDEO_BUSY:"));
    drop(permit);
}
#[test]
fn source_rejects_paths_signatures_and_replacements() {
    let dir = tempfile::tempdir().unwrap();
    assert!(inspect_source(Path::new("relative.mp4")).is_err());
    assert!(inspect_source(dir.path()).is_err());
    let path = dir.path().join("private ü source.mp4");
    fs::write(&path, b"not a video signature").unwrap();
    let original = open_source(&path).unwrap();
    let before = identity(&original).unwrap();
    assert!(
        inspect_source(&path)
            .unwrap_err()
            .starts_with("VIDEO_TYPE:")
    );
    fs::remove_file(&path).unwrap();
    fs::write(&path, b"different file bytes").unwrap();
    assert_ne!(identity(&open_source(&path).unwrap()).unwrap(), before);
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = dir.path().join("link.mp4");
        symlink(&path, &link).unwrap();
        assert!(open_source(&link).is_err());
        let linked_dir = dir.path().join("linked-dir");
        symlink(dir.path(), &linked_dir).unwrap();
        // Parent-directory symlinks are allowed (bookmarks); only leaf links are rejected.
        assert!(open_source(&linked_dir.join("private ü source.mp4")).is_ok());
        use std::os::unix::ffi::OsStrExt;
        let fifo = dir.path().join("fifo.mp4");
        let c = std::ffi::CString::new(fifo.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let start = Instant::now();
        assert!(open_source(&fifo).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
#[test]
#[ignore = "requires installed FFmpeg/FFprobe with libx264"]
fn real_ffmpeg_clip_inspected_by_production_native_helper() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("CFR ü space.mp4");
    let output = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=64x48:r=30000/1001",
            "-frames:v",
            "3",
            "-c:v",
            "libx264",
            "-threads",
            "1",
            "-pix_fmt",
            "yuv420p",
            "-x264-params",
            "colorprim=bt709:transfer=bt709:colormatrix=bt709",
        ])
        .arg(&path)
        .output()
        .expect("installed ffmpeg is required for this native smoke test");
    assert!(output.status.success(), "ffmpeg fixture generation failed");
    let before = fs::read(&path).unwrap();
    let inspected = inspect_source(&path).unwrap();
    assert_eq!(inspected.name, "CFR ü space.mp4");
    assert_eq!((inspected.width, inspected.height), (64, 48));
    assert_eq!(
        (inspected.fps_numerator, inspected.fps_denominator),
        (30000, 1001)
    );
    assert!(
        inspected
            .duration_seconds
            .is_some_and(|d| d > 0.09 && d < 0.2)
    );
    assert!(inspected.blockers.is_empty(), "{:?}", inspected.blockers);
    assert!(!inspected.processing_ready && !inspected.timing_verified);
    // The Windows exit-status race failed about two in three inspections; one pass proves little.
    for _ in 0..20 {
        inspect_source(&path).unwrap();
    }
    assert_eq!(before, fs::read(&path).unwrap());
    println!(
        "Real ffmpeg/ffprobe inspection: {}",
        serde_json::to_string(&inspected).unwrap()
    );
}

#[cfg(target_os = "linux")]
#[test]
fn forced_worker_death_also_terminates_initial_source_probe() {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    const READY: &str = "DEAS_TEST_SOURCE_PROBE_DEATH_READY";
    if let Some(ready) = std::env::var_os(READY) {
        let mut command = Command::new("sleep");
        command.arg("60");
        let probe = OwnedProbe::spawn(command).unwrap();
        fs::write(ready, probe.child.id().to_string()).unwrap();
        loop {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let ready = dir.path().join("probe-pid");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "video_post::tests::forced_worker_death_also_terminates_initial_source_probe",
        ])
        .env(READY, &ready);
    let mut worker = OwnedProbe::spawn(command).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let pid: i32 = fs::read_to_string(ready).unwrap().parse().unwrap();
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
    assert!(raw >= 0);
    let pidfd = unsafe { OwnedFd::from_raw_fd(raw) };
    worker.child.kill().unwrap();
    drop(worker);
    let mut poll = libc::pollfd {
        fd: pidfd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let dead = unsafe { libc::poll(&mut poll, 1, 3000) };
    if dead <= 0 {
        // Reap the exact fixture if the regression fails; never signal a reusable numeric PID.
        unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                pidfd.as_raw_fd(),
                libc::SIGKILL,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            );
        }
    }
    assert!(
        dead > 0,
        "initial source probe survived forced worker death"
    );
}

// Real child fixtures use this test binary, not a shell, Python or a PATH override.
#[test]
fn probe_child_fixture() {
    let Ok(mode) = std::env::var("DEAS_VIDEO_POST_TEST_CHILD") else {
        return;
    };
    match mode.as_str() {
        "timeout" => std::thread::sleep(Duration::from_secs(30)),
        "stdout" => {
            use std::io::Write;
            let _ = std::io::stdout().write_all(&vec![b'x'; OUTPUT_LIMIT + 1]);
        }
        "stderr" => {
            use std::io::Write;
            let _ = std::io::stderr().write_all(&vec![b'x'; STDERR_LIMIT + 1]);
        }
        "failure" => {
            eprintln!("secret /private/path token=credential");
            std::process::exit(7);
        }
        _ => panic!("unknown fixture"),
    }
}
#[test]
fn native_probe_bounds_timeout_output_stderr_and_failed_exit() {
    for (mode, expected) in [
        ("timeout", "VIDEO_PROBE_TIMEOUT:"),
        ("stdout", "VIDEO_PROBE_LIMIT:"),
        ("stderr", "VIDEO_PROBE_LIMIT:"),
        ("failure", "VIDEO_PROBE_FAILED:"),
    ] {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "video_post::tests::probe_child_fixture",
                "--nocapture",
            ])
            .env("DEAS_VIDEO_POST_TEST_CHILD", mode);
        let start = Instant::now();
        let error = run_probe(command, Duration::from_millis(250)).unwrap_err();
        assert!(error.starts_with(expected), "{mode}: {error}");
        assert!(
            !error.contains("secret")
                && !error.contains("/private")
                && !error.contains("credential")
        );
        assert!(start.elapsed() < Duration::from_secs(3));
    }
    let dir = tempfile::tempdir().unwrap();
    let command = Command::new(dir.path().join("not-installed-ffprobe"));
    let error = run_probe(command, Duration::from_secs(1)).unwrap_err();
    assert!(error.starts_with("VIDEO_TOOL_MISSING:") && error.contains("PATH"));
    assert!(!error.contains(dir.path().to_str().unwrap()));
}
