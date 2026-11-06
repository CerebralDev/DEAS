//! Header-first ACE-Step architecture and conservative training-variant identity.
use super::{FamilyEvidence, TensorShapes};

/// Signatures: ComfyUI b5a43215 comfy/model_detection.py (genre_embedder / lyric_encoder).
/// Shapes corroborated with HTTP-range-only Comfy-Org headers:
/// ACE-Step@ea4f3d349e84d5c098cc8e8b904a30fde59fc99a/all_in_one/ace_step_v1_3.5b.safetensors;
/// ACE-Step-1.5@371bb1680f8c88d8c1083f6ffb6330d3d55bff76/split_files/diffusion_models/
/// acestep_v1.5_{base,turbo}.safetensors and acestep_v1.5_xl_{sft,turbo}_bf16.safetensors.
/// Published metadata is only format=pt (XL Turbo: absent), not a distillation marker.
pub(super) fn classify(
    keys: &str,
    arch: &str,
    shapes: &TensorShapes,
    file: &str,
) -> Option<(String, FamilyEvidence)> {
    use FamilyEvidence::{Conflict, Filename, Header, Unresolved};
    // A metadata architecture is an explicit declaration, not a filename convention. Exact
    // registered IDs only; synthetic metadata tests do not imply a published producer uses it.
    let metadata = super::families()
        .iter()
        .find(|f| f.id.starts_with("ace-step") && f.id == arch)
        .map(|f| f.id.as_str());
    if arch.contains("lora")
        || keys.lines().any(|k| {
            ["lora.up", "lora.down", "lora.A", "lora.B"]
                .iter()
                .any(|s| k.contains(s))
        })
    {
        return None;
    }
    let shape = |key: &str, dims: &[u64]| {
        shapes.iter().any(|(k, s)| {
            (k == key || k.strip_prefix("model.diffusion.model.") == Some(key)) && s == dims
        })
    };
    let v1 = shape("genre.embedder.weight", &[2560, 768]);
    let encoder = shape(
        "encoder.lyric.encoder.layers.0.input.layernorm.weight",
        &[2048],
    );
    let regular = encoder
        && shape("decoder.layers.0.self.attn.norm.weight", &[2048])
        && shape("decoder.layers.0.mlp.gate.proj.weight", &[6144, 2048])
        && shape("decoder.layers.0.self.attn.q.proj.weight", &[2048, 2048]);
    let xl = encoder
        && shape("decoder.layers.0.self.attn.norm.weight", &[2560])
        && shape("decoder.layers.0.mlp.gate.proj.weight", &[9728, 2560])
        && shape("decoder.layers.0.self.attn.q.proj.weight", &[4096, 2560]);
    let structural = if v1 {
        Some("ace-step-1-0")
    } else if regular {
        Some("ace-step-1-5")
    } else if xl {
        Some("ace-step-1-5-xl")
    } else {
        None
    };
    // Observed on a local AIO export: this denotes 1.5, not its width or training variant.
    let metadata = if arch == "ace-step-1_5" {
        match structural {
            Some(id) if id.starts_with("ace-step-1-5") => Some(id),
            Some(_) => return Some(("ace-step".into(), Conflict)),
            None => return Some(("ace-step-1-5".into(), Unresolved)),
        }
    } else {
        metadata
    };
    let legacy = keys.contains("lyric.encoder.layers") || keys.contains("genre.embedder.weight");
    if metadata.is_none() && structural.is_none() && !legacy {
        return None;
    }
    let identity = |id: &str| {
        if id.starts_with("ace-step-1-0") {
            1
        } else if id.starts_with("ace-step-1-5-xl") {
            3
        } else if id.starts_with("ace-step-1-5") {
            2
        } else {
            0
        }
    };
    if v1 && (regular || xl)
        || metadata
            .zip(structural)
            .is_some_and(|(m, s)| identity(m) != 0 && identity(m) != identity(s))
        || (structural.is_some() && !arch.is_empty() && metadata.is_none())
    {
        return Some(("ace-step".into(), Conflict));
    }
    let id = metadata
        .filter(|m| identity(m) != 0)
        .or(structural)
        .unwrap_or("ace-step");
    let tokens: Vec<_> = file
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    let ace_name = tokens.contains(&"acestep") || tokens.windows(2).any(|w| w == ["ace", "step"]);
    let version15 = tokens
        .windows(2)
        .any(|w| ["v1", "1"].contains(&w[0]) && w[1] == "5");
    let version10 = tokens
        .windows(2)
        .any(|w| ["v1", "1"].contains(&w[0]) && w[1] == "0")
        || (!version15 && tokens.contains(&"v1"));
    let named_xl = tokens.contains(&"xl");
    if ace_name
        && ((version15 && identity(id) == 1)
            || (version10 && identity(id) >= 2)
            || (version15 && identity(id) >= 2 && named_xl != (identity(id) == 3)))
    {
        return Some(("ace-step".into(), Conflict));
    }
    let exact_variant = ["-base", "-sft", "-turbo"].iter().any(|s| id.ends_with(s));
    if exact_variant || id == "ace-step-1-0" {
        return Some((id.into(), Header));
    }
    if id == "ace-step" {
        return Some((id.into(), Unresolved));
    }
    let variants: Vec<_> = ["base", "sft", "turbo"]
        .into_iter()
        .filter(|v| tokens.contains(v))
        .collect();
    let unsupported = tokens
        .iter()
        .any(|t| t.starts_with("shift") || *t == "continuous");
    if ace_name && version15 && variants.len() == 1 && !unsupported {
        return Some((format!("{id}-{}", variants[0]), Filename));
    }
    Some((id.into(), Unresolved))
}

