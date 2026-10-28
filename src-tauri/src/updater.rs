//! Self-update from GitHub Releases: a quiet check shortly after launch and every few hours,
//! plus "Check for updates" in Kairo's settings (Updates).

use std::thread;
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;

/// Let sign-in settle before going to the network.
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(60);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// Dev builds would "update" themselves into the installed release, so they never install.
const AVAILABLE: bool = !cfg!(debug_assertions);

pub fn spawn(app: AppHandle) {
    if !AVAILABLE {
        return;
    }
    thread::Builder::new()
        .name("kairo-updater".into())
        .spawn(move || {
            thread::sleep(FIRST_CHECK_AFTER);
            loop {
                tauri::async_runtime::block_on(check(&app, false));
                thread::sleep(CHECK_EVERY);
            }
        })
        .expect("failed to start updater thread");
}

/// Installs a newer release if there is one. Background checks stay silent unless they find an
/// update; `manual` checks also report "up to date" and errors.
pub async fn check(app: &AppHandle, manual: bool) {
    if !AVAILABLE {
        if manual {
            notify(app, "Updates are off in development builds", "Install Kairo from a release to get updates.");
        }
        return;
    }
    match install_latest(app).await {
        Ok(()) if manual => {
            let version = &app.package_info().version;
            notify(app, "Kairo is up to date", &format!("You have the latest version, {version}."));
        }
        Err(e) if manual => notify(app, "Couldn't check for updates", &e.to_string()),
        Err(e) => eprintln!("updater: {e}"),
        Ok(()) => {}
    }
}

/// Returns only when there is nothing newer; installing ends this process and restarts Kairo.
async fn install_latest(app: &AppHandle) -> tauri_plugin_updater::Result<()> {
    let Some(update) = app.updater()?.check().await? else {
        return Ok(());
    };
    notify(app, "Updating Kairo", &format!("Installing version {}. Kairo restarts in a moment.", update.version));
    update.download_and_install(|_, _| {}, || {}).await?;
    // On Windows the installer has usually closed Kairo by now; this covers the rest.
    app.restart()
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("updater: cannot notify: {e}");
    }
}
