//! Media Analysis: animetimm checkpoints found as PyTorch weights, converted once to ONNX.
//!
//! Ported from PromptForge, where animetimm ConvNeXtV2-Huge is the default tagger through
//! an ONNX copy made by `scripts/export-animetimm-onnx.py`. ComfyUI's animetimm node keeps
//! these checkpoints as PyTorch weights only, and the Hugging Face repos are gated, so the
//! copy is made here from the weights people already have.
//!
//! Only when asked, per checkpoint: the conversion tools (PyTorch CPU, timm, onnx; about
//! 300 MB, hash-locked) go into a throwaway environment under the analysis runtime, the
//! exporter writes the copy, and the environment is removed. The downloaded packages stay in
//! uv's cache while another checkpoint here is waiting, and go with the last conversion.
//! The checkpoint's own folder is only read. The copy lives in `<runtime>/converted/` with
//! a note of where it came from, and discovery lists it in place of the checkpoint.

use crate::analysis::{
    check_cancel, find_uv, managed_uv, model_roots, native_analysis_overview, python, run,
    runtime_root, status, Overview, CANCEL, INSTALLING,
};
use crate::analysis_taggers::{discover, Converted};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::Ordering,
};

#[cfg(not(windows))]
const REQUIREMENTS: &str = include_str!("analysis_convert_requirements.txt");
#[cfg(windows)]
const REQUIREMENTS: &str = include_str!("analysis_convert_requirements_windows.txt");
const EXPORTER: &str = include_str!("animetimm_export.py");
/// What the conversion tools download, roughly (PyTorch CPU is most of it).
pub const TOOLS_BYTES: u64 = 300_000_000;
/// The packages worth dropping from uv's cache afterwards: the rest are small or shared
/// with the tagger runtime.
const BIG_PACKAGES: &[&str] = &["torch", "torchvision", "timm", "onnx"];

#[derive(Serialize, Deserialize)]
struct SourceNote {
    source: String,
    place: String,
    bytes: u64,
}

fn converted_root() -> PathBuf {
    runtime_root().join("converted")
}

/// `<folder name>-<8 hex of its path>`: readable, and two checkpoints with one name do not clash.
fn slug(folder: &Path) -> String {
    let name = folder
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("tagger")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let hash = format!("{:x}", Sha256::digest(folder.to_string_lossy().as_bytes()));
    format!("{name}-{}", &hash[..8])
}

/// Every finished copy: a model, its data, its tag list and the note of where it came from.
pub fn converted() -> Vec<Converted> {
    converted_in(&converted_root())
}

