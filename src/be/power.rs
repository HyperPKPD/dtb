//! Power and sample size for bioequivalence.
//!
//! * ABE: exact power of the two one-sided tests by numerical integration over the distribution of the residual variance (the same
//!   quantity as Owen's Q), for any crossover or replicate design and for parallel groups.
//! * ABEL, RSABE, NTID: simulation of the whole decision with the design's own estimator.
//!
//! Designs are lists of sequences (for example ["TR", "RT"]). All constants (variance factor, degrees of freedom) come from the
//! fixed-effects model of the design itself, not from a table.

use super::analyze::{abel_limits, howe_bound};
use super::{cv_to_sd, fda, Ema};
use crate::stats::*;
use crate::trial::sim::Streams;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize)]
pub struct DesignPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub sequences: Option<Vec<&'static str>>,
}

pub fn design_presets() -> Vec<DesignPreset> {
    vec![
        DesignPreset { id: "2x2x2", label: "Two-sequence, two-period crossover (TR, RT)", sequences: Some(vec!["TR", "RT"]) },
        DesignPreset { id: "2x2x3", label: "Two-sequence, three-period replicate (TRT, RTR)", sequences: Some(vec!["TRT", "RTR"]) },
        DesignPreset { id: "2x2x4", label: "Two-sequence, four-period full replicate (TRTR, RTRT)", sequences: Some(vec!["TRTR", "RTRT"]) },
        DesignPreset { id: "3x2x3", label: "Three-sequence, three-period partial replicate (TRR, RTR, RRT)", sequences: Some(vec!["TRR", "RTR", "RRT"]) },
        DesignPreset {
            id: "4x2x4",
            label: "Four-sequence, four-period replicate (TRTR, RTRT, TRRT, RTTR)",
            sequences: Some(vec!["TRTR", "RTRT", "TRRT", "RTTR"]),
        },
        DesignPreset { id: "parallel", label: "Parallel groups", sequences: None },
    ]
}

/// The sequences of a design preset id ("2x2x2" ...), or None for parallel (and for an unknown id).
pub fn preset_sequences(id: &str) -> Option<Vec<String>> {
    design_presets().into_iter().find(|d| d.id == id).and_then(|d| d.sequences).map(|v| v.into_iter().map(String::from).collect())
}

// ------------------------------------------------------------------------------------------------ linear algebra on a design

/// Invert a small symmetric matrix by Gauss-Jordan with a pivoting tolerance; dependent columns are dropped (aliased).
fn invert_sym(a: &[Vec<f64>]) -> (Vec<Vec<f64>>, Vec<usize>) {
    let q = a.len();
    let mut m: Vec<Vec<f64>> = a.to_vec();
    let diag0: Vec<f64> = (0..q).map(|i| a[i][i]).collect();
    let mut done = vec![false; q];
    for k in 0..q {
        let piv = m[k][k];
        if !(piv > 1e-9 * diag0[k].max(1e-300)) {
            for j in 0..q {
                m[k][j] = 0.0;
                m[j][k] = 0.0;
            }
            continue;
        }
        done[k] = true;
        let d = 1.0 / piv;
        for j in 0..q {
            if j != k {
                m[k][j] *= d;
            }
        }
        for i in 0..q {
            if i == k {
                continue;
            }
            let f = m[i][k];
            if f == 0.0 {
                continue;
            }
            for j in 0..q {
                if j != k {
                    m[i][j] -= f * m[k][j];
                }
            }
            m[i][k] = -f * d;
        }
        m[k][k] = d;
    }
    let keep = (0..q).filter(|&k| done[k]).collect();
    (m, keep)
}

#[derive(Clone, Copy, Debug)]
pub struct DRow {
    pub s: usize,
    pub p: usize,
    pub t: f64,
}

/// A fitter for one design: fixed subject effects removed by within-subject centring, then period and treatment columns. The matrices
/// are computed once; each call with a vector of log values costs O(n q).
pub struct Fitter {
    pub n: usize,
    pub n_subjects: usize,
    pub df: f64,
    pub rank: usize,
    pub trt_ok: bool,
    pub se_unit: f64,
    pub weights: Vec<f64>,
    subj_of: Vec<usize>,
    cnt: Vec<f64>,
    z: Vec<Vec<f64>>,
    inv: Vec<Vec<f64>>,
    keep: Vec<usize>,
}

