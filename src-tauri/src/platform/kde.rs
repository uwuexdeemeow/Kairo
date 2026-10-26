//! KDE Plasma on Wayland, where apps can't see each other's windows: Kairo loads a KWin script
//! (`kwin-script/`) that reports each change of the window in front back to Kairo over the
//! session bus, and a one-off script to minimize a blocked window.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use zbus::blocking::Connection;

use super::WaylandWindow;

const NAME: &str = "io.github.uwuexdeemeow.Kairo";
const PATH: &str = "/Kairo";
const FOCUS_SCRIPT: &str = "kairo-focus";
const MINIMIZE_SCRIPT: &str = "kairo-minimize";
/// Windows KWin names are told apart from X11 and GNOME ones by this bit.
pub const WINDOW: usize = 1 << (usize::BITS - 2);

struct Front {
    pid: u32,
    title: String,
    /// KWin's id for the window.
    id: String,
    fullscreen: bool,
}

/// The window in front as the script last said; None until it first has.
static FRONT: Mutex<Option<Option<Front>>> = Mutex::new(None);
/// KWin's ids, which are text, at the number Kairo gives each window.
static IDS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static BUS: OnceLock<Connection> = OnceLock::new();

struct Listener;

#[zbus::interface(name = "io.github.uwuexdeemeow.Kairo")]
impl Listener {
    fn focused(&self, pid: String, caption: String, id: String, fullscreen: String) {
        let pid = pid.parse().unwrap_or(0);
        let front = (pid != 0).then(|| Front { pid, title: caption, id, fullscreen: fullscreen == "1" });
        *FRONT.lock().unwrap_or_else(|e| e.into_inner()) = Some(front);
    }
}

/// Starts listening and loads the script into KWin, on Plasma's Wayland session.
pub fn start() {
    if !super::wayland() || !super::desktop_is("KDE") {
        return;
    }
    if let Err(e) = listen() {
        eprintln!("kde: cannot watch windows: {e}");
    }
}

fn listen() -> zbus::Result<()> {
    let bus = zbus::blocking::connection::Builder::session()?.name(NAME)?.serve_at(PATH, Listener)?.build()?;
    let bus = BUS.get_or_init(|| bus);
    let script = write_script("focus.js", include_str!("../../kwin-script/focus.js"))
        .ok_or_else(|| zbus::Error::Failure("cannot write the KWin script".into()))?;
    run_script(bus, &script, FOCUS_SCRIPT)
}

/// What the script says is in front: None before it has said, Some(None) when nothing is.
pub fn focused() -> Option<Option<WaylandWindow>> {
    let front = FRONT.lock().unwrap_or_else(|e| e.into_inner());
    let front = front.as_ref()?;
    Some(front.as_ref().map(|f| WaylandWindow { pid: f.pid, title: f.title.clone(), window: number(&f.id) | WINDOW, fullscreen: f.fullscreen }))
}

fn number(id: &str) -> usize {
    let mut ids = IDS.lock().unwrap_or_else(|e| e.into_inner());
    ids.iter().position(|known| known == id).unwrap_or_else(|| {
        ids.push(id.to_string());
        ids.len() - 1
    })
}

/// Minimizes a window the script named, with a script of its own.
pub fn minimize(window: usize) -> bool {
    let Some(bus) = BUS.get() else { return false };
    let Some(id) = IDS.lock().unwrap_or_else(|e| e.into_inner()).get(window & !WINDOW).cloned() else { return false };
    // KWin's ids are UUIDs in braces; nothing else goes into a script.
    if !is_window_id(&id) {
        return false;
    }
    let text = include_str!("../../kwin-script/minimize.js").replace("KAIRO_WINDOW", &id);
    write_script("minimize.js", &text).is_some_and(|script| run_script(bus, &script, MINIMIZE_SCRIPT).is_ok())
}

fn is_window_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit() || matches!(c, '{' | '}' | '-'))
}

/// Time since the last keyboard or mouse input, from Plasma's screen locker, which counts it in
/// milliseconds.
pub fn idle_ms() -> Option<i64> {
    let reply = BUS
        .get()?
        .call_method(
            Some("org.freedesktop.ScreenSaver"),
            "/org/freedesktop/ScreenSaver",
            Some("org.freedesktop.ScreenSaver"),
            "GetSessionIdleTime",
            &(),
        )
        .ok()?;
    let ms: u32 = reply.body().deserialize().ok()?;
    Some(ms as i64)
}

/// Loads a script into KWin and runs it, replacing one of the same name, as from an earlier run.
/// KWin reads the file on its own time, so a one-off script stays loaded until the next.
fn run_script(bus: &Connection, script: &Path, name: &str) -> zbus::Result<()> {
    let _ = scripting(bus, "unloadScript", &(name,));
    scripting(bus, "loadScript", &(script.to_string_lossy().into_owned(), name))?;
    scripting(bus, "start", &())
}

fn scripting<B: serde::Serialize + zbus::zvariant::DynamicType>(bus: &Connection, method: &str, body: &B) -> zbus::Result<()> {
    bus.call_method(Some("org.kde.KWin"), "/Scripting", Some("org.kde.kwin.Scripting"), method, body).map(|_| ())
}

/// Scripts live in Kairo's cache folder, where KWin reads them.
fn write_script(file: &str, text: &str) -> Option<PathBuf> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?
        .join("kairo");
    std::fs::create_dir_all(&cache).ok()?;
    let path = cache.join(file);
    std::fs::write(&path, text).ok()?;
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_kwin_window_ids_go_into_scripts() {
        assert!(is_window_id("{4b1c3a2e-9d7f-4e1a-8b2c-0123456789ab}"));
        assert!(!is_window_id("\"); workspace.windowList().forEach(w => w.closeWindow()); (\""));
        assert!(!is_window_id(""));
    }

    #[test]
    fn windows_keep_their_number() {
        let first = number("{a}");
        assert_eq!(number("{b}"), first + 1);
        assert_eq!(number("{a}"), first);
    }
}
