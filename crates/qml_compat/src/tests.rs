use super::*;

fn root_of(type_name: &str, bindings: &[&str]) -> Root {
    Root {
        type_name: type_name.to_string(),
        bindings: bindings.iter().map(ToString::to_string).collect(),
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
    for (old, new) in TYPE_RENAMES {
        assert!(ui.type_named(new).is_some(), "{new}");
        assert!(ui.type_named(old).is_none(), "{old}");
    }
    for (ty, renames) in MEMBER_RENAMES {
        let ty = ui.type_named(ty).expect("a renaming type");
        for (old, new) in *renames {
            assert!(ty.property(new).is_some(), "{}.{new}", ty.name);
            assert!(ty.property(old).is_none(), "{}.{old}", ty.name);
            assert!(
                ty.signal(&format!("{new}Changed")).is_some(),
                "{new}Changed"
            );
        }
    }
}

#[test]
fn a_file_without_qtquick_is_unchanged() {
    assert_eq!(
        lower("import QtQml\nQtObject { }\n", &no_locals),
        Outcome::Unchanged
    );
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
    // Nothing else moved.
    for (a, b) in source.lines().zip(text.lines()) {
        if a != b {
            assert!(
                a.contains("import QtQuick") || a.contains("color:") || a.contains("MouseArea"),
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

#[test]
fn a_script_reads_the_new_name_off_an_id() {
    let text = lowered(
        "import QtQuick\nItem {\n    Rectangle { id: r }\n    Text { id: t }\n    \
         Component.onCompleted: { t.color = r.color; r.colorChanged.connect(f) }\n    \
         function f() {}\n}\n",
    );
    assert!(
        text.contains("t.color = r.fill; r.fillChanged.connect(f)"),
        "{text}"
    );
}

#[test]
fn an_unqualified_member_is_the_scope_objects() {
    let text = lowered(
        "import QtQuick\nRectangle {\n    width: 10\n    onColorChanged: console.log(color)\n    \
         Item { height: color.r }\n    Text { width: color.r }\n}\n",
    );
    assert!(text.contains("onFillChanged: console.log(fill)"), "{text}");
    // An Item has no colour: it is the root's.
    assert!(text.contains("Item { height: fill.r }"), "{text}");
    assert!(text.contains("Text { width: color.r }"), "{text}");
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

#[test]
fn a_file_of_the_project_is_the_type_its_root_is() {
    let locals = |name: &str| (name == "Swatch").then(|| root_of("Rectangle", &[]));
    let Outcome::Lowered(lowered) = lower(
        "import QtQuick\nItem { Swatch { id: s; color: \"red\" } Text { color: s.color } }\n",
        &locals,
    ) else {
        panic!("not lowered");
    };
    assert!(
        lowered
            .text
            .contains("Swatch { id: s; fill: \"red\" } Text { color: s.fill }"),
        "{}",
        lowered.text
    );
}

#[test]
fn a_file_names_the_files_of_the_project_it_may_use() {
    let refs = references(
        "import QtQuick\nimport \"parts\"\nimport \"lib.js\" as Lib\nItem {\n    property Swatch s\n    \
         Button { }\n    Loader { source: \"panels/Side.qml\" }\n    \
         function f() { return Qt.createComponent(\"Pop.qml\") }\n}\n",
    );
    assert_eq!(refs.dirs, ["parts"]);
    assert_eq!(refs.files, ["Pop.qml", "panels/Side.qml"]);
    for ty in ["Item", "Swatch", "Button", "Loader", "Qt"] {
        assert!(refs.types.iter().any(|t| t == ty), "{ty}: {:?}", refs.types);
    }
}

fn alone(source: &str) -> Alone {
    let locals = |name: &str| (name == "Button").then(|| root_of("Rectangle", &["color"]));
    Alone {
        outcome: lower(source, &locals),
        references: references(source),
    }
}

/// A kept file that sets `color` on a lowered `Button` would find a `fill`.
#[test]
fn files_that_name_one_another_are_lowered_together() {
    let button = "import QtQuick\nRectangle { color: \"red\" }\n";
    let files = std::collections::BTreeMap::from([
        ("app/Button".to_string(), alone(button)),
        (
            "app/Main".to_string(),
            // Kept: `Flow` is not in Ljos.Ui.
            alone("import QtQuick\nFlow { Button { color: \"blue\" } }\n"),
        ),
        ("other/Lone".to_string(), alone(button)),
    ]);
    let outcomes = group(&files);
    let Outcome::Kept(kept) = &outcomes["app/Button"] else {
        panic!("{:?}", outcomes["app/Button"]);
    };
    assert!(
        kept.reason.contains("`Main.qml`, which is kept"),
        "{}",
        kept.reason
    );
    assert!(matches!(outcomes["app/Main"], Outcome::Kept(_)));
    assert!(matches!(outcomes["other/Lone"], Outcome::Lowered(_)));
}

#[test]
fn a_file_reached_by_address_or_directory_is_of_the_group() {
    let lowers = "import QtQuick\nRectangle { color: \"red\" }\n";
    let kept = "import QtQuick\nImage { }\n";
    for (main, other) in [
        (
            "import QtQuick\nLoader { source: \"../parts/Part.qml\" }\n",
            "parts/Part",
        ),
        (
            "import QtQuick\nimport \"../parts\"\nItem { }\n",
            "parts/Part",
        ),
    ] {
        let files = std::collections::BTreeMap::from([
            ("app/Main".to_string(), alone(main)),
            (other.to_string(), alone(kept)),
            ("app/Swatch".to_string(), alone(lowers)),
        ]);
        let outcomes = group(&files);
        assert!(matches!(outcomes["app/Main"], Outcome::Kept(_)), "{main}");
        // Named by nothing: alone.
        assert!(matches!(outcomes["app/Swatch"], Outcome::Lowered(_)));
    }
}

/// `import MyControls` keeps the importer; the module's `MyButton`, lowered
/// alone, would have a `fill` where the importer sets `color`.
#[test]
fn a_module_of_the_project_is_of_the_group_of_the_files_that_import_it() {
    let button = "import QtQuick\nRectangle { }\n";
    let main = "import QtQuick\nimport MyControls\nItem { MyButton { color: \"red\" } }\n";
    for module in ["app/MyControls/MyButton", "imports/MyControls/MyButton"] {
        let files = std::collections::BTreeMap::from([
            ("app/Main".to_string(), alone(main)),
            (module.to_string(), alone(button)),
            ("app/Lone".to_string(), alone(button)),
        ]);
        let outcomes = group(&files);
        assert!(matches!(outcomes["app/Main"], Outcome::Kept(_)));
        assert!(
            matches!(outcomes[module], Outcome::Kept(_)),
            "{module}: {:?}",
            outcomes[module]
        );
        assert!(matches!(outcomes["app/Lone"], Outcome::Lowered(_)));
    }
    // A dotted name is a directory per part.
    let dotted = "import QtQuick\nimport My.Controls\nItem { MyButton { color: \"red\" } }\n";
    let files = std::collections::BTreeMap::from([
        ("app/Main".to_string(), alone(dotted)),
        ("lib/My/Controls/MyButton".to_string(), alone(button)),
        ("lib/Controls/Other".to_string(), alone(button)),
    ]);
    let outcomes = group(&files);
    assert!(matches!(
        outcomes["lib/My/Controls/MyButton"],
        Outcome::Kept(_)
    ));
    assert!(matches!(
        outcomes["lib/Controls/Other"],
        Outcome::Lowered(_)
    ));
}

#[test]
fn qml_text_made_while_the_file_runs_keeps_it() {
    assert!(
        kept_for("import QtQuick\nItem { function f() { Qt.createQmlObject(\"import QtQuick; Rectangle {}\", this) } }\n")
            .contains("QML text")
    );
}

const LAYOUTS: &str = "import QtQuick\nimport QtQuick.Layouts\n";

/// What a `RowLayout` does by default is written out: its spacing, and a
/// child centred across the row.
#[test]
fn a_row_layout_is_a_row_with_its_defaults_written_out() {
    let text = lowered(&format!(
        "{LAYOUTS}RowLayout {{\n    width: 200\n    Rectangle {{ implicitWidth: 20; implicitHeight: 10 }}\n    \
         Rectangle {{ Layout.fillWidth: true; Layout.alignment: Qt.AlignTop }}\n    \
         Item {{ Layout.preferredWidth: 30; Layout.alignment: Qt.AlignRight }}\n}}\n"
    ));
    assert_eq!(
        text,
        "pragma Scripts: JavaScript; import Ljos.Ui\n\nRow { spacing: 5;\n    width: 200\n    \
         Rectangle { Layout.alignment: Qt.AlignVCenter; implicitWidth: 20; implicitHeight: 10 }\n    \
         Rectangle { Layout.fillWidth: true; Layout.alignment: Qt.AlignTop }\n    \
         Item { Layout.preferredWidth: 30; Layout.alignment: Qt.AlignRight | Qt.AlignVCenter }\n}\n"
    );
}

#[test]
fn a_column_layout_counts_an_empty_child_and_keeps_its_spacing() {
    // Not the root: no file that uses this one can size it.
    let text = lowered(&format!(
        "{LAYOUTS}Item {{ ColumnLayout {{ spacing: 2; Item {{ width: 10 }} Rectangle {{ width: 10; height: 4 }} }} }}\n"
    ));
    assert!(
        text.ends_with(
            "Item { Column { spacing: 2; Item { Layout.fillWidth: false; width: 10 } Rectangle { width: 10; height: 4 } } }\n"
        ),
        "{text}"
    );
}

/// Each way a line layout and Ljós's arrangement would place a child
/// differently keeps the file, and says which.
#[test]
fn where_a_layout_and_the_arrangement_disagree_the_file_is_kept() {
    for (body, why) in [
        (
            "RowLayout { Text { text: \"a\"; width: 80 } }",
            "`width` of a `Text` in a `RowLayout`",
        ),
        (
            "RowLayout { Item { implicitWidth: 8; width: 80 } }",
            "`width` of a `Item`",
        ),
        (
            "RowLayout { Text { text: \"a\" } }",
            "implicit width of a `Text` may be a fraction",
        ),
        (
            "RowLayout { Item { implicitWidth: 2.5 } }",
            "`implicitWidth` is not a whole number",
        ),
        (
            "RowLayout { width: 100; Item { implicitWidth: 8 } }",
            "spreads the room",
        ),
        (
            "ColumnLayout { anchors.fill: parent; Item { implicitHeight: 8 } }",
            "give a child `Layout.fillHeight: true`",
        ),
        (
            "ColumnLayout { RowLayout { } }",
            "a `RowLayout` in a `ColumnLayout` fills it",
        ),
        (
            "RowLayout { Item { anchors.top: parent.top } }",
            "`anchors.top` on a child",
        ),
        ("RowLayout { uniformCellSizes: true }", "`uniformCellSizes`"),
        (
            "RowLayout { Item { Layout.alignment: Qt.AlignBaseline } }",
            "`Qt.AlignBaseline`",
        ),
        (
            "RowLayout { Item { Layout.alignment: a } }",
            "not one the pass can read",
        ),
        ("RowLayout { Swatch { } }", "a file of the project"),
        ("GridLayout { }", "`GridLayout` is not in Ljos.Ui"),
        ("StackLayout { }", "`StackLayout` is not in Ljos.Ui"),
    ] {
        let reason = kept_for(&format!("{LAYOUTS}{body}\n"));
        assert!(reason.contains(why), "{body}: {reason}");
    }
}

#[test]
fn a_positioner_is_lowered_as_it_is_written() {
    let source =
        "import QtQuick\nColumn { spacing: 4; Rectangle { width: 6; height: 4 } Swatch { } }\n";
    let locals = |name: &str| (name == "Swatch").then(|| root_of("Rectangle", &["color", "width"]));
    let Outcome::Lowered(lowered) = lower(source, &locals) else {
        panic!("not lowered");
    };
    assert!(
        lowered
            .text
            .ends_with("Column { spacing: 4; Rectangle { width: 6; height: 4 } Swatch { } }\n")
    );
}

/// A child that is a file of the project is placed by what its root binds
/// too: Qt's `Row` ignores a `Layout` and a `y` there, Ljós's would not.
#[test]
fn a_positioner_child_is_checked_by_its_files_root() {
    let locals = |name: &str| match name {
        "Swatch" => Some(root_of("Item", &["Layout.fillWidth", "width"])),
        "Tile" => Some(root_of("Item", &["y"])),
        "Chip" => Some(root_of("Tile", &["color"])),
        "Pin" => Some(root_of("Item", &["anchors.centerIn"])),
        _ => None,
    };
    for (body, child, why) in [
        (
            "Row { Swatch { } }",
            "Swatch",
            "`Swatch`'s root sets `Layout.fillWidth`, which is ignored by Qt's `Row` and would \
             be honoured by Ljós's",
        ),
        ("Row { Tile { } }", "Tile", "`Tile`'s root sets `y`"),
        ("Row { Chip { } }", "Chip", "`Chip`'s root sets `y`"),
        ("Row { Pin { } }", "Pin", "`anchors.centerIn` on a child"),
        (
            "Item { component Mark: Item { y: 2 } Row { Mark { } } }",
            "Mark { }",
            "`Mark`'s root sets `y`",
        ),
    ] {
        let source = format!("import QtQuick\n{body}\n");
        let Outcome::Kept(kept) = lower(&source, &locals) else {
            panic!("{body}: lowered");
        };
        assert!(kept.reason.contains(why), "{body}: {}", kept.reason);
        assert_eq!(
            kept.at,
            source.rfind(child).unwrap(),
            "{body}: at the child"
        );
    }
}

/// A layout at the root of a file, or of an inline component, may be sized
/// by a file that uses it: with nothing to fill the room, Qt spreads it.
#[test]
fn a_root_layout_with_nothing_to_fill_it_is_kept() {
    for body in [
        "RowLayout { Item { implicitWidth: 8 } }",
        "Item { component Bar: ColumnLayout { Item { implicitHeight: 8 } } }",
        "Item { property Item bar: RowLayout { Item { implicitWidth: 8 } } }",
        "Item { Component { RowLayout { Item { implicitWidth: 8 } } } }",
    ] {
        let reason = kept_for(&format!("{LAYOUTS}{body}\n"));
        assert!(reason.contains("spreads the room"), "{body}: {reason}");
        assert!(
            reason.contains("a file that uses it may size it"),
            "{body}: {reason}"
        );
    }
    lowered(&format!(
        "{LAYOUTS}RowLayout {{ Item {{ implicitWidth: 8; Layout.fillWidth: true }} }}\n"
    ));
}

#[test]
fn an_alignment_the_pass_cannot_read_is_reported_at_its_binding() {
    let source = format!(
        "{LAYOUTS}Item {{ RowLayout {{ Item {{ Layout.alignment: {{ return Qt.AlignTop }} }} }} }}\n"
    );
    let Outcome::Kept(kept) = lower(&source, &no_locals) else {
        panic!("lowered");
    };
    assert_eq!(
        kept.at,
        source.find("Layout.alignment").unwrap(),
        "{}",
        kept.reason
    );
}

#[test]
fn where_a_positioner_and_the_arrangement_disagree_the_file_is_kept() {
    for (body, why) in [
        (
            "Row { Item { Layout.fillWidth: true } }",
            "`Layout.fillWidth` is ignored by Qt's `Row` and would be honoured by Ljós's",
        ),
        ("Row { Item { y: 4 } }", "leaves a child's `y` to it"),
        ("Column { Item { x: 4 } }", "leaves a child's `x` to it"),
        (
            "Row { Item { anchors.fill: parent } }",
            "`anchors.fill` on a child",
        ),
        (
            "Row { add: Transition { } }",
            "`add` is a member of Qt's `Row`",
        ),
        (
            "Column { Text { text: Positioner.index } }",
            "it uses `Positioner`",
        ),
        ("Row { Repeater { model: 2; Item { y: 1 } } }", "`y`"),
    ] {
        let reason = kept_for(&format!("import QtQuick\n{body}\n"));
        assert!(reason.contains(why), "{body}: {reason}");
    }
}

#[test]
fn a_text_s_members_take_ljos_s_names() {
    let text = lowered(
        "import QtQuick\nText {\n    id: t\n    horizontalAlignment: Text.AlignRight; wrapMode: Text.WordWrap\n    textFormat: Text.RichText; elide: Text.ElideRight\n    onWrapModeChanged: console.log(t.wrapMode, textFormat)\n}\n",
    );
    assert_eq!(
        text,
        "pragma Scripts: JavaScript; import Ljos.Ui\nText {\n    id: t\n    align: Text.AlignRight; wrap: Text.WordWrap\n    format: Text.RichText; elide: Text.ElideRight\n    onWrapChanged: console.log(t.wrap, format)\n}\n"
    );
}

#[test]
fn a_font_s_pixel_size_is_its_size_wherever_it_is_named() {
    let text = lowered(
        "import QtQuick\nItem {\n    Text { id: t; font.pixelSize: 12; font { pixelSize: 14 } }\n    states: State { PropertyChanges { target: t; font.pixelSize: 20 } }\n    State { PropertyChanges { t.font.pixelSize: 30 } }\n    Component.onCompleted: t.font.pixelSize = t.font.pixelSize + 1\n}\n",
    );
    assert_eq!(text.matches("pixelSize").count(), 0, "{text}");
    assert_eq!(text.matches("size").count(), 6, "{text}");
}

#[test]
fn a_bold_font_is_a_weight() {
    let text = lowered(
        "import QtQuick\nItem {\n    Text { id: a; font.bold: true }\n    Text { font.bold: false }\n    Text { font.bold: a.wrapMode === Text.Wrap }\n}\n",
    );
    assert!(
        text.contains("Text { id: a; font.weight: Font.Bold }"),
        "{text}"
    );
    assert!(text.contains("Text { font.weight: Font.Normal }"), "{text}");
    assert!(
        text.contains("Text { font.weight: (a.wrap === Text.Wrap) ? Font.Bold : Font.Normal }"),
        "{text}"
    );
}

#[test]
fn a_font_the_names_cannot_say_keeps_the_file() {
    for (source, why) in [
        (
            "import QtQuick\nText { id: t; text: t.font.bold ? \"b\" : \"\" }\n",
            "`.bold` of a font",
        ),
        (
            "import QtQuick\nText { font.bold: true; font.weight: 300 }\n",
            "both set",
        ),
        (
            "import QtQuick\nText { font { bold: true; weight: 300 } }\n",
            "both set",
        ),
        ("import QtQuick\nText { font.pointSize: 9 }\n", "`Screen`"),
        (
            "import QtQuick\nText { Component.onCompleted: font.pointSize = 9 }\n",
            "`Screen`",
        ),
        (
            "import QtQuick\nText { font.bold: { return true } }\n",
            "a block",
        ),
        (
            "import QtQuick\nItem { property var f; Text { font.pixelSize: f.pixelSize } }\n",
            "not known",
        ),
        (
            "import QtQuick\nItem { Text { id: t } Component.onCompleted: t.font[\"pixelSize\"] = 3 }\n",
            "renames",
        ),
    ] {
        let reason = kept_for(source);
        assert!(reason.contains(why), "{source}: {reason}");
    }
}

#[test]
fn a_member_named_as_a_font_s_off_a_font_is_kept_as_it_is() {
    // `bold` and `pixelSize` mean a font's only on a font.
    let text = lowered(
        "import QtQuick\nItem {\n    property bool bold: true\n    Rectangle { id: r; border.width: bold ? 2 : 1 }\n    Text { text: r.bold }\n}\n",
    );
    assert!(text.contains("border.width: bold ? 2 : 1"), "{text}");
}
