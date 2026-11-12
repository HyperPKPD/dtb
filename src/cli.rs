//! The command line.

use crate::commands::{self, info, Ctx};
use crate::core::config::{Config, EXAMPLE, FILE_NAME};
use crate::core::error::{DtbError, Result};
use crate::core::io::{read_limited, write_file};
use crate::core::provenance::{error_envelope, ENGINE, PROTOCOL, VERSION};
use crate::{pipeline, runner};
use clap::{Args, Parser, Subcommand};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const GROUPS: &[&str] = &["prep", "trial", "be", "power", "ddi", "nca", "renal", "hepatic", "stats"];

#[derive(Parser, Debug)]
#[command(
    name = "dtb",
    version = VERSION,
    about = "dtb: the computation engine behind Prep Studio, Trial Simulator and BE & DDI Studio.",
    long_about = "dtb: the computation engine behind the nlmx apps Prep Studio (PREP), Trial Simulator (TRIAL) and BE & DDI Studio (BEDDI).\n\
Every command reads JSON parameters and writes one JSON document (the result with a provenance record). The same seed, configuration and inputs always give the same bytes, on any machine and any number of threads.",
    after_help = "Command groups (shortcuts for `call GROUP.COMMAND`): prep, trial, be, power, ddi, nca, renal, hepatic, stats.\n  dtb trial run --input design.json --set trials=2000\n  dtb be analyze --data study.csv --set map.subject=SUBJ\n  dtb run pipeline.toml        dtb verify dtb-out/manifest.json --rerun\nRun `dtb commands` for every command and `dtb describe COMMAND` for its parameters.\nEnvironment: DTB_CONFIG, DTB_THREADS, DTB_SEED, DTB_TIMING, DTB_PRETTY.\nExit codes: 0 ok, 2 usage, 3 input, 4 file, 5 limit, 6 verification, 70 internal.",
    propagate_version = true
)]
pub struct Cli {
    /// Configuration file (default: $DTB_CONFIG, then ./dtb.toml).
    #[arg(long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,
    /// Ignore any configuration file and use the built-in defaults.
    #[arg(long, global = true, conflicts_with = "config")]
    pub no_config: bool,
    /// Worker threads (0 = all cores). Results do not depend on it.
    #[arg(long, global = true, env = "DTB_THREADS", value_name = "N")]
    pub threads: Option<usize>,
    /// Default random seed for calls that give none.
    #[arg(long, global = true, env = "DTB_SEED", value_name = "N")]
    pub seed: Option<u64>,
    /// Add elapsed time and thread count to the output (the output then differs between runs).
    #[arg(long, global = true, env = "DTB_TIMING")]
    pub timing: bool,
    /// Indented JSON.
    #[arg(long, global = true, env = "DTB_PRETTY")]
    pub pretty: bool,
    /// Write the output to a file instead of standard output (refuses to overwrite).
    #[arg(short, long, global = true, value_name = "FILE")]
    pub out: Option<PathBuf>,
    /// Allow --out to overwrite, and pipelines to replace files in their output folder.
    #[arg(long, global = true)]
    pub force: bool,
    /// Report errors as a JSON document on standard output (for programs calling dtb).
    #[arg(long, global = true)]
    pub json_errors: bool,
    /// No messages on standard error.
    #[arg(short, long, global = true)]
    pub quiet: bool,
    #[command(subcommand)]
    pub command: Cmd,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Version, protocol, build, platform, limits and the effective configuration.
    Info,
    /// Self-test: known answers of the generators and engines, and the determinism of parallel runs.
    Doctor,
    /// List the commands (optionally only those of one group).
    Commands {
        /// A group such as trial, or part of a name.
        filter: Option<String>,
    },
    /// Describe one command and its parameters.
    Describe { command: String },
    /// Show, locate, create or check the configuration.
    #[command(subcommand)]
    Config(ConfigCmd),
    /// Run any command: `call trial.run --input design.json`.
    Call(CallArgs),
    /// Run a pipeline file and write its results and a manifest.
    Run {
        /// The pipeline (TOML).
        pipeline: PathBuf,
        /// Output folder (default: `dtb-out` next to the pipeline, or settings.out_dir).
        #[arg(long, value_name = "DIR")]
        out_dir: Option<PathBuf>,
    },
    /// Check a manifest against its files; with --rerun, run the pipeline again and compare.
    Verify {
        manifest: PathBuf,
        /// Run the pipeline again in a temporary folder and compare every result.
        #[arg(long)]
        rerun: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// Print the effective configuration (TOML, or JSON with --json).
    Show {
        #[arg(long)]
        json: bool,
    },
    /// Print the path of the configuration file in use.
    Path,
    /// Write a commented example configuration (to ./dtb.toml, or --out).
    Init,
    /// Check a configuration file and print its hash.
    Check { file: Option<PathBuf> },
}

#[derive(Args, Debug)]
pub struct CallArgs {
    /// The command, such as `trial.run` (`dtb commands` lists them; case and dashes do not matter).
    pub command: String,
    /// The parameters as a JSON object in a file, or `-` for standard input.
    #[arg(short, long, value_name = "FILE|-")]
    pub input: Option<String>,
    /// Put the content of a file in a parameter: KEY=PATH. A .json file is read as JSON, anything else as text. Repeatable.
    #[arg(short, long = "file", value_name = "KEY=PATH")]
    pub files: Vec<String>,
    /// Shortcut for `--file table=PATH` (a data table as CSV/TSV text).
    #[arg(short, long, value_name = "FILE")]
    pub data: Option<String>,
    /// Set a parameter: KEY=VALUE (a.b.c for nested; VALUE is JSON when it parses as JSON, otherwise text). Repeatable.
    #[arg(short, long = "set", value_name = "KEY=VALUE")]
    pub sets: Vec<String>,
    /// Print only the result, without the provenance record.
    #[arg(long)]
    pub result: bool,
    /// Print only the part of the result at this path (a.b.0); a text value is printed as it is.
    #[arg(long, value_name = "PATH")]
    pub extract: Option<String>,
}

pub struct Setup {
    pub config: Config,
    pub config_path: Option<PathBuf>,
}

fn setup(cli: &Cli) -> Result<Setup> {
    let path = if cli.no_config { None } else { Config::locate(cli.config.as_deref()) };
    if let (Some(p), None) = (&cli.config, &path) {
        return Err(DtbError::Usage(format!("{} not found.", p.display())));
    }
    let (mut config, used, _) = Config::load(path.as_deref())?;
    if let Some(t) = cli.threads {
        config.general.threads = t;
    }
    if let Some(s) = cli.seed {
        config.general.seed = Some(s);
    }
    if cli.timing {
        config.general.timing = true;
    }
    if cli.pretty {
        config.general.pretty = true;
    }
    Ok(Setup { config, config_path: used })
}

fn init_threads(n: usize) {
    if n > 0 {
        // the global pool can be built once per process; a failure only means it exists already
        let _ = rayon::ThreadPoolBuilder::new().num_threads(n).build_global();
    }
}

fn to_text(v: &Value, pretty: bool) -> Result<String> {
    Ok(if pretty { serde_json::to_string_pretty(v)? } else { serde_json::to_string(v)? })
}

/// Print a document on standard output, or write it to `--out`.
fn emit(cli: &Cli, text: String) -> Result<()> {
    match &cli.out {
        Some(p) => {
            let r = write_file(p, format!("{text}\n").as_bytes(), cli.force)?;
            if !cli.quiet {
                eprintln!("dtb: wrote {} ({} bytes, sha256 {})", p.display(), r.bytes, r.sha256);
            }
        }
        None => {
            use std::io::Write;
            let mut o = std::io::stdout().lock();
            // a closed pipe (`dtb ... | head`) is not an error worth reporting
            let _ = writeln!(o, "{text}").and_then(|_| o.flush());
        }
    }
    Ok(())
}

fn split_pair<'a>(s: &'a str, what: &str) -> Result<(&'a str, &'a str)> {
    s.split_once('=').filter(|(k, _)| !k.is_empty()).ok_or_else(|| DtbError::Usage(format!("{what} wants KEY=VALUE, not \"{s}\".")))
}

