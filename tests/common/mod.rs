//! Helpers for the parity tests: fixtures written by `tools/gen_fixtures.mjs` from the JavaScript engines.
#![allow(dead_code)]
use serde_json::Value;

pub fn fixture(name: &str) -> Value {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("fixtures").join(name);
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))).unwrap()
}

/// A number from a fixture; the generator writes NaN and infinities as marker strings.
pub fn f(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap(),
        Value::String(s) if s == "__NaN__" => f64::NAN,
        Value::String(s) if s == "__Inf__" => f64::INFINITY,
        Value::String(s) if s == "__-Inf__" => f64::NEG_INFINITY,
        Value::Null => f64::NAN,
        other => panic!("not a number: {other}"),
    }
}

pub fn close(a: f64, b: f64, tol: f64, what: &str) {
    if b.is_nan() || a.is_nan() {
        assert!(a.is_nan() && b.is_nan() || a.is_nan() && b.is_infinite() || b.is_nan() && a.is_infinite(), "{what}: {a} vs {b}");
        return;
    }
    if a.is_infinite() || b.is_infinite() {
        assert!(a == b, "{what}: {a} vs {b}");
        return;
    }
    assert!((a - b).abs() <= tol * b.abs().max(1.0), "{what}: {a} vs {b} (tolerance {tol})");
}

/// Deep comparison of the Rust output (`got`) with the JavaScript output (`want`). Numbers within `tol`; a null in `got` stands for
/// any non-finite number in `want`; keys of `want` must all be present in `got` (and the reverse when `strict`).
pub fn deep(got: &Value, want: &Value, tol: f64, strict: bool, path: &str) {
    match (got, want) {
        (_, Value::String(s)) if s.starts_with("__") && s.ends_with("__") => {
            assert!(got.is_null() || got.as_str().is_some_and(|g| g == s), "{path}: {got} vs {want}");
        }
        (Value::Number(a), Value::Number(b)) => close(a.as_f64().unwrap(), b.as_f64().unwrap(), tol, path),
        (Value::Object(a), Value::Object(b)) => {
            for (k, wv) in b {
                match a.get(k) {
                    Some(gv) => deep(gv, wv, tol, strict, &format!("{path}.{k}")),
                    None => assert!(wv.is_null(), "{path}: key {k} missing in output"),
                }
            }
            if strict {
                for k in a.keys() {
                    assert!(b.contains_key(k), "{path}: unexpected key {k} in output");
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}: array length");
            for (i, (g, w)) in a.iter().zip(b).enumerate() {
                deep(g, w, tol, strict, &format!("{path}[{i}]"));
            }
        }
        (Value::Null, Value::Null) => {}
        (g, w) => assert_eq!(g, w, "{path}"),
    }
}
