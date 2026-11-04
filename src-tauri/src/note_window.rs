//! A Ledger note popped out into a small window of its own, to keep in view beside other apps,
//! or the list of notes to pick one from. One note at a time: popping out another switches the
//! window to it. Made and shown as Kairo's other small windows are (`small_window`).

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

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

/// Shows the note `id` in the pop-out window, or the list of notes for none, making the window if
/// it's closed.
pub(crate) fn pop_out(app: &AppHandle, id: Option<&str>) {
    *POPPED.lock().unwrap_or_else(|e| e.into_inner()) = id.map(str::to_string);
    let spec = crate::small_window::Spec {
        label: LABEL,
        file: "note.html",
        title: "Note",
        size: (360.0, 480.0),
        min: (260.0, 200.0),
        on_top: on_top(app),
        takes_files: false,
    };
    crate::small_window::show(app, spec, "kairo:note-open", id);
}
