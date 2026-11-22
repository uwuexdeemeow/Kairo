//! Pressing Ctrl+V (Command+V on a Mac) for a command that pastes: the text is already on the
//! clipboard, and the key goes to whatever window the palette gave the keyboard back to once it
//! is away. Where a system won't let Kairo press keys, nothing is pressed and the caller says so.

use std::thread;
use std::time::Duration;

/// How long to wait for the palette to be out of the way and the window behind it to have the
/// keyboard again, before the key is pressed.
const SETTLE: Duration = Duration::from_millis(250);

/// Presses paste shortly, once the palette is away. False when this system doesn't let Kairo
/// press keys (Wayland, or X11 without `xdotool`): nothing is pressed.
pub fn soon() -> bool {
    if !can() {
        return false;
    }
    thread::spawn(|| {
        thread::sleep(SETTLE);
        press();
    });
    true
}

#[cfg(windows)]
fn can() -> bool {
    true
}

#[cfg(windows)]
fn press() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP, VK_CONTROL};
    const V: u8 = 0x56;
    // SAFETY: plain key events, with no pointers; they go to whichever window has the keyboard.
    unsafe {
        keybd_event(VK_CONTROL.0 as u8, 0, Default::default(), 0);
        keybd_event(V, 0, Default::default(), 0);
        keybd_event(V, 0, KEYEVENTF_KEYUP, 0);
        keybd_event(VK_CONTROL.0 as u8, 0, KEYEVENTF_KEYUP, 0);
    }
}

#[cfg(target_os = "macos")]
fn can() -> bool {
    true
}

/// Through System Events, which asks once to allow Automation, as the system commands do.
#[cfg(target_os = "macos")]
fn press() {
    let _ = std::process::Command::new("osascript").args(["-e", "tell application \"System Events\" to keystroke \"v\" using command down"]).status();
}

/// X11 only, and only with `xdotool`: Wayland keeps apps from pressing keys in each other.
#[cfg(all(unix, not(target_os = "macos")))]
fn can() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_none() && on_path("xdotool")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn press() {
    let _ = std::process::Command::new("xdotool").args(["key", "--clearmodifiers", "ctrl+v"]).status();
}

/// What a command that pastes says when the keys can't be pressed: the text is copied, so it is one
/// press away.
pub const COPIED_INSTEAD: &str = "Copied. This system doesn't let Kairo press Ctrl+V for you, so paste it yourself.";
