//! Combining computers: each computer writes its own tracked time into a folder the user's cloud
//! drive already syncs (OneDrive, Dropbox, iCloud Drive, Syncthing...), and reads the others' in,
//! so the day views, limits and recaps add up every computer. There's no server; the drive does
//! the moving. The categories, limits, goals and rules the user chooses are shared too, the latest
//! choice winning everywhere; focus sessions stay per computer.
//!
//! In the folder, `Kairo computers/<computer id>/` holds `computer.json` (its name),
//! `choices.json` (its choices, with when each was made) and one small file per day
//! (`<day>.kairo-day`, days since 1970 in UTC), rewritten only when that day changed, so the
//! drive moves a few kilobytes rather than the whole history.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::{db, privacy};

pub mod seal;
pub mod link;
pub mod pair;
pub mod relay;
pub mod store;

use store::Store;

const FOLDER_KEY: &str = "sync_folder";
const DEVICE_KEY: &str = "device_id";
const DIR: &str = "Kairo computers";
const DAY_FILE: &str = "kairo-day";
const ABOUT_FILE: &str = "computer.json";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const FIRST_AFTER: Duration = Duration::from_secs(30);
const EVERY: Duration = Duration::from_secs(5 * 60);

type Result<T> = std::result::Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Serialize, Deserialize)]
struct About {
    name: String,
    updated_ms: i64,
}

/// A computer whose time is combined, for Settings.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Computer {
    pub id: String,
    pub name: String,
    /// When it last wrote its time.
    pub updated_ms: i64,
    pub this_one: bool,
}

/// The folder being synced, if combining is on.
pub fn folder(conn: &Connection) -> Result<Option<PathBuf>> {
    Ok(db::get_setting(conn, FOLDER_KEY).map_err(err)?.filter(|f| !f.is_empty()).map(PathBuf::from))
}

const SHOWN_KEY: &str = "shown_computer";

/// Whose time Today, Week and Patterns show: `analysis::ALL_COMPUTERS`, "this" or
/// another computer's id. Everyone's, unless asked otherwise while combining.
pub fn shown(conn: &Connection) -> Result<String> {
    let shown = db::get_setting(conn, SHOWN_KEY).map_err(err)?;
    Ok(match shown {
        Some(s) if combining(conn)? => s,
        _ => crate::analysis::ALL_COMPUTERS.to_string(),
    })
}

pub fn set_shown(conn: &Connection, computer: &str) -> Result<()> {
    db::set_setting(conn, SHOWN_KEY, computer).map_err(err)
}

/// This computer's id among the others, made the first time it's asked for.
pub fn device_id(conn: &Connection) -> Result<String> {
    if let Some(id) = db::get_setting(conn, DEVICE_KEY).map_err(err)? {
        return Ok(id);
    }
    let id = new_id();
    db::set_setting(conn, DEVICE_KEY, &id).map_err(err)?;
    Ok(id)
}

pub(crate) fn new_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    // RandomState is seeded randomly for each run of Kairo.
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    hasher.write_u32(std::process::id());
    format!("{:016x}", hasher.finish())
}

/// The computer's own name, as the system calls it.
pub fn device_name() -> String {
    #[cfg(windows)]
    let name = std::env::var("COMPUTERNAME").ok();
    #[cfg(not(windows))]
    let name = {
        let mut buf = [0u8; 256];
        (unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0)
            .then(|| std::ffi::CStr::from_bytes_until_nul(&buf).ok().map(|n| n.to_string_lossy().trim_end_matches(".local").to_string()))
            .flatten()
    };
    name.filter(|n| !n.is_empty()).unwrap_or_else(|| "This computer".into())
}

/// Starts combining through `dir`, at once. True when another computer's choices came in.
pub fn start(conn: &Connection, dir: &Path, now_ms: i64) -> Result<bool> {
    // Moving to another folder takes this computer out of the old one, so the computers still
    // there drop its time, as stopping would.
    if let Some(old) = folder(conn)?.filter(|old| old != dir) {
        let _ = store::FolderStore::new(&old, &device_id(conn)?).remove_me();
    }
    db::set_setting(conn, FOLDER_KEY, &dir.to_string_lossy()).map_err(err)?;
    run_once(conn, now_ms)
}

/// Stops combining: the other computers' time leaves this one, and this one's folder leaves the
/// synced folder, so they drop its time too.
pub fn stop(conn: &Connection) -> Result<()> {
    if let Some(root) = folder(conn)? {
        let _ = store::FolderStore::new(&root, &device_id(conn)?).remove_me();
    }
    db::set_setting(conn, FOLDER_KEY, "").map_err(err)?;
    forget_everyone(conn)
}

/// The other computers' time leaves this one, as does what was noted about them and what this one
/// wrote, so a group joined next gets all of this computer's files.
fn forget_everyone(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DELETE FROM sessions WHERE device IS NOT NULL; DELETE FROM sync_imports; DELETE FROM sync_written;
         DELETE FROM settings WHERE key IN ('sync_computers', 'sync_seen', 'sync_choices_written', 'sync_about_written');",
    )
    .map_err(err)
}

/// Whether this computer is combining with others, through a folder or the relay.
pub fn combining(conn: &Connection) -> Result<bool> {
    Ok(folder(conn)?.is_some() || relay::group(conn)?.is_some())
}

const SYNCED_KEY: &str = "relay_synced_ms";
const ERROR_KEY: &str = "relay_error";
/// `relay_error` when another computer started over and this one's group went away.
pub const UNLINKED: &str = "unlinked";

/// When this computer last synced through the relay, and what went wrong since, if anything.
pub fn relay_status(conn: &Connection) -> Result<(Option<i64>, Option<String>)> {
    let synced = db::get_setting(conn, SYNCED_KEY).map_err(err)?.and_then(|s| s.parse().ok());
    let error = db::get_setting(conn, ERROR_KEY).map_err(err)?.filter(|e| !e.is_empty());
    Ok((synced, error))
}

/// Starts combining through a relay group this computer just made or joined. A round still out
/// with the group it was in before writes nothing more to that one.
pub fn join_relay(conn: &Connection, group: &relay::Group) -> Result<()> {
    let me = device_id(conn)?;
    if let Some(old) = relay::group(conn)?.filter(|old| old.id != group.id) {
        relay::retire(&old.id, &me);
    }
    relay::unretire(&group.id, &me);
    forget_everyone(conn)?;
    db::set_setting(conn, FOLDER_KEY, "").map_err(err)?;
    db::set_setting(conn, ERROR_KEY, "").map_err(err)?;
    relay::set_group(conn, Some(group))
}

/// One round through the relay. When the relay no longer knows the group (another computer
/// started over), this computer stops combining and notes that it was unlinked.
pub fn run_relay(conn: &Connection, api: std::sync::Arc<dyn relay::Api>, now_ms: i64) -> Result<bool> {
    let Some(group) = relay::group(conn)? else { return Ok(false) };
    run_relay_in(conn, api, &group, now_ms)
}

/// One round through `group`, the group this computer was in when the round began. Leaving or
/// starting over may happen while it runs, on another thread; then the round's uploads are refused,
/// what it read in is dropped, and the group kept now is left alone.
fn run_relay_in(conn: &Connection, api: std::sync::Arc<dyn relay::Api>, group: &relay::Group, now_ms: i64) -> Result<bool> {
    let me = device_id(conn)?;
    let store = relay::RelayStore::new(api, group, &me);
    let result = run_with(conn, &store, now_ms);
    if relay::is_retired(&group.id, &me) {
        // Its time came from a group this computer isn't in; the next round reads its own again.
        forget_everyone(conn)?;
        return Ok(false);
    }
    match result {
        Ok(changed) => {
            db::set_setting(conn, SYNCED_KEY, &now_ms.to_string()).map_err(err)?;
            db::set_setting(conn, ERROR_KEY, "").map_err(err)?;
            Ok(changed)
        }
        // Only the group this round was in: another may have been joined since it began.
        Err(e) if e == relay::GONE && relay::group(conn)?.as_ref() == Some(group) => {
            relay::set_group(conn, None)?;
            forget_everyone(conn)?;
            db::set_setting(conn, ERROR_KEY, UNLINKED).map_err(err)?;
            Err(e)
        }
        Err(e) if e == relay::GONE => Err(e),
        Err(e) => {
            db::set_setting(conn, ERROR_KEY, &e).map_err(err)?;
            Err(e)
        }
    }
}

/// Stops combining through the relay: this computer's files leave it, so the others drop its
/// time, and theirs leaves this one. The group is forgotten even when the relay can't be reached.
pub fn leave_relay(conn: &Connection, api: std::sync::Arc<dyn relay::Api>) -> Result<()> {
    let Some(group) = relay::group(conn)? else { return Ok(()) };
    let me = device_id(conn)?;
    // First, so a round the sync thread still has out uploads nothing after the files go.
    relay::retire(&group.id, &me);
    let left = match relay::RelayStore::new(api, &group, &me).remove_me() {
        Err(e) if e == relay::GONE => Ok(()),
        other => other,
    };
    relay::set_group(conn, None)?;
    db::set_setting(conn, ERROR_KEY, "").map_err(err)?;
    forget_everyone(conn)?;
    left
}

