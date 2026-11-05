//! Copy/verify/commit/cleanup relocation. Guards protect against other users, not
//! concurrent hostile processes running as this user. Caller must exclude app writers.
use super::Folders;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveReport {
    pub folders: Folders,
    pub moved_files: usize,
    pub warnings: Vec<String>,
}

pub fn validate(folders: &Folders) -> Result<(), String> {
    normalized(folders.clone()).map(|_| ())
}

fn normalized(mut folders: Folders) -> Result<Folders, String> {
    for path in [&mut folders.models, &mut folders.input, &mut folders.output]
        .into_iter()
        .flatten()
    {
        *path = platform::chosen(Path::new(path))?
            .to_str()
            .ok_or("Folder path must be UTF-8")?
            .to_owned();
    }
    // Extra model roots are canonicalised and validated like the primary ones; the mover
    // never touches their contents.
    for path in &mut folders.extra_models {
        *path = platform::chosen(Path::new(path))?
            .to_str()
            .ok_or("Folder path must be UTF-8")?
            .to_owned();
    }
    Ok(folders)
}

pub(super) fn save(root: &Path, folders: Folders) -> Result<Folders, String> {
    platform::save(root, folders)
}

pub fn move_folders(root: &Path, folders: Folders) -> Result<MoveReport, String> {
    platform::relocate(root, folders, |_| Ok(()))
}

