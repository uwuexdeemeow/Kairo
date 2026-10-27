//! Blocking during focus sessions: distracting apps are minimized behind a Kairo "blocked" window,
//! game launchers are closed so a game can't be started, and the browser extension swaps
//! distracting sites for a blocked page. Pausing a session pauses all of it.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rusqlite::{params, Connection, Result};
use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_notification::NotificationExt;

use crate::analysis::{self, Category};
use crate::{classify, db, focus, platform};

const TICK: Duration = Duration::from_millis(500);
/// Launchers are looked for every this many ticks; listing processes costs more than a glance
/// at the window in front.
const LAUNCHER_EVERY_TICKS: u64 = 4;
const ENABLED_KEY: &str = "focus_block_enabled";
const LAUNCHERS_KEY: &str = "focus_block_launchers";
const STRICT_KEY: &str = "focus_strict";
pub const STRICT_WAIT_MS: i64 = 30_000;
/// How long a finished wait stays good for its confirming click.
const STRICT_CONFIRM_MS: i64 = 2 * 60_000;
pub const ALLOW_MINUTES: i64 = 5;
const MIN_MS: i64 = 60_000;
pub const WINDOW_LABEL: &str = "blocked";

/// Game launchers, as (executable stem, name), closed during focus sessions.
pub const LAUNCHERS: &[(&str, &str)] = &[
    ("steam", "Steam"),
    ("epicgameslauncher", "Epic Games"),
    ("riotclientservices", "Riot Client"),
    ("riotclientux", "Riot Client"),
    ("battle.net", "Battle.net"),
    ("eadesktop", "EA app"),
    ("upc", "Ubisoft Connect"),
    ("galaxyclient", "GOG Galaxy"),
];

/// Never blocked, whatever the rules say: Kairo itself and the parts of Windows you need to get
/// around.
const NEVER: &[&str] = &[
    "kairo", "explorer", "lockapp", "searchhost", "shellexperiencehost", "startmenuexperiencehost", "taskmgr",
    // Their Mac counterparts.
    "finder", "loginwindow", "activity monitor",
];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Rule {
    /// Blocked when its category is distracting.
    Auto,
    Block,
    Allow,
}

impl Rule {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Rule::Auto),
            "block" => Some(Rule::Block),
            "allow" => Some(Rule::Allow),
            _ => None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub enabled: bool,
    pub launchers: bool,
    /// Ending a session early or allowing something blocked takes a wait first.
    pub strict: bool,
}

/// On unless turned off; strict mode is the one setting that starts off.
fn flag(conn: &Connection, key: &str, default: bool) -> Result<bool> {
    Ok(match db::get_setting(conn, key)?.as_deref() {
        Some("1") => true,
        Some("0") => false,
        _ => default,
    })
}

pub fn settings(conn: &Connection) -> Result<Settings> {
    Ok(Settings {
        enabled: flag(conn, ENABLED_KEY, true)?,
        launchers: flag(conn, LAUNCHERS_KEY, true)?,
        strict: flag(conn, STRICT_KEY, false)?,
    })
}

/// `which` is "enabled" (blocking as a whole), "launchers" or "strict". False for anything else.
pub fn set_setting(conn: &Connection, which: &str, on: bool) -> Result<bool> {
    let key = match which {
        "enabled" => ENABLED_KEY,
        "launchers" => LAUNCHERS_KEY,
        "strict" => STRICT_KEY,
        _ => return Ok(false),
    };
    db::set_setting(conn, key, if on { "1" } else { "0" })?;
    Ok(true)
}

/// Strict mode only bites while a session is on, paused or not.
pub fn strict_now(conn: &Connection) -> Result<bool> {
    Ok(settings(conn)?.strict && focus::active(conn)?.is_some())
}

/// Strict mode's pending requests: action to when it was first asked for.
pub type Waits = Arc<Mutex<HashMap<String, i64>>>;

/// Whether an action may go ahead. In strict mode during a session, the first request starts a
/// wait and is turned down with the time it's ready; asking again once ready lets it through.
/// A request left unconfirmed for long lapses, so a later one starts a fresh wait.
pub fn strict_gate(conn: &Connection, waits: &Waits, action: &str, now: i64) -> Result<std::result::Result<(), i64>> {
    if !strict_now(conn)? {
        return Ok(Ok(()));
    }
    let mut waits = waits.lock().unwrap_or_else(|e| e.into_inner());
    match waits.get(action).copied() {
        Some(asked) if now < asked + STRICT_WAIT_MS => Ok(Err(asked + STRICT_WAIT_MS)),
        Some(asked) if now <= asked + STRICT_WAIT_MS + STRICT_CONFIRM_MS => {
            waits.remove(action);
            Ok(Ok(()))
        }
        _ => {
            waits.insert(action.to_string(), now);
            Ok(Err(now + STRICT_WAIT_MS))
        }
    }
}

