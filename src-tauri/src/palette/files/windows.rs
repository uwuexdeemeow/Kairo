//! Windows file search: asks the Windows Search index through a `search-ms:` query, listed with the
//! shell's own item enumeration.

use std::path::{Path, PathBuf};
use std::time::Instant;

use windows::core::{HSTRING, PWSTR};
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED};
use windows::Win32::UI::Shell::{IEnumShellItems, IShellItem, SHCreateItemFromParsingName, BHID_EnumItems, SIGDN_FILESYSPATH};

use super::Unavailable;

/// The most paths one search lists.
const MAX_PATHS: usize = 50;

/// Percent-encodes everything but ASCII letters and digits, as a `search-ms:` URL wants.
fn escape(text: &str) -> String {
    text.bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// The `search-ms:` URL asking for files named like the query anywhere under the folder.
fn search_url(query: &str, home: &Path) -> String {
    format!("search-ms:query={}&crumb=location:{}", escape(query), escape(&home.to_string_lossy()))
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

/// Asks Windows Search for files named like the query under `home`, up to 50 or until the deadline.
pub fn ask(query: &str, home: &Path, deadline: Instant) -> Result<Vec<PathBuf>, Unavailable> {
    let failed = |e: windows::core::Error| Unavailable::Failed(e.to_string());
    // Initialised on this thread, whichever it is; a thread already set up the other way is left alone.
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if hr.is_err() && hr != RPC_E_CHANGED_MODE {
        return Err(Unavailable::Failed(windows::core::Error::from(hr).to_string()));
    }
    let _com = Com(hr.is_ok());
    unsafe {
        let folder: IShellItem = SHCreateItemFromParsingName(&HSTRING::from(search_url(query, home)), None).map_err(failed)?;
        let items: IEnumShellItems = folder.BindToHandler(None, &BHID_EnumItems).map_err(failed)?;
        let mut found = Vec::new();
        while found.len() < MAX_PATHS && Instant::now() < deadline {
            let mut next = [None];
            let mut fetched = 0;
            items.Next(&mut next, Some(&mut fetched)).map_err(failed)?;
            let Some(Some(item)) = next.into_iter().next().filter(|_| fetched > 0) else { break };
            let name: PWSTR = item.GetDisplayName(SIGDN_FILESYSPATH).map_err(failed)?;
            found.push(PathBuf::from(name.to_string().map_err(|e| Unavailable::Failed(e.to_string()))?));
            CoTaskMemFree(Some(name.0 as *const _));
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_query_and_folder_are_escaped() {
        assert_eq!(
            search_url("tax & \"2026\"", Path::new(r"C:\Users\Ana")),
            "search-ms:query=tax%20%26%20%222026%22&crumb=location:C%3A%5CUsers%5CAna"
        );
    }
}
