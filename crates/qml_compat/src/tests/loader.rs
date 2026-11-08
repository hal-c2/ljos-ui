//! Tests of loading: `Loader`, components, QML text made at run time.

use super::*;

#[test]
fn qml_text_made_while_the_file_runs_keeps_it() {
    assert!(
        kept_for("import QtQuick\nItem { function f() { Qt.createQmlObject(\"import QtQuick; Rectangle {}\", this) } }\n")
            .contains("QML text")
    );
}

fn page(name: &str) -> Option<Root> {
    (name == "Page").then(|| root_of("Rectangle", &[]))
}

#[test]
fn a_loader_of_a_file_beside_it_names_the_file_as_a_type() {
    let source = "import QtQuick\nItem {\n  Loader { source: \"Page.qml\" }\n  Loader { source: \"./Page.qml\" }\n  Loader { id: l; sourceComponent: c; onSourceComponentChanged: l.sourceComponent }\n  Component { id: c; Item { } }\n}\n";
    let Outcome::Lowered(done) = lower(source, &page) else {
        panic!("not lowered");
    };
    assert_eq!(
        done.text,
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core\nItem {\n  Loader { component: Page }\n  Loader { component: Page }\n  Loader { id: l; component: c; onComponentChanged: l.component }\n  Component { id: c; Item { } }\n}\n"
    );
}

#[test]
fn a_loader_whose_source_is_not_a_file_beside_it_keeps_the_file() {
    let kept = |source: &str| match lower(source, &page) {
        Outcome::Kept(kept) => kept.reason,
        other => panic!("lowered: {other:?}"),
    };
    let made = "a `Loader`'s `source` is made as it runs, so what it loads is not known";
    for (source, why) in [
        ("Loader { source: \"Pa\" + \"ge.qml\" }", made.to_string()),
        (
            "Loader { source: Qt.resolvedUrl(\"Page.qml\") }",
            made.to_string(),
        ),
        (
            "Loader { source: \"pages/Page.qml\" }",
            "a `Loader`'s `source` is \"pages/Page.qml\", which is not a file beside this one"
                .to_string(),
        ),
        (
            "Loader { source: \"qrc:/Page.qml\" }",
            "a `Loader`'s `source` is \"qrc:/Page.qml\", which is not a file beside this one"
                .to_string(),
        ),
        (
            "Loader { source: \"Other.qml\" }",
            "a `Loader`'s `source` is \"Other.qml\", which is not a file beside this one"
                .to_string(),
        ),
        (
            "Item { Loader { id: l; source: \"Page.qml\" } Component.onCompleted: l.setSource(\"Page.qml\", {}) }",
            "a `Loader`'s `source` is read or set as well as written".to_string(),
        ),
        (
            "Loader { source: \"Page.qml\"; onSourceChanged: {} }",
            "a `Loader`'s `source` is read or set as well as written".to_string(),
        ),
    ] {
        assert_eq!(
            kept(&format!("import QtQuick\n{source}\n")),
            why,
            "{source}"
        );
    }
    // A file beside it whose name is no type's: written as it is, it would
    // be an id, a subtraction or a member read.
    let any = |_: &str| Some(root_of("Rectangle", &[]));
    for file in ["page.qml", "my-page.qml", "Page.ui.qml"] {
        let source = format!("import QtQuick\nLoader {{ source: \"{file}\" }}\n");
        let Outcome::Kept(kept) = lower(&source, &any) else {
            panic!("{file} lowered");
        };
        assert_eq!(
            kept.reason,
            format!("a `Loader`'s `source` is \"{file}\", whose name is not a type's"),
        );
    }
}

