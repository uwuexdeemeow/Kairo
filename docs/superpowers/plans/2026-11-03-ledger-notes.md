# Ledger, part 2 (notes) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Standalone Markdown notes in Ledger, attachable to tasks, synced one file per note.

**Architecture:** A `notebook` table and module own notes and their sync (read others' newer
copies, make a conflicted copy for an unsynced local edit, write own copies, purge old markers),
called from the sync round. The page draws a textarea over a highlighted copy of its text.

**Tech Stack:** Rust (rusqlite, serde_json), TypeScript (Vitest/jsdom). No new dependencies.

**Spec:** `docs/superpowers/specs/2026-11-03-ledger-notes-design.md`

## Global Constraints

- No new npm or cargo dependencies; the editor is hand-made (project goal: as lightweight as possible).
- Note cap 100,000 characters; title 80 characters, "New note" when empty.
- Note file name `note-<id>.json`; JSON fields `id, body, created, edited, changed, pinned, deleted`.
- Markers purged after 30 days (`30 * DAY_MS`).
- Task link choice `tasknote:<task id>`, applied after `tasks:all`.
- Copy exactly: "Write anything: plans, lists, things to remember.", "This note is at its limit.",
  "Delete “<title>”? Tasks attached to it lose the link.", " (conflicted copy)", "New note".

## Review Focus

- Two computers editing the same note between syncs end with identical note sets (one copy each side, not two).
- An older copy on a third computer never resurrects a deleted note.
- Typing while a sync takes in a newer version: what's on screen isn't replaced mid-typing.
- A note at the cap still saves edits that shorten it.
- The highlight layer escapes `<`, `&` so note text can't inject markup.

---

### Task 1: The notebook

**Files:** Create `src-tauri/src/notebook.rs`; modify `src-tauri/src/db.rs` (table; `tasks.note` column), `src-tauri/src/lib.rs` (mod).

**Produces:** `pub struct Note { id, title, body, created_ms, edited_ms, pinned: bool, tasks: Vec<(String, String)> /* open tasks attached: id, title */ }` (camelCase serde); `create(conn, text, now) -> Result<Note>`; `save(conn, id, body, now) -> Result<Option<Note>>` (None when gone; body capped); `set_pinned(conn, id, pinned, now)`; `remove(conn, id, now)` (marker; `UPDATE tasks SET note = NULL WHERE note = id`, returns detached task ids); `list(conn, query: &str) -> Result<Vec<Note>>` (bodies included); `get(conn, id) -> Result<Option<Note>>`; `title_of(body) -> String`; `pub const MAX_CHARS: usize = 100_000`.

- [ ] Tests: `the_title_is_the_first_line_without_hashes`, `a_note_is_kept_to_its_limit`, `pinned_first_then_newest_edited`, `search_finds_title_and_text_any_case`, `deleting_leaves_a_hidden_marker_and_detaches_tasks`, `pinning_doesnt_reorder_by_edit`.
- [ ] Run → fail; implement; pass; commit.

### Task 2: Notes sync

**Files:** `src-tauri/src/notebook.rs` (`sync`), `src-tauri/src/sync/mod.rs` (call from `run_with`; `tasknote:` in `apply` and the apply order), `src-tauri/src/tasks.rs` (`note` field, `set_note`).

**Produces:** `pub fn sync(conn, store: &dyn Store, listing: &Listing, me: &str, seen: &mut Seen-like, now_ms) -> Result<bool>`; `Seen` gets `pub(crate)` `has`/`note` for notebook to use.

- [ ] Tests (two `Computer`s on a `MemStore`, as `sharing()`): `a_new_note_reaches_the_other_computer`, `the_newest_change_wins`, `an_unsynced_edit_meeting_a_newer_one_becomes_a_conflicted_copy` (both sides then hold the same two notes), `a_deletion_spreads_and_stays`, `a_computer_linked_later_gets_every_note`, `markers_go_after_thirty_days`, `equal_changes_pick_the_same_winner`, `task_notes_travel_and_wait_for_their_task`.
- [ ] Run → fail; implement; pass; commit.

### Task 3: Commands and opening Ledger on a note or task

**Files:** `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs` (`AppState.ledger_open`, `show_ledger_at`, register), `src/api.ts`.

**Produces (api.ts):** `notes(query)`, `note(id)`, `newNote(text) -> Note`, `saveNote(id, body) -> Note | null`, `pinNote(id, pinned)`, `removeNote(id)`, `setTaskNote(task, note | null) -> Task[]`, `syncSoon()`, `takeLedgerOpen() -> string | null` (`"note:<id>"` / `"task:<id>"`); event `kairo:ledger`; `Task.note: string | null`.

- [ ] Rust test: `ledger_open` round trip is covered by Vitest later; here `cargo test`, `tsc` pass; commit.

### Task 4: The editor

**Files:** Create `src/note-editor.ts`, `src/note-editor.test.ts`; styles.

**Produces:** `highlight(text: string): string`; `continueList(text, caret) -> { text, caret } | null`; `toggleBoxAt(text, caret) -> string | null`; `wrapSelection(text, start, end, mark) -> { text, start, end }`; `mountEditor(root, text, onChange(text)) -> { set(text), value(), focus() }`.

- [ ] Vitest: headings/bold/italic/code/link/checkbox classes; `<b>&` escaped; list continuation for `- `, `1. ` (→ `2. `), `- [x] ` (→ `- [ ] `), empty item ends; box toggling only inside brackets; Ctrl+B wraps.
- [ ] Run → fail; implement; pass; commit.

### Task 5: The Notes view and the task panel's Note row

**Files:** Create `src/ledger-notes.ts`, `src/ledger-notes.test.ts`; modify `ledger.html`, `src/ledger.ts`, `src/ledger-tasks.ts` (+ test), styles.

- [ ] Vitest: list order and rows ("2 tasks", time), empty copy, search calls `notes(query)`; new note; delete asks in place then removes; Keep cancels; attached tasks link; autosave 500 ms after typing; `syncSoon` 30 s after; task panel Note row (link, ×, attach picker with "New note: <title>"); 📝 on rows with a note.
- [ ] Run → fail; implement; pass; commit.

### Task 6: Palette

**Files:** `src-tauri/src/palette/search.rs`, `src-tauri/src/palette/mod.rs`.

- [ ] Tests: `note buy list` → `NewNote("buy list")`; `notes` lists pinned then newest; `notes essay` filters; a query matching only a note's text offers it under "Notes", below an app match; at most 3.
- [ ] Run → fail; implement; pass; commit.

### Task 7: Docs

`docs/device-checklist.md` (Notes section), `README.md` (Ledger paragraph mentions notes). Commit.
