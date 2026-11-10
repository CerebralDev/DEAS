//! App-owned sequential metadata scans; dropping the invoking webview does not cancel work.
use super::*;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanScope {
    All,
    Models,
    Loras,
}
impl ScanScope {
    fn includes(self, model: &LocalModel) -> bool {
        match self {
            Self::All => true,
            Self::Models => model.category != "loras",
            Self::Loras => model.category == "loras",
        }
    }
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelScanStatus {
    pub id: String,
    pub root: String,
    pub state: String,
    pub file: Option<String>,
    pub completed: usize,
    pub total: usize,
    pub remaining: usize,
    pub failed: usize,
    pub matched: usize,
    pub unmatched: usize,
    pub cancel_requested: bool,
    pub outcomes: Vec<FileOutcome>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileOutcome {
    pub path: String,
    pub state: String,
}

#[derive(Clone)]
pub struct ModelScanEngine(Arc<Mutex<ModelScanStatus>>, Arc<Mutex<Option<PathBuf>>>);
impl Default for ModelScanEngine {
    fn default() -> Self {
        Self(
            Arc::new(Mutex::new(ModelScanStatus {
                state: "idle".into(),
                ..Default::default()
            })),
            Arc::new(Mutex::new(None)),
        )
    }
}
impl ModelScanEngine {
    fn cancel(&self, id: &str) -> Result<ModelScanStatus, String> {
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if s.id != id || s.state != "running" {
            return Err("That metadata refresh is not running".into());
        }
        s.cancel_requested = true;
        Ok(s.clone())
    }
    pub fn status(&self) -> ModelScanStatus {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn update(&self, change: impl FnOnce(&mut ModelScanStatus)) -> ModelScanStatus {
        let mut status = self.0.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut status);
        status.remaining = status.total.saturating_sub(status.completed);
        status.clone()
    }

    fn reserve(&self, root: String) -> Result<ModelScanStatus, String> {
        let mut status = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if status.state == "running" {
            return Err("A Library file scan is already running".into());
        }
        *status = ModelScanStatus {
            id: uuid::Uuid::new_v4().to_string(),
            root,
            state: "running".into(),
            ..Default::default()
        };
        Ok(status.clone())
    }

    fn start(
        &self,
        root: String,
        scope: ScanScope,
        provider: Provider,
        header: Option<HeaderValue>,
    ) -> Result<ModelScanStatus, String> {
        self.start_job(root, scope, provider, header, None)
    }

    fn start_job(
        &self,
        root: String,
        scope: ScanScope,
        provider: Provider,
        header: Option<HeaderValue>,
        paths: Option<Vec<String>>,
    ) -> Result<ModelScanStatus, String> {
        if paths.as_ref().is_some_and(|p| {
            p.is_empty()
                || p.len() > 5000
                || p.iter().any(|p| {
                    !Path::new(p).is_absolute()
                        || p.len() > 32768
                        || p.chars().any(char::is_control)
                })
        }) {
            return Err("Select 1–5000 absolute model file identities".into());
        }
        if paths.is_none()
            && (!Path::new(&root).is_absolute()
                || root.len() > 32768
                || root.chars().any(char::is_control))
        {
            return Err("Models folder must be an absolute path without control characters".into());
        }
        let initial = self.reserve(root.clone())?;
        report(&initial, "INFO", "started");
        let worker = self.clone();
        let task =
            tokio::spawn(
                async move { worker.run_paths(root, scope, provider, header, paths).await },
            );
        let owner = self.clone();
        tokio::spawn(async move {
            let outcome = task.await;
            let failed = !matches!(outcome, Ok(Ok(())));
            let status = owner.update(|s| {
                s.state = if s.cancel_requested {
                    "cancelled"
                } else if failed || s.failed > 0 {
                    "failed"
                } else {
                    "succeeded"
                }
                .into();
            });
            report(
                &status,
                if status.state == "failed" {
                    "ERROR"
                } else {
                    "INFO"
                },
                if failed {
                    "scan failed; check models folder access"
                } else {
                    "finished"
                },
            );
        });
        Ok(initial)
    }

    async fn run_paths(
        &self,
        root: String,
        scope: ScanScope,
        provider: Provider,
        header: Option<HeaderValue>,
        paths: Option<Vec<String>>,
    ) -> Result<(), String> {
        let paths = if let Some(paths) = paths {
            let mut seen = std::collections::HashSet::new();
            paths
                .into_iter()
                .filter(|p| seen.insert(p.clone()))
                .collect::<Vec<_>>()
        } else {
            let models = tokio::task::spawn_blocking(move || scan(Path::new(&root)))
                .await
                .map_err(|_| "Model listing task failed")??;
            models
                .into_iter()
                .filter(|m| scope.includes(m))
                .map(|m| m.path)
                .collect::<Vec<_>>()
        };
        let status = self.update(|s| s.total = paths.len());
        report(&status, "INFO", "listed");
        for path in paths {
            if self.status().cancel_requested {
                break;
            }
            let status = self.update(|s| s.file = Some(path.clone()));
            report(&status, "INFO", "processing");
            // Reuse the verified-handle hash/cache/publication path, one request at a time.
            let cache = self.1.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let outcome = if let Some(cache) = cache {
                assets::enrich(&provider, PathBuf::from(&path), header.clone(), cache).await
            } else {
                fetch(&provider, PathBuf::from(&path), header.clone()).await
            };
            let failed = outcome.is_err();
            let matched = outcome
                .as_ref()
                .is_ok_and(|m| m.info.as_ref().is_some_and(|i| i.civitai.is_some()));
            let status = self.update(|s| {
                s.completed += 1;
                s.failed += usize::from(failed);
                s.matched += usize::from(matched);
                s.unmatched += usize::from(!failed && !matched);
                s.outcomes.push(FileOutcome {
                    path: path.clone(),
                    state: if failed {
                        "failed"
                    } else if matched {
                        "matched"
                    } else {
                        "unmatched"
                    }
                    .into(),
                });
            });
            // Do not log provider response bodies: they can contain credentials or arbitrary data.
            report(
                &status,
                if failed { "ERROR" } else { "INFO" },
                if failed {
                    "metadata lookup/cache failed; check file access and CivitAI availability"
                } else {
                    "processed"
                },
            );
        }
        Ok(())
    }
}

fn report(status: &ModelScanStatus, level: &str, operation: &str) {
    let text = format!(
        "{level}: Library operation=scan job={} {operation} file={:?} completed={} total={} remaining={} failed={}",
        status.id,
        status.file.as_deref().unwrap_or(""),
        status.completed,
        status.total,
        status.remaining,
        status.failed
    );
    eprintln!("{text}");
    crate::logs::push(crate::logs::Source::Deas, &text);
}

#[tauri::command]
pub fn native_model_scan_status(scans: tauri::State<'_, ModelScanEngine>) -> ModelScanStatus {
    scans.status()
}
#[tauri::command]
pub fn native_model_scan_cancel(
    scans: tauri::State<'_, ModelScanEngine>,
    id: String,
) -> Result<ModelScanStatus, String> {
    let result = scans.cancel(&id);
    if let Ok(ref s) = result {
        report(s, "INFO", "cancel requested; finishing current file");
    }
    result
}

#[tauri::command]
pub async fn native_model_scan_start(
    app: tauri::AppHandle,
    scans: tauri::State<'_, ModelScanEngine>,
    engine: tauri::State<'_, DownloadEngine>,
    auth: tauri::State<'_, AuthState>,
    root: String,
    scope: ScanScope,
    authenticated: bool,
) -> Result<ModelScanStatus, String> {
    let result = (|| {
        if engine.status()?.state == "running" {
            return Err("Wait for downloads to finish before scanning files".into());
        }
        let catalog = engine.catalog()?;
        let provider = catalog
            .providers
            .iter()
            .find(|p| p.kind == "civitai")
            .ok_or("No CivitAI provider in catalog")?
            .clone();
        let header = credential(&auth, &provider, authenticated)?;
        use tauri::Manager;
        *scans.1.lock().unwrap_or_else(|e| e.into_inner()) = Some(
            app.path()
                .app_local_data_dir()
                .map_err(|_| "Asset directory unavailable")?
                .join("model-assets"),
        );
        scans.start(root, scope, provider, header)
    })();
    if result.is_err() {
        report(
            &scans.status(),
            "ERROR",
            "start rejected; check running jobs, models path and provider authorization",
        );
    }
    result
}

/// Explicit selected-file refresh, including models from additional roots. No weight downloads.
#[tauri::command]
pub async fn native_model_scan_selected(
    app: tauri::AppHandle,
    scans: tauri::State<'_, ModelScanEngine>,
    engine: tauri::State<'_, DownloadEngine>,
    auth: tauri::State<'_, AuthState>,
    paths: Vec<String>,
    authenticated: bool,
) -> Result<ModelScanStatus, String> {
    use tauri::Manager;
    *scans.1.lock().unwrap_or_else(|e| e.into_inner()) = Some(
        app.path()
            .app_local_data_dir()
            .map_err(|_| "Asset directory unavailable")?
            .join("model-assets"),
    );
    let result = (|| {
        if engine.status()?.state == "running" {
            return Err("Wait for downloads to finish before scanning files".into());
        }
        let catalog = engine.catalog()?;
        let provider = catalog
            .providers
            .iter()
            .find(|p| p.kind == "civitai")
            .ok_or("No CivitAI provider in catalog")?
            .clone();
        let header = credential(&auth, &provider, authenticated)?;
        scans.start_job(
            "Selected model files".into(),
            ScanScope::All,
            provider,
            header,
            Some(paths),
        )
    })();
    if result.is_err() {
        report(
            &scans.status(),
            "ERROR",
            "selected refresh rejected; check selection and provider authorization",
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn selected_scan_cancellation_stops_remaining_and_preserves_completed() {
        let engine = ModelScanEngine::default();
        let initial = engine.reserve("selected".into()).unwrap();
        assert!(engine.reserve("other".into()).is_err());
        engine.update(|s| {
            s.total = 2;
            s.completed = 1;
            s.matched = 1;
            s.outcomes.push(FileOutcome {
                path: "original".into(),
                state: "matched".into(),
            });
        });
        assert!(engine.cancel("wrong request").is_err());
        engine.cancel(&initial.id).unwrap();
        let p = Provider {
            id: "fixture".into(),
            kind: "civitai".into(),
            label: "Fixture".into(),
            origin: "http://127.0.0.1:1".into(),
            scopes: vec!["model".into()],
            anonymous_notice: String::new(),
        };
        // A cancelled continuation must not even attempt this invalid network destination.
        engine
            .run_paths(
                "selected".into(),
                ScanScope::All,
                p,
                None,
                Some(vec!["one".into(), "two".into()]),
            )
            .await
            .unwrap();
        let s = engine.status();
        assert_eq!(s.completed, 1);
        assert_eq!(s.outcomes.len(), 1);
        assert_eq!(s.matched, 1);
        assert_eq!(s.remaining, 1);
    }

    #[test]
    fn background_scan_scopes_keep_non_loras_in_models() {
        let root = tempfile::tempdir().unwrap();
        for folder in ["loras", "checkpoints", "diffusion_models", "vae"] {
            std::fs::create_dir(root.path().join(folder)).unwrap();
            std::fs::write(root.path().join(folder).join("a.pt"), b"model").unwrap();
        }
        let models = scan(root.path()).unwrap();
        assert_eq!(
            models.iter().filter(|m| ScanScope::All.includes(m)).count(),
            4
        );
        assert_eq!(
            models
                .iter()
                .filter(|m| ScanScope::Models.includes(m))
                .count(),
            3
        );
        assert_eq!(
            models
                .iter()
                .filter(|m| ScanScope::Loras.includes(m))
                .count(),
            1
        );
        assert!(serde_json::from_str::<ScanScope>("\"other\"").is_err());
    }

    #[tokio::test]
    async fn background_scan_is_detached_serial_and_logs_failures_and_counts() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("loras");
        std::fs::create_dir(&dir).unwrap();
        for name in ["a.pt", "b.pt"] {
            std::fs::write(dir.join(name), b"model").unwrap();
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (release, wait) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            for code in [500, 404] {
                let deadline = Instant::now() + Duration::from_secs(10);
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(s) => break s,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline);
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(e) => panic!("{e}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = vec![];
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                    assert!(request.len() < 8192);
                }
                wait.recv_timeout(Duration::from_secs(10)).unwrap();
                write!(
                    stream,
                    "HTTP/1.1 {code} Fixture\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
            }
        });
        let provider = Provider {
            id: "fixture".into(),
            kind: "civitai".into(),
            label: "Fixture".into(),
            origin,
            scopes: vec!["model".into()],
            anonymous_notice: String::new(),
        };
        let engine = ModelScanEngine::default();
        assert_eq!(engine.status().state, "idle");
        let monitor = engine.clone();
        let initial = engine
            .start(
                root.path().to_string_lossy().into_owned(),
                ScanScope::All,
                provider.clone(),
                Some(HeaderValue::from_static("Bearer scan-test-secret")),
            )
            .unwrap();
        drop(engine);
        tokio::time::timeout(Duration::from_secs(10), async {
            while monitor.status().file.is_none() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let active = monitor.status();
        assert_eq!(
            (active.completed, active.total, active.remaining),
            (0, 2, 2)
        );
        assert!(active.file.unwrap().ends_with("a.pt"));
        assert!(monitor
            .start(
                root.path().to_string_lossy().into_owned(),
                ScanScope::All,
                provider,
                None
            )
            .is_err());
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while monitor.status().completed == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            (monitor.status().completed, monitor.status().remaining),
            (1, 1)
        );
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            while monitor.status().state == "running" {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        tokio::task::spawn_blocking(move || server.join().unwrap())
            .await
            .unwrap();
        let status = monitor.status();
        assert_eq!(status.state, "failed");
        assert_eq!(
            (
                status.completed,
                status.total,
                status.remaining,
                status.failed
            ),
            (2, 2, 0, 1)
        );
        assert!(dir.join("b.pt.deas.json").is_file());
        assert_eq!(std::fs::read(dir.join("a.pt")).unwrap(), b"model");
        let logs = crate::logs::native_logs(0);
        let entries: Vec<_> = logs
            .iter()
            .filter(|e| e.text.contains(&initial.id))
            .collect();
        assert!(
            entries
                .iter()
                .all(|e| e.source == crate::logs::Source::Deas
                    && !e.text.contains("scan-test-secret"))
        );
        assert!(entries.iter().any(|e| e.text.contains("ERROR: Library")
            && e.text.contains("a.pt")
            && e.text.contains("completed=1 total=2 remaining=1 failed=1")));
        assert!(entries.iter().any(|e| e.text.contains("finished")
            && e.text.contains("completed=2 total=2 remaining=0 failed=1")));
    }

    #[tokio::test]
    async fn background_scan_listing_failure_releases_the_running_lock() {
        let engine = ModelScanEngine::default();
        let provider = Provider {
            id: "fixture".into(),
            kind: "civitai".into(),
            label: "Fixture".into(),
            origin: "http://127.0.0.1:1".into(),
            scopes: vec!["model".into()],
            anonymous_notice: String::new(),
        };
        assert!(engine
            .start("relative".into(), ScanScope::All, provider.clone(), None)
            .is_err());
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("missing").to_string_lossy().into_owned();
        engine
            .start(missing.clone(), ScanScope::All, provider.clone(), None)
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while engine.status().state == "running" {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(engine.status().state, "failed");
        assert!(engine
            .start(
                root.path().to_string_lossy().into_owned(),
                ScanScope::All,
                provider,
                None
            )
            .is_ok());
        tokio::time::timeout(Duration::from_secs(5), async {
            while engine.status().state == "running" {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(engine.status().state, "succeeded");
        assert_eq!(
            (
                engine.status().completed,
                engine.status().total,
                engine.status().remaining
            ),
            (0, 0, 0)
        );
    }
}
