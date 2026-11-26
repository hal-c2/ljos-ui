//! A Repeater over a native list of records, compiled ahead of time: each
//! delegate's required `modelData` is its record (issue #773).

use ljos::prelude::*;

fn suggestions() -> Value {
    Value::List(std::sync::Arc::new(
        ["a", "b", "c"]
            .iter()
            .map(|t| Value::map([("kind", Value::from("server")), ("title", Value::from(*t))]))
            .collect(),
    ))
}

fn titles(engine: &Engine, root: ObjectId) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if &*engine.read_by_name(o, "objectName").to_js_string() == "rowTitle" {
            out.push(engine.read_by_name(o, "text").to_js_string().to_string());
        }
        stack.extend(engine.children(o));
    }
    out.sort();
    out
}

/// `release`: as a release build runs, without the cross-checks that run
/// every compiled evaluation dynamically too and keep the dynamic result.
fn check(aot: bool, release: bool) {
    let (engine, root) = ljos_facade_aot::app().root("Offered").build().unwrap();
    engine.set_aot(aot);
    engine.set_aot_check(!release);
    engine.set_tier1_check(!release);
    let source = engine.read_by_name(root, "source").as_object().unwrap();
    engine
        .write_by_name(source, "suggestions", suggestions())
        .unwrap();
    assert_eq!(
        titles(&engine, root),
        ["a", "b", "c"],
        "aot {aot}, release {release}"
    );
}

#[test]
fn as_a_release_build_runs_the_rows_read_their_records() {
    check(true, true);
}

#[test]
fn with_the_cross_checks_the_rows_read_their_records() {
    check(true, false);
}

#[test]
fn without_compiled_code_as_a_release_build_runs_the_rows_read_their_records() {
    check(false, true);
}
