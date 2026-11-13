//! What the palette shows for what's typed: matching, the days people name, actions, and the
//! suggestions before anything is typed. No drawing here, so all of it can be tested.

use std::collections::HashMap;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Weekday};

use super::calc;
use super::commands;
use super::mode::{self, Mode};
use super::settings::{self, Switch};
use super::AppEntry;
use crate::analysis::Category;
use crate::system::{Power, Program};

const MAX_APPS: usize = 6;
/// Rows `tasks` lists at most.
const MAX_TASKS: usize = 8;
/// Web searches by a short word before what's searched for: the word, the site's name, and the
/// address the search goes on the end of.
pub const WEB: [(&str, &str, &str); 6] = [
    ("g", "Google", "https://www.google.com/search?q="),
    ("yt", "YouTube", "https://www.youtube.com/results?search_query="),
    ("ddg", "DuckDuckGo", "https://duckduckgo.com/?q="),
    ("w", "Wikipedia", "https://en.wikipedia.org/w/index.php?search="),
    ("gh", "GitHub", "https://github.com/search?q="),
    ("maps", "Google Maps", "https://www.google.com/maps/search/"),
];
/// Window rows shown at most.
const MAX_WINDOWS: usize = 8;
/// A window that matches by its app's name ranks this far below one that matches by title.
const WINDOW_APP_PENALTY: f64 = 20.0;
/// Window rows stay below commands (a focus session, a day, an answer), whatever they match.
const MAX_WINDOW_SCORE: f64 = 300.0;
/// Where `windows` lists them: below an app whose name starts with the word, like Windows
/// Terminal, so typing it and pressing Enter still opens that app.
const LISTED_WINDOW_SCORE: f64 = 140.0;
/// How long a session started on a task from the palette runs.
const TASK_MINUTES: u32 = 25;
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
    Calc,
    Search,
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
    /// A focus session: minutes, label, and the task it's for, by id.
    StartFocus(u32, Option<String>, Option<String>),
    /// A saved focus mode, by name, and the task it's for by id if the query named one: its
    /// session, its block list and its apps.
    RunMode(String, Option<String>),
    /// A pomodoro cycle: minutes per round and per break (the saved ones when None), label, and
    /// the task it's for, by id.
    Pomodoro(Option<u32>, Option<u32>, Option<String>, Option<String>),
    /// Empties the clipboard history.
    ClearClips,
    /// Locks or sleeps the computer, at once.
    Power(Power),
    /// The first Enter on logging out, restarting or shutting down: nothing happens yet but the
    /// row asks again. The text is what was typed.
    ConfirmPower(Power, String),
    /// The second Enter.
    PowerNow(Power),
    /// Leaves what was waiting to be confirmed.
    CancelPower,
    /// Ends every process of the program with this name.
    Kill(String),
    /// Adds a task to Ledger, from quick-add text.
    AddTask(String),
    /// Opens Ledger.
    OpenLedger,
    /// Opens Ledger's Plan view: the day planned, or wrapped up.
    OpenPlan,
    /// Opens the app ring.
    OpenRing,
    /// Opens a note in Ledger, by id.
    OpenNote(String),
    /// Starts a note with this text and opens it in the pop-out window.
    NewNote(String),
    /// Opens a note in the pop-out window, by id; none for the list of notes.
    PopOutNote(Option<String>),
    /// Opens the VirusTotal scan window, looking up this link or hash when there is one.
    Scan(Option<String>),
    /// Opens the speed test window, which starts a test.
    SpeedTest,
    /// Ticks a task off, by id.
    FinishTask(String),
    /// Opens a web address in the default browser.
    OpenUrl(String),
    /// Brings an open window forward, by the system's id for it.
    FocusWindow(String),
    /// Asks an open window to close, by the system's id for it.
    CloseWindow(String),
    CheckUpdates,
    /// Puts an answer's line on the clipboard.
    CopyAnswer(Subject, Period),
    /// Puts this text on the clipboard.
    Copy(String),
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
    /// Puts the palette in a mode, shown as a bubble in the input (mode.rs).
    EnterMode(Mode),
    /// Switches to a theme, by its ground's id, or "system".
    Theme(&'static str),
    /// Pauses, resumes, extends or ends the running focus session.
    Session(SessionDo),
    /// Allows what the blocked window is blocking, by its key, for a few minutes.
    Allow(String),
    /// Brings the blocked window forward.
    ShowBlocked,
    /// Starts a quick timer: minutes and a name.
    StartTimer(u32, Option<String>),
    /// Cancels a quick timer, by id.
    CancelTimer(i64),
    /// Pauses tracking, for this many minutes when given.
    PauseTracking(Option<u32>),
    ResumeTracking,
    /// Does nothing, for a row that only says something.
    Nothing,
}

/// What can be done to a running focus session.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SessionDo {
    Pause,
    Resume,
    /// Adds this many minutes.
    Extend(u32),
    End,
    /// Ends a pomodoro break and starts the next round.
    SkipBreak,
}

impl Action {
    /// Whether it opens a window of its own (Kairo's, or a file dialog), so the palette gets out
    /// of the way first rather than sitting over it.
    pub fn opens_window(&self) -> bool {
        matches!(self, Action::Go(..) | Action::Dashboard(..) | Action::OpenUrl(..) | Action::ExportTime | Action::Backup | Action::Restore | Action::RevealFile(..) | Action::Setting(..) | Action::ShowBlocked)
    }

    /// Whether the palette stays open after it, to show the new state. A row with nothing to do
    /// on Enter (a running timer, which Ctrl+Enter cancels) leaves the palette as it is too.
    pub fn keeps_palette_open(&self) -> bool {
        matches!(self, Action::Toggle(..) | Action::EnterMode(..) | Action::Nothing) || self.acts_in_place()
    }

    /// Whether it changes something the palette shows and leaves the palette open, which then
    /// reads how things stand (`Live`) and asks again: a theme can be tried after another, and a
    /// paused session offers to resume.
    pub fn acts_in_place(&self) -> bool {
        matches!(
            self,
            Action::Theme(..)
                | Action::Session(SessionDo::Pause | SessionDo::Resume | SessionDo::Extend(..))
                | Action::CancelTimer(..)
                | Action::ConfirmPower(..)
                | Action::CancelPower
                | Action::PauseTracking(..)
                | Action::ResumeTracking
        )
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

impl Preview {
    /// Whether the card waits for Enter (or a click) on its row rather than showing as the row
    /// is selected: a time chart is tall, and coming and going under the pointer it pushed the
    /// rows away from it.
    pub fn on_enter(&self) -> bool {
        matches!(self, Preview::Answer(..))
    }
}

/// A row's second line while its card, asked for with Enter, is showing: what Enter does now.
pub fn enter_again(sub: Option<&str>) -> String {
    let mut chars = sub.unwrap_or_default().chars();
    match chars.next() {
        Some(first) => format!("Enter again to {}{}", first.to_lowercase(), chars.as_str()),
        None => "Enter again to open it".into(),
    }
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
    /// Whether it only says something ("Not a sum yet", "Looking for files…"): drawn as a note,
    /// passed over by the arrows, with nothing for Enter to do.
    pub fn is_note(&self) -> bool {
        self.action == Action::Nothing && self.second.is_none()
    }

    pub(super) fn new(title: impl Into<String>, icon: Icon, section: &'static str, action: Action) -> Self {
        Item { title: title.into(), at: Vec::new(), sub: None, detail: None, icon, score: 0.0, section, action, preview: None, second: None }
    }
}

/// An open window, as the palette shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowEntry {
    pub id: String,
    pub title: String,
    /// Its app's name; empty when unknown.
    pub app: String,
    /// The program's path, for its icon.
    pub icon: Option<String>,
}

/// A note as the palette searches it: its title, and the start of its text in lower case.
#[derive(Clone, Debug, Default)]
pub struct NoteEntry {
    pub id: String,
    pub title: String,
    pub text: String,
}

/// The running focus session, as the palette says it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionNow {
    pub label: Option<String>,
    pub left_ms: i64,
    pub paused: bool,
}

