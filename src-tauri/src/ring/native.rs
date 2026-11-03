//! The app ring drawn natively on Windows: a see-through, top-most popup on its own thread,
//! painted with Direct2D into a bitmap with per-pixel transparency (a layered window), so the
//! ring shows and nothing around it. No web view: it opens at once and costs a few megabytes,
//! and its drawing resources go whenever it hides.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Instant;

use rusqlite::Connection;
use tauri::AppHandle;
use windows::core::{w, Interface, Result, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_F,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory, ID2D1PathGeometry, ID2D1RenderTarget, ID2D1StrokeStyle, D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_SMALL, D2D1_CAP_STYLE_ROUND,
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT, D2D1_LINE_JOIN_ROUND, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_SOFTWARE, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_EXTRA_BOLD, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_WORD_WRAPPING_NO_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetMonitorInfoW, MonitorFromPoint, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::NotifyWinEvent;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, GetAsyncKeyState, GetKeyState, ReleaseCapture, SetCapture, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_LWIN, VK_MENU, VK_RETURN,
    VK_RIGHT, VK_SHIFT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetCursorPos, GetForegroundWindow, GetMessageW, IsWindowVisible, KillTimer, LoadCursorW, PostMessageW,
    RegisterClassExW, SetForegroundWindow, SetTimer, SetWindowTextW, ShowWindow, TranslateMessage, UpdateLayeredWindow, CHILDID_SELF, EVENT_OBJECT_NAMECHANGE, HWND_TOPMOST, IDC_ARROW, MSG,
    OBJID_WINDOW, SWP_NOACTIVATE, SW_HIDE,
    SW_SHOW, ULW_ALPHA, WA_INACTIVE, WM_ACTIVATE, WM_APP, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_SYSKEYDOWN, WM_TIMER, WNDCLASSEXW,
    WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows_numerics::{Matrix3x2, Vector2};

use super::{Release, Ring, Segment, CENTRE, DRAG_PX, HEIGHT, INNER, OUTER, PROUD, SEGMENTS, STEP, WIDTH};
use crate::palette::native::{color, font_collection, theme_for, Theme, TEXT_FONT};

/// Fraunces, for the lit segment's name, as on the web ring.
const DISPLAY_FONT: &[u8] = include_bytes!("../../../node_modules/@fontsource-variable/fraunces/files/fraunces-latin-wght-normal.woff2");

static WINDOW: AtomicIsize = AtomicIsize::new(0);
const WM_OPEN_OR_TURN: u32 = WM_APP + 1;
/// Watches the modifier and moves the turning animation along.
const TICK: usize = 1;
const TICK_MS: u32 = 16;
const TURN_MS: f32 = 140.0;

/// The app inks of a theme, as `src/styles.css` has them.
#[derive(Clone, Copy)]
struct Inks {
    chrono: u32,
    ledger: u32,
    palette: u32,
    kairo: u32,
    ribbon: u32,
    track: u32,
}

const DARK_INKS: Inks = Inks { chrono: 0x7fc4e8, ledger: 0xe8a86a, palette: 0xb9a2e6, kairo: 0xa7b0bf, ribbon: 0xd9b23f, track: 0x232c3d };
const LIGHT_INKS: Inks = Inks { chrono: 0x2f6f8f, ledger: 0xa35a1f, palette: 0x6a4c93, kairo: 0x7a6f63, ribbon: 0xd9b23f, track: 0xe6dccb };

/// `a` toward `b` by `t`.
fn mix(a: u32, b: u32, t: f32) -> u32 {
    let ch = |shift: u32| {
        let (x, y) = (((a >> shift) & 255) as f32, ((b >> shift) & 255) as f32);
        ((x + (y - x) * t).round() as u32) << shift
    };
    ch(16) | ch(8) | ch(0)
}

fn v(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}

const NAMES: [&str; 7] = ["Chrono", "Ledger", "Palette", "Dashboard", "New task", "New note", "Focus 25"];

/// Everything the drawing needs: the ring, what each segment says, and how far a turn has got.
struct Scene {
    ring: Ring,
    lines: [String; 7],
    focus_running: bool,
    /// Degrees the ring is still to turn, easing to 0.
    shift: f32,
    theme: Theme,
    inks: Inks,
}

/// Direct2D and DirectWrite, made once per thread.
struct Painter {
    d2d: ID2D1Factory,
    stroke: ID2D1StrokeStyle,
    name: IDWriteTextFormat,
    line: IDWriteTextFormat,
}

impl Painter {
    unsafe fn new() -> Result<Painter> {
        let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let loaded = font_collection(&dwrite, &[DISPLAY_FONT, TEXT_FONT]);
        let fonts = loaded.map(|(c, _)| c).ok();
        let format = |family: windows::core::PCWSTR, weight: DWRITE_FONT_WEIGHT, size: f32, fallback: windows::core::PCWSTR| -> Result<IDWriteTextFormat> {
            let (family, fonts): (_, Option<&IDWriteFontCollection>) = if fonts.is_some() { (family, fonts.as_ref()) } else { (fallback, None) };
            let f = dwrite.CreateTextFormat(family, fonts, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!("en-us"))?;
            f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            Ok(f)
        };
        let stroke = d2d.CreateStrokeStyle(
            &D2D1_STROKE_STYLE_PROPERTIES { startCap: D2D1_CAP_STYLE_ROUND, endCap: D2D1_CAP_STYLE_ROUND, lineJoin: D2D1_LINE_JOIN_ROUND, ..Default::default() },
            None,
        )?;
        Ok(Painter {
            // DirectWrite files this cut of Fraunces under its default style's name.
            name: format(w!("Fraunces NonWonky"), DWRITE_FONT_WEIGHT_EXTRA_BOLD, 16.0, w!("Georgia"))?,
            line: format(w!("Instrument Sans"), DWRITE_FONT_WEIGHT_NORMAL, 11.0, w!("Segoe UI"))?,
            stroke,
            d2d,
        })
    }

    /// The ring at `scale`, as premultiplied BGRA pixels in a DIB selected into a memory DC.
    unsafe fn render(&self, scene: &Scene, scale: f32) -> Result<Frame> {
        let (w, h) = ((WIDTH * scale).round() as i32, (HEIGHT * scale).round() as i32);
        let frame = Frame::new(w, h)?;
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let target: ID2D1DCRenderTarget = self.d2d.CreateDCRenderTarget(&props)?;
        target.BindDC(frame.dc, &RECT { left: 0, top: 0, right: w, bottom: h })?;
        let rt: ID2D1RenderTarget = target.cast()?;
        rt.BeginDraw();
        rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
        rt.SetTransform(&Matrix3x2 { M11: scale, M12: 0.0, M21: 0.0, M22: scale, M31: 0.0, M32: 0.0 });
        self.draw(&rt, scene, scale)?;
        rt.EndDraw(None, None)?;
        Ok(frame)
    }

    unsafe fn draw(&self, rt: &ID2D1RenderTarget, scene: &Scene, scale: f32) -> Result<()> {
        let (t, inks) = (scene.theme, scene.inks);
        // Back to front: the far ones first, the lit one last so its outline sits on top.
        let mut order: Vec<i32> = (-3..=3).collect();
        order.sort_by(|a, b| (*b as f32 * STEP + scene.shift).abs().total_cmp(&(*a as f32 * STEP + scene.shift).abs()));
        for k in order {
            let deg = k as f32 * STEP + scene.shift;
            let near = (deg.abs() / STEP).min(3.0);
            // 1 at the top, 0.5 one away, 0.22 two away, then gone.
            let alpha = match near {
                n if n <= 1.0 => 1.0 - 0.5 * n,
                n if n <= 2.0 => 0.5 - 0.28 * (n - 1.0),
                n => (0.22 - 0.22 * (n - 2.0)).max(0.0),
            };
            if alpha <= 0.01 {
                continue;
            }
            let lit = (1.0 - near).max(0.0);
            let segment = scene.ring.at(k);
            let ink = ink(segment, inks);
            let shape = self.band(deg, OUTER + PROUD * lit)?;
            // A soft shadow under each, then the paper, then its edge.
            rt.SetTransform(&Matrix3x2 { M11: scale, M12: 0.0, M21: 0.0, M22: scale, M31: 0.0, M32: 4.0 * scale });
            rt.FillGeometry(&shape, &rt.CreateSolidColorBrush(&color(0x000000, 0.22 * alpha), None)?, None);
            rt.SetTransform(&Matrix3x2 { M11: scale, M12: 0.0, M21: 0.0, M22: scale, M31: 0.0, M32: 0.0 });
            let paper = mix(t.raised, t.ink, 0.12 * lit);
            let dim = if segment == Segment::Focus && scene.focus_running { 0.6 } else { 1.0 };
            rt.FillGeometry(&shape, &rt.CreateSolidColorBrush(&color(paper, alpha * dim), None)?, None);
            let edge = if lit > 0.5 { ink } else { t.line };
            rt.DrawGeometry(&shape, &rt.CreateSolidColorBrush(&color(edge, alpha), None)?, 1.5 + 1.0 * lit, None);

            // Its mark, and for the one at the top, its name and line.
            let r = (OUTER + INNER) / 2.0 + 8.0 * lit;
            let (x, y) = (CENTRE.0 + r * deg.to_radians().sin(), CENTRE.1 - r * deg.to_radians().cos());
            let size = 26.0 + 8.0 * lit;
            let mark_y = if lit > 0.0 { y - 22.0 * lit } else { y };
            self.mark(rt, segment, x, mark_y, size, alpha * if lit > 0.0 { 1.0 } else { 0.55 }, scale, &t, &inks)?;
            if lit > 0.6 {
                let a = (lit - 0.6) / 0.4;
                let index = SEGMENTS.iter().position(|s| *s == segment).unwrap_or(0);
                self.text(rt, NAMES[index], &self.name, x, mark_y + size / 2.0 + 2.0, t.ink, a)?;
                self.text(rt, &scene.lines[index], &self.line, x, mark_y + size / 2.0 + 22.0, t.muted, a)?;
            }
        }
        Ok(())
    }

    /// A band of the ring `deg` degrees round from the top, reaching out to `outer`.
    unsafe fn band(&self, deg: f32, outer: f32) -> Result<ID2D1PathGeometry> {
        let gap = 0.6;
        let (from, to) = ((deg - STEP / 2.0 + gap).to_radians(), (deg + STEP / 2.0 - gap).to_radians());
        let at = |r: f32, a: f32| v(CENTRE.0 + r * a.sin(), CENTRE.1 - r * a.cos());
        let path = self.d2d.CreatePathGeometry()?;
        let sink = path.Open()?;
        sink.BeginFigure(at(outer, from), D2D1_FIGURE_BEGIN_FILLED);
        sink.AddArc(&D2D1_ARC_SEGMENT {
            point: at(outer, to),
            size: D2D_SIZE_F { width: outer, height: outer },
            rotationAngle: 0.0,
            sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
            arcSize: D2D1_ARC_SIZE_SMALL,
        });
        sink.AddLine(at(INNER, to));
        sink.AddArc(&D2D1_ARC_SEGMENT {
            point: at(INNER, from),
            size: D2D_SIZE_F { width: INNER, height: INNER },
            rotationAngle: 0.0,
            sweepDirection: D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
            arcSize: D2D1_ARC_SIZE_SMALL,
        });
        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
        sink.Close()?;
        Ok(path)
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn text(&self, rt: &ID2D1RenderTarget, s: &str, format: &IDWriteTextFormat, cx: f32, top: f32, hex: u32, alpha: f32) -> Result<()> {
        let wide: Vec<u16> = s.encode_utf16().collect();
        let brush = rt.CreateSolidColorBrush(&color(hex, alpha), None)?;
        let r = D2D_RECT_F { left: cx - 90.0, top, right: cx + 90.0, bottom: top + 24.0 };
        rt.DrawText(&wide, format, &r, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE, Default::default());
        Ok(())
    }

    /// A segment's logo, drawn as `src/logos.ts` draws it, in a `size` box centred on (cx, cy).
    #[allow(clippy::too_many_arguments)]
    unsafe fn mark(&self, rt: &ID2D1RenderTarget, segment: Segment, cx: f32, cy: f32, size: f32, alpha: f32, scale: f32, t: &Theme, inks: &Inks) -> Result<()> {
        // The drawings use a 100-unit box.
        let k = size / 100.0;
        rt.SetTransform(&Matrix3x2 { M11: k * scale, M12: 0.0, M21: 0.0, M22: k * scale, M31: (cx - size / 2.0) * scale, M32: (cy - size / 2.0) * scale });
        let brush = |hex: u32, a: f32| rt.CreateSolidColorBrush(&color(hex, a * alpha), None);
        let line = |a: (f32, f32), b: (f32, f32), hex: u32, w: f32| -> Result<()> {
            rt.DrawLine(v(a.0, a.1), v(b.0, b.1), &brush(hex, 1.0)?, w, &self.stroke);
            Ok(())
        };
        let circle = |x: f32, y: f32, r: f32| D2D1_ELLIPSE { point: v(x, y), radiusX: r, radiusY: r };
        let rounded = |l: f32, top: f32, r: f32, b: f32, rad: f32| D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: l, top, right: r, bottom: b }, radiusX: rad, radiusY: rad };
        match segment {
            Segment::Chrono => {
                rt.FillEllipse(&circle(50.0, 50.0, 44.0), &brush(t.raised, 1.0)?);
                rt.DrawEllipse(&circle(50.0, 50.0, 44.0), &brush(inks.chrono, 1.0)?, 5.0, None);
                rt.DrawEllipse(&circle(50.0, 50.0, 31.0), &brush(inks.track, 1.0)?, 9.0, None);
                // A third of the ring, from the top.
                let path = self.d2d.CreatePathGeometry()?;
                let sink = path.Open()?;
                sink.BeginFigure(v(50.0, 19.0), Default::default());
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: v(50.0 + 31.0 * 120f32.to_radians().sin(), 50.0 - 31.0 * 120f32.to_radians().cos()),
                    size: D2D_SIZE_F { width: 31.0, height: 31.0 },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                sink.Close()?;
                rt.DrawGeometry(&path, &brush(inks.chrono, 1.0)?, 9.0, &self.stroke);
                line((50.0, 50.0), (50.0, 27.0), t.ink, 5.0)?;
                line((50.0, 50.0), (63.0, 58.0), t.ink, 5.0)?;
            }
            Segment::Ledger => {
                rt.FillRoundedRectangle(&rounded(20.0, 10.0, 82.0, 90.0, 6.0), &brush(inks.ledger, 1.0)?);
                rt.FillRoundedRectangle(&rounded(20.0, 10.0, 31.0, 90.0, 3.0), &brush(mix(inks.ledger, 0x000000, 0.28), 1.0)?);
                let ribbon = self.d2d.CreatePathGeometry()?;
                let sink = ribbon.Open()?;
                sink.BeginFigure(v(64.0, 6.0), D2D1_FIGURE_BEGIN_FILLED);
                for p in [(74.0, 6.0), (74.0, 40.0), (69.0, 34.0), (64.0, 40.0)] {
                    sink.AddLine(v(p.0, p.1));
                }
                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                sink.Close()?;
                rt.FillGeometry(&ribbon, &brush(inks.ribbon, 1.0)?, None);
                let paper = mix(inks.ledger, 0xffffff, 0.7);
                for (y, end) in [(40.0, 70.0), (52.0, 70.0), (64.0, 60.0)] {
                    rt.DrawLine(v(40.0, y), v(end, y), &brush(paper, 0.7)?, 4.0, &self.stroke);
                }
            }
            Segment::Palette => {
                rt.FillRoundedRectangle(&rounded(12.0, 30.0, 88.0, 82.0, 13.0), &brush(mix(inks.palette, 0x000000, 0.45), 1.0)?);
                rt.FillRoundedRectangle(&rounded(12.0, 22.0, 88.0, 74.0, 13.0), &brush(t.raised, 1.0)?);
                rt.DrawRoundedRectangle(&rounded(12.0, 22.0, 88.0, 74.0, 13.0), &brush(inks.palette, 1.0)?, 4.0, None);
                line((34.0, 48.0), (66.0, 48.0), inks.palette, 5.0)?;
                line((42.0, 40.0), (34.0, 48.0), inks.palette, 5.0)?;
                line((34.0, 48.0), (42.0, 56.0), inks.palette, 5.0)?;
            }
            Segment::Dashboard => {
                // Kairo's own mark, as its app icon: a whole ring, one stretch at the top lit gold.
                rt.DrawEllipse(&circle(50.0, 50.0, 37.0), &brush(t.ink, 1.0)?, 13.0, None);
                let path = self.d2d.CreatePathGeometry()?;
                let sink = path.Open()?;
                let half = 45f32.to_radians();
                sink.BeginFigure(v(50.0 - 37.0 * half.sin(), 50.0 - 37.0 * half.cos()), Default::default());
                sink.AddArc(&D2D1_ARC_SEGMENT {
                    point: v(50.0 + 37.0 * half.sin(), 50.0 - 37.0 * half.cos()),
                    size: D2D_SIZE_F { width: 37.0, height: 37.0 },
                    rotationAngle: 0.0,
                    sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                    arcSize: D2D1_ARC_SIZE_SMALL,
                });
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                sink.Close()?;
                // Flat ends, as in the icon.
                rt.DrawGeometry(&path, &brush(inks.ribbon, 1.0)?, 13.0, None);
            }
            Segment::Task => {
                line((50.0, 22.0), (50.0, 78.0), inks.ledger, 10.0)?;
                line((22.0, 50.0), (78.0, 50.0), inks.ledger, 10.0)?;
            }
            Segment::Note => {
                let pen = self.d2d.CreatePathGeometry()?;
                let sink = pen.Open()?;
                sink.BeginFigure(v(28.0, 74.0), D2D1_FIGURE_BEGIN_FILLED);
                for p in [(34.0, 54.0), (68.0, 20.0), (82.0, 34.0), (48.0, 68.0)] {
                    sink.AddLine(v(p.0, p.1));
                }
                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                sink.Close()?;
                rt.DrawGeometry(&pen, &brush(inks.ledger, 1.0)?, 7.0, &self.stroke);
                line((28.0, 80.0), (72.0, 80.0), t.ink, 5.0)?;
            }
            Segment::Focus => {
                rt.DrawEllipse(&circle(50.0, 54.0, 32.0), &brush(inks.chrono, 1.0)?, 7.0, None);
                line((50.0, 54.0), (50.0, 36.0), t.ink, 7.0)?;
                line((42.0, 14.0), (58.0, 14.0), t.ink, 7.0)?;
            }
        }
        rt.SetTransform(&Matrix3x2 { M11: scale, M12: 0.0, M21: 0.0, M22: scale, M31: 0.0, M32: 0.0 });
        Ok(())
    }
}

