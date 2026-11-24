//! What an item inherits (`visible`, `enabled`) is passed down by its
//! parent's change hooks and by a reparent; the stack each item keeps for
//! it stays whole however the tree is made and moved.
//! `features/items/inherited-*.feature` hold the behaviour checked
//! against Qt.

mod common;

use common::*;
use ljos_engine::{Engine, ObjectId, WriteKind};
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

// A child that shows itself each time it reads hidden: the write goes
// under the cover as its own value, so it reads false, is told once and
// shows again with its parent, as on Qt.
const SHOWS_ITSELF: &str = r#"
import QtQuick
Item {
    width: 100; height: 100
    property int told: 0
    Item {
        objectName: "parent"
        Item {
            objectName: "child"
            onVisibleChanged: { told++; visible = true }
        }
    }
}
"#;

#[test]
fn a_child_that_shows_itself_under_a_hidden_parent_keeps_it_as_its_own_value() {
    let e = engine();
    let root = load(&e, SHOWS_ITSELF);
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    let [parent, child] = ["parent", "child"].map(|n| find(&e, root, n));
    set(&e, parent, "visible", false.into());
    frame(&mut s, &e);
    assert_eq!(warnings(&e), Vec::<String>::new());
    assert!(!shown(&e, child));
    assert_eq!(get(&e, root, "told").to_number(), 1.0);
    set(&e, parent, "visible", true.into());
    frame(&mut s, &e);
    assert!(shown(&e, child));
    assert_eq!(get(&e, root, "told").to_number(), 2.0);
}

// What writes `true` through the cover as the toolkit does, each time the
// child reads hidden, is a new own value each time and the cover put back
// another change, so the pass never settles. Bounded, it stops with a
// fault that names the child and the property, and the write returns.
const CHILD: &str = r#"
import QtQuick
Item {
    width: 100; height: 100
    Item {
        objectName: "parent"
        Item { objectName: "child" }
    }
}
"#;

#[test]
fn a_pass_that_never_settles_stops_with_a_fault_that_names_the_item() {
    let e = engine();
    let root = load(&e, CHILD);
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    assert_eq!(warnings(&e), Vec::<String>::new());
    let [parent, child] = ["parent", "child"].map(|n| find(&e, root, n));
    let visible = Item::VISIBLE.index;
    let _shows = e
        .effect(move |e| {
            if !e.read(child, visible).to_boolean() {
                let _ = e.write_internal(child, visible, true.into(), WriteKind::Host);
            }
        })
        .unwrap();
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

/// Many siblings whose `enabled` and `visible` bind to one value of the
/// page, moved by native code in a batch with other writes, as an app's
/// model publishes: `features/items/inherited-bound-on-many-siblings.feature`
/// holds the behaviour from QML.
const SIBLINGS: &str = r#"
import QtQuick
Item {
    id: root
    width: 100; height: 100
    property string chosen: "a"
    property int tick: 0
    readonly property string target: chosen === "" ? "" : chosen + "!"
    property int enabledChanges: 0
    property int visibleChanges: 0
    Component { id: tile
        Item {
            enabled: root.target !== ""
            visible: root.target !== "none!"
            onEnabledChanged: root.enabledChanges++
            onVisibleChanged: root.visibleChanges++
            Item { property bool lit: enabled; Item { property bool shown: visible } }
        }
    }
    Component.onCompleted: { for (let i = 0; i < 600; i++) tile.createObject(root) }
}
"#;

/// Every tile's child reads `lit` from its own `enabled` and its grandchild
/// `shown` from its own `visible`, so a pass that stopped at the tile would
/// leave them at the old value.
#[track_caller]
fn descendants_hold(e: &Engine, tiles: &[ObjectId], lit: bool, shown: bool) {
    for &t in tiles {
        let [child] = e.visual_children(t)[..] else {
            panic!("a tile has one child")
        };
        let [grandchild] = e.visual_children(child)[..] else {
            panic!("a tile's child has one child")
        };
        assert_eq!(get(e, child, "lit").to_boolean(), lit, "lit");
        assert_eq!(get(e, grandchild, "shown").to_boolean(), shown, "shown");
    }
}

#[test]
fn siblings_bound_to_one_value_are_told_side_by_side_not_one_inside_the_last() {
    let e = engine();
    let root = load(&e, SIBLINGS);
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    assert_eq!(warnings(&e), Vec::<String>::new());
    let tiles = e.visual_children(root);
    assert_eq!(tiles.len(), 600);
    let all = |p: &str| tiles.iter().all(|&t| get(&e, t, p).to_boolean());
    let none = |p: &str| tiles.iter().all(|&t| !get(&e, t, p).to_boolean());
    let mut tick = 0;
    let mut publish = |chosen: &str| {
        tick += 1;
        e.batch(|| {
            set(&e, root, "chosen", chosen.into());
            set(&e, root, "tick", f64::from(tick).into());
        });
    };
    let counts = || {
        (
            num(&e, root, "enabledChanges"),
            num(&e, root, "visibleChanges"),
        )
    };

    publish("b");
    assert_eq!(
        warnings(&e),
        Vec::<String>::new(),
        "a move that keeps them on"
    );
    assert_eq!(counts(), (0.0, 0.0), "no item is told a result that stayed");
    assert!(all("enabled") && all("visible"));
    descendants_hold(&e, &tiles, true, true);

    publish("");
    assert_eq!(
        warnings(&e),
        Vec::<String>::new(),
        "a move that disables them"
    );
    assert_eq!(counts(), (600.0, 0.0));
    assert!(none("enabled") && all("visible"));
    descendants_hold(&e, &tiles, false, true);

    publish("c");
    assert_eq!(warnings(&e), Vec::<String>::new(), "enabled again");
    assert_eq!(counts(), (1200.0, 0.0));
    assert!(all("enabled"));
    descendants_hold(&e, &tiles, true, true);

    publish("none");
    assert_eq!(warnings(&e), Vec::<String>::new(), "a move that hides them");
    assert_eq!(counts(), (1200.0, 600.0));
    assert!(none("visible") && all("enabled"));
    descendants_hold(&e, &tiles, true, false);

    publish("d");
    assert_eq!(warnings(&e), Vec::<String>::new(), "shown again");
    assert_eq!(counts(), (1200.0, 1200.0));
    assert!(all("visible"));
    descendants_hold(&e, &tiles, true, true);
    frame(&mut s, &e);
    assert_eq!(warnings(&e), Vec::<String>::new());
}

/// [`siblings_bound_to_one_value_are_told_side_by_side_not_one_inside_the_last`]
/// with each move a write of its own, as a script makes it.
#[test]
fn siblings_bound_to_one_value_moved_by_a_plain_write_are_told_side_by_side() {
    let e = engine();
    let root = load(&e, SIBLINGS);
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    let tiles = e.visual_children(root);
    let counts = || {
        (
            num(&e, root, "enabledChanges"),
            num(&e, root, "visibleChanges"),
        )
    };
    for (chosen, want, on) in [
        ("b", (0.0, 0.0), true),
        ("", (600.0, 0.0), false),
        ("c", (1200.0, 0.0), true),
    ] {
        set(&e, root, "chosen", chosen.into());
        assert_eq!(warnings(&e), Vec::<String>::new(), "{chosen:?}");
        assert_eq!(counts(), want, "{chosen:?}");
        assert!(
            tiles
                .iter()
                .all(|&t| get(&e, t, "enabled").to_boolean() == on)
        );
        descendants_hold(&e, &tiles, on, true);
    }
}
