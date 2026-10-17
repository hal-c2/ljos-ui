//! Shared helpers: build item trees with the native Engine API.

#![allow(dead_code)]

use ljos_engine::{Engine, ObjectId, QmlType, Value};
use ljos_quick::{Item, QuickScene, add_data};
use ljos_render::kurbo;
use ljos_runtime::Time;

pub fn engine() -> Engine {
    let e = Engine::new();
    ljos_quick::register(&e);
    e
}

/// Creates a `T` under `parent` (owner and, for items, visual parent).
pub fn make<T: QmlType + Default>(e: &Engine, parent: Option<ObjectId>) -> ObjectId {
    let o = e.create_native(T::default(), parent).unwrap();
    if let Some(p) = parent {
        add_data(e, p, o).unwrap();
    }
    e.complete(o).unwrap();
    o
}

/// Sets the font of `o` to 14 px. Measuring tests hold numbers that Qt gives
/// for the bundled font at that size, not at the default size.
pub fn at_14_px(e: &Engine, o: ObjectId) {
    let font = ljos_engine::Font {
        pixel_size: 14,
        point_size: -1.0,
        ..Default::default()
    };
    set(e, o, "font", Value::Font(std::sync::Arc::new(font)));
}

/// Sets `x`, `y`, `width`, `height`.
pub fn geom(e: &Engine, o: ObjectId, x: f64, y: f64, w: f64, h: f64) {
    e.set(o, Item::X, x).unwrap();
    e.set(o, Item::Y, y).unwrap();
    e.set(o, Item::WIDTH, w).unwrap();
    e.set(o, Item::HEIGHT, h).unwrap();
}

pub fn set(e: &Engine, o: ObjectId, name: &str, v: Value) {
    e.write_by_name(o, name, v)
        .unwrap_or_else(|err| panic!("{name}: {err}"));
}

pub fn get(e: &Engine, o: ObjectId, name: &str) -> Value {
    e.read_by_name(o, name)
}

pub fn num(e: &Engine, o: ObjectId, name: &str) -> f64 {
    get(e, o, name).to_number()
}

/// `(x, y, width, height)`.
pub fn rect(e: &Engine, o: ObjectId) -> (f64, f64, f64, f64) {
    (
        num(e, o, "x"),
        num(e, o, "y"),
        num(e, o, "width"),
        num(e, o, "height"),
    )
}

pub fn scene(e: &Engine, root: ObjectId, w: f64, h: f64) -> QuickScene {
    let mut s = QuickScene::new(kurbo::Size::new(w, h));
    s.set_root(e, root).unwrap();
    s
}

pub fn frame(s: &mut QuickScene, e: &Engine) -> ljos_quick::FrameOutput {
    s.frame(e, Time::ZERO)
}

/// Compiles `files` (`("Main", source)`) for `e` to load.
pub fn compile(e: &Engine, files: &[(&str, &str)]) {
    use ljos_compiler::{Project, compile_file};
    let mut project = Project::new(qml_front::QtTypeOracle);
    for (k, s) in files {
        project.add(k, *s);
    }
    let mut source = ljos_engine::MapSource::new();
    for (k, _) in files {
        let c = compile_file(&project, k).unwrap_or_else(|d| panic!("{k}: {d:?}"));
        assert!(!c.has_errors(), "{k}: {:?}", c.diagnostics);
        source.insert(*k, c.file);
    }
    e.set_component_source(source);
}

/// Compiles `source` as `Main.qml` and creates it.
pub fn load(e: &Engine, source: &str) -> ObjectId {
    compile(e, &[("Main", source)]);
    e.create_from_url("Main", None, Vec::new())
        .unwrap_or_else(|err| panic!("Main.qml: {err}"))
}

/// The first object below `root` (depth first, by QObject ownership)
/// whose objectName is `name`.
pub fn find(e: &Engine, root: ObjectId, name: &str) -> ObjectId {
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if e.read_by_name(o, "objectName").to_js_string().as_str() == name {
            return o;
        }
        let mut kids = e.children(o);
        kids.reverse();
        stack.extend(kids);
    }
    panic!("no object named {name}")
}

/// The objects in list property `name` of `o`.
pub fn list(e: &Engine, o: ObjectId, name: &str) -> Vec<ObjectId> {
    let p = e
        .meta_of(o)
        .and_then(|m| m.property_index(name))
        .unwrap_or_else(|| panic!("no list {name}"));
    (0..e.list_count(o, p))
        .filter_map(|i| e.list_at(o, p, i))
        .collect()
}
