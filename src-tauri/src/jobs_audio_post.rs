//! Native-owned optional audio stage; backend completion is not job completion.
#[cfg(test)]
#[path = "jobs_audio_post_tests.rs"]
mod tests;
use super::*;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

pub(super) fn settings(
    intent: &Value,
) -> Result<Option<crate::audio_enhance::AudioSettings>, String> {
    let Some(value) = intent.get("audioEnhance") else {
        return Ok(None);
    };
    if !matches!(
        intent["recipeId"].as_str(),
        Some("video-generation" | "music-generation" | "video-enhance")
    ) {
        return Err("AUDIO_INTENT: Audio enhancement requires video or music".into());
    }
    let settings: crate::audio_enhance::AudioSettings = serde_json::from_value(value.clone())
        .map_err(|_| "AUDIO_SETTINGS: Expected {mode: clean|deep|stems, profile: mix|speech}")?;
    if !matches!(settings.mode.as_str(), "clean" | "deep" | "stems")
        || !matches!(settings.profile.as_str(), "mix" | "speech")
        || (settings.mode == "deep" && settings.profile != "speech")
    {
        return Err("AUDIO_SETTINGS: Deep is speech-only; select a supported mode/profile".into());
    }
    Ok(Some(settings))
}

impl Jobs {
    /// Cancel only this app's native post-processing before its owned runtime shuts down.
    pub fn cancel_audio_stages(&self) {
        if let Ok(mut session) = self.lock() {
            session.stop_requested = true;
            if let Some(cancel) = &session.audio_post {
                cancel.store(true, Ordering::Relaxed);
            }
            for held in &session.workbench {
                if let Ok(mut entry) = held.lock() {
                    entry.stop_requested = true;
                    if let Some(cancel) = &entry.audio_post {
                        cancel.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
    }

    pub(super) fn start_audio_observer(&self) {
        let jobs = self.clone();
        tokio::spawn(async move {
            let mut delay = 250;
            loop {
                let id = match jobs.lock() {
                    Ok(session) => match session.job.as_ref() {
                        Some(job) if !job.terminal() && !session.stop_requested => job.id.clone(),
                        _ => break,
                    },
                    Err(_) => break,
                };
                let recovered = jobs
                    .status(id)
                    .await
                    .is_ok_and(|snapshot| snapshot["state"] != "unknown");
                delay = if recovered {
                    250
                } else {
                    (delay * 2).min(5000)
                };
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
        });
    }

    pub(super) fn start_audio_post(&self, job: Job, cancel: Arc<AtomicBool>) {
        let jobs = self.clone();
        tokio::spawn(async move {
            let id = job.id.clone();
            let worker_cancel = cancel.clone();
            let result = tokio::task::spawn_blocking(move || {
                let mut outputs = job.outputs.clone();
                for output in &mut outputs {
                    output["role"] = json!("original");
                }
                let result = process(&job, &worker_cancel, &mut outputs);
                if result.is_ok() && outputs.len() == job.outputs.len() {
                    // A video without audio is a true no-op, including its existing output roles.
                    outputs = job.outputs.clone();
                }
                (outputs, result)
            })
            .await;
            let Ok(mut session) = jobs.lock() else { return };
            let Some(job) = session.job.as_mut().filter(|job| job.id == id) else {
                return;
            };
            match result {
                Ok((outputs, result)) => {
                    job.outputs = outputs;
                    let result = if cancel.load(Ordering::Relaxed) {
                        Err("AUDIO_CANCELLED: Audio stage cancelled; original preserved".into())
                    } else {
                        result
                    };
                    match result {
                        Ok(()) => {
                            job.state = "succeeded";
                            job.error = None;
                        }
                        Err(error) => {
                            job.state = if error.starts_with("AUDIO_CANCELLED:") {
                                "cancelled"
                            } else {
                                "failed"
                            };
                            job.error = Some(error);
                        }
                    }
                }
                Err(_) => {
                    job.state = "failed";
                    job.error = Some("AUDIO_WORKER: Audio stage failed; original preserved".into());
                }
            }
            job.history_outputs = job.outputs.clone();
            job.stop_observer_if_terminal();
            jobs.3.observe(job);
            workbench::report_job("audio-post", &job.snapshot());
            session.audio_post = None;
            session.retain_completed();
        });
    }
}

fn process(job: &Job, cancel: &AtomicBool, outputs: &mut Vec<Value>) -> Result<(), String> {
    let settings = settings(&job.plan.intent)?.ok_or("AUDIO_SETTINGS: Missing audio stage")?;
    let root = job
        .plan
        .media_root
        .as_ref()
        .and_then(job_outputs::MediaRoot::current_path)
        .ok_or("AUDIO_SOURCE: Automatic enhancement requires an unchanged owned media root")?;
    let canonical_root =
        std::fs::canonicalize(root).map_err(|_| "AUDIO_SOURCE: Root unavailable")?;
    let candidates: Vec<_> = job
        .outputs
        .iter()
        .filter(|output| {
            output["role"] != "original"
                && output["filename"].as_str().is_some_and(|name| {
                    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
                    [
                        "mp4", "mkv", "webm", "mov", "flac", "wav", "mp3", "ogg", "opus", "m4a",
                    ]
                    .contains(&ext.as_str())
                })
        })
        .cloned()
        .collect();
    if candidates.is_empty() {
        return Err("AUDIO_SOURCE: No encoded audio/video product; original preserved".into());
    }
    if outputs.len() + candidates.len() > 64 {
        return Err("AUDIO_OUTPUTS: Too many paired products; originals preserved".into());
    }
    for output in candidates {
        if cancel.load(Ordering::Relaxed) {
            return Err("AUDIO_CANCELLED: Audio stage cancelled; original preserved".into());
        }
        let source = root.join(output["subfolder"].as_str().unwrap_or("")).join(
            output["filename"]
                .as_str()
                .ok_or("AUDIO_SOURCE: Missing filename")?,
        );
        let canonical =
            std::fs::canonicalize(&source).map_err(|_| "AUDIO_SOURCE: Output unavailable")?;
        if !canonical.starts_with(&canonical_root) {
            return Err("AUDIO_SOURCE: Output escaped owned root".into());
        }
        crate::video_post::open_source(&source)?;
        let source_extension = source
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let extension = if ["mp4", "mkv", "webm", "mov"].contains(&source_extension.as_str()) {
            "mkv"
        } else {
            "flac"
        };
        let filename = format!("deas-audio-{}.{}", uuid::Uuid::new_v4(), extension);
        let target: PathBuf = source
            .parent()
            .ok_or("AUDIO_SOURCE: Missing parent")?
            .join(&filename);
        let parent = crate::downloads::fs::Anchor::open(
            source.parent().ok_or("AUDIO_SOURCE: Missing parent")?,
        )?;
        let stage_name = format!(".deas-audio-stage-{}.{}", uuid::Uuid::new_v4(), extension);
        let stage = target.with_file_name(&stage_name);
        // The automatic stage never interrupts other work: a busy compute gate or busy
        // owned queue defers it; after bounded waits the baseline stays and the stage fails.
        let mut attempt = 0_u8;
        let processed_stage = loop {
            match crate::audio_enhance::process_file(&source, &stage, &settings, cancel) {
                Ok(_) => break Ok(()),
                Err(error)
                    if error.contains("RESOURCE_BUSY")
                        && attempt < 6
                        && !cancel.load(Ordering::Relaxed) =>
                {
                    attempt += 1;
                    std::thread::sleep(Duration::from_secs(5));
                }
                Err(error) if extension == "mkv" && error.starts_with("AUDIO_NO_STREAM:") => {
                    break Err(error)
                }
                Err(error) => break Err(error),
            }
        };
        if let Err(error) = processed_stage {
            if extension == "mkv" && error.starts_with("AUDIO_NO_STREAM:") {
                continue;
            }
            return Err(error);
        }
        let staged = crate::video_post::open_source(&stage)?;
        let published = (|| {
            if cancel.load(Ordering::Relaxed) {
                return Err("AUDIO_CANCELLED: Audio stage cancelled; original preserved".into());
            }
            job.plan
                .media_root
                .as_ref()
                .and_then(job_outputs::MediaRoot::current_path)
                .ok_or("AUDIO_SOURCE: Owned media root changed before publication")?;
            parent.validate_directory(false)?;
            if !parent.owns_partial(&stage_name, &staged) {
                return Err("AUDIO_OUTPUT: Staged output changed".into());
            }
            parent.publish(&stage_name, &filename)
        })();
        if parent.owns_partial(&stage_name, &staged) {
            parent.remove_partial(&stage_name);
        }
        published?;
        let mut processed = output;
        processed["filename"] = json!(filename);
        processed["format"] = json!(if extension == "mkv" {
            "video/x-matroska"
        } else {
            "audio/flac"
        });
        processed["role"] = json!("processed");
        outputs.insert(0, processed);
    }
    Ok(())
}
