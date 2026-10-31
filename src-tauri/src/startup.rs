//! Starting Kairo when the user signs in.

use rusqlite::Connection;
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use crate::db;

/// Passed by the login entry so Kairo starts quietly in the tray instead of opening its window.
pub const MINIMIZED_ARG: &str = "--minimized";
const INITIALIZED_KEY: &str = "autostart_initialized";

/// Dev builds run from target\debug; registering that path at login would go stale on every rebuild.
pub const AVAILABLE: bool = !cfg!(debug_assertions);

/// Opens the command palette, for a launcher or a desktop shortcut where apps can't listen for
/// keys themselves (Linux on Wayland), even over a game when Settings allows. A running Kairo gets
/// it too.
pub const PALETTE_ARG: &str = "--palette";

/// Opens Chrono, the screen time tracker's window, rather than the palette; Chrono's own Start
/// menu and app menu entry passes it.
pub const CHRONO_ARG: &str = "--chrono";

/// Opens the dashboard, Kairo's own window, rather than the palette.
pub const DASHBOARD_ARG: &str = "--dashboard";

/// Prints the files the system finds for the words after it, and exits, for trying file search
/// from a terminal.
pub const SEARCH_FILES_ARG: &str = "--search-files";

/// The words after `--search-files`, joined by spaces; None without the flag.
pub fn search_files_query(args: &[String]) -> Option<String> {
    let at = args.iter().position(|a| a == SEARCH_FILES_ARG)?;
    Some(args[at + 1..].join(" "))
}

/// What a launch opens.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Launch {
    Palette,
    Chrono,
    Dashboard,
    Quiet,
}

/// What a launch opens: Kairo itself is the palette, so opening it from the Start menu, Spotlight
/// or a launcher does, while signing in starts it quietly and Chrono's shortcut opens Chrono.
pub fn launch_for(args: &[String]) -> Launch {
    let has = |arg: &str| args.iter().any(|a| a == arg);
    if has(PALETTE_ARG) {
        Launch::Palette
    } else if has(DASHBOARD_ARG) {
        Launch::Dashboard
    } else if has(CHRONO_ARG) {
        Launch::Chrono
    } else if has(MINIMIZED_ARG) {
        Launch::Quiet
    } else {
        Launch::Palette
    }
}

/// Turn start-at-login on the first time an installed build runs; after that the user's choice stands.
pub fn apply_default(app: &AppHandle, conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    if !AVAILABLE || db::get_setting(conn, INITIALIZED_KEY)?.is_some() {
        return Ok(());
    }
    app.autolaunch().enable()?;
    db::set_setting(conn, INITIALIZED_KEY, "1")?;
    Ok(())
}

pub fn is_enabled(app: &AppHandle) -> bool {
    AVAILABLE && app.autolaunch().is_enabled().unwrap_or(false)
}

/// Why Start at login can't be changed, when `available` says it can't.
pub fn refusal(available: bool) -> Option<&'static str> {
    (!available).then_some("Start at login only works in the installed app.")
}

pub fn set_enabled(app: &AppHandle, on: bool) -> Result<(), String> {
    if let Some(why) = refusal(AVAILABLE) {
        return Err(why.into());
    }
    let launcher = app.autolaunch();
    if on { launcher.enable() } else { launcher.disable() }.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_launch_opens_what_it_names() {
        let run = |args: &[&str]| launch_for(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>());
        assert_eq!(run(&["kairo"]), Launch::Palette);
        assert_eq!(run(&["kairo", PALETTE_ARG]), Launch::Palette);
        assert_eq!(run(&["kairo", CHRONO_ARG]), Launch::Chrono);
        assert_eq!(run(&["kairo", DASHBOARD_ARG]), Launch::Dashboard);
        assert_eq!(run(&["kairo", MINIMIZED_ARG]), Launch::Quiet);
    }

    #[test]
    fn the_words_after_the_search_flag_are_the_query() {
        let args = |a: &[&str]| a.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        assert_eq!(search_files_query(&args(&["kairo", "--search-files", "tax", "2026"])), Some("tax 2026".into()));
        assert_eq!(search_files_query(&args(&["kairo", "--palette"])), None);
    }
}
