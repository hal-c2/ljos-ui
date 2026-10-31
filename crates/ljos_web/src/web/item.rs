//! `ljos::item` on the web: hooks kept by type for the face to call.

use std::any::TypeId;
use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use js_sys::Function;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

use super::paint::{Ctx, replay};
use super::store::{
    self, ENGINE, Engine, ObjectId, Prop, QmlType, by_ref_face, paint_of, set_paint, tracking,
    type_of,
};
use super::value::QmlValue;

pub use super::event::{
    KeyEvent, KeyKind, Modifiers, MouseButtons, PointerEvent, PointerId, PointerKind,
    PointerSource, ScrollPhase, Time, WheelEvent,
};
pub use super::text::{
    ElideMode, LayoutParams, Line, LineRun, TextEngine, TextLayout, TextStyle, WrapMode,
};
pub use ljos_render::{
    Border, Brush, BrushGradient, Decorations, DisplayList, DisplayListBuilder, DrawOp, FillRule,
    GlyphRun, GradientGeometry, GradientStop, ImageHandle, LinearGradient, Paint, PathStyle,
    PositionedGlyph, Rgba, ShapeStyle, Spread, TextAlign, kurbo, peniko,
};

/// The Qt key numbers, which the runtime's events carry too.
pub mod key {
    pub const SPACE: i32 = 0x20;
    pub const ESCAPE: i32 = 0x0100_0000;
    pub const TAB: i32 = 0x0100_0001;
    pub const BACKSPACE: i32 = 0x0100_0003;
    pub const RETURN: i32 = 0x0100_0004;
    pub const ENTER: i32 = 0x0100_0005;
    pub const DELETE: i32 = 0x0100_0007;
    pub const HOME: i32 = 0x0100_0010;
    pub const END: i32 = 0x0100_0011;
    pub const LEFT: i32 = 0x0100_0012;
    pub const UP: i32 = 0x0100_0013;
    pub const RIGHT: i32 = 0x0100_0014;
    pub const DOWN: i32 = 0x0100_0015;
}

/// The web's `Item`: a native visual type's base. Its properties live on the
/// face, the runtime's own `Item`, so this holds nothing.
#[derive(Clone, Copy, Debug, Default)]
pub struct Item;

impl Item {
    pub const OBJECT_NAME: Prop<String> = Prop::slot("objectName", 0);
    pub const X: Prop<f64> = Prop::slot("x", 0);
    pub const Y: Prop<f64> = Prop::slot("y", 0);
    pub const Z: Prop<f64> = Prop::slot("z", 0);
    pub const WIDTH: Prop<f64> = Prop::slot("width", 0);
    pub const HEIGHT: Prop<f64> = Prop::slot("height", 0);
    pub const IMPLICIT_WIDTH: Prop<f64> = Prop::slot("implicitWidth", 0);
    pub const IMPLICIT_HEIGHT: Prop<f64> = Prop::slot("implicitHeight", 0);
    pub const OPACITY: Prop<f64> = Prop::slot("opacity", 0);
    pub const VISIBLE: Prop<bool> = Prop::slot("visible", 0);
    pub const ENABLED: Prop<bool> = Prop::slot("enabled", 0);
    pub const ROTATION: Prop<f64> = Prop::slot("rotation", 0);
    pub const SCALE: Prop<f64> = Prop::slot("scale", 0);
}

/// What an item shows, read from its properties.
pub trait ItemPaint: std::fmt::Debug {
    fn paint(&self, size: kurbo::Size, text: &mut TextEngine, out: &mut DisplayListBuilder);
}

/// Reads what `obj` shows. Tracked: whatever it reads repaints the item.
pub type ReadPaint = fn(&Engine, ObjectId) -> Rc<dyn ItemPaint>;

#[derive(Clone, Copy, Debug)]
pub struct PointerItem {
    pub press: fn(&Engine, ObjectId, &PointerEvent, kurbo::Point) -> bool,
    pub moved: fn(&Engine, ObjectId, &PointerEvent, kurbo::Point),
    pub release: fn(&Engine, ObjectId, &PointerEvent, kurbo::Point),
    pub cancel: fn(&Engine, ObjectId),
    pub hover: fn(&Engine, ObjectId, bool),
}

#[derive(Clone, Copy, Debug)]
pub struct WheelItem {
    pub wheel: fn(&Engine, ObjectId, &WheelEvent, kurbo::Point) -> bool,
}

#[derive(Clone, Copy, Debug)]
pub struct KeyItem {
    pub shortcut_override: fn(&Engine, ObjectId, &KeyEvent) -> bool,
    pub press: fn(&Engine, ObjectId, &KeyEvent) -> bool,
    pub release: fn(&Engine, ObjectId, &KeyEvent) -> bool,
}

#[derive(Clone, Copy, Default)]
struct Hooks {
    read: Option<ReadPaint>,
    pointer: Option<PointerItem>,
    wheel: Option<WheelItem>,
    key: Option<KeyItem>,
}

