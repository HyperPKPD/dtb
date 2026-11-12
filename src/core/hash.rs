//! SHA-256 of bytes, text, files and canonical JSON (keys sorted, no whitespace), for provenance and manifests.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

pub fn sha256_bytes(b: &[u8]) -> String {
    hex(&Sha256::digest(b))
}

fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// Hash a file by streaming it; returns the digest and the size in bytes.
pub fn sha256_file(p: &Path) -> std::io::Result<(String, u64)> {
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut n = 0u64;
    loop {
        let k = f.read(&mut buf)?;
        if k == 0 {
            break;
        }
        h.update(&buf[..k]);
        n += k as u64;
    }
    Ok((hex(&h.finalize()), n))
}

/// The canonical text of a JSON value: object keys are sorted (serde_json keeps them in a sorted map), no whitespace.
pub fn canonical(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

pub fn sha256_json(v: &Value) -> String {
    sha256_bytes(canonical(v).as_bytes())
}
