//! Pointer events and the QML types that receive them: `MouseArea`,
//! `TapHandler` and the `mouse` event object.
//!
//! The event types are Ljós's own, so this crate never sees egui; the
//! shell translates its input into [`PointerEvent`]s. Delivery lives in
//! [`crate::QuickScene::pointer_event`].

use ljos_engine::{Cx, EngineError, ObjectId, Point, QmlObject, QtObject, Value, qml_impl};
use ljos_render::kurbo;
use ljos_runtime::Time;

use crate::enums::{gesture_policy, mouse_button};
use crate::item::Item;

bitflags::bitflags! {
    /// Mouse buttons, with Qt's `Qt.MouseButton` values.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
    pub struct MouseButtons: u32 {
        const LEFT = 0x1;
        const RIGHT = 0x2;
        const MIDDLE = 0x4;
        const BACK = 0x8;
        const FORWARD = 0x10;
    }
}

bitflags::bitflags! {
    /// Keyboard modifiers, with Qt's `Qt.KeyboardModifier` values.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
    pub struct Modifiers: u32 {
        const SHIFT = 0x0200_0000;
        const CONTROL = 0x0400_0000;
        const ALT = 0x0800_0000;
        const META = 0x1000_0000;
        /// The key is on the keypad.
        const KEYPAD = 0x2000_0000;
    }
}

/// What happened to a pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Press,
    Release,
    Move,
    /// The platform took the pointer away (a system gesture, focus loss).
    Cancel,
    /// The pointer left the window.
    Leave,
}

/// Identifies a pointer: the mouse, or one touch point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct PointerId(pub u64);

/// One pointer event in scene (logical pixel) coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerEvent {
    pub kind: PointerKind,
    pub pos: kurbo::Point,
    /// The button that changed (press and release only).
    pub button: Option<MouseButtons>,
    /// Buttons held after the event.
    pub buttons: MouseButtons,
    pub modifiers: Modifiers,
    pub id: PointerId,
    pub time: Time,
}

impl PointerEvent {
    fn base(kind: PointerKind, x: f64, y: f64, time: Time) -> Self {
        Self {
            kind,
            pos: kurbo::Point::new(x, y),
            button: None,
            buttons: MouseButtons::empty(),
            modifiers: Modifiers::empty(),
            id: PointerId::default(),
            time,
        }
    }

    /// A left-button press.
    pub fn press(x: f64, y: f64, time: Time) -> Self {
        Self {
            button: Some(MouseButtons::LEFT),
            buttons: MouseButtons::LEFT,
            ..Self::base(PointerKind::Press, x, y, time)
        }
    }

    /// A left-button release.
    pub fn release(x: f64, y: f64, time: Time) -> Self {
        Self {
            button: Some(MouseButtons::LEFT),
            ..Self::base(PointerKind::Release, x, y, time)
        }
    }

    /// A move with `buttons` held.
    pub fn moved(x: f64, y: f64, buttons: MouseButtons, time: Time) -> Self {
        Self {
            buttons,
            ..Self::base(PointerKind::Move, x, y, time)
        }
    }

    pub fn cancel(time: Time) -> Self {
        Self::base(PointerKind::Cancel, 0.0, 0.0, time)
    }

    pub fn leave(time: Time) -> Self {
        Self::base(PointerKind::Leave, 0.0, 0.0, time)
    }

    pub fn with_button(mut self, b: MouseButtons) -> Self {
        self.button = Some(b);
        if self.kind == PointerKind::Press {
            self.buttons = b;
        }
        self
    }

    pub fn with_modifiers(mut self, m: Modifiers) -> Self {
        self.modifiers = m;
        self
    }

    pub fn with_id(mut self, id: PointerId) -> Self {
        self.id = id;
        self
    }
}

/// The `mouse` parameter of MouseArea's signals (`MouseEvent` in Qt).
/// Coordinates are in the MouseArea's space.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "MouseEvent", extends = QtObject, uncreatable)]
pub struct MouseEvent {
    #[qml(base)]
    pub object: QtObject,
    #[qml(readonly)]
    pub x: f64,
    #[qml(readonly)]
    pub y: f64,
    #[qml(readonly)]
    pub button: i32,
    #[qml(readonly)]
    pub buttons: i32,
    #[qml(readonly)]
    pub modifiers: i32,
    #[qml(readonly)]
    pub was_held: bool,
    #[qml(readonly)]
    pub is_click: bool,
    pub accepted: bool,
}

