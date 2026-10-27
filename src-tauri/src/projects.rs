//! Projects: time by what it was for rather than which app, across apps. Each project has keywords,
//! and a stretch of time belongs to the first project with a keyword in its window title or site,
//! so "kairo" gathers VS Code's kairo folder, the kairo repository in a browser and a terminal
//! titled kairo into one total. Titles hidden by a privacy rule can't match; their sites still can.

use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    #[serde(default)]
    pub id: i64,
    pub name: String,
    /// Lowercase words or phrases, any of which marks time as this project's.
    pub keywords: Vec<String>,
}

/// Keywords as the user typed them, comma-separated, tidied: trimmed, lowercase, no empties.
pub fn keywords(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for k in text.split(',').map(|k| k.trim().to_lowercase()).filter(|k| !k.is_empty()) {
        if !out.contains(&k) {
            out.push(k);
        }
    }
    out
}

pub fn list(conn: &Connection) -> Result<Vec<Project>> {
    let mut stmt = conn.prepare("SELECT id, name, keywords FROM projects ORDER BY position, id")?;
    let projects = stmt
        .query_map([], |r| Ok(Project { id: r.get(0)?, name: r.get(1)?, keywords: keywords(&r.get::<_, String>(2)?) }))?
        .collect();
    projects
}

/// Adds a project, or with an id changes that one. None for one with no name or no keywords.
pub fn save(conn: &Connection, id: Option<i64>, name: &str, keyword_text: &str) -> Result<Option<i64>> {
    let (name, words) = (name.trim(), keywords(keyword_text));
    if name.is_empty() || words.is_empty() {
        return Ok(None);
    }
    let joined = words.join(", ");
    match id {
        Some(id) => {
            conn.execute("UPDATE projects SET name = ?1, keywords = ?2 WHERE id = ?3", params![name, joined, id])?;
            Ok(Some(id))
        }
        None => {
            conn.execute(
                "INSERT INTO projects (name, keywords, position) VALUES (?1, ?2, (SELECT IFNULL(MAX(position), 0) + 1 FROM projects))",
                params![name, joined],
            )?;
            Ok(Some(conn.last_insert_rowid()))
        }
    }
}

pub fn remove(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM projects WHERE id = ?1", [id])?;
    Ok(())
}

/// All projects as text, for sharing with combined computers.
pub fn to_json(conn: &Connection) -> Result<String> {
    let projects: Vec<Project> = list(conn)?.into_iter().map(|p| Project { id: 0, ..p }).collect();
    Ok(serde_json::to_string(&projects).unwrap_or_else(|_| "[]".into()))
}

/// Takes another computer's projects in place of this one's.
pub fn replace_from_json(conn: &Connection, json: &str) -> std::result::Result<(), String> {
    let projects: Vec<Project> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM projects", []).map_err(|e| e.to_string())?;
    for p in projects {
        save(&tx, None, &p.name, &p.keywords.join(",")).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

/// The first project a stretch with this title and site belongs to.
fn project_of<'a>(projects: &'a [Project], title: &str, site: Option<&str>) -> Option<&'a Project> {
    let (title, site) = (title.to_lowercase(), site.map(str::to_lowercase).unwrap_or_default());
    projects.iter().find(|p| p.keywords.iter().any(|k| title.contains(k.as_str()) || site.contains(k.as_str())))
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTime {
    pub id: i64,
    pub name: String,
    pub ms: i64,
}

/// Time in each project over [from, to), most first, leaving out projects with none. `computer`
/// picks whose time, as `analysis::load_rows_of` does.
pub fn totals(conn: &Connection, from: i64, to: i64, computer: &str) -> Result<Vec<ProjectTime>> {
    let projects = list(conn)?;
    if projects.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT title, site, start_ms, end_ms FROM sessions
         WHERE idle = 0 AND end_ms > ?1 AND start_ms < ?2
           AND (?3 = 'all' OR (?3 = 'this' AND device IS NULL) OR device = ?3)",
    )?;
    let rows = stmt.query_map(params![from, to, computer], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?))
    })?;
    let mut ms = vec![0i64; projects.len()];
    for row in rows {
        let (title, site, start, end) = row?;
        if let Some(p) = project_of(&projects, &title, site.as_deref()) {
            let i = projects.iter().position(|q| q.id == p.id).unwrap_or(0);
            ms[i] += end.min(to) - start.max(from);
        }
    }
    let mut out: Vec<ProjectTime> =
        projects.into_iter().zip(ms).filter(|(_, ms)| *ms > 0).map(|(p, ms)| ProjectTime { id: p.id, name: p.name, ms }).collect();
    out.sort_by_key(|p| std::cmp::Reverse(p.ms));
    Ok(out)
}

