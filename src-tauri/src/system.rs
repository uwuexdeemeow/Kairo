//! System commands for the palette: lock, sleep, log out, restart, shut down, emptying the trash,
//! the volume, the system's dark or light mode, and ending a program by name. Each system does the first five its own way, with nothing but what it has
//! already; none forces anything past a program with unsaved work.

use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::sync::Mutex;

use crate::{classify, platform};

/// What the palette can ask the computer to do with itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Power {
    Lock,
    Sleep,
    LogOut,
    Restart,
    ShutDown,
    /// Deletes what's in the trash (the Recycle Bin on Windows) for good.
    EmptyTrash,
    /// Asks every app with a window open to close.
    QuitAll,
}

impl Power {
    pub const ALL: [Power; 7] = [Power::Lock, Power::Sleep, Power::LogOut, Power::Restart, Power::ShutDown, Power::EmptyTrash, Power::QuitAll];

    /// Whether it asks twice: the ones that end programs and lose unsaved work, or delete for
    /// good, if pressed by accident.
    pub fn confirms(self) -> bool {
        matches!(self, Power::LogOut | Power::Restart | Power::ShutDown | Power::EmptyTrash | Power::QuitAll)
    }

    /// The row's title.
    pub fn title(self) -> &'static str {
        match self {
            Power::Lock => "Lock the computer",
            Power::Sleep => "Put the computer to sleep",
            Power::LogOut => "Log out",
            Power::Restart => "Restart",
            Power::ShutDown => "Shut down",
            Power::EmptyTrash if cfg!(windows) => "Empty the Recycle Bin",
            Power::EmptyTrash => "Empty the trash",
            Power::QuitAll => "Quit all apps",
        }
    }

    /// What it's called in a sentence: "Couldn't shut down."
    pub fn verb(self) -> &'static str {
        match self {
            Power::Lock => "lock the computer",
            Power::Sleep => "put the computer to sleep",
            Power::LogOut => "log out",
            Power::Restart => "restart",
            Power::ShutDown => "shut down",
            Power::EmptyTrash if cfg!(windows) => "empty the Recycle Bin",
            Power::EmptyTrash => "empty the trash",
            Power::QuitAll => "quit all apps",
        }
    }

    /// The row's second line.
    pub fn about(self) -> &'static str {
        match self {
            Power::Lock => "Locks the screen; everything keeps running",
            Power::Sleep => "Wakes with a key or the power button",
            Power::LogOut => "Asks to confirm first",
            Power::Restart => "Asks to confirm first",
            Power::ShutDown => "Asks to confirm first",
            Power::EmptyTrash => "Deletes it for good; asks to confirm first",
            Power::QuitAll => "Closes every window, so you can save first; asks to confirm",
        }
    }

    /// What typed outright finds it, and other words it's found by.
    pub fn words(self) -> (&'static [&'static str], &'static str) {
        match self {
            Power::Lock => (&["lock", "lock screen", "lock computer", "lock pc"], "lock screen computer secure"),
            Power::Sleep => (&["sleep", "suspend", "standby"], "sleep suspend standby computer"),
            Power::LogOut => (&["log out", "logout", "sign out", "signout", "log off", "logoff"], "log out sign out log off user"),
            Power::Restart => (&["restart", "reboot"], "restart reboot computer"),
            Power::ShutDown => (&["shut down", "shutdown", "power off", "poweroff", "turn off"], "shut down power off turn off computer"),
            Power::EmptyTrash => (&["empty trash", "empty the trash", "empty recycle bin", "empty the recycle bin", "empty bin"], "empty trash recycle bin delete bin"),
            Power::QuitAll => (&["quit all", "quit all apps", "close all apps", "close all windows"], "quit all apps close every window"),
        }
    }
}

/// What is waiting for its second Enter, with the text it was asked for in, so that typing
/// anything else leaves it behind. Held between the palette's keystrokes.
#[derive(Default)]
pub struct PendingPower(Mutex<Option<(Power, String)>>);

impl PendingPower {
    pub fn set(&self, power: Power, typed: String) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some((power, typed));
    }

    pub fn clear(&self) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    pub fn get(&self) -> Option<(Power, String)> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// Runs a command, true when it started and finished well; nothing it prints is wanted.
fn ran(program: &str, args: &[&str]) -> bool {
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up for a command-line tool.
        command.creation_flags(0x0800_0000);
    }
    command.status().is_ok_and(|s| s.success())
}

