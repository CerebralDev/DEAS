use super::*;

fn request() -> Request {
    serde_json::from_value(json!({"sourceId":uuid::Uuid::new_v4().to_string(),
        "mode":"clean","profile":"mix","title":"Save enhanced audio","confirmed":true}))
    .unwrap()
}

#[test]
fn audio_request_boundary_and_runtime_gates() {
    let mut r = request();
    assert!(r.validate().is_ok());
    r.confirmed = false;
    assert!(r.validate().unwrap_err().starts_with("AUDIO_CONSENT:"));
    r.confirmed = true;
    // Device selection: stems only, checked values, absent on legacy payloads.
    r.device = Some("cuda".into());
    assert!(r.validate().unwrap_err().starts_with("AUDIO_DEVICE:"));
    r.mode = "stems".into();
    r.profile = "mix".into();
    r.device = Some("gpu".into());
    assert!(r.validate().unwrap_err().starts_with("AUDIO_DEVICE:"));
    for device in ["auto", "cpu", "cuda"] {
        r.device = Some(device.into());
        assert_eq!(
            r.validate().is_ok(),
            crate::audio_runtime::available("stems"),
            "{device}"
        );
    }
    r.device = None;
    r.mode = "deep".into();
    assert!(r.validate().unwrap_err().starts_with("AUDIO_PROFILE:"));
    for mode in ["deep", "stems"] {
        r.mode = mode.into();
        r.profile = "speech".into();
        assert_eq!(r.validate().is_ok(), crate::audio_runtime::available(mode));
    }
    r.mode = "clean".into();
    r.job_id = Some(uuid::Uuid::new_v4().to_string());
    r.index = Some(0);
    assert!(r.validate().is_err());
    r.source_id = None;
    assert!(r.validate().is_ok());
    r.index = None;
    assert!(r.validate().is_err());
    r.index = Some(0);
    r.profile = "unknown".into();
    assert!(r.validate().is_err());
    assert!(serde_json::from_value::<Request>(json!({"path":"/private.wav"})).is_err());
}

#[test]
fn audio_commands_registered_and_logged_without_paths() {
    for name in [
        "native_audio_enhance_capabilities",
        "native_audio_enhance_export",
        "native_audio_enhance_cancel",
        "native_audio_enhance_install",
        "native_audio_enhance_install_cancel",
        "native_audio_enhance_install_status",
    ] {
        assert!(include_str!("main.rs").contains(&format!("deas::{name},")));
        assert!(include_str!("../permissions/video_post.toml").contains(&format!("\"{name}\"")));
    }
    assert!(include_str!("../capabilities/main.json").contains("allow-native-audio-enhance"));
    let id = uuid::Uuid::new_v4();
    log(id, "AUDIO_PROCESS", "ERROR");
    let entries = crate::logs::after(0);
    assert!(entries.iter().any(|entry| {
        let text = serde_json::to_string(entry).unwrap();
        text.contains(&id.to_string()) && text.contains("audio-enhance") && text.contains("ERROR")
    }));
}

fn fixture(path: &Path, video: bool, silent: bool) {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-v", "error", "-nostdin"]);
    if video {
        cmd.args(["-f", "lavfi", "-i", "testsrc2=size=64x48:rate=10"]);
    }
    if !silent {
        cmd.args(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000"]);
    }
    cmd.args(["-t", "0.8"]);
    if video {
        cmd.args(["-c:v", "mpeg4"]);
    }
    if !silent {
        cmd.args(["-c:a", "flac"]);
    }
    assert!(cmd.arg(path).status().unwrap().success());
}

fn audio_pcm(path: &Path) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-f", "s16le", "pipe:1"])
        .output()
        .unwrap();
    assert!(out.status.success());
    out.stdout
}

