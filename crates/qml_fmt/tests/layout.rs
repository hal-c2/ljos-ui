//! The layout rules, one case each. Every case is also checked for what the
//! formatter promises of all its output: the same document, and nothing
//! left to do on a second pass.

use qml_fmt::{Error, format, same_tree};

#[track_caller]
fn fmt(source: &str) -> String {
    let out = format(source).unwrap_or_else(|e| panic!("{e}\n{source}"));
    same_tree(source, &out).unwrap_or_else(|e| panic!("{e}\n{out}"));
    assert_eq!(
        format(&out).as_deref(),
        Ok(out.as_str()),
        "a second pass changed it"
    );
    out
}

#[test]
fn one_member_a_line_four_spaces_a_level() {
    let out = fmt("import QtQuick\nItem { width: 10; height: 20\n  Text{text:\"hi\"}\n}\n");
    assert_eq!(
        out,
        "import QtQuick\n\nItem {\n    width: 10\n    height: 20\n    Text {\n        text: \"hi\"\n    }\n}\n"
    );
}

#[test]
fn an_empty_object_is_one_line_unless_a_comment_is_in_it() {
    assert_eq!(
        fmt("Item {\n    Rectangle {\n    }\n}"),
        "Item {\n    Rectangle {}\n}\n"
    );
    assert_eq!(
        fmt("Item {\n    Rectangle {\n   // todo\n    }\n}"),
        "Item {\n    Rectangle {\n        // todo\n    }\n}\n"
    );
    assert_eq!(fmt("Item { /* empty */ }"), "Item { /* empty */\n}\n");
}

#[test]
fn the_brace_comes_up_to_the_type() {
    assert_eq!(
        fmt("Item\n{\n    Text\n    {\n    }\n}\n"),
        "Item {\n    Text {}\n}\n"
    );
}

#[test]
fn one_empty_line_is_kept_between_members_and_none_at_an_edge() {
    let out = fmt("Item {\n\n    width: 1\n\n\n\n    height: 2\n    x: 3\n\n}\n");
    assert_eq!(out, "Item {\n    width: 1\n\n    height: 2\n    x: 3\n}\n");
}

#[test]
fn an_empty_line_follows_the_imports_and_none_is_added_between_them() {
    let out = fmt("pragma Singleton\nimport QtQuick\n\nimport QtQuick.Controls\nItem {}\n");
    assert_eq!(
        out,
        "pragma Singleton\nimport QtQuick\n\nimport QtQuick.Controls\n\nItem {}\n"
    );
    // Nothing to separate: no line is invented.
    assert_eq!(fmt("\n\nItem {}\n\n\n"), "Item {}\n");
}

#[test]
fn pragmas_and_imports_are_spaced_and_not_sorted() {
    let source = "pragma ComponentBehavior:Bound;\nimport  QtQuick.Controls   as   C ;\nimport QtQuick  2.15\nimport 'parts'  as  P\nimport \"a b/c.js\" as Lib\nItem {}\n";
    assert_eq!(
        fmt(source),
        "pragma ComponentBehavior: Bound\nimport QtQuick.Controls as C\nimport QtQuick 2.15\nimport 'parts' as P\nimport \"a b/c.js\" as Lib\n\nItem {}\n"
    );
}

#[test]
fn comments_keep_their_place() {
    let source = "// A box.\nimport QtQuick // the module\n// The root.\nItem {\n  // how wide\n  width: 10   // in pixels\n  /* and\n     how tall */\n  height: 20\n} // done\n// bye\n";
    assert_eq!(
        fmt(source),
        "// A box.\nimport QtQuick // the module\n\n// The root.\nItem {\n    // how wide\n    width: 10 // in pixels\n    /* and\n       how tall */\n    height: 20\n} // done\n// bye\n"
    );
}

