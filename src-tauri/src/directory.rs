//! A native folder chooser, not a browser upload or a filesystem mutation.
use std::path::PathBuf;

fn validate_title(title: &str) -> Result<(), String> {
    if title.trim().is_empty() || title.len() > 240 || title.chars().any(char::is_control) {
        return Err("Directory chooser title must be nonempty, at most 240 bytes and contain no control characters".into());
    }
    Ok(())
}

fn selected_directory(path: Option<PathBuf>) -> Result<Option<String>, String> {
    let Some(path) = path else { return Ok(None) };
    if !path.is_absolute() || !path.is_dir() {
        return Err("The chooser did not return an existing absolute directory".into());
    }
    // The chooser may return a symlink alias; bind the real parent before planning.
    let path =
        dunce::canonicalize(path).map_err(|e| format!("Cannot resolve selected directory: {e}"))?;
    path.to_str()
        .map(|value| Some(value.to_owned()))
        .ok_or_else(|| "The selected directory cannot be represented as a UTF-8 path".into())
}

/// Selecting a parent never creates or installs anything; cancellation is null.
#[tauri::command]
pub async fn native_pick_directory(title: String) -> Result<Option<String>, String> {
    validate_title(&title)?;
    let chosen = rfd::AsyncFileDialog::new()
        .set_title(title)
        .set_can_create_directories(false)
        .pick_folder()
        .await;
    selected_directory(chosen.map(|handle| handle.path().to_owned()))
}

/// JSON documents the user saves or opens through OS dialogs (e.g. Director reference packs): small and well-formed only.
const MAX_JSON_FILE: u64 = 1024 * 1024;

fn checked_json(text: &str) -> Result<(), String> {
    if text.len() as u64 > MAX_JSON_FILE {
        return Err("JSON file exceeds 1 MiB".into());
    }
    serde_json::from_str::<serde_json::Value>(text)
        .map(drop)
        .map_err(|e| format!("Not a JSON document: {e}"))
}

/// Save dialog for one JSON document; cancellation is `false`, nothing else is written.
#[tauri::command]
pub async fn native_json_save(title: String, name: String, text: String) -> Result<bool, String> {
    validate_title(&title)?;
    if name.len() > 120
        || !name.ends_with(".json")
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
    {
        return Err("Invalid JSON file name".into());
    }
    checked_json(&text)?;
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter("JSON", &["json"])
        .set_file_name(name)
        .save_file()
        .await
    else {
        return Ok(false);
    };
    std::fs::write(file.path(), text).map_err(|e| format!("Cannot write JSON file: {e}"))?;
    Ok(true)
}

fn read_json(path: &std::path::Path) -> Result<String, String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("Cannot read JSON file: {e}"))?
        .len();
    if size > MAX_JSON_FILE {
        return Err("JSON file exceeds 1 MiB".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("Cannot read JSON file: {e}"))?;
    // Returned stripped: the frontend parses this text and JSON.parse rejects a BOM too.
    let text = crate::strip_bom(&text);
    checked_json(text)?;
    Ok(text.to_string())
}

/// Open dialog for one JSON document; returns its text (validated JSON, ≤ 1 MiB) or `None` on cancel.
#[tauri::command]
pub async fn native_json_open(title: String) -> Result<Option<String>, String> {
    validate_title(&title)?;
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter("JSON", &["json"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    read_json(file.path()).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_files_are_size_capped_and_must_parse() {
        let dir = tempfile::tempdir().unwrap();
        let ok = dir.path().join("pack.json");
        std::fs::write(&ok, r#"{"items":[]}"#).unwrap();
        assert_eq!(read_json(&ok).unwrap(), r#"{"items":[]}"#);
        // Editors that write a UTF-8 BOM (issue #27): reads are BOM-tolerant and return stripped text.
        let bom = dir.path().join("bom.json");
        std::fs::write(
            &bom,
            [b"\xef\xbb\xbf".as_slice(), br#"{"items":[]}"#].concat(),
        )
        .unwrap();
        assert_eq!(read_json(&bom).unwrap(), r#"{"items":[]}"#);
        let bad = dir.path().join("bad.json");
        std::fs::write(&bad, "not json").unwrap();
        assert!(read_json(&bad).is_err());
        let big = dir.path().join("big.json");
        std::fs::write(&big, vec![b' '; MAX_JSON_FILE as usize + 1]).unwrap();
        assert!(read_json(&big).unwrap_err().contains("1 MiB"));
        assert!(checked_json("[1,").is_err());
    }

    #[test]
    fn cancellation_is_not_a_path_or_error() {
        assert_eq!(selected_directory(None).unwrap(), None);
    }

    #[test]
    fn returns_exact_existing_parent_without_creating_target() {
        let parent = tempfile::tempdir().unwrap();
        let selected = selected_directory(Some(parent.path().to_owned()))
            .unwrap()
            .unwrap();
        assert_eq!(
            selected,
            dunce::canonicalize(parent.path())
                .unwrap()
                .to_str()
                .unwrap()
        );
        assert!(!parent.path().join("deas-environment").exists());
        assert!(selected_directory(Some(parent.path().join("missing"))).is_err());
        let file = parent.path().join("not-a-directory");
        std::fs::write(&file, b"fixture").unwrap();
        assert!(selected_directory(Some(file)).is_err());
        assert!(selected_directory(Some(PathBuf::from("relative"))).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn selected_symlink_resolves_without_creating_or_changing_anything() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        assert_eq!(
            selected_directory(Some(alias.clone())).unwrap(),
            Some(real.to_str().unwrap().into())
        );
        assert!(alias.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(!real.join("deas-environment").exists());
    }
    #[test]
    fn validates_dialog_title_before_opening() {
        assert!(validate_title("Speicherordner wählen").is_ok());
        for title in ["", " ", "bad\ntext", "bad\0title"] {
            assert!(validate_title(title).is_err());
        }
        assert!(validate_title(&"x".repeat(241)).is_err());
    }

    #[tokio::test]
    #[ignore = "Opens a real OS directory chooser; explicitly select or cancel"]
    async fn native_directory_picker_interactive() {
        let chosen = native_pick_directory("DEAS — directory picker verification".into())
            .await
            .unwrap();
        println!("Native picker returned: {chosen:?}");
        if let Some(path) = chosen {
            assert!(std::path::Path::new(&path).is_dir());
        }
    }
}
