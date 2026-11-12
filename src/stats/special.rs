//! Special functions: ln Gamma, incomplete gamma and beta, error function.

use std::f64::consts::PI;

/// ln Gamma(x), x > 0 (Lanczos, about 1e-14 relative).
pub fn lgamma(x: f64) -> f64 {
    if x < 0.5 {
        return (PI / (PI * x).sin().abs()).ln() - lgamma(1.0 - x);
    }
    let x = x - 1.0;
    const G: f64 = 7.0;
    const C: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    let mut a = C[0];
    let t = x + G + 0.5;
    for (i, c) in C.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    0.5 * (2.0 * PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

/// Regularized lower incomplete gamma P(a, x).
pub fn gamma_p(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if !x.is_finite() {
        return 1.0;
    }
    if x < a + 1.0 {
        let mut sum = 1.0 / a;
        let mut term = sum;
        for n in 1..1000 {
            term *= x / (a + n as f64);
            sum += term;
            if term.abs() < sum.abs() * 1e-16 {
                break;
            }
        }
        return sum * (-x + a * x.ln() - lgamma(a)).exp();
    }
    1.0 - gamma_q_cf(a, x)
}

fn gamma_q_cf(a: f64, x: f64) -> f64 {
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / 1e-300;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..1000 {
        let i = i as f64;
        let an = -i * (i - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < 1e-300 {
            d = 1e-300;
        }
        c = b + an / c;
        if c.abs() < 1e-300 {
            c = 1e-300;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-16 {
            break;
        }
    }
    (-x + a * x.ln() - lgamma(a)).exp() * h
}

/// Regularized upper incomplete gamma Q(a, x), accurate for small values.
pub fn gamma_q(a: f64, x: f64) -> f64 {
    if x <= 0.0 {
        return 1.0;
    }
    if x < a + 1.0 {
        return 1.0 - gamma_p(a, x);
    }
    gamma_q_cf(a, x)
}

fn beta_cf(x: f64, a: f64, b: f64) -> f64 {
    let mut c = 1.0;
    let mut d = 1.0 - ((a + b) * x) / (a + 1.0);
    if d.abs() < 1e-300 {
        d = 1e-300;
    }
    d = 1.0 / d;
    let mut h = d;
    for m in 1..2000 {
        let m = m as f64;
        let m2 = 2.0 * m;
        let mut an = (m * (b - m) * x) / ((a + m2 - 1.0) * (a + m2));
        d = 1.0 + an * d;
        if d.abs() < 1e-300 {
            d = 1e-300;
        }
        c = 1.0 + an / c;
        if c.abs() < 1e-300 {
            c = 1e-300;
        }
        d = 1.0 / d;
        h *= d * c;
        an = (-(a + m) * (a + b + m) * x) / ((a + m2) * (a + m2 + 1.0));
        d = 1.0 + an * d;
        if d.abs() < 1e-300 {
            d = 1e-300;
        }
        c = 1.0 + an / c;
        if c.abs() < 1e-300 {
            c = 1e-300;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < 1e-16 {
            break;
        }
    }
    h
}

/// Regularized incomplete beta I_x(a, b).
pub fn beta_i(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let lbeta = lgamma(a + b) - lgamma(a) - lgamma(b);
    let front = (lbeta + a * x.ln() + b * (1.0 - x).ln()).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        front * beta_cf(x, a, b) / a
    } else {
        1.0 - front * beta_cf(1.0 - x, b, a) / b
    }
}

pub fn erf(x: f64) -> f64 {
    if x == 0.0 {
        0.0
    } else if x > 0.0 {
        gamma_p(0.5, x * x)
    } else {
        -gamma_p(0.5, x * x)
    }
}
