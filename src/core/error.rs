//! One error type with a stable exit code per kind, so that scripts (and the apps' servers) can tell the cases apart.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, DtbError>;

#[derive(Debug, Error)]
pub enum DtbError {
    /// The command line or a configuration file is wrong.
    #[error("{0}")]
    Usage(String),
    /// The data or the design cannot be analysed (the message says why, in plain words).
    #[error("{0}")]
    Input(String),
    /// A file could not be read or written.
    #[error("{0}")]
    Io(String),
    /// A safety limit was reached (input size, number of trials, rows ...).
    #[error("{0}")]
    Limit(String),
    /// A step of a pipeline failed, or a manifest does not match its files.
    #[error("{0}")]
    Verify(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl DtbError {
    /// Process exit code: 2 usage, 3 input, 4 file, 5 limit, 6 verification, 70 internal.
    pub fn exit_code(&self) -> i32 {
        match self {
            DtbError::Usage(_) => 2,
            DtbError::Input(_) => 3,
            DtbError::Io(_) => 4,
            DtbError::Limit(_) => 5,
            DtbError::Verify(_) => 6,
            DtbError::Internal(_) => 70,
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            DtbError::Usage(_) => "usage",
            DtbError::Input(_) => "input",
            DtbError::Io(_) => "io",
            DtbError::Limit(_) => "limit",
            DtbError::Verify(_) => "verify",
            DtbError::Internal(_) => "internal",
        }
    }
}

impl From<std::io::Error> for DtbError {
    fn from(e: std::io::Error) -> Self {
        DtbError::Io(e.to_string())
    }
}
impl From<serde_json::Error> for DtbError {
    fn from(e: serde_json::Error) -> Self {
        DtbError::Input(format!("The JSON input is not valid: {e}"))
    }
}
impl From<String> for DtbError {
    fn from(e: String) -> Self {
        DtbError::Input(e)
    }
}
