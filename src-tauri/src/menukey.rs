//! Alt+Space on Windows opens the window's own menu (Restore, Move, Size, Minimize, Close) before
//! a page gets to treat it as a key, and a page can't stop that. While Settings is recording a
//! shortcut this takes the request away from the window, so Alt+Space can be chosen; the rest of
//! the time the menu works as it always does. Nothing to do on a Mac or Linux.

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};

    use tauri::{AppHandle, WebviewWindow};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::WM_SYSCOMMAND;

    static RECORDING: AtomicBool = AtomicBool::new(false);
    /// The menu opened from the keyboard, as WM_SYSCOMMAND's wparam says it (low four bits are the system's).
    const SC_KEYMENU: usize = 0xF100;

    /// Whether this is the window menu being opened by Alt+Space (lparam holds the key's character).
    fn is_space_menu(msg: u32, wparam: usize, lparam: isize) -> bool {
        msg == WM_SYSCOMMAND && wparam & 0xFFF0 == SC_KEYMENU && lparam == ' ' as isize
    }

    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
        if RECORDING.load(Ordering::Relaxed) && is_space_menu(msg, wparam.0, lparam.0) {
            return LRESULT(0);
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }

    pub fn recording(app: &AppHandle, window: &WebviewWindow, on: bool) {
        RECORDING.store(on, Ordering::Relaxed);
        if !on {
            return;
        }
        let Ok(hwnd) = window.hwnd() else { return };
        let raw = hwnd.0 as isize;
        // A window is subclassed from the thread that made it. Asking again for the same hook is
        // harmless: it's the one already there.
        let _ = app.run_on_main_thread(move || unsafe {
            let _ = SetWindowSubclass(HWND(raw as *mut _), Some(proc), 1, 0);
        });
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn only_the_menu_from_alt_space_is_taken() {
            let space = ' ' as isize;
            assert!(is_space_menu(WM_SYSCOMMAND, 0xF100, space));
            // The system adds a few low bits of its own to wparam.
            assert!(is_space_menu(WM_SYSCOMMAND, 0xF10F, space));
            // Alt alone (lparam 0), other keys, other system commands and other messages are left.
            assert!(!is_space_menu(WM_SYSCOMMAND, 0xF100, 0));
            assert!(!is_space_menu(WM_SYSCOMMAND, 0xF100, 'f' as isize));
            assert!(!is_space_menu(WM_SYSCOMMAND, 0xF020, space));
            assert!(!is_space_menu(0x0100, 0xF100, space));
        }
    }
}

#[cfg(windows)]
pub use imp::recording;

#[cfg(not(windows))]
pub fn recording(_app: &tauri::AppHandle, _window: &tauri::WebviewWindow, _on: bool) {}
