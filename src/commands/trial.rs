//! Trial Simulator: validate a design, simulate trials, scenarios, sensitivity, exposure bands and the exposure-response curve.

use super::{count, flag, map_err, num, req, seed, streams, typed, typed_req, Command, Ctx};
use crate::core::error::{DtbError, Result};
use crate::trial::{apply_overrides, er_curve, pk_bands, run_scenario, run_scenarios, tornado, validate_design, Design, RunOptions, Scenario};
use serde_json::{json, Value};

pub fn commands() -> Vec<Command> {
    vec![
        Command {
            name: "trial.validate",
            summary: "Check a trial design; list errors and warnings in plain words.",
            params: &[("design", "the design object of the Trial Simulator")],
            run: validate,
        },
        Command {
            name: "trial.run",
            summary: "Simulate many trials of one design: power, decisions, effect distribution.",
            params: &[
                ("design", "the design object"),
                ("trials?", "number of simulated trials (default trial.trials)"),
                ("seed?", "random seed (default design.seed)"),
                ("streams?", "\"blocks\" (parallel, default) or \"js\" (same numbers as the browser)"),
                ("values?", "keep per-trial values (default true)"),
            ],
            run,
        },
        Command {
            name: "trial.scenarios",
            summary: "Run named scenarios (overrides of the base design) with a shared seed.",
            params: &[
                ("design", "the base design"),
                ("scenarios", "[{ name, overrides }]"),
                ("trials?", "simulated trials (config trial.trials)"),
                ("seed?", "random seed"),
                ("streams?", "\"blocks\" (parallel, any thread count) or \"js\" (the browser's numbers)"),
                ("values?", "true to include every trial's values"),
            ],
            run: scenarios,
        },
        Command {
            name: "trial.tornado",
            summary: "One-at-a-time sensitivity of the probability of Go for one arm.",
            params: &[
                ("design", "the trial design (arms, endpoint, PK/PD, decision rule)"),
                ("armId", "id of the arm"),
                ("trials?", "simulated trials (config trial.trials)"),
                ("seed?", "random seed"),
                ("streams?", "\"blocks\" (parallel, any thread count) or \"js\" (the browser's numbers)"),
            ],
            run: tornado_cmd,
        },
        Command {
            name: "trial.bands",
            summary: "Percentile bands of the concentration-time profile of each arm.",
            params: &[
                ("design", "the trial design (arms, endpoint, PK/PD, decision rule)"),
                ("from?", "hours (0)"),
                ("to?", "hours (168)"),
                ("points?", "140"),
                ("nSub?", "subjects per arm (150)"),
                ("seed?", "7"),
            ],
            run: bands,
        },
        Command {
            name: "trial.erCurve",
            summary: "The exposure-response curve of the design with the arms marked on it.",
            params: &[("design", "the trial design (arms, endpoint, PK/PD, decision rule)"), ("points?", "120")],
            run: curve,
        },
    ]
}

fn design(p: &Value) -> Result<Design> {
    typed_req(p, "design")
}

fn validate(p: &Value, _ctx: &Ctx) -> Result<Value> {
    let v = validate_design(&design(p)?);
    Ok(json!({ "ok": v.errors.is_empty(), "errors": v.errors, "warnings": v.warnings }))
}

fn options(p: &Value, ctx: &Ctx, d: &Design, trials_default: usize) -> Result<RunOptions> {
    let n = count(p, "trials")?.unwrap_or(trials_default);
    if n == 0 {
        return Err(DtbError::Usage("trials must be at least 1.".into()));
    }
    ctx.limit("trials", n, ctx.config.limits.max_trials, "max_trials")?;
    Ok(RunOptions {
        n_trials: n,
        seed: seed(p, ctx, d.seed)?,
        streams: streams(p, &ctx.config.trial.streams)?,
        values: flag(p, "values", true),
        timing: ctx.timing,
    })
}

fn run(p: &Value, ctx: &Ctx) -> Result<Value> {
    let d = design(p)?;
    let o = options(p, ctx, &d, ctx.config.trial.trials)?;
    Ok(serde_json::to_value(map_err(run_scenario(&d, &o))?)?)
}

fn scenarios(p: &Value, ctx: &Ctx) -> Result<Value> {
    let d = design(p)?;
    let sc: Vec<Scenario> = typed(p, "scenarios")?;
    req(p, "scenarios")?;
    ctx.limit("scenarios", sc.len(), ctx.config.limits.max_scenarios, "max_scenarios")?;
    let o = options(p, ctx, &d, ctx.config.trial.trials)?;
    ctx.limit("trials x scenarios", o.n_trials.saturating_mul(sc.len().max(1)), ctx.config.limits.max_trials.saturating_mul(4), "max_trials")?;
    let _ = apply_overrides; // overrides are applied inside run_scenarios
    Ok(serde_json::to_value(map_err(run_scenarios(&d, &sc, &o))?)?)
}

fn tornado_cmd(p: &Value, ctx: &Ctx) -> Result<Value> {
    let d = design(p)?;
    let arm = super::text(p, "armId").ok_or_else(|| DtbError::Usage("The parameter \"armId\" is required.".into()))?;
    let mut o = options(p, ctx, &d, (ctx.config.trial.trials / 3).max(1))?;
    o.values = false;
    Ok(serde_json::to_value(map_err(tornado(&d, arm, &o))?)?)
}

fn bands(p: &Value, ctx: &Ctx) -> Result<Value> {
    let d = design(p)?;
    let points = count(p, "points")?.unwrap_or(140);
    let n_sub = count(p, "nSub")?.unwrap_or(150);
    ctx.limit("points", points, ctx.config.limits.max_points, "max_points")?;
    ctx.limit("nSub", n_sub, ctx.config.limits.max_subjects, "max_subjects")?;
    let (from, to) = (num(p, "from")?.unwrap_or(0.0), num(p, "to")?.unwrap_or(168.0));
    if !(to > from) {
        return Err(DtbError::Usage("to must be larger than from.".into()));
    }
    let v = validate_design(&d);
    if let Some(e) = v.errors.first() {
        return Err(DtbError::Input(e.clone()));
    }
    Ok(serde_json::to_value(pk_bands(&d, from, to, points.max(2), n_sub.max(1), seed(p, ctx, 7)?))?)
}

fn curve(p: &Value, ctx: &Ctx) -> Result<Value> {
    let d = design(p)?;
    let points = count(p, "points")?.unwrap_or(120);
    ctx.limit("points", points, ctx.config.limits.max_points, "max_points")?;
    Ok(serde_json::to_value(map_err(er_curve(&d, points.max(2)))?)?)
}
