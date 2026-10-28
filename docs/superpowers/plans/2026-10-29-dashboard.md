# Kairo Dashboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A "Kairo" dashboard window, opened from a trimmed tray menu and tray clicks, that lists Kairo's apps and holds Kairo-wide settings, while Chrono keeps only its own settings.

**Architecture:** A second Tauri window (`dashboard`, page `dashboard.html` + `src/dashboard.ts`) made on demand and freed on close, like Chrono's `main`. Window opening in `lib.rs` becomes generic over the label. The Kairo-wide settings code moves out of `src/main.ts` into `src/kairo-settings.ts`, with shared helpers in `src/ui.ts`; both pages import them. No stored data changes.

**Tech Stack:** Rust (Tauri 2, rusqlite), TypeScript + Vite (no framework), one shared `styles.css`.

**Spec:** `docs/superpowers/specs/2026-10-29-dashboard-design.md`

## Global Constraints

- Tray menu, in order, exactly: "Open dashboard", "Open Chrono", "Command palette", "Pause tracking", "Quit Kairo" (separators allowed around "Pause tracking").
- Left click (single or double) on the tray icon opens the dashboard on Windows and macOS.
- Launch arguments: `--palette` or none opens the palette; `--chrono` opens Chrono; `--minimized` starts quietly; new `--dashboard` opens the dashboard. Same for a second launch reaching the running copy.
- Dashboard window: label `dashboard`, title "Kairo", `create: false`, `visible: false`, 860 × 620, minimum 560 × 440, background `#0e0f12`, `dragDropEnabled: false`.
- Dashboard tabs: "Home" and "Settings". Home tiles: Chrono ("Screen time tracker", live line, Open, Settings) and Command palette (shortcut keys, Open).
- Chrono tile line: "{on screen} on screen · {n}% productive" from today's `get_day`; "Paused" while tracking is paused; "Nothing tracked yet today" with nothing.
- Kairo settings sections, in order: Appearance, Command palette, Start with Windows (its name per OS from `startup::LABEL`), Your data, Combine computers, Updates, App settings.
- App settings row for Chrono: "Chrono" with "Goals, privacy, projects, calendar, browser extension, nudges, evening summary, focus blocking, breaks and demo data", and "Open".
- Chrono's Settings page keeps only: Goals, Privacy, Projects, Calendar, Browser extension, Focus nudges, Evening summary, Focus blocking, Breaks, Demo data; first line: "Theme, the command palette, starting at sign-in, your data, combining computers and updates are in Kairo's settings." with a link "Open Kairo's settings".
- The sidebar's "whose time" computer picker stays in Chrono.
- Every setting keeps its stored key and its existing command.
- Ships on Windows, macOS and Linux; never run Kairo on the user's computer.

## Review Focus

- Opening the dashboard twice in quick succession (double-click, then the menu) must make one window, not two. Test in Task 2.
- Choosing Settings on Chrono's tile while Chrono is closed must land on Chrono's Settings page, not Today. Test in Task 2 (navigation stored before the window exists) and checked by screenshot in Task 6.
- Changing the theme in the dashboard while Chrono is open must repaint Chrono without a restart. Covered in Task 2 (event emitted) and Task 4 (Chrono listens).
- On macOS, closing one of the two windows while the other is open must keep Kairo in the Dock. Test in Task 2.
- A second launch with `--dashboard` while Kairo runs in the tray must open the dashboard, not the palette. Test in Task 1.

---

### Task 1: Launch arguments and the tray

**Files:**
- Modify: `src-tauri/src/startup.rs` (arguments, launch decision, tests)
- Modify: `src-tauri/src/lib.rs` (single-instance handler, first-launch handling, `setup_tray`, remove `NudgeToggle`/`AutostartToggle`)
- Modify: `src-tauri/src/commands.rs` (`set_nudges_enabled`, `set_autostart` stop touching tray items)

**Interfaces:**
- Produces: `pub const DASHBOARD_ARG: &str = "--dashboard";`
- Produces: `pub enum Launch { Palette, Chrono, Dashboard, Quiet }` (derive `Debug, PartialEq, Clone, Copy`) and `pub fn launch_for(args: &[String]) -> Launch`, replacing `opens_palette`/`opens_palette_for`/`launched_for_chrono`. Priority: `--palette` → Palette; `--dashboard` → Dashboard; `--chrono` → Chrono; `--minimized` → Quiet; otherwise Palette.
- Produces: `pub fn tray_click_opens_dashboard(button: MouseButton, state: MouseButtonState) -> bool` in `lib.rs` (true for Left + Up).
- Consumes: `show_dashboard(app, None)` from Task 2. Until Task 2 lands, call `show_main` in its place and switch in Task 2.

