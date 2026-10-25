//! Focus sessions (a timed stretch of work with a nudge when a distraction takes over) and break
//! reminders after a long unbroken stretch at the screen.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::goals::fmt_dur;
use crate::{classify, db, nudge, platform};

const TICK: Duration = Duration::from_secs(5);
const MIN_MS: i64 = 60_000;
/// A distraction has to hold the screen this long before a focus session speaks up...
const DISTRACTION_GRACE_MS: i64 = 30_000;
/// ...and then it waits at least this long before speaking up again.
const DISTRACTION_REPEAT_MS: i64 = 5 * MIN_MS;
const BREAK_KEY: &str = "break_every_min";
const DEFAULT_BREAK_MIN: u32 = 90;
/// This long away from the keyboard and mouse counts as a break.
const BREAK_RESETS_AFTER_MS: i64 = 5 * MIN_MS;
/// A tick gap this large means the machine slept, which is a break too.
const SLEEP_GAP_MS: i64 = 30_000;

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Active {
    start_ms: i64,
    planned_ms: i64,
}

impl Active {
    fn ends_ms(&self) -> i64 {
        self.start_ms + self.planned_ms
    }
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    start: i64,
    end: i64,
    completed: bool,
}

impl Block {
    pub fn ms(&self) -> i64 {
        self.end - self.start
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// The running session, if any; an unfinished one survives a restart.
pub fn active(conn: &Connection) -> Result<Option<Active>> {
    conn.query_row("SELECT start_ms, planned_ms FROM focus_sessions WHERE end_ms IS NULL ORDER BY id DESC LIMIT 1", [], |r| {
        Ok(Active { start_ms: r.get(0)?, planned_ms: r.get(1)? })
    })
    .optional()
}

/// Starts a session now, ending any that is running.
pub fn start(conn: &Connection, minutes: u32, now: i64) -> Result<Active> {
    stop(conn, now)?;
    let a = Active { start_ms: now, planned_ms: minutes as i64 * MIN_MS };
    conn.execute("INSERT INTO focus_sessions (start_ms, planned_ms) VALUES (?1, ?2)", params![a.start_ms, a.planned_ms])?;
    Ok(a)
}

/// Ends the running session early.
pub fn stop(conn: &Connection, now: i64) -> Result<()> {
    conn.execute("UPDATE focus_sessions SET end_ms = MAX(?1, start_ms), completed = 0 WHERE end_ms IS NULL", [now])?;
    Ok(())
}

/// Closes a session whose time is up, returning it when that just happened.
fn complete_if_due(conn: &Connection, now: i64) -> Result<Option<Active>> {
    let Some(a) = active(conn)? else { return Ok(None) };
    if now < a.ends_ms() {
        return Ok(None);
    }
    conn.execute("UPDATE focus_sessions SET end_ms = start_ms + planned_ms, completed = 1 WHERE end_ms IS NULL", [])?;
    Ok(Some(a))
}

/// Sessions overlapping [from, to); a running one reaches to `now`.
pub fn blocks(conn: &Connection, from: i64, to: i64, now: i64) -> Result<Vec<Block>> {
    let mut stmt = conn.prepare(
        "SELECT start_ms, COALESCE(end_ms, ?3), completed FROM focus_sessions
         WHERE start_ms < ?2 AND COALESCE(end_ms, ?3) > ?1 ORDER BY start_ms",
    )?;
    let rows = stmt.query_map(params![from, to, now], |r| Ok(Block { start: r.get(0)?, end: r.get(1)?, completed: r.get(2)? }))?;
    rows.collect()
}

/// Minutes of unbroken screen time before a break reminder; 0 turns them off.
pub fn break_minutes(conn: &Connection) -> Result<u32> {
    Ok(db::get_setting(conn, BREAK_KEY)?.and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_BREAK_MIN))
}

pub fn set_break_minutes(conn: &Connection, minutes: u32) -> Result<()> {
    db::set_setting(conn, BREAK_KEY, &minutes.to_string())
}

/// What the background tick remembers between runs.
#[derive(Default)]
struct Watch {
    last_tick: i64,
    /// When the current stretch at the screen began; None while on a break.
    stretch_since: Option<i64>,
    /// Break reminders already sent this stretch, so the next one waits another interval.
    breaks_sent: i64,
    distracted_since: Option<i64>,
    last_distraction_nudge: i64,
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("focus: cannot notify: {e}");
    }
}

