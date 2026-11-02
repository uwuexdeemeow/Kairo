# Ledger, part 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Ledger window holding Kairo's tasks, with due days, three priorities and reminders.

**Architecture:** Tasks keep their table and `tasks:all` sharing; the new plan fields are columns
synced as one `taskplan:<id>` choice per task. Quick-add parsing and reminder selection are pure
Rust functions; a `reminders` thread and a new `ledger` window sit on top.

**Tech Stack:** Rust (Tauri 2, rusqlite, chrono, tauri-plugin-notification, sys-locale), TypeScript
(Vite, Vitest/jsdom).

**Spec:** `docs/superpowers/specs/2026-11-03-ledger-tasks-design.md`

## Global Constraints

- Windows, macOS and Linux; CI builds all three. No platform-only code paths except the Dock rule.
- `tasks:all` JSON stays byte-compatible with 1.26 (no new fields in it).
- Priority: 0 Low, 1 Medium (default), 2 High. Due day: local `YYYY-MM-DD`. Morning default `09:00`.
- Reminder marks: `due:<day>`, `over:<day>`, `at:<ms>`, space-separated in `reminded`.
- In use = `platform::idle_ms() < 120_000`. Late limit 24 h. Thread checks every 60 s, first after 20 s.
- Copy exactly as the spec: "Due today: …", "Overdue: …", "Reminder: …", "… and n more",
  "Nothing due today", "Tasks and notes".

## Review Focus

- A `taskplan:<id>` arriving before its task (via `tasks:all`) must not be lost: it applies once the task exists.
- A 1.26 computer's `tasks:all` replace must not wipe due/priority/marks of tasks that remain.
- Quick add on a title that's only shortcuts (`fri !h`) keeps the text as the title.
- A reminder computed while idle at 09:00 still shows at 09:30 when the user returns, and only once.
- Year roll: `7 jan` typed on 3 Nov 2026 → 2027-01-07; `3 nov` typed on 3 Nov → today.

---

### Task 1: Plan fields on tasks

**Files:** Modify `src-tauri/src/db.rs` (added-columns list), `src-tauri/src/tasks.rs`.

**Interfaces — Produces:**
- `Task` gains `due_day: Option<String>`, `priority: u8`, `remind_ms: Option<i64>`, `reminded: String` (serde camelCase; `#[serde(default)]`, priority default 1), and `group: String` (`"overdue" | "today" | "upcoming" | "none" | "done"`, computed, `#[serde(default, skip_deserializing)]`).
- `pub fn list(conn, now: i64, today: &str) -> Result<Vec<Task>>` sorted: groups in that order; within, priority desc then position; upcoming by due_day first; done by done_ms desc.
- `pub struct Plan { due: Option<String>, priority: u8, remind: Option<i64>, sent: String }` (Serialize/Deserialize).
- `pub fn set_plan(conn, id, due: Option<&str>, priority: u8, remind: Option<i64>) -> Result<()>` — clears `reminded` when due or remind differ.
- `pub fn mark_sent(conn, id, mark: &str) -> Result<()>`, `pub fn plan(conn, id) -> Result<Option<Plan>>`, `pub fn apply_plan(conn, id, &Plan) -> Result<bool>` (false when no such task).
- `to_json` unchanged in shape (no plan fields: serialize a 1.26-shaped struct).

- [ ] Tests: `plan_fields_default_to_medium_with_no_day`, `groups_and_order` (overdue/today/upcoming/none/done; High before Low; upcoming by day), `changing_due_or_remind_clears_marks` (priority change keeps them), `replacing_the_list_keeps_plans_of_tasks_that_stay`, `shared_list_has_no_plan_fields` (to_json lacks `dueDay`).
- [ ] Run `cargo test --lib tasks::` → fail; implement; pass; commit.

### Task 2: Quick add

**Files:** Create `src-tauri/src/quick_add.rs`; add `sys-locale = "0.3"` to `src-tauri/Cargo.toml`.

**Interfaces — Produces:** `pub struct Quick { title: String, due: Option<String>, priority: Option<u8> }`; `pub fn parse(text: &str, today: NaiveDate, day_first: bool) -> Quick`; `pub fn day_first() -> bool` (false only when `sys_locale::get_locale()` starts with `en-US`); `pub fn describe(&Quick, today) -> String` (`"High · Fri 7 Nov"`, parts only when present; priority shown only when given).

- [ ] Tests (today = 2026-11-03, a Tuesday): `essay draft !h fri` → ("essay draft", 2026-11-06, 2); `call "tue"` → title `call tue`, no due; `fri !h` → title `fri !h`, nothing; `tue` on Tue → 2026-11-10; `7 jan` → 2027-01-07; `3 nov` → 2026-11-03; `nov 7` / `7 nov` → 2026-11-07; `11/7` month-first → 2026-11-07, day-first → 2027-07-11; `meet mon tomorrow` → title `meet mon`, due 2026-11-04; `Friday's notes` → unchanged; `!LOW tmr` suffix works case-insensitively; `describe` → `High · Fri 6 Nov`.
- [ ] Fail; implement (tokenize from the end; stop at the first non-shortcut token; quoted tokens never shortcuts and lose their quotes); pass; commit.

