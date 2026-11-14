# "What was I doing at 3 pm?" in the palette

Small change, built on what Kairo already records. No new storage, network or dependency.

## Goal
Type a time into the palette and see what was on screen then, from Kairo's own history.

## What the person sees
- Typing `3pm`, `3:30 pm`, `15:00`, `noon`, `yesterday 3pm`, `3pm yesterday`, `tue 3:30 pm`,
  `friday morning`, `last night`, `this morning`, or `what was I doing at 2pm yesterday?` shows one
  row: "What was I doing yesterday at 3:00 pm?". Enter opens a card; Enter again opens that day in
  Chrono. A time with no day is today's, or yesterday's when today's hasn't come yet. A moment still
  ahead is no moment. Parts of a day mean 10:00, 15:00, 19:00 and 22:00, and need a day or "this"
  beside them so a lone `night` stays a search.
- The row says nothing of what was open; the card waits for Enter, so nothing sensitive shows while
  typing in front of someone.
- The card: the stretch holding that moment (marked), up to two before and two after, each with
  its time and length; whether it was in a focus session or a meeting; "Away" when idle; a note when
  nothing was recorded.
- Window titles (and a meeting's name) are **hidden by default**. A switch in Kairo's settings under
  the palette ("Show window titles when I ask what I was doing at a time") shows them.

## Privacy, as it already works
Ignored apps are never recorded so never appear; "Hide title" apps and titles past the title
retention are blank; the card reads the same history as the day views.

## Non-goals
Searching by what was on screen ("when did I open X"), dates with times in other forms, a
palette-settings entry for the switch (the settings page has it).

## Approaches
- A card read from the database on Enter, like the day and time-chart cards (chosen): the pure
  search stays pure, and the sensitive part is only fetched when asked for.
- Rows built while typing: would read the history on every keystroke and show titles in the list.
- A Chrono-only view: not reachable from the keyboard, which is the point of the palette.

## Built by
`search.rs` (`parse_moment`, the row, `Preview::Moment`), `cards.rs` (`moment`, `Card::Moment`),
`native.rs` and `palette.ts` (the card on Windows and on macOS/Linux), `mod.rs`, `commands.rs`,
`lib.rs`, `api.ts`, `kairo-settings.ts` and `dashboard.html` (the switch).