fn packet_hashes(path: &Path) -> Vec<String> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_packets",
            "-show_data_hash",
            "sha256",
            "-show_entries",
            "packet=data_hash",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .unwrap();
    assert!(out.status.success());
    serde_json::from_slice::<Value>(&out.stdout).unwrap()["packets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["data_hash"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
#[ignore = "requires real FFmpeg/FFprobe on PATH; run explicitly"]
fn audio_real_ffmpeg_clean_remux_and_failure_preservation() {
    assert!(
        capabilities().clean,
        "FFmpeg DSP filters, FLAC encoder and FFprobe required"
    );
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("音 source.mkv");
    fixture(&source, true, false);
    let original = std::fs::read(&source).unwrap();
    let target = dir.path().join("enhanced.mkv");
    let cancel = AtomicBool::new(false);
    let result = process(
        Input::open(source.clone()).unwrap(),
        &target,
        "mix",
        &cancel,
    )
    .unwrap();
    assert_eq!(result["size"], std::fs::metadata(&target).unwrap().len());
    assert_eq!(
        result["sha256"],
        format!("{:x}", Sha256::digest(std::fs::read(&target).unwrap()))
    );
    let before = packet_hashes(&source);
    assert!(!before.is_empty());
    assert_eq!(
        before,
        packet_hashes(&target),
        "video packets must be byte-identical"
    );
    assert_eq!(std::fs::read(&source).unwrap(), original);
    let raw_pcm = audio_pcm(&source);
    let clean_pcm = audio_pcm(&target);
    assert_eq!(
        raw_pcm.len(),
        clean_pcm.len(),
        "preserve decoded sample count"
    );
    assert_ne!(raw_pcm, clean_pcm, "DSP must really modify the audio");
    let broken = dir.path().join("truncated.mkv");
    let mut truncated = std::fs::read(&target).unwrap();
    truncated.truncate(truncated.len() / 2);
    std::fs::write(&broken, truncated).unwrap();
    assert!(validate_product(&Input::open(source.clone()).unwrap(), &broken, &cancel).is_err());
    let published = std::fs::read(&target).unwrap();
    assert!(process(
        Input::open(source.clone()).unwrap(),
        &target,
        "mix",
        &cancel
    )
    .is_err());
    assert_eq!(std::fs::read(&target).unwrap(), published, "no clobber");
    let aborted = dir.path().join("cancelled.mkv");
    cancel.store(true, Ordering::Relaxed);
    assert!(process(
        Input::open(source.clone()).unwrap(),
        &aborted,
        "mix",
        &cancel
    )
    .unwrap_err()
    .starts_with("AUDIO_CANCELLED:"));
    assert!(!aborted.exists());
    let changed = Input::open(source.clone()).unwrap();
    std::fs::write(&source, b"changed source after inspection").unwrap();
    let error = process_retained(
        changed,
        &aborted,
        &AudioSettings {
            mode: "clean".into(),
            profile: "mix".into(),
            device: None,
        },
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(error.starts_with("AUDIO_CHANGED:"), "{error}");
    assert!(!aborted.exists());
    let audio = dir.path().join("voice.flac");
    fixture(&audio, false, false);
    let clean = dir.path().join("voice-clean.flac");
    process(
        Input::open(audio).unwrap(),
        &clean,
        "speech",
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(std::fs::metadata(clean).unwrap().len() > 12);
    let silent = dir.path().join("no-audio.mkv");
    fixture(&silent, true, true);
    assert!(Input::open(silent)
        .err()
        .unwrap()
        .starts_with("AUDIO_NO_STREAM:"));
    assert!(!std::fs::read_dir(dir.path()).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".deas-audio-")));
    eprintln!("real FFmpeg: clean mix MKV, speech FLAC, identical video packet hashes, source preservation, no-clobber, cancellation, mutation, missing-audio passed");
}

#[test]
#[ignore = "explicit network/model installation and CPU inference; set DEAS_AUDIO_RUNTIME_DIR to an approved artifact directory"]
fn audio_real_install_deep_stems_cpu() {
    assert!(
        std::env::var_os("DEAS_AUDIO_RUNTIME_DIR").is_some(),
        "Explicit artifact root required"
    );
    tauri::async_runtime::block_on(crate::audio_runtime::install_requested(
        "stems".into(),
        true,
        Some(true),
        None,
    ))
    .unwrap();
    let dir = tempfile::Builder::new()
        .prefix("acceptance-")
        .tempdir_in(crate::audio_runtime::root())
        .unwrap();
    let source = dir.path().join("source.mkv");
    fixture(&source, true, false);
    let source_bytes = std::fs::read(&source).unwrap();
    for (mode, profile) in [("deep", "speech"), ("stems", "mix"), ("stems", "speech")] {
        let settings = AudioSettings {
            mode: mode.into(),
            profile: profile.into(),
            device: None,
        };
        let target = dir.path().join(format!("{mode}-{profile}.mkv"));
        let started = std::time::Instant::now();
        let receipt = process_file(&source, &target, &settings, &AtomicBool::new(false)).unwrap();
        assert_eq!(receipt["size"], std::fs::metadata(&target).unwrap().len());
        assert_eq!(audio_pcm(&source).len(), audio_pcm(&target).len());
        assert_ne!(audio_pcm(&source), audio_pcm(&target));
        assert_eq!(packet_hashes(&source), packet_hashes(&target));
        assert!(process_file(&source, &target, &settings, &AtomicBool::new(false)).is_err());
        let aborted = dir.path().join(format!("{mode}-{profile}-cancelled.mkv"));
        assert!(
            process_file(&source, &aborted, &settings, &AtomicBool::new(true))
                .unwrap_err()
                .starts_with("AUDIO_CANCELLED:")
        );
        assert!(!aborted.exists());
        eprintln!(
            "real {mode}/{profile} CPU: {:?}; samples/video packets preserved",
            started.elapsed()
        );
    }
    assert_eq!(std::fs::read(&source).unwrap(), source_bytes);
    let noisy = crate::audio_runtime::root()
        .parent()
        .unwrap()
        .join("acceptance/noisy_snr0.wav");
    if noisy.is_file() {
        let target = noisy
            .parent()
            .unwrap()
            .join(format!("deep-{}.flac", uuid::Uuid::new_v4()));
        process_file(
            &noisy,
            &target,
            &AudioSettings {
                mode: "deep".into(),
                profile: "speech".into(),
                device: None,
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(audio_pcm(&noisy).len(), audio_pcm(&target).len());
        assert_ne!(audio_pcm(&noisy), audio_pcm(&target));
        eprintln!("upstream noisy speech result: {}", target.display());
    }
}

#[test]
#[ignore = "requires FFmpeg/FFprobe; verifies manual cancellation cannot poison automatic processing"]
fn audio_real_manual_cancel_does_not_poison_automatic_token() {
    let dir = tempfile::Builder::new()
        .prefix("cancel-isolation-")
        .tempdir_in(crate::audio_runtime::root())
        .unwrap();
    let source = dir.path().join("source.flac");
    fixture(&source, false, false);
    let target = dir.path().join("automatic.flac");
    native_audio_enhance_cancel();
    let result = process_file(
        &source,
        &target,
        &AudioSettings {
            mode: "clean".into(),
            profile: "mix".into(),
            device: None,
        },
        &AtomicBool::new(false),
    );
    CANCEL.store(false, Ordering::Relaxed);
    result.unwrap();
    assert_eq!(audio_pcm(&source).len(), audio_pcm(&target).len());
    assert_ne!(audio_pcm(&source), audio_pcm(&target));
}

#[test]
#[ignore = "requires real FFmpeg/FFprobe on PATH; run explicitly"]
fn audio_real_owned_runner_cancellation() {
    let cancel = AtomicBool::new(false);
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-hide_banner",
        "-v",
        "error",
        "-nostdin",
        "-re",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440",
        "-t",
        "60",
        "-f",
        "null",
        "-",
    ]);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(150));
            cancel.store(true, Ordering::Relaxed);
        });
        let started = std::time::Instant::now();
        assert!(run_tool(cmd, Duration::from_secs(10), &cancel, None)
            .unwrap_err()
            .starts_with("AUDIO_CANCELLED:"));
        assert!(started.elapsed() < Duration::from_secs(5));
    });
}
