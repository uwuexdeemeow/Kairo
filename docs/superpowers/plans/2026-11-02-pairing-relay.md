# Linking computers with a code — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Combine computers by typing a short code, through an end-to-end encrypted Cloudflare relay, alongside today's folder.

**Architecture:** `sync.rs` becomes `sync/mod.rs` and does its file work through a `Store` trait (folder, relay). `sync/seal.rs` derives keys and seals contents and names; `sync/relay.rs` is the relay store over an `Api` trait (HTTP, or in memory for tests); `sync/pair.rs` runs SPAKE2 over a `Mailbox` trait. The relay is a Worker with D1 in `relay/`.

**Tech Stack:** Rust (Tauri 2), `spake2 0.4`, `chacha20poly1305 0.10`, `hkdf 0.12`, `hmac 0.12`, `sha2 0.10`, `getrandom 0.2`, `flate2 1`, `reqwest` (already present); TypeScript Worker, D1, `wrangler`, `@cloudflare/vitest-pool-workers`.

**Spec:** `docs/superpowers/specs/2026-11-02-pairing-relay-design.md`

## Global Constraints

- Builds and tests on Windows, macOS and Linux; only pure-Rust crates.
- Code: 8 characters of Crockford base32 (`0123456789ABCDEFGHJKMNPQRSTVWXYZ`), shown `XXXX-XXXX`; first 2 = slot, last 6 = SPAKE2 password; one answer per slot; 10 minutes.
- HKDF-SHA256 infos: `"kairo file"`, `"kairo name"`, `"kairo access"`, `"kairo invite"`; SPAKE2 identity `"kairo-pair-v1"`.
- Contents: deflate, then XChaCha20-Poly1305, random 24-byte nonce first, AAD `"<computer>/<name>"`.
- Names: nonce = HMAC-SHA256(name key, name)[..24]; object name = base64url-nopad(nonce ‖ sealed).
- Settings keys `relay_group`, `relay_key`, `relay_synced_ms`, `relay_error` join `backup::LOCAL_SETTINGS`.
- Relay limits: 2 MB a file, 50 MB a group, 4 KB pairing bodies, slots 10 minutes, groups unused 180 days cleared.
- The relay is never reached while `AppState.db` is locked.
- UI copy exactly as the spec's "What the user sees".

## Review Focus

- Code typed with lowercase, spaces, `O`/`I`/`L` → still parses (pair tests).
- Day deleted on one computer while relay-combined → the others drop it (relay sync test mirrors the folder test).
- Relay answers 404 for the group → computer stops combining and records "unlinked" (relay store test).
- Second `PUT b` on a slot → 409, so a code allows one guess (Worker test).
- Restore from a backup taken on another computer keeps this computer's group (backup test).

---

### Task 1: Sealing (`sync/seal.rs`)

**Files:** Move `src-tauri/src/sync.rs` → `src-tauri/src/sync/mod.rs` (`git mv`, add `mod seal;`). Create `src-tauri/src/sync/seal.rs`. Modify `src-tauri/Cargo.toml`.

**Interfaces — Produces:**
- `pub struct Keys { file: [u8;32], name: [u8;32], pub token: [u8;32] }`, `Keys::derive(group_key: &[u8;32]) -> Keys`
- `pub fn random<const N: usize>() -> [u8; N]`
- `Keys::seal(&self, aad: &str, plain: &[u8]) -> Vec<u8>`, `Keys::open(&self, aad: &str, sealed: &[u8]) -> Result<Vec<u8>, String>` (deflate inside)
- `Keys::seal_name(&self, name: &str) -> String`, `Keys::open_name(&self, sealed: &str) -> Result<String, String>`
- `pub fn token_hash(token: &[u8;32]) -> String` (hex SHA-256)
- `pub fn invite_key(spake_key: &[u8]) -> [u8;32]` and `seal_with(key, aad, plain)` / `open_with(key, aad, sealed)` for pairing

- [ ] Tests: `round_trip`, `wrong_aad_fails`, `flipped_byte_fails`, `names_are_deterministic_and_readable` (same name → same string; open gives name back; two names differ), `token_hash_is_hex_sha256` (known vector of 32 zero bytes).
- [ ] Run `cargo test sync::seal` — fails; implement; passes. Whole `cargo test` still green after the move.
- [ ] Commit "Seal what goes to the relay".

