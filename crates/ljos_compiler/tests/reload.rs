//! The reload diff, the session that keeps a project compiled, and the lint
//! that asks for ids (ARCHITECTURE.md §12.1).

use ljos_compiler::{CompileFault, ReloadSession, reload_lint};
use ljos_ir::{KeySeg, ObjectKey, ReloadOp, ReloadPatch};
use qml_front::QtTypeOracle;

fn session(files: &[(&str, &str)]) -> ReloadSession<QtTypeOracle> {
    let mut s = ReloadSession::in_memory(QtTypeOracle);
    for (key, source) in files {
        s.set_file(key, *source);
    }
    s.start();
    let failed: Vec<&str> = s.failures().map(|(key, _)| key).collect();
    assert!(failed.is_empty(), "{failed:?} do not compile");
    s
}

/// The patch from `before` to `after`, both the file `Main`.
fn patch(before: &str, after: &str) -> ReloadPatch {
    let mut s = session(&[("Main", before)]);
    s.set_file("Main", after);
    s.build().expect("compiles").expect("a change")
}

fn key(segments: &[KeySeg]) -> ObjectKey {
    ObjectKey {
        component: "Main".into(),
        segments: segments.to_vec(),
    }
}

fn id(name: &str) -> KeySeg {
    KeySeg::Id(name.into())
}

fn typed(ty: &str, nth: u32) -> KeySeg {
    KeySeg::Typed { ty: ty.into(), nth }
}

fn changed(object: ObjectKey, property: &str) -> ReloadOp {
    ReloadOp::PropertyChanged {
        file: "Main".into(),
        object,
        property: property.into(),
    }
}

#[test]
fn a_changed_value_is_one_op() {
    let p = patch(
        "import QtQuick\nItem { width: 10; height: 5 }\n",
        "import QtQuick\nItem { width: 20; height: 5 }\n",
    );
    assert_eq!(p.ops, [changed(key(&[]), "width")]);
    assert_eq!((p.base, p.generation), (0, 1));
    assert_eq!(p.files.len(), 1);
    assert!(!p.files[0].added);
}

#[test]
fn a_value_that_becomes_a_binding_is_a_change() {
    let p = patch(
        "import QtQuick\nItem { property int n: 2; width: 10 }\n",
        "import QtQuick\nItem { property int n: 2; width: n * 2 }\n",
    );
    assert_eq!(p.ops, [changed(key(&[]), "width")]);
}

#[test]
fn a_changed_binding_is_a_change_and_an_unchanged_one_is_not() {
    let p = patch(
        "import QtQuick\nItem { property int n: 2; width: n * 2; height: n + 1 }\n",
        "import QtQuick\nItem { property int n: 2; width: n * 3; height: n + 1 }\n",
    );
    assert_eq!(p.ops, [changed(key(&[]), "width")]);
}

#[test]
fn a_binding_is_the_same_when_an_object_is_added_before_what_it_names() {
    let p = patch(
        "import QtQuick\nItem { width: box.width; Rectangle { id: box } }\n",
        "import QtQuick\nItem { width: box.width; Text {} Rectangle { id: box } }\n",
    );
    assert_eq!(
        p.ops,
        [ReloadOp::ObjectAdded {
            file: "Main".into(),
            object: key(&[typed("Text", 0)]),
        }]
    );
}

#[test]
fn group_and_attached_properties_are_named_with_their_prefix() {
    let p = patch(
        "import QtQuick\nimport QtQuick.Layouts\nItem { Rectangle { id: r; anchors.margins: 1; Layout.fillWidth: true } }\n",
        "import QtQuick\nimport QtQuick.Layouts\nItem { Rectangle { id: r; anchors.margins: 2; Layout.fillWidth: false } }\n",
    );
    assert_eq!(
        p.ops,
        [
            changed(key(&[id("r")]), "Layout.fillWidth"),
            changed(key(&[id("r")]), "anchors.margins"),
        ]
    );
}

