//! The commands. Every command is a function from a JSON parameter object to a JSON result, so the command line, the pipeline runner
//! and the apps' servers all reach the same code. The registry below is the single list of what dtb can do.

use crate::core::config::Config;
use crate::core::error::{DtbError, Result};
use crate::core::io::InputRef;
use crate::core::{csvio, Table};
use crate::prep::table::Frame;
use crate::trial::Streams;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::cell::RefCell;
use std::path::PathBuf;

pub mod be;
pub mod ddi;
pub mod info;
pub mod nca;
pub mod power;
pub mod prep;
pub mod renal;
pub mod stats;
pub mod trial;

/// What a running command can see: the configuration and the options of the call.
pub struct Ctx {
    pub config: Config,
    pub timing: bool,
    /// Files read for this call, for the provenance record.
    pub inputs: RefCell<Vec<InputRef>>,
    /// The folder that relative `$file` references in pipelines are resolved against.
    pub base: PathBuf,
}

impl Ctx {
    pub fn new(config: Config, timing: bool, base: PathBuf) -> Ctx {
        Ctx { config, timing, inputs: RefCell::new(vec![]), base }
    }
    pub fn max_bytes(&self) -> u64 {
        self.config.limits.max_input_mb * 1_048_576
    }
    pub fn limit(&self, what: &str, n: usize, max: usize, key: &str) -> Result<()> {
        if n > max {
            Err(DtbError::Limit(format!("{what}: {n} is above the limit of {max} (limits.{key}). Raise it in the configuration if this is intended.")))
        } else {
            Ok(())
        }
    }
}

pub type Handler = fn(&Value, &Ctx) -> Result<Value>;

pub struct Command {
    pub name: &'static str,
    pub summary: &'static str,
    /// `(parameter, description)`; a trailing `?` on the name marks an optional one.
    pub params: &'static [(&'static str, &'static str)],
    pub run: Handler,
}

pub fn all() -> Vec<Command> {
    let mut v = Vec::new();
    v.extend(info::commands());
    v.extend(stats::commands());
    v.extend(prep::commands());
    v.extend(trial::commands());
    v.extend(be::commands());
    v.extend(power::commands());
    v.extend(ddi::commands());
    v.extend(nca::commands());
    v.extend(renal::commands());
    v
}

pub fn find(name: &str) -> Option<Command> {
    all().into_iter().find(|c| c.name == name)
}

/// The same, forgiving about case and dashes (`trial.er-curve` finds `trial.erCurve`), and accepting a space or `/` for the dot.
pub fn resolve(name: &str) -> Option<Command> {
    let norm = |s: &str| s.chars().filter(|c| *c != '-' && *c != '_').collect::<String>().to_ascii_lowercase().replace(['/', ' '], ".");
    let want = norm(name);
    all().into_iter().find(|c| c.name == name || norm(c.name) == want)
}

// ------------------------------------------------------------------------------------------------ reading parameters

pub fn invalid(what: &str, e: impl std::fmt::Display) -> DtbError {
    DtbError::Input(format!("{what}: {e}"))
}

pub fn obj<'a>(p: &'a Value, what: &str) -> Result<&'a serde_json::Map<String, Value>> {
    p.as_object().ok_or_else(|| DtbError::Usage(format!("{what} must be a JSON object.")))
}

pub fn req<'a>(p: &'a Value, key: &str) -> Result<&'a Value> {
    p.get(key).filter(|v| !v.is_null()).ok_or_else(|| DtbError::Usage(format!("The parameter \"{key}\" is required.")))
}

/// A typed parameter; missing means the default. A wrong shape is reported with the parameter's name.
pub fn typed<T: DeserializeOwned + Default>(p: &Value, key: &str) -> Result<T> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(T::default()),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| invalid(&format!("The parameter \"{key}\" is not valid"), e)),
    }
}
pub fn typed_req<T: DeserializeOwned>(p: &Value, key: &str) -> Result<T> {
    serde_json::from_value(req(p, key)?.clone()).map_err(|e| invalid(&format!("The parameter \"{key}\" is not valid"), e))
}

