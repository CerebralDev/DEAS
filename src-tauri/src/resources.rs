//! Native admission for dependent work. Caller holds Jobs' serial pipeline guard through execution.
//! `/free` only acknowledges flags: main.py's idle worker resets caches/GC later; telemetry is proof.
use reqwest::{Client, Url};
use serde_json::{json, Value};
use std::{
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Need {
    pub ram_bytes: u64,
    pub vram_bytes: u64,
    /// Physical telemetry index, not CUDA's remapped visible index.
    pub device_index: Option<u32>,
}

static CONTEXT: Mutex<Option<crate::install::Engine>> = Mutex::new(None);
static COMPUTE: std::sync::LazyLock<std::sync::Arc<tokio::sync::Semaphore>> =
    std::sync::LazyLock::new(|| std::sync::Arc::new(tokio::sync::Semaphore::new(1)));

/// Hold through native admission + inference, or backend admission + submit response.
/// Never acquire this gate for Stop/cancel; busy callers defer rather than block a runtime.
pub(crate) fn reserve_compute() -> Result<tokio::sync::OwnedSemaphorePermit, String> {
    COMPUTE.clone().try_acquire_owned().map_err(|_| {
        "RESOURCE_BUSY: Another native compute stage is active; retry after it finishes".into()
    })
}

/// Native authority for admissions: only the approved owned environment may release memory.
pub(crate) fn owned_venv_python() -> Option<std::path::PathBuf> {
    let engine = CONTEXT.lock().ok()?.clone()?;
    let plan = engine.approved_plan()?;
    let python = std::path::PathBuf::from(plan.request.storage_path).join(if cfg!(windows) {
        "venv/Scripts/python.exe"
    } else {
        "venv/bin/python"
    });
    python.is_file().then_some(python)
}

/// Binds the shared native Engine authority for memory admissions (called once at startup).
pub fn bind_resource_authority(engine: crate::install::Engine) {
    configure_context(engine);
}

/// Bind native authority, never a frontend URL. Rebinding affects only subsequent admissions.
pub(crate) fn configure_context(engine: crate::install::Engine) {
    *CONTEXT.lock().unwrap_or_else(|e| e.into_inner()) = Some(engine);
}

/// Blocking-worker entry point; never call from a Tokio runtime thread.
pub(crate) fn admit(need: Need, cancel: &AtomicBool) -> Result<(), String> {
    let result = (|| {
        cancelled(cancel)?;
        let engine = CONTEXT
            .lock()
            .map_err(|_| "RESOURCE_CONTEXT: Native context unavailable")?
            .clone();
        let base = engine
            .as_ref()
            .and_then(|e| {
                let status = e.backend.status();
                (status.state == "running")
                    .then_some(status.endpoint)
                    .flatten()
            })
            .map(|s| Url::parse(&s).map_err(|_| "RESOURCE_OWNER: Invalid native endpoint"))
            .transpose()?;
        let owner = |base: &Url| {
            let engine = engine
                .as_ref()
                .ok_or("RESOURCE_OWNER: No owned backend configured")?;
            let inner = crate::install::lock(&engine.inner);
            let approved = inner
                .approved
                .as_ref()
                .ok_or("RESOURCE_OWNER: No approved environment")?;
            approved
                .validate()
                .map_err(|_| "RESOURCE_OWNER: Approved environment changed")?;
            drop(inner);
            engine.backend.cancellation_owner()(base)
                .map_err(|_| "RESOURCE_OWNER: Owned endpoint or lifecycle changed".into())
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| "RESOURCE_RUNTIME: Cannot create admission runtime")?;
        runtime.block_on(interruptible(cancel, async {
            run(
                need,
                cancel,
                base.as_ref(),
                &owner,
                || crate::monitoring::sample(None),
                Duration::from_secs(20),
            )
            .await
        }))
    })();
    let message = match &result {
        Ok(()) => "resources info admission: available RAM/VRAM budget satisfied",
        Err(_) if cancel.load(Ordering::Acquire) => "resources info admission: cancelled",
        Err(_) => "resources warn admission: deferred; resource budget, telemetry or owned-idle proof unavailable",
    };
    // No endpoint, remote body, path or transport error reaches the shared log.
    crate::logs::push(crate::logs::Source::Deas, message);
    result
}

fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        Err("RESOURCE_CANCELLED: Admission cancelled".into())
    } else {
        Ok(())
    }
}

