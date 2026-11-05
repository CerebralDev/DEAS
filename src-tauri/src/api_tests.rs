use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const HEALTH: &str = r#"{"status":"healthy","upstream":"http://127.0.0.1:9"}"#;
const JOBS: &str = r#"{"jobs":[],"truncated":false}"#;
const PROMPT: &str = r#"{"exec_info":{"queue_remaining":0}}"#;

async fn serve(
    responses: Vec<(u16, &str, String)>,
) -> (String, tokio::task::JoinHandle<Vec<String>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let owned: Vec<_> = responses
        .into_iter()
        .map(|(s, c, b)| (s, c.to_owned(), b))
        .collect();
    let task = tokio::spawn(async move {
        let mut requests = vec![];
        for (status, content_type, body) in owned {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 4096];
            let n = stream.read(&mut bytes).await.unwrap();
            let request = String::from_utf8_lossy(&bytes[..n]);
            assert!(!request.to_lowercase().contains("authorization:"));
            requests.push(request.lines().next().unwrap().to_owned());
            let headers = format!(
                "HTTP/1.1 {status} Fixture\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nLocation: http://127.0.0.1:9/forbidden\r\n\r\n",
                body.len()
            );
            stream.write_all(headers.as_bytes()).await.unwrap();
            let _ = stream.write_all(body.as_bytes()).await;
        }
        requests
    });
    (url, task)
}
fn json(status: u16, body: &str) -> (u16, &'static str, String) {
    (status, "application/json", body.into())
}

#[tokio::test]
async fn prefers_v2_and_preserves_prefix_and_camel_case() {
    let (url, server) = serve(vec![json(200, HEALTH), json(200, JOBS)]).await;
    let result = native_negotiate_api(format!("{url}/comfy")).await.unwrap();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        serde_json::json!({
            "baseUrl":format!("{url}/comfy/"),"protocol":"v2","generationReady":false,"capabilities":["v2Health","v2JobListing"]
        })
    );
    assert_eq!(
        server.await.unwrap(),
        [
            "GET /comfy/api/v2/health HTTP/1.1",
            "GET /comfy/api/v2/jobs?limit=1 HTTP/1.1"
        ]
    );
}
#[tokio::test]
async fn classic_only_after_optional_404_and_valid_queue() {
    let (url, server) = serve(vec![json(404, ""), json(200, PROMPT)]).await;
    let result = native_negotiate_api(format!("{url}/backend/"))
        .await
        .unwrap();
    assert_eq!(result.protocol, "classic");
    assert!(!result.generation_ready);
    assert_eq!(result.capabilities, ["classicQueueStatus"]);
    assert_eq!(
        server.await.unwrap(),
        [
            "GET /backend/api/v2/health HTTP/1.1",
            "GET /backend/prompt HTTP/1.1"
        ]
    );
}
#[tokio::test]
async fn errors_never_fallback() {
    for (status, ctype, body, code) in [
        (401, "application/json", "{}", "authentication"),
        (403, "application/json", "{}", "authentication"),
        (302, "application/json", "{}", "redirect"),
        (500, "application/json", "{}", "http"),
        (200, "text/html", "<html>Login</html>", "html"),
        (200, "application/json", "<html>Login</html>", "html"),
        (200, "application/json", "broken", "malformed"),
        (200, "application/json", "{}", "malformed"),
        (
            200,
            "application/json",
            r#"{"status":"unhealthy","upstream":"http://host"}"#,
            "incompatible",
        ),
        (
            200,
            "application/json",
            r#"{"status":"healthy","upstream":"file:///etc/passwd"}"#,
            "incompatible",
        ),
        (200, "text/plain", HEALTH, "malformed"),
    ] {
        let (url, server) = serve(vec![(status, ctype, body.into())]).await;
        let error = native_negotiate_api(url).await.unwrap_err();
        assert_eq!(error.code, code, "{error:?}");
        assert_eq!(server.await.unwrap().len(), 1);
    }
}
#[tokio::test]
async fn protected_listing_must_pass_even_when_health_is_public() {
    for (status, body, code) in [
        (401, "{}", "authentication"),
        (404, "{}", "http"),
        (200, "{}", "malformed"),
        (200, r#"{"jobs":[],"truncated":"false"}"#, "malformed"),
        (200, r#"{"jobs":[{},{}],"truncated":false}"#, "incompatible"),
        (200, r#"{"jobs":[{}],"truncated":false}"#, "malformed"),
        (
            200,
            r#"{"jobs":[{"id":"c5c64579-e8cb-42aa-834f-ace0013706d7","status":"new-version","outputs":[]}],"truncated":false}"#,
            "incompatible",
        ),
    ] {
        let (url, server) = serve(vec![json(200, HEALTH), json(status, body)]).await;
        assert_eq!(native_negotiate_api(url).await.unwrap_err().code, code);
        assert_eq!(server.await.unwrap().len(), 2);
    }
}
#[tokio::test]
async fn nonempty_documented_job_list() {
    let (url,server) = serve(vec![json(200,HEALTH),json(200,r#"{"jobs":[{"id":"c5c64579-e8cb-42aa-834f-ace0013706d7","status":"queued","outputs":[]}],"truncated":false}"#)]).await;
    assert_eq!(native_negotiate_api(url).await.unwrap().protocol, "v2");
    server.await.unwrap();
}
#[tokio::test]
async fn rejects_malformed_classic_status() {
    for body in [
        "{}",
        r#"{"exec_info":{"queue_remaining":-1}}"#,
        r#"{"exec_info":{"queue_remaining":1.5}}"#,
        r#"{"exec_info":{"queue_remaining":"0"}}"#,
    ] {
        let (url, server) = serve(vec![json(404, ""), json(200, body)]).await;
        assert_eq!(
            native_negotiate_api(url).await.unwrap_err().code,
            "malformed"
        );
        server.await.unwrap();
    }
}
#[tokio::test]
async fn rejects_invalid_explicit_endpoints_without_network() {
    for input in [
        "",
        " ",
        "not-url",
        "file:///etc/passwd",
        "ftp://host",
        "http://@host",
        "http://user:pass@host",
        "http://host?token=x",
        "http://host/#x",
    ] {
        assert_eq!(
            native_negotiate_api(input.into()).await.unwrap_err().code,
            "invalidEndpoint"
        );
    }
}
#[tokio::test]
async fn caps_content_length_and_chunked_body() {
    let (url, server) = serve(vec![json(200, &" ".repeat(MAX_PROBE_BYTES + 1))]).await;
    assert_eq!(
        native_negotiate_api(url).await.unwrap_err().code,
        "responseTooLarge"
    );
    server.await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = [0; 4096];
        assert!(stream.read(&mut buf).await.unwrap() > 0);
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").await.unwrap();
        let chunk = vec![b' '; 32768];
        for _ in 0..3 {
            if stream.write_all(b"8000\r\n").await.is_err() {
                break;
            }
            if stream.write_all(&chunk).await.is_err() {
                break;
            }
            if stream.write_all(b"\r\n").await.is_err() {
                break;
            }
        }
        let _ = stream.write_all(b"0\r\n\r\n").await;
    });
    assert_eq!(
        native_negotiate_api(url).await.unwrap_err().code,
        "responseTooLarge"
    );
    server.await.unwrap();
}
#[tokio::test]
async fn bounds_stalled_http_response() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(20)).await;
    });
    let start = std::time::Instant::now();
    assert_eq!(native_negotiate_api(url).await.unwrap_err().code, "timeout");
    assert!(start.elapsed() < Duration::from_secs(8));
    server.abort();
}
