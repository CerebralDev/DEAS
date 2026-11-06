//! Optional, bounded metadata persistence. This never grants execution or filesystem authority.
use super::*;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    path::Path,
    sync::mpsc,
    time::{SystemTime, UNIX_EPOCH},
};

const FILE: &str = "recent-generation-history.json";
const LOCK: &str = ".recent-generation-history.lock";
const LIMIT: usize = 10;
// Wildcard's existing 4096 × 512-UTF16-key contract can encode ~12 MiB of escaped keys.
// Preserve that valid intent, plus prompts/references/options, without weakening input bounds.
const RECORD_BYTES: usize = 16 * 1024 * 1024;
const FILE_BYTES: u64 = (LIMIT * RECORD_BYTES + 4096) as u64;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryOutput {
    filename: String,
    subfolder: String,
    #[serde(rename = "type")]
    output_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    role: Option<String>,
}

/// This is the validated data-only recipe intent, never a Plan/JobStatus/graph or mutable draft.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct EffectiveIntent(Value);
impl<'de> Deserialize<'de> for EffectiveIntent {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(d)?;
        compile(&value)
            .map_err(|_| serde::de::Error::custom("Invalid historical effective intent"))?;
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryRecord {
    id: String,
    client_id: String,
    prompt_id: Option<String>,
    origin: String,
    app_version: String,
    admitted_at: u64,
    observed_at: u64,
    revision: u64,
    started_at: Option<u64>,
    generation_ms: Option<u64>,
    /// Host/GPU maxima sampled while the job was active; absent for records older than 0.2.9.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    peaks: Option<crate::monitoring::Peaks>,
    state: String,
    intent: EffectiveIntent,
    outputs: Vec<HistoryOutput>,
    output_root: Option<String>,
    output_root_identity: Option<String>,
    backend_origin: String,
    error: Option<String>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Document {
    schema_version: u32,
    records: Vec<HistoryRecord>, // oldest admission first, never promoted by updates
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryView {
    schema_version: u32,
    records: Vec<HistoryRecord>,
    read_only: bool,
    error: Option<String>,
}

#[derive(Default)]
struct State {
    document: Document,
    revision: u64,
    persisted_revision: u64,
    error: Option<String>,
    sender: Option<mpsc::SyncSender<()>>,
    initialized: bool,
    last_error_log: Option<std::time::Instant>,
}

#[derive(Clone, Default)]
pub(super) struct History(Arc<Mutex<State>>, Arc<std::sync::Condvar>);

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

impl History {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }
    pub(super) fn view(&self) -> HistoryView {
        let state = self.lock();
        HistoryView {
            schema_version: 1,
            records: state.document.records.iter().rev().cloned().collect(),
            read_only: state.sender.is_none(),
            error: state.error.clone(),
        }
    }
    fn outputs(&self, id: &str, approved_root: &Path) -> Result<Vec<&'static str>, String> {
        let record = self
            .lock()
            .document
            .records
            .iter()
            .find(|record| record.id == id)
            .cloned()
            .ok_or("Unknown recent history record")?;
        validate_record(&record)?;
        let anchor = crate::downloads::fs::Anchor::open(approved_root)?;
        let original_identity = anchor.identity()?;
        let identity = format!("{}:{}", original_identity.0, original_identity.1);
        if record.output_root.as_deref() != approved_root.to_str()
            || record.output_root_identity.as_deref() != Some(&identity)
        {
            return Err("The selected output context does not match this historical record".into());
        }
        // No historical path grants: only bounded descriptors beneath the current native-approved root.
        let states = record
            .outputs
            .iter()
            .map(|output| output_availability(approved_root, output))
            .collect();
        if crate::downloads::fs::Anchor::open(approved_root)?.identity()? != original_identity {
            return Err("The selected output directory changed during validation".into());
        }
        Ok(states)
    }
    fn degrade(&self, operation: &str, id: Option<&str>, message: &str) {
        let mut state = self.lock();
        state.error = Some(message.into());
        if state
            .last_error_log
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(60))
        {
            state.last_error_log = Some(std::time::Instant::now());
            // Fixed messages only. Never forward OS paths, backend bodies, prompts or tokens.
            crate::logs::push(
                crate::logs::Source::Deas,
                &format!(
                    "ERROR: Generation history operation={operation} job={} {message}; persistence degraded (repeats limited to once per minute)",
                    id.filter(|id| canonical_uuid(id)).unwrap_or("unknown")
                ),
            );
        }
    }
    pub(super) fn unavailable(&self) {
        self.degrade(
            "initialize",
            None,
            "The app-data path is unavailable; recent history cannot be persisted",
        );
    }
    pub(super) fn initialize(&self, path: &Path) {
        {
            let mut state = self.lock();
            if state.initialized {
                return;
            }
            state.initialized = true;
        }
        match Writer::open(path) {
            Ok(writer) => {
                let (sender, receiver) = mpsc::sync_channel(1);
                {
                    let mut state = self.lock();
                    state.document = writer.document.clone();
                    state.sender = Some(sender);
                }
                let history = Arc::downgrade(&self.0);
                let completed = self.1.clone();
                // Weak state avoids retaining an abandoned app/test's writer lock forever.
                if std::thread::Builder::new()
                    .name("deas-generation-history".into())
                    .spawn(move || Self::worker(history, completed, writer, receiver))
                    .is_err()
                {
                    self.degrade(
                        "initialize",
                        None,
                        "The recent history persistence worker could not be started",
                    );
                    self.lock().sender = None;
                }
            }
            Err(_) => {
                if let Ok(root) = crate::projects::Root::open(path) {
                    if let Ok((document, _)) = load(&root) {
                        self.lock().document = document;
                    }
                }
                self.degrade(
                    "open",
                    None,
                    "Recent history could not be opened safely; existing data was preserved",
                );
            }
        }
    }
    fn worker(
        state: std::sync::Weak<Mutex<State>>,
        completed: Arc<std::sync::Condvar>,
        mut writer: Writer,
        receiver: mpsc::Receiver<()>,
    ) {
        while receiver.recv().is_ok() {
            let Some(state) = state.upgrade() else {
                break;
            };
            let history = Self(state, completed.clone());
            // One pending notification, and one latest committed snapshot: no unbounded task queue.
            while receiver.try_recv().is_ok() {}
            let (revision, document) = {
                let state = history.lock();
                (state.revision, state.document.clone())
            };
            if history.lock().persisted_revision == revision {
                continue;
            }
            if let Err(error) = writer.save(&document) {
                // Freeze writes on any uncertainty rather than overwriting a corrupt/foreign file.
                history.degrade("publish", document.records.last().map(|record| record.id.as_str()), &format!("Recent history could not be saved safely ({error}); the previous file was preserved where publication had not completed"));
                history.lock().sender = None;
                completed.notify_all();
                break;
            }
            let mut state = history.lock();
            state.persisted_revision = revision;
            if state.revision == revision {
                state.error = None;
            }
            completed.notify_all();
        }
    }
    /// Bounded exit checkpoint; no job lock is held and persistence never changes execution results.
    pub(super) fn flush(&self) {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut state = self.lock();
        let target = state.revision;
        while state.persisted_revision < target && state.sender.is_some() {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                drop(state);
                self.degrade(
                    "flush",
                    None,
                    "Recent history did not finish saving before the bounded exit deadline",
                );
                return;
            }
            let (next, _) = self
                .1
                .wait_timeout(state, remaining)
                .unwrap_or_else(|error| error.into_inner());
            state = next;
        }
    }
    fn changed(state: &mut State) {
        state.revision += 1;
        if let Some(sender) = &state.sender {
            let _ = sender.try_send(());
        }
    }
    /// Called under the owning job commit lock, before the first submission await.
    pub(super) fn admit(&self, job: &Job, origin: &str) {
        let admitted_at = now();
        let record = HistoryRecord {
            id: job.id.clone(),
            client_id: job.client_id.clone(),
            prompt_id: job.prompt_id.clone(),
            origin: origin.into(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            admitted_at,
            observed_at: admitted_at,
            revision: 1,
            started_at: timing(job, "startedAt"),
            generation_ms: timing(job, "generationMs"),
            peaks: None,
            state: job.state.into(),
            intent: EffectiveIntent(effective_intent(&job.plan.intent)),
            outputs: projection_outputs(job),
            output_root: job
                .plan
                .media_root
                .as_ref()
                .map(|root| root.recorded_path())
                .and_then(|root| root.to_str())
                .map(str::to_owned),
            output_root_identity: job.plan.media_root.as_ref().map(|root| {
                let (device, file) = root.recorded_identity();
                format!("{device}:{file}")
            }),
            backend_origin: job.plan.base.origin().ascii_serialization(),
            error: safe_error(job),
        };
        let mut state = self.lock();
        if state
            .document
            .records
            .iter()
            .any(|record| record.id == job.id)
        {
            return;
        }
        state.document.schema_version = 1;
        state.document.records.push(record);
        if state.document.records.len() > LIMIT {
            state.document.records.remove(0);
        }
        Self::changed(&mut state);
    }
    /// Updates never create a row: a pruned old completion cannot displace a newer admission.
    pub(super) fn observe(&self, job: &Job) {
        let mut state = self.lock();
        let Some(record) = state
            .document
            .records
            .iter_mut()
            .find(|record| record.id == job.id)
        else {
            return;
        };
        let outputs = projection_outputs(job);
        let outputs = if outputs.is_empty() {
            record.outputs.clone()
        } else {
            outputs
        };
        let error = safe_error(job);
        let started_at = timing(job, "startedAt").or(record.started_at);
        let generation_ms = timing(job, "generationMs").or(record.generation_ms);
        let peaks = (job.terminal() && record.peaks.is_none())
            .then(|| crate::monitoring::take_peaks(&job.id))
            .flatten();
        if record.state == job.state
            && record.prompt_id == job.prompt_id
            && record.outputs == outputs
            && record.error == error
            && record.started_at == started_at
            && record.generation_ms == generation_ms
            && peaks.is_none()
        {
            return;
        }
        if peaks.is_some() {
            record.peaks = peaks;
        }
        record.started_at = started_at;
        record.generation_ms = generation_ms;
        record.state = job.state.into();
        record.prompt_id = job.prompt_id.clone();
        record.outputs = outputs;
        record.error = error;
        record.observed_at = now().max(record.observed_at);
        record.revision += 1;
        Self::changed(&mut state);
    }
    /// The physical move already succeeded through existing authority; metadata is best-effort.
    pub(super) fn relocate(&self, root: &Path, identity: (u64, u64), source: &Path, target: &Path) {
        if target == source {
            return;
        }
        let (Some(root_text), Some(filename), Some(parent)) = (
            root.to_str(),
            target.file_name().and_then(|n| n.to_str()),
            target.parent().and_then(|p| p.strip_prefix(root).ok()),
        ) else {
            return;
        };
        let subfolder = parent
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        let identity = format!("{}:{}", identity.0, identity.1);
        let mut state = self.lock();
        let mut changed = false;
        for record in &mut state.document.records {
            if record.output_root.as_deref() != Some(root_text)
                || record.output_root_identity.as_deref() != Some(&identity)
            {
                continue;
            }
            let mut touched = false;
            for output in &mut record.outputs {
                if root.join(&output.subfolder).join(&output.filename) == source {
                    output.filename = filename.into();
                    output.subfolder = subfolder.clone();
                    touched = true;
                }
            }
            if touched {
                record.revision += 1;
                record.observed_at = now().max(record.observed_at);
                changed = true;
            }
        }
        if changed {
            Self::changed(&mut state);
        }
    }
}

