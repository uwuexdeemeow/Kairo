# Tasks tied to focus

## Goal

A short to-do list on Chrono's Focus page where each task gathers the focus time spent on it.
Starting a focus session on a task, from the page or the palette, counts that session's time for
it. On Windows, macOS and Linux; all data stays on the device.

## On Chrono's Focus page

- A **Tasks** block beside the timer, with an "Add a task" field: type, Enter adds it at the
  bottom. Titles are trimmed and kept to 80 characters (`focus::MAX_LABEL_CHARS`); an empty one
  isn't added.
- Each open task is a row with:
  - a tick box: ticking marks it done;
  - its title: clicking it edits it in place; Enter or leaving the field saves, Esc cancels, and
    an emptied title keeps the old one;
  - its focus time, like "1h 20m", or nothing before any;
  - **Start**: starts a focus session on it, for the length picked on the page's timer;
  - **×**: deletes it, without asking.
- While a session runs for a task, its row says "Focusing now" in place of Start.
- Below the open tasks, a collapsed **Done (n)** section lists finished tasks, newest first, with
  their totals. Unticking one reopens it at the bottom of the open list.
- With no tasks yet, the block shows only the field and a line: "Add what you're working on,
  then start a focus session on it."
- Focus history rows show the session's label as today; a task's sessions are labelled with its
  title.

## In the palette

- **`focus 50 essay`**: when the label matches an open task, the session counts for it and the
  row's second line says `For your task “Essay”` (in place of `Labelled “essay”`). The label
  matches a task when it scores 100 or more with the palette's `fuzzy` matching against the
  task's title (the whole label at the start of a word); the best score wins, ties to the older
  task. No match: a plain label, as today.
- A label that names a block list still picks that list for blocking, as today. A task can match
  the same label; then the list decides what's blocked and the task gets the time, and the
  second line says `Blocks your “Social” list · For your task “Social post”`.
- **`task <title>`** or **`add task <title>`**: a row `Add task “<title>”` in a **Tasks**
  section. Enter adds it (the palette closes, as after any action).
- **`tasks`**: the open tasks as rows in the **Tasks** section, in the page's order, each with
  its focus time as the detail. Enter starts a 25-minute session on it (the palette's usual
  length); Ctrl+Enter (⌘+Enter on a Mac) marks it done, with the footer hint "Mark done".
  Typing more after `tasks ` filters them with `fuzzy`.
- Tasks don't show among the suggestions before anything is typed.

## Data

- New table:

  ```sql
  CREATE TABLE IF NOT EXISTS tasks (
      id         TEXT PRIMARY KEY,   -- random, the same on every combined computer
      title      TEXT NOT NULL,
      created_ms INTEGER NOT NULL,
      done_ms    INTEGER,            -- NULL while open
      position   INTEGER NOT NULL
  );
  ```

- `focus_sessions` gains `task TEXT` (NULL without a task), added the way the database adds
  columns to existing tables.
- A task's id is 16 random hex characters, made when it's added. Ids, not rowids, link sessions
  to tasks, because the list is rewritten when it arrives from another computer.
- A task's time is the sum of its sessions' focused time: from start to end (now for a running
  one) minus time paused, the same reckoning the Focus page's totals use.
- Deleting a task leaves its sessions as they are; a session whose task is gone counts for
  nothing and keeps its label.
- Renaming a task changes its title only; past sessions keep the label they started with.

## Combined computers and backups

- The whole list (ids, titles, created and done times, order) is one shared choice,
  `tasks:all`, as JSON, like `lists:block`: each change stamps it, the latest wins everywhere,
  and a computer seeds it from its own list when combining starts.
- Focus sessions stay per computer, so each computer shows the time from its own sessions.
- Backups carry tasks with the rest of the database; restoring one brings its tasks back.

## How it's built

- **`src-tauri/src/tasks.rs`** (new): the table, `add`, `rename`, `set_done`, `remove`,
  `list` (open in order, then done newest first, each with its focus time), `matching(label)`
  for the palette, and `to_json` / `replace_from_json` for sharing.
- **`focus.rs`**: `start` and `start_by_hand` take an optional task id and store it.
- **`sync.rs`**: seeds and applies `tasks:all`, beside `lists:block`.
- **`commands.rs`**, **`src/api.ts`**: commands for the Focus page (list, add, rename, done,
  remove, start on a task).
- **Palette** (`search.rs`, `mod.rs`): the `Context` carries the open tasks; new actions
  `AddTask(String)`, `StartTask(id, minutes)` and `FinishTask(id)`; `focus_command` rows say
  which task they're for.
- **`src/main.ts`**, **`index.html`**, **`styles.css`**: the Tasks block on the Focus page.

## Testing

- `tasks.rs`: add (trimmed, capped, empty refused), rename (empty keeps the old title), done and
  reopen (reopened goes to the bottom), remove, order, time per task with a pause left out and a
  running session counted to now, a deleted task's sessions counting for nothing, and the JSON
  round trip replacing the list and keeping ids.
- `search.rs`: `focus 50 essay` with an "Essay" task says it's for the task; with none, the
  label as today; with a block list "Social" and a task "Social post", both; `task buy milk`
  offers to add it; `tasks` lists open tasks with Ctrl+Enter "Mark done"; done tasks never show.
- `sync.rs`: `tasks:all` is seeded and applied.
- Vitest: the Tasks block adds on Enter, ticks a task into Done, renames in place (Esc cancels),
  shows "Focusing now" for the running task.
- `docs/device-checklist.md`: a Tasks section.

## Out of scope

Due dates, priorities, subtasks, tasks inside projects, dragging to reorder, reminders, and
counting other computers' sessions.
