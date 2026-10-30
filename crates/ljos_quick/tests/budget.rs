//! What a new instance costs, counted exactly: graph nodes by kind, engine
//! objects, lookups by name, heap allocations and the bytes they keep.
//! The delegate track
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
use std::rc::Rc;

use common::*;
use ljos_engine::{Engine, Lookups, ObjectId, Value, lookups};
use ljos_quick::{Item, QuickScene, child_items, reduced_motion};
use ljos_reactive::Census;
use ljos_runtime::{Time, VirtualClock};

thread_local! {
    // The clock of the engine made last, which the frames are for.
    static CLOCK: std::cell::RefCell<Rc<VirtualClock>> = Default::default();
}

/// An engine on a clock only the frames move. On the machine's clock each
/// frame's incubation budget is spent in its time, so how many rows a
/// frame makes, and what they cost, would follow how busy the machine is.
fn engine() -> Engine {
    let e = common::engine();
    let clock = Rc::new(VirtualClock::new());
    CLOCK.with(|c| *c.borrow_mut() = Rc::clone(&clock));
    e.set_clock(clock);
    e
}

fn frame_at(s: &mut QuickScene, e: &Engine, ms: u64) {
    let t = Time::from_millis(ms);
    CLOCK.with(|c| c.borrow().set(t));
    s.frame(e, t);
}

struct Counting;

thread_local! {
    // A const initialiser and no destructor: the allocator may run while
    // the thread's locals are being torn down.
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
    // Bytes allocated and not yet freed: what an instance keeps, where
    // the count of calls cannot tell a small box from a large one.
    static LIVE: Cell<i64> = const { Cell::new(0) };
}

fn bump(bytes: i64) {
    let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
    keep(bytes);
}

fn keep(bytes: i64) {
    let _ = LIVE.try_with(|c| c.set(c.get() + bytes));
}

// SAFETY: every call forwards to `System` unchanged; the count is a side
// effect that neither allocates nor touches the memory.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        bump(l.size() as i64);
        // SAFETY: the caller's contract, passed on.
        unsafe { System.alloc(l) }
    }

    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        bump(l.size() as i64);
        // SAFETY: the caller's contract, passed on.
        unsafe { System.alloc_zeroed(l) }
    }

    unsafe fn realloc(&self, p: *mut u8, l: Layout, size: usize) -> *mut u8 {
        bump(size as i64 - l.size() as i64);
        // SAFETY: the caller's contract, passed on.
        unsafe { System.realloc(p, l, size) }
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        keep(-(l.size() as i64));
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
    /// Heap bytes the step left allocated. A list that grew counts its
    /// whole new capacity on the step that grew it.
    bytes: i64,
}

fn now(e: &Engine) -> Budget {
    Budget {
        objects: e.object_count(),
        graph: e.graph().census(),
        lookups: lookups(),
        allocations: ALLOCS.with(Cell::get),
        bytes: LIVE.with(Cell::get),
    }
}

/// What `step` and the frames after it added.
fn spent(e: &Engine, s: &mut QuickScene, from: u64, step: impl FnOnce()) -> Budget {
    let a = now(e);
    step();
    for i in 0..FRAMES {
        frame_at(s, e, from + i * 16);
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
        bytes: b.bytes - a.bytes,
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
        frame_at(s, e, from + i * 16);
    }
}

fn shown(e: &Engine, view: ObjectId) -> usize {
    let content = get(e, view, "contentItem").as_object().unwrap();
    child_items(e, content).len()
}

/// The ListView delegate of `bench/listview.qml`: a Rectangle with a
/// required index, a binding to the view's width and a Text anchored in it.
/// A row's text: one short line.
const ONE_LINE: &str = r##"Text {
        anchors.verticalCenter: parent.verticalCenter
        x: 12
        text: "Row " + row.index
      }"##;

/// A row's text: a message wrapped on two lines, the second elided.
const WRAPPED: &str = r##"Text {
        x: 12
        width: row.width - 24
        wrapMode: Text.WordWrap
        maximumLineCount: 2
        elide: Text.ElideRight
        text: "Message " + row.index + ": the quick brown fox jumps over the lazy dog while light falls on northern hills and every small window glows"
      }"##;

fn listview_delegates(text: &str) -> (Budget, usize) {
    listview_delegates_to(text, 1000.0)
}

