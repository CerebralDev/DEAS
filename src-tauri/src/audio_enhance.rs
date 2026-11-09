//! Explicit, non-destructive audio post-export shared by TVE and terminal Studio outputs.
use crate::video_post::{identity, open_source, run_probe, run_tool, Identity};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AudioSettings {
    pub mode: String,
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

pub(crate) fn validate_settings(settings: &AudioSettings) -> Result<(), String> {
    if !matches!(settings.profile.as_str(), "mix" | "speech") {
        return Err("AUDIO_PROFILE: Select mix or speech".into());
    }
    if let Some(device) = &settings.device {
        if !matches!(device.as_str(), "auto" | "cpu" | "cuda") {
            return Err("AUDIO_DEVICE: Select auto, cpu or cuda".into());
        }
        if settings.mode != "stems" {
            return Err("AUDIO_DEVICE: Device selection applies to stems only".into());
        }
    }
    // A missing device on older saved stems settings resolves to auto at execution time.
    match settings.mode.as_str() {
        "off" | "clean" => Ok(()),
        "deep" if settings.profile != "speech" => {
            Err("AUDIO_PROFILE: DeepFilterNet is for speech, not a complete music mix".into())
        }
        "deep" | "stems" if crate::audio_runtime::available(&settings.mode) => Ok(()),
        "deep" | "stems" => {
            Err("AUDIO_UNAVAILABLE: Explicitly install the selected audio engine first".into())
        }
        _ => Err("AUDIO_MODE: Select clean, deep or stems".into()),
    }
}

pub(crate) fn process_file(
    input: &Path,
    target: &Path,
    settings: &AudioSettings,
    cancel: &AtomicBool,
) -> Result<Value, String> {
    process_retained(
        Input::open_with_cancel(input.to_owned(), cancel)?,
        target,
        settings,
        cancel,
    )
}

fn process_retained(
    input: Input,
    target: &Path,
    settings: &AudioSettings,
    cancel: &AtomicBool,
) -> Result<Value, String> {
    validate_settings(settings)?;
    if settings.mode == "off" {
        return Err("AUDIO_MODE: Off does not process media".into());
    }
    static PROCESSING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        match PROCESSING.try_lock() {
            Ok(guard) => break guard,
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(_) => return Err("AUDIO_PROCESS: Audio coordinator unavailable".into()),
        }
    };
    // Shared native compute gate: admissions and inference run while no other compute
    // stage or owned-backend release request can interleave. Stop/cancel never waits on it.
    let _compute = loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        match crate::resources::reserve_compute() {
            Ok(permit) => break permit,
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    };
    process_mode(input, target, settings, cancel)
}

static EXPORTING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
static CANCEL: AtomicBool = AtomicBool::new(false);
const CANCELLED: &str = "AUDIO_CANCELLED: Audio enhancement cancelled; original preserved.";
const MIX: &str = "loudnorm=I=-16:TP=-1.5:LRA=11";
const SPEECH: &str = "highpass=f=80,afftdn=nr=10:nf=-35,deesser=i=0.2,dynaudnorm=f=150:g=15,loudnorm=I=-16:TP=-1.5:LRA=11";

