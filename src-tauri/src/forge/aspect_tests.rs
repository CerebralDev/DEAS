use super::super::message::{Canvas, DrawnItem, StudioLayout};
use super::*;

fn fixture() -> Data {
    let model = |layout: bool| {
        json!({
            "entry":{
                "modes":["Natural language","Layout JSON"],"default_mode":"Natural language",
                "segments":["Positive prompt"],
                "segments_by_mode":{"Natural language":["Positive prompt"],"Layout JSON":["Positive prompt","Layout JSON"]},
                "aspect_ratios":["N/A","1:1 (Square)","16:9 (Landscape)"],
                "layout_canvas":{"N/A":[1024,1024],"1:1 (Square)":[1024,1024],"16:9 (Landscape)":[1536,864]}
            },
            "system":"Fixture writer.","layout_rewrite":if layout { Some("Rewrite one section.") } else { None }
        })
    };
    let pack = serde_json::from_value(json!({
        "source":{},"config":{},
        "defaults":{"image_reference_roles":{"scene":{"label":"Scene","rule":"take the scene"}},"default_image_reference_role":"scene","max_image_references":6},
        "shared":{"second_pass":"Revise the draft.","reference_blueprint":"Look before writing."},
        "models":{"illustrious":model(false),"anima":model(false),"krea2":model(false),"moss":model(true)}
    })).unwrap();
    Data {
        pack,
        tags: None,
        canon: None,
    }
}

fn request(target: &str, aspect: &str, canvas: Option<Canvas>) -> WriteRequest {
    WriteRequest {
        target: target.into(),
        brief: "A fixture landscape.".into(),
        model: "fixture-model".into(),
        mode: None,
        segments_on: None,
        controls: Controls {
            aspect_ratio: Some(aspect.into()),
            canvas,
            ..Controls::default()
        },
    }
}

fn layout(width: f64, height: f64) -> StudioLayout {
    StudioLayout {
        width,
        height,
        items: vec![DrawnItem {
            name: "Sign".into(),
            label: "text".into(),
            caption: "a sign".into(),
            text: "KEEP".into(),
            bbox: vec![-200.0, -300.0, 200.0, 300.0],
        }],
    }
}

#[test]
fn forge_canvas_geometry_uses_route_specific_bounds_and_strict_shape() {
    for (width, height) in [
        (64.0, 64.0),
        (72.0, 80.0),
        (1000.0, 1000.0),
        (8192.0, 2048.0),
        (1216.0, 832.0),
    ] {
        Canvas { width, height }.validate(false).unwrap();
    }
    for (width, height) in [
        (63.0, 64.0),
        (0.0, 512.0),
        (512.5, 512.0),
        (8193.0, 512.0),
        (8192.0, 4096.0),
        (f64::NAN, 512.0),
        (f64::INFINITY, 512.0),
    ] {
        assert!(Canvas { width, height }.validate(false).is_err());
    }
    for (width, height) in [
        (64.0, 128.0),
        (136.0, 128.0),
        (1000.0, 1000.0),
        (8192.0, 4096.0),
    ] {
        assert!(Canvas { width, height }.validate(true).is_err());
    }
    Canvas {
        width: 128.0,
        height: 128.0,
    }
    .validate(true)
    .unwrap();
    Canvas {
        width: 8192.0,
        height: 2048.0,
    }
    .validate(true)
    .unwrap();
    for value in [
        json!({"width":512}),
        json!({"width":"512","height":512}),
        json!({"width":512,"height":512,"token":"secret"}),
    ] {
        assert!(serde_json::from_value::<Canvas>(value).is_err());
    }
}

#[test]
fn forge_new_numeric_auto_custom_canvases_reach_all_native_writers() {
    let data = fixture();
    for target in ["illustrious", "anima", "krea2", "moss"] {
        for aspect in ["3:2", "auto", "custom"] {
            let req = request(
                target,
                aspect,
                Some(Canvas {
                    width: 1216.0,
                    height: 832.0,
                }),
            );
            let prepared = prepare(&data, &req, &RefState::default()).unwrap();
            assert!(
                prepared.user.contains("Canvas: 1216×832 pixels"),
                "{target}/{aspect}"
            );
            assert_eq!(prepared.controls.canvas, req.controls.canvas);
        }
    }
    // Bucket dimensions need not equal the mathematical ratio after snapping.
    validate_request(
        &data,
        &request(
            "anima",
            "3:2",
            Some(Canvas {
                width: 1216.0,
                height: 832.0,
            }),
        ),
    )
    .unwrap();
    // A non-square ratio may collapse to square at the minimum grid.
    validate_request(
        &data,
        &request(
            "anima",
            "1:2",
            Some(Canvas {
                width: 64.0,
                height: 64.0,
            }),
        ),
    )
    .unwrap();
    // Studio's common generation grid admits 1000; the MOSS layout route keeps its /16 contract.
    let mut common = request(
        "illustrious",
        "custom",
        Some(Canvas {
            width: 1000.0,
            height: 1000.0,
        }),
    );
    validate_request(&data, &common).unwrap();
    common.target = "moss".into();
    common.mode = Some("Layout JSON".into());
    assert!(validate_request(&data, &common).is_err());
    for (aspect, canvas) in [
        (
            "1:1",
            Canvas {
                width: 1216.0,
                height: 832.0,
            },
        ),
        (
            "16:9",
            Canvas {
                width: 832.0,
                height: 1216.0,
            },
        ),
        (
            "9:16",
            Canvas {
                width: 1216.0,
                height: 832.0,
            },
        ),
        (
            "N/A",
            Canvas {
                width: 512.0,
                height: 512.0,
            },
        ),
    ] {
        assert!(validate_request(&data, &request("anima", aspect, Some(canvas))).is_err());
    }
}

