//! The palette's calculator: whether a query is maths or a unit conversion, and its answer,
//! worked out offline by fend. No drawing here, so all of it can be tested.

use std::time::{Duration, Instant};

/// How long one calculation may take, so an odd input can't stall the palette.
const LIMIT: Duration = Duration::from_millis(100);
/// Significant digits an answer is rounded to.
const DIGITS: i32 = 6;
const OPERATORS: [char; 10] = ['+', '-', '*', '/', '×', '÷', '^', '%', '(', ')'];
const CONVERSIONS: [&str; 3] = ["in", "to", "as"];
/// Data sizes and speeds as fend wants them, by how people type them in lower case; fend reads
/// `mb` as millibits and `mbps` as millibits a second. Capitals the user typed are kept.
const UNITS: [(&str, &str); 10] = [
    ("kb", "KB"),
    ("mb", "MB"),
    ("gb", "GB"),
    ("tb", "TB"),
    ("kib", "KiB"),
    ("mib", "MiB"),
    ("gib", "GiB"),
    ("tib", "TiB"),
    ("mbps", "Mbps"),
    ("gbps", "Gbps"),
];
/// How dates are written, so one is never taken for a sum.
const DATES: [&str; 5] = ["%Y-%m-%d", "%d/%m/%Y", "%Y/%m/%d", "%m/%d/%Y", "%d.%m.%Y"];

/// A worked-out answer.
#[derive(Debug, PartialEq)]
pub struct Answer {
    /// The answer with its unit, like "3.10686 mi".
    pub text: String,
    /// The number alone, like "3.10686"; None when the answer has no unit.
    pub number: Option<String>,
}

/// Where a conversion's `in`, `to` or `as` sits among the words: a number and a unit before it
/// and a unit after, like "5 km in mi".
fn conversion_at(words: &[&str]) -> Option<usize> {
    let starts_with_number = words.first()?.starts_with(|c: char| c.is_ascii_digit() || c == '.' || c == '-');
    let has_letters = |side: &[&str]| side.iter().any(|w| w.chars().any(char::is_alphabetic));
    // The first that has a unit on both sides, so the `in` of "3 in to cm" stays inches.
    (1..words.len()).find(|&at| starts_with_number && CONVERSIONS.contains(&words[at].to_lowercase().as_str()) && has_letters(&words[..at]) && has_letters(&words[at + 1..]))
}

/// Whether the query is a date, part of one being typed (2026-10), a range of years
/// (2024-2025) or a number like a phone number (555-1234): numbers joined by `-` or `/` that
/// aren't a sum anyone means.
fn date_like(query: &str) -> bool {
    let query = query.trim();
    if DATES.iter().any(|f| chrono::NaiveDate::parse_from_str(query, f).is_ok()) {
        return true;
    }
    let Some(sep) = query.chars().find(|c| *c == '-' || *c == '/') else { return false };
    let parts: Vec<&str> = query.split(sep).collect();
    let numbers = (2..=3).contains(&parts.len()) && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    numbers && (parts[0].len() == 4 || parts.iter().all(|p| p.len() >= 3))
}

/// Whether a comma is followed by other than three digits, like "1,5": a decimal comma, which fend
/// would read as a thousands separator and answer wrongly.
fn decimal_comma(query: &str) -> bool {
    query.match_indices(',').any(|(i, _)| query[i + 1..].chars().take_while(char::is_ascii_digit).count() != 3)
}

/// Whether the query is maths or a conversion: a digit, and an operator or a conversion, and not
/// a date or a decimal comma.
pub fn is_maths(query: &str) -> bool {
    let words: Vec<&str> = query.split_whitespace().collect();
    query.chars().any(|c| c.is_ascii_digit())
        && (query.contains(OPERATORS) || conversion_at(&words).is_some())
        && !date_like(query)
        && !decimal_comma(query)
}

