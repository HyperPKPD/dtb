//! The command line as a program: exit codes, the JSON envelope, determinism and safety.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;

fn dtb() -> Command {
    let mut c = Command::cargo_bin("dtb").unwrap();
    c.env_remove("DTB_CONFIG").env_remove("DTB_THREADS").env_remove("DTB_SEED").env_remove("DTB_TIMING").env_remove("DTB_PRETTY").arg("--no-config");
    c
}

fn json(out: &[u8]) -> Value {
    serde_json::from_slice(out).unwrap()
}

#[test]
fn version_and_commands() {
    dtb().arg("--version").assert().success().stdout(predicate::str::starts_with("dtb "));
    dtb().arg("commands").assert().success().stdout(predicate::str::contains("trial.run")).stdout(predicate::str::contains("prep.run"));
    dtb().args(["commands", "renal"]).assert().success().stdout(predicate::str::contains("renal.egfr")).stdout(predicate::str::contains("trial.run").not());
    dtb().args(["trial"]).assert().success().stdout(predicate::str::contains("trial.tornado"));
    dtb().args(["commands", "nothing-like-this"]).assert().code(2);
}

#[test]
fn doctor_passes() {
    let o = dtb().arg("doctor").assert().success().get_output().stdout.clone();
    let v = json(&o);
    assert_eq!(v["result"]["ok"], true);
}

#[test]
fn info_reports_protocol_and_commands() {
    let v = json(&dtb().arg("info").assert().success().get_output().stdout.clone());
    assert_eq!(v["dtb"]["protocol"], 1);
    assert!(v["result"]["commands"].as_array().unwrap().len() >= 30);
    assert_eq!(v["result"]["limits"]["max_trials"], 1_000_000);
}

#[test]
fn call_with_set_and_group_alias() {
    let a = dtb().args(["call", "stats.describe", "--set", "values=[1,2,3,4,10]", "--result"]).assert().success().get_output().stdout.clone();
    let b = dtb().args(["stats", "describe", "--set", "values=[1,2,3,4,10]", "--result"]).assert().success().get_output().stdout.clone();
    assert_eq!(a, b);
    let v = json(&a);
    assert_eq!(v["mean"], 4.0);
    assert_eq!(v["n"], 5);
}

#[test]
fn command_names_are_forgiving() {
    dtb().args(["trial", "er-curve", "--input", "examples/data/trial-design.json"]).assert().code(2); // design must be inside "design"
    dtb().args(["call", "TRIAL.ERCURVE", "--file", "design=examples/data/trial-design.json", "--set", "points=10", "--result"]).assert().success();
}

