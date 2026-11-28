//! A web address typed into the palette (`github.com`, `localhost:3000`), told apart from a file
//! name, a sum or a search. No drawing here, so all of it can be tested.

/// The endings taken for a web address on their own. Short on purpose: `notes.md`, `main.rs` and
/// `setup.sh` are files, though their endings are countries too. With `http://`, `www.` or a path,
/// any ending will do.
const ENDINGS: &str = "com net org edu gov int io dev app ai co me tv gg xyz info biz site online store tech blog cloud \
    page wiki design art ly fm im cc news live chat sg my id th vn ph jp kr cn hk tw in au nz uk ie de fr es it nl be \
    ch at se no dk fi pl pt cz sk hu ro gr ru ua tr il ae sa pk bd lk za ng ke eg br ar mx cl pe ca us eu asia";

fn host_label(label: &str) -> bool {
    !label.is_empty() && !label.starts_with('-') && !label.ends_with('-') && label.chars().all(|c| c.is_alphanumeric() || c == '-')
}

/// The address to open for what was typed, with `https://` put in front when it has no scheme
/// (`http://` for this computer and bare IP addresses, which seldom have a certificate). None
/// when it isn't an address.
pub fn typed(query: &str) -> Option<String> {
    let q = query.trim();
    if q.is_empty() || q.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let lower = q.to_lowercase();
    let scheme = ["https://", "http://"].into_iter().find(|s| lower.starts_with(s));
    let rest = &q[scheme.map_or(0, str::len)..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    // A name before an @ makes it an email address, or a link that isn't what it looks like.
    if authority.contains('@') {
        return None;
    }
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if port.is_some_and(|p| !p.chars().all(|c| c.is_ascii_digit()) || p.parse::<u16>().is_err()) {
        return None;
    }
    let labels: Vec<&str> = host.strip_suffix('.').unwrap_or(host).split('.').collect();
    if !labels.iter().all(|l| host_label(l)) {
        return None;
    }
    let local = host.eq_ignore_ascii_case("localhost");
    let numbers = labels.iter().all(|l| l.chars().all(|c| c.is_ascii_digit()));
    let ip = labels.len() == 4 && labels.iter().all(|l| l.parse::<u8>().is_ok());
    let known = if scheme.is_some() {
        true
    } else if local {
        // The word alone is a search; a port or a path says it's the address.
        port.is_some() || !tail.is_empty()
    } else if numbers {
        ip
    } else {
        let ending = labels.last()?.to_lowercase();
        let lettered = ending.chars().count() >= 2 && ending.chars().all(char::is_alphabetic);
        let said = labels[0].eq_ignore_ascii_case("www") && labels.len() >= 3;
        labels.len() >= 2 && lettered && (said || tail.starts_with('/') || ENDINGS.split_whitespace().any(|e| e == ending))
    };
    if !known {
        return None;
    }
    Some(match scheme {
        Some(_) => q.to_string(),
        None if local || ip => format!("http://{q}"),
        None => format!("https://{q}"),
    })
}

#[cfg(test)]
mod tests {
    use super::typed;

    #[test]
    fn addresses_get_a_scheme_when_they_have_none() {
        for (q, url) in [
            ("github.com", "https://github.com"),
            ("  GitHub.com  ", "https://GitHub.com"),
            ("news.ycombinator.com/item?id=1", "https://news.ycombinator.com/item?id=1"),
            ("www.example.museum", "https://www.example.museum"),
            ("docs.rs/serde", "https://docs.rs/serde"),
            ("linear.app", "https://linear.app"),
            ("bbc.co.uk", "https://bbc.co.uk"),
            ("münchen.de", "https://münchen.de"),
            ("localhost:3000", "http://localhost:3000"),
            ("localhost/admin", "http://localhost/admin"),
            ("192.168.1.1", "http://192.168.1.1"),
            ("127.0.0.1:8787/health", "http://127.0.0.1:8787/health"),
            ("http://intranet", "http://intranet"),
            ("HTTPS://Example.com/A", "HTTPS://Example.com/A"),
        ] {
            assert_eq!(typed(q).as_deref(), Some(url), "{q}");
        }
    }

    #[test]
    fn files_sums_words_and_emails_are_not_addresses() {
        for q in [
            "", "github", "localhost", "notes.md", "main.rs", "setup.sh", "report.pdf", "photo.jpg", "archive.zip", "3.14", "1.2.3", "300.1.1.1", "e.g.", "version 2.com",
            "me@example.com", "https://me@example.com", "example.com:abc", "example.com:99999", "-bad.com", "a..com", "https://", "src/main.rs", "12/4", "mailto:me@x.com",
        ] {
            assert_eq!(typed(q), None, "{q}");
        }
    }
}
