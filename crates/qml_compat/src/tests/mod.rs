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
        Outcome::Kept(kept) => kept.reason,
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
    let names = member_renames().map(|(name, _)| (*name, *name));
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
            if member_renames().any(|(name, _)| *name == ty.name) {
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
        assert!(seen.contains(name), "no type of Ljos.Ui is a {name}");
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

#[test]
fn the_corpus_button_is_lowered_line_for_line() {
    let Some(source) = corpus_file("examples/demos/hangman/qml/SimpleButton.qml") else {
        return;
    };
    let source = source.as_str();
    let text = lowered(source);
    assert_eq!(text.lines().count(), source.lines().count());
    assert!(text.contains("pragma Scripts: JavaScript; import Ljos.Ui\n"));
    assert!(!text.contains("import QtQuick"));
    // The rectangle's colour is its fill, in the binding and in the states;
    // the text's colour and the border's are colours still.
    assert!(text.contains("        fill: buttonColor\n"));
    assert!(text.contains("            color: textColor\n"));
    assert_eq!(text.matches("fill: button.buttonColor").count(), 1);
    assert_eq!(text.matches("fill: \"transparent\"").count(), 1);
    assert_eq!(text.matches("border.color:").count(), 2);
    assert_eq!(text.matches("color: button.textColor").count(), 1);
    assert!(text.contains("    PointerArea {\n"));
    assert!(!text.contains("MouseArea"));
    // The states name their objects in each change, not as a target.
    assert!(!text.contains("target:"));
    assert_eq!(text.matches("buttonRect.fill: ").count(), 2);
    assert_eq!(text.matches("buttonRect.border.color: ").count(), 2);
    assert_eq!(text.matches("buttonText.color: ").count(), 2);
    // Nothing else moved; a target's line is left empty.
    for (a, b) in source.lines().zip(text.lines()) {
        if a != b {
            assert!(
                a.contains("import QtQuick")
                    || a.contains("color:")
                    || a.contains("MouseArea")
                    || (a.contains("target:") && b.is_empty()),
                "{a:?} became {b:?}"
            );
        }
    }
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
    assert!(kept_for("import QtQuick\nItem { Image { } }\n").contains("`Image` is not in Ljos.Ui"));
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
