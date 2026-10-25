//! Starting Kairo when the user signs in.

use rusqlite::Connection;
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use crate::db;

/// Passed by the login entry so Kairo starts quietly in the tray instead of opening its window.
pub const MINIMIZED_ARG: &str = "--minimized";
const INITIALIZED_KEY: &str = "autostart_initialized";
/// The tray menu's name for it.
pub const LABEL: &str = if cfg!(target_os = "macos") {
    "Open at Login"
} else if cfg!(windows) {
    "Start with Windows"
} else {
    "Start at login"
};

/// Dev builds run from target\debug; registering that path at login would go stale on every rebuild.
pub const AVAILABLE: bool = !cfg!(debug_assertions);

pub fn launched_at_login() -> bool {
    std::env::args().any(|a| a == MINIMIZED_ARG)
}

/// Opens the command palette rather than Kairo's window, for a launcher or a desktop shortcut
/// where apps can't listen for keys themselves (Linux on Wayland). A running Kairo gets it too.
pub const PALETTE_ARG: &str = "--palette";

pub fn launched_for_palette() -> bool {
    std::env::args().any(|a| a == PALETTE_ARG)
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

pub fn set_enabled(app: &AppHandle, on: bool) -> Result<(), String> {
    if !AVAILABLE {
        return Err("Start at login only works in the installed app.".into());
    }
    let launcher = app.autolaunch();
    if on { launcher.enable() } else { launcher.disable() }.map_err(|e| e.to_string())
}
