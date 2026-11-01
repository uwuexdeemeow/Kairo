//! OS hooks: which window is in front, and how long since the last keyboard, mouse or controller input.

pub struct Foreground {
    /// Full path of the process that owns the foreground window.
    pub exe_path: String,
    pub title: String,
    /// The window itself, for minimizing it during a focus session (on a Mac, the app's process id).
    pub window: usize,
}

/// An open window a person could switch to, as the palette lists it. The id is the system's own
/// for the window, as text: a window handle on Windows, "<pid>:<window number>" on a Mac, an X11
/// id, or the GNOME extension's ("g…") or KWin's ("k…").
#[derive(Clone, Debug, PartialEq)]
pub struct OpenWindow {
    pub id: String,
    pub title: String,
    pub pid: u32,
    /// The program's path, for its name and icon; None when the system won't say.
    pub path: Option<String>,
}

/// Window titles past this are cut, so one odd window can't stretch the palette.
const MAX_WINDOW_TITLE: usize = 200;

/// The windows worth listing, in the order given: not Kairo's own, and with a title.
pub fn switchable(windows: Vec<OpenWindow>, own_pid: u32) -> Vec<OpenWindow> {
    windows
        .into_iter()
        .filter(|w| w.pid != own_pid)
        .filter_map(|w| {
            let title: String = w.title.trim().chars().take(MAX_WINDOW_TITLE).collect();
            (!title.is_empty()).then_some(OpenWindow { title, ..w })
        })
        .collect()
}

// Each system gives `open_windows` (front to back), `activate_window` (forward, restored when
// minimized), `close_window` (as its close button would) and `can_close_windows`; the last two
// say false when the window is gone or the system refused.
#[cfg(windows)]
pub use win::{activate_window, can_close_windows, close_window, open_windows};

#[cfg(target_os = "macos")]
pub use mac::{activate_window, can_close_windows, close_window, open_windows};

