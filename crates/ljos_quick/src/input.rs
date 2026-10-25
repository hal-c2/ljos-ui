//! Pointer events and the QML types that receive them: `MouseArea`,
//! `TapHandler` and the `mouse` event object.
//!
//! The event types are Ljós's own, so this crate never sees egui; the
//! shell translates its input into [`PointerEvent`]s. Delivery lives in
//! [`crate::QuickScene::pointer_event`].

use ljos_engine::{
    Cx, Engine, EngineError, IntPoint, ObjectId, Point, PropIndex, QmlObject, QmlType, QmlValue,
    QtObject, Value, Vector2d, WriteKind, qml_impl,
};
use ljos_render::kurbo;
use ljos_runtime::Time;

use crate::enums::{gesture_policy, mouse_button};
use crate::handlers::PointValues;
use crate::item::{Item, scene_transform};

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

/// What a pointer event came from, as a handler's `acceptedDevices` and
/// `acceptedPointerTypes` see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerSource {
    /// A `PointerDevice.DeviceType`.
    pub device: i32,
    /// A `PointerDevice.PointerType`.
    pub pointer: i32,
}

impl PointerSource {
    pub const MOUSE: Self = Self {
        device: 0x1,
        pointer: 0x1,
    };
    /// A finger on a touch screen.
    pub const FINGER: Self = Self {
        device: 0x2,
        pointer: 0x2,
    };
}

impl Default for PointerSource {
    fn default() -> Self {
        Self::MOUSE
    }
}

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
    pub source: PointerSource,
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
            source: PointerSource::MOUSE,
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

    pub fn with_source(mut self, source: PointerSource) -> Self {
        self.source = source;
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
    /// `pressed` while the pointer is inside the area.
    #[qml(notify, readonly)]
    pub contains_press: bool,
    #[qml(notify, readonly, flags = "Qt.MouseButtons")]
    pub pressed_buttons: i32,
    #[qml(notify, readonly)]
    pub mouse_x: f64,
    #[qml(notify, readonly)]
    pub mouse_y: f64,
    #[qml(notify, flags = "Qt.MouseButtons")]
    pub accepted_buttons: i32,
    /// Stored; nothing in M0 steals a grab (no Flickable yet), so it has no
    /// effect beyond being readable.
    #[qml(notify)]
    pub prevent_stealing: bool,
    /// Moving an item by dragging the area (`crate::mouse_drag`).
    #[qml(group)]
    pub drag: crate::drag::Drag,
    /// The shape the cursor takes over the area. Kept as Qt keeps it:
    /// the arrow (0) until given, and `undefined` takes a given shape
    /// back to it. Nothing here draws a cursor; the shape is what the
    /// area holds, and disabling or hiding the area leaves it as it was.
    #[qml(notify, reset, enumeration = "Qt.CursorShape")]
    pub cursor_shape: i32,
    /// Milliseconds a press is held before it says `pressAndHold`. A
    /// negative value reads as the system's (`crate::press_and_hold`).
    #[qml(notify, reset)]
    pub press_and_hold_interval: i32,
    #[qml(skip)]
    pub(crate) hold: std::rc::Rc<crate::press_and_hold::Hold>,
}

impl Default for MouseArea {
    fn default() -> Self {
        Self {
            item: Item::default(),
            hover_enabled: false,
            contains_mouse: false,
            pressed: false,
            contains_press: false,
            pressed_buttons: 0,
            mouse_x: 0.0,
            mouse_y: 0.0,
            accepted_buttons: mouse_button::LEFT,
            prevent_stealing: false,
            drag: crate::drag::Drag::default(),
            cursor_shape: 0,
            press_and_hold_interval: crate::system_prefs::PRESS_AND_HOLD_INTERVAL,
            hold: Default::default(),
        }
    }
}

