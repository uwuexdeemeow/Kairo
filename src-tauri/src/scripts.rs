//! Script commands: your own scripts as palette commands. A script in Kairo's scripts folder is
//! found by its title; a few comment lines near its top say how it's shown and run:
//!
//! ```text
//! # @kairo.title Deploy site
//! # @kairo.description Builds and uploads the blog
//! # @kairo.argument Branch            (asks for one word or more after the title)
//! # @kairo.output notify             (notify, copy, paste or silent; notify when not said)
//! ```
//!
//! Any comment mark works (`#`, `//`, `--`, `;`, `REM`, `::`). Kairo picks the program to run it
//! by its ending (.ps1, .py, .js, .sh, .bat, .rb and others), or runs it as it is. A script runs
//! only when you press Enter on it, in the scripts folder, and is stopped after a minute. Scripts
//! stay on this computer: they never sync and aren't in backups.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

/// Scripts listed at most, and how much of each is read for its comment lines.
const MAX_SCRIPTS: usize = 100;
const HEAD_BYTES: usize = 4096;
const HEAD_LINES: usize = 30;
/// A script is stopped after this.
pub const TIME_LIMIT: Duration = Duration::from_secs(60);
/// Output kept at most.
const MAX_OUTPUT: usize = 100_000;
/// Lines of output a notification shows.
const NOTIFY_LINES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// Shows the output's first lines in a notification.
    Notify,
    /// Puts the output on the clipboard.
    Copy,
    /// Pastes the output into the window that was in front.
    Paste,
    /// Says nothing unless it fails.
    Silent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Script {
    pub path: String,
    pub title: String,
    pub description: Option<String>,
    /// What the argument is, when the script takes one.
    pub argument: Option<String>,
    pub output: Output,
}

/// Where your scripts go: `scripts` in Kairo's data folder.
pub fn folder(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("scripts"))
}

/// The value after `@kairo.<key>` on a comment line, if it is one.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let at = line.find(&format!("@kairo.{key}"))?;
    let before = line[..at].trim();
    // Only in a comment: what comes before is a comment mark and nothing else.
    let marks = ["#", "//", "--", ";", "rem", "::", "<#", "'", "/*", "*", "%"];
    if !before.is_empty() && !marks.iter().any(|m| before.eq_ignore_ascii_case(m)) {
        return None;
    }
    let rest = &line[at + "@kairo.".len() + key.len()..];
    // "@kairo.title" and not "@kairo.titles".
    if rest.starts_with(|c: char| c.is_alphanumeric()) {
        return None;
    }
    Some(rest.trim().trim_end_matches("*/").trim_end_matches("#>").trim()).filter(|v| !v.is_empty())
}

/// A script's comment lines read: None when it has no title and so isn't one Kairo lists, or its
/// file name isn't text.
pub fn parse(path: &Path, head: &str) -> Option<Script> {
    let lines: Vec<&str> = head.lines().take(HEAD_LINES).collect();
    let get = |key: &str| lines.iter().find_map(|l| field(l, key)).map(str::to_string);
    let title = get("title")?;
    let output = match get("output").map(|o| o.to_lowercase()).as_deref() {
        Some("copy") => Output::Copy,
        Some("paste") => Output::Paste,
        Some("silent") => Output::Silent,
        _ => Output::Notify,
    };
    Some(Script { path: path.to_str()?.to_string(), title: title.chars().take(60).collect(), description: get("description"), argument: get("argument"), output })
}

/// The scripts in the folder, by title. A file that can't be read, or has no title, is skipped.
pub fn list(dir: &Path) -> Vec<Script> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<Script> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .take(MAX_SCRIPTS * 2)
        .filter_map(|e| {
            let mut head = vec![0u8; HEAD_BYTES];
            let n = std::fs::File::open(e.path()).and_then(|mut f| f.read(&mut head)).ok()?;
            parse(&e.path(), &String::from_utf8_lossy(&head[..n]))
        })
        .take(MAX_SCRIPTS)
        .collect();
    found.sort_by_key(|s| s.title.to_lowercase());
    found
}

