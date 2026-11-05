//! Scoped pending deletion and running interruption; never clears the backend queue.
use super::*;

pub(super) type Owner = Arc<dyn Fn(&reqwest::Url) -> Result<u64, String> + Send + Sync>;

pub(super) type InterruptOwner =
    Arc<dyn Fn(&reqwest::Url) -> Result<(u64, String), String> + Send + Sync>;

pub(super) enum Cancellation {
    Pending(Owner),
    Running(InterruptOwner),
}
impl Cancellation {
    fn authority(&self, base: &reqwest::Url) -> Result<(u64, Option<String>), String> {
        match self {
            Self::Pending(owner) => owner(base).map(|epoch| (epoch, None)),
            Self::Running(owner) => owner(base).map(|(epoch, token)| (epoch, Some(token))),
        }
    }
    fn running(&self) -> bool {
        matches!(self, Self::Running(_))
    }
    fn operation(&self) -> &'static str {
        if self.running() {
            "running-interrupt"
        } else {
            "pending-cancel"
        }
    }
}

impl Jobs {
    pub(super) async fn cancel_pending(
        &self,
        id: String,
        confirmed: bool,
        owner: Owner,
    ) -> Result<Value, String> {
        self.cancel_scoped(id, confirmed, Cancellation::Pending(owner), false)
            .await
    }

    pub(super) async fn interrupt_running(
        &self,
        id: String,
        confirmed: bool,
        owner: InterruptOwner,
    ) -> Result<Value, String> {
        self.cancel_scoped(id, confirmed, Cancellation::Running(owner), false)
            .await
    }

    pub(super) async fn cancel_scoped(
        &self,
        id: String,
        confirmed: bool,
        cancellation: Cancellation,
        stopping: bool,
    ) -> Result<Value, String> {
        let operation = cancellation.operation();
        let before = self
            .workbench_job(&id)?
            .map(|held| held.current())
            .transpose()?;
        let result = self
            .cancel_scoped_inner(id, confirmed, cancellation, stopping)
            .await;
        if result.is_err() {
            cancel_log(
                before.as_ref(),
                operation,
                true,
                "confirmation, identity, ownership, busy or prior-attempt refusal",
            );
        }
        result
    }

    async fn cancel_scoped_inner(
        &self,
        id: String,
        confirmed: bool,
        cancellation: Cancellation,
        stopping: bool,
    ) -> Result<Value, String> {
        if !confirmed {
            return Err("Explicit job cancellation confirmation is required".into());
        }
        let held = self
            .workbench_job(&id)?
            .ok_or("Unknown native Workbench job")?;
        let mut job = {
            let admission = self.lock()?;
            if admission.stop.is_some() && !stopping {
                return Err("Workbench Stop is in progress; no separate cancellation sent".into());
            }
            let mut session = held.lock()?;
            let attempted = if cancellation.running() {
                session.interrupt_attempted
            } else {
                session.cancel_attempted
            };
            let job = session.job.as_mut().ok_or("Unknown native Workbench job")?;
            if job.terminal() {
                return Ok(session.snapshot());
            }
            if job.submitting || job.polling {
                return Err("Native Workbench job is busy; no cancellation sent".into());
            }
            if attempted {
                return Err("Job cancellation was already attempted; reconcile status without another mutation".into());
            }
            if job.prompt_id.is_none() {
                return Ok(session.snapshot());
            }
            cancellation.authority(&job.plan.base)?;
            job.polling = true;
            job.clone()
        };
        // Own the task, not the IPC waiter; always release the shared status/cancellation flag.
        tokio::spawn(async move {
            let operation = cancellation.operation();
            let previous_outputs = job.outputs.clone();
            let result = tokio::time::timeout(
                Duration::from_secs(40),
                cancel_observed(&held, &mut job, &cancellation),
            ).await;
            let error = match result {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error),
                Err(_) => Some("Job cancellation deadline exceeded; reconcile status without retrying mutation".into()),
            };
            if let Some(ref error) = error {
                // Keep outputs/reference handles intact even when the mutation outcome is unknown.
                job.state = "unknown";
                job.error = Some(error.clone());
            }
            if job.outputs.is_empty() { job.outputs = previous_outputs; }
            job.polling = false;
            job.stop_observer_if_terminal();
            let mut session = held.lock()?;
            held.3.observe(&job);
            session.job = Some(job);
            session.retain_completed();
            let snapshot = session.snapshot();
            cancel_log(Some(&snapshot), operation, snapshot["state"] == "unknown", error.as_deref().unwrap_or("correlated state observed"));
            Ok(snapshot)
        }).await.map_err(|_| "Job cancellation task failed")?
    }
}

