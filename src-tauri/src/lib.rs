mod analysis;
mod browser;
mod classify;
mod collector;
mod commands;
mod db;
mod demo;
mod focus;
mod goals;
mod nudge;
mod platform;
mod privacy;
mod recap;
mod startup;
mod updater;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent, Wry};

pub struct AppState {
    /// Connection used by UI commands; the collector thread has its own.
    pub db: Mutex<rusqlite::Connection>,
    pub paused: Arc<AtomicBool>,
    pub window_cache: nudge::SharedCache,
    pub privacy_rules: privacy::SharedRules,
    pub browser_tab: browser::SharedTab,
    /// False when another program holds the extension's port.
    pub browser_listening: bool,
}

/// The tray's checkboxes, kept so the in-app toggles can update them.
pub struct NudgeToggle(pub CheckMenuItem<Wry>);
pub struct AutostartToggle(pub CheckMenuItem<Wry>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be first: a second launch (e.g. from Start while Kairo sits in the tray) just
        // focuses the running copy instead of starting another collector.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![startup::MINIMIZED_ARG]),
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Size, position and maximized state come back as they were; visibility stays ours,
        // since Kairo starts hidden in the tray at sign-in.
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(tauri_plugin_window_state::StateFlags::all() - tauri_plugin_window_state::StateFlags::VISIBLE)
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
            let paused = Arc::new(AtomicBool::new(false));
            let window_cache = nudge::SharedCache::default();
            let privacy_rules = privacy::SharedRules::new(privacy::load(&conn)?.into());
            let browser_tab = browser::SharedTab::default();
            let browser_listening = browser::spawn(browser_tab.clone());
            collector::spawn(db_path.clone(), paused.clone(), privacy_rules.clone(), browser_tab.clone());
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
            });
            setup_tray(app, paused, nudges_on)?;
            updater::spawn(app.handle().clone());
            // The window starts hidden; show it unless Windows launched us at sign-in.
            if !startup::launched_at_login() {
                show_main(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps Kairo tracking in the tray; "Quit" in the tray menu exits.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
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
            commands::stop_focus,
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
            commands::get_version,
            commands::check_for_updates,
            commands::seed_demo_data,
            commands::clear_demo_data,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kairo");
}

fn setup_tray(app: &tauri::App, paused: Arc<AtomicBool>, nudges_on: bool) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Kairo", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause tracking", true, None::<&str>)?;
    let nudges = CheckMenuItem::with_id(app, "nudges", "Focus nudges", true, nudges_on, None::<&str>)?;
    let autostart_on = startup::is_enabled(app.handle());
    let autostart =
        CheckMenuItem::with_id(app, "autostart", "Start with Windows", startup::AVAILABLE, autostart_on, None::<&str>)?;
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
                let result = match id.trim_start_matches("focus:") {
                    "stop" => focus::stop(&conn, now),
                    minutes => minutes.parse().map_or(Ok(()), |m| focus::start(&conn, m, now).map(|_| ())),
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

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
