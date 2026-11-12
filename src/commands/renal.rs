//! BE & DDI Studio: renal and hepatic impairment.

use super::{num, text, typed, typed_req, Command, Ctx};
use crate::core::error::{DtbError, Result};
use crate::impair::*;
use serde_json::{json, Value};

pub fn commands() -> Vec<Command> {
    vec![
        Command {
            name: "renal.egfr",
            summary: "Kidney function of one or more persons: Cockcroft-Gault, MDRD, CKD-EPI 2009 and 2021, BSA, ideal weight.",
            params: &[("persons?", "[{ age, weight, height, female, scr, black, weightType }]"), ("person?", "or one such person")],
            run: egfr,
        },
        Command { name: "renal.categories", summary: "The renal function categories and their limits.", params: &[], run: categories },
        Command {
            name: "renal.giusti",
            summary: "Giusti-Hayton dose adjustment from the fraction excreted unchanged.",
            params: &[
                ("fe", "fraction excreted unchanged"),
                ("crcl?", "creatinine clearance of the patient"),
                ("crclNormal?", "120"),
                ("dose?", "the usual dose"),
                ("tau?", "the usual dosing interval, hours"),
            ],
            run: giusti,
        },
        Command {
            name: "renal.groups",
            summary: "Compare exposure of impairment groups with a reference group on the log scale.",
            params: &[
                ("groups", "[{ name, values }]"),
                ("ref", "name of the reference group"),
                ("alpha?", "0.1"),
                ("pooled?", "true"),
                ("bounds?", "[0.8, 1.25] for the dose advice"),
            ],
            run: groups,
        },
        Command {
            name: "renal.regress",
            summary: "Regression of log exposure on kidney function with predictions.",
            params: &[("x", "list"), ("y", "list"), ("options?", "{ logX, ref, points, alpha }")],
            run: regress,
        },
        Command {
            name: "hepatic.childPugh",
            summary: "Child-Pugh score and class.",
            params: &[("input", "{ bilirubin, albumin, inr, ascites, encephalopathy }")],
            run: child_pugh_cmd,
        },
        Command {
            name: "hepatic.liver",
            summary: "Well-stirred liver model with changes in flow, intrinsic clearance and binding.",
            params: &[("input", "{ q, fu, bp, clint, clRenal, fa, fg }"), ("mult?", "{ q, clint, fu }")],
            run: liver,
        },
    ]
}

fn egfr(p: &Value, ctx: &Ctx) -> Result<Value> {
    let persons: Vec<Person> = match p.get("persons") {
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| DtbError::Usage(format!("persons: {e}")))?,
        None => vec![typed_req(p, "person")?],
    };
    ctx.limit("persons", persons.len(), ctx.config.limits.max_subjects, "max_subjects")?;
    let rows: Vec<Value> = persons
        .iter()
        .map(|q| {
            let b = bsa(q.height, q.weight, "dubois");
            let e21 = ckd_epi_2021(q);
            json!({
                "cockcroftGault": cockcroft_gault(q), "mdrd": mdrd(q), "ckdEpi2009": ckd_epi_2009(q), "ckdEpi2021": e21,
                "bsaDuBois": b, "bsaMosteller": bsa(q.height, q.weight, "mosteller"), "idealWeight": ideal_weight(q.height, q.female),
                "ckdEpi2021Denormalised": denormalise(e21, b), "category": renal_category(e21),
            })
        })
        .collect();
    Ok(json!({ "rows": rows }))
}

fn categories(_p: &Value, _ctx: &Ctx) -> Result<Value> {
    Ok(json!(renal_categories()))
}

fn giusti(p: &Value, _ctx: &Ctx) -> Result<Value> {
    let fe = num(p, "fe")?.ok_or_else(|| DtbError::Usage("The parameter \"fe\" is required.".into()))?;
    if !(0.0..=1.0).contains(&fe) {
        return Err(DtbError::Input("fe is a fraction between 0 and 1.".into()));
    }
    let normal = num(p, "crclNormal")?.unwrap_or(120.0);
    let mut out = json!({ "table": giusti_hayton_table(fe, normal, num(p, "dose")?, num(p, "tau")?) });
    if let Some(c) = num(p, "crcl")? {
        out["one"] = serde_json::to_value(giusti_hayton(fe, c, normal))?;
    }
    Ok(out)
}

fn groups(p: &Value, _ctx: &Ctx) -> Result<Value> {
    let g: Vec<GroupIn> = typed_req(p, "groups")?;
    let reference = text(p, "ref").ok_or_else(|| DtbError::Usage("The parameter \"ref\" is required.".into()))?;
    let alpha = num(p, "alpha")?.unwrap_or(0.1);
    let pooled = super::flag(p, "pooled", true);
    let bounds: [f64; 2] = typed::<Option<[f64; 2]>>(p, "bounds")?.unwrap_or([0.8, 1.25]);
    let r = compare_groups(&g, reference, alpha, pooled);
    let advice: Vec<Value> = r.rows.iter().map(|row| serde_json::to_value(dose_advice(row, bounds)).unwrap_or(Value::Null)).collect();
    let mut v = serde_json::to_value(&r)?;
    v["advice"] = json!(advice);
    Ok(v)
}

fn regress(p: &Value, _ctx: &Ctx) -> Result<Value> {
    let (x, y): (Vec<f64>, Vec<f64>) = (typed_req(p, "x")?, typed_req(p, "y")?);
    if x.len() != y.len() {
        return Err(DtbError::Input(format!("x has {} values and y has {}; they must pair up.", x.len(), y.len())));
    }
    let o: RegressOpts = typed(p, "options")?;
    Ok(serde_json::to_value(regress_covariate(&x, &y, &o).map_err(DtbError::Input)?)?)
}

fn child_pugh_cmd(p: &Value, _ctx: &Ctx) -> Result<Value> {
    let i: ChildPughIn = typed_req(p, "input")?;
    Ok(serde_json::to_value(child_pugh(&i))?)
}

fn liver(p: &Value, _ctx: &Ctx) -> Result<Value> {
    let i: LiverIn = typed(p, "input")?;
    let m: Mult = typed(p, "mult")?;
    Ok(serde_json::to_value(well_stirred(&i, &m))?)
}
