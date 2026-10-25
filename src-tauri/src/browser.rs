//! The browser extension's side door: it reports the active tab's domain to a listener on
//! localhost, so sites are known exactly instead of guessed from window titles.

use std::io::Read;
use std::sync::{Arc, Mutex};
use std::thread;

use serde::{Deserialize, Serialize};
use tiny_http::{Header, Method, Response, Server};

/// Fixed so the extension knows where to find Kairo; only this computer can reach it.
pub const ADDR: &str = "127.0.0.1:47613";
/// The extension reports on every tab change and every 30 seconds in between.
const FRESH_MS: i64 = 75_000;
const MAX_BODY: u64 = 4 * 1024;

/// Domains that map onto the site names Kairo already uses for categories and limits.
const DOMAINS: &[(&str, &str)] = &[
    ("youtube.com", "youtube"),
    ("youtu.be", "youtube"),
    ("reddit.com", "reddit"),
    ("github.com", "github"),
    ("stackoverflow.com", "stackoverflow"),
    ("mail.google.com", "gmail"),
    ("docs.google.com", "google docs"),
    ("sheets.google.com", "google sheets"),
    ("calendar.google.com", "google calendar"),
    ("netflix.com", "netflix"),
    ("twitch.tv", "twitch"),
    ("instagram.com", "instagram"),
    ("facebook.com", "facebook"),
    ("tiktok.com", "tiktok"),
    ("linkedin.com", "linkedin"),
    ("x.com", "x"),
    ("twitter.com", "x"),
    ("chatgpt.com", "chatgpt"),
    ("chat.openai.com", "chatgpt"),
    ("claude.ai", "claude"),
    ("notion.so", "notion"),
    ("figma.com", "figma"),
    ("wikipedia.org", "wikipedia"),
    ("docs.rs", "docs.rs"),
    ("developer.mozilla.org", "mdn"),
];

/// Browsers the extension runs in: (id, name, executable, extensions page).
pub const BROWSERS: &[(&str, &str, &str, &str)] = &[
    ("brave", "Brave", "brave.exe", "brave://extensions"),
    ("chrome", "Chrome", "chrome.exe", "chrome://extensions"),
    ("edge", "Edge", "msedge.exe", "edge://extensions"),
    ("vivaldi", "Vivaldi", "vivaldi.exe", "vivaldi://extensions"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// None for pages without a web address, like a new tab or browser settings.
    pub site: Option<String>,
    pub incognito: bool,
    pub at_ms: i64,
}

/// The latest report, shared between the listener and the collector.
pub type SharedTab = Arc<Mutex<Option<Report>>>;

#[derive(Deserialize)]
struct Payload {
    host: Option<String>,
    #[serde(default)]
    incognito: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    listening: bool,
    /// When the extension last reported, if ever since Kairo started.
    last_report_ms: Option<i64>,
}

/// The site name for a hostname: a known site's usual name, or the bare domain.
pub fn site_for_host(host: &str) -> Option<String> {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    let valid = !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    if !valid || host == "localhost" {
        return None;
    }
    let bare = host
        .strip_prefix("www.")
        .or_else(|| host.strip_prefix("m."))
        .unwrap_or(&host);
    let known = DOMAINS
        .iter()
        .find(|(domain, _)| bare == *domain || bare.ends_with(&format!(".{domain}")));
    // Amazon has a domain per country.
    let amazon = bare.split('.').any(|part| part == "amazon");
    Some(match known {
        Some((_, site)) => site.to_string(),
        None if amazon => "amazon".to_string(),
        None => bare.to_string(),
    })
}

/// The site in front, if the extension's report is recent enough to trust.
pub fn fresh(shared: &SharedTab, now_ms: i64) -> Option<Report> {
    let report = shared.lock().unwrap_or_else(|e| e.into_inner()).clone()?;
    (now_ms - report.at_ms <= FRESH_MS).then_some(report)
}

pub fn status(shared: &SharedTab, listening: bool) -> Status {
    let last = shared
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|r| r.at_ms);
    Status {
        listening,
        last_report_ms: last,
    }
}

/// Requests have to come from a browser extension; anything else is turned away.
fn from_extension(origin: Option<&str>) -> bool {
    origin
        .is_some_and(|o| o.starts_with("chrome-extension://") || o.starts_with("moz-extension://"))
}

/// Starts the listener; false when the port is taken, e.g. by another program.
pub fn spawn(shared: SharedTab) -> bool {
    let server = match Server::http(ADDR) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("browser: cannot listen on {ADDR}: {e}");
            return false;
        }
    };
    thread::Builder::new()
        .name("kairo-browser".into())
        .spawn(move || serve(&server, &shared))
        .expect("failed to start browser listener");
    true
}

