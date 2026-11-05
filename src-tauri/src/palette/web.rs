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
pub(crate) mod panel {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
    use objc2::{define_class, msg_send, sel, ClassType, MainThreadMarker, MainThreadOnly};
    use objc2_app_kit::{
        NSAutoresizingMaskOptions, NSPanel, NSResponder, NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindow,
        NSWindowCollectionBehavior, NSWindowOrderingMode, NSWindowStyleMask,
    };
    use objc2_foundation::{NSObject, NSPoint, NSRect, NSSize};
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

    /// The class the palette's window had before it became the panel. By then WebKit observes the
    /// window, so that class is the one macOS made to tell it of changes, and closing the window
    /// needs it back: removing WebKit's observer from the panel throws, which ends Kairo.
    static WAS: AtomicUsize = AtomicUsize::new(0);
    /// The frosted glass behind the panel, while the window has it; its window keeps it alive.
    static GLASS: AtomicUsize = AtomicUsize::new(0);
    /// The panel's corners, as `.palette` rounds them.
    const GLASS_RADIUS: f64 = 14.0;

    /// Turns the palette's window into the panel, waiting until it's done so it's shown as one.
    pub fn convert(window: &WebviewWindow) {
        on_main(window, |ns_window| {
            // Safety: on the main thread, with the window Tauri just made.
            unsafe { make_panel(ns_window) };
        });
    }

    /// Gives the window back its own class, so it can be closed.
    pub fn revert(window: &WebviewWindow) {
        on_main(window, |ns_window| {
            GLASS.store(0, Ordering::Release);
            let was = WAS.swap(0, Ordering::AcqRel) as *const AnyClass;
            if !was.is_null() {
                // Safety: on the main thread; the class is the one this window had, which holds
                // the same state as the panel's.
                unsafe { AnyObject::set_class(&*(ns_window as *const AnyObject), &*was) };
            }
        });
    }

    /// Runs `work` with the window's NSWindow on the main thread, and waits for it.
    fn on_main(window: &WebviewWindow, work: impl FnOnce(*mut c_void) + Send + 'static) {
        let Ok(ns_window) = window.ns_window() else { return };
        let ns_window = ns_window as usize;
        let (done, wait) = mpsc::channel();
        let sent = window.run_on_main_thread(move || {
            work(ns_window as *mut c_void);
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
        WAS.store(object.class() as *const AnyClass as usize, Ordering::Release);
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
        add_glass(panel);
    }

    /// Frosted glass behind the page, hidden until the page says where its panel is: macOS
    /// blurs what's behind the window through it, and the panel's tint goes over it.
    unsafe fn add_glass(window: &NSWindow) {
        let Some(content) = window.contentView() else { return };
        let mtm = MainThreadMarker::new_unchecked();
        let glass = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), NSRect::ZERO);
        // Popover follows the window's light or dark look; Active keeps it frosted while Kairo
        // isn't the app in front, which it never is under the palette.
        glass.setMaterial(NSVisualEffectMaterial::Popover);
        glass.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        glass.setState(NSVisualEffectState::Active);
        glass.setWantsLayer(true);
        let layer: *mut AnyObject = msg_send![&*glass, layer];
        if !layer.is_null() {
            let _: () = msg_send![layer, setCornerRadius: GLASS_RADIUS];
            let _: () = msg_send![layer, setMasksToBounds: Bool::YES];
        }
        // As the page fits the window to the panel, the glass keeps its margins and so its place.
        glass.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
        glass.setHidden(true);
        content.addSubview_positioned_relativeTo(&glass, NSWindowOrderingMode::Below, None);
        GLASS.store(Retained::as_ptr(&glass) as usize, Ordering::Release);
    }

    /// Puts the glass under the panel, `left`, `top`, `right` and `bottom` points in from the
    /// window's edges. Doesn't wait: the page asks each time it fits the window.
    pub fn place_glass(window: &WebviewWindow, left: f64, top: f64, right: f64, bottom: f64) {
        let _ = window.run_on_main_thread(move || {
            let glass = GLASS.load(Ordering::Acquire) as *const NSVisualEffectView;
            if glass.is_null() {
                return;
            }
            // Safety: on the main thread, and the window that holds the glass is still open:
            // `revert` forgets it before the window closes.
            unsafe {
                let glass = &*glass;
                let Some(parent) = glass.superview() else { return };
                let bounds = parent.bounds();
                let (width, height) = ((bounds.size.width - left - right).max(0.0), (bounds.size.height - top - bottom).max(0.0));
                let y = if parent.isFlipped() { top } else { bottom };
                glass.setFrame(NSRect::new(NSPoint::new(left, y), NSSize::new(width, height)));
                glass.setHidden(false);
            }
        });
    }
}

/// Closes the palette's page once it has been put away for a while; runs on the thread that
/// made it, and ends with it.
fn let_go_when_unused(app: &AppHandle) {
    loop {
        std::thread::sleep(Duration::from_secs(60));
        let Some(window) = app.get_webview_window(LABEL) else { return };
        if !window.is_visible().unwrap_or(true) && now_ms() - HIDDEN_AT.load(Ordering::Relaxed) > KEEP_FOR_MS {
            #[cfg(target_os = "macos")]
            panel::revert(&window);
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
