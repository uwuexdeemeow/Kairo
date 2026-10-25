//! The command palette drawn natively: a Win32 popup on its own thread, painted with Direct2D
//! and DirectWrite around a real edit box, in Kairo's look. No web view, so it opens at once and
//! costs a few megabytes; its drawing resources are let go whenever it hides.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};

use rusqlite::Connection;
use tauri::{AppHandle, Manager};
use windows::core::{w, Interface, Result, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_IGNORE, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_OPEN, D2D1_PIXEL_FORMAT, D2D_RECT_F,
    D2D_SIZE_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Bitmap, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1RenderTarget, ID2D1SolidColorBrush, ID2D1StrokeStyle, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
    D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_PROPERTIES, D2D1_CAP_STYLE_FLAT, D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_DASH, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_LINE_JOIN_ROUND, D2D1_PRESENT_OPTIONS_NONE,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES,
    D2D1_SWEEP_DIRECTION_CLOCKWISE,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteFactory5, IDWriteFontCollection, IDWriteTextFormat, IDWriteTextLayout, IDWriteTextLayout1, DWRITE_CONTAINER_TYPE_WOFF2,
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_LIGHT, DWRITE_FONT_WEIGHT_MEDIUM, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_HIT_TEST_METRICS, DWRITE_LINE_METRICS, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_TRIMMING,
    DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DWM_WINDOW_CORNER_PREFERENCE};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    AddFontMemResourceEx, BeginPaint, CreateFontIndirectW, CreateSolidBrush, DeleteObject, EndPaint, GetMonitorInfoW, InvalidateRect, MonitorFromPoint, SetBkColor,
    SetTextColor, CLEARTYPE_QUALITY, HBRUSH, HDC, HFONT, LOGFONTW, MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, GetKeyState, SetFocus, KEYEVENTF_KEYUP, VK_BACK, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_MENU, VK_RETURN, VK_SHIFT, VK_TAB, VK_UP,
};
use windows::Win32::UI::Controls::EM_SETSEL;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetCursorPos, GetForegroundWindow, GetMessageW, GetWindowTextLengthW, GetWindowTextW,
    IsWindowVisible, LoadCursorW, PostMessageW, RegisterClassExW, SendMessageW, SetForegroundWindow, SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage,
    CS_DROPSHADOW, HWND_TOPMOST, IDC_ARROW, MSG, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, WA_INACTIVE, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_ACTIVATE, WM_APP, WM_CHAR, WM_COMMAND, WM_CTLCOLOREDIT, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT,
    WM_SETFONT, WNDCLASSEXW, WS_CHILD, WS_CLIPCHILDREN, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows_numerics::Vector2;

use super::search::{self, fmt_dur, Action, Context, Glyph, Icon, Item, Tone};
use super::cards::{self, Answer, Card, FocusPlan};
use super::search::Preview;
use super::{DayGlance, Segment};
use crate::analysis::Category;
use crate::{db, platform};

/// The window, once its thread has made it; the shortcut posts to it from Tauri's thread.
static WINDOW: AtomicIsize = AtomicIsize::new(0);
const WM_TOGGLE: u32 = WM_APP + 1;
const EN_CHANGE: u32 = 0x0300;
const EM_SETCUEBANNER: u32 = 0x1501;
const ES_AUTOHSCROLL: u32 = 0x0080;

// Sizes in device-independent pixels, the same as the drafts.
const W: f32 = 640.0;
const HEADER: f32 = 56.0;
const FOOTER: f32 = 38.0;
const MAX_H: f32 = 520.0;
const ROW_H: f32 = 44.0;
const TILE_H: f32 = 42.0;
const CHIP_H: f32 = 32.0;
const LABEL_H: f32 = 26.0;
/// The card above the results, by what it shows, counting the gap above it.
const DAY_CARD_H: f32 = 92.0;
const ANSWER_CARD_H: f32 = 200.0;
const FOCUS_CARD_H: f32 = 140.0;
/// From the top of the screen to the palette, as a share of the screen's height.
const FROM_TOP: f32 = 0.16;

const GEIST: &[u8] = include_bytes!("../../../node_modules/@fontsource-variable/geist/files/geist-latin-wght-normal.woff2");
const GEIST_MONO: &[u8] = include_bytes!("../../../node_modules/@fontsource-variable/geist-mono/files/geist-mono-latin-wght-normal.woff2");

/// Kairo's two palettes, the same values as the tokens in `src/styles.css`.
#[derive(Clone, Copy, PartialEq)]
struct Theme {
    bg: u32,
    ink: u32,
    muted: u32,
    rule: u32,
    line: u32,
    raised: u32,
    productive: u32,
    neutral: u32,
    distracting: u32,
    warm: u32,
    /// A change for the better, in an answer.
    good: u32,
}

const DARK: Theme = Theme {
    bg: 0x0e0f12,
    ink: 0xe8e9ec,
    muted: 0x9399a4,
    rule: 0x1e2026,
    line: 0x2a2d34,
    raised: 0x17191e,
    productive: 0x3987e5,
    neutral: 0x4a505c,
    distracting: 0xd95926,
    warm: 0xf0a27c,
    good: 0x7fd18b,
};

const LIGHT: Theme = Theme {
    bg: 0xf6f6f4,
    ink: 0x17181c,
    muted: 0x565c66,
    rule: 0xe6e6e3,
    line: 0xd2d3cf,
    raised: 0xffffff,
    productive: 0x2f6fd0,
    neutral: 0xa9aeb6,
    distracting: 0xcf5220,
    warm: 0xb95420,
    good: 0x2b8a3e,
};

fn color(hex: u32, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: ((hex >> 16) & 255) as f32 / 255.0, g: ((hex >> 8) & 255) as f32 / 255.0, b: (hex & 255) as f32 / 255.0, a: alpha }
}

fn colorref(hex: u32) -> COLORREF {
    COLORREF(((hex & 255) << 16) | (hex & 0xff00) | ((hex >> 16) & 255))
}

fn rect(l: f32, t: f32, r: f32, b: f32) -> D2D_RECT_F {
    D2D_RECT_F { left: l, top: t, right: r, bottom: b }
}

fn v(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}

/// "system" follows Windows' own app theme.
fn theme_for(mode: &str) -> Theme {
    match mode {
        "light" => LIGHT,
        "dark" => DARK,
        _ if platform_prefers_light() => LIGHT,
        _ => DARK,
    }
}

fn platform_prefers_light() -> bool {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let key: Vec<u16> = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize".encode_utf16().chain([0]).collect();
    let value: Vec<u16> = "AppsUseLightTheme".encode_utf16().chain([0]).collect();
    let mut data = 0u32;
    let mut size = 4u32;
    let rc = unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), (&mut data as *mut u32).cast(), &mut size) };
    rc == 0 && data == 1
}

#[derive(Clone, Copy, PartialEq)]
enum Kind2 {
    Row,
    Tile,
    Chip,
}

/// An item laid out in the results area, in its scrolled-away coordinates.
struct Placed {
    index: usize,
    kind: Kind2,
    rect: D2D_RECT_F,
}

struct Formats {
    title: IDWriteTextFormat,
    sub: IDWriteTextFormat,
    label: IDWriteTextFormat,
    mono: IDWriteTextFormat,
    kbd: IDWriteTextFormat,
    stat: IDWriteTextFormat,
    stat_label: IDWriteTextFormat,
    letter: IDWriteTextFormat,
    chip: IDWriteTextFormat,
    /// An answer's total.
    big: IDWriteTextFormat,
}

