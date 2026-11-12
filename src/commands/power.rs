//! BE & DDI Studio: power and sample size for bioequivalence studies.

use super::{count, flag, map_err, num, streams, text, typed, typed_req, Command, Ctx};
use crate::be::power::{
    design_constants, design_presets, power_abe, power_sim, sample_size_abe, sample_size_sim, sample_size_table, with_dropout, AbeArgs, SimArgs,
};
use crate::core::error::{DtbError, Result};
use serde_json::{json, Value};

pub fn commands() -> Vec<Command> {
    vec![
        Command { name: "power.designs", summary: "The study designs (sequences) the power functions know.", params: &[], run: designs },
        Command {
            name: "power.abe",
            summary: "Exact power of the two one-sided tests (ABE) for any design.",
            params: &[("args", "{ cv, theta0, n, sequences | design, limits, alpha }")],
            run: abe,
        },
        Command {
            name: "power.size",
            summary: "Smallest sample size reaching the target power for ABE.",
            params: &[
                ("args", "as power.abe (n is ignored)"),
                ("target?", "0.8"),
                ("minN?", "12"),
                ("maxN?", "5000"),
                ("dropout?", "expected drop-out rate; adds nEnrol"),
            ],
            run: size,
        },
        Command {
            name: "power.sim",
            summary: "Power of ABEL, RSABE or NTID by simulating the whole decision.",
            params: &[
                ("args", "{ method, cvT, cvR, theta0, n, sequences, nSim, seed, limits, alpha, abel }"),
                ("streams?", "\"blocks\" (parallel, any thread count) or \"js\" (the browser's numbers)"),
            ],
            run: sim,
        },
        Command {
            name: "power.simSize",
            summary: "Smallest sample size reaching the target power by simulation.",
            params: &[
                ("args", "as power.sim"),
                ("target?", "0.8"),
                ("minN?", "smallest N tried (12)"),
                ("maxN?", "largest N tried (600)"),
                ("streams?", "\"blocks\" (parallel, any thread count) or \"js\" (the browser's numbers)"),
            ],
            run: sim_size,
        },
        Command {
            name: "power.plan",
            summary: "Everything the app's sample-size page shows in one call: size, power curve, enrolment with dropout, a check at one N and the table by CV and ratio.",
            params: &[
                ("method?", "abe (default), abel, rsabe or ntid"),
                ("sequences?", "list such as [\"TR\",\"RT\"]; null for parallel groups"),
                ("cvT", "within-subject CV of the test (the only CV for abe)"),
                ("cvR?", "within-subject CV of the reference (scaled methods; default cvT)"),
                ("theta0", "true ratio test / reference"),
                ("limits?", "[0.8, 1.25]"),
                ("alpha?", "one-sided level (0.05)"),
                ("target?", "target power (0.8)"),
                ("minN?", "smallest N (12)"),
                ("dropout?", "expected drop-out rate (0)"),
                ("nSim?", "simulated studies for the scaled methods (config power.sims)"),
                ("seed?", "random seed for the scaled methods (12345)"),
                ("nCheck?", "also give the power at this N (0 = skip)"),
                ("table?", "also give the table by CV and ratio (abe only; default true)"),
                ("streams?", "\"blocks\" (parallel, any thread count) or \"js\" (the browser's numbers)"),
            ],
            run: plan,
        },
        Command {
            name: "power.table",
            summary: "Sample sizes over a grid of CVs and true ratios (ABE).",
            params: &[
                ("cvs", "list of CVs"),
                ("ratios", "list of true ratios"),
                ("sequences?", "list or null for parallel"),
                ("limits?", "[0.8, 1.25]"),
                ("alpha?", "0.05"),
                ("target?", "0.8"),
                ("minN?", "12"),
            ],
            run: table,
        },
    ]
}

fn designs(_p: &Value, _ctx: &Ctx) -> Result<Value> {
    Ok(json!(design_presets()))
}

