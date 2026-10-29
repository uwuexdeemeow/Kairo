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
/// How long typing must pause before the index is asked, in milliseconds.
pub const QUIET_MS: u64 = 120;
/// How long the index gets to answer before it's given up on, in milliseconds.
pub const LIMIT_MS: u64 = 1500;

/// Why the system's index gave no answer.
#[derive(Debug, PartialEq)]
pub enum Unavailable {
    /// There is no index to ask, like a Linux desktop without localsearch or plocate.
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

/// Folders whose files are clutter wherever they sit under home.
const CLUTTER: [&str; 4] = ["node_modules", ".git", "target", "__pycache__"];
/// Top-level folders in home that hold the system's and apps' own files, not the user's.
const APP_FOLDERS: [&str; 2] = ["library", "appdata"];

/// Whether a path is worth showing: under home, and not hidden, inside a build or cache folder,
/// or in the app data folders at the top of home.
pub fn kept(path: &Path, home: &Path) -> bool {
    let Some(inside) = under(path, home) else { return false };
    let mut any = false;
    for (i, part) in inside.components().enumerate() {
        let name = part.as_os_str().to_string_lossy().to_lowercase();
        if name.starts_with('.') || CLUTTER.contains(&name.as_str()) || (i == 0 && APP_FOLDERS.contains(&name.as_str())) {
            return false;
        }
        any = true;
    }
    any
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// How a name matches the query, as (rank, characters matched): 0 when it starts with it, 1 when
/// a word in it does, 2 when it's only inside. None when it isn't there.
fn rank(name: &str, query: &[char]) -> Option<(u8, Vec<usize>)> {
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

/// The rows for what an index found: leftovers filtered out, each path once, names starting with
/// the query first, then words, then anywhere, newest first within each, and at most `MAX_FILES`.
pub fn rows(query: &str, home: &Path, found: Vec<PathBuf>) -> Vec<Item> {
    let query: Vec<char> = query.trim().chars().map(lower).collect();
    if query.is_empty() {
        return Vec::new();
    }
    let mut seen = std::collections::HashSet::new();
    let mut matched: Vec<(u8, SystemTime, Vec<usize>, PathBuf)> = found
        .into_iter()
        .filter(|path| kept(path, home) && seen.insert(path.clone()))
        .filter_map(|path| {
            let (rank, at) = rank(&path.file_name()?.to_string_lossy(), &query)?;
            Some((rank, modified(&path), at, path))
        })
        .collect();
    matched.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));
    matched.truncate(MAX_FILES);
    matched
        .into_iter()
        .map(|(_, _, at, path)| {
            let shown = path.to_string_lossy().to_string();
            let folder = path.parent().and_then(|p| under(p, home)).unwrap_or_default();
            let mut item = Item::new(path.file_name().unwrap_or_default().to_string_lossy(), Icon::File(shown.clone()), "Files", Action::OpenFile(shown.clone()));
            item.at = at;
            item.sub = Some(if folder.as_os_str().is_empty() { "~".to_string() } else { format!("~{}{}", std::path::MAIN_SEPARATOR, folder.display()) });
            item.second = Some((Action::RevealFile(shown), "Show in folder"));
            item
        })
        .collect()
}

/// The row shown in place of files when this computer has nothing to search them with.
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
    pending: Option<(u64, String, Instant)>,
    /// Whether the missing index was said since the palette last opened.
    told_no_indexer: bool,
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

    /// Searches for `query` once typing pauses, in place of any query still waiting.
    pub fn ask(&self, seq: u64, query: String) {
        let (lock, wake) = &*self.0;
        lock.lock().unwrap_or_else(|e| e.into_inner()).pending = Some((seq, query, Instant::now()));
        wake.notify_one();
    }

