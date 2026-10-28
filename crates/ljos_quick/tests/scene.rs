//! The frame pipeline: idle frames, culling, render-tree mapping and
//! golden images through the R backend (`VelloCpuReference`).

mod common;

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use common::*;
use ljos_engine::{Color, Engine, ObjectId, Value};
use ljos_quick::{Item, QuickScene, Rectangle};
use ljos_render::{FrameView, RasterConfig, Rgba, RgbaImage, TextMode, VelloCpuReference};
use ljos_runtime::Time;

fn rgb(r: u8, g: u8, b: u8) -> Value {
    Value::Color(Color::from_rgba8(r, g, b, 255))
}

fn rect_item(e: &Engine, parent: Option<ObjectId>, g: (f64, f64, f64, f64), c: Value) -> ObjectId {
    let r = make::<Rectangle>(e, parent);
    geom(e, r, g.0, g.1, g.2, g.3);
    set(e, r, "color", c);
    r
}

#[test]
fn a_second_frame_with_nothing_to_do_is_idle() {
    let e = engine();
    let root = rect_item(&e, None, (0.0, 0.0, 100.0, 100.0), rgb(200, 0, 0));
    let child = rect_item(&e, Some(root), (10.0, 10.0, 20.0, 20.0), rgb(0, 0, 200));
    let mut s = scene(&e, root, 100.0, 100.0);
    let first = frame(&mut s, &e);
    assert!(first.changed);
    assert_eq!(first.next_wake, None);
    let rebuilds = s.render_tree().content_rebuilds();
    let second = s.frame(&e, Time::from_millis(16));
    assert!(!second.changed);
    assert!(Arc::ptr_eq(&first.snapshot, &second.snapshot));
    assert_eq!(second.next_wake, None);
    assert_eq!(s.render_tree().content_rebuilds(), rebuilds);
    assert!(!s.needs_frame());

    // A change wakes the scene and rebuilds only what changed.
    let woke = Rc::new(Cell::new(0));
    let w = woke.clone();
    s.set_waker(move || w.set(w.get() + 1));
    set(&e, child, "color", rgb(0, 200, 0));
    assert!(s.needs_frame());
    assert_eq!(woke.get(), 1);
    let third = s.frame(&e, Time::from_millis(32));
    assert!(third.changed);
    assert_eq!(s.render_tree().content_rebuilds(), rebuilds + 1);
    // A move changes the snapshot without repainting content.
    set(&e, child, "x", Value::Real(30.0));
    let fourth = s.frame(&e, Time::from_millis(48));
    assert!(fourth.changed);
    assert_eq!(s.render_tree().content_rebuilds(), rebuilds + 1);
}

/// A frame where one item of many changes colour or moves leaves the
/// paint effects of the rest alone: it does not look at every item to
/// tell which are on screen, as it must when one comes or goes.
#[test]
fn a_frame_that_shows_the_same_items_does_not_look_at_them_all() {
    let e = engine();
    let root = rect_item(&e, None, (0.0, 0.0, 400.0, 400.0), rgb(200, 0, 0));
    for i in 0..200 {
        let x = f64::from(i % 20) * 20.0;
        let y = f64::from(i / 20) * 20.0;
        rect_item(&e, Some(root), (x, y, 10.0, 10.0), rgb(0, 0, 200));
    }
    let busy = rect_item(&e, Some(root), (0.0, 300.0, 10.0, 10.0), rgb(0, 200, 0));
    let mut s = scene(&e, root, 400.0, 400.0);
    frame(&mut s, &e);
    let looked = s.content_effects_looked_at();
    assert!(looked >= 202);
    for i in 1..=30u32 {
        let c = if i % 2 == 0 {
            rgb(0, 200, 0)
        } else {
            rgb(0, 0, 9)
        };
        set(&e, busy, "color", c);
        set(&e, busy, "x", Value::Real(f64::from(i)));
        assert!(s.frame(&e, Time::from_millis(16 * u64::from(i))).changed);
    }
    assert_eq!(s.content_effects_looked_at(), looked);
    // One that comes into view is matched; the rest still are not redrawn.
    let rebuilds = s.render_tree().content_rebuilds();
    rect_item(&e, Some(root), (390.0, 390.0, 10.0, 10.0), rgb(9, 9, 9));
    s.frame(&e, Time::from_millis(600));
    assert!(s.content_effects_looked_at() > looked);
    assert_eq!(s.render_tree().content_rebuilds(), rebuilds + 1);
}

