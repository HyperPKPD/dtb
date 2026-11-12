//! Drug-drug interaction predictions.
//!
//! 1. Static screening (basic models of the FDA 2020 and EMA 2012 guidance): R1 reversible inhibition, R2 time-dependent inhibition,
//!    R3 induction, and the gut versions, each with its cut-off.
//! 2. Mechanistic static model: the predicted AUC ratio of the victim drug (liver and gut, several enzymes at once).
//! 3. Dynamic model: the perpetrator's concentration over time drives the enzyme (ordinary differential equations, Runge-Kutta), the
//!    victim's clearance follows, and the victim profile with and without the perpetrator is compared.
//!
//! Units: concentrations in micromolar, dose in mg, molecular weight in g/mol, volumes in litres, times in hours, rate constants in 1/h
//! (kinact is entered in 1/min, as it is usually reported, and converted here).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preset {
    pub label: String,
    pub r1: f64,
    pub r2: f64,
    pub r3: f64,
    pub gut_rev: f64,
    pub gut_tdi: f64,
    pub tdi_factor: f64,
    pub ind_factor: f64,
    pub qh: f64,
    pub qen: f64,
    pub igut: String,
}
impl Default for Preset {
    fn default() -> Self {
        preset("fda2020").unwrap()
    }
}

pub fn preset(id: &str) -> Option<Preset> {
    let mk = |label: &str, tdi: f64, qh: f64| Preset {
        label: label.into(),
        r1: 1.02,
        r2: 1.25,
        r3: 0.8,
        gut_rev: 11.0,
        gut_tdi: 1.25,
        tdi_factor: tdi,
        ind_factor: 10.0,
        qh,
        qen: 18.0,
        igut: "dose".into(),
    };
    match id {
        "fda2020" => Some(mk("FDA 2020", 1.0, 96.0)),
        "ema2012" => Some(mk("EMA 2012", 50.0, 97.0)),
        _ => None,
    }
}

/// Typical turnover rates (1/h) from the literature; check them against your own source.
pub const KDEG: [(&str, f64); 3] = [("CYP3A4 (liver)", 0.019), ("CYP3A4 (gut)", 0.03), ("CYP1A2", 0.018)];

