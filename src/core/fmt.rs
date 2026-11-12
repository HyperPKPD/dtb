//! Number formatting that matches JavaScript's `Number.prototype.toFixed` (exact ties round away from zero), so that the text the
//! engines write into results is the same as in the apps.

/// `v.toFixed(d)` of JavaScript.
pub fn fixed(v: f64, d: usize) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    let neg = v < 0.0;
    let s = format!("{:.*}", 45, v.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let mut digits: Vec<u8> = int.bytes().chain(frac.bytes().take(d)).map(|b| b - b'0').collect();
    let next = frac.as_bytes().get(d).map_or(0, |b| b - b'0');
    if next >= 5 {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, 1);
                break;
            }
            i -= 1;
            if digits[i] == 9 {
                digits[i] = 0;
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    let int_len = digits.len() - d;
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    for (i, dg) in digits.iter().enumerate() {
        if i == int_len {
            out.push('.');
        }
        out.push((b'0' + dg) as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::fixed;

    #[test]
    fn ties_round_away_from_zero_like_javascript() {
        assert_eq!(fixed(-0.0625, 3), "-0.063");
        assert_eq!(fixed(0.0625, 3), "0.063");
        assert_eq!(fixed(2.5, 0), "3");
        assert_eq!(fixed(1.005, 2), "1.00"); // 1.005 is below the tie in binary
        assert_eq!(fixed(9.9996, 3), "10.000");
        assert_eq!(fixed(0.0, 2), "0.00");
        assert_eq!(fixed(123.456, 0), "123");
        assert_eq!(fixed(-0.0001, 2), "-0.00");
    }
}
