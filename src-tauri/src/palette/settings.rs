//! Every setting of Chrono and the dashboard, listed once so the palette can find them by name or
//! by the words people use for them, flip the switches among them, and jump to the rest.

use rusqlite::Connection;
use tauri::{AppHandle, Emitter};

use super::search::{fuzzy, Action, Glyph, Icon, Item, Tone};
use crate::{blocking, evening, nudge, startup};

/// A setting that is on or off, which the palette can flip without opening a window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Switch {
    Nudges,
    Evening,
    Blocking,
    Launchers,
    Suggestions,
    OverFullscreen,
    SearchFiles,
    Autostart,
}

/// Which settings page an entry is on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Page {
    Chrono,
    Kairo,
}

/// A setting as the palette knows it.
pub struct Entry {
    pub title: &'static str,
    /// What else it's found by, beyond its title.
    pub words: &'static str,
    pub page: Page,
    /// Its control's element id, without the `#`; also its id in `Action::Setting`.
    pub control: &'static str,
    pub switch: Option<Switch>,
    /// What Enter does instead of jumping, for the entries that are really actions.
    pub runs: Option<Action>,
}

/// What Start at login is called on this platform, as its page calls it.
pub const AUTOSTART_TITLE: &str = if cfg!(windows) {
    "Start with Windows"
} else if cfg!(target_os = "macos") {
    "Open at login"
} else {
    "Start at login"
};

fn chrono(title: &'static str, words: &'static str, control: &'static str, switch: Option<Switch>) -> Entry {
    Entry { title, words, page: Page::Chrono, control, switch, runs: None }
}

fn kairo(title: &'static str, words: &'static str, control: &'static str, switch: Option<Switch>, runs: Option<Action>) -> Entry {
    Entry { title, words, page: Page::Kairo, control, switch, runs }
}

/// Every setting there is.
pub fn entries() -> Vec<Entry> {
    use Switch::*;
    vec![
        chrono("Daily productive goal", "goal target hours day productive", "goal-productive", None),
        chrono("Daily distracting limit", "goal limit cap hours day distracting", "goal-distracting", None),
        chrono("Weekly productive goal", "goal target hours week productive", "goal-week-productive", None),
        chrono("Weekly distracting limit", "goal limit cap hours week distracting", "goal-week-distracting", None),
        chrono("Keep time for", "retention history how long delete old data", "retention", None),
        chrono("Delete time", "erase remove clear range data history", "delete-range", None),
        chrono("Projects", "project goals name track", "project-name", None),
        chrono("Calendar", "ical meetings link url events", "calendar-urls", None),
        chrono("Browser extension", "chrome firefox edge sites tabs", "ext-open", None),
        chrono("Focus nudges", "nudge notification heads up reminder", "nudges-enabled", Some(Nudges)),
        chrono("Evening summary", "recap end of day notification time", "evening-enabled", Some(Evening)),
        chrono("Focus blocking", "block distracting apps sites session", "block-enabled", Some(Blocking)),
        chrono("Block launchers too", "game steam epic launcher close", "block-launchers", Some(Launchers)),
        chrono("Strict blocking", "strict mode lock session", "block-strict", None),
        chrono("Breaks", "break rest reminder every", "break-every", None),
        chrono("Demo data", "sample example fake try", "demo-load", None),
        kairo("Kairo settings", "preferences options dashboard", "appearance", None, Some(Action::Dashboard(Some("settings")))),
        kairo("Theme", "appearance dark mode light system colors", "appearance", None, None),
        kairo("Palette shortcut", "command palette keys hotkey keyboard", "palette-record", None, None),
        kairo("Palette suggestions", "suggested recent apps when it opens", "palette-suggestions", Some(Suggestions), None),
        kairo("Open over full screen", "fullscreen games palette", "palette-over-fullscreen", Some(OverFullscreen), None),
        kairo("Search files", "documents folders index palette", "palette-search-files", Some(SearchFiles), None),
        kairo(AUTOSTART_TITLE, "start at login open at login start with windows startup boot sign in", "autostart-enabled", Some(Autostart), None),
        kairo("Export time as CSV", "export spreadsheet data download time", "export-time", None, Some(Action::ExportTime)),
        kairo("Back up everything", "backup back up save data copy", "backup-save", None, Some(Action::Backup)),
        kairo("Restore a backup", "restore load data", "backup-restore", None, Some(Action::Restore)),
        kairo("Combine computers", "sync folder share devices", "sync-choose", None, None),
        kairo("Check for updates", "update version upgrade new release", "update-check", None, Some(Action::CheckUpdates)),
    ]
}