#[qml_impl]
impl MouseArea {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        crate::press_and_hold::completed(cx.engine, cx.this);
        Ok(())
    }

    /// The press was held for `pressAndHoldInterval`.
    #[qml(signal, name = "pressAndHold")]
    fn press_and_hold(mouse: Option<ObjectId>);
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
    /// The wheel turned over the area. Only an area with a handler for it
    /// takes the wheel ([`crate::wheel`]).
    #[qml(signal)]
    fn wheel(wheel: Option<ObjectId>);
}

/// The `wheel` parameter of `MouseArea.wheel` and `WheelHandler.wheel`
/// (`WheelEvent` in Qt). The position is in the space of the area, or of
/// the handler's item.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "WheelEvent", extends = QtObject, uncreatable)]
pub struct WheelEventObject {
    #[qml(base)]
    pub object: QtObject,
    #[qml(readonly)]
    pub x: f64,
    #[qml(readonly)]
    pub y: f64,
    /// In eighths of a degree. A device that only scrolls by the pixel
    /// tells none; Ljós then says what a wheel would have turned to
    /// scroll as far, so that a handler that reads only this still works.
    /// Whole numbers, as Qt's are: they print `QPoint(0, 120)`.
    #[qml(readonly)]
    pub angle_delta: IntPoint,
    #[qml(readonly)]
    pub pixel_delta: IntPoint,
    #[qml(readonly, flags = "Qt.MouseButtons")]
    pub buttons: i32,
    #[qml(readonly, flags = "Qt.KeyboardModifiers")]
    pub modifiers: i32,
    /// Always false: no host of Ljós tells that the system turns the
    /// deltas around.
    #[qml(readonly)]
    pub inverted: bool,
    /// A `Qt.ScrollPhase`.
    #[qml(readonly, enumeration = "Qt.ScrollPhase")]
    pub phase: i32,
    pub accepted: bool,
}

impl Default for WheelEventObject {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            x: 0.0,
            y: 0.0,
            angle_delta: IntPoint::default(),
            pixel_delta: IntPoint::default(),
            buttons: mouse_button::NO_BUTTON,
            modifiers: 0,
            inverted: false,
            phase: 0,
            accepted: true,
        }
    }
}

/// The `point` group of a pointer handler (`handlerPoint` in Qt): where
/// the pointer that presses it is. Empty, with `id` -1, while nothing
/// presses the handler.
#[derive(Debug, QmlObject)]
#[qml(name = "HandlerPoint", uncreatable)]
pub struct HandlerPoint {
    /// In the handler's item.
    #[qml(notify, readonly)]
    pub position: Point,
    #[qml(notify, readonly)]
    pub scene_position: Point,
    /// Where the pointer went down, in the handler's item.
    #[qml(notify, readonly)]
    pub press_position: Point,
    #[qml(notify, readonly)]
    pub scene_press_position: Point,
    #[qml(notify, readonly, flags = "Qt.MouseButtons")]
    pub pressed_buttons: i32,
    #[qml(notify, readonly)]
    pub id: i32,
    /// Pixels per second, smoothed over the last moves. Zero where the
    /// handler does not measure it.
    #[qml(notify, readonly)]
    pub velocity: Vector2d,
    /// What the pointer came from: a plain object with the `type`,
    /// `pointerType` and `name` of Qt's `PointerDevice`
    /// ([`crate::handlers::device_value`]). The mouse while nothing
    /// presses the handler.
    #[qml(notify, readonly)]
    pub device: Value,
    /// The modifiers held as the pointer last did something.
    #[qml(notify, readonly, flags = "Qt.KeyboardModifiers")]
    pub modifiers: i32,
}

