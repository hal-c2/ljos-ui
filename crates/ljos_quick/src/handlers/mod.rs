//! The pointer handlers besides `TapHandler` and `WheelHandler`:
//! `DragHandler`, `HoverHandler`, `PointHandler` and `PinchHandler`, the
//! `PointerHandler` properties they share, and the grab model that lets a
//! handler take the pointer from a `MouseArea`, a `Flickable`, a
//! `TapHandler` or another handler.
//!
//! A handler lives in its item's `resources`. At a press each handler of
//! an item the press reaches takes a passive grab: it hears the moves and
//! the release whoever holds the pointer. A handler whose gesture begins
//! (a `DragHandler` past its threshold) asks for the exclusive grab; it
//! gets it when what holds the pointer approves, and the scene then gives
//! the pointer to it as it does to an item of another crate
//! ([`crate::pointer_item`]).
//!
//! Behaviour follows doc.qt.io/qt-6/qml-qtquick-pointerhandler.html and
//! the pages of each handler, with what qml6 shows where they say little;
//! it is pinned by `features/input/*-handler.feature`.

mod drag;
mod hover;
mod pinch;
mod point;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, EngineError, MetaRef, ObjectId, Point, PropIndex, QmlObject, QmlType, QmlValue,
    QtObject, Value, Vector2d, WriteKind, qml_impl,
};
use ljos_render::kurbo;

pub use drag::{DragAxis, DragHandler, snap_mode};
pub use hover::HoverHandler;
pub(crate) use pinch::gesture as pinch_gesture;
pub use pinch::{PinchGesture, PinchHandler, PinchPhase};
pub use point::PointHandler;

use crate::enums::mouse_button;
use crate::input::{MouseArea, PointerEvent, TapHandler};
use crate::item::{Item, is_a, is_item, is_unset, scene_transform};
use crate::scene::{DRAG_THRESHOLD, item_enabled};
use crate::tree::resources;

/// `PointerHandler.GrabPermission` values.
pub mod grab_permission {
    pub const TAKE_OVER_FORBIDDEN: i32 = 0x0;
    pub const CAN_TAKE_OVER_FROM_HANDLERS_OF_SAME_TYPE: i32 = 0x1;
    pub const CAN_TAKE_OVER_FROM_HANDLERS_OF_DIFFERENT_TYPE: i32 = 0x2;
    pub const CAN_TAKE_OVER_FROM_ITEMS: i32 = 0x4;
    pub const CAN_TAKE_OVER_FROM_ANYTHING: i32 = 0xF;
    pub const APPROVES_TAKE_OVER_BY_HANDLERS_OF_SAME_TYPE: i32 = 0x10;
    pub const APPROVES_TAKE_OVER_BY_HANDLERS_OF_DIFFERENT_TYPE: i32 = 0x20;
    pub const APPROVES_TAKE_OVER_BY_ITEMS: i32 = 0x40;
    pub const APPROVES_CANCELLATION: i32 = 0x80;
    pub const APPROVES_TAKE_OVER_BY_ANYTHING: i32 = 0xF0;
    /// What every handler starts with (read on qml6).
    pub const DEFAULT: i32 = CAN_TAKE_OVER_FROM_ITEMS
        | CAN_TAKE_OVER_FROM_HANDLERS_OF_DIFFERENT_TYPE
        | APPROVES_TAKE_OVER_BY_ANYTHING;
}

/// `PointerDevice.GrabTransition` values, as `grabChanged` tells them.
pub mod grab_transition {
    pub const GRAB_PASSIVE: i32 = 0x01;
    pub const UNGRAB_PASSIVE: i32 = 0x02;
    pub const CANCEL_GRAB_PASSIVE: i32 = 0x03;
    pub const GRAB_EXCLUSIVE: i32 = 0x10;
    pub const UNGRAB_EXCLUSIVE: i32 = 0x20;
    pub const CANCEL_GRAB_EXCLUSIVE: i32 = 0x30;
}

