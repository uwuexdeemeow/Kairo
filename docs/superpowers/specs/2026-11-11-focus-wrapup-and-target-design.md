# Focus: a wrap-up when a session ends, and a daily focus target

## Why

A focus session ends with "Nice work" and nothing else: no word on how it went. And the goals count
productive time, which piles up whether or not a session was ever started, so there's no reason of
its own to start one each day.

## What

### The wrap-up

When a session ends, by itself or early, the Focus page shows a card for it above the stats:

- **How long**: "Focused 23m of 25m", with "Finished" or "Ended early".
- **Nudges**: "No nudges", "1 nudge", "3 nudges": how often the session spoke up about a
  distraction.
- **What pulled you away**: up to three distracting apps or sites that were in front while it ran,
  with their time, biggest first; those under 30 seconds are left out. "Nothing distracting" with
  none.
- **Against your usual**: up to two lines comparing it with the 20 sessions before it, given at
  least 3: "12m longer than your usual 38m" (or shorter; "About your usual 38m" within 5 minutes),
  and "Fewer nudges than usual" or "More nudges than usual" when it's off the average by more than
  half a nudge.
- **Dismiss**, which puts the card away.

The card is for the latest session that ended. It goes when dismissed, when another session starts,
or 12 hours after the session ended.

The notification for a session that ran its course says the same in a line under "Focus session
done: 25m": "2 nudges · YouTube 3m. The wrap-up is on Chrono's Focus page.", or "No nudges and
nothing distracting. Nice work." A session ended early sends no notification, as now.

Time in a distracting app during a pause counts as pulling you away: pauses are kept as a total, not
as when they were. Nudges weren't counted before this, so sessions from before it are left out of
the nudge comparison.

### The daily focus target

A goal like "2 hours of focus each day", set in Settings under Goals: "Focus sessions, each day",
from Off through 30 minutes to 5 hours.

- It counts time focused in sessions that started that day, pauses left out.
- **Focus page**: a bar above the stats, "1h 10m of 2h today", with "4-day streak" beside it.
- **Today**: a row in Goals, "Focus time", like the productive goal's, with its streak in its value.
  The Goals box shows with only this goal set.
- A notification when it's reached, once a day: "Goal reached: 2h of focus", "You've focused for
  2h 5m today."
- **Its own streak**: days in a row before today on which it was reached. A day with nothing
  tracked and no session neither counts nor breaks it, as for the other goals. It doesn't touch
  the streak the other goals share.
- The palette finds it as a setting, "Daily focus target".
- It travels to combined computers with the other goals.

## How

### `src-tauri/src/focus.rs`

- `focus_sessions.nudges INTEGER`, NULL for sessions from before; a new session starts at 0, and
  the tick adds one each time it sends the distraction nudge.
- `Block` gains `nudges: Option<i64>`.
- `target_minutes`, `set_target_minutes` (setting `goal_focus_min`).
- `focused_started_in(conn, from, to, now) -> i64`.
- `target(conn, date, now) -> Option<Target>`: `target_ms`, `used_ms`, `met`, `streak`; None when
  off.

### `src-tauri/src/wrapup.rs` (new)

- `WrapUp { start, end, planned_ms, focused_ms, completed, label, nudges, pulled: Vec<Pull>, versus:
  Vec<String> }`.
- `latest(conn, now) -> Option<WrapUp>`: for the last session that ended; None while one runs, once
  dismissed (setting `focus_wrapup_dismissed`, the session's start), or after 12 hours.
- `dismiss(conn)`.
- `line(&WrapUp) -> String`: the notification's.
- The comparison and the grouping of what pulled you away are plain functions over rows, tested
  without a database.

### Goals: `goals.rs`

- `Kind::Focus`; `Progress` gains `streak: Option<u32>`, set only for it.
- `for_day` adds the focus target's row after the others; the shared streak is worked out without
  it.
- `set_named_goal("focus", …)`; `check` notifies when it's reached, under the key `goal:focus`.

### Commands and pages

- `FocusOverview` gains `wrap_up` and `target`; `dismiss_focus_wrapup`; `GoalSettings` gains
  `focus_min`.
- `index.html`: the card and the target's bar on the Focus page; the picker in Settings.
- `src/focus-summary.ts` (new) draws the card and the bar, as `src/tasks.ts` draws the task picker;
  `src/main.ts` calls it, and its Goals box draws `focus` rows as it does `productive` ones.
- `palette/settings.rs`: the "Daily focus target" entry.

All shared code: nothing per system.

## Testing

- `focus.rs`: nudges start at 0 and count; an older database gains the column as NULL; the target's
  progress, a day's sessions counting by when they started, the streak skipping empty days and
  stopping at a missed one.
- `wrapup.rs`: what pulled you away, grouped, ordered and cut off; each comparison line and none
  with too few sessions; older sessions left out of the nudge comparison; the latest wrap-up hidden
  while a session runs, once dismissed and after 12 hours; the notification's line.
- `goals.rs`: the focus row follows the others with its own streak and leaves theirs alone; its
  notification.
- `focus-summary.test.ts`: the card and the bar draw what they're given, and Dismiss puts the card
  away.

## Not in this

Pomodoro cycles. A wrap-up in a window of its own. Counting how often the blocked window came up.
