//! A session over an application's files built ahead, with its user's
//! directory over them (ARCHITECTURE.md §12.1, a shipped application its
//! users change): what the build compiled is taken as it is when it was
//! compiled from the text the pack carries, and compiled again when not;
//! the user's files are read through the directory's checks only.

use std::collections::BTreeMap;
use std::sync::Arc;

use ljos_compiler::confine::UserDir;
use ljos_compiler::sources::PackSources;
use ljos_compiler::{FileSet, ReloadSession};
use qml_front::QtTypeOracle;

const MAIN: &str = "import QtQuick\nItem { Card { objectName: \"card\" } }\n";
const CARD: &str = "import QtQuick\nRectangle { width: 10 }\n";

/// What a build compiled `files` to, as its pack holds them.
fn built(files: &[(&str, &str)]) -> FileSet {
    let mut s = ReloadSession::in_memory(QtTypeOracle);
    for (key, text) in files {
        s.set_file(key, *text);
    }
    s.start().clone()
}

fn base(files: &[(&str, &str)]) -> Arc<PackSources> {
    Arc::new(PackSources {
        files: files
            .iter()
            .map(|(key, text)| (format!("{key}.qml"), (*text).to_owned()))
            .collect::<BTreeMap<_, _>>(),
        style: None,
    })
}

/// A layered session seeded with `pack`, its sources `sources`.
fn seeded(
    dir: &std::path::Path,
    sources: &[(&str, &str)],
    pack: &FileSet,
) -> (ReloadSession<QtTypeOracle>, Vec<bool>) {
    let mut s = ReloadSession::layered(UserDir::new(dir), base(sources), QtTypeOracle);
    let taken = sources
        .iter()
        .map(|(key, text)| s.seed(key, text, Arc::clone(&pack[*key])))
        .collect();
    let faults = s.start_seeded();
    assert!(faults.is_empty(), "{faults:?}");
    (s, taken)
}

#[test]
fn what_the_build_compiled_from_the_carried_text_is_taken_as_it_is() {
    let files = [("Main", MAIN), ("Card", CARD)];
    let pack = built(&files);
    let dir = tempfile::tempdir().unwrap();
    let (s, taken) = seeded(dir.path(), &files, &pack);
    assert_eq!(taken, [true, true]);
    for key in ["Main", "Card"] {
        assert!(
            Arc::ptr_eq(&s.live()[key], &pack[key]),
            "{key} compiled again"
        );
    }
}

#[test]
fn a_file_built_from_another_text_than_the_carried_one_is_compiled_again() {
    let pack = built(&[
        ("Main", MAIN),
        ("Card", "import QtQuick\nRectangle { width: 99 }\n"),
    ]);
    let files = [("Main", MAIN), ("Card", CARD)];
    let dir = tempfile::tempdir().unwrap();
    let (s, taken) = seeded(dir.path(), &files, &pack);
    assert_eq!(taken, [true, false]);
    // What uses it was compiled against the other text too: it is
    // compiled again, against the carried one.
    let now = built(&files);
    for key in ["Main", "Card"] {
        assert!(!Arc::ptr_eq(&s.live()[key], &pack[key]), "{key} taken");
        assert_eq!(s.live()[key], now[key], "{key}");
    }
}

#[test]
fn a_users_file_is_compiled_over_the_applications_and_the_rest_is_taken() {
    let files = [("Main", MAIN), ("Card", CARD)];
    let pack = built(&files);
    let dir = tempfile::tempdir().unwrap();
    let users = "import QtQuick\nRectangle { width: 30 }\n";
    std::fs::write(dir.path().join("Card.qml"), users).unwrap();
    let (s, _) = seeded(dir.path(), &files, &pack);
    assert_eq!(
        s.live()["Card"],
        built(&[("Main", MAIN), ("Card", users)])["Card"]
    );
    assert_ne!(s.live()["Card"], pack["Card"]);
}

#[test]
fn a_users_new_file_is_a_type_of_its_own() {
    let files = [("Main", MAIN), ("Card", CARD)];
    let pack = built(&files);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Shell.qml"), "import QtQuick\nItem {}\n").unwrap();
    let (s, _) = seeded(dir.path(), &files, &pack);
    assert!(s.live().contains_key("Shell"));
    assert!(Arc::ptr_eq(&s.live()["Card"], &pack["Card"]));
}

