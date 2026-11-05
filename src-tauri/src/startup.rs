//! Startup optimizations for the owned backend: a fixed allowlist of ComfyUI launch flags,
//! stored as `deas-startup.json` beside the environment and applied on the next (re)start.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE: &str = "deas-startup.json";
/// Mutually exclusive flag groups (ComfyUI argparse groups or one DEAS choice); nothing outside them is ever passed.
const GROUPS: [&[&str]; 8] = [
    &[
        "--use-pytorch-cross-attention",
        "--use-ck-attention",
        "--use-sage-attention",
        "--use-flash-attention",
    ],
    // Not a default: it turns off ComfyUI's Dynamic VRAM. (Async offload is already on by default on NVIDIA/AMD.)
    &["--highvram"],
    &["--disable-smart-memory"],
    &["--disable-dynamic-vram"],
    // --cpu-vae is not in ComfyUI's VAE precision group; DEAS offers one VAE choice.
    &["--fp32-vae", "--bf16-vae", "--cpu-vae"],
    &["--bf16-unet", "--fp16-unet"],
    &["--fp16-text-enc"],
    &["--cache-classic", "--cache-none"],
];
/// Explicit `--fast` features only: a bare `--fast` would enable every current and future one.
/// `cublas_ops` is left out: no wheels exist (source build against the exact Torch/CUDA).
const FAST: [&str; 3] = ["fp16_accumulation", "fp8_matrix_mult", "autotune"];
/// Flag/feature → Python module it imports; ComfyUI fails or silently ignores it without the package.
const PACKAGES: [(&str, &str); 2] = [
    ("--use-sage-attention", "sageattention"),
    ("--use-flash-attention", "flash_attn"),
];

/// Named presets, resolved against the installed hardware at every start (a new GPU re-resolves them).
const PRESETS: [&str; 3] = ["speed", "compatibility", "off"];

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Startup {
    /// When set, the explicit fields stay empty and the preset decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub fast: Vec<String>,
    /// `--reserve-vram <GB>` for VRAM shared with other software.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserve_vram: Option<f64>,
    /// `--cache-lru <N>` node results; exclusive with the cache flags.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_lru: Option<u32>,
}

/// NVIDIA compute capability of the installed plan, `None` for CPU or unknown hardware.
fn capability(plan: Option<&crate::install::Plan>) -> Option<(u32, u32)> {
    let plan = plan.filter(|p| matches!(p.request.backend, crate::install::Backend::Nvidia))?;
    let (major, minor) = plan.hardware.as_ref()?.compute_capability.split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

/// Speed+ = Comfy Kitchen INT8 attention + fp16 accumulation + kernel autotune; Comfy Kitchen
/// needs SM 7.5+ (ComfyUI exits otherwise), so older cards get autotune only. Compatibility =
/// autotune only (same math, any CUDA GPU). Off = ComfyUI's own defaults. CPU: nothing.
pub fn preset(name: &str, plan: Option<&crate::install::Plan>) -> Startup {
    let strings = |v: &[&str]| v.iter().map(|s| s.to_string()).collect();
    match (name, capability(plan)) {
        ("speed", Some(cc)) if cc >= (7, 5) => Startup {
            flags: strings(&["--use-ck-attention"]),
            fast: strings(&["fp16_accumulation", "autotune"]),
            ..Default::default()
        },
        ("speed" | "compatibility", Some(_)) => Startup {
            fast: strings(&["autotune"]),
            ..Default::default()
        },
        _ => Startup::default(),
    }
}

fn resolve(s: Startup, plan: Option<&crate::install::Plan>) -> Startup {
    match &s.preset {
        Some(name) => preset(name, plan),
        None => s,
    }
}

fn validate(s: &Startup, root: &Path) -> Result<(), String> {
    if let Some(name) = &s.preset {
        if !PRESETS.contains(&name.as_str())
            || *s
                != (Startup {
                    preset: s.preset.clone(),
                    ..Default::default()
                })
        {
            return Err("A preset excludes explicit startup options".into());
        }
        return Ok(());
    }
    let mut seen = std::collections::HashSet::new();
    for flag in &s.flags {
        let group = GROUPS
            .iter()
            .position(|g| g.contains(&flag.as_str()))
            .ok_or_else(|| format!("Unsupported startup flag {flag}"))?;
        if !seen.insert(group) {
            return Err(format!("{flag} conflicts with another selected flag"));
        }
    }
    let mut fast = std::collections::HashSet::new();
    if s.fast
        .iter()
        .any(|f| !FAST.contains(&f.as_str()) || !fast.insert(f))
    {
        return Err("Unsupported or repeated --fast feature".into());
    }
    if s.reserve_vram
        .is_some_and(|gb| !(gb.is_finite() && gb > 0.0 && gb <= 128.0))
    {
        return Err("Reserved VRAM must be in (0, 128] GB".into());
    }
    if s.cache_lru.is_some_and(|n| !(1..=10_000).contains(&n))
        || (s.cache_lru.is_some() && s.flags.iter().any(|f| f.starts_with("--cache-")))
    {
        return Err("LRU cache size must be 1–10000 and excludes the other cache modes".into());
    }
    let present = packages(root);
    for (flag, module) in PACKAGES {
        if (s.flags.iter().any(|f| f == flag) || s.fast.iter().any(|f| f == flag))
            && !present.contains(&module)
        {
            return Err(format!(
                "{flag} needs the Python package {module} in the backend"
            ));
        }
    }
    Ok(())
}

/// Read-only: which optional packages the environment's venv contains (no Python started).
pub fn packages(root: &Path) -> Vec<&'static str> {
    let venv = root.join("venv");
    let mut dirs: Vec<PathBuf> = vec![venv.join("Lib").join("site-packages")];
    if let Ok(entries) = std::fs::read_dir(venv.join("lib")) {
        dirs.extend(entries.flatten().map(|e| e.path().join("site-packages")));
    }
    let names: Vec<String> = dirs
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    // A package dir `sageattention/` or an extension module `flash_attn.cpython-312-….so`.
    PACKAGES
        .iter()
        .map(|(_, module)| *module)
        .filter(|m| names.iter().any(|n| n.split('.').next() == Some(*m)))
        .collect()
}