pub fn rules(conn: &Connection) -> Result<HashMap<String, Rule>> {
    let mut stmt = conn.prepare("SELECT key, rule FROM focus_rules")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = HashMap::new();
    for row in rows {
        let (key, rule) = row?;
        if let Some(rule) = Rule::parse(&rule) {
            out.insert(key, rule);
        }
    }
    Ok(out)
}

/// `Auto` clears the override, so the category decides again.
pub fn set_rule(conn: &Connection, key: &str, rule: Rule) -> Result<()> {
    match rule {
        Rule::Auto => conn.execute("DELETE FROM focus_rules WHERE key = ?1", [key])?,
        Rule::Block | Rule::Allow => conn.execute(
            "INSERT INTO focus_rules (key, rule) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET rule = excluded.rule",
            params![key, if rule == Rule::Block { "block" } else { "allow" }],
        )?,
    };
    Ok(())
}

/// "Allow 5 minutes" exceptions: key to the time they run out.
pub type Allowances = Arc<Mutex<HashMap<String, i64>>>;

pub fn allow(allowances: &Allowances, key: &str, now: i64) {
    allowances.lock().unwrap_or_else(|e| e.into_inner()).insert(key.to_string(), now + ALLOW_MINUTES * MIN_MS);
}

fn allowed(allowances: &Allowances, key: &str, now: i64) -> bool {
    allowances.lock().unwrap_or_else(|e| e.into_inner()).get(key).is_some_and(|&until| now < until)
}

/// Whether something is blocked during a focus session, before any "Allow 5 minutes". A rule set
/// in Categories wins, a site's over its browser's; otherwise anything distracting is blocked.
pub fn decide(rules: &HashMap<String, Rule>, cats: &HashMap<String, Category>, exe: Option<&str>, site: Option<&str>) -> bool {
    if exe.is_some_and(|e| NEVER.contains(&e)) {
        return false;
    }
    let keys = [site.map(|s| format!("site:{s}")), exe.map(|e| format!("app:{e}"))];
    for key in keys.iter().flatten() {
        match rules.get(key) {
            Some(Rule::Block) => return true,
            Some(Rule::Allow) => return false,
            _ => {}
        }
    }
    analysis::resolve(cats, exe.unwrap_or(""), site) == Category::Distracting
}

/// For the browser extension: when a site is blocked right now, the time the block lifts.
pub fn site_blocked_until(conn: &Connection, allowances: &Allowances, site: &str, now: i64) -> Result<Option<i64>> {
    let Some(until) = focus::running_until(conn, now)? else { return Ok(None) };
    if !settings(conn)?.enabled || allowed(allowances, &format!("site:{site}"), now) {
        return Ok(None);
    }
    let blocked = decide(&rules(conn)?, &analysis::category_map(conn)?, None, Some(site));
    Ok(blocked.then_some(until))
}

/// What the blocked window shows.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Blocked {
    label: String,
    key: String,
    until_ms: i64,
}

fn show_blocked(app: &AppHandle, blocked: &Blocked, appearance_mode: &str) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string(blocked)?;
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.eval(format!("window.kairoBlocked && window.kairoBlocked({json})"))?;
        window.unminimize()?;
        window.show()?;
        window.set_focus()?;
        return Ok(());
    }
    // The page reads its first message before any of its own code runs.
    let window = WebviewWindowBuilder::new(app, WINDOW_LABEL, WebviewUrl::App("blocked.html".into()))
        .title("Blocked during focus")
        .inner_size(460.0, 320.0)
        .resizable(false)
        .maximizable(false)
        .always_on_top(true)
        .center()
        .initialization_script(format!("window.__KAIRO_BLOCKED__ = {json};"))
        .build()?;
    crate::appearance::apply_to(&window, appearance_mode);
    Ok(())
}

pub fn close_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        let _ = window.close();
    }
}

pub fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("blocking: cannot notify: {e}");
    }
}

