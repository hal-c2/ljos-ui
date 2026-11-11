use super::*;

mod controls;
mod core;
mod dialogs;
mod gestures;
mod groups;
mod layout;
mod loader;
mod models;
mod natives;
mod shapes;
mod states;
mod text;
mod tools;
mod types;
mod ui;
mod window;

fn root_of(type_name: &str, bindings: &[&str]) -> Root {
    Root {
        type_name: type_name.to_string(),
        bindings: bindings.iter().map(ToString::to_string).collect(),
        ..Root::default()
    }
}

fn lowered(source: &str) -> String {
    match lower(source, &no_locals) {
        Outcome::Lowered(lowered) => lowered.text,
        other => panic!("not lowered: {other:?}"),
    }
}

fn kept_for(source: &str) -> String {
    match lower(source, &no_locals) {
        Outcome::Kept(kept) => kept.to_string(),
        other => panic!("lowered: {other:?}"),
    }
}

/// Every rename in the table says what `Ljos.Ui` has: the new name is
/// there and the old one is not, so the table and the runtime cannot drift.
#[test]
fn the_renames_are_what_ljos_ui_has() {
    let ui = qt_types::module(MODULE).expect("Ljos.Ui is described");
    for (old, new) in type_renames() {
        assert!(ui.type_named(new).is_some(), "{new}");
        // An old name is gone, unless another type now has it: Qt's
        // `DragHandler` is `Drag`, and Qt's `Drag` `DragSource`.
        assert!(
            ui.type_named(old).is_none() || type_renames().any(|(_, n)| n == old),
            "{old}"
        );
    }
    // Each type Ljos.Ui has of a Qt type that renames members, by the name
    // it has there; every entry of the table is some such type's.
    let quick = qt_types::module("QtQuick").expect("QtQuick is described");
    let names = member_renames().map(|(name, _)| (name, name));
    let mut seen = Vec::new();
    for (qt, name) in type_renames().copied().chain(names) {
        let (Some(qt), Some(ty)) = (quick.type_named(qt), ui.type_named(name)) else {
            continue;
        };
        let Some(renames) = renames_of(qt) else {
            continue;
        };
        // The entry that renames for it: its own, or its nearest ancestor's.
        let mut at = Some(qt);
        while let Some(ty) = at {
            if member_renames().any(|(name, _)| name == ty.name) {
                seen.push(ty.name);
                break;
            }
            at = ty.prototype();
        }
        for (old, new) in renames {
            assert!(ty.property(new).is_some(), "{}.{new}", ty.name);
            assert!(ty.property(old).is_none(), "{}.{old}", ty.name);
            assert!(
                ty.signal(&format!("{new}Changed")).is_some(),
                "{new}Changed"
            );
        }
    }
    for (name, _) in member_renames() {
        assert!(seen.contains(&name), "no type of Ljos.Ui is a {name}");
    }
}

#[test]
fn a_file_without_qt_imports_is_unchanged() {
    assert_eq!(lower("Item { }\n", &no_locals), Outcome::Unchanged);
}

/// The corpus is a submodule, not this repository's (CLEAN_ROOM.md): the
/// file is read where it is checked out, and the test says so where it is not.
fn corpus_file(path: &str) -> Option<String> {
    let at = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corpus/qtdoc")
        .join(path);
    let text = std::fs::read_to_string(&at).ok();
    if text.is_none() {
        eprintln!(
            "SKIPPED: no corpus at {} (`git submodule update --init corpus/qtdoc`)",
            at.display()
        );
    }
    text
}

/// The corpus button sizes its text in points, which has no Ljós name until
/// `Screen` is in `Ljos.Ui`; the file is kept, and says where.
#[test]
fn the_corpus_button_is_kept_for_its_point_size() {
    let Some(source) = corpus_file("examples/demos/hangman/qml/SimpleButton.qml") else {
        return;
    };
    assert_eq!(
        kept_for(&source),
        format!("`font.pointSize` has no Ljós name until `Screen` is in {MODULE}")
    );
}

#[test]
fn var_is_unknown() {
    let text =
        lowered("import QtQuick\nItem {\n    property var model\n    property variant old: 1\n}\n");
    assert!(text.contains("property unknown model"));
    assert!(text.contains("property unknown old: 1"));
}

/// `Behavior on color` would be `on fill`, once `Ljos.Ui` has `Behavior`.
#[test]
fn a_type_outside_the_slice_keeps_the_file_whatever_it_is_on() {
    assert!(
        kept_for("import QtQuick\nRectangle {\n    Behavior on color { }\n}\n")
            .contains("`Behavior` is not in Ljos.Ui yet")
    );
}

#[test]
fn what_the_pass_cannot_type_keeps_the_file() {
    assert!(kept_for("import QtQuick\nItem { width: parent.color.r }\n").contains("`.color`"));
    assert!(
        kept_for("import QtQuick\nItem { PathView { } }\n")
            .contains("`PathView` is not in Ljos.Ui")
    );
    assert!(
        kept_for("import QtQuick\nItem { MyThing { color: \"red\" } }\n")
            .contains("whose type is not known")
    );
    assert!(
        kept_for("import QtQuick\nimport QtQuick.Controls\nItem { }\n")
            .contains("QtQuick.Controls")
    );
    assert!(kept_for("import QtQuick as Q\nQ.Item { }\n").contains("under a name"));
    assert!(
        kept_for("import QtQuick\nRectangle { function f() { let color = 1; return color } }\n")
            .contains("a local is called `color`")
    );
}

