//! The Qt documentation's examples, compiled against Qt's types: the
//! compiler's baseline. The corpus is a submodule
//! (`git submodule update --init corpus/qtdoc`) and the type table is made
//! from a local Qt (`mise run types`). The examples use what Qt has, not
//! what Ljós has so far, so this is a measure against Qt's table only:
//! without it, or without the corpus, each test says on the terminal that
//! it was skipped and why, and passes.

use std::path::{Path, PathBuf};

use ljos_compiler::{Project, baseline};
use qml_front::QtTypeOracle;

fn examples(test: &str) -> Option<PathBuf> {
    if !qt_types::test_has_qt_table(test) {
        return None;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/qtdoc/examples");
    if !root.is_dir() {
        use std::io::Write as _;
        // To the stream itself: the harness swallows what a passing test
        // prints.
        let _ = writeln!(
            std::io::stderr(),
            "SKIPPED {test}: no corpus at {} (`git submodule update --init corpus/qtdoc`)",
            root.display()
        );
        return None;
    }
    Some(root)
}

/// One small project, quick enough for every test run.
#[test]
fn a_tutorial_compiles_whole() {
    let Some(root) = examples("a_tutorial_compiles_whole") else {
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
    let Some(root) = examples("the_corpus_baseline") else {
        return;
    };
    let measured = baseline::measure(&root, || QtTypeOracle);
    println!("{measured}");
    assert!(measured.files > 0);
    // What the parser gives up on is a regression of the front end.
    assert_eq!(measured.failed, 0, "{:?}", measured.errors);
}

/// Qt loads every example, so none of them writes a plain value that is no
/// key for a property that holds an enum: an "unknown enumeration" here is
/// the compiler refusing what Qt takes.
#[test]
#[ignore = "compiles the whole corpus"]
fn no_example_is_refused_for_an_enum_value() {
    let Some(root) = examples("no_example_is_refused_for_an_enum_value") else {
        return;
    };
    let mut refused = Vec::new();
    for dir in baseline::project_dirs(&root) {
        let Ok(project) = Project::load(&dir, QtTypeOracle) else {
            continue;
        };
        for key in project.files() {
            let diagnostics = match ljos_compiler::compile_file(&project, key) {
                Ok(compiled) => compiled.diagnostics,
                Err(diagnostics) => diagnostics,
            };
            for diagnostic in diagnostics {
                if diagnostic.message.contains("unknown enumeration") {
                    refused.push(format!("{}/{key}:{diagnostic}", dir.display()));
                }
            }
        }
    }
    assert!(refused.is_empty(), "{refused:#?}");
}