    /// The palette opened, so a missing index is said again.
    pub fn opened(&self) {
        self.0 .0.lock().unwrap_or_else(|e| e.into_inner()).told_no_indexer = false;
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
        let (seq, query) = {
            let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if s.stop {
                    return;
                }
                // Each new query restarts the pause, so typing a word asks once.
                let due = s.pending.as_ref().map(|(_, _, at)| *at + Duration::from_millis(QUIET_MS));
                match due.map(|due| due.saturating_duration_since(Instant::now())) {
                    None => s = wake.wait(s).unwrap_or_else(|e| e.into_inner()),
                    Some(wait) if !wait.is_zero() => s = wake.wait_timeout(s, wait).unwrap_or_else(|e| e.into_inner()).0,
                    Some(_) => break,
                }
            }
            let Some((seq, query, _)) = s.pending.take() else { continue };
            (seq, query)
        };
        let Some(home) = home() else { continue };
        let deadline = Instant::now() + Duration::from_millis(LIMIT_MS);
        let found = match ask_by(backend, &query, &home, deadline, &late) {
            Some(Ok(paths)) => Ok(rows(&query, &home, paths)),
            Some(Err(e)) => Err(e),
            // Past the deadline: nothing shows for it.
            None => continue,
        };
        let mut s = lock.lock().unwrap_or_else(|e| e.into_inner());
        // Gone, or a newer query is waiting, which makes these rows out of date already.
        if s.stop {
            return;
        }
        if s.pending.is_some() {
            continue;
        }
        let items = match found {
            Ok(items) => items,
            Err(Unavailable::NoIndexer) if !s.told_no_indexer => {
                s.told_no_indexer = true;
                vec![no_indexer_row()]
            }
            Err(Unavailable::NoIndexer) => Vec::new(),
            Err(Unavailable::Failed(e)) => {
                eprintln!("palette: file search failed: {e}");
                Vec::new()
            }
        };
        drop(s);
        if !items.is_empty() {
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
fn attach_console() {
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

    fn too_slow(_: &str, home: &Path, _: Instant) -> Result<Vec<PathBuf>, Unavailable> {
        std::thread::sleep(Duration::from_millis(1600));
        Ok(vec![home.join("budget.txt")])
    }

    fn no_indexer(_: &str, _: &Path, _: Instant) -> Result<Vec<PathBuf>, Unavailable> {
        *NO_INDEXER_CALLS.lock().unwrap() += 1;
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
        worker.ask(1, "bu".into());
        worker.ask(2, "bud".into());
        worker.ask(3, "budget".into());
        let (seq, items) = rx.recv_timeout(Duration::from_secs(1)).expect("an answer");
        assert_eq!(seq, 3);
        assert_eq!(items[0].title, "budget.txt");
        assert_eq!(*SEEN_NEWEST.lock().unwrap(), ["budget"]);
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err(), "asked once");
    }

    #[test]
    fn a_search_past_the_limit_shows_nothing() {
        let (worker, rx) = worker(too_slow);
        worker.ask(1, "budget".into());
        assert!(rx.recv_timeout(Duration::from_millis(2500)).is_err());
    }

    #[test]
    fn no_indexer_is_said_once_per_opening() {
        let (worker, rx) = worker(no_indexer);
        worker.ask(1, "bu".into());
        let (seq, items) = rx.recv_timeout(Duration::from_secs(1)).expect("the no-indexer row");
        assert_eq!((seq, items.len(), items[0].title.as_str()), (1, 1, no_indexer_row().title.as_str()));
        worker.ask(2, "bud".into());
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
        assert_eq!(*NO_INDEXER_CALLS.lock().unwrap(), 2, "the second query was asked too");
        worker.opened();
        worker.ask(3, "budg".into());
        assert_eq!(rx.recv_timeout(Duration::from_secs(1)).expect("said again").0, 3);
    }

    #[test]
    fn a_failed_search_shows_nothing() {
        let (worker, rx) = worker(fails);
        worker.ask(1, "budget".into());
        assert!(rx.recv_timeout(Duration::from_millis(400)).is_err());
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
    fn odd_characters_in_the_query_are_only_matched() {
        let h = home();
        let found = vec![h.join("Résumé 2026.pdf")];
        let _ = rows("\"*%& résumé", &h, found.clone());
        assert_eq!(rows("résumé", &h, found).len(), 1);
    }
}