fn serve(server: &Server, shared: &SharedTab) {
    for mut request in server.incoming_requests() {
        let origin = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Origin"))
            .map(|h| h.value.as_str().to_string());
        let code = if !from_extension(origin.as_deref()) {
            403
        } else if *request.method() == Method::Post && request.url() == "/tab" {
            let mut body = String::new();
            let read = request.as_reader().take(MAX_BODY).read_to_string(&mut body);
            match read
                .ok()
                .and_then(|_| serde_json::from_str::<Payload>(&body).ok())
            {
                Some(p) => {
                    let site = p.host.as_deref().and_then(site_for_host);
                    let report = Report {
                        site,
                        incognito: p.incognito,
                        at_ms: chrono::Utc::now().timestamp_millis(),
                    };
                    *shared.lock().unwrap_or_else(|e| e.into_inner()) = Some(report);
                    204
                }
                None => 400,
            }
        } else {
            404
        };
        let mut response = Response::empty(code);
        if let Ok(h) = Header::from_bytes("Cache-Control", "no-store") {
            response.add_header(h);
        }
        let _ = request.respond(response);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_map_to_familiar_site_names() {
        assert_eq!(site_for_host("www.youtube.com").as_deref(), Some("youtube"));
        assert_eq!(site_for_host("m.youtube.com").as_deref(), Some("youtube"));
        assert_eq!(site_for_host("old.reddit.com").as_deref(), Some("reddit"));
        assert_eq!(
            site_for_host("en.wikipedia.org").as_deref(),
            Some("wikipedia")
        );
        assert_eq!(site_for_host("www.amazon.co.uk").as_deref(), Some("amazon"));
        assert_eq!(
            site_for_host("news.ycombinator.com").as_deref(),
            Some("news.ycombinator.com")
        );
        assert_eq!(
            site_for_host("WWW.Example.COM.").as_deref(),
            Some("example.com")
        );
        assert_eq!(site_for_host("notx.com").as_deref(), Some("notx.com"));
        assert_eq!(site_for_host("localhost"), None);
        assert_eq!(site_for_host("bad host/<script>"), None);
    }

    /// Sends a raw request to the listener and returns the status code.
    fn post(addr: std::net::SocketAddr, origin: Option<&str>, body: &str) -> u16 {
        use std::io::Write;
        let mut stream = std::net::TcpStream::connect(addr).unwrap();
        let origin = origin
            .map(|o| format!("Origin: {o}\r\n"))
            .unwrap_or_default();
        write!(stream, "POST /tab HTTP/1.1\r\nHost: {addr}\r\n{origin}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
            .split(' ')
            .nth(1)
            .and_then(|c| c.parse().ok())
            .unwrap()
    }

    #[test]
    fn the_listener_takes_reports_from_extensions_only() {
        let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
        let addr = server.server_addr().to_ip().unwrap();
        let shared = SharedTab::default();
        let (s, t) = (server.clone(), shared.clone());
        thread::spawn(move || serve(&s, &t));

        let ext = Some("chrome-extension://kairo");
        assert_eq!(
            post(addr, Some("https://evil.example"), r#"{"host":"x.com"}"#),
            403
        );
        assert!(shared.lock().unwrap().is_none());
        assert_eq!(post(addr, ext, "not json"), 400);
        assert_eq!(
            post(addr, ext, r#"{"host":"www.youtube.com","incognito":false}"#),
            204
        );
        let report = shared.lock().unwrap().clone().unwrap();
        assert_eq!(
            (report.site.as_deref(), report.incognito),
            (Some("youtube"), false)
        );
        assert_eq!(post(addr, ext, r#"{"host":null}"#), 204);
        assert_eq!(shared.lock().unwrap().as_ref().unwrap().site, None);
        server.unblock();
    }

    #[test]
    fn only_extensions_get_in_and_old_reports_expire() {
        assert!(from_extension(Some("chrome-extension://abcdef")));
        assert!(!from_extension(Some("https://evil.example")));
        assert!(!from_extension(None));
        let shared = SharedTab::default();
        *shared.lock().unwrap() = Some(Report {
            site: Some("github".into()),
            incognito: false,
            at_ms: 1_000,
        });
        assert!(fresh(&shared, 1_000 + FRESH_MS).is_some());
        assert!(fresh(&shared, 1_001 + FRESH_MS).is_none());
    }
}
