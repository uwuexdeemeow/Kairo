# Window Switcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The palette lists open windows matching what's typed; Enter brings one forward, Ctrl+Enter asks it to close, on Windows, macOS and Linux.

**Architecture:** `platform` gains `OpenWindow`, `open_windows()`, `activate_window(id)`, `close_window(id)`, `can_close_windows()` per system, plus a pure `switchable()` filter. The palette snapshots the windows into its `Context` when it opens and adds rows and two actions. Linux Wayland goes through the GNOME extension's D-Bus methods and one-off KWin scripts.

**Tech Stack:** Rust (Tauri 2), `windows-sys` (Win32), objc2/Core Graphics + AX (macOS), `x11rb`, `zbus`, GNOME Shell JS, KWin JS.

**Spec:** `docs/superpowers/specs/2026-11-01-window-switcher-design.md`

## Global Constraints

- Section `"Windows"`; second-action label `"Close window"`; `windows` alone lists at most 8 rows.
- Window score: `max(fuzzy(title), fuzzy(app) - 20)`; window rows score below 900 (days), 1000 (focus command, tasks add) and 2000 (calculator) by never exceeding 300.
- Ids are text: Windows `HWND` as decimal; macOS `"<pid>:<CGWindowNumber>"`; X11 window id decimal; GNOME `"g<id>"`; KDE `"k<internalId>"`.
- Kairo's own windows (its process id) are never listed.
- Only Linux and macOS code paths that this Windows machine can't compile are checked by CI; push the branch after Tasks 3 and 4 and read CI before going on.
- No real window is activated or closed by any automated test, on any system.

## Review Focus

- A window title with only spaces, or a huge title (10 000 chars): filtered or truncated to 200 characters, never a palette layout break.
- The palette opened over a full-screen game: listing windows must not take focus or slow the opening noticeably (snapshot must stay under ~20 ms on Windows).
- Enter on a window that closed since the snapshot: no crash, palette closes.
- macOS without Accessibility: rows exist, Enter activates the app, no "Close window".
- GNOME before re-login (old extension without `Windows`): no Windows section, no error log spam per opening (log once).

---

### Task 1: Shared types, the filter, and the palette rows

**Files:**
- Modify: `src-tauri/src/platform.rs` (top: `OpenWindow`, `switchable`, tests in a new `#[cfg(test)] mod window_tests` that runs on every system; stubs `open_windows`/`activate_window`/`close_window`/`can_close_windows` returning empty/false on every system for now)
- Modify: `src-tauri/src/palette/search.rs` (`Context.windows`, `WindowEntry`, actions, rows, tests), `src-tauri/src/palette/mod.rs` (fill `Context.windows`; run the actions)

**Interfaces:**
- Produces:
  - `#[derive(Clone, Debug, PartialEq)] pub struct OpenWindow { pub id: String, pub title: String, pub pid: u32, pub path: Option<String> }`
  - `pub fn switchable(windows: Vec<OpenWindow>, own_pid: u32) -> Vec<OpenWindow>` — drops `pid == own_pid` and blank titles, trims titles and cuts them to 200 characters, keeps order.
  - `pub fn open_windows() -> Vec<OpenWindow>`, `pub fn activate_window(id: &str) -> bool`, `pub fn close_window(id: &str) -> bool`, `pub fn can_close_windows() -> bool`.
  - `search::WindowEntry { pub id: String, pub title: String, pub app: String, pub icon: Option<String> }`; `Context.windows: Vec<WindowEntry>`; `Context.can_close_windows: bool`.
  - `Action::FocusWindow(String)`, `Action::CloseWindow(String)`.

