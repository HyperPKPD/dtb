//! Dates and clock times in the forms that clinical files come in. All values are read as local clock time with no time zone (a time
//! zone is never guessed) and returned as milliseconds since 1970-01-01 00:00 of that clock.

use super::table::Cell;
use regex::Regex;
use std::sync::OnceLock;

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
pub const DAY_MS: f64 = 86_400_000.0;

pub fn unit_ms(unit: &str) -> Option<f64> {
    match unit {
        "min" => Some(60_000.0),
        "h" => Some(3_600_000.0),
        "d" => Some(DAY_MS),
        "w" => Some(7.0 * DAY_MS),
        _ => None,
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn valid(y: i64, m: i64, d: i64) -> bool {
    if !(1..=12).contains(&m) || d < 1 {
        return false;
    }
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let dim = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][(m - 1) as usize];
    d <= dim
}

fn re(cell: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).unwrap())
}

/// The time of day in "14:30", "14:30:15", "2:30 PM", "1430" … as milliseconds, or None.
pub fn parse_clock(c: &Cell) -> Option<f64> {
    match c {
        Cell::Null => None,
        Cell::Num(s) => {
            let s = *s;
            if (0.0..1.0).contains(&s) {
                return Some(super::super::core::js::round(s * DAY_MS));
            }
            if (0.0..=2359.0).contains(&s) && s.fract() == 0.0 {
                let (h, m) = ((s / 100.0).floor(), s % 100.0);
                return if h < 24.0 && m < 60.0 { Some((h * 60.0 + m) * 60_000.0) } else { None };
            }
            None
        }
        Cell::Text(s) => clock_text(s.trim()),
    }
}

fn clock_text(t: &str) -> Option<f64> {
    static A: OnceLock<Regex> = OnceLock::new();
    static B: OnceLock<Regex> = OnceLock::new();
    let a = re(&A, r"^([0-9]{1,2}):([0-9]{2})(?::([0-9]{2})(?:[.,]([0-9]+))?)?\s*([AaPp][Mm])?$");
    let (h, mi, se, frac, ap): (f64, f64, f64, f64, String);
    if let Some(m) = a.captures(t) {
        h = m[1].parse().ok()?;
        mi = m[2].parse().ok()?;
        se = m.get(3).map_or(0.0, |x| x.as_str().parse().unwrap_or(0.0));
        frac = m.get(4).map_or(0.0, |x| format!("0.{}", x.as_str()).parse().unwrap_or(0.0));
        ap = m.get(5).map_or(String::new(), |x| x.as_str().to_lowercase());
    } else {
        let b = re(&B, r"^([0-9]{2})([0-9]{2})([0-9]{2})?$");
        let m = b.captures(t)?;
        h = m[1].parse().ok()?;
        mi = m[2].parse().ok()?;
        se = m.get(3).map_or(0.0, |x| x.as_str().parse().unwrap_or(0.0));
        frac = 0.0;
        ap = String::new();
    }
    let mut h = h;
    if !ap.is_empty() {
        if !(1.0..=12.0).contains(&h) {
            return None;
        }
        if ap == "pm" && h < 12.0 {
            h += 12.0;
        }
        if ap == "am" && h == 12.0 {
            h = 0.0;
        }
    }
    if h > 23.0 || mi > 59.0 || se > 59.0 {
        return None;
    }
    Some(super::super::core::js::round(((h * 60.0 + mi) * 60.0 + se + frac) * 1000.0))
}

#[derive(Debug, Clone, PartialEq)]
pub enum Dt {
    /// Not a date in any form this reads.
    None,
    /// A date that is ambiguous or impossible, with the reason in plain words.
    Error(String),
    Ok {
        ms: f64,
        has_time: bool,
    },
}