/// On Wayland, the GNOME extension or KWin script lists windows, with their ids marked "g" or
/// "k"; other Wayland desktops can't. On X11, the window manager does.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn open_windows() -> Vec<OpenWindow> {
    if wayland() {
        return gnome::windows().or_else(kde::windows).unwrap_or_default();
    }
    x11::open_windows()
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn activate_window(id: &str) -> bool {
    match id.split_at_checked(1) {
        Some(("g", rest)) => gnome::activate(rest),
        Some(("k", rest)) => kde::activate(rest),
        _ => x11::activate(id),
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn close_window(id: &str) -> bool {
    match id.split_at_checked(1) {
        Some(("g", rest)) => gnome::close(rest),
        Some(("k", rest)) => kde::close(rest),
        _ => x11::close(id),
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn can_close_windows() -> bool {
    true
}

/// What a browser's address bar shows, read the way a screen reader would: through UI Automation
/// on Windows, the Accessibility API on a Mac.
#[cfg(any(windows, target_os = "macos"))]
pub enum Address {
    /// The text in the address bar, usually the page's address without "https://".
    Shows(String),
    /// Someone is typing in it, so it shows what they type rather than the page.
    Typing,
}

#[cfg(windows)]
mod uia;
#[cfg(windows)]
pub use uia::AddressBars;

#[cfg(windows)]
pub use win::{app_path, close_process, foreground, fullscreen_in_front, icon_pixels, icon_png, idle_ms, minimize, open_path, processes, start_apps};

#[cfg(windows)]
mod win {
    use super::OpenWindow;
    use std::sync::Mutex;

    use super::Foreground;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS};
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows_sys::Win32::UI::Input::XboxController::{XInputGetState, XINPUT_STATE};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };

    pub fn foreground() -> Option<Foreground> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }

            let mut title = [0u16; 512];
            let len = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32).max(0);
            let title = String::from_utf16_lossy(&title[..len as usize]);

            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 {
                return None;
            }

            Some(Foreground { exe_path: process_path(pid)?, title, window: hwnd as usize })
        }
    }

    /// The full path of a process's program; None when Windows won't say.
    fn process_path(pid: u32) -> Option<String> {
        unsafe {
            // Limited access is enough to read the image name and works for most elevated processes too.
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return None;
            }
            let mut path = [0u16; 1024];
            let mut size = path.len() as u32;
            let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut size);
            CloseHandle(process);
            (ok != 0).then(|| String::from_utf16_lossy(&path[..size as usize]))
        }
    }

    pub(super) fn window_id(hwnd: windows_sys::Win32::Foundation::HWND) -> String {
        (hwnd as usize).to_string()
    }

    pub(super) fn hwnd(id: &str) -> Option<windows_sys::Win32::Foundation::HWND> {
        id.parse::<usize>().ok().filter(|&h| h != 0).map(|h| h as windows_sys::Win32::Foundation::HWND)
    }

    /// Whether a window's styles make it one to switch to: not one that never takes focus, and
    /// not a popup without a title bar (system surfaces like DWM's notification window), unless
    /// its app marks it for the taskbar.
    pub(super) fn styled_for_switching(style: u32, ex_style: u32) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::{WS_CAPTION, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_POPUP};
        if ex_style & WS_EX_APPWINDOW != 0 {
            return true;
        }
        ex_style & WS_EX_NOACTIVATE == 0 && (style & WS_POPUP == 0 || style & WS_CAPTION != 0)
    }

    /// Whether a top-level window is one a person would switch to: shown or minimized, not owned
    /// by another (dialogs, tool palettes), not a tool window, and not cloaked (the suspended
    /// Store apps and other desktops' windows Windows keeps around unseen).
    unsafe fn switchable_window(hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
        use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindow, GetWindowLongW, IsIconic, IsWindowVisible, GWL_EXSTYLE, GWL_STYLE, GW_OWNER, WS_EX_TOOLWINDOW};
        if IsWindowVisible(hwnd) == 0 && IsIconic(hwnd) == 0 {
            return false;
        }
        if !GetWindow(hwnd, GW_OWNER).is_null() || GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW != 0 {
            return false;
        }
        if !styled_for_switching(GetWindowLongW(hwnd, GWL_STYLE) as u32, GetWindowLongW(hwnd, GWL_EXSTYLE) as u32) {
            return false;
        }
        let mut cloaked = 0u32;
        let read = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED as u32, (&mut cloaked as *mut u32).cast(), std::mem::size_of::<u32>() as u32);
        read != 0 || cloaked == 0
    }

    /// Top-level windows a person would switch to, front to back (`EnumWindows` goes in z-order).
    pub fn open_windows() -> Vec<OpenWindow> {
        use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
        use windows_sys::Win32::UI::WindowsAndMessaging::EnumWindows;
        unsafe extern "system" fn each(hwnd: HWND, found: LPARAM) -> BOOL {
            let found = &mut *(found as *mut Vec<HWND>);
            if switchable_window(hwnd) {
                found.push(hwnd);
            }
            1
        }
        let mut found: Vec<HWND> = Vec::new();
        unsafe { EnumWindows(Some(each), &mut found as *mut Vec<HWND> as LPARAM) };
        found
            .into_iter()
            .filter_map(|hwnd| unsafe {
                let mut title = [0u16; 512];
                let len = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32).max(0);
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, &mut pid);
                (len > 0).then(|| OpenWindow { id: window_id(hwnd), title: String::from_utf16_lossy(&title[..len as usize]), pid, path: process_path(pid) })
            })
            .collect()
    }

    /// Restores a minimized window and brings it forward. Windows lets the app that just had
    /// the keyboard (the palette) hand it on.
    pub fn activate_window(id: &str) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindow, SetForegroundWindow, ShowWindow, SW_RESTORE};
        let Some(hwnd) = hwnd(id) else { return false };
        unsafe {
            if IsWindow(hwnd) == 0 {
                return false;
            }
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            SetForegroundWindow(hwnd) != 0
        }
    }

    /// Asks a window to close, as its close button would, so it can still ask about unsaved work.
    pub fn close_window(id: &str) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::{IsWindow, PostMessageW, WM_CLOSE};
        let Some(hwnd) = hwnd(id) else { return false };
        unsafe { IsWindow(hwnd) != 0 && PostMessageW(hwnd, WM_CLOSE, 0, 0) != 0 }
    }

    pub fn can_close_windows() -> bool {
        true
    }

    /// Minimizes a window; false when Windows refuses, as it does for programs running with more
    /// rights than Kairo (many games with anti-cheat).
    pub fn minimize(window: usize) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, ShowWindow, SW_MINIMIZE};
        let hwnd = window as windows_sys::Win32::Foundation::HWND;
        unsafe {
            ShowWindow(hwnd, SW_MINIMIZE);
            IsIconic(hwnd) != 0
        }
    }

    /// Opens a file the way double-clicking it in Explorer would; for a Start menu shortcut, that
    /// starts the app. False when Windows couldn't.
    pub fn open_path(path: &str) -> bool {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        if path.starts_with(START_APP) {
            return unsafe { open_start_app(path) };
        }
        let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
        let open: Vec<u16> = "open".encode_utf16().chain([0]).collect();
        // Values above 32 mean success.
        unsafe {
            ShellExecuteW(std::ptr::null_mut(), open.as_ptr(), wide.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL)
                as isize
                > 32
        }
    }

    /// How an app in the Start menu's All apps list is named: Windows opens it from there.
    pub const START_APP: &str = r"shell:AppsFolder\";

    /// Every app the Start menu lists under All apps, as (name, `shell:AppsFolder\<id>`): desktop
    /// programs, Store apps, and the games Steam, Epic and others add as links. Web links and
    /// documents left there by installers are skipped. None when Windows won't give the list.
    pub fn start_apps() -> Option<Vec<(String, String)>> {
        use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, IBindCtx, COINIT_APARTMENTTHREADED};
        use windows::Win32::UI::Shell::{
            BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IShellItem, SHGetKnownFolderItem, KF_FLAG_DEFAULT, SIGDN, SIGDN_NORMALDISPLAY,
            SIGDN_PARENTRELATIVEPARSING,
        };
        unsafe fn text(item: &IShellItem, kind: SIGDN) -> Option<String> {
            let name = item.GetDisplayName(kind).ok()?;
            let text = name.to_string().ok();
            CoTaskMemFree(Some(name.0 as *const _));
            text
        }
        const NOT_APPS: [&str; 9] = [".txt", ".pdf", ".chm", ".htm", ".html", ".rtf", ".url", ".ini", ".log"];
        unsafe {
            // The shell wants COM on the calling thread; already there, this changes nothing.
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let folder: IShellItem = SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None).ok()?;
            let items: IEnumShellItems = folder.BindToHandler(None::<&IBindCtx>, &BHID_EnumItems).ok()?;
            let mut apps = Vec::new();
            loop {
                let mut next = [None];
                let mut fetched = 0;
                if items.Next(&mut next, Some(&mut fetched)).is_err() || fetched == 0 {
                    break;
                }
                let Some(item) = next[0].take() else { break };
                let (Some(name), Some(id)) = (text(&item, SIGDN_NORMALDISPLAY), text(&item, SIGDN_PARENTRELATIVEPARSING)) else { continue };
                let lower = id.to_lowercase();
                if ["http:", "https:", "mailto:"].iter().any(|s| lower.starts_with(s)) || NOT_APPS.iter().any(|e| lower.ends_with(e)) {
                    continue;
                }
                apps.push((name, format!("{START_APP}{id}")));
            }
            Some(apps)
        }
    }

    /// Opens an app from the Start menu's list as clicking it there would, through the shell's
    /// own item for it rather than its name, which for a game can be a `steam://` link.
    unsafe fn open_start_app(path: &str) -> bool {
        use windows::core::HSTRING;
        use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_APARTMENTTHREADED};
        use windows::Win32::UI::Shell::Common::ITEMIDLIST;
        use windows::Win32::UI::Shell::{SHParseDisplayName, ShellExecuteExW, SEE_MASK_IDLIST, SHELLEXECUTEINFOW};
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let mut item: *mut ITEMIDLIST = std::ptr::null_mut();
        if SHParseDisplayName(&HSTRING::from(path), None, &mut item, 0, None).is_err() || item.is_null() {
            return false;
        }
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_IDLIST,
            lpIDList: item.cast(),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        let opened = ShellExecuteExW(&mut info).is_ok();
        CoTaskMemFree(Some(item as *const _));
        opened
    }

    /// An app's icon from the Start menu's list, as the shell draws it.
    unsafe fn start_app_icon(path: &str) -> Option<(u32, u32, Vec<u8>)> {
        use windows::core::HSTRING;
        use windows::Win32::Foundation::SIZE;
        use windows::Win32::Graphics::Gdi::{DeleteObject, GetObjectW, BITMAP};
        use windows::Win32::System::Com::IBindCtx;
        use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY};
        let item: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None::<&IBindCtx>).ok()?;
        let bitmap = item.GetImage(SIZE { cx: 32, cy: 32 }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
        let mut info = BITMAP::default();
        let read = GetObjectW(bitmap.into(), std::mem::size_of::<BITMAP>() as i32, Some((&mut info as *mut BITMAP).cast()));
        let pixels = if read == 0 { None } else { read_bits(bitmap.0, info.bmWidth, info.bmHeight) };
        let _ = DeleteObject(bitmap.into());
        let (w, h, mut px) = (info.bmWidth as u32, info.bmHeight as u32, pixels?);
        // The shell hands back premultiplied BGRA; Kairo works in plain RGBA.
        for p in px.chunks_exact_mut(4) {
            let a = p[3] as u32;
            if a > 0 && a < 255 {
                for c in &mut p[..3] {
                    *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                }
            }
            p.swap(0, 2);
        }
        Some((w, h, px))
    }

    /// A program's icon as Explorer shows it: (width, height, top-down RGBA). None when it has
    /// none to give.
    pub fn icon_pixels(path: &str) -> Option<(u32, u32, Vec<u8>)> {
        use windows_sys::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;
        if path.starts_with(START_APP) {
            return unsafe { start_app_icon(path) };
        }
        let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
        unsafe {
            let mut info: SHFILEINFOW = std::mem::zeroed();
            let found = SHGetFileInfoW(wide.as_ptr(), 0, &mut info, std::mem::size_of::<SHFILEINFOW>() as u32, SHGFI_ICON | SHGFI_LARGEICON);
            if found == 0 || info.hIcon.is_null() {
                return None;
            }
            let pixels = icon_rgba(info.hIcon);
            DestroyIcon(info.hIcon);
            pixels
        }
    }

    /// A program's icon as Explorer shows it, as PNG bytes. None when it has none to give.
    pub fn icon_png(path: &str) -> Option<Vec<u8>> {
        let (width, height, rgba) = icon_pixels(path)?;
        {
            let mut png = Vec::new();
            let mut encoder = png::Encoder::new(&mut png, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header().ok()?.write_image_data(&rgba).ok()?;
            Some(png)
        }
    }

    /// An icon's pixels as top-down RGBA.
    unsafe fn icon_rgba(icon: windows_sys::Win32::UI::WindowsAndMessaging::HICON) -> Option<(u32, u32, Vec<u8>)> {
        use windows_sys::Win32::Graphics::Gdi::{DeleteObject, GetObjectW, BITMAP};
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetIconInfo, ICONINFO};
        let mut parts: ICONINFO = std::mem::zeroed();
        if GetIconInfo(icon, &mut parts) == 0 {
            return None;
        }
        let pixels = (|| {
            // Old black-and-white icons have no color part; a letter badge does better than those.
            if parts.hbmColor.is_null() {
                return None;
            }
            let mut bitmap: BITMAP = std::mem::zeroed();
            if GetObjectW(parts.hbmColor, std::mem::size_of::<BITMAP>() as i32, (&mut bitmap as *mut BITMAP).cast()) == 0 {
                return None;
            }
            let (w, h) = (bitmap.bmWidth, bitmap.bmHeight);
            let mut px = read_bits(parts.hbmColor, w, h)?;
            // Older icons leave the alpha channel empty and keep their shape in the mask instead.
            if px.chunks_exact(4).all(|p| p[3] == 0) {
                let mask = read_bits(parts.hbmMask, w, h)?;
                for (p, m) in px.chunks_exact_mut(4).zip(mask.chunks_exact(4)) {
                    p[3] = if m[0] == 0 { 255 } else { 0 };
                }
            }
            for p in px.chunks_exact_mut(4) {
                p.swap(0, 2); // BGRA to RGBA
            }
            Some((w as u32, h as u32, px))
        })();
        DeleteObject(parts.hbmColor);
        DeleteObject(parts.hbmMask);
        pixels
    }

    /// A bitmap's pixels as top-down 32-bit BGRA.
    unsafe fn read_bits(bitmap: windows_sys::Win32::Graphics::Gdi::HBITMAP, w: i32, h: i32) -> Option<Vec<u8>> {
        use windows_sys::Win32::Graphics::Gdi::{GetDC, GetDIBits, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS};
        if w <= 0 || h <= 0 {
            return None;
        }
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = w;
        info.bmiHeader.biHeight = -h; // negative: rows top to bottom
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(std::ptr::null_mut());
        let lines = GetDIBits(dc, bitmap, 0, h as u32, buf.as_mut_ptr().cast(), &mut info, DIB_RGB_COLORS);
        ReleaseDC(std::ptr::null_mut(), dc);
        (lines == h).then_some(buf)
    }

    /// Every running process: (process id, executable file name such as `steam.exe`).
    pub fn processes() -> Vec<(u32, String)> {
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
        };
        let mut out = Vec::new();
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return out;
            }
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut more = Process32FirstW(snapshot, &mut entry) != 0;
            while more {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                out.push((entry.th32ProcessID, String::from_utf16_lossy(&entry.szExeFile[..len])));
                more = Process32NextW(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
        }
        out
    }

    /// Ends a process outright; false when it's gone already or Windows won't allow it.
    pub fn close_process(pid: u32) -> bool {
        use windows_sys::Win32::System::Threading::{TerminateProcess, PROCESS_TERMINATE};
        unsafe {
            let process = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if process.is_null() {
                return false;
            }
            let ok = TerminateProcess(process, 1) != 0;
            CloseHandle(process);
            ok
        }
    }

    /// Whether a game or other app fills the screen in front: a Direct3D game Windows knows is
    /// full screen, or a window without a title bar covering its whole monitor, as borderless
    /// games and full-screen video and browsers do, maximized or not. A maximized window with a
    /// title bar is just a big window, even where the taskbar hides itself and it covers the whole
    /// screen; the desktop covers it too, so it doesn't count either.
    pub fn fullscreen_in_front() -> bool {
        use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL};
        use windows_sys::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetClassNameW, GetShellWindow, GetWindowLongW, GetWindowRect, IsZoomed, GWL_STYLE, WS_CAPTION,
        };
        unsafe {
            let mut state = 0;
            if SHQueryUserNotificationState(&mut state) == 0 && state == QUNS_RUNNING_D3D_FULL_SCREEN {
                return true;
            }
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() || hwnd == GetShellWindow() {
                return false;
            }
            let titled = GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_CAPTION == WS_CAPTION;
            if IsZoomed(hwnd) != 0 && titled {
                return false;
            }
            let mut class = [0u16; 32];
            let len = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
            if matches!(String::from_utf16_lossy(&class[..len]).as_str(), "Progman" | "WorkerW") {
                return false;
            }
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL);
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            let mut rect = std::mem::zeroed();
            if monitor.is_null() || GetMonitorInfoW(monitor, &mut info) == 0 || GetWindowRect(hwnd, &mut rect) == 0 {
                return false;
            }
            let screen = info.rcMonitor;
            rect.left <= screen.left && rect.top <= screen.top && rect.right >= screen.right && rect.bottom >= screen.bottom
        }
    }

    /// Time since the last keyboard, mouse or game controller input.
    pub fn idle_ms() -> i64 {
        unsafe {
            let now = GetTickCount();
            let mut info = LASTINPUTINFO {
                cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };
            if GetLastInputInfo(&mut info) == 0 {
                return 0;
            }
            // Both are 32-bit tick counts that wrap every ~49 days; wrapping_sub handles that.
            let keyboard_mouse = now.wrapping_sub(info.dwTime) as i64;
            match pad_idle_ms(now) {
                Some(pad) => keyboard_mouse.min(pad),
                None => keyboard_mouse,
            }
        }
    }

    /// Where an installed program lives, from the "App Paths" Windows keeps for `Start`-style
    /// launching, e.g. `brave.exe`. Per-user installs come first.
    pub fn app_path(exe: &str) -> Option<String> {
        use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
        let key: Vec<u16> =
            format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{exe}").encode_utf16().chain([0]).collect();
        for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            let mut buf = [0u16; 1024];
            let mut size = (buf.len() * 2) as u32;
            let rc = unsafe {
                RegGetValueW(root, key.as_ptr(), std::ptr::null(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut size)
            };
            if rc != ERROR_SUCCESS {
                continue;
            }
            // `size` is in bytes and includes the terminating null.
            let len = (size as usize / 2).saturating_sub(1);
            let path = String::from_utf16_lossy(&buf[..len]).trim_matches('"').to_string();
            if std::path::Path::new(&path).is_file() {
                return Some(path);
            }
        }
        None
    }

    const PAD_SLOTS: usize = 4;
    /// Asking about an empty slot is slow, so unplugged ones are only rechecked this often.
    const EMPTY_SLOT_RECHECK_MS: u32 = 5_000;

    struct Pads {
        /// Each connected controller's packet number, which changes with every button or stick move.
        packets: [Option<u32>; PAD_SLOTS],
        next_check: [u32; PAD_SLOTS],
        last_input: Option<u32>,
    }

    static PADS: Mutex<Pads> =
        Mutex::new(Pads { packets: [None; PAD_SLOTS], next_check: [0; PAD_SLOTS], last_input: None });

    /// Windows' idle timer ignores game controllers, so they're polled here instead. None until a
    /// controller has been used since Kairo started.
    fn pad_idle_ms(now: u32) -> Option<i64> {
        let mut pads = PADS.lock().unwrap_or_else(|e| e.into_inner());
        for slot in 0..PAD_SLOTS {
            // Wrapping comparison: is `now` at or past the slot's next check?
            if pads.packets[slot].is_none() && (now.wrapping_sub(pads.next_check[slot]) as i32) < 0 {
                continue;
            }
            let mut state: XINPUT_STATE = unsafe { std::mem::zeroed() };
            let packet = (unsafe { XInputGetState(slot as u32, &mut state) } == ERROR_SUCCESS).then_some(state.dwPacketNumber);
            // A controller's first sighting isn't input; a changed packet number is.
            if packet.is_some() && pads.packets[slot].is_some() && packet != pads.packets[slot] {
                pads.last_input = Some(now);
            }
            if packet.is_none() {
                pads.next_check[slot] = now.wrapping_add(EMPTY_SLOT_RECHECK_MS);
            }
            pads.packets[slot] = packet;
        }
        pads.last_input.map(|t| now.wrapping_sub(t) as i64)
    }
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn window_ids_round_trip() {
        assert_eq!(super::win::window_id(super::win::hwnd("123456").unwrap()), "123456");
        assert!(super::win::hwnd("junk").is_none());
        assert!(super::win::hwnd("0").is_none());
    }

    #[test]
    fn caption_less_popups_and_unfocusable_windows_are_left_out() {
        use super::win::styled_for_switching as ok;
        // Brave, and the DWM notification window seen on a real desktop.
        assert!(ok(0x17CF_0000, 0x0020_0100));
        assert!(!ok(0xA400_0000, 0));
        // A frameless popup the app marks for the taskbar stays; one that never takes focus goes.
        assert!(ok(0x8000_0000, 0x0004_0000));
        assert!(!ok(0x14CF_0000, 0x0800_0000));
    }

    #[test]
    fn idle_time_with_controller_polling_stays_cheap() {
        // The collector asks every second; empty controller slots must not make that slow.
        let start = std::time::Instant::now();
        for _ in 0..20 {
            assert!(super::idle_ms() >= 0);
        }
        assert!(start.elapsed() < std::time::Duration::from_millis(500), "took {:?}", start.elapsed());
    }

    #[test]
    fn program_icons_come_out_as_png() {
        // Explorer is on every Windows install and has a color icon.
        let explorer = std::env::var("WINDIR").map(|w| format!(r"{w}\explorer.exe")).unwrap();
        let png = super::icon_png(&explorer).expect("explorer.exe has an icon");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert!(super::icon_png(r"C:\no\such\program.exe").is_none());
    }

    #[test]
    fn the_process_list_includes_this_test_and_unknown_processes_cant_be_closed() {
        let me = std::process::id();
        assert!(super::processes().iter().any(|(pid, file)| *pid == me && file.ends_with(".exe")));
        // Process id 0 is the idle process; nothing gets to end it.
        assert!(!super::close_process(0));
    }
}

