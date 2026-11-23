//! Your browsers' bookmarks, for the palette to find by name or address: Chrome, Edge, Brave,
//! Vivaldi, Chromium, Arc and Opera (each profile's `Bookmarks` file), and Firefox (its
//! `places.sqlite` and its log, read from a copy so a running Firefox is left alone). Read from this
//! computer's own files only, again only when one of them changes; nothing leaves the computer.
//! Safari keeps its bookmarks where macOS asks for Full Disk Access, so it isn't read.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// Bookmarks kept at most, across browsers.
const MOST: usize = 5000;

#[derive(Clone, Debug, PartialEq)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
    /// The browser it's in, as people call it: "Chrome", "Firefox".
    pub browser: &'static str,
}

/// Where each Chromium browser keeps its profiles, under the system's folder for app data.
fn chromium_roots() -> Vec<(&'static str, PathBuf)> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        for (name, dir) in [
            ("Chrome", "Google\\Chrome\\User Data"),
            ("Edge", "Microsoft\\Edge\\User Data"),
            ("Brave", "BraveSoftware\\Brave-Browser\\User Data"),
            ("Vivaldi", "Vivaldi\\User Data"),
            ("Chromium", "Chromium\\User Data"),
        ] {
            roots.push((name, local.join(dir)));
        }
        if let Some(roaming) = std::env::var_os("APPDATA").map(PathBuf::from) {
            roots.push(("Opera", roaming.join("Opera Software\\Opera Stable")));
        }
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let support = home.join("Library/Application Support");
        for (name, dir) in [
            ("Chrome", "Google/Chrome"),
            ("Edge", "Microsoft Edge"),
            ("Brave", "BraveSoftware/Brave-Browser"),
            ("Vivaldi", "Vivaldi"),
            ("Chromium", "Chromium"),
            ("Arc", "Arc/User Data"),
            ("Opera", "com.operasoftware.Opera"),
        ] {
            roots.push((name, support.join(dir)));
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
        if let Some(config) = config {
            for (name, dir) in [
                ("Chrome", "google-chrome"),
                ("Edge", "microsoft-edge"),
                ("Brave", "BraveSoftware/Brave-Browser"),
                ("Vivaldi", "vivaldi"),
                ("Chromium", "chromium"),
                ("Opera", "opera"),
            ] {
                roots.push((name, config.join(dir)));
            }
        }
    }
    roots
}

/// Where Firefox keeps its profiles.
fn firefox_root() -> Option<PathBuf> {
    #[cfg(windows)]
    return std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Mozilla\\Firefox\\Profiles"));
    #[cfg(target_os = "macos")]
    return std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/Firefox/Profiles"));
    #[cfg(not(any(windows, target_os = "macos")))]
    return std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".mozilla/firefox"));
}

/// Every bookmarks file there is, with its browser: a Chromium browser's root itself (Opera keeps
/// one profile there) and each profile folder under it, and each Firefox profile's database.
fn files() -> Vec<(&'static str, PathBuf)> {
    let mut found = Vec::new();
    let children = |dir: &Path| std::fs::read_dir(dir).map(|d| d.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect::<Vec<_>>()).unwrap_or_default();
    for (browser, root) in chromium_roots() {
        for dir in std::iter::once(root.clone()).chain(children(&root)) {
            let file = dir.join("Bookmarks");
            if file.is_file() {
                found.push((browser, file));
            }
        }
    }
    if let Some(root) = firefox_root() {
        for dir in children(&root) {
            let file = dir.join("places.sqlite");
            if file.is_file() {
                found.push(("Firefox", file));
            }
        }
    }
    found
}

/// The bookmarks in a Chromium `Bookmarks` file: every `url` node under its roots.
pub fn from_chromium(json: &str, browser: &'static str) -> Vec<Bookmark> {
    fn walk(node: &serde_json::Value, browser: &'static str, out: &mut Vec<Bookmark>) {
        if node.get("type").and_then(|t| t.as_str()) == Some("url") {
            let url = node.get("url").and_then(|u| u.as_str()).unwrap_or("");
            if url.starts_with("http://") || url.starts_with("https://") {
                let title = node.get("name").and_then(|n| n.as_str()).unwrap_or("").trim();
                out.push(Bookmark { title: if title.is_empty() { url.to_string() } else { title.to_string() }, url: url.to_string(), browser });
            }
        }
        if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
            for child in children {
                walk(child, browser, out);
            }
        }
    }
    let mut out = Vec::new();
    if let Ok(doc) = serde_json::from_str::<serde_json::Value>(json) {
        if let Some(roots) = doc.get("roots").and_then(|r| r.as_object()) {
            for root in roots.values() {
                walk(root, browser, &mut out);
            }
        }
    }
    out
}

/// The log a running Firefox writes new bookmarks to first, beside its database.
fn wal_of(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push("-wal");
    PathBuf::from(name)
}

/// Firefox's bookmarks, from a copy of its database and its write-ahead log: a running Firefox
/// keeps new bookmarks in the log until it checkpoints, and reading the database while it changes
/// could fail. The copy is read and then removed; Firefox's own files are only read.
fn from_firefox(path: &Path) -> Vec<Bookmark> {
    static COPIES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COPIES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("kairo-places-{}-{n}", std::process::id()));
    let read = || -> Option<Vec<Bookmark>> {
        std::fs::create_dir_all(&dir).ok()?;
        let copy = dir.join("places.sqlite");
        std::fs::copy(path, &copy).ok()?;
        let wal = wal_of(path);
        if wal.is_file() {
            std::fs::copy(&wal, wal_of(&copy)).ok()?;
        }
        Some(read_places(&copy))
    };
    let found = read().unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    found
}

