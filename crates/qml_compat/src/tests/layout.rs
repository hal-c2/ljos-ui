//! Tests of layout: `RowLayout`, `ColumnLayout` and the positioners.

use super::*;

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
        "pragma Scripts: JavaScript; import Ljos.Ui\nimport Ljos.Layout\nRow { spacing: 5;\n    width: 200\n    \
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
        ("GridLayout { }", "`GridLayout` is not in Ljos.Layout"),
        ("StackLayout { }", "`StackLayout` is not in Ljos.Layout"),
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

/// A file that arranges children with Qt's positioners imports
/// `Ljos.Layout` on the line it imports `Ljos.Ui`; one that imported
/// QtQuick.Layouts imports it on that line.
#[test]
fn a_file_that_arranges_imports_ljos_layout() {
    let text = lowered("import QtQuick\nRow { Rectangle { width: 4; height: 4 } }\n");
    assert!(
        text.starts_with("pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Layout\nRow"),
        "{text}"
    );
    let text = lowered("import QtQuick\nRectangle { color: \"red\" }\n");
    assert!(!text.contains("Ljos.Layout"), "{text}");
    let text = lowered(&format!("{LAYOUTS}Item {{ }}\n"));
    assert!(
        text.starts_with("pragma Scripts: JavaScript; import Ljos.Ui\nimport Ljos.Layout\nItem"),
        "{text}"
    );
}

const TILE: &str = "Rectangle { width: 10; height: 10 }";

#[test]
fn a_grid_and_a_flow_are_lowered_as_they_are_written() {
    for body in [
        format!("Grid {{ columns: 2; spacing: 4; {TILE} {TILE} {TILE} }}"),
        // No spacing: a line it does not fill counts none.
        format!("Grid {{ columns: 3; {TILE} {TILE} }}"),
        format!("Grid {{ rows: 2; flow: Grid.TopToBottom; spacing: 3; {TILE} {TILE} {TILE} }}"),
        format!("Flow {{ width: 30; spacing: 2; {TILE} {TILE} {TILE} }}"),
    ] {
        let text = lowered(&format!("import QtQuick\n{body}\n"));
        assert!(text.ends_with(&format!("\n{body}\n")), "{body}: {text}");
    }
    let text = lowered(&format!("import QtQuick\nGrid {{ {TILE} }}\n"));
    assert!(
        text.ends_with(&format!("\nGrid {{ columns: 4; {TILE} }}\n")),
        "{text}"
    );
}

/// Each way Qt's `Grid` counts room Ljós's does not keeps the file, and
/// says which.
#[test]
fn where_a_grid_positioner_and_ljos_grid_disagree_the_file_is_kept() {
    for (body, why) in [
        (
            format!("Grid {{ columns: 3; spacing: 5; {TILE} {TILE} }}"),
            "spacing after a row of children it does not fill",
        ),
        (
            format!("Grid {{ spacing: 1; {TILE} }}"),
            "spacing after a row of children it does not fill",
        ),
        (
            format!("Grid {{ rows: 4; rowSpacing: 2; {TILE} {TILE} {TILE} {TILE} {TILE} }}"),
            "it has 4 rows and 5 children to fill them, 2 to a row",
        ),
        (
            format!("Grid {{ rows: 1; columns: 1; {TILE} {TILE} }}"),
            "a child beyond its rows and columns",
        ),
        (
            format!("Grid {{ columns: 2; spacing: 2; Repeater {{ model: 3; {TILE} }} }}"),
            "it has a `Repeater`",
        ),
        (
            format!("Grid {{ columns: 2; spacing: 2; Item {{ }} {TILE} }}"),
            "for as many children as it may show",
        ),
        (
            format!(
                "Grid {{ columns: 2; spacing: 2; {TILE} Rectangle {{ visible: on; width: 1; height: 1 }} }}"
            ),
            "for as many children as it may show",
        ),
        (
            "Grid { columns: n }".to_string(),
            "`columns` of a `Grid` is not a whole number",
        ),
        (
            "Grid { flow: f }".to_string(),
            "`flow` of a `Grid` is not one the pass can read",
        ),
        (
            "Grid { horizontalItemAlignment: Grid.AlignHCenter }".to_string(),
            "`horizontalItemAlignment` is not in Ljós's `Grid` yet",
        ),
        (
            "Grid { Rectangle { anchors.centerIn: parent } }".to_string(),
            "`anchors.centerIn` on a child of a `Grid`",
        ),
        (
            "Flow { add: Transition { } }".to_string(),
            "`add` is a member of Qt's `Flow`",
        ),
    ] {
        let reason = kept_for(&format!("import QtQuick\n{body}\n"));
        assert!(reason.contains(why), "{body}: {reason}");
    }
    // A hidden child and one with no size are left out by both.
    lowered(&format!(
        "import QtQuick\nGrid {{ columns: 2; spacing: 2; {TILE} {TILE} Rectangle {{ visible: false }} Item {{ width: 0 }} }}\n"
    ));
}
