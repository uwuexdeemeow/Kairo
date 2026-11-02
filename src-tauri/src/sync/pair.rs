//! Linking a computer with a code. The computer showing the code (the host) and the one typing it
//! run SPAKE2 with the code's last six characters as the password, through a slot on the relay
//! named by its first two; the relay sees neither the password nor the key they agree on. With
//! that key the host seals an invitation (the group and its key) and the other computer seals its
//! reply (its name). A wrong code gives each side a different key, so the invitation won't open;
//! the slot takes one answer, so a code allows one guess. Each side also gives the relay the hash
//! of a secret of its own, so only the host can write the invitation or close the slot, and only
//! the computer that answered can reply. A message whose answer was lost on the way is checked for
//! in the slot before it's sent again, so a dropped connection doesn't spend the code.

use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use serde::{Deserialize, Serialize};
use spake2::{Ed25519Group, Identity, Password, Spake2};

use super::relay::{ApiError, Group};
use super::seal;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// How long a code works.
pub const CODE_MS: i64 = 10 * 60 * 1000;

/// The messages swapped in a slot, those sent so far.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parts {
    pub a: Option<Vec<u8>>,
    pub b: Option<Vec<u8>>,
    pub invite: Option<Vec<u8>>,
    pub reply: Option<Vec<u8>>,
}

/// Where the two computers leave messages for each other: the relay's pairing slots.
pub trait Mailbox: Send + Sync {
    /// A new slot holding the host's message, kept for the host whose secret this is; its
    /// two-character name.
    fn open(&self, a: &[u8], secret: &str) -> Result<String, ApiError>;
    /// `Gone` once the slot has run out or was closed.
    fn read(&self, slot: &str) -> Result<Parts, ApiError>;
    /// `part` is "b", "invite" or "reply", in that order; each can be written once. `secret` is
    /// the writer's own: the answer ("b") leaves its hash, the invitation and reply prove it.
    fn write(&self, slot: &str, part: &str, bytes: &[u8], secret: &str) -> Result<(), ApiError>;
    fn close(&self, slot: &str, secret: &str);
}

/// A secret of this computer's own for one pairing.
fn new_secret() -> String {
    seal::random::<32>().iter().map(|b| format!("{b:02x}")).collect()
}

/// Writes a part, and when the write seems to fail, looks whether it got there anyway, as when
/// only the relay's answer was lost; then it counts as written. When that look can't reach the
/// relay either, it's left unknown, and unreachable, so the caller tries again: a write refused
/// as already written may well be this one's own.
fn deliver(mailbox: &dyn Mailbox, slot: &str, part: &str, bytes: &[u8], secret: &str) -> Result<(), ApiError> {
    let Err(e) = mailbox.write(slot, part, bytes, secret) else { return Ok(()) };
    let arrived = match mailbox.read(slot) {
        Ok(parts) => match part {
            "b" => parts.b,
            "invite" => parts.invite,
            _ => parts.reply,
        },
        Err(dropped @ ApiError::Unreachable(_)) => return Err(dropped),
        Err(_) => None,
    };
    if arrived.as_deref() == Some(bytes) {
        Ok(())
    } else {
        Err(e)
    }
}

/// `deliver`, again and again while the relay can't be reached, until `deadline`.
fn deliver_by(mailbox: &dyn Mailbox, slot: &str, part: &str, bytes: &[u8], secret: &str, deadline: Instant, poll: Duration) -> Result<(), ApiError> {
    loop {
        match deliver(mailbox, slot, part, bytes, secret) {
            Err(ApiError::Unreachable(_)) if Instant::now() < deadline => std::thread::sleep(poll),
            other => return other,
        }
    }
}

/// Crockford's base32: no I, L, O or U, so nothing reads as something else.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const IDENTITY: &[u8] = b"kairo-pair-v1";
/// What the computer typing the code replies when the invitation won't open, so the host can say
/// so rather than wait out the code.
const NO_MATCH: &[u8] = b"mismatch";

pub fn new_password() -> String {
    seal::random::<6>().iter().map(|b| ALPHABET[(b % 32) as usize] as char).collect()
}

pub fn format_code(slot: &str, password: &str) -> String {
    let all = format!("{slot}{password}");
    format!("{}-{}", &all[..4], &all[4..])
}

