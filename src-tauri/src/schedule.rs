//! Scheduled focus: focus sessions that start on their own at set times each week, like weekdays
//! 9 to 12. The focus thread asks [`due`] every few seconds; a slot starts once a day, when the
//! user is at the computer, and runs to the end of its window, so arriving late gives a shorter
//! session. Ending it early keeps it from starting again that day.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone};
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};

/// Shorter than this left in a window isn't worth starting a session for.
const MIN_LEFT_MINUTES: u32 = 5;
pub const MAX_MINUTES: u32 = 8 * 60;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Slot {
    #[serde(default)]
    pub id: i64,
    /// Which days, Monday in bit 0 to Sunday in bit 6.
    pub days: u8,
    /// Minutes after local midnight.
    pub start_min: u32,
    pub minutes: u32,
    pub label: Option<String>,
}

impl Slot {
    fn on(&self, date: NaiveDate) -> bool {
        self.days & (1 << date.weekday().num_days_from_monday()) != 0
    }

    /// When the slot starts and ends on `date`, in local time; None on a day the clocks skip it.
    fn window(&self, date: NaiveDate) -> Option<(DateTime<Local>, DateTime<Local>)> {
        let start = Local.from_local_datetime(&date.and_hms_opt(self.start_min / 60, self.start_min % 60, 0)?).earliest()?;
        Some((start, start + Duration::minutes(self.minutes as i64)))
    }
}

pub fn list(conn: &Connection) -> Result<Vec<Slot>> {
    let mut stmt = conn.prepare("SELECT id, days, start_min, minutes, label FROM focus_schedule ORDER BY start_min, id")?;
    let slots = stmt
        .query_map([], |r| Ok(Slot { id: r.get(0)?, days: r.get(1)?, start_min: r.get(2)?, minutes: r.get(3)?, label: r.get(4)? }))?
        .collect();
    slots
}

/// A slot as the user described it, trimmed to what makes sense; None for one with no days.
pub fn tidy(days: u8, start_min: u32, minutes: u32, label: Option<&str>) -> Option<Slot> {
    let days = days & 0x7f;
    let label = label.map(str::trim).filter(|l| !l.is_empty()).map(|l| l.chars().take(crate::focus::MAX_LABEL_CHARS).collect());
    (days != 0).then(|| Slot { id: 0, days, start_min: start_min.min(24 * 60 - 1), minutes: minutes.clamp(MIN_LEFT_MINUTES, MAX_MINUTES), label })
}

