//! Spike for research note 13: the same eight cases run four ways.
//!
//! * tier 1 and QuickJS: `qml/World.qml`, the cases written in QML;
//! * back end B: `qml/Shell.qml` plus Rust generated from `cases/*.ts`;
//! * shipped (`aot`): `qml/World.qml` with the code the shipped-file
//!   generator (`ljos_aot`, milestone A1) made from it at build time, the
//!   dynamic tiers running what it refused;
//! * back end A (feature `scriptc`): `qml/Shell.qml` plus a native library
//!   scriptc made from `scriptc/cases.ts`, called through FFI.
//!
//! Handlers are connected through a [`Slot`], so that one can be swapped
//! for its dynamic version at run time without moving it in the
//! connection order (the fallback measurement).

#![allow(missing_debug_implementations)]

pub mod rt;

#[cfg(feature = "scriptc")]
pub mod a;

#[allow(non_snake_case, unused_labels, unused_parens, clippy::all)]
pub mod b {
    include!(concat!(env!("OUT_DIR"), "/b_cases.rs"));
}

/// What the shipped-file generator made of World.qml (`COMPILED[0]`) and
/// Bench.qml (`COMPILED[1]`).
pub mod shipped {
    include!(concat!(env!("OUT_DIR"), "/shipped.rs"));
}

use std::cell::RefCell;
use std::rc::Rc;

use ljos_compiler::{Project, compile_file};
use ljos_engine::{
    Cx, Engine, EngineError, MapSource, ObjectId, PropIndex, QmlList, QmlObject, QmlType, QtObject,
    SignalIndex, Value,
};
use qml_front::QtTypeOracle;

pub const WORLD: &str = include_str!("../qml/World.qml");
pub const SHELL: &str = include_str!("../qml/Shell.qml");
pub const BENCH: &str = include_str!("../qml/Bench.qml");
pub const SWAP: &str = include_str!("../qml/Swap.qml");

/// `Item` as the engine's own tests stand it in: the real one lives in
/// `ljos_quick`, which would bring a renderer the cases do not need.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "Item", extends = QtObject, default = "data")]
pub struct Item {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub width: f64,
    #[qml(list, append = item_data_append)]
    pub data: QmlList,
    #[qml(list)]
    pub children: QmlList,
    #[qml(list)]
    pub resources: QmlList,
}

fn item_data_append(cx: &Cx, child: ObjectId) -> Result<(), EngineError> {
    let e = cx.engine;
    let is_item = e
        .meta_of(child)
        .is_some_and(|m| m.inherits(Item::static_meta()));
    if is_item {
        e.list_push(cx.this, Item::CHILDREN.index, child)?;
        e.set_visual_parent(child, Some(cx.this))?;
    } else {
        e.list_push(cx.this, Item::RESOURCES.index, child)?;
    }
    e.list_push(cx.this, Item::DATA.index, child)
}

/// A binding or a handler one back end gives for one export.
pub enum Export {
    Binding(ObjectId, PropIndex, Box<dyn FnMut(&Engine) -> Value>),
    Handler(ObjectId, SignalIndex, Handler),
}

pub type Handler = Rc<dyn Fn(&Engine, &[Value])>;

/// A handler that can be replaced where it stands in the connection order.
pub type Slot = Rc<RefCell<Handler>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Tier1,
    QuickJs,
    Shipped,
    B,
    #[cfg(feature = "scriptc")]
    A,
}

impl Mode {
    pub fn all() -> Vec<Mode> {
        vec![
            Mode::Tier1,
            Mode::QuickJs,
            Mode::Shipped,
            #[cfg(feature = "scriptc")]
            Mode::A,
            Mode::B,
        ]
    }
    pub fn name(self) -> &'static str {
        match self {
            Mode::Tier1 => "tier1",
            Mode::QuickJs => "quickjs",
            Mode::Shipped => "aot",
            Mode::B => "b",
            #[cfg(feature = "scriptc")]
            Mode::A => "a",
        }
    }
    pub fn parse(s: &str) -> Option<Mode> {
        Mode::all().into_iter().find(|m| m.name() == s)
    }
}