fn call(cli: &Cli, st: &Setup, a: &CallArgs) -> Result<()> {
    let ctx = Ctx::new(st.config.clone(), st.config.general.timing, std::env::current_dir().unwrap_or_default());
    let mut params = match &a.input {
        Some(p) => {
            let b = read_limited(p, ctx.max_bytes())?;
            let text = String::from_utf8(b).map_err(|_| DtbError::Input("The parameters are not UTF-8 text.".into()))?;
            let v: Value =
                serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| DtbError::Input(format!("The parameters are not valid JSON: {e}")))?;
            if !v.is_object() {
                return Err(DtbError::Usage("The parameters must be a JSON object.".into()));
            }
            v
        }
        None => json!({}),
    };
    // a request on standard input may come from a program that relays a user's request: it must not be able to name files
    let files_allowed = a.input.as_deref() != Some("-");
    runner::resolve_refs(&mut params, &ctx, &Default::default(), files_allowed, 0)?;
    if let Some(d) = &a.data {
        runner::set_path(&mut params, "table", runner::read_param_file(d, &ctx)?)?;
    }
    for f in &a.files {
        let (k, p) = split_pair(f, "--file")?;
        runner::set_path(&mut params, k, runner::read_param_file(p, &ctx)?)?;
    }
    for s in &a.sets {
        let (k, v) = split_pair(s, "--set")?;
        runner::set_path(&mut params, k, runner::parse_value(v))?;
    }
    let o = runner::execute(&a.command, &params, &ctx)?;
    let doc = if let Some(path) = &a.extract {
        let v = runner::pick(&o.result, path).ok_or_else(|| DtbError::Input(format!("The result has nothing at \"{path}\".")))?;
        if let Some(s) = v.as_str() {
            return match &cli.out {
                Some(_) => emit(cli, s.trim_end_matches('\n').to_string()),
                None => {
                    use std::io::Write;
                    let _ = std::io::stdout().lock().write_all(s.as_bytes());
                    Ok(())
                }
            };
        }
        to_text(v, st.config.general.pretty)?
    } else if a.result {
        to_text(&o.result, st.config.general.pretty)?
    } else {
        to_text(&o.envelope, st.config.general.pretty)?
    };
    emit(cli, doc)
}