/// A temperature unit as fend wants it, for a word that is one.
fn temperature(word: &str) -> Option<&'static str> {
    match word.to_lowercase().as_str() {
        "c" | "°c" => Some("°C"),
        "f" | "°f" => Some("°F"),
        "k" => Some("K"),
        "celsius" => Some("celsius"),
        "fahrenheit" => Some("fahrenheit"),
        "kelvin" => Some("kelvin"),
        _ => None,
    }
}

/// A lower-case data size or speed as fend wants it.
fn unit(word: &str) -> Option<&'static str> {
    UNITS.iter().find(|(typed, _)| *typed == word).map(|(_, unit)| *unit)
}

/// The query with everyday shorthand written the way fend reads it: data sizes and speeds in
/// capitals, a number stuck to one split off ("16gb" → "16 GB"), inches written out (fend takes a
/// lone `in` for the conversion), and the lone letters of a temperature conversion as degrees
/// ("30 c in f" → "30 °C in °F"; fend alone reads `c` as the speed of light).
pub fn rewrite(query: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    for word in query.split_whitespace() {
        let digits = word.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(word.len());
        match unit(&word[digits..]) {
            Some(size) if digits > 0 => words.extend([word[..digits].to_string(), size.to_string()]),
            Some(size) => words.push(size.to_string()),
            None => words.push(word.to_string()),
        }
    }
    let solid: Vec<&str> = words.iter().map(String::as_str).collect();
    let Some(at) = conversion_at(&solid) else { return words.join(" ") };
    // Only "<number> <temperature> in <temperature>", so a lone letter elsewhere stays as typed.
    if let (4, 2, Some(from), Some(to)) = (solid.len(), at, temperature(solid[1]), temperature(solid[3])) {
        return format!("{} {from} {} {to}", solid[0], solid[2]);
    }
    let inches = |(i, w): (usize, &str)| if i != at && w.eq_ignore_ascii_case("in") { "inches".to_string() } else { w.to_string() };
    solid.into_iter().enumerate().map(inches).collect::<Vec<_>>().join(" ")
}

/// Stops fend at a deadline.
struct Deadline(Instant);

impl fend_core::Interrupt for Deadline {
    fn should_interrupt(&self) -> bool {
        Instant::now() >= self.0
    }
}

/// The answer to a maths query, or None when it isn't maths, can't be worked out in time, or
/// would only repeat what was typed.
pub fn answer(query: &str) -> Option<Answer> {
    if !is_maths(query) {
        return None;
    }
    let deadline = Deadline(Instant::now() + LIMIT);
    let ask = |q: &str| fend_core::evaluate_with_interrupt(&rewrite(q), &mut fend_core::Context::new(), &deadline).ok();
    let q = query.trim();
    // fend's units are lower case ("km"); typed in capitals, they're asked again in lower case.
    let result = ask(q).or_else(|| q.chars().any(char::is_uppercase).then(|| ask(&q.to_lowercase())).flatten())?;
    let (text, number) = format(result.get_main_result())?;
    let bare = |s: &str| s.chars().filter(|c| !c.is_whitespace() && *c != '(' && *c != ')').collect::<String>();
    (bare(&text) != bare(query)).then_some(Answer { text, number })
}

/// A number rounded to `DIGITS` significant digits without trailing zeros, in scientific notation
/// when it's very large or very small. Whole numbers short of that keep every digit.
fn number(x: f64) -> String {
    let trim = |s: String| if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if x == 0.0 {
        return "0".into();
    }
    if x.abs() >= 1e15 || x.abs() < 1e-6 {
        let s = format!("{:.*e}", (DIGITS - 1) as usize, x);
        let (mantissa, exponent) = s.split_once('e').unwrap_or((&s, "0"));
        return format!("{}e{exponent}", trim(mantissa.to_string()));
    }
    let decimals = (DIGITS - 1 - x.abs().log10().floor() as i32).max(0) as usize;
    trim(format!("{x:.decimals$}"))
}

