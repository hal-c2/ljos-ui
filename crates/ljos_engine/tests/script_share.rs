//! The share of the script heap's room one dispatch may take, as an
//! application gets it: from `Engine::new`, with no setting of its own.

mod support;

use ljos_engine::{
    MetaBuilder, MetaRef, PropertyFlags, QmlType, QtObject, TypeTag, Value, fault_kind,
};
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

/// A type of `WIDE` properties, so that making what a script needs to meet
/// one of its objects is most of a small room.
fn wide() -> MetaBuilder {
    const WIDE: usize = 4000;
    (0..WIDE).fold(
        MetaBuilder::new("Wide").base(QtObject::static_meta()),
        |b, i| b.property(&format!("p{i}"), TypeTag::Var, PropertyFlags::empty(), true),
    )
}

/// An engine whose script is about to read its first object of `meta`: the
/// next 10 ms meet it.
fn about_to_meet(meta: MetaRef) -> (Harness, ljos_engine::ObjectId) {
    let h = engine();
    let root = h.load(
        r#"import QtQuick
Item {
  id: page
  property var target: null
  property int met: 0
  Timer {
    interval: 10; running: true; repeat: true
    onTriggered: if (page.target) { var first = page.target.p0; page.met = 1 }
  }
}
"#,
    );
    // Once without it, so all else the handler needs is made already.
    h.pass_ms(10);
    let target = h.engine.create(meta, Some(root)).unwrap();
    h.set(root, "target", Value::Object(target));
    h.engine.collect_garbage();
    (h, root)
}

/// Meets an object of `make`'s type with a quarter more room than meeting
/// one keeps: enough, and more than a dispatch's share of it.
fn meet_with_little_room(make: impl Fn() -> MetaRef) -> (Harness, f64) {
    let (h, root) = about_to_meet(make());
    let before = h.engine.script_heap_bytes().unwrap();
    h.pass_ms(10);
    assert_eq!(h.num(root, "met"), 1.0, "not met with room to spare");
    h.engine.collect_garbage();
    let kept = h.engine.script_heap_bytes().unwrap() - before;

    let (h, root) = about_to_meet(make());
    let limit = h.engine.script_heap_bytes().unwrap() + kept / 4 * 5;
    assert!(h.engine.set_script_heap_limit(Some(limit)));
    h.pass_ms(10);
    let met = h.num(root, "met");
    (h, met)
}

/// Registering a type built into the program is the engine's, made once,
/// and outside the share of the dispatch that first meets it.
#[test]
fn first_meeting_a_wide_built_in_type_is_outside_the_share() {
    let (h, met) = meet_with_little_room(|| MetaRef::Static(Box::leak(Box::new(wide().build()))));
    assert_eq!(met, 1.0, "{:?}", h.engine.faults());
    assert!(!ran_out(&h), "{:?}", h.engine.faults());
}

/// A script can cause types like one QML declares without end, so making
/// one is the dispatch's.
#[test]
fn first_meeting_as_wide_a_declared_type_is_within_the_share() {
    let (h, met) = meet_with_little_room(|| wide().build_shared());
    assert_eq!(met, 0.0, "registered past the share");
    assert!(ran_out(&h), "{:?}", h.engine.faults());
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
