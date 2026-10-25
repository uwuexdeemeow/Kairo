//! Background thread that samples the foreground window once a second and records sessions.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rusqlite::Connection;

use crate::browser::{self, SharedTab};
use crate::privacy::{self, Mode, Rules, SharedRules};
use crate::{classify, db, platform};

const TICK: Duration = Duration::from_secs(1);
/// How often the open session's end time is written, so a crash loses at most this much.
const FLUSH_MS: i64 = 10_000;
/// A tick gap this large means the machine slept; don't stretch a session across it.
const SLEEP_GAP_MS: i64 = 15_000;
const IDLE_AFTER_MS: i64 = 2 * 60_000;
/// Watching a video without touching the mouse is not being away.
const PASSIVE_IDLE_AFTER_MS: i64 = 30 * 60_000;
/// History older than the retention setting is cleared at launch and then hourly.
const PRUNE_EVERY_MS: i64 = 60 * 60_000;

#[derive(PartialEq)]
struct Sample {
    app: String,
    exe: String,
    title: String,
    site: Option<String>,
    idle: bool,
    game: bool,
}

impl Sample {
    fn idle() -> Self {
        Sample { app: "Idle".into(), exe: String::new(), title: String::new(), site: None, idle: true, game: false }
    }
}

struct Open {
    id: i64,
    sample: Sample,
    start_ms: i64,
    flushed_ms: i64,
}

pub fn spawn(db_path: PathBuf, paused: Arc<AtomicBool>, rules: SharedRules, tab: SharedTab) {
    thread::Builder::new()
        .name("kairo-collector".into())
        .spawn(move || {
            let conn = match db::open(&db_path) {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("collector: cannot open database: {e}");
                    return;
                }
            };
            run(&conn, &paused, &rules, &tab);
        })
        .expect("failed to start collector thread");
}

fn run(conn: &Connection, paused: &AtomicBool, rules: &SharedRules, tab: &SharedTab) {
    let mut open: Option<Open> = None;
    let mut last_tick = now_ms();
    let mut adopted_games: HashSet<String> = HashSet::new();
    let mut pruned_ms = i64::MIN;

    loop {
        thread::sleep(TICK);
        let now = now_ms();
        if now.saturating_sub(pruned_ms) >= PRUNE_EVERY_MS {
            log(privacy::prune(conn, now));
            pruned_ms = now;
        }

        if now - last_tick > SLEEP_GAP_MS || paused.load(Ordering::Relaxed) {
            close(conn, open.take(), last_tick.min(now));
        }
        last_tick = now;
        if paused.load(Ordering::Relaxed) {
            continue;
        }

        let captured = capture(now, &rules.read().unwrap_or_else(|e| e.into_inner()), browser::fresh(tab, now));
        let Some((sample, since)) = captured else {
            close(conn, open.take(), now);
            continue;
        };
        match open.as_mut() {
            Some(cur) if cur.sample == sample => {
                if now - cur.flushed_ms >= FLUSH_MS {
                    match db::set_session_end(conn, cur.id, now) {
                        // Deleted from the UI; the next tick starts a fresh session.
                        Ok(false) => open = None,
                        result => log(result.map(|_| cur.flushed_ms = now)),
                    }
                }
            }
            _ => {
                // When idle is detected, it actually began at the last input, so end the previous session there.
                close(conn, open.take(), since);
                if sample.game && adopted_games.insert(sample.exe.clone()) {
                    log(db::adopt_game(conn, &sample.exe, &sample.app));
                }
                let start = since.min(now);
                let row = db::NewSession {
                    app: &sample.app,
                    exe: &sample.exe,
                    title: &sample.title,
                    site: sample.site.as_deref(),
                    start_ms: start,
                    end_ms: now,
                    idle: sample.idle,
                    demo: false,
                };
                match db::insert_session(conn, &row) {
                    Ok(id) => open = Some(Open { id, sample, start_ms: start, flushed_ms: now }),
                    Err(e) => eprintln!("collector: insert failed: {e}"),
                }
            }
        }
    }
}

/// What the user is doing right now, and when that started (earlier than now only for idle).
/// None while something the user excluded from recording is in front.
fn capture(now: i64, rules: &Rules, tab: Option<browser::Report>) -> Option<(Sample, i64)> {
    let Some(fg) = platform::foreground() else {
        return Some((Sample::idle(), now));
    };
    let exe = classify::exe_stem(&fg.exe_path);
    if exe == "lockapp" {
        return Some((Sample::idle(), now));
    }

    let mut title = classify::clean_title(&fg.title);
    let browser = classify::is_browser(&exe);
    // With the extension reporting, the browser in front says exactly which site it shows;
    // without it, the site is guessed from the window title.
    let tab = tab.filter(|_| browser);
    let private = browser && (tab.as_ref().is_some_and(|t| t.incognito) || classify::is_private_window(&title));
    let site = match tab {
        _ if !browser || private => None,
        Some(t) => t.site,
        None => classify::site_from_title(&title),
    };
    let mode = privacy::resolve(rules, &exe, site.as_deref());
    if mode == Mode::Ignore {
        return None;
    }
    let game = classify::is_game(&exe, &fg.exe_path);
    let idle_for = platform::idle_ms();
    // Controller input never reaches the idle timer, so games get the long threshold too.
    let threshold = if game || classify::is_passive(&exe, site.as_deref()) { PASSIVE_IDLE_AFTER_MS } else { IDLE_AFTER_MS };
    if idle_for >= threshold {
        return Some((Sample::idle(), now - idle_for));
    }

    // Named before the title goes, since some app names come from it.
    let app = classify::game_name(&fg.exe_path).unwrap_or_else(|| classify::display_name(&exe, &title));
    if private || mode == Mode::HideTitle {
        title.clear();
    }
    Some((Sample { app, exe, title, site, idle: false, game }, now))
}

fn close(conn: &Connection, open: Option<Open>, end: i64) {
    if let Some(cur) = open {
        // Idle detection can point before this session began (e.g. a video autoplayed into a new title).
        log(db::set_session_end(conn, cur.id, end.max(cur.start_ms)));
    }
}

fn log<T>(result: rusqlite::Result<T>) {
    if let Err(e) = result {
        eprintln!("collector: write failed: {e}");
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
