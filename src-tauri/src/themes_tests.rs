use super::*;
use serde_json::json;

#[test]
fn all_existing_theme_packs_are_valid_without_mutating_them() {
    let t = env();
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("theme-packs");
    fs::create_dir_all(t.path().join("themes")).unwrap();
    ensure_starter_pack(&t.path().join("themes"), &source).unwrap();
    let result = scan(t.path().to_str().unwrap()).unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert_eq!(result.packs.len(), 1);
    assert_eq!(result.packs[0].id, "new-theme");
}

#[test]
fn starter_pack_is_created_once_without_overwriting_user_edits() {
    let source = env();
    let target = env();
    fs::create_dir_all(source.path().join("new-theme")).unwrap();
    fs::write(
        source.path().join("new-theme/theme.json"),
        r#"{"schemaVersion":1,"id":"new-theme","name":"Starter","author":"You","base":"default"}"#,
    )
    .unwrap();

    ensure_starter_pack(&target.path().join("themes"), source.path()).unwrap();
    fs::write(
        target.path().join("themes/new-theme/theme.json"),
        r#"{"schemaVersion":1,"id":"my-theme","name":"Edited","author":"You","base":"default"}"#,
    )
    .unwrap();
    ensure_starter_pack(&target.path().join("themes"), source.path()).unwrap();

    let edited = fs::read_to_string(target.path().join("themes/new-theme/theme.json")).unwrap();
    assert!(edited.contains("my-theme"));
    assert!(edited.contains("Edited"));
}

fn env() -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    fs::write(t.path().join("deas-install.json"), "{}").unwrap();
    t
}
fn manifest(id: &str) -> serde_json::Value {
    json!({"schemaVersion":1,"id":id,"name":"Theme","author":"DEAS","base":"default","colors":{"panel":"#123456","ok":"#abc"}})
}
fn put(t: &tempfile::TempDir, folder: &str, value: &serde_json::Value) -> PathBuf {
    let p = t.path().join("themes").join(folder);
    fs::create_dir_all(&p).unwrap();
    fs::write(p.join("theme.json"), serde_json::to_vec(value).unwrap()).unwrap();
    p
}
fn png_file(path: &Path) {
    let file = fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&[255; 16])
        .unwrap();
}
#[test]
fn bom_prefixed_manifest_is_supported() {
    let t = env();
    let p = put(&t, "bom", &manifest("bom"));
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend(serde_json::to_vec(&manifest("bom")).unwrap());
    fs::write(p.join("theme.json"), bytes).unwrap();
    let catalog = scan(t.path().to_str().unwrap()).unwrap();
    assert!(catalog.warnings.is_empty(), "{:?}", catalog.warnings);
    assert_eq!(catalog.packs.len(), 1);
}

#[test]
fn animated_png_is_rejected() {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_animated(2, 0).unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[255; 16]).unwrap();
        writer.write_image_data(&[0; 16]).unwrap();
    }
    assert!(image_mime(&bytes, false).is_err());
}

#[cfg(unix)]
#[test]
fn bounded_reads_reject_symlinked_parent_components() {
    use std::os::unix::fs::symlink;
    let t = env();
    let p = put(&t, "pack", &manifest("pack"));
    symlink(&p, t.path().join("alias")).unwrap();
    assert!(bounded(&t.path().join("alias/theme.json"), MANIFEST_LIMIT).is_err());
}

#[test]
fn missing_folder_is_read_only() {
    let t = env();
    assert!(scan(t.path().to_str().unwrap()).unwrap().packs.is_empty());
    assert!(!t.path().join("themes").exists());
    assert!(scan("relative").is_err());
}
#[test]
fn valid_colors_and_local_images() {
    let t = env();
    let mut m = manifest("sage");
    m["assets"] = json!({"cursor":"cursor.png"});
    let p = put(&t, "sage", &m);
    png_file(&p.join("cursor.png"));
    let result = scan(t.path().to_str().unwrap()).unwrap();
    assert_eq!(result.packs.len(), 1);
    assert!(result.packs[0].assets["cursor"].starts_with("data:image/png;base64,"));
    assert_eq!(result.packs[0].colors["panel"], "#123456");
}

