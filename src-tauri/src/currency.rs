//! Exchange rates for the palette's calculator ("20 usd in eur"): the European Central Bank's
//! daily reference rates, a public file with no account or key. They are downloaded only once a
//! currency conversion is typed, at most once a day after that, and kept in Kairo's settings so
//! they work offline; an answer says which day's rates it used. Nothing is sent but the request
//! for the file.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const SOURCE: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml";
const KEY: &str = "currency_rates";
/// Rates this old are asked for again (the bank publishes once each working day).
const STALE_MS: i64 = 24 * 60 * 60 * 1000;

/// The currencies the bank gives a rate for, and the euro, its base.
pub const CODES: [&str; 31] = [
    "EUR", "USD", "JPY", "BGN", "CZK", "DKK", "GBP", "HUF", "PLN", "RON", "SEK", "CHF", "ISK", "NOK", "TRY", "AUD", "BRL", "CAD", "CNY", "HKD", "IDR", "ILS", "INR", "KRW", "MXN", "MYR", "NZD", "PHP", "SGD", "THB", "ZAR",
];
/// Other ways currencies are written, which fend knows.
/// Not "pound": that's a weight as often as money, and "5 pounds in kg" isn't money ("£" and "gbp"
/// are).
const WORDS: [&str; 10] = ["$", "€", "£", "¥", "dollar", "dollars", "euro", "euros", "yen", "us$"];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Rates {
    /// The day the bank set them.
    #[serde(with = "day")]
    pub date: NaiveDate,
    /// How many of each currency one euro buys.
    pub per_euro: HashMap<String, f64>,
    /// When they were downloaded.
    pub fetched_ms: i64,
}

/// A day kept as 2026-11-20.
mod day {
    use chrono::NaiveDate;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(d: &NaiveDate, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&d.format("%Y-%m-%d").to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<NaiveDate, D::Error> {
        let text = String::deserialize(d)?;
        NaiveDate::parse_from_str(&text, "%Y-%m-%d").map_err(serde::de::Error::custom)
    }
}

static RATES: RwLock<Option<Rates>> = RwLock::new(None);
static APP: OnceLock<AppHandle> = OnceLock::new();
static FETCHING: AtomicBool = AtomicBool::new(false);
/// When a download was last started, and how long after it another may be.
static LAST_TRY: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(i64::MIN / 2);
const RETRY_MS: i64 = 5 * 60_000;

/// Whether an answer is an amount of money: it ends in a currency code or starts with a symbol.
pub fn is_money(answer: &str) -> bool {
    let last = answer.split_whitespace().last().unwrap_or("");
    code(last).is_some() || answer.trim_start().starts_with(['$', '€', '£', '¥'])
}

/// Reads the kept rates, and remembers the app so a download can be started from the calculator.
/// Called as Kairo starts, before its state is managed, so it is handed the database itself.
pub fn init(app: &AppHandle, conn: &rusqlite::Connection) {
    let _ = APP.set(app.clone());
    if let Some(rates) = crate::db::get_setting(conn, KEY).ok().flatten().and_then(|json| serde_json::from_str::<Rates>(&json).ok()) {
        *RATES.write().unwrap_or_else(|e| e.into_inner()) = Some(rates);
    }
}

/// Whether a query names a currency: a code (usd), a symbol ($) or a word (euros).
pub fn mentions_currency(query: &str) -> bool {
    query.split(|c: char| c.is_whitespace() || c.is_ascii_digit() || c == '.' || c == ',').filter(|w| !w.is_empty()).any(|w| {
        let lower = w.to_lowercase();
        CODES.iter().any(|c| c.eq_ignore_ascii_case(w)) || WORDS.contains(&lower.as_str()) || w.starts_with(['$', '€', '£', '¥'])
    })
}

/// A currency code typed in lower case, in the capitals fend knows it by.
pub fn code(word: &str) -> Option<&'static str> {
    CODES.iter().find(|c| c.eq_ignore_ascii_case(word)).copied()
}

/// The day of the rates in use, if any.
pub fn rates_date() -> Option<NaiveDate> {
    RATES.read().unwrap_or_else(|e| e.into_inner()).as_ref().map(|r| r.date)
}

/// How many of `currency` one euro buys, from the rates in use.
fn per_euro(currency: &str) -> Option<f64> {
    if currency == "EUR" {
        return Some(1.0);
    }
    RATES.read().unwrap_or_else(|e| e.into_inner()).as_ref()?.per_euro.get(currency).copied()
}

/// Gives fend the rates, with the euro as its base.
pub struct Handler;

impl fend_core::ExchangeRateFnV2 for Handler {
    fn relative_to_base_currency(&self, currency: &str, _: &fend_core::ExchangeRateFnV2Options) -> Result<f64, Box<dyn std::error::Error + Send + Sync + 'static>> {
        per_euro(currency).ok_or_else(|| format!("no rate for {currency}").into())
    }
}

