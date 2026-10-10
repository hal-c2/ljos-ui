//! Test types: an Item-like base, a Rectangle-like derived type with a
//! group, signals and methods, and an attached type.

#![allow(dead_code)]

use ljos_engine::{Color, Cx, EngineError, QmlObject, qml_impl};

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Test", name = "Item", default = "data")]
pub struct Item {
    #[qml(notify)]
    pub x: f64,
    #[qml(notify)]
    pub y: f64,
    #[qml(notify)]
    pub width: f64,
    #[qml(notify)]
    pub height: f64,
    #[qml(notify, readonly)]
    pub implicit_width: f64,
    #[qml(notify)]
    pub visible: bool,
    /// Not a property.
    #[qml(skip)]
    pub layout_passes: u32,
}

#[derive(Debug, Default, QmlObject)]
#[qml(name = "Border", uncreatable)]
pub struct Border {
    #[qml(notify)]
    pub width: f64,
    #[qml(notify)]
    pub color: Color,
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Test", name = "Rectangle", extends = Item)]
pub struct Rectangle {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub color: Color,
    #[qml(notify)]
    pub radius: f64,
    #[qml(group)]
    pub border: Border,
    #[qml(rename = "antialiasing")]
    pub smooth_edges: bool,
    #[qml(skip)]
    pub completed_runs: u32,
}

#[qml_impl]
impl Rectangle {
    #[qml(signal)]
    fn clicked(x: f64, y: f64);

    #[qml(signal, name = "doubleClicked")]
    fn double_click();

    /// Reads fields only: borrows the struct.
    #[qml(invokable)]
    fn area(&self) -> f64 {
        self.item.width * self.item.height
    }

    /// Goes through the engine: may run bindings and handlers.
    #[qml(invokable)]
    fn grow(cx: &Cx, by: f64) -> Result<f64, EngineError> {
        let r = cx.get(Rectangle::RADIUS)? + by;
        cx.set(Rectangle::RADIUS, r)?;
        Ok(r)
    }

    #[qml(invokable, name = "setSize")]
    fn set_size(cx: &Cx, w: f64, h: f64) -> Result<(), EngineError> {
        cx.engine.batch(|| {
            cx.set(Item::WIDTH, w)?;
            cx.set(Item::HEIGHT, h)
        })
    }

    #[qml(invokable)]
    fn fail(&self) -> Result<(), String> {
        Err("nope".into())
    }

    #[qml(invokable)]
    fn bump(&mut self) {
        self.completed_runs += 10;
    }

    #[qml(on_complete)]
    fn completed(&mut self) {
        self.completed_runs += 1;
    }

    /// Not exposed.
    pub fn helper(&self) -> f64 {
        self.radius * 2.0
    }
}

/// `Keys`-like attached type. Creatable: the engine makes attached
/// objects with the attached type's constructor.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "KeysAttached")]
pub struct KeysAttached {
    #[qml(notify)]
    pub enabled: bool,
}

#[derive(Debug, Default, QmlObject)]
#[qml(module = "Test", name = "Keys", attached = KeysAttached, uncreatable)]
pub struct Keys {}

pub fn engine() -> ljos_engine::Engine {
    let e = ljos_engine::Engine::new();
    e.register_module(ljos_engine::module!("Test" 1.0 => [Item, Rectangle]));
    e.register_module(ljos_engine::module!("Test" 1.1 => [Keys]));
    e
}
