//! The design of a trial: its shape, defaults, checks and scenario overrides. A design is plain JSON.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Arm {
    pub id: String,
    pub name: String,
    pub dose: f64,
    pub n: f64,
    pub control: bool,
}
impl Default for Arm {
    fn default() -> Self {
        Arm { id: String::new(), name: String::new(), dose: 0.0, n: 50.0, control: false }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PkParams {
    pub model: String,
    #[serde(rename = "CL")]
    pub cl: f64,
    #[serde(rename = "V")]
    pub v: f64,
    pub ka: f64,
    #[serde(rename = "F")]
    pub f: f64,
    #[serde(rename = "cvCL")]
    pub cv_cl: f64,
    #[serde(rename = "cvV")]
    pub cv_v: f64,
    #[serde(rename = "cvKa")]
    pub cv_ka: f64,
    pub conc_factor: f64,
}
impl Default for PkParams {
    fn default() -> Self {
        PkParams { model: "oral".into(), cl: 5.0, v: 50.0, ka: 1.0, f: 1.0, cv_cl: 0.3, cv_v: 0.25, cv_ka: 0.4, conc_factor: 1000.0 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Adherence {
    pub mean: f64,
    pub sd: f64,
}
impl Default for Adherence {
    fn default() -> Self {
        Adherence { mean: 0.9, sd: 0.12 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Dropout {
    pub rate_per_week: f64,
    pub dose_slope: f64,
}
impl Default for Dropout {
    fn default() -> Self {
        Dropout { rate_per_week: 0.012, dose_slope: 0.0 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Endpoint {
    #[serde(rename = "type")]
    pub kind: String,
    pub metric: String,
    pub benefit: String,
    pub label: String,
    pub unit: String,
}
impl Default for Endpoint {
    fn default() -> Self {
        Endpoint {
            kind: "continuous".into(),
            metric: "cavg".into(),
            benefit: "lower".into(),
            label: "Change in systolic blood pressure".into(),
            unit: "mmHg".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ErContinuous {
    pub placebo: f64,
    pub emax: f64,
    pub ec50: f64,
    pub hill: f64,
    pub sd: f64,
}
impl Default for ErContinuous {
    fn default() -> Self {
        ErContinuous { placebo: -3.0, emax: -10.0, ec50: 500.0, hill: 1.0, sd: 12.0 }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ErBinary {
    pub baseline: f64,
    pub emax: f64,
    pub ec50: f64,
    pub hill: f64,
}
impl Default for ErBinary {
    fn default() -> Self {
        ErBinary { baseline: 0.25, emax: 1.8, ec50: 250.0, hill: 1.0 }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ErTte {
    pub hazard0: f64,
    pub imax: f64,
    pub ec50: f64,
    pub hill: f64,
}
impl Default for ErTte {
    fn default() -> Self {
        ErTte { hazard0: 0.02, imax: 0.5, ec50: 250.0, hill: 1.0 }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Er {
    pub continuous: ErContinuous,
    pub binary: ErBinary,
    pub tte: ErTte,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Uncertainty {
    pub enabled: bool,
    #[serde(rename = "sdLogEC50")]
    pub sd_log_e_c50: f64,
    pub rel_sd_emax: f64,
    #[serde(rename = "sdLogCL")]
    pub sd_log_c_l: f64,
}
impl Default for Uncertainty {
    fn default() -> Self {
        Uncertainty { enabled: false, sd_log_e_c50: 0.4, rel_sd_emax: 0.25, sd_log_c_l: 0.15 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Decision {
    pub kind: String,
    pub alpha: f64,
    pub mcid: f64,
    pub go_threshold: f64,
    pub go_prob: f64,
    pub nogo_threshold: f64,
    pub nogo_prob: f64,
}
impl Default for Decision {
    fn default() -> Self {
        Decision { kind: "lrv-tv".into(), alpha: 0.025, mcid: 3.0, go_threshold: 2.0, go_prob: 0.8, nogo_threshold: 5.0, nogo_prob: 0.2 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Design {
    pub name: String,
    pub seed: u64,
    pub arms: Vec<Arm>,
    pub tau: f64,
    pub duration_weeks: f64,
    pub pk: PkParams,
    pub adherence: Adherence,
    pub dropout: Dropout,
    pub dropout_handling: String,
    pub endpoint: Endpoint,
    pub er: Er,
    pub uncertainty: Uncertainty,
    pub decision: Decision,
}

impl Default for Design {
    fn default() -> Self {
        let arm = |id: &str, name: &str, dose: f64, control: bool| Arm { id: id.into(), name: name.into(), dose, n: 50.0, control };
        Design {
            name: "New trial".into(),
            seed: 20260101,
            arms: vec![arm("a1", "Placebo", 0.0, true), arm("a2", "50 mg once daily", 50.0, false), arm("a3", "100 mg once daily", 100.0, false)],
            tau: 24.0,
            duration_weeks: 12.0,
            pk: PkParams::default(),
            adherence: Adherence::default(),
            dropout: Dropout::default(),
            dropout_handling: "completers".into(),
            endpoint: Endpoint::default(),
            er: Er::default(),
            uncertainty: Uncertainty::default(),
            decision: Decision::default(),
        }
    }
}

/// The exposure-response parameters of the active endpoint on one footing.
#[derive(Clone, Copy, Debug, Default)]
pub struct ErView {
    pub ec50: f64,
    pub hill: f64,
    pub placebo: f64,
    pub emax: f64,
    pub sd: f64,
    pub baseline: f64,
    pub hazard0: f64,
    pub imax: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Continuous,
    Binary,
    Tte,
}

impl Design {
    pub fn kind(&self) -> Result<Kind, String> {
        match self.endpoint.kind.as_str() {
            "continuous" => Ok(Kind::Continuous),
            "binary" => Ok(Kind::Binary),
            "tte" => Ok(Kind::Tte),
            other => Err(format!("Unknown endpoint type \"{other}\" (use continuous, binary or tte).")),
        }
    }

    pub fn er_view(&self) -> ErView {
        match self.kind().unwrap_or(Kind::Continuous) {
            Kind::Continuous => {
                let e = &self.er.continuous;
                ErView { ec50: e.ec50, hill: e.hill, placebo: e.placebo, emax: e.emax, sd: e.sd, ..Default::default() }
            }
            Kind::Binary => {
                let e = &self.er.binary;
                ErView { ec50: e.ec50, hill: e.hill, baseline: e.baseline, emax: e.emax, ..Default::default() }
            }
            Kind::Tte => {
                let e = &self.er.tte;
                ErView { ec50: e.ec50, hill: e.hill, hazard0: e.hazard0, imax: e.imax, ..Default::default() }
            }
        }
    }

    pub fn n_doses(&self) -> i64 {
        ((self.duration_weeks * 168.0) / self.tau).floor() as i64
    }

    /// Thresholds are typed in natural units and used on a scale where positive is better.
    pub fn to_benefit(&self, x: f64) -> f64 {
        if self.endpoint.kind == "tte" {
            -x.ln()
        } else {
            x
        }
    }

    pub fn total_n(&self) -> f64 {
        self.arms.iter().map(|a| a.n).sum()
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Validation {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

fn num(v: f64) -> bool {
    v.is_finite()
}

/// Plain-language problems with a design; an empty list of errors means it can be simulated.
pub fn validate_design(d: &Design) -> Validation {
    let mut e: Vec<String> = Vec::new();
    let mut w: Vec<String> = Vec::new();
    if d.arms.len() < 2 {
        e.push("The trial needs at least two arms.".into());
    } else {
        if d.arms.iter().filter(|a| a.control).count() != 1 {
            e.push("Mark exactly one arm as the control arm.".into());
        }
        for (i, a) in d.arms.iter().enumerate() {
            if !(a.n >= 2.0 && a.n.fract() == 0.0) {
                e.push(format!("Arm {} ({}): the number of subjects must be a whole number of at least 2.", i + 1, a.name));
            }
            if !(a.dose >= 0.0) {
                e.push(format!("Arm {} ({}): the dose cannot be negative.", i + 1, a.name));
            }
        }
        if let Some(c) = d.arms.iter().find(|a| a.control) {
            if c.dose > 0.0 {
                w.push("The control arm has a dose above 0: it is simulated as an active arm and the treatment effect is measured against it.".into());
            }
        }
        if !d.arms.iter().any(|a| !a.control && a.dose > 0.0) {
            e.push("At least one active arm needs a dose above 0.".into());
        }
        let names: std::collections::HashSet<&str> = d.arms.iter().map(|a| a.name.as_str()).collect();
        if names.len() != d.arms.len() {
            w.push("Two arms have the same name.".into());
        }
    }
    if !(d.tau > 0.0) {
        e.push("The dosing interval must be above 0 hours.".into());
    }
    if !(d.duration_weeks > 0.0) {
        e.push("The treatment duration must be above 0 weeks.".into());
    } else if d.tau > 0.0 && ((d.duration_weeks * 168.0) / d.tau).floor() < 1.0 {
        e.push("The duration is shorter than one dosing interval.".into());
    }
    let p = &d.pk;
    for (k, v) in [("CL", p.cl), ("V", p.v), ("ka", p.ka)] {
        if !(v > 0.0) {
            e.push(format!("PK: {k} must be above 0."));
        }
    }
    if !(p.f > 0.0 && p.f <= 1.0001) {
        e.push("PK: bioavailability must be between 0 and 1.".into());
    }
    for (k, v) in [("cvCL", p.cv_cl), ("cvV", p.cv_v), ("cvKa", p.cv_ka)] {
        if !(v >= 0.0) {
            e.push(format!("PK: {k} cannot be negative."));
        }
    }
    if !(d.adherence.mean > 0.0 && d.adherence.mean <= 1.0) {
        e.push("Adherence: the mean must be above 0 and at most 1.".into());
    }
    if !(d.adherence.sd >= 0.0) {
        e.push("Adherence: the spread cannot be negative.".into());
    } else if d.adherence.mean < 1.0 && d.adherence.sd * d.adherence.sd >= d.adherence.mean * (1.0 - d.adherence.mean) {
        w.push("Adherence: the spread is too large for this mean; it is reduced to the largest possible value.".into());
    }
    if !(d.dropout.rate_per_week >= 0.0) {
        e.push("Dropout: the rate cannot be negative.".into());
    }
    if !matches!(d.dropout_handling.as_str(), "completers" | "all") {
        w.push(format!("Dropout handling \"{}\" is not recognised; subjects who drop out are counted as no effect.", d.dropout_handling));
    }
    match d.kind() {
        Err(m) => e.push(m),
        Ok(k) => {
            let er = d.er_view();
            if !(er.ec50 > 0.0) {
                e.push("Exposure-response: EC50 must be above 0.".into());
            }
            if !(er.hill > 0.0) {
                e.push("Exposure-response: the Hill coefficient must be above 0.".into());
            }
            match k {
                Kind::Continuous => {
                    if !(er.sd > 0.0) {
                        e.push("Exposure-response: the standard deviation must be above 0.".into());
                    }
                    if !num(er.emax) || !num(er.placebo) {
                        e.push("Exposure-response: Emax and the placebo change must be numbers.".into());
                    }
                    if (d.endpoint.benefit == "lower" && er.emax > 0.0) || (d.endpoint.benefit == "higher" && er.emax < 0.0) {
                        w.push("The sign of Emax is the opposite of the benefit direction: the drug would make the endpoint worse.".into());
                    }
                }
                Kind::Binary => {
                    if !(er.baseline > 0.0 && er.baseline < 1.0) {
                        e.push("Exposure-response: the placebo response rate must be between 0 and 1.".into());
                    }
                    if !num(er.emax) {
                        e.push("Exposure-response: Emax (log odds) must be a number.".into());
                    }
                }
                Kind::Tte => {
                    if !(er.hazard0 > 0.0) {
                        e.push("Exposure-response: the baseline hazard must be above 0.".into());
                    }
                    if !(er.imax >= 0.0 && er.imax < 1.0) {
                        e.push("Exposure-response: Imax must be from 0 up to (not including) 1.".into());
                    }
                }
            }
            let dc = &d.decision;
            if !(dc.alpha > 0.0 && dc.alpha < 0.5) {
                e.push("Decision: the one-sided significance level must be between 0 and 0.5.".into());
            }
            if dc.kind == "lrv-tv" {
                if !(dc.go_prob > 0.0 && dc.go_prob < 1.0) || !(dc.nogo_prob > 0.0 && dc.nogo_prob < 1.0) {
                    e.push("Decision: the probabilities must be between 0 and 1.".into());
                }
                if k == Kind::Tte && (!(dc.go_threshold > 0.0) || !(dc.nogo_threshold > 0.0)) {
                    e.push("Decision: thresholds for a time-to-event endpoint are hazard ratios, above 0.".into());
                }
                if k == Kind::Tte && dc.go_threshold < dc.nogo_threshold {
                    w.push("Decision: the Go threshold asks for less effect (a higher hazard ratio) than the No-Go threshold.".into());
                }
                if k != Kind::Tte && dc.go_threshold > dc.nogo_threshold {
                    w.push("Decision: the Go threshold is above the No-Go threshold; usually the lower reference value (LRV) is the Go threshold and the target value (TV) the No-Go threshold.".into());
                }
            } else if !matches!(dc.kind.as_str(), "significance" | "mcid") {
                e.push(format!("Decision: unknown rule \"{}\" (use significance, mcid or lrv-tv).", dc.kind));
            }
            if dc.kind == "mcid" && k == Kind::Tte && !(dc.mcid > 0.0) {
                e.push("Decision: the minimum effect for a time-to-event endpoint is a hazard ratio above 0.".into());
            }
        }
    }
    Validation { errors: e, warnings: w }
}

/// A named change of the base design; only the fields present are applied.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Overrides {
    pub n: Option<f64>,
    pub n_total: Option<f64>,
    pub dose_scale: Option<f64>,
    pub doses: Option<BTreeMap<String, f64>>,
    pub dropout: Option<f64>,
    pub adherence: Option<f64>,
    pub duration_weeks: Option<f64>,
    pub ec50_scale: Option<f64>,
    pub effect_scale: Option<f64>,
    pub placebo_shift: Option<f64>,
    pub sd_scale: Option<f64>,
    pub cl_scale: Option<f64>,
    pub uncertainty: Option<bool>,
}

fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

pub fn apply_overrides(base: &Design, o: &Overrides) -> Design {
    let mut d = base.clone();
    if let Some(n) = o.n {
        for a in &mut d.arms {
            a.n = js_round(n).max(2.0);
        }
    }
    if let Some(nt) = o.n_total {
        let k = d.arms.len() as f64;
        for a in &mut d.arms {
            a.n = js_round(nt / k).max(2.0);
        }
    }
    if let Some(s) = o.dose_scale {
        for a in &mut d.arms {
            if !a.control {
                a.dose *= s;
            }
        }
    }
    if let Some(m) = &o.doses {
        for a in &mut d.arms {
            if let Some(v) = m.get(&a.id) {
                a.dose = *v;
            }
        }
    }
    if let Some(v) = o.dropout {
        d.dropout.rate_per_week = v;
    }
    if let Some(v) = o.adherence {
        d.adherence.mean = v;
    }
    if let Some(v) = o.duration_weeks {
        d.duration_weeks = v;
    }
    let kind = d.kind().unwrap_or(Kind::Continuous);
    if let Some(s) = o.ec50_scale {
        match kind {
            Kind::Continuous => d.er.continuous.ec50 *= s,
            Kind::Binary => d.er.binary.ec50 *= s,
            Kind::Tte => d.er.tte.ec50 *= s,
        }
    }
    if let Some(s) = o.effect_scale {
        match kind {
            Kind::Tte => d.er.tte.imax = (d.er.tte.imax * s).min(0.99),
            Kind::Binary => d.er.binary.emax *= s,
            Kind::Continuous => d.er.continuous.emax *= s,
        }
    }
    if let (Some(s), Kind::Continuous) = (o.placebo_shift, kind) {
        d.er.continuous.placebo += s;
    }
    if let (Some(s), Kind::Continuous) = (o.sd_scale, kind) {
        d.er.continuous.sd *= s;
    }
    if let Some(s) = o.cl_scale {
        d.pk.cl *= s;
    }
    if let Some(u) = o.uncertainty {
        d.uncertainty.enabled = u;
    }
    d
}