struct Palette {
    app: AppHandle,
    conn: Connection,
    hwnd: HWND,
    edit: HWND,
    d2d: ID2D1Factory,
    dwrite: IDWriteFactory,
    formats: Formats,
    stroke: ID2D1StrokeStyle,
    /// For an answer's limit line.
    dash: ID2D1StrokeStyle,
    target: Option<ID2D1HwndRenderTarget>,
    /// Icons as premultiplied BGRA, kept across openings; their bitmaps go with the target.
    pixels: HashMap<String, Option<(u32, u32, Vec<u8>)>>,
    bitmaps: HashMap<String, ID2D1Bitmap>,
    theme: Theme,
    /// The edit box's font, and the DPI it was made for.
    edit_font: Option<(HFONT, f32)>,
    /// The face the edit box uses: Geist when Windows took it, else Segoe UI.
    edit_face: &'static str,
    dpi: f32,
    /// Where the mouse last was, in window pixels: Windows reports a move when the palette opens
    /// under a resting mouse, which shouldn't change the selection.
    mouse: (i32, i32),
    ctx: Option<Context>,
    footer: String,
    items: Vec<Item>,
    rows: usize,
    tiles: usize,
    placed: Vec<Placed>,
    labels: Vec<(String, f32)>,
    content_h: f32,
    selected: usize,
    scroll: f32,
    preview: Option<(Preview, Card)>,
    error: Option<String>,
}

thread_local! {
    static PALETTE: RefCell<Option<Palette>> = const { RefCell::new(None) };
    /// The edit box's colors, read while Windows paints it, which can happen mid-update.
    static EDIT_COLORS: RefCell<(COLORREF, COLORREF, Option<HBRUSH>)> = const { RefCell::new((COLORREF(0), COLORREF(0), None)) };
}

/// Runs `f` on the palette, unless it's already busy (Windows calls back into the window while
/// the palette is mid-change; those calls get the default handling).
fn with<R>(f: impl FnOnce(&mut Palette) -> R) -> Option<R> {
    PALETTE.with(|p| p.try_borrow_mut().ok().and_then(|mut p| p.as_mut().map(f)))
}

/// Opens the palette, or puts it away if it's open. Safe from any thread.
pub fn toggle() {
    let hwnd = WINDOW.load(Ordering::Acquire);
    if hwnd != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_TOGGLE, WPARAM(0), LPARAM(0));
        }
    }
}

/// Starts the palette's thread and window, hidden until the shortcut unless `open`.
pub fn spawn(app: AppHandle, db_path: PathBuf, open: bool) {
    std::thread::Builder::new()
        .name("kairo-palette".into())
        .spawn(move || {
            // Reading shortcuts' icons and opening them goes through the shell, which wants COM.
            let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            let conn = match db::open(&db_path) {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("palette: cannot open database: {e}");
                    return;
                }
            };
            match unsafe { create(app, conn) } {
                Ok(()) => {
                    if open {
                        toggle();
                    }
                    unsafe { pump() }
                }
                Err(e) => eprintln!("palette: cannot create the window: {e}"),
            }
        })
        .expect("failed to start palette thread");
}

unsafe fn pump() {
    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

// ---------- fonts ----------

/// Geist and Geist Mono from Kairo's own copies, for DirectWrite, and Geist's raw data for the
/// edit box, which draws with GDI. None where Windows won't take them; text then falls back to
/// the system font.
unsafe fn load_fonts(dwrite: &IDWriteFactory) -> (Option<IDWriteFontCollection>, bool) {
    let Ok(factory) = dwrite.cast::<IDWriteFactory5>() else { return (None, false) };
    let collection = (|| -> Result<(IDWriteFontCollection, Vec<u8>)> {
        let loader = factory.CreateInMemoryFontFileLoader()?;
        factory.RegisterFontFileLoader(&loader)?;
        let builder = factory.CreateFontSetBuilder()?;
        let mut geist = Vec::new();
        for (i, packed) in [GEIST, GEIST_MONO].iter().enumerate() {
            let stream = factory.UnpackFontFile(DWRITE_CONTAINER_TYPE_WOFF2, packed.as_ptr().cast(), packed.len() as u32)?;
            let size = stream.GetFileSize()?;
            let mut start = std::ptr::null_mut();
            let mut context = std::ptr::null_mut();
            stream.ReadFileFragment(&mut start, 0, size, &mut context)?;
            let data = std::slice::from_raw_parts(start as *const u8, size as usize).to_vec();
            stream.ReleaseFileFragment(context);
            // With no owner, the loader keeps its own copy.
            let file = loader.CreateInMemoryFontFileReference(&factory, data.as_ptr().cast(), data.len() as u32, None)?;
            builder.AddFontFile(&file)?;
            if i == 0 {
                geist = data;
            }
        }
        let set = builder.CreateFontSet()?;
        Ok((factory.CreateFontCollectionFromFontSet(&set)?.cast()?, geist))
    })();
    match collection {
        Ok((fonts, geist)) => {
            // GDI needs the face registered with it too, for the edit box.
            let mut count = 0u32;
            let gdi = !AddFontMemResourceEx(geist.as_ptr().cast(), geist.len() as u32, None, std::ptr::addr_of_mut!(count)).is_invalid();
            (Some(fonts), gdi)
        }
        Err(e) => {
            eprintln!("palette: Geist unavailable, using the system font: {e}");
            (None, false)
        }
    }
}

unsafe fn format(dwrite: &IDWriteFactory, fonts: Option<&IDWriteFontCollection>, mono: bool, weight: DWRITE_FONT_WEIGHT, size: f32) -> Result<IDWriteTextFormat> {
    let family = match (fonts.is_some(), mono) {
        (true, false) => w!("Geist"),
        (true, true) => w!("Geist Mono"),
        (false, false) => w!("Segoe UI"),
        (false, true) => w!("Cascadia Mono"),
    };
    let f = dwrite.CreateTextFormat(family, fonts, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!("en-us"))?;
    f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
    let sign = dwrite.CreateEllipsisTrimmingSign(&f)?;
    f.SetTrimming(&DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 }, &sign)?;
    Ok(f)
}

// ---------- the window ----------