fn alone(source: &str) -> Alone {
    let locals = |name: &str| (name == "Button").then(|| root_of("Rectangle", &["color"]));
    Alone {
        outcome: lower(source, &locals),
        references: references(source),
        uses: Vec::new(),
    }
}

/// What the pass makes of `source` in a program that starts with the
/// controls style `style`.
fn in_style(source: &str, style: &str) -> Outcome {
    lower(
        source,
        &Styled {
            locals: &no_locals,
            style,
            natives: None,
        },
    )
}

// What a kept file's cause is, as a value: a caller can tell one reason
// from another without reading its text.

fn cause_of(source: &str) -> Cause {
    match lower(source, &no_locals) {
        Outcome::Kept(kept) => kept.cause,
        other => panic!("lowered: {other:?}"),
    }
}

#[test]
fn a_file_is_kept_for_its_imports() {
    assert_eq!(cause_of("import QtQuick\nItem {\n"), Cause::Unparsed);
    assert_eq!(
        cause_of("import QtQuick as Q\nQ.Item { }\n"),
        Cause::ImportedUnderName {
            uri: "QtQuick".into()
        }
    );
    assert_eq!(
        cause_of("import QtQuick\nimport QtQuick.Controls.Material\nButton { }\n"),
        Cause::ModuleNotLowered {
            uri: "QtQuick.Controls.Material".into()
        }
    );
    assert_eq!(
        cause_of("import QtQuick\nimport QtQuick.Controls\nItem { }\n"),
        Cause::ControlsStyle { style: None }
    );
}

#[test]
fn a_file_is_kept_for_a_type_its_module_lacks() {
    assert_eq!(
        cause_of("import QtQuick\nItem { PathView { } }\n"),
        Cause::NotInModule {
            name: "PathView".into(),
            module: MODULE
        }
    );
    assert_eq!(
        cause_of("import QtQuick\nimport QtQuick.Controls.Basic\nItem { Dial { } }\n"),
        Cause::NotInModule {
            name: "Dial".into(),
            module: CONTROLS
        }
    );
}

#[test]
fn a_file_is_kept_for_what_its_scripts_read() {
    assert_eq!(
        cause_of("import QtQuick\nItem { function f() { Qt.createQmlObject(\"\", this) } }\n"),
        Cause::QmlText
    );
    assert_eq!(
        cause_of("import QtQuick\nRectangle { function f() { let color = 1; return color } }\n"),
        Cause::LocalCalledRenamed {
            name: "color".into()
        }
    );
    assert_eq!(
        cause_of("import QtQuick\nItem { width: parent.color.r }\n"),
        Cause::ReadOffUnknown {
            name: "color".into()
        }
    );
}

#[test]
fn a_file_is_kept_for_its_fonts_loaders_rows_and_layouts() {
    assert_eq!(
        cause_of("import QtQuick\nText { font.pointSize: 12 }\n"),
        Cause::Font(Font::PointSize)
    );
    assert_eq!(
        cause_of("import QtQuick\nLoader { source: \"Pa\" + \"ge.qml\" }\n"),
        Cause::Loader(Loader::SourceMade)
    );
    assert_eq!(
        cause_of(
            "import QtQuick\nItem {\n  ListModel { id: zones; ListElement { city: \"a\" } }\n  \
             Repeater { model: zones; Text { text: { var model = 1; return model.city } } }\n}\n"
        ),
        Cause::Row(Row::LocalModel)
    );
    assert_eq!(
        cause_of("import QtQuick\nColumn { Rectangle { Layout.fillWidth: true } }\n"),
        Cause::Layout(Layout::Ignored {
            name: "Layout.fillWidth".into(),
            root: None,
            qt: "Column"
        })
    );
}

#[test]
fn a_file_of_a_kept_group_says_which_file_keeps_it() {
    let files = std::collections::BTreeMap::from([
        (
            "app/Button".to_string(),
            alone("import QtQuick\nRectangle { color: \"red\" }\n"),
        ),
        (
            "app/Main".to_string(),
            alone("import QtQuick\nPathView { Button { color: \"blue\" } }\n"),
        ),
    ]);
    let outcomes = group(&files);
    let Outcome::Kept(kept) = &outcomes["app/Button"] else {
        panic!("{:?}", outcomes["app/Button"]);
    };
    assert_eq!(
        kept.cause,
        Cause::Group {
            with: "Main".into(),
            cause: Box::new(Cause::NotInModule {
                name: "PathView".into(),
                module: MODULE
            })
        }
    );
}

#[test]
fn a_file_is_kept_for_a_name_the_lowered_file_finds_first() {
    // Nothing the file imports, nor the project, has the name: Qt finds it
    // in the context, and the lowered file in one of Ljós's modules.
    assert_eq!(
        cause_of("import QtQuick\nItem { property var d: PointerArea }\n"),
        Cause::FoundFirst {
            name: "PointerArea".into(),
            is: None,
            module: MODULE
        }
    );
    let boxed = |name: &str| (name == "Box").then(|| root_of("Rectangle", &[]));
    match lower("import QtQuick\nItem { Box { } }\n", &boxed) {
        Outcome::Kept(kept) => assert_eq!(
            kept.cause,
            Cause::FoundFirst {
                name: "Box".into(),
                is: Some(Owner::Project),
                module: LAYOUT
            }
        ),
        other => panic!("lowered: {other:?}"),
    }
}
