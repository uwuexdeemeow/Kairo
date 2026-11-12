# Ledger, part 4: repeating tasks, steps and the Done log

## Goal

Make Ledger something to open every day: tasks that come back on their own, big tasks broken into
steps, and a place that shows what got done. On Windows, macOS and Linux, synced to linked
computers. Kairo's point is to be as lightweight as possible: no new dependencies, a few
kilobytes of page code, nothing running while Ledger is closed.

## 1. Repeating tasks

- A task repeats **every day**, **every weekday** (Mon–Fri), **every week** (on its due day's
  weekday), **every month** (on its due day's date; a 31st becomes the month's last day) or
  **every year**. One rule per task; none is the default.
- Set in the task panel next to the due day: a **Repeat** choice, "Never" first. Choosing one on a
  task with no due day gives it today.
- Quick add reads it off the end like the due day: `water plants every day`, `report every fri`
  (due the next Friday, repeating weekly). Only `every <day|weekday|week|month|year|mon…sun>` is
  read, at the end like the other shortcuts; a word in double quotes is never a shortcut.
  The palette's `task` command reads the same words (`quick_add.rs`).
- Known limit: the next date comes from the one just ticked, so a monthly task on the 31st falls
  on 28 Feb and stays on the 28th from then on.
- **Ticking a repeating task** marks it done as usual (so it shows in the Done log) and adds the
  next one: same title, priority, reminder time of day, attached note, repeat rule, steps
  unticked; due on the first matching day after the later of its due day and today, so ticking
  early skips nothing and ticking late doesn't leave one overdue. Unticking the done one removes
  the copy it made if that copy is still untouched (open, never started, not edited).
- The next task's id is made from the finished one's (`<id>~<n>`), so two combined computers
  ticking the same task make the *same* new task, not two.
- Reminders and the morning summary treat the new task like any other.

## 2. Steps

- A task holds up to 30 steps of up to 120 characters: a checklist inside its panel, under the
  plan, above the attached note. Click to tick, Enter adds the next, Backspace on an empty step
  removes it, Alt+↑/↓ to move (no drag handle: the keyboard way costs nothing and is enough).
- The task's row shows "2/5" after the title while any step is open or ticked; the task can be
  ticked with steps left (steps never block it).
- Pasting several lines into the new-step field makes one step per line (leading `- `, `* `,
  `- [ ]` removed).
- Stored as text in `tasks.steps` (JSON `[{"t":"…","d":false}]`).

## 3. The Done log

- Ledger's sidebar: Tasks, Notes, **Done**, Settings. Done shows the tasks ticked off, newest
  first, under their day ("Today", "Yesterday", "Mon 9 Nov", the year after a year).
- Top: **This week: 12 done · 6 h 20 min focused**, and "last week 9 · 5 h 05 min" beside it
  (weeks start on Monday). Focus time counts the sessions on those tasks on this
  computer, as the task rows already do.
- A row: title, the focus time on it, the time it was done, **Reopen** (puts it back at the
  bottom of the open ones). Delete is in the row's menu, with the same Undo as tasks.
- Shows the last 4 weeks; **Show older** adds 4 more each press. A **Find** field filters by title.
- With none: "Nothing done yet. Tick a task off and it lands here."
- Reads the done tasks the list already has; no new store, nothing computed while Ledger is closed.

## Sharing with combined computers

The list, `taskplan:<id>` and `tasknote:<id>` are left exactly as they are, so a Kairo from before
this can't wipe what it doesn't know. Repeat and steps travel as one new shared choice per task,
**`taskmore:<id>`** = `{"repeat": "weekly" | null, "steps": [...]}`, latest change wins like the
plan. A computer without it ignores it. Sharing at the start of combining includes tasks with a
repeat or steps; backup and restore carry both columns.

## Where it lives

- `src-tauri/src/db.rs`: columns `tasks.repeat TEXT`, `tasks.steps TEXT NOT NULL DEFAULT ''`.
- `src-tauri/src/tasks.rs`: `Task` gets `repeat`, `steps`; `set_repeat`, `set_steps`,
  `next_due(rule, due, today)`, `complete` (done + next); `more_json` / `apply_more` / `with_more`
  beside the plan's.
- `src-tauri/src/quick_add.rs`: reads `every …`.
- `src/ledger-tasks.ts`: Repeat choice, steps list, "2/5". `src/ledger-done.ts` and `ledger.html`:
  the Done view. Palette `tasks` shows the steps count; ticking from it repeats too.
- Tests: `next_due` over month ends, leap days and weekdays; ticking and unticking; the same new
  id from two computers; quick add (`every fri`, quoted, nothing left for a title); steps limits,
  pasting and reordering; the Done log's grouping, week totals and Reopen.

## Order of work

1. Done log (no schema change, shows value at once).
2. Steps.
3. Repeating tasks.

Each ships with its tests and its lines in `docs/device-checklist.md`.