/// One thing copied lately, for the `clip` command.
#[derive(Clone, Debug, PartialEq)]
pub struct ClipNow {
    pub text: String,
    /// How long ago it was copied.
    pub age_ms: i64,
}

/// A pomodoro cycle that's on, as the palette says it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CycleNow {
    /// The round running, or on a break the one next.
    pub round: u32,
    pub rounds: u32,
    pub on_break: bool,
    pub left_ms: i64,
    /// The break is over and the next round waits for someone to come back.
    pub waiting: bool,
}

/// A quick timer still to ring (timers.rs).
#[derive(Clone, Debug, PartialEq)]
pub struct TimerNow {
    pub id: i64,
    /// Its name, or its length when it has none.
    pub name: String,
    pub left_ms: i64,
    /// When it rings, in local time.
    pub ends: NaiveDateTime,
}

/// What the palette's own commands change, so it's read again after one runs in place.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Live {
    /// The local time this was read at.
    pub now: NaiveDateTime,
    /// The running focus session, if one runs.
    pub session: Option<SessionNow>,
    /// The pomodoro cycle, while one is on.
    pub cycle: Option<CycleNow>,
    /// The latest copies, newest first; None while the clipboard history is off. Only ever shown
    /// for a query led by `clip`.
    pub clips: Option<Vec<ClipNow>>,
    /// What is waiting for its second Enter, with the text it was asked for in.
    pub pending_power: Option<(Power, String)>,
    /// Whether strict mode holds the session: ending it and allowing a blocked app wait first.
    pub strict: bool,
    /// None while tracking; paused, until the time inside when there is one.
    pub tracking_paused: Option<Option<NaiveDateTime>>,
    /// What the blocked window is blocking, as (label, key), while it's up.
    pub blocked: Option<(String, String)>,
    /// The quick timers, soonest first.
    pub timers: Vec<TimerNow>,
    /// The theme in use: a ground's id, or "system".
    pub theme: String,
}

/// What's known when the palette opens.
pub struct Context {
    pub today: NaiveDate,
    pub apps: Vec<AppEntry>,
    pub live: Live,
    /// Today's busiest apps by name, with their time.
    pub today_apps: Vec<(String, i64)>,
    /// Limits passed today: (name, used, limit).
    pub over_limits: Vec<(String, i64, i64)>,
    pub suggestions: bool,
    /// Whether the palette opens as only its search bar: nothing shows until something's typed.
    pub compact: bool,
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
    /// The saved focus modes, in the order they were made.
    pub modes: Vec<crate::focus_modes::FocusMode>,
    /// The open tasks, in Ledger's order.
    pub tasks: Vec<crate::tasks::Task>,
    /// Whether a typed date like 11/7 means 11 July (quick_add.rs).
    pub day_first: bool,
    /// Ledger's notes, pinned then newest.
    pub notes: Vec<NoteEntry>,
    /// The open windows when the palette opened, front to back.
    pub windows: Vec<WindowEntry>,
    /// Whether a window can be asked to close here (not on a Mac without Accessibility).
    pub can_close_windows: bool,
    /// The next meeting on the user's calendars: its title, when it starts, and how far off that
    /// is from now, in milliseconds.
    pub next_meeting: Option<(String, NaiveDateTime, i64)>,
    /// The programs running as the palette opened, for `kill`.
    pub programs: Vec<Program>,
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

/// Minutes at the start of `words`, as "50", "50m" or "50 min", kept to 1 to 240, and the words
/// after them joined as a label. None when the first word isn't a number of minutes.
pub(super) fn minutes_and_label(words: &[&str]) -> Option<(u32, Option<String>)> {
    let minutes: u32 = words.first()?.trim_end_matches(|c: char| c.is_ascii_alphabetic()).parse().ok()?;
    let mut rest = &words[1..];
    if rest.first().is_some_and(|w| matches!(w.to_lowercase().as_str(), "m" | "min" | "mins" | "minute" | "minutes")) {
        rest = &rest[1..];
    }
    let label = (!rest.is_empty()).then(|| rest.join(" "));
    Some((minutes.clamp(1, MAX_FOCUS_MINUTES), label))
}

fn focus_command(query: &str) -> Option<(u32, Option<String>)> {
    let words: Vec<&str> = query.split_whitespace().collect();
    if !words.first()?.eq_ignore_ascii_case("focus") {
        return None;
    }
    minutes_and_label(&words[1..])
}

/// The open task a focus label is for: the best match with the whole label at the start of a
/// word in its title, the older one on a tie; None when none matches that well.
pub(super) fn task_for<'a>(tasks: &'a [crate::tasks::Task], label: &str) -> Option<&'a crate::tasks::Task> {
    let mut best: Option<(&crate::tasks::Task, f64)> = None;
    for task in tasks {
        let Some(m) = fuzzy(label, &task.title).filter(|m| m.score >= 100.0) else { continue };
        if best.is_none_or(|(b, score)| m.score > score || (m.score == score && task.created_ms < b.created_ms)) {
            best = Some((task, m.score));
        }
    }
    best.map(|(task, _)| task)
}

/// What follows `prefix` at the start of `query`, ignoring case; None without it. Compared on the
/// query itself, so a character whose lower case is longer can't shift where it's cut.
pub(super) fn after<'a>(query: &'a str, prefix: &str) -> Option<&'a str> {
    query.get(..prefix.len()).filter(|head| head.eq_ignore_ascii_case(prefix)).map(|_| &query[prefix.len()..])
}

/// The row that adds `title` to Ledger as a task, saying what quick-add reads off its end.
fn add_task_item(ctx: &Context, title: &str) -> Item {
    let read = crate::quick_add::parse(title, ctx.today, ctx.day_first);
    let mut item = Item::new(format!("Add task “{}”", read.title), Icon::Glyph(Glyph::Focus, Tone::Plain), "Tasks", Action::AddTask(title.to_string()));
    let shown = crate::quick_add::describe(&read);
    item.sub = Some(if shown.is_empty() { "In Ledger".into() } else { format!("{shown} · In Ledger") });
    item
}

