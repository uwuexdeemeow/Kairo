//! A week (Monday to Sunday) or a month at a glance: totals against the stretch before, each day,
//! the apps and projects that took the most time and how the goals went, plus a Sunday-evening
//! notification for the week and one early in the month for the month before.

use std::collections::HashMap;

use chrono::{Datelike, Days, Local, Months, NaiveDate, NaiveDateTime, Timelike, Weekday};
use rusqlite::{Connection, Result};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::analysis::{self, Category, Row};
use crate::goals::{self, fmt_dur, Kind};
use crate::projects::{self, ProjectTime};
use crate::{db, focus};

const SENT_KEY: &str = "recap_sent_for";
const MONTH_SENT_KEY: &str = "month_recap_sent_for";
/// Sunday evening, when the week is all but done.
const RECAP_DAY: Weekday = Weekday::Sun;
const RECAP_HOUR: u32 = 18;
/// The month's summary waits for the morning of the 1st, or the first day at the computer after.
const MONTH_RECAP_HOUR: u32 = 10;
/// After the first week, last month is old news.
const MONTH_RECAP_LAST_DAY: u32 = 7;
/// Weeks with less than this tracked aren't worth a notification.
const MIN_WEEK_ACTIVE_MS: i64 = 60 * 60 * 1000;
const MIN_MONTH_ACTIVE_MS: i64 = 5 * 60 * 60 * 1000;
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

/// A week or a month.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Period {
    start: String,
    /// The last day.
    end: String,
    totals: Totals,
    /// The stretch just before, of the same kind: last week or last month.
    previous: Totals,
    days: Vec<DayLine>,
    best_day: Option<String>,
    goal_days_met: u32,
    /// Tracked days that had goals to meet.
    goal_days: u32,
    /// For a month: its finished weeks (by their Monday) that met every weekly goal, out of those
    /// with time tracked. Always 0 for a week, whose own goals show beside it.
    goal_weeks_met: u32,
    goal_weeks: u32,
    top_apps: Vec<AppWeek>,
    top_projects: Vec<ProjectTime>,
    limits_over: Vec<LimitWeek>,
    focus_sessions: usize,
    focus_ms: i64,
}

pub fn week_start(date: NaiveDate) -> NaiveDate {
    date - Days::new(date.weekday().num_days_from_monday() as u64)
}

pub fn month_start(date: NaiveDate) -> NaiveDate {
    date.with_day(1).unwrap_or(date)
}

