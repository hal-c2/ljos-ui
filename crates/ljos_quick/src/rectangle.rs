//! `Rectangle`.

use ljos_engine::{
    Color, Cx, Engine, EngineError, LogLevel, ObjectFlags, ObjectId, QmlObject, Value, WriteKind,
    qml_impl,
};
use ljos_render::{Border as RenderBorder, DisplayListBuilder, DrawOp, Paint, Rgba, kurbo};

use crate::gradient::{GradientPaint, as_gradient};
use crate::item::{Item, is_unset};

/// `Rectangle` (QtQuick): a filled, optionally rounded and bordered box.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "Rectangle", extends = Item)]
pub struct Rectangle {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub color: Color,
    #[qml(notify)]
    pub radius: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_radius))]
    pub top_left_radius: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_radius))]
    pub top_right_radius: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_radius))]
    pub bottom_left_radius: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(all_radius))]
    pub bottom_right_radius: f64,
    #[qml(group)]
    pub border: RectangleBorder,
    /// A `Gradient` to fill the rectangle with instead of `color`, or
    /// nothing. Qt has it as a script value without a change signal: it
    /// reads `undefined` until it is assigned, and `null` after `null`.
    /// Qt's presets (`Gradient.NightFade`, `"NightFade"`) are not built.
    #[qml(reset, write = gradient_written)]
    pub gradient: Value,
}

fn all_radius(engine: &Engine, obj: ObjectId, _corner: ljos_engine::PropIndex) -> Value {
    engine.read(obj, Rectangle::RADIUS.index)
}

impl Default for Rectangle {
    fn default() -> Self {
        Self {
            item: Item::default(),
            color: Color::WHITE,
            radius: 0.0,
            top_left_radius: 0.0,
            top_right_radius: 0.0,
            bottom_left_radius: 0.0,
            bottom_right_radius: 0.0,
            border: RectangleBorder::default(),
            gradient: Value::Undefined,
        }
    }
}

#[qml_impl]
impl Rectangle {
    /// Qt's default for `antialiasing` is on for a rounded rectangle and
    /// off otherwise, and it follows `radius` until it is assigned.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let obj = cx.this;
        let e = cx.engine;
        follow_radius(e, obj)?;
        // What is not a gradient is refused when it is assigned, as Qt
        // does, and not each time the rectangle is painted: what was given
        // before now, here, and later writes by `gradient_written`.
        let given = cx
            .engine
            .untrack(|| cx.engine.read(obj, Rectangle::GRADIENT.index));
        refuse_what_is_no_gradient(cx.engine, obj, &given);
        Ok(())
    }
}

/// `radius` and the four corners, which `antialiasing` follows.
const RADII: [ljos_engine::PropIndex; 5] = [
    Rectangle::RADIUS.index,
    Rectangle::TOP_LEFT_RADIUS.index,
    Rectangle::TOP_RIGHT_RADIUS.index,
    Rectangle::BOTTOM_LEFT_RADIUS.index,
    Rectangle::BOTTOM_RIGHT_RADIUS.index,
];

/// Binds each corner that has no value of its own to `radius`, and
/// `antialiasing`, while it is not assigned, to whether any is rounded.
/// Each binding is made when something first reads or watches the
/// property: painting works the same out for itself, so most rectangles
/// never have them.
fn follow_radius(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    for &corner in &RADII[1..] {
        if is_unset(engine, obj, corner) {
            engine.bind_on_read(obj, corner, |e, obj, corner| {
                e.bind(obj, corner, move |e| e.read(obj, Rectangle::RADIUS.index))
            })?;
        }
    }
    if is_unset(engine, obj, Item::ANTIALIASING.index) {
        engine.bind_on_read(obj, Item::ANTIALIASING.index, |e, obj, aa| {
            e.bind(obj, aa, move |e| {
                Value::Bool(RADII.iter().any(|&r| e.read(obj, r).to_number() > 0.0))
            })
        })?;
    }
    Ok(())
}

/// `gradient` written to a complete rectangle. A filter rather than a
/// watcher, which would be a node and an effect on every rectangle.
fn gradient_written(cx: &Cx, v: Value) -> Option<Value> {
    let complete = cx
        .engine
        .flags(cx.this)
        .is_some_and(|f| f.contains(ObjectFlags::COMPLETED));
    match complete.then(|| no_gradient(cx.engine, &v)).flatten() {
        Some(why) => {
            warn_no_gradient(cx.engine, &why);
            Some(Value::Undefined)
        }
        None => Some(v),
    }
}

/// What was given before the rectangle was complete, refused as
/// [`gradient_written`] refuses a later write.
fn refuse_what_is_no_gradient(engine: &Engine, rect: ObjectId, v: &Value) {
    let Some(why) = no_gradient(engine, v) else {
        return;
    };
    warn_no_gradient(engine, &why);
    // Not an assignment: a binding that gave the value stays, as in Qt.
    let _ = engine.write_internal(
        rect,
        Rectangle::GRADIENT.index,
        Value::Undefined,
        WriteKind::Host,
    );
}

fn warn_no_gradient(engine: &Engine, why: &str) {
    engine.log(LogLevel::Warning, "qml", format!("QML Rectangle: {why}"));
}

