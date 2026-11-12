//! BE & DDI Studio: drug-drug interactions (static and mechanistic prediction, steady state, dynamic model).

use super::{map_err, num, text, typed, typed_req, Command, Ctx};
use crate::core::error::{DtbError, Result};
use crate::ddi;
use serde_json::{json, Value};

pub fn commands() -> Vec<Command> {
    vec![
        Command { name: "ddi.presets", summary: "The regulatory presets (FDA 2020, EMA 2012) and their constants.", params: &[], run: presets },
        Command {
            name: "ddi.predict",
            summary: "Static R-values per enzyme, the mechanistic AUC ratio and its classification.",
            params: &[
                ("preset?", "fda2020 | ema2012"),
                ("presetOverrides?", "constants to change"),
                ("conc", "{ doseMg, mw, cmax, fup, ka, faFg }"),
                ("enzymes", "[{ enzyme, ki, kinact, KI, kdeg, kdegGut, emax, ec50, fm, gut }]"),
                ("victim?", "{ fg, hepaticConc: imax|inlet, gut }"),
            ],
            run: predict,
        },
        Command {
            name: "ddi.steady",
            summary: "Steady-state peak, trough and average of repeated oral dosing.",
            params: &[("input", "{ doseMg, mw, cl, v, ka, f, tau }")],
            run: steady,
        },
        Command {
            name: "ddi.dynamic",
            summary: "Dynamic (time-course) interaction model, with and without the perpetrator.",
            params: &[("model", "the dynamic model object"), ("window?", "[from, to] hours for AUC"), ("every?", "keep every n-th point of the series (1)")],
            run: dynamic,
        },
    ]
}

fn presets(_p: &Value, _ctx: &Ctx) -> Result<Value> {
    Ok(
        json!({ "fda2020": ddi::preset("fda2020"), "ema2012": ddi::preset("ema2012"), "kdeg": ddi::KDEG.iter().map(|(n, k)| json!({"name": n, "kdeg": k})).collect::<Vec<_>>() }),
    )
}

fn predict(p: &Value, _ctx: &Ctx) -> Result<Value> {
    let id = text(p, "preset").unwrap_or("fda2020");
    let mut base = serde_json::to_value(ddi::preset(id).ok_or_else(|| DtbError::Usage(format!("Unknown preset \"{id}\" (use fda2020 or ema2012).")))?)?;
    if let (Some(o), Some(ov)) = (base.as_object_mut(), p.get("presetOverrides").and_then(Value::as_object)) {
        for (k, v) in ov {
            o.insert(k.clone(), v.clone());
        }
    }
    let preset: ddi::Preset = serde_json::from_value(base).map_err(|e| DtbError::Usage(format!("presetOverrides: {e}")))?;
    let cin: ddi::ConcInput = typed_req(p, "conc")?;
    if !(cin.dose_mg > 0.0 && cin.mw > 0.0 && cin.cmax > 0.0 && cin.fup > 0.0) {
        return Err(DtbError::Input("The dose, molecular weight, Cmax and unbound fraction must be positive numbers.".into()));
    }
    let enzymes: Vec<ddi::Enzyme> = typed_req(p, "enzymes")?;
    let victim: ddi::Victim = typed(p, "victim")?;
    let conc = ddi::concentrations(&cin, &preset);
    let stat: Vec<_> = enzymes.iter().map(|e| ddi::static_r(e, &conc, &preset)).collect();
    let mech = ddi::mechanistic_aucr(&enzymes, &conc, &victim);
    let class = ddi::classify_aucr(mech.aucr);
    Ok(json!({ "preset": preset, "conc": conc, "static": stat, "mechanistic": mech, "class": class }))
}

fn steady(p: &Value, _ctx: &Ctx) -> Result<Value> {
    let i: ddi::SteadyIn = typed_req(p, "input")?;
    if !(i.dose_mg > 0.0 && i.mw > 0.0 && i.cl > 0.0 && i.v > 0.0 && i.tau > 0.0) {
        return Err(DtbError::Input("The dose, molecular weight, clearance, volume and interval must be positive numbers.".into()));
    }
    Ok(serde_json::to_value(ddi::steady_state(&i))?)
}

fn dynamic(p: &Value, ctx: &Ctx) -> Result<Value> {
    let m: ddi::DynModel = typed_req(p, "model")?;
    let dt = m.dt.unwrap_or(0.05);
    if !(dt > 0.0) || !(m.t_end > 0.0) {
        return Err(DtbError::Input("The time step and the end time must be positive.".into()));
    }
    ctx.limit("time steps", (m.t_end / dt) as usize, 50_000_000, "max_points")?;
    let window: Option<[f64; 2]> = typed::<Option<[f64; 2]>>(p, "window")?;
    let c = ddi::compare_ddi(&m, window);
    let every = super::count(p, "every")?.unwrap_or(1).max(1);
    let mut v = serde_json::to_value(&c)?;
    if every > 1 {
        for part in ["control", "interaction"] {
            if let Some(o) = v[part].as_object_mut() {
                for (_, arr) in o.iter_mut() {
                    if let Some(a) = arr.as_array() {
                        *arr = Value::Array(a.iter().step_by(every).cloned().collect());
                    }
                }
            }
        }
    }
    let _ = (num(p, "x"), map_err::<()>(Ok(())));
    Ok(v)
}
