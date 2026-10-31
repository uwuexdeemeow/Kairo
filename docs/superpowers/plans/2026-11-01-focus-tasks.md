# Tasks Tied to Focus Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A to-do list on Chrono's Focus page whose tasks gather the focus time of sessions started on them, from the page or the palette.

**Architecture:** A new `tasks.rs` owns the `tasks` table and its JSON for combined computers; `focus_sessions.task` links a session to a task by its random text id. Focus starts take an optional task. The palette gets the open tasks in its `Context` and three actions; the Focus page draws a Tasks block from a new, separately tested `src/tasks.ts`.

**Tech Stack:** Rust (Tauri 2, rusqlite), TypeScript + Vitest, no framework.

**Spec:** `docs/superpowers/specs/2026-11-01-focus-tasks-design.md`

## Global Constraints

- Titles: trimmed, at most `focus::MAX_LABEL_CHARS` (80) characters; empty never stored.
- Ids: 16 random hex characters from `sync::new_id` (made `pub(crate)`).
- Order: open tasks by `position` ascending; done tasks by `done_ms` descending.
- Focused time: the sum of `focus::Block::ms` over a task's sessions, a running one counted to now.
- Shared choice item: `tasks:all`, recorded with `sync::record` after every change.
- Copy, exactly: "Tasks", "Add a task", "Focusing now", "Done (n)", "Add what you're working on, then start a focus session on it.", `For your task “<title>”`, `Add task “<title>”`, "Mark done".
- Palette default length for a task row: 25 minutes.
- All three platforms; no platform-specific code in this plan.

## Review Focus

- A task deleted while its session runs: the running session keeps going, and its time counts for nothing; the Focus page must not show "Focusing now" on another row or fail to render.
- A shared `tasks:all` arriving with a task id this computer's sessions point at but a new title: the time stays with that id, the title changes.
- A label that matches two tasks equally (`focus 25 report` with "Report A" and "Report B"): the older task wins, every time.
- Renaming a task to text with quotes or HTML (`<b>"x"</b>`): shown as text on the page and in the palette, never as markup.
- A huge list (200 tasks): the palette's `tasks` rows stay capped at the palette's usual row limit, and the page renders them all.

---

### Task 1: The tasks store and time per task

**Files:**
- Create: `src-tauri/src/tasks.rs` (tests inline at the end)
- Modify: `src-tauri/src/db.rs` (the `tasks` table in the schema batch; `("focus_sessions", "task", "TEXT")` in the added-columns list, with a comment like its neighbours)
- Modify: `src-tauri/src/sync.rs` (`fn new_id` → `pub(crate) fn new_id`)
- Modify: `src-tauri/src/lib.rs` (`mod tasks;`)

**Interfaces:**
- Produces:
  - `#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)] #[serde(rename_all = "camelCase")] pub struct Task { pub id: String, pub title: String, pub created_ms: i64, pub done_ms: Option<i64>, pub position: i64, #[serde(default)] pub focused_ms: i64 }`
  - `pub fn add(conn: &Connection, title: &str, now: i64) -> Result<Option<Task>>` — None for an empty title; position = max open position + 1.
  - `pub fn rename(conn: &Connection, id: &str, title: &str) -> Result<()>` — an empty title changes nothing.
  - `pub fn set_done(conn: &Connection, id: &str, done: bool, now: i64) -> Result<()>` — reopening moves it to the bottom of the open list.
  - `pub fn remove(conn: &Connection, id: &str) -> Result<()>`
  - `pub fn list(conn: &Connection, now: i64) -> Result<Vec<Task>>` — open then done, each with `focused_ms`.
  - `pub fn title(conn: &Connection, id: &str) -> Result<Option<String>>`
  - `pub fn to_json(conn: &Connection) -> Result<String>` (without `focused_ms`) and `pub fn replace_from_json(conn: &Connection, json: &str) -> std::result::Result<(), String>` — same shapes as `lists::to_json` / `lists::replace_from_json`.

- [ ] **Step 1: Write the failing tests** in `tasks.rs`, using an in-memory database opened the way other modules' tests open one (`db::open_in_memory()` or the helper `lists.rs` tests use):

```rust
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
```

(`M` is 60 000; `conn()` opens a migrated in-memory database.)

- [ ] **Step 2: Run them to see them fail**

Run: `cd src-tauri && cargo test --lib tasks::`
Expected: compile errors — `add`, `list` and the rest not found.

- [ ] **Step 3: Implement** the table, the column and `tasks.rs`. `list` reads tasks, then one query `SELECT task, start_ms, COALESCE(end_ms, ?now), paused_ms + COALESCE(?now - paused_at, 0) FROM focus_sessions WHERE task IS NOT NULL` summed per id with the same arithmetic as `Block::ms`. `replace_from_json` runs in a transaction: delete all, insert each.

- [ ] **Step 4: Run the tests to see them pass**

Run: `cd src-tauri && cargo test --lib tasks::`
Expected: 5 passed.

- [ ] **Step 5: Commit** — `git commit -m "Keep tasks, and count each one's focus time"`

