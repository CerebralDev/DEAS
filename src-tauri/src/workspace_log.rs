//! Bounded shared diagnostics; no Chat or library dependency.
use std::collections::BTreeMap;
#[tauri::command]
pub fn native_logs_workspace_error(operation: String) {
    use std::sync::Mutex;
    static LAST: Mutex<BTreeMap<String, std::time::Instant>> = Mutex::new(BTreeMap::new());
    if !["context", "suggestion", "library", "assets", "monitoring"].contains(&operation.as_str()) {
        return;
    }
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last
        .get(&operation)
        .is_some_and(|at| at.elapsed().as_secs() < 60)
    {
        return;
    }
    last.insert(operation.clone(), std::time::Instant::now());
    crate::logs::push(crate::logs::Source::Deas,&format!("ERROR: workspace {operation}: operation failed; details shown in the affected pane (repeats limited to once per minute)"));
}
