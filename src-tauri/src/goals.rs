//! Daily goals (enough productive time, not too much distraction) and per-app or per-site time
//! limits, with a notification as a limit gets close and when a goal is reached or passed.

use std::collections::HashMap;
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
use crate::{classify, db};

const PRODUCTIVE_KEY: &str = "goal_productive_min";
const DISTRACTING_KEY: &str = "goal_distracting_max_min";
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
    fn is_empty(&self) -> bool {
        self.productive_min == 0 && self.distracting_max_min == 0 && self.limits.is_empty()
    }
}

pub fn load(conn: &Connection) -> Result<Goals> {
    let minutes = |key| -> Result<u32> { Ok(db::get_setting(conn, key)?.and_then(|v| v.parse().ok()).unwrap_or(0)) };
    let mut stmt = conn.prepare("SELECT key, minutes FROM limits ORDER BY key")?;
    let limits = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_>>()?;
    Ok(Goals { productive_min: minutes(PRODUCTIVE_KEY)?, distracting_max_min: minutes(DISTRACTING_KEY)?, limits })
}

/// Sets the productive target or the distracting cap; `Kind::Limit` goes through [`set_limit`].
pub fn set_goal(conn: &Connection, kind: Kind, minutes: u32) -> Result<()> {
    let key = match kind {
        Kind::Productive => PRODUCTIVE_KEY,
        Kind::Distracting => DISTRACTING_KEY,
        Kind::Limit => return Ok(()),
    };
    db::set_setting(conn, key, &minutes.to_string())
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
    label: String,
    kind: Kind,
    used_ms: i64,
    target_ms: i64,
    /// A target is met once reached; a cap or limit while it isn't passed.
    met: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayGoals {
    items: Vec<Progress>,
    /// Completed days in a row, before this one, on which every goal was met.
    streak: u32,
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
fn progress(goals: &Goals, rows: &[Row], labels: &HashMap<String, String>) -> Vec<Progress> {
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

fn rows_for(conn: &Connection, date: NaiveDate, now_ms: i64) -> Result<Vec<Row>> {
    match analysis::day_bounds(date) {
        Some((from, to)) => analysis::load_rows(conn, from, to.min(now_ms)),
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

fn fmt_dur(ms: i64) -> String {
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
                }
            }
        })
        .expect("failed to start goals thread");
}

fn check(app: &AppHandle, conn: &Connection) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let goals = load(conn)?;
    if goals.is_empty() {
        return Ok(());
    }
    let now = chrono::Utc::now().timestamp_millis();
    // Only tell someone who is at the computer.
    if db::current_activity(conn, now)?.is_none() {
        return Ok(());
    }
    let today = Local::now().date_naive();
    let day = today.to_string();
    conn.execute("DELETE FROM goal_alerts WHERE day != ?1", [&day])?;
    for p in progress(&goals, &rows_for(conn, today, now)?, &labels(conn, &goals)?) {
        let Some((level, title, body)) = alert(&p) else { continue };
        let sent: Option<i64> =
            conn.query_row("SELECT level FROM goal_alerts WHERE day = ?1 AND key = ?2", params![day, p.key], |r| r.get(0)).ok();
        // A warning never follows the "passed" notification, and each is sent once a day.
        if sent.is_some_and(|s| s >= level) {
            continue;
        }
        app.notification().builder().title(&title).body(&body).show()?;
        conn.execute("INSERT OR REPLACE INTO goal_alerts (day, key, level) VALUES (?1, ?2, ?3)", params![day, p.key, level])?;
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
        set_goal(&conn, Kind::Distracting, 60).unwrap();
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
}
