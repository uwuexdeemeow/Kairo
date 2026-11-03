//! The app ring: a small dial of Kairo's apps and three quick actions, opened at the pointer by
//! its own shortcut and picked by turning or dragging it. This is the part both rings share, the
//! Windows one drawn natively and the web one on macOS and Linux: what's on it, how it turns,
//! what's under a point, and where it goes on the screen.

/// What's on the ring, in order around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Segment {
    Chrono,
    Ledger,
    Palette,
    Dashboard,
    Task,
    Note,
    Focus,
}

pub const SEGMENTS: [Segment; 7] = [Segment::Chrono, Segment::Ledger, Segment::Palette, Segment::Dashboard, Segment::Task, Segment::Note, Segment::Focus];

/// The window, in pixels.
pub const WIDTH: f32 = 300.0;
pub const HEIGHT: f32 = 210.0;
/// The ring is an arc of a circle whose centre is below the window.
pub const CENTRE: (f32, f32) = (WIDTH / 2.0, 360.0);
pub const OUTER: f32 = 330.0;
pub const INNER: f32 = 230.0;
/// How far the lit segment stands out.
pub const PROUD: f32 = 12.0;
/// Degrees each segment takes around the ring.
pub const STEP: f32 = 21.0;
/// A modifier let go sooner than this after opening was a tap: the ring stays open.
pub const TAP_MS: i64 = 200;
/// A press that moves less than this is a click, not a drag.
pub const DRAG_PX: f32 = 4.0;

/// What letting go of the shortcut's modifier means.
#[derive(Debug, PartialEq, Eq)]
pub enum Release {
    /// Open the lit segment.
    Pick,
    /// It was a tap: stay open for keys, scrolling or the mouse.
    Stay,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ring {
    /// The segment at the top.
    pub lit: usize,
}

impl Ring {
    pub fn new(last: usize) -> Self {
        Ring { lit: if last < SEGMENTS.len() { last } else { 0 } }
    }

    /// Turns `steps` segments: positive brings the next ones (to the right) to the top.
    pub fn turn(&mut self, steps: i32) {
        self.lit = wrap(self.lit as i32 + steps);
    }

    pub fn segment(&self) -> Segment {
        SEGMENTS[self.lit]
    }

    /// The segment shown `offset` places from the top (negative to the left).
    pub fn at(&self, offset: i32) -> Segment {
        SEGMENTS[wrap(self.lit as i32 + offset)]
    }

