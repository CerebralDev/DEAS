use super::*;
use std::path::Path;

fn draft_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        zip.start_file(
            *name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn package_roundtrip_copies_only_committed_manifest_and_current_input() {
    let folder = parent();
    let projects = Projects::default();
    let first = projects.create(Some(folder.path())).unwrap().unwrap();
    let source = folder.path().join("日本語 source.png");
    let bytes = png_file(&source);
    let saved = projects
        .import_image(&first.id, 0, Some(&source))
        .unwrap()
        .unwrap();
    let saved = projects
        .save(
            &saved.id,
            saved.revision,
            ImageDraft {
                positive_prompt: "private 日本語".into(),
                ..saved.draft
            },
        )
        .unwrap();
    let root = Path::new(&saved.root);
    for name in [
        "credentials.json",
        "model.safetensors",
        "result.png",
        "orphan.png",
    ] {
        std::fs::write(root.join(name), b"secret").unwrap();
    }
    let package = folder.path().join("日本語 draft.zip");
    assert!(!projects
        .export_package(&saved.id, saved.revision, None)
        .unwrap());
    assert!(projects
        .export_package(&saved.id, saved.revision, Some(&package))
        .unwrap());
    let original_package = std::fs::read(&package).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&original_package)).unwrap();
    assert_eq!(zip.len(), 2);
    let mut exported_manifest = zip.by_index(0).unwrap();
    assert_eq!(exported_manifest.name().unwrap(), MANIFEST);
    let mut exported_bytes = Vec::new();
    exported_manifest.read_to_end(&mut exported_bytes).unwrap();
    assert_eq!(exported_bytes, std::fs::read(root.join(MANIFEST)).unwrap());
    assert!(projects
        .export_package(&saved.id, saved.revision, Some(&package))
        .is_err());
    assert_eq!(std::fs::read(&package).unwrap(), original_package);
    std::fs::remove_file(&source).unwrap();
    assert!(projects
        .import_package(Some(&package), None)
        .unwrap()
        .is_none());
    let imported = projects
        .import_package(Some(&package), Some(folder.path()))
        .unwrap()
        .unwrap();
    assert_ne!(imported.root, saved.root);
    assert_eq!(
        (
            imported.id.clone(),
            imported.revision,
            imported.draft.clone()
        ),
        (saved.id.clone(), saved.revision, saved.draft.clone())
    );
    assert_eq!(
        std::fs::read(&imported.input.as_ref().unwrap().path).unwrap(),
        bytes
    );
    assert_eq!(
        std::fs::read(Path::new(&imported.root).join(MANIFEST)).unwrap(),
        std::fs::read(root.join(MANIFEST)).unwrap()
    );
    assert_eq!(std::fs::read(&package).unwrap(), original_package);
    projects.close(&imported.id).unwrap();
    assert_eq!(
        projects
            .open(Some(Path::new(&imported.root)))
            .unwrap()
            .unwrap(),
        imported
    );
}

#[test]
fn package_export_rejects_stale_external_changes_and_replaced_locks() {
    let folder = parent();
    let projects = Projects::default();
    let first = projects.create(Some(folder.path())).unwrap().unwrap();
    let output = folder.path().join("output.zip");
    assert!(projects
        .export_package(&first.id, 1, Some(&output))
        .is_err());
    assert!(!output.exists());
    let manifest = Path::new(&first.root).join(MANIFEST);
    let original = std::fs::read(&manifest).unwrap();
    std::fs::write(&manifest, b"{}").unwrap();
    assert!(projects
        .export_package(&first.id, 0, Some(&output))
        .is_err());
    std::fs::write(&manifest, original).unwrap();
    std::fs::rename(
        Path::new(&first.root).join(LOCK),
        Path::new(&first.root).join("old-lock"),
    )
    .unwrap();
    std::fs::write(Path::new(&first.root).join(LOCK), b"").unwrap();
    assert!(projects
        .export_package(&first.id, 0, Some(&output))
        .is_err());
    assert!(!output.exists());
}

