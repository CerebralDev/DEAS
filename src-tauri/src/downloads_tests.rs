use super::*;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
};
#[cfg(any(target_os = "linux", windows))]
fn scratch() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}
fn request(stream: &mut TcpStream) -> String {
    let mut b = Vec::new();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    while !b.ends_with(b"\r\n\r\n") {
        let mut c = [0];
        stream.read_exact(&mut c).unwrap();
        b.push(c[0]);
        assert!(b.len() < 16384);
    }
    String::from_utf8(b).unwrap()
}
fn fixture(body: Vec<u8>) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let req = request(&mut stream);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(&body).unwrap();
        req
    });
    (url, task)
}
fn provider(origin: String) -> Provider {
    Provider {
        id: "fixture".into(),
        kind: "http".into(),
        label: "Fixture".into(),
        origin,
        scopes: vec!["index".into()],
        anonymous_notice: "Fixture only".into(),
    }
}
#[cfg(any(target_os = "linux", windows))]
fn resource(origin: &str, hash: Option<String>, size: Option<u64>) -> Resource {
    Resource {
        id: "fixture-file".into(),
        label: "Tiny file".into(),
        kind: "file".into(),
        url: Some(format!("{origin}/tiny.bin")),
        size,
        sha256: hash,
        downloadable: true,
        locator: Some("tiny.bin".into()),
        license: Some("fixture license".into()),
        preview: None,
        category: None,
        auth: false,
        nsfw: false,
        base: None,
        published_at: None,
        version: None,
        variant: None,
    }
}
#[cfg(any(target_os = "linux", windows))]
fn engine(p: Provider, r: &Resource) -> DownloadEngine {
    let e = DownloadEngine::default();
    let mut i = e.inner.lock().unwrap();
    i.catalog = Ok(Catalog {
        version: 1,
        providers: vec![p.clone()],
        entries: vec![],
    });
    i.known
        .insert((p.id, r.id.clone()), (r.clone(), "tiny.bin".into()));
    drop(i);
    e
}
#[cfg(any(target_os = "linux", windows))]
fn prepare(e: &DownloadEngine, r: Resource, d: &tempfile::TempDir) -> DownloadPlan {
    e.plan(PlanRequest {
        provider_id: "fixture".into(),
        resources: vec![r],
        parent_path: d.path().to_str().unwrap().into(),
        folder_name: "stage".into(),
        authenticated: false,
    })
    .unwrap()
}
#[cfg(any(target_os = "linux", windows))]
fn finished(e: &DownloadEngine) -> DownloadStatus {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let s = e.status().unwrap();
        if s.state != "running" {
            return s;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(20));
    }
}
#[cfg(not(any(target_os = "linux", windows)))]
#[test]
fn unsupported_platform_anchor_fails_closed() {
    assert!(fs::Anchor::open(std::path::Path::new(".")).is_err());
}

#[cfg(target_os = "linux")]
#[test]
fn shared_parent_rejected_before_staging_write() {
    use std::os::unix::fs::PermissionsExt;
    for mode in [0o720, 0o702, 0o777, 0o1777] {
        let d = scratch();
        let anchor = fs::Anchor::open(d.path()).unwrap();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(mode)).unwrap();
        assert!(
            anchor.create("stage").is_err(),
            "accepted parent mode {mode:o}"
        );
        assert!(!d.path().join("stage").exists());
    }
}

