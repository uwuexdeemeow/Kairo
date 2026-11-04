# Scanning with VirusTotal

## Goal

Check a downloaded file, a link or a hash against VirusTotal from Kairo, without uploading
anything by accident. Each person uses their own free key; Kairo ships none.

## The key

- Kairo's settings, a **VirusTotal** section: an API key field (shown as dots), **Test key**, and
  "Kept on this computer only, never synced. Free from virustotal.com → your profile → API key."
  Test says "Works" or why not (not accepted; no connection).
- Saved as the setting `virustotal_key`; never synced, never in a backup's export of settings to
  other computers, never logged.

## Opening

- The palette: `virustotal`, `vt` or `scan` opens the scan window; with a link or hash after it
  (`scan https://example.com`, `vt 3f2a…`), the window opens looking that up.
- One window, label `scan`, 380 × 480 at first (at least 300 × 360), size and place remembered.
  Opened again with a query, it looks that up. Not kept while closed.
- Without a key, the window says "Add your VirusTotal key in Kairo's settings" with a button
  opening the dashboard there.

## The window

- A drop zone ("Drop a file here", **Choose file…**), and a field for a link or a hash with
  **Check**. A typed value that is neither says "That's not a link or a hash".
- Results under it, newest first, the last 10 while it's open: the name (file name, link's host,
  or the hash cut short), when, and its state:
  - **Checking…** while asked.
  - **The verdict:** "● 3 of 71 engines flag it" — red if any say malicious, amber if only
    suspicious, green if none; up to three detection names with their engine, "+N more"; when it
    was last scanned; **Full report ↗** (virustotal.com).
  - **Not seen yet** (a file): "VirusTotal hasn't seen this file. Uploading shares it with
    security companies. Only upload what you'd be fine with others seeing." **Upload and scan** /
    **Not now**. Over 32 MB: "Too big to upload with a free key."
  - **Scanning…** "usually under a minute": an upload, or a link VirusTotal hadn't seen (links
    are submitted without asking, as the link is already public). Checked every 20 seconds until
    done.
  - **An error** in words: the key not accepted; "VirusTotal's free limit: try again in a
    minute"; no connection.

## How it asks

- A file: its SHA-256 worked out on this computer (read in pieces, any size); only the hash is
  sent: `GET /api/v3/files/{sha256}`.
- A hash (MD5, SHA-1 or SHA-256): `GET /api/v3/files/{hash}`.
- A link: `GET /api/v3/urls/{base64url(link)}`; not found: `POST /api/v3/urls`, then the analysis.
- An upload: `POST /api/v3/files` (multipart, built by hand), then the analysis.
- An analysis: `GET /api/v3/analyses/{id}` until `completed`, then the file or link again.
- Kairo's existing web client (reqwest, rustls); no new dependency.

## Testing

- Rust: reading a file's SHA-256 in pieces; telling links, hashes and neither apart; the link id;
  the multipart body; a verdict from a VirusTotal answer (malicious, suspicious, clean, names
  capped at three); errors from 401, 404, 429.
- Vitest: the window's states (checking, verdict colours, not seen with its warning and buttons,
  too big, scanning then done, errors), no key, a query handed over, the 10-result cap; the
  settings section (save, test); the palette's rows.
- By hand: a known file, the EICAR test file's hash, a link, an unknown file uploaded.