pub struct FitOut {
    pub rss: f64,
    pub mse: f64,
    pub delta: f64,
}

impl Fitter {
    pub fn new(rows: &[DRow], treatment: bool) -> Fitter {
        let n = rows.len();
        let mut subs: Vec<usize> = Vec::new();
        for r in rows {
            if !subs.contains(&r.s) {
                subs.push(r.s);
            }
        }
        let n_s = subs.len();
        let subj_of: Vec<usize> = rows.iter().map(|r| subs.iter().position(|&s| s == r.s).unwrap()).collect();
        let mut cnt = vec![0.0f64; n_s];
        for &k in &subj_of {
            cnt[k] += 1.0;
        }
        let mut pers: Vec<usize> = rows.iter().map(|r| r.p).collect();
        pers.sort_unstable();
        pers.dedup();
        let mut cols: Vec<Vec<f64>> = Vec::new();
        for k in 1..pers.len() {
            cols.push(rows.iter().map(|r| if r.p == pers[k] { 1.0 } else { 0.0 }).collect());
        }
        let trt_col = if treatment { Some(cols.len()) } else { None };
        if treatment {
            cols.push(rows.iter().map(|r| r.t).collect());
        }
        let q = cols.len();
        let centre = |v: &[f64]| -> Vec<f64> {
            let mut m = vec![0.0f64; n_s];
            for i in 0..n {
                m[subj_of[i]] += v[i];
            }
            for k in 0..n_s {
                m[k] /= cnt[k];
            }
            (0..n).map(|i| v[i] - m[subj_of[i]]).collect()
        };
        let z: Vec<Vec<f64>> = cols.iter().map(|c| centre(c)).collect();
        let ztz: Vec<Vec<f64>> = (0..q).map(|a| (0..q).map(|b| (0..n).map(|i| z[a][i] * z[b][i]).sum()).collect()).collect();
        let (inv, keep) = if q > 0 { invert_sym(&ztz) } else { (vec![], vec![]) };
        let rank = keep.len();
        let df = n as f64 - n_s as f64 - rank as f64;
        let trt_ok = trt_col.is_some_and(|c| keep.contains(&c));
        let se_unit = if trt_ok { inv[trt_col.unwrap()][trt_col.unwrap()].sqrt() } else { f64::NAN };
        let mut weights = Vec::new();
        if trt_ok {
            let tc = trt_col.unwrap();
            weights = (0..n).map(|i| keep.iter().map(|&a| inv[tc][a] * z[a][i]).sum()).collect();
        }
        Fitter { n, n_subjects: n_s, df, rank, trt_ok, se_unit, weights, subj_of, cnt, z, inv, keep }
    }

    pub fn fit(&self, y: &[f64], ydm: &mut [f64]) -> FitOut {
        let n = self.n;
        let mut m = vec![0.0f64; self.n_subjects];
        for i in 0..n {
            m[self.subj_of[i]] += y[i];
        }
        for k in 0..self.n_subjects {
            m[k] /= self.cnt[k];
        }
        let mut yy = 0.0;
        for i in 0..n {
            let v = y[i] - m[self.subj_of[i]];
            ydm[i] = v;
            yy += v * v;
        }
        let q = self.z.len();
        let mut zty = vec![0.0f64; q];
        for &a in &self.keep {
            zty[a] = self.z[a].iter().zip(ydm.iter()).map(|(p, q)| p * q).sum();
        }
        let mut bz = 0.0;
        for &a in &self.keep {
            let b: f64 = self.keep.iter().map(|&c| self.inv[a][c] * zty[c]).sum();
            bz += b * zty[a];
        }
        let rss = (yy - bz).max(0.0);
        let delta = if self.trt_ok { self.weights.iter().zip(y.iter()).map(|(w, v)| w * v).sum() } else { f64::NAN };
        FitOut { rss, mse: if self.df > 0.0 { rss / self.df } else { f64::NAN }, delta }
    }
}