impl Default for HandlerPoint {
    fn default() -> Self {
        Self {
            position: Point::default(),
            scene_position: Point::default(),
            press_position: Point::default(),
            scene_press_position: Point::default(),
            pressed_buttons: mouse_button::NO_BUTTON,
            id: -1,
            velocity: Vector2d::default(),
            device: crate::handlers::device_value(PointerSource::MOUSE),
            modifiers: 0,
        }
    }
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
    #[qml(notify, enumeration = "GesturePolicy")]
    pub gesture_policy: i32,
    #[qml(notify, flags = "Qt.MouseButtons")]
    pub accepted_buttons: i32,
    #[qml(notify, readonly)]
    pub parent: Option<ObjectId>,
    /// The pointer that presses the handler. Qt has more in it than the
    /// six values here (`velocity`, `pressure`, `sceneGrabPosition` …).
    #[qml(group)]
    pub point: HandlerPoint,
    /// The properties every pointer handler has (`crate::handlers`).
    /// `acceptedDevices`, `acceptedPointerTypes` and `acceptedModifiers`
    /// choose the presses it takes ([`TAP_ACCEPTED`]).
    #[qml(notify)]
    pub target: Option<ObjectId>,
    #[qml(notify)]
    pub margin: f64,
    #[qml(notify, reset)]
    pub drag_threshold: i32,
    #[qml(flags = "GrabPermissions")]
    pub grab_permissions: i32,
    #[qml(notify, flags = "DeviceTypes")]
    pub accepted_devices: i32,
    #[qml(notify, flags = "PointerTypes")]
    pub accepted_pointer_types: i32,
    #[qml(notify, flags = "Qt.KeyboardModifiers")]
    pub accepted_modifiers: i32,
    #[qml(notify, reset, enumeration = "Qt.CursorShape")]
    pub cursor_shape: i32,
    /// Seconds; 0 turns long presses off. Unset or negative, it is the
    /// system's press-and-hold interval (`crate::long_press`).
    #[qml(notify, reset)]
    pub long_press_threshold: f64,
    /// Seconds the press has been held; -1 while nothing is pressed. While
    /// it is held it is brought up to date once for each frame
    /// (`crate::long_press::tick`).
    #[qml(notify, readonly)]
    pub time_held: f64,
    #[qml(skip)]
    pub(crate) long_press: std::rc::Rc<crate::long_press::LongPress>,
}

/// Where a tap handler keeps what it accepts.
pub(crate) const TAP_ACCEPTED: crate::handlers::Accepted = crate::handlers::Accepted {
    devices: TapHandler::ACCEPTED_DEVICES.index,
    pointer_types: TapHandler::ACCEPTED_POINTER_TYPES.index,
    modifiers: TapHandler::ACCEPTED_MODIFIERS.index,
};

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
            point: HandlerPoint::default(),
            target: None,
            margin: 0.0,
            drag_threshold: crate::scene::DRAG_THRESHOLD as i32,
            grab_permissions: crate::handlers::grab_permission::DEFAULT,
            accepted_devices: 0x7FFF_FFFF,
            accepted_pointer_types: 0x7FFF,
            accepted_modifiers: -0x0200_0000,
            cursor_shape: 0,
            long_press_threshold: f64::from(crate::system_prefs::PRESS_AND_HOLD_INTERVAL) / 1000.0,
            time_held: -1.0,
            long_press: Default::default(),
        }
    }
}

#[qml_impl]
impl TapHandler {
    /// Something in `point` changed. The values of the group signal on
    /// their own as well, which Qt's do not.
    #[qml(signal, name = "pointChanged")]
    fn point_changed();

