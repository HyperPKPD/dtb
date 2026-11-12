//! JavaScript-compatible number and text helpers. The data engine of Prep Studio is written in JavaScript; to give the same
//! answers (and the same words in its notes) dtb formats numbers, rounds and compares text the way JavaScript does.

use std::cmp::Ordering;

/// `String(x)` of a JavaScript number: shortest round-trip digits; exponent form below 1e-6 and from 1e21.
pub fn num_str(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if x == 0.0 {
        return "0".into();
    }
    // the common cases: Rust prints the shortest round-trip digits in positional form, as JavaScript does between 1e-6 and 1e21
    let a = x.abs();
    if (1e-6..1e21).contains(&a) {
        return if a < 1e15 && x.fract() == 0.0 { (x as i64).to_string() } else { format!("{x}") };
    }
    num_str_general(x)
}

/// The general form of `num_str`, built from the digits of the shortest exponent form (also the reference for the fast path).
fn num_str_general(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    let neg = x < 0.0;
    let e = format!("{:e}", x.abs()); // shortest round-trip: d.ddde[-]x
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exp + 1; // position of the decimal point relative to the digits
    let mut s = String::new();
    if neg {
        s.push('-');
    }
    if k <= n && n <= 21 {
        s.push_str(&digits);
        s.push_str(&"0".repeat((n - k) as usize));
    } else if 0 < n && n <= 21 {
        s.push_str(&digits[..n as usize]);
        s.push('.');
        s.push_str(&digits[n as usize..]);
    } else if -6 < n && n <= 0 {
        s.push_str("0.");
        s.push_str(&"0".repeat((-n) as usize));
        s.push_str(&digits);
    } else {
        let e1 = n - 1;
        s.push_str(&digits[..1]);
        if k > 1 {
            s.push('.');
            s.push_str(&digits[1..]);
        }
        s.push('e');
        s.push(if e1 < 0 { '-' } else { '+' });
        s.push_str(&e1.abs().to_string());
    }
    s
}

/// `Number(s)` of a JavaScript string (trimmed; empty is 0; NaN when it is not a number).
pub fn number(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return 0.0;
    }
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    let (sign, body) = match t.as_bytes()[0] {
        b'-' => (-1.0, &t[1..]),
        b'+' => (1.0, &t[1..]),
        _ => (1.0, t),
    };
    let lower = body.to_ascii_lowercase();
    for (p, radix) in [("0x", 16u32), ("0o", 8), ("0b", 2)] {
        if let Some(h) = lower.strip_prefix(p) {
            if t.len() == body.len() && !h.is_empty() {
                return u64::from_str_radix(h, radix).map(|v| v as f64).unwrap_or(f64::NAN);
            }
            return f64::NAN;
        }
    }
    if body.is_empty() || !body.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-')) {
        return f64::NAN;
    }
    match body.parse::<f64>() {
        Ok(v) => sign * v,
        Err(_) => f64::NAN,
    }
}

/// `Math.round`: halves go towards +infinity.
pub fn round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let f = x.floor();
    if x - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}

/// `Math.sign`.
pub fn sign(x: f64) -> f64 {
    if x.is_nan() || x == 0.0 {
        x
    } else if x > 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// `n.toLocaleString('en-US')` for whole numbers (thousands separators).
pub fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A close match to `a.localeCompare(b, undefined, { numeric: true })` for the identifiers, names and codes found in datasets:
/// digit runs compare as numbers, letters compare without regard to case first (lower case before upper case on a tie).
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let ca: Vec<char> = a.chars().collect();
    let cb: Vec<char> = b.chars().collect();
    let (mut i, mut j) = (0, 0);
    let mut tie = Ordering::Equal;
    while i < ca.len() && j < cb.len() {
        if ca[i].is_ascii_digit() && cb[j].is_ascii_digit() {
            let si = i;
            while i < ca.len() && ca[i].is_ascii_digit() {
                i += 1;
            }
            let sj = j;
            while j < cb.len() && cb[j].is_ascii_digit() {
                j += 1;
            }
            let na: String = ca[si..i].iter().collect::<String>().trim_start_matches('0').to_string();
            let nb: String = cb[sj..j].iter().collect::<String>().trim_start_matches('0').to_string();
            let o = na.len().cmp(&nb.len()).then_with(|| na.cmp(&nb));
            if o != Ordering::Equal {
                return o;
            }
            continue;
        }
        let (x, y) = (ca[i], cb[j]);
        let (lx, ly) = (x.to_lowercase().next().unwrap_or(x), y.to_lowercase().next().unwrap_or(y));
        let o = class(lx).cmp(&class(ly)).then_with(|| lx.cmp(&ly));
        if o != Ordering::Equal {
            return o;
        }
        if tie == Ordering::Equal && x != y {
            tie = if x.is_lowercase() { Ordering::Less } else { Ordering::Greater };
        }
        i += 1;
        j += 1;
    }
    (ca.len() - i).cmp(&(cb.len() - j)).then(tie)
}

/// Collation classes: punctuation and symbols, then digits, then letters.
fn class(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if c.is_alphanumeric() {
        if c.is_numeric() {
            2
        } else {
            3
        }
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn positional_fast_path_equals_the_general_form() {
        for x in [
            1.0,
            -1.0,
            0.5,
            0.1,
            123456789.123,
            1e-6,
            1.5e-6,
            9.999999e-7,
            1e20,
            123456789012345680000.0,
            1e21,
            0.000123,
            100.0,
            4.35,
            1e15,
            999999999999999.9,
            -2.5e-5,
            5e-324,
            1.7976931348623157e308,
        ] {
            assert_eq!(num_str(x), num_str_general(x), "{x}");
            assert_eq!(num_str(-x), num_str_general(-x), "-{x}");
        }
    }

    #[test]
    fn formats_like_javascript() {
        assert_eq!(num_str(1.0), "1");
        assert_eq!(num_str(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(num_str(1e21), "1e+21");
        assert_eq!(num_str(1.5e-7), "1.5e-7");
        assert_eq!(num_str(0.000001), "0.000001");
        assert_eq!(num_str(123456789012.0), "123456789012");
        assert_eq!(num_str(-2.5), "-2.5");
    }
    #[test]
    fn number_parsing() {
        assert_eq!(number(" 12 "), 12.0);
        assert_eq!(number(""), 0.0);
        assert!(number("abc").is_nan());
        assert!(number("inf").is_nan());
        assert_eq!(number("0x10"), 16.0);
        assert_eq!(number("1e3"), 1000.0);
    }
    #[test]
    fn rounds_halves_up() {
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(round(0.49999999999999994), 0.0);
    }
    #[test]
    fn natural_order() {
        assert_eq!(natural_cmp("S2", "S10"), Ordering::Less);
        assert_eq!(natural_cmp("a", "B"), Ordering::Less);
        assert_eq!(natural_cmp("a", "A"), Ordering::Less);
        assert_eq!(thousands(1234567), "1,234,567");
    }
}
