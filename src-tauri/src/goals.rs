//! Daily goals (enough productive time, not too much distraction) and per-app or per-site time
//! limits, with a notification as a limit gets close and when a goal is reached or passed; and the
//! same two goals for a whole week, Monday to Sunday.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use chrono::{Local, NaiveDate};
use rusqlite::{params, Connection, Result};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::analysis::{self, Category, Row};
use crate::{classify, db, evening, recap};

const PRODUCTIVE_KEY: &str = "goal_productive_min";
const DISTRACTING_KEY: &str = "goal_distracting_max_min";
const WEEK_PRODUCTIVE_KEY: &str = "goal_week_productive_min";
const WEEK_DISTRACTING_KEY: &str = "goal_week_distracting_max_min";
const WEEK_STREAK_LOOKBACK: u32 = 26;
const MIN_MS: i64 = 60_000;
const CHECK_EVERY: Duration = Duration::from_secs(60);
/// The "almost there" warning, as a share of a cap.
const WARN_AT: f64 = 0.8;
/// Caps this short only get the notification when they're used up.
const MIN_WARNED_CAP_MS: i64 = 10 * MIN_MS;
const STREAK_LOOKBACK_DAYS: usize = 60;
/// Alert levels, stored per goal per day so each is sent once.
const WARNED: i64 = 80;
const DONE: i64 = 100;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// At least this much productive time.
    Productive,
    /// At most this much distracting time.
    Distracting,
    /// At most this much time in one app or site.
    Limit,
}

/// What the user has set, in minutes; 0 means off.
pub struct Goals {
    pub productive_min: u32,
    pub distracting_max_min: u32,
    /// (`app:<exe>` or `site:<name>`, minutes)
    pub limits: Vec<(String, u32)>,
}

impl Goals {
    pub fn is_empty(&self) -> bool {
        self.productive_min == 0 && self.distracting_max_min == 0 && self.limits.is_empty()
    }
}

pub fn load(conn: &Connection) -> Result<Goals> {
    let minutes = |key| -> Result<u32> { Ok(db::get_setting(conn, key)?.and_then(|v| v.parse().ok()).unwrap_or(0)) };
    let mut stmt = conn.prepare("SELECT key, minutes FROM limits ORDER BY key")?;
    let limits = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_>>()?;
    Ok(Goals { productive_min: minutes(PRODUCTIVE_KEY)?, distracting_max_min: minutes(DISTRACTING_KEY)?, limits })
}

/// Sets a goal by its name: "productive" or "distracting" for each day, "week-productive" or
/// "week-distracting" for the week. False for a name it doesn't know. Limits go through
/// [`set_limit`].
pub fn set_named_goal(conn: &Connection, name: &str, minutes: u32) -> Result<bool> {
    let key = match name {
        "productive" => PRODUCTIVE_KEY,
        "distracting" => DISTRACTING_KEY,
        "week-productive" => WEEK_PRODUCTIVE_KEY,
        "week-distracting" => WEEK_DISTRACTING_KEY,
        _ => return Ok(false),
    };
    db::set_setting(conn, key, &minutes.to_string())?;
    Ok(true)
}

/// The weekly goals, in minutes; 0 means off.
pub fn load_week(conn: &Connection) -> Result<Goals> {
    let minutes = |key| -> Result<u32> { Ok(db::get_setting(conn, key)?.and_then(|v| v.parse().ok()).unwrap_or(0)) };
    Ok(Goals { productive_min: minutes(WEEK_PRODUCTIVE_KEY)?, distracting_max_min: minutes(WEEK_DISTRACTING_KEY)?, limits: Vec::new() })
}

/// The limits that block their app or site once passed, by key.
pub fn hard_limits(conn: &Connection) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare("SELECT key FROM limits WHERE hard = 1")?;
    let keys = stmt.query_map([], |r| r.get(0))?.collect();
    keys
}

