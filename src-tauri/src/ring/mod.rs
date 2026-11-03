//! The app ring: a small dial of Kairo's apps and quick actions, as arranged in Kairo's
//! settings, opened at the pointer by its own shortcut and picked by turning or dragging it. This is the part both rings share, the
//! Windows one drawn natively and the web one on macOS and Linux: what's on it, how it turns,
//! what's under a point, and where it goes on the screen.
// The model (Ring, its sizes, the tap and drag rules) is drawn from only by the Windows ring;
// the web ring has its own copy in src/ring-model.ts, with the same tests. Elsewhere it is tests only.
#![cfg_attr(not(windows), allow(dead_code))]

use tauri::AppHandle;

#[cfg(windows)]
pub mod native;
#[cfg(not(windows))]
pub mod web;

/// Opens the ring at the pointer; if it's open, turns it one segment, so holding the modifier and
/// pressing the key again goes round.
pub fn toggle(app: &AppHandle) {
    #[cfg(windows)]
    {
        let _ = app;
        native::open_or_turn();
    }
    #[cfg(not(windows))]
    web::open_or_turn(app);
}

/// What picking a segment does. The ring has closed by then; the segment is remembered.
pub fn perform(app: &AppHandle, conn: &rusqlite::Connection, segment: Segment) -> Result<(), String> {
    let _ = set_last(conn, segment);
    match segment {
        Segment::Chrono => crate::show_main(app),
        Segment::Ledger => crate::show_ledger(app),
        Segment::Palette => crate::palette::toggle(app),
        Segment::Dashboard => crate::show_dashboard(app, None),
        Segment::Task => crate::show_ledger_at(app, "add:task"),
        Segment::Note => {
            // In the small window, ready to type, without bringing up Ledger.
            let note = crate::notebook::create(conn, "", chrono::Utc::now().timestamp_millis()).map_err(|e| e.to_string())?;
            crate::note_window::changed(app, &note.id, "ring", false);
            crate::note_window::pop_out(app, Some(&note.id));
        }
        Segment::Focus => {
            if let Some(why) = focus_refusal(conn) {
                return Err(why);
            }
            if crate::blocking::strict_now(conn).unwrap_or(false) {
                return Err("Strict mode is on: end the current session from Chrono's Focus page first.".into());
            }
            crate::focus::start_by_hand(conn, focus_minutes(conn), None, None, chrono::Utc::now().timestamp_millis()).map_err(|e| e.to_string())?;
        }
        Segment::Notes => crate::note_window::pop_out(app, None),
        Segment::Week => crate::open_main_at(app, "week", None),
        Segment::Settings => crate::show_dashboard(app, Some("settings")),
        Segment::Pause => crate::toggle_paused(app),
    }
    Ok(())
}

/// A refusal from the ring, said as a notification, since the ring has gone.
pub fn tell(app: &AppHandle, why: &str) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title("Kairo").body(why).show();
}

/// The ring's keys as last set, so the shortcut handler can tell them from the palette's.
static KEYS: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

pub fn remember_keys(keys: &str) {
    *KEYS.lock().unwrap_or_else(|e| e.into_inner()) = keys.to_string();
}

/// Whether a global shortcut pressed is the ring's.
pub fn is_ring(pressed: &tauri_plugin_global_shortcut::Shortcut) -> bool {
    same_keys(&KEYS.lock().unwrap_or_else(|e| e.into_inner()), pressed)
}

/// Whether `keys` as saved ("Alt+Q") name the shortcut `pressed`.
pub fn same_keys(keys: &str, pressed: &tauri_plugin_global_shortcut::Shortcut) -> bool {
    keys.parse::<tauri_plugin_global_shortcut::Shortcut>().is_ok_and(|k| &k == pressed)
}

