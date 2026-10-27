//! Calendar import: a private calendar address (the "secret address in iCal format" Google,
//! Outlook and iCloud offer) read every half hour, two weeks either side of now, so time away
//! during a meeting labels itself, scheduled focus waits out meetings, and the palette knows the
//! next one. The calendar is only read; nothing goes back to it.

use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use rrule::RRuleSet;
use rusqlite::{params, Connection};
use serde::Serialize;

use crate::db;

const URLS_KEY: &str = "calendar_urls";
const STATUS_KEY: &str = "calendar_status";
const READ_EVERY: Duration = Duration::from_secs(30 * 60);
const FIRST_AFTER: Duration = Duration::from_secs(45);
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// How far back and ahead events are kept.
const WINDOW_MS: i64 = 14 * DAY_MS;
/// Occurrences of one recurring event within the window, at most; a daily one has 29.
const MAX_OCCURRENCES: u16 = 200;
const TIMEOUT: Duration = Duration::from_secs(30);

type Result<T> = std::result::Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub start_ms: i64,
    pub end_ms: i64,
    pub title: String,
}

// ---------- reading a calendar ----------

/// An iCalendar file's lines, with long lines that continue on the next (indented) one joined.
fn unfold(ics: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for line in ics.lines() {
        match (line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')), lines.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ => lines.push(line.to_string()),
        }
    }
    lines
}

/// One property: its name, its parameters as written (";TZID=..."), and its value.
struct Prop {
    name: String,
    params: String,
    value: String,
}

impl Prop {
    fn parse(line: &str) -> Option<Self> {
        let (head, value) = line.split_once(':')?;
        let (name, params) = head.split_once(';').map_or((head, ""), |(n, p)| (n, p));
        Some(Prop { name: name.to_ascii_uppercase(), params: params.to_string(), value: value.to_string() })
    }

    /// The property as a line `rrule` reads, under another name: a DTEND or RECURRENCE-ID read as
    /// a DTSTART or EXDATE.
    fn as_line(&self, name: &str) -> String {
        if self.params.is_empty() {
            format!("{name}:{}", self.value)
        } else {
            format!("{name};{}:{}", self.params, self.value)
        }
    }

    /// A date with no time of day: an all-day event's.
    fn is_date(&self) -> bool {
        self.params.to_ascii_uppercase().contains("VALUE=DATE") && !self.params.to_ascii_uppercase().contains("VALUE=DATE-TIME")
            || self.value.trim().len() == 8
    }
}

/// When a DTSTART-like property falls, in unix milliseconds, time zone and all. `rrule` reads
/// dates only alongside a rule or extra dates, so the same date is given as one.
fn instant(prop: &Prop) -> Option<i64> {
    let set: RRuleSet = format!("{}\n{}", prop.as_line("DTSTART"), prop.as_line("RDATE")).parse().ok()?;
    Some(set.get_dt_start().timestamp_millis())
}

/// An iCalendar DURATION like "PT1H30M" or "P1D", in milliseconds.
fn duration_ms(value: &str) -> Option<i64> {
    let (negative, value) = value.strip_prefix('-').map_or((false, value), |v| (true, v));
    let mut rest = value.trim_start_matches('+').strip_prefix('P')?;
    let mut total = 0i64;
    let mut in_time = false;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix('T') {
            in_time = true;
            rest = r;
            continue;
        }
        let digits = rest.find(|c: char| !c.is_ascii_digit())?;
        let n: i64 = rest[..digits].parse().ok()?;
        let unit = rest[digits..].chars().next()?;
        total += n * match (unit, in_time) {
            ('W', false) => 7 * DAY_MS,
            ('D', false) => DAY_MS,
            ('H', true) => 3_600_000,
            ('M', true) => 60_000,
            ('S', true) => 1_000,
            _ => return None,
        };
        rest = &rest[digits + 1..];
    }
    Some(if negative { -total } else { total })
}

/// Text as iCalendar escapes it: "\," for a comma, "\n" for a line break.
fn unescape(text: &str) -> String {
    text.replace("\\n", " ").replace("\\N", " ").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\").trim().to_string()
}

