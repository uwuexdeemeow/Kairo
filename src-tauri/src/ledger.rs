//! Ledger's settings: whether tasks remind, when on a due day, and the hours of the working day
//! the Plan view looks for free time in. All shared choices (`ledger:reminders`, `ledger:morning`,
//! `ledger:day_from`, `ledger:day_to`), so combined computers remind and plan alike.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;

const REMINDERS_KEY: &str = "ledger_reminders";
const MORNING_KEY: &str = "ledger_morning";
const DAY_FROM_KEY: &str = "ledger_day_from";
const DAY_TO_KEY: &str = "ledger_day_to";
pub const MORNING: &str = "09:00";
pub const DAY_FROM: &str = "09:00";
pub const DAY_TO: &str = "18:00";
/// The shortest working day the Plan view will plan in, in minutes.
const MIN_DAY_MIN: i64 = 60;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub reminders: bool,
    /// "09:00"
    pub morning: String,
    /// The working day the Plan view looks for free time in: "09:00" to "18:00".
    pub day_from: String,
    pub day_to: String,
}

/// A time of day as "HH:MM", or None when it isn't one.
fn clean_time(text: &str) -> Option<String> {
    chrono::NaiveTime::parse_from_str(text.trim(), "%H:%M").ok().map(|t| t.format("%H:%M").to_string())
}

/// Minutes into the day for an "HH:MM" time.
fn minutes(time: &str) -> Option<i64> {
    chrono::NaiveTime::parse_from_str(time, "%H:%M").ok().map(|t| chrono::Timelike::num_seconds_from_midnight(&t) as i64 / 60)
}

/// Whether a day from one time to another is one the Plan view can plan in.
fn day_ok(from: &str, to: &str) -> bool {
    matches!((minutes(from), minutes(to)), (Some(a), Some(b)) if b - a >= MIN_DAY_MIN)
}

pub fn settings(conn: &Connection) -> rusqlite::Result<Settings> {
    let reminders = db::get_setting(conn, REMINDERS_KEY)?.is_none_or(|v| v != "0");
    let morning = db::get_setting(conn, MORNING_KEY)?.and_then(|m| clean_time(&m)).unwrap_or_else(|| MORNING.into());
    let from = db::get_setting(conn, DAY_FROM_KEY)?.and_then(|m| clean_time(&m)).unwrap_or_else(|| DAY_FROM.into());
    let to = db::get_setting(conn, DAY_TO_KEY)?.and_then(|m| clean_time(&m)).unwrap_or_else(|| DAY_TO.into());
    // Two computers changing each end at different times can leave an end before its start:
    // then the usual day is used rather than none.
    let (day_from, day_to) = if day_ok(&from, &to) { (from, to) } else { (DAY_FROM.into(), DAY_TO.into()) };
    Ok(Settings { reminders, morning, day_from, day_to })
}

/// Changes them here; the caller shares them. A time that isn't one is refused.
pub fn set(conn: &Connection, reminders: bool, morning: &str, day_from: &str, day_to: &str) -> Result<Settings, String> {
    let morning = clean_time(morning).ok_or("Use a time like 09:00.")?;
    let (from, to) = (clean_time(day_from).ok_or("Use a time like 09:00.")?, clean_time(day_to).ok_or("Use a time like 18:00.")?);
    if !day_ok(&from, &to) {
        return Err("The working day needs to be at least an hour long.".into());
    }
    db::set_setting(conn, DAY_FROM_KEY, &from).map_err(|e| e.to_string())?;
    db::set_setting(conn, DAY_TO_KEY, &to).map_err(|e| e.to_string())?;
    db::set_setting(conn, REMINDERS_KEY, if reminders { "1" } else { "0" }).map_err(|e| e.to_string())?;
    db::set_setting(conn, MORNING_KEY, &morning).map_err(|e| e.to_string())?;
    settings(conn).map_err(|e| e.to_string())
}

/// Takes another computer's `ledger:<key>` choice.
pub fn apply(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    let now = settings(conn).map_err(|e| e.to_string())?;
    match key {
        "reminders" => set(conn, value == "1", &now.morning, &now.day_from, &now.day_to).map(|_| ()),
        "morning" => set(conn, now.reminders, value, &now.day_from, &now.day_to).map(|_| ()),
        // Each end alone: if the other computer hasn't caught up the pair is checked when read.
        "day_from" => clean_time(value).ok_or_else(|| "Use a time like 09:00.".to_string()).and_then(|t| db::set_setting(conn, DAY_FROM_KEY, &t).map_err(|e| e.to_string())),
        "day_to" => clean_time(value).ok_or_else(|| "Use a time like 18:00.".to_string()).and_then(|t| db::set_setting(conn, DAY_TO_KEY, &t).map_err(|e| e.to_string())),
        _ => Err("unknown Ledger setting".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        db::open(std::path::Path::new(":memory:")).unwrap()
    }

    #[test]
    fn the_working_day_is_nine_to_six_until_changed_and_must_be_an_hour_long() {
        let c = conn();
        let s = settings(&c).unwrap();
        assert_eq!((s.day_from.as_str(), s.day_to.as_str()), ("09:00", "18:00"));
        let s = set(&c, true, "09:00", "08:00", "16:30").unwrap();
        assert_eq!((s.day_from.as_str(), s.day_to.as_str()), ("08:00", "16:30"));
        assert_eq!(settings(&c).unwrap(), s);
        // Nothing changes when a day is refused.
        assert!(set(&c, true, "09:00", "10:00", "10:30").is_err());
        assert!(set(&c, true, "09:00", "18:00", "09:00").is_err());
        assert!(set(&c, true, "09:00", "soon", "18:00").is_err());
        assert_eq!(settings(&c).unwrap(), s);
        // Changing the others keeps it.
        let kept = set(&c, false, "07:15", &s.day_from, &s.day_to).unwrap();
        assert_eq!((kept.morning.as_str(), kept.day_from.as_str()), ("07:15", "08:00"));
    }
}
