//! The palette's own commands, typed plainly as `focus 50 essay` is: a theme, a running focus
//! session (pause, resume, extend, end), allowing what's blocked, quick timers, pausing tracking,
//! and a session that runs until the next meeting. A command's word alone lists its choices or
//! says what to type. No drawing here, so all of it can be tested.

use chrono::Duration;

use super::mode::Mode;
use super::search::{after, fmt_dur, fuzzy, minutes_and_label, note_row, task_for, Action, Context, Glyph, Icon, Item, Preview, SessionDo, TimerNow, Tone};
use crate::blocking::ALLOW_MINUTES;
use crate::system::Power;
use crate::timers::MAX_RUNNING;

/// Where a command typed outright sits: at the top, with the focus command.
const LEAD: f64 = 1000.0;
/// Found by other words, a command ranks with the actions, this far below an equal match.
const LOOSE_PENALTY: f64 = 8.0;
/// A command is found by other words once this many characters are typed.
const LOOSE_MIN_CHARS: usize = 2;
/// A theme is found by its name alone only when the name starts with what's typed, at least
/// this much of it, so a letter or two doesn't offer nine themes.
const THEME_MIN_CHARS: usize = 3;
/// A match with what's typed at the start of a word scores at least this (`search::fuzzy`).
const WORD_START: f64 = 100.0;
/// A session until the next meeting needs at least this long to be worth starting.
const MEETING_MIN_MINUTES: i64 = 5;
/// What `extend` adds with no number.
const EXTEND_MINUTES: u32 = 5;

/// Every theme: its id as saved, its name as the settings page has it, and another word for it.
const THEMES: [(&str, &str, &str); 9] = [
    ("system", "System", ""),
    ("dark", "Ink", "dark"),
    ("graphite", "Graphite", ""),
    ("walnut", "Walnut", ""),
    ("baize", "Baize", ""),
    ("midnight", "Midnight", "black"),
    ("light", "Paper", "light"),
    ("linen", "Linen", ""),
    ("sage", "Sage", ""),
];

fn minutes_said(n: i64) -> String {
    if n == 1 {
        "1 minute".into()
    } else {
        format!("{n} minutes")
    }
}

/// The better of a title and other words for it, as (score, the title's characters to underline).
fn found(query: &str, title: &str, words: &str) -> Option<(f64, Vec<usize>)> {
    let by_title = fuzzy(query, title);
    let at = by_title.as_ref().map(|m| m.at.clone()).unwrap_or_default();
    let best = [by_title, fuzzy(query, words)].into_iter().flatten().map(|m| m.score).max_by(f64::total_cmp)?;
    Some((best, at))
}

/// Where a command's row sits for `query`: at the top when the query is one of `exact`, with the
/// actions when its title or `words` are found, nowhere otherwise.
fn placed(query: &str, exact: &[&str], title: &str, words: &str) -> Option<(f64, Vec<usize>)> {
    let q = query.trim();
    if exact.iter().any(|e| q.eq_ignore_ascii_case(e)) {
        return Some((LEAD, fuzzy(q, title).map(|m| m.at).unwrap_or_default()));
    }
    if q.chars().count() < LOOSE_MIN_CHARS {
        return None;
    }
    found(q, title, words).map(|(score, at)| (score - LOOSE_PENALTY, at))
}

fn row(title: impl Into<String>, glyph: Glyph, section: &'static str, action: Action, sub: impl Into<String>, (score, at): (f64, Vec<usize>)) -> Item {
    let mut item = Item::new(title, Icon::Glyph(glyph, Tone::Accent), section, action);
    item.sub = Some(sub.into());
    item.score = score;
    item.at = at;
    item
}

/// Whether `query` is one of these commands with something after its word, so that the
/// calculator leaves it alone as it does `focus 50`.
pub fn leads(query: &str) -> bool {
    let q = query.trim();
    ["extend ", "timer ", "allow ", "pause tracking ", "pomodoro "].iter().any(|word| after(q, word).is_some())
}

/// The rows these commands offer for `query`.
pub fn rows(ctx: &Context, query: &str) -> Vec<Item> {
    let mut items = theme_rows(ctx, query);
    items.extend(session_rows(ctx, query));
    items.extend(pomodoro_rows(ctx, query));
    items.extend(clip_rows(ctx, query));
    items.extend(system_rows(ctx, query));
    items.extend(allow_rows(ctx, query));
    items.extend(timer_rows(ctx, query));
    items.extend(tracking_rows(ctx, query));
    items.extend(meeting_rows(ctx, query));
    items.extend(mode_rows(ctx, query));
    items
}

// ---------- focus modes ----------

/// Each saved focus mode by its name, or all of them for `mode` or `modes`: Enter starts its
/// session, applies its block list and opens its apps.
fn mode_rows(ctx: &Context, query: &str) -> Vec<Item> {
    ctx.modes
        .iter()
        .enumerate()
        .filter_map(|(i, mode)| {
            let lead = [mode.name.as_str(), "mode", "modes"];
            let (score, at) = placed(query, &lead, &mode.name, "mode focus session")?;
            // Listed all together, in the order they were made.
            Some(row(mode.name.clone(), Glyph::Focus, "Modes", Action::RunMode(mode.name.clone()), mode.says(), (score - i as f64 / 100.0, at)))
        })
        .collect()
}

/// With nothing typed: the timer that rings soonest, which Ctrl+Enter cancels.
pub fn suggestion(ctx: &Context) -> Option<Item> {
    let timer = ctx.live.timers.first()?;
    let mut item = timer_row(timer, "Suggested");
    item.sub = Some(format!("{} left", fmt_dur(timer.left_ms)));
    Some(item)
}

// ---------- theme ----------

/// `theme` lists every theme, the one in use marked; `theme walnut`, `walnut theme` or the name
/// alone finds one.
fn theme_rows(ctx: &Context, query: &str) -> Vec<Item> {
    let q = query.trim();
    let lower = q.to_lowercase();
    // What's typed with the word "theme": the name, or nothing for all of them.
    let named: Option<String> = if lower == "theme" || lower == "themes" {
        Some(String::new())
    } else {
        after(q, "theme ").map(|rest| rest.trim().to_lowercase()).or_else(|| lower.strip_suffix(" theme").map(|rest| rest.trim().to_string()))
    };
    let mut items = Vec::new();
    for (i, &(id, name, word)) in THEMES.iter().enumerate() {
        let title = format!("Theme: {name}");
        let order = i as f64 / 100.0;
        let placed = match &named {
            Some(n) if n.is_empty() => Some((LEAD - order, Vec::new())),
            Some(n) => found(n, name, word).map(|_| (LEAD - order, fuzzy(n, &title).map(|m| m.at).unwrap_or_default())),
            None if q.chars().count() >= THEME_MIN_CHARS => found(q, name, word).filter(|(score, _)| *score >= WORD_START).map(|(score, _)| (score - LOOSE_PENALTY, fuzzy(q, &title).map(|m| m.at).unwrap_or_default())),
            None => None,
        };
        let Some(placed) = placed else { continue };
        let sub = match crate::appearance::ground_of(id) {
            None => "Light or dark, as your system is",
            Some(g) if g.light => "Light",
            Some(_) => "Dark",
        };
        let mut item = row(title, Glyph::Settings, "Theme", Action::Theme(id), sub, placed);
        item.detail = (ctx.live.theme == id).then(|| "In use".to_string());
        items.push(item);
    }
    items
}