#[test]
fn strict_catalog_and_extension() {
    let e = DownloadEngine::default();
    let c = e.catalog().unwrap();
    assert_eq!(c.providers.len(), 5);
    assert_eq!(
        serde_json::to_value(&c).unwrap()["entries"]
            .as_array()
            .unwrap()
            .len(),
        14 + 35
    );
    let bundled = c
        .entries
        .iter()
        .filter(|e| e.id.starts_with("civitai-") && !e.id.starts_with("civitai-creator"))
        .collect::<Vec<_>>();
    assert_eq!(bundled.len(), 35);
    assert!(bundled.iter().any(|e| e.auth) && bundled.iter().any(|e| e.media.is_some()));
    assert!(bundled.iter().any(|e| e.nsfw) && bundled.iter().any(|e| !e.nsfw));
    assert!(bundled.iter().all(|e| e.base.is_some()));
    assert!(bundled.iter().all(|e| e.scope == "model"
        && matches!(
            e.target.as_str(),
            "checkpoints" | "diffusion_models" | "loras" | "workflows"
        )));
    assert!(bundled.iter().filter(|e| e.preview.is_some()).count() >= 10);
    for field in ["command='sh'", "token='secret'", "hook='evil'"] {
        assert!(Catalog::parse(&format!("version=1\nproviders=[]\n{field}")).is_err());
    }
    let extra = "version=1\n[[providers]]\nid='custom'\nkind='http'\nlabel='Files'\norigin='https://files.example.org'\nscopes=['index']\nanonymousNotice='Anonymous only'";
    assert_eq!(e.import(extra.into()).unwrap().providers.len(), 6);
    assert!(e.import(extra.into()).is_err());
    assert_eq!(e.catalog().unwrap().providers.len(), 6);
}
#[test]
fn catalog_entry_extension_references_existing_provider() {
    let e = DownloadEngine::default();
    let text = "version=1\nproviders=[]\n[[entries]]\nid='another-repo'\nlabel='Another repo'\nproviderId='github'\nscope='repo'\nlocator='alice/tiny'\nkind='repo'\ntarget='tiny-files'";
    let c = e.import(text.into()).unwrap();
    assert_eq!(c.entries.last().unwrap().id, "another-repo");
    assert!(e.import(text.into()).is_err());
}
#[tokio::test]
async fn direct_json_file_and_index_bounds() {
    let p = provider("https://files.example.org".into());
    let r = BrowseRequest {
        provider_id: p.id.clone(),
        scope: "index".into(),
        locator: "direct:https://files.example.org/workflow.json".into(),
        cursor: None,
        authenticated: false,
        nsfw: false,
    };
    let page = providers::list(&p, &r, None, None).await.unwrap();
    assert_eq!(page.resources[0].locator.as_deref(), Some("workflow.json"));
    assert!(page.resources[0].downloadable);
    let (origin, server) = fixture(b"not JSON".to_vec());
    let p = provider(origin.clone());
    let r = BrowseRequest {
        provider_id: p.id.clone(),
        scope: "index".into(),
        locator: format!("{origin}/index.json"),
        cursor: None,
        authenticated: false,
        nsfw: false,
    };
    assert!(providers::list(&p, &r, None, None).await.is_err());
    server.join().unwrap();
}
#[cfg(any(target_os = "linux", windows))]
#[test]
fn parent_identity_change_and_existing_target_block_start() {
    let d = scratch();
    let parent = d.path().join("parent");
    std::fs::create_dir(&parent).unwrap();
    let r = resource("https://files.example.org", None, None);
    let e = engine(provider("https://files.example.org".into()), &r);
    let req = PlanRequest {
        provider_id: "fixture".into(),
        resources: vec![r],
        parent_path: parent.to_str().unwrap().into(),
        folder_name: "stage".into(),
        authenticated: false,
    };
    let p = e.plan(req).unwrap();
    std::fs::rename(&parent, d.path().join("old")).unwrap();
    std::fs::create_dir(&parent).unwrap();
    assert!(e.start(p.id, true, &AuthState::default()).is_err());
    assert!(!parent.join("stage").exists());
}
#[test]
fn url_path_metadata_and_auth_safety() {
    for s in ["../x", "x/y", "x\\y", "..", "CON", "x.", "x:"] {
        assert!(safe_name(s).is_err());
    }
    for s in [
        "https://evil.example/x?token=s",
        "https://user:pass@evil.example/x",
        "https://@evil.example/x",
        "http://example.org/x",
        "https://192.168.1.1/x",
    ] {
        assert!(validate_url(s, false, false).is_err());
    }
    for ip in [
        "127.0.0.1",
        "10.1.1.1",
        "169.254.1.1",
        "100.64.1.1",
        "::1",
        "fc00::1",
        "::ffff:127.0.0.1",
    ] {
        assert!(!public_ip(ip.parse().unwrap()));
    }
    assert!(public_ip("1.1.1.1".parse().unwrap()));
    // Real CivitAI version downloads select one file of the version with ?fileId=<n>.
    let mut civitai = provider("https://civitai.com".into());
    civitai.kind = "civitai".into();
    for ok in [
        "https://civitai.com/api/download/models/2953474?fileId=2833009",
        "https://civitai.com/api/download/models/34?type=Model&format=SafeTensor",
    ] {
        assert!(source_url(&civitai, ok).is_ok(), "{ok}");
    }
    for bad in [
        "https://civitai.com/api/download/models/34?token=secret",
        "https://civitai.com/api/download/models/34?fileId=1/2",
    ] {
        assert!(source_url(&civitai, bad).is_err(), "{bad}");
    }
    let p = provider("https://files.example.org".into());
    let auth = AuthState::default();
    assert!(credential(&auth, &p, true).is_err());
    assert!(credential(&auth, &p, false).unwrap().is_none());
}
#[cfg(any(target_os = "linux", windows))]
#[test]
fn immutable_plan_rejects_frontend_forgery() {
    let d = scratch();
    let r = resource("https://files.example.org", None, Some(3));
    let e = engine(provider("https://files.example.org".into()), &r);
    let plan = prepare(&e, r.clone(), &d);
    assert!(!d.path().join("stage").exists());
    assert_eq!(plan.total_known_bytes, 3);
    let mut forged = r.clone();
    forged.size = Some(1);
    assert!(e
        .plan(PlanRequest {
            provider_id: "fixture".into(),
            resources: vec![forged],
            parent_path: d.path().to_str().unwrap().into(),
            folder_name: "bad".into(),
            authenticated: false
        })
        .is_err());
    assert!(e.start(plan.id, false, &AuthState::default()).is_err());
}
#[cfg(any(target_os = "linux", windows))]
#[test]
fn real_stream_digest_bytes_and_status() {
    let bytes = b"real tiny fixture bytes\n".to_vec();
    let (origin, server) = fixture(bytes.clone());
    let r = resource(
        &origin,
        Some(format!("{:x}", Sha256::digest(&bytes))),
        Some(bytes.len() as u64),
    );
    let e = engine(provider(origin), &r);
    let d = scratch();
    let plan = prepare(&e, r, &d);
    let initial = e
        .start(plan.id.clone(), true, &AuthState::default())
        .unwrap();
    assert_eq!(initial.state, "running");
    let status = finished(&e);
    assert_eq!(status.state, "succeeded", "{:?}", status.error);
    assert_eq!(status.completed, 1);
    assert_eq!(status.total, 1);
    assert_eq!(status.bytes_downloaded, bytes.len() as u64);
    assert_eq!(
        std::fs::read(d.path().join("stage/tiny.bin")).unwrap(),
        bytes
    );
    assert_eq!(
        std::fs::read_dir(d.path().join("stage")).unwrap().count(),
        2
    );
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(d.path().join("stage/deas-download-manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        manifest["plan"]["resources"][0]["license"],
        "fixture license"
    );
    assert!(manifest["plan"]["resources"][0]["sha256"].is_string());
    assert!(!server
        .join()
        .unwrap()
        .to_lowercase()
        .contains("authorization:"));
    assert!(e.start(plan.id, true, &AuthState::default()).is_err());
}
#[cfg(any(target_os = "linux", windows))]
#[test]
fn corrupt_digest_or_size_never_publishes() {
    for (hash, size) in [(Some("0".repeat(64)), Some(3)), (None, Some(4))] {
        let (origin, server) = fixture(b"abc".to_vec());
        let r = resource(&origin, hash, size);
        let e = engine(provider(origin), &r);
        let d = scratch();
        let p = prepare(&e, r, &d);
        e.start(p.id, true, &AuthState::default()).unwrap();
        assert_eq!(finished(&e).state, "failed");
        assert_eq!(
            std::fs::read_dir(d.path().join("stage")).unwrap().count(),
            0
        );
        server.join().unwrap();
    }
}
#[tokio::test]
async fn foreign_redirect_strips_credential() {
    let (dest, server) = fixture(b"abc".to_vec());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let redirect = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let req = request(&mut s);
        write!(s,"HTTP/1.1 302 Found\r\nLocation: {dest}/tiny.bin\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        req
    });
    let p = provider(origin.clone());
    let mut response = fetch(
        &client_for(false, None).unwrap(),
        &p,
        &format!("{origin}/tiny.bin"),
        Some(HeaderValue::from_static("Bearer fixture-secret")),
        &AtomicBool::new(false),
        None,
    )
    .await
    .unwrap();
    assert_eq!(response.chunk().await.unwrap().unwrap().as_ref(), b"abc");
    assert!(redirect
        .join()
        .unwrap()
        .to_lowercase()
        .contains("authorization: bearer fixture-secret"));
    assert!(!server
        .join()
        .unwrap()
        .to_lowercase()
        .contains("authorization:"));
}
#[cfg(any(target_os = "linux", windows))]
#[test]
fn cancellation_interrupts_stalled_stream_and_keeps_the_partial_for_resume() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    let task = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        request(&mut s);
        s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nabc")
            .unwrap();
        tx.send(()).unwrap();
        thread::sleep(Duration::from_millis(700));
    });
    let r = resource(&origin, None, None);
    let e = engine(provider(origin), &r);
    let d = scratch();
    let p = prepare(&e, r, &d);
    e.start(p.id, true, &AuthState::default()).unwrap();
    rx.recv_timeout(Duration::from_secs(3)).unwrap();
    e.cancel().unwrap();
    assert_eq!(finished(&e).state, "cancelled");
    // Cancelling keeps what arrived so the next run can resume it.
    let kept: Vec<_> = std::fs::read_dir(d.path().join("stage"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert!(kept[0].starts_with("deas-") && kept[0].ends_with(".partial"));
    task.join().unwrap();
}
#[cfg(any(target_os = "linux", windows))]
#[test]
fn empty_folder_creates_only_the_absent_destination_leaf_after_confirmation() {
    let bytes = b"latent fixture".to_vec();
    let (origin, server) = fixture(bytes.clone());
    let r = resource(
        &origin,
        Some(format!("{:x}", Sha256::digest(&bytes))),
        Some(bytes.len() as u64),
    );
    let e = engine(provider(origin), &r);
    let d = scratch();
    let target = d.path().join("latent_upscale_models");
    let p = e
        .plan(PlanRequest {
            provider_id: "fixture".into(),
            resources: vec![r],
            parent_path: target.to_str().unwrap().into(),
            folder_name: String::new(),
            authenticated: false,
        })
        .unwrap();
    assert_eq!(p.target, target.to_str().unwrap());
    assert!(!target.exists());
    assert!(e.start(p.id.clone(), false, &AuthState::default()).is_err());
    assert!(!target.exists());
    e.start(p.id, true, &AuthState::default()).unwrap();
    let status = finished(&e);
    assert_eq!(status.state, "succeeded", "{:?}", status.error);
    server.join().unwrap();
    assert_eq!(std::fs::read(target.join("tiny.bin")).unwrap(), bytes);
    assert!(!d.path().join("tiny.bin").exists());
}
#[cfg(any(target_os = "linux", windows))]
#[test]
fn absent_destination_leaf_rejects_unsafe_paths_and_start_collisions() {
    let d = scratch();
    let r = resource("https://files.example.org", None, Some(3));
    let e = engine(provider("https://files.example.org".into()), &r);
    let request = |path: &std::path::Path, folder: &str| PlanRequest {
        provider_id: "fixture".into(),
        resources: vec![r.clone()],
        parent_path: path.to_str().unwrap().into(),
        folder_name: folder.into(),
        authenticated: false,
    };
    for path in ["missing/latent_upscale_models", "CON", "bad.", "bad:"] {
        assert!(e.plan(request(&d.path().join(path), "")).is_err(), "{path}");
    }
    assert!(e
        .plan(request(&d.path().join("latent_upscale_models"), "nested"))
        .is_err());
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 0);
    let file = d.path().join("file");
    std::fs::write(&file, b"keep").unwrap();
    for path in [&file, &file.join("latent_upscale_models")] {
        assert!(e.plan(request(path, "")).is_err());
    }
    for directory in [false, true] {
        let target = d
            .path()
            .join(if directory { "directory" } else { "collision" });
        let p = e.plan(request(&target, "")).unwrap();
        if directory {
            std::fs::create_dir(&target).unwrap();
            std::fs::write(target.join("tiny.bin"), b"keep").unwrap();
        } else {
            std::fs::write(&target, b"keep").unwrap();
        }
        assert!(e.start(p.id, true, &AuthState::default()).is_err());
        let existing = if directory {
            target.join("tiny.bin")
        } else {
            target
        };
        assert_eq!(std::fs::read(existing).unwrap(), b"keep");
    }
    let parent = d.path().join("models");
    std::fs::create_dir(&parent).unwrap();
    let p = e
        .plan(request(&parent.join("latent_upscale_models"), ""))
        .unwrap();
    std::fs::rename(&parent, d.path().join("old models")).unwrap();
    std::fs::create_dir(&parent).unwrap();
    assert!(e.start(p.id, true, &AuthState::default()).is_err());
    assert!(!parent.join("latent_upscale_models").exists());
    assert!(!d.path().join("old models/latent_upscale_models").exists());
}
#[cfg(target_os = "linux")]
#[test]
fn absent_destination_leaf_rejects_symlinks_before_plan_and_start() {
    use std::os::unix::fs::symlink;
    let d = scratch();
    let outside = scratch();
    let r = resource("https://files.example.org", None, Some(3));
    let e = engine(provider("https://files.example.org".into()), &r);
    let target = d.path().join("latent_upscale_models");
    let request = PlanRequest {
        provider_id: "fixture".into(),
        resources: vec![r],
        parent_path: target.to_str().unwrap().into(),
        folder_name: String::new(),
        authenticated: false,
    };
    for destination in [outside.path().to_owned(), outside.path().join("missing")] {
        symlink(&destination, &target).unwrap();
        assert!(e.plan(request.clone()).is_err());
        std::fs::remove_file(&target).unwrap();
        let p = e.plan(request.clone()).unwrap();
        symlink(&destination, &target).unwrap();
        assert!(e.start(p.id, true, &AuthState::default()).is_err());
        assert_eq!(std::fs::read_link(&target).unwrap(), destination);
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
        std::fs::remove_file(&target).unwrap();
    }
    symlink(outside.path(), d.path().join("linked models")).unwrap();
    assert!(e
        .plan(PlanRequest {
            parent_path: d
                .path()
                .join("linked models/latent_upscale_models")
                .to_str()
                .unwrap()
                .into(),
            ..request
        })
        .is_err());
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}
#[cfg(any(target_os = "linux", windows))]
#[test]
fn empty_folder_stages_into_existing_parent_without_manifest_or_clobber() {
    let (origin, server) = fixture(b"abc".to_vec());
    let r = resource(&origin, None, Some(3));
    let e = engine(provider(origin.clone()), &r);
    let d = scratch();
    let request = PlanRequest {
        provider_id: "fixture".into(),
        resources: vec![r.clone()],
        parent_path: d.path().to_str().unwrap().into(),
        folder_name: String::new(),
        authenticated: false,
    };
    let p = e.plan(request.clone()).unwrap();
    assert_eq!(p.target, d.path().to_str().unwrap());
    e.start(p.id, true, &AuthState::default()).unwrap();
    assert_eq!(finished(&e).state, "succeeded");
    server.join().unwrap();
    assert_eq!(std::fs::read(d.path().join("tiny.bin")).unwrap(), b"abc");
    assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
    // A second run into the same folder must not replace the existing file.
    let (origin2, server2) = fixture(b"xyz".to_vec());
    let r2 = resource(&origin2, None, Some(3));
    let e2 = engine(provider(origin2), &r2);
    let p2 = e2
        .plan(PlanRequest {
            resources: vec![r2],
            ..request
        })
        .unwrap();
    e2.start(p2.id, true, &AuthState::default()).unwrap();
    assert_eq!(finished(&e2).state, "failed");
    let _ = server2.join();
    assert_eq!(std::fs::read(d.path().join("tiny.bin")).unwrap(), b"abc");
}
#[cfg(target_os = "linux")]
#[test]
fn anchored_writes_refuse_symlinks_and_overwrites() {
    use std::os::unix::fs::symlink;
    let d = scratch();
    let outside = scratch();
    symlink(outside.path(), d.path().join("link")).unwrap();
    assert!(fs::Anchor::open(&d.path().join("link")).is_err());
    let a = fs::Anchor::open(d.path()).unwrap();
    let sub = a.create("stage").unwrap();
    assert!(a.create("stage").is_err());
    let mut f = sub.partial("part").unwrap();
    f.write_all(b"real").unwrap();
    sub.publish("part", "file").unwrap();
    let mut f = sub.partial("next").unwrap();
    f.write_all(b"bad").unwrap();
    assert!(sub.publish("next", "file").is_err());
    assert_eq!(std::fs::read(d.path().join("stage/file")).unwrap(), b"real");
}
#[tokio::test]
async fn official_adapter_shapes_use_real_fixture_transport() {
    let cases = vec![
        (
            "huggingface",
            "user",
            "datasets:alice",
            serde_json::json!([{"id":"alice/tiny"}]),
            "/api/datasets?author=alice&limit=50",
            false,
        ),
        (
            "huggingface",
            "repo",
            "datasets:alice/tiny",
            serde_json::json!([
                {"type":"file","path":".gitattributes","size":100},
                {"type":"file","path":"sub/.metadata","size":100},
                {"type":"file","path":"sub/tiny.bin","size":3,"lfs":{"oid":"0".repeat(64),"size":3}}
            ]),
            "/api/datasets/alice/tiny/tree/main?recursive=true&limit=50",
            true,
        ),
        (
            "huggingface",
            "repo",
            "models:alice/upscaler",
            serde_json::json!([
                {"type":"file","path":".gitattributes","size":100},
                {"type":"file","path":"weights/upscaler.safetensors","size":3,"lfs":{"oid":"0".repeat(64),"size":3}}
            ]),
            "/api/models/alice/upscaler/tree/main?recursive=true&limit=50",
            true,
        ),
        (
            "github",
            "user",
            "alice",
            serde_json::json!([{"full_name":"alice/tiny","license":{"spdx_id":"MIT"}}]),
            "/users/alice/repos?per_page=50&page=1",
            false,
        ),
        (
            "civitai",
            "model",
            "12",
            serde_json::json!({"id":12,"modelVersions":[{"id":34,"name":"fp8 v2","files":[{"id":56,"name":"tiny.safetensors","downloadUrl":"https://civitai.com/api/download/models/34?fileId=56","hashes":{"SHA256":"0".repeat(64)},"sizeKB":14188263.9375,"metadata":{"format":"SafeTensor","size":"pruned","fp":"fp8"}}]}]}),
            "/api/v1/models/12",
            true,
        ),
        (
            "civitai",
            "user",
            "alice",
            serde_json::json!({"items":[{"id":12,"modelVersions":[{"id":34,"files":[{"id":56,"name":"tiny.safetensors","downloadUrl":"https://civitai.com/api/download/models/34","hashes":{"SHA256":"0".repeat(64)}}]}]}],"metadata":{}}),
            "/api/v1/models?username=alice&sort=Newest&limit=10&page=1&nsfw=false",
            true,
        ),
        (
            "civitai",
            "search",
            "dark lora",
            serde_json::json!({"items":[{"id":12,"name":"Dark","type":"LORA","nsfw":true,"modelVersions":[{"id":34,"name":"v1","availability":"EarlyAccess","baseModel":"LTXV 2.3","publishedAt":"2026-01-02T00:00:00Z","images":[{"type":"video","url":"https://image.civitai.com/x/original=true/0.mp4"},{"type":"image","url":"https://image.civitai.com/x/original=true/1.jpeg"}],"files":[]}]}],"metadata":{}}),
            "/api/v1/models?query=dark+lora&limit=20&sort=Newest&nsfw=false",
            false,
        ),
    ];
    for (kind, scope, locator, body, expected, downloadable) in cases {
        let (origin, server) = fixture(serde_json::to_vec(&body).unwrap());
        let mut p = provider(origin);
        p.kind = kind.into();
        let r = BrowseRequest {
            provider_id: p.id.clone(),
            scope: scope.into(),
            locator: locator.into(),
            cursor: None,
            authenticated: false,
            nsfw: false,
        };
        let page = providers::list(&p, &r, None, None).await.unwrap();
        assert_eq!(page.resources.len(), 1);
        assert_eq!(page.resources[0].downloadable, downloadable);
        assert!(server
            .join()
            .unwrap()
            .starts_with(&format!("GET {expected} HTTP/1.1")));
        if kind == "civitai" && scope == "search" {
            assert_eq!(page.resources[0].locator.as_deref(), Some("12"));
            assert_eq!(page.resources[0].category.as_deref(), Some("loras"));
            assert_eq!(
                page.resources[0].preview.as_deref(),
                Some("https://image.civitai.com/x/width=450/1.jpeg")
            );
            assert_eq!(page.resources[0].label, "Dark · v1");
            assert!(page.resources[0].auth && page.resources[0].nsfw);
            assert_eq!(page.resources[0].base.as_deref(), Some("LTX 2.3"));
            assert_eq!(
                page.resources[0].published_at.as_deref(),
                Some("2026-01-02T00:00:00Z")
            );
        } else if kind == "civitai" {
            assert!(page.resources[0].size.is_none());
            assert_eq!(
                page.resources[0].version.as_deref(),
                (scope == "model").then_some("fp8 v2")
            );
            if scope == "model" {
                assert_eq!(
                    page.resources[0].variant.as_deref(),
                    Some("fp8 · pruned · SafeTensor · ≈13.5 GB")
                );
            }
            assert!(page.resources[0].sha256.is_some());
            assert_eq!(page.resources[0].category.as_deref(), Some("other"));
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let reply = serde_json::json!([{"type":"file","name":"tiny.bin","path":"dir/tiny.bin","size":3,"url":format!("{origin}/repos/alice/tiny/contents/dir/tiny.bin?ref=main")}]);
    let task = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let req = request(&mut s);
        let b = serde_json::to_vec(&reply).unwrap();
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            b.len()
        )
        .unwrap();
        s.write_all(&b).unwrap();
        req
    });
    let mut p = provider(origin);
    p.kind = "github".into();
    let r = BrowseRequest {
        provider_id: p.id.clone(),
        scope: "repo".into(),
        locator: "alice/tiny/dir".into(),
        cursor: None,
        authenticated: false,
        nsfw: false,
    };
    let page = providers::list(&p, &r, None, None).await.unwrap();
    assert!(page.resources[0]
        .url
        .as_deref()
        .unwrap()
        .ends_with("/contents/dir/tiny.bin"));
    assert!(task
        .join()
        .unwrap()
        .starts_with("GET /repos/alice/tiny/contents/dir "));
}
#[tokio::test]
async fn continuation_is_native_opaque_and_bound_to_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let origin2 = origin.clone();
    let task = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        request(&mut s);
        let body = b"[]";
        write!(s,"HTTP/1.1 200 OK\r\nLink: <{origin2}/users/alice/repos?per_page=50&page=2>; rel=\"next\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
        s.write_all(body).unwrap();
    });
    let mut p = provider(origin);
    p.kind = "github".into();
    p.scopes = vec!["user".into()];
    let e = DownloadEngine::default();
    e.inner.lock().unwrap().catalog = Ok(Catalog {
        version: 1,
        providers: vec![p.clone()],
        entries: vec![],
    });
    let mut r = BrowseRequest {
        provider_id: p.id,
        scope: "user".into(),
        locator: "alice".into(),
        cursor: None,
        authenticated: false,
        nsfw: false,
    };
    let page = e.browse(r.clone(), &AuthState::default()).await.unwrap();
    let cursor = page.next_cursor.unwrap();
    assert!(uuid::Uuid::parse_str(&cursor).is_ok());
    task.join().unwrap();
    r.cursor = Some(cursor);
    r.locator = "bob".into();
    assert!(e.browse(r, &AuthState::default()).await.is_err());
}
#[tokio::test]
async fn real_generic_index_and_ollama_non_downloadable() {
    let index = serde_json::json!({"version":1,"resources":[{"id":"index-file","label":"Fixture file","kind":"file","url":"https://files.example.org/tiny.bin","size":3,"sha256":"0".repeat(64),"downloadable":true,"locator":"tiny.bin","license":"fixture license"}]});
    let (origin, server) = fixture(serde_json::to_vec(&index).unwrap());
    let p = provider(origin.clone());
    let r = BrowseRequest {
        provider_id: p.id.clone(),
        scope: "index".into(),
        locator: format!("{origin}/index.json"),
        cursor: None,
        authenticated: false,
        nsfw: false,
    };
    let page = providers::list(&p, &r, None, None).await.unwrap();
    assert_eq!(
        page.resources[0].license.as_deref(),
        Some("fixture license")
    );
    server.join().unwrap();
    let (origin, server) = fixture(
        br#"{"models":[{"name":"fixture:tiny","size":3,"digest":"not-file-hash"}]}"#.to_vec(),
    );
    let mut p = provider(origin.clone());
    p.kind = "ollama".into();
    p.scopes = vec!["backend".into()];
    let r = BrowseRequest {
        provider_id: p.id.clone(),
        scope: "backend".into(),
        locator: origin,
        cursor: None,
        authenticated: false,
        nsfw: false,
    };
    let page = providers::list(&p, &r, None, None).await.unwrap();
    assert!(!page.resources[0].downloadable);
    assert!(page.resources[0].url.is_none());
    server.join().unwrap();
}