#[test]
fn malicious_packages_leave_source_active_and_parent_unchanged() {
    let folder = parent();
    let projects = Projects::default();
    let first = projects.create(Some(folder.path())).unwrap().unwrap();
    let raw = std::fs::read(Path::new(&first.root).join(MANIFEST)).unwrap();
    let archive = folder.path().join("untrusted.zip");
    let mut bads = vec![
        b"not ZIP".to_vec(),
        draft_zip(&[(MANIFEST, &raw), (".deas-project.lock", b"secret")]),
        draft_zip(&[(MANIFEST, &raw), ("credentials.json", b"secret")]),
    ];
    for name in [
        "../outside",
        "/absolute",
        "C:\\outside",
        "a/b",
        "a\\b",
        "project.json/",
        "PROJECT.JSON",
    ] {
        bads.push(draft_zip(&[(name, b"bad")]));
    }
    bads.push(draft_zip(&[(MANIFEST, &raw), ("a", b""), ("b", b"")]));
    let mut duplicate = draft_zip(&[(MANIFEST, &raw), ("project.jsox", &raw)]);
    for i in 0..duplicate.len() - 11 {
        if &duplicate[i..i + 12] == b"project.jsox" {
            duplicate[i + 11] = b'n';
        }
    }
    bads.push(duplicate);
    for (field, value) in [
        ("schemaVersion", serde_json::json!(2)),
        ("revision", serde_json::json!(9_007_199_254_740_992_u64)),
        ("token", serde_json::json!("SECRET")),
    ] {
        let mut data: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        data[field] = value;
        bads.push(draft_zip(&[(
            MANIFEST,
            &serde_json::to_vec(&data).unwrap(),
        )]));
    }
    let mut crc_bad = draft_zip(&[(MANIFEST, &raw)]);
    crc_bad[30 + MANIFEST.len()] ^= 42;
    bads.push(crc_bad);
    bads.push(draft_zip(&[(
        MANIFEST,
        &vec![b' '; MAX_MANIFEST as usize + 1],
    )]));
    let mut huge_declared = draft_zip(&[(MANIFEST, &raw)]);
    let central = huge_declared
        .windows(4)
        .position(|b| b == b"PK\x01\x02")
        .unwrap();
    huge_declared[central + 24..central + 28].copy_from_slice(&u32::MAX.to_le_bytes());
    bads.push(huge_declared);
    let mut link = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    link.add_symlink(
        MANIFEST,
        "outside",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    bads.push(link.finish().unwrap().into_inner());
    for bad in bads {
        std::fs::write(&archive, &bad).unwrap();
        let before = std::fs::read_dir(folder.path()).unwrap().count();
        assert!(projects
            .import_package(Some(&archive), Some(folder.path()))
            .is_err());
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), before);
        assert_eq!(std::fs::read(&archive).unwrap(), bad);
        assert_eq!(
            projects
                .open(Some(Path::new(&first.root)))
                .unwrap()
                .unwrap(),
            first
        );
    }
}

#[test]
fn package_rejects_missing_extra_and_corrupt_managed_media() {
    let folder = parent();
    let projects = Projects::default();
    let first = projects.create(Some(folder.path())).unwrap().unwrap();
    let source = folder.path().join("source.png");
    let image = png_file(&source);
    let saved = projects
        .import_image(&first.id, 0, Some(&source))
        .unwrap()
        .unwrap();
    let manifest = std::fs::read(Path::new(&saved.root).join(MANIFEST)).unwrap();
    let data: Manifest = serde_json::from_slice(&manifest).unwrap();
    let name = &data.input.as_ref().unwrap().path;
    let archive = folder.path().join("bad-media.zip");
    for bytes in [
        draft_zip(&[(MANIFEST, &manifest)]),
        draft_zip(&[(MANIFEST, &manifest), (name, b"not PNG")]),
    ] {
        std::fs::write(&archive, bytes).unwrap();
        assert!(projects
            .import_package(Some(&archive), Some(folder.path()))
            .is_err());
    }
    let mut corrupt = image;
    corrupt[45] ^= 1;
    let mut data = data;
    use sha2::{Digest, Sha256};
    data.input.as_mut().unwrap().sha256 = format!("{:x}", Sha256::digest(&corrupt));
    let manifest = serde_json::to_vec(&data).unwrap();
    std::fs::write(
        &archive,
        draft_zip(&[(MANIFEST, &manifest), (&data.input.unwrap().path, &corrupt)]),
    )
    .unwrap();
    assert!(projects
        .import_package(Some(&archive), Some(folder.path()))
        .is_err());
    assert_eq!(
        projects
            .open(Some(Path::new(&saved.root)))
            .unwrap()
            .unwrap(),
        saved
    );
}

