# Palette commands: act from the palette without opening a window

## Why

The palette starts a focus session (`focus 50 essay`) but can't do anything to one that's running:
"End the focus session" only opens the Focus page. Changing the theme, pausing tracking and timing
something small all mean opening a window or the tray. The user wants to type these and have them
happen.

## What

Commands are typed plainly, like `focus 50 essay`: a row says what Enter will do, and Enter does
it. There are no new bubbles; bubbles stay scopes for searching. A command's word with nothing
after it lists its choices or says what to type, so nothing has to be remembered.

Every command row sits at the top when its words lead the query, as the focus command does. Typed
loosely (`dark`, `tea timer`), the rows are found by fuzzy match and rank with the other actions,
below a strong app match.

### Theme

| Typed | Row | Enter |
|---|---|---|
| `theme` | One row per theme, "Theme: Walnut" and so on, System first; the one in use says "In use" | Switches to it |
| `theme walnut`, `walnut theme` | "Theme: Walnut" | Switches to it |

The themes are the ones in `appearance::GROUNDS` plus System, under the names the settings page
shows (Ink for `dark`, Paper for `light`; `dark` and `light` find them too). The palette stays
open and repaints in the new theme, so several can be tried in a row. The existing "Theme" settings
row stays, below these.

### A running focus session

Offered only while a session runs.

| Typed | Row | Enter |
|---|---|---|
| `pause` | "Pause the focus session" (running, not paused) | Pauses it |
| `resume` | "Resume the focus session" (paused) | Resumes it |
| `extend 10`, `extend` | "Add 10 minutes to the focus session"; 5 with no number | Adds them, up to the 240-minute maximum |
| `end focus`, `stop focus`, `end` | "End the focus session" | Ends it |

Each row's second line says how the session stands: "32m left · “essay”", or "Paused · 32m left".

In strict mode, ending waits and then asks again. The palette doesn't skip that: with strict mode
on, "End the focus session" opens the Focus page, where the countdown is, as it does today. Pause,
resume and extend aren't held by strict mode today and aren't here.

### Allow 5 minutes

Offered only while the blocked window is showing.

| Typed | Row | Enter |
|---|---|---|
| `allow`, `allow 5` | "Allow Steam for 5 minutes" | Allows it and closes the blocked window |

The length is the blocked window's own (`blocking::ALLOW_MINUTES`); a typed number other than 5
changes nothing. In strict mode the row brings the blocked window forward instead, since the wait
and its countdown are there.

### Quick timer

A countdown with a notification at the end. It tracks nothing, blocks nothing, and isn't a focus
session; it never appears in history.

| Typed | Row | Enter |
|---|---|---|
| `timer` | Running timers, soonest first: "Tea" with "6m left · rings at 14:32"; then a note, "Type minutes and a name: timer 10 tea" | Nothing on a timer's row; Ctrl+Enter cancels it |
| `timer 10 tea` | "Start a 10-minute timer", "Called “tea”" | Starts it |
| `timer 10` | "Start a 10-minute timer", "No name" | Starts it |

- Minutes are 1 to 240, read as the focus command reads them (`10`, `10m`, `10 min`).
- Several run at once. At most 20; past that the row says so and Enter does nothing.
- When one ends: a notification, "Timer done: tea" (or "Timer done: 10 minutes"), within five
  seconds of its time.
- Timers are saved by their end time, so a restart or update keeps them. One that ended while Kairo
  was closed rings when Kairo next starts, saying when it was due: "Timer done: tea", "It ended at
  14:32".
- While any runs, the palette's suggestions (nothing typed) show a row for the soonest: "Timer:
  tea", "6m left".

### Pausing tracking

| Typed | Row | Enter |
|---|---|---|
| `pause tracking` | "Pause tracking", "Until you resume it" | Pauses, as the tray's item does |
| `pause tracking 30` | "Pause tracking for 30 minutes", "Resumes at 15:02" | Pauses, and resumes by itself then |
| `resume tracking` | "Resume tracking" (paused) with "Paused until 15:02" or "Paused" | Resumes |

- "Pause tracking" is offered while tracking; "Resume tracking" while paused.
- `pause` alone during a focus session leads with the session's row and offers "Pause tracking"
  under it; with no session it's "Pause tracking".
- Minutes are 1 to 240. Pausing again while paused replaces the time to resume.
- Resuming by hand, from the palette, the tray or the ring, forgets the time to resume.
- The pause is not saved: Kairo starts tracking when it starts, as now, so the time to resume
  isn't saved either.
- The tray's item keeps saying which it would do, as now.

### Focus until the next meeting

| Typed | Row | Enter |
|---|---|---|
| `focus until meeting`, `focus until next meeting`, `focus till meeting` | "Focus until Design review", "47 minutes, to 14:00" | Starts a session of that length |

- Words after `meeting` are the label, as in `focus 50 essay`: `focus until meeting essay`. With no
  label the session is unlabelled.
- Offered when the next meeting starts 5 minutes to 240 minutes from now. Nearer, later or with no
  meeting, the row is a note saying why: "Design review starts in 3 minutes", "Your next meeting
  is more than 4 hours away", "No meetings on your calendar".
- The length is the whole minutes until the meeting starts, so the session ends at or just before
  it.
