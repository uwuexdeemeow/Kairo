//! The command palette: a small window a global shortcut brings up over whatever you're doing, to
//! open apps, jump around Kairo and run its actions. Windows draws it natively (`native`); macOS
//! and Linux show a small page in a web view (`web`, `src/palette.ts`). Both show what `search`
//! finds in what's gathered here: the installed apps, today so far, and the settings.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{Local, NaiveDate, TimeZone};
use rusqlite::{Connection, Result};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::analysis::Category;
use crate::goals::Kind;
use crate::{db, focus, goals};
use search::{fmt_dur, Action, Context, Glyph, Icon, Item, Tone};

pub mod cards;
pub mod files;
#[cfg(windows)]
pub mod native;
pub mod search;
pub mod settings;
#[cfg(not(windows))]
pub mod web;

/// The web palette's window; Windows has none of these.
pub const LABEL: &str = "palette";
/// On a Mac, Ctrl+Space switches input sources, and Option+Space is where launchers live.
pub const DEFAULT_SHORTCUT: &str = if cfg!(target_os = "macos") { "Alt+Space" } else { "Ctrl+Space" };
const SHORTCUT_KEY: &str = "palette_shortcut";
const SUGGESTIONS_KEY: &str = "palette_suggestions";
const SEARCH_FILES_KEY: &str = "palette_search_files";
const OVER_FULLSCREEN_KEY: &str = "palette_over_fullscreen";
/// Whether the shortcut opens the palette over a game or full-screen app too, as saved; kept
/// here so each key press needn't ask the database.
static OVER_FULLSCREEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The installed apps are read again after this long, so new installs show up.
const INDEX_FOR_MS: i64 = 5 * 60_000;
/// Usage counts toward an app's rank over this window.
const USAGE_WINDOW_MS: i64 = 30 * 24 * 60 * 60_000;
/// Entries that aren't apps: uninstallers, readmes, links to websites.
const NOISE: &[&str] = &["uninstall", "readme", "read me", "release notes", "website", "documentation", "license", "help", "manual"];

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    name: String,
    /// What gets opened: a Start menu shortcut, an app bundle or a desktop entry.
    path: String,
    /// Time spent in an app of the same name over the last 30 days, for ranking.
    used_ms: i64,
}

/// The installed apps, kept a few minutes between reads.
#[derive(Default)]
pub struct Index(Mutex<Option<(i64, Vec<AppEntry>)>>);

impl Index {
    /// Whether `path` is one of the apps last listed.
    pub fn knows(&self, path: &str) -> bool {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|(_, apps)| apps.iter().any(|a| a.path == path))
    }
}

/// Opens the palette, or puts it away if it's open.
pub fn toggle(app: &AppHandle) {
    #[cfg(windows)]
    {
        let _ = app;
        native::toggle();
    }
    #[cfg(not(windows))]
    web::toggle(app);
}

// ---------- the installed apps ----------

/// Files under `dir`, `depth` folders down at most, whose names end in `extension`, by name.
/// Folders that end in it count as files: a Mac app is a folder.
fn walk(dir: &Path, depth: u32, extension: &str, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case(extension)) {
            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                out.push((name.to_string(), path));
            }
        } else if depth > 0 && path.is_dir() {
            walk(&path, depth - 1, extension, out);
        }
    }
}

/// The Start menu's All apps list, the way Windows shows it: programs, Store apps, and games
/// that launchers like Steam add as links. Should Windows not give it, its shortcut folders,
/// yours before the machine's so yours win on a shared name.
#[cfg(windows)]
fn app_files() -> Vec<(String, String)> {
    if let Some(apps) = crate::platform::start_apps().filter(|a| !a.is_empty()) {
        return apps;
    }
    let programs = |base: Option<std::ffi::OsString>| base.map(|b| PathBuf::from(b).join(r"Microsoft\Windows\Start Menu\Programs"));
    let mut found = Vec::new();
    for dir in [programs(std::env::var_os("APPDATA")), programs(std::env::var_os("ProgramData"))].into_iter().flatten() {
        walk(&dir, 4, "lnk", &mut found);
    }
    found.into_iter().map(|(name, path)| (name, path.display().to_string())).collect()
}

/// App bundles where a Mac keeps them, one folder down for the likes of Utilities.
#[cfg(target_os = "macos")]
fn app_files() -> Vec<(String, String)> {
    let home = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Applications"));
    let dirs = [Some(PathBuf::from("/Applications")), Some(PathBuf::from("/System/Applications")), home];
    let mut found = Vec::new();
    for dir in dirs.into_iter().flatten() {
        walk(&dir, 1, "app", &mut found);
    }
    found.into_iter().map(|(name, path)| (name, path.display().to_string())).collect()
}

