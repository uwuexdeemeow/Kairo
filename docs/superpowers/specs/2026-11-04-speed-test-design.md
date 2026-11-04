# Network speed test

## Goal

How fast the internet connection is, from the palette: download, upload, ping and jitter, in a
small window that fills in as it measures. No account; nothing runs unless asked.

## Opening

- The palette: `speed test`, `speedtest`, `internet speed` → "Test internet speed".
- One window, label `speed`, 380 × 400 (at least 320 × 340), made by `small_window`; it starts a
  test as it opens, and opened again while open, starts another if none is running.

## Measuring

Against Cloudflare's public speed test (speed.cloudflare.com), with Kairo's web client:

1. **Ping and jitter:** 11 `GET /__down?bytes=0`, the first dropped (it pays for the connection).
   Ping is the median; jitter the mean difference between consecutive ones.
2. **Download:** `GET /__down?bytes=N`, N = 1 MB, 10 MB, then 25 MB, until 8 seconds or 70 MB.
3. **Upload:** `POST /__up` with N bytes, N = 1 MB, 5 MB, then 10 MB, until 6 seconds or 30 MB.
4. A direction's speed: the 90th percentile of its transfers of 1 MB or more (all of them if none
   is). Shown live after each transfer.
5. **Server:** the Cloudflare location from `cf-ray`'s suffix ("LHR").

At most 100 MB a test. Closing the window, or Run again, stops the one running between transfers.

## The window

- ↓ Download and ↑ Upload in large figures (Mbps, one decimal under 100, none above), ping and
  jitter in ms, "Cloudflare · LHR".
- What it's doing: "Measuring ping…", "Testing download…", "Testing upload…", then "Done", or the
  error in words ("Couldn't reach the speed test. Check your connection.").
- **Run again**, and "Used about 80 MB".

## Testing

- Rust: Mbps from bytes and time; median, jitter; the 90th percentile rule; the next transfer's
  size by elapsed time and data used; the location from `cf-ray`.
- Vitest: live figures, Done, errors, Run again; formatting.
- By hand: one real test.
