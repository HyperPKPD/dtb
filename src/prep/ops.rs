//! The steps of a recipe. Each step is `{ op, p }`, where `p` holds the settings. A step takes the working table and returns a new one;
//! it never changes its input. It may add `notes` (what it did) and `warnings` (what to look at). A step that cannot run returns an
//! error message that says what to change. This is a port of the JavaScript steps of the app and gives the same tables and notes.

use super::covformulas as cov;
use super::datetime::{parse_clock, parse_date_time, unit_ms, Dt};
use super::expr::{evaluate, to_cell, to_num, truthy, Val};
use super::table::{key_of, require_cols, Cell, Frame};
use crate::core::fixed;
use crate::core::js;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

pub struct Ctx<'a> {
    pub sources: &'a [(String, Frame)],
}
impl Ctx<'_> {
    pub fn get(&self, name: &str) -> Option<&Frame> {
        self.sources.iter().find(|(n, _)| n == name).map(|(_, t)| t)
    }
}

pub struct OpResult {
    pub table: Frame,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
    pub info: Option<Value>,
}
type R = Result<OpResult, String>;

fn result(table: Frame, notes: Vec<String>, warnings: Vec<String>, info: Option<Value>) -> R {
    Ok(OpResult { table, notes, warnings, info })
}

pub const OP_NAMES: &[&str] = &[
    "use",
    "columns",
    "rename",
    "filter",
    "sort",
    "dedupe",
    "join",
    "stack",
    "derive",
    "tonumber",
    "recode",
    "bin",
    "idmap",
    "limits",
    "time",
    "tad",
    "nmcols",
    "blq",
    "addl",
    "impute",
    "covariates",
];

/// The default settings of each step (what a step has when the recipe leaves a setting out).
pub fn defaults(op: &str) -> Option<Value> {
    Some(match op {
        "use" => json!({"source": ""}),
        "columns" => json!({"mode": "keep", "columns": []}),
        "rename" => json!({"pairs": [["", ""]]}),
        "filter" => json!({"action": "keep", "expr": "", "why": ""}),
        "sort" => json!({"pairs": [["", "asc"]]}),
        "dedupe" => json!({"keys": [], "keep": "first"}),
        "join" => json!({"source": "", "pairs": [["", ""]], "type": "left", "take": [], "dups": "error"}),
        "stack" => json!({"source": "", "pairs": [], "constants": []}),
        "derive" => json!({"name": "", "expr": "", "where": "", "by": []}),
        "tonumber" => json!({"columns": []}),
        "recode" => json!({"col": "", "out": "", "pairs": [["", ""]], "other": "missing", "otherValue": "", "ignoreCase": true}),
        "bin" => json!({"col": "", "out": "", "breaks": "", "labels": "", "right": true}),
        "idmap" => json!({"id": "", "out": "ID", "keep": "USUBJID", "order": "appearance", "start": 1}),
        "limits" => json!({"col": "", "min": "", "max": "", "action": "flag", "flag": ""}),
        "time" => json!({"id": "", "date": "", "clock": "", "format": "auto", "ref": "", "unit": "h", "out": "TIME"}),
        "tad" => json!({"id": "ID", "time": "TIME", "dose": "EVID == 1", "out": "TAD"}),
        "nmcols" => json!({"amt": "AMT", "dv": "DV", "evid": "", "cmtDose": 1, "cmtObs": 2, "cmt": "", "tidy": true}),
        "blq" => json!({"dv": "DV", "lloq": "", "lloqCol": "", "rule": "M1", "id": "ID", "obs": "", "flag": "BLQ"}),
        "addl" => json!({"id": "ID", "time": "TIME", "amt": "AMT", "dose": "", "tol": 0.01, "min": 3}),
        "impute" => json!({"columns": [], "id": "ID", "method": "locf", "value": "", "flag": true}),
        "covariates" => {
            json!({"formula": "bmi", "wt": "WT", "wtUnit": "kg", "ht": "HT", "htUnit": "cm", "age": "AGE", "scr": "SCR", "scrUnit": "mg/dL", "sex": "SEX", "male": "M, 1", "weightFor": "actual", "out": ""})
        }
        _ => return None,
    })
}

// ------------------------------------------------------------------------------------------------ small helpers

fn pl(n: usize, w: &str) -> String {
    format!("{} {w}{}", js::thousands(n), if n == 1 { "" } else { "s" })
}
fn pct(a: usize, b: usize) -> String {
    if b > 0 {
        format!("{}%", fixed(100.0 * a as f64 / b as f64, 1))
    } else {
        "0%".into()
    }
}
fn list(a: &[String], n: usize) -> String {
    if a.len() <= n {
        a.join(", ")
    } else {
        format!("{} and {} more", a[..n].join(", "), a.len() - n)
    }
}

/// `String(v)` of a JSON value, the way a template string writes it.
fn jstr(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::String(s) => s.clone(),
        Value::Number(n) => js::num_str(n.as_f64().unwrap_or(f64::NAN)),
        Value::Bool(b) => b.to_string(),
        Value::Array(a) => a.iter().map(|x| if x.is_null() { String::new() } else { jstr(x) }).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}
/// `Number(v)` of a JSON value.
fn jnum(v: &Value) -> f64 {
    match v {
        Value::Null => 0.0,
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::Bool(b) => f64::from(*b),
        Value::String(s) => js::number(s),
        _ => f64::NAN,
    }
}
fn jtruthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

struct P<'a>(&'a Value);
impl P<'_> {
    fn get(&self, k: &str) -> Option<&Value> {
        self.0.get(k)
    }
    /// The setting as trimmed text; empty when it is not set.
    fn s(&self, k: &str) -> String {
        match self.get(k) {
            None | Some(Value::Null) => String::new(),
            Some(v) => jstr(v).trim().to_string(),
        }
    }
    /// The setting as text, without trimming (None when not set).
    fn raw(&self, k: &str) -> Option<String> {
        match self.get(k) {
            None | Some(Value::Null) => None,
            Some(v) => Some(jstr(v)),
        }
    }
    fn is(&self, k: &str, v: &str) -> bool {
        matches!(self.get(k), Some(Value::String(s)) if s == v)
    }
    fn or(&self, k: &str, d: &str) -> String {
        match self.get(k) {
            Some(Value::String(s)) if !s.is_empty() => s.clone(),
            _ => d.to_string(),
        }
    }
    fn truthy(&self, k: &str) -> bool {
        jtruthy(self.get(k))
    }
    fn not_false(&self, k: &str) -> bool {
        !matches!(self.get(k), Some(Value::Bool(false)))
    }
    fn num(&self, k: &str) -> f64 {
        self.get(k).map_or(f64::NAN, jnum)
    }
    fn arr(&self, k: &str) -> Vec<String> {
        match self.get(k) {
            Some(Value::Array(a)) => a.iter().filter(|x| !matches!(x, Value::Null) && !matches!(x, Value::String(s) if s.is_empty())).map(jstr).collect(),
            _ => vec![],
        }
    }
    /// Pairs whose first element is not empty.
    fn pairs(&self, k: &str) -> Vec<(Value, Value)> {
        match self.get(k) {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|x| {
                    let x = x.as_array()?;
                    let a0 = x.first().cloned().unwrap_or(Value::Null);
                    if jstr(&if a0.is_null() { Value::String(String::new()) } else { a0.clone() }).trim().is_empty() {
                        return None;
                    }
                    Some((a0, x.get(1).cloned().unwrap_or(Value::Null)))
                })
                .collect(),
            _ => vec![],
        }
    }
}

fn plain_number_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^[+-]?([0-9]+\.?[0-9]*|\.[0-9]+)([eE][+-]?[0-9]+)?$").unwrap())
}
/// A cell from typed text: empty and "." are missing, plain numbers are numbers.
fn as_cell(s: &str) -> Cell {
    let t = s.trim();
    if t.is_empty() || t == "." {
        return Cell::Null;
    }
    let v = js::number(t);
    if plain_number_re().is_match(t) && v.is_finite() {
        Cell::Num(v)
    } else {
        Cell::Text(t.to_string())
    }
}
fn as_cell_v(v: &Value) -> Cell {
    if v.is_null() {
        Cell::Null
    } else {
        as_cell(&jstr(v))
    }
}

fn must_name(name: &str, what: &str) -> Result<String, String> {
    if name.is_empty() {
        return Err(format!("{what} needs a name."));
    }
    if name.contains(['\r', '\n', ',', '"']) {
        return Err(format!("{what}: the name \"{name}\" may not contain commas, quotes or line breaks."));
    }
    Ok(name.to_string())
}

