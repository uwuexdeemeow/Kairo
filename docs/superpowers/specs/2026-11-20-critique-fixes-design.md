# Critique pass over all of Kairo's pages (1.31.62)

Eight pages were reviewed on 20 November (Chrono, the dashboard, the web palette, the ring, the
blocked window, the speed test, the note pop-out, the VirusTotal scan window), each by a design
review and a mechanical check, run apart. Their snapshots are in `.impeccable/critique/`. This is
what 1.31.62 did about them, and what it left.

## Done

**Privacy**
- A link VirusTotal has no record of is no longer sent for a scan on Enter. `vt_check` only looks it
  up; a link with no record comes back as `linkNotSeen`, and "Scan this link" (`vt_scan_url`) sends it
  after saying so: scanning shares it with security companies, and a link can carry a private token.
  A palette-launched link asks the same way. A line under the field always says what is sent where.
- A paste that isn't a link or a hash is said under the field (`askable`, which mirrors `classify`),
  the text is kept, and nothing is sent or turned into a result that can't be tried again.

**The floor (contrast, targets, focus, announcements)**
- The palette's selected row has a 3px bar (rows) or a 2px outline (tiles) as well as its tint, in the
  web and the native palette. The footer says "Show" while the first Enter on a time chart only shows
  it. The empty state, an error and a failed search are said. `aria-expanded` follows the list; the
  Enter chip isn't read. Chips are 36px, the search keycap's edge shows focus.
- Neutral time in charts holds 3:1 in both grounds (`--neutral` #6b7384 dark, #7f7a73 light; the
  native palette's copies too).
- The blocked window's strict-mode hold uses `aria-disabled`, so the keyboard stays on the button; its
  state changes are read from one status line, not the time left; a failed call says so; the limit
  reads as the person's own.
- The note pop-out's ⋯ menu returns the keyboard to ⋯ (or the note), is a `menu` with `menuitem`s,
  opens with the down arrow. The editor and its copy reserve the same scrollbar gutter.
- The ring shows a pointer and a hover outline on a segment, ellipsises a long line, and closes if its
  state can't be read.
- Calendar ticks, task checkboxes and the tag delete are 24px.
- The meetings band holds 3:1 (60% of the ink) and has a hover ring and a taller target.

**Failures said, not swallowed**
- Goal selects save with a "Saved" or "Couldn't save." line and put the old value back on failure;
  the demo-data button says when it fails; three raw error strings go through `plain()`.
- The speed test keeps what it measured when a later step fails, offers "Try again", doesn't resize
  its button, and shows 99.96 as 100.
- The scan window offers "Try again" only for what trying again can fix, and lets a result be
  removed or a queued one cancelled.
- A failed meetings read says so on Today.

**Structure and words**
- Settings groups: "Look and upkeep", "Palette and ring", "Capture and scanning", and Find knows the
  words people use (shortcut, hotkey, startup, backup...) and says how many match.
- Home's title holds its line while it loads, says "overdue" when everything is late, and the empty
  day doesn't say "Nothing due today." twice. The focus strip is announced when a session starts,
  pauses or ends, not as the minutes change.
- Patterns says so when nothing was tracked, and its axes use the timeline's clock ("6am").
- Chrono's stat definitions are read with the figure; "Distracting" is one word everywhere; the
  welcome page says the browser extension is optional.
- "Clean" is "Nothing flagged".

## Left, with the reason
- The ring's neighbours' names and the digit shortcuts: a layout change that the native ring must
  match, and it can't be checked on a machine from here.
- The speed test's running figure and a Stop: needs progress events from the backend.
- The note window's "Saving…" flicker, always-on-top default and double-booked pin: the save status
  was designed around its tests; the default needs a decision.
- "Your apps" stays the first Settings group.
- Today's stat row still has no lead tier, and Focus's doubled presets and scattered settings are
  not changed.
- Review's rows have no arrow-key movement beyond Tab.