/// Makes a limit block its app or site once passed, or only notify; a key without a limit is
/// left alone.
pub fn set_hard(conn: &Connection, key: &str, on: bool) -> Result<()> {
    conn.execute("UPDATE limits SET hard = ?1 WHERE key = ?2", params![on, key])?;
    Ok(())
}

/// What's past a hard limit today, by key, with when the block lifts: the next local midnight.
/// Time from every computer counts, as for the limit's notification.
pub fn hard_over(conn: &Connection, now_ms: i64) -> Result<HashMap<String, i64>> {
    let hard = hard_limits(conn)?;
    if hard.is_empty() {
        return Ok(HashMap::new());
    }
    let limits = load(conn)?.limits.into_iter().filter(|(key, _)| hard.contains(key)).collect();
    let goals = Goals { productive_min: 0, distracting_max_min: 0, limits };
    let today = Local::now().date_naive();
    let Some((_, midnight)) = analysis::day_bounds(today) else { return Ok(HashMap::new()) };
    Ok(over_limits(&goals, &rows_for(conn, today, now_ms)?).into_iter().map(|key| (key, midnight)).collect())
}

/// The keys of the limits `rows` go past.
fn over_limits(goals: &Goals, rows: &[Row]) -> Vec<String> {
    progress(goals, rows, &HashMap::new()).into_iter().filter(|p| p.kind == Kind::Limit && !p.met).map(|p| p.key).collect()
}

pub fn set_limit(conn: &Connection, key: &str, minutes: u32) -> Result<()> {
    if minutes == 0 {
        conn.execute("DELETE FROM limits WHERE key = ?1", [key])?;
    } else {
        conn.execute(
            "INSERT INTO limits (key, minutes) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET minutes = excluded.minutes",
            params![key, minutes],
        )?;
    }
    Ok(())
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    key: String,
    pub label: String,
    pub kind: Kind,
    pub used_ms: i64,
    pub target_ms: i64,
    /// A target is met once reached; a cap or limit while it isn't passed.
    pub met: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayGoals {
    pub items: Vec<Progress>,
    /// Completed days (or for weekly goals, weeks) in a row, before this one, on which every goal
    /// was met.
    pub streak: u32,
}

fn item(key: &str, label: String, kind: Kind, used_ms: i64, minutes: u32) -> Progress {
    let target_ms = minutes as i64 * MIN_MS;
    let met = match kind {
        Kind::Productive => used_ms >= target_ms,
        Kind::Distracting | Kind::Limit => used_ms <= target_ms,
    };
    Progress { key: key.to_string(), label, kind, used_ms, target_ms, met }
}

/// Progress on every goal over `rows`; `labels` names app limits with no time in `rows`.
pub fn progress(goals: &Goals, rows: &[Row], labels: &HashMap<String, String>) -> Vec<Progress> {
    let used = |matches: &dyn Fn(&Row) -> bool| rows.iter().filter(|r| !r.idle && matches(r)).map(Row::dur).sum::<i64>();
    let mut out = Vec::new();
    if goals.productive_min > 0 {
        let ms = used(&|r| r.category == Category::Productive);
        out.push(item("goal:productive", "Productive time".into(), Kind::Productive, ms, goals.productive_min));
    }
    if goals.distracting_max_min > 0 {
        let ms = used(&|r| r.category == Category::Distracting);
        out.push(item("goal:distracting", "Distracting time".into(), Kind::Distracting, ms, goals.distracting_max_min));
    }
    for (key, minutes) in &goals.limits {
        let (ms, label) = match key.split_once(':') {
            Some(("app", exe)) => {
                let label = rows.iter().find(|r| r.exe == exe).map(|r| r.app.clone());
                let label = label.or_else(|| labels.get(key).cloned()).unwrap_or_else(|| classify::display_name(exe, ""));
                (used(&|r| r.exe == exe), label)
            }
            Some(("site", site)) => (used(&|r| r.site.as_deref() == Some(site)), site.to_string()),
            _ => continue,
        };
        out.push(item(key, label, Kind::Limit, ms, *minutes));
    }
    out
}

/// The most recent name for each app with a limit, e.g. "THE FINALS" rather than "Discovery".
fn labels(conn: &Connection, goals: &Goals) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT app FROM sessions WHERE exe = ?1 ORDER BY id DESC LIMIT 1")?;
    let mut out = HashMap::new();
    for (key, _) in &goals.limits {
        if let Some(exe) = key.strip_prefix("app:") {
            if let Some(app) = stmt.query_map([exe], |r| r.get::<_, String>(0))?.next() {
                out.insert(key.clone(), app?);
            }
        }
    }
    Ok(out)
}