/// Add a column, or replace it where it exists; returns its index.
fn set_column(t: &mut Frame, name: &str, values: Vec<Cell>) -> usize {
    match t.col(name) {
        Some(j) => {
            for (i, r) in t.rows.iter_mut().enumerate() {
                r[j] = values.get(i).cloned().unwrap_or(Cell::Null);
            }
            j
        }
        None => {
            let j = t.columns.len();
            t.columns.push(name.to_string());
            for (i, r) in t.rows.iter_mut().enumerate() {
                r.push(values.get(i).cloned().unwrap_or(Cell::Null));
            }
            j
        }
    }
}

fn eval(t: &Frame, src: &str, by: &[String]) -> Result<Vec<Val>, String> {
    evaluate(t, src, by).map(|e| e.values).map_err(|e| e.message)
}
fn idx_of(t: &Frame, name: &str) -> Option<usize> {
    if name.is_empty() {
        None
    } else {
        t.col(name)
    }
}
fn cols_ok(t: &Frame, names: &[&str]) -> Result<(), String> {
    require_cols(t, &names.iter().map(|s| s.to_string()).collect::<Vec<_>>(), "column")
}

// ------------------------------------------------------------------------------------------------ dispatch

pub fn apply(op: &str, t: Option<&Frame>, p: &Value, ctx: &Ctx) -> R {
    if op == "use" {
        return op_use(p, ctx);
    }
    let t = t.ok_or("There is no table yet. Start the recipe with \"Start from a file\".")?;
    let p = P(p);
    match op {
        "columns" => op_columns(t, &p),
        "rename" => op_rename(t, &p),
        "filter" => op_filter(t, &p),
        "sort" => op_sort(t, &p),
        "dedupe" => op_dedupe(t, &p),
        "join" => op_join(t, &p, ctx),
        "stack" => op_stack(t, &p, ctx),
        "derive" => op_derive(t, &p),
        "tonumber" => op_tonumber(t, &p),
        "recode" => op_recode(t, &p),
        "bin" => op_bin(t, &p),
        "idmap" => op_idmap(t, &p),
        "limits" => op_limits(t, &p),
        "time" => op_time(t, &p),
        "tad" => op_tad(t, &p),
        "nmcols" => op_nmcols(t, &p),
        "blq" => op_blq(t, &p),
        "addl" => op_addl(t, &p),
        "impute" => op_impute(t, &p),
        "covariates" => op_covariates(t, &p),
        other => Err(format!("The operation \"{other}\" is not known to this version.")),
    }
}

fn op_use(p: &Value, ctx: &Ctx) -> R {
    let name = P(p).raw("source").unwrap_or_default();
    let s = ctx.get(&name).ok_or_else(|| format!("The file \"{name}\" is not loaded. Add it in Files, or choose another."))?;
    result(s.clone(), vec![format!("{} and {} from {name}.", pl(s.rows.len(), "row"), pl(s.columns.len(), "column"))], vec![], None)
}

fn op_columns(t: &Frame, p: &P) -> R {
    let c = p.arr("columns");
    if c.is_empty() {
        return Err("Choose at least one column.".into());
    }
    require_cols(t, &c, "column")?;
    let mode = p.or("mode", "keep");
    let keep: Vec<String> = match mode.as_str() {
        "keep" => c.clone(),
        "drop" => t.columns.iter().filter(|x| !c.contains(x)).cloned().collect(),
        _ => c.iter().cloned().chain(t.columns.iter().filter(|x| !c.contains(x)).cloned()).collect(),
    };
    if keep.is_empty() {
        return Err("That would remove every column.".into());
    }
    let idx: Vec<usize> = keep.iter().map(|x| t.col(x).unwrap()).collect();
    let gone: Vec<String> = t.columns.iter().filter(|x| !keep.contains(x)).cloned().collect();
    let note = if gone.is_empty() { "Columns reordered.".to_string() } else { format!("Removed {}: {}.", pl(gone.len(), "column"), list(&gone, 6)) };
    result(Frame { columns: keep, rows: t.rows.iter().map(|r| idx.iter().map(|j| r[*j].clone()).collect()).collect() }, vec![note], vec![], None)
}

fn op_rename(t: &Frame, p: &P) -> R {
    let pr: Vec<(String, String)> = p
        .pairs("pairs")
        .into_iter()
        .filter(|(_, b)| !jstr(b).trim().is_empty() && !b.is_null())
        .map(|(a, b)| (jstr(&a), jstr(&b).trim().to_string()))
        .collect();
    if pr.is_empty() {
        return Err("Add at least one column and its new name.".into());
    }
    require_cols(t, &pr.iter().map(|x| x.0.clone()).collect::<Vec<_>>(), "column")?;
    let mut map: HashMap<&str, String> = HashMap::new();
    for (a, b) in &pr {
        map.insert(a.as_str(), must_name(b, "The new column")?);
    }
    let cols: Vec<String> = t.columns.iter().map(|c| map.get(c.as_str()).cloned().unwrap_or_else(|| c.clone())).collect();
    if let Some((i, d)) = cols.iter().enumerate().find(|(i, c)| cols.iter().position(|x| x == *c) != Some(*i)) {
        let _ = i;
        return Err(format!("Two columns would be called \"{d}\". Rename or drop one of them first."));
    }
    result(Frame { columns: cols, rows: t.rows.clone() }, pr.iter().map(|x| format!("{} → {}", x.0, x.1)).collect(), vec![], None)
}

fn op_filter(t: &Frame, p: &P) -> R {
    let src = p.s("expr");
    if src.is_empty() {
        return Err("Write the condition.".into());
    }
    let values = eval(t, &src, &[])?;
    let keep_true = p.or("action", "keep") == "keep";
    let rows: Vec<Vec<Cell>> = t.rows.iter().zip(&values).filter(|(_, v)| truthy(v) == keep_true).map(|(r, _)| r.clone()).collect();
    let gone = t.rows.len() - rows.len();
    let mut w = vec![];
    if rows.is_empty() && !t.rows.is_empty() {
        w.push("No rows are left. Check the condition.".to_string());
    }
    let note = if gone > 0 {
        format!("Removed {} ({}); {} left.", pl(gone, "row"), pct(gone, t.rows.len()), pl(rows.len(), "row"))
    } else {
        "No rows were removed.".into()
    };
    result(Frame { columns: t.columns.clone(), rows }, vec![note], w, Some(json!({"removed": gone, "why": p.s("why")})))
}

