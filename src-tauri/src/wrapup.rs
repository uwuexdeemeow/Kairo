//! A focus session's wrap-up, shown on the Focus page once it ends: how long it ran against its
//! plan, how often it nudged, what distracting things were in front while it ran, and how it
//! compares with the sessions before it.

use rusqlite::{Connection, Result};
use serde::Serialize;

use crate::analysis::{self, Category, Row};
use crate::focus::{self, Block};
use crate::goals::fmt_dur;
use crate::db;

const MIN_MS: i64 = 60_000;
/// The card stays up this long after its session ended, unless dismissed or another starts.
const SHOWN_FOR_MS: i64 = 12 * 60 * MIN_MS;
/// The session whose card was dismissed, by when it started.
const DISMISSED_KEY: &str = "focus_wrapup_dismissed";
/// "Your usual" is the average over this many sessions before it, given at least `USUAL_MIN`.
const USUAL_SESSIONS: usize = 20;
const USUAL_MIN: usize = 3;
/// What pulled you away: this many at most, each in front for at least this long.
const MAX_PULLS: usize = 3;
const MIN_PULL_MS: i64 = 30_000;
/// Within this of the usual length, a session is "about" it.
const SAME_LENGTH_MS: i64 = 5 * MIN_MS;
/// Off the usual number of nudges by more than this, it's fewer or more.
const NUDGE_GAP: f64 = 0.5;

/// Something distracting that was in front during a session, and for how long.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Pull {
    pub label: String,
    pub ms: i64,
}

/// A pomodoro cycle in total, added to the wrap-up of its last round.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CycleSummary {
    pub rounds: usize,
    /// Rounds that were ended early.
    pub early: usize,
    pub focused_ms: i64,
    /// Time between the rounds.
    pub break_ms: i64,
    pub nudges: i64,
    pub pulled: Vec<Pull>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WrapUp {
    pub start: i64,
    pub end: i64,
    pub planned_ms: i64,
    /// Time focused: the session less its pauses.
    pub focused_ms: i64,
    /// Whether it ran its course rather than being ended early.
    pub completed: bool,
    pub label: Option<String>,
    pub nudges: i64,
    /// Biggest first.
    pub pulled: Vec<Pull>,
    /// Lines comparing it with the sessions before it; none with too few to compare with.
    pub versus: Vec<String>,
    /// The whole cycle, when the session was the last round of one with at least two rounds.
    pub cycle: Option<CycleSummary>,
}

/// A cycle's rounds in total; None for fewer than two, since a single round says it all. Breaks
/// are the gaps between rounds.
fn cycle_summary(rounds: &[Block], pulled: Vec<Pull>) -> Option<CycleSummary> {
    if rounds.len() < 2 {
        return None;
    }
    Some(CycleSummary {
        rounds: rounds.len(),
        early: rounds.iter().filter(|b| !b.completed()).count(),
        focused_ms: rounds.iter().map(Block::ms).sum(),
        break_ms: rounds.windows(2).map(|w| (w[1].start() - w[0].end()).max(0)).sum(),
        nudges: rounds.iter().map(|b| b.nudges().unwrap_or(0)).sum(),
        pulled,
    })
}

/// The distracting apps and sites among `rows`, by time in front, biggest first.
fn pulled(rows: &[Row]) -> Vec<Pull> {
    let mut pulls: Vec<Pull> = Vec::new();
    for r in rows.iter().filter(|r| !r.idle && r.category == Category::Distracting) {
        match pulls.iter_mut().find(|p| p.label == r.label()) {
            Some(p) => p.ms += r.dur(),
            None => pulls.push(Pull { label: r.label().to_string(), ms: r.dur() }),
        }
    }
    pulls.retain(|p| p.ms >= MIN_PULL_MS);
    pulls.sort_by_key(|p| std::cmp::Reverse(p.ms));
    pulls.truncate(MAX_PULLS);
    pulls
}

