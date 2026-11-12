//! One-compartment pharmacokinetics with first-order absorption (or an intravenous bolus), for repeated doses that may be missed.
//! Amounts in the gut and the central compartment are carried from dose to dose with closed-form steps, so the cost is proportional to
//! the number of doses. Units: mg, L/h, L, h; concentration = amount / V x concFactor (1000 gives ng/mL).

#[derive(Clone, Copy, Debug)]
pub struct Rates {
    pub ke: f64,
    pub ka: f64,
    pub c: f64,
}

/// Rate constants of a subject; ka is nudged when it equals ke (the formulas divide by ka - ke).
pub fn rates(cl: f64, v: f64, ka: f64) -> Rates {
    let ke = cl / v;
    let mut k = ka;
    if (k - ke).abs() < 1e-7 * ke {
        k = ke * (1.0 + 1e-6);
    }
    Rates { ke, ka: k, c: k / (k - ke) }
}

#[derive(Clone, Copy, Debug)]
pub struct Subject<'a> {
    pub dose: f64,
    pub f: f64,
    pub cl: f64,
    pub v: f64,
    pub ka: f64,
    pub tau: f64,
    pub n: usize,
    /// `taken[k]` is 1 when dose k was taken (None: all taken).
    pub taken: Option<&'a [u8]>,
    pub iv: bool,
    pub conc_factor: f64,
}

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct Exposure {
    pub auc: f64,
    pub cavg: f64,
    pub cmax: f64,
    pub cmin: f64,
}

impl Exposure {
    pub fn get(&self, metric: &str) -> f64 {
        match metric {
            "auc" => self.auc,
            "cmax" => self.cmax,
            "cmin" => self.cmin,
            _ => self.cavg,
        }
    }
}

#[inline]
fn took(taken: Option<&[u8]>, k: usize) -> bool {
    taken.is_none_or(|t| t[k] != 0)
}

/// Exposure over the last dosing interval.
pub fn exposure(s: &Subject) -> Exposure {
    if !(s.dose > 0.0) || s.n < 1 {
        return Exposure::default();
    }
    let r = rates(s.cl, s.v, s.ka);
    let (ke, ka) = (r.ke, r.ka);
    let ex_ka = (-ka * s.tau).exp();
    let ex_ke = (-ke * s.tau).exp();
    let (mut g, mut c) = (0.0f64, 0.0f64);
    let amt = s.dose * s.f;
    for k in 0..s.n {
        if took(s.taken, k) {
            if s.iv {
                c += amt
            } else {
                g += amt
            }
        }
        if k < s.n - 1 {
            if s.iv {
                c *= ex_ke;
            } else {
                let ng = g * ex_ka;
                let nc = c * ex_ke + g * r.c * (ex_ke - ex_ka);
                g = ng;
                c = nc;
            }
        }
    }
    let cf = s.conc_factor / s.v;
    let mut auc = if s.iv { c * (1.0 - ex_ke) / ke } else { c * (1.0 - ex_ke) / ke + g * r.c * ((1.0 - ex_ke) / ke - (1.0 - ex_ka) / ka) };
    auc *= cf;
    let (mut cmax, mut cmin) = (0.0f64, f64::INFINITY);
    const G: usize = 24;
    for i in 0..=G {
        let x = s.tau * i as f64 / G as f64;
        let e = (-ke * x).exp();
        let v = if s.iv { c * e } else { c * e + g * r.c * (e - (-ka * x).exp()) };
        let cc = v * cf;
        if cc > cmax {
            cmax = cc;
        }
        if cc < cmin {
            cmin = cc;
        }
    }
    Exposure { auc, cavg: auc / s.tau, cmax, cmin }
}

/// Concentration at the given ascending times (hours from the first dose).
pub fn profile(s: &Subject, times: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; times.len()];
    if !(s.dose > 0.0) || s.n < 1 {
        return out;
    }
    let r = rates(s.cl, s.v, s.ka);
    let (ke, ka) = (r.ke, r.ka);
    let cf = s.conc_factor / s.v;
    let amt = s.dose * s.f;
    let (mut g, mut c) = (0.0f64, 0.0f64);
    let mut ti = 0usize;
    for k in 0..=s.n {
        let t0 = k as f64 * s.tau;
        let t1 = if k < s.n { (k + 1) as f64 * s.tau } else { f64::INFINITY };
        if k < s.n && took(s.taken, k) {
            if s.iv {
                c += amt
            } else {
                g += amt
            }
        }
        while ti < times.len() && times[ti] < t1 {
            let x = times[ti] - t0;
            if x >= 0.0 {
                let e = (-ke * x).exp();
                out[ti] = (if s.iv { c * e } else { c * e + g * r.c * (e - (-ka * x).exp()) }) * cf;
            }
            ti += 1;
        }
        if ti >= times.len() {
            break;
        }
        if k < s.n {
            let ex_ke = (-ke * s.tau).exp();
            if s.iv {
                c *= ex_ke;
            } else {
                let ng = g * (-ka * s.tau).exp();
                let nc = c * ex_ke + g * r.c * (ex_ke - (-ka * s.tau).exp());
                g = ng;
                c = nc;
            }
        }
    }
    out
}

/// E = C^h / (EC50^h + C^h)
pub fn hill(c: f64, ec50: f64, h: f64) -> f64 {
    if !(c > 0.0) {
        return 0.0;
    }
    let a = c.powf(h);
    let b = ec50.powf(h);
    a / (a + b)
}
