//! The palette's emoji picker: `:fire` or `emoji fire` lists the emoji that word names, the ones
//! you use most first; `:` or `emoji` alone lists those. Enter pastes one into the window that was
//! in front, Ctrl+Enter copies it. The emoji and their words come from `emoji.txt`, built into
//! Kairo, so nothing is looked up anywhere. No drawing here, so all of it can be tested.

use std::sync::OnceLock;

use super::commands::{row, LEAD};
use super::search::{after, note_row, Action, Context, Glyph, Item};

/// Rows shown at most, so a short word doesn't fill the palette.
const MAX_ROWS: usize = 12;
/// Emoji remembered as used at most.
pub const MAX_USES: usize = 40;

pub struct Emoji {
    pub ch: &'static str,
    pub name: &'static str,
    pub words: &'static str,
}

fn all() -> &'static [Emoji] {
    static ALL: OnceLock<Vec<Emoji>> = OnceLock::new();
    ALL.get_or_init(|| {
        include_str!("emoji.txt")
            .lines()
            .filter(|l| !l.starts_with('#') && !l.is_empty())
            .filter_map(|l| {
                let mut parts = l.splitn(3, '\t');
                Some(Emoji { ch: parts.next()?, name: parts.next()?, words: parts.next().unwrap_or("") })
            })
            .collect()
    })
}

/// What's typed after `:` or `emoji`, maybe nothing; None when the query isn't asking for emoji.
pub fn led_by(query: &str) -> Option<&str> {
    let q = query.trim_start();
    if let Some(rest) = q.strip_prefix(':') {
        // ":" then a word, not a time like ":30" or a smiley typed for its own sake.
        return (rest.is_empty() || rest.starts_with(|c: char| c.is_alphabetic())).then(|| rest.trim());
    }
    if q.trim().eq_ignore_ascii_case("emoji") || q.trim().eq_ignore_ascii_case("emojis") {
        return Some("");
    }
    after(q, "emoji ").map(str::trim)
}

/// How well `wanted` (lower case) names an emoji: a word of its name starting with it best, then
/// one of its other words, then anywhere in its name. None when it doesn't.
fn score(e: &Emoji, wanted: &str) -> Option<f64> {
    let name = e.name.to_lowercase();
    if name == wanted {
        return Some(100.0);
    }
    if name.split([' ', '-', ':']).any(|w| w.starts_with(wanted)) {
        return Some(80.0 - name.len() as f64 / 10.0);
    }
    if e.words.split(' ').any(|w| w == wanted) {
        return Some(60.0);
    }
    if e.words.split(' ').any(|w| w.starts_with(wanted)) {
        return Some(40.0);
    }
    name.contains(wanted).then_some(20.0)
}

/// The emoji's name as a row's title: "Fire".
fn title(e: &Emoji) -> String {
    let mut chars = e.name.chars();
    let name = chars.next().map(|c| c.to_uppercase().chain(chars).collect::<String>()).unwrap_or_default();
    format!("{}  {name}", e.ch)
}

fn emoji_row(ctx: &Context, e: &Emoji, place: f64) -> Item {
    let words: Vec<&str> = e.words.split(' ').filter(|w| !w.is_empty() && !w.starts_with(':')).take(4).collect();
    let mut item = row(title(e), Glyph::Smile, "Emoji", Action::Emoji(e.ch.to_string(), ctx.windows.first().map(|w| w.id.clone())), words.join(", "), (place, Vec::new()));
    item.second = Some((Action::Copy(e.ch.to_string()), "Copy"));
    item
}

/// How often you've used `ch`, from what Kairo remembers.
fn uses(ctx: &Context, ch: &str) -> i64 {
    ctx.emoji_uses.iter().find(|(e, _)| e == ch).map_or(0, |(_, n)| *n)
}

