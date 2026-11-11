//! What a closed popup costs, counted exactly: engine objects, graph
//! nodes by kind, heap allocations and the bytes they keep, for one
//! instance that is never opened, and for opening it the first time.
//!
//! An application has many more popups than it ever shows at once (a
//! menu per menu-bar entry, a tool tip per button, a combo box per row),
//! so what one costs while closed is paid by every one of them.
//!
//! The counts are per thread, and each case makes its controls once
//! before it counts, so the caches of the process are warm.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use ljos_engine::{Engine, ObjectId};
use ljos_quick::QuickScene;
use ljos_render::kurbo;
use ljos_runtime::Time;

struct Counting;

thread_local! {
    // A const initialiser and no destructor: the allocator may run while
    // the thread's locals are being torn down.
    static ALLOCS: Cell<u64> = const { Cell::new(0) };
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

fn alone() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
}

/// What a step added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Budget {
    objects: i64,
    values: i64,
    bindings: i64,
    effects: i64,
    scopes: i64,
    allocations: i64,
    bytes: i64,
}

fn now(e: &Engine) -> Budget {
    let c = e.graph().census();
    Budget {
        objects: e.object_count() as i64,
        values: c.values as i64,
        bindings: c.bindings as i64,
        effects: c.effects as i64,
        scopes: c.scopes as i64,
        allocations: ALLOCS.with(Cell::get) as i64,
        bytes: LIVE.with(Cell::get),
    }
}

fn between(a: Budget, b: Budget) -> Budget {
    Budget {
        objects: b.objects - a.objects,
        values: b.values - a.values,
        bindings: b.bindings - a.bindings,
        effects: b.effects - a.effects,
        scopes: b.scopes - a.scopes,
        allocations: b.allocations - a.allocations,
        bytes: b.bytes - a.bytes,
    }
}

/// `a`, less `b`, over `n`: what one more instance costs.
fn per(a: Budget, b: Budget, n: i64) -> Budget {
    let d = between(b, a);
    Budget {
        objects: d.objects / n,
        values: d.values / n,
        bindings: d.bindings / n,
        effects: d.effects / n,
        scopes: d.scopes / n,
        allocations: d.allocations / n,
        bytes: d.bytes / n,
    }
}

const N: i64 = 10;

fn engine() -> Engine {
    let e = Engine::new();
    ljos_quick::register(&e);
    ljos_controls::register(&e);
    e
}

/// `Main` holds `control` once, `Ten` ten times, `None` not at all.
fn compile(e: &Engine, control: &str) {
    use ljos_compiler::{Project, compile_file};
    let mut project = Project::new(qml_front::QtTypeOracle);
    let file = |n: i64| {
        let mut s = String::from(
            "import QtQuick\nimport QtQuick.Controls.Basic\nItem {\n  width: 400; height: 400\n",
        );
        for i in 0..n {
            s.push_str(&format!(
                "  {}\n",
                control.replace("NAME", &format!("c{i}"))
            ));
        }
        s.push_str("}\n");
        s
    };
    project.add("One", file(1));
    project.add("Ten", file(N));
    project.add("None", file(0));
    let mut files = ljos_engine::MapSource::new();
    for name in ["One", "Ten", "None"] {
        let c = compile_file(&project, name).unwrap_or_else(|d| panic!("{d:?}"));
        assert!(!c.has_errors(), "{:?}", c.diagnostics);
        files.insert(name, c.file);
    }
    e.set_component_source(files);
}

fn scene(e: &Engine, root: ObjectId) -> QuickScene {
    let mut s = QuickScene::new(kurbo::Size::new(400.0, 400.0));
    s.set_root(e, root).unwrap();
    s
}

fn settle(s: &mut QuickScene, e: &Engine) {
    ljos_drive::settle_at(e, s, Time::ZERO).unwrap();
}

/// Makes `name` and its scene, and lets it settle.
fn make(e: &Engine, name: &str) -> (ObjectId, QuickScene) {
    let root = e.create_from_url(name, None, Vec::new()).unwrap();
    let mut s = scene(e, root);
    settle(&mut s, e);
    (root, s)
}

/// What one closed `control` costs, and what opening it the first time
/// adds.
fn closed_and_opened(control: &str) -> (Budget, Budget) {
    let e = engine();
    compile(&e, control);
    // Warm: the types' metadata, the style's components, the scene's lists.
    let (warm, mut ws) = make(&e, "One");
    let c0 = ljos_drive::find_below(&e, warm, "c0").unwrap();
    e.invoke_by_name(c0, "open", &[]).unwrap();
    settle(&mut ws, &e);
    let a = now(&e);
    let (_none, _ns) = make(&e, "None");
    let b = now(&e);
    let (ten, mut ts) = make(&e, "Ten");
    let c = now(&e);
    let closed = per(between(b, c), between(a, b), N);
    // The first instance opened grows the engine's tables to fit; what an
    // instance adds is counted on the next.
    let open = |name: &str| {
        let p = ljos_drive::find_below(&e, ten, name).unwrap();
        e.invoke_by_name(p, "open", &[]).unwrap();
        p
    };
    let c0 = open("c0");
    settle(&mut ts, &e);
    e.invoke_by_name(c0, "close", &[]).unwrap();
    settle(&mut ts, &e);
    let before = now(&e);
    open("c1");
    settle(&mut ts, &e);
    let opened = between(before, now(&e));
    (closed, opened)
}

/// Reports what one closed `control` costs and what opening it adds, and
/// holds the closed one to at most `most` objects, values, bindings and
/// effects. Allocations and bytes follow the capacity of the engine's
/// tables, so they are reported and not held.
fn report(name: &str, control: &str, most: [i64; 4]) {
    let _alone = alone();
    let (closed, opened) = closed_and_opened(control);
    eprintln!("{name}: closed {closed:?}\n{name}: first open {opened:?}");
    let got = [
        closed.objects,
        closed.values,
        closed.bindings,
        closed.effects,
    ];
    assert!(
        got.iter().zip(most).all(|(g, m)| *g <= m),
        "{name} closed: objects, values, bindings, effects {got:?}, at most {most:?}"
    );
}

#[test]
fn a_closed_popup() {
    // Three of the objects and one binding are the style's shade: the
    // components of `T.Overlay.modal` and `.modeless`, and the attached
    // Overlay that holds them, as in every style that dims.
    report("Popup", r#"Popup { objectName: "NAME" }"#, [8, 20, 81, 28]);
}

#[test]
fn a_closed_tool_tip() {
    report(
        "ToolTip",
        r#"ToolTip { objectName: "NAME"; text: "tip" }"#,
        [5, 21, 86, 30],
    );
}

#[test]
fn a_closed_menu() {
    report(
        "Menu",
        r#"Menu { objectName: "NAME"; MenuItem { text: "a" } MenuItem { text: "b" } MenuItem { text: "c" } }"#,
        // With the style's shade, as a popup's.
        [47, 147, 320, 122],
    );
}

#[test]
fn a_closed_combo_box() {
    // The ComboBox's own `open` is its popup's: found through `popup`.
    report(
        "ComboBox",
        r#"ComboBox { model: ["a", "b", "c"]; popup.objectName: "NAME" }"#,
        [26, 104, 217, 114],
    );
}