impl Default for MouseEvent {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            x: 0.0,
            y: 0.0,
            button: mouse_button::NO_BUTTON,
            buttons: mouse_button::NO_BUTTON,
            modifiers: 0,
            was_held: false,
            is_click: false,
            accepted: true,
        }
    }
}

/// `MouseArea` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "MouseArea", extends = Item)]
pub struct MouseArea {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub hover_enabled: bool,
    #[qml(notify, readonly)]
    pub contains_mouse: bool,
    #[qml(notify, readonly)]
    pub pressed: bool,
    #[qml(notify, readonly)]
    pub pressed_buttons: i32,
    #[qml(notify, readonly)]
    pub mouse_x: f64,
    #[qml(notify, readonly)]
    pub mouse_y: f64,
    #[qml(notify)]
    pub accepted_buttons: i32,
    /// Stored; nothing in M0 steals a grab (no Flickable yet), so it has no
    /// effect beyond being readable.
    #[qml(notify)]
    pub prevent_stealing: bool,
}

impl Default for MouseArea {
    fn default() -> Self {
        Self {
            item: Item::default(),
            hover_enabled: false,
            contains_mouse: false,
            pressed: false,
            pressed_buttons: 0,
            mouse_x: 0.0,
            mouse_y: 0.0,
            accepted_buttons: mouse_button::LEFT,
            prevent_stealing: false,
        }
    }
}

#[qml_impl]
impl MouseArea {
    #[qml(signal, name = "pressed")]
    fn press_signal(mouse: Option<ObjectId>);
    #[qml(signal)]
    fn released(mouse: Option<ObjectId>);
    #[qml(signal)]
    fn clicked(mouse: Option<ObjectId>);
    #[qml(signal, name = "doubleClicked")]
    fn double_clicked(mouse: Option<ObjectId>);
    #[qml(signal, name = "positionChanged")]
    fn position_changed(mouse: Option<ObjectId>);
    #[qml(signal)]
    fn entered();
    #[qml(signal)]
    fn exited();
    #[qml(signal)]
    fn canceled();
}

/// `TapHandler` (QtQuick). Its parent item is the object that owns it,
/// and it lives in that item's `resources`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "TapHandler",
    extends = QtObject,
    enums = crate::enums::tap_handler_enums
)]
pub struct TapHandler {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify, readonly)]
    pub pressed: bool,
    #[qml(notify, readonly)]
    pub tap_count: i32,
    /// [`crate::enums::gesture_policy`].
    #[qml(notify)]
    pub gesture_policy: i32,
    #[qml(notify)]
    pub accepted_buttons: i32,
    #[qml(notify, readonly)]
    pub parent: Option<ObjectId>,
}

impl Default for TapHandler {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            enabled: true,
            pressed: false,
            tap_count: 0,
            gesture_policy: gesture_policy::DRAG_THRESHOLD,
            accepted_buttons: mouse_button::LEFT,
            parent: None,
        }
    }
}

#[qml_impl]
impl TapHandler {
    /// `tapped(eventPoint, button)`. Deviation: `eventPoint` is the
    /// scene position as a `point`, not Qt's `eventPoint` value type.
    #[qml(signal)]
    fn tapped(event_point: Point, button: i32);
    #[qml(signal, name = "singleTapped")]
    fn single_tapped(event_point: Point, button: i32);
    #[qml(signal, name = "doubleTapped")]
    fn double_tapped(event_point: Point, button: i32);
    #[qml(signal)]
    fn canceled(event_point: Point);

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let owner = cx.engine.owner(cx.this);
        if cx.get(TapHandler::PARENT)?.is_none() {
            cx.engine.set_internal(cx.this, TapHandler::PARENT, owner)?;
        }
        Ok(())
    }
}

/// The item a TapHandler acts on: its `parent`, else its owner.
pub fn tap_handler_target(engine: &ljos_engine::Engine, h: ObjectId) -> Option<ObjectId> {
    match engine.read(h, TapHandler::PARENT.index) {
        Value::Object(o) => Some(o),
        _ => engine.owner(h),
    }
    .filter(|o| crate::item::is_item(engine, *o))
}

/// Whether a key went down or came up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyKind {
    Press,
    Release,
}

