//! Moving the window that was in front before the palette opened: halves, thirds, two-thirds,
//! quarters, maximize, center, the
//! next display and back as it was; and layouts, the places of the open windows saved by a name
//! and put back later, also when a focus session with the layout's label starts. Where a window
//! goes is worked out here as plain sums, so it can be tested; the system moves it
//! (`platform::place_window`).

use std::collections::HashMap;
use std::sync::Mutex;

use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};

use crate::platform::{self, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Snap {
    LeftHalf,
    RightHalf,
    TopHalf,
    BottomHalf,
    LeftThird,
    CenterThird,
    RightThird,
    LeftTwoThirds,
    RightTwoThirds,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Maximize,
    Center,
    NextDisplay,
    /// Back where it was before Kairo first moved it.
    Restore,
}

impl Snap {
    pub const ALL: [Snap; 17] = [
        Snap::LeftHalf,
        Snap::RightHalf,
        Snap::TopHalf,
        Snap::BottomHalf,
        Snap::LeftThird,
        Snap::CenterThird,
        Snap::RightThird,
        Snap::LeftTwoThirds,
        Snap::RightTwoThirds,
        Snap::TopLeft,
        Snap::TopRight,
        Snap::BottomLeft,
        Snap::BottomRight,
        Snap::Maximize,
        Snap::Center,
        Snap::NextDisplay,
        Snap::Restore,
    ];

    /// The row's title, which is also what's typed for it.
    pub fn title(self) -> &'static str {
        match self {
            Snap::LeftHalf => "Left half",
            Snap::RightHalf => "Right half",
            Snap::TopHalf => "Top half",
            Snap::BottomHalf => "Bottom half",
            Snap::LeftThird => "Left third",
            Snap::CenterThird => "Center third",
            Snap::RightThird => "Right third",
            Snap::LeftTwoThirds => "Left two thirds",
            Snap::RightTwoThirds => "Right two thirds",
            Snap::TopLeft => "Top left quarter",
            Snap::TopRight => "Top right quarter",
            Snap::BottomLeft => "Bottom left quarter",
            Snap::BottomRight => "Bottom right quarter",
            Snap::Maximize => "Maximize",
            Snap::Center => "Center",
            Snap::NextDisplay => "Next display",
            Snap::Restore => "Restore window",
        }
    }

    /// Other words it's found by.
    pub fn words(self) -> &'static str {
        match self {
            Snap::LeftHalf => "snap window move left side",
            Snap::RightHalf => "snap window move right side",
            Snap::TopHalf => "snap window move top up",
            Snap::BottomHalf => "snap window move bottom down",
            Snap::LeftThird => "snap window move left side 1/3",
            Snap::CenterThird => "snap window move middle centre 1/3",
            Snap::RightThird => "snap window move right side 1/3",
            Snap::LeftTwoThirds => "snap window move left side 2/3 bigger",
            Snap::RightTwoThirds => "snap window move right side 2/3 bigger",
            Snap::TopLeft => "snap window move corner upper 1/4",
            Snap::TopRight => "snap window move corner upper 1/4",
            Snap::BottomLeft => "snap window move corner lower 1/4",
            Snap::BottomRight => "snap window move corner lower 1/4",
            Snap::Maximize => "snap window full fill maximise bigger",
            Snap::Center => "snap window middle centre",
            Snap::NextDisplay => "snap window monitor screen other display move",
            Snap::Restore => "snap window undo back unsnap original size",
        }
    }

    /// Words typed outright that put the row first.
    pub fn exact(self) -> &'static [&'static str] {
        match self {
            Snap::LeftHalf => &["left", "left half"],
            Snap::RightHalf => &["right", "right half"],
            Snap::TopHalf => &["top half"],
            Snap::BottomHalf => &["bottom half"],
            Snap::LeftThird => &["left third"],
            Snap::CenterThird => &["center third", "centre third", "middle third"],
            Snap::RightThird => &["right third"],
            Snap::LeftTwoThirds => &["left two thirds", "left 2/3"],
            Snap::RightTwoThirds => &["right two thirds", "right 2/3"],
            Snap::TopLeft => &["top left", "top left quarter"],
            Snap::TopRight => &["top right", "top right quarter"],
            Snap::BottomLeft => &["bottom left", "bottom left quarter"],
            Snap::BottomRight => &["bottom right", "bottom right quarter"],
            Snap::Maximize => &["maximize", "maximise", "max"],
            Snap::Center => &["center", "centre"],
            Snap::NextDisplay => &["next display", "next monitor", "next screen"],
            Snap::Restore => &["restore", "unsnap"],
        }
    }
}