#[test]
fn empty_draft_package_roundtrip_and_package_logs_are_redacted() {
    let folder = parent();
    let projects = Projects::default();
    let first = projects.create(Some(folder.path())).unwrap().unwrap();
    let package = folder.path().join("SECRET.zip");
    projects
        .export_package(&first.id, 0, Some(&package))
        .unwrap();
    let imported = projects
        .import_package(Some(&package), Some(folder.path()))
        .unwrap()
        .unwrap();
    assert!(imported.input.is_none());
    assert!(projects.import_package(None, None).unwrap().is_none());
    for _ in 0..2 {
        assert!(projects
            .export_package(&first.id, 42, Some(&package))
            .is_err());
    }
    let logs = crate::logs::native_logs(0);
    let own: Vec<_> = logs
        .iter()
        .filter(|e| {
            e.text.contains(&first.id)
                && e.text.contains("operation=export-package")
                && e.text.contains("ERROR")
        })
        .collect();
    assert_eq!(own.len(), 1);
    assert!(own
        .iter()
        .all(|e| !e.text.contains("SECRET") && !e.text.contains(&first.root)));
}

#[test]
fn deflated_draft_imports_but_unreferenced_image_and_oversized_archive_do_not() {
    let folder = parent();
    let projects = Projects::default();
    let first = projects.create(Some(folder.path())).unwrap().unwrap();
    let raw = std::fs::read(Path::new(&first.root).join(MANIFEST)).unwrap();
    let source = folder.path().join("package.zip");
    std::fs::write(&source, draft_zip(&[(MANIFEST, &raw)])).unwrap();
    let imported = projects
        .import_package(Some(&source), Some(folder.path()))
        .unwrap()
        .unwrap();
    assert_eq!(imported.draft, first.draft);
    assert_ne!(imported.root, first.root);
    let input_name = "input-00000000-0000-4000-8000-000000000001.png";
    std::fs::write(
        &source,
        draft_zip(&[(MANIFEST, &raw), (input_name, b"extra")]),
    )
    .unwrap();
    assert!(projects
        .import_package(Some(&source), Some(folder.path()))
        .is_err());
    let huge = std::fs::File::create(&source).unwrap();
    huge.set_len(MAX_PNG + MAX_MANIFEST + 8193).unwrap();
    drop(huge);
    assert!(projects
        .import_package(Some(&source), Some(folder.path()))
        .is_err());
    assert_eq!(
        projects
            .open(Some(Path::new(&imported.root)))
            .unwrap()
            .unwrap(),
        imported
    );
}

#[cfg(target_os = "linux")]
#[test]
fn package_files_reject_symlinks_and_failed_parent_writes_preserve_active() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let folder = parent();
    let projects = Projects::default();
    let first = projects.create(Some(folder.path())).unwrap().unwrap();
    let archive = folder.path().join("draft.zip");
    projects
        .export_package(&first.id, 0, Some(&archive))
        .unwrap();
    let linked = folder.path().join("linked.zip");
    symlink(&archive, &linked).unwrap();
    assert!(projects
        .import_package(Some(&linked), Some(folder.path()))
        .is_err());
    assert!(projects
        .export_package(&first.id, 0, Some(&linked))
        .is_err());
    let target = folder.path().join("read-only");
    std::fs::create_dir(&target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o500)).unwrap();
    assert!(projects
        .import_package(Some(&archive), Some(&target))
        .is_err());
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        projects
            .open(Some(Path::new(&first.root)))
            .unwrap()
            .unwrap(),
        first
    );
    assert!(archive.is_file());
    let source = folder.path().join("input.png");
    png_file(&source);
    let saved = projects
        .import_image(&first.id, 0, Some(&source))
        .unwrap()
        .unwrap();
    std::fs::write(&saved.input.as_ref().unwrap().path, b"corrupt").unwrap();
    let destination = folder.path().join("corrupt.zip");
    assert!(projects
        .export_package(&saved.id, saved.revision, Some(&destination))
        .is_err());
    assert!(!destination.exists());
}