- [ ] **Step 1: Write the failing tests**
  - platform: `switchable_drops_kairo_and_blank_titles_and_keeps_order` — input `[a "Budget" pid 5, b "  " pid 6, c "Chrono" pid 99, d "x".repeat(300) pid 7]`, own pid 99 → ids `[a, d]`, `d`'s title 200 chars.
  - search.rs, with `ctx()` given `windows: vec![win("1", "budget.xlsx — Excel", "Excel"), win("2", "Inbox — Mail", "Mail")]`, `can_close_windows: true`, and an app "Excel":
    - `a_window_title_match_leads`: `search(&c, "budget")[0]` is the window row: section "Windows", title "budget.xlsx — Excel", sub "Excel", action `FocusWindow("1")`, second `(CloseWindow("1"), "Close window")`.
    - `an_app_name_puts_the_app_first_then_its_windows`: "excel" → first row is the Excel app, a later row is window "1".
    - `windows_lists_them_all_in_order_capped`: 12 windows; `windows` → 8 rows in "Windows", in context order.
    - `no_close_where_closing_isnt_possible`: `can_close_windows: false` → window rows have `second: None`.
    - `windows_never_beat_commands`: a window titled "focus 50"; `focus 50` → first row still the focus command; a window "12*7"; `12*7` → first row Calculator.
    - `no_windows_among_suggestions`: `suggestions(&c)` has no row in "Windows".

- [ ] **Step 2: Run them to see them fail** — `cd src-tauri && cargo test --lib palette:: platform::` → compile errors.

- [ ] **Step 3: Implement.** `mod.rs` builds `windows` from `platform::switchable(platform::open_windows(), std::process::id())`, naming each by `classify::app_name(path, exe_stem(path), title)` (or "" without a path) and using `path` as the icon; runs `FocusWindow` / `CloseWindow` through the platform functions (a false result is not an error: the palette closes). `Action::opens_window()` includes `FocusWindow`.

- [ ] **Step 4: Run all tests** — `cargo test` → all pass.

- [ ] **Step 5: Commit** — "Find open windows in the palette"

### Task 2: Windows

**Files:**
- Modify: `src-tauri/src/platform.rs` (`win` module), `src-tauri/Cargo.toml` (`windows-sys` features if `Win32_Graphics_Dwm` is missing), `src-tauri/src/lib.rs` + `startup.rs` (`--list-windows`)

**Interfaces:**
- Consumes: `OpenWindow`.
- Produces: the Windows `open_windows`, `activate_window`, `close_window`, `can_close_windows` (true); `startup::LIST_WINDOWS_ARG = "--list-windows"`, printing `id\tpid\ttitle\tpath` per window and exiting, as `--search-files` does (attach to the parent console).

- [ ] **Step 1: Write the failing tests** (`win` tests): `window_ids_round_trip` (an `HWND` value → id → back); `startup` test: `list_windows_flag_is_read`.
- [ ] **Step 2: Run them to see them fail.**
- [ ] **Step 3: Implement.** `EnumWindows` with a callback collecting windows that pass: `IsWindowVisible` or `IsIconic`; not `GetWindow(GW_OWNER)`; no `WS_EX_TOOLWINDOW`; `DwmGetWindowAttribute(DWMWA_CLOAKED) == 0`; non-empty `GetWindowTextW`; pid via `GetWindowThreadProcessId`; path via the same `QueryFullProcessImageNameW` steps `foreground()` uses (extract them into a helper `process_path(pid)`). Activate: `IsIconic` → `ShowWindow(SW_RESTORE)`, then `SetForegroundWindow`; false when `IsWindow` is false. Close: `PostMessageW(WM_CLOSE)`.
- [ ] **Step 4: Verify.** `cargo test`; then build and run `target/debug/kairo.exe --list-windows`, which only reads. Expected: this PC's open windows, Kairo's own absent, no blank titles, in about 20 ms or less.
- [ ] **Step 5: Commit** — "List, bring forward and close windows on Windows"

### Task 3: Linux (X11, GNOME, KDE)

**Files:**
- Modify: `src-tauri/src/platform.rs` (Linux section and `x11` module), `src-tauri/src/platform/gnome.rs`, `src-tauri/src/platform/kde.rs`
- Modify: `src-tauri/gnome-extension/modern/extension.js` and `legacy/extension.js` (`Windows`, `Activate`, `Close`)
- Create: `src-tauri/kwin-script/windows.js`, `activate.js`, `close.js`

