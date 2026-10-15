//! What a `.js` import keeps lives as long as the instance that imported
//! it, and no longer.

mod common;

use common::*;
use ljos_compiler::{Project, compile_file};

fn compile_with_script(e: &ljos_engine::Engine, files: &[(&str, &str)], script: (&str, &str)) {
    let mut project = Project::new(qml_front::QtTypeOracle);
    for (k, s) in files {
        project.add(k, *s);
    }
    project.add_script(script.0, script.1);
    let mut source = ljos_engine::MapSource::new();
    for (k, _) in files {
        let c = compile_file(&project, k).unwrap_or_else(|d| panic!("{k}: {d:?}"));
        assert!(!c.has_errors(), "{k}: {:?}", c.diagnostics);
        source.insert(*k, c.file);
    }
    e.set_component_source(source);
}

#[test]
fn a_destroyed_instance_takes_its_script_objects_with_it() {
    let e = engine();
    compile_with_script(
        &e,
        &[
            (
                "Foo",
                "import QtQuick\nimport \"count.js\" as Count\nItem { property int n: Count.bump() }",
            ),
            ("Main", "import QtQuick\nItem { }"),
        ],
        ("count.js", "var n = 0\nfunction bump() { n++; return n }"),
    );
    assert_eq!(e.script_objects_held(), 0);
    for _ in 0..50 {
        let foo = e.create_from_url("Foo", None, Vec::new()).unwrap();
        // One copy for the instance, and its own count.
        assert_eq!(num(&e, foo, "n"), 1.0);
        assert_eq!(e.script_objects_held(), 1);
        assert!(e.destroy(foo));
        assert_eq!(e.script_objects_held(), 1, "destruction is deferred");
        e.process_deferred_deletes();
        assert_eq!(e.script_objects_held(), 0);
    }
}

#[test]
fn an_edited_library_replaces_the_old_object_instead_of_adding_one() {
    let e = engine();
    let foo = "import QtQuick\nimport \"lib.js\" as Lib\nItem { property int v: Lib.v }";
    compile_with_script(
        &e,
        &[("Foo", foo)],
        ("lib.js", ".pragma library\nvar v = 1"),
    );
    let a = e.create_from_url("Foo", None, Vec::new()).unwrap();
    assert_eq!(num(&e, a, "v"), 1.0);
    assert_eq!(e.script_objects_held(), 1);
    compile_with_script(
        &e,
        &[("Foo", foo)],
        ("lib.js", ".pragma library\nvar v = 2"),
    );
    let b = e.create_from_url("Foo", None, Vec::new()).unwrap();
    assert_eq!(num(&e, b, "v"), 2.0);
    assert_eq!(
        e.script_objects_held(),
        1,
        "the old library object is dropped"
    );
}
