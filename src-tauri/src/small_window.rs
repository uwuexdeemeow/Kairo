//! Kairo's small windows beside its main one: a popped-out note, the scan window. Each is one
//! window, made when asked for and not kept while closed, since its web view is most of what it
//! costs; asked for again while open, it's told what for and brought forward.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::AppState;

/// What a small window is.
#[derive(Clone, Copy)]
pub(crate) struct Spec {
    pub label: &'static str,
    /// Its page.
    pub file: &'static str,
    pub title: &'static str,
    pub size: (f64, f64),
    pub min: (f64, f64),
    pub on_top: bool,
    /// Files dropped on it reach the page with their paths, rather than the web view opening them.
    pub takes_files: bool,
}

/// What asking for a small window does, given whether it's open.
#[derive(Debug, PartialEq)]
enum Ask {
    /// Tell its page, and bring it forward.
    Tell,
    /// Make it; its page asks what for once loaded.
    Open,
}

fn ask_for(window_open: bool) -> Ask {
    if window_open {
        Ask::Tell
    } else {
        Ask::Open
    }
}

/// Brings up `spec`'s window: an open one hears `event` with `payload`; a closed one is made.
pub(crate) fn show<P: Serialize + Clone>(app: &AppHandle, spec: Spec, event: &str, payload: P) {
    crate::in_dock(app, true);
    match ask_for(app.get_webview_window(spec.label).is_some()) {
        Ask::Tell => {
            let _ = app.emit_to(spec.label, event, payload);
            if let Some(window) = app.get_webview_window(spec.label) {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
        Ask::Open => {
            // As with Kairo's window: made on a thread of its own, since making a web view from
            // inside one of Windows' callbacks can hang.
            if !crate::begin_opening(spec.label) {
                return;
            }
            let app = app.clone();
            std::thread::spawn(move || {
                if let Err(e) = open(&app, spec) {
                    eprintln!("cannot open the {} window: {e}", spec.label);
                }
                crate::end_opening(spec.label);
            });
        }
    }
}

fn open(app: &AppHandle, spec: Spec) -> tauri::Result<()> {
    let mut builder = WebviewWindowBuilder::new(app, spec.label, WebviewUrl::App(spec.file.into()))
        .title(spec.title)
        .inner_size(spec.size.0, spec.size.1)
        .min_inner_size(spec.min.0, spec.min.1)
        .always_on_top(spec.on_top)
        .visible(false);
    if !spec.takes_files {
        builder = builder.disable_drag_drop_handler();
    }
    let window = builder.build()?;
    let mode = app
        .try_state::<AppState>()
        .and_then(|state| state.db.lock().ok().and_then(|conn| crate::appearance::get(&conn).ok()))
        .unwrap_or_default();
    crate::appearance::apply_to(&window, &mode);
    window.show()?;
    window.set_focus()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_open_window_is_told_a_closed_one_is_made() {
        assert_eq!(ask_for(true), Ask::Tell);
        assert_eq!(ask_for(false), Ask::Open);
    }
}
