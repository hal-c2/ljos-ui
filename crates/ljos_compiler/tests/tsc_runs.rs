//! How `ljos check --tsc` treats what TypeScript says, with a stand-in for
//! `tsc` that says what each test needs, so the outcome does not depend on
//! the TypeScript the machine has.

#![cfg(unix)]

use std::{os::unix::fs::PermissionsExt, path::Path};

use ljos_compiler::ts::{self, Checker, TsError};

const GAUGE: &str = "import Ljos.Ui\nItem {\n    property real level: 0\n}\n";

/// A `tsc` that prints `said` and exits with `status`.
fn fake_tsc(dir: &Path, said: &str, status: i32) -> Checker {
    let path = dir.join("tsc");
    std::fs::write(
        &path,
        format!("#!/bin/sh\ncat <<'EOF'\n{said}\nEOF\nexit {status}\n"),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    Checker::Tsc(path)
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Gauge.qml"), GAUGE).unwrap();
    dir
}

/// Declarations TypeScript cannot read are a failed check, not a clean one:
/// every file would otherwise pass for want of anything to check against.
#[test]
fn an_error_in_the_declarations_fails_the_check() {
    let project = project();
    let bin = tempfile::tempdir().unwrap();
    let tsc = fake_tsc(
        bin.path(),
        "ljos.d.ts(3,23): error TS2304: Cannot find name 'Missing'.",
        2,
    );
    match ts::check_dir(project.path(), Some(tsc)) {
        Err(TsError::Declarations { said }) => {
            assert!(said.contains("ljos.d.ts:3:23"), "{said}");
            assert!(
                said.contains("Cannot find name 'Missing'. [TS2304]"),
                "{said}"
            );
        }
        other => panic!("expected the declarations' error, got {other:?}"),
    }
}

/// The scratch directory is new and private: one made beforehand under the
/// name the check would use is not written through.
#[test]
fn the_scratch_directory_is_not_one_made_beforehand() {
    let project = project();
    let bin = tempfile::tempdir().unwrap();
    let victim = bin.path().join("victim");
    std::fs::write(&victim, "kept").unwrap();
    let mut planted = Vec::new();
    for n in 0..64 {
        let dir = std::env::temp_dir().join(format!("ljos-tsc-{}-{n}", std::process::id()));
        if std::fs::create_dir(&dir).is_ok() {
            for name in [ts::DECLARATIONS, "f0.ts"] {
                std::os::unix::fs::symlink(&victim, dir.join(name)).unwrap();
            }
            planted.push(dir);
        }
    }
    let tsc = fake_tsc(bin.path(), "", 0);
    let outcome = ts::check_dir(project.path(), Some(tsc));
    for dir in &planted {
        let _ = std::fs::remove_dir_all(dir);
    }
    outcome.expect("the check ran");
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "kept");
}
