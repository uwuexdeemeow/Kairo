//! Focus sessions (a timed stretch of work with a nudge when a distraction takes over) and break
//! reminders after a long unbroken stretch at the screen.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

use chrono::{Local, TimeZone, Timelike};

use crate::analysis::{Insight, InsightKind};
use crate::goals::fmt_dur;
use crate::{blocking, classify, db, nudge, platform, schedule};

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
    /// The task it was started on, by id (tasks.rs).
    task: Option<String>,
}

impl Active {
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn is_paused(&self) -> bool {
        self.paused_at.is_some()
    }

    /// How long it has left to run at `now`; a pause in progress doesn't use it up.
    pub fn left_ms(&self, now: i64) -> i64 {
        (self.ends_ms(now) - now).max(0)
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
    /// How often it spoke up about a distraction; None for a session from before that was counted.
    nudges: Option<i64>,
    /// The pomodoro cycle it was a round of (its id), if it was.
    cycle: Option<i64>,
}

impl Block {
    pub fn cycle(&self) -> Option<i64> {
        self.cycle
    }

    pub fn start(&self) -> i64 {
        self.start
    }

    pub fn end(&self) -> i64 {
        self.end
    }

    pub fn planned_ms(&self) -> i64 {
        self.planned_ms
    }

    pub fn completed(&self) -> bool {
        self.completed
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn nudges(&self) -> Option<i64> {
        self.nudges
    }

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
    #[cfg(test)]
    pub(crate) fn of_ms(sessions: usize, completed: usize, focused_ms: i64) -> Stats {
        Stats { sessions, completed, focused_ms }
    }

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

const BLOCK_COLUMNS: &str = "start_ms, COALESCE(end_ms, ?1), planned_ms, paused_ms + COALESCE(?1 - paused_at, 0), completed, label, nudges, cycle_id";

fn block(r: &rusqlite::Row) -> Result<Block> {
    Ok(Block { start: r.get(0)?, end: r.get(1)?, planned_ms: r.get(2)?, paused_ms: r.get(3)?, completed: r.get(4)?, label: r.get(5)?, nudges: r.get(6)?, cycle: r.get(7)? })
}

/// The running session, if any; an unfinished one survives a restart.
pub fn active(conn: &Connection) -> Result<Option<Active>> {
    conn.query_row(
        "SELECT start_ms, planned_ms, paused_ms, paused_at, label, task FROM focus_sessions WHERE end_ms IS NULL ORDER BY id DESC LIMIT 1",
        [],
        |r| Ok(Active { start_ms: r.get(0)?, planned_ms: r.get(1)?, paused_ms: r.get(2)?, paused_at: r.get(3)?, label: r.get(4)?, task: r.get(5)? }),
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
    insert(conn, minutes, label, now)
}

/// Adds a session that begins now; the caller has seen to any that was running.
fn insert(conn: &Connection, minutes: u32, label: Option<&str>, now: i64) -> Result<Active> {
    let label = label.map(str::trim).filter(|l| !l.is_empty()).map(|l| l.chars().take(MAX_LABEL_CHARS).collect::<String>());
    let a = Active { start_ms: now, planned_ms: minutes.min(MAX_MINUTES) as i64 * MIN_MS, paused_ms: 0, paused_at: None, label, task: None };
    conn.execute(
        "INSERT INTO focus_sessions (start_ms, planned_ms, label, nudges) VALUES (?1, ?2, ?3, 0)",
        params![a.start_ms, a.planned_ms, a.label],
    )?;
    Ok(a)
}

/// Starts a session someone asked for: it blocks the saved list its label names, or else the
/// default list, or else as usual (see `lists::for_session`). Started on a task that exists, it
/// counts for the task and takes its title as the label; the typed label still picks the list.
pub fn start_by_hand(conn: &Connection, minutes: u32, label: Option<&str>, task: Option<&str>, now: i64) -> Result<Active> {
    stop(conn, now)?;
    begin_by_hand(conn, minutes, label, task, None, now)
}

/// `start_by_hand` without ending what runs, for a pomodoro cycle's rounds (`cycle` is the
/// cycle's id), which come when no session is running and must not end the cycle.
pub(crate) fn begin_by_hand(conn: &Connection, minutes: u32, label: Option<&str>, task: Option<&str>, cycle: Option<i64>, now: i64) -> Result<Active> {
    let list = crate::lists::for_session(conn, label)?;
    let task = match task {
        Some(id) => crate::tasks::title(conn, id)?.map(|title| (id.to_string(), title)),
        None => None,
    };
    let mut a = insert(conn, minutes, task.as_ref().map_or(label, |(_, title)| Some(title.as_str())), now)?;
    a.task = task.map(|(id, _)| id);
    conn.execute("UPDATE focus_sessions SET list = ?1, task = ?2, cycle_id = ?3 WHERE end_ms IS NULL", params![list, a.task, cycle])?;
    Ok(a)
}

/// Starts a session for scheduled slot `slot`, whose block list then applies while it runs.
pub fn start_from_slot(conn: &Connection, minutes: u32, label: Option<&str>, slot: i64, now: i64) -> Result<Active> {
    let a = start(conn, minutes, label, now)?;
    conn.execute("UPDATE focus_sessions SET slot = ?1 WHERE end_ms IS NULL", [slot])?;
    Ok(a)
}

/// Ends the running session early, folding a pause in progress into its paused time. That ends a
/// pomodoro cycle too, whether a round or a break is on.
pub fn stop(conn: &Connection, now: i64) -> Result<()> {
    crate::pomodoro::clear(conn)?;
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
pub(crate) fn complete_if_due(conn: &Connection, now: i64) -> Result<Option<Active>> {
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

/// Counts a distraction nudge against the running session, for its wrap-up.
pub(crate) fn count_nudge(conn: &Connection) -> Result<()> {
    conn.execute("UPDATE focus_sessions SET nudges = COALESCE(nudges, 0) + 1 WHERE end_ms IS NULL", [])?;
    Ok(())
}

/// The sessions that have ended, the latest to end first.
pub fn ended(conn: &Connection, limit: usize) -> Result<Vec<Block>> {
    let mut stmt = conn.prepare(&format!("SELECT {BLOCK_COLUMNS} FROM focus_sessions WHERE end_ms IS NOT NULL ORDER BY end_ms DESC, id DESC LIMIT ?2"))?;
    let rows = stmt.query_map(params![0, limit as i64], block)?;
    rows.collect()
}

/// The rounds of pomodoro cycle `id` that have ended, the first first.
pub fn cycle_blocks(conn: &Connection, id: i64) -> Result<Vec<Block>> {
    let mut stmt = conn.prepare(&format!("SELECT {BLOCK_COLUMNS} FROM focus_sessions WHERE end_ms IS NOT NULL AND cycle_id = ?2 ORDER BY start_ms"))?;
    let rows = stmt.query_map(params![0, id], block)?;
    rows.collect()
}

// ---------- the daily focus target ----------

const TARGET_KEY: &str = "goal_focus_min";
const TARGET_STREAK_LOOKBACK_DAYS: usize = 60;

/// Minutes of focus aimed for each day; 0 means none.
pub fn target_minutes(conn: &Connection) -> Result<u32> {
    Ok(db::get_setting(conn, TARGET_KEY)?.and_then(|v| v.parse().ok()).unwrap_or(0))
}

pub fn set_target_minutes(conn: &Connection, minutes: u32) -> Result<()> {
    db::set_setting(conn, TARGET_KEY, &minutes.to_string())
}

/// Time focused in the sessions that started in [from, to), pauses left out; a running one
/// counts up to `now`.
pub fn focused_started_in(conn: &Connection, from: i64, to: i64, now: i64) -> Result<i64> {
    Ok(blocks(conn, from, to, now)?.iter().filter(|b| b.start >= from).map(Block::ms).sum())
}

/// How a day stands against the daily focus target.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub target_ms: i64,
    pub used_ms: i64,
    pub met: bool,
    /// Days in a row before this one on which it was reached.
    pub streak: u32,
}

/// The daily focus target on `date`; None when none is set.
pub fn target(conn: &Connection, date: chrono::NaiveDate, now: i64) -> Result<Option<Target>> {
    let target_ms = target_minutes(conn)? as i64 * MIN_MS;
    if target_ms == 0 {
        return Ok(None);
    }
    let focused = |day: chrono::NaiveDate| -> Result<Option<(i64, i64, i64)>> {
        let Some((from, to)) = crate::analysis::day_bounds(day) else { return Ok(None) };
        Ok(Some((focused_started_in(conn, from, to, now)?, from, to)))
    };
    let used_ms = focused(date)?.map_or(0, |f| f.0);
    // Days with nothing tracked and no session (weekends away) neither count nor break it.
    let mut streak = 0;
    let mut day = date;
    for _ in 0..TARGET_STREAK_LOOKBACK_DAYS {
        let Some(prev) = day.pred_opt() else { break };
        day = prev;
        let Some((ms, from, to)) = focused(day)? else { break };
        if ms >= target_ms {
            streak += 1;
        } else if ms > 0 || conn.query_row("SELECT EXISTS(SELECT 1 FROM sessions WHERE idle = 0 AND end_ms > ?1 AND start_ms < ?2)", [from, to], |r| r.get::<_, bool>(0))? {
            break;
        }
    }
    Ok(Some(Target { target_ms, used_ms, met: used_ms >= target_ms, streak }))
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

/// Whether a scheduled slot that's due may start: once someone is at the computer, nothing else is
/// running, and they're neither in a game, which a session would block mid-match, nor in a meeting
/// on their calendar; the slot waits and starts when that's over, for what's left of its window.
fn schedule_may_start(session_running: bool, paused: bool, idle_ms: i64, busy: bool) -> bool {
    !session_running && !paused && idle_ms < SCHEDULE_NEEDS_ACTIVE_MS && !busy
}

fn tick(app: &AppHandle, conn: &Connection, w: &mut Watch, paused: bool) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let now = now_ms();
    let slept = now - w.last_tick > SLEEP_GAP_MS;
    w.last_tick = now;

    // Quick timers ring whether or not tracking is paused, and a pause asked for by the minute
    // ends here; `paused` is as it was when this tick began, which the next one puts right.
    for timer in crate::timers::take_due(conn, now)? {
        let (title, body) = timer.notice(now);
        notify(app, &title, &body);
    }
    crate::resume_if_due(app, now);

    let completed = complete_if_due(conn, now)?;
    // A cycle starts its break when a round ends and its next round when the break does.
    let cycle_event = crate::pomodoro::tick(conn, now, platform::idle_ms())?;
    if completed.is_some() {
        crate::reminders::announce(crate::reminders::Moment::FocusEnded);
    }
    if let Some(done) = &completed {
        // How it went, in a line; the whole wrap-up waits on the Focus page. In a cycle the break
        // is announced instead, and the wrap-up waits for the cycle to end.
        if cycle_event.is_none() {
            let said = crate::wrapup::latest(conn, now).ok().flatten().map_or_else(|| "Nice work.".to_string(), |w| crate::wrapup::line(&w));
            notify(app, &format!("Focus session done: {}", fmt_dur(done.planned_ms)), &said);
        }
    }
    if let Some(event) = &cycle_event {
        let (title, body) = event.notice();
        notify(app, &title, &body);
    }
    if completed.is_some() || cycle_event.is_some() {
        // The session's end is a natural break point.
        w.stretch_since = Some(now);
        w.breaks_sent = 0;
        // The open window re-reads the session now rather than at its next poll.
        let _ = app.emit("kairo:focus", ());
    }
    let mut session = active(conn)?;

    let fg = platform::foreground();
    let gaming = fg.as_ref().is_some_and(|f| classify::is_game(&classify::exe_stem(&f.exe_path), &f.exe_path));

    let in_meeting = crate::calendar::busy_at(conn, now)?.is_some();
    if schedule_may_start(session.is_some(), paused, platform::idle_ms(), gaming || in_meeting) {
        if let Some((slot, day, left)) = schedule::due(conn, Local::now())? {
            let started = start_from_slot(conn, left, Some(slot.label.as_deref().unwrap_or("Scheduled focus")), slot.id, now)?;
            schedule::mark_started(conn, slot.id, day)?;
            let until = Local.timestamp_millis_opt(started.ends_ms(now)).single().map(|t| t.format("%H:%M").to_string()).unwrap_or_default();
            let body = match schedule::session_blocks(conn)? {
                Some(keys) => format!("{} blocked until then.", blocking::describe(conn, &keys)?),
                None => "Distracting apps and sites are blocked until then, as in any focus session.".to_string(),
            };
            notify(app, &format!("Scheduled focus until {until}"), &body);
            let _ = app.emit("kairo:focus", ());
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
                count_nudge(conn)?;
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
    fn scheduled_focus_waits_out_games_and_absence() {
        // At the computer, nothing running, not gaming: it starts.
        assert!(schedule_may_start(false, false, 10_000, false));
        // In a game or a meeting it waits, however active the user is.
        assert!(!schedule_may_start(false, false, 0, true));
        // Away from the computer, with a session already running, or with tracking paused, too.
        assert!(!schedule_may_start(false, false, 3 * M, false));
        assert!(!schedule_may_start(true, false, 0, false));
        assert!(!schedule_may_start(false, true, 0, false));
    }

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
                Block { start: t, end: t + 10 * M, planned_ms: 25 * M, paused_ms: 0, completed: false, label: None, nudges: Some(0), cycle: None },
                Block { start: t + 10 * M, end: t + 60 * M, planned_ms: 50 * M, paused_ms: 0, completed: true, label, nudges: Some(0), cycle: None },
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
    fn nudges_are_counted_against_the_running_session() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let t = 1_000 * M;
        start(&conn, 25, None, t).unwrap();
        stop(&conn, t + 5 * M).unwrap();
        start(&conn, 25, None, t + 10 * M).unwrap();
        count_nudge(&conn).unwrap();
        count_nudge(&conn).unwrap();
        stop(&conn, t + 20 * M).unwrap();
        // With none running there's nothing to count against.
        count_nudge(&conn).unwrap();
        let latest_first: Vec<Option<i64>> = ended(&conn, 5).unwrap().iter().map(Block::nudges).collect();
        assert_eq!(latest_first, [Some(2), Some(0)]);
    }

    #[test]
    fn the_daily_target_counts_sessions_by_the_day_they_started_and_keeps_a_streak() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        let day = |ago: u64| crate::analysis::day_bounds(today - chrono::Days::new(ago)).unwrap().0;
        let now = day(0) + 15 * 60 * M;
        assert_eq!(target(&conn, today, now).unwrap(), None);
        set_target_minutes(&conn, 60).unwrap();
        let session = |from: i64, minutes: i64| {
            start(&conn, minutes as u32, None, from).unwrap();
            stop(&conn, from + minutes * M).unwrap();
        };
        // Yesterday: an hour in two sittings. Two days ago: nothing tracked at all. Three days
        // ago: 90 minutes. Four days ago: at the computer, but only 20 minutes of focus.
        session(day(1) + 9 * 60 * M, 40);
        session(day(1) + 14 * 60 * M, 20);
        session(day(3) + 9 * 60 * M, 90);
        session(day(4) + 9 * 60 * M, 20);
        session(day(5) + 9 * 60 * M, 120);
        // Today: 25 minutes done, and one paused for 5 of its 20 so far.
        session(day(0) + 9 * 60 * M, 25);
        start(&conn, 50, None, now - 20 * M).unwrap();
        pause(&conn, now - 5 * M).unwrap();
        assert_eq!(target(&conn, today, now).unwrap(), Some(Target { target_ms: 60 * M, used_ms: 40 * M, met: false, streak: 2 }));
        // A day at the computer without a session breaks it.
        let row = db::NewSession { app: "Code", exe: "code", title: "", site: None, start_ms: day(2) + 600 * M, end_ms: day(2) + 660 * M, idle: false, demo: false };
        db::insert_session(&conn, &row).unwrap();
        assert_eq!(target(&conn, today, now).unwrap().unwrap().streak, 1);
        // A session that began late yesterday and ran past midnight is yesterday's.
        stop(&conn, now).unwrap();
        session(day(0) - 10 * M, 60);
        assert_eq!(target(&conn, today, day(0) + 16 * 60 * M).unwrap().unwrap().used_ms, 40 * M);
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
            nudges: None,
            cycle: None,
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
        // Its nudges were never counted, which isn't the same as none.
        assert_eq!(old.nudges, None);
        drop(conn);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    #[test]
    fn starting_on_a_task_labels_it_with_the_title_and_keeps_the_typed_label_for_lists() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        crate::lists::save(&conn, None, "Social", vec!["site:reddit".into()]).unwrap().unwrap();
        let task = crate::tasks::add(&conn, "Social post", 0).unwrap().unwrap();
        let a = start_by_hand(&conn, 25, Some("social"), Some(&task.id), 1_000).unwrap();
        assert_eq!((a.label(), a.task.as_deref()), (Some("Social post"), Some(task.id.as_str())));
        assert_eq!(active(&conn).unwrap().unwrap().task.as_deref(), Some(task.id.as_str()));
        assert_eq!(crate::lists::of_session(&conn).unwrap().as_deref(), Some("Social"));
        let b = start_by_hand(&conn, 25, Some("social"), Some("gone"), 2_000).unwrap();
        assert_eq!((b.label(), b.task.as_deref()), (Some("social"), None));
    }
}