/// The colour a segment is outlined in when lit.
fn ink(segment: Segment, inks: Inks) -> u32 {
    match segment {
        Segment::Chrono | Segment::Focus => inks.chrono,
        Segment::Ledger | Segment::Task | Segment::Note => inks.ledger,
        Segment::Palette => inks.palette,
        Segment::Dashboard => inks.kairo,
    }
}

/// A 32-bit top-down DIB in a memory DC, the layered window's picture.
struct Frame {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    width: i32,
    height: i32,
}

impl Frame {
    unsafe fn new(width: i32, height: i32) -> Result<Frame> {
        let dc = CreateCompatibleDC(None);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
        let old = SelectObject(dc, bitmap.into());
        Ok(Frame { dc, bitmap, old, bits: bits.cast(), width, height })
    }

    /// The pixels as straight (not premultiplied) RGBA, for a PNG.
    unsafe fn rgba(&self) -> Vec<u8> {
        let n = (self.width * self.height) as usize;
        let bgra = std::slice::from_raw_parts(self.bits, n * 4);
        let mut out = Vec::with_capacity(n * 4);
        for px in bgra.chunks_exact(4) {
            let a = px[3] as f32 / 255.0;
            let un = |c: u8| if a > 0.0 { ((c as f32 / a).round().min(255.0)) as u8 } else { 0 };
            out.extend_from_slice(&[un(px[2]), un(px[1]), un(px[0]), px[3]]);
        }
        out
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

// ---------- the window ----------

struct Popup {
    app: AppHandle,
    conn: Connection,
    hwnd: HWND,
    painter: Painter,
    scene: Scene,
    scale: f32,
    pos: POINT,
    opened: Instant,
    /// Let go of the modifier soon after opening: a tap, so the ring stays.
    tapped: bool,
    /// The shortcut's modifier key, watched for its release.
    modifier: VIRTUAL_KEY,
    modifier_was_down: bool,
    press: Option<(f32, f32, f32, i32)>,
}

thread_local! {
    static POPUP: RefCell<Option<Popup>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Popup) -> R) -> Option<R> {
    POPUP.with(|p| p.try_borrow_mut().ok().and_then(|mut p| p.as_mut().map(f)))
}

/// Opens the ring at the pointer, or turns it if it's open. Safe from any thread.
pub fn open_or_turn() {
    let hwnd = WINDOW.load(Ordering::Acquire);
    if hwnd != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_OPEN_OR_TURN, WPARAM(0), LPARAM(0));
        }
    }
}

