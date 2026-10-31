//! Tasks on Chrono's Focus page: a short to-do list whose tasks gather the focus time of the
//! sessions started on them. Each has a random id, so the list can travel to combined computers
//! whole and still match the sessions there that point at it.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};

use crate::focus::MAX_LABEL_CHARS;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    pub created_ms: i64,
    /// When it was ticked off; None while open.
    pub done_ms: Option<i64>,
    pub position: i64,
    /// The focus time of its sessions on this computer; sent as 0 to combined computers.
    #[serde(default)]
    pub focused_ms: i64,
}

/// A title as kept: trimmed and short; None when nothing is left.
fn clean(title: &str) -> Option<String> {
    let title: String = title.trim().chars().take(MAX_LABEL_CHARS).collect();
    let title = title.trim_end().to_string();
    (!title.is_empty()).then_some(title)
}

/// The position after the last open task.
fn next_position(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COALESCE(MAX(position), -1) + 1 FROM tasks WHERE done_ms IS NULL", [], |r| r.get(0))
}

/// Adds a task at the bottom of the open ones; None for an empty title.
pub fn add(conn: &Connection, title: &str, now: i64) -> Result<Option<Task>> {
    let Some(title) = clean(title) else { return Ok(None) };
    let task = Task { id: crate::sync::new_id(), title, created_ms: now, done_ms: None, position: next_position(conn)?, focused_ms: 0 };
    conn.execute(
        "INSERT INTO tasks (id, title, created_ms, done_ms, position) VALUES (?1, ?2, ?3, NULL, ?4)",
        params![task.id, task.title, task.created_ms, task.position],
    )?;
    Ok(Some(task))
}

/// Gives a task a new title; an empty one changes nothing.
pub fn rename(conn: &Connection, id: &str, title: &str) -> Result<()> {
    if let Some(title) = clean(title) {
        conn.execute("UPDATE tasks SET title = ?2 WHERE id = ?1", params![id, title])?;
    }
    Ok(())
}

/// Ticks a task off, or reopens it at the bottom of the open ones.
pub fn set_done(conn: &Connection, id: &str, done: bool, now: i64) -> Result<()> {
    if done {
        conn.execute("UPDATE tasks SET done_ms = ?2 WHERE id = ?1 AND done_ms IS NULL", params![id, now])?;
    } else {
        conn.execute("UPDATE tasks SET done_ms = NULL, position = ?2 WHERE id = ?1 AND done_ms IS NOT NULL", params![id, next_position(conn)?])?;
    }
    Ok(())
}

/// Takes a task away. Its sessions stay as they are, counting for nothing.
pub fn remove(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
    Ok(())
}

pub fn title(conn: &Connection, id: &str) -> Result<Option<String>> {
    conn.query_row("SELECT title FROM tasks WHERE id = ?1", [id], |r| r.get(0)).optional()
}