/// Does it, or says why it couldn't.
pub fn run(power: Power) -> Result<(), String> {
    if power == Power::QuitAll {
        let open = platform::switchable(platform::open_windows(), std::process::id());
        let ids: Vec<String> = open.into_iter().map(|w| w.id).collect();
        return match platform::quit_windows(&ids) {
            0 => Ok(()),
            _ => Err("Some apps didn't let Kairo close them. They may be running as administrator.".into()),
        };
    }
    if perform(power) {
        return Ok(());
    }
    let hint = match power {
        Power::EmptyTrash if cfg!(target_os = "macos") => " Allow Kairo to control Finder in System Settings → Privacy & Security → Automation.",
        Power::Lock => "",
        _ if cfg!(target_os = "macos") => " Allow Kairo to control System Events in System Settings → Privacy & Security → Automation.",
        _ => "",
    };
    Err(format!("Couldn't {}. The system didn't allow it.{hint}", power.verb()))
}

#[cfg(windows)]
fn perform(power: Power) -> bool {
    use windows_sys::Win32::System::Power::SetSuspendState;
    use windows_sys::Win32::System::Shutdown::LockWorkStation;
    match power {
        Power::Lock => unsafe { LockWorkStation() != 0 },
        Power::Sleep => {
            // Returns once the computer wakes again, so not on the palette's thread.
            std::thread::spawn(|| unsafe {
                SetSuspendState(0, 0, 0);
            });
            true
        }
        Power::LogOut => ran("shutdown", &["/l"]),
        Power::Restart => ran("shutdown", &["/r", "/t", "0"]),
        Power::ShutDown => ran("shutdown", &["/s", "/t", "0"]),
        Power::EmptyTrash => {
            use windows_sys::Win32::UI::Shell::{SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND};
            let result = unsafe { SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND) };
            // An empty bin answers E_UNEXPECTED: there was nothing to do.
            result >= 0 || result as u32 == 0x8000_FFFF
        }
        // Done by `run`, the same way on every system.
        Power::QuitAll => false,
    }
}

#[cfg(target_os = "macos")]
fn perform(power: Power) -> bool {
    let event = |what: &str| ran("osascript", &["-e", &format!("tell application \"System Events\" to {what}")]);
    match power {
        // The lock-screen key chord, which needs the Accessibility access Kairo asks for anyway;
        // failing that, the display sleeps and locks if the Mac is set to ask for a password.
        Power::Lock => event("keystroke \"q\" using {control down, command down}") || ran("pmset", &["displaysleepnow"]),
        Power::Sleep => event("sleep"),
        Power::LogOut => event("log out"),
        Power::Restart => event("restart"),
        Power::ShutDown => event("shut down"),
        Power::EmptyTrash => ran("osascript", &["-e", "tell application \"Finder\" to empty trash"]),
        Power::QuitAll => false,
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn perform(power: Power) -> bool {
    match power {
        Power::Lock => ran("loginctl", &["lock-session"]) || ran("xdg-screensaver", &["lock"]),
        Power::Sleep => ran("systemctl", &["suspend"]),
        Power::LogOut => match std::env::var("XDG_SESSION_ID") {
            Ok(session) if !session.is_empty() => ran("loginctl", &["terminate-session", &session]),
            _ => ran("loginctl", &["terminate-user", &std::env::var("USER").unwrap_or_default()]),
        },
        Power::Restart => ran("systemctl", &["reboot"]),
        Power::ShutDown => ran("systemctl", &["poweroff"]),
        Power::EmptyTrash => ran("gio", &["trash", "--empty"]),
        Power::QuitAll => false,
    }
}

// ---------- the volume and the system's look ----------

/// Settings of the computer the palette changes at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    VolumeUp,
    VolumeDown,
    /// Mutes, or unmutes when muted.
    Mute,
    /// The volume set to this percent.
    Volume(u8),
    /// The system in dark mode (true) or light mode.
    Dark(bool),
}

impl Control {
    /// The ones found by their words; `volume 40` is read on its own.
    pub const LISTED: [Control; 5] = [Control::VolumeUp, Control::VolumeDown, Control::Mute, Control::Dark(true), Control::Dark(false)];

    pub fn title(self) -> String {
        match self {
            Control::VolumeUp => "Volume up".into(),
            Control::VolumeDown => "Volume down".into(),
            Control::Mute => "Mute or unmute".into(),
            Control::Volume(n) => format!("Set the volume to {n}%"),
            Control::Dark(true) => "Turn on dark mode".into(),
            Control::Dark(false) => "Turn on light mode".into(),
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Control::VolumeUp | Control::VolumeDown => "Press Enter again for more",
            Control::Mute => "The computer's sound",
            Control::Volume(_) => "The computer's sound, unmuted",
            Control::Dark(_) => "For the whole system, not only Kairo",
        }
    }

    /// What typed outright finds it, and other words it's found by.
    pub fn words(self) -> (&'static [&'static str], &'static str) {
        match self {
            Control::VolumeUp => (&["volume up", "vol up", "louder", "turn up", "sound up"], "volume sound audio louder up raise"),
            Control::VolumeDown => (&["volume down", "vol down", "quieter", "softer", "turn down", "sound down"], "volume sound audio quieter softer down lower"),
            Control::Mute => (&["mute", "unmute", "mute sound", "silence"], "mute unmute volume sound audio silence quiet"),
            Control::Volume(_) => (&[], ""),
            Control::Dark(true) => (&["dark mode", "system dark mode", "dark mode on", "light mode off"], "dark mode appearance system night"),
            Control::Dark(false) => (&["light mode", "system light mode", "dark mode off", "light mode on"], "light mode appearance system day"),
        }
    }

    /// `volume 40`, `volume 40%`, `set volume 40`, `vol 0`: the volume asked for.
    pub fn volume_typed(q: &str) -> Option<Control> {
        let q = q.trim().to_lowercase();
        let rest = ["set volume to ", "set volume ", "volume to ", "volume ", "vol "].iter().find_map(|p| q.strip_prefix(p))?;
        let n: u8 = rest.trim().trim_end_matches('%').trim().parse().ok()?;
        (n <= 100).then_some(Control::Volume(n))
    }
}

