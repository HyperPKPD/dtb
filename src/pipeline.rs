//! Pipelines: a TOML file lists steps (commands with parameters). Running it writes every step's result, any requested files and a
//! manifest with the SHA-256 of the pipeline, the configuration, every input, every result and every file written. `dtb verify` checks
//! a manifest against the files, and with `--rerun` re-runs the pipeline and compares the results.

use crate::commands::Ctx;
use crate::core::config::Config;
use crate::core::error::{DtbError, Result};
use crate::core::hash::{sha256_bytes, sha256_json};
use crate::core::io::{confined, read_text, sha256_ref, write_file, InputRef};
use crate::core::provenance::{PROTOCOL, VERSION};
use crate::runner::{execute, pick, resolve_refs};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub seed: Option<u64>,
    pub out_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteSpec {
    /// The part of the step's result to write, "a.b" (empty: the whole result).
    pub from: String,
    /// The file name inside the output folder.
    pub to: String,
    /// "text" (a string, written as it is), "json" or "csv" (a table of the data engine).
    pub format: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepSpec {
    pub id: String,
    pub command: String,
    pub enabled: Option<bool>,
    pub note: Option<String>,
    pub params: Option<toml::Value>,
    #[serde(default)]
    pub write: Vec<WriteSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pipeline {
    pub name: Option<String>,
    pub description: Option<String>,
    pub settings: Option<Settings>,
    #[serde(default)]
    pub step: Vec<StepSpec>,
}

pub fn parse(text: &str, what: &str) -> Result<Pipeline> {
    let p: Pipeline = toml::from_str(text).map_err(|e| DtbError::Usage(format!("{what}: {}", e.message().trim())))?;
    if p.step.is_empty() {
        return Err(DtbError::Usage(format!("{what}: the pipeline has no [[step]].")));
    }
    let mut seen = std::collections::HashSet::new();
    for s in &p.step {
        if s.id.is_empty() || !s.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err(DtbError::Usage(format!("{what}: the step id \"{}\" may only hold letters, digits, - and _.", s.id)));
        }
        if !seen.insert(s.id.clone()) {
            return Err(DtbError::Usage(format!("{what}: two steps are called \"{}\".", s.id)));
        }
    }
    Ok(p)
}

/// The path of `to` relative to the folder `from` (both absolute).
fn relative(from: &Path, to: &Path) -> String {
    let f: Vec<Component> = from.components().collect();
    let t: Vec<Component> = to.components().collect();
    let common = f.iter().zip(&t).take_while(|(a, b)| a == b).count();
    let mut out = PathBuf::new();
    for _ in common..f.len() {
        out.push("..");
    }
    for c in &t[common..] {
        out.push(c.as_os_str());
    }
    if out.as_os_str().is_empty() {
        ".".into()
    } else {
        out.to_string_lossy().replace('\\', "/")
    }
}

fn render(v: &Value, format: &str, cfg: &Config) -> Result<Vec<u8>> {
    match format {
        "text" => v.as_str().map(|s| s.as_bytes().to_vec()).ok_or_else(|| DtbError::Input("format \"text\" needs a text value.".into())),
        "json" => Ok(format!("{}\n", serde_json::to_string_pretty(v)?).into_bytes()),
        "csv" => {
            let f = crate::prep::table::Frame::from_json(v).map_err(DtbError::Input)?;
            Ok(crate::prep::export::dataset_text(&f, "comma", &cfg.prep.missing, true).into_bytes())
        }
        o => Err(DtbError::Usage(format!("write format must be text, json or csv, not \"{o}\"."))),
    }
}

pub struct RunReport {
    pub out_dir: PathBuf,
    pub manifest: Value,
    pub steps: Vec<(String, String)>,
}

/// Run a pipeline file. Results go to `out_dir` (default: `dtb-out` next to the pipeline file).
pub fn run(file: &Path, out_dir: Option<&Path>, mut config: Config, force: bool) -> Result<RunReport> {
    let pdir = file.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let max_bytes = config.limits.max_input_mb * 1_048_576;
    let (text, pref) = read_text(&file.to_string_lossy(), max_bytes)?;
    let pipe = parse(&text, &file.display().to_string())?;
    super::commands::Ctx::new(config.clone(), false, pdir.clone()).limit("steps", pipe.step.len(), config.limits.max_steps, "max_steps")?;
    if let Some(s) = pipe.settings.as_ref().and_then(|s| s.seed) {
        config.general.seed = Some(s);
    }
    let out = match (out_dir, pipe.settings.as_ref().and_then(|s| s.out_dir.as_deref())) {
        (Some(o), _) => o.to_path_buf(),
        (None, Some(s)) => pdir.join(s),
        (None, None) => pdir.join("dtb-out"),
    };
    std::fs::create_dir_all(&out).map_err(|e| DtbError::Io(format!("Cannot create {}: {e}", out.display())))?;
    let config_sha = config.sha256();
    let mut results: HashMap<String, Value> = HashMap::new();
    let mut steps_out: Vec<Value> = vec![];
    let mut summary = vec![];
    for s in &pipe.step {
        if s.enabled == Some(false) {
            continue;
        }
        let ctx = Ctx::new(config.clone(), false, pdir.clone());
        let mut params: Value = match &s.params {
            Some(t) => serde_json::to_value(t).map_err(|e| DtbError::Usage(format!("step \"{}\": {e}", s.id)))?,
            None => json!({}),
        };
        resolve_refs(&mut params, &ctx, &results, true, 0)?;
        let o = execute(&s.command, &params, &ctx).map_err(|e| DtbError::Verify(format!("Step \"{}\" ({}) failed: {e}", s.id, s.command)))?;
        let env_bytes = format!("{}\n", serde_json::to_string_pretty(&o.envelope)?).into_bytes();
        let output = write_file(&confined(&out, &format!("{}.json", s.id))?, &env_bytes, force)?;
        let mut files: Vec<Value> = vec![];
        for w in &s.write {
            let v = pick(&o.result, &w.from).ok_or_else(|| DtbError::Input(format!("Step \"{}\": the result has nothing at \"{}\".", s.id, w.from)))?;
            let bytes = render(v, w.format.as_deref().unwrap_or(if v.is_string() { "text" } else { "json" }), &config)?;
            let r = write_file(&confined(&out, &w.to)?, &bytes, force)?;
            files.push(json!({ "path": w.to, "sha256": r.sha256, "bytes": r.bytes }));
        }
        let rel = |r: &InputRef| json!({ "path": r.path, "sha256": r.sha256, "bytes": r.bytes });
        steps_out.push(json!({
            "id": s.id, "command": s.command, "note": s.note,
            "paramsSha256": o.params_sha256,
            "inputs": o.inputs.iter().map(rel).collect::<Vec<_>>(),
            "resultSha256": sha256_json(&o.result),
            "output": { "path": format!("{}.json", s.id), "sha256": output.sha256, "bytes": output.bytes },
            "files": files,
        }));
        summary.push((s.id.clone(), s.command.clone()));
        results.insert(s.id.clone(), o.result);
    }
    let manifest = json!({
        "format": "dtb-manifest", "version": 1,
        "dtb": { "version": VERSION, "protocol": PROTOCOL },
        "pipeline": { "name": pipe.name, "description": pipe.description, "file": file.file_name().map(|f| f.to_string_lossy().to_string()), "base": relative(&std::fs::canonicalize(&out).unwrap_or(out.clone()), &std::fs::canonicalize(&pdir).unwrap_or(pdir.clone())), "sha256": pref.sha256 },
        "configSha256": config_sha, "seed": config.general.seed,
        "steps": steps_out,
    });
    let mut text = serde_json::to_string_pretty(&manifest)?;
    text.push('\n');
    write_file(&out.join("manifest.json"), text.as_bytes(), force)?;
    Ok(RunReport { out_dir: out, manifest, steps: summary })
}

/// Check a manifest against the files it names; with `rerun`, run the pipeline again into a temporary folder and compare every result.
pub fn verify(manifest_path: &Path, rerun: bool, config: Config) -> Result<Value> {
    let max_bytes = config.limits.max_input_mb * 1_048_576;
    let (text, _) = read_text(&manifest_path.to_string_lossy(), max_bytes)?;
    let m: Value = serde_json::from_str(&text).map_err(|e| DtbError::Input(format!("{} is not valid JSON: {e}", manifest_path.display())))?;
    if m["format"] != "dtb-manifest" {
        return Err(DtbError::Input(format!("{} is not a dtb manifest.", manifest_path.display())));
    }
    let dir = manifest_path.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let base = dir.join(m["pipeline"]["base"].as_str().unwrap_or("."));
    let mut problems: Vec<String> = vec![];
    let mut checked = 0usize;
    fn check(problems: &mut Vec<String>, checked: &mut usize, label: String, path: &Path, want: &Value) {
        *checked += 1;
        match sha256_ref(path) {
            Ok((got, _)) if Some(got.as_str()) == want.as_str() => {}
            Ok(_) => problems.push(format!("{label}: {} has changed.", path.display())),
            Err(e) => problems.push(format!("{label}: cannot read {} ({e}).", path.display())),
        }
    }
    if let Some(f) = m["pipeline"]["file"].as_str() {
        check(&mut problems, &mut checked, "pipeline".into(), &base.join(f), &m["pipeline"]["sha256"]);
    }
    if m["dtb"]["protocol"].as_u64() != Some(PROTOCOL as u64) {
        problems.push(format!("the manifest was written with protocol {}, this dtb has protocol {PROTOCOL}.", m["dtb"]["protocol"]));
    }
    let mut our = config.clone();
    our.general.seed = m["seed"].as_u64();
    let our_config = our.sha256();
    let mut config_note = None;
    if m["configSha256"].as_str() != Some(our_config.as_str()) {
        config_note = Some("The configuration now in effect differs from the one that made the manifest; a re-run may not reproduce it.".to_string());
    }
    for s in m["steps"].as_array().cloned().unwrap_or_default() {
        let id = s["id"].as_str().unwrap_or("?");
        for i in s["inputs"].as_array().cloned().unwrap_or_default() {
            let p = i["path"].as_str().unwrap_or("");
            let full = if Path::new(p).is_absolute() { PathBuf::from(p) } else { base.join(p) };
            check(&mut problems, &mut checked, format!("step {id} input"), &full, &i["sha256"]);
        }
        check(&mut problems, &mut checked, format!("step {id} output"), &dir.join(s["output"]["path"].as_str().unwrap_or("")), &s["output"]["sha256"]);
        for f in s["files"].as_array().cloned().unwrap_or_default() {
            check(&mut problems, &mut checked, format!("step {id} file"), &dir.join(f["path"].as_str().unwrap_or("")), &f["sha256"]);
        }
    }
    let mut rerun_report = Value::Null;
    if rerun && problems.is_empty() {
        let tmp = std::env::temp_dir().join(format!("dtb-verify-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let pfile = base.join(m["pipeline"]["file"].as_str().unwrap_or("pipeline.toml"));
        let mut cfg = config.clone();
        cfg.general.seed = m["seed"].as_u64();
        let r = run(&pfile, Some(&tmp), cfg, true);
        let mut diffs = vec![];
        match r {
            Ok(rep) => {
                let now = rep.manifest["steps"].as_array().cloned().unwrap_or_default();
                for (a, b) in m["steps"].as_array().cloned().unwrap_or_default().iter().zip(&now) {
                    checked += 1;
                    if a["resultSha256"] != b["resultSha256"] {
                        diffs.push(format!("step {}: the result is not the same when run again.", a["id"].as_str().unwrap_or("?")));
                    }
                }
                if now.len() != m["steps"].as_array().map_or(0, Vec::len) {
                    diffs.push("the number of steps differs.".into());
                }
            }
            Err(e) => diffs.push(format!("the pipeline could not be run again: {e}")),
        }
        let _ = std::fs::remove_dir_all(&tmp);
        problems.extend(diffs);
        rerun_report = json!(true);
    }
    Ok(
        json!({ "ok": problems.is_empty(), "checked": checked, "problems": problems, "rerun": rerun_report, "note": config_note, "digest": sha256_bytes(text.as_bytes()) }),
    )
}