- It's an ordinary session from then on: strict mode refuses it while another session runs, as it
  refuses `focus 50`.

## How

### Search: `src-tauri/src/palette/commands.rs` (new)

`search.rs` is 1,700 lines; the new commands get their own file beside it, as `settings.rs` and
`mode.rs` have.

- `pub fn rows(ctx: &Context, query: &str) -> Vec<Item>`: every row above. `search::search` adds
  them next to `task_items`.
- `pub fn leads(query: &str) -> bool`: whether the query is one of these commands, so the
  calculator stands down as it does for `focus 50` (`extend 10` is no sum).
- `pub fn suggestion(ctx: &Context) -> Option<Item>`: the soonest timer's row for
  `search::suggestions`.
- Parsing reuses `search::after` and the minutes reading of `focus_command`, pulled out as
  `search::minutes(word) -> Option<u32>`.

`Context` gains:

- `session: Option<SessionNow>` in place of `Option<Option<String>>`: `label`, `left_ms`, `paused`.
- `strict: bool` (`blocking::strict_now`).
- `tracking_paused: Option<Option<NaiveDateTime>>`: None while tracking; the time to resume, if
  one is set.
- `blocked: Option<(String, String)>`: the blocked window's label and key, while it shows.
- `timers: Vec<Timer>`.
- `theme: String`: the appearance in use.

`Action` gains `Theme(&'static str)`, `Session(SessionDo)` with `Pause`, `Resume`, `Extend(u32)`,
`End`; `Allow(String)`; `StartTimer(u32, Option<String>)`; `CancelTimer(i64)`;
`PauseTracking(Option<u32>)`; `ResumeTracking`. "Focus until meeting" is a `StartFocus`.

The palettes already ask again after a `Toggle` to show the new state. The same happens after
`Theme`, `CancelTimer` and the tracking and session actions, so the palette stays open on them;
`StartTimer`, `Allow` and `Session(End)` close it, as `StartFocus` does.

### Doing them: `palette::perform`

| Action | Does |
|---|---|
| `Theme` | What `commands::set_appearance` does, moved into `appearance::choose(app, conn, mode)` so both call it |
| `Session` | `focus::pause`, `resume`, `extend`, `stop`, then emits `kairo:focus` so an open Focus page re-reads |
| `Allow` | `blocking::allow` and `blocking::close_window` |
| `StartTimer`, `CancelTimer` | `timers::start`, `timers::cancel` |
| `PauseTracking`, `ResumeTracking` | `crate::set_paused(app, paused, until)` |

### Timers: `src-tauri/src/timers.rs` (new)

- A table `timers (id INTEGER PRIMARY KEY, ends_ms INTEGER NOT NULL, minutes INTEGER NOT NULL,
  label TEXT)`, made in `db.rs` with the others.
- `start(conn, minutes, label, now) -> Result<Timer>`, `cancel(conn, id)`, `running(conn) ->
  Vec<Timer>` (soonest first), `take_due(conn, now) -> Vec<Timer>` (removes and returns those
  whose time has come).
- No thread of its own: the focus tick (`focus::tick`, every five seconds) calls `take_due` first
  thing and notifies for each. Paused tracking doesn't hold timers back.

### Tracking: `lib.rs`

- `PauseItem` gains the time to resume, an `AtomicI64` (0 for none).
- `set_paused(app, paused: bool, until: Option<i64>)` sets the flag, the time and the tray's
  words; `toggle_paused` calls it, so the tray and the ring clear the time.
- The focus tick resumes tracking once the time has passed.

### Blocking: `blocking.rs`

- A managed `Showing(Mutex<Option<(String, String)>>)`: `show_blocked` sets the label and key,
  `close_window` clears it. The palette's context reads it.

### The palettes

Nothing new is drawn. The native palette (`native.rs`) and the web one (`web.rs`,
`src/palette.ts`) get the new rows through `search` and run them through `perform`; the only
change in each is the list of actions that keep the palette open and ask again.

## All three systems

Everything here is shared Rust and the web page: no platform code. Notifications go through
`tauri_plugin_notification`, as the focus session's do. The native palette is Windows only and is
built and tested by Windows CI; macOS and Linux use the web palette.

## Testing

- `commands.rs`: each command's rows from a `Context`; session rows only with a session, the right
  one of pause and resume; strict mode's "End" opening the Focus page; `allow` only while blocked;
  `timer` listing, starting, the cap, and its note; `pause` alone with and without a session;
  `pause tracking 30` and its time; each of "focus until meeting"'s notes and its length; `extend
  10` and `timer 10` not read as sums; loose words (`dark`, `walnut`) finding their rows below an
  app of that name.
- `timers.rs`: start, list in order, cancel, `take_due` taking only what's due, a timer due before
  now (Kairo was closed) taken at once.
- `lib.rs` or `focus.rs`: a pause with a time resumes once it passes; resuming by hand clears it.
- `blocking.rs`: `Showing` set and cleared.
- Web palette (`palette.test.ts`): choosing a theme row keeps the palette open and asks again;
  Ctrl+Enter on a timer row cancels it.
- Existing tests that build a `Context` take the new fields.

## Not in this

Pomodoro cycles, the session wrap-up card and the daily focus target are the next spec. Window
snapping and labelling the current stretch aren't planned.
