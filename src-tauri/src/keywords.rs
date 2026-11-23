//! Keywords: a text command's keyword (`;sig`) typed in any app is swapped for its text. The
//! system's listener (`platform::typing`) tells this what was typed; the last few characters are
//! kept in memory only, and forgotten on a click, a new window in front, or a key that moves the
//! caret, so a keyword is only ever one typed in a row. When what was typed ends with a keyword,
//! it's taken back with Backspaces and the text pasted in its place, through the clipboard, which
//! is put back as it was after.
//!
//! The listener runs only while some command has a keyword, and not at all on Wayland, where no
//! app may see another's keys. Nothing typed is written anywhere or leaves this module.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use rusqlite::Connection;
use tauri::AppHandle;

/// What a key press means for the buffer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Typed {
    Char(char),
    /// Backspace: the last character is gone.
    Back,
    /// A click, another window, or a key that moves the caret: what came before no longer leads
    /// to where the caret is.
    Reset,
}

/// Characters kept, enough for the longest keyword.
pub const KEPT: usize = 32;

/// The characters typed last, in order.
#[derive(Default, Debug)]
pub struct Buffer {
    chars: Vec<char>,
}

impl Buffer {
    /// Takes a key press; when what's typed now ends with one of `keywords`, the index of that
    /// keyword (the longest, when one ends another) and the buffer starts again.
    pub fn feed(&mut self, typed: Typed, keywords: &[String]) -> Option<usize> {
        match typed {
            Typed::Reset => self.chars.clear(),
            Typed::Back => {
                self.chars.pop();
            }
            Typed::Char(c) => {
                self.chars.push(c);
                if self.chars.len() > KEPT {
                    self.chars.remove(0);
                }
                let found = keywords
                    .iter()
                    .enumerate()
                    .filter(|(_, k)| !k.is_empty() && self.ends_with(k))
                    .max_by_key(|(_, k)| k.chars().count())
                    .map(|(i, _)| i);
                if found.is_some() {
                    self.chars.clear();
                }
                return found;
            }
        }
        None
    }

    fn ends_with(&self, keyword: &str) -> bool {
        let k: Vec<char> = keyword.chars().collect();
        self.chars.len() >= k.len() && self.chars[self.chars.len() - k.len()..] == k[..]
    }
}

/// The keywords and the text each pastes, as last read from the database.
static KEYWORDS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
static BUFFER: Mutex<Buffer> = Mutex::new(Buffer { chars: Vec::new() });
static APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();
/// While a keyword is being swapped, keys aren't read, so our own don't count as typing.
static SWAPPING: AtomicBool = AtomicBool::new(false);
static LISTENING: AtomicBool = AtomicBool::new(false);

/// Whether keywords can work here: false on Wayland.
pub fn available() -> bool {
    crate::platform::typing::available()
}

/// Reads the keywords again, after they've changed, and starts listening the first time one
/// exists. Listening, once started, stays for the run but does nothing without keywords.
pub fn refresh(app: &AppHandle, conn: &Connection) {
    let _ = APP.set(app.clone());
    let found: Vec<(String, String)> = crate::custom_commands::list(conn)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| Some((c.keyword?, c.target)))
        .collect();
    let any = !found.is_empty();
    *KEYWORDS.lock().unwrap_or_else(|e| e.into_inner()) = found;
    BUFFER.lock().unwrap_or_else(|e| e.into_inner()).chars.clear();
    if any && available() && !LISTENING.swap(true, Ordering::SeqCst) && !crate::platform::typing::start(typed) {
        LISTENING.store(false, Ordering::SeqCst);
        eprintln!("keywords: the system wouldn't let Kairo hear typing");
    }
}

/// After a sync round that changed something: the keywords, and the commands' keys, as they
/// stand now. Nothing before Kairo has started up.
pub fn after_sync(conn: &Connection) {
    if let Some(app) = APP.get() {
        refresh(app, conn);
        crate::command_keys::refresh(app, conn);
    }
}

/// Whether the listener is running, for the settings page to say when it needs a permission.
pub fn listening() -> bool {
    LISTENING.load(Ordering::SeqCst)
}

