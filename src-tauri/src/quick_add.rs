//! Quick add: the shortcuts a new task's text can end with, for its priority (`!h`, `!med`,
//! `!low`) and due day (`today`, `fri`, `7 nov`, `11/7`), read the same way by Ledger's field and
//! the palette's `task` command.

use chrono::NaiveDate;

#[derive(Debug, Clone, PartialEq)]
pub struct Quick {
    pub title: String,
    /// "2026-11-07"
    pub due: Option<String>,
    pub priority: Option<u8>,
    /// How it repeats: one of `tasks::REPEATS`.
    pub repeat: Option<String>,
}

/// Reads `text`'s shortcuts as of `today`: whole words at its end, in any order, one priority and
/// one due day; the first word that isn't one stops the reading, and the rest is the title. A
/// word in double quotes is never a shortcut. Nothing left for a title: the text as typed.
/// `day_first` reads `11/7` as 11 July.
pub fn parse(text: &str, today: NaiveDate, day_first: bool) -> Quick {
    let words: Vec<&str> = text.split_whitespace().collect();
    let (mut due, mut priority) = (None, None);
    // "every fri" repeats weekly and is due the next Friday, unless a day is given beside it.
    let (mut repeat, mut repeat_day): (Option<&str>, Option<NaiveDate>) = (None, None);
    let mut end = words.len();
    while end > 0 {
        let word = words[end - 1];
        if repeat.is_none() && end >= 2 && words[end - 2].eq_ignore_ascii_case("every") {
            if let Some((rule, day)) = repeat_of(word, today) {
                (repeat, repeat_day) = (Some(rule), day);
                end -= 2;
                continue;
            }
        }
        if priority.is_none() {
            if let Some(p) = priority_of(word) {
                priority = Some(p);
                end -= 1;
                continue;
            }
        }
        if due.is_none() {
            // Two words ("7 nov", "nov 7") before one, so "7 nov" isn't read as just "nov".
            if end >= 2 {
                if let Some(d) = day_and_month(words[end - 2], word, today) {
                    due = Some(d);
                    end -= 2;
                    continue;
                }
            }
            if let Some(d) = one_word_day(word, today, day_first) {
                due = Some(d);
                end -= 1;
                continue;
            }
        }
        break;
    }
    let unquote = |w: &&str| w.strip_prefix('"').and_then(|w| w.strip_suffix('"')).filter(|w| !w.is_empty()).unwrap_or(w).to_string();
    let title: Vec<String> = words[..end].iter().map(unquote).collect();
    if title.is_empty() {
        let all: Vec<String> = words.iter().map(unquote).collect();
        return Quick { title: all.join(" "), due: None, priority: None, repeat: None };
    }
    // A repeating task always has a day: the one beside it, the weekday named, or today (the next
    // weekday, for "every weekday" on a weekend).
    if repeat.is_some() && due.is_none() {
        due = repeat_day.or_else(|| if repeat == Some("weekday") { crate::tasks::next_due("weekday", None, today.pred_opt().unwrap_or(today)) } else { Some(today) });
    }
    Quick { title: title.join(" "), due: due.map(|d| d.format("%Y-%m-%d").to_string()), priority, repeat: repeat.map(String::from) }
}

/// What "every <word>" repeats: its rule, and for a weekday's name the next such day.
fn repeat_of(word: &str, today: NaiveDate) -> Option<(&'static str, Option<NaiveDate>)> {
    let w = word.to_ascii_lowercase();
    let rule = match w.as_str() {
        "day" => "day",
        "weekday" => "weekday",
        "week" => "week",
        "month" => "month",
        "year" => "year",
        _ => return one_word_day(&w, today, true).filter(|_| WEEKDAYS.iter().any(|(names, _)| names.contains(&w.as_str()))).map(|d| ("week", Some(d))),
    };
    Some((rule, None))
}

fn priority_of(word: &str) -> Option<u8> {
    match word.to_ascii_lowercase().as_str() {
        "!h" | "!high" => Some(crate::tasks::HIGH),
        "!m" | "!med" | "!medium" => Some(crate::tasks::MEDIUM),
        "!l" | "!low" => Some(crate::tasks::LOW),
        _ => None,
    }
}

const WEEKDAYS: [(&[&str], chrono::Weekday); 7] = [
    (&["mon", "monday"], chrono::Weekday::Mon),
    (&["tue", "tues", "tuesday"], chrono::Weekday::Tue),
    (&["wed", "weds", "wednesday"], chrono::Weekday::Wed),
    (&["thu", "thur", "thurs", "thursday"], chrono::Weekday::Thu),
    (&["fri", "friday"], chrono::Weekday::Fri),
    (&["sat", "saturday"], chrono::Weekday::Sat),
    (&["sun", "sunday"], chrono::Weekday::Sun),
];

