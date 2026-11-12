mod common;
use common::*;
use dtb::be::power::*;
use dtb::be::*;
use dtb::core::Table;
use dtb::trial::Streams;
use dtb::{ddi, impair, nca};
use serde_json::{json, to_value, Value};

fn table(v: &Value) -> Table {
    serde_json::from_value(v.clone()).unwrap()
}
fn fv(a: &Value) -> Vec<f64> {
    a.as_array().unwrap().iter().map(f).collect()
}
fn seqs(v: &Value) -> Option<Vec<String>> {
    v.as_array().map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect())
}

#[test]
fn bioequivalence_analyses_match_the_browser_engine() {
    let fx = fixture("beddi.json");
    let mut n = 0;
    for case in fx["be"].as_array().unwrap() {
        let t = table(&case["table"]);
        let map: RecordMap = serde_json::from_value(case["map"].clone()).unwrap();
        let rec = records_from_table(&t, &map);
        assert!(rec.errors.is_empty());
        if let Some(w) = case.get("recordWarnings") {
            assert_eq!(json!(rec.warnings), *w);
        }
        let o: BeOptions = serde_json::from_value(case["options"].clone()).unwrap();
        let r = analyze_be(&rec.records, case["param"].as_str().unwrap(), &o);
        let tag = format!("{}/{}/{}", case["study"], case["param"], case["options"]);
        deep(&to_value(&r).unwrap(), &case["result"], 1e-8, false, &tag);
        n += 1;
    }
    assert_eq!(n, 32);
}

#[test]
fn residual_table_matches() {
    let fx = fixture("beddi.json");
    let c = &fx["residuals"];
    let t = table(&c["table"]);
    let map: RecordMap = serde_json::from_value(c["map"].clone()).unwrap();
    let rec = records_from_table(&t, &map);
    let rows = residual_table(&rec.records, "Cmax", &BeOptions::default());
    deep(&to_value(&rows).unwrap(), &c["rows"], 1e-8, false, "residuals");
}

#[test]
fn design_constants_and_exact_power_match() {
    let fx = fixture("beddi.json");
    for c in fx["power"]["constants"].as_array().unwrap() {
        let s = seqs(&c["seqs"]);
        let dc = design_constants(s.as_deref());
        close(dc.c, f(&c["c"]), 1e-10, "c");
        for (i, n) in [12.0, 24.0, 40.0].iter().enumerate() {
            close(dc.df(*n), f(&c["df"][i]), 0.0, "df");
            if !dc.parallel {
                close(dc.df_r(*n), f(&c["dfR"][i]), 0.0, "dfR");
                close(dc.df_t(*n), f(&c["dfT"][i]), 0.0, "dfT");
            }
        }
    }
    for c in fx["power"]["abe"].as_array().unwrap() {
        let a: AbeArgs = serde_json::from_value(c["args"].clone()).unwrap();
        close(power_abe(&a), f(&c["power"]), 1e-9, &format!("power {}", c["args"]));
    }
    for c in fx["power"]["size"].as_array().unwrap() {
        let a: AbeArgs = serde_json::from_value(c["args"].clone()).unwrap();
        let r = sample_size_abe(&a, 0.8, 12.0, 5000.0);
        close(r.n, f(&c["result"]["n"]), 0.0, "size n");
        close(r.power, f(&c["result"]["power"]), 1e-9, "size power");
    }
    let t = &fx["power"]["sizeTable"];
    let rows = sample_size_table(&fv(&t["in"]["cvs"]), &fv(&t["in"]["ratios"]), seqs(&t["in"]["sequences"]).as_deref(), [0.8, 1.25], 0.05, 0.8, 12.0);
    deep(&to_value(&rows).unwrap(), &t["out"], 0.0, false, "sizeTable");
    for r in fx["power"]["dropout"].as_array().unwrap() {
        close(with_dropout(f(&r[0]), f(&r[1]), f(&r[2])), f(&r[3]), 0.0, "dropout");
    }
}

#[test]
fn simulated_power_matches_with_the_same_stream() {
    let fx = fixture("beddi.json");
    for c in fx["power"]["sim"].as_array().unwrap() {
        let a: SimArgs = serde_json::from_value(c["args"].clone()).unwrap();
        let r = power_sim(&a, Streams::Js).unwrap();
        deep(&to_value(&r).unwrap(), &c["result"], 1e-9, false, &format!("sim {}", c["args"]));
    }
    let c = &fx["power"]["simSize"];
    let a: SimArgs = serde_json::from_value(c["args"].clone()).unwrap();
    let r = sample_size_sim(&a, 0.8, 12.0, 120.0, Streams::Js).unwrap();
    close(r.n, f(&c["result"]["n"]), 0.0, "sim size n");
    close(r.power, f(&c["result"]["power"]), 1e-9, "sim size power");
}

