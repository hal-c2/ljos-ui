//! `DragHandler`: moves its target as a pointer drags it.
//!
//! The order of what it tells was read on qml6 (`features/input/
//! drag-handler.feature`): the move that crosses the threshold moves the
//! target and makes the handler active, and only the moves after it change
//! the translations. At the release `activeTranslation`, `translation` and
//! the axes' `activeValue` go back to zero without saying so.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, EngineError, ObjectId, QmlObject, Value, Vector2d, WriteKind, qml_impl,
};
use ljos_render::kurbo;
use ljos_runtime::Time;

use super::{
    EnumTable, PointValues, PointerHandler, axis, clamp, emit, flag, handler_item, handler_target,
    local, member, num, put, put_point, read_vector, scene_point, set_active, vector_value,
};
use crate::input::{HandlerPoint, PointerEvent};
use crate::item::Item;

/// `DragHandler.SnapMode` values.
pub mod snap_mode {
    pub const NO_SNAP: i32 = 0;
    pub const SNAP_AUTO: i32 = 1;
    pub const SNAP_IF_PRESSED_OUTSIDE_TARGET: i32 = 2;
    pub const SNAP_ALWAYS: i32 = 3;
}

fn drag_handler_enums() -> EnumTable {
    use snap_mode as s;
    vec![(
        "SnapMode",
        vec![
            ("NoSnap", i64::from(s::NO_SNAP)),
            ("SnapAuto", i64::from(s::SNAP_AUTO)),
            (
                "SnapIfPressedOutsideTarget",
                i64::from(s::SNAP_IF_PRESSED_OUTSIDE_TARGET),
            ),
            ("SnapAlways", i64::from(s::SNAP_ALWAYS)),
        ],
    )]
}

/// The `xAxis` and `yAxis` groups of a `DragHandler` (`DragAxis` in Qt):
/// whether the handler drags along the axis, and how far.
#[derive(Debug, QmlObject)]
#[qml(name = "DragAxis", uncreatable)]
pub struct DragAxis {
    /// The least the target's coordinate (or, without a target,
    /// `persistentTranslation`) may become.
    #[qml(notify)]
    pub minimum: f64,
    #[qml(notify)]
    pub maximum: f64,
    #[qml(notify)]
    pub enabled: bool,
    /// How far the active drag went along the axis. Deviation: Qt's
    /// `activeValueChanged` carries the change; this one carries nothing.
    #[qml(notify, readonly)]
    pub active_value: f64,
}

impl Default for DragAxis {
    fn default() -> Self {
        Self {
            minimum: -f64::MAX,
            maximum: f64::MAX,
            enabled: true,
            active_value: 0.0,
        }
    }
}

/// What a drag remembers between moves. Not properties: nothing binds to
/// it.
#[derive(Debug, Default)]
pub(crate) struct DragState {
    /// Where the target was, in its parent, when the drag began.
    start: Cell<kurbo::Point>,
    /// `persistentTranslation` when the drag began.
    persistent: Cell<kurbo::Vec2>,
    /// Added to the drag so the target's centre comes under the pointer.
    snap: Cell<kurbo::Vec2>,
    /// The time and scene position of the last event, and the speed.
    last: Cell<Option<(Time, kurbo::Point)>>,
    velocity: Cell<kurbo::Vec2>,
}

/// How much of a move's speed the centroid's velocity takes, the rest
/// being the speed it had (as a flick's, `flickable::physics`).
const VELOCITY_GAIN: f64 = 0.7;

