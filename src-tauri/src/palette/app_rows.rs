//! What can be done to apps from the palette besides opening them: marking favorites, hiding ones
//! you never want to see, and quitting the open ones. Favorites and hidden apps are kept by name,
//! in the settings on this computer. No drawing here, so all of it can be tested.

use rusqlite::{Connection, Result};

use super::commands::{row, LEAD};
use super::search::{after, fuzzy, note_row, Action, Context, Glyph, Icon, Item};
use crate::db;

/// Which list an app's name is kept in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum List {
    Favorites,
    Hidden,
}

impl List {
    fn key(self) -> &'static str {
        match self {
            List::Favorites => "palette_favorite_apps",
            List::Hidden => "palette_hidden_apps",
        }
    }
}

/// Rows `hide` and `quit` offer at most.
const MAX_ROWS: usize = 6;

/// The names in a list, in the order they were added.
pub fn names(conn: &Connection, list: List) -> Result<Vec<String>> {
    Ok(db::get_setting(conn, list.key())?.and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default())
}

/// Whether `list` has this app, whatever its case.
pub fn has(list: &[String], name: &str) -> bool {
    list.iter().any(|n| n.to_lowercase() == name.to_lowercase())
}

/// Puts an app in a list or takes it out. An app is a favorite or hidden, never both, so putting
/// it in one takes it out of the other.
pub fn set(conn: &Connection, list: List, name: &str, on: bool) -> Result<()> {
    let write = |which: List, add: bool| -> Result<()> {
        let mut kept = names(conn, which)?;
        kept.retain(|n| n.to_lowercase() != name.to_lowercase());
        if add {
            kept.push(name.to_string());
        }
        db::set_setting(conn, which.key(), &serde_json::to_string(&kept).unwrap_or_else(|_| "[]".into()))
    };
    write(list, on)?;
    if on {
        write(if list == List::Favorites { List::Hidden } else { List::Favorites }, false)?;
    }
    Ok(())
}

/// What Ctrl+Enter does on an app's row.
pub fn favorite_action(ctx: &Context, name: &str) -> (Action, &'static str) {
    if has(&ctx.live.favorites, name) {
        (Action::FavoriteApp(name.to_string(), false), "Unfavorite")
    } else {
        (Action::FavoriteApp(name.to_string(), true), "Favorite")
    }
}

pub fn rows(ctx: &Context, query: &str) -> Vec<Item> {
    let mut items = hide_rows(ctx, query);
    items.extend(quit_rows(ctx, query));
    items
}

fn hidden_row(name: &str, icon: Option<&str>, hidden: bool, (score, at): (f64, Vec<usize>)) -> Item {
    let (title, sub) = if hidden { (format!("Show {name} again"), "Hidden from the palette") } else { (format!("Hide {name}"), "Leaves it out of the palette; “hidden apps” brings it back") };
    // The highlight is on the name, which comes after the verb and a space.
    let lead = if hidden { "Show " } else { "Hide " }.chars().count();
    let at = at.into_iter().map(|c| c + lead).collect();
    let mut item = row(title, Glyph::Search, "Apps to hide", Action::HideApp(name.to_string(), !hidden), sub, (score, at));
    if let Some(path) = icon {
        item.icon = Icon::App(path.to_string());
    }
    item
}

/// `hide <app>` offers to hide an app, or to show it again once hidden; `hidden apps` lists the
/// hidden ones, and `unhide <app>` finds one among them.
fn hide_rows(ctx: &Context, query: &str) -> Vec<Item> {
    let q = query.trim();
    let listing = ["hidden", "hidden apps", "unhide"].iter().any(|w| q.eq_ignore_ascii_case(w));
    let unhiding = after(q, "unhide ").map(str::trim);
    if listing || unhiding.is_some() {
        let wanted = unhiding.unwrap_or_default();
        let found: Vec<Item> = ctx
            .live
            .hidden
            .iter()
            .filter_map(|name| fuzzy(wanted, name).map(|m| (name, m.at)))
            .enumerate()
            .map(|(i, (name, at))| {
                let icon = ctx.apps.iter().find(|a| a.name.eq_ignore_ascii_case(name)).map(|a| a.path.as_str());
                hidden_row(name, icon, true, (LEAD - i as f64 / 100.0, at))
            })
            .collect();
        if found.is_empty() {
            let mut item = note_row(if ctx.live.hidden.is_empty() { "No apps are hidden. Type “hide” and an app's name" } else { "No hidden app matches" }, Glyph::Search, "Apps to hide");
            item.score = LEAD;
            return vec![item];
        }
        return found;
    }
    let Some(wanted) = after(q, "hide ").map(str::trim).filter(|w| !w.is_empty()) else { return Vec::new() };
    let mut found: Vec<_> = ctx.apps.iter().filter_map(|a| fuzzy(wanted, &a.name).map(|m| (m.score, m.at, a))).collect();
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found.into_iter().take(MAX_ROWS).enumerate().map(|(i, (_, at, a))| hidden_row(&a.name, Some(&a.path), has(&ctx.live.hidden, &a.name), (LEAD - i as f64 / 100.0, at))).collect()
}

