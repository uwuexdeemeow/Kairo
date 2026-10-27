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

use chrono::{Local, TimeZone, Timelike};

use crate::analysis::{Insight, InsightKind};
use crate::goals::fmt_dur;
use crate::{classify, db, nudge, platform, schedule};

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
/// A scheduled session waits for someone at the computer: input within this long.
const SCHEDULE_NEEDS_ACTIVE_MS: i64 = 2 * MIN_MS;
/// A tick gap this large means the machine slept, which is a break too.
const SLEEP_GAP_MS: i64 = 30_000;

/// Sessions run 1 to 240 minutes, and labels are short notes, not documents.
pub const MAX_MINUTES: u32 = 240;
pub const MAX_LABEL_CHARS: usize = 80;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Active {
    start_ms: i64,
    planned_ms: i64,
    /// Time spent paused so far, not counting a pause still in progress.
    paused_ms: i64,
    /// Set while paused.
    paused_at: Option<i64>,
    label: Option<String>,
}

impl Active {
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// When the session will end, given that a pause in progress pushes the end back.
    fn ends_ms(&self, now: i64) -> i64 {
        self.start_ms + self.planned_ms + self.paused_ms + self.paused_at.map_or(0, |p| now - p)
    }
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    start: i64,
    end: i64,
    planned_ms: i64,
    paused_ms: i64,
    completed: bool,
    label: Option<String>,
}

impl Block {
    /// Time actually spent focused: the session less its pauses.
    pub fn ms(&self) -> i64 {
        (self.end - self.start - self.paused_ms).max(0)
    }
}

#[derive(Serialize, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    sessions: usize,
    completed: usize,
    focused_ms: i64,
}

impl Stats {
    fn of(blocks: &[Block]) -> Stats {
        Stats {
            sessions: blocks.len(),
            completed: blocks.iter().filter(|b| b.completed).count(),
            focused_ms: blocks.iter().map(Block::ms).sum(),
        }
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

const BLOCK_COLUMNS: &str = "start_ms, COALESCE(end_ms, ?1), planned_ms, paused_ms + COALESCE(?1 - paused_at, 0), completed, label";

fn block(r: &rusqlite::Row) -> Result<Block> {
    Ok(Block { start: r.get(0)?, end: r.get(1)?, planned_ms: r.get(2)?, paused_ms: r.get(3)?, completed: r.get(4)?, label: r.get(5)? })
}

/// The running session, if any; an unfinished one survives a restart.
pub fn active(conn: &Connection) -> Result<Option<Active>> {
    conn.query_row(
        "SELECT start_ms, planned_ms, paused_ms, paused_at, label FROM focus_sessions WHERE end_ms IS NULL ORDER BY id DESC LIMIT 1",
        [],
        |r| Ok(Active { start_ms: r.get(0)?, planned_ms: r.get(1)?, paused_ms: r.get(2)?, paused_at: r.get(3)?, label: r.get(4)? }),
    )
    .optional()
}

/// When the running session ends; None with no session, or while it's paused (a pause is a
/// break, so nothing is blocked).
pub fn running_until(conn: &Connection, now: i64) -> Result<Option<i64>> {
    Ok(active(conn)?.filter(|a| a.paused_at.is_none()).map(|a| a.ends_ms(now)))
}

/// Starts a session now, ending any that is running.
pub fn start(conn: &Connection, minutes: u32, label: Option<&str>, now: i64) -> Result<Active> {
    stop(conn, now)?;
    let label = label.map(str::trim).filter(|l| !l.is_empty()).map(|l| l.chars().take(MAX_LABEL_CHARS).collect::<String>());
    let a = Active { start_ms: now, planned_ms: minutes.min(MAX_MINUTES) as i64 * MIN_MS, paused_ms: 0, paused_at: None, label };
    conn.execute(
        "INSERT INTO focus_sessions (start_ms, planned_ms, label) VALUES (?1, ?2, ?3)",
        params![a.start_ms, a.planned_ms, a.label],
    )?;
    Ok(a)
}

/// Ends the running session early, folding a pause in progress into its paused time.
pub fn stop(conn: &Connection, now: i64) -> Result<()> {
    conn.execute(
        "UPDATE focus_sessions SET paused_ms = paused_ms + COALESCE(?1 - paused_at, 0), paused_at = NULL,
             end_ms = MAX(?1, start_ms), completed = 0
         WHERE end_ms IS NULL",
        [now],
    )?;
    Ok(())
}

pub fn pause(conn: &Connection, now: i64) -> Result<()> {
    conn.execute("UPDATE focus_sessions SET paused_at = ?1 WHERE end_ms IS NULL AND paused_at IS NULL", [now])?;
    Ok(())
}

pub fn resume(conn: &Connection, now: i64) -> Result<()> {
    conn.execute(
        "UPDATE focus_sessions SET paused_ms = paused_ms + (?1 - paused_at), paused_at = NULL
         WHERE end_ms IS NULL AND paused_at IS NOT NULL",
        [now],
    )?;
    Ok(())
}

/// Adds time to the running session, up to the usual maximum length.
pub fn extend(conn: &Connection, minutes: u32) -> Result<()> {
    conn.execute(
        "UPDATE focus_sessions SET planned_ms = MIN(planned_ms + ?1, ?2) WHERE end_ms IS NULL",
        params![minutes as i64 * MIN_MS, MAX_MINUTES as i64 * MIN_MS],
    )?;
    Ok(())
}

/// Closes a session whose time is up, returning it when that just happened. A paused session
/// never runs out.
fn complete_if_due(conn: &Connection, now: i64) -> Result<Option<Active>> {
    let Some(a) = active(conn)? else { return Ok(None) };
    if a.paused_at.is_some() || now < a.ends_ms(now) {
        return Ok(None);
    }
    conn.execute(
        "UPDATE focus_sessions SET end_ms = start_ms + planned_ms + paused_ms, completed = 1 WHERE end_ms IS NULL",
        [],
    )?;
    Ok(Some(a))
}

/// Sessions overlapping [from, to); a running one reaches to `now`.
pub fn blocks(conn: &Connection, from: i64, to: i64, now: i64) -> Result<Vec<Block>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {BLOCK_COLUMNS} FROM focus_sessions WHERE start_ms < ?3 AND COALESCE(end_ms, ?1) > ?2 ORDER BY start_ms"
    ))?;
    let rows = stmt.query_map(params![now, from, to], block)?;
    rows.collect()
}

