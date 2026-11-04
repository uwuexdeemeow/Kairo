//! Ledger's notes: standalone Markdown text in one list, newest edited first, pinned on top.
//! Each has a random id, the same on every combined computer, and a task can point at one.
//! A deleted note stays a while as a marker, so the deletion reaches every computer.

use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};

/// Notes are text, not documents; this keeps one well inside a sync file.
pub const MAX_CHARS: usize = 100_000;
const TITLE_CHARS: usize = 80;
pub const UNTITLED: &str = "New note";
/// The paper a note is written on; the first is every note's to begin with.
pub const PAPERS: [&str; 4] = ["lined", "blank", "dotted", "grid"];

/// A paper Kairo knows, or the first one: a newer computer's choice this one hasn't heard of
/// shows as lined.
fn paper_of(paper: &str) -> &'static str {
    PAPERS.iter().find(|p| **p == paper).unwrap_or(&PAPERS[0])
}

fn first_paper() -> String {
    PAPERS[0].to_string()
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
    pub created_ms: i64,
    pub edited_ms: i64,
    pub pinned: bool,
    /// Lined, blank, dotted or grid.
    pub paper: String,
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

/// The paper it's written on; a change to share, but not an edit.
pub fn set_paper(conn: &Connection, id: &str, paper: &str, now: i64) -> Result<()> {
    conn.execute(
        "UPDATE notebook SET paper = ?2, changed_ms = MAX(changed_ms + 1, ?3) WHERE id = ?1 AND deleted = 0 AND paper != ?2",
        params![id, paper_of(paper), now],
    )?;
    Ok(())
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
    Ok(Note { tasks: attached(conn, &id)?, title: title_of(&body), id, body, created_ms: r.get(2)?, edited_ms: r.get(3)?, pinned: r.get(4)?, paper: paper_of(&r.get::<_, String>(5)?).to_string() })
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<Note>> {
    conn.query_row("SELECT id, body, created_ms, edited_ms, pinned, paper FROM notebook WHERE id = ?1 AND deleted = 0", [id], |r| from_row(conn, r)).optional()
}

/// The notes, pinned first then newest edited; with `query`, those whose title or text has it.
pub fn list(conn: &Connection, query: &str) -> Result<Vec<Note>> {
    let q = query.trim().to_lowercase();
    let mut stmt = conn.prepare("SELECT id, body, created_ms, edited_ms, pinned, paper FROM notebook WHERE deleted = 0 ORDER BY pinned DESC, edited_ms DESC, id")?;
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

// ---------- sharing with combined computers ----------
//
// Every computer keeps a file for every note it has, `note-<id>.json`, rewritten when its copy
// changes, so one linked later finds them all. Per note the newest change wins; ties go to the
// deletion, then the larger text, so every computer picks the same one.

/// How long a deleted note's marker stays, for computers that were away to hear of it.
const MARKER_MS: i64 = 30 * 24 * 60 * 60 * 1000;
const CONFLICT: &str = " (conflicted copy)";

#[derive(Serialize, Deserialize, Clone, Debug)]
struct NoteFile {
    id: String,
    body: String,
    created: i64,
    edited: i64,
    changed: i64,
    pinned: bool,
    deleted: bool,
    /// Missing from the files of computers from before notes had paper.
    #[serde(default = "first_paper")]
    paper: String,
}

fn file_name(id: &str) -> String {
    format!("note-{id}.json")
}

fn id_of(file: &str) -> Option<&str> {
    file.strip_prefix("note-")?.strip_suffix(".json")
}

/// This computer's copy of a note, deleted ones included, with when it was last shared.
fn local(conn: &Connection, id: &str) -> Result<Option<(NoteFile, i64)>> {
    conn.query_row("SELECT id, body, created_ms, edited_ms, changed_ms, pinned, deleted, synced_ms, paper FROM notebook WHERE id = ?1", [id], |r| {
        Ok((NoteFile { id: r.get(0)?, body: r.get(1)?, created: r.get(2)?, edited: r.get(3)?, changed: r.get(4)?, pinned: r.get(5)?, deleted: r.get(6)?, paper: r.get(8)? }, r.get(7)?))
    })
    .optional()
}

/// Which of two copies wins: the later change, then a deletion, then the larger text.
fn rank(n: &NoteFile) -> (i64, bool, &str) {
    (n.changed, n.deleted, n.body.as_str())
}

/// A losing edit's text, kept as a note of its own: its first line marked as a conflicted copy.
fn conflicted(body: &str) -> String {
    let mut lines: Vec<String> = body.lines().map(String::from).collect();
    match lines.iter_mut().find(|l| !l.trim().is_empty()) {
        Some(first) => first.push_str(CONFLICT),
        None => lines.insert(0, format!("{UNTITLED}{CONFLICT}")),
    }
    lines.join("\n")
}

/// One round of sharing notes with combined computers: takes in the newer copies the others
/// have, then writes this computer's and lets old deletion markers go. True when a note here
/// changed.
pub fn sync(conn: &Connection, store: &dyn crate::sync::store::Store, listing: &crate::sync::store::Listing, me: &str, seen: &mut crate::sync::Seen, now: i64) -> std::result::Result<bool, String> {
    let err = |e: rusqlite::Error| e.to_string();
    // The best new copy of each note among the others' files.
    let mut best: std::collections::HashMap<String, NoteFile> = std::collections::HashMap::new();
    for (computer, files) in listing.iter().filter(|(c, _)| *c != me) {
        for (name, version) in files {
            if id_of(name).is_none() || seen.has(computer, name, version) {
                continue;
            }
            let Ok(bytes) = store.get(computer, name) else { continue };
            let Ok(copy) = serde_json::from_slice::<NoteFile>(&bytes) else { continue };
            seen.note(computer, name, version);
            if best.get(&copy.id).is_none_or(|b| rank(&copy) > rank(b)) {
                best.insert(copy.id.clone(), copy);
            }
        }
    }
    let mut changed = false;
    for copy in best.into_values() {
        let mine = local(conn, &copy.id).map_err(err)?;
        if mine.as_ref().is_some_and(|(m, _)| rank(m) >= rank(&copy)) {
            continue;
        }
        if let Some((m, synced)) = &mine {
            if m.changed > *synced && !m.deleted && !copy.deleted && m.body != copy.body {
                create(conn, &conflicted(&m.body), now).map_err(err)?;
            }
        }
        conn.execute(
            "INSERT INTO notebook (id, body, created_ms, edited_ms, changed_ms, pinned, deleted, synced_ms, paper) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?5, ?8)
             ON CONFLICT(id) DO UPDATE SET body = ?2, created_ms = ?3, edited_ms = ?4, changed_ms = ?5, pinned = ?6, deleted = ?7, synced_ms = ?5, paper = ?8",
            params![copy.id, copy.body, copy.created, copy.edited, copy.changed, copy.pinned, copy.deleted, copy.paper],
        )
        .map_err(err)?;
        if copy.deleted {
            detach(conn, &copy.id).map_err(err)?;
        }
        changed = true;
    }

    // Write this computer's copies that are new or behind, and let old markers go.
    let mine = listing.get(me).cloned().unwrap_or_default();
    let mut stmt = conn.prepare("SELECT id, written_ms, changed_ms, deleted FROM notebook").map_err(err)?;
    let rows: Vec<(String, i64, i64, bool)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map_err(err)?.collect::<Result<_>>().map_err(err)?;
    let mut kept = std::collections::HashSet::new();
    let mut failed = Ok(());
    for (id, written, changed_ms, deleted) in rows {
        let name = file_name(&id);
        if deleted && changed_ms < now - MARKER_MS {
            if mine.contains_key(&name) {
                let _ = store.delete(&name);
            }
            conn.execute("DELETE FROM notebook WHERE id = ?1", [&id]).map_err(err)?;
            continue;
        }
        kept.insert(name.clone());
        if written == changed_ms && mine.contains_key(&name) {
            continue;
        }
        let Some((copy, _)) = local(conn, &id).map_err(err)? else { continue };
        match store.put(&name, &serde_json::to_vec(&copy).expect("plain data")) {
            Ok(()) => {
                conn.execute("UPDATE notebook SET written_ms = ?2, synced_ms = ?2 WHERE id = ?1", params![id, changed_ms]).map_err(err)?;
            }
            Err(e) => failed = Err(e),
        }
    }
    for name in mine.keys().filter(|n| id_of(n).is_some() && !kept.contains(*n)) {
        let _ = store.delete(name);
    }
    failed.map(|()| changed)
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
    fn paper_is_shared_but_isnt_an_edit() {
        let c = conn();
        let a = create(&c, "A", 1).unwrap();
        assert_eq!(a.paper, "lined");
        set_paper(&c, &a.id, "grid", 9).unwrap();
        let n = get(&c, &a.id).unwrap().unwrap();
        assert_eq!((n.paper.as_str(), n.edited_ms), ("grid", 1));
        let changed: i64 = c.query_row("SELECT changed_ms FROM notebook WHERE id = ?1", [&a.id], |r| r.get(0)).unwrap();
        assert_eq!(changed, 9);
        // One Kairo doesn't know is kept as lined.
        set_paper(&c, &a.id, "parchment", 10).unwrap();
        assert_eq!(get(&c, &a.id).unwrap().unwrap().paper, "lined");
    }

    #[test]
    fn a_file_from_before_paper_reads_as_lined() {
        let old = r#"{"id":"ab","body":"x","created":1,"edited":1,"changed":1,"pinned":false,"deleted":false}"#;
        assert_eq!(serde_json::from_str::<NoteFile>(old).unwrap().paper, "lined");
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
        assert_eq!(remove(&c, &n.id, 5).unwrap(), std::slice::from_ref(&t.id));
        assert!(list(&c, "").unwrap().is_empty());
        assert_eq!(get(&c, &n.id).unwrap(), None);
        let still: Option<String> = c.query_row("SELECT note FROM tasks WHERE id = ?1", [&t.id], |r| r.get(0)).unwrap();
        assert_eq!(still, None);
        let marker: i64 = c.query_row("SELECT deleted FROM notebook WHERE id = ?1", [&n.id], |r| r.get(0)).unwrap();
        assert_eq!(marker, 1);
    }
}