/// Enhanced video keeps the source's audio encode settings (the TVE/video-combine node's
/// codec, plus bitrate where the container reports one); unknown codecs and audio-only
/// masters fall back to lossless FLAC.
/// shortcut: Matroska carries no per-stream bitrate, so MKV sources keep the codec only and
/// fall to encoder default quality; add an explicit bitrate to the export intent if needed.
fn audio_encoder(codec: &str, video: bool) -> (&'static str, &'static str) {
    match if video { codec } else { "" } {
        "aac" => ("aac", "aac"),
        "opus" | "libopus" => ("libopus", "opus"),
        "vorbis" | "libvorbis" => ("libvorbis", "vorbis"),
        "mp3" | "libmp3lame" => ("libmp3lame", "mp3"),
        _ => ("flac", "flac"),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    source_id: Option<String>,
    job_id: Option<String>,
    index: Option<usize>,
    mode: String,
    profile: String,
    #[serde(default)]
    device: Option<String>,
    title: String,
    confirmed: bool,
}
impl Request {
    fn validate(&self) -> Result<(), String> {
        crate::video_post::validate_title(&self.title)?;
        if !self.confirmed {
            return Err("AUDIO_CONSENT: Explicit export confirmation is required".into());
        }
        if !matches!(self.profile.as_str(), "mix" | "speech") {
            return Err("AUDIO_PROFILE: Select mix or speech".into());
        }
        match self.mode.as_str() {
            "clean" | "deep" | "stems" => validate_settings(&AudioSettings {
                mode: self.mode.clone(),
                profile: self.profile.clone(),
                device: self.device.clone(),
            })?,
            _ => {
                return Err(
                    "AUDIO_MODE: Select clean, deep or stems; off does not process media".into(),
                )
            }
        }
        match (&self.source_id, &self.job_id, self.index) {
            (Some(id), None, None) | (None, Some(id), Some(_)) => {
                crate::video_post_source::strict_id(id)
            }
            _ => Err(
                "AUDIO_SOURCE: Choose exactly one retained sourceId or terminal jobId with index"
                    .into(),
            ),
        }
    }
}

#[derive(Serialize)]
pub struct Capabilities {
    clean: bool,
    deep: bool,
    stems: bool,
    cuda: bool,
    /// Stems installed CPU-only while an owned environment could lend CUDA Torch.
    #[serde(rename = "gpuRepair")]
    gpu_repair: bool,
}

pub(crate) fn capabilities() -> Capabilities {
    let mut filters = Command::new("ffmpeg");
    filters.args(["-hide_banner", "-v", "error", "-filters"]);
    let mut encoders = Command::new("ffmpeg");
    encoders.args(["-hide_banner", "-v", "error", "-encoders"]);
    let mut probe = Command::new("ffprobe");
    probe.args(["-v", "error", "-version"]);
    let clean = run_probe(filters, Duration::from_secs(10)).is_ok_and(|bytes| {
        let text = String::from_utf8_lossy(&bytes);
        ["highpass", "afftdn", "deesser", "dynaudnorm", "loudnorm"]
            .iter()
            .all(|name| text.split_whitespace().any(|word| word == *name))
    }) && run_probe(encoders, Duration::from_secs(10)).is_ok_and(|bytes| {
        String::from_utf8_lossy(&bytes)
            .split_whitespace()
            .any(|word| word == "flac")
    }) && run_probe(probe, Duration::from_secs(10)).is_ok();
    Capabilities {
        clean,
        deep: clean && crate::audio_runtime::available("deep"),
        stems: clean && crate::audio_runtime::available("stems"),
        cuda: clean && crate::audio_runtime::gpu_ready(),
        gpu_repair: clean && crate::audio_runtime::gpu_repairable(),
    }
}

#[tauri::command]
pub async fn native_audio_enhance_capabilities() -> Result<Capabilities, String> {
    tauri::async_runtime::spawn_blocking(capabilities)
        .await
        .map_err(|_| "AUDIO_PROBE: Capability probe failed".into())
}

fn log(id: uuid::Uuid, outcome: &str, severity: &str) {
    crate::logs::push(
        crate::logs::Source::Deas,
        &format!("{severity}: audio-enhance operation=export operationId={id} outcome={outcome}"),
    );
}

#[tauri::command]
pub fn native_audio_enhance_cancel() {
    CANCEL.store(true, Ordering::Relaxed);
}

pub fn shutdown() {
    native_audio_enhance_cancel();
    crate::audio_runtime::native_audio_enhance_install_cancel();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while EXPORTING.available_permits() == 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Input {
    path: PathBuf,
    file: File,
    before: Identity,
    demuxer: &'static str,
    video: bool,
    streams: Vec<(String, String, u32, u32, Option<u64>)>,
    audio_offsets: Vec<f64>,
}
impl Input {
    #[cfg(test)]
    fn open(path: PathBuf) -> Result<Self, String> {
        Self::open_with_cancel(path, &AtomicBool::new(false))
    }
    fn open_with_cancel(path: PathBuf, cancel: &AtomicBool) -> Result<Self, String> {
        let mut file = open_source(&path)?;
        let before = identity(&file)?;
        let mut header = [0u8; 12];
        file.read_exact(&mut header)
            .map_err(|_| "AUDIO_SOURCE: Cannot read media header")?;
        let demuxer = if &header[4..8] == b"ftyp" {
            "mov"
        } else if header[..4] == [0x1a, 0x45, 0xdf, 0xa3] {
            "matroska"
        } else if &header[..4] == b"fLaC" {
            "flac"
        } else if &header[..4] == b"RIFF" && &header[8..] == b"WAVE" {
            "wav"
        } else if &header[..4] == b"OggS" {
            "ogg"
        } else if &header[..3] == b"ID3" || (header[0] == 0xff && header[1] & 0xe0 == 0xe0) {
            "mp3"
        } else {
            return Err(
                "AUDIO_SOURCE: Supported containers are MP4, MKV, FLAC, WAV, Ogg and MP3".into(),
            );
        };
        let mut input = Self {
            path,
            file,
            before,
            demuxer,
            video: false,
            streams: Vec::new(),
            audio_offsets: Vec::new(),
        };
        let mut probe = Command::new("ffprobe");
        probe.args(["-v", "error"]);
        input.arguments(&mut probe);
        probe
            .args([
                "-show_entries",
                "stream=codec_type,codec_name,sample_rate,channels,start_time,bit_rate:stream_tags:format=duration:format_tags",
                "-of",
                "json",
                "-i",
            ])
            .arg(&input.path);
        let bytes = run_tool(probe, Duration::from_secs(10), cancel, None).map_err(tool_error)?;
        let parsed: Value =
            serde_json::from_slice(&bytes).map_err(|_| "AUDIO_SOURCE: Invalid stream metadata")?;
        let streams = parsed["streams"]
            .as_array()
            .filter(|s| s.len() <= 256)
            .ok_or("AUDIO_SOURCE: Missing stream metadata")?;
        let count = |kind| streams.iter().filter(|s| s["codec_type"] == kind).count();
        if count("audio") == 0 {
            return Err("AUDIO_NO_STREAM: This source has no audio stream".into());
        }
        if count("video") > 1 {
            return Err("AUDIO_SOURCE: Multiple video streams are not supported".into());
        }
        input.video = count("video") == 1;
        if !input.video && count("audio") != 1 {
            return Err("AUDIO_SOURCE: Audio-only export requires exactly one audio stream".into());
        }
        input.streams = streams
            .iter()
            .map(|stream| {
                let kind = stream["codec_type"]
                    .as_str()
                    .ok_or("AUDIO_SOURCE: Missing stream type")?;
                let codec = stream["codec_name"]
                    .as_str()
                    .ok_or("AUDIO_SOURCE: Missing stream codec")?;
                let rate = stream["sample_rate"]
                    .as_str()
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0);
                let channels = stream["channels"].as_u64().unwrap_or(0) as u32;
                if kind == "audio"
                    && (!(8000..=384000).contains(&rate) || !(1..=64).contains(&channels))
                {
                    return Err("AUDIO_SOURCE: Unsupported audio rate/channel count");
                }
                let bitrate = stream["bit_rate"]
                    .as_str()
                    .and_then(|s| s.parse::<u64>().ok());
                Ok((kind.to_owned(), codec.to_owned(), rate, channels, bitrate))
            })
            .collect::<Result<_, &str>>()?;
        input.audio_offsets = streams
            .iter()
            .filter(|s| s["codec_type"] == "audio")
            .map(|s| {
                s["start_time"]
                    .as_str()
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite())
                    .unwrap_or(0.0)
            })
            .collect();
        input.recheck()?;
        Ok(input)
    }
    fn arguments(&self, command: &mut Command) {
        command.args([
            "-protocol_whitelist",
            "file",
            "-f",
            self.demuxer,
            "-probesize",
            "4194304",
            "-analyzeduration",
            "1000000",
            "-max_alloc",
            "67108864",
        ]);
        if self.demuxer == "mov" {
            command.args(["-enable_drefs", "0", "-use_absolute_path", "0"]);
        }
    }
    fn recheck(&self) -> Result<(), String> {
        if identity(&self.file)? != self.before
            || identity(&open_source(&self.path)?)? != self.before
        {
            return Err("AUDIO_CHANGED: Source changed; select it again".into());
        }
        Ok(())
    }
    /// Video keeps the source's container (the TVE/video-combine node's choice), which also
    /// drives the save-dialog filter and the target-extension guard; audio-only masters stay FLAC.
    fn muxer(&self) -> (&'static str, &'static str) {
        if !self.video {
            return ("flac", "flac");
        }
        match self
            .path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "mp4" => ("mp4", "mp4"),
            "mov" => ("mov", "mov"),
            "webm" => ("webm", "webm"),
            _ => ("matroska", "mkv"),
        }
    }
    fn metadata_arguments(&self, command: &mut Command) {
        // Learned tracks come from extra inputs; provenance and track labels belong to input 0.
        command.args(["-map_metadata", "0"]);
        let mut output = 0;
        for (index, stream) in self.streams.iter().enumerate() {
            if self.video || stream.0 == "audio" {
                command.args([format!("-map_metadata:s:{output}"), format!("0:s:{index}")]);
                output += 1;
            }
        }
        if matches!(self.muxer().0, "mp4" | "mov") {
            command.args(["-movflags", "+use_metadata_tags"]);
        }
    }
    fn extension(&self) -> &'static str {
        self.muxer().1
    }
}

