mod analysis;
mod classify;
mod collector;
mod commands;
mod db;
mod demo;
mod platform;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};

pub struct AppState {
    /// Connection used by UI commands; the collector thread has its own.
    pub db: Mutex<rusqlite::Connection>,
    pub paused: Arc<AtomicBool>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db_path = dir.join("kairo.db");

            let conn = db::open(&db_path)?;
            let paused = Arc::new(AtomicBool::new(false));
            collector::spawn(db_path, paused.clone());
            app.manage(AppState { db: Mutex::new(conn), paused: paused.clone() });
            setup_tray(app, paused)?;
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
            commands::seed_demo_data,
            commands::clear_demo_data,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kairo");
}

fn setup_tray(app: &tauri::App, paused: Arc<AtomicBool>) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Kairo", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause tracking", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Kairo", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &pause, &quit])?;

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
