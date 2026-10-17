//! `ljos shaders`: what it prints, what it writes and what status it
//! leaves, which is what a build script is written against.

use std::{path::Path, process::Command};

use ljos_shader::{Compiled, Stage};

const FRAG: &str = "#version 440
layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;
layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec4 tint;
};
void main() { fragColor = tint * qt_Opacity; }
";

const BROKEN: &str = "#version 440
layout(location = 0) out vec4 fragColor;
void main() { fragColor = nothing; }
";

fn write(dir: &Path, name: &str, source: &str) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
}

/// Runs `ljos shaders` in `dir`: its standard output, its standard error
/// and its status.
fn shaders(dir: &Path, args: &[&str]) -> (String, String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_ljos"))
        .arg("shaders")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("ljos runs");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().expect("ljos exits"),
    )
}

#[test]
fn a_directory_gets_an_artifact_per_shader_and_a_second_run_writes_none() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "shaders/tint.frag", FRAG);
    write(dir.path(), "shaders/notes.txt", "not a shader");
    let (out, err, status) = shaders(dir.path(), &["."]);
    assert_eq!(out, "compiled shaders/tint.frag\n1 shaders, 1 compiled\n");
    assert_eq!(err, "");
    assert_eq!(status, 0);
    let text = std::fs::read_to_string(dir.path().join("shaders/tint.frag.ljsh")).unwrap();
    let artifact = Compiled::from_artifact(&text).expect("the artifact reads");
    assert_eq!(artifact.stage, Stage::Fragment);

    let (out, _, status) = shaders(dir.path(), &["."]);
    assert_eq!(out, "1 shaders, 0 compiled\n");
    assert_eq!(status, 0);
}

#[test]
fn check_names_what_is_missing_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "tint.frag", FRAG);
    let (out, _, status) = shaders(dir.path(), &["--check", "tint.frag"]);
    assert_eq!(
        out,
        "would compile tint.frag\n1 shaders, 1 would be compiled\n"
    );
    assert_eq!(status, 1);
    assert!(!dir.path().join("tint.frag.ljsh").exists());
}

#[test]
fn a_shader_that_does_not_compile_is_an_error_and_the_others_are_still_written() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.frag", BROKEN);
    write(dir.path(), "b.frag", FRAG);
    let (out, err, status) = shaders(dir.path(), &["."]);
    assert_eq!(out, "compiled b.frag\n2 shaders, 1 compiled\n");
    assert!(err.starts_with("a.frag: error: "), "{err}");
    assert_eq!(status, 2);
    assert!(!dir.path().join("a.frag.ljsh").exists());
    assert!(dir.path().join("b.frag.ljsh").exists());
}