/// Focus time by task, as the Focus page counts it: start to end (now for a running session)
/// less time paused.
fn focused_by_task(conn: &Connection, now: i64) -> Result<HashMap<String, i64>> {
    let mut stmt = conn.prepare(
        "SELECT task, SUM(MAX(0, COALESCE(end_ms, ?1) - start_ms - (paused_ms + COALESCE(?1 - paused_at, 0))))
         FROM focus_sessions WHERE task IS NOT NULL GROUP BY task",
    )?;
    let rows = stmt.query_map([now], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

/// Open tasks in order, then done ones newest first, each with its focus time.
pub fn list(conn: &Connection, now: i64) -> Result<Vec<Task>> {
    let focused = focused_by_task(conn, now)?;
    let mut stmt = conn.prepare(
        "SELECT id, title, created_ms, done_ms, position FROM tasks
         ORDER BY done_ms IS NOT NULL, CASE WHEN done_ms IS NULL THEN position ELSE -done_ms END, created_ms",
    )?;
    let rows = stmt.query_map([], |r| {
        let id: String = r.get(0)?;
        Ok(Task { focused_ms: focused.get(&id).copied().unwrap_or(0), id, title: r.get(1)?, created_ms: r.get(2)?, done_ms: r.get(3)?, position: r.get(4)? })
    })?;
    rows.collect()
}

/// The whole list as text, for sharing with combined computers.
pub fn to_json(conn: &Connection) -> Result<String> {
    let tasks: Vec<Task> = list(conn, 0)?.into_iter().map(|t| Task { focused_ms: 0, ..t }).collect();
    Ok(serde_json::to_string(&tasks).unwrap_or_else(|_| "[]".into()))
}

/// Takes another computer's list in place of this one's.
pub fn replace_from_json(conn: &Connection, json: &str) -> std::result::Result<(), String> {
    let tasks: Vec<Task> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM tasks", []).map_err(|e| e.to_string())?;
    for t in tasks {
        let Some(title) = clean(&t.title) else { continue };
        tx.execute(
            "INSERT OR IGNORE INTO tasks (id, title, created_ms, done_ms, position) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![t.id, title, t.created_ms, t.done_ms, t.position],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: i64 = 60_000;

    fn conn() -> Connection {
        crate::db::open(std::path::Path::new(":memory:")).unwrap()
    }

    #[test]
    fn adding_trims_caps_and_refuses_empty() {
        let c = conn();
        assert_eq!(add(&c, "   ", 1).unwrap(), None);
        let t = add(&c, "  Essay  ", 1).unwrap().unwrap();
        assert_eq!(t.title, "Essay");
        assert_eq!(t.id.len(), 16);
        assert_eq!(add(&c, &"x".repeat(200), 2).unwrap().unwrap().title.chars().count(), 80);
    }

    #[test]
    fn renaming_to_nothing_keeps_the_title() {
        let c = conn();
        let t = add(&c, "Essay", 1).unwrap().unwrap();
        rename(&c, &t.id, "  ").unwrap();
        assert_eq!(title(&c, &t.id).unwrap().as_deref(), Some("Essay"));
        rename(&c, &t.id, "Essay draft").unwrap();
        assert_eq!(title(&c, &t.id).unwrap().as_deref(), Some("Essay draft"));
    }

    #[test]
    fn done_tasks_follow_open_ones_newest_first_and_reopen_at_the_bottom() {
        let c = conn();
        let [a, b, d] = ["A", "B", "D"].map(|n| add(&c, n, 1).unwrap().unwrap());
        set_done(&c, &a.id, true, 10).unwrap();
        set_done(&c, &b.id, true, 20).unwrap();
        let titles = |c: &Connection| list(c, 30).unwrap().into_iter().map(|t| t.title).collect::<Vec<_>>();
        assert_eq!(titles(&c), ["D", "B", "A"]);
        set_done(&c, &a.id, false, 30).unwrap();
        assert_eq!(titles(&c), ["D", "A", "B"]);
        remove(&c, &d.id).unwrap();
        assert_eq!(titles(&c), ["A", "B"]);
    }

    #[test]
    fn a_task_gathers_its_sessions_focused_time_without_pauses() {
        let c = conn();
        let t = add(&c, "Essay", 0).unwrap().unwrap();
        // A finished 30-minute session with 5 minutes paused, and a running one 10 minutes in.
        c.execute("INSERT INTO focus_sessions (start_ms, planned_ms, end_ms, paused_ms, task) VALUES (0, ?1, ?1, ?2, ?3)", params![30 * M, 5 * M, t.id]).unwrap();
        c.execute("INSERT INTO focus_sessions (start_ms, planned_ms, task) VALUES (?1, ?2, ?3)", params![60 * M, 25 * M, t.id]).unwrap();
        assert_eq!(list(&c, 70 * M).unwrap()[0].focused_ms, 35 * M);
        remove(&c, &t.id).unwrap();
        assert!(list(&c, 70 * M).unwrap().is_empty());
    }

    #[test]
    fn the_shared_list_replaces_this_one_and_keeps_ids() {
        let (here, there) = (conn(), conn());
        let t = add(&there, "Essay", 1).unwrap().unwrap();
        set_done(&there, &t.id, true, 5).unwrap();
        add(&here, "Old", 1).unwrap();
        replace_from_json(&here, &to_json(&there).unwrap()).unwrap();
        let got = list(&here, 10).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].id.as_str(), got[0].title.as_str(), got[0].done_ms), (t.id.as_str(), "Essay", Some(5)));
        assert!(replace_from_json(&here, "not json").is_err());
    }
}
