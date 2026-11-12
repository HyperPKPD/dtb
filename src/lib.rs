//! dtb: the computation engine behind the nlmx apps (Prep Studio, Trial Simulator, BE & DDI Studio).
#![forbid(unsafe_code)]
// The numerical code keeps the shape of the browser engines it is checked against, term for term. `!(x > 0.0)` is deliberate: it is
// also true for NaN, which `x <= 0.0` is not. Index loops mirror the matrix algebra of the original.
#![allow(
    clippy::neg_cmp_op_on_partial_ord,
    clippy::needless_range_loop,
    clippy::manual_range_contains,
    clippy::should_implement_trait,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::manual_clamp,
    clippy::unnecessary_sort_by,
    clippy::option_filter_map,
    clippy::unnecessary_filter_map
)]

pub mod be;
pub mod cli;
pub mod commands;
pub mod core;
pub mod ddi;
pub mod impair;
pub mod nca;
pub mod pipeline;
pub mod prep;
pub mod runner;
pub mod stats;
pub mod trial;
