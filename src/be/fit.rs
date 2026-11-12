//! Fixed-effects least squares of y on subject, period and treatment (treatment coded 1 for the test product).

use super::records::{cmp_per, Per};
use crate::stats::{ols, Ols};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Obs {
    pub s: String,
    pub p: Per,
    pub t: f64,
    pub y: f64,
    pub seq: String,
    pub raw: f64,
}

pub struct Fit {
    pub ols: Ols,
    pub trt: Option<usize>,
}

pub fn uniq_in_order<'a, I: Iterator<Item = &'a str>>(it: I) -> Vec<&'a str> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for s in it {
        if seen.insert(s) {
            out.push(s);
        }
    }
    out
}

pub fn fit_fixed(obs: &[Obs], period: bool, treatment: bool) -> Fit {
    let subs = uniq_in_order(obs.iter().map(|r| r.s.as_str()));
    let si: HashMap<&str, usize> = subs.iter().enumerate().map(|(i, s)| (*s, i)).collect();
    let mut pers: Vec<Per> = Vec::new();
    for r in obs {
        if !pers.iter().any(|p| p.key() == r.p.key()) {
            pers.push(r.p.clone());
        }
    }
    pers.sort_by(cmp_per);
    let pi: HashMap<String, usize> = pers.iter().enumerate().map(|(i, p)| (p.key(), i)).collect();
    let use_per = period && pers.len() > 1;
    let n_s = subs.len();
    let n_p = if use_per { pers.len() - 1 } else { 0 };
    let p = n_s + n_p + usize::from(treatment);
    let x: Vec<Vec<f64>> = obs
        .iter()
        .map(|r| {
            let mut row = vec![0.0; p];
            row[si[r.s.as_str()]] = 1.0;
            if use_per {
                let k = pi[&r.p.key()];
                if k > 0 {
                    row[n_s + k - 1] = 1.0;
                }
            }
            if treatment {
                row[p - 1] = r.t;
            }
            row
        })
        .collect();
    let y: Vec<f64> = obs.iter().map(|r| r.y).collect();
    Fit { ols: ols(&x, &y), trt: if treatment { Some(p - 1) } else { None } }
}