### Task 2: The store (`sync/mod.rs`, `sync/store.rs`)

**Files:** Create `src-tauri/src/sync/store.rs`; modify `sync/mod.rs`.

**Interfaces — Produces:**
```rust
pub trait Store {
    /// Every computer's files: computer id → (file name → version).
    fn list(&self) -> Result<HashMap<String, HashMap<String, String>>>;
    fn get(&self, computer: &str, name: &str) -> Result<Vec<u8>>;
    fn put(&self, name: &str, bytes: &[u8]) -> Result<()>;   // this computer's own
    fn delete(&self, name: &str) -> Result<()>;
    fn remove_me(&self) -> Result<()>;
}
pub struct FolderStore { root: PathBuf /* <folder>/Kairo computers */, me: String }
#[cfg(test)] pub struct MemStore  // shared map, cloned per computer, versions counted
```
- `write_choices/read_choices/write_own/write_day/read_others/import_day/computers` take `&dyn Store`. `write_own` checks presence through `list()` instead of `day_path().exists()`. Day files: written into a temp file via `ATTACH`, read to bytes, `put`; imported by writing bytes to a temp file, then `read_day`.
- `pub fn run_with(conn, store: &dyn Store, now_ms) -> Result<bool>`; `run_once` builds the store for the mode.
- `pub fn combining(conn) -> Result<bool>`; `shown` uses it.

- [ ] Existing sync tests pass unchanged against the folder.
- [ ] New test `combines_through_any_store`: two computers over `MemStore` — time crosses, choices cross, a deleted day disappears on the other, `remove_me` drops the computer.
- [ ] Commit "Combine through any store".

### Task 3: The relay Worker (`relay/`)

**Files:** `relay/package.json`, `relay/wrangler.toml`, `relay/schema.sql`, `relay/src/index.ts`, `relay/test/relay.test.ts`, `relay/vitest.config.ts`, `relay/tsconfig.json`, `.github/workflows/relay.yml`.

Routes, tables, limits and status codes exactly as the spec's table; daily cron `0 4 * * *` clears expired pairs and groups with `used_ms` older than 180 days. Slots: two Crockford characters, chosen randomly among free ones.

- [ ] Tests: `needs the token` (401 without, 404 unknown group), `stores lists and deletes files` (version bumps per write), `refuses files over 2 MB` (413), `refuses groups over 50 MB` (507), `pair parts are write-once` (second `PUT b` → 409), `expired slots are gone` (404 after expiry), `deleting a group removes its files`.
- [ ] `cd relay && npm install && npm test` passes.
- [ ] Commit "Relay for linking computers with a code".

### Task 4: Relay store (`sync/relay.rs`)

**Interfaces — Produces:**
```rust
pub trait Api: Send + Sync {
    fn create_group(&self, group: &str, token_hash: &str) -> Result<(), ApiError>;
    fn delete_group(&self, group: &str, token: &str) -> Result<(), ApiError>;
    fn list(&self, group: &str, token: &str) -> Result<Vec<(String, String, String)>, ApiError>; // computer, name, version
    fn get(&self, group, token, computer, name) -> Result<Vec<u8>, ApiError>;
    fn put(&self, group, token, computer, name, bytes: &[u8]) -> Result<(), ApiError>;
    fn delete(&self, group, token, computer, name) -> Result<(), ApiError>;
    fn delete_computer(&self, group, token, computer) -> Result<(), ApiError>;
}
pub enum ApiError { Unreachable(String), Gone /*401,404*/, Other(String) }
pub struct Http { base: String }          // reqwest, block_on, 20s timeout; base from KAIRO_RELAY or RELAY_URL
pub struct RelayStore { api: Arc<dyn Api>, group: String, keys: Keys, me: String }  // impl Store
pub struct Group { pub id: String, pub key: [u8;32] }
pub fn group(conn) -> Result<Option<Group>>; pub fn set_group(conn, Option<&Group>) -> Result<()>;
#[cfg(test)] pub struct MemApi
```
- `RelayStore::list` opens names; unreadable names are skipped.
- `mod.rs` `run_once`: on `ApiError::Gone`, clear the group, delete imported time (as `stop`), set `relay_error = "unlinked"`; otherwise record `relay_synced_ms`/`relay_error`.

