//! The Qt documentation's examples, compiled against Qt's types: the
//! compiler's baseline. The corpus is a submodule
//! (`git submodule update --init corpus/qtdoc`) and the type table is made
//! from a local Qt (`mise run types`); without either, these tests say so
//! and pass.

use std::path::{Path, PathBuf};

use ljos_compiler::{Project, baseline};
use qml_front::QtTypeOracle;

fn examples() -> Option<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/qtdoc/examples");
    if !root.is_dir() {
        eprintln!("skipped: no corpus at {}", root.display());
        return None;
    }
    if !qt_types::is_available() {
        eprintln!("skipped: no Qt type table");
        return None;
    }
    Some(root)
}

/// One small project, quick enough for every test run.
#[test]
fn a_tutorial_compiles_whole() {
    let Some(root) = examples() else {
        return;
    };
    let dir = root.join("tutorials/alarms");
    let project = Project::load(&dir, QtTypeOracle).expect("the tutorial is readable");
    let mut measured = baseline::Baseline::default();
    measured.add(&project, "tutorials/alarms");
    eprint!("{measured}");
    assert!(measured.files > 0);
    assert_eq!(measured.failed, 0);
    assert_eq!(measured.partial, 0, "{:?}", measured.errors);
    assert_eq!(
        measured.resolved, measured.bindings,
        "{:?}",
        measured.unknown
    );
}

/// Every example: `cargo test -p ljos_compiler --release -- --ignored`, or
/// `mise run compile`.
#[test]
#[ignore = "compiles the whole corpus"]
fn the_corpus_baseline() {
    let Some(root) = examples() else {
        return;
    };
    let measured = baseline::measure(&root, || QtTypeOracle);
    println!("{measured}");
    assert!(measured.files > 0);
    // What the parser gives up on is a regression of the front end.
    assert_eq!(measured.failed, 0, "{:?}", measured.errors);
}
