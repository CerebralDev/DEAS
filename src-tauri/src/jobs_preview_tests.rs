use super::*;
const BUDGET: usize = 4 * 1024 * 1024;
fn data(bytes: &[u8], mime: &str) -> Value {
    json!({"node_id":"unique-node","w":32,"h":32,"fps":8,"mime":mime,"image":base64::engine::general_purpose::STANDARD.encode(bytes)})
}

#[test]
fn decoded_tiny_vae_fixtures_keep_changing_frames_and_timing() {
    for (bytes, count) in [
        (include_bytes!("../test-data/preview/h3.webp").as_slice(), 3),
        (
            include_bytes!("../test-data/preview/ltx.webp").as_slice(),
            9,
        ),
    ] {
        let preview = parse(&data(bytes, "image/webp"), "job", 1, BUDGET).unwrap();
        let frames = preview.data["frames"].as_array().unwrap();
        assert_eq!(frames.len(), count);
        assert!(frames.iter().all(|f| f["durationMs"] == 125));
        assert_ne!(frames[0]["src"], frames[1]["src"]);
        assert!(preview.video.is_none());
    }
}
#[test]
fn mp4_is_immutable_leased_and_removed_after_last_reader() {
    let bytes = include_bytes!("../test-data/preview/preview.mp4");
    let preview = parse(&data(bytes, "video/mp4"), "job", 3, BUDGET).unwrap();
    assert_eq!(preview.data["video"]["previewRevision"], 3);
    let lease = preview.video.clone().unwrap();
    let path = lease.path().to_owned();
    drop(preview);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    drop(lease);
    assert!(!path.exists());
}
#[test]
fn rejects_truncation_base64_budget_dimensions_and_timing() {
    for (bytes, mime) in [
        (
            include_bytes!("../test-data/preview/h3.webp").as_slice(),
            "image/webp",
        ),
        (
            include_bytes!("../test-data/preview/preview.mp4").as_slice(),
            "video/mp4",
        ),
    ] {
        for n in 0..bytes.len() {
            assert!(
                parse(&data(&bytes[..n], mime), "job", 1, BUDGET).is_none(),
                "prefix {n} {mime}"
            );
        }
        let valid = data(bytes, mime);
        assert!(parse(&valid, "job", 1, 128).is_none());
        for (key, value) in [
            ("w", json!(200000)),
            ("fps", json!(0)),
            ("fps", json!(100000)),
            ("image", json!("%%%")),
            ("mime", json!("text/html")),
        ] {
            let mut malformed = valid.clone();
            malformed[key] = value;
            assert!(parse(&malformed, "job", 1, BUDGET).is_none());
        }
    }
}
#[test]
fn rejects_hidden_oversized_h264_sps_and_excessive_animation_duration() {
    let mut mp4 = include_bytes!("../test-data/preview/oversized-sps.mp4").to_vec();
    let pos = mp4.windows(4).rposition(|b| b == b"avc1").unwrap();
    mp4[pos + 4 + 24..pos + 4 + 28].copy_from_slice(&[0, 32, 0, 32]);
    assert!(parse(&data(&mp4, "video/mp4"), "job", 1, BUDGET).is_none());
    let mut webp = include_bytes!("../test-data/preview/h3.webp").to_vec();
    let pos = webp.windows(4).position(|b| b == b"ANMF").unwrap();
    webp[pos + 8 + 12..pos + 8 + 15].copy_from_slice(&[255, 255, 255]);
    assert!(parse(&data(&webp, "image/webp"), "job", 1, BUDGET).is_none());
}
#[test]
fn jpeg_initial_payload_without_mime_is_still_bounded() {
    let bytes = include_bytes!("../test-data/preview/preview.jpg");
    let mut payload = data(bytes, "image/jpeg");
    payload.as_object_mut().unwrap().remove("mime");
    assert!(jpeg(&payload, BUDGET)
        .unwrap()
        .starts_with("data:image/jpeg;base64,"));
    assert!(jpeg(&payload, 128).is_none());
    payload["mime"] = json!("video/mp4");
    assert!(jpeg(&payload, BUDGET).is_none());
}
#[test]
fn graph_preview_ids_are_plan_unique_and_all_links_are_rewritten() {
    let source = json!({"17":{"class_type":"ModelPreviewOverrideKJ","inputs":{"model":["1",0]}},"18":{"class_type":"ModelPreviewOverrideKJ","inputs":{"model":["17",0]}},"2":{"class_type":"Sampler","inputs":{"model":["18",0],"seed":7}},"1":{"class_type":"Loader","inputs":{}}});
    let mut first = source.clone();
    let mut second = source.clone();
    bind_nodes(&mut first, "first");
    bind_nodes(&mut second, "second");
    assert_ne!(nodes(&first), nodes(&second));
    assert_eq!(
        first["2"]["inputs"]["model"],
        json!(["deas-preview-first-18", 0])
    );
    assert_eq!(
        first["deas-preview-first-18"]["inputs"]["model"],
        json!(["deas-preview-first-17", 0])
    );
    assert_eq!(first["2"]["inputs"]["seed"], 7);
    assert!(first.get("17").is_none());
}
#[test]
fn webp_coded_dimensions_are_checked_before_decoder_allocation() {
    let mut bytes = include_bytes!("../test-data/preview/h3.webp").to_vec();
    let pos = bytes.windows(4).position(|b| b == b"ANMF").unwrap();
    bytes[pos + 8 + 6..pos + 8 + 9].copy_from_slice(&[255, 255, 255]);
    assert!(parse(&data(&bytes, "image/webp"), "job", 1, BUDGET).is_none());
}
