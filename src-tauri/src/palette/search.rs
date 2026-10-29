//! What the palette shows for what's typed: matching, the days people name, actions, and the
//! suggestions before anything is typed. No drawing here, so all of it can be tested.

use std::collections::HashMap;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Weekday};

use super::settings::{self, Switch};
use super::AppEntry;
use crate::analysis::Category;

const MAX_APPS: usize = 6;
const RECENT_APPS: usize = 3;
const HOUR_MS: i64 = 60 * 60_000;
pub const MAX_FOCUS_MINUTES: u32 = 240;

// ---------- matching ----------

pub struct Match {
    pub score: f64,
    /// Positions (in characters) that matched, for underlining.
    pub at: Vec<usize>,
}

fn starts_word(chars: &[char], i: usize) -> bool {
    i == 0 || matches!(chars[i - 1], ' ' | '-' | '_' | '.' | '(') || (chars[i - 1].is_lowercase() && chars[i].is_uppercase())
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// The query's characters in order within `text`, each one either carrying on from the last
/// match or starting a word, so "vsc" finds Visual Studio Code but stray letters don't match.
/// Starting the text, and longer runs, score higher; None when it doesn't match.
pub fn fuzzy(query: &str, text: &str) -> Option<Match> {
    let original: Vec<char> = text.chars().collect();
    let t: Vec<char> = original.iter().map(|&c| lower(c)).collect();
    let q: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).map(lower).collect();
    if q.is_empty() {
        return Some(Match { score: 0.0, at: Vec::new() });
    }
    // The whole query, spaces and all, at the start of a word.
    let whole: Vec<char> = query.trim().chars().map(lower).collect();
    if let Some(i) = (0..=t.len().saturating_sub(whole.len())).find(|&i| t[i..].starts_with(&whole) && starts_word(&original, i)) {
        let score = 100.0 + if i == 0 { 60.0 } else { 30.0 } - t.len() as f64 / 10.0;
        return Some(Match { score, at: (i..i + whole.len()).collect() });
    }
    let mut at = Vec::new();
    let mut score = 0.0;
    let mut next = 0;
    for &c in &q {
        let carry_on = !at.is_empty() && t.get(next) == Some(&c);
        let i = if carry_on { next } else { (next..t.len()).find(|&j| t[j] == c && starts_word(&original, j))? };
        score += if carry_on { 6.0 } else { 10.0 };
        at.push(i);
        next = i + 1;
    }
    Some(Match { score: score - t.len() as f64 / 20.0, at })
}

// ---------- days ----------

const WEEKDAYS: [(&str, Weekday); 7] = [
    ("monday", Weekday::Mon),
    ("tuesday", Weekday::Tue),
    ("wednesday", Weekday::Wed),
    ("thursday", Weekday::Thu),
    ("friday", Weekday::Fri),
    ("saturday", Weekday::Sat),
    ("sunday", Weekday::Sun),
];
const MONTHS: [&str; 12] = ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"];

fn weekday(word: &str) -> Option<Weekday> {
    WEEKDAYS.iter().find(|(name, _)| word.len() >= 3 && name.starts_with(word)).map(|&(_, d)| d)
}

fn month(word: &str) -> Option<u32> {
    MONTHS.iter().position(|name| word.len() >= 3 && name.starts_with(word)).map(|m| m as u32 + 1)
}

/// A past day named the way people say it: "yesterday", "3 days ago", "tue", "last tue",
/// "14 oct", "oct 14". None when the words aren't a day.
pub fn parse_day(query: &str, today: NaiveDate) -> Option<NaiveDate> {
    let q = query.trim().to_lowercase();
    let words: Vec<&str> = q.split_whitespace().collect();
    match words.as_slice() {
        ["today"] => Some(today),
        ["yesterday"] => Some(today - Duration::days(1)),
        [n, "days" | "day" | "d", "ago"] => n.parse::<i64>().ok().filter(|&n| n < 1000).map(|n| today - Duration::days(n)),
        [nd, "ago"] if nd.ends_with('d') => nd[..nd.len() - 1].parse::<i64>().ok().filter(|&n| n < 1000).map(|n| today - Duration::days(n)),
        [name] if weekday(name).is_some() => Some(last_weekday(today, weekday(name)?, false)),
        ["last", name] if weekday(name).is_some() => Some(last_weekday(today, weekday(name)?, true)),
        [a, b] => {
            let (day, name) = if a.chars().all(|c| c.is_ascii_digit()) { (a, b) } else { (b, a) };
            let (day, m) = (day.parse::<u32>().ok()?, month(name)?);
            let date = NaiveDate::from_ymd_opt(today.year(), m, day)?;
            // A date still ahead this year means last year's.
            if date > today { NaiveDate::from_ymd_opt(today.year() - 1, m, day) } else { Some(date) }
        }
        _ => None,
    }
}

