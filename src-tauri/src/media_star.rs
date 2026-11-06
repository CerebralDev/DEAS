//! Physical starring inside the native-authorized output root.
use crate::downloads::fs::Anchor;
use std::path::{Component, Path, PathBuf};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StarStatus {
    pub starred: bool,
    pub unavailable: Option<String>,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StarMove {
    pub old_path: String,
    pub path: String,
}
pub(crate) fn output_root(engine: &crate::install::Engine) -> Result<PathBuf, String> {
    let plan = engine
        .approved_plan()
        .ok_or("Select an owned installed environment; external media cannot be starred")?;
    let environment = Path::new(&plan.request.storage_path);
    let folders = crate::settings::read(environment)?;
    Ok(folders
        .output
        .map(PathBuf::from)
        .unwrap_or_else(|| environment.join("ComfyUI/output")))
}
pub(crate) fn validate(root: &Path, source: &Path) -> Result<bool, String> {
    preflight(root, source).map(|(starred, _)| starred)
}
fn preflight(root: &Path, source: &Path) -> Result<(bool, std::fs::File), String> {
    if !source.is_absolute()
        || source.components().any(|p| {
            !matches!(
                p,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
    {
        return Err("Media path must be absolute without traversal".into());
    }
    let relative = source
        .strip_prefix(root)
        .map_err(|_| "External/unmanaged media cannot be moved")?;
    if relative.as_os_str().is_empty() {
        return Err("Select a media file".into());
    }
    Anchor::open(root)?.writable()?;
    Anchor::open(source.parent().ok_or("Missing media parent")?)?.writable()?;
    // Project assets require a manifest transaction, not an out-of-band filesystem edit.
    if source
        .ancestors()
        .skip(1)
        .any(|p| p.join(crate::projects::MANIFEST).exists())
    {
        return Err(
            "Project media requires project reference repair; starring is unavailable".into(),
        );
    }
    let meta = std::fs::symlink_metadata(source).map_err(|_| "Media is missing or unreadable")?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.permissions().readonly() {
        return Err("Only writable regular generated media can be starred".into());
    }

    check_sidecars(source)?;
    let file = open_media(source)?;
    let parent = Anchor::open(source.parent().ok_or("Missing media parent")?)?;
    let name = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid media filename")?;
    if !parent.owns_partial(name, &file) {
        return Err("Media identity changed".into());
    }
    if crate::media::media_kind(name).is_none() {
        return Err("Starring unsupported media extension is unavailable".into());
    }
    Ok((relative.starts_with("starred"), file))
}
// Unstar removes only the first starred/ component; no receipt or history is required.
fn check_unstar_target(root: &Path, source: &Path) -> Result<PathBuf, String> {
    let relative = source
        .strip_prefix(root)
        .map_err(|_| "External/unmanaged media cannot be moved")?
        .strip_prefix("starred")
        .map_err(|_| "Not a starred file")?;
    let destination = root.join("raw").join(relative);
    if destination
        .ancestors()
        .skip(1)
        .any(|p| p.join(crate::projects::MANIFEST).exists())
    {
        return Err(
            "Project media requires project reference repair; unstar is unavailable".into(),
        );
    }
    let mut parent = root.to_path_buf();
    let relative = Path::new("raw").join(relative);
    for part in relative
        .parent()
        .ok_or("Missing target parent")?
        .components()
    {
        let Component::Normal(name) = part else {
            return Err("Unsafe target path".into());
        };
        parent.push(name);
        match std::fs::symlink_metadata(&parent) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            _ => Anchor::open(&parent)?.writable()?,
        }
    }
    match std::fs::symlink_metadata(&destination) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => return Err("Output filename is occupied; starred file was not moved".into()),
    }
    check_sidecars(&destination)?;
    Ok(destination)
}
fn check_sidecars(source: &Path) -> Result<(), String> {
    for extension in ["json", "txt", "xmp"] {
        if [
            source.with_extension(extension),
            PathBuf::from(format!("{}.{}", source.display(), extension)),
        ]
        .iter()
        .any(|path| !matches!(std::fs::symlink_metadata(path), Err(error) if error.kind() == std::io::ErrorKind::NotFound))
        {
            return Err(
                "Media with associated sidecars requires a grouped move; starring is unavailable"
                    .into(),
            );
        }
    }
    Ok(())
}
fn open_media(source: &Path) -> Result<std::fs::File, String> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(source)
            .map_err(|_| "Cannot open regular media without following symlinks".into())
    }
    #[cfg(windows)]
    {
        crate::win::open_no_follow(source)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = source;
        Err("Safe starring requires Linux or Windows".into())
    }
}
fn unchanged(file: &std::fs::File, before: &std::fs::Metadata) -> Result<(), String> {
    let after = file.metadata().map_err(|_| "Cannot inspect media")?;
    if after.len() != before.len()
        || after.modified().ok().is_none()
        || after.modified().ok() != before.modified().ok()
    {
        return Err("Media is still changing; retry after writing finishes".into());
    }
    Ok(())
}
pub(crate) fn move_media(root: &Path, source: &Path) -> Result<PathBuf, String> {
    move_media_checked(root, source, |_| {})
}
fn same_directory(path: &Path, held: &Anchor) -> Result<(), String> {
    if Anchor::open(path)?.identity()? != held.identity()? {
        return Err("Media directory namespace changed".into());
    }
    Ok(())
}
// Checkpoints let tests place external renames exactly around the transaction.
pub(crate) fn move_media_checked(
    root: &Path,
    source: &Path,
    checkpoint: impl FnMut(bool),
) -> Result<PathBuf, String> {
    relocate_checked(root, source, false, checkpoint)
}
pub(crate) fn unstar_media(root: &Path, source: &Path) -> Result<PathBuf, String> {
    relocate_checked(root, source, true, |_| {})
}
fn relocate_checked(
    root: &Path,
    source: &Path,
    unstar: bool,
    mut checkpoint: impl FnMut(bool),
) -> Result<PathBuf, String> {
    let held_root = Anchor::open(root)?;
    let source_parent = source.parent().ok_or("Missing media parent")?;
    let parent = Anchor::open(source_parent)?;
    let (starred, file) = preflight(root, source)?;
    if starred && !unstar {
        return Ok(source.to_path_buf());
    }
    if !starred && unstar {
        return Err("Select a starred file to unstar".into());
    }
    if unstar {
        check_unstar_target(root, source)?;
    }
    let before = file.metadata().map_err(|_| "Cannot inspect media")?;
    let mut target = Anchor::open(root)?;
    let mut destination = root.to_path_buf();
    let relative = source
        .strip_prefix(root)
        .map_err(|_| "External/unmanaged media cannot be moved")?;
    let folders = if unstar {
        Path::new("raw").join(
            relative
                .strip_prefix("starred")
                .map_err(|_| "Not a starred file")?
                .parent()
                .ok_or("Missing relative parent")?,
        )
    } else {
        Path::new("starred").join(
            relative
                .strip_prefix("raw")
                .unwrap_or(relative)
                .parent()
                .ok_or("Missing relative parent")?,
        )
    };
    for part in folders.components() {
        let Component::Normal(name) = part else {
            return Err("Unsafe target path".into());
        };
        let name = name.to_str().ok_or("Invalid target folder")?;
        destination.push(name);
        target = match Anchor::open(&destination) {
            Ok(anchor) => anchor,
            Err(_) => target.create(name)?,
        };
        target.writable()?;
    }
    let name = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid media filename")?;
    if !parent.owns_partial(name, &file) {
        return Err("Media identity changed".into());
    }
    let namespace = || {
        same_directory(root, &held_root)?;
        same_directory(source_parent, &parent)?;
        same_directory(&destination, &target)
    };
    for attempt in 0..if unstar { 1 } else { 100 } {
        let candidate = if attempt == 0 {
            name.to_owned()
        } else {
            format!(
                "{}-{}.{}",
                source.file_stem().unwrap().to_string_lossy(),
                attempt,
                source.extension().unwrap().to_string_lossy()
            )
        };
        if target.absent(&candidate).is_err() {
            if unstar {
                return Err("Output filename is occupied; starred file was not moved".into());
            }
            continue;
        }
        // shortcut: this bounded observation cannot exclude future external writers; native jobs stay locked out.
        std::thread::sleep(std::time::Duration::from_millis(50));
        unchanged(&file, &before)?;
        if !parent.owns_partial(name, &file) {
            return Err("Media identity changed".into());
        }
        parent.writable()?;
        target.writable()?;
        namespace()?;
        let result = destination.join(&candidate);
        check_sidecars(&result)?;
        checkpoint(false);
        unchanged(&file, &before)?;
        if !parent.owns_partial(name, &file) {
            return Err("Media identity changed".into());
        }
        check_sidecars(source)?;
        check_sidecars(&result)?;
        namespace()?;
        rename_no_replace(&parent, name, &target, &candidate, source, &result)?;
        checkpoint(true);
        let verified = namespace().is_ok()
            && unchanged(&file, &before).is_ok()
            && Anchor::open(&destination).is_ok_and(|current| {
                current
                    .identity()
                    .is_ok_and(|identity| target.identity() == Ok(identity))
                    && current.owns_partial(&candidate, &file)
            });
        if !verified {
            // Never roll back a replacement or clobber a concurrently recreated source.
            let restored = target.owns_partial(&candidate, &file)
                && rename_no_replace(&target, &candidate, &parent, name, &result, source).is_ok()
                && parent.owns_partial(name, &file);
            if restored
                && same_directory(root, &held_root).is_ok()
                && same_directory(source_parent, &parent).is_ok()
            {
                return Err("Media namespace changed during move; rolled back to source".into());
            }
            return Err("Media namespace changed during move; source path could not be confirmed restored. Media may remain in the renamed source/destination directory; inspect before retrying".into());
        }
        return Ok(result);
    }
    Err("Cannot find an unused starred filename".into())
}
fn rename_no_replace(
    from: &Anchor,
    name: &str,
    to: &Anchor,
    target: &str,
    source: &Path,
    destination: &Path,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        let (name, target) = (
            std::ffi::CString::new(name).map_err(|_| "Invalid filename")?,
            std::ffi::CString::new(target).map_err(|_| "Invalid filename")?,
        );
        let _ = (source, destination);
        if unsafe {
            libc::renameat2(
                from.0.as_raw_fd(),
                name.as_ptr(),
                to.0.as_raw_fd(),
                target.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        } != 0
        {
            return Err("Cannot move media without overwrite".into());
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        let _ = (name, target);
        let source_parent = Anchor::open(source.parent().ok_or("Missing source parent")?)?;
        let destination_parent =
            Anchor::open(destination.parent().ok_or("Missing destination parent")?)?;
        if source_parent.identity()? != from.identity()?
            || destination_parent.identity()? != to.identity()?
        {
            return Err("Media directory identity changed".into());
        }
        source_parent.writable()?;
        destination_parent.writable()?;
        crate::win::rename_no_replace(source, destination)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (from, name, to, target, source, destination);
        Err("Safe starring requires Linux or Windows".into())
    }
}
#[tauri::command]
pub fn native_media_star_status(
    path: String,
    engine: tauri::State<'_, crate::install::Engine>,
    jobs: tauri::State<'_, crate::jobs::Jobs>,
) -> StarStatus {
    match output_root(&engine) {
        Ok(root) => star_status(&root, Path::new(&path), &jobs),
        Err(reason) => StarStatus {
            starred: false,
            unavailable: Some(reason),
        },
    }
}
fn star_status(root: &Path, source: &Path, jobs: &crate::jobs::Jobs) -> StarStatus {
    let starred = source
        .strip_prefix(root)
        .is_ok_and(|relative| relative.starts_with("starred"));
    let result = jobs
        .media_move_idle()
        .and_then(|_| jobs.media_move_ready(root, source))
        .and_then(|_| {
            if starred {
                check_unstar_target(root, source)?;
            }
            Ok(())
        });
    StarStatus {
        starred,
        unavailable: result.err(),
    }
}
#[tauri::command]
pub fn native_media_star(
    path: String,
    engine: tauri::State<'_, crate::install::Engine>,
    jobs: tauri::State<'_, crate::jobs::Jobs>,
    operations: tauri::State<'_, crate::FolderOperations>,
) -> Result<StarMove, String> {
    move_owned(path, &engine, &jobs, &operations, false)
}
#[tauri::command]
pub fn native_media_unstar(
    path: String,
    engine: tauri::State<'_, crate::install::Engine>,
    jobs: tauri::State<'_, crate::jobs::Jobs>,
    operations: tauri::State<'_, crate::FolderOperations>,
) -> Result<StarMove, String> {
    move_owned(path, &engine, &jobs, &operations, true)
}
fn move_owned(
    path: String,
    engine: &crate::install::Engine,
    jobs: &crate::jobs::Jobs,
    operations: &crate::FolderOperations,
    unstar: bool,
) -> Result<StarMove, String> {
    let result = (|| {
        let _guard = operations.enter()?;
        let root = output_root(engine)?;
        let source = Path::new(&path);
        let target = jobs.relocate_media(&root, source, || {
            if unstar {
                unstar_media(&root, source)
            } else {
                move_media(&root, source)
            }
        })?;
        Ok(StarMove {
            old_path: path.clone(),
            path: target.to_str().ok_or("Non-Unicode media path")?.to_owned(),
        })
    })();
    crate::logs::push(
        crate::logs::Source::Deas,
        match (unstar, result.is_ok()) {
            (false, true) => "INFO: Media operation=star completed",
            (true, true) => "INFO: Media operation=unstar completed",
            (false, false) => {
                "ERROR: Media operation=star failed; inspect the returned error for recovery status"
            }
            (true, false) => {
                "ERROR: Media operation=unstar failed; inspect the returned error for recovery status"
            }
        },
    );
    result
}
#[tauri::command]
pub fn native_media_star_notice() {
    static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last.is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(60)) {
        return;
    }
    *last = Some(std::time::Instant::now());
    crate::logs::push(
        crate::logs::Source::Deas,
        "ERROR: Media operation=star-transport native availability/move response unavailable or invalid (repeats limited to once per minute)",
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    fn png_bytes() -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Grayscale);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[127])
                .unwrap();
        }
        bytes
    }
    #[test]
    fn legacy_star_unstars_to_output_with_its_relative_path_without_receipts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let source = root.join("starred/image/2026-10-19/11-08-52-雪.png");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"legacy image").unwrap();
        let status = star_status(&root, &source, &crate::jobs::Jobs::default());
        assert!(
            status.starred && status.unavailable.is_none(),
            "{:?}",
            status.unavailable
        );
        let returned = unstar_media(&root, &source).unwrap();
        assert_eq!(returned, root.join("raw/image/2026-10-19/11-08-52-雪.png"));
        assert_eq!(std::fs::read(returned).unwrap(), b"legacy image");
        assert!(!source.exists());
    }
    #[test]
    fn raw_star_roundtrip_preserves_nested_path_without_receipts() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let source = root.join("raw/image/2026-10-19/雪.png");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"original bytes").unwrap();
        let target = move_media(&root, &source).unwrap();
        assert_eq!(target, root.join("starred/image/2026-10-19/雪.png"));
        assert!(!root.join(".deas-stars").exists());
        let restarted = crate::jobs::Jobs::default();
        assert_eq!(
            restarted
                .relocate_media(&root, &target, || unstar_media(&root, &target))
                .unwrap(),
            source
        );
        assert_eq!(std::fs::read(&source).unwrap(), b"original bytes");
        assert!(!target.exists());
        assert_eq!(
            unstar_media(&root, &move_media(&root, &source).unwrap()).unwrap(),
            source
        );
    }
    #[test]
    fn unstar_blocks_occupied_relative_target_and_accepts_existing_stars() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let source = root.join("starred/nested/a.png");
        let destination = root.join("raw/nested/a.png");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
        std::fs::write(&source, b"original").unwrap();
        std::fs::write(&destination, b"occupied").unwrap();
        let status = star_status(&root, &source, &crate::jobs::Jobs::default());
        assert!(status.starred && status.unavailable.unwrap().contains("occupied"));
        assert!(unstar_media(&root, &source).is_err());
        assert_eq!(std::fs::read(&destination).unwrap(), b"occupied");
        assert_eq!(std::fs::read(&source).unwrap(), b"original");
        std::fs::remove_file(&destination).unwrap();
        assert_eq!(unstar_media(&root, &source).unwrap(), destination);
        assert!(unstar_media(&root, &destination).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn unstar_rejects_symlinked_raw_and_project_destinations() {
        for project in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let root = dunce::canonicalize(dir.path()).unwrap();
            let source = root.join("starred/image/a.png");
            std::fs::create_dir_all(source.parent().unwrap()).unwrap();
            std::fs::write(&source, b"original").unwrap();
            let outside = tempfile::tempdir().unwrap();
            if project {
                std::fs::create_dir_all(root.join("raw/image")).unwrap();
                std::fs::write(
                    root.join("raw/image").join(crate::projects::MANIFEST),
                    b"{}",
                )
                .unwrap();
            } else {
                std::os::unix::fs::symlink(outside.path(), root.join("raw")).unwrap();
            }
            let status = star_status(&root, &source, &crate::jobs::Jobs::default());
            assert!(status.starred && status.unavailable.is_some());
            assert!(unstar_media(&root, &source).is_err());
            assert_eq!(std::fs::read(&source).unwrap(), b"original");
            assert!(!root.join("raw/image/a.png").exists());
            assert!(!outside.path().join("image/a.png").exists());
        }
    }
    #[test]
    fn unstar_rejects_a_file_changed_during_the_rename() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let source = root.join("raw/a.png");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"original").unwrap();
        let target = move_media(&root, &source).unwrap();
        let error = relocate_checked(&root, &target, true, |committed| {
            if committed {
                std::fs::write(&source, b"unfinished writer").unwrap();
            }
        })
        .unwrap_err();
        assert!(error.contains("rolled back"), "{error}");
        assert!(!source.exists());
        assert_eq!(std::fs::read(&target).unwrap(), b"unfinished writer");
    }
    #[test]
    fn unstar_does_not_attach_an_unrelated_output_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let source = root.join("raw/a.png");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"original").unwrap();
        let target = move_media(&root, &source).unwrap();
        std::fs::write(root.join("raw/a.xmp"), b"unrelated metadata").unwrap();
        assert!(unstar_media(&root, &target)
            .unwrap_err()
            .contains("sidecars"));
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
        assert!(!source.exists());
    }
    #[test]
    fn unstar_keeps_the_current_filename_after_a_star_collision() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        std::fs::create_dir(root.join("starred")).unwrap();
        std::fs::write(root.join("starred/a.png"), b"existing").unwrap();
        let source = root.join("a.png");
        std::fs::write(&source, b"original").unwrap();
        let target = move_media(&root, &source).unwrap();
        assert_eq!(target, root.join("starred/a-1.png"));
        assert_eq!(
            unstar_media(&root, &target).unwrap(),
            root.join("raw/a-1.png")
        );
        assert_eq!(
            std::fs::read(root.join("starred/a.png")).unwrap(),
            b"existing"
        );
    }
    #[test]
    fn unstar_failure_reaches_shared_logs_without_paths() {
        assert!(move_owned(
            "/private/media.png".into(),
            &crate::install::Engine::default(),
            &crate::jobs::Jobs::default(),
            &crate::FolderOperations::default(),
            true
        )
        .is_err());
        let entries = crate::logs::native_logs(0);
        let entry = entries
            .iter()
            .find(|e| e.text.contains("operation=unstar failed"))
            .unwrap();
        assert_eq!(entry.source, crate::logs::Source::Deas);
        assert!(entry.text.starts_with("ERROR:"));
        assert!(!entry.text.contains("private"));
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn unstar_namespace_replacement_never_reports_stale_success() {
        for after_commit in [false, true] {
            for changed in ["root", "source"] {
                let dir = tempfile::tempdir().unwrap();
                let base = dunce::canonicalize(dir.path()).unwrap();
                let root = base.join("output");
                std::fs::create_dir(&root).unwrap();
                let source = root.join("a.png");
                std::fs::write(&source, b"original").unwrap();
                let target = move_media(&root, &source).unwrap();
                let displaced = base.join("displaced");
                let changed_path = if changed == "root" {
                    root.clone()
                } else {
                    root.join("starred")
                };
                assert!(relocate_checked(&root, &target, true, |committed| {
                    if committed == after_commit {
                        std::fs::rename(&changed_path, &displaced).unwrap();
                        std::fs::create_dir(&changed_path).unwrap();
                    }
                })
                .is_err());
                let recovered = if changed == "root" {
                    displaced.join("starred/a.png")
                } else {
                    displaced.join("a.png")
                };
                assert_eq!(std::fs::read(recovered).unwrap(), b"original");
                assert!(!source.exists());
            }
        }
    }
    #[test]
    fn source_replacement_before_rename_is_not_moved() {
        for unstar in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let root = dunce::canonicalize(dir.path()).unwrap();
            let original = root.join("a.png");
            std::fs::write(&original, b"original").unwrap();
            let source = if unstar {
                move_media(&root, &original).unwrap()
            } else {
                original.clone()
            };
            let held = root.join("held.png");
            let error = relocate_checked(&root, &source, unstar, |committed| {
                if !committed {
                    std::fs::rename(&source, &held).unwrap();
                    std::fs::write(&source, b"replacement").unwrap();
                }
            })
            .unwrap_err();
            assert!(error.contains("identity changed"), "{error}");
            assert_eq!(std::fs::read(&source).unwrap(), b"replacement");
            assert_eq!(std::fs::read(&held).unwrap(), b"original");
            if unstar {
                assert!(!original.exists());
            } else {
                assert!(!root.join("starred/a.png").exists());
            }
        }
    }
    #[test]
    fn transport_failures_reach_shared_logs_and_repeats_are_limited() {
        native_media_star_notice();
        native_media_star_notice();
        let entries = crate::logs::native_logs(0);
        let matching: Vec<_> = entries
            .iter()
            .filter(|e| e.text.contains("Media operation=star-transport"))
            .collect();
        assert_eq!(matching.len(), 1);
        assert_eq!(matching[0].source, crate::logs::Source::Deas);
        assert!(matching[0].text.starts_with("ERROR:"));
    }
    #[test]
    fn refuses_unsafe_origins_and_failed_moves_preserve_source() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let source = root.join("a.png");
        std::fs::write(&source, png_bytes()).unwrap();
        let external = tempfile::tempdir().unwrap();
        let foreign = external.path().join("outside.mp4");
        std::fs::write(&foreign, b"outside").unwrap();
        assert!(move_media(&root, &foreign).is_err());
        assert!(move_media(&root, &root.join("sub/../a.png")).is_err());
        std::fs::write(root.join("starred"), b"not a directory").unwrap();
        assert!(move_media(&root, &source).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), png_bytes());
        std::fs::remove_file(root.join("starred")).unwrap();
        std::fs::write(root.join("a.json"), b"sidecar").unwrap();
        assert!(move_media(&root, &source).unwrap_err().contains("sidecars"));
        std::fs::remove_file(root.join("a.json")).unwrap();
        std::fs::write(root.join(crate::projects::MANIFEST), b"{}").unwrap();
        assert!(move_media(&root, &source).unwrap_err().contains("Project"));
        assert_eq!(std::fs::read(&source).unwrap(), png_bytes());
    }
    #[cfg(unix)]
    #[test]
    fn refuses_readonly_files_and_symlinked_destinations() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let source = root.join("a.png");
        std::fs::write(&source, png_bytes()).unwrap();
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o400)).unwrap();
        assert!(move_media(&root, &source).is_err());
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o600)).unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.join("starred")).unwrap();
        assert!(move_media(&root, &source).is_err());
        assert!(source.exists());
        assert!(!outside.path().join("a.png").exists());
    }
    #[test]
    fn existing_nested_png_moves_without_a_job_record_and_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let source = root.join("2026-10-21/model/file_00001.png");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, png_bytes()).unwrap();
        let jobs = crate::jobs::Jobs::default();
        jobs.media_move_ready(&root, &source).unwrap();
        let target = jobs
            .relocate_media(&root, &source, || move_media(&root, &source))
            .unwrap();
        assert_eq!(target, root.join("starred/2026-10-21/model/file_00001.png"));
        assert_eq!(std::fs::read(&target).unwrap(), png_bytes());
        assert!(!source.exists());
        let restarted = crate::jobs::Jobs::default();
        assert_eq!(
            restarted
                .relocate_media(&root, &target, || move_media(&root, &target))
                .unwrap(),
            target
        );
        std::fs::write(&source, png_bytes()).unwrap();
        let second = restarted
            .relocate_media(&root, &source, || move_media(&root, &source))
            .unwrap();
        assert_eq!(
            second,
            root.join("starred/2026-10-21/model/file_00001-1.png")
        );
        assert_eq!(
            std::fs::read(&target).unwrap(),
            std::fs::read(&second).unwrap()
        );
    }
    #[test]
    fn recognized_media_moves_nested_arbitrary_bytes_without_parsing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        // Arbitrary byte-preservation fixtures, not decoded media samples.
        let bytes = b"\0arbitrary media bytes\xff\x01";
        let jobs = crate::jobs::Jobs::default();
        for (index, extension) in [
            "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "avif", "svg", "mp4", "mkv",
            "webm", "mov", "avi", "m4v", "mpg", "mpeg", "wav", "mp3", "flac", "ogg", "m4a", "aac",
            "opus", "JPG",
        ]
        .into_iter()
        .enumerate()
        {
            // One stem per case: `JPG` must not land on the earlier `jpg` on case-insensitive filesystems.
            let relative = format!("2026-10-21/model/file_{index:05}.{extension}");
            let source = root.join(&relative);
            std::fs::create_dir_all(source.parent().unwrap()).unwrap();
            std::fs::write(&source, bytes).unwrap();
            jobs.media_move_ready(&root, &source).unwrap();
            let target = jobs
                .relocate_media(&root, &source, || move_media(&root, &source))
                .unwrap();
            assert_eq!(target, root.join("starred").join(&relative));
            assert_eq!(std::fs::read(&target).unwrap(), bytes);
            assert!(!source.exists());
            let returned = jobs
                .relocate_media(&root, &target, || unstar_media(&root, &target))
                .unwrap();
            assert_eq!(returned, root.join("raw").join(&relative));
            assert_eq!(std::fs::read(returned).unwrap(), bytes);
            assert!(!target.exists());
        }
    }
    #[test]
    fn png_envelope_errors_are_not_move_errors() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let original = png_bytes();
        let mut corrupt = original.clone();
        corrupt[29] ^= 1; // IHDR CRC deliberately invalid; starring does not parse it.
        let mut trailing = original.clone();
        trailing.push(0);
        for (index, bytes) in [&original[..original.len() - 1], &corrupt, &trailing]
            .into_iter()
            .enumerate()
        {
            let source = root.join(format!("unparsed-{index}.png"));
            std::fs::write(&source, bytes).unwrap();
            validate(&root, &source).unwrap();
            let target = move_media(&root, &source).unwrap();
            assert_eq!(std::fs::read(target).unwrap(), bytes);
            assert!(!source.exists());
        }
    }
    #[test]
    fn status_preflight_does_not_read_or_limit_media_bytes() {
        use std::io::Seek;
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let source = root.join("large.png");
        // Sparse arbitrary-byte fixture exceeding the former PNG-only size limit.
        std::fs::File::create(&source)
            .unwrap()
            .set_len(145 * 1024 * 1024)
            .unwrap();
        let (starred, mut file) = preflight(&root, &source).unwrap();
        assert!(!starred);
        assert_eq!(file.stream_position().unwrap(), 0);
        crate::jobs::Jobs::default()
            .media_move_ready(&root, &source)
            .unwrap();
        assert!(source.exists());
        assert!(!root.join("starred").exists());
    }
    #[test]
    fn unsupported_suffixes_remain_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        for name in ["unknown.bin", "no-extension", "media.png.part"] {
            let source = root.join(name);
            std::fs::write(&source, b"arbitrary bytes").unwrap();
            assert!(validate(&root, &source)
                .unwrap_err()
                .contains("unsupported"));
            assert!(move_media(&root, &source)
                .unwrap_err()
                .contains("unsupported"));
            assert_eq!(std::fs::read(source).unwrap(), b"arbitrary bytes");
        }
        assert!(!root.join("starred").exists());
    }
    #[test]
    fn stability_and_open_identity_checks_reject_changed_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let source = root.join("changing.png");
        std::fs::write(&source, png_bytes()).unwrap();
        let file = open_media(&source).unwrap();
        let before = file.metadata().unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&source)
            .unwrap()
            .set_len(1)
            .unwrap();
        assert!(unchanged(&file, &before)
            .unwrap_err()
            .contains("still changing"));
        let parent = Anchor::open(&root).unwrap();
        std::fs::rename(&source, root.join("old.png")).unwrap();
        std::fs::write(&source, png_bytes()).unwrap();
        assert!(!parent.owns_partial("changing.png", &file));
    }
    #[cfg(unix)]
    #[test]
    fn refuses_symlink_sources_ancestors_and_nested_target_escapes() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let outside = tempfile::tempdir().unwrap();
        let external = outside.path().join("file.png");
        std::fs::write(&external, png_bytes()).unwrap();
        symlink(&external, root.join("link.png")).unwrap();
        symlink(outside.path(), root.join("linked-dir")).unwrap();
        assert!(move_media(&root, &root.join("link.png")).is_err());
        assert!(move_media(&root, &root.join("linked-dir/file.png")).is_err());
        let source = root.join("nested/model/file.png");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, png_bytes()).unwrap();
        symlink(root.join("missing-sidecar"), source.with_extension("json")).unwrap();
        assert!(move_media(&root, &source).unwrap_err().contains("sidecars"));
        std::fs::remove_file(source.with_extension("json")).unwrap();
        std::fs::create_dir(root.join("starred")).unwrap();
        symlink(outside.path(), root.join("starred/nested")).unwrap();
        assert!(move_media(&root, &source).is_err());
        assert!(source.exists());
        assert!(!outside.path().join("model").exists());
        assert_eq!(std::fs::read(external).unwrap(), png_bytes());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn directory_namespace_replacements_never_return_a_stale_success() {
        for after_commit in [false, true] {
            for changed in ["root", "source", "destination"] {
                let dir = tempfile::tempdir().unwrap();
                let base = dunce::canonicalize(dir.path()).unwrap();
                let root = base.join("output");
                let source = root.join("nested/a.png");
                std::fs::create_dir_all(source.parent().unwrap()).unwrap();
                std::fs::write(&source, b"original bytes").unwrap();
                let displaced = base.join("displaced");
                let changed_path = match changed {
                    "root" => root.clone(),
                    "source" => source.parent().unwrap().to_path_buf(),
                    _ => root.join("starred/nested"),
                };
                let error = move_media_checked(&root, &source, |committed| {
                    if committed == after_commit {
                        std::fs::rename(&changed_path, &displaced).unwrap();
                        std::fs::create_dir_all(&changed_path).unwrap();
                    }
                })
                .unwrap_err();
                let recovered = match changed {
                    "root" => displaced.join("nested/a.png"),
                    "source" => displaced.join("a.png"),
                    _ => source.clone(),
                };
                assert_eq!(std::fs::read(recovered).unwrap(), b"original bytes");
                assert!(!root.join("starred/nested/a.png").exists());
                if after_commit && changed == "destination" {
                    assert!(error.contains("rolled back to source"), "{error}");
                    assert!(!displaced.join("a.png").exists());
                } else if after_commit {
                    assert!(error.contains("could not be confirmed restored"), "{error}");
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn rollback_never_clobbers_a_concurrent_source_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let base = dunce::canonicalize(dir.path()).unwrap();
        let root = base.join("output");
        std::fs::create_dir(&root).unwrap();
        let source = root.join("a.png");
        std::fs::write(&source, b"original bytes").unwrap();
        let displaced = base.join("displaced");
        let error = move_media_checked(&root, &source, |committed| {
            if committed {
                std::fs::rename(root.join("starred"), &displaced).unwrap();
                std::fs::create_dir(root.join("starred")).unwrap();
                std::fs::write(&source, b"concurrent replacement").unwrap();
            }
        })
        .unwrap_err();
        assert!(error.contains("could not be confirmed restored"), "{error}");
        assert!(!error.contains("rolled back to source"));
        assert_eq!(std::fs::read(&source).unwrap(), b"concurrent replacement");
        assert_eq!(
            std::fs::read(displaced.join("a.png")).unwrap(),
            b"original bytes"
        );
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn postcommit_replacement_of_media_is_not_returned_or_rolled_back() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        let source = root.join("a.png");
        std::fs::write(&source, b"original bytes").unwrap();
        let target = root.join("starred/a.png");
        let recovered = root.join("original.png");
        let error = move_media_checked(&root, &source, |committed| {
            if committed {
                std::fs::rename(&target, &recovered).unwrap();
                std::fs::write(&target, b"concurrent replacement").unwrap();
            }
        })
        .unwrap_err();
        assert!(error.contains("could not be confirmed restored"));
        assert!(!source.exists());
        assert_eq!(std::fs::read(target).unwrap(), b"concurrent replacement");
        assert_eq!(std::fs::read(recovered).unwrap(), b"original bytes");
    }
    #[test]
    fn moves_unicode_without_clobber_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        std::fs::create_dir(root.join("starred")).unwrap();
        std::fs::write(root.join("starred/雪.png"), b"existing").unwrap();
        let source = root.join("雪.png");
        std::fs::write(&source, png_bytes()).unwrap();
        let target = move_media(&root, &source).unwrap();
        assert_eq!(target, root.join("starred/雪-1.png"));
        assert!(!source.exists());
        assert_eq!(std::fs::read(&target).unwrap(), png_bytes());
        assert_eq!(
            std::fs::read(root.join("starred/雪.png")).unwrap(),
            b"existing"
        );
        assert_eq!(move_media(&root, &target).unwrap(), target);
    }
}