/// Desktop entries, as a Linux app menu lists them: yours first, then each data folder's, and an
/// entry yours hides or overrides stays that way.
#[cfg(not(any(windows, target_os = "macos")))]
fn app_files() -> Vec<(String, String)> {
    let mut seen = std::collections::HashSet::new();
    let mut found = Vec::new();
    for dir in linux::data_dirs() {
        let mut files = Vec::new();
        walk(&dir.join("applications"), 3, "desktop", &mut files);
        for (_, path) in files {
            let Some(id) = path.file_name().map(|n| n.to_os_string()) else { continue };
            if !seen.insert(id) {
                continue;
            }
            if let Some(entry) = std::fs::read_to_string(&path).ok().and_then(|text| linux::DesktopEntry::parse(&text)) {
                found.push((entry.name, path.display().to_string()));
            }
        }
    }
    found
}

/// Every app the palette can open, by name, without the noise or repeats.
pub fn installed_apps() -> Vec<(String, String)> {
    let mut seen = std::collections::HashSet::new();
    app_files()
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
    let mut apps: Vec<AppEntry> = installed_apps()
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
    if !index.knows(path) {
        return Err("That isn't one of the apps Kairo found installed.".into());
    }
    if crate::platform::open_path(path) {
        Ok(())
    } else {
        Err("It couldn't be opened.".into())
    }
}

/// An app's icon for the web palette, as a data URL; None leaves its letter.
pub fn icon_url(path: &str) -> Option<String> {
    use base64::Engine;
    #[cfg(not(any(windows, target_os = "macos")))]
    let (mime, data) = linux::icon(path)?;
    #[cfg(any(windows, target_os = "macos"))]
    let (mime, data) = ("image/png", crate::platform::icon_png(path)?);
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(data)))
}

/// The icon of a program Kairo tracked, by its file, for Kairo's pages; on Linux, the icon of
/// the desktop entry that runs it.
pub fn program_icon_url(path: &str) -> Option<String> {
    #[cfg(not(any(windows, target_os = "macos")))]
    let path = &linux::entry_for_program(path, &app_files())?;
    icon_url(path)
}

// ---------- settings ----------

pub fn shortcut(conn: &Connection) -> Result<String> {
    Ok(db::get_setting(conn, SHORTCUT_KEY)?.unwrap_or_else(|| DEFAULT_SHORTCUT.to_string()))
}

pub fn suggestions(conn: &Connection) -> Result<bool> {
    Ok(db::get_setting(conn, SUGGESTIONS_KEY)?.as_deref() != Some("0"))
}

pub fn set_suggestions(conn: &Connection, on: bool) -> Result<()> {
    db::set_setting(conn, SUGGESTIONS_KEY, if on { "1" } else { "0" })
}

/// Whether typing in the palette also looks for files; on unless turned off.
pub fn search_files(conn: &Connection) -> Result<bool> {
    Ok(db::get_setting(conn, SEARCH_FILES_KEY)?.as_deref() != Some("0"))
}

pub fn set_search_files(conn: &Connection, on: bool) -> Result<()> {
    db::set_setting(conn, SEARCH_FILES_KEY, if on { "1" } else { "0" })
}

/// Off unless asked for, so the shortcut can't steal keys mid-game.
pub fn over_fullscreen(conn: &Connection) -> Result<bool> {
    let on = db::get_setting(conn, OVER_FULLSCREEN_KEY)?.as_deref() == Some("1");
    OVER_FULLSCREEN.store(on, std::sync::atomic::Ordering::Relaxed);
    Ok(on)
}

