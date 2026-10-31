//! Getting your data out and back in: tracked time as a spreadsheet, and everything as one backup
//! file that restores on any computer, Windows, Mac or Linux, since apps go by the same names on
//! each.

use std::io::Write;
use std::path::Path;

use chrono::{Local, TimeZone};
use rusqlite::{Connection, OpenFlags};

use crate::analysis;

/// What a restore brings back. Where programs live (`app_paths`) stays as it is on this computer.
const RESTORED: &[&str] = &["sessions", "categories", "settings", "privacy_rules", "limits", "focus_sessions", "focus_rules", "goal_alerts", "projects", "focus_schedule", "block_lists", "week_notes", "tasks"];
/// Settings that belong to this computer rather than to you: the palette's keys differ between a
/// Mac and the rest, and each computer combining its time with others has its own id.
const LOCAL_SETTINGS: &[&str] = &["palette_shortcut", "device_id", "sync_folder", "shown_computer", "choices_seeded"];

type Result<T> = std::result::Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Writes tracked time to `path` as CSV, one row per stretch in one app or site, oldest first,
/// in this computer's time zone. Returns how many rows.
pub fn export_csv(conn: &Connection, path: &Path) -> Result<usize> {
    let cats = analysis::category_map(conn).map_err(err)?;
    let mut stmt = conn
        // Moments under a second, as when passing through a window, aren't worth a row.
        .prepare("SELECT start_ms, end_ms, app, exe, site, title FROM sessions WHERE idle = 0 AND demo = 0 AND end_ms - start_ms >= 1000 ORDER BY start_ms")
        .map_err(err)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, Option<String>>(4)?, r.get::<_, String>(5)?))
        })
        .map_err(err)?;
    let mut out = std::io::BufWriter::new(std::fs::File::create(path).map_err(err)?);
    // The byte-order mark tells Excel the file is UTF-8.
    out.write_all("\u{feff}date,start,end,minutes,app,site,category,title\r\n".as_bytes()).map_err(err)?;
    let mut count = 0;
    for row in rows {
        let (start, end, app, exe, site, title) = row.map_err(err)?;
        let (Some(from), Some(to)) = (Local.timestamp_millis_opt(start).single(), Local.timestamp_millis_opt(end).single()) else { continue };
        let category = analysis::resolve(&cats, &exe, site.as_deref()).as_str();
        let fields = [
            from.format("%Y-%m-%d").to_string(),
            from.format("%H:%M:%S").to_string(),
            to.format("%H:%M:%S").to_string(),
            format!("{:.2}", (end - start) as f64 / 60_000.0),
            app,
            site.unwrap_or_default(),
            category.to_string(),
            title,
        ];
        let line: Vec<String> = fields.iter().map(|f| csv_field(f)).collect();
        out.write_all(line.join(",").as_bytes()).map_err(err)?;
        out.write_all(b"\r\n").map_err(err)?;
        count += 1;
    }
    out.flush().map_err(err)?;
    Ok(count)
}

