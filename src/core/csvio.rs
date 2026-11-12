//! Delimited text: reading and writing CSV / TSV / semicolon / pipe / space files, with the same rules as the apps.

use serde::{Deserialize, Serialize};

/// Candidate delimiters, in order of preference when the counts tie.
pub const DELIMITERS: [char; 5] = [',', '\t', ';', '|', ' '];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn col(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }
}

#[derive(Clone, Debug)]
pub struct Parsed {
    pub table: Table,
    pub delimiter: char,
    pub warnings: Vec<String>,
}

fn split_line(line: &str, d: char) -> usize {
    let mut count = 1;
    let mut cur_empty = true;
    let mut q = false;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if q {
            if c == '"' {
                if chars.get(i + 1) == Some(&'"') {
                    cur_empty = false;
                    i += 1;
                } else {
                    q = false;
                }
            } else {
                cur_empty = false;
            }
        } else if c == '"' && cur_empty {
            q = true;
        } else if c == d {
            count += 1;
            cur_empty = true;
        } else {
            cur_empty = false;
        }
        i += 1;
    }
    count
}

/// Guess the delimiter from the first lines: the one that splits every sampled line into the same number (> 1) of fields.
pub fn detect_delimiter(text: &str) -> char {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let lines: Vec<&str> = text.split(['\n', '\r']).filter(|l| !l.trim().is_empty()).take(20).collect();
    if lines.is_empty() {
        return ',';
    }
    let mut best = ',';
    let mut best_score = -1.0f64;
    for d in DELIMITERS {
        let counts: Vec<usize> = lines.iter().map(|l| split_line(l, d)).collect();
        let first = counts[0];
        if first < 2 {
            continue;
        }
        let consistent = counts.iter().filter(|&&c| c == first).count() as f64 / counts.len() as f64;
        let score = consistent * 1000.0 + first as f64;
        if score > best_score {
            best_score = score;
            best = d;
        }
    }
    best
}

/// Parse delimited text with a header row. Quotes, doubled quotes, CR / LF / CRLF, a byte-order mark and blank trailing lines are
/// handled. Short rows are padded with empty strings; long rows are kept whole and reported in the warnings.
#[allow(unused_assignments)]
pub fn parse_delimited(text: &str, delimiter: Option<char>) -> Parsed {
    let src = text.strip_prefix('\u{feff}').unwrap_or(text);
    let d = delimiter.unwrap_or_else(|| detect_delimiter(src));
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    // the text is scanned as bytes: every delimiter is ASCII, so a field is always a whole number of UTF-8 characters
    let bytes = src.as_bytes();
    let db = if d.is_ascii() { d as u8 } else { 0 };
    let mut cur: Vec<u8> = Vec::new();
    let mut q = false;
    let mut any = false;
    let mut i = 0;
    let field = |cur: &mut Vec<u8>| -> String {
        let s = String::from_utf8_lossy(cur).into_owned();
        cur.clear();
        s
    };
    macro_rules! push_field {
        () => {{
            row.push(field(&mut cur));
        }};
    }
    macro_rules! push_row {
        () => {{
            push_field!();
            if !(row.len() == 1 && row[0].is_empty() && !any) {
                let cap = row.len();
                rows.push(std::mem::replace(&mut row, Vec::with_capacity(cap)));
            } else {
                row.clear();
            }
            any = false;
        }};
    }
    while i < bytes.len() {
        let c = bytes[i];
        if q {
            if c == b'"' {
                if bytes.get(i + 1) == Some(&b'"') {
                    cur.push(b'"');
                    i += 1;
                } else {
                    q = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == b'"' && cur.is_empty() {
            q = true;
            any = true;
        } else if c == db {
            push_field!();
            any = true;
        } else if c == b'\r' {
            if bytes.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            push_row!();
        } else if c == b'\n' {
            push_row!();
        } else {
            cur.push(c);
            any = true;
        }
        i += 1;
    }
    if !cur.is_empty() || !row.is_empty() || any {
        push_row!();
    }
    while rows.last().is_some_and(|r| r.iter().all(|x| x.is_empty())) {
        rows.pop();
    }
    let mut warnings = Vec::new();
    if rows.is_empty() {
        return Parsed { table: Table::default(), delimiter: d, warnings: vec!["The file is empty.".into()] };
    }
    let mut columns: Vec<String> =
        rows[0].iter().enumerate().map(|(i, c)| if c.trim().is_empty() { format!("V{}", i + 1) } else { c.trim().to_string() }).collect();
    let mut used = std::collections::HashSet::new();
    for i in 0..columns.len() {
        let n = columns[i].clone();
        if used.contains(&n) {
            let mut k = 2;
            while used.contains(&format!("{n}_{k}")) {
                k += 1;
            }
            warnings.push(format!("Duplicate column name \"{n}\" renamed to \"{n}_{k}\"."));
            columns[i] = format!("{n}_{k}");
        }
        used.insert(columns[i].clone());
    }
    let mut body: Vec<Vec<String>> = rows.into_iter().skip(1).collect();
    let mut long = 0;
    for r in &mut body {
        while r.len() < columns.len() {
            r.push(String::new());
        }
        if r.len() > columns.len() {
            long += 1;
        }
    }
    if long > 0 {
        warnings.push(format!("{long} row(s) have more fields than the header."));
    }
    Parsed { table: Table { columns, rows: body }, delimiter: d, warnings }
}

/// Write a table as delimited text.
pub fn to_delimited(columns: &[String], rows: &[Vec<String>], delimiter: char, missing: &str) -> String {
    let q = |s: &str| -> String {
        if s.is_empty() {
            return missing.to_string();
        }
        let needs = s.contains(['"', '\r', '\n'])
            || s.contains(delimiter)
            || (delimiter != ' ' && (s.starts_with(char::is_whitespace) || s.ends_with(char::is_whitespace)));
        if needs {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    };
    let mut out = String::new();
    out.push_str(&columns.iter().map(|c| q(c)).collect::<Vec<_>>().join(&delimiter.to_string()));
    out.push('\n');
    for r in rows {
        out.push_str(&r.iter().map(|c| q(c)).collect::<Vec<_>>().join(&delimiter.to_string()));
        out.push('\n');
    }
    out
}

/// A number from text, or NaN. Empty, ".", NA, NaN, N/A and null are missing; "-99" is a number.
pub fn num(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() || t == "." {
        return f64::NAN;
    }
    let l = t.to_ascii_lowercase();
    if matches!(l.as_str(), "na" | "nan" | "n/a" | "null") {
        return f64::NAN;
    }
    // JavaScript's Number() accepts decimal, exponent, hex (0x), Infinity; keep to the decimal forms that data files hold
    match t.parse::<f64>() {
        Ok(v) if v.is_finite() && !l.contains("inf") && !l.contains("nan") => v,
        _ => f64::NAN,
    }
}