#[cfg(any(target_os = "linux", windows))]
#[test]
fn interrupted_transfer_resumes_from_the_kept_partial() {
    let bytes = b"resume fixture payload".to_vec();
    let half = bytes.len() / 2;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let served = bytes.clone();
    let server = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let req = request(&mut s);
        write!(
            s,
            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
            served.len() - half,
            half,
            served.len() - 1,
            served.len()
        )
        .unwrap();
        s.write_all(&served[half..]).unwrap();
        req
    });
    let r = resource(&origin, Some(hash), Some(bytes.len() as u64));
    let e = engine(provider(origin), &r);
    let d = scratch();
    // A previous run kept half the file; the shared model folder is where it survives.
    std::fs::write(d.path().join("deas-tiny.bin.partial"), &bytes[..half]).unwrap();
    let plan = e
        .plan(PlanRequest {
            provider_id: "fixture".into(),
            resources: vec![r],
            parent_path: d.path().to_str().unwrap().into(),
            folder_name: String::new(),
            authenticated: false,
        })
        .unwrap();
    e.start(plan.id, true, &AuthState::default()).unwrap();
    let status = finished(&e);
    assert_eq!(status.state, "succeeded", "{:?}", status.error);
    assert_eq!(status.bytes_downloaded, (bytes.len() - half) as u64);
    assert_eq!(std::fs::read(d.path().join("tiny.bin")).unwrap(), bytes);
    let req = server.join().unwrap().to_lowercase();
    assert!(req.contains(&format!("range: bytes={half}-")), "{req}");
}
#[cfg(any(target_os = "linux", windows))]
#[test]
fn server_ignoring_range_restarts_the_partial_cleanly() {
    let bytes = b"whole fixture payload".to_vec();
    let (origin, server) = fixture(bytes.clone());
    let r = resource(
        &origin,
        Some(format!("{:x}", Sha256::digest(&bytes))),
        Some(bytes.len() as u64),
    );
    let e = engine(provider(origin), &r);
    let d = scratch();
    std::fs::write(d.path().join("deas-tiny.bin.partial"), &bytes[..4]).unwrap();
    let plan = e
        .plan(PlanRequest {
            provider_id: "fixture".into(),
            resources: vec![r],
            parent_path: d.path().to_str().unwrap().into(),
            folder_name: String::new(),
            authenticated: false,
        })
        .unwrap();
    e.start(plan.id, true, &AuthState::default()).unwrap();
    let status = finished(&e);
    assert_eq!(status.state, "succeeded", "{:?}", status.error);
    assert_eq!(std::fs::read(d.path().join("tiny.bin")).unwrap(), bytes);
    server.join().unwrap();
}
#[cfg(any(target_os = "linux", windows))]
#[test]
fn changed_remote_file_fails_verification_and_retries_once_from_zero() {
    let bytes = b"changed fixture payload".to_vec();
    let half = bytes.len() / 2;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let mut wrong = bytes.clone();
    wrong[half] ^= 0xff;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let served = bytes.clone();
    let server = thread::spawn(move || {
        let mut requests = vec![];
        for (index, tail) in [&wrong[half..], &served[..]].into_iter().enumerate() {
            let (mut s, _) = listener.accept().unwrap();
            requests.push(request(&mut s));
            let (status, range) = if index == 0 {
                (
                    206,
                    format!(
                        "Content-Range: bytes {}-{}/{}\r\n",
                        half,
                        served.len() - 1,
                        served.len()
                    ),
                )
            } else {
                (200, String::new())
            };
            write!(
                s,
                "HTTP/1.1 {} X\r\nContent-Length: {}\r\n{}Connection: close\r\n\r\n",
                status,
                tail.len(),
                range
            )
            .unwrap();
            s.write_all(tail).unwrap();
        }
        requests.join("\n")
    });
    let r = resource(&origin, Some(hash), Some(bytes.len() as u64));
    let e = engine(provider(origin), &r);
    let d = scratch();
    std::fs::write(d.path().join("deas-tiny.bin.partial"), &bytes[..half]).unwrap();
    let plan = e
        .plan(PlanRequest {
            provider_id: "fixture".into(),
            resources: vec![r],
            parent_path: d.path().to_str().unwrap().into(),
            folder_name: String::new(),
            authenticated: false,
        })
        .unwrap();
    e.start(plan.id, true, &AuthState::default()).unwrap();
    let status = finished(&e);
    assert_eq!(status.state, "succeeded", "{:?}", status.error);
    assert_eq!(std::fs::read(d.path().join("tiny.bin")).unwrap(), bytes);
    let requests = server.join().unwrap().to_lowercase();
    assert!(
        requests.contains(&format!("range: bytes={half}-")),
        "{requests}"
    );
    assert_eq!(requests.matches("range: bytes=").count(), 1, "{requests}");
}

