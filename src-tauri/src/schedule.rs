//! Scheduled focus: focus sessions that start on their own at set times each week, like weekdays
//! 9 to 12. The focus thread asks [`due`] every few seconds; a slot starts once a day, when the
//! user is at the computer, and runs to the end of its window, so arriving late gives a shorter
//! session. Ending it early keeps it from starting again that day. A slot can block its own list
//! of apps and sites instead of everything distracting.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone};
use rusqlite::{params, Connection, OptionalExtension, Result};
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
    /// `app:<exe>` and `site:<name>` keys this slot blocks, and nothing else; None blocks what
    /// any session does.
    #[serde(default)]
    pub blocks: Option<Vec<String>>,
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

fn slot(r: &rusqlite::Row) -> Result<Slot> {
    let blocks = r.get::<_, Option<String>>(5)?.and_then(|b| serde_json::from_str(&b).ok());
    Ok(Slot { id: r.get(0)?, days: r.get(1)?, start_min: r.get(2)?, minutes: r.get(3)?, label: r.get(4)?, blocks })
}

pub fn list(conn: &Connection) -> Result<Vec<Slot>> {
    let mut stmt = conn.prepare("SELECT id, days, start_min, minutes, label, blocks FROM focus_schedule ORDER BY start_min, id")?;
    let slots = stmt.query_map([], slot)?.collect();
    slots
}

/// A block list as given, tidied: only app and site keys, each once; an empty one means none.
fn tidy_blocks(blocks: Option<Vec<String>>) -> Option<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for key in blocks?.into_iter().map(|k| k.trim().to_string()) {
        if (key.starts_with("app:") || key.starts_with("site:")) && key.len() > 5 && !out.contains(&key) {
            out.push(key);
        }
    }
    (!out.is_empty()).then_some(out)
}

fn blocks_json(blocks: &Option<Vec<String>>) -> Option<String> {
    blocks.as_ref().map(|b| serde_json::to_string(b).unwrap_or_else(|_| "[]".into()))
}

/// Gives slot `id` its own block list, or with None, the usual blocking back.
pub fn set_blocks(conn: &Connection, id: i64, blocks: Option<Vec<String>>) -> Result<()> {
    conn.execute("UPDATE focus_schedule SET blocks = ?1 WHERE id = ?2", params![blocks_json(&tidy_blocks(blocks)), id])?;
    Ok(())
}

/// The block list of the slot that started the running session; None when it blocks as usual,
/// was started by hand, or its slot has since gone.
pub fn session_blocks(conn: &Connection) -> Result<Option<Vec<String>>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT s.blocks FROM focus_sessions f JOIN focus_schedule s ON s.id = f.slot
             WHERE f.end_ms IS NULL ORDER BY f.id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(json.and_then(|b| serde_json::from_str(&b).ok()))
}

/// A slot as the user described it, trimmed to what makes sense; None for one with no days.
pub fn tidy(days: u8, start_min: u32, minutes: u32, label: Option<&str>) -> Option<Slot> {
    let days = days & 0x7f;
    let label = label.map(str::trim).filter(|l| !l.is_empty()).map(|l| l.chars().take(crate::focus::MAX_LABEL_CHARS).collect());
    (days != 0).then(|| Slot { id: 0, days, start_min: start_min.min(24 * 60 - 1), minutes: minutes.clamp(MIN_LEFT_MINUTES, MAX_MINUTES), label, blocks: None })
}

pub fn add(conn: &Connection, slot: &Slot) -> Result<i64> {
    conn.execute(
        "INSERT INTO focus_schedule (days, start_min, minutes, label, blocks) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![slot.days, slot.start_min, slot.minutes, slot.label, blocks_json(&tidy_blocks(slot.blocks.clone()))],
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
            add(&tx, &Slot { blocks: s.blocks, ..slot }).map_err(|e| e.to_string())?;
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
        let id = add(&here, &tidy(0b110_0000, 10 * 60, 60, Some("Weekend reading")).unwrap()).unwrap();
        set_blocks(&here, id, Some(vec!["site:youtube".into()])).unwrap();
        add(&there, &tidy(1, 8 * 60, 30, None).unwrap()).unwrap();
        replace_from_json(&there, &to_json(&here).unwrap()).unwrap();
        let slots = list(&there).unwrap();
        assert_eq!(slots.len(), 1);
        assert_eq!((slots[0].days, slots[0].start_min, slots[0].label.as_deref()), (0b110_0000, 600, Some("Weekend reading")));
        assert_eq!(slots[0].blocks, Some(vec!["site:youtube".to_string()]));
        // A schedule written before slots had block lists still reads.
        replace_from_json(&there, r#"[{"days":1,"startMin":540,"minutes":60,"label":null}]"#).unwrap();
        assert_eq!(list(&there).unwrap()[0].blocks, None);
    }

    #[test]
    fn a_session_takes_the_block_list_of_the_slot_that_started_it() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let id = add(&conn, &tidy(0b111_1111, 9 * 60, 60, None).unwrap()).unwrap();
        // Tidied: keys only, each once, and an empty list is no list.
        set_blocks(&conn, id, Some(vec!["site:reddit".into(), " app:steam ".into(), "site:reddit".into(), "nonsense".into()])).unwrap();
        assert_eq!(list(&conn).unwrap()[0].blocks, Some(vec!["site:reddit".to_string(), "app:steam".to_string()]));

        // Started by hand, a session blocks as usual.
        crate::focus::start(&conn, 30, None, 1_000).unwrap();
        assert_eq!(session_blocks(&conn).unwrap(), None);
        crate::focus::start_from_slot(&conn, 30, None, id, 2_000).unwrap();
        assert_eq!(session_blocks(&conn).unwrap(), Some(vec!["site:reddit".to_string(), "app:steam".to_string()]));

        set_blocks(&conn, id, Some(Vec::new())).unwrap();
        assert_eq!(session_blocks(&conn).unwrap(), None);
        set_blocks(&conn, id, Some(vec!["site:reddit".into()])).unwrap();
        remove(&conn, id).unwrap();
        assert_eq!(session_blocks(&conn).unwrap(), None);
    }
}