/// `Qt.KeyboardModifierMask`: the handler reacts whatever is held.
pub(crate) const ANY_MODIFIERS: i32 = -0x0200_0000;
/// `PointerDevice.AllDevices`.
pub(crate) const ALL_DEVICES: i32 = 0x7FFF_FFFF;
/// `PointerDevice.AllPointerTypes`.
pub(crate) const ALL_POINTER_TYPES: i32 = 0x7FFF;

pub(crate) type EnumTable = Vec<(&'static str, Vec<(&'static str, i64)>)>;

pub(crate) fn grab_permission_enums() -> EnumTable {
    use grab_permission as g;
    let keys = [
        ("TakeOverForbidden", g::TAKE_OVER_FORBIDDEN),
        (
            "CanTakeOverFromHandlersOfSameType",
            g::CAN_TAKE_OVER_FROM_HANDLERS_OF_SAME_TYPE,
        ),
        (
            "CanTakeOverFromHandlersOfDifferentType",
            g::CAN_TAKE_OVER_FROM_HANDLERS_OF_DIFFERENT_TYPE,
        ),
        ("CanTakeOverFromItems", g::CAN_TAKE_OVER_FROM_ITEMS),
        ("CanTakeOverFromAnything", g::CAN_TAKE_OVER_FROM_ANYTHING),
        (
            "ApprovesTakeOverByHandlersOfSameType",
            g::APPROVES_TAKE_OVER_BY_HANDLERS_OF_SAME_TYPE,
        ),
        (
            "ApprovesTakeOverByHandlersOfDifferentType",
            g::APPROVES_TAKE_OVER_BY_HANDLERS_OF_DIFFERENT_TYPE,
        ),
        ("ApprovesTakeOverByItems", g::APPROVES_TAKE_OVER_BY_ITEMS),
        ("ApprovesCancellation", g::APPROVES_CANCELLATION),
        (
            "ApprovesTakeOverByAnything",
            g::APPROVES_TAKE_OVER_BY_ANYTHING,
        ),
    ];
    vec![(
        "GrabPermissions",
        keys.iter().map(|(k, v)| (*k, i64::from(*v))).collect(),
    )]
}

/// What every pointer handler has: Qt's `PointerHandler` and
/// `PointerDeviceHandler` together, since no file names the second. Not
/// made from QML: the handlers derive from it.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "PointerHandler",
    extends = QtObject,
    uncreatable,
    enums = grab_permission_enums
)]
pub struct PointerHandler {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub enabled: bool,
    /// The handler holds the pointer for its gesture.
    #[qml(notify, readonly)]
    pub active: bool,
    /// What the gesture acts on; the parent unless one is given.
    #[qml(notify)]
    pub target: Option<ObjectId>,
    /// The item the handler is in.
    #[qml(notify)]
    pub parent: Option<ObjectId>,
    /// Who may take the pointer from the handler, and from whom it may
    /// take it. No change signal: Qt's is `grabPermissionChanged`.
    #[qml(flags = "GrabPermissions")]
    pub grab_permissions: i32,
    /// How far around the parent a press still reaches the handler.
    #[qml(notify)]
    pub margin: f64,
    /// How far a point moves, along one axis, before it is a drag.
    #[qml(notify, reset)]
    pub drag_threshold: i32,
    /// Stored: the shell shows the cursor of what is under the pointer.
    #[qml(notify, reset, enumeration = "Qt.CursorShape")]
    pub cursor_shape: i32,
    /// Stored: every pointer of Ljós is accepted.
    #[qml(notify)]
    pub accepted_devices: i32,
    /// Stored: every pointer of Ljós is accepted.
    #[qml(notify)]
    pub accepted_pointer_types: i32,
    #[qml(notify, flags = "Qt.MouseButtons")]
    pub accepted_buttons: i32,
    /// The modifiers that must be held, no more and no fewer;
    /// `Qt.KeyboardModifierMask` for any.
    #[qml(notify, flags = "Qt.KeyboardModifiers")]
    pub accepted_modifiers: i32,
}

