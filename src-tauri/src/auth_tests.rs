use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[derive(Clone, Default)]
struct FakeStore {
    values: Arc<Mutex<BTreeMap<String, String>>>,
    fail: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
}
impl CredentialStore for FakeStore {
    fn read(&self, account: &str) -> Result<Option<String>, ()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return Err(());
        }
        Ok(self.values.lock().unwrap().get(account).cloned())
    }
    fn write(&self, account: &str, token: &str) -> Result<(), ()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return Err(());
        }
        self.values
            .lock()
            .unwrap()
            .insert(account.into(), token.into());
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<(), ()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return Err(());
        }
        self.values.lock().unwrap().remove(account);
        Ok(())
    }
}
fn fixture() -> (AuthState, FakeStore) {
    let store = FakeStore::default();
    (AuthState::with_store(Box::new(store.clone())), store)
}
#[test]
fn session_token_is_scoped_and_metadata_only() {
    let (state, _) = fixture();
    assert_eq!(
        state
            .credential("huggingface", "https://huggingface.co")
            .unwrap(),
        None
    );
    let status = state
        .set("huggingface", "fixture-token".into(), false)
        .unwrap();
    assert!(status.configured);
    assert!(!status.persisted);
    assert_eq!(
        state
            .credential("huggingface", "https://huggingface.co")
            .unwrap()
            .as_deref(),
        Some("fixture-token")
    );
    assert!(
        state
            .credential("huggingface", "https://evil.example")
            .is_err()
    );
    let json = serde_json::to_value(state.status().unwrap()).unwrap();
    for item in json.as_array().unwrap() {
        assert_eq!(item.as_object().unwrap().len(), 3);
        assert!(item.get("providerId").is_some());
    }
    assert!(!json.to_string().contains("fixture-token"));
    state.remove("huggingface").unwrap();
    assert_eq!(
        state
            .credential("huggingface", "https://huggingface.co")
            .unwrap(),
        None
    );
}
#[test]
fn anonymous_defaults_never_access_vault() {
    let (state, store) = fixture();
    for (id, origin) in PROVIDERS {
        assert!(state.credential(id, origin).unwrap().is_none());
    }
    assert!(
        state
            .status()
            .unwrap()
            .iter()
            .all(|s| !s.configured && !s.persisted)
    );
    state.set("github", "session-only".into(), false).unwrap();
    assert_eq!(store.calls.load(Ordering::SeqCst), 0);
    assert!(store.values.lock().unwrap().is_empty());
    let default = AuthState::default(); // No vault access: only memory methods.
    assert!(default.status().unwrap().iter().all(|s| !s.configured));
}
#[test]
fn persisted_credentials_are_activated_and_deleted_across_restart() {
    let (state, store) = fixture();
    assert!(
        state
            .set("github", "saved-fixture".into(), true)
            .unwrap()
            .persisted
    );
    drop(state);
    let restarted = AuthState::with_store(Box::new(store.clone())).restore_saved();
    assert!(
        restarted
            .status()
            .unwrap()
            .iter()
            .any(|s| s.provider_id == "github" && s.configured && s.persisted)
    );
    assert_eq!(
        restarted
            .credential("github", "https://api.github.com")
            .unwrap()
            .as_deref(),
        Some("saved-fixture")
    );
    restarted.remove("github").unwrap();
    assert!(!restarted.restore("github").unwrap().configured);
    assert!(store.values.lock().unwrap().is_empty());
    restarted.remove("github").unwrap();
}
#[test]
fn startup_restore_is_scoped_and_survives_unavailable_or_invalid_vault() {
    let store = FakeStore::default();
    store
        .values
        .lock()
        .unwrap()
        .insert(account("github", "https://evil.example"), "foreign".into());
    store.values.lock().unwrap().insert(
        account("civitai", "https://civitai.com"),
        "bad\nsecret".into(),
    );
    store.values.lock().unwrap().insert(
        account("huggingface", "https://huggingface.co"),
        "saved".into(),
    );
    let state = AuthState::with_store(Box::new(store.clone())).restore_saved();
    assert_eq!(
        state
            .credential("huggingface", "https://huggingface.co")
            .unwrap()
            .as_deref(),
        Some("saved")
    );
    assert!(
        state
            .credential("github", "https://api.github.com")
            .unwrap()
            .is_none()
    );
    assert!(
        state
            .credential("civitai", "https://civitai.com")
            .unwrap()
            .is_none()
    );
    store.fail.store(true, Ordering::SeqCst);
    let unavailable = AuthState::with_store(Box::new(store.clone())).restore_saved();
    assert!(unavailable.status().unwrap().iter().all(|s| !s.configured));
    store.fail.store(false, Ordering::SeqCst);
    assert!(unavailable.restore("huggingface").unwrap().configured);
}
#[test]
fn failed_vault_writes_and_removals_preserve_existing_memory() {
    let (state, store) = fixture();
    state.set("civitai", "old-fixture".into(), true).unwrap();
    store.fail.store(true, Ordering::SeqCst);
    assert!(state.set("civitai", "new-fixture".into(), true).is_err());
    assert!(
        state
            .set("civitai", "session-fixture".into(), false)
            .is_err()
    );
    assert!(state.remove("civitai").is_err());
    assert!(state.restore("civitai").is_err());
    assert_eq!(
        state
            .credential("civitai", "https://civitai.com")
            .unwrap()
            .as_deref(),
        Some("old-fixture")
    );
    assert!(
        state
            .status()
            .unwrap()
            .iter()
            .find(|s| s.provider_id == "civitai")
            .unwrap()
            .persisted
    );
}
#[test]
fn explicit_downgrade_deletes_saved_credential() {
    let (state, store) = fixture();
    state.set("github", "saved-fixture".into(), true).unwrap();
    assert!(
        !state
            .set("github", "session-fixture".into(), false)
            .unwrap()
            .persisted
    );
    assert!(store.values.lock().unwrap().is_empty());
}
#[test]
fn invalid_input_is_bounded_and_never_echoed() {
    let (state, store) = fixture();
    for token in [
        "".to_owned(),
        "x".repeat(8193),
        "secret\nvalue".into(),
        "secret value".into(),
        "secret\tvalue".into(),
        "secreté".into(),
    ] {
        let error = state.set("github", token.clone(), true).unwrap_err();
        if !token.is_empty() {
            assert!(!error.contains(&token));
        }
    }
    assert!(
        state
            .set("unknown-secret-id", "fixture".into(), false)
            .unwrap_err()
            .find("unknown-secret-id")
            .is_none()
    );
    assert!(state.remove("unknown").is_err());
    assert!(state.restore("unknown").is_err());
    assert_eq!(store.calls.load(Ordering::SeqCst), 0);
    state.set("github", "x".repeat(8192), false).unwrap();
}
#[test]
fn exact_origin_excludes_paths_ports_redirects_and_userinfo() {
    let (state, _) = fixture();
    state.set("huggingface", "fixture".into(), false).unwrap();
    for origin in [
        "https://huggingface.co/",
        "https://huggingface.co:443",
        "https://HUGGINGFACE.co",
        "http://huggingface.co",
        "https://huggingface.co:8443",
        "https://huggingface.co/path",
        "https://huggingface.co.evil",
        "https://user@huggingface.co",
    ] {
        assert!(state.credential("huggingface", origin).is_err());
    }
    for target in [
        "https://evil.example/file",
        "http://huggingface.co/file",
        "https://user@huggingface.co/file",
    ] {
        assert!(
            state
                .authorization("huggingface", &Url::parse(target).unwrap())
                .unwrap()
                .is_none()
        );
    }
    let header = state
        .authorization(
            "huggingface",
            &Url::parse("https://huggingface.co/file").unwrap(),
        )
        .unwrap()
        .unwrap();
    assert!(header.is_sensitive());
    assert!(!format!("{header:?}").contains("fixture"));
}
#[test]
fn custom_accounts_are_explicit_immutable_and_separate_vault_keys() {
    let (state, store) = fixture();
    for origin in [
        "http://example.com",
        "https://user@example.com",
        "https://example.com/",
        "https://example.com?q=1",
        " https://example.com",
        "https://example.com:443",
        "https://example.com/path",
        "https://@example.com",
    ] {
        assert!(
            state
                .register_generic_provider("custom-invalid", origin)
                .is_err()
        );
    }
    assert!(
        state
            .register_generic_provider("github", "https://evil.example")
            .is_err()
    );
    state
        .register_generic_provider("custom-ollama", "http://localhost:11434")
        .unwrap();
    state
        .register_generic_provider("custom-ollama", "http://localhost:11434")
        .unwrap();
    assert!(
        state
            .register_generic_provider("custom-ollama", "http://127.0.0.1:11434")
            .is_err()
    );
    state
        .set("custom-ollama", "fixture-custom".into(), true)
        .unwrap();
    let next = AuthState::with_store(Box::new(store));
    next.register_generic_provider("custom-ollama", "http://127.0.0.1:11434")
        .unwrap();
    assert!(!next.restore("custom-ollama").unwrap().configured);
    assert!(
        next.credential("custom-ollama", "http://localhost:11434")
            .is_err()
    );
}
#[test]
fn session_removal_does_not_require_os_vault() {
    let (state, store) = fixture();
    state
        .set("github", "session-fixture".into(), false)
        .unwrap();
    store.fail.store(true, Ordering::SeqCst);
    assert!(!state.remove("github").unwrap().configured);
    assert_eq!(store.calls.load(Ordering::SeqCst), 0);
}
#[test]
fn restore_missing_entry_clears_stale_persisted_metadata() {
    let (state, store) = fixture();
    state.set("github", "saved-fixture".into(), true).unwrap();
    store.values.lock().unwrap().clear();
    let status = state.restore("github").unwrap();
    assert!(!status.configured);
    assert!(!status.persisted);
}
#[test]
fn invalid_vault_content_never_enters_memory_or_errors() {
    let (state, store) = fixture();
    store.values.lock().unwrap().insert(
        account("github", "https://api.github.com"),
        "fixture\nsecret".into(),
    );
    let error = state.restore("github").unwrap_err();
    assert!(!error.contains("fixture"));
    assert!(
        state
            .credential("github", "https://api.github.com")
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn identity_probe_never_confuses_transport_or_public_responses_with_valid_auth() {
    for (status, content_type, body, expected) in [
        (
            401,
            "application/json",
            r#"{"error":"fixture-secret"}"#,
            "rejected",
        ),
        (403, "application/json", "{}", "unavailable"),
        (429, "application/json", "{}", "unavailable"),
        (503, "application/json", "{}", "unavailable"),
        (
            302,
            "application/json",
            r#"{"id":1,"login":"fixture"}"#,
            "unavailable",
        ),
        (
            200,
            "text/html",
            r#"{"id":1,"login":"fixture"}"#,
            "unavailable",
        ),
        (
            200,
            "application/json",
            r#"{"items":[],"metadata":{}}"#,
            "unavailable",
        ),
        (
            200,
            "application/json",
            r#"{"id":0,"login":"fixture"}"#,
            "unavailable",
        ),
        (
            200,
            "application/json",
            r#"{"id":1,"login":""}"#,
            "unavailable",
        ),
        (
            200,
            "application/json",
            "not-json fixture-secret",
            "unavailable",
        ),
    ] {
        let (url, server) = identity_fixture(status, content_type, body).await;
        let result = probe_identity(
            "github",
            url,
            HeaderValue::from_static("Bearer fixture-secret"),
        )
        .await;
        assert_eq!(result, expected);
        assert!(!result.contains("fixture-secret"));
        server.await.unwrap();
    }
    let huge = "x".repeat(65537);
    let (url, server) = identity_fixture(200, "application/json", &huge).await;
    assert_eq!(
        probe_identity(
            "github",
            url,
            HeaderValue::from_static("Bearer fixture-secret")
        )
        .await,
        "unavailable"
    );
    server.await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    drop(listener);
    assert_eq!(
        probe_identity(
            "github",
            url,
            HeaderValue::from_static("Bearer fixture-secret")
        )
        .await,
        "unavailable"
    );
}

#[tokio::test]
async fn identity_probe_bounds_streamed_body_and_stalled_response() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        let n = stream.read(&mut request).await.unwrap();
        assert!(n > 0);
        let body = "x".repeat(65537);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.ok();
    });
    assert_eq!(
        probe_identity(
            "github",
            url,
            HeaderValue::from_static("Bearer fixture-secret")
        )
        .await,
        "unavailable"
    );
    server.await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let start = std::time::Instant::now();
    assert_eq!(
        probe_identity(
            "github",
            url,
            HeaderValue::from_static("Bearer fixture-secret")
        )
        .await,
        "unavailable"
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(10));
}

