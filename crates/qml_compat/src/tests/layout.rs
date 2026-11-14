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
         Rectangle { Layout.alignment: Align.VCenter; implicitWidth: 20; implicitHeight: 10 }\n    \
         Rectangle { Layout.fillWidth: true; Layout.alignment: Align.Top }\n    \
         Item { Layout.preferredWidth: 30; Layout.alignment: Align.Right | Align.VCenter }\n}\n"
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

/// A size from a child's content is rounded up by Qt's layouts and by
/// Ljós's box once a child has a `Layout`: a `ColumnLayout`'s child is
/// given one where none says anything through it.
#[test]
fn a_size_from_the_content_is_rounded_by_both() {
    for (body, out) in [
        (
            "Item { RowLayout { Text { text: \"a\" } Item { implicitWidth: 2.5; implicitHeight: 3 } } }",
            "Item { Row { spacing: 5; Text { Layout.alignment: Align.VCenter; text: \"a\" } \
             Item { Layout.alignment: Align.VCenter; implicitWidth: 2.5; implicitHeight: 3 } } }\n",
        ),
        (
            "Item { ColumnLayout { Text { text: \"a\" } Rectangle { width: 4; height: 4 } } }",
            "Item { Column { spacing: 5; Text { Layout.fillWidth: false; text: \"a\" } \
             Rectangle { width: 4; height: 4 } } }\n",
        ),
    ] {
        let text = lowered(&format!("{LAYOUTS}{body}\n"));
        assert!(text.ends_with(out), "{body}: {text}");
    }
}

/// A `RowLayout` or `ColumnLayout` in a layout grows where a child of it
/// fills, and the pass says so; Ljós's box holds it to its children's
/// minimum, as Qt's does.
#[test]
fn a_layout_in_a_layout_is_told_to_fill_where_it_grows() {
    for (body, out) in [
        (
            "Item { ColumnLayout { RowLayout { Item { implicitWidth: 4; implicitHeight: 4; Layout.fillWidth: true } } } }",
            "Item { Column { spacing: 5; Row { Layout.fillWidth: true; spacing: 5; \
             Item { Layout.alignment: Align.VCenter; implicitWidth: 4; implicitHeight: 4; Layout.fillWidth: true } } } }\n",
        ),
        (
            "Item { RowLayout { ColumnLayout { Item { implicitWidth: 4; Layout.fillHeight: true } } } }",
            "Item { Row { spacing: 5; Column { Layout.fillHeight: true; Layout.alignment: Align.VCenter; spacing: 5; \
             Item { implicitWidth: 4; Layout.fillHeight: true } } } }\n",
        ),
        (
            "Item { ColumnLayout { RowLayout { Item { implicitWidth: 4; implicitHeight: 4 } } } }",
            "Item { Column { spacing: 5; Row { spacing: 5; \
             Item { Layout.alignment: Align.VCenter; implicitWidth: 4; implicitHeight: 4 } } } }\n",
        ),
        // A hidden child that fills grows nothing.
        (
            "Item { ColumnLayout { RowLayout { Item { visible: false; implicitWidth: 4; Layout.fillWidth: true } } } }",
            "Item { Column { spacing: 5; Row { spacing: 5; \
             Item { Layout.alignment: Align.VCenter; visible: false; implicitWidth: 4; Layout.fillWidth: true } } } }\n",
        ),
    ] {
        let text = lowered(&format!("{LAYOUTS}{body}\n"));
        assert!(text.ends_with(out), "{body}: {text}");
    }
}

/// A `GridLayout` is a `Grid` with Qt's gaps written out, and each child
/// centred in its row; a name of it in a script is `Grid`'s.
#[test]
fn a_grid_layout_is_a_grid_with_its_defaults_written_out() {
    let text = lowered(&format!(
        "{LAYOUTS}Item {{ GridLayout {{ columns: 2; rowSpacing: 1; flow: GridLayout.LeftToRight\n    \
         Rectangle {{ implicitWidth: 20; implicitHeight: 10 }}\n    \
         Item {{ Layout.preferredWidth: 30; Layout.alignment: Qt.AlignRight }} }} }}\n"
    ));
    assert!(
        text.ends_with(
            "Item { Grid { columnSpacing: 5; columns: 2; rowSpacing: 1; flow: Grid.LeftToRight\n    \
             Rectangle { Layout.alignment: Align.VCenter; implicitWidth: 20; implicitHeight: 10 }\n    \
             Item { Layout.preferredWidth: 30; Layout.alignment: Align.Right | Align.VCenter } } }\n"
        ),
        "{text}"
    );
}

