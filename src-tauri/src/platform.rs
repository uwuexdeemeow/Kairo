//! OS hooks: which window is in front, and how long since the last keyboard, mouse or controller input.

pub struct Foreground {
    /// Full path of the process that owns the foreground window.
    pub exe_path: String,
    pub title: String,
    /// The window itself, for minimizing it during a focus session (on a Mac, the app's process id).
    pub window: usize,
}

#[cfg(windows)]
pub use win::{app_path, close_process, foreground, fullscreen_in_front, icon_pixels, icon_png, idle_ms, minimize, open_path, processes, start_apps};

#[cfg(windows)]
mod win {
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

            // Limited access is enough to read the image name and works for most elevated processes too.
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return None;
            }
            let mut path = [0u16; 1024];
            let mut size = path.len() as u32;
            let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut size);
            CloseHandle(process);
            if ok == 0 {
                return None;
            }

            Some(Foreground {
                exe_path: String::from_utf16_lossy(&path[..size as usize]),
                title,
                window: hwnd as usize,
            })
        }
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
    /// full screen, or a window that isn't maximized yet covers its whole monitor, as borderless
    /// games and full-screen video and browsers do. The desktop covers it too, so it doesn't count.
    pub fn fullscreen_in_front() -> bool {
        use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL};
        use windows_sys::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_RUNNING_D3D_FULL_SCREEN};
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetShellWindow, GetWindowRect, IsZoomed};
        unsafe {
            let mut state = 0;
            if SHQueryUserNotificationState(&mut state) == 0 && state == QUNS_RUNNING_D3D_FULL_SCREEN {
                return true;
            }
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() || hwnd == GetShellWindow() || IsZoomed(hwnd) != 0 {
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
pub use mac::{app_path, close_process, foreground, frontmost_path, fullscreen_in_front, icon_png, idle_ms, minimize, open_path, processes, request_window_titles};

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

    /// Time since the last keyboard, mouse or trackpad input.
    pub fn idle_ms() -> i64 {
        let seconds = unsafe { CGEventSourceSecondsSinceLastEventType(COMBINED_SESSION_STATE, ANY_INPUT_EVENT) };
        (seconds * 1000.0).max(0.0) as i64
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

// Linux and others: other apps' windows come from X11. That covers X11 desktops and, under
// Wayland, apps running through XWayland, as most games do; Wayland shows apps no windows of
// native Wayland apps, so with one of those in front Kairo sees nothing. Programs come from /proc.
#[cfg(not(any(windows, target_os = "macos")))]
pub use x11::{foreground, fullscreen_in_front, idle_ms, minimize};

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

    pub fn foreground() -> Option<Foreground> {
        let x = Display::open()?;
        let window = x.active_window()?;
        let pid = x.property(window, "_NET_WM_PID", AtomEnum::CARDINAL, 1)?.value32()?.next()?;
        // The modern UTF-8 title, or the old one older programs set instead.
        let title = ["_NET_WM_NAME", "WM_NAME"]
            .iter()
            .find_map(|name| {
                let p = x.property(window, name, AtomEnum::ANY, 1024)?;
                (!p.value.is_empty()).then(|| String::from_utf8_lossy(&p.value).into_owned())
            })
            .unwrap_or_default();
        Some(Foreground { exe_path: super::program(pid)?, title, window: window as usize })
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

    /// Time since the last keyboard or mouse input. On Wayland, X11 hears only XWayland apps'
    /// input, so GNOME is asked instead; other Wayland desktops count as never idle.
    pub fn idle_ms() -> i64 {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return gnome_idle_ms().unwrap_or(0);
        }
        let Some(x) = Display::open() else { return 0 };
        x.conn.screensaver_query_info(x.root).ok().and_then(|c| c.reply().ok()).map_or(0, |info| info.ms_since_user_input as i64)
    }

    fn gnome_idle_ms() -> Option<i64> {
        let out = std::process::Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--dest",
                "org.gnome.Mutter.IdleMonitor",
                "--object-path",
                "/org/gnome/Mutter/IdleMonitor/Core",
                "--method",
                "org.gnome.Mutter.IdleMonitor.GetIdletime",
            ])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        out.status.success().then(|| dbus_number(&String::from_utf8_lossy(&out.stdout)))?
    }

    /// The number in a reply gdbus prints, like `(uint64 1234,)`.
    pub(super) fn dbus_number(reply: &str) -> Option<i64> {
        reply.trim().strip_prefix('(')?.split([' ', ',', ')']).find_map(|word| word.parse().ok())
    }
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

#[cfg(all(test, not(any(windows, target_os = "macos"))))]
mod linux_tests {
    #[test]
    fn wine_processes_go_by_their_exe() {
        let cmdline = b"Z:\\home\\me\\.steam\\steam\\steamapps\\common\\Hades\\x64\\Hades.exe\0-vulkan\0";
        assert_eq!(super::windows_program(cmdline).as_deref(), Some("Z:\\home\\me\\.steam\\steam\\steamapps\\common\\Hades\\x64\\Hades.exe"));
        assert_eq!(super::windows_program(b"/usr/bin/wineserver\0"), None);
    }

    #[test]
    fn gnome_idle_time_is_read_from_gdbus() {
        assert_eq!(super::x11::dbus_number("(uint64 1234,)\n"), Some(1234));
        assert_eq!(super::x11::dbus_number("nothing"), None);
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