fn op_sort(t: &Frame, p: &P) -> R {
    let pr = p.pairs("pairs");
    if pr.is_empty() {
        return Err("Choose at least one column to sort by.".into());
    }
    let names: Vec<String> = pr.iter().map(|x| jstr(&x.0)).collect();
    require_cols(t, &names, "column")?;
    let spec: Vec<(usize, f64)> =
        pr.iter().map(|x| (t.col(&jstr(&x.0)).unwrap(), if matches!(&x.1, Value::String(s) if s == "desc") { -1.0 } else { 1.0 })).collect();
    let mut rows = t.rows.clone();
    rows.sort_by(|a, b| {
        for (j, d) in &spec {
            let (x, y) = (&a[*j], &b[*j]);
            let (mx, my) = (x.is_null(), y.is_null());
            if mx || my {
                if mx && my {
                    continue;
                }
                return if mx { std::cmp::Ordering::Greater } else { std::cmp::Ordering::Less };
            }
            if let (Cell::Num(p), Cell::Num(q)) = (x, y) {
                if p != q {
                    let o = if p < q { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
                    return if *d < 0.0 { o.reverse() } else { o };
                }
            } else {
                let o = js::natural_cmp(&x.text(), &y.text());
                if o != std::cmp::Ordering::Equal {
                    return if *d < 0.0 { o.reverse() } else { o };
                }
            }
        }
        std::cmp::Ordering::Equal
    });
    let desc = pr
        .iter()
        .map(|x| format!("{}{}", jstr(&x.0), if matches!(&x.1, Value::String(s) if s == "desc") { " (descending)" } else { "" }))
        .collect::<Vec<_>>()
        .join(", ");
    result(Frame { columns: t.columns.clone(), rows }, vec![format!("Sorted by {desc}.")], vec![], None)
}

fn op_dedupe(t: &Frame, p: &P) -> R {
    let keys = p.arr("keys");
    require_cols(t, &keys, "column")?;
    let idx: Vec<usize> = if keys.is_empty() { (0..t.columns.len()).collect() } else { keys.iter().map(|c| t.col(c).unwrap()).collect() };
    let k = |r: &Vec<Cell>| idx.iter().map(|j| if r[*j].is_null() { "\u{0}".to_string() } else { r[*j].text() }).collect::<Vec<_>>().join("\u{1}");
    let last = p.is("keep", "last");
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (i, r) in t.rows.iter().enumerate() {
        let key = k(r);
        if last || !seen.contains_key(&key) {
            seen.insert(key, i);
        }
    }
    let keep: HashSet<usize> = seen.values().copied().collect();
    let rows: Vec<Vec<Cell>> = t.rows.iter().enumerate().filter(|(i, _)| keep.contains(i)).map(|(_, r)| r.clone()).collect();
    let gone = t.rows.len() - rows.len();
    result(
        Frame { columns: t.columns.clone(), rows },
        vec![if gone > 0 { format!("Removed {}.", pl(gone, "duplicate row")) } else { "No duplicates found.".into() }],
        vec![],
        Some(json!({"removed": gone})),
    )
}

fn op_join(t: &Frame, p: &P, ctx: &Ctx) -> R {
    let source = p.raw("source").unwrap_or_default();
    let r = ctx.get(&source).ok_or_else(|| format!("The file \"{source}\" is not loaded."))?;
    let pr: Vec<(String, String)> = p.pairs("pairs").into_iter().filter(|(_, b)| jtruthy(Some(b))).map(|(a, b)| (jstr(&a), jstr(&b))).collect();
    if pr.is_empty() {
        return Err("Choose the key columns to match on.".into());
    }
    require_cols(t, &pr.iter().map(|x| x.0.clone()).collect::<Vec<_>>(), "column")?;
    require_cols(r, &pr.iter().map(|x| x.1.clone()).collect::<Vec<_>>(), &format!("column of {source}"))?;
    let l_idx: Vec<usize> = pr.iter().map(|x| t.col(&x.0).unwrap()).collect();
    let r_idx: Vec<usize> = pr.iter().map(|x| r.col(&x.1).unwrap()).collect();
    let r_key_names: HashSet<&str> = pr.iter().map(|x| x.1.as_str()).collect();
    let mut take: Vec<String> = p.arr("take").into_iter().filter(|c| !r_key_names.contains(c.as_str())).collect();
    if take.is_empty() {
        take = r.columns.iter().filter(|c| !r_key_names.contains(c.as_str())).cloned().collect();
    }
    require_cols(r, &take, &format!("column of {source}"))?;
    let t_idx: Vec<usize> = take.iter().map(|c| r.col(c).unwrap()).collect();
    let dups = p.or("dups", "error");
    let mut index: HashMap<String, &Vec<Cell>> = HashMap::new();
    let mut dup_keys = 0usize;
    for row in &r.rows {
        let k = r_idx.iter().map(|j| key_of(&row[*j])).collect::<Vec<_>>().join("\u{1}");
        match index.entry(k) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                dup_keys += 1;
                if dups == "error" {
                    let what = r_idx
                        .iter()
                        .map(|j| format!("{}={}", r.columns[*j], if row[*j].is_null() { "null".to_string() } else { row[*j].text() }))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(format!(
                        "{source} has more than one row for {what}. Choose \"Use the first\" or \"Use the last\", or remove the duplicates in that file first."
                    ));
                }
                if dups == "last" {
                    e.insert(row);
                }
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(row);
            }
        }
    }
    static EXT: OnceLock<Regex> = OnceLock::new();
    static NONW: OnceLock<Regex> = OnceLock::new();
    let stem = EXT.get_or_init(|| Regex::new(r"\.[^.]+$").unwrap()).replace(&source, "").to_string();
    let stem = NONW.get_or_init(|| Regex::new(r"[^A-Za-z0-9_]+").unwrap()).replace_all(&stem, "_").to_string();
    let new_names: Vec<String> = take.iter().map(|c| if t.columns.contains(c) { format!("{c}_{stem}") } else { c.clone() }).collect();
    let inner = p.is("type", "inner");
    let mut rows = Vec::new();
    let (mut hit, mut miss) = (0usize, 0usize);
    let mut miss_keys: Vec<String> = Vec::new();
    for row in &t.rows {
        let k = l_idx.iter().map(|j| key_of(&row[*j])).collect::<Vec<_>>().join("\u{1}");
        match index.get(&k) {
            Some(m) => {
                hit += 1;
                let mut o = row.clone();
                o.extend(t_idx.iter().map(|j| m[*j].clone()));
                rows.push(o);
            }
            None => {
                miss += 1;
                let mk = k.split('\u{1}').collect::<Vec<_>>().join("/");
                if !miss_keys.contains(&mk) {
                    miss_keys.push(mk);
                }
                if !inner {
                    let mut o = row.clone();
                    o.extend(t_idx.iter().map(|_| Cell::Null));
                    rows.push(o);
                }
            }
        }
    }
    let mut notes = vec![format!("{} matched; added {}.", pl(hit, "row"), new_names.join(", "))];
    let mut w = vec![];
    if miss > 0 {
        w.push(format!(
            "{} ({}) had no match in {source}{}; keys such as {}.",
            pl(miss, "row"),
            pct(miss, t.rows.len()),
            if inner { " and were removed" } else { "" },
            list(&miss_keys, 4)
        ));
    }
    if dup_keys > 0 {
        w.push(format!("{source} had {}; the {} row was used.", pl(dup_keys, "repeated key"), if dups == "last" { "last" } else { "first" }));
    }
    let renamed: Vec<String> = new_names.iter().zip(&take).filter(|(n, c)| n != c).map(|(n, _)| n.clone()).collect();
    if !renamed.is_empty() {
        notes.push(format!("Columns that already existed were added as {}.", list(&renamed, 6)));
    }
    let mut columns = t.columns.clone();
    columns.extend(new_names);
    result(Frame { columns, rows }, notes, w, Some(json!({"unmatched": miss, "removed": if inner { miss } else { 0 }})))
}

fn op_stack(t: &Frame, p: &P, ctx: &Ctx) -> R {
    let source = p.raw("source").unwrap_or_default();
    let s = ctx.get(&source).ok_or_else(|| format!("The file \"{source}\" is not loaded."))?;
    let mut map: Vec<(String, String)> = Vec::new(); // target column -> source column
    let put = |map: &mut Vec<(String, String)>, k: String, v: String| match map.iter_mut().find(|x| x.0 == k) {
        Some(e) => e.1 = v,
        None => map.push((k, v)),
    };
    for (a, b) in p.pairs("pairs") {
        if jtruthy(Some(&b)) {
            let b = jstr(&b);
            require_cols(s, std::slice::from_ref(&b), &format!("column of {source}"))?;
            let key = jstr(&a).trim().to_string();
            put(&mut map, if key.is_empty() { b.clone() } else { key }, b);
        }
    }
    for c in &s.columns {
        if !map.iter().any(|x| &x.1 == c) && !map.iter().any(|x| &x.0 == c) && t.columns.contains(c) {
            put(&mut map, c.clone(), c.clone());
        }
    }
    let consts: Vec<(String, Cell)> =
        p.pairs("constants").into_iter().filter(|x| !jstr(&x.0).trim().is_empty()).map(|x| (jstr(&x.0).trim().to_string(), as_cell_v(&x.1))).collect();
    let mut cols = t.columns.clone();
    for (k, _) in &map {
        if !cols.contains(k) {
            cols.push(k.clone());
        }
    }
    for (k, _) in &consts {
        if !cols.contains(k) {
            cols.push(k.clone());
        }
    }
    let nc = cols.len();
    let base: Vec<Vec<Cell>> = t
        .rows
        .iter()
        .map(|r| {
            let mut o = vec![Cell::Null; nc];
            for (j, c) in r.iter().enumerate() {
                o[j] = c.clone();
            }
            o
        })
        .collect();
    let add: Vec<Vec<Cell>> = s
        .rows
        .iter()
        .map(|r| {
            let mut o = vec![Cell::Null; nc];
            for (k, sc) in &map {
                o[cols.iter().position(|c| c == k).unwrap()] = r[s.col(sc).unwrap()].clone();
            }
            for (k, v) in &consts {
                o[cols.iter().position(|c| c == k).unwrap()] = v.clone();
            }
            o
        })
        .collect();
    let unmatched: Vec<String> = s.columns.iter().filter(|c| !map.iter().any(|x| &x.1 == *c)).cloned().collect();
    let mut w = vec![];
    if !unmatched.is_empty() {
        w.push(format!("Columns of {source} that were not carried over: {}.", list(&unmatched, 6)));
    }
    let total = base.len() + add.len();
    let n_add = add.len();
    let mut rows = base;
    rows.extend(add);
    result(Frame { columns: cols, rows }, vec![format!("Added {} from {source}; {} in all.", pl(n_add, "row"), pl(total, "row"))], w, None)
}