impl Default for PointerHandler {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            enabled: true,
            active: false,
            target: None,
            parent: None,
            grab_permissions: grab_permission::DEFAULT,
            margin: 0.0,
            drag_threshold: DRAG_THRESHOLD as i32,
            cursor_shape: 0,
            accepted_devices: ALL_DEVICES,
            accepted_pointer_types: ALL_POINTER_TYPES,
            accepted_buttons: mouse_button::LEFT,
            accepted_modifiers: ANY_MODIFIERS,
        }
    }
}

#[qml_impl]
impl PointerHandler {
    /// `grabChanged(transition, point)`. Deviation: `point` is the scene
    /// position as a `point`, not Qt's `eventPoint`.
    #[qml(signal, name = "grabChanged")]
    fn grab_changed(transition: i32, point: Point);
    /// The gesture was taken away. `point` as in `grabChanged`.
    #[qml(signal)]
    fn canceled(point: Point);
}

/// Fills in `parent` and, when nobody gave one, `target` (a `target: null`
/// is a target of none). For each handler's `on_complete`.
pub(crate) fn completed(cx: &Cx) -> Result<(), EngineError> {
    let (e, h) = (cx.engine, cx.this);
    if cx.get(PointerHandler::PARENT)?.is_none() {
        e.set_internal(h, PointerHandler::PARENT, e.owner(h))?;
    }
    if is_unset(e, h, PointerHandler::TARGET.index) {
        let parent = cx.get(PointerHandler::PARENT)?;
        e.set_internal(h, PointerHandler::TARGET, parent)?;
    }
    noted_margin(e, h);
    e.watch(h, PointerHandler::MARGIN.index, move |e, _| {
        noted_margin(e, h)
    })?;
    Ok(())
}

/// What the handlers that hear the pointer are doing. Per engine; the
/// scene has one pointer.
#[derive(Default)]
struct Grabs {
    /// Handlers with a passive grab, in the order the press reached them.
    passive: RefCell<Vec<ObjectId>>,
    /// The handler that holds the pointer, and what held it before.
    exclusive: Cell<Option<(ObjectId, Option<ObjectId>)>>,
    /// The move that gave the handler the pointer: Qt tells nothing more
    /// of it, so the scene's delivery of that same move is dropped.
    activating: Cell<bool>,
    /// Where the press was, in the scene.
    press: Cell<kurbo::Point>,
    /// Handlers whose `margin` reaches beyond their item.
    margins: RefCell<Vec<ObjectId>>,
    /// Hovered `HoverHandler`s.
    hovered: RefCell<Vec<ObjectId>>,
}

fn grabs(engine: &Engine) -> Rc<Grabs> {
    engine.extension::<Grabs>()
}

fn noted_margin(engine: &Engine, h: ObjectId) {
    let g = grabs(engine);
    let mut m = g.margins.borrow_mut();
    m.retain(|o| *o != h && engine.is_alive(*o));
    if num(engine, h, PointerHandler::MARGIN.index) > 0.0 {
        m.push(h);
    }
}

/// The kinds of handler this module delivers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Drag,
    Point,
    Hover,
    Pinch,
}

fn kind(engine: &Engine, h: ObjectId) -> Option<Kind> {
    if is_a::<DragHandler>(engine, h) {
        Some(Kind::Drag)
    } else if is_a::<PointHandler>(engine, h) {
        Some(Kind::Point)
    } else if is_a::<HoverHandler>(engine, h) {
        Some(Kind::Hover)
    } else if is_a::<PinchHandler>(engine, h) {
        Some(Kind::Pinch)
    } else {
        None
    }
}

pub(crate) fn num(engine: &Engine, obj: ObjectId, p: PropIndex) -> f64 {
    engine.untrack(|| engine.read(obj, p)).to_number()
}

pub(crate) fn flag(engine: &Engine, obj: ObjectId, p: PropIndex) -> bool {
    engine.untrack(|| engine.read(obj, p)).to_boolean()
}

