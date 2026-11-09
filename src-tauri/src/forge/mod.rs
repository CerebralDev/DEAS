//! Prompt Forge for the image models (Illustrious, Anima, Krea2, MOSS), run
//! natively instead of through the node pack's Prompt Writer: character canon
//! from the local tables, the tag-search and guidance tools in the writing
//! loop, and the mechanical clean-up after it. Ported from PromptForge; the
//! MiniMax H3 Director Forge stays on the node pack.

pub mod appearance;
pub mod assistant;
pub mod canon;
pub mod character;
pub mod chat;
pub mod clean;
pub mod data;
pub mod engine;
pub mod gpu_swap;
pub mod message;
pub mod pack;
pub mod picker;
pub mod randomizer;
pub mod refs;
pub mod search;
pub mod segments;
pub mod tags;
pub mod tools;

pub use engine::{native_forge_revise, native_forge_targets, native_forge_write};

#[cfg(test)]
mod bundle_tests {
    #[test]
    fn fresh_environment_has_complete_forge_data_without_writes() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("forge-data");
        let data = super::engine::Data::load(&dir).unwrap();
        for model in ["illustrious", "anima", "krea2", "moss", "qwen_image"] {
            assert!(data.pack.models.contains_key(model), "{model}");
        }
        // DEAS-owned writer: base shared rules + its own guide, base controls, no negative.
        let qwen = &data.pack.models["qwen_image"];
        assert!(qwen.system.starts_with("# PromptForge — Shared Rules"));
        assert!(qwen
            .system
            .contains("# Qwen Image 2.1 — Observer description"));
        assert!(!qwen.system.contains("# Krea2"));
        assert_eq!(qwen.str("base"), Some("krea2"));
        assert_eq!(qwen.optional(), ["What changed"]);
        assert_eq!(qwen.expected(None), ["Enhanced prompt", "What changed"]);
        assert!(qwen.entry["styles"]["Photorealistic"].is_array());
        assert!(qwen.entry["style_placement"].is_null());
        assert!(qwen.segment_blocks["What changed"].contains("Qwen Image 2.1"));
        assert!(data
            .pack
            .shared
            .chat
            .as_ref()
            .is_some_and(|s| !s.is_empty()));
        assert!(data.tags.is_some());
        assert!(data.canon.is_some());
        assert!(super::randomizer::Randomizer::load(&dir).is_ok());
        let hits = super::picker::search(&dir, "illustrious", "frier").unwrap();
        assert!(hits.iter().any(|hit| hit.tag == "frieren"));
        assert!(super::picker::traits(&dir, "illustrious", "frieren")
            .unwrap()
            .iter()
            .any(|t| t == "pointy ears"));
        assert!(
            !dir.exists(),
            "discovery must not write into an environment"
        );
    }

    #[test]
    fn bundled_files_match_pinned_manifest() {
        use sha2::{Digest, Sha256};
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../forge-data/MANIFEST.json")).unwrap();
        for (name, entry) in manifest["files"].as_object().unwrap() {
            // The manifest pins the embedded (compressed) payload, not the decoded bytes.
            let bytes = super::data::bundled(name).unwrap();
            assert_eq!(
                format!("{:x}", Sha256::digest(bytes)),
                entry["sha256"].as_str().unwrap()
            );
        }
    }

    #[test]
    fn invalid_local_override_is_not_replaced_or_modified() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("forge-pack.json");
        std::fs::write(&path, "invalid fixture").unwrap();
        assert!(super::pack::Pack::load(&path).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "invalid fixture");
        let path = root.path().join("tags.sqlite");
        std::fs::write(&path, "invalid database").unwrap();
        assert!(super::tags::TagStore::open(&path).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "invalid database");
    }
}
