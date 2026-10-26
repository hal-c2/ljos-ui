//! `ljos design export`: the files it writes are the module Ljos.Design
//! that Ljós has built in, so an application imports the same thing on Qt.

use std::{fs, path::Path, process::Command};

fn export(dir: &Path, args: &[&str]) -> (String, String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_ljos"))
        .args(["design", "export"])
        .arg(dir)
        .args(args)
        .output()
        .expect("ljos runs");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().expect("ljos exits"),
    )
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort_unstable();
    names
}

#[test]
fn the_export_is_the_built_in_module() {
    let out = tempfile::tempdir().unwrap();
    let (stdout, stderr, status) = export(out.path(), &[]);
    assert_eq!(status, 0, "{stderr}");
    let written = out.path().join("Ljos/Design");
    assert_eq!(stdout.trim(), written.display().to_string());
    let built_in = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ljos_design_qml/qml/Ljos/Design");
    assert_eq!(names(&written), names(&built_in));
    for name in names(&built_in) {
        assert_eq!(
            fs::read_to_string(written.join(&name)).unwrap(),
            fs::read_to_string(built_in.join(&name)).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn a_seed_gives_one_palette_and_a_bad_one_is_refused() {
    let out = tempfile::tempdir().unwrap();
    let (_, stderr, status) = export(out.path(), &["--seed", "#1d4ed8"]);
    assert_eq!(status, 0, "{stderr}");
    let colors = fs::read_to_string(out.path().join("Ljos/Design/Colors.qml")).unwrap();
    assert_eq!(colors.matches("\"darkHigh\": [").count(), 1);

    let (_, stderr, status) = export(out.path(), &["--seed", "bright"]);
    assert_ne!(status, 0);
    assert!(stderr.contains("--seed bright"), "{stderr}");
}
