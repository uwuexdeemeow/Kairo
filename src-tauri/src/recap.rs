//! The week at a glance, Monday to Sunday: totals against the week before, each day, the apps
//! that took the most time and how the goals went, plus a Sunday-evening notification.

use std::collections::HashMap;

use chrono::{Datelike, Days, Local, NaiveDate, Timelike, Weekday};
use rusqlite::{Connection, Result};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::analysis::{self, Category, Row};
use crate::goals::{self, fmt_dur, Kind};
use crate::{db, focus};

const SENT_KEY: &str = "recap_sent_for";
/// Sunday evening, when the week is all but done.
const RECAP_DAY: Weekday = Weekday::Sun;
const RECAP_HOUR: u32 = 18;
/// Weeks with less than this tracked aren't worth a notification.
const MIN_WEEK_ACTIVE_MS: i64 = 60 * 60 * 1000;
/// Changes smaller than this read as "about the same".
const STEADY_SHARE: f64 = 0.05;
const TOP_APPS: usize = 5;

#[derive(Serialize, Default, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    active_ms: i64,
    productive_ms: i64,
    neutral_ms: i64,
    distracting_ms: i64,
}

impl Totals {
    fn add(&mut self, rows: &[Row]) {
        for r in rows.iter().filter(|r| !r.idle) {
            self.active_ms += r.dur();
            match r.category {
                Category::Productive => self.productive_ms += r.dur(),
                Category::Neutral => self.neutral_ms += r.dur(),
                Category::Distracting => self.distracting_ms += r.dur(),
            }
        }
    }

