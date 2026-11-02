//! Combining through Kairo's relay: the same files a synced folder would hold, sealed on this
//! computer (`seal.rs`) and kept by a small Cloudflare Worker (`relay/`) under names it can't
//! read. The relay is only ever reached from the sync and pairing threads, never while the app's
//! database is locked.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use rusqlite::Connection;

use super::seal::{self, Keys};
use super::store::{Listing, Store};
use super::{err, Result};
use crate::db;

/// Where Kairo's relay answers. `KAIRO_RELAY` in the environment overrides it, for local runs.
pub const RELAY_URL: &str = "https://kairo-relay.kairo-relay.workers.dev";
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

/// A file on the relay: (computer, sealed name, version).
pub type Entry = (String, String, String);

/// What changed in a group since a count the relay gave before.
#[derive(Debug, Default, PartialEq)]
pub struct Changes {
    /// Where the group's count is now, to ask from next time; 0 when the relay doesn't count.
    pub seq: u64,
    /// Every file, rather than what changed: the first time, or when the relay can't say.
    pub full: bool,
    pub files: Vec<Entry>,
    /// Files deleted since.
    pub gone: Vec<Entry>,
}

/// What the relay offers for a group's files.
pub trait Api: Send + Sync {
    fn create_group(&self, group: &str, token_hash: &str) -> std::result::Result<(), ApiError>;
    fn delete_group(&self, group: &str, token: &str) -> std::result::Result<(), ApiError>;
    /// Every file.
    fn list(&self, group: &str, token: &str) -> std::result::Result<Vec<Entry>, ApiError>;
    /// What changed since `since` (0 for everything). A relay that can't say lists every file.
    fn list_since(&self, group: &str, token: &str, _since: u64) -> std::result::Result<Changes, ApiError> {
        Ok(Changes { seq: 0, full: true, files: self.list(group, token)?, gone: Vec::new() })
    }
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

/// The groups this computer has left or started over from, while Kairo runs, as (group, computer).
/// A sync round that read its group before then may still be running; its uploads check here
/// first, under this lock, so none lands after this computer's files were taken off.
static RETIRED: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

fn retired_list() -> std::sync::MutexGuard<'static, Vec<(String, String)>> {
    RETIRED.lock().unwrap_or_else(|e| e.into_inner())
}

/// Marks `group` as left by computer `me`: from now on its rounds write nothing to it. Waits for
/// an upload already under way, so the files taken off after this stay off.
pub fn retire(group: &str, me: &str) {
    let mut list = retired_list();
    if !list.iter().any(|(g, m)| g == group && m == me) {
        list.push((group.to_string(), me.to_string()));
    }
}

/// Lets computer `me` write to `group` again, as when it rejoins a group it left.
pub fn unretire(group: &str, me: &str) {
    retired_list().retain(|(g, m)| !(g == group && m == me));
}

/// Whether computer `me` left or started over from `group` since Kairo started.
pub fn is_retired(group: &str, me: &str) -> bool {
    retired_list().iter().any(|(g, m)| g == group && m == me)
}

/// What a store error says when an upload was stopped because this computer left the group.
pub const LEFT: &str = "This computer left that group.";

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

impl RelayStore {
    /// Runs a write unless this computer left the group, holding the lock `retire` takes, so a
    /// write either lands before leaving takes this computer's files off or not at all.
    fn unless_left(&self, write: impl FnOnce() -> std::result::Result<(), ApiError>) -> Result<()> {
        let list = retired_list();
        if list.iter().any(|(g, m)| *g == self.group && *m == self.me) {
            return Err(LEFT.into());
        }
        Ok(write()?)
    }
}

/// Each group's files as this computer last listed them, and the relay's count then, while Kairo
/// runs, by (group, computer), so a round asks only for what changed since the last one. The first
/// round after Kairo starts lists everything.
type Known = (u64, HashMap<(String, String), String>);
static KNOWN: std::sync::Mutex<Option<HashMap<(String, String), Known>>> = std::sync::Mutex::new(None);

