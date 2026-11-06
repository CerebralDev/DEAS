use super::Anchor;
use std::os::unix::fs::{symlink, PermissionsExt};

fn scratch() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn directory_metadata_requires_effective_owner_and_directory_type() {
    let uid = unsafe { libc::geteuid() };
    for private in [false, true] {
        assert!(Anchor::validate_directory_metadata(uid, libc::S_IFDIR | 0o700, private).is_ok());
        assert!(Anchor::validate_directory_metadata(
            uid.wrapping_add(1),
            libc::S_IFDIR | 0o700,
            private
        )
        .is_err());
        for kind in [libc::S_IFREG, libc::S_IFLNK, libc::S_IFIFO] {
            assert!(Anchor::validate_directory_metadata(uid, kind | 0o700, private).is_err());
        }
    }
    assert!(Anchor::validate_directory_metadata(uid, libc::S_IFDIR | 0o755, false).is_ok());
    for permission in [0o040, 0o020, 0o010, 0o004, 0o002, 0o001] {
        assert!(
            Anchor::validate_directory_metadata(uid, libc::S_IFDIR | 0o700 | permission, true)
                .is_err()
        );
    }
}

#[test]
fn opened_substituted_child_is_rejected_before_download_writes() {
    for mode in [0o740, 0o720, 0o710, 0o704, 0o702, 0o701, 0o777] {
        let d = scratch();
        let parent = Anchor::open(d.path()).unwrap();
        let stage = d.path().join("stage");
        let original = d.path().join("original");
        let result = parent.create_checked("stage", || {
            std::fs::rename(&stage, &original).unwrap();
            std::fs::create_dir(&stage).unwrap();
            std::fs::set_permissions(&stage, std::fs::Permissions::from_mode(mode)).unwrap();
        });
        assert!(result.is_err(), "accepted substituted mode {mode:o}");
        assert_eq!(std::fs::read_dir(&stage).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(&original).unwrap().count(), 0);
    }
}

#[test]
fn substituted_symlink_is_rejected_without_touching_outside() {
    let d = scratch();
    let outside = scratch();
    let parent = Anchor::open(d.path()).unwrap();
    let stage = d.path().join("stage");
    assert!(parent
        .create_checked("stage", || {
            std::fs::remove_dir(&stage).unwrap();
            symlink(outside.path(), &stage).unwrap();
        })
        .is_err());
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    assert!(stage.symlink_metadata().unwrap().file_type().is_symlink());
}

#[test]
fn opened_non_directory_cannot_validate_as_child_or_create_anything() {
    let d = scratch();
    let file = d.path().join("file");
    std::fs::write(&file, b"untouched").unwrap();
    let anchor = Anchor(std::fs::File::open(&file).unwrap());
    assert!(anchor.validate_directory(true).is_err());
    assert!(anchor.create("stage").is_err());
    assert_eq!(std::fs::read(&file).unwrap(), b"untouched");
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
}