fn tool_error(error: String) -> String {
    if error.starts_with("AUDIO_CANCELLED:") {
        error
    } else {
        format!("AUDIO_PROCESS: FFmpeg/FFprobe failed ({}); install compatible tools on PATH and check source/codecs. No output was published.", error.split(':').next().unwrap_or("tool"))
    }
}

fn validate_product(input: &Input, path: &Path, cancel: &AtomicBool) -> Result<(), String> {
    let output = Input::open_with_cancel(path.to_owned(), cancel)?;
    let expected: Vec<_> = input
        .streams
        .iter()
        .filter(|s| input.video || s.0 == "audio")
        .collect();
    if output.streams.len() != expected.len()
        || output
            .streams
            .iter()
            .zip(expected)
            .any(|(actual, expected)| {
                actual.0 != expected.0
                    || if actual.0 == "audio" {
                        actual.1 != audio_encoder(&expected.1, input.video).1
                            || actual.2 != expected.2
                            || actual.3 != expected.3
                    } else {
                        actual.1 != expected.1
                    }
            })
    {
        return Err(
            "AUDIO_VERIFY: Output track types, codecs, audio rate or channels changed unexpectedly"
                .into(),
        );
    }
    let source_duration = decoded_duration(input, cancel)?;
    let duration = decoded_duration(&output, cancel)?;
    if (duration - source_duration).abs() > 0.05_f64.max(source_duration * 0.0001) {
        return Err(
            "AUDIO_VERIFY: Decoded audio duration differs from the source; export refused".into(),
        );
    }
    output.recheck()?;
    Ok(())
}