#[test]
fn the_same_call_gives_the_same_bytes_on_any_thread_count() {
    let run = |threads: &str| {
        dtb()
            .args(["--threads", threads, "trial", "run", "--file", "design=examples/data/trial-design.json", "--set", "trials=800", "--set", "seed=5"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone()
    };
    let a = run("1");
    let b = run("4");
    assert_eq!(a, b);
    let v = json(&a);
    assert_eq!(v["dtb"]["seed"], 5);
    assert_eq!(v["dtb"]["inputs"][0]["path"], "examples/data/trial-design.json");
    assert_eq!(v["dtb"]["inputs"][0]["sha256"].as_str().unwrap().len(), 64);
    assert!(v["dtb"].get("run").is_none(), "no timing unless asked for");
}

#[test]
fn timing_is_opt_in() {
    let v = json(&dtb().args(["--timing", "stats", "describe", "--set", "values=[1,2]"]).assert().success().get_output().stdout.clone());
    assert!(v["dtb"]["run"]["elapsedMs"].is_number());
}

#[test]
fn errors_have_exit_codes_and_json() {
    dtb().args(["call", "nope"]).assert().code(2).stderr(predicate::str::contains("Unknown command"));
    let o = dtb().args(["call", "nope", "--json-errors"]).assert().code(2).get_output().stdout.clone();
    assert_eq!(json(&o)["error"]["kind"], "usage");
    dtb().args(["call", "stats.describe", "--input", "no-such-file.json"]).assert().code(4);
    dtb().args(["call", "trial.validate", "--set", "design=5"]).assert().failure();
    dtb().args(["call", "stats.describe", "--set", "values=[1,2]", "--set", "x"]).assert().code(2);
    dtb().args(["--bogus"]).assert().code(2);
}

#[test]
fn misspelt_parameters_are_refused() {
    let o = dtb().args(["power", "size", "--set", "cv=0.3", "--json-errors"]).assert().code(2).get_output().stdout.clone();
    let m = json(&o)["error"]["message"].as_str().unwrap().to_string();
    assert!(m.contains("no parameter \"cv\"") && m.contains("args"), "{m}");
    dtb()
        .args(["power", "size", "--set", "args.cv=0.3", "--set", "args.theta0=0.95", "--set", "args.design=2x2", "--result"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"n\""));
    dtb().args(["info", "--set", "x=1"]).assert().failure(); // not a flag of info
    dtb().args(["call", "info", "--set", "x=1"]).assert().code(2);
}

#[test]
fn limits_are_enforced() {
    let d = tempfile::tempdir().unwrap();
    let cfg = d.path().join("dtb.toml");
    fs::write(&cfg, "[limits]\nmax_trials = 100\n").unwrap();
    let o = Command::cargo_bin("dtb")
        .unwrap()
        .args(["--config", cfg.to_str().unwrap(), "trial", "run", "--file", "design=examples/data/trial-design.json", "--set", "trials=1000", "--json-errors"])
        .assert()
        .code(5)
        .get_output()
        .stdout
        .clone();
    assert_eq!(json(&o)["error"]["kind"], "limit");
}

#[test]
fn input_size_limit() {
    let d = tempfile::tempdir().unwrap();
    let cfg = d.path().join("dtb.toml");
    fs::write(&cfg, "[limits]\nmax_input_mb = 1\n").unwrap();
    let big = d.path().join("big.csv");
    fs::write(&big, vec![b'a'; 2 * 1_048_576]).unwrap();
    Command::cargo_bin("dtb").unwrap().args(["--config", cfg.to_str().unwrap(), "prep", "profile", "--data", big.to_str().unwrap()]).assert().code(5);
}

#[test]
fn stdin_requests_cannot_name_files() {
    dtb()
        .args(["call", "stats.describe", "--input", "-", "--json-errors"])
        .write_stdin(r#"{"values":{"$file":"Cargo.toml"}}"#)
        .assert()
        .code(2)
        .stdout(predicate::str::contains("not allowed"));
    dtb().args(["call", "stats.describe", "--input", "-", "--result"]).write_stdin(r#"{"values":[1,2,3]}"#).assert().success();
}

#[test]
fn unknown_configuration_keys_are_refused() {
    let d = tempfile::tempdir().unwrap();
    let cfg = d.path().join("dtb.toml");
    fs::write(&cfg, "[limits]\nmax_trails = 5\n").unwrap();
    Command::cargo_bin("dtb").unwrap().args(["--config", cfg.to_str().unwrap(), "info"]).assert().code(2).stderr(predicate::str::contains("max_trails"));
}

#[test]
fn config_init_show_check() {
    let d = tempfile::tempdir().unwrap();
    let cfg = d.path().join("c.toml");
    dtb().args(["config", "init", "--out", cfg.to_str().unwrap(), "-q"]).assert().success();
    dtb().args(["config", "init", "--out", cfg.to_str().unwrap(), "-q"]).assert().code(4); // does not overwrite
    dtb().args(["config", "init", "--out", cfg.to_str().unwrap(), "-q", "--force"]).assert().success();
    dtb().args(["config", "check", cfg.to_str().unwrap()]).assert().success().stdout(predicate::str::contains("configSha256"));
    let shown = String::from_utf8(dtb().args(["config", "show"]).assert().success().get_output().stdout.clone()).unwrap();
    assert!(shown.contains("max_input_mb = 256"));
}

#[test]
fn out_refuses_to_overwrite() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("o.json");
    let args = ["stats", "describe", "--set", "values=[1,2]", "-q", "-o", f.to_str().unwrap()];
    dtb().args(args).assert().success();
    dtb().args(args).assert().code(4);
    dtb().args(args).arg("--force").assert().success();
}

#[test]
fn extract_prints_text_as_it_is() {
    let d = tempfile::tempdir().unwrap();
    let csv = d.path().join("t.csv");
    fs::write(&csv, "ID,TIME,DV\n1,0,5\n1,1,3\n2,0,4\n").unwrap();
    let o = dtb().args(["prep", "export", "--data", csv.to_str().unwrap(), "--extract", "dataset"]).assert().success().get_output().stdout.clone();
    let text = String::from_utf8(o).unwrap();
    assert!(text.starts_with("ID"), "{text}");
}

#[test]
fn a_pipeline_runs_verifies_and_detects_changes() {
    let d = tempfile::tempdir().unwrap();
    let work = d.path().join("w");
    fs::create_dir_all(work.join("data")).unwrap();
    fs::copy("examples/data/crossover.csv", work.join("data/crossover.csv")).unwrap();
    fs::copy("examples/pipeline-be.toml", work.join("pipeline-be.toml")).unwrap();
    let p = work.join("pipeline-be.toml");
    let out = work.join("out");
    dtb().args(["run", p.to_str().unwrap(), "--out-dir", out.to_str().unwrap(), "-q"]).assert().success();
    assert!(out.join("manifest.json").is_file());
    assert!(out.join("cmax.json").is_file());
    // a second run into the same folder is refused, with --force it is the same bytes
    dtb().args(["run", p.to_str().unwrap(), "--out-dir", out.to_str().unwrap(), "-q"]).assert().code(4);
    let before = fs::read(out.join("manifest.json")).unwrap();
    dtb().args(["run", p.to_str().unwrap(), "--out-dir", out.to_str().unwrap(), "-q", "--force", "--threads", "2"]).assert().success();
    assert_eq!(before, fs::read(out.join("manifest.json")).unwrap());
    dtb().args(["verify", out.join("manifest.json").to_str().unwrap(), "--rerun"]).assert().success();
    // changing an input is found
    fs::write(work.join("data/crossover.csv"), "x").unwrap();
    dtb().args(["verify", out.join("manifest.json").to_str().unwrap()]).assert().code(6).stdout(predicate::str::contains("has changed"));
}

#[test]
fn pipeline_outputs_stay_inside_the_output_folder() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("p.toml");
    fs::write(
        &p,
        "[[step]]\nid = \"a\"\ncommand = \"stats.describe\"\n[step.params]\nvalues = [1, 2, 3]\n[[step.write]]\nfrom = \"\"\nto = \"../escape.json\"\n",
    )
    .unwrap();
    dtb().args(["run", p.to_str().unwrap(), "-q"]).assert().code(2);
    assert!(!d.path().join("escape.json").exists());
}

#[test]
fn pipeline_references_use_earlier_results() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("p.toml");
    fs::write(
        &p,
        "[[step]]\nid = \"d\"\ncommand = \"stats.describe\"\n[step.params]\nvalues = [1, 2, 3]\n\n[[step]]\nid = \"q\"\ncommand = \"stats.dist\"\n[step.params]\ndist = \"norm\"\nfn = \"cdf\"\nx = { \"$ref\" = \"d.mean\" }\n",
    )
    .unwrap();
    dtb().args(["run", p.to_str().unwrap(), "-q"]).assert().success();
    let v: Value = serde_json::from_slice(&fs::read(d.path().join("dtb-out/q.json")).unwrap()).unwrap();
    assert!(v["result"]["value"].is_number() || v["result"].is_number() || v["result"].is_object(), "{v}");
}

#[test]
fn pipeline_mistakes_are_reported() {
    let d = tempfile::tempdir().unwrap();
    let run = |text: &str| {
        let p = d.path().join("p.toml");
        fs::write(&p, text).unwrap();
        dtb().args(["run", p.to_str().unwrap(), "-q", "--force", "--out-dir", d.path().join("o").to_str().unwrap()]).assert()
    };
    run("").code(2);
    run("[[step]]\nid = \"a\"\ncommand = \"stats.describe\"\n[[step]]\nid = \"a\"\ncommand = \"stats.describe\"\n").code(2);
    run("[[step]]\nid = \"a\"\ncommand = \"nope\"\n").code(6);
    run("[[step]]\nid = \"a\"\ncommand = \"stats.describe\"\ntypo = 1\n").code(2);
    run("[[step]]\nid = \"a\"\ncommand = \"stats.describe\"\n[step.params]\nvalues = { \"$ref\" = \"zzz.x\" }\n").code(2);
}

#[test]
fn the_sample_size_page_in_one_call() {
    let o = dtb()
        .args([
            "power",
            "plan",
            "--set",
            "cvT=0.3",
            "--set",
            "theta0=0.95",
            "--set",
            "sequences=[\"TR\",\"RT\"]",
            "--set",
            "dropout=0.1",
            "--set",
            "nCheck=20",
            "--result",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v = json(&o);
    assert_eq!(v["n"], 40.0);
    assert!(v["enrol"].as_f64().unwrap() >= 44.0);
    assert_eq!(v["table"]["rows"].as_array().unwrap().len(), 5);
    assert!(v["check"]["power"].as_f64().unwrap() > 0.4);
    dtb()
        .args(["power", "plan", "--set", "cvT=0.3", "--set", "theta0=0.95", "--set", "method=rsabe", "--set", "sequences=null", "--json-errors"])
        .assert()
        .code(3);
}

#[test]
fn a_csv_file_is_checked_and_profiled() {
    let d = tempfile::tempdir().unwrap();
    let csv = d.path().join("d.csv");
    fs::write(&csv, "ID,TIME,AMT,DV,EVID\n1,0,100,.,1\n1,1,.,5.2,0\n1,0.5,.,4,0\n2,0,100,.,1\n2,2,.,3.1,0\n").unwrap();
    let v = json(&dtb().args(["prep", "qc", "--data", csv.to_str().unwrap(), "--result"]).assert().success().get_output().stdout.clone());
    assert!(v["findings"].as_array().unwrap().iter().any(|f| f["level"] != "ok"), "TIME goes backwards for ID 1");
    let p = json(&dtb().args(["prep", "profile", "--data", csv.to_str().unwrap(), "--result"]).assert().success().get_output().stdout.clone());
    assert_eq!(p["profile"].as_array().unwrap().len(), 5);
}