#[cfg(target_os = "macos")]
pub use mac::{
    app_path, close_process, foreground, frontmost_path, fullscreen_in_front, icon_png, idle_ms, minimize, open_path, processes, request_window_titles,
    AddressBars,
};

/// On a Mac, `Foreground::window` holds the app's process id: apps are hidden as a whole, and
/// `processes` gives full paths, which `classify::exe_stem` reads by their app bundle.
#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{c_void, CStr};
    use std::path::{Path, PathBuf};

    use objc2::rc::autoreleasepool;
    use objc2::AnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSRunningApplication, NSWorkspace};
    use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

    use super::Foreground;
    use crate::classify;

    mod ax;
    pub use ax::AddressBars;

    /// Each listed window's title by its id, from the last listing: the Accessibility API finds a
    /// window by its title, since it doesn't know Core Graphics' numbers.
    static TITLES: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

    /// "<pid>:<window number>", as `open_windows` names a window.
    pub(super) fn window_of(id: &str) -> Option<(i32, u32)> {
        let (pid, number) = id.split_once(':')?;
        Some((pid.parse().ok().filter(|&p| p > 0)?, number.parse().ok()?))
    }

    /// Other apps' ordinary windows (layer 0) with a title, front to back, minimized ones too.
    /// Titles need the Screen Recording access Kairo already asks for.
    pub fn open_windows() -> Vec<super::OpenWindow> {
        let mut found = Vec::new();
        unsafe {
            let list = CGWindowListCopyWindowInfo(ALL_WINDOWS | EXCLUDE_DESKTOP_ELEMENTS, 0);
            if list.is_null() {
                return found;
            }
            for i in 0..CFArrayGetCount(list) {
                let window = CFArrayGetValueAtIndex(list, i);
                let (Some(pid), Some(layer), Some(number)) = (number(window, kCGWindowOwnerPID), number(window, kCGWindowLayer), number(window, kCGWindowNumber)) else { continue };
                let Some(title) = string(CFDictionaryGetValue(window, kCGWindowName)).filter(|t| !t.trim().is_empty()) else { continue };
                if layer != 0 {
                    continue;
                }
                found.push(super::OpenWindow { id: format!("{pid}:{number}"), title, pid: pid as u32, path: process_path(pid as i32) });
            }
            CFRelease(list);
        }
        *TITLES.lock().unwrap_or_else(|e| e.into_inner()) = found.iter().map(|w| (w.id.clone(), w.title.clone())).collect();
        found
    }

    fn title_of(id: &str) -> Option<String> {
        TITLES.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(known, _)| known == id).map(|(_, t)| t.clone())
    }

    /// Brings the window's app forward, then, with Accessibility access, the window itself.
    pub fn activate_window(id: &str) -> bool {
        use objc2_app_kit::NSApplicationActivationOptions;
        let Some((pid, _)) = window_of(id) else { return false };
        let activated = autoreleasepool(|_| {
            NSRunningApplication::runningApplicationWithProcessIdentifier(pid).is_some_and(|app| app.activateWithOptions(NSApplicationActivationOptions(0)))
        });
        if ax::trusted() {
            if let Some(title) = title_of(id) {
                ax::raise(pid, &title);
            }
        }
        activated
    }

    /// Presses the window's close button; only with Accessibility access.
    pub fn close_window(id: &str) -> bool {
        let (Some((pid, _)), Some(title)) = (window_of(id), title_of(id)) else { return false };
        ax::trusted() && ax::close(pid, &title)
    }

    /// Closing one window needs Accessibility access (Privacy & Security → Accessibility).
    pub fn can_close_windows() -> bool {
        ax::trusted()
    }

    type CFTypeRef = *const c_void;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFArrayGetCount(array: CFTypeRef) -> isize;
        fn CFArrayGetValueAtIndex(array: CFTypeRef, index: isize) -> CFTypeRef;
        fn CFDictionaryGetValue(dict: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
        fn CFNumberGetValue(number: CFTypeRef, kind: isize, out: *mut c_void) -> u8;
        fn CFStringGetLength(string: CFTypeRef) -> isize;
        fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
        fn CFStringGetCString(string: CFTypeRef, buf: *mut std::ffi::c_char, size: isize, encoding: u32) -> u8;
        fn CFRelease(cf: CFTypeRef);
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        static kCGWindowOwnerPID: CFTypeRef;
        static kCGWindowNumber: CFTypeRef;
        static kCGWindowLayer: CFTypeRef;
        static kCGWindowName: CFTypeRef;
        static kCGWindowBounds: CFTypeRef;
        fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> CFTypeRef;
        fn CGRectMakeWithDictionaryRepresentation(dict: CFTypeRef, rect: *mut CGRect) -> bool;
        fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
        fn CGDisplayBounds(display: u32) -> CGRect;
        fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CGRect {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    }

    const ON_SCREEN_ONLY: u32 = 1 << 0;
    /// Every window, minimized and other desktops' too, not only those on screen.
    const ALL_WINDOWS: u32 = 0;
    const EXCLUDE_DESKTOP_ELEMENTS: u32 = 1 << 4;
    const COMBINED_SESSION_STATE: i32 = 0;
    const ANY_INPUT_EVENT: u32 = !0;
    const NUMBER_SINT64: isize = 4;
    const UTF8: u32 = 0x0800_0100;
    /// Icon size in points; Retina screens get twice the pixels.
    const ICON_POINTS: f64 = 32.0;

    pub fn foreground() -> Option<Foreground> {
        autoreleasepool(|_| {
            let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
            let pid = app.processIdentifier();
            Some(Foreground { exe_path: process_path(pid)?, title: window_title(pid).unwrap_or_default(), window: pid as usize })
        })
    }

    /// The program in front, without the window list `foreground` reads for its title.
    pub fn frontmost_path() -> Option<String> {
        autoreleasepool(|_| process_path(NSWorkspace::sharedWorkspace().frontmostApplication()?.processIdentifier()))
    }

    /// Window titles of other apps need Screen Recording access. Asks once; macOS remembers the
    /// answer, and without it Kairo still tracks apps, just not titles or sites guessed from them.
    pub fn request_window_titles() {
        unsafe {
            if !CGPreflightScreenCaptureAccess() {
                CGRequestScreenCaptureAccess();
            }
        }
    }

    /// The title of an app's frontmost window. The window list runs front to back, and layer 0
    /// holds ordinary windows; menus, the Dock and overlays sit above it.
    fn window_title(pid: i32) -> Option<String> {
        unsafe {
            let list = CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP_ELEMENTS, 0);
            if list.is_null() {
                return None;
            }
            let mut title = None;
            for i in 0..CFArrayGetCount(list) {
                let window = CFArrayGetValueAtIndex(list, i);
                if number(window, kCGWindowOwnerPID) == Some(pid as i64) && number(window, kCGWindowLayer) == Some(0) {
                    title = string(CFDictionaryGetValue(window, kCGWindowName));
                    break;
                }
            }
            CFRelease(list);
            title
        }
    }

    /// Whether the app in front fills a display: its frontmost window covers one whole screen,
    /// menu bar and Dock included, as a full-screen app or game does. Window bounds, unlike
    /// titles, don't need Screen Recording access.
    pub fn fullscreen_in_front() -> bool {
        let Some(pid) = autoreleasepool(|_| Some(NSWorkspace::sharedWorkspace().frontmostApplication()?.processIdentifier())) else {
            return false;
        };
        unsafe {
            let mut displays = [0u32; 16];
            let mut count = 0u32;
            if CGGetActiveDisplayList(displays.len() as u32, displays.as_mut_ptr(), &mut count) != 0 {
                return false;
            }
            let screens: Vec<CGRect> = displays[..count as usize].iter().map(|&d| CGDisplayBounds(d)).collect();
            let list = CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP_ELEMENTS, 0);
            if list.is_null() {
                return false;
            }
            let mut covers = false;
            for i in 0..CFArrayGetCount(list) {
                let window = CFArrayGetValueAtIndex(list, i);
                if number(window, kCGWindowOwnerPID) == Some(pid as i64) && number(window, kCGWindowLayer) == Some(0) {
                    let mut rect = CGRect::default();
                    covers = CGRectMakeWithDictionaryRepresentation(CFDictionaryGetValue(window, kCGWindowBounds), &mut rect)
                        && screens.iter().any(|s| {
                            rect.x <= s.x && rect.y <= s.y && rect.x + rect.width >= s.x + s.width && rect.y + rect.height >= s.y + s.height
                        });
                    break;
                }
            }
            CFRelease(list);
            covers
        }
    }

    unsafe fn number(dict: CFTypeRef, key: CFTypeRef) -> Option<i64> {
        let value = CFDictionaryGetValue(dict, key);
        let mut out = 0i64;
        (!value.is_null() && CFNumberGetValue(value, NUMBER_SINT64, (&mut out as *mut i64).cast()) != 0).then_some(out)
    }

    unsafe fn string(value: CFTypeRef) -> Option<String> {
        if value.is_null() {
            return None;
        }
        let size = CFStringGetMaximumSizeForEncoding(CFStringGetLength(value), UTF8) + 1;
        let mut buf = vec![0u8; size as usize];
        if CFStringGetCString(value, buf.as_mut_ptr().cast(), size, UTF8) == 0 {
            return None;
        }
        CStr::from_bytes_until_nul(&buf).ok().map(|s| s.to_string_lossy().into_owned())
    }

    fn process_path(pid: i32) -> Option<String> {
        let mut buf = [0u8; 4096];
        let len = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
        (len > 0).then(|| String::from_utf8_lossy(&buf[..len as usize]).into_owned())
    }

    /// Hides the app, which is as close as a Mac comes to minimizing it; false when it's gone.
    pub fn minimize(window: usize) -> bool {
        autoreleasepool(|_| {
            NSRunningApplication::runningApplicationWithProcessIdentifier(window as i32).is_some_and(|app| app.hide())
        })
    }

    /// Opens a file the way double-clicking it in Finder would; for an app, that starts it.
    pub fn open_path(path: &str) -> bool {
        std::process::Command::new("/usr/bin/open").arg(path).status().is_ok_and(|s| s.success())
    }

    /// An app's icon as Finder shows it, as PNG bytes. None when the program is gone.
    pub fn icon_png(path: &str) -> Option<Vec<u8>> {
        // Inside a bundle, the executable only has the generic icon; the bundle has the real one.
        let target = classify::bundle_path(path).unwrap_or(path);
        if !Path::new(target).exists() {
            return None;
        }
        autoreleasepool(|_| unsafe {
            let icon = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(target));
            let mut rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(ICON_POINTS, ICON_POINTS));
            let image = icon.CGImageForProposedRect_context_hints(&mut rect, None, None)?;
            let bitmap = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &image);
            let png = bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())?;
            Some(png.to_vec())
        })
    }

    /// Every running process: (process id, full path of its program).
    pub fn processes() -> Vec<(u32, String)> {
        let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
        if count <= 0 {
            return Vec::new();
        }
        // Room for processes that start between the two calls.
        let mut pids = vec![0 as libc::pid_t; count as usize + 64];
        let size = (pids.len() * std::mem::size_of::<libc::pid_t>()) as i32;
        let count = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), size) };
        pids.truncate(count.max(0) as usize);
        pids.into_iter().filter(|&pid| pid > 0).filter_map(|pid| Some((pid as u32, process_path(pid)?))).collect()
    }

    /// Ends a process outright; false when it's gone already or macOS won't allow it.
    pub fn close_process(pid: u32) -> bool {
        // 0 would mean Kairo's own process group to kill().
        pid != 0 && i32::try_from(pid).is_ok_and(|pid| unsafe { libc::kill(pid, libc::SIGKILL) } == 0)
    }

    /// Time since the last keyboard, mouse, trackpad or game controller input.
    pub fn idle_ms() -> i64 {
        let seconds = unsafe { CGEventSourceSecondsSinceLastEventType(COMBINED_SESSION_STATE, ANY_INPUT_EVENT) };
        let keyboard_mouse = (seconds * 1000.0).max(0.0) as i64;
        match pads::idle_ms() {
            Some(pad) => keyboard_mouse.min(pad),
            None => keyboard_mouse,
        }
    }

    /// macOS's idle time ignores game controllers, so a HID manager on a thread of its own
    /// watches gamepads and joysticks, and only those: watching keyboards would need Input
    /// Monitoring access.
    mod pads {
        use std::ffi::c_void;
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::{Once, OnceLock};
        use std::time::Instant;

        use super::{CFRelease, CFTypeRef};

        #[link(name = "CoreFoundation", kind = "framework")]
        extern "C" {
            static kCFTypeDictionaryKeyCallBacks: c_void;
            static kCFTypeDictionaryValueCallBacks: c_void;
            static kCFTypeArrayCallBacks: c_void;
            static kCFRunLoopDefaultMode: CFTypeRef;
            fn CFNumberCreate(allocator: CFTypeRef, kind: isize, value: *const c_void) -> CFTypeRef;
            fn CFStringCreateWithCString(allocator: CFTypeRef, text: *const std::ffi::c_char, encoding: u32) -> CFTypeRef;
            fn CFDictionaryCreate(
                allocator: CFTypeRef,
                keys: *const CFTypeRef,
                values: *const CFTypeRef,
                count: isize,
                key_callbacks: *const c_void,
                value_callbacks: *const c_void,
            ) -> CFTypeRef;
            fn CFArrayCreate(allocator: CFTypeRef, values: *const CFTypeRef, count: isize, callbacks: *const c_void) -> CFTypeRef;
            fn CFRunLoopGetCurrent() -> CFTypeRef;
            fn CFRunLoopRun();
        }

        type InputCallback = extern "C" fn(context: *mut c_void, result: i32, sender: *mut c_void, value: CFTypeRef);

        #[link(name = "IOKit", kind = "framework")]
        extern "C" {
            fn IOHIDManagerCreate(allocator: CFTypeRef, options: u32) -> CFTypeRef;
            fn IOHIDManagerSetDeviceMatchingMultiple(manager: CFTypeRef, matching: CFTypeRef);
            fn IOHIDManagerRegisterInputValueCallback(manager: CFTypeRef, callback: InputCallback, context: *mut c_void);
            fn IOHIDManagerScheduleWithRunLoop(manager: CFTypeRef, run_loop: CFTypeRef, mode: CFTypeRef);
            fn IOHIDManagerOpen(manager: CFTypeRef, options: u32) -> i32;
            fn IOHIDValueGetElement(value: CFTypeRef) -> CFTypeRef;
            fn IOHIDValueGetIntegerValue(value: CFTypeRef) -> isize;
            fn IOHIDElementGetUsagePage(element: CFTypeRef) -> u32;
            fn IOHIDElementGetLogicalMin(element: CFTypeRef) -> isize;
            fn IOHIDElementGetLogicalMax(element: CFTypeRef) -> isize;
        }

        const GENERIC_DESKTOP: i32 = 0x01;
        const JOYSTICK: i32 = 0x04;
        const GAMEPAD: i32 = 0x05;
        const MULTI_AXIS: i32 = 0x08;
        const BUTTON_PAGE: u32 = 0x09;
        const NUMBER_SINT32: isize = 3;
        const UTF8: u32 = 0x0800_0100;

        static START: OnceLock<Instant> = OnceLock::new();
        /// Milliseconds after START of the last input, plus one; 0 until a controller is used.
        static LAST_INPUT: AtomicU64 = AtomicU64::new(0);
        static WATCH: Once = Once::new();

        /// None until a controller has been used since Kairo started.
        pub fn idle_ms() -> Option<i64> {
            let start = *START.get_or_init(Instant::now);
            WATCH.call_once(|| {
                if let Err(e) = std::thread::Builder::new().name("kairo-pads".into()).spawn(watch) {
                    eprintln!("pads: cannot watch controllers: {e}");
                }
            });
            let last = LAST_INPUT.load(Ordering::Relaxed).checked_sub(1)?;
            Some((start.elapsed().as_millis() as u64).saturating_sub(last) as i64)
        }

        fn watch() {
            unsafe {
                let manager = IOHIDManagerCreate(std::ptr::null(), 0);
                if manager.is_null() {
                    return;
                }
                let page_key = CFStringCreateWithCString(std::ptr::null(), c"DeviceUsagePage".as_ptr(), UTF8);
                let usage_key = CFStringCreateWithCString(std::ptr::null(), c"DeviceUsage".as_ptr(), UTF8);
                let number = |n: i32| CFNumberCreate(std::ptr::null(), NUMBER_SINT32, (&n as *const i32).cast());
                let page = number(GENERIC_DESKTOP);
                let matches: Vec<CFTypeRef> = [JOYSTICK, GAMEPAD, MULTI_AXIS]
                    .into_iter()
                    .map(|usage| {
                        let usage = number(usage);
                        let keys = [page_key, usage_key];
                        let values = [page, usage];
                        let dict = CFDictionaryCreate(
                            std::ptr::null(),
                            keys.as_ptr(),
                            values.as_ptr(),
                            2,
                            &kCFTypeDictionaryKeyCallBacks,
                            &kCFTypeDictionaryValueCallBacks,
                        );
                        CFRelease(usage);
                        dict
                    })
                    .collect();
                let matching = CFArrayCreate(std::ptr::null(), matches.as_ptr(), matches.len() as isize, &kCFTypeArrayCallBacks);
                for cf in matches.into_iter().chain([page, page_key, usage_key]) {
                    CFRelease(cf);
                }
                IOHIDManagerSetDeviceMatchingMultiple(manager, matching);
                CFRelease(matching);
                IOHIDManagerRegisterInputValueCallback(manager, on_input, std::ptr::null_mut());
                IOHIDManagerScheduleWithRunLoop(manager, CFRunLoopGetCurrent(), kCFRunLoopDefaultMode);
                IOHIDManagerOpen(manager, 0);
                CFRunLoopRun();
            }
        }

        thread_local! {
            /// Where each stick and trigger last counted as input, by element.
            static AXES: std::cell::RefCell<std::collections::HashMap<usize, f64>> = Default::default();
        }

        /// A button, or a stick or trigger moved far enough to be more than drift.
        extern "C" fn on_input(_context: *mut c_void, _result: i32, _sender: *mut c_void, value: CFTypeRef) {
            let used = unsafe {
                let element = IOHIDValueGetElement(value);
                if element.is_null() {
                    return;
                }
                if IOHIDElementGetUsagePage(element) == BUTTON_PAGE {
                    true
                } else {
                    let (min, max) = (IOHIDElementGetLogicalMin(element), IOHIDElementGetLogicalMax(element));
                    let at = (IOHIDValueGetIntegerValue(value) - min) as f64 / (max - min).max(1) as f64;
                    AXES.with_borrow_mut(|axes| super::super::moved(axes, element as usize, at))
                }
            };
            if used {
                let start = *START.get_or_init(Instant::now);
                LAST_INPUT.store(start.elapsed().as_millis() as u64 + 1, Ordering::Relaxed);
            }
        }
    }

    /// Where an installed app's program lives, for a Windows exe name such as `brave.exe`; looks
    /// in /Applications, then in the user's own Applications folder.
    pub fn app_path(exe: &str) -> Option<String> {
        let stem = exe.strip_suffix(".exe").unwrap_or(exe);
        let &(name, _) = classify::MAC_APPS.iter().find(|(_, s)| *s == stem)?;
        let home = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Applications"));
        [Some(PathBuf::from("/Applications")), home]
            .into_iter()
            .flatten()
            .map(|dir| dir.join(format!("{name}.app/Contents/MacOS/{name}")))
            .find(|path| path.is_file())
            .map(|path| path.display().to_string())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn mac_window_ids_are_parsed() {
        assert_eq!(super::mac::window_of("501:77"), Some((501, 77)));
        assert_eq!(super::mac::window_of("0:77"), None);
        assert_eq!(super::mac::window_of("junk"), None);
    }

    #[test]
    fn idle_time_is_never_negative() {
        assert!(super::idle_ms() >= 0);
    }

    #[test]
    fn app_icons_come_out_as_png() {
        // Finder is on every Mac and has a color icon.
        let png = super::icon_png("/System/Library/CoreServices/Finder.app/Contents/MacOS/Finder").expect("Finder has an icon");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert!(super::icon_png("/Applications/No Such App.app/Contents/MacOS/No Such App").is_none());
    }

    #[test]
    fn the_process_list_includes_this_test_and_unknown_processes_cant_be_closed() {
        let me = std::process::id();
        assert!(super::processes().iter().any(|(pid, path)| *pid == me && path.starts_with('/')));
        assert!(!super::close_process(0));
        // launchd belongs to root; nothing else gets to end it.
        assert!(!super::close_process(1));
    }
}

