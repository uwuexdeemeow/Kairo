//! Time zones in the palette: `time in tokyo` (or `tokyo time`) says the time there now, and
//! `3pm pst in london`, `9:30 in new york` or `noon tokyo to berlin` turns a time in one place
//! (here, when none is said) into another's. Places are cities from the time zone database
//! (`chrono-tz`, already built in for the calendar), a few more cities and countries, and the
//! usual abbreviations (PST, EST, CET, IST, JST…). Nothing is looked up anywhere. No drawing here,
//! so all of it can be tested.

use chrono::{DateTime, Duration, NaiveTime, Offset, TimeZone, Utc};
use chrono_tz::Tz;

/// A time-zone answer: the row's title, what Enter copies, and the line under it.
#[derive(Debug, PartialEq)]
pub struct ZoneAnswer {
    pub title: String,
    pub copy: String,
    pub sub: String,
}

/// Words that name a zone without being a city in the database.
const ALIASES: &[(&str, &str)] = &[
    ("pst", "America/Los_Angeles"),
    ("pdt", "America/Los_Angeles"),
    ("pt", "America/Los_Angeles"),
    ("pacific", "America/Los_Angeles"),
    ("san francisco", "America/Los_Angeles"),
    ("sf", "America/Los_Angeles"),
    ("seattle", "America/Los_Angeles"),
    ("california", "America/Los_Angeles"),
    ("la", "America/Los_Angeles"),
    ("est", "America/New_York"),
    ("edt", "America/New_York"),
    ("et", "America/New_York"),
    ("eastern", "America/New_York"),
    ("nyc", "America/New_York"),
    ("boston", "America/New_York"),
    ("washington", "America/New_York"),
    ("miami", "America/New_York"),
    ("atlanta", "America/New_York"),
    ("cst", "America/Chicago"),
    ("cdt", "America/Chicago"),
    ("ct", "America/Chicago"),
    ("central", "America/Chicago"),
    ("austin", "America/Chicago"),
    ("dallas", "America/Chicago"),
    ("houston", "America/Chicago"),
    ("mst", "America/Denver"),
    ("mdt", "America/Denver"),
    ("mt", "America/Denver"),
    ("gmt", "UTC"),
    ("utc", "UTC"),
    ("bst", "Europe/London"),
    ("uk", "Europe/London"),
    ("england", "Europe/London"),
    ("cet", "Europe/Berlin"),
    ("cest", "Europe/Berlin"),
    ("germany", "Europe/Berlin"),
    ("france", "Europe/Paris"),
    ("spain", "Europe/Madrid"),
    ("italy", "Europe/Rome"),
    ("netherlands", "Europe/Amsterdam"),
    ("ist", "Asia/Kolkata"),
    ("india", "Asia/Kolkata"),
    ("delhi", "Asia/Kolkata"),
    ("new delhi", "Asia/Kolkata"),
    ("mumbai", "Asia/Kolkata"),
    ("bangalore", "Asia/Kolkata"),
    ("bengaluru", "Asia/Kolkata"),
    ("jst", "Asia/Tokyo"),
    ("japan", "Asia/Tokyo"),
    ("kst", "Asia/Seoul"),
    ("korea", "Asia/Seoul"),
    ("china", "Asia/Shanghai"),
    ("beijing", "Asia/Shanghai"),
    ("hkt", "Asia/Hong_Kong"),
    ("sgt", "Asia/Singapore"),
    ("philippines", "Asia/Manila"),
    ("malaysia", "Asia/Kuala_Lumpur"),
    ("kl", "Asia/Kuala_Lumpur"),
    ("indonesia", "Asia/Jakarta"),
    ("thailand", "Asia/Bangkok"),
    ("vietnam", "Asia/Ho_Chi_Minh"),
    ("uae", "Asia/Dubai"),
    ("aest", "Australia/Sydney"),
    ("aedt", "Australia/Sydney"),
    ("australia", "Australia/Sydney"),
    ("nz", "Pacific/Auckland"),
    ("new zealand", "Pacific/Auckland"),
    ("brazil", "America/Sao_Paulo"),
    ("mexico", "America/Mexico_City"),
    ("canada", "America/Toronto"),
];

