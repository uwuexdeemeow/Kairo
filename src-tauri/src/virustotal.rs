//! Checking files, links and hashes against VirusTotal with the user's own free key. A file is
//! looked up by its SHA-256, worked out here; it's uploaded only when VirusTotal hasn't seen it
//! and the user says so. See docs/superpowers/specs/2026-11-04-virustotal-design.md.

use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use base64::Engine;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tauri::State;

use crate::AppState;

const KEY_SETTING: &str = "virustotal_key";
const API: &str = "https://www.virustotal.com/api/v3";
/// The most a free key may upload.
pub const UPLOAD_MAX: u64 = 32 * 1024 * 1024;
/// The EICAR test file's hash: known to VirusTotal, harmless, and enough to try a key on.
const EICAR_SHA256: &str = "275a021bbfb6489e54d471899f7db9d1663fc695ec2fe2a2c4538aabf651fd0f";
const LIMIT: &str = "VirusTotal's free limit: try again in a minute.";
const REFUSED: &str = "VirusTotal didn't accept the key. Check it in Kairo's settings.";
const NO_KEY: &str = "Add your VirusTotal key in Kairo's settings.";

// ---------- what was typed ----------

#[derive(Debug, PartialEq)]
pub enum Query {
    /// MD5, SHA-1 or SHA-256, lower case.
    Hash(String),
    /// A link, with https:// added when it was typed without.
    Url(String),
}

/// A link, a hash, or neither.
pub fn classify(text: &str) -> Option<Query> {
    let t = text.trim();
    if matches!(t.len(), 32 | 40 | 64) && t.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(Query::Hash(t.to_ascii_lowercase()));
    }
    if t.contains(char::is_whitespace) || t.is_empty() {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Some(Query::Url(t.to_string()));
    }
    // A bare host ("example.com/page"): letters for its last part, as a domain has.
    let host = t.split(['/', '?', '#']).next().unwrap_or("");
    let last = host.rsplit('.').next().unwrap_or("");
    (host.contains('.') && last.len() >= 2 && last.chars().all(|c| c.is_ascii_alphabetic())).then(|| Query::Url(format!("https://{t}")))
}

/// VirusTotal's id for a link: the link itself, base64url without padding.
pub fn url_id(url: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(url)
}

/// A file's SHA-256 and size, read in pieces so a large file isn't held in memory.
pub fn sha256_file(path: &Path) -> std::io::Result<(String, u64)> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        hasher.update(&buf[..n]);
    }
    Ok((format!("{:x}", hasher.finalize()), size))
}

// ---------- what came back ----------

#[derive(Serialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub malicious: u32,
    pub suspicious: u32,
    /// The engines that gave an answer: flagged, harmless or undetected.
    pub total: u32,
    /// Up to three detections: (engine, what it called it), malicious first.
    pub names: Vec<(String, String)>,
    /// Detections beyond those three.
    pub more: u32,
    /// When it was last scanned, in milliseconds.
    pub last_ms: Option<i64>,
    /// The full report on virustotal.com.
    pub link: String,
    /// The file's best-known name, or the link.
    pub title: Option<String>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Answer {
    Verdict(Verdict),
    /// A file VirusTotal hasn't seen: uploading is up to the user.
    #[serde(rename_all = "camelCase")]
    NotSeen { sha256: String, size: u64, too_big: bool },
    /// Being scanned; asked again with `then` (`file:<sha256>` or `url:<id>`) once done.
    #[serde(rename_all = "camelCase")]
    Scanning { analysis: String, then: String },
}