fn read_places(path: &Path) -> Vec<Bookmark> {
    let Ok(conn) = rusqlite::Connection::open(path) else { return Vec::new() };
    let rows = conn
        .prepare("SELECT IFNULL(b.title, ''), p.url FROM moz_bookmarks b JOIN moz_places p ON b.fk = p.id WHERE b.type = 1 AND (p.url LIKE 'http://%' OR p.url LIKE 'https://%')")
        .and_then(|mut stmt| stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>());
    rows.unwrap_or_default()
        .into_iter()
        .map(|(title, url)| Bookmark { title: if title.trim().is_empty() { url.clone() } else { title.trim().to_string() }, url, browser: "Firefox" })
        .collect()
}

/// What was read last, and the files and their times it was read from.
type Cache = (Vec<(PathBuf, Option<SystemTime>)>, Arc<Vec<Bookmark>>);
static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// The bookmarks, each address once (the first browser's), read again only when a file changed.
pub fn all() -> Arc<Vec<Bookmark>> {
    let found = files();
    let stamp = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    // Firefox's log changes when a bookmark is added and the database doesn't, so it's watched too.
    let stamps: Vec<(PathBuf, Option<SystemTime>)> = found.iter().flat_map(|(_, p)| [(p.clone(), stamp(p)), (wal_of(p), stamp(&wal_of(p)))]).collect();
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((was, list)) = cache.as_ref() {
        if *was == stamps {
            return list.clone();
        }
    }
    let mut list: Vec<Bookmark> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (browser, path) in &found {
        let read = if *browser == "Firefox" { from_firefox(path) } else { std::fs::read_to_string(path).map(|j| from_chromium(&j, browser)).unwrap_or_default() };
        for b in read {
            if list.len() < MOST && seen.insert(b.url.clone()) {
                list.push(b);
            }
        }
    }
    let list = Arc::new(list);
    *cache = Some((stamps, list.clone()));
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chromium_file_gives_its_web_bookmarks_in_every_folder() {
        let json = r#"{"roots":{
            "bookmark_bar":{"type":"folder","children":[
                {"type":"url","name":"Kairo issues","url":"https://github.com/me/kairo/issues"},
                {"type":"folder","name":"Work","children":[{"type":"url","name":"","url":"https://linear.app/team"}]}
            ]},
            "other":{"type":"folder","children":[
                {"type":"url","name":"A script","url":"javascript:alert(1)"},
                {"type":"url","name":"Settings","url":"chrome://settings"}
            ]}
        },"version":1}"#;
        let found = from_chromium(json, "Chrome");
        assert_eq!(
            found.iter().map(|b| (b.title.as_str(), b.url.as_str())).collect::<Vec<_>>(),
            [("Kairo issues", "https://github.com/me/kairo/issues"), ("https://linear.app/team", "https://linear.app/team")]
        );
        assert!(from_chromium("not json", "Chrome").is_empty());
    }

    #[test]
    fn a_firefox_database_is_read_without_changing_it() {
        let dir = std::env::temp_dir().join(format!("kairo-bookmarks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("places.sqlite");
        {
            let c = rusqlite::Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT);
                 CREATE TABLE moz_bookmarks (id INTEGER PRIMARY KEY, type INTEGER, fk INTEGER, title TEXT);
                 INSERT INTO moz_places VALUES (1, 'https://developer.mozilla.org/'), (2, 'place:sort=8'), (3, 'https://example.com/');
                 INSERT INTO moz_bookmarks VALUES (1, 1, 1, 'MDN'), (2, 1, 2, 'Recent'), (3, 2, NULL, 'A folder'), (4, 1, 3, NULL);",
            )
            .unwrap();
        }
        let found = from_firefox(&path);
        assert_eq!(found.iter().map(|b| b.title.as_str()).collect::<Vec<_>>(), ["MDN", "https://example.com/"]);
        assert!(found.iter().all(|b| b.browser == "Firefox"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_bookmark_firefox_has_only_in_its_log_is_found() {
        let dir = std::env::temp_dir().join(format!("kairo-bookmarks-wal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("places.sqlite");
        // Firefox, running: the database in WAL mode, the new bookmark not checkpointed yet.
        let firefox = rusqlite::Connection::open(&path).unwrap();
        firefox
            .execute_batch(
                "PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;
                 CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT);
                 CREATE TABLE moz_bookmarks (id INTEGER PRIMARY KEY, type INTEGER, fk INTEGER, title TEXT);
                 INSERT INTO moz_places VALUES (1, 'https://example.com/new');
                 INSERT INTO moz_bookmarks VALUES (1, 1, 1, 'Just added');",
            )
            .unwrap();
        assert!(wal_of(&path).is_file());
        assert_eq!(from_firefox(&path).iter().map(|b| b.title.as_str()).collect::<Vec<_>>(), ["Just added"]);
        drop(firefox);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Run by hand (`cargo test --lib bookmarks -- --ignored --nocapture`) to see what this
    /// computer's browsers give.
    #[test]
    #[ignore]
    fn this_computer_s_bookmarks() {
        let all = all();
        let mut by: std::collections::BTreeMap<&str, usize> = Default::default();
        for b in all.iter() {
            *by.entry(b.browser).or_default() += 1;
        }
        println!("{} bookmarks: {by:?}", all.len());
        assert!(Arc::ptr_eq(&all, &super::all()), "read again though nothing changed");
    }
}