/// `quit <app>` asks every window of an open app to close, as its close button would, so unsaved
/// work can still be saved. `quit all` is a system command (system.rs), which asks twice.
fn quit_rows(ctx: &Context, query: &str) -> Vec<Item> {
    let q = query.trim();
    let note = |text: String| {
        let mut item = note_row(text, Glyph::Window, "Quit");
        item.score = LEAD;
        vec![item]
    };
    if q.eq_ignore_ascii_case("quit") {
        return note("Type an app's name, like quit chrome, or quit all".into());
    }
    let Some(wanted) = after(q, "quit ").map(str::trim).filter(|w| !w.is_empty()) else { return Vec::new() };
    if crate::system::Power::QuitAll.words().0.iter().any(|w| q.eq_ignore_ascii_case(w)) {
        return Vec::new();
    }
    // The open apps, front to back, each with its windows.
    let mut apps: Vec<(&str, Option<&str>, Vec<String>)> = Vec::new();
    for w in ctx.windows.iter().filter(|w| !w.app.is_empty() && crate::platform::can_quit(&w.app)) {
        match apps.iter_mut().find(|(name, ..)| *name == w.app) {
            Some((.., ids)) => ids.push(w.id.clone()),
            None => apps.push((&w.app, w.icon.as_deref(), vec![w.id.clone()])),
        }
    }
    let mut found: Vec<_> = apps.into_iter().filter_map(|app| fuzzy(wanted, app.0).map(|m| (m.score, m.at, app))).collect();
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    if found.is_empty() {
        return note(format!("No open app matches “{wanted}”"));
    }
    found
        .into_iter()
        .take(MAX_ROWS)
        .enumerate()
        .map(|(i, (_, at, (name, icon, ids)))| {
            let sub = match ids.len() {
                1 => "Asks it to close, so you can save first".to_string(),
                n => format!("Asks its {n} windows to close, so you can save first"),
            };
            let at = at.into_iter().map(|c| c + "Quit ".chars().count()).collect();
            let mut item = row(format!("Quit {name}"), Glyph::Window, "Quit", Action::QuitApp(ids), sub, (LEAD - i as f64 / 100.0, at));
            item.icon = Icon::App(icon.unwrap_or_default().to_string());
            item
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::search::tests::ctx;
    use super::super::search::{search, suggestions, WindowEntry};
    use super::*;

    fn window(id: &str, app: &str) -> WindowEntry {
        WindowEntry { id: id.into(), title: format!("{app} window {id}"), app: app.into(), icon: None }
    }

    #[test]
    fn lists_are_kept_by_name_and_an_app_is_only_in_one() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        assert!(names(&conn, List::Favorites).unwrap().is_empty());
        set(&conn, List::Favorites, "Cursor", true).unwrap();
        set(&conn, List::Favorites, "Brave", true).unwrap();
        set(&conn, List::Favorites, "cursor", true).unwrap();
        assert_eq!(names(&conn, List::Favorites).unwrap(), ["Brave", "cursor"]);
        set(&conn, List::Hidden, "BRAVE", true).unwrap();
        assert_eq!(names(&conn, List::Favorites).unwrap(), ["cursor"]);
        assert_eq!(names(&conn, List::Hidden).unwrap(), ["BRAVE"]);
        set(&conn, List::Hidden, "brave", false).unwrap();
        assert!(names(&conn, List::Hidden).unwrap().is_empty());
    }

    #[test]
    fn ctrl_enter_on_an_app_marks_it_and_a_favorite_ranks_first() {
        let mut c = ctx();
        let first = &search(&c, "cur")[0];
        assert_eq!((first.title.as_str(), first.second.clone()), ("Cursor", Some((Action::FavoriteApp("Cursor".into(), true), "Favorite"))));
        c.live.favorites = vec!["curseforge".into()];
        let found = search(&c, "cur");
        assert_eq!((found[0].title.as_str(), found[0].second.clone()), ("CurseForge", Some((Action::FavoriteApp("CurseForge".into(), false), "Unfavorite"))));
        assert!(Action::FavoriteApp("x".into(), true).acts_in_place());
    }

    #[test]
    fn the_empty_palette_lists_favorites() {
        let mut c = ctx();
        c.live.favorites = vec!["Discord".into(), "Gone".into(), "notepad".into()];
        let shown: Vec<(String, Action)> = suggestions(&c).rows.into_iter().filter(|r| r.section == "Favorites").map(|r| (r.title, r.action)).collect();
        assert_eq!(shown.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["Discord", "Notepad"]);
        assert!(matches!(shown[0].1, Action::Launch(_)));
    }

    #[test]
    fn a_hidden_app_is_left_out_everywhere() {
        let mut c = ctx();
        c.live.hidden = vec!["cursor".into()];
        assert!(!search(&c, "cur").iter().any(|i| i.title == "Cursor" && matches!(i.action, Action::Launch(_))));
        assert!(!suggestions(&c).tiles.iter().any(|t| t.title == "Cursor"));
    }

    #[test]
    fn hide_offers_to_hide_and_then_to_show_again() {
        let mut c = ctx();
        let row = &search(&c, "hide disc")[0];
        assert_eq!((row.title.as_str(), row.action.clone()), ("Hide Discord", Action::HideApp("Discord".into(), true)));
        assert!(row.action.acts_in_place());
        c.live.hidden = vec!["Discord".into()];
        let row = &search(&c, "hide disc")[0];
        assert_eq!((row.title.as_str(), row.action.clone()), ("Show Discord again", Action::HideApp("Discord".into(), false)));
        for q in ["hidden apps", "hidden", "unhide", "unhide disc"] {
            assert_eq!(search(&c, q)[0].action, Action::HideApp("Discord".into(), false), "{q}");
        }
        assert!(search(&ctx(), "hidden apps")[0].is_note());
    }

    #[test]
    fn quit_asks_all_of_an_apps_windows_to_close() {
        let mut c = ctx();
        c.windows = vec![window("1", "Brave"), window("2", "Cursor"), window("3", "Brave")];
        let row = &search(&c, "quit bra")[0];
        assert_eq!((row.title.as_str(), row.action.clone()), ("Quit Brave", Action::QuitApp(vec!["1".into(), "3".into()])));
        assert_eq!(row.sub.as_deref(), Some("Asks its 2 windows to close, so you can save first"));
        assert!(!row.action.keeps_palette_open());
        assert!(search(&c, "quit figma")[0].is_note());
        assert!(search(&c, "quit")[0].is_note());
        // Only when asked: an app's name alone never offers to quit it.
        assert!(!search(&c, "brave").iter().any(|i| matches!(i.action, Action::QuitApp(..))));
    }

    #[test]
    fn quit_all_asks_twice() {
        use crate::system::Power;
        let mut c = ctx();
        c.windows = vec![window("1", "Brave")];
        let first = &search(&c, "quit all")[0];
        assert_eq!(first.action, Action::ConfirmPower(Power::QuitAll, "quit all".into()));
        c.live.pending_power = Some((Power::QuitAll, "quit all".into()));
        assert_eq!(search(&c, "quit all")[0].action, Action::PowerNow(Power::QuitAll));
    }

    #[test]
    fn a_typed_address_opens_it_unless_an_app_has_that_name() {
        let mut c = ctx();
        let first = &search(&c, "github.com")[0];
        assert_eq!((first.title.as_str(), first.action.clone()), ("Open github.com", Action::OpenUrl("https://github.com".into())));
        c.apps.push(crate::palette::AppEntry { name: "draw.io".into(), path: "draw".into(), used_ms: 0 });
        assert_eq!(search(&c, "draw.io")[0].action, Action::Launch("draw".into()));
    }
}
