//! OS hooks: which window is in front, and how long since the last keyboard, mouse or controller input.

pub struct Foreground {
    /// Full path of the process that owns the foreground window.
    pub exe_path: String,
    pub title: String,
    /// The window itself, for minimizing it during a focus session.
    pub window: usize,
}

#[cfg(windows)]
pub use win::{app_path, close_process, foreground, icon_png, idle_ms, minimize, open_path, processes};

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
        let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
        let open: Vec<u16> = "open".encode_utf16().chain([0]).collect();
        // Values above 32 mean success.
        unsafe {
            ShellExecuteW(std::ptr::null_mut(), open.as_ptr(), wide.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL)
                as isize
                > 32
        }
    }

    /// A program's icon as Explorer shows it, as PNG bytes. None when it has none to give.
    pub fn icon_png(path: &str) -> Option<Vec<u8>> {
        use windows_sys::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;
        let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
        unsafe {
            let mut info: SHFILEINFOW = std::mem::zeroed();
            let found = SHGetFileInfoW(wide.as_ptr(), 0, &mut info, std::mem::size_of::<SHFILEINFOW>() as u32, SHGFI_ICON | SHGFI_LARGEICON);
            if found == 0 || info.hIcon.is_null() {
                return None;
            }
            let pixels = icon_rgba(info.hIcon);
            DestroyIcon(info.hIcon);
            let (width, height, rgba) = pixels?;
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

// Other platforms are not supported yet; the collector simply records nothing.
#[cfg(not(windows))]
pub fn foreground() -> Option<Foreground> {
    None
}

#[cfg(not(windows))]
pub fn idle_ms() -> i64 {
    0
}

#[cfg(not(windows))]
pub fn app_path(_exe: &str) -> Option<String> {
    None
}

#[cfg(not(windows))]
pub fn icon_png(_path: &str) -> Option<Vec<u8>> {
    None
}

#[cfg(not(windows))]
pub fn open_path(_path: &str) -> bool {
    false
}

#[cfg(not(windows))]
pub fn minimize(_window: usize) -> bool {
    false
}

#[cfg(not(windows))]
pub fn processes() -> Vec<(u32, String)> {
    Vec::new()
}

#[cfg(not(windows))]
pub fn close_process(_pid: u32) -> bool {
    false
}