fn decoded_duration(input: &Input, cancel: &AtomicBool) -> Result<f64, String> {
    audio_duration(input, None, cancel)
}
fn audio_duration(input: &Input, track: Option<usize>, cancel: &AtomicBool) -> Result<f64, String> {
    // Streaming FLAC has no seek-patched duration; decode audio and check actual end time.
    let mut decode = Command::new("ffmpeg");
    decode.args(["-hide_banner", "-v", "error", "-nostdin", "-xerror"]);
    input.arguments(&mut decode);
    decode.arg("-i").arg(&input.path).args([
        "-map",
        &track
            .map(|i| format!("0:a:{i}"))
            .unwrap_or_else(|| "0:a".into()),
        "-af",
        "asetpts=N/SR/TB",
        "-stats_period",
        "2147483647",
        "-progress",
        "pipe:1",
        "-f",
        "null",
        "-",
    ]);
    let bytes =
        run_tool(decode, Duration::from_secs(6 * 60 * 60), cancel, None).map_err(tool_error)?;
    let duration = String::from_utf8_lossy(&bytes)
        .lines()
        .filter_map(|line| line.strip_prefix("out_time_us="))
        .filter_map(|value| value.parse::<f64>().ok())
        .next_back()
        .map(|n| n / 1_000_000.0)
        .filter(|n| n.is_finite() && *n > 0.0)
        .ok_or("AUDIO_VERIFY: Output has no decodable audio duration")?;
    input.recheck()?;
    Ok(duration)
}

