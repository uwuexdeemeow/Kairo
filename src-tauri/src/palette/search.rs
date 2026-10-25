//! What the palette shows for what's typed: matching, the days people name, actions, and the
//! suggestions before anything is typed. No drawing here, so all of it can be tested.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

use super::AppEntry;

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
    Update,
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
    Glyph(Glyph, Tone),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Launch(String),
    /// A Kairo view, and for Today or Week a date.
    Go(&'static str, Option<NaiveDate>),
    StartFocus(u32, Option<String>),
    CheckUpdates,
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
    /// A day to preview above the results while this item is selected.
    pub preview: Option<NaiveDate>,
}

impl Item {
    fn new(title: impl Into<String>, icon: Icon, section: &'static str, action: Action) -> Self {
        Item { title: title.into(), at: Vec::new(), sub: None, detail: None, icon, score: 0.0, section, action, preview: None }
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
    ("settings preferences options", "Settings", "settings", Glyph::Settings, -1),
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
    item.preview = Some(d);
    item
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
        item.sub = Some(label.map_or("No label".into(), |l| format!("Labelled “{l}”")));
        item.score = 1000.0;
        items.push(item);
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
        item.sub = Some("Kairo".into());
        item.score = m.score - 5.0;
        items.push(item);
    }
    let idle = ctx.session.is_none();
    let actions: [(&str, String, Glyph, Action, bool); 6] = [
        ("focus start session pomodoro 25", "Start a 25-minute focus session".into(), Glyph::Focus, Action::StartFocus(25, None), idle),
        ("focus start session 50", "Start a 50-minute focus session".into(), Glyph::Focus, Action::StartFocus(50, None), idle),
        ("focus start session 90", "Start a 90-minute focus session".into(), Glyph::Focus, Action::StartFocus(90, None), idle),
        ("focus end stop session", "End the focus session".into(), Glyph::Focus, Action::Go("focus", None), !idle),
        ("check for updates update version", "Check for updates".into(), Glyph::Update, Action::CheckUpdates, true),
        ("open kairo show window", "Open Kairo".into(), Glyph::Today, Action::Go("today", None), true),
    ];
    for (words, title, glyph, action, offered) in actions {
        if !offered {
            continue;
        }
        let Some(m) = fuzzy(query, &title).or_else(|| fuzzy(query, words)) else { continue };
        let mut item = Item::new(title.clone(), Icon::Glyph(glyph, Tone::Accent), "Actions", action);
        item.at = fuzzy(query, &title).map(|m| m.at).unwrap_or_default();
        item.score = m.score - 8.0;
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
        .filter(|p| ["Today", "Yesterday", "This week", "Focus", "Settings"].contains(&p.1))
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
        }
    }

    fn titles(items: &[Item]) -> Vec<&str> {
        items.iter().map(|i| i.title.as_str()).collect()
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
        assert_eq!(titles(&search(&c, "cur")), ["Cursor", "CurseForge"]);
        assert_eq!(titles(&search(&c, "vsc")), ["Visual Studio Code"]);
        assert_eq!(search(&c, "last tue")[0].preview, NaiveDate::from_ymd_opt(2026, 10, 20));
        let focus = &search(&c, "focus 50 essay")[0];
        assert_eq!(focus.action, Action::StartFocus(50, Some("essay".into())));
        let found = search(&c, "foc");
        assert_eq!(found[0].title, "Focus");
        assert!(found[1..].iter().all(|i| i.section == "Actions"));
        assert!(search(&c, "zzzq").is_empty());
    }

    #[test]
    fn suggestions_offer_focus_limits_and_todays_apps() {
        let s = suggestions(&ctx());
        assert_eq!(titles(&s.rows), ["Start a 25-minute focus session", "THE FINALS: 12m over today’s limit"]);
        // THE FINALS isn't in the Start menu, so it can't be a tile.
        assert_eq!(titles(&s.tiles), ["Cursor", "Brave"]);
        assert_eq!(titles(&s.chips), ["Today", "Yesterday", "This week", "Focus", "Settings"]);
        let quiet = suggestions(&Context { suggestions: false, ..ctx() });
        assert!(quiet.rows.is_empty() && quiet.tiles.is_empty() && quiet.chips.len() == 5);
    }
}