pub fn spawn(app: AppHandle, db_path: PathBuf, paused: Arc<AtomicBool>) {
    thread::Builder::new()
        .name("kairo-focus".into())
        .spawn(move || {
            let conn = match db::open(&db_path) {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("focus: cannot open database: {e}");
                    return;
                }
            };
            let mut watch = Watch { last_tick: now_ms(), ..Watch::default() };
            loop {
                thread::sleep(TICK);
                if let Err(e) = tick(&app, &conn, &mut watch, paused.load(Ordering::Relaxed)) {
                    eprintln!("focus: {e}");
                }
            }
        })
        .expect("failed to start focus thread");
}

fn tick(app: &AppHandle, conn: &Connection, w: &mut Watch, paused: bool) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let now = now_ms();
    let slept = now - w.last_tick > SLEEP_GAP_MS;
    w.last_tick = now;

    if let Some(done) = complete_if_due(conn, now)? {
        notify(app, &format!("Focus session done: {}", fmt_dur(done.planned_ms)), "Nice work. A good moment to stand up for a few minutes.");
        // The session's end is a natural break point.
        w.stretch_since = Some(now);
        w.breaks_sent = 0;
    }
    let session = active(conn)?;

    // Breaks: away long enough, asleep or paused all reset the stretch.
    if slept || paused || platform::idle_ms() >= BREAK_RESETS_AFTER_MS {
        w.stretch_since = None;
        w.breaks_sent = 0;
        w.distracted_since = None;
        return Ok(());
    }
    let since = *w.stretch_since.get_or_insert(now);

    let fg = platform::foreground();
    let gaming = fg.as_ref().is_some_and(|f| classify::is_game(&classify::exe_stem(&f.exe_path), &f.exe_path));
    let every = break_minutes(conn)? as i64 * MIN_MS;
    // Held back during focus sessions and games; it goes out once they're over.
    if every > 0 && session.is_none() && !gaming && now - since >= every * (w.breaks_sent + 1) {
        notify(app, &format!("You've been at the screen for {}", fmt_dur(now - since)), "Take five: look away, stretch, get some water.");
        w.breaks_sent += 1;
    }

    // Focus sessions: speak up when a distraction holds the screen.
    let Some(session) = session else {
        w.distracted_since = None;
        return Ok(());
    };
    match nudge::current_distraction(conn) {
        Some(label) => {
            let since = *w.distracted_since.get_or_insert(now);
            if now - since >= DISTRACTION_GRACE_MS && now - w.last_distraction_nudge >= DISTRACTION_REPEAT_MS {
                let left = fmt_dur((session.ends_ms() - now).max(MIN_MS));
                notify(app, &format!("Focus session: {left} left"), &format!("{label} can wait until it's done."));
                w.last_distraction_nudge = now;
            }
        }
        None => w.distracted_since = None,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_start_stop_complete_and_survive_as_blocks() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let t = 1_000 * MIN_MS;
        start(&conn, 25, t).unwrap();
        // Starting another ends the first early.
        let a = start(&conn, 50, t + 10 * MIN_MS).unwrap();
        assert_eq!(active(&conn).unwrap(), Some(a));
        assert!(complete_if_due(&conn, t + 30 * MIN_MS).unwrap().is_none());
        assert_eq!(complete_if_due(&conn, t + 61 * MIN_MS).unwrap(), Some(a));
        assert!(active(&conn).unwrap().is_none());
        assert_eq!(
            blocks(&conn, t, t + 120 * MIN_MS, t + 120 * MIN_MS).unwrap(),
            vec![
                Block { start: t, end: t + 10 * MIN_MS, completed: false },
                Block { start: t + 10 * MIN_MS, end: t + 60 * MIN_MS, completed: true },
            ]
        );
        assert_eq!(break_minutes(&conn).unwrap(), DEFAULT_BREAK_MIN);
        set_break_minutes(&conn, 0).unwrap();
        assert_eq!(break_minutes(&conn).unwrap(), 0);
    }
}
