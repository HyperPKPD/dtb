//! Reading inputs and writing outputs with limits. Nothing here touches the network or starts a process.

use super::error::{DtbError, Result};
use super::hash::sha256_bytes;
use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize)]
pub struct InputRef {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

/// Read a whole file or standard input (`-`) as bytes, refusing more than `max_bytes`.
pub fn read_limited(path: &str, max_bytes: u64) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    if path == "-" {
        std::io::stdin().lock().take(max_bytes + 1).read_to_end(&mut buf).map_err(|e| DtbError::Io(format!("Cannot read standard input: {e}")))?;
    } else {
        let p = Path::new(path);
        let meta = std::fs::metadata(p).map_err(|e| DtbError::Io(format!("Cannot read {path}: {e}")))?;
        if !meta.is_file() {
            return Err(DtbError::Io(format!("{path} is not a file.")));
        }
        if meta.len() > max_bytes {
            return Err(DtbError::Limit(format!("{path} is {} MB; the limit is {} MB (limits.max_input_mb).", meta.len() / 1_048_576, max_bytes / 1_048_576)));
        }
        std::fs::File::open(p).and_then(|f| f.take(max_bytes + 1).read_to_end(&mut buf)).map_err(|e| DtbError::Io(format!("Cannot read {path}: {e}")))?;
    }
    if buf.len() as u64 > max_bytes {
        return Err(DtbError::Limit(format!("The input is larger than {} MB (limits.max_input_mb).", max_bytes / 1_048_576)));
    }
    Ok(buf)
}

/// Read a text file (UTF-8; a byte-order mark is removed) and describe it for the provenance record.
pub fn read_text(path: &str, max_bytes: u64) -> Result<(String, InputRef)> {
    let b = read_limited(path, max_bytes)?;
    let r = InputRef { path: path.to_string(), sha256: sha256_bytes(&b), bytes: b.len() as u64 };
    let s = String::from_utf8(b).map_err(|_| DtbError::Input(format!("{path} is not UTF-8 text.")))?;
    Ok((s.strip_prefix('\u{feff}').map(String::from).unwrap_or(s), r))
}

/// Write a file without overwriting unless `force`. Parent folders are created. Returns the digest and size.
pub fn write_file(path: &Path, bytes: &[u8], force: bool) -> Result<InputRef> {
    if path.is_dir() {
        return Err(DtbError::Io(format!("{} is a folder.", path.display())));
    }
    if path.exists() && !force {
        return Err(DtbError::Io(format!("{} exists. Use --force to overwrite it.", path.display())));
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(|e| DtbError::Io(format!("Cannot create {}: {e}", parent.display())))?;
    }
    // write to a temporary name first so that a failure never leaves a half-written result
    let tmp: PathBuf = path.with_extension(format!("{}.dtb-tmp", path.extension().and_then(|e| e.to_str()).unwrap_or("")));
    std::fs::write(&tmp, bytes).map_err(|e| DtbError::Io(format!("Cannot write {}: {e}", path.display())))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        DtbError::Io(format!("Cannot write {}: {e}", path.display()))
    })?;
    Ok(InputRef { path: path.display().to_string(), sha256: sha256_bytes(bytes), bytes: bytes.len() as u64 })
}

/// A relative path that stays inside its folder: no absolute paths, no `..`, no drive letters.
pub fn confined(base: &Path, rel: &str) -> Result<PathBuf> {
    let p = Path::new(rel);
    let bad = p.is_absolute()
        || rel.contains(':')
        || p.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::Prefix(_) | std::path::Component::RootDir));
    if rel.is_empty() || bad {
        return Err(DtbError::Usage(format!("\"{rel}\" is not allowed: output files must be relative paths inside the output folder.")));
    }
    Ok(base.join(p))
}

/// Digest and size of a file.
pub fn sha256_ref(p: &Path) -> std::io::Result<(String, u64)> {
    super::hash::sha256_file(p)
}