/// `DragHandler` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "DragHandler",
    extends = PointerHandler,
    enums = drag_handler_enums
)]
pub struct DragHandler {
    #[qml(base)]
    pub handler: PointerHandler,
    /// Stored: Ljós drags with one pointer.
    #[qml(notify)]
    pub minimum_point_count: i32,
    /// Stored: Ljós drags with one pointer.
    #[qml(notify)]
    pub maximum_point_count: i32,
    /// The pointer dragging, while one presses the handler.
    #[qml(group)]
    pub centroid: HandlerPoint,
    #[qml(group)]
    pub x_axis: DragAxis,
    #[qml(group)]
    pub y_axis: DragAxis,
    /// How far the active drag went. Says so with `translationChanged`.
    #[qml(readonly)]
    pub translation: Vector2d,
    /// The same as `translation` (Qt 6.2's name for it). Says so with
    /// `activeTranslationChanged`.
    #[qml(readonly)]
    pub active_translation: Vector2d,
    /// All the drags together, limited by the axes when there is no
    /// target.
    #[qml(notify)]
    pub persistent_translation: Vector2d,
    #[qml(notify, enumeration = "SnapMode")]
    pub snap_mode: i32,
    #[qml(skip)]
    pub(crate) state: Rc<DragState>,
}

impl Default for DragHandler {
    fn default() -> Self {
        Self {
            handler: PointerHandler::default(),
            minimum_point_count: 1,
            maximum_point_count: 1,
            centroid: HandlerPoint::default(),
            x_axis: DragAxis::default(),
            y_axis: DragAxis::default(),
            translation: Vector2d::default(),
            active_translation: Vector2d::default(),
            persistent_translation: Vector2d::default(),
            snap_mode: snap_mode::SNAP_AUTO,
            state: Rc::default(),
        }
    }
}

#[qml_impl]
impl DragHandler {
    #[qml(signal, name = "centroidChanged")]
    fn centroid_changed();
    /// `delta` is what the translation changed by.
    #[qml(signal, name = "translationChanged")]
    fn translation_changed(delta: Vector2d);
    #[qml(signal, name = "activeTranslationChanged")]
    fn active_translation_changed();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        super::completed(cx)
    }
}

fn state(engine: &Engine, h: ObjectId) -> Option<Rc<DragState>> {
    engine.with(h, |d: &DragHandler| d.state.clone()).ok()
}

/// How far the pointer is from the press, in the scene, with what a
/// disabled axis would move taken out.
fn dragged(engine: &Engine, h: ObjectId, ev: &PointerEvent, press: kurbo::Point) -> kurbo::Vec2 {
    let d = ev.pos - press;
    let x = if axis(engine, h, DragHandler::X_AXIS.index).0 {
        d.x
    } else {
        0.0
    };
    let y = if axis(engine, h, DragHandler::Y_AXIS.index).0 {
        d.y
    } else {
        0.0
    };
    kurbo::Vec2::new(x, y)
}

fn centroid(engine: &Engine, h: ObjectId, ev: &PointerEvent, press: kurbo::Point) {
    let velocity = state(engine, h).map_or(kurbo::Vec2::ZERO, |st| {
        if let Some((t0, p0)) = st.last.get() {
            let dt = ev.time.saturating_since(t0).as_secs_f64();
            if dt > 0.0 {
                let speed = (ev.pos - p0) / dt;
                let v = speed * VELOCITY_GAIN + st.velocity.get() * (1.0 - VELOCITY_GAIN);
                st.velocity.set(v);
                st.last.set(Some((ev.time, ev.pos)));
            }
        }
        st.velocity.get()
    });
    put_point(
        engine,
        h,
        "centroid",
        &PointValues {
            velocity,
            ..PointValues::of(engine, h, ev, press)
        },
    );
}

/// A press reached the handler: the pointer is at rest.
pub(super) fn pressed(engine: &Engine, h: ObjectId, ev: &PointerEvent) {
    if let Some(st) = state(engine, h) {
        st.last.set(Some((ev.time, ev.pos)));
        st.velocity.set(kurbo::Vec2::ZERO);
    }
    centroid(engine, h, ev, ev.pos);
}

/// The pointer moved while the handler is not yet active.
pub(super) fn passive_move(engine: &Engine, h: ObjectId, ev: &PointerEvent, press: kurbo::Point) {
    centroid(engine, h, ev, press);
}

