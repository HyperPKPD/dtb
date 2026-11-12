//! Distributions: normal, Student t, chi-square, F.

use super::special::*;
use std::f64::consts::PI;

pub fn norm_pdf(x: f64, m: f64, s: f64) -> f64 {
    (-0.5 * ((x - m) / s).powi(2)).exp() / (s * (2.0 * PI).sqrt())
}

/// Standard normal cdf, accurate in both tails.
pub fn norm_cdf(x: f64) -> f64 {
    let z = x;
    if z == 0.0 {
        return 0.5;
    }
    if z < -1.5 {
        return 0.5 * gamma_q(0.5, z * z / 2.0);
    }
    0.5 * (if z < 0.0 { 1.0 - gamma_p(0.5, z * z / 2.0) } else { 1.0 + gamma_p(0.5, z * z / 2.0) })
}

/// Standard normal quantile (Acklam's algorithm with two Halley steps).
pub fn norm_quantile(p: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    const A: [f64; 6] =
        [-3.969_683_028_665_376e1, 2.209_460_984_245_205e2, -2.759_285_104_469_687e2, 1.383_577_518_672_69e2, -3.066_479_806_614_716e1, 2.506_628_277_459_239];
    const B: [f64; 5] = [-5.447_609_879_822_406e1, 1.615_858_368_580_409e2, -1.556_989_798_598_866e2, 6.680_131_188_771_972e1, -1.328_068_155_288_572e1];
    const C: [f64; 6] =
        [-7.784_894_002_430_293e-3, -3.223_964_580_411_365e-1, -2.400_758_277_161_838, -2.549_732_539_343_734, 4.374_664_141_464_968, 2.938_163_982_698_783];
    const D: [f64; 4] = [7.784_695_709_041_462e-3, 3.224_671_290_700_398e-1, 2.445_134_137_142_996, 3.754_408_661_907_416];
    let mut x;
    if p < 0.02425 {
        let q = (-2.0 * p.ln()).sqrt();
        x = (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5]) / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0);
    } else if p > 1.0 - 0.02425 {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        x = -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5]) / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0);
    } else {
        let q = p - 0.5;
        let r = q * q;
        x = ((((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q) / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0);
    }
    for _ in 0..2 {
        let e = norm_cdf(x) - p;
        let u = e * (2.0 * PI).sqrt() * (x * x / 2.0).exp();
        x -= u / (1.0 + x * u / 2.0);
    }
    x
}

/// Student t cdf.
pub fn t_cdf(t: f64, df: f64) -> f64 {
    if !t.is_finite() {
        return if t > 0.0 { 1.0 } else { 0.0 };
    }
    if df.is_infinite() {
        return norm_cdf(t);
    }
    let x = df / (df + t * t);
    let ib = 0.5 * beta_i(x, df / 2.0, 0.5);
    if t > 0.0 {
        1.0 - ib
    } else {
        ib
    }
}

pub fn t_pdf(t: f64, df: f64) -> f64 {
    (lgamma((df + 1.0) / 2.0) - lgamma(df / 2.0)).exp() / (df * PI).sqrt() * (1.0 + t * t / df).powf(-(df + 1.0) / 2.0)
}

/// Student t quantile.
pub fn t_quantile(p: f64, df: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    if p == 0.5 {
        return 0.0;
    }
    if df.is_infinite() || df > 1e7 {
        return norm_quantile(p);
    }
    if df == 1.0 {
        return (PI * (p - 0.5)).tan();
    }
    if df == 2.0 {
        let a = 4.0 * p * (1.0 - p);
        return (2.0 * p - 1.0) * (2.0 / a).sqrt();
    }
    let upper = p > 0.5;
    let pp = if upper { 1.0 - p } else { p };
    let mut lo = -1.0f64;
    let mut hi = 0.0f64;
    while t_cdf(lo, df) > pp {
        hi = lo;
        lo *= 2.0;
        if lo < -1e12 {
            break;
        }
    }
    let mut x = norm_quantile(pp);
    if !(x > lo && x < hi) {
        x = (lo + hi) / 2.0;
    }
    for _ in 0..100 {
        let f = t_cdf(x, df) - pp;
        let g = t_pdf(x, df);
        let mut nx = if g > 0.0 { x - f / g } else { (lo + hi) / 2.0 };
        if f > 0.0 {
            hi = hi.min(x);
        } else {
            lo = lo.max(x);
        }
        if !(nx > lo && nx < hi) {
            nx = (lo + hi) / 2.0;
        }
        if (nx - x).abs() < 1e-13 * x.abs().max(1.0) {
            x = nx;
            break;
        }
        x = nx;
    }
    if upper {
        -x
    } else {
        x
    }
}

pub fn chi2_cdf(x: f64, df: f64) -> f64 {
    if x <= 0.0 {
        0.0
    } else {
        gamma_p(df / 2.0, x / 2.0)
    }
}

pub fn chi2_pdf(x: f64, df: f64) -> f64 {
    if x <= 0.0 {
        0.0
    } else {
        ((df / 2.0 - 1.0) * x.ln() - x / 2.0 - (df / 2.0) * std::f64::consts::LN_2 - lgamma(df / 2.0)).exp()
    }
}

/// Chi-square quantile.
pub fn chi2_quantile(p: f64, df: f64) -> f64 {
    if p <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    let mut lo = 0.0f64;
    let mut hi = df.max(1.0);
    while chi2_cdf(hi, df) < p {
        hi *= 2.0;
    }
    let mut x = (df * (1.0 - 2.0 / (9.0 * df) + norm_quantile(p) * (2.0 / (9.0 * df)).sqrt()).powi(3)).max(1e-12);
    if !(x > lo && x < hi) {
        x = (lo + hi) / 2.0;
    }
    for _ in 0..200 {
        let f = chi2_cdf(x, df) - p;
        let g = chi2_pdf(x, df);
        if f > 0.0 {
            hi = hi.min(x);
        } else {
            lo = lo.max(x);
        }
        let mut nx = if g > 0.0 { x - f / g } else { (lo + hi) / 2.0 };
        if !(nx > lo && nx < hi) {
            nx = (lo + hi) / 2.0;
        }
        if (nx - x).abs() < 1e-13 * x.max(1.0) {
            x = nx;
            break;
        }
        x = nx;
    }
    x
}

/// F distribution cdf.
pub fn f_cdf(x: f64, d1: f64, d2: f64) -> f64 {
    if !(x > 0.0) {
        return 0.0;
    }
    if !x.is_finite() {
        return 1.0;
    }
    beta_i(d1 * x / (d1 * x + d2), d1 / 2.0, d2 / 2.0)
}

/// F quantile by bisection on the cdf.
pub fn f_quantile(p: f64, d1: f64, d2: f64) -> f64 {
    if p <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    let mut hi = 1.0f64;
    while f_cdf(hi, d1, d2) < p && hi < 1e12 {
        hi *= 2.0;
    }
    super::num::bisect(|x| f_cdf(x, d1, d2) - p, 0.0, hi, 1e-13 * hi.max(1.0), 200)
}