    /// The segment under a point in the window, if any is drawn there: within the ring's band,
    /// and no further round than the two either side of the top, which is all that shows.
    pub fn hit(&self, x: f32, y: f32) -> Option<Segment> {
        let r = (x - CENTRE.0).hypot(y - CENTRE.1);
        if !(INNER..=OUTER + PROUD).contains(&r) {
            return None;
        }
        let offset = (angle(x, y) / STEP).round() as i32;
        (offset.abs() <= 2).then(|| self.at(offset))
    }
}

fn wrap(i: i32) -> usize {
    i.rem_euclid(SEGMENTS.len() as i32) as usize
}

// ---------- what each segment says, and settings ----------

/// Alt+Q; Option+Q on a Mac, where Alt is Option.
pub const DEFAULT_SHORTCUT: &str = "Alt+Q";
const SHORTCUT_KEY: &str = "ring_shortcut";
const LAST_KEY: &str = "ring_last";

/// Chrono's line: today's time on screen.
pub fn chrono_line(active_ms: i64) -> String {
    if active_ms <= 0 {
        "Nothing yet today".into()
    } else {
        format!("{} today", crate::palette::search::fmt_dur(active_ms))
    }
}

/// Ledger's line, as on the dashboard.
pub fn ledger_line(due: usize, late: usize) -> String {
    let parts: Vec<String> = [(due, "due today"), (late, "overdue")].iter().filter(|(n, _)| *n > 0).map(|(n, what)| format!("{n} {what}")).collect();
    if parts.is_empty() {
        "Nothing due today".into()
    } else {
        parts.join(" · ")
    }
}

/// A shortcut as the palette's segment shows it: "Ctrl + Space".
pub fn keys_line(shortcut: &str) -> String {
    shortcut.split('+').collect::<Vec<_>>().join(" + ")
}

/// The line under each segment, in ring order.
pub fn lines(conn: &rusqlite::Connection, now: i64) -> [String; 7] {
    let today = chrono::Local::now().date_naive();
    let active = crate::goals::rows_for(conn, today, now).map(|rows| crate::analysis::glance(&rows).active_ms).unwrap_or(0);
    let tasks = crate::tasks::list(conn, now, &crate::tasks::today()).unwrap_or_default();
    let count = |g: &str| tasks.iter().filter(|t| t.group == g).count();
    let palette = crate::palette::shortcut(conn).unwrap_or_else(|_| crate::palette::DEFAULT_SHORTCUT.into());
    [
        chrono_line(active),
        ledger_line(count("today"), count("overdue")),
        keys_line(&palette),
        "Kairo".into(),
        "In Ledger".into(),
        "In Ledger".into(),
        if focus_running(conn) { "A session is running".into() } else { "25 minutes".into() },
    ]
}

fn focus_running(conn: &rusqlite::Connection) -> bool {
    crate::focus::active(conn).ok().flatten().is_some()
}

/// Why Focus 25 can't start now, if it can't.
pub fn focus_refusal(conn: &rusqlite::Connection) -> Option<String> {
    if focus_running(conn) {
        return Some("A focus session is already running.".into());
    }
    None
}

pub fn shortcut(conn: &rusqlite::Connection) -> rusqlite::Result<String> {
    Ok(crate::db::get_setting(conn, SHORTCUT_KEY)?.filter(|s| !s.is_empty()).unwrap_or_else(|| DEFAULT_SHORTCUT.into()))
}

pub fn set_shortcut(conn: &rusqlite::Connection, keys: &str) -> rusqlite::Result<()> {
    crate::db::set_setting(conn, SHORTCUT_KEY, keys)
}

/// The segment picked last, which the ring opens on.
pub fn last(conn: &rusqlite::Connection) -> usize {
    crate::db::get_setting(conn, LAST_KEY).ok().flatten().and_then(|s| s.parse().ok()).filter(|i| *i < SEGMENTS.len()).unwrap_or(0)
}

pub fn set_last(conn: &rusqlite::Connection, segment: Segment) -> rusqlite::Result<()> {
    let at = SEGMENTS.iter().position(|s| *s == segment).unwrap_or(0);
    crate::db::set_setting(conn, LAST_KEY, &at.to_string())
}

/// The angle of a point about the ring's centre, in degrees clockwise from straight up.
pub fn angle(x: f32, y: f32) -> f32 {
    (x - CENTRE.0).atan2(CENTRE.1 - y).to_degrees()
}

/// How many segments a drag from one angle to another turns the ring, snapped: pulling it
/// leftward (anticlockwise) brings the next ones up.
pub fn drag_steps(from: f32, to: f32) -> i32 {
    ((from - to) / STEP).round() as i32
}

pub fn on_modifier_up(elapsed_ms: i64) -> Release {
    if elapsed_ms < TAP_MS {
        Release::Stay
    } else {
        Release::Pick
    }
}

/// Where the window goes: centred on the pointer, moved fully into the work area (left, top,
/// right, bottom); an area too small for it gets it at its top left.
pub fn place(cursor: (i32, i32), area: (i32, i32, i32, i32), size: (i32, i32)) -> (i32, i32) {
    let (l, t, r, b) = area;
    let fit = |want: i32, lo: i32, hi: i32| if hi < lo { lo } else { want.clamp(lo, hi) };
    (fit(cursor.0 - size.0 / 2, l, r - size.0), fit(cursor.1 - size.1 / 2, t, b - size.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> rusqlite::Connection {
        crate::db::open(std::path::Path::new(":memory:")).unwrap()
    }

    #[test]
    fn each_segment_says_how_things_stand() {
        assert_eq!(chrono_line(0), "Nothing yet today");
        assert_eq!(chrono_line(125 * 60_000), "2h 05m today");
        assert_eq!(ledger_line(3, 1), "3 due today · 1 overdue");
        assert_eq!(ledger_line(0, 1), "1 overdue");
        assert_eq!(ledger_line(2, 0), "2 due today");
        assert_eq!(ledger_line(0, 0), "Nothing due today");
        assert_eq!(keys_line("Ctrl+Space"), "Ctrl + Space");
        let c = conn();
        let lines = lines(&c, 0);
        assert_eq!(lines[3], "Kairo");
        assert_eq!((lines[4].as_str(), lines[5].as_str(), lines[6].as_str()), ("In Ledger", "In Ledger", "25 minutes"));
    }

    #[test]
    fn a_running_session_dims_focus() {
        let c = conn();
        assert!(focus_refusal(&c).is_none());
        crate::focus::start_by_hand(&c, 25, None, None, 0).unwrap();
        assert_eq!(lines(&c, 1)[6], "A session is running");
        assert_eq!(focus_refusal(&c).as_deref(), Some("A focus session is already running."));
    }

    #[test]
    fn its_shortcut_and_last_segment_are_kept() {
        let c = conn();
        assert_eq!(shortcut(&c).unwrap(), DEFAULT_SHORTCUT);
        set_shortcut(&c, "Ctrl+Alt+R").unwrap();
        assert_eq!(shortcut(&c).unwrap(), "Ctrl+Alt+R");
        assert_eq!(last(&c), 0);
        set_last(&c, Segment::Note).unwrap();
        assert_eq!(last(&c), 5);
    }

    /// A point on the ring's middle line at `deg` degrees from straight up.
    fn on_ring(deg: f32) -> (f32, f32) {
        let r = (OUTER + INNER) / 2.0;
        let a = deg.to_radians();
        (CENTRE.0 + r * a.sin(), CENTRE.1 - r * a.cos())
    }

    #[test]
    fn it_opens_on_the_last_one_and_turns_both_ways_round() {
        let mut ring = Ring::new(1);
        assert_eq!(ring.segment(), Segment::Ledger);
        ring.turn(1);
        assert_eq!(ring.segment(), Segment::Palette);
        ring.turn(-3);
        assert_eq!(ring.segment(), Segment::Focus);
        ring.turn(8);
        assert_eq!(ring.segment(), Segment::Chrono);
        assert_eq!((ring.at(-1), ring.at(1), ring.at(2)), (Segment::Focus, Segment::Ledger, Segment::Palette));
        // A remembered segment that's no longer there starts at the first.
        assert_eq!(Ring::new(40).segment(), Segment::Chrono);
    }

    #[test]
    fn a_point_is_over_the_segment_drawn_there() {
        let ring = Ring::new(0);
        let (x, y) = on_ring(0.0);
        assert_eq!(ring.hit(x, y), Some(Segment::Chrono));
        let (x, y) = on_ring(STEP);
        assert_eq!(ring.hit(x, y), Some(Segment::Ledger));
        let (x, y) = on_ring(-STEP);
        assert_eq!(ring.hit(x, y), Some(Segment::Focus));
        // Just short of halfway to the next still counts for this one.
        let (x, y) = on_ring(STEP * 0.45);
        assert_eq!(ring.hit(x, y), Some(Segment::Chrono));
        // Inside the ring, or above it, is nothing.
        assert_eq!(ring.hit(CENTRE.0, CENTRE.1 - INNER + 20.0), None);
        assert_eq!(ring.hit(CENTRE.0, 1.0), None);
    }

    #[test]
    fn angles_run_clockwise_from_the_top() {
        assert!((angle(CENTRE.0, 0.0) - 0.0).abs() < 0.01);
        assert!((angle(CENTRE.0 + 100.0, CENTRE.1) - 90.0).abs() < 0.01);
        assert!((angle(CENTRE.0 - 100.0, CENTRE.1) + 90.0).abs() < 0.01);
    }

    #[test]
    fn dragging_snaps_to_segments() {
        // Pulling the ring leftward brings the next one up.
        assert_eq!(drag_steps(STEP, 0.0), 1);
        assert_eq!(drag_steps(0.0, STEP * 2.0), -2);
        assert_eq!(drag_steps(0.0, STEP * 0.4), 0);
        assert_eq!(drag_steps(0.0, -STEP * 0.6), 1);
    }

    #[test]
    fn a_quick_release_was_a_tap() {
        assert_eq!(on_modifier_up(150), Release::Stay);
        assert_eq!(on_modifier_up(TAP_MS), Release::Pick);
        assert_eq!(on_modifier_up(250), Release::Pick);
    }

    #[test]
    fn it_opens_round_the_pointer_and_stays_on_screen() {
        let screen = (0, 0, 1920, 1040);
        let size = (300, 210);
        assert_eq!(place((960, 520), screen, size), (810, 415));
        // Corners and edges.
        assert_eq!(place((5, 5), screen, size), (0, 0));
        assert_eq!(place((1915, 1035), screen, size), (1620, 830));
        assert_eq!(place((960, 1035), screen, size), (810, 830));
        // A second screen to the left, above the first's top.
        assert_eq!(place((-1900, -300), (-1920, -400, 0, 680), size), (-1920, -400));
        // A screen too small for it: its top left.
        assert_eq!(place((50, 50), (0, 0, 200, 100), size), (0, 0));
    }
}
