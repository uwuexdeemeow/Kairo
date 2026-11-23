//! Hearing what's typed, for keywords (`keywords.rs`): each system's way of telling Kairo about
//! key presses in other apps, turned into characters, Backspace, or "start again" (a click, a
//! shortcut, a key that moves the caret). Only presses are heard, never what's on screen, and
//! nothing is kept here. Keys Kairo presses itself are left out.
//!
//! Windows: a low-level keyboard and mouse hook. macOS: a listen-only event tap, which needs
//! Input Monitoring (macOS asks); macOS itself keeps password fields from it. X11: the RECORD
//! extension. Wayland lets no app hear another's keys, so there's nothing.

use crate::keywords::Typed;

/// Whether this system lets Kairo hear typing at all.
pub fn available() -> bool {
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        !super::wayland()
    }
    #[cfg(any(windows, target_os = "macos"))]
    {
        true
    }
}

/// Starts hearing typing on a thread of its own, telling `send` each press; false when the
/// system refused.
pub fn start(send: fn(Typed)) -> bool {
    imp::start(send)
}

/// Presses Backspace `n` times for the window with the keyboard.
pub fn backspaces(n: usize) -> bool {
    imp::backspaces(n)
}

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::OnceLock;

    use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetKeyState, GetKeyboardLayout, SendInput, ToUnicodeEx, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_BACK,
        VK_CAPITAL, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, SetWindowsHookExW, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL,
        WH_MOUSE_LL, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_SYSKEYDOWN,
    };

    use super::Typed;

    static SEND: OnceLock<fn(Typed)> = OnceLock::new();
    /// The window in front at the last key, so a key in another one starts again.
    static FRONT: AtomicUsize = AtomicUsize::new(0);

    fn send(t: Typed) {
        if let Some(send) = SEND.get() {
            send(t);
        }
    }

    fn held(vk: u16) -> bool {
        unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
    }

    /// What a key pressed means: None for one that changes nothing (a modifier on its own, F5).
    unsafe fn meaning(vk: u32, scan: u32) -> Option<Typed> {
        match vk {
            0x08 => return Some(Typed::Back),
            // Enter, Tab, Esc, Page Up/Down, End, Home, the arrows, Insert, Delete.
            0x0D | 0x09 | 0x1B | 0x21..=0x28 | 0x2D | 0x2E => return Some(Typed::Reset),
            // Shift, Ctrl, Alt (either side), Caps Lock and the Windows keys on their own.
            0x10..=0x12 | 0xA0..=0xA5 | 0x14 | 0x5B | 0x5C => return None,
            _ => {}
        }
        let (ctrl, alt) = (held(VK_CONTROL), held(VK_MENU));
        // A shortcut (Ctrl+V, Win+D), not typing; Ctrl with Alt is AltGr, which types.
        if (ctrl && !alt) || held(VK_LWIN) || held(VK_RWIN) {
            return Some(Typed::Reset);
        }
        let mut state = [0u8; 256];
        for vk in [VK_SHIFT, VK_CONTROL, VK_MENU, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5] {
            if held(vk) {
                state[vk as usize] = 0x80;
            }
        }
        if GetKeyState(VK_CAPITAL as i32) & 1 != 0 {
            state[VK_CAPITAL as usize] = 0x01;
        }
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
        let layout = GetKeyboardLayout(thread);
        let mut out = [0u16; 8];
        // 4: leave the keyboard's state alone, so a dead key (´ then e) still works in the app.
        let n = ToUnicodeEx(vk, scan, state.as_ptr(), out.as_mut_ptr(), out.len() as i32, 4, layout);
        if n != 1 {
            return None;
        }
        char::from_u32(out[0] as u32).filter(|c| !c.is_control()).map(Typed::Char)
    }

    unsafe extern "system" fn keyboard(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 && (wparam == WM_KEYDOWN as usize || wparam == WM_SYSKEYDOWN as usize) {
            let k = &*(lparam as *const KBDLLHOOKSTRUCT);
            if k.flags & LLKHF_INJECTED == 0 {
                let front = GetForegroundWindow() as usize;
                if FRONT.swap(front, Ordering::Relaxed) != front {
                    send(Typed::Reset);
                }
                if let Some(t) = meaning(k.vkCode, k.scanCode) {
                    send(t);
                }
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }

    unsafe extern "system" fn mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 && matches!(wparam as u32, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN) {
            send(Typed::Reset);
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }

    pub fn start(send: fn(Typed)) -> bool {
        let _ = SEND.set(send);
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("kairo-keywords".into()).spawn(move || unsafe {
            // Low-level hooks are called on the thread that set them, from its message loop.
            let keys = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), std::ptr::null_mut(), 0);
            let clicks = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), std::ptr::null_mut(), 0);
            let _ = tx.send(!keys.is_null());
            if keys.is_null() {
                return;
            }
            let _ = clicks;
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}
        });
        spawned.is_ok() && rx.recv().unwrap_or(false)
    }

    pub fn backspaces(n: usize) -> bool {
        let key = |up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_BACK, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { 0 }, time: 0, dwExtraInfo: 0 } },
        };
        let inputs: Vec<INPUT> = (0..n).flat_map(|_| [key(false), key(true)]).collect();
        unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) == inputs.len() as u32 }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::c_void;
    use std::sync::OnceLock;

    use super::Typed;

    type Callback = extern "C" fn(*mut c_void, u32, *mut c_void, *mut c_void) -> *mut c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventTapCreate(tap: u32, place: u32, options: u32, events: u64, callback: Callback, user: *mut c_void) -> *mut c_void;
        fn CGEventTapEnable(tap: *mut c_void, enable: bool);
        fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
        fn CGEventGetFlags(event: *mut c_void) -> u64;
        fn CGEventKeyboardGetUnicodeString(event: *mut c_void, max: usize, actual: *mut usize, out: *mut u16);
        fn CGEventCreateKeyboardEvent(source: *const c_void, key: u16, down: bool) -> *mut c_void;
        fn CGEventPost(tap: u32, event: *mut c_void);
        fn CGPreflightListenEventAccess() -> bool;
        fn CGRequestListenEventAccess() -> bool;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFMachPortCreateRunLoopSource(allocator: *const c_void, port: *mut c_void, order: isize) -> *mut c_void;
        fn CFRunLoopGetCurrent() -> *const c_void;
        fn CFRunLoopAddSource(run_loop: *const c_void, source: *mut c_void, mode: *const c_void);
        fn CFRunLoopRun();
        fn CFRelease(object: *const c_void);
        static kCFRunLoopCommonModes: *const c_void;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    const SESSION_TAP: u32 = 1;
    const HEAD: u32 = 0;
    const LISTEN_ONLY: u32 = 1;
    const KEY_DOWN: u32 = 10;
    const MOUSE_DOWNS: [u32; 3] = [1, 3, 25];
    const TAP_DISABLED: [u32; 2] = [0xFFFF_FFFE, 0xFFFF_FFFF];
    const KEYCODE_FIELD: u32 = 9;
    const SOURCE_PID_FIELD: u32 = 41;
    const CONTROL: u64 = 1 << 18;
    const COMMAND: u64 = 1 << 20;
    const BACKSPACE: u16 = 51;

    static SEND: OnceLock<fn(Typed)> = OnceLock::new();
    /// The tap, to turn back on when macOS turns it off for being slow.
    static TAP: std::sync::atomic::AtomicPtr<c_void> = std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

    fn meaning(event: *mut c_void) -> Option<Typed> {
        unsafe {
            if CGEventGetIntegerValueField(event, SOURCE_PID_FIELD) == std::process::id() as i64 {
                return None;
            }
            let key = CGEventGetIntegerValueField(event, KEYCODE_FIELD);
            match key {
                51 => return Some(Typed::Back),
                // Return, Tab, Esc, keypad Enter, Home, Page Up, Delete, End, Page Down, the arrows.
                36 | 48 | 53 | 76 | 115 | 116 | 117 | 119 | 121 | 123..=126 => return Some(Typed::Reset),
                _ => {}
            }
            if CGEventGetFlags(event) & (CONTROL | COMMAND) != 0 {
                return Some(Typed::Reset);
            }
            let mut out = [0u16; 4];
            let mut n = 0usize;
            CGEventKeyboardGetUnicodeString(event, out.len(), &mut n, out.as_mut_ptr());
            if n != 1 {
                return None;
            }
            char::from_u32(out[0] as u32).filter(|c| !c.is_control()).map(Typed::Char)
        }
    }

    extern "C" fn tapped(_proxy: *mut c_void, kind: u32, event: *mut c_void, _user: *mut c_void) -> *mut c_void {
        if TAP_DISABLED.contains(&kind) {
            let tap = TAP.load(std::sync::atomic::Ordering::Relaxed);
            if !tap.is_null() {
                unsafe { CGEventTapEnable(tap, true) };
            }
            return event;
        }
        let Some(send) = SEND.get() else { return event };
        if MOUSE_DOWNS.contains(&kind) {
            send(Typed::Reset);
        } else if kind == KEY_DOWN {
            if let Some(t) = meaning(event) {
                send(t);
            }
        }
        event
    }

    pub fn start(send: fn(Typed)) -> bool {
        let _ = SEND.set(send);
        unsafe {
            if !CGPreflightListenEventAccess() && !AXIsProcessTrusted() {
                // Asks once, in System Settings; the tap works from the next start after it's allowed.
                CGRequestListenEventAccess();
            }
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("kairo-keywords".into()).spawn(move || unsafe {
            let mask = (1u64 << KEY_DOWN) | MOUSE_DOWNS.iter().fold(0u64, |m, k| m | (1u64 << k));
            let tap = CGEventTapCreate(SESSION_TAP, HEAD, LISTEN_ONLY, mask, tapped, std::ptr::null_mut());
            let _ = tx.send(!tap.is_null());
            if tap.is_null() {
                return;
            }
            TAP.store(tap, std::sync::atomic::Ordering::Relaxed);
            let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
            CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);
            CFRunLoopRun();
        });
        spawned.is_ok() && rx.recv().unwrap_or(false)
    }

    pub fn backspaces(n: usize) -> bool {
        if unsafe { !AXIsProcessTrusted() } {
            return false;
        }
        unsafe {
            for _ in 0..n {
                for down in [true, false] {
                    let event = CGEventCreateKeyboardEvent(std::ptr::null(), BACKSPACE, down);
                    if event.is_null() {
                        return false;
                    }
                    CGEventPost(0, event);
                    CFRelease(event as *const c_void);
                }
            }
        }
        true
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use std::sync::OnceLock;

    use x11rb::connection::Connection;
    use x11rb::protocol::record::{self, ConnectionExt as _};
    use x11rb::protocol::xproto::ConnectionExt as _;

    use super::Typed;

    static SEND: OnceLock<fn(Typed)> = OnceLock::new();

    const KEY_PRESS: u8 = 2;
    const KEY_RELEASE: u8 = 3;
    const BUTTON_PRESS: u8 = 4;

    /// A keysym as a character, for the ones that type one.
    pub(super) fn keysym_char(sym: u32) -> Option<char> {
        match sym {
            0x20..=0x7e | 0xa0..=0xff => char::from_u32(sym),
            s if s & 0xff00_0000 == 0x0100_0000 => char::from_u32(s & 0x00ff_ffff),
            _ => None,
        }
    }

    /// Modifier keys held, as Kairo follows them from presses and releases.
    #[derive(Default)]
    struct Held {
        shift: u8,
        shortcut: u8,
        caps: bool,
    }

    fn meaning(syms: &[u32], held: &Held) -> Option<Typed> {
        let base = *syms.first()?;
        match base {
            0xff08 => return Some(Typed::Back),
            // Return, Tab, Escape, Home, the arrows, Page Up/Down, End, keypad Enter, Delete.
            0xff0d | 0xff09 | 0xff1b | 0xff50..=0xff57 | 0xff8d | 0xffff => return Some(Typed::Reset),
            _ => {}
        }
        if held.shortcut > 0 {
            return Some(Typed::Reset);
        }
        let shifted = syms.get(1).copied().filter(|s| *s != 0).unwrap_or(base);
        let sym = if held.shift > 0 { shifted } else { base };
        let c = keysym_char(sym)?;
        // Caps Lock swaps a letter's case.
        Some(Typed::Char(if held.caps && c.is_alphabetic() {
            if c.is_lowercase() {
                c.to_uppercase().next().unwrap_or(c)
            } else {
                c.to_lowercase().next().unwrap_or(c)
            }
        } else {
            c
        }))
    }

    fn listen() -> Option<()> {
        let (ctrl, _) = x11rb::connect(None).ok()?;
        let (data, _) = x11rb::connect(None).ok()?;
        let setup = ctrl.setup();
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let map = ctrl.get_keyboard_mapping(min, max - min + 1).ok()?.reply().ok()?;
        let per = map.keysyms_per_keycode.max(1) as usize;
        let table: Vec<&[u32]> = map.keysyms.chunks(per).collect();
        let none: &[u32] = &[];
        let syms = |code: u8| code.checked_sub(min).and_then(|i| table.get(i as usize).copied()).unwrap_or(none);
        let context = ctrl.generate_id().ok()?;
        let none8 = record::Range8 { first: 0, last: 0 };
        let none_ext = record::ExtRange { major: none8, minor: record::Range16 { first: 0, last: 0 } };
        let range = record::Range {
            core_requests: none8,
            core_replies: none8,
            ext_requests: none_ext,
            ext_replies: none_ext,
            delivered_events: none8,
            device_events: record::Range8 { first: KEY_PRESS, last: BUTTON_PRESS },
            errors: none8,
            client_started: false,
            client_died: false,
        };
        ctrl.record_create_context(context, 0, &[record::CS::ALL_CLIENTS.into()], &[range]).ok()?.check().ok()?;
        let send = *SEND.get()?;
        let mut held = Held::default();
        for reply in data.record_enable_context(context).ok()? {
            let Ok(reply) = reply else { break };
            // 0: from the server; the rest says the recording started or stopped.
            if reply.category != 0 {
                continue;
            }
            for event in reply.data.chunks_exact(32) {
                let (kind, code) = (event[0] & 0x7f, event[1]);
                let first = syms(code).first().copied().unwrap_or(0);
                let down = kind == KEY_PRESS;
                match (kind, first) {
                    (BUTTON_PRESS, _) => send(Typed::Reset),
                    (KEY_PRESS | KEY_RELEASE, 0xffe1 | 0xffe2) => held.shift = if down { held.shift.saturating_add(1) } else { held.shift.saturating_sub(1) },
                    // Ctrl, Alt, Super and Meta: a shortcut while held.
                    (KEY_PRESS | KEY_RELEASE, 0xffe3 | 0xffe4 | 0xffe7..=0xffec) => {
                        held.shortcut = if down { held.shortcut.saturating_add(1) } else { held.shortcut.saturating_sub(1) }
                    }
                    (KEY_PRESS, 0xffe5) => held.caps = !held.caps,
                    (KEY_PRESS, _) => {
                        if let Some(t) = meaning(syms(code), &held) {
                            send(t);
                        }
                    }
                    _ => {}
                }
            }
        }
        Some(())
    }

    pub fn start(send: fn(Typed)) -> bool {
        let _ = SEND.set(send);
        // RECORD is asked for up front, so a server without it says no here rather than later.
        let has_record = x11rb::connect(None).ok().is_some_and(|(c, _)| c.record_query_version(1, 13).ok().and_then(|v| v.reply().ok()).is_some());
        has_record && std::thread::Builder::new().name("kairo-keywords".into()).spawn(|| listen().unwrap_or(())).is_ok()
    }

    pub fn backspaces(n: usize) -> bool {
        super::super::x11::backspaces(n)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn keys_mean_characters_with_shift_and_caps_lock() {
            let a = [0x61, 0x41];
            let one = [0x31, 0x21];
            let plain = Held::default();
            assert_eq!(meaning(&a, &plain), Some(Typed::Char('a')));
            assert_eq!(meaning(&one, &Held { shift: 1, ..Held::default() }), Some(Typed::Char('!')));
            assert_eq!(meaning(&a, &Held { caps: true, ..Held::default() }), Some(Typed::Char('A')));
            assert_eq!(meaning(&a, &Held { shortcut: 1, ..Held::default() }), Some(Typed::Reset));
            assert_eq!(meaning(&[0xff08], &plain), Some(Typed::Back));
            assert_eq!(meaning(&[0xff51], &plain), Some(Typed::Reset));
            assert_eq!(keysym_char(0x0100_00e9), Some('é'));
        }
    }
}