/// The slot and password in what someone typed, forgiving case, spaces, dashes and letters that
/// look like digits; None when it can't be a code.
pub fn parse_code(typed: &str) -> Option<(String, String)> {
    let code: String = typed
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        })
        .collect();
    if code.len() != 8 || !code.bytes().all(|b| ALPHABET.contains(&b)) {
        return None;
    }
    Some((code[..2].to_string(), code[2..].to_string()))
}

fn spake(password: &str) -> (Spake2<Ed25519Group>, Vec<u8>) {
    Spake2::<Ed25519Group>::start_symmetric(&Password::new(password.as_bytes()), &Identity::new(IDENTITY))
}

#[derive(Serialize, Deserialize)]
struct InviteBody {
    group: String,
    key: String,
    from: String,
}

#[derive(Serialize, Deserialize)]
struct ReplyBody {
    name: String,
}

/// What the computer typing the code receives.
#[derive(Clone, Debug, PartialEq)]
pub struct Invite {
    pub group: Group,
    /// The host's name.
    pub from: String,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PairState {
    Waiting,
    Linked { name: String },
    Expired,
    Mismatch,
    Failed { message: String },
}

/// The computer showing the code.
pub struct Host {
    pub code: String,
    pub expires_ms: i64,
    mailbox: Arc<dyn Mailbox>,
    slot: String,
    secret: String,
    spake: Option<Spake2<Ed25519Group>>,
    key: Option<[u8; 32]>,
    /// The sealed invitation, until the relay has it.
    unsent: Option<Vec<u8>>,
    group: Group,
    my_name: String,
    state: PairState,
}

impl Host {
    pub fn start(mailbox: Arc<dyn Mailbox>, group: &Group, my_name: &str, now_ms: i64) -> Result<Host, ApiError> {
        let password = new_password();
        let (spake, message) = spake(&password);
        let secret = new_secret();
        let slot = mailbox.open(&message, &secret)?;
        Ok(Host {
            code: format_code(&slot, &password),
            expires_ms: now_ms + CODE_MS,
            mailbox,
            slot,
            secret,
            spake: Some(spake),
            key: None,
            unsent: None,
            group: group.clone(),
            my_name: my_name.to_string(),
            state: PairState::Waiting,
        })
    }

    fn finish(&mut self, state: PairState) -> PairState {
        self.mailbox.close(&self.slot, &self.secret);
        self.state = state.clone();
        state
    }

    /// Checks the slot once and moves on as far as it can.
    pub fn step(&mut self, now_ms: i64) -> PairState {
        if self.state != PairState::Waiting {
            return self.state.clone();
        }
        if now_ms >= self.expires_ms {
            return self.finish(PairState::Expired);
        }
        let parts = match self.mailbox.read(&self.slot) {
            Ok(parts) => parts,
            Err(ApiError::Gone) => return self.finish(PairState::Expired),
            // Offline for a moment: tried again at the next step.
            Err(_) => return PairState::Waiting,
        };
        if self.key.is_none() {
            let Some(b) = parts.b.as_ref() else { return PairState::Waiting };
            let Some(spake) = self.spake.take() else { return self.finish(PairState::Mismatch) };
            let Ok(shared) = spake.finish(b) else { return self.finish(PairState::Mismatch) };
            let key = seal::invite_key(&shared);
            let body = InviteBody { group: self.group.id.clone(), key: B64.encode(self.group.key), from: self.my_name.clone() };
            self.unsent = Some(seal::seal_with(&key, &format!("invite/{}", self.slot), &serde_json::to_vec(&body).expect("plain data")));
            self.key = Some(key);
        }
        if let Some(sealed) = self.unsent.clone() {
            match deliver(self.mailbox.as_ref(), &self.slot, "invite", &sealed, &self.secret) {
                Ok(()) => self.unsent = None,
                // Offline for a moment: sent again at the next step.
                Err(ApiError::Unreachable(_)) => return PairState::Waiting,
                Err(e) => return self.finish(PairState::Failed { message: e.to_string() }),
            }
            return PairState::Waiting;
        }
        let (Some(reply), Some(key)) = (parts.reply.as_ref(), self.key.as_ref()) else { return PairState::Waiting };
        match seal::open_with(key, &format!("reply/{}", self.slot), reply).ok().and_then(|r| serde_json::from_slice::<ReplyBody>(&r).ok()) {
            Some(reply) => self.finish(PairState::Linked { name: reply.name }),
            None => self.finish(PairState::Mismatch),
        }
    }

