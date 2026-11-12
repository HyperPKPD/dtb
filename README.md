# dtb

`dtb` is a single Rust program with no network access and no other files to install. The apps find it by themselves when they start and hand it the heavy work: simulations, bioequivalence models, dataset checks and recipes. Without it the apps keep working with their built-in JavaScript.

It is also a command-line tool in its own right. Everything the apps compute can be run from a terminal, from a script or from a pipeline file, with the same seed giving the same bytes on any machine.

Version 0.1.0 · protocol 1 · see [CHANGELOG.md](CHANGELOG.md), [DEPLOYMENT.md](DEPLOYMENT.md) and the [command reference](docs/COMMANDS.md).

## Contents

1. [Quick start](#quick-start)
2. [What it does](#what-it-does)
3. [Calling a command](#calling-a-command)
4. [Results and provenance](#results-and-provenance)
5. [Reproducibility](#reproducibility)
6. [Configuration](#configuration)
7. [Pipelines and manifests](#pipelines-and-manifests)
8. [Security and limits](#security-and-limits)
9. [Exit codes](#exit-codes)
10. [How the apps use it](#how-the-apps-use-it)
11. [Speed](#speed)
12. [How the files are organised](#how-the-files-are-organised)
13. [Tests](#tests)
14. [Versions](#versions)

## Quick start

```
cargo build --release
target\release\dtb.exe doctor
target\release\dtb.exe commands
```

Examples (from the `dtb` folder; on Linux and macOS use `/` and `target/release/dtb`):

```
# simulate a trial: 2000 trials, power and decisions per arm
dtb trial run --file design=examples\data\trial-design.json --set trials=2000 --set seed=7 --result

# bioequivalence of a crossover study
dtb be analyze --data examples\data\crossover.csv ^
    --set map.subject=SUBJ --set map.sequence=SEQ --set map.period=PER --set map.treatment=TRT ^
    --set param=Cmax --set options.test=T --set options.ref=R --result

# sample size for average bioequivalence
dtb power size --set args.cv=0.3 --set args.theta0=0.95 --set args.design=2x2 --result

# a whole analysis as a pipeline, then check it later
dtb run examples\pipeline-be.toml
dtb verify examples\out-be\manifest.json --rerun
```

## What it does

37 commands in nine groups. `dtb commands` lists them, `dtb describe COMMAND` shows the parameters of one, and [docs/COMMANDS.md](docs/COMMANDS.md) has them all.

| Group | Commands | For |
|---|---|---|
| `prep` | `profile` `qc` `run` `eval` `export` | Prep Studio: column profile, the 25+ NONMEM dataset checks, a whole recipe (21 steps, BLQ M1 and M3–M7), the expression language, dataset and `$INPUT` export |
| `trial` | `validate` `run` `scenarios` `tornado` `bands` `erCurve` | Trial Simulator: design checks, simulation of continuous, binary and time-to-event trials with uncertainty, scenarios, sensitivity, concentration bands, exposure–response |
| `be` | `analyze` `residuals` | BE & DDI Studio: ABE, ABEL, RSABE, NTID and non-parametric bioequivalence with ANOVA, carry-over and residuals |
| `power` | `designs` `abe` `size` `sim` `simSize` `table` `plan` | exact and simulated power, sample size with dropout, tables; `plan` is the whole sample-size page in one call |
| `ddi` | `presets` `predict` `steady` `dynamic` | static, steady-state and dynamic drug–drug interaction predictions |
| `nca` | `profile` `table` | non-compartmental analysis |
| `renal` `hepatic` | `egfr` `categories` `giusti` `groups` `regress` · `childPugh` `liver` | kidney and liver impairment |
| `stats` | `describe` `dist` | summaries and distribution functions |
| (general) | `info` `doctor` | version and build; self-test |

Command names ignore case and dashes: `trial.erCurve`, `trial er-curve` and `TRIAL.ERCURVE` are the same.

## Calling a command

Every command is a function from a JSON object of parameters to one JSON document. Two equivalent forms:

```
dtb call trial.run --input run.json
dtb trial run --input run.json            # group shortcut
```

Ways to give the parameters, combined in this order (later ones win):

| Option | Meaning |
|---|---|
| `-i, --input FILE` or `-` | the parameters as a JSON object, from a file or standard input |
| `-f, --file KEY=PATH` | put a file in a parameter. A `.json` file is read as JSON, anything else as text (a CSV table, a recipe is JSON). `KEY` may be nested, `design.arms.0`. Repeatable |
| `-d, --data FILE` | shortcut for `--file table=FILE` |
| `-s, --set KEY=VALUE` | set one parameter. `VALUE` is JSON if it parses as JSON (`5`, `true`, `[1,2]`, `{"a":1}`), otherwise text. `KEY` may be nested (`options.alpha=0.1`). Repeatable |

Inside a JSON parameter file a value `{ "$file": "path" }` is replaced by that file's content, as for `--file`. This is not allowed when the parameters come from standard input (see [Security](#security-and-limits)).

What is printed:

| Option | Meaning |
|---|---|
| (none) | the envelope `{ "dtb": {…}, "result": … }` |
| `--result` | only the result |
| `--extract PATH` | only the part at `PATH` (`a.b.0`). A text value is printed as it is, so `dtb prep export --data x.csv --extract dataset > nonmem.csv` writes the dataset |
| `-o, --out FILE` | write to a file instead of standard output; refuses to overwrite unless `--force` |
| `--pretty` | indented JSON |
| `--timing` | add `run: { elapsedMs, threads }` (the output then differs between runs) |
| `--json-errors` | an error is printed as JSON on standard output (`{ "error": { kind, message, exitCode } }`), as well as the exit code |
| `-q` | nothing on standard error except internal errors |

Other commands: `dtb info` (version, protocol, build, platform, limits, effective configuration), `dtb doctor`, `dtb commands [group]`, `dtb describe COMMAND`, `dtb config show|path|init|check`, `dtb run PIPELINE`, `dtb verify MANIFEST`.

## Results and provenance

```json
{
  "dtb": {
    "version": "0.1.0", "protocol": 1, "command": "trial.run", "seed": 7,
    "configSha256": "…", "paramsSha256": "…",
    "inputs": [ { "path": "examples/data/trial-design.json", "sha256": "…", "bytes": 1432 } ]
  },
  "result": { … }
}
```

* `configSha256` is the SHA-256 of the settings that can change a result (limits, engine defaults, seed). Threads, timing and layout are left out because they never change a result.
* `paramsSha256` is the SHA-256 of the canonical JSON of the parameters after files and references were read in.
* `inputs` lists every file read, with its SHA-256.
* There is no date, host name, user name or thread count. The same call gives the same bytes.

Canonical JSON means sorted keys, no spaces, shortest exact numbers; `NaN` and infinities become `null`.

## Reproducibility

* **The random generator is the apps'.** `xoshiro128**` seeded through `splitmix32`, with the same normal, exponential, gamma, beta and chi-square generators as the JavaScript engines, so `--set streams=js` reproduces the browser's numbers for a given seed (to the last digit of a double; checked by the parity tests).
* **`blocks` streams (the default) are parallel.** The work is cut into blocks of trials, each with its own stream derived from the seed and the block number, so the results are **identical on one thread or sixty** (checked by `dtb doctor` and the tests). They differ from the browser's single stream, which cannot be split without changing it; use `streams = "js"` when you need the browser's numbers (that runs on one thread).
* **Number formatting is JavaScript's.** Rounding, `toFixed`, and natural-order sorting follow the JavaScript rules, so tables, labels and datasets are the same text as in the apps.
* **No hidden inputs.** Nothing depends on the clock, locale, environment (other than the documented `DTB_*` settings) or the number of threads.
* **Check it yourself:** `dtb doctor` runs known answers; `dtb verify MANIFEST --rerun` runs a whole pipeline again and compares every result.

## Configuration

A TOML file, `dtb.toml`. `dtb config init` writes a commented example with every setting at its default. Unknown keys are an error (a misspelt setting is never ignored silently).

```toml
[general]
threads = 0          # worker threads; 0 = all cores
# seed = 12345       # default random seed when a call gives none
timing = false
pretty = false

[limits]
max_input_mb = 256
max_rows = 5000000
max_columns = 2000
max_trials = 1000000
max_scenarios = 100
max_sims = 5000000
max_points = 5000
max_subjects = 200000
max_steps = 500

[trial]
streams = "blocks"   # or "js"
trials = 1000

[power]
streams = "blocks"
sims = 10000

[prep]
delimiter = "auto"   # auto, comma, tab, semicolon, pipe, space
missing = "."
```

Where it is found, first match wins: `--config FILE`, the `DTB_CONFIG` environment variable, `./dtb.toml`. `--no-config` ignores all of them.

Precedence of a setting, highest first: **command-line option**, **the parameters of the call**, **`DTB_*` environment variable** (`DTB_THREADS`, `DTB_SEED`, `DTB_TIMING`, `DTB_PRETTY`, `DTB_CONFIG`), **the file**, **the built-in default**.

`dtb config show` prints the effective configuration; `dtb config check FILE` validates a file and prints its hash.

## Pipelines and manifests

A pipeline is a TOML file of steps. Each step runs one command; later steps can use earlier results.

```toml
name = "Bioequivalence and sample size"

[settings]
seed = 2026
out_dir = "out-be"          # default: dtb-out next to the pipeline

[[step]]
id = "be"
command = "be.analyze"
[step.params]
table = { "$file" = "data/crossover.csv" }   # a file, relative to the pipeline
params = ["Cmax", "AUC"]
[step.params.map]
subject = "SUBJ"
sequence = "SEQ"
period = "PER"
treatment = "TRT"
[[step.write]]                                # write part of the result as a file
from = "results.Cmax"                         # a.b.0 path into the result ("" = all of it)
to = "cmax.json"                              # inside the output folder only
format = "json"                               # text | json | csv

[[step]]
id = "size"
command = "power.size"
[step.params.args]
cv = { "$ref" = "be.results.Cmax.cv" }        # a part of an earlier step's result
theta0 = 0.95
design = "2x2"
```

`dtb run pipeline.toml` writes `STEP.json` (the full envelope) for every step, the requested files, and **`manifest.json`**:

* the pipeline's SHA-256, the settings' SHA-256 and the seed,
* for every step: the command, the SHA-256 of its parameters, of every input file, of its result and of every file it wrote,
* no timestamps, so running the pipeline again gives the same manifest byte for byte.

`dtb verify manifest.json` checks every file in the manifest against its hash and exits with code 6 if any input, result or output changed. `dtb verify manifest.json --rerun` also runs the pipeline again in a temporary folder and compares every step's result.

Steps can be switched off with `enabled = false` and described with `note = "…"`. A failing step stops the pipeline and names the step.

## Security and limits

dtb is built to be run on data it did not write, and to be called by a server on behalf of a user.

* **No network, no shell.** It never opens a connection or starts another program. `#![forbid(unsafe_code)]` is set for the whole crate.
* **Limits** (see Configuration) on input size, rows, columns, trials, scenarios, simulations, points, subjects and steps. A request over a limit is refused before the work starts (exit code 5) and says which setting to change.
* **Standard input cannot name files.** When the parameters come from `--input -`, a `{ "$file": … }` in them is refused. A server relaying a user's request can therefore pass it through without checking it for paths.
* **Outputs are confined.** Pipeline `write` paths must be relative and stay inside the output folder (no `..`, no absolute paths, no drive letters). Nothing is overwritten without `--force`, and a file is written under a temporary name and renamed, so a failure never leaves half a result.
* **Inputs are read once, with a size limit, as text.** A file that is not UTF-8 is refused.
* **Parameters and config files are strict.** An unknown parameter of a command, an unknown key in a configuration or pipeline file, and an out-of-range value are errors that name the culprit; nothing is silently ignored.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | done |
| 2 | wrong command line, parameters or configuration |
| 3 | the data or design cannot be analysed (the message says why) |
| 4 | a file could not be read or written |
| 5 | a safety limit was reached |
| 6 | verification failed, a pipeline step failed, or `doctor` found a problem |
| 70 | internal error (a bug; please report it with the command) |

With `--json-errors` the same information is printed as `{ "dtb": {…}, "error": { "kind": "usage", "message": "…", "exitCode": 2 } }`.

## How the apps use it

Each app's server looks for dtb when it starts (and again every 30 seconds while it is not found or has changed): the `DTB_BIN` variable, then `..\..\..\dtb\target\release\dtb.exe` relative to the app folder, then the `PATH`. When found it runs `dtb info`, keeps the version, protocol and command list, and reports them in the app's `/api/health`:

```json
"dtb": { "found": true, "usable": true, "path": "…\\dtb.exe", "version": "0.1.0", "protocol": 1, "commands": 6, "supported": [1] }
```

`commands` is the number of dtb commands this app may use. The app shows *dtb 0.1.0* in its top bar and sends heavy work to dtb through `POST /api/dtb/<command>`. The server accepts only the commands of that app, runs `dtb --no-config --json-errors --quiet call COMMAND --input - --result` with the request body on standard input (no shell, no file names accepted, a time limit of 300 s, a limit on the size of the answer, at most two runs at once; `DTB_CONFIG`, `DTB_THREADS`, `DTB_TIMEOUT_S` and `DTB_MAX_PARALLEL` adjust these), and returns the result. If dtb is missing, has an unknown protocol, times out or fails, the app computes the same thing in JavaScript. The result is the same either way up to the random stream (see Reproducibility): the apps ask for `streams = "js"` when they need the browser's numbers for a seed.

| App | What goes to dtb | What stays in the browser |
|---|---|---|
| TRIAL | `trial.run` (Simulate), `trial.scenarios` and `trial.tornado` (Scenarios) | design checks, concentration bands and the exposure–response curve (small and interactive) |
| BEDDI | `power.plan` (the whole sample-size page: size, curve, enrolment, check, table) | the bioequivalence analysis, NCA, interactions and impairment pages (instant for study-sized data) |
| PREP | `prep.qc` for tables of 100 000 rows or more (the page stays free; there is no row limit) · `node scripts/replay.mjs --dtb` runs a whole recipe and its checks in dtb | the recipe preview, the checks of smaller tables, exports and the derivation report |

Every command of dtb can also be used directly (command line, pipelines, or `POST /api/dtb/<command>` on an app's server for the commands of that app). A status badge in each app's top bar shows *dtb 0.1.0* when it is in use; clicking it cycles through the choices: the browser's random numbers (default: the same results as without dtb), parallel streams (TRIAL and BEDDI: fastest, other random numbers) and browser only. The tooltip reports the last problem if dtb ever failed and the browser took over.

## Speed

`node tools/bench.mjs` times dtb against the apps' JavaScript engines on the same work, including starting dtb as a new process (Node 22; a 2-core container, so more cores widen the gap):

| Work | JavaScript | dtb, 1 thread | dtb, 2 threads |
|---|---|---|---|
| `trial.run`, 2 000 trials | 444 ms | 97 ms | 52 ms |
| `trial.run`, 10 000 trials | 2 384 ms | 396 ms | 220 ms |
| `power.sim`, RSABE, 20 000 studies | 262 ms | 119 ms | 59 ms |

These use `streams = "js"` for one thread (the browser's numbers) and `blocks` for two. A 15.6 MB CSV of 198 400 rows, read and checked with all 25+ dataset checks: JavaScript 1.8 s, dtb about 1.3 s (reading and typing the table dominate). For small tables the browser is as fast, which is why the apps only send tables of 100 000 rows and more.

## How the files are organised

```
dtb/
  Cargo.toml  build.rs  rustfmt.toml  .gitignore
  README.md  CHANGELOG.md  DEPLOYMENT.md
  docs/COMMANDS.md          every command and parameter (generated)
  configs/dtb.example.toml  the commented example configuration
  examples/                 example data and two pipelines
  src/
    main.rs  cli.rs         the program and its command line
    runner.rs               assembling parameters, running a command, the envelope
    pipeline.rs             pipelines, manifests, verify
    commands/               the registry: one module per group (trial, be, power, ddi, nca, renal, stats, prep, info)
    core/                   shared: errors, config, hashing, I/O with limits, provenance, JavaScript-compatible numbers, CSV
    stats/                  distributions, linear models, the random generator
    trial/                  design, PK/PD, simulation, decisions, analysis
    be/ ddi/ nca/ impair/   bioequivalence and power; interactions; NCA; renal and hepatic
    prep/                   table, dates, expression language, 21 operations, recipes, checks, export
  tests/                    parity tests against the JavaScript engines (fixtures/), command-line tests
  tools/                    fixture generator, benchmark, reference generator
```

Every command is `fn(&Value, &Ctx) -> Result<Value>` in the registry in `src/commands/mod.rs`; the command line, the pipeline runner and the apps' servers all reach the same code. To add a command: write the function, add it to its module's `commands()` list, add a test, and run `node tools/gen_commands_doc.mjs`.

## Tests

```
cargo test                 # unit, parity and command-line tests
cargo clippy --all-targets
dtb doctor                 # the self-test of the built program
```

* **Parity tests** (`tests/parity_*.rs`) compare the Rust engines with numbers produced by the JavaScript engines (`tests/fixtures/*.json`, made by `node tools/gen_fixtures.mjs`; they are committed, so the tests need no Node). They cover distributions and linear models, the random streams, trial simulation, bioequivalence, power, drug interactions, NCA, impairment, and the whole of Prep Studio's recipe, expressions and checks.
* **Command-line tests** (`tests/cli.rs`) run the built program: exit codes, the envelope, identical bytes on 1 and 4 threads, limits, the standard-input sandbox, confined outputs, pipelines, manifests and tamper detection.

## Versions

Semantic versioning in `Cargo.toml`; `dtb --version`, `dtb info` and every result report it. The *protocol* number changes only when a command's meaning or output changes incompatibly. Releases are archived in `.Archives\dtb-vX.Y.Z.tar.gz` and described in [CHANGELOG.md](CHANGELOG.md).