pub fn set_over_fullscreen(conn: &Connection, on: bool) -> Result<()> {
    db::set_setting(conn, OVER_FULLSCREEN_KEY, if on { "1" } else { "0" })?;
    OVER_FULLSCREEN.store(on, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// Whether the shortcut may open the palette now: not over a game or full-screen app, unless
/// Settings says so.
pub fn shortcut_may_open() -> bool {
    OVER_FULLSCREEN.load(std::sync::atomic::Ordering::Relaxed) || !crate::platform::fullscreen_in_front()
}

/// Swaps the global shortcut. If the new one can't be had (another app holds it, or the system
/// doesn't know the keys), the old one stays and the reason comes back.
pub fn change_shortcut(app: &AppHandle, conn: &Connection, new: &str) -> std::result::Result<(), String> {
    let old = shortcut(conn).map_err(|e| e.to_string())?;
    if !crate::platform::desktop_palette_shortcut(new) {
        let keys = app.global_shortcut();
        let _ = keys.unregister(old.as_str());
        if let Err(e) = keys.register(new) {
            let _ = keys.register(old.as_str());
            return Err(format!("{new} can't be used: {e}. Another app may already use it."));
        }
    }
    db::set_setting(conn, SHORTCUT_KEY, new).map_err(|e| e.to_string())?;
    if let Some(item) = tauri::Manager::try_state::<crate::PaletteItem>(app) {
        let _ = item.0.set_accelerator(Some(new));
    }
    Ok(())
}

// ---------- what both palettes show ----------

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// `part` as a rounded percentage of `whole`.
pub fn percent(part: i64, whole: i64) -> i64 {
    if whole > 0 {
        (part * 100 + whole / 2) / whole
    } else {
        0
    }
}

/// What's known as the palette opens (apps, the focus session, today's apps and limits, what
/// can be asked about), and the footer's line about today.
pub fn context(app: &AppHandle, conn: &Connection, index: &Index) -> (Context, String) {
    let now = now_ms();
    let today = Local::now().date_naive();
    let apps = apps(conn, index, now).unwrap_or_default();
    let session = focus::active(conn).ok().flatten().map(|a| a.label().map(str::to_string));
    let rows = goals::rows_for(conn, today, now).unwrap_or_default();
    let glance = crate::analysis::glance(&rows);
    let over_limits = goals::load(conn)
        .map(|g| goals::progress(&g, &rows, &HashMap::new()))
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.kind != Kind::Productive && !p.met)
        .map(|p| (p.label.clone(), p.used_ms, p.target_ms))
        .collect();
    let footer = format!("{} today · {}% productive", fmt_dur(glance.active_ms), percent(glance.productive_ms, glance.active_ms));
    let subjects = cards::subjects(conn, &apps).unwrap_or_default();
    let ctx = Context {
        today,
        apps,
        session,
        today_apps: glance.top_apps,
        over_limits,
        suggestions: suggestions(conn).unwrap_or(true),
        search_files: search_files(conn).unwrap_or(true),
        switches: settings::read(app, conn),
        subjects,
        limits: cards::limits(conn),
        week_goals: week_goals(conn, today, now),
        block_lists: crate::lists::list(conn).unwrap_or_default().into_iter().map(|l| l.name).collect(),
        next_meeting: crate::calendar::next_after(conn, now).ok().flatten().and_then(|m| {
            let at = Local.timestamp_millis_opt(m.start_ms).single()?.naive_local();
            Some((m.title, at, m.start_ms - now))
        }),
    };
    (ctx, footer)
}

/// The weekly goals as one line, like "7h 53m of 15h 00m productive · 3h 39m of 5h 00m
/// distracting · 6h of 10h Kairo"; None with none set.
fn week_goals(conn: &Connection, today: chrono::NaiveDate, now: i64) -> Option<String> {
    let week = goals::for_week(conn, crate::recap::week_start(today), now).ok()?;
    let parts: Vec<String> = week
        .items
        .iter()
        .map(|p| {
            let what = if p.is_project() { p.label.as_str() } else if p.kind == Kind::Productive { "productive" } else { "distracting" };
            let over = if p.kind != Kind::Productive && !p.met { ", over" } else { "" };
            format!("{} of {} {what}{over}", fmt_dur(p.used_ms), fmt_dur(p.target_ms))
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// A stretch of a day, as shares of it from midnight.
#[derive(Serialize, Clone, Copy)]
pub struct Segment {
    pub from: f32,
    pub to: f32,
    pub category: Category,
}

/// How a day went, for the card above a selected day.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayGlance {
    pub active_ms: i64,
    pub productive_pct: i64,
    pub focus_count: usize,
    pub focus_ms: i64,
    pub segments: Vec<Segment>,
}

pub fn day_glance(conn: &Connection, day: NaiveDate) -> DayGlance {
    let now = now_ms();
    let rows = goals::rows_for(conn, day, now).unwrap_or_default();
    let glance = crate::analysis::glance(&rows);
    let (from, to) = crate::analysis::day_bounds(day).unwrap_or((0, 0));
    let blocks = focus::blocks(conn, from, to, now).unwrap_or_default();
    let share = |ms: i64| ((ms - from) as f32 / (to - from).max(1) as f32).clamp(0.0, 1.0);
    DayGlance {
        active_ms: glance.active_ms,
        productive_pct: percent(glance.productive_ms, glance.active_ms),
        focus_count: blocks.len(),
        focus_ms: blocks.iter().map(focus::Block::ms).sum(),
        segments: glance.segments.iter().map(|&(s, e, category)| Segment { from: share(s), to: share(e), category }).collect(),
    }
}

/// Does what an item says. A move to Kairo's window wants the palette out of the way first,
/// which is the caller's to do.
pub fn perform(app: &AppHandle, conn: &Connection, index: &Index, action: &Action) -> std::result::Result<(), String> {
    match action {
        Action::Launch(path) => launch(index, path),
        Action::Go(view, date) => {
            crate::open_main_at(app, view, *date);
            Ok(())
        }
        Action::Dashboard(tab) => {
            crate::show_dashboard(app, *tab);
            Ok(())
        }
        // Starting anew ends the running session, which strict mode only allows after its wait.
        Action::StartFocus(..) if crate::blocking::strict_now(conn).unwrap_or(false) => {
            Err("Strict mode is on: end the current session from Chrono's Focus page first.".into())
        }
        Action::StartFocus(minutes, label) => focus::start_by_hand(conn, *minutes, label.as_deref(), now_ms()).map(|_| ()).map_err(|e| e.to_string()),
        Action::CheckUpdates => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { crate::updater::check(&app, true).await });
            Ok(())
        }
        Action::CopyAnswer(subject, period) => {
            use tauri_plugin_clipboard_manager::ClipboardExt;
            app.clipboard().write_text(cards::answer(conn, subject, *period).text).map_err(|e| e.to_string())
        }
        Action::ExportTime | Action::Backup | Action::Restore => {
            with_file_dialog(app, action.clone());
            Ok(())
        }
        Action::OpenFile(path) if std::path::Path::new(path).exists() && crate::platform::open_path(path) => Ok(()),
        Action::OpenFile(_) => Err("Couldn't open it: it was moved or deleted.".into()),
        Action::RevealFile(path) if files::reveal(path) => Ok(()),
        Action::RevealFile(_) => Err("Couldn't show it in its folder.".into()),
        // The caller shows the new state by asking again, so the result is of no use here.
        Action::Toggle(switch) => settings::flip(app, conn, *switch).map(|_| ()),
        // Opens the window that has the setting and lands on its control.
        Action::Setting(control) => {
            match settings::entries().iter().find(|e| e.control == *control).map(|e| e.page) {
                Some(settings::Page::Kairo) => crate::show_dashboard(app, Some(&format!("settings#{control}"))),
                _ => crate::open_main_to(app, "settings", None, Some(control)),
            }
            Ok(())
        }
        Action::Nothing => Ok(()),
    }
}

/// Runs a data action off the palette's thread, since it waits on a file dialog, and says how it
/// went in a notification. A restore restarts Kairo and says nothing.
fn with_file_dialog(app: &AppHandle, action: Action) {
    use tauri_plugin_notification::NotificationExt;
    let app = app.clone();
    std::thread::spawn(move || {
        let said = match action {
            Action::ExportTime => crate::commands::export_time_dialog(&app).map(|rows| rows.map(|n| format!("Saved {n} rows of tracked time."))),
            Action::Backup => crate::commands::backup_dialog(&app).map(|name| name.map(|n| format!("Saved {n}."))),
            Action::Restore => crate::commands::restore_dialog(&app).map(|_| None),
            _ => return,
        };
        let (title, body) = match said {
            Ok(Some(body)) => ("Kairo", body),
            Ok(None) => return,
            Err(e) => ("Couldn't do that", e),
        };
        if let Err(e) = app.notification().builder().title(title).body(&body).show() {
            eprintln!("palette: cannot notify: {e}");
        }
    });
}

// ---------- the web palette's side ----------

/// An item as the web palette draws it.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    title: String,
    /// Characters of the title to underline.
    at: Vec<usize>,
    sub: Option<String>,
    detail: Option<String>,
    icon: IconView,
    section: &'static str,
    /// Whether it has a card to show while it's selected (`palette_card`).
    preview: bool,
    /// The label of what Ctrl+Enter does instead.
    second: Option<&'static str>,
}