/// `args.design` ("2x2x2" …) may stand in for the list of sequences.
fn resolve(args: &mut Value) {
    if let Some(id) = args.get("design").and_then(Value::as_str).map(String::from) {
        if args.get("sequences").is_none() {
            args["sequences"] = match crate::be::power::preset_sequences(&id) {
                Some(s) => json!(s),
                None => Value::Null,
            };
        }
    }
}

fn abe_args(p: &Value) -> Result<AbeArgs> {
    let mut a = p.get("args").cloned().unwrap_or_else(|| json!({}));
    resolve(&mut a);
    let args: AbeArgs = serde_json::from_value(a).map_err(|e| DtbError::Usage(format!("The parameter \"args\" is not valid: {e}")))?;
    if !(args.cv > 0.0) || !(args.theta0 > 0.0) || !(args.alpha > 0.0 && args.alpha < 1.0) || !(args.limits[0] > 0.0 && args.limits[1] > args.limits[0]) {
        return Err(DtbError::Input("Check cv (> 0), theta0 (> 0), alpha (between 0 and 1) and the limits (0 < lower < upper).".into()));
    }
    Ok(args)
}

fn abe(p: &Value, _ctx: &Ctx) -> Result<Value> {
    let a = abe_args(p)?;
    if !(a.n >= 2.0) {
        return Err(DtbError::Input("n must be at least 2.".into()));
    }
    let dc = design_constants(a.sequences.as_deref());
    Ok(json!({ "power": power_abe(&a), "n": a.n, "df": dc.df(a.n), "parallel": dc.parallel }))
}

fn size(p: &Value, _ctx: &Ctx) -> Result<Value> {
    let a = abe_args(p)?;
    let (target, min_n, max_n) = (num(p, "target")?.unwrap_or(0.8), num(p, "minN")?.unwrap_or(12.0), num(p, "maxN")?.unwrap_or(5000.0));
    let s = sample_size_abe(&a, target, min_n, max_n);
    let n_seq = a.sequences.as_ref().map_or(2.0, |s| s.len() as f64);
    let mut out = serde_json::to_value(&s)?;
    if let Some(d) = num(p, "dropout")? {
        out["nEnrol"] = json!(with_dropout(s.n, d, n_seq));
    }
    Ok(out)
}

fn sim_args(p: &Value, ctx: &Ctx) -> Result<SimArgs> {
    let mut a = p.get("args").cloned().unwrap_or_else(|| json!({}));
    resolve(&mut a);
    if a.get("sequences").is_some_and(Value::is_null) {
        a.as_object_mut().map(|o| o.remove("sequences"));
    }
    let args: SimArgs = serde_json::from_value(a).map_err(|e| DtbError::Usage(format!("The parameter \"args\" is not valid: {e}")))?;
    ctx.limit("nSim", args.n_sim, ctx.config.limits.max_sims, "max_sims")?;
    if args.n_sim == 0 {
        return Err(DtbError::Usage("nSim must be at least 1.".into()));
    }
    Ok(args)
}

fn args_with_defaults(p: &Value, ctx: &Ctx) -> Result<SimArgs> {
    let mut q = p.clone();
    // the configured number of simulations and the call's own seed are defaults of `args`
    if let Some(args) = q.get_mut("args").and_then(Value::as_object_mut) {
        args.entry("nSim").or_insert_with(|| json!(ctx.config.power.sims));
        if let Some(s) = ctx.config.general.seed {
            args.entry("seed").or_insert_with(|| json!(s));
        }
    }
    sim_args(&q, ctx)
}

fn sim(p: &Value, ctx: &Ctx) -> Result<Value> {
    let a = args_with_defaults(p, ctx)?;
    let r = map_err(power_sim(&a, streams(p, &ctx.config.power.streams)?))?;
    let mut v = serde_json::to_value(&r)?;
    v["method"] = json!(a.method);
    v["seed"] = json!(a.seed);
    Ok(v)
}

