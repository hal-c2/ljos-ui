//! Saves in, a patch out: the reloader without a thread or a clock.

use std::fs;

use ljos_dev::{Build, Reloader, Touch, classify, reloader::project_dirs};
use qml_front::QtTypeOracle;

const MAIN: &str = "import QtQuick\nItem {\n  width: 10\n}\n";

fn started(files: &[(&str, &str)]) -> Reloader<QtTypeOracle> {
    let mut r = Reloader::in_memory(QtTypeOracle);
    for (key, text) in files {
        r.save(key, Some((*text).to_owned()));
    }
    r.start();
    let failed: Vec<&str> = r.session().failures().map(|(key, _)| key).collect();
    assert!(failed.is_empty(), "{failed:?} do not compile");
    r
}

/// A build, and the build after the quiet time again when the first held
/// back an error: what a watcher with no event between them tells.
fn told(r: &mut Reloader<QtTypeOracle>) -> Build {
    match r.build() {
        Build::Unsettled => r.build(),
        other => other,
    }
}

fn patch_of(r: &mut Reloader<QtTypeOracle>) -> u64 {
    match r.build() {
        Build::Patch { patch, .. } => patch.generation,
        other => panic!("expected a patch, got {other:?}"),
    }
}

#[test]
fn nothing_saved_is_no_change() {
    let mut r = started(&[("Main", MAIN)]);
    assert!(!r.has_saves());
    assert_eq!(r.build(), Build::NoChange);
}

#[test]
fn the_same_text_saved_again_compiles_nothing() {
    let mut r = started(&[("Main", MAIN)]);
    r.save("Main", Some(MAIN.to_owned()));
    assert!(r.has_saves());
    assert_eq!(r.build(), Build::NoChange);
}

#[test]
fn another_text_is_a_patch_until_it_is_answered() {
    let mut r = started(&[("Main", MAIN)]);
    r.save("Main", Some(MAIN.replace("10", "20")));
    let Build::Patch { patch, .. } = r.build() else {
        panic!("expected a patch");
    };
    assert_eq!((patch.base, patch.generation), (0, 1));
    assert_eq!(patch.files.len(), 1);
    r.ack(1);
    assert_eq!(r.session().generation(), 1);
    assert_eq!(r.build(), Build::NoChange);
}

#[test]
fn the_last_save_of_a_file_counts() {
    let mut r = started(&[("Main", MAIN)]);
    r.save("Main", Some(MAIN.replace("10", "20")));
    r.save("Main", Some(MAIN.to_owned()));
    // Changed and changed back within the quiet time: nothing happened.
    assert_eq!(r.build(), Build::NoChange);
}

#[test]
fn a_file_that_does_not_compile_fails_the_build_and_says_where() {
    let mut r = started(&[("Main", MAIN)]);
    r.save(
        "Main",
        Some("import QtQuick\nItem {\n  width: \n".to_owned()),
    );
    let Build::Failed(faults) = told(&mut r) else {
        panic!("expected a failure");
    };
    assert_eq!(faults[0].file, "Main.qml");
    assert!(faults[0].line >= 3, "{:?}", faults[0]);
    // The same broken text again is not compiled again: the error stands.
    r.save(
        "Main",
        Some("import QtQuick\nItem {\n  width: \n".to_owned()),
    );
    assert_eq!(r.build(), Build::NoChange);
}

#[test]
fn undoing_a_broken_edit_says_all_is_well_again() {
    let mut r = started(&[("Main", MAIN)]);
    r.save("Main", Some("import QtQuick\nItem {".to_owned()));
    assert!(matches!(told(&mut r), Build::Failed(_)));
    r.save("Main", Some(MAIN.to_owned()));
    assert_eq!(r.build(), Build::UpToDate);
}

#[test]
fn a_refused_patch_is_built_again_from_what_runs() {
    let mut r = started(&[("Main", MAIN)]);
    r.save("Main", Some(MAIN.replace("10", "20")));
    assert_eq!(patch_of(&mut r), 1);
    r.reject(1);
    assert_eq!(r.session().generation(), 0);
    r.save("Main", Some(MAIN.replace("10", "30")));
    let Build::Patch { patch, .. } = r.build() else {
        panic!("expected a patch");
    };
    // From what runs, not from what was refused.
    assert_eq!((patch.base, patch.generation), (0, 2));
}

