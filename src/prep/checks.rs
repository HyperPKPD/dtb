//! Checks of a finished table against what NONMEM expects of a PK/PD dataset. Every finding says what is wrong, how many records are
//! concerned and lists the first few rows (numbered as in the exported file, header = line 1) so they can be found.
//!
//! Findings have the level `error` (NONMEM will stop or give wrong results), `warn` (probably a mistake), `info` or `ok`.

use super::table::{key_of, Cell, Frame};
use crate::core::fixed;
use crate::core::js;
use regex::Regex;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const RESERVED: &[&str] = &["DATE", "DAT1", "DAT2", "DAT3", "DROP", "SKIP", "DEFAULT", "NULL"];
pub const NUMERIC_ROLES: &[&str] = &["ID", "TIME", "AMT", "DV", "EVID", "MDV", "CMT", "RATE", "ADDL", "II", "SS", "CENS", "LLOQ"];
const SAMPLE: usize = 8;

#[derive(Default, Clone)]
pub struct CheckOptions {
    pub roles: HashMap<String, String>,
    pub lloq: Option<f64>,
    pub covariates: Vec<String>,
    /// `false` leaves out the note that a covariate varies within subjects.
    pub baseline: Option<bool>,
}

/// Find the NONMEM role columns by name (case-insensitive); `overrides` is `{ ID: "SUBJ", … }`.
pub fn detect_roles(columns: &[String], overrides: &HashMap<String, String>) -> Vec<(String, String)> {
    let mut out = vec![];
    for r in NUMERIC_ROLES {
        if let Some(o) = overrides.get(*r) {
            if columns.contains(o) {
                out.push((r.to_string(), o.clone()));
                continue;
            }
        }
        if let Some(c) = columns.iter().find(|x| x.to_uppercase() == *r) {
            out.push((r.to_string(), c.clone()));
        }
    }
    out
}

fn name_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^[A-Za-z][A-Za-z0-9_]*$").unwrap())
}
fn fmt(n: usize) -> String {
    js::thousands(n)
}

struct Findings(Vec<Value>);
impl Findings {
    fn add(&mut self, level: &str, id: &str, title: &str, detail: &str, rows: &[usize], count: Option<usize>) {
        self.0.push(json!({
            "level": level, "id": id, "title": title, "detail": detail,
            "count": count.unwrap_or(rows.len()),
            "rows": rows.iter().take(SAMPLE).map(|i| i + 2).collect::<Vec<_>>(),
        }));
    }
    fn ok(&mut self, id: &str, title: &str) {
        self.0.push(json!({"level": "ok", "id": id, "title": title, "detail": "", "count": 0, "rows": []}));
    }
    fn has(&self, id: &str) -> bool {
        self.0.iter().any(|x| x["id"] == id)
    }
}