/// The display a window is mostly on, by its place in `areas`; the first when it is on none.
pub fn display_of(window: &Rect, areas: &[Rect]) -> usize {
    areas
        .iter()
        .enumerate()
        .map(|(i, a)| (i, window.intersect(a).map_or(0, |r| r.area())))
        .max_by_key(|&(i, overlap)| (overlap, std::cmp::Reverse(i)))
        .filter(|&(_, overlap)| overlap > 0)
        .map_or(0, |(i, _)| i)
}

/// `size` made to fit inside `within`, and its corner moved so all of it is inside.
fn fit(r: Rect, within: &Rect) -> Rect {
    let w = r.w.min(within.w);
    let h = r.h.min(within.h);
    let x = r.x.clamp(within.x, within.x + within.w - w);
    let y = r.y.clamp(within.y, within.y + within.h - h);
    Rect { x, y, w, h }
}

/// Columns `from..to` of `cols` and rows `top..bottom` of `rows` of the work area `a`, with edges
/// worked out from the area's own, so neighbouring parts always meet.
fn part(a: Rect, (from, to, cols): (i32, i32, i32), (top, bottom, rows): (i32, i32, i32)) -> Rect {
    let x = |i: i32| a.x + (a.w as i64 * i as i64 / cols as i64) as i32;
    let y = |i: i32| a.y + (a.h as i64 * i as i64 / rows as i64) as i32;
    Rect { x: x(from), y: y(top), w: x(to) - x(from), h: y(bottom) - y(top) }
}

/// Where a window goes for `snap`, given where it is and the displays' work areas; None for
/// Restore (which goes where it was) or with no display.
pub fn target(snap: Snap, window: Rect, areas: &[Rect]) -> Option<Rect> {
    let a = *areas.get(display_of(&window, areas))?;
    let (half_w, half_h) = (a.w / 2, a.h / 2);
    Some(match snap {
        Snap::LeftHalf => Rect { w: half_w, ..a },
        Snap::RightHalf => Rect { x: a.x + half_w, w: a.w - half_w, ..a },
        Snap::TopHalf => Rect { h: half_h, ..a },
        Snap::BottomHalf => Rect { y: a.y + half_h, h: a.h - half_h, ..a },
        Snap::LeftThird => part(a, (0, 1, 3), (0, 1, 1)),
        Snap::CenterThird => part(a, (1, 2, 3), (0, 1, 1)),
        Snap::RightThird => part(a, (2, 3, 3), (0, 1, 1)),
        Snap::LeftTwoThirds => part(a, (0, 2, 3), (0, 1, 1)),
        Snap::RightTwoThirds => part(a, (1, 3, 3), (0, 1, 1)),
        Snap::TopLeft => part(a, (0, 1, 2), (0, 1, 2)),
        Snap::TopRight => part(a, (1, 2, 2), (0, 1, 2)),
        Snap::BottomLeft => part(a, (0, 1, 2), (1, 2, 2)),
        Snap::BottomRight => part(a, (1, 2, 2), (1, 2, 2)),
        Snap::Maximize => a,
        Snap::Center => {
            let w = window.w.min(a.w);
            let h = window.h.min(a.h);
            Rect { x: a.x + (a.w - w) / 2, y: a.y + (a.h - h) / 2, w, h }
        }
        // To the display to the right, around to the first: the same place in it, scaled.
        Snap::NextDisplay => {
            let mut order: Vec<usize> = (0..areas.len()).collect();
            order.sort_by_key(|&i| (areas[i].x, areas[i].y));
            let here = display_of(&window, areas);
            let at = order.iter().position(|&i| i == here).unwrap_or(0);
            let b = areas[order[(at + 1) % order.len()]];
            let scale = |v: i32, from: i32, to: i32| (v as i64 * to as i64 / from.max(1) as i64) as i32;
            let moved = Rect { x: b.x + scale(window.x - a.x, a.w, b.w), y: b.y + scale(window.y - a.y, a.h, b.h), w: scale(window.w, a.w, b.w), h: scale(window.h, a.h, b.h) };
            fit(moved, &b)
        }
        Snap::Restore => return None,
    })
}

