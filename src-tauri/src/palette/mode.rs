//! The palette's modes: a scope it can be put in, shown as a bubble at the left of the input, so
//! that everything typed after it goes to that mode alone (the calculator, a web search on one
//! site, files, open windows, tasks or notes). Which modes there are, how they're named, the short
//! words that open them and the rows that offer them. No drawing here, so all of it can be tested.

use super::search::{fuzzy, Action, Glyph, Icon, Item, Tone, WEB};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Calc,
    /// A web search on one site, by its place in `search::WEB`; the first is Google, "the web".
    Web(usize),
    Files,
    Windows,
    Tasks,
    Notes,
    /// Everything copied lately, to choose one to put back.
    Clipboard,
}

/// The short word for the calculator; the sites' are in `search::WEB`.
const CALC_WORD: &str = "=";
/// Mode rows rank this far below an equal match, so an app whose name it is stays first: "github"
/// and Enter still opens GitHub Desktop.
const ROW_PENALTY: f64 = 10.0;
/// A mode row needs this many characters typed, so a letter or two doesn't offer every mode.
const ROW_MIN_CHARS: usize = 2;

impl Mode {
    /// Every mode, in the order rows offer them.
    pub fn all() -> Vec<Mode> {
        let mut all = vec![Mode::Calc];
        all.extend((0..WEB.len()).map(Mode::Web));
        all.extend([Mode::Files, Mode::Windows, Mode::Tasks, Mode::Notes, Mode::Clipboard]);
        all
    }

    /// The bubble's name.
    pub fn name(self) -> &'static str {
        match self {
            Mode::Calc => "Calculator",
            Mode::Web(i) => WEB[i].1,
            Mode::Files => "Files",
            Mode::Windows => "Windows",
            Mode::Tasks => "Tasks",
            Mode::Notes => "Notes",
            Mode::Clipboard => "Clipboard",
        }
    }

    pub fn glyph(self) -> Glyph {
        match self {
            Mode::Calc => Glyph::Calc,
            Mode::Web(_) | Mode::Files | Mode::Windows => Glyph::Search,
            Mode::Tasks => Glyph::Focus,
            Mode::Notes | Mode::Clipboard => Glyph::Tag,
        }
    }

    /// The input's hint while the bubble is there.
    pub fn hint(self) -> String {
        match self {
            Mode::Calc => "Type a sum…".into(),
            Mode::Web(i) => format!("Search {}…", WEB[i].1),
            Mode::Files => "Type part of a file's name…".into(),
            Mode::Windows => "Find an open window…".into(),
            Mode::Tasks => "Find a task, or add one…".into(),
            Mode::Notes => "Find a note, or start one…".into(),
            Mode::Clipboard => "Find something you copied…".into(),
        }
    }

    /// A stable name for the web palette to hand back: `calc`, `web:gh`, `files` and so on.
    pub fn key(self) -> String {
        match self {
            Mode::Calc => "calc".into(),
            Mode::Web(i) => format!("web:{}", WEB[i].0),
            Mode::Files => "files".into(),
            Mode::Windows => "windows".into(),
            Mode::Tasks => "tasks".into(),
            Mode::Notes => "notes".into(),
            Mode::Clipboard => "clipboard".into(),
        }
    }

    pub fn from_key(key: &str) -> Option<Mode> {
        Mode::all().into_iter().find(|m| m.key() == key)
    }

    /// The row that offers the mode, and other words it's found by.
    fn row(self) -> (String, &'static str) {
        match self {
            Mode::Calc => ("Calculator".into(), "calc maths math sums"),
            Mode::Web(0) => ("Search the web".into(), "web google internet"),
            Mode::Web(i) => (format!("Search {}", WEB[i].1), WEB[i].1),
            Mode::Files => ("Find files".into(), "files documents"),
            Mode::Windows => ("Find a window".into(), "windows open switch"),
            Mode::Tasks => ("Find a task".into(), "tasks todo to-do"),
            Mode::Notes => ("Find a note".into(), "notes"),
            Mode::Clipboard => ("Clipboard history".into(), "clip clipboard copied paste"),
        }
    }
}