/// The most recent sessions, newest first.
pub fn recent(conn: &Connection, limit: usize, now: i64) -> Result<Vec<Block>> {
    let mut stmt = conn.prepare(&format!("SELECT {BLOCK_COLUMNS} FROM focus_sessions ORDER BY start_ms DESC LIMIT ?2"))?;
    let rows = stmt.query_map(params![now, limit as i64], block)?;
    rows.collect()
}

pub fn stats(conn: &Connection, from: i64, to: i64, now: i64) -> Result<Stats> {
    Ok(Stats::of(&blocks(conn, from, to, now)?))
}

// ---------- insights ----------

/// Below these, a finish rate says more about chance than habit.
const MIN_SESSIONS_FOR_INSIGHTS: usize = 6;
const MIN_PER_GROUP: usize = 3;
/// How far apart two finish rates have to be before the difference is worth a sentence.
const NOTABLE_GAP: f64 = 0.2;

/// (key, phrase) for each part of the day a session can start in.
fn part_of_day(ms: i64) -> (&'static str, &'static str) {
    let hour = chrono::DateTime::from_timestamp_millis(ms).unwrap_or_default().with_timezone(&chrono::Local).hour();
    match hour {
        5..=11 => ("morning", "in the morning"),
        12..=16 => ("afternoon", "in the afternoon"),
        17..=21 => ("evening", "in the evening"),
        _ => ("night", "late at night"),
    }
}

/// (order, phrase, short label) for a session's planned length.
fn length_bucket(planned_ms: i64) -> (u8, &'static str, &'static str) {
    match planned_ms / MIN_MS {
        ..=30 => (0, "sessions of 30 minutes or less", "of short sessions"),
        31..=60 => (1, "sessions of 30 to 60 minutes", "of 30–60 min sessions"),
        _ => (2, "sessions over an hour", "of long sessions"),
    }
}