/// A top-to-bottom `GridLayout` counts its rows, the count along its flow,
/// as Ljós's `Grid` does; its `flow` names `Grid`'s enumeration.
#[test]
fn a_top_to_bottom_grid_layout_is_a_grid_of_its_rows() {
    let text = lowered(&format!(
        "{LAYOUTS}Item {{ GridLayout {{ flow: GridLayout.TopToBottom; rows: 2\n    \
         Rectangle {{ implicitWidth: 20; implicitHeight: 10 }}\n    \
         Rectangle {{ implicitWidth: 30; implicitHeight: 20 }}\n    \
         Rectangle {{ implicitWidth: 10; implicitHeight: 10 }} }} }}\n"
    ));
    assert!(
        text.ends_with(
            "Item { Grid { columnSpacing: 5; rowSpacing: 5; flow: Grid.TopToBottom; rows: 2\n    \
             Rectangle { Layout.alignment: Align.VCenter; implicitWidth: 20; implicitHeight: 10 }\n    \
             Rectangle { Layout.alignment: Align.VCenter; implicitWidth: 30; implicitHeight: 20 }\n    \
             Rectangle { Layout.alignment: Align.VCenter; implicitWidth: 10; implicitHeight: 10 } } }\n"
        ),
        "{text}"
    );
}

/// A `StackLayout` is `Pages`, with what it says as it says it.
#[test]
fn a_stack_layout_is_pages() {
    let text = lowered(&format!(
        "{LAYOUTS}Item {{ StackLayout {{ currentIndex: 1; Item {{ }} Item {{ }} }} }}\n"
    ));
    assert!(
        text.ends_with("Item { Pages { currentIndex: 1; Item { } Item { } } }\n"),
        "{text}"
    );
}

