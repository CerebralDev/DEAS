// Include inside install.rs's existing #[cfg(test)] mod tests.
#[test]
fn gguf_legacy_upgrade_adds_only_the_explicit_dependency_and_persists_after_success() {
    let tmp = scratch();
    let mut legacy = Engine::default().plan(request(tmp.path())).unwrap();
    legacy.gguf_ref = None;
    legacy.steps.retain(|s| s.id != "gguf");
    let root = Path::new(&legacy.request.storage_path);
    std::fs::create_dir_all(root).unwrap();
    let original = serde_json::to_vec_pretty(&legacy).unwrap();
    std::fs::write(root.join("deas-plan.json"), &original).unwrap();
    let mut updated = legacy.clone();
    enable_gguf_upgrade(&mut updated);
    assert_eq!(updated.gguf_ref.as_deref(), Some(GGUF_REVISION));
    assert_eq!(updated.steps.iter().filter(|s| s.id == "gguf").count(), 1);
    assert_eq!(
        serde_json::to_value(&updated.request).unwrap(),
        serde_json::to_value(&legacy.request).unwrap()
    );
    assert_eq!(
        std::fs::read(root.join("deas-plan.json")).unwrap(),
        original
    );
    persist_updated_plan(&updated).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(root.join("deas-plan.json")).unwrap()
        )
        .unwrap(),
        serde_json::to_value(&updated).unwrap()
    );
    enable_gguf_upgrade(&mut updated);
    assert_eq!(updated.steps.iter().filter(|s| s.id == "gguf").count(), 1);
    let current = std::fs::read(root.join("deas-plan.json")).unwrap();
    updated.request.comfy_ref = "different-unapproved-source".into();
    assert!(persist_updated_plan(&updated).is_err());
    assert_eq!(std::fs::read(root.join("deas-plan.json")).unwrap(), current);
}

#[tokio::test]
async fn gguf_checkout_refuses_git_worktree_redirection_before_writes() {
    let tmp = scratch();
    prepare_dirs(tmp.path()).unwrap();
    let git = git_path().expect("test requires existing Git prerequisite");
    let source = tmp.path().join("source");
    let target = tmp.path().join("target");
    let outside = tmp.path().join("outside");
    for dir in [&source, &target, &outside] {
        std::fs::create_dir(dir).unwrap();
    }
    let inner = Arc::new(Mutex::new(Inner::default()));
    let cancel = Arc::new(AtomicBool::new(false));
    for dir in [&source, &target] {
        run(&git, &["init", "."], dir, tmp.path(), &inner, &cancel)
            .await
            .unwrap();
    }
    std::fs::write(source.join("nodes.py"), "pinned source").unwrap();
    run(
        &git,
        &["add", "nodes.py"],
        &source,
        tmp.path(),
        &inner,
        &cancel,
    )
    .await
    .unwrap();
    run(
        &git,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "fixture",
        ],
        &source,
        tmp.path(),
        &inner,
        &cancel,
    )
    .await
    .unwrap();
    run(
        &git,
        &["remote", "add", "origin", source.to_str().unwrap()],
        &target,
        tmp.path(),
        &inner,
        &cancel,
    )
    .await
    .unwrap();
    run(
        &git,
        &["config", "core.worktree", outside.to_str().unwrap()],
        &target,
        tmp.path(),
        &inner,
        &cancel,
    )
    .await
    .unwrap();
    let result = checkout(
        &git,
        source.to_str().unwrap(),
        "HEAD",
        &target,
        tmp.path(),
        &inner,
        &cancel,
    )
    .await;
    assert!(result.unwrap_err().contains("worktree"));
    assert!(!outside.join("nodes.py").exists());
}

