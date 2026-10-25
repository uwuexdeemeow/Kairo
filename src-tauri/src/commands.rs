use std::collections::HashMap;
use std::sync::atomic::Ordering;

use chrono::{Local, NaiveDate, TimeZone};
use rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::analysis::{self, day_bounds, Category, DaySummary, Patterns};
use crate::privacy::{self, Mode};
use crate::{classify, db, demo, goals, nudge, recap, startup, updater, AppState, AutostartToggle, NudgeToggle};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// A `YYYY-MM-DD` local date from the UI, defaulting to today.
fn parse_day(date: Option<String>) -> CmdResult<NaiveDate> {
    match date {
        Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d").map_err(err),
        None => Ok(Local::now().date_naive()),
    }
}

/// One local day's summary; `date` is `YYYY-MM-DD` and defaults to today.
#[tauri::command]
pub fn get_day(state: State<AppState>, date: Option<String>) -> CmdResult<DaySummary> {
    let (from, to) = day_bounds(parse_day(date)?).ok_or("that date has no local midnight")?;
    let conn = state.db.lock().map_err(err)?;
    let rows = analysis::load_rows(&conn, from, to.min(now_ms())).map_err(err)?;
    Ok(analysis::day_summary(&rows))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayTotal {
    date: String,
    active_ms: i64,
    productive_ms: i64,
}

/// Time tracked on each local day from `from` to `to` (inclusive, at most 62 days), for the calendar.
#[tauri::command]
pub fn get_day_totals(state: State<AppState>, from: String, to: String) -> CmdResult<Vec<DayTotal>> {
    let (from, to) = (parse_day(Some(from))?, parse_day(Some(to))?);
    let conn = state.db.lock().map_err(err)?;
    let now = now_ms();
    let mut out = Vec::new();
    for date in from.iter_days().take_while(|d| *d <= to).take(62) {
        let Some((start, end)) = day_bounds(date) else { continue };
        if start > now {
            break;
        }
        let rows = analysis::load_rows(&conn, start, end.min(now)).map_err(err)?;
        let active = rows.iter().filter(|r| !r.idle);
        let (active_ms, productive_ms) = active.fold((0, 0), |(a, p), r| {
            (a + r.dur(), p + if r.category == Category::Productive { r.dur() } else { 0 })
        });
        if active_ms > 0 {
            out.push(DayTotal { date: date.to_string(), active_ms, productive_ms });
        }
    }
    Ok(out)
}

/// The local date of the earliest recorded session, so the day picker knows where history starts.
#[tauri::command]
pub fn get_first_day(state: State<AppState>) -> CmdResult<Option<String>> {
    let conn = state.db.lock().map_err(err)?;
    let first: Option<i64> = conn.query_row("SELECT MIN(start_ms) FROM sessions", [], |r| r.get(0)).map_err(err)?;
    Ok(first
        .and_then(|ms| Local.timestamp_millis_opt(ms).single())
        .map(|t| t.date_naive().to_string()))
}

#[tauri::command]
pub fn get_patterns(state: State<AppState>, days: u32) -> CmdResult<Patterns> {
    let conn = state.db.lock().map_err(err)?;
    let now = now_ms();
    let rows = analysis::load_rows(&conn, now - days as i64 * DAY_MS, now).map_err(err)?;
    Ok(analysis::patterns(&rows))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryEntry {
    key: String,
    label: String,
    kind: &'static str,
    category: Category,
    privacy: Mode,
    /// Daily limit in minutes; 0 means none.
    limit_min: u32,
    ms: i64,
}

/// Every app and site seen in the last 30 days, with its current category and recording rule,
/// plus anything the user stopped recording, so it can be turned back on.
#[tauri::command]
pub fn get_categories(state: State<AppState>) -> CmdResult<Vec<CategoryEntry>> {
    let conn = state.db.lock().map_err(err)?;
    let cats = analysis::category_map(&conn).map_err(err)?;
    let rules = privacy::load(&conn).map_err(err)?;
    let limits: HashMap<String, u32> = goals::load(&conn).map_err(err)?.limits.into_iter().collect();
    let since = now_ms() - 30 * DAY_MS;

    let mut out = Vec::new();
    let mut collect = |sql: &str, kind: &'static str| -> rusqlite::Result<()> {
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![since], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
        })?;
        for row in rows {
            let (id, label, ms) = row?;
            let key = format!("{kind}:{id}");
            let category = cats.get(&key).copied().unwrap_or(Category::Neutral);
            let privacy = rules.get(&key).copied().unwrap_or(Mode::Track);
            let limit_min = limits.get(&key).copied().unwrap_or(0);
            out.push(CategoryEntry { key, label, kind, category, privacy, limit_min, ms });
        }
        Ok(())
    };
    collect(
        "SELECT exe, MAX(app), SUM(end_ms - start_ms) FROM sessions
         WHERE idle = 0 AND exe != '' AND start_ms >= ?1 GROUP BY exe",
        "app",
    )
    .map_err(err)?;
    collect(
        "SELECT site, site, SUM(end_ms - start_ms) FROM sessions
         WHERE idle = 0 AND site IS NOT NULL AND start_ms >= ?1 GROUP BY site",
        "site",
    )
    .map_err(err)?;

    let mut user_rules = conn.prepare("SELECT key FROM privacy_rules WHERE user_set = 1 AND mode != 'track'").map_err(err)?;
    let keys: Vec<String> = user_rules.query_map([], |r| r.get(0)).map_err(err)?.collect::<Result<_, _>>().map_err(err)?;
    for key in keys {
        if out.iter().any(|e| e.key == key) {
            continue;
        }
        let (kind, label) = match key.split_once(':') {
            Some(("app", exe)) => ("app", classify::display_name(exe, "")),
            Some(("site", site)) => ("site", site.to_string()),
            _ => continue,
        };
        let category = cats.get(&key).copied().unwrap_or(Category::Neutral);
        let privacy = rules.get(&key).copied().unwrap_or(Mode::Track);
        let limit_min = limits.get(&key).copied().unwrap_or(0);
        out.push(CategoryEntry { key, label, kind, category, privacy, limit_min, ms: 0 });
    }

    out.sort_by_key(|e| std::cmp::Reverse(e.ms));
    Ok(out)
}

