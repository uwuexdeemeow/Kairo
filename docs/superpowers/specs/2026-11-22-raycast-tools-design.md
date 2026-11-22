# Raycast-style tools in the palette

Built in 1.32.0 from a list the owner approved ("build the features you mentioned"): the parts of
Raycast that are used every day, minus AI chat, the extension store, and rarely used extras (color
picker, dictionary, translate, system settings search). Each works on Windows, macOS and Linux
where the system allows it, and says plainly where it doesn't.

## What was already there

Clipboard history (text, in memory, password managers' copies skipped), your own commands (link,
path, text), unit conversion in the calculator. These were extended rather than rebuilt.

## What it adds

| Feature | Typed | Enter | Where |
|---|---|---|---|
| Paste into the window you were in | (rows below) | pastes; Ctrl+Enter copies | `platform::paste`, `palette/mod.rs` `paste_after_hiding` |
| Clipboard: source app, pins | `clip`, `clip chrome` | pastes; Ctrl+Enter pins/unpins | `clipboard.rs` (table `clipboard_pins`) |
| Quicklinks | `jira 123` for a link with `{query}` | opens it filled | `custom_commands.rs` `fill_link` |
| Snippets | a text command | pastes with `{date}`, `{time}`, `{clipboard}` | `custom_commands.rs` `expand`, `Action::Snippet` |
| Aliases | `vs` | runs what "visual studio code" finds first | `Kind::Alias`, `palette/commands.rs` `alias_row` |
| Window snapping | `left half`, `maximize`, `next display`, `restore`… | moves the window you were in | `snap.rs`, `platform::{window_rect, place_window, work_areas}` |
| Layouts | `save layout x`, `layout x` | saves / puts back; a focus label restores it | `snap.rs` (table `window_layouts`), `focus::insert` |
| Currency | `20 usd in eur` | copies the answer | `currency.rs`, fend's exchange-rate handler |
| Emoji | `:fire`, `emoji fire` | pastes; most used first | `palette/emoji.rs`, `emoji.txt` (table `emoji_uses`) |
| Scripts | a script's `@kairo.title` | runs it; output notified, copied or pasted | `scripts.rs` |

## Calls made

- **Paste presses the system's keys** (SendInput, XTEST, CGEvent) after the palette hides and the
  window before it is brought back. Wayland forbids it and a Mac needs Accessibility: there the
  text stays on the clipboard and a notification says to paste. No system-wide keyboard hook:
  snippets are inserted from the palette, never expanded as you type.
- **Pins are the only clipboard text written to disk**, by an explicit choice per copy; they never
  sync. The rest stays in memory, as before.
- **Currency rates are downloaded only once a conversion is typed**, then at most daily, from the
  ECB's public file (no key, nothing sent but the request). The answer names the rates' day.
- **Restore** is per window and per run of Kairo: it goes back to where the window was before Kairo
  first moved it.
- **Layouts match windows by app path, then title**, each open window used once; apps not open are
  left alone (Kairo doesn't start them).
- **Scripts** run only on Enter, in the scripts folder, by an interpreter chosen from the ending,
  without a console window on Windows, and are stopped after a minute. They never sync. The output
  shows in a notification rather than in the palette, which has closed by then.
- **New glyphs** (window, smile, script, link) are drawn in both palettes; the native palette now
  draws colour emoji (`D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT`).

## Not in this

Images in the clipboard history. Expanding snippets as you type. Running a script with its output
in the palette. Moving windows on Wayland (the GNOME extension could learn it later). Starting a
layout's apps that aren't open.

## Not yet seen

Nothing here has been used on a real machine; the cloud container that built it has no display, and
its network can't reach the ECB. `docs/device-checklist.md` step 20 covers each feature.