// Linux and others: other apps' windows come from X11, or on Wayland, which shows X11 only the
// apps running through XWayland, from the desktop itself: Kairo's GNOME Shell extension on GNOME
// and its KWin script on KDE Plasma. Programs come from /proc.
#[cfg(not(any(windows, target_os = "macos")))]
mod gnome;
#[cfg(not(any(windows, target_os = "macos")))]
mod kde;

/// The window in front as a Wayland desktop tells it.
#[cfg(not(any(windows, target_os = "macos")))]
struct WaylandWindow {
    pid: u32,
    title: String,
    /// The window, marked with the desktop's bit (`gnome::WINDOW`, `kde::WINDOW`).
    window: usize,
    fullscreen: bool,
}

/// Whether this is a Wayland session.
#[cfg(not(any(windows, target_os = "macos")))]
fn wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// Whether the session is `name`'s desktop, e.g. "GNOME" or "KDE".
#[cfg(not(any(windows, target_os = "macos")))]
fn desktop_is(name: &str) -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.split(':').any(|d| d.eq_ignore_ascii_case(name)))
}

/// What the desktop says is in front on Wayland: None where it can't say, which leaves X11;
/// Some(None) when nothing is.
#[cfg(not(any(windows, target_os = "macos")))]
fn wayland_front() -> Option<Option<WaylandWindow>> {
    if !wayland() {
        return None;
    }
    gnome::focused().or_else(kde::focused)
}