/// Rows for the task list: `tasks` lists the open ones (more words filter them), and
/// `task <title>` or `add task <title>` offers to add one.
fn task_items(ctx: &Context, query: &str) -> Vec<Item> {
    let q = query.trim();
    // "add task" always means it; a bare "task" ranks below an app it names, like Task Manager.
    let adding = [("add task ", 1000.0), ("task ", 90.0)].into_iter().find_map(|(p, score)| after(q, p).map(|t| (t.trim(), score))).filter(|(t, _)| !t.is_empty());
    if let Some((title, score)) = adding {
        let mut item = add_task_item(ctx, title);
        item.score = score;
        return vec![item];
    }
    let rest = if q.eq_ignore_ascii_case("tasks") {
        ""
    } else if let Some(rest) = after(q, "tasks ") {
        rest.trim()
    } else {
        return Vec::new();
    };
    task_rows(ctx, rest)
}

/// The open tasks whose title `rest` matches, all of them for nothing, in Ledger's order.
fn task_rows(ctx: &Context, rest: &str) -> Vec<Item> {
    ctx.tasks
        .iter()
        .filter_map(|t| fuzzy(rest, &t.title).map(|m| (t, m)))
        .take(MAX_TASKS)
        .enumerate()
        .map(|(i, (t, m))| {
            let mut item = Item::new(t.title.clone(), Icon::Glyph(Glyph::Focus, Tone::Plain), "Tasks", Action::StartFocus(TASK_MINUTES, None, Some(t.id.clone())));
            item.at = m.at;
            item.sub = Some(format!("Start a {TASK_MINUTES}-minute focus session on it"));
            // When it's due, or failing that the time spent on it.
            let due = t.due_day.as_deref().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()).map(|d| d.format("%a %-d %b").to_string());
            item.detail = due.or_else(|| (t.focused_ms > 0).then(|| fmt_dur(t.focused_ms)));
            item.second = Some((Action::FinishTask(t.id.clone()), "Mark done"));
            // Above the other matches, in the list's own order.
            item.score = 1100.0 - i as f64;
            item
        })
        .collect()
}

/// A new note, starting with `text` (none for an empty one), opened in the small window.
fn new_note_item(text: &str) -> Item {
    let title = if text.is_empty() { "New note".to_string() } else { format!("New note “{text}”") };
    let mut item = Item::new(title, Icon::Glyph(Glyph::Tag, Tone::Plain), "Notes", Action::NewNote(text.to_string()));
    item.sub = Some("In a small window".into());
    item
}

fn open_note_item(n: &NoteEntry) -> Item {
    let mut item = Item::new(n.title.clone(), Icon::Glyph(Glyph::Tag, Tone::Plain), "Notes", Action::OpenNote(n.id.clone()));
    item.sub = Some("Open in Ledger".into());
    item.second = Some((Action::PopOutNote(Some(n.id.clone())), "Pop out"));
    item
}

/// The list of notes in the small window, above the notes themselves.
fn pop_out_notes_item() -> Item {
    let mut item = Item::new("All notes in a small window", Icon::Glyph(Glyph::Tag, Tone::Plain), "Notes", Action::PopOutNote(None));
    item.sub = Some("Pick one, or start a new one, beside your work".into());
    item.score = 1101.0;
    item
}

/// The notes whose title or text has `rest`, all of them for nothing, pinned then newest.
fn note_rows(ctx: &Context, rest: &str) -> Vec<Item> {
    let rest_lower = rest.to_lowercase();
    ctx.notes
        .iter()
        .filter(|n| rest.is_empty() || fuzzy(rest, &n.title).is_some() || n.text.contains(&rest_lower))
        .take(MAX_TASKS)
        .enumerate()
        .map(|(i, n)| {
            let mut item = open_note_item(n);
            item.at = fuzzy(rest, &n.title).map(|m| m.at).unwrap_or_default();
            item.score = 1100.0 - i as f64;
            item
        })
        .collect()
}

/// `vt`, `scan` or `virustotal` and a link or hash: that, looked up as the scan window opens.
/// Anything else after them is an ordinary search.
fn scan_item(query: &str) -> Option<Item> {
    let q = query.trim();
    let rest = ["virustotal ", "vt ", "scan "].iter().find_map(|p| after(q, p)).map(str::trim)?;
    crate::virustotal::classify(rest)?;
    let mut item = Item::new(format!("Check “{rest}” on VirusTotal"), Icon::Glyph(Glyph::Search, Tone::Accent), "Actions", Action::Scan(Some(rest.to_string())));
    item.sub = Some("In the scan window".into());
    item.score = 1000.0;
    Some(item)
}

/// Ledger's notes shown for any query, at most this many.
const MAX_NOTES: usize = 3;

/// Rows for notes: `note <text>` or `new note <text>` starts one; `notes` lists them (more words
/// search them); and any other query offers a few whose title or text has it, below apps.
fn note_items(ctx: &Context, query: &str) -> Vec<Item> {
    let q = query.trim();
    // "new note" or "create note" alone starts an empty one.
    if ["new note", "create note"].iter().any(|c| q.eq_ignore_ascii_case(c)) {
        let mut item = new_note_item("");
        item.score = 1000.0;
        return vec![item];
    }
    let starting = [("new note ", 1000.0), ("create note ", 1000.0), ("note ", 90.0)]
        .into_iter()
        .find_map(|(p, score)| after(q, p).map(|t| (t.trim(), score)))
        .filter(|(t, _)| !t.is_empty());
    if let Some((text, score)) = starting {
        let mut item = new_note_item(text);
        item.score = score;
        return vec![item];
    }
    let listing = if q.eq_ignore_ascii_case("notes") { Some("") } else { after(q, "notes ").map(str::trim) };
    if let Some(rest) = listing {
        let mut rows = note_rows(ctx, rest);
        if rest.is_empty() {
            rows.insert(0, pop_out_notes_item());
        }
        return rows;
    }
    if q.chars().count() < 2 {
        return Vec::new();
    }
    let lower = q.to_lowercase();
    let mut found: Vec<Item> = ctx
        .notes
        .iter()
        .filter_map(|n| {
            let by_title = fuzzy(q, &n.title);
            let score = match &by_title {
                // Below an app matched as well.
                Some(m) => m.score - 30.0,
                None if n.text.contains(&lower) => 40.0,
                None => return None,
            };
            let mut item = open_note_item(n);
            item.at = by_title.map(|m| m.at).unwrap_or_default();
            item.score = score;
            Some(item)
        })
        .collect();
    found.sort_by(|a, b| b.score.total_cmp(&a.score));
    found.truncate(MAX_NOTES);
    found
}

/// Rows for open windows: those matching by title, or less well by their app's name, best first;
/// `windows` alone lists them all, front to back.
fn window_items(ctx: &Context, query: &str) -> Vec<Item> {
    let q = query.trim();
    window_rows(ctx, q, q.eq_ignore_ascii_case("windows"))
}

