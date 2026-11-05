//! The app ring on macOS and Linux: `ring.html` in a small see-through window at the pointer,
//! made each time it opens and let go when it closes. Windows draws its own (`native`).

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent};

use crate::AppState;

use super::LABEL;
/// Set while the ring's window is being made.
static OPENING: AtomicBool = AtomicBool::new(false);

/// Opens the ring at the pointer, or turns it if it's open.
pub fn open_or_turn(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.emit("ring:turn", ());
        return;
    }
    // Made on a thread of its own, as the palette's: the shortcut arrives on the main thread.
    if OPENING.swap(true, Ordering::AcqRel) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        open(&app);
        OPENING.store(false, Ordering::Release);
    });
}

fn open(app: &AppHandle) {
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("ring.html".into()))
        .title("Kairo")
        .inner_size(super::WIDTH as f64, super::HEIGHT as f64)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .disable_drag_drop_handler()
        .visible(false)
        .build();
    let window = match built {
        Ok(window) => window,
        Err(e) => return eprintln!("ring: cannot open: {e}"),
    };
    let mode = app.state::<AppState>().db.lock().ok().and_then(|conn| crate::appearance::get(&conn).ok()).unwrap_or_default();
    crate::appearance::apply_to(&window, &mode);
    #[cfg(target_os = "macos")]
    crate::palette::web::panel::convert(&window);
    // At the pointer, kept on its screen. Wayland doesn't say where the pointer is: the middle of
    // the screen instead.
    let monitor = window
        .cursor_position()
        .ok()
        .and_then(|p| window.monitor_from_point(p.x, p.y).ok().flatten().map(|m| (m, Some(p))))
        .or_else(|| window.primary_monitor().ok().flatten().map(|m| (m, None)));
    if let (Some((monitor, cursor)), Ok(size)) = (monitor, window.outer_size()) {
        let area = monitor.work_area();
        let (x, y, w, h) = (area.position.x, area.position.y, area.size.width as i32, area.size.height as i32);
        let at = cursor.map_or((x + w / 2, y + h / 2), |p| (p.x as i32, p.y as i32));
        let (left, top) = super::place(at, (x, y, x + w, y + h), (size.width as i32, size.height as i32));
        let _ = window.set_position(PhysicalPosition::new(left, top));
    }
    let _ = window.show();
    #[cfg(not(target_os = "macos"))]
    let _ = window.set_focus();
}

/// Closes the ring; its web view goes with it.
pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        #[cfg(target_os = "macos")]
        crate::palette::web::panel::revert(&window);
        let _ = window.destroy();
    }
}

/// Clicking elsewhere, or another window taking the keyboard, closes the ring.
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if window.label() == LABEL && matches!(event, WindowEvent::Focused(false)) {
        close(window.app_handle());
    }
}
