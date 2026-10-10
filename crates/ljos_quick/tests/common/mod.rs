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
