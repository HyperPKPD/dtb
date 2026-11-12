//! One simulated trial (subjects, pharmacokinetics, adherence, dropout, exposure-response, endpoint, analysis) and many of them.

use super::analysis::{log_rank, prop_diff, welch, Analysis, Moments};
use super::decision::{decide, Call, Decision};
use super::design::{validate_design, Arm, Design, ErView, Kind};
use super::pk::{exposure, hill, Subject};
use crate::stats::{mean, quantile_sorted, sorted, Rng};
use rayon::prelude::*;
use serde::Serialize;

fn sdlog(cv: f64) -> f64 {
    (1.0 + cv * cv).ln().sqrt()
}
fn expit(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}
fn logit(p: f64) -> f64 {
    (p / (1.0 - p)).ln()
}

/// Beta parameters of the per-subject adherence from a mean and a standard deviation (None: everyone takes the same share).
pub fn adherence_beta(mean: f64, sd: f64) -> Option<(f64, f64)> {
    if mean >= 1.0 || !(sd > 0.0) {
        return None;
    }
    let v = (sd * sd).min(0.98 * mean * (1.0 - mean));
    let k = mean * (1.0 - mean) / v - 1.0;
    Some((mean * k, (1.0 - mean) * k))
}

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub cl_scale: f64,
    pub ec50: f64,
    pub eff: f64,
}

#[derive(Default)]
struct ArmOut {
    n: f64,
    dropped: f64,
    exp_sum: f64,
    adh_sum: f64,
    mean: f64,
    v: f64,
    x: f64,
    t: Vec<f64>,
    e: Vec<u8>,
    events: Option<f64>,
    exp_mean: f64,
    adh_mean: f64,
}

pub struct Trial<'a> {
    pub d: &'a Design,
    kind: Kind,
    er: ErView,
    pub n_doses: usize,
    iv: bool,
    control: &'a Arm,
    pub actives: Vec<&'a Arm>,
    max_dose: f64,
    beta: Option<(f64, f64)>,
    s_cl: f64,
    s_v: f64,
    s_ka: f64,
    sign: f64,
    dur: f64,
}

#[derive(Clone, Debug)]
pub struct ArmRun {
    pub an: Analysis,
    pub truth: f64,
    pub dec: Decision,
    pub dropout: f64,
    pub exposure: f64,
    pub adherence: f64,
    pub events: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct TrialRun {
    pub control_dropout: f64,
    pub control_events: Option<f64>,
    pub arms: Vec<ArmRun>,
}

impl<'a> Trial<'a> {
    pub fn new(d: &'a Design) -> Result<Self, String> {
        let kind = d.kind()?;
        let control = d.arms.iter().find(|a| a.control).ok_or("Mark exactly one arm as the control arm.")?;
        let actives: Vec<&Arm> = d.arms.iter().filter(|a| !a.control).collect();
        let max_dose = d.arms.iter().map(|a| a.dose).fold(1e-12, f64::max);
        Ok(Trial {
            d,
            kind,
            er: d.er_view(),
            n_doses: d.n_doses().max(0) as usize,
            iv: d.pk.model == "iv",
            control,
            actives,
            max_dose,
            beta: adherence_beta(d.adherence.mean, d.adherence.sd),
            s_cl: sdlog(d.pk.cv_cl),
            s_v: sdlog(d.pk.cv_v),
            s_ka: sdlog(d.pk.cv_ka),
            sign: if d.endpoint.benefit == "lower" { -1.0 } else { 1.0 },
            dur: d.duration_weeks,
        })
    }

    /// The trial-level parameters (the assumed values, or a draw around them when uncertainty is on).
    pub fn draw(&self, rng: &mut Rng) -> Params {
        let u = &self.d.uncertainty;
        let base_eff = if self.kind == Kind::Tte { self.er.imax } else { self.er.emax };
        if !u.enabled {
            return Params { cl_scale: 1.0, ec50: self.er.ec50, eff: base_eff };
        }
        let mut eff = base_eff * (1.0 + u.rel_sd_emax * rng.std_normal());
        if self.kind == Kind::Tte {
            eff = eff.max(0.0).min(0.99);
        }
        let cl_scale = (u.sd_log_c_l * rng.std_normal()).exp();
        let ec50 = self.er.ec50 * (u.sd_log_e_c50 * rng.std_normal()).exp();
        Params { cl_scale, ec50, eff }
    }

    fn typical_exposure(&self, dose: f64, scale: f64) -> f64 {
        let s = Subject {
            dose: dose * self.d.adherence.mean,
            f: self.d.pk.f,
            cl: self.d.pk.cl * scale,
            v: self.d.pk.v,
            ka: self.d.pk.ka,
            tau: self.d.tau,
            n: self.n_doses,
            taken: None,
            iv: self.iv,
            conc_factor: self.d.pk.conc_factor,
        };
        exposure(&s).get(&self.d.endpoint.metric)
    }