#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum IconView {
    App { path: String },
    /// The kind is `file_kind` because `kind` already names the variant.
    File {
        path: String,
        #[serde(rename = "fileKind")]
        file_kind: &'static str,
    },
    Glyph { glyph: &'static str, tone: &'static str },
}

impl From<&Item> for ItemView {
    fn from(item: &Item) -> Self {
        let icon = match &item.icon {
            Icon::App(path) => IconView::App { path: path.clone() },
            Icon::File(path) => IconView::File { path: path.clone(), file_kind: files::file_kind(path) },
            Icon::Glyph(glyph, tone) => IconView::Glyph {
                glyph: match glyph {
                    Glyph::Focus => "focus",
                    Glyph::Day => "day",
                    Glyph::Chart => "chart",
                    Glyph::Tag => "tag",
                    Glyph::Settings => "settings",
                    Glyph::Today => "today",
                    Glyph::Alert => "alert",
                    Glyph::Update => "update",
                },
                tone: match tone {
                    Tone::Plain => "",
                    Tone::Accent => "accent",
                    Tone::Warm => "warm",
                },
            },
        };
        ItemView {
            title: item.title.clone(),
            at: item.at.clone(),
            sub: item.sub.clone(),
            detail: item.detail.clone(),
            icon,
            section: item.section,
            preview: item.preview.is_some(),
            second: item.second.as_ref().map(|(_, label)| *label),
        }
    }
}

