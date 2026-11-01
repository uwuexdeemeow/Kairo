//! Linking from Kairo's settings: showing a code (and starting over), typing one, and what's
//! shown while a code waits. Everything here may reach the relay, so it runs on threads of its
//! own with connections of its own, never under the app's database lock.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::pair::{self, Mailbox, PairState};
use super::relay::{self, Api};
use super::{err, Result};
use crate::{db, sync};

/// A code being shown, as Settings sees it.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Linking {
    pub code: String,
    pub expires_ms: i64,
    #[serde(flatten)]
    pub state: PairState,
}

/// The code being shown, if any, and how to cancel it.
#[derive(Clone, Default)]
pub struct Shared(Arc<Mutex<Option<Showing>>>);

/// A code being shown, and the flag that cancels it.
type Showing = (Linking, Arc<AtomicBool>);

impl Shared {
    pub fn status(&self) -> Option<Linking> {
        self.0.lock().ok()?.as_ref().map(|(l, _)| l.clone())
    }

    pub fn cancel(&self) {
        if let Ok(mut current) = self.0.lock() {
            if let Some((linking, cancelled)) = current.as_mut() {
                cancelled.store(true, Ordering::SeqCst);
                if linking.state == PairState::Waiting {
                    linking.state = PairState::Failed { message: "Cancelled.".into() };
                }
            }
        }
    }

    fn set_state(&self, state: PairState) {
        if let Ok(mut current) = self.0.lock() {
            if let Some((linking, _)) = current.as_mut() {
                linking.state = state;
            }
        }
    }
}

/// Shows a code another computer can type. A computer not yet combining makes a group for it,
/// and joins it once the other computer has. `start_over` deletes this computer's group from the
/// relay first and starts a new one with this computer alone, so a lost computer is left out.
/// `on_linked` runs when the other computer has linked.
pub fn host<R: Api + Mailbox + 'static>(
    shared: &Shared,
    relay: Arc<R>,
    db_path: &Path,
    start_over: bool,
    poll: Duration,
    on_linked: impl Fn() + Send + 'static,
) -> Result<Linking> {
    let conn = db::open(db_path).map_err(err)?;
    if sync::folder(&conn)?.is_some() {
        return Err("Stop combining through the folder first.".into());
    }
    let group = match relay::group(&conn)? {
        Some(group) if !start_over => group,
        old => {
            if let Some(old) = old.filter(|_| start_over) {
                relay::delete_group(relay.as_ref(), &old)?;
            }
            let group = relay::new_group();
            relay::register(relay.as_ref(), &group)?;
            if start_over {
                sync::join_relay(&conn, &group)?;
            }
            group
        }
    };
    shared.cancel();
    let now = chrono::Utc::now().timestamp_millis();
    let mut code = pair::Host::start(relay.clone(), &group, &sync::device_name(), now)?;
    let linking = Linking { code: code.code.clone(), expires_ms: code.expires_ms, state: PairState::Waiting };
    let cancelled = Arc::new(AtomicBool::new(false));
    *shared.0.lock().map_err(err)? = Some((linking.clone(), cancelled.clone()));

    let (shared, db_path) = (shared.clone(), db_path.to_path_buf());
    std::thread::Builder::new()
        .name("kairo-link".into())
        .spawn(move || loop {
            std::thread::sleep(poll);
            if cancelled.load(Ordering::SeqCst) {
                code.cancel();
                return;
            }
            let state = code.step(chrono::Utc::now().timestamp_millis());
            if let PairState::Linked { .. } = state {
                // The other computer is in; this one joins the group if it wasn't in it yet.
                let joined = db::open(&db_path).map_err(err).and_then(|conn| match relay::group(&conn)? {
                    Some(g) if g == group => Ok(()),
                    _ => sync::join_relay(&conn, &group),
                });
                if let Err(e) = joined {
                    shared.set_state(PairState::Failed { message: e });
                    return;
                }
                on_linked();
            }
            if cancelled.load(Ordering::SeqCst) {
                return;
            }
            let done = state != PairState::Waiting;
            shared.set_state(state);
            if done {
                return;
            }
        })
        .map_err(err)?;
    Ok(linking)
}

const NO_MATCH: &str = "That code didn't match, or it's been used. Make a new one on the other computer.";

/// Links this computer with a code another one shows; the other computer's name.
pub fn join<R: Api + Mailbox>(relay: &R, db_path: &Path, code: &str) -> Result<String> {
    join_waiting(relay, db_path, code, Duration::from_secs(30))
}