/// The timed events in a calendar file that overlap [from, to), recurring ones expanded. All-day,
/// cancelled and free ("transparent") events don't count as meetings; a recurring event's moved
/// or cancelled occurrence follows its own entry.
pub fn parse(ics: &str, from_ms: i64, to_ms: i64) -> Vec<Event> {
    let mut events: Vec<Vec<Prop>> = Vec::new();
    let mut current: Option<Vec<Prop>> = None;
    for line in unfold(ics) {
        match line.trim_end() {
            "BEGIN:VEVENT" => current = Some(Vec::new()),
            "END:VEVENT" => events.extend(current.take()),
            line => {
                if let (Some(props), Some(prop)) = (current.as_mut(), Prop::parse(line)) {
                    props.push(prop);
                }
            }
        }
    }
    let find =|props: &Vec<Prop>, name: &str| -> Option<usize> { props.iter().position(|p| p.name == name) };

    // Occurrences moved or cancelled on their own, by the event they belong to.
    let mut moved: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for props in &events {
        if let (Some(uid), Some(rid)) = (find(props, "UID"), find(props, "RECURRENCE-ID")) {
            moved.entry(props[uid].value.clone()).or_default().push(props[rid].as_line("EXDATE"));
        }
    }

    let mut out = Vec::new();
    for props in &events {
        let value = |name: &str| find(props, name).map(|i| props[i].value.trim().to_ascii_uppercase());
        if value("STATUS").as_deref() == Some("CANCELLED") || value("TRANSP").as_deref() == Some("TRANSPARENT") {
            continue;
        }
        let Some(start) = find(props, "DTSTART").map(|i| &props[i]) else { continue };
        if start.is_date() {
            continue;
        }
        let Some(first_ms) = instant(start) else { continue };
        let length = match (find(props, "DTEND"), find(props, "DURATION")) {
            (Some(end), _) => instant(&props[end]).map(|end| end - first_ms),
            (None, Some(d)) => duration_ms(&props[d].value),
            (None, None) => None,
        };
        let Some(length) = length.filter(|&l| l > 0) else { continue };
        let title = find(props, "SUMMARY").map(|i| unescape(&props[i].value)).filter(|t| !t.is_empty()).unwrap_or_else(|| "Meeting".into());

        let repeats = props.iter().any(|p| matches!(p.name.as_str(), "RRULE" | "RDATE"));
        let recurring: Vec<String> = props.iter().filter(|p| matches!(p.name.as_str(), "RRULE" | "RDATE" | "EXDATE")).map(|p| p.as_line(&p.name)).collect();
        let starts = if !repeats || find(props, "RECURRENCE-ID").is_some() {
            vec![first_ms]
        } else {
            let mut text = vec![start.as_line("DTSTART")];
            text.extend(recurring);
            if let Some(uid) = find(props, "UID") {
                text.extend(moved.get(&props[uid].value).into_iter().flatten().cloned());
            }
            let Ok(set) = text.join("\n").parse::<RRuleSet>() else { continue };
            let tz = set.get_dt_start().timezone();
            let at = |ms: i64| chrono::DateTime::from_timestamp_millis(ms).map(|t| t.with_timezone(&tz));
            let (Some(after), Some(before)) = (at(from_ms - length), at(to_ms)) else { continue };
            set.after(after).before(before).all(MAX_OCCURRENCES).dates.iter().map(|d| d.timestamp_millis()).collect()
        };
        for s in starts {
            if s < to_ms && s + length > from_ms {
                out.push(Event { start_ms: s, end_ms: s + length, title: title.clone() });
            }
        }
    }
    out.sort_by_key(|e| e.start_ms);
    out
}

// ---------- keeping them ----------

/// Replaces the events kept with `events`.
pub fn store(conn: &Connection, events: &[Event]) -> rusqlite::Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM calendar_events", [])?;
    for e in events {
        tx.execute("INSERT INTO calendar_events (start_ms, end_ms, title) VALUES (?1, ?2, ?3)", params![e.start_ms, e.end_ms, e.title])?;
    }
    tx.commit()
}

/// Events overlapping [from, to), earliest first.
pub fn between(conn: &Connection, from: i64, to: i64) -> rusqlite::Result<Vec<Event>> {
    let mut stmt = conn.prepare("SELECT start_ms, end_ms, title FROM calendar_events WHERE end_ms > ?1 AND start_ms < ?2 ORDER BY start_ms")?;
    let events = stmt.query_map(params![from, to], |r| Ok(Event { start_ms: r.get(0)?, end_ms: r.get(1)?, title: r.get(2)? }))?.collect();
    events
}

