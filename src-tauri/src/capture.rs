//! Capture: what was copied, kept in Ledger from any window. A line short enough to be a task's
//! title becomes a task; anything longer, or of more than one line, a note. It reads only what's
//! already on the clipboard: Kairo never sends a copy keystroke or reads a selection.
//!
//! It's reached by its own global shortcut, set in Kairo's settings and off until then (a
//! shortcut taken by default would take those keys from every other app), by the palette's
//! "Capture what I copied", and by `kairo --capture`, for desktops (Wayland) where apps can't
//! listen for keys and the desktop's own shortcut runs it.

use rusqlite::{Connection, Result};
use tauri::AppHandle;

use crate::db;

const SHORTCUT_KEY: &str = "capture_shortcut";
/// The longest single line kept as a task; a longer one is a note. A task's title is cut there.
const TASK_CHARS: usize = 80;

/// What a copy becomes.
#[derive(Debug, PartialEq, Eq)]
pub enum Kind {
    Task(String),
    Note(String),
}

/// What `text` becomes: a short single line a task with that title, anything else a note with all
/// of it; nothing for text that's blank.
pub fn kind_of(text: &str) -> Option<Kind> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let line = !text.contains('\n') && text.chars().count() <= TASK_CHARS;
    Some(if line { Kind::Task(text.to_string()) } else { Kind::Note(text.to_string()) })
}

/// Something kept: the notification's words, and where a click on it opens Ledger ("task:<id>").
#[derive(Debug, PartialEq, Eq)]
pub struct Kept {
    pub title: String,
    pub body: String,
    pub opens: String,
}

/// Keeps `text` in Ledger as a task or a note, and says so; tells others (Ledger's window, other
/// computers) through the same paths as adding one by hand.
pub fn keep(app: Option<&AppHandle>, conn: &Connection, text: &str, now: i64) -> std::result::Result<Kept, String> {
    match kind_of(text) {
        None => Err("Nothing to capture: copy some text first.".into()),
        Some(Kind::Task(title)) => {
            // Taken as it is: copied words aren't quick-add text, so "tomorrow" in them stays a word.
            let task = crate::tasks::add(conn, &title, now).map_err(|e| e.to_string())?.ok_or("Nothing to capture: copy some text first.")?;
            if let Some(app) = app {
                crate::commands::tasks_changed(app, conn)?;
            }
            Ok(Kept { title: "Added to Ledger".into(), body: task.title, opens: format!("task:{}", task.id) })
        }
        Some(Kind::Note(body)) => {
            let note = crate::notebook::create(conn, &body, now).map_err(|e| e.to_string())?;
            if let Some(app) = app {
                crate::note_window::changed(app, &note.id, "capture", false);
            }
            Ok(Kept { title: "Saved as a note".into(), body: note.title, opens: format!("note:{}", note.id) })
        }
    }
}

/// Reads the clipboard's text and keeps it, from the shortcut or `--capture`: the outcome is said
/// in a notification, which opens what was kept when clicked.
pub fn from_keys(app: &AppHandle) {
    use tauri::Manager;
    let text = read(app);
    let Some(state) = app.try_state::<crate::AppState>() else { return };
    let kept = match text {
        Ok(text) => {
            let conn = state.db.lock().unwrap_or_else(|e| e.into_inner());
            keep(Some(app), &conn, &text, now_ms())
        }
        Err(why) => Err(why),
    };
    match kept {
        Ok(k) => tell(app, &k.title, &k.body, Some(&k.opens)),
        Err(why) => tell(app, "Kairo", &why, None),
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// The clipboard's text; an image or nothing copied counts as nothing to capture.
pub fn read(app: &AppHandle) -> std::result::Result<String, String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().read_text().map_err(|_| "Nothing to capture: copy some text first.".to_string())
}

/// A notification; with `opens`, a click on it opens Ledger there (reminders::follow_clicks).
pub fn tell(app: &AppHandle, title: &str, body: &str, opens: Option<&str>) {
    use tauri_plugin_notification::NotificationExt;
    let mut n = app.notification().builder().title(title).body(body);
    if let Some(at) = opens {
        n = n.extra(crate::reminders::OPENS, at);
    }
    if let Err(e) = n.show() {
        eprintln!("capture: {e}");
    }
}

/// The capture shortcut as saved: empty while there is none.
pub fn shortcut(conn: &Connection) -> Result<String> {
    Ok(db::get_setting(conn, SHORTCUT_KEY)?.unwrap_or_default())
}

/// The keys as last set, so the shortcut handler can tell them from the palette's and the ring's.
static KEYS: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

pub fn remember_keys(keys: &str) {
    *KEYS.lock().unwrap_or_else(|e| e.into_inner()) = keys.to_string();
}

/// Whether a global shortcut pressed is capture's.
pub fn is_capture(pressed: &tauri_plugin_global_shortcut::Shortcut) -> bool {
    let keys = KEYS.lock().unwrap_or_else(|e| e.into_inner());
    !keys.is_empty() && crate::ring::same_keys(&keys, pressed)
}

/// Why `new` can't be capture's keys: they're the palette's or the ring's. None when they're free.
pub fn taken_by(new: &str, palette: &str, ring: &str) -> Option<String> {
    if new.eq_ignore_ascii_case(palette) {
        Some(format!("{new} opens the command palette."))
    } else if new.eq_ignore_ascii_case(ring) {
        Some(format!("{new} opens the app ring."))
    } else {
        None
    }
}

/// Sets capture's keys, or with "" takes them away. If the new ones can't be had the old ones stay.
pub fn change_shortcut(app: &AppHandle, conn: &Connection, new: &str) -> std::result::Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let old = shortcut(conn).map_err(|e| e.to_string())?;
    let palette = crate::palette::shortcut(conn).map_err(|e| e.to_string())?;
    let ring = crate::ring::shortcut(conn).map_err(|e| e.to_string())?;
    if !new.is_empty() {
        if let Some(why) = taken_by(new, &palette, &ring) {
            return Err(why);
        }
    }
    let keys = app.global_shortcut();
    if !old.is_empty() {
        let _ = keys.unregister(old.as_str());
    }
    if !new.is_empty() {
        if let Err(e) = keys.register(new) {
            if !old.is_empty() {
                let _ = keys.register(old.as_str());
            }
            return Err(format!("{new} can't be used: {e}. Another app may already use it."));
        }
    }
    db::set_setting(conn, SHORTCUT_KEY, new).map_err(|e| e.to_string())?;
    remember_keys(new);
    Ok(())
}