#[test]
fn handlers_and_functions_are_changes_of_their_own() {
    let p = patch(
        "import QtQuick\nItem { property int n; onNChanged: n = 1; function f() { return 1 } }\n",
        "import QtQuick\nItem { property int n; onNChanged: n = 2; function f() { return 2 } }\n",
    );
    assert_eq!(
        p.ops,
        [
            ReloadOp::HandlerChanged {
                file: "Main".into(),
                object: key(&[]),
                signal: "nChanged".into(),
            },
            ReloadOp::DeclarationChanged {
                file: "Main".into(),
                object: key(&[]),
                name: "f".into(),
            },
        ]
    );
}

#[test]
fn completed_handlers_are_named_by_the_type_that_attaches_them() {
    let p = patch(
        "import QtQuick\nItem { }\n",
        "import QtQuick\nItem { Component.onCompleted: console.log(1) }\n",
    );
    assert_eq!(
        p.ops,
        [ReloadOp::HandlerChanged {
            file: "Main".into(),
            object: key(&[]),
            signal: "Component.completed".into(),
        }]
    );
}

#[test]
fn objects_are_added_and_removed() {
    let p = patch(
        "import QtQuick\nItem { Rectangle { id: a } Text { } }\n",
        "import QtQuick\nItem { Rectangle { id: a } Rectangle { id: b } }\n",
    );
    assert_eq!(
        p.ops,
        [
            ReloadOp::ObjectRemoved {
                file: "Main".into(),
                object: key(&[typed("Text", 0)]),
            },
            ReloadOp::ObjectAdded {
                file: "Main".into(),
                object: key(&[id("b")]),
            },
        ]
    );
}

#[test]
fn an_object_with_an_id_that_changes_parents_is_moved() {
    let p = patch(
        "import QtQuick\nItem { Item { id: group } Rectangle { id: box; width: 1 } }\n",
        "import QtQuick\nItem { Item { id: group; Rectangle { id: box; width: 1 } } }\n",
    );
    assert_eq!(
        p.ops,
        [ReloadOp::ObjectMoved {
            file: "Main".into(),
            from: key(&[id("box")]),
            to: key(&[id("group"), id("box")]),
        }]
    );
}

#[test]
fn siblings_that_change_places_are_moved() {
    let p = patch(
        "import QtQuick\nItem { Rectangle { objectName: \"a\" } Rectangle { objectName: \"b\" } }\n",
        "import QtQuick\nItem { Rectangle { objectName: \"b\" } Rectangle { objectName: \"a\" } }\n",
    );
    assert_eq!(p.ops.len(), 2, "{:?}", p.ops);
    assert!(
        p.ops
            .iter()
            .all(|op| matches!(op, ReloadOp::ObjectMoved { .. }))
    );
}

#[test]
fn another_type_under_the_same_id_is_a_type_change() {
    let p = patch(
        "import QtQuick\nItem { Rectangle { id: box; width: 1 } }\n",
        "import QtQuick\nItem { Item { id: box; width: 2 } }\n",
    );
    assert_eq!(
        p.ops,
        [ReloadOp::TypeChanged {
            file: "Main".into(),
            object: key(&[id("box")]),
        }]
    );
}

#[test]
fn a_renamed_id_is_one_object_gone_and_one_new() {
    let p = patch(
        "import QtQuick\nItem { Rectangle { id: box } }\n",
        "import QtQuick\nItem { Rectangle { id: panel } }\n",
    );
    assert_eq!(
        p.ops,
        [
            ReloadOp::ObjectRemoved {
                file: "Main".into(),
                object: key(&[id("box")]),
            },
            ReloadOp::ObjectAdded {
                file: "Main".into(),
                object: key(&[id("panel")]),
            },
        ]
    );
}

#[test]
fn a_declared_property_changes_the_signature_of_its_component() {
    let p = patch(
        "import QtQuick\nItem { property int count: 0 }\n",
        "import QtQuick\nItem { property real count: 0; property bool extra: true }\n",
    );
    assert_eq!(
        p.ops,
        [
            ReloadOp::SignatureChanged {
                file: "Main".into(),
                component: "Main".into(),
            },
            changed(key(&[]), "count"),
            changed(key(&[]), "extra"),
        ]
    );
}

