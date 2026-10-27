//! CLAUDE.md rule 5: an application that does not let its users change it
//! links no parser. The shell takes the watcher and the reload driver,
//! and with them the compiler, only behind its `dev` feature; this asks
//! cargo what a build without the feature links.

use std::process::Command;

/// Crates that parse or compile QML, and what only the dev loop needs.
const DEV_ONLY: [&str; 5] = [
    "qml_syntax",
    "qml_front",
    "ljos_compiler",
    "ljos_dev",
    "notify",
];

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

#[test]
fn the_shell_links_no_parser_without_the_dev_feature() {
    let crates = linked(&[]);
    assert!(
        crates.iter().any(|c| c == "ljos_engine"),
        "the list is the shell's: {crates:?}"
    );
    for name in DEV_ONLY {
        assert!(
            !crates.iter().any(|c| c == name),
            "ljos_shell links {name} without the `dev` feature"
        );
    }
}

#[test]
fn the_dev_feature_is_what_brings_the_parser_in() {
    // So that the test above cannot pass because the names went stale.
    let crates = linked(&["dev"]);
    for name in DEV_ONLY {
        assert!(
            crates.iter().any(|c| c == name),
            "{name} is not linked with the `dev` feature either"
        );
    }
}