#[cfg(test)]
fn process(
    input: Input,
    target: &Path,
    profile: &str,
    cancel: &AtomicBool,
) -> Result<Value, String> {
    process_mode(
        input,
        target,
        &AudioSettings {
            mode: "clean".into(),
            profile: profile.into(),
            device: None,
        },
        cancel,
    )
}

fn process_mode(
    input: Input,
    target: &Path,
    settings: &AudioSettings,
    cancel: &AtomicBool,
) -> Result<Value, String> {
    use crate::downloads::fs::Anchor;
    input.recheck()?;
    if cancel.load(Ordering::Relaxed) {
        return Err(CANCELLED.into());
    }
    if !target.is_absolute()
        || target.extension().and_then(|s| s.to_str()) != Some(input.extension())
        || target.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        })
    {
        return Err(format!(
            "AUDIO_TARGET: Select a new absolute .{} file",
            input.extension()
        ));
    }
    let name = target
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.chars().any(char::is_control))
        .ok_or("AUDIO_TARGET: Invalid output filename")?;
    let parent = Anchor::open(target.parent().ok_or("AUDIO_TARGET: Missing parent")?)?;
    parent.validate_directory(false)?;
    parent.absent(name)?;
    let temp = format!(".deas-audio-{}.partial", uuid::Uuid::new_v4());
    let file = parent.partial(&temp)?;
    let result = (|| {
        let work = tempfile::Builder::new()
            .prefix(".deas-audio-work-")
            .tempdir_in(target.parent().ok_or("AUDIO_TARGET: Missing parent")?)
            .map_err(|_| "AUDIO_WRITE: Cannot create private audio workspace")?;
        let mut processed = Vec::new();
        if settings.mode != "clean" {
            let duration = decoded_duration(&input, cancel)?;
            crate::audio_runtime::check_memory(
                duration,
                settings.mode.as_str(),
                input.streams.iter().map(|s| s.3).max().unwrap_or(1),
                cancel,
            )?;
            for (index, stream) in input.streams.iter().filter(|s| s.0 == "audio").enumerate() {
                if settings.mode == "stems" && stream.3 > 2 {
                    return Err("AUDIO_CHANNELS: Smule Windowed RoFormer supports mono/stereo, not surround; original preserved".into());
                }
                let duration = audio_duration(&input, Some(index), cancel)?;
                let dir = work.path().join(index.to_string());
                std::fs::create_dir(&dir)
                    .map_err(|_| "AUDIO_WRITE: Cannot create track workspace")?;
                let wav = dir.join("track.wav");
                let mut decode = Command::new("ffmpeg");
                decode.args(["-v", "error", "-nostdin"]);
                input.arguments(&mut decode);
                decode
                    .arg("-i")
                    .arg(&input.path)
                    .args([
                        "-map",
                        &format!("0:a:{index}"),
                        "-ar",
                        if settings.mode == "deep" {
                            "48000"
                        } else {
                            "44100"
                        },
                        "-ac",
                        &stream.3.to_string(),
                        "-c:a",
                        "pcm_s16le",
                    ])
                    .arg(&wav);
                run_tool(decode, Duration::from_secs(6 * 60 * 60), cancel, None)
                    .map_err(tool_error)?;
                let enhanced = crate::audio_runtime::enhance(
                    &wav,
                    &dir,
                    &settings.mode,
                    &settings.profile,
                    settings.device.as_deref(),
                    cancel,
                )?;
                let actual = Input::open_with_cancel(enhanced.clone(), cancel)?;
                if (decoded_duration(&actual, cancel)? - duration).abs() > 0.05 {
                    return Err(
                        "AUDIO_VERIFY: Engine changed decoded duration; original preserved".into(),
                    );
                }
                processed.push(enhanced);
            }
        }
        let mut command = Command::new("ffmpeg");
        command.args(["-hide_banner", "-v", "error", "-nostdin"]);
        input.arguments(&mut command);
        command.args(["-i"]).arg(&input.path);
        for path in &processed {
            command.arg("-i").arg(path);
        }
        // Map every track for video; fail rather than silently dropping unsupported streams.
        if processed.is_empty() {
            command.args(["-map", if input.video { "0" } else { "0:a:0" }]);
        } else {
            let mut audio = 0;
            for (index, stream) in input.streams.iter().enumerate() {
                if stream.0 == "audio" {
                    audio += 1;
                    command.args(["-map", &format!("{audio}:a:0")]);
                } else if input.video {
                    command.args(["-map", &format!("0:{index}")]);
                }
            }
        }
        input.metadata_arguments(&mut command);
        command.args([
            "-c",
            "copy",
            "-filter:a",
            if settings.mode == "clean" && settings.profile == "speech" {
                SPEECH
            } else {
                MIX
            },
            "-filter_threads",
            "1",
            "-threads",
            "1",
        ]);
        // loudnorm works internally at 192 kHz; restore each source track's rate explicitly.
        for (index, stream) in input.streams.iter().filter(|s| s.0 == "audio").enumerate() {
            let (encoder, _) = audio_encoder(&stream.1, input.video);
            command.args([
                format!("-c:a:{index}"),
                encoder.to_owned(),
                format!("-ar:a:{index}"),
                stream.2.to_string(),
                format!("-ac:a:{index}"),
                stream.3.to_string(),
            ]);
            if encoder != "flac" {
                if let Some(bitrate) = stream.4 {
                    command.args([format!("-b:a:{index}"), bitrate.to_string()]);
                }
            }
            if !processed.is_empty() {
                let duration = audio_duration(&input, Some(index), cancel)?;
                let samples = (duration * f64::from(stream.2)).round() as u64;
                command.args([
                    format!("-filter:a:{index}"),
                    format!(
                        "{MIX},aresample={},apad,atrim=end_sample={samples},asetpts=PTS+{}/TB",
                        stream.2, input.audio_offsets[index]
                    ),
                ]);
            }
        }
        let (muxer, _) = input.muxer();
        command.args(["-f", muxer]);
        if matches!(muxer, "mp4" | "mov") {
            // MP4/MOV writers must seek back to patch their index, which a pipe cannot take:
            // encode to the private workspace, then stream the finished bytes into the anchor.
            let staged = work.path().join(format!("container.{muxer}"));
            command.arg(&staged);
            run_tool(command, Duration::from_secs(6 * 60 * 60), cancel, None)
                .map_err(tool_error)?;
            if cancel.load(Ordering::Relaxed) {
                return Err(CANCELLED.into());
            }
            std::io::copy(
                &mut File::open(&staged)
                    .map_err(|_| "AUDIO_WRITE: Cannot read staged container")?,
                &mut file
                    .try_clone()
                    .map_err(|_| "AUDIO_WRITE: Cannot open output stream")?,
            )
            .map_err(|_| "AUDIO_WRITE: Cannot stream enhanced output")?;
        } else {
            command.arg("pipe:1");
            run_tool(
                command,
                Duration::from_secs(6 * 60 * 60),
                cancel,
                Some(
                    file.try_clone()
                        .map_err(|_| "AUDIO_WRITE: Cannot open output stream")?,
                ),
            )
            .map_err(tool_error)?;
        }
        input.recheck()?;
        file.sync_all()
            .map_err(|_| "AUDIO_WRITE: Cannot sync enhanced output")?;
        if cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        parent.validate_directory(false)?;
        if !parent.owns_partial(&temp, &file) {
            return Err("AUDIO_WRITE: Temporary output changed".into());
        }
        validate_product(
            &input,
            &target
                .parent()
                .ok_or("AUDIO_TARGET: Missing parent")?
                .join(&temp),
            cancel,
        )?;
        input.recheck()?;
        if !parent.owns_partial(&temp, &file) {
            return Err("AUDIO_WRITE: Temporary output changed during validation".into());
        }
        let (mut read, size) = parent
            .resume_partial(&temp)?
            .ok_or("AUDIO_WRITE: Output disappeared")?;
        if size < 12 {
            return Err("AUDIO_WRITE: FFmpeg produced no media".into());
        }
        read.seek(SeekFrom::Start(0))
            .map_err(|_| "AUDIO_WRITE: Cannot read output")?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(CANCELLED.into());
            }
            let len = read
                .read(&mut buffer)
                .map_err(|_| "AUDIO_WRITE: Cannot hash output")?;
            if len == 0 {
                break;
            }
            hash.update(&buffer[..len]);
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(CANCELLED.into());
        }
        parent.publish(&temp, name)?;
        Ok(json!({"path":target,"size":size,"sha256":format!("{:x}", hash.finalize())}))
    })();
    if parent.owns_partial(&temp, &file) {
        parent.remove_partial(&temp);
    }
    result
}