#[test]
fn window_close_diagnostics_are_fixed_redacted_and_rate_limited() {
    assert!(native_project_close_notice("SECRET/path".into()).is_err());
    for _ in 0..2 {
        native_project_close_notice("blocked".into()).unwrap();
        native_project_close_notice("unavailable".into()).unwrap();
    }
    let entries = crate::logs::native_logs(0);
    let own: Vec<_> = entries
        .iter()
        .filter(|e| e.text.contains("Projects operation=window-close "))
        .collect();
    assert_eq!(own.len(), 2);
    assert!(own
        .iter()
        .any(|e| e.text.starts_with("INFO:") && e.text.contains("blocked")));
    assert!(own
        .iter()
        .any(|e| e.text.starts_with("ERROR:") && e.text.contains("unavailable")));
    assert!(own
        .iter()
        .all(|e| e.source == crate::logs::Source::Deas && !e.text.contains("SECRET")));
}

fn png_chunk(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut crc = flate2::Crc::new();
    crc.update(kind);
    crc.update(payload);
    let mut chunk = (payload.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(kind);
    chunk.extend_from_slice(payload);
    chunk.extend_from_slice(&crc.sum().to_be_bytes());
    chunk
}
#[test]
fn png_envelope_checksums_and_metadata_are_strict_without_decompressing_ancillary_data() {
    let folder = parent();
    let bytes = png_file(&folder.path().join("base.png"));
    let mut trailing = bytes.clone();
    trailing.extend_from_slice(b"hidden");
    assert!(png_dimensions(&trailing).is_err());
    for kind in [b"zTXt", b"iCCP"] {
        let chunk = png_chunk(kind, b"metadata\0\0not-a-zlib-stream");
        let mut with_ignored_metadata = bytes[..33].to_vec();
        with_ignored_metadata.extend_from_slice(&chunk);
        with_ignored_metadata.extend_from_slice(&bytes[33..]);
        assert_eq!(png_dimensions(&with_ignored_metadata).unwrap(), (2, 3));
        with_ignored_metadata[33 + chunk.len() - 1] ^= 1;
        assert!(png_dimensions(&with_ignored_metadata).is_err());
    }
    let mut animated = bytes[..33].to_vec();
    animated.extend_from_slice(&png_chunk(b"acTL", &[0, 0, 0, 1, 0, 0, 0, 0]));
    animated.extend_from_slice(&bytes[33..]);
    assert!(png_dimensions(&animated).is_err());
    let idat = bytes.windows(4).position(|s| s == b"IDAT").unwrap();
    let length = u32::from_be_bytes(bytes[idat - 4..idat].try_into().unwrap()) as usize;
    let mut corrupt_adler = bytes.clone();
    corrupt_adler[idat + 4 + length - 1] ^= 1;
    let chunk = png_chunk(b"IDAT", &corrupt_adler[idat + 4..idat + 4 + length]);
    corrupt_adler[idat - 4..idat + 4 + length + 4].copy_from_slice(&chunk);
    assert!(png_dimensions(&corrupt_adler).is_err());
}
#[test]
fn external_manifest_replacement_is_not_overwritten_by_stale_active_state() {
    let folder = parent();
    let projects = Projects::default();
    let first = projects.create(Some(folder.path())).unwrap().unwrap();
    let path = Path::new(&first.root).join(MANIFEST);
    let mut changed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    changed["draft"]["positivePrompt"] = serde_json::json!("external edit");
    let bytes = serde_json::to_vec(&changed).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    assert!(projects.save(&first.id, 0, first.draft.clone()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

fn png_file(path: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 3);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[42; 24])
            .unwrap();
    }
    std::fs::write(path, &bytes).unwrap();
    bytes
}

#[test]
fn import_preserves_original_manages_copy_and_reopens_with_relative_disk_path() {
    let folder = parent();
    let source = folder.path().join("source 日本語.png");
    let bytes = png_file(&source);
    let projects = Projects::default();
    let first = projects.create(Some(folder.path())).unwrap().unwrap();
    assert!(projects.import_image(&first.id, 0, None).unwrap().is_none());
    let snapshot = projects
        .import_image(&first.id, 0, Some(&source))
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.revision, 1);
    let input = snapshot.input.as_ref().unwrap();
    assert_eq!((input.width, input.height), (2, 3));
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    assert_eq!(std::fs::read(&input.path).unwrap(), bytes);
    let raw = std::fs::read(Path::new(&first.root).join(MANIFEST)).unwrap();
    let disk: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert!(!Path::new(disk["input"]["path"].as_str().unwrap()).is_absolute());
    assert!(disk.get("root").is_none());
    assert!(projects.import_image(&first.id, 0, Some(&source)).is_err());
    assert_eq!(
        std::fs::read(Path::new(&first.root).join(MANIFEST)).unwrap(),
        raw
    );
    std::fs::remove_file(&source).unwrap();
    projects.close(&first.id).unwrap();
    assert_eq!(
        projects
            .open(Some(Path::new(&first.root)))
            .unwrap()
            .unwrap(),
        snapshot
    );
    let source2 = folder.path().join("second.png");
    png_file(&source2);
    let second = projects
        .import_image(&first.id, 1, Some(&source2))
        .unwrap()
        .unwrap();
    assert_ne!(second.input.unwrap().path, input.path);
    assert_eq!(std::fs::read(&input.path).unwrap(), bytes);
}