/// Open windows matching `q`, or with `all` every one of them, front to back.
fn window_rows(ctx: &Context, q: &str, all: bool) -> Vec<Item> {
    let mut found: Vec<(f64, Vec<usize>, &WindowEntry)> = if all {
        ctx.windows.iter().enumerate().map(|(i, w)| (LISTED_WINDOW_SCORE - i as f64, Vec::new(), w)).collect()
    } else {
        ctx.windows
            .iter()
            .filter_map(|w| {
                let by_title = fuzzy(q, &w.title).map(|m| (m.score, m.at));
                let by_app = fuzzy(q, &w.app).filter(|_| !w.app.is_empty()).map(|m| (m.score - WINDOW_APP_PENALTY, Vec::new()));
                let (score, at) = [by_title, by_app].into_iter().flatten().max_by(|a, b| a.0.total_cmp(&b.0))?;
                Some((score.min(MAX_WINDOW_SCORE), at, w))
            })
            .collect()
    };
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found
        .into_iter()
        .take(MAX_WINDOWS)
        .map(|(score, at, w)| {
            let mut item = Item::new(w.title.clone(), Icon::App(w.icon.clone().unwrap_or_default()), "Windows", Action::FocusWindow(w.id.clone()));
            item.at = at;
            item.sub = (!w.app.is_empty()).then(|| w.app.clone());
            item.second = ctx.can_close_windows.then(|| (Action::CloseWindow(w.id.clone()), "Close window"));
            item.score = score;
            item
        })
        .collect()
}