async fn cancel_observed(
    held: &Jobs,
    job: &mut Job,
    cancellation: &Cancellation,
) -> Result<(), String> {
    let epoch = cancellation.authority(&job.plan.base)?.0;
    let reconcile_only =
        job.state == "unknown" || (!cancellation.running() && job.state == "running");
    let present = poll_present(job)
        .await
        .map_err(|_| "Job cancellation correlation/status failed; no mutation sent")?;
    if cancellation.authority(&job.plan.base)?.0 != epoch {
        return Err("Owned backend lifecycle changed; no mutation sent".into());
    }
    if !present {
        return Err("Prompt absent from history and queue; cancellation remains unknown".into());
    }
    if reconcile_only
        || job.state
            != if cancellation.running() {
                "running"
            } else {
                "queued"
            }
    {
        return Ok(());
    }
    let client = client()?;
    // Recheck immediately before the only mutation; retain this marker even after a lost reply.
    let (current_epoch, token) = cancellation.authority(&job.plan.base)?;
    if current_epoch != epoch {
        return Err("Owned backend lifecycle changed; no mutation sent".into());
    }
    {
        let mut session = held.lock()?;
        if cancellation.running() {
            session.interrupt_attempted = true;
        } else {
            session.cancel_attempted = true;
        }
    }
    let accepted = mutate_job(&client, job, token).await;
    if cancellation.authority(&job.plan.base)?.0 != epoch {
        return Err("Owned backend lifecycle changed after mutation; outcome unknown".into());
    }
    let mut present = poll_present(job)
        .await
        .map_err(|_| "Job cancellation readback failed; outcome unknown; do not retry mutation")?;
    if !present {
        // History was read before queue: close the completion-between-reads window.
        present = poll_present(job).await.map_err(
            |_| "Job cancellation final readback failed; outcome unknown; do not retry mutation",
        )?;
    }
    if cancellation.authority(&job.plan.base)?.0 != epoch {
        return Err("Owned backend lifecycle changed during readback; outcome unknown".into());
    }
    if present && job.terminal() {
        return Ok(());
    }
    if present && job.state == "running" {
        job.error = accepted.err();
        return Ok(());
    }
    accepted?;
    if cancellation.running() {
        return Err(
            "Prompt absent without correlated terminal interruption; outcome unknown; do not retry"
                .into(),
        );
    }
    if present {
        return Err(
            "Prompt remains pending after cancellation; outcome unknown; do not retry mutation"
                .into(),
        );
    }
    job.state = "cancelled";
    job.error = None;
    Ok(())
}

async fn mutate_job(
    client: &reqwest::Client,
    job: &Job,
    token: Option<String>,
) -> Result<(), String> {
    let url = job
        .plan
        .base
        .join(if token.is_some() {
            "deas/jobs/interrupt"
        } else {
            "queue"
        })
        .map_err(|_| "Invalid held backend endpoint")?;
    let prompt_id = job.prompt_id.as_deref().ok_or("Missing held prompt ID")?;
    let request = if let Some(token) = token {
        let mut header = reqwest::header::HeaderValue::from_str(&token)
            .map_err(|_| "Invalid native interruption authority")?;
        header.set_sensitive(true);
        client
            .post(url)
            .header("X-DEAS-Interrupt-Token", header)
            .json(&json!({"prompt_id":prompt_id,"client_id":job.client_id,"prompt":job.plan.graph}))
    } else {
        client.post(url).json(&json!({"delete":[prompt_id]}))
    };
    let mut response = request
        .send()
        .await
        .map_err(|_| "Job cancellation reply lost; outcome unknown; do not retry mutation")?;
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|n| n > RESPONSE_LIMIT as u64)
    {
        return Err("Job cancellation response exceeded size bound; outcome unknown".into());
    }
    let mut size = 0;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Job cancellation reply interrupted; outcome unknown")?
    {
        if chunk.len() > RESPONSE_LIMIT - size {
            return Err("Job cancellation response exceeded size bound; outcome unknown".into());
        }
        size += chunk.len();
    }
    if status != reqwest::StatusCode::OK {
        return Err(format!(
            "Job cancellation rejected (HTTP {}); reconcile without retrying mutation",
            status.as_u16()
        ));
    }
    if size != 0 {
        return Err(
            "Job cancellation returned a non-empty cancellation acknowledgement; outcome unknown"
                .into(),
        );
    }
    Ok(())
}

pub(super) fn cancel_log(job: Option<&Value>, operation: &str, limited: bool, reason: &str) {
    static LAST: Mutex<Option<std::time::Instant>> = Mutex::new(None);
    if limited {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_some_and(|at| at.elapsed() < Duration::from_secs(60)) {
            return;
        }
        *last = Some(std::time::Instant::now());
    }
    let id = job.and_then(|j| j["id"].as_str()).unwrap_or("unavailable");
    let state = job.and_then(|j| j["state"].as_str()).unwrap_or("unchanged");
    let level = if limited || state == "unknown" {
        "WARN"
    } else {
        "INFO"
    };
    crate::logs::push(
        crate::logs::Source::Deas,
        &format!(
            "{level}: Workbench {operation}: job={id} state={state}; {reason}; no global interrupt"
        ),
    );
}

#[tauri::command]
pub async fn native_workbench_interrupt_running(
    state: tauri::State<'_, Jobs>,
    engine: tauri::State<'_, crate::install::Engine>,
    job_id: String,
    confirmed: bool,
) -> Result<Value, String> {
    state
        .interrupt_running(
            job_id,
            confirmed,
            Arc::new(engine.backend.interruption_owner()),
        )
        .await
}

#[tauri::command]
pub async fn native_workbench_cancel_pending(
    state: tauri::State<'_, Jobs>,
    engine: tauri::State<'_, crate::install::Engine>,
    job_id: String,
    confirmed: bool,
) -> Result<Value, String> {
    state
        .cancel_pending(
            job_id,
            confirmed,
            Arc::new(engine.backend.cancellation_owner()),
        )
        .await
}
