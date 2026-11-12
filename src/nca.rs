//! Non-compartmental analysis of concentration-time data (single dose, extravascular or intravenous), linear-up / log-down
//! trapezoidal rule. The output feeds the bioequivalence analysis: Cmax, Tmax, AUClast, AUCinf, Tlast, lambda_z, half-life.

use crate::core::{num, Table};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct Slope {
    pub lambda: f64,
    pub intercept: f64,
    pub r2: f64,
    pub adj_r2: f64,
    pub n: usize,
    pub from: f64,
    pub to: f64,
}

/// Terminal slope: the best-fitting regression on the last 3 or more points after Tmax (largest adjusted R squared; the longest wins ties).
pub fn terminal_slope(t: &[f64], c: &[f64], tmax_index: usize, min_points: usize) -> Option<Slope> {
    let idx: Vec<usize> = ((tmax_index + 1)..t.len()).filter(|&i| c[i] > 0.0).collect();
    let mut best: Option<Slope> = None;
    for k in min_points..=idx.len() {
        let sel = &idx[idx.len() - k..];
        let x: Vec<f64> = sel.iter().map(|&i| t[i]).collect();
        let y: Vec<f64> = sel.iter().map(|&i| c[i].ln()).collect();
        let kf = k as f64;
        let mx = x.iter().sum::<f64>() / kf;
        let my = y.iter().sum::<f64>() / kf;
        let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
        for i in 0..k {
            sxx += (x[i] - mx).powi(2);
            sxy += (x[i] - mx) * (y[i] - my);
            syy += (y[i] - my).powi(2);
        }
        if !(sxx > 0.0) || !(syy > 0.0) {
            continue;
        }
        let slope = sxy / sxx;
        if !(slope < 0.0) {
            continue;
        }
        let r2 = sxy * sxy / (sxx * syy);
        let adj = if k > 2 { 1.0 - (1.0 - r2) * (kf - 1.0) / (kf - 2.0) } else { r2 };
        let cand = Slope { lambda: -slope, intercept: my - slope * mx, r2, adj_r2: adj, n: k, from: x[0], to: x[k - 1] };
        if best.as_ref().is_none_or(|b| adj > b.adj_r2 - 1e-4) {
            best = Some(cand);
        }
    }
    best
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NcaOptions {
    pub dose: Option<f64>,
    pub lloq: Option<f64>,
    pub zero_at_start: Option<bool>,
    pub min_points: Option<usize>,
    pub max_extrap: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Nca {
    pub ok: bool,
    pub flags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmax: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tmax: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clast: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tlast: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auc_last: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aumc_last: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lambda: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub half_life: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adj_r2: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_terminal: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auc_inf: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pct_extrap: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cl_f: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vz_f: Option<f64>,
}

/// NCA of one profile (NaN marks a missing value).
pub fn nca_profile(time: &[f64], conc: &[f64], o: &NcaOptions) -> Nca {
    let mut order: Vec<usize> = (0..time.len()).filter(|&i| time[i].is_finite() && conc[i].is_finite()).collect();
    order.sort_by(|&a, &b| time[a].total_cmp(&time[b]));
    let mut t: Vec<f64> = order.iter().map(|&i| time[i]).collect();
    let mut c: Vec<f64> = order.iter().map(|&i| conc[i]).collect();
    let mut res = Nca::default();
    if let Some(lloq) = o.lloq.filter(|l| l.is_finite() && *l > 0.0) {
        // below the limit: zero before the first quantifiable value, left out after it
        let first = c.iter().position(|&v| v >= lloq);
        let (mut t2, mut c2) = (Vec::new(), Vec::new());
        for i in 0..t.len() {
            if c[i] >= lloq {
                t2.push(t[i]);
                c2.push(c[i]);
            } else if first.is_none_or(|f| i < f) {
                t2.push(t[i]);
                c2.push(0.0);
            }
        }
        t = t2;
        c = c2;
    }
    if o.zero_at_start != Some(false) && !t.is_empty() && t[0] > 0.0 {
        t.insert(0, 0.0);
        c.insert(0, 0.0);
        res.flags.push("A zero concentration was added at time 0.".into());
    }
    if t.len() < 3 || c.iter().all(|v| !(*v > 0.0)) {
        res.flags.push("Too few quantifiable concentrations.".into());
        return res;
    }
    let mut imax = 0;
    for i in 1..c.len() {
        if c[i] > c[imax] {
            imax = i;
        }
    }
    let mut ilast = c.len() - 1;
    while ilast > 0 && !(c[ilast] > 0.0) {
        ilast -= 1;
    }
    let (mut auc, mut aumc) = (0.0, 0.0);
    for i in 1..=ilast {
        let dt = t[i] - t[i - 1];
        let (c1, c2) = (c[i - 1], c[i]);
        if c2 < c1 && c2 > 0.0 {
            let k = (c1 / c2).ln() / dt;
            auc += (c1 - c2) / k;
            aumc += (c1 * t[i - 1] - c2 * t[i]) / k + (c1 - c2) / (k * k);
        } else {
            auc += (c1 + c2) / 2.0 * dt;
            aumc += (c1 * t[i - 1] + c2 * t[i]) / 2.0 * dt;
        }
    }
    res.ok = true;
    res.cmax = Some(c[imax]);
    res.tmax = Some(t[imax]);
    res.clast = Some(c[ilast]);
    res.tlast = Some(t[ilast]);
    res.auc_last = Some(auc);
    res.aumc_last = Some(aumc);
    res.n = Some(t.len());
    match terminal_slope(&t, &c, imax, o.min_points.unwrap_or(3)).filter(|s| s.adj_r2 >= 0.8) {
        Some(ts) => {
            let clast_pred = (ts.intercept - ts.lambda * t[ilast]).exp();
            res.lambda = Some(ts.lambda);
            res.half_life = Some(std::f64::consts::LN_2 / ts.lambda);
            res.adj_r2 = Some(ts.adj_r2);
            res.n_terminal = Some(ts.n);
            let auc_inf = auc + clast_pred / ts.lambda;
            res.auc_inf = Some(auc_inf);
            let pct = 100.0 * (clast_pred / ts.lambda) / auc_inf;
            res.pct_extrap = Some(pct);
            if pct > o.max_extrap.unwrap_or(20.0) {
                res.flags.push(format!("AUCinf extrapolates {} % beyond the last point.", crate::core::fixed(pct, 0)));
            }
            if let Some(dose) = o.dose.filter(|d| *d != 0.0) {
                let clf = dose / auc_inf;
                res.cl_f = Some(clf);
                res.vz_f = Some(clf / ts.lambda);
            }
        }
        None => res.flags.push("The terminal slope could not be estimated reliably, so AUCinf is missing.".into()),
    }
    res
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NcaMap {
    pub subject: String,
    pub treatment: String,
    pub time: String,
    pub conc: String,
    pub period: String,
    pub sequence: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Profile {
    pub subject: String,
    pub treatment: String,
    pub period: String,
    pub sequence: String,
    pub t: Vec<f64>,
    pub c: Vec<f64>,
    pub result: Nca,
}

#[derive(Clone, Debug, Serialize)]
pub struct NcaTable {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub errors: Vec<String>,
    pub profiles: Vec<Profile>,
}

fn is_blq(s: &str) -> bool {
    let l = s.trim().to_ascii_lowercase();
    l == "blq" || l == "bql" || l.starts_with('<')
}

/// Significant digits as JavaScript's `String(Number(v.toPrecision(d)))`.
pub fn sig(v: f64, d: usize) -> String {
    if !v.is_finite() {
        return String::new();
    }
    let s = format!("{:.*e}", d.saturating_sub(1), v);
    let x: f64 = s.parse().unwrap_or(v);
    format!("{x}")
}

/// NCA of every subject-treatment-period profile in a long table.
pub fn nca_table(t: &Table, map: &NcaMap, o: &NcaOptions) -> NcaTable {
    let ix = |n: &str| if n.is_empty() { None } else { t.col(n) };
    let (is, it, ip, iq, ia, ic) = (ix(&map.subject), ix(&map.treatment), ix(&map.period), ix(&map.sequence), ix(&map.time), ix(&map.conc));
    let (Some(is), Some(it), Some(ia), Some(ic)) = (is, it, ia, ic) else {
        return NcaTable {
            columns: vec![],
            rows: vec![],
            errors: vec!["Choose the subject, treatment, time and concentration columns.".into()],
            profiles: vec![],
        };
    };
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Profile> = std::collections::HashMap::new();
    for r in &t.rows {
        let key = format!("{}\u{1}{}\u{1}{}", r[is], r[it], ip.map_or("1", |p| r[p].as_str()));
        let g = groups.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            Profile {
                subject: r[is].trim().to_string(),
                treatment: r[it].trim().to_string(),
                period: ip.map_or("1".to_string(), |p| r[p].trim().to_string()),
                sequence: iq.map_or(String::new(), |q| r[q].trim().to_string()),
                t: vec![],
                c: vec![],
                result: Nca::default(),
            }
        });
        g.t.push(num(&r[ia]));
        g.c.push(if is_blq(&r[ic]) { 0.0 } else { num(&r[ic]) });
    }
    let columns: Vec<String> =
        ["Subject", "Sequence", "Period", "Treatment", "Cmax", "Tmax", "AUClast", "AUCinf", "Tlast", "Lambda_z", "HalfLife", "PctExtrap"]
            .iter()
            .map(|s| s.to_string())
            .collect();
    let mut rows = Vec::new();
    let mut profiles = Vec::new();
    let f = |v: Option<f64>, d: usize| v.map(|x| sig(x, d)).unwrap_or_default();
    for key in order {
        let mut g = groups.remove(&key).unwrap();
        let r = nca_profile(&g.t, &g.c, o);
        rows.push(vec![
            g.subject.clone(),
            g.sequence.clone(),
            g.period.clone(),
            g.treatment.clone(),
            f(r.cmax, 5),
            f(r.tmax, 5),
            f(r.auc_last, 5),
            f(r.auc_inf, 5),
            f(r.tlast, 5),
            f(r.lambda, 5),
            f(r.half_life, 5),
            f(r.pct_extrap, 3),
        ]);
        g.result = r;
        profiles.push(g);
    }
    NcaTable { columns, rows, errors: vec![], profiles }
}
