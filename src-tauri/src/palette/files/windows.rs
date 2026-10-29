//! Windows file search: asks the Windows Search index through the Storage API's file query, which
//! answers straight from the index. (A `search-ms:` folder enumerated through the shell fails with
//! E_UNEXPECTED, so it can't be used here.)

use std::path::{Path, PathBuf};
use std::time::Instant;

use windows::core::HSTRING;
use windows::Storage::Search::{FolderDepth, IndexerOption, QueryOptions};
use windows::Storage::StorageFolder;
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

use super::Unavailable;

/// The most paths one search lists.
const MAX_PATHS: u32 = 50;

/// The Advanced Query Syntax filter for file names containing the query. AQS has no way to escape
/// a double quote inside a phrase, so those are dropped.
fn name_filter(query: &str) -> String {
    format!("System.FileName:~~\"{}\"", query.replace('"', ""))
}

/// Balances a successful `CoInitializeEx` when dropped.
struct Com(bool);

impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

/// Asks Windows Search for files named like the query under `home`, up to 50. The index answers in
/// one call, so the deadline is left to the caller, which stops waiting for a late answer.
pub fn ask(query: &str, home: &Path, _deadline: Instant) -> Result<Vec<PathBuf>, Unavailable> {
    let failed = |e: windows::core::Error| Unavailable::Failed(e.to_string());
    // Initialised on this thread, whichever it is; a thread already set up the other way is left alone.
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if hr.is_err() && hr != RPC_E_CHANGED_MODE {
        return Err(Unavailable::Failed(windows::core::Error::from(hr).to_string()));
    }
    let _com = Com(hr.is_ok());
    let folder = StorageFolder::GetFolderFromPathAsync(&HSTRING::from(home.as_os_str())).and_then(|op| op.join()).map_err(failed)?;
    let options = QueryOptions::new().map_err(failed)?;
    options.SetFolderDepth(FolderDepth::Deep).map_err(failed)?;
    // Only the index: without it, a folder outside the index would be walked file by file.
    options.SetIndexerOption(IndexerOption::OnlyUseIndexer).map_err(failed)?;
    options.SetApplicationSearchFilter(&HSTRING::from(name_filter(query))).map_err(failed)?;
    let files = folder
        .CreateFileQueryWithOptions(&options)
        .and_then(|q| q.GetFilesAsync(0, MAX_PATHS))
        .and_then(|op| op.join())
        .map_err(failed)?;
    let mut found = Vec::new();
    for file in files {
        found.push(PathBuf::from(file.Path().map_err(failed)?.to_os_string()));
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filter_matches_names_containing_the_query() {
        assert_eq!(name_filter("tax 2026"), "System.FileName:~~\"tax 2026\"");
        assert_eq!(name_filter("say \"hi\""), "System.FileName:~~\"say hi\"");
    }

    /// Asks the real index for a file made just for this, so a search that can't answer fails here
    /// rather than on someone's machine. Run it with `cargo test -- --ignored` on a machine whose
    /// Windows Search indexes the user folder, as a normal Windows install does.
    #[test]
    #[ignore = "needs the Windows Search service and time for it to index"]
    fn the_index_finds_a_file_under_home() {
        let home = super::super::home().expect("a home folder");
        let name = format!("kairo-search-probe-{}.txt", std::process::id());
        let path = home.join("Documents").join(&name);
        std::fs::write(&path, "probe").unwrap();
        let mut found = Vec::new();
        for _ in 0..60 {
            found = ask(&name, &home, Instant::now()).unwrap();
            if !found.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(2));
        }
        std::fs::remove_file(&path).ok();
        assert!(found.iter().any(|p| p.file_name().is_some_and(|n| n == name.as_str())), "found {found:?}");
    }
}