// ---------- a running focus session ----------

/// Pause or resume, extend and end, while a session runs. Each says how the session stands.
fn session_rows(ctx: &Context, query: &str) -> Vec<Item> {
    let Some(session) = &ctx.live.session else { return Vec::new() };
    let q = query.trim();
    let left = format!("{} left", fmt_dur(session.left_ms));
    let mut status = if session.paused { format!("Paused · {left}") } else { left };
    if let Some(cycle) = ctx.live.cycle.as_ref().filter(|c| !c.on_break) {
        status.push_str(&format!(" · round {} of {}", cycle.round, cycle.rounds));
    }
    if let Some(label) = &session.label {
        status.push_str(&format!(" · “{label}”"));
    }
    let mut items = Vec::new();
    let glyph = Glyph::Focus;

    if session.paused {
        let title = "Resume the focus session";
        if let Some(p) = placed(q, &["resume", "resume focus", "resume session"], title, "resume focus session continue carry on unpause") {
            items.push(row(title, glyph, "Focus", Action::Session(SessionDo::Resume), status.clone(), p));
        }
    } else {
        let title = "Pause the focus session";
        if let Some(p) = placed(q, &["pause", "pause focus", "pause session"], title, "pause focus session break hold") {
            items.push(row(title, glyph, "Focus", Action::Session(SessionDo::Pause), status.clone(), p));
        }
    }

    // `extend 10`, or `extend` alone for the usual few minutes.
    let typed = after(q, "extend").filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace));
    let asked = typed.and_then(|rest| {
        let words: Vec<&str> = rest.split_whitespace().collect();
        if words.is_empty() {
            Some(EXTEND_MINUTES)
        } else {
            minutes_and_label(&words).map(|(minutes, _)| minutes)
        }
    });
    let minutes = asked.unwrap_or(EXTEND_MINUTES);
    let title = format!("Add {} to the focus session", minutes_said(minutes as i64));
    let extend = match asked {
        Some(_) => Some((LEAD, Vec::new())),
        None => placed(q, &[], &title, "extend focus session longer more time add minutes"),
    };
    if let Some(p) = extend {
        items.push(row(title, glyph, "Focus", Action::Session(SessionDo::Extend(minutes)), status.clone(), p));
    }

    let title = if ctx.live.cycle.is_some() { "End the pomodoro" } else { "End the focus session" };
    if let Some(p) = placed(q, &["end", "stop", "end focus", "stop focus", "end session", "stop session"], title, "focus end stop session finish quit") {
        // Strict mode makes ending wait, and the Focus page is where that counts down.
        let (action, sub) = if ctx.live.strict {
            (Action::Go("focus", None), "Strict mode: the Focus page counts down first".to_string())
        } else {
            (Action::Session(SessionDo::End), status)
        };
        items.push(row(title, glyph, "Focus", action, sub, p));
    }
    items
}

// ---------- a pomodoro cycle ----------

/// `pomodoro`, `pomodoro 50 10 essay`: rounds of 50 minutes, breaks of 10, a label. Without
/// numbers the lengths saved in Settings apply. Returns (round, break, label).
fn pomodoro_typed(q: &str) -> Option<(Option<u32>, Option<u32>, Option<String>)> {
    let rest = ["pomodoro", "pomo"].iter().find_map(|word| after(q, word).filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace)))?;
    let words: Vec<&str> = rest.split_whitespace().collect();
    if words.is_empty() {
        return Some((None, None, None));
    }
    match minutes_and_label(&words) {
        Some((work, label)) => {
            // A second number is the break, the rest the label.
            let (rest_min, label) = match label.as_deref().and_then(|l| l.split_whitespace().next().map(|w| (w, l))) {
                Some((first, whole)) if first.chars().all(|c| c.is_ascii_digit()) => {
                    let after_it = whole[first.len()..].trim();
                    (first.parse().ok().map(|m: u32| m.clamp(1, 60)), (!after_it.is_empty()).then(|| after_it.to_string()))
                }
                _ => (None, label),
            };
            Some((Some(work), rest_min, label))
        }
        None => Some((None, None, Some(words.join(" ")))),
    }
}

/// Starting a cycle, and on a break skipping it or ending the cycle.
fn pomodoro_rows(ctx: &Context, query: &str) -> Vec<Item> {
    let q = query.trim();
    let mut items = Vec::new();
    let glyph = Glyph::Focus;

    let typed = pomodoro_typed(q);
    let title = "Start a pomodoro";
    let loose = if typed.is_none() { placed(q, &[], title, "pomodoro cycle rounds tomato") } else { None };
    if typed.is_some() || loose.is_some() {
        let (work, rest, label) = typed.unwrap_or((None, None, None));
        let task = label.as_deref().and_then(|l| task_for(&ctx.tasks, l));
        let mut sub = match (work, rest) {
            (Some(w), Some(r)) => format!("{w}m rounds, {r}m breaks"),
            (Some(w), None) => format!("{w}m rounds, breaks as set in Settings"),
            _ => "Rounds and breaks as set in Settings".to_string(),
        };
        match (task, &label) {
            (Some(task), _) => sub.push_str(&format!(" · For your task “{}”", task.title)),
            (None, Some(label)) => sub.push_str(&format!(" · Labelled “{label}”")),
            (None, None) => {}
        }
        let action = Action::Pomodoro(work, rest, label.clone(), task.map(|t| t.id.clone()));
        items.push(row(title, glyph, "Focus", action, sub, loose.unwrap_or((LEAD, Vec::new()))));
    }

    let Some(cycle) = ctx.live.cycle.as_ref().filter(|c| c.on_break) else { return items };
    let status = if cycle.waiting {
        format!("Break over · round {} of {} waits for you", cycle.round, cycle.rounds)
    } else {
        format!("Break · {} left · round {} of {} next", fmt_dur(cycle.left_ms), cycle.round, cycle.rounds)
    };
    let title = "Skip the break";
    if let Some(p) = placed(q, &["skip", "skip break", "next round"], title, "skip break next round start now") {
        items.push(row(title, glyph, "Focus", Action::Session(SessionDo::SkipBreak), status.clone(), p));
    }
    let title = "End the pomodoro";
    if let Some(p) = placed(q, &["end", "stop", "end focus", "stop focus", "end pomodoro", "stop pomodoro"], title, "pomodoro end stop cycle finish quit") {
        items.push(row(title, glyph, "Focus", Action::Session(SessionDo::End), status, p));
    }
    items
}

// ---------- the computer itself ----------

/// `kill` lists at most this many programs.
const MAX_KILL_ROWS: usize = 8;

/// Where a system command's row sits: as `placed`, but found by other words only once three
/// characters are typed, so two letters never offer to restart the computer.
fn system_placed(q: &str, exact: &[&str], title: &str, words: &str) -> Option<(f64, Vec<usize>)> {
    let place = placed(q, exact, title, words)?;
    (place.0 >= LEAD || q.chars().count() >= 3).then_some(place)
}

/// What was typed, as a confirmation waiting for its second Enter remembers it.
fn typed_form(q: &str) -> String {
    q.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// What follows `kill`, `end task` or `force quit` when the query starts with one as a word.
fn kill_typed(q: &str) -> Option<&str> {
    ["end task", "force quit", "kill"].iter().find_map(|word| after(q, word).filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace)).map(str::trim))
}

