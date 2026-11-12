//! The analysis of one parameter: ABE, ABEL, RSABE, NTID and the non-parametric Tmax comparison.

use super::design::{detect_design, DesignInfo};
use super::fit::{fit_fixed, uniq_in_order, Obs};
use super::nonpar::{mann_whitney_counts, signed_rank_counts};
use super::records::{cmp_per, Per, Record};
use super::{cv_to_sd, fda, sd_to_cv, Ema};
use crate::core::fixed;
use crate::stats::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BeOptions {
    pub test: String,
    #[serde(rename = "ref")]
    pub reference: String,
    pub alpha: f64,
    pub limits: [f64; 2],
    pub method: String,
    pub welch: bool,
    pub abel: Ema,
    pub np_margin: Option<[f64; 2]>,
}
impl Default for BeOptions {
    fn default() -> Self {
        BeOptions {
            test: "T".into(),
            reference: "R".into(),
            alpha: 0.1,
            limits: [0.8, 1.25],
            method: "abe".into(),
            welch: true,
            abel: Ema::default(),
            np_margin: None,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Counts {
    pub test: usize,
    #[serde(rename = "ref")]
    pub reference: usize,
    pub obs: usize,
    pub subjects: usize,
}
#[derive(Clone, Debug, Serialize)]
pub struct Gm {
    pub test: f64,
    #[serde(rename = "ref")]
    pub reference: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct AnovaRow {
    pub name: String,
    pub df: f64,
    pub ss: f64,
    pub ms: f64,
    #[serde(rename = "F")]
    pub f: f64,
    pub p: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Carryover {
    pub t: f64,
    pub df: f64,
    pub p: f64,
    pub note: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Conditions {
    pub scaled: bool,
    pub abe: bool,
    pub ratio: bool,
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BeResult {
    pub param: String,
    pub method: String,
    pub ok: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub options: Option<BeOptions>,
    pub design: Option<DesignInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<Counts>,
    pub limits: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gm: Option<Gm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub se: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub df: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mse: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gmr: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lo: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hi: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cv: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p_tost_lower: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p_tost_upper: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p_tost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inside: Option<bool>,
    pub pass: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub criterion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anova: Option<Vec<AnovaRow>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub between: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carryover: Option<Carryover>,
    // reference scaling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swr: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cvwr: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub df_r: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_r: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swt: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cvwt: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub df_t: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_t: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scaled_limits: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widened: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capped: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pe_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ci_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scaled_applied: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub howe: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theta: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub var_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub var_ratio_upper: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cond: Option<Conditions>,
    // non-parametric
    #[serde(skip_serializing_if = "Option::is_none")]
    pub c_level: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_subjects_np: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub how: Option<String>,
}

fn pct(x: f64, d: usize) -> String {
    fixed(100.0 * x, d)
}

/// A number with at most 3 decimals and no trailing zeros; infinities as +Inf / -Inf.
fn fmt3(v: f64) -> String {
    if v.is_finite() {
        let s = fixed(v, 3);
        let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
        if s == "-0" {
            "0".into()
        } else {
            s
        }
    } else if v > 0.0 {
        "+Inf".into()
    } else {
        "-Inf".into()
    }
}

/// Within-subject variance of one treatment from its own observations (subject and period effects).
pub struct WithinVar {
    pub s2: f64,
    pub df: f64,
    pub n_subjects: usize,
}
pub fn within_variance(obs: &[Obs], t: f64) -> WithinVar {
    let sub: Vec<&Obs> = obs.iter().filter(|r| r.t == t).collect();
    let mut cnt: HashMap<&str, usize> = HashMap::new();
    for r in &sub {
        *cnt.entry(r.s.as_str()).or_default() += 1;
    }
    let keep: Vec<Obs> = sub.iter().filter(|r| cnt[r.s.as_str()] >= 2).map(|r| (*r).clone()).collect();
    if keep.len() < 4 {
        return WithinVar { s2: f64::NAN, df: 0.0, n_subjects: 0 };
    }
    let f = fit_fixed(&keep, true, false);
    WithinVar { s2: f.ols.sigma2, df: f.ols.df, n_subjects: uniq_in_order(keep.iter().map(|r| r.s.as_str())).len() }
}

/// Upper (1 - alpha) bound of delta^2 - theta * sigma_R^2 by Howe's approximation.
pub fn howe_bound(d: f64, se: f64, df: f64, s2r: f64, dfr: f64, theta: f64, alpha: f64) -> f64 {
    let e = d * d - theta * s2r;
    let t = t_quantile(1.0 - alpha, df);
    let u1 = ((d.abs() + t * se).powi(2) - d * d).powi(2);
    let q = chi2_quantile(1.0 - alpha, dfr);
    let u2 = (theta * s2r * (1.0 - dfr / q)).powi(2);
    e + (u1 + u2).sqrt()
}

#[derive(Clone, Copy, Debug)]
pub struct AbelLimits {
    pub lower: f64,
    pub upper: f64,
    pub widened: bool,
    pub capped: bool,
}
/// EMA limits (as ratios) from the reference standard deviation.
pub fn abel_limits(swr: f64, a: &Ema) -> AbelLimits {
    let sw0 = cv_to_sd(a.cv_switch);
    let swc = cv_to_sd(a.cv_cap);
    if !(swr > sw0) {
        return AbelLimits { lower: 0.8, upper: 1.25, widened: false, capped: false };
    }
    let capped = swr >= swc;
    let u = (a.k * swr.min(swc)).exp();
    AbelLimits { lower: 1.0 / u, upper: u, widened: true, capped }
}

/// Analyse one parameter.
pub fn analyze_be(records: &[Record], param: &str, o: &BeOptions) -> BeResult {
    let mut res = BeResult { param: param.to_string(), method: o.method.clone(), options: Some(o.clone()), ..Default::default() };
    let recs: Vec<&Record> = records.iter().filter(|r| r.treatment == o.test || r.treatment == o.reference).collect();
    res.design = Some(detect_design(records, &o.test, &o.reference));
    if o.test == o.reference {
        res.errors.push("Test and reference must be different treatments.".into());
        return res;
    }
    let method = o.method.as_str();
    let with_val: Vec<&&Record> = recs.iter().filter(|r| r.values.get(param).is_some_and(|v| v.is_finite())).collect();
    let dropped = recs.len() - with_val.len();
    if dropped > 0 {
        res.warnings.push(format!("{dropped} observation(s) have no value for {param} and are left out."));
    }
    let parametric = method != "np";
    let bad = if parametric { with_val.iter().filter(|r| !(r.values[param] > 0.0)).count() } else { 0 };
    if bad > 0 {
        res.warnings.push(format!("{bad} observation(s) of {param} are zero or negative and cannot be log-transformed; they are left out."));
    }
    let obs: Vec<Obs> = with_val
        .iter()
        .filter(|r| !parametric || r.values[param] > 0.0)
        .map(|r| {
            let raw = r.values[param];
            Obs {
                s: r.subject.clone(),
                p: r.period.clone(),
                t: if r.treatment == o.test { 1.0 } else { 0.0 },
                y: if parametric { raw.ln() } else { raw },
                seq: r.sequence.clone(),
                raw,
            }
        })
        .collect();
    let t_n = uniq_in_order(obs.iter().filter(|r| r.t == 1.0).map(|r| r.s.as_str())).len();
    let r_n = uniq_in_order(obs.iter().filter(|r| r.t == 0.0).map(|r| r.s.as_str())).len();
    res.n = Some(Counts { test: t_n, reference: r_n, obs: obs.len(), subjects: uniq_in_order(obs.iter().map(|r| r.s.as_str())).len() });
    if t_n < 2 || r_n < 2 {
        res.errors.push("Each treatment needs at least two subjects with a value.".into());
        return res;
    }
    res.limits = Some(o.limits);
    let gm = |t: f64| {
        let v: Vec<f64> = obs.iter().filter(|r| r.t == t).map(|r| r.raw).collect();
        if v.is_empty() {
            f64::NAN
        } else if parametric {
            geomean(&v)
        } else {
            mean(&v)
        }
    };
    res.gm = Some(Gm { test: gm(1.0), reference: gm(0.0) });
    let filtered: Vec<Record> = records.iter().filter(|r| r.values.get(param).is_some_and(|v| v.is_finite() && (!parametric || *v > 0.0))).cloned().collect();
    let d = detect_design(&filtered, &o.test, &o.reference);
    res.warnings.extend(d.warnings.iter().cloned());
    res.design = Some(d.clone());
    if method == "np" {
        nonparametric(&mut res, &obs, &d, o);
        return res;
    }
    if d.kind == "parallel" {
        if method != "abe" {
            res.errors.push("Reference-scaled methods need a replicated crossover design; this is a parallel design.".into());
            return res;
        }
        parallel_abe(&mut res, &obs, o);
        return res;
    }
    crossover(&mut res, &obs, &d, o);
    res
}

fn finish_abe(res: &mut BeResult, o: &BeOptions, delta: f64, se: f64, df: f64, mse: f64) {
    let [l, u] = o.limits;
    let (ln_l, ln_u) = (l.ln(), u.ln());
    let tq = t_quantile(1.0 - o.alpha / 2.0, df);
    res.delta = Some(delta);
    res.se = Some(se);
    res.df = Some(df);
    res.mse = Some(mse);
    res.gmr = Some(delta.exp());
    let lo = (delta - tq * se).exp();
    let hi = (delta + tq * se).exp();
    res.lo = Some(lo);
    res.hi = Some(hi);
    res.cv = Some(sd_to_cv(mse.max(0.0).sqrt()));
    res.confidence = Some(1.0 - o.alpha);
    let pl = 1.0 - t_cdf((delta - ln_l) / se, df);
    let pu = t_cdf((delta - ln_u) / se, df);
    res.p_tost_lower = Some(pl);
    res.p_tost_upper = Some(pu);
    res.p_tost = Some(pl.max(pu));
    let inside = lo >= l && hi <= u;
    res.inside = Some(inside);
    res.pass = Some(inside);
    res.ok = true;
    res.criterion = Some(format!("{} % CI {} to {} % inside {} to {} %", fixed(100.0 * (1.0 - o.alpha), 0), pct(lo, 2), pct(hi, 2), pct(l, 2), pct(u, 2)));
}

fn parallel_abe(res: &mut BeResult, obs: &[Obs], o: &BeOptions) {
    let a: Vec<f64> = obs.iter().filter(|r| r.t == 1.0).map(|r| r.y).collect();
    let b: Vec<f64> = obs.iter().filter(|r| r.t == 0.0).map(|r| r.y).collect();
    let (n1, n2) = (a.len() as f64, b.len() as f64);
    let (v1, v2) = (variance(&a), variance(&b));
    let delta = mean(&a) - mean(&b);
    let (se, df, mse);
    if o.welch {
        se = (v1 / n1 + v2 / n2).sqrt();
        df = (v1 / n1 + v2 / n2).powi(2) / ((v1 / n1).powi(2) / (n1 - 1.0) + (v2 / n2).powi(2) / (n2 - 1.0));
        mse = (v1 + v2) / 2.0;
    } else {
        mse = ((n1 - 1.0) * v1 + (n2 - 1.0) * v2) / (n1 + n2 - 2.0);
        se = (mse * (1.0 / n1 + 1.0 / n2)).sqrt();
        df = n1 + n2 - 2.0;
    }
    res.warnings.push(format!(
        "Parallel design: the variability is between subjects (total CV), and the interval is based on the {} two-sample comparison of ln values.",
        if o.welch { "Welch" } else { "pooled-variance" }
    ));
    finish_abe(res, o, delta, se, df, mse);
    res.between = Some(true);
}

fn crossover(res: &mut BeResult, obs: &[Obs], d: &DesignInfo, o: &BeOptions) {
    let full = fit_fixed(obs, true, true);
    let trt = full.trt.unwrap();
    if full.ols.aliased[trt] {
        res.errors.push("The treatment effect cannot be estimated: no subject has both treatments, or the treatment is confounded with the period.".into());
        return;
    }
    if !(full.ols.df > 0.0) {
        res.errors.push("No residual degrees of freedom: the model has as many parameters as observations.".into());
        return;
    }
    let delta = full.ols.beta[trt];
    let se = full.ols.se[trt];
    let no_t = fit_fixed(obs, true, false);
    let no_p = fit_fixed(obs, false, true);
    let mse = full.ols.sigma2;
    let row = |name: &str, ss: f64, df: f64| {
        let ms = if df > 0.0 { ss / df } else { f64::NAN };
        AnovaRow {
            name: name.into(),
            df,
            ss,
            ms,
            f: if df > 0.0 { ms / mse } else { f64::NAN },
            p: if df > 0.0 { 1.0 - f_cdf(ms / mse, df, full.ols.df) } else { f64::NAN },
        }
    };
    let anova = vec![
        row("Treatment", no_t.ols.rss - full.ols.rss, no_t.ols.df - full.ols.df),
        row("Period", no_p.ols.rss - full.ols.rss, no_p.ols.df - full.ols.df),
        AnovaRow { name: "Residual".into(), df: full.ols.df, ss: full.ols.rss, ms: full.ols.rss / full.ols.df, f: f64::NAN, p: f64::NAN },
    ];
    extra_carryover(res, obs, d);
    finish_abe(res, o, delta, se, full.ols.df, mse);
    res.anova = Some(anova);
    res.rank = Some(full.ols.rank);
    if o.method == "abe" {
        return;
    }
    scaled(res, obs, o, delta, se, full.ols.df);
}

/// Sequence (carry-over) test for the 2x2x2 design from subject totals: reported for information only.
fn extra_carryover(res: &mut BeResult, obs: &[Obs], d: &DesignInfo) {
    if d.label != "2x2x2" || !d.complete {
        return;
    }
    struct B {
        sum: f64,
        first: Option<f64>,
        fp: Per,
    }
    let mut order: Vec<&str> = Vec::new();
    let mut by: HashMap<&str, B> = HashMap::new();
    for r in obs {
        let b = by.entry(r.s.as_str()).or_insert_with(|| {
            order.push(r.s.as_str());
            B { sum: 0.0, first: None, fp: Per::N(0.0) }
        });
        b.sum += r.y;
    }
    for r in obs {
        let b = by.get_mut(r.s.as_str()).unwrap();
        if b.first.is_none() || cmp_per(&r.p, &b.fp) == std::cmp::Ordering::Less {
            b.first = Some(r.t);
            b.fp = r.p.clone();
        }
    }
    let mut g: [Vec<f64>; 2] = [vec![], vec![]];
    for s in &order {
        let b = &by[s];
        g[if b.first == Some(1.0) { 0 } else { 1 }].push(b.sum);
    }
    if g[0].len() < 2 || g[1].len() < 2 {
        return;
    }
    let (n1, n2) = (g[0].len() as f64, g[1].len() as f64);
    let sp = ((n1 - 1.0) * variance(&g[0]) + (n2 - 1.0) * variance(&g[1])) / (n1 + n2 - 2.0);
    let t = (mean(&g[0]) - mean(&g[1])) / (sp * (1.0 / n1 + 1.0 / n2)).sqrt();
    let df = n1 + n2 - 2.0;
    res.carryover = Some(Carryover {
        t,
        df,
        p: 2.0 * (1.0 - t_cdf(t.abs(), df)),
        note: "Test of the sequence effect on subject totals (Grizzle). For information only: a preliminary test is not used to choose the analysis.".into(),
    });
}

fn scaled(res: &mut BeResult, obs: &[Obs], o: &BeOptions, lnpe: f64, se: f64, df: f64) {
    let r = within_variance(obs, 0.0);
    let t = within_variance(obs, 1.0);
    let swr = r.s2.sqrt();
    res.swr = Some(swr);
    res.cvwr = Some(sd_to_cv(swr));
    res.df_r = Some(r.df);
    res.n_r = Some(r.n_subjects);
    if t.df > 0.0 {
        res.swt = Some(t.s2.sqrt());
        res.cvwt = Some(sd_to_cv(t.s2.sqrt()));
        res.df_t = Some(t.df);
        res.n_t = Some(t.n_subjects);
    }
    if !(r.df > 0.0) {
        res.ok = false;
        res.errors.push("The reference is not replicated (fewer than two subjects with two reference observations): reference scaling is not possible.".into());
        res.pass = None;
        return;
    }
    let pe = res.gmr.unwrap();
    let (lo, hi) = (res.lo.unwrap(), res.hi.unwrap());
    let cvwr = res.cvwr.unwrap();
    match o.method.as_str() {
        "abel" => {
            let a = &o.abel;
            let lim = abel_limits(swr, a);
            res.scaled_limits = Some([lim.lower, lim.upper]);
            res.widened = Some(lim.widened);
            res.capped = Some(lim.capped);
            let [pl, pu] = a.pe_limits;
            let pe_ok = pe >= pl && pe <= pu;
            let ci_ok = lo >= lim.lower && hi <= lim.upper;
            res.pe_ok = Some(pe_ok);
            res.ci_ok = Some(ci_ok);
            res.pass = Some(ci_ok && pe_ok);
            res.criterion = Some(format!(
                "CVwR {} %: limits {} to {} %{}; CI {} to {} %; point estimate {} % {} {} to {} %",
                pct(cvwr, 1),
                pct(lim.lower, 2),
                pct(lim.upper, 2),
                if lim.capped { " (capped)" } else { "" },
                pct(lo, 2),
                pct(hi, 2),
                pct(pe, 2),
                if pe_ok { "within" } else { "outside" },
                pct(pl, 0),
                pct(pu, 0)
            ));
        }
        "rsabe" => {
            let use_scaled = swr >= fda::SW_SWITCH;
            res.scaled_applied = Some(use_scaled);
            let pe_ok = pe >= 0.8 && pe <= 1.25;
            res.pe_ok = Some(pe_ok);
            if use_scaled {
                let howe = howe_bound(lnpe, se, df, r.s2, r.df, fda::theta(), 0.05);
                res.howe = Some(howe);
                res.theta = Some(fda::theta());
                res.pass = Some(howe <= 0.0 && pe_ok);
                res.criterion = Some(format!(
                    "s_WR {} (CVwR {} %) is 0.294 or more: 95 % upper bound of the scaled criterion {} (must be 0 or less); point estimate {} % {} 80 to 125 %",
                    fixed(swr, 4),
                    pct(cvwr, 1),
                    fixed(howe, 4),
                    pct(pe, 2),
                    if pe_ok { "within" } else { "outside" }
                ));
            } else {
                res.pass = res.inside;
                res.criterion = Some(format!(
                    "s_WR {} is below 0.294: average bioequivalence, 90 % CI {} to {} % inside 80 to 125 %",
                    fixed(swr, 4),
                    pct(lo, 2),
                    pct(hi, 2)
                ));
            }
        }
        "ntid" => {
            if !(t.df > 0.0) {
                res.ok = false;
                res.errors.push("The narrow-therapeutic-index method needs the test product replicated as well (a four-period full replicate).".into());
                res.pass = None;
                return;
            }
            res.theta = Some(fda::nti_theta());
            let howe = howe_bound(lnpe, se, df, r.s2, r.df, fda::nti_theta(), 0.05);
            res.howe = Some(howe);
            let ratio = t.s2 / r.s2;
            let upper = ratio / f_quantile(0.05, t.df, r.df);
            res.var_ratio = Some(ratio);
            res.var_ratio_upper = Some(upper);
            let cond = Conditions { scaled: howe <= 0.0, abe: lo >= 0.8 && hi <= 1.25, ratio: upper <= 2.5 };
            res.pass = Some(cond.scaled && cond.abe && cond.ratio);
            res.criterion = Some(format!(
                "scaled bound {} (0 or less), CI {} to {} % inside 80 to 125 %, s²WT/s²WR {} with 90 % upper bound {} (2.5 or less)",
                fixed(howe, 4),
                pct(lo, 2),
                pct(hi, 2),
                fixed(ratio, 3),
                fixed(upper, 3)
            ));
            res.cond = Some(cond);
        }
        _ => {}
    }
}

fn median_of_sorted(s: &[f64]) -> f64 {
    let n = s.len();
    if n % 2 == 1 {
        s[(n - 1) / 2]
    } else {
        (s[n / 2 - 1] + s[n / 2]) / 2.0
    }
}

fn nonparametric(res: &mut BeResult, obs: &[Obs], d: &DesignInfo, o: &BeOptions) {
    res.method = "np".into();
    if d.kind != "crossover" {
        res.errors.push("The non-parametric Tmax analysis needs a crossover design.".into());
        return;
    }
    let alpha = o.alpha;
    let mut order: Vec<&str> = Vec::new();
    let mut by: HashMap<&str, Vec<&Obs>> = HashMap::new();
    for r in obs {
        by.entry(r.s.as_str())
            .or_insert_with(|| {
                order.push(r.s.as_str());
                Vec::new()
            })
            .push(r);
    }
    let (est, lo, hi, n, how, c_level);
    if d.label == "2x2x2" && d.complete {
        let mut groups: [Vec<f64>; 2] = [vec![], vec![]];
        for s in &order {
            let mut v = by[s].clone();
            v.sort_by(|a, b| cmp_per(&a.p, &b.p));
            let di = (v[1].y - v[0].y) / 2.0;
            groups[if v[0].t == 1.0 { 0 } else { 1 }].push(di);
        }
        let (tr, rt) = (&groups[0], &groups[1]);
        if tr.len() < 2 || rt.len() < 2 {
            res.errors.push("Each sequence needs at least two subjects.".into());
            return;
        }
        let mut diffs = Vec::with_capacity(rt.len() * tr.len());
        for a in rt {
            for b in tr {
                diffs.push(a - b);
            }
        }
        let w = sorted(&diffs);
        est = median_of_sorted(&w);
        let (m, nn) = (rt.len(), tr.len());
        let dist = if m * nn <= 6000 { Some(mann_whitney_counts(m, nn)) } else { None };
        let mut c: usize = 0;
        if let Some(dist) = &dist {
            let mut cum = 0.0;
            for (k, v) in dist.iter().enumerate() {
                cum += v;
                if cum <= alpha / 2.0 {
                    c = k + 1;
                } else {
                    break;
                }
            }
        } else {
            let mu = (m * nn) as f64 / 2.0;
            let sg = ((m * nn * (m + nn + 1)) as f64 / 12.0).sqrt();
            c = ((mu - norm_quantile(1.0 - alpha / 2.0) * sg - 0.5).floor() + 1.0).max(0.0) as usize;
        }
        lo = if c >= 1 { w[c - 1] } else { f64::NEG_INFINITY };
        hi = if c >= 1 { w[w.len() - c] } else { f64::INFINITY };
        n = m + nn;
        how = "Hauschke (2x2x2): Hodges-Lehmann estimate from the period-corrected half-differences, Moses interval from the Mann-Whitney distribution.";
        c_level = match &dist {
            Some(d) => 1.0 - 2.0 * d.iter().take(c).sum::<f64>(),
            None => 1.0 - alpha,
        };
    } else {
        let mut dif = Vec::new();
        for s in &order {
            let t: Vec<f64> = by[s].iter().filter(|r| r.t == 1.0).map(|r| r.y).collect();
            let r: Vec<f64> = by[s].iter().filter(|r| r.t == 0.0).map(|r| r.y).collect();
            if !t.is_empty() && !r.is_empty() {
                dif.push(mean(&t) - mean(&r));
            }
        }
        if dif.len() < 3 {
            res.errors.push("At least three subjects with both treatments are needed.".into());
            return;
        }
        let mut walsh = Vec::new();
        for i in 0..dif.len() {
            for j in i..dif.len() {
                walsh.push((dif[i] + dif[j]) / 2.0);
            }
        }
        let w = sorted(&walsh);
        est = median_of_sorted(&w);
        n = dif.len();
        let dist = signed_rank_counts(n);
        let mut k: i64 = -1;
        let mut cum = 0.0;
        for (i, v) in dist.iter().enumerate() {
            cum += v;
            if cum <= alpha / 2.0 {
                k = i as i64;
            } else {
                break;
            }
        }
        lo = if k >= 0 { w[k as usize] } else { f64::NEG_INFINITY };
        hi = if k >= 0 { w[w.len() - 1 - k as usize] } else { f64::INFINITY };
        how = "Paired design: Hodges-Lehmann estimate (median of Walsh averages of the within-subject differences) with the Wilcoxon signed-rank interval. Period effects are not removed.";
        c_level = if k >= 0 { 1.0 - 2.0 * dist.iter().take(k as usize + 1).sum::<f64>() } else { 0.0 };
    }
    res.delta = Some(est);
    res.lo = Some(lo);
    res.hi = Some(hi);
    res.n_subjects_np = Some(n);
    res.how = Some(how.into());
    res.c_level = Some(c_level);
    res.confidence = Some(c_level);
    match o.np_margin {
        Some([a, b]) if a.is_finite() && b.is_finite() => {
            let inside = lo >= a && hi <= b;
            res.inside = Some(inside);
            res.pass = Some(inside);
            res.limits = Some([a, b]);
            res.criterion = Some(format!("CI {} to {} inside {} to {}", fmt3(lo), fmt3(hi), a, b));
        }
        _ => {
            res.pass = None;
            res.limits = None;
            res.criterion = Some(format!("difference {} (CI {} to {}); no limits set", fmt3(est), fmt3(lo), fmt3(hi)));
        }
    }
    res.gmr = Some(f64::NAN);
    res.ok = true;
}

#[derive(Clone, Debug, Serialize)]
pub struct ResidualRow {
    pub subject: String,
    pub period: Per,
    pub treatment: String,
    pub value: f64,
    pub residual: f64,
    pub std: f64,
}

/// Per-observation residuals of the log model (standardised by the residual SD) to spot outliers. Crossover only.
pub fn residual_table(records: &[Record], param: &str, o: &BeOptions) -> Vec<ResidualRow> {
    let used: Vec<&Record> =
        records.iter().filter(|r| (r.treatment == o.test || r.treatment == o.reference) && r.values.get(param).is_some_and(|v| *v > 0.0)).collect();
    if used.is_empty() {
        return vec![];
    }
    let obs: Vec<Obs> = used
        .iter()
        .map(|r| Obs {
            s: r.subject.clone(),
            p: r.period.clone(),
            t: if r.treatment == o.test { 1.0 } else { 0.0 },
            y: r.values[param].ln(),
            seq: String::new(),
            raw: r.values[param],
        })
        .collect();
    let f = fit_fixed(&obs, true, true);
    if !(f.ols.df > 0.0) {
        return vec![];
    }
    // rebuild the design matrix to get the fitted values
    let subs = uniq_in_order(obs.iter().map(|r| r.s.as_str()));
    let si: HashMap<&str, usize> = subs.iter().enumerate().map(|(i, s)| (*s, i)).collect();
    let mut pers: Vec<Per> = Vec::new();
    for r in &obs {
        if !pers.iter().any(|p| p.key() == r.p.key()) {
            pers.push(r.p.clone());
        }
    }
    pers.sort_by(cmp_per);
    let n_s = subs.len();
    let p = n_s + pers.len().saturating_sub(1) + 1;
    let sd = f.ols.sigma2.sqrt();
    obs.iter()
        .zip(used.iter())
        .map(|(r, rec)| {
            let mut x = vec![0.0; p];
            x[si[r.s.as_str()]] = 1.0;
            let k = pers.iter().position(|q| q.key() == r.p.key()).unwrap();
            if k > 0 {
                x[n_s + k - 1] = 1.0;
            }
            x[p - 1] = r.t;
            let fit: f64 = (0..p).filter(|&j| !f.ols.aliased[j]).map(|j| x[j] * f.ols.beta[j]).sum();
            let res = r.y - fit;
            ResidualRow {
                subject: r.s.clone(),
                period: r.p.clone(),
                treatment: if r.t == 1.0 { o.test.clone() } else { o.reference.clone() },
                value: rec.values[param],
                residual: res,
                std: res / sd,
            }
        })
        .collect()
}