unsafe fn create(app: AppHandle, conn: Connection) -> Result<()> {
    let instance = GetModuleHandleW(None)?;
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_DROPSHADOW,
        lpfnWndProc: Some(window_proc),
        hInstance: instance.into(),
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        lpszClassName: w!("KairoPalette"),
        ..Default::default()
    };
    RegisterClassExW(&class);
    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        w!("KairoPalette"),
        w!("Kairo"),
        WS_POPUP | WS_CLIPCHILDREN,
        0,
        0,
        W as i32,
        200,
        None,
        None,
        Some(instance.into()),
        None,
    )?;
    // Windows 11 rounds the corners and draws the border; older versions ignore both.
    let round = DWMWCP_ROUND;
    let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, (&round as *const DWM_WINDOW_CORNER_PREFERENCE).cast(), 4);

    let edit = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("EDIT"),
        PCWSTR::null(),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(ES_AUTOHSCROLL),
        48,
        16,
        500,
        24,
        Some(hwnd),
        None,
        Some(instance.into()),
        None,
    )?;
    let _ = SetWindowSubclass(edit, Some(edit_proc), 1, 0);
    let cue: Vec<u16> = "Open an app, jump to a day, or type a command".encode_utf16().chain([0]).collect();
    SendMessageW(edit, EM_SETCUEBANNER, Some(WPARAM(1)), Some(LPARAM(cue.as_ptr() as isize)));

    let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
    let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
    let (fonts, gdi_geist) = load_fonts(&dwrite);
    let f = fonts.as_ref();
    let formats = Formats {
        title: format(&dwrite, f, false, DWRITE_FONT_WEIGHT_NORMAL, 14.0)?,
        sub: format(&dwrite, f, false, DWRITE_FONT_WEIGHT_NORMAL, 12.0)?,
        label: format(&dwrite, f, false, DWRITE_FONT_WEIGHT_MEDIUM, 11.0)?,
        mono: format(&dwrite, f, true, DWRITE_FONT_WEIGHT_NORMAL, 12.0)?,
        kbd: format(&dwrite, f, true, DWRITE_FONT_WEIGHT_NORMAL, 11.0)?,
        stat: format(&dwrite, f, true, DWRITE_FONT_WEIGHT_NORMAL, 15.0)?,
        stat_label: format(&dwrite, f, false, DWRITE_FONT_WEIGHT_NORMAL, 11.0)?,
        letter: format(&dwrite, f, true, DWRITE_FONT_WEIGHT_SEMI_BOLD, 10.0)?,
        chip: format(&dwrite, f, false, DWRITE_FONT_WEIGHT_NORMAL, 13.0)?,
        big: format(&dwrite, f, false, DWRITE_FONT_WEIGHT_LIGHT, 40.0)?,
    };
    let stroke = d2d.CreateStrokeStyle(
        &D2D1_STROKE_STYLE_PROPERTIES { startCap: D2D1_CAP_STYLE_ROUND, endCap: D2D1_CAP_STYLE_ROUND, lineJoin: D2D1_LINE_JOIN_ROUND, ..Default::default() },
        None,
    )?;
    let dash = d2d.CreateStrokeStyle(
        &D2D1_STROKE_STYLE_PROPERTIES { startCap: D2D1_CAP_STYLE_FLAT, endCap: D2D1_CAP_STYLE_FLAT, dashStyle: D2D1_DASH_STYLE_DASH, ..Default::default() },
        None,
    )?;

    PALETTE.with(|p| {
        *p.borrow_mut() = Some(Palette {
            app,
            conn,
            hwnd,
            edit,
            d2d,
            dwrite,
            formats,
            stroke,
            dash,
            target: None,
            pixels: HashMap::new(),
            bitmaps: HashMap::new(),
            theme: DARK,
            edit_font: None,
            edit_face: if gdi_geist { "Geist" } else { "Segoe UI" },
            dpi: 96.0,
            mouse: (-1, -1),
            ctx: None,
            footer: String::new(),
            items: Vec::new(),
            rows: 0,
            tiles: 0,
            placed: Vec::new(),
            labels: Vec::new(),
            content_h: 0.0,
            selected: 0,
            scroll: 0.0,
            preview: None,
            error: None,
        })
    });
    WINDOW.store(hwnd.0 as isize, Ordering::Release);
    Ok(())
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TOGGLE => {
            if IsWindowVisible(hwnd).as_bool() {
                with(|p| p.hide());
            } else {
                with(|p| p.show());
            }
            LRESULT(0)
        }
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            BeginPaint(hwnd, &mut ps);
            with(|p| {
                if let Err(e) = p.paint() {
                    eprintln!("palette: cannot paint: {e}");
                }
            });
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        // The palette is a glance: clicking anywhere else puts it away.
        WM_ACTIVATE if (wparam.0 & 0xffff) as u32 == WA_INACTIVE => {
            with(|p| p.hide());
            LRESULT(0)
        }
        WM_COMMAND if ((wparam.0 >> 16) & 0xffff) as u32 == EN_CHANGE => {
            with(|p| p.refresh());
            LRESULT(0)
        }
        WM_CTLCOLOREDIT => EDIT_COLORS.with(|c| {
            let (text, back, brush) = *c.borrow();
            let hdc = HDC(wparam.0 as *mut _);
            SetTextColor(hdc, text);
            SetBkColor(hdc, back);
            LRESULT(brush.map_or(0, |b| b.0 as isize))
        }),
        WM_MOUSEMOVE => {
            let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let x = (lparam.0 & 0xffff) as i16 as i32;
            with(|p| p.hover(x, y));
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            let x = (lparam.0 & 0xffff) as i16 as i32;
            with(|p| {
                if let Some(i) = p.hit(x, y) {
                    p.run(i);
                }
            });
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xffff) as i16 as f32;
            with(|p| p.scroll_by(-delta / 120.0 * 48.0));
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Keys the edit box shouldn't handle itself: moving through results, running one, closing.
unsafe extern "system" fn edit_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
    let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
    match (msg, wparam.0 as u16) {
        (WM_KEYDOWN, k) if k == VK_DOWN.0 || (k == VK_TAB.0 && !shift) => {
            with(|p| p.select_by(1));
            LRESULT(0)
        }
        (WM_KEYDOWN, k) if k == VK_UP.0 || (k == VK_TAB.0 && shift) => {
            with(|p| p.select_by(-1));
            LRESULT(0)
        }
        (WM_KEYDOWN, k) if k == VK_RETURN.0 => {
            with(|p| p.run(p.selected));
            LRESULT(0)
        }
        (WM_KEYDOWN, k) if k == VK_ESCAPE.0 => {
            with(|p| p.hide());
            LRESULT(0)
        }
        // Ctrl+Backspace deletes a word, as everywhere else in Windows.
        (WM_KEYDOWN, k) if k == VK_BACK.0 && ctrl => {
            let text = edit_text(hwnd);
            let trimmed = text.trim_end();
            let cut = trimmed.rfind(' ').map_or(0, |i| i + 1);
            set_edit_text(hwnd, &text[..cut]);
            LRESULT(0)
        }
        // Their characters would otherwise beep, or type a box for Ctrl+Backspace.
        (WM_CHAR, 0x09 | 0x0d | 0x1b | 0x7f) => LRESULT(0),
        _ => DefSubclassProc(hwnd, msg, wparam, lparam),
    }
}

