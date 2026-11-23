//! The palette's tools for the rest of the desktop: moving the window that was in front (halves,
//! maximize, center, next display, restore), window layouts, and your own scripts. No drawing
//! here, so all of it can be tested.

use super::commands::{placed, row, LEAD};
use super::search::{after, note_row, Action, Context, Glyph, Item};
use crate::snap::Snap;

/// A window's title past this is cut on the row that says what moves.
const TITLE_CHARS: usize = 50;

fn short(title: &str) -> String {
    if title.chars().count() > TITLE_CHARS {
        format!("{}…", title.chars().take(TITLE_CHARS).collect::<String>().trim_end())
    } else {
        title.to_string()
    }
}

/// "Left half", "maximize", "next display" and the rest, for the window that was in front when
/// the palette opened. Nothing when there was none.
pub fn snap_rows(ctx: &Context, query: &str) -> Vec<Item> {
    let Some(front) = ctx.windows.first() else {
        // Asked for outright with no other window open: say so, rather than nothing.
        let asked = Snap::ALL.into_iter().find(|s| s.exact().iter().any(|e| query.trim().eq_ignore_ascii_case(e)));
        return asked
            .map(|s| {
                let mut item = note_row(format!("{}: no window to move. Open the palette from the window you want to move", s.title()), Glyph::Window, "Window");
                item.score = LEAD;
                item
            })
            .into_iter()
            .collect();
    };
    let says = format!("Moves “{}”", short(&front.title));
    Snap::ALL
        .into_iter()
        .filter_map(|snap| {
            let place = placed(query, snap.exact(), snap.title(), snap.words())?;
            Some(row(snap.title(), Glyph::Window, "Window", Action::Snap(snap, front.id.clone()), says.clone(), place))
        })
        .collect()
}

/// `save layout writing` saves where the open windows are; `layout` lists the saved ones and
/// `layout writing` puts that one back. A layout is also found by its name once a couple of
/// letters are typed.
pub fn layout_rows(ctx: &Context, query: &str) -> Vec<Item> {
    let q = query.trim();
    if let Some(name) = after(q, "save layout ").map(str::trim).filter(|n| !n.is_empty()) {
        let replaces = ctx.layouts.iter().any(|l| l.name.eq_ignore_ascii_case(name));
        let sub = if replaces { "Replaces the saved one with where the open windows are now" } else { "Keeps where the open windows are, to put them back later" };
        return vec![row(format!("Save layout “{name}”"), Glyph::Window, "Layouts", Action::SaveLayout(name.to_string()), sub, (LEAD, Vec::new()))];
    }
    if q.eq_ignore_ascii_case("save layout") {
        let mut item = note_row("Type a name after it, like “save layout writing”", Glyph::Window, "Layouts");
        item.score = LEAD;
        return vec![item];
    }
    let filter = if q.eq_ignore_ascii_case("layout") || q.eq_ignore_ascii_case("layouts") { Some("") } else { after(q, "layout ").map(str::trim) };
    let mut items: Vec<Item> = ctx
        .layouts
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            let place = match filter {
                Some(f) if l.name.to_lowercase().starts_with(&f.to_lowercase()) => (LEAD - 1.0 - i as f64 / 100.0, Vec::new()),
                Some(_) => return None,
                None => placed(q, &[], &l.name, "layout windows")?,
            };
            let n = l.windows.len();
            let mut sub = format!("Puts {n} {} back", if n == 1 { "window" } else { "windows" });
            if let Some(label) = &l.focus_label {
                sub.push_str(&format!(" · also when focus “{label}” starts"));
            }
            Some(row(l.name.clone(), Glyph::Window, "Layouts", Action::RestoreLayout(l.name.clone()), sub, place))
        })
        .collect();
    if filter == Some("") || (filter.is_none() && placed(q, &[], "Save layout", "window layout arrangement").is_some()) {
        let place = if filter.is_some() { (LEAD - 2.0, Vec::new()) } else { placed(q, &[], "Save layout", "window layout arrangement").unwrap_or_default() };
        let mut item = note_row("Type “save layout” and a name to keep where your windows are", Glyph::Window, "Layouts");
        item.score = place.0;
        if ctx.layouts.is_empty() || filter.is_some() {
            items.push(item);
        }
    }
    items
}

