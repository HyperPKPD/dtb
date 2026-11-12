//! BE & DDI Studio: bioequivalence analysis of a crossover or parallel study.

use super::{flag, map_err, req, string_table, typed, typed_req, Command, Ctx};
use crate::be::{analyze_be, records_from_table, residual_table, BeOptions, RecordMap};
use crate::core::error::{DtbError, Result};
use serde_json::{json, Map, Value};

pub fn commands() -> Vec<Command> {
    const COMMON: &[(&str, &str)] = &[
        ("table", "the data: CSV text or { columns, rows }"),
        ("map", "{ subject, treatment, period, sequence, params: [..] } column names"),
        ("param?", "the parameter to analyse, for example \"Cmax\""),
        ("params?", "or a list of parameters to analyse"),
        ("options?", "{ test, ref, alpha, limits, method: abe|abel|rsabe|ntid|np, welch, abel, npMargin }"),
        ("delimiter?", "for CSV text"),
    ];
    vec![
        Command {
            name: "be.analyze",
            summary: "Bioequivalence analysis: ABE, ABEL, RSABE, NTID or non-parametric, with ANOVA and carry-over checks.",
            params: COMMON,
            run: analyze,
        },
        Command { name: "be.residuals", summary: "Residuals and standardised residuals of the fitted model of one parameter.", params: COMMON, run: residuals },
    ]
}

struct Prepared {
    records: Vec<crate::be::Record>,
    warnings: Vec<String>,
    params: Vec<String>,
    options: BeOptions,
}

fn prepare(p: &Value, ctx: &Ctx) -> Result<Prepared> {
    let t = string_table(p, req(p, "table")?, ctx, "The table")?;
    let mut map: RecordMap = typed_req(p, "map")?;
    let mut params: Vec<String> = match p.get("params").and_then(Value::as_array) {
        Some(a) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        None => super::text(p, "param").map(|s| vec![s.to_string()]).unwrap_or_default(),
    };
    if params.is_empty() {
        params = map.params.clone();
    }
    if params.is_empty() {
        return Err(DtbError::Usage("Name the parameter to analyse (\"param\" or \"params\").".into()));
    }
    for q in &params {
        if !map.params.contains(q) {
            map.params.push(q.clone());
        }
    }
    let rec = records_from_table(&t, &map);
    if !rec.errors.is_empty() {
        return Err(DtbError::Input(rec.errors.join(" ")));
    }
    ctx.limit("subjects", rec.records.len(), ctx.config.limits.max_rows, "max_rows")?;
    Ok(Prepared {
        records: rec.records,
        warnings: rec.warnings,
        params,
        options: typed::<BeOptions>(p, "options").map_err(|_| DtbError::Usage("The parameter \"options\" is not valid.".into()))?,
    })
}

fn analyze(p: &Value, ctx: &Ctx) -> Result<Value> {
    let pr = prepare(p, ctx)?;
    let mut results = Map::new();
    for q in &pr.params {
        results.insert(q.clone(), serde_json::to_value(analyze_be(&pr.records, q, &pr.options))?);
    }
    Ok(json!({ "warnings": pr.warnings, "records": pr.records.len(), "results": results }))
}

fn residuals(p: &Value, ctx: &Ctx) -> Result<Value> {
    let pr = prepare(p, ctx)?;
    let q = &pr.params[0];
    let _ = (flag(p, "x", true), map_err::<()>(Ok(())));
    Ok(json!({ "param": q, "rows": residual_table(&pr.records, q, &pr.options) }))
}
