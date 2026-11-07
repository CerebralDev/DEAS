//! Workbench opt-in: each Run owns an existing exclusive lifecycle, never a deferred FIFO.
use super::*;

const WORKBENCH_LIMIT: usize = 20;
pub(super) const MAX_ADMISSION_EPOCH: u64 = (1_u64 << 53) - 1;

// Shared Workbench failure log limiter: once per minute.
static WORKBENCH_FAILURE_LAST: Mutex<Option<std::time::Instant>> = Mutex::new(None);

/// Test-only: log assertions run in one process sharing this limiter.
#[cfg(test)]
pub(super) fn reset_workbench_failure_logging() {
    *WORKBENCH_FAILURE_LAST
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;
}

impl Session {
    pub(super) fn workbench_busy(&self) -> Result<bool, String> {
        if self.stop.is_some() || self.auto_refine {
            return Ok(true);
        }
        for held in &self.workbench {
            if held.lock()?.job.as_ref().is_none_or(|job| !job.terminal()) {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

impl Jobs {
    /// True while a held Workbench job is not terminal or a Stop is settling; updates must wait.
    pub(crate) fn workbench_busy(&self) -> Result<bool, String> {
        self.lock()?.workbench_busy()
    }
    pub(super) fn workbench_job(&self, id: &str) -> Result<Option<Arc<Jobs>>, String> {
        for held in &self.lock()?.workbench {
            if held.lock()?.job.as_ref().is_some_and(|job| job.id == id) {
                return Ok(Some(held.clone()));
            }
        }
        Ok(None)
    }

    pub(super) fn workbench_current(&self) -> Result<Value, String> {
        let session = self.lock()?;
        let mut jobs = Vec::new();
        let mut preparing = 0;
        for held in &session.workbench {
            let held = held.lock()?;
            if held.job.is_some() {
                jobs.push(held.snapshot());
            } else {
                preparing += 1;
            }
        }
        Ok(
            json!({"jobs":jobs,"preparing":preparing,"capacity":WORKBENCH_LIMIT,"stopAvailable":true,"admissionEpoch":session.admission_epoch,"stopping":session.stop.is_some(),"autoRefining":session.auto_refine}),
        )
    }

    pub(super) async fn workbench_poll(&self) -> Result<Value, String> {
        use futures_util::{stream, StreamExt};
        let Ok(guard) = self.2.clone().try_lock_owned() else {
            return self.workbench_current();
        };
        let mut pending = Vec::new();
        for held in &self.lock()?.workbench {
            let session = held.lock()?;
            if let Some(job) = session
                .job
                .as_ref()
                .filter(|j| !j.terminal() && !j.submitting)
            {
                pending.push((held.clone(), job.id.clone()));
            }
        }
        // Finish even if the IPC waiter disappears: status must clear its per-job polling flag.
        tokio::spawn(async move {
            let _guard = guard;
            let results = stream::iter(pending)
                .map(|(held, id)| async move { held.workbench_status(id).await })
                .buffer_unordered(WORKBENCH_LIMIT)
                .collect::<Vec<_>>()
                .await;
            // Transport errors become unknown in status; storage errors cannot short-circuit peers.
            for result in results {
                result?;
            }
            Ok::<(), String>(())
        })
        .await
        .map_err(|_| "Workbench polling task failed")??;
        self.workbench_current()
    }

    async fn workbench_status(&self, job_id: String) -> Result<Value, String> {
        let before = self.current()?;
        let result = self.status(job_id).await?;
        if before["state"] != result["state"] || before["error"] != result["error"] {
            report_job("status", &result);
        }
        Ok(result)
    }

    pub(super) async fn status_for(&self, job_id: String) -> Result<Value, String> {
        if let Some(held) = self.workbench_job(&job_id)? {
            return held.workbench_status(job_id).await;
        }
        self.status(job_id).await
    }

    pub(super) async fn workbench_run(
        &self,
        base_url: String,
        intent: Value,
        confirmed: bool,
        admission_epoch: u64,
        engine: Option<&crate::install::Engine>,
    ) -> Result<Value, String> {
        let result = self
            .workbench_run_inner(base_url, intent, confirmed, admission_epoch, engine, false)
            .await;
        match &result {
            Ok(job) => report_job("run", job),
            Err(_) => {
                let mut last = WORKBENCH_FAILURE_LAST
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if last.is_none_or(|at| at.elapsed() >= Duration::from_secs(60)) {
                    *last = Some(std::time::Instant::now());
                    crate::logs::push(
                        crate::logs::Source::Deas,
                        "ERROR: Workbench run: preparation or admission failed; see operation result (repeats limited to once per minute)",
                    );
                }
            }
        }
        result
    }

    pub(super) async fn workbench_auto_refine_run(
        &self,
        base_url: String,
        intent: Value,
        admission_epoch: u64,
        engine: &crate::install::Engine,
    ) -> Result<Value, String> {
        let result = self
            .workbench_run_inner(base_url, intent, true, admission_epoch, Some(engine), true)
            .await;
        if let Ok(job) = &result {
            report_job("auto-refine", job);
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    async fn workbench_run_inner(
        &self,
        base_url: String,
        intent: Value,
        confirmed: bool,
        admission_epoch: u64,
        engine: Option<&crate::install::Engine>,
        auto_refine: bool,
    ) -> Result<Value, String> {
        if !confirmed {
            return Err("Explicit Workbench submission confirmation is required".into());
        }
        if admission_epoch > MAX_ADMISSION_EPOCH {
            return Err(
                "Invalid Workbench admission epoch; expected a nonnegative JavaScript safe integer"
                    .into(),
            );
        }
        let held = Arc::new(Jobs(
            Arc::new(Mutex::new(Session {
                studio: true,
                ..Session::default()
            })),
            Arc::default(),
            Arc::default(),
            self.3.clone(),
        ));
        {
            let mut session = self.lock()?;
            if session.auto_refine && !auto_refine {
                return Err(
                    "Auto Refine owns Workbench admission until its serial operation finishes"
                        .into(),
                );
            }
            if session.stop.is_some() {
                return Err("Workbench Stop is in progress; no admission".into());
            }
            // Stop advances this fence under the same lock, even when its IPC waiters disappear.
            if admission_epoch != session.admission_epoch {
                return Err(
                    "Stale Workbench admission epoch; read current state before a new Run".into(),
                );
            }
            if session.preparing.is_some() || session.job.as_ref().is_some_and(|j| !j.terminal()) {
                return Err("An exclusive diagnostic operation is in flight or unknown".into());
            }
            if session.workbench.len() == WORKBENCH_LIMIT {
                let mut evict = None;
                for (index, entry) in session.workbench.iter().enumerate() {
                    if entry.lock()?.job.as_ref().is_some_and(|job| {
                        job.terminal()
                            && (!auto_refine || job.plan.intent["recipeId"] == "sam3-mask")
                    }) {
                        evict = Some(index);
                        break;
                    }
                }
                let index = evict.ok_or(
                    "Workbench queue capacity reached; active and unknown jobs are retained",
                )?;
                session.workbench.remove(index);
            }
            session.workbench.push(held.clone());
        }
        let jobs = self.clone();
        let media_root = engine.and_then(|engine| {
            crate::validate_base_url(&base_url)
                .ok()
                .and_then(|base| job_outputs::MediaRoot::owned(engine, base.as_str()))
        });
        let model_roots = engine
            .map(|engine| LocalModels::owned(engine, &base_url))
            .unwrap_or_default();
        // Admission owns preparation/submission even when its IPC waiter is cancelled.
        tokio::spawn(async move {
            jobs.workbench_execute(held, base_url, intent, media_root, model_roots)
                .await
        })
        .await
        .map_err(|_| "Workbench Run task failed")?
    }

    /// Auto Refine retains every non-SAM pass permanently in the queue; check the whole run fits
    /// before the first upload, with SAM intermediates counted as evictable.
    pub(super) fn refine_slots_ok(&self, passes: usize) -> Result<(), String> {
        let occupied = self
            .lock()?
            .workbench
            .iter()
            .filter(|held| {
                held.lock()
                    .map(|session| {
                        session.job.as_ref().is_none_or(|job| {
                            !job.terminal() || job.plan.intent["recipeId"] != "sam3-mask"
                        })
                    })
                    .unwrap_or(true)
            })
            .count();
        if occupied + passes > WORKBENCH_LIMIT {
            return Err(format!(
                "Auto Refine needs {} retained queue slots but only {} are free; reduce targets or masks",
                passes,
                WORKBENCH_LIMIT - occupied
            ));
        }
        Ok(())
    }

    async fn workbench_execute(
        &self,
        held: Arc<Jobs>,
        base_url: String,
        intent: Value,
        media_root: Option<job_outputs::MediaRoot>,
        model_roots: Vec<std::path::PathBuf>,
    ) -> Result<Value, String> {
        let plan = match held
            .prepare_with_models(base_url, intent, model_roots)
            .await
        {
            Ok(plan) => plan,
            Err(error) => {
                self.lock()?
                    .workbench
                    .retain(|entry| !Arc::ptr_eq(entry, &held));
                return Err(error);
            }
        };
        if let Some(plan) = held.lock()?.plan.as_mut() {
            plan.media_root = media_root;
        }
        let result = held
            .submit(plan["id"].as_str().unwrap().to_owned(), true)
            .await;
        if result.is_err() && held.lock()?.job.is_none() {
            self.lock()?
                .workbench
                .retain(|entry| !Arc::ptr_eq(entry, &held));
        }
        result
    }
}

pub(super) fn report_job(operation: &str, job: &Value) {
    // Never forward backend errors, prompts, filenames or endpoints to the shared log.
    let level = if matches!(job["state"].as_str(), Some("unknown" | "failed")) {
        "ERROR"
    } else {
        "INFO"
    };
    crate::logs::push(
        crate::logs::Source::Deas,
        &format!(
            "{level}: Workbench {operation}: job={} state={}",
            job["id"].as_str().unwrap_or("unknown"),
            job["state"].as_str().unwrap_or("unknown")
        ),
    );
}

/// One explicit Run prepares and immediately submits an immutable native intent.
#[tauri::command]
pub async fn native_workbench_run(
    state: tauri::State<'_, Jobs>,
    engine: tauri::State<'_, crate::install::Engine>,
    base_url: String,
    intent: Value,
    confirmed: bool,
    admission_epoch: u64,
) -> Result<Value, String> {
    state
        .workbench_run(base_url, intent, confirmed, admission_epoch, Some(&engine))
        .await
}

/// Refresh only held Workbench jobs; no submission, cancellation, or diagnostic polling.
#[tauri::command]
pub async fn native_workbench_poll(state: tauri::State<'_, Jobs>) -> Result<Value, String> {
    state.workbench_poll().await
}

/// Session recovery only: no backend requests, automatic retries, or cancellation.
#[tauri::command]
pub fn native_workbench_current(state: tauri::State<'_, Jobs>) -> Result<Value, String> {
    state.workbench_current()
}
