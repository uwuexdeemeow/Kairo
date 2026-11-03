//! Files in the palette: what the system's own index finds for what's typed, ranked and turned
//! into rows. Asking the index is each OS's own module; everything here is testable without one.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use super::search::{Action, Icon, Item};

#[cfg(windows)]
mod windows;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(windows, target_os = "macos")))]
mod linux;
#[cfg(unix)]
mod process;

/// Fewest characters typed before the index is asked.
pub const MIN_CHARS: usize = 2;
/// Most file rows shown.
pub const MAX_FILES: usize = 5;
/// File rows shown at most with the palette in its Files mode, where they're all it shows.
pub const MODE_MAX_FILES: usize = 12;
/// How long typing must pause before the index is asked, in milliseconds.
pub const QUIET_MS: u64 = 120;
/// How long the index gets to answer before it's given up on, in milliseconds.
pub const LIMIT_MS: u64 = 1500;

/// Why the system's index gave no answer.
#[derive(Debug, PartialEq)]
pub enum Unavailable {
    /// There is no index to ask, like a Linux desktop without localsearch or plocate. Windows
    /// always has Windows Search, so it never says this.
    #[cfg_attr(windows, allow(dead_code))]
    NoIndexer,
    /// The index was asked and could not answer, for the reason given.
    Failed(String),
}

/// Asks an index for files named like the query, giving up at the deadline.
pub type Backend = fn(&str, &Path, Instant) -> Result<Vec<PathBuf>, Unavailable>;

/// The user's home folder, which files are searched under and shown relative to.
pub fn home() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).filter(|h| !h.is_empty()).map(PathBuf::from)
}

/// What follows `home` in a path, or None when it isn't under it. Windows paths differ in case
/// between indexers, so there the folders compare without it.
fn under(path: &Path, home: &Path) -> Option<PathBuf> {
    if let Ok(rest) = path.strip_prefix(home) {
        return Some(rest.to_path_buf());
    }
    #[cfg(windows)]
    {
        let same = |a: std::path::Component, b: std::path::Component| a.as_os_str().to_string_lossy().to_lowercase() == b.as_os_str().to_string_lossy().to_lowercase();
        let mut parts = path.components();
        for h in home.components() {
            if !same(parts.next()?, h) {
                return None;
            }
        }
        return Some(parts.as_path().to_path_buf());
    }
    #[allow(unreachable_code)]
    None
}

/// Folders whose files are clutter wherever they sit under home: dependencies, caches and build
/// output, which copy the user's own files ("dist/resume.pdf") or bury them in thousands of others.
/// Compared without case.
const CLUTTER: [&str; 10] = ["node_modules", ".git", "target", "__pycache__", "site-packages", "dist", "build", "venv", "vendor", "pods"];

/// Extensions of source code and its config, which rank after documents, pictures and the rest:
/// a search is for a file of one's own far more often than for a program's parts.
const CODE: [&str; 36] = [
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "pyc", "go", "java", "kt", "swift", "c", "h", "cpp", "hpp", "cs", "rb", "php", "lua", "sh", "ps1", "html", "css",
    "scss", "vue", "svelte", "json", "yaml", "yml", "toml", "lock", "xml", "map", "d",
];

/// Whether a path is source code or its config, by its extension.
fn is_code(path: &Path) -> bool {
    path.extension().is_some_and(|e| CODE.contains(&e.to_string_lossy().to_lowercase().as_str()))
}
/// Top-level folders in home that hold the system's and apps' own files, not the user's.
const APP_FOLDERS: [&str; 2] = ["library", "appdata"];

/// Folders inside `~/Library` that hold the user's own files: iCloud Drive and apps' iCloud
/// folders, and the drives macOS's File Provider keeps (Dropbox, OneDrive, Google Drive).
const CLOUD_FOLDERS: [&str; 2] = ["mobile documents", "cloudstorage"];

