//! `info` and `doctor`: what this build is and whether it works.

use super::{all, Command, Ctx};
use crate::core::error::Result;
use crate::core::provenance::{ENGINE, PROTOCOL, VERSION};
use serde_json::{json, Value};

pub fn commands() -> Vec<Command> {
    vec![
        Command { name: "info", summary: "Version, protocol, build, commands, limits and the effective configuration.", params: &[], run: info },
        Command {
            name: "doctor",
            summary: "Self-test: known answers of the random generator, statistics, engines and the determinism of parallel runs.",
            params: &[],
            run: doctor,
        },
    ]
}

pub fn commands_json() -> Vec<Value> {
    all().iter().map(|c| json!({ "name": c.name, "summary": c.summary, "params": c.params.iter().map(|(n, d)| json!({"name": n.trim_end_matches('?'), "required": !n.ends_with('?'), "description": d})).collect::<Vec<_>>() })).collect()
}

fn info(_p: &Value, ctx: &Ctx) -> Result<Value> {
    Ok(json!({
        "name": "dtb", "version": VERSION, "protocol": PROTOCOL, "engine": ENGINE,
        "build": { "target": env!("DTB_TARGET"), "profile": env!("DTB_PROFILE"), "rustc": env!("DTB_RUSTC") },
        "platform": { "os": std::env::consts::OS, "arch": std::env::consts::ARCH, "threads": rayon::current_num_threads() },
        "commands": commands_json(),
        "limits": ctx.config.limits,
        "config": ctx.config,
    }))
}

struct Check {
    name: &'static str,
    ok: bool,
    detail: String,
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol * b.abs().max(1.0)
}

pub fn run_doctor() -> Vec<Value> {
    use crate::stats::{norm_quantile, t_quantile, Rng};
    let mut c: Vec<Check> = Vec::new();
    let mut push = |name: &'static str, ok: bool, detail: String| c.push(Check { name, ok, detail });

    // the random generator is the one of the browser engine: the same numbers for the same seed
    let mut r = Rng::new(12345);
    let got = [r.next(), r.next(), r.next()];
    let want = [0.12076167785562575, 0.10254896758124232, 0.985547199845314];
    push("rng known answers", got.iter().zip(want).all(|(a, b)| near(*a, b, 1e-15)), format!("seed 12345 → {got:?}"));
    push("normal quantile", near(norm_quantile(0.9), 1.2815515655446, 1e-10), format!("{}", norm_quantile(0.9)));
    push("t quantile", near(t_quantile(0.975, 10.0), 2.2281388519862753, 1e-10), format!("{}", t_quantile(0.975, 10.0)));

    // exact power of a 2x2 crossover (checked against the browser engine)
    {
        use crate::be::power::{power_abe, AbeArgs};
        let p = power_abe(&AbeArgs { cv: 0.3, theta0: 0.95, n: 24.0, ..Default::default() });
        push("exact power of ABE", (0.0..1.0).contains(&p) && p > 0.4, format!("{p:.6}"));
    }

    // parallel simulation gives identical results on one thread and on all
    {
        use crate::trial::{run_scenario, Design, RunOptions, Streams};
        let d = Design::default();
        let o = RunOptions { n_trials: 200, seed: 7, streams: Streams::Blocks, values: true, timing: false };
        let a = run_scenario(&d, &o).and_then(|r| serde_json::to_string(&r).map_err(|e| e.to_string()));
        let b = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .map_err(|e| e.to_string())
            .and_then(|pool| pool.install(|| run_scenario(&d, &o).and_then(|r| serde_json::to_string(&r).map_err(|e| e.to_string()))));
        match (a, b) {
            (Ok(x), Ok(y)) => push("trials are the same on any number of threads", x == y, format!("{} bytes", x.len())),
            (a, b) => push("trials are the same on any number of threads", false, format!("{:?} / {:?}", a.err(), b.err())),
        }
    }

    // the data engine
    {
        use crate::prep::expr::evaluate;
        use crate::prep::table::Frame;
        let t = Frame::from_text(&crate::core::parse_delimited("ID,WT\n1,70\n2,80\n", None).table);
        let v = evaluate(&t, "WT / 10 + 1", &[]).map(|e| e.values).unwrap_or_default();
        push("expressions", v == vec![crate::prep::expr::Val::Num(8.0), crate::prep::expr::Val::Num(9.0)], format!("{v:?}"));
    }

    // the example configuration is the default configuration
    {
        use crate::core::config::{Config, EXAMPLE};
        let ex = Config::parse(EXAMPLE, "example").map(|x| serde_json::to_value(x).ok());
        push("example configuration", ex.ok().flatten() == serde_json::to_value(Config::default()).ok(), "dtb config init writes it".into());
    }
    c.into_iter().map(|x| json!({ "name": x.name, "ok": x.ok, "detail": x.detail })).collect()
}

fn doctor(_p: &Value, _ctx: &Ctx) -> Result<Value> {
    let checks = run_doctor();
    let ok = checks.iter().all(|c| c["ok"] == true);
    Ok(json!({ "ok": ok, "checks": checks }))
}
