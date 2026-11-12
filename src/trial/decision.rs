//! Decision rules: from the estimate of a treatment effect and its standard error to Go, Consider or No-Go.

use super::analysis::Analysis;
use super::design::Design;
use crate::stats::{norm_cdf, t_cdf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    Go,
    Consider,
    NoGo,
}

#[derive(Clone, Copy, Debug)]
pub struct Decision {
    pub sig: bool,
    pub call: Call,
    pub p_go: Option<f64>,
    pub p_nogo: Option<f64>,
}

fn cdf(z: f64, df: f64) -> f64 {
    if df > 300.0 || !df.is_finite() {
        norm_cdf(z)
    } else {
        t_cdf(z, df)
    }
}

/// Probability that the true effect exceeds `x` given the estimate (flat prior).
pub fn prob_above(a: &Analysis, x: f64) -> f64 {
    if a.se > 0.0 {
        cdf((a.delta - x) / a.se, a.df)
    } else if a.delta > x {
        1.0
    } else {
        0.0
    }
}

pub fn decide(d: &Design, a: &Analysis) -> Decision {
    let dc = &d.decision;
    let sig = a.p <= dc.alpha;
    match dc.kind.as_str() {
        "significance" => Decision { sig, call: if sig { Call::Go } else { Call::NoGo }, p_go: None, p_nogo: None },
        "mcid" => {
            let m = d.to_benefit(dc.mcid);
            Decision {
                sig,
                call: if sig {
                    if a.delta >= m {
                        Call::Go
                    } else {
                        Call::Consider
                    }
                } else {
                    Call::NoGo
                },
                p_go: None,
                p_nogo: None,
            }
        }
        _ => {
            let p_go = prob_above(a, d.to_benefit(dc.go_threshold));
            let p_no = prob_above(a, d.to_benefit(dc.nogo_threshold));
            let call = if p_go >= dc.go_prob {
                Call::Go
            } else if p_no < dc.nogo_prob {
                Call::NoGo
            } else {
                Call::Consider
            };
            Decision { sig, call, p_go: Some(p_go), p_nogo: Some(p_no) }
        }
    }
}

/// The decision rule in words, for reports.
pub fn describe_rule(d: &Design) -> String {
    let dc = &d.decision;
    let t = d.endpoint.kind.as_str();
    let unit = if t == "continuous" { format!(" {}", d.endpoint.unit).trim_end().to_string() } else { String::new() };
    let th = |x: f64| match t {
        "tte" => format!("a hazard ratio of {x}"),
        "binary" => format!("a difference of {x} in the response rate"),
        _ => format!("an effect of {x}{unit}"),
    };
    let a = format!("one-sided α = {}", dc.alpha);
    match dc.kind.as_str() {
        "significance" => format!("Go if the effect is statistically significant ({a}); otherwise No-Go."),
        "mcid" => {
            format!("Go if significant ({a}) and the observed effect reaches {}; Consider if significant but smaller; No-Go if not significant.", th(dc.mcid))
        }
        _ => format!(
            "Go if the probability that the effect exceeds {} is at least {}%; No-Go if the probability that it exceeds {} is below {}%; otherwise Consider.",
            th(dc.go_threshold),
            (dc.go_prob * 100.0).round(),
            th(dc.nogo_threshold),
            (dc.nogo_prob * 100.0).round()
        ),
    }
}
