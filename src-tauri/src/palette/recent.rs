//! Recent files: `recent` lists the files you opened lately, from the system's own list (the
//! Recent folder's shortcuts on Windows, Spotlight's last-used dates on a Mac, the desktop's
//! recently-used.xbel on Linux), and `recent invoice` those whose names hold "invoice". Only a
//! query led by the word shows them. Read when asked, kept for half a minute; nothing is saved.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use super::commands::{row, LEAD};
use super::search::{after, note_row, Action, Context, Glyph, Icon, Item};

/// Rows shown at most.
const MAX_ROWS: usize = 10;
/// Files read from the system's list at most.
const MAX_READ: usize = 200;
const KEEP_FOR: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq)]
pub struct Recent {
    /// What opens it: the file, or on Windows its shortcut in the Recent folder.
    pub path: String,
    /// Its name as shown: "Budget 2026.xlsx".
    pub name: String,
    pub at: SystemTime,
}

static CACHE: Mutex<Option<(Instant, Vec<Recent>)>> = Mutex::new(None);

/// The recent files, newest first, from the cache when it's fresh.
pub fn list() -> Vec<Recent> {
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, files)) = cache.as_ref() {
        if at.elapsed() < KEEP_FOR {
            return files.clone();
        }
    }
    let mut files = read();
    files.sort_by(|a, b| b.at.cmp(&a.at));
    files.truncate(MAX_READ);
    *cache = Some((Instant::now(), files.clone()));
    files
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// The Recent folder's shortcuts, by their own times: Windows names each after its file
/// ("report.docx.lnk"), and opening it opens the file. Folders' shortcuts are left out.
#[cfg(windows)]
fn read() -> Vec<Recent> {
    let Some(dir) = std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("Microsoft").join("Windows").join("Recent")) else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_str()?.strip_suffix(".lnk")?.to_string();
            // A folder's shortcut has no extension of its own in its name.
            Path::new(&name).extension()?;
            Some(Recent { at: modified(&path)?, path: path.to_string_lossy().into_owned(), name })
        })
        .collect()
}

/// Files Spotlight saw used in the last week, in your home folder.
#[cfg(target_os = "macos")]
fn read() -> Vec<Recent> {
    let Some(home) = super::files::home() else { return Vec::new() };
    let mut cmd = std::process::Command::new("mdfind");
    cmd.arg("-onlyin").arg(&home).arg("kMDItemLastUsedDate >= $time.today(-7) && kMDItemContentTypeTree != public.folder");
    let deadline = Instant::now() + Duration::from_millis(800);
    let lines = super::files::process::lines(cmd, deadline, MAX_READ, &[0], |_| true).unwrap_or_default();
    lines
        .into_iter()
        .filter_map(|line| {
            let path = PathBuf::from(&line);
            if !super::files::kept(&path, &home) {
                return None;
            }
            Some(Recent { name: path.file_name()?.to_string_lossy().into_owned(), at: std::fs::metadata(&path).and_then(|m| m.accessed().or(m.modified())).ok()?, path: line })
        })
        .collect()
}

/// The desktop's list of recently used files (GTK's and KDE's both keep it).
#[cfg(not(any(windows, target_os = "macos")))]
fn read() -> Vec<Recent> {
    let file = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).or_else(|| super::files::home().map(|h| h.join(".local").join("share"))).map(|d| d.join("recently-used.xbel"));
    let Some(text) = file.and_then(|f| std::fs::read_to_string(f).ok()) else { return Vec::new() };
    parse_xbel(&text).into_iter().filter(|r| Path::new(&r.path).is_file()).collect()
}

/// A `file://` address as a path: "%20" and the like decoded.
pub fn file_path(href: &str) -> Option<String> {
    let encoded = href.strip_prefix("file://")?;
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?, 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok()
}

/// The bookmarks in recently-used.xbel: each `<bookmark href="file:///…" modified="2026-…Z">`,
/// timed by when it was last visited or changed.
pub fn parse_xbel(text: &str) -> Vec<Recent> {
    let attr = |tag: &str, name: &str| -> Option<String> {
        let at = tag.find(&format!(" {name}=\""))? + name.len() + 3;
        Some(tag[at..].split('"').next()?.replace("&amp;", "&").replace("&apos;", "'").replace("&quot;", "\""))
    };
    text.split("<bookmark ")
        .skip(1)
        .filter_map(|tag| {
            // The split took the space before the first attribute.
            let tag = format!(" {}", tag.split('>').next()?);
            let path = file_path(&attr(&tag, "href")?)?;
            let when = attr(&tag, "visited").or_else(|| attr(&tag, "modified"))?;
            let at = chrono::DateTime::parse_from_rfc3339(&when).ok()?;
            let at = SystemTime::UNIX_EPOCH + Duration::from_millis(at.timestamp_millis().max(0) as u64);
            let name = Path::new(&path).file_name()?.to_string_lossy().into_owned();
            Some(Recent { path, name, at })
        })
        .collect()
}

