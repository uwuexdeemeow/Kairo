//! GNOME on Wayland, where apps can't see each other's windows: Kairo's own small GNOME Shell
//! extension (`gnome-extension/`) answers which window is in front, over the session bus. Kairo
//! installs it itself, and GNOME loads it at the next login.

use std::path::PathBuf;
use std::sync::OnceLock;

use zbus::blocking::Connection;

use super::{desktop_is, wayland, WaylandWindow};

pub const UUID: &str = "kairo@uwuexdeemeow.github.io";
const PATH: &str = "/org/gnome/Shell/Extensions/Kairo";
const INTERFACE: &str = "org.gnome.Shell.Extensions.Kairo";

/// Windows the extension names are told apart from X11 and KDE ones by this bit.
pub const WINDOW: usize = 1 << (usize::BITS - 1);

fn session() -> Option<&'static Connection> {
    static SESSION: OnceLock<Option<Connection>> = OnceLock::new();
    SESSION.get_or_init(|| Connection::session().ok()).as_ref()
}

/// What the extension says is in front: None when it isn't running, Some(None) when nothing is.
pub fn focused() -> Option<Option<WaylandWindow>> {
    if !desktop_is("GNOME") {
        return None;
    }
    let reply = session()?.call_method(Some("org.gnome.Shell"), PATH, Some(INTERFACE), "Focused", &()).ok()?;
    let (pid, title, id, fullscreen): (u32, String, u64, bool) = reply.body().deserialize().ok()?;
    Some((pid != 0).then(|| WaylandWindow { pid, title, window: id as usize | WINDOW, fullscreen }))
}

/// One window as the extension lists it.
#[derive(serde::Deserialize)]
struct Listed {
    id: u64,
    title: String,
    pid: u32,
}

/// The extension's window list, newest first, with ids marked "g"; empty when it can't be read.
fn parse_windows(json: &str) -> Vec<super::OpenWindow> {
    serde_json::from_str::<Vec<Listed>>(json)
        .unwrap_or_default()
        .into_iter()
        .map(|w| super::OpenWindow { id: format!("g{}", w.id), title: w.title, pid: w.pid, path: None })
        .collect()
}

/// The windows the extension lists: None off GNOME, or when the extension running is one from
/// before it could list them (it's replaced at the next login), which is said once.
pub fn windows() -> Option<Vec<super::OpenWindow>> {
    static TOLD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !desktop_is("GNOME") {
        return None;
    }
    let reply = match session()?.call_method(Some("org.gnome.Shell"), PATH, Some(INTERFACE), "Windows", &()) {
        Ok(reply) => reply,
        Err(e) => {
            if !TOLD.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!("gnome: cannot list windows until the next login: {e}");
            }
            return None;
        }
    };
    let json: String = reply.body().deserialize().ok()?;
    let mut windows = parse_windows(&json);
    for w in &mut windows {
        w.path = super::program(w.pid);
    }
    Some(windows)
}

fn ask(method: &str, id: &str) -> bool {
    let (Some(bus), Ok(id)) = (session(), id.parse::<u64>()) else { return false };
    bus.call_method(Some("org.gnome.Shell"), PATH, Some(INTERFACE), method, &(id,))
        .ok()
        .and_then(|reply| reply.body().deserialize::<bool>().ok())
        .unwrap_or(false)
}

/// Brings forward a window the extension listed, by its id without the "g".
pub fn activate(id: &str) -> bool {
    ask("Activate", id)
}

/// Asks a window the extension listed to close, by its id without the "g".
pub fn close(id: &str) -> bool {
    ask("Close", id)
}

/// Minimizes a window the extension named.
pub fn minimize(window: usize) -> bool {
    let Some(bus) = session() else { return false };
    let id = (window & !WINDOW) as u64;
    bus.call_method(Some("org.gnome.Shell"), PATH, Some(INTERFACE), "Minimize", &(id,))
        .ok()
        .and_then(|reply| reply.body().deserialize::<bool>().ok())
        .unwrap_or(false)
}

