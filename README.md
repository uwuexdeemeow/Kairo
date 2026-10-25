# Kairo
Kairo takes its name from kairos, the ancient Greek word for the right or opportune moment. It’s a desktop productivity hub that helps you understand and make the most of your time. It begins with a screen time tracker that quietly runs in the background, records which apps and sites you use, and turns that into clear insights about when you focus best and where your time slips away. All data stays on your device. Kairo is built to grow into a single home for everyday productivity tools, with a clipboard manager, command palette launcher and local AI file search on the roadmap.

## Screen time tracker

Kairo runs in the system tray and samples the foreground window once a second. Consecutive samples in the same window become a **session** stored in a local SQLite database (`%APPDATA%\com.kairo.app\kairo.db`). Closing the window keeps tracking; use the tray menu to pause or quit.

```
collector thread ──► SQLite (sessions, categories) ──► analysis ──► dashboard
 (Win32 foreground                                     (Rust)       (Vite + TS)
  window + idle time)
```

- **Collector** (`src-tauri/src/collector.rs`): foreground app, window title and idle detection. Idle starts after 2 minutes without input (30 minutes on video sites, so watching isn't counted as being away). Sleep and lock screen end the current session.
- **Classification** (`src-tauri/src/classify.rs`): readable app names, sites guessed from browser titles, and default productive, neutral and distracting categories. Users can override any category in the app. Games are named after their install folder (`steamapps\common\THE FINALS` → THE FINALS).
- **Goals** (`src-tauri/src/goals.rs`): a daily productive target, a cap on distracting time and per-app or per-site limits, shown on Today with a streak. A notification goes out at 80% of a limit, when it's passed, and when a target is reached, each at most once a day.
- **Browser extension** (`extension/`, `src-tauri/src/browser.rs`): a small Manifest V3 extension for Brave, Chrome and other Chromium browsers that reports the active tab's domain to a listener on `127.0.0.1:47613`, so sites are exact rather than guessed from titles. It ships inside the installer; Settings opens its folder for **Load unpacked**. Only the hostname is sent, and only requests from an extension are accepted.
- **Focus sessions and breaks** (`src-tauri/src/focus.rs`): 25, 50 or 90-minute sessions from the sidebar or tray, with a nudge when a distraction holds the screen for 30 seconds, shown on the Today timeline and the Week view. A break reminder follows a long unbroken stretch at the screen (90 minutes by default); 5 minutes away resets it.
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

The Release workflow builds, signs and publishes the installer with the `latest.json` the app reads. It signs with the `TAURI_SIGNING_PRIVATE_KEY` repo secret, a copy of `%USERPROFILE%\.tauri\kairo.key`. Keep a backup of that key: without it, installed copies can't verify new updates and have to be reinstalled by hand.
