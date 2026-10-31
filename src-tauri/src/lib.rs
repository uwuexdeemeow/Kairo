mod analysis;
mod appearance;
mod away;
mod backup;
mod blocking;
mod calendar;
mod browser;
mod classify;
mod collector;
mod commands;
mod db;
mod demo;
mod evening;
mod focus;
mod goals;
mod lists;
mod notes;
mod nudge;
mod palette;
mod platform;
mod privacy;
mod projects;
mod recap;
mod schedule;
mod startup;
mod sync;
mod tasks;
mod updater;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use chrono::NaiveDate;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, RunEvent, Wry};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use startup::Launch;

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
    /// The dashboard tab asked for, until the page takes it.
    pub dashboard_tab: Mutex<Option<String>>,
}

/// A view, and for Today or Week a local `YYYY-MM-DD` date, and for Settings the id of the
/// control to land on.
#[derive(Serialize, Clone)]
pub struct Navigation {
    view: String,
    date: Option<String>,
    target: Option<String>,
}

/// The tray's palette item, which shows the shortcut and follows it when it changes.
pub struct PaletteItem(pub MenuItem<Wry>);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Some(query) = startup::search_files_query(&std::env::args().collect::<Vec<_>>()) {
        palette::files::print_search(&query);
        return;
    }
    tauri::Builder::default()
        // Must be first: a second launch (e.g. from Start while Kairo sits in the tray) reaches the
        // running copy instead of starting another collector. Kairo itself is the command
        // palette; Chrono's own shortcut opens Chrono, the screen time tracker's window.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            match startup::launch_for(&args) {
                Launch::Palette => {
                    // On Wayland the desktop's own shortcut runs this, so it stays out of games too.
                    if !args.iter().any(|a| a == startup::PALETTE_ARG) || palette::shortcut_may_open() {
                        palette::toggle(app);
                    }
                }
                Launch::Chrono => show_main(app),
                Launch::Dashboard => show_dashboard(app, None),
                Launch::Quiet => {}
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![startup::MINIMIZED_ARG]),
        ))
        .plugin(tauri_plugin_notification::init())
        // The command palette copies its answers.
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
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
                    // A game or full-screen app in front keeps its keys to itself, unless
                    // Settings lets the palette open over it.
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed
                        && palette::shortcut_may_open()
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

            let launch = startup::launch_for(&std::env::args().collect::<Vec<_>>());
            let conn = db::open(&db_path)?;
            if let Err(e) = startup::apply_default(app.handle(), &conn) {
                eprintln!("startup: cannot enable start at login: {e}");
            }
            let palette_shortcut = palette::shortcut(&conn)?;
            palette::over_fullscreen(&conn)?;
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
            // Files join the web palette's rows when the index answers, if they're still wanted.
            // Windows' own palette keeps a worker of its own.
            #[cfg(not(windows))]
            let handle = app.handle().clone();
            #[cfg(not(windows))]
            app.manage(palette::files::Worker::new(palette::files::ask_system, move |seq, items| {
                if let Some(rows) = handle.state::<palette::Session>().add_files(seq, items) {
                    if let Err(e) = handle.emit_to(palette::LABEL, "palette:files", palette::FilesFound { seq, rows }) {
                        eprintln!("palette: cannot show files: {e}");
                    }
                }
            }));
            #[cfg(windows)]
            palette::native::spawn(app.handle().clone(), db_path.clone(), launch == Launch::Palette);
            let scripted_tab = browser::SharedScripted::default();
            #[cfg(target_os = "macos")]
            {
                platform::request_window_titles();
                browser::spawn_scripted(scripted_tab.clone(), paused.clone(), browser_tab.clone());
            }
            #[cfg(windows)]
            browser::spawn_scripted(scripted_tab.clone(), paused.clone(), browser_tab.clone());
            collector::spawn(db_path.clone(), paused.clone(), privacy_rules.clone(), browser_tab.clone(), scripted_tab);
            goals::spawn(app.handle().clone(), db_path.clone(), paused.clone());
            sync::spawn(db_path.clone(), privacy_rules.clone(), window_cache.clone());
            calendar::spawn(db_path.clone());
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
                dashboard_tab: Mutex::default(),
            });
            setup_tray(app, paused, &palette_shortcut)?;
            // Wayland shows apps no other windows; on GNOME, Kairo's extension does, once GNOME
            // has loaded it at a login, and on KDE Plasma its KWin script does right away.
            #[cfg(not(any(windows, target_os = "macos")))]
            {
                if platform::watch_wayland() {
                    use tauri_plugin_notification::NotificationExt;
                    let _ = app
                        .notification()
                        .builder()
                        .title("Log out and back in to finish setting up Chrono")
                        .body("GNOME turns on the extension that lets Chrono see your apps at your next login.")
                        .show();
                }
                let handle = app.handle().clone();
                platform::on_palette_shortcut(Box::new(move || {
                    if palette::shortcut_may_open() {
                        palette::toggle(&handle);
                    }
                }));
            }
            // Wayland keeps keys from apps, so there the desktop's own shortcuts open the palette.
            // Another app holding the keys mustn't stop Kairo starting; the clash is only logged.
            if !platform::desktop_palette_shortcut(&palette_shortcut) {
                if let Err(e) = app.global_shortcut().register(palette_shortcut.as_str()) {
                    eprintln!("palette: cannot use {palette_shortcut}: {e}");
                }
            }
            updater::spawn(app.handle().clone());
            // Chrono's window is made when it's wanted; at sign-in Kairo starts in the tray, and
            // opened by hand it's the palette.
            in_dock(app.handle(), false);
            match launch {
                Launch::Chrono => show_main(app.handle()),
                Launch::Dashboard => show_dashboard(app.handle(), None),
                // Windows' palette opens itself once its window exists.
                #[cfg(not(windows))]
                Launch::Palette => palette::toggle(app.handle()),
                _ => {}
            }
            Ok(())
        })
        // On macOS and Linux the palette is a web page, which goes away when something else
        // takes the keyboard; Windows' own palette watches for that itself.
        .on_window_event(|window, event| {
            #[cfg(not(windows))]
            palette::web::on_window_event(window, event);
            if matches!(event, tauri::WindowEvent::Destroyed) && matches!(window.label(), "main" | "dashboard") {
                // The window being destroyed may still be listed.
                let app = window.app_handle();
                let windows = app.webview_windows();
                let open: Vec<&str> = windows.keys().map(String::as_str).filter(|l| *l != window.label()).collect();
                in_dock(app, keep_in_dock(&open));
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
            commands::get_month,
            commands::get_year,
            commands::get_week_notes,
            commands::set_week_notes,
            commands::get_notes_between,
            commands::get_evening_summary,
            commands::set_evening_summary,
            commands::send_test_evening_summary,
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
            commands::get_focus_schedule,
            commands::get_projects,
            commands::save_project,
            commands::remove_project,
            commands::get_project_times,
            commands::get_project_detail,
            commands::add_focus_slot,
            commands::remove_focus_slot,
            commands::set_slot_blocks,
            commands::set_slot_list,
            commands::get_block_lists,
            commands::save_block_list,
            commands::remove_block_list,
            commands::set_default_block_list,
            commands::set_project_target,
            commands::get_goals,
            commands::get_away,
            commands::label_away,
            commands::dismiss_away,
            commands::get_calendar,
            commands::set_calendar,
            commands::get_goal_settings,
            commands::get_week_goals,
            commands::set_goal,
            commands::set_limit,
            commands::set_limit_hard,
            commands::set_privacy_rule,
            commands::apply_privacy_to_past,
            commands::get_retention,
            commands::set_retention,
            commands::delete_history,
            commands::get_extension_status,
            commands::open_extension_folder,
            commands::take_navigation,
            commands::open_dashboard,
            commands::take_dashboard_tab,
            commands::open_chrono,
            commands::open_palette,
            commands::palette_open,
            commands::palette_query,
            commands::palette_icons,
            commands::palette_card,
            commands::palette_run,
            commands::palette_run_second,
            commands::palette_hide,
            commands::get_palette_settings,
            commands::set_palette_shortcut,
            commands::set_palette_suggestions,
            commands::set_palette_search_files,
            commands::set_palette_over_fullscreen,
            commands::get_appearance,
            commands::set_appearance,
            commands::get_app_icons,
            commands::get_version,
            commands::check_for_updates,
            commands::seed_demo_data,
            commands::export_time,
            commands::backup_data,
            commands::restore_data,
            commands::get_sync,
            commands::choose_sync_folder,
            commands::stop_sync,
            commands::set_shown_computer,
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

/// Whether a click on the tray icon opens the dashboard: the left button, once it's released.
fn tray_click_opens_dashboard(button: MouseButton, state: MouseButtonState) -> bool {
    button == MouseButton::Left && state == MouseButtonState::Up
}

fn setup_tray(app: &tauri::App, paused: Arc<AtomicBool>, palette_shortcut: &str) -> tauri::Result<()> {
    let dashboard = MenuItem::with_id(app, "dashboard", "Open dashboard", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Open Chrono", true, None::<&str>)?;
    let updates = MenuItem::with_id(app, "updates", "Check for updates", true, None::<&str>)?;
    // The shortcut shows beside it; the menu opens the palette even where the shortcut doesn't,
    // like over a game, since choosing it from the tray is no stray key press.
    let palette_item = MenuItem::with_id(app, "palette", "Command palette", true, Some(palette_shortcut))?;
    let pause = MenuItem::with_id(app, "pause", "Pause tracking", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Kairo", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&dashboard, &open, &updates, &palette_item, &PredefinedMenuItem::separator(app)?, &pause, &PredefinedMenuItem::separator(app)?, &quit],
    )?;
    app.manage(PaletteItem(palette_item));

    let pause_item = pause.clone();
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("bundle has an icon"))
        .tooltip("Kairo")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "dashboard" => show_dashboard(app, None),
            "open" => show_main(app),
            // As in Kairo's settings: the result arrives as a notification.
            "updates" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move { updater::check(&app, true).await });
            }
            "palette" => palette::toggle(app),
            "pause" => {
                let now_paused = !paused.fetch_xor(true, Ordering::Relaxed);
                let _ = pause_item.set_text(if now_paused { "Resume tracking" } else { "Pause tracking" });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Windows reports a quick second click as a double click instead.
            let (button, state) = match event {
                TrayIconEvent::Click { button, button_state, .. } => (button, button_state),
                TrayIconEvent::DoubleClick { button, .. } => (button, MouseButtonState::Up),
                _ => return,
            };
            if tray_click_opens_dashboard(button, state) {
                show_dashboard(tray.app_handle(), None);
            }
        })
        .build(app)?;
    Ok(())
}