async fn identity_fixture(
    status: u16,
    content_type: &str,
    body: &str,
) -> (Url, tokio::task::JoinHandle<String>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!(
        "http://{}/identity",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let response = format!(
        "HTTP/1.1 {status} Fixture\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nLocation: http://127.0.0.1:9/forbidden\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = vec![0; 16384];
        let n = stream.read(&mut bytes).await.unwrap();
        stream.write_all(response.as_bytes()).await.ok();
        String::from_utf8_lossy(&bytes[..n]).to_string()
    });
    (url, task)
}
#[tokio::test]
async fn identity_probe_confirms_each_provider_only_from_authenticated_identity_schema() {
    for (provider, body) in [
        ("civitai", r#"{"id":1,"username":"fixture"}"#),
        ("github", r#"{"id":1,"login":"fixture"}"#),
        (
            "huggingface",
            r#"{"id":"abc","name":"fixture","type":"user"}"#,
        ),
    ] {
        let (url, server) = identity_fixture(200, "application/json", body).await;
        let mut header = HeaderValue::from_static("Bearer fixture-secret");
        header.set_sensitive(true);
        assert_eq!(probe_identity(provider, url, header).await, "verified");
        let request = server.await.unwrap().to_lowercase();
        assert!(request.starts_with("get /identity http/1.1"));
        assert!(request.contains("authorization: bearer fixture-secret"));
        assert!(!request.contains("cookie:"));
    }
}
