//! The analyses of one simulated trial. Every treatment effect is on a scale where a positive number is a benefit.

use crate::stats::{norm_cdf, t_cdf};

#[derive(Clone, Copy, Debug)]
pub struct Analysis {
    pub delta: f64,
    pub se: f64,
    pub df: f64,
    pub p: f64,
    pub events: Option<f64>,
}

fn upper_p(z: f64, df: f64) -> f64 {
    if df > 300.0 || !df.is_finite() {
        1.0 - norm_cdf(z)
    } else {
        1.0 - t_cdf(z, df)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Moments {
    pub n: f64,
    pub mean: f64,
    pub v: f64,
}

/// Welch comparison; `sign` is +1 when higher is better and -1 when lower is better.
pub fn welch(c: &Moments, a: &Moments, sign: f64) -> Analysis {
    let v1 = c.v / c.n;
    let v2 = a.v / a.n;
    let se = (v1 + v2).sqrt();
    let delta = sign * (a.mean - c.mean);
    if !(se > 0.0) {
        return Analysis { delta, se: f64::NAN, df: f64::NAN, p: if delta > 0.0 { 0.0 } else { 1.0 }, events: None };
    }
    let df = (v1 + v2).powi(2) / (v1 * v1 / (c.n - 1.0) + v2 * v2 / (a.n - 1.0));
    Analysis { delta, se, df, p: upper_p(delta / se, df), events: None }
}

/// Difference of two proportions with a Wald standard error (half-count correction where a proportion is 0 or 1).
pub fn prop_diff(cx: f64, cn: f64, ax: f64, an: f64) -> Analysis {
    let adj = |x: f64, n: f64| if x == 0.0 || x == n { (x + 0.5) / (n + 1.0) } else { x / n };
    let (pc, pa) = (cx / cn, ax / an);
    let (qc, qa) = (adj(cx, cn), adj(ax, an));
    let se = (qc * (1.0 - qc) / cn + qa * (1.0 - qa) / an).sqrt();
    let delta = pa - pc;
    Analysis { delta, se, df: f64::INFINITY, p: 1.0 - norm_cdf(delta / se), events: None }
}

/// Log-rank comparison, active against control, from (time, event) records. The effect is -log(HR) with the Peto estimate (O-E)/V.
pub fn log_rank(ct: &[f64], ce: &[u8], at: &[f64], ae: &[u8]) -> (Analysis, f64) {
    let (cn, an) = (ct.len(), at.len());
    let n = cn + an;
    let mut t = Vec::with_capacity(n);
    let mut e = Vec::with_capacity(n);
    let mut g = Vec::with_capacity(n);
    for i in 0..cn {
        t.push(ct[i]);
        e.push(ce[i]);
        g.push(0u8);
    }
    for i in 0..an {
        t.push(at[i]);
        e.push(ae[i]);
        g.push(1u8);
    }
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&x, &y| t[x].partial_cmp(&t[y]).unwrap_or(std::cmp::Ordering::Equal).then((e[y] as i32 - e[x] as i32).cmp(&0)));
    let (mut n_a, mut n_c) = (an as f64, cn as f64);
    let (mut o, mut ex, mut v, mut events) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut i = 0;
    while i < n {
        let tt = t[idx[i]];
        let (mut d, mut d_a, mut rem, mut rem_a) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        let mut j = i;
        while j < n && t[idx[j]] == tt {
            if e[idx[j]] != 0 {
                d += 1.0;
                if g[idx[j]] != 0 {
                    d_a += 1.0;
                }
            }
            rem += 1.0;
            if g[idx[j]] != 0 {
                rem_a += 1.0;
            }
            j += 1;
        }
        let nn = n_a + n_c;
        if d > 0.0 {
            events += d;
        }
        if d > 0.0 && nn > 1.0 {
            let f = n_a / nn;
            o += d_a;
            ex += d * f;
            v += d * f * (1.0 - f) * (nn - d) / (nn - 1.0);
        }
        n_a -= rem_a;
        n_c -= rem - rem_a;
        i = j;
    }
    if !(v > 0.0) {
        return (Analysis { delta: 0.0, se: f64::NAN, df: f64::INFINITY, p: 1.0, events: Some(events) }, 1.0);
    }
    let ln_hr = (o - ex) / v;
    let delta = -ln_hr;
    let se = 1.0 / v.sqrt();
    (Analysis { delta, se, df: f64::INFINITY, p: 1.0 - norm_cdf(delta / se), events: Some(events) }, ln_hr.exp())
}