/// One key event, as the host saw it. Delivery lives in
/// [`crate::QuickScene::key_event`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub kind: KeyKind,
    /// A `Qt.Key` number ([`crate::keys::key`]). Shift+Tab is
    /// [`crate::keys::key::BACKTAB`], as on a desktop.
    pub key: i32,
    /// The modifiers held besides the key itself.
    pub modifiers: Modifiers,
    /// What the key types; empty for a key that types nothing.
    pub text: String,
    /// The press comes from holding the key down.
    pub auto_repeat: bool,
    /// How many presses this event stands for (a host may fold repeats).
    pub count: u32,
    /// The platform's number for the physical key; 0 when unknown.
    pub native_scan_code: u32,
}

impl KeyEvent {
    fn base(kind: KeyKind, key: i32) -> Self {
        Self {
            kind,
            key,
            modifiers: Modifiers::empty(),
            text: String::new(),
            auto_repeat: false,
            count: 1,
            native_scan_code: 0,
        }
    }

    pub fn press(key: i32) -> Self {
        Self::base(KeyKind::Press, key)
    }

    pub fn release(key: i32) -> Self {
        Self::base(KeyKind::Release, key)
    }

    pub fn with_modifiers(mut self, m: Modifiers) -> Self {
        self.modifiers = m;
        self
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    /// Marks the press as one that comes from holding the key.
    pub fn repeated(mut self) -> Self {
        self.auto_repeat = true;
        self
    }

    pub fn with_count(mut self, count: u32) -> Self {
        self.count = count;
        self
    }

    pub fn with_native_scan_code(mut self, code: u32) -> Self {
        self.native_scan_code = code;
        self
    }
}

/// Where a scroll gesture is, for a device that says so (a touchpad, a
/// finger on a screen). A wheel that turns in notches has no phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ScrollPhase {
    /// The device does not say.
    #[default]
    None,
    /// The fingers came down.
    Begin,
    /// The fingers moved.
    Update,
    /// The fingers left.
    End,
    /// The platform keeps the content moving after the fingers left.
    Momentum,
}

/// One scroll event in scene (logical pixel) coordinates: a wheel turned,
/// or two fingers moved on a touchpad. Delivery lives in
/// [`crate::QuickScene::wheel_event`].
///
/// Both deltas have Qt's signs (`WheelEvent.angleDelta` and `pixelDelta`):
/// positive `y` is the wheel turned away from the user, which shows what
/// is further up; positive `x` shows what is further left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelEvent {
    pub pos: kurbo::Point,
    /// How far the wheel turned, in eighths of a degree: one notch of a
    /// common mouse wheel is [`WheelEvent::NOTCH`].
    pub angle_delta: kurbo::Vec2,
    /// How far the device asks the content to move, in logical pixels.
    /// Zero for a device that only knows notches; when it is not zero it
    /// is what scrolls, whatever `angle_delta` says.
    pub pixel_delta: kurbo::Vec2,
    pub phase: ScrollPhase,
    pub modifiers: Modifiers,
    pub time: Time,
}

impl WheelEvent {
    /// One notch of a mouse wheel, in the unit of `angle_delta`.
    pub const NOTCH: f64 = 120.0;

    /// A wheel turned by `dx` and `dy` notches at a position.
    pub fn notches(x: f64, y: f64, dx: f64, dy: f64, time: Time) -> Self {
        Self {
            pos: kurbo::Point::new(x, y),
            angle_delta: kurbo::Vec2::new(dx * Self::NOTCH, dy * Self::NOTCH),
            pixel_delta: kurbo::Vec2::ZERO,
            phase: ScrollPhase::None,
            modifiers: Modifiers::empty(),
            time,
        }
    }

    /// A device that scrolls by pixels moved by `dx` and `dy`.
    pub fn pixels(x: f64, y: f64, dx: f64, dy: f64, time: Time) -> Self {
        Self {
            pos: kurbo::Point::new(x, y),
            angle_delta: kurbo::Vec2::ZERO,
            pixel_delta: kurbo::Vec2::new(dx, dy),
            phase: ScrollPhase::None,
            modifiers: Modifiers::empty(),
            time,
        }
    }

    pub fn with_phase(mut self, phase: ScrollPhase) -> Self {
        self.phase = phase;
        self
    }

    pub fn with_modifiers(mut self, m: Modifiers) -> Self {
        self.modifiers = m;
        self
    }
}
