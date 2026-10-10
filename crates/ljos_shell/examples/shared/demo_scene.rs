//! The `demo` example's scene, built natively through the Engine API.
//! Shared by `examples/demo.rs` and `tests/demos.rs` through `#[path]`, so
//! the window and its offscreen render are the same scene.

use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::{Color, Engine, EngineError, Font, ObjectId, QmlType, Value};
use ljos_quick::{Column, Item, MouseArea, Rectangle, Row, Text, add_data};

const PALETTE: [Color; 4] = [
    Color::from_rgba8(0x2e, 0x7d, 0x32, 255),
    Color::from_rgba8(0xc6, 0x28, 0x28, 255),
    Color::from_rgba8(0x15, 0x65, 0xc0, 255),
    Color::from_rgba8(0xef, 0x6c, 0x00, 255),
];

fn make<T: QmlType + Default>(
    e: &Engine,
    parent: Option<ObjectId>,
) -> Result<ObjectId, EngineError> {
    let o = e.create_native(T::default(), parent)?;
    if let Some(p) = parent {
        add_data(e, p, o)?;
    }
    e.complete(o)?;
    Ok(o)
}

fn rect(
    e: &Engine,
    parent: ObjectId,
    w: f64,
    h: f64,
    color: Color,
) -> Result<ObjectId, EngineError> {
    let r = make::<Rectangle>(e, Some(parent))?;
    e.set(r, Item::WIDTH, w)?;
    e.set(r, Item::HEIGHT, h)?;
    e.write_by_name(r, "color", Value::Color(color))?;
    Ok(r)
}

fn text(
    e: &Engine,
    parent: ObjectId,
    s: &str,
    px: i32,
    color: Color,
) -> Result<ObjectId, EngineError> {
    let t = make::<Text>(e, Some(parent))?;
    e.write_by_name(t, "text", Value::string(s))?;
    e.write_by_name(t, "color", Value::Color(color))?;
    let font = Font {
        pixel_size: px,
        ..Font::default()
    };
    e.write_by_name(t, "font", Value::Font(Arc::new(font)))?;
    Ok(t)
}

pub struct Built {
    pub root: ObjectId,
    /// The status line under the swatches, for messages to write to.
    pub status: ObjectId,
    /// The bottom-right button, so a test can find where to click.
    #[allow(dead_code, reason = "only tests/demos.rs reads it")]
    pub button: ObjectId,
}

pub fn build(e: &Rc<Engine>) -> Result<Built, EngineError> {
    let root = make::<Item>(e, None)?;
    let ink = Color::from_rgba8(0x21, 0x21, 0x21, 255);

    // Column { Text; Row { Rectangle × 4 }; Text }
    let col = make::<Column>(e, Some(root))?;
    e.set(col, Item::X, 24.0)?;
    e.set(col, Item::Y, 24.0)?;
    e.write_by_name(col, "spacing", Value::Real(12.0))?;
    text(e, col, "Ljós on egui", 24, ink)?;
    let row = make::<Row>(e, Some(col))?;
    e.write_by_name(row, "spacing", Value::Real(8.0))?;
    for (i, c) in PALETTE.iter().enumerate() {
        let r = rect(e, row, 48.0, 48.0, *c)?;
        e.write_by_name(r, "radius", Value::Real(i as f64 * 8.0))?;
    }
    let status = text(e, col, "Waiting for the worker thread…", 14, ink)?;

    // A framed box centred in the window.
    let frame = rect(
        e,
        root,
        220.0,
        90.0,
        Color::from_rgba8(0xff, 0xf8, 0xe1, 255),
    )?;
    e.write_by_name(frame, "anchors.centerIn", Value::Object(root))?;
    e.write_by_name(frame, "radius", Value::Real(10.0))?;
    e.write_by_name(frame, "border.width", Value::Real(2.0))?;
    e.write_by_name(frame, "border.color", Value::Color(PALETTE[3]))?;
    let caption = text(e, frame, "anchors.centerIn: parent", 14, ink)?;
    e.write_by_name(caption, "anchors.centerIn", Value::Object(frame))?;

    // A button in the bottom-right corner.
    let button = rect(e, root, 160.0, 56.0, PALETTE[2])?;
    e.write_by_name(button, "anchors.right", e.read_by_name(root, "right"))?;
    e.write_by_name(button, "anchors.bottom", e.read_by_name(root, "bottom"))?;
    e.write_by_name(button, "anchors.margins", Value::Real(24.0))?;
    e.write_by_name(button, "radius", Value::Real(6.0))?;
    let label = text(e, button, "Click me", 16, Color::WHITE)?;
    e.write_by_name(label, "anchors.centerIn", Value::Object(button))?;
    let area = make::<MouseArea>(e, Some(button))?;
    e.write_by_name(area, "anchors.fill", Value::Object(button))?;
    e.write_by_name(area, "hoverEnabled", Value::Bool(true))?;
    let clicks = std::cell::Cell::new(0usize);
    e.connect_by_name(area, "clicked", move |e, _| {
        clicks.set(clicks.get() + 1);
        let n = clicks.get();
        let _ = e.write_by_name(button, "color", Value::Color(PALETTE[n % PALETTE.len()]));
        let _ = e.write_by_name(label, "text", Value::string(format!("Clicked {n}×")));
    })?;
    // Hover feedback, to show `containsMouse` changes reaching the scene.
    e.connect_by_name(area, "containsMouseChanged", move |e, _| {
        let over = e.read_by_name(area, "containsMouse") == Value::Bool(true);
        let _ = e.write_by_name(
            button,
            "opacity",
            Value::Real(if over { 0.85 } else { 1.0 }),
        );
    })?;

    Ok(Built {
        root,
        status,
        button,
    })
}
