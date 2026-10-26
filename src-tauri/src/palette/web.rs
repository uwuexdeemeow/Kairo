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

/// Brings the palette up with the keyboard. On a Mac, showing the panel is enough: focusing it
/// the usual way would activate Kairo, and macOS answers that by leaving a full-screen app for
/// Kairo's Space.
fn present(window: &WebviewWindow) {
    let _ = window.show();
    #[cfg(not(target_os = "macos"))]
    let _ = window.set_focus();
}

fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        place(&window);
        present(&window);
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
    #[cfg(target_os = "macos")]
    panel::convert(&window);
    place(&window);
    present(&window);
    true
}

/// On a Mac the palette is a panel, like Spotlight: it opens over full-screen apps on the Space
/// you're in, and takes the keyboard without bringing Kairo forward. An ordinary window can't do
/// either for an app with a Dock icon.
#[cfg(target_os = "macos")]
mod panel {
    use std::ffi::c_void;
    use std::sync::mpsc;

    use objc2::runtime::{AnyObject, Bool, Sel};
    use objc2::{define_class, msg_send, sel, ClassType, MainThreadOnly};
    use objc2_app_kit::{NSPanel, NSResponder, NSWindow, NSWindowCollectionBehavior, NSWindowStyleMask};
    use objc2_foundation::NSObject;
    use tauri::WebviewWindow;

    define_class!(
        #[unsafe(super(NSPanel, NSWindow, NSResponder, NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "KairoPalettePanel"]
        struct Panel;

        impl Panel {
            // A borderless window only takes the keyboard when it says it can.
            #[unsafe(method(canBecomeKeyWindow))]
            fn can_become_key_window(&self) -> bool {
                true
            }

            #[unsafe(method(canBecomeMainWindow))]
            fn can_become_main_window(&self) -> bool {
                false
            }
        }
    );

    /// Turns the palette's window into the panel, waiting until it's done so it's shown as one.
    pub fn convert(window: &WebviewWindow) {
        let Ok(ns_window) = window.ns_window() else { return };
        let ns_window = ns_window as usize;
        let (done, wait) = mpsc::channel();
        let sent = window.run_on_main_thread(move || {
            // Safety: on the main thread, with the window Tauri just made.
            unsafe { make_panel(ns_window as *mut c_void) };
            let _ = done.send(());
        });
        if sent.is_ok() {
            let _ = wait.recv();
        }
    }

    /// NSPanel adds nothing to NSWindow's memory (both are 520 bytes on macOS 26), so swapping
    /// the class leaves the state tao keeps in its window where tao expects it.
    unsafe fn make_panel(ns_window: *mut c_void) {
        let object = &*(ns_window as *const AnyObject);
        AnyObject::set_class(object, Panel::class());
        let panel = &*(ns_window as *const NSPanel);
        panel.setStyleMask(panel.styleMask() | NSWindowStyleMask::NonactivatingPanel);
        // Adding the style after the window exists doesn't reach the window server on recent
        // macOS versions; this private call tells it the panel mustn't activate Kairo.
        let prevents: Sel = sel!(_setPreventsActivation:);
        let responds: Bool = msg_send![panel, respondsToSelector: prevents];
        if responds.as_bool() {
            let _: () = msg_send![panel, _setPreventsActivation: Bool::YES];
        }
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        panel.setFloatingPanel(true);
        panel.setHidesOnDeactivate(false);
    }
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