/// Starts a download when the rates are missing or a day old and none is under way. Called as a
/// currency conversion is typed, so nothing is downloaded for someone who never converts.
pub fn refresh_if_old(now_ms: i64) {
    let old = RATES.read().unwrap_or_else(|e| e.into_inner()).as_ref().is_none_or(|r| now_ms - r.fetched_ms > STALE_MS);
    // Tried lately and failed (offline, say): not again on every keystroke.
    if !old || now_ms - LAST_TRY.load(Ordering::SeqCst) < RETRY_MS || FETCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    LAST_TRY.store(now_ms, Ordering::SeqCst);
    let Some(app) = APP.get().cloned() else {
        FETCHING.store(false, Ordering::SeqCst);
        return;
    };
    tauri::async_runtime::spawn(async move {
        if let Some(rates) = download(now_ms).await {
            if let Ok(json) = serde_json::to_string(&rates) {
                let state = app.state::<crate::AppState>();
                let saved = state.db.lock().map(|conn| crate::db::set_setting(&conn, KEY, &json));
                let _ = saved;
            }
            *RATES.write().unwrap_or_else(|e| e.into_inner()) = Some(rates);
        }
        FETCHING.store(false, Ordering::SeqCst);
    });
}

/// Whether rates are being downloaded now.
pub fn fetching() -> bool {
    FETCHING.load(Ordering::SeqCst)
}

async fn download(now_ms: i64) -> Option<Rates> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let client = reqwest::Client::builder().timeout(Duration::from_secs(15)).build().ok()?;
    let body = client.get(SOURCE).send().await.ok()?.error_for_status().ok()?.text().await.ok()?;
    parse(&body, now_ms)
}

/// The value of `name='…'` (or with double quotes) in a tag.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let at = tag.find(&format!("{name}="))? + name.len() + 1;
    let quote = tag[at..].chars().next()?;
    let rest = &tag[at + quote.len_utf8()..];
    Some(&rest[..rest.find(quote)?])
}

/// The bank's file: `<Cube time='2026-11-20'>` holding `<Cube currency='USD' rate='1.0812'/>`s.
pub fn parse(xml: &str, now_ms: i64) -> Option<Rates> {
    let mut date = None;
    let mut per_euro = HashMap::new();
    for tag in xml.split('<').filter(|t| t.starts_with("Cube")) {
        if let Some(day) = attr(tag, "time") {
            date = NaiveDate::parse_from_str(day, "%Y-%m-%d").ok();
        }
        if let (Some(currency), Some(rate)) = (attr(tag, "currency"), attr(tag, "rate").and_then(|r| r.parse::<f64>().ok())) {
            if rate.is_finite() && rate > 0.0 {
                per_euro.insert(currency.to_string(), rate);
            }
        }
    }
    (!per_euro.is_empty()).then_some(Rates { date: date?, per_euro, fetched_ms: now_ms })
}

#[cfg(test)]
pub fn set_for_test(rates: Option<Rates>) {
    *RATES.write().unwrap_or_else(|e| e.into_inner()) = rates;
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gesmes:Envelope xmlns:gesmes="http://www.gesmes.org/xml/2002-08-01" xmlns="http://www.ecb.int/vocabulary/2002-08-01/eurofxref">
	<gesmes:subject>Reference rates</gesmes:subject>
	<Cube>
		<Cube time='2026-11-20'>
			<Cube currency='USD' rate='1.0812'/>
			<Cube currency='JPY' rate='163.42'/>
			<Cube currency='GBP' rate="0.8571"/>
			<Cube currency='BAD' rate='nope'/>
		</Cube>
	</Cube>
</gesmes:Envelope>"#;

    #[test]
    fn the_banks_file_gives_its_day_and_rates() {
        let r = parse(FILE, 5).unwrap();
        assert_eq!(r.date, NaiveDate::from_ymd_opt(2026, 11, 20).unwrap());
        assert_eq!(r.per_euro.get("USD"), Some(&1.0812));
        assert_eq!(r.per_euro.get("GBP"), Some(&0.8571));
        assert_eq!(r.per_euro.len(), 3);
        assert_eq!(r.fetched_ms, 5);
        assert!(parse("<html>Sorry</html>", 0).is_none());
        // Kept as text and read back the same.
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"2026-11-20\""));
        assert_eq!(serde_json::from_str::<Rates>(&json).unwrap(), r);
    }

    #[test]
    fn currencies_are_seen_by_code_symbol_or_word() {
        for q in ["20 usd in eur", "€15 to $", "100 GBP in yen", "3 euros in dollars", "5000 jpy to chf"] {
            assert!(mentions_currency(q), "{q}");
        }
        // A weight isn't money.
        assert!(!mentions_currency("5 pounds in kg"));
        assert!(is_money("16 EUR") && is_money("$12.50") && !is_money("2.26796 kg"));
        for q in ["5 km in mi", "2+2", "100 c in f", "16gb in mb"] {
            assert!(!mentions_currency(q), "{q}");
        }
        assert_eq!(code("usd"), Some("USD"));
        assert_eq!(code("km"), None);
    }
}