fn object(engine: &Engine, obj: ObjectId, p: PropIndex) -> Option<ObjectId> {
    engine
        .untrack(|| engine.read(obj, p))
        .as_object()
        .filter(|o| engine.is_alive(*o))
}

/// Sets a property the handler shows. Whether it changed.
pub(crate) fn put(engine: &Engine, obj: ObjectId, p: PropIndex, v: Value) -> bool {
    if engine.untrack(|| engine.read(obj, p)).change_equals(&v) {
        return false;
    }
    engine
        .write_internal(obj, p, v, WriteKind::Imperative)
        .is_ok()
}

pub(crate) fn emit(engine: &Engine, obj: ObjectId, name: &str, args: &[Value]) {
    if engine.is_alive(obj) {
        let _ = engine.emit_by_name(obj, name, args);
    }
}

pub(crate) fn point_value(p: kurbo::Point) -> Value {
    Point { x: p.x, y: p.y }.to_value()
}

pub(crate) fn vector_value(v: kurbo::Vec2) -> Value {
    Vector2d {
        x: v.x as f32,
        y: v.y as f32,
    }
    .to_value()
}

pub(crate) fn read_vector(engine: &Engine, obj: ObjectId, p: PropIndex) -> kurbo::Vec2 {
    match engine.untrack(|| engine.read(obj, p)) {
        Value::Vector2d(v) => kurbo::Vec2::new(f64::from(v.x), f64::from(v.y)),
        Value::Point(p) => kurbo::Vec2::new(p.x, p.y),
        _ => kurbo::Vec2::ZERO,
    }
}

/// The item a handler is in: its `parent`, else its owner.
pub(crate) fn handler_item(engine: &Engine, h: ObjectId) -> Option<ObjectId> {
    object(engine, h, PointerHandler::PARENT.index)
        .or_else(|| engine.owner(h))
        .filter(|o| is_item(engine, *o))
}

/// What the gesture acts on, if anything.
pub(crate) fn handler_target(engine: &Engine, h: ObjectId) -> Option<ObjectId> {
    object(engine, h, PointerHandler::TARGET.index).filter(|o| is_item(engine, *o))
}

/// `pos` in `item`'s coordinates.
pub(crate) fn local(engine: &Engine, item: ObjectId, pos: kurbo::Point) -> kurbo::Point {
    engine.untrack(|| scene_transform(engine, item)).inverse() * pos
}

fn visible(engine: &Engine, item: ObjectId) -> bool {
    let mut cur = Some(item);
    // Bounded so a corrupt parent cycle cannot hang.
    for _ in 0..4096 {
        let Some(o) = cur.filter(|o| is_item(engine, *o)) else {
            return true;
        };
        if !flag(engine, o, Item::VISIBLE.index) {
            return false;
        }
        cur = engine.untrack(|| engine.visual_parent(o));
    }
    true
}

/// Whether `item` contains the scene position `pos`, `margin` around it
/// included.
pub(crate) fn reaches(engine: &Engine, item: ObjectId, pos: kurbo::Point, margin: f64) -> bool {
    let p = local(engine, item, pos);
    let w = num(engine, item, Item::WIDTH.index);
    let h = num(engine, item, Item::HEIGHT.index);
    p.x >= -margin && p.y >= -margin && p.x < w + margin && p.y < h + margin
}

/// Whether a handler is on, in an item that is on and shown.
pub(crate) fn usable(engine: &Engine, h: ObjectId) -> bool {
    if !flag(engine, h, PointerHandler::ENABLED.index) {
        return false;
    }
    let Some(item) = handler_item(engine, h) else {
        return false;
    };
    engine.untrack(|| item_enabled(engine, item)) && visible(engine, item)
}

