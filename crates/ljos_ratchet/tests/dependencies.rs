//! The judging half stays small: an application's acceptance runner that
//! depends on it builds no Gherkin, cucumber or Ljós crate. `cargo test -p
//! ljos_ratchet` building this crate alone is the other half of the check.

/// What `[dependencies]` may name: nothing that brings a backend.
const ALLOWED: [&str; 3] = ["thiserror", "blake3", "globset"];

#[test]
fn the_crate_depends_on_nothing_but_its_small_set() {
    let manifest = include_str!("../Cargo.toml");
    let mut section = "";
    let mut named = Vec::new();
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line;
            continue;
        }
        let Some((key, _)) = line.split_once('=') else {
            continue;
        };
        if section == "[dependencies]" && !line.starts_with('#') {
            named.push(key.trim().split('.').next().unwrap_or_default().to_owned());
        }
    }
    let extra: Vec<&String> = named
        .iter()
        .filter(|n| !ALLOWED.contains(&n.as_str()))
        .collect();
    assert!(
        extra.is_empty(),
        "the judging half must not depend on {extra:?}"
    );
    assert!(!named.is_empty(), "the manifest was read");
}
