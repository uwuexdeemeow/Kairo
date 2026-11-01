//! Combining through Kairo's relay: the same files a synced folder would hold, sealed on this
//! computer (`seal.rs`) and kept by a small Cloudflare Worker (`relay/`) under names it can't
//! read. The relay is only ever reached from the sync and pairing threads, never while the app's
//! database is locked.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use rusqlite::Connection;

use super::seal::{self, Keys};
use super::store::{Listing, Store};
use super::{err, Result};
use crate::db;

/// Where Kairo's relay answers. `KAIRO_RELAY` in the environment overrides it, for local runs.
pub const RELAY_URL: &str = "https://kairo-relay.uwuexdeemeow.workers.dev";
const TIMEOUT: Duration = Duration::from_secs(20);
/// What a store error says when the relay doesn't know the group (or its token) any more.
pub const GONE: &str = "Kairo's relay doesn't know this group any more.";

const GROUP_KEY: &str = "relay_group";
const KEY_KEY: &str = "relay_key";
const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;
const B64URL: base64::engine::GeneralPurpose = base64::engine::general_purpose::URL_SAFE_NO_PAD;

#[derive(Debug, Clone, PartialEq)]
pub enum ApiError {
    /// No answer: offline, or the relay is down.
    Unreachable(String),
    /// The group, or the slot, isn't there, or the token doesn't match.
    Gone,
    Other(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::Unreachable(_) => write!(f, "Can't reach Kairo's relay right now."),
            ApiError::Gone => write!(f, "{GONE}"),
            ApiError::Other(e) => write!(f, "Kairo's relay said: {e}"),
        }
    }
}

impl From<ApiError> for String {
    fn from(e: ApiError) -> String {
        e.to_string()
    }
}

/// What the relay offers for a group's files.
pub trait Api: Send + Sync {
    fn create_group(&self, group: &str, token_hash: &str) -> std::result::Result<(), ApiError>;
    fn delete_group(&self, group: &str, token: &str) -> std::result::Result<(), ApiError>;
    /// Every file: (computer, sealed name, version).
    fn list(&self, group: &str, token: &str) -> std::result::Result<Vec<(String, String, String)>, ApiError>;
    fn get(&self, group: &str, token: &str, computer: &str, name: &str) -> std::result::Result<Vec<u8>, ApiError>;
    fn put(&self, group: &str, token: &str, computer: &str, name: &str, bytes: &[u8]) -> std::result::Result<(), ApiError>;
    fn delete(&self, group: &str, token: &str, computer: &str, name: &str) -> std::result::Result<(), ApiError>;
    fn delete_computer(&self, group: &str, token: &str, computer: &str) -> std::result::Result<(), ApiError>;
}

/// A group of linked computers: its id, and the key they share.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub id: String,
    pub key: [u8; 32],
}

impl Group {
    fn token(&self) -> String {
        B64URL.encode(Keys::derive(&self.key).token)
    }
}

pub fn new_group() -> Group {
    Group { id: seal::random::<16>().iter().map(|b| format!("{b:02x}")).collect(), key: seal::random() }
}

/// Tells the relay about a new group, by its token's hash alone.
/// The relay keeps the hash of the token as it's sent, and compares each request's against it.
pub fn register(api: &dyn Api, group: &Group) -> Result<()> {
    api.create_group(&group.id, &seal::token_hash(group.token().as_bytes())).map_err(String::from)
}

/// This computer's group, while it combines through the relay.
pub fn group(conn: &Connection) -> Result<Option<Group>> {
    let id = db::get_setting(conn, GROUP_KEY).map_err(err)?.filter(|g| !g.is_empty());
    let key = db::get_setting(conn, KEY_KEY).map_err(err)?.and_then(|k| B64.decode(k).ok()).and_then(|k| <[u8; 32]>::try_from(k).ok());
    Ok(id.zip(key).map(|(id, key)| Group { id, key }))
}

pub fn set_group(conn: &Connection, group: Option<&Group>) -> Result<()> {
    db::set_setting(conn, GROUP_KEY, group.map_or("", |g| &g.id)).map_err(err)?;
    db::set_setting(conn, KEY_KEY, &group.map_or(String::new(), |g| B64.encode(g.key))).map_err(err)
}

/// A relay group, seen as a store.
pub struct RelayStore {
    api: Arc<dyn Api>,
    group: String,
    token: String,
    keys: Keys,
    me: String,
}

impl RelayStore {
    pub fn new(api: Arc<dyn Api>, group: &Group, me: &str) -> Self {
        Self { api, group: group.id.clone(), token: group.token(), keys: Keys::derive(&group.key), me: me.to_string() }
    }
}

impl RelayStore {
    /// A file's name on the relay: sealed together with its computer, so the same file on two
    /// computers doesn't share a name and the relay can't line up which days they both tracked.
    fn object(&self, computer: &str, name: &str) -> String {
        self.keys.seal_name(&format!("{computer}/{name}"))
    }
}