#[test]
fn invalid_dialog_titles_are_rejected() {
    assert!(validate_title("Create project").is_ok());
    assert!(validate_title("bad\ntitle").is_err());
    assert!(validate_title("").is_err());
    assert!(validate_title("   ").is_err());
    assert!(validate_title(&"x".repeat(241)).is_err());
}

fn parent() -> tempfile::TempDir {
    tempfile::tempdir_in(std::env::temp_dir()).unwrap()
}

#[test]
fn corrupt_future_unknown_and_unsafe_input_manifests_fail_closed() {
    let folder = parent();
    let projects = Projects::default();
    let mut good = projects.create(Some(folder.path())).unwrap().unwrap();
    let bad = Projects::default()
        .create(Some(folder.path()))
        .unwrap()
        .unwrap();
    let path = Path::new(&bad.root).join(MANIFEST);
    let original = std::fs::read(&path).unwrap();
    let base: serde_json::Value = serde_json::from_slice(&original).unwrap();
    for (key, value) in [
        ("schemaVersion", serde_json::json!(2)),
        ("token", serde_json::json!("secret")),
        (
            "input",
            serde_json::json!({"id": "00000000-0000-4000-8000-000000000001", "path": "../source.png", "sha256": "0".repeat(64), "width": 2, "height": 3}),
        ),
        (
            "input",
            serde_json::json!({"id": "00000000-0000-4000-8000-000000000001", "path": "input-00000000-0000-4000-8000-000000000001.png", "sha256": "0".repeat(64), "width": 2, "height": 3}),
        ),
    ] {
        let mut changed = base.clone();
        changed[key] = value;
        std::fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(
            projects.open(Some(Path::new(&bad.root))).is_err(),
            "accepted {key}: {changed}"
        );
        good = projects
            .save(&good.id, good.revision, good.draft.clone())
            .unwrap();
        // Keep checking that the previous active project survived the failed open.
        let reopened = projects.open(Some(Path::new(&good.root))).unwrap().unwrap();
        assert_eq!(reopened.id, good.id);
    }
    std::fs::write(&path, b"not json").unwrap();
    assert!(projects.open(Some(Path::new(&bad.root))).is_err());
    let mut missing = base;
    missing.as_object_mut().unwrap().remove("input");
    std::fs::write(&path, serde_json::to_vec(&missing).unwrap()).unwrap();
    assert!(projects.open(Some(Path::new(&bad.root))).is_err());
    let huge = std::fs::File::create(&path).unwrap();
    huge.set_len(MAX_MANIFEST + 1).unwrap();
    assert!(projects.open(Some(Path::new(&bad.root))).is_err());
}

