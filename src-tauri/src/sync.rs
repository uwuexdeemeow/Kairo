//! Combining computers: each computer writes its own tracked time into a folder the user's cloud
//! drive already syncs (OneDrive, Dropbox, iCloud Drive, Syncthing...), and reads the others' in,
//! so the day views, limits and recaps add up every computer. There's no server; the drive does
//! the moving. Only tracked time is combined: categories, limits and goals stay per computer.
//!
//! In the folder, `Kairo computers/<computer id>/` holds `computer.json` (its name) and one small
//! file per day (`<day>.kairo-day`, days since 1970 in UTC), rewritten only when that day changed,
//! so the drive moves a few kilobytes rather than the whole history.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::{db, privacy};

const FOLDER_KEY: &str = "sync_folder";
const DEVICE_KEY: &str = "device_id";
const DIR: &str = "Kairo computers";
const DAY_FILE: &str = "kairo-day";
const ABOUT_FILE: &str = "computer.json";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const FIRST_AFTER: Duration = Duration::from_secs(30);
const EVERY: Duration = Duration::from_secs(5 * 60);

type Result<T> = std::result::Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Serialize, Deserialize)]
struct About {
    name: String,
    updated_ms: i64,
}

/// A computer whose time is combined, for Settings.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Computer {
    pub name: String,
    /// When it last wrote its time.
    pub updated_ms: i64,
    pub this_one: bool,
}

/// The folder being synced, if combining is on.
pub fn folder(conn: &Connection) -> Result<Option<PathBuf>> {
    Ok(db::get_setting(conn, FOLDER_KEY).map_err(err)?.filter(|f| !f.is_empty()).map(PathBuf::from))
}

/// This computer's id among the others, made the first time it's asked for.
pub fn device_id(conn: &Connection) -> Result<String> {
    if let Some(id) = db::get_setting(conn, DEVICE_KEY).map_err(err)? {
        return Ok(id);
    }
    let id = new_id();
    db::set_setting(conn, DEVICE_KEY, &id).map_err(err)?;
    Ok(id)
}

fn new_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    // RandomState is seeded randomly for each run of Kairo.
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    hasher.write_u32(std::process::id());
    format!("{:016x}", hasher.finish())
}

/// The computer's own name, as the system calls it.
pub fn device_name() -> String {
    #[cfg(windows)]
    let name = std::env::var("COMPUTERNAME").ok();
    #[cfg(not(windows))]
    let name = {
        let mut buf = [0u8; 256];
        (unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0)
            .then(|| std::ffi::CStr::from_bytes_until_nul(&buf).ok().map(|n| n.to_string_lossy().trim_end_matches(".local").to_string()))
            .flatten()
    };
    name.filter(|n| !n.is_empty()).unwrap_or_else(|| "This computer".into())
}

/// Starts combining through `dir`, at once.
pub fn start(conn: &Connection, dir: &Path, now_ms: i64) -> Result<()> {
    db::set_setting(conn, FOLDER_KEY, &dir.to_string_lossy()).map_err(err)?;
    run_once(conn, now_ms)
}

/// Stops combining: the other computers' time leaves this one, and this one's folder leaves the
/// synced folder, so they drop its time too.
pub fn stop(conn: &Connection) -> Result<()> {
    if let Some(root) = folder(conn)? {
        let _ = std::fs::remove_dir_all(root.join(DIR).join(device_id(conn)?));
    }
    db::set_setting(conn, FOLDER_KEY, "").map_err(err)?;
    conn.execute_batch("DELETE FROM sessions WHERE device IS NOT NULL; DELETE FROM sync_imports; DELETE FROM sync_written;").map_err(err)
}

/// The computers in the synced folder, this one first.
pub fn computers(conn: &Connection) -> Result<Vec<Computer>> {
    let Some(root) = folder(conn)? else { return Ok(Vec::new()) };
    let me = device_id(conn)?;
    let mut found: Vec<Computer> = std::fs::read_dir(root.join(DIR))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let about: About = serde_json::from_str(&std::fs::read_to_string(entry.path().join(ABOUT_FILE)).ok()?).ok()?;
            Some(Computer { name: about.name, updated_ms: about.updated_ms, this_one: entry.file_name().to_str() == Some(me.as_str()) })
        })
        .collect();
    found.sort_by_key(|c| (!c.this_one, c.name.to_lowercase()));
    Ok(found)
}