/// Content that moves under the window shows other rows, but the items
/// stay the same: the frame looks at the rows that came or went, not at
/// every item.
#[test]
fn a_scroll_looks_only_at_the_rows_that_came_or_went() {
    let e = engine();
    let root = rect_item(&e, None, (0.0, 0.0, 100.0, 100.0), rgb(200, 0, 0));
    let content = rect_item(&e, Some(root), (0.0, 0.0, 100.0, 4000.0), rgb(0, 0, 0));
    for i in 0..200 {
        rect_item(
            &e,
            Some(content),
            (0.0, f64::from(i) * 20.0, 100.0, 10.0),
            rgb(0, 0, 200),
        );
    }
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    let first = s.content_effects_looked_at();
    for i in 1..=10u32 {
        let looked = s.content_effects_looked_at();
        set(&e, content, "y", Value::Real(-20.0 * f64::from(i)));
        assert!(s.frame(&e, Time::from_millis(16 * u64::from(i))).changed);
        let now = s.content_effects_looked_at() - looked;
        assert!(now <= 4, "a scroll by a row looked at {now} items");
    }
    assert!(s.content_effects_looked_at() > first, "rows came and went");
}

/// The reads that showing a hidden Column of `rows` rows records, and
/// hiding it again, each with its frame.
fn reads_to_show_and_hide(rows: usize) -> u64 {
    let e = engine();
    let root = load(
        &e,
        &format!(
            r#"import QtQuick
Item {{
    width: 200; height: 400
    Item {{
        objectName: "panel"; visible: false
        Column {{
            Repeater {{ model: {rows}; Item {{ width: 100; height: 20 }} }}
        }}
    }}
}}
"#
        ),
    );
    let panel = find(&e, root, "panel");
    let mut s = scene(&e, root, 200.0, 400.0);
    frame(&mut s, &e);
    let before = e.graph().stats().reads;
    for (i, shown) in [true, false].into_iter().enumerate() {
        set(&e, panel, "visible", Value::Bool(shown));
        s.frame(&e, Time::from_millis(16 * (i as u64 + 1)));
    }
    e.graph().stats().reads - before
}

/// A hidden parent that shows its children changes each one's `visible`
/// in turn. A Column reads every child, and reads them all again once it
/// is laid out, not once for each child that changed: four times the
/// rows cost four times the reads, not sixteen.
#[test]
fn showing_a_column_reads_its_rows_once_not_once_per_row() {
    let small = reads_to_show_and_hide(100);
    let large = reads_to_show_and_hide(400);
    assert!(
        large < small * 6,
        "100 rows recorded {small} reads and 400 rows {large}"
    );
}

#[test]
fn an_item_that_only_moves_reads_only_where_it_is() {
    let e = engine();
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 400.0, 400.0);
    let items: Vec<ObjectId> = (0..50)
        .map(|i| {
            rect_item(
                &e,
                Some(root),
                (0.0, f64::from(i) * 4.0, 10.0, 4.0),
                rgb(9, 9, 9),
            )
        })
        .collect();
    let mut s = scene(&e, root, 400.0, 400.0);
    frame(&mut s, &e);
    let reads = e.graph().stats().reads;
    for (i, o) in items.iter().enumerate() {
        set(&e, *o, "x", Value::Real(i as f64 + 1.0));
    }
    let out = s.frame(&e, Time::from_millis(16));
    assert!(out.changed);
    // `x` and `y`; not the eleven other inputs of the node's geometry.
    let per_item = (e.graph().stats().reads - reads) as f64 / items.len() as f64;
    assert!(per_item <= 2.0, "{per_item} reads per moved item");
    for (i, o) in items.iter().enumerate() {
        let n = s.render_tree().node(s.node_of(*o).unwrap()).unwrap();
        let at = n.local().translation();
        assert_eq!((at.x, at.y), (i as f64 + 1.0, i as f64 * 4.0));
    }

    // Moved and resized together: both reach the node.
    set(&e, items[0], "y", Value::Real(100.0));
    set(&e, items[0], "width", Value::Real(30.0));
    s.frame(&e, Time::from_millis(32));
    let n = s.render_tree().node(s.node_of(items[0]).unwrap()).unwrap();
    assert_eq!(n.local().translation().y, 100.0);
    assert_eq!(n.size().width, 30.0);
}

