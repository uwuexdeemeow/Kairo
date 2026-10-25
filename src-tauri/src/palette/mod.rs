//! The command palette: a small window a global shortcut brings up over whatever you're doing, to
//! open apps, jump around Kairo and run its actions. It's drawn natively (see `native`); this is
//! what it knows: the Start menu's apps and its settings.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, Result};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::db;

#[cfg(windows)]
pub mod native;
pub mod search;

pub const DEFAULT_SHORTCUT: &str = "Ctrl+Space";
const SHORTCUT_KEY: &str = "palette_shortcut";
const SUGGESTIONS_KEY: &str = "palette_suggestions";
/// The Start menu is read again after this long, so new installs show up.
const INDEX_FOR_MS: i64 = 5 * 60_000;
/// Usage counts toward an app's rank over this window.
const USAGE_WINDOW_MS: i64 = 30 * 24 * 60 * 60_000;
/// Shortcuts that aren't apps: uninstallers, readmes, links to websites.
const NOISE: &[&str] = &["uninstall", "readme", "read me", "release notes", "website", "documentation", "license", "help", "manual"];

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    name: String,
    /// The Start menu shortcut, which is what gets opened.
    path: String,
    /// Time spent in an app of the same name over the last 30 days, for ranking.
    used_ms: i64,
}

/// The Start menu's apps, kept a few minutes between reads.
#[derive(Default)]
pub struct Index(Mutex<Option<(i64, Vec<AppEntry>)>>);

fn start_menu_dirs() -> Vec<PathBuf> {
    let programs = |base: Option<std::ffi::OsString>| base.map(|b| PathBuf::from(b).join(r"Microsoft\Windows\Start Menu\Programs"));
    // Your own shortcuts first, so they win over machine-wide ones of the same name.
    [programs(std::env::var_os("APPDATA")), programs(std::env::var_os("ProgramData"))].into_iter().flatten().collect()
}

fn walk(dir: &Path, depth: u32, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < 4 {
                walk(&path, depth + 1, out);
            }
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk")) {
            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                out.push((name.to_string(), path.display().to_string()));
            }
        }
    }
}

/// Every app shortcut in the Start menu, by name, without the noise.
pub fn start_menu_apps() -> Vec<(String, String)> {
    let mut found = Vec::new();
    for dir in start_menu_dirs() {
        walk(&dir, 0, &mut found);
    }
    let mut seen = std::collections::HashSet::new();
    found
        .into_iter()
        .filter(|(name, _)| {
            let lower = name.to_lowercase();
            !NOISE.iter().any(|n| lower.contains(n)) && seen.insert(lower)
        })
        .collect()
}

fn usage(conn: &Connection, now: i64) -> Result<HashMap<String, i64>> {
    let mut stmt = conn.prepare(
        "SELECT lower(app), SUM(end_ms - start_ms) FROM sessions
         WHERE idle = 0 AND demo = 0 AND start_ms >= ?1 GROUP BY lower(app)",
    )?;
    let rows = stmt.query_map([now - USAGE_WINDOW_MS], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

/// The apps the palette can open, most used first.
pub fn apps(conn: &Connection, index: &Index, now: i64) -> Result<Vec<AppEntry>> {
    let mut cached = index.0.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, apps)) = cached.as_ref() {
        if now - at < INDEX_FOR_MS {
            return Ok(apps.clone());
        }
    }
    let used = usage(conn, now)?;
    let mut apps: Vec<AppEntry> = start_menu_apps()
        .into_iter()
        .map(|(name, path)| {
            let used_ms = used.get(&name.to_lowercase()).copied().unwrap_or(0);
            AppEntry { name, path, used_ms }
        })
        .collect();
    apps.sort_by(|a, b| b.used_ms.cmp(&a.used_ms).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    *cached = Some((now, apps.clone()));
    Ok(apps)
}

/// Opens an app, but only one the palette listed, never an arbitrary path.
pub fn launch(index: &Index, path: &str) -> std::result::Result<(), String> {
    let known = index.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|(_, apps)| apps.iter().any(|a| a.path == path));
    if !known {
        return Err("That isn't one of the apps Kairo found in the Start menu.".into());
    }
    if crate::platform::open_path(path) {
        Ok(())
    } else {
        Err("Windows couldn't open it.".into())
    }
}

pub fn shortcut(conn: &Connection) -> Result<String> {
    Ok(db::get_setting(conn, SHORTCUT_KEY)?.unwrap_or_else(|| DEFAULT_SHORTCUT.to_string()))
}

pub fn suggestions(conn: &Connection) -> Result<bool> {
    Ok(db::get_setting(conn, SUGGESTIONS_KEY)?.as_deref() != Some("0"))
}

pub fn set_suggestions(conn: &Connection, on: bool) -> Result<()> {
    db::set_setting(conn, SUGGESTIONS_KEY, if on { "1" } else { "0" })
}

/// Swaps the global shortcut. If the new one can't be had (another app holds it, or Windows
/// doesn't know the keys), the old one stays and the reason comes back.
pub fn change_shortcut(app: &AppHandle, conn: &Connection, new: &str) -> std::result::Result<(), String> {
    let old = shortcut(conn).map_err(|e| e.to_string())?;
    let keys = app.global_shortcut();
    let _ = keys.unregister(old.as_str());
    if let Err(e) = keys.register(new) {
        let _ = keys.register(old.as_str());
        return Err(format!("{new} can't be used: {e}. Another app may already use it."));
    }
    db::set_setting(conn, SHORTCUT_KEY, new).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_start_menu_lists_apps_without_uninstallers() {
        let apps = start_menu_apps();
        // Every Windows install has some Start menu shortcuts.
        assert!(!apps.is_empty());
        assert!(apps.iter().all(|(name, path)| !name.to_lowercase().contains("uninstall") && path.ends_with(".lnk")));
        let mut names: Vec<String> = apps.iter().map(|(n, _)| n.to_lowercase()).collect();
        let count = names.len();
        names.dedup();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), count, "names are unique");
    }

    #[test]
    fn only_listed_apps_can_be_launched() {
        let index = Index::default();
        assert!(launch(&index, r"C:\Windows\System32\cmd.exe").is_err());
    }
}
