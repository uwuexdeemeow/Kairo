# Focus: a pomodoro cycle

## Why

A focus session is one stretch, 25 minutes to 4 hours, and then it's over. Many people work in
rounds instead: 25 minutes, a short break, 25 again, and a longer rest after four. Today that means
starting each round by hand and pausing for every break, and a pause is a poor break: it hangs
until someone remembers to resume. A cycle should run the rounds and the breaks itself.

## What

### A cycle

A cycle is a run of ordinary focus sessions ("rounds") with breaks between them.

- **Round**: a normal focus session of the chosen length. It blocks, nudges, counts toward the daily
  focus target, the stats and the wrap-up exactly as a session started by hand does.
- **Break**: starts by itself when a round runs its course. **Nothing is blocked during a break**,
  as when a session is paused or none is running; the nudge stays quiet too. A break is not a
  session: it isn't in the stats, the target or the streak.
- **After a break** the next round starts by itself, unless the user is away (no input for 2
  minutes), in which case it waits and starts at the first input, as scheduled focus does. A
  notification says "Break over. Round 3 of 4." when it starts.
- **Long break**: every fourth break is the long one.
- **Ending**: ending the session early (the Focus page, the sidebar, `end focus`) ends the whole
  cycle. So does ending during a break. Starting any other focus session by hand replaces the cycle.
  A cycle also ends after 12 hours with no round started, so it can't linger after an absence.
- **Strict mode** applies to rounds, as now. A break has nothing to be strict about.

Skipping a break is one action: "Skip break" starts the next round now.

### Starting one

- **Palette**: `pomodoro` starts a cycle with the saved lengths. `pomodoro 50 10` sets the round and
  the short break, in minutes; text after the numbers is the label, as with `focus 50 essay`
  (`pomodoro 50 10 essay`). The row says what Enter will do: "Pomodoro: 25m rounds, 5m breaks".
  A label that names a saved block list picks that list, as for any session.
- **Focus page**: the start form gets a "Pomodoro" switch beside the minutes. On, it starts a
  cycle with the saved lengths; the minutes field becomes the round length.
- **Settings → Focus**: round length (default 25), short break (5), long break (15), rounds before
  the long break (4). Typed lengths in the palette or the form are for that cycle only.

### While it runs

- **Focus page**: the running card shows "Round 2 of 4" under the countdown; on a break it shows
  "Break · 3m left" with **Skip break** and **End**. Between rounds the long-break count restarts
  each time a long break finishes.
- **Palette**, while a cycle is on: `pause` and `extend` behave as for a session. On a break the
  rows are "Skip the break" (`skip`, `skip break`) and "End the pomodoro" (`end focus`,
  `end pomodoro`), and the second line says "Break · 3m left · round 3 of 4 next".
- **Notifications**: a round that ran its course says "Round 2 done. 5 minutes' break." in place of
  the wrap-up line; the long break says "Round 4 done. Take 15." These go out whether or not the
  window is open.

### The wrap-up and the target

Every round is a session, so each counts toward the daily focus target, its streak and the stats,
and the target's "reached" notification fires as usual. The wrap-up card shows for the latest round
that ended, as now. A round that runs its course inside a cycle sends the break notification
instead of the wrap-up line; a round the user ends early (which ends the cycle) sends nothing, as
now.

## How

### `src-tauri/src/pomodoro.rs` (new)

- Setting keys `pomodoro_work_min`, `pomodoro_break_min`, `pomodoro_long_min`,
  `pomodoro_rounds`, with getters and setters clamped (work 5–120, short 1–30, long 5–60, rounds
  2–8).
- `Cycle { work_ms, break_ms, long_ms, rounds, round, break_until: Option<i64>, last_activity }`
  kept in the `settings` table as one JSON value (`pomodoro_cycle`), absent when no cycle is on.
  A cycle is a few numbers, so it survives a restart without its own table.
- `start(conn, work, short, label, now)`: stores the cycle and starts round 1 through
  `focus::start_by_hand`.
- `next(&Cycle, now) -> Step`: a plain function: `Break { until, long }`, `Round`, `Wait`.
  Tested without a database.
- `tick(conn, now, idle_ms) -> Option<Event>`: called from the focus tick. When the round just
  completed (`focus::complete_if_due` returned it) it begins a break; when a break is over and the
  user isn't away it starts the next round; returns what to notify.
- `skip_break(conn, now)`, `end(conn, now)`, `current(conn, now) -> Option<Status>` for the pages.

### `src-tauri/src/focus.rs`

- `stop` and `start` clear the cycle unless the call comes from the cycle itself (a private
  `start_round` the cycle uses), so a hand-started session replaces it and an early end finishes it.
- No change to blocking: with no session running (`focus::running_until` is None) nothing is
  blocked, which is already how a break must behave. Tests pin that down.
- The tick calls `pomodoro::tick` after `complete_if_due`; a completed round's own "Focus session
  done" notification is replaced by the break's while a cycle is on.

### Commands and pages

- `FocusOverview` gains `cycle: Option<Status>` (`round`, `rounds`, `on_break`, `break_left_ms`);
  `start_pomodoro`, `skip_pomodoro_break` commands; `GoalSettings`-style `PomodoroSettings` for the
  four numbers.
- `index.html`, `src/main.ts`: the switch, the "Round n of m" line, the break card and the four
  fields. `src/focus-summary.ts` draws the break card, as it draws the wrap-up.
- `palette/commands.rs`: `pomodoro` rows and the break rows; `palette/settings.rs`: the four
  settings as palette settings.
- Lengths and the on/off switch are per computer, like focus sessions; they don't sync.

All shared code: nothing per system.

## Testing

- `pomodoro.rs`: `next` over a whole cycle (round, break, round, … long break after the fourth, the
  count restarting); a break waiting for activity when the user is away; the 12-hour lapse; getters
  clamping; typed palette numbers overriding the saved ones for one cycle only.
- `focus.rs`: a hand-started session clears the cycle; stopping early clears it; the cycle's own
  rounds don't; nothing is blocked during a break (`blocking` over a break in the middle of a
  cycle); the round's time counts toward the target and the stats, the break's doesn't.
- `palette/commands.rs`: `pomodoro`, `pomodoro 50 10 essay` and the break rows parse and say what
  they do.
- `focus-summary.test.ts`: the break card and the "Round n of m" line draw what they're given; Skip
  break and End call their commands.

## Not in this

A summary of a whole cycle. Different lengths for each round. Sound at the end of a round beyond
the notification. Syncing a cycle between computers. Breaks that suggest something to do.