/// Lock and sleep act at once; logging out, restarting and shutting down ask twice: the first
/// Enter turns the row into Confirm, with Cancel under it, and the second does it. `kill` lists
/// the running programs it names.
fn system_rows(ctx: &Context, query: &str) -> Vec<Item> {
    let q = query.trim();
    let typed = typed_form(q);
    let mut items = Vec::new();
    for power in Power::ALL {
        let (exact, words) = power.words();
        let Some(place) = system_placed(q, exact, power.title(), words) else { continue };
        let waiting = ctx.live.pending_power.as_ref().is_some_and(|(kind, text)| *kind == power && *text == typed);
        if waiting {
            let mut confirm = Item::new(format!("Confirm: {}", power.verb()), Icon::Glyph(Glyph::Alert, Tone::Warm), "System", Action::PowerNow(power));
            confirm.sub = Some("Press Enter to do it now. Anything else cancels".into());
            confirm.score = place.0 + 0.5;
            let mut cancel = Item::new("Cancel", Icon::Glyph(Glyph::Alert, Tone::Plain), "System", Action::CancelPower);
            cancel.sub = Some("Keep working".into());
            cancel.score = place.0 + 0.4;
            items.extend([confirm, cancel]);
        } else if power.confirms() {
            items.push(row(power.title(), Glyph::Alert, "System", Action::ConfirmPower(power, typed.clone()), power.about(), place));
        } else {
            items.push(row(power.title(), Glyph::Settings, "System", Action::Power(power), power.about(), place));
        }
    }
    items.extend(kill_rows(ctx, q));
    items
}

fn kill_rows(ctx: &Context, q: &str) -> Vec<Item> {
    let Some(rest) = kill_typed(q) else { return Vec::new() };
    let note = |text: String| {
        let mut item = note_row(text, Glyph::Alert, "System");
        item.score = LEAD;
        vec![item]
    };
    if rest.is_empty() {
        return note("Type a program's name, like kill chrome".into());
    }
    let mut found: Vec<_> = ctx.programs.iter().filter_map(|p| fuzzy(rest, &p.name).map(|m| (m.score, m.at, p))).collect();
    found.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.2.name.cmp(&b.2.name)));
    found.truncate(MAX_KILL_ROWS);
    if found.is_empty() {
        return note(format!("No running program matches “{rest}”"));
    }
    found
        .into_iter()
        .enumerate()
        .map(|(i, (_, at, program))| {
            let title = format!("Kill {}", program.name);
            let count = if program.count == 1 { "1 process, lost with its unsaved work".to_string() } else { format!("{} processes, lost with their unsaved work", program.count) };
            // The highlight is on the name, which comes after "Kill ".
            let at = at.into_iter().map(|c| c + "Kill ".chars().count()).collect();
            row(title, Glyph::Alert, "System", Action::Kill(program.name.clone()), format!("Ends it at once · {count}"), (LEAD - i as f64 / 100.0, at))
        })
        .collect()
}

// ---------- clipboard history ----------

/// `clip` shows this many of the latest copies, with a row for the rest.
const RECENT_CLIPS: usize = 5;

/// A row for a copy: its first line, how long ago and how big. Enter puts it on the clipboard.
fn clip_row(clip: &super::search::ClipNow, place: f64) -> Item {
    let age = if clip.age_ms < 60_000 { "just now".to_string() } else { format!("{} ago", fmt_dur(clip.age_ms)) };
    let sub = format!("{age} · {}", crate::clipboard::size_of(&clip.text));
    row(crate::clipboard::title_of(&clip.text), Glyph::Tag, "Clipboard", Action::Copy(clip.text.clone()), sub, (place, Vec::new()))
}

fn clear_clips_row(count: usize, place: f64) -> Item {
    row("Clear clipboard history", Glyph::Tag, "Clipboard", Action::ClearClips, format!("Forget {count} copied"), (place, Vec::new()))
}

fn clip_note(text: &str) -> Vec<Item> {
    let mut item = note_row(text, Glyph::Tag, "Clipboard");
    item.score = LEAD;
    vec![item]
}

/// `clip` lists the latest few copies and a row to see them all (the Clipboard mode); `clip
/// invoice` those containing "invoice"; Enter puts one back on the clipboard. Only a query led by
/// the word asks for them: a password copied a minute ago must not surface while typing
/// something else.
fn clip_rows(ctx: &Context, query: &str) -> Vec<Item> {
    let q = query.trim();
    if q.to_lowercase().starts_with("clear clip") {
        return match &ctx.live.clips {
            Some(clips) if !clips.is_empty() => vec![clear_clips_row(clips.len(), LEAD)],
            _ => Vec::new(),
        };
    }
    let Some(filter) = crate::clipboard::led_by(q) else { return Vec::new() };
    let Some(clips) = &ctx.live.clips else { return clip_note("Clipboard history is off. Turn it on in Kairo's settings") };
    if clips.is_empty() {
        return clip_note("Nothing copied since Kairo started");
    }
    if !filter.is_empty() {
        return matching_clips(clips, filter);
    }
    let mut items: Vec<Item> = clips.iter().take(RECENT_CLIPS).enumerate().map(|(i, c)| clip_row(c, LEAD - i as f64 / 100.0)).collect();
    if clips.len() > RECENT_CLIPS {
        let mut more = row(format!("Show all {} copies", clips.len()), Glyph::Tag, "Clipboard", Action::EnterMode(Mode::Clipboard), "Everything copied, to choose from", (LEAD - 0.4, Vec::new()));
        more.second = None;
        items.push(more);
    }
    items.push(clear_clips_row(clips.len(), LEAD - 0.5));
    items
}

/// The copies containing `filter`, any case, newest first; a note when none do.
fn matching_clips(clips: &[super::search::ClipNow], filter: &str) -> Vec<Item> {
    let wanted = filter.to_lowercase();
    let items: Vec<Item> = clips.iter().filter(|c| c.text.to_lowercase().contains(&wanted)).enumerate().map(|(i, c)| clip_row(c, LEAD - i as f64 / 100.0)).collect();
    if items.is_empty() {
        clip_note(&format!("No copies contain “{filter}”"))
    } else {
        items
    }
}

/// The Clipboard mode: every copy, or those containing what's typed.
pub(super) fn clipboard_mode_rows(ctx: &Context, text: &str) -> Vec<Item> {
    let Some(clips) = &ctx.live.clips else {
        let mut item = Item::new("Clipboard history is off", Icon::Glyph(Glyph::Settings, Tone::Plain), "Clipboard", Action::Setting("clipboard-history"));
        item.sub = Some("Turn it on in Kairo's settings".into());
        return vec![item];
    };
    if clips.is_empty() {
        return clip_note("Nothing copied since Kairo started");
    }
    let q = text.trim();
    let mut items = matching_clips(clips, q);
    if q.is_empty() {
        items.push(clear_clips_row(clips.len(), LEAD - 0.5));
    }
    items
}

// ---------- allowing what's blocked ----------