const PAINT: u32 = 1;
const POINTER: u32 = 2;
const WHEEL: u32 = 4;
const KEY: u32 = 8;

type Painter = fn(&Rc<dyn ItemPaint>, &Ctx, f64, f64);

thread_local! {
    static HOOKS: RefCell<HashMap<TypeId, Hooks>> = RefCell::default();
    // The way to the replay and the text layout goes through here, set only
    // by `register_paint_item`, so that an application with no paint hook has
    // neither of them in its wasm after LTO.
    static PAINTER: Cell<Option<Painter>> = const { Cell::new(None) };
    static IMAGE_LOADER: RefCell<Option<Function>> = const { RefCell::new(None) };
}

fn hooks(id: ObjectId) -> Hooks {
    type_of(id)
        .and_then(|ty| HOOKS.with_borrow(|h| h.get(&ty).copied()))
        .unwrap_or_default()
}

fn with_hooks<T: QmlType>(f: impl FnOnce(&mut Hooks)) {
    HOOKS.with_borrow_mut(|h| f(h.entry(TypeId::of::<T>()).or_default()));
}

pub(super) fn hooks_bits(id: ObjectId) -> u32 {
    let h = hooks(id);
    let bit = |on: bool, b: u32| if on { b } else { 0 };
    bit(h.read.is_some(), PAINT)
        | bit(h.pointer.is_some(), POINTER)
        | bit(h.wheel.is_some(), WHEEL)
        | bit(h.key.is_some(), KEY)
}

fn draw(paint: &Rc<dyn ItemPaint>, ctx: &Ctx, width: f64, height: f64) {
    let mut out = DisplayListBuilder::new();
    let mut text = TextEngine::new();
    paint.paint(kurbo::Size::new(width, height), &mut text, &mut out);
    replay(ctx, out.finish().ops());
}

pub fn register_paint_item<T: QmlType>(_engine: &Engine, read: ReadPaint) {
    PAINTER.with(|p| p.set(Some(draw)));
    with_hooks::<T>(|h| h.read = Some(read));
}

pub fn register_pointer_item<T: QmlType>(_engine: &Engine, hooks: PointerItem) {
    with_hooks::<T>(|h| h.pointer = Some(hooks));
}

pub fn register_wheel_item<T: QmlType>(_engine: &Engine, hooks: WheelItem) {
    with_hooks::<T>(|h| h.wheel = Some(hooks));
}

pub fn register_key_item<T: QmlType>(_engine: &Engine, hooks: KeyItem) {
    with_hooks::<T>(|h| h.key = Some(hooks));
}

/// As natively: a property, or its default if it cannot be read.
pub fn prop<T: QmlValue + Default>(engine: &Engine, obj: ObjectId, p: Prop<T>) -> T {
    engine.get(obj, p).unwrap_or_default()
}

/// The next paint is due whatever the hook read.
pub fn repaint(_engine: &Engine, item: ObjectId) {
    call_face(item, "$repaint");
}

/// Why an item got (or lost) active focus: Qt's `Qt.FocusReason`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum FocusReason {
    /// A pointer press.
    Mouse = 0,
    /// The Tab key.
    Tab = 1,
    /// Shift+Tab.
    Backtab = 2,
    /// The window became active or inactive.
    ActiveWindow = 3,
    /// A popup took or gave back the keyboard.
    Popup = 4,
    /// A shortcut or a label's mnemonic.
    Shortcut = 5,
    /// The menu bar.
    MenuBar = 6,
    /// Anything else: an assignment to `focus`, `forceActiveFocus()`.
    #[default]
    Other = 7,
}

pub fn force_active_focus(_engine: &Engine, item: ObjectId, reason: FocusReason) {
    if let Some((face, f)) = by_ref_face(item, "forceActiveFocus") {
        let _ = f.call1(&face, &JsValue::from(reason as i32));
    }
}

pub fn device_pixel_ratio(_engine: &Engine, _item: ObjectId) -> f64 {
    js_sys::Reflect::get(&js_sys::global(), &"devicePixelRatio".into())
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0)
}

fn call_face(item: ObjectId, method: &str) {
    if let Some((face, f)) = by_ref_face(item, method) {
        let _ = f.call0(&face);
    }
}

/// A picture failed to come; what the browser said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageLoadError(pub String);

impl std::fmt::Display for ImageLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ImageLoadError {}

/// A picture by its URL. The reply comes after the call has returned, as it
/// does natively, from a helper of the page's (`web/face.js`).
pub fn load_image(
    _engine: &Engine,
    url: &str,
    reply: impl FnOnce(&Engine, Result<ImageHandle, ImageLoadError>) + 'static,
) {
    let Some(loader) = IMAGE_LOADER.with_borrow(Clone::clone) else {
        store::console_warn("ljos: load_image before the runtime's face helper is loaded");
        return;
    };
    let done = wasm_bindgen::closure::Closure::once_into_js(
        move |width: u32, height: u32, data: Option<Vec<u8>>, error: Option<String>| {
            let _call = store::Call::enter();
            let result = match data {
                Some(data) => ImageHandle::from_straight(width, height, data)
                    .map_err(|e| ImageLoadError(e.to_string())),
                None => Err(ImageLoadError(error.unwrap_or_default())),
            };
            reply(&ENGINE, result);
        },
    );
    let _ = loader.call2(&JsValue::NULL, &JsValue::from_str(url), &done);
}