/// Takes the saved keys at startup; a failure leaves capture to the palette until they're changed.
pub fn register_saved(app: &AppHandle, keys: &str) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    remember_keys(keys);
    if !keys.is_empty() {
        if let Err(e) = app.global_shortcut().register(keys) {
            eprintln!("capture: cannot use {keys}: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        db::open(std::path::Path::new(":memory:")).unwrap()
    }

    #[test]
    fn a_short_line_is_a_task_and_anything_longer_a_note() {
        assert_eq!(kind_of("  Call Leo about the invoice \n"), Some(Kind::Task("Call Leo about the invoice".into())));
        assert_eq!(kind_of(&"x".repeat(80)), Some(Kind::Task("x".repeat(80))));
        assert_eq!(kind_of(&"x".repeat(81)), Some(Kind::Note("x".repeat(81))));
        assert_eq!(kind_of("Groceries\nmilk\neggs"), Some(Kind::Note("Groceries\nmilk\neggs".into())));
        assert_eq!(kind_of(" \n\t "), None);
    }

    #[test]
    fn keeps_a_task_as_written_and_a_note_whole_and_says_where_each_opens() {
        let c = conn();
        let task = keep(None, &c, "Book the dentist tomorrow !high", 1_000).unwrap();
        assert_eq!((task.title.as_str(), task.body.as_str()), ("Added to Ledger", "Book the dentist tomorrow !high"));
        let listed = crate::tasks::list(&c, 2_000, &crate::tasks::today()).unwrap();
        assert_eq!(listed.len(), 1);
        // Copied words aren't quick-add text: no due day or priority read into them.
        assert_eq!((listed[0].due_day.as_deref(), listed[0].title.as_str()), (None, "Book the dentist tomorrow !high"));
        assert_eq!(task.opens, format!("task:{}", listed[0].id));

        let note = keep(None, &c, "Meeting notes\n- ship Friday\n- ask Marta", 1_000).unwrap();
        assert_eq!((note.title.as_str(), note.body.as_str()), ("Saved as a note", "Meeting notes"));
        let id = note.opens.strip_prefix("note:").unwrap();
        assert_eq!(crate::notebook::get(&c, id).unwrap().unwrap().body, "Meeting notes\n- ship Friday\n- ask Marta");

        assert_eq!(keep(None, &c, "   ", 1_000), Err("Nothing to capture: copy some text first.".into()));
    }

    #[test]
    fn its_keys_cant_be_the_palettes_or_the_rings() {
        assert_eq!(taken_by("ctrl+space", "Ctrl+Space", "Alt+Q").as_deref(), Some("ctrl+space opens the command palette."));
        assert_eq!(taken_by("Alt+Q", "Ctrl+Space", "Alt+Q").as_deref(), Some("Alt+Q opens the app ring."));
        assert_eq!(taken_by("Alt+Shift+C", "Ctrl+Space", "Alt+Q"), None);
    }
}
