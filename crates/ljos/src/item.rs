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
//! - [`register_pointer_item`]: what it does with a press, the moves and
//!   release that follow it, and the pointer coming over it and leaving
//!   ([`PointerItem`], [`PointerEvent`]); [`register_hover_move`] adds the
//!   moves over it with no button held, for an item with parts of its
//!   own under the pointer (cells, a selection) or that reports motion.
//! - [`register_text_input_client`]: what an input method may do with it
//!   while it has the active focus ([`TextInputClient`]): the text it
//!   commits, what it is composing, and the cursor rectangle its
//!   candidate window is placed by.
//!
//! What the item shows can depend on things the hooks above learn
//! outside of `read`: [`text_style`] and [`with_text_engine`] measure
//! text with the scene's own fonts (a terminal's cell size, which decides
//! its rows and columns before anything paints), and
//! [`device_pixel_ratio`] is the scale of the display its window is on
//! (tracked: a `read` that asks repaints when the window moves to
//! another display). When what it shows changes where no property sees
//! it (a frame decoded, cells a program wrote), [`repaint`] paints it
//! again; it needs no property of its own for that.
//!
//! A picture the item draws is an [`ImageHandle`] made from its pixels
//! ([`ImageHandle::from_straight`]) and drawn with
//! [`DisplayListBuilder::image`]; [`place_image`] fits it to the item as an
//! `Image`'s `fillMode` would ([`ImageFit`], [`ImageAlign`]). A picture
//! at a URL comes from [`load_image`], read and decoded off the UI thread
//! and kept in the cache `Image` uses; the item keeps the handle it gets
//! and asks to be painted again ([`repaint`]).
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

use std::rc::Rc;

use ljos_engine::{Engine, MetaRef, ObjectId, Prop, QmlType, QmlValue};

pub use ljos_engine::Font;
pub use ljos_quick::LoadError as ImageLoadError;
pub use ljos_quick::focus::{FocusReason, active_focus_item, force_active_focus};
pub use ljos_quick::keys::{KeyItem, key};
pub use ljos_quick::keys::{Surrounding, TextInputClient};
pub use ljos_quick::text_input::{HighlightSpan, Highlighter, set_highlighter, set_highlights};
pub use ljos_quick::{
    HoverMove, MouseButtons, PointerEvent, PointerId, PointerItem, PointerKind, contains_point,
};
pub use ljos_quick::{
    Item, ItemPaint, KeyEvent, KeyKind, Modifiers, ReadPaint, ScrollPhase, WheelEvent,
    flickable::WheelItem,
};
pub use ljos_render::{
    Border, Brush, Decorations, DisplayList, DisplayListBuilder, DrawOp, FillRule, GlyphRun,
    GradientStop, LinearGradient, Paint, PathStyle, PositionedGlyph, Rgba, ShapeStyle, Spread,
    TextAlign, kurbo, peniko,
};
pub use ljos_render::{
    ImageAlign, ImageError, ImageFit, ImageHandle, ImagePlacement, painted_size, place_image,
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

/// Has the picture at `url` read and decoded, and calls `reply` with it
/// on the UI thread when it is ready, never inside this call. `url` is
/// absolute: a file's ([`crate::prelude::file_url`]), `qrc:`, `data:`,
/// `http(s):` or a resource the host registered. The picture goes
/// through the cache and the decode threads `Image` uses, so one an
/// `Image` shows is not decoded again; it stays cached while the handle
/// is held.
pub fn load_image(
    engine: &Engine,
    url: &str,
    reply: impl FnOnce(&Engine, Result<ImageHandle, ImageLoadError>) + 'static,
) {
    ljos_quick::ImageStore::of(engine).load(engine, url, reply);
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

/// Makes the scene deliver the pointer to items of type `T`, and of the
/// types derived from it, through `hooks`, in their own coordinates. A
/// press the item takes brings it the moves and the release (or cancel)
/// that follow, wherever the pointer goes; one it does not take goes on to
/// the items beneath. A later registration wins.
pub fn register_pointer_item<T: QmlType>(engine: &Engine, hooks: PointerItem) {
    ljos_quick::register_pointer_item(engine, MetaRef::Static(T::static_meta()), hooks);
}

/// Makes the scene tell a hovered item of type `T` (or of a type derived
/// from it) where the pointer is each time it moves over it with no press
/// the item holds. `T` must also be registered with
/// [`register_pointer_item`]: only such items are hovered.
pub fn register_hover_move<T: QmlType>(engine: &Engine, moved: HoverMove) {
    ljos_quick::register_hover_move(engine, MetaRef::Static(T::static_meta()), moved);
}

/// Makes items of type `T`, and of the types derived from it, what the
/// platform's input method talks to while one has the active focus: it
/// asks `client` for the text around the cursor and the cursor's
/// rectangle, and hands it what it composes and commits. One client
/// serves every item of the type. A later registration wins.
pub fn register_text_input_client<T: QmlType>(engine: &Engine, client: Rc<dyn TextInputClient>) {
    ljos_quick::keys::register_text_input_client(engine, MetaRef::Static(T::static_meta()), client);
}

/// A QML `font` as a `Text` lays it out: its family, pixel size (a point
/// size at the nearest whole pixel, as Qt sets it), weight and style, at
/// the system's text size when the font follows it. Tracked: a binding or
/// a `read` that asks follows the system's text size.
pub fn text_style(engine: &Engine, font: &Font) -> TextStyle {
    ljos_quick::shown_text_style(engine, font)
}

/// Runs `f` with the scene's text engine: the one every `Text` is laid
/// out with, holding the fonts the host chose, so what `f` measures is
/// what the scene draws. For measuring outside `ItemPaint::paint` (a
/// cell size when an item's size or font changes); inside `paint` the
/// engine is lent as its argument, and this returns `None`, as it does
/// when called from inside another `f`.
///
/// Read or write no QML property inside `f`: a binding that runs then
/// may measure text itself.
pub fn with_text_engine<R>(f: impl FnOnce(&mut TextEngine) -> R) -> Option<R> {
    let shared = ljos_quick::text_engine();
    let mut engine = shared.try_borrow_mut().ok()?;
    Some(f(&mut engine))
}

/// How many device pixels make one logical pixel where `item` is shown:
/// Qt's `item->window()->devicePixelRatio()`, QML's
/// `Screen.devicePixelRatio`. The display's of the window the item is in,
/// the primary display's for an item in no window, 1 when no display
/// says. Makes no `Screen` attached object. Tracked: a `read` that asks
/// repaints the item when its window moves to a display of another ratio
/// or the display's ratio changes.
pub fn device_pixel_ratio(engine: &Engine, item: ObjectId) -> f64 {
    ljos_controls::device_pixel_ratio(engine, item)
}

/// Paints `item` again in the next frame, with what its type's `read`
/// returns then: for what it shows that no property holds (a decoded
/// frame, cells a program wrote). Costs nothing for an item that has not
/// been painted yet, or is not a paint item.
pub fn repaint(engine: &Engine, item: ObjectId) {
    ljos_quick::request_repaint(engine, item);
}