/// Your scripts, found by their titles like your commands: from the first letter by their start,
/// loosely once a couple of letters are typed. One that takes an argument takes what's typed
/// after its title. `scripts` lists them all, with a row for the folder they're in.
pub fn script_rows(ctx: &Context, query: &str) -> Vec<Item> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }
    let window = ctx.windows.first().map(|w| w.id.clone());
    let listing = q.eq_ignore_ascii_case("scripts") || q.eq_ignore_ascii_case("script");
    let mut items: Vec<Item> = ctx
        .scripts
        .iter()
        .enumerate()
        .filter_map(|(i, sc)| {
            let file = std::path::Path::new(&sc.path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
            let says = sc.description.clone().unwrap_or_else(|| format!("Runs {file}"));
            if sc.argument.is_some() {
                if let Some(arg) = after(q, &format!("{} ", sc.title)).map(str::trim).filter(|a| !a.is_empty()) {
                    let mut item = row(format!("{} {arg}", sc.title), Glyph::Script, "Scripts", Action::RunScript(sc.path.clone(), Some(arg.to_string()), window.clone()), says, (LEAD + 1.0 - i as f64 / 100.0, Vec::new()));
                    item.at = (0..sc.title.chars().count()).collect();
                    return Some(item);
                }
            }
            let place = if listing {
                (LEAD - 1.0 - i as f64 / 100.0, Vec::new())
            } else if sc.title.to_lowercase().starts_with(&q.to_lowercase()) {
                (LEAD - if sc.title.eq_ignore_ascii_case(q) { 0.0 } else { 1.0 } - i as f64 / 100.0, (0..q.chars().count()).collect())
            } else {
                placed(q, &[], &sc.title, &sc.description.clone().unwrap_or_default())?
            };
            let mut item = row(sc.title.clone(), Glyph::Script, "Scripts", Action::RunScript(sc.path.clone(), None, window.clone()), says, place);
            if let Some(arg) = &sc.argument {
                item.action = Action::Type(format!("{} ", sc.title));
                item.sub = Some(format!("Type the {} after “{} ”", arg.to_lowercase(), sc.title));
                item.second = Some((Action::RunScript(sc.path.clone(), None, window.clone()), "Run without"));
            }
            Some(item)
        })
        .collect();
    if listing || placed(q, &[], "Open scripts folder", "scripts folder your own").is_some() {
        let place = if listing { (LEAD - 2.0, Vec::new()) } else { placed(q, &[], "Open scripts folder", "scripts folder your own").unwrap_or_default() };
        let sub = if ctx.scripts.is_empty() { "Put a script there to run it from here. Opening it adds an example" } else { "Where your scripts are" };
        items.push(row("Open scripts folder", Glyph::Script, "Scripts", Action::OpenScripts, sub, place));
    }
    items
}

/// `remind me in 20m to call Leo`: one row saying when it rings, or what's missing.
pub fn remind_rows(ctx: &Context, query: &str) -> Vec<Item> {
    use chrono::TimeZone;
    let Some(text) = crate::remind::led_by(query) else { return Vec::new() };
    let mut item = match crate::remind::parse(text, ctx.live.now) {
        Ok(ask) => {
            let Some(at_ms) = chrono::Local.from_local_datetime(&ask.at).earliest().map(|t| t.timestamp_millis()) else { return Vec::new() };
            let sub = format!("{} · added to Ledger as a task", crate::remind::when_said(ask.at, ctx.live.now));
            row(format!("Remind me: {}", ask.what), Glyph::Day, "Reminders", Action::Remind(ask.what.clone(), at_ms), sub, (LEAD, Vec::new()))
        }
        Err(missing) => note_row(missing, Glyph::Day, "Reminders"),
    };
    item.score = LEAD;
    vec![item]
}

