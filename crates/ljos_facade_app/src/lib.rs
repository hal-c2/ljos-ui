//! An application's native module, written as `docs/MIGRATION.md`
//! section 6 tells an application to write one: `ljos` is its only
//! dependency and `use ljos::prelude::*;` its only import of Ljós. It
//! builds with the workspace, so the macros' generated paths, the
//! prelude and the facade's manifest (no `[patch]` to copy) stay
//! enough for a crate outside this repository. Its visual item paints
//! itself and takes keys and the wheel through `ljos::item`.

use std::rc::Rc;

use ljos::item::{self, DisplayListBuilder, ItemPaint, KeyEvent, KeyItem, Rgba, TextEngine};
use ljos::item::{WheelEvent, WheelItem, key, kurbo};
use ljos::prelude::*;

/// A native type with a property, a signal and an invokable method.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Counter")]
pub struct Counter {
    #[qml(notify)]
    pub count: i32,
}

#[qml_impl]
impl Counter {
    #[qml(signal)]
    fn bumped();

    #[qml(invokable)]
    fn increment(cx: &Cx) -> Result<(), EngineError> {
        let n = cx.get(Counter::COUNT)?;
        cx.set(Counter::COUNT, n + 1)?;
        Counter::emit_bumped(cx)
    }
}

/// A native singleton: one object per engine, made when a script first
/// names it.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Greeter")]
#[qml_singleton]
pub struct Greeter {
    #[qml(notify)]
    pub greeting: Atom,
}

#[qml_impl]
impl Greeter {
    #[qml(invokable)]
    fn greet(&self, name: Atom) -> Atom {
        Atom::from(format!("{} {name}", self.greeting))
    }
}

/// A visual native item that paints itself: a bar as long as its value.
/// Up and Down, and the wheel, move the value.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "Example", name = "Gauge", extends = Item)]
pub struct Gauge {
    #[qml(base)]
    pub item: Item,
    /// How full, 0 to 1.
    #[qml(notify)]
    pub value: f64,
}

/// What a gauge shows.
#[derive(Debug)]
pub struct Bar(pub f64);

impl ItemPaint for Bar {
    fn paint(&self, size: kurbo::Size, _: &mut TextEngine, out: &mut DisplayListBuilder) {
        let filled = kurbo::Rect::new(0.0, 0.0, size.width * self.0, size.height);
        out.fill_rect(filled, Rgba::BLACK);
    }
}

/// Tracked: the gauge repaints when its value changes.
pub fn read_gauge(engine: &Engine, gauge: ObjectId) -> Rc<dyn ItemPaint> {
    Rc::new(Bar(item::prop(engine, gauge, Gauge::VALUE)))
}

fn step(engine: &Engine, gauge: ObjectId, by: f64) {
    let now = item::prop(engine, gauge, Gauge::VALUE);
    let _ = engine.set(gauge, Gauge::VALUE, (now + by).clamp(0.0, 1.0));
}

fn gauge_key(engine: &Engine, gauge: ObjectId, ev: &KeyEvent) -> bool {
    match ev.key {
        key::UP => step(engine, gauge, 0.25),
        key::DOWN => step(engine, gauge, -0.25),
        _ => return false,
    }
    true
}

fn gauge_wheel(engine: &Engine, gauge: ObjectId, ev: &WheelEvent, _: kurbo::Point) -> bool {
    step(engine, gauge, ev.angle_delta.y / WheelEvent::NOTCH / 4.0);
    true
}

/// The one seam a host calls.
pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Example" 1.0 => [Counter, Gauge]));
    engine.register_singleton::<Greeter>("Example", Version::new(1, 0));
    item::register_paint_item::<Gauge>(engine, read_gauge);
    item::register_key_item::<Gauge>(
        engine,
        KeyItem {
            shortcut_override: |_, _, _| false,
            press: gauge_key,
            release: |_, _, _| false,
        },
    );
    item::register_wheel_item::<Gauge>(engine, WheelItem { wheel: gauge_wheel });
}
