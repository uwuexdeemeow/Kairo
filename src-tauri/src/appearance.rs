//! Light, dark or matching Windows: the window frame and background follow the choice, and the
//! page picks its palette to match.

use rusqlite::{Connection, Result};
use tauri::window::Color;
use tauri::{AppHandle, Manager, Theme, WebviewWindow};

use crate::db;

const KEY: &str = "appearance";
/// The page backgrounds, so a window never shows the other theme's color while it loads.
const DARK_BG: Color = Color(14, 15, 18, 255);
const LIGHT_BG: Color = Color(246, 246, 244, 255);

/// "system" (match Windows), "light" or "dark".
pub fn get(conn: &Connection) -> Result<String> {
    Ok(match db::get_setting(conn, KEY)?.as_deref() {
        Some(mode @ ("light" | "dark")) => mode.to_string(),
        _ => "system".to_string(),
    })
}

pub fn set(conn: &Connection, mode: &str) -> Result<bool> {
    if !matches!(mode, "system" | "light" | "dark") {
        return Ok(false);
    }
    db::set_setting(conn, KEY, mode)?;
    Ok(true)
}

/// The window theme for a mode: None lets Windows decide.
pub fn theme(mode: &str) -> Option<Theme> {
    match mode {
        "light" => Some(Theme::Light),
        "dark" => Some(Theme::Dark),
        _ => None,
    }
}

/// Windows that are see-through around what they draw: the web palette around its panel and the
/// app ring around the ring. A page color there would show as a solid box behind them.
fn see_through(label: &str) -> bool {
    [crate::palette::LABEL, crate::ring::LABEL].contains(&label)
}

pub fn apply_to(window: &WebviewWindow, mode: &str) {
    let chosen = theme(mode);
    if let Err(e) = window.set_theme(chosen) {
        eprintln!("appearance: cannot set the window theme: {e}");
    }
    if see_through(window.label()) {
        let _ = window.set_background_color(Some(Color(0, 0, 0, 0)));
        return;
    }
    let shown = chosen.or_else(|| window.theme().ok()).unwrap_or(Theme::Dark);
    let _ = window.set_background_color(Some(if shown == Theme::Light { LIGHT_BG } else { DARK_BG }));
}

pub fn apply(app: &AppHandle, mode: &str) {
    for window in app.webview_windows().values() {
        apply_to(window, mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_and_the_ring_stay_see_through() {
        assert!(see_through("palette") && see_through("ring"));
        assert!(!see_through("main") && !see_through("dashboard") && !see_through("blocked"));
    }
}
