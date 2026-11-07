//! Core SAM3.1 mask recipe. The explicitly selected checkpoint supplies model + text encoder.
use serde_json::{json, Value};

pub(crate) const NODES: [&str; 2] = ["SAM3_Detect", "PrimitiveBoundingBox"];
pub(crate) fn is_sam3(i: &Value) -> bool {
    i["recipeId"] == "sam3-mask"
}
fn integer(v: &Value, min: f64, max: f64) -> bool {
    v.as_f64()
        .is_some_and(|n| n.is_finite() && n.fract() == 0.0 && n >= min && n <= max)
}
fn categories(text: &str) -> Vec<String> {
    text.replace(['(', ')'], "")
        .split(',')
        .filter_map(|s| {
            let s = s.trim();
            let s = s
                .rsplit_once(':')
                .filter(|(_, n)| {
                    let n = n.trim();
                    !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit() || b == b'.')
                })
                .map_or(s, |(label, _)| label.trim());
            (!s.is_empty()).then(|| s.to_owned())
        })
        .collect()
}
pub(crate) fn compile(i: &Value) -> Result<Value, String> {
    let required = [
        "recipeId",
        "version",
        "checkpoint",
        "inputImage",
        "width",
        "height",
        "outputPrefix",
        "threshold",
    ];
    let optional = ["prompt", "positivePoints", "negativePoints", "box"];
    let o = i.as_object().ok_or("Invalid SAM3 mask intent")?;
    if required.iter().any(|k| !o.contains_key(*k))
        || o.keys()
            .any(|k| !required.contains(&k.as_str()) && !optional.contains(&k.as_str()))
        || !is_sam3(i)
        || i["version"].as_f64() != Some(1.0)
        || !i["checkpoint"]
            .as_str()
            .is_some_and(crate::jobs::comfy_model_name)
        || !i["inputImage"]
            .as_str()
            .is_some_and(crate::jobs::model_name)
        || !integer(&i["width"], 1.0, 8192.0)
        || !integer(&i["height"], 1.0, 8192.0)
        || i["width"].as_f64().unwrap() * i["height"].as_f64().unwrap() > 16_777_216.0
        || !i["outputPrefix"]
            .as_str()
            .is_some_and(crate::jobs::safe_output_prefix)
        || !i["threshold"]
            .as_f64()
            .is_some_and(|n| n.is_finite() && (0.0..=1.0).contains(&n))
    {
        return Err("Invalid SAM3 mask intent".into());
    }
    let (w, h) = (i["width"].as_f64().unwrap(), i["height"].as_f64().unwrap());
    if o.contains_key("prompt")
        && !i["prompt"]
            .as_str()
            .is_some_and(|s| s.len() <= 16384 && !s.contains('\0'))
    {
        return Err("Invalid SAM3 prompt".into());
    }
    for key in ["positivePoints", "negativePoints"] {
        if o.contains_key(key)
            && !i[key].as_array().is_some_and(|ps| {
                ps.iter().all(|p| {
                    p.as_object().is_some_and(|p| p.len() == 2)
                        && integer(&p["x"], 0.0, w - 1.0)
                        && integer(&p["y"], 0.0, h - 1.0)
                })
            })
        {
            return Err("Invalid SAM3 source-pixel points".into());
        }
    }
    let b = &i["box"];
    if o.contains_key("box")
        && !(b.as_object().is_some_and(|b| b.len() == 4)
            && integer(&b["x"], 0.0, w - 1.0)
            && integer(&b["y"], 0.0, h - 1.0)
            && integer(&b["width"], 1.0, w - b["x"].as_f64().unwrap())
            && integer(&b["height"], 1.0, h - b["y"].as_f64().unwrap()))
    {
        return Err("Invalid SAM3 source-pixel box".into());
    }
    let prompts = categories(i["prompt"].as_str().unwrap_or_default());
    let has_points = ["positivePoints", "negativePoints"]
        .iter()
        .any(|key| i[key].as_array().is_some_and(|ps| !ps.is_empty()));
    if prompts.is_empty() && !has_points && !o.contains_key("box") {
        return Err("SAM3 needs text, points or a box".into());
    }
    let mut graph = json!({
        "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":i["checkpoint"]}},
        "2":{"class_type":"LoadImage","inputs":{"image":i["inputImage"]}},
        "4":{"class_type":"SAM3_Detect","inputs":{"model":["1",0],"image":["2",0],"threshold":i["threshold"],"refine_iterations":2,"individual_masks":false}},
        "6":{"class_type":"MaskToImage","inputs":{"mask":["4",0]}},
        "7":{"class_type":"SaveImage","inputs":{"images":["6",0],"filename_prefix":i["outputPrefix"]}}
    });
    if !prompts.is_empty() {
        // Core SAM3Detector has 200 queries; sam3_clip's unsuffixed category defaults to top-1.
        let text = prompts
            .iter()
            .map(|s| format!("{s}:200"))
            .collect::<Vec<_>>()
            .join(", ");
        graph["3"] = json!({"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":text}});
        graph["4"]["inputs"]["conditioning"] = json!(["3", 0]);
    }
    for (field, socket) in [
        ("positivePoints", "positive_coords"),
        ("negativePoints", "negative_coords"),
    ] {
        if let Some(points) = i[field].as_array().filter(|p| !p.is_empty()) {
            graph["4"]["inputs"][socket] =
                json!(serde_json::to_string(points).map_err(|_| "Invalid SAM3 point JSON")?);
        }
    }
    if o.contains_key("box") {
        graph["B"] = json!({"class_type":"PrimitiveBoundingBox","inputs":b});
        graph["4"]["inputs"]["bboxes"] = json!(["B", 0]);
    }
    Ok(graph)
}

