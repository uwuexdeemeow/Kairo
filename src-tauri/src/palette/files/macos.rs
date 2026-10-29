//! macOS file search: asks Spotlight through `mdfind`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use super::{process, Unavailable};

/// The most paths one search lists.
const MAX_PATHS: usize = 50;

/// Asks Spotlight for files named like the query under `home`, up to 50 or until the deadline.
pub fn ask(query: &str, home: &Path, deadline: Instant) -> Result<Vec<PathBuf>, Unavailable> {
    let mut cmd = Command::new("mdfind");
    cmd.arg("-onlyin").arg(home).arg("-name").arg(query);
    Ok(process::lines(cmd, deadline, MAX_PATHS)?.into_iter().map(PathBuf::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