fn sim_size(p: &Value, ctx: &Ctx) -> Result<Value> {
    let a = args_with_defaults(p, ctx)?;
    let (target, min_n, max_n) = (num(p, "target")?.unwrap_or(0.8), num(p, "minN")?.unwrap_or(12.0), num(p, "maxN")?.unwrap_or(600.0));
    Ok(serde_json::to_value(map_err(sample_size_sim(&a, target, min_n, max_n, streams(p, &ctx.config.power.streams)?))?)?)
}

fn table(p: &Value, ctx: &Ctx) -> Result<Value> {
    let cvs: Vec<f64> = typed_req(p, "cvs")?;
    let ratios: Vec<f64> = typed_req(p, "ratios")?;
    ctx.limit("grid cells", cvs.len().saturating_mul(ratios.len()), ctx.config.limits.max_points, "max_points")?;
    let seqs: Option<Vec<String>> = match p.get("sequences") {
        Some(Value::Null) | None => (text(p, "design") != Some("parallel")).then(|| vec!["TR".into(), "RT".into()]),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| DtbError::Usage(format!("sequences: {e}")))?,
    };
    let seqs = match (text(p, "design"), seqs) {
        (Some(id), _) => crate::be::power::preset_sequences(id),
        (None, s) => s,
    };
    let limits: [f64; 2] = typed::<Option<[f64; 2]>>(p, "limits")?.unwrap_or([0.8, 1.25]);
    let _ = count(p, "x");
    Ok(serde_json::to_value(sample_size_table(
        &cvs,
        &ratios,
        seqs.as_deref(),
        limits,
        num(p, "alpha")?.unwrap_or(0.05),
        num(p, "target")?.unwrap_or(0.8),
        num(p, "minN")?.unwrap_or(12.0),
    ))?)
}

/// The CVs and true ratios of the table on the app's sample-size page.
const TABLE_CVS: [f64; 8] = [0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.5];
const TABLE_RATIOS: [f64; 5] = [0.9, 0.925, 0.95, 0.975, 1.0];

