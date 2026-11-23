# Snippet keywords and command keys Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Text commands expand from a keyword typed in any app; any custom command runs from its own global shortcut.

**Architecture:** Two new nullable columns on `custom_commands` (`keyword`, `keys`). Keys register through the existing global-shortcut plugin (or GNOME/KDE on Wayland) and run the command as its palette row would. Keywords are matched by a pure buffer (`keywords.rs`) fed by a per-OS keyboard listener (`platform/typing.rs`); a match sends Backspaces and pastes through the clipboard.

**Tech Stack:** Rust (Tauri 2, rusqlite, windows-sys, CoreGraphics FFI, x11rb record), TypeScript settings page.

**Spec:** `docs/superpowers/specs/2026-11-23-snippet-keywords-and-command-keys-design.md`

## Global Constraints

- Keyword: 1–20 characters, no whitespace, unique across commands (case-insensitive), Text commands only.
- Keyboard buffer: last 32 characters, memory only, cleared on click, front-window change, caret keys.
- Listener runs only while a keyword exists. Wayland: no expansion, said in the editor.
- Keys: not the palette's, ring's, capture's or another command's; old keys stay on failure.
- Keywords sync; keys never sync (kept across a sync by command name).
- Ship to Windows, macOS and Linux.

## Review Focus

- Our own Backspaces/paste must not feed the buffer (injected events ignored, buffer cleared after expanding).
- A keyword that ends another (`;s` and `;sig`): longest match wins.
- Renaming a command keeps its keys registered under the new name.
- Removing a command unregisters its keys.
- A sync from another computer must not wipe this computer's keys.

---

### Task 1: Storage and validation (`custom_commands.rs`, `db.rs`)

- Columns `keyword TEXT`, `keys TEXT` via the column-migration list.
- `CustomCommand { keyword: Option<String>, #[serde(skip_serializing_if...)] keys: Option<String> }` — keys not in `to_json`.
- `set_keyword(conn, name, keyword: &str) -> Result<Result<(), String>>` ("" clears). `set_keys_saved(conn, name, keys)`.
- `replace_from_json` keeps local keys by name.
- Tests: keyword rules, uniqueness, text only, sync keeps keys, rename keeps both.

### Task 2: Keyword matcher (`keywords.rs`)

- `Typed { Char(char), Back, Reset }`; `Buffer::feed(&mut self, Typed, keywords: &[String]) -> Option<usize>` (index of keyword matched, longest first). Clears on match.
- Tests: match, backspace, reset, longest wins, 32-char cap, case-sensitive.

### Task 3: Listeners (`platform/typing.rs`) and expansion (`keywords.rs`)

- `platform::typing::start(send: fn(Typed)) -> bool`, `platform::typing::backspaces(n) -> bool`.
- Windows `WH_KEYBOARD_LL` + `WH_MOUSE_LL`, skipping `LLKHF_INJECTED`; macOS listen-only CGEventTap; X11 RECORD.
- `keywords::refresh(app, conn)` starts the listener once when a keyword exists; expansion fills placeholders, backspaces, writes clipboard, pastes, restores the clipboard.

### Task 4: Keys (`command_keys.rs`, `lib.rs`, `startup.rs`, GNOME/KDE)

- `register_saved(app, conn)`, `change(app, conn, name, keys) -> Result<(), String>`, `name_for(&Shortcut) -> Option<String>`, `run(app, name)`.
- `--command <name>`; Wayland through `gnome::command_shortcut` / `kde::command_shortcut`.
- Tests: `taken_by` refusal for palette/ring/capture/other command.

### Task 5: Settings UI and docs

- Each row: Keys recorder (reusing `kairo-settings.ts`'s recorder); Text rows: Keyword box.
- README and dashboard copy. Then release.
