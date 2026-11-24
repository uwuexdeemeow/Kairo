//! The palette's small tools for people who work with code: a fresh `uuid`, `base64` text in and
//! out, a `sha256` or `sha512` of some text, `json` made readable or squeezed onto one line, and
//! `lorem` filler. Each answers with a row: Enter copies the result, Ctrl+Enter pastes it where you
//! were. With no text after the word, the newest thing you copied is used (while the clipboard
//! history is on). No drawing and nothing is stored, so all of it can be tested.

use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use base64::Engine;
use sha2::{Digest, Sha256, Sha512};

use super::commands::{row, LEAD};
use super::search::{note_row, Action, Context, Glyph, Item};

const SECTION: &str = "Developer";
/// Text longer than this isn't worked on: a row is for a line, not a file.
const MAX_INPUT: usize = 200_000;
/// A row's title shows this much of the result.
const TITLE_CHARS: usize = 80;
const MAX_PARAGRAPHS: usize = 10;
const MAX_WORDS: usize = 500;

/// The words this module answers to, with the space that follows them when something is typed
/// after; the calculator leaves a query that starts with one alone.
pub const WORDS: &[&str] = &["uuid", "guid", "base64", "sha256", "sha512", "hash", "json", "lorem"];

/// What follows `word` when the query starts with it as a whole word: Some("") for the word alone.
fn rest<'a, 'w>(query: &'a str, words: &[&'w str]) -> Option<(&'w str, &'a str)> {
    let q = query.trim();
    let (first, rest) = q.split_once(char::is_whitespace).unwrap_or((q, ""));
    let word = words.iter().find(|w| first.eq_ignore_ascii_case(w))?;
    Some((word, rest.trim()))
}

/// Whether the query starts with one of this module's words, as a word.
pub fn leads(query: &str) -> bool {
    rest(query, WORDS).is_some()
}

// ---------- the work ----------

/// A random version 4 UUID in its usual lowercase form.
pub fn uuid() -> String {
    let mut b = [0u8; 16];
    if getrandom::getrandom(&mut b).is_err() {
        // No randomness from the system is rare; a clock-mixed fallback beats a broken row.
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        for (i, x) in b.iter_mut().enumerate() {
            *x = (n >> ((i % 8) * 8)) as u8 ^ (i as u8).wrapping_mul(151);
        }
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[0..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..32])
}

pub fn encode64(text: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(text.as_bytes())
}

/// Text from base64, standard or URL-safe, with or without the `=` padding and any spaces.
pub fn decode64(text: &str) -> Result<String, String> {
    let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let config = GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent);
    let bytes = GeneralPurpose::new(&alphabet::STANDARD, config).decode(&clean).or_else(|_| GeneralPurpose::new(&alphabet::URL_SAFE, config).decode(&clean)).map_err(|_| "That isn't base64.".to_string())?;
    String::from_utf8(bytes).map_err(|_| "That decodes to bytes that aren't text.".to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256(text: &str) -> String {
    hex(&Sha256::digest(text.as_bytes()))
}

pub fn sha512(text: &str) -> String {
    hex(&Sha512::digest(text.as_bytes()))
}

/// JSON text made readable (two-space indent) or, with `indent` None, put on one line. Order,
/// numbers and strings are kept exactly as written; only the space between them changes. Says why
/// when it isn't JSON.
pub fn json(text: &str, indent: Option<usize>) -> Result<String, String> {
    serde_json::from_str::<serde::de::IgnoredAny>(text).map_err(|e| format!("That isn't valid JSON: {e}"))?;
    let chars: Vec<char> = text.trim().chars().collect();
    let mut out = String::with_capacity(chars.len());
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    let newline = |out: &mut String, depth: usize| {
        if let Some(step) = indent {
            out.push('\n');
            out.push_str(&" ".repeat(depth * step));
        }
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
        } else {
            match c {
                '"' => {
                    in_string = true;
                    out.push(c);
                }
                '{' | '[' => {
                    out.push(c);
                    let close = if c == '{' { '}' } else { ']' };
                    // An empty one stays `{}` or `[]`.
                    let next = chars[i + 1..].iter().position(|x| !x.is_whitespace()).map(|p| i + 1 + p);
                    if next.is_some_and(|n| chars[n] == close) {
                        out.push(close);
                        i = next.unwrap_or(i);
                    } else {
                        depth += 1;
                        newline(&mut out, depth);
                    }
                }
                '}' | ']' => {
                    depth = depth.saturating_sub(1);
                    newline(&mut out, depth);
                    out.push(c);
                }
                ',' => {
                    out.push(c);
                    newline(&mut out, depth);
                }
                ':' => out.push_str(if indent.is_some() { ": " } else { ":" }),
                c if c.is_whitespace() => {}
                c => out.push(c),
            }
        }
        i += 1;
    }
    Ok(out)
}