#[test]
fn a_session_without_the_users_files_reads_them_at_the_next_rescan() {
    let files = [("Main", MAIN), ("Card", CARD)];
    let pack = built(&files);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Card.qml"),
        "import QtQuick\nRectangle { width: 30 }\n",
    )
    .unwrap();
    let mut s = ReloadSession::layered_base(UserDir::new(dir.path()), base(&files), QtTypeOracle);
    for (key, text) in files {
        assert!(s.seed(key, text, Arc::clone(&pack[key])));
    }
    assert!(s.start_seeded().is_empty());
    assert!(Arc::ptr_eq(&s.live()["Card"], &pack["Card"]));
    s.rescan().unwrap();
    let patch = s.build().expect("compiles").expect("the user's file");
    assert_eq!(patch.files.len(), 1);
    assert_eq!(patch.files[0].file.file, "Card");
}

#[test]
fn a_users_file_that_is_removed_gives_the_applications_back() {
    let files = [("Main", MAIN), ("Card", CARD)];
    let pack = built(&files);
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Card.qml"),
        "import QtQuick\nRectangle { width: 30 }\n",
    )
    .unwrap();
    let (mut s, _) = seeded(dir.path(), &files, &pack);
    assert_ne!(s.live()["Card"], pack["Card"]);
    s.remove_file("Card");
    let patch = s
        .build()
        .expect("compiles")
        .expect("the application's file again");
    s.ack(patch.generation);
    assert_eq!(s.live()["Card"], pack["Card"]);
}

#[test]
fn a_path_outside_the_users_directory_is_refused_and_told() {
    let files = [("Main", MAIN), ("Card", CARD)];
    let pack = built(&files);
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    std::fs::write(other.path().join("Evil.qml"), "import QtQuick\nItem {}\n").unwrap();
    let (mut s, _) = seeded(dir.path(), &files, &pack);
    assert!(s.read_file(&other.path().join("Evil.qml")).is_err());
    let told = s.take_refused();
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(
        told[0].message.starts_with("refused: "),
        "{}",
        told[0].message
    );
    assert!(s.take_refused().is_empty(), "told once");
}

const USES: &str =
    "import QtQuick\nimport \"calc.js\" as Calc\nItem { property int n: Calc.n() }\n";
const CALC: &str = "function n() { return 1 }\n";

/// A layered session over `Main` that imports `calc.js`, seeded with
/// what a build compiled from both.
fn seeded_with_script(
    dir: &std::path::Path,
) -> (
    ReloadSession<QtTypeOracle>,
    FileSet,
    Vec<ljos_compiler::CompileFault>,
) {
    let pack = built(&[("Main", USES), ("calc.js", CALC)]);
    let base = Arc::new(PackSources {
        files: [("Main.qml", USES), ("calc.js", CALC)]
            .into_iter()
            .map(|(k, t)| (k.to_owned(), t.to_owned()))
            .collect(),
        style: None,
    });
    let mut s = ReloadSession::layered(UserDir::new(dir), base, QtTypeOracle);
    assert!(s.seed("Main", USES, Arc::clone(&pack["Main"])));
    let faults = s.start_seeded();
    (s, pack, faults)
}

#[test]
fn a_users_script_compiles_again_what_imports_it() {
    let dir = tempfile::tempdir().unwrap();
    let theirs = "function n() { return 2 }\n";
    std::fs::write(dir.path().join("calc.js"), theirs).unwrap();
    let (s, pack, faults) = seeded_with_script(dir.path());
    assert!(faults.is_empty(), "{faults:?}");
    assert!(!Arc::ptr_eq(&s.live()["Main"], &pack["Main"]), "Main taken");
    assert_eq!(
        s.live()["Main"],
        built(&[("Main", USES), ("calc.js", theirs)])["Main"]
    );
}

#[test]
fn the_applications_script_keeps_what_imports_it_taken() {
    let dir = tempfile::tempdir().unwrap();
    let (s, pack, faults) = seeded_with_script(dir.path());
    assert!(faults.is_empty(), "{faults:?}");
    assert!(Arc::ptr_eq(&s.live()["Main"], &pack["Main"]));
}

#[test]
fn a_users_script_that_does_not_parse_is_told_and_what_imports_it_runs_as_built() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("calc.js"), "function n( {\n").unwrap();
    let (mut s, pack, faults) = seeded_with_script(dir.path());
    assert_eq!(faults.len(), 1, "{faults:?}");
    assert_eq!(faults[0].file, "calc.js");
    assert!(Arc::ptr_eq(&s.live()["Main"], &pack["Main"]));

    // A save that still does not parse is refused; one that does is built.
    std::fs::write(dir.path().join("calc.js"), "function n( {\n  return 3\n").unwrap();
    s.rescan().unwrap();
    assert!(s.build().is_err());
    std::fs::write(dir.path().join("calc.js"), "function n() { return 3 }\n").unwrap();
    s.rescan().unwrap();
    let patch = s.build().expect("compiles").expect("Main again");
    assert!(patch.files.iter().any(|f| f.file.file == "Main"));
}
