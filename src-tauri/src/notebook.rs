//! Ledger's notes: standalone Markdown text in one list, newest edited first, pinned on top.
//! Each has a random id, the same on every combined computer, and a task can point at one.
//! A deleted note stays a while as a marker, so the deletion reaches every computer.

use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::Serialize;

/// Notes are text, not documents; this keeps one well inside a sync file.
pub const MAX_CHARS: usize = 100_000;
const TITLE_CHARS: usize = 80;
pub const UNTITLED: &str = "New note";

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
    pub created_ms: i64,
    pub edited_ms: i64,
    pub pinned: bool,
    /// The open tasks it's attached to: (id, title).
    pub tasks: Vec<(String, String)>,
}

/// A note's title: its first line, without leading `#`s, kept short.
pub fn title_of(body: &str) -> String {
    let first = body.lines().map(|l| l.trim_start_matches(|c: char| c == '#' || c.is_whitespace()).trim_end()).find(|l| !l.is_empty());
    match first {
        Some(line) => line.chars().take(TITLE_CHARS).collect::<String>().trim_end().to_string(),
        None => UNTITLED.to_string(),
    }
}

fn capped(body: &str) -> &str {
    match body.char_indices().nth(MAX_CHARS) {
        Some((at, _)) => &body[..at],
        None => body,
    }
}

pub fn create(conn: &Connection, text: &str, now: i64) -> Result<Note> {
    let id = crate::sync::new_id();
    conn.execute(
        "INSERT INTO notebook (id, body, created_ms, edited_ms, changed_ms) VALUES (?1, ?2, ?3, ?3, ?3)",
        params![id, capped(text), now],
    )?;
    Ok(get(conn, &id)?.expect("just made"))
}

/// Saves a note's text, kept to `MAX_CHARS`; None when it's gone.
pub fn save(conn: &Connection, id: &str, body: &str, now: i64) -> Result<Option<Note>> {
    conn.execute(
        "UPDATE notebook SET body = ?2, edited_ms = ?3, changed_ms = MAX(changed_ms + 1, ?3)
         WHERE id = ?1 AND deleted = 0 AND body != ?2",
        params![id, capped(body), now],
    )?;
    get(conn, id)
}

pub fn set_pinned(conn: &Connection, id: &str, pinned: bool, now: i64) -> Result<()> {
    conn.execute(
        "UPDATE notebook SET pinned = ?2, changed_ms = MAX(changed_ms + 1, ?3) WHERE id = ?1 AND deleted = 0 AND pinned != ?2",
        params![id, pinned, now],
    )?;
    Ok(())
}

/// Deletes a note, leaving a marker for combined computers, and detaches it from its tasks,
/// whose ids come back so their links can be shared again.
pub fn remove(conn: &Connection, id: &str, now: i64) -> Result<Vec<String>> {
    conn.execute("UPDATE notebook SET deleted = 1, body = '', pinned = 0, changed_ms = MAX(changed_ms + 1, ?2) WHERE id = ?1 AND deleted = 0", params![id, now])?;
    detach(conn, id)
}