/// The number of days in the month starting `first`.
fn month_days(first: NaiveDate) -> u64 {
    first.checked_add_months(Months::new(1)).map_or(31, |next| (next - first).num_days() as u64)
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

pub fn week(conn: &Connection, start: NaiveDate, now_ms: i64) -> Result<Period> {
    week_of(conn, start, now_ms, analysis::ALL_COMPUTERS)
}

/// The week from Monday `start` with time from some computers only (see
/// `analysis::load_rows_of`); goals and limits still count every computer, as their alerts do.
pub fn week_of(conn: &Connection, start: NaiveDate, now_ms: i64, computer: &str) -> Result<Period> {
    period_of(conn, Span { start, len: 7, previous_start: start - Days::new(7), previous_len: 7, count_weeks: false }, now_ms, computer)
}

pub fn month(conn: &Connection, first: NaiveDate, now_ms: i64) -> Result<Period> {
    month_of(conn, first, now_ms, analysis::ALL_COMPUTERS)
}

/// The month starting `first`, against the whole month before; whose time as in [`week_of`].
pub fn month_of(conn: &Connection, first: NaiveDate, now_ms: i64, computer: &str) -> Result<Period> {
    let before = first.checked_sub_months(Months::new(1)).unwrap_or(first);
    let span = Span { start: first, len: month_days(first), previous_start: before, previous_len: month_days(before), count_weeks: true };
    period_of(conn, span, now_ms, computer)
}

/// The days a [`Period`] covers, and the stretch it compares with.
struct Span {
    start: NaiveDate,
    len: u64,
    previous_start: NaiveDate,
    previous_len: u64,
    /// Whether to count how the weeks inside it did on the weekly goals.
    count_weeks: bool,
}

fn period_of(conn: &Connection, span: Span, now_ms: i64, computer: &str) -> Result<Period> {
    let Span { start, len, previous_start, previous_len, count_weeks } = span;
    let goals = goals::load(conn)?;
    let mut totals = Totals::default();
    let mut days = Vec::new();
    let mut apps = AppTimes::new();
    let mut limits: HashMap<String, (i64, u32)> = HashMap::new();
    let (mut goal_days, mut goal_days_met) = (0, 0);

    for date in start.iter_days().take(len as usize) {
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
    for date in previous_start.iter_days().take(previous_len as usize) {
        let rows = goals::rows_of(conn, date, now_ms, computer)?;
        previous.add(&rows);
        add_apps(&mut previous_apps, &rows);
    }

    let (mut goal_weeks, mut goal_weeks_met) = (0, 0);
    let week_goals = goals::load_week(conn)?;
    if count_weeks && !week_goals.is_empty() {
        for monday in start.iter_days().take(len as usize).filter(|d| d.weekday() == Weekday::Mon) {
            // A week still under way hasn't met or missed anything yet.
            let ended = analysis::day_bounds(monday + Days::new(6)).is_some_and(|b| b.1 <= now_ms);
            if !ended {
                continue;
            }
            if let Some(met) = goals::week_met(conn, &week_goals, monday, now_ms)? {
                goal_weeks += 1;
                goal_weeks_met += u32::from(met);
            }
        }
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

    let end = start + Days::new(len - 1);
    let from = analysis::day_bounds(start).map_or(0, |b| b.0);
    let to = analysis::day_bounds(end).map_or(now_ms, |b| b.1);
    let focus = focus::blocks(conn, from, to, now_ms)?;
    let focus_ms = focus.iter().map(focus::Block::ms).sum();
    let top_projects = projects::totals(conn, from, to.min(now_ms), computer)?;

    Ok(Period {
        start: start.to_string(),
        end: end.to_string(),
        totals,
        previous,
        days,
        best_day,
        goal_days_met,
        goal_days,
        goal_weeks_met,
        goal_weeks,
        top_apps,
        top_projects,
        limits_over,
        focus_sessions: focus.len(),
        focus_ms,
    })
}

fn day_named(iso: &str, format: &str) -> String {
    NaiveDate::parse_from_str(iso, "%Y-%m-%d").map(|d| d.format(format).to_string()).unwrap_or_default()
}

/// "Up 20% on last week.", or None with nothing before to compare with.
fn change(current: i64, previous: i64, before: &str) -> Option<String> {
    if previous <= 0 {
        return None;
    }
    Some(match (current - previous) as f64 / previous as f64 {
        c if c.abs() < STEADY_SHARE => format!("About the same as {before}."),
        c if c > 0.0 => format!("Up {}% on {before}.", (c * 100.0).round()),
        c => format!("Down {}% on {before}.", (-c * 100.0).round()),
    })
}

/// The Sunday notification: (title, body).
fn message(w: &Period) -> (String, String) {
    let title = format!("Your week: {} productive", fmt_dur(w.totals.productive_ms));
    let mut parts = Vec::new();
    parts.extend(change(w.totals.productive_ms, w.previous.productive_ms, "last week"));
    if let Some(day) = &w.best_day {
        parts.push(format!("Best day: {}.", day_named(day, "%A")));
    }
    if w.goal_days > 0 {
        parts.push(format!("Goals met on {} of {} days.", w.goal_days_met, w.goal_days));
    }
    if let Some(l) = w.limits_over.first() {
        parts.push(format!("{} over your {} limit.", fmt_dur(l.over_ms), l.label));
    }
    (title, parts.join(" "))
}

/// The month's notification: (title, body).
fn month_message(m: &Period) -> (String, String) {
    let title = format!("Your {}: {} productive", day_named(&m.start, "%B"), fmt_dur(m.totals.productive_ms));
    let before = NaiveDate::parse_from_str(&m.start, "%Y-%m-%d").ok().and_then(|d| d.checked_sub_months(Months::new(1)));
    let before = before.map_or_else(|| "last month".to_string(), |d| d.format("%B").to_string());
    let mut parts = Vec::new();
    parts.extend(change(m.totals.productive_ms, m.previous.productive_ms, &before));
    if let Some(day) = &m.best_day {
        parts.push(format!("Best day: {}.", day_named(day, "%-d %B")));
    }
    if m.goal_days > 0 {
        parts.push(format!("Goals met on {} of {} days.", m.goal_days_met, m.goal_days));
    }
    if let Some(p) = m.top_projects.first() {
        parts.push(format!("Top project: {}.", p.name));
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

/// The first day of the month whose summary is due at `now`, if one is: last month's, from the
/// morning of the 1st through the first week, unless already sent (`sent_for` holds its 1st).
fn month_due(now: NaiveDateTime, sent_for: Option<&str>) -> Option<NaiveDate> {
    let today = now.date();
    if today.day() > MONTH_RECAP_LAST_DAY || (today.day() == 1 && now.hour() < MONTH_RECAP_HOUR) {
        return None;
    }
    let last = month_start(today).checked_sub_months(Months::new(1))?;
    (sent_for != Some(last.to_string().as_str())).then_some(last)
}

pub fn maybe_notify_month(app: &AppHandle, conn: &Connection) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let now = Local::now();
    let Some(first) = month_due(now.naive_local(), db::get_setting(conn, MONTH_SENT_KEY)?.as_deref()) else {
        return Ok(());
    };
    let now_ms = now.timestamp_millis();
    // Only tell someone who is at the computer.
    if db::current_activity(conn, now_ms)?.is_none() {
        return Ok(());
    }
    let m = month(conn, first, now_ms)?;
    if m.totals.active_ms >= MIN_MONTH_ACTIVE_MS {
        let (title, body) = month_message(&m);
        app.notification().builder().title(&title).body(&body).show()?;
    }
    db::set_setting(conn, MONTH_SENT_KEY, &first.to_string())?;
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
        assert_eq!((w.end.as_str(), w.goal_weeks), ("2026-10-25", 0));
    }

    #[test]
    fn month_spans_its_days_and_compares_with_the_month_before() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let date = |m, d| NaiveDate::from_ymd_opt(2026, m, d).unwrap();
        assert_eq!(month_start(date(10, 28)), date(10, 1));
        assert_eq!((month_days(date(2, 1)), month_days(date(10, 1)), month_days(date(11, 1))), (28, 31, 30));

        // Two Mondays in October whose weeks end inside it (5 and 12), the 26th's still running.
        session(&conn, "cursor", "Cursor", date(10, 6), 600);
        session(&conn, "cursor", "Cursor", date(10, 14), 120);
        session(&conn, "cursor", "Cursor", date(10, 27), 60);
        session(&conn, "cursor", "Cursor", date(9, 30), 500);
        session(&conn, "cursor", "Cursor", date(11, 1), 300);
        goals::set_named_goal(&conn, "week-productive", 300).unwrap();

        let now = analysis::day_bounds(date(10, 28)).unwrap().0;
        let m = month(&conn, date(10, 1), now).unwrap();
        assert_eq!((m.start.as_str(), m.end.as_str(), m.days.len()), ("2026-10-01", "2026-10-31", 31));
        assert_eq!(m.totals.productive_ms, 780 * MIN_MS);
        assert_eq!(m.previous.productive_ms, 500 * MIN_MS);
        assert_eq!(m.best_day.as_deref(), Some("2026-10-06"));
        assert_eq!((m.goal_weeks_met, m.goal_weeks), (1, 2));

        let (title, body) = month_message(&m);
        assert_eq!(title, "Your October: 13h productive");
        assert_eq!(body, "Up 56% on September. Best day: 6 October.");

        // January compares with December of the year before.
        let jan = month(&conn, NaiveDate::from_ymd_opt(2027, 1, 1).unwrap(), now).unwrap();
        assert_eq!((jan.end.as_str(), jan.days.len()), ("2027-01-31", 31));
    }

    #[test]
    fn month_summary_is_due_in_the_first_week_once() {
        let at = |d: &str, h| NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap().and_hms_opt(h, 0, 0).unwrap();
        let oct = NaiveDate::from_ymd_opt(2026, 10, 1);
        assert_eq!(month_due(at("2026-11-01", 9), None), None);
        assert_eq!(month_due(at("2026-11-01", 10), None), oct);
        assert_eq!(month_due(at("2026-11-04", 8), Some("2026-09-01")), oct);
        assert_eq!(month_due(at("2026-11-04", 8), Some("2026-10-01")), None);
        assert_eq!(month_due(at("2026-11-08", 12), None), None);
        assert_eq!(month_due(at("2027-01-02", 12), None), NaiveDate::from_ymd_opt(2026, 12, 1));
    }
}
