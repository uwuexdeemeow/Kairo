//! The cards above the palette's results while an item is selected: how a day went, time in an
//! app or site over a period, and what a focus session would do. Read from the database each
//! time an item asks for one.

use std::collections::HashMap;

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Timelike};
use rusqlite::{Connection, Result};
use serde::Serialize;

use super::search::{fmt_dur, Period, Preview, Subject};
use super::{day_glance, now_ms, AppEntry, DayGlance};
use crate::analysis::{self, Category, Row};
use crate::{blocking, db, focus, goals, platform};

const HOUR_MS: i64 = 60 * 60_000;
const DAY_MS: i64 = 24 * HOUR_MS;
/// What can be asked about: anything with time over this long a stretch.
const SUBJECTS_FOR_MS: i64 = 30 * DAY_MS;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Card {
    Day(DayGlance),
    Answer(Answer),
    Moment(MomentCard),
    Focus(FocusPlan),
}

pub fn card(conn: &Connection, preview: &Preview) -> Card {
    match preview {
        Preview::Moment(at) => Card::Moment(moment(conn, *at, super::moment_titles(conn).unwrap_or(false))),
        Preview::Day(day) => Card::Day(day_glance(conn, *day)),
        Preview::Answer(subject, period, against) => Card::Answer(answer(conn, subject, *period, *against)),
        Preview::Focus(minutes, label) => Card::Focus(focus_plan(conn, *minutes, label.as_deref())),
    }
}

// ---------- what can be asked about ----------

/// Apps and sites with time over the last 30 days, most used first.
pub fn subjects(conn: &Connection, apps: &[AppEntry]) -> Result<Vec<Subject>> {
    let cats = analysis::category_map(conn)?;
    let mut stmt = conn.prepare(
        "SELECT app, exe, site, SUM(end_ms - start_ms) FROM sessions
         WHERE idle = 0 AND start_ms >= ?1 GROUP BY app, exe, site",
    )?;
    let rows = stmt.query_map([now_ms() - SUBJECTS_FOR_MS], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, i64>(3)?)))?;
    let mut found: HashMap<String, Subject> = HashMap::new();
    for row in rows {
        let (app, exe, site, ms) = row?;
        let category = analysis::resolve(&cats, &exe, None);
        let key = format!("app:{exe}");
        found.entry(key.clone()).or_insert_with(|| Subject { name: app, key, category, icon: None, used_ms: 0 }).used_ms += ms;
        if let Some(site) = site {
            let category = analysis::resolve(&cats, &exe, Some(&site));
            let key = format!("site:{site}");
            found.entry(key.clone()).or_insert_with(|| Subject { name: site, key, category, icon: None, used_ms: 0 }).used_ms += ms;
        }
    }
    let mut subjects: Vec<Subject> = found.into_values().collect();
    for s in subjects.iter_mut() {
        let Some(exe) = s.key.strip_prefix("app:") else { continue };
        // The installed entry it opens from, else the program Kairo saw it run as.
        s.icon = apps.iter().find(|a| a.name.eq_ignore_ascii_case(&s.name)).map(|a| a.path.clone()).or_else(|| db::app_path(conn, exe).ok().flatten());
    }
    // Projects can be asked about too ("kairo this week"), by their time across apps.
    for p in crate::projects::totals(conn, now_ms() - SUBJECTS_FOR_MS, now_ms(), crate::analysis::ALL_COMPUTERS)? {
        subjects.push(Subject { name: p.name, key: format!("project:{}", p.id), category: Category::Neutral, icon: None, used_ms: p.ms });
    }
    subjects.sort_by_key(|s| std::cmp::Reverse(s.used_ms));
    Ok(subjects)
}

/// Daily limits by key, in milliseconds.
pub fn limits(conn: &Connection) -> HashMap<String, i64> {
    goals::load(conn).map(|g| g.limits.into_iter().map(|(key, minutes)| (key, minutes as i64 * 60_000)).collect()).unwrap_or_default()
}

// ---------- answers ----------