#[test]
fn imports_and_pragmas_are_reported() {
    let p = patch(
        "import QtQuick\nItem { }\n",
        "pragma LjosReload: reset\nimport QtQuick\nimport QtQuick.Controls\nItem { }\n",
    );
    assert_eq!(
        p.ops,
        [
            ReloadOp::ImportAdded {
                file: "Main".into(),
                source: "QtQuick.Controls".into(),
            },
            ReloadOp::PragmaChanged {
                file: "Main".into(),
                name: "LjosReload".into(),
            },
        ]
    );
}

#[test]
fn a_moved_line_is_a_patch_without_ops() {
    let p = patch(
        "import QtQuick\nItem { width: 10 }\n",
        "import QtQuick\n\nItem { width: 10 }\n",
    );
    assert!(p.ops.is_empty(), "{:?}", p.ops);
    assert_eq!(p.files.len(), 1);
}

#[test]
fn files_are_added_and_removed() {
    let mut s = session(&[("Main", "import QtQuick\nItem { }\n")]);
    s.set_file("Badge", "import QtQuick\nRectangle { }\n");
    s.set_file("Main", "import QtQuick\nItem { Badge { } }\n");
    let p = s.build().unwrap().unwrap();
    assert!(p.ops.contains(&ReloadOp::FileAdded {
        file: "Badge".into()
    }));
    assert!(p.files.iter().any(|f| f.added && f.file.file == "Badge"));
    s.ack(p.generation);
    assert_eq!(s.generation(), 1);

    s.set_file("Main", "import QtQuick\nItem { }\n");
    s.remove_file("Badge");
    let p = s.build().unwrap().unwrap();
    assert_eq!(p.removed, ["Badge"]);
    assert!(p.ops.contains(&ReloadOp::FileRemoved {
        file: "Badge".into()
    }));
    assert_eq!((p.base, p.generation), (1, 2));
}

#[test]
fn a_file_that_names_a_changed_file_is_compiled_again() {
    let mut s = session(&[
        ("Main", "import QtQuick\nItem { Counter { count: 2 } }\n"),
        (
            "Counter",
            "import QtQuick\nItem { property int count: 0 }\n",
        ),
        ("Other", "import QtQuick\nItem { }\n"),
    ]);
    s.set_file(
        "Counter",
        "import QtQuick\nItem { property real count: 0 }\n",
    );
    let p = s.build().unwrap().unwrap();
    let files: Vec<&str> = p.files.iter().map(|f| f.file.file.as_str()).collect();
    assert!(files.contains(&"Counter"), "{files:?}");
    assert!(!files.contains(&"Other"), "{files:?}");
}

#[test]
fn nothing_changed_is_no_patch() {
    let source = "import QtQuick\nItem { width: 10 }\n";
    let mut s = session(&[("Main", source)]);
    s.set_file("Main", source);
    assert!(s.build().unwrap().is_none());
    s.set_file("Main", "import QtQuick\nItem { width: 20 }\n");
    s.set_file("Main", source);
    assert!(s.build().unwrap().is_none());
}

#[test]
fn a_patch_that_is_not_acknowledged_is_offered_again_from_the_same_base() {
    let mut s = session(&[("Main", "import QtQuick\nItem { width: 10 }\n")]);
    s.set_file("Main", "import QtQuick\nItem { width: 20 }\n");
    let first = s.build().unwrap().unwrap();
    s.reject(first.generation);
    let second = s.build().unwrap().unwrap();
    assert_eq!(second.base, 0);
    assert_eq!(second.generation, 2);
    assert_eq!(second.ops, first.ops);
}

fn faults(before: &str, after: &str) -> Vec<CompileFault> {
    let mut s = session(&[("Main", before)]);
    s.set_file("Main", after);
    s.build().expect_err("does not compile")
}

#[test]
fn a_syntax_error_names_its_file_and_line() {
    let f = faults(
        "import QtQuick\nItem { width: 10 }\n",
        "import QtQuick\nItem {\n  objectName: \"root\"\n  width: 20 +\n  property int taps: 0\n}\n",
    );
    assert_eq!(f[0].file, "Main.qml");
    assert_eq!(f[0].line, 5, "{f:?}");
}

