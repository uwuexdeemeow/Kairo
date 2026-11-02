//! Tasks, kept in Ledger: a to-do list whose tasks gather the focus time of the
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
    /// The local day it's due ("2026-11-07"); None for none.
    #[serde(default)]
    pub due_day: Option<String>,
    /// 0 Low, 1 Medium, 2 High.
    #[serde(default = "medium")]
    pub priority: u8,
    /// An exact reminder time; None for none.
    #[serde(default)]
    pub remind_ms: Option<i64>,
    /// The reminders already sent for it, space-separated: `due:<day>`, `over:<day>`, `at:<ms>`.
    #[serde(default)]
    pub reminded: String,
    /// Where Ledger lists it: "overdue", "today", "upcoming", "none" or "done".
    #[serde(default, skip_deserializing)]
    pub group: String,
}

fn medium() -> u8 {
    MEDIUM
}

impl Default for Task {
    fn default() -> Self {
        Task {
            id: String::new(),
            title: String::new(),
            created_ms: 0,
            done_ms: None,
            position: 0,
            focused_ms: 0,
            due_day: None,
            priority: MEDIUM,
            remind_ms: None,
            reminded: String::new(),
            group: "none".into(),
        }
    }
}

pub const LOW: u8 = 0;
pub const MEDIUM: u8 = 1;
pub const HIGH: u8 = 2;

/// A task's plan, which travels to combined computers as its own shared choice
/// (`taskplan:<id>`), apart from the list, so a Kairo from before plans can't wipe it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Plan {
    pub due: Option<String>,
    pub priority: u8,
    pub remind: Option<i64>,
    #[serde(default)]
    pub sent: String,
}

/// The list as combined computers share it: what Kairo 1.26 knows of a task, and no more.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Shared {
    id: String,
    title: String,
    created_ms: i64,
    done_ms: Option<i64>,
    position: i64,
    #[serde(default)]
    focused_ms: i64,
}

/// Today's local day, as tasks keep due days.
pub fn today() -> String {
    chrono::Local::now().date_naive().format("%Y-%m-%d").to_string()
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
    let task = Task {
        id: crate::sync::new_id(),
        title,
        created_ms: now,
        done_ms: None,
        position: next_position(conn)?,
        focused_ms: 0,
        due_day: None,
        priority: MEDIUM,
        remind_ms: None,
        reminded: String::new(),
        group: "none".into(),
    };
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

/// Ledger's order: overdue, today, upcoming (by day), undated, each High to Low and then in the
/// order added; then done ones, newest first. Each with its focus time and group.
pub fn list(conn: &Connection, now: i64, today: &str) -> Result<Vec<Task>> {
    let focused = focused_by_task(conn, now)?;
    let mut stmt = conn.prepare("SELECT id, title, created_ms, done_ms, position, due_day, priority, remind_ms, reminded FROM tasks")?;
    let rows = stmt.query_map([], |r| {
        let id: String = r.get(0)?;
        Ok(Task {
            focused_ms: focused.get(&id).copied().unwrap_or(0),
            id,
            title: r.get(1)?,
            created_ms: r.get(2)?,
            done_ms: r.get(3)?,
            position: r.get(4)?,
            due_day: r.get(5)?,
            priority: r.get(6)?,
            remind_ms: r.get(7)?,
            reminded: r.get(8)?,
            group: String::new(),
        })
    })?;
    let mut tasks = rows.collect::<Result<Vec<Task>>>()?;
    for t in &mut tasks {
        t.group = group(t, today).to_string();
    }
    let rank = |g: &str| GROUPS.iter().position(|x| *x == g).unwrap_or(GROUPS.len());
    tasks.sort_by(|a, b| {
        rank(&a.group).cmp(&rank(&b.group)).then_with(|| match a.group.as_str() {
            "done" => b.done_ms.cmp(&a.done_ms).then(a.created_ms.cmp(&b.created_ms)),
            "upcoming" => a.due_day.cmp(&b.due_day).then(b.priority.cmp(&a.priority)).then(a.position.cmp(&b.position)),
            _ => b.priority.cmp(&a.priority).then(a.position.cmp(&b.position)),
        })
    });
    Ok(tasks)
}

const GROUPS: [&str; 5] = ["overdue", "today", "upcoming", "none", "done"];

fn group(t: &Task, today: &str) -> &'static str {
    match (t.done_ms, t.due_day.as_deref()) {
        (Some(_), _) => "done",
        (None, None) => "none",
        (None, Some(day)) if day < today => "overdue",
        (None, Some(day)) if day == today => "today",
        _ => "upcoming",
    }
}

