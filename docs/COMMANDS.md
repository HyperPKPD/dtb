# Command reference

Generated from dtb 0.1.0 (protocol 1) by `tools/gen_commands_doc.mjs`; `dtb describe COMMAND` prints the same for one command.

Every command takes a JSON object of parameters and returns `{ "dtb": { …provenance… }, "result": … }`. Give parameters with `--input FILE|-`, `--file KEY=PATH`, `--data FILE` and `--set KEY=VALUE` (see the README). A parameter marked *optional* has a default.

## General

### `info`

Version, protocol, build, commands, limits and the effective configuration.

### `doctor`

Self-test: known answers of the random generator, statistics, engines and the determinism of parallel runs.

## Statistics

### `stats.describe`

n, mean, SD, quartiles, geometric mean and a Wilson interval of a list of numbers.

| Parameter | | Meaning |
|---|---|---|
| `values` | required | list of numbers |
| `quantiles` | optional | extra quantiles, e.g. [0.05, 0.95] |

### `stats.dist`

Distribution function: cdf or quantile of normal, t, chisq or f.

| Parameter | | Meaning |
|---|---|---|
| `dist` | required | norm \| t \| chisq \| f |
| `fn` | required | cdf \| quantile |
| `x` | optional | value (cdf) |
| `p` | optional | probability (quantile) |
| `df` | optional | degrees of freedom (t, chisq, f numerator) |
| `df2` | optional | denominator degrees of freedom (f) |

## Prep Studio (data preparation)

### `prep.profile`

Per-column facts: type, missing, unique values, range, mean, examples.

| Parameter | | Meaning |
|---|---|---|
| `table` | required | CSV text or { columns, rows } |
| `delimiter` | optional | auto \| comma \| tab \| semicolon \| pipe \| space |

### `prep.qc`

Check a dataset against what NONMEM expects (IDs, TIME order, EVID/MDV/AMT/DV, BLQ, ADDL/II, covariates).

| Parameter | | Meaning |
|---|---|---|
| `table` | required | the data: CSV text or { columns, rows } |
| `roles` | optional | { ID: "SUBJ", … } column names when they are not the standard ones |
| `lloq` | optional | number |
| `covariates` | optional | list of covariate columns |
| `baseline` | optional | false to skip the 'varies within subject' note |

### `prep.run`

Replay a Prep Studio recipe on source tables; returns the dataset, the per-step report and the checks.

| Parameter | | Meaning |
|---|---|---|
| `recipe` | required | a recipe object (format nlmx-prep-recipe) |
| `sources` | required | { name: table } for every file the recipe uses |
| `upTo` | optional | index of the last step to run |
| `qc` | optional | true or the options of prep.qc |
| `include` | optional | what to return: table (default), dataset, nonmem, profile |
| `export` | optional | { delimiter: comma\|tab\|semicolon\|space, missing, header, file, ignore } |

### `prep.eval`

Evaluate an expression for every row of a table.

| Parameter | | Meaning |
|---|---|---|
| `table` | required | the data: CSV text or { columns, rows } |
| `expr` | required | the expression |
| `by` | optional | group columns for lag, first, cumsum … |

### `prep.export`

The dataset as delimited text and the $DATA/$INPUT lines of a control stream.

| Parameter | | Meaning |
|---|---|---|
| `table` | required | the data: CSV text or { columns, rows } |
| `export` | optional | { delimiter, missing, header, file, ignore } |

## Trial Simulator

### `trial.validate`

Check a trial design; list errors and warnings in plain words.

| Parameter | | Meaning |
|---|---|---|
| `design` | required | the design object of the Trial Simulator |

### `trial.run`

Simulate many trials of one design: power, decisions, effect distribution.

| Parameter | | Meaning |
|---|---|---|
| `design` | required | the design object |
| `trials` | optional | number of simulated trials (default trial.trials) |
| `seed` | optional | random seed (default design.seed) |
| `streams` | optional | "blocks" (parallel, default) or "js" (same numbers as the browser) |
| `values` | optional | keep per-trial values (default true) |

### `trial.scenarios`

Run named scenarios (overrides of the base design) with a shared seed.