/// Whether the modifiers held are the ones the handler takes.
pub(crate) fn modifiers_fit(engine: &Engine, h: ObjectId, ev: &PointerEvent) -> bool {
    let modifiers = num(engine, h, PointerHandler::ACCEPTED_MODIFIERS.index) as i32;
    modifiers == ANY_MODIFIERS || modifiers == ev.modifiers.bits() as i32
}

/// Whether a handler reacts to a press with this button and modifiers.
fn accepts(engine: &Engine, h: ObjectId, ev: &PointerEvent) -> bool {
    if !usable(engine, h) || !modifiers_fit(engine, h, ev) {
        return false;
    }
    let button = ev.button.map_or(ev.buttons.bits(), |b| b.bits()) as i32;
    let buttons = num(engine, h, PointerHandler::ACCEPTED_BUTTONS.index) as i32;
    button & buttons != 0
}

// ------------------------------------------------------- handler points

/// The values of a `HandlerPoint` group (`point`, `centroid`).
pub(crate) struct PointValues {
    pub position: kurbo::Point,
    pub scene_position: kurbo::Point,
    pub press_position: kurbo::Point,
    pub scene_press_position: kurbo::Point,
    pub pressed_buttons: i32,
    pub id: i32,
}

impl PointValues {
    pub(crate) fn empty() -> Self {
        Self {
            position: kurbo::Point::ZERO,
            scene_position: kurbo::Point::ZERO,
            press_position: kurbo::Point::ZERO,
            scene_press_position: kurbo::Point::ZERO,
            pressed_buttons: mouse_button::NO_BUTTON,
            id: -1,
        }
    }

    /// A pointer at `ev`, pressed at `press` (both in the scene), as the
    /// handler's item sees it.
    pub(crate) fn of(engine: &Engine, h: ObjectId, ev: &PointerEvent, press: kurbo::Point) -> Self {
        let item = handler_item(engine, h);
        let at = |p| item.map_or(p, |i| local(engine, i, p));
        Self {
            position: at(ev.pos),
            scene_position: ev.pos,
            press_position: at(press),
            scene_press_position: press,
            pressed_buttons: ev.buttons.bits() as i32,
            id: ev.id.0 as i32,
        }
    }
}

/// Writes the group `group` (`"point"`, `"centroid"`) together, so that
/// whatever reads two of its values never sees half a point, and says so
/// with `<group>Changed` when anything changed.
pub(crate) fn put_point(engine: &Engine, h: ObjectId, group: &str, v: &PointValues) -> bool {
    let index = |name: &str| engine.property_index(h, &format!("{group}.{name}")).ok();
    let values = [
        ("position", point_value(v.position)),
        ("scenePosition", point_value(v.scene_position)),
        ("pressPosition", point_value(v.press_position)),
        ("scenePressPosition", point_value(v.scene_press_position)),
        ("pressedButtons", Value::Int(v.pressed_buttons)),
        ("id", Value::Int(v.id)),
    ];
    let changed = engine.batch(|| {
        let mut changed = false;
        for (name, value) in values {
            if let Some(p) = index(name) {
                changed |= put(engine, h, p, value);
            }
        }
        changed
    });
    if changed {
        emit(engine, h, &format!("{group}Changed"), &[]);
    }
    changed
}

/// The scene position a handler's point group shows.
fn scene_point(engine: &Engine, h: ObjectId, group: &str) -> kurbo::Point {
    let Ok(p) = engine.property_index(h, &format!("{group}.scenePosition")) else {
        return kurbo::Point::ZERO;
    };
    match engine.untrack(|| engine.read(h, p)) {
        Value::Point(p) => kurbo::Point::new(p.x, p.y),
        _ => kurbo::Point::ZERO,
    }
}

fn grab_changed(engine: &Engine, h: ObjectId, transition: i32, at: kurbo::Point) {
    emit(
        engine,
        h,
        "grabChanged",
        &[Value::Int(transition), point_value(at)],
    );
}

pub(crate) fn set_active(engine: &Engine, h: ObjectId, on: bool) {
    put(engine, h, PointerHandler::ACTIVE.index, Value::Bool(on));
}

