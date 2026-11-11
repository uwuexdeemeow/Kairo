//! Pomodoro cycles: focus sessions ("rounds") with breaks between them. A round is an ordinary
//! session; a break is no session at all, so nothing is blocked during one. The cycle itself is a
//! few numbers kept as one setting, so it survives a restart.

use rusqlite::Connection;
use rusqlite::Result;
use serde::{Deserialize, Serialize};

use crate::{db, focus};

const MIN_MS: i64 = 60_000;
const KEY: &str = "pomodoro_cycle";
/// A break that nobody came back from this long after it ended ends the cycle.
const LAPSE_MS: i64 = 12 * 60 * MIN_MS;
/// The next round waits for someone at the computer: input within this long.
const AWAY_MS: i64 = 2 * MIN_MS;

const WORK_KEY: &str = "pomodoro_work_min";
const BREAK_KEY: &str = "pomodoro_break_min";
const LONG_KEY: &str = "pomodoro_long_min";
const ROUNDS_KEY: &str = "pomodoro_rounds";

/// The lengths a cycle starts with unless the palette or the form says otherwise.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Lengths {
    pub work_min: u32,
    pub break_min: u32,
    pub long_min: u32,
    /// Rounds before the long break.
    pub rounds: u32,
}

impl Lengths {
    const DEFAULT: Lengths = Lengths { work_min: 25, break_min: 5, long_min: 15, rounds: 4 };

    fn clamped(self) -> Lengths {
        Lengths {
            work_min: self.work_min.clamp(5, 120),
            break_min: self.break_min.clamp(1, 30),
            long_min: self.long_min.clamp(5, 60),
            rounds: self.rounds.clamp(2, 8),
        }
    }
}

pub fn lengths(conn: &Connection) -> Result<Lengths> {
    let get = |key: &str, default: u32| -> Result<u32> { Ok(db::get_setting(conn, key)?.and_then(|v| v.parse().ok()).unwrap_or(default)) };
    let d = Lengths::DEFAULT;
    Ok(Lengths { work_min: get(WORK_KEY, d.work_min)?, break_min: get(BREAK_KEY, d.break_min)?, long_min: get(LONG_KEY, d.long_min)?, rounds: get(ROUNDS_KEY, d.rounds)? }.clamped())
}

pub fn set_lengths(conn: &Connection, l: Lengths) -> Result<()> {
    let l = l.clamped();
    db::set_setting(conn, WORK_KEY, &l.work_min.to_string())?;
    db::set_setting(conn, BREAK_KEY, &l.break_min.to_string())?;
    db::set_setting(conn, LONG_KEY, &l.long_min.to_string())?;
    db::set_setting(conn, ROUNDS_KEY, &l.rounds.to_string())
}

/// A running cycle.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Cycle {
    /// Its first round's start; every round carries it.
    id: i64,
    work_min: u32,
    break_min: u32,
    long_min: u32,
    rounds: u32,
    /// How many rounds have started, the one running or just done included.
    round: u32,
    label: Option<String>,
    task: Option<String>,
    /// While on a break: when it ends.
    break_until: Option<i64>,
    /// Whether the break is the long one.
    long: bool,
}

impl Cycle {
    /// A round's place in its set of rounds before the long break, from 1.
    fn shown(&self, round: u32) -> u32 {
        (round - 1) % self.rounds + 1
    }
}

/// What the pages and the palette show of a running cycle.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The round running, or on a break the one coming next, from 1.
    pub round: u32,
    pub rounds: u32,
    pub on_break: bool,
    pub long_break: bool,
    pub break_left_ms: i64,
    /// The break is over and the next round waits for someone to come back.
    pub waiting: bool,
}

/// What a cycle did on its own, for the notification.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    BreakStarted { round: u32, rounds: u32, long: bool, minutes: u32 },
    RoundStarted { round: u32, rounds: u32 },
}