/// A task's plan; None when there's no such task.
pub fn plan(conn: &Connection, id: &str) -> Result<Option<Plan>> {
    conn.query_row("SELECT due_day, priority, remind_ms, reminded FROM tasks WHERE id = ?1", [id], |r| {
        Ok(Plan { due: r.get(0)?, priority: r.get(1)?, remind: r.get(2)?, sent: r.get(3)? })
    })
    .optional()
}

/// Sets a task's due day, priority and exact reminder. A new day or reminder time clears the
/// reminders sent, so the new one is sent too.
pub fn set_plan(conn: &Connection, id: &str, due: Option<&str>, priority: u8, remind: Option<i64>) -> Result<()> {
    conn.execute(
        "UPDATE tasks SET
             reminded = CASE WHEN due_day IS ?2 AND remind_ms IS ?4 THEN reminded ELSE '' END,
             due_day = ?2, priority = ?3, remind_ms = ?4
         WHERE id = ?1",
        params![id, due, priority.min(HIGH), remind],
    )?;
    Ok(())
}

/// Notes a reminder as sent for a task, once.
pub fn mark_sent(conn: &Connection, id: &str, mark: &str) -> Result<()> {
    let Some(p) = plan(conn, id)? else { return Ok(()) };
    if p.sent.split(' ').any(|m| m == mark) {
        return Ok(());
    }
    let sent = if p.sent.is_empty() { mark.to_string() } else { format!("{} {mark}", p.sent) };
    conn.execute("UPDATE tasks SET reminded = ?2 WHERE id = ?1", params![id, sent])?;
    Ok(())
}

/// Takes another computer's plan for a task; false when the task isn't here (yet).
pub fn apply_plan(conn: &Connection, id: &str, p: &Plan) -> Result<bool> {
    let n = conn.execute(
        "UPDATE tasks SET due_day = ?2, priority = ?3, remind_ms = ?4, reminded = ?5 WHERE id = ?1",
        params![id, p.due, p.priority.min(HIGH), p.remind, p.sent],
    )?;
    Ok(n > 0)
}

/// The whole list as text, for sharing with combined computers.
pub fn to_json(conn: &Connection) -> Result<String> {
    let tasks: Vec<Shared> = list(conn, 0, "")?
        .into_iter()
        .map(|t| Shared { id: t.id, title: t.title, created_ms: t.created_ms, done_ms: t.done_ms, position: t.position, focused_ms: 0 })
        .collect();
    Ok(serde_json::to_string(&tasks).unwrap_or_else(|_| "[]".into()))
}

