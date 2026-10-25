//! `PinchArea`: an item that scales and turns its `pinch.target` as two
//! fingers on it do, and tells each step in `pinchUpdated`.
//!
//! Only touch reaches it ([`crate::QuickScene::touch_event`]): a mouse
//! goes through it to what is under it, as in Qt. Measured on qml6: the
//! pinch starts once the distance between the fingers changed by the drag
//! threshold, turning alone never starts it, `scale` and `rotation` count
//! from where it started, and `angle` is counter-clockwise while
//! `rotation` is clockwise.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{Engine, ObjectId, Point, QmlObject, QmlType, QtObject, Value, qml_impl};
use ljos_render::kurbo;

use crate::enums::EnumTable;
use crate::handlers::{emit, flag, num, put};
use crate::item::Item;
use crate::scene::{DRAG_THRESHOLD, local_point};

/// `Pinch.Axis`: which coordinates of the target follow the fingers.
pub mod pinch_axis {
    pub const NO_DRAG: i32 = 0;
    pub const X_AXIS: i32 = 1;
    pub const Y_AXIS: i32 = 2;
    pub const X_AND_Y_AXIS: i32 = 3;
}

fn pinch_enums() -> EnumTable {
    vec![(
        "Axis",
        vec![
            ("NoDrag", i64::from(pinch_axis::NO_DRAG)),
            ("XAxis", i64::from(pinch_axis::X_AXIS)),
            ("YAxis", i64::from(pinch_axis::Y_AXIS)),
            ("XAndYAxis", i64::from(pinch_axis::X_AND_Y_AXIS)),
            ("XandYAxis", i64::from(pinch_axis::X_AND_Y_AXIS)),
        ],
    )]
}

/// The `pinch` group of a `PinchArea` (`Pinch` in Qt). The limits are
/// qml6's: a target is neither scaled nor turned until they are opened.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "Pinch", enums = pinch_enums)]
pub struct Pinch {
    #[qml(notify, reset)]
    pub target: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub active: bool,
    #[qml(notify)]
    pub minimum_scale: f64,
    #[qml(notify)]
    pub maximum_scale: f64,
    #[qml(notify)]
    pub minimum_rotation: f64,
    #[qml(notify)]
    pub maximum_rotation: f64,
    #[qml(notify, enumeration = "Axis")]
    pub drag_axis: i32,
    #[qml(notify)]
    pub minimum_x: f64,
    #[qml(notify)]
    pub maximum_x: f64,
    #[qml(notify)]
    pub minimum_y: f64,
    #[qml(notify)]
    pub maximum_y: f64,
}

impl Default for Pinch {
    fn default() -> Self {
        let far = f64::from(f32::MAX);
        Self {
            target: None,
            active: false,
            minimum_scale: 1.0,
            maximum_scale: 1.0,
            minimum_rotation: 0.0,
            maximum_rotation: 0.0,
            drag_axis: pinch_axis::NO_DRAG,
            minimum_x: -far,
            maximum_x: far,
            minimum_y: -far,
            maximum_y: far,
        }
    }
}

/// The `pinch` parameter of a PinchArea's signals (`PinchEvent` in Qt,
/// which no file names). Points are in the area's space.
#[derive(Debug, QmlObject)]
#[qml(name = "PinchEvent", extends = QtObject, uncreatable)]
pub struct PinchEvent {
    #[qml(base)]
    pub object: QtObject,
    #[qml(readonly)]
    pub center: Point,
    #[qml(readonly)]
    pub start_center: Point,
    #[qml(readonly)]
    pub previous_center: Point,
    #[qml(readonly)]
    pub scale: f64,
    #[qml(readonly)]
    pub previous_scale: f64,
    #[qml(readonly)]
    pub angle: f64,
    #[qml(readonly)]
    pub previous_angle: f64,
    #[qml(readonly)]
    pub rotation: f64,
    #[qml(readonly)]
    pub point1: Point,
    #[qml(readonly)]
    pub point2: Point,
    #[qml(readonly)]
    pub start_point1: Point,
    #[qml(readonly)]
    pub start_point2: Point,
    #[qml(readonly)]
    pub point_count: i32,
    pub accepted: bool,
}

impl Default for PinchEvent {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            center: Point::default(),
            start_center: Point::default(),
            previous_center: Point::default(),
            scale: 1.0,
            previous_scale: 1.0,
            angle: 0.0,
            previous_angle: 0.0,
            rotation: 0.0,
            point1: Point::default(),
            point2: Point::default(),
            start_point1: Point::default(),
            start_point2: Point::default(),
            point_count: 0,
            accepted: true,
        }
    }
}

