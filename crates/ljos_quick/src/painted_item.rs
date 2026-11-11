//! `PaintedItem` (`QtQuick`): what every item that paints itself has, as
//! Qt's `QQuickPaintedItem` does.
//!
//! Reference: <https://doc.qt.io/qt-6/qquickpainteditem.html>. A type
//! that paints through [`crate::paint_item`] and extends `PaintedItem`
//! is painted as Qt paints a painted item, measured on qml6
//! (features/lottie/lottie-painted-item.feature):
//!
//! - What the type paints is drawn `contentsScale` times as large, about
//!   the item's top-left corner. The type paints at the item's own size,
//!   as a scaled painter would.
//! - Painting is cut off at the painted area: the item's rectangle and the
//!   `contentsSize` scaled by `contentsScale`, from the top-left corner,
//!   together. A `contentsSize` of -1 by -1, the default, adds nothing.
//! - `fillColor` fills the whole painted area, behind what is painted.
//! - A `contentsScale` of 0 paints nothing at all, not even the fill; a
//!   negative one paints the fill only.
//!
//! `textureSize` and `renderTarget` are kept and change nothing: Qt draws
//! into a picture of that many pixels, kept in that kind of buffer, and
//! Ljós draws what is painted as shapes at the display's own resolution.

use std::rc::Rc;

use ljos_engine::{Color, Cx, Engine, ObjectId, QmlObject, Size, Value, qml_impl};
use ljos_render::{DisplayListBuilder, kurbo};
use ljos_text::TextEngine;

use crate::enums::EnumTable;
use crate::item::Item;
use crate::paint_item::ItemPaint;
use crate::rectangle::to_rgba;

/// The values of `PaintedItem.RenderTarget`.
pub(crate) mod render_target {
    pub const IMAGE: i32 = 0;
    pub const FRAMEBUFFER_OBJECT: i32 = 1;
    pub const INVERTED_Y_FRAMEBUFFER_OBJECT: i32 = 2;
}

fn painted_item_enums() -> EnumTable {
    vec![
        (
            "RenderTarget",
            vec![
                ("Image", i64::from(render_target::IMAGE)),
                (
                    "FramebufferObject",
                    i64::from(render_target::FRAMEBUFFER_OBJECT),
                ),
                (
                    "InvertedYFramebufferObject",
                    i64::from(render_target::INVERTED_Y_FRAMEBUFFER_OBJECT),
                ),
            ],
        ),
        ("PerformanceHints", vec![("FastFBOResizing", 1)]),
    ]
}

/// What Qt's painted item has. Uncreatable: it is the base of types that
/// paint, such as `LottieAnimation`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "PaintedItem",
    extends = Item,
    uncreatable,
    enums = painted_item_enums
)]
pub struct PaintedItem {
    #[qml(base)]
    pub item: Item,
    /// How much is painted beyond the item, before `contentsScale`, in
    /// whole pixels.
    #[qml(notify, write = whole_size)]
    pub contents_size: Size,
    /// What the painted area is filled with, behind what is painted.
    #[qml(notify)]
    pub fill_color: Color,
    /// How many times as large what is painted is drawn.
    #[qml(notify)]
    pub contents_scale: f64,
    /// One of [`render_target`]. Kept and not applied.
    #[qml(notify, enumeration = "RenderTarget")]
    pub render_target: i32,
    /// The picture's size in pixels, in whole pixels. Kept and not applied.
    #[qml(notify, write = whole_size)]
    pub texture_size: Size,
}

impl Default for PaintedItem {
    fn default() -> Self {
        // Qt's defaults, read on qml6.
        let unset = Size {
            width: -1.0,
            height: -1.0,
        };
        Self {
            item: Item::default(),
            contents_size: unset,
            fill_color: Color::TRANSPARENT,
            contents_scale: 1.0,
            render_target: render_target::IMAGE,
            texture_size: unset,
        }
    }
}

#[qml_impl]
impl PaintedItem {}

/// Qt's sizes here are whole pixels: what is written is rounded, as Qt
/// rounds a `size` it is given.
fn whole_size(_: &Cx, s: Size) -> Option<Size> {
    Some(Size {
        width: s.width.round(),
        height: s.height.round(),
    })
}

/// What a painted item paints: the type's own painting, scaled, over the
/// fill, cut off at the painted area.
#[derive(Debug)]
pub(crate) struct Painted {
    inner: Rc<dyn ItemPaint>,
    scale: f64,
    fill: Color,
    contents: Size,
}

/// What `obj` paints as a painted item, when its type extends
/// `PaintedItem`; `inner` as it is otherwise. Tracked, as the read of
/// `inner` is.
pub(crate) fn read(e: &Engine, obj: ObjectId, inner: Rc<dyn ItemPaint>) -> Rc<dyn ItemPaint> {
    if !crate::item::is_a::<PaintedItem>(e, obj) {
        return inner;
    }
    let scale = e.read(obj, PaintedItem::CONTENTS_SCALE.index).to_number();
    let fill = match e.read(obj, PaintedItem::FILL_COLOR.index) {
        Value::Color(c) => c,
        _ => Color::TRANSPARENT,
    };
    let contents = match e.read(obj, PaintedItem::CONTENTS_SIZE.index) {
        Value::Size(s) => *s,
        _ => Size::default(),
    };
    Rc::new(Painted {
        inner,
        scale,
        fill,
        contents,
    })
}

impl Painted {
    /// Where painting is cut off, for an item of `size`; `None` where
    /// nothing is painted.
    pub(crate) fn area(&self, size: kurbo::Size) -> Option<kurbo::Rect> {
        if self.scale == 0.0 || !self.scale.is_finite() {
            return None;
        }
        let w = size.width.max(self.contents.width * self.scale);
        let h = size.height.max(self.contents.height * self.scale);
        Some(kurbo::Rect::new(0.0, 0.0, w, h))
    }
}

impl ItemPaint for Painted {
    fn paint(&self, size: kurbo::Size, text: &mut TextEngine, out: &mut DisplayListBuilder) {
        let Some(area) = self.area(size) else {
            return;
        };
        if self.fill.a != 0 && area.width() > 0.0 && area.height() > 0.0 {
            out.fill_rect(area, to_rgba(self.fill));
        }
        // A painter scaled by a negative factor turns everything it draws
        // over, out of the area: Qt shows the fill alone then.
        if self.scale < 0.0 {
            return;
        }
        if self.scale == 1.0 {
            self.inner.paint(size, text, out);
        } else {
            let mut inner = DisplayListBuilder::new();
            self.inner.paint(size, text, &mut inner);
            out.append_scaled(inner, self.scale);
        }
    }

    fn clip(&self, size: kurbo::Size) -> Option<kurbo::Rect> {
        Some(self.area(size).unwrap_or(kurbo::Rect::ZERO))
    }
}