- [ ] **Step 1: Write the failing tests** in `startup.rs`, replacing `opening_kairo_opens_the_palette_and_chrono_opens_chrono`:

```rust
#[test]
fn each_launch_opens_what_it_names() {
    let run = |args: &[&str]| launch_for(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>());
    assert_eq!(run(&["kairo"]), Launch::Palette);
    assert_eq!(run(&["kairo", PALETTE_ARG]), Launch::Palette);
    assert_eq!(run(&["kairo", CHRONO_ARG]), Launch::Chrono);
    assert_eq!(run(&["kairo", DASHBOARD_ARG]), Launch::Dashboard);
    assert_eq!(run(&["kairo", MINIMIZED_ARG]), Launch::Quiet);
}
```

and in `lib.rs` a `#[cfg(test)] mod tests` with:

```rust
#[test]
fn a_left_click_on_the_tray_opens_the_dashboard() {
    assert!(tray_click_opens_dashboard(MouseButton::Left, MouseButtonState::Up));
    assert!(!tray_click_opens_dashboard(MouseButton::Left, MouseButtonState::Down));
    assert!(!tray_click_opens_dashboard(MouseButton::Right, MouseButtonState::Up));
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cd src-tauri && cargo test each_launch_opens a_left_click`
Expected: compile errors (`launch_for`, `Launch`, `DASHBOARD_ARG`, `tray_click_opens_dashboard` not found).

- [ ] **Step 3: Implement** `launch_for` and `tray_click_opens_dashboard`. Route both the single-instance handler and the first launch through `launch_for`: Palette → `palette::toggle` (keeping the `--palette` `shortcut_may_open` check for the second-launch case), Chrono → `show_main`, Dashboard → the dashboard, Quiet → nothing. In `setup_tray`, build the five-item menu from Global Constraints with ids `dashboard`, `open`, `palette`, `pause`, `quit`. Handle `TrayIconEvent::Click` and `TrayIconEvent::DoubleClick` through `tray_click_opens_dashboard`. Delete the focus, nudges, autostart and update menu code, the `NudgeToggle`/`AutostartToggle` structs, their `app.manage` calls and their uses in `commands.rs`. `PaletteItem` stays, since the shortcut shows beside "Command palette".

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test`
Expected: all pass (92 before this task, now 93).

- [ ] **Step 5: Commit** — `git commit -am "Trim the tray menu and route launches through one decision"`

---

### Task 2: The dashboard window in Rust

**Files:**
- Modify: `src-tauri/tauri.conf.json` (second window config, per Global Constraints, `"url": "dashboard.html"`)
- Modify: `src-tauri/capabilities/default.json` (`"windows"` gains `"dashboard"`; update its description)
- Modify: `src-tauri/src/lib.rs` (generic window opening, `show_dashboard`, Dock handling, `AppState.dashboard_tab`)
- Modify: `src-tauri/src/commands.rs` (new commands; `set_appearance` emits)
- Test: `src-tauri/src/lib.rs` tests module

**Interfaces:**
- Produces: `pub(crate) fn show_window(app: &AppHandle, label: &str)`, the body of today's `show_main`, made generic. It keeps one in-flight open per label: replace `OPENING: AtomicBool` with a `Mutex<HashSet<String>>` guard via `fn begin_opening(label: &str) -> bool` / `fn end_opening(label: &str)`.
- Produces: `show_main(app)` = `show_window(app, "main")`; `pub(crate) fn show_dashboard(app: &AppHandle, tab: Option<&str>)`, which stores `tab` in `AppState.dashboard_tab: Mutex<Option<String>>`, shows the window, then emits `"kairo:dashboard"` to `"dashboard"`.
- Produces: `fn keep_in_dock(open: &[&str]) -> bool`, true when `"main"` or `"dashboard"` is among the open labels. Used on `WindowEvent::Destroyed` of either window with the labels still open (minus the one being destroyed) to call `in_dock(app, keep_in_dock(..))`.
- Produces commands (registered in `generate_handler!`):
  - `open_dashboard(app, tab: Option<String>)`
  - `take_dashboard_tab(state) -> CmdResult<Option<String>>`
  - `open_chrono(app, view: Option<String>)`: `open_main_at(app, &view, None)` when a view is given, else `show_main`
  - `open_palette(app)`: `palette::toggle`
- Changes: `set_appearance` additionally does `app.emit("kairo:appearance", &mode)`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn only_one_window_of_a_kind_opens_at_a_time() {
    assert!(begin_opening("dashboard"));
    assert!(!begin_opening("dashboard"));
    assert!(begin_opening("main"));
    end_opening("dashboard");
    end_opening("main");
    assert!(begin_opening("dashboard"));
    end_opening("dashboard");
}

#[test]
fn kairo_stays_in_the_dock_while_either_window_is_open() {
    assert!(keep_in_dock(&["dashboard"]));
    assert!(keep_in_dock(&["main", "palette"]));
    assert!(!keep_in_dock(&["palette", "blocked"]));
    assert!(!keep_in_dock(&[]));
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cd src-tauri && cargo test only_one_window kairo_stays_in_the_dock`
Expected: compile errors (functions not found).

