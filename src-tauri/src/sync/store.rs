//! Where combined computers keep their files: a folder a cloud drive syncs, or Kairo's relay.
//! Each computer has its own files, by name, and reads everyone's.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{err, Result};

/// Every computer's files: computer id → (file name → version). A version changes whenever its
/// file does.
pub type Listing = HashMap<String, HashMap<String, String>>;

pub trait Store {
    fn list(&self) -> Result<Listing>;
    fn get(&self, computer: &str, name: &str) -> Result<Vec<u8>>;
    /// Writes one of this computer's own files.
    fn put(&self, name: &str, bytes: &[u8]) -> Result<()>;
    fn delete(&self, name: &str) -> Result<()>;
    /// Takes all of this computer's files away, so the others drop its time.
    fn remove_me(&self) -> Result<()>;
}

/// `<folder>/Kairo computers/<computer id>/<file>`.
pub struct FolderStore {
    dir: PathBuf,
    me: String,
}

impl FolderStore {
    pub fn new(folder: &Path, me: &str) -> Self {
        Self { dir: folder.join(super::DIR), me: me.to_string() }
    }

    fn mine(&self) -> PathBuf {
        self.dir.join(&self.me)
    }
}

impl Store for FolderStore {
    fn list(&self) -> Result<Listing> {
        let mut out = Listing::new();
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return Ok(out) };
        for entry in entries.flatten() {
            let Some(computer) = entry.file_name().to_str().map(str::to_string).filter(|_| entry.path().is_dir()) else { continue };
            let files = std::fs::read_dir(entry.path())
                .map_err(err)?
                .flatten()
                .filter_map(|f| {
                    let name = f.file_name().to_str()?.to_string();
                    if name.ends_with(".tmp") {
                        return None;
                    }
                    let meta = f.metadata().ok()?;
                    let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis();
                    Some((name, format!("{}:{modified}", meta.len())))
                })
                .collect();
            out.insert(computer, files);
        }
        Ok(out)
    }

    fn get(&self, computer: &str, name: &str) -> Result<Vec<u8>> {
        std::fs::read(self.dir.join(computer).join(name)).map_err(err)
    }

    /// Writes next to the file and then moves it into place, so a drive syncing the folder never
    /// takes a half-written file.
    fn put(&self, name: &str, bytes: &[u8]) -> Result<()> {
        let mine = self.mine();
        std::fs::create_dir_all(&mine).map_err(err)?;
        let path = mine.join(name);
        let tmp = path.with_extension("tmp");
        let _ = std::fs::remove_file(&tmp);
        if let Err(e) = std::fs::write(&tmp, bytes) {
            let _ = std::fs::remove_file(&tmp);
            return Err(err(e));
        }
        std::fs::rename(&tmp, path).map_err(err)
    }

    fn delete(&self, name: &str) -> Result<()> {
        match std::fs::remove_file(self.mine().join(name)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(err(e)),
            _ => Ok(()),
        }
    }

    fn remove_me(&self) -> Result<()> {
        match std::fs::remove_dir_all(self.mine()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(err(e)),
            _ => Ok(()),
        }
    }
}

/// (computer, name) → (bytes, version).
#[cfg(test)]
type MemFiles = HashMap<(String, String), (Vec<u8>, u64)>;

/// A store in memory, shared between the computers of a test.
#[cfg(test)]
#[derive(Clone, Default)]
pub struct MemStore {
    files: std::sync::Arc<std::sync::Mutex<MemFiles>>,
    count: std::sync::Arc<std::sync::atomic::AtomicU64>,
    me: String,
}

#[cfg(test)]
impl MemStore {
    pub fn for_computer(&self, me: &str) -> Self {
        Self { me: me.to_string(), ..self.clone() }
    }
}

#[cfg(test)]
impl Store for MemStore {
    fn list(&self) -> Result<Listing> {
        let mut out = Listing::new();
        for ((computer, name), (_, version)) in self.files.lock().unwrap().iter() {
            out.entry(computer.clone()).or_default().insert(name.clone(), version.to_string());
        }
        Ok(out)
    }

    fn get(&self, computer: &str, name: &str) -> Result<Vec<u8>> {
        self.files.lock().unwrap().get(&(computer.to_string(), name.to_string())).map(|(b, _)| b.clone()).ok_or_else(|| "no such file".into())
    }

    fn put(&self, name: &str, bytes: &[u8]) -> Result<()> {
        let version = self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.files.lock().unwrap().insert((self.me.clone(), name.to_string()), (bytes.to_vec(), version));
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<()> {
        self.files.lock().unwrap().remove(&(self.me.clone(), name.to_string()));
        Ok(())
    }

    fn remove_me(&self) -> Result<()> {
        self.files.lock().unwrap().retain(|(computer, _), _| computer != &self.me);
        Ok(())
    }
}
