# What you usually do now

Built in 1.33.0, after the owner agreed to it as the next feature. The palette's empty state looked
back (today's busiest apps); this has it look ahead, from the time Chrono already records.

## Rule

- For each of the last 4 weeks, the same weekday, the half hour either side of now: the apps and
  sites with at least 2 minutes there, and the focus sessions started there (by planned length and
  label, case ignored).
- Something turns up as a habit when it appears in at least 3 of those 4 windows.
- Anything used in the last 15 minutes is left out: you're doing it already.
- One focus session at most, and it leads; then most weeks, then most time.
- The palette shows up to 3, skipping apps it has no way to open and sites it can't turn into an
  address (`browser::address_for_site`).

## What the rows say

"3 of the last 4 Mondays, around 9:00", or "Every Monday lately, around 9:00" for 4 of 4; the time
is rounded to the nearest half hour. A usual focus session takes the place of the generic
"Start a 25-minute focus session" row.

## Calls made

- Nothing new is stored: it's worked out from `sessions` and `focus_sessions` as the palette opens
  (five small queries).
- It sits under the existing Palette suggestions switch rather than a switch of its own.
- Layouts are not suggested: Kairo doesn't record when a layout is restored, so there's no history
  to be honest about.

## Where

`src-tauri/src/usual.rs` (rule and tests), `palette/search.rs` `suggestions`, `browser.rs`
`address_for_site`.
