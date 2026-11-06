//! Portable draft packages: exact allowlist, bounded decode, no arbitrary extraction.
use super::*;
use std::{collections::BTreeMap, io::Cursor};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const MAX_PACKAGE: u64 = MAX_PNG + MAX_MANIFEST + 8192;

fn package_name(name: &str) -> bool {
    name == MANIFEST
        || name
            .strip_prefix("input-")
            .and_then(|n| n.strip_suffix(".png"))
            .is_some_and(valid_id)
}
fn selected_file(path: &Path) -> Result<(Root, String), String> {
    let parent = path.parent().ok_or("Invalid package path")?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid package filename")?;
    if name.is_empty()
        || name.len() > 240
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
        || name.ends_with([' ', '.'])
    {
        return Err("Unsafe package filename".into());
    }
    let anchor =
        Anchor::open(parent).map_err(|_| "Package path must contain only real directories")?;
    let path = dunce::canonicalize(parent).map_err(|_| "Cannot resolve package directory")?;
    Ok((Root { anchor, path }, name.into()))
}
// Bound central-directory allocation before handing the archive to the ZIP decoder.
// shortcut: classic ZIP without archive comments/multidisk/ZIP64 only; extend for larger project schemas.
fn preflight(bytes: &[u8]) -> Result<usize, String> {
    let bad = || "Invalid or unsupported draft ZIP structure".to_string();
    let end = bytes.len().checked_sub(22).ok_or_else(bad)?;
    let e = &bytes[end..];
    let u16_at = |b: &[u8], n| u16::from_le_bytes(b[n..n + 2].try_into().unwrap()) as usize;
    let u32_at = |b: &[u8], n| u32::from_le_bytes(b[n..n + 4].try_into().unwrap()) as usize;
    let count = u16_at(e, 10);
    let start = u32_at(e, 16);
    let size = u32_at(e, 12);
    if &e[..4] != b"PK\x05\x06"
        || e[4..8] != [0; 4]
        || u16_at(e, 8) != count
        || !(1..=2).contains(&count)
        || u16_at(e, 20) != 0
        || size > 4096
        || start.checked_add(size) != Some(end)
    {
        return Err(bad());
    }
    let mut offset = start;
    let mut names = std::collections::BTreeSet::new();
    for _ in 0..count {
        let header = bytes
            .get(offset..offset.checked_add(46).ok_or_else(bad)?)
            .ok_or_else(bad)?;
        let name_len = u16_at(header, 28);
        let extra = u16_at(header, 30);
        let comment = u16_at(header, 32);
        if &header[..4] != b"PK\x01\x02"
            || name_len > 64
            || extra > 1024
            || comment > 1024
            || u16_at(header, 34) != 0
            || u16_at(header, 8) & 1 != 0
        {
            return Err(bad());
        }
        let name = bytes
            .get(offset + 46..offset + 46 + name_len)
            .ok_or_else(bad)?;
        let name = std::str::from_utf8(name).map_err(|_| bad())?;
        if !package_name(name) || !names.insert(name) {
            return Err(bad());
        }
        offset += 46 + name_len + extra + comment;
        if offset > end {
            return Err(bad());
        }
    }
    if offset != end {
        return Err(bad());
    }
    Ok(count)
}
fn decode(bytes: &[u8]) -> Result<(Manifest, Option<Vec<u8>>), String> {
    let count = preflight(bytes)?;
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|_| "Invalid draft ZIP")?;
    if zip.len() != count {
        return Err("Duplicate ZIP entries".into());
    }
    let mut entries = BTreeMap::new();
    let mut total = 0;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|_| "Invalid or encrypted ZIP entry")?;
        let name = entry
            .name()
            .map_err(|_| "Invalid ZIP filename")?
            .into_owned();
        let limit = if name == MANIFEST {
            MAX_MANIFEST
        } else {
            MAX_PNG
        };
        let kind = entry.unix_mode().unwrap_or(0) & 0o170000;
        if !package_name(&name)
            || entry.is_dir()
            || !matches!(kind, 0 | 0o100000)
            || !matches!(
                entry.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
            || entry.size() > limit
        {
            return Err("Unsafe or oversized ZIP entry".into());
        }
        let size = entry.size();
        let mut data = Vec::new();
        entry
            .by_ref()
            .take(limit + 1)
            .read_to_end(&mut data)
            .map_err(|_| "Corrupt ZIP entry")?;
        total += data.len() as u64;
        if data.len() as u64 != size
            || total > MAX_PNG + MAX_MANIFEST
            || entries.insert(name, data).is_some()
        {
            return Err("Invalid ZIP entry size or duplicate".into());
        }
    }
    let manifest = entries.remove(MANIFEST).ok_or("Package manifest missing")?;
    let data: Manifest =
        serde_json::from_slice(&manifest).map_err(|_| "Invalid project manifest")?;
    validate_manifest(&data)?;
    let image = if let Some(input) = &data.input {
        validate_input_reference(input)?;
        let image = entries
            .remove(&input.path)
            .ok_or("Managed image missing from package")?;
        validate_input_bytes(input, &image)?;
        Some(image)
    } else {
        None
    };
    if !entries.is_empty() {
        return Err("Unexpected package entries".into());
    }
    Ok((data, image))
}
impl Projects {
    pub fn export_package(
        &self,
        id: &str,
        revision: u64,
        destination: Option<&Path>,
    ) -> Result<bool, String> {
        let Some(destination) = destination else {
            crate::logs::push(
                crate::logs::Source::Deas,
                "INFO: Projects operation=export-package cancelled",
            );
            return Ok(false);
        };
        report(
            "export-package",
            Some(id),
            (|| {
                let active = self.0.lock().map_err(|_| "Project state unavailable")?;
                let current = active.as_ref().ok_or("No active project")?;
                current.check(id, revision)?;
                validate_manifest(&current.data)?;
                let image = if let Some(input) = &current.data.input {
                    validate_input_reference(input)?;
                    let bytes = current.root.read(&input.path, MAX_PNG)?;
                    validate_input_bytes(input, &bytes)?;
                    Some(bytes)
                } else {
                    None
                };
                let manifest = current.root.read(MANIFEST, MAX_MANIFEST)?;
                let committed: Manifest =
                    serde_json::from_slice(&manifest).map_err(|_| "Invalid project manifest")?;
                if committed != current.data {
                    return Err("Project manifest changed outside DEAS".into());
                }
                let (target, name) = selected_file(destination)?;
                target.anchor.writable()?;
                target.anchor.absent(&name)?;
                let temporary = format!(".package-{}.tmp", uuid::Uuid::new_v4());
                let mut file = target.anchor.partial(&temporary)?;
                let result = (|| {
                    let mut zip = ZipWriter::new(&mut file);
                    let options = SimpleFileOptions::default()
                        .compression_method(CompressionMethod::Stored)
                        .unix_permissions(0o600);
                    zip.start_file(MANIFEST, options)
                        .map_err(|_| "Cannot write package manifest")?;
                    zip.write_all(&manifest)
                        .map_err(|_| "Cannot write package manifest")?;
                    if let (Some(input), Some(bytes)) = (&current.data.input, &image) {
                        zip.start_file(&input.path, options)
                            .map_err(|_| "Cannot write package image")?;
                        zip.write_all(bytes)
                            .map_err(|_| "Cannot write package image")?;
                    }
                    zip.finish().map_err(|_| "Cannot finish package")?;
                    file.sync_all().map_err(|_| "Cannot sync package")?;
                    current.check(id, revision)?;
                    target.verify()?;
                    if !target.anchor.owns_partial(&temporary, &file) {
                        return Err("Package temporary file replaced".into());
                    }
                    target.anchor.publish(&temporary, &name)?;
                    Ok(true)
                })();
                if target.anchor.owns_partial(&temporary, &file) {
                    target.anchor.remove_partial(&temporary);
                }
                result
            })(),
        )
    }
    pub fn import_package(
        &self,
        source: Option<&Path>,
        parent: Option<&Path>,
    ) -> Result<Option<ProjectSnapshot>, String> {
        report_selection(
            "import-package",
            None,
            (|| {
                let (Some(source), Some(parent)) = (source, parent) else {
                    return Ok(None);
                };
                let (source_root, name) = selected_file(source)?;
                let (data, image) = decode(&source_root.read(&name, MAX_PACKAGE)?)?;
                let mut active = self.0.lock().map_err(|_| "Project state unavailable")?;
                let parent = Root::open(parent)?;
                let child = format!("deas-project-{}", uuid::Uuid::new_v4());
                let anchor = parent.anchor.create(&child)?;
                let root = Root {
                    anchor,
                    path: parent.path.join(child),
                };
                let lock = acquire(&root)?;
                // Failed writes may leave an unactivated unique child; never recursively delete paths on failure.
                if let (Some(input), Some(bytes)) = (&data.input, &image) {
                    root.verify()?;
                    let mut file = root.anchor.partial(&input.path)?;
                    file.write_all(bytes)
                        .map_err(|_| "Cannot write imported image")?;
                    file.sync_all().map_err(|_| "Cannot sync imported image")?;
                    validate_input(&root, input)?;
                }
                root.write(&data, true)?;
                parent.sync()?;
                let next = Active { root, data, lock };
                let snapshot = next.snapshot();
                *active = Some(next);
                Ok(Some(snapshot))
            })(),
        )
    }
}
#[tauri::command]
pub async fn native_project_export_package(
    title: String,
    project_id: String,
    expected_revision: u64,
    state: tauri::State<'_, Projects>,
) -> Result<bool, String> {
    validate_title(&title)?;
    let selected = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter("DEAS draft ZIP", &["zip"])
        .set_file_name("deas-draft.zip")
        .save_file()
        .await;
    let path = selected.map(|f| f.path().to_path_buf());
    let projects = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        projects.export_package(&project_id, expected_revision, path.as_deref())
    })
    .await
    .map_err(|_| "Project package worker unavailable")?
}
#[tauri::command]
pub async fn native_project_import_package(
    title: String,
    state: tauri::State<'_, Projects>,
) -> Result<Option<ProjectSnapshot>, String> {
    validate_title(&title)?;
    let selected = rfd::AsyncFileDialog::new()
        .set_title(&title)
        .add_filter("DEAS draft ZIP", &["zip"])
        .pick_file()
        .await;
    let Some(source) = selected else {
        return state.import_package(None, None);
    };
    let parent = rfd::AsyncFileDialog::new()
        .set_title(title)
        .pick_folder()
        .await;
    let source = source.path().to_path_buf();
    let parent = parent.map(|f| f.path().to_path_buf());
    let projects = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        projects.import_package(Some(&source), parent.as_deref())
    })
    .await
    .map_err(|_| "Project package worker unavailable")?
}