/// One bar of an answer's chart: a day, or an hour of a single day.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bar {
    /// Shown under the bar; empty for most hours.
    pub label: String,
    pub ms: i64,
    /// Over the daily limit.
    pub over: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    /// "THE FINALS this week".
    pub title: String,
    /// What to draw its icon from, as for an app in the results.
    pub icon: Option<String>,
    /// "Mon 19 – Sun 25 October · distracting".
    pub range: String,
    pub category: Category,
    pub total_ms: i64,
    /// Against the same stretch of the period before: "−1h 30m vs last week".
    pub change: Option<String>,
    /// Whether that change is for the better: less of a distraction, more of real work.
    pub better: Option<bool>,
    pub bars: Vec<Bar>,
    /// The daily limit, when the bars are days and there is one.
    pub limit_ms: Option<i64>,
    /// A line on what stands out.
    pub note: String,
    /// The answer as one line, for copying.
    pub text: String,
}

fn bounds(day: NaiveDate) -> (i64, i64) {
    analysis::day_bounds(day).unwrap_or((0, 0))
}

/// Time in rows within [from, to).
fn within(rows: &[Row], from: i64, to: i64) -> i64 {
    rows.iter().map(|r| (r.end.min(to) - r.start.max(from)).max(0)).sum()
}

/// "Wednesday", "Wednesday and Friday", "Monday, Wednesday and Friday".
fn list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Which part of the day most of the time fell in, when one clearly holds most of it.
fn part_of_day(rows: &[Row], total: i64) -> Option<&'static str> {
    let mut parts = [0i64; 4];
    for r in rows {
        let hour = chrono::DateTime::from_timestamp_millis(r.start)?.with_timezone(&Local).hour();
        let part = match hour {
            5..=11 => 0,
            12..=16 => 1,
            17..=21 => 2,
            _ => 3,
        };
        parts[part] += r.dur();
    }
    let (i, &most) = parts.iter().enumerate().max_by_key(|(_, &ms)| ms)?;
    (total > 0 && most * 10 >= total * 6).then_some(["in the morning", "in the afternoon", "in the evening", "at night"][i])
}

