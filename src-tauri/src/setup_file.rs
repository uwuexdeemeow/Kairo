//! A setup file: your own commands, window layouts and scripts in one file, to carry to another
//! computer or keep. Adding one only ever adds: a name that is already here keeps what it has, and
//! a command's keys are never in the file, because they belong to the computer.

use std::io::Write;
use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::custom_commands::{self, CustomCommand};
use crate::scripts;
use crate::snap::{self, Layout};

const FORMAT: &str = "kairo-setup";
const VERSION: u32 = 1;
/// A file bigger than this isn't a setup.
pub const MAX_FILE_BYTES: u64 = 2_000_000;
/// A script longer than this stays out of the file.
const MAX_SCRIPT_BYTES: usize = 200_000;
const MAX_SCRIPTS: usize = 100;
const MAX_FILE_NAME: usize = 100;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ScriptFile {
    pub file: String,
    pub text: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Setup {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub commands: Vec<CustomCommand>,
    #[serde(default)]
    pub layouts: Vec<Layout>,
    #[serde(default)]
    pub scripts: Vec<ScriptFile>,
}

/// How many of each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub commands: usize,
    pub layouts: usize,
    pub scripts: usize,
}

impl Counts {
    pub fn is_empty(self) -> bool {
        self.commands + self.layouts + self.scripts == 0
    }

    /// "3 commands, 1 layout and 2 scripts", leaving out what there are none of.
    pub fn says(self) -> String {
        let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let mut parts = Vec::new();
        if self.commands > 0 {
            parts.push(plural(self.commands, "command", "commands"));
        }
        if self.layouts > 0 {
            parts.push(plural(self.layouts, "layout", "layouts"));
        }
        if self.scripts > 0 {
            parts.push(plural(self.scripts, "script", "scripts"));
        }
        match parts.len() {
            0 => "nothing".into(),
            1 => parts.remove(0),
            _ => {
                let last = parts.pop().unwrap_or_default();
                format!("{} and {last}", parts.join(", "))
            }
        }
    }
}

