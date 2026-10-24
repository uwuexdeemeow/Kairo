mod analysis;
mod classify;
mod collector;
mod commands;
mod db;
mod demo;
mod nudge;
mod platform;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent, Wry};

pub struct AppState {
    /// Connection used by UI commands; the collector thread has its own.
    pub db: Mutex<rusqlite::Connection>,
    pub paused: Arc<AtomicBool>,
    pub window_cache: nudge::SharedCache,
}

/// The tray's "Focus nudges" checkbox, kept so the in-app toggle can update it.
pub struct NudgeToggle(pub CheckMenuItem<Wry>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db_path = dir.join("kairo.db");

            let conn = db::open(&db_path)?;
            let nudges_on = nudge::enabled(&conn)?;
            let paused = Arc::new(AtomicBool::new(false));
            let window_cache = nudge::SharedCache::default();
            collector::spawn(db_path.clone(), paused.clone());
            nudge::spawn(app.handle().clone(), db_path, paused.clone(), window_cache.clone());
            app.manage(AppState { db: Mutex::new(conn), paused: paused.clone(), window_cache });
            setup_tray(app, paused, nudges_on)?;
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
            commands::get_today,
            commands::get_patterns,
            commands::get_categories,
            commands::set_category,
            commands::get_status,
            commands::get_heads_up,
            commands::dismiss_heads_up,
            commands::get_nudges_enabled,
            commands::set_nudges_enabled,
            commands::send_test_nudge,
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
    let quit = MenuItem::with_id(app, "quit", "Quit Kairo", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &pause, &nudges, &quit])?;
    app.manage(NudgeToggle(nudges.clone()));

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
