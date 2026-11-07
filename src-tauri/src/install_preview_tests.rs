// Include inside install.rs's existing #[cfg(test)] mod tests.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn preview_probe_requires_producer_only_when_explicitly_planned() {
    let tmp = scratch();
    prepare_dirs(tmp.path()).unwrap();
    std::fs::write(tmp.path().join("main.py"), "def start_comfyui(): pass\n").unwrap();
    std::fs::write(tmp.path().join("nodes.py"), "NODE_CLASS_MAPPINGS = {}\n").unwrap();
    let inner = Arc::new(Mutex::new(Inner::default()));
    let cancel = Arc::new(AtomicBool::new(false));
    for selected in ["legacy", "preview"] {
        let result = run(
            Path::new("/usr/bin/python3"),
            &["-c", COMFY_PROBE, "cpu", "core", "legacy", selected],
            tmp.path(),
            tmp.path(),
            &inner,
            &cancel,
        )
        .await;
        if selected == "preview" {
            assert!(result.unwrap_err().contains("ModelPreviewOverrideKJ"));
        } else {
            assert!(result.is_ok());
        }
    }
    std::fs::write(
        tmp.path().join("nodes.py"),
        "NODE_CLASS_MAPPINGS = {'ModelPreviewOverrideKJ': object}\n",
    )
    .unwrap();
    let result = run(
        Path::new("/usr/bin/python3"),
        &["-c", COMFY_PROBE, "cpu", "core", "legacy", "preview"],
        tmp.path(),
        tmp.path(),
        &inner,
        &cancel,
    )
    .await
    .unwrap();
    let result: serde_json::Value = serde_json::from_str(result.trim()).unwrap();
    assert_eq!(
        result["selectedNodes"],
        serde_json::json!(["ModelPreviewOverrideKJ"])
    );
    assert_eq!(result["generationReady"], false);
}

#[test]
fn preview_plan_declares_pin_and_consent_without_writing_files() {
    let tmp = scratch();
    let plan = Engine::default().plan(request(tmp.path())).unwrap();
    assert_eq!(plan.preview_ref.as_deref(), Some(KJNODES_REVISION));
    let steps: Vec<_> = plan.steps.iter().filter(|s| s.id == "preview").collect();
    assert_eq!(steps.len(), 1);
    assert!(steps[0].detail.contains(KJNODES_REVISION));
    assert!(steps[0].detail.contains("ComfyUI-KJNodes"));
    assert!(steps[0].detail.contains("no weights"));
    assert!(!Path::new(&plan.request.storage_path).exists());
}

#[test]
fn preview_legacy_open_is_read_only_and_upgrade_is_explicit_and_immutable() {
    let tmp = scratch();
    let mut plan = Engine::default().plan(request(tmp.path())).unwrap();
    plan.preview_ref = None;
    plan.gguf_ref = None;
    plan.steps.retain(|s| s.id != "preview" && s.id != "gguf");
    let legacy = serde_json::to_value(&plan).unwrap();
    assert!(legacy.get("previewRef").is_none());
    let reopened: Plan = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(serde_json::to_value(&reopened).unwrap(), legacy);
    validate_gguf_install(&reopened, tmp.path()).unwrap();
    assert!(!tmp.path().join("ComfyUI").exists());

    let root = Path::new(&plan.request.storage_path);
    std::fs::create_dir_all(root).unwrap();
    let original = serde_json::to_vec_pretty(&plan).unwrap();
    std::fs::write(root.join("deas-plan.json"), &original).unwrap();
    let mut updated = plan.clone();
    enable_preview_upgrade(&mut updated);
    enable_preview_upgrade(&mut updated);
    assert_eq!(updated.preview_ref.as_deref(), Some(KJNODES_REVISION));
    assert_eq!(
        updated.steps.iter().filter(|s| s.id == "preview").count(),
        1
    );
    assert_eq!(
        std::fs::read(root.join("deas-plan.json")).unwrap(),
        original
    );
    persist_updated_plan(&updated).unwrap();
    let saved = std::fs::read(root.join("deas-plan.json")).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&saved).unwrap(),
        serde_json::to_value(&updated).unwrap()
    );
    updated.preview_ref = Some("unapproved-source".into());
    assert!(persist_updated_plan(&updated).is_err());
    updated.preview_ref = Some(KJNODES_REVISION.into());
    updated.request.comfy_ref = "unapproved-core".into();
    assert!(persist_updated_plan(&updated).is_err());
    assert_eq!(std::fs::read(root.join("deas-plan.json")).unwrap(), saved);
}

