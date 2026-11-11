# Palette: clipboard history

## Why

Copying something replaces what was copied before it, and the palette can already put text on the
clipboard (answers, calculator results) but can't get an earlier copy back. The user wants to type
a word and see the last things copied.

## What

Kairo remembers the last pieces of text copied while it runs and the palette lists them. **The
history lives in memory only**: nothing is written to disk, to a backup, to a sync or to a log, and
it's gone when Kairo quits. That is deliberate: copied text often holds passwords and tokens, and
the point of the project is to stay light.

### The command

| Typed | Rows | Enter |
|---|---|---|
| `clip`, `clipboard` | The latest 5 copies, newest first, then "Show all N copies" (when there are more) and "Clear clipboard history" | Copies the row back and closes the palette |
| `clip ` (a space after it) | The **Clipboard** mode, a bubble like Files or Notes: every copy, newest first; typing narrows them to those containing it (case ignored) | Copies the row back and closes the palette |

- A row's title is the copy's first non-empty line, cut to one line; its second line says "3m ago ·
  214 characters", or "· 4 lines" when it has more than one.
- Choosing a copy puts it on the clipboard and moves it to the top of the history, as a new copy
  would. It does not paste: the palette closes and the user pastes with the usual shortcut, since
  typing into another program is something the palette doesn't do anywhere.
- "Clear clipboard history" empties the list at once. With nothing copied yet the one row says
  "Nothing copied since Kairo started".
- **Clipboard history never appears in ordinary results.** Only a query led by `clip` or
  `clipboard` shows it, so a password copied a minute ago can't surface while typing "pas" for
  something else.

### What it keeps

- Text only. Images, files and other formats are ignored.
- The latest 25. A new copy that's already in the list moves to the top instead of repeating.
- Each copy up to 10,000 characters; longer ones are skipped, not cut, so a pasted-back copy is
  always whole.
- Anything copied by Kairo itself (an answer from the palette) is kept like any copy.

### Settings

One switch, **Remember what I copy**, in Kairo's settings (the dashboard) under the palette's,
on by default. Turning it off
stops watching and clears the list. The palette finds it as a setting, "Clipboard history", and jumps to it. It's
per computer and does not sync.

## How

### `src-tauri/src/clipboard.rs` (new)

- `History`: a `VecDeque<Entry { text, at_ms }>` behind a `Mutex`, managed as Tauri state, with
  `push(text, now)`, `recent(query, now) -> Vec<Row>`, `clear()`. Capacity and length limits are
  constants. `push` ignores empty and whitespace-only text, over-long text, and trims nothing.
- `spawn(app)`: a thread that reads the clipboard's text once a second and pushes it when it
  differs from the last text seen. No cost beyond that read; it skips the read while the setting is
  off, and it never logs what it read. Reading goes through `tauri-plugin-clipboard-manager`,
  already a dependency, so nothing new is added.
- The last text seen is remembered even when the list is cleared, so clearing doesn't bring the
  clipboard's current content straight back. What was on the clipboard when Kairo started, or when
  the history was turned on, isn't kept: it wasn't copied while Kairo was watching.
- `text_when_led(query) -> Option<&str>`: whether the query starts with `clip`/`clipboard`, and the
  rest of it. A plain function with tests.

### `src-tauri/src/palette/commands.rs`

- `clipboard_rows(ctx, query)` after the other command groups, only when `text_when_led` matches;
  `Action::CopyClip(index)`/`Action::ClearClips` run through the existing clipboard write in
  `palette/mod.rs` (`write_text`) and move the entry to the top.
- The existing rule that command rows lead when their words lead the query applies; these never
  match loosely.

### Settings

- Setting key `clipboard_history` ("1"/"0", default on). `palette/settings.rs`: the "Clipboard
  history" entry. `index.html` and `src/main.ts`: the switch. Not part of
  `choices.json`, so it doesn't sync or travel in a backup.

All shared code: nothing per system. Wayland sessions that refuse clipboard reads from a background
program simply record nothing; the command then says "Nothing copied since Kairo started", and the
device checklist gets a line for it.

## Testing

- `clipboard.rs`: the cap evicts the oldest; a repeat moves to the top and doesn't duplicate; empty,
  blank and over-long text are skipped; `recent` filters case-insensitively and orders newest first;
  clearing empties the list but the same clipboard content isn't pushed again; the first-line title
  and the ages and counts in the second line.
- `text_when_led`: `clip`, `clipboard`, `clip foo bar` match; `clipart`, `eclip` and an empty query
  don't.
- `palette/commands.rs`: the rows appear only for a led query; nothing from the history shows in an
  ordinary search; Enter on a row writes that text and moves it up.
- Settings: off clears the list and stops pushes.

## Not in this

Writing the history to disk. Pinning a copy. Images and files. Reading the system's clipboard change counter instead of the text. Pasting straight into the program
that was in front. Skipping copies from password managers (the clipboard can't tell us where a copy
came from through the plugin, so the safeguards are memory-only, a short list, never showing in
ordinary results and a switch to turn it off). Syncing the history between computers.
