//! Prep Studio: profile a table, run the NONMEM dataset checks, replay a recipe, evaluate an expression, export a dataset.

use super::{flag, frame, req, text, Command, Ctx};
use crate::core::error::{DtbError, Result};
use crate::prep::checks::{check_dataset, CheckOptions};
use crate::prep::export::{dataset_text, nonmem_input};
use crate::prep::expr::{evaluate, Val};
use crate::prep::recipe::{fingerprint, read_recipe, run_recipe, sources_used, RunOptions};
use crate::prep::table::{profile, Frame};
use serde_json::{json, Value};

pub fn commands() -> Vec<Command> {
    vec![
        Command {
            name: "prep.profile",
            summary: "Per-column facts: type, missing, unique values, range, mean, examples.",
            params: &[("table", "CSV text or { columns, rows }"), ("delimiter?", "auto | comma | tab | semicolon | pipe | space")],
            run: profile_cmd,
        },
        Command {
            name: "prep.qc",
            summary: "Check a dataset against what NONMEM expects (IDs, TIME order, EVID/MDV/AMT/DV, BLQ, ADDL/II, covariates).",
            params: &[
                ("table", "the data: CSV text or { columns, rows }"),
                ("roles?", "{ ID: \"SUBJ\", … } column names when they are not the standard ones"),
                ("lloq?", "number"),
                ("covariates?", "list of covariate columns"),
                ("baseline?", "false to skip the 'varies within subject' note"),
            ],
            run: qc,
        },
        Command {
            name: "prep.run",
            summary: "Replay a Prep Studio recipe on source tables; returns the dataset, the per-step report and the checks.",
            params: &[
                ("recipe", "a recipe object (format nlmx-prep-recipe)"),
                ("sources", "{ name: table } for every file the recipe uses"),
                ("upTo?", "index of the last step to run"),
                ("qc?", "true or the options of prep.qc"),
                ("include?", "what to return: table (default), dataset, nonmem, profile"),
                ("export?", "{ delimiter: comma|tab|semicolon|space, missing, header, file, ignore }"),
            ],
            run,
        },
        Command {
            name: "prep.eval",
            summary: "Evaluate an expression for every row of a table.",
            params: &[("table", "the data: CSV text or { columns, rows }"), ("expr", "the expression"), ("by?", "group columns for lag, first, cumsum …")],
            run: eval,
        },
        Command {
            name: "prep.export",
            summary: "The dataset as delimited text and the $DATA/$INPUT lines of a control stream.",
            params: &[("table", "the data: CSV text or { columns, rows }"), ("export?", "{ delimiter, missing, header, file, ignore }")],
            run: export,
        },
    ]
}

fn table_of(p: &Value, ctx: &Ctx) -> Result<Frame> {
    frame(p, req(p, "table")?, ctx, "The table")
}

fn profile_cmd(p: &Value, ctx: &Ctx) -> Result<Value> {
    let t = table_of(p, ctx)?;
    Ok(json!({ "rows": t.rows.len(), "columns": t.columns.len(), "profile": profile(&t) }))
}