#[test]
fn an_unknown_type_names_its_file_and_line() {
    let f = faults(
        "import QtQuick\nItem { width: 10 }\n",
        "import QtQuick\nItem {\n  width: 20\n  Rectangel {}\n}\n",
    );
    assert_eq!(f[0].file, "Main.qml");
    assert_eq!(f[0].line, 4, "{f:?}");
    assert!(f[0].message.contains("Rectangel"), "{f:?}");
}

#[test]
fn a_broken_file_is_tried_again_and_a_good_one_then_applies() {
    let mut s = session(&[("Main", "import QtQuick\nItem { width: 10 }\n")]);
    s.set_file("Main", "import QtQuick\nItem { width: }\n");
    assert!(s.build().is_err());
    assert!(s.build().is_err());
    s.set_file("Main", "import QtQuick\nItem { width: 30 }\n");
    let p = s.build().unwrap().unwrap();
    assert_eq!(p.base, 0);
    assert_eq!(p.ops.len(), 1);
}

fn lint(source: &str) -> Vec<String> {
    let s = session(&[
        ("Main", source),
        (
            "Counter",
            "import QtQuick\nItem { property int count: 0 }\n",
        ),
        ("Plain", "import QtQuick\nItem { }\n"),
    ]);
    let file = &s.live()["Main"];
    reload_lint::lint(file, &|key| s.live().get(key).map(|f| &**f))
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn the_lint_asks_for_ids_on_stateful_siblings_of_one_type() {
    let out = lint("import QtQuick\nItem {\n  TextInput { }\n  TextInput { }\n}\n");
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(
        out[0].starts_with("3:3: warning: 2 TextInput objects"),
        "{out:?}"
    );
}

#[test]
fn the_lint_is_quiet_about_ids_names_single_objects_and_stateless_types() {
    for source in [
        "import QtQuick\nItem { TextInput { id: a } TextInput { id: b } }\n",
        "import QtQuick\nItem { TextInput { objectName: \"a\" } TextInput { id: b } }\n",
        "import QtQuick\nItem { TextInput { } Rectangle { } Rectangle { } }\n",
        "import QtQuick\nItem { Plain { } Plain { } }\n",
    ] {
        assert!(lint(source).is_empty(), "{source}");
    }
}

#[test]
fn the_lint_sees_state_in_declared_properties_and_through_component_files() {
    let out = lint("import QtQuick\nItem { Counter { } Counter { } }\n");
    assert_eq!(out.len(), 1, "{out:?}");
    let out = lint("import QtQuick\nItem { Item { property int n } Item { } }\n");
    assert_eq!(out.len(), 1, "{out:?}");
}

#[test]
fn a_session_warns_with_the_lint() {
    let s = session(&[(
        "Main",
        "import QtQuick\nItem {\n  Timer { }\n  Timer { }\n}\n",
    )]);
    let w: Vec<&CompileFault> = s.warnings().collect();
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!((w[0].file.as_str(), w[0].line), ("Main.qml", 3));
}

#[test]
fn a_session_over_a_directory_reads_it_and_reads_it_again() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Main.qml"), "import QtQuick\nItem { }\n").unwrap();
    let mut s = ReloadSession::open(dir.path(), QtTypeOracle).unwrap();
    s.start();
    assert_eq!(s.files().collect::<Vec<_>>(), ["Main"]);
    assert_eq!(
        s.key_of(&dir.path().join("sub/Thing.qml")).as_deref(),
        Some("sub/Thing")
    );
    assert_eq!(s.key_of(&dir.path().join("notes.txt")), None);

    std::fs::write(dir.path().join("Badge.qml"), "import QtQuick\nItem { }\n").unwrap();
    s.rescan().unwrap();
    let p = s.build().unwrap().unwrap();
    assert_eq!(
        p.ops,
        [ReloadOp::FileAdded {
            file: "Badge".into()
        }]
    );
}