fn op_derive(t: &Frame, p: &P) -> R {
    let name = must_name(&p.s("name"), "The new column")?;
    let src = p.s("expr");
    if src.is_empty() {
        return Err("Write the expression.".into());
    }
    let by = p.arr("by");
    require_cols(t, &by, "column")?;
    let values = eval(t, &src, &by)?;
    let where_ = p.s("where");
    let sel: Option<Vec<bool>> = if where_.is_empty() { None } else { Some(eval(t, &where_, &by)?.iter().map(truthy).collect()) };
    let j0 = t.col(&name);
    let out: Vec<Cell> = values
        .iter()
        .enumerate()
        .map(|(i, v)| match &sel {
            Some(s) if !s[i] => j0.map_or(Cell::Null, |j| t.rows[i][j].clone()),
            _ => to_cell(v),
        })
        .collect();
    let nulls = out.iter().filter(|c| c.is_null()).count();
    let n = out.len();
    let mut nt = t.clone();
    set_column(&mut nt, &name, out);
    let mut w = vec![];
    if nulls > 0 && nulls == n {
        w.push("The expression gave no value for any row. Check the column names and the expression.".to_string());
    }
    result(nt, vec![format!("{} {name}: {}, {nulls} empty.", if j0.is_some() { "Replaced" } else { "Added" }, pl(n - nulls, "value"))], w, None)
}

fn op_tonumber(t: &Frame, p: &P) -> R {
    let c = p.arr("columns");
    if c.is_empty() {
        return Err("Choose the columns.".into());
    }
    require_cols(t, &c, "column")?;
    let mut nt = t.clone();
    let (mut notes, mut w) = (vec![], vec![]);
    for col in &c {
        let j = nt.col(col).unwrap();
        let mut bad = 0usize;
        let mut ex: Vec<String> = vec![];
        for r in nt.rows.iter_mut() {
            let v = r[j].clone();
            if matches!(v, Cell::Num(_) | Cell::Null) {
                continue;
            }
            match to_num(&Val::Text(v.text().trim().replace(',', ""))) {
                None => {
                    bad += 1;
                    let s = v.text();
                    if ex.len() < 3 && !ex.contains(&s) {
                        ex.push(s);
                    }
                    r[j] = Cell::Null;
                }
                Some(n) => r[j] = Cell::Num(n),
            }
        }
        notes.push(format!("{col}: converted{}.", if bad > 0 { format!(", {} could not be read", pl(bad, "value")) } else { String::new() }));
        if bad > 0 {
            w.push(format!("{col}: {} are not numbers (such as {}) and are now empty.", pl(bad, "value"), list(&ex, 6)));
        }
    }
    result(nt, notes, w, None)
}

fn op_recode(t: &Frame, p: &P) -> R {
    let col = p.s("col");
    require_cols(t, std::slice::from_ref(&col), "column")?;
    let pr = p.pairs("pairs");
    if pr.is_empty() {
        return Err("List at least one value to replace.".into());
    }
    let ic = p.truthy("ignoreCase");
    let norm = |c: &Cell| {
        let k = key_of(c);
        if ic {
            k.to_lowercase()
        } else {
            k
        }
    };
    let mut map: HashMap<String, Cell> = HashMap::new();
    for (a, b) in &pr {
        map.insert(norm(&as_cell_v(a)), as_cell_v(b));
    }
    let j = t.col(&col).unwrap();
    let mut un: Vec<(String, usize)> = vec![];
    let other = p.or("other", "");
    let mut out = Vec::with_capacity(t.rows.len());
    for r in &t.rows {
        let v = &r[j];
        if v.is_null() {
            out.push(Cell::Null);
            continue;
        }
        let k = norm(v);
        if let Some(m) = map.get(&k) {
            out.push(m.clone());
        } else if other == "value" {
            out.push(as_cell(&p.raw("otherValue").unwrap_or_default()));
        } else if other == "keep" {
            out.push(v.clone());
        } else {
            let s = v.text();
            match un.iter_mut().find(|x| x.0 == s) {
                Some(e) => e.1 += 1,
                None => un.push((s, 1)),
            }
            out.push(Cell::Null);
        }
    }
    let mut nt = t.clone();
    let target = p.s("out");
    set_column(&mut nt, if target.is_empty() { &col } else { &target }, out);
    let mut w = vec![];
    if !un.is_empty() {
        w.push(format!(
            "{} value(s) matched no rule and became empty: {}.",
            un.iter().map(|x| x.1).sum::<usize>(),
            list(&un.iter().map(|x| format!("\"{}\"", x.0)).collect::<Vec<_>>(), 6)
        ));
    }
    result(nt, vec![format!("{col}: {} applied.", pl(map.len(), "rule"))], w, None)
}

fn op_bin(t: &Frame, p: &P) -> R {
    let col = p.s("col");
    require_cols(t, std::slice::from_ref(&col), "column")?;
    let out = must_name(&p.s("out"), "The new column")?;
    static SPLIT: OnceLock<Regex> = OnceLock::new();
    static SPLIT2: OnceLock<Regex> = OnceLock::new();
    let mut br: Vec<f64> = SPLIT.get_or_init(|| Regex::new(r"[,;\s]+").unwrap()).split(&p.s("breaks")).filter(|x| !x.is_empty()).map(js::number).collect();
    if br.is_empty() || br.iter().any(|x| !x.is_finite()) {
        return Err("Cut points must be numbers separated by commas.".into());
    }
    br.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let lab: Vec<Cell> = if !p.s("labels").is_empty() {
        SPLIT2.get_or_init(|| Regex::new(r"[,;]+").unwrap()).split(&p.s("labels")).map(as_cell).collect()
    } else {
        (0..=br.len()).map(|i| Cell::Num(i as f64 + 1.0)).collect()
    };
    if lab.len() != br.len() + 1 {
        return Err(format!("{} cut points make {} groups, but {} codes were given.", br.len(), br.len() + 1, lab.len()));
    }
    let j = t.col(&col).unwrap();
    let right = p.not_false("right");
    let vals: Vec<Cell> = t
        .rows
        .iter()
        .map(|r| {
            let Cell::Num(v) = r[j] else { return Cell::Null };
            let mut g = 0;
            while g < br.len() && (if right { v > br[g] } else { v >= br[g] }) {
                g += 1;
            }
            lab[g].clone()
        })
        .collect();
    let mut nt = t.clone();
    set_column(&mut nt, &out, vals);
    result(nt, vec![format!("{out}: {} groups from {col}.", lab.len())], vec![], None)
}

fn op_idmap(t: &Frame, p: &P) -> R {
    let id = p.s("id");
    require_cols(t, std::slice::from_ref(&id), "column")?;
    let out = must_name(
        &{
            let s = p.s("out");
            if s.is_empty() {
                "ID".to_string()
            } else {
                s
            }
        },
        "The new column",
    )?;
    let keep = p.s("keep");
    let j = t.col(&id).unwrap();
    let mut first: Vec<(String, Cell)> = vec![];
    let mut seen: HashSet<String> = HashSet::new();
    for r in &t.rows {
        let k = key_of(&r[j]);
        if seen.insert(k.clone()) {
            first.push((k, r[j].clone()));
        }
    }
    let mut keys: Vec<String> = first.iter().map(|x| x.0.clone()).collect();
    if p.is("order", "sorted") {
        keys.sort_by(|a, b| js::natural_cmp(a, b));
    }
    let sv = p.num("start");
    let start = if sv.is_finite() { sv } else { 1.0 };
    let map: HashMap<&str, f64> = keys.iter().enumerate().map(|(i, k)| (k.as_str(), start + i as f64)).collect();
    let mut nt = t.clone();
    if !keep.is_empty() && keep != out {
        let orig: Vec<Cell> = nt.rows.iter().map(|r| r[j].clone()).collect();
        set_column(&mut nt, &keep, orig);
    }
    let vals: Vec<Cell> = nt.rows.iter().map(|r| map.get(key_of(&r[j]).as_str()).map_or(Cell::Null, |v| Cell::Num(*v))).collect();
    set_column(&mut nt, &out, vals);
    let mut w = vec![];
    if t.rows.iter().any(|r| r[j].is_null()) {
        w.push("Some rows have no subject identifier; their number is empty.".to_string());
    }
    let fmap: HashMap<&str, &Cell> = first.iter().map(|(k, c)| (k.as_str(), c)).collect();
    let info_map: Vec<Value> = keys.iter().map(|k| json!([fmap[k.as_str()].to_json(), map[k.as_str()]])).collect();
    result(
        nt,
        vec![format!("{} numbered {} to {}.", pl(map.len(), "subject"), js::num_str(start), js::num_str(start + map.len() as f64 - 1.0))],
        w,
        Some(json!({"map": info_map})),
    )
}