/// A join row shows in the empty palette from this long before a meeting starts…
const JOIN_BEFORE_MIN: i64 = 5;
/// …until this long after.
const JOIN_AFTER_MIN: i64 = 10;

/// When a meeting is, as a row says it: "Starts in 3 min", "Started 4 min ago", "Tomorrow at
/// 09:00", with its call service when it has one.
fn meeting_when(m: &super::search::MeetingNow, now: chrono::NaiveDateTime) -> String {
    let minutes = |d: chrono::Duration| (d.num_seconds() + 59) / 60;
    let when = if m.start > now {
        let wait = m.start - now;
        match (minutes(wait), (m.start.date() - now.date()).num_days()) {
            (n, _) if n < 60 => format!("Starts in {n} min"),
            (_, 0) => format!("Today at {}", crate::quick_add::clock(m.start)),
            (_, 1) => format!("Tomorrow at {}", crate::quick_add::clock(m.start)),
            _ => format!("{} at {}", m.start.format("%A"), crate::quick_add::clock(m.start)),
        }
    } else {
        match (now - m.start).num_minutes() {
            0 => "Starting now".to_string(),
            n if n < 60 => format!("Started {n} min ago"),
            _ => format!("On until {}", crate::quick_add::clock(m.end)),
        }
    };
    match m.link.as_deref().and_then(crate::calendar::call_service) {
        Some(service) => format!("{when} · {service}"),
        None => when,
    }
}

fn join_row(m: &super::search::MeetingNow, now: chrono::NaiveDateTime, section: &'static str, place: (f64, Vec<usize>)) -> Item {
    match &m.link {
        Some(link) => row(format!("Join “{}”", m.title), Glyph::Link, section, Action::OpenUrl(link.clone()), meeting_when(m, now), place),
        // Nothing to open: said, with when it is.
        None => {
            let mut item = note_row(format!("“{}” has no video link", m.title), Glyph::Day, section);
            item.sub = Some(meeting_when(m, now));
            item.score = place.0;
            item
        }
    }
}

/// `agenda`, `meetings today` or `my schedule`: today's meetings still to come, each opening its
/// call (or Chrono's day when it has none).
pub fn agenda_rows(ctx: &Context, query: &str) -> Vec<Item> {
    let exact = ["agenda", "my agenda", "meetings", "meetings today", "today's meetings", "todays meetings", "my schedule", "my day"];
    let Some(place) = placed(query, &exact, "Today's meetings", "agenda schedule calendar meetings today day") else { return Vec::new() };
    if ctx.agenda.is_empty() {
        let mut item = note_row("No more meetings today", Glyph::Day, "Today's meetings");
        item.score = place.0;
        return if exact.iter().any(|e| query.trim().eq_ignore_ascii_case(e)) { vec![item] } else { Vec::new() };
    }
    let now = ctx.live.now;
    ctx.agenda
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let span = format!("{}–{}", crate::quick_add::clock(m.start), crate::quick_add::clock(m.end));
            let mut said = vec![if m.start <= now { format!("{span}, on now") } else { span }];
            said.extend(m.link.as_deref().and_then(crate::calendar::call_service).map(String::from));
            let action = match &m.link {
                Some(link) => Action::OpenUrl(link.clone()),
                None => Action::Go("today", None),
            };
            let mut item = row(m.title.clone(), if m.link.is_some() { Glyph::Link } else { Glyph::Day }, "Today's meetings", action, said.join(" · "), (place.0 - i as f64 / 100.0, Vec::new()));
            if m.link.is_some() {
                item.second = Some((Action::Go("today", None), "Show the day"));
            }
            item
        })
        .collect()
}