#[test]
fn a_comment_on_the_line_of_the_brace_stays_on_it() {
    let out = fmt("Item { // the root\n    width: 1\n    Text { /* t */ text: \"a\" }\n}\n");
    assert_eq!(
        out,
        "Item { // the root\n    width: 1\n    Text { /* t */\n        text: \"a\"\n    }\n}\n"
    );
}

#[test]
fn a_comment_before_a_member_on_its_line_stays_before_it() {
    assert_eq!(
        fmt("Item {\n    /* w */ width: 1; /* h */ height: 2\n}\n"),
        "Item {\n    /* w */ width: 1 /* h */\n    height: 2\n}\n"
    );
}

#[test]
fn a_comment_where_the_layout_would_write_keeps_the_member_as_written() {
    // Between a name and its value, a type and its brace, or the objects of
    // a list: nowhere to put it but where it is.
    let source = "Item {\n  width /* w */ :   10\n  Text /* t */ {\n      text: \"a\";  color: \"red\"\n  }\n  states: [ State {}, /* s */ State {} ]\n  property int /* p */ n: 1\n  height: 2 /* h */;\n}\n";
    assert_eq!(
        fmt(source),
        "Item {\n    width /* w */ :   10\n    Text /* t */ {\n        text: \"a\"\n        color: \"red\"\n    }\n    states: [ State {}, /* s */ State {} ]\n    property int /* p */ n: 1\n    height: 2 /* h */;\n}\n"
    );
}

#[test]
fn javascript_moves_with_its_member_and_is_not_rewritten() {
    let source = "Item {\n  property int taps;\n  signal tapped();\n  onTapped: {\n      if (taps>2)   taps = 0\n      else\n          taps += 1\n  }\n  function reset(to) { taps=to }\n  width: taps\n      + 1;\n}\n";
    assert_eq!(
        fmt(source),
        "Item {\n    property int taps\n    signal tapped()\n    onTapped: {\n        if (taps>2)   taps = 0\n        else\n            taps += 1\n    }\n    function reset(to) { taps=to }\n    width: taps\n        + 1\n}\n"
    );
}

#[test]
fn a_line_left_of_its_member_comes_to_the_member() {
    let source = "Item {\n        onTapped: {\n    a()\n            b()\n        }\n}\n";
    assert_eq!(
        fmt(source),
        "Item {\n    onTapped: {\n    a()\n        b()\n    }\n}\n"
    );
}

#[test]
fn tabs_before_a_line_count_four_columns() {
    let source = "Item {\n\tonTapped: {\n\t\ta()\n\t}\n}\n";
    assert_eq!(
        fmt(source),
        "Item {\n    onTapped: {\n        a()\n    }\n}\n"
    );
}

#[test]
fn text_over_several_lines_is_not_touched() {
    let source =
        "Text {\n  text: `one\n  two   \n\n      three`\n  property string s: \"a\\\n   b\"\n}\n";
    assert_eq!(
        fmt(source),
        "Text {\n    text: `one\n  two   \n\n      three`\n    property string s: \"a\\\n   b\"\n}\n"
    );
}

#[test]
fn white_space_at_a_line_end_goes() {
    let source = "Item {   \n    onTapped: {   \n        a()  \n   \n        b()\n    }  \n}  \n";
    assert_eq!(
        fmt(source),
        "Item {\n    onTapped: {\n        a()\n\n        b()\n    }\n}\n"
    );
}

#[test]
fn a_list_of_objects_has_a_line_for_each() {
    let source = "Item {\n    states: [ State { name: \"on\" }, State {}, ]\n    transitions: []\n    property list<Item> things: [Item {}]\n}\n";
    assert_eq!(
        fmt(source),
        "Item {\n    states: [\n        State {\n            name: \"on\"\n        },\n        State {}\n    ]\n    transitions: []\n    property list<Item> things: [\n        Item {}\n    ]\n}\n"
    );
}