/// Sets up seeing windows on a Wayland desktop: installs the GNOME extension or loads the KWin
/// script. True when the GNOME extension is newly installed, which works from the next login.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn watch_wayland() -> bool {
    kde::start();
    gnome::install()
}

/// On Wayland, which keeps keys from apps, makes `shortcut` open the palette through the
/// desktop's own shortcuts: GNOME runs `kairo --palette`, and KDE Plasma tells Kairo, which runs
/// what `on_palette_shortcut` was given. False where Kairo listens for the keys itself.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn desktop_palette_shortcut(shortcut: &str) -> bool {
    wayland() && (gnome::palette_shortcut(shortcut) || kde::palette_shortcut(shortcut))
}

#[cfg(not(any(windows, target_os = "macos")))]
pub use kde::on_palette_shortcut;

/// Windows and macOS let Kairo listen for the palette's keys itself.
#[cfg(any(windows, target_os = "macos"))]
pub fn desktop_palette_shortcut(_shortcut: &str) -> bool {
    false
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn foreground() -> Option<Foreground> {
    if let Some(front) = wayland_front() {
        let f = front?;
        return Some(Foreground { exe_path: program(f.pid)?, title: f.title, window: f.window });
    }
    x11::foreground()
}

/// Whether the window in front is full screen, as the window manager marks it for a game or
/// video.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn fullscreen_in_front() -> bool {
    if let Some(front) = wayland_front() {
        return front.is_some_and(|f| f.fullscreen);
    }
    x11::fullscreen_in_front()
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn minimize(window: usize) -> bool {
    if window & gnome::WINDOW != 0 {
        gnome::minimize(window)
    } else if window & kde::WINDOW != 0 {
        kde::minimize(window)
    } else {
        x11::minimize(window)
    }
}

/// Time since the last keyboard, mouse or game controller input. On Wayland, X11 hears only
/// XWayland apps' input, so GNOME or Plasma is asked instead; other Wayland desktops count as
/// never idle.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn idle_ms() -> i64 {
    let keyboard_mouse = if wayland() { gnome::idle_ms().or_else(kde::idle_ms).unwrap_or(0) } else { x11::idle_ms() };
    match pads::idle_ms() {
        Some(pad) => keyboard_mouse.min(pad),
        None => keyboard_mouse,
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod x11 {
    use x11rb::connection::Connection;
    use x11rb::protocol::screensaver::ConnectionExt as _;
    use x11rb::protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, GetPropertyReply, Window};
    use x11rb::rust_connection::RustConnection;

    use super::Foreground;

    /// A connection to the X server; each call opens its own, which takes a millisecond or so.
    struct Display {
        conn: RustConnection,
        root: Window,
    }

    impl Display {
        fn open() -> Option<Self> {
            let (conn, screen) = x11rb::connect(None).ok()?;
            let root = conn.setup().roots.get(screen)?.root;
            Some(Self { conn, root })
        }

        fn atom(&self, name: &str) -> Option<u32> {
            Some(self.conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok()?.atom)
        }

        /// A window property, `len` 32-bit units at most; `kind` ANY takes any type.
        fn property(&self, window: Window, name: &str, kind: impl Into<u32>, len: u32) -> Option<GetPropertyReply> {
            self.conn.get_property(false, window, self.atom(name)?, kind, 0, len).ok()?.reply().ok()
        }

        /// The window the window manager says has the keyboard.
        fn active_window(&self) -> Option<Window> {
            self.property(self.root, "_NET_ACTIVE_WINDOW", AtomEnum::WINDOW, 1)?.value32()?.next().filter(|&w| w != 0)
        }
    }

    impl Display {
        /// A window's title: the modern UTF-8 one, or the old one older programs set instead.
        fn title(&self, window: Window) -> String {
            ["_NET_WM_NAME", "WM_NAME"]
                .iter()
                .find_map(|name| {
                    let p = self.property(window, name, AtomEnum::ANY, 1024)?;
                    (!p.value.is_empty()).then(|| String::from_utf8_lossy(&p.value).into_owned())
                })
                .unwrap_or_default()
        }

        fn pid(&self, window: Window) -> Option<u32> {
            self.property(window, "_NET_WM_PID", AtomEnum::CARDINAL, 1)?.value32()?.next()
        }

        /// Asks the window manager about a window with a client message to the root, as pagers do.
        fn ask(&self, window: Window, message: &str, data: [u32; 5]) -> bool {
            let Some(kind) = self.atom(message) else { return false };
            let event = ClientMessageEvent::new(32, window, kind, data);
            self.conn.send_event(false, self.root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, event).is_ok() && self.conn.flush().is_ok()
        }
    }

    pub fn foreground() -> Option<Foreground> {
        let x = Display::open()?;
        let window = x.active_window()?;
        let pid = x.pid(window)?;
        Some(Foreground { exe_path: super::program(pid)?, title: x.title(window), window: window as usize })
    }

    pub(super) fn window_of(id: &str) -> Option<Window> {
        id.parse().ok().filter(|&w: &Window| w != 0)
    }

    /// The windows the window manager manages, front first (its stacking list runs bottom up).
    pub fn open_windows() -> Vec<super::OpenWindow> {
        let Some(x) = Display::open() else { return Vec::new() };
        let Some(stack) = x.property(x.root, "_NET_CLIENT_LIST_STACKING", AtomEnum::WINDOW, 4096) else { return Vec::new() };
        let windows: Vec<Window> = stack.value32().map(|v| v.collect()).unwrap_or_default();
        windows
            .into_iter()
            .rev()
            .map(|w| {
                let pid = x.pid(w).unwrap_or(0);
                super::OpenWindow { id: w.to_string(), title: x.title(w), pid, path: super::program(pid) }
            })
            .collect()
    }

    /// Asks the window manager to bring a window forward, as a pager would (source 2).
    pub fn activate(id: &str) -> bool {
        let (Some(x), Some(w)) = (Display::open(), window_of(id)) else { return false };
        x.ask(w, "_NET_ACTIVE_WINDOW", [2, 0, 0, 0, 0])
    }

    /// Asks the window manager to close a window, as its close button would.
    pub fn close(id: &str) -> bool {
        let (Some(x), Some(w)) = (Display::open(), window_of(id)) else { return false };
        x.ask(w, "_NET_CLOSE_WINDOW", [0, 2, 0, 0, 0])
    }

    /// Whether the active window is full screen, as the window manager marks it for a game or
    /// video.
    pub fn fullscreen_in_front() -> bool {
        let fullscreen = || -> Option<bool> {
            let x = Display::open()?;
            let state = x.property(x.active_window()?, "_NET_WM_STATE", AtomEnum::ATOM, 64)?;
            let full = x.atom("_NET_WM_STATE_FULLSCREEN")?;
            let found = state.value32()?.any(|a| a == full);
            Some(found)
        };
        fullscreen().unwrap_or(false)
    }

    /// Asks the window manager to minimize a window, as its own minimize button would.
    pub fn minimize(window: usize) -> bool {
        const ICONIC_STATE: u32 = 3;
        let Some(x) = Display::open() else { return false };
        let Some(change_state) = x.atom("WM_CHANGE_STATE") else { return false };
        let event = ClientMessageEvent::new(32, window as Window, change_state, [ICONIC_STATE, 0, 0, 0, 0]);
        x.conn.send_event(false, x.root, EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY, event).is_ok()
            && x.conn.flush().is_ok()
    }

    /// Time since the last keyboard or mouse input, from the X server's screen saver.
    pub fn idle_ms() -> i64 {
        Display::open().and_then(|x| x.conn.screensaver_query_info(x.root).ok()?.reply().ok()).map_or(0, |info| info.ms_since_user_input as i64)
    }
}

/// Game controllers, which the desktop's idle time ignores, read from the joystick devices Linux
/// makes for them without waiting. They're kept open: opening one replays its whole state.
#[cfg(not(any(windows, target_os = "macos")))]
mod pads {
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::{ErrorKind, Read};
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    const RESCAN_EVERY: Duration = Duration::from_secs(5);
    const BUTTON: u8 = 0x01;
    const AXIS: u8 = 0x02;
    /// Marks the events a device replays when opened, which aren't input.
    const INIT: u8 = 0x80;

    struct Pad {
        path: PathBuf,
        file: File,
        /// Where each stick and trigger last counted as input.
        axes: HashMap<usize, f64>,
    }

    struct Pads {
        open: Vec<Pad>,
        next_scan: Option<Instant>,
        last_input: Option<Instant>,
    }

    static PADS: Mutex<Pads> = Mutex::new(Pads { open: Vec::new(), next_scan: None, last_input: None });

    /// None until a controller has been used since Kairo started.
    pub fn idle_ms() -> Option<i64> {
        let mut pads = PADS.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if pads.next_scan.map_or(true, |at| now >= at) {
            pads.next_scan = Some(now + RESCAN_EVERY);
            let found: Vec<PathBuf> = std::fs::read_dir("/dev/input")
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("js")))
                .collect();
            for path in found {
                if pads.open.iter().any(|pad| pad.path == path) {
                    continue;
                }
                if let Ok(file) = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(&path) {
                    pads.open.push(Pad { path, file, axes: HashMap::new() });
                }
            }
        }
        let mut used = false;
        // A controller that's unplugged stops reading and is dropped.
        pads.open.retain_mut(|pad| loop {
            // Each event: time (4 bytes), value (2), kind (1), which button or axis (1).
            let mut event = [0u8; 8];
            match pad.file.read(&mut event) {
                Ok(8) => {
                    let value = i16::from_ne_bytes([event[4], event[5]]);
                    let (kind, number) = (event[6], event[7] as usize);
                    let at = (value as f64 + 32767.0) / 65534.0;
                    if kind & INIT != 0 {
                        pad.axes.insert(number, at);
                    } else if kind == BUTTON || (kind == AXIS && super::moved(&mut pad.axes, number, at)) {
                        used = true;
                    }
                }
                Ok(0) => break false,
                Ok(_) => {}
                Err(e) => break e.kind() == ErrorKind::WouldBlock,
            }
        });
        if used {
            pads.last_input = Some(now);
        }
        pads.last_input.map(|at| now.duration_since(at).as_millis() as i64)
    }
}

