//! Turning raw window data into something a person recognises: app names, websites, default categories.

const BROWSERS: &[&str] = &["chrome", "msedge", "firefox", "brave", "opera", "vivaldi", "arc"];

/// Sites recognised from browser window titles: (text to look for, canonical name).
const KNOWN_SITES: &[(&str, &str)] = &[
    ("youtube", "youtube"),
    ("reddit", "reddit"),
    ("github", "github"),
    ("stack overflow", "stackoverflow"),
    ("gmail", "gmail"),
    ("google docs", "google docs"),
    ("google sheets", "google sheets"),
    ("google calendar", "google calendar"),
    ("netflix", "netflix"),
    ("twitch", "twitch"),
    ("instagram", "instagram"),
    ("facebook", "facebook"),
    ("tiktok", "tiktok"),
    ("linkedin", "linkedin"),
    ("twitter", "x"),
    ("/ x", "x"),
    ("chatgpt", "chatgpt"),
    ("claude", "claude"),
    ("notion", "notion"),
    ("figma", "figma"),
    ("wikipedia", "wikipedia"),
    ("docs.rs", "docs.rs"),
    ("mdn web docs", "mdn"),
    ("amazon", "amazon"),
];

/// Sites and apps where sitting still is normal (watching, listening), so idle kicks in much later.
const PASSIVE: &[&str] = &["youtube", "netflix", "twitch", "vlc", "primevideo", "disney+"];

/// Categories shipped with Kairo. Users can override any of them; keys are `app:<exe>` or `site:<name>`.
pub const DEFAULT_CATEGORIES: &[(&str, &str)] = &[
    ("app:code", "productive"),
    ("app:cursor", "productive"),
    ("app:zed", "productive"),
    ("app:devenv", "productive"),
    ("app:idea64", "productive"),
    ("app:pycharm64", "productive"),
    ("app:rider64", "productive"),
    ("app:webstorm64", "productive"),
    ("app:sublime_text", "productive"),
    ("app:notepad++", "productive"),
    ("app:windowsterminal", "productive"),
    ("app:powershell", "productive"),
    ("app:pwsh", "productive"),
    ("app:cmd", "productive"),
    ("app:winword", "productive"),
    ("app:excel", "productive"),
    ("app:powerpnt", "productive"),
    ("app:onenote", "productive"),
    ("app:obsidian", "productive"),
    ("app:notion", "productive"),
    ("app:figma", "productive"),
    ("app:postman", "productive"),
    ("app:steam", "distracting"),
    ("app:epicgameslauncher", "distracting"),
    ("app:discord", "distracting"),
    ("app:overwatch", "distracting"),
    ("app:valorant", "distracting"),
    ("app:leagueclientux", "distracting"),
    ("app:league of legends", "distracting"),
    ("app:robloxplayerbeta", "distracting"),
    ("app:cs2", "distracting"),
    ("app:fortniteclient-win64-shipping", "distracting"),
    ("site:github", "productive"),
    ("site:stackoverflow", "productive"),
    ("site:google docs", "productive"),
    ("site:google sheets", "productive"),
    ("site:chatgpt", "productive"),
    ("site:claude", "productive"),
    ("site:notion", "productive"),
    ("site:figma", "productive"),
    ("site:docs.rs", "productive"),
    ("site:mdn", "productive"),
    ("site:youtube", "distracting"),
    ("site:reddit", "distracting"),
    ("site:x", "distracting"),
    ("site:instagram", "distracting"),
    ("site:facebook", "distracting"),
    ("site:tiktok", "distracting"),
    ("site:netflix", "distracting"),
    ("site:twitch", "distracting"),
];

/// Library folders whose next folder is a game, named the way players know it ("THE FINALS").
const GAME_LIBRARIES: &[&[&str]] = &[&["steamapps", "common"], &["epic games"], &["riot games"], &["xboxgames"], &["gog games"]];

/// Launchers and tools that live in those folders but aren't games.
const NOT_GAMES: &[&str] = &["launcher", "riot client", "steamworks shared", "steamvr", "directxredist"];

/// Games installed outside a library folder (Battle.net, Roblox), so they're only known by exe.
const GAME_EXES: &[&str] = &["overwatch", "robloxplayerbeta", "league of legends", "valorant", "cs2", "fortniteclient-win64-shipping"];

/// The game's name when the exe lives in a game library, e.g. `...\steamapps\common\THE FINALS\...\Discovery.exe`.
pub fn game_name(exe_path: &str) -> Option<String> {
    let parts: Vec<&str> = exe_path.split(['\\', '/']).filter(|p| !p.is_empty()).collect();
    for library in GAME_LIBRARIES {
        let found = parts.windows(library.len()).position(|w| w.iter().zip(*library).all(|(a, b)| a.eq_ignore_ascii_case(b)));
        let Some(i) = found else { continue };
        let folder = i + library.len();
        // The game folder must contain the exe, not be the exe itself.
        if folder + 1 >= parts.len() {
            return None;
        }
        let name = clean_title(parts[folder]);
        return (!NOT_GAMES.iter().any(|n| name.eq_ignore_ascii_case(n))).then_some(name);
    }
    None
}

pub fn is_game(exe: &str, exe_path: &str) -> bool {
    GAME_EXES.contains(&exe) || game_name(exe_path).is_some()
}