const SENTENCES: &[&str] = &[
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.",
    "Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.",
    "Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur.",
    "Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.",
    "Sed ut perspiciatis unde omnis iste natus error sit voluptatem accusantium doloremque laudantium.",
    "Totam rem aperiam, eaque ipsa quae ab illo inventore veritatis et quasi architecto beatae vitae dicta sunt explicabo.",
    "Nemo enim ipsam voluptatem quia voluptas sit aspernatur aut odit aut fugit, sed quia consequuntur magni dolores eos.",
    "Neque porro quisquam est, qui dolorem ipsum quia dolor sit amet, consectetur, adipisci velit.",
];

/// `count` paragraphs of filler, four sentences each, starting from the classic one.
pub fn lorem_paragraphs(count: usize) -> String {
    (0..count.clamp(1, MAX_PARAGRAPHS))
        .map(|p| (0..4).map(|s| SENTENCES[(p * 4 + s) % SENTENCES.len()]).collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// `count` words of filler, from the classic start.
pub fn lorem_words(count: usize) -> String {
    let all: Vec<&str> = SENTENCES.iter().flat_map(|s| s.split(' ')).collect();
    let words: Vec<&str> = (0..count.clamp(1, MAX_WORDS)).map(|i| all[i % all.len()]).collect();
    let mut text = words.join(" ");
    // Ends on a full stop, with no comma left hanging.
    text = text.trim_end_matches([',', '.']).to_string();
    text.push('.');
    text
}

// ---------- the rows ----------

/// A result as a row: the text (cut to one line) as the title, Enter copies it, Ctrl+Enter pastes.
fn result_row(ctx: &Context, what: &str, text: String, place: f64) -> Item {
    let first = text.lines().next().unwrap_or("");
    let cut: String = first.chars().take(TITLE_CHARS).collect();
    let more = if cut.chars().count() < first.chars().count() || text.lines().nth(1).is_some() { "…" } else { "" };
    let mut item = row(format!("{cut}{more}"), Glyph::Script, SECTION, Action::Copy(text.clone()), what.to_string(), (place, Vec::new()));
    item.second = Some((Action::Paste(text, ctx.windows.first().map(|w| w.id.clone())), "Paste"));
    item
}

fn problem(why: impl Into<String>) -> Vec<Item> {
    let mut item = note_row(why, Glyph::Script, SECTION);
    item.score = LEAD;
    vec![item]
}

/// The newest thing copied as text, while the history is on.
fn copied(ctx: &Context) -> Option<&str> {
    ctx.live.clips.as_ref()?.iter().find(|c| c.image.is_none() && !c.text.is_empty()).map(|c| c.text.as_str())
}

/// What the tool works on: what's typed after its word, or else the newest copy. Err is the line
/// that says what's missing or too big.
fn input<'a>(ctx: &'a Context, typed: &'a str, word: &str) -> Result<(&'a str, bool), String> {
    let (text, from_clip) = if typed.is_empty() {
        match copied(ctx) {
            Some(c) => (c, true),
            None => {
                return Err(if ctx.live.clips.is_none() {
                    format!("Type some text after {word}, or turn on the clipboard history to use what you copied")
                } else {
                    format!("Type some text after {word}, or copy some first")
                })
            }
        }
    } else {
        (typed, false)
    };
    if text.len() > MAX_INPUT {
        return Err("That's too much text for the palette.".into());
    }
    Ok((text, from_clip))
}

fn source(from_clip: bool) -> &'static str {
    if from_clip {
        " · of what you copied"
    } else {
        ""
    }
}