#[cfg(test)]
mod tests {
    use super::super::{detect_with_evidence, validate_override};
    use super::*;
    use serde_json::{json, Value};

    // Reduced real header shapes from the pinned Comfy-Org range reads above. Their metadata
    // contains no architecture/variant. Base and Turbo have the SAME regular shape signature.
    pub(crate) fn published_header(xl: bool) -> Value {
        let (hidden, intermediate, q) = if xl {
            (2560, 9728, 4096)
        } else {
            (2048, 6144, 2048)
        };
        json!({
            "__metadata__": {"format": "pt"},
            "encoder.lyric_encoder.layers.0.input_layernorm.weight": {"shape": [2048]},
            "decoder.layers.0.self_attn_norm.weight": {"shape": [hidden]},
            "decoder.layers.0.mlp.gate_proj.weight": {"shape": [intermediate, hidden]},
            "decoder.layers.0.self_attn.q_proj.weight": {"shape": [q, hidden]}
        })
    }

    fn detect(head: Value, file: &str, category: &str) -> (Option<String>, Option<FamilyEvidence>) {
        let dir = tempfile::tempdir().unwrap();
        let text = head.to_string();
        let mut bytes = (text.len() as u64).to_le_bytes().to_vec();
        bytes.extend(text.as_bytes());
        let path = dir.path().join(file);
        std::fs::write(&path, bytes).unwrap();
        detect_with_evidence(category, &path, None)
    }

    fn check(head: Value, file: &str, expected: &str, evidence: FamilyEvidence) {
        assert_eq!(
            detect(head, file, "diffusion_models"),
            (Some(expected.into()), Some(evidence)),
            "{file}"
        );
    }

    #[test]
    fn ace_exported_modelspect_alias_respects_shapes_in_unet() {
        use FamilyEvidence::{Conflict, Filename, Unresolved};
        for xl in [false, true] {
            let mut head = published_header(xl);
            head["__metadata__"]["modelspec.architecture"] = json!("ace-step-1_5");
            let (name, expected) = if xl {
                (
                    "ace_step_1.5_xl_turbo_aio.safetensors",
                    "ace-step-1-5-xl-turbo",
                )
            } else {
                ("ace_step_1.5_turbo_aio.safetensors", "ace-step-1-5-turbo")
            };
            assert_eq!(
                detect(head.clone(), name, "unet"),
                (Some(expected.into()), Some(Filename))
            );
            assert_eq!(
                detect(head, "renamed.safetensors", "unet"),
                (
                    Some(
                        if xl {
                            "ace-step-1-5-xl"
                        } else {
                            "ace-step-1-5"
                        }
                        .into()
                    ),
                    Some(Unresolved)
                )
            );
        }
        let original = json!({"__metadata__":{"modelspec.architecture":"ace-step-1_5"},"genre_embedder.weight":{"shape":[2560,768]}});
        assert_eq!(
            detect(original, "ace_step_1.5_turbo_aio.safetensors", "unet"),
            (Some("ace-step".into()), Some(Conflict))
        );
        let incomplete = json!({"__metadata__":{"modelspec.architecture":"ace-step-1_5"}});
        assert_eq!(
            detect(incomplete, "ace_step_1.5_turbo_aio.safetensors", "unet"),
            (Some("ace-step-1-5".into()), Some(Unresolved))
        );
    }

