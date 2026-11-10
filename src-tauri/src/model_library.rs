//! Virtual organization only. Model paths are opaque identities, never move/delete targets.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    io::{Read, Write},
    path::Path,
    sync::Mutex,
};
use tauri::Manager;

const FILE: &str = "deas-model-library.json";
const LIMIT: usize = 8 * 1024 * 1024;
static WRITE: Mutex<()> = Mutex::new(());
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub folder: Option<String>,
    pub tags: Vec<String>,
    pub kind: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub thumbnail: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavedView {
    pub name: String,
    pub scope: String,
    pub mode: String,
    pub folder: String,
    pub kind: String,
    pub family: String,
    pub tag: String,
    pub query: String,
    pub order: String,
    pub pinned: bool,
    pub missing: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Library {
    pub schema_version: u32,
    pub revision: u64,
    pub folders: Vec<Folder>,
    pub entries: BTreeMap<String, Entry>,
    pub order: Vec<String>,
    #[serde(default)]
    pub views: Vec<SavedView>,
}
impl Default for Library {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            folders: vec![],
            entries: BTreeMap::new(),
            order: vec![],
            views: vec![],
        }
    }
}
fn text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
fn validate(v: &Library) -> Result<(), String> {
    if v.schema_version != 1
        || v.folders.len() > 500
        || v.entries.len() > 10000
        || v.order.len() > 10000
    {
        return Err("Unsupported or oversized model library".into());
    }
    if v.views.len() > 100
        || v.views.iter().any(|w| {
            !text(&w.name, 100)
                || !text(&w.scope, 100)
                || !matches!(w.mode.as_str(), "disk" | "auto" | "manual")
                || !matches!(w.order.as_str(), "default" | "type" | "manual")
                || [&w.folder, &w.kind, &w.family, &w.tag, &w.query]
                    .iter()
                    .any(|s| s.len() > 1000 || s.chars().any(char::is_control))
        })
    {
        return Err("Invalid saved library view".into());
    }
    let mut view_names = HashSet::new();
    if v.views
        .iter()
        .any(|w| !view_names.insert((&w.scope, w.name.to_lowercase())))
    {
        return Err("Saved view names must be unique".into());
    }
    if v.entries.values().any(|e| {
        e.notes.len() > 8000
            || e.thumbnail.as_ref().is_some_and(|s| {
                s.len() > 100 || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.')
            })
    }) {
        return Err("Invalid notes or cached thumbnail identity".into());
    }
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for f in &v.folders {
        if !text(&f.id, 100)
            || !text(&f.name, 150)
            || !ids.insert(f.id.as_str())
            || !names.insert((f.parent.as_deref(), f.name.trim().to_lowercase()))
        {
            return Err(
                "Folder names and identities must be valid and unique within their parent".into(),
            );
        }
    }
    for f in &v.folders {
        let mut parent = f.parent.as_deref();
        let mut seen = HashSet::from([f.id.as_str()]);
        while let Some(id) = parent {
            if !seen.insert(id) {
                return Err("A virtual folder cannot contain itself".into());
            }
            parent = v
                .folders
                .iter()
                .find(|p| p.id == id)
                .ok_or("Unknown parent folder")?
                .parent
                .as_deref();
            if seen.len() > 16 {
                return Err("Virtual folder depth is limited to 16".into());
            }
        }
    }
    for (path, e) in &v.entries {
        if !text(path, 32768)
            || e.folder.as_deref().is_some_and(|f| !ids.contains(f))
            || e.notes.chars().count() > 8000
            || e.notes.contains('\0')
            || e.thumbnail.as_ref().is_some_and(|v| {
                v.len() > 100 || v.contains('/') || v.contains('\\') || v.contains("..")
            })
            || e.tags.len() > 40
            || (!e.kind.is_empty() && !text(&e.kind, 100))
            || e.tags.iter().any(|t| !text(t, 100))
        {
            return Err("Invalid model organization entry".into());
        }
        let tags: HashSet<_> = e.tags.iter().map(|t| t.to_lowercase()).collect();
        if tags.len() != e.tags.len() {
            return Err("Duplicate tags".into());
        }
    }
    let mut paths = HashSet::new();
    if v.order.iter().any(|p| !text(p, 32768) || !paths.insert(p)) {
        return Err("Invalid manual model order".into());
    }
    Ok(())
}
fn read(root: &Path) -> Result<Library, String> {
    let path = root.join(FILE);
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Library::default()),
        Ok(m) if m.is_file() && !m.file_type().is_symlink() => (),
        _ => return Err("Model library is not a regular readable file".into()),
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(LIMIT as u64 + 1).read_to_end(&mut bytes))
        .map_err(|_| "Cannot read model library")?;
    if bytes.len() > LIMIT {
        return Err("Model library is too large".into());
    }
    let value: Library = serde_json::from_slice(crate::bom_free(&bytes))
        .map_err(|_| "Invalid model library JSON; original retained")?;
    validate(&value)?;
    Ok(value)
}
fn save(root: &Path, mut value: Library) -> Result<Library, String> {
    let _lock = WRITE.lock().map_err(|_| "Model library lock unavailable")?;
    validate(&value)?;
    let old = read(root)?;
    if old.revision != value.revision {
        return Err("Model library changed elsewhere. Reload before saving your edits.".into());
    }
    value.revision = value
        .revision
        .checked_add(1)
        .ok_or("Model library revision exhausted")?;
    let bytes = serde_json::to_vec(&value).map_err(|_| "Cannot encode model library")?;
    if bytes.len() > LIMIT {
        return Err("Model library is too large".into());
    }
    std::fs::create_dir_all(root).map_err(|_| "Cannot create model library directory")?;
    let mut temp = tempfile::NamedTempFile::new_in(root)
        .map_err(|_| "Cannot create model library temporary file")?;
    temp.write_all(&bytes)
        .and_then(|_| temp.as_file().sync_all())
        .map_err(|_| "Cannot save model library")?;
    temp.persist(root.join(FILE))
        .map_err(|_| "Cannot replace model library")?;
    Ok(value)
}
fn report<T>(_op: &str, result: Result<T, String>) -> Result<T, String> {
    if result.is_err() {
        crate::workspace_log::native_logs_workspace_error("library".into());
    }
    result
}
#[tauri::command]
pub fn native_model_library_read(app: tauri::AppHandle) -> Result<Library, String> {
    report(
        "read",
        app.path()
            .app_local_data_dir()
            .map_err(|_| "Cannot locate model library".into())
            .and_then(|p| read(&p)),
    )
}
#[tauri::command]
pub fn native_model_library_write(
    app: tauri::AppHandle,
    library: Library,
) -> Result<Library, String> {
    report(
        "write",
        app.path()
            .app_local_data_dir()
            .map_err(|_| "Cannot locate model library".into())
            .and_then(|p| save(&p, library)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn virtual_library_roundtrip_conflict_and_original_file_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("model.safetensors");
        std::fs::write(&original, b"unchanged").unwrap();
        let mut v = Library::default();
        v.folders.push(Folder {
            id: "a".into(),
            name: "Motion".into(),
            parent: None,
        });
        v.entries.insert(
            original.to_string_lossy().into(),
            Entry {
                folder: Some("a".into()),
                tags: vec!["Band".into()],
                kind: "Motion".into(),
                ..Default::default()
            },
        );
        let saved = save(dir.path(), v.clone()).unwrap();
        assert_eq!(read(dir.path()).unwrap(), saved);
        assert_eq!(saved.revision, 1);
        assert!(save(dir.path(), v)
            .unwrap_err()
            .contains("changed elsewhere"));
        assert_eq!(std::fs::read(original).unwrap(), b"unchanged");
    }
    #[test]
    fn virtual_library_rejects_cycles_and_unknown_folders() {
        let mut v = Library::default();
        v.folders = vec![
            Folder {
                id: "a".into(),
                name: "A".into(),
                parent: Some("b".into()),
            },
            Folder {
                id: "b".into(),
                name: "B".into(),
                parent: Some("a".into()),
            },
        ];
        assert!(validate(&v).is_err());
        v.folders.clear();
        v.entries.insert(
            "opaque-model-key".into(),
            Entry {
                folder: Some("missing".into()),
                ..Default::default()
            },
        );
        assert!(validate(&v).is_err());
    }
    #[test]
    fn virtual_library_corrupt_data_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), b"broken").unwrap();
        assert!(save(dir.path(), Library::default()).is_err());
        assert_eq!(std::fs::read(dir.path().join(FILE)).unwrap(), b"broken");
    }
}
