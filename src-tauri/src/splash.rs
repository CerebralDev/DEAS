//! Native startup splash: shows real startup phases while the main window initializes hidden,
//! then hands off once the UI state is restored. It never starts or probes the backend.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Phase keys the splash page knows (it starts on "shell"); anything else is refused.
const PHASES: [&str; 2] = ["interface", "restore"];
/// A main page that never reports ready is shown anyway (no stuck splash).
const HANDOFF_DEADLINE: Duration = Duration::from_secs(30);
/// A fresh start (nothing remembered) reports ready within milliseconds; keep the splash readable.
const MIN_VISIBLE: Duration = Duration::from_millis(1500);
static HANDED_OFF: AtomicBool = AtomicBool::new(false);
static OPENED: OnceLock<Instant> = OnceLock::new();

pub fn open<R: Runtime>(app: &tauri::App<R>) {
    let _ = OPENED.set(Instant::now());
    // A splash failure must never block startup: the deadline below still reveals the main window.
    let _ = WebviewWindowBuilder::new(app, "splash", WebviewUrl::App("splash.html".into()))
        .title("DEAS")
        .inner_size(440.0, 320.0)
        .resizable(false)
        .decorations(false)
        .skip_taskbar(true)
        .center()
        // Paint the splash colour before the page loads (no white flash).
        .background_color(tauri::window::Color(23, 27, 30, 255))
        .build();
    let handle = app.handle().clone();
    std::thread::spawn(move || {
        std::thread::sleep(HANDOFF_DEADLINE);
        hand_off(&handle);
    });
}

pub fn phase<R: Runtime>(app: &AppHandle<R>, phase: &str) {
    if !PHASES.contains(&phase) || HANDED_OFF.load(Ordering::SeqCst) {
        return;
    }
    if let Some(splash) = app.get_webview_window("splash") {
        let _ = splash.eval(format!("window.deasPhase&&window.deasPhase({phase:?})"));
    }
}

/// Idempotent: the first caller (ready signal or deadline) reveals main and closes the splash.
pub fn hand_off<R: Runtime>(app: &AppHandle<R>) {
    if HANDED_OFF.swap(true, Ordering::SeqCst) {
        return;
    }
    let remaining = OPENED
        .get()
        .map_or(Duration::ZERO, |t| MIN_VISIBLE.saturating_sub(t.elapsed()));
    if remaining.is_zero() {
        reveal(app);
        return;
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(remaining);
        reveal(&handle);
    });
}

fn reveal<R: Runtime>(app: &AppHandle<R>) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.set_focus();
    }
    if let Some(splash) = app.get_webview_window("splash") {
        let _ = splash.close();
    }
}

#[tauri::command]
pub fn native_startup_phase<R: Runtime>(
    window: WebviewWindow<R>,
    phase: String,
) -> Result<(), String> {
    if window.label() != "main" || !PHASES.contains(&phase.as_str()) {
        return Err("Invalid startup phase".into());
    }
    self::phase(window.app_handle(), &phase);
    Ok(())
}

#[tauri::command]
pub fn native_startup_ready<R: Runtime>(window: WebviewWindow<R>) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Invalid startup window".into());
    }
    hand_off(window.app_handle());
    Ok(())
}
