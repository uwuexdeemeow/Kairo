use std::sync::atomic::Ordering;

use chrono::Local;
use rusqlite::params;
use serde::Serialize;
use tauri::State;

use crate::analysis::{self, Category, DaySummary, Patterns};
use crate::{demo, AppState};

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
    let mut stmt = conn
        .prepare("SELECT app, start_ms, end_ms, idle FROM sessions WHERE demo = 0 ORDER BY id DESC LIMIT 200")
        .map_err(err)?;
    let recent: Vec<(String, i64, i64, bool)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .map_err(err)?
        .collect::<Result<_, _>>()
        .map_err(err)?;

    // The collector flushes the open session every 10s, so anything older has already ended.
    let live = recent.first().filter(|(_, _, end, idle)| tracking && !idle && now_ms() - end < 15_000);
    let Some((app, mut since, _, _)) = live.cloned() else {
        return Ok(Status { tracking, app: None, since_ms: None });
    };
    // Title changes split sessions; walk back over contiguous ones in the same app.
    for pair in recent.windows(2) {
        let ((_, start, _, _), (prev_app, _, prev_end, _)) = (&pair[0], &pair[1]);
        if *prev_app != app || start - prev_end > 5_000 {
            break;
        }
        since = pair[1].1;
    }
    Ok(Status { tracking, app: Some(app), since_ms: Some(since) })
}

#[tauri::command]
pub fn seed_demo_data(state: State<AppState>) -> CmdResult<usize> {
    let mut conn = state.db.lock().map_err(err)?;
    demo::seed(&mut conn, 21).map_err(err)
}

#[tauri::command]
pub fn clear_demo_data(state: State<AppState>) -> CmdResult<usize> {
    let conn = state.db.lock().map_err(err)?;
    demo::clear(&conn).map_err(err)
}