/// `PinchArea` (QtQuick).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "PinchArea", extends = Item)]
pub struct PinchArea {
    #[qml(base)]
    pub item: Item,
    #[qml(group)]
    pub pinch: Pinch,
    #[qml(skip)]
    pub(crate) state: Rc<PinchAreaState>,
}

#[qml_impl]
impl PinchArea {
    #[qml(signal, name = "pinchStarted")]
    fn pinch_started(pinch: ObjectId);
    #[qml(signal, name = "pinchUpdated")]
    fn pinch_updated(pinch: ObjectId);
    #[qml(signal, name = "pinchFinished")]
    fn pinch_finished(pinch: ObjectId);
    /// Never sent: no platform here tells a smart zoom.
    #[qml(signal, name = "smartZoom")]
    fn smart_zoom(pinch: ObjectId);
}

/// The two fingers, in the scene: where they came down and where they are.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Fingers {
    pub press: [kurbo::Point; 2],
    pub now: [kurbo::Point; 2],
}

impl Fingers {
    fn distance(&self) -> f64 {
        (self.now[1] - self.now[0]).hypot()
    }

    fn center(&self) -> kurbo::Point {
        self.now[0].midpoint(self.now[1])
    }

    /// The line from the first finger to the second, counter-clockwise
    /// from the x axis, in degrees.
    fn angle(&self) -> f64 {
        let d = self.now[1] - self.now[0];
        -d.y.atan2(d.x).to_degrees()
    }
}

/// A pinch of one area, from its start.
#[derive(Clone, Copy, Debug)]
struct Run {
    distance: f64,
    center: kurbo::Point,
    rotation: f64,
    /// What the last event said.
    last_center: kurbo::Point,
    last_scale: f64,
    last_angle: f64,
    /// The target's scale, rotation and position when it started.
    target: Option<(ObjectId, f64, f64, kurbo::Point)>,
}

#[derive(Debug, Default)]
pub(crate) struct PinchAreaState {
    run: Cell<Option<Run>>,
    /// The fingers came down and the start was turned down.
    refused: Cell<bool>,
    last: RefCell<Option<Fingers>>,
}

fn state(engine: &Engine, area: ObjectId) -> Option<Rc<PinchAreaState>> {
    engine.with(area, |a: &PinchArea| a.state.clone()).ok()
}

fn index(name: &str) -> Option<ljos_engine::PropIndex> {
    PinchArea::static_meta().property_index(name)
}

fn pinch_num(engine: &Engine, area: ObjectId, name: &str) -> f64 {
    index(&format!("pinch.{name}")).map_or(0.0, |p| num(engine, area, p))
}

/// Whether `obj` is a PinchArea that takes touches.
pub(crate) fn takes_touch(engine: &Engine, obj: ObjectId) -> bool {
    crate::item::is_a::<PinchArea>(engine, obj)
        && flag(engine, obj, Item::ENABLED.index)
        && flag(engine, obj, Item::VISIBLE.index)
}

fn clamp(v: f64, min: f64, max: f64) -> f64 {
    if min > max { v } else { v.clamp(min, max) }
}

/// From `from` to `to`, the shorter way round, in degrees.
fn turned(from: f64, to: f64) -> f64 {
    let d = (to - from) % 360.0;
    if d > 180.0 {
        d - 360.0
    } else if d < -180.0 {
        d + 360.0
    } else {
        d
    }
}

fn local(engine: &Engine, area: ObjectId, p: kurbo::Point) -> Point {
    let l = local_point(engine, area, p);
    Point { x: l.x, y: l.y }
}

/// Emits `name` with `event`. Whether it was left accepted.
fn send(engine: &Engine, area: ObjectId, name: &str, event: PinchEvent) -> bool {
    let Ok(ev) = engine.create_native_incomplete(event, Some(area)) else {
        return true;
    };
    emit(engine, area, name, &[Value::Object(ev)]);
    let accepted = engine.read(ev, PinchEvent::ACCEPTED.index).to_boolean();
    engine.destroy_now(ev);
    accepted
}

fn event(engine: &Engine, area: ObjectId, f: &Fingers, run: &Run, count: i32) -> PinchEvent {
    PinchEvent {
        center: local(engine, area, f.center()),
        start_center: local(engine, area, run.center),
        previous_center: local(engine, area, run.last_center),
        scale: f.distance() / run.distance.max(f64::MIN_POSITIVE),
        previous_scale: run.last_scale,
        angle: f.angle(),
        previous_angle: run.last_angle,
        rotation: run.rotation,
        point1: local(engine, area, f.now[0]),
        point2: local(engine, area, f.now[1]),
        start_point1: local(engine, area, f.press[0]),
        start_point2: local(engine, area, f.press[1]),
        point_count: count,
        ..PinchEvent::default()
    }
}