#[test]
fn forge_unset_and_exact_legacy_aspects_stay_compatible_without_unknown_fallback() {
    let data = fixture();
    for aspect in ["", "N/A", "16:9 (Landscape)"] {
        validate_request(&data, &request("illustrious", aspect, None)).unwrap();
    }
    let mut req = request("moss", "N/A", None);
    req.mode = Some("Layout JSON".into());
    req.controls.layout = Some(layout(1536.0, 1024.0));
    validate_request(&data, &req).unwrap();
    assert_eq!(
        layout_size(
            &data.pack.models["moss"],
            req.mode.as_deref(),
            &req.controls
        ),
        Some((1536.0, 1024.0))
    );
    req.controls.layout = None;
    req.controls.aspect_ratio = Some("16:9 (Landscape)".into());
    validate_request(&data, &req).unwrap();
    assert_eq!(
        layout_size(
            &data.pack.models["moss"],
            req.mode.as_deref(),
            &req.controls
        ),
        Some((1536.0, 864.0))
    );
    for aspect in [
        "stale/secret-token",
        "auto",
        "custom",
        "21:9",
        "0:1",
        "NaN:1",
        "1:0",
    ] {
        req.controls.aspect_ratio = Some(aspect.into());
        assert!(validate_request(&data, &req).is_err(), "{aspect}");
        assert!(layout_size(
            &data.pack.models["moss"],
            req.mode.as_deref(),
            &req.controls
        )
        .is_none());
    }
}

#[test]
fn forge_layout_consumes_one_canvas_and_preserves_drawn_geometry() {
    let data = fixture();
    let mut req = request(
        "moss",
        "3:2",
        Some(Canvas {
            width: 1216.0,
            height: 832.0,
        }),
    );
    req.mode = Some("Layout JSON".into());
    req.controls.layout = Some(layout(1216.0, 832.0));
    let prepared = prepare(&data, &req, &RefState::default()).unwrap();
    assert!(prepared
        .user
        .contains("Canvas: 1216×832 — measure every Layout JSON bbox"));
    let mut body = json!({"summary":{"caption":"scene"},"items":[{"label":"text","text":"WRONG","caption":"written caption","bbox":[0,0,1,1]}]}).to_string();
    assert!(tidy_layout(
        prepared.model,
        prepared.mode.as_deref(),
        &prepared.controls,
        &mut body
    )
    .is_none());
    let result: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(result["items"][0]["bbox"], json!([-200, -300, 200, 300]));
    assert_eq!(result["items"][0]["text"], "KEEP");
    assert_eq!(result["items"][0]["caption"], "written caption");
    req.controls.layout.as_mut().unwrap().width = 1232.0;
    assert!(validate_request(&data, &req)
        .unwrap_err()
        .contains("layout and effective canvas disagree"));
    req.controls.canvas = None;
    req.controls.aspect_ratio = Some("N/A".into());
    req.controls.layout.as_mut().unwrap().width = 136.0;
    assert!(validate_request(&data, &req).is_err());
}

#[tokio::test]
async fn forge_write_revise_and_layout_reject_geometry_before_reference_or_llm_network() {
    let data = fixture();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let config = LlmConfig {
        provider: Provider::Openai,
        base_url: format!("http://{}", listener.local_addr().unwrap()),
        api_key: String::new(),
    };
    let mut invalid = vec![
        request("illustrious", "unknown-aspect", None),
        request("anima", "auto", None),
        request(
            "krea2",
            "custom",
            Some(Canvas {
                width: 8192.0,
                height: 8192.0,
            }),
        ),
        request(
            "moss",
            "16:9",
            Some(Canvas {
                width: 832.0,
                height: 1216.0,
            }),
        ),
    ];
    let mut mismatch = request(
        "moss",
        "3:2",
        Some(Canvas {
            width: 1216.0,
            height: 832.0,
        }),
    );
    mismatch.mode = Some("Layout JSON".into());
    mismatch.controls.layout = Some(layout(1024.0, 1024.0));
    invalid.push(mismatch);
    for mut req in invalid {
        req.controls.image_references = vec![refs::ImageRef {
            role: "scene".into(),
            image: "data:image/png;base64,AA==".into(),
            ..Default::default()
        }];
        let error = tokio::time::timeout(Duration::from_secs(2), write(&data, &config, &req))
            .await
            .expect("invalid geometry reached a reference/LLM request")
            .unwrap_err();
        assert!(error.starts_with(message::CANVAS_ERROR));
        let revision = ReviseRequest {
            write: req,
            segments: vec![Segment {
                label: "Positive prompt".into(),
                body: "a draft".into(),
            }],
            instruction: Some("refine".into()),
            reference_analysis: None,
        };
        let error = tokio::time::timeout(Duration::from_secs(2), revise(&data, &config, &revision))
            .await
            .expect("invalid geometry reached revision inference")
            .unwrap_err();
        assert!(error.starts_with(message::CANVAS_ERROR));
    }
    let rewrite = LayoutRewrite {
        task: "scene".into(),
        width: 136.0,
        height: 128.0,
        layout: json!({"items":[]}),
        description: "a scene".into(),
        element: None,
    };
    let error = tokio::time::timeout(
        Duration::from_secs(2),
        layout_rewrite(&data, &config, "fixture", &rewrite),
    )
    .await
    .expect("invalid layout reached inference")
    .unwrap_err();
    assert!(error.starts_with(message::CANVAS_ERROR));
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
