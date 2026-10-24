use std::path::Path;

use rusqlite::{params, Connection, Result};

use crate::classify::DEFAULT_CATEGORIES;

/// One row per uninterrupted stretch in the same window. Times are unix milliseconds.
pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    // WAL lets the collector write while the UI reads.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sessions (
             id       INTEGER PRIMARY KEY,
             app      TEXT    NOT NULL,
             exe      TEXT    NOT NULL,
             title    TEXT    NOT NULL,
             site     TEXT,
             start_ms INTEGER NOT NULL,
             end_ms   INTEGER NOT NULL,
             idle     INTEGER NOT NULL DEFAULT 0,
             demo     INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS sessions_start ON sessions(start_ms);
         CREATE TABLE IF NOT EXISTS categories (
             key      TEXT PRIMARY KEY,
             category TEXT NOT NULL CHECK (category IN ('productive', 'neutral', 'distracting')),
             user_set INTEGER NOT NULL DEFAULT 0
         );",
    )?;
    // INSERT OR IGNORE keeps any category the user has changed.
    let mut stmt = conn.prepare("INSERT OR IGNORE INTO categories (key, category) VALUES (?1, ?2)")?;
    for (key, category) in DEFAULT_CATEGORIES {
        stmt.execute(params![key, category])?;
    }
    drop(stmt);
    Ok(conn)
}

pub struct NewSession<'a> {
    pub app: &'a str,
    pub exe: &'a str,
    pub title: &'a str,
    pub site: Option<&'a str>,
    pub start_ms: i64,
    pub end_ms: i64,
    pub idle: bool,
    pub demo: bool,
}

pub fn insert_session(conn: &Connection, s: &NewSession) -> Result<i64> {
    conn.execute(
        "INSERT INTO sessions (app, exe, title, site, start_ms, end_ms, idle, demo)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![s.app, s.exe, s.title, s.site, s.start_ms, s.end_ms, s.idle, s.demo],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn set_session_end(conn: &Connection, id: i64, end_ms: i64) -> Result<()> {
    conn.execute("UPDATE sessions SET end_ms = ?1 WHERE id = ?2", params![end_ms, id])?;
    Ok(())
}
