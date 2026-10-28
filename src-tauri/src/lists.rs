//! Block lists: named sets of apps and sites, like "Social" or "Games", that a scheduled slot or a
//! focus session started by hand blocks instead of everything distracting. Slots and sessions
//! point at a list by its name, which survives the trip to combined computers where ids wouldn't;
//! renaming a list takes them along.

use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};

use crate::db;

/// The list sessions started by hand block, unless their label names another.
const DEFAULT_KEY: &str = "focus_default_list";
const MAX_NAME_CHARS: usize = 40;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BlockList {
    pub name: String,
    /// `app:<exe>` and `site:<name>` keys.
    pub keys: Vec<String>,
}

pub fn list(conn: &Connection) -> Result<Vec<BlockList>> {
    let mut stmt = conn.prepare("SELECT name, keys FROM block_lists ORDER BY position, rowid")?;
    let lists = stmt
        .query_map([], |r| {
            let keys: String = r.get(1)?;
            Ok(BlockList { name: r.get(0)?, keys: serde_json::from_str(&keys).unwrap_or_default() })
        })?
        .collect();
    lists
}

/// The list called `name` (any case), with its name as saved; None when there's no such list.
pub fn find(conn: &Connection, name: &str) -> Result<Option<BlockList>> {
    Ok(list(conn)?.into_iter().find(|l| l.name.eq_ignore_ascii_case(name.trim())))
}

/// What the list called `name` blocks; None when there's no such list or it's empty, so whatever
/// pointed at it blocks as usual.
pub fn keys_of(conn: &Connection, name: &str) -> Result<Option<Vec<String>>> {
    Ok(find(conn, name)?.map(|l| l.keys).filter(|k| !k.is_empty()))
}

/// Adds a list, or with `old`, changes that one: its name, its keys or both. Renaming takes the
/// slots using it, and a running session, along. The inner error is for the user: no name, or one
/// another list already has.
pub fn save(conn: &Connection, old: Option<&str>, name: &str, keys: Vec<String>) -> Result<std::result::Result<(), String>> {
    let name: String = name.trim().chars().take(MAX_NAME_CHARS).collect();
    if name.is_empty() {
        return Ok(Err("Give the list a name.".into()));
    }
    if find(conn, &name)?.is_some_and(|l| old.is_none_or(|o| !l.name.eq_ignore_ascii_case(o))) {
        return Ok(Err(format!("There's already a list called {name}.")));
    }
    let keys = serde_json::to_string(&crate::schedule::tidy_blocks(Some(keys)).unwrap_or_default()).unwrap_or_else(|_| "[]".into());
    match old.map(str::trim).and_then(|o| list(conn).ok()?.into_iter().find(|l| l.name.eq_ignore_ascii_case(o))) {
        Some(was) => {
            conn.execute("UPDATE block_lists SET name = ?1, keys = ?2 WHERE name = ?3", params![name, keys, was.name])?;
            conn.execute("UPDATE focus_schedule SET list = ?1 WHERE list = ?2 COLLATE NOCASE", params![name, was.name])?;
            conn.execute("UPDATE focus_sessions SET list = ?1 WHERE list = ?2 COLLATE NOCASE AND end_ms IS NULL", params![name, was.name])?;
            if default_list(conn)?.is_some_and(|d| d.eq_ignore_ascii_case(&was.name)) {
                db::set_setting(conn, DEFAULT_KEY, &name)?;
            }
        }
        None => {
            conn.execute(
                "INSERT INTO block_lists (name, keys, position) VALUES (?1, ?2, (SELECT IFNULL(MAX(position), 0) + 1 FROM block_lists))",
                params![name, keys],
            )?;
        }
    }
    Ok(Ok(()))
}

/// Takes a list away; slots that used it block as usual again.
pub fn remove(conn: &Connection, name: &str) -> Result<()> {
    conn.execute("DELETE FROM block_lists WHERE name = ?1 COLLATE NOCASE", [name])?;
    conn.execute("UPDATE focus_schedule SET list = NULL WHERE list = ?1 COLLATE NOCASE", [name])?;
    if default_list(conn)?.is_some_and(|d| d.eq_ignore_ascii_case(name)) {
        set_default(conn, None)?;
    }
    Ok(())
}

/// The list that sessions started by hand block; None blocks as usual.
pub fn default_list(conn: &Connection) -> Result<Option<String>> {
    Ok(db::get_setting(conn, DEFAULT_KEY)?.filter(|n| !n.is_empty()))
}

pub fn set_default(conn: &Connection, name: Option<&str>) -> Result<()> {
    db::set_setting(conn, DEFAULT_KEY, name.unwrap_or(""))
}

/// The list a session started by hand with `label` blocks: the one its label names ("focus 50
/// social"), or else the default.
pub fn for_session(conn: &Connection, label: Option<&str>) -> Result<Option<String>> {
    if let Some(named) = label.map(|l| find(conn, l)).transpose()?.flatten() {
        return Ok(Some(named.name));
    }
    let default = default_list(conn)?;
    Ok(match default {
        Some(name) => find(conn, &name)?.map(|l| l.name),
        None => None,
    })
}

/// All lists as text, for sharing with combined computers.
pub fn to_json(conn: &Connection) -> Result<String> {
    Ok(serde_json::to_string(&list(conn)?).unwrap_or_else(|_| "[]".into()))
}

