//! Focus modes: a name for a way of working ("Study", "Write"), run with one palette command. A
//! mode says how long to focus, which block list applies and which apps to open; running it starts
//! the session, applies the list and opens the apps. They stay on the computer they were made on:
//! the apps are paths that mean something only there.

use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};

use crate::{focus, lists};

const MAX_NAME_CHARS: usize = 30;
/// Apps one mode opens at most.
pub const MAX_APPS: usize = 12;

/// An app a mode opens: its name, and the path the palette opens it by.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ModeApp {
    pub name: String,
    pub path: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FocusMode {
    pub name: String,
    pub minutes: u32,
    /// The block list that applies, by name; None for whatever starting a session by hand would
    /// block (the default list, or as usual).
    pub list: Option<String>,
    pub apps: Vec<ModeApp>,
}

impl FocusMode {
    /// What running it does, in a line: "50 min · blocks Social · opens Word and Chrome".
    pub fn says(&self) -> String {
        let mut parts = vec![format!("{} min", self.minutes)];
        if let Some(list) = &self.list {
            parts.push(format!("blocks {list}"));
        }
        let names: Vec<&str> = self.apps.iter().map(|a| a.name.as_str()).collect();
        match names.as_slice() {
            [] => {}
            [one] => parts.push(format!("opens {one}")),
            [first, second] => parts.push(format!("opens {first} and {second}")),
            [first, second, rest @ ..] => parts.push(format!("opens {first}, {second} and {} more", rest.len())),
        }
        parts.join(" · ")
    }
}

pub fn list(conn: &Connection) -> Result<Vec<FocusMode>> {
    let mut stmt = conn.prepare("SELECT name, minutes, list, apps FROM focus_modes ORDER BY position, rowid")?;
    let modes = stmt
        .query_map([], |r| {
            let apps: String = r.get(3)?;
            Ok(FocusMode { name: r.get(0)?, minutes: r.get::<_, i64>(1)? as u32, list: r.get(2)?, apps: serde_json::from_str(&apps).unwrap_or_default() })
        })?
        .collect();
    modes
}

/// The mode called `name`, whatever the case.
pub fn find(conn: &Connection, name: &str) -> Result<Option<FocusMode>> {
    Ok(list(conn)?.into_iter().find(|m| m.name.eq_ignore_ascii_case(name.trim())))
}

/// Adds a mode, or with `old`, changes the one of that name. The inner error is for the user: no
/// name, one already taken, or a length a session can't have.
pub fn save(conn: &Connection, old: Option<&str>, name: &str, minutes: u32, list_name: Option<&str>, apps: Vec<ModeApp>) -> Result<std::result::Result<(), String>> {
    let name: String = name.trim().chars().take(MAX_NAME_CHARS).collect();
    let name = name.trim_end().to_string();
    if name.is_empty() {
        return Ok(Err("Give the mode a name.".into()));
    }
    if !(1..=focus::MAX_MINUTES).contains(&minutes) {
        return Ok(Err(format!("A session runs from 1 to {} minutes.", focus::MAX_MINUTES)));
    }
    if find(conn, &name)?.is_some_and(|m| old.is_none_or(|o| !m.name.eq_ignore_ascii_case(o.trim()))) {
        return Ok(Err(format!("There's already a mode called {name}.")));
    }
    // A list that's gone is none; the name as the list has it.
    let list_name = match list_name.map(str::trim).filter(|l| !l.is_empty()) {
        Some(l) => lists::find(conn, l)?.map(|l| l.name),
        None => None,
    };
    let mut seen = std::collections::HashSet::new();
    let apps: Vec<ModeApp> = apps.into_iter().filter(|a| !a.path.is_empty() && seen.insert(a.path.clone())).take(MAX_APPS).collect();
    let apps = serde_json::to_string(&apps).unwrap_or_else(|_| "[]".into());
    match old.map(str::trim).and_then(|o| list(conn).ok()?.into_iter().find(|m| m.name.eq_ignore_ascii_case(o))) {
        Some(was) => {
            conn.execute("UPDATE focus_modes SET name = ?1, minutes = ?2, list = ?3, apps = ?4 WHERE name = ?5", params![name, minutes, list_name, apps, was.name])?;
        }
        None => {
            conn.execute(
                "INSERT INTO focus_modes (name, minutes, list, apps, position) VALUES (?1, ?2, ?3, ?4, (SELECT IFNULL(MAX(position), 0) + 1 FROM focus_modes))",
                params![name, minutes, list_name, apps],
            )?;
        }
    }
    Ok(Ok(()))
}

