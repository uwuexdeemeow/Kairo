//! Synthetic history so the dashboard and insights can be tried before weeks of real data exist.
//! Demo rows are flagged and can be removed without touching real data.

use chrono::{Datelike, Duration, Local, NaiveTime, Weekday};
use rusqlite::{Connection, Result};

use crate::db::{self, NewSession};

struct Activity {
    app: &'static str,
    exe: &'static str,
    site: Option<&'static str>,
    title: &'static str,
    min_s: i64,
    max_s: i64,
}

const fn act(app: &'static str, exe: &'static str, site: Option<&'static str>, title: &'static str, min_s: i64, max_s: i64) -> Activity {
    Activity { app, exe, site, title, min_s, max_s }
}

const CODE: Activity = act("VS Code", "code", None, "analysis.rs - kairo - Visual Studio Code", 600, 2400);
const TERMINAL: Activity = act("Windows Terminal", "windowsterminal", None, "PowerShell", 60, 300);
const GITHUB: Activity = act("Chrome", "chrome", Some("github"), "Pull requests · GitHub - Google Chrome", 60, 600);
const DOCS: Activity = act("Chrome", "chrome", Some("docs.rs"), "rusqlite - Rust - docs.rs - Google Chrome", 60, 600);
const SLACK: Activity = act("Slack", "slack", None, "general - Slack", 30, 300);
const OUTLOOK: Activity = act("Outlook", "outlook", None, "Inbox - Outlook", 60, 400);
const YOUTUBE: Activity = act("Chrome", "chrome", Some("youtube"), "Video - YouTube - Google Chrome", 120, 1500);
const REDDIT: Activity = act("Chrome", "chrome", Some("reddit"), "r/programming - Reddit - Google Chrome", 60, 900);
const DISCORD: Activity = act("Discord", "discord", None, "Discord", 60, 900);

/// (activity, weight) per part of the day. The planted patterns: sharp mornings,
/// fragmented mid-afternoons, and focus collapsing late at night.
type Profile = &'static [(&'static Activity, u32)];

const MORNING: Profile = &[(&CODE, 50), (&TERMINAL, 10), (&GITHUB, 10), (&DOCS, 10), (&SLACK, 10), (&OUTLOOK, 7), (&YOUTUBE, 3)];
const AFTERNOON: Profile = &[(&CODE, 30), (&GITHUB, 8), (&DOCS, 8), (&SLACK, 25), (&OUTLOOK, 17), (&YOUTUBE, 7), (&REDDIT, 5)];
const EVENING: Profile = &[(&CODE, 30), (&YOUTUBE, 25), (&REDDIT, 15), (&DISCORD, 10), (&DOCS, 10), (&SLACK, 10)];
const LATE: Profile = &[(&YOUTUBE, 40), (&REDDIT, 30), (&DISCORD, 20), (&CODE, 10)];

/// Tiny deterministic PRNG; the demo doesn't need a crate.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
    fn chance(&mut self, p: f64) -> bool {
        (self.next() % 10_000) as f64 / 10_000.0 < p
    }
    fn pick<'a>(&mut self, options: &[(&'a Activity, u32)]) -> &'a Activity {
        let total: u32 = options.iter().map(|(_, w)| w).sum();
        let mut roll = (self.next() % total as u64) as u32;
        for (a, w) in options {
            if roll < *w {
                return a;
            }
            roll -= w;
        }
        options[0].0
    }
}

pub fn seed(conn: &mut Connection, days: i64) -> Result<usize> {
    clear(conn)?;
    let tx = conn.transaction()?;
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let now = chrono::Utc::now().timestamp_millis();
    let today = Local::now().date_naive();
    let mut count = 0;

    for back in (1..=days).rev() {
        let date = today - Duration::days(back);
        let weekend = matches!(date.weekday(), Weekday::Sat | Weekday::Sun);
        let at = |h: u32, m: u32| {
            date.and_time(NaiveTime::from_hms_opt(h, m, 0).unwrap())
                .and_local_timezone(Local)
                .earliest()
                .map(|t| t.timestamp_millis())
        };

        let mut parts: Vec<(i64, i64, Profile)> = Vec::new();
        let mut push = |from: Option<i64>, to: Option<i64>, profile| {
            if let (Some(from), Some(to)) = (from, to) {
                parts.push((from, to, profile));
            }
        };
        if !weekend {
            push(at(9, rng.range(0, 20) as u32), at(12, 0), MORNING);
            push(at(13, rng.range(0, 15) as u32), at(17, 30), AFTERNOON);
        }
        push(at(19, rng.range(0, 30) as u32), at(22, 0), EVENING);
        // Late night runs past midnight, so it ends at "24:xx" on this date.
        push(at(22, 0), at(23, 59).map(|t| t + rng.range(10, 70) * 60_000), LATE);

        for (from, to, profile) in parts {
            let mut t = from;
            let mut prev_exe = "";
            while t < to.min(now) {
                // Breaks away from the keyboard.
                if rng.chance(0.08) {
                    let end = (t + rng.range(3, 15) * 60_000).min(to);
                    insert(&tx, &IDLE, t, end, true)?;
                    count += 1;
                    t = end;
                    prev_exe = "";
                    continue;
                }
                // Planted trigger: Slack often leads straight into YouTube.
                let a = if prev_exe == "slack" && rng.chance(0.45) { &YOUTUBE } else { rng.pick(profile) };
                let end = (t + rng.range(a.min_s, a.max_s) * 1000).min(to);
                insert(&tx, a, t, end, false)?;
                count += 1;
                prev_exe = a.exe;
                t = end;
            }
        }
    }
    tx.commit()?;
    Ok(count)
}

const IDLE: Activity = act("Idle", "", None, "", 0, 0);

fn insert(conn: &Connection, a: &Activity, start_ms: i64, end_ms: i64, idle: bool) -> Result<()> {
    db::insert_session(
        conn,
        &NewSession { app: a.app, exe: a.exe, title: a.title, site: a.site, start_ms, end_ms, idle, demo: true },
    )
    .map(|_| ())
}

pub fn clear(conn: &Connection) -> Result<usize> {
    conn.execute("DELETE FROM sessions WHERE demo = 1", [])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis;

    #[test]
    fn demo_history_surfaces_planted_patterns() {
        let mut conn = db::open(std::path::Path::new(":memory:")).unwrap();
        assert!(seed(&mut conn, 21).unwrap() > 500);

        let now = chrono::Utc::now().timestamp_millis();
        let rows = analysis::load_rows(&conn, now - 30 * 86_400_000, now).unwrap();
        let p = serde_json::to_value(analysis::patterns(&rows)).unwrap();
        let titles: Vec<&str> = p["insights"].as_array().unwrap().iter().map(|i| i["title"].as_str().unwrap()).collect();
        println!("{titles:#?}");

        assert!(titles.iter().any(|t| t.contains("after 10pm")), "{titles:?}");
        assert!(titles.iter().any(|t| t.starts_with("Your sharpest hours")), "{titles:?}");
        assert!(titles.iter().any(|t| t.contains("right after Slack")), "{titles:?}");

        assert!(clear(&conn).unwrap() > 0);
    }
}