impl Store for RelayStore {
    fn list(&self) -> Result<Listing> {
        let mut out = Listing::new();
        for (computer, sealed, version) in self.api.list(&self.group, &self.token)? {
            // A name this key doesn't open, or one filed under another computer, isn't one of
            // ours; it's left alone.
            let opened = self.keys.open_name(&sealed).ok();
            if let Some(name) = opened.as_deref().and_then(|n| n.strip_prefix(&format!("{computer}/"))) {
                out.entry(computer.clone()).or_default().insert(name.to_string(), version);
            }
        }
        Ok(out)
    }

    fn get(&self, computer: &str, name: &str) -> Result<Vec<u8>> {
        let sealed = self.api.get(&self.group, &self.token, computer, &self.object(computer, name))?;
        self.keys.open(&format!("{computer}/{name}"), &sealed)
    }

    fn put(&self, name: &str, bytes: &[u8]) -> Result<()> {
        let sealed = self.keys.seal(&format!("{}/{name}", self.me), bytes);
        Ok(self.api.put(&self.group, &self.token, &self.me, &self.object(&self.me, name), &sealed)?)
    }

    fn delete(&self, name: &str) -> Result<()> {
        Ok(self.api.delete(&self.group, &self.token, &self.me, &self.object(&self.me, name))?)
    }

    fn remove_me(&self) -> Result<()> {
        Ok(self.api.delete_computer(&self.group, &self.token, &self.me)?)
    }
}