const COMPUTERS_KEY: &str = "sync_computers";

/// The combined computers as of the last sync, this one first. Kept from the last sync rather
/// than read from the store, so Settings never waits on a drive or the network.
pub fn computers(conn: &Connection) -> Result<Vec<Computer>> {
    let me = device_id(conn)?;
    let Some(text) = db::get_setting(conn, COMPUTERS_KEY).map_err(err)? else { return Ok(Vec::new()) };
    let about: HashMap<String, About> = serde_json::from_str(&text).unwrap_or_default();
    let mut found: Vec<Computer> =
        about.into_iter().map(|(id, a)| Computer { this_one: id == me, id, name: a.name, updated_ms: a.updated_ms }).collect();
    found.sort_by_key(|c| (!c.this_one, c.name.to_lowercase()));
    Ok(found)
}

/// Notes who's combined, from each computer's `computer.json`, read again only when it changed,
/// and this computer as it is now.
fn note_computers(conn: &Connection, store: &dyn Store, listing: &store::Listing, me: &str, seen: &mut Seen, now_ms: i64) -> Result<()> {
    let mut known: HashMap<String, About> = db::get_setting(conn, COMPUTERS_KEY).map_err(err)?.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let mut about = HashMap::new();
    for (id, files) in listing.iter().filter(|(id, _)| *id != me) {
        let Some(version) = files.get(ABOUT_FILE) else { continue };
        let read = match known.remove(id) {
            Some(old) if seen.has(id, ABOUT_FILE, version) => Some(old),
            _ => store.get(id, ABOUT_FILE).ok().and_then(|b| serde_json::from_slice::<About>(&b).ok()),
        };
        if let Some(read) = read {
            seen.note(id, ABOUT_FILE, version);
            about.insert(id.clone(), read);
        }
    }
    about.insert(me.to_string(), About { name: device_name(), updated_ms: now_ms });
    db::set_setting(conn, COMPUTERS_KEY, &serde_json::to_string(&about).map_err(err)?).map_err(err)
}

const SEEN_KEY: &str = "sync_seen";

/// The version of each other computer's `computer.json` and `choices.json` last read, so a round
/// reads them only when they changed. Kept for one version of Kairo: a newer one reads them all
/// again, since it may take choices the one before had to leave waiting.
#[derive(Serialize, Deserialize, Default)]
pub(crate) struct Seen {
    kairo: String,
    files: HashMap<String, String>,
}

impl Seen {
    fn load(conn: &Connection) -> Result<Seen> {
        let seen: Seen = db::get_setting(conn, SEEN_KEY).map_err(err)?.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Ok(if seen.kairo == env!("CARGO_PKG_VERSION") { seen } else { Seen { kairo: env!("CARGO_PKG_VERSION").into(), files: HashMap::new() } })
    }

    fn save(&self, conn: &Connection) -> Result<()> {
        db::set_setting(conn, SEEN_KEY, &serde_json::to_string(self).map_err(err)?).map_err(err)
    }

    pub(crate) fn has(&self, computer: &str, file: &str, version: &str) -> bool {
        self.files.get(&format!("{computer}/{file}")).is_some_and(|v| v == version)
    }

    pub(crate) fn note(&mut self, computer: &str, file: &str, version: &str) {
        self.files.insert(format!("{computer}/{file}"), version.to_string());
    }
}

/// Writes this computer's changed days and reads the others' changed ones.
/// True when another computer's choices came in, which the caller passes on to what keeps them in
/// memory.
pub fn run_once(conn: &Connection, now_ms: i64) -> Result<bool> {
    let Some(root) = folder(conn)? else {
        if relay::group(conn)?.is_some() {
            return run_relay(conn, std::sync::Arc::new(relay::Http::default()), now_ms);
        }
        return Ok(false);
    };
    if !root.is_dir() {
        return Err(format!("{} isn't there; is the drive connected?", root.display()));
    }
    run_with(conn, &store::FolderStore::new(&root, &device_id(conn)?), now_ms)
}

/// One round through `store`. Choices go first, so another computer's recording rules apply to
/// its time. A failed upload doesn't keep the others' time out: everything is read in, and then
/// the upload's error is returned, so a full relay or a file too big says so rather than stopping
/// all combining.
pub fn run_with(conn: &Connection, store: &dyn Store, now_ms: i64) -> Result<bool> {
    let me = device_id(conn)?;
    seed_choices(conn)?;
    let listing = store.list()?;
    let mine = listing.get(&me).cloned().unwrap_or_default();
    let mut seen = Seen::load(conn)?;
    let wrote_choices = write_choices(conn, store, &mine);
    let changed = read_choices(conn, store, &listing, &me, &mut seen)?;
    let wrote_days = write_own(conn, store, &mine, now_ms);
    read_others(conn, store, &listing, &me, now_ms)?;
    note_computers(conn, store, &listing, &me, &mut seen, now_ms)?;
    // Like the days, a note that fails to go up doesn't keep the rest from coming in.
    let notes = crate::notebook::sync(conn, store, &listing, &me, &mut seen, now_ms);
    seen.save(conn)?;
    wrote_choices.and(wrote_days)?;
    Ok(changed | notes?)
}

/// What Settings shows of combining that a round can change: whether this computer combines, and
/// what went wrong. Who's combined is left out; it's read when a window opens.
/// After a wake, the rounds that follow come quickly: the other computer may not have put
/// anything up yet when the first one looks, as when both sync the moment they link.
const QUICK_ROUNDS: [Duration; 2] = [Duration::from_secs(30), Duration::from_secs(60)];

/// How long to wait for the next round, after `quick_rounds_done` quick rounds since the last wake.
fn wait_after(quick_rounds_done: usize) -> Duration {
    QUICK_ROUNDS.get(quick_rounds_done).copied().unwrap_or(EVERY)
}

/// What open windows show of combining: whether it's on, why it last failed, and who's combined.
fn shown_state(conn: &Connection) -> (bool, Option<String>, Vec<String>) {
    (
        combining(conn).unwrap_or(false),
        relay_status(conn).map(|(_, error)| error).unwrap_or(None),
        computers(conn).unwrap_or_default().into_iter().map(|c| c.id).collect(),
    )
}

/// Every few minutes, while Kairo runs, and at once whenever something sends on `wake` (linking,
/// joining, starting over). Choices that come in reach the collector's recording rules and the
/// nudges at once; `on_woken` runs after each round a wake asked for, and after any round that
/// changed what Settings shows, as when another computer started over, so open windows can show it.
pub fn spawn(
    db_path: PathBuf,
    rules: privacy::SharedRules,
    cache: crate::nudge::SharedCache,
    wake: std::sync::mpsc::Receiver<()>,
    on_woken: impl Fn() + Send + 'static,
) {
    use std::sync::mpsc::RecvTimeoutError;
    thread::Builder::new()
        .name("kairo-sync".into())
        .spawn(move || {
            let mut woken = wake.recv_timeout(FIRST_AFTER).is_ok();
            let mut quick_rounds_done = if woken { 0 } else { QUICK_ROUNDS.len() };
            let conn = match db::open(&db_path) {
                Ok(conn) => conn,
                Err(e) => return eprintln!("sync: cannot open the database: {e}"),
            };
            loop {
                let before = shown_state(&conn);
                match run_once(&conn, chrono::Utc::now().timestamp_millis()) {
                    Ok(true) => {
                        if let Err(e) = privacy::refresh(&conn, &rules) {
                            eprintln!("sync: {e}");
                        }
                        crate::nudge::invalidate(&cache);
                    }
                    Ok(false) => {}
                    Err(e) => {
                        eprintln!("sync: {e}");
                        // Choices may have come in before an upload failed, or everyone's time
                        // gone when the group did.
                        if let Err(e) = privacy::refresh(&conn, &rules) {
                            eprintln!("sync: {e}");
                        }
                        crate::nudge::invalidate(&cache);
                    }
                }
                if woken || shown_state(&conn) != before {
                    on_woken();
                }
                let wait = wait_after(quick_rounds_done);
                quick_rounds_done = quick_rounds_done.saturating_add(1);
                woken = match wake.recv_timeout(wait) {
                    Ok(()) => {
                        while wake.try_recv().is_ok() {}
                        quick_rounds_done = 0;
                        true
                    }
                    Err(RecvTimeoutError::Timeout) => false,
                    Err(RecvTimeoutError::Disconnected) => {
                        thread::sleep(EVERY);
                        false
                    }
                };
            }
        })
        .expect("failed to start sync thread");
}

// ---------- shared choices ----------

const CHOICES_FILE: &str = "choices.json";
const SEEDED_KEY: &str = "choices_seeded";

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
struct Choice {
    value: String,
    changed_ms: i64,
}

/// Notes a choice the user made, so the other computers take it: `item` names it, like
/// `category:app:code`, `limit:site:youtube`, `goal:productive`, `focus:app:steam` or
/// `privacy:site:bank`, and `value` is what it became.
pub fn record(conn: &Connection, item: &str, value: &str) -> Result<()> {
    record_at(conn, item, value, chrono::Utc::now().timestamp_millis())
}