pub fn rows(ctx: &Context, query: &str) -> Vec<Item> {
    let Some((word, typed)) = rest(query, WORDS) else { return Vec::new() };
    let word = word.to_lowercase();
    match word.as_str() {
        "uuid" | "guid" => vec![result_row(ctx, "A new random UUID", uuid(), LEAD)],
        "base64" => {
            let (decode, typed) = match typed.split_once(char::is_whitespace) {
                Some((d, r)) if d.eq_ignore_ascii_case("decode") || d.eq_ignore_ascii_case("d") => (true, r.trim()),
                None if typed.eq_ignore_ascii_case("decode") => (true, ""),
                _ => (false, typed),
            };
            let (text, clip) = match input(ctx, typed, if decode { "base64 decode" } else { "base64" }) {
                Ok(t) => t,
                Err(e) => return problem(e),
            };
            if decode {
                match decode64(text) {
                    Ok(out) => vec![result_row(ctx, &format!("Decoded from base64{}", source(clip)), out, LEAD)],
                    Err(e) => problem(e),
                }
            } else {
                vec![result_row(ctx, &format!("Encoded as base64{} · base64 decode goes back", source(clip)), encode64(text), LEAD)]
            }
        }
        "sha256" | "sha512" | "hash" => {
            let (text, clip) = match input(ctx, typed, &word) {
                Ok(t) => t,
                Err(e) => return problem(e),
            };
            if word == "sha512" {
                vec![result_row(ctx, &format!("SHA-512{}", source(clip)), sha512(text), LEAD)]
            } else {
                // `hash` alone is the usual one, and offers the long one beside it.
                let mut items = vec![result_row(ctx, &format!("SHA-256{}", source(clip)), sha256(text), LEAD)];
                if word == "hash" {
                    items.push(result_row(ctx, &format!("SHA-512{}", source(clip)), sha512(text), LEAD - 0.01));
                }
                items
            }
        }
        "json" => {
            let (min, typed) = match typed.split_once(char::is_whitespace) {
                Some((m, r)) if m.eq_ignore_ascii_case("min") || m.eq_ignore_ascii_case("minify") => (true, r.trim()),
                None if typed.eq_ignore_ascii_case("min") || typed.eq_ignore_ascii_case("minify") => (true, ""),
                _ => (false, typed),
            };
            let (text, clip) = match input(ctx, typed, if min { "json min" } else { "json" }) {
                Ok(t) => t,
                Err(e) => return problem(e),
            };
            match json(text, if min { None } else { Some(2) }) {
                Ok(out) => {
                    let lines = out.lines().count();
                    let what = if min { format!("JSON on one line{}", source(clip)) } else { format!("JSON made readable, {lines} {}{} · json min squeezes it", if lines == 1 { "line" } else { "lines" }, source(clip)) };
                    vec![result_row(ctx, &what, out, LEAD)]
                }
                Err(e) => problem(e),
            }
        }
        "lorem" => {
            let mut parts = typed.split_whitespace();
            let n: Option<usize> = parts.next().and_then(|n| n.parse().ok());
            let words = parts.next().is_some_and(|w| w.eq_ignore_ascii_case("words") || w.eq_ignore_ascii_case("word"));
            let (text, what) = if words {
                let n = n.unwrap_or(20);
                (lorem_words(n), format!("{} filler words", n.clamp(1, MAX_WORDS)))
            } else {
                let n = n.unwrap_or(1);
                let shown = n.clamp(1, MAX_PARAGRAPHS);
                (lorem_paragraphs(n), format!("{shown} {} of filler text · lorem 20 words for words", if shown == 1 { "paragraph" } else { "paragraphs" }))
            };
            vec![result_row(ctx, &what, text, LEAD)]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uuid_is_version_four_and_never_the_same_twice() {
        let a = uuid();
        let parts: Vec<&str> = a.split('-').collect();
        assert_eq!(parts.iter().map(|p| p.len()).collect::<Vec<_>>(), vec![8, 4, 4, 4, 12]);
        assert!(parts[2].starts_with('4'));
        assert!(matches!(parts[3].chars().next(), Some('8' | '9' | 'a' | 'b')));
        assert!(a.chars().all(|c| c == '-' || c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_ne!(a, uuid());
    }

    #[test]
    fn base64_goes_there_and_back() {
        assert_eq!(encode64("hello"), "aGVsbG8=");
        assert_eq!(decode64("aGVsbG8=").unwrap(), "hello");
        // No padding, spaces, and the URL-safe letters all read.
        assert_eq!(decode64("aGVs bG8").unwrap(), "hello");
        assert_eq!(decode64(&encode64("héllo wörld ✓")).unwrap(), "héllo wörld ✓");
        assert_eq!(decode64("_-8").unwrap_err(), "That decodes to bytes that aren't text.");
        assert_eq!(decode64("not base64!").unwrap_err(), "That isn't base64.");
    }

    #[test]
    fn hashes_match_the_known_answers() {
        assert_eq!(sha256("hello"), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
        assert_eq!(sha256(""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert!(sha512("hello").starts_with("9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca7"));
    }

    #[test]
    fn json_is_made_readable_or_squeezed_with_its_order_and_numbers_kept() {
        let src = r#"{"b":1.50,"a":[1,2,{"x":"a, b: {c}"}],"e":{},"f":[],"s":"q\"uote"}"#;
        let pretty = json(src, Some(2)).unwrap();
        assert_eq!(
            pretty,
            "{\n  \"b\": 1.50,\n  \"a\": [\n    1,\n    2,\n    {\n      \"x\": \"a, b: {c}\"\n    }\n  ],\n  \"e\": {},\n  \"f\": [],\n  \"s\": \"q\\\"uote\"\n}"
        );
        // Squeezing it again gives the original back, whitespace aside.
        assert_eq!(json(&pretty, None).unwrap(), src);
        assert_eq!(json("  [ 1 , 2 ]  ", None).unwrap(), "[1,2]");
        assert_eq!(json("[]", Some(2)).unwrap(), "[]");
        assert!(json("{\"a\":}", Some(2)).unwrap_err().starts_with("That isn't valid JSON"));
        assert!(json("hello", None).is_err());
    }

    #[test]
    fn filler_has_the_asked_size_and_a_most() {
        assert!(lorem_paragraphs(1).starts_with("Lorem ipsum dolor sit amet"));
        assert_eq!(lorem_paragraphs(3).split("\n\n").count(), 3);
        assert_eq!(lorem_paragraphs(0).split("\n\n").count(), 1);
        assert_eq!(lorem_paragraphs(999).split("\n\n").count(), MAX_PARAGRAPHS);
        assert_eq!(lorem_words(5), "Lorem ipsum dolor sit amet.");
        assert_eq!(lorem_words(30).split(' ').count(), 30);
        assert!(lorem_words(10_000).split(' ').count() <= MAX_WORDS);
    }

    #[test]
    fn a_word_leads_only_as_a_whole_word() {
        assert!(leads("uuid") && leads("JSON {\"a\":1}") && leads("base64 hello") && leads("  hash x"));
        assert!(!leads("jsonify") && !leads("uuids") && !leads("hashtag") && !leads("10 + 5"));
        assert_eq!(rest("sha256   hello world ", WORDS), Some(("sha256", "hello world")));
    }

    #[test]
    fn rows_copy_on_enter_paste_on_ctrl_enter_and_fall_back_to_what_you_copied() {
        use super::super::search::{tests::ctx, ClipNow};
        let mut c = ctx();
        let r = &rows(&c, "base64 hello")[0];
        assert_eq!(r.title, "aGVsbG8=");
        assert!(matches!(&r.action, Action::Copy(t) if t == "aGVsbG8="));
        assert!(matches!(&r.second, Some((Action::Paste(t, _), "Paste")) if t == "aGVsbG8="));
        assert_eq!(rows(&c, "base64 decode aGVsbG8=")[0].title, "hello");
        assert_eq!(rows(&c, "hash abc").len(), 2);
        assert_eq!(rows(&c, "sha512 abc").len(), 1);
        // With nothing typed and no history, it says what to do; with history it uses the newest text.
        c.live.clips = None;
        assert!(rows(&c, "json")[0].title.contains("clipboard history"));
        c.live.clips = Some(vec![ClipNow { image: Some((1, 2, 2)), ..Default::default() }, ClipNow { text: "{\"a\":[1,2]}".into(), ..Default::default() }]);
        let r = &rows(&c, "json")[0];
        assert!(r.sub.as_deref().unwrap().contains("of what you copied"), "{:?}", r.sub);
        assert!(matches!(&r.action, Action::Copy(t) if t.starts_with("{
  \"a\": [")));
        assert!(matches!(&rows(&c, "json min")[0].action, Action::Copy(t) if t == "{\"a\":[1,2]}"));
        // A mistake says what's wrong and isn't copyable.
        assert!(rows(&c, "json {oops")[0].title.starts_with("That isn't valid JSON"));
        assert!(rows(&c, "base64 decode ***")[0].title == "That isn't base64.");
        assert!(rows(&c, "lorem 2 words")[0].sub.as_deref().unwrap().starts_with("2 filler words"));
        assert!(rows(&c, "something else").is_empty());
    }
}
