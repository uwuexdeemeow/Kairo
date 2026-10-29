# Files and settings in the command palette

Date: 29 October 2026. Status: agreed in chat, written up for review.

## Why

The command palette opens apps, jumps around Kairo and answers questions about your time. Two
things people reach for it to do aren't there yet: finding a file, and getting to one setting.
Today "theme" finds a single "Kairo settings" row and "settings" a single "Chrono settings" row,
each opening a whole page; a file can't be found at all.

## What the user asked for, and what was decided

- Search for files from the palette, on Windows, macOS and Linux.
- Search Kairo's and Chrono's settings from the palette.
- Files come from **the system's own index**, not a crawl of Kairo's own (chosen over Kairo's own
  name index, recent files only, and whole drives).
- File results show **below the rest, a few at a time**, arriving a moment after the other
  results (chosen over a prefix, and over mixing them in by score).
- Choosing a setting **jumps to it, and on/off switches flip in place** (chosen over jumping
  only).
- Results arrive **in two steps** (approach A): typing never waits for the file index (chosen over
  waiting inside each search with a timeout, and over Kairo caching the index's names).

## Keys

The same for every row:

- **Enter** does the row's main thing: opens an app or file, flips a switch, jumps to a setting,
  runs an action.
- **Ctrl+Enter** (**⌘+Enter** on a Mac) shows where it lives: a file in its folder, a switch on
  its settings page. Rows without a second action ignore it.
- The palette's footer names the Ctrl/⌘+Enter key whenever the selected row has a second action
  ("Show in folder", "Open in settings").

## File search

### What it finds

- File and folder **names**, not contents, under the user's **home folder**.
- Left out: anything under a hidden folder or named with a leading dot; `AppData` on Windows and
  `~/Library` on a Mac; build clutter (`node_modules`, `.git`, `target`, `__pycache__`).
- Only once **2 or more characters** are typed.
- **Up to 5** rows, under a **Files** heading, after every other section.
- Ranked: the name starts with the query, then a word in the name starts with it, then the name
  contains it; ties go to the most recently modified. Duplicates (the same path from two sources)
  show once.
- Matching ignores case, and accents where the system's index does.

### Where it asks, per system

Each system's code takes a query and a time limit and gives back paths; nothing more. Filtering,
ranking and building rows is shared.

- **Windows:** Windows Search, through a `search-ms:` shell query
  (`search-ms:query=<q>&crumb=location:<home>`), whose results are read as shell items. No admin
  rights, no install. If the spike (see Order of work) finds it unusable, the fallback is Windows
  Search's own query interface (`ISearchQueryHelper` with OLE DB).
- **macOS:** Spotlight, by running `mdfind -onlyin ~ -name <q>`, read line by line and stopped
  after enough results or the time limit.
- **Linux:** the first that's installed:
  1. localsearch / tracker (GNOME's indexer), through its command line (`localsearch search` or
     `tracker3 search --files`);
  2. `plocate`, then `locate`, case-insensitive, limited to paths under the home folder.

  With none installed, one row says so once per palette opening ("File search needs localsearch
  or plocate"), choosing it does nothing more.

### Timing

- One background thread does all file searches. Each query is handed to it; a newer one replaces
  an older one that hasn't started, so it only ever works on the latest.
- It waits **about 120 ms** of quiet before asking, so typing a word asks once.
- A search that takes longer than **1.5 s** is dropped; nothing shows for it.
- Results are tagged with the query they belong to (the session's `seq`). Results for anything
  but the current query are thrown away, so the rows never change under a press of Enter for
  another query.

### Rows

- Title: the file's name, the matched letters underlined.
- Second line: its folder, from `~` (`~/Documents/Taxes`).
- Icon: the file's own icon on Windows and macOS, through the code the app icons already use;
  on Linux a glyph by kind (document, image, folder, other).
- Enter opens it with its default app; Ctrl/⌘+Enter shows it in Explorer, Finder or the file
  manager (on Linux, through the file manager's `ShowItems` call where there is one, else by
  opening its folder).

### Switch and privacy

- A **Search files** switch in Kairo's settings, under Command palette, on by default.
- File names are only shown. They are never saved, tracked, put in the database or synced.

## Settings search

### One list

A new `palette/settings.rs` lists every setting once. Each entry has:

- an id, its title ("Evening summary") and its page ("Chrono settings" or "Kairo settings"),
  shown as the second line;
- the words it's found by ("evening summary recap end of day notification time");
- where it lives: the window (Chrono or the dashboard) and its control's element id
  (`#evening-enabled`);
- for switches, how to read and change its state.

The settings rows search finds today ("Kairo settings", "Chrono settings", Export, Backup,
Restore, Check for updates) fold into this list. The four actions keep doing what they do today
on Enter (each asks through its file dialog or notification first), and gain Ctrl/⌘+Enter to jump
to their place in the settings. Rows show under a **Settings** heading.

### What's in it

Every section of both pages, and individual controls where they have names of their own:

- **Chrono:** daily productive goal, daily distracting limit, weekly productive goal, weekly
  distracting limit, keep time for, delete time, projects, calendar, browser extension, focus
  nudges, evening summary, focus blocking, block launchers too, strict blocking, breaks, demo
  data.
- **Kairo:** theme, palette shortcut, palette suggestions, open over full screen, search files,
  start at login, export time, back up, restore a backup, combine computers, check for updates.

### Jumping

Choosing a setting that isn't a switch, or Ctrl/⌘+Enter on any setting:

1. The palette closes. Chrono opens on Settings through the navigation it already uses
   (`Navigation`), which gains an optional `target` element id; the dashboard opens on its
   Settings tab, the asked-for tab carrying the id (`settings#palette-suggestions`).
2. The page scrolls the control into view, focuses it (through `shownControl`, so a dropdown's
   button gets it) and highlights its section for about 1.5 s.

### Switches

These show **On** or **Off** on the right, and Enter flips them:

- Focus nudges
- Evening summary (keeps its time)
- Focus blocking
- Block launchers too
- Palette suggestions
- Open over full screen
- Search files
- Start at login

After a flip the palette stays open and the row shows its new state. The change goes through the
same code the settings pages use. Kairo then emits `kairo:settings`, and an open Chrono or
dashboard window reloads its controls.

Found, but only jumped to: **strict blocking** (it can lock things for a whole session and can't
be undone mid-session) and **demo data** (it adds or removes data).

## In the code

### Shared (`src-tauri/src/palette/`)

- `files.rs`: the file-search thread, filtering, ranking, rows; `files/windows.rs`,
  `files/macos.rs`, `files/linux.rs` ask the index.
- `settings.rs`: the list, its search, reading and flipping switches.
- `search.rs`: `Action` gains `OpenFile(path)` and `Toggle(id)`, and the settings jump
  (`Setting(id)`); `Item` gains an optional second action (`RevealFile(path)`, `Setting(id)`) and
  an optional state ("On"/"Off"); `Icon` gains `File(path)`.
- `mod.rs`: `Session` keeps each query's file rows next to its other rows; `perform` runs the new
  actions; the new switch is stored with the palette's other settings.

### Windows palette (`native.rs`)

File rows arrive as a new window message, as the palette is already told to open. It draws the
Files section and the On/Off text, handles Ctrl+Enter and its footer hint, and repaints a flipped
row in place.

### macOS and Linux palette (`web.rs`, `src/palette.ts`)

File rows arrive as a `palette:files` event naming their `seq`; the page appends them under
Files, or ignores them when a newer query has replaced theirs. Keys, footer and flips work as in
the Windows palette.

### Chrono and the dashboard

- `Navigation` gains `target`; `src/main.ts` scrolls, focuses and highlights it after showing the
  view.
- The dashboard's requested tab may carry `#id`; `src/dashboard.ts` does the same.
- Both listen for `kairo:settings` and reload their settings controls.

## Errors

- No index, or it fails: no Files section; typing is never held up. On Linux, the "needs
  localsearch or plocate" row, once per opening.
- Too slow: dropped after 1.5 s; the next query starts fresh.
- A file moved or deleted since it was found: "Couldn't open it: it was moved or deleted." in the
  palette.
- A flip that fails (Start at login in a development build): the row keeps its state and shows
  why on its second line.

## Testing

- **Rust**
  - Ranking and filtering: hidden and cluttered folders left out, home folder only, duplicates
    once, newest first on ties.
  - Reading each system's output (`mdfind`, plocate/locate, tracker), from saved samples.
  - The thread: an older query's rows are dropped; the time limit holds.
  - The settings list: every entry found by its title and words; switches read and flip.
  - Every control id in the list exists in `index.html` or `dashboard.html`, so the list can't
    fall behind the pages.
- **Vitest**
  - The web palette: file rows arriving late, rows for an old query ignored, Ctrl/⌘+Enter, a
    switch row updating after a flip.
  - Chrono's and the dashboard's jump: scroll, focus, highlight.
  - Both reloading on `kairo:settings`.
- **On real machines:** new steps in `docs/device-checklist.md` for each system: a file in
  Documents is found, Ctrl/⌘+Enter shows it in its folder, Start at login flips from the palette.

## Order of work

1. A spike: the Windows `search-ms:` query, read as shell items, fast enough and complete enough.
   If not, the fallback above.
2. Settings search and switches (no system risk).
3. File search: macOS, then Linux, then Windows.
4. One release, 1.20.0, built for all three systems.

## Out of scope

- Searching inside files.
- Files outside the home folder, or on other drives.
- Kairo's own index of files.
- Flipping settings that aren't on/off switches (goals, theme, times) from the palette.
