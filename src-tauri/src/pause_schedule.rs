//! Quiet hours: tracking pauses by itself at set times each week, like every night or weekends.
//! The focus thread asks [`apply`] every few seconds. Inside a window it pauses tracking until the
//! window ends, the way "pause tracking for an hour" does, so the tray, the palette and the ring
//! all show it and any of them can resume it. Resuming inside a window is respected: tracking
//! stays on until the next window. Windows may run past midnight ("10 pm to 7 am"). They stay on
//! the computer they were made on: whether to be recorded is a choice about this machine.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone};
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::AppHandle;

/// Shorter than this isn't worth a window.
pub const MIN_MINUTES: u32 = 15;
/// A window is at most this long: past it, pausing tracking by hand is what is meant.
pub const MAX_MINUTES: u32 = 16 * 60;
/// Windows at most, so the list stays short enough to read.
pub const MAX_WINDOWS: usize = 12;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    #[serde(default)]
    pub id: i64,
    /// The days it starts on, Monday in bit 0 to Sunday in bit 6.
    pub days: u8,
    /// Minutes after local midnight.
    pub start_min: u32,
    pub minutes: u32,
}

impl Window {
    fn starts_on(&self, date: NaiveDate) -> bool {
        self.days & (1 << date.weekday().num_days_from_monday()) != 0
    }

    /// When it starts and ends if it starts on `date`; None on a day the clocks skip it.
    fn on(&self, date: NaiveDate) -> Option<(DateTime<Local>, DateTime<Local>)> {
        let start = Local.from_local_datetime(&date.and_hms_opt(self.start_min / 60, self.start_min % 60, 0)?).earliest()?;
        Some((start, start + Duration::minutes(self.minutes as i64)))
    }
}

pub fn list(conn: &Connection) -> Result<Vec<Window>> {
    let mut stmt = conn.prepare("SELECT id, days, start_min, minutes FROM pause_schedule ORDER BY start_min, id")?;
    let rows = stmt.query_map([], |r| Ok(Window { id: r.get(0)?, days: r.get(1)?, start_min: r.get(2)?, minutes: r.get(3)? }))?.collect();
    rows
}

/// A window as described, trimmed to what makes sense; the error is for the user.
pub fn tidy(days: u8, start_min: u32, minutes: u32) -> std::result::Result<Window, String> {
    let days = days & 0x7f;
    if days == 0 {
        return Err("Pick at least one day.".into());
    }
    Ok(Window { id: 0, days, start_min: start_min.min(24 * 60 - 1), minutes: minutes.clamp(MIN_MINUTES, MAX_MINUTES) })
}

/// Adds a window; the inner error is for the user.
pub fn add(conn: &Connection, days: u8, start_min: u32, minutes: u32) -> Result<std::result::Result<(), String>> {
    let w = match tidy(days, start_min, minutes) {
        Ok(w) => w,
        Err(e) => return Ok(Err(e)),
    };
    if list(conn)?.len() >= MAX_WINDOWS {
        return Ok(Err(format!("That's {MAX_WINDOWS} times, the most there can be.")));
    }
    conn.execute("INSERT INTO pause_schedule (days, start_min, minutes) VALUES (?1, ?2, ?3)", params![w.days, w.start_min, w.minutes])?;
    Ok(Ok(()))
}

pub fn remove(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM pause_schedule WHERE id = ?1", [id])?;
    Ok(())
}

/// When the window `now` is in ends, in milliseconds; None outside every window. A window that
/// started yesterday and runs past midnight counts, and where windows overlap the later end wins.
pub fn active_end(windows: &[Window], now: DateTime<Local>) -> Option<i64> {
    let today = now.date_naive();
    let yesterday = today.pred_opt()?;
    windows
        .iter()
        .flat_map(|w| [today, yesterday].into_iter().filter(|d| w.starts_on(*d)).filter_map(|d| w.on(d)))
        .filter(|(start, end)| *start <= now && now < *end)
        .map(|(_, end)| end.timestamp_millis())
        .max()
}

/// What the scheduler has seen, to tell tracking resumed by hand from one that was never paused.
#[derive(Default)]
pub struct Auto {
    was_paused: bool,
    /// The end of the window tracking was resumed in by hand: it isn't paused again within it.
    skipped_end: i64,
}

#[derive(Debug, PartialEq)]
pub enum Step {
    Nothing,
    /// Pause tracking until this time, in milliseconds.
    Pause(i64),
}

impl Auto {
    /// One look at the clock: whether tracking is `paused` now and which window, if any, it is in.
    pub fn step(&mut self, paused: bool, active_end: Option<i64>) -> Step {
        // Paused a moment ago and not now, inside a window that hasn't ended: resumed by hand.
        if self.was_paused && !paused {
            if let Some(end) = active_end {
                self.skipped_end = end;
            }
        }
        self.was_paused = paused;
        match active_end {
            Some(end) if !paused && end != self.skipped_end => {
                self.was_paused = true;
                Step::Pause(end)
            }
            _ => Step::Nothing,
        }
    }
}

static AUTO: Mutex<Option<Auto>> = Mutex::new(None);

