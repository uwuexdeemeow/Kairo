# A note, popped out

## Goal

Keep a Ledger note in view while working in other apps: pop it out of Ledger into a small window
of its own, edit it there, and keep it above other windows if you like. One note at a time, so
Kairo stays light.

## The window

- One window, label `note`, 400 × 540 at first (at least 260 × 200), its size and place
  remembered by the window-state plugin like Kairo's window. It is never kept while closed: a
  closed pop-out costs no memory.
- Popping out a note opens the window on it; with the window already open on another note, the
  window switches to this one (the page is told, it doesn't reload). Never a second window.
- It lives apart from Kairo's window: closing Kairo's window, or changing its page, leaves the
  pop-out open. On a Mac, Kairo stays in the Dock while either window is open.
- It floats above other windows when 📌 is on: on at first, the choice remembered in the setting
  `note_window_on_top`.
- Opened from a **Pop out** button in the open note's toolbar in Ledger.

## Two editors, one note

- Saving is as now: half a second after typing stops. A window also saves at once when it loses
  focus, so the text is in the other window before you can type there.
- `save_note`, `pin_note` and `remove_note` tell every window `kairo:note` with the note's id, the
  label of the window that changed it, and whether it's gone.
- A window showing that note, other than the one that changed it, takes the new text when it has
  nothing unsaved, keeping the caret where it was (clamped to the new length). While it has
  unsaved typing it keeps its own; its save follows a moment later.
- Changes synced from another computer arrive as `kairo:sync`, as now; the pop-out reloads its
  note on it under the same rule.
- A note deleted anywhere closes the pop-out.

## The pop-out page

- `note.html`, `src/note-window.ts`. Window title: the note's title (its first line), kept up to
  date as it's renamed.
- A bar: B, I, ☐ on the left; 📌 (float on top, pressed when on) and ↗ (open in Ledger) on the
  right. The system's own title bar closes it.
- Ledger's editor (`mountEditor`), its font a step smaller.
- A footer: "Saving…" / "Saved" and the word count, as in Ledger.
- The theme followed as Kairo's other pages do; Ledger's ink. Esc does nothing.
- A note that can't be found (deleted before the window loaded) closes the window.

## Commands

- `pop_out_note(id)`: opens the window on the note, or switches it (`kairo:note-open` with the id),
  then shows and focuses it.
- `popped_note()`: the id the window is on, asked by the page once it's loaded.
- `note_on_top()` / `set_note_on_top(on)`: read and set floating; setting it applies to the open
  window at once.
- ↗ uses `show_ledger_at("note:<id>")`.

## Everywhere

- Windows, macOS and Linux alike; `always_on_top` is Tauri's on all three. Over full-screen games
  on Windows it may not show; that's fine for a glance window.
- Capabilities gain `note`; `keep_in_dock` counts `note` as well as `kairo`; Vite builds
  `note.html`.

## Testing

- Rust: `keep_in_dock` with only `note` open.
- Vitest (`note-window.test.ts`): loads the note given and titles the window; saves after typing
  and at once on blur; an outside change is taken when idle, caret kept; an outside change is
  ignored while typing is unsaved; its own change is ignored; a deleted note closes the window;
  📌 calls `setNoteOnTop` and shows pressed; ↗ calls `openLedgerAt("note:<id>")`; switching to
  another note saves the first.
- Vitest (Ledger): Pop out calls `popOutNote` with the open note; an outside `kairo:note` reloads
  the open note when idle.
- By hand (device checklist): pop out, type in each window in turn, float on and off, close and
  reopen (size and place kept), delete the note in Ledger. No focus session involved.
