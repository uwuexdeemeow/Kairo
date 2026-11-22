//! Your own palette commands: a name you choose that opens a link, opens a file, folder or app, or
//! copies a piece of text. Typing the name (or its start) shows a row, and Enter does it, with the
//! palette's own actions for each. Links and text go to the computers you've combined; files,
//! folders and apps stay on the one they were made on, since a path means something only there.

use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};

use crate::palette::search::Action;

/// Commands at most, so the palette's list stays one you can hold in your head.
pub const MAX_COMMANDS: usize = 50;
const MAX_NAME_CHARS: usize = 30;
const MAX_TARGET_CHARS: usize = 2000;

/// What a command does when run.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// Opens a web address in the browser.
    Link,
    /// Opens a file, a folder or an app, by its path.
    Open,
    /// Puts text on the clipboard.
    Text,
}

impl Kind {
    fn key(self) -> &'static str {
        match self {
            Kind::Link => "link",
            Kind::Open => "open",
            Kind::Text => "text",
        }
    }

    fn from_key(key: &str) -> Option<Kind> {
        [Kind::Link, Kind::Open, Kind::Text].into_iter().find(|k| k.key() == key)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CustomCommand {
    pub name: String,
    pub kind: Kind,
    pub target: String,
}

impl CustomCommand {
    /// What running it does, as the palette's action.
    pub fn action(&self) -> Action {
        match self.kind {
            Kind::Link => Action::OpenUrl(self.target.clone()),
            Kind::Open => Action::OpenFile(self.target.clone()),
            Kind::Text => Action::Copy(self.target.clone()),
        }
    }

    /// What it does, in a line under its name: where a link goes, what a path opens, the start of a
    /// text it copies.
    pub fn says(&self) -> String {
        match self.kind {
            Kind::Link => format!("Opens {}", self.target.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/')),
            Kind::Open => format!("Opens {}", self.target),
            Kind::Text => {
                let first = self.target.lines().next().unwrap_or("");
                let cut: String = first.chars().take(60).collect();
                let more = if cut.chars().count() < first.chars().count() || self.target.lines().nth(1).is_some() { "…" } else { "" };
                format!("Copies “{cut}{more}”")
            }
        }
    }
}

pub fn list(conn: &Connection) -> Result<Vec<CustomCommand>> {
    let mut stmt = conn.prepare("SELECT name, kind, target FROM custom_commands ORDER BY position, rowid")?;
    let rows = stmt
        .query_map([], |r| {
            let kind: String = r.get(1)?;
            Ok((r.get::<_, String>(0)?, kind, r.get::<_, String>(2)?))
        })?
        .collect::<Result<Vec<_>>>()?;
    Ok(rows.into_iter().filter_map(|(name, kind, target)| Some(CustomCommand { name, kind: Kind::from_key(&kind)?, target })).collect())
}

/// A link as it will be opened: a web address, with https:// put in front when only a site was
/// typed. Anything that isn't a web address or an email link is turned down, so a command can't
/// start some other program through a link.
fn link(target: &str) -> std::result::Result<String, String> {
    let t = target.trim();
    if t.is_empty() {
        return Err("Say where the link goes.".into());
    }
    let lower = t.to_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:") {
        return Ok(t.to_string());
    }
    // A scheme of some other kind (file:, ms-settings:, javascript:) isn't a web address.
    if let Some((scheme, _)) = t.split_once(':') {
        // One letter is a drive ("C:\\"), not a scheme.
        if scheme.len() > 1 && scheme.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err("A link starts with http:// or https://. Choose “Opens a file, folder or app” for a path.".into());
        }
    }
    if t.contains(' ') || !t.contains('.') {
        return Err("That doesn't look like a web address.".into());
    }
    Ok(format!("https://{t}"))
}

/// Adds a command, or with `old`, changes the one of that name. The inner error is for the user.
pub fn save(conn: &Connection, old: Option<&str>, name: &str, kind: Kind, target: &str) -> Result<std::result::Result<(), String>> {
    let name: String = name.trim().chars().take(MAX_NAME_CHARS).collect();
    let name = name.trim_end().to_string();
    if name.is_empty() {
        return Ok(Err("Give the command a name.".into()));
    }
    let target = match kind {
        Kind::Link => match link(target) {
            Ok(t) => t,
            Err(e) => return Ok(Err(e)),
        },
        Kind::Open => target.trim().trim_matches('"').to_string(),
        // Text keeps its line breaks and spaces: a signature is made of them.
        Kind::Text => target.trim_matches(['\r', '\n']).to_string(),
    };
    if target.trim().is_empty() {
        return Ok(Err(match kind {
            Kind::Link => "Say where the link goes.",
            Kind::Open => "Say which file, folder or app it opens.",
            Kind::Text => "Write the text it copies.",
        }
        .into()));
    }
    if target.chars().count() > MAX_TARGET_CHARS {
        return Ok(Err(format!("That's longer than {MAX_TARGET_CHARS} characters.")));
    }
    let all = list(conn)?;
    let editing = old.map(str::trim).and_then(|o| all.iter().find(|c| c.name.eq_ignore_ascii_case(o)).cloned());
    if all.iter().any(|c| c.name.eq_ignore_ascii_case(&name) && editing.as_ref().is_none_or(|e| !e.name.eq_ignore_ascii_case(&c.name))) {
        return Ok(Err(format!("There's already a command called {name}.")));
    }
    if editing.is_none() && all.len() >= MAX_COMMANDS {
        return Ok(Err(format!("That's {MAX_COMMANDS} commands, the most there can be.")));
    }
    match editing {
        Some(was) => {
            conn.execute("UPDATE custom_commands SET name = ?1, kind = ?2, target = ?3 WHERE name = ?4", params![name, kind.key(), target, was.name])?;
        }
        None => {
            conn.execute(
                "INSERT INTO custom_commands (name, kind, target, position) VALUES (?1, ?2, ?3, (SELECT IFNULL(MAX(position), 0) + 1 FROM custom_commands))",
                params![name, kind.key(), target],
            )?;
        }
    }
    Ok(Ok(()))
}

