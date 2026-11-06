use super::*;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

async fn declaration(root: &Path, category: &str, name: &str) -> PathBuf {
    let path = root.join(category).join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"fixture weights").unwrap();
    crate::downloads::models::native_model_family_write(
        path.to_str().unwrap().into(),
        Some("sdxl".into()),
    )
    .await
    .unwrap();
    path
}

fn corrupt(path: &Path, replacement: bool) {
    if replacement {
        let other = path.with_extension("replacement");
        std::fs::write(&other, b"replaced weights").unwrap();
        std::fs::rename(other, path).unwrap();
    } else {
        std::fs::write(format!("{}.deas.json", path.display()), b"{broken").unwrap();
    }
}

async fn backend() -> (String, tokio::task::JoinHandle<()>, Arc<AtomicUsize>) {
    let posts = Arc::new(AtomicUsize::new(0));
    let observed = posts.clone();
    let (base, task) = server(move |path, _| {
        if path == "GET /comfy/object_info HTTP/1.1" {
            let mut info = nodes();
            info["LoraLoader"] =
                json!({"input":{"required":{"lora_name":[["style/a.safetensors"]]}}});
            (200, info.to_string())
        } else {
            assert_eq!(path, "POST /comfy/prompt HTTP/1.1");
            observed.fetch_add(1, Ordering::SeqCst);
            (
                200,
                json!({"prompt_id":PROMPT_ID,"number":1,"node_errors":{}}).to_string(),
            )
        }
    })
    .await;
    (base, task, posts)
}

#[tokio::test]
async fn local_declaration_preparation_rechecks_scanned_model_and_lora_without_post() {
    let (base, task, posts) = backend().await;
    for category in ["checkpoints", "loras"] {
        for replacement in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let model = declaration(root.path(), "checkpoints", "sd/model.safetensors").await;
            let lora = declaration(root.path(), "loras", "style/a.safetensors").await;
            let scanned = crate::downloads::models::scan_roots(&[root.path().into()]).unwrap();
            assert_eq!(scanned.len(), 2);
            assert!(scanned.iter().all(
                |m| m.family_override.as_deref() == Some("sdxl") && m.metadata_error.is_none()
            ));
            corrupt(
                if category == "checkpoints" {
                    &model
                } else {
                    &lora
                },
                replacement,
            );
            let mut draft = intent();
            draft["loras"] = json!([{"name":"style/a.safetensors","strength":0.5}]);
            let jobs = Jobs::default();
            let error = jobs
                .prepare_with_models(base.clone(), draft, vec![root.path().into()])
                .await
                .unwrap_err();
            assert!(
                error.contains("MODEL_METADATA_INVALID"),
                "{category}: {error}"
            );
            assert!(jobs.lock().unwrap().plan.is_none());
        }
    }
    assert_eq!(posts.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
async fn local_declaration_valid_and_undeclared_preparation_submit_normally() {
    let (base, task, posts) = backend().await;
    for declared in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let model = declaration(root.path(), "checkpoints", "sd/model.safetensors").await;
        if !declared {
            std::fs::remove_file(format!("{}.deas.json", model.display())).unwrap();
        }
        let jobs = Jobs::default();
        let plan = jobs
            .prepare_with_models(base.clone(), intent(), vec![root.path().into()])
            .await
            .unwrap();
        let job = jobs
            .submit(plan["id"].as_str().unwrap().into(), true)
            .await
            .unwrap();
        assert_eq!(job["state"], "queued");
    }
    assert_eq!(posts.load(Ordering::SeqCst), 2);
    task.abort();
}

