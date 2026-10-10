//! `Rectangle`.

use ljos_engine::{Color, Cx, Engine, EngineError, ObjectId, QmlObject, Value, qml_impl};
use ljos_render::{Border as RenderBorder, DisplayListBuilder, DrawOp, Paint, Rgba, kurbo};

use crate::item::{Item, is_unset};

/// `Rectangle` (QtQuick): a filled, optionally rounded and bordered box.
///
/// TODO(gradient): `gradient` is not implemented; see the `@backlog`
/// scenario in `features/items/rectangle.feature`. `DrawOp::Rect` already
/// takes a `Paint::LinearGradient`, so only the `Gradient`/`GradientStop`
/// types are missing.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "Rectangle", extends = Item)]
pub struct Rectangle {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub color: Color,
    #[qml(notify)]
    pub radius: f64,
    #[qml(group)]
    pub border: RectangleBorder,
}

impl Default for Rectangle {
    fn default() -> Self {
        Self {
            item: Item::default(),
            color: Color::WHITE,
            radius: 0.0,
            border: RectangleBorder::default(),
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
        if is_unset(cx.engine, obj, Item::ANTIALIASING.index) {
            cx.engine.bind(obj, Item::ANTIALIASING.index, move |e| {
                Value::Bool(e.read(obj, Rectangle::RADIUS.index).to_number() > 0.0)
            })?;
        }
        Ok(())
    }
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
    pub border_width: f64,
    pub border_color: Color,
    pub antialiasing: bool,
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
        let assigned_name = |name| engine.property_index(obj, name).is_ok_and(&assigned);
        // Qt reads `border.width` as 1 by default but draws no border until
        // the author assigns its width or colour (checked on qml6).
        let has_border = assigned_name("border.width") || assigned_name("border.color");
        // A rectangle painted before it completes has no binding yet.
        let antialiasing = if assigned(Item::ANTIALIASING.index) {
            engine.read(obj, Item::ANTIALIASING.index).to_boolean()
        } else {
            radius > 0.0
        };
        RectanglePaint {
            color: match engine.read(obj, Rectangle::COLOR.index) {
                ljos_engine::Value::Color(c) => c,
                _ => Color::TRANSPARENT,
            },
            radius,
            border_width: if has_border { num("border.width") } else { 0.0 },
            border_color: color("border.color"),
            antialiasing,
        }
    }

    pub fn paint(&self, size: kurbo::Size, out: &mut DisplayListBuilder) {
        let rect = kurbo::Rect::from_origin_size((0.0, 0.0), size);
        // Qt clamps the radius to half the shorter side.
        let r = self.radius.clamp(0.0, size.width.min(size.height) / 2.0) as f32;
        let border = (self.border_width > 0.0 && self.border_color.a > 0).then(|| RenderBorder {
            width: self.border_width as f32,
            paint: Paint::Solid(to_rgba(self.border_color)),
        });
        let fill = Paint::Solid(to_rgba(self.color));
        if border.is_none() && self.color.a == 0 {
            return;
        }
        out.push(DrawOp::Rect {
            rect,
            radii: [r; 4],
            fill,
            border,
            aa: self.antialiasing,
        });
    }
}

/// Engine colour (16-bit straight channels) to render colour.
pub fn to_rgba(c: Color) -> Rgba {
    let [r, g, b, a] = c.to_rgba_f();
    Rgba::from_straight(r as f32, g as f32, b as f32, a as f32)
}
