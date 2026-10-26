//! The browser extension's side door: it reports the active tab's domain to a listener on
//! localhost, so sites are known exactly instead of guessed from window titles.

use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Method, Response, Server};

use crate::blocking::{self, Allowances, Waits};
use crate::db;

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

/// What the browser in front said about its tab when asked: through Apple Events on a Mac, its
/// address bar on Windows (empty on Linux): the browser's executable name, and the report.
pub type SharedScripted = Arc<Mutex<Option<(String, Report)>>>;

/// The browser's own answer, if it's about `exe` and recent enough to trust. Asked every second,
/// so anything older means the browser stopped answering.
pub fn scripted_for(shared: &SharedScripted, exe: &str, now_ms: i64) -> Option<Report> {
    let guard = shared.lock().unwrap_or_else(|e| e.into_inner());
    let (from, report) = guard.as_ref()?;
    (from == exe && now_ms - report.at_ms <= SCRIPTED_FRESH_MS).then(|| report.clone())
}

const SCRIPTED_FRESH_MS: i64 = 3_000;

/// Browsers that answer Apple Events about their tabs: (executable name, bundle id, Chromium).
/// Firefox and Opera don't say which page they show.
#[cfg(any(target_os = "macos", test))]
const SCRIPTABLE: &[(&str, &str, bool)] = &[
    ("safari", "com.apple.Safari", false),
    ("chrome", "com.google.Chrome", true),
    ("brave", "com.brave.Browser", true),
    ("msedge", "com.microsoft.edgemac", true),
    ("vivaldi", "com.vivaldi.Vivaldi", true),
    ("arc", "company.thebrowser.Browser", true),
];

/// The AppleScript that asks a browser for the front tab's address and, for Chromium browsers,
/// whether the window is incognito. It never starts a browser that has just quit.
#[cfg(any(target_os = "macos", test))]
fn tab_script(exe: &str) -> Option<String> {
    let &(_, id, chromium) = SCRIPTABLE.iter().find(|(e, _, _)| *e == exe)?;
    let ask = if chromium {
        // Not every Chromium browser knows `mode`; those just count as normal windows.
        "set w to front window\n\
         set m to \"normal\"\n\
         try\n\
         set m to mode of w\n\
         end try\n\
         return (URL of active tab of w) & linefeed & m"
    } else {
        "return URL of current tab of front window"
    };
    Some(format!(
        "if application id \"{id}\" is running then\n\
         tell application id \"{id}\"\n\
         if (count of windows) is 0 then return \"\"\n\
         {ask}\n\
         end tell\n\
         end if\n\
         return \"\""
    ))
}

/// The hostname in a web address; None for anything but http and https pages.
#[cfg(any(target_os = "macos", windows, test))]
fn host_of(url: &str) -> Option<&str> {
    let (scheme, rest) = url.trim().split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?.split(':').next()?;
    (!host.is_empty()).then_some(host)
}

/// Reads a browser's answer: the address on the first line, the window's mode on the second.
#[cfg(any(target_os = "macos", test))]
fn read_answer(out: &str, now_ms: i64) -> Report {
    let mut lines = out.lines();
    let site = lines.next().and_then(host_of).and_then(site_for_host);
    let incognito = lines.next().is_some_and(|m| m.trim() == "incognito");
    Report { site, incognito, at_ms: now_ms }
}

/// Asks the browser in front which page it shows, once a second, so sites are exact without the
/// extension, including in Safari. macOS asks once per browser whether Kairo may; until then, or
/// after a no, the browser is asked again only every minute.
#[cfg(target_os = "macos")]
pub fn spawn_scripted(shared: SharedScripted, paused: Arc<std::sync::atomic::AtomicBool>) {
    use std::collections::HashMap;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    const RETRY_DENIED_MS: i64 = 60_000;

    thread::Builder::new()
        .name("kairo-tabs".into())
        .spawn(move || {
            let mut denied: HashMap<String, i64> = HashMap::new();
            loop {
                thread::sleep(Duration::from_secs(1));
                if paused.load(Ordering::Relaxed) {
                    continue;
                }
                let Some(path) = crate::platform::frontmost_path() else { continue };
                let exe = crate::classify::exe_stem(&path);
                let Some(script) = tab_script(&exe) else { continue };
                let now = chrono::Utc::now().timestamp_millis();
                if denied.get(&exe).is_some_and(|&until| now < until) {
                    continue;
                }
                match run_script(&script) {
                    Ok(out) => {
                        let at = chrono::Utc::now().timestamp_millis();
                        *shared.lock().unwrap_or_else(|e| e.into_inner()) = Some((exe, read_answer(&out, at)));
                    }
                    Err(true) => {
                        denied.insert(exe, now + RETRY_DENIED_MS);
                    }
                    // E.g. Safari's settings window in front, which has no tab.
                    Err(false) => {}
                }
            }
        })
        .expect("failed to start tab reader");
}