### Task 3: Sync of plans and Ledger settings

**Files:** Modify `src-tauri/src/sync/mod.rs` (`seed_choices`, `apply`), `src-tauri/src/tasks.rs` if helpers needed.

**Interfaces — Consumes:** Task 1 `plan`/`apply_plan`. **Produces:** choice kinds `taskplan:<id>` (value = `Plan` JSON) and `ledger:reminders` / `ledger:morning`; `pub fn ledger_settings(conn) -> (bool, String)` and `set_ledger_settings` in a new `src-tauri/src/ledger.rs`, recording the choices.

- [ ] Tests: `task_plans_travel` (desk sets plan, records; laptop applies via `read_choices` path or `apply`); `a_plan_for_a_task_not_here_yet_waits` (`apply` errors → choice not recorded; after `tasks:all` arrives, next round applies); `seeding_covers_existing_plans`; `ledger_settings_travel`.
- [ ] Fail; implement; pass; commit.

### Task 4: Reminders

**Files:** Create `src-tauri/src/reminders.rs`; register `spawn` in `lib.rs` setup.

**Interfaces — Produces:** `pub struct Reminder { title: String, body: String, marks: Vec<(String /*task id*/, String /*mark*/)> }`; `pub fn due_now(conn, now_ms: i64, now_local: NaiveDateTime, morning: NaiveTime, in_use: bool) -> Result<Vec<Reminder>>` (empty when not in use; never marks); `pub fn spawn(app: AppHandle, db_path: PathBuf)`.

- [ ] Tests: `due_today_at_morning_not_before`; `several_due_in_one` ("Due today: A and 2 more", High named first); `overdue_next_morning_once`; `exact_reminder_with_due_body` (body "Due Fri 6 Nov"); `waits_while_idle_then_sends`; `dropped_after_a_day` (exact remind 25 h ago → none, and marked so it never shows); `never_twice` (after marks); `done_tasks_never_remind`; overdue only for the day after (two days late: none).
- [ ] Fail; implement; the thread: reads `ledger_settings`, skips when off, notifies, `mark_sent` each, then records `taskplan:<id>` for each marked task and wakes nothing (next sync carries it); pass; commit.

### Task 5: Commands and the Ledger window

**Files:** Modify `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `vite.config.ts`, `src/api.ts`; create `ledger.html` (shell only).

**Interfaces — Produces (commands → api.ts):** `get_tasks() -> Task[]` (new fields, `group`); `add_task(text)` parses quick-add; `set_task_plan(id, due, priority, remind)`; `parse_task(text) -> { title, due, priority, shown }`; `open_ledger()`; `ledger_settings() -> { reminders, morning }`; `set_ledger_settings(reminders, morning)`. Every task change records `tasks:all` and, for plan changes, `taskplan:<id>`; `remove_task` also deletes its `taskplan:<id>` choice. Window `ledger` (960×680, min 640×480); `show_ledger`; tray "Open Ledger" after "Open Chrono"; `keep_in_dock` includes `ledger`; Destroyed handling like main/dashboard.

- [ ] Test (Rust): `keep_in_dock(&["ledger"])` is true.
- [ ] Implement; `cargo test --lib` and `npx tsc --noEmit` pass; commit.

### Task 6: Ledger page

**Files:** Create `src/ledger.ts`, `src/ledger-tasks.ts`, `src/ledger-tasks.test.ts`; fill `ledger.html`; styles in `src/styles.css`.

**Interfaces — Consumes:** Task 5 api. **Produces:** `renderTaskList(tasks: Task[], act: LedgerActions, today: string): HTMLElement` and `renderDetail(task, act)`.

- [ ] Vitest: groups render with headings, empty ones absent; Overdue heading has class `overdue`; High row has `.prio-high` dot; bell when `remindMs`; tick calls `act.done`; title click opens detail; detail priority button calls `act.plan`; clearing due calls `act.plan` with `null`; quick-add preview shows `parse_task(...).shown`; Enter adds; empty state copy exact.
- [ ] Implement; `npx vitest run` passes; commit.

### Task 7: Chrono picker, dashboard tile, palette

**Files:** Modify `src/main.ts`, `index.html` (Focus page task block → picker + "Open Ledger"), delete `src/tasks.ts` / `src/tasks.test.ts` usage, `src/dashboard.ts` (+ test), `src-tauri/src/palette/search.rs`, `src-tauri/src/palette/mod.rs`.

**Interfaces — Consumes:** Tasks 1, 2, 5.

- [ ] Vitest: picker lists open tasks in Ledger order and starts a session on the chosen one; tile line "3 due today · 1 overdue" / "Nothing due today".
- [ ] Rust: `task essay !h fri` row title `Add task “essay”` with detail `High · Fri 6 Nov` and action `AddTask("essay !h fri")` (the command parses); `tasks` rows in Ledger order with due detail; `ledger` → `Action::OpenLedger`.
- [ ] Implement; all tests pass; commit.

### Task 8: Checklist and README

**Files:** `docs/device-checklist.md` (Ledger section: open from tile/tray/palette; quick add; groups; a due-today reminder at the morning time and an exact one, on Windows and a Mac; linked computers share due/priority), `README.md` (Ledger paragraph).

- [ ] Write; commit.