- [ ] **Step 3: Implement** the Interfaces above. Make `open_main` generic as `open_window(app, label)`, keeping its appearance step. Point Task 1's dashboard routes at `show_dashboard(app, None)`.

- [ ] **Step 4: Run the tests and build**

Run: `cd src-tauri && cargo test && cargo clippy --all-targets`
Expected: all tests pass; no new clippy warnings (the existing `startup.rs` "items after a test module" warning may remain).

- [ ] **Step 5: Commit** — `git commit -am "Add the dashboard window and the commands that open each window"`

---

### Task 3: The palette knows the dashboard

**Files:**
- Modify: `src-tauri/src/palette/search.rs` (new action and items, tests)
- Modify: `src-tauri/src/palette/mod.rs` (`perform` handles it)
- Modify: `src-tauri/src/palette/native.rs` only if a `match` on `Action` there needs the new arm (the compiler says so)

**Interfaces:**
- Consumes: `crate::show_dashboard(app, tab)` from Task 2.
- Produces: `Action::Dashboard(Option<&'static str>)`: `None` opens Home, `Some("settings")` opens Settings.

- [ ] **Step 1: Write the failing test** in `search.rs` tests:

```rust
#[test]
fn the_dashboard_and_both_settings_are_a_few_letters_away() {
    let c = ctx();
    assert_eq!(search(&c, "dashboard")[0].action, Action::Dashboard(None));
    let settings = search(&c, "settings");
    assert!(settings.iter().any(|i| i.title == "Kairo settings" && i.action == Action::Dashboard(Some("settings"))));
    assert!(settings.iter().any(|i| i.title == "Chrono settings" && i.action == Action::Go("settings", None)));
    assert_eq!(search(&c, "theme")[0].action, Action::Dashboard(Some("settings")));
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cd src-tauri && cargo test the_dashboard_and_both_settings`
Expected: compile error (`Action::Dashboard` not found).

- [ ] **Step 3: Implement.** Add items:
  - "Dashboard", keywords `"dashboard home apps kairo"`, glyph `Glyph::Day`
  - "Kairo settings", keywords `"settings theme appearance dark light palette shortcut startup sign in login updates backup restore combine computers"`, glyph `Glyph::Settings`

  Rename the `PAGES` "Settings" entry's title to "Chrono settings", keeping keywords `"settings preferences options"`. In `perform`, `Action::Dashboard(tab) => { crate::show_dashboard(app, *tab); Ok(()) }`; like `Go`, the caller takes the palette out of the way first.

- [ ] **Step 4: Run the tests**

Run: `cd src-tauri && cargo test`
Expected: all pass, including the existing palette tests (fix any expectation that named the old "Settings" title).

- [ ] **Step 5: Commit** — `git commit -am "Open the dashboard and Kairo's settings from the palette"`

---

### Task 4: Shared frontend modules