fn op_limits(t: &Frame, p: &P) -> R {
    let col = p.s("col");
    require_cols(t, std::slice::from_ref(&col), "column")?;
    let lo = if p.s("min").is_empty() { None } else { Some(p.num("min")) };
    let hi = if p.s("max").is_empty() { None } else { Some(p.num("max")) };
    if lo.is_none() && hi.is_none() {
        return Err("Give a lowest or a highest value.".into());
    }
    if lo.is_some_and(|x| !x.is_finite()) || hi.is_some_and(|x| !x.is_finite()) {
        return Err("The limits must be numbers.".into());
    }
    let j = t.col(&col).unwrap();
    let bad: Vec<bool> = t.rows.iter().map(|r| matches!(r[j], Cell::Num(v) if lo.is_some_and(|l| v < l) || hi.is_some_and(|h| v > h))).collect();
    let nb = bad.iter().filter(|b| **b).count();
    let mut nt = t.clone();
    let action = p.or("action", "");
    if action == "missing" {
        for (i, r) in nt.rows.iter_mut().enumerate() {
            if bad[i] {
                r[j] = Cell::Null;
            }
        }
    }
    if action == "cap" {
        for (i, r) in nt.rows.iter_mut().enumerate() {
            if bad[i] {
                let v = r[j].num().unwrap();
                r[j] = Cell::Num(match lo {
                    Some(l) if v < l => l,
                    _ => hi.unwrap_or(f64::NAN),
                });
            }
        }
    }
    if !p.s("flag").is_empty() || action == "flag" {
        let fname = must_name(
            &{
                let f = p.s("flag");
                if f.is_empty() {
                    format!("{col}_FLAG")
                } else {
                    f
                }
            },
            "The new column",
        )?;
        set_column(&mut nt, &fname, bad.iter().map(|b| Cell::Num(if *b { 1.0 } else { 0.0 })).collect());
    }
    let remove = action == "remove";
    let rows: Vec<Vec<Cell>> = if remove { nt.rows.iter().enumerate().filter(|(i, _)| !bad[*i]).map(|(_, r)| r.clone()).collect() } else { nt.rows.clone() };
    let suffix = match action.as_str() {
        "remove" => "; rows removed",
        "missing" => "; made empty",
        "cap" => "; capped",
        _ => "; flagged",
    };
    let note =
        format!("{} of {col} outside {} to {}{suffix}.", pl(nb, "value"), lo.map_or("−∞".to_string(), js::num_str), hi.map_or("∞".to_string(), js::num_str));
    let w = if nb > 0 { vec![format!("{} of {col} are outside the limits.", pl(nb, "value"))] } else { vec![] };
    result(Frame { columns: nt.columns, rows }, vec![note], w, Some(json!({"removed": if remove { nb } else { 0 }})))
}

fn op_time(t: &Frame, p: &P) -> R {
    let (id, dc, cc) = (p.s("id"), p.s("date"), p.s("clock"));
    cols_ok(t, &[&id, &dc, &cc])?;
    let out = must_name(
        &{
            let o = p.s("out");
            if o.is_empty() {
                "TIME".to_string()
            } else {
                o
            }
        },
        "The new column",
    )?;
    let di = t.col(&dc).ok_or("Choose the date column.")?;
    let ci = idx_of(t, &cc);
    let ii = t.col(&id).ok_or("Choose the subject column.")?;
    let format = p.or("format", "auto");
    let n = t.rows.len();
    let mut abs: Vec<Option<f64>> = vec![None; n];
    let mut errs: Vec<String> = vec![];
    let mut unread = 0usize;
    for (i, r) in t.rows.iter().enumerate() {
        if r[di].is_null() {
            continue;
        }
        match parse_date_time(&r[di], &format) {
            Dt::None => {
                unread += 1;
                if errs.len() < 3 {
                    let e = format!("\"{}\" is not a date I know", r[di].text());
                    if !errs.contains(&e) {
                        errs.push(e);
                    }
                }
            }
            Dt::Error(m) => {
                unread += 1;
                if errs.len() < 3 && !errs.contains(&m) {
                    errs.push(m);
                }
            }
            Dt::Ok { ms, has_time } => {
                let mut ms = ms;
                if !has_time {
                    if let Some(ci) = ci {
                        let c = parse_clock(&r[ci]);
                        if c.is_none() && !r[ci].is_null() {
                            unread += 1;
                            if errs.len() < 3 {
                                let e = format!("The clock time \"{}\" is not understood.", r[ci].text());
                                if !errs.contains(&e) {
                                    errs.push(e);
                                }
                            }
                            continue;
                        }
                        ms += c.unwrap_or(0.0);
                    }
                }
                abs[i] = Some(ms);
            }
        }
    }
    let dated = t.rows.iter().filter(|r| !r[di].is_null()).count();
    if !errs.is_empty() && unread as f64 > dated as f64 * 0.5 {
        return Err(format!("Most dates could not be read. {} Change \"Date order\".", errs[0]));
    }
    let ref_sel: Option<Vec<bool>> =
        if p.s("ref").is_empty() { None } else { Some(eval(t, &p.raw("ref").unwrap_or_default(), &[])?.iter().map(truthy).collect()) };
    let mut zero: HashMap<String, f64> = HashMap::new();
    for (i, r) in t.rows.iter().enumerate() {
        let Some(a) = abs[i] else { continue };
        if ref_sel.as_ref().is_some_and(|s| !s[i]) {
            continue;
        }
        let e = zero.entry(key_of(&r[ii])).or_insert(a);
        if a < *e {
            *e = a;
        }
    }
    let unit_key = p.or("unit", "h");
    let unit = unit_ms(&unit_key).ok_or_else(|| format!("The unit \"{unit_key}\" is not known (use min, h, d or w)."))?;
    let mut no_ref = 0usize;
    let vals: Vec<Option<f64>> = t
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let a = abs[i]?;
            match zero.get(&key_of(&r[ii])) {
                None => {
                    no_ref += 1;
                    None
                }
                Some(z) => Some(js::round(((a - z) / unit) * 1e6) / 1e6),
            }
        })
        .collect();
    let mut nt = t.clone();
    set_column(&mut nt, &out, vals.iter().map(|v| v.map_or(Cell::Null, Cell::Num)).collect());
    let mut w = vec![];
    if unread > 0 {
        w.push(format!("{} could not be read ({}); TIME is empty there.", pl(unread, "date"), errs.join("; ")));
    }
    if no_ref > 0 {
        let r = p.s("ref");
        w.push(format!(
            "{} belong to subjects with no reference row (\"{}\"); TIME is empty there.",
            pl(no_ref, "row"),
            if r.is_empty() { "earliest record".to_string() } else { r }
        ));
    }
    let neg = vals.iter().filter(|v| v.is_some_and(|x| x < 0.0)).count();
    if neg > 0 {
        w.push(format!("{} are before the reference (negative TIME), for example samples taken before the first dose.", pl(neg, "row")));
    }
    let uname = match unit_key.as_str() {
        "min" => "minutes",
        "d" => "days",
        "w" => "weeks",
        _ => "hours",
    };
    result(nt, vec![format!("{out}: {} values in {uname}.", vals.iter().filter(|v| v.is_some()).count())], w, None)
}

fn op_tad(t: &Frame, p: &P) -> R {
    let (id, tm) = (p.s("id"), p.s("time"));
    cols_ok(t, &[&id, &tm])?;
    let out = must_name(
        &{
            let o = p.s("out");
            if o.is_empty() {
                "TAD".to_string()
            } else {
                o
            }
        },
        "The new column",
    )?;
    let dose_src = p.s("dose");
    let is_dose: Vec<bool> = eval(t, if dose_src.is_empty() { "false" } else { &dose_src }, &[])?.iter().map(truthy).collect();
    let ii = t.col(&id).ok_or("Choose the subject column.")?;
    let ti = t.col(&tm).ok_or("Choose the TIME column.")?;
    let mut last: HashMap<String, f64> = HashMap::new();
    let mut before = 0usize;
    let vals: Vec<Cell> = t
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let k = key_of(&r[ii]);
            let Cell::Num(x) = r[ti] else { return Cell::Null };
            if is_dose[i] {
                last.insert(k.clone(), x);
            }
            match last.get(&k) {
                None => {
                    before += 1;
                    Cell::Null
                }
                Some(l) => Cell::Num(js::round((x - l) * 1e9) / 1e9),
            }
        })
        .collect();
    let mut nt = t.clone();
    set_column(&mut nt, &out, vals);
    let w = if before > 0 { vec![format!("{} come before the first dose of their subject; {out} is empty there.", pl(before, "row"))] } else { vec![] };
    result(nt, vec![format!("{out} added.")], w, None)
}