/// Whether a controller's stick or trigger, `at` of the way along its travel, moved a quarter of
/// its travel since it last counted as input; a resting stick drifts a little, which isn't.
#[cfg(not(windows))]
fn moved(seen: &mut std::collections::HashMap<usize, f64>, axis: usize, at: f64) -> bool {
    let far = seen.get(&axis).map_or(true, |&last| (at - last).abs() >= 0.25);
    if far {
        seen.insert(axis, at);
    }
    far
}

/// The program a process runs, from /proc. A Windows program under Wine or Proton goes by the
/// .exe it started, as it would on Windows, rather than by Wine's loader, so games keep their
/// names and share categories and limits with Windows.
#[cfg(not(any(windows, target_os = "macos")))]
fn program(pid: u32) -> Option<String> {
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?.to_string_lossy().into_owned();
    if exe.rsplit('/').next().is_some_and(|file| file.starts_with("wine")) {
        if let Some(windows) = std::fs::read(format!("/proc/{pid}/cmdline")).ok().and_then(|c| windows_program(&c)) {
            return Some(windows);
        }
    }
    Some(exe)
}

/// The .exe in a Wine process's command line, whose arguments are separated by nulls.
#[cfg(not(any(windows, target_os = "macos")))]
fn windows_program(cmdline: &[u8]) -> Option<String> {
    cmdline.split(|&b| b == 0).map(String::from_utf8_lossy).find(|arg| arg.to_ascii_lowercase().ends_with(".exe")).map(|arg| arg.into_owned())
}

