//! Runs the real `ljos i18n extract` on a small QML tree, as a build script
//! or a CI job does.

use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn extract(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ljos"))
        .current_dir(dir)
        .args(["i18n", "extract", "ui", "-o", "i18n/app_is.ts"])
        .args(args)
        .output()
        .expect("spawn ljos")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn tree(dir: &Path) {
    fs::create_dir_all(dir.join("ui/parts")).unwrap();
    fs::create_dir_all(dir.join("i18n")).unwrap();
    fs::write(
        dir.join("ui/Main.qml"),
        r#"import QtQuick
import "parts/format.js" as Format
Item {
    //: The window title
    property string title: qsTr("Files")
    property string open: qsTr("Open", "verb")
    property string count: qsTr("%n file(s)", "", 3)
    //% "Application"
    property string name: qsTrId("app-name")
}
"#,
    )
    .unwrap();
    fs::write(
        dir.join("ui/parts/Row.qml"),
        "import QtQuick\nText { text: qsTranslate(\"Main\", \"Open\", \"verb\") + qsTr(\"Remove\") }\n",
    )
    .unwrap();
    fs::write(
        dir.join("ui/parts/format.js"),
        ".pragma library\nfunction size(n) { return qsTr(\"%n byte(s)\", \"\", n) }\n",
    )
    .unwrap();
    // Not imported by any QML file: not part of the program.
    fs::write(dir.join("ui/parts/unused.js"), "var s = qsTr(\"Unused\")\n").unwrap();
}

#[test]
fn a_new_catalogue_lists_every_text_of_the_tree() {
    let dir = tempfile::tempdir().unwrap();
    tree(dir.path());

    let output = extract(dir.path(), &["--target-language", "is_IS"]);
    assert!(output.status.success(), "{output:?}");
    let said = stdout(&output);
    assert!(
        said.contains("Searched 2 QML and 1 JavaScript file(s)"),
        "{said}"
    );
    assert!(
        said.contains("Found 6 source text(s) (6 new and 0 already existing)"),
        "{said}"
    );

    let ts = fs::read_to_string(dir.path().join("i18n/app_is.ts")).unwrap();
    assert!(
        ts.contains(r#"<TS version="2.1" language="is_IS">"#),
        "{ts}"
    );
    // Paths are as seen from the catalogue, which is how Qt Linguist finds
    // the sources.
    assert!(
        ts.contains(r#"<location filename="../ui/Main.qml" line="5"/>"#),
        "{ts}"
    );
    assert!(
        ts.contains("<extracomment>The window title</extracomment>"),
        "{ts}"
    );
    assert!(ts.contains("<comment>verb</comment>"), "{ts}");
    assert!(ts.contains(r#"<message id="app-name">"#), "{ts}");
    assert!(ts.contains("<source>Application</source>"), "{ts}");
    // One message, used in two files.
    assert_eq!(ts.matches("<source>Open</source>").count(), 1, "{ts}");
    assert!(
        ts.contains(r#"<location filename="../ui/parts/Row.qml" line="2"/>"#),
        "{ts}"
    );
    // Icelandic has two plural forms.
    assert_eq!(ts.matches("<numerusform></numerusform>").count(), 4, "{ts}");
    assert!(ts.contains("<name>format</name>"), "{ts}");
    assert!(!ts.contains("Unused"), "{ts}");
}

/// Build output holds copies of the sources, which are not more texts to
/// translate: the directories `ljos check` and `ljos fmt` leave out are left
/// out here too.
#[test]
fn build_output_in_the_tree_is_not_searched() {
    let dir = tempfile::tempdir().unwrap();
    tree(dir.path());
    for copy in ["build", "target", "build-desktop", ".rcc"] {
        fs::create_dir_all(dir.path().join("ui").join(copy)).unwrap();
        fs::write(
            dir.path().join("ui").join(copy).join("Main.qml"),
            "import QtQuick\nItem { property string t: qsTr(\"Copied\") }\n",
        )
        .unwrap();
    }

    let output = extract(dir.path(), &["--target-language", "is_IS"]);
    assert!(output.status.success(), "{output:?}");
    let said = stdout(&output);
    assert!(
        said.contains("Searched 2 QML and 1 JavaScript file(s)"),
        "{said}"
    );
    let ts = fs::read_to_string(dir.path().join("i18n/app_is.ts")).unwrap();
    assert!(!ts.contains("Copied"), "{ts}");
}

#[test]
fn an_update_keeps_what_translators_did() {
    let dir = tempfile::tempdir().unwrap();
    tree(dir.path());
    let path = dir.path().join("i18n/app_is.ts");
    assert!(
        extract(dir.path(), &["--target-language", "is_IS"])
            .status
            .success()
    );

    // The translator finishes two texts.
    let ts = fs::read_to_string(&path).unwrap();
    let ts = ts
        .replace(
            "<source>Files</source>\n        <extracomment>The window title</extracomment>\n        <translation type=\"unfinished\"></translation>",
            "<source>Files</source>\n        <extracomment>The window title</extracomment>\n        <translation>Skrár</translation>",
        )
        .replace(
            "<source>Remove</source>\n        <translation type=\"unfinished\"></translation>",
            "<source>Remove</source>\n        <translation>Fjarlægja</translation>",
        );
    assert!(ts.contains("Skrár") && ts.contains("Fjarlægja"), "{ts}");
    fs::write(&path, ts).unwrap();

    // The program loses "Remove" and "Open", and gains "Close".
    fs::write(
        dir.path().join("ui/parts/Row.qml"),
        "import QtQuick\nText { text: qsTr(\"Close\") }\n",
    )
    .unwrap();
    let main = fs::read_to_string(dir.path().join("ui/Main.qml")).unwrap();
    fs::write(
        dir.path().join("ui/Main.qml"),
        main.replace("    property string open: qsTr(\"Open\", \"verb\")\n", ""),
    )
    .unwrap();

    let output = extract(dir.path(), &[]);
    assert!(output.status.success(), "{output:?}");
    let said = stdout(&output);
    assert!(
        said.contains("Found 5 source text(s) (1 new and 4 already existing)"),
        "{said}"
    );
    assert!(said.contains("Kept 1 translated text(s)"), "{said}");
    assert!(said.contains("Removed 1 untranslated text(s)"), "{said}");

    let ts = fs::read_to_string(&path).unwrap();
    assert!(ts.contains("<translation>Skrár</translation>"), "{ts}");
    assert!(
        ts.contains("<translation type=\"vanished\">Fjarlægja</translation>"),
        "{ts}"
    );
    assert!(!ts.contains("<source>Open</source>"), "{ts}");
    assert!(ts.contains("<source>Close</source>"), "{ts}");
    assert!(ts.contains(r#"language="is_IS""#), "{ts}");
}

#[test]
fn check_fails_when_the_catalogue_is_behind_the_sources_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    tree(dir.path());
    let path = dir.path().join("i18n/app_is.ts");

    // No catalogue yet.
    let output = extract(dir.path(), &["--check"]);
    assert!(!output.status.success());
    assert!(!path.exists());

    assert!(extract(dir.path(), &[]).status.success());
    assert!(extract(dir.path(), &["--check"]).status.success());

    // Lines move: nothing for a translator to do.
    let main = fs::read_to_string(dir.path().join("ui/Main.qml")).unwrap();
    fs::write(
        dir.path().join("ui/Main.qml"),
        main.replace("Item {\n", "Item {\n\n    id: root\n"),
    )
    .unwrap();
    assert!(extract(dir.path(), &["--check"]).status.success());

    // A new text.
    let before = fs::read_to_string(&path).unwrap();
    fs::write(
        dir.path().join("ui/parts/Row.qml"),
        "import QtQuick\nText { text: qsTr(\"Open\", \"verb\") + qsTr(\"Remove\") + qsTr(\"New\") }\n",
    )
    .unwrap();
    let output = extract(dir.path(), &["--check"]);
    assert!(!output.status.success());
    let complaint = String::from_utf8_lossy(&output.stderr);
    assert!(complaint.contains("out of date"), "{complaint}");
    assert_eq!(fs::read_to_string(&path).unwrap(), before);
}

#[test]
fn what_a_translator_will_not_get_is_said() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("ui")).unwrap();
    fs::create_dir_all(dir.path().join("i18n")).unwrap();
    fs::write(
        dir.path().join("ui/Main.qml"),
        "import QtQuick\nItem {\n    property string a: qsTr(`${width} wide`)\n}\n",
    )
    .unwrap();
    let output = extract(dir.path(), &[]);
    assert!(output.status.success());
    let notes = String::from_utf8_lossy(&output.stderr);
    assert!(
        notes.contains("Main.qml:3: qsTr() template strings with arguments are not supported"),
        "{notes}"
    );
    assert!(stdout(&output).contains("Found 0 source text(s)"));
}
