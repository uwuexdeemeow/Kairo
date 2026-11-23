//! Picking a colour from anywhere on the screen, each system its own way. A Mac shows its own
//! magnifier (NSColorSampler), which needs no permission. Linux asks the desktop portal, which
//! shows the desktop's own picker on GNOME and Plasma, on X11 and Wayland alike. Windows has no
//! picker of its own: the next left click anywhere is taken (not passed to what's under it) and
//! the pixel under it read; a right click, Esc or twenty seconds leave it.
//!
//! Each blocks until a colour is picked or the picking is left, so it's called off the palette's
//! thread. None when nothing was picked.

pub type Rgb = [u8; 3];

#[cfg(windows)]
pub fn pick(_app: &tauri::AppHandle) -> Option<Rgb> {
    imp::pick()
}

#[cfg(windows)]
mod imp {
    use std::sync::Mutex;

    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC, CLR_INVALID};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, KillTimer, PostQuitMessage, SetTimer, SetWindowsHookExW, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL,
        WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_RBUTTONDOWN, WM_TIMER,
    };

    /// The point clicked, or None for leaving: what the hooks found.
    static CLICKED: Mutex<Option<Option<POINT>>> = Mutex::new(None);
    /// The click's release is taken too, so the app under it doesn't see half a click.
    static SWALLOW_UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    unsafe extern "system" fn mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let m = &*(lparam as *const MSLLHOOKSTRUCT);
            match wparam as u32 {
                WM_LBUTTONDOWN => {
                    *CLICKED.lock().unwrap_or_else(|e| e.into_inner()) = Some(Some(m.pt));
                    SWALLOW_UP.store(true, std::sync::atomic::Ordering::SeqCst);
                    PostQuitMessage(0);
                    return 1;
                }
                WM_LBUTTONUP if SWALLOW_UP.swap(false, std::sync::atomic::Ordering::SeqCst) => return 1,
                WM_RBUTTONDOWN => {
                    *CLICKED.lock().unwrap_or_else(|e| e.into_inner()) = Some(None);
                    PostQuitMessage(0);
                    return 1;
                }
                _ => {}
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }

    unsafe extern "system" fn keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 && wparam as u32 == WM_KEYDOWN && (*(lparam as *const KBDLLHOOKSTRUCT)).vkCode == 0x1B {
            *CLICKED.lock().unwrap_or_else(|e| e.into_inner()) = Some(None);
            PostQuitMessage(0);
            return 1;
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }

    pub fn pick() -> Option<[u8; 3]> {
        *CLICKED.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let point = unsafe {
            let mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), std::ptr::null_mut(), 0);
            let key_hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), std::ptr::null_mut(), 0);
            if mouse_hook.is_null() {
                return None;
            }
            let timer = SetTimer(std::ptr::null_mut(), 0, 20_000, None);
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                if msg.message == WM_TIMER {
                    break;
                }
            }
            KillTimer(std::ptr::null_mut(), timer);
            // The release of the click comes a moment later; wait for it to be taken.
            std::thread::sleep(std::time::Duration::from_millis(150));
            UnhookWindowsHookEx(mouse_hook);
            if !key_hook.is_null() {
                UnhookWindowsHookEx(key_hook);
            }
            CLICKED.lock().unwrap_or_else(|e| e.into_inner()).take().flatten()?
        };
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            let colour = GetPixel(screen, point.x, point.y);
            ReleaseDC(std::ptr::null_mut(), screen);
            (colour != CLR_INVALID).then(|| [(colour & 0xff) as u8, (colour >> 8 & 0xff) as u8, (colour >> 16 & 0xff) as u8])
        }
    }
}

#[cfg(target_os = "macos")]
pub fn pick(app: &tauri::AppHandle) -> Option<Rgb> {
    use block2::RcBlock;
    use objc2_app_kit::{NSColor, NSColorSampler, NSColorSpace};

    let (tx, rx) = std::sync::mpsc::channel::<Option<Rgb>>();
    // AppKit's sampler runs on the main thread and answers there.
    app.run_on_main_thread(move || {
        let sampler = NSColorSampler::new();
        let handler = RcBlock::new(move |colour: *mut NSColor| {
            let picked = unsafe { colour.as_ref() }.and_then(|c| c.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())).map(|c| {
                let byte = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u8;
                [byte(c.redComponent()), byte(c.greenComponent()), byte(c.blueComponent())]
            });
            let _ = tx.send(picked);
        });
        unsafe { sampler.showSamplerWithSelectionHandler(&handler) };
    })
    .ok()?;
    rx.recv().ok().flatten()
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn pick(_app: &tauri::AppHandle) -> Option<Rgb> {
    use std::collections::HashMap;
    use zbus::zvariant::{OwnedValue, Value};

    let conn = zbus::blocking::Connection::session().ok()?;
    let token = format!("kairo{}", std::process::id());
    let sender = conn.unique_name()?.trim_start_matches(':').replace('.', "_");
    let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    // Listened for before asking, so a quick answer isn't missed.
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.portal.Request")
        .ok()?
        .member("Response")
        .ok()?
        .path(path)
        .ok()?
        .build();
    let answers = zbus::blocking::MessageIterator::for_match_rule(rule, &conn, Some(1)).ok()?;
    let mut options: HashMap<&str, Value> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    conn.call_method(Some("org.freedesktop.portal.Desktop"), "/org/freedesktop/portal/desktop", Some("org.freedesktop.portal.Screenshot"), "PickColor", &("", options)).ok()?;
    for message in answers {
        let message = message.ok()?;
        let (response, results): (u32, HashMap<String, OwnedValue>) = message.body().deserialize().ok()?;
        if response != 0 {
            return None;
        }
        let colour: Value = results.get("color")?.try_clone().ok()?.into();
        let Value::Structure(s) = colour else { return None };
        let byte = |v: &Value| match v {
            Value::F64(f) => Some((f * 255.0).round().clamp(0.0, 255.0) as u8),
            _ => None,
        };
        let f = s.fields();
        return Some([byte(f.first()?)?, byte(f.get(1)?)?, byte(f.get(2)?)?]);
    }
    None
}
