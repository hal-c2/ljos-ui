//! A program that decodes or shows video without playing sound leaves the
//! sound library out: `media` is the decoders, `media-audio` the system's
//! output device. This asks cargo what a build of the facade links.

use std::process::Command;

/// The crates a build of `ljos` links (no dev- or build-dependencies:
/// the facade's tests take its default features).
fn linked(features: &[&str]) -> Vec<String> {
    let mut cargo = Command::new(env!("CARGO"));
    cargo
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["tree", "-p", "ljos", "--edges", "normal"])
        .args(["--prefix", "none", "--format", "{p}", "--offline"])
        .arg("--no-default-features");
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

fn links(crates: &[String], name: &str) -> bool {
    crates.iter().any(|c| c == name)
}

#[test]
fn media_alone_decodes_without_the_sound_library() {
    for features in [&["media"][..], &["media", "shell"]] {
        let crates = linked(features);
        assert!(links(&crates, "ljos_media"), "{features:?}: {crates:?}");
        assert!(
            !links(&crates, "cpal"),
            "ljos links cpal with only {features:?}"
        );
    }
}

#[test]
fn media_audio_is_what_brings_it_in() {
    // So that the test above cannot pass because the name went stale.
    let crates = linked(&["media-audio"]);
    assert!(links(&crates, "cpal"), "{crates:?}");
}