#[test]
fn cancelled_operations_are_distinct_and_errors_are_redacted_and_rate_limited() {
    let folder = parent();
    let projects = Projects::default();
    let first = projects.create(Some(folder.path())).unwrap().unwrap();
    let mut secret = first.draft.clone();
    secret.positive_prompt = "DO-NOT-LOG-PROMPT".into();
    secret.checkpoint = "DO-NOT-LOG-PATH/../secret".into();
    assert!(projects.save(&first.id, 0, secret.clone()).is_err());
    assert!(projects.save(&first.id, 0, secret).is_err());
    projects.import_image(&first.id, 0, None).unwrap();
    let logs = crate::logs::native_logs(0);
    let own: Vec<_> = logs.iter().filter(|e| e.text.contains(&first.id)).collect();
    let errors: Vec<_> = own
        .iter()
        .filter(|e| e.text.contains("ERROR") && e.text.contains("operation=save"))
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].source, crate::logs::Source::Deas);
    assert!(own
        .iter()
        .any(|e| e.text.contains("operation=import-image") && e.text.contains("cancelled")));
    assert!(own
        .iter()
        .all(|e| !e.text.contains("DO-NOT-LOG") && !e.text.contains(&first.root)));
}

#[test]
fn invalid_pngs_and_oversized_files_leave_manifest_and_source_unchanged() {
    let folder = parent();
    let projects = Projects::default();
    let first = projects.create(Some(folder.path())).unwrap().unwrap();
    let manifest = Path::new(&first.root).join(MANIFEST);
    let before = std::fs::read(&manifest).unwrap();
    let source = folder.path().join("invalid.png");
    let bytes = png_file(&source);
    let mut crc_bad = bytes.clone();
    crc_bad[45] ^= 1;
    for bad in [
        b"not a PNG".to_vec(),
        bytes[..bytes.len() - 8].to_vec(),
        bytes[..33].to_vec(),
        crc_bad,
    ] {
        std::fs::write(&source, &bad).unwrap();
        assert!(projects.import_image(&first.id, 0, Some(&source)).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), bad);
        assert_eq!(std::fs::read(&manifest).unwrap(), before);
    }
    let mut large_dimensions = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut large_dimensions, 8193, 1);
        encoder.set_color(png::ColorType::Grayscale);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&vec![0; 8193])
            .unwrap();
    }
    std::fs::write(&source, &large_dimensions).unwrap();
    assert!(projects.import_image(&first.id, 0, Some(&source)).is_err());
    let file = std::fs::File::create(&source).unwrap();
    file.set_len(MAX_PNG + 1).unwrap();
    drop(file);
    assert!(projects.import_image(&first.id, 0, Some(&source)).is_err());
    assert_eq!(std::fs::metadata(&source).unwrap().len(), MAX_PNG + 1);
    assert_eq!(std::fs::read(&manifest).unwrap(), before);
}