// ------------------------------------------------------ the scene's side

/// A press began; what the last one left behind is forgotten. Handlers
/// whose `margin` reaches the press without their item being under it get
/// it first: the scene only knows the items under the pointer.
pub(crate) fn pressed(engine: &Engine, ev: &PointerEvent) {
    let g = grabs(engine);
    g.press.set(ev.pos);
    g.activating.set(false);
    if let Some((h, _)) = g.exclusive.take()
        && engine.is_alive(h)
    {
        deactivate(engine, h, grab_transition::CANCEL_GRAB_EXCLUSIVE);
    }
    let stale = std::mem::take(&mut *g.passive.borrow_mut());
    for h in stale {
        if engine.is_alive(h) {
            ungrab_passive(engine, h, grab_transition::CANCEL_GRAB_PASSIVE);
        }
    }
    let margins: Vec<ObjectId> = g.margins.borrow().clone();
    for h in margins {
        if !engine.is_alive(h) || !matches!(kind(engine, h), Some(Kind::Drag | Kind::Point)) {
            continue;
        }
        let Some(item) = handler_item(engine, h) else {
            continue;
        };
        let margin = num(engine, h, PointerHandler::MARGIN.index);
        if !reaches(engine, item, ev.pos, 0.0) && reaches(engine, item, ev.pos, margin) {
            grab_passive(engine, h, ev);
        }
    }
}

/// The press reached `item`, before the item itself: its handlers take a
/// passive grab. Whether any did. A passive grab does not take the press:
/// the items beneath get it too.
pub(crate) fn press(engine: &Engine, item: ObjectId, ev: &PointerEvent) -> bool {
    let mut any = false;
    for h in resources(engine, item) {
        if matches!(kind(engine, h), Some(Kind::Drag | Kind::Point))
            && !grabs(engine).passive.borrow().contains(&h)
        {
            any |= grab_passive(engine, h, ev);
        }
    }
    any
}

fn grab_passive(engine: &Engine, h: ObjectId, ev: &PointerEvent) -> bool {
    if !accepts(engine, h, ev) {
        return false;
    }
    grabs(engine).passive.borrow_mut().push(h);
    match kind(engine, h) {
        // qml6: a DragHandler's centroid changes before it says it grabbed,
        // a PointHandler grabs, then is active, then moves its point.
        Some(Kind::Drag) => {
            drag::pressed(engine, h, ev);
            grab_changed(engine, h, grab_transition::GRAB_PASSIVE, ev.pos);
        }
        Some(Kind::Point) => {
            grab_changed(engine, h, grab_transition::GRAB_PASSIVE, ev.pos);
            point::pressed(engine, h, ev);
        }
        _ => {}
    }
    true
}

/// Whether what holds the pointer lets `taker` have it.
fn approves(engine: &Engine, held: Option<ObjectId>, taker: ObjectId) -> bool {
    use grab_permission as g;
    let Some(o) = held.filter(|o| engine.is_alive(*o)) else {
        return true;
    };
    let may = num(engine, taker, PointerHandler::GRAB_PERMISSIONS.index) as i32;
    if is_a::<TapHandler>(engine, o) {
        let theirs = crate::input::tap_grab_permissions(engine, o);
        return may & g::CAN_TAKE_OVER_FROM_HANDLERS_OF_DIFFERENT_TYPE != 0
            && theirs & g::APPROVES_TAKE_OVER_BY_HANDLERS_OF_DIFFERENT_TYPE != 0;
    }
    if is_a::<PointerHandler>(engine, o) {
        let theirs = num(engine, o, PointerHandler::GRAB_PERMISSIONS.index) as i32;
        let same = kind(engine, o) == kind(engine, taker);
        let (need, give) = if same {
            (
                g::CAN_TAKE_OVER_FROM_HANDLERS_OF_SAME_TYPE,
                g::APPROVES_TAKE_OVER_BY_HANDLERS_OF_SAME_TYPE,
            )
        } else {
            (
                g::CAN_TAKE_OVER_FROM_HANDLERS_OF_DIFFERENT_TYPE,
                g::APPROVES_TAKE_OVER_BY_HANDLERS_OF_DIFFERENT_TYPE,
            )
        };
        return may & need != 0 && theirs & give != 0;
    }
    // An item: a MouseArea that prevents stealing, a dragging Flickable
    // and a control that keeps its press say no.
    if may & g::CAN_TAKE_OVER_FROM_ITEMS == 0
        || crate::flickable::drag::keeps_pointer_grab(engine, o)
        || crate::flickable::drag::is_dragging(engine, o)
    {
        return false;
    }
    !(is_a::<MouseArea>(engine, o) && flag(engine, o, MouseArea::PREVENT_STEALING.index))
}