/// A stretch of time an answer covers, the way people ask for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Period {
    Today,
    Yesterday,
    ThisWeek,
    LastWeek,
    ThisMonth,
    LastMonth,
}

/// Longest first, so "last week" isn't read as "week".
const PERIODS: [(&str, Period); 8] = [
    ("this month", Period::ThisMonth),
    ("last month", Period::LastMonth),
    ("this week", Period::ThisWeek),
    ("last week", Period::LastWeek),
    ("yesterday", Period::Yesterday),
    ("today", Period::Today),
    ("month", Period::ThisMonth),
    ("week", Period::ThisWeek),
];

impl Period {
    /// As said in a title: "THE FINALS this week".
    pub fn label(self) -> &'static str {
        match self {
            Period::Today => "today",
            Period::Yesterday => "yesterday",
            Period::ThisWeek => "this week",
            Period::LastWeek => "last week",
            Period::ThisMonth => "this month",
            Period::LastMonth => "last month",
        }
    }

    /// The one before it, which the answer compares with.
    pub fn before(self) -> &'static str {
        match self {
            Period::Today => "yesterday",
            Period::Yesterday => "the day before",
            Period::ThisWeek => "last week",
            Period::LastWeek => "the week before",
            Period::ThisMonth => "last month",
            Period::LastMonth => "the month before",
        }
    }

    /// The first and last day it covers.
    pub fn days(self, today: NaiveDate) -> (NaiveDate, NaiveDate) {
        let monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);
        let first_of = |d: NaiveDate| d.with_day(1).unwrap_or(d);
        match self {
            Period::Today => (today, today),
            Period::Yesterday => (today - Duration::days(1), today - Duration::days(1)),
            Period::ThisWeek => (monday, monday + Duration::days(6)),
            Period::LastWeek => (monday - Duration::days(7), monday - Duration::days(1)),
            Period::ThisMonth => {
                let start = first_of(today);
                (start, first_of(start + Duration::days(32)) - Duration::days(1))
            }
            Period::LastMonth => {
                let end = first_of(today) - Duration::days(1);
                (first_of(end), end)
            }
        }
    }

    /// The same span just before it: yesterday for today, the week before for a week.
    pub fn previous_days(self, today: NaiveDate) -> (NaiveDate, NaiveDate) {
        let (first, last) = self.days(today);
        match self {
            Period::ThisMonth | Period::LastMonth => {
                let end = first - Duration::days(1);
                (end.with_day(1).unwrap_or(end), end)
            }
            _ => {
                let len = (last - first).num_days() + 1;
                (first - Duration::days(len), last - Duration::days(len))
            }
        }
    }
}

/// A query ending in a period, as "finals this week": the rest, and the period. Without one, or
/// with nothing before it, the whole query and None.
pub fn split_period(query: &str) -> (String, Option<Period>) {
    let q = query.trim().to_lowercase();
    for (words, period) in PERIODS {
        if let Some(rest) = q.strip_suffix(words).filter(|r| r.ends_with(' ')) {
            let rest = rest.trim();
            if !rest.is_empty() {
                return (rest.to_string(), Some(period));
            }
        }
    }
    (query.trim().to_string(), None)
}

/// "tue" on a Tuesday is today; "last tue" is always one before today.
fn last_weekday(today: NaiveDate, day: Weekday, before_today: bool) -> NaiveDate {
    let back = (today.weekday().num_days_from_monday() + 7 - day.num_days_from_monday()) % 7;
    let back = if back == 0 && before_today { 7 } else { back };
    today - Duration::days(back as i64)
}

