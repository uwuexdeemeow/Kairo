//! "Remind me": `remind me in 20m to call Leo`, `remind me at 3pm to send the invoice`, `remind me
//! to stretch tomorrow at 9`. The reminder is a Ledger task with that exact reminder time, so it
//! rings like any task's (reminders.rs, within a minute of its time), shows in Ledger to be ticked
//! off, and travels to combined computers without ringing twice. Only the reading of the words is
//! here, so all of it can be tested.

use chrono::{Duration, NaiveDateTime, NaiveTime};

/// A reminder this far off at most: a longer one belongs in Ledger with a due day.
const MAX_AHEAD_DAYS: i64 = 7;
/// "tomorrow" with no time, and "tonight".
const MORNING: (u32, u32) = (9, 0);
const EVENING: (u32, u32) = (20, 0);

#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    /// What to be reminded of, as typed.
    pub what: String,
    pub at: NaiveDateTime,
}

/// What's typed after "remind me" (or "remind"); None when the query isn't asking for a reminder.
pub fn led_by(query: &str) -> Option<&str> {
    let q = query.trim_start();
    let lower = q.to_lowercase();
    if lower.trim_end() == "remind me" || lower.trim_end() == "remind" {
        return Some("");
    }
    for lead in ["remind me ", "remind "] {
        if lower.starts_with(lead) {
            return Some(q[lead.len()..].trim());
        }
    }
    None
}

/// A clock time as people type it: "3pm", "3:30 pm" (two words), "15:30", "noon", "9". A bare hour
/// from 1 to 12 is either half of the day, so both are given, morning first.
fn clock(words: &[&str]) -> Option<(Vec<NaiveTime>, usize)> {
    let first = words.first()?.to_lowercase();
    if first == "noon" || first == "midday" {
        return Some((vec![NaiveTime::from_hms_opt(12, 0, 0)?], 1));
    }
    if first == "midnight" {
        return Some((vec![NaiveTime::from_hms_opt(0, 0, 0)?], 1));
    }
    // "3pm", or "3" then "pm".
    let (body, half, used) = match (first.strip_suffix("am").or_else(|| first.strip_suffix("a.m.")), first.strip_suffix("pm").or_else(|| first.strip_suffix("p.m."))) {
        (Some(b), _) => (b.to_string(), Some(false), 1),
        (_, Some(b)) => (b.to_string(), Some(true), 1),
        _ => match words.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("am") | Some("a.m.") => (first.clone(), Some(false), 2),
            Some("pm") | Some("p.m.") => (first.clone(), Some(true), 2),
            _ => (first.clone(), None, 1),
        },
    };
    let (h, m) = match body.split_once([':', '.']) {
        Some((h, m)) if m.len() == 2 => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        Some(_) => return None,
        None => (body.parse::<u32>().ok()?, 0),
    };
    if m > 59 {
        return None;
    }
    let at = |h: u32| NaiveTime::from_hms_opt(h, m, 0);
    let times = match half {
        Some(pm) if (1..=12).contains(&h) => vec![at(h % 12 + if pm { 12 } else { 0 })?],
        Some(_) => return None,
        // "15:30" is plain; "9" or "9:30" could be either.
        None if (1..=12).contains(&h) => vec![at(h % 12)?, at(h % 12 + 12)?],
        None if h <= 23 => vec![at(h)?],
        None => return None,
    };
    Some((times, used))
}

/// "20m", "2h", "90min": a length stuck to its unit; or a unit word alone.
fn unit_minutes(unit: &str) -> Option<i64> {
    match unit {
        "m" | "min" | "mins" | "minute" | "minutes" => Some(1),
        "h" | "hr" | "hrs" | "hour" | "hours" => Some(60),
        _ => None,
    }
}

fn length(words: &[&str]) -> Option<(i64, usize)> {
    let first = words.first()?.to_lowercase();
    if let Ok(n) = first.parse::<i64>() {
        let per = unit_minutes(&words.get(1)?.to_lowercase())?;
        return Some((n * per, 2));
    }
    let digits = first.find(|c: char| !c.is_ascii_digit())?;
    let n: i64 = first[..digits].parse().ok()?;
    Some((n * unit_minutes(&first[digits..])?, 1))
}

