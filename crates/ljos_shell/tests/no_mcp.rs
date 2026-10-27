//! ARCHITECTURE.md §19.7: the inspector is off unless a build asks for
//! it. The shell takes the MCP SDK and the HTTP server only behind its
//! `mcp` feature; this asks cargo what a build without the feature links,
//! as `no_parser.rs` does for the compiler.

mod common;

use common::linked;

/// The MCP SDK, the crate that alone names it, and the HTTP server.
const MCP_ONLY: [&str; 4] = ["rmcp", "ljos_mcp_core", "hyper", "hyper-util"];

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