/// Changes it, or says why it couldn't.
pub fn control(c: Control) -> Result<(), String> {
    if adjust(c) {
        return Ok(());
    }
    let what = match c {
        Control::Dark(_) if cfg!(target_os = "macos") => "change the appearance. Allow Kairo to control System Events in System Settings → Privacy & Security → Automation.",
        Control::Dark(_) => "change the appearance here.",
        _ => "change the volume here.",
    };
    Err(format!("Couldn't {what}"))
}

/// Presses a media key `times` times, as the keyboard's own volume keys would.
#[cfg(windows)]
fn media_key(vk: u16, times: usize) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP};
    let key = |up: bool| INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { 0 }, time: 0, dwExtraInfo: 0 } } };
    let inputs: Vec<INPUT> = (0..times).flat_map(|_| [key(false), key(true)]).collect();
    unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) == inputs.len() as u32 }
}

#[cfg(windows)]
fn adjust(c: Control) -> bool {
    const UP: u16 = 0xAF;
    const DOWN: u16 = 0xAE;
    const MUTE: u16 = 0xAD;
    match c {
        // Each press is 2%, as the keyboard's.
        Control::VolumeUp => media_key(UP, 3),
        Control::VolumeDown => media_key(DOWN, 3),
        Control::Mute => media_key(MUTE, 1),
        // All the way down, then up to it; a press up also unmutes.
        Control::Volume(n) => media_key(DOWN, 50) && media_key(UP, (n as usize).div_ceil(2)),
        Control::Dark(on) => unsafe {
            use windows_sys::Win32::Foundation::LPARAM;
            use windows_sys::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD};
            use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};
            let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
            let key = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
            let light: u32 = if on { 0 } else { 1 };
            let set = ["AppsUseLightTheme", "SystemUsesLightTheme"].iter().all(|name| {
                RegSetKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), wide(name).as_ptr(), REG_DWORD, &light as *const u32 as *const _, 4) == 0
            });
            // Explorer and open apps repaint when told the colours changed.
            let what = wide("ImmersiveColorSet");
            SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, what.as_ptr() as LPARAM, SMTO_ABORTIFHUNG, 1000, std::ptr::null_mut());
            set
        },
    }
}

