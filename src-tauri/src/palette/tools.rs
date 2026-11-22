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
                item.action = Action::Nothing;
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

    #[test]
    fn snaps_move_the_window_that_was_in_front() {
        let c = with_window();
        let found = search(&c, "left half");
        assert_eq!(found[0].action, Action::Snap(Snap::LeftHalf, "42".into()));
        assert_eq!(found[0].sub.as_deref(), Some("Moves “notes.md — Visual Studio Code”"));
        assert_eq!(search(&c, "maximize")[0].action, Action::Snap(Snap::Maximize, "42".into()));
        assert_eq!(search(&c, "next display")[0].action, Action::Snap(Snap::NextDisplay, "42".into()));
        assert_eq!(search(&c, "restore")[0].action, Action::Snap(Snap::Restore, "42".into()));
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
        assert_eq!(found[0].action, Action::Nothing);
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
