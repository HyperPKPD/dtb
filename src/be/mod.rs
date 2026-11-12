//! Bioequivalence statistics on non-compartmental parameters.
//!
//! * ABE: fixed-effects model of ln(parameter) with subject, period and treatment (any crossover, replicate or incomplete design),
//!   90 % confidence interval of the geometric mean ratio against fixed limits; parallel designs use the Welch or pooled interval.
//! * ABEL: EMA reference-scaled limits (k = 0.760, switch at CVwR 30 %, cap at 50 %) with the point-estimate condition.
//! * RSABE: FDA reference-scaled average bioequivalence (Howe's upper bound of the linearised criterion).
//! * NTID: FDA narrow-therapeutic-index criteria (scaled bound, 80-125 % interval, within-subject variance ratio).
//! * NP: Hodges-Lehmann estimate and confidence interval for Tmax (2x2x2: Hauschke; other crossovers: paired signed-rank).
//! * Power and sample size: exact for ABE, simulated for the scaled methods.

pub mod analyze;
pub mod design;
pub mod fit;
pub mod nonpar;
pub mod power;
pub mod records;

pub use analyze::{abel_limits, analyze_be, howe_bound, residual_table, within_variance, BeOptions, BeResult};
pub use design::{detect_design, DesignInfo};
pub use records::{records_from_table, Per, Record, RecordMap, Records};

pub const LN125: f64 = 0.223_143_551_314_209_76;

/// FDA constants.
pub mod fda {
    pub const SW0: f64 = 0.25;
    pub const SW_SWITCH: f64 = 0.294;
    pub fn theta() -> f64 {
        (super::LN125 / SW0).powi(2)
    }
    pub fn nti_theta() -> f64 {
        ((1.11f64).ln() / 0.1).powi(2)
    }
}

/// EMA constants.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Ema {
    pub k: f64,
    pub cv_switch: f64,
    pub cv_cap: f64,
    pub pe_limits: [f64; 2],
}
impl Default for Ema {
    fn default() -> Self {
        Ema { k: 0.76, cv_switch: 0.3, cv_cap: 0.5, pe_limits: [0.8, 1.25] }
    }
}

pub fn cv_to_sd(cv: f64) -> f64 {
    (1.0 + cv * cv).ln().sqrt()
}
pub fn sd_to_cv(s: f64) -> f64 {
    ((s * s).exp() - 1.0).sqrt()
}