#[tokio::test]
async fn local_declaration_submit_rechecks_after_successful_native_preparation() {
    let (base, task, posts) = backend().await;
    for category in ["checkpoints", "loras"] {
        for replacement in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let model = declaration(root.path(), "checkpoints", "sd/model.safetensors").await;
            let lora = declaration(root.path(), "loras", "style/a.safetensors").await;
            let mut draft = intent();
            draft["loras"] = json!([{"name":"style/a.safetensors","strength":0.5}]);
            let jobs = Jobs::default();
            let plan = jobs
                .prepare_with_models(base.clone(), draft, vec![root.path().into()])
                .await
                .unwrap();
            corrupt(
                if category == "checkpoints" {
                    &model
                } else {
                    &lora
                },
                replacement,
            );
            let error = jobs
                .submit(plan["id"].as_str().unwrap().into(), true)
                .await
                .unwrap_err();
            assert!(
                error.contains("MODEL_METADATA_INVALID"),
                "{category}: {error}"
            );
            assert!(jobs.lock().unwrap().job.is_none());
        }
    }
    assert_eq!(posts.load(Ordering::SeqCst), 0);
    task.abort();
}

#[tokio::test]
#[allow(clippy::result_large_err)] // Tungstenite fixes the handshake callback's error type.
async fn local_declaration_changed_during_observer_connect_fails_before_prompt() {
    for replacement in [false, true] {
        let (base, task, posts) = backend().await;
        let root = tempfile::tempdir().unwrap();
        let path = declaration(root.path(), "checkpoints", "sd/model.safetensors").await;
        let jobs = Jobs::default();
        let plan = jobs
            .prepare_with_models(base, intent(), vec![root.path().into()])
            .await
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        // Route this held plan through a real handshake that invalidates the metadata.
        jobs.lock().unwrap().plan.as_mut().unwrap().base =
            crate::validate_base_url(&format!("http://{}", listener.local_addr().unwrap()))
                .unwrap();
        let peer = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_hdr_async(
                socket,
                move |_: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                    corrupt(&path, replacement);
                    Ok(response)
                },
            )
            .await
            .unwrap();
            use futures_util::StreamExt;
            assert!(matches!(
                socket.next().await,
                Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_)))
            ));
        });
        let id = plan["id"].as_str().unwrap().to_owned();
        let error = jobs.submit(id.clone(), true).await.unwrap_err();
        assert!(error.contains("MODEL_METADATA_INVALID"), "{error}");
        let job = jobs.lock().unwrap().job.clone().unwrap();
        assert_eq!(job.state, "failed");
        assert!(!job.submitting);
        assert!(job.plan.consumed);
        assert!(job.prompt_id.is_none());
        let completed = job.live.lock().unwrap().observer.completion();
        super::observer_tests::finished(completed).await;
        tokio::time::timeout(Duration::from_secs(5), peer)
            .await
            .unwrap()
            .unwrap();
        assert!(jobs
            .submit(id, true)
            .await
            .unwrap_err()
            .contains("retries are forbidden"));
        assert_eq!(posts.load(Ordering::SeqCst), 0);
        task.abort();
    }
}

#[tokio::test]
async fn local_declaration_external_backend_has_no_machine_path_authority() {
    let (base, task, posts) = backend().await;
    let root = tempfile::tempdir().unwrap();
    let path = declaration(root.path(), "checkpoints", "sd/model.safetensors").await;
    corrupt(&path, false);
    let engine = crate::install::Engine::default();
    assert!(LocalModels::owned(&engine, &base).is_empty());
    let jobs = Jobs::default();
    let plan = jobs
        .prepare_with_models(
            base,
            intent(),
            LocalModels::owned(&engine, "http://127.0.0.1:9"),
        )
        .await
        .unwrap();
    assert_eq!(
        jobs.submit(plan["id"].as_str().unwrap().into(), true)
            .await
            .unwrap()["state"],
        "queued"
    );
    assert_eq!(posts.load(Ordering::SeqCst), 1);
    task.abort();
}