/// Writes this computer's changed days and reads the others' changed ones.
pub fn run_once(conn: &Connection, now_ms: i64) -> Result<()> {
    let Some(root) = folder(conn)? else { return Ok(()) };
    if !root.is_dir() {
        return Err(format!("{} isn't there; is the drive connected?", root.display()));
    }
    let dir = root.join(DIR);
    let me = device_id(conn)?;
    let mine = dir.join(&me);
    std::fs::create_dir_all(&mine).map_err(err)?;
    write_own(conn, &mine, now_ms)?;
    read_others(conn, &dir, &me, now_ms)
}

/// Every few minutes, while Kairo runs.
pub fn spawn(db_path: PathBuf) {
    thread::Builder::new()
        .name("kairo-sync".into())
        .spawn(move || {
            thread::sleep(FIRST_AFTER);
            let conn = match db::open(&db_path) {
                Ok(conn) => conn,
                Err(e) => return eprintln!("sync: cannot open the database: {e}"),
            };
            loop {
                if let Err(e) = run_once(&conn, chrono::Utc::now().timestamp_millis()) {
                    eprintln!("sync: {e}");
                }
                thread::sleep(EVERY);
            }
        })
        .expect("failed to start sync thread");
}

/// Writes the days whose sessions changed since they were last written, and takes away the files
/// of days whose history was deleted.
fn write_own(conn: &Connection, mine: &Path, now_ms: i64) -> Result<()> {
    let mut stmt = conn
        .prepare(
            "SELECT start_ms / ?1, COUNT(*), SUM(start_ms), SUM(end_ms), SUM(length(title) + length(app) + length(ifnull(site, '')))
             FROM sessions WHERE device IS NULL AND demo = 0 GROUP BY 1",
        )
        .map_err(err)?;
    let now: HashMap<i64, String> = stmt
        .query_map([DAY_MS], |r| Ok((r.get::<_, i64>(0)?, format!("{}:{}:{}:{}", r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?))))
        .map_err(err)?
        .collect::<rusqlite::Result<_>>()
        .map_err(err)?;
    let written: HashMap<i64, String> = conn
        .prepare("SELECT day, signature FROM sync_written")
        .map_err(err)?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(err)?
        .collect::<rusqlite::Result<_>>()
        .map_err(err)?;
    for (day, signature) in &now {
        if written.get(day) != Some(signature) || !day_path(mine, *day).exists() {
            write_day(conn, mine, *day)?;
            conn.execute("INSERT OR REPLACE INTO sync_written (day, signature) VALUES (?1, ?2)", params![day, signature]).map_err(err)?;
        }
    }
    for day in written.keys().filter(|d| !now.contains_key(d)) {
        let _ = std::fs::remove_file(day_path(mine, *day));
        conn.execute("DELETE FROM sync_written WHERE day = ?1", [day]).map_err(err)?;
    }
    let about = About { name: device_name(), updated_ms: now_ms };
    write_atomically(&mine.join(ABOUT_FILE), |tmp| std::fs::write(tmp, serde_json::to_vec(&about).map_err(err)?).map_err(err))
}

fn day_path(dir: &Path, day: i64) -> PathBuf {
    dir.join(format!("{day}.{DAY_FILE}"))
}

/// One day's sessions in a small database of its own.
fn write_day(conn: &Connection, mine: &Path, day: i64) -> Result<()> {
    write_atomically(&day_path(mine, day), |tmp| {
        conn.execute("ATTACH DATABASE ?1 AS day_out", [tmp.to_string_lossy()]).map_err(err)?;
        let result = conn
            .execute_batch(
                "CREATE TABLE day_out.sessions (
                     app TEXT NOT NULL, exe TEXT NOT NULL, title TEXT NOT NULL, site TEXT,
                     start_ms INTEGER NOT NULL, end_ms INTEGER NOT NULL, idle INTEGER NOT NULL
                 )",
            )
            .and_then(|_| {
                conn.execute(
                    "INSERT INTO day_out.sessions SELECT app, exe, title, site, start_ms, end_ms, idle FROM main.sessions
                     WHERE device IS NULL AND demo = 0 AND start_ms >= ?1 AND start_ms < ?2",
                    params![day * DAY_MS, (day + 1) * DAY_MS],
                )
            })
            .map_err(err);
        let _ = conn.execute("DETACH DATABASE day_out", []);
        result.map(|_| ())
    })
}

