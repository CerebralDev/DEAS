//! Native whole-Workbench Stop; shared backend jobs and services are never cleared/stopped.
use super::*;
use cancel::{Cancellation, InterruptOwner, Owner};

impl Jobs {
    pub(super) async fn workbench_stop(
        &self,
        confirmed: bool,
        owner: Owner,
        interrupt_owner: InterruptOwner,
    ) -> Result<Value, String> {
        if !confirmed {
            return Err("Explicit Workbench Stop confirmation is required".into());
        }
        let mut receiver = {
            let mut session = self.lock()?;
            if let Some(receiver) = &session.stop {
                receiver.clone()
            } else {
                let next_epoch = session
                    .admission_epoch
                    .checked_add(1)
                    .filter(|epoch| *epoch <= workbench::MAX_ADMISSION_EPOCH)
                    .ok_or("Workbench admission epoch exhausted; Stop refused without mutation")?;
                session.admission_epoch = next_epoch;
                let (sender, receiver) = tokio::sync::watch::channel(None);
                let mut retained = Vec::new();
                let mut discarded = 0;
                for held in &session.workbench {
                    let mut entry = held.lock()?;
                    // The submit install and this invalidation share the held session lock.
                    entry.stop_requested = true;
                    if let Some(job) = &entry.job {
                        // Display observation ends at confirmed invalidation, even if backend
                        // reconciliation is partial or leaves the execution outcome unknown.
                        job.stop_observer();
                        if let Some(cancel) = entry.audio_post.as_ref() {
                            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        let epoch = if job.terminal() || entry.audio_post.is_some() {
                            Ok(0)
                        } else {
                            owner(&job.plan.base)
                        };
                        retained.push((held.clone(), epoch));
                    } else {
                        entry.preparing = None;
                        entry.plan = None;
                        discarded += 1;
                    }
                }
                session.workbench = retained.iter().map(|(held, _)| held.clone()).collect();
                session.stop = Some(receiver.clone());
                let jobs = self.clone();
                // Stop owns its completion, not the IPC waiter. Overlapping callers share a result.
                tokio::spawn(async move {
                    let mut result = jobs
                        .stop_held(retained.clone(), discarded, owner, interrupt_owner)
                        .await;
                    if let Ok(mut session) = jobs.lock() {
                        for (held, _) in &retained {
                            if let Ok(mut entry) = held.lock() {
                                entry.stop_requested = false;
                            }
                        }
                        // Publish before reopening admission, under the same session lock.
                        if let Ok(snapshot) = &mut result {
                            snapshot["stopping"] = json!(false);
                        }
                        sender.send_replace(Some(result));
                        session.stop = None;
                    }
                });
                receiver
            }
        };
        let result = receiver
            .wait_for(|result| result.is_some())
            .await
            .map_err(|_| "Workbench Stop task failed")?
            .clone();
        result.ok_or("Workbench Stop result unavailable")?
    }

    async fn stop_held(
        &self,
        held: Vec<(Arc<Jobs>, Result<u64, String>)>,
        discarded: usize,
        owner: Owner,
        interrupt_owner: InterruptOwner,
    ) -> Result<Value, String> {
        use futures_util::{stream, StreamExt};
        let results = stream::iter(held)
            .map(|(held, epoch)| {
                let owner = owner.clone();
                let interrupt_owner = interrupt_owner.clone();
                async move {
                    let id = held
                        .lock()?
                        .job
                        .as_ref()
                        .ok_or("Missing held Stop job")?
                        .id
                        .clone();
                    let result = self
                        .stop_one(&held, &id, epoch, owner, interrupt_owner)
                        .await;
                    let snapshot = held.current()?;
                    cancel::cancel_log(
                        Some(&snapshot),
                        "stop",
                        result.is_err() || snapshot["state"] == "unknown",
                        "target reconciled or unresolved; see operation result",
                    );
                    Ok::<_, String>((id, result))
                }
            })
            .buffer_unordered(20)
            .collect::<Vec<_>>()
            .await;
        let mut issues = Vec::new();
        for result in results {
            let (id, result) = result?;
            if let Err(error) = result {
                issues.push(json!({"jobId":id,"error":error}));
            }
        }
        let mut snapshot = self.workbench_current()?;
        let jobs = snapshot["jobs"]
            .as_array()
            .ok_or("Invalid Workbench snapshot")?;
        let outcome = if !issues.is_empty()
            || jobs.iter().any(|j| {
                !matches!(
                    j["state"].as_str(),
                    Some("succeeded" | "failed" | "cancelled")
                )
            }) {
            "partial"
        } else if jobs.is_empty() && discarded == 0 {
            "empty"
        } else {
            "settled"
        };
        snapshot["outcome"] = json!(outcome);
        snapshot["discarded"] = json!(discarded);
        snapshot["issues"] = json!(issues);
        crate::logs::push(
            crate::logs::Source::Deas,
            &format!(
                "{}: Workbench stop: outcome={outcome} discarded={discarded} issues={}; no global interrupt",
                if outcome == "partial" { "WARN" } else { "INFO" },
                json!(issues)
            ),
        );
        Ok(snapshot)
    }

    async fn stop_one(
        &self,
        held: &Jobs,
        id: &str,
        epoch: Result<u64, String>,
        owner: Owner,
        interrupt_owner: InterruptOwner,
    ) -> Result<(), String> {
        // Existing submission, status and scoped mutation tasks have bounded network deadlines.
        tokio::time::timeout(Duration::from_secs(45), async {
            loop {
                let busy = {
                    let entry = held.lock()?;
                    entry.audio_post.is_some()
                        || entry
                            .job
                            .as_ref()
                            .is_some_and(|j| j.submitting || j.polling)
                };
                if !busy {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok::<_, String>(())
        })
        .await
        .map_err(|_| "Workbench Stop job stayed busy; no further mutation sent")??;
        let (terminal, prompt, reconcile_only, base) = {
            let entry = held.lock()?;
            let job = entry.job.as_ref().ok_or("Missing held Stop job")?;
            (
                job.terminal(),
                job.prompt_id.is_some(),
                job.state == "unknown",
                job.plan.base.clone(),
            )
        };
        if terminal {
            return Ok(());
        }
        if !prompt {
            return Err(
                "Submission prompt ID unknown; no cancellation sent or submission retried".into(),
            );
        }
        let epoch = epoch?;
        if owner(&base)? != epoch {
            return Err("Owned backend lifecycle changed during Stop; no mutation sent".into());
        }
        let pinned_owner: Owner = Arc::new(move |base| {
            let current = owner(base)?;
            if current != epoch {
                return Err("Owned backend lifecycle changed during Stop; no mutation sent".into());
            }
            Ok(current)
        });
        let pinned_interrupt: InterruptOwner = Arc::new(move |base| {
            let authority = interrupt_owner(base)?;
            if authority.0 != epoch {
                return Err("Owned backend lifecycle changed during Stop; no mutation sent".into());
            }
            Ok(authority)
        });
        held.status_inner(id.to_owned(), true).await?;
        if reconcile_only {
            return Ok(());
        }
        let (state, pending_attempted, interrupt_attempted) = {
            let entry = held.lock()?;
            let job = entry.job.as_ref().ok_or("Missing held Stop job")?;
            if job.terminal() {
                return Ok(());
            }
            (job.state, entry.cancel_attempted, entry.interrupt_attempted)
        };
        if state == "queued" && !pending_attempted {
            self.cancel_scoped(
                id.to_owned(),
                true,
                Cancellation::Pending(pinned_owner),
                true,
            )
            .await?;
        }
        // A pending deletion may lose the race to execution: use the distinct scoped path once.
        if held.current()?["state"] == "running" && !interrupt_attempted {
            self.cancel_scoped(
                id.to_owned(),
                true,
                Cancellation::Running(pinned_interrupt),
                true,
            )
            .await?;
        }
        Ok(())
    }
}

#[tauri::command]
pub async fn native_workbench_stop(
    state: tauri::State<'_, Jobs>,
    engine: tauri::State<'_, crate::install::Engine>,
    confirmed: bool,
) -> Result<Value, String> {
    state
        .workbench_stop(
            confirmed,
            Arc::new(engine.backend.cancellation_owner()),
            Arc::new(engine.backend.interruption_owner()),
        )
        .await
}
