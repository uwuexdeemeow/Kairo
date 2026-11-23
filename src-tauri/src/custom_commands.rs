//! Your own palette commands: a name you choose that opens a link, opens a file, folder or app,
//! pastes a piece of text (a snippet), or runs another palette command (an alias). Typing the name
//! (or its start) shows a row, and Enter does it, with the palette's own actions for each.
//!
//! A link may hold `{query}`, filled with what's typed after the name: `jira 123` opens ticket
//! 123. Text may hold `{date}`, `{time}` and `{clipboard}`, filled in as it is pasted. Links, text
//! and aliases go to the computers you've combined; files, folders and apps stay on the one they
//! were made on, since a path means something only there.
//!
//! A text command may have a keyword (`;sig`): typing it in any app swaps it for the text
//! (`keywords.rs`). Any command may have keys of its own, a global shortcut that runs it from
//! anywhere (`command_keys.rs`). Keywords go to combined computers with their text; keys don't,
//! since the same keys may already mean something else on another computer.

use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};

use crate::palette::search::Action;

/// Commands at most, so the palette's list stays one you can hold in your head.
pub const MAX_COMMANDS: usize = 50;
const MAX_NAME_CHARS: usize = 30;
const MAX_TARGET_CHARS: usize = 2000;
/// The longest keyword: a few characters is the point.
pub const MAX_KEYWORD_CHARS: usize = 20;

/// What a command does when run.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// Opens a web address in the browser.
    Link,
    /// Opens a file, a folder or an app, by its path.
    Open,
    /// Pastes text into the window that was in front, its placeholders filled in.
    Text,
    /// Runs what the palette offers first for other words: `vs` for "visual studio code".
    Alias,
}

impl Kind {
    fn key(self) -> &'static str {
        match self {
            Kind::Link => "link",
            Kind::Open => "open",
            Kind::Text => "text",
            Kind::Alias => "alias",
        }
    }

    fn from_key(key: &str) -> Option<Kind> {
        [Kind::Link, Kind::Open, Kind::Text, Kind::Alias].into_iter().find(|k| k.key() == key)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CustomCommand {
    pub name: String,
    pub kind: Kind,
    pub target: String,
    /// Typed anywhere, it's swapped for the text; text commands only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyword: Option<String>,
    /// Its global shortcut, as the shortcut plugin spells it ("Ctrl+Alt+S"). Kept on this computer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<String>,
}

/// The slot in a link that what's typed after its name fills.
pub const QUERY: &str = "{query}";

impl CustomCommand {
    #[cfg(test)]
    pub fn new(name: &str, kind: Kind, target: &str) -> CustomCommand {
        CustomCommand { name: name.into(), kind, target: target.into(), keyword: None, keys: None }
    }

    /// What running it does, as the palette's action, for the window that was in front (which
    /// text is pasted into). An alias's action is the palette's to find.
    pub fn action(&self, window: Option<String>) -> Action {
        match self.kind {
            Kind::Link => Action::OpenUrl(fill_link(&self.target, "")),
            Kind::Open => Action::OpenFile(self.target.clone()),
            Kind::Text => Action::Snippet(self.target.clone(), window, true),
            Kind::Alias => Action::Nothing,
        }
    }

    /// Whether the link has a slot for what's typed after the name.
    pub fn takes_query(&self) -> bool {
        self.kind == Kind::Link && self.target.contains(QUERY)
    }

    /// What it does, in a line under its name: where a link goes, what a path opens, the start of a
    /// text it pastes, the words an alias runs.
    pub fn says(&self) -> String {
        match self.kind {
            Kind::Link if self.takes_query() => format!("Opens {} with what you type after “{}”", site(&self.target.replace(QUERY, "…")), self.name),
            Kind::Link => format!("Opens {}", site(&self.target)),
            Kind::Open => format!("Opens {}", self.target),
            Kind::Text => format!("Pastes “{}”", first_line(&self.target)),
            Kind::Alias => format!("Runs “{}”", self.target),
        }
    }
}

/// A web address without its scheme or a closing slash, as a row says it.
pub fn site(url: &str) -> String {
    url.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_string()
}

fn first_line(text: &str) -> String {
    let first = text.lines().next().unwrap_or("");
    let cut: String = first.chars().take(60).collect();
    let more = if cut.chars().count() < first.chars().count() || text.lines().nth(1).is_some() { "…" } else { "" };
    format!("{cut}{more}")
}

/// A link with what was typed put in its slot, made safe for an address: `jira 1 2` gives
/// `…browse/1%202`. A link without a slot is as it was.
pub fn fill_link(target: &str, typed: &str) -> String {
    let mut encoded = String::new();
    for b in typed.trim().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => encoded.push(b as char),
            _ => encoded.push_str(&format!("%{b:02X}")),
        }
    }
    target.replace(QUERY, &encoded)
}

