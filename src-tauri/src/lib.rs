mod analysis;
mod appearance;
mod blocking;
mod browser;
mod classify;
mod collector;
mod commands;
mod db;
mod demo;
mod focus;
mod goals;
mod nudge;
mod palette;
mod platform;
mod privacy;
mod recap;
mod startup;
mod updater;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use chrono::NaiveDate;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, RunEvent, Wry};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

pub struct AppState {
    /// Connection used by UI commands; the collector thread has its own.
    pub db: Mutex<rusqlite::Connection>,
    pub paused: Arc<AtomicBool>,
    pub window_cache: nudge::SharedCache,
    pub privacy_rules: privacy::SharedRules,
    pub browser_tab: browser::SharedTab,
    /// "Allow 5 minutes" exceptions during focus sessions, shared with the extension's listener.
    pub allowances: blocking::Allowances,
    /// Strict mode's pending "end early" and "allow" requests, shared with the extension's listener.
    pub strict_waits: blocking::Waits,
    /// App icons as data URLs, by executable; None for apps with no icon to show.
    pub icons: Mutex<HashMap<String, Option<String>>>,
    /// False when another program holds the extension's port.
    pub browser_listening: bool,
    /// Where the palette last asked Kairo's window to go, until the page takes it.
    pub navigation: Mutex<Option<Navigation>>,
}

/// A view, and for Today or Week a local `YYYY-MM-DD` date.
#[derive(Serialize, Clone)]
pub struct Navigation {
    view: String,
    date: Option<String>,
}