#[derive(Default)]
struct Watch {
    ticks: u64,
    /// Launchers already announced this session, so closing one again stays quiet.
    closed: HashSet<&'static str>,
}

pub fn spawn(app: AppHandle, db_path: PathBuf, paused: Arc<AtomicBool>, allowances: Allowances) {
    thread::Builder::new()
        .name("kairo-blocking".into())
        .spawn(move || {
            let conn = match db::open(&db_path) {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("blocking: cannot open database: {e}");
                    return;
                }
            };
            let mut watch = Watch::default();
            loop {
                thread::sleep(TICK);
                if let Err(e) = tick(&app, &conn, &mut watch, paused.load(Ordering::Relaxed), &allowances) {
                    eprintln!("blocking: {e}");
                }
            }
        })
        .expect("failed to start blocking thread");
}

fn tick(app: &AppHandle, conn: &Connection, w: &mut Watch, paused: bool, allowances: &Allowances) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let now = chrono::Utc::now().timestamp_millis();
    w.ticks += 1;
    let until = if paused { None } else { focus::running_until(conn, now)? };
    let settings = settings(conn)?;
    let Some(until) = until.filter(|_| settings.enabled) else {
        w.closed.clear();
        close_window(app);
        return Ok(());
    };
    let rules = rules(conn)?;

    // The app in front. Browsers are the extension's job, since the site matters more than the app.
    if let Some(fg) = platform::foreground() {
        let exe = classify::exe_stem(&fg.exe_path);
        let key = format!("app:{exe}");
        if !classify::is_browser(&exe)
            && decide(&rules, &analysis::category_map(conn)?, Some(&exe), None)
            && !allowed(allowances, &key, now)
            // Windows won't let Kairo minimize programs running with more rights, like many games
            // with anti-cheat; the focus session's nudge still speaks up for those.
            && platform::minimize(fg.window)
        {
            let label = classify::app_name(&fg.exe_path, &exe, &classify::clean_title(&fg.title));
            show_blocked(app, &Blocked { label, key, until_ms: until }, &crate::appearance::get(conn)?)?;
        }
    }

    if settings.launchers && w.ticks.is_multiple_of(LAUNCHER_EVERY_TICKS) {
        for (pid, stem, name) in launchers_to_close(&platform::processes()) {
            let key = format!("app:{stem}");
            if rules.get(&key) == Some(&Rule::Allow) || allowed(allowances, &key, now) {
                continue;
            }
            if platform::close_process(pid) && w.closed.insert(name) {
                notify(app, &format!("Closed {name} for your focus session"), "Game launchers stay closed until the session ends or you pause it.");
            }
        }
    }
    Ok(())
}