const MONTHS: [&str; 12] = ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"];

/// A month from its name or the first three or more letters of it.
fn month_of(word: &str) -> Option<u32> {
    let w = word.to_ascii_lowercase();
    if w.len() < 3 {
        return None;
    }
    MONTHS.iter().position(|m| m.starts_with(&w)).map(|i| i as u32 + 1)
}

/// The day `month`/`day` falls on next: this year, or next year once it's past.
fn next_date(month: u32, day: u32, today: NaiveDate) -> Option<NaiveDate> {
    use chrono::Datelike;
    let this_year = NaiveDate::from_ymd_opt(today.year(), month, day);
    match this_year {
        Some(d) if d >= today => Some(d),
        _ => NaiveDate::from_ymd_opt(today.year() + 1, month, day),
    }
}

fn day_and_month(first: &str, second: &str, today: NaiveDate) -> Option<NaiveDate> {
    let (day, month) = match (first.parse::<u32>(), second.parse::<u32>()) {
        (Ok(d), Err(_)) => (d, month_of(second)?),
        (Err(_), Ok(d)) => (d, month_of(first)?),
        _ => return None,
    };
    next_date(month, day, today)
}

fn one_word_day(word: &str, today: NaiveDate, day_first: bool) -> Option<NaiveDate> {
    use chrono::Datelike;
    let w = word.to_ascii_lowercase();
    match w.as_str() {
        "today" => return Some(today),
        "tomorrow" | "tmr" | "tmrw" => return today.succ_opt(),
        _ => {}
    }
    if let Some((_, weekday)) = WEEKDAYS.iter().find(|(names, _)| names.contains(&w.as_str())) {
        let ahead = (weekday.num_days_from_monday() + 7 - today.weekday().num_days_from_monday()) % 7;
        return today.checked_add_days(chrono::Days::new(if ahead == 0 { 7 } else { ahead } as u64));
    }
    let (a, b) = w.split_once('/')?;
    let (a, b) = (a.parse::<u32>().ok()?, b.parse::<u32>().ok()?);
    let (month, day) = if day_first { (b, a) } else { (a, b) };
    next_date(month, day, today)
}

/// What was read, for under the field and the palette's row: "High · Fri 6 Nov".
pub fn describe(q: &Quick) -> String {
    let priority = q.priority.map(|p| ["Low", "Medium", "High"][p.min(2) as usize].to_string());
    let due = q.due.as_deref().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()).map(|d| d.format("%a %-d %b").to_string());
    let repeat = q.repeat.as_deref().map(|r| format!("Every {r}"));
    [priority, due, repeat].into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