#[test]
fn off_viewport_items_never_pull_their_paint_bindings() {
    let e = engine();
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 100.0, 100.0);
    let r = make::<Rectangle>(&e, Some(root));
    geom(&e, r, 500.0, 0.0, 20.0, 20.0);
    let pulls = Rc::new(Cell::new(0));
    let p = pulls.clone();
    e.bind(r, Rectangle::COLOR.index, move |_| {
        p.set(p.get() + 1);
        Value::Color(Color::BLACK)
    })
    .unwrap();
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);
    frame(&mut s, &e);
    assert_eq!(pulls.get(), 0);
    set(&e, r, "x", Value::Real(10.0));
    frame(&mut s, &e);
    assert_eq!(pulls.get(), 1);
}

#[test]
fn invisible_items_are_not_painted_or_pulled() {
    let e = engine();
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 100.0, 100.0);
    let r = make::<Rectangle>(&e, Some(root));
    geom(&e, r, 0.0, 0.0, 20.0, 20.0);
    set(&e, r, "visible", Value::Bool(false));
    let pulls = Rc::new(Cell::new(0));
    let p = pulls.clone();
    e.bind(r, Rectangle::COLOR.index, move |_| {
        p.set(p.get() + 1);
        Value::Color(Color::BLACK)
    })
    .unwrap();
    let mut s = scene(&e, root, 100.0, 100.0);
    let out = frame(&mut s, &e);
    assert_eq!(pulls.get(), 0);
    let node = s.node_of(r).unwrap();
    assert!(out.snapshot.items.iter().all(|i| i.node != node));
}

#[test]
fn items_map_to_render_nodes_and_back() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let a = make::<Item>(&e, Some(root));
    let s = scene(&e, root, 10.0, 10.0);
    let n = s.node_of(a).unwrap();
    assert_eq!(s.item_of(n), Some(a));
    let root_node = s.node_of(root).unwrap();
    let kids: Vec<_> = s.render_tree().node(root_node).unwrap().children().to_vec();
    assert_eq!(kids, [n]);
}

#[test]
fn destroyed_items_leave_the_render_tree() {
    let e = engine();
    let root = make::<Item>(&e, None);
    let a = rect_item(&e, Some(root), (0.0, 0.0, 10.0, 10.0), rgb(1, 2, 3));
    let mut s = scene(&e, root, 10.0, 10.0);
    frame(&mut s, &e);
    let n = s.node_of(a).unwrap();
    e.destroy_now(a);
    let out = frame(&mut s, &e);
    assert!(out.changed);
    assert_eq!(s.node_of(a), None);
    assert!(s.render_tree().node(n).is_none());
}

fn render(s: &QuickScene) -> RgbaImage {
    let snap = s.snapshot().expect("no frame yet");
    VelloCpuReference::new(RasterConfig {
        background: Rgba::WHITE,
        text: TextMode::Skip,
    })
    .render(&FrameView::new(snap, 1.0))
    .unwrap()
}

fn assert_golden(name: &str, img: &RgbaImage) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.png"));
    if std::env::var_os("LJOS_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, img.to_png().unwrap()).unwrap();
    }
    let bytes = std::fs::read(&path).unwrap_or_else(|err| {
        panic!(
            "missing golden {}: {err} (run with LJOS_BLESS=1)",
            path.display()
        )
    });
    let want = RgbaImage::from_png(&bytes).unwrap();
    assert_eq!((img.width, img.height), (want.width, want.height), "{name}");
    if let Some(i) = img.data.iter().zip(&want.data).position(|(a, b)| a != b) {
        let px = i as u32 / 4;
        panic!(
            "{name}: pixel ({}, {}) differs",
            px % img.width,
            px / img.width
        );
    }
}

#[test]
fn golden_rectangle_with_radius_and_border() {
    let e = engine();
    let root = make::<Item>(&e, None);
    geom(&e, root, 0.0, 0.0, 64.0, 48.0);
    let r = rect_item(&e, Some(root), (8.0, 8.0, 48.0, 32.0), rgb(60, 140, 220));
    set(&e, r, "radius", Value::Real(8.0));
    set(&e, r, "border.width", Value::Real(3.0));
    set(&e, r, "border.color", rgb(20, 40, 80));
    let mut s = scene(&e, root, 64.0, 48.0);
    frame(&mut s, &e);
    let img = render(&s);
    // Spot checks that do not depend on antialiasing.
    assert_eq!(img.pixel(1, 1), [255, 255, 255, 255]);
    assert_eq!(img.pixel(32, 24), [60, 140, 220, 255]);
    assert_eq!(img.pixel(32, 9), [20, 40, 80, 255]);
    // The rounded corner leaves the corner pixel unpainted.
    assert_eq!(img.pixel(8, 8), [255, 255, 255, 255]);
    assert_golden("rectangle_radius_border", &img);
}

