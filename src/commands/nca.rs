//! BE & DDI Studio: non-compartmental analysis.

use super::{req, string_table, typed, typed_req, Command, Ctx};
use crate::core::error::{DtbError, Result};
use crate::nca::{nca_profile, nca_table, NcaMap, NcaOptions};
use serde_json::{json, Value};

pub fn commands() -> Vec<Command> {
    vec![
        Command {
            name: "nca.profile",
            summary: "NCA of one concentration-time profile.",
            params: &[("time", "list of times"), ("conc", "list of concentrations"), ("options?", "{ dose, lloq, zeroAtStart, minPoints, maxExtrap }")],
            run: profile,
        },
        Command {
            name: "nca.table",
            summary: "NCA of every subject-treatment-period profile of a long table.",
            params: &[
                ("table", "CSV text or { columns, rows }"),
                ("map", "{ subject, treatment, time, conc, period, sequence }"),
                ("options?", "as nca.profile"),
                ("profiles?", "include the profiles (default false)"),
            ],
            run: table,
        },
    ]
}

fn profile(p: &Value, ctx: &Ctx) -> Result<Value> {
    let t: Vec<f64> = typed_req(p, "time")?;
    let c: Vec<f64> = typed_req(p, "conc")?;
    if t.len() != c.len() {
        return Err(DtbError::Input(format!("time has {} values and conc has {}; they must pair up.", t.len(), c.len())));
    }
    ctx.limit("points", t.len(), ctx.config.limits.max_rows, "max_rows")?;
    let o: NcaOptions = typed(p, "options")?;
    Ok(serde_json::to_value(nca_profile(&t, &c, &o))?)
}

fn table(p: &Value, ctx: &Ctx) -> Result<Value> {
    let t = string_table(p, req(p, "table")?, ctx, "The table")?;
    let map: NcaMap = typed_req(p, "map")?;
    let o: NcaOptions = typed(p, "options")?;
    let r = nca_table(&t, &map, &o);
    if !r.errors.is_empty() && r.rows.is_empty() {
        return Err(DtbError::Input(r.errors.join(" ")));
    }
    let with = super::flag(p, "profiles", false);
    let mut v = json!({ "columns": r.columns, "rows": r.rows, "errors": r.errors });
    if with {
        v["profiles"] = serde_json::to_value(&r.profiles)?;
    }
    Ok(v)
}