#[test]
fn a_file_can_come_and_go() {
    let mut r = started(&[("Main", MAIN)]);
    r.save("Badge", Some("import QtQuick\nItem {}\n".to_owned()));
    let Build::Patch { patch, .. } = r.build() else {
        panic!("expected a patch");
    };
    assert!(patch.files.iter().any(|f| f.added));
    r.ack(patch.generation);
    r.save("Badge", None);
    let Build::Patch { patch, .. } = r.build() else {
        panic!("expected a patch");
    };
    assert_eq!(patch.removed, ["Badge".to_owned()]);
    // A file that never was is not missed.
    r.ack(patch.generation);
    r.save("Never", None);
    assert_eq!(r.build(), Build::NoChange);
}

#[test]
fn a_project_that_starts_broken_offers_the_file_once_it_compiles() {
    let mut r = Reloader::in_memory(QtTypeOracle);
    r.save("Main", Some("import QtQuick\nItem {".to_owned()));
    let live = r.start();
    assert!(live.is_empty());
    let failed: Vec<&str> = r.session().failures().map(|(key, _)| key).collect();
    assert_eq!(failed, ["Main"]);
    assert_eq!(r.build(), Build::NoChange);
    r.save("Main", Some(MAIN.to_owned()));
    let Build::Patch { patch, .. } = r.build() else {
        panic!("expected a patch");
    };
    assert_eq!((patch.base, patch.generation), (0, 1));
    assert!(patch.files.iter().all(|f| f.added));
    r.ack(1);
    assert_eq!(r.session().failures().count(), 0);
}

#[test]
fn a_file_that_failed_at_the_start_does_not_hold_back_a_save_of_another() {
    let broken = "import QtQuick\nItem {\n  NoSuchThing { }\n}\n";
    let mut r = Reloader::in_memory(QtTypeOracle);
    r.save("Main", Some(MAIN.to_owned()));
    r.save("Broken", Some(broken.to_owned()));
    let live = r.start();
    assert_eq!(live.keys().collect::<Vec<_>>(), ["Main"]);

    r.save("Main", Some(MAIN.replace("10", "20")));
    assert_eq!(patch_of(&mut r), 1);
    r.ack(1);

    // A save of the broken file that is still broken is a reload error,
    // and what runs is kept.
    r.save("Broken", Some(broken.replace("NoSuchThing", "NorThis")));
    let Build::Failed(faults) = told(&mut r) else {
        panic!("expected the save to fail");
    };
    assert_eq!(faults[0].file, "Broken.qml");

    // Mended, it is added.
    r.save("Broken", Some("import QtQuick\nItem {}\n".to_owned()));
    let Build::Patch { patch, .. } = r.build() else {
        panic!("expected a patch");
    };
    assert!(
        patch
            .files
            .iter()
            .any(|f| f.added && f.file.file == "Broken")
    );
}

#[test]
fn a_file_that_failed_at_the_start_and_is_removed_goes_from_the_source() {
    let mut r = Reloader::in_memory(QtTypeOracle);
    r.save("Main", Some(MAIN.to_owned()));
    r.save("Broken", Some("import QtQuick\nItem {".to_owned()));
    r.start();
    r.save("Broken", None);
    let Build::Patch { patch, .. } = r.build() else {
        panic!("expected a patch");
    };
    assert_eq!(patch.removed, ["Broken".to_owned()]);
    r.ack(patch.generation);
    assert_eq!(r.session().failures().count(), 0);
}

#[test]
fn the_lint_for_stateful_siblings_comes_with_the_patch() {
    let mut r = started(&[("Main", MAIN)]);
    r.save(
        "Main",
        Some(
            "import QtQuick\nItem {\n  Rectangle { property int taps: 0 }\n  \
             Rectangle { property int taps: 0 }\n}\n"
                .to_owned(),
        ),
    );
    let Build::Patch { warnings, .. } = r.build() else {
        panic!("expected a patch");
    };
    assert!(
        warnings.iter().any(|w| w.message.contains("id")),
        "{warnings:?}"
    );
}

