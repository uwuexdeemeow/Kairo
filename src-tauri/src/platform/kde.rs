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
const WINDOWS_SCRIPT: &str = "kairo-windows";
const ACTIVATE_SCRIPT: &str = "kairo-activate";
const CLOSE_SCRIPT: &str = "kairo-close";
/// How long the palette waits for the window list a fresh script sends.
const WINDOWS_WAIT: std::time::Duration = std::time::Duration::from_millis(150);
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
/// The window list the script last sent, and how many it has sent.
static WINDOWS: Mutex<(Vec<super::OpenWindow>, u64)> = Mutex::new((Vec::new(), 0));
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

    fn windows(&self, json: String) {
        let mut list = WINDOWS.lock().unwrap_or_else(|e| e.into_inner());
        *list = (parse_windows(&json), list.1 + 1);
    }
}

/// One window as the script lists it; all text, as KWin sends it.
#[derive(serde::Deserialize)]
struct Listed {
    id: String,
    title: String,
    pid: String,
}

/// The script's window list, front first, with ids marked "k"; ids that aren't KWin's own are
/// left out, since they go back into scripts.
fn parse_windows(json: &str) -> Vec<super::OpenWindow> {
    serde_json::from_str::<Vec<Listed>>(json)
        .unwrap_or_default()
        .into_iter()
        .filter(|w| is_window_id(&w.id))
        .map(|w| super::OpenWindow { id: format!("k{}", w.id), title: w.title, pid: w.pid.parse().unwrap_or(0), path: None })
        .collect()
}

/// The windows KWin lists, asked for afresh and waited for briefly: None off Plasma's Wayland
/// session; the last list when the fresh one is slow.
pub fn windows() -> Option<Vec<super::OpenWindow>> {
    let bus = BUS.get()?;
    let before = WINDOWS.lock().unwrap_or_else(|e| e.into_inner()).1;
    if let Some(script) = write_script("windows.js", include_str!("../../kwin-script/windows.js")) {
        let _ = run_script(bus, &script, WINDOWS_SCRIPT);
    }
    let until = std::time::Instant::now() + WINDOWS_WAIT;
    while WINDOWS.lock().unwrap_or_else(|e| e.into_inner()).1 == before && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let mut windows = WINDOWS.lock().unwrap_or_else(|e| e.into_inner()).0.clone();
    for w in &mut windows {
        w.path = super::program(w.pid);
    }
    Some(windows)
}

/// Runs a one-off script on one window, by KWin's id.
fn on_window(id: &str, file: &str, text: &str, name: &str) -> bool {
    let Some(bus) = BUS.get() else { return false };
    if !is_window_id(id) {
        return false;
    }
    write_script(file, &text.replace("KAIRO_WINDOW", id)).is_some_and(|script| run_script(bus, &script, name).is_ok())
}

/// Brings forward a window the script listed, by its id without the "k".
pub fn activate(id: &str) -> bool {
    on_window(id, "activate.js", include_str!("../../kwin-script/activate.js"), ACTIVATE_SCRIPT)
}