fn converted_in(root: &Path) -> Vec<Converted> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(root)
        .map(|r| {
            r.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    dirs.into_iter()
        .filter(|d| {
            ["model.onnx", "model.onnx.data", "selected_tags.csv"]
                .iter()
                .all(|f| d.join(f).is_file())
        })
        .filter_map(|d| {
            let note: SourceNote =
                serde_json::from_slice(&fs::read(d.join("source.json")).ok()?).ok()?;
            Some(Converted {
                source: PathBuf::from(note.source),
                place: note.place,
                model: d.join("model.onnx"),
            })
        })
        .collect()
}

/// `more_to_convert`: another checkpoint here is still waiting, so the downloaded tools stay in
/// uv's cache for it instead of being fetched again.
fn convert_blocking(
    uv: PathBuf,
    folder: PathBuf,
    place: String,
    bytes: u64,
    more_to_convert: bool,
) -> Result<(), String> {
    let base = runtime_root();
    if !base.is_absolute() {
        return Err("ANALYSIS_CONVERT: No user data folder to convert into".into());
    }
    fs::create_dir_all(converted_root())
        .map_err(|_| "ANALYSIS_CONVERT: Cannot create the folder for converted taggers")?;
    // The whole environment lives in here and goes when this returns, finished or not.
    let work = tempfile::Builder::new()
        .prefix(".convert-")
        .tempdir_in(&base)
        .map_err(|_| "ANALYSIS_CONVERT: Cannot make a working folder for the conversion")?;
    let dir = work.path().to_owned();
    fs::write(dir.join("requirements.txt"), REQUIREMENTS)
        .map_err(|_| "ANALYSIS_CONVERT: Cannot write the package lock")?;
    fs::write(dir.join("animetimm_export.py"), EXPORTER)
        .map_err(|_| "ANALYSIS_CONVERT: Cannot write the converter")?;

    status("convert-environment", 0, 0, None);
    let mut venv = Command::new(&uv);
    venv.args([
        "--no-config",
        "venv",
        "--python",
        "3.12",
        "--managed-python",
    ])
    .arg(dir.join("venv"));
    run(venv, "ANALYSIS_CONVERT", |_| {})?;
    check_cancel()?;

    status("convert-packages", 0, 0, None);
    let mut pip = Command::new(&uv);
    pip.args([
        "--no-config",
        "pip",
        "install",
        "--require-hashes",
        "--only-binary",
        ":all:",
        "--python",
    ])
    .arg(python(&dir))
    .args([
        "--index-url",
        "https://pypi.org/simple",
        "--extra-index-url",
        "https://download.pytorch.org/whl/cpu",
        "--index-strategy",
        "unsafe-best-match",
        "-r",
    ])
    .arg(dir.join("requirements.txt"));
    run(pip, "ANALYSIS_CONVERT", |_| {})?;
    check_cancel()?;

    status("convert-loading", 0, 0, None);
    let out = dir.join("out");
    let mut export = Command::new(python(&dir));
    export
        .args(["-s", "-E", "-X", "utf8", "-u"])
        .arg(dir.join("animetimm_export.py"))
        .arg("--src")
        .arg(&folder)
        .arg("--out")
        .arg(&out);
    run(export, "ANALYSIS_CONVERT", |line| {
        if let Some(stage) = line.strip_prefix("STAGE ") {
            status(&format!("convert-{}", stage.trim()), 0, 0, None);
        }
    })?;
    check_cancel()?;

    // Published whole: a copy is either all there with its note, or not there at all.
    fs::write(
        out.join("source.json"),
        serde_json::to_vec_pretty(&SourceNote {
            source: folder.to_string_lossy().into(),
            place,
            bytes,
        })
        .map_err(|_| "ANALYSIS_CONVERT: Cannot write the converted tagger's note")?,
    )
    .map_err(|_| "ANALYSIS_CONVERT: Cannot write the converted tagger's note")?;
    let target = converted_root().join(slug(&folder));
    if target.exists() {
        fs::remove_dir_all(&target).map_err(|_| "ANALYSIS_CONVERT: Cannot replace the older converted copy; close other DEAS windows and try again")?;
    }
    fs::rename(&out, &target).map_err(|_| "ANALYSIS_CONVERT: Cannot keep the converted tagger")?;

    status("convert-cleanup", 0, 0, None);
    drop(work);
    // PyTorch and friends are only needed for converting: once nothing is left to convert,
    // leave nothing big behind. (A failed or cancelled run keeps them for the retry.)
    if !more_to_convert {
        let mut clean = Command::new(&uv);
        clean
            .args(["--no-config", "cache", "clean"])
            .args(BIG_PACKAGES);
        let _ = run(clean, "ANALYSIS_CONVERT", |_| {});
    }
    Ok(())
}

/// Converts the animetimm checkpoint at `folder` (as discovery reported it) to ONNX.
#[tauri::command]
pub async fn native_analysis_convert(
    engine: tauri::State<'_, crate::install::Engine>,
    models: Vec<String>,
    folder: String,
) -> Result<Overview, String> {
    // Only a folder discovery offered for conversion, never an arbitrary path.
    let offers: Vec<_> = discover(&model_roots(&models))
        .unusable
        .into_iter()
        .filter(|u| u.convert.is_some())
        .collect();
    let more_to_convert = offers.len() > 1;
    let offer = offers
        .into_iter()
        .find(|u| u.convert.as_ref().is_some_and(|c| c.folder == folder))
        .ok_or(
            "ANALYSIS_CONVERT: That tagger is no longer where it was found. Press Look again.",
        )?;
    let bytes = offer.convert.as_ref().map(|c| c.bytes).unwrap_or(0);
    let _permit = INSTALLING
        .try_acquire()
        .map_err(|_| "ANALYSIS_BUSY: Tagger setup or a conversion is already running")?;
    CANCEL.store(false, Ordering::Relaxed);
    status("convert-starting", 0, 0, None);
    let preferred = managed_uv(&engine);
    let result = async {
        let uv = find_uv(preferred)?;
        let source = PathBuf::from(&folder);
        let place = offer.place.clone();
        tauri::async_runtime::spawn_blocking(move || {
            convert_blocking(uv, source, place, bytes, more_to_convert)
        })
        .await
        .map_err(|_| "ANALYSIS_CONVERT: The conversion worker failed".to_string())?
    }
    .await;
    match &result {
        Ok(()) => status("completed", 0, 0, None),
        Err(e) if CANCEL.load(Ordering::Relaxed) => status("cancelled", 0, 0, Some(e.clone())),
        Err(e) => {
            crate::logs::push(
                crate::logs::Source::Deas,
                &format!("ERROR: Media Analysis conversion of {}: {e}", offer.name),
            );
            status("failed", 0, 0, Some(e.clone()))
        }
    }
    result?;
    native_analysis_overview(models, None).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis_taggers::discover_with;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn a_checkpoint_is_offered_for_conversion_and_its_copy_takes_its_place() {
        let tmp = tempfile::tempdir().unwrap();
        let models = tmp.path().join("ComfyUI/models");
        let huge = models.join("animetimm/convnextv2_huge.dbv4-full");
        write(&huge.join("pytorch_model.bin"), "weights!");
        write(
            &huge.join("selected_tags.csv"),
            "tag_id,name,category,count\n1,1girl,0,5\n2,frieren,4,3\n",
        );
        write(&huge.join("preprocess.json"), r#"{"test":[]}"#);
        write(
            &huge.join("config.json"),
            r#"{"architecture":"convnextv2_huge","num_classes":2}"#,
        );
        // No config naming an architecture: listed, not offered.
        let odd = models.join("animetimm/odd.dbv4-full");
        write(&odd.join("pytorch_model.bin"), "x");
        write(
            &odd.join("selected_tags.csv"),
            "tag_id,name,category,count\n1,1girl,0,5\n",
        );
        write(&odd.join("preprocess.json"), "{}");

        let before = discover_with(std::slice::from_ref(&models), &[]);
        assert!(before.taggers.is_empty());
        let offers: Vec<(&str, Option<u64>)> = before
            .unusable
            .iter()
            .map(|u| (u.name.as_str(), u.convert.as_ref().map(|c| c.bytes)))
            .collect();
        assert_eq!(
            offers,
            vec![
                ("animetimm ConvNeXtV2-Huge", Some(8)),
                ("odd.dbv4-full", None)
            ]
        );
        assert!(before.unusable[0]
            .reason
            .contains("DEAS can convert it to ONNX once"));
        assert!(before.unusable[1].reason.contains("would be used"));

        // A finished copy, as the converter publishes it.
        let root = tmp.path().join("runtime/converted");
        let copy = root.join(slug(&huge));
        write(&copy.join("model.onnx"), "x");
        write(&copy.join("model.onnx.data"), "x");
        write(
            &copy.join("selected_tags.csv"),
            "tag_id,name,category,count\n1,1girl,0,5\n2,frieren,4,3\n",
        );
        write(
            &copy.join("source.json"),
            &serde_json::to_string(&SourceNote {
                source: huge.to_string_lossy().into(),
                place: "models/animetimm/convnextv2_huge.dbv4-full".into(),
                bytes: 8,
            })
            .unwrap(),
        );
        // Half-written (no data file): never listed.
        write(&root.join("broken-00000000/model.onnx"), "x");
        let converted = converted_in(&root);
        assert_eq!(converted.len(), 1);

        let after = discover_with(std::slice::from_ref(&models), &converted);
        let got: Vec<(&str, &str, usize, f64)> = after
            .taggers
            .iter()
            .map(|t| {
                (
                    t.name.as_str(),
                    t.place.as_str(),
                    t.tags,
                    t.character_threshold,
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![(
                "animetimm ConvNeXtV2-Huge",
                "models/animetimm/convnextv2_huge.dbv4-full (converted)",
                2,
                0.51
            )]
        );
        assert_eq!(
            after
                .unusable
                .iter()
                .map(|u| u.name.as_str())
                .collect::<Vec<_>>(),
            vec!["odd.dbv4-full"]
        );

        // The checkpoint moved away: its copy still works and says where it came from.
        fs::remove_dir_all(&huge).unwrap();
        let gone = discover_with(&[models], &converted);
        assert_eq!(
            gone.taggers[0].place,
            "models/animetimm/convnextv2_huge.dbv4-full (converted)"
        );
    }

    #[test]
    fn the_lock_names_cpu_torch_with_hashes() {
        assert!(REQUIREMENTS.contains("torch==2.7.1+cpu"));
        assert!(REQUIREMENTS.contains("timm==1.0.15"));
        let pins: Vec<&str> = REQUIREMENTS
            .lines()
            .filter(|l| !l.starts_with('#') && l.contains("=="))
            .collect();
        let packages = pins.len();
        // Every pin continues onto its --hash lines.
        let hashed = pins.iter().all(|l| l.trim_end().ends_with('\\'));
        assert!(packages > 20 && hashed);
        assert!(EXPORTER.contains("STAGE ") && EXPORTER.contains("all_tensors_to_one_file"));
    }
}