pub fn check_dataset(t: &Frame, opts: &CheckOptions) -> Value {
    let mut f = Findings(vec![]);
    let roles = detect_roles(&t.columns, &opts.roles);
    let ix: Vec<(String, usize)> = roles.iter().map(|(k, v)| (k.clone(), t.col(v).unwrap())).collect();
    // the position of every role column, found once (the checks look a column up for every record)
    let mut slot: [Option<usize>; 13] = [None; 13];
    for (k, j) in &ix {
        if let Some(p) = NUMERIC_ROLES.iter().position(|r| r == k) {
            slot[p] = Some(*j);
        }
    }
    let role_pos = |k: &str| -> usize {
        match k {
            "ID" => 0,
            "TIME" => 1,
            "AMT" => 2,
            "DV" => 3,
            "EVID" => 4,
            "MDV" => 5,
            "CMT" => 6,
            "RATE" => 7,
            "ADDL" => 8,
            "II" => 9,
            "SS" => 10,
            "CENS" => 11,
            _ => 12,
        }
    };
    let has = |k: &str| slot[role_pos(k)].is_some();
    let ixof = |k: &str| slot[role_pos(k)];
    let n = t.rows.len();
    let col = |k: &str, i: usize| -> Option<&Cell> { ixof(k).map(|j| &t.rows[i][j]) };
    let num = |k: &str, i: usize| -> Option<f64> { col(k, i).and_then(Cell::num) };

    // ----- header
    {
        let bad: Vec<&String> =
            t.columns.iter().filter(|c| !name_re().is_match(c) || c.chars().count() > 20 || RESERVED.contains(&c.to_uppercase().as_str())).collect();
        if !bad.is_empty() {
            let mut uniq: Vec<&String> = vec![];
            for b in &bad {
                if !uniq.contains(b) {
                    uniq.push(b);
                }
            }
            f.add(
                "warn",
                "HDR",
                "Some column names are not valid NONMEM names",
                &format!(
                    "NONMEM names start with a letter, hold letters, digits and underscores, and have at most 20 characters; {}{}. Rename them, or drop the columns in $INPUT.",
                    uniq.iter().take(10).map(|b| format!("\"{b}\"")).collect::<Vec<_>>().join(", "),
                    if bad.len() > 10 { " …" } else { "" }
                ),
                &[],
                Some(bad.len()),
            );
        } else {
            f.ok("HDR", "Column names are valid");
        }
        let lacking: Vec<&str> = ["ID", "TIME", "DV"].into_iter().filter(|r| !has(r)).collect();
        if !lacking.is_empty() {
            f.add(
                "error",
                "ROLE",
                &format!("No {} column", lacking.join(", ")),
                &format!("Name the columns {} (upper or lower case), or choose them in \"Columns used by the checks\".", lacking.join(", ")),
                &[],
                None,
            );
        }
    }
    if !has("ID") || n == 0 {
        return finish(f.0, t, &roles, &ix);
    }

    // ----- numeric-ness
    {
        let text: Vec<(&String, usize)> = ix.iter().filter(|(_, j)| t.rows.iter().any(|r| r[*j].is_text())).map(|(k, j)| (k, *j)).collect();
        if !text.is_empty() {
            let d = text
                .iter()
                .map(|(k, j)| {
                    let ex = t.rows.iter().find(|r| r[*j].is_text()).map(|r| r[*j].text()).unwrap_or_default();
                    let name = &roles.iter().find(|x| &x.0 == *k).unwrap().1;
                    format!("{name} (for example \"{ex}\")")
                })
                .collect::<Vec<_>>()
                .join("; ");
            f.add(
                "error",
                "NUM",
                "Text in columns NONMEM reads as numbers",
                &format!("{d}. Convert them with \"Make columns numeric\" or handle the BLQ markers."),
                &[],
                Some(text.len()),
            );
        } else {
            f.ok("NUM", "ID, TIME, AMT, DV and the other record columns are numeric");
        }
        let role_cols: Vec<&String> = roles.iter().map(|x| &x.1).collect();
        let other: Vec<&String> =
            t.columns.iter().enumerate().filter(|(j, c)| !role_cols.contains(c) && t.rows.iter().any(|r| r[*j].is_text())).map(|(_, c)| c).collect();
        if !other.is_empty() {
            f.add(
                "info",
                "TEXT",
                "Text columns",
                &format!(
                    "{} hold text. NONMEM cannot read text: they are written as {} in $INPUT, or recode them to numbers.",
                    other.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
                    other.iter().map(|c| format!("{c}=DROP")).collect::<Vec<_>>().join(", ")
                ),
                &[],
                Some(other.len()),
            );
        }
    }

    // ----- missing
    {
        let no_id: Vec<usize> = (0..n).filter(|i| col("ID", *i).is_some_and(Cell::is_null)).collect();
        if !no_id.is_empty() {
            f.add("error", "MISS-ID", "Rows without a subject ID", "Every record needs an ID.", &no_id, None);
        }
        if has("TIME") {
            let no_t: Vec<usize> = (0..n).filter(|i| col("TIME", *i).is_some_and(Cell::is_null)).collect();
            if !no_t.is_empty() {
                f.add(
                    "error",
                    "MISS-TIME",
                    "Rows without TIME",
                    "Every record needs a TIME (use 0 on the first record if the dataset has no time).",
                    &no_t,
                    None,
                );
            }
        }
        if no_id.is_empty() && (!has("TIME") || !f.has("MISS-TIME")) {
            f.ok("MISS", "ID and TIME are present on every record");
        }
    }

    // ----- subjects, order
    let idx = ixof("ID").unwrap();
    let mut blocks: Vec<(String, Vec<usize>)> = vec![];
    let mut bpos: HashMap<String, usize> = HashMap::new();
    let mut non_contig: Vec<String> = vec![];
    let mut prev: Option<String> = None;
    for i in 0..n {
        let k = key_of(&t.rows[i][idx]);
        if prev.as_ref() != Some(&k) {
            if bpos.contains_key(&k) {
                if !non_contig.contains(&k) {
                    non_contig.push(k.clone());
                }
            } else {
                bpos.insert(k.clone(), blocks.len());
                blocks.push((k.clone(), vec![]));
            }
            prev = Some(k.clone());
        }
        blocks[bpos[&k]].1.push(i);
    }
    if !non_contig.is_empty() {
        let mut rows: Vec<usize> = non_contig.iter().flat_map(|k| blocks[bpos[k]].1.clone()).collect();
        rows.sort_unstable();
        f.add(
            "error",
            "ID-BLOCK",
            "Subjects appear in more than one block",
            &format!(
                "NONMEM treats each change of ID as a new subject: {}{} appear in several places. Sort by ID and TIME.",
                non_contig.iter().take(5).cloned().collect::<Vec<_>>().join(", "),
                if non_contig.len() > 5 { " …" } else { "" }
            ),
            &rows,
            Some(non_contig.len()),
        );
    } else {
        f.ok("ID-BLOCK", "Each subject is one block of rows");
    }
    if has("TIME") {
        let (mut time_bad, mut time_neg) = (vec![], vec![]);
        for (_, rows) in &blocks {
            let mut last = f64::NEG_INFINITY;
            for &i in rows {
                let Some(v) = num("TIME", i) else { continue };
                if v < last {
                    time_bad.push(i);
                } else {
                    last = v;
                }
                if v < 0.0 {
                    time_neg.push(i);
                }
            }
        }
        if !time_bad.is_empty() {
            f.add(
                "error",
                "TIME-ORDER",
                "TIME goes backwards within a subject",
                "Records must be in time order within each subject. Sort by ID and TIME.",
                &time_bad,
                None,
            );
        } else {
            f.ok("TIME-ORDER", "TIME never goes backwards within a subject");
        }
        if !time_neg.is_empty() {
            f.add(
                "warn",
                "TIME-NEG",
                "Negative TIME",
                "Allowed by NONMEM, but often a mistake (a sample before the first dose, or a wrong reference date).",
                &time_neg,
                None,
            );
        }
    }

    // ----- record types
    let evid = |i: usize| -> f64 {
        if let Some(e) = num("EVID", i) {
            return e;
        }
        if num("AMT", i).is_some_and(|a| a > 0.0) {
            1.0
        } else {
            0.0
        }
    };
    if has("EVID") {
        let bad: Vec<usize> = (0..n).filter(|i| !num("EVID", *i).is_some_and(|e| [0.0, 1.0, 2.0, 3.0, 4.0].contains(&e))).collect();
        if !bad.is_empty() {
            f.add("error", "EVID", "EVID is not 0, 1, 2, 3 or 4", "The event ID must be a whole number from 0 to 4 on every record.", &bad, None);
        } else {
            f.ok("EVID", "EVID values are valid");
        }
    } else if has("AMT") {
        f.add("info", "EVID", "No EVID column", "NONMEM will treat every row with AMT > 0 as a dose. Adding EVID makes the records explicit.", &[], None);
    }
    if has("MDV") {
        let bad: Vec<usize> = (0..n).filter(|i| !num("MDV", *i).is_some_and(|m| m == 0.0 || m == 1.0)).collect();
        if !bad.is_empty() {
            f.add("error", "MDV", "MDV is not 0 or 1", "", &bad, None);
        }
    }
    let (mut dv_missing_obs, mut dose_no_amt, mut obs_with_amt, mut mdv_wrong, mut dose_with_dv, mut mdv0_no_dv) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);
    for i in 0..n {
        let e = evid(i);
        let (a, d, m) = (num("AMT", i), num("DV", i), num("MDV", i));
        let d_null = col("DV", i).is_some_and(Cell::is_null);
        if e == 0.0 {
            if has("DV") && d_null && m != Some(1.0) {
                dv_missing_obs.push(i);
            }
            if has("AMT") && a.is_some_and(|a| a > 0.0) {
                obs_with_amt.push(i);
            }
        }
        if e == 1.0 || e == 4.0 {
            if has("AMT") && !a.is_some_and(|a| a > 0.0) {
                dose_no_amt.push(i);
            }
            if m == Some(0.0) {
                mdv_wrong.push(i);
            }
            if d.is_some_and(|d| d != 0.0) && m != Some(1.0) {
                dose_with_dv.push(i);
            }
        }
        if (e == 2.0 || e == 3.0) && m == Some(0.0) {
            mdv_wrong.push(i);
        }
        if e == 0.0 && m == Some(1.0) && d.is_some() {
            mdv0_no_dv.push(i);
        }
    }
    if !dv_missing_obs.is_empty() {
        f.add(
            "error",
            "DV-MISS",
            "Observation records without a concentration",
            "EVID = 0 and MDV = 0 need a DV. Set MDV = 1 for a missing sample, or remove the row.",
            &dv_missing_obs,
            None,
        );
    }
    if !dose_no_amt.is_empty() {
        f.add("error", "AMT-DOSE", "Dose records without a positive AMT", "EVID = 1 or 4 needs AMT > 0.", &dose_no_amt, None);
    }
    if !obs_with_amt.is_empty() {
        f.add("error", "AMT-OBS", "Observation records with a dose amount", "EVID = 0 with AMT > 0 is contradictory.", &obs_with_amt, None);
    }
    if !mdv_wrong.is_empty() {
        f.add("warn", "MDV-DOSE", "MDV is 0 on a dose record", "Dose records are not observations: MDV should be 1.", &mdv_wrong, None);
    }
    if !dose_with_dv.is_empty() {
        f.add("warn", "DV-DOSE", "Dose records carry a DV", "A DV on a dose row is ignored only if MDV = 1.", &dose_with_dv, None);
    }
    if !mdv0_no_dv.is_empty() {
        f.add(
            "info",
            "MDV-OBS",
            "Observation records excluded with MDV = 1",
            &format!("{} records have a DV but MDV = 1; they are not used in the fit.", fmt(mdv0_no_dv.len())),
            &mdv0_no_dv,
            None,
        );
    }
    if dv_missing_obs.is_empty() && dose_no_amt.is_empty() && obs_with_amt.is_empty() {
        f.ok("REC", "Dose and observation records are consistent");
    }

    // ----- dose / observation presence per subject
    {
        let (mut no_dose, mut no_obs, mut only_one) = (vec![], vec![], vec![]);
        for (_, rows) in &blocks {
            let d = rows.iter().filter(|i| evid(**i) == 1.0 || evid(**i) == 4.0).count();
            let o =
                rows.iter().filter(|i| evid(**i) == 0.0 && num("MDV", **i) != Some(1.0) && (!has("DV") || !col("DV", **i).is_some_and(Cell::is_null))).count();
            if d == 0 && has("AMT") {
                no_dose.push(rows[0]);
            }
            if o == 0 {
                no_obs.push(rows[0]);
            } else if o == 1 {
                only_one.push(rows[0]);
            }
        }
        if has("AMT") && !no_dose.is_empty() {
            f.add(
                "warn",
                "NO-DOSE",
                &format!("{} subject(s) without a dose record", fmt(no_dose.len())),
                "Their concentrations cannot be modelled. Check the first row of each.",
                &no_dose,
                None,
            );
        }
        if !no_obs.is_empty() {
            f.add("warn", "NO-OBS", &format!("{} subject(s) without an observation", fmt(no_obs.len())), "They add nothing to the fit.", &no_obs, None);
        }
        if !only_one.is_empty() {
            f.add(
                "info",
                "ONE-OBS",
                &format!("{} subject(s) with a single observation", fmt(only_one.len())),
                "Fine for a population model, but they contribute little to between-subject variability.",
                &only_one,
                None,
            );
        }
    }

    // ----- compartment, rate, addl
    if has("CMT") {
        let bad: Vec<usize> = (0..n).filter(|i| !num("CMT", *i).is_some_and(|c| c.fract() == 0.0 && c >= 1.0)).collect();
        if !bad.is_empty() {
            f.add("error", "CMT", "CMT is not a positive whole number", "Give every record a compartment.", &bad, None);
        } else {
            f.ok("CMT", "CMT is valid");
        }
    }
    if has("RATE") {
        let (mut bad, mut zero) = (vec![], vec![]);
        for i in 0..n {
            let (r, a) = (num("RATE", i), num("AMT", i));
            if r.is_some_and(|r| r < 0.0 && r != -1.0 && r != -2.0) {
                bad.push(i);
            }
            if r.is_some_and(|r| r > 0.0) && !a.is_some_and(|a| a > 0.0) {
                zero.push(i);
            }
        }
        if !bad.is_empty() {
            f.add("error", "RATE", "Invalid RATE", "RATE is a positive infusion rate, 0, −1 (rate in the model) or −2 (duration in the model).", &bad, None);
        }
        if !zero.is_empty() {
            f.add("warn", "RATE-AMT", "RATE without a dose", "", &zero, None);
        }
    }
    if has("ADDL") || has("II") {
        let (mut bad, mut no_ii, mut no_addl) = (vec![], vec![], vec![]);
        for i in 0..n {
            let (a, ii) = (num("ADDL", i), num("II", i));
            if has("ADDL") && a.is_some_and(|a| a < 0.0 || a.fract() != 0.0) {
                bad.push(i);
            }
            if a.is_some_and(|a| a > 0.0) && !ii.is_some_and(|x| x > 0.0) {
                no_ii.push(i);
            }
            if ii.is_some_and(|x| x > 0.0) && !a.is_some_and(|a| a > 0.0) && evid(i) != 0.0 && !has("SS") {
                no_addl.push(i);
            }
        }
        if !bad.is_empty() {
            f.add("error", "ADDL", "ADDL is not a non-negative whole number", "", &bad, None);
        }
        if !no_ii.is_empty() {
            f.add("error", "II", "ADDL without an interval", "ADDL > 0 needs II > 0.", &no_ii, None);
        }
        if !no_addl.is_empty() {
            f.add("info", "II-ONLY", "II without ADDL", "An interval has no effect without ADDL or SS.", &no_addl, None);
        }
    }

    // ----- DV values
    if has("DV") {
        let (mut neg, mut zero, mut below) = (vec![], vec![], vec![]);
        for i in 0..n {
            if evid(i) != 0.0 || num("MDV", i) == Some(1.0) {
                continue;
            }
            let Some(d) = num("DV", i) else { continue };
            if d < 0.0 {
                neg.push(i);
            }
            if d == 0.0 {
                zero.push(i);
            }
            let l = if has("LLOQ") { num("LLOQ", i) } else { opts.lloq };
            if l.is_some_and(|l| d < l) && !(has("CENS") && num("CENS", i) == Some(1.0)) {
                below.push(i);
            }
        }
        if !neg.is_empty() {
            f.add("warn", "DV-NEG", "Negative concentrations", "Not possible for a concentration; it may be a baseline-corrected value.", &neg, None);
        }
        if !zero.is_empty() {
            f.add(
                "warn",
                "DV-ZERO",
                "Concentrations equal to 0",
                "Often a BLQ record written as 0 (the residual error model cannot fit 0 on a log scale). Apply a BLQ method.",
                &zero,
                None,
            );
        }
        if !below.is_empty() {
            f.add(
                "warn",
                "BLQ",
                "Concentrations below the LLOQ that are still in the fit",
                "A BLQ method (M1, M3 …) has not been applied to these. In the Check page enter the LLOQ, or use the BLQ step.",
                &below,
                None,
            );
        }
        if neg.is_empty() && zero.is_empty() && below.is_empty() {
            f.ok("DV", "No negative, zero or sub-LLOQ concentrations");
        }
    }

    // ----- duplicates
    {
        let mut seen: HashSet<String> = HashSet::new();
        let mut dup = vec![];
        for (i, r) in t.rows.iter().enumerate() {
            let k = r.iter().map(Cell::text).collect::<Vec<_>>().join("\u{1}");
            if !seen.insert(k) {
                dup.push(i);
            }
        }
        if !dup.is_empty() {
            f.add("warn", "DUP", "Duplicate rows", "Identical rows (a repeated dose or sample is counted twice).", &dup, None);
        } else {
            f.ok("DUP", "No duplicate rows");
        }
    }

    // ----- covariates
    {
        let cov: Vec<&String> = opts.covariates.iter().filter(|c| t.columns.contains(c)).collect();
        for c in &cov {
            let j = t.col(c).unwrap();
            let miss_rows: Vec<usize> = (0..n).filter(|i| t.rows[*i][j].is_null()).collect();
            let miss = miss_rows.len();
            let mut varying = vec![];
            for (_, rows) in &blocks {
                let vals: HashSet<String> = rows
                    .iter()
                    .filter(|i| !t.rows[**i][j].is_null())
                    .map(|i| match &t.rows[*i][j] {
                        Cell::Num(x) => format!("n{}", x.to_bits()),
                        o => format!("t{}", o.text()),
                    })
                    .collect();
                if vals.len() > 1 {
                    varying.push(rows[0]);
                }
            }
            if miss > 0 {
                f.add(
                    "warn",
                    &format!("COV-MISS-{c}"),
                    &format!("{c}: {} missing values ({}% of rows)", fmt(miss), fixed(100.0 * miss as f64 / n as f64, 1)),
                    "NONMEM reads an empty covariate as 0. Fill them (Fill missing covariate values) or exclude the subjects.",
                    &miss_rows,
                    Some(miss),
                );
            }
            if !varying.is_empty() && opts.baseline != Some(false) {
                f.add(
                    "info",
                    &format!("COV-VAR-{c}"),
                    &format!("{c} varies within {} subject(s)", fmt(varying.len())),
                    "Fine for a time-varying covariate; if it should be baseline, take one value per subject.",
                    &varying,
                    None,
                );
            }
        }
        if !cov.is_empty() && !f.0.iter().any(|x| x["id"].as_str().is_some_and(|s| s.starts_with("COV-MISS"))) {
            f.ok("COV", &format!("No missing values in {}", cov.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
        }
    }
    finish(f.0, t, &roles, &ix)
}

fn finish(mut f: Vec<Value>, t: &Frame, roles: &[(String, String)], ix: &[(String, usize)]) -> Value {
    let n = t.rows.len();
    let ixof = |k: &str| ix.iter().find(|(r, _)| r == k).map(|x| x.1);
    let mut roles_obj = Map::new();
    for (k, v) in roles {
        roles_obj.insert(k.clone(), json!(v));
    }
    let mut summary = Map::new();
    summary.insert("records".into(), json!(n));
    summary.insert("columns".into(), json!(t.columns.len()));
    summary.insert("roles".into(), Value::Object(roles_obj));
    if let (Some(id), true) = (ixof("ID"), n > 0) {
        let mut ids: HashSet<String> = HashSet::new();
        for r in &t.rows {
            ids.insert(key_of(&r[id]));
        }
        summary.insert("subjects".into(), json!(ids.len()));
        let ev = |r: &Vec<Cell>| -> f64 {
            if let Some(e) = ixof("EVID").and_then(|j| r[j].num()) {
                return e;
            }
            if ixof("AMT").and_then(|j| r[j].num()).is_some_and(|a| a > 0.0) {
                1.0
            } else {
                0.0
            }
        };
        summary.insert("doses".into(), json!(t.rows.iter().filter(|r| ev(r) == 1.0 || ev(r) == 4.0).count()));
        let obs: Vec<&Vec<Cell>> = t
            .rows
            .iter()
            .filter(|r| ev(r) == 0.0 && !ixof("MDV").is_some_and(|j| r[j] == Cell::Num(1.0)) && ixof("DV").is_none_or(|j| !r[j].is_null()))
            .collect();
        summary.insert("observations".into(), json!(obs.len()));
        if let Some(tj) = ixof("TIME") {
            let ts: Vec<f64> = t.rows.iter().filter_map(|r| r[tj].num()).collect();
            if !ts.is_empty() {
                summary.insert(
                    "timeRange".into(),
                    json!([ts.iter().cloned().fold(f64::INFINITY, f64::min), ts.iter().cloned().fold(f64::NEG_INFINITY, f64::max)]),
                );
            }
        }
        if let Some(aj) = ixof("AMT") {
            let mut lv: Vec<(f64, usize)> = vec![];
            for r in &t.rows {
                if ev(r) == 1.0 || ev(r) == 4.0 {
                    if let Some(a) = r[aj].num() {
                        match lv.iter_mut().find(|x| x.0 == a) {
                            Some(e) => e.1 += 1,
                            None => lv.push((a, 1)),
                        }
                    }
                }
            }
            lv.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            summary.insert("doseLevels".into(), json!(lv.iter().take(12).map(|x| json!([x.0, x.1])).collect::<Vec<_>>()));
        }
        let mut per: HashMap<String, usize> = HashMap::new();
        for r in &obs {
            *per.entry(key_of(&r[id])).or_insert(0) += 1;
        }
        let mut pv: Vec<usize> = per.values().copied().collect();
        pv.sort_unstable();
        if !pv.is_empty() {
            let med = if pv.len() % 2 == 1 { pv[(pv.len() - 1) / 2] as f64 } else { (pv[pv.len() / 2 - 1] + pv[pv.len() / 2]) as f64 / 2.0 };
            summary.insert("obsPerSubject".into(), json!({"min": pv[0], "median": med, "max": pv[pv.len() - 1]}));
        }
        if let Some(dj) = ixof("DV") {
            let d: Vec<f64> = obs.iter().filter_map(|r| r[dj].num()).collect();
            if !d.is_empty() {
                summary.insert(
                    "dv".into(),
                    json!({"min": d.iter().cloned().fold(f64::INFINITY, f64::min), "max": d.iter().cloned().fold(f64::NEG_INFINITY, f64::max)}),
                );
            }
        }
    }
    let mut counts = Map::new();
    for l in ["error", "warn", "info", "ok"] {
        counts.insert(l.into(), json!(f.iter().filter(|x| x["level"] == l).count()));
    }
    let order = |v: &Value| match v["level"].as_str().unwrap_or("") {
        "error" => 0,
        "warn" => 1,
        "info" => 2,
        _ => 3,
    };
    f.sort_by_key(order);
    let errors = counts["error"].as_u64().unwrap_or(0);
    json!({"findings": f, "counts": counts, "summary": Value::Object(summary), "ready": errors == 0})
}
