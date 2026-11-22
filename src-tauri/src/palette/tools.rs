//! The palette's tools for the rest of the desktop: moving the window that was in front (halves,
//! maximize, center, next display, restore) and window layouts. No drawing here, so all of it can
//! be tested.

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
    let Some(front) = ctx.windows.first() else { return Vec::new() };
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
        // Found loosely too.
        assert!(search(&c, "snap").iter().any(|i| i.section == "Window"));
    }

    #[test]
    fn with_no_window_in_front_nothing_offers_to_move_one() {
        let c = ctx();
        assert!(!search(&c, "left half").iter().any(|i| i.section == "Window"));
    }

    fn layout(name: &str, label: Option<&str>) -> Layout {
        Layout { name: name.into(), windows: vec![Placed { app: "code.exe".into(), title: "x".into(), rect: Rect { x: 0, y: 0, w: 1, h: 1 } }], focus_label: label.map(Into::into) }
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
