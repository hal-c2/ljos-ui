//! A visual item of the application's own that paints itself and takes
//! keys, the wheel and focus: a terminal, a chart, a video frame, a
//! syntax-highlighting editor. This module is the supported surface for
//! one, as a `QQuickPaintedItem` subclass needs only the Qt Quick module.
//!
//! The type extends [`Item`] (`extends = Item` with a `#[qml(base)]`
//! field) and is registered as any native type is. Its hooks are then
//! registered for it:
//!
//! - [`register_paint_item`]: what it shows. The `read` function reads
//!   the item's properties ([`prop`]) and returns an [`ItemPaint`]; the
//!   read is tracked, so a change of what it read repaints the item. The
//!   item paints into a [`DisplayListBuilder`] in its own coordinates,
//!   with the scene's [`TextEngine`] lent for laying out text.
//! - [`register_key_item`]: what it does with a [`KeyEvent`] while it has
//!   the active focus. [`force_active_focus`] gives it the focus (on a
//!   click, say), as `forceActiveFocus()` does in QML.
//! - [`register_wheel_item`]: what it does with a [`WheelEvent`] over it.
//!
//! A `TextEdit` that colours its text takes a [`Highlighter`] instead:
//! [`set_highlighter`].
//!
//! ```
//! use std::rc::Rc;
//!
//! use ljos::item::{self, DisplayListBuilder, ItemPaint, Rgba, TextEngine, kurbo};
//! use ljos::prelude::*;
//!
//! #[derive(Debug, Default, QmlObject)]
//! #[qml(module = "Example", name = "Gauge", extends = Item)]
//! pub struct Gauge {
//!     #[qml(base)]
//!     pub item: Item,
//!     /// How full, 0 to 1.
//!     #[qml(notify)]
//!     pub value: f64,
//! }
//!
//! /// What a gauge shows: a bar as long as its value.
//! #[derive(Debug)]
//! struct Bar(f64);
//!
//! impl ItemPaint for Bar {
//!     fn paint(&self, size: kurbo::Size, _: &mut TextEngine, out: &mut DisplayListBuilder) {
//!         let filled = kurbo::Rect::new(0.0, 0.0, size.width * self.0, size.height);
//!         out.fill_rect(filled, Rgba::BLACK);
//!     }
//! }
//!
//! fn read(engine: &Engine, gauge: ObjectId) -> Rc<dyn ItemPaint> {
//!     Rc::new(Bar(item::prop(engine, gauge, Gauge::VALUE)))
//! }
//!
//! pub fn register(engine: &Engine) {
//!     engine.register_module(ljos::module!("Example" 1.0 => [Gauge]));
//!     item::register_paint_item::<Gauge>(engine, read);
//! }
//! # register(&ljos::engine());
//! ```

use ljos_engine::{Engine, MetaRef, ObjectId, Prop, QmlType, QmlValue};

pub use ljos_engine::Font;
pub use ljos_quick::focus::{FocusReason, active_focus_item, force_active_focus};
pub use ljos_quick::keys::{KeyItem, key};
pub use ljos_quick::text_input::{HighlightSpan, Highlighter, set_highlighter, set_highlights};
pub use ljos_quick::{
    Item, ItemPaint, KeyEvent, KeyKind, Modifiers, ReadPaint, ScrollPhase, WheelEvent,
    flickable::WheelItem,
};
pub use ljos_render::{
    Border, Brush, Decorations, DisplayList, DisplayListBuilder, DrawOp, FillRule, GlyphRun,
    GradientStop, LinearGradient, Paint, PathStyle, PositionedGlyph, Rgba, ShapeStyle, Spread,
    TextAlign, kurbo, peniko,
};
pub use ljos_text::{
    ElideMode, FontMetrics, LayoutParams, Line, LineRun, TextEngine, TextLayout, TextStyle,
    WrapMode,
};

/// Reads a property of `obj` through the handle its type's derive made
/// (`Gauge::VALUE`). Inside a paint item's `read` the read is tracked:
/// the item repaints when the property changes. An object that is gone,
/// or not of the handle's type, reads as `T::default()`, which is what an
/// item that is being destroyed has left to paint.
pub fn prop<T: QmlValue + Default>(engine: &Engine, obj: ObjectId, p: Prop<T>) -> T {
    engine.get(obj, p).unwrap_or_default()
}

/// Makes the scene paint items of type `T`, and of the types derived
/// from it, with what `read` returns. A later registration wins.
pub fn register_paint_item<T: QmlType>(engine: &Engine, read: ReadPaint) {
    ljos_quick::register_paint_item(engine, MetaRef::Static(T::static_meta()), read);
}

/// Makes key delivery ask items of type `T`, and of the types derived
/// from it, through `hooks` while one has the active focus. Each hook
/// says whether the item took the key; one it did not take goes on to
/// the item's parent, as in Qt.
pub fn register_key_item<T: QmlType>(engine: &Engine, hooks: KeyItem) {
    ljos_quick::keys::register_key_item(engine, MetaRef::Static(T::static_meta()), hooks);
}

/// Makes the scene offer wheel events to items of type `T`, and of the
/// types derived from it, in their own coordinates. One the item does
/// not use goes on to the items beneath.
pub fn register_wheel_item<T: QmlType>(engine: &Engine, hooks: WheelItem) {
    ljos_quick::flickable::register_wheel_item(engine, MetaRef::Static(T::static_meta()), hooks);
}