/// Whether the system writes dates day first: everywhere but the US.
pub fn day_first() -> bool {
    !sys_locale::get_locale().is_some_and(|l| l.replace('_', "-").starts_with("en-US"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Tuesday.
    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 3).unwrap()
    }

    fn read(text: &str) -> (String, Option<String>, Option<u8>) {
        let q = parse(text, today(), true);
        (q.title, q.due, q.priority)
    }

    fn day(s: &str) -> Option<String> {
        Some(s.to_string())
    }

    #[test]
    fn priority_and_weekday_come_off_the_end() {
        assert_eq!(read("essay draft !h fri"), ("essay draft".into(), day("2026-11-06"), Some(2)));
        assert_eq!(read("essay draft fri !high"), ("essay draft".into(), day("2026-11-06"), Some(2)));
        assert_eq!(read("buy milk !LOW tmr"), ("buy milk".into(), day("2026-11-04"), Some(0)));
        assert_eq!(read("call !med"), ("call".into(), None, Some(1)));
        assert_eq!(read("plan trip today"), ("plan trip".into(), day("2026-11-03"), None));
        assert_eq!(read("plan trip Friday"), ("plan trip".into(), day("2026-11-06"), None));
    }

    #[test]
    fn a_weekday_is_the_next_one_after_today() {
        assert_eq!(read("gym tue").1, day("2026-11-10"));
        assert_eq!(read("gym wed").1, day("2026-11-04"));
        assert_eq!(read("gym mon").1, day("2026-11-09"));
    }

    #[test]
    fn dates_in_words_and_numbers() {
        assert_eq!(read("report 7 nov").1, day("2026-11-07"));
        assert_eq!(read("report nov 7").1, day("2026-11-07"));
        assert_eq!(read("report 7 November").1, day("2026-11-07"));
        assert_eq!(read("report 3 nov").1, day("2026-11-03"));
        // Past this year: next year.
        assert_eq!(read("report 7 jan").1, day("2027-01-07"));
        assert_eq!(read("report 11/7").1, day("2027-07-11"));
        assert_eq!(parse("report 11/7", today(), false).due, day("2026-11-07"));
        assert_eq!(read("report 31/2"), ("report 31/2".into(), None, None));
    }

    #[test]
    fn only_the_last_date_counts() {
        assert_eq!(read("meet mon tomorrow"), ("meet mon".into(), day("2026-11-04"), None));
        assert_eq!(read("meet !l !h"), ("meet !l".into(), None, Some(2)));
    }

    #[test]
    fn quoted_and_ordinary_words_stay() {
        assert_eq!(read("call \"tue\""), ("call tue".into(), None, None));
        assert_eq!(read("Friday's notes"), ("Friday's notes".into(), None, None));
        assert_eq!(read("mon report"), ("mon report".into(), None, None));
        assert_eq!(read("  spaced   out  "), ("spaced out".into(), None, None));
    }

    #[test]
    fn nothing_but_shortcuts_is_taken_as_typed() {
        assert_eq!(read("fri !h"), ("fri !h".into(), None, None));
    }

    fn repeats(text: &str) -> (String, Option<String>, Option<String>) {
        let q = parse(text, today(), true);
        (q.title, q.due, q.repeat)
    }

    #[test]
    fn every_something_repeats_and_gives_the_task_a_day() {
        // today() is Tuesday 3 Nov 2026.
        assert_eq!(repeats("water plants every day"), ("water plants".into(), day("2026-11-03"), Some("day".into())));
        assert_eq!(repeats("report every fri"), ("report".into(), day("2026-11-06"), Some("week".into())));
        assert_eq!(repeats("report every Friday"), ("report".into(), day("2026-11-06"), Some("week".into())));
        assert_eq!(repeats("gym every tue"), ("gym".into(), day("2026-11-10"), Some("week".into())));
        assert_eq!(repeats("rent every month"), ("rent".into(), day("2026-11-03"), Some("month".into())));
        assert_eq!(repeats("passport every year"), ("passport".into(), day("2026-11-03"), Some("year".into())));
        assert_eq!(repeats("standup every weekday"), ("standup".into(), day("2026-11-03"), Some("weekday".into())));
        assert_eq!(repeats("standup EVERY WEEK"), ("standup".into(), day("2026-11-03"), Some("week".into())));
    }

    #[test]
    fn a_day_beside_the_repeat_is_its_first_day_and_the_priority_still_reads() {
        assert_eq!(repeats("rent every month 1 dec"), ("rent".into(), day("2026-12-01"), Some("month".into())));
        assert_eq!(repeats("rent 1 dec every month"), ("rent".into(), day("2026-12-01"), Some("month".into())));
        assert_eq!(read("water plants !h every day"), ("water plants".into(), day("2026-11-03"), Some(2)));
        assert_eq!(read("water plants every day !h"), ("water plants".into(), day("2026-11-03"), Some(2)));
    }

    #[test]
    fn every_on_a_weekend_starts_on_the_next_weekday() {
        let saturday = NaiveDate::from_ymd_opt(2026, 11, 7).unwrap();
        assert_eq!(parse("standup every weekday", saturday, true).due, day("2026-11-09"));
    }

    #[test]
    fn only_a_rule_after_every_counts() {
        assert_eq!(repeats("every day"), ("every day".into(), None, None));
        assert_eq!(repeats("clean every other day"), ("clean every other day".into(), None, None));
        assert_eq!(repeats("every fortnight"), ("every fortnight".into(), None, None));
        assert_eq!(repeats("call \"every\" day"), ("call every day".into(), None, None));
        assert_eq!(repeats("party every night"), ("party every night".into(), None, None));
        assert_eq!(repeats("do it every today"), ("do it every".into(), day("2026-11-03"), None));
    }

    #[test]
    fn what_was_read_reads_back() {
        assert_eq!(describe(&parse("essay !h fri", today(), true)), "High · Fri 6 Nov");
        assert_eq!(describe(&parse("essay tmr", today(), true)), "Wed 4 Nov");
        assert_eq!(describe(&parse("essay !l", today(), true)), "Low");
        assert_eq!(describe(&parse("essay", today(), true)), "");
        assert_eq!(describe(&parse("essay !h every fri", today(), true)), "High · Fri 6 Nov · Every week");
        assert_eq!(describe(&parse("rent every month", today(), true)), "Tue 3 Nov · Every month");
    }
}