/// `allow`, while the blocked window is up: lets what it blocks through for a few minutes.
fn allow_rows(ctx: &Context, query: &str) -> Vec<Item> {
    let Some((label, key)) = &ctx.live.blocked else { return Vec::new() };
    let q = query.trim();
    let title = format!("Allow {label} for {}", minutes_said(ALLOW_MINUTES));
    let typed = q.eq_ignore_ascii_case("allow") || after(q, "allow ").is_some();
    let placed = if typed { Some((LEAD, fuzzy("allow", &title).map(|m| m.at).unwrap_or_default())) } else { placed(q, &[], &title, "allow unblock let through") };
    let Some(placed) = placed else { return Vec::new() };
    // Strict mode makes allowing wait, and the blocked window is where that counts down.
    let (action, sub) = if ctx.live.strict {
        (Action::ShowBlocked, "Strict mode: the blocked window counts down first")
    } else {
        (Action::Allow(key.clone()), "Then it's blocked again")
    };
    vec![row(title, Glyph::Focus, "Focus", action, sub, placed)]
}

// ---------- quick timers ----------

/// A running timer's row: nothing for Enter to do, and Ctrl+Enter cancels it.
fn timer_row(timer: &TimerNow, section: &'static str) -> Item {
    let mut item = Item::new(format!("Timer: {}", timer.name), Icon::Glyph(Glyph::Day, Tone::Accent), section, Action::Nothing);
    item.sub = Some(format!("{} left · rings at {}", fmt_dur(timer.left_ms), timer.ends.format("%H:%M")));
    item.second = Some((Action::CancelTimer(timer.id), "Cancel timer"));
    item
}

/// `timer 10 tea` starts one; `timer` lists those running and says what to type.
fn timer_rows(ctx: &Context, query: &str) -> Vec<Item> {
    let q = query.trim();
    let words: Vec<&str> = q.split_whitespace().collect();
    let first = words.first().map(|w| w.to_lowercase()).unwrap_or_default();
    let typed = first == "timer" || first == "timers";
    if typed {
        if let Some((minutes, label)) = minutes_and_label(&words[1..]) {
            let title = format!("Start a {minutes}-minute timer");
            if ctx.live.timers.len() >= MAX_RUNNING {
                let mut full = note_row(title, Glyph::Day, "Timers");
                full.sub = Some(format!("{MAX_RUNNING} timers are running already. Cancel one first."));
                full.score = LEAD;
                return vec![full];
            }
            let sub = label.as_ref().map_or("No name".to_string(), |l| format!("Called “{l}”"));
            return vec![row(title, Glyph::Day, "Timers", Action::StartTimer(minutes, label), sub, (LEAD, Vec::new()))];
        }
    }
    // The list: outright for `timer`, or found by the word half typed or by another word for it.
    let score = if typed {
        LEAD
    } else if q.chars().count() >= LOOSE_MIN_CHARS {
        match found(q, "Timers", "timer countdown alarm") {
            Some((score, _)) => score - LOOSE_PENALTY,
            None => return Vec::new(),
        }
    } else {
        return Vec::new();
    };
    let mut items: Vec<Item> = ctx
        .live
        .timers
        .iter()
        .enumerate()
        .map(|(i, timer)| {
            let mut item = timer_row(timer, "Timers");
            item.score = score - i as f64 / 100.0;
            item
        })
        .collect();
    let mut hint = note_row("Type minutes and a name: timer 10 tea", Glyph::Day, "Timers");
    hint.score = score - 1.0;
    items.push(hint);
    items
}

// ---------- pausing tracking ----------

/// `pause tracking`, `pause tracking 30` and `resume tracking`. `pause` and `resume` alone mean
/// the focus session first when one runs, and tracking under it.
fn tracking_rows(ctx: &Context, query: &str) -> Vec<Item> {
    let q = query.trim();
    let paused = &ctx.live.tracking_paused;
    let glyph = Glyph::Today;
    let below_session = (LEAD - 1.0, Vec::new());

    // With minutes, a pause can be asked for again while paused: it moves the time to resume.
    let rest = after(q, "pause tracking").filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace));
    let words: Vec<&str> = rest.unwrap_or_default().split_whitespace().collect();
    if let Some((minutes, _)) = rest.and_then(|_| minutes_and_label(&words)) {
        let until = ctx.live.now + Duration::minutes(minutes as i64);
        let title = format!("Pause tracking for {}", minutes_said(minutes as i64));
        return vec![row(title, glyph, "Tracking", Action::PauseTracking(Some(minutes)), format!("Resumes at {}", until.format("%H:%M")), (LEAD, Vec::new()))];
    }

    match paused {
        None => {
            let title = "Pause tracking";
            let placed = if q.eq_ignore_ascii_case("pause") { Some(below_session) } else { placed(q, &["pause tracking"], title, "pause tracking stop recording private break") };
            placed.map(|p| row(title, glyph, "Tracking", Action::PauseTracking(None), "Until you resume it", p)).into_iter().collect()
        }
        Some(until) => {
            let title = "Resume tracking";
            let placed = if q.eq_ignore_ascii_case("resume") { Some(below_session) } else { placed(q, &["resume tracking"], title, "resume tracking start recording unpause") };
            let sub = until.map_or("Paused".to_string(), |t| format!("Paused until {}", t.format("%H:%M")));
            placed.map(|p| row(title, glyph, "Tracking", Action::ResumeTracking, sub, p)).into_iter().collect()
        }
    }
}

// ---------- focus until the next meeting ----------

/// What's typed after `focus until meeting`, as a label; None when the query isn't that command.
/// The command may still be on its way: `focus un`, `focus until`, `focus until mee`.
fn until_meeting(query: &str) -> Option<Option<String>> {
    let words: Vec<&str> = query.split_whitespace().collect();
    let lower: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
    if lower.len() < 2 || lower[0] != "focus" {
        return None;
    }
    let until = lower[1].as_str();
    if until != "until" && until != "till" {
        // Still typing the second word.
        let on_its_way = lower.len() == 2 && until.len() >= 2 && ("until".starts_with(until) || "till".starts_with(until));
        return on_its_way.then_some(None);
    }
    let mut i = 2;
    for filler in [["my", "the"].as_slice(), ["next"].as_slice()] {
        if lower.get(i).is_some_and(|w| filler.contains(&w.as_str())) {
            i += 1;
        }
    }
    match lower.get(i).map(String::as_str) {
        None => Some(None),
        Some("meeting") => Some((words.len() > i + 1).then(|| words[i + 1..].join(" "))),
        // Still typing "meeting".
        Some(part) if i + 1 == lower.len() && "meeting".starts_with(part) => Some(None),
        Some(_) => None,
    }
}

/// `focus until meeting`: a session that ends as the next meeting on the calendar starts. With
/// none it can run until, a row says why.
fn meeting_rows(ctx: &Context, query: &str) -> Vec<Item> {
    let q = query.trim();
    let typed = until_meeting(q);
    let loose = if typed.is_none() && q.chars().count() >= LOOSE_MIN_CHARS { found(q, "Focus until your next meeting", "meeting focus until next calendar") } else { None };
    let label = match (&typed, &loose) {
        (Some(label), _) => label.clone(),
        (None, Some(_)) => None,
        (None, None) => return Vec::new(),
    };
    let say = |text: String| {
        // Why not, only when it was asked for outright.
        typed.as_ref().map(|_| {
            let mut note = note_row(text, Glyph::Focus, "Focus");
            note.score = LEAD;
            note
        })
    };
    let Some((meeting, at, in_ms)) = &ctx.next_meeting else {
        return say("No meetings on your calendar".into()).into_iter().collect();
    };
    let minutes = in_ms / 60_000;
    if minutes < MEETING_MIN_MINUTES {
        let when = if minutes < 1 { "is starting now".to_string() } else { format!("starts in {}", minutes_said(minutes)) };
        return say(format!("{meeting} {when}")).into_iter().collect();
    }
    if minutes > super::search::MAX_FOCUS_MINUTES as i64 {
        return say("Your next meeting is more than 4 hours away".into()).into_iter().collect();
    }
    let task = label.as_deref().and_then(|l| task_for(&ctx.tasks, l));
    let action = Action::StartFocus(minutes as u32, label.clone(), task.map(|t| t.id.clone()));
    let mut sub = format!("{}, to {}", minutes_said(minutes), at.format("%H:%M"));
    match (task, &label) {
        (Some(task), _) => sub.push_str(&format!(" · For your task “{}”", task.title)),
        (None, Some(label)) => sub.push_str(&format!(" · Labelled “{label}”")),
        (None, None) => {}
    }
    let placed = match loose {
        Some((score, at)) if typed.is_none() => (score - LOOSE_PENALTY, at),
        _ => (LEAD, Vec::new()),
    };
    let mut item = row(format!("Focus until {meeting}"), Glyph::Focus, "Focus", action, sub, placed);
    item.preview = Some(Preview::Focus(minutes as u32, task.map(|t| t.title.clone()).or(label)));
    vec![item]
}

