//! A record of panics, kept in Kairo's data folder as `kairo-crash.log`. A release build has no
//! console, and a panic on the main thread ends Kairo at once, so without this a crash leaves
//! nothing but the system's own report, which can't say what went wrong.

use std::io::Write;
use std::path::{Path, PathBuf};

pub const FILE: &str = "kairo-crash.log";
/// Past this, the log starts again rather than growing for good.
const MAX_BYTES: u64 = 256 * 1024;

/// Writes every panic to `dir/kairo-crash.log` before it goes on as it would have.
pub fn install(dir: &Path) {
    let path = dir.join(FILE);
    let earlier = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        let backtrace = std::backtrace::Backtrace::force_capture();
        let _ = append(&path, &entry(&thread, &info.to_string(), &backtrace.to_string()));
        earlier(info);
    }));
}

/// One panic as the log has it: when, which thread, what and where, and how it got there.
fn entry(thread: &str, panic: &str, backtrace: &str) -> String {
    let when = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    format!("--- {when} Kairo {} on thread '{thread}'\n{panic}\n{backtrace}\n", env!("CARGO_PKG_VERSION"))
}

fn append(path: &PathBuf, text: &str) -> std::io::Result<()> {
    let start_over = std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES);
    let mut file = std::fs::OpenOptions::new().create(true).append(!start_over).write(true).truncate(start_over).open(path)?;
    file.write_all(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_is_written_down_and_the_log_starts_again_when_big() {
        let dir = std::env::temp_dir().join(format!("kairo-crashlog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE);
        let _ = std::fs::remove_file(&path);
        append(&path, &entry("main", "panicked at src/x.rs:1:1:\nboom", "0: kairo::x")).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("on thread 'main'") && text.contains("boom") && text.contains("kairo::x"), "{text}");
        std::fs::write(&path, vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        append(&path, "fresh\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "fresh\n");
        let _ = std::fs::remove_dir_all(dir);
    }
}
