//! System commands for the palette: lock, sleep, log out, restart, shut down, and ending a
//! program by name. Each system does the first five its own way, with nothing but what it has
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
}

impl Power {
    pub const ALL: [Power; 5] = [Power::Lock, Power::Sleep, Power::LogOut, Power::Restart, Power::ShutDown];

    /// Whether it asks twice: the ones that end programs and lose unsaved work if pressed by accident.
    pub fn confirms(self) -> bool {
        matches!(self, Power::LogOut | Power::Restart | Power::ShutDown)
    }

    /// The row's title.
    pub fn title(self) -> &'static str {
        match self {
            Power::Lock => "Lock the computer",
            Power::Sleep => "Put the computer to sleep",
            Power::LogOut => "Log out",
            Power::Restart => "Restart",
            Power::ShutDown => "Shut down",
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
    if perform(power) {
        return Ok(());
    }
    let hint = if cfg!(target_os = "macos") && power != Power::Lock { " Allow Kairo to control System Events in System Settings → Privacy & Security → Automation." } else { "" };
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
        assert_eq!(confirming, [Power::LogOut, Power::Restart, Power::ShutDown]);
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
