//! App-local, data-only Studio preferences; never backend configuration or execution input.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::Manager;

const FILE: &str = "deas-presets.json";
const MAX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    pub name: String,
    pub settings: Map<String, Value>,
}

fn validate(presets: &[Preset]) -> Result<(), String> {
    if presets.len() > 100 {
        return Err("At most 100 presets are allowed".into());
    }
    let mut names = HashSet::new();
    for preset in presets {
        if preset.name.trim().is_empty() || preset.name.chars().count() > 100 {
            return Err("Preset names must contain 1–100 characters".into());
        }
        if !names.insert(&preset.name) {
            return Err("Preset names must be unique".into());
        }
        for key in preset.settings.keys() {
            let Some(setting) = key.strip_prefix("studio.run.") else {
                return Err("Only Studio run preferences may be saved".into());
            };
            if setting.is_empty()
                || ["recentSeeds", "metadataOpen", "continuity"]
                    .iter()
                    .any(|blocked| {
                        setting == *blocked || setting.starts_with(&format!("{blocked}."))
                    })
            {
                return Err("Transient Studio preferences cannot be saved".into());
            }
        }
    }
    Ok(())
}

struct BoundedJson(Vec<u8>);
impl Write for BoundedJson {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_BYTES - self.0.len() {
            return Err(std::io::Error::other("Preset JSON exceeds 2 MiB"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn read(root: &Path) -> Result<Vec<Preset>, String> {
    let target = root.join(FILE);
    match std::fs::symlink_metadata(&target) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Ok(m) if m.is_file() => (),
        _ => return Err("Preset target must be a readable regular file".into()),
    }
    let mut bytes = Vec::new();
    std::fs::File::open(target)
        .and_then(|f| f.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes))
        .map_err(|_| "Cannot read Studio presets")?;
    if bytes.len() > MAX_BYTES {
        return Err("Preset JSON exceeds 2 MiB".into());
    }
    // Never include parser errors: they can contain private preference values.
    let presets: Vec<Preset> = serde_json::from_slice(crate::bom_free(&bytes))
        .map_err(|_| "Invalid Studio preset JSON")?;
    validate(&presets)?;
    Ok(presets)
}

fn write(root: &Path, presets: &[Preset]) -> Result<(), String> {
    validate(presets)?;
    let mut bytes = BoundedJson(Vec::new());
    serde_json::to_writer(&mut bytes, presets).map_err(|_| "Preset JSON exceeds 2 MiB")?;
    std::fs::create_dir_all(root).map_err(|_| "Cannot create Studio preferences directory")?;
    let target = root.join(FILE);
    match std::fs::symlink_metadata(&target) {
        Ok(m) if !m.is_file() => return Err("Preset target must be a regular file".into()),
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err("Cannot inspect Studio preset target".into())
        }
        _ => (),
    }
    let mut tmp = tempfile::NamedTempFile::new_in(root)
        .map_err(|_| "Cannot create Studio preset temporary file")?;
    tmp.write_all(&bytes.0)
        .and_then(|()| tmp.as_file().sync_all())
        .map_err(|_| "Cannot write Studio presets")?;
    tmp.persist(target)
        .map_err(|_| "Cannot replace Studio presets")?;
    Ok(())
}

fn path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_local_data_dir()
        .map_err(|_| "Cannot locate Studio preferences directory".into())
}

fn report<T>(operation: usize, result: Result<T, String>) -> Result<T, String> {
    static LAST: Mutex<[Option<Instant>; 3]> = Mutex::new([None; 3]);
    if let Err(error) = &result {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if last[operation].is_none_or(|at| at.elapsed() >= Duration::from_secs(60)) {
            last[operation] = Some(Instant::now());
            crate::logs::push(
                crate::logs::Source::Deas,
                &format!(
                    "ERROR: Studio presets {}: {error} (repeats limited to once per minute)",
                    ["read", "write", "invalid-load"][operation]
                ),
            );
        }
    }
    result
}

#[tauri::command]
pub fn native_presets_read(app: tauri::AppHandle) -> Result<Vec<Preset>, String> {
    report(0, path(&app).and_then(|root| read(&root)))
}

#[tauri::command]
pub fn native_presets_write(app: tauri::AppHandle, presets: Vec<Preset>) -> Result<(), String> {
    report(1, path(&app).and_then(|root| write(&root, &presets)))
}

