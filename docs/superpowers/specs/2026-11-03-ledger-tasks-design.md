# Ledger, part 1: tasks with due dates, priorities and reminders

## Goal

A new Kairo app, **Ledger**, for tasks and (in part 2) notes. Part 1 moves tasks out of Chrono's
Focus page into Ledger's own window, and gives them a due day, one of three priorities and
reminders. Tasks keep gathering the focus time of the sessions started on them. On Windows, macOS
and Linux; all data stays on the device and the user's linked computers.

Part 2 (its own spec) adds standalone Markdown notes in one searchable list, which a task can link
to, synced one file per note. Its module can't be called `notes`: `notes.rs` holds the weekly
review notes.

## Opening Ledger

- A **Ledger** tile on the dashboard's Home, after Chrono: "Tasks and notes". Its line:
  "3 due today · 1 overdue", "1 overdue", "2 due today", or "Nothing due today".
- The tray menu: **Open Ledger**, after Open Chrono.
- The palette: `ledger` offers "Open Ledger".
- Its own window (label `ledger`, `ledger.html`, 960×680, min 640×480), made when opened and
  dropped when closed like Chrono's and the dashboard's, in the same theme. While it's open Kairo
  is in the Dock on a Mac.

## The window

- A sidebar with **Tasks** (Notes arrives in part 2, hidden until then) and **Settings**.
- The Tasks view: an "Add a task" field at the top, then the open tasks in groups:
  **Overdue** (red heading), **Today**, **Upcoming** (each row shows its day, "Fri 7 Nov"),
  **No date**; then a collapsed **Done (n)**, newest first. Empty groups don't show. Within a
  group: High, then Medium, then Low; then by position (the order added). Upcoming sorts by day
  first.
- A row: tick box; priority dot (red High, amber Medium, none for Low); title; due day (in the
  Overdue and Upcoming groups, and "Today" isn't repeated); a bell when it has an exact reminder;
  focus time ("1h 20m", nothing before any); **Start focus** (Chrono's default session length;
  "Focusing now" while a session runs for it); ticking moves it to Done, unticking reopens it.
- Clicking a row's title opens a **detail panel** on the right: title (Enter or leaving saves, an
  emptied title keeps the old one), due day (a date field and a × to clear), priority (three
  buttons), exact reminder (date and time field and ×), its focus time, and **Delete**. Esc or ×
  closes it.
- With no tasks: the field and "Add what you're working on. Give it a day with “fri” or a
  priority with “!h”."
- **Settings**: **Reminders** on or off (on), and **Morning reminder at** (09:00). Both are shared
  choices.

## Quick add

The "Add a task" field and the palette's `task` / `add task` read shortcuts as whole words at the
end of the text, in any order, and take them out of the title:

- Priority: `!h` / `!high`, `!m` / `!med` / `!medium`, `!l` / `!low` (any case). None: Medium.
- Due day: `today`, `tomorrow` / `tmr`, a weekday (`mon`…`sun`, or in full; the next one after
  today, so `fri` on a Friday means next Friday), `7 nov` / `nov 7` (month names of 3+ letters), or
  `11/7` (month/day when the system's locale is US English, day/month otherwise; the page and
  the palette both ask Rust, which reads the locale once with the `sys-locale` crate). A day already past this
  year means next year. Only the last date word counts; earlier ones stay in the title.
- A word in double quotes is never read as a shortcut: `call "tue"` is a task "call tue".
- If nothing is left for the title, the text is taken as typed.
- As you type, the field shows what it read below it: `High · Fri 7 Nov`.

Parsing lives in Rust (`tasks::parse_quick`), so the palette and the page agree; the page asks
for it through a command while typing.

## Reminders

A `reminders` thread checks once a minute (first 20 seconds after start).

- **Due:** on a task's due day, at or after the morning time: "Due today: Essay draft". Several due
  the same day: "Due today: Essay draft and 2 more" (the highest priority named).
- **Overdue:** the day after the due day, at or after the morning time, once: "Overdue: Essay
  draft" (or "… and n more").
- **Exact:** at or after its time: "Reminder: Essay draft", with "Due Fri 7 Nov" as the body when it
  has a due day.
- A reminder shows only while this computer is in use (input in the last 2 minutes); otherwise it
  waits. One more than 24 hours late is dropped without showing.