/// Text made safe for a web address: everything but ASCII letters, digits and `-_.~` written as
/// %XX bytes, spaces included.
fn url_escape(text: &str) -> String {
    text.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

fn web_item(title: String, url: String, score: f64) -> Item {
    let mut item = Item::new(title, Icon::Glyph(Glyph::Search, Tone::Plain), "Web", Action::OpenUrl(url));
    item.sub = Some("Opens in your browser".into());
    item.score = score;
    item
}


/// Files mode's row while the system's index is asked.
pub const LOOKING_FOR_FILES: &str = "Looking for files…";

/// Files mode's row when the index found nothing for `query`.
pub fn no_files_row(query: &str) -> Item {
    note_row(format!("No files named “{}”", query.trim()), Glyph::Search, "Files")
}

/// A row that only says something, like a mode's hint before anything is typed.
pub(super) fn note_row(title: impl Into<String>, glyph: Glyph, section: &'static str) -> Item {
    Item::new(title, Icon::Glyph(glyph, Tone::Plain), section, Action::Nothing)
}

/// What `text` means with the palette in `mode`: that mode's rows alone. Files come from the
/// system's index afterwards, as they do for an ordinary search.
pub fn search_in(ctx: &Context, mode: Mode, text: &str) -> Vec<Item> {
    let q = text.trim();
    match mode {
        Mode::Calc => match calc::evaluate(q) {
            Some(answer) => {
                let mut item = Item::new(answer.text.clone(), Icon::Glyph(Glyph::Calc, Tone::Accent), "Calculator", Action::Copy(answer.text));
                item.sub = Some(q.to_string());
                item.second = answer.number.map(|n| (Action::Copy(n), "Copy number"));
                vec![item]
            }
            None if q.is_empty() => vec![note_row("Type a sum, like 12% of 80 or 5 km in mi", Glyph::Calc, "Calculator")],
            None => vec![note_row("Not a sum yet", Glyph::Calc, "Calculator")],
        },
        Mode::Web(i) => {
            let (_, site, address) = WEB[i];
            if q.is_empty() {
                vec![note_row(format!("Type what to search {site} for"), Glyph::Search, "Web")]
            } else {
                vec![web_item(format!("Search {site} for “{q}”"), format!("{address}{}", url_escape(q)), 0.0)]
            }
        }
        Mode::Files if !ctx.search_files => {
            let mut item = Item::new("File search is off", Icon::Glyph(Glyph::Settings, Tone::Plain), "Files", Action::Setting("palette-search-files"));
            item.sub = Some("Turn it on in Kairo's settings".into());
            vec![item]
        }
        Mode::Files if q.chars().count() < super::files::MIN_CHARS => vec![note_row("Type part of a file's name", Glyph::Search, "Files")],
        // Files come from the system's index a moment later, in its place (mod.rs, files_mode_rows).
        Mode::Files => vec![note_row(LOOKING_FOR_FILES, Glyph::Search, "Files")],
        Mode::Windows => {
            let found = window_rows(ctx, q, q.is_empty());
            if found.is_empty() {
                vec![note_row(if ctx.windows.is_empty() { "No open windows" } else { "No window matches" }, Glyph::Search, "Windows")]
            } else {
                found
            }
        }
        Mode::Tasks => {
            let found = task_rows(ctx, q);
            match (found.is_empty(), q.is_empty()) {
                (false, _) => found,
                (true, false) => vec![add_task_item(ctx, q)],
                (true, true) => vec![note_row("No open tasks. Type one to add it", Glyph::Focus, "Tasks")],
            }
        }
        Mode::Clipboard => commands::clipboard_mode_rows(ctx, q),
        Mode::Notes => {
            let found = note_rows(ctx, q);
            match (found.is_empty(), q.is_empty()) {
                (false, true) => std::iter::once(pop_out_notes_item()).chain(found).collect(),
                (false, false) => found,
                (true, false) => vec![new_note_item(q)],
                (true, true) => vec![note_row("No notes yet. Type one to start it", Glyph::Tag, "Notes")],
            }
        }
    }
}

/// What a typed query could mean, best first: the top match, then the rest by section.
pub fn search(ctx: &Context, query: &str) -> Vec<Item> {
    let mut items = Vec::new();

    // Maths leads with its answer; a command or a day keeps its own meaning.
    if focus_command(query).is_none() && !commands::leads(query) && parse_day(query, ctx.today).is_none() {
        if let Some(answer) = calc::answer(query) {
            let mut item = Item::new(answer.text.clone(), Icon::Glyph(Glyph::Calc, Tone::Accent), "Calculator", Action::Copy(answer.text));
            item.sub = Some(query.trim().to_string());
            item.second = answer.number.map(|n| (Action::Copy(n), "Copy number"));
            item.score = 2000.0;
            items.push(item);
        }
    }

    if let Some((minutes, label)) = focus_command(query) {
        let task = label.as_deref().and_then(|l| task_for(&ctx.tasks, l));
        let action = Action::StartFocus(minutes, label.clone(), task.map(|t| t.id.clone()));
        let mut item = Item::new(format!("Start a {minutes}-minute focus session"), Icon::Glyph(Glyph::Focus, Tone::Accent), "Focus", action);
        let list = label.as_ref().and_then(|l| ctx.block_lists.iter().find(|n| n.eq_ignore_ascii_case(l.trim())));
        let says: Vec<String> = [list.map(|l| format!("Blocks your “{l}” list")), task.map(|t| format!("For your task “{}”", t.title))].into_iter().flatten().collect();
        item.sub = Some(match (says.is_empty(), &label) {
            (false, _) => says.join(" · "),
            (true, Some(l)) => format!("Labelled “{l}”"),
            (true, None) => "No label".into(),
        });
        item.score = 1000.0;
        // The card shows what the session will be called: a task's title, or the label.
        item.preview = Some(Preview::Focus(minutes, task.map(|t| t.title.clone()).or(label)));
        items.push(item);
    }
    items.extend(commands::rows(ctx, query));
    items.extend(task_items(ctx, query));
    items.extend(note_items(ctx, query));
    items.extend(scan_item(query));
    items.extend(window_items(ctx, query));
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
    // A running session's own rows (pause, extend, end) are among the commands.
    let idle = ctx.live.session.is_none();
    // Ledger's Plan view: planned in the day, wrapped up from five o'clock.
    let plan_title = if chrono::Timelike::hour(&ctx.live.now) >= 17 { "Wrap up the day" } else { "Plan the day" };
    let actions: [(&str, String, Glyph, Action, bool); 11] = [
        ("focus start session pomodoro 25", "Start a 25-minute focus session".into(), Glyph::Focus, Action::StartFocus(25, None, None), idle),
        ("focus start session 50", "Start a 50-minute focus session".into(), Glyph::Focus, Action::StartFocus(50, None, None), idle),
        ("focus start session 90", "Start a 90-minute focus session".into(), Glyph::Focus, Action::StartFocus(90, None, None), idle),
        ("open chrono screen time tracker show window", "Open Chrono".into(), Glyph::Today, Action::Go("today", None), true),
        ("dashboard home apps kairo", "Dashboard".into(), Glyph::Day, Action::Dashboard(None), true),
        ("ledger tasks to-do todo notes", "Open Ledger".into(), Glyph::Focus, Action::OpenLedger, true),
        ("plan day plan my day wrap up shutdown review tomorrow carry over", plan_title.into(), Glyph::Day, Action::OpenPlan, true),
        ("ring dial apps wheel", "Open the app ring".into(), Glyph::Day, Action::OpenRing, true),
        ("schedule scheduled focus plan routine", "Focus schedule".into(), Glyph::Focus, Action::Go("focus", None), true),
        ("virustotal vt virus malware", "Scan with VirusTotal".into(), Glyph::Search, Action::Scan(None), true),
        ("speed test speedtest internet speed bandwidth wifi", "Test internet speed".into(), Glyph::Chart, Action::SpeedTest, true),
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
        if let Action::StartFocus(minutes, None, None) = item.action {
            item.preview = Some(Preview::Focus(minutes, None));
        }
        items.push(item);
    }

    for mut item in settings::search(query, &ctx.switches) {
        item.score -= 8.0;
        items.push(item);
    }

    items.extend(mode::rows(query));
    // With nothing else to offer, the web is.
    if items.is_empty() && !query.trim().is_empty() {
        let q = query.trim();
        items.push(web_item(format!("Search the web for “{q}”"), format!("{}{}", WEB[0].2, url_escape(q)), 0.0));
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
    if ctx.compact {
        return Suggestions { rows: Vec::new(), tiles: Vec::new(), chips: Vec::new() };
    }
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
    match &ctx.live.session {
        Some(session) => {
            let mut item = Item::new("Focus session in progress", Icon::Glyph(Glyph::Focus, Tone::Accent), "Suggested", Action::Go("focus", None));
            item.sub = Some(session.label.as_ref().map_or("Open the timer".into(), |l| format!("“{l}”")));
            rows.push(item);
        }
        None => {
            let mut item = Item::new("Start a 25-minute focus session", Icon::Glyph(Glyph::Focus, Tone::Accent), "Suggested", Action::StartFocus(25, None, None));
            item.sub = Some("Or type “focus 50 essay” for any length and a label".into());
            rows.push(item);
        }
    }
    rows.extend(commands::suggestion(ctx));
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
pub(super) mod tests {
    use super::*;

    fn app(name: &str, hours: i64) -> AppEntry {
        AppEntry { name: name.into(), path: format!(r"C:\Start\{name}.lnk"), used_ms: hours * HOUR_MS }
    }

    /// A palette opened on a Monday with a few apps, nothing running; the commands' tests use
    /// it too.
    pub(in crate::palette) fn ctx() -> Context {
        Context {
            today: NaiveDate::from_ymd_opt(2026, 10, 26).unwrap(), // a Monday
            apps: vec![app("Cursor", 61), app("Brave", 19), app("Discord", 7), app("CurseForge", 0), app("Visual Studio Code", 0), app("Notepad", 0)],
            live: Live::default(),
            today_apps: vec![("Cursor".into(), 2 * HOUR_MS), ("THE FINALS".into(), HOUR_MS), ("Brave".into(), HOUR_MS / 2)],
            over_limits: vec![("THE FINALS".into(), 132 * 60_000, 120 * 60_000)],
            suggestions: true,
            compact: false,
            search_files: true,
            switches: Vec::new(),
            subjects: vec![subject("THE FINALS", "app:discovery", 30), subject("YouTube", "site:youtube", 12), subject("Cursor", "app:cursor", 61)],
            limits: HashMap::from([("app:discovery".to_string(), 2 * HOUR_MS)]),
            week_goals: Some("7h 53m of 15h 00m productive".into()),
            block_lists: vec!["Social".into()],
            modes: Vec::new(),
            tasks: Vec::new(),
            day_first: true,
            notes: Vec::new(),
            windows: Vec::new(),
            can_close_windows: true,
            next_meeting: Some(("Design review".into(), NaiveDate::from_ymd_opt(2026, 10, 26).unwrap().and_hms_opt(14, 0, 0).unwrap(), 35 * 60_000)),
            programs: Vec::new(),
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
        // The theme's row leads to Kairo's settings, to its own control; the themes themselves
        // come before it (commands.rs).
        let found = search(&c, "theme");
        let theme = found.iter().find(|i| i.section == "Settings").expect("the setting's row");
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
        assert_eq!(focus.action, Action::StartFocus(50, Some("essay".into()), None));
        assert_eq!(focus.sub.as_deref(), Some("Labelled “essay”"));
        let social = &search(&c, "focus 25 social")[0];
        assert_eq!((social.action.clone(), social.sub.as_deref()), (Action::StartFocus(25, Some("social".into()), None), Some("Blocks your “Social” list")));
        let found = search(&c, "foc");
        assert_eq!(found[0].title, "Focus");
        // Starting a session, by the clock or until the next meeting, and the settings for one.
        assert!(found[1..].iter().all(|i| i.section == "Actions" || i.section == "Settings" || i.title == "Focus until Design review"));
        // Nothing of Kairo's matches; only the web is offered.
        assert_eq!(search(&c, "zzzq").iter().map(|i| i.section).collect::<Vec<_>>(), ["Web"]);
    }

    #[test]
    fn suggestions_offer_focus_limits_and_todays_apps() {
        let s = suggestions(&ctx());
        assert_eq!(titles(&s.rows), ["Start a 25-minute focus session", "THE FINALS: 12m over today’s limit"]);
        // THE FINALS isn't in the Start menu, so it can't be a tile.
        assert_eq!(titles(&s.tiles), ["Cursor", "Brave"]);
        assert_eq!(titles(&s.chips), ["Today", "Yesterday", "This week", "Focus", "Chrono settings"]);
        let quiet = suggestions(&Context { suggestions: false, ..ctx() });
        let bare = suggestions(&Context { compact: true, ..ctx() });
        assert!(bare.rows.is_empty() && bare.tiles.is_empty() && bare.chips.is_empty(), "the compact palette opens empty");
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
    fn only_time_charts_wait_for_enter() {
        let c = ctx();
        let answer = search(&c, "finals this week").remove(0);
        assert!(answer.preview.as_ref().is_some_and(Preview::on_enter));
        assert!(!Preview::Day(c.today).on_enter());
        assert!(!Preview::Focus(25, None).on_enter());
        assert_eq!(enter_again(answer.sub.as_deref()), "Enter again to open this week in Chrono");
        assert_eq!(enter_again(None), "Enter again to open it");
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

    #[test]
    fn maths_leads_with_a_row_that_copies_the_answer() {
        let items = search(&ctx(), "5 km in mi");
        let first = &items[0];
        assert_eq!(first.section, "Calculator");
        assert_eq!(first.title, "3.10686 mi");
        assert_eq!(first.sub.as_deref(), Some("5 km in mi"));
        assert_eq!(first.action, Action::Copy("3.10686 mi".into()));
        assert_eq!(first.second, Some((Action::Copy("3.10686".into()), "Copy number")));
        assert!(search(&ctx(), "12*7+3")[0].second.is_none());
    }

    #[test]
    fn focus_commands_and_days_never_become_sums() {
        for q in ["focus 50", "2026-10-30", "14 oct"] {
            assert!(search(&ctx(), q).iter().all(|i| i.section != "Calculator"), "{q}");
        }
    }

    fn task(id: &str, title: &str) -> crate::tasks::Task {
        crate::tasks::Task { id: id.into(), title: title.into(), created_ms: 0, done_ms: None, position: 0, focused_ms: 80 * 60_000, ..Default::default() }
    }

    #[test]
    fn focus_on_a_task_counts_for_it() {
        let mut c = ctx();
        c.tasks = vec![task("t1", "Essay")];
        let first = &search(&c, "focus 50 essay")[0];
        assert_eq!(first.action, Action::StartFocus(50, Some("essay".into()), Some("t1".into())));
        assert_eq!(first.sub.as_deref(), Some("For your task “Essay”"));
        // The card shows what the session will be called: the task's title.
        assert_eq!(first.preview, Some(Preview::Focus(50, Some("Essay".into()))));
    }

    #[test]
    fn a_label_naming_a_list_and_a_task_says_both() {
        let mut c = ctx();
        c.tasks = vec![task("t1", "Social post")];
        let first = &search(&c, "focus 25 social")[0];
        assert_eq!(first.sub.as_deref(), Some("Blocks your “Social” list · For your task “Social post”"));
        assert_eq!(first.action, Action::StartFocus(25, Some("social".into()), Some("t1".into())));
    }

    #[test]
    fn an_unmatched_label_stays_a_label() {
        let first = &search(&ctx(), "focus 50 essay")[0];
        assert_eq!(first.action, Action::StartFocus(50, Some("essay".into()), None));
        assert_eq!(first.sub.as_deref(), Some("Labelled “essay”"));
    }

    #[test]
    fn equal_matches_go_to_the_older_task() {
        let mut c = ctx();
        c.tasks = vec![task("t1", "Report A"), task("t2", "Report B")];
        assert_eq!(search(&c, "focus 25 report")[0].action, Action::StartFocus(25, Some("report".into()), Some("t1".into())));
    }

    #[test]
    fn tasks_lists_open_ones_and_typing_adds_one() {
        let mut c = ctx();
        c.tasks = vec![task("t1", "Essay"), task("t2", "Taxes")];
        let found = search(&c, "tasks");
        let rows: Vec<&Item> = found.iter().filter(|i| i.section == "Tasks").collect();
        assert_eq!(rows.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), ["Essay", "Taxes"]);
        assert_eq!(rows[0].action, Action::StartFocus(25, None, Some("t1".into())));
        assert_eq!(rows[0].second, Some((Action::FinishTask("t1".into()), "Mark done")));
        assert_eq!(rows[0].detail.as_deref(), Some("1h 20m"));
        let filtered: Vec<String> = search(&c, "tasks ess").into_iter().filter(|i| i.section == "Tasks").map(|i| i.title).collect();
        assert_eq!(filtered, ["Essay"]);
        for q in ["task buy milk", "add task buy milk"] {
            let add = search(&c, q).into_iter().find(|i| i.action == Action::AddTask("buy milk".into())).expect(q);
            assert_eq!(add.title, "Add task “buy milk”");
        }
    }

    #[test]
    fn tasks_rows_are_capped() {
        let mut c = ctx();
        c.tasks = (0..20).map(|n| task(&format!("t{n}"), &format!("Task {n}"))).collect();
        assert_eq!(search(&c, "tasks").iter().filter(|i| i.section == "Tasks").count(), 8);
    }

    #[test]
    fn an_app_named_task_still_leads() {
        let mut c = ctx();
        c.apps.push(app("Task Manager", 0));
        for q in ["task manager", "Task Manager"] {
            assert_eq!(search(&c, q)[0].title, "Task Manager", "{q}");
        }
        // With nothing else matching, it still offers to add one; "add task" always does first.
        assert_eq!(search(&c, "task buy milk")[0].action, Action::AddTask("buy milk".into()));
        assert_eq!(search(&c, "add task manager")[0].action, Action::AddTask("manager".into()));
    }

    #[test]
    fn adding_a_task_says_what_its_shortcuts_read_as() {
        let c = ctx();
        // ctx's today is Monday 26 Oct 2026.
        let row = &search(&c, "task essay !h fri")[0];
        assert_eq!(row.title, "Add task “essay”");
        assert_eq!(row.sub.as_deref(), Some("High · Fri 30 Oct · In Ledger"));
        assert_eq!(row.action, Action::AddTask("essay !h fri".into()));
        assert_eq!(search(&c, "task buy milk")[0].sub.as_deref(), Some("In Ledger"));
    }

    #[test]
    fn listed_tasks_show_their_due_day() {
        let mut c = ctx();
        let mut due = task("t1", "Essay");
        due.due_day = Some("2026-10-30".into());
        c.tasks = vec![due, task("t2", "Laundry")];
        let rows = search(&c, "tasks");
        assert_eq!(rows[0].detail.as_deref(), Some("Fri 30 Oct"));
        assert_eq!(rows[1].detail.as_deref(), Some("1h 20m"));
    }

    fn note(id: &str, title: &str, text: &str) -> NoteEntry {
        NoteEntry { id: id.into(), title: title.into(), text: text.to_lowercase() }
    }

    fn with_notes() -> Context {
        let mut c = ctx();
        c.notes = vec![note("n1", "Essay plan", "essay plan\nsources: library"), note("n2", "Shopping", "shopping\nmilk, eggs")];
        c
    }

    #[test]
    fn note_starts_a_new_one() {
        let c = with_notes();
        let row = &search(&c, "note buy list")[0];
        assert_eq!((row.title.as_str(), row.section), ("New note “buy list”", "Notes"));
        assert_eq!(row.action, Action::NewNote("buy list".into()));
        assert_eq!(row.sub.as_deref(), Some("In a small window"));
        assert_eq!(search(&c, "new note x")[0].action, Action::NewNote("x".into()));
        assert_eq!(search(&c, "create note x")[0].action, Action::NewNote("x".into()));
    }

    #[test]
    fn speed_test_opens_its_window() {
        let c = ctx();
        for q in ["speed test", "speedtest", "internet speed"] {
            assert_eq!(search(&c, q)[0].action, Action::SpeedTest, "{q}");
        }
    }

    #[test]
    fn virustotal_opens_the_scan_window() {
        let c = ctx();
        for q in ["virustotal", "vt", "scan"] {
            assert!(search(&c, q).iter().any(|i| i.action == Action::Scan(None)), "{q}");
        }
        // A link or hash after it is looked up as the window opens.
        let row = &search(&c, "scan https://example.com")[0];
        assert_eq!((row.title.as_str(), &row.action), ("Check “https://example.com” on VirusTotal", &Action::Scan(Some("https://example.com".into()))));
        let hash = "275a021bbfb6489e54d471899f7db9d1663fc695ec2fe2a2c4538aabf651fd0f";
        assert_eq!(search(&c, &format!("vt {hash}"))[0].action, Action::Scan(Some(hash.into())));
        // Anything else after "scan" is an ordinary search.
        assert!(!search(&c, "scan document").iter().any(|i| matches!(i.action, Action::Scan(Some(_)))));
    }

    #[test]
    fn new_note_alone_starts_an_empty_one() {
        let c = with_notes();
        for q in ["new note", "create note", "Create Note ", "new note  "] {
            let row = &search(&c, q)[0];
            assert_eq!((row.title.as_str(), &row.action), ("New note", &Action::NewNote(String::new())), "{q}");
        }
    }

    #[test]
    fn notes_can_be_popped_out() {
        let c = with_notes();
        let rows: Vec<_> = search(&c, "notes").into_iter().filter(|i| i.section == "Notes").collect();
        assert_eq!((rows[0].title.as_str(), &rows[0].action), ("All notes in a small window", &Action::PopOutNote(None)));
        // Each note opens in Ledger, or with Ctrl+Enter in the small window.
        assert_eq!(rows[1].second, Some((Action::PopOutNote(Some("n1".into())), "Pop out")));
        // The Notes mode offers the list too, above its notes.
        assert_eq!(search_in(&c, Mode::Notes, "")[0].action, Action::PopOutNote(None));
    }

    #[test]
    fn notes_lists_and_searches_them() {
        let c = with_notes();
        let all: Vec<_> = search(&c, "notes").into_iter().filter(|i| i.section == "Notes").map(|i| i.action).collect();
        assert_eq!(all, [Action::PopOutNote(None), Action::OpenNote("n1".into()), Action::OpenNote("n2".into())]);
        assert_eq!(search(&c, "notes milk")[0].action, Action::OpenNote("n2".into()));
    }

    #[test]
    fn any_query_offers_notes_that_match_below_apps() {
        let c = with_notes();
        let rows = search(&c, "library");
        let first = rows.iter().position(|i| i.section == "Notes").expect("the note whose text has it");
        assert_eq!(rows[first].action, Action::OpenNote("n1".into()));
        // An app called like the query stays above a note.
        let mut c = with_notes();
        c.notes.push(note("n3", "Cursor tips", "cursor tips"));
        let rows = search(&c, "cursor");
        assert_eq!(rows[0].title, "Cursor");
        assert!(rows.iter().any(|i| i.action == Action::OpenNote("n3".into())));
        // At most three.
        let mut c = ctx();
        c.notes = (0..6).map(|i| note(&format!("m{i}"), &format!("Milk {i}"), "milk")).collect();
        assert_eq!(search(&c, "milk").iter().filter(|i| i.section == "Notes").count(), 3);
    }

    #[test]
    fn the_ring_opens_from_the_palette() {
        let c = ctx();
        assert_eq!(search(&c, "ring")[0].action, Action::OpenRing);
    }

    #[test]
    fn ledger_opens_from_the_palette() {
        let c = ctx();
        assert_eq!(search(&c, "ledger")[0].action, Action::OpenLedger);
    }

    #[test]
    fn the_plan_is_offered_as_planning_by_day_and_wrapping_up_from_five() {
        use chrono::Timelike;
        let mut c = ctx();
        c.live.now = c.live.now.with_hour(10).unwrap();
        let morning = search(&c, "plan");
        assert_eq!((morning[0].title.as_str(), morning[0].action.clone()), ("Plan the day", Action::OpenPlan));
        c.live.now = c.live.now.with_hour(18).unwrap();
        let evening = search(&c, "plan");
        assert_eq!((evening[0].title.as_str(), evening[0].action.clone()), ("Wrap up the day", Action::OpenPlan));
        // Either way, either word finds it.
        assert_eq!(search(&c, "wrap up")[0].action, Action::OpenPlan);
        assert_eq!(search(&c, "carry over")[0].action, Action::OpenPlan);
    }

    #[test]
    fn equal_matches_go_to_the_older_task_whatever_the_order() {
        let mut c = ctx();
        let mut newer = task("new", "Report B");
        newer.created_ms = 200;
        let mut older = task("old", "Report A");
        older.created_ms = 100;
        c.tasks = vec![newer, older];
        assert_eq!(search(&c, "focus 25 report")[0].action, Action::StartFocus(25, Some("report".into()), Some("old".into())));
    }

    #[test]
    fn odd_characters_near_task_words_never_panic() {
        let c = ctx();
        for q in ["tas\u{212A} manager", "task\u{212A} x", "add tas\u{212A} y", "\u{212A}asks"] {
            let _ = search(&c, q);
        }
    }

    fn win(id: &str, title: &str, app: &str) -> WindowEntry {
        WindowEntry { id: id.into(), title: title.into(), app: app.into(), icon: None }
    }

    fn with_windows() -> Context {
        let mut c = ctx();
        c.apps.push(app("Excel", 3));
        c.windows = vec![win("1", "budget.xlsx — Excel", "Excel"), win("2", "Inbox — Mail", "Mail")];
        c
    }

    #[test]
    fn a_window_title_match_leads() {
        let first = search(&with_windows(), "budget").into_iter().next().unwrap();
        assert_eq!((first.section, first.title.as_str(), first.sub.as_deref()), ("Windows", "budget.xlsx — Excel", Some("Excel")));
        assert_eq!(first.action, Action::FocusWindow("1".into()));
        assert_eq!(first.second, Some((Action::CloseWindow("1".into()), "Close window")));
    }

    #[test]
    fn an_app_name_puts_the_app_first_then_its_windows() {
        let found = search(&with_windows(), "excel");
        assert_eq!(found[0].title, "Excel");
        assert!(found.iter().any(|i| i.action == Action::FocusWindow("1".into())));
    }

    #[test]
    fn windows_lists_them_all_in_order_capped() {
        let mut c = ctx();
        c.windows = (0..12).map(|n| win(&n.to_string(), &format!("Window {n}"), "App")).collect();
        let rows: Vec<String> = search(&c, "windows").into_iter().filter(|i| i.section == "Windows").map(|i| i.title).collect();
        assert_eq!(rows, (0..8).map(|n| format!("Window {n}")).collect::<Vec<_>>());
    }

    #[test]
    fn no_close_where_closing_isnt_possible() {
        let mut c = with_windows();
        c.can_close_windows = false;
        assert!(search(&c, "budget").iter().filter(|i| i.section == "Windows").all(|i| i.second.is_none()));
    }

    #[test]
    fn windows_never_beat_commands() {
        let mut c = ctx();
        c.windows = vec![win("1", "focus 50", "Notes"), win("2", "12*7", "Notes")];
        assert_eq!(search(&c, "focus 50")[0].section, "Focus");
        assert_eq!(search(&c, "12*7")[0].section, "Calculator");
    }

    #[test]
    fn no_windows_among_suggestions() {
        let s = suggestions(&with_windows());
        assert!(s.rows.iter().all(|i| i.section != "Windows"));
    }

    #[test]
    fn typing_windows_still_opens_an_app_named_windows() {
        let mut c = with_windows();
        c.apps.push(app("Windows Terminal", 0));
        let found = search(&c, "windows");
        assert_eq!(found[0].title, "Windows Terminal");
        assert!(found.iter().any(|i| i.section == "Windows"));
    }

    #[test]
    fn bringing_a_window_forward_happens_before_the_palette_hides() {
        assert!(!Action::FocusWindow("1".into()).opens_window());
    }

    fn web_row(items: &[Item]) -> Option<&Item> {
        items.iter().find(|i| i.section == "Web")
    }

    /// What typing `text` shows once a leading short word has become its bubble.
    fn typed(c: &Context, text: &str) -> Vec<Item> {
        let (m, rest) = mode::shortcut(text).expect("a short word");
        search_in(c, m, rest)
    }

    #[test]
    fn web_modes_search_the_right_site() {
        let first = &typed(&ctx(), "yt lofi beats")[0];
        assert_eq!(first.title, "Search YouTube for “lofi beats”");
        assert_eq!(first.action, Action::OpenUrl("https://www.youtube.com/results?search_query=lofi%20beats".into()));
        for (q, url) in [
            ("g kairo", "https://www.google.com/search?q=kairo"),
            ("G Kairo", "https://www.google.com/search?q=Kairo"),
            ("ddg rust", "https://duckduckgo.com/?q=rust"),
            ("w Kairos", "https://en.wikipedia.org/w/index.php?search=Kairos"),
            ("gh tauri", "https://github.com/search?q=tauri"),
            ("maps changi airport", "https://www.google.com/maps/search/changi%20airport"),
        ] {
            assert_eq!(web_row(&typed(&ctx(), q)).map(|i| &i.action), Some(&Action::OpenUrl(url.into())), "{q}");
        }
        // Before anything is typed after it, the bubble only says what to do.
        let empty = typed(&ctx(), "gh ");
        assert_eq!((empty[0].title.as_str(), &empty[0].action), ("Type what to search GitHub for", &Action::Nothing));
    }

    #[test]
    fn searches_are_encoded_for_an_address() {
        assert_eq!(web_row(&typed(&ctx(), "g a&b ü/#")).map(|i| &i.action), Some(&Action::OpenUrl("https://www.google.com/search?q=a%26b%20%C3%BC%2F%23".into())));
    }

    #[test]
    fn a_site_name_offers_its_search_beside_its_app() {
        let mut c = ctx();
        c.apps.push(app("GitHub Desktop", 0));
        let found = search(&c, "github");
        assert_eq!(found[0].title, "GitHub Desktop");
        assert!(found.iter().any(|i| i.action == Action::EnterMode(Mode::Web(4)) && i.title == "Search GitHub"));
        // The short words are bubbles now: ordinary search doesn't search a site for them.
        assert!(search(&c, "gh tauri").iter().all(|i| !matches!(&i.action, Action::OpenUrl(u) if u.contains("github"))));
        assert!(Action::EnterMode(Mode::Calc).keeps_palette_open());
    }

    #[test]
    fn the_calculator_mode_works_out_anything_it_can() {
        let c = ctx();
        assert_eq!(search_in(&c, Mode::Calc, "2^10")[0].title, "1024");
        assert_eq!(search_in(&c, Mode::Calc, "sqrt 16")[0].title, "4");
        assert!(search_in(&c, Mode::Calc, "pi")[0].title.starts_with("3.14159"));
        let bad = &search_in(&c, Mode::Calc, "2 +")[0];
        assert_eq!((bad.title.as_str(), &bad.action), ("Not a sum yet", &Action::Nothing));
        assert_eq!(search_in(&c, Mode::Calc, "").len(), 1);
        // Copying the answer, or with Ctrl+Enter the number alone.
        let unit = &search_in(&c, Mode::Calc, "5 km to mi")[0];
        assert!(matches!(unit.action, Action::Copy(_)) && unit.second.is_some());
    }

    #[test]
    fn task_and_note_modes_list_find_and_add() {
        let mut c = ctx();
        c.tasks = vec![task("a", "Essay draft"), task("b", "Buy milk")];
        c.notes = vec![NoteEntry { id: "n1".into(), title: "Ring ideas".into(), text: "ring ideas".into() }];
        assert_eq!(titles(&search_in(&c, Mode::Tasks, "")), vec!["Essay draft", "Buy milk"]);
        assert_eq!(titles(&search_in(&c, Mode::Tasks, "milk")), vec!["Buy milk"]);
        assert_eq!(search_in(&c, Mode::Tasks, "call mum")[0].action, Action::AddTask("call mum".into()));
        assert_eq!(search_in(&c, Mode::Notes, "ring")[0].action, Action::OpenNote("n1".into()));
        assert_eq!(search_in(&c, Mode::Notes, "groceries")[0].action, Action::NewNote("groceries".into()));
    }

    #[test]
    fn window_and_file_modes_keep_to_their_own() {
        let mut c = ctx();
        c.windows = vec![
            WindowEntry { id: "1".into(), title: "Kairo – README.md".into(), app: "Visual Studio Code".into(), icon: None },
            WindowEntry { id: "2".into(), title: "Inbox".into(), app: "Mail".into(), icon: None },
        ];
        assert_eq!(search_in(&c, Mode::Windows, "").len(), 2);
        assert_eq!(titles(&search_in(&c, Mode::Windows, "inbox")), vec!["Inbox"]);
        assert_eq!(search_in(&c, Mode::Windows, "zzqx")[0].action, Action::Nothing);
        // Files come from the system's index; before enough is typed, or with file search off,
        // the mode says so.
        assert_eq!(titles(&search_in(&c, Mode::Files, "budget")), vec![LOOKING_FOR_FILES]);
        assert_eq!(search_in(&c, Mode::Files, "b")[0].action, Action::Nothing);
        let off = Context { search_files: false, ..ctx() };
        assert_eq!(search_in(&off, Mode::Files, "budget")[0].action, Action::Setting("palette-search-files"));
    }

    #[test]
    fn a_prefix_alone_never_searches() {
        for q in ["gh", "yt", "g ", "maps"] {
            assert!(search(&ctx(), q).iter().all(|i| !matches!(i.action, Action::OpenUrl(_)) || i.title.starts_with("Search the web")), "{q}");
        }
    }

    #[test]
    fn the_web_is_offered_only_when_nothing_else_matches() {
        let none = search(&ctx(), "zzqx nothing here");
        assert_eq!(none.len(), 1);
        assert_eq!(none[0].title, "Search the web for “zzqx nothing here”");
        assert_eq!(none[0].action, Action::OpenUrl("https://www.google.com/search?q=zzqx%20nothing%20here".into()));
        assert!(web_row(&search(&ctx(), "cursor")).is_none());
    }

    #[test]
    fn commands_still_lead_over_web_shortcuts() {
        let mut c = ctx();
        c.apps.push(app("GitHub Desktop", 0));
        assert_eq!(search(&c, "gh")[0].title, "GitHub Desktop");
        assert_eq!(search(&c, "focus 50")[0].section, "Focus");
    }
}