/// The stretches of time in project `id` over [from, to), clipped to it, as rows the palette's
/// answers chart; from every computer. Each stretch goes to its first matching project, as in
/// [`totals`].
pub fn rows_of(conn: &Connection, id: i64, from: i64, to: i64) -> Result<Vec<crate::analysis::Row>> {
    use crate::analysis::{Category, Row};
    let projects = list(conn)?;
    let mut stmt = conn.prepare("SELECT app, exe, title, site, start_ms, end_ms FROM sessions WHERE idle = 0 AND end_ms > ?1 AND start_ms < ?2 ORDER BY start_ms")?;
    let rows = stmt.query_map(params![from, to], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, i64>(4)?, r.get::<_, i64>(5)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (app, exe, title, site, start, end) = row?;
        if project_of(&projects, &title, site.as_deref()).is_some_and(|p| p.id == id) {
            let category = Category::Neutral;
            out.push(Row { app, exe, site, start: start.max(from), end: end.min(to), idle: false, category, app_category: category });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    const M: i64 = 60_000;

    fn track(conn: &Connection, app: &str, title: &str, site: Option<&str>, start: i64, minutes: i64) {
        let s = db::NewSession { app, exe: app, title, site, start_ms: start, end_ms: start + minutes * M, idle: false, demo: false };
        db::insert_session(conn, &s).unwrap();
    }

    #[test]
    fn time_gathers_by_project_across_apps() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        save(&conn, None, "Kairo", " Kairo, kairo.app ,kairo ").unwrap();
        save(&conn, None, "Thesis", "thesis, overleaf").unwrap();
        assert_eq!(list(&conn).unwrap()[0].keywords, ["kairo", "kairo.app"]);
        assert_eq!(save(&conn, None, "  ", "x").unwrap(), None);

        let t = 1_000 * M;
        track(&conn, "code", "focus.rs - kairo - Visual Studio Code", None, t, 60);
        track(&conn, "brave", "uwuexdeemeow/Kairo: Pull requests", Some("github"), t + 60 * M, 20);
        track(&conn, "brave", "Untitled - Overleaf", Some("overleaf.com"), t + 80 * M, 30);
        track(&conn, "discord", "Friends", None, t + 110 * M, 15);
        // Half of this one falls outside the range asked about.
        track(&conn, "code", "kairo", None, t + 170 * M, 20);

        let times = totals(&conn, t, t + 180 * M, "all").unwrap();
        let shape: Vec<(&str, i64)> = times.iter().map(|p| (p.name.as_str(), p.ms / M)).collect();
        assert_eq!(shape, [("Kairo", 90), ("Thesis", 30)]);

        // The same time, as the palette charts it, stretch by stretch.
        let kairo = times[0].id;
        let rows = rows_of(&conn, kairo, t, t + 180 * M).unwrap();
        let apps: Vec<(&str, i64)> = rows.iter().map(|r| (r.app.as_str(), r.dur() / M)).collect();
        assert_eq!(apps, [("code", 60), ("brave", 20), ("code", 10)]);
    }

    #[test]
    fn projects_move_between_computers_as_text() {
        let (here, there) = (db::open(std::path::Path::new(":memory:")).unwrap(), db::open(std::path::Path::new(":memory:")).unwrap());
        save(&here, None, "Kairo", "kairo").unwrap();
        save(&there, None, "Old", "old").unwrap();
        replace_from_json(&there, &to_json(&here).unwrap()).unwrap();
        let names: Vec<String> = list(&there).unwrap().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["Kairo"]);
    }
}