/// What reads or sets a Loader's `source` other than its own binding would
/// be told it is empty once the Loader is given a component.
#[test]
fn a_loader_whose_source_is_read_where_the_pass_does_not_follow_keeps_the_file() {
    let read = "a `Loader`'s `source` is read or set as well as written";
    for source in [
        // Its own handler, unqualified.
        "Loader { source: \"Page.qml\"; onStatusChanged: console.warn(source) }",
        "Loader { source: \"Page.qml\"; Component.onCompleted: setSource(\"Page.qml\", {}) }",
        "Loader { source: \"Page.qml\"; function again() { setSource(\"Page.qml\") } }",
        // Another object aimed at it.
        "Item { Loader { id: l; source: \"Page.qml\" } Connections { target: l; function onSourceChanged() { } } }",
        "Item { Loader { id: l; source: \"Page.qml\" } Connections { target: l; onSourceChanged: { } } }",
        "Item { id: root; Loader { id: l; source: \"Page.qml\" } states: State { name: \"o\"; PropertyChanges { target: l; source: \"Page.qml\" } } }",
        "Item { id: root; Loader { id: l; source: \"Page.qml\" } states: State { name: \"o\"; PropertyChanges { l.source: \"Page.qml\" } } }",
        // A Loader that is the root: an unqualified `source` anywhere may be its.
        "Loader { source: \"Page.qml\"; Item { width: source.length } }",
        // So is the root of an inline component.
        "Item { component P: Loader { source: \"Page.qml\"; Item { width: source.length } } }",
        // Its change signal, connected in a script.
        "Item { Loader { id: l; source: \"Page.qml\" } Component.onCompleted: l.sourceChanged.connect(f); function f() { } }",
        "Loader { source: \"Page.qml\"; Component.onCompleted: sourceChanged.connect(f); function f() { } }",
    ] {
        match lower(&format!("import QtQuick\n{source}\n"), &page) {
            Outcome::Kept(kept) => assert_eq!(kept.reason, read, "{source}"),
            other => panic!("{source}: {other:?}"),
        }
    }
    // A child's own `source` is its own.
    let Outcome::Lowered(done) = lower(
        "import QtQuick\nItem {\n  Loader { source: \"Page.qml\"; Item { property string source; width: source.length } }\n}\n",
        &page,
    ) else {
        panic!("not lowered");
    };
    assert!(
        done.text.contains("Loader { component: Page;"),
        "{}",
        done.text
    );
    // A parameter or a local a script binds is the script's, as it is for
    // every other name; the root Loader's `source` is not read.
    for script in [
        "function f(source) { return source.length }",
        "function f() { var source = \"x\"; return source }",
        "onStatusChanged: { let source = 1; console.log(source) }",
        "property var g: (source) => source",
    ] {
        let source = format!("import QtQuick\nLoader {{ source: \"Page.qml\"; {script} }}\n");
        let done = match lower(&source, &page) {
            Outcome::Lowered(done) => done,
            other => panic!("{script}: {other:?}"),
        };
        assert!(
            done.text.contains("Loader { component: Page;"),
            "{}",
            done.text
        );
    }
    // An inline component's root Loader is not the file root's scope.
    let Outcome::Lowered(done) = lower(
        "import QtQuick\nItem {\n  property string source\n  width: source.length\n  component P: Loader { source: \"Page.qml\" }\n}\n",
        &page,
    ) else {
        panic!("not lowered");
    };
    assert!(
        done.text.contains("Loader { component: Page }"),
        "{}",
        done.text
    );
}

/// Qt's `sourceComponent` stays empty when a `source` is given; lowered,
/// it is the component, so whatever reads it or hears it change is told
/// otherwise.
#[test]
fn a_loader_whose_component_is_read_keeps_the_file() {
    let read = "a `Loader`'s `sourceComponent` is read or heard, and lowered it is not empty";
    for source in [
        "Loader { property int n; source: \"Page.qml\"; onSourceComponentChanged: n++ }",
        "Loader { source: \"Page.qml\"; onLoaded: console.log(sourceComponent) }",
        "Item { Loader { id: l; source: \"Page.qml\" } property bool b: l.sourceComponent !== null }",
        "Item { Loader { id: l; source: \"Page.qml\" } Connections { target: l; function onSourceComponentChanged() { } } }",
        "Item { Loader { id: l; source: \"Page.qml\" } Component.onCompleted: l.sourceComponentChanged.connect(f); function f() { } }",
        "Loader { source: \"Page.qml\"; Component.onCompleted: sourceComponentChanged.connect(f); function f() { } }",
    ] {
        match lower(&format!("import QtQuick\n{source}\n"), &page) {
            Outcome::Kept(kept) => assert_eq!(kept.reason, read, "{source}"),
            other => panic!("{source}: {other:?}"),
        }
    }
}

#[test]
fn a_loader_given_both_a_source_and_a_component_keeps_the_file() {
    match lower(
        "import QtQuick\nItem {\n  Loader { source: \"Page.qml\"; sourceComponent: c }\n  Component { id: c; Item { } }\n}\n",
        &page,
    ) {
        Outcome::Kept(kept) => assert_eq!(
            kept.reason,
            "a `Loader` is given both a `source` and a `sourceComponent`"
        ),
        other => panic!("{other:?}"),
    }
}
