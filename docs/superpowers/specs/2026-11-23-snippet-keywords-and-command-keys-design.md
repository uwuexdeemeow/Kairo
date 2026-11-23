# Snippets that expand as you type, and keys for any command

Two Raycast features Kairo lacked, built on the custom commands it already has
(`custom_commands.rs`, `src/custom-commands.ts`).

## Intent

- **Keywords:** a text command can have a keyword (`;sig`). Typing it in any app replaces it with
  the command's text, placeholders filled, without opening the palette.
- **Keys:** any custom command can have a global shortcut. An app or a built-in command gets one
  through an alias (`vs` → "visual studio code", `lh` → "left half"), so one mechanism covers all.

## Approaches

1. *Keyword is the command's name* — no new field, but a name like `sig` would fire inside
   "signature". Rejected.
2. *Separate keyword field, expand at once when typed* (Raycast's way) — chosen. `;` is suggested as a
   start so words don't collide.
3. *Expand on a space or Tab after the keyword* — fewer surprises but a different feel from Raycast
   and an extra key each time. Not chosen.

For keys: a shortcut on any palette row was considered; putting keys on custom commands (with
alias for everything else) reuses the editor, sync and storage, so it was chosen.

## Design

### Keywords (Windows, macOS, X11)

- New optional `keyword` on Text commands (1–20 characters, no spaces, unique across commands).
- A keyboard listener runs only while at least one keyword exists. It keeps the last 32 typed
  characters in memory, nothing written anywhere, and clears them on a mouse click, a change of
  front window, and on keys that move the caret (arrows, Home/End, Enter, Tab, Esc).
- When the buffer ends with a keyword: send as many Backspaces as the keyword's characters, then
  paste the filled text through the clipboard like the palette does, and restore the clipboard.
- Windows: `WH_KEYBOARD_LL` on its own thread, ignoring injected keys (our own Backspaces/paste).
  macOS: a listen-only `CGEventTap` (Input Monitoring permission; macOS itself silences it in secure
  password fields). X11: XInput2 raw key events on the root window, mapped through the keymap.
- Wayland: not possible; the editor says so and the field is hidden there.
- Keywords sync with the text they belong to (they're part of the command).

### Keys (all platforms)

- New optional `keys` on every custom command, in the shortcut plugin's format (`Ctrl+Alt+S`).
- Rejected when they're the palette's, the ring's, capture's, or another command's, or when the
  system refuses them; the old keys stay.
- Pressing them runs the command as Enter on its palette row would, without showing the palette.
  A link with `{query}` opens with nothing typed. An alias runs the first thing its words find.
  Held back while a game is in front unless Settings lets the palette open over games.
- Wayland: registered through GNOME's or KDE's own shortcuts (like the palette's), which run
  `kairo --command <name>`.
- Keys don't sync: the same keys may mean something else on another computer.

## Non-goals

- Rich text or images in snippets; per-app keywords; expansion on Wayland; a shortcut on rows that
  aren't custom commands.

## Left to the builder

The recorder widget reuses the palette shortcut's recorder in settings; exact layout of the two new
fields in the command editor.
