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
fn a_font_s_points_are_a_size_by_the_points_of_its_screen() {
    let text = lowered(
        "import QtQuick\nItem {\n    Text { font.pointSize: 12 }\n    Text { font { pointSize: big ? 18 : 9; bold: true } }\n}\n",
    );
    assert!(text.contains("font.size: Screen.points(12)"), "{text}");
    assert!(text.contains("size: Screen.points(big ? 18 : 9)"), "{text}");
    assert_eq!(text.matches("pointSize").count(), 0, "{text}");
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
        (
            "import QtQuick\nText { font.pointSize: 9; font.pixelSize: 12 }\n",
            "both set",
        ),
        (
            "import QtQuick\nText { font.pointSize: { return 9 } }\n",
            "a block",
        ),
        (
            "import QtQuick\nText { Component.onCompleted: font.pointSize = 9 }\n",
            "`.pointSize` of a font",
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

#[test]
fn qs_tr_is_a_tagged_template() {
    let text = lowered(
        "import QtQuick\nItem {\n    property int count: 3\n    property string name: \"Ada\"\n    property string open: qsTr(\"Open\")\n    property string of: qsTr(\"%1 of %2\").arg(name).arg(count)\n    property string back: qsTr(\"%2 of %1\").arg(name).arg(count)\n    property string files: qsTr(\"%n file(s)\", \"\", count)\n    property string grouped: qsTr(\"%Ln file(s)\", \"\", count)\n}\n",
    );
    assert_eq!(
        text,
        "pragma Scripts: JavaScript; import Ljos.Ui; import Ljos.Core\nItem {\n    property int count: 3\n    property string name: \"Ada\"\n    property string open: t`Open`\n    property string of: t`${name} of ${count}`\n    property string back: t`${count} of ${name}`\n    property string files: plural(count, { other: `${count} file(s)` })\n    property string grouped: plural(count, { other: \"# file(s)\" })\n}\n"
    );
}

#[test]
fn contexts_and_ids_keep_what_they_say() {
    let text = lowered(
        "import QtQuick\nItem {\n    property string a: qsTr(\"Open\", \"menu\")\n    property string b: qsTranslate(\"Dialog\", \"Close\")\n    property string c: qsTrId(\"nav.home\")\n    property string g: qsTr(\"%Ln file(s) in %1\", \"disk\", 2).arg(a)\n}\n",
    );
    for line in [
        "a: t({ message: \"Open\", context: \"menu\" })",
        "b: t({ message: \"Close\", context: \"Dialog\" })",
        "c: t({ id: \"nav.home\" })",
        "g: t({ message: plural(2, { other: `# file(s) in ${a}` }), context: \"disk\" })",
    ] {
        assert!(text.contains(line), "{line}\n{text}");
    }
}

#[test]
fn a_message_s_values_keep_their_own_lowering_and_lines() {
    // A `Rectangle`'s `color` is its `fill`, inside a value too;
    // a call over lines keeps its lines.
    let text = lowered(
        "import QtQuick\nRectangle {\n    id: r; Text {\n        text: qsTr(\"%1 and `${x}`\\\\\")\n            .arg(r.color)\n    }\n}\n",
    );
    assert!(
        text.contains("text: t`${\nr.fill} and \\`\\${x}\\`\\\\`"),
        "{text}"
    );
    assert_eq!(text.lines().count(), 7, "{text}");
}

#[test]
fn a_value_written_in_another_place_keeps_its_own_lowering() {
    // `.arg` fills the lowest-numbered `%N` left, so `r.color` moves
    // after `b`, and is `r.fill` there too; `%n` says the count again.
    let text = lowered(
        "import QtQuick\nRectangle {\n    id: r\n    property int c: 2\n    property string b: \"x\"\n    property string s: qsTr(\"%2 of %1, %1\").arg(r.color).arg(b)\n    property string n: qsTr(\"%1: %n\", \"\", r.c).arg(r.color)\n}\n",
    );
    for line in [
        "s: t`${b} of ${r.fill}, ${r.fill}`",
        "n: plural(r.c, { other: `${r.fill}: ${r.c}` })",
    ] {
        assert!(text.contains(line), "{line}\n{text}");
    }
    assert_eq!(text.lines().count(), 8, "{text}");
}

#[test]
fn a_count_qt_s_types_say_is_an_int_is_rewritten() {
    let text = lowered(
        "import QtQuick\nText {\n    id: label\n    property string a: qsTr(\"%n line(s)\", \"\", lineCount)\n    property string b: qsTr(\"%n\", \"\", label.lineCount)\n    property string c: qsTr(\"%n\", \"\", a.length)\n}\n",
    );
    for line in [
        "a: plural(lineCount, { other: `${lineCount} line(s)` })",
        "b: plural(label.lineCount, { other: `${label.lineCount}` })",
        "c: plural(a.length, { other: `${a.length}` })",
    ] {
        assert!(text.contains(line), "{line}\n{text}");
    }
}

#[test]
fn a_message_that_cannot_be_written_out_keeps_the_file() {
    for (source, why) in [
        ("property string key: \"Open\"; property string shown: qsTr(key)", "`qsTr` of a value that is not written out"),
        ("property string shown: qsTr(\"%1 %2\").arg(1, 2)", "not of one value"),
        ("property string shown: qsTr(\"%1\").arg(1).arg(2)", "no `%N` left"),
        ("property string shown: qsTr(\"%L1\").arg(1)", "a `%LN`"),
        ("property string shown: qsTr(\"%2 %1\").arg(parent\n        .width).arg(2)", "spans lines"),
        ("property string shown: qsTr(\"%2 %1\").arg(1 + 1).arg(2)", "not a name, a member of one or a literal"),
        ("property string shown: qsTr(\"%1 %1\").arg(Date.now())", "not a name, a member of one or a literal"),
        ("property string shown: qsTr(\"%n\", \"\", 2.5)", "not known to be an `int`"),
        ("property string shown: qsTr(\"%n\", \"\", width)", "not known to be an `int`"),
        ("property real n: 2; property string shown: qsTr(\"%Ln\", \"\", n)", "not known to be an `int`"),
        ("property string later: QT_TR_NOOP(\"Later\")", "a string to Qt"),
        ("property var later: QT_TRANSLATE_NOOP(\"C\", \"x\")", "a string to Qt"),
        ("property var later: QT_TRID_NOOP(\"id\")", "a string to Qt"),
        ("property string shown: qsTrId(\"a\").arg(1)", "`t({ id })`"),
        ("property string shown: qsTranslate(\"C\", \"x\", \"d\")", "one context"),
        ("property var f: qsTr", "read as a value"),
        ("property string shown: qsTr(\"#%n\", \"\", 2)", "`plural` reads"),
        ("property string t: \"x\"; property string shown: qsTr(\"x\")", "`t` is another name"),
        ("function plural(n) { return n } property string shown: qsTr(\"%n\", \"\", 1)", "`plural` is another name"),
    ] {
        let why_kept = kept_for(&format!("import QtQuick\nItem {{\n    {source}\n}}\n"));
        assert!(why_kept.contains(why), "{source}: {why_kept}");
    }
}

#[test]
fn a_word_in_a_string_or_a_comment_is_no_name() {
    let text = lowered(
        "import QtQuick\nItem {\n    // don't\n    property string a: \"can't\"\n    property string b: qsTr(\"Don't\")\n}\n",
    );
    assert!(text.contains("b: t`Don't`"), "{text}");
}

#[test]
fn a_message_where_the_pass_cannot_see_every_name_stays_qt_s() {
    // A delegate's row may have a role `t`; the file is lowered, and
    // `qsTr` still runs.
    let text = lowered(
        "import QtQuick\nListView {\n    model: 3\n    delegate: Text { text: qsTr(\"Row\") }\n}\n",
    );
    assert!(text.contains("qsTr(\"Row\")"), "{text}");
    assert!(!text.contains("Ljos.Core"), "{text}");
}

#[test]
fn a_list_element_s_message_stays_qt_s() {
    let text = lowered(
        "import QtQuick\nItem {\n    property string a: qsTr(\"A\")\n    ListModel { ListElement { name: qsTr(\"B\") } }\n}\n",
    );
    assert!(text.contains("a: t`A`"), "{text}");
    assert!(text.contains("name: qsTr(\"B\")"), "{text}");
}

#[test]
fn a_local_named_qs_tr_is_the_script_s() {
    let text = lowered(
        "import QtQuick\nItem {\n    function f(qsTr) { return qsTr(\"x\") }\n}\n",
    );
    assert!(text.contains("return qsTr(\"x\")"), "{text}");
}
