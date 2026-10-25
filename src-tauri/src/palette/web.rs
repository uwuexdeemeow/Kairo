//! The command palette on macOS and Linux: `palette.html` in a small see-through window, made the
//! first time it's wanted and let go after a while unused. Windows draws its own (`native`).

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent};

use super::LABEL;
use crate::AppState;

/// From the top of the screen to the palette, as a share of the screen's height.
const FROM_TOP: f64 = 0.16;
/// The page is let go after this long put away: a web view holds tens of megabytes, while
/// keeping it means the palette opens at once, ready for typing.
const KEEP_FOR_MS: i64 = 10 * 60_000;
/// When the palette was last put away.
static HIDDEN_AT: AtomicI64 = AtomicI64::new(0);
/// Set while the palette's window is being made.
static OPENING: AtomicBool = AtomicBool::new(false);

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Over the middle of the screen the mouse is on, in its upper part, like Spotlight.
fn place(window: &WebviewWindow) {
    let monitor = window
        .cursor_position()
        .ok()
        .and_then(|p| window.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());
    let (Some(monitor), Ok(size)) = (monitor, window.outer_size()) else { return };
    let (origin, screen) = (monitor.position(), monitor.size());
    let x = origin.x + (screen.width as i32 - size.width as i32) / 2;
    let y = origin.y + (screen.height as f64 * FROM_TOP) as i32;
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        place(&window);
        let _ = window.show();
        let _ = window.set_focus();
        // The page clears the last search and loads fresh suggestions.
        let _ = window.emit("palette:open", ());
        return;
    }
    // Made on a thread of its own: the shortcut arrives on the app's main thread, which mustn't
    // wait on a web view starting up, and a second press meanwhile mustn't make another.
    if OPENING.swap(true, Ordering::AcqRel) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let opened = open(&app);
        OPENING.store(false, Ordering::Release);
        if opened {
            let_go_when_unused(&app);
        }
    });
}

fn open(app: &AppHandle) -> bool {
    // A new page loads its own suggestions.
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("palette.html".into()))
        .title("Kairo")
        .inner_size(680.0, 580.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        // On a Mac, it opens on the Space you're in rather than taking you to Kairo's.
        .visible_on_all_workspaces(true)
        .disable_drag_drop_handler()
        .visible(false)
        .build();
    let window = match built {
        Ok(window) => window,
        Err(e) => {
            eprintln!("palette: cannot open: {e}");
            return false;
        }
    };
    let mode = app.state::<AppState>().db.lock().ok().and_then(|conn| crate::appearance::get(&conn).ok()).unwrap_or_default();
    crate::appearance::apply_to(&window, &mode);
    place(&window);
    let _ = window.show();
    let _ = window.set_focus();
    true
}

/// Closes the palette's page once it has been put away for a while; runs on the thread that
/// made it, and ends with it.
fn let_go_when_unused(app: &AppHandle) {
    loop {
        std::thread::sleep(Duration::from_secs(60));
        let Some(window) = app.get_webview_window(LABEL) else { return };
        if !window.is_visible().unwrap_or(true) && now_ms() - HIDDEN_AT.load(Ordering::Relaxed) > KEEP_FOR_MS {
            let _ = window.close();
            return;
        }
    }
}

/// Puts the palette away. `give_back` hands the keyboard back to the app that had it, which on a
/// Mac means stepping Kairo aside unless one of its own windows is up.
pub fn hide(app: &AppHandle, give_back: bool) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let _ = window.hide();
    HIDDEN_AT.store(now_ms(), Ordering::Relaxed);
    #[cfg(target_os = "macos")]
    if give_back && !app.webview_windows().values().any(|w| w.label() != LABEL && w.is_visible().unwrap_or(false)) {
        let _ = app.hide();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = give_back;
}

pub fn toggle(app: &AppHandle) {
    if app.get_webview_window(LABEL).is_some_and(|w| w.is_visible().unwrap_or(false)) {
        hide(app, true);
    } else {
        show(app);
    }
}

/// The palette is a glance: clicking anywhere else puts it away.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() == LABEL && matches!(event, WindowEvent::Focused(false)) {
        hide(window.app_handle(), false);
    }
}