#[test]
fn structured_assets_and_webp_are_supported_with_correct_mime() {
    let t = env();
    let mut m = manifest("structured");
    m["assets"] = json!({"cursor":"assets/cursor.png","background":"assets/background.webp"});
    let p = put(&t, "structured", &m);
    fs::create_dir(p.join("assets")).unwrap();
    png_file(&p.join("assets/cursor.png"));
    // Lossless 2x2 WebP encoded by the same maintained pure-Rust image codec.
    let mut webp = Vec::new();
    image_webp::WebPEncoder::new(&mut webp)
        .encode(&[255; 16], 2, 2, image_webp::ColorType::Rgba8)
        .unwrap();
    assert_eq!(image_mime(&webp, true).unwrap(), "image/webp");
    let mut extended = b"RIFF".to_vec();
    extended.extend_from_slice(&(webp.len() as u32 - 8 + 18).to_le_bytes());
    extended.extend_from_slice(b"WEBPVP8X");
    extended.extend_from_slice(&10u32.to_le_bytes());
    extended.extend_from_slice(&[0, 0, 0, 0, 1, 0, 0, 1, 0, 0]);
    extended.extend_from_slice(&webp[12..]);
    assert_eq!(image_mime(&extended, true).unwrap(), "image/webp");
    fs::write(p.join("assets/background.webp"), extended).unwrap();
    let result = scan(t.path().to_str().unwrap()).unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert!(result.packs[0].assets["background"].starts_with("data:image/webp;base64,"));
    assert!(result.packs[0].assets["cursor"].starts_with("data:image/png;base64,"));
}

#[test]
fn structured_assets_reject_traversal_aliases_and_invalid_webp() {
    let t = env();
    for (i, file) in [
        "assets/../cursor.png",
        "assets/sub/cursor.png",
        "assets\\cursor.png",
        "C:/cursor.png",
        "assets/.hidden.png",
        "assets/Cursor.png",
        "/assets/cursor.png",
        "other/cursor.png",
    ]
    .iter()
    .enumerate()
    {
        let mut m = manifest(&format!("bad-path-{i}"));
        m["assets"] = json!({"cursor":file});
        let p = put(&t, &format!("bad-path-{i}"), &m);
        fs::create_dir(p.join("assets")).unwrap();
        png_file(&p.join("assets/cursor.png"));
    }
    let mut m = manifest("invalid-webp");
    m["assets"] = json!({"background":"assets/background.webp"});
    let p = put(&t, "invalid-webp", &m);
    fs::create_dir(p.join("assets")).unwrap();
    png_file(&p.join("assets/background.webp"));
    let result = scan(t.path().to_str().unwrap()).unwrap();
    assert!(result.packs.is_empty());
    assert_eq!(result.warnings.len(), 9);
}
#[test]
fn no_remote_paths_scripts_or_unknown_schema() {
    let t = env();
    for (i, value) in [
        json!({"background":"url(https://example.com)"}),
        json!({"assets":{"x":"../private.png"}}),
        json!({"assets":{"x":"https://example.com/a.png"}}),
        json!({"colors":{"panel":"url(secret)"}}),
        json!({"colors":{"width":"#fff"}}),
        json!({"script":"app.js"}),
        json!({"schemaVersion":2}),
        json!({"presentation":"execute"}),
    ]
    .iter()
    .enumerate()
    {
        let mut m = manifest(&format!("bad-{i}"));
        m.as_object_mut()
            .unwrap()
            .extend(value.as_object().unwrap().clone());
        put(&t, &format!("bad-{i}"), &m);
    }
    let result = scan(t.path().to_str().unwrap()).unwrap();
    assert!(result.packs.is_empty());
    assert_eq!(result.warnings.len(), 8);
}
#[test]
fn invalid_pack_does_not_break_valid_pack_and_duplicates_are_not_order_dependent() {
    let t = env();
    put(&t, "a", &manifest("duplicate"));
    put(&t, "b", &manifest("duplicate"));
    put(&t, "valid", &manifest("valid"));
    let p = put(&t, "invalid", &manifest("invalid"));
    fs::write(p.join("theme.json"), "broken").unwrap();
    let r = scan(t.path().to_str().unwrap()).unwrap();
    assert_eq!(r.packs.len(), 1);
    assert_eq!(r.packs[0].id, "valid");
    assert_eq!(r.warnings.len(), 2);
}
#[test]
fn size_and_image_content_are_bounded() {
    let t = env();
    let p = put(&t, "oversize", &manifest("large"));
    fs::write(
        p.join("theme.json"),
        vec![b' '; MANIFEST_LIMIT as usize + 1],
    )
    .unwrap();
    let mut m = manifest("fake");
    m["assets"] = json!({"image":"image.png"});
    let p = put(&t, "fake", &m);
    fs::write(p.join("image.png"), "not a PNG").unwrap();
    let r = scan(t.path().to_str().unwrap()).unwrap();
    assert!(r.packs.is_empty());
    assert_eq!(r.warnings.len(), 2);
}
#[test]
fn static_images_enforce_dimensions_and_catalog_budget() {
    let mut webp = Vec::new();
    image_webp::WebPEncoder::new(&mut webp)
        .encode(&vec![255; 4097 * 4], 4097, 1, image_webp::ColorType::Rgba8)
        .unwrap();
    assert!(image_mime(&webp, true).unwrap_err().contains("dimensions"));
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 4097, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&vec![255; 4097 * 4])
            .unwrap();
    }
    assert!(image_mime(&bytes, false)
        .unwrap_err()
        .contains("dimensions"));
    let t = env();
    let mut m = manifest("budget");
    m["assets"] = json!({"cursor":"cursor.png"});
    let p = put(&t, "budget", &m);
    png_file(&p.join("cursor.png"));
    let mut used = CATALOG_LIMIT;
    assert!(read_pack(&p, &mut used).unwrap_err().contains("budget"));
    assert_eq!(used, CATALOG_LIMIT);
    fs::write(p.join("cursor.png"), vec![0; ASSET_LIMIT as usize + 1]).unwrap();
    assert!(read_pack(&p, &mut 0).unwrap_err().contains("size limit"));
}

