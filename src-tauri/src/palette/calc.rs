//! The palette's calculator: whether a query is maths or a unit conversion, and its answer,
//! worked out offline by fend. No drawing here, so all of it can be tested.

use std::time::{Duration, Instant};

/// How long one calculation may take, so an odd input can't stall the palette.
const LIMIT: Duration = Duration::from_millis(100);
/// Significant digits an answer is rounded to.
const DIGITS: i32 = 6;
const OPERATORS: [char; 10] = ['+', '-', '*', '/', '×', '÷', '^', '%', '(', ')'];
const CONVERSIONS: [&str; 3] = ["in", "to", "as"];
/// Data sizes as fend wants them; written in lower case, fend reads `mb` as millibits.
const DATA_SIZES: [&str; 8] = ["KB", "MB", "GB", "TB", "KiB", "MiB", "GiB", "TiB"];

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
    let at = words.iter().position(|w| CONVERSIONS.contains(&w.to_lowercase().as_str()))?;
    (starts_with_number && at > 0 && has_letters(&words[..at]) && has_letters(&words[at + 1..])).then_some(at)
}

/// Whether the query is maths or a conversion: a digit, and an operator or a conversion.
pub fn is_maths(query: &str) -> bool {
    let words: Vec<&str> = query.split_whitespace().collect();
    query.chars().any(|c| c.is_ascii_digit()) && (query.contains(OPERATORS) || conversion_at(&words).is_some())
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

/// The query with everyday shorthand written the way fend reads it: data sizes in capitals, and
/// the lone letters of a temperature conversion as degrees ("30 c in f" → "30 °C in °F"; fend
/// alone reads `c` as the speed of light).
pub fn rewrite(query: &str) -> String {
    let mut words: Vec<String> = query.split(' ').map(str::to_string).collect();
    for word in &mut words {
        if let Some(size) = DATA_SIZES.iter().find(|s| s.eq_ignore_ascii_case(word)) {
            *word = size.to_string();
        }
    }
    let solid: Vec<&str> = query.split_whitespace().collect();
    // Only "<number> <temperature> in <temperature>", so a lone letter elsewhere stays as typed.
    if let (4, Some(2)) = (solid.len(), conversion_at(&solid)) {
        if let (Some(from), Some(to)) = (temperature(solid[1]), temperature(solid[3])) {
            return format!("{} {from} {} {to}", solid[0], solid[2]);
        }
    }
    words.join(" ")
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
    let mut context = fend_core::Context::new();
    let result = fend_core::evaluate_with_interrupt(&rewrite(query.trim()), &mut context, &Deadline(Instant::now() + LIMIT)).ok()?;
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

/// fend's answer tidied: "approx." dropped and the number rounded. Gives the text with its unit
/// and the number alone when there is a unit; None when it doesn't start with a number.
fn format(result: &str) -> Option<(String, Option<String>)> {
    let result = result.trim().trim_start_matches("approx. ");
    let (value, unit) = result.split_once(' ').unwrap_or((result, ""));
    let value = number(value.parse::<f64>().ok().filter(|x| x.is_finite())?);
    Some(if unit.is_empty() { (value, None) } else { (format!("{value} {unit}"), Some(value)) })
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
    fn answers_are_rounded_and_long_ones_go_scientific() {
        assert_eq!(format("approx. 3.1068559612 mi"), Some(("3.10686 mi".into(), Some("3.10686".into()))));
        assert_eq!(format("approx. 0.3333333333"), Some(("0.333333".into(), None)));
        assert_eq!(format("2.5"), Some(("2.5".into(), None)));
        assert_eq!(format("18446744073709551616"), Some(("1.84467e19".into(), None)));
        assert_eq!(format("0.0000000032 s"), Some(("3.2e-9 s".into(), Some("3.2e-9".into()))));
    }
}