impl Event {
    pub fn notice(&self) -> (String, String) {
        match *self {
            Event::BreakStarted { round, long: false, minutes, .. } => (format!("Round {round} done"), format!("{minutes} minutes' break. Nothing is blocked until the next round.")),
            Event::BreakStarted { round, long: true, minutes, .. } => (format!("Round {round} done"), format!("Take {minutes}. Nothing is blocked until the next round.")),
            Event::RoundStarted { round, rounds } => (format!("Break over. Round {round} of {rounds}."), "Blocking is back on.".to_string()),
        }
    }
}

pub fn load(conn: &Connection) -> Result<Option<Cycle>> {
    Ok(db::get_setting(conn, KEY)?.filter(|v| !v.is_empty()).and_then(|v| serde_json::from_str(&v).ok()))
}

fn save(conn: &Connection, c: &Cycle) -> Result<()> {
    db::set_setting(conn, KEY, &serde_json::to_string(c).unwrap_or_default())
}

/// Ends the cycle, if one is on.
pub fn clear(conn: &Connection) -> Result<()> {
    if db::get_setting(conn, KEY)?.is_some_and(|v| !v.is_empty()) {
        db::set_setting(conn, KEY, "")?;
    }
    Ok(())
}

/// Starts a cycle and its first round, replacing whatever ran. `work` and `rest` are minutes for
/// this cycle only; without them the saved lengths apply.
pub fn start(conn: &Connection, work: Option<u32>, rest: Option<u32>, label: Option<&str>, task: Option<&str>, now: i64) -> Result<Cycle> {
    let saved = lengths(conn)?;
    let work_min = work.unwrap_or(saved.work_min).clamp(1, focus::MAX_MINUTES);
    let break_min = rest.unwrap_or(saved.break_min).clamp(1, 60);
    // Starting anew ends any cycle that was on.
    focus::start_by_hand(conn, work_min, label, task, now)?;
    conn.execute("UPDATE focus_sessions SET cycle_id = ?1 WHERE end_ms IS NULL", [now])?;
    let cycle = Cycle {
        id: now,
        work_min,
        break_min,
        long_min: saved.long_min,
        rounds: saved.rounds,
        round: 1,
        label: label.map(str::to_string),
        task: task.map(str::to_string),
        break_until: None,
        long: false,
    };
    save(conn, &cycle)?;
    Ok(cycle)
}

fn start_next(conn: &Connection, c: &mut Cycle, now: i64) -> Result<Event> {
    c.round += 1;
    c.break_until = None;
    c.long = false;
    save(conn, c)?;
    focus::begin_by_hand(conn, c.work_min, c.label.as_deref(), c.task.as_deref(), Some(c.id), now)?;
    Ok(Event::RoundStarted { round: c.shown(c.round), rounds: c.rounds })
}

/// Called from the focus tick, after a round that ran out has been closed: begins the break when
/// a round has just ended, and the next round when the break is over and someone is at the
/// computer. `idle_ms` is the time since the last input.
pub fn tick(conn: &Connection, now: i64, idle_ms: i64) -> Result<Option<Event>> {
    let Some(mut c) = load(conn)? else { return Ok(None) };
    if focus::active(conn)?.is_some() {
        return Ok(None);
    }
    match c.break_until {
        // No round running and no break yet: a round just ran its course (ending one early would
        // have ended the cycle).
        None => {
            let long = c.round % c.rounds == 0;
            let minutes = if long { c.long_min } else { c.break_min };
            c.break_until = Some(now + minutes as i64 * MIN_MS);
            c.long = long;
            save(conn, &c)?;
            Ok(Some(Event::BreakStarted { round: c.shown(c.round), rounds: c.rounds, long, minutes }))
        }
        Some(until) if now - until > LAPSE_MS => {
            clear(conn)?;
            Ok(None)
        }
        Some(until) if now >= until && idle_ms < AWAY_MS => start_next(conn, &mut c, now).map(Some),
        Some(_) => Ok(None),
    }
}