/// Deletes the whole group from the relay, for starting over.
pub fn delete_group(api: &dyn Api, group: &Group) -> Result<()> {
    match api.delete_group(&group.id, &group.token()) {
        Ok(()) | Err(ApiError::Gone) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// The relay over HTTPS.
pub struct Http {
    base: String,
}

impl Default for Http {
    fn default() -> Self {
        let base = std::env::var("KAIRO_RELAY").ok().filter(|b| !b.is_empty()).unwrap_or_else(|| RELAY_URL.to_string());
        Self { base: base.trim_end_matches('/').to_string() }
    }
}

pub enum Body<'a> {
    None,
    Bytes(&'a [u8]),
    Json(serde_json::Value),
}

impl Http {
    /// One request: its status and body. Runs on the calling thread, which must not be one of the
    /// async runtime's own.
    pub fn send(&self, method: &str, path: &str, token: Option<&str>, body: Body) -> std::result::Result<(u16, Vec<u8>), ApiError> {
        // rustls needs its cryptography chosen once per process; the updater does the same.
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        let url = format!("{}{path}", self.base);
        let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| ApiError::Other(e.to_string()))?;
        tauri::async_runtime::block_on(async {
            let client = reqwest::Client::builder().timeout(TIMEOUT).build().map_err(|e| ApiError::Other(e.to_string()))?;
            let mut request = client.request(method, &url);
            if let Some(token) = token {
                request = request.header("authorization", format!("Bearer {token}"));
            }
            request = match body {
                Body::None => request,
                Body::Bytes(bytes) => request.header("content-type", "application/octet-stream").body(bytes.to_vec()),
                Body::Json(value) => request.header("content-type", "application/json").body(value.to_string()),
            };
            let response = request.send().await.map_err(|e| ApiError::Unreachable(e.to_string()))?;
            let status = response.status().as_u16();
            let bytes = response.bytes().await.map_err(|e| ApiError::Unreachable(e.to_string()))?;
            Ok((status, bytes.to_vec()))
        })
    }

    /// A group request: 401 and 404 both mean the group is gone for this computer.
    fn group(&self, method: &str, path: &str, token: &str, body: Body) -> std::result::Result<Vec<u8>, ApiError> {
        match self.send(method, path, Some(token), body)? {
            (200..=299, bytes) => Ok(bytes),
            (401 | 404, _) => Err(ApiError::Gone),
            (status, bytes) if status >= 500 && status != 507 => Err(ApiError::Unreachable(format!("{status} {}", String::from_utf8_lossy(&bytes)))),
            (status, bytes) => Err(ApiError::Other(format!("{status} {}", String::from_utf8_lossy(&bytes)))),
        }
    }
}

impl Api for Http {
    fn create_group(&self, group: &str, token_hash: &str) -> std::result::Result<(), ApiError> {
        match self.send("POST", "/v1/groups", None, Body::Json(serde_json::json!({ "id": group, "tokenHash": token_hash })))? {
            (201, _) => Ok(()),
            (status, bytes) => Err(ApiError::Other(format!("{status} {}", String::from_utf8_lossy(&bytes)))),
        }
    }

    fn delete_group(&self, group: &str, token: &str) -> std::result::Result<(), ApiError> {
        self.group("DELETE", &format!("/v1/groups/{group}"), token, Body::None).map(|_| ())
    }

    fn list(&self, group: &str, token: &str) -> std::result::Result<Vec<(String, String, String)>, ApiError> {
        #[derive(serde::Deserialize)]
        struct Entry {
            computer: String,
            name: String,
            version: String,
        }
        let bytes = self.group("GET", &format!("/v1/groups/{group}/files"), token, Body::None)?;
        let entries: Vec<Entry> = serde_json::from_slice(&bytes).map_err(|e| ApiError::Other(e.to_string()))?;
        Ok(entries.into_iter().map(|e| (e.computer, e.name, e.version)).collect())
    }

    fn get(&self, group: &str, token: &str, computer: &str, name: &str) -> std::result::Result<Vec<u8>, ApiError> {
        // A missing file is not a missing group.
        match self.send("GET", &format!("/v1/groups/{group}/files/{computer}/{name}"), Some(token), Body::None)? {
            (200, bytes) => Ok(bytes),
            (401, _) => Err(ApiError::Gone),
            (status, _) => Err(ApiError::Other(format!("{status}"))),
        }
    }

    fn put(&self, group: &str, token: &str, computer: &str, name: &str, bytes: &[u8]) -> std::result::Result<(), ApiError> {
        self.group("PUT", &format!("/v1/groups/{group}/files/{computer}/{name}"), token, Body::Bytes(bytes)).map(|_| ())
    }

    fn delete(&self, group: &str, token: &str, computer: &str, name: &str) -> std::result::Result<(), ApiError> {
        self.group("DELETE", &format!("/v1/groups/{group}/files/{computer}/{name}"), token, Body::None).map(|_| ())
    }

    fn delete_computer(&self, group: &str, token: &str, computer: &str) -> std::result::Result<(), ApiError> {
        self.group("DELETE", &format!("/v1/groups/{group}/computers/{computer}"), token, Body::None).map(|_| ())
    }
}

/// A file in the in-memory relay: group, computer, sealed name.
#[cfg(test)]
type MemKey = (String, String, String);

/// A relay in memory, for tests: groups by token hash, files by group, computer and name.
#[cfg(test)]
#[derive(Default)]
pub struct MemApi {
    /// Pairing slots, for `pair::Mailbox`.
    pub slots: std::sync::Mutex<std::collections::HashMap<String, super::pair::Parts>>,
    groups: std::sync::Mutex<std::collections::HashMap<String, String>>,
    files: std::sync::Mutex<std::collections::BTreeMap<MemKey, (Vec<u8>, u64)>>,
}

#[cfg(test)]
impl MemApi {
    fn check(&self, group: &str, token: &str) -> std::result::Result<(), ApiError> {
        match self.groups.lock().unwrap().get(group) {
            Some(h) if *h == seal::token_hash(token.as_bytes()) => Ok(()),
            _ => Err(ApiError::Gone),
        }
    }

    /// Every stored file: its sealed name and sealed bytes.
    pub fn stored(&self) -> Vec<(String, Vec<u8>)> {
        self.files.lock().unwrap().iter().map(|((_, _, name), (bytes, _))| (name.clone(), bytes.clone())).collect()
    }

    pub fn forget_groups(&self) {
        self.groups.lock().unwrap().clear();
        self.files.lock().unwrap().clear();
    }
}

#[cfg(test)]
impl Api for MemApi {
    fn create_group(&self, group: &str, token_hash: &str) -> std::result::Result<(), ApiError> {
        self.groups.lock().unwrap().insert(group.into(), token_hash.into());
        Ok(())
    }

    fn delete_group(&self, group: &str, token: &str) -> std::result::Result<(), ApiError> {
        self.check(group, token)?;
        self.groups.lock().unwrap().remove(group);
        self.files.lock().unwrap().retain(|(g, _, _), _| g != group);
        Ok(())
    }

    fn list(&self, group: &str, token: &str) -> std::result::Result<Vec<(String, String, String)>, ApiError> {
        self.check(group, token)?;
        Ok(self.files.lock().unwrap().iter().filter(|((g, _, _), _)| g == group).map(|((_, c, n), (_, v))| (c.clone(), n.clone(), v.to_string())).collect())
    }

    fn get(&self, group: &str, token: &str, computer: &str, name: &str) -> std::result::Result<Vec<u8>, ApiError> {
        self.check(group, token)?;
        self.files.lock().unwrap().get(&(group.into(), computer.into(), name.into())).map(|(b, _)| b.clone()).ok_or(ApiError::Other("404".into()))
    }

    fn put(&self, group: &str, token: &str, computer: &str, name: &str, bytes: &[u8]) -> std::result::Result<(), ApiError> {
        self.check(group, token)?;
        let mut files = self.files.lock().unwrap();
        let version = files.get(&(group.into(), computer.into(), name.into())).map_or(1, |(_, v)| v + 1);
        files.insert((group.into(), computer.into(), name.into()), (bytes.to_vec(), version));
        Ok(())
    }

    fn delete(&self, group: &str, token: &str, computer: &str, name: &str) -> std::result::Result<(), ApiError> {
        self.check(group, token)?;
        self.files.lock().unwrap().remove(&(group.into(), computer.into(), name.into()));
        Ok(())
    }

    fn delete_computer(&self, group: &str, token: &str, computer: &str) -> std::result::Result<(), ApiError> {
        self.check(group, token)?;
        self.files.lock().unwrap().retain(|(g, c, _), _| !(g == group && c == computer));
        Ok(())
    }
}
