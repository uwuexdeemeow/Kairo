//! An optional evening notification with the day's totals, once a day at a set time.

use std::collections::HashMap;

use chrono::{Local, NaiveDate, NaiveDateTime, Timelike};
use rusqlite::{Connection, Result};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::analysis::{self, Category};
use crate::goals::{self, fmt_dur};
use crate::{db, focus};

const ENABLED_KEY: &str = "evening_summary_enabled";
const TIME_KEY: &str = "evening_summary_minute";
const SENT_KEY: &str = "evening_summary_sent_for";
/// 9pm, in minutes after midnight.
const DEFAULT_MINUTE: u32 = 21 * 60;
/// Days with less than this tracked aren't worth a notification.
const MIN_DAY_ACTIVE_MS: i64 = 30 * 60 * 1000;

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Off unless turned on.
    pub enabled: bool,
    /// When to send it, in minutes after midnight.
    pub minute: u32,
}

pub fn settings(conn: &Connection) -> Result<Settings> {
    let enabled = db::get_setting(conn, ENABLED_KEY)?.as_deref() == Some("1");
    let minute = db::get_setting(conn, TIME_KEY)?.and_then(|m| m.parse().ok()).filter(|m| *m < 24 * 60).unwrap_or(DEFAULT_MINUTE);
    Ok(Settings { enabled, minute })
}

pub fn set(conn: &Connection, enabled: bool, minute: u32) -> Result<()> {
    db::set_setting(conn, ENABLED_KEY, if enabled { "1" } else { "0" })?;
    db::set_setting(conn, TIME_KEY, &minute.min(24 * 60 - 1).to_string())
}

/// Whether today's summary is due at `now`: turned on, past its time, and not sent today.
fn due(now: NaiveDateTime, s: &Settings, sent_for: Option<&str>) -> bool {
    s.enabled && now.hour() * 60 + now.minute() >= s.minute && sent_for != Some(now.date().to_string().as_str())
}

/// What the summary is made of, for one day.
#[derive(Debug, Default, PartialEq)]
struct Day {
    active_ms: i64,
    productive_ms: i64,
    distracting_ms: i64,
    /// (met, set), when any goals are set.
    goals: Option<(usize, usize)>,
    focus_sessions: usize,
    focus_ms: i64,
    top_app: Option<String>,
}

/// The day `date` up to `now_ms`, from every computer, as goals count it.
fn day(conn: &Connection, date: NaiveDate, now_ms: i64) -> Result<Day> {
    let rows = goals::rows_for(conn, date, now_ms)?;
    let mut d = Day::default();
    for r in rows.iter().filter(|r| !r.idle) {
        d.active_ms += r.dur();
        match r.category {
            Category::Productive => d.productive_ms += r.dur(),
            Category::Distracting => d.distracting_ms += r.dur(),
            Category::Neutral => {}
        }
    }
    let goal_set = goals::load(conn)?;
    if !goal_set.is_empty() {
        let progress = goals::progress(&goal_set, &rows, &HashMap::new());
        d.goals = Some((progress.iter().filter(|p| p.met).count(), progress.len()));
    }
    let (from, to) = analysis::day_bounds(date).unwrap_or((now_ms, now_ms));
    let blocks = focus::blocks(conn, from, to, now_ms)?;
    d.focus_sessions = blocks.len();
    d.focus_ms = blocks.iter().map(focus::Block::ms).sum();
    d.top_app = analysis::glance(&rows).top_apps.into_iter().next().map(|(label, _)| label);
    Ok(d)
}

/// The notification: (title, body).
fn message(d: &Day) -> (String, String) {
    let title = format!("Your day: {} productive", fmt_dur(d.productive_ms));
    let mut parts = vec![format!("{} distracting.", fmt_dur(d.distracting_ms))];
    if let Some((met, set)) = d.goals {
        parts.push(format!("Goals met: {met} of {set}."));
    }
    match d.focus_sessions {
        0 => {}
        1 => parts.push(format!("Focus: 1 session, {}.", fmt_dur(d.focus_ms))),
        n => parts.push(format!("Focus: {n} sessions, {}.", fmt_dur(d.focus_ms))),
    }
    if let Some(app) = &d.top_app {
        parts.push(format!("Most time in {app}."));
    }
    (title, parts.join(" "))
}