#[cfg(any(target_os = "linux", windows))]
#[test]
fn complete_partial_is_published_without_another_request() {
    let bytes = b"finished fixture payload".to_vec();
    let hash = format!("{:x}", Sha256::digest(&bytes));
    // A dropped listener refuses connections, so any request would fail the run.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let r = resource(&origin, Some(hash), Some(bytes.len() as u64));
    let e = engine(provider(origin), &r);
    let d = scratch();
    // A crash after the last byte, before publication, leaves the whole file in the partial.
    std::fs::write(d.path().join("deas-tiny.bin.partial"), &bytes).unwrap();
    let plan = e
        .plan(PlanRequest {
            provider_id: "fixture".into(),
            resources: vec![r],
            parent_path: d.path().to_str().unwrap().into(),
            folder_name: String::new(),
            authenticated: false,
        })
        .unwrap();
    e.start(plan.id, true, &AuthState::default()).unwrap();
    let status = finished(&e);
    assert_eq!(status.state, "succeeded", "{:?}", status.error);
    assert_eq!(std::fs::read(d.path().join("tiny.bin")).unwrap(), bytes);
}

/// Live CivitAI acceptance for a bundled checkpoint card; run with `cargo test -- --ignored live_civitai`.
#[tokio::test]
#[ignore]
async fn live_civitai_checkpoint_lists_selectable_files() {
    let e = DownloadEngine::default();
    e.inner.lock().unwrap().catalog = Catalog::load();
    let listing = e
        .browse(
            BrowseRequest {
                provider_id: "civitai".into(),
                scope: "model".into(),
                locator: "1981116".into(),
                cursor: None,
                authenticated: false,
                nsfw: true,
            },
            &AuthState::default(),
        )
        .await
        .unwrap();
    let files: Vec<_> = listing
        .resources
        .iter()
        .filter(|r| r.downloadable)
        .collect();
    assert!(!files.is_empty());
    assert!(files.iter().all(|r| r.version.is_some()));
    assert!(files.iter().all(|r| r.variant.is_some()));
    eprintln!(
        "{} files, first {:?} ({:?})",
        files.len(),
        files[0].label,
        files[0].version
    );
}