/// Stops sharing a choice whose thing is gone here, like a deleted task's plan.
pub fn drop_choice(conn: &Connection, item: &str) -> Result<()> {
    conn.execute("DELETE FROM shared_choices WHERE item = ?1", [item]).map_err(err)?;
    Ok(())
}

fn record_at(conn: &Connection, item: &str, value: &str, changed_ms: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO shared_choices (item, value, changed_ms) VALUES (?1, ?2, ?3)
         ON CONFLICT(item) DO UPDATE SET value = excluded.value, changed_ms = excluded.changed_ms",
        params![item, value, changed_ms],
    )
    .map_err(err)?;
    Ok(())
}

/// Choices made before they were shared count as the oldest of all, so they reach computers that
/// haven't chosen but never undo a choice made since. Once.
fn seed_choices(conn: &Connection) -> Result<()> {
    if db::get_setting(conn, SEEDED_KEY).map_err(err)?.is_some() {
        return Ok(());
    }
    conn.execute_batch(
        "INSERT OR IGNORE INTO shared_choices (item, value, changed_ms)
             SELECT 'category:' || key, category, 1 FROM categories WHERE user_set = 1;
         INSERT OR IGNORE INTO shared_choices (item, value, changed_ms)
             SELECT 'limit:' || key, minutes, 1 FROM limits;
         INSERT OR IGNORE INTO shared_choices (item, value, changed_ms)
             SELECT 'hardlimit:' || key, '1', 1 FROM limits WHERE hard = 1;
         INSERT OR IGNORE INTO shared_choices (item, value, changed_ms)
             SELECT 'focus:' || key, rule, 1 FROM focus_rules;
         INSERT OR IGNORE INTO shared_choices (item, value, changed_ms)
             SELECT 'privacy:' || key, mode, 1 FROM privacy_rules WHERE user_set = 1;
         INSERT OR IGNORE INTO shared_choices (item, value, changed_ms)
             SELECT 'goal:productive', value, 1 FROM settings WHERE key = 'goal_productive_min';
         INSERT OR IGNORE INTO shared_choices (item, value, changed_ms)
             SELECT 'goal:distracting', value, 1 FROM settings WHERE key = 'goal_distracting_max_min';
         INSERT OR IGNORE INTO shared_choices (item, value, changed_ms)
             SELECT 'goal:week-productive', value, 1 FROM settings WHERE key = 'goal_week_productive_min';
         INSERT OR IGNORE INTO shared_choices (item, value, changed_ms)
             SELECT 'goal:week-distracting', value, 1 FROM settings WHERE key = 'goal_week_distracting_max_min';",
    )
    .map_err(err)?;
    if !crate::projects::list(conn).map_err(err)?.is_empty() {
        let projects = crate::projects::to_json(conn).map_err(err)?;
        conn.execute("INSERT OR IGNORE INTO shared_choices (item, value, changed_ms) VALUES ('projects:all', ?1, 1)", [projects]).map_err(err)?;
    }
    for n in crate::notes::all(conn).map_err(err)? {
        conn.execute(
            "INSERT OR IGNORE INTO shared_choices (item, value, changed_ms) VALUES (?1, ?2, 1)",
            [format!("notes:{}", n.monday), crate::notes::to_json(&n.note)],
        )
        .map_err(err)?;
    }
    if !crate::lists::list(conn).map_err(err)?.is_empty() {
        let lists = crate::lists::to_json(conn).map_err(err)?;
        conn.execute("INSERT OR IGNORE INTO shared_choices (item, value, changed_ms) VALUES ('lists:block', ?1, 1)", [lists]).map_err(err)?;
    }
    if !crate::tasks::list(conn, 0, "").map_err(err)?.is_empty() {
        let tasks = crate::tasks::to_json(conn).map_err(err)?;
        conn.execute("INSERT OR IGNORE INTO shared_choices (item, value, changed_ms) VALUES ('tasks:all', ?1, 1)", [tasks]).map_err(err)?;
    }
    for (id, plan) in crate::tasks::planned(conn).map_err(err)? {
        conn.execute("INSERT OR IGNORE INTO shared_choices (item, value, changed_ms) VALUES (?1, ?2, 1)", [format!("taskplan:{id}"), plan]).map_err(err)?;
    }
    if !crate::schedule::list(conn).map_err(err)?.is_empty() {
        let schedule = crate::schedule::to_json(conn).map_err(err)?;
        conn.execute("INSERT OR IGNORE INTO shared_choices (item, value, changed_ms) VALUES ('schedule:focus', ?1, 1)", [schedule]).map_err(err)?;
    }
    db::set_setting(conn, SEEDED_KEY, "1").map_err(err)
}

fn choices(conn: &Connection) -> Result<std::collections::BTreeMap<String, Choice>> {
    let mut stmt = conn.prepare("SELECT item, value, changed_ms FROM shared_choices").map_err(err)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, Choice { value: r.get(1)?, changed_ms: r.get(2)? })))
        .map_err(err)?
        .collect::<rusqlite::Result<_>>()
        .map_err(err);
    rows
}

const CHOICES_WRITTEN_KEY: &str = "sync_choices_written";

/// Writes this computer's choices when they differ from what it last wrote, or aren't there.
fn write_choices(conn: &Connection, store: &dyn Store, mine: &HashMap<String, String>) -> Result<()> {
    let text = serde_json::to_string_pretty(&choices(conn)?).map_err(err)?;
    if mine.contains_key(CHOICES_FILE) && db::get_setting(conn, CHOICES_WRITTEN_KEY).map_err(err)?.as_deref() == Some(text.as_str()) {
        return Ok(());
    }
    store.put(CHOICES_FILE, text.as_bytes())?;
    db::set_setting(conn, CHOICES_WRITTEN_KEY, &text).map_err(err)
}

/// Takes the other computers' choices made after this one's, from those whose choices changed
/// since they were last read; true when any came in.
fn read_choices(conn: &Connection, store: &dyn Store, listing: &store::Listing, me: &str, seen: &mut Seen) -> Result<bool> {
    let mut ours = choices(conn)?;
    let mut changed = false;
    for (computer, files) in listing {
        let Some(version) = files.get(CHOICES_FILE).filter(|_| computer != me) else { continue };
        if seen.has(computer, CHOICES_FILE, version) {
            continue;
        }
        let Ok(text) = store.get(computer, CHOICES_FILE) else { continue };
        let Ok(theirs) = serde_json::from_slice::<HashMap<String, Choice>>(&text) else { continue };
        seen.note(computer, CHOICES_FILE, version);
        // A limit before whether it blocks, and the task list before the tasks' plans and notes:
        // each needs the other to be there.
        let mut theirs: Vec<(String, Choice)> = theirs.into_iter().collect();
        theirs.sort_by_key(|(item, _)| (item.starts_with("hardlimit:") || item.starts_with("taskplan:") || item.starts_with("tasknote:"), item.clone()));
        for (item, choice) in theirs {
            if ours.get(&item).is_some_and(|o| o.changed_ms >= choice.changed_ms) {
                continue;
            }
            // A kind of choice from a newer Kairo, or a value this one doesn't know, waits.
            if apply(conn, &item, &choice.value).is_ok() {
                record_at(conn, &item, &choice.value, choice.changed_ms)?;
                ours.insert(item, choice);
                changed = true;
            }
        }
    }
    Ok(changed)
}

/// Makes another computer's choice here.
fn apply(conn: &Connection, item: &str, value: &str) -> Result<()> {
    use crate::{analysis, blocking, goals};
    let (kind, key) = item.split_once(':').ok_or("not a choice")?;
    let minutes = || value.parse::<u32>().map_err(err);
    match kind {
        "category" => analysis::set_category(conn, key, analysis::Category::parse(value).ok_or("unknown category")?).map_err(err),
        "limit" => goals::set_limit(conn, key, minutes()?).map_err(err),
        "hardlimit" => goals::set_hard(conn, key, value == "1").map_err(err),
        "goal" => match goals::set_named_goal(conn, key, minutes()?).map_err(err)? {
            true => Ok(()),
            false => Err("unknown goal".into()),
        },
        "focus" => blocking::set_rule(conn, key, blocking::Rule::parse(value).ok_or("unknown focus rule")?).map_err(err),
        "privacy" => privacy::set_rule(conn, key, privacy::Mode::parse(value).ok_or("unknown recording rule")?).map_err(err),
        "schedule" if key == "focus" => crate::schedule::replace_from_json(conn, value),
        "projects" if key == "all" => crate::projects::replace_from_json(conn, value),
        "lists" if key == "block" => crate::lists::replace_from_json(conn, value),
        "tasks" if key == "all" => crate::tasks::replace_from_json(conn, value),
        // A plan whose task hasn't arrived yet waits for it.
        "taskplan" => match crate::tasks::apply_plan(conn, key, &serde_json::from_str(value).map_err(err)?).map_err(err)? {
            true => Ok(()),
            false => Err("no such task yet".into()),
        },
        "ledger" => crate::ledger::apply(conn, key, value),
        // Which note a task is attached to (empty for none); waits for its task like a plan.
        "tasknote" => match crate::tasks::set_note(conn, key, Some(value)).map_err(err)? {
            true => Ok(()),
            false => Err("no such task yet".into()),
        },
        "notes" => crate::notes::set_from_json(conn, key, value),
        _ => Err("unknown choice".into()),
    }
}

