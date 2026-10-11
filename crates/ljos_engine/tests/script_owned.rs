//! An incubator belongs to the script that holds it: it goes when the
//! script lets go, and not before (script_owned.rs, incubate.rs).

mod support;

use ljos_engine::{ObjectId, Value};
use ljos_runtime::JobPoll;
use support::*;

const SOURCE: &str = r#"import QtQuick
Item {
    id: root
    property var inc: null
    property var kept: []
    property string seen: ""
    property int made: 0
    Component { id: comp; Item { objectName: "kid"; Component.onCompleted: root.made += 1 } }
    function unheld(mode) { comp.incubateObject(root, {}, mode) }
    function inProperty(mode) { inc = comp.incubateObject(root, {}, mode) }
    function inArray(mode) { kept.push(comp.incubateObject(root, {}, mode)) }
    function closed(mode) {
        var i = comp.incubateObject(root, {}, mode)
        i.onStatusChanged = function(s) { root.seen += s + ":" + (i.object !== null) + " " }
    }
    function drop() { inc = null; kept = [] }
    function look() { return inc.status + "|" + (inc.object !== null) + "|" + (inc === inc) }
    function lookKept() { return kept[0].status + "|" + (kept[0].object !== null) }
}
"#;

const SYNCHRONOUS: f64 = 1.0;
const ASYNCHRONOUS: f64 = 0.0;

fn call(h: &Harness, root: ObjectId, f: &str, mode: f64) {
    h.engine
        .invoke_by_name(root, f, &[Value::from(mode)])
        .unwrap();
}

/// Runs the queued incubations to their end, a step at a time.
fn incubate(h: &Harness) {
    for mut job in h.engine.take_incubations() {
        let mut steps = 0;
        while matches!(job.step(&|| true), JobPoll::Yield) {
            steps += 1;
            assert!(steps < 1000, "the job never ends");
        }
    }
}

/// Collects, and destroys what the collection let go of. Before the
/// first script ran there is nothing to collect.
fn collect(h: &Harness) {
    h.engine.collect_garbage();
    h.engine.process_deferred_deletes();
}

fn text(h: &Harness, root: ObjectId, f: &str) -> String {
    h.engine
        .invoke_by_name(root, f, &[])
        .unwrap()
        .to_js_string()
        .to_string()
}

#[test]
fn an_incubator_nobody_kept_goes_and_its_object_stays() {
    for mode in [SYNCHRONOUS, ASYNCHRONOUS] {
        let h = engine();
        let root = h.load(SOURCE);
        collect(&h);
        let before = h.engine.object_count();
        for _ in 0..5 {
            call(&h, root, "unheld", mode);
        }
        incubate(&h);
        assert_eq!(h.num(root, "made"), 5.0, "mode {mode}");
        collect(&h);
        // The five objects are the root's; their incubators are nobody's.
        assert_eq!(h.engine.object_count(), before + 5, "mode {mode}");
        assert!(h.warnings().is_empty());
    }
}

#[test]
fn an_incubator_nobody_kept_still_makes_its_object() {
    let h = engine();
    let root = h.load(SOURCE);
    call(&h, root, "unheld", ASYNCHRONOUS);
    // Collected while loading: the engine holds it until it is done.
    collect(&h);
    assert_eq!(h.num(root, "made"), 0.0);
    incubate(&h);
    assert_eq!(h.num(root, "made"), 1.0);
    assert!(find(&h.engine, root, "kid").is_some());
}

#[test]
fn an_incubator_in_a_property_stays_until_the_property_lets_go() {
    for mode in [SYNCHRONOUS, ASYNCHRONOUS] {
        let h = engine();
        let root = h.load(SOURCE);
        collect(&h);
        let before = h.engine.object_count();
        call(&h, root, "inProperty", mode);
        incubate(&h);
        collect(&h);
        collect(&h);
        assert_eq!(h.engine.object_count(), before + 2, "mode {mode}");
        assert_eq!(text(&h, root, "look"), "1|true|true", "mode {mode}");
        call(&h, root, "drop", mode);
        collect(&h);
        assert_eq!(h.engine.object_count(), before + 1, "mode {mode}");
    }
}

#[test]
fn an_incubator_in_an_array_stays_until_the_array_goes() {
    let h = engine();
    let root = h.load(SOURCE);
    collect(&h);
    let before = h.engine.object_count();
    call(&h, root, "inArray", ASYNCHRONOUS);
    incubate(&h);
    collect(&h);
    assert_eq!(h.engine.object_count(), before + 2);
    assert_eq!(text(&h, root, "lookKept"), "1|true");
    call(&h, root, "drop", ASYNCHRONOUS);
    collect(&h);
    assert_eq!(h.engine.object_count(), before + 1);
}

#[test]
fn a_function_that_closes_over_its_incubator_is_called_and_collected_with_it() {
    let h = engine();
    let root = h.load(SOURCE);
    collect(&h);
    let before = h.engine.object_count();
    for _ in 0..3 {
        call(&h, root, "closed", ASYNCHRONOUS);
    }
    collect(&h);
    incubate(&h);
    assert_eq!(
        h.get(root, "seen").to_js_string().to_string(),
        "1:true 1:true 1:true "
    );
    // The function and the incubator hold each other: only the cycle
    // collector lets them go.
    collect(&h);
    assert_eq!(h.engine.object_count(), before + 3);
    assert!(h.warnings().is_empty());
}

#[test]
fn a_property_given_one_incubator_after_another_holds_the_last_only() {
    let h = engine();
    let root = h.load(SOURCE);
    collect(&h);
    let before = h.engine.object_count();
    for _ in 0..100 {
        call(&h, root, "inProperty", SYNCHRONOUS);
    }
    // Without a collection: the ones let go went as the property was
    // given others, but for the few not yet looked for.
    h.engine.process_deferred_deletes();
    let lingering = h.engine.object_count() - before - 100;
    assert!((1..40).contains(&lingering), "{lingering} incubators");
    collect(&h);
    assert_eq!(h.engine.object_count(), before + 100 + 1);
    assert_eq!(text(&h, root, "look"), "1|true|true");
}