#[test]
fn files_on_disk_are_read_when_the_build_starts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let main = root.join("Main.qml");
    fs::write(&main, MAIN).unwrap();
    let mut r = Reloader::open(&root, QtTypeOracle).unwrap();
    r.start();

    // An event with nothing behind it.
    assert!(r.touch(&main, false));
    assert_eq!(r.build(), Build::NoChange);

    // Two writes, one build: the text at the time of the build counts.
    fs::write(&main, "import QtQuick\nItem {").unwrap();
    assert!(r.touch(&main, false));
    fs::write(&main, MAIN.replace("10", "20")).unwrap();
    assert!(r.touch(&main, false));
    assert_eq!(patch_of(&mut r), 1);
    r.ack(1);

    // A file that is gone, and one that came.
    let badge = root.join("Badge.qml");
    fs::write(&badge, "import QtQuick\nItem {}\n").unwrap();
    assert!(r.touch(&badge, false));
    assert_eq!(patch_of(&mut r), 2);
    r.ack(2);
    fs::remove_file(&badge).unwrap();
    assert!(r.touch(&badge, false));
    let Build::Patch { patch, .. } = r.build() else {
        panic!("expected a patch");
    };
    assert_eq!(patch.removed, ["Badge".to_owned()]);
}

#[test]
fn a_file_found_empty_while_being_written_waits_for_its_next_event() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let main = root.join("Main.qml");
    fs::write(&main, MAIN).unwrap();
    let mut r = Reloader::open(&root, QtTypeOracle).unwrap();
    r.start();

    // Emptied by a save in place, its writer not done: no build until the
    // file's next event, however late.
    fs::write(&main, "").unwrap();
    assert!(r.touch(&main, false));
    assert_eq!(r.build(), Build::Waiting);
    assert_eq!(r.build(), Build::NoChange);
    fs::write(&main, MAIN.replace("10", "20")).unwrap();
    assert!(r.closed(&main));
    assert_eq!(patch_of(&mut r), 1);
    r.ack(1);

    // Its writer closed it empty: that is the save.
    fs::write(&main, "").unwrap();
    assert!(r.closed(&main));
    assert!(matches!(told(&mut r), Build::Failed(_)));
}

#[test]
fn a_reload_asked_for_takes_a_file_being_written_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let main = root.join("Main.qml");
    fs::write(&main, MAIN).unwrap();
    let mut r = Reloader::open(&root, QtTypeOracle).unwrap();
    r.start();
    fs::write(&main, "").unwrap();
    assert!(r.touch(&main, false));
    assert_eq!(r.build(), Build::Waiting);
    assert!(matches!(r.build_now(), Build::Failed(_)));
}

#[test]
fn a_saved_script_recompiles_the_files_that_import_it() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let logic = root.join("logic.js");
    fs::write(&logic, "function size() { return 10 }\n").unwrap();
    let main = "import QtQuick\nimport \"logic.js\" as Logic\nItem {\n  width: Logic.size()\n}\n";
    fs::write(root.join("Main.qml"), main).unwrap();
    fs::write(root.join("Other.qml"), "import QtQuick\nItem {}\n").unwrap();
    let mut r = Reloader::open(&root, QtTypeOracle).unwrap();
    r.start();

    fs::write(&logic, "function size() { return 20 }\n").unwrap();
    assert!(r.closed(&logic));
    let Build::Patch { patch, .. } = r.build() else {
        panic!("expected a patch");
    };
    let files: Vec<&str> = patch.files.iter().map(|f| f.file.file.as_str()).collect();
    assert_eq!(files, ["Main"]);
    r.ack(patch.generation);

    // A script that does not parse is refused where it is wrong, and the
    // build after its fix is offered.
    fs::write(&logic, "\nfunction size() { return 20 * }\n").unwrap();
    assert!(r.closed(&logic));
    let Build::Failed(faults) = told(&mut r) else {
        panic!("expected a failure");
    };
    assert_eq!((faults[0].file.as_str(), faults[0].line), ("logic.js", 2));
    fs::write(&logic, "function size() { return 30 }\n").unwrap();
    assert!(r.closed(&logic));
    assert!(matches!(r.build(), Build::Patch { .. }));
}

