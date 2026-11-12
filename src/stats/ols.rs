//! Ordinary least squares by Gauss-Jordan sweeping with a pivot tolerance: a column that is a combination of earlier columns is
//! aliased (its coefficient is NaN, as lm() does in R), so over-parameterised designs are handled.

#[derive(Clone, Debug)]
pub struct Ols {
    pub beta: Vec<f64>,
    pub se: Vec<f64>,
    pub rss: f64,
    pub df: f64,
    pub rank: usize,
    pub n: usize,
    pub sigma2: f64,
    pub aliased: Vec<bool>,
}

pub fn ols(x: &[Vec<f64>], y: &[f64]) -> Ols {
    let n = x.len();
    let p = if n > 0 { x[0].len() } else { 0 };
    let w = p + 1;
    let mut a = vec![0.0f64; p * w];
    for i in 0..n {
        let r = &x[i];
        for ja in 0..p {
            let xa = r[ja];
            if xa == 0.0 {
                continue;
            }
            for jb in ja..p {
                a[ja * w + jb] += xa * r[jb];
            }
            a[ja * w + p] += xa * y[i];
        }
    }
    for ja in 0..p {
        for jb in 0..ja {
            a[ja * w + jb] = a[jb * w + ja];
        }
    }
    let yy: f64 = y.iter().map(|v| v * v).sum();
    let diag0: Vec<f64> = (0..p).map(|i| a[i * w + i]).collect();
    let mut aliased = vec![false; p];
    // sweep operator on [X'X X'y; . y'y]
    let mut t = vec![0.0f64; w * w];
    for i in 0..p {
        for j in 0..=p {
            t[i * w + j] = a[i * w + j];
        }
    }
    for j in 0..p {
        t[p * w + j] = a[j * w + p];
    }
    t[p * w + p] = yy;
    let mut rank = 0;
    for k in 0..p {
        let piv = t[k * w + k];
        if !(piv > 1e-9 * diag0[k].max(1e-300)) {
            aliased[k] = true;
            continue;
        }
        rank += 1;
        let d = 1.0 / piv;
        for j in 0..=p {
            if j != k {
                t[k * w + j] *= d;
            }
        }
        for i in 0..=p {
            if i == k {
                continue;
            }
            let f = t[i * w + k];
            if f == 0.0 {
                continue;
            }
            for j in 0..=p {
                if j != k {
                    t[i * w + j] -= f * t[k * w + j];
                }
            }
            t[i * w + k] = -f * d;
        }
        t[k * w + k] = d;
    }
    let rss = t[p * w + p].max(0.0);
    let df = n as f64 - rank as f64;
    let sigma2 = if df > 0.0 { rss / df } else { f64::NAN };
    let mut beta = vec![f64::NAN; p];
    let mut se = vec![f64::NAN; p];
    for k in 0..p {
        if aliased[k] {
            continue;
        }
        beta[k] = t[k * w + p];
        se[k] = (t[k * w + k] * sigma2).max(0.0).sqrt();
    }
    Ols { beta, se, rss, df, rank, n, sigma2, aliased }
}
