//! Quick timers: a countdown started from the palette (`timer 10 tea`) that ends in a
//! notification. They track nothing and block nothing, and aren't focus sessions. Kept by the time
//! they end, so a restart doesn't lose them.

use chrono::{Local, TimeZone};
use rusqlite::{params, Connection, Result};

const MIN_MS: i64 = 60_000;
/// Timers run 1 to 240 minutes, like focus sessions.
pub const MAX_MINUTES: u32 = 240;
/// This many run at once at most.
pub const MAX_RUNNING: usize = 20;
/// Names are a word or two.
const MAX_LABEL_CHARS: usize = 40;
/// A timer that rings this long after its time says when it was due: Kairo was closed, or the
/// computer asleep.
const LATE_MS: i64 = MIN_MS;

#[derive(Clone, Debug, PartialEq)]
pub struct Timer {
    pub id: i64,
    pub ends_ms: i64,
    pub minutes: u32,
    pub label: Option<String>,
}

impl Timer {
    /// What it's called: its name, or its length.
    pub fn name(&self) -> String {
        match &self.label {
            Some(label) => label.clone(),
            None if self.minutes == 1 => "1 minute".into(),
            None => format!("{} minutes", self.minutes),
        }
    }

    /// The notification for its end, rung at `now`: the title and the line under it.
    pub fn notice(&self, now: i64) -> (String, String) {
        let body = if now - self.ends_ms >= LATE_MS {
            let at = Local.timestamp_millis_opt(self.ends_ms).single().map(|t| t.format("%H:%M").to_string()).unwrap_or_default();
            format!("It ended at {at}.")
        } else {
            "Time's up.".to_string()
        };
        (format!("Timer done: {}", self.name()), body)
    }
}

/// Starts a timer now. None when as many as may run already do.
pub fn start(conn: &Connection, minutes: u32, label: Option<&str>, now: i64) -> Result<Option<Timer>> {
    if running(conn)?.len() >= MAX_RUNNING {
        return Ok(None);
    }
    let minutes = minutes.clamp(1, MAX_MINUTES);
    let label = label.map(str::trim).filter(|l| !l.is_empty()).map(|l| l.chars().take(MAX_LABEL_CHARS).collect::<String>());
    let ends_ms = now + minutes as i64 * MIN_MS;
    conn.execute("INSERT INTO timers (ends_ms, minutes, label) VALUES (?1, ?2, ?3)", params![ends_ms, minutes, label])?;
    Ok(Some(Timer { id: conn.last_insert_rowid(), ends_ms, minutes, label }))
}

pub fn cancel(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM timers WHERE id = ?1", [id])?;
    Ok(())
}

/// The timers still to ring, soonest first.
pub fn running(conn: &Connection) -> Result<Vec<Timer>> {
    let mut stmt = conn.prepare("SELECT id, ends_ms, minutes, label FROM timers ORDER BY ends_ms, id")?;
    let rows = stmt.query_map([], |r| Ok(Timer { id: r.get(0)?, ends_ms: r.get(1)?, minutes: r.get(2)?, label: r.get(3)? }))?;
    rows.collect()
}

/// The timers whose time has come, taken out so each rings once.
pub fn take_due(conn: &Connection, now: i64) -> Result<Vec<Timer>> {
    let due: Vec<Timer> = running(conn)?.into_iter().filter(|t| t.ends_ms <= now).collect();
    if !due.is_empty() {
        conn.execute("DELETE FROM timers WHERE ends_ms <= ?1", [now])?;
    }
    Ok(due)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const M: i64 = MIN_MS;

    fn memory() -> Connection {
        crate::db::open(Path::new(":memory:")).unwrap()
    }

    #[test]
    fn timers_run_side_by_side_soonest_first() {
        let conn = memory();
        let laundry = start(&conn, 25, Some("laundry"), 0).unwrap().unwrap();
        let tea = start(&conn, 10, Some(" tea "), 0).unwrap().unwrap();
        assert_eq!((tea.ends_ms, tea.label.as_deref()), (10 * M, Some("tea")));
        assert_eq!(running(&conn).unwrap(), [tea.clone(), laundry.clone()]);
        cancel(&conn, tea.id).unwrap();
        assert_eq!(running(&conn).unwrap(), [laundry]);
    }

    #[test]
    fn only_what_is_due_rings_and_only_once() {
        let conn = memory();
        start(&conn, 10, Some("tea"), 0).unwrap();
        start(&conn, 25, None, 0).unwrap();
        assert!(take_due(&conn, 9 * M).unwrap().is_empty());
        let due = take_due(&conn, 10 * M).unwrap();
        assert_eq!(due.iter().map(Timer::name).collect::<Vec<_>>(), ["tea"]);
        assert!(take_due(&conn, 11 * M).unwrap().is_empty());
        // One that ended while Kairo was closed rings as soon as it's looked for.
        assert_eq!(take_due(&conn, 300 * M).unwrap().len(), 1);
        assert!(running(&conn).unwrap().is_empty());
    }

    #[test]
    fn lengths_and_names_are_kept_in_bounds() {
        let conn = memory();
        assert_eq!(start(&conn, 0, None, 0).unwrap().unwrap().minutes, 1);
        assert_eq!(start(&conn, 999, Some("   "), 0).unwrap().unwrap(), Timer { id: 2, ends_ms: 240 * M, minutes: 240, label: None });
        for _ in 2..MAX_RUNNING {
            assert!(start(&conn, 5, None, 0).unwrap().is_some());
        }
        assert_eq!(start(&conn, 5, None, 0).unwrap(), None);
    }

    #[test]
    fn a_late_timer_says_when_it_was_due() {
        let named = Timer { id: 1, ends_ms: 10 * M, minutes: 10, label: Some("tea".into()) };
        assert_eq!(named.notice(10 * M + 4000), ("Timer done: tea".to_string(), "Time's up.".to_string()));
        let (title, body) = Timer { label: None, ..named.clone() }.notice(90 * M);
        assert_eq!(title, "Timer done: 10 minutes");
        assert!(body.starts_with("It ended at ") && body.ends_with('.'), "{body}");
        assert_eq!(Timer { minutes: 1, label: None, ..named }.name(), "1 minute");
    }
}
