# One Kairo window, and Ledger polished

## Goal

Kairo's apps feel like screens of one app: one window that changes between the dashboard, Chrono
and Ledger, with a way back. And Ledger's screens stop looking unfinished: better buttons and
fields, no empty-looking areas, steadier spacing, at any window size.

## One window

- One window, label `kairo`, one size for all screens (1120 × 780, at least 720 × 520), its size
  and place remembered once. It replaces the `main` (Chrono), `dashboard` and `ledger` windows.
- Opening an app (dashboard objects, tray, palette, ring, reminders, Chrono's Start menu entry) shows
  that app's page in the window, making it if it's closed: `show_page(app, Page)` with
  `Page::{Dashboard, Chrono, Ledger}`. A page change loads the other page in the same window
  (its address made from the window's own, so development and installed builds agree); the window's
  title becomes the app's ("Kairo", "Chrono", "Ledger"). The asks a page makes on load (where to go,
  which tab, which note) stay as they are.
- Chrono's and Ledger's sidebars start with **← Kairo**, back to the dashboard.
- The Dock rule, window-closed handling and capabilities name `kairo` instead of the three.

## Buttons and fields, everywhere

- `.btn`: 36 px, radius 10, a 1 px edge, a faint top light; hover lifts 1 px; primary in the app's
  ink. `.filter` fields: 38 px, radius 10, a filled `--raised` surface, an `--app` focus ring of
  3 px at 25%. Link buttons keep their look.

## Ledger: Tasks (cards)

- Sidebar: ← Kairo, the wordmark, Tasks and Notes with counts (open tasks; notes), Settings.
- Head: "Tasks", then a summary line: "3 due today · 1 overdue · 1h 20m focused this week" (parts
  only when non-zero; "Nothing due" otherwise).
- Add a task: a raised field with a + mark and an "Enter" key hint; under it the "what it reads"
  line, or, while empty, "Try essay draft !h fri for High priority, due Friday".
- Groups as now, each heading with a count pill.
- Each task a card: tick (round), priority dot, title, chips (due day; 🔔 time; 📝 note; focus
  time), then a ▶ Start focus that shows on hover and keyboard focus, and the Rename bubble.
- Responsive (container queries on the list): wide shows chips in full; under 560 px chips show
  icons only; under 420 px chips wrap onto a second line under the title. Titles never cut to
  nothing: at least 40% of the card.
- The task panel slides over the list from the right (an overlay, 320 px, max 90%), with a × and
  Esc to close; the list keeps its full width.

## Ledger: Notes (two panes)

- List: search, + New note, then notes as cards: title, a two-line preview of the text after the
  title, chips (tasks attached, when edited); pinned ones wear the gold ribbon; the open one lit.
- The open note on a sheet: a toolbar (attached tasks as chips, then B, I, ☐ checkbox, Pin/Unpin,
  Delete), the editor, and a footer ("Saved" / "Saving…", word count, "Edited 9:14").
- Under 760 px the list and the note take turns: the list shows until a note opens; the note has a
  ← Notes button.

## Testing

- Rust: `page_url` keeps the window's origin and swaps the page; `keep_in_dock(["kairo"])`.
- Vitest: summary line; card chips per task (due, reminder, note, time); ▶ shows only on hover
  class / focus and starts focus; the panel overlay opens and closes (×, Esc); note previews skip
  the title line; toolbar B/I/☐ act on the editor; footer says Saving… then Saved and counts
  words; narrow Notes shows list or note with ← Notes; ← Kairo calls `openDashboard`.
- By eye: the preview at 1120, 800 and 600 px wide, both themes.
- Device checklist: "One window" section.

## Out of scope

Chrono's screens beyond the new buttons and fields; the glass palette (next).
