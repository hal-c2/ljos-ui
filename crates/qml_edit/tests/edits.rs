//! Each edit on a small file, with the exact text it must give.

use qml_edit::{BadPath, Edit, Error, NodePath, Step, apply, find_id};

const PAGE: &str = r#"import QtQuick

// The page a designer opens.
Item {
    id: page
    width: 640 // wide enough for two columns
    height: 480

    /* The header: its colour
       comes from the theme. */
    Rectangle {
        id: header
        color: "steelblue"
        Text { text: "Title"; color: "white" }
    }

    // Footer, kept short.
    Rectangle {
        id: footer
        height: 40
    }

    function reset() { width = 640 }
}
"#;

fn edit(edit: Edit) -> String {
    apply(PAGE, &edit).expect("the edit applies").text
}

fn root() -> NodePath {
    NodePath::root()
}

#[test]
fn changing_a_value_touches_only_the_value() {
    let out = edit(Edit::SetBinding {
        object: root(),
        name: "width".into(),
        value: "800".into(),
    });
    assert_eq!(out, PAGE.replace("width: 640 //", "width: 800 //"));
}

#[test]
fn a_new_binding_goes_after_the_last_binding_before_the_children() {
    let out = edit(Edit::SetBinding {
        object: root(),
        name: "clip".into(),
        value: "true".into(),
    });
    assert_eq!(
        out,
        PAGE.replace("    height: 480\n", "    height: 480\n    clip: true\n")
    );
}

#[test]
fn a_binding_on_a_line_shared_with_others_is_added_inside_the_line() {
    let out = edit(Edit::SetBinding {
        object: root().child(0).child(0),
        name: "font.bold".into(),
        value: "true".into(),
    });
    assert_eq!(
        out,
        PAGE.replace(
            r#"color: "white" }"#,
            r#"color: "white"; font.bold: true }"#
        )
    );
}

#[test]
fn removing_a_binding_takes_its_trailing_comment_and_its_line() {
    let out = edit(Edit::RemoveBinding {
        object: root(),
        name: "width".into(),
    });
    assert_eq!(
        out,
        PAGE.replace("    width: 640 // wide enough for two columns\n", "")
    );
}