#[cfg(not(target_os = "linux"))]
mod platform {
    use super::*;
    use std::path::PathBuf;
    pub(super) fn chosen(path: &Path) -> Result<PathBuf, String> {
        if !path.is_absolute() || !path.is_dir() {
            return Err("Folder must be an existing absolute directory".into());
        }
        let real = dunce::canonicalize(path).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        if !crate::win::is_directory(&crate::win::open_no_follow(&real)?)? {
            return Err("Folder must be a directory".into());
        }
        Ok(real)
    }
    pub(super) fn save(root: &Path, folders: Folders) -> Result<Folders, String> {
        use std::io::Write;
        let folders = normalized(folders)?;
        let root = chosen(root)?;
        let target = root.join(super::super::FILE);
        match std::fs::symlink_metadata(&target) {
            Ok(m) if !m.is_file() || m.file_type().is_symlink() => {
                return Err("Settings target must be a regular file".into());
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
            _ => (),
        }
        let tmp = root.join(format!(".{}.{}", super::super::FILE, uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)
                .map_err(|e| e.to_string())?;
            file.write_all(&serde_json::to_vec_pretty(&folders).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            drop(file);
            std::fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
            Ok(folders)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(tmp);
        }
        result
    }
    pub(super) fn relocate(
        _: &Path,
        _: Folders,
        _: impl FnMut(&str) -> Result<(), String>,
    ) -> Result<MoveReport, String> {
        Err("Safe folder relocation currently requires Linux".into())
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::{
        fs::{self, File, Metadata, OpenOptions},
        io::{Read, Write},
        os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
        path::PathBuf,
    };

    fn error(path: &Path, e: impl std::fmt::Display) -> String {
        format!("{}: {e}", path.display())
    }
    fn metadata(path: &Path) -> Result<Option<Metadata>, String> {
        match fs::symlink_metadata(path) {
            Ok(m) => Ok(Some(m)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(error(path, e)),
        }
    }
    fn safe_dir(path: &Path, owned: bool) -> Result<(), String> {
        let m = fs::symlink_metadata(path).map_err(|e| error(path, e))?;
        let uid = unsafe { libc::geteuid() };
        if !m.is_dir() || m.mode() & 0o022 != 0 || (m.uid() != uid && (owned || m.uid() != 0)) {
            return Err(error(
                path,
                "requires a safe owned directory without group/world write",
            ));
        }
        Ok(())
    }
    // Root-owned read-only ancestors are trusted; shared writable ancestors are not.
    fn safe_path(path: &Path) -> Result<(), String> {
        if !path.is_absolute() {
            return Err("Folder must be absolute".into());
        }
        for p in path.ancestors() {
            safe_dir(p, p == path)?;
        }
        Ok(())
    }
    pub(super) fn chosen(path: &Path) -> Result<PathBuf, String> {
        if !path.is_absolute() {
            return Err("Folder must be absolute".into());
        }
        let real = dunce::canonicalize(path).map_err(|e| error(path, e))?;
        safe_path(&real)?;
        Ok(real)
    }
    fn default_path(root: &Path, category: &str) -> Result<PathBuf, String> {
        let path = root.join("ComfyUI").join(category);
        for p in [root.join("ComfyUI"), path.clone()] {
            if metadata(&p)?.is_some() {
                safe_dir(&p, true)?;
            }
        }
        Ok(path)
    }
    fn regular(path: &Path, m: &Metadata) -> Result<(), String> {
        if !m.is_file() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o022 != 0 {
            return Err(error(
                path,
                "requires an owned regular file without group/world write",
            ));
        }
        Ok(())
    }
    fn open(path: &Path) -> Result<File, String> {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| error(path, e))
    }
    fn stamp(m: &Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
        (
            m.dev(),
            m.ino(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        )
    }
    fn same_inode(a: &Metadata, b: &Metadata) -> bool {
        a.dev() == b.dev() && a.ino() == b.ino()
    }
    fn equal(a: &Path, b: &Path) -> Result<(), String> {
        let mut a = open(a)?;
        let mut b = open(b)?;
        let mut x = [0u8; 65536];
        let mut y = [0u8; 65536];
        loop {
            let n = a.read(&mut x).map_err(|e| e.to_string())?;
            b.read_exact(&mut y[..n]).map_err(|e| e.to_string())?;
            if x[..n] != y[..n] {
                return Err("Copied bytes differ".into());
            }
            if n == 0 {
                if b.read(&mut y[..1]).map_err(|e| e.to_string())? != 0 {
                    return Err("Copied size differs".into());
                }
                return Ok(());
            }
        }
    }
    pub(super) fn save(root: &Path, folders: Folders) -> Result<Folders, String> {
        let folders = normalized(folders)?;
        let root = chosen(root)?;
        let target = root.join(super::super::FILE);
        if let Some(m) = metadata(&target)? {
            regular(&target, &m)?;
        }
        let tmp = root.join(format!(".{}.{}", super::super::FILE, uuid::Uuid::new_v4()));
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp)
            .map_err(|e| error(&tmp, e))?;
        let result = (|| {
            f.write_all(&serde_json::to_vec_pretty(&folders).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
            // A directory sync failure must occur before commit, never trigger rollback after rename.
            let dir = File::open(&root).map_err(|e| e.to_string())?;
            dir.sync_all().map_err(|e| e.to_string())?;
            fs::rename(&tmp, &target).map_err(|e| error(&target, e))?;
            // shortcut: post-rename directory sync is best effort; power-loss durability is not transactional.
            let _ = dir.sync_all();
            Ok(folders)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }
    struct Entry {
        source: PathBuf,
        target: PathBuf,
        original: Metadata,
    }
    fn scan(
        source: &Path,
        target: &Path,
        entries: &mut Vec<Entry>,
        dirs: &mut Vec<PathBuf>,
    ) -> Result<(), String> {
        safe_dir(source, true)?;
        match metadata(target)? {
            Some(_) => safe_dir(target, true)?,
            None => dirs.push(target.to_owned()),
        }
        for child in fs::read_dir(source).map_err(|e| error(source, e))? {
            let child = child.map_err(|e| e.to_string())?;
            let src = child.path();
            let dst = target.join(child.file_name());
            let m = fs::symlink_metadata(&src).map_err(|e| error(&src, e))?;
            if m.is_dir() {
                scan(&src, &dst, entries, dirs)?;
            } else {
                regular(&src, &m)?;
                if metadata(&dst)?.is_some() {
                    return Err(error(&dst, "destination conflict (never overwritten)"));
                }
                entries.push(Entry {
                    source: src,
                    target: dst,
                    original: m,
                });
            }
        }
        Ok(())
    }
    fn inspect_tree(path: &Path) -> Result<(), String> {
        safe_dir(path, true)?;
        for child in fs::read_dir(path).map_err(|e| error(path, e))? {
            let p = child.map_err(|e| e.to_string())?.path();
            let m = fs::symlink_metadata(&p).map_err(|e| error(&p, e))?;
            if m.is_dir() {
                inspect_tree(&p)?;
            } else {
                regular(&p, &m)?;
            }
        }
        Ok(())
    }
    fn ensure_dir(path: &Path, created: &mut Vec<PathBuf>) -> Result<(), String> {
        if metadata(path)?.is_some() {
            return safe_dir(path, true);
        }
        ensure_dir(path.parent().ok_or("Missing parent")?, created)?;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .map_err(|e| error(path, e))?;
        created.push(path.to_owned());
        Ok(())
    }
    pub(super) fn relocate(
        root: &Path,
        folders: Folders,
        mut hook: impl FnMut(&str) -> Result<(), String>,
    ) -> Result<MoveReport, String> {
        let root = chosen(root)?;
        let folders = normalized(folders)?;
        let old = normalized(super::super::read(&root)?)?;
        let mut pairs = Vec::new();
        for (category, before, after) in [
            ("models", old.models, &folders.models),
            ("input", old.input, &folders.input),
            ("output", old.output, &folders.output),
        ] {
            let src = match before {
                Some(p) => PathBuf::from(p),
                None => default_path(&root, category)?,
            };
            let dst = match after {
                Some(p) => PathBuf::from(p),
                None => default_path(&root, category)?,
            };
            pairs.push((src, dst));
        }
        for (i, (a, b)) in pairs.iter().enumerate() {
            for (j, (c, d)) in pairs.iter().enumerate() {
                for x in [a, b] {
                    for y in [c, d] {
                        if (i != j || x != y) && (x.starts_with(y) || y.starts_with(x)) {
                            return Err(
                                "Folder paths overlap or are nested (including across categories)"
                                    .into(),
                            );
                        }
                    }
                }
            }
        }
        let mut entries = Vec::new();
        let mut dirs = Vec::new();
        for (src, dst) in &pairs {
            if src == dst {
                continue;
            }
            if metadata(dst)?.is_some() {
                inspect_tree(dst)?;
            }
            if metadata(src)?.is_some() {
                scan(src, dst, &mut entries, &mut dirs)?;
            }
        }
        // All trees/conflicts have been inspected before the first write.
        let mut created_dirs = Vec::new();
        let mut copied: Vec<(PathBuf, Metadata)> = Vec::new();
        let result = (|| {
            for dir in &dirs {
                ensure_dir(dir, &mut created_dirs)?;
            }
            for entry in &entries {
                let mut src = open(&entry.source)?;
                if stamp(&src.metadata().map_err(|e| e.to_string())?) != stamp(&entry.original) {
                    return Err(error(&entry.source, "source changed"));
                }
                let mut dst = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&entry.target)
                    .map_err(|e| error(&entry.target, e))?;
                copied.push((
                    entry.target.clone(),
                    dst.metadata().map_err(|e| e.to_string())?,
                ));
                std::io::copy(&mut src, &mut dst).map_err(|e| error(&entry.target, e))?;
                dst.sync_all().map_err(|e| e.to_string())?;
                hook("copied")?;
                equal(&entry.source, &entry.target)?;
                if stamp(&src.metadata().map_err(|e| e.to_string())?) != stamp(&entry.original) {
                    return Err(error(&entry.source, "source changed during copy"));
                }
            }
            hook("save")?;
            save(&root, folders.clone())
        })();
        let folders = match result {
            Ok(f) => f,
            Err(mut e) => {
                for (p, identity) in copied.iter().rev() {
                    match metadata(p) {
                        Ok(Some(m)) if same_inode(&m, identity) => {
                            if let Err(err) = fs::remove_file(p) {
                                e.push_str(&format!(
                                    "; copied destination leftover {}: {err}",
                                    p.display()
                                ));
                            }
                        }
                        Ok(None) => (),
                        _ => {
                            e.push_str(&format!("; destination leftover/replaced: {}", p.display()))
                        }
                    }
                }
                for p in created_dirs.iter().rev() {
                    if let Err(err) = fs::remove_dir(p) {
                        e.push_str(&format!(
                            "; destination directory leftover {}: {err}",
                            p.display()
                        ));
                    }
                }
                return Err(e);
            }
        };
        let mut warnings = Vec::new();
        if let Err(e) = hook("cleanup") {
            warnings.push(e);
        }
        let mut moved_files = 0;
        for entry in &entries {
            let unchanged = (|| {
                let m = metadata(&entry.source)?.ok_or("Source disappeared")?;
                if stamp(&m) != stamp(&entry.original) {
                    return Err("Source changed; original retained".into());
                }
                equal(&entry.source, &entry.target)?;
                let after = metadata(&entry.source)?.ok_or("Source disappeared")?;
                if stamp(&after) != stamp(&entry.original) {
                    return Err(
                        "Source changed during cleanup verification; original retained".into(),
                    );
                }
                fs::remove_file(&entry.source).map_err(|e| e.to_string())
            })();
            match unchanged {
                Ok(()) => moved_files += 1,
                Err(e) => warnings.push(error(&entry.source, e)),
            }
        }
        // Keep source directories: removing them could remove newly-created empty user folders.
        Ok(MoveReport {
            folders,
            moved_files,
            warnings,
        })
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "settings_move_tests.rs"]
mod tests;