#[test]
#[ignore = "read-only pinned checkout fixture: set DEAS_GGUF_SOURCE; no installation or inference"]
fn gguf_pinned_source_contents_are_verified_not_just_head_text() {
    let source = PathBuf::from(std::env::var_os("DEAS_GGUF_SOURCE").unwrap());
    let tmp = scratch();
    let mut plan = Engine::default().plan(request(tmp.path())).unwrap();
    plan.preview_ref = None;
    let node = tmp.path().join("ComfyUI/custom_nodes/ComfyUI-GGUF");
    std::fs::create_dir_all(node.join(".git")).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../gguf-source.json")).unwrap();
    for entry in manifest["files"].as_object().unwrap().keys() {
        let target = node.join(entry);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(source.join(entry), target).unwrap();
    }
    std::fs::write(node.join(".git/HEAD"), format!("{GGUF_REVISION}\n")).unwrap();
    std::fs::write(
        tmp.path().join("deas-install.json"),
        format!(r#"{{"ggufRevision":"{GGUF_REVISION}"}}"#),
    )
    .unwrap();
    validate_gguf_install(&plan, tmp.path()).unwrap();
    std::fs::write(node.join("ops.py"), "changed after approval").unwrap();
    assert!(validate_gguf_install(&plan, tmp.path()).is_err());
}

#[test]
fn gguf_invalid_plan_failure_reaches_shared_log() {
    let tmp = scratch();
    let mut plan = Engine::default().plan(request(tmp.path())).unwrap();
    plan.preview_ref = None;
    plan.gguf_ref = Some("--not-a-commit".into());
    assert!(validate_gguf_install(&plan, tmp.path()).is_err());
    assert!(crate::logs::native_logs(0).iter().any(|entry| entry.source
        == crate::logs::Source::Deas
        && entry
            .text
            .contains("ERROR installer.gguf verify: Invalid immutable ComfyUI-GGUF revision")));
}

#[test]
fn gguf_baseline_is_planned_without_a_new_option_or_files() {
    let tmp = scratch();
    let engine = Engine::default();
    let r = request(tmp.path());
    let original = serde_json::to_value(&r).unwrap();
    let plan = engine.plan(r).unwrap();
    let gguf: Vec<_> = plan.steps.iter().filter(|s| s.id == "gguf").collect();
    assert_eq!(gguf.len(), 1);
    assert!(gguf[0]
        .detail
        .contains("6ea2651e7df66d7585f6ffee804b20e92fb38b8a"));
    assert_eq!(serde_json::to_value(&plan.request).unwrap(), original);
    assert!(!Path::new(&plan.request.storage_path).exists());
}

#[test]
fn gguf_legacy_plan_validation_is_read_only_and_needs_no_pack() {
    let tmp = scratch();
    let mut plan = Engine::default().plan(request(tmp.path())).unwrap();
    plan.preview_ref = None;
    plan.gguf_ref = None;
    plan.steps.retain(|s| s.id != "gguf");
    let legacy = serde_json::to_value(&plan).unwrap();
    assert!(legacy.get("ggufRef").is_none());
    let reopened: Plan = serde_json::from_value(legacy.clone()).unwrap();
    assert_eq!(serde_json::to_value(&reopened).unwrap(), legacy);
    validate_gguf_install(&plan, tmp.path()).unwrap();
    assert!(!tmp.path().join("ComfyUI").exists());
}

#[test]
fn gguf_reopen_requires_pin_manifest_and_regular_source_files() {
    let tmp = scratch();
    let mut plan = Engine::default().plan(request(tmp.path())).unwrap();
    plan.preview_ref = None;
    assert!(validate_gguf_install(&plan, tmp.path()).is_err());
    let node = tmp.path().join("ComfyUI/custom_nodes/ComfyUI-GGUF");
    std::fs::create_dir_all(node.join(".git")).unwrap();
    for file in ["__init__.py", "nodes.py", "requirements.txt"] {
        std::fs::write(node.join(file), "fixture").unwrap();
    }
    std::fs::write(node.join(".git/HEAD"), format!("{GGUF_REVISION}\n")).unwrap();
    std::fs::write(
        tmp.path().join("deas-install.json"),
        format!(r#"{{"ggufRevision":"{GGUF_REVISION}"}}"#),
    )
    .unwrap();
    assert!(validate_gguf_install(&plan, tmp.path())
        .unwrap_err()
        .contains("source"));
    std::fs::write(node.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    assert!(validate_gguf_install(&plan, tmp.path()).is_err());
    std::fs::write(node.join(".git/HEAD"), format!("{GGUF_REVISION}\n")).unwrap();
    std::fs::write(
        tmp.path().join("deas-install.json"),
        r#"{"ggufRevision":"wrong"}"#,
    )
    .unwrap();
    assert!(validate_gguf_install(&plan, tmp.path()).is_err());
    std::fs::write(
        tmp.path().join("deas-install.json"),
        format!(r#"{{"ggufRevision":"{GGUF_REVISION}"}}"#),
    )
    .unwrap();
    std::fs::remove_file(node.join("nodes.py")).unwrap();
    assert!(validate_gguf_install(&plan, tmp.path()).is_err());
}

#[cfg(unix)]
#[test]
fn gguf_checkout_rejects_external_symlink_without_writes() {
    let tmp = scratch();
    let outside = scratch();
    let comfy = tmp.path().join("ComfyUI");
    std::fs::create_dir(&comfy).unwrap();
    std::os::unix::fs::symlink(outside.path(), comfy.join("custom_nodes")).unwrap();
    assert!(node_target(&comfy, "ComfyUI-GGUF").is_err());
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}
