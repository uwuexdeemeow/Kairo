# Files and Settings in the Command Palette Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The palette finds files through each system's own index, finds every Kairo and Chrono setting, jumps to any setting, and flips on/off settings in place, on Windows, macOS and Linux.

**Architecture:** Shared Rust in `src-tauri/src/palette/` decides everything: a settings list (`settings.rs`) searched with the other rows, and a file search (`files.rs`) whose per-system code only asks the index for paths. File rows come from a background worker thread and reach a palette after its other rows: the macOS/Linux page gets a `palette:files` event, Windows' native palette a window message. Chrono and the dashboard learn to scroll to, focus and highlight a control named by the palette.

**Tech Stack:** Rust (Tauri 2, `windows` 0.62, rusqlite), TypeScript (Vite, Vitest + jsdom), `mdfind`, `localsearch`/`tracker3`, `plocate`/`locate`, Windows Search via `search-ms:`.

**Spec:** `docs/superpowers/specs/2026-10-29-palette-files-settings-design.md`

## Global Constraints

- Every change ships for Windows, macOS and Linux; Windows-only and macOS-only code must compile in CI on its own system.
- Files: names only, under the home folder only; never saved, tracked, stored in the database or synced.
- File search starts at **2** typed characters, waits **120 ms** of quiet, gives up after **1.5 s**, shows at most **5** rows under the heading **Files**, after every other section.
- Ranking: name starts with the query > a word in the name starts with it > the name contains it; ties to the most recently modified; one row per path.
- Left out: any path with a component starting with `.`, `AppData` (Windows), `~/Library` (macOS), `node_modules`, `.git`, `target`, `__pycache__`.
- Second line of a file row: its folder from `~`, e.g. `~/Documents/Taxes`.
- Enter = the row's main thing; Ctrl+Enter (⌘+Enter on a Mac) = where it lives. Footer labels: `Show in folder` (files), `Open in settings` (settings).
- Switch rows show `On` / `Off` on the right; Enter flips them; the palette stays open.
- Switches: Focus nudges, Evening summary (keeps its time), Focus blocking, Block launchers too, Palette suggestions, Open over full screen, Search files, Start at login.
- Only jumped to, never flipped: strict blocking, demo data.
- Export, Back up, Restore a backup, Check for updates keep running on Enter; Ctrl/⌘+Enter jumps to them.
- New Kairo setting **Search files**, under Command palette, on by default.
- Linux with no indexer: one row, `File search needs localsearch or plocate`, once per palette opening; choosing it does nothing.
- Error copy: `Couldn't open it: it was moved or deleted.`
- Jumping highlights the control's section for about **1.5 s**.
- Kairo emits `kairo:settings` after a palette flip; open Chrono and dashboard windows reload their controls.
- Release: **1.20.0**.
- Code reads like the surrounding code: `///` doc comments in full sentences, the same density; user-facing words as in the spec.

## Review Focus

- A file row arrives while the user's Enter is already on its way: Enter must run the row they saw, never a file that slid in at that index (Task 7, Task 8 tests).
- A query with characters the index treats specially (`"`, `*`, `%`, `&`, `#`, spaces, non-ASCII like `résumé`): no crash, no injected query syntax, and the file is still found where the index can (Tasks 2, 5, 6 tests).
- Flipping Focus blocking or Block launchers while strict mode is on: refused with the existing strict-mode message on the row's second line, state unchanged (Task 3 test).
- Start at login flipped in a development build (`startup::AVAILABLE == false`): row stays as it was and says why (Task 3 test).
- A settings page changed so a listed control id no longer exists: a test fails before release (Task 3 test).

## File Structure

- Create `src-tauri/src/palette/files.rs`: query → ranked file `Item`s; the worker thread; `print_search` for `kairo --search-files`.
- Create `src-tauri/src/palette/files/{windows,macos,linux}.rs`: each `pub fn ask(query: &str, home: &Path, deadline: Instant) -> Result<Vec<PathBuf>, Unavailable>`.
- Create `src-tauri/src/palette/settings.rs`: the settings list, its search, reading and flipping switches.
- Modify `src-tauri/src/palette/search.rs`: new actions, `Item.second`, `Icon::File`, `Context.switches`, `Context.search_files`; Kairo-settings rows move to `settings.rs`.
- Modify `src-tauri/src/palette/mod.rs`: `perform` for the new actions; `Session` keeps file rows; `ItemView` gains `second` and the file icon; the Search files setting.
- Modify `src-tauri/src/palette/native.rs`, `web.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/startup.rs`.
- Modify `src/palette.ts`, `src/api.ts`, `src/main.ts`, `src/dashboard.ts`, `src/kairo-settings.ts`, `dashboard.html`, `src/styles.css`.
- Create `src/palette.test.ts`, `src/jump.ts` (shared scroll/focus/highlight), `src/jump.test.ts`.
- Create `.github/workflows/windows.yml`.
- Modify `docs/device-checklist.md`, version files.

