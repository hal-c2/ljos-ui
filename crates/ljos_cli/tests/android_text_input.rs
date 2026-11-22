//! The Android template's `TextInputOf.java`, compiled as it is and driven.
//! What it extends are stand-ins (`android_text_input/`) that make the calls
//! games-activity 4.4.0 makes, in its order; that Android makes them so is
//! for a device to show. It needs a JDK, not the Android SDK.

use std::path::{Path, PathBuf};
use std::process::Command;

fn java_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            java_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "java") {
            out.push(path);
        }
    }
}

/// `name` of the JDK `JAVA_HOME` names, else the one on `PATH`.
fn jdk(name: &str) -> PathBuf {
    std::env::var_os("JAVA_HOME").map_or_else(
        || PathBuf::from(name),
        |home| PathBuf::from(home).join("bin").join(name),
    )
}

#[test]
fn the_keyboards_connection_tells_the_native_side_only_what_the_method_changed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut sources = vec![
        root.join("../../android/app/src/main/java/com/google/androidgamesdk/TextInputOf.java"),
    ];
    java_files(&root.join("tests/android_text_input"), &mut sources);
    let classes = tempfile::tempdir().unwrap();
    let compiled = Command::new(jdk("javac"))
        .args(["-J-XX:-UsePerfData", "-encoding", "UTF-8", "-d"])
        .arg(classes.path())
        .args(&sources)
        .output();
    let compiled = match compiled {
        Ok(compiled) => compiled,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && std::env::var_os("CI").is_none() => {
            eprintln!("skipped: no javac in JAVA_HOME or on PATH");
            return;
        }
        Err(e) => panic!("javac: {e}"),
    };
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let ran = Command::new(jdk("java"))
        .args(["-XX:-UsePerfData", "-cp"])
        .arg(classes.path())
        .arg("com.google.androidgamesdk.Cases")
        .output()
        .unwrap();
    assert!(
        ran.status.success(),
        "{}{}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
}