/// The zone a place's name means, and the name to show for it: a city from the database by its
/// last part ("tokyo", "new york", "los angeles"), or one of the aliases.
pub fn place(name: &str) -> Option<(Tz, String)> {
    let name = name.trim().trim_end_matches('?').trim().to_lowercase();
    if name.is_empty() {
        return None;
    }
    if let Some((_, id)) = ALIASES.iter().find(|(word, _)| *word == name) {
        let tz: Tz = id.parse().ok()?;
        let shown = if name.len() <= 4 && !name.contains(' ') { name.to_uppercase() } else { titled(&name) };
        return Some((tz, shown));
    }
    chrono_tz::TZ_VARIANTS.iter().find_map(|tz| {
        let city = tz.name().rsplit('/').next()?.replace('_', " ");
        city.eq_ignore_ascii_case(&name).then(|| (*tz, city))
    })
}

fn titled(name: &str) -> String {
    name.split(' ').map(|w| w[..1].to_uppercase() + &w[1..]).collect::<Vec<_>>().join(" ")
}

/// A time of day as people type it: 3pm, 3:30 pm, 15:00, noon, midnight. A bare number isn't
/// one (`15 in hex` is maths).
fn clock(text: &str) -> Option<NaiveTime> {
    let t = text.trim().to_lowercase();
    match t.as_str() {
        "noon" | "midday" => return NaiveTime::from_hms_opt(12, 0, 0),
        "midnight" => return NaiveTime::from_hms_opt(0, 0, 0),
        _ => {}
    }
    let (digits, half) = match t.strip_suffix("am").or_else(|| t.strip_suffix("a.m.")) {
        Some(d) => (d.trim(), Some(false)),
        None => match t.strip_suffix("pm").or_else(|| t.strip_suffix("p.m.")) {
            Some(d) => (d.trim(), Some(true)),
            None => (t.as_str(), None),
        },
    };
    let (h, m) = match digits.split_once(':') {
        Some((h, m)) if m.len() == 2 => (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?),
        None if half.is_some() => (digits.parse::<u32>().ok()?, 0),
        _ => return None,
    };
    let h = match half {
        Some(pm) if (1..=12).contains(&h) => h % 12 + if pm { 12 } else { 0 },
        Some(_) => return None,
        None => h,
    };
    NaiveTime::from_hms_opt(h, m, 0)
}

/// A clock at the start of `text` and what follows it: "3pm pst" gives 3 pm and "pst", "3 pm"
/// gives 3 pm and "".
fn clock_then(text: &str) -> Option<(NaiveTime, String)> {
    let words: Vec<&str> = text.split_whitespace().collect();
    // "3 pm" is two words; try the longer reading first.
    for take in [2, 1] {
        if words.len() >= take {
            if let Some(t) = clock(&words[..take].join(" ")) {
                return Some((t, words[take..].join(" ")));
            }
        }
    }
    None
}

fn time_of<T: TimeZone>(at: &DateTime<T>) -> String
where
    T::Offset: std::fmt::Display,
{
    at.format("%-I:%M %P").to_string()
}

/// "8 hours ahead", "2 hours 30 minutes behind", "the same time": `there` against `here`.
fn difference(there_secs: i32, here_secs: i32) -> String {
    let diff = there_secs - here_secs;
    if diff == 0 {
        return "the same time".into();
    }
    let (h, m) = (diff.abs() / 3600, diff.abs() % 3600 / 60);
    let hours = match h {
        0 => String::new(),
        1 => "1 hour".into(),
        h => format!("{h} hours"),
    };
    let minutes = if m > 0 { format!("{m} minutes") } else { String::new() };
    let amount = [hours, minutes].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
    format!("{amount} {}", if diff > 0 { "ahead" } else { "behind" })
}