// ---------- items ----------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Glyph {
    Focus,
    Day,
    Chart,
    Tag,
    Settings,
    Today,
    Alert,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tone {
    Plain,
    Accent,
    Warm,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Icon {
    /// A Start menu app's own icon, by its shortcut's path.
    App(String),
    /// A file, by its path.
    File(String),
    Glyph(Glyph, Tone),
}

/// Something Kairo has recorded time in, which can be asked about: an app or a site.
#[derive(Clone, Debug, PartialEq)]
pub struct Subject {
    pub name: String,
    /// `app:<exe>` or `site:<name>`, as categories and limits know it.
    pub key: String,
    pub category: Category,
    /// What to draw its icon from: an installed app's entry or its program, when known.
    pub icon: Option<String>,
    /// Time in it over the last 30 days, for picking between near matches.
    pub used_ms: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Launch(String),
    /// A Kairo view, and for Today or Week a date.
    Go(&'static str, Option<NaiveDate>),
    /// Kairo's dashboard: `None` for Home, or the tab by name.
    Dashboard(Option<&'static str>),
    StartFocus(u32, Option<String>),
    CheckUpdates,
    /// Puts an answer's line on the clipboard.
    CopyAnswer(Subject, Period),
    /// Saves tracked time as a CSV, backs everything up, or restores a backup, each asking where
    /// with the system's file dialog.
    ExportTime,
    Backup,
    Restore,
    /// Opens a file with its default app, or shows it in its folder.
    OpenFile(String),
    RevealFile(String),
    /// Flips a switch among the settings, in place.
    Toggle(Switch),
    /// Jumps to a setting's control, by its element id.
    Setting(&'static str),
    /// Does nothing, for a row that only says something.
    Nothing,
}

impl Action {
    /// Whether it opens a window of its own (Kairo's, or a file dialog), so the palette gets out
    /// of the way first rather than sitting over it.
    pub fn opens_window(&self) -> bool {
        matches!(self, Action::Go(..) | Action::Dashboard(..) | Action::ExportTime | Action::Backup | Action::Restore | Action::RevealFile(..) | Action::Setting(..))
    }

    /// Whether the palette stays open after it, to show the new state.
    pub fn keeps_palette_open(&self) -> bool {
        matches!(self, Action::Toggle(..))
    }
}

/// The card shown above the results while an item is selected.
#[derive(Clone, Debug, PartialEq)]
pub enum Preview {
    /// How a day went.
    Day(NaiveDate),
    /// Time in something over a period, day by day.
    Answer(Subject, Period),
    /// What a focus session would do once started.
    Focus(u32, Option<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub title: String,
    /// Characters of the title to underline.
    pub at: Vec<usize>,
    pub sub: Option<String>,
    /// Right-hand detail, like an app's time this month.
    pub detail: Option<String>,
    pub icon: Icon,
    pub score: f64,
    pub section: &'static str,
    pub action: Action,
    /// A card to show above the results while this item is selected.
    pub preview: Option<Preview>,
    /// What Ctrl+Enter (Command+Enter on a Mac) does instead, and the label shown for it.
    pub second: Option<(Action, &'static str)>,
}

impl Item {
    pub(super) fn new(title: impl Into<String>, icon: Icon, section: &'static str, action: Action) -> Self {
        Item { title: title.into(), at: Vec::new(), sub: None, detail: None, icon, score: 0.0, section, action, preview: None, second: None }
    }
}

/// What's known when the palette opens.
pub struct Context {
    pub today: NaiveDate,
    pub apps: Vec<AppEntry>,
    /// The running focus session's label, if one runs; `Some(None)` for an unlabelled one.
    pub session: Option<Option<String>>,
    /// Today's busiest apps by name, with their time.
    pub today_apps: Vec<(String, i64)>,
    /// Limits passed today: (name, used, limit).
    pub over_limits: Vec<(String, i64, i64)>,
    pub suggestions: bool,
    /// Whether files are looked for too.
    pub search_files: bool,
    /// Which of the settings' switches are on.
    pub switches: Vec<(Switch, bool)>,
    /// What Kairo has recorded lately, to answer questions about.
    pub subjects: Vec<Subject>,
    /// Daily limits by key, in milliseconds.
    pub limits: HashMap<String, i64>,
    /// How the weekly goals stand, like "7h 53m of 15h 00m productive"; None with none set.
    pub week_goals: Option<String>,
    /// The saved block lists' names; "focus 50 social" blocks the one called Social.
    pub block_lists: Vec<String>,
    /// The next meeting on the user's calendars: its title, when it starts, and how far off that
    /// is from now, in milliseconds.
    pub next_meeting: Option<(String, NaiveDateTime, i64)>,
}

pub fn fmt_dur(ms: i64) -> String {
    let mins = (ms + 30_000) / 60_000;
    match mins {
        m if m < 1 => "<1m".into(),
        m if m < 60 => format!("{m}m"),
        m => format!("{}h {:02}m", m / 60, m % 60),
    }
}

fn long_date(d: NaiveDate) -> String {
    d.format("%A %-d %B").to_string()
}

const PAGES: [(&str, &str, &str, Glyph, i64); 8] = [
    ("today", "Today", "today", Glyph::Today, -1),
    ("yesterday", "Yesterday", "today", Glyph::Day, 1),
    ("week this week", "This week", "week", Glyph::Day, -1),
    ("last week", "Last week", "week", Glyph::Day, 7),
    ("focus timer sessions", "Focus", "focus", Glyph::Focus, -1),
    ("patterns insights", "Patterns", "patterns", Glyph::Chart, -1),
    ("categories apps sites limits", "Categories", "categories", Glyph::Tag, -1),
    ("settings preferences options", "Chrono settings", "settings", Glyph::Settings, -1),
];

fn page_action(ctx: &Context, view: &'static str, days_back: i64) -> Action {
    Action::Go(view, (days_back >= 0).then(|| ctx.today - Duration::days(days_back)))
}

fn day_item(ctx: &Context, d: NaiveDate, score: f64) -> Item {
    let back = (ctx.today - d).num_days();
    let mut item = Item::new(long_date(d), Icon::Glyph(Glyph::Day, Tone::Plain), "Go to", Action::Go("today", Some(d)));
    item.sub = Some(match back {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        n => format!("{n} days ago"),
    });
    item.score = score;
    item.preview = Some(Preview::Day(d));
    item
}

/// The recorded app or site a query most likely means.
fn best_subject<'a>(ctx: &'a Context, query: &str) -> Option<(&'a Subject, Match)> {
    ctx.subjects
        .iter()
        .filter_map(|s| fuzzy(query, &s.name).map(|m| (s, m)))
        .max_by(|(a, x), (b, y)| (x.score + (a.used_ms as f64 / HOUR_MS as f64).min(20.0)).total_cmp(&(y.score + (b.used_ms as f64 / HOUR_MS as f64).min(20.0))))
}

/// "THE FINALS’" or "Chrome’s".
fn possessive(name: &str) -> String {
    if name.ends_with(['s', 'S']) {
        format!("{name}’")
    } else {
        format!("{name}’s")
    }
}

/// How much time went to something over a period: the answer, which opens that stretch in
/// Kairo, and when asked outright, rows to change its limit or copy the answer.
fn answer_items(ctx: &Context, subject: &Subject, m: Match, period: Period, asked: bool) -> Vec<Item> {
    let (first, _) = period.days(ctx.today);
    let (go, place) = match period {
        Period::Today | Period::Yesterday => (Action::Go("today", Some(first)), period.label()),
        Period::ThisWeek | Period::LastWeek => (Action::Go("week", Some(first)), period.label()),
        Period::ThisMonth | Period::LastMonth => (Action::Go("patterns", None), "Patterns"),
    };
    let preview = Some(Preview::Answer(subject.clone(), period));
    let icon = Icon::App(subject.icon.clone().unwrap_or_else(|| subject.key.clone()));
    let mut answer = Item::new(format!("{} {}", subject.name, period.label()), icon, "Answers", go);
    answer.at = m.at;
    answer.sub = Some(format!("Open {place} in Chrono"));
    answer.preview = preview.clone();
    if !asked {
        // A name alone most likely means the app; the answer waits below it.
        answer.score = m.score - 20.0;
        return vec![answer];
    }
    answer.score = 950.0;
    let limit = ctx.limits.get(&subject.key).copied();
    let mut change = Item::new(
        match limit {
            Some(_) => format!("Change {} daily limit", possessive(&subject.name)),
            None => format!("Set a daily limit for {}", subject.name),
        },
        Icon::Glyph(Glyph::Tag, Tone::Plain),
        "Answers",
        Action::Go("categories", None),
    );
    change.detail = limit.map(fmt_dur);
    change.score = 949.0;
    change.preview = preview.clone();
    let mut copy = Item::new("Copy the answer", Icon::Glyph(Glyph::Chart, Tone::Plain), "Answers", Action::CopyAnswer(subject.clone(), period));
    copy.score = 948.0;
    copy.preview = preview;
    vec![answer, change, copy]
}

fn focus_command(query: &str) -> Option<(u32, Option<String>)> {
    let mut words = query.split_whitespace();
    if !words.next()?.eq_ignore_ascii_case("focus") {
        return None;
    }
    let minutes: u32 = words.next()?.trim_end_matches(|c: char| c.is_ascii_alphabetic()).parse().ok()?;
    let mut rest: Vec<&str> = words.collect();
    if rest.first().is_some_and(|w| matches!(w.to_lowercase().as_str(), "m" | "min" | "mins" | "minutes")) {
        rest.remove(0);
    }
    let label = (!rest.is_empty()).then(|| rest.join(" "));
    Some((minutes.clamp(1, MAX_FOCUS_MINUTES), label))
}

/// What a typed query could mean, best first: the top match, then the rest by section.
pub fn search(ctx: &Context, query: &str) -> Vec<Item> {
    let mut items = Vec::new();

    if let Some((minutes, label)) = focus_command(query) {
        let mut item = Item::new(format!("Start a {minutes}-minute focus session"), Icon::Glyph(Glyph::Focus, Tone::Accent), "Focus", Action::StartFocus(minutes, label.clone()));
        let list = label.as_ref().and_then(|l| ctx.block_lists.iter().find(|n| n.eq_ignore_ascii_case(l.trim())));
        item.sub = Some(match (list, &label) {
            (Some(list), _) => format!("Blocks your “{list}” list"),
            (None, Some(l)) => format!("Labelled “{l}”"),
            (None, None) => "No label".into(),
        });
        item.score = 1000.0;
        item.preview = Some(Preview::Focus(minutes, label));
        items.push(item);
    }
    // "finals this week" asks about THE FINALS; "finals" alone also offers that, further down.
    match split_period(query) {
        (asked, Some(period)) => {
            if let Some((subject, m)) = best_subject(ctx, &asked) {
                items.extend(answer_items(ctx, subject, m, period, true));
            }
        }
        (_, None) => {
            if let Some((subject, m)) = best_subject(ctx, query).filter(|(_, m)| m.score >= 100.0) {
                items.extend(answer_items(ctx, subject, m, Period::ThisWeek, false));
            }
        }
    }
    if let Some(d) = parse_day(query, ctx.today) {
        items.push(day_item(ctx, d, 900.0));
    }
    for app in &ctx.apps {
        let Some(m) = fuzzy(query, &app.name) else { continue };
        let mut item = Item::new(app.name.clone(), Icon::App(app.path.clone()), "Apps", Action::Launch(app.path.clone()));
        item.at = m.at;
        item.detail = (app.used_ms > 0).then(|| format!("{} this month", fmt_dur(app.used_ms)));
        // Apps you actually use rise above ones you don't.
        item.score = m.score + (app.used_ms as f64 / HOUR_MS as f64).min(40.0);
        items.push(item);
    }
    for &(words, title, view, glyph, back) in &PAGES {
        let Some(m) = fuzzy(query, title).or_else(|| fuzzy(query, words)) else { continue };
        let mut item = Item::new(title, Icon::Glyph(glyph, Tone::Plain), "Go to", page_action(ctx, view, back));
        item.at = fuzzy(query, title).map(|m| m.at).unwrap_or_default();
        item.sub = Some("Chrono".into());
        item.score = m.score - 5.0;
        items.push(item);
    }
    // "meeting", "next", "calendar": the next meeting on the calendar.
    if let Some((title, at, in_ms)) = &ctx.next_meeting {
        if let Some(m) = fuzzy(query, "Next meeting").or_else(|| fuzzy(query, "calendar meetings next up")) {
            let day = match (at.date() - ctx.today).num_days() {
                0 => "Today".to_string(),
                1 => "Tomorrow".to_string(),
                _ => at.format("%A %-d %B").to_string(),
            };
            let mut item = Item::new(format!("Next meeting: {title}"), Icon::Glyph(Glyph::Day, Tone::Accent), "Answers", Action::Go("today", None));
            item.at = fuzzy(query, "Next meeting").map(|m| m.at).unwrap_or_default();
            item.sub = Some(format!("{day} at {}, in {}", at.format("%H:%M"), fmt_dur(*in_ms)));
            item.score = m.score - 1.0;
            items.push(item);
        }
    }
    // "goals", "weekly goals", "how's my week": the weekly goals, or where to set them.
    if let Some(m) = fuzzy(query, "Goals this week").or_else(|| fuzzy(query, "weekly goals how is my week progress")) {
        let (title, sub, action) = match &ctx.week_goals {
            Some(summary) => ("Goals this week", summary.clone(), Action::Go("week", None)),
            None => ("Set weekly goals", "Settings, Goals".to_string(), Action::Go("settings", None)),
        };
        let mut item = Item::new(title, Icon::Glyph(Glyph::Chart, Tone::Accent), "Answers", action);
        item.at = fuzzy(query, title).map(|m| m.at).unwrap_or_default();
        item.sub = Some(sub);
        item.score = m.score - 2.0;
        items.push(item);
    }
    let idle = ctx.session.is_none();
    let actions: [(&str, String, Glyph, Action, bool); 7] = [
        ("focus start session pomodoro 25", "Start a 25-minute focus session".into(), Glyph::Focus, Action::StartFocus(25, None), idle),
        ("focus start session 50", "Start a 50-minute focus session".into(), Glyph::Focus, Action::StartFocus(50, None), idle),
        ("focus start session 90", "Start a 90-minute focus session".into(), Glyph::Focus, Action::StartFocus(90, None), idle),
        ("focus end stop session", "End the focus session".into(), Glyph::Focus, Action::Go("focus", None), !idle),
        ("open chrono screen time tracker show window", "Open Chrono".into(), Glyph::Today, Action::Go("today", None), true),
        ("dashboard home apps kairo", "Dashboard".into(), Glyph::Day, Action::Dashboard(None), true),
        ("schedule scheduled focus plan routine", "Focus schedule".into(), Glyph::Focus, Action::Go("focus", None), true),
    ];
    for (words, title, glyph, action, offered) in actions {
        if !offered {
            continue;
        }
        // The better of the title and the words for it: "backup" means backing up, though
        // "Restore a backup" has it in its title.
        let Some(m) = [fuzzy(query, &title), fuzzy(query, words)].into_iter().flatten().max_by(|a, b| a.score.total_cmp(&b.score)) else { continue };
        let mut item = Item::new(title.clone(), Icon::Glyph(glyph, Tone::Accent), "Actions", action);
        item.at = fuzzy(query, &title).map(|m| m.at).unwrap_or_default();
        item.score = m.score - 8.0;
        if let Action::StartFocus(minutes, None) = item.action {
            item.preview = Some(Preview::Focus(minutes, None));
        }
        items.push(item);
    }

    for mut item in settings::search(query, &ctx.switches) {
        item.score -= 8.0;
        items.push(item);
    }

    items.sort_by(|a, b| b.score.total_cmp(&a.score));
    // Plenty of apps match a letter or two; a handful is enough.
    let mut app_count = 0;
    items.retain(|i| i.section != "Apps" || {
        app_count += 1;
        app_count <= MAX_APPS
    });
    // The best match leads; the rest stay together by section, sections in order of their best.
    let mut items = items.into_iter();
    let Some(top) = items.next() else { return Vec::new() };
    let rest: Vec<Item> = items.collect();
    let mut sections: Vec<&str> = Vec::new();
    for i in &rest {
        if !sections.contains(&i.section) {
            sections.push(i.section);
        }
    }
    let mut out = vec![top];
    for s in sections {
        out.extend(rest.iter().filter(|i| i.section == s).cloned());
    }
    out
}

/// Before anything is typed: rows of suggestions, recent apps as tiles, places as chips.
pub struct Suggestions {
    pub rows: Vec<Item>,
    pub tiles: Vec<Item>,
    pub chips: Vec<Item>,
}

pub fn suggestions(ctx: &Context) -> Suggestions {
    let chips = PAGES
        .iter()
        .filter(|p| ["Today", "Yesterday", "This week", "Focus", "Chrono settings"].contains(&p.1))
        .map(|&(_, title, view, glyph, back)| Item::new(title, Icon::Glyph(glyph, Tone::Plain), "Go to", page_action(ctx, view, back)))
        .collect();
    let mut rows = Vec::new();
    let mut tiles = Vec::new();
    if !ctx.suggestions {
        return Suggestions { rows, tiles, chips };
    }
    match &ctx.session {
        Some(label) => {
            let mut item = Item::new("Focus session in progress", Icon::Glyph(Glyph::Focus, Tone::Accent), "Suggested", Action::Go("focus", None));
            item.sub = Some(label.as_ref().map_or("Open the timer".into(), |l| format!("“{l}”")));
            rows.push(item);
        }
        None => {
            let mut item = Item::new("Start a 25-minute focus session", Icon::Glyph(Glyph::Focus, Tone::Accent), "Suggested", Action::StartFocus(25, None));
            item.sub = Some("Or type “focus 50 essay” for any length and a label".into());
            rows.push(item);
        }
    }
    for (name, used, limit) in &ctx.over_limits {
        let mut item = Item::new(format!("{name}: {} over today’s limit", fmt_dur(used - limit)), Icon::Glyph(Glyph::Alert, Tone::Warm), "Suggested", Action::Go("today", None));
        item.sub = Some(format!("{} of {}", fmt_dur(*used), fmt_dur(*limit)));
        rows.push(item);
    }
    // Today's busiest apps, where the Start menu has a way to open them.
    for (name, ms) in &ctx.today_apps {
        let Some(app) = ctx.apps.iter().find(|a| a.name.eq_ignore_ascii_case(name)) else { continue };
        let mut item = Item::new(app.name.clone(), Icon::App(app.path.clone()), "Recent apps", Action::Launch(app.path.clone()));
        item.detail = Some(fmt_dur(*ms));
        tiles.push(item);
        if tiles.len() == RECENT_APPS {
            break;
        }
    }
    Suggestions { rows, tiles, chips }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, hours: i64) -> AppEntry {
        AppEntry { name: name.into(), path: format!(r"C:\Start\{name}.lnk"), used_ms: hours * HOUR_MS }
    }

    fn ctx() -> Context {
        Context {
            today: NaiveDate::from_ymd_opt(2026, 10, 26).unwrap(), // a Monday
            apps: vec![app("Cursor", 61), app("Brave", 19), app("Discord", 7), app("CurseForge", 0), app("Visual Studio Code", 0), app("Notepad", 0)],
            session: None,
            today_apps: vec![("Cursor".into(), 2 * HOUR_MS), ("THE FINALS".into(), HOUR_MS), ("Brave".into(), HOUR_MS / 2)],
            over_limits: vec![("THE FINALS".into(), 132 * 60_000, 120 * 60_000)],
            suggestions: true,
            search_files: true,
            switches: Vec::new(),
            subjects: vec![subject("THE FINALS", "app:discovery", 30), subject("YouTube", "site:youtube", 12), subject("Cursor", "app:cursor", 61)],
            limits: HashMap::from([("app:discovery".to_string(), 2 * HOUR_MS)]),
            week_goals: Some("7h 53m of 15h 00m productive".into()),
            block_lists: vec!["Social".into()],
            next_meeting: Some(("Design review".into(), NaiveDate::from_ymd_opt(2026, 10, 26).unwrap().and_hms_opt(14, 0, 0).unwrap(), 35 * 60_000)),
        }
    }

    fn subject(name: &str, key: &str, hours: i64) -> Subject {
        Subject { name: name.into(), key: key.into(), category: Category::Distracting, icon: None, used_ms: hours * HOUR_MS }
    }

    fn titles(items: &[Item]) -> Vec<&str> {
        items.iter().map(|i| i.title.as_str()).collect()
    }

    #[test]
    fn the_dashboard_and_both_settings_are_a_few_letters_away() {
        let c = ctx();
        assert_eq!(search(&c, "dashboard")[0].action, Action::Dashboard(None));
        let settings = search(&c, "settings");
        assert!(settings.iter().any(|i| i.title == "Kairo settings" && i.action == Action::Dashboard(Some("settings"))));
        assert!(settings.iter().any(|i| i.title == "Chrono settings" && i.action == Action::Go("settings", None)));
        // The theme leads to Kairo's settings, to its own control.
        let theme = &search(&c, "theme")[0];
        assert_eq!((theme.title.as_str(), theme.action.clone()), ("Theme", Action::Setting("appearance")));
        assert!(theme.action.opens_window() && !theme.action.keeps_palette_open());
    }

    #[test]
    fn data_and_goals_are_a_few_letters_away() {
        let c = ctx();
        assert_eq!(search(&c, "export")[0].action, Action::ExportTime);
        // A switch flips in place and keeps the palette open.
        let flip = &search(&Context { switches: vec![(Switch::Evening, true)], ..ctx() }, "evening")[0];
        assert_eq!((flip.action.clone(), flip.detail.as_deref()), (Action::Toggle(Switch::Evening), Some("On")));
        assert!(flip.action.keeps_palette_open() && !Action::Backup.keeps_palette_open());
        assert_eq!(search(&c, "backup")[0].action, Action::Backup);
        assert!(search(&c, "restore").iter().any(|i| i.action == Action::Restore));
        let goals = search(&c, "weekly goals");
        assert_eq!((goals[0].title.as_str(), goals[0].sub.as_deref()), ("Goals this week", Some("7h 53m of 15h 00m productive")));
        assert_eq!(goals[0].action, Action::Go("week", None));
        // With none set, it points to where they're set.
        let none = Context { week_goals: None, ..ctx() };
        assert_eq!(search(&none, "goals")[0].action, Action::Go("settings", None));
        assert!(titles(&search(&c, "schedule")).contains(&"Focus schedule"));
        let next = search(&c, "next meeting");
        assert_eq!((next[0].title.as_str(), next[0].sub.as_deref()), ("Next meeting: Design review", Some("Today at 14:00, in 35m")));
    }

    #[test]
    fn matching_takes_word_starts_and_runs_but_not_stray_letters() {
        assert!(fuzzy("vsc", "Visual Studio Code").is_some());
        assert_eq!(fuzzy("cur", "Cursor").unwrap().at, vec![0, 1, 2]);
        assert!(fuzzy("cur", "Check for updates").is_none());
        assert!(fuzzy("ursor", "Cursor").is_none());
        assert!(fuzzy("cur", "Cursor").unwrap().score > fuzzy("cur", "CurseForge").unwrap().score);
    }

    #[test]
    fn days_are_read_the_way_people_say_them() {
        let today = ctx().today;
        let d = |y, m, d| NaiveDate::from_ymd_opt(y, m, d);
        assert_eq!(parse_day("yesterday", today), d(2026, 10, 25));
        assert_eq!(parse_day("3 days ago", today), d(2026, 10, 23));
        assert_eq!(parse_day("3d ago", today), d(2026, 10, 23));
        assert_eq!(parse_day("tue", today), d(2026, 10, 20));
        assert_eq!(parse_day("mon", today), d(2026, 10, 26));
        assert_eq!(parse_day("last mon", today), d(2026, 10, 19));
        assert_eq!(parse_day("14 oct", today), d(2026, 10, 14));
        assert_eq!(parse_day("oct 14", today), d(2026, 10, 14));
        assert_eq!(parse_day("30 dec", today), d(2025, 12, 30));
        assert_eq!(parse_day("31 feb", today), None);
        assert_eq!(parse_day("cursor", today), None);
    }

    #[test]
    fn search_leads_with_the_best_match_and_groups_the_rest() {
        let c = ctx();
        // Apps first; what Cursor took this week comes after them.
        assert_eq!(titles(&search(&c, "cur")), ["Cursor", "CurseForge", "Cursor this week"]);
        assert_eq!(titles(&search(&c, "vsc")), ["Visual Studio Code"]);
        assert_eq!(search(&c, "last tue")[0].preview, Some(Preview::Day(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap())));
        let focus = &search(&c, "focus 50 essay")[0];
        assert_eq!(focus.action, Action::StartFocus(50, Some("essay".into())));
        assert_eq!(focus.sub.as_deref(), Some("Labelled “essay”"));
        let social = &search(&c, "focus 25 social")[0];
        assert_eq!((social.action.clone(), social.sub.as_deref()), (Action::StartFocus(25, Some("social".into())), Some("Blocks your “Social” list")));
        let found = search(&c, "foc");
        assert_eq!(found[0].title, "Focus");
        assert!(found[1..].iter().all(|i| i.section == "Actions" || i.section == "Settings"));
        assert!(search(&c, "zzzq").is_empty());
    }

    #[test]
    fn suggestions_offer_focus_limits_and_todays_apps() {
        let s = suggestions(&ctx());
        assert_eq!(titles(&s.rows), ["Start a 25-minute focus session", "THE FINALS: 12m over today’s limit"]);
        // THE FINALS isn't in the Start menu, so it can't be a tile.
        assert_eq!(titles(&s.tiles), ["Cursor", "Brave"]);
        assert_eq!(titles(&s.chips), ["Today", "Yesterday", "This week", "Focus", "Chrono settings"]);
        let quiet = suggestions(&Context { suggestions: false, ..ctx() });
        assert!(quiet.rows.is_empty() && quiet.tiles.is_empty() && quiet.chips.len() == 5);
    }

    #[test]
    fn periods_end_a_question() {
        assert_eq!(split_period("finals this week"), ("finals".into(), Some(Period::ThisWeek)));
        assert_eq!(split_period("youtube last week"), ("youtube".into(), Some(Period::LastWeek)));
        assert_eq!(split_period("cursor today"), ("cursor".into(), Some(Period::Today)));
        assert_eq!(split_period("today"), ("today".into(), None));
        assert_eq!(split_period("homeweek"), ("homeweek".into(), None));
        let today = ctx().today;
        let d = |m, d| NaiveDate::from_ymd_opt(2026, m, d).unwrap();
        assert_eq!(Period::ThisWeek.days(today), (d(10, 26), d(11, 1)));
        assert_eq!(Period::LastWeek.days(today), (d(10, 19), d(10, 25)));
        assert_eq!(Period::ThisMonth.days(today), (d(10, 1), d(10, 31)));
        assert_eq!(Period::LastMonth.days(today), (d(9, 1), d(9, 30)));
        assert_eq!(Period::ThisMonth.previous_days(today), (d(9, 1), d(9, 30)));
        assert_eq!(Period::Today.previous_days(today), (d(10, 25), d(10, 25)));
    }

    #[test]
    fn questions_get_answers_with_their_follow_ups() {
        let c = ctx();
        let found = search(&c, "finals this week");
        assert_eq!(titles(&found), ["THE FINALS this week", "Change THE FINALS’ daily limit", "Copy the answer"]);
        assert_eq!(found[0].action, Action::Go("week", NaiveDate::from_ymd_opt(2026, 10, 26)));
        assert_eq!(found[1].detail.as_deref(), Some("2h 00m"));
        assert!(matches!(&found[2].preview, Some(Preview::Answer(s, Period::ThisWeek)) if s.key == "app:discovery"));
        assert_eq!(titles(&search(&c, "youtube today"))[1], "Set a daily limit for YouTube");
        // A name alone opens the app first; the answer comes after.
        let cursor = search(&c, "cursor");
        assert_eq!(titles(&cursor)[..2], ["Cursor", "Cursor this week"]);
        assert!(matches!(search(&c, "focus 25")[0].preview, Some(Preview::Focus(25, None))));
    }
}