### Task 2: Starting focus on a task, sharing the list, and the commands

**Files:**
- Modify: `src-tauri/src/focus.rs` (`Active` gains `task: Option<String>`; `start`, `start_by_hand`, `start_from_slot`; the active-session query)
- Modify: `src-tauri/src/sync.rs` (seed `tasks:all` beside `lists:block`; apply `"tasks" if key == "all"`)
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs` (register commands)
- Modify: `src/api.ts` (types and calls)

**Interfaces:**
- Consumes: Task 1's functions and `Task`.
- Produces:
  - `pub fn start(conn, minutes: u32, label: Option<&str>, task: Option<&str>, now: i64) -> Result<Active>`; `pub fn start_by_hand(conn, minutes: u32, label: Option<&str>, task: Option<&str>, now: i64) -> Result<Active>` — the block list is still chosen from `label`; when `task` names an existing task, the stored label is its title and `task` is stored. `start_from_slot` passes no task.
  - `Active::task(&self) -> Option<&str>`; serialised as `task`.
  - Commands: `get_tasks() -> Vec<Task>`, `add_task(title)`, `rename_task(id, title)`, `set_task_done(id, done)`, `remove_task(id)`, each returning the new `Vec<Task>` and recording `tasks:all`; `start_focus(minutes, label, task: Option<String>)`.
  - `api.ts`: `type Task = { id: string; title: string; createdMs: number; doneMs: number | null; position: number; focusedMs: number }`; `FocusSession` gains `task: string | null`; `tasks()`, `addTask(title)`, `renameTask(id, title)`, `setTaskDone(id, done)`, `removeTask(id)`, `startFocus(minutes, label?, task?)`.

- [ ] **Step 1: Write the failing tests**
  - `focus.rs`: `starting_on_a_task_labels_it_with_the_title_and_keeps_the_typed_label_for_lists` — with a block list "Social" and a task "Social post", `start_by_hand(&c, 25, Some("social"), Some(&id), now)` stores label "Social post", task = id, and list "Social"; with an unknown task id, the label stays "social" and task is NULL.
  - `sync.rs`: `tasks_are_seeded_and_applied_like_lists` — seeding with one task writes `tasks:all`; applying a `tasks:all` value replaces the list.

- [ ] **Step 2: Run them to see them fail**

Run: `cd src-tauri && cargo test --lib focus:: sync::`
Expected: compile errors (`start_by_hand` takes 4 arguments) then, once compiling, the new tests fail.

- [ ] **Step 3: Implement** the signatures above and update every caller (`commands.rs`, `palette/mod.rs`'s `Action::StartFocus` arm passes `None` for now, `schedule.rs`, tests). Register the five commands in `lib.rs`'s handler list. Add the `api.ts` types and calls.

- [ ] **Step 4: Run all tests**

Run: `cd src-tauri && cargo test` and `npx tsc --noEmit`
Expected: all pass; no type errors.

- [ ] **Step 5: Commit** — `git commit -m "Start focus on a task, and share the task list between computers"`

### Task 3: Tasks in the palette

**Files:**
- Modify: `src-tauri/src/palette/search.rs` (`Context.tasks`, actions, `focus_command` rows, `tasks` and `task <title>` rows, tests)
- Modify: `src-tauri/src/palette/mod.rs` (fill `Context.tasks` with open tasks; run the new actions)

**Interfaces:**
- Consumes: `tasks::list`, `tasks::add`, `tasks::set_done`, `focus::start_by_hand` with a task (Task 2).
- Produces:
  - `Context.tasks: Vec<crate::tasks::Task>` — open tasks only, in order.
  - `Action::StartFocus(u32, Option<String>, Option<String>)` — minutes, label, task id (every existing `StartFocus(m, l)` gains `, None`).
  - `Action::AddTask(String)`, `Action::FinishTask(String)`.
  - `fn task_for<'a>(tasks: &'a [Task], label: &str) -> Option<&'a Task>` — best `fuzzy(label, &t.title)` with score ≥ 100; ties to the earlier (older) one.
  - Section name `"Tasks"`, glyph `Glyph::Focus` for task rows.

- [ ] **Step 1: Write the failing tests** in `search.rs`, with `ctx()` extended by `tasks: Vec::new()` and a helper `task(id, title)`:
  - `focus_on_a_task_counts_for_it`: tasks ["Essay"]; `search(&c, "focus 50 essay")[0]` has action `StartFocus(50, Some("essay".into()), Some("t1".into()))` and sub `For your task “Essay”`.
  - `a_label_naming_a_list_and_a_task_says_both`: block list "Social", task "Social post"; sub is `Blocks your “Social” list · For your task “Social post”`.
  - `an_unmatched_label_stays_a_label`: no tasks; `focus 50 essay` → `StartFocus(50, Some("essay".into()), None)`, sub `Labelled “essay”`.
  - `equal_matches_go_to_the_older_task`: tasks ["Report A" (t1), "Report B" (t2)]; `focus 25 report` → task `t1`.
  - `tasks_lists_open_ones_and_typing_adds_one`: `search(&c, "tasks")` rows in section "Tasks" titled by task, action `StartFocus(25, None, Some(id))`, second `(FinishTask(id), "Mark done")`; `search(&c, "task buy milk")` and `"add task buy milk"` contain `Add task “buy milk”` with `AddTask("buy milk")`; `tasks ess` keeps only "Essay".
  - `tasks_rows_are_capped`: 20 open tasks give 8 rows for `tasks`.

