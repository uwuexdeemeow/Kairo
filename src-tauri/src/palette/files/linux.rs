//! Linux file search: asks the desktop's own index (localsearch, formerly tracker3), else the
//! locate database (plocate, else locate).

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Instant;

use super::{kept, process, Unavailable};

/// The most paths one search lists.
const MAX_PATHS: usize = 50;

/// The search programs this can ask, in the order they're preferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Localsearch,
    Tracker3,
    Plocate,
    Locate,
}

impl Tool {
    const ALL: [Tool; 4] = [Tool::Localsearch, Tool::Tracker3, Tool::Plocate, Tool::Locate];

    fn program(self) -> &'static str {
        match self {
            Tool::Localsearch => "localsearch",
            Tool::Tracker3 => "tracker3",
            Tool::Plocate => "plocate",
            Tool::Locate => "locate",
        }
    }
}

/// Whether an executable of this name sits in one of the folders.
fn on_path(name: &str, path: &std::ffi::OsStr) -> bool {
    use std::os::unix::fs::PermissionsExt;
    env::split_paths(path).any(|dir| dir.join(name).metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

/// The first tool found in the folders, in order of preference.
fn first_on(path: &std::ffi::OsStr) -> Option<Tool> {
    Tool::ALL.into_iter().find(|t| on_path(t.program(), path))
}

/// The first tool found on `PATH`, looked up once and kept.
pub fn tool() -> Option<Tool> {
    static FOUND: OnceLock<Option<Tool>> = OnceLock::new();
    *FOUND.get_or_init(|| env::var_os("PATH").and_then(|p| first_on(&p)))
}

/// Takes the colour codes (`ESC [ ... m`) out of a line.
fn plain(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            for d in chars.by_ref() {
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Decodes `%XX` escapes, as UTF-8.
fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The paths among the `file://` lines of localsearch's or tracker3's output, which also holds
/// headers and may hold colour codes.
fn from_tracker(out: &str) -> Vec<PathBuf> {
    out.lines()
        .filter_map(|line| {
            let line = plain(line);
            let uri = line.trim().strip_prefix("file://")?;
            // Any host part is empty for local files: the path starts at the next slash.
            let path = percent_decode(&uri[uri.find('/')?..])?;
            Some(PathBuf::from(path))
        })
        .collect()
}

/// The absolute paths among plocate's or locate's lines that are under `home`.
fn from_locate(out: &str, home: &Path) -> Vec<PathBuf> {
    out.lines().map(Path::new).filter(|p| p.is_absolute() && kept(p, home)).map(Path::to_path_buf).collect()
}

/// Asks the first search tool found for files named like the query, up to the most or the deadline.
pub fn ask(query: &str, home: &Path, deadline: Instant) -> Result<Vec<PathBuf>, Unavailable> {
    let tool = tool().ok_or(Unavailable::NoIndexer)?;
    let mut cmd = Command::new(tool.program());
    match tool {
        Tool::Localsearch | Tool::Tracker3 => {
            cmd.args(["search", "--files", "--limit", "50"]).arg(query);
        }
        Tool::Plocate | Tool::Locate => {
            cmd.args(["-i", "-l", "200", "--"]).arg(query);
        }
    }
    // Tracker's lines carry headers, so they're read in full to find the paths among them.
    let lines = process::lines(cmd, deadline, if matches!(tool, Tool::Plocate | Tool::Locate) { 200 } else { MAX_PATHS * 4 })?;
    let out = lines.join("\n");
    Ok(match tool {
        Tool::Localsearch | Tool::Tracker3 => from_tracker(&out),
        Tool::Plocate | Tool::Locate => from_locate(&out, home),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_uris_become_decoded_paths_among_the_headers() {
        let out = "Results:\n  file:///home/ana/Documents/Tax%202026.pdf\n  file:///home/ana/caf%C3%A9.txt\nMore results:\n";
        assert_eq!(from_tracker(out), vec![PathBuf::from("/home/ana/Documents/Tax 2026.pdf"), PathBuf::from("/home/ana/café.txt")]);
    }

    #[test]
    fn tracker_colour_codes_are_stripped() {
        let out = "\x1b[1mFiles\x1b[0m\n  \x1b[34mfile:///home/ana/a%20b.txt\x1b[0m\n";
        assert_eq!(from_tracker(out), vec![PathBuf::from("/home/ana/a b.txt")]);
    }

    #[test]
    fn locate_keeps_only_paths_under_home() {
        let out = "/home/ana/Documents/tax.pdf\n/usr/share/doc/tax.txt\n/home/anabel/tax.md\n/home/ana/tax2.pdf\n";
        let home = Path::new("/home/ana");
        assert_eq!(from_locate(out, home), vec![PathBuf::from("/home/ana/Documents/tax.pdf"), PathBuf::from("/home/ana/tax2.pdf")]);
    }

    #[test]
    fn the_first_tool_on_path_in_order_of_preference() {
        use std::os::unix::fs::PermissionsExt;
        let dir = env::temp_dir().join(format!("kairo-tools-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(first_on(dir.as_os_str()), None);
        for name in ["locate", "plocate"] {
            let f = dir.join(name);
            std::fs::write(&f, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(first_on(dir.as_os_str()), Some(Tool::Plocate));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
