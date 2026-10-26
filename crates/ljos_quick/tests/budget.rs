//! What a new instance costs, counted exactly: graph nodes by kind, engine
//! objects, lookups by name and heap allocations. The delegate track
//! (docs/adr/0001, 0002) lowers these numbers step by step; each step
//! updates the pin it lowers, so a step that raises one fails here first.
//!
//! The counts are per thread (the allocator's and the lookups'), and every
//! case warms the process-wide caches before it counts, so the numbers do
//! not depend on which test ran first. The cases run one at a time: one
//! filling a process-wide cache while another counts moved the other's
//! census under load (B26: a binding or value more, now and then).

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use common::*;
use ljos_engine::{Engine, Lookups, ObjectId, Value, lookups};
use ljos_quick::{Item, QuickScene, child_items, reduced_motion};
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

static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Held for the whole case; a case that failed leaves it poisoned, which
/// says nothing about the next one.
fn alone() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
}

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
    // With motion reduced a view would skip what it animates, and the
    // counts would be lower than an application pays.
    assert!(!reduced_motion(e), "counted with reduced motion");
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
      id: row
      required property int index
      width: ListView.view.width
      height: 40
      color: index % 2 ? "#eeeeee" : "white"
      Text {
        anchors.verticalCenter: parent.verticalCenter
        x: 12
        text: "Row " + row.index
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
    let (e, mut s, root, _) = plain_scene();
    let before = e.children(root).len();
    let make = e.method_index(root, "make").unwrap();
    let b = spent(&e, &mut s, 2000, || {
        e.invoke(root, make, &[]).unwrap();
    });
    (b, e.children(root).len() - before)
}

/// The scene of [`plain_items`], warmed by a first ten, which are given.
fn plain_scene() -> (Engine, QuickScene, ObjectId, Vec<ObjectId>) {
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
      id: item
      width: 100; height: 20
      Text { text: "Row"; width: item.width }
    }
  }
}
"##,
    );
    let mut s = scene(&e, root, 400.0, 600.0);
    settle(&mut s, &e, 0);
    // Warm: the first instance links what every later one shares.
    let before = e.children(root);
    e.invoke_by_name(root, "make", &[]).unwrap();
    settle(&mut s, &e, 1000);
    let made = made_since(&e, root, &before);
    (e, s, root, made)
}

fn made_since(e: &Engine, root: ObjectId, before: &[ObjectId]) -> Vec<ObjectId> {
    let mut made = e.children(root);
    made.retain(|c| !before.contains(c));
    made
}

/// What ten instances of [`plain_items`] gave back when destroyed, and
/// what destroying them allocated.
#[derive(Debug, PartialEq, Eq)]
struct Teardown {
    objects: usize,
    scopes: usize,
    nodes: usize,
    allocations: u64,
}

fn plain_teardown() -> Teardown {
    let (e, mut s, root, warm) = plain_scene();
    // The first ten destroyed warm what teardown keeps for the next.
    for c in warm.iter().rev() {
        e.destroy_now(*c);
    }
    settle(&mut s, &e, 2000);
    let before = e.children(root);
    e.invoke_by_name(root, "make", &[]).unwrap();
    settle(&mut s, &e, 3000);
    let made = made_since(&e, root, &before);
    assert_eq!(made.len(), 10);
    let a = now(&e);
    for c in made.iter().rev() {
        e.destroy_now(*c);
    }
    for i in 0..FRAMES {
        s.frame(&e, Time::from_millis(4000 + i * 16));
    }
    let b = now(&e);
    let nodes = |c: Census| c.values + c.bindings + c.effects;
    Teardown {
        objects: a.objects - b.objects,
        scopes: a.graph.scopes - b.graph.scopes,
        nodes: nodes(a.graph) - nodes(b.graph),
        allocations: b.allocations - a.allocations,
    }
}

#[test]
fn ten_listview_delegates() {
    let _alone = alone();
    let (b, made) = listview_delegates();
    assert_eq!(made, 10);
    // Per delegate: 3 objects (the Rectangle, the Text and what they hold
    // attached or grouped), 9 values, 6 bindings, 16 effects, 2 scopes,
    // no member or type lookups, 142.6 allocations. What the component
    // writes and binds goes by index once its plan is linked, and its id
    // (`row`) is found by slot without being written down. The row plan
    // writes `index` on the delegate itself: no row object, and no
    // binding to one.
    // Of the text's, a run's glyphs are read from Parley once and reserved
    // for, not grown as they come (two fewer), and its lines' items are
    // kept between measuring and painting (one more).
    // A one-line plain text is shaped without Parley's builder and the
    // layout it makes (fifteen fewer). A binding is one allocation, the
    // creator's closure inside the graph's box with the wrapper (seven
    // fewer, B26), and a scope's node list is made the size the
    // declaration's last one was.
    // A new row's run is kept for when the row is shown again, its text
    // and its glyphs (21 more), and a row analyses as one before it did,
    // without Parley's analyser (seven fewer).
    // The scene keeps one record an item, its effects held inline, where it
    // kept a map per concern, a list of effects and a box for the inherited
    // layers (twenty-five fewer, B27).
    assert_eq!(
        b,
        Budget {
            objects: 30,
            graph: Census {
                values: 90,
                bindings: 60,
                effects: 160,
                scopes: 20,
            },
            lookups: Lookups {
                members: 0,
                types: 0,
                names: 0,
                atoms: 0,
            },
            allocations: 1426,
        }
    );
}

#[test]
fn ten_plain_items() {
    let _alone = alone();
    let (b, made) = plain_items();
    assert_eq!(made, 10);
    // Per instance: 2 objects, 6 values, 2 bindings, 15 effects, 2 scopes,
    // 1 member lookup (the property `createObject` is given, by its name)
    // and no type lookups, 95.5 allocations; the id (`item`) is found
    // by slot. One of the ten grows what keeps a text's lines' items
    // between measuring and painting. A one-line plain text is laid out
    // without Parley's builder (fifteen fewer over the ten), and B26 put
    // each binding in one box (thirty fewer). B27's one scene record an
    // item took twenty-eight off.
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
            allocations: 955,
        }
    );
}

#[test]
fn ten_plain_items_torn_down() {
    let _alone = alone();
    // Every object, scope and node the ten made goes, each scope's nodes
    // together; the scopes' node lists and the disposal's are kept for
    // the next ones. 5.7 allocations an instance: the scene forgetting
    // its items, the header list and a list edit of the holder. B26 took
    // it from 13.7 by holding the walks over the dead inline.
    assert_eq!(
        plain_teardown(),
        Teardown {
            objects: 20,
            scopes: 20,
            nodes: 230,
            allocations: 57,
        }
    );
}