    /// The population effect of an arm under the parameters, on the benefit scale: the "true" effect of this trial.
    pub fn true_effect(&self, arm: &Arm, p: &Params) -> f64 {
        let e = if arm.dose > 0.0 { self.typical_exposure(arm.dose, p.cl_scale) } else { 0.0 };
        let f = hill(e, p.ec50, self.er.hill);
        match self.kind {
            Kind::Continuous => self.sign * p.eff * f,
            Kind::Binary => expit(logit(self.er.baseline) + p.eff * f) - self.er.baseline,
            Kind::Tte => -(1.0 - p.eff * f).ln(),
        }
    }

    fn sim_arm(&self, arm: &Arm, p: &Params, rng: &mut Rng, taken: &mut [u8]) -> ArmOut {
        let n = arm.n as usize;
        let mut out = ArmOut::default();
        let h_drop = self.d.dropout.rate_per_week * (1.0 + self.d.dropout.dose_slope * (arm.dose / self.max_dose));
        let completers = self.d.dropout_handling == "completers";
        let (mut sum, mut sumsq, mut x) = (0.0f64, 0.0f64, 0.0f64);
        let mut t = Vec::new();
        let mut e = Vec::new();
        if self.kind == Kind::Tte {
            t = vec![0.0; n];
            e = vec![0u8; n];
        }
        let l0 = if self.kind == Kind::Binary { logit(self.er.baseline) } else { 0.0 };
        let metric = self.d.endpoint.metric.as_str();
        for i in 0..n {
            let t_drop = if h_drop > 0.0 { rng.exp(h_drop) } else { f64::INFINITY };
            let dropped = t_drop < self.dur;
            let mut f = 0.0;
            let mut adh = 1.0;
            if arm.dose > 0.0 {
                adh = match self.beta {
                    Some((a, b)) => rng.beta(a, b),
                    None => self.d.adherence.mean,
                };
                for k in 0..self.n_doses {
                    taken[k] = if rng.next() < adh { 1 } else { 0 };
                }
                let cl = self.d.pk.cl * p.cl_scale * (self.s_cl * rng.std_normal()).exp();
                let v = self.d.pk.v * (self.s_v * rng.std_normal()).exp();
                let ka = self.d.pk.ka * (self.s_ka * rng.std_normal()).exp();
                let ex = exposure(&Subject {
                    dose: arm.dose,
                    f: self.d.pk.f,
                    cl,
                    v,
                    ka,
                    tau: self.d.tau,
                    n: self.n_doses,
                    taken: Some(&taken[..self.n_doses]),
                    iv: self.iv,
                    conc_factor: self.d.pk.conc_factor,
                });
                let m = ex.get(metric);
                out.exp_sum += m;
                f = hill(m, p.ec50, self.er.hill);
            }
            out.adh_sum += adh;
            if dropped {
                out.dropped += 1.0;
            }
            match self.kind {
                Kind::Continuous => {
                    let z = rng.std_normal(); // always drawn, so scenarios share random numbers whatever the handling of dropouts
                    if dropped && completers {
                        continue;
                    }
                    let y = if dropped { 0.0 } else { self.er.placebo + p.eff * f + self.er.sd * z };
                    sum += y;
                    sumsq += y * y;
                    out.n += 1.0;
                }
                Kind::Binary => {
                    let u = rng.next();
                    if dropped && completers {
                        continue;
                    }
                    x += if dropped {
                        0.0
                    } else if u < expit(l0 + p.eff * f) {
                        1.0
                    } else {
                        0.0
                    };
                    out.n += 1.0;
                }
                Kind::Tte => {
                    let t_ev = rng.exp(self.er.hazard0 * (1.0 - p.eff * f));
                    let tc = t_drop.min(self.dur);
                    let ev = t_ev <= tc;
                    t[i] = if ev { t_ev } else { tc };
                    e[i] = ev as u8;
                    out.n += 1.0;
                }
            }
        }
        if self.kind == Kind::Continuous {
            let m = if out.n > 0.0 { sum / out.n } else { f64::NAN };
            out.mean = m;
            out.v = if out.n > 1.0 { (sumsq - out.n * m * m) / (out.n - 1.0) } else { f64::NAN };
        }
        if self.kind == Kind::Binary {
            out.x = x;
        }
        if self.kind == Kind::Tte {
            out.events = Some(e.iter().map(|&v| v as f64).sum());
            out.t = t;
            out.e = e;
        }
        out.exp_mean = if arm.dose > 0.0 { out.exp_sum / arm.n } else { 0.0 };
        out.adh_mean = out.adh_sum / arm.n;
        out
    }