/// A value as a CSV field: quoted when it holds a comma, quote or line break, and kept from
/// reading as a formula when a spreadsheet opens it, since window titles can say anything.
fn csv_field(value: &str) -> String {
    let value = if value.starts_with(['=', '+', '-', '@']) && value.parse::<f64>().is_err() { format!("'{value}") } else { value.to_string() };
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

/// Saves everything to `path`: a consistent copy of Kairo's database, taken while it runs.
pub fn backup(conn: &Connection, path: &Path) -> Result<()> {
    // VACUUM INTO won't write over a file; the save dialog has already asked about replacing it.
    if path.exists() {
        std::fs::remove_file(path).map_err(err)?;
    }
    conn.execute("VACUUM INTO ?1", [path.to_string_lossy()]).map_err(err)?;
    Ok(())
}

/// Replaces what's here with the backup at `from`, after saving what's here to `before`, so a
/// restore can be undone. Older backups lacking newer tables or columns bring back what they have.
pub fn restore(conn: &Connection, from: &Path, before: &Path) -> Result<()> {
    let is_kairo = Connection::open_with_flags(from, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .and_then(|backup| backup.prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'sessions'")?.exists([]))
        .unwrap_or(false);
    if !is_kairo {
        return Err("That file isn't a Kairo backup.".into());
    }
    backup(conn, before)?;
    conn.execute("ATTACH DATABASE ?1 AS restored", [from.to_string_lossy()]).map_err(err)?;
    let result = copy_tables(conn);
    let _ = conn.execute("DETACH DATABASE restored", []);
    result
}

fn copy_tables(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction().map_err(err)?;
    let keep_local = LOCAL_SETTINGS.iter().map(|k| format!("'{k}'")).collect::<Vec<_>>().join(", ");
    for table in RESTORED {
        let theirs = columns(&tx, "restored", table)?;
        if theirs.is_empty() {
            continue;
        }
        let shared: Vec<String> = columns(&tx, "main", table)?.into_iter().filter(|c| theirs.contains(c)).collect();
        let list = shared.join(", ");
        let only_yours = if *table == "settings" { format!(" WHERE key NOT IN ({keep_local})") } else { String::new() };
        tx.execute(&format!("DELETE FROM main.{table}{only_yours}"), []).map_err(err)?;
        tx.execute(&format!("INSERT INTO main.{table} ({list}) SELECT {list} FROM restored.{table}{only_yours}"), []).map_err(err)?;
    }
    tx.commit().map_err(err)
}

/// A table's column names in an attached database; empty when it has no such table.
fn columns(conn: &Connection, schema: &str, table: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("SELECT name FROM {schema}.pragma_table_info('{table}')")).map_err(err)?;
    let names = stmt.query_map([], |r| r.get(0)).map_err(err)?.collect::<rusqlite::Result<_>>().map_err(err)?;
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("kairo-{}-{name}", std::process::id()))
    }

    fn add(conn: &Connection, app: &str, start: i64) {
        let s = db::NewSession { app, exe: app, title: "=SUM(A1), \"quoted\"", site: None, start_ms: start, end_ms: start + 90_000, idle: false, demo: false };
        db::insert_session(conn, &s).unwrap();
    }

    #[test]
    fn a_backup_restores_everything_but_this_computers_own_settings() {
        let (saved, before, live_file) = (temp("saved.kairo"), temp("before.kairo"), temp("live.db"));
        let _ = std::fs::remove_file(&live_file);
        let live = db::open(&live_file).unwrap();
        add(&live, "code", 1_000_000);
        let essay = crate::tasks::add(&live, "Essay", 1).unwrap().unwrap();
        db::set_setting(&live, "palette_shortcut", "Ctrl+Space").unwrap();
        db::set_setting(&live, "palette_suggestions", "1").unwrap();
        backup(&live, &saved).unwrap();

        // Things change after the backup, then it's restored.
        add(&live, "discord", 2_000_000);
        crate::tasks::remove(&live, &essay.id).unwrap();
        crate::tasks::add(&live, "Local", 2).unwrap();
        db::set_setting(&live, "palette_shortcut", "Alt+Space").unwrap();
        db::set_setting(&live, "palette_suggestions", "0").unwrap();
        restore(&live, &saved, &before).unwrap();

        let apps: Vec<String> = live.prepare("SELECT app FROM sessions").unwrap().query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
        assert_eq!(apps, ["code"]);
        let tasks: Vec<String> = crate::tasks::list(&live, 0).unwrap().into_iter().map(|t| t.id).collect();
        assert_eq!(tasks, [essay.id]);
        assert_eq!(db::get_setting(&live, "palette_suggestions").unwrap().as_deref(), Some("1"));
        assert_eq!(db::get_setting(&live, "palette_shortcut").unwrap().as_deref(), Some("Alt+Space"));
        // What was here is kept, so the restore can be undone.
        assert!(before.exists());
        assert!(restore(&live, &live_file.with_extension("missing"), &before).is_err());

        drop(live);
        for f in [saved, before, live_file] {
            let _ = std::fs::remove_file(f);
        }
    }

    #[test]
    fn exported_time_reads_safely_in_a_spreadsheet() {
        let conn = db::open(Path::new(":memory:")).unwrap();
        add(&conn, "code", 1_000_000);
        let path = temp("time.csv");
        assert_eq!(export_csv(&conn, &path).unwrap(), 1);
        let text = std::fs::read_to_string(&path).unwrap();
        let row = text.lines().nth(1).unwrap();
        assert!(row.contains(",1.50,code,,"), "{row}");
        assert!(row.ends_with(r#""'=SUM(A1), ""quoted""""#), "{row}");
        let _ = std::fs::remove_file(path);
    }
}
