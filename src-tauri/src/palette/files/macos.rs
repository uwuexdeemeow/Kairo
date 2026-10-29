//! macOS file search: asks Spotlight through `mdfind`.

use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Instant;

use super::Unavailable;

/// The most paths one search lists.
const MAX_PATHS: usize = 50;

/// Reads up to `max` non-empty lines as paths, stopping at the deadline or the end of the output.
fn read_paths(out: impl BufRead, deadline: Instant, max: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for line in out.lines() {
        if found.len() >= max || Instant::now() >= deadline {
            break;
        }
        let Ok(line) = line else { break };
        if !line.is_empty() {
            found.push(PathBuf::from(line));
        }
    }
    found
}

/// A reader fed by a helper thread's chunks of output, which reports the end of the output at the
/// deadline instead of blocking while the child is quiet.
struct UntilDeadline {
    chunks: Receiver<Vec<u8>>,
    deadline: Instant,
    pending: Vec<u8>,
}

impl Read for UntilDeadline {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pending.is_empty() {
            let wait = self.deadline.saturating_duration_since(Instant::now());
            match self.chunks.recv_timeout(wait) {
                Ok(chunk) => self.pending = chunk,
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        let n = self.pending.len().min(buf.len());
        buf[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        Ok(n)
    }
}

/// Asks Spotlight for files named like the query under `home`, up to 50 or until the deadline.
pub fn ask(query: &str, home: &Path, deadline: Instant) -> Result<Vec<PathBuf>, Unavailable> {
    let mut child = Command::new("mdfind")
        .arg("-onlyin")
        .arg(home)
        .arg("-name")
        .arg(query)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| if e.kind() == io::ErrorKind::NotFound { Unavailable::NoIndexer } else { Unavailable::Failed(e.to_string()) })?;
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let (send, chunks) = mpsc::channel();
    // Ends by itself: the read fails or finds the end once the child is killed or done.
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 || send.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let found = read_paths(BufReader::new(UntilDeadline { chunks, deadline, pending: Vec::new() }), deadline, MAX_PATHS);
    let finished = found.len() < MAX_PATHS && Instant::now() < deadline;
    if !finished {
        let _ = child.kill();
    }
    // Always reaped, so searches leave no zombies behind.
    let status = child.wait().map_err(|e| Unavailable::Failed(e.to_string()))?;
    if finished && !status.success() {
        return Err(Unavailable::Failed(format!("mdfind {status}")));
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn later() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }

    #[test]
    fn three_lines_are_three_paths_and_an_empty_one_is_skipped() {
        let sample = "/Users/ana/a.txt\n\n/Users/ana/Documents/b.pdf\n/Users/ana/c.md\n";
        let paths = read_paths(sample.as_bytes(), later(), 50);
        assert_eq!(paths, vec![PathBuf::from("/Users/ana/a.txt"), PathBuf::from("/Users/ana/Documents/b.pdf"), PathBuf::from("/Users/ana/c.md")]);
    }

    #[test]
    fn reading_stops_at_the_most_asked_for() {
        let paths = read_paths("/a\n/b\n/c\n".as_bytes(), later(), 2);
        assert_eq!(paths, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
    }

    /// Deletes its file when dropped, even if the test failed.
    struct Cleanup(PathBuf);

    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    #[ignore = "needs Spotlight to index a new file"]
    fn spotlight_finds_a_new_file() {
        let home = PathBuf::from(std::env::var("HOME").unwrap());
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let word = format!("kairospotlight{}x{nanos}", std::process::id());
        let file = home.join("Documents").join(format!("{word}.txt"));
        let _cleanup = Cleanup(file.clone());
        std::fs::write(&file, "x").unwrap();
        let start = Instant::now();
        let mut found = false;
        while start.elapsed() < Duration::from_secs(10) && !found {
            found = ask(&word, &home, Instant::now() + Duration::from_millis(1500)).is_ok_and(|p| p.contains(&file));
            if !found {
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        assert!(found, "Spotlight never listed {}", file.display());
    }
}