/// Takes another computer's lists in place of this one's.
pub fn replace_from_json(conn: &Connection, json: &str) -> std::result::Result<(), String> {
    let lists: Vec<BlockList> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM block_lists", []).map_err(|e| e.to_string())?;
    for l in lists {
        // A repeated name (it can't happen here, but a hand-edited file could) keeps the first.
        let _ = save(&tx, None, &l.name, l.keys).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

/// The list the running session blocks, by name: its own, or its slot's. None for none.
pub fn of_session(conn: &Connection) -> Result<Option<String>> {
    conn.query_row(
        "SELECT COALESCE(f.list, s.list) FROM focus_sessions f LEFT JOIN focus_schedule s ON s.id = f.slot
         WHERE f.end_ms IS NULL ORDER BY f.id DESC LIMIT 1",
        [],
        |r| r.get::<_, Option<String>>(0),
    )
    .optional()
    .map(Option::flatten)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{focus, schedule};

    fn keys(k: &[&str]) -> Vec<String> {
        k.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn lists_keep_unique_names_and_renames_take_their_slots_along() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        assert_eq!(save(&conn, None, " Social ", keys(&["site:reddit", "site:reddit", "app:discord", "junk"])).unwrap(), Ok(()));
        assert_eq!(list(&conn).unwrap(), [BlockList { name: "Social".into(), keys: keys(&["site:reddit", "app:discord"]) }]);
        assert!(save(&conn, None, "social", Vec::new()).unwrap().is_err());
        assert!(save(&conn, None, "  ", Vec::new()).unwrap().is_err());
        save(&conn, None, "Games", Vec::new()).unwrap().unwrap();
        // An empty list blocks as usual.
        assert_eq!(keys_of(&conn, "games").unwrap(), None);

        let slot = schedule::add(&conn, &schedule::tidy(1, 540, 60, None).unwrap()).unwrap();
        schedule::set_list(&conn, slot, Some("social")).unwrap();
        set_default(&conn, Some("Social")).unwrap();
        save(&conn, Some("social"), "Feeds", keys(&["site:reddit"])).unwrap().unwrap();
        assert_eq!(schedule::list(&conn).unwrap()[0].list.as_deref(), Some("Feeds"));
        assert_eq!(default_list(&conn).unwrap().as_deref(), Some("Feeds"));
        // Renaming onto another list's name is refused.
        assert!(save(&conn, Some("Feeds"), "games", Vec::new()).unwrap().is_err());

        remove(&conn, "feeds").unwrap();
        assert_eq!(schedule::list(&conn).unwrap()[0].list, None);
        assert_eq!(default_list(&conn).unwrap(), None);
    }

    #[test]
    fn a_session_blocks_the_list_its_label_names_or_else_the_default() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        save(&conn, None, "Social", keys(&["site:reddit"])).unwrap().unwrap();
        save(&conn, None, "Games", keys(&["app:steam"])).unwrap().unwrap();
        assert_eq!(for_session(&conn, Some("essay")).unwrap(), None);
        assert_eq!(for_session(&conn, Some("SOCIAL")).unwrap().as_deref(), Some("Social"));
        set_default(&conn, Some("Games")).unwrap();
        assert_eq!(for_session(&conn, Some("essay")).unwrap().as_deref(), Some("Games"));
        assert_eq!(for_session(&conn, None).unwrap().as_deref(), Some("Games"));

        focus::start_by_hand(&conn, 25, Some("social"), 1_000).unwrap();
        assert_eq!(of_session(&conn).unwrap().as_deref(), Some("Social"));
        assert_eq!(schedule::session_blocks(&conn).unwrap(), Some(keys(&["site:reddit"])));
        // Renamed mid-session, the session follows.
        save(&conn, Some("Social"), "Feeds", keys(&["site:reddit", "site:x"])).unwrap().unwrap();
        assert_eq!(schedule::session_blocks(&conn).unwrap(), Some(keys(&["site:reddit", "site:x"])));

        // A slot's saved list wins over its own one.
        let slot = schedule::add(&conn, &schedule::tidy(1, 540, 60, None).unwrap()).unwrap();
        schedule::set_blocks(&conn, slot, Some(keys(&["site:youtube"]))).unwrap();
        focus::start_from_slot(&conn, 30, None, slot, 2_000).unwrap();
        assert_eq!(schedule::session_blocks(&conn).unwrap(), Some(keys(&["site:youtube"])));
        schedule::set_list(&conn, slot, Some("games")).unwrap();
        assert_eq!(schedule::session_blocks(&conn).unwrap(), Some(keys(&["app:steam"])));
    }

    #[test]
    fn lists_move_between_computers_as_text() {
        let (here, there) = (db::open(std::path::Path::new(":memory:")).unwrap(), db::open(std::path::Path::new(":memory:")).unwrap());
        save(&here, None, "Social", keys(&["site:reddit"])).unwrap().unwrap();
        save(&there, None, "Old", Vec::new()).unwrap().unwrap();
        replace_from_json(&there, &to_json(&here).unwrap()).unwrap();
        assert_eq!(list(&there).unwrap(), list(&here).unwrap());
    }
}