---

### Task 1: File search core and `kairo --search-files`

**Files:**
- Create: `src-tauri/src/palette/files.rs`
- Modify: `src-tauri/src/palette/search.rs` (Action, Item, Icon), `src-tauri/src/palette/mod.rs` (`pub mod files;`, `ItemView`, `perform` arms), `src-tauri/src/startup.rs`, `src-tauri/src/lib.rs` (`run`)

**Interfaces:**
- Produces, in `search.rs`:
  - `Action::OpenFile(String)`, `Action::RevealFile(String)`, `Action::Nothing`; `Action::opens_window()` true for `RevealFile`.
  - `Item.second: Option<(Action, &'static str)>`, the Ctrl/⌘+Enter action and its footer label; `Item::new` sets `None`.
  - `Icon::File(String)` (a path).
- Produces, in `files.rs`:
  - `pub const MIN_CHARS: usize = 2; pub const MAX_FILES: usize = 5; pub const QUIET_MS: u64 = 120; pub const LIMIT_MS: u64 = 1500;`
  - `pub enum Unavailable { NoIndexer, Failed(String) }`
  - `pub type Backend = fn(&str, &Path, Instant) -> Result<Vec<PathBuf>, Unavailable>;`
  - `pub fn home() -> Option<PathBuf>`
  - `pub fn kept(path: &Path, home: &Path) -> bool`
  - `pub fn rows(query: &str, home: &Path, found: Vec<PathBuf>) -> Vec<Item>` (filter, dedupe, rank, cap, build rows: section `"Files"`, `Icon::File`, `Action::OpenFile`, `second: Some((Action::RevealFile(..), "Show in folder"))`, `sub` the folder from `~`, `at` the matched characters)
  - `pub fn no_indexer_row() -> Item` (title `File search needs localsearch or plocate`, section `"Files"`, `Action::Nothing`)
  - `pub fn ask_system(query: &str, home: &Path, deadline: Instant) -> Result<Vec<PathBuf>, Unavailable>` dispatching to `files::{windows,macos,linux}::ask`; until Tasks 2, 5 and 6 each returns `Err(Unavailable::NoIndexer)`.
  - `pub fn print_search(query: &str)`: prints each row as `path\tsub`, or `no indexer` / `failed: <why>`.
- Produces, in `mod.rs`: `IconView::File { path: String, kind: &'static str }` (`kind`: `"folder"`, `"image"`, `"document"` or `"other"`, from the extension); `ItemView.second: Option<&'static str>` (the label). `perform`: `OpenFile` opens through `crate::platform::open_path`, else `Err("Couldn't open it: it was moved or deleted.")`; `RevealFile` shows it in its folder (Windows `explorer /select,<path>`; macOS `open -R <path>`; Linux the `org.freedesktop.FileManager1.ShowItems` D-Bus call through `gdbus`, else `xdg-open` on its folder); `Nothing` returns `Ok`.
- Produces, in `startup.rs`: `pub const SEARCH_FILES_ARG: &str = "--search-files";` `pub fn search_files_query(args: &[String]) -> Option<String>` (the words after the flag, joined by spaces).

- [ ] **Step 1: Write the failing tests** in `files.rs`'s `mod tests`, with a home of `/home/ana` (build paths with `Path::join` so they hold on Windows):

```rust
#[test] fn hidden_and_cluttered_paths_are_left_out() {
    let home = Path::new("/home/ana");
    assert!(kept(&home.join("Documents/budget.xlsx"), home));
    for p in [".config/budget", "code/app/node_modules/budget.js", "code/.git/budget", "code/app/target/budget",
              "code/__pycache__/budget.pyc", "AppData/Local/budget.txt", "Library/Caches/budget", "Documents/.budget.swp"] {
        assert!(!kept(&home.join(p), home), "{p}");
    }
    assert!(!kept(Path::new("/etc/budget"), home));
}
#[test] fn rows_rank_start_then_word_then_inside_and_cap_at_five() { /* names "budget.xlsx", "old budget.txt", "mybudget.md" → that order; 7 matches → 5 rows */ }
#[test] fn ties_go_to_the_newest() { /* two "budget" files, write the older first, set mtime with std::fs::File::set_modified on a tempdir under std::env::temp_dir() */ }
#[test] fn one_row_per_path() { /* same path twice → one row */ }
#[test] fn a_row_opens_the_file_and_shows_its_folder_from_home() {
    // title "budget.xlsx", sub "~/Documents/Taxes" (on Windows "~\\Documents\\Taxes"), section "Files",
    // action OpenFile(path), second (RevealFile(path), "Show in folder"), at == [0,1,2,3,4,5] for "budget"
}
#[test] fn odd_characters_in_the_query_are_only_matched() { /* query "\"*%& résumé" doesn't panic; "résumé" matches "Résumé 2026.pdf" */ }
```

