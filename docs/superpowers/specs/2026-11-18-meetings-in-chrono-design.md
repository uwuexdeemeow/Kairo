# Meetings in Chrono: history, the day's timeline, and a total

Built on the calendar import Kairo already has. No OS calendar access, no new dependency, no
network beyond the addresses already read. Built in 1.31.59, with these differences from the draft:
the timeline draws meetings as a lane under the focus lane, not as bands behind the app blocks
(it reads the same way the focus lane does and every meeting can be reached with the arrow keys);
the palette answers through the existing answer card with Meetings as a subject, so there is no
new card and no native-palette change; the week's data carries the total and the Sunday
notification says it, while the month and year notifications and the Patterns "mostly meetings"
line are left for later.

## Why this, and not "calendar as a source of time" from scratch
Most of that idea exists. `calendar.rs` reads private iCal addresses (Google's secret address,
Outlook's published calendar, iCloud's public share), the same way on Windows, macOS and Linux,
and only reads. Plan already shows today's meetings, finds the free gaps among them and fits tasks
with an estimate (`ledger-free.ts`). Ledger's Calendar shows meetings per day. Time away during a
meeting labels itself, scheduled focus waits for a meeting to end, and the palette answers "next
meeting".

Reading the calendar through the OS (EventKit on macOS, the Windows appointments store, Evolution
or KDE on Linux) was the heavy part of the first idea. It is left out on purpose: it needs
permissions and per-OS code, it can't be checked on Linux at all, and the iCal address already
reaches every calendar people use. The three things still missing are all in Chrono:

1. **Meetings are forgotten.** `store` deletes every event on each read and keeps two weeks either
   side of now, so a day older than two weeks has no meetings, and a day's meetings can vanish
   from under a view already opened on it.
2. **Chrono's day doesn't show them.** The timeline draws what was on screen and focus sessions,
   so a meeting looks like a gap or like whatever app was in front.
3. **Nothing adds them up.** "How much of my week was meetings?" has no answer, though the recap,
   the patterns and the palette are where it would be asked.

## What the person sees
- **The day's timeline** (Chrono, any day with a connected calendar) gets a quiet band per meeting
  behind the app blocks, in the neutral ink, with its time range and length on hover and on focus.
  A gap between meetings is the unmarked time between bands; nothing is drawn for it. Where a
  meeting overlaps tracked time, both show: the band behind, the app blocks over it. Tapping or
  pressing Enter on a band opens a small line: "Design review, 10:00 to 11:00, 1h". Its title is
  shown or blank by the rule below.
- **A Meetings line in the day's totals**, beside active, productive and focus: "Meetings 2h 30m",
  only when there were any. A meeting counts as the time it was on the calendar, whether or not
  you were at the computer, so the line says "on the calendar" in its tooltip and never claims you
  attended.
- **The week recap and the month and year recaps** gain one line each: "Meetings: 6h 30m, 3h more
  than last week". A week with no meetings leaves the line out.
- **The palette** answers `meetings this week`, `meetings today` and `meetings last week` with a
  one-line card: the total, the count and the busiest day. Windows' native palette and the web one
  draw the same card, as for the other answers.
- **Patterns** (best hours) learn to say when a weak spot is a meeting: a low-focus hour that is
  meetings on most of its days reads "mostly meetings" instead of looking like a habit.

## Data
- **Keep what was read.** `store` stops deleting the past. Each read replaces only the events inside
  the window it read (now minus 14 days to now plus 14 days) and leaves older ones. An event
  cancelled or moved inside the window disappears or moves, as before. Older rows stay as the last
  read left them. Events are pruned at 400 days, once a day with the existing read.
- **Titles follow the title retention already in privacy settings.** Times and lengths are kept for
  the 400 days; a meeting's title goes blank when titles from that day would (`privacy.rs`), and is
  blank from the start for a calendar marked "Hide titles". Chrono shows "Meeting" for a blank one.
  Nothing new is asked.
- **No second table, no migration beyond the delete rule.** `calendar_events` already holds
  `start_ms`, `end_ms`, `title`. A meeting's identity for the "inside the window" replace is its
  start, end and title, as today.
- **Meeting time** is computed when asked, from `calendar::between` clipped to the day, with
  overlapping events merged so two overlapping invites count once. All-day, cancelled and "free"
  events are already left out by the reader and stay out.
- **Combined computers.** Each computer reads the calendar itself; nothing about meetings is
  shared, as now. Two computers with the same address show the same meetings.

## Privacy
Meetings are read-only and local. Titles are the sensitive part and follow the existing retention.
The palette card for meetings shows totals and counts only, never titles. A new button under
Settings > Calendar, **Forget past meetings**, deletes every stored event older than now, for
anyone who doesn't want the history kept; it asks to press again.

## Non-goals
- Reading calendars through the operating system, or from an account (OAuth).
- Writing to a calendar, accepting invites, or making events from tasks.
- Attendees, locations, links, or joining a call.
- Counting a meeting as attended, or comparing time at the computer with the invite.
- A new view for the calendar in Chrono: the meetings appear in views that already exist.
- Attributing the app time inside a meeting to the meeting (the Zoom window stays Zoom).
- Changing Plan or Ledger's Calendar, which already show meetings.

## Approaches
- **Keep history in the table that exists and draw it in views that exist** (chosen): one changed
  delete, one total, a band in the timeline, and a palette card. Lightest, and reaches every
  platform the same way.
- **OS calendar access**: the original idea. Heavier, platform-specific, unverifiable on Linux, and
  adds nothing the iCal address doesn't.
- **A separate meetings table with attended time**: would answer "did I actually attend", at the
  cost of guessing attendance from app focus, which Kairo has no way to know.

## Edge cases
- No calendar connected: nothing changes anywhere; no Meetings line, no bands, and the palette says
  there's no calendar connected, as "next meeting" does.
- A calendar removed: its stored past stays until "Forget past meetings", and the line says so.
- A recurring meeting changed in the calendar: past occurrences stay as last read; future ones
  follow the calendar.
- A meeting across midnight: clipped to each day, counted in both.
- Daylight saving days: lengths come from the stored milliseconds, so a 23- or 25-hour day is
  right.
- Many events (a busy shared calendar): the timeline draws up to 40 bands a day and says "and 6
  more" under it; totals still count all of them.
- Overlapping invites: merged for the total; both bands drawn.
- A read that fails: old rows stay, and the status line says when it was last read, as now.

## Built by
`calendar.rs` (the replace-within-window `store`, the 400-day prune, `forget_past`, a clipped and
merged `meeting_ms(conn, from, to)`), `privacy.rs` (blank titles past retention for events),
`recap.rs` (the line for week, month and year), `analysis.rs` (the day's total and the patterns'
"mostly meetings"), `commands.rs` (`get_calendar_events` already serves the bands; one command for
Forget past meetings), `palette/search.rs` and `palette/cards.rs` (the meetings answer),
`native.rs` and `palette.ts` (the card on Windows and on macOS/Linux), `main.ts` and
`styles.css` (the bands and the totals line), `kairo-settings.ts` and `dashboard.html` (the
button), with a test beside each Rust function and TypeScript module. Windows, macOS and Linux all
get it.

## Check on a real machine before shipping
A day with several meetings, one running over another, one across midnight, and a day with none;
the bands over real app time on Windows and macOS at the narrow window size; the palette card on
both palettes; and a calendar read after two weeks of use, to see that old days keep their meetings
and a cancelled one inside the window goes.