**Interfaces:**
- GNOME: `Windows() -> s` (JSON `[{"id": <u64>, "title": "...", "pid": <u32>}]`, newest first by `get_user_time`), `Activate(t id) -> b`, `Close(t id) -> b`. Rust: `gnome::windows() -> Option<Vec<OpenWindow>>` (None when the method is missing), `gnome::activate(&str)`, `gnome::close(&str)`; ids `"g<id>"`.
- KDE: `windows.js` calls `Windows(json)` on Kairo's interface; `kde::windows() -> Option<Vec<OpenWindow>>` returns the last list (loading `windows.js` each call, so the next opening is fresh); `activate.js` / `close.js` filled with the id like `minimize.js`; ids `"k<internalId>"`, checked with `is_window_id`.
- X11: `_NET_CLIENT_LIST_STACKING` reversed (front first), titles as `foreground()` reads them, pid from `_NET_WM_PID`; `_NET_ACTIVE_WINDOW` (source 2, timestamp 0) and `_NET_CLOSE_WINDOW` client messages to the root.
- `platform::open_windows()` on Linux: Wayland → `gnome::windows()` or `kde::windows()`, else empty; X11 → x11. `can_close_windows()` true.

- [ ] **Step 1: Write the failing tests** (they compile only on Linux, so CI runs them): `gnome::tests::window_lists_are_read` (a JSON string → `OpenWindow`s with `g` ids; bad JSON → empty); `kde::tests::window_lists_are_read` (same with `k` ids, and an id failing `is_window_id` dropped); `x11` id round trip.
- [ ] **Step 2–4: Implement, then push and read CI** — `git push`, wait for the Linux build; Expected: build and tests pass. Fix and repeat on failure.
- [ ] **Step 5: Commit** — "List, bring forward and close windows on Linux"

### Task 4: macOS

**Files:**
- Modify: `src-tauri/src/platform.rs` (`mac` module), `src-tauri/src/platform/mac/ax.rs`

**Interfaces:**
- `open_windows`: `CGWindowListCopyWindowInfo(OPTION_ALL | EXCLUDE_DESKTOP_ELEMENTS)` filtered to layer 0, a name, and not Kairo's pid; ids `"<pid>:<number>"`; path via `process_path(pid)`.
- `ax::trusted() -> bool` (no prompt); `ax::raise(pid, title) -> bool` and `ax::close(pid, title) -> bool` find the app's AX window whose `AXTitle` equals the title and perform `AXRaise` / press `AXCloseButton`.
- `activate_window`: activate the app (`NSRunningApplication::activateWithOptions`), then `ax::raise` when trusted. `close_window`: `ax::close` when trusted, else false. `can_close_windows()` = `ax::trusted()`.
- The window's title for AX comes from the palette's snapshot: `activate_window` and `close_window` keep a small map from id to title, filled by `open_windows`.

- [ ] **Step 1: Write the failing tests** (macOS only, CI): `mac_window_ids_are_parsed` (`"501:77"` → `(501, 77)`, junk → None).
- [ ] **Step 2–4: Implement, push, read CI** — macOS build and tests pass.
- [ ] **Step 5: Commit** — "List, bring forward and close windows on a Mac"

### Task 5: Documentation

- [ ] **Step 1:** `docs/device-checklist.md`: "Palette: windows" — Windows (find a window by title, Enter brings it forward even when minimized, Ctrl+Enter closes and an unsaved document still asks, `windows` lists them, Kairo's own absent); Mac (same; then without Accessibility: Enter brings the app forward, no Close window); GNOME (after re-login) and KDE (same as Windows). `README.md`: one sentence in the command palette bullet.
- [ ] **Step 2: Commit** — "Add the window switcher to the README and the device checklist"
