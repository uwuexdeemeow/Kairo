# Kairo
Kairo takes its name from kairos, the ancient Greek word for the right or opportune moment. It’s a desktop productivity hub that helps you understand and make the most of your time. It begins with a screen time tracker that quietly runs in the background, records which apps and sites you use, and turns that into clear insights about when you focus best and where your time slips away. All data stays on your device. Kairo is built to grow into a single home for everyday productivity tools, with a clipboard manager, command palette launcher and local AI file search on the roadmap.

## Screen time tracker

Kairo runs in the system tray and samples the foreground window once a second. Consecutive samples in the same window become a **session** stored in a local SQLite database (`%APPDATA%\com.kairo.app\kairo.db`). Closing the window keeps tracking, and lets the window go entirely, so Kairo idles in the tray without a web view; use the tray menu to pause or quit.

```
collector thread ──► SQLite (sessions, categories) ──► analysis ──► dashboard
 (Win32 foreground                                     (Rust)       (Vite + TS)
  window + idle time)
```

- **Collector** (`src-tauri/src/collector.rs`): foreground app, window title and idle detection. Keyboard, mouse and Xbox-compatible controller input all count as activity. Idle starts after 2 minutes without input (30 minutes on video sites, so watching isn't counted as being away). Sleep and lock screen end the current session.
- **Classification** (`src-tauri/src/classify.rs`): readable app names, sites guessed from browser titles, and default productive, neutral and distracting categories. Users can override any category in the app. Games are named after their install folder (`steamapps\common\THE FINALS` → THE FINALS).
- **Goals** (`src-tauri/src/goals.rs`): a daily productive target, a cap on distracting time and per-app or per-site limits, shown on Today with a streak. A notification goes out at 80% of a limit, when it's passed, and when a target is reached, each at most once a day.
- **Browser extension** (`extension/`, `src-tauri/src/browser.rs`): a small Manifest V3 extension for Brave, Chrome and other Chromium browsers that reports the active tab's domain to a listener on `127.0.0.1:47613`, so sites are exact rather than guessed from titles. It ships inside the installer; Settings opens its folder for **Load unpacked**. Only the hostname is sent, and only requests from an extension are accepted.
- **Focus sessions and breaks** (`src-tauri/src/focus.rs`): sessions of 5 to 240 minutes, with an optional label, from the Focus view, the sidebar or the tray. They can be paused (pauses don't count as focus), extended or ended early, and the Focus view shows today, this week and all-time totals with a history of recent sessions. A distraction that holds the screen for 30 seconds mid-session gets a nudge, and sessions show on the Today timeline and the Week view. A break reminder follows a long unbroken stretch at the screen (90 minutes by default); 5 minutes away resets it.
- **Focus blocking** (`src-tauri/src/blocking.rs`): during a running (not paused) session, a distracting app in front is minimized behind a small Kairo window offering **Back to work**, **Allow 5 minutes** or **End session**; game launchers (Steam, Epic, Riot, Battle.net, EA, Ubisoft, GOG) are closed; and the browser extension swaps distracting sites for a blocked page. What counts is the category unless Categories says **Always block** or **Never block**. Windows doesn't let Kairo minimize programs running with more rights than it has, which includes many games with anti-cheat. **Strict mode** (off by default) makes ending a session early or allowing something blocked wait 30 seconds and then ask for confirmation, everywhere those can happen, and keeps the blocking settings fixed until the session ends.
- **Command palette** (`src-tauri/src/palette/`): a global shortcut (Ctrl+Space by default, Option+Space on a Mac, changeable in Settings) opens a small window over anything. On Windows it's drawn natively with Direct2D and DirectWrite (`native.rs`) rather than a web view, so it opens instantly and costs a few megabytes; on macOS and Linux it's a small page in the system's web view (`web.rs`, `src/palette.ts`), kept for ten minutes after use. Both show what the same search finds (`search.rs`). It opens installed apps, ranked by how much you use them: on Windows everything in the Start menu's All apps list, Store apps and the games Steam, Riot and others add included (`platform::start_apps`); on a Mac, app bundles; on Linux, desktop entries. It jumps to Kairo's pages and to days written the way people say them ("yesterday", "last tue", "14 oct"), with a glance at that day; answers questions about time ("finals this week", "youtube today", "slack last month") with a card of the total, the change from the period before, a bar a day against the daily limit and what stands out, plus rows to change the limit or copy the answer (`cards.rs`); and starts focus sessions ("focus 50 essay"), showing first when the session ends, what it blocks, what happens to game launchers and whether strict mode is on. With nothing typed it suggests a focus session, limits passed today and today's apps.
- **Look**: a light and a dark palette, following Windows or chosen in Settings, with the window frame to match. Apps show their real icons, read from their program files (`platform::icon_png`); sites show a letter badge.
- **Patterns**: averages per active day, insights on when distraction peaks and when focus sessions finish, an average day by hour, a weekday-by-hour heatmap, and the top apps and sites over 7, 14 or 30 days.
- **Weekly recap** (`src-tauri/src/recap.rs`): the Week view compares Monday to Sunday with the week before, day by day, with the top apps, goal days and limits passed. A summary notification goes out on Sunday evening.
- **Privacy** (`src-tauri/src/privacy.rs`): per-app and per-site rules to record normally, hide window titles or not record at all (password managers are off by default, private browser windows never keep titles), deleting the last hour, a day or everything, and an optional history limit. Deleted rows are overwritten on disk (`secure_delete`).
- **Analysis** (`src-tauri/src/analysis.rs`): focus blocks, context switches, an hour-of-day and weekday breakdown, and insights such as *"You lose focus most after 10pm"*, *"Your sharpest hours are 9–11am"* and *"Distractions often start right after Slack"*. Each insight only appears when there's enough data behind it.

### Development

Requirements: Rust (MSVC toolchain), Visual Studio C++ build tools with the Windows SDK, and Node 18+.

```sh
npm install
npm run tauri dev      # run the app
npm run tauri build    # installer in src-tauri/target/release/bundle
cd src-tauri && cargo test
```

Run builds from PowerShell or cmd. In Git Bash, `/usr/bin/link` shadows the MSVC linker.

#### macOS (preview)

Tracking, idle time, icons, focus blocking and the command palette also work on a Mac (`platform::mac`). Mac apps go by their bundle name and share categories with their Windows twins (Google Chrome.app counts as `chrome`). Window titles need **Screen Recording** access, which Kairo asks for on first launch. Sites don't: Kairo asks Safari, Chrome, Brave, Edge, Vivaldi and Arc for the front tab's address through Apple Events once a second (`browser::spawn_scripted`), and macOS asks once per browser whether that's allowed; the browser's own answer wins over the extension's, which may be about another browser. Chromium browsers say when a window is incognito, and those sites aren't kept; Safari doesn't, so its private windows are recognised only from their titles. The extension is still what blocks sites during focus sessions. Firefox and Opera don't answer, so they keep the extension or titles. Instead of being minimized, blocked apps are hidden.

The macOS build workflow runs the tests on a Mac and keeps a `.dmg` with each run (Actions → macOS build → Artifacts); releases include one too, and Mac copies update themselves like Windows ones. It's only ad-hoc signed, without an Apple Developer ID, so after copying Kairo to Applications, run `xattr -cr /Applications/Kairo.app` once before opening it.

#### Linux (preview)

The command palette, app and site tracking, focus sessions with blocking, and goals work on Linux, with apps tracked, minimized and closed through X11 and `/proc` (`platform.rs`), and app icons come from the desktop entry that runs each program. Wayland doesn't let apps see each other's windows, so on GNOME's Wayland session Kairo installs a small GNOME Shell extension of its own (`gnome-extension/`, `platform/gnome.rs`) that tells it which window is in front and minimizes blocked ones; GNOME turns it on at the next login, and idle time comes from GNOME too. On KDE Plasma's Wayland session, Kairo loads a KWin script (`kwin-script/`, `platform/kde.rs`) that reports the window in front back to it over the session bus, and idle time comes from Plasma's screen locker. On other Wayland desktops Kairo sees only apps running through XWayland, as most games do, and never counts as idle. Game controllers keep Kairo from counting time as idle on every build. Windows games under Wine or Proton go by their `.exe`, so they share names, categories and limits with Windows. The palette's shortcut works on X11. Wayland doesn't let apps listen for keys, so there, add a shortcut in the desktop's keyboard settings that runs `kairo --palette` (which also works anywhere else, from a launcher). The Linux build workflow runs the tests on Ubuntu and keeps an AppImage and a `.deb` with each run; releases include both, and both update themselves (the `.deb` asks for your password to install).

To try the insights without waiting weeks, go to **Categories → Load demo data**. Demo rows are flagged and can be cleared on their own.

`tauri build` signs the update bundle, so a local build needs the signing key in the environment:

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw "$env:USERPROFILE\.tauri\kairo.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
```

### Releasing

Installed copies update themselves from GitHub Releases: they check a minute after launch and every 6 hours, and the tray has **Check for updates**. To ship a version, bump `version` in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json`, then push a matching tag:

```sh
git tag v0.3.0 && git push origin v0.3.0
```

The Release workflow builds, signs and publishes the Windows installer, then a universal Mac app (`.dmg`, plus the update bundle), then a Linux AppImage and `.deb`, with the `latest.json` the app reads. It signs with the `TAURI_SIGNING_PRIVATE_KEY` repo secret, a copy of `%USERPROFILE%\.tauri\kairo.key`. Keep a backup of that key: without it, installed copies can't verify new updates and have to be reinstalled by hand.