#[tokio::test]
async fn gguf_sidecar_split_preparation_checks_real_loader_catalog_fields() {
    for missing in ["", "UnetLoaderGGUF", "CLIPLoaderGGUF", "encoder-file"] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("diffusion_models/renamed.gguf");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Advisory-family fixture only, not a loadable quantized model.
        std::fs::write(&path, b"unknown architecture fixture").unwrap();
        crate::downloads::models::native_model_family_write(
            path.to_str().unwrap().into(),
            Some("anima".into()),
        )
        .await
        .unwrap();
        let scanned = crate::downloads::models::scan_roots(&[root.path().into()]).unwrap();
        assert_eq!(scanned[0].family.as_deref(), Some("anima"));
        assert_eq!(scanned[0].family_override.as_deref(), Some("anima"));
        assert!(scanned[0].detected_family.is_none());
        let mut info = nodes();
        info["UnetLoaderGGUF"] = json!({"input":{"required":{"unet_name":[["renamed.gguf"]]}}});
        info["CLIPLoaderGGUF"] = json!({"input":{"required":{"clip_name":[["encoder.gguf"]],"type":[["stable_diffusion"]]}}});
        info["VAELoader"] = json!({"input":{"required":{"vae_name":[["vae.safetensors"]]}}});
        if missing == "encoder-file" {
            info["CLIPLoaderGGUF"]["input"]["required"]["clip_name"] = json!([[]]);
        } else if !missing.is_empty() {
            info.as_object_mut().unwrap().remove(missing);
        }
        let (base, task) = server(move |request, _| {
            assert_eq!(request, "GET /comfy/object_info HTTP/1.1");
            (200, info.to_string())
        })
        .await;
        let mut draft = intent();
        draft["checkpoint"] = json!("renamed.gguf");
        draft["diffusion"] = json!({"textEncoder":"encoder.gguf","clipType":"stable_diffusion","vae":"vae.safetensors"});
        let jobs = Jobs::default();
        let prepared = jobs
            .prepare_with_models(base, draft, vec![root.path().into()])
            .await;
        if missing.is_empty() {
            let plan = prepared.unwrap();
            assert_eq!(plan["graph"]["1"]["class_type"], "UnetLoaderGGUF");
            assert_eq!(plan["graph"]["C"]["class_type"], "CLIPLoaderGGUF");
        } else {
            assert!(prepared.is_err(), "missing {missing}");
        }
        assert!(jobs.lock().unwrap().job.is_none());
        task.abort();
        let reset = crate::downloads::models::native_model_family_write(
            path.to_str().unwrap().into(),
            None,
        )
        .await
        .unwrap();
        assert!(reset.family_override.is_none());
        assert!(reset.family.is_none());
    }
}

#[test]
fn gguf_image_companions_use_matching_loader_and_unsupported_formats_fail() {
    let mut draft = intent();
    draft["diffusion"] =
        json!({"textEncoder":"clip.gguf","clipType":"wan","vae":"vae.safetensors"});
    assert_eq!(
        compile(&draft).unwrap()["C"]["class_type"],
        "CLIPLoaderGGUF"
    );
    draft["diffusion"]["vae"] = json!("vae.gguf");
    assert!(compile(&draft).unwrap_err().contains("GGUF VAE"));
    draft = intent();
    draft["loras"] = json!([{"name":"legacy.ckpt","strength":1}]);
    assert!(compile(&draft).unwrap_err().contains("safetensors"));
}

#[test]
fn gguf_image_models_load_via_gguf_nodes_and_loras_stay_rejected() {
    for name in ["model.gguf", "model.GGUF"] {
        let mut draft = intent();
        draft["checkpoint"] = json!(name);
        assert!(compile(&draft)
            .unwrap_err()
            .contains("GGUF requires a diffusion recipe"));
        draft["diffusion"] =
            json!({"textEncoder":"clip.safetensors","clipType":"wan","vae":"vae.safetensors"});
        let graph = compile(&draft).unwrap();
        assert_eq!(graph["1"]["class_type"], "UnetLoaderGGUF");
        assert!(graph["1"]["inputs"].get("weight_dtype").is_none());
        // LoRA on a quantized base needs execution proof per pipeline; not advertised.
        draft["loras"] = json!([{"name":"style.safetensors","strength":0.5}]);
        assert!(compile(&draft).unwrap_err().contains("LoRA on a GGUF base"));
    }
    let mut draft = intent();
    draft["loras"] = json!([{"name":"style.gguf","strength":0.5}]);
    assert!(compile(&draft).unwrap_err().contains("GGUF LoRA"));
    draft = intent();
    draft["checkpoint"] = json!("legacy.ckpt");
    assert_eq!(
        compile(&draft).unwrap()["1"]["class_type"],
        "CheckpointLoaderSimple"
    );
}
