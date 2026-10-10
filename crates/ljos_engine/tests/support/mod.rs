//! QML test harness: compiles source strings with `ljos_compiler` and runs
//! them on an engine with small `QtQuick`-named test types. The real
//! QtQuick lives in `ljos_quick`; these stand-ins keep the engine's tests
//! independent of it.

#![allow(dead_code)]

use std::rc::Rc;
use std::sync::Arc;

use ljos_compiler::{Project, compile_file};
use ljos_engine::{
    Color, Cx, Engine, EngineError, LogEntry, LogLevel, MapSource, ObjectId, QmlList, QmlObject,
    QmlType, QtObject, Value, qml_impl,
};
use ljos_runtime::{Clock, Time, VirtualClock};
use qml_front::QtTypeOracle;

#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "Item", extends = QtObject, default = "data")]
pub struct Item {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub x: f64,
    #[qml(notify)]
    pub y: f64,
    #[qml(notify)]
    pub width: f64,
    #[qml(notify)]
    pub height: f64,
    #[qml(notify)]
    pub visible: bool,
    #[qml(list, append = item_data_append)]
    pub data: QmlList,
    #[qml(list)]
    pub children: QmlList,
    #[qml(list)]
    pub resources: QmlList,
}

/// Items become visual children; anything else is a resource.
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

#[qml_impl]
impl Item {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        // A native completion hook runs before any onCompleted handler.
        let _ = cx;
        Ok(())
    }
}

#[derive(Debug, Default, QmlObject)]
#[qml(name = "QQuickPen", uncreatable)]
pub struct Pen {
    #[qml(notify)]
    pub width: f64,
    #[qml(notify)]
    pub color: Color,
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "Rectangle", extends = Item)]
pub struct Rectangle {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub color: Color,
    #[qml(notify)]
    pub radius: f64,
    #[qml(group)]
    pub border: Pen,
}

/// A fresh engine with QtQuick test types and a virtual clock at 0.
pub struct Harness {
    pub engine: Engine,
    pub clock: Arc<VirtualClock>,
}

pub fn engine() -> Harness {
    let engine = Engine::new();
    engine.register_module(ljos_engine::module!("QtQuick" 2.0 => [Item, Rectangle]));
    let clock = Arc::new(VirtualClock::new());
    engine.set_clock(Rc::new(clock.clone()));
    Harness { engine, clock }
}

impl Harness {
    /// Compiles `files` (`("Main", source)`) and makes them loadable.
    pub fn files(&self, files: &[(&str, &str)]) {
        self.engine.set_component_source(compile(files));
    }

    /// Compiles `source` as `Main.qml` and creates it.
    pub fn load(&self, source: &str) -> ObjectId {
        self.files(&[("Main", source)]);
        self.engine
            .create_from_url("Main", None, Vec::new())
            .expect("Main.qml creates")
    }

    /// The first object (depth first from `root`) whose objectName is
    /// `name`.
    pub fn find(&self, root: ObjectId, name: &str) -> ObjectId {
        find(&self.engine, root, name).unwrap_or_else(|| panic!("no object named {name}"))
    }

    pub fn get(&self, obj: ObjectId, prop: &str) -> Value {
        self.engine.read_by_name(obj, prop)
    }

    pub fn num(&self, obj: ObjectId, prop: &str) -> f64 {
        self.get(obj, prop).to_number()
    }

    /// Assigns from JavaScript, as `obj.prop = value` would.
    pub fn set(&self, obj: ObjectId, prop: &str, value: Value) {
        self.engine.write_by_name(obj, prop, value).expect("write");
    }

    /// Lets `ms` milliseconds of application time pass.
    pub fn pass_ms(&self, ms: u64) {
        // In steps, as a host wakes at each deadline.
        let end = self.clock.now() + std::time::Duration::from_millis(ms);
        loop {
            match self.engine.next_timer_deadline() {
                Some(t) if t <= end => {
                    self.clock.set(t);
                    self.engine.advance_timers(t);
                }
                _ => break,
            }
        }
        self.clock.set(end);
        self.engine.advance_timers(end);
    }

    /// Console output and warnings so far, as text.
    pub fn log(&self) -> Vec<String> {
        self.engine
            .take_log()
            .into_iter()
            .map(|e| e.message)
            .collect()
    }

    pub fn log_entries(&self) -> Vec<LogEntry> {
        self.engine.take_log()
    }

    pub fn warnings(&self) -> Vec<String> {
        self.engine
            .take_log()
            .into_iter()
            .filter(|e| e.level >= LogLevel::Warning)
            .map(|e| e.message)
            .collect()
    }
}

pub fn find(engine: &Engine, root: ObjectId, name: &str) -> Option<ObjectId> {
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if engine.read_by_name(o, "objectName").to_js_string().as_str() == name {
            return Some(o);
        }
        let mut kids = engine.children(o);
        kids.reverse();
        stack.extend(kids);
    }
    None
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

pub fn time_ms(ms: u64) -> Time {
    Time::from_millis(ms)
}
