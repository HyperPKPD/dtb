//! Clinical trial simulation: designs, pharmacokinetics, one simulated trial, many trials, scenarios, sensitivity.
//!
//! A design is the JSON the Trial Simulator saves (`{ name, seed, arms, tau, durationWeeks, pk, adherence, dropout, ... }`); the results
//! have the shape of the browser engine's results, so the app can show either.

pub mod analysis;
pub mod decision;
pub mod design;
pub mod pk;
pub mod scenarios;
pub mod sim;

pub use design::{apply_overrides, validate_design, Design, Overrides};
pub use scenarios::{er_curve, pk_bands, run_scenarios, tornado, Scenario};
pub use sim::{run_scenario, RunOptions, ScenarioResult, Streams};