#[test]
fn a_new_directory_is_found_by_reading_again() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::write(root.join("Main.qml"), MAIN).unwrap();
    let mut r = Reloader::open(&root, QtTypeOracle).unwrap();
    r.start();
    let parts = root.join("parts");
    fs::create_dir(&parts).unwrap();
    fs::write(parts.join("Knob.qml"), "import QtQuick\nItem {}\n").unwrap();
    assert!(r.touch(&parts, true));
    assert!(r.rescan_pending());
    let Build::Patch { patch, .. } = r.build() else {
        panic!("expected a patch");
    };
    assert_eq!(patch.files[0].file.file, "parts/Knob");
    assert_eq!(project_dirs(&root), [root.clone(), parts]);
}

#[test]
fn paths_that_are_nothing_to_a_build_are_told_apart() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::create_dir_all(root.join("target/debug")).unwrap();
    fs::create_dir(root.join("ui")).unwrap();
    let c = |rel: &str, is_dir: bool| classify(&root, &root.join(rel), is_dir);
    assert_eq!(c("Main.qml", false), Touch::File);
    assert_eq!(c("ui/Page.qml", false), Touch::File);
    assert_eq!(c("ui/logic.js", false), Touch::File);
    assert_eq!(c("ui/logic.mjs", false), Touch::File);
    assert_eq!(c("ui/qmldir", false), Touch::Modules);
    assert_eq!(c("CMakeLists.txt", false), Touch::Modules);
    assert_eq!(c("ui", false), Touch::Rescan);
    assert_eq!(c("gone", true), Touch::Rescan);
    // A directory that was moved away, as a rename tells it.
    assert_eq!(c("gone", false), Touch::Rescan);
    // Shown again, not compiled.
    assert_eq!(c("picture.png", false), Touch::Picture);
    assert_eq!(c("ui/icons/Logo.SVG", false), Touch::Picture);
    assert_eq!(c("target/debug/picture.png", false), Touch::Ignore);
    // Editors' and builds' own.
    assert_eq!(c(".Main.qml.swp", false), Touch::Ignore);
    assert_eq!(c(".#Main.qml", false), Touch::Ignore);
    assert_eq!(c("Main.qml~", false), Touch::Ignore);
    assert_eq!(c("target", true), Touch::Ignore);
    assert_eq!(c("target/debug/Main.qml", false), Touch::Ignore);
    assert_eq!(c(".git/index", false), Touch::Ignore);
    assert_eq!(
        classify(&root, &root.with_file_name("Other.qml"), false),
        Touch::Ignore
    );
    // The root itself.
    assert_eq!(classify(&root, &root, true), Touch::Rescan);
}

#[test]
fn an_error_right_after_a_change_is_told_by_the_build_after_it() {
    let mut r = started(&[("Main", MAIN)]);
    let broken = "import QtQuick\nItem {\n  width: \n";
    r.save("Main", Some(broken.to_owned()));
    assert_eq!(r.build(), Build::Unsettled);
    let Build::Failed(faults) = r.build() else {
        panic!("expected the error once the files still say the same");
    };
    assert_eq!(faults[0].file, "Main.qml");
    assert_eq!(r.build(), Build::NoChange);

    // Another broken text after it is a new change, held back once too.
    r.save("Main", Some(broken.replace("width", "height")));
    assert_eq!(r.build(), Build::Unsettled);
    r.save("Main", Some(broken.replace("width", "x")));
    assert!(matches!(r.build(), Build::Failed(_)), "held back once only");
}

#[test]
fn a_file_written_in_parts_is_read_again_after_its_first_part_failed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let main = root.join("Main.qml");
    fs::write(&main, MAIN).unwrap();
    let mut r = Reloader::open(&root, QtTypeOracle).unwrap();
    r.start();

    let whole = MAIN.replace("10", "20");
    fs::write(&main, &whole[..whole.len() / 2]).unwrap();
    assert!(r.closed(&main));
    assert_eq!(r.build(), Build::Unsettled);
    // The rest is written, and its event is late or lost: the build after
    // the quiet time reads the file again.
    fs::write(&main, &whole).unwrap();
    assert!(matches!(r.build(), Build::Patch { .. }));
}

#[test]
fn a_reload_asked_for_tells_an_error_at_once() {
    let mut r = started(&[("Main", MAIN)]);
    r.save("Main", Some("import QtQuick\nItem {".to_owned()));
    assert!(matches!(r.build_now(), Build::Failed(_)));
}
