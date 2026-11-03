# Palette modes: a bubble that scopes what you type

## Why

The palette only searches a site when its short word leads the query (`gh kairo`) or when nothing
else matches. Typing `github` finds the GitHub app and never offers to search GitHub. The
calculator only answers what already looks like a sum. The user wants to pick a mode on purpose and
have everything typed after it go to that mode.

## What

A **mode** is a scope the palette can be put in. Put in one, the palette shows it as a **bubble** (a
pill at the left of the input) and everything typed after it goes to that mode alone.

### The modes

| Mode | Short word | Shows while typing | Enter | Ctrl+Enter |
|---|---|---|---|---|
| Calculator | `=` | The answer, or "Not a sum yet" | Copies the answer | Copies the number |
| Search the web (Google) | `g` | "Search Google for “…”" | Opens it in the browser | |
| GitHub | `gh` | "Search GitHub for “…”" | Opens it | |
| YouTube | `yt` | "Search YouTube for “…”" | Opens it | |
| Wikipedia | `w` | "Search Wikipedia for “…”" | Opens it | |
| DuckDuckGo | `ddg` | "Search DuckDuckGo for “…”" | Opens it | |
| Google Maps | `maps` | "Search Google Maps for “…”" | Opens it | |
| Files | | Files only, up to 12 | Opens the file | Shows it in its folder |
| Windows | | Open windows only; all of them with nothing typed | Brings it forward | Asks it to close |
| Tasks | | Open tasks only; all with nothing typed; "Add task “…”" when none match | Starts 25 minutes on it | Marks it done |
| Notes | | Notes only, pinned first; "New note: …" when none match | Opens it | |

In Calculator mode the text is worked out without the check that it looks like a sum, so `pi`,
`sqrt 16` and `2^10` answer. A web mode with nothing typed shows "Type what to search GitHub for"
and does nothing on Enter. Files mode with file search turned off says so in one row, which opens
Kairo's settings.

### Getting in

1. **A mode row.** Ordinary search offers a row per mode whose name the query matches: `calc`,
   `calculator` → "Calculator"; `search web`, `web`, `google` → "Search the web"; `github` →
   "Search GitHub"; `files` → "Search files"; `windows` → "Search windows"; `tasks` → "Search
   tasks"; `notes` → "Search notes". Rows sit in a "Modes" section, below a strong app match, so
   `github` and Enter still opens the GitHub app. Enter or Tab on a mode row puts the palette in
   that mode and empties the text.
2. **A short word and a space.** Typing `gh `, `yt `, `g `, `w `, `ddg `, `maps ` or `= ` at the
   start of the query turns it into that mode's bubble at once; anything after the space stays
   typed. This replaces today's `gh kairo` row.
3. A site's name always offers its mode row next to its app, not only when nothing matches.

### Getting out

- Backspace with the caret at the start of an empty text removes the bubble.
- Esc removes the bubble; a second Esc closes the palette as now.
- Closing the palette forgets the mode.

### What stays

With no bubble, search is as today: a sum typed straight in still answers, and "Search the web for
“…”" still appears when nothing else matches.

### The bubble

A pill at the left of the input: the mode's glyph and name in the palette's accent (violet) on a
tint of it, the text after it. The input's hint becomes the mode's ("Search GitHub…", "Type a
sum…"). Drawn alike by the native Windows palette and the web palette.

## How

### Shared rules: `src-tauri/src/palette/mode.rs`

- `enum Mode { Calc, Web(usize), Files, Windows, Tasks, Notes }`; `Web` indexes `search::WEB`.
- Per mode: `name()`, `glyph()`, `hint()` (the input's placeholder), `key()` (a stable string for
  the web page, like `web:gh`) and `from_key()`.
- `shortcut(text) -> Option<(Mode, &str)>`: the mode a leading short word and space names, and the
  rest of the text.
- `rows(query) -> Vec<Item>`: the mode rows ordinary search offers.

### Search

- `search::search_in(ctx, mode, text) -> Vec<Item>` gives a mode's rows (the table above).
- `search::search` adds `mode::rows(query)` and drops `web_shortcut` (the short words are bubbles
  now).
- `Action::EnterMode(Mode)` keeps the palette open; the palettes handle it themselves.

### The palettes

- `mod.rs`: `answer` takes the mode; `wants_files` is true in Files mode for any text of two or more
  characters; Files mode keeps up to 12 file rows instead of 5.
- Native (`native.rs`): a `mode: Option<Mode>` field; typing checks `mode::shortcut`; Enter or Tab
  on an `EnterMode` row sets it; Backspace and Esc as above; the input draws the pill and offsets
  the text and caret after it.
- Web (`web.rs`, `src/palette.ts`): `palette_query` takes `mode` (its key, or null) and returns
  `mode` (the mode in force after the query, so `gh ` comes back as GitHub with the text after it)
  and `text`. `palette_run` on an `EnterMode` row returns the new mode instead of running anything.
  The page draws the pill before the input.

## Testing

- Rust: short words become modes, and only with a space; mode rows (`github` gives the app and
  "Search GitHub", the app first); each mode's rows; Calculator's "Not a sum yet" and its looser
  reading; Tasks' "Add task" and Notes' "New note" when nothing matches; web modes' empty state;
  no `gh kairo` web row in ordinary search.
- Web palette (`palette.test.ts`): choosing a mode row shows the bubble and empties the input;
  typing `gh ` turns into the bubble; Backspace on empty text and Esc leave the mode.
- The native drawing: Windows CI builds and tests it.