fn join_waiting<R: Api + Mailbox>(relay: &R, db_path: &Path, code: &str, wait: Duration) -> Result<String> {
    let conn = db::open(db_path).map_err(err)?;
    if sync::combining(&conn)? {
        return Err("This computer is already combining. Stop combining first.".into());
    }
    match pair::join(relay, code, &sync::device_name(), wait) {
        Ok(invite) => {
            sync::join_relay(&conn, &invite.group)?;
            Ok(invite.from)
        }
        Err(pair::JoinError::BadCode | pair::JoinError::Mismatch) => Err(NO_MATCH.into()),
        Err(pair::JoinError::Expired) => Err("That code ran out.".into()),
        Err(pair::JoinError::Unreachable(_)) => Err("Can't reach Kairo's relay. Check the internet connection.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Computer {
        path: PathBuf,
    }

    impl Computer {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("kairo-link-{}-{name}.db", std::process::id()));
            let _ = std::fs::remove_file(&path);
            db::open(&path).unwrap();
            Self { path }
        }
        fn conn(&self) -> rusqlite::Connection {
            db::open(&self.path).unwrap()
        }
    }

    impl Drop for Computer {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    const FAST: Duration = Duration::from_millis(5);

    fn wait_until_done(shared: &Shared) -> PairState {
        for _ in 0..1000 {
            match shared.status().map(|l| l.state) {
                Some(PairState::Waiting) | None => std::thread::sleep(FAST),
                Some(done) => return done,
            }
        }
        panic!("the code never finished");
    }

    #[test]
    fn a_code_links_two_computers_into_one_group() {
        let relay = Arc::new(relay::MemApi::default());
        let (desk, laptop) = (Computer::new("desk"), Computer::new("laptop"));
        let shared = Shared::default();
        let linked = Arc::new(AtomicBool::new(false));
        let flag = linked.clone();
        let shown = host(&shared, relay.clone(), &desk.path, false, FAST, move || flag.store(true, Ordering::SeqCst)).unwrap();
        assert_eq!(shown.state, PairState::Waiting);
        assert_eq!(shared.status().unwrap().code, shown.code);
        // Not combining until the other computer has linked.
        assert!(!sync::combining(&desk.conn()).unwrap());

        let from = join_waiting(relay.as_ref(), &laptop.path, &shown.code, Duration::from_secs(5)).unwrap();
        assert_eq!(from, sync::device_name());
        assert!(matches!(wait_until_done(&shared), PairState::Linked { .. }));
        assert!(linked.load(Ordering::SeqCst));
        let (d, l) = (relay::group(&desk.conn()).unwrap().unwrap(), relay::group(&laptop.conn()).unwrap().unwrap());
        assert_eq!(d, l);

        // A linked computer can't type another code until it stops.
        assert!(join_waiting(relay.as_ref(), &laptop.path, "K7QX-49MP", FAST).is_err());
    }

    #[test]
    fn a_wrong_code_says_so() {
        let relay = Arc::new(relay::MemApi::default());
        let (desk, laptop) = (Computer::new("desk-w"), Computer::new("laptop-w"));
        let shared = Shared::default();
        let shown = host(&shared, relay.clone(), &desk.path, false, FAST, || {}).unwrap();
        let (slot, password) = pair::parse_code(&shown.code).unwrap();
        let wrong = pair::format_code(&slot, if password == "000000" { "111111" } else { "000000" });
        let said = join_waiting(relay.as_ref(), &laptop.path, &wrong, Duration::from_secs(5)).unwrap_err();
        assert_eq!(said, "That code didn't match, or it's been used. Make a new one on the other computer.");
        assert_eq!(wait_until_done(&shared), PairState::Mismatch);
        assert!(!sync::combining(&laptop.conn()).unwrap());
        assert_eq!(join_waiting(relay.as_ref(), &laptop.path, "??", FAST).unwrap_err(), "That code didn't match, or it's been used. Make a new one on the other computer.");
    }

    #[test]
    fn cancelling_stops_the_code() {
        let relay = Arc::new(relay::MemApi::default());
        let desk = Computer::new("desk-c");
        let shared = Shared::default();
        host(&shared, relay.clone(), &desk.path, false, FAST, || {}).unwrap();
        shared.cancel();
        assert!(matches!(wait_until_done(&shared), PairState::Failed { .. }));
        std::thread::sleep(FAST * 4);
        assert!(relay.slots.lock().unwrap().is_empty());
    }

    #[test]
    fn starting_over_leaves_the_old_group_behind() {
        let relay = Arc::new(relay::MemApi::default());
        let (desk, laptop) = (Computer::new("desk-o"), Computer::new("laptop-o"));
        let shared = Shared::default();
        let shown = host(&shared, relay.clone(), &desk.path, false, FAST, || {}).unwrap();
        join_waiting(relay.as_ref(), &laptop.path, &shown.code, Duration::from_secs(5)).unwrap();
        wait_until_done(&shared);
        let old = relay::group(&desk.conn()).unwrap().unwrap();

        let again = host(&shared, relay.clone(), &desk.path, true, FAST, || {}).unwrap();
        assert_eq!(again.state, PairState::Waiting);
        let new = relay::group(&desk.conn()).unwrap().unwrap();
        assert_ne!(new, old);
        // This computer keeps combining, alone, in the new group; the laptop's next sync finds
        // its group gone.
        assert!(sync::combining(&desk.conn()).unwrap());
        let now = 20_000 * 24 * 60 * 60 * 1000;
        assert!(sync::run_relay(&laptop.conn(), relay.clone(), now).is_err());
        assert!(!sync::combining(&laptop.conn()).unwrap());
    }

    #[test]
    fn a_folder_has_to_stop_first() {
        let relay = Arc::new(relay::MemApi::default());
        let desk = Computer::new("desk-f");
        db::set_setting(&desk.conn(), "sync_folder", "C:/somewhere").unwrap();
        let said = host(&Shared::default(), relay.clone(), &desk.path, false, FAST, || {}).unwrap_err();
        assert_eq!(said, "Stop combining through the folder first.");
    }
}
