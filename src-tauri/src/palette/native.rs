//! The command palette drawn natively: a Win32 popup on its own thread, painted with Direct2D
//! and DirectWrite around a real edit box, in Kairo's look. No web view, so it opens at once and
//! costs a few megabytes; its drawing resources are let go whenever it hides.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;

use rusqlite::Connection;
use tauri::{AppHandle, Manager};
use windows::core::{w, Interface, Result, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_IGNORE, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Bitmap, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1RenderTarget, ID2D1SolidColorBrush, ID2D1StrokeStyle, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_PROPERTIES, D2D1_CAP_STYLE_FLAT, D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_DASH, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE,
    D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_LINE_JOIN_ROUND, D2D1_PRESENT_OPTIONS_NONE,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES,
    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteFactory5, IDWriteFontCollection, IDWriteTextFormat, IDWriteTextLayout, IDWriteTextLayout1, DWRITE_CONTAINER_TYPE_WOFF2,
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_LIGHT, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_HIT_TEST_METRICS, DWRITE_LINE_METRICS, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_TRIMMING,
    DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMSBT_TRANSIENTWINDOW, DWMWA_BORDER_COLOR, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DWM_SYSTEMBACKDROP_TYPE, DWM_WINDOW_CORNER_PREFERENCE,
};
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
use windows::Win32::UI::Controls::{EM_SETSEL, MARGINS};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetCursorPos, GetForegroundWindow, GetMessageW, GetWindowTextLengthW, GetWindowTextW,
    IsWindowVisible, LoadCursorW, PostMessageW, RegisterClassExW, SendMessageW, SetForegroundWindow, SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage,
    CHILDID_SELF, CS_DROPSHADOW, EVENT_OBJECT_NAMECHANGE, HWND_TOPMOST, IDC_ARROW, MSG, OBJID_CLIENT, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, WA_INACTIVE, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_ACTIVATE, WM_APP, WM_CHAR, WM_COMMAND, WM_CTLCOLOREDIT, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT,
    WM_SETFONT, WNDCLASSEXW, WS_CHILD, WS_CLIPCHILDREN, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows_numerics::Vector2;

use super::files::{self, Worker};
use super::mode::Mode;
use windows::Win32::UI::Accessibility::NotifyWinEvent;
use super::search::{self, fmt_dur, Action, Context, Glyph, Icon, Item, Tone};
use super::settings::{self, Switch};
use super::cards::{self, Answer, Card, FocusPlan};
use super::search::Preview;
use super::{DayGlance, Segment};
use crate::analysis::Category;
use crate::{db, platform};

/// The window, once its thread has made it; the shortcut posts to it from Tauri's thread.
static WINDOW: AtomicIsize = AtomicIsize::new(0);
const WM_TOGGLE: u32 = WM_APP + 1;
/// Files were found; they wait in `FILES` for the palette's thread.
const WM_FILES: u32 = WM_APP + 2;
/// Sent to itself as it opens: what it shows is gathered afresh once it's up.
const WM_RELOAD: u32 = WM_APP + 3;
/// The latest files found and the number of the results they're for, handed from the file
/// search's thread to the palette's, which takes them when `WM_FILES` arrives.
static FILES: Mutex<Option<(u64, Vec<Item>)>> = Mutex::new(None);
const EN_CHANGE: u32 = 0x0300;
const EM_SETCUEBANNER: u32 = 0x1501;
const ES_AUTOHSCROLL: u32 = 0x0080;

// Sizes in device-independent pixels, the same as the drafts.
const W: f32 = 640.0;
const HEADER: f32 = 56.0;
/// The edit box's left edge, after the magnifier, with no bubble before it.
const EDIT_X: f32 = 48.0;
/// The mode's bubble: its height, and the room inside it around its glyph and name.
const BUBBLE_H: f32 = 26.0;
const BUBBLE_GLYPH: f32 = 14.0;
const BUBBLE_PAD: f32 = 9.0;
const DEFAULT_CUE: &str = "Open an app, jump to a day, or type a command";
const FOOTER: f32 = 38.0;
const MAX_H: f32 = 520.0;
/// How much of the panel's paper covers the glass under the search row, and of the cards' on it.
const GLASS: f32 = 0.55;
const GLASS_CARD: f32 = 0.7;
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

pub(crate) const TEXT_FONT: &[u8] = include_bytes!("../../../node_modules/@fontsource-variable/instrument-sans/files/instrument-sans-latin-wght-normal.woff2");
const MONO_FONT: &[u8] = include_bytes!("../../../node_modules/@fontsource-variable/geist-mono/files/geist-mono-latin-wght-normal.woff2");

/// Kairo's two papers, deep blue ink and cream, the same values as the tokens in `src/styles.css`;
/// `accent` is the palette's own ink. The other grounds swap in their own paper and text inks
/// (`theme_for`).
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Theme {
    /// Whether it's drawn on a light ground, which takes the deeper of each app ink.
    pub(crate) light: bool,
    pub(crate) bg: u32,
    pub(crate) ink: u32,
    pub(crate) muted: u32,
    pub(crate) rule: u32,
    pub(crate) line: u32,
    pub(crate) raised: u32,
    pub(crate) productive: u32,
    pub(crate) neutral: u32,
    pub(crate) distracting: u32,
    pub(crate) warm: u32,
    /// A change for the better, in an answer.
    pub(crate) good: u32,
    /// The palette's own ink: selection, highlights, accented glyphs.
    pub(crate) accent: u32,
}

