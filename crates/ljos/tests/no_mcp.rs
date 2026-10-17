//! ARCHITECTURE.md §19.7 and principle 5: an application on `ljos` with
//! its default features links neither the inspector nor the parser; the
//! `mcp` feature is what brings them in. This asks cargo, as the shell's
//! `no_mcp.rs` does.

use std::process::Command;

/// The MCP SDK, the crates that serve it, the HTTP server, and the parser
/// the development crate links.
const INSPECTOR_ONLY: [&str; 5] = ["rmcp", "ljos_mcp_core", "ljos_dev", "hyper", "qml_front"];

/// The crates a build of `ljos` links (no dev- or build-dependencies).
fn linked(features: &[&str]) -> Vec<String> {
    let mut cargo = Command::new(env!("CARGO"));
    cargo
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["tree", "-p", "ljos", "--edges", "normal"])
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

#[test]
fn an_application_links_no_inspector_by_default() {
    let crates = linked(&[]);
    assert!(
        crates.iter().any(|c| c == "ljos_shell"),
        "the list is the facade's: {crates:?}"
    );
    for name in INSPECTOR_ONLY {
        assert!(
            !crates.iter().any(|c| c == name),
            "ljos links {name} by default"
        );
    }
}

#[test]
fn the_mcp_feature_is_what_brings_it_in() {
    // So that the test above cannot pass because the names went stale.
    let crates = linked(&["mcp"]);
    for name in INSPECTOR_ONLY {
        assert!(
            crates.iter().any(|c| c == name),
            "{name} is not linked with the `mcp` feature either"
        );
    }
}