#[test]
fn preview_torch_only_persistence_does_not_add_the_companion() {
    let tmp = scratch();
    let mut plan = Engine::default().plan(request(tmp.path())).unwrap();
    plan.preview_ref = None;
    plan.steps.retain(|s| s.id != "preview");
    plan.torch = Some(old_cpu());
    let root = Path::new(&plan.request.storage_path);
    std::fs::create_dir_all(root).unwrap();
    std::fs::write(
        root.join("deas-plan.json"),
        serde_json::to_vec(&plan).unwrap(),
    )
    .unwrap();
    plan.torch = Some(torch_profile::cpu());
    persist_updated_plan(&plan).unwrap();
    let saved: Plan =
        serde_json::from_slice(&std::fs::read(root.join("deas-plan.json")).unwrap()).unwrap();
    assert!(saved.preview_ref.is_none());
    assert!(!saved.steps.iter().any(|s| s.id == "preview"));
    assert!(!root.join("ComfyUI").exists());
}

#[test]
fn preview_validation_rejects_unowned_pin_manifest_head_and_modified_source() {
    let tmp = scratch();
    let mut plan = Engine::default().plan(request(tmp.path())).unwrap();
    plan.request.storage_path = tmp.path().to_str().unwrap().into();
    plan.gguf_ref = None;
    assert!(validate_gguf_install(&plan, tmp.path()).is_err());
    let node = tmp.path().join("ComfyUI/custom_nodes/ComfyUI-KJNodes");
    std::fs::create_dir_all(node.join(".git")).unwrap();
    std::fs::write(tmp.path().join("deas-install.json"), "{}").unwrap();
    std::fs::write(node.join(".git/HEAD"), format!("{KJNODES_REVISION}\n")).unwrap();
    assert!(validate_gguf_install(&plan, tmp.path())
        .unwrap_err()
        .contains("pinned revision"));
    std::fs::write(
        tmp.path().join("deas-install.json"),
        format!(r#"{{"previewRevision":"{KJNODES_REVISION}"}}"#),
    )
    .unwrap();
    std::fs::write(node.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    assert!(validate_gguf_install(&plan, tmp.path())
        .unwrap_err()
        .contains("pinned revision"));
    std::fs::write(node.join(".git/HEAD"), format!("{KJNODES_REVISION}\n")).unwrap();
    std::fs::write(node.join("__init__.py"), "modified code").unwrap();
    assert!(validate_gguf_install(&plan, tmp.path())
        .unwrap_err()
        .contains("source changed"));
    assert!(foreign_nodes(&plan).unwrap().contains("ComfyUI-KJNodes"));
    plan.preview_ref = None;
    assert!(foreign_nodes(&plan).unwrap().contains("ComfyUI-KJNodes"));
    validate_gguf_install(&plan, tmp.path()).unwrap();
    plan.preview_ref = Some("--invalid-revision".into());
    assert!(validate_gguf_install(&plan, tmp.path())
        .unwrap_err()
        .contains("immutable ComfyUI-KJNodes"));
    assert!(crate::logs::native_logs(0).iter().any(|entry| entry.source
        == crate::logs::Source::Deas
        && entry
            .text
            .contains("ERROR installer.gguf verify: Unsupported immutable ComfyUI-KJNodes")));
}

#[cfg(unix)]
#[test]
fn preview_redirected_checkout_is_rejected_without_writes() {
    let tmp = scratch();
    let outside = scratch();
    let comfy = tmp.path().join("ComfyUI");
    std::fs::create_dir_all(comfy.join("custom_nodes")).unwrap();
    std::os::unix::fs::symlink(outside.path(), comfy.join("custom_nodes/ComfyUI-KJNodes")).unwrap();
    assert!(node_target(&comfy, "ComfyUI-KJNodes").is_err());
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
#[ignore = "read-only pinned checkout fixture: set DEAS_KJNODES_SOURCE; copies to scratch, no install or inference"]
fn preview_owned_pinned_source_allows_torch_but_rejects_mutation() {
    let source = PathBuf::from(std::env::var_os("DEAS_KJNODES_SOURCE").unwrap());
    let tmp = scratch();
    let mut plan = Engine::default().plan(request(tmp.path())).unwrap();
    plan.request.storage_path = tmp.path().to_str().unwrap().into();
    plan.gguf_ref = None;
    let node = tmp.path().join("ComfyUI/custom_nodes/ComfyUI-KJNodes");
    std::fs::create_dir_all(node.join(".git")).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../kjnodes-source.json")).unwrap();
    for name in manifest["files"].as_object().unwrap().keys() {
        let target = node.join(name);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(source.join(name), target).unwrap();
    }
    std::fs::write(node.join(".git/HEAD"), format!("{KJNODES_REVISION}\n")).unwrap();
    std::fs::write(
        tmp.path().join("deas-install.json"),
        format!(r#"{{"previewRevision":"{KJNODES_REVISION}"}}"#),
    )
    .unwrap();
    validate_gguf_install(&plan, tmp.path()).unwrap();
    assert!(foreign_nodes(&plan).is_none());
    std::fs::write(
        node.join("nodes/preview_override_node.py"),
        "changed after approval",
    )
    .unwrap();
    assert!(validate_gguf_install(&plan, tmp.path()).is_err());
    assert!(foreign_nodes(&plan).unwrap().contains("ComfyUI-KJNodes"));
}
