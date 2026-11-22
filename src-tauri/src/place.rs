//! Placing the window you were in from the palette: the left or right half of the screen, the top
//! or bottom, the whole of it, its middle, or the next screen. The arithmetic is plain and the same
//! everywhere; moving a window is the system's, and only Windows has it so far (see [`can`]).

/// A rectangle in screen pixels: its top left corner and its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Where a window goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Left,
    Right,
    Top,
    Bottom,
    /// Fills the screen's usable area.
    Maximize,
    /// The middle of the screen at two thirds of its size.
    Center,
    /// The same place and size, relatively, on the next screen.
    NextScreen,
}

impl Place {
    pub const ALL: [Place; 7] = [Place::Left, Place::Right, Place::Top, Place::Bottom, Place::Maximize, Place::Center, Place::NextScreen];

    /// The row's title.
    pub fn title(self) -> &'static str {
        match self {
            Place::Left => "Left half",
            Place::Right => "Right half",
            Place::Top => "Top half",
            Place::Bottom => "Bottom half",
            Place::Maximize => "Maximize",
            Place::Center => "Center",
            Place::NextScreen => "Next screen",
        }
    }

    /// What typed outright brings the row to the top.
    pub fn words(self) -> &'static [&'static str] {
        match self {
            Place::Left => &["left", "left half"],
            Place::Right => &["right", "right half"],
            Place::Top => &["top", "top half"],
            Place::Bottom => &["bottom", "bottom half"],
            Place::Maximize => &["maximize", "max", "full"],
            Place::Center => &["center", "centre"],
            Place::NextScreen => &["next screen", "next monitor", "other screen"],
        }
    }

    /// Other words that find it among the actions.
    pub fn also(self) -> &'static str {
        "window snap tile move screen monitor"
    }
}

/// The part of `work` (a screen's usable area) the window takes; None for the next screen, which
/// needs the other screen as well (see [`carry`]). Halves split the odd pixel to the second half, so
/// two halves always fill the screen.
pub fn rect_for(work: Rect, place: Place) -> Option<Rect> {
    let (hw, hh) = (work.w / 2, work.h / 2);
    Some(match place {
        Place::Left => Rect { w: hw, ..work },
        Place::Right => Rect { x: work.x + hw, w: work.w - hw, ..work },
        Place::Top => Rect { h: hh, ..work },
        Place::Bottom => Rect { y: work.y + hh, h: work.h - hh, ..work },
        Place::Maximize => work,
        Place::Center => {
            let (w, h) = (work.w * 2 / 3, work.h * 2 / 3);
            Rect { x: work.x + (work.w - w) / 2, y: work.y + (work.h - h) / 2, w, h }
        }
        Place::NextScreen => return None,
    })
}

/// A window moved from one screen's usable area to another's, keeping where it was and how much of
/// the screen it filled.
pub fn carry(from: Rect, to: Rect, window: Rect) -> Rect {
    let scale = |v: i32, a: i32, b: i32| if a == 0 { 0 } else { (v as i64 * b as i64 / a as i64) as i32 };
    Rect { x: to.x + scale(window.x - from.x, from.w, to.w), y: to.y + scale(window.y - from.y, from.h, to.h), w: scale(window.w, from.w, to.w).max(1), h: scale(window.h, from.h, to.h).max(1) }
}

/// The screen after this one, going left to right (then top to bottom), wrapping round; None with
/// only one. `areas` are the screens' usable areas and `at` the one the window is on.
pub fn next_screen(areas: &[Rect], at: usize) -> Option<usize> {
    (areas.len() > 1).then(|| (at + 1) % areas.len())
}

/// Whether this system lets Kairo move another app's window.
pub fn can() -> bool {
    cfg!(windows)
}

/// Why it can't, for a system that doesn't allow it.
#[cfg_attr(windows, allow(dead_code))]
pub const CANT: &str = "Kairo can't move windows on this system yet.";

/// Moves the window with this id (from the palette's list of open windows) to `place`.
#[cfg(windows)]
pub fn apply(id: &str, place: Place) -> Result<(), String> {
    windows::apply(id, place)
}

#[cfg(not(windows))]
pub fn apply(_id: &str, _place: Place) -> Result<(), String> {
    Err(CANT.into())
}

