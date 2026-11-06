use super::platform::relocate;
use super::*;
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::PathBuf,
};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    dest: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("env");
        let dest = temp.path().join("dest");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&dest).unwrap();
        for c in ["models", "input", "output"] {
            fs::create_dir_all(root.join("ComfyUI").join(c)).unwrap();
        }
        Self {
            _temp: temp,
            root,
            dest,
        }
    }
    fn folders(&self) -> Folders {
        Folders {
            models: Some(self.dest.to_str().unwrap().into()),
            ..Folders::default()
        }
    }
    fn source(&self) -> PathBuf {
        self.root.join("ComfyUI/models")
    }
    fn file(&self, name: &str, bytes: &[u8]) {
        let p = self.source().join(name);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }
}

#[test]
fn nested_merge_and_reset_to_default() {
    let f = Fixture::new();
    f.file("loras/a.bin", &[42; 131_077]);
    f.file("empty/child/.keep", b"");
    fs::create_dir(f.dest.join("loras")).unwrap();
    fs::write(f.dest.join("loras/existing"), b"keep").unwrap();
    let report = move_folders(&f.root, f.folders()).unwrap();
    assert_eq!(report.moved_files, 2);
    assert!(report.warnings.is_empty());
    assert!(!f.source().join("loras/a.bin").exists());
    assert_eq!(
        fs::read(f.dest.join("loras/a.bin")).unwrap(),
        vec![42; 131_077]
    );
    assert_eq!(super::super::read(&f.root).unwrap(), report.folders);
    let report = move_folders(&f.root, Folders::default()).unwrap();
    assert_eq!(report.moved_files, 3);
    assert!(f.source().join("loras/existing").exists());
    assert_eq!(report.folders, Folders::default());
    let json = serde_json::to_value(report).unwrap();
    assert_eq!(json["movedFiles"], 3);
}

#[test]
fn collisions_across_categories_abort_everything_even_identical_bytes() {
    let f = Fixture::new();
    f.file("a", b"same");
    fs::write(f.root.join("ComfyUI/input/collision"), b"same").unwrap();
    let input = f._temp.path().join("new-input");
    fs::create_dir(&input).unwrap();
    fs::write(input.join("collision"), b"same").unwrap();
    let mut folders = f.folders();
    folders.input = Some(input.to_str().unwrap().into());
    assert!(move_folders(&f.root, folders)
        .unwrap_err()
        .contains("conflict"));
    assert!(!f.dest.join("a").exists());
    assert!(f.source().join("a").exists());
    assert!(!f.root.join(super::super::FILE).exists());
}