/// How long ago, for a row: "5m ago", "3h ago", "yesterday", "4 days ago".
fn ago(at: SystemTime, now: SystemTime) -> String {
    let secs = now.duration_since(at).map(|d| d.as_secs()).unwrap_or(0);
    match secs {
        s if s < 60 => "just now".into(),
        s if s < 3600 => format!("{}m ago", s / 60),
        s if s < 86_400 => format!("{}h ago", s / 3600),
        s if s < 2 * 86_400 => "yesterday".into(),
        s => format!("{} days ago", s / 86_400),
    }
}

/// `recent` and `recent files` list them; `recent budget` those whose names hold "budget".
pub fn rows(_ctx: &Context, query: &str) -> Vec<Item> {
    let q = query.trim();
    let filter = if ["recent", "recent files", "recents"].iter().any(|w| q.eq_ignore_ascii_case(w)) {
        String::new()
    } else {
        match after(q, "recent ").or_else(|| after(q, "recents ")) {
            Some(f) => f.trim().to_lowercase(),
            None => return Vec::new(),
        }
    };
    rows_of(&list(), &filter, SystemTime::now())
}

fn rows_of(files: &[Recent], filter: &str, now: SystemTime) -> Vec<Item> {
    let found: Vec<&Recent> = files.iter().filter(|f| filter.is_empty() || f.name.to_lowercase().contains(filter)).take(MAX_ROWS).collect();
    if found.is_empty() {
        let says = if filter.is_empty() { "No recent files from the system".to_string() } else { format!("No recent file is called “{filter}”") };
        let mut item = note_row(says, Glyph::Search, "Recent files");
        item.score = LEAD;
        return vec![item];
    }
    found
        .into_iter()
        .enumerate()
        .map(|(i, f)| {
            let mut item = row(f.name.clone(), Glyph::Search, "Recent files", Action::OpenFile(f.path.clone()), ago(f.at, now), (LEAD - i as f64 / 100.0, Vec::new()));
            item.icon = Icon::File(f.path.clone());
            item.second = Some((Action::RevealFile(f.path.clone()), "Show in folder"));
            item
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const XBEL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xbel version="1.0" xmlns:bookmark="http://www.freedesktop.org/standards/desktop-bookmarks">
  <bookmark href="file:///home/me/Documents/Budget%202026.ods" added="2026-11-20T09:00:00Z" modified="2026-11-20T09:00:00Z" visited="2026-11-22T18:30:00.123456Z">
    <info/>
  </bookmark>
  <bookmark href="file:///home/me/Notes/Tom%20&amp;%20Jerry.txt" added="2026-11-21T09:00:00Z" modified="2026-11-21T10:00:00Z">
  </bookmark>
  <bookmark href="https://example.com/not-a-file" modified="2026-11-21T10:00:00Z"/>
</xbel>"#;

    #[test]
    fn the_desktops_list_gives_each_file_and_when_it_was_used() {
        let found = parse_xbel(XBEL);
        assert_eq!(found.iter().map(|r| (r.path.as_str(), r.name.as_str())).collect::<Vec<_>>(), vec![
            ("/home/me/Documents/Budget 2026.ods", "Budget 2026.ods"),
            ("/home/me/Notes/Tom & Jerry.txt", "Tom & Jerry.txt"),
        ]);
        let visited = chrono::DateTime::parse_from_rfc3339("2026-11-22T18:30:00.123Z").unwrap().timestamp_millis();
        assert_eq!(found[0].at, SystemTime::UNIX_EPOCH + Duration::from_millis(visited as u64));
        assert_eq!(file_path("file:///a/100%25%20done"), Some("/a/100% done".into()));
    }

    fn file(name: &str, mins_ago: u64, now: SystemTime) -> Recent {
        Recent { path: format!("/home/me/{name}"), name: name.into(), at: now - Duration::from_secs(mins_ago * 60) }
    }

    #[test]
    fn rows_open_the_file_newest_first_and_a_word_narrows_them() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        let files = vec![file("Budget.ods", 5, now), file("Essay draft.docx", 200, now), file("photo.png", 60 * 30, now)];
        let rows = rows_of(&files, "", now);
        assert_eq!(rows.iter().map(|r| (r.title.as_str(), r.sub.as_deref())).collect::<Vec<_>>(), vec![
            ("Budget.ods", Some("5m ago")),
            ("Essay draft.docx", Some("3h ago")),
            ("photo.png", Some("yesterday")),
        ]);
        assert_eq!(rows[0].action, Action::OpenFile("/home/me/Budget.ods".into()));
        assert_eq!(rows[0].second, Some((Action::RevealFile("/home/me/Budget.ods".into()), "Show in folder")));
        assert_eq!(rows_of(&files, "essay", now)[0].title, "Essay draft.docx");
        assert_eq!(rows_of(&files, "zzz", now)[0].title, "No recent file is called “zzz”");
    }

    #[test]
    fn only_a_query_led_by_recent_asks() {
        let c = super::super::search::tests::ctx();
        assert!(rows(&c, "receipt").is_empty());
        assert!(rows(&c, "recently").is_empty());
        assert!(!rows(&c, "recent").is_empty());
    }
}