pub fn engine() -> Engine {
    let e = Engine::new();
    e.register_module(ljos_engine::module!("QtQuick" 2.0 => [Item]));
    e
}

pub fn compile(files: &[(&str, &str)]) -> MapSource {
    let mut project = Project::new(QtTypeOracle);
    for (k, s) in files {
        project.add(k, *s);
    }
    let mut source = MapSource::new();
    for (k, _) in files {
        let c = compile_file(&project, k).unwrap_or_else(|d| panic!("{k}: {d:?}"));
        assert!(!c.has_errors(), "{k}: {:?}", c.diagnostics);
        source.insert(*k, c.file);
    }
    source
}

/// A loaded world: the engine, its root, and the handler slots a back end
/// installed, by export name.
pub struct Run {
    pub engine: Engine,
    pub root: ObjectId,
    pub slots: Vec<(String, Slot)>,
    /// The objects the steps write and emit on, found once: Qt's driver
    /// holds its pointers to them too, so a step measures the same work.
    pub held: Held,
}

/// The world's objects, by their objectName.
#[derive(Clone, Copy, Debug)]
pub struct Held {
    pub bar: ObjectId,
    pub editor: ObjectId,
    pub field: ObjectId,
    pub cards: ObjectId,
}

/// Loads the world for `mode`. `check` turns on tier 1's cross-check
/// against QuickJS where tier 1 runs.
pub fn load(mode: Mode, check: bool) -> Run {
    let e = engine();
    e.set_tier1(mode != Mode::QuickJs);
    e.set_tier1_check(check);
    if mode == Mode::Shipped {
        e.register_compiled(&shipped::COMPILED[..1])
            .expect("a new engine has run no file");
        e.set_aot_check(check);
    }
    let file = match mode {
        Mode::Tier1 | Mode::QuickJs | Mode::Shipped => "World",
        _ => "Shell",
    };
    e.set_component_source(compile(&[
        ("World", WORLD),
        ("Shell", SHELL),
        ("Swap", SWAP),
    ]));
    let root = e
        .create_from_url(file, None, Vec::new())
        .expect("the world creates");
    let find = |name: &str| rt::find(&e, root, name).expect("an object of the world");
    let held = Held {
        bar: find("bar"),
        editor: find("editor"),
        field: find("field"),
        cards: find("cards"),
    };
    let mut run = Run {
        engine: e,
        root,
        slots: Vec::new(),
        held,
    };
    match mode {
        Mode::Tier1 | Mode::QuickJs | Mode::Shipped => {}
        Mode::B => {
            let w = b::World::resolve(&run.engine, root).expect("B resolves the world");
            for (_, name) in b::EXPORTS {
                let ex = b::export(&w, name).expect("an export B generated");
                run.install(name, ex);
            }
        }
        #[cfg(feature = "scriptc")]
        Mode::A => {
            let exports = a::exports(&run.engine, root).expect("A resolves the world");
            for (name, ex) in exports {
                run.install(name, ex);
            }
        }
    }
    // The dynamic world runs onItemsChanged once as it is made (the var's
    // first list counts as a change); a back end installed after creation
    // does not see that. Both start counting from here.
    run.engine
        .write_by_name(root, "syncs", Value::Int(0))
        .expect("syncs");
    run
}

impl Run {
    pub fn install(&mut self, name: &str, ex: Export) {
        let e = &self.engine;
        match ex {
            Export::Binding(o, p, mut f) => e.bind(o, p, move |e| f(e)).expect("bind"),
            Export::Handler(o, s, h) => {
                let slot: Slot = Rc::new(RefCell::new(h));
                let at = slot.clone();
                e.connect(o, s, move |e, a| {
                    let h = at.borrow().clone();
                    h(e, a);
                })
                .expect("connect");
                self.slots.push((name.to_owned(), slot));
            }
        }
    }

    pub fn find(&self, name: &str) -> ObjectId {
        match name {
            "root" => self.root,
            "bar" => self.held.bar,
            "editor" => self.held.editor,
            "field" => self.held.field,
            "cards" => self.held.cards,
            _ => rt::find(&self.engine, self.root, name).expect("an object of the world"),
        }
    }

    pub fn set(&self, obj: &str, prop: &str, v: Value) {
        self.set_on(self.find(obj), prop, v);
    }

