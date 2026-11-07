//! Tests of the core visual types: `Item`, `Rectangle`, member renames.

use super::*;

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