pub(crate) fn requirements(i: &Value) -> Vec<(&str, Vec<&str>)> {
    let image = i["inputImage"].as_str().unwrap();
    vec![
        ("checkpoints", vec![i["checkpoint"].as_str().unwrap()]),
        (
            "images",
            if crate::jobs::mask_name(image) && image == image.to_ascii_lowercase() {
                vec![]
            } else {
                vec![image]
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn intent() -> Value {
        json!({"recipeId":"sam3-mask","version":1,"checkpoint":"sam3.1.safetensors","inputImage":"source.png","width":641,"height":479,"outputPrefix":"DEAS/mask","prompt":"eyes","threshold":0.5})
    }
    #[test]
    fn sam3_text_unions_all_detector_queries_and_saves_full_size_rgb() {
        let mut i = intent();
        i["prompt"] = json!("(eyes:1), feet");
        let g = crate::jobs::compile(&i).unwrap();
        assert_eq!(g["3"]["inputs"]["text"], "eyes:200, feet:200");
        assert_eq!(g["4"]["inputs"]["individual_masks"], false);
        assert_eq!(
            g["6"],
            json!({"class_type":"MaskToImage","inputs":{"mask":["4",0]}})
        );
        assert_eq!(g["7"]["inputs"]["images"], json!(["6", 0]));
        assert!(!g.to_string().contains("ImageScale"));
    }
    #[test]
    fn sam3_click_box_and_combined_modes_use_core_socket_contract() {
        let mut i = intent();
        i.as_object_mut().unwrap().remove("prompt");
        i["positivePoints"] = json!([{"x":640,"y":478}]);
        i["negativePoints"] = json!([{"x":0,"y":0}]);
        i["box"] = json!({"x":1,"y":2,"width":640,"height":477});
        let g = compile(&i).unwrap();
        assert_eq!(g["B"]["class_type"], "PrimitiveBoundingBox");
        assert_eq!(g["4"]["inputs"]["bboxes"], json!(["B", 0]));
        assert_eq!(
            g["4"]["inputs"]["positive_coords"],
            r#"[{"x":640,"y":478}]"#
        );
        assert!(g.get("3").is_none());
        i.as_object_mut().unwrap().remove("positivePoints");
        i.as_object_mut().unwrap().remove("negativePoints");
        assert!(compile(&i).is_ok());
        i.as_object_mut().unwrap().remove("box");
        assert!(compile(&i).is_err());
    }
    #[test]
    fn sam3_native_and_pure_ts_validation_and_graphs_match() {
        let mut cases = vec![intent()];
        for patch in [
            json!({"prompt":"(eyes:1), feet"}),
            json!({"positivePoints":[{"x":640,"y":478}],"negativePoints":[{"x":0,"y":0}],"box":{"x":1,"y":2,"width":640,"height":477}}),
            json!({"prompt":""}),
            json!({"width":0}),
            json!({"prompt":null}),
            json!({"threshold":1.01}),
            json!({"positivePoints":[{"x":641,"y":0}]}),
            json!({"box":{"x":1,"y":0,"width":641,"height":1}}),
            json!({"extra":true}),
        ] {
            let mut i = intent();
            i.as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            cases.push(i);
        }
        let mut clicks = cases[2].clone();
        clicks.as_object_mut().unwrap().remove("prompt");
        cases.push(clicks);
        let script = r#"
          import { compileSam3Mask, isSam3Intent } from './src/lib/sam3.ts';
          const allow = {nodes:['CheckpointLoaderSimple','LoadImage','CLIPTextEncode','SAM3_Detect','MaskToImage','SaveImage','PrimitiveBoundingBox'], checkpoints:['sam3.1.safetensors'],images:['source.png'],samplers:[],schedulers:[]};
          console.log(JSON.stringify(JSON.parse(process.argv[1]).map(i => isSam3Intent(i) ? {accepted:true,graph:compileSam3Mask(i,allow).graph} : {accepted:false})));
        "#;
        let output = std::process::Command::new("node")
            .current_dir(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap(),
            )
            .args([
                "--experimental-strip-types",
                "--input-type=module",
                "-e",
                script,
                &serde_json::to_string(&cases).unwrap(),
            ])
            .output()
            .expect("Node is required for compiler parity tests");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
        let expected: Vec<Value> = cases
            .iter()
            .map(|i| match compile(i) {
                Ok(graph) => json!({"accepted":true,"graph":graph}),
                Err(_) => json!({"accepted":false}),
            })
            .collect();
        assert_eq!(actual, json!(expected));
    }

    #[test]
    fn sam3_invalid_intents_fail_closed() {
        for (key, value) in [
            ("width", json!(0)),
            ("height", json!(8193)),
            ("threshold", json!(1.01)),
            ("threshold", Value::Null),
            ("checkpoint", json!("../sam3.1.safetensors")),
            ("inputImage", json!("/etc/passwd")),
            ("inputImage", json!("http://host/image")),
            ("outputPrefix", json!("../mask")),
            ("prompt", json!("\0")),
            ("prompt", json!("(), ,")),
            ("version", json!(2)),
            ("extra", json!(true)),
            ("positivePoints", json!([{"x":641,"y":0}])),
            ("negativePoints", json!([{"x":0.5,"y":0}])),
            ("box", json!({"x":1,"y":0,"width":641,"height":1})),
            ("box", Value::Null),
        ] {
            let mut i = intent();
            i[key] = value;
            assert!(compile(&i).is_err(), "{key}: {i}");
        }
        let mut i = intent();
        i["width"] = json!(8192);
        i["height"] = json!(8192);
        assert!(compile(&i).is_err());
    }
}
