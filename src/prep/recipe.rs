//! A recipe is the list of steps that turns the source files into the analysis dataset. It is plain data (JSON) so it can be saved,
//! reviewed, compared and replayed on the same files or on a refreshed delivery of them. This reads and runs the recipe files of
//! Prep Studio (`format: "nlmx-prep-recipe"`).

use super::ops::{self, Ctx};
use super::table::Frame;
use serde_json::{json, Map, Value};

pub const RECIPE_FORMAT: &str = "nlmx-prep-recipe";
pub const RECIPE_VERSION: u64 = 1;

#[derive(Clone, Debug)]
pub struct Step {
    pub id: String,
    pub op: String,
    pub enabled: bool,
    pub note: String,
    pub p: Value,
}

#[derive(Clone, Debug)]
pub struct Recipe {
    pub name: String,
    pub description: String,
    pub sources: Vec<Value>,
    pub steps: Vec<Step>,
}

/// Read a recipe from JSON; the message says what is wrong when it is not a recipe this version understands.
pub fn read_recipe(r: &Value) -> Result<Recipe, String> {
    if r.get("format").and_then(Value::as_str) != Some(RECIPE_FORMAT) {
        return Err("This file is not a Prep Studio recipe.".into());
    }
    match r.get("version").and_then(Value::as_u64) {
        Some(v) if v <= RECIPE_VERSION => {}
        other => {
            return Err(format!(
                "This recipe was made by a newer version of Prep Studio (recipe version {}); this one reads version {RECIPE_VERSION}.",
                other.map_or("?".to_string(), |v| v.to_string())
            ))
        }
    }
    let steps = r.get("steps").and_then(Value::as_array).ok_or("The recipe has no list of steps.")?;
    let mut out = Vec::with_capacity(steps.len());
    for (i, s) in steps.iter().enumerate() {
        let op = s.get("op").and_then(Value::as_str).unwrap_or("");
        let Some(mut p) = ops::defaults(op) else {
            return Err(format!("Step {} uses an operation (\"{}\") that this version does not know.", i + 1, if op.is_empty() { "undefined" } else { op }));
        };
        if let (Some(base), Some(over)) = (p.as_object_mut(), s.get("p").and_then(Value::as_object)) {
            for (k, v) in over {
                base.insert(k.clone(), v.clone());
            }
        }
        out.push(Step {
            id: s.get("id").and_then(Value::as_str).filter(|x| !x.is_empty()).map_or_else(|| format!("s{}", i + 1), String::from),
            op: op.to_string(),
            enabled: s.get("enabled") != Some(&Value::Bool(false)),
            note: s.get("note").and_then(Value::as_str).unwrap_or("").to_string(),
            p,
        });
    }
    Ok(Recipe {
        name: r.get("name").and_then(Value::as_str).unwrap_or("Recipe").to_string(),
        description: r.get("description").and_then(Value::as_str).unwrap_or("").to_string(),
        sources: r.get("sources").and_then(Value::as_array).cloned().unwrap_or_default(),
        steps: out,
    })
}

/// A short, stable description of a table, stored in a recipe so a replay can tell when a file has changed.
pub fn fingerprint(t: &Frame) -> Value {
    let mut h: u32 = 2_166_136_261;
    let mut mix = |s: &str| {
        for u in s.encode_utf16() {
            h ^= u as u32;
            h = h.wrapping_mul(16_777_619);
        }
        h ^= 0x1f;
        h = h.wrapping_mul(16_777_619);
    };
    t.columns.iter().for_each(|c| mix(c));
    let n = t.rows.len();
    let step = (n / 500).max(1);
    let mut i = 0;
    while i < n {
        for v in &t.rows[i] {
            mix(&v.text());
        }
        i += step;
    }
    json!({"rows": n, "columns": t.columns.len(), "names": t.columns.join("|").encode_utf16().count(), "hash": format!("{h:08x}")})
}

#[derive(Default, Clone)]
pub struct RunOptions {
    /// Index of the last step to run (the later ones are `skipped`).
    pub up_to: Option<usize>,
}

pub struct Run {
    pub table: Option<Frame>,
    pub steps: Vec<Value>,
    pub failed: bool,
}

/// Run the steps in order on the given sources. The first step that fails stops the run; the steps after it are `skipped`.
pub fn run_recipe(recipe: &Recipe, sources: &[(String, Frame)], opts: &RunOptions) -> Run {
    let ctx = Ctx { sources };
    let mut out = Vec::new();
    let mut table: Option<Frame> = None;
    let mut failed = false;
    for (i, s) in recipe.steps.iter().enumerate() {
        let rb = table.as_ref().map(|t| t.rows.len());
        let cb = table.as_ref().map(|t| t.columns.len());
        let mut base = Map::new();
        base.insert("id".into(), json!(s.id));
        base.insert("op".into(), json!(s.op));
        base.insert("rowsBefore".into(), json!(rb));
        base.insert("colsBefore".into(), json!(cb));
        base.insert("notes".into(), json!([]));
        base.insert("warnings".into(), json!([]));
        base.insert("error".into(), Value::Null);
        if !s.enabled {
            base.insert("status".into(), json!("off"));
            base.insert("rowsAfter".into(), json!(rb));
            base.insert("colsAfter".into(), json!(cb));
            out.push(Value::Object(base));
            continue;
        }
        if failed || opts.up_to.is_some_and(|u| i > u) {
            base.insert("status".into(), json!("skipped"));
            base.insert("rowsAfter".into(), Value::Null);
            base.insert("colsAfter".into(), Value::Null);
            out.push(Value::Object(base));
            continue;
        }
        let r = if s.op != "use" && table.is_none() {
            Err("There is no table yet. Start the recipe with \"Start from a file\".".to_string())
        } else if s.op == "use" && table.is_some() {
            Err("\"Start from a file\" can only be the first step; use \"Add rows\" or \"Add columns\" to bring in more files.".to_string())
        } else {
            ops::apply(&s.op, table.as_ref(), &s.p, &ctx)
        };
        match r {
            Ok(res) => {
                base.insert("status".into(), json!("ok"));
                base.insert("rowsAfter".into(), json!(res.table.rows.len()));
                base.insert("colsAfter".into(), json!(res.table.columns.len()));
                base.insert("notes".into(), json!(res.notes));
                base.insert("warnings".into(), json!(res.warnings));
                if let Some(info) = res.info {
                    base.insert("info".into(), info);
                }
                table = Some(res.table);
            }
            Err(e) => {
                failed = true;
                base.insert("status".into(), json!("error"));
                base.insert("rowsAfter".into(), Value::Null);
                base.insert("colsAfter".into(), Value::Null);
                base.insert("error".into(), json!(e));
            }
        }
        out.push(Value::Object(base));
    }
    Run { table, steps: out, failed }
}

/// The source names a recipe needs.
pub fn sources_used(recipe: &Recipe) -> Vec<String> {
    let mut s: Vec<String> = vec![];
    for st in &recipe.steps {
        if st.enabled && matches!(st.op.as_str(), "use" | "join" | "stack") {
            if let Some(n) = st.p.get("source").and_then(Value::as_str).filter(|n| !n.is_empty()) {
                if !s.iter().any(|x| x == n) {
                    s.push(n.to_string());
                }
            }
        }
    }
    s
}
