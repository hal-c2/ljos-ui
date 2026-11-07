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
        "pragma Scripts: JavaScript; import Ljos.Ui\nItem {\n  Loader { component: Page }\n  Loader { component: Page }\n  Loader { id: l; component: c; onComponentChanged: l.component }\n  Component { id: c; Item { } }\n}\n"
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
}