#[test]
fn golden_nested_opacity_and_clip() {
    let e = engine();
    let root = rect_item(&e, None, (0.0, 0.0, 64.0, 64.0), rgb(255, 255, 255));
    let outer = rect_item(&e, Some(root), (8.0, 8.0, 40.0, 40.0), rgb(0, 0, 255));
    set(&e, outer, "opacity", Value::Real(0.5));
    set(&e, outer, "clip", Value::Bool(true));
    let inner = rect_item(&e, Some(outer), (20.0, 20.0, 40.0, 40.0), rgb(255, 0, 0));
    set(&e, inner, "opacity", Value::Real(0.5));
    let mut s = scene(&e, root, 64.0, 64.0);
    frame(&mut s, &e);
    let img = render(&s);
    // Clipped: the inner rectangle stops at the outer one's edge (x 48).
    assert_eq!(img.pixel(52, 40), [255, 255, 255, 255]);
    // Outer alone, half blue over white.
    let [r, g, b, _] = img.pixel(12, 12);
    assert!(
        r.abs_diff(127) <= 1 && g.abs_diff(127) <= 1 && b == 255,
        "{r} {g} {b}"
    );
    // As in Qt Quick without a layer, opacity multiplies down the tree and
    // each item blends on its own: red at 25 % over the half-blue outer.
    let got = img.pixel(40, 40);
    let want = [159u8, 96, 191];
    assert!(
        got.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 2),
        "{got:?}"
    );
    assert_golden("nested_opacity_clip", &img);
}

const TURNED: &str = r#"import QtQuick
Item {
  width: 100; height: 100
  Rectangle {
    objectName: "box"; x: 10; y: 20; width: 40; height: 20; color: "red"
    transform: Rotation { objectName: "turn"; origin.x: 20; origin.y: 10; angle: 0 }
  }
  Repeater { model: 20; Rectangle { width: 5; height: 5 } }
}
"#;

fn world_of(s: &QuickScene, item: ObjectId) -> [f64; 6] {
    let node = s.node_of(item).expect("a node");
    let snap = s.snapshot().expect("a frame");
    snap.items
        .iter()
        .find(|i| i.node == node)
        .expect("painted")
        .world
        .as_coeffs()
}

#[test]
fn a_turning_transform_entry_reruns_one_effect_and_shows_what_a_fresh_scene_does() {
    let e = engine();
    let root = load(&e, TURNED);
    let (bx, turn) = (find(&e, root, "box"), find(&e, root, "turn"));
    let mut s = scene(&e, root, 100.0, 100.0);
    frame(&mut s, &e);

    let before = e.graph().stats().effect_runs;
    set(&e, turn, "angle", Value::Real(30.0));
    // The effect that follows the list, not the one of all the geometry.
    assert_eq!(e.graph().stats().effect_runs - before, 1);
    frame(&mut s, &e);
    let mut fresh = scene(&e, root, 100.0, 100.0);
    frame(&mut fresh, &e);
    assert_eq!(world_of(&s, bx), world_of(&fresh, bx));
    assert_ne!(world_of(&s, bx)[1], 0.0, "turned");

    // An emptied list leaves nothing following its old entry.
    set(&e, bx, "transform", Value::List(Default::default()));
    frame(&mut s, &e);
    let before = e.graph().stats().effect_runs;
    set(&e, turn, "angle", Value::Real(60.0));
    assert_eq!(e.graph().stats().effect_runs - before, 0);
    frame(&mut s, &e);
    assert_eq!(world_of(&s, bx), [1.0, 0.0, 0.0, 1.0, 10.0, 20.0]);
}

// A child that grows its sibling each time the Row moves it marks the
// Row for another polish pass each pass. The frame stops after
// `MAX_POLISH_PASSES` with a warning that names the Row where it was
// declared, and returns.
#[test]
fn a_polish_that_never_settles_is_named_and_the_frame_returns() {
    let e = engine();
    let root = load(
        &e,
        r#"
import QtQuick
Item {
    width: 400; height: 100
    Row {
        objectName: "row"
        Rectangle { id: a; width: 10; height: 10 }
        Rectangle { width: 10; height: 10; onXChanged: a.width += 1 }
    }
}
"#,
    );
    let mut s = scene(&e, root, 400.0, 100.0);
    frame(&mut s, &e);
    let said: Vec<String> = e.take_log().into_iter().map(|l| l.message).collect();
    let want = format!(
        "file:///Main.qml:5:5: QML Row: Possible polish loop: still dirty after {} passes",
        ljos_quick::MAX_POLISH_PASSES
    );
    assert!(said.iter().any(|w| w.starts_with(&want)), "{said:?}");
}