/// A move, before the scene delivers it: the handlers with a passive grab
/// hear it, and the first whose gesture begins (topmost first) is
/// returned when `held`, what holds the pointer, lets it have it. The
/// scene then cancels the holder's press and calls [`activate`].
pub(crate) fn moved(
    engine: &Engine,
    ev: &PointerEvent,
    held: Option<ObjectId>,
) -> Option<ObjectId> {
    let g = grabs(engine);
    g.activating.set(false);
    let passive: Vec<ObjectId> = g.passive.borrow().clone();
    if passive.is_empty() {
        return None;
    }
    let exclusive = g.exclusive.get().map(|(h, _)| h);
    let press = g.press.get();
    let mut taker = None;
    for h in passive {
        if !engine.is_alive(h) || Some(h) == exclusive {
            continue;
        }
        match kind(engine, h) {
            Some(Kind::Drag) => {
                drag::passive_move(engine, h, ev, press);
                if taker.is_none()
                    && exclusive.is_none()
                    && drag::wants_grab(engine, h, ev, press)
                    && approves(engine, held, h)
                {
                    taker = Some(h);
                }
            }
            Some(Kind::Point) => point::moved(engine, h, ev, press),
            _ => {}
        }
    }
    taker
}

/// `h` takes the pointer from `held`, whose press the scene cancelled.
pub(crate) fn activate(engine: &Engine, h: ObjectId, ev: &PointerEvent, held: Option<ObjectId>) {
    let g = grabs(engine);
    g.exclusive.set(Some((h, held)));
    g.activating.set(true);
    grab_changed(engine, h, grab_transition::GRAB_EXCLUSIVE, ev.pos);
    if let Some(Kind::Drag) = kind(engine, h) {
        drag::activate(engine, h, ev, g.press.get());
    } else {
        set_active(engine, h, true);
    }
}

/// The pointer was let go. The handler that held it heard it already
/// through its pointer-item hooks; the passive grabs end here.
pub(crate) fn released(engine: &Engine, ev: &PointerEvent) {
    let g = grabs(engine);
    let passive = std::mem::take(&mut *g.passive.borrow_mut());
    for h in passive {
        if !engine.is_alive(h) {
            continue;
        }
        match kind(engine, h) {
            Some(Kind::Point) => point::released(engine, h, ev),
            Some(Kind::Drag) => drag::passive_release(engine, h),
            _ => {}
        }
        grab_changed(engine, h, grab_transition::UNGRAB_PASSIVE, ev.pos);
    }
}

/// The platform took the pointer away: the passive grabs end. The
/// handler that held the pointer heard it through its hooks.
pub(crate) fn canceled(engine: &Engine) {
    let g = grabs(engine);
    let passive = std::mem::take(&mut *g.passive.borrow_mut());
    for h in passive {
        if engine.is_alive(h) {
            ungrab_passive(engine, h, grab_transition::CANCEL_GRAB_PASSIVE);
        }
    }
}