#[test]
fn removing_a_binding_inside_a_line_keeps_the_line() {
    let out = edit(Edit::RemoveBinding {
        object: root().child(0).child(0),
        name: "text".into(),
    });
    assert_eq!(out, PAGE.replace(r#"text: "Title"; "#, ""));
}

#[test]
fn a_child_is_inserted_at_its_siblings_indentation_and_grouping() {
    let out = edit(Edit::InsertObject {
        parent: root(),
        index: 1,
        text: "Rectangle {\n    height: 2\n    color: \"gray\"\n}".into(),
    });
    assert_eq!(
        out,
        PAGE.replace(
            "    // Footer, kept short.\n",
            "    Rectangle {\n        height: 2\n        color: \"gray\"\n    }\n\n    // Footer, kept short.\n"
        )
    );
}

#[test]
fn a_child_added_last_goes_after_the_last_member_in_its_own_group() {
    let out = edit(Edit::InsertObject {
        parent: root(),
        index: 2,
        text: "Item {}".into(),
    });
    assert_eq!(
        out,
        PAGE.replace(
            "    function reset() { width = 640 }\n",
            "    function reset() { width = 640 }\n\n    Item {}\n"
        )
    );
}

#[test]
fn a_child_put_in_an_empty_object_opens_it() {
    let source = "Item {\n    Item {}\n}\n";
    let out = apply(
        source,
        &Edit::InsertObject {
            parent: NodePath::root().child(0),
            index: 0,
            text: "Rectangle {\n    width: 1\n}".into(),
        },
    )
    .unwrap()
    .text;
    assert_eq!(
        out,
        "Item {\n    Item {\n        Rectangle {\n            width: 1\n        }\n    }\n}\n"
    );
}

#[test]
fn removing_the_last_child_of_an_object_closes_it() {
    let source =
        "Item {\n    Item {\n        Rectangle {\n            width: 1\n        }\n    }\n}\n";
    let out = apply(
        source,
        &Edit::RemoveObject {
            object: NodePath::root().child(0).child(0),
        },
    )
    .unwrap()
    .text;
    assert_eq!(out, "Item {\n    Item {}\n}\n");
}

#[test]
fn a_removed_child_takes_the_comment_above_it() {
    let out = edit(Edit::RemoveObject {
        object: root().child(1),
    });
    assert_eq!(
        out,
        PAGE.replace("    // Footer, kept short.\n    Rectangle {\n        id: footer\n        height: 40\n    }\n\n", "")
    );
}

#[test]
fn a_moved_child_keeps_its_comment_its_inside_and_the_empty_line_above_it() {
    let out = edit(Edit::MoveObject {
        object: root().child(0),
        parent: root().child(1),
        index: 0,
    });
    let expected = r#"import QtQuick

// The page a designer opens.
Item {
    id: page
    width: 640 // wide enough for two columns
    height: 480

    // Footer, kept short.
    Rectangle {
        id: footer
        height: 40

        /* The header: its colour
           comes from the theme. */
        Rectangle {
            id: header
            color: "steelblue"
            Text { text: "Title"; color: "white" }
        }
    }

    function reset() { width = 640 }
}
"#;
    assert_eq!(out, expected);
}

#[test]
fn moving_a_child_back_gives_the_file_back() {
    let moved = edit(Edit::MoveObject {
        object: root().child(0),
        parent: root(),
        index: 1,
    });
    let back = apply(
        &moved,
        &Edit::MoveObject {
            object: root().child(1),
            parent: root(),
            index: 0,
        },
    )
    .unwrap()
    .text;
    assert_eq!(back, PAGE);
}

#[test]
fn crlf_files_stay_crlf() {
    let source = PAGE.replace('\n', "\r\n");
    let out = apply(
        &source,
        &Edit::InsertObject {
            parent: NodePath::root(),
            index: 0,
            text: "Item {\n    width: 1\n}".into(),
        },
    )
    .unwrap()
    .text;
    assert!(!out.replace("\r\n", "").contains('\n'), "{out:?}");
}

#[test]
fn an_object_cannot_move_into_itself() {
    assert_eq!(
        apply(
            PAGE,
            &Edit::MoveObject {
                object: root().child(0),
                parent: root().child(0).child(0),
                index: 0,
            }
        ),
        Err(Error::IntoItself)
    );
}

#[test]
fn an_edit_that_would_break_the_file_changes_nothing() {
    let result = apply(
        PAGE,
        &Edit::SetBinding {
            object: root(),
            name: "width".into(),
            value: "(640".into(),
        },
    );
    assert!(matches!(result, Err(Error::WouldBreak(_))), "{result:?}");
}

#[test]
fn a_file_that_does_not_parse_is_refused() {
    let result = apply(
        "Item { width: }",
        &Edit::RemoveObject {
            object: root().child(0),
        },
    );
    assert!(matches!(result, Err(Error::Syntax(_))), "{result:?}");
}

#[test]
fn objects_are_found_by_id_and_through_values() {
    assert_eq!(find_id(PAGE, "footer").unwrap(), Some(root().child(1)));
    let source = "Button {\n    background: Rectangle {\n        Text {}\n    }\n}\n";
    let path = NodePath(vec![Step::Value("background".into()), Step::Child(0)]);
    let out = apply(
        source,
        &Edit::SetBinding {
            object: path,
            name: "text".into(),
            value: "\"Go\"".into(),
        },
    )
    .unwrap()
    .text;
    assert_eq!(
        out,
        "Button {\n    background: Rectangle {\n        Text { text: \"Go\" }\n    }\n}\n"
    );
}

#[test]
fn an_object_written_on_one_line_opens_up_for_a_child_of_several_lines() {
    let source = "Item {\n    Rectangle { width: 10; height: 10 }\n}\n";
    let out = apply(
        source,
        &Edit::InsertObject {
            parent: NodePath::root().child(0),
            index: 0,
            text: "Text {\n    text: \"hi\"\n}".into(),
        },
    )
    .unwrap()
    .text;
    assert_eq!(
        out,
        "Item {\n    Rectangle {\n        width: 10\n        height: 10\n        Text {\n            text: \"hi\"\n        }\n    }\n}\n"
    );
}

fn applied(source: &str, edit: Edit) -> String {
    apply(source, &edit).expect("the edit applies").text
}

/// An object opened up is laid out as the formatter lays it out.
fn formatted(text: String) -> String {
    assert_eq!(qml_fmt::format(&text).unwrap(), text);
    text
}

fn set(object: NodePath, name: &str, value: &str) -> Edit {
    Edit::SetBinding {
        object,
        name: name.into(),
        value: value.into(),
    }
}

fn remove(object: NodePath, name: &str) -> Edit {
    Edit::RemoveBinding {
        object,
        name: name.into(),
    }
}

#[test]
fn a_dotted_name_finds_its_binding_in_a_group() {
    let source = "Text {\n    font { family: \"Sans\"; bold: false }\n}\n";
    assert_eq!(
        applied(source, set(root(), "font.bold", "true")),
        "Text {\n    font { family: \"Sans\"; bold: true }\n}\n"
    );
}

#[test]
fn a_dotted_name_finds_its_binding_written_dotted() {
    let source = "Text {\n    font.bold: false\n}\n";
    assert_eq!(
        applied(source, set(root(), "font.bold", "true")),
        "Text {\n    font.bold: true\n}\n"
    );
}

#[test]
fn a_name_of_three_parts_finds_its_binding_through_nested_groups() {
    let source = "Item {\n    a {\n        b.c: 1\n    }\n    d { e { f: 2 } }\n}\n";
    let out = applied(source, set(root(), "a.b.c", "10"));
    assert_eq!(out, source.replace("b.c: 1", "b.c: 10"));
    let out = applied(source, set(root(), "d.e.f", "20"));
    assert_eq!(out, source.replace("f: 2", "f: 20"));
}

#[test]
fn a_new_binding_goes_into_the_group_the_object_has() {
    let source = "Item {\n    anchors {\n        left: parent.left\n    }\n    width: 10\n}\n";
    assert_eq!(
        applied(source, set(root(), "anchors.right", "parent.right")),
        "Item {\n    anchors {\n        left: parent.left\n        right: parent.right\n    }\n    width: 10\n}\n"
    );
}

#[test]
fn a_new_binding_goes_into_an_empty_group() {
    let source = "Text {\n    font {}\n}\n";
    assert_eq!(
        applied(source, set(root(), "font.bold", "true")),
        "Text {\n    font { bold: true }\n}\n"
    );
}

#[test]
fn a_new_dotted_binding_goes_next_to_the_ones_dotted_the_same_way() {
    let source = "Text {\n    font.pixelSize: 12\n    color: \"red\"\n    text: \"hi\"\n}\n";
    assert_eq!(
        applied(source, set(root(), "font.bold", "true")),
        "Text {\n    font.pixelSize: 12\n    font.bold: true\n    color: \"red\"\n    text: \"hi\"\n}\n"
    );
}

/// The file groups the bindings of `font` and `anchors` elsewhere.
const GROUPED: &str = r#"Item {
    Text {
        font {
            family: "Sans"
            pixelSize: 12
        }
        anchors {
            left: parent.left
            right: parent.right
        }
    }
    Text {
        text: "two"

        Rectangle {}
    }
}
"#;

#[test]
fn a_file_that_writes_groups_gets_a_new_group() {
    assert_eq!(
        applied(GROUPED, set(root().child(1), "font.bold", "true")),
        GROUPED.replace(
            "        text: \"two\"\n",
            "        text: \"two\"\n        font {\n            bold: true\n        }\n"
        )
    );
}

#[test]
fn an_attached_property_is_never_grouped() {
    assert_eq!(
        applied(GROUPED, set(root().child(1), "Layout.fillWidth", "true")),
        GROUPED.replace(
            "        text: \"two\"\n",
            "        text: \"two\"\n        Layout.fillWidth: true\n"
        )
    );
}

#[test]
fn a_file_that_writes_dotted_names_gets_a_dotted_one() {
    let source = "Item {\n    Text { font.family: \"Sans\"; font.pixelSize: 12 }\n    Text {\n        text: \"two\"\n    }\n}\n";
    assert_eq!(
        applied(source, set(root().child(1), "font.bold", "true")),
        source.replace(
            "        text: \"two\"\n",
            "        text: \"two\"\n        font.bold: true\n"
        )
    );
}

#[test]
fn removing_a_binding_from_a_group_keeps_the_others() {
    assert_eq!(
        applied(GROUPED, remove(root().child(0), "font.pixelSize")),
        GROUPED.replace("            pixelSize: 12\n", "")
    );
}

#[test]
fn removing_the_last_binding_of_a_group_removes_the_group() {
    let source = "Text {\n    font {\n        bold: true\n    }\n    text: \"hi\"\n}\n";
    assert_eq!(
        applied(source, remove(root(), "font.bold")),
        "Text {\n    text: \"hi\"\n}\n"
    );
    let nested = "Item {\n    a { b { c: 1 } }\n    width: 1\n}\n";
    assert_eq!(
        applied(nested, remove(root(), "a.b.c")),
        "Item {\n    width: 1\n}\n"
    );
}

#[test]
fn a_group_with_a_comment_of_its_own_stays() {
    let source = "Text {\n    font {\n        bold: true\n        // more to come\n    }\n}\n";
    assert_eq!(
        applied(source, remove(root(), "font.bold")),
        "Text {\n    font {\n        // more to come\n    }\n}\n"
    );
}

#[test]
fn groups_are_not_children() {
    let source = "Item {\n    font { bold: true }\n    Rectangle { id: a }\n    layer { effect: ShaderEffect { id: b } }\n}\n";
    let all: Vec<String> = qml_edit::objects(source)
        .unwrap()
        .into_iter()
        .map(|(path, name)| format!("{path} {name}"))
        .collect();
    assert_eq!(
        all,
        [
            "root Item",
            "root/0 Rectangle",
            "root/layer.effect: ShaderEffect"
        ]
    );
    assert_eq!(
        find_id(source, "b").unwrap().unwrap().to_string(),
        "root/layer.effect:"
    );
    assert_eq!(
        applied(
            source,
            Edit::RemoveObject {
                object: root().child(0)
            }
        ),
        source.replace("    Rectangle { id: a }\n", "")
    );
    assert_eq!(
        apply(
            source,
            &Edit::RemoveObject {
                object: root().child(1)
            }
        ),
        Err(Error::NotFound(root().child(1)))
    );
}

#[test]
fn members_that_share_lines_are_laid_out_one_a_line_to_take_several_lines() {
    let source =
        "Item {\n    Rectangle {\n        width: 1; height: 2\n        color: \"red\" }\n}\n";
    assert_eq!(
        formatted(applied(
            source,
            Edit::InsertObject {
                parent: root().child(0),
                index: 0,
                text: "Text {\n    text: \"hi\"\n}".into(),
            }
        )),
        "Item {\n    Rectangle {\n        width: 1\n        height: 2\n        color: \"red\"\n        Text {\n            text: \"hi\"\n        }\n    }\n}\n"
    );
}

#[test]
fn comments_stay_with_their_members_when_an_object_is_opened_up() {
    let source =
        "Row { // tools\n    spacing: 2; /* gap */ Button {} // first\n\n    Button {} }\n";
    assert_eq!(
        formatted(applied(
            source,
            Edit::InsertObject {
                parent: root(),
                index: 1,
                text: "Item {\n    width: 1\n}".into(),
            }
        )),
        // As `qml_fmt` has it, a comment on the line of a member trails it.
        "Row { // tools\n    spacing: 2 /* gap */\n    Button {} // first\n\n    Item {\n        width: 1\n    }\n\n    Button {}\n}\n"
    );
}

#[test]
fn a_new_binding_of_several_lines_opens_an_object_up() {
    let source = "Item { width: 1; height: 2 }\n";
    assert_eq!(
        formatted(applied(
            source,
            set(root(), "onWidthChanged", "{\n    log()\n}")
        )),
        "Item {\n    width: 1\n    height: 2\n    onWidthChanged: {\n        log()\n    }\n}\n"
    );
}

#[test]
fn an_empty_object_with_a_comment_keeps_it() {
    let source = "Item { /* nothing yet */ }\n";
    assert_eq!(
        formatted(applied(
            source,
            Edit::InsertObject {
                parent: root(),
                index: 0,
                text: "Item {}".into(),
            }
        )),
        "Item { /* nothing yet */\n    Item {}\n}\n"
    );
}

#[test]
fn a_child_moved_within_an_object_that_opens_up_is_not_left_behind() {
    let source = "Column {\n    Text { text: \"a\" } Text { text: \"b\" }\n    Item {\n        width: 1\n    } }\n";
    assert_eq!(
        applied(
            source,
            Edit::MoveObject {
                object: root().child(2),
                parent: root(),
                index: 0,
            }
        ),
        "Column {\n    Item {\n        width: 1\n    }\n    Text { text: \"a\" }\n    Text { text: \"b\" }\n}\n"
    );
}

#[test]
fn a_path_reads_back_what_it_writes() {
    let paths = [
        NodePath::root(),
        NodePath(vec![Step::Child(0), Step::Child(12)]),
        NodePath(vec![
            Step::Value("contentItem".into()),
            Step::Child(2),
            Step::Value("layer.effect".into()),
        ]),
        NodePath(vec![Step::Component("Badge".into()), Step::Child(1)]),
    ];
    for path in paths {
        let text = path.to_string();
        assert_eq!(text.parse::<NodePath>(), Ok(path), "{text}");
    }
}

#[test]
fn a_path_that_is_not_one_is_refused() {
    for text in [
        "",
        "0/1",
        "root/",
        "root/x",
        "root/-1",
        "root/component ",
        "root/a b:",
        "root//0",
    ] {
        assert_eq!(
            text.parse::<NodePath>(),
            Err(BadPath(text.into())),
            "{text}"
        );
    }
}

const TIGHT: &str = "Item {\n    width: 200\n\n    Rectangle {}\n    Text {}\n}\n";

#[test]
fn the_blank_line_ending_the_bindings_is_not_spacing_among_children() {
    let first = apply(
        TIGHT,
        &Edit::InsertObject {
            parent: root(),
            index: 0,
            text: "Item {}".into(),
        },
    )
    .unwrap();
    assert_eq!(
        first.text,
        "Item {\n    width: 200\n\n    Item {}\n    Rectangle {}\n    Text {}\n}\n"
    );
}

#[test]
fn a_child_put_first_among_spaced_children_is_spaced() {
    let spaced = TIGHT.replace("{}\n    Text", "{}\n\n    Text");
    let first = apply(
        &spaced,
        &Edit::InsertObject {
            parent: root(),
            index: 0,
            text: "Item {}".into(),
        },
    )
    .unwrap();
    assert_eq!(
        first.text,
        "Item {\n    width: 200\n\n    Item {}\n\n    Rectangle {}\n\n    Text {}\n}\n"
    );
}

#[test]
fn a_second_child_after_the_bindings_blank_line_is_not_spaced() {
    let one = "Item {\n    width: 200\n\n    Rectangle {}\n}\n";
    let second = apply(
        one,
        &Edit::InsertObject {
            parent: root(),
            index: 1,
            text: "Item {}".into(),
        },
    )
    .unwrap();
    assert_eq!(
        second.text,
        "Item {\n    width: 200\n\n    Rectangle {}\n    Item {}\n}\n"
    );
}