/// `against` is the period to compare with when it isn't the one just before.
pub fn answer(conn: &Connection, subject: &Subject, period: Period, against: Option<Period>) -> Answer {
    let now = now_ms();
    let today = Local::now().date_naive();
    let mine = |r: &Row| {
        !r.idle
            && match subject.key.split_once(':') {
                Some(("app", exe)) => r.exe == exe,
                Some(("site", site)) => r.site.as_deref() == Some(site),
                _ => false,
            }
    };
    // A project's time comes from window titles, which app and site rows don't carry.
    let project = subject.key.strip_prefix("project:").and_then(|id| id.parse::<i64>().ok());
    let load = |from: i64, to: i64| -> Vec<Row> {
        match project {
            Some(id) => crate::projects::rows_of(conn, id, from, to).unwrap_or_default(),
            None => analysis::load_rows(conn, from, to).unwrap_or_default().into_iter().filter(mine).collect(),
        }
    };

    let (first, last) = period.days(today);
    let (from, to) = (bounds(first).0, bounds(last).1);
    let rows = load(from, to.min(now));
    let total = within(&rows, from, to);

    // Against the period before, up to the same point when this one is still going.
    let (before_first, before_last) = against.map_or_else(|| period.previous_days(today), |a| a.days(today));
    let versus = against.map_or(period.before(), Period::label);
    let (before_from, before_to) = (bounds(before_first).0, bounds(before_last).1);
    let before_to = if against.is_none() && to > now { (before_from + (now - from)).min(before_to) } else { before_to };
    let before = within(&load(before_from, before_to), before_from, before_to);

    let limit = limits(conn).get(&subject.key).copied();
    let hourly = first == last;
    let yearly = period == Period::ThisYear;
    let daily_labels = matches!(period, Period::ThisWeek | Period::LastWeek | Period::Last7Days);
    let mut bars = Vec::new();
    if hourly {
        for h in 0..24 {
            let start = from + h * HOUR_MS;
            let label = match h {
                0 => "12a".to_string(),
                6 => "6a".to_string(),
                12 => "12p".to_string(),
                18 => "6p".to_string(),
                _ => String::new(),
            };
            bars.push(Bar { label, ms: within(&rows, start, start + HOUR_MS), over: false });
        }
    } else if yearly {
        // A bar a month: a year of days is too many to draw.
        for month in 1..=12u32 {
            let start = NaiveDate::from_ymd_opt(first.year(), month, 1).unwrap_or(first);
            let next = if month == 12 { NaiveDate::from_ymd_opt(first.year() + 1, 1, 1) } else { NaiveDate::from_ymd_opt(first.year(), month + 1, 1) }.unwrap_or(last);
            let (a, b) = (bounds(start).0, bounds(next).0);
            let label = if month % 3 == 1 { start.format("%b").to_string() } else { String::new() };
            bars.push(Bar { label, ms: within(&rows, a, b), over: false });
        }
    } else {
        let mut day = first;
        while day <= last {
            let (a, b) = bounds(day);
            let ms = within(&rows, a, b);
            let label = if daily_labels {
                day.format("%a").to_string()
            } else if day.day() == 1 || day.day() % 7 == 1 {
                day.day().to_string()
            } else {
                String::new()
            };
            bars.push(Bar { label, ms, over: limit.is_some_and(|l| ms > l) });
            day += Duration::days(1);
        }
    }

    let change = (total > 0 || before > 0).then(|| {
        let delta = total - before;
        match delta {
            0 => format!("Same as {}", versus),
            d if d > 0 => format!("+{} vs {}", fmt_dur(d), versus),
            d => format!("−{} vs {}", fmt_dur(-d), versus),
        }
    });
    let better = match (subject.category, total - before) {
        (_, 0) => None,
        _ if total == 0 && before == 0 => None,
        (Category::Distracting, d) => Some(d < 0),
        (Category::Productive, d) => Some(d > 0),
        (Category::Neutral, _) => None,
    };

    let mut note = Vec::new();
    if total == 0 {
        note.push(format!("Nothing recorded for it {}.", period.label()));
    } else if hourly {
        if let Some(l) = limit {
            note.push(if total > l {
                format!("Over its {} daily limit by {}.", fmt_dur(l), fmt_dur(total - l))
            } else {
                format!("{} left of its {} daily limit.", fmt_dur(l - total), fmt_dur(l))
            });
        }
    } else {
        let name = |i: usize| {
            if yearly {
                return NaiveDate::from_ymd_opt(first.year(), i as u32 + 1, 1).unwrap_or(first).format("%B").to_string();
            }
            let d = first + Duration::days(i as i64);
            if daily_labels { d.format("%A").to_string() } else { d.format("%-d %b").to_string() }
        };
        // A daily limit means nothing against months.
        match limit.filter(|_| !yearly) {
            Some(l) => {
                let over: Vec<String> = bars.iter().enumerate().filter(|(_, b)| b.over).map(|(i, _)| name(i)).collect();
                note.push(match over.len() {
                    0 => format!("Within its {} daily limit every day.", fmt_dur(l)),
                    n if n > 4 => format!("Over its {} daily limit on {n} days.", fmt_dur(l)),
                    _ => format!("Over its {} daily limit on {}.", fmt_dur(l), list(&over)),
                });
            }
            None => {
                if let Some((i, bar)) = bars.iter().enumerate().max_by_key(|(_, b)| b.ms) {
                    note.push(format!("Busiest on {}, with {}.", name(i), fmt_dur(bar.ms)));
                }
            }
        }
    }
    if let Some(part) = part_of_day(&rows, total) {
        note.push(format!("Most of it lands {part}."));
    }

    let range = match period {
        Period::Today | Period::Yesterday => first.format("%A %-d %B").to_string(),
        Period::ThisYear => first.format("%Y").to_string(),
        Period::ThisWeek | Period::LastWeek | Period::Last7Days | Period::Last30Days if first.month() == last.month() => format!("{} – {}", first.format("%a %-d"), last.format("%a %-d %B")),
        Period::ThisWeek | Period::LastWeek | Period::Last7Days | Period::Last30Days => format!("{} – {}", first.format("%a %-d %b"), last.format("%a %-d %b")),
        Period::ThisMonth | Period::LastMonth => first.format("%B %Y").to_string(),
    };
    let category = match subject.category {
        _ if project.is_some() => "project",
        Category::Productive => "productive",
        Category::Neutral => "neutral",
        Category::Distracting => "distracting",
    };
    let title = format!("{} {}", subject.name, period.label());
    let text = match &change {
        Some(c) => format!("{title}: {} ({})", fmt_dur(total), c.to_lowercase()),
        None => format!("{title}: {}", fmt_dur(total)),
    };
    Answer {
        title,
        icon: subject.icon.clone(),
        range: format!("{range} · {category}"),
        category: subject.category,
        total_ms: total,
        change,
        better,
        bars,
        limit_ms: limit.filter(|_| !hourly && !yearly),
        note: note.join(" "),
        text,
    }
}