/// The groups with the highest and lowest finish rates, given enough sessions in each.
fn extremes<K: Copy + PartialEq>(blocks: &[&Block], key: impl Fn(&Block) -> K) -> Option<((K, f64), (K, f64))> {
    let mut groups: Vec<(K, usize, usize)> = Vec::new();
    for b in blocks {
        let k = key(b);
        match groups.iter_mut().find(|g| g.0 == k) {
            Some(g) => {
                g.1 += 1;
                g.2 += usize::from(b.completed);
            }
            None => groups.push((k, 1, usize::from(b.completed))),
        }
    }
    let rated: Vec<(K, f64)> =
        groups.into_iter().filter(|g| g.1 >= MIN_PER_GROUP).map(|(k, n, done)| (k, done as f64 / n as f64)).collect();
    let best = rated.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1))?;
    let worst = rated.iter().copied().min_by(|a, b| a.1.total_cmp(&b.1))?;
    (best.1 - worst.1 >= NOTABLE_GAP).then_some((best, worst))
}

fn pct(share: f64) -> u32 {
    (share * 100.0).round() as u32
}

/// What focus sessions in a range say: when they stick, and what length suits.
pub fn insights(blocks: &[Block], now: i64) -> Vec<Insight> {
    // A session still running hasn't had its chance to finish.
    let finished: Vec<&Block> = blocks.iter().filter(|b| b.completed || b.end < now).collect();
    if finished.len() < MIN_SESSIONS_FOR_INSIGHTS {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Some(((best, b), (worst, w))) = extremes(&finished, |b| part_of_day(b.start)) {
        out.push(
            Insight::new(
                InsightKind::Positive,
                format!("Your focus sessions stick best {}", best.1),
                format!("You finish {}% of the sessions you start {}, against {}% {}.", pct(b), best.1, pct(w), worst.1),
            )
            .with_metric(format!("{}%", pct(b)), format!("finished {}", best.1)),
        );
    }
    if let Some(((best, b), (worst, w))) = extremes(&finished, |b| length_bucket(b.planned_ms)) {
        let title = if best.0 < worst.0 { "Shorter focus sessions are the ones you finish" } else { "Longer focus sessions are the ones you finish" };
        out.push(
            Insight::new(InsightKind::Info, title, format!("You finish {}% of {}, but only {}% of {}.", pct(b), best.1, pct(w), worst.1))
                .with_metric(format!("{}%", pct(b)), best.2),
        );
    }
    out
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
    let mut session = active(conn)?;

    // A scheduled slot starts once the user is at the computer and nothing else is running.
    if session.is_none() && !paused && platform::idle_ms() < SCHEDULE_NEEDS_ACTIVE_MS {
        if let Some((slot, day, left)) = schedule::due(conn, Local::now())? {
            let started = start(conn, left, Some(slot.label.as_deref().unwrap_or("Scheduled focus")), now)?;
            schedule::mark_started(conn, slot.id, day)?;
            let until = Local.timestamp_millis_opt(started.ends_ms(now)).single().map(|t| t.format("%H:%M").to_string()).unwrap_or_default();
            notify(app, &format!("Scheduled focus until {until}"), "Distracting apps and sites are blocked until then, as in any focus session.");
            session = Some(started);
        }
    }

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

    // Focus sessions: speak up when a distraction holds the screen, unless the session is paused.
    let Some(session) = session.filter(|s| s.paused_at.is_none()) else {
        w.distracted_since = None;
        return Ok(());
    };
    match nudge::current_distraction(conn) {
        Some(label) => {
            let since = *w.distracted_since.get_or_insert(now);
            if now - since >= DISTRACTION_GRACE_MS && now - w.last_distraction_nudge >= DISTRACTION_REPEAT_MS {
                let left = fmt_dur((session.ends_ms(now) - now).max(MIN_MS));
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

    const M: i64 = MIN_MS;

    #[test]
    fn sessions_start_stop_complete_and_survive_as_blocks() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let t = 1_000 * M;
        start(&conn, 25, None, t).unwrap();
        // Starting another ends the first early.
        let a = start(&conn, 50, Some("  Write the report  "), t + 10 * M).unwrap();
        assert_eq!(active(&conn).unwrap(), Some(a.clone()));
        assert!(complete_if_due(&conn, t + 30 * M).unwrap().is_none());
        assert_eq!(complete_if_due(&conn, t + 61 * M).unwrap(), Some(a));
        assert!(active(&conn).unwrap().is_none());
        let label = Some("Write the report".to_string());
        assert_eq!(
            blocks(&conn, t, t + 120 * M, t + 120 * M).unwrap(),
            vec![
                Block { start: t, end: t + 10 * M, planned_ms: 25 * M, paused_ms: 0, completed: false, label: None },
                Block { start: t + 10 * M, end: t + 60 * M, planned_ms: 50 * M, paused_ms: 0, completed: true, label },
            ]
        );
        assert_eq!(break_minutes(&conn).unwrap(), DEFAULT_BREAK_MIN);
        set_break_minutes(&conn, 0).unwrap();
        assert_eq!(break_minutes(&conn).unwrap(), 0);
    }

    #[test]
    fn pauses_push_the_end_back_and_dont_count_as_focus() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let t = 1_000 * M;
        start(&conn, 25, None, t).unwrap();
        pause(&conn, t + 10 * M).unwrap();
        // Paused sessions never run out, however long the pause.
        assert!(complete_if_due(&conn, t + 60 * M).unwrap().is_none());
        resume(&conn, t + 20 * M).unwrap();
        extend(&conn, 5).unwrap();
        // 25 + 5 minutes of focus plus a 10-minute pause ends at 40.
        assert!(complete_if_due(&conn, t + 39 * M).unwrap().is_none());
        assert!(complete_if_due(&conn, t + 40 * M).unwrap().is_some());
        let s = stats(&conn, t, t + 120 * M, t + 120 * M).unwrap();
        assert_eq!(s, Stats { sessions: 1, completed: 1, focused_ms: 30 * M });

        // Ending early mid-pause still leaves the pause out of the focused time.
        start(&conn, 25, None, t + 60 * M).unwrap();
        pause(&conn, t + 65 * M).unwrap();
        stop(&conn, t + 75 * M).unwrap();
        assert_eq!(recent(&conn, 1, t + 120 * M).unwrap()[0].ms(), 5 * M);
        extend(&conn, 500).unwrap();
        start(&conn, 239, None, t + 80 * M).unwrap();
        extend(&conn, 30).unwrap();
        assert_eq!(active(&conn).unwrap().unwrap().planned_ms, MAX_MINUTES as i64 * M);
    }

    #[test]
    fn insights_compare_finish_rates_by_time_of_day_and_length() {
        use chrono::TimeZone;
        let at = |hour| chrono::Local.with_ymd_and_hms(2026, 10, 20, hour, 0, 0).unwrap().timestamp_millis();
        let block = |hour, minutes: i64, completed| Block {
            start: at(hour),
            end: at(hour) + minutes * M,
            planned_ms: minutes * M,
            paused_ms: 0,
            completed,
            label: None,
        };
        // Mornings: 25-minute sessions, all finished. Evenings: 90-minute ones, mostly not.
        let blocks = vec![
            block(9, 25, true),
            block(10, 25, true),
            block(11, 25, true),
            block(18, 90, false),
            block(19, 90, true),
            block(20, 90, false),
        ];
        let now = at(23);
        let found = serde_json::to_value(insights(&blocks, now)).unwrap();
        assert_eq!(found[0]["title"], "Your focus sessions stick best in the morning");
        assert_eq!(found[0]["detail"], "You finish 100% of the sessions you start in the morning, against 33% in the evening.");
        assert_eq!(found[1]["title"], "Shorter focus sessions are the ones you finish");
        // Too few sessions, or no real difference, and there's nothing to say.
        assert!(insights(&blocks[..5], now).is_empty());
        let even: Vec<Block> = (9..15).map(|h| block(h, 25, true)).collect();
        assert!(insights(&even, now).is_empty());
    }

    #[test]
    fn an_older_database_gains_the_new_columns() {
        let path = std::env::temp_dir().join(format!("kairo-focus-{}.db", std::process::id()));
        {
            let old = Connection::open(&path).unwrap();
            old.execute_batch(
                "CREATE TABLE focus_sessions (id INTEGER PRIMARY KEY, start_ms INTEGER NOT NULL,
                     planned_ms INTEGER NOT NULL, end_ms INTEGER, completed INTEGER NOT NULL DEFAULT 0);
                 INSERT INTO focus_sessions (start_ms, planned_ms, end_ms, completed) VALUES (0, 1500000, 1500000, 1);",
            )
            .unwrap();
        }
        let conn = db::open(&path).unwrap();
        let old = &recent(&conn, 5, 0).unwrap()[0];
        assert_eq!((old.ms(), old.label.as_deref()), (25 * M, None));
        drop(conn);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }
}
