//! Describing the design found in the records for the two treatments.

use super::records::{cmp_per, Per, Record};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignInfo {
    pub kind: String,
    pub label: String,
    pub name: String,
    pub sequences: BTreeMap<String, usize>,
    pub n_subjects: usize,
    pub n_with_both: usize,
    pub periods: Vec<Per>,
    pub ref_replicated: bool,
    pub test_replicated: bool,
    pub max_obs: usize,
    pub complete: bool,
    pub warnings: Vec<String>,
}

pub fn detect_design(records: &[Record], test: &str, reference: &str) -> DesignInfo {
    let used: Vec<&Record> = records.iter().filter(|r| r.treatment == test || r.treatment == reference).collect();
    let mut subs: Vec<&str> = Vec::new();
    let mut by_sub: HashMap<&str, Vec<&Record>> = HashMap::new();
    for r in &used {
        if !by_sub.contains_key(r.subject.as_str()) {
            subs.push(r.subject.as_str());
        }
        by_sub.entry(r.subject.as_str()).or_default().push(r);
    }
    let mut periods: Vec<Per> = Vec::new();
    for r in &used {
        if !periods.iter().any(|p| p.key() == r.period.key()) {
            periods.push(r.period.clone());
        }
    }
    periods.sort_by(cmp_per);
    let has_both = subs.iter().filter(|s| by_sub[*s].iter().any(|r| r.treatment == test) && by_sub[*s].iter().any(|r| r.treatment == reference)).count();
    let mut sequences: Vec<(String, usize)> = Vec::new();
    for s in &subs {
        let v = &by_sub[s];
        let given = v.iter().find(|r| !r.sequence.is_empty()).map(|r| r.sequence.clone());
        let pat = given.unwrap_or_else(|| {
            let mut w = v.clone();
            w.sort_by(|a, b| cmp_per(&a.period, &b.period));
            w.iter().map(|r| if r.treatment == test { 'T' } else { 'R' }).collect()
        });
        match sequences.iter_mut().find(|(p, _)| *p == pat) {
            Some(e) => e.1 += 1,
            None => sequences.push((pat, 1)),
        }
    }
    let kind = if has_both == 0 { "parallel" } else { "crossover" };
    let max_obs = subs.iter().map(|s| by_sub[s].len()).max().unwrap_or(0);
    let n_ref = |s: &str| by_sub[s].iter().filter(|r| r.treatment == reference).count();
    let n_tst = |s: &str| by_sub[s].iter().filter(|r| r.treatment == test).count();
    let ref_rep = kind == "crossover" && subs.iter().filter(|s| n_ref(s) >= 2).count() >= 2;
    let tst_rep = kind == "crossover" && subs.iter().filter(|s| n_tst(s) >= 2).count() >= 2;
    let label = if kind == "parallel" { "parallel".to_string() } else { format!("{}x2x{}", sequences.len(), periods.len()) };
    let counts: std::collections::HashSet<usize> = subs.iter().map(|s| by_sub[s].len()).collect();
    let complete = kind == "crossover" && subs.len() == has_both && counts.len() == 1;
    let name = match label.as_str() {
        "2x2x2" => "Two-sequence, two-period crossover".to_string(),
        "2x2x3" => "Two-sequence, three-period replicate".to_string(),
        "2x2x4" => "Two-sequence, four-period full replicate".to_string(),
        "3x2x3" => "Three-sequence, three-period partial replicate".to_string(),
        "parallel" => "Parallel groups".to_string(),
        _ => format!("Crossover with {} sequence(s) and {} period(s)", sequences.len(), periods.len()),
    };
    let mut warnings = Vec::new();
    if kind == "crossover" && has_both < subs.len() {
        warnings.push(format!("{} subject(s) have only one of the two treatments; they do not contribute to the treatment comparison.", subs.len() - has_both));
    }
    if kind == "crossover" && !complete {
        warnings.push("The design is unbalanced (missing observations); the estimates come from the fixed-effects model with the data available.".to_string());
    }
    let sizes: Vec<usize> = sequences.iter().map(|s| s.1).collect();
    if kind == "crossover" && sizes.len() > 1 {
        let (mx, mn) = (*sizes.iter().max().unwrap() as f64, *sizes.iter().min().unwrap() as f64);
        if mx - mn > 0.34 * mx {
            warnings.push("The sequences differ much in size.".to_string());
        }
    }
    DesignInfo {
        kind: kind.into(),
        label,
        name,
        sequences: sequences.into_iter().collect(),
        n_subjects: subs.len(),
        n_with_both: has_both,
        periods,
        ref_replicated: ref_rep,
        test_replicated: tst_rep,
        max_obs,
        complete,
        warnings,
    }
}