/// Ends the break now and starts the next round; false when no break is on.
pub fn skip_break(conn: &Connection, now: i64) -> Result<bool> {
    match load(conn)? {
        Some(mut c) if c.break_until.is_some() => {
            start_next(conn, &mut c, now)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub fn current(conn: &Connection, now: i64) -> Result<Option<Status>> {
    let Some(c) = load(conn)? else { return Ok(None) };
    Ok(Some(match c.break_until {
        Some(until) => Status { round: c.shown(c.round + 1), rounds: c.rounds, on_break: true, long_break: c.long, break_left_ms: (until - now).max(0), waiting: now >= until },
        None => Status { round: c.shown(c.round), rounds: c.rounds, on_break: false, long_break: false, break_left_ms: 0, waiting: false },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: i64 = MIN_MS;
    const T: i64 = 1_000 * M;

    fn db() -> Connection {
        db::open(std::path::Path::new(":memory:")).unwrap()
    }

    /// Lets the running round run out, as the focus tick does.
    fn finish_round(conn: &Connection, at: i64) {
        assert!(focus::complete_if_due(conn, at).unwrap().is_some());
    }

    #[test]
    fn lengths_default_and_clamp() {
        let conn = db();
        assert_eq!(lengths(&conn).unwrap(), Lengths { work_min: 25, break_min: 5, long_min: 15, rounds: 4 });
        set_lengths(&conn, Lengths { work_min: 1, break_min: 99, long_min: 1, rounds: 50 }).unwrap();
        assert_eq!(lengths(&conn).unwrap(), Lengths { work_min: 5, break_min: 30, long_min: 5, rounds: 8 });
    }

    #[test]
    fn a_cycle_runs_rounds_and_breaks_with_a_long_one_after_the_fourth() {
        let conn = db();
        start(&conn, None, None, Some("essay"), None, T).unwrap();
        let mut now = T;
        for round in 1..=4u32 {
            let status = current(&conn, now).unwrap().unwrap();
            assert_eq!((status.round, status.rounds, status.on_break), (round, 4, false));
            now += 25 * M;
            finish_round(&conn, now);
            // The break begins on the next tick.
            let long = round == 4;
            let minutes = if long { 15 } else { 5 };
            assert_eq!(tick(&conn, now, 0).unwrap(), Some(Event::BreakStarted { round, rounds: 4, long, minutes }));
            let status = current(&conn, now).unwrap().unwrap();
            assert!(status.on_break && status.long_break == long);
            assert_eq!(status.break_left_ms, minutes as i64 * M);
            assert_eq!(status.round, round % 4 + 1);
            // Nothing runs during the break, which is what keeps everything unblocked.
            assert!(focus::active(&conn).unwrap().is_none());
            assert!(focus::running_until(&conn, now).unwrap().is_none());
            // Mid-break nothing happens; at its end the next round starts.
            assert_eq!(tick(&conn, now + (minutes as i64 - 1) * M, 0).unwrap(), None);
            now += minutes as i64 * M;
            let next = tick(&conn, now, 0).unwrap();
            assert_eq!(next, Some(Event::RoundStarted { round: round % 4 + 1, rounds: 4 }));
            assert_eq!(focus::active(&conn).unwrap().unwrap().label(), Some("essay"));
        }
        // The count started over after the long break: this is round 1 of a new set.
        assert_eq!(current(&conn, now).unwrap().unwrap().round, 1);
    }

    #[test]
    fn the_next_round_waits_for_someone_to_come_back_and_a_long_absence_ends_the_cycle() {
        let conn = db();
        start(&conn, Some(25), Some(5), None, None, T).unwrap();
        finish_round(&conn, T + 25 * M);
        tick(&conn, T + 25 * M, 0).unwrap();
        // The break is over but nobody's there.
        assert_eq!(tick(&conn, T + 31 * M, 10 * M).unwrap(), None);
        let status = current(&conn, T + 31 * M).unwrap().unwrap();
        assert!(status.on_break && status.waiting);
        // They come back.
        assert!(matches!(tick(&conn, T + 40 * M, 0).unwrap(), Some(Event::RoundStarted { round: 2, .. })));
        // Another round, another break, and nobody for 12 hours: it ends.
        finish_round(&conn, T + 65 * M);
        tick(&conn, T + 65 * M, 0).unwrap();
        assert_eq!(tick(&conn, T + 65 * M + 13 * 60 * M, 10 * M).unwrap(), None);
        assert!(load(&conn).unwrap().is_none());
    }

    #[test]
    fn ending_early_or_starting_another_session_ends_the_cycle() {
        let conn = db();
        start(&conn, None, None, None, None, T).unwrap();
        focus::stop(&conn, T + 10 * M).unwrap();
        assert!(load(&conn).unwrap().is_none());
        assert_eq!(tick(&conn, T + 11 * M, 0).unwrap(), None);

        start(&conn, None, None, None, None, T + 20 * M).unwrap();
        focus::start_by_hand(&conn, 30, None, None, T + 25 * M).unwrap();
        assert!(load(&conn).unwrap().is_none());

        // Ending during a break ends it as well.
        start(&conn, None, None, None, None, T + 100 * M).unwrap();
        finish_round(&conn, T + 125 * M);
        tick(&conn, T + 125 * M, 0).unwrap();
        focus::stop(&conn, T + 127 * M).unwrap();
        assert!(current(&conn, T + 127 * M).unwrap().is_none());
    }

    #[test]
    fn skipping_a_break_starts_the_next_round_now() {
        let conn = db();
        assert!(!skip_break(&conn, T).unwrap());
        start(&conn, None, None, None, None, T).unwrap();
        // Not on a break yet.
        assert!(!skip_break(&conn, T + 5 * M).unwrap());
        finish_round(&conn, T + 25 * M);
        tick(&conn, T + 25 * M, 0).unwrap();
        assert!(skip_break(&conn, T + 26 * M).unwrap());
        let status = current(&conn, T + 26 * M).unwrap().unwrap();
        assert_eq!((status.round, status.on_break), (2, false));
        assert!(focus::active(&conn).unwrap().is_some());
    }

    #[test]
    fn rounds_are_sessions_of_one_cycle_and_a_break_is_not_focus_time() {
        let conn = db();
        let c = start(&conn, Some(25), Some(5), None, None, T).unwrap();
        finish_round(&conn, T + 25 * M);
        tick(&conn, T + 25 * M, 0).unwrap();
        tick(&conn, T + 30 * M, 0).unwrap();
        finish_round(&conn, T + 55 * M);
        let blocks = focus::cycle_blocks(&conn, c.id).unwrap();
        assert_eq!(blocks.len(), 2);
        assert!(blocks.iter().all(|b| b.completed() && b.cycle() == Some(c.id)));
        // Two 25-minute rounds, not the 5 minutes between them.
        assert_eq!(blocks.iter().map(focus::Block::ms).sum::<i64>(), 50 * M);
        assert_eq!(focus::stats(&conn, T, T + 120 * M, T + 120 * M).unwrap(), focus::Stats::of_ms(2, 2, 50 * M));
        // A session started by hand belongs to no cycle.
        focus::start_by_hand(&conn, 25, None, None, T + 60 * M).unwrap();
        focus::stop(&conn, T + 70 * M).unwrap();
        assert_eq!(focus::ended(&conn, 1).unwrap()[0].cycle(), None);
    }

    #[test]
    fn notices_say_what_happened() {
        assert_eq!(Event::BreakStarted { round: 2, rounds: 4, long: false, minutes: 5 }.notice().0, "Round 2 done");
        assert!(Event::BreakStarted { round: 4, rounds: 4, long: true, minutes: 15 }.notice().1.starts_with("Take 15."));
        assert_eq!(Event::RoundStarted { round: 3, rounds: 4 }.notice().0, "Break over. Round 3 of 4.");
    }
}
