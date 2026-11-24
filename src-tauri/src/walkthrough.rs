//! The walkthrough shown the first time Kairo opens. It is due until it has been finished or
//! skipped once, and never for someone who already has recorded time: an update must not show it to
//! a person who has been using Kairo for months.

use rusqlite::Connection;

use crate::db;

const KEY: &str = "walkthrough_done";

/// Whether to show the walkthrough now. Marks it done, and says no, when there's recorded time.
pub fn due(conn: &Connection) -> rusqlite::Result<bool> {
    if db::get_setting(conn, KEY)?.is_some() {
        return Ok(false);
    }
    let any: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sessions)", [], |r| r.get(0))?;
    if any {
        finish(conn)?;
        return Ok(false);
    }
    Ok(true)
}

/// Remembers it was seen, finished or skipped.
pub fn finish(conn: &Connection) -> rusqlite::Result<()> {
    db::set_setting(conn, KEY, "1")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        db::open(std::path::Path::new(":memory:")).unwrap()
    }

    #[test]
    fn a_new_install_is_due_until_it_is_finished() {
        let c = conn();
        assert!(due(&c).unwrap());
        // Asking again doesn't use it up: only finishing or skipping does.
        assert!(due(&c).unwrap());
        finish(&c).unwrap();
        assert!(!due(&c).unwrap());
    }

    #[test]
    fn someone_with_recorded_time_is_never_shown_it() {
        let c = conn();
        c.execute("INSERT INTO sessions (app, exe, title, site, start_ms, end_ms, idle, demo) VALUES ('Code', 'code.exe', 't', NULL, 1000, 2000, 0, 0)", []).unwrap();
        assert!(!due(&c).unwrap());
        assert_eq!(db::get_setting(&c, KEY).unwrap().as_deref(), Some("1"));
    }
}