| Parameter | | Meaning |
|---|---|---|
| `design` | required | the base design |
| `scenarios` | required | [{ name, overrides }] |
| `trials` | optional | simulated trials (config trial.trials) |
| `seed` | optional | random seed |
| `streams` | optional | "blocks" (parallel, any thread count) or "js" (the browser's numbers) |
| `values` | optional | true to include every trial's values |

### `trial.tornado`

One-at-a-time sensitivity of the probability of Go for one arm.

| Parameter | | Meaning |
|---|---|---|
| `design` | required | the trial design (arms, endpoint, PK/PD, decision rule) |
| `armId` | required | id of the arm |
| `trials` | optional | simulated trials (config trial.trials) |
| `seed` | optional | random seed |
| `streams` | optional | "blocks" (parallel, any thread count) or "js" (the browser's numbers) |

### `trial.bands`

Percentile bands of the concentration-time profile of each arm.

| Parameter | | Meaning |
|---|---|---|
| `design` | required | the trial design (arms, endpoint, PK/PD, decision rule) |
| `from` | optional | hours (0) |
| `to` | optional | hours (168) |
| `points` | optional | 140 |
| `nSub` | optional | subjects per arm (150) |
| `seed` | optional | 7 |

### `trial.erCurve`

The exposure-response curve of the design with the arms marked on it.

| Parameter | | Meaning |
|---|---|---|
| `design` | required | the trial design (arms, endpoint, PK/PD, decision rule) |
| `points` | optional | 120 |

## Bioequivalence

### `be.analyze`

Bioequivalence analysis: ABE, ABEL, RSABE, NTID or non-parametric, with ANOVA and carry-over checks.

| Parameter | | Meaning |
|---|---|---|
| `table` | required | the data: CSV text or { columns, rows } |
| `map` | required | { subject, treatment, period, sequence, params: [..] } column names |
| `param` | optional | the parameter to analyse, for example "Cmax" |
| `params` | optional | or a list of parameters to analyse |
| `options` | optional | { test, ref, alpha, limits, method: abe\|abel\|rsabe\|ntid\|np, welch, abel, npMargin } |
| `delimiter` | optional | for CSV text |

### `be.residuals`

Residuals and standardised residuals of the fitted model of one parameter.

| Parameter | | Meaning |
|---|---|---|
| `table` | required | the data: CSV text or { columns, rows } |
| `map` | required | { subject, treatment, period, sequence, params: [..] } column names |
| `param` | optional | the parameter to analyse, for example "Cmax" |
| `params` | optional | or a list of parameters to analyse |
| `options` | optional | { test, ref, alpha, limits, method: abe\|abel\|rsabe\|ntid\|np, welch, abel, npMargin } |
| `delimiter` | optional | for CSV text |

## Power and sample size

### `power.designs`

The study designs (sequences) the power functions know.

### `power.abe`

Exact power of the two one-sided tests (ABE) for any design.

| Parameter | | Meaning |
|---|---|---|
| `args` | required | { cv, theta0, n, sequences \| design, limits, alpha } |

### `power.size`

Smallest sample size reaching the target power for ABE.

| Parameter | | Meaning |
|---|---|---|
| `args` | required | as power.abe (n is ignored) |
| `target` | optional | 0.8 |
| `minN` | optional | 12 |
| `maxN` | optional | 5000 |
| `dropout` | optional | expected drop-out rate; adds nEnrol |

### `power.sim`

Power of ABEL, RSABE or NTID by simulating the whole decision.

| Parameter | | Meaning |
|---|---|---|
| `args` | required | { method, cvT, cvR, theta0, n, sequences, nSim, seed, limits, alpha, abel } |
| `streams` | optional | "blocks" (parallel, any thread count) or "js" (the browser's numbers) |

### `power.simSize`

Smallest sample size reaching the target power by simulation.

| Parameter | | Meaning |
|---|---|---|
| `args` | required | as power.sim |
| `target` | optional | 0.8 |
| `minN` | optional | smallest N tried (12) |
| `maxN` | optional | largest N tried (600) |
| `streams` | optional | "blocks" (parallel, any thread count) or "js" (the browser's numbers) |

### `power.plan`

Everything the app's sample-size page shows in one call: size, power curve, enrolment with dropout, a check at one N and the table by CV and ratio.

| Parameter | | Meaning |
|---|---|---|
| `method` | optional | abe (default), abel, rsabe or ntid |
| `sequences` | optional | list such as ["TR","RT"]; null for parallel groups |
| `cvT` | required | within-subject CV of the test (the only CV for abe) |
| `cvR` | optional | within-subject CV of the reference (scaled methods; default cvT) |
| `theta0` | required | true ratio test / reference |
| `limits` | optional | [0.8, 1.25] |
| `alpha` | optional | one-sided level (0.05) |
| `target` | optional | target power (0.8) |
| `minN` | optional | smallest N (12) |
| `dropout` | optional | expected drop-out rate (0) |
| `nSim` | optional | simulated studies for the scaled methods (config power.sims) |
| `seed` | optional | random seed for the scaled methods (12345) |
| `nCheck` | optional | also give the power at this N (0 = skip) |
| `table` | optional | also give the table by CV and ratio (abe only; default true) |
| `streams` | optional | "blocks" (parallel, any thread count) or "js" (the browser's numbers) |

### `power.table`

Sample sizes over a grid of CVs and true ratios (ABE).

| Parameter | | Meaning |
|---|---|---|
| `cvs` | required | list of CVs |
| `ratios` | required | list of true ratios |
| `sequences` | optional | list or null for parallel |
| `limits` | optional | [0.8, 1.25] |
| `alpha` | optional | 0.05 |
| `target` | optional | 0.8 |
| `minN` | optional | 12 |

## Drug–drug interactions

### `ddi.presets`

The regulatory presets (FDA 2020, EMA 2012) and their constants.

### `ddi.predict`

Static R-values per enzyme, the mechanistic AUC ratio and its classification.

| Parameter | | Meaning |
|---|---|---|
| `preset` | optional | fda2020 \| ema2012 |
| `presetOverrides` | optional | constants to change |
| `conc` | required | { doseMg, mw, cmax, fup, ka, faFg } |
| `enzymes` | required | [{ enzyme, ki, kinact, KI, kdeg, kdegGut, emax, ec50, fm, gut }] |
| `victim` | optional | { fg, hepaticConc: imax\|inlet, gut } |

### `ddi.steady`

Steady-state peak, trough and average of repeated oral dosing.

| Parameter | | Meaning |
|---|---|---|
| `input` | required | { doseMg, mw, cl, v, ka, f, tau } |

### `ddi.dynamic`

Dynamic (time-course) interaction model, with and without the perpetrator.

| Parameter | | Meaning |
|---|---|---|
| `model` | required | the dynamic model object |
| `window` | optional | [from, to] hours for AUC |
| `every` | optional | keep every n-th point of the series (1) |

## Non-compartmental analysis

### `nca.profile`

NCA of one concentration-time profile.

| Parameter | | Meaning |
|---|---|---|
| `time` | required | list of times |
| `conc` | required | list of concentrations |
| `options` | optional | { dose, lloq, zeroAtStart, minPoints, maxExtrap } |

### `nca.table`

NCA of every subject-treatment-period profile of a long table.

| Parameter | | Meaning |
|---|---|---|
| `table` | required | CSV text or { columns, rows } |
| `map` | required | { subject, treatment, time, conc, period, sequence } |
| `options` | optional | as nca.profile |
| `profiles` | optional | include the profiles (default false) |

## Kidney function

### `renal.egfr`

Kidney function of one or more persons: Cockcroft-Gault, MDRD, CKD-EPI 2009 and 2021, BSA, ideal weight.

| Parameter | | Meaning |
|---|---|---|
| `persons` | optional | [{ age, weight, height, female, scr, black, weightType }] |
| `person` | optional | or one such person |

### `renal.categories`

The renal function categories and their limits.

### `renal.giusti`

Giusti-Hayton dose adjustment from the fraction excreted unchanged.

| Parameter | | Meaning |
|---|---|---|
| `fe` | required | fraction excreted unchanged |
| `crcl` | optional | creatinine clearance of the patient |
| `crclNormal` | optional | 120 |
| `dose` | optional | the usual dose |
| `tau` | optional | the usual dosing interval, hours |

### `renal.groups`

Compare exposure of impairment groups with a reference group on the log scale.

| Parameter | | Meaning |
|---|---|---|
| `groups` | required | [{ name, values }] |
| `ref` | required | name of the reference group |
| `alpha` | optional | 0.1 |
| `pooled` | optional | true |
| `bounds` | optional | [0.8, 1.25] for the dose advice |

### `renal.regress`

Regression of log exposure on kidney function with predictions.

| Parameter | | Meaning |
|---|---|---|
| `x` | required | list |
| `y` | required | list |
| `options` | optional | { logX, ref, points, alpha } |

## Liver function

### `hepatic.childPugh`

Child-Pugh score and class.

| Parameter | | Meaning |
|---|---|---|
| `input` | required | { bilirubin, albumin, inr, ascites, encephalopathy } |

### `hepatic.liver`

Well-stirred liver model with changes in flow, intrinsic clearance and binding.

| Parameter | | Meaning |
|---|---|---|
| `input` | required | { q, fu, bp, clint, clRenal, fa, fg } |
| `mult` | optional | { q, clint, fu } |