fn qc_options(v: Option<&Value>) -> Result<CheckOptions> {
    let Some(o) = v.filter(|x| x.is_object()) else { return Ok(CheckOptions::default()) };
    let strs = |k: &str| -> Vec<String> {
        o.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
    };
    let roles = o
        .get("roles")
        .and_then(Value::as_object)
        .map(|m| m.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
        .unwrap_or_default();
    Ok(CheckOptions {
        roles,
        lloq: o.get("lloq").and_then(Value::as_f64),
        covariates: strs("covariates"),
        baseline: o.get("baseline").and_then(Value::as_bool),
    })
}

fn qc(p: &Value, ctx: &Ctx) -> Result<Value> {
    let t = table_of(p, ctx)?;
    Ok(check_dataset(&t, &qc_options(Some(p))?))
}

fn export_text(t: &Frame, ex: Option<&Value>, ctx: &Ctx) -> (String, String) {
    let g = |k: &str| ex.and_then(|e| e.get(k)).and_then(Value::as_str);
    let delimiter = g("delimiter").unwrap_or("comma");
    let missing = g("missing").unwrap_or(&ctx.config.prep.missing);
    let header = ex.and_then(|e| e.get("header")).and_then(Value::as_bool).unwrap_or(true);
    (dataset_text(t, delimiter, missing, header), nonmem_input(t, g("file").unwrap_or("dataset.csv"), g("ignore").unwrap_or("@")))
}

fn run(p: &Value, ctx: &Ctx) -> Result<Value> {
    let recipe = read_recipe(req(p, "recipe")?).map_err(DtbError::Input)?;
    ctx.limit("steps", recipe.steps.len(), ctx.config.limits.max_steps, "max_steps")?;
    let src = req(p, "sources")?.as_object().ok_or_else(|| DtbError::Usage("\"sources\" must be an object of tables.".into()))?;
    let mut sources: Vec<(String, Frame)> = Vec::new();
    for (name, v) in src {
        sources.push((name.clone(), frame(p, v, ctx, &format!("The source \"{name}\""))?));
    }
    let missing: Vec<String> = sources_used(&recipe).into_iter().filter(|n| !sources.iter().any(|s| &s.0 == n)).collect();
    if !missing.is_empty() {
        return Err(DtbError::Input(format!(
            "The recipe needs the file(s) {} but \"sources\" does not have them.",
            missing.iter().map(|m| format!("\"{m}\"")).collect::<Vec<_>>().join(", ")
        )));
    }
    // did the files change since the recipe was written?
    let checks: Vec<Value> = recipe
        .sources
        .iter()
        .filter_map(|s| {
            let name = s.get("name")?.as_str()?;
            let (_, t) = sources.iter().find(|x| x.0 == name)?;
            let got = fingerprint(t);
            Some(json!({ "name": name, "recorded": s.get("fingerprint"), "actual": got, "match": s.get("fingerprint").map(|f| *f == got).unwrap_or(false) }))
        })
        .collect();
    let up_to = super::count(p, "upTo")?;
    let r = run_recipe(&recipe, &sources, &RunOptions { up_to });
    let include: Vec<&str> = match p.get("include").and_then(Value::as_array) {
        Some(a) => a.iter().filter_map(Value::as_str).collect(),
        None => vec!["table"],
    };
    let mut out = json!({ "name": recipe.name, "ok": !r.failed, "failed": r.failed, "steps": r.steps, "sourceChecks": checks });
    if let Some(t) = &r.table {
        out["rows"] = json!(t.rows.len());
        out["columns"] = json!(t.columns);
        if include.contains(&"table") {
            out["table"] = t.to_json();
        }
        if include.contains(&"dataset") || include.contains(&"nonmem") {
            let (d, n) = export_text(t, p.get("export"), ctx);
            if include.contains(&"dataset") {
                out["dataset"] = json!(d);
            }
            if include.contains(&"nonmem") {
                out["nonmem"] = json!(n);
            }
        }
        if include.contains(&"profile") {
            out["profile"] = json!(profile(t));
        }
        match p.get("qc") {
            Some(Value::Bool(true)) => out["qc"] = check_dataset(t, &CheckOptions::default()),
            Some(o @ Value::Object(_)) => out["qc"] = check_dataset(t, &qc_options(Some(o))?),
            _ => {}
        }
    }
    let _ = (flag(p, "x", false), text(p, "x"));
    Ok(out)
}

fn eval(p: &Value, ctx: &Ctx) -> Result<Value> {
    let t = table_of(p, ctx)?;
    let src = text(p, "expr").ok_or_else(|| DtbError::Usage("The parameter \"expr\" is required.".into()))?;
    let by: Vec<String> = p.get("by").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    let r = evaluate(&t, src, &by).map_err(|e| DtbError::Input(e.message))?;
    let values: Vec<Value> = r
        .values
        .iter()
        .map(|v| match v {
            Val::Null => Value::Null,
            Val::Bool(b) => json!(b),
            Val::Num(x) => json!(x),
            Val::Text(s) => json!(s),
        })
        .collect();
    Ok(json!({ "values": values, "columns": r.columns }))
}

fn export(p: &Value, ctx: &Ctx) -> Result<Value> {
    let t = table_of(p, ctx)?;
    let (d, n) = export_text(&t, p.get("export"), ctx);
    Ok(json!({ "dataset": d, "nonmem": n, "rows": t.rows.len(), "columns": t.columns }))
}