pub fn rows_for(conn: &Connection, date: NaiveDate, now_ms: i64) -> Result<Vec<Row>> {
    rows_of(conn, date, now_ms, analysis::ALL_COMPUTERS)
}

/// A day's rows from some computers only (see `analysis::load_rows_of`).
pub fn rows_of(conn: &Connection, date: NaiveDate, now_ms: i64, computer: &str) -> Result<Vec<Row>> {
    match analysis::day_bounds(date) {
        Some((from, to)) => analysis::load_rows_of(conn, from, to.min(now_ms), computer),
        None => Ok(Vec::new()),
    }
}

/// Days with nothing tracked (weekends away, holidays) neither count nor break the streak.
fn streak(conn: &Connection, goals: &Goals, date: NaiveDate, now_ms: i64) -> Result<u32> {
    let mut count = 0;
    let mut day = date;
    for _ in 0..STREAK_LOOKBACK_DAYS {
        let Some(prev) = day.pred_opt() else { break };
        day = prev;
        let rows = rows_for(conn, day, now_ms)?;
        if !rows.iter().any(|r| !r.idle) {
            continue;
        }
        if !progress(goals, &rows, &HashMap::new()).iter().all(|p| p.met) {
            break;
        }
        count += 1;
    }
    Ok(count)
}

pub fn for_day(conn: &Connection, date: NaiveDate, now_ms: i64) -> Result<DayGoals> {
    let goals = load(conn)?;
    if goals.is_empty() {
        return Ok(DayGoals { items: Vec::new(), streak: 0 });
    }
    let items = progress(&goals, &rows_for(conn, date, now_ms)?, &labels(conn, &goals)?);
    Ok(DayGoals { items, streak: streak(conn, &goals, date, now_ms)? })
}

/// The week from Monday `monday` up to now, from every computer.
fn week_rows(conn: &Connection, monday: NaiveDate, now_ms: i64) -> Result<Vec<Row>> {
    let from = analysis::day_bounds(monday).map_or(0, |b| b.0);
    let to = monday.checked_add_days(chrono::Days::new(6)).and_then(analysis::day_bounds).map_or(now_ms, |b| b.1);
    analysis::load_rows(conn, from, to.min(now_ms))
}

/// Progress on the weekly goals, named apart from the daily ones.
fn week_progress(goals: &Goals, rows: &[Row]) -> Vec<Progress> {
    progress(goals, rows, &HashMap::new())
        .into_iter()
        .map(|mut p| {
            (p.key, p.label) = match p.kind {
                Kind::Productive => ("goal:week-productive".into(), "Productive time this week".into()),
                _ => ("goal:week-distracting".into(), "Distracting time this week".into()),
            };
            p
        })
        .collect()
}

/// The weekly goals for the week starting `monday`, and how many weeks before it in a row met
/// them all; weeks with nothing tracked neither count nor break the streak.
pub fn for_week(conn: &Connection, monday: NaiveDate, now_ms: i64) -> Result<DayGoals> {
    let goals = load_week(conn)?;
    if goals.is_empty() {
        return Ok(DayGoals { items: Vec::new(), streak: 0 });
    }
    let items = week_progress(&goals, &week_rows(conn, monday, now_ms)?);
    let mut streak = 0;
    let mut week = monday;
    for _ in 0..WEEK_STREAK_LOOKBACK {
        let Some(earlier) = week.checked_sub_days(chrono::Days::new(7)) else { break };
        week = earlier;
        match week_met(conn, &goals, week, now_ms)? {
            None => continue,
            Some(false) => break,
            Some(true) => streak += 1,
        }
    }
    Ok(DayGoals { items, streak })
}

