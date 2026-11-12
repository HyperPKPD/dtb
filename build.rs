//! Records build information (target, profile, rustc version) so that `dtb --version` and `dtb info` can report it.
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-env=DTB_TARGET={}", std::env::var("TARGET").unwrap_or_default());
    println!("cargo:rustc-env=DTB_PROFILE={}", std::env::var("PROFILE").unwrap_or_default());
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let v = Command::new(rustc).arg("--version").output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();
    println!("cargo:rustc-env=DTB_RUSTC={}", v.trim());
}