- [ ] **Step 2: Run them to see them fail**

Run: `cd src-tauri && cargo test --lib palette::`
Expected: compile errors (`Context.tasks`, new actions).

- [ ] **Step 3: Implement.** In `focus_command`'s row: compute `task_for`; the sub joins the list part and the task part with ` · `. `tasks` rows: up to the palette's usual row cap (`MAX_APPS`-sized lists use their own constants; use 8). `task `/`add task ` rows score 1000. `mod.rs` runs `AddTask` via `tasks::add` and records `tasks:all`; `FinishTask` via `tasks::set_done(.., true, ..)` and records; `StartFocus(m, l, t)` passes `t` to `start_by_hand`. Fill `Context.tasks` with the open tasks from `tasks::list`.

- [ ] **Step 4: Run all tests**

Run: `cd src-tauri && cargo test`
Expected: all pass.

- [ ] **Step 5: Commit** — `git commit -m "Start, add and finish tasks from the palette"`

### Task 4: The Tasks block on the Focus page

**Files:**
- Create: `src/tasks.ts` and `src/tasks.test.ts`
- Modify: `index.html` (an empty `<section class="block" aria-label="Tasks" id="focus-tasks"></section>` after `#focus-stats`), `src/main.ts` (draw it with the Focus page and after each change), `src/styles.css`

**Interfaces:**
- Consumes: `api.tasks/addTask/renameTask/setTaskDone/removeTask/startFocus`, `FocusSession.task` (Task 2).
- Produces: `export function renderTasks(root: HTMLElement, tasks: Task[], runningTask: string | null, act: TaskActions): void`, with `export type TaskActions = { add(title: string): void; rename(id: string, title: string): void; done(id: string, done: boolean): void; remove(id: string): void; start(id: string): void }`. Built with `ui.ts`'s `h()` only (text, never `innerHTML`).

- [ ] **Step 1: Write the failing tests** in `src/tasks.test.ts` (jsdom, as the other tests):
  - `adds on Enter and ignores blank`: typing "Essay" + Enter in the "Add a task" field calls `act.add("Essay")`; "  " + Enter calls nothing.
  - `ticks a task into Done`: the open row's checkbox calls `act.done(id, true)`; a done task sits under a collapsed "Done (1)" and its checkbox calls `act.done(id, false)`.
  - `renames in place, Esc cancels`: clicking the title shows an input; Enter calls `act.rename(id, "New")`; Esc calls nothing and shows the old title.
  - `shows Focusing now for the running task`: with `runningTask = id`, that row shows "Focusing now" and no Start button; others show Start, which calls `act.start(id)`.
  - `survives a running task that was deleted`: `runningTask` naming no listed task renders every row with Start and no "Focusing now".
  - `shows the empty line`: no tasks → "Add what you're working on, then start a focus session on it."
  - `shows titles as text`: a title `<b>"x"</b>` appears literally in `textContent`, and no `b` element exists.
  - `shows focus time`: `focusedMs: 80 * 60000` shows "1h 20m"; `0` shows nothing.

- [ ] **Step 2: Run them to see them fail**

Run: `npx vitest run src/tasks.test.ts`
Expected: FAIL — `./tasks` has no `renderTasks`.

- [ ] **Step 3: Implement** `renderTasks` (heading "Tasks", the field, open rows, the Done `<details>`), and in `main.ts` call it from the Focus page's refresh with `api.tasks()` and the running session's `task`, each action calling the API then redrawing; Start uses the page's picked minutes (the timer's value) and the task id. Styles follow the page's existing `.block` rows.

- [ ] **Step 4: Run the tests to see them pass**

Run: `npm test` and `npx tsc --noEmit`
Expected: all pass; no type errors.

- [ ] **Step 5: Commit** — `git commit -m "Add the Tasks block to Chrono's Focus page"`

### Task 5: Documentation

**Files:**
- Modify: `docs/device-checklist.md` (a "Tasks" section), `README.md` (one sentence on tasks in the Focus sessions bullet)

- [ ] **Step 1: Write the checklist section**, for all three systems: add "Essay" on the Focus page; Start it and see "Focusing now"; end the session and see its time on the row; tick it into Done (1) and untick it back; rename it, then Esc a rename; in the palette `focus 25 ess` says `For your task “Essay”`; `tasks` lists it and Ctrl+Enter marks it done; `task buy milk` adds a task; with two combined computers, a task added on one appears on the other within about 10 minutes.

- [ ] **Step 2: Commit** — `git commit -m "Add tasks to the README and the device checklist"`
