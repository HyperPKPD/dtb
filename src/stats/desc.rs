//! Descriptive statistics. Non-finite values are the caller's concern, as in the JavaScript library.

use super::dist::norm_quantile;

pub fn sum(a: &[f64]) -> f64 {
    a.iter().sum()
}
pub fn mean(a: &[f64]) -> f64 {
    if a.is_empty() {
        f64::NAN
    } else {
        sum(a) / a.len() as f64
    }
}
pub fn variance(a: &[f64]) -> f64 {
    if a.len() < 2 {
        return f64::NAN;
    }
    let m = mean(a);
    a.iter().map(|v| (v - m) * (v - m)).sum::<f64>() / (a.len() as f64 - 1.0)
}
pub fn sd(a: &[f64]) -> f64 {
    variance(a).sqrt()
}
pub fn geomean(a: &[f64]) -> f64 {
    if a.is_empty() {
        f64::NAN
    } else {
        (a.iter().map(|v| v.ln()).sum::<f64>() / a.len() as f64).exp()
    }
}

/// Ascending copy (NaN sorts last, as a numeric typed-array sort does).
pub fn sorted(a: &[f64]) -> Vec<f64> {
    let mut s = a.to_vec();
    s.sort_by(|x, y| x.total_cmp(y));
    s
}

/// Quantile of a sorted slice, type 7 (the default of R and Excel's PERCENTILE.INC).
pub fn quantile_sorted(s: &[f64], p: f64) -> f64 {
    let n = s.len();
    if n == 0 {
        return f64::NAN;
    }
    let h = (n as f64 - 1.0) * p;
    let lo = h.floor() as usize;
    let hi = h.ceil() as usize;
    s[lo] + (h - lo as f64) * (s[hi] - s[lo])
}
pub fn quantile(a: &[f64], p: f64) -> f64 {
    quantile_sorted(&sorted(a), p)
}
pub fn median(a: &[f64]) -> f64 {
    quantile(a, 0.5)
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Summary {
    pub n: usize,
    pub mean: f64,
    pub sd: f64,
    pub min: f64,
    pub q1: f64,
    pub median: f64,
    pub q3: f64,
    pub max: f64,
}

pub fn summary(a: &[f64]) -> Summary {
    let x: Vec<f64> = a.iter().copied().filter(|v| v.is_finite()).collect();
    let s = sorted(&x);
    Summary {
        n: x.len(),
        mean: mean(&x),
        sd: sd(&x),
        min: s.first().copied().unwrap_or(f64::NAN),
        q1: quantile_sorted(&s, 0.25),
        median: quantile_sorted(&s, 0.5),
        q3: quantile_sorted(&s, 0.75),
        max: s.last().copied().unwrap_or(f64::NAN),
    }
}

/// Wilson score interval for a proportion.
pub fn wilson(k: f64, n: f64, conf: f64) -> (f64, f64) {
    if n == 0.0 {
        return (f64::NAN, f64::NAN);
    }
    let z = norm_quantile(1.0 - (1.0 - conf) / 2.0);
    let p = k / n;
    let d = 1.0 + z * z / n;
    let c = (p + z * z / (2.0 * n)) / d;
    let h = (z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt()) / d;
    ((c - h).max(0.0), (c + h).min(1.0))
}