- Each sent reminder is marked on its task (`due:<day>`, `over:<day>`, `at:<ms>`), and the mark
  travels with the task's plan, so a linked computer doesn't send it again. Two computers in use in
  the same sync gap may both show it.
- Changing a task's due day or reminder time clears its marks. Done tasks never remind. With
  Reminders off, nothing is sent and nothing is marked.
- Clicking a notification does nothing more than the system's default: Kairo's notification
  plugin reports clicks only on mobile. Notifications turned off in the system: nothing breaks;
  the tile and the list still show what's due.

## Data

- `tasks` gains, through `db.rs`'s added-columns list:
  `due_day TEXT` (local "2026-11-07", NULL for none), `priority INTEGER NOT NULL DEFAULT 1`
  (0 Low, 1 Medium, 2 High), `remind_ms INTEGER` (NULL for none), `reminded TEXT NOT NULL DEFAULT ''`
  (sent marks, space-separated).
- Shared choices: `ledger:reminders` ("1"/"0") and `ledger:morning` ("09:00").

## Combined computers

- `tasks:all` stays as it is (ids, titles, created and done times, order), so a 1.26 computer keeps
  sharing the list.
- Each task's plan is its own shared choice, `taskplan:<id>`, JSON `{due, priority, remind, sent}`:
  stamped on each change, latest wins, seeded for every task when combining starts or the shared
  choices are first seeded. A 1.26 computer can't apply that kind and leaves it waiting.
- Taking another computer's `tasks:all` keeps the plan columns of tasks that stay; a plan whose
  task isn't here yet waits like any choice that can't apply, and applies once the task arrives.
  Deleting a task drops its plan choice.

## How it's built

- **`tasks.rs`**: the new fields on `Task`; `set_plan` (due, priority, remind; clears marks when
  due or remind change), `mark_sent`; `list` sorted as above with a `group`; `plan_json` /
  `apply_plan`; `parse_quick(text, today, day_first) -> Quick { title, due, priority }`.
- **`reminders.rs`** (new): `due_now(conn, now, local_today, morning, in_use) -> Vec<Reminder>`,
  pure over its inputs, and `spawn` (the thread: settings, idle from `platform::idle_ms`,
  notification through `tauri-plugin-notification`, marks, sync).
- **`sync/mod.rs`**: seed and apply `taskplan:*` and `ledger:*`.
- **`lib.rs`**: the `ledger` window, `show_ledger(app)`, the tray item, the Dock rule.
- **`commands.rs`**, **`src/api.ts`**: `get_tasks` returns the new fields; `set_task_plan`,
  `parse_task`, `open_ledger`, `ledger_settings` / `set_ledger_settings`;
  `add_task` takes the quick-add text.
- **Palette**: `task` uses `parse_quick` and its row says what it read; `tasks` sorts like Ledger
  and shows the due day as the detail; `ledger` → `Action::OpenLedger`.
- **Front end**: `ledger.html`, `src/ledger.ts` (page), `src/ledger-tasks.ts` (list and detail
  panel, replacing `src/tasks.ts`'s block); Chrono's Focus page gets a "Focus on a task" picker
  and an "Open Ledger" link; the dashboard tile; `vite.config.ts` and the capability list gain
  `ledger`.

## Testing

- `tasks.rs`: plan fields and defaults; grouping and order; `parse_quick` (each shortcut, quotes,
  past dates roll to next year, only the last date counts, a title of only shortcuts, day-first and
  month-first); marks cleared when due or remind change; `tasks:all` replace keeps plans.
- `reminders.rs`: due at morning, not before; several in one; overdue next morning, once; exact;
  waits while idle; dropped after 24 h; never twice; reset after a change; never for done tasks;
  off sends nothing.
- `sync`: `taskplan:*` seeded and applied; one for a missing task waits then applies.
- Palette: `task essay !h fri` reads and shows it; `tasks` order and detail; `ledger`.
- Vitest: Ledger's groups, detail panel edits, quick-add preview, Chrono's picker, the tile's line.
- `docs/device-checklist.md`: a Ledger section, with a reminder on Windows and a Mac.

## Out of scope

Notes (part 2), opening a task from its notification, repeating tasks, subtasks, dragging to reorder, snoozing, calendar sync, and
counting other computers' focus time.
