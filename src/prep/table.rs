//! The typed table of Prep Studio: columns by name, rows of cells. A cell is a number, a text or missing. Numbers are only made from
//! text when every value of the column is a plain number, so identifiers such as "007" stay text.

use crate::core::js;
use crate::core::Table;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Null,
    Num(f64),
    Text(String),
}

impl Cell {
    pub fn is_null(&self) -> bool {
        matches!(self, Cell::Null)
    }
    pub fn num(&self) -> Option<f64> {
        if let Cell::Num(v) = self {
            Some(*v)
        } else {
            None
        }
    }
    pub fn is_text(&self) -> bool {
        matches!(self, Cell::Text(_))
    }
    /// `String(v)` for a non-missing cell.
    pub fn text(&self) -> String {
        match self {
            Cell::Null => String::new(),
            Cell::Num(v) => js::num_str(*v),
            Cell::Text(s) => s.clone(),
        }
    }
    pub fn to_json(&self) -> Value {
        match self {
            Cell::Null => Value::Null,
            Cell::Num(v) => json!(*v),
            Cell::Text(s) => Value::String(s.clone()),
        }
    }
    pub fn from_json(v: &Value) -> Cell {
        match v {
            Value::Null => Cell::Null,
            Value::Number(n) => Cell::Num(n.as_f64().unwrap_or(f64::NAN)),
            Value::String(s) => Cell::Text(s.clone()),
            Value::Bool(b) => Cell::Num(if *b { 1.0 } else { 0.0 }),
            other => Cell::Text(other.to_string()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

fn leading_zero_dot() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^[+-]?0\.").unwrap())
}

/// `^[+-]?([0-9]+\.?[0-9]*|\.[0-9]+)([eE][+-]?[0-9]+)?$` without a regular expression (this runs on every cell of a large table).
pub fn is_plain_number(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - int_start;
    let mut frac_digits = 0;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let f = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        frac_digits = i - f;
    }
    if int_digits == 0 && frac_digits == 0 {
        return false;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let e = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == e {
            return false;
        }
    }
    i == b.len()
}

/// `^[+-]?0[0-9]`: a number written with a leading zero (an identifier such as 007, not a number).
fn leading_zero_digit(t: &str) -> bool {
    let b = t.as_bytes();
    let i = usize::from(!b.is_empty() && (b[0] == b'+' || b[0] == b'-'));
    b.len() > i + 1 && b[i] == b'0' && b[i + 1].is_ascii_digit()
}

pub fn is_missing_text(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t == "." || ["na", "nan", "n/a", "null"].iter().any(|m| t.eq_ignore_ascii_case(m))
}

impl Frame {
    pub fn col(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }

    /// Make a typed table from parsed text.
    pub fn from_text(p: &Table) -> Frame {
        let nc = p.columns.len();
        // a column is numeric when every non-missing cell is a plain number without a leading zero (found in one pass over the rows)
        let mut numeric = vec![true; nc];
        let mut any = vec![false; nc];
        for r in &p.rows {
            for j in 0..nc {
                if !numeric[j] {
                    continue;
                }
                let Some(s) = r.get(j) else { continue };
                if is_missing_text(s) {
                    continue;
                }
                any[j] = true;
                let t = s.trim();
                if !is_plain_number(t) || (leading_zero_digit(t) && !leading_zero_dot().is_match(t) && !t.contains(['e', 'E'])) {
                    numeric[j] = false;
                }
            }
        }
        let rows: Vec<Vec<Cell>> = p
            .rows
            .iter()
            .map(|r| {
                (0..nc)
                    .map(|j| match r.get(j) {
                        None => Cell::Null,
                        Some(s) if is_missing_text(s) => Cell::Null,
                        Some(s) if numeric[j] && any[j] => {
                            let t = s.trim();
                            // a plain number: the standard parser gives JavaScript's value
                            Cell::Num(t.parse::<f64>().unwrap_or_else(|_| js::number(t)))
                        }
                        Some(s) => Cell::Text(s.trim().to_string()),
                    })
                    .collect()
            })
            .collect();
        Frame { columns: p.columns.clone(), rows }
    }

    /// The rows as text for export: missing becomes `missing`, numbers keep their shortest exact form.
    pub fn text_rows(&self, missing: &str) -> Vec<Vec<String>> {
        self.rows
            .iter()
            .map(|r| {
                r.iter()
                    .map(|v| match v {
                        Cell::Null => missing.to_string(),
                        Cell::Num(x) => num_text(*x),
                        Cell::Text(s) => s.clone(),
                    })
                    .collect()
            })
            .collect()
    }

    pub fn to_json(&self) -> Value {
        json!({
            "columns": self.columns,
            "rows": self.rows.iter().map(|r| r.iter().map(Cell::to_json).collect::<Vec<_>>()).collect::<Vec<_>>(),
        })
    }

    pub fn from_json(v: &Value) -> Result<Frame, String> {
        let cols = v.get("columns").and_then(Value::as_array).ok_or("A table needs a list of columns.")?;
        let columns: Vec<String> = cols.iter().map(|c| c.as_str().map(String::from).unwrap_or_else(|| c.to_string())).collect();
        let rows = v.get("rows").and_then(Value::as_array).ok_or("A table needs a list of rows.")?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let a = r.as_array().ok_or("Every row of a table must be a list of values.")?;
            let mut row: Vec<Cell> = a.iter().map(Cell::from_json).collect();
            row.resize(columns.len(), Cell::Null);
            out.push(row);
        }
        Ok(Frame { columns, rows: out })
    }
}

/// The shortest exact text of a number (twelve significant digits at most for fractions).
pub fn num_text(v: f64) -> String {
    if v.fract() == 0.0 && v.is_finite() {
        return js::num_str(v);
    }
    let s = js::num_str(v);
    if s.contains(['e', 'E']) {
        return s;
    }
    let p: f64 = format!("{:.11e}", v).parse().unwrap_or(v);
    js::num_str(p)
}

/// The key that matches rows in joins: numbers and numeric text compare equal ("1" and 1), text is trimmed.
pub fn key_of(c: &Cell) -> String {
    match c {
        Cell::Null => "\u{0}".into(),
        Cell::Num(v) => js::num_str(*v),
        Cell::Text(s) => {
            let t = s.trim();
            if is_plain_number(t) && !leading_zero_digit(t) {
                js::num_str(js::number(s))
            } else {
                t.to_string()
            }
        }
    }
}

pub fn require_cols(t: &Frame, names: &[String], what: &str) -> Result<(), String> {
    let miss: Vec<&String> = names.iter().filter(|n| !n.is_empty() && !t.columns.contains(n)).collect();
    if miss.is_empty() {
        return Ok(());
    }
    let list = miss.iter().map(|m| format!("\"{m}\"")).collect::<Vec<_>>().join(", ");
    Err(format!(
        "The {what}{} {list} {} not in the table (columns now: {}).",
        if miss.len() > 1 { "s" } else { "" },
        if miss.len() > 1 { "are" } else { "is" },
        t.columns.join(", ")
    ))
}

fn cell_key(c: &Cell) -> String {
    match c {
        Cell::Null => "z".into(),
        Cell::Num(v) => format!("n{}", v.to_bits()),
        Cell::Text(s) => format!("t{s}"),
    }
}

/// Per-column facts for the profile view.
pub fn profile(t: &Frame) -> Vec<Value> {
    (0..t.columns.len())
        .map(|j| {
            let (mut n_num, mut n_text, mut n_miss) = (0usize, 0usize, 0usize);
            let (mut min, mut max, mut sum) = (f64::INFINITY, f64::NEG_INFINITY, 0.0);
            let mut uniq: HashSet<String> = HashSet::new();
            let mut examples: Vec<Cell> = Vec::new();
            for r in &t.rows {
                let v = &r[j];
                match v {
                    Cell::Null => {
                        n_miss += 1;
                        continue;
                    }
                    Cell::Num(x) => {
                        n_num += 1;
                        sum += x;
                        if *x < min {
                            min = *x;
                        }
                        if *x > max {
                            max = *x;
                        }
                    }
                    Cell::Text(_) => n_text += 1,
                }
                if uniq.len() < 2000 {
                    uniq.insert(cell_key(v));
                }
                if examples.len() < 3 && !examples.contains(v) {
                    examples.push(v.clone());
                }
            }
            let ty = if n_num + n_text == 0 {
                "empty"
            } else if n_text == 0 {
                "number"
            } else if n_num == 0 {
                "text"
            } else {
                "mixed"
            };
            json!({
                "name": t.columns[j], "type": ty, "n": t.rows.len(), "missing": n_miss,
                "unique": if uniq.len() >= 2000 { json!("2000+") } else { json!(uniq.len()) },
                "min": if n_num > 0 { json!(min) } else { Value::Null },
                "max": if n_num > 0 { json!(max) } else { Value::Null },
                "mean": if n_num > 0 { json!(sum / n_num as f64) } else { Value::Null },
                "examples": examples.iter().map(Cell::to_json).collect::<Vec<_>>(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_number_scanners_agree_with_their_regular_expressions() {
        let plain = Regex::new(r"^[+-]?([0-9]+\.?[0-9]*|\.[0-9]+)([eE][+-]?[0-9]+)?$").unwrap();
        let lead = Regex::new(r"^[+-]?0[0-9]").unwrap();
        for t in [
            "",
            "0",
            "1",
            "-1",
            "+1",
            "1.",
            ".5",
            "-.5",
            "1.5",
            "1e5",
            "1E-5",
            "1.e5",
            ".5e+3",
            "e5",
            "1e",
            "1e+",
            "+",
            "-",
            ".",
            "..",
            "1.2.3",
            "1,5",
            "0x10",
            " 1",
            "1 ",
            "007",
            "-007",
            "0.5",
            "00.5",
            "0",
            "00",
            "+0",
            "1e5e5",
            "NaN",
            "Infinity",
            "１２",
            "12345678901234567890",
            "-0",
            "0e0",
        ] {
            assert_eq!(is_plain_number(t), plain.is_match(t), "plain {t:?}");
            assert_eq!(leading_zero_digit(t), lead.is_match(t), "leading zero {t:?}");
        }
    }

    #[test]
    fn missing_markers() {
        for t in ["", " ", ".", "NA", "na", "NaN", "n/a", "N/A", "null", "NULL", " null "] {
            assert!(is_missing_text(t), "{t:?}");
        }
        for t in ["0", "-", "a", "nan1", "none"] {
            assert!(!is_missing_text(t), "{t:?}");
        }
    }
}