#[test]
fn tampered_managed_image_hash_and_dimensions_fail_on_reopen() {
    let folder = parent();
    let source = folder.path().join("source.png");
    png_file(&source);
    let projects = Projects::default();
    let first = projects.create(Some(folder.path())).unwrap().unwrap();
    let saved = projects
        .import_image(&first.id, 0, Some(&source))
        .unwrap()
        .unwrap();
    std::fs::write(saved.input.as_ref().unwrap().path.clone(), b"broken").unwrap();
    assert!(projects.open(Some(Path::new(&first.root))).is_err());
    projects.close(&first.id).unwrap();
    png_file(Path::new(&saved.input.as_ref().unwrap().path));
    let path = Path::new(&first.root).join(MANIFEST);
    let base: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for (key, value) in [
        ("sha256", serde_json::json!("f".repeat(64))),
        ("width", serde_json::json!(1)),
        ("path", serde_json::json!("C:\\secret.png")),
    ] {
        let mut bad = base.clone();
        bad["input"][key] = value;
        std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
        assert!(projects.open(Some(Path::new(&first.root))).is_err());
    }
    std::fs::write(&path, serde_json::to_vec(&base).unwrap()).unwrap();
    std::fs::write(saved.input.unwrap().path, b"broken").unwrap();
    assert!(projects.open(Some(Path::new(&first.root))).is_err());
}

#[test]
fn create_cancel_save_close_reopen_and_single_writer() {
    let folder = parent();
    let projects = Projects::default();
    assert!(projects.create(None).unwrap().is_none());
    let first = projects.create(Some(folder.path())).unwrap().unwrap();
    // Roots are canonical; Windows temp may be an 8.3 short path.
    assert!(Path::new(&first.root).starts_with(dunce::canonicalize(folder.path()).unwrap()));
    assert_ne!(Path::new(&first.root), folder.path());
    assert_eq!(first.draft, ImageDraft::default());
    let other = Projects::default();
    assert!(other.open(Some(Path::new(&first.root))).is_err());
    assert_eq!(
        projects
            .open(Some(Path::new(&first.root)))
            .unwrap()
            .unwrap(),
        first
    );
    assert!(projects.open(None).unwrap().is_none());
    assert!(other.open(Some(Path::new(&first.root))).is_err());
    assert!(projects.open(Some(folder.path())).is_err());
    let mut draft = first.draft.clone();
    draft.positive_prompt = "private prompt 日本語".into();
    let saved = projects
        .save(&first.id, first.revision, draft.clone())
        .unwrap();
    assert_eq!(saved.revision, first.revision + 1);
    assert!(projects.save(&first.id, first.revision, draft).is_err());
    projects.close(&first.id).unwrap();
    let reopened = other.open(Some(Path::new(&first.root))).unwrap().unwrap();
    assert_eq!(reopened, saved);
    other.close(&first.id).unwrap();
    let next = projects.create(Some(folder.path())).unwrap().unwrap();
    assert_ne!(first.root, next.root);
    assert!(Path::new(&first.root).join(MANIFEST).is_file());
}

