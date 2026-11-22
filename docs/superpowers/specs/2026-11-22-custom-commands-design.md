# Your own palette commands

A name you choose that does something when typed in the palette. Built in 1.31.71 without a
design conversation: the owner asked to "just dive in", so every call below is mine, made from how
the palette and focus modes already work, and open to change.

## Intent

The palette's modes (web searches, Calculator, Files) are fixed. The owner wanted words of their
own: a short thing typed once a day that opens a place or pastes something, so the palette stays
the quickest way to reach anything. The reason behind it was not given.

## What it is

Three kinds, each an existing palette action, so neither palette (web or Windows native) needed
new drawing or new action handling:

| Kind | Does | Action reused |
|---|---|---|
| Link | opens a web address | `OpenUrl` |
| Open | opens a file, folder or app by path | `OpenFile` |
| Text | copies text, line breaks kept | `Copy` |

Typing a name, or its start from the first letter, shows its row under "Commands" with a line
saying what Enter does. An exact name leads; a name only begun ranks just below it; two letters
find it loosely like the actions. The folder kind draws the file's own icon.

## Goals

- Quick: no dialog, no confirmation, nothing between the name and the result.
- Nothing new in the shipped program but a table, a module and a settings section: no dependency.
- A link can't start another program: only `http`, `https` and `mailto` are kept, and a bare site
  gets `https://`.

## Non-goals and calls left out

- **Links and text sync, paths don't** (added in 1.31.72). They travel as the `commands:custom`
  choice like the focus schedule does: the other computer's links and texts replace the shared
  ones here, its paths are never taken, and a path made here keeps its name. What arrives is held
  to the same rules as what is typed. A Kairo that doesn't know the choice leaves it waiting.
- **A file and folder picker** (also 1.31.72) fills a path from the system's dialog.
- **No arguments, no scripts, no running a program with options.** "Open" opens; it does not run
  a command line. That would be a different feature with a different risk.
- **Names are not unique across the palette.** A command named like an app ranks first when typed
  outright; the app is still found by other letters.

## Limits

50 commands, names up to 30 characters, targets up to 2,000. A restore brings them back; an older
backup without the table leaves what is there.

## Where

`src-tauri/src/custom_commands.rs` (table `custom_commands`, rules, tests), the rows in
`palette/commands.rs` (`custom_rows`), three Tauri commands, `src/custom-commands.ts` and the
"Your commands" section of Kairo's settings (`dashboard.html`), found by the palette's settings
search ("Your commands").

## Not yet seen

Nothing here has been used on a real machine. `docs/device-checklist.md` step 18 covers it, and
asks about an app's path on Linux and a Mac, where `OpenFile` hands the path to the system.