/// Whether a path is worth showing: under home, and not hidden, inside a build or cache folder,
/// or in the app data folders at the top of home, cloud drives inside them aside.
pub fn kept(path: &Path, home: &Path) -> bool {
    let Some(inside) = under(path, home) else { return false };
    let parts: Vec<String> = inside.components().map(|p| p.as_os_str().to_string_lossy().to_lowercase()).collect();
    let cloud = parts.len() > 2 && parts[0] == "library" && CLOUD_FOLDERS.contains(&parts[1].as_str());
    !parts.is_empty()
        && parts.iter().enumerate().all(|(i, name)| {
            !name.starts_with('.') && !CLUTTER.contains(&name.as_str()) && !(i == 0 && !cloud && APP_FOLDERS.contains(&name.as_str()))
        })
}

/// A file's folder as its row shows it: from home ("~/School/Notes"), or in a cloud drive inside
/// Library by the drive's name ("iCloud Drive/Downloads", "iCloud Drive/Pages", "Dropbox/Taxes").
fn shown_folder(folder: &Path) -> String {
    let parts: Vec<String> = folder.components().map(|p| p.as_os_str().to_string_lossy().to_string()).collect();
    let is = |i: usize, name: &str| parts.get(i).is_some_and(|p| p.eq_ignore_ascii_case(name));
    let sep = std::path::MAIN_SEPARATOR.to_string();
    let (label, rest) = if is(0, "Library") && is(1, "Mobile Documents") && parts.len() > 2 {
        if parts[2] == "com~apple~CloudDocs" {
            ("iCloud Drive".to_string(), &parts[3..])
        } else {
            // An app's iCloud folder, like com~apple~Pages, keeps its files in Documents.
            let app = parts[2].rsplit('~').next().unwrap_or(&parts[2]);
            let rest = if is(3, "Documents") { &parts[4..] } else { &parts[3..] };
            (format!("iCloud Drive{sep}{app}"), rest)
        }
    } else if is(0, "Library") && is(1, "CloudStorage") && parts.len() > 2 {
        (parts[2].clone(), &parts[3..])
    } else if parts.is_empty() {
        return "~".to_string();
    } else {
        ("~".to_string(), &parts[..])
    };
    std::iter::once(label).chain(rest.iter().cloned()).collect::<Vec<_>>().join(&sep)
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// How a name matches the query, as (rank, characters matched): 0 when it starts with it, 1 when
/// a word in it does, 2 when it's only inside, and 3 when the query's words are all in it but
/// apart, the way the indexes match them ("amos lim" in "Amos_Lim_Resume.pdf"). None otherwise.
fn rank(name: &str, query: &[char]) -> Option<(u8, Vec<usize>)> {
    if let Some(found) = rank_phrase(name, query) {
        return Some(found);
    }
    let words: Vec<&[char]> = query.split(|c| c.is_whitespace()).filter(|w| !w.is_empty()).collect();
    if words.len() < 2 {
        return None;
    }
    let mut at = Vec::new();
    for word in words {
        at.extend(rank_phrase(name, word)?.1);
    }
    at.sort_unstable();
    at.dedup();
    Some((3, at))
}

/// How a name holds `query` in one piece, as `rank` counts it from 0 to 2.
fn rank_phrase(name: &str, query: &[char]) -> Option<(u8, Vec<usize>)> {
    let chars: Vec<char> = name.chars().map(lower).collect();
    let at = |i: usize| (i..i + query.len()).collect::<Vec<_>>();
    let found: Vec<usize> = (0..=chars.len().checked_sub(query.len())?).filter(|&i| chars[i..i + query.len()] == *query).collect();
    let first = *found.first()?;
    if first == 0 {
        return Some((0, at(0)));
    }
    match found.iter().find(|&&i| matches!(chars[i - 1], ' ' | '_' | '-' | '.' | '(' | ')' | '[' | ']')) {
        Some(&i) => Some((1, at(i))),
        None => Some((2, at(first))),
    }
}

/// When a file was last changed; the oldest time when that can't be read.
fn modified(path: &Path) -> SystemTime {
    std::fs::metadata(path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH)
}

/// The rows for what an index found: leftovers filtered out, each path once, everything but code
/// before code, and within each names starting with the query first, then words, then anywhere,
/// newest first, and at most `MAX_FILES`.
pub fn rows(query: &str, home: &Path, found: Vec<PathBuf>) -> Vec<Item> {
    rows_up_to(query, home, found, MAX_FILES)
}

/// As `rows`, keeping at most `limit`.
pub fn rows_up_to(query: &str, home: &Path, found: Vec<PathBuf>, limit: usize) -> Vec<Item> {
    let query: Vec<char> = query.trim().chars().map(lower).collect();
    if query.is_empty() {
        return Vec::new();
    }
    let mut seen = std::collections::HashSet::new();
    let mut matched: Vec<((bool, u8), SystemTime, Vec<usize>, PathBuf)> = found
        .into_iter()
        .filter(|path| kept(path, home) && seen.insert(path.clone()))
        .filter_map(|path| {
            let (rank, at) = rank(&path.file_name()?.to_string_lossy(), &query)?;
            Some(((is_code(&path), rank), modified(&path), at, path))
        })
        .collect();
    matched.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));
    matched.truncate(limit);
    matched
        .into_iter()
        .map(|(_, _, at, path)| {
            let shown = path.to_string_lossy().to_string();
            let folder = shown_folder(&path.parent().and_then(|p| under(p, home)).unwrap_or_default());
            let mut item = Item::new(path.file_name().unwrap_or_default().to_string_lossy(), Icon::File(shown.clone()), "Files", Action::OpenFile(shown.clone()));
            item.at = at;
            item.sub = Some(folder);
            item.second = Some((Action::RevealFile(shown), "Show in folder"));
            item
        })
        .collect()
}