/// What this computer has, as a setup. Keys are left out; scripts are the titled ones in the
/// folder, as text.
pub fn build(conn: &Connection, scripts_dir: Option<&Path>) -> rusqlite::Result<Setup> {
    let commands = custom_commands::list(conn)?.into_iter().map(|c| CustomCommand { keys: None, ..c }).collect();
    let layouts = snap::list(conn)?;
    let scripts = scripts_dir
        .map(|dir| {
            scripts::list(dir)
                .into_iter()
                .filter_map(|s| {
                    let path = Path::new(&s.path);
                    let text = std::fs::read_to_string(path).ok().filter(|t| t.len() <= MAX_SCRIPT_BYTES)?;
                    Some(ScriptFile { file: path.file_name()?.to_str()?.to_string(), text })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Setup { format: FORMAT.into(), version: VERSION, commands, layouts, scripts })
}

/// Reads a setup file's text; says why when it isn't one.
pub fn parse(text: &str) -> Result<Setup, String> {
    let setup: Setup = serde_json::from_str(text).map_err(|_| "That isn't a Kairo setup file.".to_string())?;
    if setup.format != FORMAT {
        return Err("That isn't a Kairo setup file.".into());
    }
    if setup.version > VERSION {
        return Err("That file is from a newer Kairo. Update Kairo to add it.".into());
    }
    Ok(setup)
}

/// A script's file name that stays in the scripts folder: a plain name, not hidden.
fn safe_script_name(file: &str) -> bool {
    let plain = Path::new(file).file_name().is_some_and(|n| n == file);
    plain && !file.is_empty() && !file.starts_with('.') && file.chars().count() <= MAX_FILE_NAME && !file.contains(['/', '\\', ':'])
}

/// Whether this script could be added: a safe name, a title line, and nothing of that name there.
fn script_fits(dir: Option<&Path>, s: &ScriptFile) -> bool {
    let Some(dir) = dir else { return false };
    safe_script_name(&s.file) && s.text.len() <= MAX_SCRIPT_BYTES && scripts::parse(Path::new(&s.file), &s.text).is_some() && !dir.join(&s.file).exists()
}

/// What adding this setup would add: the names not already here. A few more may still be turned
/// down as they are added (a link that isn't one, or past the most there can be).
pub fn fresh(conn: &Connection, scripts_dir: Option<&Path>, setup: &Setup) -> rusqlite::Result<Counts> {
    let have = custom_commands::list(conn)?;
    let commands = setup.commands.iter().filter(|c| !have.iter().any(|h| h.name.eq_ignore_ascii_case(c.name.trim()))).count();
    let saved = snap::list(conn)?;
    let layouts = setup.layouts.iter().filter(|l| !saved.iter().any(|s| s.name.eq_ignore_ascii_case(l.name.trim()))).count();
    let scripts = setup.scripts.iter().take(MAX_SCRIPTS).filter(|s| script_fits(scripts_dir, s)).count();
    Ok(Counts { commands, layouts, scripts })
}

/// Adds what isn't here, and says how many of each went in.
pub fn apply(conn: &Connection, scripts_dir: Option<&Path>, setup: &Setup) -> Result<Counts, String> {
    let e = |x: rusqlite::Error| x.to_string();
    let mut added = Counts::default();
    for c in &setup.commands {
        if custom_commands::list(conn).map_err(e)?.iter().any(|h| h.name.eq_ignore_ascii_case(c.name.trim())) {
            continue;
        }
        // One the rules turn down (or past the most) is left out, not a failure for the rest.
        if custom_commands::save(conn, None, &c.name, c.kind, &c.target).map_err(e)?.is_err() {
            continue;
        }
        if let Some(keyword) = &c.keyword {
            let _ = custom_commands::set_keyword(conn, &c.name, keyword).map_err(e)?;
        }
        added.commands += 1;
    }
    for l in &setup.layouts {
        if snap::find(conn, &l.name).map_err(e)?.is_some() {
            continue;
        }
        if snap::save(conn, &l.name, l.windows.clone()).map_err(e)?.is_err() {
            continue;
        }
        // A focus label already on another layout stays there.
        if let Some(label) = l.focus_label.as_deref().filter(|f| snap::for_focus_label(conn, f).ok().flatten().is_none()) {
            snap::set_focus_label(conn, &l.name, Some(label)).map_err(e)?;
        }
        added.layouts += 1;
    }
    if let Some(dir) = scripts_dir {
        for s in setup.scripts.iter().take(MAX_SCRIPTS).filter(|s| script_fits(Some(dir), s)) {
            std::fs::create_dir_all(dir).map_err(|x| x.to_string())?;
            // Never over a file of that name.
            let Ok(mut f) = std::fs::OpenOptions::new().write(true).create_new(true).open(dir.join(&s.file)) else { continue };
            if f.write_all(s.text.as_bytes()).is_ok() {
                added.scripts += 1;
            }
        }
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom_commands::Kind;
    use crate::platform::Rect;
    use crate::snap::Placed;

    fn conn() -> Connection {
        crate::db::open(Path::new(":memory:")).unwrap()
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kairo-setup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn windows() -> Vec<Placed> {
        vec![Placed { app: "code.exe".into(), title: "a".into(), rect: Rect { x: 0, y: 0, w: 800, h: 600 } }]
    }

    #[test]
    fn what_one_computer_saves_another_adds_without_keys() {
        let (from, to) = (conn(), conn());
        custom_commands::save(&from, None, "gh", Kind::Link, "github.com").unwrap().unwrap();
        custom_commands::save(&from, None, "sig", Kind::Text, "Best,").unwrap().unwrap();
        custom_commands::set_keyword(&from, "sig", ";sig").unwrap().unwrap();
        custom_commands::set_keys_saved(&from, "gh", "Ctrl+Alt+G").unwrap();
        snap::save(&from, "Writing", windows()).unwrap().unwrap();
        snap::set_focus_label(&from, "Writing", Some("essay")).unwrap();
        let (src, dst) = (temp("src"), temp("dst"));
        std::fs::write(src.join("deploy.sh"), "# @kairo.title Deploy\necho hi").unwrap();
        std::fs::write(src.join("notes.txt"), "no title").unwrap();

        let setup = parse(&serde_json::to_string(&build(&from, Some(&src)).unwrap()).unwrap()).unwrap();
        assert!(setup.commands.iter().all(|c| c.keys.is_none()));
        assert_eq!(setup.scripts.len(), 1);

        assert_eq!(fresh(&to, Some(&dst), &setup).unwrap(), Counts { commands: 2, layouts: 1, scripts: 1 });
        assert_eq!(apply(&to, Some(&dst), &setup).unwrap(), Counts { commands: 2, layouts: 1, scripts: 1 });
        let got = custom_commands::list(&to).unwrap();
        assert_eq!(got.iter().find(|c| c.name == "sig").unwrap().keyword.as_deref(), Some(";sig"));
        assert!(got.iter().all(|c| c.keys.is_none()));
        assert_eq!(snap::find(&to, "writing").unwrap().unwrap().focus_label.as_deref(), Some("essay"));
        assert!(dst.join("deploy.sh").exists());
        // The same file again adds nothing.
        assert!(fresh(&to, Some(&dst), &setup).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn a_name_already_here_keeps_what_it_has() {
        let c = conn();
        custom_commands::save(&c, None, "gh", Kind::Link, "gitlab.com").unwrap().unwrap();
        snap::save(&c, "Writing", windows()).unwrap().unwrap();
        snap::set_focus_label(&c, "Writing", Some("essay")).unwrap();
        let dst = temp("keep");
        std::fs::write(dst.join("deploy.sh"), "# @kairo.title Mine\n").unwrap();
        let setup = Setup {
            format: FORMAT.into(),
            version: VERSION,
            commands: vec![CustomCommand::new("GH", Kind::Link, "github.com")],
            layouts: vec![
                Layout { name: "writing".into(), windows: windows(), focus_label: None },
                // A label that is taken stays with the layout that has it.
                Layout { name: "Reading".into(), windows: windows(), focus_label: Some("Essay".into()) },
            ],
            scripts: vec![ScriptFile { file: "deploy.sh".into(), text: "# @kairo.title Theirs\n".into() }],
        };
        assert_eq!(apply(&c, Some(&dst), &setup).unwrap(), Counts { commands: 0, layouts: 1, scripts: 0 });
        assert_eq!(custom_commands::list(&c).unwrap()[0].target, "https://gitlab.com");
        assert_eq!(std::fs::read_to_string(dst.join("deploy.sh")).unwrap(), "# @kairo.title Mine\n");
        assert_eq!(snap::find(&c, "Writing").unwrap().unwrap().focus_label.as_deref(), Some("essay"));
        assert_eq!(snap::find(&c, "Reading").unwrap().unwrap().focus_label, None);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn scripts_stay_in_the_folder_and_need_a_title() {
        let dst = temp("names");
        let s = |file: &str, text: &str| ScriptFile { file: file.into(), text: text.into() };
        let good = "# @kairo.title Fine\n";
        assert!(script_fits(Some(&dst), &s("ok.sh", good)));
        for bad in ["../x.sh", "a/b.sh", "a\\b.sh", ".hidden.sh", "C:x.sh", ""] {
            assert!(!script_fits(Some(&dst), &s(bad, good)), "{bad}");
        }
        assert!(!script_fits(Some(&dst), &s("untitled.sh", "echo hi")));
        assert!(!script_fits(None, &s("ok.sh", good)));
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn a_file_that_is_not_a_setup_says_so() {
        assert!(parse("not json").is_err());
        assert!(parse("{\"format\":\"other\",\"version\":1}").is_err());
        assert!(parse("{\"format\":\"kairo-setup\",\"version\":99}").unwrap_err().contains("newer Kairo"));
        // A file with only some parts is fine.
        assert_eq!(parse("{\"format\":\"kairo-setup\",\"version\":1,\"commands\":[]}").unwrap().layouts.len(), 0);
    }

    #[test]
    fn counts_read_aloud() {
        assert_eq!(Counts { commands: 3, layouts: 1, scripts: 2 }.says(), "3 commands, 1 layout and 2 scripts");
        assert_eq!(Counts { commands: 1, layouts: 0, scripts: 0 }.says(), "1 command");
        assert_eq!(Counts { commands: 0, layouts: 1, scripts: 2 }.says(), "1 layout and 2 scripts");
    }
}