/// Where a program Kairo knows by its Windows name is installed, from the commands on the PATH
/// its Linux packages add, e.g. `google-chrome` for `chrome.exe`.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn app_path(exe: &str) -> Option<String> {
    let stem = exe.strip_suffix(".exe").unwrap_or(exe);
    let commands: &[&str] = match stem {
        "chrome" => &["google-chrome", "google-chrome-stable"],
        "brave" => &["brave-browser", "brave"],
        "msedge" => &["microsoft-edge", "microsoft-edge-stable"],
        _ => std::slice::from_ref(&stem),
    };
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .flat_map(|dir| commands.iter().map(move |c| dir.join(c)))
        .find(|p| p.is_file())
        .map(|p| p.display().to_string())
}

/// Only the Windows command palette draws icons from pixels.
#[cfg(not(windows))]
pub fn icon_pixels(_path: &str) -> Option<(u32, u32, Vec<u8>)> {
    None
}

/// Opens a file as the desktop would. A desktop entry (an installed app) goes through GLib's
/// launcher, which every GNOME, KDE and most other desktops have, or its older GTK cousin.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn open_path(path: &str) -> bool {
    use std::process::{Command, Stdio};
    let run = |program: &str, args: &[&str]| {
        Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
    };
    if let Some(id) = path.strip_suffix(".desktop").and_then(|p| p.rsplit('/').next()) {
        return run("gio", &["launch", path]) || run("gtk-launch", &[id]);
    }
    // xdg-open can wait on what it opened, so it's left to finish on its own.
    match Command::new("xdg-open").arg(path).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
            true
        }
        Err(_) => false,
    }
}