#[test]
fn animated_webp_is_rejected_before_frame_decode() {
    let mut frame = Vec::new();
    image_webp::WebPEncoder::new(&mut frame)
        .encode(&[255; 16], 2, 2, image_webp::ColorType::Rgba8)
        .unwrap();
    let mut chunks = Vec::new();
    let mut frame_header = vec![0; 16];
    frame_header[6] = 1;
    frame_header[9] = 1;
    frame_header[12] = 1;
    frame_header.extend_from_slice(&frame[12..]);
    for (tag, payload) in [
        (b"VP8X", vec![2, 0, 0, 0, 1, 0, 0, 1, 0, 0]),
        (b"ANIM", vec![0; 6]),
        (b"ANMF", frame_header),
    ] {
        chunks.extend_from_slice(tag);
        chunks.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        chunks.extend_from_slice(&payload);
        if payload.len() % 2 != 0 {
            chunks.push(0);
        }
    }
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&(chunks.len() as u32 + 4).to_le_bytes());
    bytes.extend_from_slice(b"WEBP");
    bytes.extend(chunks);
    let decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(&bytes)).unwrap();
    assert!(decoder.is_animated());
    assert!(image_mime(&bytes, true).unwrap_err().contains("animated"));
}

#[test]
fn extended_webp_cannot_hide_oversized_coded_dimensions() {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(b"WEBPVP8X");
    bytes.extend_from_slice(&10u32.to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0, 1, 0, 0, 1, 0, 0]);
    bytes.extend_from_slice(b"VP8 ");
    bytes.extend_from_slice(&10u32.to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0x9d, 1, 0x2a]);
    bytes.extend_from_slice(&4097u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    let decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(&bytes)).unwrap();
    assert_eq!(decoder.dimensions(), (2, 2));
    assert!(image_mime(&bytes, true).unwrap_err().contains("dimensions"));
}