/// Whether the pointer went far enough along an axis the handler drags.
pub(super) fn wants_grab(
    engine: &Engine,
    h: ObjectId,
    ev: &PointerEvent,
    press: kurbo::Point,
) -> bool {
    if ev.buttons.is_empty() {
        return false;
    }
    // A handler that names no threshold follows the system's, which a
    // host may change after the handler was made.
    let threshold = if crate::item::is_unset(engine, h, PointerHandler::DRAG_THRESHOLD.index) {
        crate::system_prefs::start_drag_distance(engine)
    } else {
        num(engine, h, PointerHandler::DRAG_THRESHOLD.index)
    };
    let d = dragged(engine, h, ev, press);
    d.x.abs() > threshold || d.y.abs() > threshold
}

fn position(engine: &Engine, item: ObjectId) -> kurbo::Point {
    kurbo::Point::new(
        num(engine, item, Item::X.index),
        num(engine, item, Item::Y.index),
    )
}

/// `d`, a scene vector starting at `from`, in the coordinates of the
/// parent of `target`.
fn in_parent(engine: &Engine, target: ObjectId, from: kurbo::Point, d: kurbo::Vec2) -> kurbo::Vec2 {
    match engine.untrack(|| engine.visual_parent(target)) {
        Some(p) if crate::item::is_item(engine, p) => {
            local(engine, p, from + d) - local(engine, p, from)
        }
        _ => d,
    }
}

/// Whether the target's centre goes under the pointer when the drag
/// begins.
fn snaps(engine: &Engine, h: ObjectId, target: ObjectId, press: kurbo::Point) -> bool {
    let mode = num(engine, h, DragHandler::SNAP_MODE.index) as i32;
    let outside = !super::reaches(engine, target, press, 0.0);
    match mode {
        snap_mode::SNAP_ALWAYS => true,
        snap_mode::SNAP_IF_PRESSED_OUTSIDE_TARGET => outside,
        snap_mode::SNAP_AUTO => {
            // Only a target inside the handler's item: dragging a
            // separate item by a handle should keep the item where it is
            // relative to the handle.
            outside
                && handler_item(engine, h)
                    .is_some_and(|item| item != target && is_inside(engine, target, item))
        }
        _ => false,
    }
}

fn is_inside(engine: &Engine, obj: ObjectId, ancestor: ObjectId) -> bool {
    let mut cur = engine.untrack(|| engine.visual_parent(obj));
    for _ in 0..4096 {
        match cur {
            Some(o) if o == ancestor => return true,
            Some(o) => cur = engine.untrack(|| engine.visual_parent(o)),
            None => return false,
        }
    }
    false
}

/// Puts the target where the drag `d` (scene) takes it.
fn place(engine: &Engine, h: ObjectId, st: &DragState, target: ObjectId, d: kurbo::Vec2) {
    let press = super::grabs(engine).press.get();
    let move_by = in_parent(engine, target, press, d) + st.snap.get();
    let start = st.start.get();
    let (x_on, x_min, x_max) = axis(engine, h, DragHandler::X_AXIS.index);
    let (y_on, y_min, y_max) = axis(engine, h, DragHandler::Y_AXIS.index);
    let write = |p: ljos_engine::PropIndex, v: f64| {
        // A NaN limit leaves the target where it is along that axis.
        if !v.is_nan()
            && !engine
                .untrack(|| engine.read(target, p))
                .change_equals(&Value::Real(v))
            && let Err(err) = engine.write(target, p, Value::Real(v), WriteKind::Imperative)
        {
            engine.log(
                ljos_engine::LogLevel::Warning,
                crate::scene::SCENE_CATEGORY,
                format!("DragHandler: cannot move the target: {err}"),
            );
        }
    };
    if x_on {
        write(Item::X.index, clamp(start.x + move_by.x, x_min, x_max));
    }
    if y_on {
        write(Item::Y.index, clamp(start.y + move_by.y, y_min, y_max));
    }
}