fn finite_or(v: f64, d: f64) -> f64 {
    if v.is_finite() {
        v
    } else {
        d
    }
}

fn op_nmcols(t: &Frame, p: &P) -> R {
    let (amt, dv, ev, cm) = (p.s("amt"), p.s("dv"), p.s("evid"), p.s("cmt"));
    cols_ok(t, &[&amt, &dv, &ev, &cm])?;
    if amt.is_empty() && dv.is_empty() {
        return Err("Choose the dose column or the concentration column (or both).".into());
    }
    let mut nt = t.clone();
    let (ai, di, ei, ci) = (idx_of(&nt, &amt), idx_of(&nt, &dv), idx_of(&nt, &ev), idx_of(&nt, &cm));
    let (mut evs, mut mdv, mut cmt) = (vec![], vec![], vec![]);
    let (mut both, mut neither, mut n_dose, mut n_obs) = (0usize, 0usize, 0usize, 0usize);
    let (cd, co) = (finite_or(p.num("cmtDose"), 1.0), finite_or(p.num("cmtObs"), 2.0));
    let tidy = p.not_false("tidy");
    for r in nt.rows.iter_mut() {
        let a = ai.and_then(|j| r[j].num());
        let d = di.and_then(|j| r[j].num());
        let e = match ei.and_then(|j| r[j].num()) {
            Some(e) => e,
            None => {
                if a.is_some_and(|a| a > 0.0) {
                    1.0
                } else {
                    0.0
                }
            }
        };
        if e == 0.0 && d.is_none() {
            neither += 1;
        }
        if e == 1.0 && d.is_some() {
            both += 1;
        }
        let is_dose = e == 1.0 || e == 4.0;
        if is_dose {
            n_dose += 1;
        } else if e == 0.0 {
            n_obs += 1;
        }
        evs.push(Cell::Num(e));
        mdv.push(Cell::Num(if e != 0.0 || d.is_none() { 1.0 } else { 0.0 }));
        cmt.push(Cell::Num(ci.and_then(|j| r[j].num()).unwrap_or(if is_dose { cd } else { co })));
        if tidy {
            if let Some(j) = ai {
                if !is_dose && (r[j].is_null() || r[j] == Cell::Num(0.0)) {
                    r[j] = Cell::Num(0.0);
                }
            }
            if let Some(j) = di {
                if is_dose {
                    r[j] = Cell::Null;
                }
            }
        }
    }
    set_column(&mut nt, "EVID", evs);
    set_column(&mut nt, "MDV", mdv);
    if ci.is_none() || cm != "CMT" {
        set_column(&mut nt, "CMT", cmt);
    }
    let mut w = vec![];
    if both > 0 && !tidy {
        w.push(format!("{} also have a concentration.", pl(both, "dose row")));
    }
    if neither > 0 {
        w.push(format!("{} have no concentration; MDV is 1 for them.", pl(neither, "observation row")));
    }
    result(nt, vec![format!("{}, {}; EVID, MDV and CMT set.", pl(n_dose, "dose record"), pl(n_obs, "observation record"))], w, None)
}

fn op_blq(t: &Frame, p: &P) -> R {
    let (dv, lc, id) = (p.s("dv"), p.s("lloqCol"), p.s("id"));
    cols_ok(t, &[&dv, &lc, &id])?;
    let rule = p.or("rule", "M1");
    if rule == "M6" && id.is_empty() {
        return Err("M6 works within each subject: choose the subject column.".into());
    }
    let lloq_num = if p.s("lloq").is_empty() { None } else { Some(p.num("lloq")) };
    if lloq_num.is_some_and(|l| !(l > 0.0)) {
        return Err("The LLOQ must be a positive number.".into());
    }
    let di = t.col(&dv).ok_or("Choose the concentration column.")?;
    let li = idx_of(t, &lc);
    let ii = idx_of(t, &id);
    let n = t.rows.len();
    let obs_src = p.s("obs");
    let is_obs: Vec<bool> = if obs_src.is_empty() {
        t.rows.iter().map(|r| !r[di].is_null()).collect()
    } else {
        eval(t, &p.raw("obs").unwrap_or_default(), &[])?.iter().map(truthy).collect()
    };
    let mut blq = vec![false; n];
    let mut lv: Vec<Option<f64>> = vec![None; n];
    let mut val: Vec<Cell> = t.rows.iter().map(|r| r[di].clone()).collect();
    let (mut text, mut no_l) = (0usize, 0usize);
    let mut odd: Vec<String> = vec![];
    static M1: OnceLock<Regex> = OnceLock::new();
    static M2: OnceLock<Regex> = OnceLock::new();
    let lt = M1.get_or_init(|| Regex::new(r"^<\s*=?\s*([0-9.]+(?:[eE][+-]?[0-9]+)?)$").unwrap());
    let marker = M2.get_or_init(|| Regex::new(r"(?i)^(blq|bql|bld|bloq|nd|lloq|<\s*lloq|<\s*loq|below.*)$").unwrap());
    for i in 0..n {
        if !is_obs[i] || val[i].is_null() {
            continue;
        }
        let row_l = li.and_then(|j| t.rows[i][j].num()).or(lloq_num);
        match &val[i] {
            Cell::Num(v) => {
                lv[i] = row_l;
                if row_l.is_some_and(|l| *v < l) {
                    blq[i] = true;
                }
            }
            Cell::Text(s) => {
                let s = s.trim().to_string();
                if let Some(m) = lt.captures(&s) {
                    blq[i] = true;
                    lv[i] = Some(js::number(&m[1]));
                    text += 1;
                } else if marker.is_match(&s) {
                    blq[i] = true;
                    lv[i] = row_l;
                    text += 1;
                    if row_l.is_none() {
                        no_l += 1;
                    }
                } else if let Some(n2) = to_num(&Val::Text(s.clone())) {
                    val[i] = Cell::Num(n2);
                    lv[i] = row_l;
                    if row_l.is_some_and(|l| n2 < l) {
                        blq[i] = true;
                    }
                } else {
                    if odd.len() < 4 && !odd.contains(&s) {
                        odd.push(s);
                    }
                    val[i] = Cell::Null;
                }
            }
            Cell::Null => {}
        }
    }
    if no_l > 0 {
        return Err(format!("{} are written as text without a limit (\"BLQ\"). Enter the LLOQ.", pl(no_l, "BLQ value")));
    }
    let n_obs = (0..n).filter(|i| is_obs[*i] && (!val[*i].is_null() || blq[*i])).count();
    let n_b = blq.iter().filter(|b| **b).count();
    if n_b > 0 && lloq_num.is_none() && li.is_none() && lv.iter().enumerate().all(|(i, x)| !blq[i] || x.is_none()) {
        return Err("Enter the LLOQ.".into());
    }
    let mut nt = t.clone();
    let mut keep = vec![true; n];
    let mut cens = vec![0.0; n];
    let mut out: Vec<Cell> = val.iter().map(|v| if matches!(v, Cell::Num(_) | Cell::Null) { v.clone() } else { Cell::Null }).collect();
    let (mut dropped, mut changed) = (0usize, 0usize);
    for i in 0..n {
        if !blq[i] {
            continue;
        }
        let l = lv[i].or(lloq_num);
        match rule.as_str() {
            "M1" => {
                keep[i] = false;
                dropped += 1;
            }
            "M3" | "M4" => {
                out[i] = l.map_or(Cell::Null, Cell::Num);
                cens[i] = 1.0;
                changed += 1;
            }
            "M5" => {
                out[i] = Cell::Num(l.unwrap_or(0.0) / 2.0);
                changed += 1;
            }
            "M7" => {
                out[i] = Cell::Num(0.0);
                changed += 1;
            }
            _ => {}
        }
    }
    if rule == "M6" {
        let mut groups: Vec<Vec<usize>> = vec![];
        let mut gi: HashMap<String, usize> = HashMap::new();
        if let Some(ii) = ii {
            for (i, r) in t.rows.iter().enumerate() {
                let k = key_of(&r[ii]);
                let g = *gi.entry(k).or_insert_with(|| {
                    groups.push(vec![]);
                    groups.len() - 1
                });
                groups[g].push(i);
            }
        }
        for idx in &groups {
            let mut prev = false;
            for &i in idx {
                if !is_obs[i] || (val[i].is_null() && !blq[i]) {
                    continue;
                }
                if blq[i] {
                    if !prev {
                        out[i] = Cell::Num(lv[i].or(lloq_num).unwrap_or(0.0) / 2.0);
                        changed += 1;
                    } else {
                        keep[i] = false;
                        dropped += 1;
                    }
                    prev = true;
                } else {
                    prev = false;
                }
            }
        }
    }
    for i in 0..n {
        if is_obs[i] && !blq[i] {
            if let Cell::Num(_) = val[i] {
                out[i] = val[i].clone();
            }
        }
    }
    for i in 0..n {
        nt.rows[i][di] = out[i].clone();
    }
    let flag = must_name(
        &{
            let f = p.s("flag");
            if f.is_empty() {
                "BLQ".to_string()
            } else {
                f
            }
        },
        "The new column",
    )?;
    set_column(&mut nt, &flag, blq.iter().map(|b| Cell::Num(f64::from(*b))).collect());
    if rule == "M3" || rule == "M4" {
        set_column(&mut nt, "CENS", cens.iter().map(|c| Cell::Num(*c)).collect());
        set_column(&mut nt, "LLOQ", (0..n).map(|i| if is_obs[i] { lv[i].or(lloq_num).map_or(Cell::Null, Cell::Num) } else { Cell::Null }).collect());
    }
    let rows: Vec<Vec<Cell>> = nt.rows.iter().enumerate().filter(|(i, _)| keep[*i]).map(|(_, r)| r.clone()).collect();
    let mut w = vec![];
    if !odd.is_empty() {
        w.push(format!(
            "Some concentrations are text that is not a number or a BLQ marker (such as {}); they were made empty.",
            list(&odd.iter().map(|x| format!("\"{x}\"")).collect::<Vec<_>>(), 6)
        ));
    }
    if n_obs > 0 && n_b as f64 / n_obs as f64 > 0.2 {
        w.push(format!("{} of the observations are BLQ: the choice of method matters (consider M3 or M4).", pct(n_b, n_obs)));
    }
    if rule == "M4" {
        w.push("M4 is M3 plus a positivity constraint: both are set in the control stream, the data are the same.".to_string());
    }
    let mid = match rule.as_str() {
        "M1" => format!("{} removed", pl(dropped, "record")),
        "M6" => format!("{} set to LLOQ/2, {}", pl(changed, "record"), pl(dropped, "removed")),
        _ => format!("{} changed", pl(changed, "record")),
    };
    let tail = if text > 0 { format!(" {} {}.", pl(text, "value"), if text == 1 { "was a text marker" } else { "were text markers" }) } else { String::new() };
    let note = format!("{} of {} ({}); {rule}: {mid}.{tail}", pl(n_b, "BLQ record"), pl(n_obs, "observation"), pct(n_b, n_obs));
    result(Frame { columns: nt.columns, rows }, vec![note], w, Some(json!({"removed": dropped, "blq": n_b, "obs": n_obs, "text": text})))
}