**Files:**
- Create: `src/ui.ts`
- Create: `src/kairo-settings.ts`
- Modify: `src/main.ts` (import from both; delete what moved)
- Modify: `src/api.ts` (bindings for Task 2's commands)

**Interfaces:**
- `src/ui.ts` exports: `$`, `h`, `s`, `fmtDur`, `fmtClock`, `esc`, `pct`, `IS_MAC`, `IS_LINUX`, `appearance` (via `getAppearance(): Appearance`), `applyTheme(mode: Appearance): void`, and `followTheme(): void`. `followTheme` loads `api.appearance()`, applies it, and listens for `"kairo:appearance"` to re-apply. Moved verbatim from `main.ts` lines 17–88, apart from the new `followTheme`.
- `src/kairo-settings.ts` exports `wireKairoSettings(): void` (attaches every handler once) and `loadKairoSettings(): Promise<void>` (fills the controls). It owns the DOM ids now in Chrono's Settings for these sections: `#appearance`, `#appearance-system`, `#palette-keys`, `#palette-record`, `#palette-note`, `#palette-suggestions`, `#palette-over-fullscreen`, `#autostart-enabled`, `#autostart-title`, `#autostart-label`, `#autostart-note`, `#export-time`, `#backup-save`, `#backup-restore`, `#data-status`, `#sync-where`, `#sync-computers`, `#sync-choose`, `#sync-stop`, `#sync-status`, `#update-check`, `#update-status`, `#settings-version`. Moved code: `keyName`, `showShortcut`, `shortcutKey`, the recorder, the palette checkboxes, appearance select, autostart, `dataAction` and its buttons, `ago`, `renderSync`, the sync buttons and update check. `renderComputerPick` and `#shown-computer` stay in `main.ts`, which calls `api.sync()` for them.
- `src/api.ts` gains: `openDashboard(tab?: "settings")`, `takeDashboardTab(): Promise<string | null>`, `openChrono(view?: string)`, `openPalette()`.

- [ ] **Step 1: Move the code** as above; `main.ts` calls `followTheme()` in place of `api.appearance().then(applyTheme, …)`.
- [ ] **Step 2: Verify**

Run: `npx tsc --noEmit -p . && npm run build`
Expected: no errors. `grep -n "function showShortcut\|function applyTheme\|function renderSync" src/main.ts` prints nothing.

- [ ] **Step 3: Commit** — `git commit -am "Move Kairo-wide settings code into modules both pages can use"`

---

### Task 5: The dashboard page

**Files:**
- Create: `dashboard.html` (same head as `palette.html`, including the saved-theme script; loads `/src/dashboard.ts`)
- Create: `src/dashboard.ts`
- Modify: `vite.config.ts` (`input` gains `dashboard: "dashboard.html"`; update its comment)
- Modify: `src/styles.css` (dashboard layout, tiles)
- Modify: `index.html` (the Kairo-wide sections' markup moves from here into `dashboard.html`)

**Interfaces:**
- Consumes: Task 4's modules and API bindings; `api.day()`, `api.status()`, `api.paletteSettings()`.
- Produces in `dashboard.ts`: `interface KairoApp { id: string; name: string; what: string; line: () => Promise<string>; open: () => void; settings?: { covers: string; open: () => void } }` and `const APPS: KairoApp[]` holding Chrono (`open: () => api.openChrono()`, settings `open: () => api.openChrono("settings")`) and Command palette (`open: () => api.openPalette()`, no settings). Both Home tiles and the App settings rows are drawn from `APPS`.
- Tabs: two buttons `[data-tab="home"]` and `[data-tab="settings"]` in the `.range` style. `showTab(tab: "home" | "settings")`. On load, and on each `"kairo:dashboard"` event, `takeDashboardTab()` picks the tab (null keeps the current one; first load defaults to Home).

- [ ] **Step 1: Build the page** per the spec's Home and Settings sections and Global Constraints. Tile lines refresh every 30 s while the window is open. Visual language matches Chrono: `page-head`, `block`, `eyebrow`, `link-btn`, `btn`.
- [ ] **Step 2: Verify**

Run: `npx tsc --noEmit -p . && npm run build`
Expected: no errors; `dist/dashboard.html` exists.

- [ ] **Step 3: Screenshot** with the stand-in backend (a copy of the page with a fake `__TAURI_INTERNALS__`, served by `npx vite --port 5199`, shot with headless Edge using a separate profile). Check Home and Settings in dark and light at 860 × 620 and at 560 px wide. Expected: tiles in a grid; sections in the Global Constraints order; nothing clipped at 560 px. Delete the preview copy afterwards.
- [ ] **Step 4: Commit** — `git commit -am "Add the dashboard page: apps and Kairo's settings"`

---

### Task 6: Chrono's settings, trimmed; docs

**Files:**
- Modify: `index.html` (Chrono's Settings: keep only its sections; add the first line and link)
- Modify: `src/main.ts` (settings load no longer fills moved controls; link calls `api.openDashboard("settings")`)
- Modify: `README.md` (tray, dashboard, where settings live)
- Modify: `docs/device-checklist.md` (tray steps: menu items and click behaviour per OS)

- [ ] **Step 1: Trim and link** per Global Constraints. Run `grep -n "palette-keys\|autostart-enabled\|sync-choose\|update-check\|id=\"appearance\"" index.html src/main.ts`; expected: no matches (those ids now live only in `dashboard.html` and `src/kairo-settings.ts`).
- [ ] **Step 2: Verify**

Run: `npx tsc --noEmit -p . && npm run build && cd src-tauri && cargo test`
Expected: all pass.

- [ ] **Step 3: Screenshot** Chrono's Settings with the stand-in backend: the first line and link at the top, then Goals; no Appearance, palette, startup, data, combining or updates sections. Run a stand-in "open Chrono on settings" check: with `take_navigation` returning `{view: "settings"}`, the page opens on Settings.
- [ ] **Step 4: Commit and push; open a PR** to `main` that lists what to try on a real machine (tray clicks on Windows and macOS, the menu on Linux).

```bash
git commit -am "Keep only Chrono's own settings in Chrono, and document the dashboard"
git push -u origin dashboard
```
