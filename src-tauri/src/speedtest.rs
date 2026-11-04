//! How fast the internet connection is: ping, jitter, download and upload, measured against
//! Cloudflare's public speed test and told to the speed window as it goes. At most 100 MB a test.
//! See docs/superpowers/specs/2026-11-04-speed-test-design.md.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

const BASE: &str = "https://speed.cloudflare.com";
const MB: u64 = 1_000_000;
const PINGS: usize = 11;
const DOWN_SIZES: [u64; 3] = [MB, 10 * MB, 25 * MB];
const UP_SIZES: [u64; 3] = [MB, 5 * MB, 10 * MB];
const DOWN_FOR: Duration = Duration::from_secs(8);
const UP_FOR: Duration = Duration::from_secs(6);
const DOWN_CAP: u64 = 70 * MB;
const UP_CAP: u64 = 30 * MB;

pub(crate) const LABEL: &str = "speed";

/// Which test is running; a newer one (Run again) or the window closing stops an older one.
static RUN: AtomicU64 = AtomicU64::new(0);

// ---------- the maths ----------

/// Megabits a second for `bytes` in `secs`.
pub fn mbps(bytes: u64, secs: f64) -> f64 {
    if secs <= 0.0 {
        return 0.0;
    }
    bytes as f64 * 8.0 / secs / 1_000_000.0
}

pub fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let mid = v.len() / 2;
    Some(if v.len() % 2 == 0 { (v[mid - 1] + v[mid]) / 2.0 } else { v[mid] })
}

/// How much ping varies: the mean difference between one and the next.
pub fn jitter(pings: &[f64]) -> Option<f64> {
    if pings.len() < 2 {
        return None;
    }
    Some(pings.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (pings.len() - 1) as f64)
}

/// A direction's speed from its transfers (bytes, Mbps): the 90th percentile of those of 1 MB or
/// more, which have got up to speed; all of them when none has.
pub fn speed_of(transfers: &[(u64, f64)]) -> Option<f64> {
    let big: Vec<f64> = transfers.iter().filter(|(b, _)| *b >= MB).map(|(_, s)| *s).collect();
    let mut v = if big.is_empty() { transfers.iter().map(|(_, s)| *s).collect() } else { big };
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let at = ((v.len() as f64 * 0.9).ceil() as usize).clamp(1, v.len()) - 1;
    Some(v[at])
}

/// The next transfer's size: growing through `sizes`, then the largest again, until `limit` has
/// passed or another would go past `cap`.
pub fn next_size(sizes: &[u64], done: usize, elapsed: Duration, limit: Duration, used: u64, cap: u64) -> Option<u64> {
    if elapsed >= limit {
        return None;
    }
    let size = sizes[done.min(sizes.len() - 1)];
    (used + size <= cap).then_some(size)
}

/// The Cloudflare location from a `cf-ray` header ("8d1f...-LHR" → "LHR").
pub fn location(ray: &str) -> Option<String> {
    ray.rsplit_once('-').map(|(_, at)| at.trim().to_string()).filter(|at| !at.is_empty() && at.chars().all(|c| c.is_ascii_alphanumeric()))
}

// ---------- measuring ----------

/// What the window shows, sent after each step.
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// "ping", "download", "upload", "done" or "error".
    pub phase: &'static str,
    pub ping_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub down_mbps: Option<f64>,
    pub up_mbps: Option<f64>,
    pub used_bytes: u64,
    pub server: Option<String>,
    pub error: Option<String>,
}

fn client() -> Option<&'static reqwest::Client> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            // rustls needs its cryptography chosen once per process, as the relay and updater do.
            if rustls::crypto::CryptoProvider::get_default().is_none() {
                let _ = rustls::crypto::ring::default_provider().install_default();
            }
            reqwest::Client::builder().timeout(Duration::from_secs(30)).build().ok()
        })
        .as_ref()
}

const UNREACHABLE: &str = "Couldn't reach the speed test. Check your connection.";

