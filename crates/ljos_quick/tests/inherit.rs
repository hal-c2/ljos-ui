//! What an item inherits (`visible`, `enabled`) is passed down by its
//! parent's change hooks and by a reparent; the stack each item keeps for
//! it stays whole however the tree is made and moved.
//! `features/items/inherited-at-once.feature` holds the behaviour checked
//! against Qt.

mod common;

use common::*;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::Item;

fn warnings(e: &Engine) -> Vec<String> {
    e.take_log()
        .into_iter()
        .filter(|l| l.level >= ljos_engine::LogLevel::Warning)
        .map(|l| l.message)
        .collect()
}

fn shown(e: &Engine, o: ObjectId) -> bool {
    get(e, o, "visible").to_boolean()
}

const OVERLAY: &str = r#"
import QtQuick
Item {
    width: 100; height: 100
    function hideShow() { overlay.visible = false; overlay.visible = true }
    function intoOverlay() { pop.parent = overlay; overlay.visible = true; pop.visible = true }
    function outOf() { overlay.visible = false; pop.parent = overlay.parent }
    function disable() { overlay.enabled = false; pop.parent = overlay; overlay.enabled = true }
    Item {
        id: overlay; objectName: "overlay"; visible: false
        Item { objectName: "row"; Item { objectName: "label" } }
    }
    Item { id: pop; objectName: "pop"; visible: false; Item { objectName: "content" } }
}
"#;

#[test]
fn an_item_shown_again_and_one_moved_under_an_overlay_keep_their_stacks() {
    let e = engine();
    let root = load(&e, OVERLAY);
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    assert_eq!(warnings(&e), Vec::<String>::new());
    let [row, label, pop, content] = ["row", "label", "pop", "content"].map(|n| find(&e, root, n));
    for (m, want) in [
        ("hideShow", [true, true, false, false]),
        ("intoOverlay", [true, true, true, true]),
        ("outOf", [false, false, true, true]),
        ("intoOverlay", [true, true, true, true]),
        ("hideShow", [true, true, true, true]),
        ("disable", [true, true, true, true]),
    ] {
        e.invoke_by_name(root, m, &[]).unwrap();
        frame(&mut s, &e);
        assert_eq!(warnings(&e), Vec::<String>::new(), "{m}");
        assert_eq!(
            [row, label, pop, content].map(|o| shown(&e, o)),
            want,
            "{m}"
        );
    }
    assert!(get(&e, content, "enabled").to_boolean());
}

#[test]
fn an_item_adopted_before_its_parent_is_shown_takes_what_the_parent_passes() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    // Made loose, with a child, then put under a hidden parent that is
    // itself not in the scene yet.
    let panel = make::<Item>(&e, None);
    set(&e, panel, "visible", false.into());
    let loose = make::<Item>(&e, None);
    let inner = make::<Item>(&e, Some(loose));
    set(&e, loose, "parent", ljos_engine::Value::Object(panel));
    set(&e, panel, "parent", ljos_engine::Value::Object(root));
    frame(&mut s, &e);
    assert_eq!(warnings(&e), Vec::<String>::new());
    assert!(!shown(&e, loose) && !shown(&e, inner));
    set(&e, panel, "visible", true.into());
    frame(&mut s, &e);
    assert_eq!(warnings(&e), Vec::<String>::new());
    assert!(shown(&e, loose) && shown(&e, inner));
    set(&e, panel, "visible", false.into());
    set(&e, loose, "parent", ljos_engine::Value::Object(root));
    frame(&mut s, &e);
    assert_eq!(warnings(&e), Vec::<String>::new());
    assert!(shown(&e, loose) && shown(&e, inner));
}

// A child that shows itself each time it reads hidden: on Ljós each
// write of `true` through the cover is seen as a change, and the cover
// put back as another, so the pass never settles (Qt returns, the child
// told once). Bounded, it stops with a fault that names the child and
// the property, and the write returns.
const SHOWS_ITSELF: &str = r#"
import QtQuick
Item {
    width: 100; height: 100
    Item {
        objectName: "parent"
        Item {
            objectName: "child"
            onVisibleChanged: visible = true
        }
    }
}
"#;

#[test]
fn a_pass_that_never_settles_stops_with_a_fault_that_names_the_item() {
    let e = engine();
    let root = load(&e, SHOWS_ITSELF);
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    assert_eq!(warnings(&e), Vec::<String>::new());
    let [parent, child] = ["parent", "child"].map(|n| find(&e, root, n));
    set(&e, parent, "visible", false.into());
    frame(&mut s, &e);
    let loops: Vec<_> = e
        .faults()
        .into_iter()
        .filter(|f| f.fault.to_string().contains("Inheritance loop"))
        .collect();
    assert!(!loops.is_empty(), "{:?}", warnings(&e));
    assert!(loops.iter().all(|f| f.origin == Some(child)));
    let said = warnings(&e);
    let want = "file:///Main.qml:7:9: internal: QML Item: Inheritance loop detected for property \"visible\"";
    assert!(said.iter().any(|w| w.ends_with(want)), "{said:?}");
}