pub fn read(root: &Path) -> Result<Startup, String> {
    match std::fs::read(root.join(FILE)) {
        Ok(bytes) => serde_json::from_slice(crate::bom_free(&bytes))
            .map_err(|e| format!("Invalid {FILE}: {e}")),
        // No saved choice: Speed+ is the default.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Startup {
            preset: Some("speed".into()),
            ..Default::default()
        }),
        Err(e) => Err(format!("{FILE}: {e}")),
    }
}

pub fn write(root: &Path, startup: Startup) -> Result<Startup, String> {
    if !root.is_absolute() || !root.is_dir() {
        return Err("Environment root must be an existing absolute directory".into());
    }
    validate(&startup, root)?;
    let target = root.join(FILE);
    match std::fs::symlink_metadata(&target) {
        Ok(m) if !m.is_file() => {
            return Err("Startup settings target must be a regular file".into())
        }
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
        _ => (),
    }
    let bytes = serde_json::to_vec_pretty(&startup).map_err(|e| e.to_string())?;
    let tmp = root.join(format!(".{FILE}.{}", uuid::Uuid::new_v4()));
    let result = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut f| {
            use std::io::Write;
            f.write_all(&bytes)?;
            f.sync_all()
        })
        .and_then(|()| std::fs::rename(&tmp, &target));
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.to_string());
    }
    Ok(startup)
}

/// Launch arguments; revalidated at every start so a removed package fails the start instead of the job.
pub fn args(root: &Path, plan: Option<&crate::install::Plan>) -> Result<Vec<String>, String> {
    let s = resolve(read(root)?, plan);
    validate(&s, root)?;
    let mut args = s.flags.clone();
    if !s.fast.is_empty() {
        args.push("--fast".into());
        args.extend(s.fast.iter().cloned());
    }
    if let Some(gb) = s.reserve_vram {
        args.extend(["--reserve-vram".into(), gb.to_string()]);
    }
    if let Some(n) = s.cache_lru {
        args.extend(["--cache-lru".into(), n.to_string()]);
    }
    Ok(args)
}

#[derive(Serialize)]
pub struct StartupState {
    startup: Startup,
    packages: Vec<&'static str>,
    /// Each preset resolved for the open environment's hardware, for display.
    presets: std::collections::BTreeMap<&'static str, Startup>,
}

#[tauri::command]
pub fn native_startup_read(
    root: String,
    engine: tauri::State<'_, crate::install::Engine>,
) -> Result<StartupState, String> {
    let path = PathBuf::from(&root);
    if !path.is_absolute() {
        return Err("Environment root must be absolute".into());
    }
    let plan = engine
        .approved_plan()
        .filter(|p| p.request.storage_path == root);
    Ok(StartupState {
        startup: read(&path)?,
        packages: packages(&path),
        presets: PRESETS
            .iter()
            .map(|name| (*name, preset(name, plan.as_ref())))
            .collect(),
    })
}

