//! What Kairo keeps: per-app and per-site recording rules, deleting history, and how long it lives.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};

use crate::db;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Track,
    /// Time counts, but the window title is never stored.
    HideTitle,
    /// Nothing is recorded while it's in front.
    Ignore,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Mode::Track => "track",
            Mode::HideTitle => "hide_title",
            Mode::Ignore => "ignore",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "track" => Some(Mode::Track),
            "hide_title" => Some(Mode::HideTitle),
            "ignore" => Some(Mode::Ignore),
            _ => None,
        }
    }
}

/// Password managers show vault entries in their titles; nothing about them is worth keeping.
pub const DEFAULT_RULES: &[(&str, &str)] = &[
    ("app:1password", "ignore"),
    ("app:bitwarden", "ignore"),
    ("app:keepass", "ignore"),
    ("app:keepassxc", "ignore"),
    ("app:dashlane", "ignore"),
    ("app:nordpass", "ignore"),
    ("app:enpass", "ignore"),
];

pub type Rules = HashMap<String, Mode>;
/// Read by the collector every tick, replaced by the UI when a rule changes.
pub type SharedRules = Arc<RwLock<Rules>>;

const RETENTION_KEY: &str = "retention_days";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

pub fn load(conn: &Connection) -> Result<Rules> {
    let mut stmt = conn.prepare("SELECT key, mode FROM privacy_rules")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut rules = HashMap::new();
    for row in rows {
        let (key, mode) = row?;
        if let Some(mode) = Mode::parse(&mode) {
            rules.insert(key, mode);
        }
    }
    Ok(rules)
}

pub fn refresh(conn: &Connection, shared: &SharedRules) -> Result<()> {
    let rules = load(conn)?;
    *shared.write().unwrap_or_else(|e| e.into_inner()) = rules;
    Ok(())
}

/// A site's rule wins over its browser's, the same way categories resolve.
pub fn resolve(rules: &Rules, exe: &str, site: Option<&str>) -> Mode {
    site.and_then(|s| rules.get(&format!("site:{s}")))
        .or_else(|| rules.get(&format!("app:{exe}")))
        .copied()
        .unwrap_or(Mode::Track)
}

/// Stored even for "track", so a default the user turned off isn't re-added at the next launch.
pub fn set_rule(conn: &Connection, key: &str, mode: Mode) -> Result<()> {
    conn.execute(
        "INSERT INTO privacy_rules (key, mode, user_set) VALUES (?1, ?2, 1)
         ON CONFLICT(key) DO UPDATE SET mode = excluded.mode, user_set = 1",
        params![key, mode.as_str()],
    )?;
    Ok(())
}

/// Brings what's already stored in line with a rule: forgets ignored sessions, blanks hidden
/// titles. Returns how many sessions changed.
pub fn apply_to_past(conn: &Connection, key: &str, mode: Mode) -> Result<usize> {
    let (column, value) = match key.split_once(':') {
        Some(("app", v)) => ("exe", v),
        Some(("site", v)) => ("site", v),
        _ => return Ok(0),
    };
    match mode {
        Mode::Track => Ok(0),
        Mode::HideTitle => conn.execute(&format!("UPDATE sessions SET title = '' WHERE {column} = ?1 AND title != ''"), [value]),
        Mode::Ignore => conn.execute(&format!("DELETE FROM sessions WHERE {column} = ?1"), [value]),
    }
}

/// Forgets everything recorded in [from, to). Sessions crossing an edge are trimmed to it.
pub fn delete_between(conn: &Connection, from: i64, to: i64) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let mut changed = tx.execute("DELETE FROM sessions WHERE start_ms >= ?1 AND end_ms <= ?2", params![from, to])?;
    changed += tx.execute("UPDATE sessions SET end_ms = ?1 WHERE start_ms < ?1 AND end_ms > ?1", params![from])?;
    changed += tx.execute("UPDATE sessions SET start_ms = ?1 WHERE start_ms < ?1 AND end_ms > ?1", params![to])?;
    tx.commit()?;
    Ok(changed)
}

pub fn delete_all(conn: &Connection) -> Result<usize> {
    conn.execute("DELETE FROM sessions", [])
}