#[tauri::command]
pub async fn native_audio_enhance_export(
    state: tauri::State<'_, crate::jobs::Jobs>,
    request: Request,
) -> Result<Option<Value>, String> {
    let id = uuid::Uuid::new_v4();
    let result = export(state.inner().clone(), request, id).await;
    match &result {
        Ok(None) => log(id, "chooser-cancelled", "INFO"),
        Ok(Some(_)) => log(id, "published", "INFO"),
        Err(error) if error.starts_with("AUDIO_CANCELLED:") => log(id, "cancelled", "INFO"),
        Err(error) => {
            // Fixed error class only: no source paths, names, OS errors or tool output.
            let kind = error.split(':').next().unwrap_or("validation");
            log(id, kind, "ERROR");
        }
    }
    result
}

async fn export(
    jobs: crate::jobs::Jobs,
    request: Request,
    id: uuid::Uuid,
) -> Result<Option<Value>, String> {
    request.validate()?;
    let permit = EXPORTING
        .try_acquire()
        .map_err(|_| "AUDIO_BUSY: Another audio export is active")?;
    CANCEL.store(false, Ordering::Relaxed);
    let path = if let Some(source_id) = &request.source_id {
        crate::video_post_source::source(source_id)?
            .path()
            .to_owned()
    } else {
        jobs.audio_source(request.job_id.as_deref().unwrap(), request.index.unwrap())?
    };
    let input =
        tauri::async_runtime::spawn_blocking(move || Input::open_with_cancel(path, &CANCEL))
            .await
            .map_err(|_| "AUDIO_SOURCE: Inspection worker failed")??;
    let extension = input.extension();
    let selected = rfd::AsyncFileDialog::new()
        .set_title(&request.title)
        .add_filter(extension.to_ascii_uppercase(), &[extension])
        .set_file_name(format!("DEAS-enhanced.{extension}"))
        .set_can_create_directories(false)
        .save_file()
        .await;
    let Some(selected) = selected else {
        return Ok(None);
    };
    log(id, "started", "INFO");
    let target = selected.path().to_owned();
    // The chooser may return a path through symlinked folders (bookmarks like ~/Downloads);
    // resolve the parent so the strict anchored write below sees the real directory. Other
    // seams keep their own symlink policies untouched.
    let target = match target
        .parent()
        .and_then(|parent| std::fs::canonicalize(parent).ok())
    {
        Some(real) => match target.file_name() {
            Some(name) => real.join(name),
            None => target,
        },
        None => target,
    };
    // The worker owns admission and cleanup even if the invoking webview stops awaiting IPC.
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        let result = process_retained(
            input,
            &target,
            &AudioSettings {
                mode: request.mode,
                profile: request.profile,
                device: request.device,
            },
            &CANCEL,
        );
        log(
            id,
            if result.is_ok() {
                "worker-completed"
            } else {
                "worker-stopped"
            },
            "INFO",
        );
        result.map(Some)
    })
    .await
    .map_err(|_| "AUDIO_PROCESS: Enhancement worker failed")?
}

#[cfg(test)]
#[path = "audio_enhance_tests.rs"]
mod tests;
