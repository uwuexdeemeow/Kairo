//! Tasks, kept in Ledger: a to-do list whose tasks gather the focus time of the
//! sessions started on them. Each has a random id, so the list can travel to combined computers
//! whole and still match the sessions there that point at it.

use std::collections::HashMap;

use chrono::{Datelike, Days, NaiveDate, Weekday};
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
    /// The note it's attached to, by id (notebook.rs); shared as `tasknote:<id>`.
    #[serde(default)]
    pub note: Option<String>,
    /// Its checklist; shared with `repeat` as `taskmore:<id>`.
    #[serde(default)]
    pub steps: Vec<Step>,
    /// How it comes back once ticked: "day", "weekday", "week", "month" or "year"; None for never.
    #[serde(default)]
    pub repeat: Option<String>,
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
            note: None,
            steps: Vec::new(),
            repeat: None,
            group: "none".into(),
        }
    }
}

/// One line of a task's checklist.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Step {
    /// What to do.
    pub t: String,
    /// Whether it's ticked.
    pub d: bool,
}

/// Steps a task may hold, and how long one may be.
pub const MAX_STEPS: usize = 30;
pub const MAX_STEP_CHARS: usize = 120;

/// The ways a task can repeat.
pub const REPEATS: [&str; 5] = ["day", "weekday", "week", "month", "year"];

/// What a task has beyond its plan, which travels to combined computers as its own shared choice
/// (`taskmore:<id>`), so a Kairo from before it can't wipe it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct More {
    #[serde(default)]
    pub repeat: Option<String>,
    #[serde(default)]
    pub steps: Vec<Step>,
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
        note: None,
        steps: Vec::new(),
        repeat: None,
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
    let mut stmt = conn.prepare("SELECT id, title, created_ms, done_ms, position, due_day, priority, remind_ms, reminded, note, steps, repeat FROM tasks")?;
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
            note: r.get(9)?,
            steps: parse_steps(&r.get::<_, String>(10)?),
            repeat: r.get(11)?,
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

/// Attaches a note to a task, or (None) detaches it; false when there's no such task.
pub fn set_note(conn: &Connection, id: &str, note: Option<&str>) -> Result<bool> {
    Ok(conn.execute("UPDATE tasks SET note = ?2 WHERE id = ?1", params![id, note.filter(|n| !n.is_empty())])? > 0)
}

/// A task's plan as its shared choice carries it; None when there's no such task.
pub fn plan_json(conn: &Connection, id: &str) -> Result<Option<String>> {
    Ok(plan(conn, id)?.map(|p| serde_json::to_string(&p).expect("plain data")))
}

/// The tasks with a plan beyond how they came (a day, a reminder or another priority), with
/// their plans' shared text, for sharing when combining starts.
pub fn planned(conn: &Connection) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT id FROM tasks WHERE due_day IS NOT NULL OR remind_ms IS NOT NULL OR priority != 1")?;
    let ids = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>>>()?;
    ids.into_iter().filter_map(|id| plan_json(conn, &id).transpose().map(|p| p.map(|p| (id, p)))).collect()
}

/// Takes another computer's plan for a task; false when the task isn't here (yet).
pub fn apply_plan(conn: &Connection, id: &str, p: &Plan) -> Result<bool> {
    let n = conn.execute(
        "UPDATE tasks SET due_day = ?2, priority = ?3, remind_ms = ?4, reminded = ?5 WHERE id = ?1",
        params![id, p.due, p.priority.min(HIGH), p.remind, p.sent],
    )?;
    Ok(n > 0)
}

/// A checklist as kept: each step trimmed and short, empty ones dropped, at most `MAX_STEPS`.
pub fn clean_steps(steps: Vec<Step>) -> Vec<Step> {
    steps
        .into_iter()
        .filter_map(|s| {
            let t: String = s.t.trim().chars().take(MAX_STEP_CHARS).collect();
            let t = t.trim_end().to_string();
            (!t.is_empty()).then_some(Step { t, d: s.d })
        })
        .take(MAX_STEPS)
        .collect()
}

/// A checklist read from its stored text; none for text that isn't one.
fn parse_steps(text: &str) -> Vec<Step> {
    if text.is_empty() {
        return Vec::new();
    }
    serde_json::from_str(text).map(clean_steps).unwrap_or_default()
}

