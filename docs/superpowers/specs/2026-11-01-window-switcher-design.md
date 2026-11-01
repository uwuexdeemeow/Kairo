# Switching windows from the command palette

## Goal

The palette finds open windows by title or app and brings one to the front, or closes it, on
Windows, macOS and Linux (X11, GNOME and KDE on Wayland), so the palette is the one place to go
for apps, files, settings and the windows already open.

## What the user sees

- Typing matches open windows by title and by app name. Matches show in a **Windows** section:
  the row's title is the window's title (`budget.xlsx — Excel`), its second line is the app's
  name, and its icon is the app's icon.
- `windows` alone lists every open window, most recently used first, at most 8 rows.
- **Enter** brings the window to the front, restoring it when minimized. **Ctrl+Enter**
  (⌘+Enter on a Mac) asks it to close, as its close button would, so the app can still ask
  about unsaved work; the footer hint says "Close window". The palette closes after either.
- Listed: windows a person would switch to, which have a title and are shown or minimized. Not
  listed: Kairo's own windows (the palette, Chrono, the dashboard, the blocked window), hidden
  and tool windows, and those with an empty title.
- Ranking: a window scores by the palette's `fuzzy` match on its title, or on its app's name
  less 20, whichever is higher. So "budget" puts `budget.xlsx — Excel` first, while "excel"
  puts the Excel app first and its windows just under. Window rows never outrank a focus
  command, a calculator answer or a day.
- Nothing typed: windows don't show among the suggestions.

## When the list is taken

Once each time the palette opens, a snapshot of the open windows, in the system's front-to-back
order, is taken and kept with the palette's context; typing searches it. A window that closed
since then: Enter and Ctrl+Enter do nothing, and the palette closes as after any action.

## Per system

- **Windows:** `EnumWindows` over top-level windows that are visible or minimized, not cloaked
  (`DWMWA_CLOAKED`), not tool windows (`WS_EX_TOOLWINDOW`), not owned, with a title, and not
  Kairo's own process. Front-to-back order is `EnumWindows`'s own z-order. Activating restores
  a minimized window and brings it forward the way Windows allows an app that was just used
  (the palette had the keyboard); closing posts `WM_CLOSE`.
- **macOS:** `CGWindowListCopyWindowInfo` (on-screen and minimized, layer 0, other apps), with
  titles through the Screen Recording access Kairo already asks for. Raising or closing one
  window uses the Accessibility API (`AXRaise` and the window's close button) through the
  access Firefox and Opera site tracking already ask for. Without that access, Enter activates
  the window's app instead, and no Ctrl+Enter is offered.
- **Linux, X11:** the window manager's `_NET_CLIENT_LIST_STACKING`, titles from `_NET_WM_NAME`,
  apps from `_NET_WM_PID`; activating sends `_NET_ACTIVE_WINDOW`, closing `_NET_CLOSE_WINDOW`
  (both as client messages to the root window).
- **GNOME on Wayland:** Kairo's GNOME Shell extension (`org.gnome.Shell.Extensions.Kairo`) gains
  `Windows()` (the list as JSON: id, title, pid, newest first), `Activate(id)` and `Close(id)`.
  The new extension takes effect at the next login, as when it was installed; until then the
  extension doesn't answer `Windows()` and no Windows section shows.
- **KDE Plasma on Wayland:** a one-off KWin script, loaded the way `minimize.js` is, sends the
  list to Kairo's own D-Bus interface (`io.github.uwuexdeemeow.Kairo`, new method `windows`);
  `activate.js` and `close.js` act on one window by its id. The list arrives a moment after the
  palette opens, and searches use it from then on.
- **Other Wayland desktops:** no list, as with tracking there; no Windows section.

## How it's built

- **`platform` (all systems)**: `pub struct OpenWindow { id: String, title: String, pid: u32,
  path: Option<String> }`, `pub fn open_windows() -> Vec<OpenWindow>` (front to back),
  `pub fn activate_window(id: &str) -> bool` and `pub fn close_window(id: &str) -> bool`, each
  system's way as above; `pub fn can_close_windows() -> bool` (false on a Mac without
  Accessibility). Ids are text so every system fits: a window handle, a CG window number, an X11
  id, or the shell's id.
- **Filtering** that doesn't need the system (empty titles, Kairo's own process, order) is a
  pure function `platform::switchable(windows, own_pid)`, tested on every system.
- **Palette** (`search.rs`, `mod.rs`): `Context.windows: Vec<WindowEntry>` (id, title, app
  name, icon path) filled from `open_windows()` when the palette opens; new actions
  `FocusWindow(id)` and `CloseWindow(id)`; rows as above. `mod.rs` runs them through the
  platform functions.
- **GNOME extension** (`gnome-extension/modern` and `legacy`) and **KWin scripts**
  (`kwin-script/windows.js`, `activate.js`, `close.js`).

## Testing

- `search.rs`: "budget" puts the window first; "excel" puts the app first and its window next;
  `windows` lists all, capped at 8, in snapshot order; Enter is `FocusWindow`, Ctrl+Enter
  `CloseWindow` with "Close window"; no `CloseWindow` when closing isn't possible; windows never
  beat a focus command or a calculator answer; none among the suggestions.
- `platform::switchable`: drops empty titles and Kairo's own windows, keeps order.
- Each system's parsing that can be tested without the system: the GNOME and KDE JSON, the X11
  property decoding.
- `docs/device-checklist.md`: a "Palette: windows" section for Windows, a Mac (with and without
  Accessibility), GNOME and KDE. Linux can't be checked on a real machine yet, so its parts
  ship checked only by CI's build and unit tests.

## Out of scope

Keyboard-only cycling like Alt+Tab, window previews, moving windows between screens or virtual
desktops, and windows of other users.
