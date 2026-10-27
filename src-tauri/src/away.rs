//! Time away from the computer, and what it was: a long stretch idle (half an hour or more) can be
//! labelled afterwards as a meeting, a call, a break or just away, and then counts as time under
//! that name like any app's, with a category of its own (`app:away-meeting` and so on).

use rusqlite::{params, Connection, Result};
use serde::Serialize;

/// Shorter stretches idle aren't worth asking about.
const MIN_AWAY_MS: i64 = 30 * 60_000;
/// Idle rows this close together are one stretch away.
const JOIN_GAP_MS: i64 = 2 * 60_000;

/// What a stretch can be labelled: (kind, the name its time goes under).
pub const KINDS: &[(&str, &str)] = &[("meeting", "Meeting"), ("call", "Call"), ("break", "Break"), ("other", "Away")];

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stretch {
    pub start_ms: i64,
    pub end_ms: i64,
}

/// This computer's unlabelled stretches away of half an hour or more within [from, to) that have
/// ended by `now`, oldest first, leaving out those the user waved off.
pub fn stretches(conn: &Connection, from: i64, to: i64, now: i64) -> Result<Vec<Stretch>> {
    let mut stmt = conn.prepare(
        "SELECT start_ms, end_ms FROM sessions
         WHERE idle = 1 AND device IS NULL AND demo = 0 AND end_ms > ?1 AND start_ms < ?2
         ORDER BY start_ms",
    )?;
    let rows: Vec<(i64, i64)> = stmt.query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_>>()?;
    let mut merged: Vec<Stretch> = Vec::new();
    for (start, end) in rows {
        match merged.last_mut() {
            Some(last) if start - last.end_ms <= JOIN_GAP_MS => last.end_ms = last.end_ms.max(end),
            _ => merged.push(Stretch { start_ms: start, end_ms: end }),
        }
    }
    let mut dismissed = conn.prepare("SELECT 1 FROM away_dismissed WHERE start_ms = ?1")?;
    let mut out = Vec::new();
    for s in merged {
        // Still away, or back only a moment: the stretch may yet grow.
        if s.end_ms - s.start_ms < MIN_AWAY_MS || s.end_ms > now - JOIN_GAP_MS || dismissed.exists([s.start_ms])? {
            continue;
        }
        out.push(s);
    }
    Ok(out)
}

/// Labels the stretch [start, end) as `kind`, so its time counts under that name. Returns how
/// many rows changed; an unknown kind changes none.
pub fn label(conn: &Connection, start: i64, end: i64, kind: &str) -> Result<usize> {
    let Some(&(kind, name)) = KINDS.iter().find(|(k, _)| *k == kind) else { return Ok(0) };
    conn.execute(
        "UPDATE sessions SET idle = 0, app = ?1, exe = ?2, title = '', site = NULL
         WHERE idle = 1 AND device IS NULL AND demo = 0 AND start_ms >= ?3 AND end_ms <= ?4",
        params![name, format!("away-{kind}"), start, end],
    )
}

/// Stops asking about the stretch starting at `start`.
pub fn dismiss(conn: &Connection, start: i64) -> Result<()> {
    conn.execute("INSERT OR IGNORE INTO away_dismissed (start_ms) VALUES (?1)", [start])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analysis, db};

    const M: i64 = 60_000;

    fn idle(conn: &Connection, start: i64, minutes: i64) {
        let s = db::NewSession { app: "Idle", exe: "", title: "", site: None, start_ms: start, end_ms: start + minutes * M, idle: true, demo: false };
        db::insert_session(conn, &s).unwrap();
    }

    #[test]
    fn long_stretches_away_can_be_labelled_and_count() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let t = 1_000_000 * M;
        // 40 minutes away in two rows a minute apart, a 10-minute pause, and an hour still going.
        idle(&conn, t, 20);
        idle(&conn, t + 21 * M, 19);
        idle(&conn, t + 100 * M, 10);
        idle(&conn, t + 200 * M, 60);
        let now = t + 260 * M;
        let found = stretches(&conn, t, now + M, now).unwrap();
        assert_eq!(found, [Stretch { start_ms: t, end_ms: t + 40 * M }]);

        assert_eq!(label(&conn, t, t + 40 * M, "meeting").unwrap(), 2);
        assert!(stretches(&conn, t, now + M, now).unwrap().is_empty());
        // The meeting counts as productive time now.
        let rows = analysis::load_rows(&conn, t, t + 40 * M).unwrap();
        assert!(rows.iter().all(|r| !r.idle && r.app == "Meeting" && r.category == analysis::Category::Productive));
        assert_eq!(label(&conn, t + 200 * M, now, "nap").unwrap(), 0);
    }

    #[test]
    fn a_waved_off_stretch_isnt_asked_about_again() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let t = 1_000_000 * M;
        idle(&conn, t, 45);
        let now = t + 60 * M;
        assert_eq!(stretches(&conn, t, now, now).unwrap().len(), 1);
        dismiss(&conn, t).unwrap();
        assert!(stretches(&conn, t, now, now).unwrap().is_empty());
    }
}
