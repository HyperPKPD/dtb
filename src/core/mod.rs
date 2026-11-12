//! Shared plumbing: delimited text, errors, hashing, provenance, configuration, input and output with limits.

pub mod config;
pub mod csvio;
pub mod error;
pub mod fmt;
pub mod hash;
pub mod io;
pub mod js;
pub mod provenance;

pub use csvio::{detect_delimiter, num, parse_delimited, to_delimited, Parsed, Table};
pub use error::{DtbError, Result};
pub use fmt::fixed;