/// The settings a query finds, best first. A switch shows whether it's on, and Enter flips it;
/// any other Enters its action or jumps to it; Ctrl/Command+Enter always jumps.
pub fn search(query: &str, switches: &[(Switch, bool)]) -> Vec<Item> {
    let mut items: Vec<Item> = entries()
        .into_iter()
        .filter_map(|e| {
            // The better of the title and the words for it, as for actions.
            let m = [fuzzy(query, e.title), fuzzy(query, e.words)].into_iter().flatten().max_by(|a, b| a.score.total_cmp(&b.score))?;
            let jump = Action::Setting(e.control);
            let action = match e.switch {
                Some(switch) => Action::Toggle(switch),
                None => e.runs.clone().unwrap_or_else(|| jump.clone()),
            };
            let mut item = Item::new(e.title, Icon::Glyph(Glyph::Settings, Tone::Plain), "Settings", action);
            item.at = fuzzy(query, e.title).map(|m| m.at).unwrap_or_default();
            item.sub = Some(match e.page {
                Page::Chrono => "Chrono settings".into(),
                Page::Kairo => "Kairo settings".into(),
            });
            item.detail = e.switch.map(|s| if switches.iter().any(|&(k, on)| k == s && on) { "On" } else { "Off" }.to_string());
            item.score = m.score;
            item.second = Some((jump, "Open in settings"));
            Some(item)
        })
        .collect();
    items.sort_by(|a, b| b.score.total_cmp(&a.score));
    items
}

/// Whether a switch is on, from what's saved. Start at login lives with the system, not here.
fn stored(conn: &Connection, switch: Switch) -> rusqlite::Result<bool> {
    Ok(match switch {
        Switch::Nudges => nudge::enabled(conn)?,
        Switch::Evening => evening::settings(conn)?.enabled,
        Switch::Blocking => blocking::settings(conn)?.enabled,
        Switch::Launchers => blocking::settings(conn)?.launchers,
        Switch::Suggestions => super::suggestions(conn)?,
        Switch::OverFullscreen => super::over_fullscreen(conn)?,
        Switch::SearchFiles => super::search_files(conn)?,
        Switch::Autostart => false,
    })
}

const STORED: [Switch; 7] = [Switch::Nudges, Switch::Evening, Switch::Blocking, Switch::Launchers, Switch::Suggestions, Switch::OverFullscreen, Switch::SearchFiles];

/// Every switch except Start at login, from what's saved.
fn read_stored(conn: &Connection) -> Vec<(Switch, bool)> {
    STORED.iter().map(|&s| (s, stored(conn, s).unwrap_or(false))).collect()
}

/// Every switch and whether it's on.
pub fn read(app: &AppHandle, conn: &Connection) -> Vec<(Switch, bool)> {
    let mut all = read_stored(conn);
    all.push((Switch::Autostart, startup::is_enabled(app)));
    all
}

/// Flips a saved switch through the code the settings pages use, and says what it is now.
pub(super) fn flip_stored(conn: &Connection, switch: Switch) -> Result<bool, String> {
    let err = |e: rusqlite::Error| e.to_string();
    let on = !stored(conn, switch).map_err(err)?;
    match switch {
        Switch::Nudges => nudge::set_enabled(conn, on).map_err(err)?,
        Switch::Evening => evening::set(conn, on, evening::settings(conn).map_err(err)?.minute).map_err(err)?,
        // Otherwise switching blocking off would be the easy way around strict mode.
        Switch::Blocking | Switch::Launchers if blocking::strict_now(conn).map_err(err)? => return Err(blocking::STRICT_REFUSAL.into()),
        Switch::Blocking => drop(blocking::set_setting(conn, "enabled", on).map_err(err)?),
        Switch::Launchers => drop(blocking::set_setting(conn, "launchers", on).map_err(err)?),
        Switch::Suggestions => super::set_suggestions(conn, on).map_err(err)?,
        Switch::OverFullscreen => super::set_over_fullscreen(conn, on).map_err(err)?,
        Switch::SearchFiles => super::set_search_files(conn, on).map_err(err)?,
        Switch::Autostart => return Err("Start at login is the system's to change.".into()),
    }
    Ok(on)
}