/// The verdict in a file or link object; None while it has never been scanned.
pub fn verdict_from(json: &Value) -> Option<Verdict> {
    let data = &json["data"];
    let attrs = &data["attributes"];
    let stats = &attrs["last_analysis_stats"];
    let count = |k: &str| stats[k].as_u64().unwrap_or(0) as u32;
    let (malicious, suspicious) = (count("malicious"), count("suspicious"));
    let total = malicious + suspicious + count("harmless") + count("undetected");
    if total == 0 {
        return None;
    }
    let mut found: Vec<(u8, String, String)> = attrs["last_analysis_results"]
        .as_object()
        .map(|results| {
            results
                .iter()
                .filter_map(|(engine, r)| {
                    let rank = match r["category"].as_str()? {
                        "malicious" => 0,
                        "suspicious" => 1,
                        _ => return None,
                    };
                    let name = r["result"].as_str().filter(|s| !s.is_empty()).unwrap_or(if rank == 0 { "malicious" } else { "suspicious" });
                    Some((rank, engine.clone(), name.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    let more = found.len().saturating_sub(3) as u32;
    let kind = if data["type"].as_str() == Some("url") { "url" } else { "file" };
    Some(Verdict {
        malicious,
        suspicious,
        total,
        names: found.into_iter().take(3).map(|(_, engine, name)| (engine, name)).collect(),
        more,
        last_ms: attrs["last_analysis_date"].as_i64().map(|s| s * 1000),
        link: format!("https://www.virustotal.com/gui/{kind}/{}", data["id"].as_str().unwrap_or_default()),
        title: attrs[if kind == "url" { "url" } else { "meaningful_name" }].as_str().map(str::to_string),
    })
}

/// What a status other than success means, in words; None for "not found".
pub fn error_for(status: u16) -> Option<String> {
    match status {
        404 => None,
        401 | 403 => Some(REFUSED.into()),
        429 => Some(LIMIT.into()),
        s => Some(format!("VirusTotal answered {s}. Try again in a moment.")),
    }
}

/// A file upload as multipart form data, built by hand rather than with another dependency.
pub fn multipart(boundary: &str, name: &str, data: &[u8]) -> Vec<u8> {
    let safe: String = name.chars().map(|c| if c == '"' || c.is_control() { '_' } else { c }).collect();
    let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{safe}\"\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

/// A link as a form field's value.
fn form_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

// ---------- asking ----------

fn client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            // rustls needs its cryptography chosen once per process, as the relay and updater do.
            if rustls::crypto::CryptoProvider::get_default().is_none() {
                let _ = rustls::crypto::ring::default_provider().install_default();
            }
            reqwest::Client::builder().timeout(Duration::from_secs(60)).build().ok()
        })
        .as_ref()
        .ok_or_else(|| "Kairo couldn't make a secure connection.".to_string())
}

/// One request; Ok(None) for "not found".
async fn call(key: &str, request: reqwest::RequestBuilder) -> Result<Option<Value>, String> {
    let response = request.header("x-apikey", key).send().await.map_err(|_| "Couldn't reach VirusTotal. Check your connection.".to_string())?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return match error_for(status) {
            None => Ok(None),
            Some(why) => Err(why),
        };
    }
    response.json().await.map(Some).map_err(|_| "VirusTotal's answer couldn't be read.".to_string())
}

async fn get(key: &str, path: &str) -> Result<Option<Value>, String> {
    call(key, client()?.get(format!("{API}{path}"))).await
}

fn key_of(state: &State<'_, AppState>) -> Result<String, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    crate::db::get_setting(&conn, KEY_SETTING).ok().flatten().filter(|k| !k.trim().is_empty()).ok_or_else(|| NO_KEY.to_string())
}

/// A file or link looked up: its verdict, or it's still being scanned.
async fn verdict_or_scanning(key: &str, path: &str, analysis: &str, then: &str) -> Result<Answer, String> {
    match get(key, path).await?.as_ref().and_then(verdict_from) {
        Some(v) => Ok(Answer::Verdict(v)),
        None => Ok(Answer::Scanning { analysis: analysis.into(), then: then.into() }),
    }
}

// ---------- commands ----------

/// Whether a key is saved; the key itself never goes back to a page.
#[tauri::command]
pub fn vt_has_key(state: State<'_, AppState>) -> bool {
    key_of(&state).is_ok()
}

/// Saves the key; an empty one takes it away.
#[tauri::command]
pub fn set_vt_key(state: State<'_, AppState>, key: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    crate::db::set_setting(&conn, KEY_SETTING, key.trim()).map_err(|e| e.to_string())
}

/// Tries the saved key on a hash VirusTotal knows.
#[tauri::command]
pub async fn test_vt_key(state: State<'_, AppState>) -> Result<(), String> {
    let key = key_of(&state)?;
    get(&key, &format!("/files/{EICAR_SHA256}")).await.map(|_| ())
}

/// A file, by its hash worked out here: only the hash is sent.
#[tauri::command]
pub async fn vt_check_file(state: State<'_, AppState>, path: String) -> Result<Answer, String> {
    let key = key_of(&state)?;
    let (sha256, size) = tauri::async_runtime::spawn_blocking(move || sha256_file(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("Couldn't read the file: {e}"))?;
    match get(&key, &format!("/files/{sha256}")).await? {
        Some(json) => Ok(verdict_from(&json).map(Answer::Verdict).unwrap_or(Answer::NotSeen { sha256, size, too_big: size > UPLOAD_MAX })),
        None => Ok(Answer::NotSeen { sha256, size, too_big: size > UPLOAD_MAX }),
    }
}

/// A link or a hash, as typed. A link VirusTotal hasn't seen is sent for a scan: it's public
/// already.
#[tauri::command]
pub async fn vt_check(state: State<'_, AppState>, text: String) -> Result<Answer, String> {
    let key = key_of(&state)?;
    match classify(&text).ok_or_else(|| "That's not a link or a hash.".to_string())? {
        Query::Hash(hash) => match get(&key, &format!("/files/{hash}")).await? {
            Some(json) => verdict_from(&json).map(Answer::Verdict).ok_or_else(|| "VirusTotal knows this hash but hasn't scanned it yet.".into()),
            None => Err("VirusTotal hasn't seen anything with this hash.".into()),
        },
        Query::Url(url) => {
            let id = url_id(&url);
            if let Some(v) = get(&key, &format!("/urls/{id}")).await?.as_ref().and_then(verdict_from) {
                return Ok(Answer::Verdict(v));
            }
            let request = client()?.post(format!("{API}/urls")).header("content-type", "application/x-www-form-urlencoded").body(format!("url={}", form_encode(&url)));
            let sent = call(&key, request).await?.ok_or("VirusTotal didn't take the link.")?;
            let analysis = sent["data"]["id"].as_str().ok_or("VirusTotal didn't say how to follow the scan.")?;
            Ok(Answer::Scanning { analysis: analysis.into(), then: format!("url:{id}") })
        }
    }
}

/// Uploads a file VirusTotal hasn't seen, once the user has said so.
#[tauri::command]
pub async fn vt_upload(state: State<'_, AppState>, path: String) -> Result<Answer, String> {
    let key = key_of(&state)?;
    let file = path.clone();
    let (data, sha256) = tauri::async_runtime::spawn_blocking(move || -> Result<(Vec<u8>, String), String> {
        let p = Path::new(&file);
        let size = std::fs::metadata(p).map_err(|e| e.to_string())?.len();
        if size > UPLOAD_MAX {
            return Err("Too big to upload with a free key.".into());
        }
        let (sha256, _) = sha256_file(p).map_err(|e| e.to_string())?;
        Ok((std::fs::read(p).map_err(|e| e.to_string())?, sha256))
    })
    .await
    .map_err(|e| e.to_string())??;
    let name = Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    let boundary = format!("kairo{:x}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let request = client()?.post(format!("{API}/files")).header("content-type", format!("multipart/form-data; boundary={boundary}")).body(multipart(&boundary, &name, &data));
    let sent = call(&key, request).await?.ok_or("VirusTotal didn't take the file.")?;
    let analysis = sent["data"]["id"].as_str().ok_or("VirusTotal didn't say how to follow the scan.")?;
    Ok(Answer::Scanning { analysis: analysis.into(), then: format!("file:{sha256}") })
}

/// A scan followed up: still going, or its verdict.
#[tauri::command]
pub async fn vt_follow(state: State<'_, AppState>, analysis: String, then: String) -> Result<Answer, String> {
    let key = key_of(&state)?;
    let status = get(&key, &format!("/analyses/{analysis}")).await?;
    let done = status.as_ref().and_then(|s| s["data"]["attributes"]["status"].as_str()) == Some("completed");
    if !done {
        return Ok(Answer::Scanning { analysis, then });
    }
    let path = match then.split_once(':') {
        Some(("file", sha)) => format!("/files/{sha}"),
        Some(("url", id)) => format!("/urls/{id}"),
        _ => return Err("Kairo lost track of this scan.".into()),
    };
    verdict_or_scanning(&key, &path, &analysis, &then).await
}

/// Opens a full report in the browser: only VirusTotal's own pages.
#[tauri::command]
pub fn vt_open_report(link: String) -> Result<(), String> {
    if !link.starts_with("https://www.virustotal.com/gui/") {
        return Err("Not a VirusTotal report.".into());
    }
    if crate::platform::open_path(&link) {
        Ok(())
    } else {
        Err("Couldn't open your browser.".into())
    }
}

/// The system's file picker, for Choose file…; None when closed.
#[tauri::command]
pub fn vt_pick_file(app: tauri::AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog().file().set_title("Scan with VirusTotal").blocking_pick_file()?.into_path().ok().map(|p| p.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn links_and_hashes_are_told_apart() {
        assert_eq!(classify(" 275A021BBFB6489E54D471899F7DB9D1663FC695EC2FE2A2C4538AABF651FD0F "), Some(Query::Hash(EICAR_SHA256.into())));
        assert_eq!(classify("44d88612fea8a8f36de82e1278abb02f"), Some(Query::Hash("44d88612fea8a8f36de82e1278abb02f".into())));
        assert_eq!(classify("https://example.com/a?b=1"), Some(Query::Url("https://example.com/a?b=1".into())));
        assert_eq!(classify("example.com/page"), Some(Query::Url("https://example.com/page".into())));
        assert_eq!(classify("hello world"), None);
        assert_eq!(classify("notes.txt1"), None);
        assert_eq!(classify("12345"), None);
        assert_eq!(classify(""), None);
    }

    #[test]
    fn a_links_id_is_base64url_without_padding() {
        assert_eq!(url_id("https://example.com"), "aHR0cHM6Ly9leGFtcGxlLmNvbQ");
    }

    #[test]
    fn a_files_hash_is_read_in_pieces() {
        let dir = std::env::temp_dir().join(format!("kairo-vt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("big.bin");
        // Larger than one piece, so the pieces are joined right.
        std::fs::write(&file, vec![b'k'; 200_000]).unwrap();
        let (sha, size) = sha256_file(&file).unwrap();
        assert_eq!(size, 200_000);
        assert_eq!(sha, format!("{:x}", Sha256::digest(vec![b'k'; 200_000])));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_verdict_counts_and_names_the_worst_first() {
        let answer = json!({ "data": { "type": "file", "id": "abc", "attributes": {
            "meaningful_name": "setup.exe",
            "last_analysis_date": 1_762_000_000,
            "last_analysis_stats": { "malicious": 3, "suspicious": 1, "harmless": 2, "undetected": 65, "timeout": 4 },
            "last_analysis_results": {
                "Zeta": { "category": "suspicious", "result": "Heur.Odd" },
                "ESET": { "category": "malicious", "result": "Win32.Agent" },
                "Kaspersky": { "category": "malicious", "result": "Trojan.Generic" },
                "Avast": { "category": "malicious", "result": null },
                "Other": { "category": "undetected", "result": null }
            }
        }}});
        let v = verdict_from(&answer).unwrap();
        assert_eq!((v.malicious, v.suspicious, v.total, v.more), (3, 1, 71, 1));
        assert_eq!(v.names, [("Avast".into(), "malicious".into()), ("ESET".into(), "Win32.Agent".into()), ("Kaspersky".into(), "Trojan.Generic".into())]);
        assert_eq!(v.last_ms, Some(1_762_000_000_000));
        assert_eq!(v.link, "https://www.virustotal.com/gui/file/abc");
        assert_eq!(v.title.as_deref(), Some("setup.exe"));
    }

    #[test]
    fn a_link_never_scanned_has_no_verdict_yet() {
        let fresh = json!({ "data": { "type": "url", "id": "u1", "attributes": { "url": "https://x.example", "last_analysis_stats": {} } } });
        assert_eq!(verdict_from(&fresh), None);
        let clean = json!({ "data": { "type": "url", "id": "u1", "attributes": { "url": "https://x.example", "last_analysis_stats": { "harmless": 70, "undetected": 24 } } } });
        let v = verdict_from(&clean).unwrap();
        assert_eq!((v.malicious, v.total, v.link.as_str(), v.title.as_deref()), (0, 94, "https://www.virustotal.com/gui/url/u1", Some("https://x.example")));
    }

    #[test]
    fn errors_say_what_went_wrong() {
        assert_eq!(error_for(404), None);
        assert_eq!(error_for(401).as_deref(), Some(REFUSED));
        assert_eq!(error_for(429).as_deref(), Some(LIMIT));
        assert!(error_for(500).unwrap().contains("500"));
    }

    #[test]
    fn an_upload_is_one_file_part() {
        let body = String::from_utf8(multipart("B", "a\"b.exe", b"MZ")).unwrap();
        assert_eq!(body, "--B\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a_b.exe\"\r\nContent-Type: application/octet-stream\r\n\r\nMZ\r\n--B--\r\n");
        assert_eq!(form_encode("https://a.b/c?d=e f"), "https%3A%2F%2Fa.b%2Fc%3Fd%3De%20f");
    }
}
