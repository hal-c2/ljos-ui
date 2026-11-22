//! The share of the script heap's room one dispatch may take, as an
//! application gets it: from `Engine::new`, with no setting of its own.

mod support;

use ljos_engine::fault_kind;
use support::{Harness, engine};

/// A dispatch that asks for `ask` bytes at once and one after it that asks
/// for a KiB.
fn load(h: &Harness, ask: usize) -> ljos_engine::ObjectId {
    h.load(&format!(
        r#"import QtQuick
Item {{
  property int took: 0
  property int after: 0
  Timer {{
    interval: 10; running: true; repeat: true
    onTriggered: took = new ArrayBuffer({ask}).byteLength
  }}
  Timer {{
    interval: 15; running: true; repeat: true
    onTriggered: after = new ArrayBuffer(1024).byteLength
  }}
}}
"#
    ))
}

fn ran_out(h: &Harness) -> bool {
    h.engine
        .faults()
        .iter()
        .any(|f| fault_kind(&f.fault) == "JsOutOfMemory")
}

/// The limit and the share are both the defaults. One buffer of 85 % of
/// the limit fits under it, and is more than one dispatch may take.
#[test]
fn by_default_a_dispatch_past_its_share_is_refused_and_the_next_runs() {
    let h = engine();
    let ask = ljos_js::default_memory_limit() / 100 * 85;
    let root = load(&h, ask);
    h.pass_ms(15);

    assert_eq!(
        h.num(root, "took"),
        0.0,
        "85 % of the limit was not refused"
    );
    assert!(ran_out(&h), "{:?}", h.engine.faults());
    assert_eq!(
        h.num(root, "after"),
        1024.0,
        "the next dispatch did not run"
    );
}

/// An application can't set the share: whatever limit it puts keeps it.
/// With no limit there is no room to have a share of.
#[test]
fn a_limit_an_application_sets_keeps_the_share_and_no_limit_has_none() {
    let h = engine();
    let ask = 27 << 20;
    let root = load(&h, ask);
    assert!(h.engine.set_script_heap_limit(Some(32 << 20)));
    h.pass_ms(15);
    assert_eq!(h.num(root, "took"), 0.0, "27 of 32 MiB was not refused");
    assert!(ran_out(&h), "{:?}", h.engine.faults());

    assert!(h.engine.set_script_heap_limit(None));
    h.pass_ms(15);
    assert_eq!(h.num(root, "took"), ask as f64);
}