- [ ] Tests: `combines_through_the_relay` (Task 2's scenario over `MemApi`), `relay_sees_no_names_or_contents` (stored names/bytes don't contain `choices.json` or an app name), `gone_group_stops_combining`.
- [ ] Commit "Combine through the relay".

### Task 5: Pairing (`sync/pair.rs`)

**Interfaces — Produces:**
```rust
pub fn new_password() -> String;                         // 6 Crockford chars
pub fn format_code(slot: &str, password: &str) -> String; // "K7QX-49MP"
pub fn parse_code(typed: &str) -> Option<(String, String)>;
pub trait Mailbox: Send + Sync {
    fn open(&self, a: &[u8]) -> Result<String, ApiError>;      // slot
    fn read(&self, slot: &str) -> Result<Parts, ApiError>;
    fn write(&self, slot: &str, part: &str, bytes: &[u8]) -> Result<(), ApiError>; // "b"|"invite"|"reply"
    fn close(&self, slot: &str);
}
pub struct Parts { pub a: Option<Vec<u8>>, pub b: Option<..>, pub invite: Option<..>, pub reply: Option<..> }
pub struct Invite { pub group: Group, pub from: String }
pub enum PairState { Waiting, Linked { name: String }, Expired, Mismatch, Failed { message: String } }
pub struct Host { pub code: String, pub expires_ms: i64, .. }  // A side
impl Host { pub fn start(mb, group: &Group, my_name, now_ms) -> Result<Host>; pub fn step(&mut self, now_ms) -> PairState; }
pub fn join(mb: &dyn Mailbox, code: &str, my_name: &str, wait: Duration) -> Result<Invite, JoinError>; // JoinError::{BadCode, Mismatch, Expired, Unreachable(String)}
```
`Http` implements `Mailbox` too.

- [ ] Tests: `codes_format_and_parse` (`"k7qx 49mp"`, `"K7QX-49MP"`, `"k7qx-49mp"` parse; `O`→`0`, `I`/`L`→`1`; 7 or 9 chars → None; `U` → None), `pairing_links_two_computers` (Host and join on two threads over `MemMailbox`; joiner gets the group; host ends `Linked { name }`), `wrong_code_is_a_mismatch_and_spends_the_slot`, `host_expires_after_ten_minutes`.
- [ ] Commit "Pair two computers with a code".

### Task 6: Wiring (`commands.rs`, `lib.rs`, `backup.rs`, `sync/mod.rs`)

- Sync thread waits on `mpsc::Receiver<()>` with `recv_timeout(EVERY)`; `AppState.sync_now: Sender<()>`.
- `AppState.pairing: Mutex<Option<pair::Host>>` stepped by a pairing thread with its own connection; on `Linked` it stores the group (if A wasn't on) and sends `sync_now`.
- Commands `pair_start`, `pair_status`, `pair_cancel`, `pair_join(code)` (async, own connection, no app lock during network), `start_over`; `SyncState` gains `mode`, `syncedMs`, `error`. `stop_sync` in relay mode: `remove_me` on a worker thread, then clears.
- `LOCAL_SETTINGS` gains the four keys.
- [ ] Test in `backup.rs`: `restore_keeps_this_computers_group`.
- [ ] `cargo test`, `cargo clippy` clean. Commit "Link computers with a code from Kairo's settings".

### Task 7: Settings UI (`dashboard.html`, `src/kairo-settings.ts`, `src/api.ts`, `src/styles.css`)

- Renders the four states from the spec; code in Geist Mono with a countdown polling `pair_status` once a second; Start over confirms in-page (no `confirm()`).
- [ ] `src/kairo-settings.test.ts`: off shows Link a computer/Enter a code/Use a folder instead; relay mode shows the encrypted line, Link another computer, Stop combining, Start over; a running code shows `K7QX-49MP` and the countdown; `mismatch` shows "That code didn't match. Make a new one."; unlinked error shows the unlinked copy.
- [ ] `npm test`, `npm run build`. Commit "Link a computer from Kairo's settings".

### Task 8: Docs

- README: Combine computers paragraph, "all data stays on your device" sentence, relay deployment under Development. Device checklist: "Linking with a code".
- [ ] Commit "Document linking with a code".