/// The commands that mean the same on any computer, as text for sharing: links and text, never
/// paths.
pub fn to_json(conn: &Connection) -> Result<String> {
    let shared: Vec<CustomCommand> = list(conn)?.into_iter().filter(|c| c.kind != Kind::Open).collect();
    Ok(serde_json::to_string(&shared).unwrap_or_else(|_| "[]".into()))
}

/// Takes another computer's links and texts in place of this one's. The paths made here stay, and
/// win a name they share; what doesn't pass the rules for a command is left out.
pub fn replace_from_json(conn: &Connection, json: &str) -> std::result::Result<(), String> {
    let theirs: Vec<CustomCommand> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM custom_commands WHERE kind != 'open'", []).map_err(|e| e.to_string())?;
    for c in theirs.into_iter().filter(|c| c.kind != Kind::Open) {
        // A name taken by a path here, or one past the most, is skipped rather than failing the rest.
        let _ = save(&tx, None, &c.name, c.kind, &c.target).map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Asks for a file or app, or with `folder` a folder, in the system's own dialog; None when it is
/// cancelled. The path is typed into the command for you, so it can't be mistyped.
#[tauri::command]
pub fn pick_command_path(app: tauri::AppHandle, folder: bool) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let dialog = app.dialog().file().set_title(if folder { "Choose a folder" } else { "Choose a file or app" });
    let chosen = if folder { dialog.blocking_pick_folder() } else { dialog.blocking_pick_file() };
    chosen?.into_path().ok().map(|p| p.to_string_lossy().into_owned())
}

pub fn remove(conn: &Connection, name: &str) -> Result<()> {
    conn.execute("DELETE FROM custom_commands WHERE name = ?1 COLLATE NOCASE", [name])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn conn() -> Connection {
        db::open(std::path::Path::new(":memory:")).unwrap()
    }

    /// The command called `name`, whatever the case.
    fn find(conn: &Connection, name: &str) -> Result<Option<CustomCommand>> {
        Ok(list(conn)?.into_iter().find(|c| c.name.eq_ignore_ascii_case(name.trim())))
    }

    fn saved(conn: &Connection, name: &str, kind: Kind, target: &str) -> std::result::Result<(), String> {
        save(conn, None, name, kind, target).unwrap()
    }

    #[test]
    fn a_link_gets_https_when_only_a_site_is_typed() {
        let c = conn();
        saved(&c, "gh", Kind::Link, "github.com/tauri-apps/tauri/issues").unwrap();
        let cmd = find(&c, "GH").unwrap().unwrap();
        assert_eq!(cmd.target, "https://github.com/tauri-apps/tauri/issues");
        assert_eq!(cmd.action(), Action::OpenUrl("https://github.com/tauri-apps/tauri/issues".into()));
        assert_eq!(cmd.says(), "Opens github.com/tauri-apps/tauri/issues");
    }

    #[test]
    fn a_link_is_a_web_address_and_nothing_that_starts_a_program() {
        let c = conn();
        assert!(saved(&c, "a", Kind::Link, "http://example.com").is_ok());
        assert!(saved(&c, "b", Kind::Link, "mailto:me@example.com").is_ok());
        for bad in ["file:///C:/Windows/System32/calc.exe", "javascript:alert(1)", "ms-settings:privacy", "not a link", "nodots", ""] {
            assert!(saved(&c, "bad", Kind::Link, bad).is_err(), "{bad} should be turned down");
        }
        assert!(find(&c, "bad").unwrap().is_none());
    }

    #[test]
    fn a_path_keeps_its_drive_and_loses_stray_quotes() {
        let c = conn();
        saved(&c, "tax", Kind::Open, "\"C:\\Users\\me\\Documents\\Tax 2026\"").unwrap();
        let cmd = find(&c, "tax").unwrap().unwrap();
        assert_eq!(cmd.target, "C:\\Users\\me\\Documents\\Tax 2026");
        assert_eq!(cmd.action(), Action::OpenFile("C:\\Users\\me\\Documents\\Tax 2026".into()));
    }

    #[test]
    fn text_keeps_its_line_breaks_and_says_where_it_goes_on() {
        let c = conn();
        saved(&c, "sig", Kind::Text, "\nBest,\nAmos\n").unwrap();
        let cmd = find(&c, "sig").unwrap().unwrap();
        assert_eq!(cmd.target, "Best,\nAmos");
        assert_eq!(cmd.action(), Action::Copy("Best,\nAmos".into()));
        assert_eq!(cmd.says(), "Copies “Best,…”");
    }

    #[test]
    fn a_name_is_needed_and_taken_names_are_said_whatever_the_case() {
        let c = conn();
        assert_eq!(saved(&c, "  ", Kind::Text, "x"), Err("Give the command a name.".into()));
        saved(&c, "Tax", Kind::Open, "C:\\tax").unwrap();
        assert_eq!(saved(&c, "tax", Kind::Text, "x"), Err("There's already a command called tax.".into()));
        assert_eq!(saved(&c, "x", Kind::Text, ""), Err("Write the text it copies.".into()));
        assert_eq!(saved(&c, "y", Kind::Open, "  "), Err("Say which file, folder or app it opens.".into()));
    }

    #[test]
    fn editing_changes_a_command_in_place_and_may_keep_its_own_name() {
        let c = conn();
        saved(&c, "tax", Kind::Open, "C:\\tax").unwrap();
        saved(&c, "web", Kind::Link, "example.com").unwrap();
        save(&c, Some("tax"), "Tax", Kind::Open, "D:\\tax").unwrap().unwrap();
        let all = list(&c).unwrap();
        assert_eq!(all.iter().map(|c| (c.name.as_str(), c.target.as_str())).collect::<Vec<_>>(), vec![("Tax", "D:\\tax"), ("web", "https://example.com")]);
        // Not onto another command's name.
        assert!(save(&c, Some("tax"), "web", Kind::Open, "D:\\tax").unwrap().is_err());
    }

    #[test]
    fn there_is_a_most_and_removing_makes_room() {
        let c = conn();
        for i in 0..MAX_COMMANDS {
            saved(&c, &format!("c{i}"), Kind::Text, "x").unwrap();
        }
        assert!(saved(&c, "one more", Kind::Text, "x").is_err());
        remove(&c, "C3").unwrap();
        saved(&c, "one more", Kind::Text, "x").unwrap();
        assert_eq!(list(&c).unwrap().len(), MAX_COMMANDS);
    }

    #[test]
    fn only_links_and_text_are_shared_never_paths() {
        let c = conn();
        saved(&c, "tax", Kind::Open, "C:\\tax").unwrap();
        saved(&c, "gh", Kind::Link, "github.com").unwrap();
        saved(&c, "sig", Kind::Text, "Best,\nAmos").unwrap();
        let shared: Vec<CustomCommand> = serde_json::from_str(&to_json(&c).unwrap()).unwrap();
        assert_eq!(shared.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["gh", "sig"]);
        assert!(!to_json(&c).unwrap().contains("tax"));
    }

    #[test]
    fn another_computers_commands_replace_the_shared_ones_and_keep_the_paths_made_here() {
        let c = conn();
        saved(&c, "tax", Kind::Open, "C:\\tax").unwrap();
        saved(&c, "old", Kind::Link, "old.example.com").unwrap();
        let theirs = r#"[{"name":"gh","kind":"link","target":"https://github.com"},{"name":"sig","kind":"text","target":"Best"},{"name":"TAX","kind":"text","target":"clash"},{"name":"nope","kind":"open","target":"D:\\x"}]"#;
        replace_from_json(&c, theirs).unwrap();
        let all = list(&c).unwrap();
        // The old link is gone, the path made here stays and keeps its name, and a path from
        // elsewhere is never taken.
        assert_eq!(all.iter().map(|c| (c.name.as_str(), c.kind)).collect::<Vec<_>>(), vec![("tax", Kind::Open), ("gh", Kind::Link), ("sig", Kind::Text)]);
        assert_eq!(find(&c, "tax").unwrap().unwrap().target, "C:\\tax");
    }

    #[test]
    fn what_arrives_is_held_to_the_same_rules_and_a_bad_file_changes_nothing() {
        let c = conn();
        saved(&c, "gh", Kind::Link, "github.com").unwrap();
        let hostile = r#"[{"name":"x","kind":"link","target":"javascript:alert(1)"},{"name":"ok","kind":"link","target":"example.com"}]"#;
        replace_from_json(&c, hostile).unwrap();
        assert_eq!(list(&c).unwrap().iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["ok"]);
        assert!(replace_from_json(&c, "not json").is_err());
        assert_eq!(list(&c).unwrap().len(), 1);
    }

    #[test]
    fn a_name_is_cut_to_what_fits() {
        let c = conn();
        saved(&c, &"n".repeat(80), Kind::Text, "x").unwrap();
        assert_eq!(list(&c).unwrap()[0].name.chars().count(), MAX_NAME_CHARS);
    }
}