/// Fixed diagnostic only; the UI rejects the whole load using its per-control validators.
#[tauri::command]
pub fn native_presets_invalid() {
    let _ = report::<()>(
        2,
        Err("UI preference validation rejected the preset load".into()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn preset(name: &str) -> Preset {
        Preset {
            name: name.into(),
            settings: serde_json::from_value(json!({
                "studio.run.steps": 20,
                "studio.run.prompt": "private prompt",
                "studio.run.opaque": {"future": [null, true, 1]}
            }))
            .unwrap(),
        }
    }

    #[test]
    fn bom_prefixed_preset_file_still_reads() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let presets = vec![preset("Portrait")];
        write(root, &presets).unwrap();
        let file = root.join(FILE);
        let bytes = std::fs::read(&file).unwrap();
        std::fs::write(&file, [b"\xef\xbb\xbf".as_slice(), &bytes].concat()).unwrap();
        assert_eq!(read(root).unwrap(), presets);
    }

    #[test]
    fn persistence_replaces_and_clears_without_side_effects() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("preferences 雪");
        assert_eq!(read(&root).unwrap(), vec![]);
        assert!(!root.exists());
        let presets = vec![preset("Portrait 雪")];
        write(&root, &presets).unwrap();
        assert_eq!(read(&root).unwrap(), presets);
        write(&root, &[preset("Landscape")]).unwrap();
        assert_eq!(read(&root).unwrap(), [preset("Landscape")]);
        write(&root, &[]).unwrap();
        assert!(read(&root).unwrap().is_empty());
        assert_eq!(std::fs::read_dir(root).unwrap().count(), 1);
    }

    #[test]
    fn validation_and_size_failures_preserve_saved_bytes() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), &[preset("saved")]).unwrap();
        let before = std::fs::read(dir.path().join(FILE)).unwrap();
        let mut invalid = vec![
            vec![preset("")],
            vec![preset("  ")],
            vec![preset(&"雪".repeat(101))],
            vec![preset("same"), preset("same")],
            vec![preset("too many"); 101],
        ];
        for key in [
            "backend.token",
            "studio.run.",
            "studio.run.recentSeeds",
            "studio.run.metadataOpen",
            "studio.run.continuity",
            "studio.run.continuity.sources",
        ] {
            let mut bad = preset("bad");
            bad.settings.insert(key.into(), json!("private secret"));
            invalid.push(vec![bad]);
        }
        let mut huge = preset("huge");
        huge.settings
            .insert("studio.run.prompt".into(), json!("x".repeat(MAX_BYTES)));
        invalid.push(vec![huge]);
        for presets in invalid {
            assert!(write(dir.path(), &presets).is_err());
            assert_eq!(std::fs::read(dir.path().join(FILE)).unwrap(), before);
        }
        assert!(validate(&[preset(&"雪".repeat(100))]).is_ok());
        assert!(validate(&(0..100).map(|i| preset(&i.to_string())).collect::<Vec<_>>()).is_ok());
    }

    #[test]
    fn two_mib_boundary_is_inclusive_for_reads_and_writes() {
        let dir = tempfile::tempdir().unwrap();
        let mut saved = preset("boundary");
        let base = serde_json::to_vec(&[saved.clone()]).unwrap().len();
        saved.settings.insert(
            "studio.run.prompt".into(),
            json!("x".repeat(MAX_BYTES - base + "private prompt".len())),
        );
        write(dir.path(), &[saved.clone()]).unwrap();
        let before = std::fs::read(dir.path().join(FILE)).unwrap();
        assert_eq!(before.len(), MAX_BYTES);
        assert_eq!(read(dir.path()).unwrap(), [saved.clone()]);
        let prompt = saved.settings["studio.run.prompt"]
            .as_str()
            .unwrap()
            .to_owned()
            + "x";
        saved
            .settings
            .insert("studio.run.prompt".into(), json!(prompt));
        assert!(write(dir.path(), &[saved]).is_err());
        assert_eq!(std::fs::read(dir.path().join(FILE)).unwrap(), before);
    }

    #[test]
    fn corrupt_and_oversized_files_are_not_reset() {
        let dir = tempfile::tempdir().unwrap();
        for bytes in [
            b"not json private secret".to_vec(),
            b"{}".to_vec(),
            b"[{}]".to_vec(),
            br#"[{"name":"bad","settings":{"backend.token":"secret"}}]"#.to_vec(),
            vec![b' '; MAX_BYTES + 1],
        ] {
            std::fs::write(dir.path().join(FILE), &bytes).unwrap();
            let error = read(dir.path()).unwrap_err();
            assert!(!error.contains("secret"));
            assert_eq!(std::fs::read(dir.path().join(FILE)).unwrap(), bytes);
        }
        std::fs::remove_file(dir.path().join(FILE)).unwrap();
        std::fs::create_dir(dir.path().join(FILE)).unwrap();
        assert!(read(dir.path()).is_err());
        assert!(write(dir.path(), &[preset("valid")]).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_target_is_never_read_or_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original");
        std::fs::write(&original, b"private").unwrap();
        std::os::unix::fs::symlink(&original, dir.path().join(FILE)).unwrap();
        assert!(read(dir.path()).is_err());
        assert!(write(dir.path(), &[preset("valid")]).is_err());
        assert_eq!(std::fs::read(original).unwrap(), b"private");
    }

    #[test]
    fn fixed_invalid_diagnostic_is_shared_and_rate_limited() {
        native_presets_invalid();
        native_presets_invalid();
        let logs = crate::logs::native_logs(0);
        let entries: Vec<_> = logs
            .iter()
            .filter(|e| e.text.starts_with("ERROR: Studio presets invalid-load:"))
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, crate::logs::Source::Deas);
        for (operation, result) in [
            (0, read(Path::new("\0")).map(|_| ())),
            (1, write(Path::new("\0"), &[preset("")])),
        ] {
            let _ = report(operation, result);
        }
        let logs = crate::logs::native_logs(0);
        for operation in ["read", "write"] {
            assert!(logs.iter().any(|e| e.source == crate::logs::Source::Deas
                && e.text
                    .starts_with(&format!("ERROR: Studio presets {operation}:"))));
        }
        assert!(logs
            .iter()
            .filter(|e| e.text.starts_with("ERROR: Studio presets"))
            .all(|e| !e.text.contains("private prompt") && !e.text.contains("secret")));
    }

    #[test]
    fn commands_are_registered_with_local_only_permission() {
        let permission = include_str!("../permissions/presets.toml");
        for command in [
            "native_presets_read",
            "native_presets_write",
            "native_presets_invalid",
        ] {
            assert!(include_str!("main.rs").contains(&format!("deas::{command},")));
            assert!(permission.contains(&format!("\"{command}\"")));
        }
        let capability: Value =
            serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
        assert_eq!(capability["local"], true);
        assert_eq!(capability["windows"], json!(["main"]));
        assert!(capability.get("remote").is_none());
        assert!(capability["permissions"]
            .as_array()
            .unwrap()
            .contains(&json!("allow-native-presets")));
    }
}
