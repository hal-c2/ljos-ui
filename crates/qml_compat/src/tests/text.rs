//! Tests of text: `Text`, `TextInput`, `TextEdit`, fonts.

use super::*;

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
        (
            "import QtQuick\nText { font.bold: true; font { weight: 300 } }\n",
            "both set",
        ),
        (
            "import QtQuick\nText { font { bold: true } font.weight: 300 }\n",
            "both set",
        ),
        (
            "import QtQuick\nItem { Item { id: someItem } Text { text: someItem.font.bold } }\n",
            "not known",
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
            "import QtQuick\nItem {\n    property alias face: label.font\n    Text { id: label }\n    Component.onCompleted: face.pixelSize = 40\n}\n",
            "not known",
        ),
        (
            "import QtQuick\nItem { Text { id: t } Component.onCompleted: t.font[\"pixelSize\"] = 3 }\n",
            "renames",
        ),
        (
            "import QtQuick\nItem {\n    id: root\n    property var face: label.font\n    Text { id: label }\n    Text { text: root.face.bold }\n}\n",
            "not known",
        ),
        (
            "import QtQuick\nItem {\n    Text { id: label; property alias face: label.font }\n    Text { text: label.face.pixelSize }\n}\n",
            "not known",
        ),
    ] {
        let reason = kept_for(source);
        assert!(reason.contains(why), "{source}: {reason}");
    }
}

#[test]
fn a_font_s_member_name_off_what_is_no_font_is_left_as_written() {
    // A row's role or a script's object may be called `bold`; only a
    // `font`, or what the file declares as a font or a `var`, may be one.
    let text = lowered(
        "import QtQuick\nItem {\n    function shown(row) { return row.pointSize }\n    ListView {\n        model: ListModel { ListElement { bold: \"a\"; pixelSize: 2 } }\n        delegate: Text { text: model.bold + model.pixelSize }\n    }\n    Component.onCompleted: console.log(({ bold: true }).bold)\n}\n",
    );
    assert!(
        text.contains("text: modelData.bold + modelData.pixelSize"),
        "{text}"
    );
    assert!(text.contains("({ bold: true }).bold"), "{text}");
    assert!(text.contains("return row.pointSize"), "{text}");
}

#[test]
fn a_font_s_member_name_off_a_parameter_or_a_local_of_a_font_s_name_is_left_as_written() {
    // What a function is given or a local holds is not followed, even
    // where its name is `font` or a property the file declares as a `var`.
    let text = lowered(
        "import QtQuick\nItem {\n    property var face\n    function size(font) { return font.pixelSize }\n    function bold() { let face = { bold: true }; return face.bold }\n}\n",
    );
    assert!(text.contains("return font.pixelSize"), "{text}");
    assert!(text.contains("return face.bold"), "{text}");
}

#[test]
fn a_member_named_as_a_font_s_off_a_font_is_kept_as_it_is() {
    // `bold` and `pixelSize` mean a font's only on a font.
    let text = lowered(
        "import QtQuick\nItem {\n    property bool bold: true\n    Rectangle { id: r; border.width: bold ? 2 : 1 }\n    Text { text: r.bold }\n}\n",
    );
    assert!(text.contains("border.width: bold ? 2 : 1"), "{text}");
}