/// The rows of a balanced design with k subjects per sequence.
pub fn design_rows(sequences: &[String], k: usize) -> Vec<DRow> {
    let mut rows = Vec::new();
    let mut id = 0;
    for seq in sequences {
        for _ in 0..k {
            id += 1;
            for (p, c) in seq.chars().enumerate() {
                rows.push(DRow { s: id, p: p + 1, t: if c == 'T' { 1.0 } else { 0.0 } });
            }
        }
    }
    rows
}

fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignConstants {
    pub parallel: bool,
    pub n_seq: usize,
    pub n_per: usize,
    pub c: f64,
    pub label: String,
    pub has_r_rep: bool,
    pub has_t_rep: bool,
    #[serde(skip)]
    df_slope: f64,
    #[serde(skip)]
    r0: f64,
    #[serde(skip)]
    r: Option<(f64, f64)>,
    #[serde(skip)]
    t: Option<(f64, f64)>,
}

impl DesignConstants {
    pub fn df(&self, n: f64) -> f64 {
        if self.parallel {
            n - 2.0
        } else {
            js_round(n * self.df_slope - self.r0)
        }
    }
    pub fn df_r(&self, n: f64) -> f64 {
        self.r.map_or(f64::NAN, |(s, r0)| js_round(n * s - r0))
    }
    pub fn df_t(&self, n: f64) -> f64 {
        self.t.map_or(f64::NAN, |(s, r0)| js_round(n * s - r0))
    }
}

/// Constants of a design: variance factor c (variance of the treatment difference = c sigma^2 / N) and the degrees of freedom as a
/// function of N (also for reference-only and test-only variance estimates).
pub fn design_constants(sequences: Option<&[String]>) -> DesignConstants {
    let Some(sequences) = sequences else {
        return DesignConstants {
            parallel: true,
            n_seq: 2,
            n_per: 1,
            c: 4.0,
            label: "parallel".into(),
            has_r_rep: false,
            has_t_rep: false,
            df_slope: 1.0,
            r0: 2.0,
            r: None,
            t: None,
        };
    };
    let n_seq = sequences.len();
    let k = 4;
    let rows = design_rows(sequences, k);
    let n0 = (n_seq * k) as f64;
    let f = Fitter::new(&rows, true);
    let n_per = sequences.iter().map(|s| s.len()).max().unwrap_or(0);
    let c = f.se_unit.powi(2) * n0;
    let r0 = f.rank as f64;
    let sub = |tv: f64| -> Option<(f64, f64)> {
        let mut cnt_by = std::collections::HashMap::new();
        for r in &rows {
            if r.t == tv {
                *cnt_by.entry(r.s).or_insert(0usize) += 1;
            }
        }
        let keep: Vec<DRow> = rows.iter().filter(|r| r.t == tv && cnt_by[&r.s] >= 2).copied().collect();
        if keep.len() < 4 {
            return None;
        }
        let g = Fitter::new(&keep, false);
        Some(((keep.len() as f64 - g.n_subjects as f64) / n0, g.rank as f64))
    };
    let (r, t) = (sub(0.0), sub(1.0));
    let df_slope = (rows.len() as f64 - f.n_subjects as f64) / n0;
    DesignConstants { parallel: false, n_seq, n_per, c, label: sequences.join("/"), has_r_rep: r.is_some(), has_t_rep: t.is_some(), df_slope, r0, r, t }
}

// ------------------------------------------------------------------------------------------------ exact power of ABE

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AbeArgs {
    pub cv: f64,
    pub theta0: f64,
    pub n: f64,
    pub sequences: Option<Vec<String>>,
    pub limits: [f64; 2],
    pub alpha: f64,
}
impl Default for AbeArgs {
    fn default() -> Self {
        AbeArgs { cv: 0.3, theta0: 0.95, n: 24.0, sequences: Some(vec!["TR".into(), "RT".into()]), limits: [0.8, 1.25], alpha: 0.05 }
    }
}

/// Exact power of the two one-sided tests for an average bioequivalence study.
pub fn power_abe(a: &AbeArgs) -> f64 {
    let dc = design_constants(a.sequences.as_deref());
    power_abe_dc(a, &dc)
}