and in `startup.rs`'s tests: `search_files_query(&["kairo".into(), "--search-files".into(), "tax".into(), "2026".into()]) == Some("tax 2026".into())`, and `None` without the flag.

- [ ] **Step 2: Run them to see them fail**

Run: `cd src-tauri && cargo test palette::files startup::`
Expected: FAIL to compile (`kept`, `rows`, `search_files_query` not found).

- [ ] **Step 3: Implement** the interfaces above. Matching is case-insensitive on the file name (`file_name()`), lowercased with `to_lowercase`; ranks 0/1/2 for starts/word-starts/contains, a word starting after any of ` _-.()[]`; then `modified` newest first (read with `std::fs::metadata`, missing → oldest). In `lib.rs` `run()`, first thing: `if let Some(q) = startup::search_files_query(&std::env::args().collect::<Vec<_>>()) { palette::files::print_search(&q); return; }`. On Windows `print_search` first calls `AttachConsole(ATTACH_PARENT_PROCESS)` (`windows-sys`, feature `Win32_System_Console`) so a release build prints to the terminal it was run from.

- [ ] **Step 4: Run the tests to see them pass**

Run: `cd src-tauri && cargo test`
Expected: PASS, all tests.

- [ ] **Step 5: Commit** `git commit -m "Rank file search results, and add kairo --search-files"`

---

### Task 2: Windows: the `search-ms:` spike and Windows CI

**Files:**
- Create: `src-tauri/src/palette/files/windows.rs`, `.github/workflows/windows.yml`
- Modify: `src-tauri/Cargo.toml` (only if a feature is missing), `src-tauri/src/palette/files.rs` (dispatch)

**Interfaces:**
- Consumes: `Unavailable`, `Backend` (Task 1).
- Produces: `pub fn ask(query: &str, home: &Path, deadline: Instant) -> Result<Vec<PathBuf>, Unavailable>`; `fn search_url(query: &str, home: &Path) -> String`.

- [ ] **Step 1: Write the failing test** (`#[cfg(windows)]`, in `windows.rs`):

```rust
#[test] fn the_query_and_folder_are_escaped() {
    assert_eq!(search_url("tax & \"2026\"", Path::new(r"C:\Users\Ana")),
        "search-ms:query=tax%20%26%20%222026%22&crumb=location:C%3A%5CUsers%5CAna");
}
```

- [ ] **Step 2: Add `.github/workflows/windows.yml`**: on push to the same paths as `linux.yml`, `runs-on: windows-latest`, checkout, setup-node 20 with npm cache, rust stable, rust-cache on `src-tauri`, `npm ci`, `cargo test` in `src-tauri`, `npm test`. No bundle step: it proves Windows-only code compiles and its tests pass.

- [ ] **Step 3: Implement `ask`**: on the calling thread `CoInitializeEx(None, COINIT_MULTITHREADED)`; `SHCreateItemFromParsingName(search_url(..))` as `IShellItem`; `BindToHandler(None, &BHID_EnumItems)` as `IEnumShellItems`; `Next` one at a time, each `GetDisplayName(SIGDN_FILESYSPATH)`, freeing with `CoTaskMemFree`; stop at 50 paths or once `Instant::now() >= deadline`. A failing COM call is `Unavailable::Failed(e.to_string())`. Percent-encode everything but ASCII letters and digits in both the query and the folder.

- [ ] **Step 4: Push the branch and check CI**

Run: `git push -u origin palette-files-settings && gh run watch $(gh run list --workflow windows.yml --limit 1 --json databaseId -q '.[0].databaseId') --exit-status`
Expected: the Windows workflow passes.

