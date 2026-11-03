//! A Ledger note popped out into a small window of its own, to keep in view beside other apps,
//! or the list of notes to pick one from. One note at a time: popping out another switches the
//! window to it. Like Kairo's window, it
//! isn't kept while closed, since its web view is most of what it costs.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::AppState;

pub(crate) const LABEL: &str = "note";
/// Whether the window floats above others: on unless turned off.
const ON_TOP: &str = "note_window_on_top";

/// The note the window is on, for its page to ask once it's loaded; none for the list.
static POPPED: Mutex<Option<String>> = Mutex::new(None);

/// Told to every window when a note changes: those showing it, other than `from`, take it.
#[derive(Clone, Serialize)]
pub(crate) struct NoteChange {
    pub id: String,
    /// The label of the window it was changed in.
    pub from: String,
    /// Deleted.
    pub gone: bool,
}

pub(crate) fn changed(app: &AppHandle, id: &str, from: &str, gone: bool) {
    let _ = app.emit("kairo:note", NoteChange { id: id.to_string(), from: from.to_string(), gone });
}

pub(crate) fn popped() -> Option<String> {
    POPPED.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub(crate) fn on_top(app: &AppHandle) -> bool {
    app.try_state::<AppState>()
        .and_then(|state| state.db.lock().ok().and_then(|conn| crate::db::get_setting(&conn, ON_TOP).ok().flatten()))
        .is_none_or(|v| v != "0")
}

pub(crate) fn set_on_top(app: &AppHandle, on: bool) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        crate::db::set_setting(&conn, ON_TOP, if on { "1" } else { "0" }).map_err(|e| e.to_string())?;
    }
    if let Some(window) = app.get_webview_window(LABEL) {
        window.set_always_on_top(on).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// What popping out a note does, given whether the window is already open.
#[derive(Debug, PartialEq)]
enum Pop {
    /// Tell the open window's page to switch to the note.
    Switch,
    /// Make the window; its page asks which note once loaded.
    Open,
}

fn pop_for(window_open: bool) -> Pop {
    if window_open {
        Pop::Switch
    } else {
        Pop::Open
    }
}

/// Shows the note `id` in the pop-out window, or the list of notes for none, making the window if
/// it's closed.
pub(crate) fn pop_out(app: &AppHandle, id: Option<&str>) {
    *POPPED.lock().unwrap_or_else(|e| e.into_inner()) = id.map(str::to_string);
    crate::in_dock(app, true);
    match pop_for(app.get_webview_window(LABEL).is_some()) {
        Pop::Switch => {
            let _ = app.emit_to(LABEL, "kairo:note-open", id);
            if let Some(window) = app.get_webview_window(LABEL) {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
        Pop::Open => {
            // As with Kairo's window: made on a thread of its own, since making a web view from
            // inside one of Windows' callbacks can hang.
            if !crate::begin_opening(LABEL) {
                return;
            }
            let app = app.clone();
            std::thread::spawn(move || {
                if let Err(e) = open(&app) {
                    eprintln!("cannot pop out the note: {e}");
                }
                crate::end_opening(LABEL);
            });
        }
    }
}

fn open(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("note.html".into()))
        .title("Note")
        .inner_size(360.0, 480.0)
        .min_inner_size(260.0, 200.0)
        .always_on_top(on_top(app))
        .disable_drag_drop_handler()
        .visible(false)
        .build()?;
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
    fn an_open_window_switches_a_closed_one_opens() {
        assert_eq!(pop_for(true), Pop::Switch);
        assert_eq!(pop_for(false), Pop::Open);
    }
}