/// The listener's callback, on its own thread: must return at once.
fn typed(t: Typed) {
    if SWAPPING.load(Ordering::SeqCst) {
        return;
    }
    let keywords = KEYWORDS.lock().unwrap_or_else(|e| e.into_inner());
    if keywords.is_empty() {
        return;
    }
    let words: Vec<String> = keywords.iter().map(|(k, _)| k.clone()).collect();
    let Some(i) = BUFFER.lock().unwrap_or_else(|e| e.into_inner()).feed(t, &words) else { return };
    let (keyword, text) = keywords[i].clone();
    drop(keywords);
    SWAPPING.store(true, Ordering::SeqCst);
    std::thread::spawn(move || {
        swap(&keyword, &text);
        SWAPPING.store(false, Ordering::SeqCst);
    });
}

/// Takes the keyword back and pastes the text in its place, then puts the clipboard back.
fn swap(keyword: &str, template: &str) {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let Some(app) = APP.get() else { return };
    // The keyword's last key is still on its way to the app.
    std::thread::sleep(std::time::Duration::from_millis(40));
    let before = app.clipboard().read_text().ok();
    let text = crate::custom_commands::expand(template, chrono::Local::now().naive_local(), before.as_deref());
    if !crate::platform::typing::backspaces(keyword.chars().count()) {
        // Said once a run: the next keyword would only say it again.
        static SAID: AtomicBool = AtomicBool::new(false);
        if !SAID.swap(true, Ordering::SeqCst) {
            let why = if cfg!(target_os = "macos") {
                "Kairo needs Accessibility, in System Settings, Privacy & Security, to swap a keyword for its text."
            } else {
                "Kairo couldn't press keys here to swap a keyword for its text."
            };
            crate::ring::tell(app, why);
        }
        return;
    }
    if crate::clipboard::write_quietly(app, &text).is_err() {
        return;
    }
    std::thread::sleep(std::time::Duration::from_millis(30));
    crate::platform::paste();
    // The app reads the clipboard as it pastes, which takes a moment.
    std::thread::sleep(std::time::Duration::from_millis(250));
    if let Some(before) = before {
        let _ = crate::clipboard::write_quietly(app, &before);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn typing(b: &mut Buffer, text: &str, keywords: &[String]) -> Option<usize> {
        let mut last = None;
        for c in text.chars() {
            last = b.feed(Typed::Char(c), keywords);
        }
        last
    }

    #[test]
    fn a_keyword_is_found_as_its_last_character_is_typed() {
        let k = words(&[";sig", ";addr"]);
        let mut b = Buffer::default();
        assert_eq!(typing(&mut b, "hello ;si", &k), None);
        assert_eq!(b.feed(Typed::Char('g'), &k), Some(0));
        // It starts again after a match, so the same word typed on doesn't fire twice.
        assert_eq!(b.feed(Typed::Char('g'), &k), None);
        assert_eq!(typing(&mut b, ";addr", &k), Some(1));
    }

    #[test]
    fn backspace_takes_back_and_a_reset_forgets() {
        let k = words(&[";sig"]);
        let mut b = Buffer::default();
        typing(&mut b, ";six", &k);
        b.feed(Typed::Back, &k);
        assert_eq!(b.feed(Typed::Char('g'), &k), Some(0));
        typing(&mut b, ";si", &k);
        b.feed(Typed::Reset, &k);
        assert_eq!(b.feed(Typed::Char('g'), &k), None);
    }

    #[test]
    fn the_longest_keyword_wins_when_one_ends_another() {
        let k = words(&["ig", ";sig"]);
        let mut b = Buffer::default();
        assert_eq!(typing(&mut b, ";sig", &k), Some(1));
        assert_eq!(typing(&mut b, "big", &k), Some(0));
    }

    #[test]
    fn keywords_match_their_case_and_only_the_last_characters_are_kept() {
        let k = words(&[";Sig"]);
        let mut b = Buffer::default();
        assert_eq!(typing(&mut b, ";sig", &k), None);
        typing(&mut b, &"x".repeat(200), &k);
        assert!(b.chars.len() <= KEPT);
        assert_eq!(typing(&mut b, ";Sig", &k), Some(0));
    }
}