#[cfg(target_os = "macos")]
fn adjust(c: Control) -> bool {
    let script = |lines: &[&str]| {
        let args: Vec<&str> = lines.iter().flat_map(|l| ["-e", *l]).collect();
        ran("osascript", &args)
    };
    match c {
        Control::VolumeUp => script(&["set volume output volume ((output volume of (get volume settings)) + 6)", "set volume without output muted"]),
        Control::VolumeDown => script(&["set volume output volume ((output volume of (get volume settings)) - 6)"]),
        Control::Mute => script(&["set volume output muted (not (output muted of (get volume settings)))"]),
        Control::Volume(n) => script(&[&format!("set volume output volume {n}"), "set volume without output muted"]),
        Control::Dark(on) => script(&[&format!("tell application \"System Events\" to tell appearance preferences to set dark mode to {on}")]),
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn adjust(c: Control) -> bool {
    // PipeWire's own tool, then PulseAudio's, then ALSA's: whichever this desktop has.
    let volume = |wpctl: &[&str], pactl: &[&str], amixer: &[&str]| ran("wpctl", wpctl) || ran("pactl", pactl) || ran("amixer", amixer);
    match c {
        Control::VolumeUp => volume(&["set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", "6%+"], &["set-sink-volume", "@DEFAULT_SINK@", "+6%"], &["-q", "set", "Master", "6%+", "unmute"]),
        Control::VolumeDown => volume(&["set-volume", "@DEFAULT_AUDIO_SINK@", "6%-"], &["set-sink-volume", "@DEFAULT_SINK@", "-6%"], &["-q", "set", "Master", "6%-"]),
        Control::Mute => volume(&["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"], &["set-sink-mute", "@DEFAULT_SINK@", "toggle"], &["-q", "set", "Master", "toggle"]),
        Control::Volume(n) => {
            let (level, percent) = (format!("{:.2}", n as f32 / 100.0), format!("{n}%"));
            let set = volume(&["set-volume", "@DEFAULT_AUDIO_SINK@", &level], &["set-sink-volume", "@DEFAULT_SINK@", &percent], &["-q", "set", "Master", &percent, "unmute"]);
            let _ = ran("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "0"]) || ran("pactl", &["set-sink-mute", "@DEFAULT_SINK@", "0"]);
            set
        }
        Control::Dark(on) => {
            let kde = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.to_uppercase().contains("KDE"));
            if kde {
                ran("plasma-apply-colorscheme", &[if on { "BreezeDark" } else { "BreezeLight" }])
            } else {
                ran("gsettings", &["set", "org.gnome.desktop.interface", "color-scheme", if on { "prefer-dark" } else { "default" }])
            }
        }
    }
}

// ---------- ending programs ----------

/// A running program, by the name the rest of Kairo knows it by, and how many processes it has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub name: String,
    pub count: usize,
}

/// Programs that must stay: without them the system, the desktop or the session goes.
const PROTECTED: [&str; 26] = [
    // Windows
    "system", "system idle process", "registry", "smss", "csrss", "wininit", "winlogon", "services", "lsass", "svchost", "dwm", "fontdrvhost",
    // macOS
    "launchd", "kernel_task", "loginwindow", "windowserver",
    // Linux
    "systemd", "init", "kthreadd", "dbus-daemon", "xorg", "xwayland", "gnome-shell", "kwin_x11", "kwin_wayland",
    // Kairo's own windows are drawn in a web view that other programs use too.
    "msedgewebview2",
];

/// Whether a process must not be listed or ended: Kairo's own, a system one, or a Mac's own part
/// of macOS.
pub fn is_protected(name: &str, path: &str, pid: u32, own_pid: u32) -> bool {
    pid <= 1 || pid == own_pid || name == "kairo" || PROTECTED.contains(&name) || path.starts_with("/System/")
}

fn named(processes: &[(u32, String)], own_pid: u32) -> BTreeMap<String, Vec<u32>> {
    let mut by_name: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for (pid, path) in processes {
        let name = classify::exe_stem(path);
        if !name.is_empty() && !is_protected(&name, path, *pid, own_pid) {
            by_name.entry(name).or_default().push(*pid);
        }
    }
    by_name
}

/// The programs running, each once with its number of processes, by name.
pub fn programs(processes: &[(u32, String)], own_pid: u32) -> Vec<Program> {
    named(processes, own_pid).into_iter().map(|(name, pids)| Program { name, count: pids.len() }).collect()
}

/// The processes of the program called `name`.
pub fn pids_named(processes: &[(u32, String)], name: &str, own_pid: u32) -> Vec<u32> {
    named(processes, own_pid).remove(name).unwrap_or_default()
}

/// Ends every process of the program called `name`, outright. How many ended, or why none did.
pub fn kill_named(name: &str) -> Result<usize, String> {
    let pids = pids_named(&platform::processes(), name, std::process::id());
    let ended = pids.iter().filter(|&&pid| platform::close_process(pid)).count();
    match (pids.len(), ended) {
        (0, _) => Err(format!("{name} isn't running any more.")),
        (_, 0) => Err(format!("Couldn't end {name}. It may be running as administrator.")),
        (_, n) => Ok(n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN: u32 = 4242;

    fn procs() -> Vec<(u32, String)> {
        [
            (0, "System Idle Process"),
            (4, "System"),
            (100, "chrome.exe"),
            (101, "chrome.exe"),
            (102, "Chrome.exe"),
            (200, "Code.exe"),
            (300, "svchost.exe"),
            (OWN, "kairo.exe"),
            (4300, "msedgewebview2.exe"),
            (500, "/Applications/Slack.app/Contents/MacOS/Slack"),
            (501, "/System/Library/CoreServices/Finder.app/Contents/MacOS/Finder"),
            (600, "/usr/bin/firefox"),
            (601, "/usr/bin/firefox"),
        ]
        .into_iter()
        .map(|(pid, path)| (pid, path.to_string()))
        .collect()
    }

    #[test]
    fn programs_are_grouped_by_name_with_their_counts_and_sorted() {
        let list = programs(&procs(), OWN);
        let names: Vec<(&str, usize)> = list.iter().map(|p| (p.name.as_str(), p.count)).collect();
        assert_eq!(names, [("chrome", 3), ("code", 1), ("firefox", 2), ("slack", 1)]);
    }

    #[test]
    fn what_must_stay_is_never_listed_or_ended() {
        let all = procs();
        let listed: Vec<String> = programs(&all, OWN).into_iter().map(|p| p.name).collect();
        for hidden in ["system", "system idle process", "svchost", "kairo", "msedgewebview2", "finder"] {
            assert!(!listed.iter().any(|n| n == hidden), "{hidden} is listed");
            assert!(pids_named(&all, hidden, OWN).is_empty(), "{hidden} can be ended");
        }
        // Kairo's own process goes by its id too, whatever it's called.
        assert!(is_protected("whatever", "whatever", OWN, OWN));
        assert!(is_protected("init", "/sbin/init", 1, OWN));
        assert!(!is_protected("chrome", "chrome.exe", 100, OWN));
        for name in PROTECTED {
            assert!(is_protected(name, name, 999, OWN), "{name}");
        }
    }

    #[test]
    fn a_program_is_ended_by_exactly_its_own_processes() {
        let all = procs();
        assert_eq!(pids_named(&all, "chrome", OWN), [100, 101, 102]);
        assert_eq!(pids_named(&all, "firefox", OWN), [600, 601]);
        assert!(pids_named(&all, "chro", OWN).is_empty());
        assert!(pids_named(&all, "nothing", OWN).is_empty());
    }

    #[test]
    fn the_power_commands_say_what_they_are() {
        let confirming: Vec<Power> = Power::ALL.into_iter().filter(|p| p.confirms()).collect();
        assert_eq!(confirming, [Power::LogOut, Power::Restart, Power::ShutDown, Power::EmptyTrash, Power::QuitAll]);
        for p in Power::ALL {
            let (typed, loose) = p.words();
            assert!(!typed.is_empty() && !loose.is_empty() && !p.title().is_empty() && !p.verb().is_empty() && !p.about().is_empty(), "{p:?}");
        }
        // No word finds two commands outright.
        let mut seen = std::collections::HashSet::new();
        for p in Power::ALL {
            for word in p.words().0 {
                assert!(seen.insert(*word), "{word} finds two commands");
            }
        }
    }

    #[test]
    fn what_waits_for_its_second_enter_is_remembered_until_cleared() {
        let pending = PendingPower::default();
        assert_eq!(pending.get(), None);
        pending.set(Power::ShutDown, "shut down".into());
        assert_eq!(pending.get(), Some((Power::ShutDown, "shut down".to_string())));
        pending.clear();
        assert_eq!(pending.get(), None);
    }
}