/// Writes next to `path` and then moves it into place, so a drive syncing the folder never
/// takes a half-written file.
fn write_atomically(path: &Path, write: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    let tmp = path.with_extension("tmp");
    let _ = std::fs::remove_file(&tmp);
    let written = write(&tmp);
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return written;
    }
    std::fs::rename(&tmp, path).map_err(err)
}

/// Reads in the other computers' days that changed since last time, and forgets days, and
/// computers, that went away.
fn read_others(conn: &Connection, dir: &Path, me: &str, now_ms: i64) -> Result<()> {
    let rules = privacy::load(conn).map_err(err)?;
    // Days older than history is kept for aren't brought back.
    let oldest_day = match privacy::retention_days(conn).map_err(err)? {
        0 => i64::MIN,
        days => (now_ms - days as i64 * DAY_MS) / DAY_MS,
    };
    let mut present = HashSet::new();
    for entry in std::fs::read_dir(dir).map_err(err)?.flatten() {
        let Some(device) = entry.file_name().to_str().map(str::to_string).filter(|d| d != me && entry.path().is_dir()) else { continue };
        present.insert(device.clone());
        let files: HashMap<i64, (PathBuf, String)> = std::fs::read_dir(entry.path())
            .map_err(err)?
            .flatten()
            .filter_map(|f| {
                let path = f.path();
                let day = path.file_name()?.to_str()?.strip_suffix(&format!(".{DAY_FILE}"))?.parse().ok()?;
                let meta = f.metadata().ok()?;
                let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis();
                Some((day, (path, format!("{}:{modified}", meta.len()))))
            })
            .collect();
        let known = imported(conn, &device)?;
        for (day, (path, stamp)) in &files {
            if *day < oldest_day || known.get(day) == Some(stamp) {
                continue;
            }
            // A file the drive is still bringing in can fail to open; it's tried again next time.
            if import_day(conn, &device, *day, path, &rules).is_ok() {
                conn.execute("INSERT OR REPLACE INTO sync_imports (device, day, stamp) VALUES (?1, ?2, ?3)", params![device, day, stamp]).map_err(err)?;
            }
        }
        for day in known.keys().filter(|d| !files.contains_key(d)) {
            forget(conn, &device, Some(*day))?;
        }
    }
    let gone: Vec<String> = conn
        .prepare("SELECT DISTINCT device FROM sync_imports")
        .map_err(err)?
        .query_map([], |r| r.get(0))
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<String>>>()
        .map_err(err)?
        .into_iter()
        .filter(|d| !present.contains(d))
        .collect();
    for device in gone {
        forget(conn, &device, None)?;
    }
    Ok(())
}

fn imported(conn: &Connection, device: &str) -> Result<HashMap<i64, String>> {
    let mut stmt = conn.prepare("SELECT day, stamp FROM sync_imports WHERE device = ?1").map_err(err)?;
    let rows = stmt.query_map([device], |r| Ok((r.get(0)?, r.get(1)?))).map_err(err)?.collect::<rusqlite::Result<_>>().map_err(err);
    rows
}

/// Takes away a computer's time for one day, or all of it.
fn forget(conn: &Connection, device: &str, day: Option<i64>) -> Result<()> {
    let (from, to) = day.map_or((i64::MIN, i64::MAX), |d| (d * DAY_MS, (d + 1) * DAY_MS));
    conn.execute("DELETE FROM sessions WHERE device = ?1 AND start_ms >= ?2 AND start_ms < ?3", params![device, from, to]).map_err(err)?;
    match day {
        Some(day) => conn.execute("DELETE FROM sync_imports WHERE device = ?1 AND day = ?2", params![device, day]),
        None => conn.execute("DELETE FROM sync_imports WHERE device = ?1", [device]),
    }
    .map_err(err)?;
    Ok(())
}

