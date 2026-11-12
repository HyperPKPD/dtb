//! Numerical integration and root finding.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;

type Nodes = (Vec<f64>, Vec<f64>);

fn cache() -> &'static Mutex<HashMap<usize, Nodes>> {
    static C: OnceLock<Mutex<HashMap<usize, Nodes>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Gauss-Legendre nodes and weights on [-1, 1].
pub fn gauss_legendre(n: usize) -> Nodes {
    if let Some(v) = cache().lock().unwrap().get(&n) {
        return v.clone();
    }
    let mut x = vec![0.0; n];
    let mut w = vec![0.0; n];
    for i in 0..n.div_ceil(2) {
        let mut z = (std::f64::consts::PI * (i as f64 + 0.75) / (n as f64 + 0.5)).cos();
        let mut pp = 0.0;
        for _ in 0..100 {
            let mut p1 = 1.0;
            let mut p2 = 0.0;
            for j in 0..n {
                let p3 = p2;
                p2 = p1;
                p1 = ((2 * j + 1) as f64 * z * p2 - j as f64 * p3) / (j as f64 + 1.0);
            }
            pp = n as f64 * (z * p1 - p2) / (z * z - 1.0);
            let dz = p1 / pp;
            z -= dz;
            if dz.abs() < 1e-15 {
                break;
            }
        }
        x[i] = -z;
        x[n - 1 - i] = z;
        w[i] = 2.0 / ((1.0 - z * z) * pp * pp);
        w[n - 1 - i] = w[i];
    }
    cache().lock().unwrap().insert(n, (x.clone(), w.clone()));
    (x, w)
}

/// Integral of f over [a, b], composite Gauss-Legendre (`panels` x `n` points).
pub fn integrate<F: Fn(f64) -> f64>(f: F, a: f64, b: f64, panels: usize, n: usize) -> f64 {
    let (x, w) = gauss_legendre(n);
    let h = (b - a) / panels as f64;
    let mut s = 0.0;
    for p in 0..panels {
        let lo = a + p as f64 * h;
        let c = lo + h / 2.0;
        for i in 0..n {
            s += w[i] * f(c + (h / 2.0) * x[i]);
        }
    }
    s * (h / 2.0)
}

/// Root of f in [lo, hi] by bisection (f(lo) and f(hi) of opposite sign); NaN when there is no sign change.
pub fn bisect<F: Fn(f64) -> f64>(f: F, mut lo: f64, mut hi: f64, tol: f64, max_iter: usize) -> f64 {
    let mut flo = f(lo);
    let fhi = f(hi);
    if flo == 0.0 {
        return lo;
    }
    if fhi == 0.0 {
        return hi;
    }
    if flo * fhi > 0.0 {
        return f64::NAN;
    }
    for _ in 0..max_iter {
        let mid = (lo + hi) / 2.0;
        let fm = f(mid);
        if fm == 0.0 || (hi - lo) / 2.0 < tol {
            return mid;
        }
        if flo * fm < 0.0 {
            hi = mid;
        } else {
            lo = mid;
            flo = fm;
        }
    }
    (lo + hi) / 2.0
}
