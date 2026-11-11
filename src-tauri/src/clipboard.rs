//! Clipboard history: the last pieces of text copied while Kairo runs, for the palette's `clip`
//! command. It lives in memory only: nothing is written to disk, to a backup or to a sync, and it
//! is gone when Kairo quits, because copied text often holds passwords and tokens.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use rusqlite::{Connection, Result};
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::db;

const KEY: &str = "clipboard_history";
/// The latest copies kept.
pub const MAX_ENTRIES: usize = 25;
/// A longer copy is skipped, not cut, so a copy put back is always whole.
pub const MAX_CHARS: usize = 10_000;
const POLL: Duration = Duration::from_secs(1);
/// A row's title is the first line of the copy, cut to this many characters.
const TITLE_CHARS: usize = 80;

/// Whether copies are remembered; on unless turned off.
pub fn enabled(conn: &Connection) -> Result<bool> {
    Ok(db::get_setting(conn, KEY)?.as_deref() != Some("0"))
}

pub fn set_enabled(conn: &Connection, on: bool) -> Result<()> {
    db::set_setting(conn, KEY, if on { "1" } else { "0" })
}

/// One copy, and when it was made.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub text: String,
    pub at_ms: i64,
}

#[derive(Default, Debug)]
pub struct History {
    /// Newest first.
    items: VecDeque<Entry>,
    /// The clipboard's text the last time it was read, so a copy isn't taken twice, and clearing
    /// the list doesn't bring back what is still on the clipboard.
    last_seen: Option<String>,
}

impl History {
    /// Takes note of the clipboard's text. Returns whether it was added: not when it is what was
    /// seen last, blank, or too long. A copy already in the list moves to the top.
    pub fn push(&mut self, text: &str, now: i64) -> bool {
        if self.last_seen.as_deref() == Some(text) {
            return false;
        }
        self.last_seen = Some(text.to_string());
        if text.trim().is_empty() || text.chars().count() > MAX_CHARS {
            return false;
        }
        self.items.retain(|e| e.text != text);
        self.items.push_front(Entry { text: text.to_string(), at_ms: now });
        self.items.truncate(MAX_ENTRIES);
        true
    }

    /// Notes what is on the clipboard without keeping it: copied before Kairo was watching.
    pub fn prime(&mut self, text: &str) {
        self.last_seen = Some(text.to_string());
    }

    /// Empties the list; what is on the clipboard now isn't taken again.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Forgets everything, including what was last seen.
    fn forget(&mut self) {
        self.items.clear();
        self.last_seen = None;
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.items.iter()
    }
}

/// The history and whether it is being kept, shared between the watcher and the palette.
pub struct Shared {
    history: Mutex<History>,
    on: AtomicBool,
}

impl Shared {
    pub fn new(on: bool) -> Shared {
        Shared { history: Mutex::new(History::default()), on: AtomicBool::new(on) }
    }

    fn lock(&self) -> MutexGuard<'_, History> {
        self.history.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Turning it off forgets everything.
    pub fn set_on(&self, on: bool) {
        self.on.store(on, Ordering::Relaxed);
        if !on {
            self.lock().forget();
        }
    }

    pub fn clear(&self) {
        self.lock().clear();
    }