/// Where each window was before Kairo first moved it, by its id, for Restore. Kept while Kairo
/// runs: a window id means nothing after it closes.
static BEFORE: Mutex<Option<HashMap<String, Rect>>> = Mutex::new(None);

fn remember(id: &str, was: Rect) {
    let mut before = BEFORE.lock().unwrap_or_else(|e| e.into_inner());
    before.get_or_insert_with(HashMap::new).entry(id.to_string()).or_insert(was);
}

fn forget(id: &str) -> Option<Rect> {
    BEFORE.lock().unwrap_or_else(|e| e.into_inner()).as_mut()?.remove(id)
}

/// The displays' work areas. AppKit only answers on its main thread, so a Mac asks there.
fn areas(app: &tauri::AppHandle) -> Vec<Rect> {
    #[cfg(target_os = "macos")]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        if app.run_on_main_thread(move || { let _ = tx.send(platform::work_areas()); }).is_ok() {
            return rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap_or_default();
        }
        Vec::new()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        platform::work_areas()
    }
}

/// Moves window `id`. The inner text is for the user.
pub fn apply(app: &tauri::AppHandle, id: &str, snap: Snap) -> std::result::Result<(), String> {
    if let Some(why) = platform::cant_move_windows() {
        return Err(why.into());
    }
    let now = platform::window_rect(id).ok_or("That window can't be moved, or it closed.")?;
    if snap == Snap::Restore {
        let was = forget(id).ok_or("Kairo hasn't moved that window, so there's nothing to restore.")?;
        return platform::place_window(id, was).then_some(()).ok_or_else(|| "That window didn't let Kairo move it.".into());
    }
    remember(id, now);
    if snap == Snap::Maximize && platform::maximize_window(id) {
        return Ok(());
    }
    let to = target(snap, now, &areas(app)).ok_or("Kairo couldn't find your display.")?;
    platform::place_window(id, to).then_some(()).ok_or_else(|| "That window didn't let Kairo move it. It may be running as administrator.".into())
}

// ---------- layouts ----------

/// Windows a layout keeps at most.
const MAX_LAYOUT_WINDOWS: usize = 30;
pub const MAX_LAYOUTS: usize = 20;
const MAX_NAME_CHARS: usize = 40;

/// One window in a layout: its app, its title then, and where it was.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Placed {
    pub app: String,
    pub title: String,
    pub rect: Rect,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub name: String,
    pub windows: Vec<Placed>,
    /// A focus session with this label puts the layout back as it starts.
    pub focus_label: Option<String>,
}

pub fn list(conn: &Connection) -> Result<Vec<Layout>> {
    let mut stmt = conn.prepare("SELECT name, windows, focus_label FROM window_layouts ORDER BY position, rowid")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)))?.collect::<Result<Vec<_>>>()?;
    Ok(rows.into_iter().map(|(name, windows, focus_label)| Layout { name, windows: serde_json::from_str(&windows).unwrap_or_default(), focus_label }).collect())
}

pub fn find(conn: &Connection, name: &str) -> Result<Option<Layout>> {
    Ok(list(conn)?.into_iter().find(|l| l.name.eq_ignore_ascii_case(name.trim())))
}