/// The tray's checkboxes, kept so the in-app toggles can update them.
pub struct NudgeToggle(pub CheckMenuItem<Wry>);
pub struct AutostartToggle(pub CheckMenuItem<Wry>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be first: a second launch (e.g. from Start while Kairo sits in the tray) just
        // focuses the running copy instead of starting another collector, or opens its palette.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == startup::PALETTE_ARG) {
                // On Wayland the desktop's own shortcut runs this, so it stays out of games too.
                if !platform::fullscreen_in_front() {
                    palette::toggle(app);
                }
            } else {
                show_main(app);
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![startup::MINIMIZED_ARG]),
        ))
        .plugin(tauri_plugin_notification::init())
        // The command palette copies its answers.
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Size, position and maximized state come back as they were; visibility stays ours,
        // since Kairo starts hidden in the tray at sign-in.
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(tauri_plugin_window_state::StateFlags::all() - tauri_plugin_window_state::StateFlags::VISIBLE)
                // These place themselves each time they open.
                .with_denylist(&[palette::LABEL, blocking::WINDOW_LABEL])
                .build(),
        )
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    // A game or full-screen app in front keeps its keys to itself.
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed
                        && !platform::fullscreen_in_front()
                    {
                        palette::toggle(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db_path = dir.join("kairo.db");

            let conn = db::open(&db_path)?;
            if let Err(e) = startup::apply_default(app.handle(), &conn) {
                eprintln!("startup: cannot enable start at login: {e}");
            }
            let nudges_on = nudge::enabled(&conn)?;
            let palette_shortcut = palette::shortcut(&conn)?;
            let paused = Arc::new(AtomicBool::new(false));
            let window_cache = nudge::SharedCache::default();
            let privacy_rules = privacy::SharedRules::new(privacy::load(&conn)?.into());
            let browser_tab = browser::SharedTab::default();
            let allowances = blocking::Allowances::default();
            let strict_waits = blocking::Waits::default();
            let browser_listening = browser::spawn(browser_tab.clone(), db_path.clone(), allowances.clone(), strict_waits.clone());
            blocking::spawn(app.handle().clone(), db_path.clone(), paused.clone(), allowances.clone());
            app.manage(palette::Index::default());
            app.manage(palette::Session::default());
            #[cfg(windows)]
            palette::native::spawn(app.handle().clone(), db_path.clone(), startup::launched_for_palette());
            let scripted_tab = browser::SharedScripted::default();
            #[cfg(target_os = "macos")]
            {
                platform::request_window_titles();
                browser::spawn_scripted(scripted_tab.clone(), paused.clone());
            }
            #[cfg(windows)]
            browser::spawn_scripted(scripted_tab.clone(), paused.clone(), browser_tab.clone());
            collector::spawn(db_path.clone(), paused.clone(), privacy_rules.clone(), browser_tab.clone(), scripted_tab);
            goals::spawn(app.handle().clone(), db_path.clone(), paused.clone());
            focus::spawn(app.handle().clone(), db_path.clone(), paused.clone());
            nudge::spawn(app.handle().clone(), db_path, paused.clone(), window_cache.clone());
            app.manage(AppState {
                db: Mutex::new(conn),
                paused: paused.clone(),
                window_cache,
                privacy_rules,
                browser_tab,
                browser_listening,
                allowances,
                strict_waits,
                icons: Mutex::default(),
                navigation: Mutex::default(),
            });
            setup_tray(app, paused, nudges_on)?;
            // Wayland shows apps no other windows; on GNOME, Kairo's extension does, once GNOME
            // has loaded it at a login, and on KDE Plasma its KWin script does right away.
            #[cfg(not(any(windows, target_os = "macos")))]
            {
                if platform::watch_wayland() {
                    use tauri_plugin_notification::NotificationExt;
                    let _ = app
                        .notification()
                        .builder()
                        .title("Log out and back in to finish setting up Kairo")
                        .body("GNOME turns on the extension that lets Kairo see your apps at your next login.")
                        .show();
                }
                let handle = app.handle().clone();
                platform::on_palette_shortcut(Box::new(move || {
                    if !platform::fullscreen_in_front() {
                        palette::toggle(&handle);
                    }
                }));
            }
            // Wayland keeps keys from apps, so there the desktop's own shortcuts open the palette.
            // Another app holding the keys mustn't stop Kairo starting; Settings shows the clash.
            if !platform::desktop_palette_shortcut(&palette_shortcut) {
                if let Err(e) = app.global_shortcut().register(palette_shortcut.as_str()) {
                    eprintln!("palette: cannot use {palette_shortcut}: {e}");
                }
            }
            updater::spawn(app.handle().clone());
            // Kairo's window is made when it's wanted; at sign-in Kairo starts in the tray.
            if startup::launched_for_palette() {
                // Windows' palette opens itself once its window exists.
                #[cfg(not(windows))]
                palette::toggle(app.handle());
            } else if !startup::launched_at_login() {
                show_main(app.handle());
            }
            Ok(())
        })
        // On macOS and Linux the palette is a web page, which goes away when something else
        // takes the keyboard; Windows' own palette watches for that itself.
        .on_window_event(|window, event| {
            #[cfg(not(windows))]
            palette::web::on_window_event(window, event);
            #[cfg(windows)]
            let _ = (window, event);
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_day,
            commands::get_first_day,
            commands::get_day_totals,
            commands::get_patterns,
            commands::get_categories,
            commands::set_category,
            commands::get_status,
            commands::get_heads_up,
            commands::dismiss_heads_up,
            commands::get_nudges_enabled,
            commands::set_nudges_enabled,
            commands::send_test_nudge,
            commands::get_autostart,
            commands::set_autostart,
            commands::get_week,
            commands::get_focus,
            commands::start_focus,
            commands::control_focus,
            commands::get_focus_overview,
            commands::get_browsers,
            commands::allow_blocked,
            commands::close_blocked,
            commands::get_focus_blocking,
            commands::set_focus_blocking,
            commands::set_focus_rule,
            commands::open_extensions_page,
            commands::get_focus_blocks,
            commands::get_break_minutes,
            commands::set_break_minutes,
            commands::get_goals,
            commands::get_goal_settings,
            commands::set_goal,
            commands::set_limit,
            commands::set_privacy_rule,
            commands::apply_privacy_to_past,
            commands::get_retention,
            commands::set_retention,
            commands::delete_history,
            commands::get_extension_status,
            commands::open_extension_folder,
            commands::take_navigation,
            commands::palette_open,
            commands::palette_query,
            commands::palette_icons,
            commands::palette_card,
            commands::palette_run,
            commands::palette_hide,
            commands::get_palette_settings,
            commands::set_palette_shortcut,
            commands::set_palette_suggestions,
            commands::get_appearance,
            commands::set_appearance,
            commands::get_app_icons,
            commands::get_version,
            commands::check_for_updates,
            commands::seed_demo_data,
            commands::clear_demo_data,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Kairo")
        .run(|_app, event| {
            // Closing Kairo's window lets it go, web view and all, and Kairo keeps tracking in the
            // tray; only "Quit" (which gives an exit code) ends it.
            if let RunEvent::ExitRequested { code: None, api, .. } = event {
                api.prevent_exit();
            }
        });
}

fn setup_tray(app: &tauri::App, paused: Arc<AtomicBool>, nudges_on: bool) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Kairo", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause tracking", true, None::<&str>)?;
    let nudges = CheckMenuItem::with_id(app, "nudges", "Focus nudges", true, nudges_on, None::<&str>)?;
    let autostart_on = startup::is_enabled(app.handle());
    let autostart =
        CheckMenuItem::with_id(app, "autostart", startup::LABEL, startup::AVAILABLE, autostart_on, None::<&str>)?;
    let focus_menu = Submenu::with_items(
        app,
        "Focus session",
        true,
        &[
            &MenuItem::with_id(app, "focus:25", "25 minutes", true, None::<&str>)?,
            &MenuItem::with_id(app, "focus:50", "50 minutes", true, None::<&str>)?,
            &MenuItem::with_id(app, "focus:90", "90 minutes", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "focus:stop", "End session", true, None::<&str>)?,
        ],
    )?;
    let update = MenuItem::with_id(app, "update", "Check for updates", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Kairo", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &focus_menu, &pause, &nudges, &autostart, &update, &quit])?;
    app.manage(NudgeToggle(nudges.clone()));
    app.manage(AutostartToggle(autostart.clone()));

    let pause_item = pause.clone();
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("bundle has an icon"))
        .tooltip("Kairo")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "pause" => {
                let now_paused = !paused.fetch_xor(true, Ordering::Relaxed);
                let _ = pause_item.set_text(if now_paused { "Resume tracking" } else { "Pause tracking" });
            }
            "nudges" => {
                // The OS flips the checkmark on click; persist whatever it now shows.
                let on = nudges.is_checked().unwrap_or(true);
                if let Ok(conn) = app.state::<AppState>().db.lock() {
                    if let Err(e) = nudge::set_enabled(&conn, on) {
                        eprintln!("tray: cannot save nudge setting: {e}");
                    }
                }
            }
            "autostart" => {
                let on = autostart.is_checked().unwrap_or(false);
                if let Err(e) = startup::set_enabled(app, on) {
                    eprintln!("tray: {e}");
                    let _ = autostart.set_checked(startup::is_enabled(app));
                }
            }
            id if id.starts_with("focus:") => {
                let state = app.state::<AppState>();
                let Ok(conn) = state.db.lock() else { return };
                let now = chrono::Utc::now().timestamp_millis();
                // The tray can't show strict mode's countdown, and starting anew would end the
                // running session, so both go through Kairo's own window instead.
                if blocking::strict_now(&conn).unwrap_or(false) {
                    drop(conn);
                    blocking::notify(app, "Strict mode is on", "End the focus session from Kairo's Focus page, after a short wait.");
                    show_main(app);
                    return;
                }
                let result = match id.trim_start_matches("focus:") {
                    "stop" => focus::stop(&conn, now),
                    minutes => minutes.parse().map_or(Ok(()), |m| focus::start(&conn, m, None, now).map(|_| ())),
                };
                if let Err(e) = result {
                    eprintln!("tray: focus session: {e}");
                }
            }
            "update" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move { updater::check(&app, true).await });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Set while Kairo's window is being made, so a second click doesn't make another.
