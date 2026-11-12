//! Exact distributions for the non-parametric Tmax analysis.

/// Distribution of the Mann-Whitney U statistic for sample sizes m and n (exact, by the Gaussian binomial recurrence).
pub fn mann_whitney_counts(m: usize, n: usize) -> Vec<f64> {
    let size = m * n + 1;
    let mut p = vec![0.0f64; size];
    p[0] = 1.0;
    for i in 1..=m {
        let mut q = vec![0.0f64; size];
        for k in 0..size {
            q[k] = p[k] - if k >= n + i { p[k - n - i] } else { 0.0 };
        }
        for k in i..size {
            q[k] += q[k - i];
        }
        p = q;
    }
    let tot: f64 = p.iter().sum();
    p.iter().map(|v| v / tot).collect()
}

/// Distribution of the Wilcoxon signed-rank statistic V for n pairs (exact).
pub fn signed_rank_counts(n: usize) -> Vec<f64> {
    let size = n * (n + 1) / 2 + 1;
    let mut c = vec![0.0f64; size];
    c[0] = 1.0;
    for k in 1..=n {
        let mut next = c.clone();
        for s in k..size {
            next[s] += c[s - k];
        }
        c = next;
    }
    let tot = 2f64.powi(n as i32);
    c.iter().map(|v| v / tot).collect()
}
