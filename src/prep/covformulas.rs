//! Covariate formulas: body size and renal function. Each returns a number, or None when an input is missing or impossible.

pub fn to_kg(w: Option<f64>, unit: &str) -> Option<f64> {
    w.map(|w| if unit == "lb" { w * 0.45359237 } else { w })
}
pub fn to_cm(h: Option<f64>, unit: &str) -> Option<f64> {
    h.map(|h| match unit {
        "m" => h * 100.0,
        "in" => h * 2.54,
        _ => h,
    })
}
pub fn to_mg_dl(s: Option<f64>, unit: &str) -> Option<f64> {
    s.map(|s| if unit == "umol/L" { s / 88.4 } else { s })
}
fn fin(v: Option<f64>) -> Option<f64> {
    v.filter(|x| x.is_finite())
}

pub fn bmi(kg: Option<f64>, cm: Option<f64>) -> Option<f64> {
    let (kg, cm) = (fin(kg)?, fin(cm)?);
    (cm > 0.0).then(|| kg / (cm / 100.0).powi(2))
}
pub fn bsa_mosteller(kg: Option<f64>, cm: Option<f64>) -> Option<f64> {
    let (kg, cm) = (fin(kg)?, fin(cm)?);
    (kg > 0.0 && cm > 0.0).then(|| ((cm * kg) / 3600.0).sqrt())
}
pub fn bsa_dubois(kg: Option<f64>, cm: Option<f64>) -> Option<f64> {
    let (kg, cm) = (fin(kg)?, fin(cm)?);
    (kg > 0.0 && cm > 0.0).then(|| 0.007184 * kg.powf(0.425) * cm.powf(0.725))
}
/// Devine ideal body weight (kg).
pub fn ibw(cm: Option<f64>, male: Option<bool>) -> Option<f64> {
    let cm = fin(cm)?;
    let male = male?;
    let inch = cm / 2.54;
    Some(if male { 50.0 } else { 45.5 } + 2.3 * (inch - 60.0).max(0.0))
}
pub fn adj_bw(kg: Option<f64>, cm: Option<f64>, male: Option<bool>) -> Option<f64> {
    let i = ibw(cm, male)?;
    let kg = fin(kg)?;
    Some(i + 0.4 * (kg - i))
}
/// James lean body mass (kg).
pub fn lbm_james(kg: Option<f64>, cm: Option<f64>, male: Option<bool>) -> Option<f64> {
    let (kg, cm) = (fin(kg)?, fin(cm)?);
    let male = male?;
    (cm > 0.0).then(|| if male { 1.1 * kg - 128.0 * (kg / cm).powi(2) } else { 1.07 * kg - 148.0 * (kg / cm).powi(2) })
}
/// Cockcroft-Gault creatinine clearance (mL/min); `wt` is the weight chosen by the caller.
pub fn crcl_cg(age: Option<f64>, wt: Option<f64>, scr: Option<f64>, male: Option<bool>) -> Option<f64> {
    let (age, wt, scr) = (fin(age)?, fin(wt)?, fin(scr)?);
    let male = male?;
    (scr > 0.0).then(|| ((140.0 - age) * wt) / (72.0 * scr) * if male { 1.0 } else { 0.85 })
}
/// CKD-EPI 2021 (race-free).
pub fn egfr_ckdepi2021(age: Option<f64>, scr: Option<f64>, male: Option<bool>) -> Option<f64> {
    let (age, scr) = (fin(age)?, fin(scr)?);
    let male = male?;
    if scr <= 0.0 {
        return None;
    }
    let (k, a) = if male { (0.9, -0.302) } else { (0.7, -0.241) };
    let r = scr / k;
    Some(142.0 * r.min(1.0).powf(a) * r.max(1.0).powf(-1.2) * 0.9938f64.powf(age) * if male { 1.0 } else { 1.012 })
}
/// CKD-EPI 2009 without the race term.
pub fn egfr_ckdepi2009(age: Option<f64>, scr: Option<f64>, male: Option<bool>) -> Option<f64> {
    let (age, scr) = (fin(age)?, fin(scr)?);
    let male = male?;
    if scr <= 0.0 {
        return None;
    }
    let (k, a) = if male { (0.9, -0.411) } else { (0.7, -0.329) };
    let r = scr / k;
    Some(141.0 * r.min(1.0).powf(a) * r.max(1.0).powf(-1.209) * 0.993f64.powf(age) * if male { 1.0 } else { 1.018 })
}
/// MDRD (IDMS-traceable, 175).
pub fn egfr_mdrd(age: Option<f64>, scr: Option<f64>, male: Option<bool>) -> Option<f64> {
    let (age, scr) = (fin(age)?, fin(scr)?);
    let male = male?;
    (scr > 0.0 && age > 0.0).then(|| 175.0 * scr.powf(-1.154) * age.powf(-0.203) * if male { 1.0 } else { 0.742 })
}
/// Bedside Schwartz (children).
pub fn egfr_schwartz(cm: Option<f64>, scr: Option<f64>) -> Option<f64> {
    let (cm, scr) = (fin(cm)?, fin(scr)?);
    (scr > 0.0).then(|| (0.413 * cm) / scr)
}

pub struct Formula {
    pub id: &'static str,
    pub label: &'static str,
    pub needs: &'static [&'static str],
    pub out: &'static str,
}

pub const FORMULAS: &[Formula] = &[
    Formula { id: "bmi", label: "Body mass index (kg/m²)", needs: &["wt", "ht"], out: "BMI" },
    Formula { id: "bsa-mosteller", label: "Body surface area, Mosteller (m²)", needs: &["wt", "ht"], out: "BSA" },
    Formula { id: "bsa-dubois", label: "Body surface area, Du Bois (m²)", needs: &["wt", "ht"], out: "BSA" },
    Formula { id: "ibw", label: "Ideal body weight, Devine (kg)", needs: &["ht", "sex"], out: "IBW" },
    Formula { id: "lbm-james", label: "Lean body mass, James (kg)", needs: &["wt", "ht", "sex"], out: "LBM" },
    Formula { id: "crcl-cg", label: "Creatinine clearance, Cockcroft-Gault (mL/min)", needs: &["age", "wt", "scr", "sex"], out: "CRCL" },
    Formula { id: "egfr-ckdepi2021", label: "eGFR, CKD-EPI 2021, race-free (mL/min/1.73 m²)", needs: &["age", "scr", "sex"], out: "EGFR" },
    Formula { id: "egfr-ckdepi2009", label: "eGFR, CKD-EPI 2009, without race (mL/min/1.73 m²)", needs: &["age", "scr", "sex"], out: "EGFR" },
    Formula { id: "egfr-mdrd", label: "eGFR, MDRD (mL/min/1.73 m²)", needs: &["age", "scr", "sex"], out: "EGFR" },
    Formula { id: "egfr-schwartz", label: "eGFR, bedside Schwartz, children (mL/min/1.73 m²)", needs: &["ht", "scr"], out: "EGFR" },
];