    /// `tapped(eventPoint, button)`: `eventPoint` is where the button came
    /// up ([`tap_event_point`]). The button has no name, as in Qt: a
    /// handler that does not declare it is not given it, and `button` is
    /// whatever the document calls so.
    #[qml(signal)]
    fn tapped(event_point: Value, _: i32);
    #[qml(signal, name = "singleTapped")]
    fn single_tapped(event_point: Value, _: i32);
    #[qml(signal, name = "doubleTapped")]
    fn double_tapped(event_point: Value, _: i32);
    /// `canceled(point)`: Qt names it `point`, not `eventPoint`.
    #[qml(signal)]
    fn canceled(point: Value);
    /// The press was held for `longPressThreshold`.
    #[qml(signal, name = "longPressed")]
    fn long_pressed();
    /// `grabChanged(transition, point)`, as every pointer handler has: the
    /// handler took the point at the press, passively with the
    /// `DragThreshold` policy and on its own with the others, or let go
    /// of it ([`tap_grab_point`]).
    #[qml(signal, name = "grabChanged")]
    fn grab_changed(transition: i32, point: Value);

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let owner = cx.engine.owner(cx.this);
        if cx.get(TapHandler::PARENT)?.is_none() {
            cx.engine.set_internal(cx.this, TapHandler::PARENT, owner)?;
        }
        if crate::item::is_unset(cx.engine, cx.this, TapHandler::TARGET.index) {
            let parent = cx.get(TapHandler::PARENT)?;
            cx.engine
                .set_internal(cx.this, TapHandler::TARGET, parent)?;
        }
        Ok(())
    }
}

/// Who may take the pointer from the handler (`PointerHandler`'s
/// `grabPermissions`).
pub(crate) fn tap_grab_permissions(engine: &Engine, h: ObjectId) -> i32 {
    engine
        .untrack(|| engine.read(h, TapHandler::GRAB_PERMISSIONS.index))
        .to_number() as i32
}

/// The item a TapHandler acts on: its `parent`, else its owner.
pub fn tap_handler_target(engine: &Engine, h: ObjectId) -> Option<ObjectId> {
    match engine.read(h, TapHandler::PARENT.index) {
        Value::Object(o) => Some(o),
        _ => engine.owner(h),
    }
    .filter(|o| crate::item::is_item(engine, *o))
}

/// The properties of a TapHandler's `point` group.
struct PointProps {
    position: PropIndex,
    scene_position: PropIndex,
    press_position: PropIndex,
    scene_press_position: PropIndex,
    pressed_buttons: PropIndex,
    id: PropIndex,
    device: PropIndex,
    modifiers: PropIndex,
}

fn point_props() -> Option<PointProps> {
    let meta = TapHandler::static_meta();
    let p = |name| meta.property_index(name);
    Some(PointProps {
        position: p("point.position")?,
        scene_position: p("point.scenePosition")?,
        press_position: p("point.pressPosition")?,
        scene_press_position: p("point.scenePressPosition")?,
        pressed_buttons: p("point.pressedButtons")?,
        id: p("point.id")?,
        device: p("point.device")?,
        modifiers: p("point.modifiers")?,
    })
}

fn point_value(p: kurbo::Point) -> Value {
    Point { x: p.x, y: p.y }.to_value()
}

/// Where a scene position is in the handler's item.
fn in_tap_item(engine: &Engine, h: ObjectId, scene: kurbo::Point) -> kurbo::Point {
    match tap_handler_target(engine, h) {
        Some(item) => engine.untrack(|| scene_transform(engine, item)).inverse() * scene,
        None => scene,
    }
}

/// Writes values of the `point` group together, so that whatever reads
/// two of them never sees half a point. Whether one changed.
fn put_point(engine: &Engine, h: ObjectId, values: &[(PropIndex, Value)]) -> bool {
    engine.batch(|| {
        let mut changed = false;
        for (p, v) in values {
            if engine.untrack(|| engine.read(h, *p)).change_equals(v) {
                continue;
            }
            changed |= engine
                .write_internal(h, *p, v.clone(), WriteKind::Imperative)
                .is_ok();
        }
        changed
    })
}

/// A press reached the handler: its `point` is filled in. The caller says
/// so with [`tap_point_changed`], once the handler reads as pressed.
pub(crate) fn tap_point_pressed(engine: &Engine, h: ObjectId, ev: &PointerEvent) {
    let Some(p) = point_props() else { return };
    let local = point_value(in_tap_item(engine, h, ev.pos));
    let scene = point_value(ev.pos);
    put_point(
        engine,
        h,
        &[
            (p.position, local.clone()),
            (p.scene_position, scene.clone()),
            (p.press_position, local),
            (p.scene_press_position, scene),
            (p.pressed_buttons, Value::Int(ev.buttons.bits() as i32)),
            (p.id, Value::Int(ev.id.0 as i32)),
            (p.device, crate::handlers::device_value(ev.source)),
            (p.modifiers, Value::Int(ev.modifiers.bits() as i32)),
        ],
    );
}