    /// A write by the property's name, as Qt's driver's `setProperty`.
    pub fn set_on(&self, o: ObjectId, prop: &str, v: Value) {
        self.engine.write_by_name(o, prop, v).expect("write");
    }

    pub fn get(&self, obj: &str, prop: &str) -> Value {
        let o = self.find(obj);
        self.engine.plain(&self.engine.read_by_name(o, prop))
    }

    /// Replaces the compiled handler of `picked` and the compiled binding of
    /// `progress` with their dynamic versions, compiled from text now: the
    /// fallback a build takes when one case's compiled code is wrong or
    /// stale.
    pub fn swap_to_dynamic(&mut self) {
        let e = &self.engine;
        let bar = self.find("bar");
        let swap = e
            .create_from_url(
                "Swap",
                None,
                vec![
                    ("r".into(), Value::Object(self.root)),
                    ("b".into(), Value::Object(bar)),
                ],
            )
            .expect("Swap creates");
        let slot = &self
            .slots
            .iter()
            .find(|(n, _)| n == "root__onPicked")
            .expect("a picked handler")
            .1;
        *slot.borrow_mut() = Rc::new(move |e: &Engine, a: &[Value]| {
            e.invoke_by_name(swap, "onPicked", a)
                .expect("the dynamic onPicked runs");
        });
        e.invoke_by_name(swap, "bindProgress", &[])
            .expect("the dynamic progress binds");
    }
}

/// What is observed of a run: each signal of the world as it is emitted
/// and each property of `root` as it changes, with its value then.
pub fn trace(run: &Run) -> Rc<RefCell<Vec<String>>> {
    let log = Rc::new(RefCell::new(Vec::new()));
    let e = &run.engine;
    let root = run.root;
    for p in [
        "lastMode",
        "selected",
        "ran",
        "cursor",
        "depth",
        "accepted",
        "syncs",
        "progress",
        "statusText",
        "runText",
    ] {
        let l = log.clone();
        let idx = e.property_index(root, p).expect("a property");
        e.connect_by_name(root, &format!("{p}Changed"), move |e, _| {
            let v = e.plain(&e.read(root, idx));
            l.borrow_mut().push(format!("{p}={}", show(&v)));
        })
        .expect("connect");
    }
    for (obj, sig) in [
        ("root", "clicked"),
        ("root", "dispatched"),
        ("root", "picked"),
        ("root", "keyPressed"),
        ("editor", "submitted"),
    ] {
        let l = log.clone();
        let o = run.find(obj);
        e.connect_by_name(o, sig, move |_, a| {
            let a: Vec<String> = a.iter().map(show).collect();
            l.borrow_mut().push(format!("{obj}.{sig}({})", a.join(",")));
        })
        .expect("connect");
    }
    let cards = run.find("cards");
    let l = log.clone();
    e.connect_by_name(cards, "countChanged", move |e, _| {
        l.borrow_mut().push(format!(
            "cards.count={}",
            e.read_by_name(cards, "count").to_number()
        ));
    })
    .expect("connect");
    log
}

pub fn show(v: &Value) -> String {
    match v {
        Value::List(l) => format!("[{}]", l.iter().map(show).collect::<Vec<_>>().join(",")),
        Value::String(s) => format!("{:?}", s.as_str()),
        v => v.to_js_string().as_str().to_owned(),
    }
}

/// The toast ids the model holds, in order.
pub fn cards(run: &Run) -> Vec<f64> {
    let e = &run.engine;
    let cards = run.find("cards");
    let n = e.read_by_name(cards, "count").to_number() as usize;
    (0..n)
        .map(|i| {
            let row = e
                .invoke_by_name(cards, "get", &[Value::Real(i as f64)])
                .expect("get");
            rt::row_num(e, &row, "toastId")
        })
        .collect()
}

/// The cases and what drives them: each sets its inputs for step `k`
/// (untimed) and then triggers the case (timed).
pub const CASES: &[&str] = &["c1", "c2", "c3", "c4", "c5", "c6", "c7", "c8"];