static OPENING: AtomicBool = AtomicBool::new(false);

/// Brings up Kairo's window, making it if it was closed. It isn't kept while closed: its web view
/// is most of the memory Kairo would use, and Kairo spends most of its time in the tray.
pub(crate) fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    // Making a web view from inside one of Windows' callbacks (a tray click, a menu) can hang,
    // so it's made on a thread of its own.
    if OPENING.swap(true, Ordering::AcqRel) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = open_main(&app) {
            eprintln!("cannot open Kairo's window: {e}");
        }
        OPENING.store(false, Ordering::Release);
    });
}

fn open_main(app: &AppHandle) -> tauri::Result<()> {
    if app.get_webview_window("main").is_some() {
        return Ok(());
    }
    let Some(config) = app.config().app.windows.iter().find(|w| w.label == "main") else { return Ok(()) };
    let window = tauri::WebviewWindowBuilder::from_config(app, config)?.build()?;
    let mode = app.state::<AppState>().db.lock().ok().and_then(|conn| appearance::get(&conn).ok()).unwrap_or_default();
    appearance::apply_to(&window, &mode);
    window.show()?;
    window.set_focus()
}

/// Opens Kairo on a view, and for Today or Week a date. A window made for it is still loading
/// when this runs, so the page asks where to go (`take_navigation`) once it's ready, and again
/// whenever it's told to.
pub(crate) fn open_main_at(app: &AppHandle, view: &str, date: Option<NaiveDate>) {
    let target = Navigation { view: view.to_string(), date: date.map(|d| d.format("%Y-%m-%d").to_string()) };
    *app.state::<AppState>().navigation.lock().unwrap_or_else(|e| e.into_inner()) = Some(target);
    show_main(app);
    let _ = app.emit_to("main", "kairo:navigate", ());
}