/// Flips a switch and says what it is now; then tells open settings pages to read theirs again.
pub fn flip(app: &AppHandle, conn: &Connection, switch: Switch) -> Result<bool, String> {
    let on = match switch {
        Switch::Autostart => {
            let on = !startup::is_enabled(app);
            startup::set_enabled(app, on)?;
            on
        }
        _ => flip_stored(conn, switch)?,
    };
    let _ = app.emit("kairo:settings", ());
    Ok(on)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn memory() -> Connection {
        crate::db::open(Path::new(":memory:")).unwrap()
    }

    #[test]
    fn every_entry_is_found_by_its_title() {
        for e in entries() {
            assert!(search(e.title, &[]).iter().any(|i| i.title == e.title), "{}", e.title);
        }
    }

    #[test]
    fn words_find_settings_their_titles_dont_name() {
        assert_eq!(search("recap", &[])[0].title, "Evening summary");
        assert_eq!(search("dark mode", &[])[0].title, "Theme");
        assert_eq!(search("login", &[])[0].title, AUTOSTART_TITLE);
        // Whatever the platform calls it, all three phrases find it.
        for phrase in ["start at login", "open at login", "start with windows"] {
            assert_eq!(search(phrase, &[])[0].title, AUTOSTART_TITLE, "{phrase}");
        }
    }

    #[test]
    fn switches_show_their_state_and_flip_on_enter() {
        let row = &search("nudges", &[(Switch::Nudges, true)])[0];
        assert_eq!(row.detail.as_deref(), Some("On"));
        assert_eq!(row.action, Action::Toggle(Switch::Nudges));
        assert_eq!(row.second, Some((Action::Setting("nudges-enabled"), "Open in settings")));
        assert_eq!((row.section, row.sub.as_deref()), ("Settings", Some("Chrono settings")));
        let off = &search("nudges", &[(Switch::Nudges, false)])[0];
        assert_eq!(off.detail.as_deref(), Some("Off"));
    }

    #[test]
    fn strict_blocking_and_demo_data_only_jump() {
        let strict = &search("strict", &[])[0];
        assert_eq!((strict.action.clone(), strict.detail.clone()), (Action::Setting("block-strict"), None));
        let demo = &search("demo", &[])[0];
        assert_eq!(demo.action, Action::Setting("demo-load"));
    }

    #[test]
    fn actions_keep_running_and_gain_a_jump() {
        let backup = &search("back up", &[])[0];
        assert_eq!(backup.action, Action::Backup);
        assert_eq!(backup.second, Some((Action::Setting("backup-save"), "Open in settings")));
        assert_eq!(search("export", &[])[0].action, Action::ExportTime);
        assert_eq!(search("restore", &[])[0].action, Action::Restore);
        assert_eq!(search("check for updates", &[])[0].action, Action::CheckUpdates);
        assert_eq!(search("kairo settings", &[])[0].action, Action::Dashboard(Some("settings")));
    }

    #[test]
    fn every_control_is_on_its_page() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let chrono = std::fs::read_to_string(root.join("index.html")).unwrap();
        let kairo = std::fs::read_to_string(root.join("dashboard.html")).unwrap();
        for e in entries() {
            let page = match e.page {
                Page::Chrono => &chrono,
                Page::Kairo => &kairo,
            };
            assert!(page.contains(&format!("id=\"{}\"", e.control)), "{} is missing #{}", e.title, e.control);
        }
    }

    #[test]
    fn every_switch_has_exactly_one_entry() {
        for s in [Switch::Nudges, Switch::Evening, Switch::Blocking, Switch::Launchers, Switch::Suggestions, Switch::OverFullscreen, Switch::SearchFiles, Switch::Autostart] {
            assert_eq!(entries().iter().filter(|e| e.switch == Some(s)).count(), 1, "{s:?}");
        }
    }

    #[test]
    fn flipping_changes_the_saved_state_and_says_the_new_one() {
        let conn = memory();
        assert_eq!(flip_stored(&conn, Switch::Nudges), Ok(false));
        assert!(!crate::nudge::enabled(&conn).unwrap());
        assert_eq!(flip_stored(&conn, Switch::Nudges), Ok(true));
        assert_eq!(flip_stored(&conn, Switch::SearchFiles), Ok(false));
        assert!(!crate::palette::search_files(&conn).unwrap());
        assert_eq!(flip_stored(&conn, Switch::Suggestions), Ok(false));
        assert_eq!(flip_stored(&conn, Switch::OverFullscreen), Ok(true));
        assert_eq!(flip_stored(&conn, Switch::Launchers), Ok(false));
        assert!(!crate::blocking::settings(&conn).unwrap().launchers);
        // The evening summary keeps the time it was set to.
        crate::evening::set(&conn, false, 21 * 60 + 15).unwrap();
        assert_eq!(flip_stored(&conn, Switch::Evening), Ok(true));
        assert_eq!(crate::evening::settings(&conn).unwrap().minute, 21 * 60 + 15);
        let state = read_stored(&conn);
        assert!(state.contains(&(Switch::Evening, true)) && state.contains(&(Switch::Nudges, true)) && state.contains(&(Switch::Launchers, false)));
    }

    #[test]
    fn flipping_blocking_in_strict_mode_is_refused() {
        let conn = memory();
        crate::blocking::set_setting(&conn, "strict", true).unwrap();
        crate::focus::start_by_hand(&conn, 25, None, chrono::Utc::now().timestamp_millis()).unwrap();
        let before = crate::blocking::settings(&conn).unwrap();
        for switch in [Switch::Blocking, Switch::Launchers] {
            assert_eq!(flip_stored(&conn, switch), Err(crate::blocking::STRICT_REFUSAL.to_string()));
        }
        let after = crate::blocking::settings(&conn).unwrap();
        assert_eq!((before.enabled, before.launchers), (after.enabled, after.launchers));
    }

    #[test]
    fn autostart_in_a_development_build_says_why() {
        assert_eq!(crate::startup::refusal(false), Some("Start at login only works in the installed app."));
        assert_eq!(crate::startup::refusal(true), None);
    }
}