/// The row shown in place of files when this computer has nothing to search them with.
/// Files mode's answer when the index was too slow or failed: said, rather than left looking.
pub fn unanswered_row() -> Item {
    Item::new("File search didn't answer. Try again in a moment", Icon::File(String::new()), "Files", Action::Nothing)
}

pub fn no_indexer_row() -> Item {
    Item::new("File search needs localsearch or plocate", Icon::File(String::new()), "Files", Action::Nothing)
}

/// What kind of file a path is, from its extension, for the icon drawn when it has none of its own.
pub fn file_kind(path: &str) -> &'static str {
    let p = Path::new(path);
    if p.extension().is_none() && p.is_dir() {
        return "folder";
    }
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "heic" | "tif" | "tiff" => "image",
        "pdf" | "doc" | "docx" | "odt" | "rtf" | "txt" | "md" | "xls" | "xlsx" | "ods" | "csv" | "ppt" | "pptx" | "odp" | "pages" | "numbers" | "key" => "document",
        _ => "other",
    }
}

/// Asks this computer's own index for files named like the query.
pub fn ask_system(query: &str, home: &Path, deadline: Instant) -> Result<Vec<PathBuf>, Unavailable> {
    #[cfg(windows)]
    return windows::ask(query, home, deadline);
    #[cfg(target_os = "macos")]
    return macos::ask(query, home, deadline);
    #[cfg(not(any(windows, target_os = "macos")))]
    return linux::ask(query, home, deadline);
}

// ---------- the worker ----------

/// Most index calls still running past their deadline; while this many are, new queries are
/// dropped rather than asked, so an index that hangs can't pile up threads.
const MAX_LATE: usize = 4;

/// What a worker and its thread share, behind one lock.
#[derive(Default)]
struct Shared {
    /// The newest query not yet asked: its results' number, its text and when it came.
    pending: Option<(u64, String, Instant, usize)>,
    /// Whether the missing index was said since the palette last opened.
    told_no_indexer: bool,
    /// Whether a failed search was logged since the palette last opened, so an index that's
    /// broken logs once rather than at every pause in typing.
    logged_failure: bool,
    /// Set when the worker goes away, so its thread ends.
    stop: bool,
}

/// Searches files on a thread of its own, only ever for the newest query, and hands the rows it
/// finds to `on_done` with the number of the results they belong to.
pub struct Worker(Arc<(Mutex<Shared>, Condvar)>);

impl Worker {
    /// A worker asking `backend`. `on_done` is called on the worker's thread, so it must be quick.
    pub fn new(backend: Backend, on_done: impl Fn(u64, Vec<Item>) + Send + 'static) -> Self {
        let shared = Arc::new((Mutex::new(Shared::default()), Condvar::new()));
        let theirs = shared.clone();
        let spawned = std::thread::Builder::new().name("palette-files".into()).spawn(move || work(&theirs, backend, on_done));
        if let Err(e) = spawned {
            eprintln!("palette: cannot search files: {e}");
        }
        Worker(shared)
    }