async fn interruptible<T>(
    cancel: &AtomicBool,
    future: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::pin!(future);
    loop {
        cancelled(cancel)?;
        tokio::select! {
            result = &mut future => { cancelled(cancel)?; return result; }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
    }
}

fn enough(need: Need, sample: &crate::monitoring::Sample) -> Result<bool, String> {
    // Spend at most 90% of currently available memory; no absolute host/device caps.
    let fits = |required: u64, total: u64, used: u64| -> Result<bool, String> {
        if total == 0 || used > total {
            return Err("RESOURCE_TELEMETRY: Invalid memory sample".into());
        }
        let available = total - used;
        Ok(u128::from(required) * 10 <= u128::from(available) * 9)
    };
    let ram_ok = if need.ram_bytes == 0 {
        true
    } else {
        let ram = sample
            .ram
            .as_ref()
            .ok_or("RESOURCE_TELEMETRY: RAM availability unavailable")?;
        fits(need.ram_bytes, ram.total, ram.used)?
    };
    let gpu_ok = if need.vram_bytes == 0 {
        true
    } else {
        let index = need
            .device_index
            .ok_or("RESOURCE_DEVICE: A physical GPU index is required")?;
        let gpu = sample
            .gpus
            .iter()
            .find(|gpu| gpu.index == index)
            .ok_or("RESOURCE_TELEMETRY: Selected GPU unavailable")?;
        fits(
            need.vram_bytes,
            gpu.memory_total
                .ok_or("RESOURCE_TELEMETRY: VRAM total unavailable")?,
            gpu.memory_used
                .ok_or("RESOURCE_TELEMETRY: VRAM usage unavailable")?,
        )?
    };
    Ok(ram_ok && gpu_ok)
}

fn same_owner(
    base: &Url,
    owner: &impl Fn(&Url) -> Result<u64, String>,
    epoch: u64,
) -> Result<(), String> {
    if owner(base)? != epoch {
        return Err("RESOURCE_OWNER: Owned lifecycle changed".into());
    }
    Ok(())
}

async fn body(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "RESOURCE_HTTP: Response read failed")?
    {
        if bytes.len().saturating_add(chunk.len()) > 65536 {
            return Err("RESOURCE_HTTP: Response exceeded limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn idle(
    client: &Client,
    base: &Url,
    owner: &impl Fn(&Url) -> Result<u64, String>,
    epoch: u64,
) -> Result<(), String> {
    same_owner(base, owner, epoch)?;
    let response = client
        .get(
            base.join("queue")
                .map_err(|_| "RESOURCE_HTTP: Invalid route")?,
        )
        .send()
        .await
        .map_err(|_| "RESOURCE_HTTP: Queue probe failed")?;
    if response.status() != reqwest::StatusCode::OK {
        return Err("RESOURCE_HTTP: Queue probe refused".into());
    }
    let queue: Value = serde_json::from_slice(&body(response).await?)
        .map_err(|_| "RESOURCE_QUEUE: Invalid queue response")?;
    same_owner(base, owner, epoch)?;
    for key in ["queue_running", "queue_pending"] {
        let entries = queue[key]
            .as_array()
            .ok_or("RESOURCE_QUEUE: Missing queue state")?;
        if !entries.is_empty() {
            return Err(
                "RESOURCE_BUSY: Backend has running or pending work; retry after it finishes"
                    .into(),
            );
        }
    }
    Ok(())
}

async fn run<F, S>(
    need: Need,
    cancel: &AtomicBool,
    base: Option<&Url>,
    owner: &impl Fn(&Url) -> Result<u64, String>,
    mut sample: S,
    wait: Duration,
) -> Result<(), String>
where
    F: Future<Output = crate::monitoring::Sample>,
    S: FnMut() -> F,
{
    cancelled(cancel)?;
    let available = enough(need, &sample().await)?;
    cancelled(cancel)?;
    let Some(base) = base else {
        return if available {
            Ok(())
        } else {
            Err("RESOURCE_INSUFFICIENT: Insufficient available memory and no running owned backend to release".into())
        };
    };
    // Defense in depth even though production derives the URL solely from Engine.
    if base.scheme() != "http"
        || base.host_str() != Some("127.0.0.1")
        || base.path() != "/"
        || !base.username().is_empty()
        || base.password().is_some()
        || base.query().is_some()
        || base.fragment().is_some()
    {
        return Err("RESOURCE_OWNER: Expected native owned loopback endpoint".into());
    }
    let epoch = owner(base)?;
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|_| "RESOURCE_HTTP: Cannot create client")?;
    idle(&client, base, owner, epoch).await?;
    if available {
        return Ok(());
    }
    // Remeasure after the idle probe: another process may already have released memory.
    if enough(need, &sample().await)? {
        cancelled(cancel)?;
        return Ok(());
    }
    idle(&client, base, owner, epoch).await?;
    cancelled(cancel)?;
    same_owner(base, owner, epoch)?;
    let response = client
        .post(
            base.join("free")
                .map_err(|_| "RESOURCE_HTTP: Invalid route")?,
        )
        .json(&json!({"unload_models": true, "free_memory": true}))
        .send()
        .await
        .map_err(|_| "RESOURCE_HTTP: Release request failed; not retried")?;
    // Upstream server.py returns an empty HTTP 200: this is acknowledgement, not completion.
    if response.status() != reqwest::StatusCode::OK || !body(response).await?.is_empty() {
        return Err("RESOURCE_ACK: Backend did not acknowledge memory release".into());
    }
    same_owner(base, owner, epoch)?;
    crate::logs::push(
        crate::logs::Source::Deas,
        "resources info release: owned idle backend acknowledged; waiting for measured memory",
    );
    let deadline = tokio::time::Instant::now() + wait;
    let verify = async {
        loop {
            cancelled(cancel)?;
            idle(&client, base, owner, epoch).await?;
            let available = enough(need, &sample().await)?;
            same_owner(base, owner, epoch)?;
            if available {
                cancelled(cancel)?;
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    };
    tokio::time::timeout_at(deadline, verify).await
        .map_err(|_| "RESOURCE_INSUFFICIENT: Release acknowledged but measured available RAM/VRAM remains below the 90% safety budget".to_string())?
}

#[cfg(test)]
#[path = "resources_tests.rs"]
mod tests;
