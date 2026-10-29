//! Runs a search command and reads its lines until it ends, has given enough, or time is up.

use std::io::{self, BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use super::Unavailable;

/// Reads up to `max` non-empty lines, stopping at the deadline or the end of the output. A line
/// that isn't UTF-8 is skipped.
fn read_lines(out: impl BufRead, deadline: Instant, max: usize) -> Vec<String> {
    let mut found = Vec::new();
    for line in out.split(b'\n') {
        if found.len() >= max || Instant::now() >= deadline {
            break;
        }
        let Ok(bytes) = line else { break };
        if let Ok(mut line) = String::from_utf8(bytes) {
            if line.ends_with('\r') {
                line.pop();
            }
            if !line.is_empty() {
                found.push(line);
            }
        }
    }
    found
}

/// A reader fed by a helper thread's chunks of output, which reports the end of the output at the
/// deadline instead of blocking while the child is quiet. `ended` tells a real end from the deadline.
struct UntilDeadline {
    chunks: Receiver<Vec<u8>>,
    deadline: Instant,
    pending: Vec<u8>,
    ended: Arc<AtomicBool>,
}

impl Read for UntilDeadline {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pending.is_empty() {
            let wait = self.deadline.saturating_duration_since(Instant::now());
            match self.chunks.recv_timeout(wait) {
                Ok(chunk) => self.pending = chunk,
                Err(RecvTimeoutError::Timeout) => return Ok(0),
                Err(RecvTimeoutError::Disconnected) => {
                    self.ended.store(true, Ordering::SeqCst);
                    return Ok(0);
                }
            }
        }
        let n = self.pending.len().min(buf.len());
        buf[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        Ok(n)
    }
}

/// Runs the command and returns up to `max` non-empty lines of its output, until the deadline.
/// The command is killed unless its output ended by itself, and always waited for. A missing
/// program is `NoIndexer`; an exit code outside `ok_codes` is `Failed`, but only when it finished
/// on its own.
pub fn lines(mut cmd: Command, deadline: Instant, max: usize, ok_codes: &[i32]) -> Result<Vec<String>, Unavailable> {
    let name = cmd.get_program().to_string_lossy().into_owned();
    let mut child = cmd
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
    let ended = Arc::new(AtomicBool::new(false));
    let found = read_lines(BufReader::new(UntilDeadline { chunks, deadline, pending: Vec::new(), ended: ended.clone() }), deadline, max);
    let finished = ended.load(Ordering::SeqCst) && found.len() < max;
    if !finished {
        let _ = child.kill();
    }
    // Always reaped, so searches leave no zombies behind.
    let status = child.wait().map_err(|e| Unavailable::Failed(e.to_string()))?;
    if finished && !status.code().is_some_and(|c| ok_codes.contains(&c)) {
        return Err(Unavailable::Failed(format!("{name} {status}")));
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
    fn three_lines_are_three_and_an_empty_one_is_skipped() {
        let sample = "/Users/ana/a.txt\n\n/Users/ana/Documents/b.pdf\n/Users/ana/c.md\n";
        assert_eq!(read_lines(sample.as_bytes(), later(), 50), vec!["/Users/ana/a.txt", "/Users/ana/Documents/b.pdf", "/Users/ana/c.md"]);
    }

    #[test]
    fn reading_stops_at_the_most_asked_for() {
        assert_eq!(read_lines("/a\n/b\n/c\n".as_bytes(), later(), 2), vec!["/a", "/b"]);
    }

    #[test]
    fn a_line_that_is_not_utf8_is_skipped() {
        assert_eq!(read_lines(&b"/a\n/\xff\xfe\n/c\n"[..], later(), 50), vec!["/a", "/c"]);
    }

    #[test]
    fn a_missing_program_is_no_indexer() {
        let r = lines(Command::new("kairo-no-such-program"), later(), 5, &[0]);
        assert!(matches!(r, Err(Unavailable::NoIndexer)));
    }

    #[test]
    fn a_failing_command_is_failed() {
        let mut c = Command::new("sh");
        c.args(["-c", "echo /a; exit 3"]);
        assert!(matches!(lines(c, later(), 5, &[0]), Err(Unavailable::Failed(_))));
    }

    #[test]
    fn an_exit_code_that_means_no_matches_is_an_empty_answer() {
        let exit1 = || {
            let mut c = Command::new("sh");
            c.args(["-c", "exit 1"]);
            c
        };
        assert_eq!(lines(exit1(), later(), 5, &[0, 1]).unwrap(), Vec::<String>::new());
        assert!(matches!(lines(exit1(), later(), 5, &[0]), Err(Unavailable::Failed(_))));
    }

    #[test]
    fn a_quiet_command_is_killed_at_the_deadline() {
        let mut c = Command::new("sh");
        c.args(["-c", "echo /a; sleep 30"]);
        let start = Instant::now();
        let r = lines(c, Instant::now() + Duration::from_millis(300), 5, &[0]).unwrap();
        assert_eq!(r, vec!["/a"]);
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}