- [ ] **Step 5: The spike gate: ask the user** to run, on their Windows PC, a build of this branch (the Windows workflow's run, or `npm run tauri build -- --debug` there), as `kairo.exe --search-files <a word in a file name under Documents>`. Pass: their file is listed within about a second. If it isn't (nothing found, or too slow), stop and replace `ask` with Windows Search's own query interface: `ISearchManager` → `ISearchCatalogManager("SystemIndex")` → `ISearchQueryHelper::GenerateSQLFromUserQuery`, run through OLE DB (`Search.CollatorDSO`), selecting `System.ItemPathDisplay` scoped to `file:<home>`, with the same test and the same gate.

- [ ] **Step 6: Commit** `git commit -m "Find files through Windows Search, and test on Windows in CI"`

---

### Task 3: The settings list, its search, and switches

**Files:**
- Create: `src-tauri/src/palette/settings.rs`
- Modify: `src-tauri/src/palette/search.rs` (Context, actions list), `src-tauri/src/palette/mod.rs` (`context`, `perform`, Search files setting), `src-tauri/src/commands.rs` (palette settings commands), `dashboard.html`, `src/kairo-settings.ts`, `src/api.ts`

**Interfaces:**
- Produces, in `settings.rs`:
  - `#[derive(Clone, Copy, Debug, PartialEq)] pub enum Switch { Nudges, Evening, Blocking, Launchers, Suggestions, OverFullscreen, SearchFiles, Autostart }`
  - `pub enum Page { Chrono, Kairo }`
  - `pub struct Entry { pub title: &'static str, pub words: &'static str, pub page: Page, pub control: &'static str, pub switch: Option<Switch>, pub runs: Option<Action> }` (`control` is the element id without `#`, and is also the entry's id in `Action::Setting`; `runs` is the Enter action for Export, Back up, Restore, Check for updates and "Kairo settings")
  - `pub fn entries() -> Vec<Entry>`, covering every spec entry. Controls: Chrono `goal-productive`, `goal-distracting`, `goal-week-productive`, `goal-week-distracting`, `retention`, `delete-range`, `project-name`, `calendar-urls`, `ext-open`, `nudges-enabled`, `evening-enabled`, `block-enabled`, `block-launchers`, `block-strict`, `break-every`, `demo-load`; Kairo `appearance`, `palette-record`, `palette-suggestions`, `palette-over-fullscreen`, `palette-search-files`, `autostart-enabled`, `export-time`, `backup-save`, `backup-restore`, `sync-choose`, `update-check`.
  - `pub fn search(query: &str, switches: &[(Switch, bool)]) -> Vec<Item>`: section `"Settings"`, sub `"Chrono settings"` / `"Kairo settings"`, glyph `Glyph::Settings`; for a switch, `detail` `On`/`Off` and action `Action::Toggle(switch)`; otherwise `runs` or `Action::Setting(control)`; `second: Some((Action::Setting(control), "Open in settings"))` on every row.
  - `pub fn read(app: &AppHandle, conn: &Connection) -> Vec<(Switch, bool)>`
  - `pub fn flip(app: &AppHandle, conn: &Connection, switch: Switch) -> Result<bool, String>`: the new state, through the same module functions the commands use (`nudge::set_enabled`, `evening::set` with the saved minute, `blocking::set_setting` behind the `blocking::strict_now` refusal with the commands' exact message, `palette::set_suggestions`, `palette::set_over_fullscreen`, `palette::set_search_files`, `startup::set_enabled`); then `app.emit("kairo:settings", ())`.
- Produces, in `search.rs`: `Action::Toggle(Switch)`, `Action::Setting(&'static str)` (`opens_window` true for `Setting`), `Action::keeps_palette_open()` true only for `Toggle`; `Context.switches: Vec<(Switch, bool)>`, `Context.search_files: bool`. `search()` appends `settings::search(query, &ctx.switches)` with each item's score lowered by 8 as the actions are; the "Kairo settings", Export, Back up, Restore and Check for updates rows leave the `actions` array.
- Produces, in `mod.rs`: `pub fn search_files(conn) -> Result<bool>` (key `palette_search_files`, on unless `"0"`), `pub fn set_search_files(conn, on) -> Result<()>`; `context` takes `app: &AppHandle` too and fills `switches` and `search_files`. `perform`: `Toggle` → `settings::flip` (result ignored there; callers re-query), `Setting(control)` → the jump of Task 4 (until then `crate::open_main_at(app, "settings", None)` or `crate::show_dashboard(app, Some("settings"))` by page).
- Produces, commands: `PaletteSettings.search_files`; `#[tauri::command] set_palette_search_files(state, on: bool)`; TS `api.setPaletteSearchFiles(on)`, `paletteSettings()` gains `searchFiles: boolean`. In `dashboard.html`, under Command palette, `<input type="checkbox" id="palette-search-files" />` labelled `Search files`, with the note `Names of files in your home folder, from the system's index. They're only shown, never saved.` `kairo-settings.ts` wires it like `#palette-suggestions` and listens for `kairo:settings` to run `loadKairoSettings()`.

- [ ] **Step 1: Write the failing tests** in `settings.rs`:

```rust
#[test] fn every_entry_is_found_by_its_title() { for e in entries() { assert!(search(e.title, &[]).iter().any(|i| i.title == e.title), "{}", e.title) } }
#[test] fn words_find_settings_their_titles_dont_name() {
    assert_eq!(search("recap", &[])[0].title, "Evening summary");
    assert_eq!(search("dark mode", &[])[0].title, "Theme");
    assert_eq!(search("login", &[])[0].title, "Start at login");
}
#[test] fn switches_show_their_state_and_flip_on_enter() {
    let row = &search("nudges", &[(Switch::Nudges, true)])[0];
    assert_eq!(row.detail.as_deref(), Some("On"));
    assert_eq!(row.action, Action::Toggle(Switch::Nudges));
    assert_eq!(row.second, Some((Action::Setting("nudges-enabled"), "Open in settings")));
}
#[test] fn strict_blocking_and_demo_data_only_jump() { /* actions Setting("block-strict") and Setting("demo-load") */ }
#[test] fn actions_keep_running_and_gain_a_jump() { /* "back up": action Action::Backup, second Setting("backup-save") */ }
#[test] fn every_control_is_on_its_page() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let chrono = std::fs::read_to_string(root.join("index.html")).unwrap();
    let kairo = std::fs::read_to_string(root.join("dashboard.html")).unwrap();
    for e in entries() {
        let page = match e.page { Page::Chrono => &chrono, Page::Kairo => &kairo };
        assert!(page.contains(&format!("id=\"{}\"", e.control)), "{} is missing #{}", e.title, e.control);
    }
}
```

Plus, against an in-memory database (`db::open_in_memory()` or however `mod.rs`'s tests make one):
- `flipping_blocking_in_strict_mode_is_refused`: start a strict session and expect `Err` with the strict message, state unchanged. This needs an `AppHandle`, so split `flip` into `flip_stored(conn, switch) -> Result<bool, String>` for every switch but `Autostart` (tested) and a thin `flip` that adds autostart and the emit.
- `autostart_in_a_development_build_says_why`, by testing the message `startup::set_enabled` returns when `!startup::AVAILABLE`.

Update `search.rs`'s `the_dashboard_and_both_settings_are_a_few_letters_away` to the folded rows.

- [ ] **Step 2: Run them to see them fail**

Run: `cd src-tauri && cargo test palette::`
Expected: FAIL to compile (`settings` module missing).

- [ ] **Step 3: Implement** the interfaces above, and the HTML/TS for the Search files switch.

- [ ] **Step 4: Run all tests**

Run: `cd src-tauri && cargo test && cd .. && npm test && npm run build`
Expected: PASS; build succeeds.

- [ ] **Step 5: Commit** `git commit -m "Find every setting from the palette, and flip switches there"`

---

### Task 4: Jumping to a setting in Chrono and the dashboard

**Files:**
- Create: `src/jump.ts`, `src/jump.test.ts`
- Modify: `src-tauri/src/lib.rs` (`Navigation`, `open_main_at`), `src-tauri/src/palette/mod.rs` (`perform` for `Setting`), `src/api.ts` (`takeNavigation` type), `src/main.ts` (`followNavigation`, `kairo:settings`), `src/dashboard.ts` (`followTab`), `src/styles.css`

**Interfaces:**
- Consumes: `settings::entries()`, `Page` (Task 3); `shownControl` (`src/dropdown.ts`).
- Produces: `Navigation.target: Option<String>`; `pub(crate) fn open_main_to(app: &AppHandle, view: &str, date: Option<NaiveDate>, target: Option<&str>)`, with `open_main_at` calling it with `None`; the dashboard tab request may be `settings#<id>`. In TS: `export function jumpTo(id: string): void` in `src/jump.ts`: finds `#id`; scrolls `shownControl(el)` to the center; focuses it; adds class `jumped` to its closest `.block` and removes it after `JUMP_MS = 1500`.

- [ ] **Step 1: Write the failing tests** in `src/jump.test.ts` (fake timers): `jumpTo("evening-enabled")` focuses the checkbox, and its section has `jumped` until 1500 ms pass; a `<select>` given a themed dropdown gets its button focused; an unknown id does nothing. In `src/dashboard.test.ts`: `open("settings#palette-suggestions")` lands on Settings with `#palette-suggestions` focused. In `src/kairo-settings.test.ts`: a `kairo:settings` event reloads the controls (`api.paletteSettings` called again).

- [ ] **Step 2: Run them to see them fail**

Run: `npx vitest run src/jump.test.ts src/dashboard.test.ts src/kairo-settings.test.ts`
Expected: FAIL (`./jump` not found; tab not parsed).

- [ ] **Step 3: Implement.** `perform(Setting(control))` looks the entry up: Chrono → `open_main_to(app, "settings", None, Some(control))`; Kairo → `show_dashboard(app, Some(&format!("settings#{control}")))`. `followNavigation` calls `jumpTo(target)` after the view's `load` has finished. `followTab` splits on `#`, shows the tab, waits for `loadKairoSettings()`, then calls `jumpTo`. Chrono listens for `kairo:settings` and reloads its settings view when it's the one showing. `.jumped` in `styles.css`: the accent ring the focused controls use, fading out.

- [ ] **Step 4: Run all tests**

Run: `npm test && npm run build && cd src-tauri && cargo test`
Expected: PASS.

- [ ] **Step 5: Commit** `git commit -m "Jump straight to a setting from the palette"`

---

### Task 5: macOS: Spotlight

**Files:**
- Create: `src-tauri/src/palette/files/macos.rs`

**Interfaces:**
- Consumes: `Unavailable` (Task 1).
- Produces: `pub fn ask(..)` as in Task 2; `fn read_paths(out: impl BufRead, deadline: Instant, max: usize) -> Vec<PathBuf>`.

- [ ] **Step 1: Write the failing tests** (`#[cfg(target_os = "macos")]`): `read_paths` over a saved sample of three `mdfind` lines gives three paths, skips an empty line, and stops at `max`. A live test, `#[ignore]` since it needs Spotlight: it writes `~/Documents/kairo-spotlight-<random>.txt`, polls `ask` for up to 10 s, expects to find it, and deletes the file.

- [ ] **Step 2: Run them to see them fail**

Run: `cd src-tauri && cargo test palette::files::macos`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**: spawn `mdfind -onlyin <home> -name <query>` (the query as one argument, never through a shell), read stdout with `read_paths` on the worker thread, kill the child at the deadline or after 50 lines. A missing `mdfind` is `NoIndexer`; a non-zero exit is `Failed`.

- [ ] **Step 4: Run tests, then the live one, then the flag**

Run: `cd src-tauri && cargo test && cargo test palette::files::macos -- --ignored && cargo run -- --search-files Kairo`
Expected: PASS; the flag lists files with "Kairo" in their names.

- [ ] **Step 5: Commit** `git commit -m "Find files through Spotlight on a Mac"`

---

### Task 6: Linux: localsearch/tracker, then plocate/locate

**Files:**
- Create: `src-tauri/src/palette/files/linux.rs`

**Interfaces:**
- Consumes: `Unavailable` (Task 1).
- Produces: `pub fn ask(..)` as in Task 2; `fn from_tracker(out: &str) -> Vec<PathBuf>`; `fn from_locate(out: &str, home: &Path) -> Vec<PathBuf>`; `enum Tool { Localsearch, Tracker3, Plocate, Locate }`; `fn tool() -> Option<Tool>` (the first found on `PATH`, looked up once and kept).

- [ ] **Step 1: Write the failing tests** (`#[cfg(not(any(windows, target_os = "macos")))]`) on saved outputs: `from_tracker` turns `file:///home/ana/Documents/Tax%202026.pdf` lines (as `localsearch search --files` and `tracker3 search --files` print them, among their headers) into `/home/ana/Documents/Tax 2026.pdf`; `from_locate` keeps only paths under `/home/ana`.

- [ ] **Step 2: Run them to see them fail**

Run: `cd src-tauri && cargo test palette::files::linux` (on Linux, or via the Linux CI workflow)
Expected: FAIL to compile.

- [ ] **Step 3: Implement**: run the tool with the query as its own argument (`localsearch search --files --limit 50 <q>`, `tracker3 search --files --limit 50 <q>`, `plocate -i -l 200 -- <q>`, `locate -i -l 200 -- <q>`), killed at the deadline. No tool → `NoIndexer`.

- [ ] **Step 4: Push and check the Linux workflow**

Run: `git push && gh run watch $(gh run list --workflow linux.yml --limit 1 --json databaseId -q '.[0].databaseId') --exit-status`
Expected: PASS.

- [ ] **Step 5: Commit** `git commit -m "Find files through localsearch or plocate on Linux"`

---

### Task 7: The file worker, the session, and the macOS/Linux plumbing

**Files:**
- Modify: `src-tauri/src/palette/files.rs` (worker), `src-tauri/src/palette/mod.rs` (`Session`), `src-tauri/src/commands.rs` (`palette_query`, `palette_run`, `palette_icons`), `src-tauri/src/lib.rs` (manage the worker), `src/api.ts`

**Interfaces:**
- Consumes: `ask_system`, `rows`, `no_indexer_row`, constants (Task 1); `Action::keeps_palette_open`, `settings::flip` (Task 3).
- Produces, in `files.rs`: `pub struct Worker`; `impl Worker { pub fn new(backend: Backend, on_done: impl Fn(u64, Vec<Item>) + Send + 'static) -> Self; pub fn ask(&self, seq: u64, query: String) }`. One thread; a newer `ask` replaces a pending one; it waits `QUIET_MS` after the newest `ask`, then runs the backend with a deadline of `LIMIT_MS`; results past the deadline are dropped; `NoIndexer` yields `vec![no_indexer_row()]` the first time per `Worker::opened()` (`pub fn opened(&self)`, called when the palette opens) and nothing after; `Failed` yields nothing.
- Produces, in `mod.rs`: `Session::add_files(&self, seq: u64, items: Vec<Item>) -> Option<Vec<ItemView>>` appends to `items` and returns their views only when `seq` is current; `Session::wants_files(&self, query: &str) -> bool` (`ctx.search_files` and `query.trim().chars().count() >= MIN_CHARS`); `Session::set_switch(&self, switch, on)`; `shows_icon` also accepts paths of the file rows shown.
- Produces, commands: `palette_query` calls `worker.ask(results.seq, query)` when `session.wants_files(&query)`; the managed worker's `on_done` emits `palette:files` to `palette::LABEL` with `{ seq, rows }` when `add_files` returns `Some`. `palette_run` returns `Option<palette::Results>`: for a `Toggle`, it flips, `session.set_switch`, re-runs the last query, and returns the new results, keeping the palette open; otherwise `None` as now. New command `palette_run_second(seq, at)` runs `item.second`'s action (or nothing). TS: `paletteRun` returns `PaletteResults | null`; `paletteRunSecond(seq, at)`; `PaletteItem.second: string | null`; icon kind `"file"` with `path` and `kind`.

- [ ] **Step 1: Write the failing tests** in `files.rs` with a fake `Backend` that records the queries it saw and sleeps as told:

```rust
#[test] fn only_the_newest_query_is_asked() { /* ask(1,"bu"), ask(2,"bud"), ask(3,"budget") within 50 ms → backend saw only "budget"; on_done got seq 3 */ }
#[test] fn a_search_past_the_limit_shows_nothing() { /* backend sleeps 1.6 s → on_done not called within 2.5 s */ }
#[test] fn no_indexer_is_said_once_per_opening() { /* two asks → one call with the no-indexer row, then none; opened() → said again */ }
```

and in `mod.rs`: `files_for_an_old_query_are_dropped` (`query("bu")`, `query("bud")`, `add_files(first_seq, ..)` is `None` and `item(second_seq, n)` doesn't return a file), `files_join_the_end_of_the_rows` (after `add_files(current, ..)`, `item(seq, rows_len)` is the first file).

- [ ] **Step 2: Run them to see them fail**

Run: `cd src-tauri && cargo test palette::`
Expected: FAIL to compile.

- [ ] **Step 3: Implement** the interfaces above; `palette_open` calls `worker.opened()`.

- [ ] **Step 4: Run all tests**

Run: `cd src-tauri && cargo test && cd .. && npm run build`
Expected: PASS.

- [ ] **Step 5: Commit** `git commit -m "Search files in the background and add them to the palette's results"`

---

### Task 8: The macOS/Linux palette page

**Files:**
- Modify: `src/palette.ts`, `src/styles.css`
- Create: `src/palette.test.ts`

**Interfaces:**
- Consumes: `paletteRun` → `PaletteResults | null`, `paletteRunSecond`, the `palette:files` event `{ seq, rows }`, `PaletteItem.second`, the `"file"` icon (Task 7).
- Produces: nothing new for later tasks.

- [ ] **Step 1: Write the failing tests** in `src/palette.test.ts` (mocks as in `dashboard.test.ts`; markup from `palette.html`'s body; `@tauri-apps/api/window` and `dpi` mocked):
  - file rows from `palette:files` for the current `seq` appear under a `Files` label after the other rows; for an older `seq` they're ignored;
  - Enter on row 0 calls `paletteRun(seq, 0)` even when files arrived after the selection was made, and the selection stays on the same item;
  - Ctrl+Enter (and Meta+Enter) calls `paletteRunSecond(seq, selected)`; the footer shows `Show in folder` on a file row and `Open in settings` on a setting row, and nothing on a row without `second`;
  - Enter on a switch row renders the returned results with that row now `Off`, selection kept, the palette not hidden;
  - a `"file"` icon on Linux (`navigator.userAgent` with `Linux`) draws a glyph by `kind`; elsewhere it asks `paletteIcons` for the path.

- [ ] **Step 2: Run them to see them fail**

Run: `npx vitest run src/palette.test.ts`
Expected: FAIL.

- [ ] **Step 3: Implement**: `listen("palette:files", …)` appends to `results.rows` and `shown`, re-renders keeping `selected`'s item; the keydown handler sends Ctrl/Meta+Enter to `runSecond`; the footer gains a `kbd` hint from `shown[selected].second` (`⌘ Enter` on a Mac, `Ctrl Enter` elsewhere); `run` renders returned results instead of waiting to be hidden.

- [ ] **Step 4: Run all tests**

Run: `npm test && npm run build`
Expected: PASS.

- [ ] **Step 5: Commit** `git commit -m "Show files, switches and second actions in the macOS and Linux palette"`

---

### Task 9: The Windows palette

**Files:**
- Modify: `src-tauri/src/palette/native.rs`

**Interfaces:**
- Consumes: `Worker` (Task 7), `Item.second`, `Icon::File`, `Action::keeps_palette_open`, `settings::flip` (Task 3), `context(app, ..)` (Task 3).
- Produces: nothing new.

- [ ] **Step 1: Write the failing tests** in `native.rs`'s tests, for the pure parts pulled out of `Palette`: `fn merge_files(items: &mut Vec<Item>, rows: &mut usize, seq: u64, current: u64, files: Vec<Item>) -> bool` (appends to the end of the rows only when `seq == current`; returns whether it changed anything) and `fn footer_hint(item: Option<&Item>) -> Option<&'static str>`.

- [ ] **Step 2: Run them to see them fail** in the Windows workflow (push).

Expected: FAIL to compile.

- [ ] **Step 3: Implement**:
  - `const WM_FILES: u32 = WM_APP + 2;`. The palette owns a `Worker`, created in `create`, whose `on_done` stores `(seq, items)` in a `static FILES: Mutex<Option<(u64, Vec<Item>)>>` and posts `WM_FILES`.
  - `Palette` gains `seq: u64`. `refresh_results` bumps it and asks the worker when `ctx.search_files` and the query has `MIN_CHARS`. `window_proc` handles `WM_FILES` with `merge_files`, then `layout` and `resize`.
  - `edit_proc`: Enter with Ctrl runs `item.second`.
  - `run`: `keeps_palette_open` → `settings::flip`, update `ctx.switches`, `refresh_results` keeping `selected`, show any error on the row.
  - `draw_row`: draws `detail` (On/Off) as today. `icon()` draws `Icon::File` through `bitmap(path)`, like `Icon::App`. The footer draws the `footer_hint` label with `kbd("Ctrl Enter", ..)`.

- [ ] **Step 4: Push and check the Windows workflow**

Run: `git push && gh run watch $(gh run list --workflow windows.yml --limit 1 --json databaseId -q '.[0].databaseId') --exit-status`
Expected: PASS.

- [ ] **Step 5: Commit** `git commit -m "Show files, switches and second actions in the Windows palette"`

---

### Task 10: Device checklist and release 1.20.0

**Files:**
- Modify: `docs/device-checklist.md`, `package.json`, `package-lock.json` (the two `"version"` fields only), `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` (the `kairo` package only), `src-tauri/tauri.conf.json`

- [ ] **Step 1: Add a "Palette: files and settings" section** to `docs/device-checklist.md` for Windows, Mac and Linux, in its style (numbered steps with *Expect:*):
  1. Type part of a file's name in Documents. *Expect:* it shows under Files within a second.
  2. Ctrl/⌘+Enter on it. *Expect:* its folder opens with it selected.
  3. Type "login" and press Enter. *Expect:* Start at login flips, and the dashboard's switch matches.
  4. Type "evening" and press Ctrl/⌘+Enter. *Expect:* Chrono opens on Evening summary, highlighted.
  5. Linux only: without localsearch or plocate. *Expect:* the one "needs" row.

- [ ] **Step 2: Run everything**

Run: `npm test && npm run build && cd src-tauri && cargo test`
Expected: PASS.

- [ ] **Step 3: Merge and release**: open a PR with `gh pr create` and merge it once the Linux, macOS and Windows workflows pass. Bump `1.19.3` → `1.20.0` in the version files, commit `Release 1.20.0`, tag `v1.20.0`, push `main` and the tag, and watch the Release workflow to success.