#[tauri::command]
pub fn native_startup_write(
    root: String,
    startup: Startup,
    operations: tauri::State<'_, crate::FolderOperations>,
) -> Result<Startup, String> {
    let _guard = operations.enter()?;
    write(Path::new(&root), startup)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(flags: &[&str], fast: &[&str]) -> Startup {
        Startup {
            flags: flags.iter().map(|f| f.to_string()).collect(),
            fast: fast.iter().map(|f| f.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn bom_prefixed_startup_file_still_reads() {
        let root = tempfile::tempdir().unwrap();
        let saved = s(&["--highvram"], &[]);
        write(root.path(), saved.clone()).unwrap();
        let file = root.path().join(FILE);
        let bytes = std::fs::read(&file).unwrap();
        std::fs::write(&file, [b"\xef\xbb\xbf".as_slice(), &bytes].concat()).unwrap();
        assert_eq!(read(root.path()).unwrap(), saved);
    }

    #[test]
    fn allowlisted_flags_roundtrip_into_launch_args() {
        let root = tempfile::tempdir().unwrap();
        // No saved file = Speed+, which is empty without NVIDIA hardware.
        assert_eq!(read(root.path()).unwrap().preset.as_deref(), Some("speed"));
        assert!(args(root.path(), None).unwrap().is_empty());
        let mut saved = s(
            &["--use-ck-attention", "--highvram", "--bf16-vae"],
            &["fp16_accumulation", "autotune"],
        );
        saved.reserve_vram = Some(1.5);
        saved.cache_lru = Some(8);
        write(root.path(), saved.clone()).unwrap();
        assert_eq!(read(root.path()).unwrap(), saved);
        assert_eq!(
            args(root.path(), None).unwrap(),
            [
                "--use-ck-attention",
                "--highvram",
                "--bf16-vae",
                "--fast",
                "fp16_accumulation",
                "autotune",
                "--reserve-vram",
                "1.5",
                "--cache-lru",
                "8"
            ]
        );
    }

    #[test]
    fn rejects_unknown_conflicting_and_package_less_flags() {
        let root = tempfile::tempdir().unwrap();
        for bad in [
            s(&["--listen"], &[]),
            s(&["--use-ck-attention", "--use-sage-attention"], &[]),
            s(&["--fp32-vae", "--cpu-vae"], &[]),
            s(&[], &["everything"]),
            s(&[], &["autotune", "autotune"]),
            s(&["--use-sage-attention"], &[]),
            s(&[], &["cublas_ops"]),
            s(&["--async-offload"], &[]),
            s(&["--use-flash-attention"], &[]),
            Startup {
                reserve_vram: Some(0.0),
                ..Default::default()
            },
            Startup {
                flags: vec!["--cache-none".into()],
                cache_lru: Some(4),
                ..Default::default()
            },
        ] {
            assert!(write(root.path(), bad.clone()).is_err(), "accepted {bad:?}");
        }
        assert!(!root.path().join(FILE).exists());
        let site = root.path().join("venv/lib/python3.12/site-packages");
        std::fs::create_dir_all(site.join("sageattention")).unwrap();
        std::fs::write(site.join("flash_attn.cpython-312-x86_64-linux-gnu.so"), "").unwrap();
        assert_eq!(packages(root.path()), ["sageattention", "flash_attn"]);
        write(root.path(), s(&["--use-sage-attention"], &[])).unwrap();
        // A package removed after saving fails the start, not the first job.
        std::fs::remove_dir(site.join("sageattention")).unwrap();
        assert!(args(root.path(), None).is_err());
    }

    #[test]
    fn presets_follow_the_installed_gpu() {
        let plan = |backend: &str, cc: &str| -> crate::install::Plan {
            let mut p: crate::install::Plan = serde_json::from_value(serde_json::json!({
                "id":"p","request":{"storagePath":"/env","backend":backend,"features":[],"options":{"sage":false,"radial":false,"flash":false,"ffmpeg":false},"comfyRef":"master"},
                "steps":[],"warnings":[],"unsupportedOptions":[],"canExecute":true,"hardware":null,"torch":null
            })).unwrap();
            p.hardware = Some(serde_json::from_value(serde_json::json!({"vendor":"NVIDIA","name":"GPU","uuid":"u","driverVersion":"1","computeCapability":cc})).unwrap());
            p
        };
        let rtx = plan("nvidia", "12.0");
        let speed = preset("speed", Some(&rtx));
        assert_eq!(speed.flags, ["--use-ck-attention"]);
        assert_eq!(speed.fast, ["fp16_accumulation", "autotune"]);
        assert_eq!(preset("compatibility", Some(&rtx)).fast, ["autotune"]);
        assert_eq!(preset("off", Some(&rtx)), Startup::default());
        // Pascal: no Comfy Kitchen INT8 kernel (SM 7.5+), so ComfyUI would exit.
        assert!(preset("speed", Some(&plan("nvidia", "6.1")))
            .flags
            .is_empty());
        assert_eq!(
            preset("speed", Some(&plan("cpu", "12.0"))),
            Startup::default()
        );
        let root = tempfile::tempdir().unwrap();
        assert_eq!(
            args(root.path(), Some(&rtx)).unwrap(),
            [
                "--use-ck-attention",
                "--fast",
                "fp16_accumulation",
                "autotune"
            ]
        );
        let mut mixed = Startup {
            preset: Some("speed".into()),
            ..Default::default()
        };
        mixed.fast.push("autotune".into());
        assert!(write(root.path(), mixed).is_err());
        assert!(write(
            root.path(),
            Startup {
                preset: Some("turbo".into()),
                ..Default::default()
            }
        )
        .is_err());
        write(
            root.path(),
            Startup {
                preset: Some("off".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(args(root.path(), Some(&rtx)).unwrap().is_empty());
    }
}