/// `join` opens the call of the meeting on now, or else the next one.
pub fn join_rows(ctx: &Context, query: &str) -> Vec<Item> {
    let exact = ["join", "join meeting", "join call", "join the meeting"];
    let Some(place) = placed(query, &exact, "Join meeting", "join meeting call video zoom meet teams webex") else { return Vec::new() };
    match &ctx.join {
        Some(m) => vec![join_row(m, ctx.live.now, "Meetings", place)],
        None if exact.iter().any(|e| query.trim().eq_ignore_ascii_case(e)) => {
            let mut item = note_row("No meetings coming up on your calendars", Glyph::Day, "Meetings");
            item.score = LEAD;
            vec![item]
        }
        None => Vec::new(),
    }
}

/// The meeting to join as the palette opens: from a few minutes before it starts until a few
/// after, and only when there's a call to open.
pub fn join_suggestion(ctx: &Context) -> Option<Item> {
    let m = ctx.join.as_ref().filter(|m| m.link.is_some())?;
    let now = ctx.live.now;
    let soon = m.start - chrono::Duration::minutes(JOIN_BEFORE_MIN) <= now && now <= m.start + chrono::Duration::minutes(JOIN_AFTER_MIN);
    (soon && now < m.end).then(|| join_row(m, now, "Suggested", (0.0, Vec::new())))
}

#[cfg(test)]
mod tests {
    use super::super::search::tests::ctx;
    use super::super::search::{search, WindowEntry};
    use super::*;
    use crate::platform::Rect;
    use crate::snap::{Layout, Placed};

    fn with_window() -> Context {
        let mut c = ctx();
        c.windows = vec![WindowEntry { id: "42".into(), title: "notes.md — Visual Studio Code".into(), app: "Code".into(), icon: None }];
        c
    }

    fn meeting(starts_in_min: i64, link: Option<&str>) -> (Context, chrono::NaiveDateTime) {
        let mut c = ctx();
        let now = chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap().and_hms_opt(9, 0, 0).unwrap();
        c.live.now = now;
        let start = now + chrono::Duration::minutes(starts_in_min);
        c.join = Some(super::super::search::MeetingNow { title: "Standup".into(), start, end: start + chrono::Duration::minutes(15), link: link.map(Into::into) });
        (c, now)
    }

    #[test]
    fn remind_me_makes_a_reminder_or_says_what_is_missing() {
        use chrono::TimeZone;
        let mut c = ctx();
        c.live.now = chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap().and_hms_opt(10, 0, 0).unwrap();
        let found = search(&c, "remind me in 20m to call Leo");
        assert_eq!(found[0].title, "Remind me: call Leo");
        assert_eq!(found[0].sub.as_deref(), Some("In 20 min, at 10:20 · added to Ledger as a task"));
        let ms = chrono::Local.from_local_datetime(&c.live.now).earliest().unwrap().timestamp_millis() + 20 * 60_000;
        assert_eq!(found[0].action, Action::Remind("call Leo".into(), ms));
        assert_eq!(found[0].action.verb(), "Remind me");
        // Nothing else leads it, the calculator included.
        assert!(!found.iter().any(|i| i.section == "Calculator"));
        let found = search(&c, "remind me to call Leo");
        assert!(found[0].is_note() && found[0].title == "Say when: in 20m, at 3pm or tomorrow at 9");
        assert_eq!(search(&c, "remind me")[0].title, "Say when and what: remind me in 20m to call Leo");
    }

    #[test]
    fn agenda_lists_todays_meetings_still_to_come() {
        use super::super::search::MeetingNow;
        let mut c = ctx();
        let now = chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap().and_hms_opt(9, 5, 0).unwrap();
        c.live.now = now;
        let t = |h: u32, m: u32| now.date().and_hms_opt(h, m, 0).unwrap();
        c.agenda = vec![
            MeetingNow { title: "Standup".into(), start: t(9, 0), end: t(9, 15), link: Some("https://meet.google.com/abc".into()) },
            MeetingNow { title: "Lunch with Sam".into(), start: t(12, 30), end: t(13, 30), link: None },
        ];
        let found: Vec<(String, Option<String>, Action)> = search(&c, "agenda").into_iter().filter(|i| i.section == "Today's meetings").map(|i| (i.title, i.sub, i.action)).collect();
        assert_eq!(
            found,
            vec![
                ("Standup".into(), Some("09:00–09:15, on now · Google Meet".into()), Action::OpenUrl("https://meet.google.com/abc".into())),
                ("Lunch with Sam".into(), Some("12:30–13:30".into()), Action::Go("today", None)),
            ]
        );
        assert_eq!(search(&c, "my schedule")[0].title, "Standup");
        c.agenda.clear();
        assert_eq!(search(&c, "meetings today")[0].title, "No more meetings today");
    }