fn op_addl(t: &Frame, p: &P) -> R {
    let (id, tm, am) = (p.s("id"), p.s("time"), p.s("amt"));
    cols_ok(t, &[&id, &tm, &am])?;
    let (ii, ti, ai) =
        (t.col(&id).ok_or("Choose the subject column.")?, t.col(&tm).ok_or("Choose the TIME column.")?, t.col(&am).ok_or("Choose the dose amount column.")?);
    let dose: Vec<bool> = if p.s("dose").is_empty() {
        t.rows.iter().map(|r| r[ai].num().is_some_and(|a| a > 0.0)).collect()
    } else {
        eval(t, &p.raw("dose").unwrap_or_default(), &[])?.iter().map(truthy).collect()
    };
    let tv = p.num("tol");
    let tol = if tv.is_finite() { tv.abs() } else { 0.01 };
    let mv = p.num("min");
    let min_n = (if mv.is_finite() && mv != 0.0 { mv } else { 3.0 }).max(2.0) as usize;
    let same: Vec<usize> = [am.as_str(), "EVID", "CMT", "RATE", "SS", "DUR"].iter().filter_map(|c| t.col(c)).collect();
    let same_dose = |a: &Vec<Cell>, b: &Vec<Cell>| same.iter().all(|j| a[*j] == b[*j]);
    let mut groups: Vec<Vec<usize>> = vec![];
    let mut gi: HashMap<String, usize> = HashMap::new();
    for (i, r) in t.rows.iter().enumerate() {
        if !dose[i] {
            continue;
        }
        let g = *gi.entry(key_of(&r[ii])).or_insert_with(|| {
            groups.push(vec![]);
            groups.len() - 1
        });
        groups[g].push(i);
    }
    let tkey = |i: usize| match &t.rows[i][ti] {
        Cell::Num(v) => *v,
        _ => 0.0,
    };
    let mut drop: HashSet<usize> = HashSet::new();
    let mut addl: HashMap<usize, (f64, f64)> = HashMap::new();
    let mut collapsed = 0usize;
    for idx in &groups {
        let mut s = idx.clone();
        s.sort_by(|a, b| tkey(*a).partial_cmp(&tkey(*b)).unwrap_or(std::cmp::Ordering::Equal));
        let mut a = 0;
        while a < s.len() {
            let mut b = a;
            let r0 = &t.rows[s[a]];
            if r0[ti].num().is_none() || r0[ai].num().is_none() {
                a += 1;
                continue;
            }
            let mut interval: Option<f64> = None;
            while b + 1 < s.len() {
                let (nx, cur) = (&t.rows[s[b + 1]], &t.rows[s[b]]);
                let (Some(tn), Some(tc)) = (nx[ti].num(), cur[ti].num()) else { break };
                if !same_dose(r0, nx) {
                    break;
                }
                let dt = tn - tc;
                if dt <= 0.0 {
                    break;
                }
                match interval {
                    None => interval = Some(dt),
                    Some(iv) if (dt - iv).abs() > tol => break,
                    _ => {}
                }
                b += 1;
            }
            let count = b - a + 1;
            if count >= min_n {
                addl.insert(s[a], ((count - 1) as f64, js::round(interval.unwrap_or(0.0) * 1e6) / 1e6));
                for q in a + 1..=b {
                    drop.insert(s[q]);
                }
                collapsed += count - 1;
            }
            a = b + 1;
        }
    }
    let mut nt = t.clone();
    let av: Vec<Cell> = (0..t.rows.len()).map(|i| if dose[i] { Cell::Num(addl.get(&i).map_or(0.0, |x| x.0)) } else { Cell::Null }).collect();
    let iv: Vec<Cell> = (0..t.rows.len()).map(|i| if dose[i] { Cell::Num(addl.get(&i).map_or(0.0, |x| x.1)) } else { Cell::Null }).collect();
    set_column(&mut nt, "ADDL", av);
    set_column(&mut nt, "II", iv);
    let rows: Vec<Vec<Cell>> = nt.rows.iter().enumerate().filter(|(i, _)| !drop.contains(i)).map(|(_, r)| r.clone()).collect();
    let note = if collapsed > 0 {
        format!("{} of regular doses collapsed; {} removed.", pl(addl.len(), "run"), pl(collapsed, "dose row"))
    } else {
        "No runs of identical, regularly spaced doses were found.".into()
    };
    result(Frame { columns: nt.columns, rows }, vec![note], vec![], Some(json!({"removed": collapsed})))
}