fn steps_text(steps: &[Step]) -> String {
    if steps.is_empty() {
        String::new()
    } else {
        serde_json::to_string(steps).expect("plain data")
    }
}

/// Gives a task its checklist (cleaned); false when there's no such task.
pub fn set_steps(conn: &Connection, id: &str, steps: Vec<Step>) -> Result<bool> {
    let text = steps_text(&clean_steps(steps));
    Ok(conn.execute("UPDATE tasks SET steps = ?2 WHERE id = ?1", params![id, text])? > 0)
}

/// What a task has beyond its plan; None when there's no such task.
pub fn more(conn: &Connection, id: &str) -> Result<Option<More>> {
    conn.query_row("SELECT repeat, steps FROM tasks WHERE id = ?1", [id], |r| Ok(More { repeat: r.get(0)?, steps: parse_steps(&r.get::<_, String>(1)?) })).optional()
}

/// A task's repeat and steps as their shared choice carries them; None when there's no such task.
pub fn more_json(conn: &Connection, id: &str) -> Result<Option<String>> {
    Ok(more(conn, id)?.map(|m| serde_json::to_string(&m).expect("plain data")))
}

/// The tasks with a repeat or steps, with their shared text, for sharing when combining starts.
pub fn with_more(conn: &Connection) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT id FROM tasks WHERE repeat IS NOT NULL OR steps != ''")?;
    let ids = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>>>()?;
    ids.into_iter().filter_map(|id| more_json(conn, &id).transpose().map(|m| m.map(|m| (id, m)))).collect()
}

/// Takes another computer's repeat and steps for a task; false when the task isn't here (yet).
pub fn apply_more(conn: &Connection, id: &str, m: &More) -> Result<bool> {
    let text = steps_text(&clean_steps(m.steps.clone()));
    let repeat = m.repeat.as_deref().filter(|r| REPEATS.contains(r));
    Ok(conn.execute("UPDATE tasks SET repeat = ?2, steps = ?3 WHERE id = ?1", params![id, repeat, text])? > 0)
}

/// The first day a repeating task comes due again, after the later of its due day and `today`, so
/// ticking early skips nothing and ticking late leaves nothing overdue. A week, month or year
/// keeps the due day's weekday, date or month and date (a 31st falls on a short month's last day;
/// 29 Feb on 28 Feb in other years); with no due day, today's. None for a rule that isn't one.
pub fn next_due(rule: &str, due: Option<NaiveDate>, today: NaiveDate) -> Option<NaiveDate> {
    let anchor = due.unwrap_or(today);
    let base = anchor.max(today);
    match rule {
        "day" => base.succ_opt(),
        "weekday" => {
            let mut day = base.succ_opt()?;
            while matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
                day = day.succ_opt()?;
            }
            Some(day)
        }
        "week" => {
            let weeks = (base - anchor).num_days() / 7 + 1;
            anchor.checked_add_days(Days::new(weeks as u64 * 7))
        }
        "month" => {
            let (mut year, mut month) = (base.year(), base.month());
            loop {
                let day = clamped(year, month, anchor.day())?;
                if day > base {
                    return Some(day);
                }
                (year, month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
            }
        }
        "year" => {
            let mut year = base.year();
            loop {
                let day = clamped(year, anchor.month(), anchor.day())?;
                if day > base {
                    return Some(day);
                }
                year += 1;
            }
        }
        _ => None,
    }
}

/// `day` of the month, or its last day when the month is shorter.
fn clamped(year: i32, month: u32, day: u32) -> Option<NaiveDate> {
    (1..=day).rev().find_map(|d| NaiveDate::from_ymd_opt(year, month, d))
}

/// The id of a repeating task's next one, made from its own so two combined computers ticking the
/// same task make the same new one: `abc` gives `abc~1`, which gives `abc~2`.
fn next_id(id: &str) -> String {
    let (base, n) = id.split_once('~').map_or((id, 0), |(base, n)| (base, n.parse::<u32>().unwrap_or(0)));
    format!("{base}~{}", n + 1)
}

/// `ms` moved to `day`, at the same local time of day.
fn shift_remind(ms: i64, day: NaiveDate) -> Option<i64> {
    use chrono::{Local, TimeZone};
    let time = Local.timestamp_millis_opt(ms).single()?.time();
    Local.from_local_datetime(&day.and_time(time)).earliest().map(|d| d.timestamp_millis())
}

