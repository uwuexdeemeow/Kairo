# Ledger: three features to build next

Why these three: each uses something only Kairo has (it already knows the front window, the site,
idle time and focus sessions, all on the device), so a sticky note or an ordinary to-do app can't
copy it by adding a field. This scopes them in the order to build them.

Constraints that apply to all three (from how Kairo is run):

- **Every OS.** Each feature ships for Windows, macOS and Linux; CI checks the two non-Windows ones.
  Linux has never been tried on a real machine, so keep Linux behaviour simple and say so in the
  checklist.
- **Lightweight first.** No new dependency unless nothing else will do. No new thread when the
  collector's one-second tick can carry the work.
- **Private by default.** Nothing leaves the device except through the existing encrypted sync.
  Respect the privacy rules (Ignore, Hide title, incognito, pause) everywhere a feature reads the
  front window or site.
- **Quiet.** A feature that fires a notification must be easy to turn off and never fires while a
  game or full-screen app is in front.

## 1. Reminders that fire at the right moment

**What the user gets.** A task can be reminded by a moment, not only a clock time: "when I next open
Gradescope", "when this focus session ends", "when I'm back after being away". The name Kairo is
that idea; today's reminders are clock-only.

**In scope (first version).**
- Two new kinds of trigger on a task: **when I open** an app or site (named by the same keywords
  Chrono's projects use), and **after this focus session**.
- Added from the task panel's "Add…" choice as "Reminder when…", shown as a field only once set,
  with the ✕ to remove it (as the other fields).
- Fires once, as the existing notification, then is marked done like a clock reminder.
- Palette: `remind me when I open gradescope: submit essay` is parsed like the other quick-add
  forms.

**Out of scope.** Location, calendar events as triggers, repeating moment reminders, a rules engine.

**Where it hooks in.**
- Triggers need to know when the front app or site changes. Nothing publishes that today (blocking,
  focus and the browser module each poll `platform::foreground()` themselves). Add one small
  broadcast from the collector's capture step (`rs/collector.rs`, where the `Sample` changes) and
  read it from `rs/reminders.rs`. This is the one structural change; it also removes the need for
  the next feature to poll.
- Stored on the task as one new column (`remind_when`, text such as `open:gradescope` or
  `after-focus`), added to the migration list in `rs/db.rs`, shown by `Task` in `rs/tasks.rs`.
- Sync: carries with the task's plan (`taskplan:<id>` in `rs/sync/mod.rs`); the existing
  `reminded` marks stop a second computer repeating it. Old copies ignore the new field, so
  computers can update at different times.
- Honour the idle gate and the `settings.reminders` switch already in `reminders.rs`; add the
  quiet-around-full-screen rule.

**Risks.**
- Matching on a title or site means a hidden title (Hide title) can't trigger it. Say so in the
  field's help text rather than failing silently.
- Firing on every window switch could be noisy: fire once per task, then mark it, and wait a minute
  before the next check.

**Done when.** A moment reminder fires once when the matching app or site comes to the front, on all
three systems, never for an Ignored app, and syncs without firing twice. Tests: the matching and
once-only logic as a pure function (as `due_now()` already is), the quick-add form, the panel field.

## 2. Tasks that know what they cost

**What the user gets.** A task shows the time actually spent on it, an estimate beside it, and a
plain warning when a day holds more than the user usually focuses.

**In scope.**
- An **estimate** on a task (minutes), added from "Add…", shown on the row as "~2h".
- **Spent** = the focus time already recorded per task (`focused_ms`, from `focus_sessions.task`),
  shown beside the estimate: "1h 35m of ~2h".
- **Day load** in Ledger's header and on Home: "4h of tasks, you usually focus about 3h". The
  usual figure comes from Chrono's recorded focus time over recent weeks; shown only when there are
  enough days behind it (the product's "honest about time" rule), otherwise nothing.
- The "usually take about twice your estimate" line only after at least five finished tasks with
  both an estimate and spent time.

**Out of scope.** Automatic time-per-task from app use (attribution by project keywords is computed
at query time and isn't stored per task), billing, charts.

**Where it hooks in.**
- One new column (`estimate_min`) on `tasks`; add to the migration list and to `taskplan:<id>` sync.
  Note `focused_ms` is sent as 0 to other computers today, so spent time is per computer: say so, or
  extend the sync to carry it (a separate, larger decision).
- Day load reads Chrono's existing focus totals; no new tracking.

**Risks.**
- Misleading numbers: a figure built from a few days is worse than none, so every derived line has a
  minimum-data rule and hides below it.
- Spent time only counts focus sessions started on that task, not time the user simply worked on
  it. Make Focus easy to start from the row (it already is) and don't imply more.

**Done when.** The estimate shows on the row and the panel, spent time reads correctly across a
paused and extended session, day load hides until enough data exists, all three systems. Tests: the
load and "usually" maths as pure functions with the minimum-data cases.

## 3. Capture from any window

**What the user gets.** Select text anywhere, press a key, and it becomes a task or a note that
remembers where it came from. This is the feature that replaces a sticky note.

**In scope.**
- A **global shortcut** (default off until the user chooses a key in Settings, as the palette's is
  recorded) that opens a tiny capture box prefilled with the selected text.
- Choose Task or Note; a task goes through the existing quick-add parser (`rs/quick_add.rs`), a note
  opens in the note pop-out.
- The source, kept as a line on the task or note: the window title and, in a browser, the site
  (subject to the privacy rules, so an Ignored or private window records no source).
- Palette: `capture` opens the same box.

**Out of scope.** Screenshots, OCR, capturing files or images, a browser-extension "send page"
button (a later step: the extension already reports the active site).

**Where it hooks in.**
- A third global shortcut beside the palette's and ring's (`rs/lib.rs` registration and handler).
  Wayland doesn't let apps listen for keys, so on GNOME and KDE it follows the same path as the
  palette (a desktop shortcut running a Kairo command-line flag, as `kairo --palette` does).
- Reading the selection is the hard, per-OS part. The least invasive route that works everywhere is
  to send the system's copy shortcut and read the clipboard, then restore what was there; the
  clipboard history module (`rs/clipboard.rs`) already reads the clipboard. State plainly that it
  briefly uses the clipboard.
- The source line is stored in the task's `note` link or a new `source` column; prefer reusing a
  note (the task already points to one) over a new column.

**Risks.**
- Sending a copy keystroke can misfire in a game or full-screen app (the product says never get in
  the way of play): do nothing there.
- macOS needs the Accessibility permission to send the keystroke; Kairo already asks for related
  permissions, so add this to the same prompt and the checklist.
- Restoring the clipboard must be reliable, or capture would silently eat what the user had copied.

**Done when.** On Windows and a Mac, select text in a browser and a document, press the key, and a
task or note exists with its source; the clipboard is back as it was; nothing happens in a game or
for an Ignored app. Linux is attempted but never checked on a real machine, as the checklist states.

## Order and size

1. **Moment reminders** (medium). One shared broadcast from the collector plus one column. It also
   unlocks the palette's "what was I doing" idea later.
2. **Cost** (small). One column, one derived line; the data already exists.
3. **Capture** (large). Per-OS selection handling and a new window; do it last, with the most
   real-machine checking.

Each ships as its own release, with its own section in `docs/device-checklist.md`.

## Questions for the owner

- Should moment reminders also match **Chrono projects** by name ("when I start working on Kairo")
  or only app and site keywords?
- Is it acceptable that capture briefly uses the clipboard, or should the first version only capture
  what was already copied (no keystroke sent)? The second is simpler and safer, but one step more
  for the user.
- Should spent time sync between computers (changes what `focused_ms` means in sync)?