    fn analyse(&self, c: &ArmOut, a: &ArmOut) -> Analysis {
        let nan = |df: f64| Analysis { delta: f64::NAN, se: f64::NAN, df, p: 1.0, events: None };
        match self.kind {
            Kind::Continuous => {
                if c.n >= 2.0 && a.n >= 2.0 {
                    welch(&Moments { n: c.n, mean: c.mean, v: c.v }, &Moments { n: a.n, mean: a.mean, v: a.v }, self.sign)
                } else {
                    nan(f64::NAN)
                }
            }
            Kind::Binary => {
                if c.n >= 1.0 && a.n >= 1.0 {
                    prop_diff(c.x, c.n, a.x, a.n)
                } else {
                    nan(f64::INFINITY)
                }
            }
            Kind::Tte => log_rank(&c.t, &c.e, &a.t, &a.e).0,
        }
    }

    /// One simulated trial.
    pub fn run(&self, rng: &mut Rng) -> TrialRun {
        let mut taken = vec![0u8; self.n_doses];
        let p = self.draw(rng);
        let c = self.sim_arm(self.control, &p, rng, &mut taken);
        let arms = self
            .actives
            .iter()
            .map(|arm| {
                let a = self.sim_arm(arm, &p, rng, &mut taken);
                let an = self.analyse(&c, &a);
                let dec = decide(self.d, &an);
                ArmRun { an, truth: self.true_effect(arm, &p), dec, dropout: a.dropped / arm.n, exposure: a.exp_mean, adherence: a.adh_mean, events: a.events }
            })
            .collect();
        TrialRun { control_dropout: c.dropped / self.control.n, control_events: c.events, arms }
    }
}

/// How the random numbers are organised.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Streams {
    /// Trials are simulated in blocks, each with its own stream derived from the seed: the result does not depend on the number of
    /// threads, and trials run in parallel.
    #[default]
    Blocks,
    /// One sequential stream, exactly as the browser engine draws it: the same seed gives the same numbers as the app (one thread).
    Js,
}

#[derive(Clone, Debug)]
pub struct RunOptions {
    pub n_trials: usize,
    pub seed: u64,
    pub streams: Streams,
    /// Keep the per-trial values in the result.
    pub values: bool,
    /// Report the elapsed time (off for byte-identical output).
    pub timing: bool,
}
impl Default for RunOptions {
    fn default() -> Self {
        RunOptions { n_trials: 1000, seed: 1, streams: Streams::Blocks, values: true, timing: true }
    }
}

