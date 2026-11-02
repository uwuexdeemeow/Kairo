# Ledger, part 2: notes

## Goal

Standalone notes in Ledger: one searchable list, Markdown-style writing, a note attachable to any
number of tasks (one note per task), synced to linked computers one file per note. On Windows,
macOS and Linux. Kairo's point is to be as lightweight as possible: no new dependencies, a few
kilobytes of page code, nothing running while Ledger is closed.

## The Notes view

- Ledger's sidebar: Tasks, **Notes**, Settings.
- Left pane: a **Search** field (title and text, as you type) and **+ New note**; then the notes,
  pinned first, then newest edited first. A row: title; "2 tasks" when open tasks are attached;
  when it was edited ("9:14" today, "Yesterday", "3 Nov"). With none: "Write anything: plans,
  lists, things to remember."
- Right pane: the open note.
  - Top bar: "Attached to: Essay draft ↗, …" (each opens that task's panel on the Tasks view),
    **Pin** / **Unpin**, **Delete**, which asks in place first (no system dialog): "Delete
    “<title>”? Tasks attached to it lose the link." with **Delete** and **Keep**.
  - The editor (below).
- A note's title is its first line without leading `#`s and spaces, cut to 80 characters;
  "New note" when that's empty.
- Saved half a second after typing stops, on switching notes, and on closing the window.
- Up to 100,000 characters; past it the text is cut there and "This note is at its limit." shows.
- A note changed on another computer refreshes in place if it's open and not typed in since it
  was shown; otherwise what's on screen stays and the sync's conflict rule decides.

## The editor

A plain `<textarea>` with a highlighted copy of the same text drawn behind it, in the same font
and wrapping, so the textarea keeps native typing, undo, spell check, IME and screen readers. No
library.

- Headings (`#`–`###` at a line's start): bold, accent colour (not larger: the layers must line
  up). `**bold**`, `*italic*` / `_italic_`, `` `code` `` (mono), `[text](url)`: styled, their
  markers shown faint. `- [ ]` / `- [x]`: drawn as a box; clicking inside the brackets ticks or
  unticks it.
- Enter on a list line (`- `, `* `, `1. `, `- [ ] `) continues the list (a checkbox line continues
  unticked); Enter on an empty item ends it.
- Ctrl/⌘+B and Ctrl/⌘+I wrap the selection in `**` / `*`.

## Tasks and notes

- A task's detail panel: **Note** row: the note's title as a link ↗ (opens it) and × (detach), or
  **+ Attach a note**: a small search over notes, with "New note: <task title>" first (a note whose
  first line is `# <task title>`).
- A row whose task has a note shows 📝.
- Deleting a note detaches it from its tasks; deleting a task leaves its note.

## The palette

- `note <text>`: "New note “<text>”" → a note whose first line is the text, opened in Ledger.
- `notes` lists notes (pinned, then newest); `notes <words>` searches them; Enter opens one.
- Any query also offers up to 3 notes whose title or text matches, in a **Notes** section after
  apps, scored below apps and above files. Matching uses the palette's `fuzzy` on titles and a
  plain case-insensitive contains on the first 4,000 characters of text.

## Data

- Table `notebook`: `id TEXT PRIMARY KEY` (16 hex), `body TEXT`, `created_ms`, `edited_ms` (text
  changed), `changed_ms` (anything changed: text, pin, deletion; what sync compares), `pinned`,
  `deleted` (a marker kept so the deletion spreads), `synced_ms` (the `changed_ms` last shared or
  taken in), `written_ms` (the `changed_ms` of the copy in this computer's file).
- `tasks.note TEXT` (a note id or NULL), shared as its own choice `tasknote:<task id>` (value: the
  note id, or empty), so a 1.27.0 computer can't wipe it.
- Module `notebook.rs` (not `notes`: `notes.rs` is the weekly review).

## Sync

Each round, after the shared choices:

1. **Read.** For every other computer, each `note-<id>.json` whose version this computer hasn't
   seen is read: `{id, body, created, edited, changed, pinned, deleted}`. Per id the newest
   `changed` wins (ties: the larger body text, then the computer id, so every computer picks the
   same). It's taken when newer than this computer's `changed_ms`:
   - If this computer changed the note since it last synced it (`changed_ms > synced_ms`), neither
     side deleted it, and the texts differ, this computer's text first becomes a new note whose
     first line has " (conflicted copy)" added.
   - Then the incoming one replaces it, and `changed_ms = synced_ms =` its `changed`; this
     computer's own file is now behind, so step 2 rewrites it.
2. **Write.** Every note (markers included) whose `written_ms` isn't its `changed_ms`, or whose
   file isn't listed, is put as `note-<id>.json`; then `written_ms = synced_ms = changed_ms`.
3. **Purge.** Markers older than 30 days are deleted, row and file. Files of this computer's for
   notes no longer here are deleted.

Every computer holds every note, so one linked later gets them all. A typical 500 notes on 3
computers is about 5 MB of the relay's 50 MB per group. An edit wakes a sync 30 seconds after
typing stops (the page asks), as linking's quick rounds do.

## How it's built

- `notebook.rs`: `Note`, `create`, `save(id, body)` (cap, title, times), `set_pinned`, `remove`
  (marker; detaches tasks), `list(query)`, `get`, `title_of(body)`, and `sync(conn, store,
  listing, me, seen, now) -> Result<bool>`.
- `sync/mod.rs`: `run_with` calls `notebook::sync`; `apply` takes `tasknote:*` (after
  `tasks:all`, like plans).
- `tasks.rs`: `note` on `Task`; `set_note`.
- `commands.rs` / `api.ts`: `notes(query)`, `note(id)`, `new_note(text)`, `save_note(id, body)`,
  `pin_note`, `remove_note`, `set_task_note(task, note)`, `sync_soon()`, `take_ledger_open()`;
  `lib.rs`: `show_ledger_at(app, what)` (`note:<id>` / `task:<id>`) and a `kairo:ledger` event.
- Palette: `Context.notes`, `Action::OpenNote(id)`, `Action::NewNote(text)`.
- Front end: `src/note-editor.ts` (pure `highlight(text) -> html`, list continuation, the
  overlay), `src/ledger-notes.ts` (list and note pane), Ledger page wiring, the task panel's Note
  row.

## Testing

- `notebook.rs`: title rules; the cap; order (pinned, then edited); search over title and text;
  delete leaves a marker that `list` hides and that detaches tasks; sync: a new note reaches the
  other computer; newest wins; an unsynced edit meeting a newer one makes one conflicted copy and
  both computers end with the same notes; a deletion spreads and isn't undone by an older copy; a
  computer linked later gets every note; markers go after 30 days; ties pick the same winner on
  both sides.
- `sync`: `tasknote:*` travels and waits for its task.
- Palette: `note buy list`, `notes`, `notes essay`, a query matching a note's text.
- Vitest: `highlight` (headings, bold, italic, code, links, checkboxes, escaping); list
  continuation; checkbox toggling; the note list's order, search and empty state; delete confirms;
  the task panel's Note row and attach picker; autosave after typing stops.
- `docs/device-checklist.md`: a Notes section, including editing on two linked computers.

## Out of scope

Folders and tags, images and attachments, sharing outside the user's computers, history,
exporting as Markdown files, and markers that hide fully.