pub fn add(conn: &Connection, slot: &Slot) -> Result<i64> {
    conn.execute(
        "INSERT INTO focus_schedule (days, start_min, minutes, label) VALUES (?1, ?2, ?3, ?4)",
        params![slot.days, slot.start_min, slot.minutes, slot.label],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn remove(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM focus_schedule WHERE id = ?1", [id])?;
    conn.execute("DELETE FROM focus_schedule_runs WHERE slot = ?1", [id])?;
    Ok(())
}

/// The whole schedule as text, for sharing with combined computers.
pub fn to_json(conn: &Connection) -> Result<String> {
    let slots: Vec<Slot> = list(conn)?.into_iter().map(|s| Slot { id: 0, ..s }).collect();
    Ok(serde_json::to_string(&slots).unwrap_or_else(|_| "[]".into()))
}

/// Takes another computer's schedule in place of this one's.
pub fn replace_from_json(conn: &Connection, json: &str) -> std::result::Result<(), String> {
    let slots: Vec<Slot> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute_batch("DELETE FROM focus_schedule; DELETE FROM focus_schedule_runs;").map_err(|e| e.to_string())?;
    for s in slots {
        if let Some(slot) = tidy(s.days, s.start_min, s.minutes, s.label.as_deref()) {
            add(&tx, &slot).map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

/// The slot whose window holds `now` and hasn't started on its day yet, with the whole minutes
/// left in it; a window that began yesterday evening counts as yesterday's.
pub fn due(conn: &Connection, now: DateTime<Local>) -> Result<Option<(Slot, NaiveDate, u32)>> {
    let today = now.date_naive();
    for slot in list(conn)? {
        for date in [today, today.pred_opt().unwrap_or(today)] {
            let Some((start, end)) = slot.window(date).filter(|_| slot.on(date)) else { continue };
            if now < start || now >= end {
                continue;
            }
            let left = ((end - now).num_seconds() / 60) as u32;
            if left < MIN_LEFT_MINUTES || started(conn, slot.id, date)? {
                continue;
            }
            return Ok(Some((slot, date, left)));
        }
    }
    Ok(None)
}

fn started(conn: &Connection, slot: i64, date: NaiveDate) -> Result<bool> {
    conn.prepare("SELECT 1 FROM focus_schedule_runs WHERE slot = ?1 AND day = ?2")?.exists(params![slot, date.to_string()])
}

/// Notes that a slot started on `date`, so it doesn't start again that day; older notes go.
pub fn mark_started(conn: &Connection, slot: i64, date: NaiveDate) -> Result<()> {
    conn.execute("INSERT OR IGNORE INTO focus_schedule_runs (slot, day) VALUES (?1, ?2)", params![slot, date.to_string()])?;
    let week_ago = date - chrono::Days::new(7);
    conn.execute("DELETE FROM focus_schedule_runs WHERE day < ?1", [week_ago.to_string()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn at(date: NaiveDate, h: u32, m: u32) -> DateTime<Local> {
        Local.from_local_datetime(&date.and_hms_opt(h, m, 0).unwrap()).earliest().unwrap()
    }

    #[test]
    fn a_slot_starts_once_on_its_days_for_what_is_left_of_its_window() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        // Weekdays, 9:00 for three hours.
        let slot = tidy(0b001_1111, 9 * 60, 180, Some("  Deep work ")).unwrap();
        assert_eq!(slot.label.as_deref(), Some("Deep work"));
        let id = add(&conn, &slot).unwrap();
        let monday = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        let saturday = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();

        assert!(due(&conn, at(monday, 8, 59)).unwrap().is_none());
        // Arriving at 10:30 leaves 90 minutes.
        let (found, day, left) = due(&conn, at(monday, 10, 30)).unwrap().unwrap();
        assert_eq!((found.id, day, left), (id, monday, 90));
        mark_started(&conn, id, monday).unwrap();
        // Once started, ending it early doesn't bring it back that day.
        assert!(due(&conn, at(monday, 11, 0)).unwrap().is_none());
        // Too little left to bother, and not on weekends.
        assert!(due(&conn, at(monday + chrono::Days::new(1), 11, 57)).unwrap().is_none());
        assert!(due(&conn, at(saturday, 10, 0)).unwrap().is_none());
        assert!(tidy(0, 0, 30, None).is_none());
    }

    #[test]
    fn a_late_evening_slot_runs_past_midnight() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let id = add(&conn, &tidy(0b111_1111, 23 * 60, 120, None).unwrap()).unwrap();
        let monday = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        let (_, day, left) = due(&conn, at(monday + chrono::Days::new(1), 0, 30)).unwrap().unwrap();
        assert_eq!((day, left), (monday, 30));
        mark_started(&conn, id, monday).unwrap();
        assert!(due(&conn, at(monday + chrono::Days::new(1), 0, 40)).unwrap().is_none());
    }

    #[test]
    fn a_schedule_moves_between_computers_as_text() {
        let (here, there) = (db::open(std::path::Path::new(":memory:")).unwrap(), db::open(std::path::Path::new(":memory:")).unwrap());
        add(&here, &tidy(0b110_0000, 10 * 60, 60, Some("Weekend reading")).unwrap()).unwrap();
        add(&there, &tidy(1, 8 * 60, 30, None).unwrap()).unwrap();
        replace_from_json(&there, &to_json(&here).unwrap()).unwrap();
        let slots = list(&there).unwrap();
        assert_eq!(slots.len(), 1);
        assert_eq!((slots[0].days, slots[0].start_min, slots[0].label.as_deref()), (0b110_0000, 600, Some("Weekend reading")));
    }
}
