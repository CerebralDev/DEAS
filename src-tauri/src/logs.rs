//! One app-wide in-memory log of DEAS messages and child process output (ComfyUI, uv/git/pip),
//! read incrementally by the Logs view. Bounded by bytes; nothing is written to disk unless exported.
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_BYTES: usize = 10 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Deas,
    Comfyui,
    Cli,
}

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub seq: u64,
    pub time: u64,
    pub source: Source,
    pub text: String,
}

#[derive(Default)]
struct Store {
    entries: VecDeque<Entry>,
    bytes: usize,
    next: u64,
}

static STORE: Mutex<Store> = Mutex::new(Store {
    entries: VecDeque::new(),
    bytes: 0,
    next: 0,
});

fn store() -> std::sync::MutexGuard<'static, Store> {
    STORE.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn push(source: Source, text: &str) {
    store().push(source, text);
}

impl Store {
    fn push(&mut self, source: Source, text: &str) {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        self.next += 1;
        let entry = Entry {
            seq: self.next,
            time,
            source,
            text: text.to_owned(),
        };
        self.bytes += entry.text.len();
        self.entries.push_back(entry);
        while self.bytes > MAX_BYTES {
            let Some(old) = self.entries.pop_front() else {
                break;
            };
            self.bytes -= old.text.len();
        }
    }
}

pub(crate) fn after(seq: u64) -> Vec<Entry> {
    let s = store();
    // Entries are seq-ordered: skip the already delivered prefix.
    let start = s.entries.partition_point(|e| e.seq <= seq);
    s.entries.range(start..).cloned().collect()
}

/// Fixed, path-free UI diagnostic; cannot append arbitrary frontend text.
#[tauri::command]
pub fn native_logs_people_image_error() {
    static LAST: Mutex<Option<std::time::Instant>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last.is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(60)) {
        return;
    }
    *last = Some(std::time::Instant::now());
    push(
        Source::Deas,
        "ERROR: Studio PeopleBoxes image-load: reference image could not be decoded; box editing disabled (repeats limited to once per minute)",
    );
}

/// Fixed reference-preview diagnostic; no media paths or frontend text cross this boundary.
#[tauri::command]
pub fn native_logs_reference_media_error() {
    static LAST: Mutex<Option<std::time::Instant>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last.is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(60)) {
        return;
    }
    *last = Some(std::time::Instant::now());
    push(
        Source::Deas,
        "ERROR: Studio ReferenceMedia preview/playback: reference media could not be decoded or played (repeats limited to once per minute)",
    );
}

/// Entries newer than `after` (0 = everything still retained).
#[tauri::command]
pub fn native_logs(after: u64) -> Vec<Entry> {
    self::after(after)
}

/// Save the given (already filtered) log text through an OS save dialog; cancel returns false.
#[tauri::command]
pub async fn native_logs_export(title: String, text: String) -> Result<bool, String> {
    if title.chars().count() > 240 || title.chars().any(char::is_control) {
        return Err("Invalid save dialog title".into());
    }
    if text.len() > MAX_BYTES * 2 {
        return Err("Log export exceeds the size limit".into());
    }
    let Some(file) = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter("Text", &["txt"])
        .set_file_name("deas-logs.txt")
        .save_file()
        .await
    else {
        return Ok(false);
    };
    std::fs::write(file.path(), text).map_err(|e| format!("Cannot write log export: {e}"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn people_image_error_is_fixed_shared_and_rate_limited() {
        native_logs_people_image_error();
        native_logs_people_image_error();
        let entries = native_logs(0);
        let reports: Vec<_> = entries
            .iter()
            .filter(|e| e.text.starts_with("ERROR: Studio PeopleBoxes image-load:"))
            .collect();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].source, Source::Deas);
        assert!(!reports[0].text.contains("private.png"));
    }

    #[test]
    fn reference_media_error_is_fixed_shared_and_rate_limited() {
        native_logs_reference_media_error();
        native_logs_reference_media_error();
        let entries = native_logs(0);
        let reports: Vec<_> = entries
            .iter()
            .filter(|e| {
                e.text
                    .starts_with("ERROR: Studio ReferenceMedia preview/playback:")
            })
            .collect();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].source, Source::Deas);
        assert!(!reports[0].text.contains("private.mp4"));
    }

    #[test]
    fn incremental_reads_and_byte_cap() {
        push(Source::Deas, "first");
        let seq = after(0).last().unwrap().seq;
        push(Source::Comfyui, "second");
        // Other tests share the global store: only assert on this test's own entries.
        assert!(
            after(seq)
                .iter()
                .any(|e| e.text == "second" && e.source == Source::Comfyui)
        );
        assert!(after(seq).iter().all(|e| e.text != "first"));
    }

    #[test]
    fn byte_cap_evicts_oldest_entries() {
        // A private store: overflowing the shared one would evict entries other tests are reading.
        let mut s = Store::default();
        s.push(Source::Deas, "first");
        let line = "x".repeat(1024 * 1024);
        for _ in 0..12 {
            s.push(Source::Cli, &line);
        }
        assert!(s.bytes <= MAX_BYTES);
        assert_eq!(
            s.bytes,
            s.entries.iter().map(|e| e.text.len()).sum::<usize>()
        );
        assert!(s.entries.iter().all(|e| e.seq > 1));
    }
}