/// The Monday-to-Sunday week containing `date` (default today), against the week before.
#[tauri::command]
pub fn get_week(state: State<AppState>, date: Option<String>) -> CmdResult<recap::Week> {
    let start = recap::week_start(parse_day(date)?);
    let conn = state.db.lock().map_err(err)?;
    recap::week(&conn, start, now_ms()).map_err(err)
}

/// Progress on every goal and limit for one local day (default today), plus the streak before it.
#[tauri::command]
pub fn get_goals(state: State<AppState>, date: Option<String>) -> CmdResult<goals::DayGoals> {
    let date = parse_day(date)?;
    let conn = state.db.lock().map_err(err)?;
    goals::for_day(&conn, date, now_ms()).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalSettings {
    productive_min: u32,
    distracting_max_min: u32,
}

#[tauri::command]
pub fn get_goal_settings(state: State<AppState>) -> CmdResult<GoalSettings> {
    let conn = state.db.lock().map_err(err)?;
    let g = goals::load(&conn).map_err(err)?;
    Ok(GoalSettings { productive_min: g.productive_min, distracting_max_min: g.distracting_max_min })
}

/// `kind` is "productive" (a daily target) or "distracting" (a daily cap); 0 minutes turns it off.
#[tauri::command]
pub fn set_goal(state: State<AppState>, kind: String, minutes: u32) -> CmdResult<()> {
    let kind = match kind.as_str() {
        "productive" => goals::Kind::Productive,
        "distracting" => goals::Kind::Distracting,
        other => return Err(format!("unknown goal: {other}")),
    };
    let conn = state.db.lock().map_err(err)?;
    goals::set_goal(&conn, kind, minutes).map_err(err)
}

/// A daily limit for `app:<exe>` or `site:<name>`; 0 minutes removes it.
#[tauri::command]
pub fn set_limit(state: State<AppState>, key: String, minutes: u32) -> CmdResult<()> {
    let conn = state.db.lock().map_err(err)?;
    goals::set_limit(&conn, &key, minutes).map_err(err)
}

fn parse_mode(mode: &str) -> CmdResult<Mode> {
    Mode::parse(mode).ok_or_else(|| format!("unknown recording mode: {mode}"))
}

/// Takes effect on the collector's next tick.
#[tauri::command]
pub fn set_privacy_rule(state: State<AppState>, key: String, mode: String) -> CmdResult<()> {
    let mode = parse_mode(&mode)?;
    let conn = state.db.lock().map_err(err)?;
    privacy::set_rule(&conn, &key, mode).map_err(err)?;
    privacy::refresh(&conn, &state.privacy_rules).map_err(err)
}

/// Applies the current rule for `key` to history already recorded; returns sessions changed.
#[tauri::command]
pub fn apply_privacy_to_past(state: State<AppState>, key: String) -> CmdResult<usize> {
    let conn = state.db.lock().map_err(err)?;
    let mode = privacy::load(&conn).map_err(err)?.get(&key).copied().unwrap_or(Mode::Track);
    let changed = privacy::apply_to_past(&conn, &key, mode).map_err(err)?;
    nudge::invalidate(&state.window_cache);
    Ok(changed)
}

/// Days of history kept; 0 means forever.
#[tauri::command]
pub fn get_retention(state: State<AppState>) -> CmdResult<u32> {
    let conn = state.db.lock().map_err(err)?;
    privacy::retention_days(&conn).map_err(err)
}

/// Saves the setting and clears anything already past it; returns sessions changed.
#[tauri::command]
pub fn set_retention(state: State<AppState>, days: u32) -> CmdResult<usize> {
    let conn = state.db.lock().map_err(err)?;
    privacy::set_retention_days(&conn, days).map_err(err)?;
    let changed = privacy::prune(&conn, now_ms()).map_err(err)?;
    nudge::invalidate(&state.window_cache);
    Ok(changed)
}

/// `range` is "hour" (the last 60 minutes), "day" (`date`, default today) or "all".
#[tauri::command]
pub fn delete_history(state: State<AppState>, range: String, date: Option<String>) -> CmdResult<usize> {
    let conn = state.db.lock().map_err(err)?;
    let changed = match range.as_str() {
        "hour" => {
            let now = now_ms();
            privacy::delete_between(&conn, now - 60 * 60 * 1000, i64::MAX)
        }
        "day" => {
            let (from, to) = day_bounds(parse_day(date)?).ok_or("that date has no local midnight")?;
            privacy::delete_between(&conn, from, to)
        }
        "all" => privacy::delete_all(&conn),
        other => return Err(format!("unknown range: {other}")),
    }
    .map_err(err)?;
    nudge::invalidate(&state.window_cache);
    Ok(changed)
}

#[tauri::command]
pub fn set_category(state: State<AppState>, key: String, category: String) -> CmdResult<()> {
    if Category::parse(&category).is_none() {
        return Err(format!("unknown category: {category}"));
    }
    let conn = state.db.lock().map_err(err)?;
    conn.execute(
        "INSERT INTO categories (key, category, user_set) VALUES (?1, ?2, 1)
         ON CONFLICT(key) DO UPDATE SET category = excluded.category, user_set = 1",
        params![key, category],
    )
    .map_err(err)?;
    nudge::invalidate(&state.window_cache);
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    tracking: bool,
    /// The app in front right now and when you switched to it; None when idle or paused.
    app: Option<String>,
    since_ms: Option<i64>,
}

#[tauri::command]
pub fn get_status(state: State<AppState>) -> CmdResult<Status> {
    let tracking = !state.paused.load(Ordering::Relaxed);
    let conn = state.db.lock().map_err(err)?;
    let current = if tracking { db::current_activity(&conn, now_ms()).map_err(err)? } else { None };
    Ok(Status {
        tracking,
        app: current.as_ref().map(|c| c.app.clone()),
        since_ms: current.map(|c| c.since_ms),
    })
}

#[tauri::command]
pub fn get_heads_up(state: State<AppState>) -> CmdResult<Option<nudge::HeadsUp>> {
    let conn = state.db.lock().map_err(err)?;
    Ok(nudge::heads_up(&conn, &state.window_cache))
}

#[tauri::command]
pub fn dismiss_heads_up(state: State<AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(err)?;
    nudge::dismiss(&conn).map_err(err)
}

#[tauri::command]
pub fn get_nudges_enabled(state: State<AppState>) -> CmdResult<bool> {
    let conn = state.db.lock().map_err(err)?;
    nudge::enabled(&conn).map_err(err)
}

#[tauri::command]
pub fn set_nudges_enabled(app: AppHandle, state: State<AppState>, enabled: bool) -> CmdResult<()> {
    let conn = state.db.lock().map_err(err)?;
    nudge::set_enabled(&conn, enabled).map_err(err)?;
    // Keep the tray checkbox in sync with the in-app toggle.
    if let Some(toggle) = app.try_state::<NudgeToggle>() {
        let _ = toggle.0.set_checked(enabled);
    }
    Ok(())
}

#[tauri::command]
pub fn send_test_nudge(app: AppHandle, state: State<AppState>) -> CmdResult<()> {
    let conn = state.db.lock().map_err(err)?;
    let message = nudge::heads_up_ignoring_time(&conn, &state.window_cache).unwrap_or_else(nudge::sample);
    nudge::notify(&app, &message).map_err(err)
}

#[derive(Serialize)]
pub struct Autostart {
    available: bool,
    enabled: bool,
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> Autostart {
    Autostart { available: startup::AVAILABLE, enabled: startup::is_enabled(&app) }
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> CmdResult<()> {
    startup::set_enabled(&app, enabled)?;
    if let Some(toggle) = app.try_state::<AutostartToggle>() {
        let _ = toggle.0.set_checked(enabled);
    }
    Ok(())
}

#[tauri::command]
pub fn get_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

/// Same as the tray's "Check for updates": the result arrives as a notification.
#[tauri::command]
pub fn check_for_updates(app: AppHandle) {
    tauri::async_runtime::spawn(async move { updater::check(&app, true).await });
}

#[tauri::command]
pub fn seed_demo_data(state: State<AppState>) -> CmdResult<usize> {
    let mut conn = state.db.lock().map_err(err)?;
    nudge::invalidate(&state.window_cache);
    demo::seed(&mut conn, 21).map_err(err)
}

#[tauri::command]
pub fn clear_demo_data(state: State<AppState>) -> CmdResult<usize> {
    let conn = state.db.lock().map_err(err)?;
    nudge::invalidate(&state.window_cache);
    demo::clear(&conn).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_bounds_span_one_local_day() {
        let date = NaiveDate::from_ymd_opt(2026, 3, 29).unwrap();
        let (from, to) = day_bounds(date).unwrap();
        assert!((23..=25).contains(&((to - from) / (60 * 60 * 1000))));
        assert_eq!(Local.timestamp_millis_opt(from).unwrap().date_naive(), date);
        assert_eq!(Local.timestamp_millis_opt(to).unwrap().date_naive(), date.succ_opt().unwrap());
    }
}