#[test]
fn native_theme_commands_are_local_and_registered() {
    let capability: serde_json::Value =
        serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
    assert_eq!(capability["local"], true);
    assert_eq!(capability["windows"], json!(["main"]));
    assert!(capability.get("remote").is_none());
    assert!(capability["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p == "allow-native-theme-packs"));
    let permission: toml::Value =
        toml::from_str(include_str!("../permissions/themes.toml")).unwrap();
    assert_eq!(
        permission["permission"][0]["commands"]["allow"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "native_theme_packs",
            "native_theme_editor",
            "native_theme_folders",
            "native_theme_read",
            "native_theme_write",
        ]
    );
    for command in [
        "native_theme_packs",
        "native_theme_editor",
        "native_theme_folders",
        "native_theme_read",
        "native_theme_write",
    ] {
        assert!(include_str!("main.rs").contains(&format!("deas::{command},")));
    }
}

#[test]
fn editor_reads_lists_and_writes_only_inside_one_pack() {
    let t = env();
    put(&t, "new-theme", &manifest("new-theme"));
    put(&t, "other", &manifest("other-theme"));
    fs::create_dir_all(t.path().join("themes/no-manifest")).unwrap();
    assert_eq!(
        list_folders(t.path().to_str().unwrap()).unwrap(),
        ["new-theme", "other"]
    );
    let read = read_file(t.path().to_str().unwrap(), "new-theme", "theme.json").unwrap();
    assert!(String::from_utf8(STANDARD.decode(read.unwrap()).unwrap())
        .unwrap()
        .contains("new-theme"));
    assert!(read_file(
        t.path().to_str().unwrap(),
        "new-theme",
        "assets/missing.png"
    )
    .unwrap()
    .is_none());

    let changed = json!({"schemaVersion":1,"id":"new-theme","name":"Edited","author":"DEAS","base":"default"});
    let encoded = STANDARD.encode(serde_json::to_vec(&changed).unwrap());
    write_file(
        t.path().to_str().unwrap(),
        "new-theme",
        "theme.json",
        &encoded,
    )
    .unwrap();
    assert_eq!(scan(t.path().to_str().unwrap()).unwrap().packs.len(), 2);
    let back = read_file(t.path().to_str().unwrap(), "new-theme", "theme.json")
        .unwrap()
        .unwrap();
    assert!(STANDARD
        .decode(&back)
        .unwrap()
        .windows(6)
        .any(|w| w == b"Edited"));

    for path in [
        "../escape.json",
        "theme.json/../../x",
        "assets/../theme.json",
        "other/file.png",
        "outside/theme.json",
    ] {
        assert!(
            read_file(t.path().to_str().unwrap(), "new-theme", path).is_err(),
            "{path}"
        );
        assert!(
            write_file(t.path().to_str().unwrap(), "new-theme", path, &encoded).is_err(),
            "{path}"
        );
    }
    assert!(write_file(t.path().to_str().unwrap(), "..", "theme.json", &encoded).is_err());
    assert!(!t.path().join("escape.json").exists());
}

#[test]
fn editor_writes_reject_invalid_content_and_links() {
    let t = env();
    put(&t, "new-theme", &manifest("new-theme"));
    let broken = json!({"schemaVersion":1,"id":"new-theme","name":"x".repeat(121),"author":"DEAS","base":"default"});
    let encoded = STANDARD.encode(serde_json::to_vec(&broken).unwrap());
    assert!(write_file(
        t.path().to_str().unwrap(),
        "new-theme",
        "theme.json",
        &encoded
    )
    .is_err());
    assert!(write_file(
        t.path().to_str().unwrap(),
        "new-theme",
        "theme.json",
        "not base64"
    )
    .is_err());
    assert!(write_file(
        t.path().to_str().unwrap(),
        "new-theme",
        "assets/x.png",
        &STANDARD.encode(b"not an image")
    )
    .is_err());
    let before = read_file(t.path().to_str().unwrap(), "new-theme", "theme.json")
        .unwrap()
        .unwrap();

    let directory = t.path().join("themes/new-theme");
    let backup = directory.join("backups/theme-old.json");
    fs::create_dir_all(backup.parent().unwrap()).unwrap();
    let encoded = STANDARD.encode(serde_json::to_vec(&manifest("new-theme")).unwrap());
    write_file(
        t.path().to_str().unwrap(),
        "new-theme",
        "backups/theme-old.json",
        &encoded,
    )
    .unwrap();
    assert!(backup.is_file());

    fs::remove_file(backup.clone()).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(t.path().join("deas-install.json"), &backup).unwrap();
        assert!(write_file(
            t.path().to_str().unwrap(),
            "new-theme",
            "backups/theme-old.json",
            &encoded
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(t.path().join("deas-install.json")).unwrap(),
            "{}"
        );
    }
    assert_eq!(
        read_file(t.path().to_str().unwrap(), "new-theme", "theme.json")
            .unwrap()
            .unwrap(),
        before
    );
}

#[test]
fn diagnostics_are_shared_redacted_and_rate_limited() {
    let t = env();
    let mut m = manifest("rejected");
    m["secret"] = json!("SUPER_SECRET_theme_payload");
    put(&t, "rejected", &m);
    for _ in 0..2 {
        assert_eq!(scan(t.path().to_str().unwrap()).unwrap().warnings.len(), 1);
    }
    let lines = crate::logs::native_logs(0)
        .into_iter()
        .filter(|l| l.text.starts_with("ERROR: Theme packs"))
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].source, crate::logs::Source::Deas);
    assert!(lines[0].text.contains("scan/open:"));
    assert!(!lines[0].text.contains("SUPER_SECRET_theme_payload"));
    assert!(!lines[0].text.contains(t.path().to_str().unwrap()));
}
#[cfg(windows)]
#[test]
fn junction_asset_directories_are_rejected() {
    let t = env();
    let outside = tempfile::tempdir().unwrap();
    png_file(&outside.path().join("cursor.png"));
    let mut m = manifest("linked-assets");
    m["assets"] = json!({"cursor":"assets/cursor.png"});
    let p = put(&t, "linked-assets", &m);
    let result = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(p.join("assets"))
        .arg(outside.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "Cannot create test junction: {:?}",
        result
    );
    let catalog = scan(t.path().to_str().unwrap()).unwrap();
    assert!(catalog.packs.is_empty());
    assert_eq!(catalog.warnings.len(), 1);
    assert!(outside.path().join("cursor.png").is_file());
}
#[cfg(unix)]
#[test]
fn linked_manifests_assets_and_catalog_roots_are_rejected() {
    use std::os::unix::fs::symlink;
    for part in ["theme.json", "assets", "assets/cursor.png"] {
        let t = env();
        let outside = env();
        let mut m = manifest("linked");
        m["assets"] = json!({"cursor":"assets/cursor.png"});
        let p = put(&t, "linked", &m);
        let external = put(&outside, "external", &m);
        fs::create_dir(p.join("assets")).unwrap();
        fs::create_dir(external.join("assets")).unwrap();
        png_file(&external.join("assets/cursor.png"));
        if part == "assets" {
            fs::remove_dir(p.join(part)).unwrap();
        }
        if part == "theme.json" {
            fs::remove_file(p.join(part)).unwrap();
        }
        symlink(external.join(part), p.join(part)).unwrap();
        let result = scan(t.path().to_str().unwrap()).unwrap();
        assert!(result.packs.is_empty(), "{part}");
        assert_eq!(result.warnings.len(), 1, "{part}");
    }
    let t = env();
    let outside = env();
    symlink(outside.path(), t.path().join("themes")).unwrap();
    assert!(scan(t.path().to_str().unwrap()).is_err());
    fs::remove_file(t.path().join("deas-install.json")).unwrap();
    symlink(
        outside.path().join("deas-install.json"),
        t.path().join("deas-install.json"),
    )
    .unwrap();
    assert!(scan(t.path().to_str().unwrap()).is_err());
}

#[cfg(unix)]
#[test]
fn symbolic_links_are_rejected() {
    use std::os::unix::fs::symlink;
    let t = env();
    let outside = env();
    let p = put(&outside, "external", &manifest("external"));
    fs::create_dir_all(t.path().join("themes")).unwrap();
    symlink(p, t.path().join("themes/link")).unwrap();
    assert!(scan(t.path().to_str().unwrap()).unwrap().packs.is_empty());
}