fn effective_intent(intent: &Value) -> Value {
    let mut effective = intent.clone();
    if crate::jobs_video::is_video(intent) {
        effective["outputPrefix"] = crate::jobs_video::effective_output_prefix(intent);
        effective["output"] = crate::jobs_video::effective_output(intent);
    }
    effective
}

fn timing(job: &Job, field: &str) -> Option<u64> {
    job.metadata
        .as_ref()
        .and_then(|m| m[field].as_u64())
        .filter(|n| *n <= 9_007_199_254_740_991)
}

fn safe_error(job: &Job) -> Option<String> {
    job.error.as_ref().map(|_| {
        match job.state {
            "failed" => "The backend rejected or failed this attempt; see its session diagnostics",
            "cancelled" => "Cancellation was observed",
            _ if job.submitting => {
                "Submission was consumed; outcome was not yet observed; do not retry automatically"
            }
            _ => "The last observed execution outcome is uncertain; no automatic retry",
        }
        .into()
    })
}

fn projection_outputs(job: &Job) -> Vec<HistoryOutput> {
    let observed = if job.outputs.is_empty() {
        &job.history_outputs
    } else {
        &job.outputs
    };
    observed
        .iter()
        .filter_map(|value| {
            Some(HistoryOutput {
                filename: value["filename"].as_str()?.into(),
                subfolder: value["subfolder"].as_str()?.into(),
                output_type: "output".into(),
                format: value["format"].as_str().map(str::to_owned),
                role: value["role"].as_str().map(str::to_owned),
            })
        })
        .collect()
}

