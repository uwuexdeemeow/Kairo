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
- **Classification** (`src-tauri/src/classify.rs`): readable app names, sites guessed from browser titles, and default productive, neutral and distracting categories. Users can override any category in the app.
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