const DARK: Theme = Theme {
    light: false,
    bg: 0x141a24,
    ink: 0xe3e7ee,
    muted: 0x97a1b2,
    rule: 0x232c3d,
    line: 0x33405a,
    raised: 0x1b2230,
    productive: 0x3987e5,
    neutral: 0x4a505c,
    distracting: 0xd95926,
    warm: 0xf0a27c,
    good: 0x7fd18b,
    accent: 0xb9a2e6,
};

const LIGHT: Theme = Theme {
    light: true,
    bg: 0xf4efe6,
    ink: 0x2b2622,
    muted: 0x5e544a,
    rule: 0xe3d9c9,
    line: 0xd3c7b3,
    raised: 0xfffaf2,
    productive: 0x2f6fd0,
    neutral: 0xa9aeb6,
    distracting: 0xcf5220,
    // Warm and good are text here, so they take the web's text inks (--warm-ink, --good-ink).
    warm: 0xae4f1e,
    good: 0x267a37,
    accent: 0x6a4c93,
};

pub(crate) fn color(hex: u32, alpha: f32) -> D2D1_COLOR_F {
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
pub(crate) fn theme_for(mode: &str) -> Theme {
    let ground = crate::appearance::ground_of(mode).unwrap_or_else(|| crate::appearance::system_ground(platform_prefers_light()));
    let base = if ground.light { LIGHT } else { DARK };
    Theme { bg: ground.bg, raised: ground.raised, ink: ground.ink, muted: ground.muted, rule: ground.rule, line: ground.line, ..base }
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
    /// The hidden label before the edit box, which names it after the selected row.
    label: HWND,
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
    /// Whether Windows frosts what's behind the panel, under the search row: there the panel's
    /// paper is a tint. The search row stays solid, since Windows' edit box draws its text with
    /// no see-through to it and over the glass the text itself would vanish.
    glass: bool,
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
    /// The row whose card waits for Enter and was asked for, with its second line from before.
    asked: Option<(usize, Option<String>)>,
    error: Option<String>,
    /// Searches files for what's typed, on a thread of its own.
    worker: Worker,
    /// The number of the results shown, so files found for older ones are dropped.
    seq: u64,
    /// The mode it's in, drawn as a bubble before the edit box; None for an ordinary search.
    mode: Option<Mode>,
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

// ---------- files and second actions ----------

/// Adds files found for the results numbered `seq` after the rows, where they belong only while
/// those are still the results shown (`current`). Returns whether anything changed.
fn merge_files(items: &mut Vec<Item>, rows: &mut usize, seq: u64, current: u64, files: Vec<Item>) -> bool {
    if seq != current || files.is_empty() {
        return false;
    }
    let at = (*rows).min(items.len());
    *rows = at + files.len();
    items.splice(at..at, files);
    true
}

/// The label over row `index`: when typing, the best match leads as the top hit, unless it's a
/// file or the calculator's answer, which keep their own section; otherwise its section.
fn heading(typed: bool, index: usize, item: &Item) -> &'static str {
    if typed && index == 0 && item.section != "Files" && item.section != "Calculator" {
        "Top hit"
    } else {
        item.section
    }
}

/// What Ctrl+Enter does for the selected item, for the footer; None when it does nothing.
fn footer_hint(item: Option<&Item>) -> Option<&'static str> {
    item.and_then(|i| i.second.as_ref()).map(|(_, label)| *label)
}

// ---------- fonts ----------

/// Instrument Sans and Geist Mono from Kairo's own copies, for DirectWrite, and Instrument Sans's
/// raw data for the edit box, which draws with GDI. None where Windows won't take them; text then falls back to
/// the system font.
unsafe fn load_fonts(dwrite: &IDWriteFactory) -> (Option<IDWriteFontCollection>, bool) {
    if dwrite.cast::<IDWriteFactory5>().is_err() {
        return (None, false);
    }
    let collection = font_collection(dwrite, &[TEXT_FONT, MONO_FONT]);
    match collection {
        Ok((fonts, geist)) => {
            // GDI needs the face registered with it too, for the edit box.
            let mut count = 0u32;
            let gdi = !AddFontMemResourceEx(geist.as_ptr().cast(), geist.len() as u32, None, std::ptr::addr_of_mut!(count)).is_invalid();
            (Some(fonts), gdi)
        }
        Err(e) => {
            eprintln!("palette: Kairo's fonts unavailable, using the system font: {e}");
            (None, false)
        }
    }
}

/// Kairo's own WOFF2 fonts as a DirectWrite collection, and the first one's unpacked data (which
/// GDI may want too). Shared with the app ring.
pub(crate) unsafe fn font_collection(dwrite: &IDWriteFactory, packed: &[&[u8]]) -> Result<(IDWriteFontCollection, Vec<u8>)> {
    let factory = dwrite.cast::<IDWriteFactory5>()?;
    let loader = factory.CreateInMemoryFontFileLoader()?;
    factory.RegisterFontFileLoader(&loader)?;
    let builder = factory.CreateFontSetBuilder()?;
    let mut first = Vec::new();
    for (i, packed) in packed.iter().enumerate() {
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
            first = data;
        }
    }
    let set = builder.CreateFontSet()?;
    Ok((factory.CreateFontCollectionFromFontSet(&set)?.cast()?, first))
}