/// Why `v` is no gradient. Qt takes a `Gradient`, the number or the name
/// of a preset, or nothing; anything else it warns about and then reads as
/// `undefined`. Ljós has no presets, so no number and no name is one.
fn no_gradient(engine: &Engine, v: &Value) -> Option<String> {
    Some(match v {
        Value::Undefined | Value::Null => return None,
        Value::Object(o) if !engine.is_alive(*o) || as_gradient(engine, v).is_some() => {
            return None;
        }
        Value::Object(o) => {
            let ty = engine.meta_of(*o).map(|m| m.name.to_string());
            format!(
                "Can't assign {} to gradient property",
                ty.as_deref().unwrap_or("object")
            )
        }
        Value::Int(n) => format!("No such gradient preset '{n}'"),
        // Qt reads a number as a whole one.
        Value::Real(n) => format!("No such gradient preset '{}'", n.trunc()),
        Value::String(name) => format!("No such gradient preset '{name}'"),
        _ => "Unknown gradient type. Expected int, string, or Gradient".to_owned(),
    })
}

/// The `border` group of [`Rectangle`].
#[derive(Debug, QmlObject)]
#[qml(name = "RectangleBorder", uncreatable)]
pub struct RectangleBorder {
    #[qml(notify)]
    pub width: f64,
    #[qml(notify)]
    pub color: Color,
}

impl Default for RectangleBorder {
    fn default() -> Self {
        Self {
            width: 1.0,
            color: Color::BLACK,
        }
    }
}

/// What a Rectangle paints. Built by the scene's content effect, which is
/// the only place its bindings are pulled.
#[derive(Clone, Debug, PartialEq)]
pub struct RectanglePaint {
    pub color: Color,
    pub radius: f64,
    /// Corner radii in `(top-left, top-right, bottom-right, bottom-left)` order.
    pub radii: [f64; 4],
    pub border_width: f64,
    pub border_color: Color,
    pub antialiasing: bool,
    /// The gradient that is painted instead of `color`, when there is one
    /// and it has stops.
    pub gradient: Option<GradientPaint>,
}

impl RectanglePaint {
    /// Reads the paint properties (tracked).
    pub fn read(engine: &Engine, obj: ObjectId) -> Self {
        let num = |name| engine.read_by_name(obj, name).to_number();
        let color = |name| match engine.read_by_name(obj, name) {
            ljos_engine::Value::Color(c) => c,
            _ => Color::TRANSPARENT,
        };
        let radius = engine.read(obj, Rectangle::RADIUS.index).to_number();
        let assigned = |p: ljos_engine::PropIndex| {
            !matches!(
                engine.provenance(obj, p),
                Some(ljos_engine::Provenance::Default) | None
            )
        };
        let corner = |p: ljos_engine::PropIndex| match engine.read_unless_bound_on_read(obj, p) {
            Some(v) if assigned(p) => v.to_number(),
            _ => radius,
        };
        let radii = [
            corner(Rectangle::TOP_LEFT_RADIUS.index),
            corner(Rectangle::TOP_RIGHT_RADIUS.index),
            corner(Rectangle::BOTTOM_RIGHT_RADIUS.index),
            corner(Rectangle::BOTTOM_LEFT_RADIUS.index),
        ];
        let assigned_name = |name| engine.property_index(obj, name).is_ok_and(&assigned);
        // Qt reads `border.width` as 1 by default but draws no border until
        // the author assigns its width or colour (checked on qml6).
        let has_border = assigned_name("border.width") || assigned_name("border.color");
        // A rectangle painted before it completes has no binding yet.
        let antialiasing = if assigned(Item::ANTIALIASING.index) {
            engine.read(obj, Item::ANTIALIASING.index).to_boolean()
        } else {
            radius > 0.0 || radii.iter().any(|r| *r > 0.0)
        };
        RectanglePaint {
            color: match engine.read(obj, Rectangle::COLOR.index) {
                ljos_engine::Value::Color(c) => c,
                _ => Color::TRANSPARENT,
            },
            radius,
            radii,
            border_width: if has_border { num("border.width") } else { 0.0 },
            border_color: color("border.color"),
            antialiasing,
            gradient: as_gradient(engine, &engine.read(obj, Rectangle::GRADIENT.index))
                .and_then(|g| GradientPaint::of(engine, g)),
        }
    }

    pub fn paint(&self, size: kurbo::Size, out: &mut DisplayListBuilder) {
        let rect = kurbo::Rect::from_origin_size((0.0, 0.0), size);
        // Qt clamps the radius to half the shorter side.
        let half = size.width.min(size.height) / 2.0;
        let radii = self.radii.map(|r| r.clamp(0.0, half) as f32);
        let border = (self.border_width > 0.0 && self.border_color.a > 0).then(|| RenderBorder {
            width: self.border_width as f32,
            paint: Paint::Solid(to_rgba(self.border_color)),
        });
        let fill = match &self.gradient {
            // Over the whole rectangle, border included (checked on qml6).
            Some(g) => g.paint(rect),
            None => Paint::Solid(to_rgba(self.color)),
        };
        if border.is_none() && fill.is_invisible() {
            return;
        }
        out.push(DrawOp::Rect {
            rect,
            radii,
            fill,
            border,
            aa: self.antialiasing,
        });
    }
}

/// Engine colour (16-bit straight channels) to render colour. An invalid
/// colour paints its channels, black unless an alpha was set, as in qml6.
pub fn to_rgba(c: Color) -> Rgba {
    let [r, g, b, a] = c.painted().to_rgba_f();
    Rgba::from_straight(r as f32, g as f32, b as f32, a as f32)
}
