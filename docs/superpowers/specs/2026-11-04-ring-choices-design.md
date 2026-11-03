# The app ring, yours to arrange; every app's settings in one place

## Goal

Choose what's on the app ring and in what order, from Kairo's own items. Show the ring on the
dashboard as one of Kairo's apps. And list every app's settings in Kairo's settings, Ledger's
included, so a future app's settings show up there by being added to the list of apps.

## What can be on the ring

- Kairo's items, by key: `chrono`, `ledger`, `palette`, `dashboard`, `task` (New task), `note`
  (New note), `focus` (Focus N), and new: `notes` (All notes: the pop-out's list), `week` (This
  week: Chrono's Week), `settings` (Kairo's settings), `pause` (Pause tracking / Resume tracking).
- The default, and what Reset gives: `chrono, ledger, palette, dashboard, task, note, focus`.
- Between 3 and 10 items. Saved as the setting `ring_items` (the keys, comma-separated); a saved
  list is read with unknown and repeated keys dropped, and one left with fewer than 3 is the
  default. Kept per computer, like the ring's shortcut.
- Focus's length is the setting `ring_focus_minutes` (5 to 240, default 25); the segment says
  "Focus 45" and starts that long a session.
- The ring opens on the item picked last, saved by key; one no longer on it opens on the first.

## The ring with any number of items

- Turning wraps round the items it has. 1–9 open the first nine.
- Five show at most: the lit one and two either side. With fewer than five, each shows once: the
  lit one, then those after it to the right and before it to the left, alternately, so 3 show
  -1..1 and 4 show -1..2.
- The new items' marks and lines: All notes, Ledger's ink, a stack of sheets, "In a small
  window"; This week, Chrono's ink, a week's bars, "Chrono"; Settings, Kairo's ink, sliders,
  "Kairo"; Pause tracking, Chrono's ink, two bars, "Tracking" or "Paused" (then named Resume
  tracking, a triangle).
- Windows (Rust) and the web ring (TypeScript) keep the same model and the same tests.

## Editing it

- In Kairo's settings, the App ring section gains **What's on it**: every item, the ones on the
  ring first in their order, then the rest. Each has a tick box (on the ring or not) and ↑ / ↓ to
  move it among those on. Ticking adds to the end; at 10 the unticked boxes are disabled, at 3 the
  ticked ones; the line under says why. A Reset link gives the default.
- Focus's length beside its row, as a number field (minutes).
- A preview beside the list: the arc as the ring opens on the first item, redrawn on each change.
- Each change saves at once.

## The dashboard

- A tile, **App ring**, "Kairo's apps at the pointer": its picture a small arc of the ring, its
  line the shortcut as keys and "· N items"; the picture opens the ring.
- **App settings** lists every app with settings, from the dashboard's list of apps. An app's
  settings are either on its own screen (**Open**) or a section of Kairo's settings (**Show**,
  which scrolls there):
  - Chrono: Open (as now).
  - Ledger: "Reminders and the morning reminder" — Open brings up Ledger on its Settings view.
  - Command palette: "Shortcut, suggestions and files" — Show.
  - App ring: "Shortcut and what's on it" — Show.
- `open_ledger` takes "settings" as well as a note or task; Ledger's page follows it.

## Testing

- Rust: reading the saved list (unknown, repeated, too few, too many); turning, hits, number keys
  and the shown offsets with 3, 4, 5 and 10 items; the new items' actions and lines; Focus's
  length.
- Vitest: the TypeScript ring model's same tests; the editor (tick, ↑/↓, limits, Reset, Focus's
  length, the preview redrawn); the tile; App settings rows for every app with settings, Ledger's
  opening its Settings view and Show scrolling to the section.
- By hand (device checklist): arrange the ring on Windows, macOS and Linux and use it.
