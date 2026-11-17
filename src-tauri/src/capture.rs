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
const QUICK_KEY: &str = "capture_quick";
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

/// Whether a copied line is read as Ledger's quick-add reads what's typed ("Call Leo tomorrow
/// !high" due tomorrow, high); off unless turned on, so copied words stay as they were.
pub fn quick(conn: &Connection) -> Result<bool> {
    Ok(db::get_setting(conn, QUICK_KEY)?.as_deref() == Some("1"))
}

pub fn set_quick(conn: &Connection, on: bool) -> Result<()> {
    db::set_setting(conn, QUICK_KEY, if on { "1" } else { "0" })
}

/// Keeps `text` in Ledger as a task or a note, and says so; tells others (Ledger's window, other
/// computers) through the same paths as adding one by hand. A note kept during a focus session on
/// a task goes on that task, unless it already has one.
pub fn keep(app: Option<&AppHandle>, conn: &Connection, text: &str, now: i64) -> std::result::Result<Kept, String> {
    const NOTHING: &str = "Nothing to capture: copy some text first.";
    match kind_of(text) {
        None => Err(NOTHING.into()),
        Some(Kind::Task(title)) => {
            let task = if quick(conn).map_err(|e| e.to_string())? {
                crate::commands::add_quick(conn, &title, None)?
            } else {
                crate::tasks::add(conn, &title, now).map_err(|e| e.to_string())?
            }
            .ok_or(NOTHING)?;
            if let Some(app) = app {
                crate::commands::tasks_changed(app, conn)?;
            }
            Ok(Kept { title: "Added to Ledger".into(), body: task.title, opens: format!("task:{}", task.id) })
        }
        Some(Kind::Note(body)) => {
            let note = crate::notebook::create(conn, &body, now).map_err(|e| e.to_string())?;
            let on = focused_task(conn).map_err(|e| e.to_string())?;
            if let Some((task, _)) = &on {
                crate::tasks::set_note(conn, task, Some(&note.id)).map_err(|e| e.to_string())?;
                crate::sync::record(conn, &format!("tasknote:{task}"), &note.id)?;
            }
            if let Some(app) = app {
                crate::note_window::changed(app, &note.id, "capture", false);
                if on.is_some() {
                    crate::commands::tasks_changed(app, conn)?;
                }
            }
            let body = match on {
                Some((_, name)) => format!("{}, on “{name}”", note.title),
                None => note.title,
            };
            Ok(Kept { title: "Saved as a note".into(), body, opens: format!("note:{}", note.id) })
        }
    }
}

/// The task the running focus session is on, by id and title, when it has no note yet: one it
/// already has isn't swapped for what was copied.
fn focused_task(conn: &Connection) -> Result<Option<(String, String)>> {
    let Some(task) = crate::focus::active(conn)?.and_then(|a| a.task().map(str::to_string)) else { return Ok(None) };
    if crate::tasks::note_of(conn, &task)?.is_some() {
        return Ok(None);
    }
    Ok(crate::tasks::title(conn, &task)?.map(|title| (task, title)))
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

/// The clipboard's text; an image or nothing copied counts as nothing to capture, and a copy a
/// password manager marked is refused, so a password never lands in Ledger.
pub fn read(app: &AppHandle) -> std::result::Result<String, String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let text = app.clipboard().read_text().map_err(|_| "Nothing to capture: copy some text first.".to_string())?;
    if crate::platform::clipboard_concealed() {
        return Err(CONCEALED.into());
    }
    Ok(text)
}

/// Why a password manager's copy isn't kept.
pub const CONCEALED: &str = "That copy is from a password manager, so Kairo doesn't keep it.";

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
    fn with_quick_add_on_a_line_is_read_as_typed_into_ledger() {
        let c = conn();
        set_quick(&c, true).unwrap();
        let kept = keep(None, &c, "Book the dentist tomorrow !high", 1_000).unwrap();
        assert_eq!(kept.body, "Book the dentist");
        let t = &crate::tasks::list(&c, 2_000, &crate::tasks::today()).unwrap()[0];
        let tomorrow = (chrono::Local::now().date_naive() + chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
        assert_eq!((t.due_day.as_deref(), t.priority), (Some(tomorrow.as_str()), crate::tasks::HIGH));
        set_quick(&c, false).unwrap();
        assert!(!quick(&c).unwrap());
    }

    #[test]
    fn a_note_kept_during_focus_on_a_task_goes_on_that_task_unless_it_has_one() {
        let c = conn();
        let essay = crate::tasks::add(&c, "Essay", 1).unwrap().unwrap();
        crate::focus::start_by_hand(&c, 25, None, Some(&essay.id), 1_000).unwrap();
        let first = keep(None, &c, "Sources\n- Smith 2019\n- Lee 2021", 2_000).unwrap();
        assert_eq!(first.body, "Sources, on “Essay”");
        let id = first.opens.strip_prefix("note:").unwrap().to_string();
        assert_eq!(crate::tasks::note_of(&c, &essay.id).unwrap().as_deref(), Some(id.as_str()));
        // Its note stays: a second copy is a note of its own.
        let second = keep(None, &c, "More\nthoughts", 3_000).unwrap();
        assert_eq!(second.body, "More");
        assert_eq!(crate::tasks::note_of(&c, &essay.id).unwrap().as_deref(), Some(id.as_str()));
        // A line is still a task of its own, not put on the one in focus.
        assert_eq!(keep(None, &c, "Email Lee", 4_000).unwrap().title, "Added to Ledger");
    }

    #[test]
    fn its_keys_cant_be_the_palettes_or_the_rings() {
        assert_eq!(taken_by("ctrl+space", "Ctrl+Space", "Alt+Q").as_deref(), Some("ctrl+space opens the command palette."));
        assert_eq!(taken_by("Alt+Q", "Ctrl+Space", "Alt+Q").as_deref(), Some("Alt+Q opens the app ring."));
        assert_eq!(taken_by("Alt+Shift+C", "Ctrl+Space", "Alt+Q"), None);
    }
}