fn describe(name: &str) -> Result<Value> {
    let c = commands::resolve(name).ok_or_else(|| DtbError::Usage(format!("Unknown command \"{name}\". Run `dtb commands` for the list.")))?;
    Ok(
        json!({ "name": c.name, "summary": c.summary, "params": c.params.iter().map(|(n, d)| json!({"name": n.trim_end_matches('?'), "required": !n.ends_with('?'), "description": d})).collect::<Vec<_>>() }),
    )
}

fn list_commands(filter: Option<&str>) -> String {
    let mut out = String::new();
    let all = commands::all();
    let w = all.iter().map(|c| c.name.len()).max().unwrap_or(0);
    for c in all.iter().filter(|c| filter.is_none_or(|f| c.name.starts_with(&format!("{f}.")) || c.name == f || c.name.contains(f))) {
        out.push_str(&format!("{:<w$}  {}\n", c.name, c.summary));
    }
    out
}

pub fn run(cli: Cli) -> Result<()> {
    let st = setup(&cli)?;
    init_threads(st.config.general.threads);
    let pretty = st.config.general.pretty;
    match &cli.command {
        Cmd::Info => {
            let ctx = Ctx::new(st.config.clone(), false, PathBuf::from("."));
            let o = runner::execute("info", &json!({}), &ctx)?;
            let mut doc = o.envelope;
            doc["result"]["configFile"] = json!(st.config_path.as_ref().map(|p| p.display().to_string()));
            emit(&cli, to_text(&doc, pretty)?)
        }
        Cmd::Doctor => {
            let checks = info::run_doctor();
            let ok = checks.iter().all(|c| c["ok"] == true);
            let doc = json!({ "dtb": { "version": VERSION, "protocol": PROTOCOL, "command": "doctor" }, "result": { "ok": ok, "engine": ENGINE, "threads": rayon::current_num_threads(), "checks": checks } });
            emit(&cli, to_text(&doc, pretty)?)?;
            if ok {
                Ok(())
            } else {
                Err(DtbError::Verify("The self-test failed.".into()))
            }
        }
        Cmd::Commands { filter } => {
            let text = list_commands(filter.as_deref());
            if text.is_empty() {
                return Err(DtbError::Usage(format!("No command matches \"{}\".", filter.as_deref().unwrap_or(""))));
            }
            emit(&cli, text.trim_end().to_string())
        }
        Cmd::Describe { command } => {
            let d = describe(command)?;
            emit(&cli, to_text(&d, true)?)
        }
        Cmd::Config(c) => match c {
            ConfigCmd::Show { json } => {
                if *json {
                    emit(&cli, to_text(&serde_json::to_value(&st.config)?, pretty)?)
                } else {
                    emit(&cli, st.config.to_toml().trim_end().to_string())
                }
            }
            ConfigCmd::Path => match &st.config_path {
                Some(p) => emit(&cli, p.display().to_string()),
                None => {
                    if !cli.quiet {
                        eprintln!("dtb: no configuration file; the built-in defaults are in effect.");
                    }
                    Ok(())
                }
            },
            ConfigCmd::Init => {
                let target = cli.out.clone().unwrap_or_else(|| PathBuf::from(FILE_NAME));
                let r = write_file(&target, EXAMPLE.as_bytes(), cli.force)?;
                if !cli.quiet {
                    eprintln!("dtb: wrote {} ({} bytes)", target.display(), r.bytes);
                }
                Ok(())
            }
            ConfigCmd::Check { file } => {
                let path = file.clone().or_else(|| st.config_path.clone());
                let Some(p) = path else {
                    return Err(DtbError::Usage("There is no configuration file to check (give one, or create dtb.toml).".into()));
                };
                let (_, _, sha) = Config::load(Some(&p))?;
                emit(&cli, to_text(&json!({ "file": p.display().to_string(), "ok": true, "configSha256": sha }), pretty)?)
            }
        },
        Cmd::Call(a) => call(&cli, &st, a),
        Cmd::Run { pipeline: file, out_dir } => {
            let rep = pipeline::run(file, out_dir.as_deref(), st.config.clone(), cli.force)?;
            if !cli.quiet {
                for (id, c) in &rep.steps {
                    eprintln!("dtb: step {id} ({c}) done");
                }
                eprintln!("dtb: results and manifest.json are in {}", rep.out_dir.display());
            }
            emit(&cli, to_text(&json!({ "outDir": rep.out_dir.display().to_string(), "manifest": rep.manifest }), pretty)?)
        }
        Cmd::Verify { manifest, rerun } => {
            let r = pipeline::verify(Path::new(manifest), *rerun, st.config.clone())?;
            let ok = r["ok"] == true;
            emit(&cli, to_text(&r, pretty)?)?;
            if ok {
                Ok(())
            } else {
                Err(DtbError::Verify("Verification failed: see the problems above.".into()))
            }
        }
    }
}

/// Rewrite `dtb trial run ...` into `dtb call trial.run ...` and `dtb trial` into `dtb commands trial`.
pub fn rewrite_groups(args: Vec<String>) -> Vec<String> {
    let with_value = ["--config", "--threads", "--seed", "--out", "-o"];
    let mut i = 1;
    while i < args.len() {
        let a = &args[i];
        if a.starts_with('-') {
            i += if with_value.contains(&a.as_str()) { 2 } else { 1 };
            continue;
        }
        if GROUPS.contains(&a.as_str()) {
            let mut v = args.clone();
            match args.get(i + 1).filter(|n| !n.starts_with('-')) {
                Some(next) => {
                    v[i] = "call".into();
                    v[i + 1] = format!("{a}.{next}");
                }
                None => {
                    v[i] = "commands".into();
                    v.insert(i + 1, a.clone());
                }
            }
            return v;
        }
        return args;
    }
    args
}

pub fn error_json(command: &str, e: &DtbError) -> String {
    serde_json::to_string(&error_envelope(command, e.kind(), &e.to_string(), e.exit_code())).unwrap_or_default()
}
