# Checking Kairo on real machines

Kairo's tests and CI builds prove the code compiles and its logic holds on Windows, macOS and
Linux, but how each system grants permissions, delivers keys and moves files only shows on a real
machine. This is what to try on each, with what should happen. Windows was gone through on
27 Oct 2026 (1.9.0): sites from Brave's address bar, the full-screen shortcut block and its
setting, the tray's Command palette, CSV export, backup and restore, and combining two copies
through a folder all worked, after three fixes (borderless maximized full-screen windows, folder
switching, sub-second CSV rows).

If something goes wrong, run Kairo from a terminal so its messages show, and copy what it prints:

- **Mac:** `/Applications/Kairo.app/Contents/MacOS/Kairo`
- **Linux:** `kairo` (from the `.deb`) or the `.AppImage` file itself
- **Windows:** messages aren't shown in release builds; describe what you saw instead.

Kairo goes idle after 2 minutes without input (30 in games and video), so keep using the machine
while checking tracking.

## Tray and dashboard

On Windows and a Mac:

1. **Click.** Left-click the tray icon (menu bar icon on a Mac), then try a double-click.
   *Expect:* the dashboard opens either way, once, never two windows. Right-click (or, on a Mac,
   any other click) shows the menu.
2. **Menu.** *Expect:* Open dashboard, Open Chrono, Command palette (with its shortcut),
   Pause tracking and Quit Kairo, in that order. Open dashboard opens the dashboard, and if it's
   already open, brings it forward.
3. **Settings.** On the dashboard's Home, press Chrono's Settings while Chrono is closed.
   *Expect:* Chrono opens on its Settings page, which starts with a line pointing to Kairo's
   settings; its link opens the dashboard's Settings tab. Change the theme there with Chrono open.
   *Expect:* Chrono repaints without a restart.

On Linux the tray doesn't report clicks, so open the menu instead: *expect* the same five items,
and Open dashboard opens the dashboard.

## Mac

1. **Install.** Open the `.dmg`, copy Kairo to Applications, run `xattr -cr /Applications/Kairo.app`
   once, open it. *Expect:* a Screen Recording request (for window titles). Allow it, then quit and
   reopen Kairo when macOS asks.
2. **Tracking.** Use a couple of apps for a few minutes. *Expect:* they show on Today with their
   names and icons, and window titles in Recent sessions.
3. **Sites in Safari or Chrome** (no extension). Visit a site for a minute. *Expect:* macOS asks
   once whether Kairo may control the browser; after allowing, the site shows under Sites.
4. **Sites in Firefox or Opera** (no extension). Visit a site for a minute. *Expect:* one
   Accessibility request (System Settings → Privacy & Security → Accessibility); after allowing,
   the site shows under Sites. If it never does, run Kairo from the terminal and note what it says.
5. **Palette.** Press Option+Space. *Expect:* the palette opens; Esc closes it.
6. **Full screen.** Put an app in full screen (the green button) and press Option+Space.
   *Expect:* nothing. Turn on Kairo's settings → Command palette → "Open it over games and full-screen
   apps too" and press it again. *Expect:* it opens. The menu bar's Kairo menu → Command palette
   opens it either way.
7. **Controller.** Connect a gamepad, open a non-game app, and for 3 minutes touch only the
   controller (move a stick fully, press buttons). *Expect:* the sidebar never says Idle, and macOS
   doesn't ask for Input Monitoring. A controller left alone should still let Kairo go idle.

## Linux, GNOME on Wayland (Ubuntu's default)

1. **Install** the `.deb` (or run the AppImage). *Expect:* a notification asking to log out and
   back in. Do so.
2. **Extension on.** `gnome-extensions list --enabled | grep kairo` *Expect:*
   `kairo@uwuexdeemeow.github.io`.
3. **Tracking.** Use Firefox, Files and the Terminal (native Wayland apps) for a few minutes.
   *Expect:* each shows on Today with its icon.
4. **Idle.** Leave the machine alone for 3 minutes. *Expect:* the sidebar says Idle.
5. **Palette.** Press Ctrl+Space. *Expect:* the palette opens. Settings → Keyboard → View and
   Customise Shortcuts → Custom Shortcuts lists "Kairo command palette". Change the shortcut in Kairo's settings; *expect* the
   GNOME shortcut to follow.
6. **Full screen.** Press F11 in Firefox, then Ctrl+Space. *Expect:* nothing, unless the
   full-screen setting is on.
7. **Focus blocking.** Mark an app distracting in Categories, start a 25-minute focus session and
   switch to it. *Expect:* it minimizes and Kairo's blocked window shows.
8. **Game (Steam, Proton or native).** Play for a minute. *Expect:* it shows under the game's
   name, filed as distracting, as on Windows.
9. **Controller.** As on the Mac, step 7.
10. **Update.** When a newer version is out, use Check for updates in Kairo's settings. *Expect:* the
    `.deb` asks for your password, then Kairo restarts on the new version.

## Linux, KDE Plasma on Wayland

1. **Install** and open Kairo; no logout needed.
2. **Tracking.** Use Dolphin, Konsole and a browser. *Expect:* each shows on Today.
3. **Idle.** Leave it alone 3 minutes. *Expect:* Idle. (If Kairo never goes idle on Plasma, say
   so: its idle time comes from Plasma's screen locker, whose units weren't certain.)
4. **Palette.** Press Ctrl+Space. *Expect:* it opens; System Settings → Shortcuts lists Kairo's
   "Open the command palette".
5. **Focus blocking**, as on GNOME, step 7.

## Two computers combined

Use two machines (any systems) with the same cloud drive.

1. On each, Settings → Combine computers → Choose a folder, and pick the **same** synced folder.
2. Wait 5–10 minutes for the drive to sync. *Expect:* each lists both computers, and Today on
   each adds up both.
3. **Showing switch.** In the sidebar, pick "This one". *Expect:* only that computer's time.
4. **Shared choices.** Change a site's category on one. *Expect:* within about 10 minutes, the
   other shows the same category.
5. **Use another folder** on one computer. *Expect:* its folder leaves the old location, and the
   other computer drops its time on its next sync.
6. **Stop combining** on one. *Expect:* each shows only its own time again.

## Any system: backup and restore

1. Settings → Your data → Back up everything; save the file.
2. Restore it on another computer (even another system). *Expect:* a confirmation, a restart, and
   the history, categories, limits and goals of the backup, with this computer's palette shortcut
   kept.