/// A difference said against you: "1 hour ahead of you", "2 hours behind you".
fn against_you(diff: &str) -> String {
    match diff {
        "the same time" => "the same time as you".into(),
        d if d.ends_with("ahead") => format!("{d} of you"),
        d => format!("{d} you"),
    }
}

/// What a query means about time zones, if anything, with `now` and this computer's `local` zone.
pub fn answer<L: TimeZone>(query: &str, now: DateTime<Utc>, local: &L) -> Option<ZoneAnswer>
where
    L::Offset: std::fmt::Display,
{
    let q = query.trim().trim_end_matches('?').trim().to_lowercase();
    let here_secs = now.with_timezone(local).offset().fix().local_minus_utc();
    // "time in tokyo", "what time is it in tokyo", "tokyo time", "now in tokyo".
    let asked = ["what time is it in ", "what's the time in ", "whats the time in ", "time in ", "now in ", "time "]
        .iter()
        .find_map(|p| q.strip_prefix(p))
        .or_else(|| q.strip_suffix(" time"));
    if let Some((tz, shown)) = asked.and_then(place) {
        let there = now.with_timezone(&tz);
        let day = if there.date_naive() != now.with_timezone(local).date_naive() { format!(" {}", there.format("%a")) } else { String::new() };
        let time = time_of(&there);
        let zone = there.format("%Z").to_string();
        return Some(ZoneAnswer {
            title: format!("{time}{day} in {shown}"),
            copy: time.clone(),
            sub: format!("{} · {}, {}", zone, there.format("%a %-d %b"), against_you(&difference(there.offset().fix().local_minus_utc(), here_secs))),
        });
    }
    // "3pm pst in london", "9:30 in new york", "noon tokyo to berlin", "9am in new york to london":
    // each way of splitting it is tried, and the first that reads whole is the answer.
    [" to ", " in "].iter().filter_map(|sep| q.rsplit_once(sep)).find_map(|(from_text, to_text)| convert(from_text, to_text, now, local))
}

