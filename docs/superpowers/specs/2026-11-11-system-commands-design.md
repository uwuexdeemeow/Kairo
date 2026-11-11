# Palette: system commands (lock, sleep, log out, restart, shut down, kill)

## Why

The palette opens apps and runs Kairo's own actions, but locking the screen, putting the computer
to sleep, shutting it down or ending a hung program each mean leaving the keyboard for the Start
menu or the task manager. These are things people type: `lock`, `shut down`, `kill chrome`.

## What

Commands are typed plainly, as `pause` and `extend` are: a row says what Enter will do. They sit at
the top when their words lead the query. Typed loosely they rank with the other actions, below a
strong app match, and a loose match needs at least three characters typed so two letters don't
offer to restart the computer. All rows are in a "System" section.

| Typed | Row | Enter |
|---|---|---|
| `lock`, `lock screen`, `lock computer` | Lock the computer | at once |
| `sleep`, `suspend`, `standby` | Put the computer to sleep | at once |
| `log out`, `logout`, `sign out`, `log off`, `logoff` | Log out | asks twice |
| `restart`, `reboot` | Restart | asks twice |
| `shut down`, `shutdown`, `power off`, `turn off` | Shut down | asks twice |
| `kill chrome`, `end task chrome`, `force quit chrome` | Kill chrome · 12 processes | at once |

### Asking twice

Logging out, restarting and shutting down lose unsaved work if pressed by accident, so the first
Enter changes nothing but the row: it becomes **Confirm: shut down** in the warm colour, with a
**Cancel** row under it. The palette stays open. Enter on Confirm does it and puts the palette away.
Cancel, Esc, closing the palette or typing something else leaves things as they were (typing the
same command again in the same opening shows Confirm straight away, clearly labelled). Lock and
sleep act at once: neither loses anything.

None of them force anything: programs with unsaved work get asked first by Windows and macOS, and
the system's own "programs are preventing shutdown" screen appears.

### Kill

`kill` names running programs: `kill chrome` lists the programs whose name matches, up to 8, each
as "Kill chrome", with how many processes it has ("12 processes", "1 process") and a warning that
unsaved work in them is lost. Enter ends every process of that program outright, the way closing
game launchers already does (`close_process`: TerminateProcess, SIGKILL). Typed alone, `kill` says
"Type a program's name, like kill chrome".

A program's name is the one the rest of Kairo uses (`classify::exe_stem`: `chrome`, `code`, a Mac
app by its bundle). Never offered, and never ended: Kairo itself; the web view Kairo's windows are
drawn in (`msedgewebview2` on Windows; anything under `/System/` on a Mac); and critical system
processes (`system`, `smss`, `csrss`, `wininit`, `winlogon`, `services`, `lsass`, `svchost`, `dwm`,
`fontdrvhost`; `launchd`, `kernel_task`, `loginwindow`, `windowserver`; `systemd`, `init`,
`kthreadd`, `dbus-daemon`, `xorg`, `xwayland`, `gnome-shell`, `kwin_x11`, `kwin_wayland`). The
system refuses to end what belongs to another user or runs with more rights; then the row's
result is "Couldn't end chrome. It may be running as administrator." and when only some processes
ended, it counts as done.

### Not in this

Hibernate. Remote, delayed or scheduled shutdowns. Network, display or volume toggles. Ending
other users' processes. Forcing a shutdown past programs with unsaved work.

## How

### `src-tauri/src/system.rs` (new)

- `Power { Lock, Sleep, LogOut, Restart, ShutDown }` with `confirms()` (the last three), a title
  and a verb for messages, and the words that find it.
- `run(Power) -> Result<(), String>`, per system, with no new dependency:
  - **Windows:** `LockWorkStation` and `SetSuspendState(false, false, false)` through `windows-sys`
    (two more features of the crate already used: `Win32_System_Shutdown`, `Win32_System_Power`;
    sleep on its own thread, since the call returns after waking), and `shutdown /s`, `/r`, `/l`
    with `/t 0`, never `/f`, with no console window.
  - **macOS:** `osascript` through System Events to shut down, restart, log out and sleep; lock
    with the lock-screen key chord, falling back to `pmset displaysleepnow`. macOS asks once to let
    Kairo control System Events (Automation), as it asks for Accessibility.
  - **Linux:** `systemctl poweroff | reboot | suspend`; `loginctl lock-session`, falling back to
    `xdg-screensaver lock`; `loginctl terminate-session $XDG_SESSION_ID`, or `terminate-user` with
    no session id.
  - A failure is "Couldn't <verb>. The system didn't allow it." (on a Mac, naming the Automation
    setting).
- `Program { name, count }`; `programs(processes, own_pid)`: the running programs by name, with
  how many processes, without the protected ones and Kairo's own, sorted by name.
  `is_protected(name, path)`; `pids_named(processes, name, own_pid)`.
- `kill_named(name) -> Result<usize, String>`: ends every process of that program that isn't
  protected, and says how many ended.
- `PendingPower`: the managed state holding what is waiting for its second Enter, with what was
  typed; `Default`.

### Palette

- `search::Action` gains `Power(Power)` (lock, sleep), `ConfirmPower(Power, String)` (the first
  Enter, with what was typed), `PowerNow(Power)`, `CancelPower` and `Kill(String)`. `ConfirmPower`
  and `CancelPower` act in place (`acts_in_place`), so the palette stays open and asks again, as
  for a theme.
- `Context.programs` (read as the palette opens, a name and a count each) and
  `Live.pending_power: Option<(Power, String)>`. Opening the palette clears what was pending.
- `palette/commands.rs` gains `system_rows`: the rows above; the Confirm and Cancel rows when
  `Live.pending_power` has this command and the typed text is the same; the Kill rows, matched
  with `fuzzy` against each program's name. `palette/mod.rs` runs the actions.
- Both the web palette and the Windows native palette take the rows from the same search, so
  neither needs anything of its own.
- `src-tauri/Cargo.toml`: the two `windows-sys` features. `lib.rs`: `mod system` and the managed
  state.

All shared code except the per-system calls in `system::run`.

## Testing

The palette and the unit tests never run a power action: it would take over the machine.

- `system.rs`: programs grouped by name with counts; the protected list and Kairo's own process
  left out (Windows file names, Mac paths, Linux names); `pids_named` finds exactly one program's
  processes and never a protected one; every `Power` has words, a title and a verb, and exactly
  `LogOut`, `Restart` and `ShutDown` confirm.
- `palette/commands.rs`: each command's words find its row, loose matching needs three
  characters; the first Enter on shut down is `ConfirmPower` and acts in place; with it pending
  and the same text typed the top row is `Confirm: shut down` (`PowerNow`) followed by Cancel; with
  other text it isn't; lock and sleep are at once; `kill chro` lists matching programs with counts,
  `kill` alone gives the hint, and a program with no match gives a note.
- Device checklist (Windows and Mac; Linux unchecked): lock, sleep, the confirm step for each of
  the three, cancelling by Esc and by typing, `kill` on a program with several processes (a browser)
  and one with administrator rights, and that Kairo and the web view are never listed.