fn validate_record(record: &HistoryRecord) -> Result<(), String> {
    compile(&record.intent.0).map_err(|_| "Invalid historical effective intent")?;
    if !canonical_uuid(&record.id)
        || !canonical_uuid(&record.client_id)
        || record
            .prompt_id
            .as_deref()
            .is_some_and(|id| !canonical_uuid(id))
        || !["studio", "diagnostic"].contains(&record.origin.as_str())
        || ![
            "unknown",
            "queued",
            "running",
            "succeeded",
            "failed",
            "cancelled",
        ]
        .contains(&record.state.as_str())
        || record.app_version.len() > 64
        || record.app_version.chars().any(char::is_control)
        || record.revision == 0
        || record.revision > 9_007_199_254_740_991
        || record.admitted_at > record.observed_at
        || record.observed_at > 9_007_199_254_740_991
        || [record.started_at, record.generation_ms]
            .iter()
            .flatten()
            .any(|n| *n > 9_007_199_254_740_991)
        || record.output_root_identity.as_ref().is_some_and(|id| {
            id.len() > 41
                || !id.split_once(':').is_some_and(|(device, file)| {
                    device.parse::<u64>().is_ok() && file.parse::<u64>().is_ok()
                })
        })
        || record.output_root.is_some() != record.output_root_identity.is_some()
        || record.backend_origin.len() > 512
        || crate::validate_base_url(&record.backend_origin).is_err()
        || reqwest::Url::parse(&record.backend_origin)
            .is_ok_and(|url| url.origin().ascii_serialization() != record.backend_origin)
        || record
            .output_root
            .as_ref()
            .is_some_and(|r| r.len() > 32768 || r.contains('\0') || !Path::new(r).is_absolute())
        || record
            .error
            .as_ref()
            .is_some_and(|e| e.len() > 200 || e.chars().any(char::is_control))
        || record.outputs.len() > 64
        || record.outputs.iter().any(|o| {
            o.role
                .as_deref()
                .is_some_and(|role| !["original", "processed"].contains(&role))
                || o.output_type != "output"
                || o.filename.len() > 255
                || o.filename.contains('/')
                || !safe_path(&o.filename, false)
                || o.subfolder.len() > 255
                || !safe_path(&o.subfolder, true)
                || o.format.as_ref().is_some_and(|f| {
                    f.len() > 64
                        || !f
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"/.+-".contains(&b))
                })
        })
        || serde_json::to_vec(record)
            .map_err(|_| "History record cannot be encoded")?
            .len()
            > RECORD_BYTES
    {
        return Err("Invalid or oversized history record".into());
    }
    Ok(())
}