/// Writes the days whose sessions changed since they were last written, and takes away the files
/// of days whose history was deleted.
fn write_own(conn: &Connection, store: &dyn Store, mine: &HashMap<String, String>, now_ms: i64) -> Result<()> {
    let mut stmt = conn
        .prepare(
            "SELECT start_ms / ?1, COUNT(*), SUM(start_ms), SUM(end_ms), SUM(length(title) + length(app) + length(ifnull(site, '')))
             FROM sessions WHERE device IS NULL AND demo = 0 GROUP BY 1",
        )
        .map_err(err)?;
    let now: HashMap<i64, String> = stmt
        .query_map([DAY_MS], |r| Ok((r.get::<_, i64>(0)?, format!("{}:{}:{}:{}", r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?))))
        .map_err(err)?
        .collect::<rusqlite::Result<_>>()
        .map_err(err)?;
    let written: HashMap<i64, String> = conn
        .prepare("SELECT day, signature FROM sync_written")
        .map_err(err)?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(err)?
        .collect::<rusqlite::Result<_>>()
        .map_err(err)?;
    for (day, signature) in &now {
        if written.get(day) != Some(signature) || !mine.contains_key(&day_name(*day)) {
            store.put(&day_name(*day), &day_bytes(conn, *day)?)?;
            conn.execute("INSERT OR REPLACE INTO sync_written (day, signature) VALUES (?1, ?2)", params![day, signature]).map_err(err)?;
        }
    }
    for day in written.keys().filter(|d| !now.contains_key(d)) {
        let _ = store.delete(&day_name(*day));
        conn.execute("DELETE FROM sync_written WHERE day = ?1", [day]).map_err(err)?;
    }
    // Its name, when that changed or isn't there, and otherwise once an hour, which is how
    // Settings on the others tells it's still about.
    let about = About { name: device_name(), updated_ms: now_ms };
    let last: Option<About> = db::get_setting(conn, ABOUT_WRITTEN_KEY).map_err(err)?.and_then(|t| serde_json::from_str(&t).ok());
    if mine.contains_key(ABOUT_FILE) && last.is_some_and(|l| l.name == about.name && now_ms - l.updated_ms < ABOUT_EVERY_MS) {
        return Ok(());
    }
    let text = serde_json::to_string(&about).map_err(err)?;
    store.put(ABOUT_FILE, text.as_bytes())?;
    db::set_setting(conn, ABOUT_WRITTEN_KEY, &text).map_err(err)
}

const ABOUT_WRITTEN_KEY: &str = "sync_about_written";
const ABOUT_EVERY_MS: i64 = 60 * 60 * 1000;

fn day_name(day: i64) -> String {
    format!("{day}.{DAY_FILE}")
}

/// A file in the temporary folder, removed when dropped.
struct TempFile(PathBuf);

impl TempFile {
    fn new(what: &str) -> Self {
        let path = std::env::temp_dir().join(format!("kairo-{what}-{}-{}.db", std::process::id(), new_id()));
        let _ = std::fs::remove_file(&path);
        Self(path)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// One day's sessions in a small database of its own.
fn day_bytes(conn: &Connection, day: i64) -> Result<Vec<u8>> {
    let tmp = TempFile::new("day-out");
    conn.execute("ATTACH DATABASE ?1 AS day_out", [tmp.0.to_string_lossy()]).map_err(err)?;
    let result = conn
        .execute_batch(
            "CREATE TABLE day_out.sessions (
                 app TEXT NOT NULL, exe TEXT NOT NULL, title TEXT NOT NULL, site TEXT,
                 start_ms INTEGER NOT NULL, end_ms INTEGER NOT NULL, idle INTEGER NOT NULL
             )",
        )
        .and_then(|_| {
            conn.execute(
                "INSERT INTO day_out.sessions SELECT app, exe, title, site, start_ms, end_ms, idle FROM main.sessions
                 WHERE device IS NULL AND demo = 0 AND start_ms >= ?1 AND start_ms < ?2",
                params![day * DAY_MS, (day + 1) * DAY_MS],
            )
        })
        .map_err(err);
    let _ = conn.execute("DETACH DATABASE day_out", []);
    result?;
    std::fs::read(&tmp.0).map_err(err)
}

/// Reads in the other computers' days that changed since last time, and forgets days, and
/// computers, that went away.
fn read_others(conn: &Connection, store: &dyn Store, listing: &store::Listing, me: &str, now_ms: i64) -> Result<()> {
    let rules = privacy::load(conn).map_err(err)?;
    // Days older than history is kept for aren't brought back.
    let oldest_day = match privacy::retention_days(conn).map_err(err)? {
        0 => i64::MIN,
        days => (now_ms - days as i64 * DAY_MS) / DAY_MS,
    };
    let mut present = HashSet::new();
    for (device, names) in listing.iter().filter(|(d, _)| *d != me) {
        present.insert(device.clone());
        let files: HashMap<i64, &String> = names
            .iter()
            .filter_map(|(name, version)| Some((name.strip_suffix(&format!(".{DAY_FILE}"))?.parse().ok()?, version)))
            .collect();
        let known = imported(conn, device)?;
        for (day, stamp) in &files {
            if *day < oldest_day || known.get(day) == Some(*stamp) {
                continue;
            }
            // A file the drive is still bringing in can fail to open; it's tried again next time.
            let bytes = store.get(device, &day_name(*day));
            if bytes.and_then(|b| import_day(conn, device, *day, &b, &rules)).is_ok() {
                conn.execute("INSERT OR REPLACE INTO sync_imports (device, day, stamp) VALUES (?1, ?2, ?3)", params![device, day, stamp]).map_err(err)?;
            }
        }
        for day in known.keys().filter(|d| !files.contains_key(d)) {
            forget(conn, device, Some(*day))?;
        }
    }
    let gone: Vec<String> = conn
        .prepare("SELECT DISTINCT device FROM sync_imports")
        .map_err(err)?
        .query_map([], |r| r.get(0))
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<String>>>()
        .map_err(err)?
        .into_iter()
        .filter(|d| !present.contains(d))
        .collect();
    for device in gone {
        forget(conn, &device, None)?;
    }
    Ok(())
}

fn imported(conn: &Connection, device: &str) -> Result<HashMap<i64, String>> {
    let mut stmt = conn.prepare("SELECT day, stamp FROM sync_imports WHERE device = ?1").map_err(err)?;
    let rows = stmt.query_map([device], |r| Ok((r.get(0)?, r.get(1)?))).map_err(err)?.collect::<rusqlite::Result<_>>().map_err(err);
    rows
}

/// Takes away a computer's time for one day, or all of it.
fn forget(conn: &Connection, device: &str, day: Option<i64>) -> Result<()> {
    let (from, to) = day.map_or((i64::MIN, i64::MAX), |d| (d * DAY_MS, (d + 1) * DAY_MS));
    conn.execute("DELETE FROM sessions WHERE device = ?1 AND start_ms >= ?2 AND start_ms < ?3", params![device, from, to]).map_err(err)?;
    match day {
        Some(day) => conn.execute("DELETE FROM sync_imports WHERE device = ?1 AND day = ?2", params![device, day]),
        None => conn.execute("DELETE FROM sync_imports WHERE device = ?1", [device]),
    }
    .map_err(err)?;
    Ok(())
}

/// Replaces what's here of a computer's day with its file, keeping to this computer's privacy
/// rules. The bytes go into a file of their own first, so the drive is never kept from updating
/// the original.
fn import_day(conn: &Connection, device: &str, day: i64, bytes: &[u8], rules: &privacy::Rules) -> Result<()> {
    let copy = TempFile::new("import");
    std::fs::write(&copy.0, bytes).map_err(err)?;
    let rows = read_day(&copy.0)?;
    let tx = conn.unchecked_transaction().map_err(err)?;
    tx.execute("DELETE FROM sessions WHERE device = ?1 AND start_ms >= ?2 AND start_ms < ?3", params![device, day * DAY_MS, (day + 1) * DAY_MS])
        .map_err(err)?;
    for (app, exe, title, site, start, end, idle) in rows {
        let title = match privacy::resolve(rules, &exe, site.as_deref()) {
            privacy::Mode::Ignore => continue,
            privacy::Mode::HideTitle => String::new(),
            privacy::Mode::Track => title,
        };
        tx.execute(
            "INSERT INTO sessions (app, exe, title, site, start_ms, end_ms, idle, demo, device) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
            params![app, exe, title, site, start, end, idle, device],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)
}

type DayRow = (String, String, String, Option<String>, i64, i64, bool);

fn read_day(path: &Path) -> Result<Vec<DayRow>> {
    let day = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(err)?;
    let mut stmt = day.prepare("SELECT app, exe, title, site, start_ms, end_ms, idle FROM sessions").map_err(err)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))
        .map_err(err)?
        .collect::<rusqlite::Result<_>>()
        .map_err(err);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Computer {
        conn: Connection,
        file: PathBuf,
    }