pub fn rows(ctx: &Context, query: &str) -> Vec<Item> {
    let Some(wanted) = led_by(query) else { return Vec::new() };
    let wanted = wanted.to_lowercase();
    if wanted.is_empty() {
        let used: Vec<&Emoji> = ctx.emoji_uses.iter().filter_map(|(ch, _)| all().iter().find(|e| e.ch == ch)).take(MAX_ROWS).collect();
        let mut items: Vec<Item> = used.iter().enumerate().map(|(i, e)| emoji_row(ctx, e, LEAD - i as f64 / 100.0)).collect();
        let mut hint = note_row("Type a word to find an emoji, like :fire or :thumbs", Glyph::Smile, "Emoji");
        hint.score = LEAD - 0.5;
        items.push(hint);
        return items;
    }
    let mut found: Vec<(f64, &Emoji)> = all().iter().filter_map(|e| Some((score(e, &wanted)? + (uses(ctx, e.ch) as f64).min(30.0), e))).collect();
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    if found.is_empty() {
        let mut item = note_row(format!("No emoji for “{wanted}”"), Glyph::Smile, "Emoji");
        item.score = LEAD;
        return vec![item];
    }
    found.into_iter().take(MAX_ROWS).enumerate().map(|(i, (_, e))| emoji_row(ctx, e, LEAD - i as f64 / 100.0)).collect()
}

// ---------- what you use ----------

/// The emoji you've used, most first: (emoji, times).
pub fn used(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare("SELECT emoji, uses FROM emoji_uses ORDER BY uses DESC, last_ms DESC LIMIT ?1")?;
    let rows = stmt.query_map([MAX_USES as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

/// Counts a use; past the most remembered, the least used go.
pub fn note_use(conn: &rusqlite::Connection, ch: &str, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO emoji_uses (emoji, uses, last_ms) VALUES (?1, 1, ?2) ON CONFLICT(emoji) DO UPDATE SET uses = uses + 1, last_ms = ?2",
        rusqlite::params![ch, now],
    )?;
    conn.execute("DELETE FROM emoji_uses WHERE emoji NOT IN (SELECT emoji FROM emoji_uses ORDER BY uses DESC, last_ms DESC LIMIT ?1)", [MAX_USES as i64])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::search::search;
    use super::super::search::tests::ctx;
    use super::*;

    #[test]
    fn the_list_is_built_in_and_whole() {
        assert!(all().len() > 1500);
        assert!(all().iter().all(|e| !e.ch.is_empty() && !e.name.is_empty()));
        assert!(all().iter().any(|e| e.ch == "🔥" && e.name == "fire"));
    }

    #[test]
    fn a_colon_or_the_word_emoji_asks_and_a_time_does_not() {
        assert_eq!(led_by(":fire"), Some("fire"));
        assert_eq!(led_by(":"), Some(""));
        assert_eq!(led_by("emoji thumbs up"), Some("thumbs up"));
        assert_eq!(led_by("Emoji"), Some(""));
        for q in [":30", "fire", "emojify", ":)", "10:30"] {
            assert_eq!(led_by(q), None, "{q}");
        }
    }

    #[test]
    fn a_word_finds_its_emoji_and_enter_pastes_it() {
        let c = ctx();
        let found = search(&c, ":fire");
        assert_eq!(found[0].title, "🔥  Fire");
        assert_eq!(found[0].section, "Emoji");
        assert_eq!(found[0].action, Action::Emoji("🔥".into(), None));
        assert_eq!(found[0].action.verb(), "Paste");
        assert_eq!(found[0].second, Some((Action::Copy("🔥".into()), "Copy")));
        // Other words find it too.
        assert!(search(&c, "emoji flame").iter().take(3).any(|i| i.title == "🔥  Fire"));
        assert!(search(&c, ":thumbs").iter().any(|i| i.title.starts_with("👍")));
        assert!(search(&c, ":qqqzzz")[0].is_note());
        assert!(search(&c, ":smile").iter().filter(|i| i.section == "Emoji").count() <= MAX_ROWS);
    }

    #[test]
    fn the_ones_you_use_come_first() {
        let mut c = ctx();
        assert!(!search(&c, ":heart")[0].title.starts_with("💜"));
        c.emoji_uses = vec![("💜".into(), 12)];
        assert!(search(&c, ":heart")[0].title.starts_with("💜"));
        // Alone, the colon lists what you use.
        assert!(search(&c, ":")[0].title.starts_with("💜"));
    }

    #[test]
    fn uses_are_counted_and_kept_to_the_most() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        note_use(&conn, "🔥", 1).unwrap();
        note_use(&conn, "👍", 2).unwrap();
        note_use(&conn, "👍", 3).unwrap();
        assert_eq!(used(&conn).unwrap(), vec![("👍".to_string(), 2), ("🔥".to_string(), 1)]);
        for (i, e) in all().iter().take(MAX_USES + 5).enumerate() {
            note_use(&conn, e.ch, 10 + i as i64).unwrap();
        }
        assert_eq!(used(&conn).unwrap().len(), MAX_USES);
    }
}