/// Saves where `windows` are as layout `name`, over one of that name. The inner error is for the
/// user.
pub fn save(conn: &Connection, name: &str, windows: Vec<Placed>) -> Result<std::result::Result<usize, String>> {
    let name: String = name.trim().chars().take(MAX_NAME_CHARS).collect();
    if name.is_empty() {
        return Ok(Err("Give the layout a name, like “save layout writing”.".into()));
    }
    if windows.is_empty() {
        return Ok(Err("There are no windows to save.".into()));
    }
    let windows: Vec<Placed> = windows.into_iter().take(MAX_LAYOUT_WINDOWS).collect();
    let count = windows.len();
    let json = serde_json::to_string(&windows).unwrap_or_else(|_| "[]".into());
    let existing = find(conn, &name)?;
    match existing {
        Some(was) => {
            conn.execute("UPDATE window_layouts SET windows = ?1 WHERE name = ?2", params![json, was.name])?;
        }
        None => {
            if list(conn)?.len() >= MAX_LAYOUTS {
                return Ok(Err(format!("That's {MAX_LAYOUTS} layouts, the most there can be. Remove one in Kairo's settings.")));
            }
            conn.execute(
                "INSERT INTO window_layouts (name, windows, position) VALUES (?1, ?2, (SELECT IFNULL(MAX(position), 0) + 1 FROM window_layouts))",
                params![name, json],
            )?;
        }
    }
    Ok(Ok(count))
}

/// Which focus label puts a layout back; None or blank for none. One label belongs to one layout.
pub fn set_focus_label(conn: &Connection, name: &str, label: Option<&str>) -> Result<()> {
    let label = label.map(str::trim).filter(|l| !l.is_empty());
    if let Some(l) = label {
        conn.execute("UPDATE window_layouts SET focus_label = NULL WHERE focus_label = ?1 COLLATE NOCASE", [l])?;
    }
    conn.execute("UPDATE window_layouts SET focus_label = ?1 WHERE name = ?2 COLLATE NOCASE", params![label, name])?;
    Ok(())
}

pub fn remove(conn: &Connection, name: &str) -> Result<()> {
    conn.execute("DELETE FROM window_layouts WHERE name = ?1 COLLATE NOCASE", [name])?;
    Ok(())
}

/// The layout a focus session labelled `label` puts back.
pub fn for_focus_label(conn: &Connection, label: &str) -> Result<Option<Layout>> {
    let l = label.trim();
    Ok(list(conn)?.into_iter().find(|x| x.focus_label.as_deref().is_some_and(|f| f.eq_ignore_ascii_case(l))))
}

/// Which open window each saved one goes to: the same app, its own title first, each open
/// window used once. `open` is (id, app, title).
pub fn matched<'a>(saved: &'a [Placed], open: &[(String, String, String)]) -> Vec<(String, &'a Placed)> {
    let mut used = vec![false; open.len()];
    let mut out = Vec::new();
    // Titles first, so a window that's still the same document gets its own place.
    let mut pending: Vec<&Placed> = Vec::new();
    for p in saved {
        match open.iter().enumerate().position(|(i, (_, app, title))| !used[i] && app.eq_ignore_ascii_case(&p.app) && *title == p.title) {
            Some(i) => {
                used[i] = true;
                out.push((open[i].0.clone(), p));
            }
            None => pending.push(p),
        }
    }
    for p in pending {
        if let Some(i) = open.iter().enumerate().position(|(i, (_, app, _))| !used[i] && app.eq_ignore_ascii_case(&p.app)) {
            used[i] = true;
            out.push((open[i].0.clone(), p));
        }
    }
    out
}

/// The open windows with their places, as a layout would keep them.
pub fn current() -> Vec<Placed> {
    platform::switchable(platform::open_windows(), std::process::id())
        .into_iter()
        .filter_map(|w| Some(Placed { app: w.path.clone()?, title: w.title.clone(), rect: platform::window_rect(&w.id)? }))
        .collect()
}

/// Puts a layout's windows back; how many moved. Apps not open are left.
pub fn restore(layout: &Layout) -> std::result::Result<usize, String> {
    if let Some(why) = platform::cant_move_windows() {
        return Err(why.into());
    }
    let open: Vec<(String, String, String)> = platform::switchable(platform::open_windows(), std::process::id()).into_iter().filter_map(|w| Some((w.id, w.path?, w.title))).collect();
    let moved = matched(&layout.windows, &open).into_iter().filter(|(id, p)| platform::place_window(id, p.rect)).count();
    Ok(moved)
}