/// Text with its placeholders filled: `{date}` as 2026-11-22, `{time}` as 14:05, `{clipboard}` as
/// what is on the clipboard (nothing when there's no text there).
pub fn expand(text: &str, now: chrono::NaiveDateTime, clipboard: Option<&str>) -> String {
    text.replace("{date}", &now.format("%Y-%m-%d").to_string()).replace("{time}", &now.format("%H:%M").to_string()).replace("{clipboard}", clipboard.unwrap_or(""))
}

pub fn list(conn: &Connection) -> Result<Vec<CustomCommand>> {
    let mut stmt = conn.prepare("SELECT name, kind, target, keyword, keys FROM custom_commands ORDER BY position, rowid")?;
    let rows = stmt
        .query_map([], |r| {
            let kind: String = r.get(1)?;
            Ok((r.get::<_, String>(0)?, kind, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<String>>(4)?))
        })?
        .collect::<Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(name, kind, target, keyword, keys)| {
            let kind = Kind::from_key(&kind)?;
            // A keyword belongs to text only; one left from before a command became a link is idle.
            let keyword = keyword.filter(|k| !k.is_empty() && kind == Kind::Text);
            Some(CustomCommand { name, kind, target, keyword, keys: keys.filter(|k| !k.is_empty()) })
        })
        .collect())
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
        Kind::Alias => target.trim().to_string(),
    };
    if kind == Kind::Alias && target.eq_ignore_ascii_case(&name) {
        return Ok(Err("An alias can't run itself. Write the words it stands for, like “visual studio code”.".into()));
    }
    if target.trim().is_empty() {
        return Ok(Err(match kind {
            Kind::Link => "Say where the link goes.",
            Kind::Open => "Say which file, folder or app it opens.",
            Kind::Text => "Write the text it pastes.",
            Kind::Alias => "Write what it stands for, like “visual studio code” or “focus 50 writing”.",
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
    let shared: Vec<CustomCommand> = list(conn)?.into_iter().filter(|c| c.kind != Kind::Open).map(|c| CustomCommand { keys: None, ..c }).collect();
    Ok(serde_json::to_string(&shared).unwrap_or_else(|_| "[]".into()))
}

/// Takes another computer's links and texts in place of this one's. The paths made here stay, and
/// win a name they share; what doesn't pass the rules for a command is left out.
pub fn replace_from_json(conn: &Connection, json: &str) -> std::result::Result<(), String> {
    let theirs: Vec<CustomCommand> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    // Keys are this computer's: a command that comes back under the same name keeps them.
    let keys: Vec<(String, String)> = list(conn).map_err(|e| e.to_string())?.into_iter().filter_map(|c| Some((c.name, c.keys?))).collect();
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM custom_commands WHERE kind != 'open'", []).map_err(|e| e.to_string())?;
    for c in theirs.into_iter().filter(|c| c.kind != Kind::Open) {
        // A name taken by a path here, or one past the most, is skipped rather than failing the rest.
        if save(&tx, None, &c.name, c.kind, &c.target).map_err(|e| e.to_string())?.is_err() {
            continue;
        }
        if let Some(keyword) = &c.keyword {
            let _ = set_keyword(&tx, &c.name, keyword).map_err(|e| e.to_string())?;
        }
        if let Some((_, k)) = keys.iter().find(|(n, _)| n.eq_ignore_ascii_case(&c.name)) {
            set_keys_saved(&tx, &c.name, k).map_err(|e| e.to_string())?;
        }
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

/// Why `keyword` can't be one: None when it can. `others` are the other commands' keywords.
pub fn keyword_refusal(keyword: &str, others: &[&str]) -> Option<String> {
    if keyword.chars().any(char::is_whitespace) {
        return Some("A keyword is one word, without spaces, like ;sig.".into());
    }
    if keyword.chars().count() > MAX_KEYWORD_CHARS {
        return Some(format!("A keyword is at most {MAX_KEYWORD_CHARS} characters."));
    }
    if keyword.chars().count() < 2 {
        return Some("A keyword needs two characters or more, like ;s, so it isn't set off by typing.".into());
    }
    if let Some(other) = others.iter().find(|o| o.eq_ignore_ascii_case(keyword)) {
        return Some(format!("Another command already uses {other}."));
    }
    None
}

/// Gives the text command `name` a keyword, or with "" takes it away. The inner error is for the user.
pub fn set_keyword(conn: &Connection, name: &str, keyword: &str) -> Result<std::result::Result<(), String>> {
    let keyword = keyword.trim();
    let all = list(conn)?;
    let Some(cmd) = all.iter().find(|c| c.name.eq_ignore_ascii_case(name.trim())) else {
        return Ok(Err("That command is gone.".into()));
    };
    if !keyword.is_empty() {
        if cmd.kind != Kind::Text {
            return Ok(Err("Only a command that pastes text has a keyword.".into()));
        }
        let others: Vec<&str> = all.iter().filter(|c| c.name != cmd.name).filter_map(|c| c.keyword.as_deref()).collect();
        if let Some(why) = keyword_refusal(keyword, &others) {
            return Ok(Err(why));
        }
    }
    let value = (!keyword.is_empty()).then_some(keyword);
    conn.execute("UPDATE custom_commands SET keyword = ?1 WHERE name = ?2", params![value, cmd.name])?;
    Ok(Ok(()))
}

/// Writes down `name`'s keys ("" for none), once the system has given them (`command_keys.rs`).
pub fn set_keys_saved(conn: &Connection, name: &str, keys: &str) -> Result<()> {
    let value = (!keys.is_empty()).then_some(keys);
    conn.execute("UPDATE custom_commands SET keys = ?1 WHERE name = ?2 COLLATE NOCASE", params![value, name.trim()])?;
    Ok(())
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
        assert_eq!(cmd.action(None), Action::OpenUrl("https://github.com/tauri-apps/tauri/issues".into()));
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
        assert_eq!(cmd.action(None), Action::OpenFile("C:\\Users\\me\\Documents\\Tax 2026".into()));
    }

    #[test]
    fn text_keeps_its_line_breaks_and_says_where_it_goes_on() {
        let c = conn();
        saved(&c, "sig", Kind::Text, "\nBest,\nAmos\n").unwrap();
        let cmd = find(&c, "sig").unwrap().unwrap();
        assert_eq!(cmd.target, "Best,\nAmos");
        assert_eq!(cmd.action(Some("9".into())), Action::Snippet("Best,\nAmos".into(), Some("9".into()), true));
        assert_eq!(cmd.says(), "Pastes “Best,…”");
    }

    #[test]
    fn a_name_is_needed_and_taken_names_are_said_whatever_the_case() {
        let c = conn();
        assert_eq!(saved(&c, "  ", Kind::Text, "x"), Err("Give the command a name.".into()));
        saved(&c, "Tax", Kind::Open, "C:\\tax").unwrap();
        assert_eq!(saved(&c, "tax", Kind::Text, "x"), Err("There's already a command called tax.".into()));
        assert_eq!(saved(&c, "x", Kind::Text, ""), Err("Write the text it pastes.".into()));
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
    fn a_link_with_a_slot_is_filled_with_what_is_typed_after_its_name() {
        let c = conn();
        saved(&c, "jira", Kind::Link, "jira.example.com/browse/{query}").unwrap();
        let cmd = find(&c, "jira").unwrap().unwrap();
        assert!(cmd.takes_query());
        assert_eq!(cmd.target, "https://jira.example.com/browse/{query}");
        assert_eq!(cmd.says(), "Opens jira.example.com/browse/… with what you type after “jira”");
        assert_eq!(fill_link(&cmd.target, " KAI-123 "), "https://jira.example.com/browse/KAI-123");
        assert_eq!(fill_link("https://x.com/s?q={query}", "a b&c/é"), "https://x.com/s?q=a%20b%26c%2F%C3%A9");
        assert_eq!(fill_link("https://x.com", "ignored"), "https://x.com");
    }

    #[test]
    fn snippets_fill_the_date_the_time_and_the_clipboard() {
        let at = chrono::NaiveDate::from_ymd_opt(2026, 11, 22).unwrap().and_hms_opt(9, 5, 0).unwrap();
        assert_eq!(expand("Sent {date} at {time}: {clipboard}", at, Some("link")), "Sent 2026-11-22 at 09:05: link");
        assert_eq!(expand("{clipboard}!", at, None), "!");
        assert_eq!(expand("plain", at, None), "plain");
    }

    #[test]
    fn an_alias_stands_for_other_words_and_not_for_itself() {
        let c = conn();
        saved(&c, "vs", Kind::Alias, "  visual studio code ").unwrap();
        let cmd = find(&c, "vs").unwrap().unwrap();
        assert_eq!((cmd.target.as_str(), cmd.says()), ("visual studio code", "Runs “visual studio code”".to_string()));
        assert!(saved(&c, "loop", Kind::Alias, "LOOP").is_err());
        assert!(saved(&c, "empty", Kind::Alias, " ").is_err());
        // Aliases mean the same on any computer, so they're shared.
        assert!(to_json(&c).unwrap().contains("visual studio code"));
    }

    #[test]
    fn a_name_is_cut_to_what_fits() {
        let c = conn();
        saved(&c, &"n".repeat(80), Kind::Text, "x").unwrap();
        assert_eq!(list(&c).unwrap()[0].name.chars().count(), MAX_NAME_CHARS);
    }

    #[test]
    fn a_keyword_is_one_short_word_of_its_own_on_text_only() {
        let c = conn();
        saved(&c, "sig", Kind::Text, "Best,\nAmos").unwrap();
        saved(&c, "addr", Kind::Text, "1 Road").unwrap();
        saved(&c, "gh", Kind::Link, "github.com").unwrap();
        assert_eq!(set_keyword(&c, "sig", ";sig").unwrap(), Ok(()));
        assert_eq!(find(&c, "sig").unwrap().unwrap().keyword.as_deref(), Some(";sig"));
        assert!(set_keyword(&c, "addr", ";SIG").unwrap().unwrap_err().contains(";sig"));
        assert!(set_keyword(&c, "addr", "; a").unwrap().is_err());
        assert!(set_keyword(&c, "addr", "x").unwrap().is_err());
        assert!(set_keyword(&c, "addr", &";".repeat(21)).unwrap().is_err());
        assert!(set_keyword(&c, "gh", ";gh").unwrap().is_err());
        // Its own keyword again is fine, and "" takes it away.
        assert_eq!(set_keyword(&c, "sig", ";sig").unwrap(), Ok(()));
        assert_eq!(set_keyword(&c, "sig", "").unwrap(), Ok(()));
        assert_eq!(find(&c, "sig").unwrap().unwrap().keyword, None);
    }

    #[test]
    fn renaming_keeps_the_keyword_and_the_keys() {
        let c = conn();
        saved(&c, "sig", Kind::Text, "Best").unwrap();
        set_keyword(&c, "sig", ";sig").unwrap().unwrap();
        set_keys_saved(&c, "sig", "Ctrl+Alt+S").unwrap();
        save(&c, Some("sig"), "signature", Kind::Text, "Best").unwrap().unwrap();
        let cmd = find(&c, "signature").unwrap().unwrap();
        assert_eq!((cmd.keyword.as_deref(), cmd.keys.as_deref()), (Some(";sig"), Some("Ctrl+Alt+S")));
    }

    #[test]
    fn keywords_are_shared_and_keys_stay_on_this_computer() {
        let c = conn();
        saved(&c, "sig", Kind::Text, "Best").unwrap();
        set_keyword(&c, "sig", ";sig").unwrap().unwrap();
        set_keys_saved(&c, "sig", "Ctrl+Alt+S").unwrap();
        let json = to_json(&c).unwrap();
        assert!(json.contains(";sig") && !json.contains("Ctrl+Alt+S"));
        // What comes back from another computer keeps this one's keys and takes their keyword.
        replace_from_json(&c, r#"[{"name":"sig","kind":"text","target":"Cheers","keyword":";s"}]"#).unwrap();
        let cmd = find(&c, "sig").unwrap().unwrap();
        assert_eq!((cmd.target.as_str(), cmd.keyword.as_deref(), cmd.keys.as_deref()), ("Cheers", Some(";s"), Some("Ctrl+Alt+S")));
    }
}