/// What ticking or reopening a task did besides: the repeat it made, or the untouched one it took away.
#[derive(Debug, Default, PartialEq)]
pub struct Ticked {
    pub added: Option<String>,
    pub removed: Option<String>,
}

/// Ticks a task off, or reopens it. Ticking a repeating one also adds its next: the same title,
/// priority, reminder time of day, note and rule, its steps unticked, due on `next_due`; if that
/// one is already there nothing is added. Reopening takes that next one away again, if it's still
/// untouched (open, no focus session on it, no step ticked).
pub fn tick(conn: &Connection, id: &str, done: bool, now: i64, today: NaiveDate) -> Result<Ticked> {
    let mut out = Ticked::default();
    type Row = (String, Option<String>, u8, Option<i64>, Option<String>, String, Option<String>, Option<i64>);
    let row: Option<Row> = conn
        .query_row("SELECT title, due_day, priority, remind_ms, note, steps, repeat, done_ms FROM tasks WHERE id = ?1", [id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
        })
        .optional()?;
    let Some((title, due, priority, remind, note, steps, repeat, done_ms)) = row else { return Ok(out) };
    set_done(conn, id, done, now)?;
    let Some(rule) = repeat else { return Ok(out) };
    let copy = next_id(id);
    if done && done_ms.is_none() {
        let due = due.and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok());
        let Some(next) = next_due(&rule, due, today) else { return Ok(out) };
        let remind = remind.and_then(|ms| shift_remind(ms, next));
        let steps = steps_text(&parse_steps(&steps).into_iter().map(|s| Step { d: false, ..s }).collect::<Vec<_>>());
        let added = conn.execute(
            "INSERT OR IGNORE INTO tasks (id, title, created_ms, done_ms, position, due_day, priority, remind_ms, reminded, note, steps, repeat)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7, '', ?8, ?9, ?10)",
            params![copy, title, now, next_position(conn)?, next.format("%Y-%m-%d").to_string(), priority, remind, note, steps, rule],
        )?;
        if added > 0 {
            out.added = Some(copy);
        }
    } else if !done && done_ms.is_some() {
        let untouched = conn
            .query_row("SELECT steps FROM tasks WHERE id = ?1 AND done_ms IS NULL", [&copy], |r| r.get::<_, String>(0))
            .optional()?
            .is_some_and(|steps| !parse_steps(&steps).iter().any(|s| s.d))
            && conn.query_row("SELECT COUNT(*) FROM focus_sessions WHERE task = ?1", [&copy], |r| r.get::<_, i64>(0))? == 0;
        if untouched {
            remove(conn, &copy)?;
            out.removed = Some(copy);
        }
    }
    Ok(out)
}

/// How a task repeats; the rule must be one of `REPEATS`, or None for never. A task made to repeat
/// with no due day is due today. False when there's no such task or the rule isn't one.
pub fn set_repeat(conn: &Connection, id: &str, repeat: Option<&str>, today: &str) -> Result<bool> {
    if repeat.is_some_and(|r| !REPEATS.contains(&r)) {
        return Ok(false);
    }
    Ok(conn.execute(
        "UPDATE tasks SET repeat = ?2, due_day = CASE WHEN ?2 IS NOT NULL AND due_day IS NULL THEN ?3 ELSE due_day END WHERE id = ?1",
        params![id, repeat, today],
    )? > 0)
}