/// How `this` session compares with those `before` it: its length, and its nudges against the
/// sessions whose nudges were counted.
fn versus(this: &Block, before: &[Block]) -> Vec<String> {
    let mut lines = Vec::new();
    if before.len() >= USUAL_MIN {
        let usual = before.iter().map(Block::ms).sum::<i64>() / before.len() as i64;
        let diff = this.ms() - usual;
        lines.push(match diff {
            d if d.abs() < SAME_LENGTH_MS => format!("About your usual {}", fmt_dur(usual)),
            d if d > 0 => format!("{} longer than your usual {}", fmt_dur(d), fmt_dur(usual)),
            d => format!("{} shorter than your usual {}", fmt_dur(-d), fmt_dur(usual)),
        });
    }
    let counted: Vec<i64> = before.iter().filter_map(Block::nudges).collect();
    if let (Some(nudges), true) = (this.nudges(), counted.len() >= USUAL_MIN) {
        let usual = counted.iter().sum::<i64>() as f64 / counted.len() as f64;
        if (nudges as f64) < usual - NUDGE_GAP {
            lines.push("Fewer nudges than usual".into());
        } else if nudges as f64 > usual + NUDGE_GAP {
            lines.push("More nudges than usual".into());
        }
    }
    lines
}

/// The wrap-up for the last session that ended. None while a session runs, once its card was
/// dismissed, or long after it ended.
pub fn latest(conn: &Connection, now: i64) -> Result<Option<WrapUp>> {
    // A cycle on a break is still going: its wrap-up waits for it to end.
    if focus::active(conn)?.is_some() || crate::pomodoro::load(conn)?.is_some() {
        return Ok(None);
    }
    let ended = focus::ended(conn, USUAL_SESSIONS + 1)?;
    let Some((this, before)) = ended.split_first() else { return Ok(None) };
    if now - this.end() > SHOWN_FOR_MS || db::get_setting(conn, DISMISSED_KEY)?.as_deref() == Some(this.start().to_string().as_str()) {
        return Ok(None);
    }
    let cycle = match this.cycle() {
        Some(id) => {
            let rounds = focus::cycle_blocks(conn, id)?;
            let mut rows = Vec::new();
            for b in &rounds {
                rows.extend(analysis::load_rows(conn, b.start(), b.end())?);
            }
            cycle_summary(&rounds, pulled(&rows))
        }
        None => None,
    };
    Ok(Some(WrapUp {
        start: this.start(),
        end: this.end(),
        planned_ms: this.planned_ms(),
        focused_ms: this.ms(),
        completed: this.completed(),
        label: this.label().map(str::to_string),
        nudges: this.nudges().unwrap_or(0),
        pulled: pulled(&analysis::load_rows(conn, this.start(), this.end())?),
        versus: versus(this, before),
        cycle,
    }))
}

/// Puts the latest session's card away for good.
pub fn dismiss(conn: &Connection) -> Result<()> {
    match focus::ended(conn, 1)?.first() {
        Some(last) => db::set_setting(conn, DISMISSED_KEY, &last.start().to_string()),
        None => Ok(()),
    }
}

pub fn nudges_said(nudges: i64) -> String {
    match nudges {
        0 => "No nudges".into(),
        1 => "1 nudge".into(),
        n => format!("{n} nudges"),
    }
}