/// Takes another computer's list in place of this one's. Tasks that stay keep their plans,
/// which travel on their own.
pub fn replace_from_json(conn: &Connection, json: &str) -> std::result::Result<(), String> {
    let tasks: Vec<Shared> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    let mut kept = Vec::new();
    for t in tasks {
        let Some(title) = clean(&t.title) else { continue };
        tx.execute(
            "INSERT INTO tasks (id, title, created_ms, done_ms, position) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET title = ?2, created_ms = ?3, done_ms = ?4, position = ?5",
            params![t.id, title, t.created_ms, t.done_ms, t.position],
        )
        .map_err(|e| e.to_string())?;
        kept.push(t.id);
    }
    let gone: Vec<String> = {
        let mut stmt = tx.prepare("SELECT id FROM tasks").map_err(|e| e.to_string())?;
        let ids = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
        ids.filter_map(|id| id.ok()).filter(|id| !kept.contains(id)).collect()
    };
    for id in gone {
        tx.execute("DELETE FROM tasks WHERE id = ?1", [id]).map_err(|e| e.to_string())?;
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
        let titles = |c: &Connection| list(c, 30, TODAY).unwrap().into_iter().map(|t| t.title).collect::<Vec<_>>();
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
        assert_eq!(list(&c, 70 * M, TODAY).unwrap()[0].focused_ms, 35 * M);
        remove(&c, &t.id).unwrap();
        assert!(list(&c, 70 * M, TODAY).unwrap().is_empty());
    }

    const TODAY: &str = "2026-11-03";

    #[test]
    fn plan_fields_default_to_medium_with_no_day() {
        let c = conn();
        let t = add(&c, "Essay", 1).unwrap().unwrap();
        let got = &list(&c, 1, TODAY).unwrap()[0];
        assert_eq!((got.priority, got.due_day.as_deref(), got.remind_ms, got.group.as_str()), (1, None, None, "none"));
        assert_eq!(plan(&c, &t.id).unwrap(), Some(Plan { due: None, priority: 1, remind: None, sent: String::new() }));
        assert_eq!(plan(&c, "nobody").unwrap(), None);
    }

    #[test]
    fn open_tasks_group_by_day_then_priority() {
        let c = conn();
        let add_plan = |title: &str, due: Option<&str>, priority: u8| {
            let t = add(&c, title, 1).unwrap().unwrap();
            set_plan(&c, &t.id, due, priority, None).unwrap();
            t
        };
        add_plan("later low", Some("2026-11-09"), 0);
        add_plan("soon", Some("2026-11-05"), 0);
        add_plan("undated", None, 1);
        add_plan("today low", Some(TODAY), 0);
        add_plan("today high", Some(TODAY), 2);
        add_plan("late", Some("2026-11-01"), 1);
        let done = add_plan("finished", Some("2026-11-01"), 2);
        set_done(&c, &done.id, true, 5).unwrap();
        let got: Vec<(String, String)> = list(&c, 10, TODAY).unwrap().into_iter().map(|t| (t.group, t.title)).collect();
        let want = [
            ("overdue", "late"),
            ("today", "today high"),
            ("today", "today low"),
            ("upcoming", "soon"),
            ("upcoming", "later low"),
            ("none", "undated"),
            ("done", "finished"),
        ];
        assert_eq!(got, want.map(|(g, t)| (g.to_string(), t.to_string())));
    }

    #[test]
    fn changing_the_day_or_reminder_clears_what_was_sent() {
        let c = conn();
        let t = add(&c, "Essay", 1).unwrap().unwrap();
        set_plan(&c, &t.id, Some(TODAY), 1, Some(500)).unwrap();
        mark_sent(&c, &t.id, "due:2026-11-03").unwrap();
        mark_sent(&c, &t.id, "at:500").unwrap();
        mark_sent(&c, &t.id, "at:500").unwrap();
        assert_eq!(plan(&c, &t.id).unwrap().unwrap().sent, "due:2026-11-03 at:500");
        // Priority alone keeps them.
        set_plan(&c, &t.id, Some(TODAY), 2, Some(500)).unwrap();
        assert_eq!(plan(&c, &t.id).unwrap().unwrap().sent, "due:2026-11-03 at:500");
        set_plan(&c, &t.id, Some("2026-11-04"), 2, Some(500)).unwrap();
        assert_eq!(plan(&c, &t.id).unwrap().unwrap().sent, "");
    }

    #[test]
    fn a_plan_applies_only_to_a_task_here() {
        let c = conn();
        let t = add(&c, "Essay", 1).unwrap().unwrap();
        let p = Plan { due: Some(TODAY.into()), priority: 2, remind: Some(9), sent: "at:9".into() };
        assert!(apply_plan(&c, &t.id, &p).unwrap());
        assert_eq!(plan(&c, &t.id).unwrap(), Some(p.clone()));
        assert!(!apply_plan(&c, "elsewhere", &p).unwrap());
    }

    #[test]
    fn replacing_the_list_keeps_plans_of_tasks_that_stay() {
        let (here, there) = (conn(), conn());
        let t = add(&here, "Essay", 1).unwrap().unwrap();
        set_plan(&here, &t.id, Some(TODAY), 2, None).unwrap();
        // The other computer has the same task, renamed, and its own list knows nothing of plans.
        replace_from_json(&there, &to_json(&here).unwrap()).unwrap();
        rename(&there, &t.id, "Essay draft").unwrap();
        replace_from_json(&here, &to_json(&there).unwrap()).unwrap();
        let got = &list(&here, 1, TODAY).unwrap()[0];
        assert_eq!((got.title.as_str(), got.priority, got.due_day.as_deref()), ("Essay draft", 2, Some(TODAY)));
    }

    #[test]
    fn the_shared_list_carries_no_plans() {
        let c = conn();
        let t = add(&c, "Essay", 1).unwrap().unwrap();
        set_plan(&c, &t.id, Some(TODAY), 2, Some(9)).unwrap();
        let json = to_json(&c).unwrap();
        for field in ["dueDay", "priority", "remindMs", "reminded", "group"] {
            assert!(!json.contains(field), "{json}");
        }
    }

    #[test]
    fn the_shared_list_replaces_this_one_and_keeps_ids() {
        let (here, there) = (conn(), conn());
        let t = add(&there, "Essay", 1).unwrap().unwrap();
        set_done(&there, &t.id, true, 5).unwrap();
        add(&here, "Old", 1).unwrap();
        replace_from_json(&here, &to_json(&there).unwrap()).unwrap();
        let got = list(&here, 10, TODAY).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].id.as_str(), got[0].title.as_str(), got[0].done_ms), (t.id.as_str(), "Essay", Some(5)));
        assert!(replace_from_json(&here, "not json").is_err());
    }
}