/// The windows being made, by label, so a second click doesn't make another.
static OPENING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Mutex::default);

/// Claims the making of `label`'s window; false when one is already being made.
fn begin_opening(label: &str) -> bool {
    OPENING.lock().unwrap_or_else(|e| e.into_inner()).insert(label.to_string())
}

fn end_opening(label: &str) {
    OPENING.lock().unwrap_or_else(|e| e.into_inner()).remove(label);
}

/// Whether Kairo belongs in the Dock: while Chrono or the dashboard is open.
fn keep_in_dock(open: &[&str]) -> bool {
    open.iter().any(|l| *l == "main" || *l == "dashboard")
}

/// Brings up Chrono's window, making it if it was closed.
pub(crate) fn show_main(app: &AppHandle) {
    show_window(app, "main");
}

/// Brings up the dashboard, on `tab` when one is given. A window made for it is still loading
/// when this runs, so the page asks for the tab (`take_dashboard_tab`) once it's ready, and again
/// whenever it's told to.
pub(crate) fn show_dashboard(app: &AppHandle, tab: Option<&str>) {
    // A second launch can get here before setup has made the state; the window still opens. No
    // tab leaves one already asked for, so a click while the page loads doesn't undo it.
    if let (Some(state), Some(tab)) = (app.try_state::<AppState>(), tab) {
        *state.dashboard_tab.lock().unwrap_or_else(|e| e.into_inner()) = Some(tab.to_string());
    }
    show_window(app, "dashboard");
    let _ = app.emit_to("dashboard", "kairo:dashboard", ());
}

