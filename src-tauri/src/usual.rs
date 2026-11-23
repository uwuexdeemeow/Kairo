//! What you usually do now: the apps, sites and focus sessions you used around this time on this
//! weekday in most of the last few weeks, for the palette to suggest as it opens. Honest about
//! time: something is suggested only when it happened in at least `MIN_WEEKS` of the last
//! `WEEKS`, and the row says so. Read from what Chrono already recorded; nothing new is kept.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Duration, Local, NaiveDateTime, TimeZone};
use rusqlite::{Connection, Result};

/// Weeks looked back over, and how many of them something must turn up in.
pub const WEEKS: usize = 4;
pub const MIN_WEEKS: usize = 3;
/// Either side of now, on each of those days.
const HALF_WINDOW_MIN: i64 = 30;
/// Time an app or site needs in a window to count as used then: a glance doesn't.
const MIN_USE_MS: i64 = 2 * 60_000;
/// What was used this recently is what you're doing already, so it isn't suggested.
const RECENT_MIN: i64 = 15;
/// Suggestions the palette shows at most, and how many habits are kept for it to choose them from
/// (some may be apps it has no way to open).
pub const MAX: usize = 3;
const KEPT: usize = 12;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Thing {
    /// An app, by the name Chrono gives it.
    App(String),
    /// A site, by its address as Chrono keeps it ("github.com").
    Site(String),
    /// A focus session: its planned minutes and label.
    Focus(u32, Option<String>),
}

/// Something usual now, and in how many of the weeks looked at it happened.
#[derive(Clone, Debug, PartialEq)]
pub struct Habit {
    pub thing: Thing,
    pub weeks: usize,
    /// Its time on those days, for ranking: the most first.
    pub ms: i64,
}

/// What one past day's window held.
#[derive(Default, Debug)]
pub struct Window {
    pub used: HashMap<Thing, i64>,
}

/// The habits in some past windows: what turned up in at least `MIN_WEEKS` of them, leaving out
/// what's in `recent`, most weeks then most time first. Focus sessions lead, since starting one is
/// a choice in a way that opening an app is not.
pub fn habits(windows: &[Window], recent: &HashSet<Thing>) -> Vec<Habit> {
    let mut seen: HashMap<&Thing, (usize, i64)> = HashMap::new();
    for w in windows {
        for (thing, ms) in &w.used {
            let e = seen.entry(thing).or_default();
            e.0 += 1;
            e.1 += ms;
        }
    }
    let mut found: Vec<Habit> = seen.into_iter().filter(|(t, (weeks, _))| *weeks >= MIN_WEEKS && !recent.contains(t)).map(|(t, (weeks, ms))| Habit { thing: t.clone(), weeks, ms }).collect();
    let focus_first = |h: &Habit| !matches!(h.thing, Thing::Focus(..));
    found.sort_by(|a, b| focus_first(a).cmp(&focus_first(b)).then(b.weeks.cmp(&a.weeks)).then(b.ms.cmp(&a.ms)).then(format!("{:?}", a.thing).cmp(&format!("{:?}", b.thing))));
    // One focus session at most: two habits at one hour are one too many to offer.
    let mut focus = false;
    found.retain(|h| !matches!(h.thing, Thing::Focus(..)) || !std::mem::replace(&mut focus, true));
    found.truncate(KEPT);
    found
}

fn ms(t: NaiveDateTime) -> Option<i64> {
    Local.from_local_datetime(&t).earliest().map(|d| d.timestamp_millis())
}