/// The handler took the pointer: the target follows from now on.
pub(super) fn activate(engine: &Engine, h: ObjectId, ev: &PointerEvent, press: kurbo::Point) {
    let Some(st) = state(engine, h) else { return };
    st.persistent.set(read_vector(
        engine,
        h,
        DragHandler::PERSISTENT_TRANSLATION.index,
    ));
    st.snap.set(kurbo::Vec2::ZERO);
    let target = handler_target(engine, h);
    if let Some(t) = target {
        st.start.set(position(engine, t));
        if snaps(engine, h, t, press) {
            let w = num(engine, t, Item::WIDTH.index);
            let ht = num(engine, t, Item::HEIGHT.index);
            // In the target's own units, which are its parent's unless
            // the target itself is scaled.
            st.snap
                .set(local(engine, t, press) - kurbo::Point::new(w / 2.0, ht / 2.0));
        }
    }
    set_active(engine, h, true);
    if let Some(t) = target {
        place(engine, h, &st, t, dragged(engine, h, ev, press));
    }
}

/// A move of the pointer the handler holds.
pub(super) fn active_move(engine: &Engine, h: ObjectId, ev: &PointerEvent, press: kurbo::Point) {
    let Some(st) = state(engine, h) else { return };
    centroid(engine, h, ev, press);
    let d = dragged(engine, h, ev, press);
    let (_, x_min, x_max) = axis(engine, h, DragHandler::X_AXIS.index);
    let (_, y_min, y_max) = axis(engine, h, DragHandler::Y_AXIS.index);
    let target = handler_target(engine, h);
    let base = st.persistent.get();
    let persistent = match target {
        // The target is what the axes limit; the persistent translation
        // goes as far as the target did.
        Some(t) => {
            let start = st.start.get();
            let moved_by = in_parent(engine, t, press, d) + st.snap.get();
            kurbo::Vec2::new(
                clamp(start.x + moved_by.x, x_min, x_max) - start.x,
                clamp(start.y + moved_by.y, y_min, y_max) - start.y,
            ) - st.snap.get()
                + base
        }
        None => kurbo::Vec2::new(
            clamp(base.x + d.x, x_min, x_max),
            clamp(base.y + d.y, y_min, y_max),
        ),
    };
    put(
        engine,
        h,
        DragHandler::PERSISTENT_TRANSLATION.index,
        vector_value(persistent),
    );
    let before = read_vector(engine, h, DragHandler::TRANSLATION.index);
    if put(
        engine,
        h,
        DragHandler::ACTIVE_TRANSLATION.index,
        vector_value(d),
    ) {
        emit(engine, h, "activeTranslationChanged", &[]);
    }
    if put(engine, h, DragHandler::TRANSLATION.index, vector_value(d)) {
        emit(engine, h, "translationChanged", &[vector_value(d - before)]);
    }
    for (group, v) in [
        (DragHandler::X_AXIS.index, d.x),
        (DragHandler::Y_AXIS.index, d.y),
    ] {
        put(
            engine,
            h,
            member(group, DragAxis::ACTIVE_VALUE.index),
            Value::Real(v),
        );
    }
    if let Some(t) = target {
        place(engine, h, &st, t, d);
    }
}

/// The drag ended. Returns where the pointer was, in the scene.
pub(super) fn deactivate(engine: &Engine, h: ObjectId) -> kurbo::Point {
    let at = scene_point(engine, h, "centroid");
    set_active(engine, h, false);
    passive_release(engine, h);
    // qml6 reads them as zero after the drag without telling.
    for p in [
        DragHandler::ACTIVE_TRANSLATION.index,
        DragHandler::TRANSLATION.index,
    ] {
        put(engine, h, p, vector_value(kurbo::Vec2::ZERO));
    }
    for group in [DragHandler::X_AXIS.index, DragHandler::Y_AXIS.index] {
        put(
            engine,
            h,
            member(group, DragAxis::ACTIVE_VALUE.index),
            Value::Real(0.0),
        );
    }
    at
}

/// The pointer let go, or the passive grab ended: the centroid empties.
pub(super) fn passive_release(engine: &Engine, h: ObjectId) {
    if flag(engine, h, PointerHandler::ACTIVE.index) {
        return;
    }
    put_point(engine, h, "centroid", &PointValues::empty());
}
