//! ARCHITECTURE.md §19.7: the inspector is off unless a build asks for
//! it. The shell takes the MCP SDK and the HTTP server only behind its
//! `mcp` feature; this asks cargo what a build without the feature links,
//! as `no_parser.rs` does for the compiler.

use std::process::Command;

/// The MCP SDK, the crate that alone names it, and the HTTP server.
const MCP_ONLY: [&str; 4] = ["rmcp", "ljos_mcp_core", "hyper", "hyper-util"];

/// The crates a build of `ljos_shell` links (no dev- or build-dependencies).
fn linked(features: &[&str]) -> Vec<String> {
    let mut cargo = Command::new(env!("CARGO"));
    cargo
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["tree", "-p", "ljos_shell", "--edges", "normal"])
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

fn links_none(features: &[&str]) {
    let crates = linked(features);
    assert!(
        crates.iter().any(|c| c == "ljos_engine"),
        "the list is the shell's: {crates:?}"
    );
    for name in MCP_ONLY {
        assert!(
            !crates.iter().any(|c| c == name),
            "ljos_shell links {name} with the features {features:?}"
        );
    }
}

#[test]
fn the_shell_links_no_mcp_by_default() {
    links_none(&[]);
}

#[test]
fn the_dev_feature_alone_links_no_mcp() {
    // Hot reload and the `top` overlay need neither.
    links_none(&["dev"]);
}

#[test]
fn the_mcp_feature_is_what_brings_it_in() {
    // So that the tests above cannot pass because the names went stale.
    let crates = linked(&["mcp"]);
    for name in MCP_ONLY {
        assert!(
            crates.iter().any(|c| c == name),
            "{name} is not linked with the `mcp` feature either"
        );
    }
}