/// A time in one place (here, when none is said) as the time in another.
fn convert<L: TimeZone>(from_text: &str, to_text: &str, now: DateTime<Utc>, local: &L) -> Option<ZoneAnswer>
where
    L::Offset: std::fmt::Display,
{
    let (to_tz, to_shown) = place(to_text)?;
    let (at, from_place) = clock_then(from_text)?;
    let from_place = from_place.strip_prefix("in ").unwrap_or(&from_place).trim().to_string();
    let from = if from_place.is_empty() { None } else { Some(place(&from_place)?) };
    let (start, start_shown) = match &from {
        Some((tz, shown)) => {
            let day = now.with_timezone(tz).date_naive();
            (tz.from_local_datetime(&day.and_time(at)).earliest()?.with_timezone(&Utc), shown.clone())
        }
        None => {
            let day = now.with_timezone(local).date_naive();
            (local.from_local_datetime(&day.and_time(at)).earliest()?.with_timezone(&Utc), "here".into())
        }
    };
    let there = start.with_timezone(&to_tz);
    let start_day = match &from {
        Some((tz, _)) => start.with_timezone(tz).date_naive(),
        None => start.with_timezone(local).date_naive(),
    };
    let start_secs = match &from {
        Some((tz, _)) => start.with_timezone(tz).offset().fix().local_minus_utc(),
        None => start.with_timezone(local).offset().fix().local_minus_utc(),
    };
    let moved = there.date_naive().signed_duration_since(start_day);
    let day = match moved {
        d if d == Duration::days(1) => " the next day".to_string(),
        d if d == Duration::days(-1) => " the day before".to_string(),
        _ => String::new(),
    };
    let time = time_of(&there);
    let start_time = start.with_timezone(&chrono::FixedOffset::east_opt(start_secs)?).format("%-I:%M %P").to_string();
    Some(ZoneAnswer {
        title: format!("{time}{day} in {to_shown}"),
        copy: time.clone(),
        sub: format!("{start_time} {} · {} is {}", if start_shown == "here" { "here".to_string() } else { format!("in {start_shown}") }, to_shown, difference(there.offset().fix().local_minus_utc(), start_secs)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    /// Monday 23 November 2026, 04:00 UTC: noon in Singapore, 8 pm the day before in Los Angeles.
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 23, 4, 0, 0).unwrap()
    }

    fn singapore() -> FixedOffset {
        FixedOffset::east_opt(8 * 3600).unwrap()
    }

    #[test]
    fn places_are_cities_abbreviations_and_countries() {
        assert_eq!(place("Tokyo").map(|(t, s)| (t.name(), s)), Some(("Asia/Tokyo", "Tokyo".into())));
        assert_eq!(place("new york").map(|(t, s)| (t.name(), s)), Some(("America/New_York", "New York".into())));
        assert_eq!(place("pst").map(|(t, s)| (t.name(), s)), Some(("America/Los_Angeles", "PST".into())));
        assert_eq!(place("san francisco").map(|(t, s)| (t.name(), s)), Some(("America/Los_Angeles", "San Francisco".into())));
        assert_eq!(place("japan").map(|(t, _)| t.name()), Some("Asia/Tokyo"));
        assert!(place("mi").is_none() && place("km").is_none() && place("").is_none());
    }

    #[test]
    fn clocks_are_read_as_people_type_them() {
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0);
        assert_eq!(clock("3pm"), t(15, 0));
        assert_eq!(clock("12am"), t(0, 0));
        assert_eq!(clock("12 pm"), t(12, 0));
        assert_eq!(clock("9:30"), t(9, 30));
        assert_eq!(clock("noon"), t(12, 0));
        assert_eq!(clock("15"), None);
        assert_eq!(clock("13pm"), None);
        assert_eq!(clock_then("3 pm pst"), Some((t(15, 0).unwrap(), "pst".into())));
    }

    #[test]
    fn the_time_there_now_says_how_far_ahead_it_is() {
        let a = answer("time in tokyo", now(), &singapore()).unwrap();
        assert_eq!(a.title, "1:00 pm in Tokyo");
        assert_eq!(a.copy, "1:00 pm");
        assert_eq!(a.sub, "JST · Mon 23 Nov, 1 hour ahead of you");
        // Another day there says which.
        let la = answer("los angeles time", now(), &singapore()).unwrap();
        assert_eq!(la.title, "8:00 pm Sun in Los Angeles");
        assert!(la.sub.ends_with("16 hours behind you"), "{}", la.sub);
        assert_eq!(answer("what time is it in kolkata?", now(), &singapore()).unwrap().sub, "IST · Mon 23 Nov, 2 hours 30 minutes behind you");
    }

    #[test]
    fn a_time_in_one_place_is_turned_into_another_s() {
        // 3 pm in Los Angeles on Sunday is 11 pm in London.
        let a = answer("3pm pst in london", now(), &singapore()).unwrap();
        assert_eq!(a.title, "11:00 pm in London");
        assert_eq!(a.sub, "3:00 pm in PST · London is 8 hours ahead");
        // From here, and onto the next day.
        let b = answer("9pm to new york", now(), &singapore()).unwrap();
        assert_eq!(b.title, "8:00 am in New York");
        assert_eq!(answer("9am to new york", now(), &singapore()).unwrap().title, "8:00 pm the day before in New York");
        assert_eq!(answer("noon tokyo to berlin", now(), &singapore()).unwrap().title, "4:00 am in Berlin");
    }

    #[test]
    fn a_time_in_one_named_place_to_another_reads_either_way() {
        let local = FixedOffset::east_opt(8 * 3600).unwrap();
        let a = answer("9am in new york to london", now(), &local).expect("read with “to” after “in”");
        assert!(a.title.contains("London"), "{}", a.title);
        assert_eq!(answer("9am new york to london", now(), &local).map(|a| a.title), Some(a.title.clone()));
    }

    #[test]
    fn maths_and_unknown_places_are_left_alone() {
        for q in ["5 km in mi", "15 in hex", "20 usd in eur", "3pm in narnia", "time", "tokyo"] {
            assert_eq!(answer(q, now(), &singapore()), None, "{q}");
        }
    }
}
