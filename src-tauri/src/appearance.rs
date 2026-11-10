//! The theme: one of Kairo's grounds, or matching the system. The window frame and background
//! follow the choice, and the page picks its palette to match.

use rusqlite::{Connection, Result};
use tauri::window::Color;
use tauri::{AppHandle, Manager, Theme, WebviewWindow};

use crate::db;

const KEY: &str = "appearance";

/// A ground: the paper a theme draws on, its sheets, and the inks of its text and rules, as the
/// tokens in `src/styles.css` have them. The app inks and the data colors are the dark or light
/// set, whichever the ground is, so they mean the same on all of them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ground {
    /// As saved and as the page's `data-ground`; "dark" and "light" are Ink and Paper, saved
    /// under those names before there were others.
    pub id: &'static str,
    pub light: bool,
    pub bg: u32,
    pub raised: u32,
    pub ink: u32,
    pub muted: u32,
    pub rule: u32,
    pub line: u32,
}

const fn ground(id: &'static str, light: bool, [bg, raised, ink, muted, rule, line]: [u32; 6]) -> Ground {
    Ground { id, light, bg, raised, ink, muted, rule, line }
}

/// Every ground, dark ones first, each list led by the one the system's own look picks.
pub const GROUNDS: &[Ground] = &[
    ground("dark", false, [0x141a24, 0x1b2230, 0xe3e7ee, 0x97a1b2, 0x232c3d, 0x33405a]),
    ground("graphite", false, [0x161718, 0x1f2022, 0xe6e6e6, 0xa0a0a0, 0x2a2b2e, 0x3c3e42]),
    ground("walnut", false, [0x1a1411, 0x241c17, 0xede3d9, 0xa99a8a, 0x30251e, 0x47372b]),
    ground("baize", false, [0x0f1a15, 0x15241d, 0xe0ebe4, 0x92a89b, 0x1d3128, 0x2b4639]),
    ground("midnight", false, [0x000000, 0x0d0f13, 0xe3e7ee, 0x97a1b2, 0x1a1d23, 0x2a2f38]),
    ground("light", true, [0xf4efe6, 0xfffaf2, 0x2b2622, 0x5e544a, 0xe3d9c9, 0xd3c7b3]),
    ground("linen", true, [0xf2f3f5, 0xffffff, 0x23262b, 0x565c66, 0xe1e4e8, 0xcdd2d9]),
    ground("sage", true, [0xeef1ea, 0xf8faf5, 0x252b24, 0x566053, 0xdbe2d5, 0xc6d0bf]),
];

/// The ground a mode names; None for "system", which takes Ink or Paper as the system looks.
pub fn ground_of(mode: &str) -> Option<&'static Ground> {
    GROUNDS.iter().find(|g| g.id == mode)
}

/// The ground the system's look picks: Paper when it's light, Ink when it's dark.
pub fn system_ground(light: bool) -> &'static Ground {
    ground_of(if light { "light" } else { "dark" }).expect("Ink and Paper are grounds")
}

/// "system" (match the system) or a ground's id.
pub fn get(conn: &Connection) -> Result<String> {
    Ok(match db::get_setting(conn, KEY)?.as_deref() {
        Some(mode) if ground_of(mode).is_some() => mode.to_string(),
        _ => "system".to_string(),
    })
}

pub fn set(conn: &Connection, mode: &str) -> Result<bool> {
    if mode != "system" && ground_of(mode).is_none() {
        return Ok(false);
    }
    db::set_setting(conn, KEY, mode)?;
    Ok(true)
}

/// Saves a choice of theme and puts every window in it; an open page repaints itself on
/// `kairo:appearance`. The settings page and the palette both choose through here.
pub fn choose(app: &AppHandle, conn: &Connection, mode: &str) -> std::result::Result<(), String> {
    use tauri::Emitter;
    if !set(conn, mode).map_err(|e| e.to_string())? {
        return Err(format!("unknown appearance: {mode}"));
    }
    apply(app, mode);
    let _ = app.emit("kairo:appearance", mode);
    Ok(())
}

/// The window theme for a mode: None lets the system decide.
pub fn theme(mode: &str) -> Option<Theme> {
    ground_of(mode).map(|g| if g.light { Theme::Light } else { Theme::Dark })
}

/// A ground's page color, for behind the page while it loads.
fn page_color(ground: &Ground) -> Color {
    Color((ground.bg >> 16) as u8, (ground.bg >> 8) as u8, ground.bg as u8, 255)
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
    let ground = ground_of(mode).unwrap_or_else(|| system_ground(window.theme().ok() == Some(Theme::Light)));
    let _ = window.set_background_color(Some(page_color(ground)));
    tint_caption(window, ground);
}

