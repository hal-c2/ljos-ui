//! Each edit on a small file, with the exact text it must give.

use qml_edit::{Edit, Error, NodePath, Step, apply, find_id};

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