/// The note a task is attached to.
pub fn note_of(conn: &Connection, id: &str) -> Result<Option<String>> {
    Ok(conn.query_row("SELECT note FROM tasks WHERE id = ?1", [id], |r| r.get(0)).optional()?.flatten())
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
    use chrono::TimeZone;

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
        set_steps(&c, &t.id, vec![Step { t: "Outline".into(), d: false }]).unwrap();
        let json = to_json(&c).unwrap();
        for field in ["dueDay", "priority", "remindMs", "reminded", "group", "steps", "repeat", "Outline"] {
            assert!(!json.contains(field), "{json}");
        }
    }

    fn step(t: &str, d: bool) -> Step {
        Step { t: t.into(), d }
    }

    #[test]
    fn steps_are_trimmed_short_and_at_most_thirty() {
        let c = conn();
        let t = add(&c, "Essay", 1).unwrap().unwrap();
        let mut steps = vec![step("  Outline  ", true), step("   ", false), step(&"x".repeat(300), false)];
        steps.extend((0..40).map(|i| step(&format!("step {i}"), false)));
        assert!(set_steps(&c, &t.id, steps).unwrap());
        let got = &list(&c, 1, TODAY).unwrap()[0].steps;
        assert_eq!(got.len(), MAX_STEPS);
        assert_eq!(got[0], step("Outline", true));
        assert_eq!(got[1].t.chars().count(), MAX_STEP_CHARS);
        assert_eq!(got[2], step("step 0", false));
        // No such task: said, not made up.
        assert!(!set_steps(&c, "elsewhere", vec![step("a", false)]).unwrap());
    }

    #[test]
    fn no_steps_is_kept_as_nothing_and_a_task_starts_with_none() {
        let c = conn();
        let t = add(&c, "Essay", 1).unwrap().unwrap();
        assert!(list(&c, 1, TODAY).unwrap()[0].steps.is_empty());
        set_steps(&c, &t.id, vec![step("a", false)]).unwrap();
        set_steps(&c, &t.id, vec![]).unwrap();
        assert!(list(&c, 1, TODAY).unwrap()[0].steps.is_empty());
        assert!(with_more(&c).unwrap().is_empty());
    }

    #[test]
    fn repeat_and_steps_travel_as_their_own_choice_and_apply_only_to_a_task_here() {
        let (here, there) = (conn(), conn());
        let t = add(&there, "Essay", 1).unwrap().unwrap();
        set_steps(&there, &t.id, vec![step("Outline", true), step("Draft", false)]).unwrap();
        assert_eq!(with_more(&there).unwrap().len(), 1);
        let json = more_json(&there, &t.id).unwrap().unwrap();
        let m: More = serde_json::from_str(&json).unwrap();
        // The task isn't here yet: it waits.
        assert!(!apply_more(&here, &t.id, &m).unwrap());
        replace_from_json(&here, &to_json(&there).unwrap()).unwrap();
        assert!(apply_more(&here, &t.id, &m).unwrap());
        assert_eq!(list(&here, 1, TODAY).unwrap()[0].steps, m.steps);
        // Another computer's unknown repeat is not kept.
        let odd = More { repeat: Some("fortnight".into()), steps: vec![] };
        apply_more(&here, &t.id, &odd).unwrap();
        let got = &list(&here, 1, TODAY).unwrap()[0];
        assert_eq!((got.repeat.clone(), got.steps.len()), (None, 0));
    }

    #[test]
    fn a_list_taken_from_another_computer_keeps_the_steps_here() {
        let (here, there) = (conn(), conn());
        let t = add(&there, "Essay", 1).unwrap().unwrap();
        replace_from_json(&here, &to_json(&there).unwrap()).unwrap();
        set_steps(&here, &t.id, vec![step("Outline", false)]).unwrap();
        rename(&there, &t.id, "Essay draft").unwrap();
        replace_from_json(&here, &to_json(&there).unwrap()).unwrap();
        assert_eq!(list(&here, 1, TODAY).unwrap()[0].steps, vec![step("Outline", false)]);
    }

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn days_and_weekdays_come_back_after_the_later_of_due_and_today() {
        // Thursday 12 Nov 2026.
        let today = d("2026-11-12");
        assert_eq!(next_due("day", Some(d("2026-11-12")), today), Some(d("2026-11-13")));
        // Ticked early: after its due day. Ticked late: after today.
        assert_eq!(next_due("day", Some(d("2026-11-20")), today), Some(d("2026-11-21")));
        assert_eq!(next_due("day", Some(d("2026-11-01")), today), Some(d("2026-11-13")));
        assert_eq!(next_due("day", None, today), Some(d("2026-11-13")));
        // Friday goes to Monday; a weekend does too.
        assert_eq!(next_due("weekday", Some(d("2026-11-13")), today), Some(d("2026-11-16")));
        assert_eq!(next_due("weekday", Some(d("2026-11-14")), today), Some(d("2026-11-16")));
        assert_eq!(next_due("weekday", Some(d("2026-11-12")), today), Some(d("2026-11-13")));
    }

    #[test]
    fn a_week_keeps_its_weekday_and_never_lands_on_or_before_today() {
        let today = d("2026-11-12");
        assert_eq!(next_due("week", Some(d("2026-11-13")), today), Some(d("2026-11-20")));
        assert_eq!(next_due("week", Some(d("2026-11-12")), today), Some(d("2026-11-19")));
        // A Monday task ticked on Thursday, a week late: the Monday after today, not a past one.
        assert_eq!(next_due("week", Some(d("2026-11-02")), today), Some(d("2026-11-16")));
        assert_eq!(next_due("week", Some(d("2026-10-05")), today), Some(d("2026-11-16")));
        assert_eq!(next_due("week", None, today), Some(d("2026-11-19")));
    }

    #[test]
    fn a_month_keeps_its_date_and_a_short_month_takes_its_last_day() {
        let today = d("2026-11-12");
        assert_eq!(next_due("month", Some(d("2026-11-15")), today), Some(d("2026-12-15")));
        assert_eq!(next_due("month", Some(d("2026-11-12")), today), Some(d("2026-12-12")));
        assert_eq!(next_due("month", Some(d("2026-12-31")), d("2026-12-31")), Some(d("2027-01-31")));
        assert_eq!(next_due("month", Some(d("2027-01-31")), d("2027-01-20")), Some(d("2027-02-28")));
        assert_eq!(next_due("month", Some(d("2028-01-31")), d("2028-01-31")), Some(d("2028-02-29")));
        // Late by months: the next one after today.
        assert_eq!(next_due("month", Some(d("2026-06-05")), today), Some(d("2026-12-05")));
        assert_eq!(next_due("month", Some(d("2026-06-20")), today), Some(d("2026-11-20")));
    }

    #[test]
    fn a_year_keeps_its_date_and_29_feb_falls_on_the_28th() {
        let today = d("2026-11-12");
        assert_eq!(next_due("year", Some(d("2026-11-12")), today), Some(d("2027-11-12")));
        assert_eq!(next_due("year", Some(d("2026-03-01")), today), Some(d("2027-03-01")));
        assert_eq!(next_due("year", Some(d("2028-02-29")), d("2028-02-29")), Some(d("2029-02-28")));
        assert_eq!(next_due("fortnight", Some(today), today), None);
    }

    fn repeating(c: &Connection, title: &str, rule: &str, due: &str) -> Task {
        let t = add(c, title, 1).unwrap().unwrap();
        set_plan(c, &t.id, Some(due), 2, None).unwrap();
        assert!(set_repeat(c, &t.id, Some(rule), TODAY).unwrap());
        t
    }

    #[test]
    fn ticking_a_repeating_task_adds_its_next_and_keeps_the_done_one() {
        let c = conn();
        let t = repeating(&c, "Water plants", "week", "2026-11-12");
        set_steps(&c, &t.id, vec![step("Kitchen", true), step("Hall", false)]).unwrap();
        let ticked = tick(&c, &t.id, true, 100, d("2026-11-12")).unwrap();
        assert_eq!(ticked.added.as_deref(), Some(format!("{}~1", t.id).as_str()));
        let all = list(&c, 100, "2026-11-12").unwrap();
        assert_eq!(all.len(), 2);
        let next = all.iter().find(|x| x.id.ends_with("~1")).unwrap();
        assert_eq!((next.title.as_str(), next.due_day.as_deref(), next.priority, next.repeat.as_deref()), ("Water plants", Some("2026-11-19"), 2, Some("week")));
        assert_eq!(next.steps, vec![step("Kitchen", false), step("Hall", false)]);
        assert!(next.done_ms.is_none());
        // The ticked one stays, done, in the log.
        assert_eq!(all.iter().find(|x| x.id == t.id).unwrap().done_ms, Some(100));
    }

    #[test]
    fn the_next_one_brings_its_reminder_time_and_note() {
        let c = conn();
        let t = repeating(&c, "Pills", "day", "2026-11-12");
        let nine = chrono::Local.with_ymd_and_hms(2026, 11, 12, 9, 0, 0).earliest().unwrap().timestamp_millis();
        set_plan(&c, &t.id, Some("2026-11-12"), 1, Some(nine)).unwrap();
        set_note(&c, &t.id, Some("note1")).unwrap();
        let new = tick(&c, &t.id, true, 100, d("2026-11-12")).unwrap().added.unwrap();
        let next = list(&c, 100, "2026-11-12").unwrap().into_iter().find(|x| x.id == new).unwrap();
        let tomorrow_nine = chrono::Local.with_ymd_and_hms(2026, 11, 13, 9, 0, 0).earliest().unwrap().timestamp_millis();
        assert_eq!((next.remind_ms, next.note.as_deref(), next.reminded.as_str()), (Some(tomorrow_nine), Some("note1"), ""));
    }

    #[test]
    fn a_task_that_doesnt_repeat_adds_nothing() {
        let c = conn();
        let t = add(&c, "Essay", 1).unwrap().unwrap();
        assert_eq!(tick(&c, &t.id, true, 5, d("2026-11-12")).unwrap(), Ticked::default());
        assert_eq!(list(&c, 5, TODAY).unwrap().len(), 1);
        assert_eq!(tick(&c, "elsewhere", true, 5, d("2026-11-12")).unwrap(), Ticked::default());
    }

    #[test]
    fn ticking_twice_or_on_two_computers_makes_one_next_task() {
        let (a, b) = (conn(), conn());
        let t = repeating(&a, "Rent", "month", "2026-11-01");
        replace_from_json(&b, &to_json(&a).unwrap()).unwrap();
        let more = more(&a, &t.id).unwrap().unwrap();
        apply_more(&b, &t.id, &more).unwrap();
        set_plan(&b, &t.id, Some("2026-11-01"), 2, None).unwrap();
        let (x, y) = (tick(&a, &t.id, true, 10, d("2026-11-12")).unwrap(), tick(&b, &t.id, true, 11, d("2026-11-12")).unwrap());
        assert_eq!(x.added, y.added);
        // Ticking a done one again changes nothing.
        assert_eq!(tick(&a, &t.id, true, 12, d("2026-11-12")).unwrap().added, None);
        assert_eq!(list(&a, 20, TODAY).unwrap().len(), 2);
    }

    #[test]
    fn the_chain_of_ids_stays_short() {
        assert_eq!(next_id("abc"), "abc~1");
        assert_eq!(next_id("abc~1"), "abc~2");
        assert_eq!(next_id("abc~41"), "abc~42");
    }

    #[test]
    fn reopening_takes_the_untouched_next_one_away_but_keeps_one_you_used() {
        let c = conn();
        let t = repeating(&c, "Gym", "day", "2026-11-12");
        let copy = tick(&c, &t.id, true, 10, d("2026-11-12")).unwrap().added.unwrap();
        let undone = tick(&c, &t.id, false, 11, d("2026-11-12")).unwrap();
        assert_eq!(undone.removed.as_deref(), Some(copy.as_str()));
        assert_eq!(list(&c, 12, TODAY).unwrap().len(), 1);
        // Tick again, then use the new one (a step ticked): reopening leaves it.
        let copy = tick(&c, &t.id, true, 20, d("2026-11-12")).unwrap().added.unwrap();
        set_steps(&c, &copy, vec![step("warm up", true)]).unwrap();
        assert_eq!(tick(&c, &t.id, false, 21, d("2026-11-12")).unwrap().removed, None);
        assert_eq!(list(&c, 22, TODAY).unwrap().len(), 2);
    }

    #[test]
    fn reopening_keeps_a_next_one_a_focus_session_was_started_on() {
        let c = conn();
        let t = repeating(&c, "Read", "day", "2026-11-12");
        let copy = tick(&c, &t.id, true, 10, d("2026-11-12")).unwrap().added.unwrap();
        c.execute("INSERT INTO focus_sessions (start_ms, planned_ms, task) VALUES (1, 60000, ?1)", [&copy]).unwrap();
        assert_eq!(tick(&c, &t.id, false, 11, d("2026-11-12")).unwrap().removed, None);
    }

    #[test]
    fn a_repeat_with_no_due_day_is_due_today_and_an_odd_rule_is_refused() {
        let c = conn();
        let t = add(&c, "Stretch", 1).unwrap().unwrap();
        assert!(!set_repeat(&c, &t.id, Some("fortnight"), TODAY).unwrap());
        assert!(!set_repeat(&c, "elsewhere", Some("day"), TODAY).unwrap());
        assert!(set_repeat(&c, &t.id, Some("day"), TODAY).unwrap());
        let got = &list(&c, 1, TODAY).unwrap()[0];
        assert_eq!((got.repeat.as_deref(), got.due_day.as_deref()), (Some("day"), Some(TODAY)));
        // Never again: the day stays.
        assert!(set_repeat(&c, &t.id, None, TODAY).unwrap());
        assert_eq!(list(&c, 1, TODAY).unwrap()[0].repeat, None);
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
