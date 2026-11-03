# Kairo's new look: "Objects" (1.29.0)

## Goal

Make Kairo's apps feel like apps, each with its own character, inside one family, without
weighing Kairo down: typefaces, colours, shapes, inline drawings and a little CSS motion. No
image files, no animation or UI libraries. Behaviour, layouts and wording stay as they are,
except the dashboard's greeting and empty states. The app ring is a separate spec (1.30.0).

## Type

- **Fraunces** (variable, Latin) for display: page titles, greetings, big numbers, app names.
- **Instrument Sans** (variable, Latin) for text and controls.
- **Geist Mono** stays for the note editor, keys and tabular figures.
- Geist Sans is dropped. Fonts come from `@fontsource-variable/*`, bundled; each page imports
  `src/fonts.ts`, which imports the three. The Windows palette embeds Instrument Sans and Fraunces
  instead of Geist (Instrument Sans only; see Palette).

## Colour

Kairo follows the system's light/dark setting (and Settings' choice), as now.

| Role | Light ("paper") | Dark ("deep blue ink") |
|---|---|---|
| `--bg` | `#f4efe6` | `#141a24` |
| `--raised` (sheets) | `#fffaf2` | `#1b2230` |
| `--ink` | `#2b2622` | `#e3e7ee` |
| `--ink-2` | `#4a423b` | `#c4cbd6` |
| `--muted` | `#6e6358` | `#97a1b2` |
| `--faint` | `#8a7f72` | `#7a8598` |
| `--rule` | `#e3d9c9` | `#232c3d` |
| `--line` | `#d3c7b3` | `#33405a` |
| `--track` | `#e6dccb` | `#232c3d` |

App inks (`--chrono`, `--ledger`, `--palette`, `--kairo`):
light `#2f6f8f`, `#a35a1f`, `#6a4c93`, `#7a6f63`; dark `#7fc4e8`, `#e8a86a`, `#b9a2e6`, `#a7b0bf`.

Each page sets `--app` to its app's ink (dashboard and blocked window: Kairo's; Chrono's window:
Chrono's; Ledger's; the palette's), and accents (buttons marked primary, links, focus rings,
selection, the active tab) use `--app`. The data colours (`--productive`, `--distracting`,
`--neutral`, `--seq-*`, `--good`, `--rec`) keep their meaning and stay as they are, so charts read
the same. A faint paper grain (an inline SVG noise image, about 0.4 kB) lies over `--bg`.

## Shape and motion

- Sheets: `--raised` panels with a 1 px `--rule` edge and a soft 1 px shadow, radius 10 px; lists
  as ruled lines; section labels in small capitals with letter-spacing.
- Titles and greetings in Fraunces 800 with an italic 400 aside.
- Motion (`--ease: cubic-bezier(.2,.8,.2,1)`, 160–320 ms): views and panels settle in (fade and
  4 px rise); hover lifts objects 2 px; a ticked task's title gets an ink stroke drawn across it
  (`--app`) before it moves; dials fill on first draw. All off under `prefers-reduced-motion`.

## The logos

Inline SVG symbols in `src/logos.ts` (`chrono`, `ledger`, `palette`, `kairo`), drawn in the app
inks, used by the dashboard, sidebars' wordmarks and the blocked window:

- **Chrono:** a watch dial; its ring can show a share (`dial(share, parts)`).
- **Ledger:** a cloth notebook with a gold ribbon (`#d9b23f`).
- **Palette:** a keycap showing a shortcut.
- **Kairo:** a small "k" in a ring.

## The apps

### Dashboard

- Head: the date in small capitals, then a greeting in Fraunces by local hour: 5–12 "Good
  morning, *let's begin.*"; 12–17 "Good afternoon, *keep going.*"; 17–22 "Good evening, *nearly
  done.*"; 22–5 "Still up? *Go easy.*" (`greeting(hour)`).
- Home: the three apps as objects in a row, each a button: Chrono's dial filled with today's
  productive (blue), neutral and distracting shares of active time, "2h 05" in its middle; Ledger's
  notebook; the palette's keycap with its real shortcut. Under each: name in its ink, and the line
  it shows today. Hover lifts, click opens; Settings stays a link under Chrono's.
- Settings tab: unchanged controls on one sheet.

### Chrono

- Sidebar wordmark: the dial logo and "chrono" in Fraunces.
- Today: the hero number ("2h 05m") in Fraunces 800; beside it a day dial (`dayDial`) whose ring
  is the day's active time by category share.
- Focus: the timer face becomes a watch face (an SVG ring that unwinds as the session runs, the
  time in Fraunces in its middle); at the end the ring pulses once.
- Charts, tables and settings: new colours and type only.

### Ledger

- Wordmark: the notebook logo and "ledger".
- Lists and the note sheet on ruled paper with a faint margin rule (`--ledger` at 25%).
- Ticking a task: the ink stroke, then it moves to Done (after 280 ms; at once with reduced
  motion). Overdue rows: a small red ribbon at their left edge. Pinned notes: a gold ribbon on the
  row.
- Empty states: a small notebook drawing above the text.

### Palette

- Web (macOS, Linux) and native (Windows): paper or deep-blue-ink sheet, violet accents, the
  search field as a raised key, section headings in small capitals, the selected row lit in
  `--palette` at 18%. Opening settles in over 120 ms (web; the native one appears at once).
- Windows (`palette/native.rs`): its `DARK` / `LIGHT` themes take the new values and a `palette`
  accent; Instrument Sans replaces Geist for text; section labels in small capitals, as on the web
  (so it needs no Fraunces, and stays lighter).

### Blocked window

Chrono's watch face with the time left, and one Fraunces line: "Back to *<label>*" with "<n>
minutes left" under it (as today's text, restyled). Buttons unchanged.

## Weight

Fraunces (Latin, variable wght+opsz) and Instrument Sans (Latin, variable) add about 120 kB to the
installer; Geist Sans's 29 kB goes. Page code grows by the logos (~3 kB) and styles. No runtime
cost beyond the fonts' first load.

## Testing

- Vitest: `greeting(hour)` at 4, 5, 11, 12, 16, 17, 21, 22; `dialArcs(totals)` (shares, empty day,
  rounding sums to the ring); the dashboard objects (Chrono's dial shares and centre text,
  Ledger's line, the keycap's shortcut keys, opening on click); a ticked task waits 280 ms before
  `done` (and not with reduced motion); `--app` set per page.
- Rust: the native palette's fonts are the bundled WOFF2 files (a new test checks each embedded
  font starts with the WOFF2 signature), and its two themes keep enough contrast between ink and
  paper (a new test over `DARK` / `LIGHT`: ink 7:1, muted 4.5:1, accent 3:1).
- By eye: screenshots of each window in light and dark on Windows, after asking.
- `docs/device-checklist.md`: a "New look" section.

## Out of scope

The ring (1.30.0), custom themes, sounds, installer icons, new chart forms, layout changes.