/// The pointer that presses the handler moved, or let go: `point` follows
/// it and says so, except at the release, where Qt keeps the change for
/// the moment the point is emptied.
pub(crate) fn tap_point_moved(engine: &Engine, h: ObjectId, ev: &PointerEvent) {
    let Some(p) = point_props() else { return };
    let changed = put_point(
        engine,
        h,
        &[
            (p.position, point_value(in_tap_item(engine, h, ev.pos))),
            (p.scene_position, point_value(ev.pos)),
            (p.pressed_buttons, Value::Int(ev.buttons.bits() as i32)),
            (p.modifiers, Value::Int(ev.modifiers.bits() as i32)),
        ],
    );
    if changed && ev.kind != PointerKind::Release {
        tap_point_changed(engine, h);
    }
}

/// The `eventPoint` a tap handler's signal is given: the pointer at `at`
/// in the scene, pressed where its `point` says.
pub(crate) fn tap_event_point(engine: &Engine, h: ObjectId, at: kurbo::Point, state: i32) -> Value {
    event_point_at(engine, h, in_tap_item(engine, h, at), at, state)
}

/// The `eventPoint` of a tap handler's `grabChanged`. A point let go of
/// (`grabbed` false) has its place in the scene for its position: where
/// in the item qml6 says it is depends on how the event came
/// (`features/input/tap-handler-grabs.feature`).
pub(crate) fn tap_grab_point(
    engine: &Engine,
    h: ObjectId,
    at: kurbo::Point,
    state: i32,
    grabbed: bool,
) -> Value {
    let position = if grabbed {
        in_tap_item(engine, h, at)
    } else {
        at
    };
    event_point_at(engine, h, position, at, state)
}

fn event_point_at(
    engine: &Engine,
    h: ObjectId,
    position: kurbo::Point,
    at: kurbo::Point,
    state: i32,
) -> Value {
    let Some(p) = point_props() else {
        return Value::Undefined;
    };
    let read = |i| match engine.untrack(|| engine.read(h, i)) {
        Value::Point(p) => kurbo::Point::new(p.x, p.y),
        _ => kurbo::Point::ZERO,
    };
    let id = (engine.untrack(|| engine.read(h, p.id)).to_number() as i32).max(0);
    let source = crate::handlers::source_of(&engine.untrack(|| engine.read(h, p.device)));
    PointValues {
        position,
        scene_position: at,
        press_position: read(p.press_position),
        scene_press_position: read(p.scene_press_position),
        pressed_buttons: mouse_button::NO_BUTTON,
        id,
        velocity: kurbo::Vec2::ZERO,
        source,
        modifiers: 0,
    }
    .event_point(state)
}

/// Empties `point`. Whether it held anything.
pub(crate) fn tap_point_clear(engine: &Engine, h: ObjectId) -> bool {
    let Some(p) = point_props() else { return false };
    let none = point_value(kurbo::Point::ZERO);
    put_point(
        engine,
        h,
        &[
            (p.position, none.clone()),
            (p.scene_position, none.clone()),
            (p.press_position, none.clone()),
            (p.scene_press_position, none),
            (p.pressed_buttons, Value::Int(mouse_button::NO_BUTTON)),
            (p.id, Value::Int(-1)),
            (
                p.device,
                crate::handlers::device_value(PointerSource::MOUSE),
            ),
            (p.modifiers, Value::Int(0)),
        ],
    )
}

pub(crate) fn tap_point_changed(engine: &Engine, h: ObjectId) {
    let _ = engine.emit_by_name(h, "pointChanged", &[]);
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