// ---------- focus sessions ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusPlan {
    /// "Start a 50-minute focus session".
    pub title: String,
    /// "Labelled “essay” · ends at 4:20 pm".
    pub sub: String,
    /// What gets blocked: "5 apps, 6 sites", or "Off".
    pub blocking: String,
    /// What happens to game launchers: "Steam will close".
    pub launchers: String,
    pub strict: String,
    /// A session is running, which this one would end.
    pub replaces: bool,
}

fn count(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

pub fn focus_plan(conn: &Connection, minutes: u32, label: Option<&str>) -> FocusPlan {
    let ends = (Local::now() + Duration::minutes(minutes as i64)).format("%-I:%M %P");
    let sub = match label {
        Some(l) => format!("Labelled “{l}” · ends at {ends}"),
        None => format!("Ends at {ends}"),
    };
    let settings = blocking::settings(conn).ok();
    let list = crate::lists::for_session(conn, label).ok().flatten().and_then(|name| Some((crate::lists::keys_of(conn, &name).ok()??, name)));
    let blocking = match (&settings, &list) {
        (Some(s), Some((keys, name))) if s.enabled => {
            let apps = keys.iter().filter(|k| k.starts_with("app:")).count();
            format!("Your “{name}” list: {}, {}", count(apps, "app"), count(keys.len() - apps, "site"))
        }
        (Some(s), None) if s.enabled => {
            let rules = blocking::rules(conn).unwrap_or_default();
            let cats = analysis::category_map(conn).unwrap_or_default();
            let mut keys: Vec<&String> = cats.keys().chain(rules.keys()).collect();
            keys.sort();
            keys.dedup();
            let (mut apps, mut sites) = (0, 0);
            for key in keys {
                match key.split_once(':') {
                    Some(("app", exe)) if blocking::decide(None, &rules, &cats, Some(exe), None) => apps += 1,
                    Some(("site", site)) if blocking::decide(None, &rules, &cats, None, Some(site)) => sites += 1,
                    _ => {}
                }
            }
            format!("{}, {}", count(apps, "app"), count(sites, "site"))
        }
        _ => "Off".into(),
    };
    let launchers = match &settings {
        Some(s) if s.launchers => {
            let running = platform::processes().into_iter().find_map(|(_, file)| {
                let exe = crate::classify::exe_stem(&file);
                blocking::LAUNCHERS.iter().find(|(stem, _)| *stem == exe).map(|&(_, name)| name)
            });
            running.map_or_else(|| "Closed if opened".to_string(), |name| format!("{name} will close"))
        }
        _ => "Left open".into(),
    };
    FocusPlan {
        title: format!("Start a {minutes}-minute focus session"),
        sub,
        blocking,
        launchers,
        strict: if settings.is_some_and(|s| s.strict) { "On".into() } else { "Off".into() },
        replaces: focus::active(conn).ok().flatten().is_some(),
    }
}

// ---------- a moment in the past ----------

/// One stretch on a moment's card: what was on screen, when, and for how long.
#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MomentLine {
    /// "2:15 pm – 2:40 pm".
    pub when: String,
    /// The app or site, or "Away".
    pub what: String,
    /// "25m", with the window title after it when titles are shown.
    pub detail: String,
    /// The stretch that holds the moment asked about.
    pub here: bool,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MomentCard {
    /// "Yesterday at 3:00 pm".
    pub title: String,
    /// "In a focus session · In a meeting"; empty for neither.
    pub sub: String,
    /// The stretch around the moment, with up to two before and two after.
    pub lines: Vec<MomentLine>,
    pub note: String,
}

/// How far either side of a moment is looked at, for its neighbours.
const AROUND_MS: i64 = 2 * 60 * 60_000;
/// A neighbour shorter than this is a glance, not something that was being done.
const GLANCE_MS: i64 = 60_000;

struct Stretch {
    what: String,
    start: i64,
    end: i64,
    away: bool,
    /// Time by window title, kept only when titles are shown.
    titles: HashMap<String, i64>,
}

/// What was on screen around `at`, from the same history the days are drawn from, so ignored apps
/// are absent and hidden titles empty. Window titles are left out unless `titles`.
pub fn moment(conn: &Connection, at: NaiveDateTime, titles: bool) -> MomentCard {
    let at_ms = Local.from_local_datetime(&at).earliest().map_or(0, |d| d.timestamp_millis());
    let today = Local::now().date_naive();
    let day = match (today - at.date()).num_days() {
        0 => "Today".to_string(),
        1 => "Yesterday".to_string(),
        _ => at.format("%A %-d %B").to_string(),
    };
    let clock = |ms: i64| chrono::DateTime::from_timestamp_millis(ms).map_or_else(String::new, |d| d.with_timezone(&Local).format("%-I:%M %P").to_string());

    let mut stretches: Vec<Stretch> = Vec::new();
    let query = conn.prepare("SELECT app, site, title, idle, start_ms, end_ms FROM sessions WHERE end_ms > ?1 AND start_ms < ?2 ORDER BY start_ms");
    if let Ok(mut stmt) = query {
        let rows = stmt.query_map([at_ms - AROUND_MS, at_ms + AROUND_MS], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?, r.get::<_, bool>(3)?, r.get::<_, i64>(4)?, r.get::<_, i64>(5)?))
        });
        for (app, site, title, idle, start, end) in rows.into_iter().flatten().flatten() {
            let what = if idle { "Away".to_string() } else { site.unwrap_or(app) };
            match stretches.last_mut() {
                // The same thing again, give or take a moment: one stretch.
                Some(last) if last.what == what && start - last.end <= GLANCE_MS => {
                    last.end = last.end.max(end);
                    if titles && !title.is_empty() {
                        *last.titles.entry(title).or_default() += end - start;
                    }
                }
                _ => {
                    let mut kept = HashMap::new();
                    if titles && !title.is_empty() {
                        kept.insert(title, end - start);
                    }
                    stretches.push(Stretch { what, start, end, away: idle, titles: kept });
                }
            }
        }
    }
    let here = stretches.iter().position(|s| s.start <= at_ms && at_ms < s.end);
    // Brief glances elsewhere are left out of the neighbours.
    let worth = |i: usize, s: &Stretch| Some(i) == here || s.end - s.start > GLANCE_MS;
    let before: Vec<usize> = stretches.iter().enumerate().filter(|&(i, s)| worth(i, s) && Some(i) != here && s.end <= at_ms).map(|(i, _)| i).collect();
    let after: Vec<usize> = stretches.iter().enumerate().filter(|&(i, s)| worth(i, s) && Some(i) != here && s.start > at_ms).map(|(i, _)| i).collect();
    let picked: Vec<usize> = before.iter().rev().take(2).rev().copied().chain(here).chain(after.iter().take(2).copied()).collect();

    let lines: Vec<MomentLine> = picked
        .iter()
        .map(|&i| {
            let s = &stretches[i];
            let mut detail = fmt_dur(s.end - s.start);
            if let Some((title, _)) = s.titles.iter().max_by_key(|(_, ms)| **ms) {
                detail = format!("{detail} · {title}");
            }
            MomentLine { when: format!("{} – {}", clock(s.start), clock(s.end)), what: s.what.clone(), detail, here: Some(i) == here }
        })
        .collect();

    let mut sub = Vec::new();
    if focus::blocks(conn, at_ms, at_ms + 1, now_ms()).is_ok_and(|b| !b.is_empty()) {
        sub.push("In a focus session".to_string());
    }
    if let Ok(Some(event)) = crate::calendar::busy_at(conn, at_ms) {
        // A meeting's name is as private as a window title.
        sub.push(if titles && !event.title.is_empty() { format!("In a meeting: {}", event.title) } else { "In a meeting".into() });
    }
    let note = match (&here, lines.is_empty(), titles) {
        (_, true, _) => "Nothing was recorded then.".to_string(),
        (None, _, _) => "Nothing was recorded at that moment; these are the nearest stretches.".to_string(),
        (Some(i), _, _) if stretches[*i].away => "You were away from the computer.".to_string(),
        (_, _, false) => "Window titles are hidden. Show them in Kairo’s settings.".to_string(),
        _ => String::new(),
    };
    MomentCard { title: format!("{day} at {}", at.format("%-I:%M %P")), sub: sub.join(" · "), lines, note }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(conn: &Connection, app: &str, title: &str, site: Option<&str>, idle: bool, from: NaiveDateTime, minutes: i64) {
        let start = Local.from_local_datetime(&from).earliest().unwrap().timestamp_millis();
        let s = db::NewSession { app, exe: app, title, site, start_ms: start, end_ms: start + minutes * 60_000, idle, demo: false };
        db::insert_session(conn, &s).unwrap();
    }

    #[test]
    fn a_moment_shows_what_was_open_with_its_neighbours_and_hides_titles_by_default() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let day = Local::now().date_naive() - Duration::days(1);
        let t = |h, m| day.and_hms_opt(h, m, 0).unwrap();
        session(&conn, "Slack", "#general", None, false, t(14, 0), 20);
        session(&conn, "Code", "main.rs - kairo", None, false, t(14, 20), 40);
        // A glance at something else inside the stretch, then the same again.
        session(&conn, "Code", "lib.rs - kairo", None, false, t(15, 0), 10);
        session(&conn, "Chrome", "Video", Some("youtube.com"), false, t(15, 10), 20);
        session(&conn, "Mail", "Inbox", None, false, t(15, 30), 1);
        session(&conn, "Code", "x", None, false, t(15, 31), 29);

        let plain = moment(&conn, t(15, 15), false);
        assert_eq!(plain.title, "Yesterday at 3:15 pm");
        let names: Vec<(&str, bool)> = plain.lines.iter().map(|l| (l.what.as_str(), l.here)).collect();
        assert_eq!(names, [("Slack", false), ("Code", false), ("youtube.com", true), ("Code", false)]);
        // The stretch asked about, and no window title anywhere.
        assert_eq!(plain.lines[2].detail, "20m");
        assert!(plain.lines.iter().all(|l| !l.detail.contains('·')));
        assert!(plain.note.contains("hidden"));

        let shown = moment(&conn, t(15, 15), true);
        assert_eq!(shown.lines[2].detail, "20m · Video");
        assert_eq!(shown.lines[1].detail.split(" · ").nth(1), Some("main.rs - kairo"));
        assert!(shown.note.is_empty());
    }

    #[test]
    fn a_moment_with_nothing_recorded_or_away_says_so() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        let day = Local::now().date_naive() - Duration::days(2);
        let t = |h, m| day.and_hms_opt(h, m, 0).unwrap();
        assert_eq!(moment(&conn, t(9, 0), false).note, "Nothing was recorded then.");
        session(&conn, "Code", "", None, false, t(8, 0), 20);
        session(&conn, "Code", "", None, true, t(8, 20), 60);
        let away = moment(&conn, t(9, 0), true);
        assert_eq!(away.note, "You were away from the computer.");
        assert_eq!(away.lines.iter().find(|l| l.here).map(|l| l.what.as_str()), Some("Away"));
        // Between recordings: the nearest are shown, and it says so.
        let gap = moment(&conn, t(9, 40), true);
        assert!(gap.lines.iter().any(|l| !l.here) && gap.note.contains("nearest"), "{gap:?}");
    }

    #[test]
    fn projects_can_be_asked_about_like_apps() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        crate::projects::save(&conn, None, "Kairo", "kairo").unwrap();
        // Up to half an hour in VS Code's kairo folder today, and an hour earlier something else.
        let now = now_ms();
        let start = (now - 31 * 60_000).max(bounds(Local::now().date_naive()).0);
        for (app, title, from) in [("code", "main.rs - kairo", start), ("discord", "Friends", start - 60 * 60_000)] {
            let s = db::NewSession { app, exe: app, title, site: None, start_ms: from, end_ms: from + 30 * 60_000, idle: false, demo: false };
            db::insert_session(&conn, &s).unwrap();
        }
        let project = subjects(&conn, &[]).unwrap().into_iter().find(|s| s.key.starts_with("project:")).unwrap();
        assert_eq!(project.name, "Kairo");
        let card = answer(&conn, &project, Period::Today, None);
        assert_eq!(card.title, "Kairo today");
        assert!(card.range.ends_with("· project"), "{}", card.range);
        assert!(card.total_ms > 0 && card.total_ms <= 30 * 60_000);
    }
}