/// Whether the week starting `monday` met every weekly goal in `goals`; None with nothing
/// tracked that week.
pub fn week_met(conn: &Connection, goals: &Goals, monday: NaiveDate, now_ms: i64) -> Result<Option<bool>> {
    let rows = week_rows(conn, monday, now_ms)?;
    if !rows.iter().any(|r| !r.idle) {
        return Ok(None);
    }
    Ok(Some(week_progress(goals, &rows).iter().all(|p| p.met)))
}

pub fn fmt_dur(ms: i64) -> String {
    let minutes = (ms.max(0) + MIN_MS / 2) / MIN_MS;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// The notification a goal has earned, if any: (level, title, body).
fn alert(p: &Progress) -> Option<(i64, String, String)> {
    let (used, target) = (fmt_dur(p.used_ms), fmt_dur(p.target_ms));
    let left = fmt_dur((p.target_ms - p.used_ms).max(MIN_MS));
    let warn = p.target_ms >= MIN_WARNED_CAP_MS && p.used_ms as f64 >= p.target_ms as f64 * WARN_AT;
    match p.kind {
        Kind::Productive if p.met => {
            Some((DONE, format!("Goal reached: {target} of productive time"), format!("You've put in {used} of focused work today.")))
        }
        Kind::Productive => None,
        Kind::Distracting if !p.met => {
            Some((DONE, "Distracting time limit passed".into(), format!("{used} of distractions today, over your {target} limit.")))
        }
        Kind::Distracting if warn => {
            Some((WARNED, format!("{left} of distracting time left today"), format!("You've had {used} of your {target} limit.")))
        }
        Kind::Limit if !p.met => Some((DONE, format!("{}: daily limit passed", p.label), format!("{used} today, over your {target} limit."))),
        Kind::Limit if warn => Some((WARNED, format!("{}: {left} left today", p.label), format!("You've used {used} of your {target} limit."))),
        _ => None,
    }
}

/// The notification a weekly goal has earned, if any, as for a day's.
fn week_alert(p: &Progress) -> Option<(i64, String, String)> {
    let (used, target) = (fmt_dur(p.used_ms), fmt_dur(p.target_ms));
    let left = fmt_dur((p.target_ms - p.used_ms).max(MIN_MS));
    match p.kind {
        Kind::Productive if p.met => {
            Some((DONE, format!("Weekly goal reached: {target} of productive time"), format!("You've put in {used} of focused work this week.")))
        }
        Kind::Distracting if !p.met => {
            Some((DONE, "Weekly distracting time limit passed".into(), format!("{used} of distractions this week, over your {target} limit.")))
        }
        Kind::Distracting if p.used_ms as f64 >= p.target_ms as f64 * WARN_AT => {
            Some((WARNED, format!("{left} of distracting time left this week"), format!("You've had {used} of your {target} limit.")))
        }
        _ => None,
    }
}

pub fn spawn(app: AppHandle, db_path: PathBuf, paused: Arc<AtomicBool>) {
    thread::Builder::new()
        .name("kairo-goals".into())
        .spawn(move || {
            let conn = match db::open(&db_path) {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("goals: cannot open database: {e}");
                    return;
                }
            };
            loop {
                thread::sleep(CHECK_EVERY);
                if !paused.load(Ordering::Relaxed) {
                    if let Err(e) = check(&app, &conn) {
                        eprintln!("goals: {e}");
                    }
                    if let Err(e) = recap::maybe_notify(&app, &conn) {
                        eprintln!("recap: {e}");
                    }
                    if let Err(e) = recap::maybe_notify_month(&app, &conn) {
                        eprintln!("month recap: {e}");
                    }
                    if let Err(e) = evening::maybe_notify(&app, &conn) {
                        eprintln!("evening summary: {e}");
                    }
                }
            }
        })
        .expect("failed to start goals thread");
}