/// The soonest of `times` after `now` on `day` or later, today counting only when `today_ok`.
fn next_at(now: NaiveDateTime, days_on: i64, times: &[NaiveTime], from_today: bool) -> Option<NaiveDateTime> {
    let day = now.date() + Duration::days(days_on);
    let mut found: Vec<NaiveDateTime> = times.iter().map(|t| day.and_time(*t)).filter(|t| *t > now).collect();
    if found.is_empty() && from_today {
        // Passed today: the same time tomorrow, the earliest of its halves.
        found = times.iter().map(|t| (day + Duration::days(1)).and_time(*t)).collect();
    }
    found.into_iter().min()
}

/// A "when" at the start of `words`: when it is and how many words it took.
fn when(words: &[&str], now: NaiveDateTime) -> Option<(NaiveDateTime, usize)> {
    let first = words.first()?.to_lowercase();
    match first.as_str() {
        "in" => {
            let (minutes, used) = length(&words[1..])?;
            (minutes > 0).then(|| (now + Duration::minutes(minutes), 1 + used))
        }
        "at" => {
            let (times, used) = clock(&words[1..])?;
            Some((next_at(now, 0, &times, true)?, 1 + used))
        }
        "tonight" => Some((next_at(now, 0, &[NaiveTime::from_hms_opt(EVENING.0, EVENING.1, 0)?], false)?, 1)),
        "today" | "tomorrow" => {
            let days = i64::from(first == "tomorrow");
            match (words.get(1).map(|w| w.to_lowercase()), words.get(2)) {
                (Some(at), Some(_)) if at == "at" => {
                    let (times, used) = clock(&words[2..])?;
                    // Tomorrow's "9" is its morning; today's is whichever half is still ahead.
                    let times = if days == 1 { vec![times[0]] } else { times };
                    Some((next_at(now, days, &times, false)?, 2 + used))
                }
                _ if days == 1 => Some((next_at(now, 1, &[NaiveTime::from_hms_opt(MORNING.0, MORNING.1, 0)?], false)?, 1)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Words that join what to be reminded of to the rest, dropped from its start.
const JOINERS: [&str; 3] = ["to", "about", "that"];

/// Reads what follows "remind me": when, from its start or its end, and what. The error says what
/// is missing.
pub fn parse(text: &str, now: NaiveDateTime) -> Result<Ask, &'static str> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let (at, rest): (Option<NaiveDateTime>, Vec<&str>) = match when(&words, now) {
        Some((at, used)) => (Some(at), words[used..].to_vec()),
        None => (1..=words.len().min(5))
            .rev()
            .find_map(|k| {
                let start = words.len() - k;
                when(&words[start..], now).filter(|(_, used)| *used == k).map(|(at, _)| (Some(at), words[..start].to_vec()))
            })
            .unwrap_or((None, words.clone())),
    };
    let mut rest = rest.as_slice();
    while let Some(first) = rest.first() {
        if JOINERS.contains(&first.to_lowercase().as_str()) {
            rest = &rest[1..];
        } else {
            break;
        }
    }
    let what = rest.join(" ");
    match (at, what.is_empty()) {
        (None, true) => Err("Say when and what: remind me in 20m to call Leo"),
        (None, false) => Err("Say when: in 20m, at 3pm or tomorrow at 9"),
        (Some(_), true) => Err("Say what to be reminded of: remind me in 20m to call Leo"),
        (Some(at), false) if at > now + Duration::days(MAX_AHEAD_DAYS) => Err("That's more than a week away. Give the task a due day in Ledger instead"),
        (Some(at), false) => Ok(Ask { what, at }),
    }
}

/// When a reminder rings, as its row says it: "In 20 min, at 15:20", "Tomorrow at 09:00".
pub fn when_said(at: NaiveDateTime, now: NaiveDateTime) -> String {
    let minutes = (at - now).num_minutes().max(1);
    let clock = crate::quick_add::clock(at);
    match (at.date() - now.date()).num_days() {
        0 if minutes < 60 => format!("In {minutes} min, at {clock}"),
        0 => format!("Today at {clock}"),
        1 => format!("Tomorrow at {clock}"),
        _ => format!("{} at {clock}", at.format("%A")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Monday 26 October 2026, 10:00.
    fn now() -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap().and_hms_opt(10, 0, 0).unwrap()
    }

    fn at(day: u32, h: u32, m: u32) -> NaiveDateTime {
        chrono::NaiveDate::from_ymd_opt(2026, 10, day).unwrap().and_hms_opt(h, m, 0).unwrap()
    }

    fn ok(text: &str) -> (String, NaiveDateTime) {
        let a = parse(text, now()).unwrap_or_else(|e| panic!("{text}: {e}"));
        (a.what, a.at)
    }

    #[test]
    fn only_a_query_led_by_remind_asks() {
        assert_eq!(led_by("remind me in 20m to call Leo"), Some("in 20m to call Leo"));
        assert_eq!(led_by("Remind me"), Some(""));
        assert_eq!(led_by("remind stretch at 3pm"), Some("stretch at 3pm"));
        assert_eq!(led_by("reminders"), None);
        assert_eq!(led_by("call Leo"), None);
    }

    #[test]
    fn in_a_length_from_now() {
        assert_eq!(ok("in 20m to call Leo"), ("call Leo".into(), at(26, 10, 20)));
        assert_eq!(ok("in 2 hours to stretch"), ("stretch".into(), at(26, 12, 0)));
        assert_eq!(ok("in 90 min check the oven"), ("check the oven".into(), at(26, 11, 30)));
        assert_eq!(ok("to call Leo in 1h"), ("call Leo".into(), at(26, 11, 0)));
    }

    #[test]
    fn at_a_clock_time_the_next_time_it_comes() {
        assert_eq!(ok("at 3pm to send the invoice"), ("send the invoice".into(), at(26, 15, 0)));
        assert_eq!(ok("at 3:30 pm send it"), ("send it".into(), at(26, 15, 30)));
        assert_eq!(ok("at 15:30 send it"), ("send it".into(), at(26, 15, 30)));
        // A bare 9 at 10:00 is this evening; a bare 11, this morning still.
        assert_eq!(ok("at 9 water the plants"), ("water the plants".into(), at(26, 21, 0)));
        assert_eq!(ok("at 11 standup notes"), ("standup notes".into(), at(26, 11, 0)));
        // 8am has passed: tomorrow.
        assert_eq!(ok("at 8am run"), ("run".into(), at(27, 8, 0)));
        assert_eq!(ok("pay rent at noon"), ("pay rent".into(), at(26, 12, 0)));
    }

    #[test]
    fn today_tomorrow_and_tonight() {
        assert_eq!(ok("tomorrow at 9 to stretch"), ("stretch".into(), at(27, 9, 0)));
        assert_eq!(ok("to stretch tomorrow"), ("stretch".into(), at(27, 9, 0)));
        assert_eq!(ok("tonight to call mum"), ("call mum".into(), at(26, 20, 0)));
        assert_eq!(ok("today at 5pm about the parcel"), ("the parcel".into(), at(26, 17, 0)));
        assert!(parse("today at 8am run", now()).is_err());
    }

    #[test]
    fn what_is_missing_is_said() {
        assert_eq!(parse("call Leo", now()), Err("Say when: in 20m, at 3pm or tomorrow at 9"));
        assert_eq!(parse("in 20m", now()), Err("Say what to be reminded of: remind me in 20m to call Leo"));
        assert_eq!(parse("", now()), Err("Say when and what: remind me in 20m to call Leo"));
        assert!(parse("in 300 hours to renew", now()).unwrap_err().starts_with("That's more than a week away"));
        // Words that only look like a time stay part of what.
        assert_eq!(ok("at 4pm read chapter 3 in 2 parts").0, "read chapter 3 in 2 parts");
    }

    #[test]
    fn when_it_rings_is_said_plainly() {
        assert_eq!(when_said(at(26, 10, 20), now()), "In 20 min, at 10:20");
        assert_eq!(when_said(at(26, 15, 0), now()), "Today at 15:00");
        assert_eq!(when_said(at(27, 9, 0), now()), "Tomorrow at 09:00");
        assert_eq!(when_said(at(29, 9, 0), now()), "Thursday at 09:00");
    }
}
