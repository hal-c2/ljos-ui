//! `PinchHandler`: scales, turns and moves its target as two fingers do.
//!
//! Ljós hears no single touch point yet (the shell sends the mouse), so a
//! pinch comes as a gesture the platform recognised: a touchpad's pinch
//! and turn, or a touch screen's, which the shell gives to
//! [`crate::QuickScene::pinch_event`] as a [`PinchGesture`].

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, EngineError, ObjectId, Point, QmlObject, Value, Vector2d, WriteKind, qml_impl,
};
use ljos_render::kurbo;

use super::{
    DragAxis, PointValues, PointerHandler, axis, clamp, emit, handler_item, handler_target, num,
    point_value, put, put_point, reaches, set_active, usable, vector_value,
};
use crate::input::{HandlerPoint, PointerEvent};
use crate::item::Item;
use crate::tree::resources;

/// Where a pinch is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinchPhase {
    Begin,
    Update,
    End,
    /// The platform took the gesture away.
    Cancel,
}

/// A pinch the platform recognised. The values are what changed since the
/// last event of the gesture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinchGesture {
    pub phase: PinchPhase,
    /// Between the fingers, in the scene.
    pub center: kurbo::Point,
    /// The factor the distance between the fingers changed by; 1 for none.
    pub scale: f64,
    /// How far the fingers turned, in degrees, clockwise.
    pub rotation: f64,
    /// How far the centre moved, in the scene.
    pub translation: kurbo::Vec2,
}

impl PinchGesture {
    pub fn new(phase: PinchPhase, center: kurbo::Point) -> Self {
        Self {
            phase,
            center,
            scale: 1.0,
            rotation: 0.0,
            translation: kurbo::Vec2::ZERO,
        }
    }
}

/// `PinchHandler` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "PinchHandler", extends = PointerHandler)]
pub struct PinchHandler {
    #[qml(base)]
    pub handler: PointerHandler,
    /// Stored: a platform pinch has two fingers.
    #[qml(notify)]
    pub minimum_point_count: i32,
    /// Stored: a platform pinch has two fingers.
    #[qml(notify)]
    pub maximum_point_count: i32,
    #[qml(group)]
    pub centroid: HandlerPoint,
    #[qml(group)]
    pub scale_axis: DragAxis,
    #[qml(group)]
    pub rotation_axis: DragAxis,
    #[qml(group)]
    pub x_axis: DragAxis,
    #[qml(group)]
    pub y_axis: DragAxis,
    /// Qt's older name for `scaleAxis.minimum`.
    #[qml(notify)]
    pub minimum_scale: f64,
    #[qml(notify)]
    pub maximum_scale: f64,
    #[qml(notify)]
    pub minimum_rotation: f64,
    #[qml(notify)]
    pub maximum_rotation: f64,
    /// Says so with `scaleChanged(delta)`.
    #[qml(readonly)]
    pub scale: f64,
    #[qml(notify, readonly)]
    pub active_scale: f64,
    #[qml(notify)]
    pub persistent_scale: f64,
    /// Says so with `rotationChanged(delta)`.
    #[qml(readonly)]
    pub rotation: f64,
    #[qml(notify, readonly)]
    pub active_rotation: f64,
    #[qml(notify)]
    pub persistent_rotation: f64,
    /// Says so with `translationChanged(delta)`.
    #[qml(readonly)]
    pub translation: Vector2d,
    #[qml(notify, readonly)]
    pub active_translation: Point,
    #[qml(notify)]
    pub persistent_translation: Point,
    #[qml(skip)]
    pub(crate) state: Rc<PinchState>,
}

impl Default for PinchHandler {
    fn default() -> Self {
        Self {
            handler: PointerHandler::default(),
            minimum_point_count: 2,
            maximum_point_count: 2,
            centroid: HandlerPoint::default(),
            scale_axis: DragAxis::default(),
            rotation_axis: DragAxis::default(),
            x_axis: DragAxis::default(),
            y_axis: DragAxis::default(),
            minimum_scale: -f64::MAX,
            maximum_scale: f64::MAX,
            minimum_rotation: -f64::MAX,
            maximum_rotation: f64::MAX,
            scale: 1.0,
            active_scale: 1.0,
            persistent_scale: 1.0,
            rotation: 0.0,
            active_rotation: 0.0,
            persistent_rotation: 0.0,
            translation: Vector2d::default(),
            active_translation: Point::default(),
            persistent_translation: Point::default(),
            state: Rc::default(),
        }
    }
}

#[qml_impl]
impl PinchHandler {
    #[qml(signal, name = "centroidChanged")]
    fn centroid_changed();
    #[qml(signal, name = "scaleChanged")]
    fn scale_changed(delta: f64);
    #[qml(signal, name = "rotationChanged")]
    fn rotation_changed(delta: f64);
    #[qml(signal, name = "translationChanged")]
    fn translation_changed(delta: Vector2d);

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        super::completed(cx)
    }
}