pub fn remove(conn: &Connection, name: &str) -> Result<()> {
    conn.execute("DELETE FROM focus_modes WHERE name = ?1 COLLATE NOCASE", [name])?;
    Ok(())
}

/// A block list was renamed: the modes using it follow.
pub fn rename_list(conn: &Connection, from: &str, to: &str) -> Result<()> {
    conn.execute("UPDATE focus_modes SET list = ?2 WHERE list = ?1 COLLATE NOCASE", params![from, to])?;
    Ok(())
}

/// A block list was removed: the modes using it block as a session by hand would again.
pub fn forget_list(conn: &Connection, name: &str) -> Result<()> {
    conn.execute("UPDATE focus_modes SET list = NULL WHERE list = ?1 COLLATE NOCASE", [name])?;
    Ok(())
}

/// Starts the session a mode asks for, blocking its list, and returns the apps it opens (for the
/// caller to open, since it holds the palette's index of apps). Ends the session that runs, as
/// starting one by hand does. With a task (by id) the time counts for it and its title is the
/// session's label.
pub fn start(conn: &Connection, mode: &FocusMode, task: Option<&str>, now: i64) -> Result<Vec<ModeApp>> {
    focus::start_by_hand(conn, mode.minutes, Some(&mode.name), task, now)?;
    // The mode's own list wins over what its name or the default would pick.
    if let Some(list) = &mode.list {
        conn.execute("UPDATE focus_sessions SET list = ?1 WHERE end_ms IS NULL", [list])?;
    }
    Ok(mode.apps.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn conn() -> Connection {
        db::open(std::path::Path::new(":memory:")).unwrap()
    }

    fn app(name: &str) -> ModeApp {
        ModeApp { name: name.into(), path: format!("path/{name}") }
    }

    #[test]
    fn a_mode_is_named_timed_and_has_apps_without_repeats() {
        let c = conn();
        assert_eq!(save(&c, None, "  Study ", 50, None, vec![app("Word"), app("Word"), app("Chrome")]).unwrap(), Ok(()));
        let got = list(&c).unwrap();
        assert_eq!((got[0].name.as_str(), got[0].minutes, got[0].list.clone()), ("Study", 50, None));
        assert_eq!(got[0].apps, [app("Word"), app("Chrome")]);
        assert_eq!(find(&c, "study").unwrap().unwrap().name, "Study");
    }

    #[test]
    fn a_mode_needs_a_name_a_free_one_and_a_length_a_session_can_have() {
        let c = conn();
        assert!(save(&c, None, "  ", 25, None, vec![]).unwrap().is_err());
        assert!(save(&c, None, "Study", 0, None, vec![]).unwrap().is_err());
        assert!(save(&c, None, "Study", focus::MAX_MINUTES + 1, None, vec![]).unwrap().is_err());
        save(&c, None, "Study", 25, None, vec![]).unwrap().unwrap();
        assert_eq!(save(&c, None, "STUDY", 25, None, vec![]).unwrap(), Err("There's already a mode called STUDY.".into()));
        // Changing it keeps its own name free.
        assert_eq!(save(&c, Some("Study"), "Study", 90, None, vec![]).unwrap(), Ok(()));
        assert_eq!(list(&c).unwrap()[0].minutes, 90);
        save(&c, None, "Write", 25, None, vec![]).unwrap().unwrap();
        assert!(save(&c, Some("Write"), "study", 25, None, vec![]).unwrap().is_err());
    }

    #[test]
    fn its_block_list_is_kept_by_name_and_follows_renames_and_removal() {
        let c = conn();
        lists::save(&c, None, "Social", vec!["site:reddit".into()]).unwrap().unwrap();
        save(&c, None, "Study", 50, Some("social"), vec![]).unwrap().unwrap();
        assert_eq!(list(&c).unwrap()[0].list.as_deref(), Some("Social"));
        // A list that isn't there is none.
        save(&c, Some("Study"), "Study", 50, Some("Games"), vec![]).unwrap().unwrap();
        assert_eq!(list(&c).unwrap()[0].list, None);
        save(&c, Some("Study"), "Study", 50, Some("Social"), vec![]).unwrap().unwrap();
        rename_list(&c, "Social", "Distractions").unwrap();
        assert_eq!(list(&c).unwrap()[0].list.as_deref(), Some("Distractions"));
        forget_list(&c, "distractions").unwrap();
        assert_eq!(list(&c).unwrap()[0].list, None);
    }

    #[test]
    fn running_one_starts_its_session_blocking_its_list_and_hands_back_its_apps() {
        let c = conn();
        lists::save(&c, None, "Social", vec!["site:reddit".into()]).unwrap().unwrap();
        lists::save(&c, None, "Games", vec!["app:steam".into()]).unwrap().unwrap();
        lists::set_default(&c, Some("Games")).unwrap();
        save(&c, None, "Study", 50, Some("Social"), vec![app("Word")]).unwrap().unwrap();
        let mode = find(&c, "study").unwrap().unwrap();
        let apps = start(&c, &mode, None, 1_000).unwrap();
        assert_eq!(apps, [app("Word")]);
        let running = focus::active(&c).unwrap().unwrap();
        assert_eq!(running.label(), Some("Study"));
        // Its own list, not the default one.
        assert_eq!(lists::of_session(&c).unwrap().as_deref(), Some("Social"));
        // With none of its own, a session by hand's usual choice applies.
        save(&c, None, "Write", 25, None, vec![]).unwrap().unwrap();
        start(&c, &find(&c, "write").unwrap().unwrap(), None, 2_000).unwrap();
        assert_eq!(lists::of_session(&c).unwrap().as_deref(), Some("Games"));
    }

    #[test]
    fn run_on_a_task_the_time_counts_for_it_and_its_title_is_the_label() {
        let c = conn();
        lists::save(&c, None, "Social", vec!["site:reddit".into()]).unwrap().unwrap();
        save(&c, None, "Study", 50, Some("Social"), vec![]).unwrap().unwrap();
        let task = crate::tasks::add(&c, "Essay", 1).unwrap().unwrap();
        start(&c, &find(&c, "study").unwrap().unwrap(), Some(&task.id), 5_000).unwrap();
        let running_task = |c: &Connection| c.query_row("SELECT task FROM focus_sessions WHERE end_ms IS NULL", [], |r| r.get::<_, Option<String>>(0)).unwrap();
        let running = focus::active(&c).unwrap().unwrap();
        assert_eq!((running.label(), running_task(&c)), (Some("Essay"), Some(task.id.clone())));
        // Its list still applies.
        assert_eq!(lists::of_session(&c).unwrap().as_deref(), Some("Social"));
        // A task that isn't there is none: the session still starts, labelled by the mode.
        start(&c, &find(&c, "study").unwrap().unwrap(), Some("gone"), 6_000).unwrap();
        let again = focus::active(&c).unwrap().unwrap();
        assert_eq!((again.label(), running_task(&c)), (Some("Study"), None));
    }

    #[test]
    fn it_says_what_running_it_does() {
        let mode = |list: Option<&str>, apps: &[&str]| FocusMode { name: "Study".into(), minutes: 50, list: list.map(Into::into), apps: apps.iter().map(|a| app(a)).collect() };
        assert_eq!(mode(None, &[]).says(), "50 min");
        assert_eq!(mode(Some("Social"), &["Word"]).says(), "50 min · blocks Social · opens Word");
        assert_eq!(mode(None, &["Word", "Chrome"]).says(), "50 min · opens Word and Chrome");
        assert_eq!(mode(None, &["Word", "Chrome", "Notion", "Figma"]).says(), "50 min · opens Word, Chrome and 2 more");
    }
}
