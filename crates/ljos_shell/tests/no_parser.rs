//! CLAUDE.md rule 5: an application that does not let its users change it
//! links no parser. The shell takes the watcher and the reload driver,
//! and with them the compiler, only behind its `dev` feature; this asks
//! cargo what a build without the feature links.

mod common;

use common::linked;

/// Crates that parse or compile QML, and what only the dev loop needs.
const DEV_ONLY: [&str; 5] = [
    "qml_syntax",
    "qml_front",
    "ljos_compiler",
    "ljos_dev",
    "notify",
];

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
