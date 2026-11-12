//! The data engine of Prep Studio, checked against the JavaScript engine of the app (fixtures written by tools/gen_fixtures.mjs).
mod common;
use common::{deep, fixture};
use dtb::core::{parse_delimited, to_delimited};
use dtb::prep::checks::{check_dataset, CheckOptions};
use dtb::prep::expr::{evaluate, Val};
use dtb::prep::recipe::{fingerprint, read_recipe, run_recipe, RunOptions};
use dtb::prep::table::{profile, Frame};
use serde_json::{json, Value};

fn frame(text: &str) -> Frame {
    Frame::from_text(&parse_delimited(text, None).table)
}

#[test]
fn recipes_give_the_same_tables_and_notes() {
    let fx = fixture("prep.json");
    let files = fx["files"].as_object().unwrap();
    for case in fx["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let extra = case["extra"].as_object().unwrap();
        let sources: Vec<(String, Frame)> = case["sources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| {
                let n = n.as_str().unwrap();
                let text = extra.get(n).or_else(|| files.get(n)).unwrap_or_else(|| panic!("{name}: no source {n}")).as_str().unwrap();
                (n.to_string(), frame(text))
            })
            .collect();
        let recipe = read_recipe(&case["recipe"]).unwrap_or_else(|e| panic!("{name}: {e}"));
        let run = run_recipe(&recipe, &sources, &RunOptions::default());
        let exp = &case["expect"];
        assert_eq!(run.failed, exp["failed"].as_bool().unwrap(), "{name}: failed");
        deep(&Value::Array(run.steps.clone()), &exp["steps"], 1e-9, false, &format!("{name}.steps"));
        match (&run.table, exp["table"].is_null()) {
            (Some(t), false) => deep(&t.to_json(), &exp["table"], 1e-9, true, &format!("{name}.table")),
            (None, true) => {}
            (t, _) => panic!("{name}: table {:?} vs {}", t.is_some(), exp["table"]),
        }
    }
}

fn val_json(v: &Val) -> Value {
    match v {
        Val::Null => Value::Null,
        Val::Bool(b) => json!(b),
        Val::Num(x) if !x.is_finite() => json!("__nonfinite__"),
        Val::Num(x) => json!(x),
        Val::Text(s) => json!(s),
    }
}

#[test]
fn expressions_give_the_same_values_and_messages() {
    let fx = fixture("prep.json");
    let t = frame(fx["exprText"].as_str().unwrap());
    for e in fx["exprs"].as_array().unwrap() {
        let src = e["src"].as_str().unwrap();
        let got = evaluate(&t, src, &["ID".to_string()]);
        if let Some(msg) = e.get("error") {
            let err = got.err().unwrap_or_else(|| panic!("`{src}` should fail with {msg}"));
            assert_eq!(&json!(err.message), msg, "`{src}`");
        } else {
            let r = got.unwrap_or_else(|er| panic!("`{src}`: {}", er.message));
            let vals: Vec<Value> = r.values.iter().map(val_json).collect();
            // the JavaScript values hold strings for text and numbers for numbers; compare numbers with a tolerance
            deep(&Value::Array(vals), &e["values"], 1e-12, false, &format!("`{src}`"));
            let cols: Vec<&str> = r.columns.iter().map(String::as_str).collect();
            let want: Vec<&str> = e["columns"].as_array().unwrap().iter().map(|c| c.as_str().unwrap()).collect();
            assert_eq!(cols, want, "`{src}` columns used");
        }
    }
}

fn opts_from(v: &Value) -> CheckOptions {
    CheckOptions {
        roles: Default::default(),
        lloq: v.get("lloq").and_then(Value::as_f64),
        covariates: v.get("covariates").and_then(Value::as_array).map(|a| a.iter().map(|x| x.as_str().unwrap().to_string()).collect()).unwrap_or_default(),
        baseline: v.get("baseline").and_then(Value::as_bool),
    }
}

#[test]
fn dataset_checks_give_the_same_findings() {
    let fx = fixture("prep.json");
    for c in fx["checks"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let t = match c.get("table") {
            Some(tj) => Frame::from_json(tj).unwrap(),
            None => frame(c["text"].as_str().unwrap()),
        };
        let got = check_dataset(&t, &opts_from(&c["opts"]));
        deep(&got, &c["expect"], 1e-9, false, name);
        assert_eq!(got["findings"].as_array().unwrap().len(), c["expect"]["findings"].as_array().unwrap().len(), "{name}: number of findings");
    }
}

#[test]
fn profiles_fingerprints_and_exports_match() {
    let fx = fixture("prep.json");
    let files = fx["files"].as_object().unwrap();
    deep(&Value::Array(profile(&frame(fx["messy"].as_str().unwrap()))), &fx["profiles"]["m"], 1e-9, true, "profile m");
    deep(&Value::Array(profile(&frame(files["pk.csv"].as_str().unwrap()))), &fx["profiles"]["pk"], 1e-9, true, "profile pk");
    deep(&Value::Array(profile(&frame(files["demog.csv"].as_str().unwrap()))), &fx["profiles"]["demog"], 1e-9, true, "profile demog");
    for (name, want) in fx["fingerprints"].as_object().unwrap() {
        assert_eq!(&fingerprint(&frame(files[name].as_str().unwrap())), want, "fingerprint {name}");
    }
    let _ = to_delimited; // the export text is checked through the final table of the example recipe below
    let case = &fx["cases"][0];
    let sources: Vec<(String, Frame)> =
        case["sources"].as_array().unwrap().iter().map(|n| (n.as_str().unwrap().to_string(), frame(files[n.as_str().unwrap()].as_str().unwrap()))).collect();
    let run = run_recipe(&read_recipe(&case["recipe"]).unwrap(), &sources, &RunOptions::default());
    let t = run.table.unwrap();
    use dtb::prep::export::{dataset_text, nonmem_input};
    assert_eq!(dataset_text(&t, "comma", ".", true), fx["exportT"]["dataset"].as_str().unwrap());
    assert_eq!(dataset_text(&t, "tab", "", false), fx["exportT"]["tab"].as_str().unwrap());
    assert_eq!(nonmem_input(&t, "ds.csv", "@"), fx["exportT"]["nonmem"].as_str().unwrap());
}