    impl Computer {
        fn new(name: &str) -> Self {
            let file = std::env::temp_dir().join(format!("kairo-sync-{}-{name}.db", std::process::id()));
            let _ = std::fs::remove_file(&file);
            Self { conn: db::open(&file).unwrap(), file }
        }

        fn track(&self, app: &str, start: i64) -> i64 {
            let s = db::NewSession { app, exe: app, title: "", site: None, start_ms: start, end_ms: start + 60_000, idle: false, demo: false };
            db::insert_session(&self.conn, &s).unwrap()
        }

        fn apps(&self) -> Vec<(String, bool)> {
            self.conn
                .prepare("SELECT app, device IS NOT NULL FROM sessions ORDER BY start_ms")
                .unwrap()
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        }
    }

    impl Drop for Computer {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.file);
        }
    }

    #[test]
    fn computers_add_up_through_a_shared_folder() {
        let folder = std::env::temp_dir().join(format!("kairo-sync-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let (desk, laptop) = (Computer::new("desk"), Computer::new("laptop"));
        let now = 20_000 * DAY_MS;

        let code = desk.track("code", now - DAY_MS);
        desk.track("discord", now);
        laptop.track("figma", now + 1_000);
        start(&desk.conn, &folder, now).unwrap();
        start(&laptop.conn, &folder, now).unwrap();
        start(&desk.conn, &folder, now).unwrap();

        assert_eq!(laptop.apps(), [("code".into(), true), ("discord".into(), true), ("figma".into(), false)]);
        assert_eq!(desk.apps(), [("code".into(), false), ("discord".into(), false), ("figma".into(), true)]);
        assert_eq!(computers(&laptop.conn).unwrap().len(), 2);
        assert!(computers(&laptop.conn).unwrap()[0].this_one);

        // The views can show every computer, this one, or another.
        let seen = |computer: &str| -> Vec<String> {
            crate::analysis::load_rows_of(&laptop.conn, 0, i64::MAX, computer).unwrap().into_iter().map(|r| r.app).collect()
        };
        assert_eq!(seen("all"), ["code", "discord", "figma"]);
        assert_eq!(seen("this"), ["figma"]);
        assert_eq!(seen(&device_id(&desk.conn).unwrap()), ["code", "discord"]);
        set_shown(&laptop.conn, "this").unwrap();
        assert_eq!(shown(&laptop.conn).unwrap(), "this");

        // History deleted on one computer leaves the others too, and only changed days are written.
        desk.conn.execute("DELETE FROM sessions WHERE id = ?1", [code]).unwrap();
        run_once(&desk.conn, now).unwrap();
        run_once(&laptop.conn, now).unwrap();
        assert_eq!(laptop.apps(), [("discord".into(), true), ("figma".into(), false)]);

        // A computer moving to another folder leaves the old one, and the others there drop it.
        let elsewhere = folder.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        start(&desk.conn, &elsewhere, now).unwrap();
        assert!(!folder.join(DIR).join(device_id(&desk.conn).unwrap()).exists());
        run_once(&laptop.conn, now).unwrap();
        assert_eq!(laptop.apps(), [("figma".into(), false)]);
        start(&desk.conn, &folder, now).unwrap();
        run_once(&laptop.conn, now).unwrap();
        assert_eq!(laptop.apps(), [("discord".into(), true), ("figma".into(), false)]);

        // A computer that stops combining takes its time away from the rest.
        stop(&desk.conn).unwrap();
        run_once(&laptop.conn, now).unwrap();
        assert_eq!(laptop.apps(), [("figma".into(), false)]);
        assert_eq!(desk.apps(), [("discord".into(), false)]);

        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn the_latest_choice_wins_on_every_computer() {
        use crate::{analysis, goals};
        let folder = std::env::temp_dir().join(format!("kairo-sync-choices-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let (desk, laptop) = (Computer::new("desk-c"), Computer::new("laptop-c"));
        let now = 20_000 * DAY_MS;
        let category = |c: &Computer| analysis::category_map(&c.conn).unwrap().get("site:reddit").copied();

        // Chosen on the desk before combining: it reaches the laptop, which hadn't chosen.
        analysis::set_category(&desk.conn, "site:reddit", analysis::Category::Distracting).unwrap();
        goals::set_limit(&desk.conn, "site:reddit", 30).unwrap();
        start(&desk.conn, &folder, now).unwrap();
        assert!(start(&laptop.conn, &folder, now).unwrap());
        assert_eq!(category(&laptop), Some(analysis::Category::Distracting));
        assert_eq!(goals::load(&laptop.conn).unwrap().limits, [("site:reddit".to_string(), 30)]);

        // Changed on the laptop later: the desk takes it, and an older change doesn't undo it.
        analysis::set_category(&laptop.conn, "site:reddit", analysis::Category::Neutral).unwrap();
        record_at(&laptop.conn, "category:site:reddit", "neutral", 2_000).unwrap();
        record_at(&desk.conn, "goal:productive", "120", 1_500).unwrap();
        run_once(&laptop.conn, now).unwrap();
        assert!(run_once(&desk.conn, now).unwrap());
        assert_eq!(category(&desk), Some(analysis::Category::Neutral));
        assert_eq!(goals::load(&laptop.conn).unwrap().productive_min, 0);
        run_once(&laptop.conn, now).unwrap();
        assert_eq!(goals::load(&laptop.conn).unwrap().productive_min, 120);
        record_at(&desk.conn, "category:site:reddit", "productive", 1_000).unwrap();
        run_once(&desk.conn, now).unwrap();
        run_once(&laptop.conn, now).unwrap();
        assert_eq!(category(&laptop), Some(analysis::Category::Neutral));
        // A choice from a newer Kairo is left for later rather than failing.
        assert!(apply(&laptop.conn, "wallpaper:blue", "x").is_err());
        // The task list travels whole, ids and all.
        let task = crate::tasks::add(&desk.conn, "Essay", 1).unwrap().unwrap();
        apply(&laptop.conn, "tasks:all", &crate::tasks::to_json(&desk.conn).unwrap()).unwrap();
        assert_eq!(crate::tasks::list(&laptop.conn, 2, "").unwrap().iter().map(|t| &t.id).collect::<Vec<_>>(), [&task.id]);

        let _ = std::fs::remove_dir_all(folder);
    }

    /// Desk and laptop, and the views each has of one shared store.
    fn sharing(name: &str) -> (Computer, Computer, store::MemStore, store::MemStore) {
        let shared = store::MemStore::default();
        let (desk, laptop) = (Computer::new(&format!("desk-{name}")), Computer::new(&format!("laptop-{name}")));
        let (a, b) = (shared.for_computer(&device_id(&desk.conn).unwrap()), shared.for_computer(&device_id(&laptop.conn).unwrap()));
        (desk, laptop, a, b)
    }

    fn note_titles(c: &Computer) -> Vec<String> {
        let mut t: Vec<String> = crate::notebook::list(&c.conn, "").unwrap().into_iter().map(|n| n.title).collect();
        t.sort();
        t
    }

    #[test]
    fn a_new_note_reaches_the_other_computer_and_the_newest_change_wins() {
        let (desk, laptop, desk_store, laptop_store) = sharing("notes");
        let now = 20_000 * DAY_MS;
        let n = crate::notebook::create(&desk.conn, "# Plan\nintro", now).unwrap();
        run_with(&desk.conn, &desk_store, now).unwrap();
        assert!(run_with(&laptop.conn, &laptop_store, now).unwrap());
        assert_eq!(crate::notebook::get(&laptop.conn, &n.id).unwrap().unwrap().body, "# Plan\nintro");
        // The laptop edits it; the desk takes that.
        crate::notebook::save(&laptop.conn, &n.id, "# Plan\nintro\nbody", now + 10).unwrap();
        run_with(&laptop.conn, &laptop_store, now + 10).unwrap();
        run_with(&desk.conn, &desk_store, now + 10).unwrap();
        assert_eq!(crate::notebook::get(&desk.conn, &n.id).unwrap().unwrap().body, "# Plan\nintro\nbody");
        // Nothing new: a round changes nothing.
        assert!(!run_with(&desk.conn, &desk_store, now + 20).unwrap());
        assert_eq!(note_titles(&desk), ["Plan"]);
    }

    #[test]
    fn an_unsynced_edit_meeting_a_newer_one_becomes_a_conflicted_copy() {
        let (desk, laptop, desk_store, laptop_store) = sharing("clash");
        let now = 20_000 * DAY_MS;
        let n = crate::notebook::create(&desk.conn, "Plan", now).unwrap();
        run_with(&desk.conn, &desk_store, now).unwrap();
        run_with(&laptop.conn, &laptop_store, now).unwrap();
        // Both edit before either syncs; the laptop's is later.
        crate::notebook::save(&desk.conn, &n.id, "Plan\ndesk's", now + 5).unwrap();
        crate::notebook::save(&laptop.conn, &n.id, "Plan\nlaptop's", now + 9).unwrap();
        run_with(&laptop.conn, &laptop_store, now + 9).unwrap();
        run_with(&desk.conn, &desk_store, now + 10).unwrap();
        run_with(&laptop.conn, &laptop_store, now + 11).unwrap();
        run_with(&desk.conn, &desk_store, now + 12).unwrap();
        let want = ["Plan", "Plan (conflicted copy)"];
        assert_eq!(note_titles(&desk), want);
        assert_eq!(note_titles(&laptop), want);
        assert_eq!(crate::notebook::get(&desk.conn, &n.id).unwrap().unwrap().body, "Plan\nlaptop's");
        let copy = crate::notebook::list(&laptop.conn, "conflicted").unwrap();
        assert_eq!(copy[0].body, "Plan (conflicted copy)\ndesk's");
    }

    #[test]
    fn a_deletion_spreads_and_stays() {
        let (desk, laptop, desk_store, laptop_store) = sharing("gone");
        let now = 20_000 * DAY_MS;
        let n = crate::notebook::create(&desk.conn, "Plan", now).unwrap();
        run_with(&desk.conn, &desk_store, now).unwrap();
        run_with(&laptop.conn, &laptop_store, now).unwrap();
        crate::notebook::remove(&laptop.conn, &n.id, now + 5).unwrap();
        run_with(&laptop.conn, &laptop_store, now + 5).unwrap();
        run_with(&desk.conn, &desk_store, now + 6).unwrap();
        assert!(note_titles(&desk).is_empty());
        // The desk's own, older copy doesn't bring it back on either side.
        run_with(&laptop.conn, &laptop_store, now + 7).unwrap();
        run_with(&desk.conn, &desk_store, now + 8).unwrap();
        assert!(note_titles(&desk).is_empty() && note_titles(&laptop).is_empty());
    }

    #[test]
    fn a_computer_linked_later_gets_every_note() {
        let shared = store::MemStore::default();
        let (desk, laptop, late) = (Computer::new("desk-late"), Computer::new("laptop-late"), Computer::new("third-late"));
        let st = |c: &Computer| shared.for_computer(&device_id(&c.conn).unwrap());
        let now = 20_000 * DAY_MS;
        crate::notebook::create(&desk.conn, "Desk's", now).unwrap();
        crate::notebook::create(&laptop.conn, "Laptop's", now).unwrap();
        run_with(&desk.conn, &st(&desk), now).unwrap();
        run_with(&laptop.conn, &st(&laptop), now).unwrap();
        run_with(&desk.conn, &st(&desk), now).unwrap();
        // The laptop leaves; its note lives on in the desk's copy.
        st(&laptop).remove_me().unwrap();
        run_with(&late.conn, &st(&late), now).unwrap();
        assert_eq!(note_titles(&late), ["Desk's", "Laptop's"]);
    }

    #[test]
    fn deletion_markers_go_after_thirty_days() {
        let (desk, _laptop, desk_store, _) = sharing("purge");
        let now = 20_000 * DAY_MS;
        let n = crate::notebook::create(&desk.conn, "Plan", now).unwrap();
        crate::notebook::remove(&desk.conn, &n.id, now).unwrap();
        run_with(&desk.conn, &desk_store, now).unwrap();
        let file = format!("note-{}.json", n.id);
        let me = device_id(&desk.conn).unwrap();
        assert!(desk_store.list().unwrap()[&me].contains_key(&file));
        run_with(&desk.conn, &desk_store, now + 31 * DAY_MS).unwrap();
        assert!(!desk_store.list().unwrap()[&me].contains_key(&file));
        let left: i64 = desk.conn.query_row("SELECT COUNT(*) FROM notebook", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 0);
    }

    #[test]
    fn equal_changes_pick_the_same_winner_everywhere() {
        let (desk, laptop, desk_store, laptop_store) = sharing("tie");
        let now = 20_000 * DAY_MS;
        let n = crate::notebook::create(&desk.conn, "Plan", now).unwrap();
        run_with(&desk.conn, &desk_store, now).unwrap();
        run_with(&laptop.conn, &laptop_store, now).unwrap();
        crate::notebook::save(&desk.conn, &n.id, "Plan\naaa", now + 5).unwrap();
        crate::notebook::save(&laptop.conn, &n.id, "Plan\nzzz", now + 5).unwrap();
        for t in 6..10 {
            run_with(&desk.conn, &desk_store, now + t).unwrap();
            run_with(&laptop.conn, &laptop_store, now + t).unwrap();
        }
        let body = |c: &Computer| crate::notebook::get(&c.conn, &n.id).unwrap().unwrap().body;
        assert_eq!(body(&desk), body(&laptop));
        assert_eq!(note_titles(&desk), note_titles(&laptop));
    }

    #[test]
    fn a_tasks_note_travels_and_waits_for_its_task() {
        let laptop = Computer::new("laptop-tasknote");
        assert!(apply(&laptop.conn, "tasknote:abc", "n1").is_err());
        laptop.conn.execute("INSERT INTO tasks (id, title, created_ms, position) VALUES ('abc', 'Essay', 1, 0)", []).unwrap();
        apply(&laptop.conn, "tasknote:abc", "n1").unwrap();
        assert_eq!(crate::tasks::list(&laptop.conn, 0, "").unwrap()[0].note.as_deref(), Some("n1"));
        apply(&laptop.conn, "tasknote:abc", "").unwrap();
        assert_eq!(crate::tasks::list(&laptop.conn, 0, "").unwrap()[0].note, None);
    }

    #[test]
    fn a_tasks_plan_travels_with_it() {
        let (desk, laptop, desk_store, laptop_store) = sharing("plan");
        let now = 20_000 * DAY_MS;
        let t = crate::tasks::add(&desk.conn, "Essay", 1).unwrap().unwrap();
        crate::tasks::set_plan(&desk.conn, &t.id, Some("2026-11-07"), 2, Some(77)).unwrap();
        record(&desk.conn, "tasks:all", &crate::tasks::to_json(&desk.conn).unwrap()).unwrap();
        record(&desk.conn, &format!("taskplan:{}", t.id), &crate::tasks::plan_json(&desk.conn, &t.id).unwrap().unwrap()).unwrap();
        run_with(&desk.conn, &desk_store, now).unwrap();
        // Both arrive in one round; the list goes first, so the plan has its task.
        run_with(&laptop.conn, &laptop_store, now).unwrap();
        let p = crate::tasks::plan(&laptop.conn, &t.id).unwrap().unwrap();
        assert_eq!((p.due.as_deref(), p.priority, p.remind), (Some("2026-11-07"), 2, Some(77)));
    }

    #[test]
    fn a_plan_for_a_task_not_here_yet_waits_for_it() {
        let laptop = Computer::new("laptop-early");
        let plan = r#"{"due":"2026-11-07","priority":2,"remind":null,"sent":""}"#;
        assert!(apply(&laptop.conn, "taskplan:abc", plan).is_err());
        assert!(apply(&laptop.conn, "taskplan:abc", "not json").is_err());
        laptop.conn.execute("INSERT INTO tasks (id, title, created_ms, position) VALUES ('abc', 'Essay', 1, 0)", []).unwrap();
        apply(&laptop.conn, "taskplan:abc", plan).unwrap();
        assert_eq!(crate::tasks::plan(&laptop.conn, "abc").unwrap().unwrap().priority, 2);
    }

    #[test]
    fn seeding_shares_the_plans_tasks_already_have() {
        let desk = Computer::new("desk-seedplan");
        let t = crate::tasks::add(&desk.conn, "Essay", 1).unwrap().unwrap();
        let plain = crate::tasks::add(&desk.conn, "Plain", 1).unwrap().unwrap();
        crate::tasks::set_plan(&desk.conn, &t.id, Some("2026-11-07"), 2, None).unwrap();
        seed_choices(&desk.conn).unwrap();
        let all = choices(&desk.conn).unwrap();
        assert!(all.contains_key(&format!("taskplan:{}", t.id)));
        // One left as it came needs nothing shared.
        assert!(!all.contains_key(&format!("taskplan:{}", plain.id)));
    }

    #[test]
    fn ledgers_settings_travel() {
        let laptop = Computer::new("laptop-ledger");
        apply(&laptop.conn, "ledger:reminders", "0").unwrap();
        apply(&laptop.conn, "ledger:morning", "07:30").unwrap();
        assert_eq!(crate::ledger::settings(&laptop.conn).unwrap(), crate::ledger::Settings { reminders: false, morning: "07:30".into() });
        assert!(apply(&laptop.conn, "ledger:morning", "25:00").is_err());
        assert!(apply(&laptop.conn, "ledger:colour", "red").is_err());
    }

    #[test]
    fn combines_through_any_store() {
        use crate::analysis;
        let shared = store::MemStore::default();
        let (desk, laptop) = (Computer::new("desk-s"), Computer::new("laptop-s"));
        let (desk_store, laptop_store) =
            (shared.for_computer(&device_id(&desk.conn).unwrap()), shared.for_computer(&device_id(&laptop.conn).unwrap()));
        let now = 20_000 * DAY_MS;

        let code = desk.track("code", now - DAY_MS);
        desk.track("discord", now);
        laptop.track("figma", now + 1_000);
        analysis::set_category(&desk.conn, "site:reddit", analysis::Category::Distracting).unwrap();
        record_at(&desk.conn, "category:site:reddit", "distracting", 5).unwrap();
        run_with(&desk.conn, &desk_store, now).unwrap();
        assert!(run_with(&laptop.conn, &laptop_store, now).unwrap());
        run_with(&desk.conn, &desk_store, now).unwrap();

        assert_eq!(laptop.apps(), [("code".into(), true), ("discord".into(), true), ("figma".into(), false)]);
        assert_eq!(desk.apps(), [("code".into(), false), ("discord".into(), false), ("figma".into(), true)]);
        assert_eq!(analysis::category_map(&laptop.conn).unwrap().get("site:reddit").copied(), Some(analysis::Category::Distracting));
        let listed = computers(&laptop.conn).unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed[0].this_one);

        // A deleted day leaves the others.
        desk.conn.execute("DELETE FROM sessions WHERE id = ?1", [code]).unwrap();
        run_with(&desk.conn, &desk_store, now).unwrap();
        run_with(&laptop.conn, &laptop_store, now).unwrap();
        assert_eq!(laptop.apps(), [("discord".into(), true), ("figma".into(), false)]);

        // A computer that leaves takes its time with it.
        desk_store.remove_me().unwrap();
        run_with(&laptop.conn, &laptop_store, now).unwrap();
        assert_eq!(laptop.apps(), [("figma".into(), false)]);
        assert_eq!(computers(&laptop.conn).unwrap().len(), 1);
    }

    /// Two computers in one relay group.
    fn linked(desk: &Computer, laptop: &Computer) -> std::sync::Arc<relay::MemApi> {
        let api = std::sync::Arc::new(relay::MemApi::default());
        let group = relay::new_group();
        relay::register(api.as_ref(), &group).unwrap();
        relay::set_group(&desk.conn, Some(&group)).unwrap();
        relay::set_group(&laptop.conn, Some(&group)).unwrap();
        api
    }

    #[test]
    fn combines_through_the_relay() {
        let (desk, laptop) = (Computer::new("desk-r"), Computer::new("laptop-r"));
        let api = linked(&desk, &laptop);
        let now = 20_000 * DAY_MS;
        let code = desk.track("code", now - DAY_MS);
        desk.track("discord", now);
        laptop.track("figma", now + 1_000);
        assert!(combining(&desk.conn).unwrap());

        run_relay(&desk.conn, api.clone(), now).unwrap();
        run_relay(&laptop.conn, api.clone(), now).unwrap();
        run_relay(&desk.conn, api.clone(), now).unwrap();
        assert_eq!(laptop.apps(), [("code".into(), true), ("discord".into(), true), ("figma".into(), false)]);
        assert_eq!(desk.apps(), [("code".into(), false), ("discord".into(), false), ("figma".into(), true)]);
        assert_eq!(computers(&desk.conn).unwrap().len(), 2);
        assert_eq!(db::get_setting(&desk.conn, "relay_synced_ms").unwrap().as_deref(), Some(now.to_string().as_str()));

        desk.conn.execute("DELETE FROM sessions WHERE id = ?1", [code]).unwrap();
        run_relay(&desk.conn, api.clone(), now).unwrap();
        run_relay(&laptop.conn, api.clone(), now).unwrap();
        assert_eq!(laptop.apps(), [("discord".into(), true), ("figma".into(), false)]);

        // Leaving takes this computer's files off the relay, and the others drop its time.
        leave_relay(&desk.conn, api.clone()).unwrap();
        assert!(!combining(&desk.conn).unwrap());
        assert_eq!(desk.apps(), [("discord".into(), false)]);
        run_relay(&laptop.conn, api.clone(), now).unwrap();
        assert_eq!(laptop.apps(), [("figma".into(), false)]);
    }

    #[test]
    fn a_computer_turning_up_changes_what_is_shown() {
        let (desk, laptop) = (Computer::new("desk-up"), Computer::new("laptop-up"));
        let api = linked(&desk, &laptop);
        let now = 20_000 * DAY_MS;
        // The desk's first round comes before the laptop has put anything up.
        run_relay(&desk.conn, api.clone(), now).unwrap();
        let alone = shown_state(&desk.conn);
        run_relay(&laptop.conn, api.clone(), now).unwrap();
        run_relay(&desk.conn, api.clone(), now + 1_000).unwrap();
        assert_ne!(shown_state(&desk.conn), alone);
        // Nothing new, so nothing to show again.
        let both = shown_state(&desk.conn);
        run_relay(&desk.conn, api.clone(), now + 2_000).unwrap();
        assert_eq!(shown_state(&desk.conn), both);
    }

    #[test]
    fn rounds_come_quickly_after_a_wake() {
        assert_eq!(wait_after(0), Duration::from_secs(30));
        assert_eq!(wait_after(1), Duration::from_secs(60));
        assert_eq!(wait_after(2), EVERY);
        assert_eq!(wait_after(10), EVERY);
    }

    #[test]
    fn a_round_with_nothing_new_only_lists_what_changed() {
        let (desk, laptop) = (Computer::new("desk-q"), Computer::new("laptop-q"));
        let api = linked(&desk, &laptop);
        let now = 20_000 * DAY_MS;
        desk.track("code", now);
        laptop.track("figma", now);
        record_at(&desk.conn, "goal:productive", "120", 5).unwrap();
        for _ in 0..2 {
            run_relay(&desk.conn, api.clone(), now).unwrap();
            run_relay(&laptop.conn, api.clone(), now).unwrap();
        }
        assert_eq!(computers(&laptop.conn).unwrap().len(), 2);
        api.reset_counts();
        run_relay(&laptop.conn, api.clone(), now + 60_000).unwrap();
        assert_eq!((api.count("list"), api.count("get"), api.count("put")), (1, 0, 0));
        // The computers are still both known, and what changes still comes in.
        assert_eq!(computers(&laptop.conn).unwrap().len(), 2);
        desk.track("discord", now + 120_000);
        record_at(&desk.conn, "goal:productive", "90", 6).unwrap();
        run_relay(&desk.conn, api.clone(), now + 120_000).unwrap();
        api.reset_counts();
        run_relay(&laptop.conn, api.clone(), now + 180_000).unwrap();
        assert_eq!(api.count("list"), 1);
        assert_eq!(api.count("get"), 2, "the changed day and the changed choices");
        assert!(laptop.apps().contains(&("discord".into(), true)));
        assert_eq!(db::get_setting(&laptop.conn, "goal_productive_min").unwrap().as_deref(), Some("90"));
    }

    #[test]
    fn this_computers_name_is_written_again_within_the_hour() {
        let (desk, laptop) = (Computer::new("desk-h1"), Computer::new("laptop-h1"));
        let api = linked(&desk, &laptop);
        let now = 20_000 * DAY_MS;
        run_relay(&desk.conn, api.clone(), now).unwrap();
        api.reset_counts();
        run_relay(&desk.conn, api.clone(), now + 30 * 60_000).unwrap();
        assert_eq!(api.count("put"), 0);
        run_relay(&desk.conn, api.clone(), now + 61 * 60_000).unwrap();
        assert_eq!(api.count("put"), 1, "computer.json, so the others see it's still about");
    }

    #[test]
    fn relay_sees_no_names_or_contents() {
        let (desk, laptop) = (Computer::new("desk-n"), Computer::new("laptop-n"));
        let api = linked(&desk, &laptop);
        let now = 20_000 * DAY_MS;
        desk.track("discord", now);
        crate::analysis::set_category(&desk.conn, "site:reddit", crate::analysis::Category::Distracting).unwrap();
        record_at(&desk.conn, "category:site:reddit", "distracting", 5).unwrap();
        run_relay(&desk.conn, api.clone(), now).unwrap();
        laptop.track("discord", now);
        run_relay(&laptop.conn, api.clone(), now).unwrap();
        let stored = api.stored();
        assert!(stored.len() >= 6);
        // The same file on two computers doesn't share a name, so the relay can't line up which
        // days they both tracked.
        let names: HashSet<&String> = stored.iter().map(|(n, _)| n).collect();
        assert_eq!(names.len(), stored.len());
        for (name, bytes) in stored {
            assert!(!name.contains("choices") && !name.contains("kairo-day") && !name.contains("computer"), "{name}");
            let text = String::from_utf8_lossy(&bytes);
            assert!(!text.contains("discord") && !text.contains("reddit") && !text.contains("SQLite"));
        }
    }

    #[test]
    fn gone_group_stops_combining() {
        let (desk, laptop) = (Computer::new("desk-g"), Computer::new("laptop-g"));
        let api = linked(&desk, &laptop);
        let now = 20_000 * DAY_MS;
        desk.track("discord", now);
        run_relay(&desk.conn, api.clone(), now).unwrap();
        run_relay(&laptop.conn, api.clone(), now).unwrap();
        assert_eq!(laptop.apps(), [("discord".into(), true)]);

        // Another computer started over: the old group is gone from the relay.
        api.forget_groups();
        assert!(run_relay(&laptop.conn, api.clone(), now).is_err());
        assert!(!combining(&laptop.conn).unwrap());
        assert!(laptop.apps().is_empty());
        assert_eq!(db::get_setting(&laptop.conn, "relay_error").unwrap().as_deref(), Some(UNLINKED));
    }

    #[test]
    fn what_settings_shows_changes_when_the_group_goes() {
        let (desk, laptop) = (Computer::new("desk-v"), Computer::new("laptop-v"));
        let api = linked(&desk, &laptop);
        let now = 20_000 * DAY_MS;
        run_relay(&laptop.conn, api.clone(), now).unwrap();
        let before = shown_state(&laptop.conn);
        run_relay(&laptop.conn, api.clone(), now).unwrap();
        assert_eq!(shown_state(&laptop.conn), before, "a quiet round changes nothing shown");
        api.forget_groups();
        let _ = run_relay(&laptop.conn, api.clone(), now);
        assert_ne!(shown_state(&laptop.conn), before);
    }

    #[test]
    fn a_round_still_running_after_leaving_puts_nothing_back() {
        let (desk, laptop) = (Computer::new("desk-l"), Computer::new("laptop-l"));
        let api = linked(&desk, &laptop);
        let now = 20_000 * DAY_MS;
        let group = relay::group(&desk.conn).unwrap().unwrap();
        laptop.track("figma", now);
        run_relay(&laptop.conn, api.clone(), now).unwrap();
        // The sync thread's round began before Stop combining, with the group it read then.
        let round = relay::RelayStore::new(api.clone(), &group, &device_id(&desk.conn).unwrap());
        desk.track("discord", now);
        leave_relay(&desk.conn, api.clone()).unwrap();
        assert!(round.put(&day_name(now / DAY_MS), b"late").is_err());
        assert!(run_relay_in(&desk.conn, api.clone(), &group, now).is_ok());
        // Nothing of the desk's reaches the laptop, and nothing of the laptop's stays on the desk.
        run_relay(&laptop.conn, api.clone(), now).unwrap();
        assert_eq!(laptop.apps(), [("figma".into(), false)]);
        assert_eq!(desk.apps(), [("discord".into(), false)]);
    }

    #[test]
    fn starting_over_while_a_round_runs_keeps_the_new_group() {
        let (desk, laptop) = (Computer::new("desk-x"), Computer::new("laptop-x"));
        let api = linked(&desk, &laptop);
        let now = 20_000 * DAY_MS;
        let old = relay::group(&desk.conn).unwrap().unwrap();
        // Start over, as link::host does it, while a round with the old group is still out.
        relay::retire(&old.id, &device_id(&desk.conn).unwrap());
        relay::delete_group(api.as_ref(), &old).unwrap();
        let new = relay::new_group();
        relay::register(api.as_ref(), &new).unwrap();
        join_relay(&desk.conn, &new).unwrap();
        assert!(run_relay_in(&desk.conn, api.clone(), &old, now).is_ok());
        assert_eq!(relay::group(&desk.conn).unwrap(), Some(new));
        assert_ne!(db::get_setting(&desk.conn, "relay_error").unwrap().as_deref(), Some(UNLINKED));
    }

    #[test]
    fn a_group_gone_mid_round_is_dropped_only_if_it_is_still_the_one_kept() {
        let (desk, laptop) = (Computer::new("desk-k"), Computer::new("laptop-k"));
        let api = linked(&desk, &laptop);
        let now = 20_000 * DAY_MS;
        let old = relay::group(&desk.conn).unwrap().unwrap();
        api.forget_groups();
        let new = relay::new_group();
        relay::register(api.as_ref(), &new).unwrap();
        join_relay(&desk.conn, &new).unwrap();
        let _ = run_relay_in(&desk.conn, api.clone(), &old, now);
        assert_eq!(relay::group(&desk.conn).unwrap(), Some(new));
        assert_ne!(db::get_setting(&desk.conn, "relay_error").unwrap().as_deref(), Some(UNLINKED));
    }

    #[test]
    fn rejoining_a_group_left_before_writes_again() {
        let (desk, laptop) = (Computer::new("desk-j"), Computer::new("laptop-j"));
        let api = linked(&desk, &laptop);
        let now = 20_000 * DAY_MS;
        let group = relay::group(&desk.conn).unwrap().unwrap();
        leave_relay(&desk.conn, api.clone()).unwrap();
        join_relay(&desk.conn, &group).unwrap();
        desk.track("code", now);
        run_relay(&desk.conn, api.clone(), now).unwrap();
        run_relay(&laptop.conn, api.clone(), now).unwrap();
        assert_eq!(laptop.apps(), [("code".into(), true)]);
    }

    /// A store whose uploads all fail, as when the relay's group is full.
    struct NoUploads<'a>(&'a dyn Store);

    impl Store for NoUploads<'_> {
        fn list(&self) -> Result<store::Listing> {
            self.0.list()
        }
        fn get(&self, computer: &str, name: &str) -> Result<Vec<u8>> {
            self.0.get(computer, name)
        }
        fn put(&self, _: &str, _: &[u8]) -> Result<()> {
            Err("full".into())
        }
        fn delete(&self, name: &str) -> Result<()> {
            self.0.delete(name)
        }
        fn remove_me(&self) -> Result<()> {
            self.0.remove_me()
        }
    }

    #[test]
    fn a_failed_upload_still_lets_the_others_time_in() {
        let (desk, laptop) = (Computer::new("desk-u"), Computer::new("laptop-u"));
        let api = linked(&desk, &laptop);
        let now = 20_000 * DAY_MS;
        desk.track("discord", now);
        run_relay(&desk.conn, api.clone(), now).unwrap();
        laptop.track("figma", now + 1_000);
        let group = relay::group(&laptop.conn).unwrap().unwrap();
        let store = relay::RelayStore::new(api.clone(), &group, &device_id(&laptop.conn).unwrap());
        assert_eq!(run_with(&laptop.conn, &NoUploads(&store), now), Err("full".into()));
        assert_eq!(laptop.apps(), [("discord".into(), true), ("figma".into(), false)]);
    }

    /// Links two computers with a code and combines them through a real relay: start one with
    /// `cd relay && npx wrangler d1 migrations apply kairo-relay --local && npx wrangler dev`, then
    /// `KAIRO_RELAY=http://127.0.0.1:8787 cargo test -- --ignored through_a_real_relay`.
    #[test]
    #[ignore]
    fn links_and_combines_through_a_real_relay() {
        use std::sync::Arc;
        let http = Arc::new(relay::Http::default());
        let (desk, laptop) = (Computer::new("desk-h"), Computer::new("laptop-h"));
        let now = chrono::Utc::now().timestamp_millis();

        let group = relay::new_group();
        relay::register(http.as_ref(), &group).unwrap();
        let mut host = pair::Host::start(http.clone(), &group, "Desk", 0).unwrap();
        let code = host.code.clone();
        let hosting = thread::spawn(move || loop {
            match host.step(1) {
                pair::PairState::Waiting => thread::sleep(Duration::from_millis(200)),
                done => return done,
            }
        });
        let invite = pair::join(http.as_ref(), &code, "Laptop", Duration::from_secs(20)).unwrap();
        assert_eq!(hosting.join().unwrap(), pair::PairState::Linked { name: "Laptop".into() });
        assert_eq!(invite.group, group);

        join_relay(&desk.conn, &group).unwrap();
        join_relay(&laptop.conn, &invite.group).unwrap();
        desk.track("code", now);
        run_relay(&desk.conn, http.clone(), now).unwrap();
        run_relay(&laptop.conn, http.clone(), now).unwrap();
        assert_eq!(laptop.apps(), [("code".into(), true)]);

        leave_relay(&desk.conn, http.clone()).unwrap();
        run_relay(&laptop.conn, http.clone(), now).unwrap();
        assert!(laptop.apps().is_empty());
        relay::delete_group(http.as_ref(), &group).unwrap();
    }

    #[test]
    fn other_computers_time_keeps_to_this_ones_privacy_rules() {
        let folder = std::env::temp_dir().join(format!("kairo-sync-privacy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let (desk, laptop) = (Computer::new("desk-p"), Computer::new("laptop-p"));
        let now = 20_000 * DAY_MS;
        desk.track("banking", now);
        desk.track("code", now + 1_000);
        privacy::set_rule(&laptop.conn, "app:banking", privacy::Mode::Ignore).unwrap();
        start(&desk.conn, &folder, now).unwrap();
        start(&laptop.conn, &folder, now).unwrap();
        assert_eq!(laptop.apps(), [("code".into(), true)]);
        let _ = std::fs::remove_dir_all(folder);
    }
}
