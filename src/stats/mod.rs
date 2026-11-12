//! Statistics library: random numbers, special functions, distributions, descriptive statistics, numerical integration, least squares.
//!
//! The algorithms and constants are those of the JavaScript library used by the apps (`src/kit/stats.js`), so the same seed gives the
//! same random stream and the same numbers; `tests/parity.rs` checks this against fixtures produced by the JavaScript engines.

pub mod desc;
pub mod dist;
pub mod num;
pub mod ols;
pub mod rng;
pub mod special;

pub use desc::*;
pub use dist::*;
pub use num::*;
pub use ols::{ols, Ols};
pub use rng::Rng;
pub use special::*;