fn edit_text(edit: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(edit);
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(edit, &mut buf);
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

fn set_edit_text(edit: HWND, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
    unsafe {
        let _ = SetWindowTextW(edit, PCWSTR(wide.as_ptr()));
        let end = text.encode_utf16().count();
        SendMessageW(edit, EM_SETSEL, Some(WPARAM(end)), Some(LPARAM(end as isize)));
    }
}

impl Palette {
    fn scale(&self) -> f32 {
        self.dpi / 96.0
    }

    fn results_top(&self) -> f32 {
        HEADER + self.card_h()
    }

    fn card_h(&self) -> f32 {
        match &self.preview {
            None => 0.0,
            Some((_, Card::Day(_))) => DAY_CARD_H,
            Some((_, Card::Answer(_))) => ANSWER_CARD_H,
            Some((_, Card::Focus(f))) => FOCUS_CARD_H + if f.replaces { 20.0 } else { 0.0 },
        }
    }

    fn results_h(&self) -> f32 {
        self.content_h.min(MAX_H - FOOTER - self.results_top())
    }

    fn height(&self) -> f32 {
        self.results_top() + self.results_h() + FOOTER
    }

    fn query(&self) -> String {
        edit_text(self.edit)
    }

    // ---------- opening and closing ----------

    fn show(&mut self) {
        self.load();
        let mode = crate::appearance::get(&self.conn).unwrap_or_default();
        self.set_theme(theme_for(&mode));
        set_edit_text(self.edit, "");
        self.refresh_results();
        unsafe {
            // Over the middle of the screen the mouse is on, in its upper part, like Spotlight.
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(monitor, &mut info);
            let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
            let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
            if dpi_x as f32 != self.dpi {
                self.dpi = dpi_x as f32;
                self.target = None;
                self.bitmaps.clear();
            }
            let s = self.scale();
            let work = info.rcWork;
            let (w, h) = ((W * s).round() as i32, (self.height() * s).round() as i32);
            let x = work.left + (work.right - work.left - w) / 2;
            let y = work.top + ((work.bottom - work.top) as f32 * FROM_TOP) as i32;
            self.mouse = (cursor.x - x, cursor.y - y);
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_SHOWWINDOW);
            self.place_edit();
            // Windows only hands over the keyboard to an app the user just acted in; pressing
            // Alt counts, so a refused request gets that nudge and another try.
            if !SetForegroundWindow(self.hwnd).as_bool() || GetForegroundWindow() != self.hwnd {
                keybd_event(VK_MENU.0 as u8, 0, Default::default(), 0);
                keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
                let _ = SetForegroundWindow(self.hwnd);
            }
            let _ = SetFocus(Some(self.edit));
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    fn hide(&mut self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
        // Drawing resources go until next time; that is most of what the palette would hold.
        self.target = None;
        self.bitmaps.clear();
        self.preview = None;
    }

    fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        let brush = unsafe { CreateSolidBrush(colorref(theme.raised)) };
        EDIT_COLORS.with(|c| {
            let mut c = c.borrow_mut();
            if let Some(old) = c.2 {
                unsafe {
                    let _ = DeleteObject(old.into());
                }
            }
            *c = (colorref(theme.ink), colorref(theme.raised), Some(brush));
        });
        unsafe {
            let border = colorref(theme.line);
            let _ = DwmSetWindowAttribute(self.hwnd, DWMWA_BORDER_COLOR, (&border as *const COLORREF).cast(), 4);
        }
    }

    fn place_edit(&mut self) {
        let s = self.scale();
        unsafe {
            if self.edit_font.is_none_or(|(_, dpi)| dpi != self.dpi) {
                let mut lf = LOGFONTW { lfHeight: -(17.0 * s).round() as i32, lfWeight: 400, lfQuality: CLEARTYPE_QUALITY, ..Default::default() };
                for (slot, c) in lf.lfFaceName.iter_mut().zip(self.edit_face.encode_utf16()) {
                    *slot = c;
                }
                let font = CreateFontIndirectW(&lf);
                SendMessageW(self.edit, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
                // The old one only goes once the box has let go of it.
                if let Some((old, _)) = self.edit_font.replace((font, self.dpi)) {
                    let _ = DeleteObject(old.into());
                }
            }
            let _ = SetWindowPos(self.edit, None, (48.0 * s) as i32, (17.0 * s) as i32, ((W - 48.0 - 72.0) * s) as i32, (24.0 * s) as i32, SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }

    /// Fits the window to its content after results change.
    fn resize(&mut self) {
        let s = self.scale();
        let (w, h) = ((W * s).round() as i32, (self.height() * s).round() as i32);
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
            if let Some(t) = &self.target {
                let _ = t.Resize(&D2D_SIZE_U { width: w as u32, height: h as u32 });
            }
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    fn repaint(&self) {
        unsafe {
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    // ---------- what it shows ----------

    /// What's known when it opens: apps, the focus session, today's apps and limits.
    fn load(&mut self) {
        let (ctx, footer) = super::context(&self.conn, &self.app.state::<super::Index>());
        self.ctx = Some(ctx);
        self.footer = footer;
    }

    fn refresh(&mut self) {
        self.refresh_results();
        self.resize();
    }

    fn refresh_results(&mut self) {
        let query = self.query();
        let Some(ctx) = &self.ctx else { return };
        if query.trim().is_empty() {
            let s = search::suggestions(ctx);
            self.rows = s.rows.len();
            self.tiles = s.tiles.len();
            self.items = s.rows.into_iter().chain(s.tiles).chain(s.chips).collect();
        } else {
            self.items = search::search(ctx, &query);
            self.rows = self.items.len();
            self.tiles = 0;
        }
        self.selected = 0;
        self.scroll = 0.0;
        self.error = None;
        self.update_preview();
        self.layout();
    }

    /// A selected day gets a glance at how it went, above the results.
    fn update_preview(&mut self) {
        let Some(wanted) = self.items.get(self.selected).and_then(|i| i.preview.clone()) else {
            self.preview = None;
            return;
        };
        if self.preview.as_ref().is_some_and(|(p, _)| *p == wanted) {
            return;
        }
        let card = cards::card(&self.conn, &wanted);
        self.preview = Some((wanted, card));
    }

    fn measure(&self, text: &str, format: &IDWriteTextFormat) -> f32 {
        unsafe {
            let wide: Vec<u16> = text.encode_utf16().collect();
            let Ok(layout) = self.dwrite.CreateTextLayout(&wide, format, 2000.0, 100.0) else { return 0.0 };
            let mut m = DWRITE_TEXT_METRICS::default();
            let _ = layout.GetMetrics(&mut m);
            m.widthIncludingTrailingWhitespace
        }
    }

    /// Positions every item: rows under section labels, tiles in threes, chips wrapping.
    fn layout(&mut self) {
        let typed = !self.query().trim().is_empty();
        let mut placed = Vec::new();
        let mut labels = Vec::new();
        // Room for a failed action's reason, above the results.
        let mut y = if self.error.is_some() { 38.0 } else { 4.0 };
        let mut section = "";
        for i in 0..self.rows {
            let item = &self.items[i];
            let heading = if typed && i == 0 { "Top hit" } else { item.section };
            if heading != section {
                y += 6.0;
                labels.push((heading.to_uppercase(), y));
                y += LABEL_H - 6.0;
                section = heading;
            }
            placed.push(Placed { index: i, kind: Kind2::Row, rect: rect(8.0, y, W - 8.0, y + ROW_H) });
            y += ROW_H + 2.0;
        }
        if self.tiles > 0 {
            y += 6.0;
            labels.push(("RECENT APPS".into(), y));
            y += LABEL_H - 6.0;
            let width = (W - 24.0 - 12.0) / 3.0;
            for t in 0..self.tiles {
                let x = 12.0 + t as f32 * (width + 6.0);
                placed.push(Placed { index: self.rows + t, kind: Kind2::Tile, rect: rect(x, y, x + width, y + TILE_H) });
            }
            y += TILE_H + 4.0;
        }
        let chips = self.rows + self.tiles..self.items.len();
        if !chips.is_empty() {
            y += 6.0;
            labels.push(("GO TO".into(), y));
            y += LABEL_H - 6.0;
            let mut x = 12.0;
            for i in chips {
                let width = self.measure(&self.items[i].title, &self.formats.chip) + 26.0;
                if x + width > W - 12.0 {
                    x = 12.0;
                    y += CHIP_H + 6.0;
                }
                placed.push(Placed { index: i, kind: Kind2::Chip, rect: rect(x, y, x + width, y + CHIP_H) });
                x += width + 6.0;
            }
            y += CHIP_H;
        }
        if self.items.is_empty() && typed {
            y += 40.0;
        }
        self.placed = placed;
        self.labels = labels;
        self.content_h = y + 10.0;
    }

    // ---------- moving and running ----------

    fn select_by(&mut self, step: i32) {
        if self.items.is_empty() {
            return;
        }
        let n = self.items.len() as i32;
        self.select(((self.selected as i32 + step) % n + n) as usize % n as usize);
    }

    fn select(&mut self, index: usize) {
        if index == self.selected {
            return;
        }
        let card_h = self.card_h();
        self.selected = index;
        self.update_preview();
        // Keep the selection in view.
        if let Some(p) = self.placed.iter().find(|p| p.index == index) {
            let view = self.results_h();
            if p.rect.top - 4.0 < self.scroll {
                self.scroll = (p.rect.top - 30.0).max(0.0);
            } else if p.rect.bottom + 4.0 > self.scroll + view {
                self.scroll = p.rect.bottom + 8.0 - view;
            }
        }
        if card_h != self.card_h() {
            self.resize();
        } else {
            self.repaint();
        }
    }

    fn scroll_by(&mut self, delta: f32) {
        let max = (self.content_h - self.results_h()).max(0.0);
        self.scroll = (self.scroll + delta).clamp(0.0, max);
        self.repaint();
    }

    /// The item at a point in window pixels.
    fn hit(&self, x: i32, y: i32) -> Option<usize> {
        let s = self.scale();
        let (x, y) = (x as f32 / s, y as f32 / s - self.results_top() + self.scroll);
        if y < self.scroll || y > self.scroll + self.results_h() {
            return None;
        }
        self.placed.iter().find(|p| x >= p.rect.left && x <= p.rect.right && y >= p.rect.top && y <= p.rect.bottom).map(|p| p.index)
    }

    fn hover(&mut self, x: i32, y: i32) {
        if self.mouse == (x, y) {
            return;
        }
        self.mouse = (x, y);
        if let Some(i) = self.hit(x, y) {
            self.select(i);
        }
    }

    fn run(&mut self, index: usize) {
        let Some(item) = self.items.get(index).cloned() else { return };
        // Kairo's window opens over where the palette was.
        if matches!(item.action, Action::Go(..)) {
            self.hide();
        }
        match super::perform(&self.app, &self.conn, &self.app.state::<super::Index>(), &item.action) {
            Ok(()) => self.hide(),
            Err(e) => {
                self.error = Some(e);
                self.layout();
                self.resize();
            }
        }
    }

    // ---------- drawing ----------

    fn target(&mut self) -> Result<ID2D1HwndRenderTarget> {
        if let Some(t) = &self.target {
            return Ok(t.clone());
        }
        let mut client = RECT::default();
        unsafe {
            GetClientRect(self.hwnd, &mut client)?;
            // Software drawing: a small panel draws fast on the CPU and keeps the graphics
            // driver's memory out of Kairo.
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_IGNORE },
                dpiX: self.dpi,
                dpiY: self.dpi,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd: self.hwnd,
                pixelSize: D2D_SIZE_U { width: client.right as u32, height: client.bottom as u32 },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            let t = self.d2d.CreateHwndRenderTarget(&props, &hwnd_props)?;
            self.target = Some(t.clone());
            Ok(t)
        }
    }

    fn brush(&self, rt: &ID2D1RenderTarget, hex: u32, alpha: f32) -> Result<ID2D1SolidColorBrush> {
        unsafe { rt.CreateSolidColorBrush(&color(hex, alpha), None) }
    }

    /// Draws text left-aligned (or right-aligned into `max_w`), marking the characters at `at`
    /// as the web version did: heavier, and underlined in the accent color. Returns the drawn
    /// width.
    #[allow(clippy::too_many_arguments)]
    fn text(&self, rt: &ID2D1RenderTarget, s: &str, format: &IDWriteTextFormat, x: f32, y: f32, max_w: f32, hex: u32, at: &[usize], right: bool) -> Result<f32> {
        unsafe {
            let wide: Vec<u16> = s.encode_utf16().collect();
            let layout: IDWriteTextLayout = self.dwrite.CreateTextLayout(&wide, format, max_w.max(0.0), 40.0)?;
            if right {
                layout.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING)?;
            }
            // Characters to UTF-16 positions.
            let mut offsets = Vec::with_capacity(s.len());
            let mut pos = 0u32;
            for c in s.chars() {
                offsets.push((pos, c.len_utf16() as u32));
                pos += c.len_utf16() as u32;
            }
            let marked: Vec<DWRITE_TEXT_RANGE> =
                at.iter().filter_map(|&i| offsets.get(i)).map(|&(start, len)| DWRITE_TEXT_RANGE { startPosition: start, length: len }).collect();
            for &range in &marked {
                layout.SetFontWeight(DWRITE_FONT_WEIGHT_SEMI_BOLD, range)?;
            }
            if std::ptr::eq(format, &self.formats.label) {
                if let Ok(spaced) = layout.cast::<IDWriteTextLayout1>() {
                    let _ = spaced.SetCharacterSpacing(0.0, 0.66, 0.0, DWRITE_TEXT_RANGE { startPosition: 0, length: pos });
                }
            }
            let brush = self.brush(rt, hex, 1.0)?;
            rt.DrawTextLayout(v(x, y), &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
            if !marked.is_empty() {
                let mut line = [DWRITE_LINE_METRICS::default()];
                let mut lines = 0;
                layout.GetLineMetrics(Some(&mut line), &mut lines)?;
                let under = y + line[0].baseline + 3.5;
                let accent = self.brush(rt, self.theme.productive, 1.0)?;
                for range in marked {
                    let (mut px, mut py) = (0.0, 0.0);
                    let mut hit = DWRITE_HIT_TEST_METRICS::default();
                    layout.HitTestTextPosition(range.startPosition, false, &mut px, &mut py, &mut hit)?;
                    // Letters cut off by the ellipsis aren't shown, so neither is their mark.
                    if hit.isTrimmed.as_bool() || hit.left + hit.width > max_w {
                        continue;
                    }
                    rt.DrawLine(v(x + hit.left, under), v(x + hit.left + hit.width, under), &accent, 1.0, None);
                }
            }
            let mut m = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut m)?;
            Ok(m.widthIncludingTrailingWhitespace)
        }
    }

    /// A key cap, drawn ending at `right`; returns its left edge.
    fn kbd(&self, rt: &ID2D1RenderTarget, label: &str, right: f32, mid_y: f32) -> Result<f32> {
        let width = self.measure(label, &self.formats.kbd) + 12.0;
        let r = rect(right - width, mid_y - 10.0, right, mid_y + 10.0);
        unsafe {
            rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: r, radiusX: 4.0, radiusY: 4.0 }, &self.brush(rt, self.theme.bg, 1.0)?);
            rt.DrawRoundedRectangle(&D2D1_ROUNDED_RECT { rect: r, radiusX: 4.0, radiusY: 4.0 }, &self.brush(rt, self.theme.line, 1.0)?, 1.0, None);
        }
        self.text(rt, label, &self.formats.kbd, r.left + 6.0, mid_y - 7.5, width, self.theme.muted, &[], false)?;
        Ok(r.left)
    }

    /// The stroke icons, in the same 24-unit grid as the web versions.
    fn glyph(&self, rt: &ID2D1RenderTarget, kind: Option<Glyph>, x: f32, y: f32, size: f32, hex: u32) -> Result<()> {
        let k = size / 24.0;
        let p = |a: f32, b: f32| v(x + a * k, y + b * k);
        let brush = self.brush(rt, hex, 1.0)?;
        let width = 1.75 * k.max(0.75);
        let stroke = &self.stroke;
        unsafe {
            let line = |a: (f32, f32), b: (f32, f32)| rt.DrawLine(p(a.0, a.1), p(b.0, b.1), &brush, width, stroke);
            let circle = |cx: f32, cy: f32, r: f32| rt.DrawEllipse(&D2D1_ELLIPSE { point: p(cx, cy), radiusX: r * k, radiusY: r * k }, &brush, width, stroke);
            match kind {
                // The search box's magnifier.
                None => {
                    circle(11.0, 11.0, 7.0);
                    line((20.0, 20.0), (16.5, 16.5));
                }
                Some(Glyph::Focus) => {
                    circle(12.0, 13.0, 8.0);
                    line((12.0, 9.0), (12.0, 13.0));
                    line((12.0, 13.0), (14.5, 15.5));
                    line((9.5, 2.0), (14.5, 2.0));
                }
                Some(Glyph::Day) => {
                    let r = D2D1_ROUNDED_RECT { rect: rect(x + 3.0 * k, y + 5.0 * k, x + 21.0 * k, y + 21.0 * k), radiusX: 2.0 * k, radiusY: 2.0 * k };
                    rt.DrawRoundedRectangle(&r, &brush, width, stroke);
                    line((3.0, 10.0), (21.0, 10.0));
                    line((8.0, 3.0), (8.0, 7.0));
                    line((16.0, 3.0), (16.0, 7.0));
                }
                Some(Glyph::Chart) => {
                    line((3.0, 21.0), (21.0, 21.0));
                    line((6.0, 17.0), (6.0, 12.0));
                    line((11.0, 17.0), (11.0, 6.0));
                    line((16.0, 17.0), (16.0, 9.0));
                    line((21.0, 17.0), (21.0, 14.0));
                }
                Some(Glyph::Tag) => {
                    let points = [(3.0, 12.0), (3.0, 4.0), (4.0, 3.0), (12.0, 3.0), (21.0, 12.0), (12.0, 21.0), (3.0, 12.0)];
                    for pair in points.windows(2) {
                        line(pair[0], pair[1]);
                    }
                    circle(7.5, 7.5, 1.5);
                }
                Some(Glyph::Settings) => {
                    line((4.0, 6.0), (14.0, 6.0));
                    line((18.0, 6.0), (20.0, 6.0));
                    line((4.0, 12.0), (8.0, 12.0));
                    line((12.0, 12.0), (20.0, 12.0));
                    line((4.0, 18.0), (16.0, 18.0));
                    circle(16.0, 6.0, 2.0);
                    circle(10.0, 12.0, 2.0);
                    circle(18.0, 18.0, 2.0);
                }
                Some(Glyph::Today) => {
                    circle(12.0, 12.0, 4.0);
                    for (a, b) in [((12.0, 2.0), (12.0, 4.0)), ((12.0, 20.0), (12.0, 22.0)), ((4.9, 4.9), (6.3, 6.3)), ((17.7, 17.7), (19.1, 19.1)), ((2.0, 12.0), (4.0, 12.0)), ((20.0, 12.0), (22.0, 12.0)), ((4.9, 19.1), (6.3, 17.7)), ((17.7, 6.3), (19.1, 4.9))] {
                        line(a, b);
                    }
                }
                Some(Glyph::Alert) => {
                    circle(12.0, 12.0, 9.0);
                    line((12.0, 8.0), (12.0, 13.0));
                    line((12.0, 16.5), (12.0, 17.0));
                }
                Some(Glyph::Update) => {
                    let geometry = self.d2d.CreatePathGeometry()?;
                    let sink = geometry.Open()?;
                    sink.BeginFigure(p(20.0, 12.0), D2D1_FIGURE_BEGIN_HOLLOW);
                    sink.AddArc(&D2D1_ARC_SEGMENT {
                        point: p(17.7, 6.3),
                        size: D2D_SIZE_F { width: 8.0 * k, height: 8.0 * k },
                        rotationAngle: 0.0,
                        sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                        arcSize: D2D1_ARC_SIZE_LARGE,
                    });
                    sink.EndFigure(D2D1_FIGURE_END_OPEN);
                    sink.Close()?;
                    rt.DrawGeometry(&geometry, &brush, width, stroke);
                    line((20.0, 4.0), (20.0, 9.0));
                    line((20.0, 9.0), (15.0, 9.0));
                }
            }
        }
        Ok(())
    }

    /// An item's icon centred on `mid_y`: the app's own, a letter when it has none, or a glyph
    /// on a tinted tile.
    fn icon(&mut self, rt: &ID2D1RenderTarget, icon: &Icon, title: &str, x: f32, mid_y: f32, size: f32) -> Result<()> {
        let t = self.theme;
        match icon {
            Icon::App(path) => {
                if let Some(bitmap) = self.bitmap(rt, path)? {
                    let r = rect(x, mid_y - size / 2.0, x + size, mid_y + size / 2.0);
                    unsafe { rt.DrawBitmap(&bitmap, Some(&r), 1.0, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, None) };
                } else {
                    let r = D2D1_ROUNDED_RECT { rect: rect(x, mid_y - size / 2.0, x + size, mid_y + size / 2.0), radiusX: 4.0, radiusY: 4.0 };
                    unsafe {
                        rt.FillRoundedRectangle(&r, &self.brush(rt, t.raised, 1.0)?);
                        rt.DrawRoundedRectangle(&r, &self.brush(rt, t.line, 1.0)?, 1.0, None);
                    }
                    let letter: String = title.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
                    let lw = self.measure(&letter, &self.formats.letter);
                    self.text(rt, &letter, &self.formats.letter, x + (size - lw) / 2.0, mid_y - 7.0, size, t.muted, &[], false)?;
                }
            }
            Icon::Glyph(g, tone) => {
                let (tile, alpha, ink) = match tone {
                    Tone::Accent => (Some(t.productive), 0.16, t.productive),
                    Tone::Warm => (Some(t.distracting), 0.14, t.warm),
                    Tone::Plain => (None, 0.0, t.muted),
                };
                if let Some(tile) = tile {
                    let r = D2D1_ROUNDED_RECT { rect: rect(x, mid_y - 14.0, x + 28.0, mid_y + 14.0), radiusX: 7.0, radiusY: 7.0 };
                    unsafe { rt.FillRoundedRectangle(&r, &self.brush(rt, tile, alpha)?) };
                }
                self.glyph(rt, Some(*g), x + 6.0, mid_y - 8.0, 16.0, ink)?;
            }
        }
        Ok(())
    }

    /// An app's icon as a bitmap for this target, read from Windows the first time it's shown.
    fn bitmap(&mut self, rt: &ID2D1RenderTarget, path: &str) -> Result<Option<ID2D1Bitmap>> {
        // Only a file or a Start menu entry has one; asked for anything else, Windows gives a
        // folder. A site, or an app whose program Kairo hasn't found, gets its letter.
        if !path.contains('\\') {
            return Ok(None);
        }
        if let Some(b) = self.bitmaps.get(path) {
            return Ok(Some(b.clone()));
        }
        let pixels = self.pixels.entry(path.to_string()).or_insert_with(|| {
            platform::icon_pixels(path).map(|(w, h, mut px)| {
                // RGBA to premultiplied BGRA, the order and form Direct2D draws.
                for p in px.chunks_exact_mut(4) {
                    let a = p[3] as u32;
                    let (r, g, b) = (p[0] as u32 * a / 255, p[1] as u32 * a / 255, p[2] as u32 * a / 255);
                    p[0] = b as u8;
                    p[1] = g as u8;
                    p[2] = r as u8;
                }
                (w, h, px)
            })
        });
        let Some((w, h, px)) = pixels else { return Ok(None) };
        let props = D2D1_BITMAP_PROPERTIES { pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED }, dpiX: 96.0, dpiY: 96.0 };
        let bitmap = unsafe { rt.CreateBitmap(D2D_SIZE_U { width: *w, height: *h }, Some(px.as_ptr().cast()), *w * 4, &props)? };
        self.bitmaps.insert(path.to_string(), bitmap.clone());
        Ok(Some(bitmap))
    }

    fn paint(&mut self) -> Result<()> {
        let hwnd_target = self.target()?;
        let rt: ID2D1RenderTarget = hwnd_target.cast()?;
        let t = self.theme;
        unsafe {
            rt.SetDpi(self.dpi, self.dpi);
            rt.BeginDraw();
            rt.Clear(Some(&color(t.raised, 1.0)));
        }
        let drawn = self.draw(&rt);
        let ended = unsafe { rt.EndDraw(None, None) };
        if ended.is_err() {
            // The target is gone (a display change, say); the next paint makes a new one.
            self.target = None;
            self.bitmaps.clear();
        }
        drawn
    }

    fn draw(&mut self, rt: &ID2D1RenderTarget) -> Result<()> {
        let t = self.theme;
        let height = self.height();
        let rule = self.brush(rt, t.rule, 1.0)?;

        // The search row: magnifier, the edit box (a real window, drawn by Windows), Esc.
        self.glyph(rt, None, 18.0, 19.0, 18.0, t.muted)?;
        self.kbd(rt, "Esc", W - 18.0, HEADER / 2.0)?;
        unsafe { rt.DrawLine(v(0.0, HEADER - 0.5), v(W, HEADER - 0.5), &rule, 1.0, None) };

        // Out while it's drawn, since drawing an icon may keep its bitmap.
        let preview = self.preview.take();
        let drawn = match preview.as_ref().map(|(_, c)| c) {
            Some(Card::Day(day)) => self.draw_day(rt, day),
            Some(Card::Answer(answer)) => self.draw_answer(rt, answer),
            Some(Card::Focus(plan)) => self.draw_focus(rt, plan),
            None => Ok(()),
        };
        self.preview = preview;
        drawn?;

        // Results, clipped to their area and scrolled.
        let top = self.results_top();
        let view = rect(0.0, top, W, top + self.results_h());
        unsafe { rt.PushAxisAlignedClip(&view, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE) };
        let offset = top - self.scroll;
        for (label, y) in self.labels.clone() {
            self.text(rt, &label, &self.formats.label, 18.0, offset + y + 4.0, W - 36.0, t.muted, &[], false)?;
        }
        if let Some(error) = self.error.clone() {
            self.text(rt, &error, &self.formats.chip, 20.0, offset + 14.0, W - 40.0, t.warm, &[], false)?;
        }
        if self.items.is_empty() && !self.query().trim().is_empty() {
            let message = "Nothing matches. Try an app’s name, a day like “last tue”, or “focus 50”.";
            self.text(rt, message, &self.formats.chip, 20.0, offset + 16.0, W - 40.0, t.muted, &[], false)?;
        }
        let placed: Vec<(usize, Kind2, D2D_RECT_F)> = self.placed.iter().map(|p| (p.index, p.kind, p.rect)).collect();
        for (index, kind, r) in placed {
            let r = rect(r.left, r.top + offset, r.right, r.bottom + offset);
            if r.bottom < top || r.top > top + self.results_h() {
                continue;
            }
            let item = self.items[index].clone();
            let selected = index == self.selected;
            match kind {
                Kind2::Row => self.draw_row(rt, &item, r, selected)?,
                Kind2::Tile => self.draw_tile(rt, &item, r, selected)?,
                Kind2::Chip => self.draw_chip(rt, &item, r, selected)?,
            }
        }
        unsafe { rt.PopAxisAlignedClip() };

        // The footer: keys, and today so far.
        let foot = height - FOOTER;
        unsafe { rt.DrawLine(v(0.0, foot + 0.5), v(W, foot + 0.5), &rule, 1.0, None) };
        let mid = foot + FOOTER / 2.0;
        let mut x = 18.0;
        for (keys, label) in [(&["↑", "↓"][..], "Move"), (&["Enter"][..], "Run")] {
            for key in keys {
                let w = self.measure(key, &self.formats.kbd) + 12.0;
                self.kbd(rt, key, x + w, mid)?;
                x += w + 6.0;
            }
            x += self.text(rt, label, &self.formats.sub, x, mid - 8.0, 100.0, t.muted, &[], false)? + 16.0;
        }
        let footer = self.footer.clone();
        self.text(rt, &footer, &self.formats.mono, x, mid - 8.0, W - 18.0 - x, t.muted, &[], true)?;
        Ok(())
    }

    /// The card's ground, from under the search row to `height` below it.
    fn card(&self, rt: &ID2D1RenderTarget, height: f32) -> Result<()> {
        let card = D2D1_ROUNDED_RECT { rect: rect(10.0, HEADER + 10.0, W - 10.0, HEADER + height), radiusX: 10.0, radiusY: 10.0 };
        unsafe { rt.FillRoundedRectangle(&card, &self.brush(rt, self.theme.bg, 1.0)?) };
        Ok(())
    }

    /// Time in an app or site over a period: its total, the change, a bar a day (or an hour)
    /// against its limit, and what stands out.
    fn draw_answer(&mut self, rt: &ID2D1RenderTarget, a: &Answer) -> Result<()> {
        let t = self.theme;
        self.card(rt, ANSWER_CARD_H)?;
        let top = HEADER + 10.0;
        let x = 30.0;
        let icon = Icon::App(a.icon.clone().unwrap_or_default());
        self.icon(rt, &icon, &a.title, x, top + 34.0, 28.0)?;
        self.text(rt, &a.title, &self.formats.title, x + 42.0, top + 17.0, W - 72.0 - x, t.ink, &[], false)?;
        self.text(rt, &a.range, &self.formats.sub, x + 42.0, top + 36.0, W - 72.0 - x, t.muted, &[], false)?;

        self.text(rt, &fmt_dur(a.total_ms), &self.formats.big, x - 2.0, top + 62.0, 200.0, t.ink, &[], false)?;
        if let Some(change) = &a.change {
            let hex = match a.better {
                Some(true) => t.good,
                Some(false) => t.warm,
                None => t.muted,
            };
            self.text(rt, change, &self.formats.mono, x, top + 114.0, 200.0, hex, &[], false)?;
        }

        // The bars, scaled to the busiest one or a little over the limit line.
        let (left, right, floor, height) = (x + 210.0, W - 30.0, top + 124.0, 62.0);
        let n = a.bars.len().max(1) as f32;
        let gap = if n > 7.0 { 3.0 } else { 8.0 };
        let width = ((right - left) - gap * (n - 1.0)) / n;
        let most = a.bars.iter().map(|b| b.ms).max().unwrap_or(0).max(a.limit_ms.map_or(0, |l| l * 6 / 5)).max(1) as f32;
        for (i, bar) in a.bars.iter().enumerate() {
            let bx = left + i as f32 * (width + gap);
            let h = if bar.ms > 0 { (bar.ms as f32 / most * height).max(3.0) } else { 2.0 };
            let hex = if bar.over {
                t.distracting
            } else if bar.ms > 0 {
                t.neutral
            } else {
                t.line
            };
            let shape = D2D1_ROUNDED_RECT { rect: rect(bx, floor - h, bx + width, floor), radiusX: 3.0_f32.min(width / 2.0), radiusY: 3.0_f32.min(width / 2.0) };
            unsafe { rt.FillRoundedRectangle(&shape, &self.brush(rt, hex, 1.0)?) };
            if !bar.label.is_empty() {
                let lw = self.measure(&bar.label, &self.formats.stat_label);
                let lx = if n > 7.0 { bx } else { bx + (width - lw) / 2.0 };
                self.text(rt, &bar.label, &self.formats.stat_label, lx, floor + 4.0, 40.0, t.muted, &[], false)?;
            }
        }
        if let Some(limit) = a.limit_ms {
            let y = (floor - limit as f32 / most * height).round() + 0.5;
            unsafe { rt.DrawLine(v(left, y), v(right, y), &self.brush(rt, t.neutral, 1.0)?, 1.0, &self.dash) };
        }

        self.text(rt, &a.note, &self.formats.chip, x, top + 152.0, W - 2.0 * x, t.muted, &[], false)?;
        Ok(())
    }

    /// What a focus session would do once started: when it ends, what's blocked, the
    /// launchers, strict mode.
    fn draw_focus(&mut self, rt: &ID2D1RenderTarget, f: &FocusPlan) -> Result<()> {
        let t = self.theme;
        // Its height from the plan itself: the card is out of `self.preview` while drawn.
        self.card(rt, FOCUS_CARD_H + if f.replaces { 20.0 } else { 0.0 })?;
        let top = HEADER + 10.0;
        let x = 30.0;
        self.icon(rt, &Icon::Glyph(Glyph::Focus, Tone::Accent), "", x, top + 34.0, 28.0)?;
        self.text(rt, &f.title, &self.formats.title, x + 42.0, top + 17.0, W - 72.0 - x, t.ink, &[], false)?;
        self.text(rt, &f.sub, &self.formats.sub, x + 42.0, top + 36.0, W - 72.0 - x, t.muted, &[], false)?;
        let tiles = [("Blocking", &f.blocking), ("Launchers", &f.launchers), ("Strict mode", &f.strict)];
        let width = (W - 2.0 * x - 20.0) / 3.0;
        for (i, (name, value)) in tiles.into_iter().enumerate() {
            let tx = x + i as f32 * (width + 10.0);
            let tile = D2D1_ROUNDED_RECT { rect: rect(tx, top + 62.0, tx + width, top + 112.0), radiusX: 8.0, radiusY: 8.0 };
            unsafe { rt.DrawRoundedRectangle(&tile, &self.brush(rt, t.line, 1.0)?, 1.0, None) };
            self.text(rt, name, &self.formats.stat_label, tx + 12.0, top + 70.0, width - 24.0, t.muted, &[], false)?;
            self.text(rt, value, &self.formats.chip, tx + 12.0, top + 87.0, width - 24.0, t.ink, &[], false)?;
        }
        if f.replaces {
            self.text(rt, "Starting it ends the session that's running.", &self.formats.chip, x, top + 122.0, W - 2.0 * x, t.warm, &[], false)?;
        }
        Ok(())
    }

    fn draw_day(&self, rt: &ID2D1RenderTarget, p: &DayGlance) -> Result<()> {
        let t = self.theme;
        self.card(rt, DAY_CARD_H)?;
        let stats = [
            ("On screen", fmt_dur(p.active_ms)),
            ("Productive", format!("{}%", p.productive_pct)),
            ("Focus sessions", if p.focus_count > 0 { format!("{} · {}", p.focus_count, fmt_dur(p.focus_ms)) } else { "0".into() }),
        ];
        let mut x = 26.0;
        for (name, value) in stats {
            self.text(rt, name, &self.formats.stat_label, x, HEADER + 22.0, 200.0, t.muted, &[], false)?;
            let w = self.text(rt, &value, &self.formats.stat, x, HEADER + 38.0, 200.0, t.ink, &[], false)?;
            x += w.max(self.measure(name, &self.formats.stat_label)) + 28.0;
        }
        let strip = rect(26.0, HEADER + 68.0, W - 26.0, HEADER + 78.0);
        unsafe {
            rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: strip, radiusX: 3.0, radiusY: 3.0 }, &self.brush(rt, t.rule, 1.0)?);
            for &Segment { from, to, category } in &p.segments {
                let hex = match category {
                    Category::Productive => t.productive,
                    Category::Neutral => t.neutral,
                    Category::Distracting => t.distracting,
                };
                let width = strip.right - strip.left;
                let seg = rect(strip.left + from * width, strip.top, (strip.left + to * width).max(strip.left + from * width + 2.0), strip.bottom);
                rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: seg, radiusX: 2.0, radiusY: 2.0 }, &self.brush(rt, hex, 1.0)?);
            }
        }
        Ok(())
    }

    fn draw_row(&mut self, rt: &ID2D1RenderTarget, item: &Item, r: D2D_RECT_F, selected: bool) -> Result<()> {
        let t = self.theme;
        if selected {
            unsafe { rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: r, radiusX: 8.0, radiusY: 8.0 }, &self.brush(rt, t.line, 1.0)?) };
        }
        let mid = (r.top + r.bottom) / 2.0;
        // Icons sit in a 28-pixel box; an app's own is 24 within it.
        let icon_x = if matches!(item.icon, Icon::App(_)) { r.left + 14.0 } else { r.left + 12.0 };
        self.icon(rt, &item.icon, &item.title, icon_x, mid, 24.0)?;
        let mut right = r.right - 12.0;
        if selected {
            right = self.kbd(rt, "Enter", right, mid)? - 10.0;
        }
        if let Some(detail) = &item.detail {
            let w = self.measure(detail, &self.formats.mono);
            self.text(rt, detail, &self.formats.mono, right - w, mid - 8.0, w + 2.0, t.muted, &[], false)?;
            right -= w + 12.0;
        }
        let x = r.left + 52.0;
        match &item.sub {
            Some(sub) => {
                self.text(rt, &item.title, &self.formats.title, x, mid - 18.0, right - x, t.ink, &item.at, false)?;
                self.text(rt, sub, &self.formats.sub, x, mid + 1.0, right - x, t.muted, &[], false)?;
            }
            None => {
                self.text(rt, &item.title, &self.formats.title, x, mid - 9.5, right - x, t.ink, &item.at, false)?;
            }
        }
        Ok(())
    }

    fn draw_tile(&mut self, rt: &ID2D1RenderTarget, item: &Item, r: D2D_RECT_F, selected: bool) -> Result<()> {
        let t = self.theme;
        let shape = D2D1_ROUNDED_RECT { rect: r, radiusX: 8.0, radiusY: 8.0 };
        unsafe {
            if selected {
                rt.FillRoundedRectangle(&shape, &self.brush(rt, t.line, 1.0)?);
            }
            rt.DrawRoundedRectangle(&shape, &self.brush(rt, t.rule, 1.0)?, 1.0, None);
        }
        let mid = (r.top + r.bottom) / 2.0;
        self.icon(rt, &item.icon, &item.title, r.left + 10.0, mid, 18.0)?;
        let mut right = r.right - 10.0;
        if let Some(detail) = &item.detail {
            let w = self.measure(detail, &self.formats.kbd);
            self.text(rt, detail, &self.formats.kbd, right - w, mid - 7.5, w + 2.0, t.muted, &[], false)?;
            right -= w + 8.0;
        }
        self.text(rt, &item.title, &self.formats.sub, r.left + 38.0, mid - 8.0, right - r.left - 38.0, t.ink, &[], false)?;
        Ok(())
    }

    fn draw_chip(&self, rt: &ID2D1RenderTarget, item: &Item, r: D2D_RECT_F, selected: bool) -> Result<()> {
        let t = self.theme;
        let shape = D2D1_ROUNDED_RECT { rect: r, radiusX: CHIP_H / 2.0, radiusY: CHIP_H / 2.0 };
        unsafe {
            if selected {
                rt.FillRoundedRectangle(&shape, &self.brush(rt, t.productive, 0.12)?);
            }
            rt.DrawRoundedRectangle(&shape, &self.brush(rt, if selected { t.productive } else { t.line }, 1.0)?, 1.0, None);
        }
        self.text(rt, &item.title, &self.formats.chip, r.left + 13.0, r.top + 7.0, r.right - r.left, t.ink, &[], false)?;
        Ok(())
    }
}