/// Every running process this user can see: (process id, full path of its program).
#[cfg(not(any(windows, target_os = "macos")))]
pub fn processes() -> Vec<(u32, String)> {
    let Ok(entries) = std::fs::read_dir("/proc") else { return Vec::new() };
    entries.flatten().filter_map(|e| e.file_name().to_str()?.parse().ok()).filter_map(|pid| Some((pid, program(pid)?))).collect()
}

/// Ends a process outright; false when it's gone already or isn't this user's.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn close_process(pid: u32) -> bool {
    // 0 would mean Kairo's own process group to kill().
    pid != 0 && i32::try_from(pid).is_ok_and(|pid| unsafe { libc::kill(pid, libc::SIGKILL) } == 0)
}

#[cfg(all(test, not(windows)))]
mod pad_tests {
    #[test]
    fn sticks_count_when_moved_not_when_drifting() {
        let mut seen = std::collections::HashMap::new();
        // First seen counts, as it's a change; drift around it doesn't; a real push does.
        assert!(super::moved(&mut seen, 0, 0.5));
        assert!(!super::moved(&mut seen, 0, 0.53));
        assert!(super::moved(&mut seen, 0, 0.95));
        // A trigger resting at one end isn't measured from the middle.
        seen.insert(1, 0.0);
        assert!(!super::moved(&mut seen, 1, 0.02));
    }
}

#[cfg(all(test, not(any(windows, target_os = "macos"))))]
mod linux_tests {
    #[test]
    fn wine_processes_go_by_their_exe() {
        let cmdline = b"Z:\\home\\me\\.steam\\steam\\steamapps\\common\\Hades\\x64\\Hades.exe\0-vulkan\0";
        assert_eq!(super::windows_program(cmdline).as_deref(), Some("Z:\\home\\me\\.steam\\steam\\steamapps\\common\\Hades\\x64\\Hades.exe"));
        assert_eq!(super::windows_program(b"/usr/bin/wineserver\0"), None);
    }

    #[test]
    fn the_process_list_includes_this_test_and_unknown_processes_cant_be_closed() {
        let me = std::process::id();
        assert!(super::processes().iter().any(|(pid, _)| *pid == me));
        // init belongs to root; nothing else gets to end it.
        assert!(!super::close_process(1));
    }

    #[test]
    fn nothing_is_in_front_without_a_display() {
        if std::env::var_os("DISPLAY").is_none() {
            assert!(super::foreground().is_none());
            assert!(!super::fullscreen_in_front());
        }
    }
}

#[cfg(test)]
mod window_tests {
    use super::*;

    fn window(id: &str, title: &str, pid: u32) -> OpenWindow {
        OpenWindow { id: id.into(), title: title.into(), pid, path: None }
    }

    #[test]
    fn switchable_drops_kairo_and_blank_titles_and_keeps_order() {
        let long = "x".repeat(300);
        let kept = switchable(vec![window("a", "Budget", 5), window("b", "  ", 6), window("c", "Chrono", 99), window("d", &long, 7)], 99);
        assert_eq!(kept.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), ["a", "d"]);
        assert_eq!(kept[1].title.chars().count(), 200);
    }
}

#[cfg(all(test, not(any(windows, target_os = "macos"))))]
mod linux_window_tests {
    #[test]
    fn x11_window_ids_are_read() {
        assert_eq!(super::x11::window_of("4194307"), Some(4194307));
        assert_eq!(super::x11::window_of("0"), None);
        assert_eq!(super::x11::window_of("g12"), None);
    }
}