#[test]
fn interactions_match() {
    let fx = fixture("beddi.json");
    let cin: ddi::ConcInput = serde_json::from_value(fx["ddi"]["input"]["cin"].clone()).unwrap();
    let enz: Vec<ddi::Enzyme> = serde_json::from_value(fx["ddi"]["input"]["enzymes"].clone()).unwrap();
    for p in fx["ddi"]["presets"].as_array().unwrap() {
        let pr = ddi::preset(p["id"].as_str().unwrap()).unwrap();
        deep(&to_value(&pr).unwrap(), &p["preset"], 0.0, false, "preset");
        let c = ddi::concentrations(&cin, &pr);
        deep(&to_value(c).unwrap(), &p["conc"], 1e-12, false, "conc");
        let st: Vec<_> = enz.iter().map(|e| ddi::static_r(e, &c, &pr)).collect();
        deep(&to_value(&st).unwrap(), &p["static"], 1e-12, false, "static");
        let m = ddi::mechanistic_aucr(&enz, &c, &ddi::Victim { fg: Some(0.6), ..Default::default() });
        deep(&to_value(&m).unwrap(), &p["mech"], 1e-12, false, "mech");
        let m2 = ddi::mechanistic_aucr(&enz, &c, &ddi::Victim { fg: Some(0.6), hepatic_conc: Some("inlet".into()), gut: Some(false) });
        deep(&to_value(&m2).unwrap(), &p["mechInlet"], 1e-12, false, "mechInlet");
    }
    for r in fx["ddi"]["classes"].as_array().unwrap() {
        let c = ddi::classify_aucr(f(&r[0]));
        assert_eq!(c.label, r[1]["label"].as_str().unwrap());
        assert_eq!(c.tone, r[1]["tone"].as_str().unwrap());
    }
    let s = &fx["ddi"]["steady"];
    let st = ddi::steady_state(&serde_json::from_value(s["in"].clone()).unwrap());
    deep(&to_value(&st).unwrap(), &s["out"], 1e-12, false, "steady");
    for r in fx["ddi"]["oral"].as_array().unwrap() {
        let v = fv(r);
        close(ddi::oral_conc(v[0], v[1], v[2], v[3], v[4], v[5]), v[6], 1e-12, "oral");
    }
    let d = &fx["ddi"]["dynamic"];
    let m: ddi::DynModel = serde_json::from_value(d["in"].clone()).unwrap();
    let c = ddi::compare_ddi(&m, None);
    let o = &d["out"];
    close(c.aucr, f(&o["aucr"]), 1e-9, "aucr");
    close(c.cmax_ratio, f(&o["cmaxRatio"]), 1e-9, "cmaxRatio");
    close(c.auc_control, f(&o["aucControl"]), 1e-9, "aucControl");
    assert_eq!(c.control.t.len() as f64, f(&o["n"]));
    let pick = |v: &Vec<f64>| -> Vec<f64> { v.iter().enumerate().filter(|(i, _)| i % 40 == 0).map(|(_, x)| *x).collect() };
    deep(&json!(pick(&c.interaction.victim)), &o["victim"], 1e-9, false, "victim");
    deep(&json!(pick(&c.interaction.enzyme)), &o["enzyme"], 1e-9, false, "enzyme");
    deep(&json!(pick(&c.control.victim)), &o["control"], 1e-9, false, "control");
    deep(&json!(pick(&c.interaction.cl_ratio)), &o["clRatio"], 1e-9, false, "clRatio");
}

#[test]
fn non_compartmental_analysis_matches() {
    let fx = fixture("beddi.json");
    let c = &fx["nca"]["table"];
    let t = table(&c["table"]);
    let map: nca::NcaMap = serde_json::from_value(c["map"].clone()).unwrap();
    let r = nca::nca_table(&t, &map, &nca::NcaOptions { dose: Some(100.0), ..Default::default() });
    assert_eq!(json!(r.columns), c["out"]["columns"]);
    assert_eq!(json!(r.rows), c["out"]["rows"]);
    for (g, w) in r.profiles.iter().zip(c["out"]["profiles"].as_array().unwrap()) {
        deep(&to_value(&g.result).unwrap(), &w["result"], 1e-10, false, "profile");
    }
    for p in fx["nca"]["profiles"].as_array().unwrap() {
        let o: nca::NcaOptions = serde_json::from_value(p["o"].clone()).unwrap();
        let r = nca::nca_profile(&fv(&p["t"]), &fv(&p["c"]), &o);
        deep(&to_value(&r).unwrap(), &p["out"], 1e-10, false, &format!("nca {}", p["c"]));
    }
}