/// Runs one test, telling the window as it goes; stops quietly if a newer one starts or the
/// window closes.
async fn run(app: &AppHandle, id: u64) -> Result<Progress, String> {
    let client = client().ok_or("Kairo couldn't make a secure connection.")?;
    let still = || RUN.load(Ordering::SeqCst) == id && app.get_webview_window(LABEL).is_some();
    let mut p = Progress { phase: "ping", ..Default::default() };
    let tell = |p: &Progress| {
        let _ = app.emit_to(LABEL, "speed:progress", p.clone());
    };
    tell(&p);

    // Ping: the first pays for the connection, so it's left out.
    let mut pings = Vec::new();
    for i in 0..PINGS {
        if !still() {
            return Err(String::new());
        }
        let started = Instant::now();
        let response = client.get(format!("{BASE}/__down?bytes=0")).send().await.map_err(|_| UNREACHABLE)?;
        if p.server.is_none() {
            p.server = response.headers().get("cf-ray").and_then(|v| v.to_str().ok()).and_then(location);
        }
        let _ = response.bytes().await;
        if i > 0 {
            pings.push(started.elapsed().as_secs_f64() * 1000.0);
        }
    }
    p.ping_ms = median(&pings);
    p.jitter_ms = jitter(&pings);

    // Download.
    p.phase = "download";
    tell(&p);
    let (began, mut transfers, mut used) = (Instant::now(), Vec::new(), 0u64);
    while let Some(size) = next_size(&DOWN_SIZES, transfers.len(), began.elapsed(), DOWN_FOR, used, DOWN_CAP) {
        if !still() {
            return Err(String::new());
        }
        let started = Instant::now();
        let got = client.get(format!("{BASE}/__down?bytes={size}")).send().await.map_err(|_| UNREACHABLE)?.bytes().await.map_err(|_| UNREACHABLE)?;
        let bytes = got.len() as u64;
        used += bytes;
        transfers.push((bytes, mbps(bytes, started.elapsed().as_secs_f64())));
        p.down_mbps = speed_of(&transfers);
        p.used_bytes = used;
        tell(&p);
    }

    // Upload.
    p.phase = "upload";
    tell(&p);
    let (began, mut transfers, mut sent) = (Instant::now(), Vec::new(), 0u64);
    while let Some(size) = next_size(&UP_SIZES, transfers.len(), began.elapsed(), UP_FOR, sent, UP_CAP) {
        if !still() {
            return Err(String::new());
        }
        let started = Instant::now();
        let response = client.post(format!("{BASE}/__up")).header("content-type", "text/plain").body(vec![b'0'; size as usize]).send().await.map_err(|_| UNREACHABLE)?;
        let _ = response.bytes().await;
        sent += size;
        transfers.push((size, mbps(size, started.elapsed().as_secs_f64())));
        p.up_mbps = speed_of(&transfers);
        p.used_bytes = used + sent;
        tell(&p);
    }

    p.phase = "done";
    tell(&p);
    Ok(p)
}

/// Starts a test, stopping any still running; the window hears each step as `speed:progress`.
#[tauri::command]
pub async fn speed_run(app: AppHandle) {
    let id = RUN.fetch_add(1, Ordering::SeqCst) + 1;
    if let Err(why) = run(&app, id).await {
        // An empty reason is a test stopped on purpose: nothing to say.
        if !why.is_empty() && RUN.load(Ordering::SeqCst) == id {
            let _ = app.emit_to(LABEL, "speed:progress", Progress { phase: "error", error: Some(why), ..Default::default() });
        }
    }
}

/// Opens the speed window; its page starts a test as it loads, or now if it's open.
pub(crate) fn open(app: &AppHandle) {
    let spec = crate::small_window::Spec {
        label: LABEL,
        file: "speed.html",
        title: "Speed test",
        size: (380.0, 400.0),
        min: (320.0, 340.0),
        on_top: false,
        takes_files: false,
    };
    crate::small_window::show(app, spec, "speed:again", ());
}

#[tauri::command]
pub fn open_speed(app: AppHandle) {
    open(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_is_megabits_a_second() {
        assert_eq!(mbps(10 * MB, 1.0), 80.0);
        assert_eq!(mbps(MB, 0.5), 16.0);
        assert_eq!(mbps(MB, 0.0), 0.0);
    }

    #[test]
    fn ping_is_the_middle_one_and_jitter_how_it_varies() {
        assert_eq!(median(&[30.0, 10.0, 20.0]), Some(20.0));
        assert_eq!(median(&[10.0, 20.0, 30.0, 40.0]), Some(25.0));
        assert_eq!(median(&[]), None);
        assert_eq!(jitter(&[10.0, 14.0, 12.0]), Some(3.0));
        assert_eq!(jitter(&[10.0]), None);
    }

    #[test]
    fn a_direction_counts_its_big_transfers_near_their_best() {
        // A small one is left out once a big one is in; then the 90th percentile.
        let t = [(100_000, 5.0), (MB, 50.0), (10 * MB, 200.0), (25 * MB, 240.0), (25 * MB, 230.0)];
        assert_eq!(speed_of(&t), Some(240.0));
        assert_eq!(speed_of(&[(100_000, 5.0), (200_000, 7.0)]), Some(7.0));
        assert_eq!(speed_of(&[]), None);
    }

    #[test]
    fn transfers_grow_and_stop_at_the_time_or_the_cap() {
        let s = Duration::from_secs;
        assert_eq!(next_size(&DOWN_SIZES, 0, s(0), DOWN_FOR, 0, DOWN_CAP), Some(MB));
        assert_eq!(next_size(&DOWN_SIZES, 1, s(1), DOWN_FOR, MB, DOWN_CAP), Some(10 * MB));
        assert_eq!(next_size(&DOWN_SIZES, 5, s(3), DOWN_FOR, 30 * MB, DOWN_CAP), Some(25 * MB));
        // Time up.
        assert_eq!(next_size(&DOWN_SIZES, 2, s(8), DOWN_FOR, 11 * MB, DOWN_CAP), None);
        // Another would go past the cap.
        assert_eq!(next_size(&DOWN_SIZES, 4, s(2), DOWN_FOR, 61 * MB, DOWN_CAP), None);
        // Together never past 100 MB.
        assert!(DOWN_CAP + UP_CAP <= 100 * MB);
    }

    #[test]
    fn the_location_comes_from_cf_ray() {
        assert_eq!(location("8d1f2a3b4c5d6e7f-LHR").as_deref(), Some("LHR"));
        assert_eq!(location("nonsense"), None);
        assert_eq!(location("abc-"), None);
    }
}