#[cfg(windows)]
mod windows {
    use super::{carry, next_screen, rect_for, Place, Rect};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsIconic, IsZoomed, SetWindowPos, ShowWindow, SWP_NOACTIVATE, SWP_NOZORDER, SW_MAXIMIZE, SW_RESTORE};

    const REFUSED: &str = "Windows won't let Kairo move that one: it runs with more rights.";

    fn rect(r: RECT) -> Rect {
        Rect { x: r.left, y: r.top, w: r.right - r.left, h: r.bottom - r.top }
    }

    /// The usable area (the screen less the taskbar) of a monitor.
    unsafe fn work_area(monitor: HMONITOR) -> Option<Rect> {
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        (GetMonitorInfoW(monitor, &mut info) != 0).then(|| rect(info.rcWork))
    }

    unsafe extern "system" fn collect(monitor: HMONITOR, _dc: HDC, _clip: *mut RECT, into: LPARAM) -> i32 {
        let list = &mut *(into as *mut Vec<Rect>);
        if let Some(area) = work_area(monitor) {
            list.push(area);
        }
        1
    }

    /// Every screen's usable area, left to right and then top to bottom.
    unsafe fn screens() -> Vec<Rect> {
        let mut list: Vec<Rect> = Vec::new();
        EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(collect), &mut list as *mut Vec<Rect> as LPARAM);
        list.sort_by_key(|r| (r.x, r.y));
        list
    }

    pub fn apply(id: &str, place: Place) -> Result<(), String> {
        let hwnd = id.parse::<usize>().map_err(|_| "That window is gone.".to_string())? as HWND;
        // SAFETY: plain Win32 calls on a window handle Windows validates, and on buffers on this stack.
        unsafe {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            let here = work_area(monitor).ok_or("Couldn't read the screen.")?;
            // A maximized or minimized window is put back first, or it would ignore a new size.
            if IsIconic(hwnd) != 0 || (IsZoomed(hwnd) != 0 && place != Place::Maximize) {
                ShowWindow(hwnd, SW_RESTORE);
            }
            if place == Place::Maximize {
                ShowWindow(hwnd, SW_MAXIMIZE);
                return Ok(());
            }
            let mut outer: RECT = std::mem::zeroed();
            if GetWindowRect(hwnd, &mut outer) == 0 {
                return Err("That window is gone.".into());
            }
            // The invisible borders Windows draws around a window, so it lines up with the edge.
            let mut frame: RECT = std::mem::zeroed();
            let framed = DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS as u32, &mut frame as *mut RECT as *mut _, std::mem::size_of::<RECT>() as u32) == 0;
            let (l, t, r, b) = if framed { (frame.left - outer.left, frame.top - outer.top, outer.right - frame.right, outer.bottom - frame.bottom) } else { (0, 0, 0, 0) };
            let target = match place {
                Place::NextScreen => {
                    let areas = screens();
                    let at = areas.iter().position(|a| *a == here).unwrap_or(0);
                    let next = next_screen(&areas, at).ok_or("There's only one screen.")?;
                    carry(here, areas[next], if framed { rect(frame) } else { rect(outer) })
                }
                other => rect_for(here, other).ok_or("Couldn't place it.")?,
            };
            if SetWindowPos(hwnd, std::ptr::null_mut(), target.x - l, target.y - t, target.w + l + r, target.h + t + b, SWP_NOZORDER | SWP_NOACTIVATE) == 0 {
                return Err(REFUSED.into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { x: 0, y: 0, w: 1920, h: 1040 };

    #[test]
    fn halves_fill_the_screen_with_no_gap_or_overlap() {
        let (l, r) = (rect_for(SCREEN, Place::Left).unwrap(), rect_for(SCREEN, Place::Right).unwrap());
        assert_eq!(l, Rect { x: 0, y: 0, w: 960, h: 1040 });
        assert_eq!(r, Rect { x: 960, y: 0, w: 960, h: 1040 });
        let (t, b) = (rect_for(SCREEN, Place::Top).unwrap(), rect_for(SCREEN, Place::Bottom).unwrap());
        assert_eq!((t.h, b.y, b.h), (520, 520, 520));
        // An odd size gives the extra pixel to the second half.
        let odd = Rect { x: 100, y: 50, w: 1001, h: 701 };
        let (l, r) = (rect_for(odd, Place::Left).unwrap(), rect_for(odd, Place::Right).unwrap());
        assert_eq!((l.x + l.w, r.x, r.x + r.w), (600, 600, 1101));
        let (t, b) = (rect_for(odd, Place::Top).unwrap(), rect_for(odd, Place::Bottom).unwrap());
        assert_eq!((t.y + t.h, b.y, b.y + b.h), (400, 400, 751));
    }

    #[test]
    fn maximize_and_center_use_the_usable_area() {
        let area = Rect { x: -1920, y: 0, w: 1920, h: 1080 };
        assert_eq!(rect_for(area, Place::Maximize), Some(area));
        let c = rect_for(area, Place::Center).unwrap();
        assert_eq!((c.w, c.h), (1280, 720));
        // Its middle is the screen's middle.
        assert_eq!((c.x + c.w / 2, c.y + c.h / 2), (-1920 + 960, 540));
    }

    #[test]
    fn the_next_screen_needs_the_other_screen_and_is_not_a_rectangle_of_this_one() {
        assert_eq!(rect_for(SCREEN, Place::NextScreen), None);
    }

    #[test]
    fn a_window_keeps_its_place_and_share_of_the_screen_on_the_next_one() {
        let from = Rect { x: 0, y: 0, w: 1920, h: 1000 };
        let to = Rect { x: 1920, y: 0, w: 2560, h: 1400 };
        // The left half of one is the left half of the other.
        let left = rect_for(from, Place::Left).unwrap();
        assert_eq!(carry(from, to, left), Rect { x: 1920, y: 0, w: 1280, h: 1400 });
        // A window in the middle stays in the middle, a little bigger.
        let win = Rect { x: 480, y: 250, w: 960, h: 500 };
        assert_eq!(carry(from, to, win), Rect { x: 1920 + 640, y: 350, w: 1280, h: 700 });
        // Nothing to divide by, nothing to show.
        assert_eq!(carry(Rect { x: 0, y: 0, w: 0, h: 0 }, to, win).w, 1);
    }

    #[test]
    fn the_next_screen_wraps_round_and_there_is_none_with_one() {
        let one = [SCREEN];
        assert_eq!(next_screen(&one, 0), None);
        let two = [SCREEN, Rect { x: 1920, ..SCREEN }];
        assert_eq!((next_screen(&two, 0), next_screen(&two, 1)), (Some(1), Some(0)));
    }

    #[test]
    fn every_place_has_a_name_and_words_to_find_it_by() {
        for place in Place::ALL {
            assert!(!place.title().is_empty() && !place.words().is_empty() && !place.also().is_empty());
        }
        assert_eq!(Place::NextScreen.words()[0], "next screen");
        assert_eq!(can(), cfg!(windows));
    }
}