    #[test]
    fn join_opens_the_call_of_the_meeting_on_now_or_next() {
        let (c, _) = meeting(3, Some("https://meet.google.com/abc-defg-hij"));
        let found = search(&c, "join");
        assert_eq!((found[0].title.as_str(), found[0].action.clone()), ("Join “Standup”", Action::OpenUrl("https://meet.google.com/abc-defg-hij".into())));
        assert_eq!(found[0].sub.as_deref(), Some("Starts in 3 min · Google Meet"));
        assert_eq!(found[0].action.verb(), "Open");
        let (c, _) = meeting(-4, Some("https://us02web.zoom.us/j/1"));
        assert_eq!(search(&c, "join meeting")[0].sub.as_deref(), Some("Started 4 min ago · Zoom"));
        let (c, _) = meeting(24 * 60 + 30, Some("https://us02web.zoom.us/j/1"));
        assert_eq!(search(&c, "join")[0].sub.as_deref(), Some("Tomorrow at 09:30 · Zoom"));
        // Found loosely too.
        let (c, _) = meeting(3, Some("https://us02web.zoom.us/j/1"));
        assert!(search(&c, "zoom").iter().any(|i| i.title == "Join “Standup”"));
    }

    #[test]
    fn join_says_so_when_there_is_no_call_or_no_meeting() {
        let (c, _) = meeting(3, None);
        let found = search(&c, "join");
        assert!(found[0].is_note());
        assert_eq!((found[0].title.as_str(), found[0].sub.as_deref()), ("“Standup” has no video link", Some("Starts in 3 min")));
        let mut c = ctx();
        c.join = None;
        assert_eq!(search(&c, "join")[0].title, "No meetings coming up on your calendars");
        assert!(!search(&c, "zoom").iter().any(|i| i.section == "Meetings"));
    }

    #[test]
    fn the_empty_palette_offers_the_call_from_just_before_until_just_after_it_starts() {
        use super::super::search::suggestions;
        let leads = |starts_in: i64, link: Option<&str>| {
            let (c, _) = meeting(starts_in, link);
            suggestions(&c).rows.first().map(|r| r.title.clone())
        };
        let link = Some("https://meet.google.com/abc");
        assert_eq!(leads(4, link).as_deref(), Some("Join “Standup”"));
        assert_eq!(leads(-9, link).as_deref(), Some("Join “Standup”"));
        // Too early, too late, or nothing to open: the palette opens as usual.
        assert_ne!(leads(20, link).as_deref(), Some("Join “Standup”"));
        assert_ne!(leads(-11, link).as_deref(), Some("Join “Standup”"));
        assert_ne!(leads(2, None).as_deref(), Some("Join “Standup”"));
    }

    #[test]
    fn snaps_move_the_window_that_was_in_front() {
        let c = with_window();
        let found = search(&c, "left half");
        assert_eq!(found[0].action, Action::Snap(Snap::LeftHalf, "42".into()));
        assert_eq!(found[0].sub.as_deref(), Some("Moves “notes.md — Visual Studio Code”"));
        assert_eq!(search(&c, "maximize")[0].action, Action::Snap(Snap::Maximize, "42".into()));
        assert_eq!(search(&c, "next display")[0].action, Action::Snap(Snap::NextDisplay, "42".into()));
        assert_eq!(search(&c, "restore")[0].action, Action::Snap(Snap::Restore, "42".into()));
        for (typed, snap) in [("left third", Snap::LeftThird), ("right 2/3", Snap::RightTwoThirds), ("top left", Snap::TopLeft), ("bottom right", Snap::BottomRight), ("left", Snap::LeftHalf)] {
            assert_eq!(search(&c, typed)[0].action, Action::Snap(snap, "42".into()), "{typed}");
        }
        assert_eq!(search(&c, "left half")[0].action.verb(), "Snap");
        // Found loosely too.
        assert!(search(&c, "snap").iter().any(|i| i.section == "Window"));
    }

