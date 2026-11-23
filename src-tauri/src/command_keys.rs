//! Keys of your own for your commands: a global shortcut on a custom command runs it from
//! anywhere, as Enter on its row in the palette would, without the palette showing. An app or one
//! of Kairo's own commands gets keys through an alias (`vs` for "visual studio code").
//!
//! Keys are kept with the command (`custom_commands.keys`) on this computer only. On Wayland,
//! which keeps keys from apps, GNOME's or KDE's own shortcuts run `kairo --command <name>`.

use std::sync::Mutex;

use rusqlite::Connection;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::custom_commands::{self, Kind};
use crate::palette::search::Action;

/// The keys registered now, and the command each runs: (keys, name).
static REGISTERED: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// The command whose keys these are, if any.
pub fn name_for(pressed: &tauri_plugin_global_shortcut::Shortcut) -> Option<String> {
    REGISTERED.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(keys, _)| crate::ring::same_keys(keys, pressed)).map(|(_, name)| name.clone())
}

/// Why `new` can't be `name`'s keys: Kairo's own, or another command's. None when they're free.
pub fn taken_by(new: &str, name: &str, kairo: &[(&str, &str)], commands: &[(String, String)]) -> Option<String> {
    let same = |a: &str, b: &str| !b.is_empty() && a.eq_ignore_ascii_case(b);
    if let Some((_, what)) = kairo.iter().find(|(keys, _)| same(new, keys)) {
        return Some(format!("{new} {what}."));
    }
    commands
        .iter()
        .find(|(keys, other)| same(new, keys) && !other.eq_ignore_ascii_case(name))
        .map(|(_, other)| format!("{new} already runs {other}."))
}

/// Gives command `name` the keys `new`, or with "" takes its keys away. If the new ones can't be
/// had, the old ones stay.
pub fn change(app: &AppHandle, conn: &Connection, name: &str, new: &str) -> Result<(), String> {
    let err = |e: rusqlite::Error| e.to_string();
    let all = custom_commands::list(conn).map_err(err)?;
    let cmd = all.iter().find(|c| c.name.eq_ignore_ascii_case(name.trim())).ok_or("That command is gone.")?;
    let old = cmd.keys.clone().unwrap_or_default();
    if !new.is_empty() && !new.eq_ignore_ascii_case(&old) {
        let palette = crate::palette::shortcut(conn).map_err(err)?;
        let ring = crate::ring::shortcut(conn).map_err(err)?;
        let capture = crate::capture::shortcut(conn).map_err(err)?;
        let kairo = [(palette.as_str(), "opens the command palette"), (ring.as_str(), "opens the app ring"), (capture.as_str(), "captures what you copied")];
        let commands: Vec<(String, String)> = all.iter().filter_map(|c| Some((c.keys.clone()?, c.name.clone()))).collect();
        if let Some(why) = taken_by(new, &cmd.name, &kairo, &commands) {
            return Err(why);
        }
        register(app, &cmd.name, new).map_err(|e| format!("{new} can't be used: {e}. Another app may already use it."))?;
    }
    if !old.is_empty() && !old.eq_ignore_ascii_case(new) {
        unregister(app, &cmd.name, &old);
    }
    custom_commands::set_keys_saved(conn, &cmd.name, new).map_err(err)?;
    refresh(app, conn);
    Ok(())
}

/// Matches what's registered to the commands as they are now: after a command is renamed,
/// removed, or replaced by another computer's. Keys no command has any more are let go; keys
/// that aren't registered yet (at startup) are taken.
pub fn refresh(app: &AppHandle, conn: &Connection) {
    let wanted: Vec<(String, String)> = custom_commands::list(conn).unwrap_or_default().into_iter().filter_map(|c| Some((c.keys?, c.name))).collect();
    let mut registered = REGISTERED.lock().unwrap_or_else(|e| e.into_inner());
    for (keys, name) in registered.iter() {
        if !wanted.iter().any(|(k, n)| k.eq_ignore_ascii_case(keys) && n.eq_ignore_ascii_case(name)) {
            unregister(app, name, keys);
        }
    }
    let mut kept = Vec::new();
    for (keys, name) in wanted {
        let had = registered.iter().any(|(k, n)| k.eq_ignore_ascii_case(&keys) && n.eq_ignore_ascii_case(&name));
        match had || register(app, &name, &keys).is_ok() {
            true => kept.push((keys, name)),
            false => eprintln!("command keys: cannot use {keys} for {name}"),
        }
    }
    *registered = kept;
}

fn register(app: &AppHandle, name: &str, keys: &str) -> Result<(), String> {
    if crate::platform::desktop_command_shortcut(name, keys) {
        return Ok(());
    }
    app.global_shortcut().register(keys).map_err(|e| e.to_string())
}

fn unregister(app: &AppHandle, name: &str, keys: &str) {
    if !crate::platform::forget_desktop_command_shortcut(name) {
        let _ = app.global_shortcut().unregister(keys);
    }
}

/// Runs command `name` as its palette row would, on a thread of its own; what goes wrong is said
/// in a notification, since there's no palette to say it in.
pub fn run(app: &AppHandle, name: &str) {
    if !crate::palette::shortcut_may_open() {
        return;
    }
    let (app, name) = (app.clone(), name.to_string());
    std::thread::spawn(move || {
        if let Err(why) = run_now(&app, &name) {
            crate::ring::tell(&app, &why);
        }
    });
}

fn run_now(app: &AppHandle, name: &str) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    let index = app.state::<crate::palette::Index>();
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let cmd = custom_commands::list(&conn).map_err(|e| e.to_string())?.into_iter().find(|c| c.name.eq_ignore_ascii_case(name)).ok_or(format!("There's no command called {name} any more."))?;
    let windows = crate::palette::open_windows();
    let front = windows.first().map(|w| w.id.clone());
    let action = match cmd.kind {
        Kind::Alias => {
            let (ctx, _) = crate::palette::context(app, &conn, &index, windows);
            crate::palette::search::search(&ctx, &cmd.target).into_iter().next().map(|i| i.action).unwrap_or(Action::Nothing)
        }
        Kind::Link if cmd.takes_query() => Action::OpenUrl(custom_commands::fill_link(&cmd.target, "")),
        _ => cmd.action(front),
    };
    if matches!(action, Action::Nothing | Action::Type(..) | Action::EnterMode(..)) {
        return Err(format!("{} needs more typed after it. Open the palette and type “{}”.", cmd.name, cmd.target));
    }
    crate::palette::perform(app, &conn, &index, &action)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_refused_when_kairo_or_another_command_has_them() {
        let kairo = [("Ctrl+Space", "opens the command palette"), ("Alt+Q", "opens the app ring"), ("", "captures what you copied")];
        let commands = vec![("Ctrl+Alt+S".to_string(), "sig".to_string())];
        assert_eq!(taken_by("ctrl+space", "vs", &kairo, &commands), Some("ctrl+space opens the command palette.".into()));
        assert_eq!(taken_by("Ctrl+Alt+S", "vs", &kairo, &commands), Some("Ctrl+Alt+S already runs sig.".into()));
        // A command's own keys, and capture's empty ones, take nothing.
        assert_eq!(taken_by("Ctrl+Alt+S", "SIG", &kairo, &commands), None);
        assert_eq!(taken_by("Ctrl+Alt+V", "vs", &kairo, &commands), None);
    }
}
