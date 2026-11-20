//! Anchors. Expected numbers were observed on Qt 6.11.2 (qml6, offscreen)
//! with the same trees written in QML.

mod common;

use common::*;
use ljos_engine::Value;
use ljos_quick::{ANCHORS_CATEGORY, Item};

fn line(e: &ljos_engine::Engine, o: ljos_engine::ObjectId, edge: &str) -> Value {
    get(e, o, edge)
}

#[test]
fn fill_with_margins_and_one_side_override() {
    let e = engine();
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 201.0, 101.0);
    let c = make::<Item>(&e, Some(root));
    set(&e, c, "anchors.fill", Value::Object(root));
    set(&e, c, "anchors.margins", Value::Real(10.0));
    set(&e, c, "anchors.leftMargin", Value::Real(3.0));
    let _s = scene(&e, root, 300.0, 300.0);
    assert_eq!(rect(&e, c), (3.0, 10.0, 188.0, 81.0));
}

#[test]
fn right_and_bottom_follow_the_parent_eagerly() {
    let e = engine();
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 201.0, 101.0);
    let d = make::<Item>(&e, Some(root));
    geom(&e, d, 0.0, 0.0, 20.0, 20.0);
    set(&e, d, "anchors.right", line(&e, root, "right"));
    set(&e, d, "anchors.bottom", line(&e, root, "bottom"));
    set(&e, d, "anchors.margins", Value::Real(4.0));
    let _s = scene(&e, root, 300.0, 300.0);
    assert_eq!(rect(&e, d), (177.0, 77.0, 20.0, 20.0));
    // No frame in between: anchors are eager.
    e.set(root, Item::WIDTH, 300.0).unwrap();
    assert_eq!(num(&e, d, "x"), 276.0);
}

#[test]
fn center_in_rounds_the_centre_line_and_half_size() {
    let e = engine();
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 201.0, 101.0);
    let a = make::<Item>(&e, Some(root));
    geom(&e, a, 0.0, 0.0, 50.0, 30.0);
    set(&e, a, "anchors.centerIn", Value::Object(root));
    let _s = scene(&e, root, 300.0, 300.0);
    assert_eq!(rect(&e, a), (76.0, 36.0, 50.0, 30.0));
}

#[test]
fn align_when_centered_rounds_only_when_set() {
    let e = engine();
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 200.0, 100.0);
    let a = make::<Item>(&e, Some(root));
    geom(&e, a, 0.0, 0.0, 51.0, 31.0);
    set(&e, a, "anchors.centerIn", Value::Object(root));
    let b = make::<Item>(&e, Some(root));
    geom(&e, b, 0.0, 0.0, 51.0, 31.0);
    set(&e, b, "anchors.centerIn", Value::Object(root));
    set(&e, b, "anchors.alignWhenCentered", Value::Bool(false));
    let _s = scene(&e, root, 300.0, 300.0);
    assert_eq!(rect(&e, a), (74.0, 34.0, 51.0, 31.0));
    assert_eq!(rect(&e, b), (74.5, 34.5, 51.0, 31.0));
}

#[test]
fn sibling_edges_and_offsets() {
    let e = engine();
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 200.0, 100.0);
    let s = make::<Item>(&e, Some(root));
    geom(&e, s, 3.0, 0.0, 41.0, 10.0);
    let left_of = make::<Item>(&e, Some(root));
    geom(&e, left_of, 0.0, 0.0, 5.0, 5.0);
    set(&e, left_of, "anchors.left", line(&e, s, "right"));
    set(&e, left_of, "anchors.leftMargin", Value::Real(2.0));
    let centre = make::<Item>(&e, Some(root));
    geom(&e, centre, 0.0, 0.0, 10.0, 10.0);
    set(&e, centre, "anchors.left", line(&e, s, "horizontalCenter"));
    let offset = make::<Item>(&e, Some(root));
    geom(&e, offset, 0.0, 0.0, 10.0, 10.0);
    set(
        &e,
        offset,
        "anchors.horizontalCenter",
        line(&e, root, "horizontalCenter"),
    );
    set(
        &e,
        offset,
        "anchors.horizontalCenterOffset",
        Value::Real(3.0),
    );
    let _sc = scene(&e, root, 300.0, 300.0);
    assert_eq!(num(&e, left_of, "x"), 46.0);
    assert_eq!(num(&e, centre, "x"), 24.0);
    assert_eq!(num(&e, offset, "x"), 98.0);
    // Moving the sibling moves what is anchored to it.
    e.set(s, Item::X, 13.0).unwrap();
    assert_eq!(num(&e, left_of, "x"), 56.0);
}

