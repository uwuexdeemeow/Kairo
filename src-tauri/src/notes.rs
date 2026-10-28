//! Weekly review notes: what went well and what to change, kept for each Monday-to-Sunday week.
//! The Sunday recap points to them; Week shows a week's, and Month the notes of its weeks.

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};

/// Notes are a few lines, not a diary.
const MAX_CHARS: usize = 2000;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    #[serde(default)]
    pub well: String,
    #[serde(default)]
    pub change: String,
}

impl Note {
    fn tidy(well: &str, change: &str) -> Note {
        let cut = |s: &str| s.trim().chars().take(MAX_CHARS).collect::<String>();
        Note { well: cut(well), change: cut(change) }
    }

    pub fn is_empty(&self) -> bool {
        self.well.is_empty() && self.change.is_empty()
    }
}

/// A week's notes, by its Monday.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WeekNote {
    pub monday: String,
    #[serde(flatten)]
    pub note: Note,
}

/// The notes for the week starting `monday`; empty when there are none.
pub fn get(conn: &Connection, monday: NaiveDate) -> Result<Note> {
    Ok(conn
        .query_row("SELECT well, change FROM week_notes WHERE monday = ?1", [monday.to_string()], |r| Ok(Note { well: r.get(0)?, change: r.get(1)? }))
        .optional()?
        .unwrap_or_default())
}

/// Keeps the notes for the week starting `monday`, trimmed; both empty takes them away. Returns
/// them as kept.
pub fn set(conn: &Connection, monday: NaiveDate, well: &str, change: &str) -> Result<Note> {
    let note = Note::tidy(well, change);
    if note.is_empty() {
        conn.execute("DELETE FROM week_notes WHERE monday = ?1", [monday.to_string()])?;
    } else {
        conn.execute(
            "INSERT INTO week_notes (monday, well, change) VALUES (?1, ?2, ?3)
             ON CONFLICT(monday) DO UPDATE SET well = excluded.well, change = excluded.change",
            params![monday.to_string(), note.well, note.change],
        )?;
    }
    Ok(note)
}

/// The weeks whose Monday falls in [from, to] that have notes, earliest first.
pub fn between(conn: &Connection, from: NaiveDate, to: NaiveDate) -> Result<Vec<WeekNote>> {
    query(conn, "WHERE monday >= ?1 AND monday <= ?2", &[&from.to_string(), &to.to_string()])
}

/// Every week's notes, for sharing with combined computers when they're first combined.
pub fn all(conn: &Connection) -> Result<Vec<WeekNote>> {
    query(conn, "", &[])
}

fn query(conn: &Connection, filter: &str, args: &[&dyn rusqlite::ToSql]) -> Result<Vec<WeekNote>> {
    let mut stmt = conn.prepare(&format!("SELECT monday, well, change FROM week_notes {filter} ORDER BY monday"))?;
    let notes = stmt.query_map(args, |r| Ok(WeekNote { monday: r.get(0)?, note: Note { well: r.get(1)?, change: r.get(2)? } }))?.collect();
    notes
}

/// A week's notes as text, the value shared for it with combined computers.
pub fn to_json(note: &Note) -> String {
    serde_json::to_string(note).unwrap_or_else(|_| "{}".into())
}

/// Takes another computer's notes for the week starting `monday` (`YYYY-MM-DD`).
pub fn set_from_json(conn: &Connection, monday: &str, json: &str) -> std::result::Result<(), String> {
    let monday = NaiveDate::parse_from_str(monday, "%Y-%m-%d").map_err(|e| e.to_string())?;
    let note: Note = serde_json::from_str(json).map_err(|e| e.to_string())?;
    set(conn, monday, &note.well, &note.change).map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn notes_keep_by_week_and_go_when_emptied() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let monday = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        assert!(get(&conn, monday).unwrap().is_empty());
        let kept = set(&conn, monday, "  Shipped the year view \n", "").unwrap();
        assert_eq!(kept, Note { well: "Shipped the year view".into(), change: String::new() });
        assert_eq!(get(&conn, monday).unwrap(), kept);
        set(&conn, monday, "Shipped the year view", "Fewer late nights").unwrap();
        set(&conn, monday - chrono::Days::new(7), "", "Start earlier").unwrap();
        set(&conn, monday - chrono::Days::new(35), "Long ago", "").unwrap();

        let october = between(&conn, NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(), NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()).unwrap();
        let mondays: Vec<&str> = october.iter().map(|n| n.monday.as_str()).collect();
        assert_eq!(mondays, ["2026-10-19", "2026-10-26"]);

        set(&conn, monday, " ", "").unwrap();
        assert!(get(&conn, monday).unwrap().is_empty());
        assert_eq!(all(&conn).unwrap().len(), 2);
        assert_eq!(set(&conn, monday, &"x".repeat(3000), "").unwrap().well.len(), MAX_CHARS);
    }

    #[test]
    fn notes_move_between_computers_as_text() {
        let (here, there) = (db::open(std::path::Path::new(":memory:")).unwrap(), db::open(std::path::Path::new(":memory:")).unwrap());
        let monday = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        let note = set(&here, monday, "Good week", "Less YouTube").unwrap();
        set_from_json(&there, "2026-10-26", &to_json(&note)).unwrap();
        assert_eq!(get(&there, monday).unwrap(), note);
        // Emptied on one computer, they go on the other too.
        set_from_json(&there, "2026-10-26", &to_json(&Note::default())).unwrap();
        assert!(get(&there, monday).unwrap().is_empty());
    }
}