/// Starts the ring's thread and its hidden window.
pub fn spawn(app: AppHandle, db_path: PathBuf) {
    std::thread::Builder::new()
        .name("kairo-ring".into())
        .spawn(move || {
            let conn = match crate::db::open(&db_path) {
                Ok(conn) => conn,
                Err(e) => return eprintln!("ring: cannot open database: {e}"),
            };
            if let Err(e) = unsafe { create(app, conn) } {
                return eprintln!("ring: cannot create the window: {e}");
            }
            let mut msg = MSG::default();
            unsafe {
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        })
        .expect("failed to start ring thread");
}

unsafe fn create(app: AppHandle, conn: Connection) -> Result<()> {
    let instance = GetModuleHandleW(None)?;
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(window_proc),
        hInstance: instance.into(),
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        lpszClassName: w!("KairoRing"),
        ..Default::default()
    };
    RegisterClassExW(&class);
    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        w!("KairoRing"),
        w!("Kairo app ring"),
        WS_POPUP,
        0,
        0,
        WIDTH as i32,
        HEIGHT as i32,
        None,
        None,
        Some(instance.into()),
        None,
    )?;
    let painter = Painter::new()?;
    let scene = Scene { ring: Ring::new(0), lines: Default::default(), focus_running: false, shift: 0.0, theme: theme_for("system"), inks: DARK_INKS };
    POPUP.with(|p| {
        *p.borrow_mut() = Some(Popup {
            app,
            conn,
            hwnd,
            painter,
            scene,
            scale: 1.0,
            pos: POINT::default(),
            opened: Instant::now(),
            tapped: false,
            modifier: VK_MENU,
            modifier_was_down: false,
            press: None,
        })
    });
    WINDOW.store(hwnd.0 as isize, Ordering::Release);
    Ok(())
}