#[test]
fn left_and_centre_stretch() {
    let e = engine();
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 200.0, 100.0);
    let lc = make::<Item>(&e, Some(root));
    geom(&e, lc, 0.0, 0.0, 10.0, 10.0);
    set(&e, lc, "anchors.left", line(&e, root, "left"));
    set(
        &e,
        lc,
        "anchors.horizontalCenter",
        line(&e, root, "horizontalCenter"),
    );
    let _s = scene(&e, root, 300.0, 300.0);
    assert_eq!(rect(&e, lc), (0.0, 0.0, 200.0, 10.0));
}

#[test]
fn three_horizontal_anchors_warn_and_keep_left_and_right() {
    let e = engine();
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 200.0, 100.0);
    let k = make::<Item>(&e, Some(root));
    geom(&e, k, 9.0, 0.0, 7.0, 0.0);
    set(&e, k, "anchors.left", line(&e, root, "left"));
    set(&e, k, "anchors.right", line(&e, root, "right"));
    set(
        &e,
        k,
        "anchors.horizontalCenter",
        line(&e, root, "horizontalCenter"),
    );
    let _s = scene(&e, root, 300.0, 300.0);
    assert_eq!(rect(&e, k), (0.0, 0.0, 200.0, 0.0));
    let log = e.take_log();
    assert_eq!(log.len(), 1, "{log:?}");
    assert_eq!(log[0].category, ANCHORS_CATEGORY);
    assert_eq!(
        log[0].message,
        "QML Item: Cannot specify left, right, and horizontalCenter anchors at the same time."
    );
}

#[test]
fn anchoring_to_a_non_sibling_warns() {
    let e = engine();
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 200.0, 100.0);
    let a = make::<Item>(&e, Some(root));
    let inner = make::<Item>(&e, Some(a));
    let b = make::<Item>(&e, Some(root));
    set(&e, b, "anchors.left", line(&e, inner, "right"));
    let _s = scene(&e, root, 300.0, 300.0);
    let log = e.take_log();
    assert!(
        log.iter().any(|l| l
            .message
            .ends_with("Cannot anchor to an item that isn't a parent or sibling.")),
        "{log:?}"
    );
}

#[test]
fn removing_an_anchor_keeps_the_geometry() {
    let e = engine();
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 200.0, 100.0);
    let s = make::<Item>(&e, Some(root));
    geom(&e, s, 0.5, 0.0, 41.0, 10.0);
    let h = make::<Item>(&e, Some(root));
    geom(&e, h, 3.0, 0.0, 10.0, 10.0);
    set(&e, h, "anchors.left", line(&e, s, "right"));
    let _sc = scene(&e, root, 300.0, 300.0);
    assert_eq!(num(&e, h, "x"), 41.5);
    set(&e, h, "anchors.left", Value::Undefined);
    assert_eq!(num(&e, h, "x"), 41.5);
    // And it is free again.
    e.set(s, Item::X, 100.0).unwrap();
    assert_eq!(num(&e, h, "x"), 41.5);
    e.set(h, Item::X, 7.0).unwrap();
    assert_eq!(num(&e, h, "x"), 7.0);
}

#[test]
fn anchors_hold_before_any_scene_shows_the_item() {
    // Qt applies anchors as an item completes, so what runs as its
    // component completes, and a tree no window shows, reads the geometry
    // they give (features/items/anchors-on-completion.feature).
    let e = engine();
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 200.0, 100.0);
    let c = make::<Item>(&e, Some(root));
    set(&e, c, "anchors.fill", Value::Object(root));
    assert_eq!(num(&e, c, "width"), 200.0);
    let _s = scene(&e, root, 300.0, 300.0);
    assert_eq!(num(&e, c, "width"), 200.0);
    geom(&e, root, 0.0, 0.0, 120.0, 100.0);
    assert_eq!(num(&e, c, "width"), 120.0);
}

#[test]
fn a_line_read_before_its_item_is_complete_follows_it() {
    let e = engine();
    let root = make::<Item>(&e, None);
    // Made, not complete: its lines are not there yet.
    let early = e
        .create_native_incomplete(Item::default(), Some(root))
        .unwrap();
    let known = std::rc::Rc::new(std::cell::Cell::new(false));
    let k = known.clone();
    let left = e.meta_of(early).unwrap().property_index("left").unwrap();
    let _effect = e
        .effect(move |e| k.set(!e.read(early, left).is_nullish()))
        .unwrap();
    assert!(!known.get());
    e.complete(early).unwrap();
    assert!(known.get(), "the reader is told when the line is there");
}