/// Time since the last keyboard or mouse input, from GNOME's own idle monitor.
pub fn idle_ms() -> Option<i64> {
    let reply = session()?
        .call_method(
            Some("org.gnome.Mutter.IdleMonitor"),
            "/org/gnome/Mutter/IdleMonitor/Core",
            Some("org.gnome.Mutter.IdleMonitor"),
            "GetIdletime",
            &(),
        )
        .ok()?;
    let ms: u64 = reply.body().deserialize().ok()?;
    Some(ms as i64)
}

/// Puts the extension where GNOME looks for it and turns it on, on GNOME's Wayland session.
/// True when it's newly there, since GNOME only loads it at the next login.
pub fn install() -> bool {
    if !wayland() || !desktop_is("GNOME") {
        return false;
    }
    let Some(dir) = extensions_dir().map(|d| d.join(UUID)) else { return false };
    // GNOME 45 changed how extensions load; older ones need the older form.
    let (extension, metadata) = if shell_version().is_some_and(|v| v < 45) {
        (include_str!("../../gnome-extension/legacy/extension.js"), include_str!("../../gnome-extension/legacy/metadata.json"))
    } else {
        (include_str!("../../gnome-extension/modern/extension.js"), include_str!("../../gnome-extension/modern/metadata.json"))
    };
    let new = !dir.join("extension.js").exists();
    let written = std::fs::create_dir_all(&dir).is_ok()
        && [("extension.js", extension), ("metadata.json", metadata)].iter().all(|(file, text)| {
            let path = dir.join(file);
            std::fs::read_to_string(&path).is_ok_and(|old| old == *text) || std::fs::write(&path, text).is_ok()
        });
    if !written {
        eprintln!("gnome: cannot write the extension to {}", dir.display());
        return false;
    }
    enable();
    new
}

fn extensions_dir() -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(data.join("gnome-shell/extensions"))
}

/// GNOME Shell's major version, from `gnome-shell --version` ("GNOME Shell 46.0").
fn shell_version() -> Option<u32> {
    let out = std::process::Command::new("gnome-shell").arg("--version").output().ok()?;
    parse_version(&String::from_utf8_lossy(&out.stdout))
}

fn parse_version(text: &str) -> Option<u32> {
    text.split_whitespace().last()?.split('.').next()?.parse().ok()
}

/// Adds the extension to the ones GNOME turns on at login, keeping the rest, unless it's there.
fn enable() {
    add_to_list("org.gnome.shell", "enabled-extensions", UUID);
}