/// Runs AppleScript through osascript. The error says whether macOS refused to let Kairo ask.
#[cfg(target_os = "macos")]
fn run_script(script: &str) -> Result<String, bool> {
    use std::time::{Duration, Instant};

    // Long enough to answer the first-time permission prompt; a browser that hangs longer than
    // this is given up on for one round.
    const TIMEOUT: Duration = Duration::from_secs(60);
    // errAEEventNotPermitted: the user said no, or hasn't answered yet.
    const NOT_PERMITTED: &str = "-1743";

    let mut child = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|_| false)?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < TIMEOUT => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(false);
            }
        }
    }
    let out = child.wait_with_output().map_err(|_| false)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).contains(NOT_PERMITTED))
    }
}

/// Browsers whose address bar Kairo reads on Windows.
#[cfg(any(windows, test))]
const READABLE: &[&str] = &["chrome", "msedge", "brave", "vivaldi", "opera", "chromium", "firefox"];

/// The site an address bar's text is on: the address usually shows without "https://", and
/// browser pages like chrome://settings have none. None when it isn't an address at all, as when
/// Chrome shows a Google search's words there instead.
#[cfg(any(windows, test))]
fn address_site(text: &str) -> Option<Option<String>> {
    let text = text.trim();
    let scheme = text.split_once("://").map(|(scheme, _)| scheme);
    // A new tab, or the browser's own pages, like chrome://settings or Firefox's about:config.
    if text.is_empty() || text.starts_with("about:") || scheme.is_some_and(|s| !s.eq_ignore_ascii_case("http") && !s.eq_ignore_ascii_case("https")) {
        return Some(None);
    }
    let address = if scheme.is_some() { text.to_string() } else { format!("https://{text}") };
    let host = host_of(&address)?;
    let looks_like_host = !host.contains(char::is_whitespace) && (host.contains('.') || host.eq_ignore_ascii_case("localhost"));
    looks_like_host.then(|| site_for_host(host))
}

/// Reads the address bar of the browser in front, once a second, so sites are exact without the
/// extension, as on a Mac. While the extension reports, it already says, so this stays out of the
/// way; reading an address bar makes the browser describe its window to Kairo.
#[cfg(windows)]
pub fn spawn_scripted(shared: SharedScripted, paused: Arc<std::sync::atomic::AtomicBool>, tab: SharedTab) {
    use std::collections::HashMap;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use crate::platform::{Address, AddressBars};

    thread::Builder::new()
        .name("kairo-tabs".into())
        .spawn(move || {
            let Some(mut bars) = AddressBars::new() else {
                eprintln!("browser: UI Automation isn't available");
                return;
            };
            // Each window's site from before someone started typing in its address bar.
            let mut shown: HashMap<usize, Option<String>> = HashMap::new();
            loop {
                thread::sleep(Duration::from_secs(1));
                let now = chrono::Utc::now().timestamp_millis();
                if paused.load(Ordering::Relaxed) || fresh(&tab, now).is_some() {
                    continue;
                }
                let Some(fg) = crate::platform::foreground() else { continue };
                let exe = crate::classify::exe_stem(&fg.exe_path);
                if !READABLE.contains(&exe.as_str()) {
                    continue;
                }
                let site = match bars.read(fg.window, exe == "firefox") {
                    Some(Address::Shows(text)) => match address_site(&text) {
                        Some(site) => {
                            shown.insert(fg.window, site.clone());
                            site
                        }
                        None => continue,
                    },
                    Some(Address::Typing) => match shown.get(&fg.window) {
                        Some(site) => site.clone(),
                        None => continue,
                    },
                    None => continue,
                };
                if shown.len() > 64 {
                    shown.retain(|&w, _| w == fg.window);
                }
                *shared.lock().unwrap_or_else(|e| e.into_inner()) = Some((exe, Report { site, incognito: false, at_ms: now }));
            }
        })
        .expect("failed to start tab reader");
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
pub fn spawn(shared: SharedTab, db_path: PathBuf, allowances: Allowances, waits: Waits) -> bool {
    let server = match Server::http(ADDR) {
        Ok(server) => server,
        Err(e) => {
            eprintln!("browser: cannot listen on {ADDR}: {e}");
            return false;
        }
    };
    thread::Builder::new()
        .name("kairo-browser".into())
        .spawn(move || match db::open(&db_path) {
            Ok(conn) => serve(&server, &shared, &conn, &allowances, &waits),
            Err(e) => eprintln!("browser: cannot open database: {e}"),
        })
        .expect("failed to start browser listener");
    true
}

/// The answer to a report: whether to swap the tab for the blocked page, and until when.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Verdict {
    block: bool,
    site: Option<String>,
    until_ms: Option<i64>,
}