/// Swaps the ring's global shortcut, as the palette's: the palette's own keys are refused, and
/// if the new ones can't be had the old ones stay.
pub fn change_shortcut(app: &AppHandle, conn: &rusqlite::Connection, new: &str) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let old = shortcut(conn).map_err(|e| e.to_string())?;
    let palette = crate::palette::shortcut(conn).map_err(|e| e.to_string())?;
    if new.eq_ignore_ascii_case(&palette) {
        return Err(format!("{new} opens the command palette."));
    }
    if !crate::platform::desktop_ring_shortcut(new) {
        let keys = app.global_shortcut();
        let _ = keys.unregister(old.as_str());
        if let Err(e) = keys.register(new) {
            let _ = keys.register(old.as_str());
            return Err(format!("{new} can't be used: {e}. Another app may already use it."));
        }
    }
    set_shortcut(conn, new).map_err(|e| e.to_string())?;
    remember_keys(new);
    if let Some(item) = tauri::Manager::try_state::<crate::RingItem>(app) {
        let _ = item.0.set_accelerator(Some(new));
    }
    Ok(())
}

/// What can be on the ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Segment {
    Chrono,
    Ledger,
    Palette,
    Dashboard,
    Task,
    Note,
    Focus,
    /// The pop-out's list of notes.
    Notes,
    /// Chrono's Week.
    Week,
    /// Kairo's settings.
    Settings,
    /// Pauses tracking, or resumes it.
    Pause,
}

/// Every item there is, in the order the settings list them.
pub const ALL: [Segment; 11] = [
    Segment::Chrono,
    Segment::Ledger,
    Segment::Palette,
    Segment::Dashboard,
    Segment::Task,
    Segment::Note,
    Segment::Focus,
    Segment::Notes,
    Segment::Week,
    Segment::Settings,
    Segment::Pause,
];

/// What's on the ring until it's arranged, and after Reset.
pub const DEFAULT: [Segment; 7] = [Segment::Chrono, Segment::Ledger, Segment::Palette, Segment::Dashboard, Segment::Task, Segment::Note, Segment::Focus];
pub const MIN_ITEMS: usize = 3;
pub const MAX_ITEMS: usize = 10;

impl Segment {
    /// Its name in settings, as saved.
    pub fn key(self) -> &'static str {
        match self {
            Segment::Chrono => "chrono",
            Segment::Ledger => "ledger",
            Segment::Palette => "palette",
            Segment::Dashboard => "dashboard",
            Segment::Task => "task",
            Segment::Note => "note",
            Segment::Focus => "focus",
            Segment::Notes => "notes",
            Segment::Week => "week",
            Segment::Settings => "settings",
            Segment::Pause => "pause",
        }
    }

    pub fn from_key(key: &str) -> Option<Segment> {
        ALL.into_iter().find(|s| s.key() == key)
    }
}

/// Where the segments drawn sit, as places from the top, in the order each comes into view: the
/// lit one, then right and left alternately. Five show at most (seven while turning, the outer two
/// faded out); with fewer items than that, each shows once.
pub fn shown(count: usize) -> Vec<i32> {
    [0, 1, -1, 2, -2, 3, -3].into_iter().take(count.min(7)).collect()
}

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
    /// What's on it, in order around it: never empty.
    pub items: Vec<Segment>,
    /// The one at the top, by its place in `items`.
    pub lit: usize,
}

impl Ring {
    /// The ring with `items` on it, opened on `last` if that's one of them, otherwise the first.
    pub fn new(items: Vec<Segment>, last: Option<Segment>) -> Self {
        let items = if items.is_empty() { DEFAULT.to_vec() } else { items };
        let lit = last.and_then(|l| items.iter().position(|s| *s == l)).unwrap_or(0);
        Ring { items, lit }
    }

    /// Turns `steps` segments: positive brings the next ones (to the right) to the top.
    pub fn turn(&mut self, steps: i32) {
        self.lit = self.wrap(self.lit as i32 + steps);
    }

    #[cfg(test)]
    pub fn segment(&self) -> Segment {
        self.items[self.lit]
    }

    /// The place in `items` of the one `offset` places from the top (negative to the left).
    pub fn index_at(&self, offset: i32) -> usize {
        self.wrap(self.lit as i32 + offset)
    }

