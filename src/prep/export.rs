//! What leaves Prep Studio: the dataset text, the `$DATA`/`$INPUT` lines and the data dictionary.

use super::table::{profile, Cell, Frame};
use crate::core::to_delimited;
use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

const RESERVED: &[&str] = &["DATE", "DAT1", "DAT2", "DAT3", "DROP", "SKIP", "DEFAULT", "NULL"];

fn valid_name(c: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^[A-Za-z][A-Za-z0-9_]*$").unwrap()).is_match(c)
}

/// The dataset as text. NONMEM reads "." as missing, so that is the default.
pub fn dataset_text(t: &Frame, delimiter: &str, missing: &str, header: bool) -> String {
    let d = match delimiter {
        "comma" => ',',
        "tab" => '\t',
        "semicolon" => ';',
        "space" => ' ',
        o => o.chars().next().unwrap_or(','),
    };
    let out = to_delimited(&t.columns, &t.text_rows(missing), d, missing);
    if header {
        out
    } else {
        out.split_once('\n').map(|x| x.1.to_string()).unwrap_or_default()
    }
}

/// `$DATA` and `$INPUT` lines for a NONMEM control stream. Text columns and reserved names are dropped with `=DROP`.
pub fn nonmem_input(t: &Frame, file: &str, ignore: &str) -> String {
    static NONW: OnceLock<Regex> = OnceLock::new();
    static LEAD: OnceLock<Regex> = OnceLock::new();
    let cols: Vec<String> = t
        .columns
        .iter()
        .enumerate()
        .map(|(j, c)| {
            let is_text = t.rows.iter().any(|r| matches!(r[j], Cell::Text(_)));
            if is_text || RESERVED.contains(&c.to_uppercase().as_str()) || !valid_name(c) {
                let nm = if valid_name(c) {
                    c.clone()
                } else {
                    let s = NONW.get_or_init(|| Regex::new(r"[^A-Za-z0-9_]").unwrap()).replace_all(c, "_").to_string();
                    let s = LEAD.get_or_init(|| Regex::new(r"^[^A-Za-z]+").unwrap()).replace(&s, "X").to_string();
                    if s.is_empty() {
                        "X".into()
                    } else {
                        s
                    }
                };
                format!("{nm}=DROP")
            } else {
                c.clone()
            }
        })
        .collect();
    let mut lines = vec![];
    let mut cur = String::from("$INPUT");
    for c in cols {
        if cur.chars().count() + 1 + c.chars().count() > 78 {
            lines.push(cur);
            cur = format!("      {c}");
        } else {
            cur.push(' ');
            cur.push_str(&c);
        }
    }
    lines.push(cur);
    format!("$DATA {file} IGNORE={ignore}\n{}", lines.join("\n"))
}

/// One row per column: type, missing, unique, min, max, mean, examples.
pub fn dictionary(t: &Frame) -> Vec<Value> {
    profile(t)
}