/// What a pinch remembers between its events.
#[derive(Debug, Default)]
pub(crate) struct PinchState {
    /// The target's `scale`, `rotation` and position when it began.
    start_scale: Cell<f64>,
    start_rotation: Cell<f64>,
    start_pos: Cell<kurbo::Point>,
    /// The persistent values when it began.
    base_scale: Cell<f64>,
    base_rotation: Cell<f64>,
    base_translation: Cell<kurbo::Vec2>,
    /// What the gesture did so far, before the limits.
    scale: Cell<f64>,
    rotation: Cell<f64>,
    translation: Cell<kurbo::Vec2>,
}

/// The handler a pinch goes to, while it lasts.
#[derive(Default)]
struct Current(Cell<Option<ObjectId>>);

fn state(engine: &Engine, h: ObjectId) -> Option<Rc<PinchState>> {
    engine.with(h, |p: &PinchHandler| p.state.clone()).ok()
}

/// The topmost usable PinchHandler of the items under `at`.
fn find(engine: &Engine, hits: &[ObjectId], at: kurbo::Point) -> Option<ObjectId> {
    hits.iter().find_map(|&item| {
        resources(engine, item).into_iter().find(|&h| {
            crate::item::is_a::<PinchHandler>(engine, h)
                && usable(engine, h)
                && handler_item(engine, h).is_some_and(|i| {
                    reaches(engine, i, at, num(engine, h, PointerHandler::MARGIN.index))
                })
        })
    })
}

fn centroid(engine: &Engine, h: ObjectId, g: &PinchGesture, press: kurbo::Point) {
    let ev = PointerEvent::moved(
        g.center.x,
        g.center.y,
        crate::input::MouseButtons::empty(),
        engine.now(),
    );
    put_point(
        engine,
        h,
        "centroid",
        &PointValues::of(engine, h, &ev, press),
    );
}

/// A pinch over `hits` (topmost first). Whether a handler took it.
pub(crate) fn gesture(engine: &Engine, g: &PinchGesture, hits: &[ObjectId]) -> bool {
    let current = engine.extension::<Current>();
    match g.phase {
        PinchPhase::Begin => {
            if let Some(h) = current.0.take().filter(|h| engine.is_alive(*h)) {
                finish(engine, h, false);
            }
            let Some(h) = find(engine, hits, g.center) else {
                return false;
            };
            current.0.set(Some(h));
            begin(engine, h, g);
            true
        }
        PinchPhase::Update => match current.0.get().filter(|h| engine.is_alive(*h)) {
            Some(h) => {
                update(engine, h, g);
                true
            }
            None => false,
        },
        PinchPhase::End | PinchPhase::Cancel => match current.0.take() {
            Some(h) if engine.is_alive(h) => {
                if g.phase == PinchPhase::End {
                    update(engine, h, g);
                }
                finish(engine, h, g.phase == PinchPhase::Cancel);
                true
            }
            _ => false,
        },
    }
}

fn begin(engine: &Engine, h: ObjectId, g: &PinchGesture) {
    let Some(st) = state(engine, h) else { return };
    let target = handler_target(engine, h);
    st.start_scale
        .set(target.map_or(1.0, |t| num(engine, t, Item::SCALE.index)));
    st.start_rotation
        .set(target.map_or(0.0, |t| num(engine, t, Item::ROTATION.index)));
    st.start_pos.set(target.map_or(kurbo::Point::ZERO, |t| {
        kurbo::Point::new(num(engine, t, Item::X.index), num(engine, t, Item::Y.index))
    }));
    st.base_scale
        .set(num(engine, h, PinchHandler::PERSISTENT_SCALE.index));
    st.base_rotation
        .set(num(engine, h, PinchHandler::PERSISTENT_ROTATION.index));
    st.base_translation.set(super::read_vector(
        engine,
        h,
        PinchHandler::PERSISTENT_TRANSLATION.index,
    ));
    st.scale.set(1.0);
    st.rotation.set(0.0);
    st.translation.set(kurbo::Vec2::ZERO);
    super::grabs(engine).press.set(g.center);
    centroid(engine, h, g, g.center);
    set_active(engine, h, true);
    update(engine, h, g);
}

