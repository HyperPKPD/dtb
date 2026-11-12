mod common;
use common::*;
use dtb::trial::pk::{exposure, profile, Subject};
use dtb::trial::*;
use serde_json::{json, Value};

fn design(fx: &Value, k: &str) -> Design {
    serde_json::from_value(fx["designs"][k].clone()).unwrap()
}

#[test]
fn simulations_reproduce_the_browser_engine_with_the_same_stream() {
    let fx = fixture("trial.json");
    for k in ["continuous", "continuousUncertain", "binary", "tte", "ivSignificance"] {
        let d = design(&fx, k);
        let o = RunOptions { n_trials: 150, seed: 777, streams: Streams::Js, values: true, timing: false };
        let r = run_scenario(&d, &o).unwrap();
        let got = serde_json::to_value(&r).unwrap();
        deep(&got, &fx["scenarios"][k], 1e-8, false, k);
    }
}

#[test]
fn validation_messages_are_the_same() {
    let fx = fixture("trial.json");
    for k in ["continuous", "continuousUncertain", "binary", "tte", "ivSignificance", "bad", "warn"] {
        let v = validate_design(&design(&fx, k));
        assert_eq!(json!(v.errors), fx["validate"][k]["errors"], "{k} errors");
        assert_eq!(json!(v.warnings), fx["validate"][k]["warnings"], "{k} warnings");
    }
}

#[test]
fn scenario_overrides_are_applied_as_in_the_app() {
    let fx = fixture("trial.json");
    let o: Overrides = serde_json::from_value(fx["overrides"]["in"].clone()).unwrap();
    let out = apply_overrides(&design(&fx, "continuous"), &o);
    deep(&serde_json::to_value(&out).unwrap(), &fx["overrides"]["out"], 1e-12, false, "overrides");
}

#[test]
fn sensitivity_bands_and_curve() {
    let fx = fixture("trial.json");
    let d = design(&fx, "continuous");
    let t = tornado(&d, "a3", &RunOptions { n_trials: 120, seed: 5, streams: Streams::Js, values: false, timing: false }).unwrap();
    deep(&serde_json::to_value(&t).unwrap(), &fx["tornado"], 1e-8, false, "tornado");
    let b = pk_bands(&d, 0.0, 168.0, 30, 40, 7);
    deep(&serde_json::to_value(&b).unwrap(), &fx["bands"], 1e-8, false, "bands");
    let c = er_curve(&design(&fx, "binary"), 20).unwrap();
    deep(&serde_json::to_value(&c).unwrap(), &fx["erCurve"], 1e-9, false, "erCurve");
}

#[test]
fn pharmacokinetics_match() {
    let fx = fixture("trial.json");
    let taken = [1u8, 1, 0, 1, 1, 1, 0, 0, 1, 1, 1, 1, 1, 0];
    let e = exposure(&Subject { dose: 100.0, f: 0.8, cl: 4.2, v: 45.0, ka: 1.3, tau: 12.0, n: 14, taken: Some(&taken), iv: false, conc_factor: 1000.0 });
    deep(&serde_json::to_value(e).unwrap(), &fx["pk"]["exposure"], 1e-12, false, "exposure");
    let e = exposure(&Subject { dose: 50.0, f: 1.0, cl: 4.2, v: 45.0, ka: 1.0, tau: 24.0, n: 5, taken: None, iv: true, conc_factor: 1000.0 });
    deep(&serde_json::to_value(e).unwrap(), &fx["pk"]["exposureIv"], 1e-12, false, "exposureIv");
    let t4 = [1u8, 0, 1, 1];
    let p = profile(
        &Subject { dose: 100.0, f: 0.8, cl: 4.2, v: 45.0, ka: 1.3, tau: 12.0, n: 4, taken: Some(&t4), iv: false, conc_factor: 1000.0 },
        &[0.0, 1.0, 5.0, 12.0, 13.0, 30.0, 47.0, 60.0],
    );
    deep(&json!(p), &fx["pk"]["profile"], 1e-12, false, "profile");
}