impl RelayStore {
    /// Every file on the relay, from what changed since the last round and what was known then.
    fn entries(&self) -> Result<Vec<Entry>> {
        let key = (self.group.clone(), self.me.clone());
        let since = KNOWN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).get(&key).map_or(0, |(seq, _)| *seq);
        let mut changes = self.api.list_since(&self.group, &self.token, since)?;
        // A count behind the one known means the relay started counting afresh: list everything.
        if since > 0 && changes.seq < since {
            changes = self.api.list_since(&self.group, &self.token, 0)?;
        }
        let mut all = KNOWN.lock().unwrap_or_else(|e| e.into_inner());
        let known = all.get_or_insert_with(HashMap::new).entry(key).or_default();
        if changes.full {
            known.1.clear();
        }
        for (computer, name, _) in changes.gone {
            known.1.remove(&(computer, name));
        }
        for (computer, name, version) in changes.files {
            known.1.insert((computer, name), version);
        }
        known.0 = changes.seq;
        Ok(known.1.iter().map(|((c, n), v)| (c.clone(), n.clone(), v.clone())).collect())
    }
}

impl Store for RelayStore {
    fn list(&self) -> Result<Listing> {
        let mut out = Listing::new();
        for (computer, sealed, version) in self.entries()? {
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
        self.unless_left(|| self.api.put(&self.group, &self.token, &self.me, &self.object(&self.me, name), &sealed))
    }

    fn delete(&self, name: &str) -> Result<()> {
        self.unless_left(|| self.api.delete(&self.group, &self.token, &self.me, &self.object(&self.me, name)))
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

/// The relay over HTTPS. One client for every request it makes, so a sync round's many
/// uploads share a connection rather than each opening its own.
pub struct Http {
    base: String,
    client: Option<reqwest::Client>,
}

impl Default for Http {
    fn default() -> Self {
        // rustls needs its cryptography chosen once per process; the updater does the same.
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        let base = std::env::var("KAIRO_RELAY").ok().filter(|b| !b.is_empty()).unwrap_or_else(|| RELAY_URL.to_string());
        // Without a client, each request says why; there's no other way to reach the relay.
        let client = reqwest::Client::builder().timeout(TIMEOUT).build().ok();
        Self { base: base.trim_end_matches('/').to_string(), client }
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
        let url = format!("{}{path}", self.base);
        let method = reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| ApiError::Other(e.to_string()))?;
        let client = self.client.as_ref().ok_or_else(|| ApiError::Other("no HTTPS client".into()))?;
        tauri::async_runtime::block_on(async {
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

    /// A group request, answered as `group_answer` reads it.
    fn group(&self, method: &str, path: &str, token: &str, body: Body) -> std::result::Result<Vec<u8>, ApiError> {
        let (status, bytes) = self.send(method, path, Some(token), body)?;
        group_answer(status, bytes)
    }
}

/// What the relay's answer to a group request means. The group is gone only when the relay itself
/// says so, in its own words: a 404 or 401 from a proxy, a Wi-Fi sign-in page or a wrong address
/// would otherwise make this computer forget its group for good.
fn group_answer(status: u16, bytes: Vec<u8>) -> std::result::Result<Vec<u8>, ApiError> {
    let said = String::from_utf8_lossy(&bytes).chars().take(80).collect::<String>();
    match (status, said.as_str()) {
        (200..=299, _) => Ok(bytes),
        (404, "no such group") | (401, "wrong token") => Err(ApiError::Gone),
        (507, _) => Err(ApiError::Other("it's full: these computers' history together is over the 50 MB it keeps. Keep less history in Settings → Privacy.".into())),
        (413, _) => Err(ApiError::Other("one day's file is too big for it.".into())),
        (500.., _) => Err(ApiError::Unreachable(format!("{status} {said}"))),
        _ => Err(ApiError::Other(format!("{status} {said}"))),
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

    fn list_since(&self, group: &str, token: &str, since: u64) -> std::result::Result<Changes, ApiError> {
        #[derive(serde::Deserialize)]
        struct Row {
            computer: String,
            name: String,
            version: String,
        }
        #[derive(serde::Deserialize)]
        struct Answer {
            seq: String,
            full: bool,
            files: Vec<Row>,
            gone: Vec<Row>,
        }
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Either {
            Changes(Answer),
            // A relay from before it counted changes ignores `since` and lists every file.
            All(Vec<Row>),
        }
        let rows = |rows: Vec<Row>| rows.into_iter().map(|r| (r.computer, r.name, r.version)).collect();
        let bytes = self.group("GET", &format!("/v1/groups/{group}/files?since={since}"), token, Body::None)?;
        Ok(match serde_json::from_slice(&bytes).map_err(|e| ApiError::Other(e.to_string()))? {
            Either::Changes(a) => Changes { seq: a.seq.parse().unwrap_or(0), full: a.full, files: rows(a.files), gone: rows(a.gone) },
            Either::All(all) => Changes { seq: 0, full: true, files: rows(all), gone: Vec::new() },
        })
    }

    fn get(&self, group: &str, token: &str, computer: &str, name: &str) -> std::result::Result<Vec<u8>, ApiError> {
        // A missing file ("no such file") is not a missing group.
        self.group("GET", &format!("/v1/groups/{group}/files/{computer}/{name}"), token, Body::None)
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
    pub slots: std::sync::Mutex<std::collections::HashMap<String, super::pair::MemSlot>>,
    groups: std::sync::Mutex<std::collections::HashMap<String, String>>,
    files: std::sync::Mutex<std::collections::BTreeMap<MemKey, (Vec<u8>, u64)>>,
    /// Each group's count of writes, which files take as their versions, as the relay does.
    seq: std::sync::Mutex<std::collections::HashMap<String, u64>>,
    /// Files deleted, under the count when they went.
    gone: std::sync::Mutex<std::collections::BTreeMap<MemKey, u64>>,
    /// How many times each kind of request was made.
    pub calls: std::sync::Mutex<std::collections::HashMap<&'static str, u32>>,
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
        self.gone.lock().unwrap().clear();
    }

    /// How many requests of `kind` were made, since the count was last reset.
    pub fn count(&self, kind: &str) -> u32 {
        self.calls.lock().unwrap().get(kind).copied().unwrap_or(0)
    }

    pub fn reset_counts(&self) {
        self.calls.lock().unwrap().clear();
    }

    fn called(&self, kind: &'static str) {
        *self.calls.lock().unwrap().entry(kind).or_default() += 1;
    }

    /// The group's next count.
    fn next(&self, group: &str) -> u64 {
        let mut seq = self.seq.lock().unwrap();
        let n = seq.entry(group.into()).or_default();
        *n += 1;
        *n
    }

    /// Notes the files `which` picks in `group` as gone, and takes them away.
    fn take(&self, group: &str, which: impl Fn(&MemKey) -> bool) {
        let n = self.next(group);
        let mut files = self.files.lock().unwrap();
        let mut gone = self.gone.lock().unwrap();
        for key in files.keys().filter(|k| k.0 == group && which(k)) {
            gone.insert(key.clone(), n);
        }
        files.retain(|k, _| !(k.0 == group && which(k)));
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

    fn list(&self, group: &str, token: &str) -> std::result::Result<Vec<Entry>, ApiError> {
        self.called("list");
        self.check(group, token)?;
        Ok(self.files.lock().unwrap().iter().filter(|((g, _, _), _)| g == group).map(|((_, c, n), (_, v))| (c.clone(), n.clone(), v.to_string())).collect())
    }

    fn list_since(&self, group: &str, token: &str, since: u64) -> std::result::Result<Changes, ApiError> {
        self.called("list");
        self.check(group, token)?;
        let seq = self.seq.lock().unwrap().get(group).copied().unwrap_or(0);
        let entry = |((_, c, n), v): (&MemKey, u64)| (c.clone(), n.clone(), v.to_string());
        let files = self.files.lock().unwrap().iter().filter(|((g, _, _), (_, v))| g == group && *v > since).map(|(k, (_, v))| entry((k, *v))).collect();
        let gone = if since == 0 {
            Vec::new()
        } else {
            self.gone.lock().unwrap().iter().filter(|((g, _, _), v)| g == group && **v > since).map(|(k, v)| entry((k, *v))).collect()
        };
        Ok(Changes { seq, full: since == 0, files, gone })
    }

    fn get(&self, group: &str, token: &str, computer: &str, name: &str) -> std::result::Result<Vec<u8>, ApiError> {
        self.called("get");
        self.check(group, token)?;
        self.files.lock().unwrap().get(&(group.into(), computer.into(), name.into())).map(|(b, _)| b.clone()).ok_or(ApiError::Other("404".into()))
    }

    fn put(&self, group: &str, token: &str, computer: &str, name: &str, bytes: &[u8]) -> std::result::Result<(), ApiError> {
        self.called("put");
        self.check(group, token)?;
        let version = self.next(group);
        let key: MemKey = (group.into(), computer.into(), name.into());
        self.gone.lock().unwrap().remove(&key);
        self.files.lock().unwrap().insert(key, (bytes.to_vec(), version));
        Ok(())
    }

    fn delete(&self, group: &str, token: &str, computer: &str, name: &str) -> std::result::Result<(), ApiError> {
        self.called("delete");
        self.check(group, token)?;
        self.take(group, |(_, c, n)| c == computer && n == name);
        Ok(())
    }

    fn delete_computer(&self, group: &str, token: &str, computer: &str) -> std::result::Result<(), ApiError> {
        self.called("delete");
        self.check(group, token)?;
        self.take(group, |(_, c, _)| c == computer);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_after_the_first_ask_only_for_what_changed() {
        let api = Arc::new(MemApi::default());
        let group = new_group();
        register(api.as_ref(), &group).unwrap();
        let (desk, laptop) = (RelayStore::new(api.clone(), &group, "d1"), RelayStore::new(api.clone(), &group, "a2"));
        desk.put("one", b"1").unwrap();
        desk.put("two", b"2").unwrap();
        let names = |s: &RelayStore| {
            let mut n: Vec<String> = s.list().unwrap().get("d1").map(|f| f.keys().cloned().collect()).unwrap_or_default();
            n.sort();
            n
        };
        assert_eq!(names(&laptop), ["one", "two"]);
        let first = laptop.list().unwrap()["d1"]["two"].clone();
        // Changes arrive, deletions too, and what didn't change is still there.
        desk.put("two", b"22").unwrap();
        desk.delete("one").unwrap();
        desk.put("three", b"3").unwrap();
        assert_eq!(names(&laptop), ["three", "two"]);
        assert_ne!(laptop.list().unwrap()["d1"]["two"], first);
        desk.remove_me().unwrap();
        assert!(laptop.list().unwrap().is_empty());
    }

    #[test]
    fn only_the_relays_own_word_means_the_group_is_gone() {
        assert_eq!(group_answer(404, b"no such group".to_vec()), Err(ApiError::Gone));
        assert_eq!(group_answer(401, b"wrong token".to_vec()), Err(ApiError::Gone));
        // A proxy, a sign-in page or a wrong address isn't the relay saying so.
        for (status, body) in [(404, ""), (404, "<html>Not Found</html>"), (401, "Proxy authentication required"), (404, "no such file")] {
            assert!(matches!(group_answer(status, body.as_bytes().to_vec()), Err(ApiError::Other(_))), "{status} {body}");
        }
        assert_eq!(group_answer(200, b"ok".to_vec()), Ok(b"ok".to_vec()));
        assert!(matches!(group_answer(503, Vec::new()), Err(ApiError::Unreachable(_))));
        assert!(group_answer(507, b"group full".to_vec()).unwrap_err().to_string().contains("full"));
    }
}