/// The game launchers among `processes` (process id, program path) to close: (process id, exe
/// stem, name). None while a game is running, since closing its launcher can take the game down
/// with it (Overwatch goes with Battle.net); the game itself is blocked as a distracting app.
fn launchers_to_close(processes: &[(u32, String)]) -> Vec<(u32, &'static str, &'static str)> {
    let stems: Vec<(u32, String, &String)> = processes.iter().map(|(pid, path)| (*pid, classify::exe_stem(path), path)).collect();
    if stems.iter().any(|(_, exe, path)| classify::is_game(exe, path)) {
        return Vec::new();
    }
    stems
        .iter()
        .filter_map(|(pid, exe, _)| LAUNCHERS.iter().find(|(l, _)| l == exe).map(|&(stem, name)| (*pid, stem, name)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launchers_stay_open_while_a_game_runs() {
        let battle_net = (10, r"C:\Program Files (x86)\Battle.net\Battle.net.exe".to_string());
        let steam = (11, r"C:\Program Files (x86)\Steam\steam.exe".to_string());
        let notepad = (12, r"C:\Windows\notepad.exe".to_string());
        // No game running: launchers close, other programs don't.
        assert_eq!(launchers_to_close(&[battle_net.clone(), steam.clone(), notepad.clone()]), [(10, "battle.net", "Battle.net"), (11, "steam", "Steam")]);
        // Overwatch would go down with Battle.net, so nothing closes while it plays.
        let overwatch = (20, r"C:\Program Files (x86)\Overwatch\_retail_\Overwatch.exe".to_string());
        assert!(launchers_to_close(&[battle_net.clone(), steam.clone(), overwatch]).is_empty());
        // Nor while a game from a library folder runs.
        let finals = (21, r"D:\SteamLibrary\steamapps\common\THE FINALS\Discovery.exe".to_string());
        assert!(launchers_to_close(&[battle_net, steam, notepad, finals]).is_empty());
    }

    #[test]
    fn distracting_things_are_blocked_unless_a_rule_says_otherwise() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let cats = analysis::category_map(&conn).unwrap();
        let none = HashMap::new();
        // Discord and YouTube are distracting by default; Cursor and GitHub aren't.
        assert!(decide(&none, &cats, Some("discord"), None));
        assert!(decide(&none, &cats, None, Some("youtube")));
        assert!(!decide(&none, &cats, Some("cursor"), None));
        assert!(!decide(&none, &cats, None, Some("github")));
        // Kairo and Explorer are never blocked, even when told to.
        set_rule(&conn, "app:explorer", Rule::Block).unwrap();
        assert!(!decide(&rules(&conn).unwrap(), &cats, Some("explorer"), None));

        set_rule(&conn, "app:discord", Rule::Allow).unwrap();
        set_rule(&conn, "site:github", Rule::Block).unwrap();
        let r = rules(&conn).unwrap();
        assert!(!decide(&r, &cats, Some("discord"), None));
        assert!(decide(&r, &cats, None, Some("github")));
        set_rule(&conn, "app:discord", Rule::Auto).unwrap();
        assert!(decide(&rules(&conn).unwrap(), &cats, Some("discord"), None));
    }

    #[test]
    fn strict_mode_makes_ending_early_wait_then_confirm() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let waits = Waits::default();
        let t = 1_000 * MIN_MS;
        focus::start(&conn, 25, None, t).unwrap();
        // Off by default: everything goes ahead at once.
        assert_eq!(strict_gate(&conn, &waits, "stop", t).unwrap(), Ok(()));

        set_setting(&conn, "strict", true).unwrap();
        assert_eq!(strict_gate(&conn, &waits, "stop", t).unwrap(), Err(t + STRICT_WAIT_MS));
        // Asking again too soon keeps the same deadline rather than restarting it.
        assert_eq!(strict_gate(&conn, &waits, "stop", t + 10_000).unwrap(), Err(t + STRICT_WAIT_MS));
        assert_eq!(strict_gate(&conn, &waits, "stop", t + STRICT_WAIT_MS).unwrap(), Ok(()));
        // Each action waits on its own, and an unconfirmed wait lapses.
        assert!(strict_gate(&conn, &waits, "allow:app:discord", t).unwrap().is_err());
        let late = t + STRICT_WAIT_MS + STRICT_CONFIRM_MS + 1;
        assert_eq!(strict_gate(&conn, &waits, "allow:app:discord", late).unwrap(), Err(late + STRICT_WAIT_MS));
        // With no session on, strict mode has nothing to guard.
        focus::stop(&conn, t + 20 * MIN_MS).unwrap();
        assert_eq!(strict_gate(&conn, &waits, "stop", t + 21 * MIN_MS).unwrap(), Ok(()));
    }

    #[test]
    fn sites_are_blocked_only_during_a_running_session() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let allowances = Allowances::default();
        let t = 1_000 * MIN_MS;
        assert_eq!(site_blocked_until(&conn, &allowances, "youtube", t).unwrap(), None);
        focus::start(&conn, 25, None, t).unwrap();
        assert_eq!(site_blocked_until(&conn, &allowances, "youtube", t).unwrap(), Some(t + 25 * MIN_MS));
        assert_eq!(site_blocked_until(&conn, &allowances, "github", t).unwrap(), None);
        // "Allow 5 minutes" lets it through, then runs out.
        allow(&allowances, "site:youtube", t);
        assert_eq!(site_blocked_until(&conn, &allowances, "youtube", t + MIN_MS).unwrap(), None);
        assert!(site_blocked_until(&conn, &allowances, "youtube", t + 6 * MIN_MS).unwrap().is_some());
        // A pause is a break: nothing is blocked.
        focus::pause(&conn, t + 7 * MIN_MS).unwrap();
        assert_eq!(site_blocked_until(&conn, &allowances, "youtube", t + 8 * MIN_MS).unwrap(), None);
        focus::resume(&conn, t + 9 * MIN_MS).unwrap();
        set_setting(&conn, "enabled", false).unwrap();
        assert_eq!(site_blocked_until(&conn, &allowances, "youtube", t + 10 * MIN_MS).unwrap(), None);
    }
}