#[test]
fn impairment_functions_match() {
    let fx = fixture("beddi.json");
    let im = &fx["impair"];
    for r in im["egfr"].as_array().unwrap() {
        let p: impair::Person = serde_json::from_value(r["p"].clone()).unwrap();
        close(impair::cockcroft_gault(&p), f(&r["cg"]), 1e-12, "cg");
        close(impair::mdrd(&p), f(&r["mdrd"]), 1e-12, "mdrd");
        close(impair::ckd_epi_2009(&p), f(&r["ckd2009"]), 1e-12, "ckd2009");
        close(impair::ckd_epi_2021(&p), f(&r["ckd2021"]), 1e-12, "ckd2021");
        close(impair::bsa(p.height, p.weight, "dubois"), f(&r["bsaD"]), 1e-12, "bsaD");
        close(impair::bsa(p.height, p.weight, "mosteller"), f(&r["bsaM"]), 1e-12, "bsaM");
        close(impair::ideal_weight(p.height, p.female), f(&r["ibw"]), 1e-12, "ibw");
        close(impair::denormalise(impair::ckd_epi_2021(&p), impair::bsa(p.height, p.weight, "dubois")), f(&r["denorm"]), 1e-12, "denorm");
    }
    for r in im["categories"].as_array().unwrap() {
        let c = impair::renal_category(f(&r[0])).map(|c| c.id);
        assert_eq!(c, r[1].as_str(), "category {}", r[0]);
    }
    deep(&to_value(impair::giusti_hayton(0.6, 45.0, 120.0)).unwrap(), &im["giusti"]["one"], 1e-12, false, "gh");
    deep(&to_value(impair::giusti_hayton_table(0.6, 120.0, Some(100.0), Some(12.0))).unwrap(), &im["giusti"]["table"], 1e-12, false, "ghTable");
    deep(&to_value(impair::giusti_hayton_table(0.3, 120.0, None, None)).unwrap(), &im["giusti"]["noDose"], 1e-12, false, "ghTable2");
    for r in im["childPugh"].as_array().unwrap() {
        let c = impair::child_pugh(&serde_json::from_value(r["in"].clone()).unwrap());
        deep(&to_value(&c).unwrap(), &r["out"], 0.0, false, "childPugh");
    }
    for r in im["liver"].as_array().unwrap() {
        let m: impair::Mult = serde_json::from_value(r["mult"].clone()).unwrap();
        let i: impair::LiverIn = serde_json::from_value(r["in"].clone()).unwrap();
        deep(&to_value(impair::well_stirred(&i, &m)).unwrap(), &r["out"], 1e-12, false, "liver");
    }
    let g = &im["groups"];
    let groups: Vec<impair::GroupIn> = serde_json::from_value(g["groups"].clone()).unwrap();
    let reference = g["ref"].as_str().unwrap();
    let pooled = impair::compare_groups(&groups, reference, 0.1, true);
    deep(&to_value(&pooled).unwrap(), &g["pooled"], 1e-10, false, "pooled");
    deep(&to_value(impair::compare_groups(&groups, reference, 0.05, false)).unwrap(), &g["welch"], 1e-10, false, "welch");
    let adv: Vec<_> = pooled.rows.iter().map(|r| impair::dose_advice(r, [0.8, 1.25])).collect();
    for (a, w) in adv.iter().zip(g["advice"].as_array().unwrap()) {
        assert_eq!(a.text, w["text"].as_str().unwrap());
        assert_eq!(a.tone, w["tone"].as_str().unwrap());
    }
    let r = &im["regress"];
    let (x, y) = (fv(&r["x"]), fv(&r["y"]));
    let a =
        impair::regress_covariate(&x, &y, &impair::RegressOpts { log_x: true, reference: 105.0, points: vec![90.0, 60.0, 30.0, 15.0], alpha: 0.1 }).unwrap();
    deep(&to_value(&a).unwrap(), &r["logX"], 1e-9, false, "regress");
    let b = impair::regress_covariate(&x, &y, &impair::RegressOpts { log_x: false, reference: 100.0, points: vec![60.0, 20.0], alpha: 0.05 }).unwrap();
    deep(&to_value(&b).unwrap(), &r["linear"], 1e-9, false, "regressLinear");
    assert!(impair::regress_covariate(&[1.0, 2.0], &[1.0, 2.0], &Default::default()).is_err());
}