/// Results for the web palette: rows, then (with nothing typed) recent apps as tiles and places
/// as chips. Items are numbered across all three, in that order.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Results {
    /// Which query these answer; running an item names it, so it's always one on screen.
    seq: u64,
    rows: Vec<ItemView>,
    tiles: Vec<ItemView>,
    chips: Vec<ItemView>,
}

impl Results {
    /// Which query these answer.
    pub fn seq(&self) -> u64 {
        self.seq
    }
}

/// File rows found for the results numbered `seq`, sent to the web palette as `palette:files`.
#[cfg(not(windows))]
#[derive(Serialize, Clone)]
pub struct FilesFound {
    pub seq: u64,
    pub rows: Vec<ItemView>,
}

#[derive(Default)]
struct SessionState {
    ctx: Option<Context>,
    items: Vec<Item>,
    seq: u64,
    /// What was typed for the results numbered `seq`, to ask again after a switch flips.
    query: String,
}

/// The web palette between keystrokes: what was known when it opened, and what it last showed.
#[derive(Default)]
pub struct Session(Mutex<SessionState>);

impl Session {
    /// Starts afresh. The numbering moves on, so files still coming for the last opening's
    /// query are dropped.
    pub fn open(&self, ctx: Context) {
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        s.ctx = Some(ctx);
        s.items.clear();
        s.query.clear();
        s.seq += 1;
    }

    pub fn query(&self, query: &str) -> Results {
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        s.seq += 1;
        s.query = query.to_string();
        let (rows, tiles, chips) = match &s.ctx {
            None => (Vec::new(), Vec::new(), Vec::new()),
            Some(ctx) if query.trim().is_empty() => {
                let found = search::suggestions(ctx);
                (found.rows, found.tiles, found.chips)
            }
            Some(ctx) => (search::search(ctx, query), Vec::new(), Vec::new()),
        };
        let view = |items: &[Item]| items.iter().map(ItemView::from).collect();
        let results = Results { seq: s.seq, rows: view(&rows), tiles: view(&tiles), chips: view(&chips) };
        s.items = rows.into_iter().chain(tiles).chain(chips).collect();
        results
    }

    /// The item at `index` in the results numbered `seq`; None once newer ones replaced them.
    pub fn item(&self, seq: u64, index: usize) -> Option<Item> {
        let s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        (s.seq == seq).then(|| s.items.get(index).cloned()).flatten()
    }

    /// What was typed for the latest results, to ask again after a switch flips.
    pub fn last_query(&self) -> String {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).query.clone()
    }

    /// Adds file rows after the rows of the results numbered `seq`, and returns how they're drawn.
    /// None once newer results replaced them, or when nothing is typed, since files would then
    /// land after the tiles and chips. Windows' own palette adds its files itself.
    #[cfg(any(not(windows), test))]
    pub fn add_files(&self, seq: u64, items: Vec<Item>) -> Option<Vec<ItemView>> {
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if s.seq != seq || s.query.trim().is_empty() {
            return None;
        }
        let views = items.iter().map(ItemView::from).collect();
        s.items.extend(items);
        Some(views)
    }

    /// Whether files are searched for `query`: the switch is on and enough is typed.
    pub fn wants_files(&self, query: &str) -> bool {
        let s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        s.ctx.as_ref().is_some_and(|c| wants_files(c, query))
    }

    /// Records that a switch was flipped, so the next results show its new state, and turns on
    /// or off what the palette itself does by it.
    pub fn set_switch(&self, switch: settings::Switch, on: bool) {
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ctx) = s.ctx.as_mut() {
            set_switch(ctx, switch, on);
        }
    }

    /// Whether `path` is an icon the palette is showing: an installed app's, the program of
    /// something it answers about, or a file in its rows.
    pub fn shows_icon(&self, path: &str) -> bool {
        let s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        s.ctx.as_ref().is_some_and(|c| c.subjects.iter().any(|sub| sub.icon.as_deref() == Some(path)))
            || (!path.is_empty() && s.items.iter().any(|i| matches!(&i.icon, Icon::File(p) if p == path)))
    }
}

/// Whether files are searched for `query`, in either palette: the switch is on and enough is typed.
pub fn wants_files(ctx: &Context, query: &str) -> bool {
    ctx.search_files && query.trim().chars().count() >= files::MIN_CHARS
}

/// Records in `ctx` that a switch was flipped, in either palette, so the next results show its new
/// state, and turns on or off what the palette itself does by it.
pub fn set_switch(ctx: &mut Context, switch: settings::Switch, on: bool) {
    if let Some(entry) = ctx.switches.iter_mut().find(|(sw, _)| *sw == switch) {
        entry.1 = on;
    }
    match switch {
        settings::Switch::SearchFiles => ctx.search_files = on,
        settings::Switch::Suggestions => ctx.suggestions = on,
        _ => {}
    }
}