pub fn prepare(run: &Run, case: &str, k: usize) {
    let s = |v: &str| Value::String(v.into());
    match case {
        "c1" => {
            run.set_on(run.root, "total", Value::Real([10.0, 0.0, 7.0][k % 3]));
            run.set_on(run.root, "done", Value::Real((k % 11) as f64));
        }
        "c2" => run.set_on(run.root, "stopMode", Value::Bool(k.is_multiple_of(2))),
        "c4" => run.set_on(
            run.root,
            "status",
            s(["todo", "doing", "blocked", "done", "dropped", "archived"][k % 6]),
        ),
        "c5" => {
            let changes: &[bool] = [&[false, true, false][..], &[], &[true], &[false]][k % 4];
            run.set_on(
                run.root,
                "changes",
                Value::List(std::sync::Arc::new(
                    changes.iter().map(|b| Value::Bool(*b)).collect(),
                )),
            );
            run.set_on(run.root, "health", Value::Bool(k.is_multiple_of(3)));
            run.set_on(
                run.root,
                "runStatus",
                s(if k % 5 == 4 { "doing" } else { "proposed" }),
            );
        }
        "c7" => {
            run.set_on(run.root, "depth", Value::Real((k % 3) as f64));
            run.set_on(
                run.held.field,
                "text",
                s(if k.is_multiple_of(2) { "" } else { "x" }),
            );
        }
        _ => {}
    }
}

/// The toast id lists case c8 steps through.
pub fn items(k: usize) -> Vec<f64> {
    match k % 5 {
        0 => vec![1.0, 2.0, 3.0],
        1 => vec![3.0, 1.0],
        2 => vec![4.0, 1.0, 2.0, 5.0],
        3 => vec![],
        _ => vec![5.0, 4.0, 3.0, 2.0, 1.0, 6.0],
    }
}

pub fn trigger(run: &Run, case: &str, k: usize) {
    let e = &run.engine;
    let s = |v: &str| Value::String(v.into());
    match case {
        "c1" => {
            std::hint::black_box(e.read_by_name(run.root, "progress"));
        }
        "c2" => e.emit_by_name(run.root, "clicked", &[]).expect("emit"),
        "c3" => {
            let editor = run.held.editor;
            e.emit_by_name(
                editor,
                "submitted",
                &[s(["foreground", "background"][k % 2])],
            )
            .expect("emit");
        }
        "c4" => {
            std::hint::black_box(e.read_by_name(run.root, "statusText"));
        }
        "c5" => {
            std::hint::black_box(e.read_by_name(run.root, "runText"));
        }
        "c6" => e
            .emit_by_name(run.root, "picked", &[s(["a", "b", "c", "a", "b"][k % 5])])
            .expect("emit"),
        "c7" => {
            let (key, mods) = [
                (0x31 + (k % 9) as i32, 0x0400_0000),
                (0x0100_0015, 0),
                (0x0100_0003, 0),
                (0x41, 0),
            ][k % 4];
            e.emit_by_name(run.root, "keyPressed", &[Value::Int(key), Value::Int(mods)])
                .expect("emit");
        }
        "c8" => {
            let v = items(k).into_iter().map(Value::Real).collect();
            run.set_on(run.root, "items", Value::List(std::sync::Arc::new(v)));
        }
        other => panic!("no case {other}"),
    }
}

/// What the host does after a step's work, outside the clock: the objects
/// the step let go of (rows a model removed) are deleted, as a frame's
/// turn deletes them. Without it the queue only grows, which no app does.
pub fn turn(run: &Run) {
    run.engine.process_deferred_deletes();
}

/// Every case in turn for `steps` steps.
pub fn drive(run: &Run, steps: std::ops::Range<usize>) {
    for k in steps {
        for c in CASES {
            prepare(run, c, k);
            trigger(run, c, k);
            turn(run);
        }
    }
}

/// What a property of every case holds at the end.
pub fn snapshot(run: &Run) -> Vec<String> {
    let mut out: Vec<String> = [
        "progress",
        "statusText",
        "runText",
        "lastMode",
        "selected",
        "ran",
        "cursor",
        "depth",
        "accepted",
        "syncs",
    ]
    .iter()
    .map(|p| format!("{p}={}", show(&run.get("root", p))))
    .collect();
    out.push(format!("cards={:?}", cards(run)));
    out
}
