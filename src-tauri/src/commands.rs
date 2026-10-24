use std::sync::atomic::Ordering;

use chrono::Local;
use rusqlite::params;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::analysis::{self, Category, DaySummary, Patterns};
use crate::{db, demo, nudge, startup, AppState, AutostartToggle, NudgeToggle};

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn local_midnight_ms() -> i64 {
    Local::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|t| t.and_local_timezone(Local).earliest())
        .map(|t| t.timestamp_millis())
        .unwrap_or_else(|| now_ms() - DAY_MS)
}

#[tauri::command]
pub fn get_today(state: State<AppState>) -> CmdResult<DaySummary> {
    let conn = state.db.lock().map_err(err)?;
    let rows = analysis::load_rows(&conn, local_midnight_ms(), now_ms()).map_err(err)?;
    Ok(analysis::day_summary(&rows))
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
    ms: i64,
}

/// Every app and site seen in the last 30 days, with its current category.
#[tauri::command]
pub fn get_categories(state: State<AppState>) -> CmdResult<Vec<CategoryEntry>> {
    let conn = state.db.lock().map_err(err)?;
    let cats = analysis::category_map(&conn).map_err(err)?;
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
            out.push(CategoryEntry { key, label, kind, category, ms });
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

    out.sort_by_key(|e| std::cmp::Reverse(e.ms));
    Ok(out)
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