#[test]
fn default_draft_roundtrips_strictly_and_rejects_invalid_settings() {
    let draft = ImageDraft::default();
    assert_eq!(draft.width, 1024);
    assert_eq!(draft.pipeline, "t2i");
    assert!(draft.validate().is_ok());
    let mut bad = draft.clone();
    bad.width = 65;
    assert!(bad.validate().is_err());
    let mut value = serde_json::to_value(&draft).unwrap();
    value["token"] = serde_json::json!("private");
    assert!(serde_json::from_value::<ImageDraft>(value).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn symlinked_parent_root_source_manifest_and_lock_are_rejected() {
    use std::os::unix::fs::symlink;
    let folder = parent();
    let projects = Projects::default();
    let first = projects.create(Some(folder.path())).unwrap().unwrap();
    let root = Path::new(&first.root);
    let linked = folder.path().join("linked");
    symlink(root, &linked).unwrap();
    assert!(projects.open(Some(&linked)).is_err());
    assert!(projects.create(Some(&linked)).is_err());
    assert!(projects.open(Some(&root.join(".."))).is_err());
    let source = folder.path().join("source.png");
    png_file(&source);
    let link = folder.path().join("source-link.png");
    symlink(&source, &link).unwrap();
    assert!(projects.import_image(&first.id, 0, Some(&link)).is_err());
    let source_dir_link = folder.path().join("source-dir");
    symlink(folder.path(), &source_dir_link).unwrap();
    assert!(projects
        .import_image(&first.id, 0, Some(&source_dir_link.join("source.png")))
        .is_err());
    projects.close(&first.id).unwrap();
    let manifest = root.join(MANIFEST);
    let saved = std::fs::read(&manifest).unwrap();
    std::fs::remove_file(&manifest).unwrap();
    symlink(&source, &manifest).unwrap();
    assert!(projects.open(Some(root)).is_err());
    std::fs::remove_file(&manifest).unwrap();
    std::fs::write(&manifest, &saved).unwrap();
    std::fs::remove_file(root.join(LOCK)).unwrap();
    symlink(&source, root.join(LOCK)).unwrap();
    assert!(projects.open(Some(root)).is_err());
    assert_eq!(
        std::fs::read(&source).unwrap(),
        png_file(&folder.path().join("comparison.png"))
    );
}

#[cfg(target_os = "linux")]
#[test]
fn failed_save_preserves_previous_manifest_and_active_revision() {
    use std::os::unix::fs::PermissionsExt;
    let folder = parent();
    let projects = Projects::default();
    let first = projects.create(Some(folder.path())).unwrap().unwrap();
    let root = Path::new(&first.root);
    let original = std::fs::read(root.join(MANIFEST)).unwrap();
    std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = projects.save(&first.id, 0, first.draft.clone());
    std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    assert_eq!(std::fs::read(root.join(MANIFEST)).unwrap(), original);
    let next = projects.save(&first.id, 0, first.draft.clone()).unwrap();
    assert_eq!(next.revision, 1);
    let saved = std::fs::read(root.join(MANIFEST)).unwrap();
    assert!(projects.save(&first.id, 0, first.draft.clone()).is_err());
    assert_eq!(std::fs::read(root.join(MANIFEST)).unwrap(), saved);
}

#[test]
fn lock_probe_child() {
    if let Some(root) = std::env::var_os("DEAS_PROJECT_LOCK_TEST_ROOT") {
        assert!(Projects::default().open(Some(Path::new(&root))).is_err());
    }
}
#[test]
fn project_lock_is_rejected_by_an_independent_process_and_released_on_drop() {
    let folder = parent();
    let projects = Projects::default();
    let first = projects.create(Some(folder.path())).unwrap().unwrap();
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "projects::tests::lock_probe_child",
            "--nocapture",
        ])
        .env("DEAS_PROJECT_LOCK_TEST_ROOT", &first.root)
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}",
        String::from_utf8_lossy(&child.stdout)
    );
    drop(projects);
    assert!(Projects::default()
        .open(Some(Path::new(&first.root)))
        .is_ok());
}

#[test]
fn draft_bounds_match_the_image_recipe_without_requiring_a_selected_model() {
    let base = serde_json::to_value(ImageDraft::default()).unwrap();
    for (key, value) in [
        ("width", serde_json::json!(8193)),
        ("height", serde_json::json!(0)),
        ("steps", serde_json::json!(151)),
        ("steps", serde_json::json!(0)),
        ("cfg", serde_json::json!(30.1)),
        ("denoise", serde_json::json!(-0.1)),
        ("seed", serde_json::json!(9_007_199_254_740_992_u64)),
        ("checkpoint", serde_json::json!("../model")),
        ("sampler", serde_json::json!("bad/choice")),
        ("scheduler", serde_json::json!("")),
        ("pipeline", serde_json::json!("video")),
        ("positivePrompt", serde_json::json!("x".repeat(16385))),
        ("negativePrompt", serde_json::json!("nul\u{0000}")),
    ] {
        let mut value_with_bad = base.clone();
        value_with_bad[key] = value;
        let draft: ImageDraft = serde_json::from_value(value_with_bad).unwrap();
        assert!(draft.validate().is_err(), "accepted {key}");
    }
    let mut big = ImageDraft {
        width: 8192,
        height: 8192,
        ..ImageDraft::default()
    };
    assert!(big.validate().is_err());
    big.width = 4096;
    big.height = 4096;
    big.seed = 9_007_199_254_740_991;
    big.positive_prompt = "😀".repeat(8192);
    big.pipeline = "i2i".into();
    assert!(big.validate().is_ok());
    big.cfg = f64::NAN;
    assert!(big.validate().is_err());
}