/// Brings up the window `label`, making it if it was closed. It isn't kept while closed: its web
/// view is most of the memory Kairo would use, and Kairo spends most of its time in the tray.
pub(crate) fn show_window(app: &AppHandle, label: &str) {
    in_dock(app, true);
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    // Making a web view from inside one of Windows' callbacks (a tray click, a menu) can hang,
    // so it's made on a thread of its own.
    if !begin_opening(label) {
        return;
    }
    let app = app.clone();
    let label = label.to_string();
    std::thread::spawn(move || {
        if let Err(e) = open_window(&app, &label) {
            eprintln!("cannot open the {label} window: {e}");
        }
        end_opening(&label);
    });
}

/// On a Mac, Kairo lives in the menu bar: it's in the Dock and the app switcher only while its
/// window is open, as Windows and Linux show it in the taskbar only then. Elsewhere, nothing.
fn in_dock(app: &AppHandle, shown: bool) {
    #[cfg(target_os = "macos")]
    {
        let policy = if shown { tauri::ActivationPolicy::Regular } else { tauri::ActivationPolicy::Accessory };
        if let Err(e) = app.set_activation_policy(policy) {
            eprintln!("cannot change Kairo's place in the Dock: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, shown);
}

fn open_window(app: &AppHandle, label: &str) -> tauri::Result<()> {
    if app.get_webview_window(label).is_some() {
        return Ok(());
    }
    let Some(config) = app.config().app.windows.iter().find(|w| w.label == label) else { return Ok(()) };
    let window = tauri::WebviewWindowBuilder::from_config(app, config)?.build()?;
    // Before setup has made the state (a second launch), the window opens in the default colors.
    let mode = app
        .try_state::<AppState>()
        .and_then(|state| state.db.lock().ok().and_then(|conn| appearance::get(&conn).ok()))
        .unwrap_or_default();
    appearance::apply_to(&window, &mode);
    window.show()?;
    window.set_focus()
}

/// Opens Kairo on a view, and for Today or Week a date. A window made for it is still loading
/// when this runs, so the page asks where to go (`take_navigation`) once it's ready, and again
/// whenever it's told to.
pub(crate) fn open_main_at(app: &AppHandle, view: &str, date: Option<NaiveDate>) {
    open_main_to(app, view, date, None);
}

/// Like `open_main_at`, and lands on the control with this element id once the view is shown.
pub(crate) fn open_main_to(app: &AppHandle, view: &str, date: Option<NaiveDate>, target: Option<&str>) {
    let target = Navigation { view: view.to_string(), date: date.map(|d| d.format("%Y-%m-%d").to_string()), target: target.map(str::to_string) };
    *app.state::<AppState>().navigation.lock().unwrap_or_else(|e| e.into_inner()) = Some(target);
    show_main(app);
    let _ = app.emit_to("main", "kairo:navigate", ());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_left_click_on_the_tray_opens_the_dashboard() {
        assert!(tray_click_opens_dashboard(MouseButton::Left, MouseButtonState::Up));
        assert!(!tray_click_opens_dashboard(MouseButton::Left, MouseButtonState::Down));
        assert!(!tray_click_opens_dashboard(MouseButton::Right, MouseButtonState::Up));
    }

    #[test]
    fn only_one_window_of_a_kind_opens_at_a_time() {
        assert!(begin_opening("dashboard"));
        assert!(!begin_opening("dashboard"));
        assert!(begin_opening("main"));
        end_opening("dashboard");
        end_opening("main");
        assert!(begin_opening("dashboard"));
        end_opening("dashboard");
    }

    #[test]
    fn kairo_stays_in_the_dock_while_either_window_is_open() {
        assert!(keep_in_dock(&["dashboard"]));
        assert!(keep_in_dock(&["main", "palette"]));
        assert!(!keep_in_dock(&["palette", "blocked"]));
        assert!(!keep_in_dock(&[]));
    }
}
