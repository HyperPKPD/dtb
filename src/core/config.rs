//! Configuration: a TOML file (`dtb.toml`) that sets defaults for the safety limits, threads, seed and engine choices. Unknown keys are
//! an error, so that a misspelt setting is never silently ignored. Precedence, highest first: command-line options, the parameters of
//! the call, environment variables (`DTB_*`), the configuration file, built-in defaults.

use super::error::{DtbError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[derive(Default)]
pub struct General {
    /// Worker threads; 0 means all cores. The results do not depend on it.
    pub threads: usize,
    /// Default seed when neither the call nor the command line gives one.
    pub seed: Option<u64>,
    /// Report elapsed time and threads in the output (makes the output differ between runs).
    pub timing: bool,
    /// Indented JSON.
    pub pretty: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub max_input_mb: u64,
    pub max_rows: usize,
    pub max_columns: usize,
    pub max_trials: usize,
    pub max_scenarios: usize,
    pub max_sims: usize,
    pub max_points: usize,
    pub max_subjects: usize,
    pub max_steps: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_input_mb: 256,
            max_rows: 5_000_000,
            max_columns: 2_000,
            max_trials: 1_000_000,
            max_scenarios: 100,
            max_sims: 5_000_000,
            max_points: 5_000,
            max_subjects: 200_000,
            max_steps: 500,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TrialCfg {
    /// "blocks" (default: independent random streams per block of trials, parallel, the same on any number of threads) or "js"
    /// (one sequential stream, the same numbers as the browser engine for a given seed).
    pub streams: String,
    pub trials: usize,
}
impl Default for TrialCfg {
    fn default() -> Self {
        TrialCfg { streams: "blocks".into(), trials: 1000 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PowerCfg {
    pub streams: String,
    pub sims: usize,
}
impl Default for PowerCfg {
    fn default() -> Self {
        PowerCfg { streams: "blocks".into(), sims: 10_000 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PrepCfg {
    /// "auto", "comma", "tab", "semicolon", "pipe" or "space".
    pub delimiter: String,
    /// What a missing value is written as in exported datasets (NONMEM reads ".").
    pub missing: String,
}
impl Default for PrepCfg {
    fn default() -> Self {
        PrepCfg { delimiter: "auto".into(), missing: ".".into() }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub general: General,
    pub limits: Limits,
    pub trial: TrialCfg,
    pub power: PowerCfg,
    pub prep: PrepCfg,
}

pub const FILE_NAME: &str = "dtb.toml";

impl Config {
    pub fn parse(text: &str, what: &str) -> Result<Config> {
        let c: Config = toml::from_str(text).map_err(|e| DtbError::Usage(format!("{what}: {}", e.message().trim())))?;
        c.validate(what)?;
        Ok(c)
    }

    pub fn validate(&self, what: &str) -> Result<()> {
        let bad = |m: &str| Err(DtbError::Usage(format!("{what}: {m}")));
        for (k, v) in [("trial.streams", &self.trial.streams), ("power.streams", &self.power.streams)] {
            if v != "blocks" && v != "js" {
                return bad(&format!("{k} must be \"blocks\" or \"js\", not \"{v}\"."));
            }
        }
        if !["auto", "comma", "tab", "semicolon", "pipe", "space"].contains(&self.prep.delimiter.as_str()) {
            return bad(&format!("prep.delimiter must be auto, comma, tab, semicolon, pipe or space, not \"{}\".", self.prep.delimiter));
        }
        let l = &self.limits;
        if l.max_input_mb == 0
            || l.max_rows == 0
            || l.max_columns == 0
            || l.max_trials == 0
            || l.max_scenarios == 0
            || l.max_sims == 0
            || l.max_points == 0
            || l.max_subjects == 0
            || l.max_steps == 0
        {
            return bad("every limit must be at least 1.");
        }
        if self.trial.trials == 0 || self.power.sims == 0 {
            return bad("trial.trials and power.sims must be at least 1.");
        }
        Ok(())
    }

    /// Where the configuration comes from, in order: an explicit path, `DTB_CONFIG`, `./dtb.toml`. `None` when there is none.
    pub fn locate(explicit: Option<&Path>) -> Option<PathBuf> {
        if let Some(p) = explicit {
            return Some(p.to_path_buf());
        }
        if let Some(p) = std::env::var_os("DTB_CONFIG").filter(|v| !v.is_empty()) {
            return Some(PathBuf::from(p));
        }
        let local = PathBuf::from(FILE_NAME);
        local.is_file().then_some(local)
    }

    pub fn load(path: Option<&Path>) -> Result<(Config, Option<PathBuf>, String)> {
        match path {
            None => Ok((Config::default(), None, Config::default().sha256())),
            Some(p) => {
                let text = std::fs::read_to_string(p).map_err(|e| DtbError::Io(format!("Cannot read the configuration {}: {e}", p.display())))?;
                let c = Config::parse(&text, &p.display().to_string())?;
                // the hash is of the effective settings (not of the file text), so comments and spacing do not change it
                let sha = c.sha256();
                Ok((c, Some(p.to_path_buf()), sha))
            }
        }
    }

    /// SHA-256 of the settings that can change a result. Threads, timing and layout never do, so they are left out.
    pub fn sha256(&self) -> String {
        let mut c = self.clone();
        c.general.threads = 0;
        c.general.timing = false;
        c.general.pretty = false;
        super::hash::sha256_json(&serde_json::to_value(&c).unwrap_or(Value::Null))
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }
}

/// A commented example configuration with every setting at its default.
pub const EXAMPLE: &str = r#"# dtb configuration. Every setting is optional; the values below are the defaults.
# Precedence (highest first): command-line options, the parameters of a call, DTB_* environment variables, this file.
# Unknown keys are an error, so a misspelt setting is never silently ignored.

[general]
threads = 0          # worker threads; 0 = all cores. Results never depend on this.
# seed = 12345       # default random seed when a call gives none
timing = false       # add elapsed time and threads to the output (output then differs between runs)
pretty = false       # indented JSON

[limits]             # safety limits: a request above any of them is refused (exit code 5)
max_input_mb = 256   # largest input file or stdin
max_rows = 5000000   # rows of any table
max_columns = 2000
max_trials = 1000000 # simulated trials per scenario
max_scenarios = 100
max_sims = 5000000   # simulated studies for scaled bioequivalence power
max_points = 5000    # points of a curve
max_subjects = 200000
max_steps = 500      # steps of a recipe or pipeline

[trial]
streams = "blocks"   # "blocks": parallel, identical on any number of threads; "js": the same numbers as the browser engine
trials = 1000

[power]
streams = "blocks"
sims = 10000

[prep]
delimiter = "auto"   # auto, comma, tab, semicolon, pipe, space
missing = "."        # how a missing value is written in exported datasets
"#;