/// Asks a window the script listed to close, by its id without the "k".
pub fn close(id: &str) -> bool {
    on_window(id, "close.js", include_str!("../../kwin-script/close.js"), CLOSE_SCRIPT)
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

const COMPONENT: &str = "kairo";
const PALETTE_ACTION: &str = "palette";
static OPEN_PALETTE: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// What to do when Plasma says the palette's shortcut was pressed.
pub fn on_palette_shortcut(open: Box<dyn Fn() + Send + Sync>) {
    let _ = OPEN_PALETTE.set(open);
}

/// Makes `shortcut` one of Plasma's global shortcuts, which its Wayland session keeps for itself,
/// and listens for it; false when Kairo isn't watching Plasma or Plasma refused it.
pub fn palette_shortcut(shortcut: &str) -> bool {
    static LISTENING: std::sync::Once = std::sync::Once::new();
    let (Some(bus), Some(keys)) = (BUS.get(), qt_keys(shortcut)) else { return false };
    let id = vec![COMPONENT, PALETTE_ACTION, "Kairo", "Open the command palette"];
    if global_shortcuts(bus, "doRegister", &(id.clone(),)).is_err() {
        return false;
    }
    // Set even where Plasma has another for it, and not taken back from Plasma's own settings.
    const SET_PRESENT_NO_AUTOLOADING: u32 = 2 | 4;
    // Plasma 6 takes key sequences; Plasma 5 key codes.
    let set = global_shortcuts(bus, "setShortcutKeys", &(id.clone(), vec![(vec![keys],)], SET_PRESENT_NO_AUTOLOADING)).is_ok()
        || global_shortcuts(bus, "setShortcut", &(id, vec![keys], SET_PRESENT_NO_AUTOLOADING)).is_ok();
    if set {
        LISTENING.call_once(|| {
            let _ = std::thread::Builder::new().name("kairo-kde-keys".into()).spawn(|| {
                if let Err(e) = listen_for_presses() {
                    eprintln!("kde: cannot hear the palette's shortcut: {e}");
                }
            });
        });
    }
    set
}

fn global_shortcuts<B: serde::Serialize + zbus::zvariant::DynamicType>(bus: &Connection, method: &str, body: &B) -> zbus::Result<()> {
    bus.call_method(Some("org.kde.kglobalaccel"), "/kglobalaccel", Some("org.kde.KGlobalAccel"), method, body).map(|_| ())
}

fn listen_for_presses() -> zbus::Result<()> {
    let Some(bus) = BUS.get() else { return Ok(()) };
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.kde.kglobalaccel.Component")?
        .member("globalShortcutPressed")?
        .path(format!("/component/{COMPONENT}"))?
        .build();
    for message in zbus::blocking::MessageIterator::for_match_rule(rule, bus, None)?.flatten() {
        let Ok((_, action, _)) = message.body().deserialize::<(String, String, i64)>() else { continue };
        if action == PALETTE_ACTION {
            if let Some(open) = OPEN_PALETTE.get() {
                open();
            }
        }
    }
    Ok(())
}

/// A shortcut as Kairo saves it ("Ctrl+Shift+K") as Qt's key code, which Plasma speaks.
fn qt_keys(shortcut: &str) -> Option<i32> {
    let mut code = 0i32;
    let mut key = None;
    for part in shortcut.split('+') {
        match part {
            "Shift" => code |= 0x0200_0000,
            "Ctrl" => code |= 0x0400_0000,
            "Alt" => code |= 0x0800_0000,
            "Super" => code |= 0x1000_0000,
            "Space" => key = Some(0x20),
            k if k.len() == 1 && k.chars().all(|c| c.is_ascii_alphanumeric()) => key = Some(k.to_ascii_uppercase().as_bytes()[0] as i32),
            k if k.starts_with('F') => key = Some(0x0100_0030 + k[1..].parse::<i32>().ok()?.checked_sub(1)?),
            _ => return None,
        }
    }
    Some(code | key?)
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
    fn shortcuts_become_qt_key_codes() {
        assert_eq!(qt_keys("Ctrl+Space"), Some(0x0400_0020));
        assert_eq!(qt_keys("Ctrl+Shift+K"), Some(0x0600_004B));
        assert_eq!(qt_keys("Super+F1"), Some(0x1100_0030));
        assert_eq!(qt_keys("Alt+7"), Some(0x0800_0037));
        assert_eq!(qt_keys("Ctrl"), None);
    }

    #[test]
    fn windows_keep_their_number() {
        let first = number("{a}");
        assert_eq!(number("{b}"), first + 1);
        assert_eq!(number("{a}"), first);
    }

    #[test]
    fn window_lists_are_read() {
        let got = parse_windows(r#"[{"id": "{a1b2-c3}", "title": "Dolphin", "pid": "41"}, {"id": "x\"); evil()", "title": "Bad", "pid": "1"}]"#);
        assert_eq!(got.iter().map(|w| (w.id.as_str(), w.title.as_str(), w.pid)).collect::<Vec<_>>(), [("k{a1b2-c3}", "Dolphin", 41)]);
        assert!(parse_windows("[").is_empty());
    }
}