fn check(app: &AppHandle, conn: &Connection) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (goals, week_goals) = (load(conn)?, load_week(conn)?);
    if goals.is_empty() && week_goals.is_empty() {
        return Ok(());
    }
    let now = chrono::Utc::now().timestamp_millis();
    // Only tell someone who is at the computer.
    if db::current_activity(conn, now)?.is_none() {
        return Ok(());
    }
    let today = Local::now().date_naive();
    let monday = recap::week_start(today);
    // Each alert is remembered for its day, or for its week as "week <Monday>".
    let (day, week) = (today.to_string(), format!("week {monday}"));
    conn.execute("DELETE FROM goal_alerts WHERE day != ?1 AND day != ?2", params![day, week])?;
    let mut due = Vec::new();
    if !goals.is_empty() {
        let hard = hard_limits(conn)?;
        for p in progress(&goals, &rows_for(conn, today, now)?, &labels(conn, &goals)?) {
            let Some((level, title, mut body)) = alert(&p) else { continue };
            if level == DONE && hard.contains(&p.key) {
                body.push_str(" It's blocked for the rest of the day.");
            }
            due.push((day.clone(), p.key.clone(), (level, title, body)));
        }
    }
    if !week_goals.is_empty() {
        for p in week_progress(&week_goals, &week_rows(conn, monday, now)?) {
            due.extend(week_alert(&p).map(|a| (week.clone(), p.key.clone(), a)));
        }
    }
    for (when, key, (level, title, body)) in due {
        let sent: Option<i64> =
            conn.query_row("SELECT level FROM goal_alerts WHERE day = ?1 AND key = ?2", params![when, key], |r| r.get(0)).ok();
        // A warning never follows the "passed" notification, and each is sent once a day or week.
        if sent.is_some_and(|s| s >= level) {
            continue;
        }
        app.notification().builder().title(&title).body(&body).show()?;
        conn.execute("INSERT OR REPLACE INTO goal_alerts (day, key, level) VALUES (?1, ?2, ?3)", params![when, key, level])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(exe: &str, site: Option<&str>, category: Category, minutes: i64) -> Row {
        Row {
            app: exe.to_uppercase(),
            exe: exe.into(),
            site: site.map(Into::into),
            start: 0,
            end: minutes * MIN_MS,
            idle: false,
            category,
            app_category: category,
        }
    }

    fn goals(productive: u32, distracting: u32, limits: &[(&str, u32)]) -> Goals {
        Goals {
            productive_min: productive,
            distracting_max_min: distracting,
            limits: limits.iter().map(|(k, m)| (k.to_string(), *m)).collect(),
        }
    }

    #[test]
    fn hard_limits_keep_their_flag_and_name_what_went_over() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        set_limit(&conn, "site:youtube", 30).unwrap();
        set_limit(&conn, "app:discord", 60).unwrap();
        set_hard(&conn, "site:youtube", true).unwrap();
        // A key with no limit can't be hard.
        set_hard(&conn, "app:steam", true).unwrap();
        // Changing the minutes keeps the flag; removing the limit drops it.
        set_limit(&conn, "site:youtube", 45).unwrap();
        assert_eq!(hard_limits(&conn).unwrap(), HashSet::from(["site:youtube".to_string()]));
        set_limit(&conn, "site:youtube", 0).unwrap();
        assert!(hard_limits(&conn).unwrap().is_empty());

        use Category::*;
        let rows = vec![row("chrome", Some("youtube"), Distracting, 50), row("discord", None, Distracting, 20)];
        assert_eq!(over_limits(&goals(0, 0, &[("site:youtube", 45), ("app:discord", 60)]), &rows), ["site:youtube"]);
    }

    #[test]
    fn progress_sums_categories_apps_and_sites() {
        use Category::*;
        let rows = vec![
            row("code", None, Productive, 90),
            row("discovery", None, Distracting, 100),
            row("chrome", Some("youtube"), Distracting, 30),
            Row { idle: true, ..row("discovery", None, Distracting, 500) },
        ];
        let g = goals(120, 60, &[("app:discovery", 120), ("site:youtube", 20)]);
        let p = progress(&g, &rows, &HashMap::new());
        let got: Vec<(&str, i64, bool)> = p.iter().map(|p| (p.key.as_str(), p.used_ms / MIN_MS, p.met)).collect();
        assert_eq!(
            got,
            vec![("goal:productive", 90, false), ("goal:distracting", 130, false), ("app:discovery", 100, true), ("site:youtube", 30, false)]
        );
        assert_eq!(p[2].label, "DISCOVERY");
    }

    #[test]
    fn alerts_warn_near_a_limit_then_once_it_passes() {
        let at = |minutes| item("app:discovery", "THE FINALS".into(), Kind::Limit, minutes * MIN_MS, 120);
        assert!(alert(&at(60)).is_none());
        let (level, title, _) = alert(&at(100)).unwrap();
        assert_eq!((level, title.as_str()), (WARNED, "THE FINALS: 20m left today"));
        let (level, title, body) = alert(&at(125)).unwrap();
        assert_eq!((level, title.as_str(), body.as_str()), (DONE, "THE FINALS: daily limit passed", "2h 5m today, over your 2h limit."));
        // Too short to warn about.
        assert!(alert(&item("site:x", "x".into(), Kind::Limit, 4 * MIN_MS, 5)).is_none());
        assert_eq!(alert(&item("goal:productive", String::new(), Kind::Productive, 240 * MIN_MS, 240)).unwrap().0, DONE);
    }

    #[test]
    fn streak_skips_empty_days_and_stops_at_a_missed_one() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        set_named_goal(&conn, "distracting", 60).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        // Discord is distracting by default. Yesterday and three days ago were under the cap,
        // two days ago nothing was tracked, and four days ago went over.
        for (days_ago, minutes) in [(1, 30), (3, 20), (4, 90)] {
            let (start, _) = analysis::day_bounds(today - chrono::Days::new(days_ago)).unwrap();
            let start = start + 10 * 60 * MIN_MS;
            let row = db::NewSession { app: "Discord", exe: "discord", title: "", site: None, start_ms: start, end_ms: start + minutes * MIN_MS, idle: false, demo: false };
            db::insert_session(&conn, &row).unwrap();
        }
        let now = analysis::day_bounds(today).unwrap().1;
        assert_eq!(for_day(&conn, today, now).unwrap().streak, 2);
    }

    #[test]
    fn weekly_goals_add_up_the_week_and_keep_their_own_streak() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        assert!(set_named_goal(&conn, "week-distracting", 120).unwrap());
        assert!(!set_named_goal(&conn, "fortnight", 1).unwrap());
        let monday = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let discord = |date: NaiveDate, minutes: i64| {
            let start = analysis::day_bounds(date).unwrap().0 + 10 * 60 * MIN_MS;
            let row = db::NewSession { app: "Discord", exe: "discord", title: "", site: None, start_ms: start, end_ms: start + minutes * MIN_MS, idle: false, demo: false };
            db::insert_session(&conn, &row).unwrap();
        };
        // This week: 50 + 50 minutes, under the 2h cap. Last week under too, the one before empty,
        // and three weeks back over.
        discord(monday, 50);
        discord(monday + chrono::Days::new(3), 50);
        discord(monday - chrono::Days::new(5), 100);
        discord(monday - chrono::Days::new(19), 150);
        let now = analysis::day_bounds(monday + chrono::Days::new(6)).unwrap().1;
        let week = for_week(&conn, monday, now).unwrap();
        assert_eq!(week.items.len(), 1);
        assert_eq!((week.items[0].key.as_str(), week.items[0].used_ms, week.items[0].met), ("goal:week-distracting", 100 * MIN_MS, true));
        assert_eq!(week.streak, 1);
        // The daily goals aren't touched.
        assert!(load(&conn).unwrap().is_empty());
        let (level, title, _) = week_alert(&week.items[0]).unwrap();
        assert_eq!((level, title.as_str()), (WARNED, "20m of distracting time left this week"));
    }
}