    /// Searches for `query` once typing pauses, in place of any query still waiting, keeping at
    /// most `limit` rows.
    pub fn ask_up_to(&self, seq: u64, query: String, limit: usize) {
        let (lock, wake) = &*self.0;
        lock.lock().unwrap_or_else(|e| e.into_inner()).pending = Some((seq, query, Instant::now(), limit));
        wake.notify_one();
    }

    /// The palette opened, so a missing index is said again, and a failing one logged again.
    pub fn opened(&self) {
        let mut s = self.0 .0.lock().unwrap_or_else(|e| e.into_inner());
        s.told_no_indexer = false;
        s.logged_failure = false;
    }
}

impl Drop for Worker {
    /// Ends the thread: at once when it's waiting, or when the search it's on gives up.
    fn drop(&mut self) {
        let (lock, wake) = &*self.0;
        lock.lock().unwrap_or_else(|e| e.into_inner()).stop = true;
        wake.notify_one();
    }
}

/// The worker's thread: waits for a query and a pause after it, asks, and hands on what's found,
/// until the worker goes away.
fn work(shared: &(Mutex<Shared>, Condvar), backend: Backend, on_done: impl Fn(u64, Vec<Item>)) {
    let (lock, wake) = shared;
    let late = Arc::new(AtomicUsize::new(0));
    loop {
        let (seq, query, limit) = {
            let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if s.stop {
                    return;
                }
                // Each new query restarts the pause, so typing a word asks once.
                let due = s.pending.as_ref().map(|(_, _, at, _)| *at + Duration::from_millis(QUIET_MS));
                match due.map(|due| due.saturating_duration_since(Instant::now())) {
                    None => s = wake.wait(s).unwrap_or_else(|e| e.into_inner()),
                    Some(wait) if !wait.is_zero() => s = wake.wait_timeout(s, wait).unwrap_or_else(|e| e.into_inner()).0,
                    Some(_) => break,
                }
            }
            let Some((seq, query, _, limit)) = s.pending.take() else { continue };
            (seq, query, limit)
        };
        // Files mode shows "Looking for files…" until it hears back, so it always does.
        let files_mode = limit == MODE_MAX_FILES;
        let deadline = Instant::now() + Duration::from_millis(LIMIT_MS);
        // None when there was no answer in time (or no home folder to look in).
        let found = home().and_then(|home| ask_by(backend, &query, &home, deadline, &late).map(|r| r.map(|paths| rows_up_to(&query, &home, paths, limit))));
        if found.is_none() && !files_mode {
            continue;
        }
        let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
        // Gone, or a newer query is waiting, which makes these rows out of date already.
        if s.stop {
            return;
        }
        if s.pending.is_some() {
            continue;
        }
        // An index that answered is passed on even with nothing found, so Files mode can say so;
        // one that failed says nothing.
        let answered = matches!(found, Some(Ok(_)));
        let mut items = match found {
            Some(Ok(items)) => items,
            // Said once per opening among other results; every time in Files mode, where it's
            // the whole answer.
            Some(Err(Unavailable::NoIndexer)) if !s.told_no_indexer || files_mode => {
                s.told_no_indexer = true;
                vec![no_indexer_row()]
            }
            Some(Err(Unavailable::NoIndexer)) | None => Vec::new(),
            // The reason names the program and how it failed, never the query or a path.
            Some(Err(Unavailable::Failed(e))) => {
                if !s.logged_failure {
                    s.logged_failure = true;
                    eprintln!("palette: file search failed: {e}");
                }
                Vec::new()
            }
        };
        if files_mode && !answered && items.is_empty() {
            items = vec![unanswered_row()];
        }
        drop(s);
        if answered || !items.is_empty() {
            on_done(seq, items);
        }
    }
}