const BLOCK: usize = 64;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mcse {
    pub power: f64,
    pub p_go: f64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeltaSummary {
    pub mean: f64,
    pub q05: f64,
    pub q25: f64,
    pub q50: f64,
    pub q75: f64,
    pub q95: f64,
    pub values: Vec<f64>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TruthSummary {
    pub mean: f64,
    pub values: Vec<f64>,
    pub p_above: Option<f64>,
    pub target: f64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArmResult {
    pub id: String,
    pub name: String,
    pub dose: f64,
    pub n: f64,
    pub power: f64,
    pub p_go: f64,
    pub p_consider: f64,
    pub p_no_go: f64,
    pub mcse: Mcse,
    pub mean_p_go: Option<f64>,
    pub mean_p_target: Option<f64>,
    pub delta: DeltaSummary,
    pub truth: TruthSummary,
    pub dropout: f64,
    pub exposure: f64,
    pub adherence: f64,
    pub events: Option<f64>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioResult {
    pub n_trials: usize,
    pub seed: u64,
    pub uncertainty: bool,
    pub total_n: f64,
    pub arms: Vec<ArmResult>,
    pub any_go: f64,
    pub any_sig: f64,
    pub control_dropout: f64,
    pub control_events: Option<f64>,
    pub seconds: f64,
    pub n_doses: usize,
    pub engine: &'static str,
    pub streams: Streams,
}

/// Simulate `n_trials` trials of a design and aggregate them.
pub fn run_scenario(design: &Design, o: &RunOptions) -> Result<ScenarioResult, String> {
    let v = validate_design(design);
    if let Some(m) = v.errors.first() {
        return Err(m.clone());
    }
    let n_trials = o.n_trials.max(1);
    let trial = Trial::new(design)?;
    let t0 = std::time::Instant::now();
    let runs: Vec<TrialRun> = match o.streams {
        Streams::Js => {
            let mut rng = Rng::new(o.seed);
            (0..n_trials).map(|_| trial.run(&mut rng)).collect()
        }
        Streams::Blocks => {
            let blocks = n_trials.div_ceil(BLOCK);
            (0..blocks)
                .into_par_iter()
                .map(|b| {
                    let mut rng = Rng::for_chunk(o.seed, b as u64);
                    let lo = b * BLOCK;
                    let hi = (lo + BLOCK).min(n_trials);
                    (lo..hi).map(|_| trial.run(&mut rng)).collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
                .into_iter()
                .flatten()
                .collect()
        }
    };
    let seconds = if o.timing { t0.elapsed().as_secs_f64() } else { 0.0 };
    Ok(aggregate(design, &trial, &runs, o, seconds))
}

fn aggregate(d: &Design, trial: &Trial, runs: &[TrialRun], o: &RunOptions, seconds: f64) -> ScenarioResult {
    let n_trials = runs.len();
    let nt = n_trials as f64;
    let dc = &d.decision;
    let (mut any_go, mut any_sig, mut ctrl_drop, mut ctrl_ev, mut n_ctrl_ev) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for r in runs {
        ctrl_drop += r.control_dropout;
        if let Some(e) = r.control_events {
            ctrl_ev += e;
            n_ctrl_ev += 1.0;
        }
        if r.arms.iter().any(|a| a.dec.call == Call::Go) {
            any_go += 1.0;
        }
        if r.arms.iter().any(|a| a.dec.sig) {
            any_sig += 1.0;
        }
    }
    let arms = trial
        .actives
        .iter()
        .enumerate()
        .map(|(k, arm)| {
            let (mut sig, mut go, mut cons, mut nogo) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
            let (mut p_go, mut p_no, mut n_p, mut drop, mut ex, mut adh, mut ev, mut n_ev) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
            let mut delta = Vec::with_capacity(n_trials);
            let mut truth = Vec::with_capacity(n_trials);
            for r in runs {
                let a = &r.arms[k];
                if a.dec.sig {
                    sig += 1.0;
                }
                match a.dec.call {
                    Call::Go => go += 1.0,
                    Call::Consider => cons += 1.0,
                    Call::NoGo => nogo += 1.0,
                }
                delta.push(a.an.delta);
                truth.push(a.truth);
                if let (Some(g), Some(n)) = (a.dec.p_go, a.dec.p_nogo) {
                    p_go += g;
                    p_no += n;
                    n_p += 1.0;
                }
                drop += a.dropout;
                ex += a.exposure;
                adh += a.adherence;
                if let Some(e) = a.events {
                    ev += e;
                    n_ev += 1.0;
                }
            }
            let finite: Vec<f64> = delta.iter().copied().filter(|x| x.is_finite()).collect();
            let s = sorted(&finite);
            let q = |p: f64| if s.is_empty() { f64::NAN } else { quantile_sorted(&s, p) };
            let pr = |x: f64| x / nt;
            let se = |p: f64| (p * (1.0 - p) / nt).sqrt();
            let tv = match dc.kind.as_str() {
                "lrv-tv" => d.to_benefit(dc.nogo_threshold),
                "mcid" => d.to_benefit(dc.mcid),
                _ => 0.0,
            };
            let p_above = if d.uncertainty.enabled { Some(truth.iter().filter(|&&x| x >= tv).count() as f64 / nt) } else { None };
            let true_mean = mean(&truth);
            ArmResult {
                id: arm.id.clone(),
                name: arm.name.clone(),
                dose: arm.dose,
                n: arm.n,
                power: pr(sig),
                p_go: pr(go),
                p_consider: pr(cons),
                p_no_go: pr(nogo),
                mcse: Mcse { power: se(pr(sig)), p_go: se(pr(go)) },
                mean_p_go: if n_p > 0.0 { Some(p_go / n_p) } else { None },
                mean_p_target: if n_p > 0.0 { Some(p_no / n_p) } else { None },
                delta: DeltaSummary {
                    mean: mean(&s),
                    q05: q(0.05),
                    q25: q(0.25),
                    q50: q(0.5),
                    q75: q(0.75),
                    q95: q(0.95),
                    values: if o.values { delta } else { vec![] },
                },
                truth: TruthSummary { mean: true_mean, values: if o.values { truth } else { vec![] }, p_above, target: tv },
                dropout: drop / nt,
                exposure: ex / nt,
                adherence: adh / nt,
                events: if n_ev > 0.0 { Some(ev / n_ev) } else { None },
            }
        })
        .collect();
    ScenarioResult {
        n_trials,
        seed: o.seed,
        uncertainty: d.uncertainty.enabled,
        total_n: d.total_n(),
        arms,
        any_go: any_go / nt,
        any_sig: any_sig / nt,
        control_dropout: ctrl_drop / nt,
        control_events: if n_ctrl_ev > 0.0 { Some(ctrl_ev / n_ctrl_ev) } else { None },
        seconds,
        n_doses: trial.n_doses,
        engine: concat!("dtb ", env!("CARGO_PKG_VERSION")),
        streams: o.streams,
    }
}