fn show(app: &AppHandle, (title, body): (String, String)) -> tauri_plugin_notification::Result<()> {
    app.notification().builder().title(&title).body(&body).show()
}

pub fn maybe_notify(app: &AppHandle, conn: &Connection) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let now = Local::now();
    if !due(now.naive_local(), &settings(conn)?, db::get_setting(conn, SENT_KEY)?.as_deref()) {
        return Ok(());
    }
    let now_ms = now.timestamp_millis();
    // Only tell someone who is at the computer.
    if db::current_activity(conn, now_ms)?.is_none() {
        return Ok(());
    }
    let today = now.date_naive();
    let d = day(conn, today, now_ms)?;
    if d.active_ms >= MIN_DAY_ACTIVE_MS {
        show(app, message(&d))?;
    }
    db::set_setting(conn, SENT_KEY, &today.to_string())?;
    Ok(())
}

/// Today's summary so far, sent now whatever the time, or a made-up one on a day with too little
/// tracked to show.
pub fn send_test(app: &AppHandle, conn: &Connection) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let now = Local::now();
    let d = day(conn, now.date_naive(), now.timestamp_millis())?;
    let d = if d.active_ms >= MIN_DAY_ACTIVE_MS { d } else { sample() };
    show(app, message(&d))?;
    Ok(())
}

fn sample() -> Day {
    const MIN: i64 = 60_000;
    Day {
        active_ms: 420 * MIN,
        productive_ms: 310 * MIN,
        distracting_ms: 80 * MIN,
        goals: Some((2, 3)),
        focus_sessions: 2,
        focus_ms: 100 * MIN,
        top_app: Some("Visual Studio Code".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN_MS: i64 = 60_000;

    fn at(date: &str, hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    #[test]
    fn settings_default_to_off_at_nine() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        assert_eq!(settings(&conn).unwrap(), Settings { enabled: false, minute: 21 * 60 });
        set(&conn, true, 20 * 60 + 30).unwrap();
        assert_eq!(settings(&conn).unwrap(), Settings { enabled: true, minute: 20 * 60 + 30 });
        set(&conn, true, 5000).unwrap();
        assert_eq!(settings(&conn).unwrap().minute, 24 * 60 - 1);
    }

    #[test]
    fn due_once_a_day_after_its_time() {
        let on = Settings { enabled: true, minute: 21 * 60 };
        assert!(!due(at("2026-10-28", 20, 59), &on, None));
        assert!(due(at("2026-10-28", 21, 0), &on, None));
        assert!(due(at("2026-10-28", 23, 30), &on, Some("2026-10-27")));
        assert!(!due(at("2026-10-28", 23, 30), &on, Some("2026-10-28")));
        let off = Settings { enabled: false, ..on };
        assert!(!due(at("2026-10-28", 22, 0), &off, None));
    }

    #[test]
    fn summarizes_the_day() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
        let start = analysis::day_bounds(date).unwrap().0 + 9 * 60 * MIN_MS;
        // Cursor is productive and Discord distracting by default.
        for (app, exe, from, minutes) in [("Cursor", "cursor", 0, 190), ("Discord", "discord", 200, 50)] {
            let s = start + from * MIN_MS;
            let row = db::NewSession { app, exe, title: "", site: None, start_ms: s, end_ms: s + minutes * MIN_MS, idle: false, demo: false };
            db::insert_session(&conn, &row).unwrap();
        }
        goals::set_limit(&conn, "app:discord", 60).unwrap();

        let d = day(&conn, date, start + 12 * 60 * MIN_MS).unwrap();
        assert_eq!((d.productive_ms, d.distracting_ms), (190 * MIN_MS, 50 * MIN_MS));
        assert_eq!(d.goals, Some((1, 1)));
        assert_eq!(d.top_app.as_deref(), Some("Cursor"));

        let (title, body) = message(&d);
        assert_eq!(title, "Your day: 3h 10m productive");
        assert_eq!(body, "50m distracting. Goals met: 1 of 1. Most time in Cursor.");

        let (_, body) = message(&sample());
        assert_eq!(body, "1h 20m distracting. Goals met: 2 of 3. Focus: 2 sessions, 1h 40m. Most time in Visual Studio Code.");
    }
}
