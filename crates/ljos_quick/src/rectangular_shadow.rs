//! `RectangularShadow` (`QtQuick.Effects`): a box shadow.
//!
//! Reference: <https://doc.qt.io/qt-6/qml-qtquick-effects-rectangularshadow.html>.
//! The item's rectangle, moved by `offset` and grown by `spread` on every
//! side, with corners of `radius + spread`, is filled with `color` and
//! blurred: measured on qml6 drawing with OpenGL, the edge is half the
//! colour and the fade runs `blur` pixels to either side of it, as a
//! Gaussian of deviation `blur / 2` does. The renderer's blurred rounded
//! rectangle draws it, so it costs one draw and no picture.
//!
//! `material`, Qt's ShaderEffect to draw it with, is kept and not
//! applied; `cached` is kept, there being no picture to cache.

use std::rc::Rc;

use ljos_engine::{Color, Engine, MetaRef, ObjectId, QmlObject, QmlType, Vector2d, qml_impl};
use ljos_render::{DisplayListBuilder, kurbo};
use ljos_text::TextEngine;

use crate::item::Item;
use crate::paint_item::{ItemPaint, register_paint_item};
use crate::rectangle::to_rgba;

#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Effects", name = "RectangularShadow", extends = Item)]
pub struct RectangularShadow {
    #[qml(base)]
    pub item: Item,
    /// How far the edges fade, in pixels, to either side of them.
    #[qml(notify)]
    pub blur: f64,
    /// How much the rectangle grows on every side before it is blurred.
    #[qml(notify)]
    pub spread: f64,
    /// The radius of its corners, before `spread`.
    #[qml(notify)]
    pub radius: f64,
    #[qml(notify)]
    pub color: Color,
    /// Where the shadow is drawn from the item's own place.
    #[qml(notify)]
    pub offset: Vector2d,
    /// Kept; there is no picture to cache.
    #[qml(notify)]
    pub cached: bool,
    /// Kept and not applied.
    #[qml(notify)]
    pub material: Option<ObjectId>,
}

impl Default for RectangularShadow {
    fn default() -> Self {
        Self {
            item: Item::default(),
            // Qt's defaults, read on qml6.
            blur: 10.0,
            spread: 0.0,
            radius: 0.0,
            color: Color::BLACK,
            offset: Vector2d::default(),
            cached: false,
            material: None,
        }
    }
}

#[qml_impl]
impl RectangularShadow {}

#[derive(Debug)]
struct ShadowPaint {
    blur: f64,
    spread: f64,
    radius: f64,
    color: Color,
    offset: Vector2d,
}

impl ItemPaint for ShadowPaint {
    fn paint(&self, size: kurbo::Size, _: &mut TextEngine, out: &mut DisplayListBuilder) {
        if self.color.a == 0 || size.width <= 0.0 || size.height <= 0.0 {
            return;
        }
        let origin = (f64::from(self.offset.x), f64::from(self.offset.y));
        let rect = kurbo::Rect::from_origin_size(origin, size);
        out.shadow(
            rect,
            [self.radius.max(0.0) as f32; 4],
            self.blur.max(0.0) as f32,
            self.spread as f32,
            to_rgba(self.color),
        );
    }
}

fn read(e: &Engine, obj: ObjectId) -> Rc<dyn ItemPaint> {
    let num = |p| e.read(obj, p).to_number();
    Rc::new(ShadowPaint {
        blur: num(RectangularShadow::BLUR.index),
        spread: num(RectangularShadow::SPREAD.index),
        radius: num(RectangularShadow::RADIUS.index),
        color: match e.read(obj, RectangularShadow::COLOR.index) {
            ljos_engine::Value::Color(c) => c,
            _ => Color::TRANSPARENT,
        },
        offset: match e.read(obj, RectangularShadow::OFFSET.index) {
            ljos_engine::Value::Vector2d(v) => v,
            _ => Vector2d::default(),
        },
    })
}

pub(crate) fn register_paint(engine: &Engine) {
    register_paint_item(
        engine,
        MetaRef::Static(RectangularShadow::static_meta()),
        read,
    );
}