#[derive(Deserialize)]
struct Allow {
    site: String,
}

fn read_json<T: serde::de::DeserializeOwned>(request: &mut tiny_http::Request) -> Option<T> {
    let mut body = String::new();
    request.as_reader().take(MAX_BODY).read_to_string(&mut body).ok()?;
    serde_json::from_str(&body).ok()
}

/// Strict mode's answer to "Allow 5 minutes": ask again at this time.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Wait {
    ready_at: i64,
}

fn serve(server: &Server, shared: &SharedTab, conn: &Connection, allowances: &Allowances, waits: &Waits) {
    for mut request in server.incoming_requests() {
        let origin = request.headers().iter().find(|h| h.field.equiv("Origin")).map(|h| h.value.as_str().to_string());
        let now = chrono::Utc::now().timestamp_millis();
        let post = *request.method() == Method::Post;
        let (code, body) = match request.url() {
            _ if !from_extension(origin.as_deref()) => (403, None),
            "/tab" if post => match read_json::<Payload>(&mut request) {
                Some(p) => {
                    let site = p.host.as_deref().and_then(site_for_host);
                    let until = match &site {
                        Some(s) if !p.incognito => blocking::site_blocked_until(conn, allowances, s, now).unwrap_or_else(|e| {
                            eprintln!("browser: cannot check blocking: {e}");
                            None
                        }),
                        _ => None,
                    };
                    let verdict = Verdict { block: until.is_some(), site: site.clone(), until_ms: until };
                    *shared.lock().unwrap_or_else(|e| e.into_inner()) = Some(Report { site, incognito: p.incognito, at_ms: now });
                    (200, serde_json::to_string(&verdict).ok())
                }
                None => (400, None),
            },
            // "Allow 5 minutes" on the extension's blocked page; strict mode answers 423 with
            // when to ask again.
            "/allow" if post => match read_json::<Allow>(&mut request).and_then(|a| site_for_host(&a.site)) {
                Some(site) => {
                    let key = format!("site:{site}");
                    match blocking::strict_gate(conn, waits, &format!("allow:{key}"), now) {
                        Ok(Ok(())) => {
                            blocking::allow(allowances, &key, now);
                            (204, None)
                        }
                        Ok(Err(ready_at)) => (423, serde_json::to_string(&Wait { ready_at }).ok()),
                        Err(e) => {
                            eprintln!("browser: cannot check strict mode: {e}");
                            (500, None)
                        }
                    }
                }
                None => (400, None),
            },
            _ => (404, None),
        };
        let mut response = match body {
            Some(json) => {
                let mut r = Response::from_string(json).with_status_code(code);
                if let Ok(h) = Header::from_bytes("Content-Type", "application/json") {
                    r.add_header(h);
                }
                r.boxed()
            }
            None => Response::empty(code).boxed(),
        };
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

    #[test]
    fn web_addresses_give_their_host() {
        assert_eq!(host_of("https://www.youtube.com/watch?v=1"), Some("www.youtube.com"));
        assert_eq!(host_of("http://user:pw@example.com:8080/x"), Some("example.com"));
        assert_eq!(host_of("HTTPS://GitHub.com#top"), Some("GitHub.com"));
        assert_eq!(host_of("https://docs.rs?q=1"), Some("docs.rs"));
        assert_eq!(host_of("chrome://extensions"), None);
        assert_eq!(host_of("file:///Users/me/a.pdf"), None);
        assert_eq!(host_of("missing value"), None);
        assert_eq!(host_of(""), None);
    }

    #[test]
    fn address_bars_give_the_site_they_show() {
        let youtube = site_for_host("youtube.com");
        assert_eq!(address_site("youtube.com/watch?v=1"), Some(youtube.clone()));
        assert_eq!(address_site("https://www.youtube.com/watch?v=1"), Some(youtube));
        assert_eq!(address_site("docs.rs/serde"), Some(site_for_host("docs.rs")));
        // The browser's own pages and a new tab have no site.
        assert_eq!(address_site("chrome://settings"), Some(None));
        assert_eq!(address_site("about:preferences"), Some(None));
        assert_eq!(address_site(""), Some(None));
        // Search words Chrome shows instead of the address say nothing.
        assert_eq!(address_site("rust lifetimes"), None);
        assert_eq!(address_site("rust"), None);
        assert!(READABLE.contains(&"firefox"));
    }

    #[test]
    fn browser_answers_become_reports() {
        assert_eq!(read_answer("https://www.youtube.com/watch?v=1\nnormal\n", 5), Report { site: Some("youtube".into()), incognito: false, at_ms: 5 });
        assert_eq!(read_answer("https://reddit.com/\nincognito\n", 5), Report { site: Some("reddit".into()), incognito: true, at_ms: 5 });
        // Safari says nothing about the window.
        assert_eq!(read_answer("https://github.com/rust-lang\n", 5).site.as_deref(), Some("github"));
        assert_eq!(read_answer("chrome://newtab/\nnormal\n", 5).site, None);
        assert_eq!(read_answer("", 5).site, None);
    }

    #[test]
    fn only_scriptable_browsers_get_a_script() {
        let chrome = tab_script("chrome").unwrap();
        assert!(chrome.contains("application id \"com.google.Chrome\" is running"));
        assert!(chrome.contains("mode of w"));
        assert!(tab_script("safari").unwrap().contains("current tab of front window"));
        assert!(tab_script("firefox").is_none());
        assert!(tab_script("code").is_none());
        assert!(SCRIPTABLE.iter().all(|(exe, _, _)| crate::classify::is_browser(exe)));
    }

    #[test]
    fn scripted_answers_count_only_for_their_browser_while_fresh() {
        let shared = SharedScripted::default();
        assert_eq!(scripted_for(&shared, "safari", 0), None);
        let report = Report { site: Some("github".into()), incognito: false, at_ms: 1_000 };
        *shared.lock().unwrap() = Some(("safari".into(), report.clone()));
        assert_eq!(scripted_for(&shared, "safari", 2_000), Some(report));
        assert_eq!(scripted_for(&shared, "chrome", 2_000), None);
        assert_eq!(scripted_for(&shared, "safari", 1_000 + SCRIPTED_FRESH_MS + 1), None);
    }

    /// Sends a raw request to the listener; returns the status code and body.
    fn post(addr: std::net::SocketAddr, path: &str, origin: Option<&str>, body: &str) -> (u16, String) {
        use std::io::Write;
        let mut stream = std::net::TcpStream::connect(addr).unwrap();
        let origin = origin.map(|o| format!("Origin: {o}\r\n")).unwrap_or_default();
        write!(stream, "POST {path} HTTP/1.1\r\nHost: {addr}\r\n{origin}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let code = response.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap();
        let body = response.split("\r\n\r\n").nth(1).unwrap_or_default().to_string();
        (code, body)
    }

    #[test]
    fn the_listener_takes_reports_from_extensions_only_and_says_what_to_block() {
        let path = std::env::temp_dir().join(format!("kairo-browser-{}.db", std::process::id()));
        let conn = db::open(&path).unwrap();
        let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
        let addr = server.server_addr().to_ip().unwrap();
        let shared = SharedTab::default();
        let allowances = Allowances::default();
        let (s, t, a, p) = (server.clone(), shared.clone(), allowances.clone(), path.clone());
        thread::spawn(move || serve(&s, &t, &db::open(&p).unwrap(), &a, &Waits::default()));

        let ext = Some("chrome-extension://kairo");
        let youtube = r#"{"host":"www.youtube.com","incognito":false}"#;
        assert_eq!(post(addr, "/tab", Some("https://evil.example"), r#"{"host":"x.com"}"#).0, 403);
        assert!(shared.lock().unwrap().is_none());
        assert_eq!(post(addr, "/tab", ext, "not json").0, 400);
        assert_eq!(post(addr, "/tab", ext, youtube), (200, r#"{"block":false,"site":"youtube","untilMs":null}"#.into()));
        let report = shared.lock().unwrap().clone().unwrap();
        assert_eq!((report.site.as_deref(), report.incognito), (Some("youtube"), false));

        // During a focus session, a distracting site comes back blocked until "Allow 5 minutes".
        let now = chrono::Utc::now().timestamp_millis();
        crate::focus::start(&conn, 25, None, now).unwrap();
        let (code, body) = post(addr, "/tab", ext, youtube);
        assert_eq!(code, 200);
        assert!(body.starts_with(r#"{"block":true,"site":"youtube","untilMs":"#), "{body}");
        assert_eq!(post(addr, "/allow", ext, r#"{"site":"www.youtube.com"}"#).0, 204);
        assert!(post(addr, "/tab", ext, youtube).1.starts_with(r#"{"block":false"#));

        // In strict mode, allowing waits first and the listener says when to ask again.
        blocking::set_setting(&conn, "strict", true).unwrap();
        let (code, body) = post(addr, "/allow", ext, r#"{"site":"reddit.com"}"#);
        assert_eq!(code, 423);
        assert!(body.starts_with(r#"{"readyAt":"#), "{body}");

        assert_eq!(post(addr, "/tab", ext, r#"{"host":null}"#).0, 200);
        assert_eq!(shared.lock().unwrap().as_ref().unwrap().site, None);
        server.unblock();
        drop(conn);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
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
