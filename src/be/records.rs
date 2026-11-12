//! Turning a table into records (one row per subject, treatment and period).

use crate::core::{num, Table};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};

/// A period label: a number when the whole column is numeric, otherwise text.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Per {
    N(f64),
    S(String),
}

impl Per {
    pub fn key(&self) -> String {
        match self {
            Per::N(v) => format!("{v}"),
            Per::S(s) => s.clone(),
        }
    }
}

fn natural(a: &str, b: &str) -> Ordering {
    let (ca, cb): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (mut i, mut j) = (0, 0);
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
            let na: f64 = ca[si..i].iter().collect::<String>().parse().unwrap_or(0.0);
            let nb: f64 = cb[sj..j].iter().collect::<String>().parse().unwrap_or(0.0);
            match na.partial_cmp(&nb).unwrap_or(Ordering::Equal) {
                Ordering::Equal => {}
                o => return o,
            }
        } else {
            let (x, y) = (ca[i].to_lowercase().next().unwrap_or(ca[i]), cb[j].to_lowercase().next().unwrap_or(cb[j]));
            match x.cmp(&y) {
                Ordering::Equal => {}
                o => return o,
            }
            i += 1;
            j += 1;
        }
    }
    (ca.len() - i).cmp(&(cb.len() - j))
}

pub fn cmp_per(a: &Per, b: &Per) -> Ordering {
    match (a, b) {
        (Per::N(x), Per::N(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        _ => natural(&a.key(), &b.key()),
    }
}

#[derive(Clone, Debug)]
pub struct Record {
    pub subject: String,
    pub sequence: String,
    pub period: Per,
    pub treatment: String,
    pub values: BTreeMap<String, f64>,
    pub row: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RecordMap {
    pub subject: String,
    pub treatment: String,
    pub period: String,
    pub sequence: String,
    pub params: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Records {
    pub records: Vec<Record>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Turn a table into records.
pub fn records_from_table(t: &Table, map: &RecordMap) -> Records {
    let ix = |n: &str| if n.is_empty() { None } else { t.col(n) };
    let (is, it, ip, iq) = (ix(&map.subject), ix(&map.treatment), ix(&map.period), ix(&map.sequence));
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    if is.is_none() {
        errors.push("Choose the subject column.".to_string());
    }
    if it.is_none() {
        errors.push("Choose the treatment column.".to_string());
    }
    let pcols: Vec<(String, usize)> = map.params.iter().filter_map(|p| t.col(p).map(|i| (p.clone(), i))).collect();
    if pcols.is_empty() {
        errors.push("Choose at least one parameter column.".to_string());
    }
    if !errors.is_empty() {
        return Records { records: vec![], warnings, errors };
    }
    let (is, it) = (is.unwrap(), it.unwrap());
    let mut records = Vec::new();
    let (mut blank, mut dupe) = (0, 0);
    let mut seen = HashSet::new();
    let period_numeric = ip.is_some_and(|ip| t.rows.iter().all(|r| r[ip].is_empty() || num(&r[ip]).is_finite()));
    for (k, r) in t.rows.iter().enumerate() {
        let subject = r[is].trim().to_string();
        let treatment = r[it].trim().to_string();
        if subject.is_empty() || treatment.is_empty() {
            blank += 1;
            continue;
        }
        let period = match ip {
            Some(ip) if period_numeric => {
                let v = num(&r[ip]);
                Per::N(if v.is_nan() { 1.0 } else { v })
            }
            Some(ip) => Per::S(r[ip].trim().to_string()),
            None => Per::N(1.0),
        };
        let key = format!("{subject}\u{1}{treatment}\u{1}{}", period.key());
        if !seen.insert(key) {
            dupe += 1;
        }
        let mut values = BTreeMap::new();
        for (name, i) in &pcols {
            let v = num(&r[*i]);
            if v.is_finite() {
                values.insert(name.clone(), v);
            }
        }
        records.push(Record { subject, sequence: iq.map(|iq| r[iq].trim().to_string()).unwrap_or_default(), period, treatment, values, row: k + 2 });
    }
    if blank > 0 {
        warnings.push(format!("{blank} row(s) without a subject or treatment were skipped."));
    }
    if dupe > 0 {
        warnings.push(format!("{dupe} row(s) repeat a subject, treatment and period; each is used as a separate observation, check the data."));
    }
    Records { records, warnings, errors }
}