    #[test]
    #[ignore = "Requires an explicitly selected local ACE model; header-only, no inference"]
    fn ace_live_unet_header_identity() {
        let file = std::env::var_os("DEAS_ACE_LOCAL_MODEL").expect("DEAS_ACE_LOCAL_MODEL");
        let path = std::path::Path::new(&file);
        let detected = detect_with_evidence("unet", path, None);
        println!("ACE header identity: {:?}", detected);
        assert_eq!(
            detected,
            (
                Some("ace-step-1-5-turbo".into()),
                Some(FamilyEvidence::Filename)
            )
        );
    }

    #[test]
    fn ace_published_headers_and_rename_do_not_invent_distillation() {
        use FamilyEvidence::{Filename, Unresolved};
        for xl in [false, true] {
            let prefix = if xl {
                "ace-step-1-5-xl"
            } else {
                "ace-step-1-5"
            };
            let name_prefix = if xl {
                "acestep_v1.5_xl"
            } else {
                "acestep_v1.5"
            };
            for variant in ["base", "sft", "turbo"] {
                check(
                    published_header(xl),
                    &format!("{name_prefix}_{variant}_bf16.safetensors"),
                    &format!("{prefix}-{variant}"),
                    Filename,
                );
            }
            check(
                published_header(xl),
                "renamed.safetensors",
                prefix,
                Unresolved,
            );
            check(
                published_header(xl),
                "turbo.safetensors",
                prefix,
                Unresolved,
            );
        }
    }

    #[test]
    fn ace_unknown_partial_and_misleading_files_stay_safe() {
        use FamilyEvidence::Unresolved;
        assert_eq!(
            detect(
                json!({"random.weight": {"shape": [2048]}}),
                "acestep_v1.5_turbo.safetensors",
                "checkpoints"
            ),
            (None, None)
        );
        check(
            json!({"encoder.lyric_encoder.layers.0.input_layernorm.weight": {"shape": [2048]}}),
            "acestep_v1.5_turbo.safetensors",
            "ace-step",
            Unresolved,
        );
        let mut wrong = published_header(false);
        wrong["decoder.layers.0.self_attn_norm.weight"]["shape"] = json!([17]);
        check(
            wrong,
            "acestep_v1.5_turbo.safetensors",
            "ace-step",
            Unresolved,
        );
        assert_eq!(
            detect(json!({}), "acestep_v1.5_turbo.gguf", "diffusion_models"),
            (None, None)
        );
    }

    #[test]
    fn ace_ambiguous_and_unsupported_distillations_never_become_turbo() {
        for suffix in [
            "base_sft",
            "sft_turbo",
            "turbo_base",
            "turbo-shift1",
            "turbo-shift3",
            "turbo-continuous",
            "turbo-shift-1",
            "turboshift1",
            "myturbo",
            "turbocharged",
        ] {
            check(
                published_header(false),
                &format!("acestep_v1.5_{suffix}.safetensors"),
                "ace-step-1-5",
                FamilyEvidence::Unresolved,
            );
        }
    }

    #[test]
    fn ace_version_and_xl_conflicts_do_not_guess() {
        for (xl, name) in [
            (false, "acestep_v1.0_turbo.safetensors"),
            (false, "acestep_v1.5_xl_turbo.safetensors"),
            (true, "acestep_v1.5_turbo.safetensors"),
        ] {
            check(
                published_header(xl),
                name,
                "ace-step",
                FamilyEvidence::Conflict,
            );
        }
        let original =
            json!({"model.diffusion_model.genre_embedder.weight": {"shape": [2560, 768]}});
        check(
            original.clone(),
            "renamed.safetensors",
            "ace-step-1-0",
            FamilyEvidence::Header,
        );
        check(
            original,
            "acestep_v1.5_base.safetensors",
            "ace-step",
            FamilyEvidence::Conflict,
        );
        let mut head = published_header(false);
        head["__metadata__"]["modelspec.architecture"] = json!("ace-step-1-5-xl-sft");
        check(
            head,
            "renamed.safetensors",
            "ace-step",
            FamilyEvidence::Conflict,
        );
    }