fn ungrab_passive(engine: &Engine, h: ObjectId, transition: i32) {
    let at = match kind(engine, h) {
        Some(Kind::Point) => {
            let at = scene_point(engine, h, "point");
            point::canceled(engine, h);
            at
        }
        Some(Kind::Drag) => {
            let at = scene_point(engine, h, "centroid");
            drag::passive_release(engine, h);
            at
        }
        _ => kurbo::Point::ZERO,
    };
    grab_changed(engine, h, transition, at);
}

/// Ends the exclusive grab of `h` (released or taken away).
fn deactivate(engine: &Engine, h: ObjectId, transition: i32) {
    let g = grabs(engine);
    if g.exclusive.get().is_some_and(|(o, _)| o == h) {
        g.exclusive.set(None);
    }
    let at = match kind(engine, h) {
        Some(Kind::Drag) => drag::deactivate(engine, h),
        _ => {
            set_active(engine, h, false);
            kurbo::Point::ZERO
        }
    };
    grab_changed(engine, h, transition, at);
    if transition == grab_transition::CANCEL_GRAB_EXCLUSIVE {
        emit(engine, h, "canceled", &[point_value(at)]);
    }
}

/// Where a Flickable around the handler `h`, which holds the pointer,
/// starts looking when it would take the pointer for a drag of its own:
/// the handler's item. None when it may not: the handler took the press
/// from a Flickable (qml6 leaves it with the handler then), or the
/// handler does not let items take it.
pub(crate) fn yields_to_flickable(engine: &Engine, h: ObjectId) -> Option<ObjectId> {
    let (holder, from) = grabs(engine).exclusive.get()?;
    if holder != h {
        return None;
    }
    let perms = num(engine, h, PointerHandler::GRAB_PERMISSIONS.index) as i32;
    if perms & grab_permission::APPROVES_TAKE_OVER_BY_ITEMS == 0 {
        return None;
    }
    if from.is_some_and(|f| crate::flickable::is_flickable(engine, f)) {
        return None;
    }
    handler_item(engine, h)
}

// ------------------------------------------------ the exclusive grabber

/// How the scene delivers to a handler that holds the pointer.
fn hooks() -> crate::pointer_item::PointerItem {
    fn press(_: &Engine, _: ObjectId, _: &PointerEvent, _: kurbo::Point) -> bool {
        false
    }
    fn moved(e: &Engine, h: ObjectId, ev: &PointerEvent, _: kurbo::Point) {
        let g = grabs(e);
        if g.activating.replace(false) {
            return;
        }
        if let Some(Kind::Drag) = kind(e, h) {
            drag::active_move(e, h, ev, g.press.get());
        }
    }
    fn release(e: &Engine, h: ObjectId, _: &PointerEvent, _: kurbo::Point) {
        deactivate(e, h, grab_transition::UNGRAB_EXCLUSIVE);
    }
    fn cancel(e: &Engine, h: ObjectId) {
        deactivate(e, h, grab_transition::CANCEL_GRAB_EXCLUSIVE);
    }
    fn hover(_: &Engine, _: ObjectId, _: bool) {}
    crate::pointer_item::PointerItem {
        press,
        moved,
        release,
        cancel,
        hover,
    }
}

// ----------------------------------------------------------------- hover

/// The pointer is over `hits` (topmost first): the `HoverHandler`s of
/// those items follow.
pub(crate) fn hover(engine: &Engine, ev: &PointerEvent, hits: &[ObjectId]) {
    let g = grabs(engine);
    if g.hovered.borrow().is_empty() && !hover::any(engine) {
        return;
    }
    let mut now = std::mem::take(&mut *g.hovered.borrow_mut());
    hover::update(engine, ev, hits, &mut now);
    *g.hovered.borrow_mut() = now;
}

/// Registers the handlers' types with module `QtQuick`.
pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [
        PointerHandler, DragHandler, HoverHandler, PointHandler, PinchHandler,
    ]));
    for meta in [
        MetaRef::Static(DragHandler::static_meta()),
        MetaRef::Static(PointHandler::static_meta()),
    ] {
        crate::pointer_item::register_pointer_item(engine, meta, hooks());
    }
}