    #[test]
    fn with_no_window_in_front_nothing_offers_to_move_one() {
        let c = ctx();
        let found = search(&c, "left half");
        assert!(found[0].is_note() && found[0].title.starts_with("Left half: no window to move"));
        assert!(!found.iter().any(|i| matches!(i.action, Action::Snap(..))));
        // Not for words merely like one.
        assert!(!search(&c, "lef").iter().any(|i| i.section == "Window"));
    }

    fn layout(name: &str, label: Option<&str>) -> Layout {
        Layout { name: name.into(), windows: vec![Placed { app: "code.exe".into(), title: "x".into(), rect: Rect { x: 0, y: 0, w: 1, h: 1 } }], focus_label: label.map(Into::into) }
    }

    fn script(title: &str, argument: Option<&str>) -> crate::scripts::Script {
        crate::scripts::Script { path: format!("/s/{title}.sh"), title: title.into(), description: None, argument: argument.map(Into::into), output: crate::scripts::Output::Notify }
    }

    #[test]
    fn scripts_are_found_by_title_and_take_what_is_typed_after_it() {
        let mut c = with_window();
        c.scripts = vec![script("Deploy site", Some("Branch")), script("Clean downloads", None)];
        let found = search(&c, "deploy site main");
        assert_eq!((found[0].title.as_str(), found[0].action.clone()), ("Deploy site main", Action::RunScript("/s/Deploy site.sh".into(), Some("main".into()), Some("42".into()))));
        // Its title alone asks for the argument.
        let found = search(&c, "deploy");
        assert_eq!(found[0].sub.as_deref(), Some("Type the branch after “Deploy site ”"));
        assert_eq!(found[0].action, Action::Type("Deploy site ".into()));
        let found = search(&c, "clean");
        assert_eq!((found[0].action.clone(), found[0].sub.as_deref()), (Action::RunScript("/s/Clean downloads.sh".into(), None, Some("42".into())), Some("Runs Clean downloads.sh")));
        // `scripts` lists them and the folder.
        let listed: Vec<String> = search(&c, "scripts").into_iter().filter(|i| i.section == "Scripts").map(|i| i.title).collect();
        assert_eq!(listed, ["Deploy site", "Clean downloads", "Open scripts folder"]);
    }

    #[test]
    fn layouts_are_saved_listed_and_put_back_by_name() {
        let mut c = ctx();
        let found = search(&c, "save layout Writing");
        assert_eq!((found[0].title.as_str(), found[0].action.clone()), ("Save layout “Writing”", Action::SaveLayout("Writing".into())));
        assert!(search(&c, "save layout")[0].is_note());
        c.layouts = vec![layout("Writing", Some("essay")), layout("Code", None)];
        let listed: Vec<String> = search(&c, "layout").into_iter().filter(|i| i.section == "Layouts" && !i.is_note()).map(|i| i.title).collect();
        assert_eq!(listed, ["Writing", "Code"]);
        let found = search(&c, "layout wr");
        assert_eq!(found[0].action, Action::RestoreLayout("Writing".into()));
        assert_eq!(found[0].sub.as_deref(), Some("Puts 1 window back · also when focus “essay” starts"));
        // Saving over a name says it replaces it.
        assert!(search(&c, "save layout code")[0].sub.as_deref().unwrap().starts_with("Replaces"));
        // Found by its name alone.
        assert!(search(&c, "writing").iter().any(|i| i.action == Action::RestoreLayout("Writing".into())));
    }
}