fn output_availability(root: &Path, output: &HistoryOutput) -> &'static str {
    let mut parent = root.to_path_buf();
    for part in output.subfolder.split('/').filter(|part| !part.is_empty()) {
        let Ok(anchor) = crate::projects::Root::open(&parent) else {
            return "unvalidated";
        };
        if anchor.absent(part).is_ok() {
            return "missing";
        }
        parent.push(part);
    }
    let Ok(anchor) = crate::projects::Root::open(&parent) else {
        return "unvalidated";
    };
    if anchor.absent(&output.filename).is_ok() {
        "missing"
    } else if anchor.file(&output.filename, false).is_ok() {
        "available"
    } else {
        "unvalidated"
    }
}

struct Writer {
    root: crate::projects::Root,
    _lock: File,
    previous: Option<Vec<u8>>,
    document: Document,
}
impl Writer {
    fn open(path: &Path) -> Result<Self, String> {
        ensure_directory(path)?;
        let root = crate::projects::Root::open(path)?;
        let lock = root.file(LOCK, true)?;
        lock.try_lock()
            .map_err(|_| "History already has a writer")?;
        let (document, previous) = load(&root)?;
        Ok(Self {
            root,
            _lock: lock,
            previous,
            document,
        })
    }
    fn save(&mut self, document: &Document) -> Result<(), String> {
        if document.schema_version != 1 || document.records.len() > LIMIT {
            return Err("Invalid history document".into());
        }
        self.root.verify()?;
        // Lock identity plus previous bytes catch cooperative and external competing writers.
        let current_lock = self.root.file(LOCK, false)?;
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::MetadataExt;
            let held = self
                ._lock
                .metadata()
                .map_err(|_| "Cannot inspect history writer")?;
            let current = current_lock
                .metadata()
                .map_err(|_| "Cannot inspect history writer")?;
            if (held.dev(), held.ino()) != (current.dev(), current.ino()) {
                return Err("History writer was replaced".into());
            }
        }
        #[cfg(windows)]
        if crate::win::identity(&self._lock)? != crate::win::identity(&current_lock)? {
            return Err("History writer was replaced".into());
        }
        if let Some(previous) = &self.previous {
            if &self.root.read(FILE, FILE_BYTES)? != previous {
                return Err("History changed outside this writer".into());
            }
        }
        for record in &document.records {
            validate_record(record)?;
        }
        let bytes = serde_json::to_vec(document).map_err(|_| "Cannot encode history")?;
        if bytes.len() as u64 > FILE_BYTES {
            return Err("History exceeds byte bound".into());
        }
        self.root
            .write_json_bytes(FILE, &bytes, self.previous.is_none())?;
        self.previous = Some(bytes);
        Ok(())
    }
}