/// Runs `backend` on a thread of its own and waits for it until `deadline`; None when it's late
/// (its thread ends on its own and its answer is dropped) or too many before it still are.
fn ask_by(backend: Backend, query: &str, home: &Path, deadline: Instant, late: &Arc<AtomicUsize>) -> Option<Result<Vec<PathBuf>, Unavailable>> {
    if late.load(Ordering::SeqCst) >= MAX_LATE {
        return None;
    }
    let (tx, rx) = mpsc::channel();
    let (query, home, running) = (query.to_string(), home.to_path_buf(), late.clone());
    running.fetch_add(1, Ordering::SeqCst);
    let spawned = std::thread::Builder::new().name("palette-files-ask".into()).spawn(move || {
        let _ = tx.send(backend(&query, &home, deadline));
        running.fetch_sub(1, Ordering::SeqCst);
    });
    if let Err(e) = spawned {
        late.fetch_sub(1, Ordering::SeqCst);
        eprintln!("palette: cannot search files: {e}");
        return None;
    }
    // A backend that panics drops its sender, which ends the wait at once.
    rx.recv_timeout(deadline.saturating_duration_since(Instant::now())).ok()
}

/// Opens a file's folder with the file picked out where the file manager allows, else the folder.
/// False when nothing could be started.
pub fn reveal(path: &str) -> bool {
    use std::process::Command;
    let started = |c: &mut Command| c.spawn().is_ok();
    #[cfg(windows)]
    {
        // Explorer exits 1 even when it worked, so only starting it counts.
        {
            use std::os::windows::process::CommandExt;
            // Explorer wants the path quoted on its own, not the whole argument.
            started(Command::new("explorer").raw_arg(format!("/select,\"{path}\"")))
        }
    }
    #[cfg(target_os = "macos")]
    {
        started(Command::new("open").arg("-R").arg(path))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let uri: String = path.bytes().map(|b| if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
        let shown = Command::new("gdbus")
            .args(["call", "--session", "--dest", "org.freedesktop.FileManager1", "--object-path", "/org/freedesktop/FileManager1"])
            .args(["--method", "org.freedesktop.FileManager1.ShowItems"])
            .arg(format!("['file://{uri}']"))
            .arg("")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        shown || Path::new(path).parent().is_some_and(|folder| started(Command::new("xdg-open").arg(folder)))
    }
}

/// Prints what a search finds, one `path<tab>folder` per line, for `kairo --search-files`.
pub fn print_search(query: &str) {
    #[cfg(windows)]
    attach_console();
    let Some(home) = home() else {
        println!("failed: no home folder");
        return;
    };
    match ask_system(query, &home, Instant::now() + std::time::Duration::from_millis(LIMIT_MS)) {
        Ok(found) => {
            for item in rows(query, &home, found) {
                if let Action::OpenFile(path) = &item.action {
                    println!("{path}\t{}", item.sub.as_deref().unwrap_or(""));
                }
            }
        }
        Err(Unavailable::NoIndexer) => println!("no indexer"),
        Err(Unavailable::Failed(why)) => println!("failed: {why}"),
    }
}

/// A release build has no console of its own, so print to the one it was run from.
#[cfg(windows)]
pub(crate) fn attach_console() {
    use std::os::windows::io::IntoRawHandle;
    use windows_sys::Win32::System::Console::{AttachConsole, SetStdHandle, ATTACH_PARENT_PROCESS, STD_OUTPUT_HANDLE};
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) != 0 {
            if let Ok(out) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
                SetStdHandle(STD_OUTPUT_HANDLE, out.into_raw_handle() as _);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::search::{Action, Icon};

    fn home() -> PathBuf {
        PathBuf::from("/home/ana")
    }

    #[test]
    fn hidden_and_cluttered_paths_are_left_out() {
        let home = Path::new("/home/ana");
        assert!(kept(&home.join("Documents/budget.xlsx"), home));
        for p in [
            ".config/budget",
            "Library/Preferences/budget.plist",
            "AppData/Roaming/budget.txt",
            "code/app/node_modules/budget.js",
            "venv/lib/python3.12/site-packages/budget.py",
            "code/.git/budget",
            "code/app/target/budget",
            "code/__pycache__/budget.pyc",
                        "Documents/.budget.swp",
        ] {
            assert!(!kept(&home.join(p), home), "{p}");
        }
        assert!(!kept(Path::new("/etc/budget"), home));
        for p in ["Documents/Caches/budget.txt", "Documents/Library/budget.txt"] {
            assert!(kept(&home.join(p), home), "{p}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_match_home_whatever_their_case() {
        let home = Path::new(r"C:\Users\Ana");
        let path = PathBuf::from(r"c:\users\ana\Documents\Taxes\budget.xlsx");
        assert!(kept(&path, home));
        assert_eq!(rows("budget", home, vec![path]).remove(0).sub.as_deref(), Some(r"~\Documents\Taxes"));
    }

    #[test]
    fn rows_rank_start_then_word_then_inside_and_cap_at_five() {
        let h = home();
        let found = vec![h.join("mybudget.md"), h.join("old budget.txt"), h.join("budget.xlsx")];
        let titles: Vec<String> = rows("budget", &h, found).into_iter().map(|i| i.title).collect();
        assert_eq!(titles, ["budget.xlsx", "old budget.txt", "mybudget.md"]);
        let many: Vec<PathBuf> = (0..7).map(|n| h.join(format!("budget{n}.txt"))).collect();
        assert_eq!(rows("budget", &h, many).len(), MAX_FILES);
    }

    #[test]
    fn ties_go_to_the_newest() {
        let dir = std::env::temp_dir().join(format!("kairo-files-ties-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (old, new) = (dir.join("budget old.txt"), dir.join("budget new.txt"));
        for (path, age) in [(&new, 10), (&old, 1000)] {
            let file = std::fs::File::create(path).unwrap();
            file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(age)).unwrap();
        }
        // Both name "budget" at the start of their name; the newer wins whatever order they came in.
        let titles: Vec<String> = rows("budget", &dir, vec![old.clone(), new.clone()]).into_iter().map(|i| i.title).collect();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(titles, ["budget new.txt", "budget old.txt"]);
    }

    #[test]
    fn one_row_per_path() {
        let h = home();
        let path = h.join("budget.xlsx");
        assert_eq!(rows("budget", &h, vec![path.clone(), path]).len(), 1);
    }

    #[test]
    fn a_row_opens_the_file_and_shows_its_folder_from_home() {
        let h = home();
        let path = h.join("Documents").join("Taxes").join("budget.xlsx");
        let row = rows("budget", &h, vec![path.clone()]).remove(0);
        let shown = path.to_string_lossy().to_string();
        assert_eq!(row.title, "budget.xlsx");
        let sub = if cfg!(windows) { "~\\Documents\\Taxes" } else { "~/Documents/Taxes" };
        assert_eq!(row.sub.as_deref(), Some(sub));
        assert_eq!(row.section, "Files");
        assert_eq!(row.icon, Icon::File(shown.clone()));
        assert_eq!(row.action, Action::OpenFile(shown.clone()));
        assert_eq!(row.second, Some((Action::RevealFile(shown), "Show in folder")));
        assert_eq!(row.at, [0, 1, 2, 3, 4, 5]);
    }

    /// What each fake backend saw, and how long the slow one takes, per test, since a
    /// `Backend` is a plain function and can't carry its own.
    static SEEN_NEWEST: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static NO_INDEXER_CALLS: Mutex<u32> = Mutex::new(0);

    fn finds_budget(query: &str, home: &Path, _: Instant) -> Result<Vec<PathBuf>, Unavailable> {
        SEEN_NEWEST.lock().unwrap().push(query.to_string());
        Ok(vec![home.join("budget.txt")])
    }

    /// Answers well past the limit, so a slow test runner can't bring it back inside.
    fn too_slow(_: &str, home: &Path, _: Instant) -> Result<Vec<PathBuf>, Unavailable> {
        std::thread::sleep(Duration::from_millis(LIMIT_MS + 500));
        Ok(vec![home.join("budget.txt")])
    }

    fn no_indexer(_: &str, _: &Path, _: Instant) -> Result<Vec<PathBuf>, Unavailable> {
        *NO_INDEXER_CALLS.lock().unwrap() += 1;
        Err(Unavailable::NoIndexer)
    }

    /// No index, without counting: for tests other than the one that counts.
    fn no_indexer_here(_: &str, _: &Path, _: Instant) -> Result<Vec<PathBuf>, Unavailable> {
        Err(Unavailable::NoIndexer)
    }

    fn fails(_: &str, _: &Path, _: Instant) -> Result<Vec<PathBuf>, Unavailable> {
        Err(Unavailable::Failed("broken".into()))
    }

    /// A worker over `backend` whose answers arrive on the returned channel.
    fn worker(backend: Backend) -> (Worker, mpsc::Receiver<(u64, Vec<Item>)>) {
        let (tx, rx) = mpsc::channel();
        let worker = Worker::new(backend, move |seq, items| {
            let _ = tx.send((seq, items));
        });
        (worker, rx)
    }

    #[test]
    fn only_the_newest_query_is_asked() {
        if super::home().is_none() {
            return;
        }
        let (worker, rx) = worker(finds_budget);
        worker.ask_up_to(1, "bu".into(), MAX_FILES);
        worker.ask_up_to(2, "bud".into(), MAX_FILES);
        worker.ask_up_to(3, "budget".into(), MAX_FILES);
        let (seq, items) = rx.recv_timeout(Duration::from_secs(1)).expect("an answer");
        assert_eq!(seq, 3);
        assert_eq!(items[0].title, "budget.txt");
        assert_eq!(*SEEN_NEWEST.lock().unwrap(), ["budget"]);
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err(), "asked once");
    }

    #[test]
    fn a_search_past_the_limit_shows_nothing() {
        let (worker, rx) = worker(too_slow);
        worker.ask_up_to(1, "budget".into(), MAX_FILES);
        // Waits longer than the slow answer takes, so one let through late would arrive.
        assert!(rx.recv_timeout(Duration::from_millis(QUIET_MS + LIMIT_MS + 1500)).is_err());
    }

    #[test]
    fn no_indexer_is_said_once_per_opening() {
        let (worker, rx) = worker(no_indexer);
        worker.ask_up_to(1, "bu".into(), MAX_FILES);
        let (seq, items) = rx.recv_timeout(Duration::from_secs(1)).expect("the no-indexer row");
        assert_eq!((seq, items.len(), items[0].title.as_str()), (1, 1, no_indexer_row().title.as_str()));
        worker.ask_up_to(2, "bud".into(), MAX_FILES);
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
        assert_eq!(*NO_INDEXER_CALLS.lock().unwrap(), 2, "the second query was asked too");
        worker.opened();
        worker.ask_up_to(3, "budg".into(), MAX_FILES);
        assert_eq!(rx.recv_timeout(Duration::from_secs(1)).expect("said again").0, 3);
    }

    #[test]
    fn a_failed_search_shows_nothing() {
        let (worker, rx) = worker(fails);
        worker.ask_up_to(1, "budget".into(), MAX_FILES);
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
    }

    #[test]
    fn files_mode_always_hears_back() {
        // Its "Looking for files…" row waits for an answer: a late or failed search says so,
        // and a missing index after the first time says nothing was found rather than leaving it.
        let (failing, rx) = worker(fails);
        failing.ask_up_to(1, "budget".into(), MODE_MAX_FILES);
        let (seq, items) = rx.recv_timeout(Duration::from_secs(1)).expect("an answer");
        assert_eq!((seq, items[0].title.as_str()), (1, unanswered_row().title.as_str()));
        let (slow, rx) = worker(too_slow);
        slow.ask_up_to(2, "budget".into(), MODE_MAX_FILES);
        let (seq, items) = rx.recv_timeout(Duration::from_millis(QUIET_MS + LIMIT_MS + 500)).expect("an answer");
        assert_eq!((seq, items[0].title.as_str()), (2, unanswered_row().title.as_str()));
        let (missing, rx) = worker(no_indexer_here);
        missing.ask_up_to(3, "bu".into(), MODE_MAX_FILES);
        assert_eq!(rx.recv_timeout(Duration::from_secs(1)).expect("the no-indexer row").1[0].title, no_indexer_row().title);
        missing.ask_up_to(4, "bud".into(), MODE_MAX_FILES);
        assert_eq!(rx.recv_timeout(Duration::from_secs(1)).expect("an answer").1[0].title, no_indexer_row().title);
    }

    #[test]
    fn a_failed_search_is_logged_once_per_opening() {
        let (worker, rx) = worker(fails);
        let logged = || worker.0 .0.lock().unwrap().logged_failure;
        assert!(!logged());
        worker.ask_up_to(1, "budget".into(), MAX_FILES);
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
        assert!(logged());
        worker.opened();
        assert!(!logged());
    }

    #[test]
    fn the_thread_ends_with_its_worker() {
        let alive = Arc::new(());
        let held = alive.clone();
        let worker = Worker::new(fails, move |_, _| {
            let _ = &held;
        });
        assert_eq!(Arc::strong_count(&alive), 2);
        drop(worker);
        let start = Instant::now();
        while Arc::strong_count(&alive) > 1 {
            assert!(start.elapsed() < Duration::from_secs(1), "the thread is still running");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn cloud_drives_inside_library_are_searched() {
        let home = Path::new("/Users/ana");
        for p in [
            "Library/Mobile Documents/com~apple~CloudDocs/Downloads/IC.pdf",
            "Library/Mobile Documents/com~apple~Pages/Documents/Essay.pages",
            "Library/CloudStorage/Dropbox/Taxes/budget.xlsx",
            "Library/CloudStorage/OneDrive-Personal/budget.xlsx",
        ] {
            assert!(kept(&home.join(p), home), "{p}");
        }
        // The rest of Library is still the system's and apps' own.
        for p in ["Library/Caches/IC.pdf", "Library/Mobile Documents/com~apple~CloudDocs/.Trash/IC.pdf"] {
            assert!(!kept(&home.join(p), home), "{p}");
        }
    }

    #[test]
    fn cloud_folders_are_shown_by_the_drive_s_name() {
        let home = Path::new("/Users/ana");
        let sub = |p: &str| rows("ic", home, vec![home.join(p)]).remove(0).sub.unwrap();
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(sub("Library/Mobile Documents/com~apple~CloudDocs/Downloads/IC.pdf"), format!("iCloud Drive{sep}Downloads"));
        assert_eq!(sub("Library/Mobile Documents/com~apple~CloudDocs/IC.pdf"), "iCloud Drive");
        assert_eq!(sub("Library/Mobile Documents/com~apple~Pages/Documents/IC.pages"), format!("iCloud Drive{sep}Pages"));
        assert_eq!(sub("Library/CloudStorage/Dropbox/Taxes/IC.pdf"), format!("Dropbox{sep}Taxes"));
        assert_eq!(sub("Documents/IC.pdf"), format!("~{sep}Documents"));
    }

    #[test]
    fn build_output_is_left_out() {
        let home = Path::new("/home/ana");
        for p in ["code/site/dist/budget.pdf", "code/app/build/budget.js", "code/tool/venv/bin/budget", "code/web/vendor/budget.php", "code/ios/Pods/Budget.h"] {
            assert!(!kept(&home.join(p), home), "{p}");
        }
    }

    #[test]
    fn documents_come_before_code_that_matches_better() {
        let h = home();
        let found = vec![h.join("code/budget.ts"), h.join("code/budgets.test.ts"), h.join("Downloads/old budget.xlsx"), h.join("code/BudgetCard.tsx")];
        let titles: Vec<String> = rows("budget", &h, found).into_iter().map(|i| i.title).collect();
        assert_eq!(titles[0], "old budget.xlsx");
        assert_eq!(titles.len(), 4);
    }

    #[test]
    fn words_typed_apart_match_a_name_that_joins_them() {
        let h = home();
        let found = vec![h.join("Amos_Lim_Resume_2026.pdf"), h.join("amos lim notes.txt"), h.join("lim amos.txt"), h.join("amos.txt")];
        let rows = rows("amos lim", &h, found);
        let titles: Vec<&str> = rows.iter().map(|i| i.title.as_str()).collect();
        // The words as typed come first; then names holding every word, in any order.
        assert_eq!(titles[0], "amos lim notes.txt");
        assert_eq!(titles.len(), 3);
        assert!(titles.contains(&"Amos_Lim_Resume_2026.pdf") && titles.contains(&"lim amos.txt"));
        let resume = rows.iter().find(|i| i.title.starts_with("Amos_")).unwrap();
        assert_eq!(resume.at, [0, 1, 2, 3, 5, 6, 7]);
    }

    #[test]
    fn odd_characters_in_the_query_are_only_matched() {
        let h = home();
        let found = vec![h.join("Résumé 2026.pdf")];
        let _ = rows("\"*%& résumé", &h, found.clone());
        assert_eq!(rows("résumé", &h, found).len(), 1);
    }
}