/// Pauses tracking when a window has begun. Called every few seconds by the focus thread.
pub fn apply(app: &AppHandle, conn: &Connection) {
    let Ok(windows) = list(conn) else { return };
    let Ok(mut guard) = AUTO.lock() else { return };
    let auto = guard.get_or_insert_with(Auto::default);
    // With no windows there is nothing to remember: a window made later starts fresh.
    if windows.is_empty() {
        *auto = Auto::default();
        return;
    }
    if let Step::Pause(until) = auto.step(crate::is_paused(app), active_end(&windows, Local::now())) {
        crate::set_paused(app, true, Some(until));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use chrono::NaiveDateTime;

    fn at(s: &str) -> DateTime<Local> {
        Local.from_local_datetime(&NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()).earliest().unwrap()
    }

    // 2026-11-02 is a Monday.
    const NIGHTS: Window = Window { id: 1, days: 0x7f, start_min: 22 * 60, minutes: 9 * 60 };
    const WEEKEND_DAY: Window = Window { id: 2, days: 0x60, start_min: 9 * 60, minutes: 4 * 60 };

    #[test]
    fn a_window_that_runs_past_midnight_counts_after_it() {
        let end = at("2026-11-04 07:00").timestamp_millis();
        assert_eq!(active_end(&[NIGHTS], at("2026-11-03 23:30")), Some(at("2026-11-04 07:00").timestamp_millis()));
        // After midnight it is still the one that began the evening before.
        assert_eq!(active_end(&[NIGHTS], at("2026-11-04 02:00")), Some(end));
        assert_eq!(active_end(&[NIGHTS], at("2026-11-04 06:59")), Some(end));
        // The end itself is outside.
        assert_eq!(active_end(&[NIGHTS], at("2026-11-04 07:00")), None);
        assert_eq!(active_end(&[NIGHTS], at("2026-11-04 12:00")), None);
        assert_eq!(active_end(&[NIGHTS], at("2026-11-03 21:59")), None);
    }

    #[test]
    fn only_the_days_it_starts_on_count() {
        // Sunday 22:00 is a night; Monday is not a weekend day.
        assert!(active_end(&[WEEKEND_DAY], at("2026-11-07 10:00")).is_some());
        assert!(active_end(&[WEEKEND_DAY], at("2026-11-02 10:00")).is_none());
        // A Friday-night window carries over into Saturday morning.
        let fri = Window { id: 3, days: 1 << 4, start_min: 23 * 60, minutes: 4 * 60 };
        assert!(active_end(&[fri.clone()], at("2026-11-07 01:00")).is_some());
        assert!(active_end(&[fri], at("2026-11-08 01:00")).is_none());
    }

    #[test]
    fn where_windows_overlap_the_later_end_wins() {
        let early = Window { id: 1, days: 0x7f, start_min: 20 * 60, minutes: 120 };
        let late = Window { id: 2, days: 0x7f, start_min: 21 * 60, minutes: 240 };
        assert_eq!(active_end(&[early, late], at("2026-11-03 21:30")), Some(at("2026-11-04 01:00").timestamp_millis()));
    }

    #[test]
    fn tracking_pauses_when_a_window_begins_and_not_before() {
        let mut auto = Auto::default();
        assert_eq!(auto.step(false, None), Step::Nothing);
        assert_eq!(auto.step(false, Some(900)), Step::Pause(900));
        // Now paused, the next looks leave it be.
        assert_eq!(auto.step(true, Some(900)), Step::Nothing);
        // The pause ended with the window.
        assert_eq!(auto.step(false, None), Step::Nothing);
        // The next night starts again.
        assert_eq!(auto.step(false, Some(1900)), Step::Pause(1900));
    }

    #[test]
    fn tracking_resumed_by_hand_stays_on_until_the_next_window() {
        let mut auto = Auto::default();
        assert_eq!(auto.step(false, Some(900)), Step::Pause(900));
        assert_eq!(auto.step(true, Some(900)), Step::Nothing);
        // Resumed by hand inside the window: not paused again within it.
        assert_eq!(auto.step(false, Some(900)), Step::Nothing);
        assert_eq!(auto.step(false, Some(900)), Step::Nothing);
        assert_eq!(auto.step(false, None), Step::Nothing);
        // The next window pauses as usual.
        assert_eq!(auto.step(false, Some(1900)), Step::Pause(1900));
    }

    #[test]
    fn a_pause_by_hand_in_a_window_and_its_resume_are_left_alone_too() {
        let mut auto = Auto::default();
        // Paused by hand before the window began, and resumed inside it.
        assert_eq!(auto.step(true, None), Step::Nothing);
        assert_eq!(auto.step(true, Some(900)), Step::Nothing);
        assert_eq!(auto.step(false, Some(900)), Step::Nothing);
    }

    #[test]
    fn windows_are_kept_short_and_need_a_day() {
        assert_eq!(tidy(0, 600, 60), Err("Pick at least one day.".into()));
        assert_eq!(tidy(0x80, 600, 60), Err("Pick at least one day.".into()));
        let w = tidy(0x7f, 99_999, 5).unwrap();
        assert_eq!((w.start_min, w.minutes), (24 * 60 - 1, MIN_MINUTES));
        assert_eq!(tidy(1, 0, 99_999).unwrap().minutes, MAX_MINUTES);
    }

    #[test]
    fn windows_are_saved_in_order_up_to_a_most() {
        let c = db::open(std::path::Path::new(":memory:")).unwrap();
        add(&c, 0x1f, 22 * 60, 480).unwrap().unwrap();
        add(&c, 0x60, 9 * 60, 240).unwrap().unwrap();
        assert_eq!(list(&c).unwrap().iter().map(|w| w.start_min).collect::<Vec<_>>(), vec![9 * 60, 22 * 60]);
        assert_eq!(add(&c, 0, 0, 60).unwrap(), Err("Pick at least one day.".into()));
        for _ in 0..MAX_WINDOWS {
            let _ = add(&c, 1, 0, 60).unwrap();
        }
        assert!(add(&c, 1, 0, 60).unwrap().is_err());
        let id = list(&c).unwrap()[0].id;
        remove(&c, id).unwrap();
        assert_eq!(list(&c).unwrap().len(), MAX_WINDOWS - 1);
    }
}