/// What a window of time held: apps and sites with at least `MIN_USE_MS` in it, and focus sessions
/// started in it.
fn window(conn: &Connection, from: i64, to: i64) -> Result<Window> {
    let mut w = Window::default();
    for r in crate::analysis::load_rows_of(conn, from, to, crate::analysis::ALL_COMPUTERS)?.into_iter().filter(|r| !r.idle) {
        *w.used.entry(Thing::App(r.app.clone())).or_default() += r.dur();
        if let Some(site) = &r.site {
            *w.used.entry(Thing::Site(site.clone())).or_default() += r.dur();
        }
    }
    w.used.retain(|_, ms| *ms >= MIN_USE_MS);
    let mut stmt = conn.prepare("SELECT planned_ms, label FROM focus_sessions WHERE start_ms >= ?1 AND start_ms < ?2")?;
    let started = stmt.query_map([from, to], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?)))?;
    for s in started {
        let (planned, label) = s?;
        let label = label.map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty());
        // Planned length counts in the habit, so "focus 50 essay" and "focus 25" are two.
        w.used.insert(Thing::Focus((planned / 60_000).clamp(1, 600) as u32, label), planned);
    }
    Ok(w)
}

/// The habits for `now`: the same weekday in each of the last `WEEKS` weeks, the half hour either
/// side of this time.
pub fn usual(conn: &Connection, now: NaiveDateTime) -> Result<Vec<Habit>> {
    let mut windows = Vec::new();
    for k in 1..=WEEKS as i64 {
        let at = now - Duration::weeks(k);
        let (Some(from), Some(to)) = (ms(at - Duration::minutes(HALF_WINDOW_MIN)), ms(at + Duration::minutes(HALF_WINDOW_MIN))) else { continue };
        windows.push(window(conn, from, to)?);
    }
    let mut recent: HashSet<Thing> = HashSet::new();
    if let (Some(from), Some(to)) = (ms(now - Duration::minutes(RECENT_MIN)), ms(now)) {
        // Anything used at all lately, not only for two minutes.
        for r in crate::analysis::load_rows_of(conn, from, to, crate::analysis::ALL_COMPUTERS)?.into_iter().filter(|r| !r.idle) {
            recent.insert(Thing::App(r.app.clone()));
            if let Some(site) = r.site {
                recent.insert(Thing::Site(site));
            }
        }
    }
    Ok(habits(&windows, &recent))
}