#[test]
fn directory_file_collision_and_special_files_rejected() {
    let f = Fixture::new();
    f.file("nested/file", b"data");
    fs::write(f.dest.join("nested"), b"collision").unwrap();
    assert!(move_folders(&f.root, f.folders()).is_err());
    fs::remove_file(f.dest.join("nested")).unwrap();
    let name = std::ffi::CString::new(f.source().join("fifo").to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(move_folders(&f.root, f.folders()).is_err());
    assert!(!f.dest.join("nested").exists());
}

#[test]
fn root_symlinks_normalized_but_internal_symlinks_rejected() {
    let f = Fixture::new();
    f.file("a", b"data");
    let alias = f._temp.path().join("alias");
    symlink(&f.dest, &alias).unwrap();
    let mut folders = f.folders();
    folders.models = Some(alias.to_str().unwrap().into());
    validate(&folders).unwrap();
    let root_alias = f._temp.path().join("env-alias");
    symlink(&f.root, &root_alias).unwrap();
    let report = move_folders(&root_alias, folders).unwrap();
    assert_eq!(report.folders, f.folders());
    symlink(f.dest.join("a"), f.dest.join("link")).unwrap();
    assert!(move_folders(&f.root, Folders::default()).is_err());
    assert!(f.dest.join("a").exists());
}

#[test]
fn source_and_destination_tree_symlinks_rejected() {
    for source in [true, false] {
        let f = Fixture::new();
        f.file("a", b"data");
        let p = if source { f.source() } else { f.dest.clone() };
        symlink(&f.root, p.join("linked-dir")).unwrap();
        assert!(move_folders(&f.root, f.folders()).is_err());
        assert!(f.source().join("a").exists());
        assert!(!f.dest.join("a").exists());
    }
}

#[test]
fn overlapping_paths_rejected() {
    let f = Fixture::new();
    fs::create_dir(f.source().join("nested")).unwrap();
    for path in [
        f.source().join("nested"),
        f.root.join("ComfyUI"),
        f.root.join("ComfyUI/input"),
    ] {
        let mut folders = f.folders();
        folders.models = Some(path.to_str().unwrap().into());
        assert!(move_folders(&f.root, folders)
            .unwrap_err()
            .contains("overlap"));
    }
    let mut folders = f.folders();
    folders.input = folders.models.clone();
    assert!(move_folders(&f.root, folders)
        .unwrap_err()
        .contains("overlap"));
}

#[test]
fn unsafe_dirs_and_files_rejected() {
    let f = Fixture::new();
    f.file("a", b"data");
    fs::set_permissions(&f.dest, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(validate(&f.folders()).is_err());
    assert!(move_folders(&f.root, f.folders()).is_err());
    fs::set_permissions(&f.dest, fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(f.source().join("a"), fs::Permissions::from_mode(0o666)).unwrap();
    assert!(move_folders(&f.root, f.folders()).is_err());
}

#[test]
fn failed_copy_and_save_keep_all_sources_and_old_settings() {
    for stage in ["copied", "save"] {
        let f = Fixture::new();
        f.file("nested/a", b"a");
        f.file("b", b"b");
        super::super::write(&f.root, Folders::default()).unwrap();
        let old = fs::read(f.root.join(super::super::FILE)).unwrap();
        let error = relocate(&f.root, f.folders(), |s| {
            if s == stage {
                Err("injected I/O failure".into())
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert!(error.contains("injected"));
        assert_eq!(fs::read(f.root.join(super::super::FILE)).unwrap(), old);
        assert!(f.source().join("nested/a").exists());
        assert!(f.source().join("b").exists());
        assert_eq!(fs::read_dir(&f.dest).unwrap().count(), 0);
    }
}

#[test]
fn actual_save_failure_keeps_sources_and_removes_copies() {
    let f = Fixture::new();
    f.file("a", b"a");
    fs::create_dir(f.root.join(super::super::FILE)).unwrap();
    // A malformed settings file fails during read, still before copying.
    assert!(move_folders(&f.root, f.folders()).is_err());
    fs::remove_dir(f.root.join(super::super::FILE)).unwrap();
    let result = relocate(&f.root, f.folders(), |stage| {
        if stage == "save" {
            fs::create_dir(f.root.join(super::super::FILE)).unwrap();
        }
        Ok(())
    });
    assert!(result.is_err());
    assert!(f.source().join("a").exists());
    assert!(!f.dest.join("a").exists());
}

#[test]
fn source_changes_fail_copy_or_are_retained_after_commit() {
    for stage in ["copied", "cleanup"] {
        let f = Fixture::new();
        f.file("a", b"old");
        let result = relocate(&f.root, f.folders(), |s| {
            if s == stage {
                fs::write(f.source().join("a"), b"new").unwrap();
            }
            Ok(())
        });
        assert_eq!(fs::read(f.source().join("a")).unwrap(), b"new");
        if stage == "copied" {
            assert!(result.is_err());
            assert!(!f.dest.join("a").exists());
        } else {
            let report = result.unwrap();
            assert_eq!(report.moved_files, 0);
            assert_eq!(report.warnings.len(), 1);
            assert_eq!(fs::read(f.dest.join("a")).unwrap(), b"old");
        }
    }
}

#[test]
fn exclusive_create_failure_after_one_copy_preserves_every_source() {
    let f = Fixture::new();
    f.file("a", b"a");
    f.file("b", b"b");
    let mut injected = false;
    let error = relocate(&f.root, f.folders(), |stage| {
        if stage == "copied" && !injected {
            injected = true;
            let next = if f.dest.join("a").exists() { "b" } else { "a" };
            fs::write(f.dest.join(next), b"external collision").unwrap();
        }
        Ok(())
    })
    .unwrap_err();
    assert!(error.contains("exists"));
    assert_eq!(fs::read(f.source().join("a")).unwrap(), b"a");
    assert_eq!(fs::read(f.source().join("b")).unwrap(), b"b");
    assert!(!f.root.join(super::super::FILE).exists());
    let left = fs::read_dir(&f.dest)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(fs::read(left[0].path()).unwrap(), b"external collision");
}

#[test]
fn settings_symlink_rejected_and_yaml_path_quoted() {
    let f = Fixture::new();
    let special = f._temp.path().join("model # : \"quoted\"");
    fs::create_dir(&special).unwrap();
    let folders = Folders {
        models: Some(special.to_str().unwrap().into()),
        ..Folders::default()
    };
    super::super::write(&f.root, folders).unwrap();
    let args = super::super::backend_args(&f.root).unwrap();
    let yaml = fs::read_to_string(&args[1]).unwrap();
    let line = yaml
        .lines()
        .nth(1)
        .unwrap()
        .strip_prefix("  base_path: ")
        .unwrap();
    assert_eq!(
        serde_json::from_str::<String>(line).unwrap(),
        special.to_str().unwrap()
    );
    fs::remove_file(f.root.join(super::super::FILE)).unwrap();
    symlink(f.dest.join("untouched"), f.root.join(super::super::FILE)).unwrap();
    assert!(super::super::write(&f.root, Folders::default()).is_err());
    assert!(!f.dest.join("untouched").exists());
}
