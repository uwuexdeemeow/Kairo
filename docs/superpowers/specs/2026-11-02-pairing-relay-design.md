# Linking computers with a code

## Goal

Combining computers takes a short code instead of a cloud-drive folder: one computer shows
`K7QX-49MP`, the other types it, and from then on they combine exactly as they do through a
folder (everyone's time on Today, Week and Patterns, shared categories, limits, goals, rules,
projects, lists, tasks, notes and schedule; focus sessions per computer). The time travels
through a small relay Kairo runs on Cloudflare, encrypted end to end: the relay stores and
hands back bytes it can't read. Combining through a folder stays, for anyone who wants no
server involved.

## What the user sees

Kairo's settings, **Combine computers**:

- **Off:** "See the time from all your computers together." Buttons **Link a computer** and
  **Enter a code**, and a quieter **Use a folder instead** (today's folder picker).
- **Link a computer:** shows the code, large and in Geist Mono, as `K7QX-49MP`, with "Type this
  on your other computer. It works once, for 10 minutes." and a countdown, plus **Cancel**. When
  the other computer finishes: "Linked with Laptop." and the computers list fills in after its
  first sync. When the time runs out: "The code ran out. Make a new one." A code whose guess was
  wrong is used up the same way: "That code didn't match. Make a new one."
- **Enter a code:** a text field (case, spaces and dashes don't matter; `O`→`0`, `I`/`L`→`1`)
  and **Link**. While it works: "Linking…". Then "Linked with Desk-PC." or one of: "That code
  didn't match, or it's been used. Make a new one on the other computer.", "That code ran out.",
  "Can't reach Kairo's relay. Check the internet connection."
- **On, through the relay:** "Combined through Kairo's relay, encrypted end to end: the relay
  can't read your time." The computers list as today, with each one's last sync. Under it
  **Link another computer**, **Stop combining** and **Start over**. When the last sync failed:
  "Last synced 2 hours ago. Can't reach Kairo's relay right now." (the time still adds up
  locally; nothing is lost).
- **On, through a folder:** as today, plus **Link a computer** is not offered (switching means
  stopping first).
- **Stop combining:** this computer's files leave the relay, so the others drop its time, and
  the others' time leaves this one, as with a folder. It forgets the group key.
- **Start over** (for a lost or sold computer, which still holds the key): asks to confirm, then
  deletes the whole group from the relay, starts a new one with this computer alone, and shows
  a code to link the others again. The others notice their group is gone at their next sync
  (the relay answers 404): they stop combining and say "This computer was unlinked. Enter a new
  code to combine again." in Settings.
- Any linked computer can show a code for the next. **Enter a code** is offered only while
  combining is off, so a computer is never in two groups, or a group and a folder, at once.

Copy follows Kairo's voice: plain, short, no jargon beyond "encrypted end to end".

## The group and its keys

- Linking the first computer makes a **group**: a random 16-byte id (hex) and a random 32-byte
  **group key**, kept in the settings table (`relay_group`, `relay_key` base64), added to
  backup's `LOCAL_SETTINGS` so restoring never moves a computer into another group.
- From the group key, HKDF-SHA256 derives:
  - the **file key** (`info = "kairo file"`), which encrypts contents;
  - the **name key** (`info = "kairo name"`), which scrambles file names;
  - the **access token** (`info = "kairo access"`), sent as `Authorization: Bearer <base64url>`.
    The relay keeps only its SHA-256, so it can tell who may read and write but holds nothing
    that decrypts.
- **Contents:** each file is deflated, then sealed with XChaCha20-Poly1305 under the file key,
  a random 24-byte nonce first, with the associated data `"<computer id>/<file name>"`, so the
  relay can't pass one file off as another.
- **Names:** sealed deterministically, so the same name always lands on the same object and can
  still be read back: nonce = the first 24 bytes of HMAC-SHA256(name key, name), then
  XChaCha20-Poly1305 under the name key; the object's name is base64url(nonce ‖ sealed). The
  relay can't tell a day file from `choices.json` or which days were tracked.
- Computer ids stay readable; they're random already.

## Pairing

The code is 8 characters from Crockford's base32 (`0-9 A-Z` without `I L O U`), shown as two
groups of four. The first two pick a **slot** on the relay; the other six (30 bits) are the
password for a SPAKE2 exchange (`spake2` crate, Ed25519 group, symmetric, identity
`"kairo-pair-v1"`). Each slot takes **one** answer, so a wrong guess spends the code; slots last
10 minutes.

1. **A** (the computer showing the code; if it isn't in a group yet it makes one now and
   registers it with the relay, without switching itself on) asks the relay for a free slot,
   sending its SPAKE2 message: `POST /v1/pair` → `{ slot }`. It draws the six-character
   password itself and shows slot + password.
2. **B** reads A's message (`GET /v1/pair/{slot}`), sends its own (`PUT /v1/pair/{slot}/b`,
   refused if one was already sent) and works out the shared key K.
3. **A**, checking once a second, gets B's message, works out K, and puts the **invitation**
   sealed under a key derived from K (`info = "kairo invite"`): `{ group, key, from: A's name }`
   (`PUT /v1/pair/{slot}/invite`).
4. **B** opens it. If it won't open, the code was wrong: B says so and stops. If it opens, B
   joins the group, switches on, seals `{ name: B's name }` as the **reply** and puts it
   (`PUT /v1/pair/{slot}/reply`).
5. **A** opens the reply, switches itself on if it wasn't, and shows "Linked with B". Both
   wake their sync thread so the first sync runs at once.

A gives up after 10 minutes or when cancelled (`DELETE /v1/pair/{slot}`); B gives up after 30
seconds without an invitation. Neither ever sends the code itself to the relay beyond the slot.

## The relay (`relay/`)

A Cloudflare Worker in TypeScript with one D1 database (no R2, which needs a card on file even
on the free plan). `wrangler.toml`, `schema.sql`, `src/index.ts`, tests with
`@cloudflare/vitest-pool-workers`. Its own `package.json`, outside the app's build.

Tables: `groups (id, token_hash, created_ms, used_ms, bytes)`, `files (group_id, computer, name,
version, data BLOB, size, PRIMARY KEY (group_id, computer, name))`, `pairs (slot, a, b, invite,
reply, expires_ms)`.

| Request | Does |
|---|---|
| `POST /v1/groups` `{ id, tokenHash }` | Makes a group; 409 if the id is taken |
| `DELETE /v1/groups/{g}` | Deletes the group and its files (Start over) |
| `GET /v1/groups/{g}/files` | `[{ computer, name, version }]`, every computer's |
| `PUT /v1/groups/{g}/files/{c}/{name}` | Stores the body; answers `{ version }` |
| `GET /v1/groups/{g}/files/{c}/{name}` | The body |
| `DELETE /v1/groups/{g}/files/{c}/{name}` | One file |
| `DELETE /v1/groups/{g}/computers/{c}` | All of a computer's files (Stop combining) |
| `POST /v1/pair` `{ a }` | A free slot, `{ slot }`; 503 when all 1024 are busy |
| `GET /v1/pair/{slot}` | `{ a, b, invite, reply }`, those present |
| `PUT /v1/pair/{slot}/{b,invite,reply}` | Each once; 409 after |
| `DELETE /v1/pair/{slot}` | Frees it |

Group requests need the bearer token (401 otherwise, 404 when the group doesn't exist). The
version is a counter bumped on every write. Limits: 2 MB a file (413), 50 MB a group (507),
pairing bodies 4 KB, expired slots and groups unused for 180 days cleared by a daily cron.
Everything is base64 in JSON for pairing, raw bytes for files.

## Inside Kairo

- **`sync.rs` gets a store.** Its file work moves behind
  `trait Store { fn list(&self) -> Result<HashMap<String, HashMap<String, String>>>; fn get(&self, computer, name) -> Result<Vec<u8>>; fn put(&self, name, bytes) -> Result<()>; fn delete(&self, name) -> Result<()>; fn remove_me(&self) -> Result<()>; }`
  where `list` gives each computer's files and a version for each (the folder gives size and
  modified time, as the stamp does today). `put`, `delete` and `remove_me` act on this
  computer's own files. `FolderStore` is today's behaviour; `RelayStore` (`sync/relay.rs`)
  does the sealing and HTTP. `write_choices`, `read_choices`, `write_own`, `write_day`,
  `read_others`, `import_day` and `computers` take a `&dyn Store`, and day files go through a
  temporary file to and from bytes.
- **Which store:** `sync_folder` set → folder; `relay_group` set → relay; neither → off.
  `combining(conn)` replaces `folder(conn).is_some()` in `shown`.
- **Network off the database lock.** The relay is only reached from the sync thread and the
  pairing thread, each with its own connection, never while the app's shared connection is
  locked. The sync thread waits on a channel with a 5-minute timeout, so linking, stopping and
  "Start over" can wake it at once.
- **Last sync:** the sync thread keeps `relay_synced_ms` and `relay_error` in settings for
  Settings to show; a 404 for the group stops combining and records "unlinked".
- **`sync/pair.rs`:** the code (draw, format, parse with the lenient rules above), the SPAKE2
  steps and sealing of invitation and reply, all over a small `Mailbox` trait so tests run
  without the network. A pairing in progress lives in `AppState` (one at a time): its code,
  expiry and state (`waiting`, `linked { name }`, `expired`, `mismatch`, `failed { message }`).
- **Commands:** `pair_start` → `{ code, expiresMs }`; `pair_status` → the state; `pair_cancel`;
  `pair_join(code)` → `SyncState` (runs on a worker thread); `start_over` → `{ code, expiresMs }`.
  `get_sync`'s `SyncState` gains `mode: "off" | "folder" | "relay"`, `syncedMs`, `error`.
- **Relay address:** `https://kairo-relay.<account>.workers.dev`, a constant in
  `sync/relay.rs`; `KAIRO_RELAY` in the environment overrides it, for tests and local runs.
- **New crates** (pure Rust, same on all three systems): `chacha20poly1305`, `hkdf`, `hmac`,
  `sha2`, `spake2`, `getrandom`, `flate2`.

## Errors

- Relay unreachable or 5xx: the round is skipped and retried at the next one; `relay_error`
  says so. Tracking never depends on the relay.
- A file that won't open (tampered, or from a newer format): skipped and logged, like a file a
  drive is still bringing in.
- 401: the group's token no longer matches (another computer started over and an attacker
  reused the id, or a bug): treated as unlinked.
- 413/507: logged; that day isn't sent until it's smaller. Real days are far under 2 MB.

## Testing

- Rust: sealing round trip and tamper detection (wrong associated data, flipped byte);
  deterministic names; code formatting and lenient parsing; a full pairing between two
  threads over an in-memory mailbox, including a wrong code; the existing sync tests run
  against both the folder store and an in-memory store that seals like the relay.
- Relay: Worker tests for auth, write-once pairing parts, limits, listing and deletion.
- CI: the existing workflows run the Rust tests on all three systems; a new `relay.yml` runs
  the Worker's tests.
- Device checklist: a "Linking with a code" section (two computers, a wrong code, an expired
  code, Stop combining, Start over).

## Not now

- Removing one computer without starting over (needs key rotation).
- Showing the code as a QR code.
- Choosing another relay in Settings.