fn set_active(engine: &Engine, area: ObjectId, on: bool) {
    if let Some(p) = index("pinch.active") {
        put(engine, area, p, Value::Bool(on));
    }
}

/// The two fingers on `area` moved (or the second came down).
pub(crate) fn moved(engine: &Engine, area: ObjectId, f: Fingers) {
    let Some(st) = state(engine, area) else {
        return;
    };
    *st.last.borrow_mut() = Some(f);
    let Some(mut run) = st.run.get() else {
        let start = (f.press[1] - f.press[0]).hypot();
        if st.refused.get() || (f.distance() - start).abs() < DRAG_THRESHOLD {
            return;
        }
        let target = index("pinch.target")
            .and_then(|p| engine.untrack(|| engine.read(area, p)).as_object())
            .filter(|t| engine.is_alive(*t))
            .map(|t| {
                (
                    t,
                    num(engine, t, Item::SCALE.index),
                    num(engine, t, Item::ROTATION.index),
                    kurbo::Point::new(num(engine, t, Item::X.index), num(engine, t, Item::Y.index)),
                )
            });
        let run = Run {
            distance: f.distance(),
            center: f.center(),
            rotation: 0.0,
            last_center: f.center(),
            last_scale: 1.0,
            last_angle: f.angle(),
            target,
        };
        if send(
            engine,
            area,
            "pinchStarted",
            event(engine, area, &f, &run, 2),
        ) {
            st.run.set(Some(run));
            set_active(engine, area, true);
        } else {
            st.refused.set(true);
        }
        return;
    };
    run.rotation += turned(f.angle(), run.last_angle);
    let scale = f.distance() / run.distance.max(f64::MIN_POSITIVE);
    if let Some((t, scale0, rotation0, pos0)) = run.target.filter(|t| engine.is_alive(t.0)) {
        let p = |n| pinch_num(engine, area, n);
        let write = |prop, v: f64| {
            if !engine
                .untrack(|| engine.read(t, prop))
                .change_equals(&Value::Real(v))
            {
                let _ = engine.write(t, prop, Value::Real(v), ljos_engine::WriteKind::Imperative);
            }
        };
        write(
            Item::SCALE.index,
            clamp(scale0 * scale, p("minimumScale"), p("maximumScale")),
        );
        write(
            Item::ROTATION.index,
            clamp(
                rotation0 + run.rotation,
                p("minimumRotation"),
                p("maximumRotation"),
            ),
        );
        let axis = p("dragAxis") as i32;
        let moved = f.center() - run.center;
        if axis & pinch_axis::X_AXIS != 0 {
            write(
                Item::X.index,
                clamp(pos0.x + moved.x, p("minimumX"), p("maximumX")),
            );
        }
        if axis & pinch_axis::Y_AXIS != 0 {
            write(
                Item::Y.index,
                clamp(pos0.y + moved.y, p("minimumY"), p("maximumY")),
            );
        }
    }
    send(
        engine,
        area,
        "pinchUpdated",
        event(engine, area, &f, &run, 2),
    );
    run.last_center = f.center();
    run.last_scale = scale;
    run.last_angle = f.angle();
    st.run.set(Some(run));
}

/// Fewer than two fingers are left on `area`.
/// The fingers came off; `one_left` is whether the other stayed down
/// in the event that lifted the first.
pub(crate) fn released(engine: &Engine, area: ObjectId, one_left: bool) {
    let Some(st) = state(engine, area) else {
        return;
    };
    st.refused.set(false);
    let last = st.last.borrow_mut().take();
    let Some(run) = st.run.take() else {
        return;
    };
    if let Some(f) = last {
        // As in Qt, a finger lifted while the other stays is told in an
        // update with that one, and the end repeats it, while the pinch is
        // active. Both lifted at once only end it.
        if one_left {
            let mut ev = event(engine, area, &f, &run, 1);
            ev.previous_scale = run.last_scale;
            ev.previous_angle = run.last_angle;
            ev.previous_center = local(engine, area, run.last_center);
            send(engine, area, "pinchUpdated", ev);
        }
        let mut ev = event(engine, area, &f, &run, 0);
        ev.previous_scale = ev.scale;
        ev.previous_angle = ev.angle;
        ev.previous_center = ev.center;
        send(engine, area, "pinchFinished", ev);
    }
    set_active(engine, area, false);
}
