use super::*;

#[test]
fn compute_reservation_excludes_other_admissions_and_releases_on_drop() {
    let first = reserve_compute().unwrap();
    assert!(reserve_compute().is_err());
    drop(first);
    assert!(reserve_compute().is_ok());
}
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn need() -> Need {
    Need {
        ram_bytes: 450,
        vram_bytes: 450,
        device_index: Some(3),
    }
}
fn memory(free: u64) -> crate::monitoring::Sample {
    crate::monitoring::Sample {
        ram: Some(crate::monitoring::Memory {
            total: 1000,
            used: 1000 - free,
        }),
        gpus: vec![crate::monitoring::Gpu {
            index: 3,
            name: "fixture".into(),
            utilization: None,
            memory_used: Some(1000 - free),
            memory_total: Some(1000),
            temperature: None,
            power_w: None,
            power_limit_w: None,
        }],
        ..Default::default()
    }
}

struct Server {
    base: Url,
    posts: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn server(queue: Value, ack: u16, ack_body: &'static str) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let posts = Arc::new(AtomicUsize::new(0));
    let count = posts.clone();
    let task = tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let header_end = loop {
                let mut buffer = [0; 1024];
                let n = stream.read(&mut buffer).await.unwrap();
                assert_ne!(n, 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let headers = String::from_utf8_lossy(&bytes[..header_end]).to_string();
            let length = headers
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            while bytes.len() < header_end + length {
                let mut buffer = [0; 1024];
                let n = stream.read(&mut buffer).await.unwrap();
                assert_ne!(n, 0);
                bytes.extend_from_slice(&buffer[..n]);
            }
            let (status, body) = if headers.starts_with("POST /free ") {
                let payload: Value = serde_json::from_slice(&bytes[header_end..]).unwrap();
                assert_eq!(payload, json!({"unload_models":true,"free_memory":true}));
                count.fetch_add(1, Ordering::SeqCst);
                (ack, ack_body.to_string())
            } else {
                assert!(
                    headers.starts_with("GET /queue "),
                    "Unexpected route: {headers}"
                );
                (200, queue.to_string())
            };
            stream.write_all(format!("HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
    });
    Server { base, posts, task }
}
fn quiet() -> Value {
    json!({"queue_running":[],"queue_pending":[]})
}

#[test]
fn proportional_budget_uses_available_memory_and_exact_physical_device() {
    assert!(enough(need(), &memory(500)).unwrap());
    assert!(!enough(need(), &memory(499)).unwrap());
    let cpu = Need {
        ram_bytes: 450,
        vram_bytes: 0,
        device_index: None,
    };
    let mut sample = memory(500);
    sample.gpus.clear();
    assert!(enough(cpu, &sample).unwrap());
    assert!(enough(need(), &sample).is_err());
    assert!(enough(
        Need {
            device_index: None,
            ..need()
        },
        &memory(500)
    )
    .is_err());
    assert!(enough(need(), &crate::monitoring::Sample::default()).is_err());
    assert!(!enough(
        Need {
            ram_bytes: u64::MAX,
            ..cpu
        },
        &memory(500)
    )
    .unwrap());
}

#[tokio::test]
async fn no_pressure_never_posts() {
    let server = server(quiet(), 200, "").await;
    assert!(run(
        need(),
        &AtomicBool::new(false),
        Some(&server.base),
        &|_| Ok(7),
        || std::future::ready(memory(500)),
        Duration::ZERO
    )
    .await
    .is_ok());
    assert_eq!(server.posts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn only_owned_idle_release_with_exact_payload_and_measured_recovery() {
    let server = server(quiet(), 200, "").await;
    let mut samples = 0;
    let result = run(
        need(),
        &AtomicBool::new(false),
        Some(&server.base),
        &|_| Ok(7),
        || {
            samples += 1;
            std::future::ready(memory(if samples < 4 { 100 } else { 500 }))
        },
        Duration::from_secs(2),
    )
    .await;
    assert!(result.is_ok(), "{result:?}");
    assert!(samples >= 4);
    assert_eq!(server.posts.load(Ordering::SeqCst), 1);
    let entries = crate::logs::after(0);
    assert!(entries
        .iter()
        .any(|entry| entry.source == crate::logs::Source::Deas
            && entry.text.starts_with("resources info release:")));
}

#[tokio::test]
async fn running_pending_or_malformed_queue_never_posts_even_without_pressure() {
    for queue in [
        json!({"queue_running":[["foreign"]],"queue_pending":[]}),
        json!({"queue_running":[],"queue_pending":[["foreign"]]}),
        json!({}),
    ] {
        let server = server(queue, 200, "").await;
        for free in [100, 500] {
            assert!(run(
                need(),
                &AtomicBool::new(false),
                Some(&server.base),
                &|_| Ok(1),
                || std::future::ready(memory(free)),
                Duration::ZERO
            )
            .await
            .is_err());
        }
        assert_eq!(server.posts.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn cancelled_or_foreign_owner_never_posts() {
    let server = server(quiet(), 200, "").await;
    let result = run(
        need(),
        &AtomicBool::new(true),
        Some(&server.base),
        &|_| Ok(1),
        || std::future::ready(memory(100)),
        Duration::ZERO,
    )
    .await;
    assert!(result.unwrap_err().starts_with("RESOURCE_CANCELLED"));
    assert!(run(
        need(),
        &AtomicBool::new(false),
        Some(&server.base),
        &|_| Err("foreign".into()),
        || std::future::ready(memory(100)),
        Duration::ZERO
    )
    .await
    .is_err());
    assert_eq!(server.posts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn stale_lifecycle_never_posts() {
    let server = server(quiet(), 200, "").await;
    let calls = AtomicUsize::new(0);
    let owner = |_: &Url| Ok(calls.fetch_add(1, Ordering::SeqCst) as u64);
    assert!(run(
        need(),
        &AtomicBool::new(false),
        Some(&server.base),
        &owner,
        || std::future::ready(memory(100)),
        Duration::ZERO
    )
    .await
    .unwrap_err()
    .starts_with("RESOURCE_OWNER"));
    assert_eq!(server.posts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn failed_or_unexpected_ack_fails_without_retry() {
    for (status, body) in [
        (500, "secret fixture body"),
        (200, "{\"ok\":true}"),
        (204, ""),
    ] {
        let server = server(quiet(), status, body).await;
        let error = run(
            need(),
            &AtomicBool::new(false),
            Some(&server.base),
            &|_| Ok(1),
            || std::future::ready(memory(100)),
            Duration::from_millis(20),
        )
        .await
        .unwrap_err();
        assert!(error.starts_with("RESOURCE_ACK"));
        assert!(!error.contains("secret"));
        assert_eq!(server.posts.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn successful_ack_without_actual_memory_release_fails() {
    let server = server(quiet(), 200, "").await;
    let error = run(
        need(),
        &AtomicBool::new(false),
        Some(&server.base),
        &|_| Ok(1),
        || std::future::ready(memory(100)),
        Duration::from_millis(20),
    )
    .await
    .unwrap_err();
    assert!(error.starts_with("RESOURCE_INSUFFICIENT"));
    assert_eq!(server.posts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn unavailable_telemetry_never_posts() {
    let server = server(quiet(), 200, "").await;
    assert!(run(
        need(),
        &AtomicBool::new(false),
        Some(&server.base),
        &|_| Ok(1),
        || std::future::ready(crate::monitoring::Sample::default()),
        Duration::ZERO
    )
    .await
    .unwrap_err()
    .starts_with("RESOURCE_TELEMETRY"));
    assert_eq!(server.posts.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn no_owner_only_admits_already_sufficient_resources() {
    for (free, expected) in [(100, false), (500, true)] {
        assert_eq!(
            run(
                need(),
                &AtomicBool::new(false),
                None,
                &|_| panic!("No owner access"),
                || std::future::ready(memory(free)),
                Duration::ZERO
            )
            .await
            .is_ok(),
            expected
        );
    }
}

#[tokio::test]
async fn cancellation_interrupts_waiting_io() {
    let cancel = AtomicBool::new(false);
    let result = tokio::join!(
        interruptible(&cancel, std::future::pending::<Result<(), String>>()),
        async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            cancel.store(true, Ordering::Release);
        }
    );
    assert!(result.0.unwrap_err().starts_with("RESOURCE_CANCELLED"));
}