#[test]
fn values_that_are_objects_are_laid_out_too() {
    let source = "Item {\n    background: Rectangle { color: \"red\" }\n    Behavior   on   x { NumberAnimation{} }\n    font { bold: true; pixelSize: 12 }\n    property Item mark: Item{}\n    component Dot : Rectangle { radius: 2 }\n}\n";
    assert_eq!(
        fmt(source),
        "Item {\n    background: Rectangle {\n        color: \"red\"\n    }\n    Behavior on x {\n        NumberAnimation {}\n    }\n    font {\n        bold: true\n        pixelSize: 12\n    }\n    property Item mark: Item {}\n    component Dot: Rectangle {\n        radius: 2\n    }\n}\n"
    );
}

#[test]
fn declarations_keep_their_words() {
    let source = "Item {\n    default   property alias content: inner.data;\n    required property string title\n    readonly property var o: ({ a: 1 })\n    required   name;\n    signal picked(int index,\n                  name: string);\n    enum Mode { Off, On = 4 }\n    Item { id: inner }\n}\n";
    assert_eq!(
        fmt(source),
        "Item {\n    default property alias content: inner.data\n    required property string title\n    readonly property var o: ({ a: 1 })\n    required name\n    signal picked(int index,\n                  name: string)\n    enum Mode { Off, On = 4 }\n    Item {\n        id: inner\n    }\n}\n"
    );
}

#[test]
fn a_semicolon_stays_where_the_next_line_would_carry_the_expression_on() {
    // `b` and then `in: 1` on the next line is `b in: 1`, which is no QML.
    assert!(format("QtObject {\n    property var a: b\n    in: 1\n}\n").is_err());
    let source = "QtObject { property var a: b; in: 1; x: c; instanceof: 2; inner: 3; }";
    assert_eq!(
        fmt(source),
        "QtObject {\n    property var a: b;\n    in: 1\n    x: c;\n    instanceof: 2\n    inner: 3\n}\n"
    );
}

#[test]
fn windows_line_ends_and_a_byte_order_mark_are_normalised() {
    let out = fmt("\u{feff}import QtQuick\r\nItem {\r\n    width: 1\r\n}\r\n");
    assert_eq!(out, "import QtQuick\n\nItem {\n    width: 1\n}\n");
}

#[test]
fn a_text_that_does_not_parse_is_refused_with_where() {
    let Err(Error::Syntax(errors)) = format("import QtQuick\nItem {\n    width: 10 +\n}\n") else {
        panic!("it was formatted");
    };
    assert!(!errors.is_empty());
    assert_eq!(errors[0].line, 4);
    assert!(!errors[0].message.is_empty());
    assert!(matches!(format(""), Err(Error::Syntax(_))));
    assert!(matches!(format("Item {"), Err(Error::Syntax(_))));
}

#[test]
fn the_same_text_always_gives_the_same_layout() {
    let source = "import QtQuick\nItem { width: 10; Text { text: \"hi\" } // greeting\n}\n";
    let first = fmt(source);
    for _ in 0..3 {
        assert_eq!(fmt(source), first);
    }
    assert_eq!(
        first,
        "import QtQuick\n\nItem {\n    width: 10\n    Text {\n        text: \"hi\"\n    } // greeting\n}\n"
    );
}

#[test]
fn same_tree_tells_a_changed_token_from_changed_spacing() {
    assert!(same_tree("Item { width: 1+2 }", "Item {\n    width: 1 + 2\n}\n").is_ok());
    assert!(same_tree("Item { width: 1+2 }", "Item { width: 1+3 }").is_err());
    assert!(same_tree("Item { width: 1 }", "Item { height: 1 }").is_err());
    assert!(same_tree("Item { /* a */ }", "Item { /* b */ }").is_err());
    assert!(
        same_tree(
            "Item { function f() { return 1 } }",
            "Item { function f() { return 2 } }"
        )
        .is_err()
    );
    assert!(same_tree("import A\nItem {}", "import B\nItem {}").is_err());
    assert!(same_tree("Item {}", "Item {").is_err());
}