/// Puts back the layout for a focus session's label, if one has it, off the caller's thread.
pub fn on_focus_start(conn: &Connection, label: Option<&str>) {
    let Some(layout) = label.and_then(|l| for_focus_label(conn, l).ok().flatten()) else { return };
    std::thread::spawn(move || {
        let _ = restore(&layout);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    const ONE: Rect = Rect { x: 0, y: 0, w: 1920, h: 1040 };
    const TWO: Rect = Rect { x: 1920, y: 0, w: 2560, h: 1400 };

    #[test]
    fn halves_split_the_display_the_window_is_mostly_on() {
        let w = Rect { x: 2000, y: 100, w: 800, h: 600 };
        assert_eq!(target(Snap::LeftHalf, w, &[ONE, TWO]), Some(Rect { x: 1920, y: 0, w: 1280, h: 1400 }));
        assert_eq!(target(Snap::RightHalf, w, &[ONE, TWO]), Some(Rect { x: 3200, y: 0, w: 1280, h: 1400 }));
        // An odd width gives the extra pixel to the right half, so the two meet.
        let odd = Rect { x: 0, y: 0, w: 1921, h: 1041 };
        let l = target(Snap::LeftHalf, odd, &[odd]).unwrap();
        let r = target(Snap::RightHalf, odd, &[odd]).unwrap();
        assert_eq!((l.x + l.w, r.x + r.w), (r.x, 1921));
        let t = target(Snap::TopHalf, odd, &[odd]).unwrap();
        let b = target(Snap::BottomHalf, odd, &[odd]).unwrap();
        assert_eq!((t.h, b.y, b.h), (520, 520, 521));
    }

    #[test]
    fn thirds_and_quarters_meet_and_fill_the_display() {
        let odd = Rect { x: 100, y: 0, w: 1921, h: 1041 };
        let [l, c, r] = [Snap::LeftThird, Snap::CenterThird, Snap::RightThird].map(|s| target(s, odd, &[odd]).unwrap());
        assert_eq!((l.x, l.x + l.w, c.x + c.w, r.x + r.w), (100, c.x, r.x, 2021));
        assert!([l.w, c.w, r.w].iter().all(|w| (640..=641).contains(w)));
        assert_eq!(target(Snap::LeftTwoThirds, odd, &[odd]).unwrap(), Rect { w: c.x + c.w - 100, ..odd });
        assert_eq!(target(Snap::RightTwoThirds, odd, &[odd]).unwrap(), Rect { x: c.x, w: 2021 - c.x, ..odd });
        let tl = target(Snap::TopLeft, odd, &[odd]).unwrap();
        let br = target(Snap::BottomRight, odd, &[odd]).unwrap();
        assert_eq!((tl.x, tl.y, tl.x + tl.w, tl.y + tl.h), (100, 0, br.x, br.y));
        assert_eq!((br.x + br.w, br.y + br.h), (2021, 1041));
        assert_eq!(target(Snap::TopRight, odd, &[odd]).unwrap(), Rect { x: br.x, y: 0, w: br.w, h: tl.h });
        assert_eq!(target(Snap::BottomLeft, odd, &[odd]).unwrap(), Rect { x: 100, y: br.y, w: tl.w, h: br.h });
    }

    #[test]
    fn center_keeps_the_size_and_maximize_fills_the_work_area() {
        let w = Rect { x: 10, y: 10, w: 800, h: 600 };
        assert_eq!(target(Snap::Center, w, &[ONE]), Some(Rect { x: 560, y: 220, w: 800, h: 600 }));
        let huge = Rect { x: 0, y: 0, w: 5000, h: 5000 };
        assert_eq!(target(Snap::Center, huge, &[ONE]), Some(ONE));
        assert_eq!(target(Snap::Maximize, w, &[ONE, TWO]), Some(ONE));
    }

    #[test]
    fn next_display_keeps_the_place_scaled_and_comes_back_around() {
        let w = Rect { x: 960, y: 520, w: 960, h: 520 };
        let there = target(Snap::NextDisplay, w, &[TWO, ONE]).unwrap();
        assert_eq!(there, Rect { x: 1920 + 1280, y: 700, w: 1280, h: 700 });
        let back = target(Snap::NextDisplay, there, &[TWO, ONE]).unwrap();
        assert_eq!(back, w);
        // One display: it stays where it is.
        assert_eq!(target(Snap::NextDisplay, w, &[ONE]), Some(w));
    }

    #[test]
    fn a_window_off_every_display_counts_as_on_the_first() {
        let lost = Rect { x: -5000, y: -5000, w: 100, h: 100 };
        assert_eq!(display_of(&lost, &[ONE, TWO]), 0);
        assert_eq!(target(Snap::LeftHalf, lost, &[]), None);
        assert_eq!(target(Snap::Restore, lost, &[ONE]), None);
    }

    #[test]
    fn restore_goes_back_to_before_the_first_move() {
        remember("w1", Rect { x: 1, y: 2, w: 3, h: 4 });
        remember("w1", Rect { x: 9, y: 9, w: 9, h: 9 });
        assert_eq!(forget("w1"), Some(Rect { x: 1, y: 2, w: 3, h: 4 }));
        assert_eq!(forget("w1"), None);
    }

    fn placed(app: &str, title: &str, x: i32) -> Placed {
        Placed { app: app.into(), title: title.into(), rect: Rect { x, y: 0, w: 100, h: 100 } }
    }

    #[test]
    fn layouts_match_windows_by_app_and_title_each_used_once() {
        let saved = vec![placed("code.exe", "notes.md", 0), placed("code.exe", "main.rs", 100), placed("chrome.exe", "Docs", 200), placed("slack.exe", "Slack", 300)];
        let open = vec![
            ("a".to_string(), "code.exe".to_string(), "main.rs".to_string()),
            ("b".to_string(), "Code.exe".to_string(), "other.rs".to_string()),
            ("c".to_string(), "chrome.exe".to_string(), "Mail".to_string()),
        ];
        let m: Vec<(String, i32)> = matched(&saved, &open).into_iter().map(|(id, p)| (id, p.rect.x)).collect();
        // main.rs keeps its own place; notes.md takes the other Code window; Slack isn't open.
        assert_eq!(m, vec![("a".into(), 100), ("b".into(), 0), ("c".into(), 200)]);
    }

    #[test]
    fn layouts_are_saved_by_name_replaced_and_found_by_focus_label() {
        let c = db::open(std::path::Path::new(":memory:")).unwrap();
        assert!(save(&c, " ", vec![placed("a", "a", 0)]).unwrap().is_err());
        assert!(save(&c, "Writing", vec![]).unwrap().is_err());
        assert_eq!(save(&c, "Writing", vec![placed("a", "a", 0)]).unwrap(), Ok(1));
        assert_eq!(save(&c, "writing", vec![placed("a", "a", 5), placed("b", "b", 6)]).unwrap(), Ok(2));
        let all = list(&c).unwrap();
        assert_eq!((all.len(), all[0].name.as_str(), all[0].windows.len()), (1, "Writing", 2));
        save(&c, "Code", vec![placed("a", "a", 0)]).unwrap().unwrap();
        set_focus_label(&c, "writing", Some("essay")).unwrap();
        assert_eq!(for_focus_label(&c, "ESSAY").unwrap().unwrap().name, "Writing");
        // A label moves to the layout it's given to last.
        set_focus_label(&c, "code", Some("Essay")).unwrap();
        assert_eq!(for_focus_label(&c, "essay").unwrap().unwrap().name, "Code");
        assert_eq!(find(&c, "writing").unwrap().unwrap().focus_label, None);
        remove(&c, "CODE").unwrap();
        assert!(for_focus_label(&c, "essay").unwrap().is_none());
    }
}