/// Window titles with invisible characters removed; some games pad their title with them.
pub fn clean_title(title: &str) -> String {
    title
        .chars()
        .filter(|c| !matches!(c, '\u{00ad}' | '\u{200b}'..='\u{200f}' | '\u{2060}'..='\u{2064}' | '\u{feff}'))
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn exe_stem(exe_path: &str) -> String {
    let file = exe_path.rsplit(['\\', '/']).next().unwrap_or(exe_path);
    let stem = file.strip_suffix(".exe").or_else(|| file.strip_suffix(".EXE")).unwrap_or(file);
    stem.to_lowercase()
}

pub fn is_browser(exe: &str) -> bool {
    BROWSERS.contains(&exe)
}

pub fn display_name(exe: &str, title: &str) -> String {
    let name = match exe {
        "code" => "VS Code",
        "chrome" => "Chrome",
        "msedge" => "Edge",
        "firefox" => "Firefox",
        "brave" => "Brave",
        "opera" => "Opera",
        "vivaldi" => "Vivaldi",
        "explorer" => "File Explorer",
        "windowsterminal" => "Windows Terminal",
        "devenv" => "Visual Studio",
        "outlook" | "olk" => "Outlook",
        "winword" => "Word",
        "excel" => "Excel",
        "powerpnt" => "PowerPoint",
        "ms-teams" | "teams" => "Teams",
        // UWP apps all run inside this host; the window title is the best name we have.
        "applicationframehost" => return title.rsplit(" - ").next().unwrap_or(title).trim().to_string(),
        _ => {
            let mut chars = exe.chars();
            return match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => exe.to_string(),
            };
        }
    };
    name.to_string()
}

/// Best guess at the site a browser tab shows, from the window title alone.
/// A browser extension will replace this with the real hostname later.
pub fn site_from_title(title: &str) -> Option<String> {
    // Edge puts a zero-width space inside "Microsoft Edge".
    let clean = title.replace('\u{200b}', "").to_lowercase();
    let mut parts: Vec<&str> = clean
        .split(" - ")
        .flat_map(|p| p.split(" — "))
        .flat_map(|p| p.split(" | "))
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if parts.last().is_some_and(|p| is_browser_suffix(p)) {
        parts.pop();
    }

    // Sites usually put their own name at the end of the title.
    for part in parts.iter().rev().take(3) {
        if let Some((_, site)) = KNOWN_SITES.iter().find(|(needle, _)| part.contains(needle)) {
            return Some(site.to_string());
        }
    }
    // Some pages show a bare domain ("docs.rs", "news.ycombinator.com").
    parts
        .last()
        .filter(|p| parts.len() > 1 && !p.contains(' ') && p.contains('.') && p.len() <= 40)
        .map(|p| p.to_string())
}

fn is_browser_suffix(part: &str) -> bool {
    ["google chrome", "microsoft edge", "mozilla firefox"].iter().any(|b| part.contains(b))
        || ["brave", "opera", "vivaldi", "arc"].contains(&part)
}

/// Incognito, InPrivate and private browsing windows, as far as their titles give them away.
pub fn is_private_window(title: &str) -> bool {
    let title = title.to_lowercase();
    ["incognito", "inprivate", "private browsing", "private window"].iter().any(|m| title.contains(m))
}

pub fn is_passive(exe: &str, site: Option<&str>) -> bool {
    PASSIVE.contains(&exe) || site.is_some_and(|s| PASSIVE.contains(&s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_sites_from_browser_titles() {
        assert_eq!(site_from_title("Lofi beats - YouTube - Google Chrome").as_deref(), Some("youtube"));
        assert_eq!(site_from_title("Home / X - Google Chrome").as_deref(), Some("x"));
        assert_eq!(site_from_title("rusqlite - Rust - docs.rs - Personal - Microsoft\u{200b} Edge").as_deref(), Some("docs.rs"));
        assert_eq!(site_from_title("Some blog post - Mozilla Firefox"), None);
    }

    #[test]
    fn exe_names_are_normalised() {
        assert_eq!(exe_stem(r"C:\Program Files\Google\Chrome\Application\chrome.exe"), "chrome");
        assert_eq!(display_name("code", ""), "VS Code");
        assert_eq!(display_name("obsidian", ""), "Obsidian");
    }

    #[test]
    fn games_are_named_after_their_install_folder() {
        let finals = r"D:\SteamLibrary\steamapps\common\THE FINALS\Discovery\Binaries\Win64\Discovery.exe";
        assert_eq!(game_name(finals).as_deref(), Some("THE FINALS"));
        assert_eq!(game_name(r"C:\Riot Games\VALORANT\live\VALORANT.exe").as_deref(), Some("VALORANT"));
        assert_eq!(game_name(r"C:\Program Files\Epic Games\Launcher\Portal\Binaries\Win64\EpicGamesLauncher.exe"), None);
        assert_eq!(game_name(r"C:\Riot Games\Riot Client\RiotClientServices.exe"), None);
        assert_eq!(game_name(r"C:\Program Files (x86)\Steam\steam.exe"), None);
        assert!(is_game("overwatch", r"C:\Program Files (x86)\Overwatch\_retail_\Overwatch.exe"));
        assert!(!is_game("cursor", r"C:\Users\me\AppData\Local\Programs\cursor\Cursor.exe"));
    }

    #[test]
    fn private_browser_windows_are_recognised() {
        assert!(is_private_window("New Tab - Google Chrome (Incognito)"));
        assert!(is_private_window("[InPrivate] Bing - Microsoft Edge"));
        assert!(is_private_window("Mozilla Firefox Private Browsing"));
        assert!(!is_private_window("Privacy policy - Google Chrome"));
    }

    #[test]
    fn invisible_characters_are_stripped_from_titles() {
        assert_eq!(clean_title("\u{200b}\u{200b}\u{feff}T\u{200b}H\u{feff}E FI\u{200b}NALS"), "THE FINALS");
    }
}