// Create only through already anchored, trusted parents; never follow an existing link to mkdir.
fn ensure_directory(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("History app-data path must be absolute".into());
    }
    match std::fs::symlink_metadata(path) {
        Ok(_) => crate::downloads::fs::Anchor::open(path)?.writable(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or("Missing history parent")?;
            ensure_directory(parent)?;
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or("Invalid history directory")?;
            crate::downloads::fs::Anchor::open(parent)?.create(name)?;
            Ok(())
        }
        Err(_) => Err("Cannot inspect history directory".into()),
    }
}

fn load(root: &crate::projects::Root) -> Result<(Document, Option<Vec<u8>>), String> {
    let bytes = match root.read(FILE, FILE_BYTES) {
        Ok(bytes) => bytes,
        Err(error) => {
            if root.absent(FILE).is_ok() {
                return Ok((
                    Document {
                        schema_version: 1,
                        records: vec![],
                    },
                    None,
                ));
            }
            return Err(error);
        }
    };
    let document: Document = serde_json::from_slice(&bytes).map_err(|_| "Invalid history JSON")?;
    if document.schema_version != 1 || document.records.len() > LIMIT {
        return Err("Unsupported history schema".into());
    }
    let mut ids = std::collections::HashSet::new();
    for record in &document.records {
        validate_record(record)?;
        if !ids.insert(&record.id) {
            return Err("Duplicate historical identity".into());
        }
    }
    Ok((document, Some(bytes)))
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HistoryLogOperation {
    Read,
    Outputs,
}

/// A fixed UI transport/contract diagnostic; never accepts arbitrary frontend text.
#[tauri::command]
pub fn native_logs_history_error(operation: Option<HistoryLogOperation>) {
    static LAST: Mutex<[Option<std::time::Instant>; 2]> = Mutex::new([None; 2]);
    let index = usize::from(matches!(operation, Some(HistoryLogOperation::Outputs)));
    let mut last = LAST.lock().unwrap_or_else(|error| error.into_inner());
    if last[index].is_some_and(|at| at.elapsed() < Duration::from_secs(60)) {
        return;
    }
    last[index] = Some(std::time::Instant::now());
    let message = if index == 0 {
        "ERROR: origin=studio-history severity=error operation=read native history response unavailable (repeats limited to once per minute)"
    } else {
        "ERROR: origin=studio-history severity=error operation=outputs historical output availability could not be validated (repeats limited to once per minute)"
    };
    crate::logs::push(crate::logs::Source::Deas, message);
}

/// Read-only data. No historical endpoint/root is contacted, opened, adopted or granted.
#[tauri::command]
pub fn native_jobs_history(state: tauri::State<'_, Jobs>) -> HistoryView {
    state.3.view()
}

/// Explicit availability check under the currently approved environment's output authority.
#[tauri::command(async)]
pub fn native_jobs_history_outputs(
    state: tauri::State<'_, Jobs>,
    engine: tauri::State<'_, crate::install::Engine>,
    job_id: String,
) -> Result<Vec<&'static str>, String> {
    let result =
        crate::media::star::output_root(&engine).and_then(|root| state.3.outputs(&job_id, &root));
    result.map_err(|_| {
        native_logs_history_error(Some(HistoryLogOperation::Outputs));
        "Historical output availability could not be validated in the selected environment".into()
    })
}

#[cfg(test)]
#[path = "jobs_history_tests.rs"]
mod tests;