/// The files in the folder Kairo can't list because they have no `@kairo.title` line, by name, for
/// settings to say so. Hidden files are left out.
pub fn skipped(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .take(MAX_SCRIPTS * 2)
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            if name.starts_with('.') {
                return None;
            }
            let mut head = vec![0u8; HEAD_BYTES];
            let n = std::fs::File::open(e.path()).and_then(|mut f| f.read(&mut head)).ok()?;
            parse(&e.path(), &String::from_utf8_lossy(&head[..n])).is_none().then_some(name)
        })
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// The program that runs a script, by its ending, and what goes before the script's path.
pub fn runner(path: &Path) -> (String, Vec<String>) {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let windows = cfg!(windows);
    let (program, args): (&str, &[&str]) = match ext.as_str() {
        "ps1" if windows => ("powershell", &["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]),
        "ps1" => ("pwsh", &["-NoProfile", "-File"]),
        "py" if windows => ("python", &[]),
        "py" => ("python3", &[]),
        "js" | "mjs" => ("node", &[]),
        "rb" => ("ruby", &[]),
        "sh" | "bash" => ("bash", &[]),
        "zsh" => ("zsh", &[]),
        "bat" | "cmd" => ("cmd", &["/C"]),
        _ => return (path.to_string_lossy().into_owned(), Vec::new()),
    };
    let mut all: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    all.push(path.to_string_lossy().into_owned());
    (program.to_string(), all)
}

/// What a run gave: its output, or why it failed (with what it said on its error stream).
pub fn run(script: &Script, argument: Option<&str>) -> Result<String, String> {
    let path = Path::new(&script.path);
    let (program, mut args) = runner(path);
    args.extend(argument.map(str::to_string));
    let mut cmd = Command::new(&program);
    cmd.args(&args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(dir) = path.parent() {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("Kairo couldn't find {program} to run it."),
        std::io::ErrorKind::PermissionDenied => "That script isn't allowed to run. Make it executable, or give it an ending like .sh or .py.".to_string(),
        _ => e.to_string(),
    })?;
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut out = Vec::new();
            if let Some(p) = pipe {
                let _ = p.take(MAX_OUTPUT as u64).read_to_end(&mut out);
            }
            String::from_utf8_lossy(&out).into_owned()
        })
    };
    let out = read(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err = read(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > TIME_LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("It ran for over a minute, so Kairo stopped it.".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let (out, err) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
    if status.success() {
        Ok(out.trim_end().to_string())
    } else {
        let said = err.lines().chain(out.lines()).map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string();
        Err(match (status.code(), said.is_empty()) {
            (Some(code), true) => format!("It stopped with code {code}."),
            (_, false) => said,
            (None, true) => "It was stopped.".into(),
        })
    }
}

/// The first lines of some output, for a notification.
pub fn first_lines(out: &str) -> String {
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    let mut shown = lines.iter().take(NOTIFY_LINES).copied().collect::<Vec<_>>().join("\n");
    if lines.len() > NOTIFY_LINES {
        shown.push_str("\n…");
    }
    shown
}

/// Makes the folder, with an example to copy when it is new, and gives its path.
pub fn ensure_folder(app: &AppHandle) -> Option<PathBuf> {
    let dir = folder(app)?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir).ok()?;
        let (name, body) = example();
        let _ = std::fs::write(dir.join(name), body);
    }
    Some(dir)
}