/// The whole calculation of the sample-size page, step for step as the page does it, so that the results are the same.
fn plan(p: &Value, ctx: &Ctx) -> Result<Value> {
    use crate::core::js::round;
    use rayon::prelude::*;
    let method = text(p, "method").unwrap_or("abe").to_string();
    if !["abe", "abel", "rsabe", "ntid"].contains(&method.as_str()) {
        return Err(DtbError::Usage(format!("method must be abe, abel, rsabe or ntid, not \"{method}\".")));
    }
    let sequences: Option<Vec<String>> = typed::<Option<Vec<String>>>(p, "sequences")?;
    let need = |k: &str| num(p, k)?.ok_or_else(|| DtbError::Usage(format!("The parameter \"{k}\" is required.")));
    let (cv_t, theta0) = (need("cvT")?, need("theta0")?);
    let cv_r = num(p, "cvR")?.unwrap_or(cv_t);
    let limits: [f64; 2] = typed::<Option<[f64; 2]>>(p, "limits")?.unwrap_or([0.8, 1.25]);
    let alpha = num(p, "alpha")?.unwrap_or(0.05);
    let target = num(p, "target")?.unwrap_or(0.8);
    let min_n = num(p, "minN")?.unwrap_or(12.0);
    let dropout = num(p, "dropout")?.unwrap_or(0.0);
    let n_sim = count(p, "nSim")?.unwrap_or(ctx.config.power.sims);
    let seed = super::seed(p, ctx, 12345)?;
    let n_check = num(p, "nCheck")?.unwrap_or(0.0);
    let streams = streams(p, &ctx.config.power.streams)?;
    if !(cv_t > 0.0)
        || !(cv_r > 0.0)
        || !(theta0 > 0.0)
        || !(alpha > 0.0 && alpha < 1.0)
        || !(limits[0] > 0.0 && limits[1] > limits[0])
        || !(0.0..1.0).contains(&dropout)
        || !(target > 0.0 && target < 1.0)
    {
        return Err(DtbError::Input(
            "Check the CVs (> 0), the ratio (> 0), alpha and the target (between 0 and 1), the dropout (0 to < 1) and the limits (0 < lower < upper).".into(),
        ));
    }
    ctx.limit("nSim", n_sim, ctx.config.limits.max_sims, "max_sims")?;
    if n_sim == 0 {
        return Err(DtbError::Usage("nSim must be at least 1.".into()));
    }
    let n_seq = sequences.as_ref().map_or(2.0, |s| s.len() as f64);
    let abe_at = |n: f64| power_abe(&AbeArgs { cv: cv_t, theta0, n, sequences: sequences.clone(), limits, alpha });
    let (n, power_at, note, curve_x, curve_y);
    if method == "abe" {
        let r = sample_size_abe(&AbeArgs { cv: cv_t, theta0, n: 0.0, sequences: sequences.clone(), limits, alpha }, target, min_n, 5000.0);
        n = r.n;
        power_at = r.power;
        note = r.note.clone().unwrap_or_default();
        let hi = if n.is_finite() { round(n * 1.6) } else { 100.0 };
        let lo = (n_seq * 2.0).max((min_n / 2.0).floor());
        let ks: Vec<f64> = ((lo / n_seq).ceil() as i64..).map(|k| k as f64).take_while(|k| k * n_seq <= hi).collect();
        ctx.limit("curve points", ks.len(), ctx.config.limits.max_points, "max_points")?;
        curve_x = ks.iter().map(|k| k * n_seq).collect::<Vec<f64>>();
        curve_y = curve_x.par_iter().map(|&m| abe_at(m)).collect::<Vec<f64>>();
    } else {
        let seqs = sequences.clone().ok_or_else(|| DtbError::Input("Scaled methods need a replicate crossover design.".into()))?;
        let a = SimArgs { method: method.clone(), cv_t, cv_r, theta0, n: 0.0, sequences: seqs, n_sim, seed, limits, alpha, ..Default::default() };
        let r = map_err(sample_size_sim(&a, target, min_n, 600.0, streams))?;
        n = r.n;
        power_at = r.power;
        note = r.note.clone().unwrap_or_default();
        let hi = if n.is_finite() { round(n * 1.5) } else { 100.0 };
        let half = (n * 0.5).floor();
        let lo = (n_seq * 3.0).max(if half.is_nan() || half == 0.0 { 12.0 } else { half });
        let step = n_seq.max(round((hi - lo) / 9.0 / n_seq) * n_seq);
        let mut ms = vec![];
        let mut m = (lo / n_seq).ceil() * n_seq;
        while m <= hi {
            ms.push(m);
            m += step;
        }
        let mut ys = vec![];
        for &m in &ms {
            ys.push(map_err(power_sim(&SimArgs { n: m, n_sim: n_sim.min(4000), ..a.clone() }, streams))?.power);
        }
        curve_x = ms;
        curve_y = ys;
    }
    let enrol = if n.is_finite() { with_dropout(n, dropout, n_seq) } else { f64::NAN };
    let check = if n_check > 0.0 {
        let pw = if method == "abe" {
            abe_at(n_check)
        } else {
            let seqs = sequences.clone().unwrap_or_default();
            map_err(power_sim(
                &SimArgs { method: method.clone(), cv_t, cv_r, theta0, n: n_check, sequences: seqs, n_sim, seed, limits, alpha, ..Default::default() },
                streams,
            ))?
            .power
        };
        json!({ "n": n_check, "power": pw })
    } else {
        Value::Null
    };
    let dc = design_constants(sequences.as_deref());
    let table = if method == "abe" && flag(p, "table", true) {
        json!({ "cvs": TABLE_CVS, "rows": sample_size_table(&TABLE_CVS, &TABLE_RATIOS, sequences.as_deref(), limits, alpha, target, min_n) })
    } else {
        Value::Null
    };
    Ok(
        json!({ "n": n, "power": power_at, "note": note, "curve": { "x": curve_x, "y": curve_y }, "enrol": enrol, "check": check, "df": dc.df(n), "table": table }),
    )
}
