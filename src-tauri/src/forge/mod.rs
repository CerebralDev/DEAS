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
mod data;
pub mod engine;
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
        for model in ["illustrious", "anima", "krea2", "moss"] {
            assert!(data.pack.models.contains_key(model), "{model}");
        }
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