/// Windows 11 lets the title bar take a color: the page's own ground, with the ground's ink on it,
/// so the bar reads as part of the page. Windows 10 refuses both and keeps the system's bar.
#[cfg(windows)]
fn tint_caption(window: &WebviewWindow, ground: &Ground) {
    use windows::Win32::Foundation::{COLORREF, HWND};
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR};

    let Ok(hwnd) = window.hwnd() else { return };
    // COLORREF is 0x00BBGGRR; the grounds are 0xRRGGBB.
    let colorref = |rgb: u32| COLORREF(((rgb & 0xff) << 16) | (rgb & 0xff00) | (rgb >> 16 & 0xff));
    let (caption, text, border) = (colorref(ground.bg), colorref(ground.ink), colorref(ground.line));
    unsafe {
        let hwnd = HWND(hwnd.0);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, (&border as *const COLORREF).cast(), 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR, (&caption as *const COLORREF).cast(), 4);
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_TEXT_COLOR, (&text as *const COLORREF).cast(), 4);
    }
}

/// On a Mac the bar is made see-through, so the window's own color (set just before this) shows in
/// it; its text follows the window's light or dark theme, already set.
#[cfg(target_os = "macos")]
fn tint_caption(window: &WebviewWindow, _ground: &Ground) {
    let _ = window.set_title_bar_style(tauri::TitleBarStyle::Transparent);
}

/// On Linux the bar is GTK's own header bar, colored with a style rule of the ground's colors.
/// A desktop that draws the bar itself (no header bar for GTK to style) keeps its own.
#[cfg(all(unix, not(target_os = "macos")))]
fn tint_caption(window: &WebviewWindow, ground: &Ground) {
    use gtk::prelude::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    thread_local! {
        /// The rule each window carries now, so a new theme takes the old one out.
        static RULES: RefCell<HashMap<String, gtk::CssProvider>> = RefCell::new(HashMap::new());
    }

    let (bg, ink, line) = (ground.bg, ground.ink, ground.line);
    let label = window.label().to_string();
    let target = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(gtk_window) = target.gtk_window() else { return };
        let Some(bar) = gtk_window.titlebar() else { return };
        let css = format!(
            "headerbar, .titlebar {{ background: #{bg:06x}; background-image: none; color: #{ink:06x}; border-color: #{line:06x}; box-shadow: none; }} \
             headerbar button, headerbar label {{ color: #{ink:06x}; }}"
        );
        let provider = gtk::CssProvider::new();
        if provider.load_from_data(css.as_bytes()).is_err() {
            return;
        }
        let context = bar.style_context();
        RULES.with(|rules| {
            if let Some(old) = rules.borrow_mut().insert(label, provider.clone()) {
                context.remove_provider(&old);
            }
        });
        context.add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    });
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
    fn ink_and_paper_keep_their_old_names() {
        assert_eq!(theme("dark"), Some(Theme::Dark));
        assert_eq!(theme("light"), Some(Theme::Light));
        assert_eq!(theme("walnut"), Some(Theme::Dark));
        assert_eq!(theme("sage"), Some(Theme::Light));
        assert_eq!(theme("system"), None);
        assert_eq!(system_ground(true).id, "light");
        assert_eq!(system_ground(false).id, "dark");
    }

    #[test]
    fn only_a_ground_or_the_system_is_saved() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        assert_eq!(get(&conn).unwrap(), "system");
        assert!(set(&conn, "baize").unwrap());
        assert_eq!(get(&conn).unwrap(), "baize");
        assert!(!set(&conn, "plaid").unwrap());
        assert_eq!(get(&conn).unwrap(), "baize");
    }

    /// Each ground's colors are the ones its block in styles.css gives the page.
    #[test]
    fn the_grounds_match_the_stylesheet() {
        let css = include_str!("../../src/styles.css");
        for g in GROUNDS {
            let selector = match g.id {
                "dark" => ":root {".to_string(),
                "light" => ":root[data-theme=\"light\"] {".to_string(),
                id => format!(":root[data-ground=\"{id}\"] {{"),
            };
            let start = css.find(&selector).unwrap_or_else(|| panic!("no block for {}", g.id));
            let block = &css[start..start + css[start..].find('}').unwrap()];
            for (token, value) in [("bg", g.bg), ("raised", g.raised), ("ink", g.ink), ("muted", g.muted), ("rule", g.rule), ("line", g.line)] {
                assert!(block.contains(&format!("--{token}: #{value:06x};")), "{}: --{token} isn't #{value:06x}", g.id);
            }
        }
    }

    #[test]
    fn the_palette_and_the_ring_stay_see_through() {
        assert!(see_through("palette") && see_through("ring"));
        assert!(!see_through("main") && !see_through("dashboard") && !see_through("blocked"));
    }
}
