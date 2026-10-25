//! What a new instance costs, counted exactly: graph nodes by kind, engine
//! objects, lookups by name and heap allocations. The delegate track
//! (docs/adr/0001, 0002) lowers these numbers step by step; each step
//! updates the pin it lowers, so a step that raises one fails here first.
//!
//! The counts are per thread (the allocator's and the lookups'), and every
//! case warms the process-wide caches before it counts, so the numbers do
//! not depend on which test ran first.

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use common::*;
use ljos_engine::{Engine, Lookups, ObjectId, Value, lookups};
use ljos_quick::{Item, QuickScene, child_items};
use ljos_reactive::Census;
use ljos_runtime::Time;

struct Counting;

thread_local! {
    // A const initialiser and no destructor: the allocator may run while
    // the thread's locals are being torn down.
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
}

fn bump() {
    let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
}

// SAFETY: every call forwards to `System` unchanged; the count is a side
// effect that neither allocates nor touches the memory.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        bump();
        // SAFETY: the caller's contract, passed on.
        unsafe { System.alloc(l) }
    }

    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        bump();
        // SAFETY: the caller's contract, passed on.
        unsafe { System.alloc_zeroed(l) }
    }

    unsafe fn realloc(&self, p: *mut u8, l: Layout, size: usize) -> *mut u8 {
        bump();
        // SAFETY: the caller's contract, passed on.
        unsafe { System.realloc(p, l, size) }
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        // SAFETY: the caller's contract, passed on.
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// The counts a test pins: what a step added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Budget {
    objects: usize,
    graph: Census,
    lookups: Lookups,
    allocations: u64,
}

fn now(e: &Engine) -> Budget {
    Budget {
        objects: e.object_count(),
        graph: e.graph().census(),
        lookups: lookups(),
        allocations: ALLOCS.with(Cell::get),
    }
}

/// What `step` and the frames after it added.
fn spent(e: &Engine, s: &mut QuickScene, from: u64, step: impl FnOnce()) -> Budget {
    let a = now(e);
    step();
    for i in 0..FRAMES {
        s.frame(e, Time::from_millis(from + i * 16));
    }
    let b = now(e);
    Budget {
        objects: b.objects - a.objects,
        graph: Census {
            values: b.graph.values - a.graph.values,
            bindings: b.graph.bindings - a.graph.bindings,
            effects: b.graph.effects - a.graph.effects,
            scopes: b.graph.scopes - a.graph.scopes,
        },
        lookups: b.lookups - a.lookups,
        allocations: b.allocations - a.allocations,
    }
}

// Enough for a new row to be laid out, polished and drawn; an idle frame
// after that adds nothing, so more frames give the same counts.
const FRAMES: u64 = 4;

fn settle(s: &mut QuickScene, e: &Engine, from: u64) {
    for i in 0..40 {
        s.frame(e, Time::from_millis(from + i * 16));
    }
}

fn shown(e: &Engine, view: ObjectId) -> usize {
    let content = get(e, view, "contentItem").as_object().unwrap();
    child_items(e, content).len()
}

/// The ListView delegate of `bench/listview.qml`: a Rectangle with a
/// required index, a binding to the view's width and a Text anchored in it.
fn listview_delegates() -> (Budget, usize) {
    let e = engine();
    let root = load(
        &e,
        r##"import QtQuick
Rectangle {
  width: 400; height: 200
  ListView {
    objectName: "view"
    anchors.fill: parent
    cacheBuffer: 0
    model: 10000
    delegate: Rectangle {
      required property int index
      width: ListView.view.width
      height: 40
      color: index % 2 ? "#eeeeee" : "white"
      Text {
        anchors.verticalCenter: parent.verticalCenter
        x: 12
        text: "Row " + parent.index
      }
    }
  }
}
"##,
    );
    let view = find(&e, root, "view");
    let mut s = scene(&e, root, 400.0, 1000.0);
    settle(&mut s, &e, 0);
    // Warm: the first rows link what every later one shares.
    set(&e, root, "height", Value::from(600.0));
    settle(&mut s, &e, 1000);
    let before = shown(&e, view);
    // By index: the step looks nothing up itself, so what is counted is
    // what the new delegates cost.
    let b = spent(&e, &mut s, 2000, || {
        e.set(root, Item::HEIGHT, 1000.0).unwrap();
    });
    (b, shown(&e, view) - before)
}

/// An Item holding a Text, made by `Component.createObject` ten at a time.
fn plain_items() -> (Budget, usize) {
    let e = engine();
    let root = load(
        &e,
        r##"import QtQuick
Item {
  width: 400; height: 600
  function make() {
    for (var i = 0; i < 10; i++) one.createObject(this, { y: 20 * i })
  }
  Component {
    id: one
    Item {
      width: 100; height: 20
      Text { text: "Row" }
    }
  }
}
"##,
    );
    let mut s = scene(&e, root, 400.0, 600.0);
    settle(&mut s, &e, 0);
    // Warm: the first instance links what every later one shares.
    e.invoke_by_name(root, "make", &[]).unwrap();
    settle(&mut s, &e, 1000);
    let before = e.children(root).len();
    let make = e.method_index(root, "make").unwrap();
    let b = spent(&e, &mut s, 2000, || {
        e.invoke(root, make, &[]).unwrap();
    });
    (b, e.children(root).len() - before)
}

#[test]
fn ten_listview_delegates() {
    let (b, made) = listview_delegates();
    assert_eq!(made, 10);
    // Per delegate: 4 objects (the Rectangle, the Text and what they hold
    // attached or grouped), 8 values, 7 bindings, 16 effects, 2 scopes,
    // 4 member lookups (the view's row, found by name for each delegate)
    // and no type lookups, 181.6 allocations. What the component writes
    // and binds goes by index once its plan is linked.
    assert_eq!(
        b,
        Budget {
            objects: 40,
            graph: Census {
                values: 80,
                bindings: 70,
                effects: 160,
                scopes: 20,
            },
            lookups: Lookups {
                members: 40,
                types: 0,
                names: 0,
                atoms: 0,
            },
            allocations: 1816,
        }
    );
}

#[test]
fn ten_plain_items() {
    let (b, made) = plain_items();
    assert_eq!(made, 10);
    // Per instance: 2 objects, 6 values, 2 bindings, 15 effects, 2 scopes,
    // 1 member lookup (the property `createObject` is given, by its name)
    // and no type lookups, 103.7 allocations.
    assert_eq!(
        b,
        Budget {
            objects: 20,
            graph: Census {
                values: 60,
                bindings: 20,
                effects: 150,
                scopes: 20,
            },
            lookups: Lookups {
                members: 10,
                types: 0,
                names: 0,
                atoms: 0,
            },
            allocations: 1037,
        }
    );
}
