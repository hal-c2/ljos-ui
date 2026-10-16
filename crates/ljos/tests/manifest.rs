//! What a crate that depends on the facade (by path or by git) must
//! copy into its own manifest: nothing. Cargo applies `[patch]` only in
//! the workspace being built, so a patch here would never reach it.

const ROOT_MANIFEST: &str = include_str!("../../../Cargo.toml");
const ROOT_LOCK: &str = include_str!("../../../Cargo.lock");

#[test]
fn the_workspace_patches_no_dependency() {
    let patched: Vec<&str> = ROOT_MANIFEST
        .lines()
        .filter(|line| line.trim_start().starts_with("[patch"))
        .collect();
    assert!(
        patched.is_empty(),
        "a dependent of ljos would have to copy {patched:?}; depend on a fork by path, under a package name of its own"
    );
}

#[test]
fn the_parser_fork_is_a_package_of_its_own() {
    // The published `oxc_parser` has no QML mode: had it a place in the
    // graph, a dependent without the fork would build against it.
    assert!(ROOT_LOCK.contains("name = \"ljos_oxc_parser\""));
    assert!(!ROOT_LOCK.contains("name = \"oxc_parser\""));
}