fn op_impute(t: &Frame, p: &P) -> R {
    let cols = p.arr("columns");
    if cols.is_empty() {
        return Err("Choose the columns.".into());
    }
    let id = p.s("id");
    let mut need = cols.clone();
    need.push(id.clone());
    require_cols(t, &need, "column")?;
    let m = p.or("method", "locf");
    if matches!(m.as_str(), "locf" | "locf-only" | "nocb") && id.is_empty() {
        return Err("Carrying values needs the subject column.".into());
    }
    let mut nt = t.clone();
    let (mut notes, mut w) = (vec![], vec![]);
    let ii = idx_of(&nt, &id);
    let mut groups: Vec<Vec<usize>> = vec![];
    if let Some(ii) = ii {
        let mut gi: HashMap<String, usize> = HashMap::new();
        for (i, r) in nt.rows.iter().enumerate() {
            let g = *gi.entry(key_of(&r[ii])).or_insert_with(|| {
                groups.push(vec![]);
                groups.len() - 1
            });
            groups[g].push(i);
        }
    }
    let flag = p.not_false("flag");
    for c in &cols {
        let j = nt.col(c).unwrap();
        let before: Vec<Cell> = nt.rows.iter().map(|r| r[j].clone()).collect();
        let miss0 = before.iter().filter(|v| v.is_null()).count();
        if matches!(m.as_str(), "locf" | "locf-only" | "nocb") {
            for idx in &groups {
                if m != "nocb" {
                    let mut cur = Cell::Null;
                    for &i in idx {
                        if !nt.rows[i][j].is_null() {
                            cur = nt.rows[i][j].clone();
                        } else if !cur.is_null() {
                            nt.rows[i][j] = cur.clone();
                        }
                    }
                }
                if m == "nocb" || m == "locf" {
                    let mut cur = Cell::Null;
                    for &i in idx.iter().rev() {
                        if !nt.rows[i][j].is_null() {
                            cur = nt.rows[i][j].clone();
                        } else if !cur.is_null() {
                            nt.rows[i][j] = cur.clone();
                        }
                    }
                }
            }
        } else {
            let fill: Cell;
            if m == "constant" {
                if p.s("value").is_empty() {
                    return Err("Enter the fixed value.".into());
                }
                fill = as_cell(&p.raw("value").unwrap_or_default());
            } else {
                let mut per: Vec<Cell> = vec![];
                if ii.is_some() {
                    for idx in &groups {
                        if let Some(v) = idx.iter().map(|i| &before[*i]).find(|x| !x.is_null()) {
                            per.push(v.clone());
                        }
                    }
                } else {
                    per.extend(before.iter().filter(|v| !v.is_null()).cloned());
                }
                if per.is_empty() {
                    w.push(format!("{c}: no value to take a {m} from; left as it is."));
                    continue;
                }
                if m == "mode" {
                    let mut cnt: Vec<(Cell, usize)> = vec![];
                    let mut pos: HashMap<String, usize> = HashMap::new();
                    for v in &per {
                        let k = match v {
                            Cell::Num(x) => format!("n{}", x.to_bits()),
                            o => format!("t{}", o.text()),
                        };
                        match pos.get(&k) {
                            Some(q) => cnt[*q].1 += 1,
                            None => {
                                pos.insert(k, cnt.len());
                                cnt.push((v.clone(), 1));
                            }
                        }
                    }
                    cnt.sort_by(|a, b| b.1.cmp(&a.1));
                    fill = cnt[0].0.clone();
                } else {
                    let mut nums: Vec<f64> = per.iter().filter_map(Cell::num).collect();
                    nums.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    if nums.is_empty() {
                        w.push(format!("{c}: not numeric; use the most common value instead."));
                        continue;
                    }
                    let f = if m == "mean" {
                        nums.iter().sum::<f64>() / nums.len() as f64
                    } else if nums.len() % 2 == 1 {
                        nums[(nums.len() - 1) / 2]
                    } else {
                        (nums[nums.len() / 2 - 1] + nums[nums.len() / 2]) / 2.0
                    };
                    fill = Cell::Num(js::round(f * 1e6) / 1e6);
                }
            }
            for r in nt.rows.iter_mut() {
                if r[j].is_null() {
                    r[j] = fill.clone();
                }
            }
            notes.push(format!("{c}: filled with {}.", fill.text()));
        }
        let after: Vec<Cell> = nt.rows.iter().map(|r| r[j].clone()).collect();
        let filled = after.iter().zip(&before).filter(|(a, b)| b.is_null() && !a.is_null()).count();
        let still = after.iter().filter(|v| v.is_null()).count();
        if flag && filled > 0 {
            set_column(&mut nt, &format!("{c}_IMP"), after.iter().zip(&before).map(|(a, b)| Cell::Num(f64::from(b.is_null() && !a.is_null()))).collect());
        }
        notes.push(format!(
            "{c}: {} filled of {miss0} missing{}.",
            pl(filled, "value"),
            if still > 0 { format!("; {still} still empty") } else { String::new() }
        ));
        if still > 0 {
            w.push(format!("{c}: {still} value(s) are still empty (no value to carry in those subjects)."));
        }
    }
    result(nt, notes, w, None)
}

fn op_covariates(t: &Frame, p: &P) -> R {
    let fid = p.s("formula");
    let f = cov::FORMULAS.iter().find(|x| x.id == fid).ok_or("Choose what to calculate.")?;
    let wf = p.or("weightFor", "actual");
    let mut needs: Vec<&str> = f.needs.to_vec();
    if f.id == "crcl-cg" && wf != "actual" {
        needs.push("ht");
    }
    let mut need: Vec<(&str, String)> = vec![];
    for k in &needs {
        let v = p.s(k);
        if v.is_empty() {
            let what = match *k {
                "wt" => "weight",
                "ht" => "height",
                "age" => "age",
                "scr" => "serum creatinine",
                _ => "sex",
            };
            return Err(format!("Choose the {what} column."));
        }
        need.push((k, v));
    }
    require_cols(t, &need.iter().map(|x| x.1.clone()).collect::<Vec<_>>(), "column")?;
    let out = must_name(
        &{
            let o = p.s("out");
            if o.is_empty() {
                f.out.to_string()
            } else {
                o
            }
        },
        "The new column",
    )?;
    let idx: HashMap<&str, usize> = need.iter().map(|(k, v)| (*k, t.col(v).unwrap())).collect();
    let male_key = |s: &str| key_of(&as_cell(s)).to_lowercase();
    static SPL: OnceLock<Regex> = OnceLock::new();
    let male_txt = p.s("male");
    let male_set: HashSet<String> = SPL.get_or_init(|| Regex::new(r"[,;]+").unwrap()).split(&male_txt).map(male_key).filter(|x| !x.is_empty()).collect();
    let is_male = |c: &Cell| -> Option<bool> {
        if c.is_null() {
            None
        } else {
            Some(male_set.contains(&key_of(c).to_lowercase()))
        }
    };
    let get = |r: &Vec<Cell>, k: &str| idx.get(k).and_then(|j| r[*j].num());
    let (wt_unit, ht_unit, scr_unit) = (p.or("wtUnit", ""), p.or("htUnit", ""), p.or("scrUnit", ""));
    let mut bad = 0usize;
    let mut sex_seen: Vec<String> = vec![];
    let vals: Vec<Cell> = t
        .rows
        .iter()
        .map(|r| {
            let kg = cov::to_kg(get(r, "wt"), &wt_unit);
            let cm = cov::to_cm(get(r, "ht"), &ht_unit);
            let age = get(r, "age");
            let scr = cov::to_mg_dl(get(r, "scr"), &scr_unit);
            let male = idx.get("sex").and_then(|j| is_male(&r[*j]));
            if let Some(j) = idx.get("sex") {
                if !r[*j].is_null() {
                    let s = r[*j].text();
                    if !sex_seen.contains(&s) {
                        sex_seen.push(s);
                    }
                }
            }
            let v = match f.id {
                "bmi" => cov::bmi(kg, cm),
                "bsa-mosteller" => cov::bsa_mosteller(kg, cm),
                "bsa-dubois" => cov::bsa_dubois(kg, cm),
                "ibw" => cov::ibw(cm, male),
                "lbm-james" => cov::lbm_james(kg, cm, male),
                "crcl-cg" => {
                    let mut w = kg;
                    if wf != "actual" {
                        let cm_h = cov::to_cm(get(r, "ht"), &ht_unit);
                        let i = cm_h.and_then(|c| cov::ibw(Some(c), male));
                        w = match wf.as_str() {
                            "ideal" => i,
                            "adjusted" => cm_h.and_then(|c| cov::adj_bw(kg, Some(c), male)),
                            _ => match (i, kg) {
                                (Some(i), Some(k)) => Some(k.min(i)),
                                _ => None,
                            },
                        };
                    }
                    cov::crcl_cg(age, w, scr, male)
                }
                "egfr-ckdepi2021" => cov::egfr_ckdepi2021(age, scr, male),
                "egfr-ckdepi2009" => cov::egfr_ckdepi2009(age, scr, male),
                "egfr-mdrd" => cov::egfr_mdrd(age, scr, male),
                "egfr-schwartz" => cov::egfr_schwartz(cm, scr),
                _ => None,
            };
            match v {
                None => {
                    bad += 1;
                    Cell::Null
                }
                Some(v) => Cell::Num(js::round(v * 1e4) / 1e4),
            }
        })
        .collect();
    let mut nt = t.clone();
    set_column(&mut nt, &out, vals);
    let mut w = vec![];
    if bad > 0 {
        w.push(format!("{out} is empty for {} (a needed value is missing, not a number or impossible).", pl(bad, "row")));
    }
    if idx.contains_key("sex") && !sex_seen.is_empty() && !sex_seen.iter().any(|s| male_set.contains(&male_key(s))) {
        w.push(format!("None of the sex values ({}) matches \"{}\"; everyone was treated as female.", list(&sex_seen, 6), p.s("male")));
    }
    result(nt, vec![format!("{} → {out}.", f.label.split(" (").next().unwrap_or(f.label))], w, None)
}