/// The meeting going on at `at`, if any.
pub fn busy_at(conn: &Connection, at: i64) -> rusqlite::Result<Option<Event>> {
    Ok(between(conn, at, at + 1)?.into_iter().next())
}

/// The next meeting starting after `at`, within the days kept.
pub fn next_after(conn: &Connection, at: i64) -> rusqlite::Result<Option<Event>> {
    Ok(between(conn, at, at + WINDOW_MS)?.into_iter().find(|e| e.start_ms > at))
}

// ---------- the addresses ----------

/// The calendar addresses to read, one per line.
pub fn urls(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    Ok(db::get_setting(conn, URLS_KEY)?.unwrap_or_default().lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
}

/// Keeps the addresses given, one per line; each must be a web address (webcal:// counts).
pub fn set_urls(conn: &Connection, text: &str) -> Result<Vec<String>> {
    let urls: Vec<String> = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect();
    if let Some(bad) = urls.iter().find(|u| !["https://", "http://", "webcal://"].iter().any(|s| u.to_ascii_lowercase().starts_with(s))) {
        return Err(format!("{bad} isn't a calendar address; it should start with https:// or webcal://."));
    }
    db::set_setting(conn, URLS_KEY, &urls.join("\n")).map_err(err)?;
    if urls.is_empty() {
        store(conn, &[]).map_err(err)?;
        db::set_setting(conn, STATUS_KEY, "").map_err(err)?;
    }
    Ok(urls)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub urls: Vec<String>,
    /// When the calendars were last read, and what came of it.
    pub read_ms: Option<i64>,
    pub events: usize,
    pub error: Option<String>,
}

pub fn status(conn: &Connection) -> rusqlite::Result<Status> {
    let saved = db::get_setting(conn, STATUS_KEY)?.unwrap_or_default();
    let mut parts = saved.splitn(3, '|');
    let read_ms = parts.next().and_then(|p| p.parse().ok());
    let events = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let error = parts.next().map(String::from).filter(|e| !e.is_empty());
    Ok(Status { urls: urls(conn)?, read_ms, events, error })
}

/// Reads every calendar and keeps their events, if any calendar could be read; the status says
/// how many, or what went wrong.
pub fn read_now(conn: &Connection, now_ms: i64) -> rusqlite::Result<Status> {
    let urls = urls(conn)?;
    if urls.is_empty() {
        return status(conn);
    }
    let mut events = Vec::new();
    let mut errors = Vec::new();
    let mut any = false;
    for url in &urls {
        match fetch(url) {
            Ok(ics) => {
                any = true;
                events.extend(parse(&ics, now_ms - WINDOW_MS, now_ms + WINDOW_MS));
            }
            Err(e) => errors.push(e),
        }
    }
    if any {
        events.sort_by_key(|e| e.start_ms);
        store(conn, &events)?;
    }
    db::set_setting(conn, STATUS_KEY, &format!("{now_ms}|{}|{}", events.len(), errors.join("; ")))?;
    status(conn)
}

/// A calendar file from the web.
fn fetch(url: &str) -> Result<String> {
    // A webcal address is the same file over https.
    let url = match url.get(..9) {
        Some(scheme) if scheme.eq_ignore_ascii_case("webcal://") => format!("https://{}", &url[9..]),
        _ => url.to_string(),
    };
    // rustls needs its cryptography chosen once per process; the updater does the same.
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let host = url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or("the calendar").to_string();
    tauri::async_runtime::block_on(async {
        let client = reqwest::Client::builder().timeout(TIMEOUT).build().map_err(err)?;
        let response = client.get(&url).send().await.map_err(|_| format!("Couldn't reach {host}."))?;
        if !response.status().is_success() {
            return Err(format!("{host} said {}; is the address still right?", response.status()));
        }
        let text = response.text().await.map_err(err)?;
        if !text.contains("BEGIN:VCALENDAR") {
            return Err(format!("{host} didn't send a calendar."));
        }
        Ok(text)
    })
}

/// Every half hour: reads the calendars, then labels time away during meetings that have ended.
pub fn spawn(db_path: PathBuf) {
    thread::Builder::new()
        .name("kairo-calendar".into())
        .spawn(move || {
            thread::sleep(FIRST_AFTER);
            let conn = match db::open(&db_path) {
                Ok(conn) => conn,
                Err(e) => return eprintln!("calendar: cannot open the database: {e}"),
            };
            loop {
                let now = chrono::Utc::now().timestamp_millis();
                if !urls(&conn).unwrap_or_default().is_empty() {
                    if let Err(e) = read_now(&conn, now) {
                        eprintln!("calendar: {e}");
                    }
                }
                if let Err(e) = crate::away::label_meetings(&conn, now) {
                    eprintln!("calendar: {e}");
                }
                thread::sleep(READ_EVERY);
            }
        })
        .expect("failed to start calendar thread");
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn ms(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap().timestamp_millis()
    }

    const ICS: &str = "BEGIN:VCALENDAR\r
VERSION:2.0\r
BEGIN:VEVENT\r
UID:standup\r
SUMMARY:Daily stand\\, up\r
DTSTART:20261026T090000Z\r
DTEND:20261026T091500Z\r
RRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR\r
EXDATE:20261028T090000Z\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:standup\r
RECURRENCE-ID:20261029T090000Z\r
SUMMARY:Daily stand\\, up (moved)\r
DTSTART:20261029T100000Z\r
DTEND:20261029T101500Z\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:review\r
SUMMARY:Design review with a long title that the calendar folds onto\r
  the next line\r
DTSTART;TZID=Europe/London:20261027T140000\r
DURATION:PT1H\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:holiday\r
SUMMARY:Holiday\r
DTSTART;VALUE=DATE:20261030\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:cancelled\r
SUMMARY:Cancelled thing\r
STATUS:CANCELLED\r
DTSTART:20261027T120000Z\r
DTEND:20261027T130000Z\r
END:VEVENT\r
BEGIN:VEVENT\r
UID:lunch\r
SUMMARY:Lunch (free)\r
TRANSP:TRANSPARENT\r
DTSTART:20261027T120000Z\r
DTEND:20261027T130000Z\r
END:VEVENT\r
END:VCALENDAR\r
";

    #[test]
    fn calendars_give_their_meetings_with_repeats_moves_and_time_zones() {
        let events = parse(ICS, ms(2026, 10, 26, 0, 0), ms(2026, 10, 31, 0, 0));
        let seen: Vec<(i64, &str)> = events.iter().map(|e| (e.start_ms, e.title.as_str())).collect();
        assert_eq!(
            seen,
            [
                (ms(2026, 10, 26, 9, 0), "Daily stand, up"),
                (ms(2026, 10, 27, 9, 0), "Daily stand, up"),
                // London is on GMT again after 25 October, so 14:00 there is 14:00 UTC.
                (ms(2026, 10, 27, 14, 0), "Design review with a long title that the calendar folds onto the next line"),
                // Wednesday's stand-up is excepted, Thursday's moved to 10:00.
                (ms(2026, 10, 29, 10, 0), "Daily stand, up (moved)"),
                (ms(2026, 10, 30, 9, 0), "Daily stand, up"),
            ]
        );
        assert_eq!(events[0].end_ms - events[0].start_ms, 15 * 60_000);
        assert_eq!(events[2].end_ms - events[2].start_ms, 60 * 60_000);
    }

    #[test]
    fn meetings_are_kept_and_found_by_time() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        store(&conn, &parse(ICS, ms(2026, 10, 26, 0, 0), ms(2026, 10, 31, 0, 0))).unwrap();
        assert_eq!(busy_at(&conn, ms(2026, 10, 27, 14, 30)).unwrap().map(|e| e.title.starts_with("Design review")), Some(true));
        assert!(busy_at(&conn, ms(2026, 10, 27, 15, 30)).unwrap().is_none());
        assert_eq!(next_after(&conn, ms(2026, 10, 27, 9, 30)).unwrap().map(|e| e.start_ms), Some(ms(2026, 10, 27, 14, 0)));
    }

    #[test]
    fn addresses_are_checked_and_durations_read() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        assert!(set_urls(&conn, "ftp://example.com/cal.ics").is_err());
        assert_eq!(set_urls(&conn, "\n webcal://example.com/basic.ics \n").unwrap(), ["webcal://example.com/basic.ics"]);
        assert_eq!(duration_ms("PT1H30M"), Some(90 * 60_000));
        assert_eq!(duration_ms("P1DT2H"), Some(26 * 3_600_000));
        assert_eq!(duration_ms("nonsense"), None);
    }
}