/// "3 of the last 4 Mondays around 09:00": why a habit is suggested.
pub fn why(h: &Habit, now: NaiveDateTime) -> String {
    let day = now.weekday();
    let name = match day {
        chrono::Weekday::Mon => "Mondays",
        chrono::Weekday::Tue => "Tuesdays",
        chrono::Weekday::Wed => "Wednesdays",
        chrono::Weekday::Thu => "Thursdays",
        chrono::Weekday::Fri => "Fridays",
        chrono::Weekday::Sat => "Saturdays",
        chrono::Weekday::Sun => "Sundays",
    };
    // To the nearest half hour, as people say a time.
    let mins = (now.and_utc().timestamp() / 60).rem_euclid(24 * 60);
    let rounded = ((mins + 15) / 30 * 30) % (24 * 60);
    let when = crate::quick_add::clock(chrono::NaiveTime::from_hms_opt((rounded / 60) as u32, (rounded % 60) as u32, 0).unwrap_or_default());
    if h.weeks >= WEEKS {
        format!("Every {} lately, around {when}", &name[..name.len() - 1])
    } else {
        format!("{} of the last {WEEKS} {name}, around {when}", h.weeks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn app(name: &str) -> Thing {
        Thing::App(name.into())
    }

    fn win(things: &[(Thing, i64)]) -> Window {
        Window { used: things.iter().cloned().collect() }
    }

    #[test]
    fn only_what_turned_up_in_most_weeks_is_suggested() {
        let slack = (app("Slack"), 600_000);
        let code = (app("VS Code"), 1_200_000);
        let windows = [win(&[slack.clone(), code.clone()]), win(&[slack.clone(), code.clone()]), win(&[slack.clone()]), win(&[(app("Steam"), 900_000)])];
        let found = habits(&windows, &HashSet::new());
        // Slack 3 of 4; VS Code only 2; Steam once.
        assert_eq!(found.iter().map(|h| (&h.thing, h.weeks)).collect::<Vec<_>>(), vec![(&app("Slack"), 3)]);
    }

    #[test]
    fn what_you_are_doing_already_isnt_suggested() {
        let windows: Vec<Window> = (0..4).map(|_| win(&[(app("Slack"), 600_000), (app("Mail"), 300_000)])).collect();
        let recent: HashSet<Thing> = [app("Slack")].into_iter().collect();
        assert_eq!(habits(&windows, &recent).iter().map(|h| h.thing.clone()).collect::<Vec<_>>(), vec![app("Mail")]);
    }

    #[test]
    fn a_focus_session_leads_then_most_weeks_then_most_time() {
        let focus = (Thing::Focus(50, Some("essay".into())), 3_000_000);
        let other_focus = (Thing::Focus(25, None), 1_500_000);
        let windows: Vec<Window> = (0..4)
            .map(|i| win(&[focus.clone(), other_focus.clone(), (app("A"), 100_000 * (i + 1)), (app("B"), 900_000), (Thing::Site("github.com".into()), 400_000), (app("C"), 200_000)]))
            .collect();
        let found = habits(&windows, &HashSet::new());
        let things: Vec<Thing> = found.iter().map(|h| h.thing.clone()).take(MAX).collect();
        assert_eq!(things, vec![Thing::Focus(50, Some("essay".into())), app("B"), Thing::Site("github.com".into())]);
        // One focus session only.
        assert_eq!(found.iter().filter(|h| matches!(h.thing, Thing::Focus(..))).count(), 1);
    }

    #[test]
    fn the_reason_says_how_often_and_when() {
        // A Monday at 9:07 (UTC here; the function rounds what it is given).
        let now = chrono::NaiveDate::from_ymd_opt(2026, 11, 23).unwrap().and_hms_opt(9, 7, 0).unwrap();
        let h = |weeks| Habit { thing: app("Slack"), weeks, ms: 0 };
        assert_eq!(why(&h(3), now), "3 of the last 4 Mondays, around 09:00");
        assert_eq!(why(&h(4), now), "Every Monday lately, around 09:00");
        let late = chrono::NaiveDate::from_ymd_opt(2026, 11, 27).unwrap().and_hms_opt(23, 50, 0).unwrap();
        assert_eq!(why(&h(3), late), "3 of the last 4 Fridays, around 00:00");
    }

    #[test]
    fn past_weeks_are_read_from_what_chrono_recorded() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        let now = chrono::NaiveDate::from_ymd_opt(2026, 11, 23).unwrap().and_hms_opt(9, 0, 0).unwrap();
        let at = |weeks: i64, mins: i64| ms(now - Duration::weeks(weeks) + Duration::minutes(mins)).unwrap();
        let add = |app: &str, site: Option<&str>, from: i64, to: i64| {
            conn.execute("INSERT INTO sessions (app, exe, title, site, start_ms, end_ms) VALUES (?1, ?1, '', ?2, ?3, ?4)", params![app, site, from, to]).unwrap();
        };
        for w in 1..=3 {
            add("Slack", None, at(w, -5), at(w, 5));
            add("Chrome", Some("github.com"), at(w, 10), at(w, 20));
            // A glance doesn't count.
            add("Spotify", None, at(w, 0), at(w, 0) + 30_000);
            conn.execute("INSERT INTO focus_sessions (start_ms, planned_ms, end_ms, label) VALUES (?1, 3000000, ?2, 'Essay ')", params![at(w, 2), at(w, 52)]).unwrap();
        }
        // Outside the window, five weeks back, and Chrome a minute ago: none of these count.
        add("Steam", None, at(1, 90), at(1, 120));
        add("Steam", None, at(2, 90), at(2, 120));
        add("Steam", None, at(3, 90), at(3, 120));
        add("Chrome", Some("news.example"), ms(now).unwrap() - 60_000, ms(now).unwrap());
        let found = usual(&conn, now).unwrap();
        let things: Vec<Thing> = found.iter().map(|h| h.thing.clone()).collect();
        assert_eq!(things, vec![Thing::Focus(50, Some("essay".into())), app("Slack"), Thing::Site("github.com".into())]);
        // Chrome was in use a minute ago, so it isn't offered though it turned up each week.
        assert!(!things.contains(&app("Chrome")));
    }
}

