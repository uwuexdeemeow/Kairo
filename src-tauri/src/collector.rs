//! Background thread that samples the foreground window once a second and records sessions.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rusqlite::Connection;

use crate::{classify, db, platform};

const TICK: Duration = Duration::from_secs(1);
/// How often the open session's end time is written, so a crash loses at most this much.
const FLUSH_MS: i64 = 10_000;
/// A tick gap this large means the machine slept; don't stretch a session across it.
const SLEEP_GAP_MS: i64 = 15_000;
const IDLE_AFTER_MS: i64 = 2 * 60_000;
/// Watching a video without touching the mouse is not being away.
const PASSIVE_IDLE_AFTER_MS: i64 = 30 * 60_000;

#[derive(PartialEq)]
struct Sample {
    app: String,
    exe: String,
    title: String,
    site: Option<String>,
    idle: bool,
}

impl Sample {
    fn idle() -> Self {
        Sample { app: "Idle".into(), exe: String::new(), title: String::new(), site: None, idle: true }
    }
}

struct Open {
    id: i64,
    sample: Sample,
    start_ms: i64,
    flushed_ms: i64,
}

pub fn spawn(db_path: PathBuf, paused: Arc<AtomicBool>) {
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
            run(&conn, &paused);
        })
        .expect("failed to start collector thread");
}

fn run(conn: &Connection, paused: &AtomicBool) {
    let mut open: Option<Open> = None;
    let mut last_tick = now_ms();

    loop {
        thread::sleep(TICK);
        let now = now_ms();

        if now - last_tick > SLEEP_GAP_MS || paused.load(Ordering::Relaxed) {
            close(conn, open.take(), last_tick.min(now));
        }
        last_tick = now;
        if paused.load(Ordering::Relaxed) {
            continue;
        }

        let (sample, since) = capture(now);
        match open.as_mut() {
            Some(cur) if cur.sample == sample => {
                if now - cur.flushed_ms >= FLUSH_MS {
                    log(db::set_session_end(conn, cur.id, now));
                    cur.flushed_ms = now;
                }
            }
            _ => {
                // When idle is detected, it actually began at the last input, so end the previous session there.
                close(conn, open.take(), since);
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
fn capture(now: i64) -> (Sample, i64) {
    let Some(fg) = platform::foreground() else {
        return (Sample::idle(), now);
    };
    let exe = classify::exe_stem(&fg.exe_path);
    if exe == "lockapp" {
        return (Sample::idle(), now);
    }

    let site = if classify::is_browser(&exe) { classify::site_from_title(&fg.title) } else { None };
    let idle_for = platform::idle_ms();
    let threshold = if classify::is_passive(&exe, site.as_deref()) { PASSIVE_IDLE_AFTER_MS } else { IDLE_AFTER_MS };
    if idle_for >= threshold {
        return (Sample::idle(), now - idle_for);
    }

    let app = classify::display_name(&exe, &fg.title);
    (Sample { app, exe, title: fg.title, site, idle: false }, now)
}

fn close(conn: &Connection, open: Option<Open>, end: i64) {
    if let Some(cur) = open {
        // Idle detection can point before this session began (e.g. a video autoplayed into a new title).
        log(db::set_session_end(conn, cur.id, end.max(cur.start_ms)));
    }
}

fn log(result: rusqlite::Result<()>) {
    if let Err(e) = result {
        eprintln!("collector: write failed: {e}");
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
