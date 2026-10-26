//! GNOME on Wayland, where apps can't see each other's windows: Kairo's own small GNOME Shell
//! extension (`gnome-extension/`) answers which window is in front, over the session bus. Kairo
//! installs it itself, and GNOME loads it at the next login.

use std::path::PathBuf;
use std::sync::OnceLock;

use zbus::blocking::Connection;

pub const UUID: &str = "kairo@uwuexdeemeow.github.io";
const PATH: &str = "/org/gnome/Shell/Extensions/Kairo";
const INTERFACE: &str = "org.gnome.Shell.Extensions.Kairo";

/// Windows the extension names are told apart from X11 ones by this bit.
pub const WINDOW: usize = 1 << (usize::BITS - 1);

pub struct Focused {
    pub pid: u32,
    pub title: String,
    /// The window, with `WINDOW` set.
    pub window: usize,
    pub fullscreen: bool,
}

fn session() -> Option<&'static Connection> {
    static SESSION: OnceLock<Option<Connection>> = OnceLock::new();
    SESSION.get_or_init(|| Connection::session().ok()).as_ref()
}

/// Whether this is a Wayland session, where X11 sees only XWayland's apps.
pub fn wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// What the extension says is in front: None when it isn't running, Some(None) when nothing is.
pub fn focused() -> Option<Option<Focused>> {
    let reply = session()?.call_method(Some("org.gnome.Shell"), PATH, Some(INTERFACE), "Focused", &()).ok()?;
    let (pid, title, id, fullscreen): (u32, String, u64, bool) = reply.body().deserialize().ok()?;
    Some((pid != 0).then(|| Focused { pid, title, window: id as usize | WINDOW, fullscreen }))
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
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    if !wayland() || !desktop.split(':').any(|d| d.eq_ignore_ascii_case("GNOME")) {
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
    let run = |args: &[&str]| std::process::Command::new("gsettings").args(args).output().ok().filter(|o| o.status.success());
    let Some(out) = run(&["get", "org.gnome.shell", "enabled-extensions"]) else { return };
    let current = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if let Some(list) = with_extension(&current) {
        run(&["set", "org.gnome.shell", "enabled-extensions", &list]);
    }
}

/// GNOME's list of turned-on extensions with Kairo's added, as gsettings prints and takes it
/// (`['a@b', 'c@d']`, or `@as []` when empty); None when it's in already.
fn with_extension(list: &str) -> Option<String> {
    let inner = list.strip_prefix("@as ").unwrap_or(list).trim().strip_prefix('[')?.strip_suffix(']')?.trim();
    let mut uuids: Vec<&str> = inner.split(',').map(|s| s.trim().trim_matches('\'')).filter(|s| !s.is_empty()).collect();
    if uuids.contains(&UUID) {
        return None;
    }
    uuids.push(UUID);
    Some(format!("[{}]", uuids.iter().map(|u| format!("'{u}'")).collect::<Vec<_>>().join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_joins_the_ones_already_on() {
        assert_eq!(with_extension("@as []").as_deref(), Some("['kairo@uwuexdeemeow.github.io']"));
        assert_eq!(
            with_extension("['ubuntu-dock@ubuntu.com', 'ding@rastersoft.com']").as_deref(),
            Some("['ubuntu-dock@ubuntu.com', 'ding@rastersoft.com', 'kairo@uwuexdeemeow.github.io']")
        );
        assert_eq!(with_extension("['kairo@uwuexdeemeow.github.io']"), None);
    }

    #[test]
    fn shell_versions_are_read() {
        assert_eq!(parse_version("GNOME Shell 46.0\n"), Some(46));
        assert_eq!(parse_version("GNOME Shell 42.9"), Some(42));
        assert_eq!(parse_version(""), None);
    }
}