fn example() -> (&'static str, &'static str) {
    if cfg!(windows) {
        (
            "hello.ps1",
            "# @kairo.title Say hello\r\n# @kairo.description An example: copy it to make your own\r\n# @kairo.argument Name\r\n# @kairo.output notify\r\nparam([string]$name = \"there\")\r\nWrite-Output \"Hello, $name!\"\r\n",
        )
    } else {
        (
            "hello.sh",
            "#!/bin/sh\n# @kairo.title Say hello\n# @kairo.description An example: copy it to make your own\n# @kairo.argument Name\n# @kairo.output notify\necho \"Hello, ${1:-there}!\"\n",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_lines_give_the_title_argument_and_output() {
        let s = parse(Path::new("/s/deploy.sh"), "#!/bin/bash\n# @kairo.title  Deploy site \n# @kairo.description Builds the blog\n# @kairo.argument Branch\n# @kairo.output Copy\necho hi").unwrap();
        assert_eq!(
            s,
            Script { path: "/s/deploy.sh".into(), title: "Deploy site".into(), description: Some("Builds the blog".into()), argument: Some("Branch".into()), output: Output::Copy }
        );
        // Other comment marks, and notify when nothing says.
        let s = parse(Path::new("x.ps1"), "<# @kairo.title Clean up #>\nREM @kairo.description not read twice").unwrap();
        assert_eq!((s.title.as_str(), s.output), ("Clean up", Output::Notify));
        assert!(parse(Path::new("x.js"), "// @kairo.title Tidy\n").is_some());
        assert!(parse(Path::new("x.bat"), ":: @kairo.title Tidy\n").is_some());
    }

    #[test]
    fn a_file_without_a_title_or_with_one_outside_a_comment_isnt_listed() {
        assert!(parse(Path::new("a.sh"), "echo nothing to see").is_none());
        assert!(parse(Path::new("a.py"), "print('@kairo.title not a comment')").is_none());
        assert!(parse(Path::new("a.py"), "# @kairo.titles Nope").is_none());
        assert!(parse(Path::new("a.py"), "# @kairo.title   ").is_none());
    }

    #[test]
    fn the_runner_is_chosen_by_the_ending() {
        let (p, a) = runner(Path::new("/s/x.py"));
        assert_eq!((p.as_str(), a.last().map(String::as_str)), (if cfg!(windows) { "python" } else { "python3" }, Some("/s/x.py")));
        assert_eq!(runner(Path::new("/s/x.js")).0, "node");
        assert_eq!(runner(Path::new("/s/x.bat")), ("cmd".to_string(), vec!["/C".to_string(), "/s/x.bat".to_string()]));
        // Anything else runs as it is.
        assert_eq!(runner(Path::new("/s/tool")), ("/s/tool".to_string(), Vec::new()));
    }

    #[test]
    fn the_first_lines_of_output_are_shown_and_the_rest_cut() {
        assert_eq!(first_lines("a\n\nb\nc\nd\ne\n"), "a\nb\nc\nd\n…");
        assert_eq!(first_lines("only"), "only");
    }

    #[cfg(unix)]
    #[test]
    fn a_script_runs_in_its_folder_with_its_argument_and_failures_say_why() {
        let dir = std::env::temp_dir().join(format!("kairo-scripts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("hi.sh"), "# @kairo.title Hi\necho \"hello $1\"\npwd\n").unwrap();
        std::fs::write(dir.join("bad.sh"), "# @kairo.title Bad\necho broken >&2\nexit 3\n").unwrap();
        std::fs::write(dir.join("notes.txt"), "no title here").unwrap();
        let all = list(&dir);
        assert_eq!(all.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["Bad", "Hi"]);
        let out = run(&all[1], Some("you")).unwrap();
        assert_eq!(out.lines().next(), Some("hello you"));
        assert!(out.lines().nth(1).unwrap().ends_with(dir.file_name().unwrap().to_str().unwrap()));
        assert_eq!(run(&all[0], None), Err("broken".into()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn files_without_a_title_are_named_as_left_out() {
        let dir = std::env::temp_dir().join(format!("kairo-scripts-skipped-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ok.sh"), "# @kairo.title Fine
echo hi").unwrap();
        std::fs::write(dir.join("Notes.txt"), "no title here").unwrap();
        std::fs::write(dir.join(".hidden"), "x").unwrap();
        assert_eq!(skipped(&dir), vec!["Notes.txt".to_string()]);
        assert_eq!(list(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
