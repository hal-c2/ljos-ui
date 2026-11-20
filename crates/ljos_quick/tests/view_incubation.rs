//! A view's rows that are made over several frames (those of the
//! cacheBuffer): none is painted before the view has put it in its place.
//! Qt culls a delegate from the moment it is parented
//! (`QQuickItemView::initItem`) until the view lays it out; painted at
//! once, a row lies over the first rows, at its `y` of 0, until the view
//! places it.

mod common;

use ljos_render::kurbo;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use common::*;
use ljos_engine::{Engine, ObjectId};
use ljos_quick::{QuickScene, child_items};
use ljos_runtime::{Clock, Time};

/// A clock a look at which costs 100 µs: a delegate of many bindings
/// takes several frames' budgets to make.
#[derive(Debug)]
struct BusyClock(AtomicU64);

impl Clock for BusyClock {
    fn now(&self) -> Time {
        Time::from_micros(self.0.fetch_add(100, Ordering::Relaxed))
    }
}

fn busy_engine() -> Engine {
    let e = engine();
    e.set_eager_bindings(true);
    e.set_clock(Rc::new(BusyClock(AtomicU64::new(0))));
    e
}

/// A list of `rows` rows of 20, each of 40 items with a binding.
fn list(rows: usize, more: &str) -> String {
    let kids: String = (0..40)
        .map(|i| format!("    Item {{ width: parent.width - {i}; height: 1 }}\n"))
        .collect();
    format!(
        r##"import QtQuick
ListView {{
  width: 200; height: 100
  cacheBuffer: 100
  model: ListModel {{ id: m; Component.onCompleted: {{ for (let i = 0; i < {rows}; ++i) append({{ n: i }}) }} }}
  delegate: Rectangle {{
    required property int n
    required property int index
    color: "#336699"
    objectName: "r" + n
    width: 200; height: 20
{kids}  }}
{more}
}}
"##
    )
}

/// The rows painted where row `n` is, but for row `n` itself, made.
fn painted_over(s: &QuickScene, e: &Engine, view: ObjectId, n: usize) -> Vec<String> {
    let content = get(e, view, "contentItem").as_object().unwrap();
    let rows = child_items(e, content);
    let at = kurbo::Point::new(5.0, n as f64 * 20.0 + 10.0 - num(e, view, "contentY"));
    let snap = s.snapshot().expect("a frame was made");
    let mut over = Vec::new();
    for item in &snap.items {
        let local = item.world.inverse() * at;
        let inside = local.x >= 0.0
            && local.y >= 0.0
            && local.x < item.size.width
            && local.y < item.size.height;
        if item.list.is_none() || item.opacity <= 0.0 || !inside {
            continue;
        }
        let Some(row) = s.item_of(item.node).filter(|o| rows.contains(o)) else {
            continue;
        };
        let name = get(e, row, "objectName").to_js_string().to_string();
        if num(e, row, "index") != n as f64 || name.is_empty() {
            over.push(format!(
                "{name:?} (n {:?}) at y {}",
                get(e, row, "n"),
                num(e, row, "y")
            ));
        }
    }
    over
}

/// Whether the rows of the buffer, 5 to 9, are made and in their places.
fn buffer_placed(e: &Engine, view: ObjectId) -> bool {
    let content = get(e, view, "contentItem").as_object().unwrap();
    let rows = child_items(e, content);
    (5..10).all(|n| {
        rows.iter()
            .any(|&o| num(e, o, "index") == n as f64 && num(e, o, "y") == n as f64 * 20.0)
    })
}

/// Frames until the rows of the buffer are in their places, each checked:
/// what is painted over the five rows in sight is only those rows.
fn frames_paint_rows_in_place(s: &mut QuickScene, e: &Engine, view: ObjectId, from: u64) {
    for i in 0..200 {
        s.frame(e, Time::from_millis(from + i * 16));
        for n in 0..5 {
            let over = painted_over(s, e, view, n);
            assert!(over.is_empty(), "frame {i}: painted over r{n}: {over:?}");
        }
        if buffer_placed(e, view) {
            // Made over several frames, or the test shows nothing.
            assert!(i > 1, "the buffer was made in frame {i}");
            return;
        }
    }
    panic!("the rows of the buffer never took their places");
}

#[test]
fn the_rows_of_the_buffer_are_not_painted_before_they_are_placed() {
    let e = busy_engine();
    let root = load(&e, &list(40, ""));
    let mut s = scene(&e, root, 200.0, 100.0);
    frames_paint_rows_in_place(&mut s, &e, root, 0);
}

#[test]
fn rows_moved_into_the_buffer_are_not_painted_before_they_are_placed() {
    let e = busy_engine();
    let root = load(
        &e,
        &list(
            40,
            "  function shuffle() { m.move(30, 0, 1); m.move(31, 2, 1); m.remove(5, 3); m.insert(1, { n: 100 }) }",
        ),
    );
    let mut s = scene(&e, root, 200.0, 100.0);
    frames_paint_rows_in_place(&mut s, &e, root, 0);
    e.invoke_by_name(root, "shuffle", &[]).unwrap();
    frames_paint_rows_in_place(&mut s, &e, root, 10_000);
}

#[test]
fn the_cells_of_a_grid_s_buffer_are_not_painted_before_they_are_placed() {
    let e = busy_engine();
    let grid = list(40, "").replace("ListView {", "GridView {\n  cellWidth: 200; cellHeight: 20");
    let root = load(&e, &grid);
    let mut s = scene(&e, root, 200.0, 100.0);
    frames_paint_rows_in_place(&mut s, &e, root, 0);
}