fn pos(x: Option<f64>) -> bool {
    x.is_some_and(|v| v.is_finite() && v > 0.0)
}
fn posv(x: f64) -> bool {
    x.is_finite() && x > 0.0
}
fn kin(per_min: f64) -> f64 {
    per_min * 60.0
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConcInput {
    pub dose_mg: f64,
    pub mw: f64,
    /// Total steady-state Cmax in micromolar.
    pub cmax: f64,
    pub fup: f64,
    pub ka: Option<f64>,
    pub fa_fg: Option<f64>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conc {
    pub imax_u: f64,
    pub inlet_u: f64,
    pub igut: f64,
    pub dose_umol: f64,
}

/// Concentrations that drive the interaction.
pub fn concentrations(p: &ConcInput, preset: &Preset) -> Conc {
    let dose_umol = p.dose_mg / p.mw * 1000.0;
    let imax_u = p.cmax * p.fup;
    let ka_ok = p.ka.is_some_and(posv);
    let inlet_u = p.fup * (p.cmax + if ka_ok { p.fa_fg.unwrap_or(1.0) * p.ka.unwrap() * dose_umol / preset.qh } else { 0.0 });
    let igut = if preset.igut == "ka" && ka_ok { p.fa_fg.unwrap_or(1.0) * p.ka.unwrap() * dose_umol / preset.qen } else { dose_umol / 0.25 };
    Conc { imax_u, inlet_u, igut, dose_umol }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Enzyme {
    pub enzyme: String,
    pub ki: Option<f64>,
    /// 1/min
    pub kinact: Option<f64>,
    #[serde(rename = "KI")]
    pub big_ki: Option<f64>,
    pub kdeg: Option<f64>,
    pub kdeg_gut: Option<f64>,
    pub emax: Option<f64>,
    pub ec50: Option<f64>,
    pub fm: Option<f64>,
    pub gut: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StaticR {
    pub enzyme: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r1: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r1_flag: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r1_inlet: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r_gut_rev: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r_gut_rev_flag: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kobs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r2: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r2_flag: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r_gut_tdi: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r_gut_tdi_flag: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r3: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r3_flag: Option<bool>,
    pub flag: bool,
}

/// Basic static models (R values) for one enzyme or transporter row.
pub fn static_r(e: &Enzyme, c: &Conc, p: &Preset) -> StaticR {
    let mut o = StaticR { enzyme: e.enzyme.clone(), ..Default::default() };
    if pos(e.ki) {
        let ki = e.ki.unwrap();
        let r1 = 1.0 + c.imax_u / ki;
        o.r1 = Some(r1);
        o.r1_flag = Some(r1 >= p.r1);
        o.r1_inlet = Some(1.0 + c.inlet_u / ki);
        let g = 1.0 + c.igut / ki;
        o.r_gut_rev = Some(g);
        o.r_gut_rev_flag = Some(g >= p.gut_rev);
    }
    if pos(e.kinact) && pos(e.big_ki) && pos(e.kdeg) {
        let (kinact, big_ki, kdeg) = (e.kinact.unwrap(), e.big_ki.unwrap(), e.kdeg.unwrap());
        let i = p.tdi_factor * c.imax_u;
        let kobs = kin(kinact) * i / (big_ki + i);
        o.kobs = Some(kobs);
        let r2 = (kobs + kdeg) / kdeg;
        o.r2 = Some(r2);
        o.r2_flag = Some(r2 >= p.r2);
        if pos(e.kdeg_gut) {
            let kdg = e.kdeg_gut.unwrap();
            let kg = kin(kinact) * c.igut / (big_ki + c.igut);
            let r = (kg + kdg) / kdg;
            o.r_gut_tdi = Some(r);
            o.r_gut_tdi_flag = Some(r >= p.gut_tdi);
        }
    }
    if pos(e.emax) && pos(e.ec50) {
        let (emax, ec50) = (e.emax.unwrap(), e.ec50.unwrap());
        let i = p.ind_factor * c.imax_u;
        let r3 = 1.0 / (1.0 + emax * i / (ec50 + i));
        o.r3 = Some(r3);
        o.r3_flag = Some(r3 <= p.r3);
    }
    o.flag = [o.r1_flag, o.r2_flag, o.r3_flag, o.r_gut_rev_flag, o.r_gut_tdi_flag].contains(&Some(true));
    o
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Abc {
    #[serde(rename = "A")]
    pub a: f64,
    #[serde(rename = "B")]
    pub b: f64,
    #[serde(rename = "C")]
    pub c: f64,
    pub total: f64,
}

/// Activity multipliers A (reversible), B (time-dependent), C (induction) at an unbound concentration, and their product.
pub fn abc(e: &Enzyme, i: f64, gut: bool) -> Abc {
    let a = if pos(e.ki) { 1.0 / (1.0 + i / e.ki.unwrap()) } else { 1.0 };
    let kd = if gut { e.kdeg_gut.or(e.kdeg) } else { e.kdeg };
    let b = if pos(e.kinact) && pos(e.big_ki) && pos(kd) {
        let kd = kd.unwrap();
        kd / (kd + kin(e.kinact.unwrap()) * i / (e.big_ki.unwrap() + i))
    } else {
        1.0
    };
    let c = if pos(e.emax) && pos(e.ec50) { 1.0 + e.emax.unwrap() * i / (e.ec50.unwrap() + i) } else { 1.0 };
    Abc { a, b, c, total: a * b * c }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Victim {
    pub fg: Option<f64>,
    /// "imax" (default) or "inlet"
    pub hepatic_conc: Option<String>,
    pub gut: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MechRow {
    pub enzyme: String,
    pub fm: f64,
    pub hepatic: Abc,
    pub gut: Option<Abc>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mechanistic {
    pub aucr: f64,
    pub aucr_hepatic: f64,
    pub aucr_gut: f64,
    pub cl_ratio: f64,
    pub rows: Vec<MechRow>,
    pub fm_sum: f64,
    pub warnings: Vec<String>,
    pub hepatic_concentration: f64,
}

/// Mechanistic static prediction of the victim's AUC ratio (AUC with the perpetrator / AUC without).
pub fn mechanistic_aucr(enzymes: &[Enzyme], c: &Conc, victim: &Victim) -> Mechanistic {
    let fg = victim.fg.unwrap_or(1.0);
    let use_inlet = victim.hepatic_conc.as_deref() == Some("inlet");
    let ih = if use_inlet { c.inlet_u } else { c.imax_u };
    let rows: Vec<MechRow> = enzymes
        .iter()
        .map(|e| MechRow {
            enzyme: e.enzyme.clone(),
            fm: e.fm.unwrap_or(0.0),
            hepatic: abc(e, ih, false),
            gut: if e.gut { Some(abc(e, c.igut, true)) } else { None },
        })
        .collect();
    let fm_sum: f64 = rows.iter().map(|r| r.fm).sum();
    let mut warnings = Vec::new();
    if fm_sum > 1.0001 {
        warnings.push(format!("The fractions metabolised add up to {}; they cannot exceed 1.", crate::core::fixed(fm_sum, 2)));
    }
    let cl_ratio = rows.iter().map(|r| r.fm * r.hepatic.total).sum::<f64>() + (1.0 - fm_sum).max(0.0);
    let aucr_h = 1.0 / cl_ratio;
    let mut gut_total = 1.0;
    let g_row = rows.iter().find(|r| r.gut.is_some());
    if victim.gut != Some(false) {
        if let Some(g) = g_row {
            gut_total = g.gut.unwrap().total;
        }
    }
    let aucr_g = if victim.gut == Some(false) { 1.0 } else { 1.0 / (gut_total * (1.0 - fg) + fg) };
    Mechanistic { aucr: aucr_h * aucr_g, aucr_hepatic: aucr_h, aucr_gut: aucr_g, cl_ratio, rows, fm_sum, warnings, hepatic_concentration: ih }
}

#[derive(Clone, Debug, Serialize)]
pub struct Class {
    pub label: &'static str,
    pub tone: &'static str,
    pub kind: &'static str,
}

/// Class of an interaction from the AUC ratio (FDA 2020 definitions).
pub fn classify_aucr(r: f64) -> Class {
    let c = |label, tone, kind| Class { label, tone, kind };
    if !r.is_finite() {
        c("–", "neutral", "")
    } else if r >= 5.0 {
        c("Strong inhibition", "bad", "inhibition")
    } else if r >= 2.0 {
        c("Moderate inhibition", "warn", "inhibition")
    } else if r >= 1.25 {
        c("Weak inhibition", "info", "inhibition")
    } else if r > 0.8 {
        c("No relevant interaction", "ok", "none")
    } else if r > 0.5 {
        c("Weak induction", "info", "induction")
    } else if r > 0.2 {
        c("Moderate induction", "warn", "induction")
    } else {
        c("Strong induction", "bad", "induction")
    }
}

/// Concentration (micromolar) of a one-compartment oral dose given at time 0 (amounts in micromole).
pub fn oral_conc(t: f64, dose_umol: f64, f: f64, ka: f64, ke: f64, v: f64) -> f64 {
    if t < 0.0 {
        return 0.0;
    }
    if (ka - ke).abs() < 1e-9 {
        return f * dose_umol * ka * t / v * (-ke * t).exp();
    }
    f * dose_umol * ka / (v * (ka - ke)) * ((-ke * t).exp() - (-ka * t).exp())
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SteadyIn {
    pub dose_mg: f64,
    pub mw: f64,
    pub cl: f64,
    pub v: f64,
    pub ka: f64,
    pub f: f64,
    pub tau: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Steady {
    pub cmax: f64,
    pub tmax: f64,
    pub cmin: f64,
    pub cavg: f64,
}

/// Steady-state peak and average of repeated oral dosing.
pub fn steady_state(p: &SteadyIn) -> Steady {
    let du = p.dose_mg / p.mw * 1000.0;
    let ke = p.cl / p.v;
    let (mut cmax, mut tmax, mut cmin) = (0.0f64, 0.0f64, f64::INFINITY);
    let n = 2000;
    for i in 0..=n {
        let t = p.tau * i as f64 / n as f64;
        let mut c = p.f * du * p.ka / (p.v * (p.ka - ke)) * ((-ke * t).exp() / (1.0 - (-ke * p.tau).exp()) - (-p.ka * t).exp() / (1.0 - (-p.ka * p.tau).exp()));
        if (p.ka - ke).abs() < 1e-9 {
            c = 0.0;
        }
        if c > cmax {
            cmax = c;
            tmax = t;
        }
        if c < cmin {
            cmin = c;
        }
    }
    Steady { cmax, tmax, cmin, cavg: p.f * du / (p.cl * p.tau) }
}

// ------------------------------------------------------------------------------------------------ dynamic model

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DrugDyn {
    pub dose_mg: f64,
    pub mw: f64,
    pub tau: Option<f64>,
    pub n: Option<f64>,
    pub start: f64,
    pub cl: f64,
    pub v: f64,
    pub ka: f64,
    pub f: f64,
    pub fup: f64,
    pub fm: Option<f64>,
    pub fg_mult: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[derive(Default)]
pub struct DynEnzyme {
    pub ki: Option<f64>,
    pub kinact: Option<f64>,
    #[serde(rename = "KI")]
    pub big_ki: Option<f64>,
    pub emax: Option<f64>,
    pub ec50: Option<f64>,
    pub kdeg: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DynModel {
    pub perp: DrugDyn,
    pub victim: DrugDyn,
    pub enzyme: DynEnzyme,
    pub t_end: f64,
    pub dt: Option<f64>,
    pub perpetrator: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DynSeries {
    pub t: Vec<f64>,
    pub victim: Vec<f64>,
    pub perp: Vec<f64>,
    pub iu: Vec<f64>,
    pub enzyme: Vec<f64>,
    pub cl_ratio: Vec<f64>,
}

fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// Perpetrator and victim (one-compartment, first-order absorption) and an enzyme turnover model, integrated with Runge-Kutta 4.
pub fn simulate_ddi(m: &DynModel) -> DynSeries {
    let dt = m.dt.unwrap_or(0.05);
    let n_step = js_round(m.t_end / dt) as i64;
    let (p, v, e) = (&m.perp, &m.victim, &m.enzyme);
    let with_perp = m.perpetrator != Some(false);
    let mut p_doses: HashMap<i64, f64> = HashMap::new();
    let mut v_doses: HashMap<i64, f64> = HashMap::new();
    let ptau = p.tau.unwrap_or(24.0);
    for k in 0..(p.n.unwrap_or(0.0) as i64) {
        p_doses.insert(js_round((p.start + k as f64 * ptau) / dt) as i64, p.dose_mg / p.mw * 1000.0 * p.f);
    }
    for k in 0..(v.n.unwrap_or(1.0) as i64) {
        v_doses.insert(js_round((v.start + k as f64 * v.tau.unwrap_or(24.0)) / dt) as i64, v.dose_mg / v.mw * 1000.0 * v.f * v.fg_mult.unwrap_or(1.0));
    }
    let kep = p.cl / p.v;
    let kev0 = v.cl / v.v;
    let kinact = kin(e.kinact.unwrap_or(0.0));
    let kdeg = e.kdeg.unwrap_or(0.019);
    let fm = v.fm.unwrap_or(1.0);
    let ki_on = pos(e.ki);
    let big_ki_on = pos(e.big_ki);
    let ind_on = pos(e.emax) && pos(e.ec50);
    let deriv = |x: &[f64; 5]| -> [f64; 5] {
        let iu = if with_perp { (x[1] / p.v * p.fup).max(0.0) } else { 0.0 };
        let rev = if ki_on { 1.0 / (1.0 + iu / e.ki.unwrap()) } else { 1.0 };
        let tdi = if big_ki_on { kinact * iu / (e.big_ki.unwrap() + iu) } else { 0.0 };
        let ind = if ind_on { e.emax.unwrap() * iu / (e.ec50.unwrap() + iu) } else { 0.0 };
        let kev = kev0 * ((1.0 - fm) + fm * x[4] * rev);
        [-p.ka * x[0], p.ka * x[0] - kep * x[1], -v.ka * x[2], v.ka * x[2] - kev * x[3], if with_perp { kdeg * (1.0 + ind) - (kdeg + tdi) * x[4] } else { 0.0 }]
    };
    let mut out = DynSeries::default();
    let stride = (js_round(0.25 / dt) as i64).max(1);
    let mut s = [0.0, 0.0, 0.0, 0.0, 1.0];
    for i in 0..=n_step {
        if let Some(d) = p_doses.get(&i) {
            s[0] += d;
        }
        if let Some(d) = v_doses.get(&i) {
            s[2] += d;
        }
        if i % stride == 0 {
            let iu = if with_perp { s[1] / p.v * p.fup } else { 0.0 };
            let rev = if ki_on { 1.0 / (1.0 + iu / e.ki.unwrap()) } else { 1.0 };
            out.t.push(i as f64 * dt);
            out.victim.push(s[3] / v.v);
            out.perp.push(s[1] / p.v);
            out.iu.push(iu);
            out.enzyme.push(s[4]);
            out.cl_ratio.push((1.0 - fm) + fm * s[4] * rev);
        }
        let k1 = deriv(&s);
        let sh = |k: &[f64; 5], f: f64| -> [f64; 5] {
            let mut r = [0.0; 5];
            for j in 0..5 {
                r[j] = s[j] + f * dt * k[j];
            }
            r
        };
        let k2 = deriv(&sh(&k1, 0.5));
        let k3 = deriv(&sh(&k2, 0.5));
        let k4 = deriv(&sh(&k3, 1.0));
        for j in 0..5 {
            s[j] += dt / 6.0 * (k1[j] + 2.0 * k2[j] + 2.0 * k3[j] + k4[j]);
        }
    }
    out
}

fn trapz(t: &[f64], y: &[f64], a: f64, b: f64) -> f64 {
    let mut s = 0.0;
    for i in 1..t.len() {
        let lo = t[i - 1].max(a);
        let hi = t[i].min(b);
        if hi > lo {
            let f = |x: f64| y[i - 1] + (y[i] - y[i - 1]) * (x - t[i - 1]) / (t[i] - t[i - 1]);
            s += (f(lo) + f(hi)) / 2.0 * (hi - lo);
        }
    }
    s
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub control: DynSeries,
    pub interaction: DynSeries,
    pub window: [f64; 2],
    pub auc_control: f64,
    pub auc_interaction: f64,
    pub aucr: f64,
    pub cmax_control: f64,
    pub cmax_interaction: f64,
    pub cmax_ratio: f64,
}

/// Run with and without the perpetrator and summarise over a window.
pub fn compare_ddi(m: &DynModel, window: Option<[f64; 2]>) -> Comparison {
    let a = simulate_ddi(&DynModel { perpetrator: Some(false), ..m.clone() });
    let b = simulate_ddi(&DynModel { perpetrator: Some(true), ..m.clone() });
    let [w0, w1] = window.unwrap_or([m.victim.start, m.t_end]);
    let stat = |r: &DynSeries| {
        let mut cmax = 0.0f64;
        for (i, t) in r.t.iter().enumerate() {
            if *t >= w0 && *t <= w1 {
                cmax = cmax.max(r.victim[i]);
            }
        }
        (trapz(&r.t, &r.victim, w0, w1), cmax)
    };
    let (ac, cc) = stat(&a);
    let (ai, ci) = stat(&b);
    Comparison {
        control: a,
        interaction: b,
        window: [w0, w1],
        auc_control: ac,
        auc_interaction: ai,
        aucr: ai / ac,
        cmax_control: cc,
        cmax_interaction: ci,
        cmax_ratio: ci / cc,
    }
}