    /// The segment shown `offset` places from the top (negative to the left).
    #[cfg(test)]
    pub fn at(&self, offset: i32) -> Segment {
        self.items[self.index_at(offset)]
    }

    /// The place in `items` of the segment under a point in the window, if one is drawn there:
    /// within the ring's band, and no further round than the two either side of the top, which
    /// is all that shows (fewer, with fewer items).
    pub fn hit(&self, x: f32, y: f32) -> Option<usize> {
        let r = (x - CENTRE.0).hypot(y - CENTRE.1);
        if !(INNER..=OUTER + PROUD).contains(&r) {
            return None;
        }
        let offset = (angle(x, y) / STEP).round() as i32;
        (offset.abs() <= 2 && shown(self.items.len()).contains(&offset)).then(|| self.index_at(offset))
    }

    fn wrap(&self, i: i32) -> usize {
        i.rem_euclid(self.items.len() as i32) as usize
    }
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

/// A segment's name: Focus says its length, Pause what it would do.
pub fn name(segment: Segment, focus_minutes: u32, paused: bool) -> String {
    match segment {
        Segment::Chrono => "Chrono".into(),
        Segment::Ledger => "Ledger".into(),
        Segment::Palette => "Palette".into(),
        Segment::Dashboard => "Dashboard".into(),
        Segment::Task => "New task".into(),
        Segment::Note => "New note".into(),
        Segment::Focus => format!("Focus {focus_minutes}"),
        Segment::Notes => "All notes".into(),
        Segment::Week => "This week".into(),
        Segment::Settings => "Settings".into(),
        Segment::Pause if paused => "Resume tracking".into(),
        Segment::Pause => "Pause tracking".into(),
    }
}

/// Each segment on the ring as it shows when lit: its name, and its line under it.
pub fn faces(conn: &rusqlite::Connection, now: i64, items: &[Segment], paused: bool) -> Vec<(String, String)> {
    let minutes = focus_minutes(conn);
    // Only what the items on it need is read.
    let has = |s: Segment| items.contains(&s);
    let active = if has(Segment::Chrono) {
        let today = chrono::Local::now().date_naive();
        crate::goals::rows_for(conn, today, now).map(|rows| crate::analysis::glance(&rows).active_ms).unwrap_or(0)
    } else {
        0
    };
    let tasks = if has(Segment::Ledger) { crate::tasks::list(conn, now, &crate::tasks::today()).unwrap_or_default() } else { Vec::new() };
    let count = |g: &str| tasks.iter().filter(|t| t.group == g).count();
    let running = has(Segment::Focus) && focus_running(conn);
    items
        .iter()
        .map(|&s| {
            let line = match s {
                Segment::Chrono => chrono_line(active),
                Segment::Ledger => ledger_line(count("today"), count("overdue")),
                Segment::Palette => keys_line(&crate::palette::shortcut(conn).unwrap_or_else(|_| crate::palette::DEFAULT_SHORTCUT.into())),
                Segment::Dashboard | Segment::Settings => "Kairo".into(),
                Segment::Task => "In Ledger".into(),
                Segment::Note | Segment::Notes => "Pop-out".into(),
                Segment::Focus if running => "A session is running".into(),
                Segment::Focus => format!("{minutes} minutes"),
                Segment::Week => "Chrono".into(),
                Segment::Pause if paused => "Paused".into(),
                Segment::Pause => "Tracking".into(),
            };
            (name(s, minutes, paused), line)
        })
        .collect()
}

// ---------- what's on it, as arranged ----------

const ITEMS_KEY: &str = "ring_items";
const FOCUS_KEY: &str = "ring_focus_minutes";
pub const DEFAULT_FOCUS_MINUTES: u32 = 25;

/// Saved keys as the ring's items: unknown and repeated ones dropped, at most `MAX_ITEMS`; too
/// few left (or none saved) is the default.
pub fn parse_items(saved: &str) -> Vec<Segment> {
    let mut items: Vec<Segment> = Vec::new();
    for s in saved.split(',').filter_map(|k| Segment::from_key(k.trim())) {
        if !items.contains(&s) && items.len() < MAX_ITEMS {
            items.push(s);
        }
    }
    if items.len() < MIN_ITEMS {
        DEFAULT.to_vec()
    } else {
        items
    }
}

pub fn items(conn: &rusqlite::Connection) -> Vec<Segment> {
    parse_items(&crate::db::get_setting(conn, ITEMS_KEY).ok().flatten().unwrap_or_default())
}

/// Saves what's on the ring, by key; refused outside 3 to 10 or with an unknown or repeated one.
pub fn set_items(conn: &rusqlite::Connection, keys: &[String]) -> Result<Vec<Segment>, String> {
    let mut items = Vec::new();
    for k in keys {
        let s = Segment::from_key(k).ok_or_else(|| format!("The ring has no item called {k}."))?;
        if items.contains(&s) {
            return Err(format!("{} is on the ring already.", name(s, DEFAULT_FOCUS_MINUTES, false)));
        }
        items.push(s);
    }
    if !(MIN_ITEMS..=MAX_ITEMS).contains(&items.len()) {
        return Err(format!("The ring holds {MIN_ITEMS} to {MAX_ITEMS} items."));
    }
    let saved = items.iter().map(|s| s.key()).collect::<Vec<_>>().join(",");
    crate::db::set_setting(conn, ITEMS_KEY, &saved).map_err(|e| e.to_string())?;
    Ok(items)
}

/// How long Focus's session is: 5 to 240 minutes, 25 unless set.
pub fn focus_minutes(conn: &rusqlite::Connection) -> u32 {
    crate::db::get_setting(conn, FOCUS_KEY).ok().flatten().and_then(|s| s.parse().ok()).filter(|m| (5..=240).contains(m)).unwrap_or(DEFAULT_FOCUS_MINUTES)
}

pub fn set_focus_minutes(conn: &rusqlite::Connection, minutes: u32) -> Result<u32, String> {
    if !(5..=240).contains(&minutes) {
        return Err("A focus session is 5 to 240 minutes.".into());
    }
    crate::db::set_setting(conn, FOCUS_KEY, &minutes.to_string()).map_err(|e| e.to_string())?;
    Ok(minutes)
}

/// The ring as it opens: its items, on the one picked last.
pub fn ring(conn: &rusqlite::Connection) -> Ring {
    Ring::new(items(conn), last(conn))
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

/// The segment picked last, which the ring opens on. Saved by key; before the ring could be
/// arranged it was saved by its place in the default order, which still reads.
pub fn last(conn: &rusqlite::Connection) -> Option<Segment> {
    let saved = crate::db::get_setting(conn, LAST_KEY).ok().flatten()?;
    Segment::from_key(&saved).or_else(|| saved.parse::<usize>().ok().and_then(|i| DEFAULT.get(i).copied()))
}

pub fn set_last(conn: &rusqlite::Connection, segment: Segment) -> rusqlite::Result<()> {
    crate::db::set_setting(conn, LAST_KEY, segment.key())
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

/// What a screen reader hears for the lit segment: its name, then its line.
pub fn spoken(name: &str, line: &str) -> String {
    if line.is_empty() {
        name.to_string()
    } else {
        format!("{name}, {line}")
    }
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
        let all = faces(&c, 0, &ALL, false);
        let face = |s: Segment| all[ALL.iter().position(|x| *x == s).unwrap()].clone();
        let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
        assert_eq!(face(Segment::Dashboard), pair("Dashboard", "Kairo"));
        assert_eq!(face(Segment::Task), pair("New task", "In Ledger"));
        assert_eq!(face(Segment::Note), pair("New note", "Pop-out"));
        assert_eq!(face(Segment::Focus), pair("Focus 25", "25 minutes"));
        assert_eq!(face(Segment::Notes), pair("All notes", "Pop-out"));
        assert_eq!(face(Segment::Week), pair("This week", "Chrono"));
        assert_eq!(face(Segment::Settings), pair("Settings", "Kairo"));
        assert_eq!(face(Segment::Pause), pair("Pause tracking", "Tracking"));
        // Paused, it offers to resume; Focus says the length it's set to.
        set_focus_minutes(&c, 45).unwrap();
        assert_eq!(faces(&c, 0, &[Segment::Pause, Segment::Focus], true), [pair("Resume tracking", "Paused"), pair("Focus 45", "45 minutes")]);
    }

    fn keys(k: &[&str]) -> Vec<String> {
        k.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn what_is_on_it_is_kept_by_key() {
        let c = conn();
        assert_eq!(items(&c), DEFAULT);
        assert_eq!(set_items(&c, &keys(&["notes", "chrono", "pause"])).unwrap(), [Segment::Notes, Segment::Chrono, Segment::Pause]);
        assert_eq!(items(&c), [Segment::Notes, Segment::Chrono, Segment::Pause]);
        // Too few, too many, unknown or repeated are refused, and what was kept stays.
        assert!(set_items(&c, &keys(&["chrono", "ledger"])).is_err());
        let every: Vec<&str> = ALL.iter().map(|s| s.key()).collect();
        assert!(set_items(&c, &keys(&every)).is_err());
        assert!(set_items(&c, &keys(&["chrono", "ledger", "spotify"])).is_err());
        assert!(set_items(&c, &keys(&["chrono", "ledger", "chrono"])).is_err());
        assert_eq!(items(&c).len(), 3);
    }

    #[test]
    fn a_saved_list_is_read_forgivingly() {
        assert_eq!(parse_items(""), DEFAULT);
        assert_eq!(parse_items("week, chrono,spotify,week,ledger"), [Segment::Week, Segment::Chrono, Segment::Ledger]);
        // Fewer than three left is the default.
        assert_eq!(parse_items("week,spotify"), DEFAULT);
        let all = ALL.iter().map(|s| s.key()).collect::<Vec<_>>().join(",");
        assert_eq!(parse_items(&all).len(), MAX_ITEMS);
    }

    #[test]
    fn focus_length_is_kept_within_reason() {
        let c = conn();
        assert_eq!(focus_minutes(&c), 25);
        assert_eq!(set_focus_minutes(&c, 50), Ok(50));
        assert_eq!(focus_minutes(&c), 50);
        assert!(set_focus_minutes(&c, 2).is_err());
        assert!(set_focus_minutes(&c, 600).is_err());
        assert_eq!(focus_minutes(&c), 50);
    }

    #[test]
    fn a_running_session_dims_focus() {
        let c = conn();
        assert!(focus_refusal(&c).is_none());
        crate::focus::start_by_hand(&c, 25, None, None, 0).unwrap();
        assert_eq!(faces(&c, 1, &[Segment::Focus], false)[0].1, "A session is running");
        assert_eq!(focus_refusal(&c).as_deref(), Some("A focus session is already running."));
    }

    #[test]
    fn the_handler_tells_the_rings_keys_from_others() {
        let pressed = |k: &str| k.parse::<tauri_plugin_global_shortcut::Shortcut>().unwrap();
        assert!(same_keys("Alt+Q", &pressed("Alt+Q")));
        assert!(same_keys("alt+q", &pressed("Alt+Q")));
        assert!(!same_keys("Alt+Q", &pressed("Ctrl+Space")));
        assert!(!same_keys("", &pressed("Alt+Q")));
    }

    #[test]
    fn its_shortcut_and_last_segment_are_kept() {
        let c = conn();
        assert_eq!(shortcut(&c).unwrap(), DEFAULT_SHORTCUT);
        set_shortcut(&c, "Ctrl+Alt+R").unwrap();
        assert_eq!(shortcut(&c).unwrap(), "Ctrl+Alt+R");
        assert_eq!(last(&c), None);
        set_last(&c, Segment::Note).unwrap();
        assert_eq!(last(&c), Some(Segment::Note));
        // As saved before the ring could be arranged: its place in the default order.
        crate::db::set_setting(&c, LAST_KEY, "1").unwrap();
        assert_eq!(last(&c), Some(Segment::Ledger));
        // Picked last but since taken off the ring: it opens on the first.
        set_last(&c, Segment::Week).unwrap();
        assert_eq!(ring(&c).segment(), Segment::Chrono);
    }

    /// A point on the ring's middle line at `deg` degrees from straight up.
    fn on_ring(deg: f32) -> (f32, f32) {
        let r = (OUTER + INNER) / 2.0;
        let a = deg.to_radians();
        (CENTRE.0 + r * a.sin(), CENTRE.1 - r * a.cos())
    }

    #[test]
    fn it_opens_on_the_last_one_and_turns_both_ways_round() {
        let mut ring = Ring::new(DEFAULT.to_vec(), Some(Segment::Ledger));
        assert_eq!(ring.segment(), Segment::Ledger);
        ring.turn(1);
        assert_eq!(ring.segment(), Segment::Palette);
        ring.turn(-3);
        assert_eq!(ring.segment(), Segment::Focus);
        ring.turn(8);
        assert_eq!(ring.segment(), Segment::Chrono);
        assert_eq!((ring.at(-1), ring.at(1), ring.at(2)), (Segment::Focus, Segment::Ledger, Segment::Palette));
        // A remembered segment that's no longer there starts at the first.
        assert_eq!(Ring::new(vec![Segment::Week, Segment::Ledger, Segment::Pause], Some(Segment::Chrono)).segment(), Segment::Week);
    }

    #[test]
    fn any_number_of_items_turns_round_and_shows_each_once() {
        let mut ring = Ring::new(vec![Segment::Notes, Segment::Week, Segment::Pause], None);
        ring.turn(4);
        assert_eq!(ring.segment(), Segment::Week);
        ring.turn(-2);
        assert_eq!(ring.segment(), Segment::Pause);
        assert_eq!(shown(3), [0, 1, -1]);
        assert_eq!(shown(4), [0, 1, -1, 2]);
        assert_eq!(shown(5), [0, 1, -1, 2, -2]);
        assert_eq!(shown(10), [0, 1, -1, 2, -2, 3, -3]);
        // Each place shown is a different item.
        for n in MIN_ITEMS..=MAX_ITEMS {
            let ring = Ring::new(ALL[..n].to_vec(), None);
            let mut seen: Vec<_> = shown(n).into_iter().map(|k| ring.index_at(k)).collect();
            seen.sort();
            seen.dedup();
            assert_eq!(seen.len(), n.min(7), "{n}");
        }
    }

    #[test]
    fn a_point_is_over_the_segment_drawn_there() {
        let ring = Ring::new(DEFAULT.to_vec(), None);
        let hit = |deg: f32| {
            let (x, y) = on_ring(deg);
            ring.hit(x, y).map(|i| ring.items[i])
        };
        assert_eq!(hit(0.0), Some(Segment::Chrono));
        assert_eq!(hit(STEP), Some(Segment::Ledger));
        assert_eq!(hit(-STEP), Some(Segment::Focus));
        // Just short of halfway to the next still counts for this one.
        assert_eq!(hit(STEP * 0.45), Some(Segment::Chrono));
        // With three, nothing is drawn two places round.
        let three = Ring::new(vec![Segment::Notes, Segment::Week, Segment::Pause], None);
        let (x, y) = on_ring(STEP * 2.0);
        assert_eq!(three.hit(x, y), None);
        let (x, y) = on_ring(-STEP);
        assert_eq!(three.hit(x, y), Some(2));
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
    fn a_screen_reader_hears_the_lit_ones_name_and_line() {
        assert_eq!(spoken("Ledger", "3 due today"), "Ledger, 3 due today");
        assert_eq!(spoken("Dashboard", ""), "Dashboard");
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
