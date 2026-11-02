//! Ledger's settings: whether tasks remind, and when on a due day. Both are shared choices
//! (`ledger:reminders`, `ledger:morning`), so combined computers remind alike.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;

const REMINDERS_KEY: &str = "ledger_reminders";
const MORNING_KEY: &str = "ledger_morning";
pub const MORNING: &str = "09:00";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Settings {
    pub reminders: bool,
    /// "09:00"
    pub morning: String,
}

/// A time of day as "HH:MM", or None when it isn't one.
fn clean_time(text: &str) -> Option<String> {
    chrono::NaiveTime::parse_from_str(text.trim(), "%H:%M").ok().map(|t| t.format("%H:%M").to_string())
}

pub fn settings(conn: &Connection) -> rusqlite::Result<Settings> {
    let reminders = db::get_setting(conn, REMINDERS_KEY)?.is_none_or(|v| v != "0");
    let morning = db::get_setting(conn, MORNING_KEY)?.and_then(|m| clean_time(&m)).unwrap_or_else(|| MORNING.into());
    Ok(Settings { reminders, morning })
}

/// Changes them here; the caller shares them. A time that isn't one is refused.
pub fn set(conn: &Connection, reminders: bool, morning: &str) -> Result<Settings, String> {
    let morning = clean_time(morning).ok_or("Use a time like 09:00.")?;
    db::set_setting(conn, REMINDERS_KEY, if reminders { "1" } else { "0" }).map_err(|e| e.to_string())?;
    db::set_setting(conn, MORNING_KEY, &morning).map_err(|e| e.to_string())?;
    settings(conn).map_err(|e| e.to_string())
}

/// Takes another computer's `ledger:<key>` choice.
pub fn apply(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    let now = settings(conn).map_err(|e| e.to_string())?;
    match key {
        "reminders" => set(conn, value == "1", &now.morning).map(|_| ()),
        "morning" => set(conn, now.reminders, value).map(|_| ()),
        _ => Err("unknown Ledger setting".into()),
    }
}