fn update(engine: &Engine, h: ObjectId, g: &PinchGesture) {
    let Some(st) = state(engine, h) else { return };
    let press = super::grabs(engine).press.get();
    centroid(engine, h, g, press);
    let (s_on, s_min, s_max) = axis(engine, h, "scaleAxis");
    let (r_on, r_min, r_max) = axis(engine, h, "rotationAxis");
    let (x_on, x_min, x_max) = axis(engine, h, "xAxis");
    let (y_on, y_min, y_max) = axis(engine, h, "yAxis");
    let s_min = s_min.max(num(engine, h, PinchHandler::MINIMUM_SCALE.index));
    let s_max = s_max.min(num(engine, h, PinchHandler::MAXIMUM_SCALE.index));
    let r_min = r_min.max(num(engine, h, PinchHandler::MINIMUM_ROTATION.index));
    let r_max = r_max.min(num(engine, h, PinchHandler::MAXIMUM_ROTATION.index));
    if s_on && g.scale.is_finite() && g.scale > 0.0 {
        st.scale.set(st.scale.get() * g.scale);
    }
    if r_on && g.rotation.is_finite() {
        st.rotation.set(st.rotation.get() + g.rotation);
    }
    let d = kurbo::Vec2::new(
        if x_on { g.translation.x } else { 0.0 },
        if y_on { g.translation.y } else { 0.0 },
    );
    st.translation.set(st.translation.get() + d);

    let persistent_scale =
        clamp(st.base_scale.get() * st.scale.get(), s_min, s_max).max(f64::MIN_POSITIVE);
    let active_scale = persistent_scale / st.base_scale.get().max(f64::MIN_POSITIVE);
    let persistent_rotation = clamp(st.base_rotation.get() + st.rotation.get(), r_min, r_max);
    let active_rotation = persistent_rotation - st.base_rotation.get();
    let t = st.translation.get();
    let base_t = st.base_translation.get();
    let persistent_t = kurbo::Vec2::new(
        clamp(base_t.x + t.x, x_min, x_max),
        clamp(base_t.y + t.y, y_min, y_max),
    );

    let before_scale = num(engine, h, PinchHandler::SCALE.index);
    let before_rotation = num(engine, h, PinchHandler::ROTATION.index);
    let before_t = super::read_vector(engine, h, PinchHandler::TRANSLATION.index);
    engine.batch(|| {
        put(
            engine,
            h,
            PinchHandler::PERSISTENT_SCALE.index,
            Value::Real(persistent_scale),
        );
        put(
            engine,
            h,
            PinchHandler::ACTIVE_SCALE.index,
            Value::Real(active_scale),
        );
        put(
            engine,
            h,
            PinchHandler::PERSISTENT_ROTATION.index,
            Value::Real(persistent_rotation),
        );
        put(
            engine,
            h,
            PinchHandler::ACTIVE_ROTATION.index,
            Value::Real(active_rotation),
        );
        put(
            engine,
            h,
            PinchHandler::PERSISTENT_TRANSLATION.index,
            point_value(persistent_t.to_point()),
        );
        put(
            engine,
            h,
            PinchHandler::ACTIVE_TRANSLATION.index,
            point_value((persistent_t - base_t).to_point()),
        );
        for (name, v) in [
            ("scaleAxis", active_scale),
            ("rotationAxis", active_rotation),
            ("xAxis", persistent_t.x - base_t.x),
            ("yAxis", persistent_t.y - base_t.y),
        ] {
            if let Ok(p) = engine.property_index(h, &format!("{name}.activeValue")) {
                put(engine, h, p, Value::Real(v));
            }
        }
    });
    if put(
        engine,
        h,
        PinchHandler::SCALE.index,
        Value::Real(persistent_scale),
    ) {
        emit(
            engine,
            h,
            "scaleChanged",
            &[Value::Real(
                persistent_scale / before_scale.max(f64::MIN_POSITIVE),
            )],
        );
    }
    if put(
        engine,
        h,
        PinchHandler::ROTATION.index,
        Value::Real(persistent_rotation),
    ) {
        emit(
            engine,
            h,
            "rotationChanged",
            &[Value::Real(persistent_rotation - before_rotation)],
        );
    }
    if put(
        engine,
        h,
        PinchHandler::TRANSLATION.index,
        vector_value(persistent_t),
    ) {
        emit(
            engine,
            h,
            "translationChanged",
            &[vector_value(persistent_t - before_t)],
        );
    }
    if let Some(target) = handler_target(engine, h) {
        let start = st.start_pos.get();
        let moved = persistent_t - base_t;
        let writes = [
            (s_on, Item::SCALE.index, st.start_scale.get() * active_scale),
            (
                r_on,
                Item::ROTATION.index,
                st.start_rotation.get() + active_rotation,
            ),
            (x_on, Item::X.index, start.x + moved.x),
            (y_on, Item::Y.index, start.y + moved.y),
        ];
        for (on, p, v) in writes {
            if on
                && !engine
                    .untrack(|| engine.read(target, p))
                    .change_equals(&Value::Real(v))
            {
                let _ = engine.write(target, p, Value::Real(v), WriteKind::Imperative);
            }
        }
    }
}

fn finish(engine: &Engine, h: ObjectId, canceled: bool) {
    let at = super::grabs(engine).press.get();
    set_active(engine, h, false);
    put_point(engine, h, "centroid", &PointValues::empty());
    if canceled {
        let point = PointValues::at(engine, h, at, at).event_point(super::point_state::UPDATED);
        emit(engine, h, "canceled", &[point]);
    }
    for (p, v) in [
        (PinchHandler::ACTIVE_SCALE.index, Value::Real(1.0)),
        (PinchHandler::ACTIVE_ROTATION.index, Value::Real(0.0)),
        (
            PinchHandler::ACTIVE_TRANSLATION.index,
            point_value(kurbo::Point::ZERO),
        ),
    ] {
        put(engine, h, p, v);
    }
}
