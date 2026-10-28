# Kairo dashboard

Date: 29 October 2026. Status: agreed in chat, written up for review.

## Why

Kairo is growing from "a command palette plus Chrono" into a home for several apps. Today the
tray icon opens Chrono, and Chrono's Settings page holds everything, including settings that
belong to all of Kairo (theme, palette shortcut, start at login, updates). There needs to be a
place above the apps: a dashboard that lists them and holds Kairo-wide settings, while each app
keeps its own.

## What the user asked for, and what was decided

- Right-clicking the tray icon offers **Open dashboard**; double-clicking it opens the dashboard.
- The dashboard lists every app in Kairo: Chrono now, more later.
- The command palette's settings move to the dashboard's settings.
- Chrono's own settings: decided to **stay inside Chrono** (see below).
- The tray menu has too many items: cut to five.

## Tray icon

- Menu, in order: **Open dashboard**, **Open Chrono**, **Command palette**, **Pause tracking**,
  **Quit Kairo**.
- Removed from the menu, and where they live instead: Focus session (Chrono's sidebar and Focus
  page, the palette), Focus nudges (Chrono's settings), Start with Windows / Open at Login / Start
  at login (Kairo's settings), Check for updates (Kairo's settings; updates also install on their
  own).
- A left click, single or double, opens the dashboard on Windows and macOS. (A double-click always
  begins with a single click, so the single click can't open something else.) Linux trays don't
  report clicks, so there the menu's first item is the way in.
- The tray no longer has check items, so the code that kept them in step with the in-app toggles
  (`NudgeToggle`, `AutostartToggle`) goes.

## Launching

- Opening Kairo from Start, Spotlight or a launcher still opens the command palette (`--palette`
  or no argument). Chrono's own entry still opens Chrono (`--chrono`). Signing in still starts
  quietly (`--minimized`).
- New `--dashboard` argument opens the dashboard, for a running copy and a fresh one alike.
- The palette gets a **Dashboard** item (and "settings" finds Kairo's settings there).

## The dashboard window

A new window, label `dashboard`, title "Kairo", made when opened and freed when closed, like
Chrono's window. Smaller than Chrono's (about 860 × 620, minimum 560 × 440). Its own page,
`dashboard.html` with `src/dashboard.ts`, sharing `styles.css`.

Two tabs:

### Home

A tile per app:

- **Chrono**: "Screen time tracker", a live line for today ("5h 10m on screen · 72%
  productive", or "Paused" / "Nothing tracked yet today"), and **Open** and **Settings**.
- **Command palette**: its shortcut as keys, and **Open**.

Apps come from one list in `dashboard.ts`, so a future app is one entry: name, line, how to open
it, and where its settings are.

### Settings

Kairo-wide settings, moved here from Chrono's Settings page unchanged in behaviour:

- Appearance (theme)
- Command palette (shortcut, suggestions, over full screen)
- Start with Windows / Open at Login / Start at login
- Your data (export, back up, restore)
- Combine computers
- Updates (version, Check for updates)
- **App settings**: a row per app with what its settings cover and **Open**, which opens the app
  on its Settings page ("Chrono: goals, privacy, projects, focus blocking…").

## Chrono's settings stay in Chrono

Chrono keeps its Settings page with only its own sections: Goals, Privacy, Projects, Calendar,
Browser extension, Focus nudges, Evening summary, Focus blocking, Breaks, Demo data. A line at the
top links to Kairo's settings for theme, palette, startup, data and updates.

Why there rather than a tab in the dashboard: goals, limits and privacy are changed while looking
at Chrono, beside Categories; and each app owning its settings scales, since a future app brings its
own page plus a link from its tile and from App settings, instead of one page growing with every
app.

## Shared code

`main.ts` holds the helpers (`h`, `$`, durations), the theme, the palette shortcut recorder,
startup, updates, data and combining code. Those move into modules both pages import:

- `src/ui.ts`: DOM and formatting helpers, the theme.
- `src/kairo-settings.ts`: the sections that move to the dashboard.

No stored data changes: every setting keeps its key and its command.

## Windows talking to each other

- The dashboard opens Chrono (optionally on its Settings page) through a command that shows Chrono
  and hands it a navigation, as the palette already does (`take_navigation`).
- The theme is now changed only in the dashboard. Changing it sends a new `kairo:appearance`
  event so an open Chrono window repaints at once; the palette already reads the theme each time
  it opens.
- macOS: Kairo shows in the Dock while either window is open (`in_dock`).

## Testing

- Rust: launch arguments (`--dashboard` opens it; the others unchanged), and which tray clicks open
  the dashboard, as plain functions with unit tests.
- Frontend: `tsc --noEmit`, `npm run build` (now with a third page), and headless screenshots of
  the dashboard's Home and Settings and Chrono's trimmed Settings against a stand-in backend.
- CI builds for macOS and Linux.
- Nothing run on the user's computer: tray behaviour on a real machine is for the user to try.

## Out of scope

- Moving Chrono's settings into the dashboard, or merging the windows.
- New apps.
- Changing what opening Kairo from Start does.