    /// The copies with how long ago each was made, newest first; None while it is off.
    pub fn snapshot(&self, now: i64) -> Option<Vec<(String, i64)>> {
        self.on.load(Ordering::Relaxed).then(|| self.lock().entries().map(|e| (e.text.clone(), (now - e.at_ms).max(0))).collect())
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Reads the clipboard's text once a second and keeps what is new. What was on the clipboard
/// when Kairo started, or when the history was turned on, isn't kept. Nothing it reads is logged.
pub fn spawn(app: AppHandle) {
    thread::Builder::new()
        .name("kairo-clipboard".into())
        .spawn(move || {
            let mut primed = false;
            loop {
                thread::sleep(POLL);
                let shared = app.state::<Shared>();
                if !shared.on.load(Ordering::Relaxed) {
                    primed = false;
                    continue;
                }
                // Anything but text (an image, files, an empty clipboard) is an error here.
                let text = app.clipboard().read_text().ok();
                let mut history = shared.lock();
                match (primed, text) {
                    (false, Some(t)) => {
                        history.prime(&t);
                        primed = true;
                    }
                    (false, None) => primed = true,
                    (true, Some(t)) => {
                        history.push(&t, now_ms());
                    }
                    (true, None) => {}
                }
            }
        })
        .expect("failed to start clipboard thread");
}

/// What follows `clip` or `clipboard` when the query starts with it as a word: the filter, maybe
/// empty. None for anything else, so the history never shows in an ordinary search.
pub fn led_by(query: &str) -> Option<&str> {
    let q = query.trim();
    ["clipboard", "clip"].iter().find_map(|word| {
        let head = q.get(..word.len()).filter(|h| h.eq_ignore_ascii_case(word))?;
        let rest = &q[head.len()..];
        (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim())
    })
}

/// The first line that has anything on it, trimmed and cut to fit a row.
pub fn title_of(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if line.chars().count() > TITLE_CHARS {
        format!("{}…", line.chars().take(TITLE_CHARS).collect::<String>().trim_end())
    } else {
        line.to_string()
    }
}

/// How big a copy is: its lines when it has several, else its characters.
pub fn size_of(text: &str) -> String {
    let lines = text.lines().filter(|l| !l.trim().is_empty()).count();
    let chars = text.chars().count();
    match (lines, chars) {
        (n, _) if n > 1 => format!("{n} lines"),
        (_, 1) => "1 character".to_string(),
        (_, n) => format!("{n} characters"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_are_kept_newest_first_without_repeats() {
        let mut h = History::default();
        assert!(h.push("one", 1));
        assert!(h.push("two", 2));
        assert!(h.push("one", 3));
        let all: Vec<(&str, i64)> = h.entries().map(|e| (e.text.as_str(), e.at_ms)).collect();
        assert_eq!(all, [("one", 3), ("two", 2)]);
        // The same text read again is the same copy.
        assert!(!h.push("one", 4));
        assert_eq!(h.entries().count(), 2);
    }

    #[test]
    fn the_list_is_capped_and_blank_or_huge_copies_are_skipped() {
        let mut h = History::default();
        for i in 0..40 {
            h.push(&format!("copy {i}"), i);
        }
        assert_eq!(h.entries().count(), MAX_ENTRIES);
        assert_eq!(h.entries().next().unwrap().text, "copy 39");
        assert_eq!(h.entries().last().unwrap().text, "copy 15");
        assert!(!h.push("   \n ", 50));
        assert!(!h.push("", 51));
        assert!(!h.push(&"x".repeat(MAX_CHARS + 1), 52));
        assert!(h.push(&"x".repeat(MAX_CHARS), 53));
        assert_eq!(h.entries().count(), MAX_ENTRIES);
    }

    #[test]
    fn clearing_doesnt_bring_back_what_is_still_on_the_clipboard() {
        let mut h = History::default();
        h.push("secret", 1);
        h.clear();
        assert_eq!(h.entries().count(), 0);
        // The watcher reads the same text again a second later.
        assert!(!h.push("secret", 2));
        // Something else, then the same again, is a new copy.
        assert!(h.push("other", 3));
        assert!(h.push("secret", 4));
    }

    #[test]
    fn what_was_there_before_watching_isnt_kept() {
        let mut h = History::default();
        h.prime("old");
        assert!(!h.push("old", 1));
        assert_eq!(h.entries().count(), 0);
        assert!(h.push("new", 2));
    }

    #[test]
    fn shared_lists_ages_and_turning_it_off_forgets_everything() {
        let shared = Shared::new(true);
        shared.lock().push("a", 1_000);
        shared.lock().push("b", 5_000);
        assert_eq!(shared.snapshot(8_000), Some(vec![("b".to_string(), 3_000), ("a".to_string(), 7_000)]));
        shared.clear();
        assert_eq!(shared.snapshot(8_000), Some(vec![]));
        shared.lock().push("c", 9_000);
        shared.set_on(false);
        assert_eq!(shared.snapshot(10_000), None);
        shared.set_on(true);
        assert_eq!(shared.snapshot(10_000), Some(vec![]));
        // Turned back on, what was last seen is forgotten too (the watcher primes again).
        assert!(shared.lock().push("c", 11_000));
    }

    #[test]
    fn only_a_query_led_by_clip_or_clipboard_asks_for_the_history() {
        assert_eq!(led_by("clip"), Some(""));
        assert_eq!(led_by("Clipboard"), Some(""));
        assert_eq!(led_by("clip invoice"), Some("invoice"));
        assert_eq!(led_by("  clipboard   a b "), Some("a b"));
        for q in ["clipart", "eclip", "", "paste", "video clip"] {
            assert_eq!(led_by(q), None, "{q}");
        }
    }

    #[test]
    fn rows_are_titled_by_their_first_line_and_sized() {
        assert_eq!(title_of("\n  hello world  \nsecond"), "hello world");
        let long = "word ".repeat(40);
        let title = title_of(&long);
        assert!(title.ends_with('…') && title.chars().count() <= TITLE_CHARS + 1);
        assert_eq!(size_of("214 chars"), "9 characters");
        assert_eq!(size_of("x"), "1 character");
        assert_eq!(size_of("a\n\nb\nc"), "3 lines");
    }
}