/// Whether Windows is asked for less motion (Settings → Accessibility → Visual effects).
fn reduce_motion() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS};
    let mut on = windows::core::BOOL(1);
    unsafe {
        let _ = SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, Some((&mut on as *mut windows::core::BOOL).cast()), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0));
    }
    !on.as_bool()
}

/// The modifier of a shortcut as saved ("Alt+Q"): the key whose release opens the lit segment.
fn modifier_of(shortcut: &str) -> VIRTUAL_KEY {
    match shortcut.split('+').next().unwrap_or("") {
        "Ctrl" => VK_CONTROL,
        "Shift" => VK_SHIFT,
        "Super" => VK_LWIN,
        _ => VK_MENU,
    }
}

fn key_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(vk.0 as i32) < 0 }
}

impl Popup {
    fn show(&mut self) {
        let now = chrono::Utc::now().timestamp_millis();
        let mode = crate::appearance::get(&self.conn).unwrap_or_default();
        self.scene.theme = theme_for(&mode);
        self.scene.inks = if self.scene.theme.bg == theme_for("light").bg { LIGHT_INKS } else { DARK_INKS };
        self.scene.ring = Ring::new(super::last(&self.conn));
        self.scene.lines = super::lines(&self.conn, now);
        self.scene.focus_running = super::focus_refusal(&self.conn).is_some();
        self.scene.shift = 0.0;
        self.announce();
        self.modifier = modifier_of(&super::shortcut(&self.conn).unwrap_or_default());
        self.modifier_was_down = key_down(self.modifier);
        self.tapped = false;
        self.press = None;
        self.opened = Instant::now();
        unsafe {
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(monitor, &mut info);
            let (mut dpi, mut dpi_y) = (96u32, 96u32);
            let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y);
            self.scale = dpi as f32 / 96.0;
            let size = ((WIDTH * self.scale).round() as i32, (HEIGHT * self.scale).round() as i32);
            let work = info.rcWork;
            let (x, y) = super::place((cursor.x, cursor.y), (work.left, work.top, work.right, work.bottom), size);
            self.pos = POINT { x, y };
            self.paint();
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(self.hwnd, Some(HWND_TOPMOST), x, y, 0, 0, windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE | SWP_NOACTIVATE);
            // The shortcut's own press lets Kairo take the keyboard. If Windows refuses, the
            // usual nudge is a pressed and released Alt, which would read as letting go of the
            // shortcut, so it's only used once the modifier is up.
            if (!SetForegroundWindow(self.hwnd).as_bool() || GetForegroundWindow() != self.hwnd) && !self.modifier_was_down {
                keybd_event(VK_MENU.0 as u8, 0, Default::default(), 0);
                keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
                let _ = SetForegroundWindow(self.hwnd);
            }
            SetTimer(Some(self.hwnd), TICK, TICK_MS, None);
        }
    }

    fn hide(&mut self) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), TICK);
            let _ = ShowWindow(self.hwnd, SW_HIDE);
            let _ = ReleaseCapture();
        }
        self.press = None;
    }

    fn paint(&mut self) {
        let frame = match unsafe { self.painter.render(&self.scene, self.scale) } {
            Ok(frame) => frame,
            Err(e) => return eprintln!("ring: cannot draw: {e}"),
        };
        let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
        let size = SIZE { cx: frame.width, cy: frame.height };
        let origin = POINT::default();
        unsafe {
            let _ = UpdateLayeredWindow(self.hwnd, None, Some(&self.pos), Some(&size), Some(frame.dc), Some(&origin), COLORREF(0), Some(&blend), ULW_ALPHA);
        }
        // The frame (DIB and DC) goes now: nothing is kept between pictures.
    }

    fn turn(&mut self, steps: i32) {
        if steps == 0 {
            return;
        }
        self.scene.ring.turn(steps);
        self.announce();
        // Eases from where it was: the new top segment starts `steps` places round.
        self.scene.shift = if reduce_motion() { 0.0 } else { (self.scene.shift + steps as f32 * STEP).clamp(-2.0 * STEP, 2.0 * STEP) };
        self.paint();
    }

    /// Names the window after the lit segment ("Ledger, 3 due today") and tells screen readers it
    /// changed: the ring is drawn, not built from controls, so this is all they have to read.
    fn announce(&self) {
        let lit = self.scene.ring.lit;
        let name = super::spoken(NAMES[lit], &self.scene.lines[lit]);
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        unsafe {
            let _ = SetWindowTextW(self.hwnd, PCWSTR(wide.as_ptr()));
            NotifyWinEvent(EVENT_OBJECT_NAMECHANGE, self.hwnd, OBJID_WINDOW.0, CHILDID_SELF as i32);
        }
    }

    fn pick(&mut self, index: usize) {
        self.hide();
        let Some(segment) = SEGMENTS.get(index).copied() else { return };
        if let Err(why) = super::perform(&self.app, &self.conn, segment) {
            super::tell(&self.app, &why);
        }
    }

    fn tick(&mut self) {
        // The modifier let go: a pick after a hold, a tap otherwise.
        let down = key_down(self.modifier);
        if self.modifier_was_down && !down && !self.tapped {
            match super::on_modifier_up(self.opened.elapsed().as_millis() as i64) {
                Release::Pick => return self.pick(self.scene.ring.lit),
                Release::Stay => self.tapped = true,
            }
        }
        self.modifier_was_down = down;
        if self.scene.shift != 0.0 {
            let step = STEP * TICK_MS as f32 / TURN_MS;
            self.scene.shift = if self.scene.shift.abs() <= step { 0.0 } else { self.scene.shift - step * self.scene.shift.signum() };
            self.paint();
        }
    }

    fn local(&self, lparam: LPARAM) -> (f32, f32) {
        let x = (lparam.0 & 0xffff) as i16 as f32 / self.scale;
        let y = ((lparam.0 >> 16) & 0xffff) as i16 as f32 / self.scale;
        (x, y)
    }

    fn key(&mut self, vk: u16) {
        let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) < 0 };
        match VIRTUAL_KEY(vk) {
            VK_RIGHT | VK_DOWN => self.turn(1),
            VK_LEFT | VK_UP => self.turn(-1),
            VK_RETURN => self.pick(self.scene.ring.lit),
            VK_ESCAPE => self.hide(),
            // 1 to 7 open a segment directly.
            k if (0x31..=0x37).contains(&k.0) => self.pick((k.0 - 0x31) as usize),
            // The shortcut's key with Shift turns back (without Shift, Windows hands it to Kairo
            // as the shortcut, which turns on).
            k if shift && (0x41..=0x5a).contains(&k.0) => self.turn(-1),
            _ => {}
        }
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_OPEN_OR_TURN => {
            if IsWindowVisible(hwnd).as_bool() {
                with(|p| p.turn(1));
            } else {
                with(|p| p.show());
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == TICK => {
            with(|p| p.tick());
            LRESULT(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            with(|p| p.key(wparam.0 as u16));
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xffff) as i16;
            with(|p| p.turn(if delta < 0 { 1 } else { -1 }));
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            with(|p| {
                let (x, y) = p.local(lparam);
                p.press = Some((x, y, super::angle(x, y), 0));
                SetCapture(p.hwnd);
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            with(|p| {
                let Some((x0, y0, from, turned)) = p.press else { return };
                let (x, y) = p.local(lparam);
                if (x - x0).hypot(y - y0) < DRAG_PX && turned == 0 {
                    return;
                }
                let steps = super::drag_steps(from, super::angle(x, y));
                p.turn(steps - turned);
                p.press = Some((x0, y0, from, steps));
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            with(|p| {
                let _ = ReleaseCapture();
                let Some((x0, y0, _, turned)) = p.press.take() else { return };
                let (x, y) = p.local(lparam);
                // A drag only turns it: the ring stays for a click or Enter, and letting go of a
                // held modifier no longer opens it.
                if turned != 0 || (x - x0).hypot(y - y0) >= DRAG_PX {
                    p.tapped = true;
                    return;
                }
                if let Some(under) = p.scene.ring.hit(x, y) {
                    p.pick(SEGMENTS.iter().position(|s| *s == under).unwrap_or(0));
                }
            });
            LRESULT(0)
        }
        // Clicking anywhere else puts it away.
        WM_ACTIVATE if (wparam.0 & 0xffff) as u32 == WA_INACTIVE => {
            with(|p| p.hide());
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Draws the ring to a PNG and returns, for looking at it without showing it: `--ring-png <file>
/// [lit] [light|dark]`. Sample lines, no database.
pub fn render_png(path: &Path, lit: usize, mode: &str) -> std::result::Result<(), String> {
    let theme = theme_for(mode);
    let scene = Scene {
        ring: Ring::new(lit),
        lines: ["2h 05m today", "3 due today · 1 overdue", "Ctrl + Space", "Kairo", "In Ledger", "In Ledger", "25 minutes"].map(String::from),
        focus_running: false,
        shift: 0.0,
        theme,
        inks: if mode == "light" { LIGHT_INKS } else { DARK_INKS },
    };
    let pixels = unsafe {
        let painter = Painter::new().map_err(|e| e.to_string())?;
        let frame = painter.render(&scene, 2.0).map_err(|e| e.to_string())?;
        (frame.width as u32, frame.height as u32, frame.rgba())
    };
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), pixels.0, pixels.1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().and_then(|mut w| w.write_image_data(&pixels.2)).map_err(|e| e.to_string())
}