/// Parse a date, or a date with a time. `format` is auto, ymd, dmy, mdy or excel.
pub fn parse_date_time(v: &Cell, format: &str) -> Dt {
    use super::super::core::js;
    match v {
        Cell::Null => return Dt::None,
        Cell::Num(n) => {
            if format == "excel" || (format == "auto" && *n > 20000.0 && *n < 80000.0) {
                return Dt::Ok { ms: js::round((n - 25569.0) * DAY_MS), has_time: n % 1.0 != 0.0 };
            }
            return Dt::None;
        }
        Cell::Text(s) if s.is_empty() => return Dt::None,
        Cell::Text(_) => {}
    }
    let s = v.text();
    let s = s.trim();
    if format == "excel" {
        let n = js::number(s);
        return if n.is_finite() { Dt::Ok { ms: js::round((n - 25569.0) * DAY_MS), has_time: n % 1.0 != 0.0 } } else { Dt::None };
    }
    static ISO: OnceLock<Regex> = OnceLock::new();
    static NAMED: OnceLock<Regex> = OnceLock::new();
    static NUMERIC: OnceLock<Regex> = OnceLock::new();
    let iso = re(&ISO, r"(?i)^([0-9]{4})[-/.]([0-9]{1,2})[-/.]([0-9]{1,2})(?:[T\s]+(.+?))?(?:Z|[+-][0-9]{2}:?[0-9]{2})?$");
    let named = re(&NAMED, r"^([0-9]{1,2})[-\s/.]?([A-Za-z]{3})[A-Za-z]*[-\s/.,]*([0-9]{2,4})(?:[T\s,]+(.+))?$");
    let numeric = re(&NUMERIC, r"^([0-9]{1,2})[-/.]([0-9]{1,2})[-/.]([0-9]{4}|[0-9]{2})(?:[T\s]+(.+))?$");
    let (y, mo, d, rest): (i64, i64, i64, Option<String>);
    if let (Some(m), true) = (iso.captures(s), format == "auto" || format == "ymd") {
        y = m[1].parse().unwrap();
        mo = m[2].parse().unwrap();
        d = m[3].parse().unwrap();
        rest = m.get(4).map(|x| x.as_str().to_string());
    } else if let Some(m) = named.captures(s) {
        let Some(mi) = MONTHS.iter().position(|x| *x == m[2].to_lowercase()) else { return Dt::None };
        d = m[1].parse().unwrap();
        mo = mi as i64 + 1;
        let mut yy: i64 = m[3].parse().unwrap();
        if yy < 100 {
            yy += if yy < 50 { 2000 } else { 1900 };
        }
        y = yy;
        rest = m.get(4).map(|x| x.as_str().to_string());
    } else if let Some(m) = numeric.captures(s) {
        let (a, b): (i64, i64) = (m[1].parse().unwrap(), m[2].parse().unwrap());
        if format == "dmy" {
            d = a;
            mo = b;
        } else if format == "mdy" {
            mo = a;
            d = b;
        } else {
            return Dt::Error(format!("\"{s}\" could be day-month or month-day: choose the date order."));
        }
        let mut yy: i64 = m[3].parse().unwrap();
        if yy < 100 {
            yy += if yy < 50 { 2000 } else { 1900 };
        }
        y = yy;
        rest = m.get(4).map(|x| x.as_str().to_string());
    } else {
        return Dt::None;
    }
    if !valid(y, mo, d) {
        return Dt::Error(format!("\"{s}\" is not a real date."));
    }
    let mut ms = days_from_civil(y, mo, d) as f64 * DAY_MS;
    let mut has_time = false;
    if let Some(r) = rest {
        static TZ: OnceLock<Regex> = OnceLock::new();
        let tz = re(&TZ, r"(?i)\s*(Z|[+-][0-9]{2}:?[0-9]{2})$");
        let cleaned = tz.replace(&r, "").to_string();
        match clock_text(cleaned.trim()) {
            Some(c) => {
                ms += c;
                has_time = true;
            }
            None => return Dt::Error(format!("The time in \"{s}\" is not understood.")),
        }
    }
    Dt::Ok { ms, has_time }
}
