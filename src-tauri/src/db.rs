use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Result};

use crate::classify::DEFAULT_CATEGORIES;
use crate::privacy::DEFAULT_RULES as DEFAULT_PRIVACY_RULES;

/// One row per uninterrupted stretch in the same window. Times are unix milliseconds.
pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    // WAL lets the collector write while the UI reads.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    // Deleted history is overwritten on disk instead of lingering in free pages.
    conn.pragma_update(None, "secure_delete", "ON")?;
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
         );
         CREATE TABLE IF NOT EXISTS settings (
             key   TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS privacy_rules (
             key      TEXT PRIMARY KEY,
             mode     TEXT NOT NULL CHECK (mode IN ('track', 'hide_title', 'ignore')),
             user_set INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS limits (
             key     TEXT PRIMARY KEY,
             minutes INTEGER NOT NULL CHECK (minutes > 0)
         );
         CREATE TABLE IF NOT EXISTS focus_sessions (
             id         INTEGER PRIMARY KEY,
             start_ms   INTEGER NOT NULL,
             planned_ms INTEGER NOT NULL,
             -- NULL while the session is running.
             end_ms     INTEGER,
             completed  INTEGER NOT NULL DEFAULT 0,
             paused_ms  INTEGER NOT NULL DEFAULT 0,
             -- Set while paused.
             paused_at  INTEGER,
             label      TEXT
         );
         CREATE TABLE IF NOT EXISTS app_paths (
             exe  TEXT PRIMARY KEY,
             path TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS focus_rules (
             key  TEXT PRIMARY KEY,
             rule TEXT NOT NULL CHECK (rule IN ('block', 'allow'))
         );
         CREATE TABLE IF NOT EXISTS goal_alerts (
             day   TEXT    NOT NULL,
             key   TEXT    NOT NULL,
             level INTEGER NOT NULL,
             PRIMARY KEY (day, key)
         );
         -- Combining computers (sync.rs): what this computer last wrote for each day, and which
         -- of another computer's day files have been read in, by their size and time.
         CREATE TABLE IF NOT EXISTS sync_written (
             day       INTEGER PRIMARY KEY,
             signature TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS sync_imports (
             device TEXT    NOT NULL,
             day    INTEGER NOT NULL,
             stamp  TEXT    NOT NULL,
             PRIMARY KEY (device, day)
         );
         -- The categories, limits, goals and rules the user chose, and when, so the latest
         -- choice wins on every computer combined.
         -- Scheduled focus (schedule.rs): when sessions start on their own, and on which days
         -- each slot already started, so ending one early doesn't bring it back.
         CREATE TABLE IF NOT EXISTS focus_schedule (
             id        INTEGER PRIMARY KEY,
             days      INTEGER NOT NULL,
             start_min INTEGER NOT NULL,
             minutes   INTEGER NOT NULL,
             label     TEXT
         );
         CREATE TABLE IF NOT EXISTS focus_schedule_runs (
             slot INTEGER NOT NULL,
             day  TEXT    NOT NULL,
             PRIMARY KEY (slot, day)
         );
         -- Block lists (lists.rs): named sets of apps and sites, as a JSON array of keys, that
         -- slots and sessions block instead of everything distracting.
         CREATE TABLE IF NOT EXISTS block_lists (
             name     TEXT    NOT NULL UNIQUE COLLATE NOCASE,
             keys     TEXT    NOT NULL,
             position INTEGER NOT NULL DEFAULT 0
         );
         -- Projects (projects.rs): a name, and comma-separated keywords marking its time.
         CREATE TABLE IF NOT EXISTS projects (
             id       INTEGER PRIMARY KEY,
             name     TEXT    NOT NULL,
             keywords TEXT    NOT NULL,
             position INTEGER NOT NULL DEFAULT 0
         );
         -- Meetings from the user's calendars (calendar.rs), two weeks either side of now.
         CREATE TABLE IF NOT EXISTS calendar_events (
             start_ms INTEGER NOT NULL,
             end_ms   INTEGER NOT NULL,
             title    TEXT    NOT NULL
         );
         CREATE INDEX IF NOT EXISTS calendar_events_start ON calendar_events(start_ms);
         -- Stretches away the user waved off rather than labelling (away.rs), by when they began.
         CREATE TABLE IF NOT EXISTS away_dismissed (
             start_ms INTEGER PRIMARY KEY
         );
         CREATE TABLE IF NOT EXISTS shared_choices (
             item       TEXT    PRIMARY KEY,
             value      TEXT    NOT NULL,
             changed_ms INTEGER NOT NULL
         );",
    )?;
    // Columns added after a table first shipped; CREATE TABLE IF NOT EXISTS won't add them.
    for (table, column, decl) in [
        ("focus_sessions", "paused_ms", "INTEGER NOT NULL DEFAULT 0"),
        ("focus_sessions", "paused_at", "INTEGER"),
        ("focus_sessions", "label", "TEXT"),
        // Which computer a session came from, when combining computers; NULL for this one.
        ("sessions", "device", "TEXT"),
        // Whether passing a limit blocks the app or site for the rest of the day (goals.rs).
        ("limits", "hard", "INTEGER NOT NULL DEFAULT 0"),
        // A scheduled slot's own block list, as a JSON array of keys; NULL blocks as usual.
        ("focus_schedule", "blocks", "TEXT"),
        // The slot that started a session, so its block list applies while it runs.
        ("focus_sessions", "slot", "INTEGER"),
        // The saved block list a slot uses, by name, instead of its own (lists.rs).
        ("focus_schedule", "list", "TEXT"),
        // The saved block list a session started by hand blocks, by name.
        ("focus_sessions", "list", "TEXT"),
        // A project's weekly target in minutes; 0 means none (goals.rs).
        ("projects", "week_target_min", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        let exists = conn.prepare(&format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1"))?.exists([column])?;
        if !exists {
            conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"))?;
        }
    }
    // INSERT OR IGNORE keeps any category or rule the user has changed.
    for (sql, defaults) in [
        ("INSERT OR IGNORE INTO categories (key, category) VALUES (?1, ?2)", DEFAULT_CATEGORIES),
        ("INSERT OR IGNORE INTO privacy_rules (key, mode) VALUES (?1, ?2)", DEFAULT_PRIVACY_RULES),
    ] {
        let mut stmt = conn.prepare(sql)?;
        for (key, value) in defaults {
            stmt.execute(params![key, value])?;
        }
    }
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

/// False when the session no longer exists, e.g. the user just deleted that stretch of history.
pub fn set_session_end(conn: &Connection, id: i64, end_ms: i64) -> Result<bool> {
    Ok(conn.execute("UPDATE sessions SET end_ms = ?1 WHERE id = ?2", params![end_ms, id])? > 0)
}

/// Where an app's program lives, for its icon. The latest sighting wins, so a moved install heals.
pub fn remember_path(conn: &Connection, exe: &str, path: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO app_paths (exe, path) VALUES (?1, ?2) ON CONFLICT(exe) DO UPDATE SET path = excluded.path",
        params![exe, path],
    )?;
    Ok(())
}

pub fn app_path(conn: &Connection, exe: &str) -> Result<Option<String>> {
    conn.query_row("SELECT path FROM app_paths WHERE exe = ?1", [exe], |r| r.get(0)).optional()
}

/// First sighting of a game: file it under distracting unless the user already chose, and relabel
/// sessions recorded under the bare exe name before Kairo knew it was a game.
pub fn adopt_game(conn: &Connection, exe: &str, name: &str) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO categories (key, category) VALUES (?1, 'distracting')",
        [format!("app:{exe}")],
    )?;
    conn.execute("UPDATE sessions SET app = ?1 WHERE exe = ?2 AND app != ?1", params![name, exe])?;
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// What is in front of the user right now.
pub struct Current {
    pub app: String,
    pub exe: String,
    pub site: Option<String>,
    /// When the user switched to this app (title changes within it don't count).
    pub since_ms: i64,
}

/// The live session, or None when idle, paused or asleep.
pub fn current_activity(conn: &Connection, now_ms: i64) -> Result<Option<Current>> {
    let mut stmt = conn.prepare(
        "SELECT app, exe, site, start_ms, end_ms, idle FROM sessions WHERE demo = 0 AND device IS NULL ORDER BY id DESC LIMIT 200",
    )?;
    let recent: Vec<(String, String, Option<String>, i64, i64, bool)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
        .collect::<Result<_>>()?;

    // The collector flushes the open session every 10s, so anything older has already ended.
    let Some((app, exe, site, start, end, idle)) = recent.first().cloned() else {
        return Ok(None);
    };
    if idle || now_ms - end > 15_000 {
        return Ok(None);
    }
    // Title changes split sessions; walk back over contiguous ones in the same app.
    let mut since = start;
    for pair in recent.windows(2) {
        let (cur, prev) = (&pair[0], &pair[1]);
        if prev.0 != app || prev.5 || cur.3 - prev.4 > 5_000 {
            break;
        }
        since = prev.3;
    }
    Ok(Some(Current { app, exe, site, since_ms: since }))
}
