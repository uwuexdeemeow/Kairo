# The app ring (1.30.0)

## Goal

A small dial of Kairo's apps and three quick actions, opened by its own shortcut at the mouse
pointer, picked by turning it or dragging around it. Native on Windows (no web view, like the
palette), a small web window on macOS and Linux. In the "Objects" look.

## Opening

- Its own shortcut, default **Alt+Q** (⌥Q on a Mac), changeable in Kairo's settings beside the
  palette's (same recorder, same clash handling). Also the tray menu's **App ring**, and the
  palette's `ring` ("Open the app ring").
- Like the palette, not over a game or full-screen app unless "Open over games and full-screen apps
  too" is on (the palette's setting covers both).
- Centred on the mouse pointer and moved fully onto that screen near its edges. On Linux under
  Wayland, which doesn't tell apps where the pointer is, centred on the screen.
- The shortcut again while it's open turns it (below); Esc, a click outside, or another window
  taking focus closes it. Linux on Wayland: GNOME and KDE get a second desktop shortcut running
  `kairo --ring`, as the palette's runs `kairo --palette`.

## What's on it

Seven segments, in this order around the ring: **Chrono**, **Ledger**, **Palette**,
**Dashboard**, **New task**, **New note**, **Focus 25**. It opens on the one picked last (Chrono
the first time).

- The window is 300 × 210 px. The ring is an arc of a larger circle whose centre sits below the
  window: the lit segment at the top, its neighbours either side at 50% and 22% opacity, the rest
  out of sight.
- The lit segment stands 12 px proud, outlined in its colour, with its logo (the apps' logos; a
  plus, a pen, a small watch for the actions), its name in Fraunces, and a line:
  Chrono "2h 05m today" (or "Nothing yet today"); Ledger "3 due today" / "1 overdue" / "Nothing due
  today"; Palette its shortcut; Dashboard "Kairo"; New task "In Ledger"; New note "In Ledger";
  Focus 25 "25 minutes" (or "A session is running", dimmed, when one is).
- Turning animates over 140 ms (none with reduced motion).

## Picking

- **Hold and turn:** keep the shortcut's modifier (Alt / ⌥) held; each further press of its key
  turns one segment clockwise, with Shift anticlockwise; letting go of the modifier opens the lit
  one.
- **Tap:** if the modifier is let go within 200 ms of opening, it stays open instead.
- Either way: ← / → or the scroll wheel turn it; Enter opens; 1–7 open a segment directly.
- **Drag:** press on the ring and drag around it; it follows the pointer's angle about its centre,
  snapping to segments; letting go leaves it turned, and a click or Enter then opens the one at the
  top (a held modifier let go after a drag no longer opens it). A press and release without
  dragging (under 4 px) on a segment opens that segment.
- Opening something closes the ring and remembers the segment.

## Actions

- Chrono, Ledger, Dashboard: open their windows. Palette: opens the palette.
- New task: Ledger on Tasks with "Add a task" focused (`ledger:add`).
- New note: a new empty note, opened in Ledger.
- Focus 25: a 25-minute session (refused, with the palette's strict-mode message as a
  notification, while strict mode keeps a session); dimmed and refused while one runs.

## How it's built

- `ring/mod.rs`: the pure model, used by both: `SEGMENTS`, `Ring { lit, opened_ms, dragging }`,
  `turn(steps)`, `angle_to_lit`, `hit(x, y)`, `on_modifier_up(now) -> Pick | Stay`, `place(cursor,
  screen) -> (x, y)`; the lines (`lines(conn, now) -> [String; 7]`); `perform(app, conn, segment)`;
  settings (`shortcut`, `last`).
- `ring/native.rs` (Windows): a layered, top-most popup on its own thread drawn with Direct2D into a
  DIB (per-pixel alpha), DirectWrite with the bundled Fraunces and Instrument Sans; freed when it
  hides.
- `ring.html`, `src/ring.ts`, `src/ring-model.ts` (macOS, Linux): a transparent, undecorated
  window made when first needed and dropped when closed; the same model in TypeScript.
- `lib.rs`: two global shortcuts, told apart in the handler; `Launch::Ring` and `--ring`; the tray
  item. `platform` GNOME/KDE: the palette's desktop-shortcut code takes a name, an argument and an
  action id, so the ring gets its own.
- Settings: the dashboard's Kairo settings get "App ring" with a shortcut recorder.

## Testing

- Rust (`ring`): turning wraps both ways; the angle under a point picks the right segment and
  snaps; hits outside the ring are none; a modifier let go at 150 ms stays, at 250 ms picks;
  placement keeps the window on screen at every edge and corner; lines for each segment (empty day,
  due counts, running session); `last` remembered; startup's `--ring`; the GNOME accelerator for
  `Alt+Q`.
- Vitest (`ring-model.ts`): the same cases as the Rust model, so both agree; the web ring draws
  seven segments with the lit one first, turns on arrows, opens on Enter.
- By eye: a preview of the web ring in a headless browser; the native ring in a small example
  program that draws it to a PNG (no window shown).
- `docs/device-checklist.md`: a "Ring" section for Windows and a Mac.

## Out of scope

Choosing what's on the ring, more than seven segments, opening other apps, sounds.