/// The wrap-up in a line, for the notification that a session is done.
pub fn line(w: &WrapUp) -> String {
    const WHERE: &str = "The wrap-up is on Chrono's Focus page.";
    match w.pulled.first() {
        Some(top) => format!("{} · {} {}. {WHERE}", nudges_said(w.nudges), top.label, fmt_dur(top.ms)),
        None if w.nudges > 0 => format!("{}. {WHERE}", nudges_said(w.nudges)),
        None => "No nudges and nothing distracting. Nice work.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: i64 = MIN_MS;
    const T: i64 = 100_000 * M;

    fn memory() -> Connection {
        db::open(std::path::Path::new(":memory:")).unwrap()
    }

    fn row(app: &str, site: Option<&str>, category: Category, seconds: i64) -> Row {
        Row { app: app.into(), exe: app.to_lowercase(), site: site.map(Into::into), start: 0, end: seconds * 1000, idle: false, category, app_category: category }
    }

    /// A session of `minutes` that ended on its own, starting at `from`.
    fn session(conn: &Connection, from: i64, minutes: i64) {
        focus::start(conn, minutes as u32, None, from).unwrap();
        focus::stop(conn, from + minutes * M).unwrap();
    }

    fn used(conn: &Connection, app: &str, exe: &str, site: Option<&str>, from: i64, minutes: i64) {
        let s = db::NewSession { app, exe, title: "", site, start_ms: from, end_ms: from + minutes * M, idle: false, demo: false };
        db::insert_session(conn, &s).unwrap();
    }

    #[test]
    fn what_pulled_you_away_is_grouped_biggest_first_and_cut_off() {
        use Category::*;
        let rows = vec![
            row("Brave", Some("youtube"), Distracting, 120),
            row("Code", None, Productive, 900),
            row("Discord", None, Distracting, 200),
            row("Brave", Some("youtube"), Distracting, 100),
            row("Steam", None, Distracting, 20),
            row("Brave", Some("reddit"), Distracting, 60),
            row("Brave", Some("twitch"), Distracting, 45),
            Row { idle: true, ..row("Discord", None, Distracting, 5000) },
        ];
        let got: Vec<(String, i64)> = pulled(&rows).into_iter().map(|p| (p.label, p.ms / 1000)).collect();
        assert_eq!(got, [("youtube".to_string(), 220), ("Discord".to_string(), 200), ("reddit".to_string(), 60)]);
        assert!(pulled(&rows[1..2]).is_empty());
    }

    #[test]
    fn a_session_is_compared_with_the_ones_before_it() {
        let conn = memory();
        for i in 0..3 {
            session(&conn, T + i * 100 * M, 40);
        }
        let compare = |minutes: i64| {
            session(&conn, T + 1000 * M, minutes);
            let all = focus::ended(&conn, 4).unwrap();
            let lines = versus(&all[0], &all[1..]);
            conn.execute("DELETE FROM focus_sessions WHERE start_ms = ?1", [T + 1000 * M]).unwrap();
            lines
        };
        assert_eq!(compare(52), ["12m longer than your usual 40m"]);
        assert_eq!(compare(25), ["15m shorter than your usual 40m"]);
        assert_eq!(compare(43), ["About your usual 40m"]);
        // With too few before it there's nothing to compare with.
        let all = focus::ended(&conn, 3).unwrap();
        assert!(versus(&all[0], &all[1..]).is_empty());
    }

    #[test]
    fn nudges_are_compared_only_with_sessions_that_counted_them() {
        let conn = memory();
        // Three sessions from before nudges were counted, then three that had two each.
        for i in 0..6 {
            session(&conn, T + i * 100 * M, 40);
        }
        conn.execute("UPDATE focus_sessions SET nudges = NULL WHERE start_ms < ?1", [T + 300 * M]).unwrap();
        conn.execute("UPDATE focus_sessions SET nudges = 2 WHERE nudges IS NOT NULL", []).unwrap();
        let with = |nudges: i64| {
            session(&conn, T + 1000 * M, 40);
            conn.execute("UPDATE focus_sessions SET nudges = ?1 WHERE start_ms = ?2", [nudges, T + 1000 * M]).unwrap();
            let all = focus::ended(&conn, 7).unwrap();
            let lines = versus(&all[0], &all[1..]);
            conn.execute("DELETE FROM focus_sessions WHERE start_ms = ?1", [T + 1000 * M]).unwrap();
            lines
        };
        assert_eq!(with(0), ["About your usual 40m", "Fewer nudges than usual"]);
        assert_eq!(with(2), ["About your usual 40m"]);
        assert_eq!(with(4), ["About your usual 40m", "More nudges than usual"]);
        // Only two that counted: nothing is said about nudges.
        conn.execute("UPDATE focus_sessions SET nudges = NULL WHERE start_ms = ?1", [T + 300 * M]).unwrap();
        assert_eq!(with(0), ["About your usual 40m"]);
    }

    #[test]
    fn the_latest_session_has_a_wrap_up_until_it_is_dismissed_or_another_starts() {
        let conn = memory();
        assert_eq!(latest(&conn, T).unwrap(), None);
        analysis::set_category(&conn, "site:youtube", Category::Distracting).unwrap();
        focus::start(&conn, 25, Some("essay"), T).unwrap();
        used(&conn, "Brave", "brave", Some("youtube"), T + 5 * M, 3);
        used(&conn, "Brave", "brave", Some("youtube"), T + 60 * M, 30);
        // Still running: no wrap-up yet.
        assert_eq!(latest(&conn, T + 10 * M).unwrap(), None);
        focus::stop(&conn, T + 23 * M).unwrap();
        let w = latest(&conn, T + 24 * M).unwrap().expect("a wrap-up");
        assert_eq!((w.focused_ms, w.planned_ms, w.completed, w.label.as_deref(), w.nudges), (23 * M, 25 * M, false, Some("essay"), 0));
        // Only what was in front while it ran.
        assert_eq!(w.pulled, [Pull { label: "youtube".into(), ms: 3 * M }]);
        assert!(w.versus.is_empty());
        // Another session takes its place, and has its own once it ends.
        focus::start(&conn, 25, None, T + 30 * M).unwrap();
        assert_eq!(latest(&conn, T + 31 * M).unwrap(), None);
        focus::stop(&conn, T + 40 * M).unwrap();
        assert_eq!(latest(&conn, T + 41 * M).unwrap().unwrap().start, T + 30 * M);
        // Half a day on, it's old news.
        assert_eq!(latest(&conn, T + 40 * M + SHOWN_FOR_MS + 1).unwrap(), None);
        dismiss(&conn).unwrap();
        assert_eq!(latest(&conn, T + 41 * M).unwrap(), None);
        // A later session isn't dismissed with it.
        session(&conn, T + 50 * M, 10);
        assert!(latest(&conn, T + 61 * M).unwrap().is_some());
    }

    #[test]
    fn a_cycle_adds_up_its_rounds_and_waits_for_it_to_end() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        // Two rounds with a 5-minute break between them; the second is ended early.
        crate::pomodoro::start(&conn, Some(25), Some(5), None, None, T).unwrap();
        focus::complete_if_due(&conn, T + 25 * M).unwrap();
        // While the cycle is on, even on a break, there's no wrap-up.
        crate::pomodoro::tick(&conn, T + 25 * M, 0).unwrap();
        assert!(latest(&conn, T + 26 * M).unwrap().is_none());
        crate::pomodoro::tick(&conn, T + 30 * M, 0).unwrap();
        focus::count_nudge(&conn).unwrap();
        focus::stop(&conn, T + 40 * M).unwrap();
        let w = latest(&conn, T + 41 * M).unwrap().unwrap();
        let cycle = w.cycle.unwrap();
        assert_eq!((cycle.rounds, cycle.early, cycle.focused_ms, cycle.break_ms, cycle.nudges), (2, 1, 35 * M, 5 * M, 1));
        // The card itself is still the last round's.
        assert_eq!((w.start, w.focused_ms, w.completed), (T + 30 * M, 10 * M, false));
    }

    #[test]
    fn a_single_round_or_a_hand_started_session_has_no_cycle_part() {
        let conn = db::open(std::path::Path::new(":memory:")).unwrap();
        crate::pomodoro::start(&conn, None, None, None, None, T).unwrap();
        focus::stop(&conn, T + 10 * M).unwrap();
        assert_eq!(latest(&conn, T + 11 * M).unwrap().unwrap().cycle, None);
        focus::start(&conn, 25, None, T + 20 * M).unwrap();
        focus::stop(&conn, T + 30 * M).unwrap();
        assert_eq!(latest(&conn, T + 31 * M).unwrap().unwrap().cycle, None);
    }

    #[test]
    fn the_notification_says_it_in_a_line() {
        let base = WrapUp { start: 0, end: 25 * M, planned_ms: 25 * M, focused_ms: 25 * M, completed: true, label: None, nudges: 0, pulled: Vec::new(), versus: Vec::new(), cycle: None };
        assert_eq!(line(&base), "No nudges and nothing distracting. Nice work.");
        assert_eq!(line(&WrapUp { nudges: 1, ..base.clone() }), "1 nudge. The wrap-up is on Chrono's Focus page.");
        let pulled = vec![Pull { label: "YouTube".into(), ms: 3 * M }, Pull { label: "Discord".into(), ms: M }];
        assert_eq!(line(&WrapUp { nudges: 2, pulled, ..base }), "2 nudges · YouTube 3m. The wrap-up is on Chrono's Focus page.");
    }
}