    #[test]
    fn ace_synthetic_explicit_architecture_overrides_filename_variant_not_size() {
        // Synthetic modelspec declarations: not metadata observed in the published checkpoints.
        let mut head = published_header(false);
        head["__metadata__"]["modelspec.architecture"] = json!("ace-step-1-5-base");
        check(
            head.clone(),
            "acestep_v1.5_turbo.safetensors",
            "ace-step-1-5-base",
            FamilyEvidence::Header,
        );
        check(
            head.clone(),
            "acestep_v1.5_turbo-shift3.safetensors",
            "ace-step-1-5-base",
            FamilyEvidence::Header,
        );
        check(
            head.clone(),
            "acestep_v1.5_xl_base.safetensors",
            "ace-step",
            FamilyEvidence::Conflict,
        );
        head["__metadata__"]["modelspec.architecture"] = json!("stable-diffusion-xl");
        check(
            head,
            "renamed.safetensors",
            "ace-step",
            FamilyEvidence::Conflict,
        );
        check(
            json!({"__metadata__": {"modelspec.architecture": "ace-step-1-5-sft"}}),
            "renamed.safetensors",
            "ace-step-1-5-sft",
            FamilyEvidence::Header,
        );
    }

    #[test]
    fn ace_checkpoint_prefix_and_evidence_serialization() {
        let mut aio = serde_json::Map::new();
        for (key, value) in published_header(false).as_object().unwrap() {
            aio.insert(
                if key == "__metadata__" {
                    key.clone()
                } else {
                    format!("model.diffusion_model.{key}")
                },
                value.clone(),
            );
        }
        assert_eq!(
            detect(Value::Object(aio), "renamed.safetensors", "checkpoints"),
            (
                Some("ace-step-1-5".into()),
                Some(FamilyEvidence::Unresolved)
            )
        );
        for (value, text) in [
            (FamilyEvidence::Header, "header"),
            (FamilyEvidence::Filename, "filename"),
            (FamilyEvidence::Unresolved, "unresolved"),
            (FamilyEvidence::Conflict, "conflict"),
        ] {
            assert_eq!(serde_json::to_value(value).unwrap(), text);
        }
        check(
            published_header(false),
            "ace-step-v1.0-v1.5-turbo.safetensors",
            "ace-step",
            FamilyEvidence::Conflict,
        );
    }

    #[test]
    fn ace_categories_adapters_and_declarations_remain_bounded() {
        for category in ["checkpoints", "diffusion_models", "unet"] {
            assert_eq!(
                detect(published_header(false), "renamed.safetensors", category)
                    .0
                    .as_deref(),
                Some("ace-step-1-5")
            );
        }
        for category in ["loras", "vae", "text_encoders", "clip", "audio_encoders"] {
            assert_eq!(
                detect(
                    published_header(false),
                    "acestep_v1.5_turbo.safetensors",
                    category
                ),
                (None, None)
            );
        }
        let mut adapter = published_header(false);
        adapter["decoder.layers.0.self_attn.q_proj.lora_A.weight"] = json!({"shape": [8,2048]});
        assert_eq!(
            detect(adapter, "acestep_v1.5_turbo.safetensors", "checkpoints"),
            (None, None)
        );
        let registry: Value =
            serde_json::from_str(include_str!("../../../src/families/builtin.json")).unwrap();
        for family in registry
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["id"].as_str().unwrap().starts_with("ace-step"))
        {
            let id = family["id"].as_str().unwrap();
            assert_eq!(
                family["pipelines"],
                if family.get("music").is_some() {
                    json!(["t2m"])
                } else {
                    json!([])
                }
            );
            for category in ["checkpoints", "diffusion_models", "unet"] {
                assert!(validate_override(category, id).is_ok());
            }
            for category in ["loras", "vae", "text_encoders"] {
                assert!(validate_override(category, id).is_err());
            }
        }
    }
}