    fn merge(&mut self, other: Totals) {
        self.active_ms += other.active_ms;
        self.productive_ms += other.productive_ms;
        self.neutral_ms += other.neutral_ms;
        self.distracting_ms += other.distracting_ms;
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayLine {
    date: String,
    totals: Totals,
    /// None when no goals are set or nothing was tracked that day.
    goals_met: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppWeek {
    /// `app:<exe>`, for the app's icon.
    key: String,
    label: String,
    category: Category,
    ms: i64,
    previous_ms: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LimitWeek {
    label: String,
    over_ms: i64,
    days_over: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Week {
    start: String,
    totals: Totals,
    previous: Totals,
    days: Vec<DayLine>,
    best_day: Option<String>,
    goal_days_met: u32,
    /// Tracked days that had goals to meet.
    goal_days: u32,
    top_apps: Vec<AppWeek>,
    limits_over: Vec<LimitWeek>,
    focus_sessions: usize,
    focus_ms: i64,
}

pub fn week_start(date: NaiveDate) -> NaiveDate {
    date - Days::new(date.weekday().num_days_from_monday() as u64)
}

/// Per-app time keyed by exe: (latest label, category, ms).
type AppTimes = HashMap<String, (String, Category, i64)>;

fn add_apps(apps: &mut AppTimes, rows: &[Row]) {
    for r in rows.iter().filter(|r| !r.idle && !r.exe.is_empty()) {
        let entry = apps.entry(r.exe.clone()).or_insert((r.app.clone(), r.app_category, 0));
        entry.0.clone_from(&r.app);
        entry.2 += r.dur();
    }
}

pub fn week(conn: &Connection, start: NaiveDate, now_ms: i64) -> Result<Week> {
    week_of(conn, start, now_ms, analysis::ALL_COMPUTERS)
}

/// `week` with time from some computers only (see `analysis::load_rows_of`); goals and limits
/// still count every computer, as their alerts do.
pub fn week_of(conn: &Connection, start: NaiveDate, now_ms: i64, computer: &str) -> Result<Week> {
    let goals = goals::load(conn)?;
    let mut totals = Totals::default();
    let mut days = Vec::new();
    let mut apps = AppTimes::new();
    let mut limits: HashMap<String, (i64, u32)> = HashMap::new();
    let (mut goal_days, mut goal_days_met) = (0, 0);

    for date in start.iter_days().take(7) {
        let rows = goals::rows_of(conn, date, now_ms, computer)?;
        let mut day = Totals::default();
        day.add(&rows);
        totals.merge(day);
        add_apps(&mut apps, &rows);

        let every_computer;
        let goal_rows = if computer == analysis::ALL_COMPUTERS {
            &rows
        } else {
            every_computer = goals::rows_for(conn, date, now_ms)?;
            &every_computer
        };
        let goals_met = (day.active_ms > 0 && !goals.is_empty()).then(|| {
            let progress = goals::progress(&goals, goal_rows, &HashMap::new());
            for p in progress.iter().filter(|p| p.kind == Kind::Limit && !p.met) {
                let entry = limits.entry(p.label.clone()).or_default();
                entry.0 += p.used_ms - p.target_ms;
                entry.1 += 1;
            }
            progress.iter().all(|p| p.met)
        });
        if let Some(met) = goals_met {
            goal_days += 1;
            goal_days_met += u32::from(met);
        }
        days.push(DayLine { date: date.to_string(), totals: day, goals_met });
    }

    let mut previous = Totals::default();
    let mut previous_apps = AppTimes::new();
    for date in (start - Days::new(7)).iter_days().take(7) {
        let rows = goals::rows_of(conn, date, now_ms, computer)?;
        previous.add(&rows);
        add_apps(&mut previous_apps, &rows);
    }

    let best_day = days.iter().filter(|d| d.totals.productive_ms > 0).max_by_key(|d| d.totals.productive_ms).map(|d| d.date.clone());

    let mut top_apps: Vec<AppWeek> = apps
        .into_iter()
        .map(|(exe, (label, category, ms))| AppWeek {
            key: format!("app:{exe}"),
            label,
            category,
            ms,
            previous_ms: previous_apps.get(&exe).map_or(0, |a| a.2),
        })
        .collect();
    top_apps.sort_by_key(|a| std::cmp::Reverse(a.ms));
    top_apps.truncate(TOP_APPS);

    let mut limits_over: Vec<LimitWeek> =
        limits.into_iter().map(|(label, (over_ms, days_over))| LimitWeek { label, over_ms, days_over }).collect();
    limits_over.sort_by_key(|l| std::cmp::Reverse(l.over_ms));

    let from = analysis::day_bounds(start).map_or(0, |b| b.0);
    let to = analysis::day_bounds(start + Days::new(6)).map_or(now_ms, |b| b.1);
    let focus = focus::blocks(conn, from, to, now_ms)?;
    let focus_ms = focus.iter().map(focus::Block::ms).sum();

    Ok(Week {
        start: start.to_string(),
        totals,
        previous,
        days,
        best_day,
        goal_days_met,
        goal_days,
        top_apps,
        limits_over,
        focus_sessions: focus.len(),
        focus_ms,
    })
}

fn weekday_name(iso: &str) -> String {
    NaiveDate::parse_from_str(iso, "%Y-%m-%d").map(|d| d.format("%A").to_string()).unwrap_or_default()
}

/// The Sunday notification: (title, body).
fn message(w: &Week) -> (String, String) {
    let title = format!("Your week: {} productive", fmt_dur(w.totals.productive_ms));
    let mut parts = Vec::new();
    if w.previous.productive_ms > 0 {
        let change = (w.totals.productive_ms - w.previous.productive_ms) as f64 / w.previous.productive_ms as f64;
        parts.push(match change {
            c if c.abs() < STEADY_SHARE => "About the same as last week.".to_string(),
            c if c > 0.0 => format!("Up {}% on last week.", (c * 100.0).round()),
            c => format!("Down {}% on last week.", (-c * 100.0).round()),
        });
    }
    if let Some(day) = &w.best_day {
        parts.push(format!("Best day: {}.", weekday_name(day)));
    }
    if w.goal_days > 0 {
        parts.push(format!("Goals met on {} of {} days.", w.goal_days_met, w.goal_days));
    }
    if let Some(l) = w.limits_over.first() {
        parts.push(format!("{} over your {} limit.", fmt_dur(l.over_ms), l.label));
    }
    (title, parts.join(" "))
}

pub fn maybe_notify(app: &AppHandle, conn: &Connection) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let now = Local::now();
    if now.weekday() != RECAP_DAY || now.hour() < RECAP_HOUR {
        return Ok(());
    }
    let start = week_start(now.date_naive()).to_string();
    if db::get_setting(conn, SENT_KEY)?.as_deref() == Some(start.as_str()) {
        return Ok(());
    }
    let now_ms = now.timestamp_millis();
    // Only tell someone who is at the computer.
    if db::current_activity(conn, now_ms)?.is_none() {
        return Ok(());
    }
    let w = week(conn, week_start(now.date_naive()), now_ms)?;
    if w.totals.active_ms >= MIN_WEEK_ACTIVE_MS {
        let (title, body) = message(&w);
        app.notification().builder().title(&title).body(&body).show()?;
    }
    db::set_setting(conn, SENT_KEY, &start)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis;

    const MIN_MS: i64 = 60_000;

    fn session(conn: &Connection, exe: &str, app: &str, date: NaiveDate, minutes: i64) {
        let start = analysis::day_bounds(date).unwrap().0 + 10 * 60 * MIN_MS;
        let row = db::NewSession { app, exe, title: "", site: None, start_ms: start, end_ms: start + minutes * MIN_MS, idle: false, demo: false };
        db::insert_session(conn, &row).unwrap();
    }

    #[test]
    fn week_compares_with_the_one_before_and_tracks_limits() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let monday = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        assert_eq!(week_start(monday + Days::new(6)), monday);
        // Cursor is productive and Discord distracting by default.
        session(&conn, "cursor", "Cursor", monday, 120);
        session(&conn, "cursor", "Cursor", monday + Days::new(1), 240);
        session(&conn, "discord", "Discord", monday + Days::new(1), 90);
        session(&conn, "cursor", "Cursor", monday - Days::new(3), 300);
        goals::set_limit(&conn, "app:discord", 60).unwrap();

        let now = analysis::day_bounds(monday + Days::new(7)).unwrap().0;
        let w = week(&conn, monday, now).unwrap();
        assert_eq!(w.totals.productive_ms, 360 * MIN_MS);
        assert_eq!(w.previous.productive_ms, 300 * MIN_MS);
        assert_eq!(w.best_day.as_deref(), Some("2026-10-20"));
        assert_eq!((w.goal_days_met, w.goal_days), (1, 2));
        assert_eq!((w.limits_over[0].label.as_str(), w.limits_over[0].over_ms), ("Discord", 30 * MIN_MS));
        assert_eq!((w.top_apps[0].label.as_str(), w.top_apps[0].ms, w.top_apps[0].previous_ms), ("Cursor", 360 * MIN_MS, 300 * MIN_MS));

        let (title, body) = message(&w);
        assert_eq!(title, "Your week: 6h productive");
        assert_eq!(body, "Up 20% on last week. Best day: Tuesday. Goals met on 1 of 2 days. 30m over your Discord limit.");
    }
}