fn power_abe_dc(a: &AbeArgs, dc: &DesignConstants) -> f64 {
    let n = a.n;
    let df = dc.df(n);
    if !(df > 0.0) {
        return 0.0;
    }
    let sw = cv_to_sd(a.cv);
    let se = sw * (dc.c / n).sqrt();
    let t = t_quantile(1.0 - a.alpha, df);
    let (ln_l, ln_u, d0) = (a.limits[0].ln(), a.limits[1].ln(), a.theta0.ln());
    let half = (ln_u - ln_l) / 2.0;
    let w_max = df * (half / (t * sw * (dc.c / n).sqrt())).powi(2);
    let w_hi = w_max.min(chi2_quantile(1.0 - 1e-13, df));
    if !(w_hi > 0.0) {
        return 0.0;
    }
    let f = |w: f64| {
        let s_hat = sw * ((w / df) * (dc.c / n)).sqrt();
        let lo = (ln_l + t * s_hat - d0) / se;
        let hi = (ln_u - t * s_hat - d0) / se;
        chi2_pdf(w, df) * (norm_cdf(hi) - norm_cdf(lo)).max(0.0)
    };
    integrate(f, 0.0, w_hi, 24, 32).clamp(0.0, 1.0)
}

#[derive(Clone, Debug, Serialize)]
pub struct SampleSize {
    pub n: f64,
    pub power: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Smallest total N (a multiple of the number of sequences, at least min_n) with power at least the target.
pub fn sample_size_abe(a: &AbeArgs, target: f64, min_n: f64, max_n: f64) -> SampleSize {
    let dc = design_constants(a.sequences.as_deref());
    let step = a.sequences.as_ref().map_or(2.0, |s| s.len() as f64);
    let lo = min_n.max(step * 2.0);
    let pw = |n: f64| power_abe_dc(&AbeArgs { n, ..a.clone() }, &dc);
    let mut n = (lo / step).ceil() * step;
    if pw(n) >= target {
        return SampleSize { n, power: pw(n), note: None };
    }
    let mut hi = n;
    while hi < max_n && pw(hi) < target {
        n = hi;
        hi = max_n.min(hi * 2.0);
    }
    if pw(hi) < target {
        return SampleSize { n: f64::NAN, power: pw(hi), note: Some("Not reached within the maximum number of subjects.".into()) };
    }
    let (mut lo_k, mut hi_k) = ((n / step).ceil(), (hi / step).ceil());
    while hi_k - lo_k > 1.0 {
        let m = ((lo_k + hi_k) / 2.0).floor();
        if pw(m * step) >= target {
            hi_k = m;
        } else {
            lo_k = m;
        }
    }
    // power is not exactly monotone in N (degrees of freedom): step down while still above target
    let mut best = hi_k;
    let floor_k = (lo / step).ceil();
    while best > floor_k && pw((best - 1.0) * step) >= target {
        best -= 1.0;
    }
    SampleSize { n: best * step, power: pw(best * step), note: None }
}

// ------------------------------------------------------------------------------------------------ simulation of the other methods

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SimArgs {
    pub method: String,
    pub cv_t: f64,
    pub cv_r: f64,
    pub theta0: f64,
    pub n: f64,
    pub sequences: Vec<String>,
    pub n_sim: usize,
    pub seed: u64,
    pub limits: [f64; 2],
    pub alpha: f64,
    pub abel: Ema,
}
impl Default for SimArgs {
    fn default() -> Self {
        SimArgs {
            method: "abel".into(),
            cv_t: 0.4,
            cv_r: 0.4,
            theta0: 0.95,
            n: 36.0,
            sequences: vec!["TRTR".into(), "RTRT".into()],
            n_sim: 10000,
            seed: 12345,
            limits: [0.8, 1.25],
            alpha: 0.05,
            abel: Ema::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerSim {
    pub power: f64,
    pub se: f64,
    pub n_sim: usize,
    pub n: usize,
    pub widened_share: f64,
    pub mean_swr: f64,
}

const SIM_BLOCK: usize = 250;

/// Probability that a study passes the chosen method, by simulating the decision.
pub fn power_sim(a: &SimArgs, streams: Streams) -> Result<PowerSim, String> {
    if a.sequences.is_empty() {
        return Err("Scaled methods need a replicate crossover design.".into());
    }
    let n_seq = a.sequences.len();
    let k = ((a.n / n_seq as f64).round() as usize).max(1);
    let rows = design_rows(&a.sequences, k);
    let full = Fitter::new(&rows, true);
    let mut cnt_r = std::collections::HashMap::new();
    let mut cnt_t = std::collections::HashMap::new();
    for r in &rows {
        *(if r.t == 1.0 { &mut cnt_t } else { &mut cnt_r }).entry(r.s).or_insert(0usize) += 1;
    }
    let rows_r: Vec<DRow> = rows.iter().filter(|r| r.t == 0.0 && cnt_r[&r.s] >= 2).copied().collect();
    let rows_t: Vec<DRow> = rows.iter().filter(|r| r.t == 1.0 && cnt_t[&r.s] >= 2).copied().collect();
    let f_r = if rows_r.len() >= 4 { Some(Fitter::new(&rows_r, false)) } else { None };
    let f_t = if rows_t.len() >= 4 { Some(Fitter::new(&rows_t, false)) } else { None };
    let Some(f_r) = f_r.filter(|f| f.df > 0.0) else { return Err("This design has no replicated reference, so reference scaling is not possible.".into()) };
    if a.method == "ntid" && f_t.as_ref().is_none_or(|f| !(f.df > 0.0)) {
        return Err("The narrow-therapeutic-index method needs the test product replicated too.".into());
    }
    if !matches!(a.method.as_str(), "abe" | "abel" | "rsabe" | "ntid") {
        return Err(format!("Unknown method \"{}\" (use abe, abel, rsabe or ntid).", a.method));
    }
    let (s_t, s_r, d0) = (cv_to_sd(a.cv_t), cv_to_sd(a.cv_r), a.theta0.ln());
    let n_obs = rows.len();
    let idx_r: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| r.t == 0.0 && cnt_r[&r.s] >= 2).map(|(i, _)| i).collect();
    let idx_t: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| r.t == 1.0 && cnt_t[&r.s] >= 2).map(|(i, _)| i).collect();
    let tcrit = t_quantile(1.0 - a.alpha, full.df);
    let (ln_l, ln_u) = (a.limits[0].ln(), a.limits[1].ln());
    let (lo80, hi125) = (0.8f64.ln(), 1.25f64.ln());
    let f_quant_nt = if a.method == "ntid" { f_quantile(0.05, f_t.as_ref().unwrap().df, f_r.df) } else { 0.0 };

    // one block of simulations with one stream
    let run_block = |rng: &mut Rng, count: usize| -> (usize, usize, f64) {
        let mut y = vec![0.0f64; n_obs];
        let mut ydm = vec![0.0f64; n_obs];
        let mut y_r = vec![0.0f64; idx_r.len()];
        let mut y_t = vec![0.0f64; idx_t.len()];
        let mut ydm_r = vec![0.0f64; idx_r.len()];
        let mut ydm_t = vec![0.0f64; idx_t.len()];
        let (mut pass, mut widen, mut swr_sum) = (0usize, 0usize, 0.0f64);
        for _ in 0..count {
            for i in 0..n_obs {
                y[i] = if rows[i].t == 1.0 { d0 + s_t * rng.std_normal() } else { s_r * rng.std_normal() };
            }
            let f = full.fit(&y, &mut ydm);
            let se = full.se_unit * f.mse.sqrt();
            for (i, &ix) in idx_r.iter().enumerate() {
                y_r[i] = y[ix];
            }
            let r = f_r.fit(&y_r, &mut ydm_r);
            let s2r = r.mse;
            let swr = s2r.sqrt();
            swr_sum += swr;
            let lo = f.delta - tcrit * se;
            let hi = f.delta + tcrit * se;
            let pe = f.delta.exp();
            let ok = match a.method.as_str() {
                "abe" => lo >= ln_l && hi <= ln_u,
                "abel" => {
                    let lim = abel_limits(swr, &a.abel);
                    if lim.widened {
                        widen += 1;
                    }
                    lo >= lim.lower.ln() && hi <= lim.upper.ln() && pe >= a.abel.pe_limits[0] && pe <= a.abel.pe_limits[1]
                }
                "rsabe" => {
                    if swr >= fda::SW_SWITCH {
                        widen += 1;
                        howe_bound(f.delta, se, full.df, s2r, f_r.df, fda::theta(), 0.05) <= 0.0 && pe >= 0.8 && pe <= 1.25
                    } else {
                        lo >= lo80 && hi <= hi125
                    }
                }
                _ => {
                    for (i, &ix) in idx_t.iter().enumerate() {
                        y_t[i] = y[ix];
                    }
                    let t = f_t.as_ref().unwrap().fit(&y_t, &mut ydm_t);
                    let upper = (t.mse / s2r) / f_quant_nt;
                    howe_bound(f.delta, se, full.df, s2r, f_r.df, fda::nti_theta(), 0.05) <= 0.0 && lo >= lo80 && hi <= hi125 && upper <= 2.5
                }
            };
            if ok {
                pass += 1;
            }
        }
        (pass, widen, swr_sum)
    };

    let n_sim = a.n_sim.max(1);
    let (pass, widen, swr_sum) = match streams {
        Streams::Js => {
            let mut rng = Rng::new(a.seed);
            run_block(&mut rng, n_sim)
        }
        Streams::Blocks => {
            let blocks = n_sim.div_ceil(SIM_BLOCK);
            let parts: Vec<(usize, usize, f64)> = (0..blocks)
                .into_par_iter()
                .map(|b| {
                    let mut rng = Rng::for_chunk(a.seed, b as u64);
                    run_block(&mut rng, SIM_BLOCK.min(n_sim - b * SIM_BLOCK))
                })
                .collect();
            parts.iter().fold((0, 0, 0.0), |s, p| (s.0 + p.0, s.1 + p.1, s.2 + p.2))
        }
    };
    let p = pass as f64 / n_sim as f64;
    Ok(PowerSim {
        power: p,
        se: (p * (1.0 - p) / n_sim as f64).sqrt(),
        n_sim,
        n: k * n_seq,
        widened_share: widen as f64 / n_sim as f64,
        mean_swr: swr_sum / n_sim as f64,
    })
}

/// Smallest N reaching the target power for a simulated method (the same random numbers at every N).
pub fn sample_size_sim(a: &SimArgs, target: f64, min_n: f64, max_n: f64, streams: Streams) -> Result<SampleSize, String> {
    let n_seq = a.sequences.len() as f64;
    let pw = |n: f64| power_sim(&SimArgs { n, ..a.clone() }, streams).map(|r| r.power);
    let step = n_seq;
    let mut lo = (min_n / step).ceil();
    let mut p = pw(lo * step)?;
    if p >= target {
        return Ok(SampleSize { n: lo * step, power: p, note: None });
    }
    let mut hi = lo;
    while hi * step < max_n && p < target {
        lo = hi;
        hi = (max_n / step).ceil().min(hi * 2.0);
        p = pw(hi * step)?;
    }
    if p < target {
        return Ok(SampleSize { n: f64::NAN, power: p, note: Some("Not reached within the maximum number of subjects.".into()) });
    }
    while hi - lo > 1.0 {
        let m = ((lo + hi) / 2.0).floor();
        if pw(m * step)? >= target {
            hi = m;
        } else {
            lo = m;
        }
    }
    Ok(SampleSize { n: hi * step, power: pw(hi * step)?, note: None })
}

/// Enrolled number to expect `n` completers when a share of subjects drop out, rounded up to a multiple of the sequences.
pub fn with_dropout(n: f64, rate: f64, n_seq: f64) -> f64 {
    ((n / (1.0 - rate)).ceil() / n_seq).ceil() * n_seq
}

#[derive(Clone, Debug, Serialize)]
pub struct SizeRow {
    pub ratio: f64,
    pub n: Vec<f64>,
}

/// Table of sample sizes for a set of CVs and ratios (ABE exact); rows computed in parallel.
pub fn sample_size_table(cvs: &[f64], ratios: &[f64], sequences: Option<&[String]>, limits: [f64; 2], alpha: f64, target: f64, min_n: f64) -> Vec<SizeRow> {
    ratios
        .par_iter()
        .map(|&th| SizeRow {
            ratio: th,
            n: cvs
                .iter()
                .map(|&cv| {
                    sample_size_abe(&AbeArgs { cv, theta0: th, n: 0.0, sequences: sequences.map(|s| s.to_vec()), limits, alpha }, target, min_n, 5000.0).n
                })
                .collect(),
        })
        .collect()
}