    pub fn cancel(&mut self) {
        if self.state == PairState::Waiting {
            self.finish(PairState::Failed { message: "Cancelled.".into() });
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum JoinError {
    /// Not something that could be a code.
    BadCode,
    /// The wrong code, or one already used.
    Mismatch,
    /// The code ran out before the other computer answered.
    Expired,
    Unreachable(String),
}

/// Links this computer with the code another one shows. Waits up to `wait` for its invitation.
pub fn join(mailbox: &dyn Mailbox, code: &str, my_name: &str, wait: Duration) -> Result<Invite, JoinError> {
    join_polling(mailbox, code, my_name, wait, Duration::from_secs(1))
}

/// How long the reply keeps being sent through a dropping connection, once the invitation is in.
const REPLY_WAIT: Duration = Duration::from_secs(20);

fn unreachable(e: ApiError) -> JoinError {
    match e {
        ApiError::Unreachable(m) => JoinError::Unreachable(m),
        ApiError::Gone => JoinError::Expired,
        ApiError::Other(m) => JoinError::Unreachable(m),
    }
}

fn join_polling(mailbox: &dyn Mailbox, code: &str, my_name: &str, wait: Duration, poll: Duration) -> Result<Invite, JoinError> {
    let (slot, password) = parse_code(code).ok_or(JoinError::BadCode)?;
    // No such slot: most likely a mistyped code, or one long gone.
    let parts = match mailbox.read(&slot) {
        Err(ApiError::Gone) => return Err(JoinError::Mismatch),
        other => other.map_err(unreachable)?,
    };
    let Some(a) = parts.a.filter(|_| parts.b.is_none()) else { return Err(JoinError::Mismatch) };
    let (spake, message) = spake(&password);
    let shared = spake.finish(&a).map_err(|_| JoinError::Mismatch)?;
    let secret = new_secret();
    let deadline = Instant::now() + wait;
    match deliver_by(mailbox, &slot, "b", &message, &secret, deadline, poll) {
        Ok(()) => {}
        // Another answer got there first.
        Err(ApiError::Other(_)) => return Err(JoinError::Mismatch),
        Err(e) => return Err(unreachable(e)),
    }
    let key = seal::invite_key(&shared);
    loop {
        let parts = match mailbox.read(&slot) {
            Ok(parts) => parts,
            Err(ApiError::Gone) => return Err(JoinError::Expired),
            // Offline for a moment: looked at again until the wait is over.
            Err(_) => Parts::default(),
        };
        if let Some(invite) = parts.invite {
            let body = seal::open_with(&key, &format!("invite/{slot}"), &invite).ok().and_then(|b| serde_json::from_slice::<InviteBody>(&b).ok());
            let group = body.as_ref().and_then(|b| Some(Group { id: b.group.clone(), key: B64.decode(&b.key).ok()?.try_into().ok()? }));
            let (Some(body), Some(group)) = (body, group) else {
                let _ = deliver(mailbox, &slot, "reply", NO_MATCH, &secret);
                return Err(JoinError::Mismatch);
            };
            let reply = seal::seal_with(&key, &format!("reply/{slot}"), &serde_json::to_vec(&ReplyBody { name: my_name.into() }).expect("plain data"));
            // The invitation is in hand, so the reply gets time of its own to arrive.
            deliver_by(mailbox, &slot, "reply", &reply, &secret, Instant::now() + REPLY_WAIT, poll).map_err(unreachable)?;
            return Ok(Invite { group, from: body.from });
        }
        if Instant::now() >= deadline {
            return Err(JoinError::Expired);
        }
        std::thread::sleep(poll);
    }
}

/// The relay's pairing slots, over HTTPS. Messages travel as base64 in JSON.
impl Mailbox for super::relay::Http {
    fn open(&self, a: &[u8], secret: &str) -> Result<String, ApiError> {
        #[derive(Deserialize)]
        struct Opened {
            slot: String,
        }
        let body = serde_json::json!({ "a": B64.encode(a), "hostHash": seal::token_hash(secret.as_bytes()) });
        match self.send("POST", "/v1/pair", None, super::relay::Body::Json(body))? {
            (200, body) => serde_json::from_slice::<Opened>(&body).map(|o| o.slot).map_err(|e| ApiError::Other(e.to_string())),
            (503, _) => Err(ApiError::Unreachable("Every code is in use right now. Try again in a few minutes.".into())),
            (status, _) => Err(ApiError::Other(status.to_string())),
        }
    }

    fn read(&self, slot: &str) -> Result<Parts, ApiError> {
        #[derive(Deserialize)]
        struct Raw {
            a: Option<String>,
            b: Option<String>,
            invite: Option<String>,
            reply: Option<String>,
        }
        match self.send("GET", &format!("/v1/pair/{slot}"), None, super::relay::Body::None)? {
            (200, body) => {
                let raw: Raw = serde_json::from_slice(&body).map_err(|e| ApiError::Other(e.to_string()))?;
                let decode = |v: Option<String>| v.and_then(|v| B64.decode(v).ok());
                Ok(Parts { a: decode(raw.a), b: decode(raw.b), invite: decode(raw.invite), reply: decode(raw.reply) })
            }
            (404, _) => Err(ApiError::Gone),
            (status, _) if status >= 500 => Err(ApiError::Unreachable(status.to_string())),
            (status, _) => Err(ApiError::Other(status.to_string())),
        }
    }

    fn write(&self, slot: &str, part: &str, bytes: &[u8], secret: &str) -> Result<(), ApiError> {
        // The answer leaves the hash of this computer's secret; the later parts prove it.
        let (body, token) = if part == "b" {
            (serde_json::json!({ "data": B64.encode(bytes), "joinerHash": seal::token_hash(secret.as_bytes()) }), None)
        } else {
            (serde_json::json!({ "data": B64.encode(bytes) }), Some(secret))
        };
        match self.send("PUT", &format!("/v1/pair/{slot}/{part}"), token, super::relay::Body::Json(body))? {
            (200, _) => Ok(()),
            (404, _) => Err(ApiError::Gone),
            (status, _) if status >= 500 => Err(ApiError::Unreachable(status.to_string())),
            (status, _) => Err(ApiError::Other(status.to_string())),
        }
    }

    fn close(&self, slot: &str, secret: &str) {
        let _ = self.send("DELETE", &format!("/v1/pair/{slot}"), Some(secret), super::relay::Body::None);
    }
}

/// The in-memory relay's pairing slots, for tests.
#[cfg(test)]
impl Mailbox for super::relay::MemApi {
    fn open(&self, a: &[u8], secret: &str) -> Result<String, ApiError> {
        let mut slots = self.slots.lock().unwrap();
        let slot = format!("{}{}", (b'A' + slots.len() as u8) as char, 'B');
        let parts = Parts { a: Some(a.to_vec()), ..Parts::default() };
        slots.insert(slot.clone(), MemSlot { parts, host: seal::token_hash(secret.as_bytes()), joiner: None });
        Ok(slot)
    }
    fn read(&self, slot: &str) -> Result<Parts, ApiError> {
        self.slots.lock().unwrap().get(slot).map(|s| s.parts.clone()).ok_or(ApiError::Gone)
    }
    fn write(&self, slot: &str, part: &str, bytes: &[u8], secret: &str) -> Result<(), ApiError> {
        let mut slots = self.slots.lock().unwrap();
        let s = slots.get_mut(slot).ok_or(ApiError::Gone)?;
        let hash = seal::token_hash(secret.as_bytes());
        let (field, before) = match part {
            "b" => (&mut s.parts.b, true),
            "invite" if s.host == hash => (&mut s.parts.invite, s.parts.b.is_some()),
            "reply" if s.joiner.as_deref() == Some(hash.as_str()) => (&mut s.parts.reply, s.parts.invite.is_some()),
            "invite" | "reply" => return Err(ApiError::Other("401".into())),
            _ => return Err(ApiError::Other("400".into())),
        };
        if field.is_some() || !before {
            return Err(ApiError::Other("409".into()));
        }
        *field = Some(bytes.to_vec());
        if part == "b" {
            s.joiner = Some(hash);
        }
        Ok(())
    }
    fn close(&self, slot: &str, secret: &str) {
        let mut slots = self.slots.lock().unwrap();
        if slots.get(slot).is_some_and(|s| s.host == seal::token_hash(secret.as_bytes())) {
            slots.remove(slot);
        }
    }
}

/// A pairing slot in the in-memory relay: its parts, and the hashes of the host's and the
/// answering computer's secrets.
#[cfg(test)]
pub struct MemSlot {
    pub parts: Parts,
    host: String,
    joiner: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    type MemMailbox = super::super::relay::MemApi;

    const FAST: Duration = Duration::from_millis(5);

    /// Steps the host on its own thread until it's done.
    fn run_host(mut host: Host) -> std::thread::JoinHandle<PairState> {
        std::thread::spawn(move || loop {
            match host.step(1_000) {
                PairState::Waiting => std::thread::sleep(FAST),
                done => return done,
            }
        })
    }

    #[test]
    fn codes_format_and_parse() {
        let password = new_password();
        assert_eq!(password.len(), 6);
        assert!(password.chars().all(|c| "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c)));
        assert_eq!(format_code("K7", "QX49MP"), "K7QX-49MP");
        let want = Some(("K7".to_string(), "QX49MP".to_string()));
        assert_eq!(parse_code("K7QX-49MP"), want);
        assert_eq!(parse_code("k7qx 49mp"), want);
        assert_eq!(parse_code(" k7qx-49mp "), want);
        assert_eq!(parse_code("O1IL-0000"), Some(("01".to_string(), "110000".to_string())));
        assert_eq!(parse_code("K7QX-49M"), None);
        assert_eq!(parse_code("K7QX-49MPP"), None);
        assert_eq!(parse_code("K7QX-49MU"), None);
        assert_eq!(parse_code(""), None);
    }

    #[test]
    fn pairing_links_two_computers() {
        let mailbox = Arc::new(MemMailbox::default());
        let group = super::super::relay::new_group();
        let host = Host::start(mailbox.clone(), &group, "Desk", 0).unwrap();
        assert_eq!(host.expires_ms, CODE_MS);
        let code = host.code.clone();
        let hosting = run_host(host);
        let invite = join_polling(mailbox.as_ref(), &code.to_lowercase(), "Laptop", Duration::from_secs(5), FAST).unwrap();
        assert_eq!(invite, Invite { group, from: "Desk".into() });
        assert_eq!(hosting.join().unwrap(), PairState::Linked { name: "Laptop".into() });
        // The slot is freed once linked.
        assert!(mailbox.slots.lock().unwrap().is_empty());
    }

    #[test]
    fn wrong_code_is_a_mismatch_and_spends_the_slot() {
        let mailbox = Arc::new(MemMailbox::default());
        let group = super::super::relay::new_group();
        let host = Host::start(mailbox.clone(), &group, "Desk", 0).unwrap();
        let code = host.code.clone();
        let (slot, password) = parse_code(&code).unwrap();
        let wrong = format_code(&slot, if password == "000000" { "111111" } else { "000000" });
        let hosting = run_host(host);
        assert_eq!(join_polling(mailbox.as_ref(), &wrong, "Laptop", Duration::from_secs(5), FAST), Err(JoinError::Mismatch));
        assert_eq!(hosting.join().unwrap(), PairState::Mismatch);
        // The right code is no good now either.
        assert_eq!(join_polling(mailbox.as_ref(), &code, "Laptop", Duration::from_secs(1), FAST), Err(JoinError::Mismatch));
        assert_eq!(join_polling(mailbox.as_ref(), "not a code!", "Laptop", Duration::from_secs(1), FAST), Err(JoinError::BadCode));
    }

    /// A connection that drops every few requests: some never reach the relay, and some writes
    /// do but their answer is lost.
    struct Flaky {
        inner: MemMailbox,
        calls: std::sync::atomic::AtomicU32,
    }

    impl Flaky {
        fn new() -> Self {
            Flaky { inner: MemMailbox::default(), calls: Default::default() }
        }
        fn turn(&self) -> u32 {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) % 3
        }
    }

    impl Mailbox for Flaky {
        fn open(&self, a: &[u8], secret: &str) -> Result<String, ApiError> {
            self.inner.open(a, secret)
        }
        fn read(&self, slot: &str) -> Result<Parts, ApiError> {
            match self.turn() {
                1 => Err(ApiError::Unreachable("dropped".into())),
                _ => self.inner.read(slot),
            }
        }
        fn write(&self, slot: &str, part: &str, bytes: &[u8], secret: &str) -> Result<(), ApiError> {
            match self.turn() {
                1 => Err(ApiError::Unreachable("dropped".into())),
                2 => {
                    let _ = self.inner.write(slot, part, bytes, secret);
                    Err(ApiError::Unreachable("answer lost".into()))
                }
                _ => self.inner.write(slot, part, bytes, secret),
            }
        }
        fn close(&self, slot: &str, secret: &str) {
            self.inner.close(slot, secret)
        }
    }

    /// Loses the answer to the first write, and drops the two reads that follow.
    #[derive(Default)]
    struct LostThenDropped {
        inner: MemMailbox,
        writes: std::sync::atomic::AtomicU32,
        reads: std::sync::atomic::AtomicU32,
    }

    impl Mailbox for LostThenDropped {
        fn open(&self, a: &[u8], secret: &str) -> Result<String, ApiError> {
            self.inner.open(a, secret)
        }
        fn read(&self, slot: &str) -> Result<Parts, ApiError> {
            match self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst) {
                0 | 1 => Err(ApiError::Unreachable("dropped".into())),
                _ => self.inner.read(slot),
            }
        }
        fn write(&self, slot: &str, part: &str, bytes: &[u8], secret: &str) -> Result<(), ApiError> {
            let result = self.inner.write(slot, part, bytes, secret);
            match self.writes.fetch_add(1, std::sync::atomic::Ordering::SeqCst) {
                0 => Err(ApiError::Unreachable("answer lost".into())),
                _ => result,
            }
        }
        fn close(&self, slot: &str, secret: &str) {
            self.inner.close(slot, secret)
        }
    }

    #[test]
    fn a_write_that_got_there_counts_even_when_the_look_after_it_drops() {
        let mailbox = LostThenDropped::default();
        let slot = mailbox.open(b"a", "host").unwrap();
        // Written, but its answer lost, and the look to see whether it arrived dropped too;
        // writing again is refused as already written, which it was.
        deliver_by(&mailbox, &slot, "b", b"answer", "joiner", Instant::now() + Duration::from_secs(5), FAST).unwrap();
        assert_eq!(mailbox.inner.read(&slot).unwrap().b.as_deref(), Some(&b"answer"[..]));
    }

    #[test]
    fn pairing_gets_through_a_connection_that_keeps_dropping() {
        let mailbox = Arc::new(Flaky::new());
        let group = super::super::relay::new_group();
        let host = Host::start(mailbox.clone(), &group, "Desk", 0).unwrap();
        let code = host.code.clone();
        let hosting = run_host(host);
        let invite = join_polling(mailbox.as_ref(), &code, "Laptop", Duration::from_secs(5), FAST).unwrap();
        assert_eq!(invite.group, group);
        assert_eq!(hosting.join().unwrap(), PairState::Linked { name: "Laptop".into() });
    }

    #[test]
    fn no_one_else_can_spoil_a_pairing_under_way() {
        let mailbox = Arc::new(MemMailbox::default());
        let group = super::super::relay::new_group();
        let host = Host::start(mailbox.clone(), &group, "Desk", 0).unwrap();
        let code = host.code.clone();
        let (slot, _) = parse_code(&code).unwrap();
        // Someone without the host's secret can't close the slot or write its invitation.
        mailbox.close(&slot, "guess");
        assert!(mailbox.read(&slot).is_ok());
        assert!(mailbox.write(&slot, "invite", b"junk", "guess").is_err());
        let hosting = run_host(host);
        let invite = join_polling(mailbox.as_ref(), &code, "Laptop", Duration::from_secs(5), FAST).unwrap();
        assert_eq!(invite.group, group);
        assert_eq!(hosting.join().unwrap(), PairState::Linked { name: "Laptop".into() });
    }

    #[test]
    fn host_expires_after_ten_minutes() {
        let mailbox = Arc::new(MemMailbox::default());
        let mut host = Host::start(mailbox.clone(), &super::super::relay::new_group(), "Desk", 0).unwrap();
        assert_eq!(host.step(CODE_MS - 1), PairState::Waiting);
        assert_eq!(host.step(CODE_MS), PairState::Expired);
        assert_eq!(host.step(CODE_MS + 1), PairState::Expired);
        assert!(mailbox.slots.lock().unwrap().is_empty());
    }

    #[test]
    fn a_cancelled_code_frees_its_slot() {
        let mailbox = Arc::new(MemMailbox::default());
        let mut host = Host::start(mailbox.clone(), &super::super::relay::new_group(), "Desk", 0).unwrap();
        host.cancel();
        assert!(mailbox.slots.lock().unwrap().is_empty());
        assert!(matches!(host.step(1), PairState::Failed { .. }));
    }
}