/// A side margin taken back holds what a new item's does: while
/// `anchors.margins` was never given, its default with nothing bound
/// (what a hot reload compares with a fresh load), and once given, the
/// margins; it follows a later `anchors.margins` either way.
#[test]
fn a_side_margin_taken_back_is_as_a_new_item_has_it() {
    let e = engine();
    let root = load(
        &e,
        r#"import QtQuick
Item {
    Item { objectName: "plain"; anchors.leftMargin: 4 }
    Item { objectName: "given"; anchors.leftMargin: 4; anchors.margins: 7 }
}
"#,
    );
    let side = |o| {
        e.meta_of(o)
            .unwrap()
            .property_index("anchors.leftMargin")
            .unwrap()
    };
    let plain = find(&e, root, "plain");
    e.reset(plain, side(plain)).unwrap();
    assert_eq!(num(&e, plain, "anchors.leftMargin"), 0.0);
    assert!(!e.is_bound(plain, side(plain)), "nothing to follow yet");
    set(&e, plain, "anchors.margins", Value::Real(5.0));
    assert_eq!(num(&e, plain, "anchors.leftMargin"), 5.0);

    let given = find(&e, root, "given");
    e.reset(given, side(given)).unwrap();
    assert_eq!(num(&e, given, "anchors.leftMargin"), 7.0);
    set(&e, given, "anchors.margins", Value::Real(9.0));
    assert_eq!(num(&e, given, "anchors.leftMargin"), 9.0);
}

/// A property map's key written by the application re-runs the anchors
/// of the items whose geometry follows that key, and of nothing else in
/// the scene: a busy model must not lay out every anchored item on each
/// write (#651). The delegates read the key through the root: an id set
/// on the root context is not found from a delegate yet (#680).
#[test]
fn a_property_map_key_reruns_only_the_anchors_that_follow_it() {
    let e = engine();
    let owner = ljos_engine::PropertyMap::new(&e, None).expect("map creates");
    owner.insert(&e, "wide", Value::Real(40.0));
    owner.insert(&e, "label", Value::from("a"));
    owner.insert(&e, "gap", Value::Real(3.0));
    e.set_context_id(e.root_context(), "owner", owner.object());
    let root = load(
        &e,
        r#"import QtQuick
Item {
    id: root
    width: 400; height: 400
    property string label: owner.label
    property real gap: owner.gap
    property string which: "wide"
    property var map: owner
    Item { id: sized; width: owner.wide; height: 10 }
    Item { objectName: "next"; anchors.left: sized.right; width: 5; height: 5 }
    Item { id: byName; width: owner[which]; height: 3 }
    Item { anchors.left: byName.right; width: 1; height: 1 }
    Item { id: byVar; width: map.wide + 1; height: 3 }
    Item { anchors.left: byVar.right; width: 1; height: 1 }
    Item { objectName: "atRight"; anchors.right: parent.right; width: owner.wide; height: 2 }
    Repeater {
        model: 50
        Item {
            anchors.left: parent.left; anchors.right: parent.right
            anchors.top: parent.top; anchors.topMargin: index * 2
            height: 1
        }
    }
    ListView {
        width: 400; height: 300; model: 30
        delegate: Item {
            width: ListView.view.width; height: 10
            Text { anchors.left: parent.left; anchors.right: parent.right; text: "row " + index }
            Item { anchors.centerIn: parent; width: 4; height: 4 }
        }
    }
    Column {
        y: 100; width: 400
        Repeater {
            model: 10
            Item {
                width: parent.width; height: 10
                Item { objectName: "inset" + index; anchors.fill: parent; anchors.margins: root.gap }
            }
        }
    }
}
"#,
    );
    let mut s = scene(&e, root, 400.0, 400.0);
    frame(&mut s, &e);
    let next = find(&e, root, "next");
    let inset = find(&e, root, "inset3");
    assert_eq!(num(&e, next, "x"), 40.0);
    assert_eq!(rect(&e, inset), (3.0, 3.0, 394.0, 4.0));

    // What a write lays out, counted at the write and at the next frame.
    let mut laid_out = |key: &str, value: Value| {
        let before = ljos_quick::anchor_computes();
        owner.insert(&e, key, value);
        frame(&mut s, &e);
        ljos_quick::anchor_computes() - before
    };
    assert_eq!(
        laid_out("label", Value::from("b")),
        0,
        "a key no geometry reads lays nothing out"
    );
    assert_eq!(
        laid_out("added", Value::Real(1.0)),
        0,
        "a key the map had not got lays nothing out"
    );
    assert_eq!(
        laid_out("wide", Value::Real(60.0)),
        4,
        "only the three items anchored to a resized one, and the one whose \
         right edge holds it, lay out again, once each"
    );
    assert_eq!(
        laid_out("gap", Value::Real(5.0)),
        10,
        "only the ten items whose margins follow the key lay out again"
    );
    assert_eq!(num(&e, next, "x"), 60.0);
    assert_eq!(num(&e, find(&e, root, "atRight"), "x"), 340.0);
    assert_eq!(rect(&e, inset), (5.0, 5.0, 390.0, 0.0));
}
