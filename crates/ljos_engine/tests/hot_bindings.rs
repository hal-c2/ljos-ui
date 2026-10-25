//! Which bindings and handlers ran, how often, for how long and what set
//! them off (`Engine::profile`), and the binding loops the graph broke
//! (`Engine::binding_loops`).

mod support;

use ljos_engine::{HotKind, Value};
use support::engine;

const SRC: &str = r#"
import QtQuick
Item {
  objectName: "root"
  property int base: 1
  property int other: 5
  property int doubled: base * 2
  property int sum: doubled + other
  property int told: 0
  onBaseChanged: told += 1
}
"#;

#[test]
fn nothing_is_profiled_until_asked() {
    let h = engine();
    let root = h.load(SRC);
    h.set(root, "base", Value::Int(2));
    assert_eq!(h.num(root, "sum"), 9.0);
    let p = h.engine.profile();
    assert!(p.since.is_none());
    assert!(p.entries.is_empty());
    assert!(p.runs.is_empty());
}

#[test]
fn a_binding_says_its_runs_where_it_is_written_and_what_set_it_off() {
    let h = engine();
    let root = h.load(SRC);
    assert_eq!(h.num(root, "sum"), 7.0);
    h.engine.set_profiled(true);
    for v in 2..5 {
        h.set(root, "base", Value::Int(v));
        assert_eq!(h.num(root, "sum"), f64::from(v) * 2.0 + 5.0);
    }
    h.set(root, "other", Value::Int(6));
    assert_eq!(h.num(root, "sum"), 14.0);
    let p = h.engine.profile();
    assert!(p.since.is_some());
    let sum = p.entries.iter().find(|e| e.name == "sum").expect("sum ran");
    assert_eq!(sum.kind, HotKind::Binding);
    assert_eq!(sum.object, root);
    assert_eq!(sum.runs, 4);
    assert_eq!(sum.at.as_deref(), Some("file:///Main.qml:8:3"));
    assert!(sum.total >= sum.own);
    // The first run profiled has no reads of an earlier one to compare
    // with; two were set off by `doubled`, the last by `other`.
    let causes: Vec<(&str, u64)> = sum
        .causes
        .iter()
        .map(|c| (c.name.as_str(), c.runs))
        .collect();
    assert_eq!(causes, [("doubled", 2), ("other", 1)]);
    let doubled = p.entries.iter().find(|e| e.name == "doubled").unwrap();
    assert_eq!(doubled.causes[0].name, "base");
    // A trace file gets every run.
    let ran = |name: &str| {
        let i = p.entries.iter().position(|e| e.name == name).unwrap();
        p.runs.iter().filter(|r| r.entry == i).count() as u64
    };
    assert_eq!(ran("sum"), 4);
}

#[test]
fn a_handler_says_its_runs_and_its_signal() {
    let h = engine();
    let root = h.load(SRC);
    h.engine.set_profiled(true);
    h.set(root, "base", Value::Int(2));
    h.set(root, "base", Value::Int(3));
    assert_eq!(h.num(root, "told"), 2.0);
    let p = h.engine.profile();
    let told = p
        .entries
        .iter()
        .find(|e| e.name == "onBaseChanged")
        .expect("the handler ran");
    assert_eq!(told.kind, HotKind::Handler);
    assert_eq!(told.runs, 2);
    assert_eq!(told.at.as_deref(), Some("file:///Main.qml:10:3"));
    assert_eq!(told.causes[0].name, "baseChanged");
    assert_eq!(told.causes[0].runs, 2);
}

#[test]
fn turning_it_off_forgets_and_on_again_starts_afresh() {
    let h = engine();
    let root = h.load(SRC);
    h.engine.set_profiled(true);
    h.set(root, "base", Value::Int(2));
    assert_eq!(h.num(root, "sum"), 9.0);
    h.engine.set_profiled(false);
    assert!(h.engine.profile().entries.is_empty());
    h.set(root, "base", Value::Int(3));
    assert_eq!(h.num(root, "sum"), 11.0);
    h.engine.set_profiled(true);
    assert!(h.engine.profile().entries.is_empty());
}

#[test]
fn a_binding_loop_is_kept_with_its_cycle_whether_or_not_profiled() {
    let h = engine();
    let root = h.load(
        r#"
import QtQuick
Item {
  property int p: q + 1
  property int q: p + 1
}
"#,
    );
    assert_eq!(h.num(root, "p"), 2.0);
    let loops = h.engine.binding_loops();
    assert_eq!(loops.len(), 1, "{loops:?}");
    let l = &loops[0];
    assert_eq!((l.type_name.as_str(), l.property.as_str()), ("Main", "p"));
    assert_eq!(l.at.as_deref(), Some("file:///Main.qml:4:3"));
    let cycle: Vec<&str> = l.cycle.iter().map(|(_, _, p)| p.as_str()).collect();
    assert!(cycle.contains(&"q"), "{cycle:?}");
    assert_eq!(l.times, 1);
}
