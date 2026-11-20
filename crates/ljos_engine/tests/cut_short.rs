//! A binding that a passing fault stops before it has read all it depends
//! on (its time up, a full heap) still wakes when the rest changes, so it
//! catches up once the fault has passed.

mod support;

use std::time::Duration;

use ljos_engine::Value;
use ljos_runtime::Fault;
use support::engine;

// `gate` is a plain JS object: what it holds is read without a dependency,
// so only `a` and `b` wake `sum`.
const PAGE: &str = r#"import QtQuick
Item {
  property int a: 1
  property int b: 2
  property var gate: ({ spin: false })
  property int sum: { var x = a; if (gate.spin) { for (;;) {} } return x + b }
  property int seen: sum
  function spin(on) { gate.spin = on }
}
"#;

#[test]
fn a_binding_interrupted_before_it_read_b_runs_again_when_b_changes() {
    let h = engine();
    let e = &h.engine;
    let root = h.load(PAGE);
    e.set_js_dispatch_budget(Duration::from_millis(5), Duration::from_millis(20))
        .unwrap();
    assert_eq!(h.num(root, "seen"), 3.0);

    e.invoke_by_name(root, "spin", &[Value::Bool(true)])
        .unwrap();
    h.set(root, "a", Value::Int(10));
    assert_eq!(h.num(root, "seen"), 3.0, "interrupted, the value is kept");
    assert!(
        e.faults()
            .iter()
            .any(|r| matches!(r.fault, Fault::JsInterrupted { .. })),
        "the interrupt is reported"
    );

    e.invoke_by_name(root, "spin", &[Value::Bool(false)])
        .unwrap();
    h.set(root, "b", Value::Int(20));
    assert_eq!(
        h.num(root, "seen"),
        30.0,
        "b, which the interrupted run never read, still wakes it"
    );
}
