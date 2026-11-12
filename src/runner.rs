//! Running one command: assembling its parameters (files, `--set`, references), calling it, and wrapping the result in the envelope.

use crate::commands::{self, Ctx};
use crate::core::error::{DtbError, Result};
use crate::core::hash::sha256_json;
use crate::core::io::{read_text, InputRef};
use crate::core::provenance::{envelope, Meta, PROTOCOL, VERSION};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// Set `a.b.c` in a JSON object, creating objects on the way. Numeric path parts index arrays.
pub fn set_path(root: &mut Value, path: &str, value: Value) -> Result<()> {
    if path.is_empty() || path.split('.').any(str::is_empty) {
        return Err(DtbError::Usage(format!("\"{path}\" is not a valid parameter name (use a.b.c).")));
    }
    let mut cur = root;
    let parts: Vec<&str> = path.split('.').collect();
    for (i, part) in parts.iter().enumerate() {
        let last = i == parts.len() - 1;
        if let Ok(idx) = part.parse::<usize>() {
            if !cur.is_array() {
                if cur.is_null() || cur.as_object().is_some_and(Map::is_empty) {
                    *cur = json!([]);
                } else {
                    return Err(DtbError::Usage(format!("\"{path}\": {} is not a list.", parts[..i].join("."))));
                }
            }
            let a = cur.as_array_mut().unwrap();
            if idx > a.len() || idx > 100_000 {
                return Err(DtbError::Usage(format!("\"{path}\": index {idx} is out of range.")));
            }
            if idx == a.len() {
                a.push(Value::Null);
            }
            if last {
                a[idx] = value;
                return Ok(());
            }
            cur = &mut a[idx];
        } else {
            if !cur.is_object() {
                if cur.is_null() {
                    *cur = json!({});
                } else {
                    return Err(DtbError::Usage(format!(
                        "\"{path}\": {} is not an object.",
                        if i == 0 { "the parameters".to_string() } else { parts[..i].join(".") }
                    )));
                }
            }
            let o = cur.as_object_mut().unwrap();
            if last {
                o.insert((*part).to_string(), value);
                return Ok(());
            }
            cur = o.entry((*part).to_string()).or_insert(Value::Null);
        }
    }
    Ok(())
}

/// A value given as text on the command line: JSON when it parses as JSON, otherwise a string.
pub fn parse_value(s: &str) -> Value {
    serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.to_string()))
}

/// Read a file for a parameter: JSON files become JSON values, everything else text (a table, a recipe is JSON).
pub fn read_param_file(path: &str, ctx: &Ctx) -> Result<Value> {
    let full = if std::path::Path::new(path).is_absolute() { std::path::PathBuf::from(path) } else { ctx.base.join(path) };
    let (text, mut r) = read_text(&full.to_string_lossy(), ctx.max_bytes())?;
    r.path = path.to_string();
    ctx.inputs.borrow_mut().push(r);
    if path.to_ascii_lowercase().ends_with(".json") {
        serde_json::from_str(&text).map_err(|e| DtbError::Input(format!("{path} is not valid JSON: {e}")))
    } else {
        Ok(Value::String(text))
    }
}

/// Replace `{ "$file": "path" }` by the file's content and `{ "$ref": "step.path" }` by a part of an earlier step's result.
/// `files_allowed` is false in the sandbox, where a request must never name a file.
pub fn resolve_refs(v: &mut Value, ctx: &Ctx, results: &HashMap<String, Value>, files_allowed: bool, depth: usize) -> Result<()> {
    if depth > 64 {
        return Err(DtbError::Input("The parameters are nested too deeply.".into()));
    }
    match v {
        Value::Object(o) => {
            if o.len() == 1 {
                if let Some(p) = o.get("$file").and_then(Value::as_str).map(String::from) {
                    if !files_allowed {
                        return Err(DtbError::Usage("\"$file\" is not allowed here (sandbox).".into()));
                    }
                    *v = read_param_file(&p, ctx)?;
                    return Ok(());
                }
                if let Some(r) = o.get("$ref").and_then(Value::as_str).map(String::from) {
                    let mut parts = r.split('.');
                    let step = parts.next().unwrap_or("");
                    let mut cur = results.get(step).ok_or_else(|| DtbError::Usage(format!("\"$ref\": \"{r}\": no earlier step is called \"{step}\".")))?;
                    for part in parts {
                        cur = match cur {
                            Value::Object(m) => m.get(part),
                            Value::Array(a) => part.parse::<usize>().ok().and_then(|i| a.get(i)),
                            _ => None,
                        }
                        .ok_or_else(|| DtbError::Usage(format!("\"$ref\": \"{r}\": there is nothing at \"{part}\".")))?;
                    }
                    *v = cur.clone();
                    return Ok(());
                }
            }
            for x in o.values_mut() {
                resolve_refs(x, ctx, results, files_allowed, depth + 1)?;
            }
        }
        Value::Array(a) => {
            for x in a {
                resolve_refs(x, ctx, results, files_allowed, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub struct Outcome {
    pub envelope: Value,
    pub result: Value,
    pub params_sha256: String,
    pub inputs: Vec<InputRef>,
}

/// Run a command on assembled parameters.
pub fn execute(name: &str, params: &Value, ctx: &Ctx) -> Result<Outcome> {
    let cmd = commands::resolve(name).ok_or_else(|| {
        let near: Vec<&'static str> = commands::all().iter().map(|c| c.name).filter(|n| n.split('.').next() == name.split('.').next()).collect();
        DtbError::Usage(format!(
            "Unknown command \"{name}\".{}",
            if near.is_empty() { " Run `dtb commands` for the list.".to_string() } else { format!(" Did you mean {}?", near.join(", ")) }
        ))
    })?;
    if !params.is_object() {
        return Err(DtbError::Usage("The parameters must be a JSON object.".into()));
    }
    // a misspelt parameter must never be ignored silently
    let declared: Vec<&str> = cmd.params.iter().map(|(n, _)| n.trim_end_matches('?')).collect();
    let has_table = declared.contains(&"table");
    if let Some(o) = params.as_object() {
        for k in o.keys() {
            if !declared.contains(&k.as_str()) && !(has_table && k == "delimiter") {
                let list = if declared.is_empty() { "none".to_string() } else { declared.join(", ") };
                return Err(DtbError::Usage(format!(
                    "{} has no parameter \"{k}\". Its parameters: {list}. (`dtb describe {}` explains them.)",
                    cmd.name, cmd.name
                )));
            }
        }
    }
    let started = std::time::Instant::now();
    let name = cmd.name;
    let result = (cmd.run)(params, ctx)?;
    let params_sha256 = sha256_json(params);
    let inputs = ctx.inputs.borrow().clone();
    let seed = commands::count(params, "seed").ok().flatten().map(|s| s as u64).or(ctx.config.general.seed);
    let meta = Meta {
        version: VERSION,
        protocol: PROTOCOL,
        command: name.to_string(),
        seed,
        config_sha256: ctx.config.sha256(),
        params_sha256: params_sha256.clone(),
        inputs: inputs.clone(),
    };
    let run = ctx.timing.then(|| json!({ "elapsedMs": started.elapsed().as_secs_f64() * 1000.0, "threads": rayon::current_num_threads() }));
    Ok(Outcome { envelope: envelope(&meta, result.clone(), run), result, params_sha256, inputs })
}

/// The part of a JSON value at `a.b.0.c` (empty path: the value itself).
pub fn pick<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = v;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        cur = match cur {
            Value::Object(m) => m.get(part)?,
            Value::Array(a) => a.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(cur)
}
