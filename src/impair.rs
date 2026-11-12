//! Renal and hepatic impairment: kidney-function estimates, categories, dose adjustment by the Giusti-Hayton method, Child-Pugh scoring,
//! a well-stirred liver model, and the analysis of an impairment study (group ratios and a covariate regression).

use crate::stats::*;
use serde::{Deserialize, Serialize};

// ------------------------------------------------------------------------------------------------ kidney function

/// Ideal body weight (kg), Devine. Height in cm.
pub fn ideal_weight(height_cm: f64, female: bool) -> f64 {
    (if female { 45.5 } else { 50.0 }) + 2.3 * (height_cm / 2.54 - 60.0).max(0.0)
}

/// Body surface area (m²): "dubois" (default) or "mosteller".
pub fn bsa(height_cm: f64, weight_kg: f64, formula: &str) -> f64 {
    if formula == "mosteller" {
        (height_cm * weight_kg / 3600.0).sqrt()
    } else {
        0.007184 * weight_kg.powf(0.425) * height_cm.powf(0.725)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Person {
    pub age: f64,
    pub weight: f64,
    pub height: f64,
    pub female: bool,
    /// serum creatinine, mg/dL
    pub scr: f64,
    pub black: bool,
    /// "actual" (default), "ideal" or "adjusted"
    pub weight_type: Option<String>,
}

/// Cockcroft-Gault creatinine clearance (mL/min).
pub fn cockcroft_gault(p: &Person) -> f64 {
    let wt = p.weight_type.as_deref().unwrap_or("actual");
    let mut w = p.weight;
    if wt != "actual" {
        let ibw = ideal_weight(p.height, p.female);
        w = if wt == "ideal" { ibw } else { ibw + 0.4 * (p.weight - ibw) };
    }
    (140.0 - p.age) * w / (72.0 * p.scr) * if p.female { 0.85 } else { 1.0 }
}

/// MDRD (IDMS-traceable) eGFR in mL/min/1.73 m².
pub fn mdrd(p: &Person) -> f64 {
    175.0 * p.scr.powf(-1.154) * p.age.powf(-0.203) * if p.female { 0.742 } else { 1.0 } * if p.black { 1.212 } else { 1.0 }
}

/// CKD-EPI 2009 creatinine equation (with the race coefficient) in mL/min/1.73 m².
pub fn ckd_epi_2009(p: &Person) -> f64 {
    let k = if p.female { 0.7 } else { 0.9 };
    let a = if p.female { -0.329 } else { -0.411 };
    141.0
        * (p.scr / k).min(1.0).powf(a)
        * (p.scr / k).max(1.0).powf(-1.209)
        * 0.993f64.powf(p.age)
        * if p.female { 1.018 } else { 1.0 }
        * if p.black { 1.159 } else { 1.0 }
}

/// CKD-EPI 2021 creatinine equation (without race) in mL/min/1.73 m².
pub fn ckd_epi_2021(p: &Person) -> f64 {
    let k = if p.female { 0.7 } else { 0.9 };
    let a = if p.female { -0.241 } else { -0.302 };
    142.0 * (p.scr / k).min(1.0).powf(a) * (p.scr / k).max(1.0).powf(-1.2) * 0.9938f64.powf(p.age) * if p.female { 1.012 } else { 1.0 }
}

/// Convert an eGFR normalised to 1.73 m² into the absolute value (mL/min) for the patient's surface area.
pub fn denormalise(egfr: f64, bsa_m2: f64) -> f64 {
    egfr * bsa_m2 / 1.73
}

#[derive(Clone, Debug, Serialize)]
pub struct RenalCategory {
    pub id: &'static str,
    pub label: &'static str,
    pub min: f64,
    pub max: f64,
    pub mid: f64,
}

/// FDA renal impairment categories (2020 guidance) from eGFR or CrCl in mL/min.
pub fn renal_categories() -> Vec<RenalCategory> {
    let c = |id, label, min, max, mid| RenalCategory { id, label, min, max, mid };
    vec![
        c("normal", "Normal", 90.0, f64::INFINITY, 105.0),
        c("mild", "Mild", 60.0, 90.0, 75.0),
        c("moderate", "Moderate", 30.0, 60.0, 45.0),
        c("severe", "Severe", 15.0, 30.0, 22.0),
        c("esrd", "End-stage (not on dialysis)", 0.0, 15.0, 10.0),
    ]
}

pub fn renal_category(v: f64) -> Option<RenalCategory> {
    if !v.is_finite() {
        return None;
    }
    let cats = renal_categories();
    Some(cats.iter().find(|c| v >= c.min && v < c.max).cloned().unwrap_or_else(|| cats[0].clone()))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Giusti {
    pub kf: f64,
    pub q: f64,
    pub dose_factor: f64,
    pub interval_factor: f64,
    pub auc_ratio: f64,
    pub cl_ratio: f64,
    pub half_life_ratio: f64,
}

/// Giusti-Hayton: Q = 1 - fe (1 - KF) with KF = patient clearance / normal clearance. The same exposure is kept by giving Q times
/// the dose, or by lengthening the interval to tau / Q.
pub fn giusti_hayton(fe: f64, crcl: f64, crcl_normal: f64) -> Giusti {
    let kf = (crcl / crcl_normal).clamp(0.0, 1.0);
    let q = 1.0 - fe * (1.0 - kf);
    Giusti { kf, q, dose_factor: q, interval_factor: 1.0 / q, auc_ratio: 1.0 / q, cl_ratio: q, half_life_ratio: 1.0 / q }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GiustiRow {
    pub category: String,
    pub crcl: f64,
    #[serde(flatten)]
    pub g: Giusti,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dose: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tau_adj: Option<f64>,
}

pub fn giusti_hayton_table(fe: f64, crcl_normal: f64, dose: Option<f64>, tau: Option<f64>) -> Vec<GiustiRow> {
    renal_categories()
        .iter()
        .map(|c| {
            let g = giusti_hayton(fe, c.mid, crcl_normal);
            GiustiRow { category: c.label.into(), crcl: c.mid, dose: dose.map(|d| d * g.q), tau_adj: tau.map(|t| t / g.q), g }
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------ liver function

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ChildPughIn {
    /// mg/dL
    pub bilirubin: f64,
    /// g/dL
    pub albumin: f64,
    pub inr: f64,
    /// 0 none, 1 mild or controlled, 2 moderate to severe
    pub ascites: f64,
    /// 0 none, 1 grade 1 to 2, 2 grade 3 to 4
    pub encephalopathy: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChildPughParts {
    pub bilirubin: f64,
    pub albumin: f64,
    pub inr: f64,
    pub ascites: f64,
    pub encephalopathy: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct ChildPugh {
    pub points: f64,
    pub parts: ChildPughParts,
    pub class: &'static str,
    pub label: &'static str,
}

pub fn child_pugh(p: &ChildPughIn) -> ChildPugh {
    let b = if p.bilirubin < 2.0 {
        1.0
    } else if p.bilirubin <= 3.0 {
        2.0
    } else {
        3.0
    };
    let a = if p.albumin > 3.5 {
        1.0
    } else if p.albumin >= 2.8 {
        2.0
    } else {
        3.0
    };
    let i = if p.inr < 1.7 {
        1.0
    } else if p.inr <= 2.3 {
        2.0
    } else {
        3.0
    };
    let parts = ChildPughParts { bilirubin: b, albumin: a, inr: i, ascites: 1.0 + p.ascites, encephalopathy: 1.0 + p.encephalopathy };
    let points = b + a + i + parts.ascites + parts.encephalopathy;
    let (class, label) = if points <= 6.0 {
        ("A", "Mild (Child-Pugh A)")
    } else if points <= 9.0 {
        ("B", "Moderate (Child-Pugh B)")
    } else {
        ("C", "Severe (Child-Pugh C)")
    };
    ChildPugh { points, parts, class, label }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LiverIn {
    /// hepatic blood flow, L/h
    pub q: f64,
    /// plasma unbound fraction
    pub fu: f64,
    /// blood-to-plasma ratio
    pub bp: f64,
    /// unbound intrinsic clearance, L/h
    pub clint: f64,
    pub cl_renal: f64,
    pub fa: f64,
    pub fg: f64,
}
impl Default for LiverIn {
    fn default() -> Self {
        LiverIn { q: 90.0, fu: 0.1, bp: 1.0, clint: 100.0, cl_renal: 0.0, fa: 1.0, fg: 1.0 }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Mult {
    pub q: f64,
    pub clint: f64,
    pub fu: f64,
}
impl Default for Mult {
    fn default() -> Self {
        Mult { q: 1.0, clint: 1.0, fu: 1.0 }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiverState {
    pub q: f64,
    pub fu: f64,
    pub clint: f64,
    pub extraction: f64,
    pub clh: f64,
    pub cl: f64,
    pub f: f64,
    pub auc_iv: f64,
    pub auc_po: f64,
    pub auc_unbound_iv: f64,
    pub auc_unbound_po: f64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiverRatio {
    pub clh: f64,
    pub auc_iv: f64,
    pub auc_po: f64,
    pub auc_unbound_iv: f64,
    pub auc_unbound_po: f64,
    pub f: f64,
    pub cl: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Liver {
    pub base: LiverState,
    pub impaired: LiverState,
    pub ratio: LiverRatio,
    pub class: &'static str,
}

/// Well-stirred liver model with multipliers for the impaired liver.
pub fn well_stirred(p: &LiverIn, m: &Mult) -> Liver {
    let one = |q: f64, fu: f64, ci: f64| {
        let fub = (fu / p.bp).min(1.0);
        let clh = q * fub * ci / (q + fub * ci);
        let e = clh / q;
        let cl = clh + p.cl_renal;
        let f = p.fa * p.fg * (1.0 - e);
        LiverState { q, fu, clint: ci, extraction: e, clh, cl, f, auc_iv: 1.0 / cl, auc_po: f / cl, auc_unbound_iv: fu / cl, auc_unbound_po: fu * f / cl }
    };
    let base = one(p.q, p.fu, p.clint);
    let imp = one(p.q * m.q, (p.fu * m.fu).min(1.0), p.clint * m.clint);
    let ratio = LiverRatio {
        clh: imp.clh / base.clh,
        auc_iv: imp.auc_iv / base.auc_iv,
        auc_po: imp.auc_po / base.auc_po,
        auc_unbound_iv: imp.auc_unbound_iv / base.auc_unbound_iv,
        auc_unbound_po: imp.auc_unbound_po / base.auc_unbound_po,
        f: imp.f / base.f,
        cl: imp.cl / base.cl,
    };
    let class = if base.extraction < 0.3 {
        "low"
    } else if base.extraction > 0.7 {
        "high"
    } else {
        "intermediate"
    };
    Liver { base, impaired: imp, ratio, class }
}

// ------------------------------------------------------------------------------------------------ study analysis

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupIn {
    pub name: String,
    pub values: Vec<f64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupRow {
    pub name: String,
    pub n: usize,
    pub gm: f64,
    pub cv: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ref: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lo: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hi: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub df: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub se: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p: Option<f64>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Groups {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub rows: Vec<GroupRow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mse: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub df_pooled: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

/// Compare each group with the reference group on the log scale (pooled one-way analysis of variance, or Welch for each pair).
pub fn compare_groups(groups: &[GroupIn], reference: &str, alpha: f64, pooled: bool) -> Groups {
    let g: Vec<(String, Vec<f64>)> = groups
        .iter()
        .map(|x| (x.name.clone(), x.values.iter().filter(|v| **v > 0.0).map(|v| v.ln()).collect::<Vec<_>>()))
        .filter(|x| !x.1.is_empty())
        .collect();
    let Some(r) = g.iter().find(|x| x.0 == reference).filter(|x| x.1.len() >= 2) else {
        return Groups {
            ok: false,
            error: Some("The reference group needs at least two subjects with a value.".into()),
            rows: vec![],
            mse: None,
            df_pooled: None,
            confidence: None,
        };
    };
    let n_tot: usize = g.iter().map(|x| x.1.len()).sum();
    let k = g.len();
    let dfp = n_tot as f64 - k as f64;
    let sse: f64 = g.iter().map(|x| if x.1.len() > 1 { (x.1.len() as f64 - 1.0) * variance(&x.1) } else { 0.0 }).sum();
    let mse = if dfp > 0.0 { sse / dfp } else { f64::NAN };
    let rows = g
        .iter()
        .map(|x| {
            let n = x.1.len();
            let m = mean(&x.1);
            let sdv = if n > 1 { variance(&x.1).sqrt() } else { f64::NAN };
            let mut row = GroupRow {
                name: x.0.clone(),
                n,
                gm: m.exp(),
                cv: ((sdv * sdv).exp() - 1.0).sqrt(),
                is_ref: None,
                ratio: None,
                lo: None,
                hi: None,
                df: None,
                se: None,
                p: None,
            };
            if x.0 == reference {
                row.is_ref = Some(true);
                return row;
            }
            let d = m - mean(&r.1);
            let (se, df) = if pooled && dfp > 0.0 {
                ((mse * (1.0 / n as f64 + 1.0 / r.1.len() as f64)).sqrt(), dfp)
            } else {
                let v1 = variance(&x.1) / n as f64;
                let v2 = variance(&r.1) / r.1.len() as f64;
                ((v1 + v2).sqrt(), (v1 + v2).powi(2) / (v1 * v1 / (n as f64 - 1.0) + v2 * v2 / (r.1.len() as f64 - 1.0)))
            };
            let t = t_quantile(1.0 - alpha / 2.0, df);
            row.ratio = Some(d.exp());
            row.lo = Some((d - t * se).exp());
            row.hi = Some((d + t * se).exp());
            row.df = Some(df);
            row.se = Some(se);
            row.p = Some(2.0 * (1.0 - t_cdf((d / se).abs(), df)));
            row
        })
        .collect();
    Groups { ok: true, error: None, rows, mse: Some(mse), df_pooled: Some(dfp), confidence: Some(1.0 - alpha) }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RegressOpts {
    pub log_x: bool,
    #[serde(rename = "ref")]
    pub reference: f64,
    pub points: Vec<f64>,
    pub alpha: f64,
}
impl Default for RegressOpts {
    fn default() -> Self {
        RegressOpts { log_x: true, reference: 1.0, points: vec![], alpha: 0.1 }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct PredPoint {
    pub x: f64,
    pub ratio: f64,
    pub lo: f64,
    pub hi: f64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Regression {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub n: usize,
    pub slope: f64,
    pub se: f64,
    pub df: f64,
    pub p: f64,
    pub r2: f64,
    pub sigma: f64,
    pub intercept: f64,
    pub log_x: bool,
    #[serde(rename = "ref")]
    pub reference: f64,
    pub pred: Vec<PredPoint>,
    pub confidence: f64,
}

/// Regression of ln(parameter) on the covariate (ln scale, or linear) with the ratio to a reference covariate value.
pub fn regress_covariate(x: &[f64], y: &[f64], o: &RegressOpts) -> Result<Regression, String> {
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for i in 0..x.len().min(y.len()) {
        if x[i].is_finite() && y[i].is_finite() && y[i] > 0.0 && (!o.log_x || x[i] > 0.0) {
            xs.push(if o.log_x { x[i].ln() } else { x[i] });
            ys.push(y[i].ln());
        }
    }
    let n = xs.len();
    if n < 4 {
        return Err("At least four subjects with the covariate and the parameter are needed.".into());
    }
    let design: Vec<Vec<f64>> = xs.iter().map(|v| vec![1.0, *v]).collect();
    let f = ols(&design, &ys);
    let (slope, se, df) = (f.beta[1], f.se[1], f.df);
    let my = mean(&ys);
    let tot: f64 = ys.iter().map(|v| (v - my).powi(2)).sum();
    let r2 = if tot > 0.0 { 1.0 - f.rss / tot } else { f64::NAN };
    let t = t_quantile(1.0 - o.alpha / 2.0, df);
    let tx = |v: f64| if o.log_x { v.ln() } else { v };
    let x0 = tx(o.reference);
    let pred = o
        .points
        .iter()
        .map(|&p| {
            let dx = tx(p) - x0;
            let d = slope * dx;
            let s = dx.abs() * se;
            PredPoint { x: p, ratio: d.exp(), lo: (d - t * s).exp(), hi: (d + t * s).exp() }
        })
        .collect();
    Ok(Regression {
        ok: true,
        error: None,
        n,
        slope,
        se,
        df,
        p: 2.0 * (1.0 - t_cdf((slope / se).abs(), df)),
        r2,
        sigma: f.sigma2.sqrt(),
        intercept: f.beta[0],
        log_x: o.log_x,
        reference: o.reference,
        pred,
        confidence: 1.0 - o.alpha,
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct Advice {
    pub text: String,
    pub tone: &'static str,
    pub factor: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uncertain: Option<bool>,
}

/// A plain statement of what a group ratio means for the dose.
pub fn dose_advice(row: &GroupRow, bounds: [f64; 2]) -> Advice {
    let (Some(ratio), Some(lo), Some(hi)) = (row.ratio, row.lo, row.hi) else {
        return Advice { text: String::new(), tone: "neutral", factor: f64::NAN, uncertain: None };
    };
    if row.is_ref == Some(true) || !ratio.is_finite() {
        return Advice { text: String::new(), tone: "neutral", factor: f64::NAN, uncertain: None };
    }
    let inside = lo >= bounds[0] && hi <= bounds[1];
    let factor = 1.0 / ratio;
    if inside {
        return Advice { text: "No dose adjustment indicated: the whole interval is within the limits.".into(), tone: "ok", factor: 1.0, uncertain: None };
    }
    let exceeds = ratio > 1.0;
    Advice {
        text: format!(
            "{} (ratio {}). Matching exposure to the reference group needs a dose of about {} % of the usual dose; decide with the exposure-response and safety data.",
            if exceeds { "Exposure is higher" } else { "Exposure is lower" },
            crate::core::fixed(ratio, 2),
            crate::core::fixed(factor * 100.0, 0)
        ),
        tone: if exceeds { "warn" } else { "info" },
        factor,
        uncertain: Some(lo < bounds[0] && hi > bounds[1]),
    }
}