unsafe fn format(dwrite: &IDWriteFactory, fonts: Option<&IDWriteFontCollection>, mono: bool, weight: DWRITE_FONT_WEIGHT, size: f32) -> Result<IDWriteTextFormat> {
    let family = match (fonts.is_some(), mono) {
        (true, false) => w!("Instrument Sans"),
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
    // Windows 11 22H2 and later frost what's behind a popup like this one; earlier versions
    // refuse the request, and the palette stays solid paper.
    let backdrop = DWMSBT_TRANSIENTWINDOW;
    let glass = DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, (&backdrop as *const DWM_SYSTEMBACKDROP_TYPE).cast(), 4).is_ok();

    // An empty label just before the box: Windows names a box after the label before it, so this
    // one carries the selected row for screen readers (Palette::announce). It has no size, so it
    // never shows.
    let label = CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("Results"), WS_CHILD | WS_VISIBLE, 0, 0, 0, 0, Some(hwnd), None, Some(instance.into()), None)?;
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
    set_cue(edit, DEFAULT_CUE);

    let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
    let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
    let (fonts, gdi_geist) = load_fonts(&dwrite);
    let f = fonts.as_ref();
    let formats = Formats {
        title: format(&dwrite, f, false, DWRITE_FONT_WEIGHT_NORMAL, 14.0)?,
        sub: format(&dwrite, f, false, DWRITE_FONT_WEIGHT_NORMAL, 12.0)?,
        label: format(&dwrite, f, false, DWRITE_FONT_WEIGHT_SEMI_BOLD, 10.0)?,
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

    // Called on the search's own thread, so it only leaves the files for the palette's thread and
    // wakes it; before the window exists there's no one to show them to.
    let worker = Worker::new(files::ask_system, |seq, items| {
        let hwnd = WINDOW.load(Ordering::Acquire);
        if hwnd == 0 {
            return;
        }
        *FILES.lock().unwrap_or_else(|e| e.into_inner()) = Some((seq, items));
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_FILES, WPARAM(0), LPARAM(0));
        }
    });

    PALETTE.with(|p| {
        *p.borrow_mut() = Some(Palette {
            app,
            conn,
            hwnd,
            edit,
            label,
            d2d,
            dwrite,
            formats,
            stroke,
            dash,
            target: None,
            pixels: HashMap::new(),
            bitmaps: HashMap::new(),
            theme: DARK,
            glass,
            edit_font: None,
            edit_face: if gdi_geist { "Instrument Sans" } else { "Segoe UI" },
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
            asked: None,
            error: None,
            worker,
            seq: 0,
            mode: None,
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
        WM_RELOAD => {
            with(|p| {
                if IsWindowVisible(p.hwnd).as_bool() {
                    p.load();
                    p.refresh();
                }
            });
            LRESULT(0)
        }
        WM_FILES => {
            with(|p| {
                if let Some((seq, items)) = FILES.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    p.add_files(seq, items);
                }
            });
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
        // Tab takes a mode row, the way a completion is taken; elsewhere it moves down.
        (WM_KEYDOWN, k) if k == VK_TAB.0 && !shift => {
            with(|p| if p.items.get(p.selected).is_some_and(|i| matches!(i.action, Action::EnterMode(_))) { p.run(p.selected) } else { p.select_by(1) });
            LRESULT(0)
        }
        (WM_KEYDOWN, k) if k == VK_DOWN.0 => {
            with(|p| p.select_by(1));
            LRESULT(0)
        }
        (WM_KEYDOWN, k) if k == VK_UP.0 || (k == VK_TAB.0 && shift) => {
            with(|p| p.select_by(-1));
            LRESULT(0)
        }
        // Ctrl+Enter does an item's second thing, like showing a file in its folder.
        (WM_KEYDOWN, k) if k == VK_RETURN.0 => {
            with(|p| if ctrl { p.run_second(p.selected) } else { p.run(p.selected) });
            LRESULT(0)
        }
        // Out of the mode first; a second Esc puts the palette away.
        (WM_KEYDOWN, k) if k == VK_ESCAPE.0 => {
            with(|p| if p.mode.is_some() { p.leave_mode() } else { p.hide() });
            LRESULT(0)
        }
        // Backspace with nothing typed after the bubble takes it away.
        (WM_KEYDOWN, k) if k == VK_BACK.0 && !ctrl && edit_text(hwnd).is_empty() && with(|p| p.mode.is_some()).unwrap_or(false) => {
            with(|p| p.leave_mode());
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
        // Their characters would otherwise beep, or type a box for Ctrl+Backspace; Ctrl+Enter's
        // is a line feed.
        (WM_CHAR, 0x09 | 0x0a | 0x0d | 0x1b | 0x7f) => LRESULT(0),
        _ => DefSubclassProc(hwnd, msg, wparam, lparam),
    }
}

/// The edit box's hint while it's empty.
fn set_cue(edit: HWND, cue: &str) {
    let wide: Vec<u16> = cue.encode_utf16().chain([0]).collect();
    unsafe {
        SendMessageW(edit, EM_SETCUEBANNER, Some(WPARAM(1)), Some(LPARAM(wide.as_ptr() as isize)));
    }
}

/// The nearest item from `from` (itself included) in direction `step` that can be chosen, going
/// round: anything but a note. None when only notes are shown.
fn choosable(items: &[Item], from: i64, step: i32) -> Option<usize> {
    let n = items.len() as i64;
    (0..n).map(|k| (from + k * step as i64).rem_euclid(n.max(1)) as usize).find(|&i| !items[i].is_note())
}

/// What a screen reader is told for the selected row: its title, then its second line.
fn spoken_row(item: Option<&Item>) -> String {
    match item {
        Some(Item { title, sub: Some(sub), .. }) => format!("{title}, {sub}"),
        Some(item) => item.title.clone(),
        None => "No results".into(),
    }
}

/// The mode a short word typed at the start of `text` puts the palette in, and what's left after
/// it; only out of any mode, so "gh " typed in the calculator stays text.
fn take_shortcut(mode: Option<Mode>, text: &str) -> Option<(Mode, &str)> {
    if mode.is_some() {
        return None;
    }
    super::mode::shortcut(text)
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
        if self.bare() {
            return HEADER;
        }
        self.results_top() + self.results_h() + FOOTER
    }

    /// Compact, with nothing typed and no mode: the search row alone.
    fn bare(&self) -> bool {
        self.ctx.as_ref().is_some_and(|c| c.compact) && self.mode.is_none() && self.query().trim().is_empty()
    }

    fn query(&self) -> String {
        edit_text(self.edit)
    }

    // ---------- opening and closing ----------

    fn show(&mut self) {
        // The first time there's nothing to show yet; after that it opens at once with what it
        // gathered last time, and gathers afresh straight after (WM_RELOAD).
        let shown_before = self.ctx.is_some();
        if !shown_before {
            self.load();
        }
        let mode = crate::appearance::get(&self.conn).unwrap_or_default();
        self.set_theme(theme_for(&mode));
        // The look as chosen now, not as it was when the rest was gathered last time.
        if let Some(ctx) = &mut self.ctx {
            ctx.compact = super::compact(&self.conn).unwrap_or(false);
        }
        // Each opening starts out of any mode.
        self.set_mode(None);
        set_edit_text(self.edit, "");
        self.worker.opened();
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
            // Placed and given its glass while still hidden, then shown and painted in one go, so
            // it never appears undrawn or changes after it's seen.
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE);
            self.extend_glass(h);
            self.place_edit();
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_SHOWWINDOW);
            // Painted here rather than through UpdateWindow: this runs inside the palette's own
            // borrow, so a WM_PAINT sent from here would find it taken, skip, and still count as done.
            let _ = self.paint();
            // Windows only hands over the keyboard to an app the user just acted in; pressing
            // Alt counts, so a refused request gets that nudge and another try.
            if !SetForegroundWindow(self.hwnd).as_bool() || GetForegroundWindow() != self.hwnd {
                keybd_event(VK_MENU.0 as u8, 0, Default::default(), 0);
                keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
                let _ = SetForegroundWindow(self.hwnd);
            }
            let _ = SetFocus(Some(self.edit));
            let _ = InvalidateRect(Some(self.hwnd), None, false);
            if shown_before {
                let _ = PostMessageW(Some(self.hwnd), WM_RELOAD, WPARAM(0), LPARAM(0));
            }
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
        self.asked = None;
        // Files still coming for what was typed are dropped.
        self.seq += 1;
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
            // The glass follows the window's light or dark look, not Windows' own.
            let dark = windows::core::BOOL::from(!theme.light);
            let _ = DwmSetWindowAttribute(self.hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, (&dark as *const windows::core::BOOL).cast(), 4);
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
            let left = EDIT_X + self.bubble_w().map_or(0.0, |w| w + 8.0);
            let _ = SetWindowPos(self.edit, None, (left * s) as i32, (17.0 * s) as i32, ((W - left - 72.0) * s) as i32, (24.0 * s) as i32, SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }

    /// Fits the window to its content after results change.
    fn resize(&mut self) {
        let s = self.scale();
        let (w, h) = ((W * s).round() as i32, (self.height() * s).round() as i32);
        unsafe {
            // The drawing surface first, then the window, then the paint at once: left to the
            // message queue, the new size would show for a frame before anything is drawn in it.
            if let Some(t) = &self.target {
                let _ = t.Resize(&D2D_SIZE_U { width: w as u32, height: h as u32 });
            }
            self.extend_glass(h);
            let _ = SetWindowPos(self.hwnd, None, 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
            let _ = InvalidateRect(Some(self.hwnd), None, false);
            let _ = self.paint();
        }
    }

    /// Lets the glass through everywhere under the search row, the window being `height` pixels
    /// tall: Windows only heeds what's see-through in the part given over to it, from the bottom.
    fn extend_glass(&self, height: i32) {
        if !self.glass {
            return;
        }
        let header = (HEADER * self.scale()).round() as i32;
        let margins = MARGINS { cxLeftWidth: 0, cxRightWidth: 0, cyTopHeight: 0, cyBottomHeight: (height - header).max(0) };
        unsafe {
            let _ = DwmExtendFrameIntoClientArea(self.hwnd, &margins);
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
        let (ctx, footer) = super::context(&self.app, &self.conn, &self.app.state::<super::Index>(), super::open_windows());
        self.ctx = Some(ctx);
        self.footer = footer;
    }

    fn refresh(&mut self) {
        self.refresh_results();
        self.resize();
    }

    /// Results for what's typed, numbered afresh, with files asked for when they're wanted; they
    /// join the rows when found.
    fn refresh_results(&mut self) {
        let mut query = self.query();
        // A short word and a space ("gh ") becomes its bubble, leaving what came after it. The
        // box's own change notice comes while the palette is busy, so it asks nothing twice.
        if let Some((mode, rest)) = take_shortcut(self.mode, &query) {
            let rest = rest.to_string();
            self.set_mode(Some(mode));
            set_edit_text(self.edit, &rest);
            query = rest;
        }
        let Some(ctx) = &self.ctx else { return };
        // New rows: none has asked for its card.
        self.asked = None;
        self.seq += 1;
        if super::wants_files(ctx, &query, self.mode) {
            self.worker.ask_up_to(self.seq, query.clone(), super::files_wanted(self.mode));
        }
        if let Some(mode) = self.mode {
            self.items = search::search_in(ctx, mode, &query);
            self.rows = self.items.len();
            self.tiles = 0;
        } else if query.trim().is_empty() {
            let s = search::suggestions(ctx);
            self.rows = s.rows.len();
            self.tiles = s.tiles.len();
            self.items = s.rows.into_iter().chain(s.tiles).chain(s.chips).collect();
        } else {
            self.items = search::search(ctx, &query);
            self.rows = self.items.len();
            self.tiles = 0;
        }
        self.selected = choosable(&self.items, 0, 1).unwrap_or(0);
        self.scroll = 0.0;
        self.error = None;
        self.update_preview();
        self.announce();
        self.layout();
    }

    /// Files found for the results numbered `seq` join the end of the rows, if those are still
    /// shown; the selection and scroll stay where they were. In Files mode they take the place of
    /// its "Looking for files…" row, or say none were found.
    fn add_files(&mut self, seq: u64, items: Vec<Item>) {
        if self.mode == Some(Mode::Files) {
            if seq != self.seq {
                return;
            }
            self.items = super::files_mode_rows(&self.query(), items);
            self.asked = None;
            self.rows = self.items.len();
            self.selected = choosable(&self.items, 0, 1).unwrap_or(0);
            self.update_preview();
            self.layout();
            self.resize();
            self.announce();
            return;
        }
        if merge_files(&mut self.items, &mut self.rows, seq, self.seq, items) {
            self.layout();
            self.resize();
        }
    }

    /// A selected day gets a glance at how it went, above the results; a time chart waits until
    /// its row is asked with Enter or a click.
    fn update_preview(&mut self) {
        let Some(wanted) = self.items.get(self.selected).and_then(|i| i.preview.clone()) else {
            self.preview = None;
            return;
        };
        if wanted.on_enter() && self.asked.as_ref().map(|(i, _)| *i) != Some(self.selected) {
            self.preview = None;
            return;
        }
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
            let heading = heading(typed && self.mode.is_none(), i, item);
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

    /// Moves the selection by `step`, passing over notes; nowhere when only notes are shown.
    fn select_by(&mut self, step: i32) {
        if let Some(i) = choosable(&self.items, self.selected as i64 + step as i64, step.signum()) {
            self.select(i);
        }
    }

    fn select(&mut self, index: usize) {
        if index == self.selected {
            return;
        }
        let card_h = self.card_h();
        // Another row puts an asked-for chart away.
        self.put_away_asked();
        self.selected = index;
        self.update_preview();
        self.announce();
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
        // A chart asked for has moved the rows; the pointer passing over them doesn't take it
        // away. A click or the arrow keys still choose another row.
        if self.asked.is_some() {
            return;
        }
        if let Some(i) = self.hit(x, y).filter(|&i| !self.items[i].is_note()) {
            self.select(i);
        }
    }

    /// Restores the second line of the row whose chart was asked for, and forgets it.
    fn put_away_asked(&mut self) {
        if let Some((i, sub)) = self.asked.take() {
            if let Some(item) = self.items.get_mut(i) {
                item.sub = sub;
            }
        }
    }

    fn run(&mut self, index: usize) {
        let Some(item) = self.items.get(index).cloned().filter(|i| !i.is_note()) else { return };
        // A time chart shows on the first Enter or click; the next one does what the row does.
        if item.preview.as_ref().is_some_and(Preview::on_enter) && self.asked.as_ref().map(|(i, _)| *i) != Some(index) {
            self.select(index);
            let card_h = self.card_h();
            let sub = item.sub.clone();
            self.items[index].sub = Some(search::enter_again(sub.as_deref()));
            self.asked = Some((index, sub));
            self.update_preview();
            self.announce();
            if card_h != self.card_h() {
                self.resize();
            } else {
                self.repaint();
            }
            return;
        }
        // A mode row puts the palette in that mode, with nothing typed after its bubble yet.
        if let Action::EnterMode(mode) = item.action {
            self.set_mode(Some(mode));
            set_edit_text(self.edit, "");
            self.refresh();
            return;
        }
        // A switch flips where it is, and the palette stays open to show its new state.
        if item.action.keeps_palette_open() {
            if let Action::Toggle(switch) = item.action {
                self.flip(switch);
            }
            return;
        }
        self.perform(&item.action);
    }

    /// Tells screen readers the selected row: the hidden label before the edit box takes its words,
    /// and the box, which has the keyboard, is said to be renamed, so the row is read out.
    fn announce(&self) {
        let wide: Vec<u16> = spoken_row(self.items.get(self.selected)).encode_utf16().chain([0]).collect();
        unsafe {
            let _ = SetWindowTextW(self.label, PCWSTR(wide.as_ptr()));
            NotifyWinEvent(EVENT_OBJECT_NAMECHANGE, self.edit, OBJID_CLIENT.0, CHILDID_SELF as i32);
        }
    }

    /// Puts the palette in `mode`, or out of any: the bubble, the edit box moved after it, and
    /// the box's hint.
    fn set_mode(&mut self, mode: Option<Mode>) {
        self.mode = mode;
        set_cue(self.edit, &mode.map_or_else(|| DEFAULT_CUE.to_string(), |m| m.hint()));
        self.place_edit();
        self.repaint();
    }

    /// Back to an ordinary search of what's typed.
    fn leave_mode(&mut self) {
        self.set_mode(None);
        self.refresh();
    }

    /// The bubble's width, with the mode it shows; None out of any mode.
    fn bubble_w(&self) -> Option<f32> {
        self.mode.map(|m| BUBBLE_PAD + BUBBLE_GLYPH + 6.0 + self.measure(m.name(), &self.formats.chip) + BUBBLE_PAD + 2.0)
    }

    /// Does what Ctrl+Enter does for item `index`; nothing for an item without a second action.
    fn run_second(&mut self, index: usize) {
        let Some((action, _)) = self.items.get(index).and_then(|i| i.second.clone()) else { return };
        self.perform(&action);
    }

    /// Does what an item says and puts the palette away, or shows why it couldn't.
    fn perform(&mut self, action: &Action) {
        // Kairo's window, or a file dialog, opens over where the palette was.
        if action.opens_window() {
            self.hide();
        }
        match super::perform(&self.app, &self.conn, &self.app.state::<super::Index>(), action) {
            Ok(()) => self.hide(),
            Err(e) => self.fail(e),
        }
    }

    /// Flips a switch and shows the results again with its new state, the same row still selected.
    /// A switch that refuses keeps its state and says why on its row's second line.
    fn flip(&mut self, switch: Switch) {
        let refused = match settings::flip(&self.app, &self.conn, switch) {
            Ok(on) => {
                if let Some(ctx) = self.ctx.as_mut() {
                    super::set_switch(ctx, switch, on);
                }
                None
            }
            Err(why) => Some(why),
        };
        let (selected, scroll) = (self.selected, self.scroll);
        self.refresh_results();
        if let Some(why) = refused {
            let rows = self.rows.min(self.items.len());
            super::say_refused(&mut self.items[..rows], switch, &why);
        }
        self.selected = selected.min(self.items.len().saturating_sub(1));
        self.update_preview();
        self.scroll = scroll.clamp(0.0, (self.content_h - self.results_h()).max(0.0));
        self.resize();
    }

    /// Shows why an action failed, above the results.
    fn fail(&mut self, error: String) {
        self.error = Some(error);
        self.layout();
        self.resize();
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
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: if self.glass { D2D1_ALPHA_MODE_PREMULTIPLIED } else { D2D1_ALPHA_MODE_IGNORE },
                },
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
            // ClearType needs solid paper behind the text; on the glass it would fringe.
            if self.glass {
                t.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            }
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
                    let _ = spaced.SetCharacterSpacing(0.0, 1.4, 0.0, DWRITE_TEXT_RANGE { startPosition: 0, length: pos });
                }
            }
            let brush = self.brush(rt, hex, 1.0)?;
            rt.DrawTextLayout(v(x, y), &layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
            if !marked.is_empty() {
                let mut line = [DWRITE_LINE_METRICS::default()];
                let mut lines = 0;
                layout.GetLineMetrics(Some(&mut line), &mut lines)?;
                let under = y + line[0].baseline + 3.5;
                let accent = self.brush(rt, self.theme.accent, 1.0)?;
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
                // The search box's magnifier, and a web search's.
                None | Some(Glyph::Search) => {
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
                Some(Glyph::Calc) => {
                    let r = D2D1_ROUNDED_RECT { rect: rect(x + 5.0 * k, y + 3.0 * k, x + 19.0 * k, y + 21.0 * k), radiusX: 2.0 * k, radiusY: 2.0 * k };
                    rt.DrawRoundedRectangle(&r, &brush, width, stroke);
                    line((8.0, 7.0), (16.0, 7.0));
                    for (kx, ky) in [(8.5, 12.0), (14.5, 12.0), (8.5, 17.0), (14.5, 17.0)] {
                        line((kx, ky), (kx + 1.0, ky));
                    }
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
            Icon::App(path) | Icon::File(path) => {
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
                    Tone::Accent => (Some(t.accent), 0.16, t.accent),
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
        // On the glass, the search row's solid paper over the tint.
        let header = if self.glass { Some(self.brush(&rt, t.raised, 1.0)?) } else { None };
        unsafe {
            rt.SetDpi(self.dpi, self.dpi);
            rt.BeginDraw();
            rt.Clear(Some(&color(t.raised, if header.is_some() { GLASS } else { 1.0 })));
            if let Some(header) = &header {
                rt.FillRectangle(&rect(0.0, 0.0, W, HEADER), header);
            }
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
        // The mode's bubble before the box: its glyph and name in the palette's ink.
        if let (Some(mode), Some(width)) = (self.mode, self.bubble_w()) {
            let r = rect(EDIT_X, (HEADER - BUBBLE_H) / 2.0, EDIT_X + width, (HEADER + BUBBLE_H) / 2.0);
            let shape = D2D1_ROUNDED_RECT { rect: r, radiusX: BUBBLE_H / 2.0, radiusY: BUBBLE_H / 2.0 };
            unsafe { rt.FillRoundedRectangle(&shape, &self.brush(rt, t.accent, 0.18)?) };
            self.glyph(rt, Some(mode.glyph()), r.left + BUBBLE_PAD, (HEADER - BUBBLE_GLYPH) / 2.0, BUBBLE_GLYPH, t.accent)?;
            self.text(rt, mode.name(), &self.formats.chip, r.left + BUBBLE_PAD + BUBBLE_GLYPH + 6.0, HEADER / 2.0 - 8.0, width, t.accent, &[], false)?;
        }
        self.kbd(rt, "Esc", W - 18.0, HEADER / 2.0)?;
        if self.bare() {
            return Ok(());
        }
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
            // Small capitals, as the web palette and Kairo's pages draw section labels.
            self.text(rt, &label.to_uppercase(), &self.formats.label, 18.0, offset + y + 4.0, W - 36.0, t.muted, &[], false)?;
        }
        if let Some(error) = self.error.clone() {
            self.text(rt, &error, &self.formats.chip, 20.0, offset + 14.0, W - 40.0, t.warm, &[], false)?;
        }
        if self.items.is_empty() && self.mode.is_none() && !self.query().trim().is_empty() {
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
        let mut keys = vec![(&["↑", "↓"][..], "Move"), (&["Enter"][..], "Run")];
        // What Ctrl+Enter does for the selected item, when it does something.
        if let Some(hint) = footer_hint(self.items.get(self.selected)) {
            keys.push((&["Ctrl Enter"][..], hint));
        }
        for (keys, label) in keys {
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
        let alpha = if self.glass { GLASS_CARD } else { 1.0 };
        unsafe { rt.FillRoundedRectangle(&card, &self.brush(rt, self.theme.bg, alpha)?) };
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
        // A note is never drawn as chosen, and its words are quieter.
        let selected = selected && !item.is_note();
        let ink = if item.is_note() { t.muted } else { t.ink };
        if selected {
            unsafe { rt.FillRoundedRectangle(&D2D1_ROUNDED_RECT { rect: r, radiusX: 8.0, radiusY: 8.0 }, &self.brush(rt, t.accent, 0.18)?) };
        }
        let mid = (r.top + r.bottom) / 2.0;
        // Icons sit in a 28-pixel box; an app's own is 24 within it.
        let icon_x = if matches!(item.icon, Icon::App(_) | Icon::File(_)) { r.left + 14.0 } else { r.left + 12.0 };
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
                self.text(rt, &item.title, &self.formats.title, x, mid - 18.0, right - x, ink, &item.at, false)?;
                self.text(rt, sub, &self.formats.sub, x, mid + 1.0, right - x, t.muted, &[], false)?;
            }
            None => {
                self.text(rt, &item.title, &self.formats.title, x, mid - 9.5, right - x, ink, &item.at, false)?;
            }
        }
        Ok(())
    }

    fn draw_tile(&mut self, rt: &ID2D1RenderTarget, item: &Item, r: D2D_RECT_F, selected: bool) -> Result<()> {
        let t = self.theme;
        let shape = D2D1_ROUNDED_RECT { rect: r, radiusX: 8.0, radiusY: 8.0 };
        unsafe {
            if selected {
                rt.FillRoundedRectangle(&shape, &self.brush(rt, t.accent, 0.18)?);
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
                rt.FillRoundedRectangle(&shape, &self.brush(rt, t.accent, 0.12)?);
            }
            rt.DrawRoundedRectangle(&shape, &self.brush(rt, if selected { t.accent } else { t.line }, 1.0)?, 1.0, None);
        }
        self.text(rt, &item.title, &self.formats.chip, r.left + 13.0, r.top + 7.0, r.right - r.left, t.ink, &[], false)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG's contrast ratio between two colours.
    fn contrast(a: u32, b: u32) -> f64 {
        let lum = |hex: u32| {
            let ch = |c: u32| {
                let c = c as f64 / 255.0;
                if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * ch((hex >> 16) & 255) + 0.7152 * ch((hex >> 8) & 255) + 0.0722 * ch(hex & 255)
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn its_fonts_are_kairos_bundled_woff2_files() {
        for font in [TEXT_FONT, MONO_FONT] {
            assert_eq!(&font[..4], b"wOF2");
        }
    }

    #[test]
    fn every_ground_keeps_its_text_readable() {
        for g in crate::appearance::GROUNDS {
            let t = theme_for(g.id);
            assert_eq!(t.light, g.light, "{}", g.id);
            for paper in [t.bg, t.raised] {
                assert!(contrast(t.ink, paper) >= 7.0, "{}: ink on {paper:06x}", g.id);
                assert!(contrast(t.muted, paper) >= 4.5, "{}: muted on {paper:06x}", g.id);
                assert!(contrast(t.accent, paper) >= 3.0, "{}: accent on {paper:06x}", g.id);
            }
        }
    }

    #[test]
    fn both_papers_keep_their_text_readable() {
        for t in [DARK, LIGHT] {
            for paper in [t.bg, t.raised] {
                assert!(contrast(t.ink, paper) >= 7.0, "ink on {paper:06x}");
                assert!(contrast(t.muted, paper) >= 4.5, "muted on {paper:06x}");
                assert!(contrast(t.warm, paper) >= 4.5, "warm on {paper:06x}");
                assert!(contrast(t.good, paper) >= 4.5, "good on {paper:06x}");
                assert!(contrast(t.accent, paper) >= 3.0, "accent on {paper:06x}");
            }
        }
        // The new papers, as in styles.css.
        assert_eq!((DARK.bg, DARK.raised, DARK.accent), (0x141a24, 0x1b2230, 0xb9a2e6));
        assert_eq!((LIGHT.bg, LIGHT.raised, LIGHT.accent), (0xf4efe6, 0xfffaf2, 0x6a4c93));
    }

    fn row(title: &str) -> Item {
        Item::new(title, Icon::Glyph(Glyph::Today, Tone::Plain), "Go to", Action::Nothing)
    }

    fn file(name: &str) -> Item {
        let path = format!(r"C:\Users\me\{name}");
        let mut item = Item::new(name, Icon::File(path.clone()), "Files", Action::OpenFile(path.clone()));
        item.second = Some((Action::RevealFile(path), "Show in folder"));
        item
    }

    #[test]
    fn files_join_the_end_of_the_rows() {
        let mut items = vec![row("Today"), row("Budget")];
        let mut rows = 2;
        assert!(merge_files(&mut items, &mut rows, 7, 7, vec![file("budget.xlsx"), file("budget.pdf")]));
        let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["Today", "Budget", "budget.xlsx", "budget.pdf"]);
        assert_eq!(rows, 4);
    }

    #[test]
    fn files_go_before_tiles_and_chips() {
        let mut items = vec![row("Today"), row("Tile"), row("Chip")];
        let mut rows = 1;
        assert!(merge_files(&mut items, &mut rows, 3, 3, vec![file("notes.txt")]));
        let titles: Vec<&str> = items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["Today", "notes.txt", "Tile", "Chip"]);
        assert_eq!(rows, 2);
    }

    #[test]
    fn files_for_older_results_are_dropped() {
        let mut items = vec![row("Today")];
        let mut rows = 1;
        assert!(!merge_files(&mut items, &mut rows, 6, 7, vec![file("budget.xlsx")]));
        assert_eq!(items.len(), 1);
        assert_eq!(rows, 1);
    }

    #[test]
    fn no_files_change_nothing() {
        let mut items = vec![row("Today")];
        let mut rows = 1;
        assert!(!merge_files(&mut items, &mut rows, 7, 7, Vec::new()));
        assert_eq!(items.len(), 1);
        assert_eq!(rows, 1);
    }

    #[test]
    fn a_file_or_an_answer_is_never_the_top_hit() {
        assert_eq!(heading(true, 0, &row("Today")), "Top hit");
        assert_eq!(heading(true, 0, &file("budget.xlsx")), "Files");
        let sum = Item::new("87", Icon::Glyph(Glyph::Calc, Tone::Accent), "Calculator", Action::Copy("87".into()));
        assert_eq!(heading(true, 0, &sum), "Calculator");
        assert_eq!(heading(false, 0, &row("Today")), "Go to");
        assert_eq!(heading(true, 1, &row("Today")), "Go to");
    }

    #[test]
    fn a_refused_switch_says_why_on_its_row() {
        let mut nudges = Item::new("Focus nudges", Icon::Glyph(Glyph::Settings, Tone::Plain), "Settings", Action::Toggle(Switch::Nudges));
        nudges.sub = Some("Chrono settings".into());
        nudges.detail = Some("On".into());
        let mut items = vec![row("Today"), nudges];
        super::super::say_refused(&mut items, Switch::Nudges, "Not now.");
        assert_eq!(items[1].sub.as_deref(), Some("Not now."));
        assert_eq!(items[1].detail.as_deref(), Some("On"));
        assert_eq!(items[0].sub, None);
    }

    #[test]
    fn the_footer_names_the_second_action() {
        assert_eq!(footer_hint(Some(&file("budget.xlsx"))), Some("Show in folder"));
    }

    #[test]
    fn a_short_word_becomes_a_bubble_only_out_of_any_mode() {
        assert_eq!(take_shortcut(None, "gh tauri"), Some((Mode::Web(4), "tauri")));
        assert_eq!(take_shortcut(None, "gh"), None);
        // In the calculator, "g " is just text.
        assert_eq!(take_shortcut(Some(Mode::Calc), "g 2"), None);
    }

    #[test]
    fn notes_are_passed_over_and_never_chosen_alone() {
        let note = || Item::new("Not a sum yet", Icon::Glyph(Glyph::Calc, Tone::Plain), "Calculator", Action::Nothing);
        let go = |title: &str| Item::new(title, Icon::Glyph(Glyph::Today, Tone::Plain), "Go to", Action::Go("today", None));
        let items = vec![note(), go("Today"), go("Week")];
        assert_eq!(choosable(&items, 0, 1), Some(1));
        assert_eq!(choosable(&items, 3, 1), Some(1));
        assert_eq!(choosable(&items, 0, -1), Some(2));
        assert_eq!(choosable(&[note()], 0, 1), None);
        assert_eq!(choosable(&[], 0, 1), None);
    }

    #[test]
    fn a_screen_reader_hears_the_selected_rows_title_then_its_second_line() {
        let mut item = row("Budget.xlsx");
        assert_eq!(spoken_row(Some(&item)), "Budget.xlsx");
        item.sub = Some("Documents".into());
        assert_eq!(spoken_row(Some(&item)), "Budget.xlsx, Documents");
        assert_eq!(spoken_row(None), "No results");
    }

    #[test]
    fn the_footer_has_no_hint_without_a_second_action() {
        assert_eq!(footer_hint(Some(&row("Today"))), None);
        assert_eq!(footer_hint(None), None);
    }
}
