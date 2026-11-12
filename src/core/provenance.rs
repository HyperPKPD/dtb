//! The envelope around every result: what produced it, from which inputs, with which seed and settings. It holds nothing that varies
//! between runs (no clock, no host name, no thread count) unless timing was asked for, so the same call always gives the same bytes.

use super::io::InputRef;
use serde::Serialize;
use serde_json::{json, Value};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Bumped when the meaning of a command or its output changes incompatibly. The apps check it.
pub const PROTOCOL: u32 = 1;
pub const ENGINE: &str = concat!("dtb ", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub version: &'static str,
    pub protocol: u32,
    pub command: String,
    pub seed: Option<u64>,
    pub config_sha256: String,
    pub params_sha256: String,
    pub inputs: Vec<InputRef>,
}

pub fn envelope(meta: &Meta, result: Value, run: Option<Value>) -> Value {
    let mut dtb = serde_json::to_value(meta).unwrap_or(Value::Null);
    if let (Some(r), Some(o)) = (run, dtb.as_object_mut()) {
        o.insert("run".into(), r);
    }
    json!({ "dtb": dtb, "result": result })
}

pub fn error_envelope(command: &str, kind: &str, message: &str, code: i32) -> Value {
    json!({ "dtb": { "version": VERSION, "protocol": PROTOCOL, "command": command }, "error": { "kind": kind, "message": message, "exitCode": code } })
}
