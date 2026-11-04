//! The scan window: files, links and hashes checked against VirusTotal (`virustotal`). Opened
//! from the palette, with what was typed after `scan` or `vt` to look up straight away.

use std::sync::Mutex;

use tauri::AppHandle;

pub(crate) const LABEL: &str = "scan";

/// What to look up as the window opens, for its page to ask once it's loaded.
static ASKED: Mutex<Option<String>> = Mutex::new(None);

/// Brings up the scan window, looking up `query` (a link or hash) when there is one.
pub(crate) fn open(app: &AppHandle, query: Option<&str>) {
    let query = query.map(str::trim).filter(|q| !q.is_empty());
    *ASKED.lock().unwrap_or_else(|e| e.into_inner()) = query.map(str::to_string);
    let spec = crate::small_window::Spec {
        label: LABEL,
        file: "scan.html",
        title: "VirusTotal",
        size: (380.0, 480.0),
        min: (300.0, 360.0),
        on_top: false,
        // Files dropped on it come with their paths, to be hashed here.
        takes_files: true,
    };
    crate::small_window::show(app, spec, "kairo:scan", query);
}

/// What the window was opened to look up, once.
#[tauri::command]
pub fn scan_take() -> Option<String> {
    ASKED.lock().unwrap_or_else(|e| e.into_inner()).take()
}

#[tauri::command]
pub fn open_scan(app: AppHandle, query: Option<String>) {
    open(&app, query.as_deref());
}
