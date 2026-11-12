//! Scenarios (named changes of a design), one-at-a-time sensitivity, concentration bands and the exposure-response curve.

use super::design::{apply_overrides, Design, Kind, Overrides};
use super::pk::{exposure, hill, profile, Subject};
use super::sim::{adherence_beta, run_scenario, RunOptions, ScenarioResult};
use crate::stats::{quantile_sorted, sorted, Rng};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Scenario {
    pub name: String,
    pub overrides: Overrides,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScenarioRun {
    pub name: String,
    pub overrides: Overrides,
    pub design: Design,
    pub result: ScenarioResult,
}

/// Run named scenarios (overrides of the base design). All use the same seed, so differences between them are not noise.
pub fn run_scenarios(base: &Design, scenarios: &[Scenario], o: &RunOptions) -> Result<Vec<ScenarioRun>, String> {
    scenarios
        .par_iter()
        .map(|sc| {
            let design = apply_overrides(base, &sc.overrides);
            let result = run_scenario(&design, o)?;
            Ok(ScenarioRun { name: sc.name.clone(), overrides: sc.overrides.clone(), design, result })
        })
        .collect()
}

#[derive(Clone, Debug, Serialize)]
pub struct TornadoItem {
    pub label: String,
    pub low: f64,
    pub high: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Tornado {
    pub base: f64,
    pub items: Vec<TornadoItem>,
}

/// One-at-a-time sensitivity: P(Go) of one active arm when each assumption is moved down and up.
pub fn tornado(base: &Design, arm_id: &str, o: &RunOptions) -> Result<Tornado, String> {
    let ov = |f: &dyn Fn(&mut Overrides)| {
        let mut x = Overrides::default();
        f(&mut x);
        x
    };
    let a0 = base.arms.first().map(|a| a.n).unwrap_or(50.0);
    let mut items: Vec<(String, Overrides, Overrides)> = vec![
        ("EC50 (×2 / ×0.5)".into(), ov(&|x| x.ec50_scale = Some(2.0)), ov(&|x| x.ec50_scale = Some(0.5))),
        ("Maximum effect (×0.7 / ×1.3)".into(), ov(&|x| x.effect_scale = Some(0.7)), ov(&|x| x.effect_scale = Some(1.3))),
        (
            "Dropout rate (×2 / ×0.5)".into(),
            ov(&|x| x.dropout = Some(base.dropout.rate_per_week * 2.0)),
            ov(&|x| x.dropout = Some(base.dropout.rate_per_week * 0.5)),
        ),
        (
            "Adherence (−10 points / +10)".into(),
            ov(&|x| x.adherence = Some((base.adherence.mean - 0.1).max(0.3))),
            ov(&|x| x.adherence = Some((base.adherence.mean + 0.1).min(1.0))),
        ),
        ("Subjects per arm (−25% / +25%)".into(), ov(&|x| x.n = Some(((a0 * 0.75 + 0.5).floor()).max(2.0))), ov(&|x| x.n = Some((a0 * 1.25 + 0.5).floor()))),
        ("Clearance (×1.3 / ×0.7)".into(), ov(&|x| x.cl_scale = Some(1.3)), ov(&|x| x.cl_scale = Some(0.7))),
    ];
    if base.kind()? == Kind::Continuous {
        items.push(("Standard deviation (×1.2 / ×0.8)".into(), ov(&|x| x.sd_scale = Some(1.2)), ov(&|x| x.sd_scale = Some(0.8))));
    }
    let mut runs = vec![Scenario { name: "base".into(), overrides: Overrides::default() }];
    for (label, lo, hi) in &items {
        runs.push(Scenario { name: format!("{label} lo"), overrides: lo.clone() });
        runs.push(Scenario { name: format!("{label} hi"), overrides: hi.clone() });
    }
    let opt = RunOptions { values: false, ..o.clone() };
    let res = run_scenarios(base, &runs, &opt)?;
    let pick = |r: &ScenarioResult| r.arms.iter().find(|a| a.id == arm_id).or(r.arms.first()).map(|a| a.p_go).unwrap_or(f64::NAN);
    let base0 = pick(&res[0].result);
    let mut out: Vec<TornadoItem> = items
        .iter()
        .enumerate()
        .map(|(i, (label, _, _))| TornadoItem { label: label.clone(), low: pick(&res[1 + 2 * i].result), high: pick(&res[2 + 2 * i].result) })
        .collect();
    out.sort_by(|a, b| (b.high - b.low).abs().total_cmp(&(a.high - a.low).abs()));
    Ok(Tornado { base: base0, items: out })
}

#[derive(Clone, Debug, Serialize)]
pub struct Band {
    pub id: String,
    pub name: String,
    pub p05: Vec<f64>,
    pub p50: Vec<f64>,
    pub p95: Vec<f64>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Bands {
    pub times: Vec<f64>,
    pub arms: Vec<Band>,
}

fn sdlog(cv: f64) -> f64 {
    (1.0 + cv * cv).ln().sqrt()
}

/// Percentile bands of the concentration-time profile of each arm (subjects with their own PK and adherence).
pub fn pk_bands(d: &Design, from: f64, to: f64, points: usize, n_sub: usize, seed: u64) -> Bands {
    let mut rng = Rng::new(seed);
    let tau = d.tau;
    let n = d.n_doses().max(0) as usize;
    let iv = d.pk.model == "iv";
    let times: Vec<f64> = (0..points).map(|i| from + (to - from) * i as f64 / (points as f64 - 1.0)).collect();
    let beta = adherence_beta(d.adherence.mean, d.adherence.sd);
    let arms = d
        .arms
        .iter()
        .filter(|a| a.dose > 0.0)
        .map(|arm| {
            let mut prof: Vec<Vec<f64>> = Vec::with_capacity(n_sub);
            for _ in 0..n_sub {
                let adh = match beta {
                    Some((a, b)) => rng.beta(a, b),
                    None => d.adherence.mean,
                };
                let taken: Vec<u8> = (0..n).map(|_| if rng.next() < adh { 1 } else { 0 }).collect();
                let cl = d.pk.cl * (sdlog(d.pk.cv_cl) * rng.std_normal()).exp();
                let v = d.pk.v * (sdlog(d.pk.cv_v) * rng.std_normal()).exp();
                let ka = d.pk.ka * (sdlog(d.pk.cv_ka) * rng.std_normal()).exp();
                prof.push(profile(&Subject { dose: arm.dose, f: d.pk.f, cl, v, ka, tau, n, taken: Some(&taken), iv, conc_factor: d.pk.conc_factor }, &times));
            }
            let cols: Vec<Vec<f64>> = (0..points).map(|j| sorted(&prof.iter().map(|p| p[j]).collect::<Vec<_>>())).collect();
            Band {
                id: arm.id.clone(),
                name: arm.name.clone(),
                p05: cols.iter().map(|c| quantile_sorted(c, 0.05)).collect(),
                p50: cols.iter().map(|c| quantile_sorted(c, 0.5)).collect(),
                p95: cols.iter().map(|c| quantile_sorted(c, 0.95)).collect(),
            }
        })
        .collect();
    Bands { times, arms }
}

#[derive(Clone, Debug, Serialize)]
pub struct Mark {
    pub id: String,
    pub name: String,
    pub dose: f64,
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErCurve {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub marks: Vec<Mark>,
    pub y_label: String,
}

/// The exposure-response curve (drug effect against the exposure metric) and the typical exposure of each arm.
pub fn er_curve(d: &Design, points: usize) -> Result<ErCurve, String> {
    let kind = d.kind()?;
    let er = d.er_view();
    let n = d.n_doses().max(0) as usize;
    let metric = d.endpoint.metric.as_str();
    let typical = |dose: f64| {
        exposure(&Subject {
            dose: dose * d.adherence.mean,
            f: d.pk.f,
            cl: d.pk.cl,
            v: d.pk.v,
            ka: d.pk.ka,
            tau: d.tau,
            n,
            taken: None,
            iv: d.pk.model == "iv",
            conc_factor: d.pk.conc_factor,
        })
        .get(metric)
    };
    let marks: Vec<(String, String, f64, f64)> =
        d.arms.iter().map(|a| (a.id.clone(), a.name.clone(), a.dose, if a.dose > 0.0 { typical(a.dose) } else { 0.0 })).collect();
    let xmax = marks.iter().map(|m| m.3 * 1.15).fold(er.ec50 * 4.0, f64::max);
    let xs: Vec<f64> = (0..points).map(|i| xmax * i as f64 / (points as f64 - 1.0)).collect();
    let eff = |x: f64| {
        let f = hill(x, er.ec50, er.hill);
        match kind {
            Kind::Continuous => er.placebo + er.emax * f,
            Kind::Binary => 1.0 / (1.0 + (-((er.baseline / (1.0 - er.baseline)).ln() + er.emax * f)).exp()),
            Kind::Tte => 1.0 - er.imax * f,
        }
    };
    let y_label = match kind {
        Kind::Continuous => format!(
            "{}{}",
            if d.endpoint.label.is_empty() { "Change" } else { &d.endpoint.label },
            if d.endpoint.unit.is_empty() { String::new() } else { format!(" ({})", d.endpoint.unit) }
        ),
        Kind::Binary => "Response probability".into(),
        Kind::Tte => "Hazard relative to control".into(),
    };
    Ok(ErCurve {
        y: xs.iter().map(|&x| eff(x)).collect(),
        marks: marks.into_iter().map(|(id, name, dose, x)| Mark { id, name, dose, x, y: eff(x) }).collect(),
        x: xs,
        y_label,
    })
}