/// What `web/face.js` gives this crate once.
#[wasm_bindgen(js_name = ljosInstall)]
pub fn install(load_image: Function) {
    IMAGE_LOADER.with_borrow_mut(|l| *l = Some(load_image));
}

#[wasm_bindgen(js_name = ljosHooks)]
pub fn js_hooks(id: u32) -> u32 {
    hooks_bits(store::by_id(id))
}

/// Tracked by the face's effect: what this reads repaints it.
#[wasm_bindgen(js_name = ljosPaintRead)]
pub fn js_paint_read(id: u32) {
    let id = store::by_id(id);
    if let Some(read) = hooks(id).read {
        set_paint(id, tracking(|| read(&ENGINE, id)));
    }
}

#[wasm_bindgen(js_name = ljosPaintDraw)]
pub fn js_paint_draw(id: u32, ctx: Ctx, width: f64, height: f64) {
    let Some(painter) = PAINTER.with(Cell::get) else {
        return;
    };
    if let Some(paint) = paint_of(store::by_id(id)) {
        painter(&paint, &ctx, width, height);
    }
}

/// `kind`: 0 press, 1 move, 2 release, 3 cancel, 4 hover in, 5 hover out.
#[wasm_bindgen(js_name = ljosPointer)]
#[allow(clippy::too_many_arguments)]
pub fn js_pointer(
    id: u32,
    kind: u32,
    sx: f64,
    sy: f64,
    lx: f64,
    ly: f64,
    button: u32,
    buttons: u32,
    modifiers: u32,
    pointer: u32,
) -> bool {
    let _call = store::Call::enter();
    let id = store::by_id(id);
    let Some(hooks) = hooks(id).pointer else {
        return false;
    };
    let event = PointerEvent {
        kind: match kind {
            0 => PointerKind::Press,
            2 => PointerKind::Release,
            3 => PointerKind::Cancel,
            5 => PointerKind::Leave,
            _ => PointerKind::Move,
        },
        pos: kurbo::Point::new(sx, sy),
        button: (button != 0).then(|| MouseButtons::from_bits_truncate(button)),
        buttons: MouseButtons::from_bits_truncate(buttons),
        modifiers: Modifiers::from_bits_truncate(modifiers),
        id: PointerId(u64::from(pointer)),
        source: PointerSource::MOUSE,
        time: Time::ZERO,
    };
    let local = kurbo::Point::new(lx, ly);
    match kind {
        0 => return (hooks.press)(&ENGINE, id, &event, local),
        1 => (hooks.moved)(&ENGINE, id, &event, local),
        2 => (hooks.release)(&ENGINE, id, &event, local),
        3 => (hooks.cancel)(&ENGINE, id),
        4 => (hooks.hover)(&ENGINE, id, true),
        _ => (hooks.hover)(&ENGINE, id, false),
    }
    true
}

#[wasm_bindgen(js_name = ljosWheel)]
#[allow(clippy::too_many_arguments)]
pub fn js_wheel(
    id: u32,
    sx: f64,
    sy: f64,
    lx: f64,
    ly: f64,
    angle_x: f64,
    angle_y: f64,
    pixel_x: f64,
    pixel_y: f64,
    modifiers: u32,
) -> bool {
    let _call = store::Call::enter();
    let id = store::by_id(id);
    let Some(hooks) = hooks(id).wheel else {
        return false;
    };
    let event = WheelEvent {
        pos: kurbo::Point::new(sx, sy),
        angle_delta: kurbo::Vec2::new(angle_x, angle_y),
        pixel_delta: kurbo::Vec2::new(pixel_x, pixel_y),
        phase: ScrollPhase::NoPhase,
        modifiers: Modifiers::from_bits_truncate(modifiers),
        time: Time::ZERO,
    };
    (hooks.wheel)(&ENGINE, id, &event, kurbo::Point::new(lx, ly))
}

#[wasm_bindgen(js_name = ljosKey)]
pub fn js_key(id: u32, press: bool, key: i32, modifiers: u32, text: String, auto: bool) -> bool {
    let _call = store::Call::enter();
    let id = store::by_id(id);
    let Some(hooks) = hooks(id).key else {
        return false;
    };
    let event = KeyEvent {
        kind: if press {
            KeyKind::Press
        } else {
            KeyKind::Release
        },
        key,
        modifiers: Modifiers::from_bits_truncate(modifiers),
        text,
        auto_repeat: auto,
        count: 1,
        native_scan_code: 0,
    };
    if press {
        (hooks.press)(&ENGINE, id, &event)
    } else {
        (hooks.release)(&ENGINE, id, &event)
    }
}