/// Days of history to keep; 0 keeps everything.
pub fn retention_days(conn: &Connection) -> Result<u32> {
    Ok(db::get_setting(conn, RETENTION_KEY)?.and_then(|v| v.parse().ok()).unwrap_or(0))
}

pub fn set_retention_days(conn: &Connection, days: u32) -> Result<()> {
    db::set_setting(conn, RETENTION_KEY, &days.to_string())
}

/// Drops history older than the retention window.
pub fn prune(conn: &Connection, now_ms: i64) -> Result<usize> {
    match retention_days(conn)? {
        0 => Ok(0),
        days => delete_between(conn, i64::MIN, now_ms - days as i64 * DAY_MS),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn_with(sessions: &[(&str, Option<&str>, &str, i64, i64)]) -> Connection {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        for (exe, site, title, start, end) in sessions {
            let row = db::NewSession { app: exe, exe, title, site: *site, start_ms: *start, end_ms: *end, idle: false, demo: false };
            db::insert_session(&conn, &row).unwrap();
        }
        conn
    }

    fn spans(conn: &Connection) -> Vec<(i64, i64)> {
        let mut stmt = conn.prepare("SELECT start_ms, end_ms FROM sessions ORDER BY start_ms").unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(|r| r.unwrap()).collect()
    }

    #[test]
    fn deleting_a_range_trims_sessions_that_cross_it() {
        let conn = conn_with(&[("a", None, "", 0, 100), ("b", None, "", 100, 200), ("c", None, "", 200, 300)]);
        delete_between(&conn, 50, 250).unwrap();
        assert_eq!(spans(&conn), vec![(0, 50), (250, 300)]);
    }

    #[test]
    fn password_managers_are_ignored_by_default_and_sites_beat_apps() {
        let conn = conn_with(&[]);
        let mut rules = load(&conn).unwrap();
        assert_eq!(resolve(&rules, "bitwarden", None), Mode::Ignore);
        assert_eq!(resolve(&rules, "chrome", Some("gmail")), Mode::Track);
        rules.insert("app:chrome".into(), Mode::HideTitle);
        rules.insert("site:gmail".into(), Mode::Ignore);
        assert_eq!(resolve(&rules, "chrome", Some("gmail")), Mode::Ignore);
        assert_eq!(resolve(&rules, "chrome", Some("github")), Mode::HideTitle);
    }

    #[test]
    fn turning_off_a_default_rule_survives_a_restart() {
        let path = std::env::temp_dir().join(format!("kairo-privacy-{}.db", std::process::id()));
        let conn = db::open(&path).unwrap();
        set_rule(&conn, "app:bitwarden", Mode::Track).unwrap();
        drop(conn);
        let conn = db::open(&path).unwrap();
        assert_eq!(resolve(&load(&conn).unwrap(), "bitwarden", None), Mode::Track);
        drop(conn);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }

    #[test]
    fn applying_rules_to_past_data() {
        let conn = conn_with(&[("chrome", Some("gmail"), "Inbox (3) - Gmail", 0, 10), ("code", None, "secret.rs", 10, 20)]);
        assert_eq!(apply_to_past(&conn, "site:gmail", Mode::Ignore).unwrap(), 1);
        assert_eq!(apply_to_past(&conn, "app:code", Mode::HideTitle).unwrap(), 1);
        let left: Vec<(String, String)> =
            conn.prepare("SELECT exe, title FROM sessions").unwrap().query_map([], |r| Ok((r.get(0)?, r.get(1)?))).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(left, vec![("code".to_string(), String::new())]);
    }

    #[test]
    fn retention_drops_only_old_history() {
        let now = 100 * DAY_MS;
        let conn = conn_with(&[("old", None, "", 0, DAY_MS), ("new", None, "", 95 * DAY_MS, 96 * DAY_MS)]);
        assert_eq!(prune(&conn, now).unwrap(), 0);
        set_retention_days(&conn, 30).unwrap();
        prune(&conn, now).unwrap();
        assert_eq!(spans(&conn), vec![(95 * DAY_MS, 96 * DAY_MS)]);
    }
}
