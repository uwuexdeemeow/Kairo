//! OS hooks: which window is in front, and how long since the last keyboard/mouse input.

pub struct Foreground {
    /// Full path of the process that owns the foreground window.
    pub exe_path: String,
    pub title: String,
}

#[cfg(windows)]
pub use win::{foreground, idle_ms};

#[cfg(windows)]
mod win {
    use super::Foreground;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
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
            })
        }
    }

    pub fn idle_ms() -> i64 {
        unsafe {
            let mut info = LASTINPUTINFO {
                cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };
            if GetLastInputInfo(&mut info) == 0 {
                return 0;
            }
            // Both are 32-bit tick counts that wrap every ~49 days; wrapping_sub handles that.
            GetTickCount().wrapping_sub(info.dwTime) as i64
        }
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
