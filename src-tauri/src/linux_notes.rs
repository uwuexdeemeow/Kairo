//! What Linux can't do on this desktop, said in Kairo's settings instead of only in a log or a
//! README. Nothing here changes what Kairo does; it names the gaps so they aren't silent.

use std::sync::Mutex;

/// Shortcuts the desktop refused, as "the palette's Ctrl+Space", kept for the settings page.
static REFUSED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Notes that a global shortcut couldn't be taken, so settings can say so. Only Linux reports it:
/// there another app or the desktop often holds the keys, and nothing else would tell the user.
pub fn shortcut_refused(what: &str, keys: &str) {
    if !cfg!(target_os = "linux") {
        return;
    }
    if let Ok(mut refused) = REFUSED.lock() {
        let line = format!("{what}'s {keys}");
        if !refused.contains(&line) {
            refused.push(line);
        }
    }
}

/// The notes for a desktop: what it is and which helper programs are missing.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn notes_for(wayland: bool, gnome: bool, kde: bool, clipboard_tool: bool, refused: &[String]) -> Vec<String> {
    let mut notes = Vec::new();
    if wayland && !gnome && !kde {
        notes.push("This Wayland desktop doesn't tell Kairo which window is in front or when you're away. Kairo sees only apps running through XWayland, and time away from the computer counts as active.".to_string());
        notes.push("Add a keyboard shortcut in your desktop's settings that runs kairo --palette, and another that runs kairo --ring for the app ring.".to_string());
    }
    if gnome {
        notes.push("No Kairo icon in the top bar? GNOME doesn't show tray icons without the AppIndicator extension. Open the dashboard from a launcher or with kairo --dashboard.".to_string());
    }
    if !refused.is_empty() {
        notes.push(format!("Another program or the desktop is holding {}. Choose different keys, or add a keyboard shortcut in your desktop's settings that runs kairo --palette.", refused.join(" and ")));
    }
    if !clipboard_tool {
        notes.push("Install wl-clipboard (Wayland) or xclip (X11) so Kairo can tell when a password manager marked a copy as hidden. Without one, those copies aren't kept out of the palette's clipboard history.".to_string());
    }
    notes
}

/// Whether a program is on the PATH.
#[cfg(target_os = "linux")]
fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

/// What this session can't do, for Kairo's settings. Empty on Windows and a Mac.
#[tauri::command]
pub fn linux_notes() -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        let desktop = |name: &str| std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.split(':').any(|d| d.eq_ignore_ascii_case(name)));
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        let refused = REFUSED.lock().map(|r| r.clone()).unwrap_or_default();
        notes_for(wayland, desktop("GNOME"), desktop("KDE"), on_path("wl-paste") || on_path("xclip"), &refused)
    }
    #[cfg(not(target_os = "linux"))]
    {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_desktop_that_can_do_everything_says_nothing() {
        assert!(notes_for(false, false, false, true, &[]).is_empty());
        assert!(notes_for(true, false, true, true, &[]).is_empty());
    }

    #[test]
    fn other_wayland_desktops_say_what_they_cant_see_and_how_to_open_the_palette() {
        let notes = notes_for(true, false, false, true, &[]);
        assert_eq!(notes.len(), 2);
        assert!(notes[0].contains("XWayland") && notes[0].contains("counts as active"));
        assert!(notes[1].contains("kairo --palette") && notes[1].contains("kairo --ring"));
        // GNOME and KDE hand Kairo what it needs.
        assert!(notes_for(true, true, false, true, &[]).iter().all(|n| !n.contains("XWayland")));
    }

    #[test]
    fn gnome_names_the_tray_extension_and_a_way_in_without_it() {
        let notes = notes_for(true, true, false, true, &[]);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("AppIndicator") && notes[0].contains("kairo --dashboard"));
    }

    #[test]
    fn a_refused_shortcut_and_a_missing_clipboard_tool_are_said() {
        let notes = notes_for(false, false, false, false, &["the palette's Ctrl+Space".to_string()]);
        assert_eq!(notes.len(), 2);
        assert!(notes[0].contains("the palette's Ctrl+Space"));
        assert!(notes[1].contains("wl-clipboard") && notes[1].contains("xclip"));
    }
}
