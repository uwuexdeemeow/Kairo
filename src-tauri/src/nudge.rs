//! Heads-up when you enter the hours you usually lose focus in: a Windows notification once a
//! night, plus a line on the Today screen until dismissed.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Local, Timelike};
use rusqlite::{Connection, Result};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::analysis::{self, Category, LowFocusWindow};
use crate::db;

const ENABLED_KEY: &str = "nudges_enabled";
const SENT_KEY: &str = "nudge_sent_for";
const DISMISSED_KEY: &str = "heads_up_dismissed_for";
const CHECK_EVERY: Duration = Duration::from_secs(60);
/// Patterns move slowly; recomputing the window hourly is plenty.
const RECOMPUTE_MS: i64 = 60 * 60 * 1000;
const LOOKBACK_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// Late-night windows run past midnight, so a nudge "night" starts at 5am.
const DAY_STARTS_AT_HOUR: i64 = 5;

#[derive(Default)]
pub struct WindowCache {
    computed_ms: Option<i64>,
    window: Option<LowFocusWindow>,
}

/// Shared by the nudge thread and the UI commands.
pub type SharedCache = Arc<Mutex<WindowCache>>;

#[derive(Serialize)]
pub struct HeadsUp {
    title: String,
    detail: String,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn night_of(now: DateTime<Local>) -> String {
    (now - chrono::Duration::hours(DAY_STARTS_AT_HOUR)).date_naive().to_string()
}

fn window(conn: &Connection, cache: &SharedCache) -> Option<LowFocusWindow> {
    let now = now_ms();
    let mut c = cache.lock().unwrap_or_else(|e| e.into_inner());
    if c.computed_ms.is_none_or(|t| now - t > RECOMPUTE_MS) {
        match analysis::load_rows(conn, now - LOOKBACK_MS, now) {
            Ok(rows) => {
                c.window = analysis::low_focus_window(&rows);
                c.computed_ms = Some(now);
            }
            Err(e) => eprintln!("nudge: cannot load history: {e}"),
        }
    }
    c.window.clone()
}

/// Forget the cached window, e.g. after categories or demo data change.
pub fn invalidate(cache: &SharedCache) {
    cache.lock().unwrap_or_else(|e| e.into_inner()).computed_ms = None;
}

/// The heads-up to show right now: inside the low-focus window and not dismissed tonight.
pub fn heads_up(conn: &Connection, cache: &SharedCache) -> Option<HeadsUp> {
    let w = window(conn, cache)?;
    let now = Local::now();
    if !w.hours.contains(&(now.hour() as usize)) {
        return None;
    }
    let dismissed = db::get_setting(conn, DISMISSED_KEY).ok().flatten();
    if dismissed.as_deref() == Some(night_of(now).as_str()) {
        return None;
    }
    Some(message(conn, &w))
}

/// The real message for this user's window, whatever the time; for "send a test nudge".
pub fn heads_up_ignoring_time(conn: &Connection, cache: &SharedCache) -> Option<HeadsUp> {
    window(conn, cache).map(|w| message(conn, &w))
}

/// Shown by the test button before Kairo has found a window.
pub fn sample() -> HeadsUp {
    HeadsUp {
        title: "This is a Chrono focus nudge.".into(),
        detail: "Once Chrono learns the hours you usually lose focus, it will let you know when they start.".into(),
    }
}

fn message(conn: &Connection, w: &LowFocusWindow) -> HeadsUp {
    let mut detail = format!(
        "{}% of your time {} usually goes to distractions, compared with {}% overall.",
        analysis::pct(w.share),
        w.phrase,
        analysis::pct(w.overall)
    );
    match current_distraction(conn) {
        Some(label) => detail.push_str(&format!(" {label} is open right now.")),
        None => detail.push_str(" A good moment to wrap up."),
    }
    HeadsUp { title: "You're entering your low-focus window.".into(), detail }
}

/// The site or app in front, if it counts as distracting.
pub fn current_distraction(conn: &Connection) -> Option<String> {
    let current = db::current_activity(conn, now_ms()).ok().flatten()?;
    let cats = analysis::category_map(conn).ok()?;
    (analysis::resolve(&cats, &current.exe, current.site.as_deref()) == Category::Distracting)
        .then(|| current.site.unwrap_or(current.app))
}

pub fn dismiss(conn: &Connection) -> Result<()> {
    db::set_setting(conn, DISMISSED_KEY, &night_of(Local::now()))
}

pub fn enabled(conn: &Connection) -> Result<bool> {
    Ok(db::get_setting(conn, ENABLED_KEY)?.as_deref() != Some("0"))
}

pub fn set_enabled(conn: &Connection, on: bool) -> Result<()> {
    db::set_setting(conn, ENABLED_KEY, if on { "1" } else { "0" })
}

pub fn notify(app: &AppHandle, h: &HeadsUp) -> tauri_plugin_notification::Result<()> {
    app.notification().builder().title(&h.title).body(&h.detail).show()
}

pub fn spawn(app: AppHandle, db_path: PathBuf, paused: Arc<AtomicBool>, cache: SharedCache) {
    thread::Builder::new()
        .name("kairo-nudge".into())
        .spawn(move || {
            let conn = match db::open(&db_path) {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("nudge: cannot open database: {e}");
                    return;
                }
            };
            loop {
                thread::sleep(CHECK_EVERY);
                if !paused.load(Ordering::Relaxed) {
                    if let Err(e) = check(&app, &conn, &cache) {
                        eprintln!("nudge: {e}");
                    }
                }
            }
        })
        .expect("failed to start nudge thread");
}

fn check(app: &AppHandle, conn: &Connection, cache: &SharedCache) -> std::result::Result<(), Box<dyn std::error::Error>> {
    if !enabled(conn)? {
        return Ok(());
    }
    let tonight = night_of(Local::now());
    if db::get_setting(conn, SENT_KEY)?.as_deref() == Some(tonight.as_str()) {
        return Ok(());
    }
    // Only nudge someone who is actually at the computer.
    if db::current_activity(conn, now_ms())?.is_none() {
        return Ok(());
    }
    if let Some(h) = heads_up(conn, cache) {
        notify(app, &h)?;
        db::set_setting(conn, SENT_KEY, &tonight)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn late_night_counts_as_the_previous_evening() {
        let at = |d, h| Local.with_ymd_and_hms(2026, 10, d, h, 30, 0).unwrap();
        assert_eq!(night_of(at(23, 23)), night_of(at(24, 1)));
        assert_ne!(night_of(at(23, 23)), night_of(at(24, 9)));
    }

    #[test]
    fn demo_history_yields_a_late_night_window() {
        let mut conn = db::open(std::path::Path::new(":memory:")).unwrap();
        crate::demo::seed(&mut conn, 21).unwrap();
        let cache = SharedCache::default();
        let w = window(&conn, &cache).expect("demo data plants a late-night slump");
        assert_eq!(w.phrase, "after 10pm");
        assert!(w.hours.contains(&23) && w.share > w.overall);
        assert!(heads_up_ignoring_time(&conn, &cache).unwrap().detail.contains("after 10pm"));
    }
}