pub fn num(p: &Value, key: &str) -> Result<Option<f64>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => Ok(n.as_f64()),
        Some(Value::String(s)) if !s.trim().is_empty() => {
            s.trim().parse::<f64>().map(Some).map_err(|_| DtbError::Usage(format!("The parameter \"{key}\" must be a number, not \"{s}\".")))
        }
        Some(o) => Err(DtbError::Usage(format!("The parameter \"{key}\" must be a number, not {o}."))),
    }
}
pub fn count(p: &Value, key: &str) -> Result<Option<usize>> {
    match num(p, key)? {
        None => Ok(None),
        Some(x) if x >= 0.0 && x.fract() == 0.0 && x < 1e15 => Ok(Some(x as usize)),
        Some(x) => Err(DtbError::Usage(format!("The parameter \"{key}\" must be a whole number of at least 0, not {x}."))),
    }
}
pub fn text<'a>(p: &'a Value, key: &str) -> Option<&'a str> {
    p.get(key).and_then(Value::as_str)
}
pub fn flag(p: &Value, key: &str, default: bool) -> bool {
    p.get(key).and_then(Value::as_bool).unwrap_or(default)
}
pub fn seed(p: &Value, ctx: &Ctx, default: u64) -> Result<u64> {
    Ok(count(p, "seed")?.map(|s| s as u64).or(ctx.config.general.seed).unwrap_or(default))
}
pub fn streams(p: &Value, default: &str) -> Result<Streams> {
    match text(p, "streams").unwrap_or(default) {
        "js" => Ok(Streams::Js),
        "blocks" => Ok(Streams::Blocks),
        o => Err(DtbError::Usage(format!("streams must be \"blocks\" or \"js\", not \"{o}\"."))),
    }
}

fn delimiter_char(name: &str) -> Option<char> {
    Some(match name {
        "comma" | "," => ',',
        "tab" | "\\t" | "\t" => '\t',
        "semicolon" | ";" => ';',
        "pipe" | "|" => '|',
        "space" | " " => ' ',
        _ => return None,
    })
}

fn csv_delimiter(p: &Value, ctx: &Ctx) -> Result<Option<char>> {
    let d = text(p, "delimiter").unwrap_or(&ctx.config.prep.delimiter);
    if d == "auto" {
        return Ok(None);
    }
    delimiter_char(d).map(Some).ok_or_else(|| DtbError::Usage(format!("delimiter must be auto, comma, tab, semicolon, pipe or space, not \"{d}\".")))
}

fn check_size(ctx: &Ctx, rows: usize, cols: usize, what: &str) -> Result<()> {
    ctx.limit(&format!("{what} rows"), rows, ctx.config.limits.max_rows, "max_rows")?;
    ctx.limit(&format!("{what} columns"), cols, ctx.config.limits.max_columns, "max_columns")
}

/// A table as text cells, from CSV text or from `{ "columns": [...], "rows": [[...]] }`.
pub fn string_table(p: &Value, v: &Value, ctx: &Ctx, what: &str) -> Result<Table> {
    match v {
        Value::String(s) => {
            let parsed = csvio::parse_delimited(s, csv_delimiter(p, ctx)?);
            check_size(ctx, parsed.table.rows.len(), parsed.table.columns.len(), what)?;
            if parsed.table.columns.is_empty() {
                return Err(DtbError::Input(format!("{what} is empty.")));
            }
            Ok(parsed.table)
        }
        Value::Object(_) => {
            let cols = v.get("columns").and_then(Value::as_array).ok_or_else(|| DtbError::Input(format!("{what} needs a list of columns.")))?;
            let rows = v.get("rows").and_then(Value::as_array).ok_or_else(|| DtbError::Input(format!("{what} needs a list of rows.")))?;
            check_size(ctx, rows.len(), cols.len(), what)?;
            let cell = |x: &Value| match x {
                Value::Null => String::new(),
                Value::String(s) => s.clone(),
                Value::Number(n) => crate::core::js::num_str(n.as_f64().unwrap_or(f64::NAN)),
                o => o.to_string(),
            };
            let columns: Vec<String> = cols.iter().map(&cell).collect();
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                let a = r.as_array().ok_or_else(|| DtbError::Input(format!("Every row of {what} must be a list.")))?;
                let mut row: Vec<String> = a.iter().map(&cell).collect();
                row.resize(columns.len(), String::new());
                out.push(row);
            }
            Ok(Table { columns, rows: out })
        }
        _ => Err(DtbError::Input(format!("{what} must be CSV text or an object with columns and rows."))),
    }
}

/// A typed table (numbers, text, missing) from CSV text or from `{ "columns", "rows" }` with typed cells.
pub fn frame(p: &Value, v: &Value, ctx: &Ctx, what: &str) -> Result<Frame> {
    match v {
        Value::String(_) => Ok(Frame::from_text(&string_table(p, v, ctx, what)?)),
        Value::Object(_) => {
            let f = Frame::from_json(v).map_err(|e| DtbError::Input(format!("{what}: {e}")))?;
            check_size(ctx, f.rows.len(), f.columns.len(), what)?;
            Ok(f)
        }
        _ => Err(DtbError::Input(format!("{what} must be CSV text or an object with columns and rows."))),
    }
}

pub fn map_err<T>(r: std::result::Result<T, String>) -> Result<T> {
    r.map_err(DtbError::Input)
}