/// The mode a short word and a space at the start of `text` name, like "gh " for GitHub or "= "
/// for the calculator, and what's typed after the space. None without a space after the word.
pub fn shortcut(text: &str) -> Option<(Mode, &str)> {
    let (word, rest) = text.split_once(' ')?;
    if word == CALC_WORD {
        return Some((Mode::Calc, rest));
    }
    if word.eq_ignore_ascii_case("clip") || word.eq_ignore_ascii_case("clipboard") {
        return Some((Mode::Clipboard, rest));
    }
    let i = WEB.iter().position(|(w, _, _)| w.eq_ignore_ascii_case(word))?;
    Some((Mode::Web(i), rest))
}

/// Rows offering the modes `query` names: "calc" offers the calculator, "github" GitHub's search,
/// "files" the files. Choosing one puts the palette in that mode.
pub fn rows(query: &str) -> Vec<Item> {
    let q = query.trim();
    if q.chars().count() < ROW_MIN_CHARS {
        return Vec::new();
    }
    Mode::all()
        .into_iter()
        // Typing `clip` already lists the latest copies and a row for the rest (commands.rs).
        .filter(|mode| *mode != Mode::Clipboard)
        .filter_map(|mode| {
            let (title, words) = mode.row();
            let best = [fuzzy(q, &title), fuzzy(q, mode.name()), fuzzy(q, words)].into_iter().flatten().max_by(|a, b| a.score.total_cmp(&b.score))?;
            let mut item = Item::new(title.clone(), Icon::Glyph(mode.glyph(), Tone::Accent), "Modes", Action::EnterMode(mode));
            item.at = fuzzy(q, &title).map(|m| m.at).unwrap_or_default();
            item.sub = Some(match mode {
                Mode::Calc => "Everything you type after it is worked out".into(),
                Mode::Web(_) => format!("Everything you type after it searches {}", mode.name()),
                _ => format!("Only {} while it's on", mode.name().to_lowercase()),
            });
            item.score = best.score - ROW_PENALTY;
            Some(item)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_word_and_a_space_is_a_mode() {
        assert_eq!(shortcut("gh kairo"), Some((Mode::Web(4), "kairo")));
        assert_eq!(shortcut("GH "), Some((Mode::Web(4), "")));
        assert_eq!(shortcut("yt lofi beats"), Some((Mode::Web(1), "lofi beats")));
        assert_eq!(shortcut("= 2+2"), Some((Mode::Calc, "2+2")));
        assert_eq!(shortcut("clip invoice"), Some((Mode::Clipboard, "invoice")));
        assert_eq!(shortcut("Clipboard "), Some((Mode::Clipboard, "")));
        assert_eq!(shortcut("clip"), None);
        assert_eq!(shortcut("clipart x"), None);
        // Without the space it's still being typed; other words are no mode.
        assert_eq!(shortcut("gh"), None);
        assert_eq!(shortcut("ghost town"), None);
        assert_eq!(shortcut("=2"), None);
    }

    #[test]
    fn every_mode_has_a_key_that_comes_back() {
        for mode in Mode::all() {
            assert_eq!(Mode::from_key(&mode.key()), Some(mode), "{mode:?}");
            assert!(!mode.hint().is_empty());
        }
        assert_eq!(Mode::Web(4).key(), "web:gh");
        assert_eq!(Mode::from_key("nonsense"), None);
    }

    #[test]
    fn modes_are_offered_by_name() {
        let first = |q: &str| rows(q).into_iter().max_by(|a, b| a.score.total_cmp(&b.score)).map(|i| i.action);
        assert_eq!(first("calculator"), Some(Action::EnterMode(Mode::Calc)));
        assert_eq!(first("calc"), Some(Action::EnterMode(Mode::Calc)));
        assert_eq!(first("search web"), Some(Action::EnterMode(Mode::Web(0))));
        assert_eq!(first("github"), Some(Action::EnterMode(Mode::Web(4))));
        assert_eq!(first("youtube"), Some(Action::EnterMode(Mode::Web(1))));
        assert_eq!(first("files"), Some(Action::EnterMode(Mode::Files)));
        assert_eq!(first("notes"), Some(Action::EnterMode(Mode::Notes)));
        // The clipboard has its own rows for `clip`, so no mode row repeats them.
        assert!(rows("clipboard").is_empty());
        // A letter alone offers nothing.
        assert!(rows("g").is_empty());
        assert!(rows("zzqx").is_empty());
    }
}
