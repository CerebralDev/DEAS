//! Read-only recommendations; never authorizes or writes an installation.
use crate::install::torch_profile::{self, Hardware};
use crate::install::Backend;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallDefaults {
    pub storage_path: String,
    pub backend: Backend,
    pub hardware: Option<Hardware>,
    pub warnings: Vec<String>,
}

fn recommend(
    executable: &Path,
    probe: Result<Hardware, String>,
) -> Result<InstallDefaults, String> {
    let parent = executable
        .parent()
        .filter(|path| path.is_absolute())
        .ok_or("Cannot determine executable directory")?;
    let target = parent.join("deas-environment");
    let mut warnings = Vec::new();
    let (backend, hardware) = match probe {
        Ok(hardware) => match torch_profile::select(&hardware) {
            Ok(_) => (Backend::Nvidia, Some(hardware)),
            Err(error) => {
                warnings.push(format!(
                    "CPU recommended because NVIDIA compatibility checks failed: {error}"
                ));
                (Backend::Cpu, Some(hardware))
            }
        },
        Err(error) => {
            warnings.push(format!("CPU recommended as the conservative supported baseline; no compatible NVIDIA device was verified: {error}"));
            (Backend::Cpu, None)
        }
    };
    if std::fs::symlink_metadata(&target).is_ok() {
        warnings.push("Default environment already exists; DEAS will check it for reuse. It is never reinstalled, renamed, overwritten or wiped.".into());
    }
    Ok(InstallDefaults {
        storage_path: target
            .to_str()
            .ok_or("Executable directory must be UTF-8")?
            .into(),
        backend,
        hardware,
        warnings,
    })
}

/// Recommendations only. The immutable plan independently validates everything
/// again; a suggested CPU backend is not a claim that a CPU/GPU was detected.
#[tauri::command]
pub async fn native_install_defaults() -> Result<InstallDefaults, String> {
    let executable =
        std::env::current_exe().map_err(|e| format!("Cannot locate executable: {e}"))?;
    tauri::async_runtime::spawn_blocking(move || recommend(&executable, torch_profile::probe()))
        .await
        .map_err(|e| format!("Default recommendation worker failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_command_is_registered_and_scoped_to_local_install_permission() {
        assert!(include_str!("main.rs").contains("deas::native_install_defaults,"));
        assert!(include_str!("../permissions/install.toml").contains("\"native_install_defaults\""));
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
        assert_eq!(capability["local"], true);
        assert_eq!(capability["windows"], serde_json::json!(["main"]));
        assert!(capability.get("remote").is_none());
    }

    #[tokio::test]
    #[ignore = "Read-only executable/GPU recommendation on actual host"]
    async fn live_install_defaults_are_read_only() {
        let before = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .join("deas-environment")
            .exists();
        let defaults = native_install_defaults().await.unwrap();
        assert_eq!(
            std::path::Path::new(&defaults.storage_path).exists(),
            before
        );
        println!("{}", serde_json::to_string(&defaults).unwrap());
    }

    #[test]
    fn defaults_use_executable_parent_not_working_directory_without_writes() {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("DEAS");
        std::fs::write(&executable, "fixture").unwrap();
        let defaults = recommend(&executable, Err("No NVIDIA probe".into())).unwrap();
        assert_eq!(
            defaults.storage_path,
            root.path().join("deas-environment").to_str().unwrap()
        );
        assert!(matches!(defaults.backend, Backend::Cpu));
        assert!(defaults.hardware.is_none());
        assert!(defaults.warnings.iter().any(|w| w.contains("CPU")));
        assert!(!root.path().join("deas-environment").exists());
    }

    #[test]
    fn defaults_recommend_only_a_compatible_nvidia_profile() {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("DEAS");
        let valid = torch_profile::parse("GPU-fixture, RTX 5090, 615.71.09, 12.0").unwrap();
        let defaults = recommend(&executable, Ok(valid)).unwrap();
        assert!(matches!(defaults.backend, Backend::Nvidia));
        assert_eq!(defaults.hardware.unwrap().uuid, "GPU-fixture");
        let old_driver = torch_profile::parse("GPU-fixture, RTX 5090, 550.1, 12.0").unwrap();
        let fallback = recommend(&executable, Ok(old_driver)).unwrap();
        assert!(matches!(fallback.backend, Backend::Cpu));
        assert!(fallback.hardware.is_some());
        assert!(fallback
            .warnings
            .iter()
            .any(|w| w.contains("NVIDIA_DRIVER")));
    }

    #[test]
    fn existing_default_directory_is_reported_not_adopted_or_renamed() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("deas-environment");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("keep"), "untouched").unwrap();
        let defaults = recommend(&root.path().join("DEAS"), Err("No GPU".into())).unwrap();
        assert_eq!(defaults.storage_path, target.to_str().unwrap());
        assert!(defaults.warnings.iter().any(|w| w.contains("exists")));
        assert_eq!(
            std::fs::read_to_string(target.join("keep")).unwrap(),
            "untouched"
        );
    }
}