/// Replaces what's here of a computer's day with its file, keeping to this computer's privacy
/// rules. The file is copied first, so the drive is never kept from updating it.
fn import_day(conn: &Connection, device: &str, day: i64, path: &Path, rules: &privacy::Rules) -> Result<()> {
    let copy = std::env::temp_dir().join(format!("kairo-import-{}-{device}-{day}.db", std::process::id()));
    std::fs::copy(path, &copy).map_err(err)?;
    let rows = read_day(&copy);
    let _ = std::fs::remove_file(&copy);
    let rows = rows?;
    let tx = conn.unchecked_transaction().map_err(err)?;
    tx.execute("DELETE FROM sessions WHERE device = ?1 AND start_ms >= ?2 AND start_ms < ?3", params![device, day * DAY_MS, (day + 1) * DAY_MS])
        .map_err(err)?;
    for (app, exe, title, site, start, end, idle) in rows {
        let title = match privacy::resolve(rules, &exe, site.as_deref()) {
            privacy::Mode::Ignore => continue,
            privacy::Mode::HideTitle => String::new(),
            privacy::Mode::Track => title,
        };
        tx.execute(
            "INSERT INTO sessions (app, exe, title, site, start_ms, end_ms, idle, demo, device) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
            params![app, exe, title, site, start, end, idle, device],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)
}

type DayRow = (String, String, String, Option<String>, i64, i64, bool);

fn read_day(path: &Path) -> Result<Vec<DayRow>> {
    let day = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(err)?;
    let mut stmt = day.prepare("SELECT app, exe, title, site, start_ms, end_ms, idle FROM sessions").map_err(err)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))
        .map_err(err)?
        .collect::<rusqlite::Result<_>>()
        .map_err(err);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Computer {
        conn: Connection,
        file: PathBuf,
    }

    impl Computer {
        fn new(name: &str) -> Self {
            let file = std::env::temp_dir().join(format!("kairo-sync-{}-{name}.db", std::process::id()));
            let _ = std::fs::remove_file(&file);
            Self { conn: db::open(&file).unwrap(), file }
        }

        fn track(&self, app: &str, start: i64) -> i64 {
            let s = db::NewSession { app, exe: app, title: "", site: None, start_ms: start, end_ms: start + 60_000, idle: false, demo: false };
            db::insert_session(&self.conn, &s).unwrap()
        }

        fn apps(&self) -> Vec<(String, bool)> {
            self.conn
                .prepare("SELECT app, device IS NOT NULL FROM sessions ORDER BY start_ms")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        }
    }

    impl Drop for Computer {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.file);
        }
    }

    #[test]
    fn computers_add_up_through_a_shared_folder() {
        let folder = std::env::temp_dir().join(format!("kairo-sync-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let (desk, laptop) = (Computer::new("desk"), Computer::new("laptop"));
        let now = 20_000 * DAY_MS;

        let code = desk.track("code", now - DAY_MS);
        desk.track("discord", now);
        laptop.track("figma", now + 1_000);
        start(&desk.conn, &folder, now).unwrap();
        start(&laptop.conn, &folder, now).unwrap();
        start(&desk.conn, &folder, now).unwrap();

        assert_eq!(laptop.apps(), [("code".into(), true), ("discord".into(), true), ("figma".into(), false)]);
        assert_eq!(desk.apps(), [("code".into(), false), ("discord".into(), false), ("figma".into(), true)]);
        assert_eq!(computers(&laptop.conn).unwrap().len(), 2);
        assert!(computers(&laptop.conn).unwrap()[0].this_one);

        // History deleted on one computer leaves the others too, and only changed days are written.
        desk.conn.execute("DELETE FROM sessions WHERE id = ?1", [code]).unwrap();
        run_once(&desk.conn, now).unwrap();
        run_once(&laptop.conn, now).unwrap();
        assert_eq!(laptop.apps(), [("discord".into(), true), ("figma".into(), false)]);

        // A computer that stops combining takes its time away from the rest.
        stop(&desk.conn).unwrap();
        run_once(&laptop.conn, now).unwrap();
        assert_eq!(laptop.apps(), [("figma".into(), false)]);
        assert_eq!(desk.apps(), [("discord".into(), false)]);

        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn other_computers_time_keeps_to_this_ones_privacy_rules() {
        let folder = std::env::temp_dir().join(format!("kairo-sync-privacy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let (desk, laptop) = (Computer::new("desk-p"), Computer::new("laptop-p"));
        let now = 20_000 * DAY_MS;
        desk.track("banking", now);
        desk.track("code", now + 1_000);
        privacy::set_rule(&laptop.conn, "app:banking", privacy::Mode::Ignore).unwrap();
        start(&desk.conn, &folder, now).unwrap();
        start(&laptop.conn, &folder, now).unwrap();
        assert_eq!(laptop.apps(), [("code".into(), true)]);
        let _ = std::fs::remove_dir_all(folder);
    }
}