/// What growing the window to `height` made: the view's new rows.
fn listview_delegates_to(text: &str, height: f64) -> (Budget, usize) {
    let e = engine();
    let root = load(
        &e,
        &format!(
            r##"import QtQuick
Rectangle {{
  width: 400; height: 200
  ListView {{
    objectName: "view"
    anchors.fill: parent
    cacheBuffer: 0
    model: 10000
    delegate: Rectangle {{
      id: row
      required property int index
      width: ListView.view.width
      height: 40
      color: index % 2 ? "#eeeeee" : "white"
      {text}
    }}
  }}
}}
"##
        ),
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
        e.set(root, Item::HEIGHT, height).unwrap();
    });
    (b, shown(&e, view) - before)
}

/// An Item holding a Text, made by `Component.createObject` ten at a time.
fn plain_items() -> (Budget, usize) {
    plain_items_times(1)
}

fn plain_items_times(times: usize) -> (Budget, usize) {
    let (e, mut s, root, _) = plain_scene();
    let before = e.children(root).len();
    let make = e.method_index(root, "make").unwrap();
    let b = spent(&e, &mut s, 2000, || {
        for _ in 0..times {
            e.invoke(root, make, &[]).unwrap();
        }
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
    /// Heap bytes destroying them gave back.
    freed: i64,
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
        frame_at(&mut s, &e, 4000 + i * 16);
    }
    let b = now(&e);
    let nodes = |c: Census| c.values + c.bindings + c.effects;
    Teardown {
        objects: a.objects - b.objects,
        scopes: a.graph.scopes - b.graph.scopes,
        nodes: nodes(a.graph) - nodes(b.graph),
        allocations: b.allocations - a.allocations,
        freed: a.bytes - b.bytes,
    }
}

#[test]
fn ten_listview_delegates() {
    let _alone = alone();
    let (b, made) = listview_delegates(ONE_LINE);
    assert_eq!(made, 10);
    // Per delegate: 3 objects (the Rectangle, the Text and what they hold
    // attached or grouped), 4 values, 6 bindings, 3 effects, 2 scopes,
    // no member or type lookups, 120.9 allocations. What the component
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
    // layers (twenty-five fewer, B27);
    // only the Text is anchored, and only an anchored item has an anchors
    // effect (the Rectangle's, and nineteen fewer). An item's place and
    // geometry are marks its properties' change hooks set, read at the
    // frame, where each was an effect (four effects and ten allocations
    // fewer a delegate, B28).
    // A text is shaped by harfrust as Parley drives it once its font is set
    // up, which sets up the font's lookups as texts first use them (17
    // more here, the rows' digits being new; none once they are, TEXT5).
    // What an item inherits, `visible` and `enabled`, is passed down by
    // its parent's change hooks, where each item had an effect that read
    // its parent's: an effect, a value and two allocations fewer a
    // delegate (B29). A text's inputs are shared between its marks and its
    // paint, one box where both held a copy (one more a delegate), and the
    // side maps of what marks carry grew (four more). The scene's list of
    // what moved, reused between frames, makes one fewer (MIR). 26.6 kB
    // kept a delegate: the step that grew those maps pays for their room
    // ([`bytes_a_thousand_instances_keep`]).
    // What an item shows of its children is matched to them when their
    // list's change hook, a Loader's, the end of an incubation or a
    // destruction marks it, where each item had an effect reading them:
    // two effects and 5.6 allocations fewer a delegate (B30). An item's
    // `parent` and `focus` are acted on by their change hooks, where each
    // item had an effect watching each, which made it a value too: four
    // effects, four values and 5.1 allocations fewer a delegate (B30).
    // Anchors hold from an item's completion, as Qt's do, not from the
    // first frame: an item completed before its parent waits for it in a
    // table the delegates share (96 bytes, no allocation more).
    assert_eq!(
        b,
        Budget {
            objects: 30,
            graph: Census {
                values: 40,
                bindings: 60,
                effects: 30,
                scopes: 20,
            },
            lookups: Lookups {
                members: 0,
                types: 0,
                names: 0,
                atoms: 0,
            },
            allocations: 1208,
            bytes: 151496,
        }
    );
}

#[test]
fn ten_wrapped_listview_delegates() {
    let _alone = alone();
    let (b, made) = listview_delegates(WRAPPED);
    assert_eq!(made, 10);
    // Per delegate: 3 objects, 2 values, 5 bindings, 2 effects, 2
    // scopes; 123.2 allocations. The two enumerations its text names,
    // `Text.WordWrap` and `Text.ElideRight`, are looked up as a type once
    // for the file, not once a delegate (twenty fewer lookups, TEXT5).
    // The text is broken into lines and its last line elided without
    // Parley's builder or the layouts it makes (32 fewer a delegate,
    // TEXT4).
    // The scene's one record an item, and an anchors effect only for an
    // anchored item, make 82 fewer and 20 fewer effects (B27). Place and
    // geometry as marks a change hook sets make 4 fewer effects a
    // delegate (B28).
    // Its text is shaped by harfrust as Parley drives it once the font is
    // set up, which sets the font's lookups up as texts first use them (56
    // more here, in the first rows; a fifth of an allocation a text once a
    // scroll has warmed the font, TEXT5).
    // What it inherits passed down by its parent's change hooks, where it
    // had an effect, makes 30 fewer, 20 fewer values and 20 fewer effects
    // (B29), and a text's inputs in one box its marks and paint share, 14
    // more. Its structure marked by change hooks, where each item had an
    // effect, makes 20 fewer effects and 57 fewer allocations (B30), and
    // `parent` and `focus` acted on by change hooks, not watched, 40 fewer
    // effects, 40 fewer values and 49 fewer allocations. The graph's nodes
    // in pages of 1024 that stay where they are, not in one vector that
    // doubles, make one fewer and keep 53 kB less, the doubling's spare
    // room (MEM3). The scene's list of what moved, reused, one fewer (MIR).
    // 23.2 kB kept a delegate.
    assert_eq!(
        b,
        Budget {
            objects: 30,
            graph: Census {
                values: 20,
                bindings: 50,
                effects: 20,
                scopes: 20,
            },
            lookups: Lookups {
                members: 0,
                types: 0,
                names: 0,
                atoms: 0,
            },
            allocations: 1230,
            bytes: 203793,
        }
    );
}

#[test]
fn ten_plain_items() {
    let _alone = alone();
    let (b, made) = plain_items();
    assert_eq!(made, 10);
    // Per instance: 2 objects, no values, 2 bindings, 1 effect, 2 scopes,
    // 1 member lookup (the property `createObject` is given, by its name)
    // and no type lookups, 67.4 allocations; the id (`item`) is found
    // by slot. One of the ten grows what keeps a text's lines' items
    // between measuring and painting. A one-line plain text is laid out
    // without Parley's builder (fifteen fewer over the ten), and B26 put
    // each binding in one box (thirty fewer). B27's one scene record an
    // item took twenty-eight off, and an anchors effect only for an item
    // with an anchor, which neither has, forty-nine more. B28's marks for
    // an item's place and geometry, where it had an effect each, took
    // seventy-three more. B29 passes down what an item inherits from its
    // parent's change hooks, where each item had an effect reading it:
    // twenty effects, twenty values and forty-seven allocations fewer,
    // and shares a text's inputs between its marks and its paint in one
    // box, thirteen more. A call keeps which of its arguments are data in
    // bits, not a list (ten fewer, B12b). B30 marks an item's structure
    // from change hooks and the end of an incubation or a destruction,
    // where each item had an effect: twenty effects and fifty-five
    // allocations fewer, and acts on `parent` and `focus` from their
    // change hooks, where each item watched both: forty effects, forty
    // values and sixty allocations fewer. The graph's nodes in pages, not
    // a vector that doubles, two fewer and 14 kB less (MEM3). The scene's
    // list of what moved, reused, one fewer (MIR). 11.2 kB kept an instance.
    assert_eq!(
        b,
        Budget {
            objects: 20,
            graph: Census {
                values: 0,
                bindings: 20,
                effects: 10,
                scopes: 20,
            },
            lookups: Lookups {
                members: 10,
                types: 0,
                names: 0,
                atoms: 0,
            },
            allocations: 671,
            bytes: 120136,
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
    // it from 13.7 by holding the walks over the dead inline. Two nodes
    // fewer an instance since B27: neither item is anchored, and has no
    // anchors effect to let go of; four fewer since B28, which keeps an
    // item's place and geometry as marks and not as two effects an item,
    // four fewer since B29, which has no inherit effect an item, and four
    // fewer since B30, which has no structure effect an item, and eight
    // fewer for the `parent` and `focus` it no longer watches. Three
    // allocations fewer since the scene sweeps the dead its destroy hook
    // lists, not a list it gathered by walking all its items. 6.9 kB
    // given back an instance; the rest of what one kept is room the next
    // ones use.
    assert_eq!(
        plain_teardown(),
        Teardown {
            objects: 20,
            scopes: 20,
            nodes: 30,
            allocations: 53,
            freed: 68734,
        }
    );
}

/// Heap bytes an instance keeps, over a thousand: the pins of ten see
/// the step in which a list or a map grew, where a thousand spread it as
/// an application that makes many pays it.
#[derive(Debug, PartialEq, Eq)]
struct Kept {
    one_line: i64,
    wrapped: i64,
    plain: i64,
}

#[test]
fn bytes_a_thousand_instances_keep() {
    let _alone = alone();
    let each = |(b, made): (Budget, usize)| {
        assert_eq!(made, 1000);
        b.bytes / made as i64
    };
    // A row is 40 high, and the window was 600.
    let kept = Kept {
        one_line: each(listview_delegates_to(ONE_LINE, 40600.0)),
        wrapped: each(listview_delegates_to(WRAPPED, 40600.0)),
        plain: each(plain_items_times(100)),
    };
    // B29 holds what is large and rare apart from the marks and record
    // every item has (688 and 576 bytes, now 56 and 328): 27.3, 29.3
    // and 17.1 kB before; B12b's call path keeps eight or nine more.
    // B30 drops the structure effect of each item (26.0, 28.9 and 15.9 kB
    // before), and the watches of `parent` and `focus` with the values
    // they made (25.9, 25.1 and 15.8 kB before). The graph's nodes in
    // pages, not a vector that doubles, leave out its spare room (22.0,
    // 25.0 and 11.7 kB before, MEM3). MIR's smaller record, 264 bytes
    // where it was 328, leaves 0.1 kB less each (21.1, 23.3 and 11.3 kB
    // before). An item without the accessible record nothing read keeps
    // 48 bytes less (20.9, 23.2 and 11.2 kB before). Anchors held from
    // completion, not from the first frame, leave the anchored rows 90
    // bytes less each (20.9 kB before). The rest is mostly
    // the graph's nodes, the objects and the text's runs.
    assert_eq!(
        kept,
        Kept {
            one_line: 20783,
            wrapped: 23147,
            plain: 11110,
        }
    );
}

/// A hundred rows a script gives a ListModel no view shows, each an object
/// literal of one number: what `append` costs beyond the script's own loop.
fn hundred_appends() -> Budget {
    let e = engine();
    let root = load(
        &e,
        r##"import QtQuick
Item {
  width: 100; height: 100
  ListModel { id: m }
  function fill() { for (var i = 0; i < 100; i++) m.append({ n: i }) }
}
"##,
    );
    let mut s = scene(&e, root, 100.0, 100.0);
    settle(&mut s, &e, 0);
    // Warm: the first rows make the role and what every later row shares.
    let fill = e.method_index(root, "fill").unwrap();
    e.invoke(root, fill, &[]).unwrap();
    settle(&mut s, &e, 1000);
    spent(&e, &mut s, 2000, || {
        e.invoke(root, fill, &[]).unwrap();
    })
}

#[test]
fn hundred_list_model_appends() {
    let _alone = alone();
    // Per row: no lookup, and 7.02 allocations. A model nobody connected
    // to sends no row signals without finding them by name (two member
    // lookups fewer, B12), and one row is held without a list of its own
    // on its way in (two fewer). Of the seven, the row's own are its
    // entries, its role's name and its values; the rest is the call's
    // (the script's object kept for it, its arguments) and the rows' list
    // growing. B12b's call path, which takes the arguments it is given
    // and not copies, took twelve off over the hundred.
    assert_eq!(
        hundred_appends(),
        Budget {
            objects: 0,
            graph: Census {
                values: 0,
                bindings: 0,
                effects: 0,
                scopes: 0,
            },
            lookups: Lookups {
                members: 0,
                types: 0,
                names: 0,
                atoms: 0,
            },
            allocations: 702,
            bytes: 7744,
        }
    );
}