// ---------- Linux ----------

#[cfg(not(any(windows, target_os = "macos")))]
mod linux {
    use std::path::{Path, PathBuf};

    /// Where desktop entries and icons live, most personal first (the XDG base directories).
    pub fn data_dirs() -> Vec<PathBuf> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let own = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).or_else(|| home.map(|h| h.join(".local/share")));
        let shared = std::env::var("XDG_DATA_DIRS").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        let mut dirs: Vec<PathBuf> = own.into_iter().chain(shared.split(':').map(PathBuf::from)).collect();
        // Snap and Flatpak apps, where the session didn't add their folders.
        for extra in ["/var/lib/snapd/desktop", "/var/lib/flatpak/exports/share"] {
            if !dirs.iter().any(|d| d == Path::new(extra)) {
                dirs.push(extra.into());
            }
        }
        dirs
    }

    pub struct DesktopEntry {
        pub name: String,
        pub icon: Option<String>,
        /// The command it runs, with placeholders like %U for files.
        pub exec: Option<String>,
        /// The window class its windows have, when the app says.
        pub wm_class: Option<String>,
    }

    impl DesktopEntry {
        /// An app's entry, or None for one that isn't an app or asks not to be shown.
        pub fn parse(text: &str) -> Option<Self> {
            let mut in_entry = false;
            let (mut name, mut icon, mut app, mut shown) = (None, None, false, true);
            let (mut exec, mut wm_class) = (None, None);
            for line in text.lines().map(str::trim) {
                if line.starts_with('[') {
                    in_entry = line == "[Desktop Entry]";
                    continue;
                }
                let Some((key, value)) = line.split_once('=').filter(|_| in_entry) else { continue };
                match (key.trim(), value.trim()) {
                    ("Name", v) => name = Some(v.to_string()),
                    ("Icon", v) if !v.is_empty() => icon = Some(v.to_string()),
                    ("Exec", v) if !v.is_empty() => exec = Some(v.to_string()),
                    ("StartupWMClass", v) if !v.is_empty() => wm_class = Some(v.to_string()),
                    ("Type", v) => app = v == "Application",
                    ("NoDisplay" | "Hidden", "true") => shown = false,
                    _ => {}
                }
            }
            if !app || !shown {
                return None;
            }
            Some(DesktopEntry { name: name.filter(|n| !n.is_empty())?, icon, exec, wm_class })
        }
    }

    /// Which of `entries` (name, desktop entry file) runs the program at `program`, for its icon.
    /// Best is an entry whose command is that program, or that Steam made for the game; then one
    /// whose windows carry the program's name; then one whose command sits in the program's own
    /// folder, as Chrome's launcher script does, preferring a plain launcher over a web app's.
    pub fn entry_for_program(program: &str, entries: &[(String, String)]) -> Option<String> {
        use crate::classify::{exe_stem, game_name};
        let stem = exe_stem(program);
        let game = game_name(program);
        let real = std::fs::canonicalize(program).ok();
        let folder = real.as_deref().and_then(Path::parent).filter(|d| !is_bin_dir(d));
        let mut best: Option<(u8, &String)> = None;
        for (name, file) in entries {
            let Some(entry) = std::fs::read_to_string(file).ok().and_then(|text| DesktopEntry::parse(&text)) else { continue };
            let runs = entry.exec.as_deref().and_then(command);
            let score = if game.as_deref().is_some_and(|g| g.eq_ignore_ascii_case(name)) {
                4
            } else if game.is_some() {
                0
            } else if runs.as_ref().is_some_and(|(raw, real_cmd, _)| {
                real.as_ref() == Some(real_cmd) || [raw, real_cmd].iter().any(|p| exe_stem(&p.to_string_lossy()) == stem)
            }) {
                4
            } else if entry.wm_class.as_deref().is_some_and(|c| c.eq_ignore_ascii_case(&stem)) {
                3
            } else {
                match &runs {
                    Some((_, real_cmd, extra)) if folder.is_some() && real_cmd.parent() == folder => if *extra { 1 } else { 2 },
                    _ => 0,
                }
            };
            if score > best.map_or(0, |(s, _)| s) {
                best = Some((score, file));
            }
        }
        best.map(|(_, file)| file.clone())
    }

    /// The program a desktop entry's command runs, as written and with links followed, and
    /// whether the command adds arguments of its own beyond the file and URL placeholders.
    fn command(exec: &str) -> Option<(PathBuf, PathBuf, bool)> {
        let mut words = exec.split_whitespace().map(|w| w.trim_matches('"')).skip_while(|w| *w == "env" || w.contains('='));
        let program = words.next()?;
        let extra = words.any(|w| !w.starts_with('%'));
        let raw = if program.contains('/') {
            PathBuf::from(program)
        } else {
            std::env::split_paths(&std::env::var_os("PATH")?).map(|dir| dir.join(program)).find(|p| p.is_file())?
        };
        let real = std::fs::canonicalize(&raw).unwrap_or_else(|_| raw.clone());
        Some((raw, real, extra))
    }

    /// Folders full of unrelated programs, where sharing one says nothing.
    fn is_bin_dir(dir: &Path) -> bool {
        matches!(dir.to_str(), Some("/bin" | "/sbin" | "/usr/bin" | "/usr/sbin" | "/usr/local/bin" | "/usr/games" | "/snap/bin"))
    }

    /// A desktop entry's icon from the shared icon theme, which every Linux desktop has, or the
    /// older pixmaps folder: (media type, file contents).
    pub fn icon(entry: &str) -> Option<(&'static str, Vec<u8>)> {
        let icon = DesktopEntry::parse(&std::fs::read_to_string(entry).ok()?)?.icon?;
        let typed = |path: &Path| {
            let mime = match path.extension()?.to_str()? {
                "png" => "image/png",
                "svg" => "image/svg+xml",
                _ => return None,
            };
            Some((mime, std::fs::read(path).ok()?))
        };
        if icon.starts_with('/') {
            return typed(Path::new(&icon));
        }
        for dir in data_dirs() {
            for size in ["48x48", "64x64", "128x128", "256x256", "32x32", "scalable"] {
                for ext in ["png", "svg"] {
                    if let Some(found) = typed(&dir.join(format!("icons/hicolor/{size}/apps/{icon}.{ext}"))) {
                        return Some(found);
                    }
                }
            }
        }
        ["png", "svg"].iter().find_map(|ext| typed(&PathBuf::from(format!("/usr/share/pixmaps/{icon}.{ext}"))))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn desktop_entries_name_apps_and_skip_hidden_ones() {
            let app = DesktopEntry::parse("[Desktop Entry]\nType=Application\nName=Firefox\nName[de]=Firefox\nIcon=firefox\n\n[Desktop Action new]\nName=New Window").unwrap();
            assert_eq!((app.name.as_str(), app.icon.as_deref()), ("Firefox", Some("firefox")));
            assert!(DesktopEntry::parse("[Desktop Entry]\nType=Application\nName=Helper\nNoDisplay=true").is_none());
            assert!(DesktopEntry::parse("[Desktop Entry]\nType=Link\nName=Docs").is_none());
        }

        #[test]
        fn tracked_programs_find_their_desktop_entry() {
            let dir = std::env::temp_dir().join(format!("kairo-entries-{}", std::process::id()));
            let app = dir.join("opt/chat");
            std::fs::create_dir_all(&app).unwrap();
            for file in ["chat-bin", "launcher"] {
                std::fs::write(app.join(file), "").unwrap();
            }
            let entry = |file: &str, text: String| {
                let path = dir.join(file);
                std::fs::write(&path, format!("[Desktop Entry]\nType=Application\n{text}")).unwrap();
                path.display().to_string()
            };
            let launcher = app.join("launcher").display().to_string();
            let web_app = entry("web.desktop", format!("Name=Web app\nExec={launcher} --app-id=abc"));
            let chat = entry("chat.desktop", format!("Name=Chat\nExec=env A=1 \"{launcher}\" %U"));
            let game = entry("game.desktop", "Name=Hades\nExec=steam steam://rungameid/1145360".into());
            let entries: Vec<(String, String)> =
                [("Web app", &web_app), ("Chat", &chat), ("Hades", &game)].iter().map(|(n, f)| (n.to_string(), f.to_string())).collect();

            // The plain launcher in the program's folder wins over a web app's.
            assert_eq!(entry_for_program(&app.join("chat-bin").display().to_string(), &entries), Some(chat));
            let hades = "/home/me/.local/share/Steam/steamapps/common/Hades/Hades";
            assert_eq!(entry_for_program(hades, &entries), Some(game));
            assert_eq!(entry_for_program("/usr/bin/unknown", &entries), None);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_stays_out_of_full_screen_apps_unless_asked() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        assert!(!over_fullscreen(&conn).unwrap());
        set_over_fullscreen(&conn, true).unwrap();
        assert!(over_fullscreen(&conn).unwrap());
        // Allowed over anything now, full-screen app or not.
        assert!(shortcut_may_open());
        set_over_fullscreen(&conn, false).unwrap();
        assert!(!over_fullscreen(&conn).unwrap());
    }

    #[test]
    fn installed_apps_come_without_uninstallers_or_repeats() {
        let apps = installed_apps();
        // Windows and macOS always have some; a bare Linux build machine may have none.
        if cfg!(any(windows, target_os = "macos")) {
            assert!(!apps.is_empty());
        }
        assert!(apps.iter().all(|(name, _)| !name.to_lowercase().contains("uninstall")));
        let mut names: Vec<String> = apps.iter().map(|(n, _)| n.to_lowercase()).collect();
        let count = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), count, "names are unique");
    }

    #[test]
    fn only_listed_apps_can_be_launched() {
        let index = Index::default();
        assert!(launch(&index, r"C:\Windows\System32\cmd.exe").is_err());
    }

    fn ctx(today: NaiveDate) -> Context {
        Context {
            today,
            apps: Vec::new(),
            session: None,
            today_apps: Vec::new(),
            over_limits: Vec::new(),
            suggestions: true,
            search_files: true,
            switches: Vec::new(),
            subjects: Vec::new(),
            limits: HashMap::new(),
            week_goals: None,
            block_lists: Vec::new(),
            next_meeting: None,
        }
    }

    /// File rows for "bud", as the worker would hand them over.
    fn budget_files() -> Vec<Item> {
        let home = Path::new("/home/ana");
        files::rows("bud", home, vec![home.join("budget.xlsx"), home.join("budget.txt")])
    }

    #[test]
    fn results_number_items_for_running_them() {
        let session = Session::default();
        let today = NaiveDate::from_ymd_opt(2026, 10, 26).unwrap();
        session.open(ctx(today));
        let first = session.query("");
        assert!(!first.chips.is_empty());
        let second = session.query("yesterday");
        assert!(second.rows[0].preview);
        // An item from results that have been replaced isn't run.
        assert!(session.item(first.seq, 0).is_none());
        assert_eq!(session.item(second.seq, 0).map(|i| i.action), Some(Action::Go("today", Some(today.pred_opt().unwrap()))));
    }

    #[test]
    fn files_for_an_old_query_are_dropped() {
        let session = Session::default();
        session.open(ctx(NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()));
        let first = session.query("bu");
        let second = session.query("bud");
        let shown = second.rows.len();
        let before: Vec<_> = (0..shown).map(|i| session.item(second.seq, i).map(|i| i.action)).collect();
        assert!(session.add_files(first.seq, budget_files()).is_none());
        // The current results are as they were, with nothing after them.
        let after: Vec<_> = (0..shown).map(|i| session.item(second.seq, i).map(|i| i.action)).collect();
        assert_eq!(before, after);
        assert!(session.item(second.seq, shown).is_none());
    }

    #[test]
    fn files_for_the_last_opening_are_dropped() {
        let session = Session::default();
        session.open(ctx(NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()));
        let old = session.query("bud");
        session.open(ctx(NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()));
        assert!(session.add_files(old.seq, budget_files()).is_none());
        assert_eq!(session.last_query(), "");
    }

    #[test]
    fn files_join_the_end_of_the_rows() {
        let session = Session::default();
        session.open(ctx(NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()));
        let results = session.query("bud");
        let files = budget_files();
        let first = files[0].clone();
        let views = session.add_files(results.seq, files).expect("current files are added");
        assert_eq!(views.len(), 2);
        assert_eq!(session.item(results.seq, results.rows.len()).map(|i| i.action), Some(first.action));
        // Their icons may be asked for now.
        let Icon::File(path) = &first.icon else { panic!("a file icon") };
        assert!(session.shows_icon(path));
    }

    #[test]
    fn files_never_follow_the_suggestions() {
        let session = Session::default();
        session.open(ctx(NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()));
        let results = session.query("");
        assert!(session.add_files(results.seq, budget_files()).is_none());
    }

    #[test]
    fn files_are_wanted_for_two_letters_with_the_switch_on() {
        let session = Session::default();
        session.open(ctx(NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()));
        assert!(!session.wants_files(" b "));
        assert!(session.wants_files("bu"));
        session.set_switch(settings::Switch::SearchFiles, false);
        assert!(!session.wants_files("budget"));
    }

    #[test]
    fn a_flipped_switch_shows_its_new_state_when_asked_again() {
        let session = Session::default();
        let mut c = ctx(NaiveDate::from_ymd_opt(2026, 10, 26).unwrap());
        c.switches = vec![(settings::Switch::Nudges, false)];
        session.open(c);
        session.query("focus nudges");
        let before = session.query(&session.last_query());
        session.set_switch(settings::Switch::Nudges, true);
        let after = session.query(&session.last_query());
        assert!(after.seq > before.seq);
        assert_ne!(serde_json::to_string(&before.rows).unwrap(), serde_json::to_string(&after.rows).unwrap());
    }
}