/// Runs gsettings; its output when it worked.
fn gsettings(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("gsettings").args(args).output().ok().filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Adds `item` to a list setting, keeping what's there; true when it's in the list after.
fn add_to_list(schema: &str, key: &str, item: &str) -> bool {
    let Some(current) = gsettings(&["get", schema, key]) else { return false };
    match with_item(&current, item) {
        Some(list) => gsettings(&["set", schema, key, &list]).is_some(),
        None => true,
    }
}

/// A list as gsettings prints and takes it (`['a', 'b']`, or `@as []` when empty) with `item`
/// added at the end; None when it's in already.
fn with_item(list: &str, item: &str) -> Option<String> {
    let inner = list.strip_prefix("@as ").unwrap_or(list).trim().strip_prefix('[')?.strip_suffix(']')?.trim();
    let mut items: Vec<&str> = inner.split(',').map(|s| s.trim().trim_matches('\'')).filter(|s| !s.is_empty()).collect();
    if items.contains(&item) {
        return None;
    }
    items.push(item);
    Some(format!("[{}]", items.iter().map(|i| format!("'{i}'")).collect::<Vec<_>>().join(", ")))
}

const KEYBINDINGS: &str = "org.gnome.settings-daemon.plugins.media-keys";
const PALETTE_KEYBINDING: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/kairo-palette/";

/// Makes `shortcut` open the palette as one of GNOME's own keyboard shortcuts, on its Wayland
/// session, where apps can't listen for keys; it runs `kairo --palette`. False elsewhere, or when
/// GNOME didn't take it, which leaves Kairo listening itself.
pub fn palette_shortcut(shortcut: &str) -> bool {
    if !wayland() || !desktop_is("GNOME") {
        return false;
    }
    let (Some(binding), Some(command)) = (accelerator(shortcut), palette_command()) else { return false };
    let schema = format!("{KEYBINDINGS}.custom-keybinding:{PALETTE_KEYBINDING}");
    let set = [("name", "Kairo command palette".to_string()), ("command", command), ("binding", binding)]
        .iter()
        .all(|(key, value)| gsettings(&["set", &schema, key, &quoted(value)]).is_some());
    set && add_to_list(KEYBINDINGS, "custom-keybindings", PALETTE_KEYBINDING)
}

/// How GNOME's shortcut runs this copy of Kairo: the AppImage it came from, if it did.
fn palette_command() -> Option<String> {
    let program = std::env::var_os("APPIMAGE").map(PathBuf::from).or_else(|| std::env::current_exe().ok())?;
    let program = program.to_str()?;
    let program = if program.contains(' ') { format!("\"{program}\"") } else { program.to_string() };
    Some(format!("{program} {}", crate::startup::PALETTE_ARG))
}

/// A shortcut as Kairo saves it ("Ctrl+Shift+K") in GNOME's spelling ("<Control><Shift>k").
fn accelerator(shortcut: &str) -> Option<String> {
    let mut out = String::new();
    for part in shortcut.split('+') {
        match part {
            "Ctrl" => out.push_str("<Control>"),
            "Alt" => out.push_str("<Alt>"),
            "Shift" => out.push_str("<Shift>"),
            "Super" => out.push_str("<Super>"),
            "Space" => out.push_str("space"),
            key if key.len() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric()) => out.push_str(&key.to_ascii_lowercase()),
            key if key.starts_with('F') && key[1..].parse::<u8>().is_ok() => out.push_str(key),
            _ => return None,
        }
    }
    Some(out)
}

/// A text value as gsettings reads it.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_joins_the_ones_already_on() {
        assert_eq!(with_item("@as []", UUID).as_deref(), Some("['kairo@uwuexdeemeow.github.io']"));
        assert_eq!(
            with_item("['ubuntu-dock@ubuntu.com', 'ding@rastersoft.com']", UUID).as_deref(),
            Some("['ubuntu-dock@ubuntu.com', 'ding@rastersoft.com', 'kairo@uwuexdeemeow.github.io']")
        );
        assert_eq!(with_item("['kairo@uwuexdeemeow.github.io']", UUID), None);
    }

    #[test]
    fn shortcuts_are_spelled_the_gnome_way() {
        assert_eq!(accelerator("Ctrl+Space").as_deref(), Some("<Control>space"));
        assert_eq!(accelerator("Ctrl+Shift+K").as_deref(), Some("<Control><Shift>k"));
        assert_eq!(accelerator("Super+F5").as_deref(), Some("<Super>F5"));
        assert_eq!(accelerator("Alt+Tab"), None);
        assert_eq!(quoted("it's"), r"'it\'s'");
    }

    #[test]
    fn shell_versions_are_read() {
        assert_eq!(parse_version("GNOME Shell 46.0\n"), Some(46));
        assert_eq!(parse_version("GNOME Shell 42.9"), Some(42));
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn window_lists_are_read() {
        let got = parse_windows(r#"[{"id": 12, "title": "Files", "pid": 300}, {"id": 7, "title": "", "pid": 0}]"#);
        assert_eq!(got.iter().map(|w| (w.id.as_str(), w.title.as_str(), w.pid)).collect::<Vec<_>>(), [("g12", "Files", 300), ("g7", "", 0)]);
        assert!(parse_windows("not json").is_empty());
    }
}
