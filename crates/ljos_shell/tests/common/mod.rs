//! What the link tests ask cargo: the crates a build links.

// Each test file takes what it asks for; the rest is unused there.
#![allow(dead_code)]

use std::process::Command;

/// The crates a build of `package` links with `features` (no dev- or
/// build-dependencies).
pub fn linked_by(package: &str, features: &[&str]) -> Vec<String> {
    let mut cargo = Command::new(env!("CARGO"));
    cargo
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["tree", "-p", package, "--edges", "normal"])
        .args(["--prefix", "none", "--format", "{p}", "--offline"]);
    if !features.is_empty() {
        cargo.args(["--features", &features.join(",")]);
    }
    let out = cargo.output().expect("cargo tree runs");
    assert!(
        out.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("cargo tree prints text")
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

/// The crates a build of `ljos_shell` links with `features`.
pub fn linked(features: &[&str]) -> Vec<String> {
    linked_by("ljos_shell", features)
}
