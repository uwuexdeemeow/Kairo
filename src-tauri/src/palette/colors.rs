//! Colours in the palette: a colour typed as `#ff8800`, `#f80`, `rgb(255, 136, 0)` or
//! `hsl(32, 100%, 50%)` is shown as hex, RGB and HSL, each a row that copies it; `pick color`
//! samples one from anywhere on the screen (`platform::colorpick`). No drawing here, so all of it
//! can be tested.

/// A colour, as red, green and blue from 0 to 255.
pub type Rgb = [u8; 3];

/// The colour a query names, if it names one.
pub fn parse(query: &str) -> Option<Rgb> {
    let q = query.trim().to_lowercase();
    if let Some(hex) = q.strip_prefix('#') {
        return from_hex(hex);
    }
    let numbers = |rest: &str| -> Option<Vec<f64>> {
        let inner = rest.trim().trim_start_matches('(').trim_end_matches(')');
        let parts: Vec<f64> = inner.split([',', ' ', '/']).filter(|p| !p.is_empty()).map(|p| p.trim_end_matches('%').parse::<f64>()).collect::<Result<_, _>>().ok()?;
        (parts.len() == 3 || parts.len() == 4).then_some(parts)
    };
    if let Some(rest) = q.strip_prefix("rgba").or_else(|| q.strip_prefix("rgb")) {
        let n = numbers(rest)?;
        let channel = |v: f64| (0.0..=255.0).contains(&v).then_some(v.round() as u8);
        return Some([channel(n[0])?, channel(n[1])?, channel(n[2])?]);
    }
    if let Some(rest) = q.strip_prefix("hsla").or_else(|| q.strip_prefix("hsl")) {
        let n = numbers(rest)?;
        if !(0.0..=100.0).contains(&n[1]) || !(0.0..=100.0).contains(&n[2]) {
            return None;
        }
        return Some(from_hsl(n[0].rem_euclid(360.0), n[1] / 100.0, n[2] / 100.0));
    }
    None
}

fn from_hex(hex: &str) -> Option<Rgb> {
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let digit = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok();
    let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    match hex.len() {
        // #f80 and #f80c (with alpha, which is left out).
        3 | 4 => Some([digit(0)? * 17, digit(1)? * 17, digit(2)? * 17]),
        6 | 8 => Some([pair(0)?, pair(2)?, pair(4)?]),
        _ => None,
    }
}

fn from_hsl(h: f64, s: f64, l: f64) -> Rgb {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match h as u32 / 60 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let byte = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [byte(r), byte(g), byte(b)]
}

/// Hue in degrees and saturation and lightness in percent, rounded as CSS writes them.
fn to_hsl([r, g, b]: Rgb) -> (u32, u32, u32) {
    let (r, g, b) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0, 0, (l * 100.0).round() as u32);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    ((h.round() as u32) % 360, (s * 100.0).round() as u32, (l * 100.0).round() as u32)
}

pub fn hex([r, g, b]: Rgb) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

pub fn rgb([r, g, b]: Rgb) -> String {
    format!("rgb({r}, {g}, {b})")
}

pub fn hsl(c: Rgb) -> String {
    let (h, s, l) = to_hsl(c);
    format!("hsl({h}, {s}%, {l}%)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_read_as_css_writes_them() {
        assert_eq!(parse("#ff8800"), Some([255, 136, 0]));
        assert_eq!(parse("#F80"), Some([255, 136, 0]));
        assert_eq!(parse("#ff8800cc"), Some([255, 136, 0]));
        assert_eq!(parse("rgb(255, 136, 0)"), Some([255, 136, 0]));
        assert_eq!(parse("rgb 255 136 0"), Some([255, 136, 0]));
        assert_eq!(parse("rgba(255, 136, 0, 0.5)"), Some([255, 136, 0]));
        assert_eq!(parse("hsl(32, 100%, 50%)"), Some([255, 136, 0]));
        assert_eq!(parse("hsl(0, 0%, 100%)"), Some([255, 255, 255]));
        for not in ["#ggg", "#12345", "rgb(300, 0, 0)", "hsl(10, 120%, 50%)", "ff8800", "#", "rgb()"] {
            assert_eq!(parse(not), None, "{not}");
        }
    }

    #[test]
    fn a_colour_is_written_three_ways() {
        let c = [255, 136, 0];
        assert_eq!((hex(c), rgb(c), hsl(c)), ("#FF8800".into(), "rgb(255, 136, 0)".into(), "hsl(32, 100%, 50%)".into()));
        assert_eq!(hsl([128, 128, 128]), "hsl(0, 0%, 50%)");
        assert_eq!(hsl([0, 0, 255]), "hsl(240, 100%, 50%)");
        // There and back.
        for c in [[18, 52, 86], [200, 10, 150], [0, 0, 0], [255, 255, 255]] {
            assert_eq!(parse(&hex(c)), Some(c));
            assert_eq!(parse(&rgb(c)), Some(c));
        }
    }

}
