//! Statistical helpers: descriptive summaries and distribution functions.

use super::{num, text, typed_req, Command, Ctx};
use crate::core::error::{DtbError, Result};
use crate::stats::*;
use serde_json::{json, Value};

pub fn commands() -> Vec<Command> {
    vec![
        Command {
            name: "stats.describe",
            summary: "n, mean, SD, quartiles, geometric mean and a Wilson interval of a list of numbers.",
            params: &[("values", "list of numbers"), ("quantiles?", "extra quantiles, e.g. [0.05, 0.95]")],
            run: describe,
        },
        Command {
            name: "stats.dist",
            summary: "Distribution function: cdf or quantile of normal, t, chisq or f.",
            params: &[
                ("dist", "norm | t | chisq | f"),
                ("fn", "cdf | quantile"),
                ("x?", "value (cdf)"),
                ("p?", "probability (quantile)"),
                ("df?", "degrees of freedom (t, chisq, f numerator)"),
                ("df2?", "denominator degrees of freedom (f)"),
            ],
            run: dist,
        },
    ]
}

fn describe(p: &Value, ctx: &Ctx) -> Result<Value> {
    let v: Vec<f64> = typed_req::<Vec<Option<f64>>>(p, "values")?.into_iter().flatten().filter(|x| x.is_finite()).collect();
    ctx.limit("values", v.len(), ctx.config.limits.max_rows, "max_rows")?;
    if v.is_empty() {
        return Err(DtbError::Input("There are no numbers to summarise.".into()));
    }
    let mut out = serde_json::to_value(summary(&v))?;
    let pos: Vec<f64> = v.iter().copied().filter(|x| *x > 0.0).collect();
    if pos.len() == v.len() {
        out["geomean"] = json!(geomean(&v));
    }
    if let Some(qs) = p.get("quantiles").and_then(Value::as_array) {
        let s = sorted(&v);
        out["quantiles"] = json!(qs.iter().filter_map(Value::as_f64).map(|q| json!([q, quantile_sorted(&s, q)])).collect::<Vec<_>>());
    }
    Ok(out)
}

fn dist(p: &Value, _ctx: &Ctx) -> Result<Value> {
    let d = text(p, "dist").ok_or_else(|| DtbError::Usage("The parameter \"dist\" is required.".into()))?;
    let f = text(p, "fn").unwrap_or("cdf");
    let (x, pr, df, df2) = (num(p, "x")?, num(p, "p")?, num(p, "df")?, num(p, "df2")?);
    let need = |v: Option<f64>, n: &str| v.ok_or_else(|| DtbError::Usage(format!("The parameter \"{n}\" is required.")));
    let v = match (d, f) {
        ("norm", "cdf") => norm_cdf(need(x, "x")?),
        ("norm", "quantile") => norm_quantile(need(pr, "p")?),
        ("t", "cdf") => t_cdf(need(x, "x")?, need(df, "df")?),
        ("t", "quantile") => t_quantile(need(pr, "p")?, need(df, "df")?),
        ("chisq", "cdf") => chi2_cdf(need(x, "x")?, need(df, "df")?),
        ("chisq", "quantile") => chi2_quantile(need(pr, "p")?, need(df, "df")?),
        ("f", "cdf") => f_cdf(need(x, "x")?, need(df, "df")?, need(df2, "df2")?),
        ("f", "quantile") => f_quantile(need(pr, "p")?, need(df, "df")?, need(df2, "df2")?),
        _ => return Err(DtbError::Usage(format!("dist must be norm, t, chisq or f and fn must be cdf or quantile (got {d}/{f})."))),
    };
    Ok(json!({ "value": v }))
}