/// Takes `id` off every task pointing at it; the tasks' ids.
pub(crate) fn detach(conn: &Connection, id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT id FROM tasks WHERE note = ?1 ORDER BY position")?;
    let tasks = stmt.query_map([id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>>>()?;
    conn.execute("UPDATE tasks SET note = NULL WHERE note = ?1", [id])?;
    Ok(tasks)
}

fn attached(conn: &Connection, id: &str) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT id, title FROM tasks WHERE note = ?1 AND done_ms IS NULL ORDER BY position")?;
    let rows = stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

fn from_row(conn: &Connection, r: &rusqlite::Row) -> Result<Note> {
    let id: String = r.get(0)?;
    let body: String = r.get(1)?;
    Ok(Note { tasks: attached(conn, &id)?, title: title_of(&body), id, body, created_ms: r.get(2)?, edited_ms: r.get(3)?, pinned: r.get(4)? })
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<Note>> {
    conn.query_row("SELECT id, body, created_ms, edited_ms, pinned FROM notebook WHERE id = ?1 AND deleted = 0", [id], |r| from_row(conn, r)).optional()
}

/// The notes, pinned first then newest edited; with `query`, those whose title or text has it.
pub fn list(conn: &Connection, query: &str) -> Result<Vec<Note>> {
    let q = query.trim().to_lowercase();
    let mut stmt = conn.prepare("SELECT id, body, created_ms, edited_ms, pinned FROM notebook WHERE deleted = 0 ORDER BY pinned DESC, edited_ms DESC, id")?;
    let rows = stmt.query_map([], |r| from_row(conn, r))?;
    let mut notes = Vec::new();
    for note in rows {
        let note = note?;
        // Matched here rather than with LIKE, which would take % and _ as wildcards and knows
        // only ASCII case.
        if q.is_empty() || note.body.to_lowercase().contains(&q) {
            notes.push(note);
        }
    }
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        crate::db::open(std::path::Path::new(":memory:")).unwrap()
    }

    fn titles(c: &Connection, q: &str) -> Vec<String> {
        list(c, q).unwrap().into_iter().map(|n| n.title).collect()
    }

    #[test]
    fn the_title_is_the_first_line_without_hashes() {
        assert_eq!(title_of("# Essay plan\n- intro"), "Essay plan");
        assert_eq!(title_of("  ### Deep  \nx"), "Deep");
        assert_eq!(title_of("Shopping"), "Shopping");
        // Blank lines before it are passed over.
        assert_eq!(title_of("\n\nlater"), "later");
        assert_eq!(title_of("#\n  \n"), UNTITLED);
        assert_eq!(title_of(""), UNTITLED);
        assert_eq!(title_of(&"x".repeat(200)).chars().count(), 80);
    }

    #[test]
    fn a_note_is_kept_to_its_limit() {
        let c = conn();
        let n = create(&c, "Big", 1).unwrap();
        let saved = save(&c, &n.id, &"é".repeat(MAX_CHARS + 50), 2).unwrap().unwrap();
        assert_eq!(saved.body.chars().count(), MAX_CHARS);
        // Shortening one at its limit still saves.
        assert_eq!(save(&c, &n.id, "short", 3).unwrap().unwrap().body, "short");
        assert_eq!(save(&c, "gone", "x", 4).unwrap(), None);
    }

    #[test]
    fn pinned_first_then_newest_edited() {
        let c = conn();
        let a = create(&c, "A", 1).unwrap();
        let b = create(&c, "B", 2).unwrap();
        create(&c, "C", 3).unwrap();
        save(&c, &a.id, "A again", 4).unwrap();
        assert_eq!(titles(&c, ""), ["A again", "C", "B"]);
        set_pinned(&c, &b.id, true, 5).unwrap();
        assert_eq!(titles(&c, ""), ["B", "A again", "C"]);
    }

    #[test]
    fn pinning_doesnt_count_as_an_edit() {
        let c = conn();
        let a = create(&c, "A", 1).unwrap();
        set_pinned(&c, &a.id, true, 9).unwrap();
        assert_eq!(get(&c, &a.id).unwrap().unwrap().edited_ms, 1);
        set_pinned(&c, &a.id, false, 10).unwrap();
        create(&c, "B", 2).unwrap();
        assert_eq!(titles(&c, ""), ["B", "A"]);
    }

    #[test]
    fn search_finds_title_and_text_any_case() {
        let c = conn();
        create(&c, "# Essay plan\nsources", 1).unwrap();
        create(&c, "Shopping\nmilk, ESSAY paper", 2).unwrap();
        create(&c, "Other", 3).unwrap();
        assert_eq!(titles(&c, "essay"), ["Shopping", "Essay plan"]);
        assert_eq!(titles(&c, "  SOURCES "), ["Essay plan"]);
        // Wildcards are taken as typed.
        assert!(titles(&c, "%").is_empty());
    }

    #[test]
    fn deleting_leaves_a_hidden_marker_and_detaches_tasks() {
        let c = conn();
        let n = create(&c, "Plan", 1).unwrap();
        let t = crate::tasks::add(&c, "Essay", 1).unwrap().unwrap();
        c.execute("UPDATE tasks SET note = ?1 WHERE id = ?2", params![n.id, t.id]).unwrap();
        assert_eq!(get(&c, &n.id).unwrap().unwrap().tasks, [(t.id.clone(), "Essay".to_string())]);
        assert_eq!(remove(&c, &n.id, 5).unwrap(), [t.id.clone()]);
        assert!(list(&c, "").unwrap().is_empty());
        assert_eq!(get(&c, &n.id).unwrap(), None);
        let still: Option<String> = c.query_row("SELECT note FROM tasks WHERE id = ?1", [&t.id], |r| r.get(0)).unwrap();
        assert_eq!(still, None);
        let marker: i64 = c.query_row("SELECT deleted FROM notebook WHERE id = ?1", [&n.id], |r| r.get(0)).unwrap();
        assert_eq!(marker, 1);
    }
}