#[cfg(test)]
mod tests {
    use super::super::search::tests::ctx;
    use super::super::search::{search, search_in, suggestions, SessionNow};
    use super::*;
    use chrono::{NaiveDate, NaiveDateTime};

    fn at(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 26).unwrap().and_hms_opt(hour, minute, 0).unwrap()
    }

    fn running(paused: bool) -> Context {
        let mut c = ctx();
        c.live.session = Some(SessionNow { label: Some("essay".into()), left_ms: 32 * 60_000, paused });
        c
    }

    fn timer(id: i64, name: &str, minutes: i64) -> TimerNow {
        TimerNow { id, name: name.into(), left_ms: minutes * 60_000, ends: at(14, 0) + Duration::minutes(minutes) }
    }

    fn titles(items: &[Item]) -> Vec<&str> {
        items.iter().map(|i| i.title.as_str()).collect()
    }

    fn with_modes() -> Context {
        use crate::focus_modes::{FocusMode, ModeApp};
        let mut c = ctx();
        c.modes = vec![
            FocusMode { name: "Study".into(), minutes: 50, list: Some("Social".into()), apps: vec![ModeApp { name: "Word".into(), path: "w".into() }] },
            FocusMode { name: "Write".into(), minutes: 25, list: None, apps: vec![] },
        ];
        c
    }

    #[test]
    fn a_mode_is_found_by_its_name_and_runs_with_enter() {
        let c = with_modes();
        let found = search(&c, "study");
        assert_eq!((found[0].title.as_str(), found[0].action.clone()), ("Study", Action::RunMode("Study".into())));
        assert_eq!(found[0].sub.as_deref(), Some("50 min · blocks Social · opens Word"));
        assert_eq!(found[0].section, "Modes");
        // A few letters of it find it too, in any case.
        assert_eq!(search(&c, "STU")[0].action, Action::RunMode("Study".into()));
        // A name that isn't one finds nothing of the kind.
        assert!(!search(&c, "nothing").iter().any(|i| matches!(i.action, Action::RunMode(_))));
    }

    #[test]
    fn mode_or_modes_lists_them_all_in_the_order_they_were_made() {
        let c = with_modes();
        for word in ["mode", "modes"] {
            let all = search(&c, word);
            let modes: Vec<&Item> = all.iter().filter(|i| matches!(i.action, Action::RunMode(_))).collect();
            assert_eq!(modes.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), ["Study", "Write"]);
            assert_eq!(all[0].title, "Study");
        }
        // With none saved, the word offers none.
        assert!(!search(&ctx(), "modes").iter().any(|i| matches!(i.action, Action::RunMode(_))));
    }

    #[test]
    fn every_theme_there_is_can_be_chosen() {
        for g in crate::appearance::GROUNDS {
            assert!(THEMES.iter().any(|t| t.0 == g.id), "{}", g.id);
        }
        assert_eq!(THEMES.len(), crate::appearance::GROUNDS.len() + 1);
    }

    #[test]
    fn theme_lists_them_all_and_marks_the_one_in_use() {
        let mut c = ctx();
        c.live.theme = "walnut".into();
        let all = search(&c, "theme");
        let themes: Vec<&Item> = all.iter().filter(|i| i.section == "Theme").collect();
        assert_eq!(themes.len(), 9);
        assert_eq!((themes[0].title.as_str(), themes[0].action.clone()), ("Theme: System", Action::Theme("system")));
        assert_eq!(all[0].title, "Theme: System");
        let in_use: Vec<&str> = themes.iter().filter(|i| i.detail.as_deref() == Some("In use")).map(|i| i.title.as_str()).collect();
        assert_eq!(in_use, ["Theme: Walnut"]);
        // The settings row is still there, under them.
        assert!(all.iter().any(|i| i.title == "Theme" && i.section == "Settings"));
        assert!(Action::Theme("dark").keeps_palette_open() && Action::Theme("dark").acts_in_place());
    }

    #[test]
    fn a_theme_is_found_by_its_name_either_side_of_the_word_or_alone() {
        let c = ctx();
        for q in ["theme walnut", "walnut theme", "Theme WAL", "walnut", "waln"] {
            let first = &search(&c, q)[0];
            assert_eq!((first.title.as_str(), first.action.clone()), ("Theme: Walnut", Action::Theme("walnut")), "{q}");
        }
        // Ink and Paper answer to dark and light too.
        assert_eq!(search(&c, "theme dark")[0].action, Action::Theme("dark"));
        assert_eq!(search(&c, "theme ink")[0].action, Action::Theme("dark"));
        assert_eq!(search(&c, "light theme")[0].title, "Theme: Paper");
        // A letter or two offers none; an app of the name stays first.
        assert!(theme_rows(&c, "sa").is_empty());
        let mut with_app = ctx();
        with_app.apps.push(super::super::AppEntry { name: "Sage".into(), path: r"C:\Start\Sage.lnk".into(), used_ms: 0 });
        let found = search(&with_app, "sage");
        assert_eq!((found[0].title.as_str(), found[0].section), ("Sage", "Apps"));
        assert!(found.iter().any(|i| i.action == Action::Theme("sage")));
    }

    #[test]
    fn a_running_session_can_be_paused_extended_and_ended() {
        let c = running(false);
        let pause = &search(&c, "pause")[0];
        assert_eq!((pause.title.as_str(), pause.action.clone(), pause.sub.as_deref()), ("Pause the focus session", Action::Session(SessionDo::Pause), Some("32m left · “essay”")));
        assert!(session_rows(&c, "resume").is_empty());
        assert_eq!(search(&c, "extend 10")[0].action, Action::Session(SessionDo::Extend(10)));
        assert_eq!(search(&c, "extend 10")[0].title, "Add 10 minutes to the focus session");
        assert_eq!(search(&c, "extend")[0].action, Action::Session(SessionDo::Extend(5)));
        assert_eq!(search(&c, "extend 1")[0].title, "Add 1 minute to the focus session");
        for q in ["end focus", "stop focus", "end"] {
            assert_eq!(search(&c, q)[0].action, Action::Session(SessionDo::End), "{q}");
        }
        // Found by other words too, among the rest.
        assert!(search(&c, "more time").iter().any(|i| i.action == Action::Session(SessionDo::Extend(5))));
        // Pausing leaves the palette open to resume; ending puts it away.
        assert!(Action::Session(SessionDo::Pause).keeps_palette_open() && !Action::Session(SessionDo::End).keeps_palette_open());
    }

    #[test]
    fn a_paused_session_offers_to_resume() {
        let c = running(true);
        let resume = &search(&c, "resume")[0];
        assert_eq!((resume.action.clone(), resume.sub.as_deref()), (Action::Session(SessionDo::Resume), Some("Paused · 32m left · “essay”")));
        assert!(session_rows(&c, "pause").is_empty());
    }

    #[test]
    fn with_no_session_there_is_nothing_to_pause_extend_or_end() {
        let c = ctx();
        for q in ["pause", "resume", "extend 10", "end focus", "end"] {
            assert!(session_rows(&c, q).is_empty(), "{q}");
        }
        // The idle palette still offers to start one.
        assert!(search(&c, "focus").iter().any(|i| i.title == "Start a 25-minute focus session"));
        assert!(!search(&running(false), "focus").iter().any(|i| i.title == "Start a 25-minute focus session"));
    }

    #[test]
    fn strict_mode_sends_ending_and_allowing_to_where_the_wait_is() {
        let mut c = running(false);
        c.live.strict = true;
        c.live.blocked = Some(("Steam".into(), "app:steam".into()));
        let end = &search(&c, "end focus")[0];
        assert_eq!((end.action.clone(), end.sub.as_deref()), (Action::Go("focus", None), Some("Strict mode: the Focus page counts down first")));
        assert_eq!(search(&c, "allow")[0].action, Action::ShowBlocked);
        // Pausing and extending aren't held by strict mode.
        assert_eq!(search(&c, "pause")[0].action, Action::Session(SessionDo::Pause));
    }

    #[test]
    fn allow_is_offered_only_while_something_is_blocked() {
        assert!(allow_rows(&ctx(), "allow").is_empty());
        let mut c = running(false);
        c.live.blocked = Some(("Steam".into(), "app:steam".into()));
        for q in ["allow", "allow 5", "Allow 30"] {
            let first = &search(&c, q)[0];
            assert_eq!((first.title.as_str(), first.action.clone()), ("Allow Steam for 5 minutes", Action::Allow("app:steam".into())), "{q}");
        }
    }

    #[test]
    fn timer_starts_one_with_minutes_and_a_name() {
        let c = ctx();
        let tea = &search(&c, "timer 10 tea")[0];
        assert_eq!((tea.title.as_str(), tea.sub.as_deref(), tea.action.clone()), ("Start a 10-minute timer", Some("Called “tea”"), Action::StartTimer(10, Some("tea".into()))));
        let bare = &search(&c, "Timer 10m")[0];
        assert_eq!((bare.sub.as_deref(), bare.action.clone()), (Some("No name"), Action::StartTimer(10, None)));
        assert_eq!(search(&c, "timer 25 min green tea")[0].action, Action::StartTimer(25, Some("green tea".into())));
        assert_eq!(search(&c, "timer 999")[0].action, Action::StartTimer(240, None));
        // Starting one puts the palette away.
        assert!(!Action::StartTimer(10, None).keeps_palette_open());
    }

    #[test]
    fn timer_alone_lists_the_running_ones_and_says_what_to_type() {
        let mut c = ctx();
        c.live.timers = vec![timer(7, "tea", 6), timer(8, "laundry", 25)];
        let rows = search(&c, "timer");
        let timers: Vec<&Item> = rows.iter().filter(|i| i.section == "Timers").collect();
        assert_eq!(titles(&timers.iter().map(|i| (*i).clone()).collect::<Vec<_>>()), ["Timer: tea", "Timer: laundry", "Type minutes and a name: timer 10 tea"]);
        assert_eq!(rows[0].title, "Timer: tea");
        assert_eq!(rows[0].sub.as_deref(), Some("6m left · rings at 14:06"));
        // Enter does nothing to a timer and leaves the palette be; Ctrl+Enter cancels it.
        assert!(!rows[0].is_note() && rows[0].action.keeps_palette_open());
        assert_eq!(rows[0].second, Some((Action::CancelTimer(7), "Cancel timer")));
        assert!(Action::CancelTimer(7).acts_in_place());
        assert!(timers[2].is_note());
        // With none running there's just the hint, and half the word finds it.
        assert_eq!(titles(&timer_rows(&ctx(), "timer")), ["Type minutes and a name: timer 10 tea"]);
        assert_eq!(titles(&timer_rows(&ctx(), "tim")), ["Type minutes and a name: timer 10 tea"]);
        assert!(timer_rows(&ctx(), "t").is_empty());
    }

    #[test]
    fn no_more_timers_start_once_enough_run() {
        let mut c = ctx();
        c.live.timers = (0..MAX_RUNNING as i64).map(|i| timer(i, "x", i + 1)).collect();
        let full = &search(&c, "timer 10 tea")[0];
        assert!(full.is_note());
        assert_eq!(full.sub.as_deref(), Some("20 timers are running already. Cancel one first."));
    }

    #[test]
    fn the_soonest_timer_is_suggested() {
        let mut c = ctx();
        assert!(suggestion(&c).is_none());
        c.live.timers = vec![timer(7, "tea", 6), timer(8, "laundry", 25)];
        let rows = suggestions(&c).rows;
        let row = rows.iter().find(|i| i.title == "Timer: tea").expect("the timer's row");
        assert_eq!((row.section, row.sub.as_deref()), ("Suggested", Some("6m left")));
        assert_eq!(row.second, Some((Action::CancelTimer(7), "Cancel timer")));
    }

    #[test]
    fn tracking_pauses_until_resumed_or_for_some_minutes() {
        let mut c = ctx();
        c.live.now = at(14, 32);
        let pause = &search(&c, "pause tracking")[0];
        assert_eq!((pause.title.as_str(), pause.sub.as_deref(), pause.action.clone()), ("Pause tracking", Some("Until you resume it"), Action::PauseTracking(None)));
        let timed = &search(&c, "pause tracking 30")[0];
        assert_eq!((timed.title.as_str(), timed.sub.as_deref(), timed.action.clone()), ("Pause tracking for 30 minutes", Some("Resumes at 15:02"), Action::PauseTracking(Some(30))));
        assert!(tracking_rows(&c, "resume tracking").is_empty());

        c.live.tracking_paused = Some(Some(at(15, 2)));
        let resume = &search(&c, "resume tracking")[0];
        assert_eq!((resume.title.as_str(), resume.sub.as_deref(), resume.action.clone()), ("Resume tracking", Some("Paused until 15:02"), Action::ResumeTracking));
        // Paused, a plain pause isn't offered, but the time to resume can be moved.
        assert!(tracking_rows(&c, "pause tracking").is_empty());
        assert_eq!(search(&c, "pause tracking 10")[0].action, Action::PauseTracking(Some(10)));
        c.live.tracking_paused = Some(None);
        assert_eq!(search(&c, "resume tracking")[0].sub.as_deref(), Some("Paused"));
    }

    #[test]
    fn pause_alone_means_the_session_first_and_tracking_under_it() {
        let both = search(&running(false), "pause");
        assert_eq!(titles(&both[..2]), ["Pause the focus session", "Pause tracking"]);
        assert_eq!(search(&ctx(), "pause")[0].title, "Pause tracking");
        let mut c = running(true);
        c.live.tracking_paused = Some(None);
        assert_eq!(titles(&search(&c, "resume")[..2]), ["Resume the focus session", "Resume tracking"]);
    }

    #[test]
    fn focus_until_meeting_runs_to_the_meetings_start() {
        let c = ctx(); // Design review at 14:00, in 35 minutes
        for q in ["focus until meeting", "focus until next meeting", "focus till meeting", "Focus Until My Next Meeting", "focus until", "focus un", "focus until mee"] {
            let first = &search(&c, q)[0];
            assert_eq!((first.title.as_str(), first.sub.as_deref(), first.action.clone()), ("Focus until Design review", Some("35 minutes, to 14:00"), Action::StartFocus(35, None, None)), "{q}");
        }
        let labelled = &search(&c, "focus until meeting Essay draft")[0];
        assert_eq!(labelled.action, Action::StartFocus(35, Some("Essay draft".into()), None));
        assert_eq!(labelled.sub.as_deref(), Some("35 minutes, to 14:00 · Labelled “Essay draft”"));
        assert!(matches!(labelled.preview, Some(Preview::Focus(35, Some(_)))));
        // Found by "meeting" too, under the meeting itself.
        assert!(search(&c, "meeting").iter().any(|i| i.title == "Focus until Design review"));
        // `focus 50` and other words after `focus` are not this.
        assert!(meeting_rows(&c, "focus 50").is_empty() && meeting_rows(&c, "focus unicorn farm").is_empty());
    }

    #[test]
    fn focus_until_meeting_says_why_it_cannot() {
        let note = |c: &Context| {
            let rows = meeting_rows(c, "focus until meeting");
            assert!(rows.len() == 1 && rows[0].is_note());
            rows[0].title.clone()
        };
        let mut c = ctx();
        c.next_meeting = None;
        assert_eq!(note(&c), "No meetings on your calendar");
        c.next_meeting = Some(("Design review".into(), at(14, 0), 3 * 60_000 + 20_000));
        assert_eq!(note(&c), "Design review starts in 3 minutes");
        c.next_meeting = Some(("Design review".into(), at(14, 0), 20_000));
        assert_eq!(note(&c), "Design review is starting now");
        c.next_meeting = Some(("Design review".into(), at(20, 0), 241 * 60_000));
        assert_eq!(note(&c), "Your next meeting is more than 4 hours away");
        // Not asked for outright, it just isn't offered.
        assert!(meeting_rows(&c, "meeting").is_empty());
        c.next_meeting = Some(("Design review".into(), at(18, 0), 240 * 60_000));
        assert_eq!(meeting_rows(&c, "focus until meeting")[0].action, Action::StartFocus(240, None, None));
    }

    #[test]
    fn commands_with_numbers_are_not_sums() {
        let mut c = running(false);
        c.live.blocked = Some(("Steam".into(), "app:steam".into()));
        for q in ["extend 10", "timer 10", "allow 5", "pause tracking 30"] {
            assert!(leads(q), "{q}");
            assert!(!search(&c, q).iter().any(|i| i.section == "Calculator"), "{q}");
        }
        assert!(!leads("10 + 5") && !leads("timer") && !leads("extended 10"));
    }
    #[test]
    fn pomodoro_is_typed_with_lengths_and_a_label() {
        let c = ctx();
        let first = |q: &str| search(&c, q).into_iter().next().unwrap();
        let plain = first("pomodoro");
        assert_eq!((plain.title.as_str(), plain.action.clone()), ("Start a pomodoro", Action::Pomodoro(None, None, None, None)));
        assert_eq!(plain.sub.as_deref(), Some("Rounds and breaks as set in Settings"));
        let sized = first("pomodoro 50 10");
        assert_eq!((sized.action.clone(), sized.sub.as_deref()), (Action::Pomodoro(Some(50), Some(10), None, None), Some("50m rounds, 10m breaks")));
        assert_eq!(first("pomodoro 50 10 essay draft").action, Action::Pomodoro(Some(50), Some(10), Some("essay draft".into()), None));
        assert_eq!(first("pomodoro 40 essay").action, Action::Pomodoro(Some(40), None, Some("essay".into()), None));
        assert_eq!(first("pomodoro essay").action, Action::Pomodoro(None, None, Some("essay".into()), None));
        // Found by its other word, too, but not by "focus", which is the plain command's.
        assert!(search(&c, "tomato").iter().any(|i| matches!(i.action, Action::Pomodoro(..))));
        assert!(!search(&c, "pomodorox").iter().any(|i| i.title == "Start a pomodoro" && i.score >= LEAD));
    }

    #[test]
    fn a_pomodoro_break_offers_to_skip_or_end_it() {
        let mut c = ctx();
        assert!(!search(&c, "skip").iter().any(|i| i.action == Action::Session(SessionDo::SkipBreak)));
        c.live.cycle = Some(super::super::search::CycleNow { round: 3, rounds: 4, on_break: true, left_ms: 3 * 60_000, waiting: false });
        let skip = search(&c, "skip").into_iter().next().unwrap();
        assert_eq!((skip.title.as_str(), skip.action.clone(), skip.sub.as_deref()), ("Skip the break", Action::Session(SessionDo::SkipBreak), Some("Break · 3m left · round 3 of 4 next")));
        assert_eq!(search(&c, "end focus")[0].title, "End the pomodoro");
        assert_eq!(search(&c, "end focus")[0].action, Action::Session(SessionDo::End));
        c.live.cycle.as_mut().unwrap().waiting = true;
        assert_eq!(search(&c, "skip")[0].sub.as_deref(), Some("Break over · round 3 of 4 waits for you"));
        // A round running says which it is.
        c.live.cycle = Some(super::super::search::CycleNow { round: 2, rounds: 4, ..Default::default() });
        c.live.session = Some(SessionNow { label: None, left_ms: 10 * 60_000, paused: false });
        assert_eq!(search(&c, "pause")[0].sub.as_deref(), Some("10m left · round 2 of 4"));
        assert_eq!(search(&c, "end")[0].title, "End the pomodoro");
    }
    #[test]
    fn clip_lists_the_latest_copies_and_only_when_asked_for() {
        use super::super::search::ClipNow;
        let mut c = ctx();
        let clip = |text: &str, age_ms: i64| ClipNow { text: text.into(), age_ms };
        c.live.clips = Some(vec![clip("Invoice 4471\nTotal due", 30_000), clip("https://example.com/a-long-address", 5 * 60_000), clip("x", 3_600_000)]);
        let rows: Vec<_> = search(&c, "clip").into_iter().filter(|i| i.section == "Clipboard").collect();
        let shown: Vec<_> = rows.iter().map(|i| (i.title.as_str(), i.sub.as_deref())).collect();
        assert_eq!(
            shown,
            [
                ("Invoice 4471", Some("just now · 2 lines")),
                ("https://example.com/a-long-address", Some("5m ago · 34 characters")),
                ("x", Some("1h 00m ago · 1 character")),
                ("Clear clipboard history", Some("Forget 3 copied")),
            ]
        );
        assert_eq!(rows[0].action, Action::Copy("Invoice 4471\nTotal due".into()));
        assert_eq!(rows[3].action, Action::ClearClips);
        assert_eq!(search(&c, "clipboard")[0].title, "Invoice 4471");
        // A filter shows only what contains it, and no Clear row.
        let found = search(&c, "clip EXAMPLE");
        assert_eq!(found.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), ["https://example.com/a-long-address"]);
        assert_eq!(search(&c, "clip nothing like it")[0].title, "No copies contain “nothing like it”");
        assert!(search_in(&c, Mode::Clipboard, "").len() == 4);
        // Never in an ordinary search, whatever the copies hold.
        for q in ["invoice", "x", "example", "total", "inv", "paste"] {
            assert!(!search(&c, q).iter().any(|i| i.section == "Clipboard"), "{q}");
        }
        assert_eq!(search(&c, "clear clipboard history")[0].action, Action::ClearClips);
    }

    #[test]
    fn clip_says_so_when_there_is_nothing_or_it_is_off() {
        let mut c = ctx();
        c.live.clips = Some(Vec::new());
        assert_eq!(search(&c, "clip")[0].title, "Nothing copied since Kairo started");
        assert!(search(&c, "clear clipboard")[0].title != "Clear clipboard history");
        c.live.clips = None;
        assert!(search(&c, "clip")[0].title.starts_with("Clipboard history is off"));
    }
    #[test]
    fn clip_shows_the_latest_five_and_a_row_for_all_of_them() {
        use super::super::search::ClipNow;
        let mut c = ctx();
        c.live.clips = Some((0..9).map(|i| ClipNow { text: format!("copy number {i}"), age_ms: (i + 1) * 120_000 }).collect());
        let rows: Vec<_> = search(&c, "clip").into_iter().filter(|i| i.section == "Clipboard").collect();
        let titles: Vec<_> = rows.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["copy number 0", "copy number 1", "copy number 2", "copy number 3", "copy number 4", "Show all 9 copies", "Clear clipboard history"]);
        assert_eq!(rows[5].action, Action::EnterMode(Mode::Clipboard));
        // The mode lists every copy, newest first, and Enter on one puts it back.
        let all = search_in(&c, Mode::Clipboard, "");
        assert_eq!(all.len(), 10);
        assert_eq!(all[8].title, "copy number 8");
        assert_eq!(all[8].action, Action::Copy("copy number 8".into()));
        assert_eq!(all[9].action, Action::ClearClips);
        // Typing filters them, with no Clear row among the matches.
        let found = search_in(&c, Mode::Clipboard, "NUMBER 7");
        assert_eq!(found.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), ["copy number 7"]);
        assert_eq!(search_in(&c, Mode::Clipboard, "zzz")[0].title, "No copies contain “zzz”");
        // Nothing to show or switched off.
        c.live.clips = Some(Vec::new());
        assert_eq!(search_in(&c, Mode::Clipboard, "")[0].title, "Nothing copied since Kairo started");
        c.live.clips = None;
        assert_eq!(search_in(&c, Mode::Clipboard, "")[0].action, Action::Setting("clipboard-history"));
    }
    #[test]
    fn lock_and_sleep_act_at_once() {
        let c = ctx();
        for (q, power) in [("lock", Power::Lock), ("lock screen", Power::Lock), ("sleep", Power::Sleep), ("suspend", Power::Sleep)] {
            let first = &search(&c, q)[0];
            assert_eq!((first.action.clone(), first.section), (Action::Power(power), "System"), "{q}");
        }
        assert!(!Action::Power(Power::Lock).keeps_palette_open());
    }

    #[test]
    fn shutting_down_asks_twice_and_cancel_or_other_typing_leaves_it() {
        let mut c = ctx();
        for (q, power) in [("shut down", Power::ShutDown), ("shutdown", Power::ShutDown), ("restart", Power::Restart), ("reboot", Power::Restart), ("log out", Power::LogOut), ("sign out", Power::LogOut)] {
            let first = &search(&c, q)[0];
            assert_eq!(first.action, Action::ConfirmPower(power, q.to_string()), "{q}");
            assert!(first.action.acts_in_place(), "{q}");
        }
        // The first Enter was on "shut down": now the top row confirms, with Cancel under it.
        c.live.pending_power = Some((Power::ShutDown, "shut down".into()));
        let rows = search(&c, "shut down");
        assert_eq!((rows[0].title.as_str(), rows[0].action.clone()), ("Confirm: shut down", Action::PowerNow(Power::ShutDown)));
        assert_eq!((rows[1].title.as_str(), rows[1].action.clone()), ("Cancel", Action::CancelPower));
        assert!(Action::CancelPower.acts_in_place() && !Action::PowerNow(Power::ShutDown).keeps_palette_open());
        // Typing something else, or another command, doesn't carry it over.
        assert!(!search(&c, "shut dow").iter().any(|i| i.action == Action::PowerNow(Power::ShutDown)));
        assert_eq!(search(&c, "restart")[0].action, Action::ConfirmPower(Power::Restart, "restart".into()));
        // Spaces and capitals don't make it a different thing.
        assert_eq!(search(&c, "  Shut   DOWN ")[0].action, Action::PowerNow(Power::ShutDown));
    }

    #[test]
    fn two_letters_never_offer_to_end_a_session_of_the_computer() {
        let c = ctx();
        for q in ["re", "sh", "lo", "sl"] {
            assert!(!search(&c, q).iter().any(|i| i.section == "System"), "{q}");
        }
        // Found by other words from three characters on, below the apps.
        let loose = search(&c, "power");
        assert!(loose.iter().any(|i| i.action == Action::ConfirmPower(Power::ShutDown, "power".into())));
    }

    #[test]
    fn kill_lists_the_matching_programs_with_how_many_processes_each_has() {
        use crate::system::Program;
        let mut c = ctx();
        c.programs = vec![
            Program { name: "chrome".into(), count: 12 },
            Program { name: "chromedriver".into(), count: 1 },
            Program { name: "code".into(), count: 4 },
        ];
        let rows = search(&c, "kill chrome");
        let shown: Vec<(&str, Action)> = rows.iter().filter(|i| i.section == "System").map(|i| (i.title.as_str(), i.action.clone())).collect();
        assert_eq!(shown, [("Kill chrome", Action::Kill("chrome".into())), ("Kill chromedriver", Action::Kill("chromedriver".into()))]);
        assert_eq!(rows[0].sub.as_deref(), Some("Ends it at once · 12 processes, lost with their unsaved work"));
        assert!(rows[1].sub.as_deref().unwrap().contains("1 process,"));
        assert_eq!(search(&c, "end task code")[0].action, Action::Kill("code".into()));
        assert_eq!(search(&c, "force quit code")[0].action, Action::Kill("code".into()));
        // Typed alone it says what to type; with nothing matching, that.
        assert_eq!(search(&c, "kill")[0].title, "Type a program's name, like kill chrome");
        assert_eq!(search(&c, "kill zzzq")[0].title, "No running program matches “zzzq”");
        // Never from an ordinary search, and never the same word as a longer one.
        assert!(!search(&c, "chrome").iter().any(|i| matches!(i.action, Action::Kill(..))));
        assert!(!search(&c, "killer").iter().any(|i| matches!(i.action, Action::Kill(..))));
        assert!(!Action::Kill("chrome".into()).keeps_palette_open());
    }

    #[test]
    fn kill_lists_no_more_than_eight() {
        use crate::system::Program;
        let mut c = ctx();
        c.programs = (0..20).map(|i| Program { name: format!("worker{i:02}"), count: 1 }).collect();
        assert_eq!(search(&c, "kill worker").iter().filter(|i| matches!(i.action, Action::Kill(..))).count(), MAX_KILL_ROWS);
    }
}