/// A whole number too big for a float, written out in digits as fend gives it, in scientific
/// notation; None when it isn't one.
fn big_number(digits: &str) -> Option<String> {
    let (sign, digits) = digits.strip_prefix('-').map_or(("", digits), |d| ("-", d));
    if digits.len() <= DIGITS as usize || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // The leading digits as d.dddddd, rounded to `DIGITS` significant ones.
    let lead: f64 = digits[..=DIGITS as usize].parse().ok()?;
    let scale = 10f64.powi(DIGITS - 1);
    let (mut mantissa, mut exponent) = ((lead / 10.0 / scale * scale).round() / scale, digits.len() - 1);
    if mantissa >= 10.0 {
        (mantissa, exponent) = (mantissa / 10.0, exponent + 1);
    }
    let mantissa = format!("{mantissa:.*}", (DIGITS - 1) as usize);
    Some(format!("{sign}{}e{exponent}", mantissa.trim_end_matches('0').trim_end_matches('.')))
}

/// fend's answer tidied: "approx." dropped and the number rounded. Gives the text with its unit
/// and the number alone when there is a unit (a percent counts); None when it doesn't start with
/// a number.
fn format(result: &str) -> Option<(String, Option<String>)> {
    let result = result.trim().trim_start_matches("approx. ");
    let (value, unit) = result.split_once(' ').unwrap_or((result, ""));
    // A percent comes stuck to its number, "100%".
    let (value, percent) = value.strip_suffix('%').map_or((value, false), |v| (v, true));
    let value = match value.parse::<f64>() {
        Ok(x) if x.is_finite() => number(x),
        _ => big_number(value)?,
    };
    Some(match (percent, unit.is_empty()) {
        (true, _) => (format!("{value}%{}", if unit.is_empty() { String::new() } else { format!(" {unit}") }), Some(value)),
        (false, true) => (value, None),
        (false, false) => (format!("{value} {unit}"), Some(value)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maths_needs_a_digit_and_an_operator_or_a_conversion() {
        for q in ["12*7+3", "15% of 240", "2^10", "sqrt(2)", "5 km in mi", "180 lb to kg", "30 c in f", "2 gb in mb", "90 min in h", "3 days in hours", "3-10", "(5)"] {
            assert!(is_maths(q), "{q}");
        }
        for q in ["2026", "kairo this week", "last 7 days", "5 km", "chrome 2", "finals", "", "in to as"] {
            assert!(!is_maths(q), "{q}");
        }
    }

    #[test]
    fn shorthand_is_rewritten_before_asking() {
        assert_eq!(rewrite("30 c in f"), "30 °C in °F");
        assert_eq!(rewrite("300 K to c"), "300 K to °C");
        assert_eq!(rewrite("2 gb in mb"), "2 GB in MB");
        assert_eq!(rewrite("4 GiB to mib"), "4 GiB to MiB");
        // A lone letter outside a temperature conversion is left alone.
        assert_eq!(rewrite("c + 1"), "c + 1");
    }

    #[test]
    fn answers_for_everyday_queries() {
        let text = |q: &str| answer(q).map(|a| a.text);
        assert_eq!(text("12*7+3").as_deref(), Some("87"));
        assert_eq!(text("15% of 240").as_deref(), Some("36"));
        assert_eq!(text("2^10").as_deref(), Some("1024"));
        assert_eq!(text("5 km in mi").as_deref(), Some("3.10686 mi"));
        assert_eq!(text("30 c in f").as_deref(), Some("86 °F"));
        assert_eq!(text("2 gb in mb").as_deref(), Some("2000 MB"));
        assert_eq!(text("90 min in h").as_deref(), Some("1.5 h"));
        assert_eq!(text("3-10").as_deref(), Some("-7"));
        assert_eq!(text("-0.5*3").as_deref(), Some("-1.5"));
        assert_eq!(answer("5 km in mi").unwrap().number.as_deref(), Some("3.10686"));
        assert_eq!(answer("12*7+3").unwrap().number, None);
    }

    #[test]
    fn nothing_for_nonsense_repeats_or_slow_sums() {
        assert!(answer("abc + 1").is_none());
        assert!(answer("(5)").is_none());
        let start = std::time::Instant::now();
        assert!(answer("10^10^10").is_none());
        assert!(start.elapsed() < std::time::Duration::from_millis(500));
    }

    #[test]
    fn sizes_and_speeds_typed_any_way_give_the_right_number() {
        let text = |q: &str| answer(q).map(|a| a.text);
        assert_eq!(text("16gb to mb").as_deref(), Some("16000 MB"));
        assert_eq!(text("1.5gb to mb").as_deref(), Some("1500 MB"));
        assert_eq!(text("500mb in gb").as_deref(), Some("0.5 GB"));
        assert_eq!(text("100 mbps to kbps").as_deref(), Some("100000 kbps"));
        assert_eq!(text("1 gbps in mbps").as_deref(), Some("1000 Mbps"));
        // Capitals the user chose stay: Mb is megabits.
        assert_eq!(text("1 MB in Mb").as_deref(), Some("8 Mb"));
    }

    #[test]
    fn inches_are_a_unit_not_the_conversion() {
        let text = |q: &str| answer(q).map(|a| a.text);
        assert_eq!(text("3 in to cm").as_deref(), Some("7.62 cm"));
        assert_eq!(text("12 in to ft").as_deref(), Some("1 ft"));
        assert!(text("1 m in in").is_some_and(|t| t.starts_with("39.3701")));
    }

    #[test]
    fn dates_ranges_and_numbers_like_phone_numbers_are_not_sums() {
        for q in ["2026-10-30", "2026-1", "2026-10", "30/10/2026", "2026/10/30", "10/30/2026", "30.10.2026", "2024-2025", "555-1234"] {
            assert!(!is_maths(q), "{q}");
        }
        // Plain fractions still are.
        assert!(is_maths("3/4"));
    }

    #[test]
    fn a_comma_decimal_is_never_read_as_thousands() {
        assert!(answer("1,5+1").is_none());
        assert_eq!(answer("1,500+1").map(|a| a.text).as_deref(), Some("1501"));
    }

    #[test]
    fn answers_are_rounded_and_long_ones_go_scientific() {
        assert_eq!(format("approx. 3.1068559612 mi"), Some(("3.10686 mi".into(), Some("3.10686".into()))));
        assert_eq!(format("approx. 0.3333333333"), Some(("0.333333".into(), None)));
        assert_eq!(format("2.5"), Some(("2.5".into(), None)));
        assert_eq!(format("18446744073709551616"), Some(("1.84467e19".into(), None)));
        assert_eq!(format("0.0000000032 s"), Some(("3.2e-9 s".into(), Some("3.2e-9".into()))));
    }

    #[test]
    fn percent_answers_show() {
        let got = |q: &str| answer(q).map(|a| (a.text, a.number));
        assert_eq!(got("50%*2"), Some(("100%".into(), Some("100".into()))));
        assert_eq!(got("15% + 5%"), Some(("20%".into(), Some("20".into()))));
        assert_eq!(format("12.5%"), Some(("12.5%".into(), Some("12.5".into()))));
    }

    #[test]
    fn units_typed_in_capitals_work() {
        assert_eq!(answer("5 KM IN MI").map(|a| a.text).as_deref(), Some("3.10686 mi"));
        assert_eq!(answer("180 LB TO KG").map(|a| a.text).as_deref(), Some("81.6466 kg"));
        // Capitals that mean something stay as typed.
        assert_eq!(answer("1 MB in Mb").map(|a| a.text).as_deref(), Some("8 Mb"));
    }

    #[test]
    fn answers_too_big_for_a_float_still_show() {
        assert_eq!(answer("2^1024").map(|a| a.text).as_deref(), Some("1.79769e308"));
        assert_eq!(format(&format!("-1234567{}", "0".repeat(302))), Some(("-1.23457e308".into(), None)));
    }
}
