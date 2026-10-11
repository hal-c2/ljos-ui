//! ARCHITECTURE.md §7.3: the shell has its own loop. eframe is taken
//! only behind the `eframe` feature, for a host whose application is an
//! eframe one; this asks cargo what a build without the feature links, as
//! `no_mcp.rs` does for the inspector.

use std::process::Command;

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

fn links_no_eframe(features: &[&str]) {
    let crates = linked(features);
    for name in ["ljos_engine", "ljos_host", "egui-winit", "egui-wgpu"] {
        assert!(
            crates.iter().any(|c| c == name),
            "the shell's loop needs {name}: {crates:?}"
        );
    }
    assert!(
        !crates.iter().any(|c| c == "eframe"),
        "ljos_shell links eframe with the features {features:?}"
    );
}

#[test]
fn the_shell_runs_without_eframe() {
    links_no_eframe(&[]);
}

#[test]
fn the_development_features_link_no_eframe_either() {
    links_no_eframe(&["dev"]);
    links_no_eframe(&["mcp"]);
}

#[test]
fn the_eframe_feature_is_what_brings_it_in() {
    // So that the tests above cannot pass because the name went stale.
    let crates = linked(&["eframe"]);
    assert!(crates.iter().any(|c| c == "eframe"), "{crates:?}");
}

#[test]
fn the_host_links_no_egui() {
    // The host is the platform's side only (ARCHITECTURE.md §10.1).
    let out = Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["tree", "-p", "ljos_host", "--edges", "normal"])
        .args(["--prefix", "none", "--format", "{p}", "--offline"])
        .output()
        .expect("cargo tree runs");
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).expect("cargo tree prints text");
    let crates: Vec<&str> = text
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert!(
        crates.contains(&"winit"),
        "the list is the host's: {crates:?}"
    );
    for name in ["egui", "epaint", "emath", "ecolor", "eframe"] {
        assert!(!crates.contains(&name), "ljos_host links {name}");
    }
}