/// What a `GridLayout` or `StackLayout` says that the pass cannot yet
/// carry over keeps the file, and says which.
#[test]
fn where_a_grid_layout_or_stack_layout_disagrees_the_file_is_kept() {
    for (body, why) in [
        (
            "GridLayout { rows: 2 }",
            "`rows` of a `GridLayout` is ignored",
        ),
        (
            "GridLayout { flow: GridLayout.TopToBottom; columns: 2 }",
            "`columns` of a `GridLayout` is ignored along a top-to-bottom flow",
        ),
        ("GridLayout { flow: f }", "`flow`"),
        (
            "GridLayout { layoutDirection: Qt.RightToLeft }",
            "`layoutDirection`",
        ),
        (
            "GridLayout { uniformCellWidths: true }",
            "`uniformCellWidths`",
        ),
        (
            "GridLayout { Item { Layout.columnSpan: 2 } }",
            "`Layout.columnSpan` puts a child",
        ),
        (
            "Item { GridLayout { width: 100; Item { implicitWidth: 10 } } }",
            "spreads the room no child fills",
        ),
        (
            "RowLayout { Item { Layout.fillWidth: true } StackLayout { } }",
            "grows only where a child of it fills and is bounded by its children",
        ),
        (
            "StackLayout { Item { visible: StackLayout.isCurrentItem } }",
            "`StackLayout`'s attached properties are not in Ljos.Layout yet",
        ),
        (
            "StackLayout { Item { StackLayout.onIsCurrentItemChanged: {} } }",
            "`StackLayout`'s attached properties",
        ),
        (
            "GridLayout { Item { Layout.alignment: a } }",
            "`GridLayout` centres a child that says nothing of how it is aligned across the \
             row, where Ljós's `Grid` puts it at the top",
        ),
        (
            "GridLayout { Item { Layout.alignment: Qt.AlignBaseline } }",
            "`Qt.AlignBaseline` is not in Ljós's `Grid` yet",
        ),
    ] {
        let reason = kept_for(&format!("{LAYOUTS}{body}\n"));
        assert!(reason.contains(why), "{body}: {reason}");
    }
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
            "RowLayout { Item { Layout.preferredWidth: 2.5 } }",
            "`Layout.preferredWidth` is not a whole number",
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
            "ColumnLayout { Item { Layout.fillHeight: true } RowLayout { width: 50 } }",
            "a `RowLayout` in a `ColumnLayout` grows only where a child of it fills",
        ),
        (
            "Item { ColumnLayout { RowLayout { Layout.fillWidth: true; Item { } } } }",
            "a `RowLayout` in a `ColumnLayout` grows only where",
        ),
        (
            "Item { property bool on; ColumnLayout { RowLayout { Item { visible: on; Layout.fillWidth: true } } } }",
            "a `RowLayout` in a `ColumnLayout` grows only where",
        ),
        (
            "Item { ColumnLayout { RowLayout { Item { Layout.fillWidth: true; Layout.maximumWidth: 9 } } } }",
            "a `RowLayout` in a `ColumnLayout` grows only where",
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
        assert!(kept.to_string().contains(why), "{body}: {}", kept);
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
        kept
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
            "Row { Item { anchors.verticalCenter: parent.verticalCenter } }",
            "`anchors.verticalCenter` places a child across Qt's `Row`, which honours it",
        ),
        (
            "Column { Item { anchors.right: parent.right } }",
            "places it across with `Layout.alignment`",
        ),
        // Along the row an anchor is overridden, as `anchors.fill` is.
        (
            "Row { Item { anchors.left: parent.left } }",
            "`anchors.left` on a child",
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

/// A count of 0 is no count to Qt's `Grid` (qml6), so a `columns: 0` is
/// its four columns, written once.
#[test]
fn a_grid_of_no_columns_is_given_qt_s_four_once() {
    for (body, want) in [
        (
            format!("Grid {{ columns: 0; {TILE} }}"),
            format!("Grid {{ columns: 4; {TILE} }}"),
        ),
        (
            format!("Grid {{ rows: 0; columns: 0; {TILE} }}"),
            format!("Grid {{ rows: 0; columns: 4; {TILE} }}"),
        ),
        (
            format!("Grid {{ rows: 0; {TILE} }}"),
            format!("Grid {{ columns: 4; rows: 0; {TILE} }}"),
        ),
    ] {
        let text = lowered(&format!("import QtQuick\n{body}\n"));
        assert!(text.ends_with(&format!("\n{want}\n")), "{body}: {text}");
    }
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
            "`add` keeps a `Flow` as Qt's until the Box rules reach a wrapping line",
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

/// A directory `parts` with a `GridLayout` of its own, built on the type
/// given.
struct OwnGrid(&'static str);

impl Locals for OwnGrid {
    fn root(&self, _: &str) -> Option<Root> {
        None
    }

    fn imported(&self, from: Imported<'_>, name: &str) -> Option<Root> {
        matches!((from, name), (Imported::Dir("parts"), "GridLayout")).then(|| root_of(self.0, &[]))
    }
}

/// Imported after QtQuick.Layouts, a `GridLayout` is the import's, in a
/// script as on an object: neither is Ljos.Layout's `Grid`.
#[test]
fn a_grid_layout_of_an_import_after_qt_is_the_imports_in_a_script_too() {
    let source = "import QtQuick\nimport QtQuick.Layouts\nimport \"parts\"\n\
                  Item { GridLayout { } property var f: GridLayout.TopToBottom }\n";
    match lower(source, &OwnGrid("Item")) {
        Outcome::Lowered(lowered) => {
            assert!(lowered.text.contains("GridLayout { }"), "{}", lowered.text);
            assert!(
                lowered.text.contains("f: GridLayout.TopToBottom"),
                "{}",
                lowered.text
            );
        }
        other => panic!("not lowered: {other:?}"),
    }
}

/// A `GridLayout` of an import after QtQuick.Layouts built on a type
/// Ljos.Ui has not yet keeps the file, though only a script names it:
/// `GridLayout.TopToBottom` is not `Grid`'s there either.
#[test]
fn a_grid_layout_of_an_import_built_on_what_is_not_lowered_keeps_the_file() {
    let source = "import QtQuick\nimport QtQuick.Layouts\nimport \"parts\"\n\
                  Item { property var f: GridLayout.TopToBottom }\n";
    match lower(source, &OwnGrid("FocusScope")) {
        Outcome::Kept(kept) => {
            assert!(kept.to_string().contains("FocusScope"), "{}", kept);
        }
        other => panic!("not kept: {other:?}"),
    }
}
